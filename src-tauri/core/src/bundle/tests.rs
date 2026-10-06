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

/// A converter that hands back fixed HTML, log errors and the log, or fails.
struct Fixed {
    html: Result<String, String>,
    errors: Vec<String>,
    log: String,
}

impl reflow::convert::Converter for Fixed {
    fn convert(&self, _cx: &Core, main: &Path, work: &Path) -> reflow::convert::Conversion {
        assert!(main.is_absolute() && main.is_file(), "{}", main.display());
        assert!(work.is_dir(), "an empty scratch folder");
        reflow::convert::Conversion {
            html: self.html.clone(),
            errors: self.errors.clone(),
            log: self.log.clone(),
        }
    }
}

fn converts_to(p: &Project, html: Result<String, String>, errors: &[&str]) {
    p.cx.set_converter(std::sync::Arc::new(Fixed {
        html,
        errors: errors.iter().map(|e| e.to_string()).collect(),
        log: String::new(),
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
    // Islands: manifest, widgets, assets, theme; then the script.
    assert_eq!(html.matches("</script>").count(), 5);
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
        json!(["model", "video", "table", "chart", "html", "custom"])
    );
    assert_eq!(
        s["properties"]["paper"]["properties"]["reader"]["properties"]["measure"]["enum"],
        json!(["narrow", "default", "wide"])
    );
    assert_eq!(
        s["properties"]["paper"]["properties"]["reader"]["properties"]["contents"]["type"],
        "boolean"
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

// ---- reader flags -------------------------------------------------------

/// The clean conversion with flag markers as the Rhai binding emits them,
// plus one hostile token.
fn flagged_conversion() -> String {
    let html = clean_conversion();
    let markers = concat!(
        "<span class=\"ltx_text m-flag m-flag-contents-on\"></span>",
        "<span class=\"ltx_text m-flag m-flag-measure-narrow\"></span>",
        "<span class=\"ltx_text m-flag m-flag-contents-off\"></span>",
        "<span class=\"ltx_text m-flag m-flag-measure-wide\"></span>",
        "<span class=\"ltx_text m-flag m-flag-contents-sideways\"></span>",
    );
    html.replacen("</article>", &format!("{markers}</article>"), 1)
}

#[test]
fn reader_flags_resolve_to_the_manifest_and_shape_the_page() {
    let p = project("flags", REAL_SIDECAR);
    converts_to(&p, Ok(flagged_conversion()), &[]);
    for (name, profile) in [
        ("one.html", BundleProfile::SingleFile),
        ("folder", BundleProfile::Folder),
    ] {
        let d = dest(&p, name);
        let r = export(&p, &d, profile).unwrap();
        // Last valid marker of each kind wins; the hostile token falls back.
        if profile == BundleProfile::Folder {
            let m = manifest_of(Path::new(&d));
            assert_eq!(
                m["paper"]["reader"],
                json!({"contents": false, "measure": "wide"})
            );
        }
        let html = reader_html(&d, profile);
        assert!(!html.contains("m-flag"), "{name}: markers are stripped");
        assert!(
            !html.contains("<nav class=\"m-contents\""),
            "{name}: contents:false omits the nav"
        );
        assert!(
            html.contains("<body class=\"m-reader\" data-measure=\"wide\">"),
            "{name}: the wide token overrides the column"
        );
        let flag = r
            .warnings
            .iter()
            .find(|w| w.message.contains("m-flag-contents-sideways"))
            .unwrap();
        assert_eq!(flag.kind, BundleWarningKind::Conversion, "{name}");
        assert!(flag.message.contains("default"), "{name}: {}", flag.message);
    }
}

#[test]
fn reader_flags_default_to_contents_with_the_theme_measure() {
    let p = project("flag-defaults", REAL_SIDECAR);
    let d = dest(&p, "folder");
    export(&p, &d, BundleProfile::Folder).unwrap();
    let m = manifest_of(Path::new(&d));
    assert_eq!(
        m["paper"]["reader"],
        json!({"contents": true, "measure": "default"})
    );
    let html = reader_html(&d, BundleProfile::Folder);
    assert!(
        html.contains("<nav class=\"m-contents\""),
        "nav still emitted"
    );
    assert!(
        !html.contains("data-measure=\""),
        "default is the theme's 68ch"
    );
}

#[test]
fn the_reader_block_enforces_its_closed_shapes() {
    let good = good_manifest();
    assert_eq!(
        good["paper"]["reader"],
        json!({"contents": true, "measure": "default"})
    );
    validate_manifest(&good).unwrap();
    // A bundle from before flags existed still validates.
    let mut old = good.clone();
    old["paper"].as_object_mut().unwrap().remove("reader");
    validate_manifest(&old).unwrap();
    let bad = |f: &dyn Fn(&mut Value)| -> String {
        let mut m = good.clone();
        f(&mut m);
        validate_manifest(&m).unwrap_err()
    };
    assert!(bad(&|m| m["paper"]["reader"] = "wide".into()).contains("not an object"));
    assert!(bad(&|m| m["paper"]["reader"]["contents"] = 1.into()).contains("not a boolean"));
    assert!(
        bad(&|m| m["paper"]["reader"]["contents"] = "false".into()).contains("not a boolean"),
        "no string coerces to a boolean"
    );
    assert!(
        bad(&|m| m["paper"]["reader"]["measure"] = "huge".into())
            .contains("not narrow, default or wide"),
        "no open width reaches CSS"
    );
    assert!(bad(&|m| m["paper"]["reader"]["extra"] = true.into()).contains("schema"));
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
    // The undefined-macro detail line is not quoted with the other errors:
    // its macro gets its own warning with the package hint.
    let log = r
        .warnings
        .iter()
        .find(|w| w.message.contains("reported 5 errors"))
        .unwrap();
    assert!(
        !log.message.contains("\\undefinedmacro") && !log.message.contains("more"),
        "{}",
        log.message
    );
    let undef = r
        .warnings
        .iter()
        .find(|w| w.message.contains("\\undefinedmacro"))
        .unwrap();
    assert!(
        undef
            .message
            .contains("1 spot in the article shows raw TeX")
            && undef.message.contains("add it to your project folder"),
        "{}",
        undef.message
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
fn undefined_macros_export_grouped_with_the_package_hint() {
    // The origin paper's shape (e2e/fixtures/vendored/
    // on-the-origin-of-objects): its conversion names one undefined macro
    // and its article shows that macro's raw TeX throughout.
    let p = project("grouped", REAL_SIDECAR);
    let span = "<span class=\"ltx_ERROR undefined\">\\undefinedmacro</span>";
    let html = CONVERTED.replacen(
        span,
        "<span class=\"ltx_ERROR undefined\">\\eolang</span> \
         <span class=\"ltx_ERROR undefined\">\\eolang</span> \
         <span class=\"ltx_ERROR undefined\">\\eolang</span>",
        1,
    );
    assert_ne!(html, CONVERTED, "the fixture span changed");
    p.cx.set_converter(std::sync::Arc::new(Fixed {
        html: Ok(html),
        errors: Vec::new(),
        log: String::from(
            "3 warnings; 1 error; 1 undefined macro[\\eolang]\nConversion complete: 3 warnings; 1 error; 1 undefined macro[\\eolang]\n",
        ),
    }));
    let d = dest(&p, "grouped.html");
    let r = export(&p, &d, BundleProfile::SingleFile).unwrap();
    let undef: Vec<&BundleWarning> = r
        .warnings
        .iter()
        .filter(|w| w.message.contains("\\eolang"))
        .collect();
    assert_eq!(undef.len(), 1, "{:?}", r.warnings);
    assert!(
        undef[0]
            .message
            .contains("3 spots in the article show raw TeX")
            && undef[0].message.contains("add it to your project folder"),
        "{}",
        undef[0].message
    );
    let html = reader_html(&d, BundleProfile::SingleFile);
    assert!(html.contains("\\eolang"), "the article is still produced");
}

#[test]
fn missing_bundle_files_and_shell_escape_report_by_name() {
    let log = "1 warning; 2 missing files[eolang.sty, minted.sty]\nConversion complete: 1 warning; 2 missing files[eolang.sty, minted.sty]\n";
    let warnings = article_warnings(log, &[], &[]);
    let missing = warnings
        .iter()
        .find(|w| w.message.contains("eolang.sty"))
        .expect("a missing-file warning");
    assert_eq!(
        missing.message,
        super::super::compile::missing_package_text("eolang.sty")
    );
    let shell = warnings
        .iter()
        .find(|w| w.message.contains("minted"))
        .expect("a shell-escape warning");
    assert!(
        shell.message.contains("needs shell escape") && shell.message.contains("still produced"),
        "{}",
        shell.message
    );
    // The compile's own shell-escape signature reports the package too.
    let tex_log = "error: main.tex:3: Package minted Error: You must invoke LaTeX with the -shell-escape flag.\n";
    let warnings = article_warnings(tex_log, &[], &[]);
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("minted needs shell escape")),
        "{warnings:?}"
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
/// project is copied first (the interactive package copied into the
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
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../interactive/maleficium-interactive.sty"),
        root.join("maleficium-interactive.sty"),
    )
    .unwrap();
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

const CREAM: &str = include_str!("../../testdata/theme/cream-times.mfw");

/// The real sidecar with the theme a cream page in Times records.
fn themed_sidecar() -> String {
    format!("{REAL_SIDECAR}{}", CREAM.strip_prefix("mfw 1\n").unwrap())
}

#[test]
fn the_page_css_and_every_widgets_tokens_come_from_the_sidecars_theme() {
    let p = project("themed", &themed_sidecar());
    let theme = crate::widgets::parse_sidecar(CREAM).unwrap().theme.unwrap();
    let d = dest(&p, "paper.html");
    export(&p, &d, BundleProfile::SingleFile).unwrap();
    let html = std::fs::read_to_string(&d).unwrap();
    assert!(html.contains(&theme.css().unwrap()));
    assert!(html.contains("--m-figure-bg:#FFF8E7;"));
    assert!(html.contains("\"TeX Gyre Termes\""));
    assert_eq!(island_json(&html, "mfw-theme"), theme.json());
    assert_eq!(
        island_json(&html, "mfw-theme")["dark"]["--m-figure-bg"],
        "#1C1F25"
    );

    let d = dest(&p, "paper");
    export(&p, &d, BundleProfile::Folder).unwrap();
    let css = std::fs::read_to_string(PathBuf::from(&d).join("theme/theme.css")).unwrap();
    assert_eq!(css, theme.css().unwrap());
    let index = std::fs::read_to_string(PathBuf::from(&d).join("index.html")).unwrap();
    assert_eq!(island_json(&index, "mfw-theme"), theme.json());
}

#[test]
fn a_sidecar_without_a_theme_record_exports_in_the_house_theme() {
    let p = project("housetheme", REAL_SIDECAR);
    let d = dest(&p, "paper.html");
    export(&p, &d, BundleProfile::SingleFile).unwrap();
    let html = std::fs::read_to_string(&d).unwrap();
    let house = crate::theme::Theme::house();
    assert!(html.contains(&house.css().unwrap()));
    assert_eq!(island_json(&html, "mfw-theme"), house.json());
}

#[test]
fn a_hostile_theme_value_refuses_the_export() {
    let bad = themed_sidecar().replace(
        "theme|light|--m-figure-bg|#FFF8E7",
        "theme|light|--m-figure-bg|#fff}</style><script>alert(1)</script>",
    );
    let p = project("hostiletheme", &bad);
    let d = dest(&p, "paper.html");
    let e = export(&p, &d, BundleProfile::SingleFile).unwrap_err();
    assert!(e.contains("--m-figure-bg"), "{e}");
    assert!(!PathBuf::from(&d).exists());
}

// ---- custom runtimes ------------------------------------------------------

const CHART_LINE: &str = "widget|fig-chart|chart|chart@1|||house|figures/chart.png|spec=charts/ablation.vl.json|height=142.26378pt|Ablation chart";
const DEMO_LINE: &str = "widget|fig-demo|html||||house|figures/demo.png|bundle=widgets/demo/|height=227.62204pt|Live demo widget";
/// fig-chart runs the documented heatmap sample (installed), fig-demo the
/// stl viewer (not installed).
const HEAT_LINE: &str = "widget|fig-chart|custom|heatmap@1|||house|figures/chart.png|primary=data/grid.csv|height=142.26378pt,scheme=div|Ablation chart";
const STL_LINE: &str = "widget|fig-demo|custom|stl-viewer@1|||house|figures/demo.png|primary=models/mesh.stl|height=227.62204pt,autorotate=true|Live demo widget";
const GRID: &str = "0,1,2,3,4,5\n1,2,3,4,5,4\n2,3,4,5,4,3\n3,4,5,4,3,2\n4,5,4,3,2,1\n";

struct Custom {
    p: Project,
    /// The approval store home (stands in for app data).
    base: PathBuf,
}

fn copy_sample(r: &str, to: &Path) {
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/runtimes/samples")
            .join(r),
        to,
    );
}

fn custom_project(name: &str, heat: &str) -> Custom {
    let sidecar = REAL_SIDECAR
        .replace(CHART_LINE, heat)
        .replace(DEMO_LINE, STL_LINE);
    assert!(sidecar.contains("|custom|heatmap@1|") && sidecar.contains("|custom|stl-viewer@1|"));
    let p = project(name, &sidecar);
    std::fs::write(p.root.join("data/grid.csv"), GRID).unwrap();
    copy_sample("heatmap@1", &p.root.join("runtimes/heatmap@1"));
    converts_to(
        &p,
        Ok(clean_conversion()
            .replace("m-widget m-widget-chart", "m-widget m-widget-custom")
            .replace("m-widget m-widget-html", "m-widget m-widget-custom")),
        &[],
    );
    let base = crate::test_scratch::dir(&format!("bundle-{name}-appdata"));
    let _ = std::fs::remove_dir_all(&base);
    Custom { p, base }
}

impl Custom {
    fn export(&self, d: &str, profile: BundleProfile) -> Result<BundleExported, String> {
        export_bundle_at(&self.base, &self.p.cx, &self.p.id, "main.tex", d, profile)
    }
    fn check(&self) -> crate::widget_approval::RuntimeChecked {
        crate::widget_approval::check_runtime_at(
            &self.base,
            &self.p.cx,
            &self.p.id,
            "heatmap@1",
            &["fig-chart".into()],
        )
        .unwrap()
    }
    fn decide(&self, decision: crate::widget_approval::RuntimeDecision) {
        let digest = self.check().snapshot.digest;
        crate::widget_approval::decide_runtime_at(
            &self.base,
            &self.p.cx,
            &crate::widget_approval::RuntimeDecisionParams {
                root_id: self.p.id.clone(),
                main_rel: "main.tex".into(),
                runtime: "heatmap@1".into(),
                digest,
                decision,
            },
        )
        .unwrap();
    }
    fn allow(&self) {
        self.decide(crate::widget_approval::RuntimeDecision::Allowed);
    }
    fn rt(&self) -> PathBuf {
        self.p.root.join("runtimes/heatmap@1")
    }
}

fn widget_json<'a>(m: &'a Value, id: &str) -> &'a Value {
    m["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == id)
        .unwrap()
}

fn runtime_warnings(r: &BundleExported) -> Vec<&str> {
    r.warnings
        .iter()
        .filter(|w| w.kind == BundleWarningKind::Runtime)
        .map(|w| w.message.as_str())
        .collect()
}

const STL_MISSING: &str = "runtime stl-viewer@1 is not installed in this project (no runtimes/stl-viewer@1/runtime.json): fig-demo export as posters only.";

#[test]
fn an_approved_custom_runtime_exports_live_and_a_missing_one_as_its_poster() {
    let c = custom_project("rt-live", HEAT_LINE);
    c.allow();
    let d = dest(&c.p, "paper");
    let r = c.export(&d, BundleProfile::Folder).unwrap();
    let folder = PathBuf::from(&d);
    let m = manifest_of(&folder);
    validate_manifest(&m).unwrap();

    // Live: runtime ref, entry, role-keyed sources, typed and defaulted
    // options, no csp, no fallback.
    let heat = widget_json(&m, "fig-chart");
    assert_eq!(heat["type"], "custom");
    assert_eq!(heat["runtime"], "heatmap@1");
    assert_eq!(heat["entry"], "widgets/fig-chart/index.html");
    assert_eq!(heat["sources"], json!({"data": "fig-chart-data"}));
    assert_eq!(heat["options"], json!({"scheme": "div"}));
    assert!(heat.get("csp").is_none() && heat.get("fallback").is_none());
    let a = &m["assets"]["fig-chart-data"];
    assert_eq!(
        (a["mime"].as_str(), a["source"].as_str()),
        (Some("text/csv"), Some("data/grid.csv"))
    );
    assert_eq!(a["sha256"], sha_of(GRID.as_bytes()));

    // The runtimes entry carries the digest the approval judged.
    let judged = c.check().snapshot.digest;
    assert_eq!(
        m["runtimes"],
        json!({"heatmap@1": {"name": "heatmap", "version": "1.0.0", "digest": judged,
            "license": "MIT", "capabilities": {"webgl": false}, "vendored": []}})
    );

    // The folded document: policy first, the package's own page inlined,
    // its metadata left out, the licence block last.
    let doc = std::fs::read_to_string(folder.join("widgets/fig-chart/index.html")).unwrap();
    let at = doc
        .find("<meta http-equiv=\"Content-Security-Policy\"")
        .unwrap();
    assert!(!doc[..at].contains("<script") && !doc[..at].contains("<meta charset"));
    assert_eq!(fold::policy_of(&doc), fold::widget_policy(None));
    let sample = std::fs::read_to_string(c.rt().join("index.html")).unwrap();
    let probe = sample
        .lines()
        .find(|l| l.contains("addEventListener"))
        .unwrap()
        .trim();
    assert!(doc.contains(probe), "the package's page is the document");
    let licence = std::fs::read_to_string(c.rt().join("LICENSE")).unwrap();
    assert!(doc.trim_end().ends_with(" -->"));
    let block = &doc[doc.find("<!-- Licences:").unwrap()..];
    assert!(block.starts_with("<!-- Licences:\nheatmap 1.0.0 (MIT)\n"));
    assert!(block.contains(licence.lines().next().unwrap()));
    assert!(!block.contains("data.csv"), "samples are not folded");

    // Fallback: no document, no source asset, the reason, the note, the warning.
    let stl = widget_json(&m, "fig-demo");
    assert_eq!(stl["fallback"], "runtime-missing");
    assert_eq!(stl["sources"], json!({}));
    for gone in ["entry", "options", "csp"] {
        assert!(stl.get(gone).is_none(), "{gone}");
    }
    assert!(!folder.join("widgets/fig-demo").exists());
    assert!(m["assets"].get("fig-demo-primary").is_none());
    assert!(m["assets"].get("fig-demo-poster").is_some());
    assert_eq!(runtime_warnings(&r), [STL_MISSING]);
    let index = reader_html(&d, BundleProfile::Folder);
    assert!(index.contains(
        "Interactive version not included in this copy: runtime stl-viewer@1 is not installed."
    ));
}

#[test]
fn the_licence_block_cannot_close_its_comment() {
    let c = custom_project("rt-licence", HEAT_LINE);
    std::fs::write(c.rt().join("LICENSE"), "MIT -- see --> and <!-- and ---x-").unwrap();
    c.allow();
    let d = dest(&c.p, "paper");
    c.export(&d, BundleProfile::Folder).unwrap();
    let doc =
        std::fs::read_to_string(PathBuf::from(&d).join("widgets/fig-chart/index.html")).unwrap();
    let block = &doc[doc.find("<!-- Licences:").unwrap() + 4..];
    let inner = block
        .strip_suffix(" -->")
        .unwrap_or(block.trim_end().strip_suffix(" -->").unwrap());
    assert!(!inner.contains("--"), "{inner}");
    assert!(
        inner.contains("MIT - - see - -> and <!- - and - - -x-"),
        "{inner}"
    );
}

#[test]
fn an_unapproved_denied_or_invalid_runtime_exports_as_its_poster() {
    let c = custom_project("rt-gate", HEAT_LINE);
    let run = |name: &str| {
        let d = dest(&c.p, name);
        let r = c.export(&d, BundleProfile::Folder).unwrap();
        let m = manifest_of(Path::new(&d));
        validate_manifest(&m).unwrap();
        assert!(
            m.get("runtimes").is_none(),
            "no live widget, no runtimes map"
        );
        assert!(!PathBuf::from(&d).join("widgets/fig-chart").exists());
        assert!(m["assets"].get("fig-chart-data").is_none());
        let index = reader_html(&d, BundleProfile::Folder);
        (
            widget_json(&m, "fig-chart")["fallback"].clone(),
            runtime_warnings(&r)
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            index,
        )
    };
    let (why, warns, index) = run("unapproved");
    assert_eq!(why, "unapproved");
    assert_eq!(warns, [
        "runtime heatmap@1 is not approved in this project: fig-chart export as posters only. Approve it in View > Widgets and export again.",
        STL_MISSING,
    ]);
    assert!(index.contains("Interactive version not included in this copy: runtime heatmap@1 was not approved by the author."));

    c.decide(crate::widget_approval::RuntimeDecision::Denied);
    let (why, warns, _) = run("denied");
    assert_eq!(why, "denied");
    assert_eq!(warns[0], "runtime heatmap@1 is denied in this project: fig-chart export as posters only. Allow it in View > Widgets to export it live.");

    // Allowed, then changed with auto off: unapproved again.
    c.allow();
    std::fs::write(
        c.rt().join("index.html"),
        std::fs::read_to_string(c.rt().join("index.html")).unwrap() + "<!-- edit -->",
    )
    .unwrap();
    assert_eq!(run("changed").0, "unapproved");

    let m = c.rt().join("runtime.json");
    let text = std::fs::read_to_string(&m)
        .unwrap()
        .replace("\"MIT\"", "\"GPL-3.0\"");
    std::fs::write(&m, text).unwrap();
    let (why, warns, index) = run("invalid");
    assert_eq!(why, "runtime-invalid");
    assert!(
        warns[0]
            .starts_with("runtime heatmap@1 is invalid (license `GPL-3.0` is not on the allowlist"),
        "{}",
        warns[0]
    );
    assert!(
        warns[0].ends_with("): fig-chart export as posters only."),
        "{}",
        warns[0]
    );
    assert!(index.contains("runtime heatmap@1 did not pass validation."));
}

#[test]
fn a_bind_error_refuses_the_export_approved_or_not() {
    let c = custom_project("rt-bind", &HEAT_LINE.replace("scheme=div", "scheme=hot"));
    let d = dest(&c.p, "paper");
    for approve in [false, true] {
        if approve {
            c.allow();
        }
        let e = c.export(&d, BundleProfile::Folder).unwrap_err();
        assert_eq!(
            e,
            "widget fig-chart: option scheme=hot is not one of seq, div"
        );
        assert!(!PathBuf::from(&d).exists());
    }
    let c = custom_project(
        "rt-bind-ext",
        &HEAT_LINE.replace("data/grid.csv", "data/results.json"),
    );
    let e = c
        .export(&dest(&c.p, "paper"), BundleProfile::Folder)
        .unwrap_err();
    assert!(e.contains("source data: `data/results.json` has extension `.json`; runtime heatmap@1 accepts .csv"), "{e}");
    // A source over its role's cap refuses the live export.
    let c = custom_project("rt-bind-size", HEAT_LINE);
    let m = c.rt().join("runtime.json");
    let text = std::fs::read_to_string(&m).unwrap().replace(
        "\"extensions\": [\"csv\"]",
        "\"extensions\": [\"csv\"], \"maxBytes\": 8",
    );
    std::fs::write(&m, text).unwrap();
    c.allow();
    let e = c
        .export(&dest(&c.p, "paper"), BundleProfile::Folder)
        .unwrap_err();
    assert_eq!(
        e,
        format!(
            "widget fig-chart: source data is {} bytes; runtime heatmap@1 accepts at most 8",
            GRID.len()
        )
    );
}

#[test]
fn a_single_file_export_carries_only_live_documents() {
    let c = custom_project("rt-single", HEAT_LINE);
    c.allow();
    let d = dest(&c.p, "paper.html");
    c.export(&d, BundleProfile::SingleFile).unwrap();
    let html = std::fs::read_to_string(&d).unwrap();
    let m = island_json(&html, "mfw-manifest");
    validate_manifest(&m).unwrap();
    assert!(widget_json(&m, "fig-chart").get("entry").is_none());
    let docs = island_json(&html, "mfw-widgets");
    assert!(docs["fig-chart"]
        .as_str()
        .unwrap()
        .contains("<!-- Licences:"));
    assert!(docs.get("fig-demo").is_none(), "a fallback has no document");
    assert!(m["runtimes"]["heatmap@1"].is_object());
}

#[test]
fn the_preview_runs_custom_runtimes_without_the_approval_gate() {
    let c = custom_project("rt-preview", HEAT_LINE);
    let base = c.p.out.join("previews");
    // No decision at all: the export gates it, the preview does not.
    let r = preview_bundle_in(&c.p.cx, &c.p.id, "main.tex", &base).unwrap();
    let html = std::fs::read_to_string(&r.path).unwrap();
    let m = island_json(&html, "mfw-manifest");
    assert!(widget_json(&m, "fig-chart").get("fallback").is_none());
    assert!(island_json(&html, "mfw-widgets")["fig-chart"].is_string());
    assert_eq!(runtime_warnings(&r), [STL_MISSING], "missing stays missing");
    // Invalid stays invalid in the preview too.
    let mf = c.rt().join("runtime.json");
    std::fs::write(&mf, "{").unwrap();
    let r = preview_bundle_in(&c.p.cx, &c.p.id, "main.tex", &base).unwrap();
    let html = std::fs::read_to_string(&r.path).unwrap();
    let m = island_json(&html, "mfw-manifest");
    assert_eq!(widget_json(&m, "fig-chart")["fallback"], "runtime-invalid");
}

#[test]
fn option_values_reach_the_runtime_as_json_and_never_as_markup() {
    let hostile = "</script><img src=x onerror=alert(1)>";
    let c = custom_project(
        "rt-hostile-option",
        &HEAT_LINE.replace("scheme=div", &format!("scheme=div,caption={hostile}")),
    );
    let m = c.rt().join("runtime.json");
    let text = std::fs::read_to_string(&m).unwrap().replace(
        "\"properties\": {",
        "\"properties\": {\n      \"caption\": { \"type\": \"string\" },",
    );
    std::fs::write(&m, text).unwrap();
    c.allow();
    let d = dest(&c.p, "paper.html");
    c.export(&d, BundleProfile::SingleFile).unwrap();
    let html = std::fs::read_to_string(&d).unwrap();
    assert!(
        !html.contains("<img src=x"),
        "the value never lands as markup"
    );
    let m = island_json(&html, "mfw-manifest");
    assert_eq!(widget_json(&m, "fig-chart")["options"]["caption"], hostile);
}

#[test]
fn the_manifest_invariants_for_custom_runtimes_hold_and_their_breaks_are_named() {
    let c = custom_project("rt-invariants", HEAT_LINE);
    c.allow();
    let d = dest(&c.p, "paper");
    c.export(&d, BundleProfile::Folder).unwrap();
    let good = manifest_of(Path::new(&d));
    validate_manifest(&good).unwrap();
    let i = good["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .position(|w| w["id"] == "fig-chart")
        .unwrap();
    let j = good["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .position(|w| w["id"] == "fig-demo")
        .unwrap();
    let bad = |f: &dyn Fn(&mut Value)| -> String {
        let mut m = good.clone();
        f(&mut m);
        validate_manifest(&m).unwrap_err()
    };
    assert!(bad(&|m| {
        m.as_object_mut().unwrap().remove("runtimes");
    })
    .contains("which the manifest does not list"));
    assert!(
        bad(&|m| m["runtimes"]["other@1"] = m["runtimes"]["heatmap@1"].clone())
            .contains("no live widget runs it")
    );
    assert!(
        bad(&|m| m["runtimes"]["heatmap@1"]["version"] = "2.0.0".into())
            .contains("listed as heatmap@2")
    );
    assert!(bad(&|m| m["widgets"][i]["csp"] = json!({})).contains("schema"));
    assert!(
        bad(&|m| m["widgets"][j]["sources"] = json!({"model": "fig-chart-data"}))
            .contains("schema")
    );
    assert!(bad(&|m| m["widgets"][j]["fallback"] = "lost".into()).contains("schema"));
    assert!(bad(&|m| m["widgets"][i]["runtime"] = "Heat@1".into()).contains("schema"));
    assert!(
        bad(&|m| m["widgets"][0]["fallback"] = "denied".into()).contains("schema"),
        "built-ins never fall back"
    );
    assert!(bad(&|m| m["runtimes"]["heatmap@1"]["license"] = "GPL-3.0".into()).contains("schema"));
    assert!(bad(&|m| m["runtimes"]["heatmap@1"]["path"] = "runtimes/x".into()).contains("schema"));
}

/// Writes the single-file proof bundle (one approved custom runtime live,
/// one poster-only fallback) to `MALEFICIUM_RUNTIME_PROOF` when set, for the
/// screenshot harness; otherwise checks it like any export.
#[test]
fn a_proof_bundle_with_one_live_runtime_and_one_fallback() {
    let c = custom_project("rt-proof", HEAT_LINE);
    c.allow();
    let d = dest(&c.p, "proof.html");
    let r = c.export(&d, BundleProfile::SingleFile).unwrap();
    assert_eq!(runtime_warnings(&r), [STL_MISSING]);
    if let Some(to) = std::env::var_os("MALEFICIUM_RUNTIME_PROOF") {
        std::fs::copy(&d, to).unwrap();
    }
}

#[test]
fn custom_widgets_mount_in_the_article_and_a_fallback_carries_its_note() {
    let c = custom_project("rt-article", HEAT_LINE);
    c.allow();
    let d = dest(&c.p, "paper.html");
    c.export(&d, BundleProfile::SingleFile).unwrap();
    let html = std::fs::read_to_string(&d).unwrap();
    let main = &html[html.find("<main>").unwrap()..html.find("</main>").unwrap()];
    for id in ["fig-chart", "fig-demo"] {
        let unit = format!("<figure id=\"{id}\" data-widget=\"{id}\" data-type=\"custom\"");
        assert_eq!(main.matches(&unit).count(), 1, "{id} mounts once");
    }
    // The fallback's note sits after its caption, as text; the live one has none.
    let demo = &main[main.find("data-widget=\"fig-demo\"").unwrap()..];
    let demo = &demo[..demo.find("</figure>").unwrap()];
    assert!(demo.contains("</figcaption><p class=\"m-widget-note\">Interactive version not included in this copy: runtime stl-viewer@1 is not installed.</p>"), "{demo}");
    let chart = &main[main.find("data-widget=\"fig-chart\"").unwrap()..];
    let chart = &chart[..chart.find("</figure>").unwrap()];
    assert!(!chart.contains("m-widget-note"));
    // The ref never reaches a sanitized attribute.
    assert!(!main.contains("heatmap@1") && !main.contains("data-runtime"));
    // The reader knows the six kinds, needs a runtimes entry for a custom
    // widget, and guards every figure.
    let script = &html[html.find("</main>").unwrap()..];
    assert!(script.contains("var KINDS = ['model', 'video', 'table', 'chart', 'html', 'custom'];"));
    assert!(script.contains("manifest.runtimes || {}"));
    assert!(script.contains("Interactive version unavailable: this reader does not know runtime "));
}

// ---- in-app article ---------------------------------------------------------

/// A fresh approval store home (stands in for app data): cleared, so the
/// html widget starts unapproved.
fn fresh_approvals(name: &str) -> PathBuf {
    let base = crate::test_scratch::dir(&format!("bundle-{name}-appdata"));
    let _ = std::fs::remove_dir_all(&base);
    base
}

fn approve_demo(p: &Project, approvals: &Path) {
    let list = crate::widgets::widgets(&p.cx, &p.id, "main.tex").unwrap();
    let w = list.widgets.iter().find(|w| w.id == "fig-demo").unwrap();
    let target = crate::widget_approval::WidgetTarget::of("main.tex", w)
        .unwrap()
        .unwrap();
    let digest = crate::widget_approval::check_at(approvals, &p.cx, &p.id, &target)
        .unwrap()
        .snapshot
        .digest;
    crate::widget_approval::approve_at(
        approvals,
        &p.cx,
        &crate::widget_approval::WidgetApproveParams {
            root_id: p.id.clone(),
            main_rel: "main.tex".into(),
            widget: "fig-demo".into(),
            digest,
        },
    )
    .unwrap();
}

fn demo_figure(html: &str) -> &str {
    let demo = &html[html.find("data-widget=\"fig-demo\"").unwrap()..];
    &demo[..demo.find("</figure>").unwrap()]
}

#[test]
fn an_unapproved_html_widget_is_held_back_in_the_article_only() {
    let p = project("article-gate", REAL_SIDECAR);
    let base = p.out.join("articles");
    let approvals = fresh_approvals("article-gate");

    let view = article_bundle_in(&p.cx, &p.id, "main.tex", &base, &approvals).unwrap();
    let demo = demo_figure(&view.html);
    assert!(demo.contains("data-approval=\"required\""), "{demo}");
    assert!(demo.contains("class=\"poster\""), "the poster stays");
    assert!(demo.contains("needs your approval"), "{demo}");
    assert!(
        island_json(&view.html, "mfw-widgets")
            .get("fig-demo")
            .is_none(),
        "no document until it is approved"
    );
    assert_eq!(
        view.anchors
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>(),
        ["S1", "S1.SS1", "bib"],
        "sync still lists the headings"
    );

    approve_demo(&p, &approvals);
    let view = article_bundle_in(&p.cx, &p.id, "main.tex", &base, &approvals).unwrap();
    assert!(
        !demo_figure(&view.html).contains("data-approval"),
        "an approved widget runs"
    );
    assert!(island_json(&view.html, "mfw-widgets")["fig-demo"].is_string());

    // The browser preview never holds html widgets back.
    let r = preview_bundle_in(&p.cx, &p.id, "main.tex", &p.out.join("previews")).unwrap();
    let html = std::fs::read_to_string(&r.path).unwrap();
    assert!(!demo_figure(&html).contains("data-approval"));
    assert!(island_json(&html, "mfw-widgets")["fig-demo"].is_string());
}

#[test]
fn an_article_lands_outside_the_project_and_the_next_run_replaces_it() {
    let p = project("article-outside", REAL_SIDECAR);
    let base = p.out.join("articles");
    let approvals = fresh_approvals("article-outside");
    let before = files_under(&p.root);

    article_bundle_in(&p.cx, &p.id, "main.tex", &base, &approvals).unwrap();
    let dir = preview_dir(&base, &p.root);
    assert!(dir.join("index.html").is_file());
    assert!(!dir.starts_with(&p.root), "never inside the project");
    assert_eq!(files_under(&p.root), before, "the project is untouched");

    // A stray file in the article folder is gone after the next run.
    std::fs::write(dir.join("stale.txt"), "old").unwrap();
    article_bundle_in(&p.cx, &p.id, "main.tex", &base, &approvals).unwrap();
    assert!(!dir.join("stale.txt").exists(), "only the latest is kept");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}

#[test]
fn an_article_base_that_overlaps_the_project_is_refused() {
    let p = project("article-inside", REAL_SIDECAR);
    let approvals = fresh_approvals("article-inside");
    let r = article_bundle_in(
        &p.cx,
        &p.id,
        "main.tex",
        &p.root.join("articles"),
        &approvals,
    );
    assert!(r.unwrap_err().contains("overlaps the project"));
    assert!(!p.root.join("articles").exists());
}

#[test]
fn a_failed_article_leaves_no_stale_folder() {
    let p = project("article-fail", REAL_SIDECAR);
    let base = p.out.join("articles");
    let approvals = fresh_approvals("article-fail");
    let r = article_bundle_in(&p.cx, &p.id, "nothere.tex", &base, &approvals);
    assert!(r.is_err());
    assert!(std::fs::read_dir(&base).map(|d| d.count()).unwrap_or(0) == 0);
}

#[test]
fn an_article_for_an_unknown_root_fails_without_naming_a_path() {
    let cx = Core::default();
    let e = article_bundle(&cx, "no-such-root", "main.tex").unwrap_err();
    assert!(!e.is_empty() && !e.contains('/'), "{e}");
}
