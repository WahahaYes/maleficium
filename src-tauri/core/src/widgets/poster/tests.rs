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
    let job = prepare(cx, &req(&id, "fig-mesh", &out)).unwrap();
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
    let job = prepare(cx, &req(&id, "fig-mesh", &out)).unwrap();
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

    let chart = prepare(cx, &req(&id, "fig-chart", &out)).unwrap();
    assert_eq!(chart.init["options"]["scale"], 2);
    assert_eq!(chart.expect, None);
    assert_eq!(chart.init["sources"]["spec"]["mime"], "application/json");
    assert!(chart.frame.0 >= params::MIN_SIDE && chart.frame.1 >= params::MIN_SIDE);
}

#[test]
fn html_table_and_video_widgets_are_never_rendered() {
    let cx = &Core::default();
    let (id, _root, out) = project(cx, "refused", "height=170.71652pt");
    for (w, want) in [
        ("fig-demo", "html widgets are not rendered"),
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
        prepare(cx, &r).unwrap().timeout,
        Duration::from_millis(MIN_TIMEOUT_MS)
    );
    r.timeout_ms = Some(10_000_000);
    assert_eq!(
        prepare(cx, &r).unwrap().timeout,
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
    let job = prepare(cx, &req(&id, "fig-mesh", &out)).unwrap();
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
