//! SyncTeX queries against explicit session roots. The one implementation:
//! the Tauri command and the MCP tool are thin adapters over it. Callers name
//! a main file and a source file by root-relative path; the pdf and outdir
//! are derived here, and results cross back as root-relative paths.

use crate::Core;

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

/// Forward result: the pdf page for a source line, `None` on no match.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ForwardHit {
    pub page: Option<u32>,
}

/// Inverse result: the source line for a pdf position. `rel_path` is `None`
/// when the hit lies outside the root; `line` is `None` on no match.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct InverseHit {
    pub rel_path: Option<String>,
    pub line: Option<u32>,
}

/// A validated query: the sidecar runs inside `outdir` against `pdf_name`.
struct Query {
    root: PathBuf,
    main_dir: PathBuf,
    outdir: PathBuf,
    pdf_name: String,
}

/// Resolve the main file inside the root and require its compiled output.
fn query_for(cx: &Core, root_id: &str, main_rel: &str) -> Result<Query, String> {
    let root = super::fs::session_root(cx, root_id)?;
    let out = super::outputs_of(cx, root_id, main_rel)?;
    if !out.outdir.join(&out.pdf_name).is_file() {
        return Err(format!("no compiled output for {}", main_rel));
    }
    Ok(Query {
        root,
        main_dir: out.dir,
        outdir: out.outdir,
        pdf_name: out.pdf_name,
    })
}

fn run_sidecar(dir: &Path, args: &[String]) -> Result<String, String> {
    let bin = super::sidecar_path_for("maleficium-engine")?;
    let output = super::quiet_command(&bin)
        .arg("synctex")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("synctex sidecar failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .rev()
        .take(500)
        .collect::<Vec<_>>()
        .join("\n"))
}

/// First `<key>:<value>` line's trimmed value.
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .map(str::trim)
}

fn parse_forward(text: &str) -> ForwardHit {
    if text.contains("no_match") || text.contains("No tag for") || text == "{}" {
        return ForwardHit { page: None };
    }
    let page = field(text, "Page").and_then(|v| v.parse::<u32>().ok());
    ForwardHit {
        page: page.map(|p| p.max(1)),
    }
}

/// A box SyncTeX placed a source line in: pdf points from the page's top
/// left, `y` the top edge (the tool reports the baseline and the height).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineBox {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Every box of a forward result, in the tool's order.
fn parse_boxes(text: &str) -> Vec<LineBox> {
    let mut boxes = Vec::new();
    for record in text.split("Output:").skip(1) {
        let num = |k: &str| field(record, k).and_then(|v| v.parse::<f32>().ok());
        let page = field(record, "Page").and_then(|v| v.parse::<u32>().ok());
        if let (Some(page), Some(h), Some(v), Some(w), Some(hh)) =
            (page, num("h"), num("v"), num("W"), num("H"))
        {
            boxes.push(LineBox {
                page: page.max(1),
                x: h,
                y: v - hh,
                width: w,
                height: hh,
            });
        }
    }
    boxes
}

/// Map an `Input:` path to a root-relative one. Relative inputs resolve
/// against the main file's directory, where the engine ran.
fn rel_in_root(root: &Path, main_dir: &Path, input: &str) -> Option<String> {
    let p = Path::new(input);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        main_dir.join(p)
    };
    let canon = dunce::canonicalize(&abs).ok()?;
    let rel = canon.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn parse_inverse(text: &str, root: &Path, main_dir: &Path) -> InverseHit {
    InverseHit {
        rel_path: field(text, "Input").and_then(|i| rel_in_root(root, main_dir, i)),
        line: field(text, "Line").and_then(|v| v.parse::<u32>().ok()),
    }
}

/// The sidecar's raw answer to a forward query.
fn forward_text(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    tex_rel: &str,
    line: u32,
) -> Result<String, String> {
    let q = query_for(cx, root_id, main_rel)?;
    let tex = super::fs::resolve_in(cx, root_id, tex_rel)?;
    // CWD=outdir with the bare pdf name: an absolute `-o` resolves the `-i`
    // tag against the wrong file table. The gz stores absolute Input paths.
    run_sidecar(
        &q.outdir,
        &[
            "view".to_string(),
            "-i".to_string(),
            format!("{}:1:{}", line, tex.to_string_lossy()),
            "-o".to_string(),
            q.pdf_name,
        ],
    )
}

/// Forward (editor → PDF): the page showing `line` of `tex_rel`.
pub fn forward(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    tex_rel: &str,
    line: u32,
) -> Result<ForwardHit, String> {
    let text = forward_text(cx, root_id, main_rel, tex_rel, line)?;
    Ok(parse_forward(&text))
}

/// Forward with geometry: the boxes showing `line`, empty on no match.
pub(crate) fn forward_boxes(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    tex_rel: &str,
    line: u32,
) -> Result<Vec<LineBox>, String> {
    let text = forward_text(cx, root_id, main_rel, tex_rel, line)?;
    if parse_forward(&text).page.is_none() {
        return Ok(Vec::new());
    }
    // run_sidecar keeps the tool's last lines newest first; records read in
    // the tool's own order.
    let text = text.lines().rev().collect::<Vec<_>>().join("\n");
    Ok(parse_boxes(&text))
}

/// Inverse (PDF → editor): the source line at a position on `page`.
pub fn inverse(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    page: u32,
    x: f32,
    y: f32,
) -> Result<InverseHit, String> {
    let q = query_for(cx, root_id, main_rel)?;
    // The tool resolves `<pdf>.synctex.gz` relative to CWD.
    let text = run_sidecar(
        &q.outdir,
        &[
            "edit".to_string(),
            "-o".to_string(),
            format!("{}:{}:{}:{}", page, x, y, q.pdf_name),
        ],
    )?;
    Ok(parse_inverse(&text, &q.root, &q.main_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant_tmp(cx: &Core, name: &str) -> (String, PathBuf) {
        let dir = crate::test_scratch::dir(&format!("sync-{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        let id = format!("sync-{}", name);
        crate::fs::grant_root(cx, &id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    #[test]
    fn query_requires_a_compiled_main_inside_the_root() {
        let cx = &Core::default();
        let (id, root) = grant_tmp(cx, "query");
        std::fs::write(root.join("main.tex"), "x").unwrap();
        for bad in crate::test_scratch::escapes() {
            assert!(query_for(cx, &id, bad).is_err(), "{bad}");
        }
        assert!(query_for(cx, &id, "missing.tex").is_err());
        assert!(query_for(cx, "unknown-root", "main.tex").is_err());
        let err = query_for(cx, &id, "main.tex").err().unwrap();
        assert!(err.contains("no compiled output"), "{}", err);
    }

    #[test]
    fn forward_parse_matches_the_tool_output() {
        let hit = "SyncTeX result begin\nOutput:/x/main.pdf\nPage:3\nx:1.0\nSyncTeX result end";
        assert_eq!(parse_forward(hit), ForwardHit { page: Some(3) });
        assert_eq!(parse_forward("Page: 0\n"), ForwardHit { page: Some(1) });
        assert_eq!(parse_forward("synctex_no_match"), ForwardHit { page: None });
        assert_eq!(
            parse_forward("No tag for main.tex"),
            ForwardHit { page: None }
        );
        assert_eq!(parse_forward("{}"), ForwardHit { page: None });
        assert_eq!(parse_forward("Page:abc"), ForwardHit { page: None });
    }

    #[test]
    fn forward_boxes_are_read_from_every_record() {
        let text = "SyncTeX result begin\nOutput:main.pdf\nPage:1\nx:225.8\ny:492.1\nh:225.8\nv:496.2\nW:160.4\nH:13.5\nbefore:\nOutput:main.pdf\nPage:2\nx:1\ny:1\nh:72\nv:100\nW:20\nH:10\nSyncTeX result end";
        let b = parse_boxes(text);
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].page, b[0].x, b[0].width), (1, 225.8, 160.4));
        assert!((b[0].y - 482.7).abs() < 0.01, "{}", b[0].y);
        assert_eq!((b[1].page, b[1].y), (2, 90.0));
        assert!(parse_boxes("synctex_no_match").is_empty());
        assert!(parse_boxes("Output:main.pdf\nPage:1\nh:abc").is_empty());
    }

    #[test]
    fn inverse_parse_returns_root_relative_paths() {
        let cx = &Core::default();
        let (_, root) = grant_tmp(cx, "inv");
        std::fs::create_dir_all(root.join("ch")).unwrap();
        std::fs::write(root.join("ch/a.tex"), "x").unwrap();
        let abs = root.join("ch/a.tex");
        let text = format!("Line:7\nInput:{}\n", abs.display());
        assert_eq!(
            parse_inverse(&text, &root, &root),
            InverseHit {
                rel_path: Some("ch/a.tex".into()),
                line: Some(7)
            }
        );
        let rel = parse_inverse("Input:./ch/a.tex\nLine:2", &root, &root);
        assert_eq!(rel.rel_path.as_deref(), Some("ch/a.tex"));
        let outside = parse_inverse("Input:/etc/hostname\nLine:1", &root, &root);
        assert_eq!(
            outside,
            InverseHit {
                rel_path: None,
                line: Some(1)
            }
        );
        assert_eq!(
            parse_inverse("", &root, &root),
            InverseHit {
                rel_path: None,
                line: None
            }
        );
    }
}
