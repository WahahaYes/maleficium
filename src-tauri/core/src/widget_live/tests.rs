use super::*;

const PDF: &[u8] = include_bytes!("../../testdata/interactive/main.pdf");
const SIDECAR: &str = include_str!("../../testdata/interactive/main.mfw");

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

/// The interactive fixture, compiled (the committed pdf and sidecar), with
/// the demo bundle declaring an origin.
fn project(cx: &Core, name: &str) -> String {
    let dir = crate::test_scratch::dir(&format!("live-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../e2e/fixtures/interactive");
    copy_dir(&fixture, &dir);
    std::fs::write(
        dir.join("widgets/demo/widget.json"),
        r#"{"csp": {"connectDomains": ["https://example.org"]}}"#,
    )
    .unwrap();
    let root = dunce::canonicalize(&dir).unwrap();
    let id = format!("live-{name}");
    crate::fs::grant_root(cx, &id, &root.to_string_lossy()).unwrap();
    let o = crate::outputs::outputs_of(cx, &id, "main.tex").unwrap();
    let _ = std::fs::remove_dir_all(&o.outdir);
    std::fs::create_dir_all(&o.outdir).unwrap();
    std::fs::write(o.outdir.join(&o.pdf_name), PDF).unwrap();
    std::fs::write(o.outdir.join("main.mfw"), SIDECAR).unwrap();
    id
}

fn entry<'a>(p: &'a LivePlan, id: &str) -> &'a WidgetPlanEntry {
    p.entries.iter().find(|e| e.id == id).unwrap()
}

fn live<'a>(p: &'a LivePlan, id: &str) -> &'a LiveWidget {
    p.live.iter().find(|w| w.id == id).unwrap()
}

#[test]
fn approvals_start_off_and_each_scope_is_granted_and_revoked_alone() {
    let dir = crate::test_scratch::dir("live-approvals");
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("approvals.json");
    assert_eq!(get_at(&path, "/p"), WidgetApproval::default());
    let a = set_at(&path, "/p".into(), WidgetApprovalScope::Run, true).unwrap();
    assert_eq!(
        a,
        WidgetApproval {
            run: true,
            network: false
        }
    );
    let a = set_at(&path, "/p".into(), WidgetApprovalScope::Network, true).unwrap();
    assert_eq!(
        a,
        WidgetApproval {
            run: true,
            network: true
        }
    );
    assert_eq!(
        get_at(&path, "/q"),
        WidgetApproval::default(),
        "per project"
    );
    let a = set_at(&path, "/p".into(), WidgetApprovalScope::Run, false).unwrap();
    assert_eq!(
        a,
        WidgetApproval {
            run: false,
            network: true
        }
    );
    set_at(&path, "/p".into(), WidgetApprovalScope::Network, false).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), "{}");
    std::fs::write(&path, "{nope").unwrap();
    assert_eq!(get_at(&path, "/p"), WidgetApproval::default());
}

#[test]
fn a_fresh_project_runs_nothing() {
    let cx = &Core::default();
    let id = project(cx, "fresh");
    let p = live_plan_with(cx, &id, "main.tex", WidgetApproval::default()).unwrap();
    assert!(p.live.is_empty());
    assert_eq!(p.entries.len(), 5);
    assert!(p
        .entries
        .iter()
        .all(|e| e.poster == Some(PosterReason::NotApproved) && !e.network));
    assert!(entry(&p, "fig-demo").declares_network);
}

#[test]
fn run_approval_starts_what_has_a_runtime_with_network_still_denied() {
    let cx = &Core::default();
    let id = project(cx, "run");
    let a = WidgetApproval {
        run: true,
        network: false,
    };
    let p = live_plan_with(cx, &id, "main.tex", a).unwrap();
    let ids: Vec<&str> = p.live.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["tab-results", "fig-chart", "fig-demo"]);
    assert_eq!(entry(&p, "fig-mesh").poster, Some(PosterReason::NoRuntime));
    assert_eq!(entry(&p, "fig-clip").poster, Some(PosterReason::NoRuntime));
    let demo = live(&p, "fig-demo");
    assert!(!demo.network && !entry(&p, "fig-demo").network);
    assert!(demo.csp.contains("connect-src 'none'"));
    assert!(!demo.csp.contains("example.org"));
    assert!(!demo.document.contains("example.org"));
    for w in &p.live {
        assert!(w.document.contains("Content-Security-Policy"), "{}", w.id);
        assert!(!w.csp.contains("'self'"), "{}", w.id);
    }
    let table = live(&p, "tab-results");
    assert_eq!(table.runtime, "table@1");
    let csv = std::fs::read(crate::fs::resolve_in(cx, &id, "data/results.csv").unwrap()).unwrap();
    assert_eq!(table.sources.len(), 1);
    assert_eq!(table.sources[0].role, "data");
    assert_eq!(table.sources[0].bytes, csv);
    assert_eq!(table.sources[0].sha256, bundle::sha_of(&csv));
    assert_eq!(table.options["pdfrows"], 2);
}

#[test]
fn network_approval_opens_only_the_declared_origins() {
    let cx = &Core::default();
    let id = project(cx, "net");
    let a = WidgetApproval {
        run: true,
        network: true,
    };
    let p = live_plan_with(cx, &id, "main.tex", a).unwrap();
    let demo = live(&p, "fig-demo");
    assert!(demo.network && entry(&p, "fig-demo").network);
    assert!(demo.csp.contains("connect-src https://example.org"));
    // A widget that declares nothing gains nothing.
    let chart = live(&p, "fig-chart");
    assert!(!chart.network);
    assert!(chart.csp.contains("connect-src 'none'"));
    // Network approval without run approval runs nothing.
    let p = live_plan_with(
        cx,
        &id,
        "main.tex",
        WidgetApproval {
            run: false,
            network: true,
        },
    )
    .unwrap();
    assert!(p.live.is_empty());
}

#[test]
fn house_tokens_follow_the_mode() {
    let light = house_tokens(false);
    let dark = house_tokens(true);
    assert_eq!(light["--m-color-bg"], "#fbfaf7");
    assert_eq!(dark["--m-color-bg"], "#16181d");
    assert_eq!(light["--m-radius"], dark["--m-radius"]);
    assert!(light.len() >= 21 && light.len() == dark.len(), "{light:?}");
}
