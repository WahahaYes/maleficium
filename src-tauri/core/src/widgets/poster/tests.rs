use super::*;
use crate::widgets::{WidgetRect, WidgetSource};

const REAL_PDF: &[u8] = include_bytes!("../../../testdata/interactive/main.pdf");
const REAL_SIDECAR: &str = include_str!("../../../testdata/interactive/main.mfw");
const GLB: &[u8] = include_bytes!("../../../../../e2e/fixtures/interactive/models/mesh.glb");
const SPEC: &[u8] =
    include_bytes!("../../../../../e2e/fixtures/interactive/charts/ablation.vl.json");

/// The real compile's pdf and sidecar (fig-mesh's options replaced by
/// `mesh_opts`) in a fresh project holding the widgets' files. Returns the
/// root id, the root and a scratch folder for posters.
fn project(cx: &Core, name: &str, mesh_opts: &str) -> (String, PathBuf, PathBuf) {
    let dir = crate::test_scratch::dir(&format!("poster-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, bytes) in [
        ("main.tex", &b"x"[..]),
        ("models/mesh.glb", GLB),
        ("charts/ablation.vl.json", SPEC),
        ("widgets/demo/index.html", b"<p>hi</p>"),
        ("out/.keep", b""),
    ] {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    let root = dunce::canonicalize(&dir).unwrap();
    let id = format!("poster-{name}");
    crate::fs::grant_root(cx, &id, &root.to_string_lossy()).unwrap();
    let o = crate::outputs::outputs_of(cx, &id, "main.tex").unwrap();
    let _ = std::fs::remove_dir_all(&o.outdir);
    std::fs::create_dir_all(&o.outdir).unwrap();
    std::fs::write(o.outdir.join(&o.pdf_name), REAL_PDF).unwrap();
    let side = REAL_SIDECAR.replace("|height=170.71652pt|", &format!("|{mesh_opts}|"));
    std::fs::write(o.outdir.join("main.mfw"), side).unwrap();
    (id, root.clone(), root.join("out"))
}

fn req(id: &str, widget: &str, out: &Path) -> PosterRequest {
    PosterRequest {
        root_id: id.into(),
        main_rel: "main.tex".into(),
        widget_id: widget.into(),
        out_path: out.join(format!("{widget}.png")).to_string_lossy().into(),
        timeout_ms: None,
        digest: None,
    }
}

/// An approval store home no test shares (stands in for app data).
fn store(name: &str) -> PathBuf {
    let base = crate::test_scratch::dir(&format!("poster-{name}-appdata"));
    let _ = std::fs::remove_dir_all(&base);
    base
}

/// The job a request builds, against an empty approval store.
fn job_of(cx: &Core, r: &PosterRequest) -> PosterJob {
    match prepare_at(&store("jobs"), cx, r).unwrap() {
        Prepared::Job(j) => j,
        Prepared::ApprovalRequired(a) => panic!("expected a job, got {a:?}"),
    }
}

#[test]
fn a_model_job_carries_its_camera_size_sources_theme_and_policy() {
    let cx = &Core::default();
    let (id, _root, out) = project(
        cx,
        "model",
        "height=170.71652pt,camera=pos=0 0 3 target=0 0 0,size=320x240,background=fff",
    );
    let job = job_of(cx, &req(&id, "fig-mesh", &out));
    assert_eq!(job.frame, (320, 240));
    assert_eq!(job.expect, Some((320, 240)));
    assert_eq!(job.timeout, Duration::from_millis(DEFAULT_TIMEOUT_MS));
    let o = &job.init["options"];
    assert_eq!(o["camera"], "1 0 0 0 0 1 0 0 0 0 1 0 0 0 3 1");
    assert_eq!(o["size"], "320x240");
    assert_eq!(o["background"], "#ffffff");
    assert_eq!(job.init["type"], "init");
    assert_eq!(job.init["protocol"], 1);
    assert_eq!(job.init["runtime"], "model@1");
    assert_eq!(job.init["theme"]["mode"], "light");
    assert_eq!(job.init["theme"]["tokens"]["--m-figure-bg"], "#ffffff");
    assert_eq!(job.init["sources"]["model"]["name"], "mesh.glb");
    assert_eq!(job.init["sources"]["model"]["mime"], "model/gltf-binary");
    assert_eq!(job.sources.len(), 1);
    assert_eq!(job.sources[0].key, "model");
    assert_eq!(job.sources[0].bytes, GLB);
    assert!(job
        .document
        .contains("Content-Security-Policy\" content=\"default-src 'none'"));
    assert!(job.document.contains("connect-src 'none'"));
}

#[test]
fn a_model_without_size_gets_one_from_its_box_and_a_chart_gets_scale_2() {
    let cx = &Core::default();
    let (id, _root, out) = project(cx, "defaults", "height=170.71652pt");
    let w = widgets(cx, &id, "main.tex").unwrap();
    let mesh = w.widgets.iter().find(|w| w.id == "fig-mesh").unwrap();
    let job = job_of(cx, &req(&id, "fig-mesh", &out));
    let want = (
        clamp_side((mesh.rect.x1 - mesh.rect.x0) * CSS_PER_PT * 2.0),
        clamp_side((mesh.rect.y1 - mesh.rect.y0) * CSS_PER_PT * 2.0),
    );
    assert_eq!(job.frame, want);
    assert_eq!(job.expect, Some(want));
    assert_eq!(
        job.init["options"]["size"],
        format!("{}x{}", want.0, want.1)
    );
    assert!(job.init["options"].get("camera").is_none());

    let chart = job_of(cx, &req(&id, "fig-chart", &out));
    assert_eq!(chart.init["options"]["scale"], 2);
    assert_eq!(chart.expect, None);
    assert_eq!(chart.init["sources"]["spec"]["mime"], "application/json");
    assert!(chart.frame.0 >= params::MIN_SIDE && chart.frame.1 >= params::MIN_SIDE);
}

#[test]
fn table_and_video_widgets_are_never_rendered() {
    let cx = &Core::default();
    let (id, _root, out) = project(cx, "refused", "height=170.71652pt");
    for (w, want) in [
        ("tab-results", "typeset rows"),
        ("fig-clip", "video posters"),
        ("ghost", "no widget `ghost`"),
    ] {
        let e = prepare(cx, &req(&id, w, &out)).unwrap_err();
        assert!(e.contains(want), "{w}: {e}");
    }
}

#[test]
fn bad_output_paths_and_timeouts_are_handled() {
    let cx = &Core::default();
    let (id, _root, out) = project(cx, "paths", "height=170.71652pt");
    let mut r = req(&id, "fig-mesh", &out);
    for (path, want) in [
        ("rel/poster.png".to_string(), "absolute"),
        (
            out.join("poster.jpg").to_string_lossy().into_owned(),
            ".png",
        ),
        (
            out.join("nope/poster.png").to_string_lossy().into_owned(),
            "folder does not exist",
        ),
    ] {
        r.out_path = path;
        let e = prepare(cx, &r).unwrap_err();
        assert!(e.contains(want), "{e}");
    }
    let mut r = req(&id, "fig-mesh", &out);
    r.timeout_ms = Some(1);
    assert_eq!(
        job_of(cx, &r).timeout,
        Duration::from_millis(MIN_TIMEOUT_MS)
    );
    r.timeout_ms = Some(10_000_000);
    assert_eq!(
        job_of(cx, &r).timeout,
        Duration::from_millis(MAX_TIMEOUT_MS)
    );
}

#[test]
fn only_shipped_runtimes_render_and_the_hang_runtime_is_debug_only() {
    let cx = &Core::default();
    let (id, _root, out) = project(cx, "runtimes", "height=170.71652pt");
    let o = crate::outputs::outputs_of(cx, &id, "main.tex").unwrap();
    let side = std::fs::read_to_string(o.outdir.join("main.mfw")).unwrap();
    std::fs::write(
        o.outdir.join("main.mfw"),
        side.replace("|model|model@1|", "|model|mystery@9|"),
    )
    .unwrap();
    let e = prepare(cx, &req(&id, "fig-mesh", &out)).unwrap_err();
    assert!(e.contains("no mystery@9 runtime"), "{e}");
    std::fs::write(
        o.outdir.join("main.mfw"),
        side.replace("|model|model@1|", &format!("|model|{TEST_HANG_RUNTIME}|")),
    )
    .unwrap();
    let job = job_of(cx, &req(&id, "fig-mesh", &out));
    assert!(job.document.contains("for(;;){}"));
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    b.extend_from_slice(&w.to_be_bytes());
    b.extend_from_slice(&h.to_be_bytes());
    b.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    b
}

fn data_url(b: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(b)
    )
}

#[test]
fn finish_checks_the_snapshot_and_writes_it_in_one_step() {
    let dir = crate::test_scratch::dir("poster-finish");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut job = PosterJob {
        widget_id: "w".into(),
        document: String::new(),
        init: Value::Null,
        sources: vec![],
        frame: (4, 3),
        expect: Some((4, 3)),
        timeout: Duration::from_secs(1),
        out: dir.join("w.png"),
    };
    let e = finish(&job, &data_url(&png(5, 3))).unwrap_err();
    assert!(e.contains("is 5x3, expected 4x3"), "{e}");
    assert!(finish(&job, "data:image/jpeg;base64,AAAA").is_err());
    assert!(finish(&job, &data_url(b"not a png at all, no")).is_err());
    assert!(finish(&job, "data:image/png;base64,***").is_err());
    assert!(!job.out.exists());
    let r = finish(&job, &data_url(&png(4, 3))).unwrap();
    assert_eq!((r.width, r.height), (4, 3));
    assert_eq!(std::fs::read(&job.out).unwrap(), png(4, 3));
    assert_eq!(r.sha256, sha_of(&png(4, 3)));
    assert!(!dir.join("w.png.part").exists());
    job.expect = None;
    assert_eq!(finish(&job, &data_url(&png(9, 9))).unwrap().width, 9);
    let _ = std::fs::remove_dir_all(dir);
}

fn widget(poster: Option<&str>) -> Widget {
    Widget {
        id: "w".into(),
        kind: WidgetType::Model,
        runtime: Some("model@1".into()),
        label: None,
        figure: None,
        theme: "house".into(),
        poster: poster.map(Into::into),
        sources: vec![WidgetSource {
            role: "model".into(),
            path: "m.glb".into(),
        }],
        options: vec![],
        alt: "a".into(),
        page: 1,
        rect: WidgetRect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        csp: None,
    }
}

#[test]
fn an_explicit_poster_wins_over_a_cached_one_and_a_placeholder_comes_last() {
    let dir = crate::test_scratch::dir("poster-precedence");
    std::fs::create_dir_all(&dir).unwrap();
    let cached = dir.join("cached.png");
    std::fs::write(&cached, png(2, 2)).unwrap();
    assert_eq!(
        poster_source(&widget(Some("figures/mesh.png")), Some(&cached)),
        PosterSource::Explicit("figures/mesh.png".into())
    );
    assert_eq!(
        poster_source(&widget(None), Some(&cached)),
        PosterSource::Cached(cached.clone())
    );
    assert_eq!(
        poster_source(&widget(Some("")), Some(&dir.join("missing.png"))),
        PosterSource::Placeholder
    );
    assert_eq!(
        poster_source(&widget(None), None),
        PosterSource::Placeholder
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_light_tokens_come_from_the_house_theme() {
    let t = light_tokens();
    assert_eq!(t["--m-color-text"], "#1e1b24");
    assert!(t.keys().all(|k| k.starts_with("--m-")));
    assert!(!t.contains_key("--m-color-target") || t["--m-color-target"] == "#fff4c2");
}

// ---- html widgets: the approval gate ------------------------------------

/// The probe's script: if a renderer ever runs it, it leaves its marker.
const PROBE_JS: &str = "window.__probe='MARKER-RAN';";

/// An html widget project whose demo bundle loads the probe script, with
/// its own approval store.
struct Html {
    cx: Core,
    id: String,
    root: PathBuf,
    out: PathBuf,
    base: PathBuf,
}

impl Html {
    fn new(name: &str) -> Self {
        let cx = Core::default();
        let (id, root, out) = project(&cx, name, "height=170.71652pt");
        // No poster= of its own: the auto-poster is the only picture.
        let o = crate::outputs::outputs_of(&cx, &id, "main.tex").unwrap();
        let side = std::fs::read_to_string(o.outdir.join("main.mfw")).unwrap();
        std::fs::write(
            o.outdir.join("main.mfw"),
            side.replace("|figures/demo.png|", "||"),
        )
        .unwrap();
        let h = Html {
            cx,
            id,
            root,
            out,
            base: store(name),
        };
        h.write(
            "index.html",
            "<!doctype html><title>probe</title><script src=\"probe.js\"></script>",
        );
        h.write("probe.js", PROBE_JS);
        h
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.root.join("widgets/demo").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    fn req(&self) -> PosterRequest {
        req(&self.id, "fig-demo", &self.out)
    }
    fn target(&self) -> WidgetTarget {
        WidgetTarget {
            id: "fig-demo".into(),
            path: "widgets/demo".into(),
            option_origins: Default::default(),
        }
    }
    fn digest(&self) -> String {
        widget_approval::check_at(&self.base, &self.cx, &self.id, &self.target())
            .unwrap()
            .snapshot
            .digest
    }
    fn approve(&self) {
        widget_approval::approve_at(
            &self.base,
            &self.cx,
            &widget_approval::WidgetApproveParams {
                root_id: self.id.clone(),
                main_rel: "main.tex".into(),
                widget: "fig-demo".into(),
                digest: self.digest(),
            },
        )
        .unwrap();
    }
    fn marker(&self) -> PathBuf {
        self.out.join("probe.marker")
    }
    fn png(&self) -> PathBuf {
        self.out.join("fig-demo.png")
    }
    /// Runs the request through a stand-in renderer that executes what it
    /// is given: a document carrying the probe's script writes the marker
    /// (the webview would run it). Returns the outcome and the document
    /// the renderer received, if it was reached at all.
    fn run(&self, r: &PosterRequest) -> (Result<PosterOutcome, String>, Option<String>) {
        let mut seen = None;
        let res = run_at(&self.base, &self.cx, r, |job| {
            seen = Some(job.document.clone());
            if job.document.contains("MARKER-RAN") {
                std::fs::write(self.marker(), "ran").unwrap();
            }
            Ok(data_url(&png(4, 3)))
        });
        (res, seen)
    }
}

impl Drop for Html {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn approval(o: &Result<PosterOutcome, String>) -> &ApprovalRequired {
    match o {
        Ok(PosterOutcome::ApprovalRequired(r)) => r,
        other => panic!("expected approval_required, got {other:?}"),
    }
}

#[test]
fn an_unapproved_html_widget_never_reaches_the_renderer() {
    let h = Html::new("html-unapproved");
    let (res, seen) = h.run(&h.req());
    let r = approval(&res);
    assert_eq!(r.widget, "fig-demo");
    assert_eq!(
        r.cause,
        maleficium_events::WidgetApprovalCause::NeverApproved
    );
    assert_eq!(r.digest, h.digest());
    assert!(seen.is_none(), "the renderer was reached");
    assert!(!h.marker().exists(), "the probe ran");
    assert!(!h.png().exists(), "a poster was written");
    // prepare alone says the same and builds no job.
    assert!(matches!(
        prepare_at(&h.base, &h.cx, &h.req()).unwrap(),
        Prepared::ApprovalRequired(_)
    ));
}

#[test]
fn an_approved_html_widget_renders_the_bytes_that_were_approved() {
    let h = Html::new("html-approved");
    h.approve();
    let (res, seen) = h.run(&h.req());
    match res {
        Ok(PosterOutcome::Rendered(p)) => assert_eq!((p.width, p.height), (4, 3)),
        other => panic!("expected rendered, got {other:?}"),
    }
    assert!(h.marker().exists(), "the approved probe ran");
    assert!(h.png().is_file());
    let doc = seen.unwrap();
    assert!(
        doc.contains(&format!("<script>{PROBE_JS}</script>")),
        "{doc}"
    );
    assert!(
        doc.contains("connect-src 'none'"),
        "a poster render never reaches the network: {doc}"
    );
    let j = match prepare_at(&h.base, &h.cx, &h.req()).unwrap() {
        Prepared::Job(j) => j,
        Prepared::ApprovalRequired(a) => panic!("{a:?}"),
    };
    assert_eq!(j.init["runtime"], HTML_RUNTIME);
    assert_eq!(j.init["widgetId"], "fig-demo");
    assert!(j.sources.is_empty() && j.expect.is_none());
}

#[test]
fn an_edit_after_approval_stops_rendering_until_it_is_approved_again() {
    let h = Html::new("html-edited");
    h.approve();
    h.write("probe.js", "window.__probe='MARKER-RAN'; /* edited */");
    let (res, seen) = h.run(&h.req());
    let r = approval(&res);
    assert_eq!(
        r.cause,
        maleficium_events::WidgetApprovalCause::ChangedSinceApproval
    );
    assert!(seen.is_none() && !h.marker().exists() && !h.png().exists());
    h.approve();
    let (res, seen) = h.run(&h.req());
    assert!(matches!(res, Ok(PosterOutcome::Rendered(_))), "{res:?}");
    assert!(seen.unwrap().contains("/* edited */"));
    assert!(h.marker().exists());
    // Revoked: nothing runs, auto-approval included.
    std::fs::remove_file(h.marker()).unwrap();
    widget_approval::revoke_at(
        &h.base,
        &h.cx,
        &widget_approval::WidgetRevokeParams {
            root_id: h.id.clone(),
            path: "widgets/demo".into(),
        },
    )
    .unwrap();
    let (res, seen) = h.run(&h.req());
    assert_eq!(
        approval(&res).cause,
        maleficium_events::WidgetApprovalCause::Revoked
    );
    assert!(seen.is_none() && !h.marker().exists());
}

#[test]
fn the_renderer_runs_the_snapshot_it_was_judged_on() {
    let h = Html::new("html-snapshot");
    h.approve();
    // The folder changes once the job is built: what runs is still the
    // approved snapshot, never a second read.
    let mut seen = None;
    let res = run_at(&h.base, &h.cx, &h.req(), |job| {
        h.write("probe.js", "window.__probe='SWAPPED';");
        seen = Some(job.document.clone());
        Ok(data_url(&png(4, 3)))
    });
    assert!(matches!(res, Ok(PosterOutcome::Rendered(_))), "{res:?}");
    let doc = seen.unwrap();
    assert!(
        doc.contains("MARKER-RAN") && !doc.contains("SWAPPED"),
        "{doc}"
    );
    // And the swapped folder is unapproved for the next render.
    assert!(matches!(
        h.run(&h.req()).0,
        Ok(PosterOutcome::ApprovalRequired(_))
    ));
}

#[test]
fn a_request_for_another_digest_than_the_folder_has_is_refused() {
    let h = Html::new("html-digest");
    // Auto-approval covers the edit, but the request was made for the
    // digest the cache key came from.
    widget_approval::set_auto_at(
        &h.base,
        &h.cx,
        &widget_approval::WidgetAutoApproveParams {
            root_id: h.id.clone(),
            on: true,
        },
    )
    .unwrap();
    let asked = h.digest();
    h.write("probe.js", "window.__probe='MARKER-RAN'; /* later */");
    let mut r = h.req();
    r.digest = Some(asked);
    let (res, seen) = h.run(&r);
    assert!(res.unwrap_err().contains("changed since"), "refused");
    assert!(seen.is_none() && !h.marker().exists());
    // At the current digest, auto-approval lets it render.
    r.digest = Some(h.digest());
    assert!(matches!(h.run(&r).0, Ok(PosterOutcome::Rendered(_))));
    // A digest on a first-party widget is a mistake, not ignored.
    let mut m = req(&h.id, "fig-mesh", &h.out);
    m.digest = Some("0".repeat(64));
    assert!(prepare_at(&h.base, &h.cx, &m)
        .unwrap_err()
        .contains("html widgets only"));
}

#[test]
fn auto_approval_never_covers_new_declared_origins_and_origins_stay_off() {
    let h = Html::new("html-origins");
    widget_approval::set_auto_at(
        &h.base,
        &h.cx,
        &widget_approval::WidgetAutoApproveParams {
            root_id: h.id.clone(),
            on: true,
        },
    )
    .unwrap();
    // Never approved, no origins: auto mode lets it run.
    assert!(matches!(h.run(&h.req()).0, Ok(PosterOutcome::Rendered(_))));
    std::fs::remove_file(h.marker()).unwrap();
    h.write(
        "widget.json",
        r#"{"csp":{"connectDomains":["https://api.example.org"],"frameDomains":["https://embed.example.org"]}}"#,
    );
    let (res, seen) = h.run(&h.req());
    assert_eq!(
        approval(&res).cause,
        maleficium_events::WidgetApprovalCause::NeverApproved
    );
    assert!(seen.is_none() && !h.marker().exists());
    // The user approves it with its origins: a poster render still gets
    // none of them.
    h.approve();
    let (res, seen) = h.run(&h.req());
    assert!(matches!(res, Ok(PosterOutcome::Rendered(_))), "{res:?}");
    let doc = seen.unwrap();
    assert!(
        doc.contains("connect-src 'none'")
            && !doc.contains("api.example.org")
            && !doc.contains("frame-src")
            && !doc.contains("embed.example.org"),
        "{doc}"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_special_files_and_oversize_folders_never_run() {
    let h = Html::new("html-refuse");
    h.approve();
    let demo = h.root.join("widgets/demo");
    std::os::unix::fs::symlink("/etc/hostname", demo.join("leak.txt")).unwrap();
    let (res, seen) = h.run(&h.req());
    assert!(res.unwrap_err().contains("symlink"));
    assert!(seen.is_none() && !h.marker().exists());
    std::fs::remove_file(demo.join("leak.txt")).unwrap();

    let fifo = std::ffi::CString::new(demo.join("pipe").to_string_lossy().as_bytes()).unwrap();
    // SAFETY: a plain mkfifo on a path we own.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let (res, seen) = h.run(&h.req());
    assert!(res.unwrap_err().contains("not a regular file"));
    assert!(seen.is_none() && !h.marker().exists());
    std::fs::remove_file(demo.join("pipe")).unwrap();

    std::fs::create_dir_all(demo.join("many")).unwrap();
    for i in 0..widget_approval::MAX_FILES {
        std::fs::write(demo.join(format!("many/{i}")), "").unwrap();
    }
    let (res, seen) = h.run(&h.req());
    assert!(res.unwrap_err().contains("more than"));
    assert!(seen.is_none() && !h.marker().exists());
    std::fs::remove_dir_all(demo.join("many")).unwrap();

    // Back as approved: it runs again.
    assert!(matches!(h.run(&h.req()).0, Ok(PosterOutcome::Rendered(_))));
    assert!(h.marker().exists());
}

#[test]
fn first_party_runtime_widgets_render_without_any_approval() {
    let h = Html::new("html-exempt");
    for w in ["fig-mesh", "fig-chart"] {
        let mut ran = false;
        let res = run_at(&h.base, &h.cx, &req(&h.id, w, &h.out), |job| {
            ran = true;
            let (pw, ph) = job.expect.unwrap_or((4, 3));
            Ok(data_url(&png(pw, ph)))
        });
        assert!(ran, "{w}");
        assert!(
            matches!(res, Ok(PosterOutcome::Rendered(_))),
            "{w}: {res:?}"
        );
    }
}
