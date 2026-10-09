//! The author's reader stylesheet: `\maleficiumreaderstyle{reader.css}`
//! names a CSS file in the project that the reader page applies after the
//! theme's tokens, in the app's article view and every export profile.
//!
//! The file is the author's own, but it lands inside the reader page, which
//! runs the widgets' host script and works offline, so it is read through
//! the project's confinement and checked before any of it is written:
//!
//! - nothing that loads or runs: `@import`, `expression(`, `javascript:`,
//!   `-moz-binding`, `behavior:` and `image-set(` are refused;
//! - nothing that leaves the `<style>` element: `</` is refused;
//! - `url()` takes a `data:` image or font, or a file in the project
//!   (relative to the stylesheet), which is inlined as a `data:` URI so the
//!   page needs no network and no extra files in any profile. Remote,
//!   absolute and escaping paths are refused.
//!
//! A refusal is a plain message for the export's warnings; the page then
//! keeps the theme alone.

use crate::Core;
use base64::Engine as _;
use std::path::{Component, Path, PathBuf};

/// Largest stylesheet read, in bytes.
pub const MAX_CSS_BYTES: usize = 256 * 1024;
/// Largest total of files its `url()`s inline, in bytes.
pub const MAX_INLINED_BYTES: usize = 8 * 1024 * 1024;

/// What the stylesheet may not contain, with the reason a refusal gives.
const REFUSED: &[(&str, &str)] = &[
    ("</", "`</` would end the page's style element"),
    (
        "@import",
        "@import loads another stylesheet; put its rules in this file",
    ),
    ("expression(", "expression() runs script"),
    ("javascript:", "javascript: URLs run script"),
    ("-moz-binding", "-moz-binding runs script"),
    ("behavior:", "behavior: runs script"),
    (
        "image-set(",
        "image-set() loads images outside url(); use url() with a project file",
    ),
];

/// The media type of a file `url()` may inline, by extension.
fn media_type(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// `rel` joined to `dir` and normalized, refusing an absolute path or one
/// that climbs above the project root (the confined read checks again).
fn join_rel(dir: &Path, rel: &str) -> Result<String, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel.starts_with('/') || rel.starts_with('\\') {
        return Err(format!(
            "`{rel}` is an absolute path; name a file in the project"
        ));
    }
    let mut out: Vec<String> = Vec::new();
    for c in dir.join(rel_path).components() {
        match c {
            Component::Normal(p) => out.push(p.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if out.pop().is_none() {
                    return Err(format!("`{rel}` is outside the project"));
                }
            }
            _ => return Err(format!("`{rel}` is not a project path")),
        }
    }
    if out.is_empty() {
        return Err(format!("`{rel}` names no file"));
    }
    Ok(out.join("/"))
}

/// The stylesheet `style` (relative to the main file's folder `main_dir`),
/// checked, with every project `url()` inlined: CSS ready to follow the
/// theme's. `Err` is the plain reason it was not applied.
pub fn load(cx: &Core, root_id: &str, main_dir: &Path, style: &str) -> Result<String, String> {
    let rel = join_rel(main_dir, style)?;
    if !rel.to_ascii_lowercase().ends_with(".css") {
        return Err(format!("reader style `{style}` is not a .css file"));
    }
    let bytes = crate::fs::read_bytes(cx, root_id, &rel)
        .map_err(|e| format!("reader style `{style}` cannot be read: {e}"))?;
    if bytes.len() > MAX_CSS_BYTES {
        return Err(format!(
            "reader style `{style}` is over {} KiB",
            MAX_CSS_BYTES / 1024
        ));
    }
    let css = String::from_utf8(bytes)
        .map_err(|_| format!("reader style `{style}` is not UTF-8 text"))?;
    let lower = css.to_ascii_lowercase();
    for (needle, why) in REFUSED {
        if lower.contains(needle) {
            return Err(format!("reader style `{style}` is not applied: {why}"));
        }
    }
    let css_dir = Path::new(&rel)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    inline_urls(&css, &css_dir, |path| {
        crate::fs::read_bytes(cx, root_id, path)
    })
    .map_err(|e| format!("reader style `{style}` is not applied: {e}"))
}

/// `css` with each `url(...)` checked and every project file inlined.
/// `read` reads a project-relative path.
pub(crate) fn inline_urls(
    css: &str,
    css_dir: &Path,
    read: impl Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<String, String> {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    let mut inlined = 0usize;
    loop {
        let Some(at) = rest.to_ascii_lowercase().find("url(") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..at]);
        let after = &rest[at + 4..];
        let close = after
            .find(')')
            .ok_or_else(|| "a url( is never closed".to_string())?;
        let raw = after[..close].trim();
        let target = raw
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(raw)
            .trim();
        let lower = target.to_ascii_lowercase();
        if let Some(kind) = lower.strip_prefix("data:") {
            if !(kind.starts_with("image/") || kind.starts_with("font/")) {
                return Err(format!(
                    "url({}) is a data: URI that is neither an image nor a font",
                    short(target)
                ));
            }
            out.push_str(&format!("url(\"{}\")", target.replace('"', "%22")));
        } else if target.is_empty() || target.starts_with('#') {
            return Err(format!("url({}) names no file", short(target)));
        } else if target.starts_with("//") || has_scheme(target) {
            return Err(format!(
                "url({}) is remote; the reader works offline, so put the file in the project",
                short(target)
            ));
        } else {
            let path = target.split(['?', '#']).next().unwrap_or(target);
            let mime = media_type(path).ok_or_else(|| {
                format!("url({path}) is not a font or image file this page can inline")
            })?;
            let rel = join_rel(css_dir, path)?;
            let bytes = read(&rel).map_err(|e| format!("url({path}): {e}"))?;
            inlined += bytes.len();
            if inlined > MAX_INLINED_BYTES {
                return Err(format!(
                    "its url() files total over {} MiB",
                    MAX_INLINED_BYTES / (1024 * 1024)
                ));
            }
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            out.push_str(&format!("url(\"data:{mime};base64,{b64}\")"));
        }
        rest = &after[close + 1..];
    }
    Ok(out)
}

/// `scheme:` at the start, as a URL parser would see it.
fn has_scheme(s: &str) -> bool {
    let Some((scheme, _)) = s.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn short(s: &str) -> String {
    if s.chars().count() > 60 {
        format!("{}…", s.chars().take(60).collect::<String>())
    } else {
        s.to_string()
    }
}

/// The project-relative folder of the main file.
pub fn main_dir(main_rel: &str) -> PathBuf {
    Path::new(main_rel)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn files(list: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_vec()))
            .collect()
    }

    fn inline(css: &str, dir: &str, fs: &BTreeMap<String, Vec<u8>>) -> Result<String, String> {
        inline_urls(css, Path::new(dir), |p| {
            fs.get(p).cloned().ok_or_else(|| format!("no file {p}"))
        })
    }

    #[test]
    fn project_files_inline_as_data_and_data_uris_pass() {
        let fs = files(&[("style/fonts/a.woff2", b"WOF2"), ("style/bg.png", b"PNG!")]);
        let out = inline(
            "@font-face{src:url(fonts/a.woff2) format('woff2')}\n.m-reader{background:url('./bg.png?v=2')}\n.x{background:URL(\"data:image/svg+xml,%3Csvg%3E\")}",
            "style",
            &fs,
        )
        .unwrap();
        assert!(
            out.contains("url(\"data:font/woff2;base64,V09GMg==\") format('woff2')"),
            "{out}"
        );
        assert!(
            out.contains("url(\"data:image/png;base64,UE5HIQ==\")"),
            "{out}"
        );
        assert!(
            out.contains("url(\"data:image/svg+xml,%3Csvg%3E\")"),
            "{out}"
        );
        assert!(!out.contains("fonts/a.woff2"));
    }

    #[test]
    fn remote_escaping_and_unknown_urls_are_refused_plainly() {
        let fs = files(&[("a.png", b"x"), ("notes.txt", b"x")]);
        for (css, says) in [
            (
                ".a{background:url(https://cdn.example.com/x.png)}",
                "is remote",
            ),
            (".a{background:url(//cdn.example.com/x.png)}", "is remote"),
            (".a{background:url(/etc/passwd.png)}", "absolute path"),
            (".a{background:url(../../a.png)}", "outside the project"),
            (".a{background:url(notes.txt)}", "not a font or image"),
            (
                ".a{background:url(data:text/html,x)}",
                "neither an image nor a font",
            ),
            (".a{background:url(missing.png)}", "no file missing.png"),
            (".a{background:url(a.png}", "never closed"),
        ] {
            let e = inline(css, "", &fs).unwrap_err();
            assert!(e.contains(says), "{css}: {e}");
        }
    }

    #[test]
    fn style_paths_stay_in_the_project() {
        assert_eq!(
            join_rel(Path::new("paper"), "reader.css").unwrap(),
            "paper/reader.css"
        );
        assert_eq!(
            join_rel(Path::new("paper"), "../shared/r.css").unwrap(),
            "shared/r.css"
        );
        assert!(join_rel(Path::new(""), "../r.css").is_err());
        assert!(join_rel(Path::new(""), "/r.css").is_err());
    }

    #[test]
    fn a_project_stylesheet_loads_and_refusals_name_the_reason() {
        let cx = &Core::default();
        let dir = crate::test_scratch::dir("reader-style");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("paper/fonts")).unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        crate::grant_root(cx, "rs", &root.to_string_lossy()).unwrap();
        std::fs::write(dir.join("paper/fonts/a.woff2"), b"WOF2").unwrap();
        std::fs::write(
            dir.join("paper/reader.css"),
            ".m-reader{--m-color-bg:#FAF3E8}\n@font-face{font-family:A;src:url(fonts/a.woff2)}",
        )
        .unwrap();
        let css = load(cx, "rs", Path::new("paper"), "reader.css").unwrap();
        assert!(css.contains("--m-color-bg:#FAF3E8") && css.contains("data:font/woff2"));

        for (body, says) in [
            ("@import url(x.css);", "@import"),
            ("/* fine */ .a{} </style><script>", "`</`"),
            (".a{width:expression(alert(1))}", "expression()"),
            (".a{background:image-set(\"a.png\" 1x)}", "image-set()"),
        ] {
            std::fs::write(dir.join("paper/bad.css"), body).unwrap();
            let e = load(cx, "rs", Path::new("paper"), "bad.css").unwrap_err();
            assert!(
                e.contains("is not applied") && e.contains(says),
                "{body}: {e}"
            );
        }
        assert!(load(cx, "rs", Path::new("paper"), "missing.css")
            .unwrap_err()
            .contains("cannot be read"));
        std::fs::write(dir.join("paper/r.txt"), ".a{}").unwrap();
        assert!(load(cx, "rs", Path::new("paper"), "r.txt")
            .unwrap_err()
            .contains("not a .css file"));
        assert!(load(cx, "rs", Path::new(""), "../outside.css")
            .unwrap_err()
            .contains("outside the project"));
    }
}
