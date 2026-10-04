use super::*;
use std::cell::Cell;

/// A real compile of e2e/fixtures/interactive: pdf and sidecar, with the
/// fixture's own source files copied beside them.
const REAL_PDF: &[u8] = include_bytes!("../../testdata/interactive/main.pdf");
const REAL_SIDECAR: &str = include_str!("../../testdata/interactive/main.mfw");
/// A latexml-shaped conversion of the same fixture (see reflow/article).
const CONVERTED: &str = include_str!("../reflow/fixtures/interactive-converted.frag");

/// The fixture's conversion without its deliberate problems: a missing
/// figure, an undefined macro and hostile markup.
fn clean_conversion() -> String {
    let start = CONVERTED.find("<figure id=\"S1.F4\"").unwrap();
    let end = CONVERTED[start..].find("</figure>").unwrap() + start + "</figure>".len();
    let mut html = format!("{}{}", &CONVERTED[..start], &CONVERTED[end..]);
    html = html.replace(
        "<span class=\"ltx_ERROR undefined\">\\undefinedmacro</span>",
        "",
    );
    let hostile = html.find("<p class=\"ltx_p\" onclick").unwrap();
    let hostile_end = html[hostile..].find("</p>").unwrap() + hostile + "</p>".len();
    html.replace_range(hostile..hostile_end, "");
    html
}

/// A converter that hands back fixed HTML and log errors, or fails.
struct Fixed {
    html: Result<String, String>,
    errors: Vec<String>,
}

impl reflow::convert::Converter for Fixed {
    fn convert(&self, _cx: &Core, main: &Path, work: &Path) -> reflow::convert::Conversion {
        assert!(main.is_absolute() && main.is_file(), "{}", main.display());
        assert!(work.is_dir(), "an empty scratch folder");
        reflow::convert::Conversion {
            html: self.html.clone(),
            errors: self.errors.clone(),
        }
    }
}

fn converts_to(p: &Project, html: Result<String, String>, errors: &[&str]) {
    p.cx.set_converter(std::sync::Arc::new(Fixed {
        html,
        errors: errors.iter().map(|e| e.to_string()).collect(),
    }));
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let t = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &t);
        } else {
            std::fs::copy(e.path(), &t).unwrap();
        }
    }
}

struct Project {
    cx: Core,
    id: String,
    root: PathBuf,
    out: PathBuf,
}

/// The fixture project granted as a root, compiled (pdf and sidecar in its
/// outdir), plus an empty scratch folder outside it to export into.
fn project(name: &str, sidecar: &str) -> Project {
    let cx = Core::default();
    let dir = crate::test_scratch::dir(&format!("bundle-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    let root_dir = dir.join("proj");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../e2e/fixtures/interactive"),
        &root_dir,
    );
    let root = dunce::canonicalize(&root_dir).unwrap();
    let id = format!("bundle-{name}");
    crate::fs::grant_root(&cx, &id, &root.to_string_lossy()).unwrap();
    let o = crate::outputs::outputs_of(&cx, &id, "main.tex").unwrap();
    let _ = std::fs::remove_dir_all(&o.outdir);
    std::fs::create_dir_all(&o.outdir).unwrap();
    std::fs::write(o.outdir.join(&o.pdf_name), REAL_PDF).unwrap();
    std::fs::write(o.outdir.join("main.mfw"), sidecar).unwrap();
    let out = dunce::canonicalize({
        std::fs::create_dir_all(dir.join("out")).unwrap();
        dir.join("out")
    })
    .unwrap();
    let p = Project { cx, id, root, out };
    converts_to(&p, Ok(clean_conversion()), &[]);
    p
}

fn export(p: &Project, dest: &str, profile: BundleProfile) -> Result<BundleExported, String> {
    export_bundle(&p.cx, &p.id, "main.tex", dest, profile, None)
}

fn dest(p: &Project, name: &str) -> String {
    p.out.join(name).to_string_lossy().to_string()
}

fn manifest_of(folder: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(folder.join("manifest.json")).unwrap()).unwrap()
}

fn files_under(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.file_type().unwrap().is_dir() {
                walk(base, &e.path(), out);
            } else {
                out.push(
                    e.path()
                        .strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v);
    v.sort();
    v
}

fn island_json(html: &str, id: &str) -> Value {
    let open = format!("<script type=\"application/json\" id=\"{id}\">");
    let s = html.find(&open).unwrap() + open.len();
    let e = html[s..].find("</script>").unwrap() + s;
    serde_json::from_str(&html[s..e]).unwrap()
}

fn kinds(e: &BundleExported) -> Vec<BundleWarningKind> {
    e.warnings.iter().map(|w| w.kind).collect()
}

#[test]
fn the_folder_profile_writes_the_layout_and_the_manifest_validates() {
    let p = project("folder", REAL_SIDECAR);
    let d = dest(&p, "paper");
    let r = export(&p, &d, BundleProfile::Folder).unwrap();
    let folder = PathBuf::from(&d);
    assert_eq!((r.widgets, r.profile), (5, BundleProfile::Folder));

    let m = manifest_of(&folder);
    validate_manifest(&m).unwrap();
    assert_eq!(m["format"], "maleficium-paper-bundle");
    assert_eq!(m["profile"], "folder");
    assert_eq!(m["pdf"]["path"], "paper.pdf");
    assert_eq!(
        std::fs::read(folder.join("paper.pdf")).unwrap(),
        REAL_PDF,
        "paper.pdf is the compiled pdf, byte for byte"
    );
    assert_eq!(m["pdf"]["sha256"], sha_of(REAL_PDF));
    assert!(m["pdf"]["pages"].as_u64().unwrap() >= 3);

    let files = files_under(&folder);
    for want in [
        "index.html",
        "manifest.json",
        "paper.pdf",
        "theme/theme.css",
        "theme/theme.json",
    ] {
        assert!(files.iter().any(|f| f == want), "{want} in {files:?}");
    }
    let ids: Vec<&str> = m["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "fig-mesh",
            "tab-results",
            "fig-clip",
            "fig-chart",
            "fig-demo"
        ]
    );
    for id in &ids {
        assert!(
            files.contains(&format!("widgets/{id}/index.html")),
            "{id}: {files:?}"
        );
    }

    // Content addressing: every bundled asset sits at <sha256>.<ext>, its
    // bytes hash to the name, and a source file hashes to the same value.
    let assets = m["assets"].as_object().unwrap();
    for (key, a) in assets {
        assert_eq!(a["mode"], "bundled", "{key}");
        let path = a["path"].as_str().unwrap();
        let bytes = std::fs::read(folder.join(path)).unwrap();
        assert_eq!(sha_of(&bytes), a["sha256"], "{key}");
        assert_eq!(bytes.len() as u64, a["bytes"].as_u64().unwrap(), "{key}");
        assert_eq!(
            path,
            format!(
                "assets/{}.{}",
                a["sha256"].as_str().unwrap(),
                path.rsplit('.').next().unwrap()
            )
        );
        if let Some(src) = a["source"].as_str() {
            assert!(!src.starts_with('/') && !src.contains(".."), "{src}");
            assert_eq!(
                sha_of(&std::fs::read(p.root.join(src)).unwrap()),
                a["sha256"],
                "{key}"
            );
        }
    }
    assert_eq!(assets["fig-mesh-model"]["mime"], "model/gltf-binary");
    assert_eq!(assets["fig-mesh-model"]["source"], "models/mesh.glb");
    // The table's poster is cropped from the pdf: a png with no source.
    assert!(assets["tab-results-poster"].get("source").is_none());
    assert_eq!(assets["tab-results-poster"]["mime"], "image/png");
    assert_eq!(
        &std::fs::read(folder.join(assets["tab-results-poster"]["path"].as_str().unwrap()))
            .unwrap()[..4],
        b"\x89PNG"
    );
    let chart = m["widgets"][3].clone();
    assert_eq!(chart["sources"]["spec"], "fig-chart-spec");
    assert_eq!(m["widgets"][1]["options"]["pdfrows"], 2);
    assert_eq!(m["widgets"][0]["rect"].as_array().unwrap().len(), 4);

    // Nothing was written into the project, and no staging folder is left.
    assert!(!files_under(&p.root).iter().any(|f| f.contains("manifest")));
    let left: Vec<_> = std::fs::read_dir(&p.out)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(left, ["paper"]);
}

#[test]
fn every_widget_is_one_inline_document_with_its_policy_first() {
    let p = project("fold", REAL_SIDECAR);
    let d = dest(&p, "paper");
    let r = export(&p, &d, BundleProfile::Folder).unwrap();
    let prologue = regex::Regex::new(r"(?i)<!doctype[^>]*>|<html[^>]*>|<head[^>]*>").unwrap();
    for id in [
        "fig-mesh",
        "tab-results",
        "fig-clip",
        "fig-chart",
        "fig-demo",
    ] {
        let html =
            std::fs::read_to_string(PathBuf::from(&d).join(format!("widgets/{id}/index.html")))
                .unwrap();
        let at = html
            .find("<meta http-equiv=\"Content-Security-Policy\"")
            .unwrap();
        let before = prologue.replace_all(&html[..at], "").trim().to_string();
        assert!(
            before.is_empty(),
            "{id}: the policy is the first element: {before}"
        );
        assert!(html.contains("default-src 'none'"), "{id}");
        assert!(html.contains("connect-src 'none'"), "{id}");
        assert!(
            !html.contains("'self'"),
            "{id}: folded documents need no 'self'"
        );
        assert!(!html.contains("<script src"), "{id}");
    }
    // Every built-in runtime exports with its own host and nothing warns.
    assert!(
        r.warnings
            .iter()
            .all(|w| w.kind == BundleWarningKind::Metadata),
        "{:?}",
        r.warnings
    );
    let host = |id: &str| {
        std::fs::read_to_string(PathBuf::from(&d).join(format!("widgets/{id}/index.html"))).unwrap()
    };
    let (model, video) = (host("fig-mesh"), host("fig-clip"));
    assert!(model.contains("id=\"view\"") && model.contains("WebGLRenderer"));
    assert!(video.contains("<video id=\"v\" controls"));
    for (id, html) in [("fig-mesh", &model), ("fig-clip", &video)] {
        assert!(!html.contains("<img"), "{id}: not a poster document");
        assert!(
            !html.contains("type=\"module\""),
            "{id}: one classic script"
        );
        assert_eq!(html.matches("<script").count(), 1, "{id}");
    }
    // The table and chart hosts are the generated runtimes.
    let table =
        std::fs::read_to_string(PathBuf::from(&d).join("widgets/tab-results/index.html")).unwrap();
    assert!(table.contains("id=\"filter\""));
}

#[test]
fn a_widget_whose_runtime_is_not_built_in_is_refused() {
    let sidecar = REAL_SIDECAR.replace("|model@1|", "|model@9|");
    assert_ne!(sidecar, REAL_SIDECAR, "the fixture line changed");
    let p = project("unknown-runtime", &sidecar);
    let d = dest(&p, "paper");
    let e = export(&p, &d, BundleProfile::Folder).unwrap_err();
    assert!(e.contains("fig-mesh") && e.contains("model@9"), "{e}");
    assert!(!PathBuf::from(&d).exists());
}

#[test]
fn the_single_file_profile_is_one_file_with_everything_inline() {
    let p = project("single", REAL_SIDECAR);
    let d = dest(&p, "paper.html");
    let r = export(&p, &d, BundleProfile::SingleFile).unwrap();
    assert!(PathBuf::from(&d).is_file());
    let left: Vec<_> = std::fs::read_dir(&p.out).unwrap().flatten().collect();
    assert_eq!(left.len(), 1, "one file and no staging remnant");
    let html = std::fs::read_to_string(&d).unwrap();
    assert_eq!(r.bytes, html.len() as u64);

    let m = island_json(&html, "mfw-manifest");
    validate_manifest(&m).unwrap();
    assert_eq!(m["profile"], "single-file");
    assert_eq!(m["sizeCapBytes"], DEFAULT_SIZE_CAP_BYTES);
    assert!(m["pdf"]["path"].is_null());
    assert!(m["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| w.get("entry").is_none()));
    let assets = m["assets"].as_object().unwrap();
    assert!(assets
        .values()
        .all(|a| a["mode"] == "inline" && a["path"].is_null()));

    let blobs = island_json(&html, "mfw-assets");
    let engine = base64::engine::general_purpose::STANDARD;
    for (key, a) in assets {
        let b = engine
            .decode(blobs[a["sha256"].as_str().unwrap()].as_str().unwrap())
            .unwrap();
        assert_eq!(sha_of(&b), a["sha256"], "{key}");
    }
    let href = html
        .split("id=\"pdf-link\" href=\"data:application/pdf;base64,")
        .nth(1)
        .and_then(|t| t.split('"').next())
        .unwrap();
    assert_eq!(
        engine.decode(href).unwrap(),
        REAL_PDF,
        "one copy, in the link"
    );
    assert!(!html.contains("mfw-pdf"));
    let widgets = island_json(&html, "mfw-widgets");
    assert_eq!(widgets.as_object().unwrap().len(), 5);
    assert!(widgets["fig-demo"]
        .as_str()
        .unwrap()
        .contains("demo widget"));
    // The reader carries the single-file policy, and a document string can
    // never close its island.
    assert!(html.contains("<meta http-equiv=\"Content-Security-Policy\""));
    assert!(!fold::policy_of(&html).contains("frame-src"));
    assert_eq!(html.matches("</script>").count(), 4);
}

#[test]
fn the_hosted_profile_is_a_folder_marked_hosted() {
    let p = project("hosted", REAL_SIDECAR);
    let d = dest(&p, "paper");
    export(&p, &d, BundleProfile::Hosted).unwrap();
    let m = manifest_of(Path::new(&d));
    validate_manifest(&m).unwrap();
    assert_eq!(m["profile"], "hosted");
    assert!(PathBuf::from(&d)
        .join("widgets/fig-mesh/index.html")
        .is_file());
}

#[test]
fn the_single_file_profile_warns_past_its_cap_and_still_writes() {
    let p = project("cap", REAL_SIDECAR);
    let d = dest(&p, "small.html");
    let r = export_bundle(
        &p.cx,
        &p.id,
        "main.tex",
        &d,
        BundleProfile::SingleFile,
        Some(1000),
    )
    .unwrap();
    assert!(
        kinds(&r).contains(&BundleWarningKind::SizeCap),
        "{:?}",
        r.warnings
    );
    assert!(
        PathBuf::from(&d).is_file(),
        "an over-cap export is never refused"
    );
    let m = island_json(&std::fs::read_to_string(&d).unwrap(), "mfw-manifest");
    assert_eq!(m["sizeCapBytes"], 1000);

    // The default cap is 50 MiB: a 51 MiB model trips it, the fixture does not.
    let calm = export(&p, &dest(&p, "calm.html"), BundleProfile::SingleFile).unwrap();
    assert!(!kinds(&calm).contains(&BundleWarningKind::SizeCap));
    let big = std::fs::File::create(p.root.join("models/mesh.glb")).unwrap();
    big.set_len(51 * 1024 * 1024).unwrap();
    let r = export(&p, &dest(&p, "big.html"), BundleProfile::SingleFile).unwrap();
    assert!(
        kinds(&r).contains(&BundleWarningKind::SizeCap),
        "{:?}",
        r.warnings
    );
    // Folder profile has no cap: the same big model is just a file.
    let r = export(&p, &dest(&p, "bigfolder"), BundleProfile::Folder).unwrap();
    assert!(!kinds(&r).contains(&BundleWarningKind::SizeCap));
}

#[test]
fn a_destination_inside_the_project_is_refused_and_nothing_is_written() {
    let p = project("inside", REAL_SIDECAR);
    let before = files_under(&p.root);
    for (name, d) in [
        (
            "inside",
            p.root.join("export").to_string_lossy().to_string(),
        ),
        ("root itself", p.root.to_string_lossy().to_string()),
        (
            "a nested file",
            p.root
                .join("figures/out.html")
                .to_string_lossy()
                .to_string(),
        ),
    ] {
        for profile in [
            BundleProfile::Folder,
            BundleProfile::SingleFile,
            BundleProfile::Hosted,
        ] {
            let e = export(&p, &d, profile).unwrap_err();
            assert!(e.contains("inside the project"), "{name}/{profile:?}: {e}");
        }
    }
    // A folder that holds the project is just as wrong.
    let parent = p.root.parent().unwrap().to_string_lossy().to_string();
    assert!(export(&p, &parent, BundleProfile::Folder).is_err());
    assert_eq!(files_under(&p.root), before);
    assert!(!p.root.join("export").exists());
    // Relative destinations never validate.
    assert!(export(&p, "out/paper", BundleProfile::Folder)
        .unwrap_err()
        .contains("absolute"));
}

#[test]
fn a_symlink_that_leaves_the_project_is_not_followed() {
    let p = project("symlink", REAL_SIDECAR);
    let outside = p.out.join("secret.glb");
    std::fs::write(&outside, b"secret").unwrap();
    std::fs::remove_file(p.root.join("models/mesh.glb")).unwrap();
    std::os::unix::fs::symlink(&outside, p.root.join("models/mesh.glb")).unwrap();
    let e = export(&p, &dest(&p, "paper"), BundleProfile::Folder).unwrap_err();
    assert!(e.contains("outside project"), "{e}");
    assert!(!p.out.join("paper").exists());
}

fn remote_only_sidecar() -> String {
    REAL_SIDECAR.replace(
        "widget|fig-clip|video|video@1|||house|figures/clip.png|video=media/clip.mp4|height=142.26378pt,remote=,sha256=|",
        "widget|fig-clip|video|video@1|||house|figures/clip.png||height=142.26378pt,remote=https://media.example.org/clip.mp4,sha256=|",
    )
}

struct Counting<'a> {
    calls: &'a Cell<u32>,
    body: &'static [u8],
}

impl Fetcher for Counting<'_> {
    fn fetch(&self, _url: &str) -> Result<Vec<u8>, String> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.body.to_vec())
    }
}

#[test]
fn a_remote_asset_with_no_local_copy_is_refused_without_approval() {
    let sidecar = remote_only_sidecar();
    assert_ne!(sidecar, REAL_SIDECAR, "the fixture line changed");
    let p = project("remote", &sidecar);
    let d = dest(&p, "paper");
    let calls = Cell::new(0);
    let fetcher = Counting {
        calls: &calls,
        body: b"video bytes",
    };

    // The plain call (what MCP makes) cannot fetch: refused, nothing written.
    let e = export(&p, &d, BundleProfile::Folder).unwrap_err();
    assert!(
        e.contains("fig-clip") && e.contains("https://media.example.org/clip.mp4"),
        "{e}"
    );
    assert!(e.contains("approve"), "{e}");
    assert!(!PathBuf::from(&d).exists());

    // An approval for another url, or a fetcher without any approval, does
    // not help either; the fetcher is never called.
    let other = ["https://other.example.org/x.mp4".to_string()];
    for approved in [&other[..], &[]] {
        let o = BundleOptions {
            size_cap_bytes: None,
            approved_fetch: approved,
            fetcher: Some(&fetcher),
        };
        let e = export_bundle_with(&p.cx, &p.id, "main.tex", &d, BundleProfile::Folder, &o)
            .unwrap_err();
        assert!(e.contains("approve"), "{e}");
    }
    assert_eq!(calls.get(), 0, "nothing is downloaded without approval");

    // Approved for exactly that url: fetched once, hashed, and kept remote.
    let yes = ["https://media.example.org/clip.mp4".to_string()];
    let o = BundleOptions {
        size_cap_bytes: None,
        approved_fetch: &yes,
        fetcher: Some(&fetcher),
    };
    export_bundle_with(&p.cx, &p.id, "main.tex", &d, BundleProfile::Folder, &o).unwrap();
    assert_eq!(calls.get(), 1);
    let m = manifest_of(Path::new(&d));
    validate_manifest(&m).unwrap();
    let a = &m["assets"]["fig-clip-video"];
    assert_eq!(a["mode"], "remote");
    assert_eq!(a["url"], "https://media.example.org/clip.mp4");
    assert_eq!(a["sha256"], sha_of(b"video bytes"));
    assert!(a.get("path").is_none());
    assert!(
        !files_under(Path::new(&d))
            .iter()
            .any(|f| f.contains(&sha_of(b"video bytes"))),
        "a remote asset is not stored"
    );

    // Approved, but no downloader in this build: an error that says so.
    let o = BundleOptions {
        size_cap_bytes: None,
        approved_fetch: &yes,
        fetcher: None,
    };
    let e = export_bundle_with(
        &p.cx,
        &p.id,
        "main.tex",
        &dest(&p, "again"),
        BundleProfile::Folder,
        &o,
    )
    .unwrap_err();
    assert!(e.contains("no downloader"), "{e}");
}

#[test]
fn a_remote_asset_with_a_local_copy_is_hashed_locally_and_not_stored() {
    let local = REAL_SIDECAR.replace(
        "remote=,sha256=|",
        "remote=https://media.example.org/clip.mp4,sha256=|",
    );
    let p = project("remote-local", &local);
    for profile in [BundleProfile::Folder, BundleProfile::SingleFile] {
        let d = dest(
            &p,
            if profile == BundleProfile::Folder {
                "f"
            } else {
                "s.html"
            },
        );
        export(&p, &d, profile).unwrap();
        let m = if profile == BundleProfile::Folder {
            manifest_of(Path::new(&d))
        } else {
            island_json(&std::fs::read_to_string(&d).unwrap(), "mfw-manifest")
        };
        validate_manifest(&m).unwrap();
        let a = &m["assets"]["fig-clip-video"];
        assert_eq!(
            a["mode"], "remote",
            "{profile:?}: remote stays remote even in single-file"
        );
        assert_eq!(a["source"], "media/clip.mp4");
        assert_eq!(
            a["sha256"],
            sha_of(&std::fs::read(p.root.join("media/clip.mp4")).unwrap())
        );
    }
    // A declared sha that disagrees with the file is an error.
    let wrong = REAL_SIDECAR.replace(
        "remote=,sha256=|",
        &format!(
            "remote=https://media.example.org/clip.mp4,sha256={}|",
            "0".repeat(64)
        ),
    );
    let p = project("remote-wrong", &wrong);
    let e = export(&p, &dest(&p, "w"), BundleProfile::Folder).unwrap_err();
    assert!(e.contains("does not match"), "{e}");
    // http is never accepted.
    let http = REAL_SIDECAR.replace(
        "remote=,sha256=|",
        "remote=http://media.example.org/clip.mp4,sha256=|",
    );
    let p = project("remote-http", &http);
    assert!(export(&p, &dest(&p, "h"), BundleProfile::Folder)
        .unwrap_err()
        .contains("https"));
}

#[test]
fn the_inline_option_puts_a_widgets_assets_inside_the_reader_page() {
    let inline = REAL_SIDECAR.replace("pdfrows=2|", "pdfrows=2,inline=true|");
    let p = project("inline", &inline);
    let d = dest(&p, "paper");
    export(&p, &d, BundleProfile::Folder).unwrap();
    let m = manifest_of(Path::new(&d));
    validate_manifest(&m).unwrap();
    assert_eq!(m["assets"]["tab-results-data"]["mode"], "inline");
    assert!(m["assets"]["tab-results-data"]["path"].is_null());
    assert_eq!(m["assets"]["fig-mesh-model"]["mode"], "bundled");
    let html = std::fs::read_to_string(PathBuf::from(&d).join("index.html")).unwrap();
    let blobs = island_json(&html, "mfw-assets");
    assert!(blobs
        .get(m["assets"]["tab-results-data"]["sha256"].as_str().unwrap())
        .is_some());
    assert!(m["widgets"][1]
        .get("options")
        .unwrap()
        .get("inline")
        .is_none());
}

#[test]
fn a_failed_export_leaves_nothing_at_the_destination() {
    let p = project("fail", REAL_SIDECAR);
    std::fs::remove_file(p.root.join("data/results.csv")).unwrap();
    let d = dest(&p, "paper");
    let e = export(&p, &d, BundleProfile::Folder).unwrap_err();
    assert!(
        e.contains("tab-results") || e.contains("results.csv"),
        "{e}"
    );
    assert!(
        std::fs::read_dir(&p.out).unwrap().next().is_none(),
        "no folder, no staging remnant"
    );

    // Never compiled: a clear error and no destination.
    let q = project("never", REAL_SIDECAR);
    let o = crate::outputs::outputs_of(&q.cx, &q.id, "main.tex").unwrap();
    std::fs::remove_file(o.outdir.join(&o.pdf_name)).unwrap();
    let e = export(&q, &dest(&q, "paper"), BundleProfile::Folder).unwrap_err();
    assert!(e.contains("compile"), "{e}");
    assert!(std::fs::read_dir(&q.out).unwrap().next().is_none());
}

#[test]
fn an_export_replaces_an_earlier_bundle_but_never_a_foreign_folder() {
    let p = project("replace", REAL_SIDECAR);
    let d = dest(&p, "paper");
    export(&p, &d, BundleProfile::Folder).unwrap();
    std::fs::write(PathBuf::from(&d).join("stale.txt"), "old").unwrap();
    export(&p, &d, BundleProfile::Folder).unwrap();
    assert!(
        !PathBuf::from(&d).join("stale.txt").exists(),
        "the earlier bundle was replaced whole"
    );

    let foreign = p.out.join("notes");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("mine.txt"), "keep").unwrap();
    let e = export(&p, &foreign.to_string_lossy(), BundleProfile::Folder).unwrap_err();
    assert!(e.contains("not empty"), "{e}");
    assert!(foreign.join("mine.txt").is_file());
    // An empty folder is fine; a file where a folder goes is not.
    let empty = p.out.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    export(&p, &empty.to_string_lossy(), BundleProfile::Folder).unwrap();
    let file = p.out.join("a-file");
    std::fs::write(&file, "x").unwrap();
    assert!(export(&p, &file.to_string_lossy(), BundleProfile::Folder).is_err());
    // A single-file export overwrites a file but not a folder.
    export(&p, &file.to_string_lossy(), BundleProfile::SingleFile).unwrap();
    assert!(export(&p, &foreign.to_string_lossy(), BundleProfile::SingleFile).is_err());
}

#[test]
fn the_export_has_no_network_code() {
    // The app does not fetch: no http client is a dependency of core, and
    // the exporter opens no socket.
    let cargo = include_str!("../../Cargo.toml");
    for client in [
        "reqwest",
        "ureq",
        "hyper",
        "curl",
        "isahc",
        "attohttpc",
        "surf",
        "minreq",
    ] {
        assert!(!cargo.contains(client), "core depends on {client}");
    }
    let src = include_str!("../bundle.rs");
    for api in ["TcpStream", "std::net", "UdpSocket", "Command::new"] {
        assert!(
            !src.contains(&format!("{api}(")) && !src.contains(&format!("use {api}")),
            "bundle.rs uses {api}"
        );
    }
}

// ---- the manifest schema and its red controls ---------------------------

fn good_manifest() -> Value {
    let p = project("manifest", REAL_SIDECAR);
    let d = dest(&p, "m");
    export(&p, &d, BundleProfile::Folder).unwrap();
    manifest_of(Path::new(&d))
}

#[test]
fn the_schema_rejects_what_the_note_forbids() {
    let good = good_manifest();
    validate_manifest(&good).unwrap();
    let bad = |f: &dyn Fn(&mut Value)| -> String {
        let mut m = good.clone();
        f(&mut m);
        validate_manifest(&m).unwrap_err()
    };
    assert!(bad(&|m| {
        m.as_object_mut().unwrap().remove("theme");
    })
    .contains("schema"));
    assert!(bad(&|m| m["formatVersion"] = 2.into()).contains("schema"));
    assert!(bad(&|m| m["extra"] = true.into()).contains("schema"));
    assert!(bad(&|m| m["pdf"]["sha256"] = "XYZ".into()).contains("schema"));
    assert!(bad(&|m| m["widgets"][0]["id"] = "Bad_Id".into()).contains("schema"));
    assert!(bad(&|m| m["widgets"][0]["alt"] = "".into()).contains("schema"));
    assert!(
        bad(&|m| m["assets"]["fig-clip-video"]["mode"] = "remote".into()).contains("schema"),
        "remote needs a url"
    );
    // Invariants the schema cannot express.
    assert!(bad(&|m| m["widgets"][1]["id"] = "fig-mesh".into()).contains("twice"));
    assert!(bad(&|m| m["widgets"][0]["poster"] = "no-such-asset".into()).contains("does not exist"));
    assert!(bad(&|m| m["widgets"][0]["page"] = 99.into()).contains("page"));
    assert!(bad(&|m| m["widgets"][0]["rect"] = json!([10, 10, 5, 20])).contains("rect"));
    assert!(bad(&|m| {
        let w = m["widgets"].as_array_mut().unwrap();
        w.swap(0, 2);
    })
    .contains("order"));
    assert!(bad(&|m| m["profile"] = "single-file".into()).contains("single-file"));
}

#[test]
fn the_committed_schema_is_the_one_the_note_describes() {
    let s: Value = serde_json::from_str(MANIFEST_SCHEMA).unwrap();
    assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!(s["$id"], "maleficium-paper-bundle/1");
    assert_eq!(s["properties"]["format"]["const"], FORMAT);
    assert_eq!(s["properties"]["formatVersion"]["const"], 1);
    assert_eq!(
        s["properties"]["profile"]["enum"],
        json!(["folder", "single-file", "hosted"])
    );
    assert_eq!(
        s["$defs"]["widget"]["properties"]["type"]["enum"],
        json!(["model", "video", "table", "chart", "html"])
    );
}

// ---- paper metadata -----------------------------------------------------

#[test]
fn metadata_comes_from_the_preamble_and_falls_back_with_a_warning() {
    let mut w = Vec::new();
    let m = paper_meta(
        "\\documentclass{article}\n% \\title{Commented}\n\\title[Short]{Towards {Private} Avatars\\thanks{funded}}\n\\author{Ada Lovelace\\\\University A \\and Alan Turing\\thanks{x}\\\\University~B}\n\\begin{document}\\begin{abstract}We study 50\\% of it.\\end{abstract}\\end{document}",
        "main",
        &mut w,
    );
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(m.title, "Towards Private Avatars");
    assert_eq!(
        m.authors,
        [
            ("Ada Lovelace".to_string(), vec!["University A".to_string()]),
            ("Alan Turing".to_string(), vec!["University B".to_string()])
        ]
    );
    assert!(m.abstract_text.unwrap().starts_with("We study"));

    let mut w = Vec::new();
    let m = paper_meta("\\begin{document}x\\end{document}", "main", &mut w);
    assert_eq!((m.title.as_str(), m.authors.len()), ("main", 1));
    assert_eq!(w.len(), 2);
    assert!(w.iter().all(|x| x.kind == BundleWarningKind::Metadata));
}

#[test]
fn the_events_name_the_profile_and_the_outcome() {
    let ok = Ok(BundleExported {
        path: "/x".into(),
        profile: BundleProfile::Folder,
        bytes: 5,
        widgets: 2,
        assets: 3,
        warnings: vec![],
    });
    let e = event(
        "main.tex",
        BundleProfile::Folder,
        &ok,
        maleficium_events::Actor::Agent,
    );
    let j = serde_json::to_value(&e).unwrap();
    assert_eq!(j["event"]["action"], "bundle.exported");
    assert_eq!(j["event"]["profile"], "folder");
    let bad = event(
        "main.tex",
        BundleProfile::SingleFile,
        &Err("no".into()),
        maleficium_events::Actor::User,
    );
    let j = serde_json::to_value(&bad).unwrap();
    assert_eq!(j["event"]["action"], "bundle.failed");
    assert_eq!(j["event"]["profile"], "single-file");
}

#[test]
fn a_preview_lands_in_a_scratch_folder_outside_the_project_and_the_next_run_replaces_it() {
    let p = project("preview", REAL_SIDECAR);
    let base = p.out.join("previews");
    let before = files_under(&p.root);

    let r = preview_bundle_in(&p.cx, &p.id, "main.tex", &base).unwrap();
    let html = PathBuf::from(&r.path);
    assert_eq!(html.file_name().unwrap(), "index.html");
    assert!(html.is_file());
    assert!(!html.starts_with(&p.root), "never inside the project");
    assert!(html.starts_with(&base));
    assert!(std::fs::read_to_string(&html)
        .unwrap()
        .contains("mfw-manifest"));
    assert_eq!(files_under(&p.root), before, "the project is untouched");

    // A stray file in the project's scratch folder is gone after the next run.
    let dir = html.parent().unwrap().to_path_buf();
    std::fs::write(dir.join("stale.txt"), "old").unwrap();
    let again = preview_bundle_in(&p.cx, &p.id, "main.tex", &base).unwrap();
    assert_eq!(again.path, r.path, "same folder per project");
    assert!(!dir.join("stale.txt").exists(), "only the latest is kept");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}

#[test]
fn a_preview_base_that_overlaps_the_project_is_refused() {
    let p = project("preview-inside", REAL_SIDECAR);
    let r = preview_bundle_in(&p.cx, &p.id, "main.tex", &p.root.join("previews"));
    assert!(r.unwrap_err().contains("overlaps the project"));
    assert!(!p.root.join("previews").exists());
}

#[test]
fn previews_of_different_projects_do_not_share_a_folder() {
    let a = project("preview-a", REAL_SIDECAR);
    let b = project("preview-b", REAL_SIDECAR);
    let base = a.out.join("previews");
    let ra = preview_bundle_in(&a.cx, &a.id, "main.tex", &base).unwrap();
    let rb = preview_bundle_in(&b.cx, &b.id, "main.tex", &base).unwrap();
    assert_ne!(ra.path, rb.path);
    assert!(PathBuf::from(&ra.path).is_file(), "b did not clear a");
}

#[test]
fn a_failed_preview_leaves_no_stale_folder() {
    let p = project("preview-fail", REAL_SIDECAR);
    let base = p.out.join("previews");
    let r = preview_bundle_in(&p.cx, &p.id, "nothere.tex", &base);
    assert!(r.is_err());
    assert!(std::fs::read_dir(&base).map(|d| d.count()).unwrap_or(0) == 0);
}

#[test]
fn the_reader_mounts_every_widget_in_article_order_with_no_external_load() {
    let p = project("reader", REAL_SIDECAR);
    // Links in the article may point at the web; nothing loads from it.
    let outside = regex::Regex::new(r#"(?:src|data|action)=\"https?://|<link "#).unwrap();
    for (name, profile) in [
        ("one.html", BundleProfile::SingleFile),
        ("folder", BundleProfile::Folder),
    ] {
        let d = dest(&p, name);
        export(&p, &d, profile).unwrap();
        let path = if profile == BundleProfile::Folder {
            PathBuf::from(&d).join("index.html")
        } else {
            PathBuf::from(&d)
        };
        let html = std::fs::read_to_string(&path).unwrap();
        // The sidecar's order is the document's.
        let ids = [
            "fig-mesh",
            "fig-clip",
            "tab-results",
            "fig-chart",
            "fig-demo",
        ];
        let at: Vec<usize> = ids
            .iter()
            .map(|i| {
                html.find(&format!("<figure id=\"{i}\" data-widget=\"{i}\""))
                    .unwrap()
            })
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{name}: document order");
        assert_eq!(html.matches("data-widget=").count(), ids.len());
        assert_eq!(html.matches("class=\"poster\"").count(), ids.len());
        let article = html.find("<article class=\"ltx_document\">").unwrap();
        assert!(article < at[0] && at[4] < html.find("</article>").unwrap());
        assert!(!html.contains("allow-same-origin"));
        assert_eq!(
            html.matches("setAttribute('sandbox', 'allow-scripts')")
                .count(),
            1
        );
        assert!(html.find("Content-Security-Policy").unwrap() < html.find("<title>").unwrap());
        // The only urls are the css and the widgets' own policies' origins.
        assert!(!outside.is_match(&html), "{name}: no external load");
    }
}

/// The fixture with its chart turned into a second html widget whose macro
/// options declare a frame origin, beside fig-demo's `widget.json` one.
fn two_embeds() -> Project {
    let sidecar = REAL_SIDECAR.replace(
        "widget|fig-chart|chart|chart@1|||house|figures/chart.png|spec=charts/ablation.vl.json|height=142.26378pt|",
        "widget|fig-chart|html||||house|figures/chart.png|bundle=widgets/other/|height=142.26378pt,framedomains=https://b.example.org|",
    );
    assert_ne!(sidecar, REAL_SIDECAR);
    let p = project("embeds", &sidecar);
    std::fs::write(
        p.root.join("widgets/demo/widget.json"),
        r#"{"csp":{"frameDomains":["https://a.example.org:8443"]}}"#,
    )
    .unwrap();
    let other = p.root.join("widgets/other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("index.html"), "<!doctype html><p>other</p>").unwrap();
    p
}

#[test]
fn declared_frame_origins_reach_only_their_own_widget_and_the_reader_union() {
    let p = two_embeds();
    let doc_policy = |html: &str| {
        regex::Regex::new(r#"<meta http-equiv="Content-Security-Policy" content="([^"]*)">"#)
            .unwrap()
            .captures(html)
            .unwrap()[1]
            .to_string()
    };
    // Folder: each widget's own document names its own origins and no other.
    let d = dest(&p, "folder");
    export(&p, &d, BundleProfile::Folder).unwrap();
    let doc = |id: &str| {
        std::fs::read_to_string(PathBuf::from(&d).join(format!("widgets/{id}/index.html"))).unwrap()
    };
    let demo = doc_policy(&doc("fig-demo"));
    let other = doc_policy(&doc("fig-chart"));
    assert!(
        demo.contains("frame-src https://a.example.org:8443;"),
        "{demo}"
    );
    assert!(!demo.contains("b.example.org"), "{demo}");
    assert!(
        other.contains("frame-src https://b.example.org;"),
        "{other}"
    );
    assert!(!other.contains("a.example.org"), "{other}");
    for id in ["fig-mesh", "tab-results", "fig-clip"] {
        let pol = doc_policy(&doc(id));
        assert!(
            !pol.contains("frame-src") && !pol.contains("example.org"),
            "{id}: {pol}"
        );
    }
    let m = manifest_of(Path::new(&d));
    let w = |id: &str| {
        m["widgets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(
        w("fig-demo")["csp"]["frameDomains"],
        json!(["https://a.example.org:8443"])
    );
    assert_eq!(
        w("fig-chart")["csp"]["frameDomains"],
        json!(["https://b.example.org"])
    );
    assert!(
        w("fig-chart")
            .get("options")
            .and_then(|o| o.get("framedomains"))
            .is_none(),
        "origins are the csp, not a runtime option"
    );
    assert!(w("fig-mesh").get("csp").is_none());
    let folder_reader = std::fs::read_to_string(PathBuf::from(&d).join("index.html")).unwrap();
    assert_eq!(doc_policy(&folder_reader), fold::FOLDER_READER_POLICY);

    // Single-file: widgets are srcdoc documents under the reader's policy,
    // whose frame-src is exactly the union.
    let s = dest(&p, "one.html");
    export(&p, &s, BundleProfile::SingleFile).unwrap();
    let html = std::fs::read_to_string(&s).unwrap();
    assert_eq!(
        doc_policy(&html),
        format!(
            "{}; frame-src https://a.example.org:8443 https://b.example.org",
            fold::SINGLE_FILE_READER_POLICY
        )
    );
    let docs = island_json(&html, "mfw-widgets");
    let demo = doc_policy(docs["fig-demo"].as_str().unwrap());
    let other = doc_policy(docs["fig-chart"].as_str().unwrap());
    assert!(
        demo.contains("frame-src https://a.example.org:8443;") && !demo.contains("b.example.org")
    );
    assert!(other.contains("frame-src https://b.example.org;") && !other.contains("a.example.org"));
}

#[test]
fn a_bundle_without_declared_frames_keeps_the_strict_single_file_reader() {
    let p = project("noframes", REAL_SIDECAR);
    let s = dest(&p, "one.html");
    export(&p, &s, BundleProfile::SingleFile).unwrap();
    let html = std::fs::read_to_string(&s).unwrap();
    assert!(html.contains(&format!("content=\"{}\"", fold::SINGLE_FILE_READER_POLICY)));
    assert!(!fold::policy_of(&html).contains("frame-src"));
}

// ---- the article --------------------------------------------------------

fn reader_html(d: &str, profile: BundleProfile) -> String {
    let path = if profile == BundleProfile::SingleFile {
        PathBuf::from(d)
    } else {
        PathBuf::from(d).join("index.html")
    };
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn the_reader_is_the_article_and_the_pdf_a_download() {
    let p = project("article", REAL_SIDECAR);
    let figure = regex::Regex::new(r#"<img src="([^"]+)" id="S1\.F3\.g1""#).unwrap();
    for (name, profile) in [
        ("one.html", BundleProfile::SingleFile),
        ("folder", BundleProfile::Folder),
    ] {
        let d = dest(&p, name);
        let r = export(&p, &d, profile).unwrap();
        let html = reader_html(&d, profile);
        for want in [
            ">Interactive Fixture</h1>",
            "Ada Lovelace",
            "class=\"ltx_abstract\"",
            "<nav class=\"m-contents\"",
            "<a href=\"#bib.bib1\"",
            "<math ",
            "id=\"pdf-link\"",
        ] {
            assert!(html.contains(want), "{name}: {want}");
        }
        assert!(
            !html.contains("<object") && !html.contains("id=\"widgets\""),
            "{name}"
        );
        assert!(
            r.warnings
                .iter()
                .all(|w| w.kind == BundleWarningKind::Metadata),
            "{name}: {:?}",
            r.warnings
        );
        let src = figure.captures(&html).expect("the plain figure")[1].to_string();
        if profile == BundleProfile::SingleFile {
            assert!(src.starts_with("data:image/png;base64,"), "{name}");
        } else {
            assert!(src.starts_with("figures/"), "{name}: {src}");
            assert_eq!(
                std::fs::read(PathBuf::from(&d).join(&src)).unwrap(),
                std::fs::read(p.root.join("figures/mesh.png")).unwrap()
            );
            assert!(PathBuf::from(&d).join("paper.pdf").is_file());
        }
    }
    // The conversion's scratch folder is gone, and nothing was written into
    // the project.
    let o = crate::outputs::outputs_of(&p.cx, &p.id, "main.tex").unwrap();
    assert!(!std::fs::read_dir(&o.outdir)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with("reader-export")));
    assert!(!files_under(&p.root)
        .iter()
        .any(|f| f.ends_with(".html") && !f.starts_with("widgets/")));
}

#[test]
fn conversion_problems_are_reported_and_never_block_the_article() {
    let p = project("problems", REAL_SIDECAR);
    converts_to(
        &p,
        Ok(CONVERTED.to_string()),
        &[
            "Error:undefined:\\undefinedmacro The token \\undefinedmacro is not defined",
            "Error:a:2",
            "Error:a:3",
            "Error:a:4",
            "Error:a:5",
            "Error:a:6",
        ],
    );
    let d = dest(&p, "one.html");
    let r = export(&p, &d, BundleProfile::SingleFile).unwrap();
    let ks = kinds(&r);
    assert_eq!(
        ks.iter()
            .filter(|k| **k == BundleWarningKind::Conversion)
            .count(),
        2,
        "{:?}",
        r.warnings
    );
    assert!(ks.contains(&BundleWarningKind::Figure));
    let log = r
        .warnings
        .iter()
        .find(|w| w.message.contains("reported 6 errors"))
        .unwrap();
    assert!(
        log.message.contains("\\undefinedmacro") && log.message.ends_with("and 1 more"),
        "{}",
        log.message
    );
    let fig = r
        .warnings
        .iter()
        .find(|w| w.kind == BundleWarningKind::Figure)
        .unwrap();
    assert!(fig.message.contains("figures/absent.png") && fig.message.contains("not found"));
    let html = reader_html(&d, BundleProfile::SingleFile);
    assert!(html.contains("ltx_missing_figure") && html.contains("\\undefinedmacro"));
    let article = &html[html.find("<article").unwrap()..html.find("</article>").unwrap()];
    for gone in [
        "onclick",
        "onerror",
        "javascript:",
        "alert(",
        "<script",
        "mfw-manifest",
    ] {
        assert!(!article.contains(gone), "{gone} reached the page");
    }
    // The event says the export warned.
    assert_eq!(
        event(
            "main.tex",
            BundleProfile::SingleFile,
            &Ok(r),
            maleficium_events::Actor::User
        )
        .kind,
        maleficium_events::EventKind::Warn
    );
}

#[test]
fn a_join_error_is_reported_and_the_widgets_still_mount() {
    let p = project("join", REAL_SIDECAR);
    let four = clean_conversion().replacen(
        "<span class=\"ltx_text m-widget m-widget-table\"></span>",
        "",
        1,
    );
    converts_to(&p, Ok(four), &[]);
    let d = dest(&p, "one.html");
    let r = export(&p, &d, BundleProfile::SingleFile).unwrap();
    let join = r
        .warnings
        .iter()
        .find(|w| w.kind == BundleWarningKind::WidgetJoin)
        .expect("a join warning");
    assert!(
        join.message.contains("4 widget placeholders") && join.message.contains("records 5"),
        "{}",
        join.message
    );
    let html = reader_html(&d, BundleProfile::SingleFile);
    let unplaced = html.find("<section class=\"m-unplaced\">").unwrap();
    assert_eq!(html[unplaced..].matches("data-widget=").count(), 5);
    assert_eq!(html.matches("data-widget=").count(), 5);
}

#[test]
fn a_conversion_with_no_html_refuses_the_export_and_writes_nothing() {
    let p = project("noconvert", REAL_SIDECAR);
    converts_to(
        &p,
        Err("the converter did not start: no such file".into()),
        &[],
    );
    for (name, profile) in [
        ("one.html", BundleProfile::SingleFile),
        ("folder", BundleProfile::Folder),
    ] {
        let d = dest(&p, name);
        let e = export(&p, &d, profile).unwrap_err();
        assert!(
            e.contains("could not be converted to HTML") && e.contains("no such file"),
            "{e}"
        );
        assert!(!PathBuf::from(&d).exists());
    }
    assert_eq!(std::fs::read_dir(&p.out).unwrap().count(), 0);
}

#[test]
fn graphicspath_entries_are_read_in_order() {
    assert_eq!(
        graphics_paths("\\graphicspath{{figs/}{./img/} }\n% \\graphicspath{{no/}}"),
        ["figs/", "./img/"]
    );
    assert!(graphics_paths("\\documentclass{article}").is_empty());
}

/// The real engine on the real fixture: runs only with MALEFICIUM_ENGINE_E2E=1
/// and a bundled engine, since it needs the engine binary, the format dumps
/// and a TeX cache the fixture's compile filled.
#[test]
fn the_engine_converts_the_fixture_end_to_end() {
    if std::env::var_os("MALEFICIUM_ENGINE_E2E").is_none()
        || crate::sidecar_path_for("maleficium-engine").is_err()
    {
        return;
    }
    let p = project("engine-e2e", REAL_SIDECAR);
    p.cx.set_converter(std::sync::Arc::new(reflow::convert::Engine));
    let d = dest(&p, "one.html");
    let r = export(&p, &d, BundleProfile::SingleFile).unwrap();
    let html = reader_html(&d, BundleProfile::SingleFile);
    assert!(
        html.contains("<article class=\"ltx_document\">"),
        "{:?}",
        r.warnings
    );
    assert_eq!(html.matches("data-widget=").count(), 5, "{:?}", r.warnings);
    assert!(
        !kinds(&r).contains(&BundleWarningKind::WidgetJoin),
        "{:?}",
        r.warnings
    );
}

/// A one-page pdf with nothing on it and no widget annotations.
fn blank_pdf() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut at = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        at.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
    for a in at {
        out.extend(format!("{a:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .bytes(),
    );
    out
}

/// A dry run on a real paper, by hand: compiles a project with the real
/// engine, converts it and exports both a single-file and a folder bundle for
/// a person (or a headless browser) to open. Runs only with
/// `MALEFICIUM_DRYRUN="<project dir>|<main.tex relative>|<output dir>"`; the
/// project is copied first (the interactive package installed into the
/// copy) and nothing is written inside it.
#[test]
fn dry_run_exports_a_real_paper() {
    let Ok(spec) = std::env::var("MALEFICIUM_DRYRUN") else {
        return;
    };
    let parts: Vec<&str> = spec.split('|').collect();
    assert_eq!(parts.len(), 3, "project dir|main rel|output dir");
    let cx = Core::default();
    let scratch = crate::test_scratch::dir("bundle-dry-run");
    let _ = std::fs::remove_dir_all(&scratch);
    let proj = scratch.join("proj");
    copy_dir(Path::new(parts[0]), &proj);
    let root = dunce::canonicalize(&proj).unwrap();
    crate::fs::grant_root(&cx, "dry-run", &root.to_string_lossy()).unwrap();
    crate::interactive::install_checked(&cx, "dry-run", false).unwrap();
    // A paper that does not compile (it has no pdf, so the export has no
    // version of record) still converts: a stand-in pdf is put where the
    // compile would have left one.
    let compiled = crate::compile::run_blocking(
        &cx,
        "dry-run",
        parts[1],
        false,
        &mut |l: &maleficium_events::CompileLine| eprintln!("compile: {}", l.text),
    );
    let o = crate::outputs::outputs_of(&cx, "dry-run", parts[1]).unwrap();
    if !o.outdir.join(&o.pdf_name).is_file() {
        eprintln!("no pdf ({compiled:?}); exporting with a stand-in");
        std::fs::create_dir_all(&o.outdir).unwrap();
        std::fs::write(o.outdir.join(&o.pdf_name), blank_pdf()).unwrap();
    }
    cx.set_converter(std::sync::Arc::new(reflow::convert::Engine));
    let out = PathBuf::from(parts[2]);
    std::fs::create_dir_all(&out).unwrap();
    for (profile, name) in [
        (BundleProfile::SingleFile, "single.html"),
        (BundleProfile::Folder, "folder"),
    ] {
        let dest = out.join(name);
        let _ = std::fs::remove_dir_all(&dest);
        let _ = std::fs::remove_file(&dest);
        let r = export_bundle(
            &cx,
            "dry-run",
            parts[1],
            &dest.to_string_lossy(),
            profile,
            None,
        )
        .unwrap();
        eprintln!("exported {name}: {} bytes, {} widgets", r.bytes, r.widgets);
        for w in &r.warnings {
            eprintln!("warning {:?}: {}", w.kind, w.message);
        }
    }
}
