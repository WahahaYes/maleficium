//! Export a project's outputs and sources to a destination outside it: the
//! compiled pdf as a copy, the sources as a zip. Nothing is written inside
//! the project.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    /// The file written.
    pub path: String,
    pub bytes: u64,
    /// Root-relative paths packed (zip only; empty for a pdf).
    pub files: Vec<String>,
}

/// An absolute destination whose directory exists, outside the root. The
/// destination itself may not exist yet, so its parent is canonicalized.
fn destination(root: &Path, dest: &str) -> Result<PathBuf, String> {
    let d = Path::new(dest);
    if !d.is_absolute() {
        return Err(format!("export destination must be absolute: {dest}"));
    }
    let name = d
        .file_name()
        .ok_or_else(|| format!("export destination has no file name: {dest}"))?;
    let parent = d
        .parent()
        .ok_or_else(|| format!("export destination has no directory: {dest}"))?
        .canonicalize()
        .map_err(|e| format!("export directory unreachable: {e}"))?;
    let out = parent.join(name);
    if out.starts_with(root) {
        return Err(String::from(
            "export destination is inside the project: choose a folder outside it",
        ));
    }
    Ok(out)
}

/// Copy main_rel's compiled pdf to `dest`.
pub fn export_pdf(root_id: &str, main_rel: &str, dest: &str) -> Result<Exported, String> {
    let root = super::fs::session_root(root_id)?;
    let o = super::outputs_of(root_id, main_rel)?;
    let pdf = o.outdir.join(&o.pdf_name);
    if !pdf.is_file() {
        return Err(format!("no compiled pdf for {main_rel}: compile first"));
    }
    let out = destination(&root, dest)?;
    let bytes = std::fs::copy(&pdf, &out).map_err(|e| format!("export failed: {e}"))?;
    Ok(Exported {
        path: out.to_string_lossy().to_string(),
        bytes,
        files: Vec::new(),
    })
}

/// The project's source files, root-relative and sorted: hidden names
/// (outputs, trash, dot files) and symlinks are left out.
fn source_files(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = root.join(&rel);
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if super::is_hidden_name(&name) {
                continue;
            }
            let Ok(ft) = entry.file_type() else { continue };
            let child = rel.join(&name);
            if ft.is_dir() {
                stack.push(child);
            } else if ft.is_file() {
                out.push(
                    child
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Zip the project's sources (uncompressed) to `dest`.
pub fn export_zip(root_id: &str, dest: &str) -> Result<Exported, String> {
    let root = super::fs::session_root(root_id)?;
    let out = destination(&root, dest)?;
    let files = source_files(&root)?;
    let mut entries = Vec::with_capacity(files.len());
    for rel in &files {
        let data = std::fs::read(root.join(rel)).map_err(|e| format!("cannot read {rel}: {e}"))?;
        entries.push((rel.clone(), data));
    }
    let zip = zip_stored(&entries)?;
    let mut f = std::fs::File::create(&out).map_err(|e| format!("export failed: {e}"))?;
    f.write_all(&zip)
        .map_err(|e| format!("export failed: {e}"))?;
    Ok(Exported {
        path: out.to_string_lossy().to_string(),
        bytes: zip.len() as u64,
        files,
    })
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A zip archive of `(name, bytes)` entries, stored without compression.
/// UTF-8 names (general-purpose bit 11); no zip64, so each entry and the
/// whole archive stay under 4 GiB.
pub fn zip_stored(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    const UTF8: u16 = 1 << 11;
    let too_big = || String::from("project too large to zip (4 GiB limit)");
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, data) in entries {
        let offset = u32::try_from(out.len()).map_err(|_| too_big())?;
        let size = u32::try_from(data.len()).map_err(|_| too_big())?;
        let crc = crc32(data);
        let name_len = u16::try_from(name.len()).map_err(|_| format!("path too long: {name}"))?;
        let common = |v: &mut Vec<u8>| {
            v.extend_from_slice(&20u16.to_le_bytes()); // version needed
            v.extend_from_slice(&UTF8.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes()); // stored
            v.extend_from_slice(&0u16.to_le_bytes()); // mod time
            v.extend_from_slice(&0x21u16.to_le_bytes()); // mod date 1980-01-01
            v.extend_from_slice(&crc.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&name_len.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes()); // extra length
        };
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        common(&mut out);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        common(&mut central);
        central.extend_from_slice(&0u16.to_le_bytes()); // comment length
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let count = u16::try_from(entries.len()).map_err(|_| String::from("too many files to zip"))?;
    let cd_offset = u32::try_from(out.len()).map_err(|_| too_big())?;
    let cd_size = u32::try_from(central.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn stored_zip_has_one_entry_per_file_and_a_central_directory() {
        let z = zip_stored(&[
            ("main.tex".into(), b"x".to_vec()),
            ("ch/a.tex".into(), b"yy".to_vec()),
        ])
        .unwrap();
        assert_eq!(&z[..4], &[0x50, 0x4b, 0x03, 0x04]);
        let eocd = z.len() - 22;
        assert_eq!(&z[eocd..eocd + 4], &[0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes([z[eocd + 10], z[eocd + 11]]), 2);
        let cd = u32::from_le_bytes(z[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
        assert_eq!(&z[cd..cd + 4], &[0x50, 0x4b, 0x01, 0x02]);
    }

    fn project(name: &str) -> (String, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("maleficium-export-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ch")).unwrap();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join("main.tex"), "x").unwrap();
        std::fs::write(dir.join("ch/a.tex"), "y").unwrap();
        std::fs::write(dir.join("main.aux"), "z").unwrap();
        std::fs::write(dir.join(".git/HEAD"), "ref").unwrap();
        let canon = dir.canonicalize().unwrap();
        let id = format!("exp-{name}");
        super::super::fs::grant_root(&id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    #[test]
    fn zip_packs_sources_only_and_refuses_a_destination_inside() {
        let (id, root) = project("zip");
        let out =
            std::env::temp_dir().join(format!("maleficium-export-{}.zip", std::process::id()));
        let e = export_zip(&id, &out.to_string_lossy()).unwrap();
        assert_eq!(
            e.files,
            vec!["ch/a.tex".to_string(), "main.tex".to_string()]
        );
        assert_eq!(e.bytes, std::fs::metadata(&out).unwrap().len());
        let inside = root.join("ch/out.zip");
        assert!(export_zip(&id, &inside.to_string_lossy())
            .unwrap_err()
            .contains("inside the project"));
        assert!(export_zip(&id, "relative.zip").is_err());
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn pdf_export_needs_a_compile_and_copies_bytes() {
        let (id, _root) = project("pdf");
        let out =
            std::env::temp_dir().join(format!("maleficium-export-{}.pdf", std::process::id()));
        assert!(export_pdf(&id, "main.tex", &out.to_string_lossy())
            .unwrap_err()
            .contains("compile first"));
        let o = super::super::outputs_of(&id, "main.tex").unwrap();
        std::fs::create_dir_all(&o.outdir).unwrap();
        std::fs::write(o.outdir.join(&o.pdf_name), b"%PDF-1.7 test").unwrap();
        let e = export_pdf(&id, "main.tex", &out.to_string_lossy()).unwrap();
        assert_eq!(e.bytes, 13);
        assert_eq!(std::fs::read(&out).unwrap(), b"%PDF-1.7 test");
        let _ = std::fs::remove_file(out);
        let _ = std::fs::remove_dir_all(&o.outdir);
    }
}
