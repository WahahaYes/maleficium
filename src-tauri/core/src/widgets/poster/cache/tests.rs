use super::*;
use crate::widgets::poster::PosterRendered;
use std::sync::Mutex;

const REAL_PDF: &[u8] = include_bytes!("../../../../testdata/interactive/main.pdf");
const REAL_SIDECAR: &str = include_str!("../../../../testdata/interactive/main.mfw");
const GLB: &[u8] = include_bytes!("../../../../../../e2e/fixtures/interactive/models/mesh.glb");
const SPEC: &[u8] =
    include_bytes!("../../../../../../e2e/fixtures/interactive/charts/ablation.vl.json");

/// The real compile's pdf and sidecar, with the model's and the chart's
/// explicit posters dropped (so the cache provides them), in a fresh
/// project. Returns the root id and the root.
fn project(cx: &Core, name: &str) -> (String, PathBuf) {
    let dir = crate::test_scratch::dir(&format!("cache-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, bytes) in [
        ("main.tex", &b"x"[..]),
        ("models/mesh.glb", GLB),
        ("charts/ablation.vl.json", SPEC),
        ("widgets/demo/index.html", b"<p>hi</p>"),
    ] {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    let root = dunce::canonicalize(&dir).unwrap();
    let id = format!("cache-{name}");
    crate::fs::grant_root(cx, &id, &root.to_string_lossy()).unwrap();
    let o = crate::outputs::outputs_of(cx, &id, "main.tex").unwrap();
    let _ = std::fs::remove_dir_all(&o.outdir);
    std::fs::create_dir_all(&o.outdir).unwrap();
    std::fs::write(o.outdir.join(&o.pdf_name), REAL_PDF).unwrap();
    let side = REAL_SIDECAR
        .replace("|figures/mesh.png|", "||")
        .replace("|figures/chart.png|", "||");
    std::fs::write(o.outdir.join("main.mfw"), side).unwrap();
    (id, root)
}

fn set_sidecar(cx: &Core, id: &str, f: impl Fn(String) -> String) {
    let o = crate::outputs::outputs_of(cx, id, "main.tex").unwrap();
    let p = o.outdir.join("main.mfw");
    let s = std::fs::read_to_string(&p).unwrap();
    std::fs::write(&p, f(s)).unwrap();
}

/// A renderer that writes a tiny PNG where asked, counting requests.
#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<String>>,
    elsewhere: Option<PathBuf>,
}

impl PosterRenderer for Fake {
    fn render(&self, _cx: &Core, reqs: &[PosterRequest]) -> Vec<Result<PosterOutcome, String>> {
        reqs.iter()
            .map(|r| {
                self.seen.lock().unwrap().push(r.widget_id.clone());
                let path = self
                    .elsewhere
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(&r.out_path));
                std::fs::write(&path, b"\x89PNG fake").map_err(|e| e.to_string())?;
                Ok(PosterOutcome::Rendered(PosterRendered {
                    widget_id: r.widget_id.clone(),
                    path: path.to_string_lossy().into_owned(),
                    width: 4,
                    height: 3,
                    sha256: String::new(),
                }))
            })
            .collect()
    }
}

fn with_fake(cx: &Core) -> Arc<Fake> {
    let f = Arc::new(Fake::default());
    cx.set_poster_renderer(f.clone());
    f
}

fn run_before(cx: &Core, id: &str) -> Vec<String> {
    let mut lines = Vec::new();
    before_compile(cx, id, "main.tex", &mut |l| lines.push(l));
    lines
}

fn run_after(cx: &Core, id: &str) -> Vec<String> {
    let mut lines = Vec::new();
    after_compile(cx, id, "main.tex", &mut |l| lines.push(l));
    lines
}

fn base() -> (Vec<(String, String)>, Vec<KeySource>) {
    (
        vec![
            ("height".into(), "113.81102pt".into()),
            ("size".into(), "320x240".into()),
        ],
        vec![("model".into(), "models/mesh.glb".into(), b"glb".to_vec())],
    )
}

#[test]
fn the_key_is_stable_for_the_same_input() {
    let (o, s) = base();
    let a = digest("model", "model@1", "<doc>", &o, &s, "--m-a:1;");
    assert_eq!(a, digest("model", "model@1", "<doc>", &o, &s, "--m-a:1;"));
    assert_eq!(a.len(), 64);
    assert!(is_key(&a));
    // Options are a set: their order in the record does not matter.
    let mut rev = o.clone();
    rev.reverse();
    assert_eq!(a, digest("model", "model@1", "<doc>", &rev, &s, "--m-a:1;"));
}

#[test]
fn any_change_to_options_sources_runtime_or_theme_changes_the_key() {
    let (o, s) = base();
    let a = digest("model", "model@1", "<doc>", &o, &s, "t");
    let mut opt = o.clone();
    opt[1].1 = "640x480".into();
    let mut added = o.clone();
    added.push(("background".into(), "#ffffff".into()));
    let mut bytes = s.clone();
    bytes[0].2 = b"glB".to_vec();
    let mut path = s.clone();
    path[0].1 = "models/other.glb".into();
    let mut role = s.clone();
    role[0].0 = "spec".into();
    for (what, b) in [
        (
            "option value",
            digest("model", "model@1", "<doc>", &opt, &s, "t"),
        ),
        (
            "added option",
            digest("model", "model@1", "<doc>", &added, &s, "t"),
        ),
        (
            "source bytes",
            digest("model", "model@1", "<doc>", &o, &bytes, "t"),
        ),
        (
            "source path",
            digest("model", "model@1", "<doc>", &o, &path, "t"),
        ),
        (
            "source role",
            digest("model", "model@1", "<doc>", &o, &role, "t"),
        ),
        (
            "runtime name",
            digest("model", "model@2", "<doc>", &o, &s, "t"),
        ),
        (
            "runtime bytes",
            digest("model", "model@1", "<doc2>", &o, &s, "t"),
        ),
        ("theme", digest("model", "model@1", "<doc>", &o, &s, "u")),
        ("type", digest("chart", "model@1", "<doc>", &o, &s, "t")),
    ] {
        assert_ne!(a, b, "{what}");
    }
    // Length prefixes: moving a character across a field boundary is a
    // different input.
    let x = digest(
        "model",
        "model@1",
        "<doc>",
        &[("ab".into(), "c".into())],
        &s,
        "t",
    );
    let y = digest(
        "model",
        "model@1",
        "<doc>",
        &[("a".into(), "bc".into())],
        &s,
        "t",
    );
    assert_ne!(x, y);
}

#[test]
fn a_widgets_key_follows_its_source_bytes_and_options() {
    let cx = &Core::default();
    let (id, root) = project(cx, "key");
    let key = |cx: &Core| {
        let l = widgets(cx, &id, "main.tex").unwrap();
        let w = l.widgets.iter().find(|w| w.id == "fig-mesh").unwrap();
        poster_key(cx, &id, "main.tex", w).unwrap()
    };
    let a = key(cx);
    assert_eq!(a, key(cx));
    std::fs::write(root.join("models/mesh.glb"), [GLB, b"!"].concat()).unwrap();
    let b = key(cx);
    assert_ne!(a, b);
    set_sidecar(cx, &id, |s| {
        s.replace("|height=170.71652pt|", "|height=170.71652pt,size=320x240|")
    });
    assert_ne!(b, key(cx));
}

#[test]
fn the_map_round_trips_and_refuses_what_tex_would_reread() {
    let key = "0123456789abcdef".repeat(4);
    let e = vec![MapEntry {
        id: "fig-mesh".into(),
        guard:
            "model@1|model=models/mesh_v2.glb|height=170.71652pt,camera=1 0 0 1,size=,background="
                .into(),
        key: key.clone(),
    }];
    let text = map_text("main.tex", &e);
    assert!(text.starts_with("% Maleficium poster map for main.tex"));
    assert!(text.contains(&format!(
        "\\mfw@postermap{{fig-mesh}}{{model@1|model=models/mesh_v2.glb|height=170.71652pt,camera=1 0 0 1,size=,background=}}{{{key}}}\n"
    )));
    assert_eq!(parse_map(&text), e);
    assert!(parse_map("\\mfw@postermap{a}{b}{not-a-key}").is_empty());
    for bad in ["a\\b", "a{b", "a}b", "a%b", "a#b", "a^^b", "a\nb"] {
        assert!(!tex_safe(bad), "{bad}");
    }
    assert!(tex_safe(
        "model@1|model=a b/c_d.glb|height=1pt,camera=1 -2e3 +4"
    ));
    assert_eq!(fold_spaces("camera=0.7  0 \t 1 "), "camera=0.7 0 1 ");
}

#[test]
fn the_readme_is_written_once_and_never_replaced() {
    let dir = crate::test_scratch::dir("cache-readme");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let c = Cache::at(&dir);
    c.ensure().unwrap();
    assert!(write_readme(&c).unwrap());
    let path = c.dir.join("README.md");
    let text = std::fs::read_to_string(&path).unwrap();
    for want in [
        "safe to delete",
        "commit",
        "renders the posters",
        "100 MiB",
        CAP_ENV,
    ] {
        assert!(text.contains(want), "{want}");
    }
    std::fs::write(&path, "my notes").unwrap();
    assert!(!write_readme(&c).unwrap());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "my notes");
}

#[cfg(unix)]
#[test]
fn a_linked_cache_folder_is_refused() {
    let dir = crate::test_scratch::dir("cache-link");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(dir.join("elsewhere"), dir.join(CACHE_DIR)).unwrap();
    let c = Cache::at(&dir);
    assert!(c.ensure().unwrap_err().contains("not a folder"));
    assert!(!c.present());
    assert!(!dir.join("elsewhere").join(POSTERS_DIR).exists());
}

fn poster(c: &Cache, key: &str, len: usize, age_s: u64) {
    let p = c.png(key);
    std::fs::write(&p, vec![0u8; len]).unwrap();
    let t = std::time::SystemTime::now() - std::time::Duration::from_secs(age_s);
    std::fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

fn k(c: char) -> String {
    c.to_string().repeat(64)
}

#[test]
fn collect_drops_orphans_and_keeps_what_maps_use() {
    let dir = crate::test_scratch::dir("cache-gc");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let c = Cache::at(&dir);
    c.ensure().unwrap();
    std::fs::write(dir.join("other.tex"), "x").unwrap();
    for ch in ['a', 'b', 'c', 'd'] {
        poster(&c, &k(ch), 10, 0);
    }
    let other = |key: String| MapEntry {
        id: "w".into(),
        guard: "g".into(),
        key,
    };
    std::fs::write(c.map("other"), map_text("other.tex", &[other(k('b'))])).unwrap();
    std::fs::write(c.map("gone"), map_text("gone.tex", &[other(k('c'))])).unwrap();
    std::fs::write(c.posters.join("x.png.part"), "half").unwrap();
    std::fs::write(c.posters.join("notes.txt"), "mine").unwrap();
    let keep = BTreeSet::from([k('a')]);
    let g = collect(&c, &dir, "main", &keep, DEFAULT_CAP_BYTES).unwrap();
    // c was only used by the map of a main file that no longer exists.
    assert_eq!(g.orphans, vec![k('c'), k('d')]);
    assert!(g.evicted.is_empty());
    assert_eq!(g.bytes, 20);
    assert!(c.png(&k('a')).is_file() && c.png(&k('b')).is_file());
    assert!(!c.map("gone").exists() && c.map("other").exists());
    assert!(!c.posters.join("x.png.part").exists());
    assert!(
        c.posters.join("notes.txt").exists(),
        "only cache files are touched"
    );
}

#[test]
fn collect_evicts_the_oldest_beyond_the_cap_but_never_the_current_paper() {
    let dir = crate::test_scratch::dir("cache-cap");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let c = Cache::at(&dir);
    c.ensure().unwrap();
    std::fs::write(dir.join("other.tex"), "x").unwrap();
    // Old, older, oldest: all used by another main; a is this paper's.
    poster(&c, &k('a'), 100, 1000);
    poster(&c, &k('b'), 100, 10);
    poster(&c, &k('c'), 100, 20);
    poster(&c, &k('d'), 100, 30);
    let entries: Vec<MapEntry> = ['b', 'c', 'd']
        .iter()
        .map(|ch| MapEntry {
            id: format!("w{ch}"),
            guard: "g".into(),
            key: k(*ch),
        })
        .collect();
    std::fs::write(c.map("other"), map_text("other.tex", &entries)).unwrap();
    let keep = BTreeSet::from([k('a')]);
    let g = collect(&c, &dir, "main", &keep, 250).unwrap();
    assert_eq!(g.evicted, vec![k('d'), k('c')]);
    assert_eq!(g.bytes, 200);
    assert!(c.png(&k('a')).is_file() && c.png(&k('b')).is_file());
    // Over the cap with only this paper's posters left: they stay.
    let g = collect(&c, &dir, "main", &BTreeSet::from([k('a'), k('b')]), 50).unwrap();
    assert!(g.evicted.is_empty());
    assert_eq!(g.bytes, 200);
}

#[test]
fn the_cap_comes_from_the_environment_in_mib() {
    // Read-only check of the parse; the variable is not set in tests.
    if std::env::var_os(CAP_ENV).is_none() {
        assert_eq!(cap_bytes(), DEFAULT_CAP_BYTES);
    }
}

#[test]
fn before_compile_renders_what_is_missing_once_and_maps_it() {
    let cx = &Core::default();
    let (id, root) = project(cx, "before");
    let fake = with_fake(cx);
    let lines = run_before(cx, &id);
    let mut seen = fake.seen.lock().unwrap().clone();
    seen.sort();
    // The model and the chart lack posters; video, html and table never
    // come from the cache.
    assert_eq!(seen, ["fig-chart", "fig-mesh"], "{lines:?}");
    let c = Cache::at(&root);
    assert!(c.dir.join("README.md").is_file());
    let map = std::fs::read_to_string(c.map("main")).unwrap();
    let m = parse_map(&map);
    assert_eq!(
        m.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["fig-mesh", "fig-chart"]
    );
    assert_eq!(
        m[0].guard,
        "model@1|model=models/mesh.glb|height=170.71652pt"
    );
    assert!(m.iter().all(|e| c.png(&e.key).is_file()));
    assert!(
        lines
            .iter()
            .any(|l| l == "posters: 0 cached, 2 rendered, 0 placeholder"),
        "{lines:?}"
    );

    // Unchanged: nothing renders again and the map is not rewritten.
    let stamp = std::fs::metadata(c.map("main"))
        .unwrap()
        .modified()
        .unwrap();
    let lines = run_before(cx, &id);
    assert_eq!(fake.seen.lock().unwrap().len(), 2, "{lines:?}");
    assert_eq!(
        std::fs::metadata(c.map("main"))
            .unwrap()
            .modified()
            .unwrap(),
        stamp
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "posters: 2 cached, 0 rendered, 0 placeholder"),
        "{lines:?}"
    );

    // Deleted: it all comes back.
    std::fs::remove_dir_all(&c.dir).unwrap();
    run_before(cx, &id);
    assert_eq!(fake.seen.lock().unwrap().len(), 4);
    assert!(c.present() && c.map("main").is_file());
}

#[test]
fn before_compile_reports_how_many_posters_it_rendered() {
    // The compile runs the engine again only when this is above zero, so
    // the pdf shows posters that were new in the compile just finished.
    let cx = &Core::default();
    let (id, _root) = project(cx, "count");
    with_fake(cx);
    let mut say = |_: String| {};
    assert_eq!(before_compile(cx, &id, "main.tex", &mut say), 2);
    assert_eq!(before_compile(cx, &id, "main.tex", &mut say), 0);
}

#[test]
fn a_failed_render_does_not_ask_for_another_compile() {
    let cx = &Core::default();
    let (id, root) = project(cx, "nocount");
    let f = Arc::new(Fake {
        elsewhere: Some(root.join("stray.png")),
        ..Fake::default()
    });
    cx.set_poster_renderer(f);
    let mut say = |_: String| {};
    assert_eq!(before_compile(cx, &id, "main.tex", &mut say), 0);
}

#[test]
fn nothing_is_written_without_a_compile_or_without_auto_posters() {
    let cx = &Core::default();
    let (id, root) = project(cx, "quiet");
    let fake = with_fake(cx);
    // Every widget gives its own poster.
    let o = crate::outputs::outputs_of(cx, &id, "main.tex").unwrap();
    std::fs::write(o.outdir.join("main.mfw"), REAL_SIDECAR).unwrap();
    run_before(cx, &id);
    run_after(cx, &id);
    assert!(!root.join(CACHE_DIR).exists());
    // Never compiled: no widget list, nothing to do.
    std::fs::remove_dir_all(&o.outdir).unwrap();
    run_before(cx, &id);
    assert!(!root.join(CACHE_DIR).exists());
    assert!(fake.seen.lock().unwrap().is_empty());
}

#[test]
fn a_renderer_writing_outside_the_cache_is_not_mapped() {
    let cx = &Core::default();
    let (id, root) = project(cx, "elsewhere");
    let f = Arc::new(Fake {
        elsewhere: Some(root.join("stray.png")),
        ..Fake::default()
    });
    cx.set_poster_renderer(f);
    let lines = run_before(cx, &id);
    assert!(
        lines.iter().any(|l| l.contains("instead of the cache")),
        "{lines:?}"
    );
    let map = std::fs::read_to_string(Cache::at(&root).map("main")).unwrap();
    assert!(parse_map(&map).is_empty());
}

#[test]
fn an_edited_widget_keeps_no_stale_entry_and_after_compile_collects() {
    let cx = &Core::default();
    let (id, root) = project(cx, "after");
    with_fake(cx);
    run_before(cx, &id);
    let c = Cache::at(&root);
    let before: Vec<MapEntry> = parse_map(&std::fs::read_to_string(c.map("main")).unwrap());
    // The compile changed the chart's options: its old poster is unused.
    set_sidecar(cx, &id, |s| {
        s.replace("|height=142.26378pt|Ablation", "|height=100pt|Ablation")
    });
    let lines = run_after(cx, &id);
    let after = parse_map(&std::fs::read_to_string(c.map("main")).unwrap());
    assert_eq!(after.len(), 1, "the chart has no poster for its new record");
    assert_eq!(after[0], before[0]);
    let chart = &before[1];
    assert!(!c.png(&chart.key).exists(), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l == "posters: removed 1 unused, 0 over the cap"),
        "{lines:?}"
    );
}

#[test]
fn cached_poster_names_the_file_only_once_it_exists() {
    let cx = &Core::default();
    let (id, _root) = project(cx, "cached");
    let l = widgets(cx, &id, "main.tex").unwrap();
    let mesh = l.widgets.iter().find(|w| w.id == "fig-mesh").unwrap();
    let demo = l.widgets.iter().find(|w| w.id == "fig-demo").unwrap();
    assert!(cached_poster(cx, &id, "main.tex", mesh).is_none());
    with_fake(cx);
    run_before(cx, &id);
    let p = cached_poster(cx, &id, "main.tex", mesh).unwrap();
    assert!(p.ends_with(format!(
        "{CACHE_DIR}/{POSTERS_DIR}/{}.png",
        poster_key(cx, &id, "main.tex", mesh).unwrap()
    )));
    assert!(cached_poster(cx, &id, "main.tex", demo).is_none());
}

#[test]
fn without_a_renderer_posters_stay_placeholders() {
    let r = ProcessRenderer {
        exe: PathBuf::from("/nonexistent/maleficium"),
    };
    let cx = &Core::default();
    let (id, _root) = project(cx, "noproc");
    let req = PosterRequest {
        root_id: id,
        main_rel: "main.tex".into(),
        widget_id: "fig-mesh".into(),
        out_path: "/tmp/x.png".into(),
        timeout_ms: Some(500),
        digest: None,
    };
    let out = r.render(cx, &[req.clone(), req]);
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|r| r.is_err()));
}

// ---- html widgets: only approved ones render, map or come from the cache --

/// A renderer that goes through the core's run (approval checked on the
/// snapshot), executing what reaches it: the probe's script, if present,
/// leaves its marker.
struct Executing {
    base: PathBuf,
    marker: PathBuf,
    ran: Mutex<Vec<String>>,
}

impl PosterRenderer for Executing {
    fn render(&self, cx: &Core, reqs: &[PosterRequest]) -> Vec<Result<PosterOutcome, String>> {
        reqs.iter()
            .map(|r| {
                super::super::run_at(&self.base, cx, r, |job| {
                    self.ran.lock().unwrap().push(job.widget_id.clone());
                    if job.document.contains("MARKER-RAN") {
                        std::fs::write(&self.marker, "ran").unwrap();
                    }
                    let (w, h) = job.expect.unwrap_or((4, 3));
                    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
                    b.extend_from_slice(&w.to_be_bytes());
                    b.extend_from_slice(&h.to_be_bytes());
                    b.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
                    use base64::Engine;
                    Ok(format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(b)
                    ))
                })
            })
            .collect()
    }
}

#[test]
fn an_html_widget_gets_an_auto_poster_only_while_approved() {
    let cx = &Core::default();
    let (id, root) = project(cx, "html");
    // The demo gives no poster= and loads the probe.
    set_sidecar(cx, &id, |s| s.replace("|figures/demo.png|", "||"));
    let demo = root.join("widgets/demo");
    std::fs::write(
        demo.join("index.html"),
        "<!doctype html><script src=\"probe.js\"></script>",
    )
    .unwrap();
    std::fs::write(demo.join("probe.js"), "window.p='MARKER-RAN';").unwrap();
    let base = crate::test_scratch::dir("cache-html-appdata");
    let _ = std::fs::remove_dir_all(&base);
    let marker = crate::test_scratch::dir("cache-html-marker").join("probe.marker");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&marker);
    let r = Arc::new(Executing {
        base: base.clone(),
        marker: marker.clone(),
        ran: Mutex::new(Vec::new()),
    });
    cx.set_poster_renderer(r.clone());
    let before = |cx: &Core| {
        let mut lines = Vec::new();
        before_compile_at(&base, cx, &id, "main.tex", &mut |l| lines.push(l));
        lines
    };
    let demo_w = || {
        widgets(cx, &id, "main.tex")
            .unwrap()
            .widgets
            .into_iter()
            .find(|w| w.id == "fig-demo")
            .unwrap()
    };
    let c = Cache::at(&root);
    let mapped = || {
        parse_map(&std::fs::read_to_string(c.map("main")).unwrap_or_default())
            .into_iter()
            .any(|e| e.id == "fig-demo")
    };
    let target = WidgetTarget {
        id: "fig-demo".into(),
        path: "widgets/demo".into(),
        option_origins: Default::default(),
    };
    let approve = || {
        let d = widget_approval::check_at(&base, cx, &id, &target)
            .unwrap()
            .snapshot
            .digest;
        widget_approval::approve_at(
            &base,
            cx,
            &widget_approval::WidgetApproveParams {
                root_id: id.clone(),
                main_rel: "main.tex".into(),
                widget: "fig-demo".into(),
                digest: d,
            },
        )
        .unwrap();
    };

    // Never approved: the compile says so, nothing runs, it keeps its
    // placeholder.
    let lines = before(cx);
    assert!(
        lines.iter().any(|l| l
            .starts_with("poster fig-demo: approval_required (never_approved): Widget 'fig-demo'")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.ends_with(", 1 awaiting approval")),
        "{lines:?}"
    );
    assert!(!r.ran.lock().unwrap().contains(&"fig-demo".to_string()));
    assert!(!marker.exists(), "the unapproved probe ran");
    let needed = approvals_needed_at(&base, cx, &id, "main.tex");
    assert_eq!(needed.len(), 1, "{needed:?}");
    assert_eq!(needed[0].widget, "fig-demo");
    assert_eq!(
        needed[0].cause,
        maleficium_events::WidgetApprovalCause::NeverApproved
    );
    assert!(!mapped());
    assert!(cached_poster_at(&base, cx, &id, "main.tex", &demo_w()).is_none());

    // Approved: it renders into the cache and is mapped.
    approve();
    let lines = before(cx);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("poster fig-demo: rendered")),
        "{lines:?}"
    );
    assert!(marker.exists() && mapped());
    assert!(
        approvals_needed_at(&base, cx, &id, "main.tex").is_empty(),
        "an approved widget asks nothing"
    );
    let png = cached_poster_at(&base, cx, &id, "main.tex", &demo_w()).unwrap();
    assert!(png.is_file());

    // Edited: its old poster stays on disk but is never used for it.
    std::fs::remove_file(&marker).unwrap();
    std::fs::write(demo.join("probe.js"), "window.p='MARKER-RAN'; // v2").unwrap();
    let n = r.ran.lock().unwrap().len();
    let lines = before(cx);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("poster fig-demo: approval_required (changed_since_approval)")),
        "{lines:?}"
    );
    assert_eq!(r.ran.lock().unwrap().len(), n, "nothing more ran");
    assert_eq!(
        approvals_needed_at(&base, cx, &id, "main.tex")[0].cause,
        maleficium_events::WidgetApprovalCause::ChangedSinceApproval
    );
    assert!(!marker.exists());
    assert!(png.is_file(), "the old poster is still there");
    assert!(!mapped(), "a cache hit never stands in for the approval");
    assert!(cached_poster_at(&base, cx, &id, "main.tex", &demo_w()).is_none());
    let mut lines = Vec::new();
    after_compile_at(&base, cx, &id, "main.tex", &mut |l| lines.push(l));
    assert!(!mapped(), "{lines:?}");

    // Edited back to the approved bytes: the same key again, mapped again
    // (rendered anew, since the collection above dropped the unused file).
    std::fs::write(demo.join("probe.js"), "window.p='MARKER-RAN';").unwrap();
    let lines = before(cx);
    assert!(mapped(), "{lines:?}");
    let _ = std::fs::remove_file(&marker);

    // Revoked: unmapped, cached file or not.
    widget_approval::revoke_at(
        &base,
        cx,
        &widget_approval::WidgetRevokeParams {
            root_id: id.clone(),
            path: "widgets/demo".into(),
        },
    )
    .unwrap();
    let lines = before(cx);
    assert!(!mapped(), "{lines:?}");
    assert!(lines
        .iter()
        .any(|l| l.starts_with("poster fig-demo: approval_required (revoked)")));
    assert!(!marker.exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn an_html_widget_with_its_own_poster_still_asks_for_approval() {
    let cx = &Core::default();
    // The real sidecar: the demo keeps poster=figures/demo.png.
    let (id, root) = project(cx, "html-explicit");
    let base = crate::test_scratch::dir("cache-html-explicit-appdata");
    let _ = std::fs::remove_dir_all(&base);
    let needed = approvals_needed_at(&base, cx, &id, "main.tex");
    assert_eq!(needed.len(), 1, "{needed:?}");
    assert_eq!(needed[0].widget, "fig-demo");
    assert_eq!(needed[0].path, "widgets/demo");
    assert_eq!(
        needed[0].cause,
        maleficium_events::WidgetApprovalCause::NeverApproved
    );
    widget_approval::approve_at(
        &base,
        cx,
        &widget_approval::WidgetApproveParams {
            root_id: id.clone(),
            main_rel: "main.tex".into(),
            widget: "fig-demo".into(),
            digest: needed[0].digest.clone(),
        },
    )
    .unwrap();
    assert!(approvals_needed_at(&base, cx, &id, "main.tex").is_empty());
    std::fs::write(root.join("widgets/demo/index.html"), "<p>edited</p>").unwrap();
    let again = approvals_needed_at(&base, cx, &id, "main.tex");
    assert_eq!(
        again[0].cause,
        maleficium_events::WidgetApprovalCause::ChangedSinceApproval
    );
    assert_ne!(again[0].digest, needed[0].digest);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn an_html_widget_has_no_runtime_key() {
    let cx = &Core::default();
    let (id, _root) = project(cx, "htmlkey");
    let l = widgets(cx, &id, "main.tex").unwrap();
    let demo = l.widgets.iter().find(|w| w.id == "fig-demo").unwrap();
    assert!(poster_key(cx, &id, "main.tex", demo)
        .unwrap_err()
        .contains("approval digest"));
    let a = html_key(demo, "widgets/demo", &"a".repeat(64));
    assert_ne!(a, html_key(demo, "widgets/demo", &"b".repeat(64)));
    assert_ne!(a, html_key(demo, "widgets/other", &"a".repeat(64)));
    assert!(is_key(&a));
}
