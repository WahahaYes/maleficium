use super::*;
use std::path::PathBuf;

/// A real compile of e2e/fixtures/interactive (Tectonic, the shipped
/// package): its pdf keeps the annotations in object streams.
const REAL_PDF: &[u8] = include_bytes!("../../testdata/interactive/main.pdf");
const REAL_SIDECAR: &str = include_str!("../../testdata/interactive/main.mfw");

/// A project with `main` (relative, e.g. `main.tex` or `paper/main.tex`)
/// and its outdir holding the given pdf and sidecar. Returns the root id,
/// the project root and the outdir (to clean up).
fn compiled(
    cx: &Core,
    name: &str,
    main: &str,
    pdf: Option<&[u8]>,
    sidecar: Option<&str>,
) -> (String, PathBuf, PathBuf) {
    let dir = crate::test_scratch::dir(&format!("widgets-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    let main_path = dir.join(main);
    std::fs::create_dir_all(main_path.parent().unwrap()).unwrap();
    std::fs::write(&main_path, "x").unwrap();
    let root = dunce::canonicalize(&dir).unwrap();
    let id = format!("widgets-{name}");
    crate::fs::grant_root(cx, &id, &root.to_string_lossy()).unwrap();
    let o = crate::outputs::outputs_of(cx, &id, main).unwrap();
    let _ = std::fs::remove_dir_all(&o.outdir);
    std::fs::create_dir_all(&o.outdir).unwrap();
    let stem = o.main_file.strip_suffix(".tex").unwrap();
    if let Some(pdf) = pdf {
        std::fs::write(o.outdir.join(&o.pdf_name), pdf).unwrap();
    }
    if let Some(side) = sidecar {
        std::fs::write(o.outdir.join(format!("{stem}.mfw")), side).unwrap();
    }
    (id, root, o.outdir)
}

fn by_id<'a>(l: &'a WidgetList, id: &str) -> &'a Widget {
    l.widgets.iter().find(|w| w.id == id).unwrap()
}

#[test]
fn the_real_pdf_yields_every_rect_through_its_object_streams() {
    // Annotations sit inside compressed object streams: a byte scan finds
    // no /NM at all, so the parser has to decompress them.
    assert!(
        !REAL_PDF.windows(8).any(|w| w == b"mfw:fig-"),
        "the fixture pdf was meant to hide its annotations in object streams"
    );
    let found = marks(REAL_PDF.to_vec()).unwrap();
    let got: Vec<(&str, u32, [f64; 4])> = found
        .iter()
        .map(|m| {
            (
                m.id.as_str(),
                m.page,
                [m.rect.x0, m.rect.y0, m.rect.x1, m.rect.y1],
            )
        })
        .collect();
    let want: [(&str, u32, [f64; 4]); 5] = [
        ("fig-mesh", 1, [305.624, 476.975, 476.007, 647.054]),
        ("tab-results", 1, [133.768, 323.662, 203.148, 360.723]),
        ("fig-clip", 2, [305.624, 525.466, 447.61, 667.198]),
        ("fig-chart", 2, [148.712, 271.056, 290.698, 412.788]),
        ("fig-demo", 3, [148.712, 440.426, 375.89, 667.198]),
    ];
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (id, page, rect) in want {
        let (_, p, r) = got
            .iter()
            .find(|g| g.0 == id)
            .unwrap_or_else(|| panic!("{id}"));
        assert_eq!(*p, page, "{id}");
        for (a, b) in r.iter().zip(rect) {
            assert!((a - b).abs() < 0.01, "{id}: {r:?} vs {rect:?}");
        }
        assert!(r[0] < r[2] && r[1] < r[3], "{id}");
    }
}

#[test]
fn the_fixture_joins_into_a_typed_list_in_document_order() {
    let cx = &Core::default();
    let (id, root, out) = compiled(cx, "full", "main.tex", Some(REAL_PDF), Some(REAL_SIDECAR));
    let bundle = root.join("widgets/demo");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(bundle.join("index.html"), "<p>hi</p>").unwrap();

    let l = widgets(cx, &id, "main.tex").unwrap();
    let order: Vec<&str> = l.widgets.iter().map(|w| w.id.as_str()).collect();
    // Page, then top to bottom: page 1 mesh (y 647) above the table (y 360).
    assert_eq!(
        order,
        [
            "fig-mesh",
            "tab-results",
            "fig-clip",
            "fig-chart",
            "fig-demo"
        ]
    );
    let mesh = by_id(&l, "fig-mesh");
    assert_eq!(mesh.kind, WidgetType::Model);
    assert_eq!(mesh.runtime.as_deref(), Some("model@1"));
    assert_eq!(
        (mesh.page, mesh.label.as_deref(), mesh.figure.as_deref()),
        (1, Some("fig:mesh"), Some("1"))
    );
    assert_eq!(
        mesh.sources,
        [WidgetSource {
            role: "model".into(),
            path: "models/mesh.glb".into()
        }]
    );
    assert_eq!(mesh.poster.as_deref(), Some("figures/mesh.png"));
    assert!(mesh.alt.starts_with("Avatar mesh"));
    assert_eq!(mesh.theme, "house");
    assert!((mesh.rect.x0 - 305.624).abs() < 0.01 && mesh.rect.y1 > mesh.rect.y0);
    assert!(mesh.csp.is_none());

    // A pre-caption widget records no label rather than a stale one.
    let clip = by_id(&l, "fig-clip");
    assert_eq!(
        (clip.page, clip.label.clone(), clip.figure.clone()),
        (2, None, None)
    );
    // Widgets outside a captioned float record no label or figure either:
    // the previous float's caption must not leak into them.
    for id in ["tab-results", "fig-chart", "fig-demo"] {
        let w = by_id(&l, id);
        assert_eq!((w.label.clone(), w.figure.clone()), (None, None), "{id}");
    }
    // The empty remote= and sha256= entries are not listed.
    assert_eq!(
        clip.options
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["height"]
    );

    let table = by_id(&l, "tab-results");
    assert_eq!(
        (table.kind, table.poster.clone()),
        (WidgetType::Table, None)
    );
    assert_eq!(
        table.options,
        [WidgetOption {
            key: "pdfrows".into(),
            value: "2".into()
        }]
    );

    let html = by_id(&l, "fig-demo");
    assert_eq!(
        (html.kind, html.runtime.clone(), html.page),
        (WidgetType::Html, None, 3)
    );
    assert_eq!(html.sources[0].role, "bundle");
    assert!(html.csp.is_none(), "no widget.json declares nothing");

    let json = serde_json::to_value(&l).unwrap();
    assert_eq!(json["widgets"][0]["type"], "model");
    assert_eq!(json["widgets"][0]["rect"]["x0"], mesh.rect.x0);
    assert!(json["widgets"][2].get("label").is_none());
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn a_plain_pdf_without_a_sidecar_lists_nothing_but_an_annotated_one_fails() {
    let cx = &Core::default();
    let (id, _r, out) = compiled(cx, "plain", "main.tex", Some(&plain_pdf()), None);
    assert_eq!(widgets(cx, &id, "main.tex").unwrap().widgets, []);
    let _ = std::fs::remove_dir_all(out);

    let (id, _r, out) = compiled(cx, "nosidecar", "main.tex", Some(REAL_PDF), None);
    let e = widgets(cx, &id, "main.tex").unwrap_err();
    assert!(
        e.contains("no widget sidecar") && e.contains("fig-mesh"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn a_sidecar_that_disagrees_with_the_pdf_is_stale_and_names_the_ids() {
    let cx = &Core::default();
    // A widget the sidecar knows that the pdf never drew.
    let extra = format!(
        "{REAL_SIDECAR}widget|gone|chart|chart@1|||house|p.png|spec=a.json|height=1pt|Gone\n"
    );
    let (id, _r, out) = compiled(cx, "stale1", "main.tex", Some(REAL_PDF), Some(&extra));
    let e = widgets(cx, &id, "main.tex").unwrap_err();
    assert!(
        e.contains("in the sidecar but not the pdf: gone") && e.contains("recompile"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(out);

    // A pdf annotation the sidecar lost.
    let short: String = REAL_SIDECAR
        .lines()
        .filter(|l| !l.starts_with("widget|fig-clip|"))
        .map(|l| format!("{l}\n"))
        .collect();
    let (id, _r, out) = compiled(cx, "stale2", "main.tex", Some(REAL_PDF), Some(&short));
    let e = widgets(cx, &id, "main.tex").unwrap_err();
    assert!(
        e.contains("in the pdf but not the sidecar: fig-clip"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(out);

    // All widgets removed and recompiled: the old sidecar outlives them.
    let (id, _r, out) = compiled(
        cx,
        "stale3",
        "main.tex",
        Some(&plain_pdf()),
        Some(REAL_SIDECAR),
    );
    let e = widgets(cx, &id, "main.tex").unwrap_err();
    assert!(e.contains("in the sidecar but not the pdf"), "{e}");
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn a_main_that_never_compiled_or_a_bad_pdf_is_an_error() {
    let cx = &Core::default();
    let (id, _r, out) = compiled(cx, "none", "main.tex", None, Some(REAL_SIDECAR));
    let e = widgets(cx, &id, "main.tex").unwrap_err();
    assert!(e.contains("no compiled pdf"), "{e}");
    let _ = std::fs::remove_dir_all(out);

    let (id, _r, out) = compiled(
        cx,
        "garbage",
        "main.tex",
        Some(b"not a pdf"),
        Some(REAL_SIDECAR),
    );
    assert!(widgets(cx, &id, "main.tex")
        .unwrap_err()
        .contains("cannot read the pdf"));
    let _ = std::fs::remove_dir_all(out);

    assert!(widgets(cx, "nope", "main.tex").is_err());
    let (id, _r, out) = compiled(cx, "esc", "main.tex", None, None);
    for bad in crate::test_scratch::escapes() {
        assert!(widgets(cx, &id, bad).is_err(), "{bad}");
    }
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn a_malformed_sidecar_fails_loudly() {
    let cx = &Core::default();
    let line = "widget|fig-mesh|model|model@1|fig:mesh|1|house|figures/mesh.png|model=models/mesh.glb|height=1pt|Alt";
    let cases: [(&str, String, &str); 7] = [
        (
            "hdr",
            format!("mfw 2\n{line}\n"),
            "unsupported widget sidecar",
        ),
        (
            "fields",
            "mfw 1\nwidget|a|model\n".to_string(),
            "3 fields, expected 11",
        ),
        ("kind", "mfw 1\nthing|a\n".to_string(), "unknown record"),
        ("dup", format!("mfw 1\n{line}\n{line}\n"), "duplicate id"),
        (
            "type",
            format!("mfw 1\n{}\n", line.replace("|model|", "|sphere|")),
            "unknown type `sphere`",
        ),
        (
            "pair",
            format!("mfw 1\n{}\n", line.replace("height=1pt", "height")),
            "malformed option",
        ),
        (
            "alt",
            format!("mfw 1\n{}\n", line.replace("|Alt", "| ")),
            "no alt",
        ),
    ];
    for (name, side, want) in cases {
        let (id, _r, out) = compiled(
            cx,
            &format!("bad-{name}"),
            "main.tex",
            Some(REAL_PDF),
            Some(&side),
        );
        let e = widgets(cx, &id, "main.tex").unwrap_err();
        assert!(e.contains(want), "{name}: {e}");
        let _ = std::fs::remove_dir_all(out);
    }
}

#[test]
fn poster_parameters_are_canonical_in_the_list_and_bad_ones_fail() {
    let cx = &Core::default();
    let side = REAL_SIDECAR
        .replace(
            "height=170.71652pt|",
            "height=170.71652pt,camera=pos=0 0 5 target=0 0 0,size=640x480,background=FFF|",
        )
        .replace(
            "|spec=charts/ablation.vl.json|height=142.26378pt|",
            "|spec=charts/ablation.vl.json|height=142.26378pt,scale=1.50|",
        );
    let (id, root, out) = compiled(cx, "params", "main.tex", Some(REAL_PDF), Some(&side));
    std::fs::create_dir_all(root.join("widgets/demo")).unwrap();
    std::fs::write(root.join("widgets/demo/index.html"), "<p>hi</p>").unwrap();
    let l = widgets(cx, &id, "main.tex").unwrap();
    let opt = |w: &str, k: &str| {
        by_id(&l, w)
            .options
            .iter()
            .find(|o| o.key == k)
            .map(|o| o.value.clone())
    };
    assert_eq!(
        opt("fig-mesh", "camera").as_deref(),
        Some("1 0 0 0 0 1 0 0 0 0 1 0 0 0 5 1")
    );
    assert_eq!(opt("fig-mesh", "size").as_deref(), Some("640x480"));
    assert_eq!(opt("fig-mesh", "background").as_deref(), Some("#ffffff"));
    assert_eq!(opt("fig-chart", "scale").as_deref(), Some("1.5"));
    let _ = std::fs::remove_dir_all(out);

    for (name, from, to, want) in [
        (
            "cam",
            "height=170.71652pt|",
            "height=170.71652pt,camera=1 2 3|",
            "widget fig-mesh: a camera matrix has 16 numbers",
        ),
        (
            "size",
            "height=170.71652pt|",
            "height=170.71652pt,size=big|",
            "widget fig-mesh: size= is WxH",
        ),
        (
            "scale",
            "height=170.71652pt|",
            "height=170.71652pt,scale=2|",
            "widget fig-mesh: scale= does not apply",
        ),
    ] {
        let (id, _r, out) = compiled(
            cx,
            &format!("params-{name}"),
            "main.tex",
            Some(REAL_PDF),
            Some(&REAL_SIDECAR.replace(from, to)),
        );
        let e = widgets(cx, &id, "main.tex").unwrap_err();
        assert!(e.contains(want), "{name}: {e}");
        let _ = std::fs::remove_dir_all(out);
    }
}

#[test]
fn only_widget_annotations_count_and_reversed_rects_are_normalised() {
    let marks = marks(annotated_pdf()).unwrap();
    assert_eq!(marks.len(), 1);
    assert_eq!((marks[0].id.as_str(), marks[0].page), ("a-1", 1));
    assert_eq!(
        marks[0].rect,
        WidgetRect {
            x0: 10.5,
            y0: 20.0,
            x1: 110.5,
            y1: 220.0
        }
    );
}

fn html_sidecar(bundle: &str) -> String {
    format!("mfw 1\nwidget|a-1|html||||house|p.png|bundle={bundle}|height=1pt|Demo\n")
}

#[test]
fn an_html_widgets_csp_comes_from_its_bundle_manifest_beside_the_main_file() {
    let cx = &Core::default();
    // The main file is in a subfolder: the bundle path is relative to it.
    let (id, root, out) = compiled(
        cx,
        "csp",
        "paper/main.tex",
        Some(&annotated_pdf()),
        Some(&html_sidecar("w/demo/")),
    );
    let b = root.join("paper/w/demo");
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(b.join("index.html"), "x").unwrap();
    assert_eq!(
        widgets(cx, &id, "paper/main.tex").unwrap().widgets[0].csp,
        None
    );

    let put = |json: &str| std::fs::write(b.join("widget.json"), json).unwrap();
    put(
        r#"{"csp":{"connectDomains":["https://api.example.org"],"frameDomains":["https://x.org:8443"]}}"#,
    );
    let csp = widgets(cx, &id, "paper/main.tex").unwrap().widgets[0]
        .csp
        .clone()
        .unwrap();
    assert_eq!(csp.connect_domains, ["https://api.example.org"]);
    assert_eq!(csp.frame_domains, ["https://x.org:8443"]);
    assert!(csp.resource_domains.is_empty());

    put(r#"{"csp":{}}"#);
    assert_eq!(
        widgets(cx, &id, "paper/main.tex").unwrap().widgets[0].csp,
        None
    );
    put(r#"{"name":"demo"}"#);
    assert_eq!(
        widgets(cx, &id, "paper/main.tex").unwrap().widgets[0].csp,
        None
    );

    // Anything that could reshape a header is refused, and so is a manifest
    // that does not parse: never a silent empty policy.
    for (bad, want) in [
        (
            r#"{"csp":{"connectDomains":["http://a.org"]}}"#,
            "only https origins",
        ),
        (
            r#"{"csp":{"connectDomains":["https://a.org; script-src *"]}}"#,
            "only https origins",
        ),
        (
            r#"{"csp":{"resourceDomains":["https://A.org"]}}"#,
            "only https origins",
        ),
        (
            r#"{"csp":{"resourceDomains":["https://a.org/path"]}}"#,
            "only https origins",
        ),
        (
            r#"{"csp":{"resourceDomains":["https://"]}}"#,
            "only https origins",
        ),
        (
            r#"{"csp":{"scriptDomains":["https://a.org"]}}"#,
            "unknown field",
        ),
        ("{not json", "not valid JSON"),
    ] {
        put(bad);
        let e = widgets(cx, &id, "paper/main.tex").unwrap_err();
        assert!(e.contains(want), "{bad}: {e}");
    }
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn a_bundle_folder_that_is_missing_or_outside_the_project_fails() {
    let cx = &Core::default();
    let (id, _r, out) = compiled(
        cx,
        "nobundle",
        "main.tex",
        Some(&annotated_pdf()),
        Some(&html_sidecar("w/demo/")),
    );
    let e = widgets(cx, &id, "main.tex").unwrap_err();
    assert!(e.contains("bundle folder"), "{e}");
    let _ = std::fs::remove_dir_all(out);

    let (id, _r, out) = compiled(
        cx,
        "escbundle",
        "main.tex",
        Some(&annotated_pdf()),
        Some(&html_sidecar("../../..")),
    );
    let e = widgets(cx, &id, "main.tex").unwrap_err();
    assert!(e.contains("forbidden path"), "{e}");
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn outcomes_become_typed_events() {
    use maleficium_events::{Actor, AppEvent, EventKind};
    let ok = event(
        "main.tex",
        &Ok(WidgetList { widgets: vec![] }),
        Actor::Agent,
    );
    assert_eq!(ok.kind, EventKind::Success);
    assert!(matches!(ok.event, AppEvent::WidgetsRead { ref main, count: 0 } if main == "main.tex"));
    let err = event("main.tex", &Err("stale".into()), Actor::User);
    assert_eq!((err.kind, err.actor), (EventKind::Error, Actor::User));
    assert!(matches!(err.event, AppEvent::WidgetsFailed { ref error, .. } if error == "stale"));
}

/// A one-page pdf with no annotations.
fn plain_pdf() -> Vec<u8> {
    build_pdf("/Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]")
}

/// A one-page pdf with three annotations: ours (reversed rect), someone
/// else's name, and a link with no name at all.
fn annotated_pdf() -> Vec<u8> {
    build_pdf(
        "/Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [\
         << /Type /Annot /Subtype /Text /NM (mfw:a-1) /Rect [110.5 220 10.5 20] >> \
         << /Type /Annot /Subtype /Text /NM (other:b) /Rect [1 2 3 4] >> \
         << /Type /Annot /Subtype /Link /Rect [5 6 7 8] >>]",
    )
}

fn build_pdf(page: &str) -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< {page} >>"),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        out.extend(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objs.len() + 1,
            xref
        )
        .as_bytes(),
    );
    out
}

#[test]
fn an_origin_is_https_host_and_port_and_nothing_else() {
    for good in [
        "https://www.youtube-nocookie.com",
        "https://player.vimeo.com",
        "https://127.0.0.1:8443",
        "https://a.org:65535",
        "https://x",
        &format!("https://{}.org", "a".repeat(63)),
    ] {
        assert!(origin_ok(good), "{good}");
    }
    for bad in [
        "http://a.org",
        "HTTPS://a.org",
        "https://A.org",
        "https://*.a.org",
        "https://*",
        "*",
        "'self'",
        "'none'",
        "https:",
        "https://",
        "https://a.org/",
        "https://a.org/path",
        "https://a.org?x",
        "https://a.org#x",
        "https://user@a.org",
        "https://user:pw@a.org",
        "https://a.org:",
        "https://a.org:0",
        "https://a.org:0443",
        "https://a.org:65536",
        "https://a.org:443:1",
        "https://a..org",
        "https://.a.org",
        "https://a.org.",
        "https://-a.org",
        "https://a-.org",
        "https://a.org; script-src *",
        "https://a.org https://b.org",
        "https://a.org\n",
        "https://[::1]",
        "https://xn--bcher-kva.example\u{0}",
        "data:text/html,x",
        "blob:https://a.org/x",
        "javascript:alert(1)",
        "ws://a.org",
        "wss://a.org",
        "filesystem:https://a.org",
        &format!("https://{}.org", "a".repeat(64)),
        &format!("https://{}", ["abcdefghij"; 26].join(".")),
    ] {
        assert!(!origin_ok(bad), "{bad:?}");
    }
}

fn html_sidecar_with(options: &str) -> String {
    format!("mfw 1\nwidget|a-1|html||||house|p.png|bundle=w/demo/|height=1pt{options}|Demo\n")
}

/// One html widget in `w/demo/` whose sidecar options end with `options`.
fn embed_project(cx: &Core, name: &str, options: &str) -> (String, PathBuf, PathBuf) {
    let (id, root, out) = compiled(
        cx,
        name,
        "main.tex",
        Some(&annotated_pdf()),
        Some(&html_sidecar_with(options)),
    );
    let b = root.join("w/demo");
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(b.join("index.html"), "x").unwrap();
    (id, root, out)
}

#[test]
fn macro_options_declare_frame_and_resource_origins_joined_with_widget_json() {
    let cx = &Core::default();
    let (id, root, out) = embed_project(
        cx,
        "optorigins",
        ",framedomains=https://b.org https://a.org https://b.org,resourcedomains=https://cdn.org",
    );
    let b = root.join("w/demo");
    let w = widgets(cx, &id, "main.tex").unwrap().widgets[0].clone();
    let csp = w.csp.clone().unwrap();
    assert_eq!(csp.frame_domains, ["https://a.org", "https://b.org"]);
    assert_eq!(csp.resource_domains, ["https://cdn.org"]);
    // The option itself is kept in canonical form: sorted, deduplicated.
    assert!(w
        .options
        .iter()
        .any(|o| o.key == "framedomains" && o.value == "https://a.org https://b.org"));

    std::fs::write(
        b.join("widget.json"),
        r#"{"csp":{"frameDomains":["https://c.org","https://a.org"],"connectDomains":["https://api.org"]}}"#,
    )
    .unwrap();
    let csp = widgets(cx, &id, "main.tex").unwrap().widgets[0]
        .csp
        .clone()
        .unwrap();
    assert_eq!(
        csp.frame_domains,
        ["https://a.org", "https://b.org", "https://c.org"]
    );
    assert_eq!(csp.connect_domains, ["https://api.org"]);
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn bad_origin_options_and_too_many_origins_fail_the_list() {
    let many: Vec<String> = (0..MAX_ORIGINS + 1)
        .map(|i| format!("https://h{i}.org"))
        .collect();
    for (i, (opts, want)) in [
        (",framedomains=http://a.org", "only https origins"),
        (",framedomains=https://*.a.org", "only https origins"),
        (",resourcedomains=https://a.org/x", "only https origins"),
        (",framedomains=data:", "only https origins"),
        (",framedomains=https://u@a.org", "only https origins"),
        (&format!(",framedomains={}", many.join(" ")), "at most"),
    ]
    .into_iter()
    .enumerate()
    {
        let cx = &Core::default();
        let (id, _root, out) = embed_project(cx, &format!("badopt{i}"), opts);
        let e = widgets(cx, &id, "main.tex").unwrap_err();
        assert!(e.contains(want), "{opts}: {e}");
        let _ = std::fs::remove_dir_all(out);
    }
    // Over the cap only together: options and widget.json share one budget.
    let half: Vec<String> = (0..MAX_ORIGINS / 2 + 1)
        .map(|i| format!("https://o{i}.org"))
        .collect();
    let cx = &Core::default();
    let (id, root, out) = embed_project(cx, "budget", &format!(",framedomains={}", half.join(" ")));
    assert!(widgets(cx, &id, "main.tex").is_ok());
    let more: Vec<String> = (0..MAX_ORIGINS / 2)
        .map(|i| format!("\"https://m{i}.org\""))
        .collect();
    std::fs::write(
        root.join("w/demo/widget.json"),
        format!(r#"{{"csp":{{"resourceDomains":[{}]}}}}"#, more.join(",")),
    )
    .unwrap();
    let e = widgets(cx, &id, "main.tex").unwrap_err();
    assert!(e.contains("at most"), "{e}");
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn origin_options_apply_only_to_html_widgets() {
    let e = params::canonical(WidgetType::Model, "framedomains", "https://a.org").unwrap_err();
    assert!(e.contains("does not apply"), "{e}");
    assert_eq!(
        params::canonical(
            WidgetType::Html,
            "framedomains",
            " https://b.org  https://a.org "
        )
        .unwrap(),
        "https://a.org https://b.org"
    );
}
