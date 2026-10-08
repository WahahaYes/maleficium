use super::*;

/// A real compile of e2e/fixtures/interactive: `fig-demo` is the html
/// widget, bundle `widgets/demo/` beside `main.tex`; the other four are
/// first-party runtime widgets.
const REAL_PDF: &[u8] = include_bytes!("../../testdata/interactive/main.pdf");
const REAL_SIDECAR: &str = include_str!("../../testdata/interactive/main.mfw");

struct Project {
    cx: Core,
    id: String,
    root: PathBuf,
    /// The approval store home for this test (stands in for app data).
    base: PathBuf,
    out: PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.out);
    }
}

impl Project {
    fn demo(&self) -> PathBuf {
        self.root.join("widgets/demo")
    }
    fn target(&self) -> WidgetTarget {
        WidgetTarget {
            id: "fig-demo".into(),
            path: "widgets/demo".into(),
            option_origins: Default::default(),
        }
    }
    fn check(&self) -> Checked {
        check_at(&self.base, &self.cx, &self.id, &self.target()).unwrap()
    }
    fn status(&self) -> WidgetsStatus {
        status_at(&self.base, &self.cx, &self.id, "main.tex").unwrap()
    }
    fn approve(&self) -> WidgetApprovalStatus {
        let digest = self.check().snapshot.digest;
        approve_at(&self.base, &self.cx, &self.approve_params(&digest)).unwrap()
    }
    fn approve_params(&self, digest: &str) -> WidgetApproveParams {
        WidgetApproveParams {
            root_id: self.id.clone(),
            main_rel: "main.tex".into(),
            widget: "fig-demo".into(),
            digest: digest.into(),
        }
    }
    fn auto(&self, on: bool) {
        set_auto_at(
            &self.base,
            &self.cx,
            &WidgetAutoApproveParams {
                root_id: self.id.clone(),
                on,
            },
        )
        .unwrap();
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.demo().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    /// The same project in a fresh core (an app restart): roots re-granted.
    fn restarted(&self) -> Core {
        let cx = Core::default();
        crate::fs::grant_root(&cx, &self.id, &self.root.to_string_lossy()).unwrap();
        cx
    }
}

/// A compiled copy of the fixture's shape: main.tex, its pdf and sidecar in
/// the outdir, and the demo widget folder.
fn project(name: &str) -> Project {
    let cx = Core::default();
    let dir = crate::test_scratch::dir(&format!("approval-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("widgets/demo")).unwrap();
    std::fs::write(dir.join("main.tex"), "x").unwrap();
    std::fs::write(
        dir.join("widgets/demo/index.html"),
        "<!doctype html><p>demo</p>",
    )
    .unwrap();
    let root = dunce::canonicalize(&dir).unwrap();
    let id = format!("approval-{name}");
    crate::fs::grant_root(&cx, &id, &root.to_string_lossy()).unwrap();
    let o = crate::outputs::outputs_of(&cx, &id, "main.tex").unwrap();
    let _ = std::fs::remove_dir_all(&o.outdir);
    std::fs::create_dir_all(&o.outdir).unwrap();
    std::fs::write(o.outdir.join(&o.pdf_name), REAL_PDF).unwrap();
    std::fs::write(o.outdir.join("main.mfw"), REAL_SIDECAR).unwrap();
    let base = crate::test_scratch::dir(&format!("approval-{name}-appdata"));
    let _ = std::fs::remove_dir_all(&base);
    Project {
        cx,
        id,
        root,
        base,
        out: o.outdir,
    }
}

fn required(s: &WidgetApprovalStatus) -> &ApprovalRequired {
    match s {
        WidgetApprovalStatus::ApprovalRequired(r) => r,
        other => panic!("expected approval_required, got {other:?}"),
    }
}

fn approved(s: &WidgetApprovalStatus) -> &WidgetApproved {
    match s {
        WidgetApprovalStatus::Approved(a) => a,
        other => panic!("expected approved, got {other:?}"),
    }
}

fn files(pairs: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
    pairs
        .iter()
        .map(|(p, b)| (p.to_string(), b.as_bytes().to_vec()))
        .collect()
}

fn csp(connect: &[&str], frame: &[&str]) -> WidgetCsp {
    WidgetCsp {
        connect_domains: connect.iter().map(|s| s.to_string()).collect(),
        resource_domains: Vec::new(),
        frame_domains: frame.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn the_digest_is_stable_and_pinned() {
    let none = WidgetCsp::default();
    let a = files(&[("index.html", "<p>hi</p>"), ("js/app.js", "x()")]);
    // Pinned (checked against an independent Python implementation): a
    // format change must be deliberate, since it unapproves everything.
    assert_eq!(
        digest(&a, &none),
        "0d004cdb43aac0a72754cdd95eae912ccceb741c31f7518f91aed079a14b4d6d"
    );
    assert_eq!(digest(&a, &none), digest(&a.clone(), &none));
    assert_eq!(digest(&BTreeMap::new(), &none).len(), 64);
}

#[test]
fn the_digest_covers_every_path_byte_and_origin_without_ambiguity() {
    let none = WidgetCsp::default();
    let base = digest(&files(&[("ab", "c")]), &none);
    for other in [
        files(&[("a", "bc")]),
        files(&[("ab", "c ")]),
        files(&[("AB", "c")]),
        files(&[("ab", "c"), ("empty", "")]),
        files(&[("x/ab", "c")]),
    ] {
        assert_ne!(digest(&other, &none), base, "{other:?}");
    }
    // Origins are covered, per directive, order-insensitive.
    let f = files(&[("index.html", "x")]);
    let plain = digest(&f, &none);
    let one = digest(&f, &csp(&["https://a.org"], &[]));
    assert_ne!(one, plain);
    assert_ne!(one, digest(&f, &csp(&[], &["https://a.org"])));
    assert_eq!(
        digest(&f, &csp(&["https://b.org", "https://a.org"], &[])),
        digest(
            &f,
            &csp(&["https://a.org", "https://b.org", "https://a.org"], &[])
        )
    );
}

#[test]
fn a_snapshot_reads_the_folder_once_and_names_it_canonically() {
    let p = project("snapshot");
    p.write("js/app.js", "run()");
    let c = p.check();
    let s = &c.snapshot;
    assert_eq!(s.path, "widgets/demo");
    assert_eq!(
        s.files.keys().map(String::as_str).collect::<Vec<_>>(),
        ["index.html", "js/app.js"]
    );
    assert_eq!(s.digest, digest(&s.files, &s.origins));
    // The status carries the same digest the snapshot was hashed to.
    assert_eq!(required(&c.status).digest, s.digest);
    // Another spelling of the same folder is the same key.
    let via = WidgetTarget {
        id: "fig-demo".into(),
        path: "widgets/../widgets/demo/".into(),
        option_origins: Default::default(),
    };
    let again = check_at(&p.base, &p.cx, &p.id, &via).unwrap().snapshot;
    assert_eq!(again.path, "widgets/demo");
    assert_eq!(again.digest, s.digest);
}

#[test]
fn a_widget_path_outside_the_project_is_refused() {
    let p = project("escape");
    let outside = crate::test_scratch::dir("approval-escape-outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("index.html"), "x").unwrap();
    let mut bad: Vec<String> = crate::test_scratch::escapes()
        .into_iter()
        .map(String::from)
        .collect();
    bad.extend([
        "../approval-escape-outside".to_string(),
        outside.to_string_lossy().into_owned(),
        "widgets/demo/index.html".to_string(),
        "widgets/missing".to_string(),
        "a\0b".to_string(),
        String::new(),
    ]);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, p.root.join("widgets/linked")).unwrap();
        bad.push("widgets/linked".to_string());
    }
    for path in bad {
        let t = WidgetTarget {
            id: "fig-demo".into(),
            path: path.clone(),
            option_origins: Default::default(),
        };
        assert!(check_at(&p.base, &p.cx, &p.id, &t).is_err(), "{path:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_folder_with_a_symlink_or_special_file_is_never_approvable() {
    let p = project("links");
    // A link to a file inside the folder: still refused (one rule, no
    // target bookkeeping).
    std::os::unix::fs::symlink("index.html", p.demo().join("alias.html")).unwrap();
    let e = check_at(&p.base, &p.cx, &p.id, &p.target()).unwrap_err();
    assert!(e.contains("symlink") && e.contains("alias.html"), "{e}");
    std::fs::remove_file(p.demo().join("alias.html")).unwrap();

    std::os::unix::fs::symlink("/etc/hostname", p.demo().join("secret.txt")).unwrap();
    assert!(check_at(&p.base, &p.cx, &p.id, &p.target())
        .unwrap_err()
        .contains("symlink"));
    std::fs::remove_file(p.demo().join("secret.txt")).unwrap();

    std::fs::create_dir_all(p.demo().join("sub")).unwrap();
    std::os::unix::fs::symlink("/etc", p.demo().join("sub/etc")).unwrap();
    assert!(check_at(&p.base, &p.cx, &p.id, &p.target())
        .unwrap_err()
        .contains("symlink"));
    std::fs::remove_file(p.demo().join("sub/etc")).unwrap();

    let fifo = std::ffi::CString::new(p.demo().join("pipe").to_string_lossy().as_bytes()).unwrap();
    // SAFETY: a plain mkfifo on a path we own.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let e = check_at(&p.base, &p.cx, &p.id, &p.target()).unwrap_err();
    assert!(e.contains("not a regular file"), "{e}");
    std::fs::remove_file(p.demo().join("pipe")).unwrap();

    // And the status lists it as unavailable rather than failing the list.
    std::os::unix::fs::symlink("index.html", p.demo().join("alias.html")).unwrap();
    let s = p.status();
    assert!(s.widgets.is_empty());
    assert_eq!(s.unavailable.len(), 1);
    assert_eq!(s.unavailable[0].widget, "fig-demo");
}

#[test]
fn a_never_approved_widget_requires_approval_with_everything_an_agent_needs() {
    let p = project("fresh");
    let s = p.status();
    assert!(!s.auto_approve);
    assert_eq!(s.pending, 1);
    assert_eq!(
        s.exempt,
        ["fig-mesh", "tab-results", "fig-clip", "fig-chart"]
    );
    let r = required(&s.widgets[0]);
    assert_eq!(r.cause, WidgetApprovalCause::NeverApproved);
    assert_approval_required_is_complete(r, "fig-demo", "widgets/demo");
    let json = serde_json::to_value(&s.widgets[0]).unwrap();
    assert_eq!(json["status"], "approval_required");
    assert_eq!(json["cause"], "never_approved");
    assert_eq!(json["kind"], "html_widget");
    for key in [
        "widget",
        "path",
        "digest",
        "declaredOrigins",
        "whatHappens",
        "userAction",
        "agentMustNot",
        "message",
        "panel",
    ] {
        assert!(json.get(key).is_some(), "{key}: {json}");
    }
}

/// Every approval_required result carries the widget id, digest, the panel
/// to open and the cause, in the fields and in the text relayed.
fn assert_approval_required_is_complete(r: &ApprovalRequired, id: &str, path: &str) {
    assert_eq!(r.kind, ApprovalKind::HtmlWidget);
    assert_eq!(r.widget, id);
    assert_eq!(r.path, path);
    assert_eq!(r.digest.len(), 64);
    assert_eq!(r.panel, PANEL);
    assert_eq!(
        r.user_action,
        format!(
            "In Maleficium open View > Widgets, find '{id}', review the source diff, and click Approve."
        )
    );
    assert!(
        r.message.contains(id) && r.message.contains(PANEL),
        "{}",
        r.message
    );
    assert!(!r.what_happens.is_empty());
    assert!(r.agent_must_not.len() >= 3);
    assert!(r.agent_must_not.iter().any(|s| s.contains("loop")));
    assert!(r.agent_must_not.iter().any(|s| s.contains("approve")));
}

#[test]
fn approval_binds_the_digest_and_any_edit_drops_it() {
    let p = project("edit");
    let a = p.approve();
    assert_eq!(approved(&a).via, ApprovedVia::User);
    assert!(p.check().status.is_approved());
    assert_eq!(p.status().pending, 0);

    // A content edit, an added file, a removed file: each unapproves.
    let first = p.check().snapshot.digest;
    p.write("index.html", "<!doctype html><p>demo!</p>");
    let r = p.check().status;
    let r = required(&r);
    assert_eq!(r.cause, WidgetApprovalCause::ChangedSinceApproval);
    assert_eq!(r.approved_digest.as_deref(), Some(first.as_str()));
    assert_ne!(r.digest, first);
    assert_approval_required_is_complete(r, "fig-demo", "widgets/demo");

    p.write("index.html", "<!doctype html><p>demo</p>");
    assert!(p.check().status.is_approved(), "back to the approved bytes");
    p.write("extra.js", "");
    assert!(!p.check().status.is_approved());
    std::fs::remove_file(p.demo().join("extra.js")).unwrap();
    assert!(p.check().status.is_approved());
    std::fs::remove_file(p.demo().join("index.html")).unwrap();
    assert!(!p.check().status.is_approved());
}

#[test]
fn approving_a_digest_other_than_the_current_one_is_refused() {
    let p = project("stale");
    let shown = p.check().snapshot.digest;
    p.write("index.html", "changed after review");
    let e = approve_at(&p.base, &p.cx, &p.approve_params(&shown)).unwrap_err();
    assert!(e.contains("review it again"), "{e}");
    assert!(!p.check().status.is_approved());
    // A first-party widget and an unknown id cannot be approved.
    for (widget, want) in [("fig-mesh", "needs no approval"), ("nope", "no widget")] {
        let mut params = p.approve_params(&shown);
        params.widget = widget.into();
        let e = approve_at(&p.base, &p.cx, &params).unwrap_err();
        assert!(e.contains(want), "{widget}: {e}");
    }
}

#[test]
fn auto_mode_approves_a_content_edit_but_not_new_declared_origins() {
    let p = project("auto");
    p.approve();
    p.auto(true);
    assert!(p.status().auto_approve);

    p.write("index.html", "edited under auto");
    let s = p.check().status;
    let a = approved(&s);
    assert_eq!(a.via, ApprovedVia::Auto);
    assert!(a.approved_digest.is_some());

    // Declaring a new origin: never automatic.
    p.write(
        "widget.json",
        r#"{"csp":{"frameDomains":["https://www.youtube-nocookie.com"]}}"#,
    );
    let s = p.check().status;
    let r = required(&s);
    assert_eq!(r.cause, WidgetApprovalCause::DeclaredOriginsChanged);
    assert_eq!(
        r.declared_origins.frame_domains,
        ["https://www.youtube-nocookie.com"]
    );
    assert_eq!(r.approved_origins, Some(WidgetCsp::default()));
    assert!(r.message.contains("youtube-nocookie"), "{}", r.message);
    assert_approval_required_is_complete(r, "fig-demo", "widgets/demo");

    // The user approves the origin; a later content edit is automatic again,
    // a narrower set too, a different origin is not.
    p.approve();
    p.write("index.html", "edited again");
    assert_eq!(approved(&p.check().status).via, ApprovedVia::Auto);
    p.write("widget.json", r#"{"csp":{}}"#);
    assert_eq!(approved(&p.check().status).via, ApprovedVia::Auto);
    p.write(
        "widget.json",
        r#"{"csp":{"connectDomains":["https://www.youtube-nocookie.com"]}}"#,
    );
    assert_eq!(
        required(&p.check().status).cause,
        WidgetApprovalCause::DeclaredOriginsChanged,
        "the same origin under another directive is a widening"
    );

    // Off again: the content edit needs the user.
    p.write(
        "widget.json",
        r#"{"csp":{"frameDomains":["https://www.youtube-nocookie.com"]}}"#,
    );
    p.auto(false);
    assert_eq!(
        required(&p.check().status).cause,
        WidgetApprovalCause::ChangedSinceApproval
    );
}

#[test]
fn auto_mode_approves_a_new_widget_only_when_it_declares_no_origins() {
    let p = project("auto-new");
    p.auto(true);
    assert_eq!(approved(&p.check().status).via, ApprovedVia::Auto);
    p.write(
        "widget.json",
        r#"{"csp":{"connectDomains":["https://api.example.org"]}}"#,
    );
    assert_eq!(
        required(&p.check().status).cause,
        WidgetApprovalCause::NeverApproved
    );
}

#[test]
fn a_revoked_widget_stays_unapproved_even_under_auto_mode() {
    let p = project("revoke");
    p.approve();
    let key = revoke_at(
        &p.base,
        &p.cx,
        &WidgetRevokeParams {
            root_id: p.id.clone(),
            path: "widgets/./demo".into(),
        },
    )
    .unwrap();
    assert_eq!(key, "widgets/demo");
    assert_eq!(
        required(&p.check().status).cause,
        WidgetApprovalCause::Revoked
    );
    p.auto(true);
    assert_eq!(
        required(&p.check().status).cause,
        WidgetApprovalCause::Revoked
    );
    // Revoking a widget auto mode had approved, before any approval.
    let q = project("revoke-auto");
    q.auto(true);
    assert!(q.check().status.is_approved());
    revoke_at(
        &q.base,
        &q.cx,
        &WidgetRevokeParams {
            root_id: q.id.clone(),
            path: "widgets/demo".into(),
        },
    )
    .unwrap();
    assert_eq!(
        required(&q.check().status).cause,
        WidgetApprovalCause::Revoked
    );
    // Only an explicit approval brings it back.
    p.approve();
    assert_eq!(approved(&p.check().status).via, ApprovedVia::User);
}

#[test]
fn approvals_live_outside_the_project_and_nothing_in_it_counts() {
    let p = project("outside");
    // Files a cloned repository could ship, claiming approval.
    let digest = p.check().snapshot.digest;
    let claim = serde_json::json!({
        "format": 2, "root": p.root.to_string_lossy(), "autoApprove": true, "runtimes": {},
        "widgets": {"widgets/demo": {"widget": "fig-demo", "digest": digest,
            "origins": {}, "files": {}, "approvedAt": 1, "revoked": false}}
    })
    .to_string();
    for rel in [
        ".maleficium/approvals.json",
        ".maleficium/widgets/approvals.json",
        ".maleficium/store.json",
        "store.json",
        "widgets/approvals.json",
    ] {
        let at = p.root.join(rel);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, &claim).unwrap();
    }
    p.write(
        "widget.json",
        r#"{"approved": true, "autoApprove": true, "digest": "x"}"#,
    );
    let s = p.status();
    assert!(!s.auto_approve);
    assert_eq!(
        required(&s.widgets[0]).cause,
        WidgetApprovalCause::NeverApproved
    );

    // Once approved, the store sits under the app data base, never the root.
    p.approve();
    let stored = project_dir(&p.base, &p.root).unwrap().join(STORE_FILE);
    assert!(stored.is_file());
    assert!(!stored.starts_with(&p.root));
    assert!(stored.starts_with(&p.base));

    // An app data dir inside the project is refused outright.
    let inside = p.root.join(".local/share/maleficium-widgets");
    let e = approve_at(
        &inside,
        &p.cx,
        &p.approve_params(&p.check().snapshot.digest),
    )
    .unwrap_err();
    assert!(e.contains("inside the project"), "{e}");
    assert!(!inside.join("approvals").exists());
    assert!(!check_at(&inside, &p.cx, &p.id, &p.target())
        .unwrap()
        .status
        .is_approved());
}

#[test]
fn a_moved_or_cloned_project_carries_no_approvals() {
    let p = project("clone-src");
    p.approve();
    p.auto(true);
    // A second checkout of the same content at another path.
    let q = project("clone-dst");
    let s = check_at(&p.base, &q.cx, &q.id, &q.target()).unwrap();
    assert_eq!(s.snapshot.digest, p.check().snapshot.digest);
    assert_eq!(
        required(&s.status).cause,
        WidgetApprovalCause::NeverApproved
    );
    assert!(
        !status_at(&p.base, &q.cx, &q.id, "main.tex")
            .unwrap()
            .auto_approve
    );
}

#[test]
fn the_store_survives_a_restart_and_fails_closed_when_corrupt() {
    let p = project("restart");
    p.approve();
    p.auto(true);
    let cx = p.restarted();
    let s = status_at(&p.base, &cx, &p.id, "main.tex").unwrap();
    assert!(s.auto_approve && s.pending == 0 && s.store_error.is_none());
    assert_eq!(approved(&s.widgets[0]).via, ApprovedVia::User);

    let file = project_dir(&p.base, &p.root).unwrap().join(STORE_FILE);
    let good = std::fs::read_to_string(&file).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&good).unwrap();
    json["root"] = "/somewhere/else".into();
    let other_root = json.to_string();
    json["root"] = p.root.to_string_lossy().into_owned().into();
    json["format"] = 1.into();
    let other_format = json.to_string();
    json["format"] = 2.into();
    json["approver"] = "agent".into();
    let unknown_field = json.to_string();
    for bad in [
        "",
        "{",
        "null",
        "[]",
        &good[..good.len() / 2],
        "\u{0}\u{1}binary",
        &other_root,
        &other_format,
        &unknown_field,
    ] {
        std::fs::write(&file, bad).unwrap();
        let s = status_at(&p.base, &cx, &p.id, "main.tex").unwrap();
        assert!(!s.auto_approve, "{bad:?}");
        assert_eq!(s.pending, 1, "{bad:?}");
        assert!(s.store_error.is_some(), "{bad:?}");
        assert!(!p.check().status.is_approved(), "{bad:?}");
    }
    // A store that is a folder: unreadable, closed.
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(&file).unwrap();
    assert!(!p.check().status.is_approved());
    std::fs::remove_dir_all(&file).unwrap();

    // The user can approve again over a corrupt store.
    std::fs::write(&file, "{").unwrap();
    p.approve();
    assert!(p.check().status.is_approved());
    assert!(p.status().store_error.is_none());
}

#[test]
fn review_lists_each_file_against_the_approved_content() {
    let p = project("review");
    p.write("app.js", "one()");
    p.write("logo.bin", "\u{0}\u{1}");
    p.approve();
    p.write("app.js", "two()");
    p.write("new.css", "p{}");
    std::fs::remove_file(p.demo().join("logo.bin")).unwrap();
    let r = review_at(&p.base, &p.cx, &p.id, "main.tex", "fig-demo").unwrap();
    assert_eq!(
        required(&r.status).cause,
        WidgetApprovalCause::ChangedSinceApproval
    );
    let by = |path: &str| r.files.iter().find(|f| f.path == path).unwrap();
    let app = by("app.js");
    assert_eq!(app.change, FileChange::Modified);
    assert_eq!(app.before.as_deref(), Some("one()"));
    assert_eq!(app.after.as_deref(), Some("two()"));
    assert_eq!(by("new.css").change, FileChange::Added);
    assert_eq!(by("new.css").before, None);
    assert_eq!(by("index.html").change, FileChange::Unchanged);
    let logo = by("logo.bin");
    assert_eq!(logo.change, FileChange::Removed);
    assert!(logo.after.is_none());
    // Re-approving drops the blobs no record names any more.
    let blobs = project_dir(&p.base, &p.root).unwrap().join(BLOBS);
    let before = std::fs::read_dir(&blobs).unwrap().count();
    p.approve();
    let after = std::fs::read_dir(&blobs).unwrap().count();
    assert_eq!(before, 3);
    assert_eq!(after, 3, "index.html, app.js two(), new.css");
    // A tampered blob is not shown as approved content.
    let sha = crate::bundle::sha_of(b"two()");
    std::fs::write(blobs.join(&sha), "tampered").unwrap();
    p.write("app.js", "three()");
    let r = review_at(&p.base, &p.cx, &p.id, "main.tex", "fig-demo").unwrap();
    let app = r.files.iter().find(|f| f.path == "app.js").unwrap();
    assert_eq!(app.change, FileChange::Modified);
    assert_eq!(app.before, None);
}

#[test]
fn digest_changes_and_user_actions_become_typed_events() {
    let p = project("events");
    let a = p.approve();
    let e = approved_event(&p.id, &a).unwrap();
    assert_eq!(e.actor, Actor::User);
    assert!(matches!(e.event, AppEvent::WidgetApproved { ref widget, .. } if widget == "fig-demo"));
    assert!(digest_changed_event(&p.id, &a, Actor::Agent).is_none());

    p.write("index.html", "changed");
    let s = p.check().status;
    let e = digest_changed_event(&p.id, &s, Actor::Agent).unwrap();
    assert_eq!(e.actor, Actor::Agent);
    assert!(matches!(
        e.event,
        AppEvent::WidgetDigestChanged {
            cause: WidgetApprovalCause::ChangedSinceApproval,
            auto_approved: false,
            ..
        }
    ));
    p.auto(true);
    let s = p.check().status;
    assert!(matches!(
        digest_changed_event(&p.id, &s, Actor::System)
            .unwrap()
            .event,
        AppEvent::WidgetDigestChanged {
            auto_approved: true,
            ..
        }
    ));
    let never = project("events-never");
    assert!(digest_changed_event(&never.id, &never.check().status, Actor::User).is_none());
    let WidgetApprovalStatus::ApprovalRequired(r) = never.check().status else {
        panic!("never approved must need approval");
    };
    let e = approval_required_event(&never.id, &r, Actor::System);
    assert_eq!(e.actor, Actor::System);
    let AppEvent::WidgetApprovalRequired {
        ref widget,
        ref digest,
        cause,
        ..
    } = e.event
    else {
        panic!("{e:?}");
    };
    assert_eq!((widget, digest), (&r.widget, &r.digest));
    assert_eq!(cause, WidgetApprovalCause::NeverApproved);
    let line = crate::eventlog::serialize(&e);
    assert!(line.contains("\"widget.approval-required\""), "{line}");

    let line = crate::eventlog::serialize(&revoked_event(&p.id, "widgets/demo"));
    assert!(line.contains("\"widget.revoked\""), "{line}");
    let line = crate::eventlog::serialize(&auto_event(&p.id, true));
    assert!(line.contains("\"widgets.auto-approve\""), "{line}");
}

#[test]
fn the_target_of_a_listed_widget_is_its_bundle_folder_from_the_main_file() {
    let cx = Core::default();
    let p = project("target");
    let list = crate::widgets::widgets(&p.cx, &p.id, "main.tex").unwrap();
    let demo = list.widgets.iter().find(|w| w.id == "fig-demo").unwrap();
    assert_eq!(
        WidgetTarget::of("main.tex", demo).unwrap(),
        Some(p.target())
    );
    assert_eq!(
        WidgetTarget::of("paper/main.tex", demo)
            .unwrap()
            .unwrap()
            .path,
        "paper/widgets/demo"
    );
    let mesh = list.widgets.iter().find(|w| w.id == "fig-mesh").unwrap();
    assert_eq!(WidgetTarget::of("main.tex", mesh).unwrap(), None);
    drop(cx);
}

#[test]
fn no_session_root_may_reach_the_approval_store() {
    let base = crate::test_scratch::dir("store-guard/data/maleficium-widgets");
    std::fs::create_dir_all(base.join("approvals/abc")).unwrap();
    let canon = |p: &Path| dunce::canonicalize(p).unwrap();
    let data = canon(base.parent().unwrap());
    // The store itself, a project dir in it, and anything containing it.
    for root in [
        canon(&base),
        canon(&base.join("approvals/abc")),
        data.clone(),
        canon(&data.join("..")),
    ] {
        let e = refuse_store_overlap(&root, &base).unwrap_err();
        assert!(e.contains("widget approvals"), "{}: {e}", root.display());
    }
    // A sibling of the store, and an unrelated project, are fine.
    std::fs::create_dir_all(data.join("maleficium-untitled")).unwrap();
    assert!(refuse_store_overlap(&data.join("maleficium-untitled"), &base).is_ok());
    let p = project("guard-elsewhere");
    assert!(refuse_store_overlap(&p.root, &base).is_ok());
    // A store base that does not exist yet still resolves through its
    // existing ancestors.
    let later = data.join("not-yet/maleficium-widgets");
    assert!(refuse_store_overlap(&data, &later).is_err());
}

/// The real grant applies it: the app's own store base cannot be granted.
#[test]
fn grant_refuses_the_real_approval_store() {
    let base = store_base();
    if std::fs::create_dir_all(&base).is_err() {
        return;
    }
    let cx = Core::default();
    let e = crate::fs::grant_root(&cx, "store", &base.to_string_lossy()).unwrap_err();
    assert!(e.contains("widget approvals"), "{e}");
    // Nor can an export or a new project be written into it.
    let p = project("store-writes");
    let into = base.join("store.json");
    let e = crate::export::destination(&p.root, &into.to_string_lossy()).unwrap_err();
    assert!(e.contains("widget approvals"), "{e}");
    let e =
        crate::templates::instantiate("article", &base.to_string_lossy(), "approvals").unwrap_err();
    assert!(e.contains("widget approvals"), "{e}");
}

impl Project {
    /// Rewrite fig-demo's sidecar options, as a recompile with other macro
    /// options would.
    fn demo_options(&self, extra: &str) {
        let line = "|bundle=widgets/demo/|height=227.62204pt|";
        assert!(REAL_SIDECAR.contains(line));
        let side = REAL_SIDECAR.replace(
            line,
            &format!("|bundle=widgets/demo/|height=227.62204pt{extra}|"),
        );
        std::fs::write(self.out.join("main.mfw"), side).unwrap();
    }
    fn demo_status(&self) -> WidgetApprovalStatus {
        let s = self.status();
        assert!(s.unavailable.is_empty(), "{:?}", s.unavailable);
        s.widgets.into_iter().next().unwrap()
    }
    fn approve_listed(&self) -> WidgetApprovalStatus {
        let digest = match self.demo_status() {
            WidgetApprovalStatus::Approved(a) => a.digest,
            WidgetApprovalStatus::ApprovalRequired(r) => r.digest,
        };
        approve_at(&self.base, &self.cx, &self.approve_params(&digest)).unwrap()
    }
}

#[test]
fn origins_declared_in_macro_options_are_part_of_the_digest_and_never_auto_approved() {
    let p = project("macro-origins");
    p.approve_listed();
    p.auto(true);
    assert_eq!(approved(&p.demo_status()).via, ApprovedVia::User);

    // A recompile that adds a frame origin through the macro alone: the
    // folder is unchanged, the widget is not.
    p.demo_options(",framedomains=https://www.youtube-nocookie.com");
    let s = p.demo_status();
    let r = required(&s);
    assert_eq!(r.cause, WidgetApprovalCause::DeclaredOriginsChanged);
    assert_eq!(
        r.declared_origins.frame_domains,
        ["https://www.youtube-nocookie.com"]
    );
    assert_ne!(Some(&r.digest), r.approved_digest.as_ref());

    // The user approves it; the same origin moved into widget.json is the
    // same policy but other content: approved again only by auto mode.
    assert!(p.approve_listed().is_approved());
    p.demo_options("");
    p.write(
        "widget.json",
        r#"{"csp":{"frameDomains":["https://www.youtube-nocookie.com"]}}"#,
    );
    assert_eq!(approved(&p.demo_status()).via, ApprovedVia::Auto);
    p.auto(false);
    assert_eq!(
        required(&p.demo_status()).cause,
        WidgetApprovalCause::ChangedSinceApproval
    );

    // Declared in both places at once: one origin, one digest.
    p.approve_listed();
    p.demo_options(",framedomains=https://www.youtube-nocookie.com");
    assert_eq!(approved(&p.demo_status()).via, ApprovedVia::User);

    // A second origin through the macro, even with auto on: the user's.
    p.auto(true);
    p.demo_options(",framedomains=https://www.youtube-nocookie.com https://player.vimeo.com");
    assert_eq!(
        required(&p.demo_status()).cause,
        WidgetApprovalCause::DeclaredOriginsChanged
    );
}

#[test]
fn the_digest_covers_macro_origins_on_top_of_the_files() {
    let p = project("macro-digest");
    let plain = p.check().snapshot;
    let mut t = p.target();
    t.option_origins.frame_domains = vec!["https://a.org".into()];
    let framed = check_at(&p.base, &p.cx, &p.id, &t).unwrap().snapshot;
    assert_eq!(plain.files, framed.files);
    assert_ne!(plain.digest, framed.digest);
    assert_eq!(framed.origins.frame_domains, ["https://a.org"]);
    // The same origin as a resource is another policy and another digest.
    let mut r = p.target();
    r.option_origins.resource_domains = vec!["https://a.org".into()];
    let res = check_at(&p.base, &p.cx, &p.id, &r).unwrap().snapshot;
    assert_ne!(res.digest, framed.digest);
    // A target carrying an origin the list would have refused is refused
    // here too: the snapshot checks again.
    let mut bad = p.target();
    bad.option_origins.frame_domains = vec!["https://*.a.org".into()];
    assert!(check_at(&p.base, &p.cx, &p.id, &bad).is_err());
}

// ---- custom runtimes ------------------------------------------------------

const RT: &str = "heatmap@1";
/// fig-chart of the real pdf, recorded as a custom widget using heatmap@1.
const CHART_LINE: &str = "widget|fig-chart|chart|chart@1|||house|figures/chart.png|spec=charts/ablation.vl.json|height=142.26378pt|Ablation chart";
const CUSTOM_LINE: &str = "widget|fig-chart|custom|heatmap@1|||house|figures/chart.png|primary=data/grid.csv|height=142.26378pt,scheme=div|Ablation chart";

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let t = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &t);
        } else {
            std::fs::copy(e.path(), &t).unwrap();
        }
    }
}

fn sample_dir(r: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../embed-runtime/samples")
        .join(r)
}

impl Project {
    fn rt_dir(&self) -> PathBuf {
        self.root.join("runtimes").join(RT)
    }
    fn install(&self) {
        copy_tree(&sample_dir(RT), &self.rt_dir());
    }
    fn rt_write(&self, rel: &str, bytes: &[u8]) {
        let p = self.rt_dir().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    fn rt_manifest(&self, f: impl Fn(&mut serde_json::Value)) {
        let p = self.rt_dir().join("runtime.json");
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        f(&mut v);
        std::fs::write(p, v.to_string()).unwrap();
    }
    fn rt_check(&self) -> RuntimeChecked {
        check_runtime_at(&self.base, &self.cx, &self.id, RT, &["fig-chart".into()]).unwrap()
    }
    fn rt_status(&self) -> WidgetApprovalStatus {
        self.rt_check().status.expect("a valid package is judged")
    }
    fn decide(
        &self,
        digest: &str,
        decision: RuntimeDecision,
    ) -> Result<WidgetApprovalStatus, String> {
        decide_runtime_at(
            &self.base,
            &self.cx,
            &RuntimeDecisionParams {
                root_id: self.id.clone(),
                main_rel: "main.tex".into(),
                runtime: RT.into(),
                digest: digest.into(),
                decision,
            },
        )
    }
    fn allow(&self) -> WidgetApprovalStatus {
        let d = self.rt_check().snapshot.digest;
        self.decide(&d, RuntimeDecision::Allowed).unwrap()
    }
}

/// The approval fixture with fig-chart recorded as a heatmap@1 widget and
/// the documented heatmap sample installed as the project copy.
fn rt_project(name: &str) -> Project {
    let p = project(name);
    let side = REAL_SIDECAR.replace(CHART_LINE, CUSTOM_LINE);
    assert_ne!(side, REAL_SIDECAR);
    std::fs::write(p.out.join("main.mfw"), side).unwrap();
    p.install();
    p
}

fn invalid_of(p: &Project) -> String {
    let c = p.rt_check();
    assert!(c.status.is_none(), "an invalid package is never judged");
    assert!(c.snapshot.manifest.is_none());
    c.snapshot.invalid.unwrap()
}

#[test]
fn the_runtime_digest_is_the_widget_digest_over_every_file_and_pinned() {
    // Pinned, checked against an independent Python implementation of the
    // same algorithm (it reproduces the html vector above too).
    let two = files(&[
        ("runtime.json", "{\"contract\":1}"),
        ("index.html", "<!doctype html><p>x</p>"),
    ]);
    assert_eq!(
        digest(&two, &WidgetCsp::default()),
        "39363733529653b52fd5f7212f54880d6053e3f7f0002828825011fa110b6a59"
    );
    let p = rt_project("rt-digest");
    let snap = snapshot_runtime(&p.cx, &p.id, RT).unwrap();
    assert_eq!(snap.path, "runtimes/heatmap@1");
    // Every file, metadata included (runtime.json, LICENSE, samples/).
    for f in ["runtime.json", "index.html", "LICENSE", "samples/data.csv"] {
        assert!(snap.files.contains_key(f), "{f}");
    }
    assert_eq!(snap.digest, digest(&snap.files, &WidgetCsp::default()));
    let before = snap.digest.clone();
    p.rt_write("samples/data.csv", b"9,9\n");
    assert_ne!(snapshot_runtime(&p.cx, &p.id, RT).unwrap().digest, before);
}

fn pure_case() -> (RuntimeSnapshot, RuntimeManifest) {
    let files = crate::runtimes::tests::sample(RT);
    let m = crate::runtimes::check_package(RT, &files).unwrap();
    let snap = RuntimeSnapshot {
        dir: PathBuf::from("/p/runtimes/heatmap@1"),
        path: "runtimes/heatmap@1".into(),
        digest: digest(&files, &WidgetCsp::default()),
        files,
        manifest: Some(m.clone()),
        invalid: None,
        warnings: Vec::new(),
    };
    (snap, m)
}

fn store_with(auto: bool, rec: Option<RuntimeRecord>) -> Store {
    let mut s = Store::empty(Path::new("/p"));
    s.auto_approve = auto;
    if let Some(r) = rec {
        s.runtimes.insert(RT.into(), r);
    }
    s
}

fn rec(
    decision: RuntimeDecision,
    digest: &str,
    license: &str,
    vendored: Vec<VendoredId>,
) -> RuntimeRecord {
    RuntimeRecord {
        decision,
        digest: digest.into(),
        license: license.into(),
        vendored,
        files: BTreeMap::new(),
        decided_at: 1,
    }
}

fn verdict(store: &Store) -> WidgetApprovalStatus {
    let (snap, m) = pure_case();
    judge_runtime(store, RT, &snap, &m, &["fig-chart".into()])
}

#[test]
fn judge_row_no_record_needs_the_user_even_with_auto_on() {
    for auto in [false, true] {
        let s = verdict(&store_with(auto, None));
        let r = required(&s);
        assert_eq!(r.cause, WidgetApprovalCause::NeverApproved);
        assert_eq!(r.kind, ApprovalKind::CustomRuntime);
        assert_eq!(
            (r.widget.as_str(), r.path.as_str()),
            ("fig-chart", "runtimes/heatmap@1")
        );
        assert!(r.declared_origins.is_empty() && r.approved_digest.is_none());
        assert_eq!(
            r.message,
            "Runtime 'heatmap@1' (used by fig-chart) has not been allowed in this project. Ask the user to open View > Widgets in Maleficium, review the runtime and click Allow; until then its widgets export as posters only."
        );
        assert!(r.agent_must_not[1].contains("no tool approves runtimes"));
        let info = r.runtime.as_ref().unwrap();
        assert_eq!(
            (info.reference.as_str(), info.version.as_str()),
            (RT, "1.0.0")
        );
        assert_eq!(info.widgets, ["fig-chart"]);
        assert_eq!((info.license.as_str(), info.webgl), ("MIT-0", false));
    }
}

#[test]
fn judge_row_denied_stays_denied_at_any_digest_and_with_auto_on() {
    let (snap, _) = pure_case();
    for d in [snap.digest.as_str(), "0".repeat(64).as_str()] {
        for auto in [false, true] {
            let s = verdict(&store_with(
                auto,
                Some(rec(RuntimeDecision::Denied, d, "MIT-0", vec![])),
            ));
            let r = required(&s);
            assert_eq!(r.cause, WidgetApprovalCause::Revoked);
            assert_eq!(r.message, "The user denied runtime 'heatmap@1' in this project. Only the user can allow it, in View > Widgets.");
            assert!(r.approved_digest.is_none());
        }
    }
}

#[test]
fn judge_row_allowed_same_digest_is_approved_by_the_user() {
    let (snap, _) = pure_case();
    let s = verdict(&store_with(
        false,
        Some(rec(RuntimeDecision::Allowed, &snap.digest, "MIT-0", vec![])),
    ));
    let a = approved(&s);
    assert_eq!(
        (a.via, a.kind),
        (ApprovedVia::User, ApprovalKind::CustomRuntime)
    );
    assert!(a.approved_digest.is_none() && a.runtime.is_some());
}

#[test]
fn judge_row_licence_or_vendored_change_always_needs_the_user() {
    let other = "1".repeat(64);
    let lib = VendoredId {
        name: "lib".into(),
        version: "1".into(),
        license: "MIT".into(),
    };
    for (license, vendored) in [("Apache-2.0", vec![]), ("MIT-0", vec![lib])] {
        for auto in [false, true] {
            let s = verdict(&store_with(
                auto,
                Some(rec(
                    RuntimeDecision::Allowed,
                    &other,
                    license,
                    vendored.clone(),
                )),
            ));
            let r = required(&s);
            assert_eq!(
                r.cause,
                WidgetApprovalCause::LicenseOrVendoredChanged,
                "{license} {auto}"
            );
            assert_eq!(r.approved_digest.as_deref(), Some(other.as_str()));
            assert!(r
                .message
                .contains("that always needs the user, even with auto-approval on"));
        }
    }
}

#[test]
fn judge_row_other_digest_with_auto_on_is_approved_by_auto() {
    let other = "1".repeat(64);
    let s = verdict(&store_with(
        true,
        Some(rec(RuntimeDecision::Allowed, &other, "MIT-0", vec![])),
    ));
    let a = approved(&s);
    assert_eq!(a.via, ApprovedVia::Auto);
    assert_eq!(a.approved_digest.as_deref(), Some(other.as_str()));
}

#[test]
fn judge_row_other_digest_with_auto_off_needs_approval_again() {
    let other = "1".repeat(64);
    let s = verdict(&store_with(
        false,
        Some(rec(RuntimeDecision::Allowed, &other, "MIT-0", vec![])),
    ));
    let r = required(&s);
    assert_eq!(r.cause, WidgetApprovalCause::ChangedSinceApproval);
    assert_eq!(
        r.message,
        "Runtime 'heatmap@1' changed since the user allowed it, so it needs approval again. Ask the user to open View > Widgets in Maleficium, review the runtime and click Allow; until then its widgets export as posters only."
    );
}

#[test]
fn a_format_1_store_reads_as_empty_and_fails_closed() {
    let p = rt_project("rt-v1");
    p.approve();
    p.allow();
    p.auto(true);
    assert!(p.rt_status().is_approved() && p.check().status.is_approved());
    let file = project_dir(&p.base, &p.root).unwrap().join(STORE_FILE);
    let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(v["format"], 2);
    // A format-1 store: the same records under the old format, and the old
    // shape (no runtimes map). Neither is read.
    v["format"] = 1.into();
    std::fs::write(&file, v.to_string()).unwrap();
    let s = p.status();
    assert_eq!(
        s.store_error.as_deref(),
        Some("approval store has format 1")
    );
    assert!(!s.auto_approve);
    assert_eq!(
        required(&s.runtimes[0]).cause,
        WidgetApprovalCause::NeverApproved
    );
    assert!(!p.check().status.is_approved());
    v.as_object_mut().unwrap().remove("runtimes");
    std::fs::write(&file, v.to_string()).unwrap();
    let s = p.status();
    assert!(s.store_error.is_some());
    assert!(!p.rt_status().is_approved() && !p.check().status.is_approved());
}

#[test]
fn decide_runtime_allows_denies_and_refuses_what_it_cannot_bind() {
    let p = rt_project("rt-decide");
    let d = p.rt_check().snapshot.digest;
    // A stale digest is refused, a denial too.
    for decision in [RuntimeDecision::Allowed, RuntimeDecision::Denied] {
        let e = p.decide(&"0".repeat(64), decision).unwrap_err();
        assert_eq!(
            e,
            format!(
                "runtime heatmap@1 changed since it was shown for review (now {}): review it again",
                &d[..12]
            )
        );
    }
    assert!(!project_dir(&p.base, &p.root)
        .unwrap()
        .join(STORE_FILE)
        .exists());
    let s = p.decide(&d, RuntimeDecision::Allowed).unwrap();
    assert_eq!(approved(&s).via, ApprovedVia::User);
    // A deny is sticky across digests, auto-approval included.
    p.auto(true);
    p.decide(&d, RuntimeDecision::Denied).unwrap();
    p.rt_write("index.html", b"<!doctype html><p>changed</p>");
    assert_eq!(required(&p.rt_status()).cause, WidgetApprovalCause::Revoked);
    // Only the user brings it back.
    let s = p.allow();
    assert_eq!(approved(&s).via, ApprovedVia::User);

    // A runtime the document does not use.
    let e = decide_runtime_at(
        &p.base,
        &p.cx,
        &RuntimeDecisionParams {
            root_id: p.id.clone(),
            main_rel: "main.tex".into(),
            runtime: "stl-viewer@1".into(),
            digest: d.clone(),
            decision: RuntimeDecision::Allowed,
        },
    )
    .unwrap_err();
    assert_eq!(e, "main.tex uses no runtime stl-viewer@1");
    // An invalid package and a missing one.
    p.rt_manifest(|v| v["license"] = "GPL-3.0".into());
    let d = p.rt_check().snapshot.digest;
    let e = p.decide(&d, RuntimeDecision::Allowed).unwrap_err();
    assert!(
        e.starts_with("runtime heatmap@1 is not valid (license `GPL-3.0` is not on the allowlist"),
        "{e}"
    );
    assert!(e.ends_with("; fix it before allowing it"), "{e}");
    assert!(p.decide(&d, RuntimeDecision::Denied).is_err());
    std::fs::remove_dir_all(p.rt_dir()).unwrap();
    let e = p.decide(&d, RuntimeDecision::Allowed).unwrap_err();
    assert_eq!(e, "runtime heatmap@1 is not valid (not installed in this project (no runtimes/heatmap@1/runtime.json)); fix it before allowing it");
}

#[test]
fn status_lists_runtimes_and_counts_them_and_never_exempts_them() {
    let p = rt_project("rt-status");
    let s = p.status();
    assert!(!s.exempt.contains(&"fig-chart".to_string()));
    assert_eq!(s.runtimes.len(), 1);
    // fig-demo (html) and heatmap@1 both wait.
    assert_eq!(s.pending, 2);
    p.allow();
    let s = p.status();
    assert_eq!(s.pending, 1);
    assert!(s.runtimes[0].is_approved());
    // Invalid and missing packages are unavailable, never judged.
    p.rt_manifest(|v| v["name"] = "other".into());
    let s = p.status();
    assert!(s.runtimes.is_empty());
    assert_eq!(s.unavailable.len(), 1);
    assert_eq!(s.unavailable[0].path, "runtimes/heatmap@1");
    assert_eq!(s.unavailable[0].widget, "fig-chart");
    assert_eq!(
        s.unavailable[0].error,
        "runtime heatmap@1: name `other` does not match its folder"
    );
    std::fs::remove_dir_all(p.rt_dir()).unwrap();
    let s = p.status();
    assert_eq!(
        s.unavailable[0].error,
        "runtime heatmap@1: not installed in this project (no runtimes/heatmap@1/runtime.json)"
    );
    assert_eq!(s.pending, 1);
}

/// One way to damage an installed package.
#[cfg(unix)]
type Hurt = Box<dyn Fn(&Project)>;

#[cfg(unix)]
#[test]
fn hostile_packages_are_invalid_and_never_judged() {
    let p = rt_project("rt-hostile");
    // A symlink inside the package.
    std::os::unix::fs::symlink("/etc/passwd", p.rt_dir().join("vendor-link")).unwrap();
    assert!(invalid_of(&p).contains("the runtime folder holds a symlink (vendor-link)"));
    std::fs::remove_file(p.rt_dir().join("vendor-link")).unwrap();
    // The package folder itself a link to another folder of the project.
    let real = p.root.join("elsewhere");
    copy_tree(&sample_dir(RT), &real);
    std::fs::remove_dir_all(p.rt_dir()).unwrap();
    std::os::unix::fs::symlink(&real, p.rt_dir()).unwrap();
    assert!(invalid_of(&p).contains("runtimes/heatmap@1 is a link or not a folder"));
    // A link out of the project.
    std::fs::remove_file(p.rt_dir()).unwrap();
    let outside = crate::test_scratch::dir("approval-rt-outside");
    copy_tree(&sample_dir(RT), &outside);
    std::os::unix::fs::symlink(&outside, p.rt_dir()).unwrap();
    assert!(invalid_of(&p).contains("forbidden path (outside project)"));
    std::fs::remove_file(p.rt_dir()).unwrap();
    p.install();
    assert!(p.rt_check().status.is_some());

    let cases: Vec<(Hurt, &str)> = vec![
        (
            Box::new(|p| p.rt_write("runtime.json", &vec![b' '; 64 * 1024 + 1])),
            "runtime heatmap@1: runtime.json is larger than 64 KiB",
        ),
        (
            Box::new(|p| p.rt_manifest(|v| v["license"] = "SSPL-1.0".into())),
            "runtime heatmap@1: license `SSPL-1.0` is not on the allowlist",
        ),
        (
            Box::new(|p| p.rt_manifest(|v| v["name"] = "heatmop".into())),
            "runtime heatmap@1: name `heatmop` does not match its folder",
        ),
        (
            Box::new(|p| {
                p.rt_manifest(|v| {
                v["vendored"] = serde_json::json!([{"name": "x", "version": "1", "license": "MIT", "source": "s",
                    "files": ["vendor/../../../main.tex"], "licenseFile": "vendor/x/LICENSE"}])
            })
            }),
            "runtime heatmap@1: path `vendor/../../../main.tex` is not a plain relative path",
        ),
        (
            Box::new(|p| {
                p.rt_manifest(|v| {
                v["vendored"] = serde_json::json!([{"name": "x", "version": "1", "license": "GPL-3.0", "source": "s",
                    "files": ["vendor/x/x.js"], "licenseFile": "vendor/x/LICENSE"}])
            })
            }),
            "runtime heatmap@1: vendored x: license `GPL-3.0` is not on the allowlist",
        ),
        (
            Box::new(|p| {
                p.rt_write(
                    "index.html",
                    b"<script src=\"https://cdn.example.com/x.js\"></script>",
                )
            }),
            "runtime heatmap@1: index.html:1: ",
        ),
        (
            Box::new(|p| p.rt_write("vendor/big.bin", &vec![0u8; (MAX_BYTES + 1) as usize])),
            "runtime heatmap@1: the runtime folder is larger than 64 MiB",
        ),
    ];
    for (hurt, want) in cases {
        std::fs::remove_dir_all(p.rt_dir()).unwrap();
        p.install();
        hurt(&p);
        let e = invalid_of(&p);
        assert!(e.starts_with(want), "want `{want}`, got: {e}");
        let s = p.status();
        assert!(s.runtimes.is_empty() && s.unavailable[0].error.starts_with(want));
    }
}

#[test]
fn the_documented_bad_cdn_sample_is_invalid_on_its_script_line() {
    let p = project("rt-cdn");
    let side = REAL_SIDECAR.replace(
        CHART_LINE,
        &CUSTOM_LINE
            .replace("heatmap@1", "bad-cdn@1")
            .replace(",scheme=div", ""),
    );
    std::fs::write(p.out.join("main.mfw"), side).unwrap();
    copy_tree(&sample_dir("bad-cdn@1"), &p.root.join("runtimes/bad-cdn@1"));
    let c = check_runtime_at(&p.base, &p.cx, &p.id, "bad-cdn@1", &["fig-chart".into()]).unwrap();
    assert!(c.status.is_none());
    let e = c.snapshot.invalid.unwrap();
    assert!(e.starts_with("runtime bad-cdn@1: index.html:"), "{e}");
}

#[test]
fn review_runtime_diffs_against_the_last_decision() {
    let p = rt_project("rt-review");
    p.allow();
    p.rt_write("index.html", b"<!doctype html><p>new</p>");
    p.rt_write("extra.js", b"var x;");
    let r = review_runtime_at(&p.base, &p.cx, &p.id, "main.tex", RT).unwrap();
    assert_eq!(
        required(&r.status).cause,
        WidgetApprovalCause::ChangedSinceApproval
    );
    let change = |f: &str| r.files.iter().find(|x| x.path == f).unwrap().change;
    assert_eq!(change("index.html"), FileChange::Modified);
    assert_eq!(change("extra.js"), FileChange::Added);
    assert_eq!(change("runtime.json"), FileChange::Unchanged);
    let html = r.files.iter().find(|x| x.path == "index.html").unwrap();
    assert!(html.before.as_deref().unwrap().contains("heatmap"));
    assert_eq!(html.after.as_deref(), Some("<!doctype html><p>new</p>"));
}

#[test]
fn runtime_events_name_the_runtime_and_the_decision() {
    let p = rt_project("rt-events");
    let s = p.rt_status();
    let e = approval_required_event(&p.id, required(&s), Actor::Agent);
    match e.event {
        AppEvent::RuntimeApprovalRequired {
            runtime,
            cause,
            widgets,
            digest,
            ..
        } => {
            assert_eq!(
                (runtime.as_str(), cause),
                (RT, WidgetApprovalCause::NeverApproved)
            );
            assert_eq!(widgets, ["fig-chart"]);
            assert_eq!(digest, p.rt_check().snapshot.digest);
        }
        other => panic!("{other:?}"),
    }
    let params = RuntimeDecisionParams {
        root_id: p.id.clone(),
        main_rel: "main.tex".into(),
        runtime: RT.into(),
        digest: "d".into(),
        decision: RuntimeDecision::Denied,
    };
    let e = runtime_decided_event(&p.id, &params);
    assert_eq!(e.actor, Actor::User);
    let v = serde_json::to_value(&e.event).unwrap();
    assert_eq!(v["action"], "runtime.decided");
    assert_eq!(v["decision"], "denied");
    // A licence change after an allow is a digest change too.
    p.allow();
    p.rt_manifest(|v| v["license"] = "ISC".into());
    let s = p.rt_status();
    assert_eq!(
        required(&s).cause,
        WidgetApprovalCause::LicenseOrVendoredChanged
    );
    let e = digest_changed_event(&p.id, &s, Actor::User).unwrap();
    match e.event {
        AppEvent::WidgetDigestChanged { path, cause, .. } => {
            assert_eq!(path, "runtimes/heatmap@1");
            assert_eq!(cause, WidgetApprovalCause::LicenseOrVendoredChanged);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_runtime_judged_is_the_snapshot_handed_back() {
    let p = rt_project("rt-one-read");
    p.allow();
    let c = p.rt_check();
    let a = approved(c.status.as_ref().unwrap());
    // The caller folds `snapshot.files`: they are the bytes that hash to the
    // approved digest, read once.
    assert_eq!(a.digest, digest(&c.snapshot.files, &WidgetCsp::default()));
    assert_eq!(a.digest, c.snapshot.digest);
}

/// After an install the agent asks the user unless auto-approval covers it:
/// it never covers a first install, and covers an update only once the
/// user allowed the runtime.
#[test]
fn after_install_asks_the_user_unless_auto_approval_covers_the_update() {
    use crate::runtime_author::after_install;
    let p = rt_project("rt-after-install");
    let first = after_install(RT, &p.rt_status());
    assert!(!first.approved && first.ask_user && !first.auto_approve);
    assert!(
        first
            .hint
            .starts_with("Ask the user to allow heatmap@1. heatmap@1 is new"),
        "{}",
        first.hint
    );
    assert!(first.hint.contains(PANEL), "{}", first.hint);

    p.auto(true);
    let first_auto = after_install(RT, &p.rt_status());
    assert!(!first_auto.approved && first_auto.ask_user && first_auto.auto_approve);
    assert!(
        first_auto.hint.contains("only covers updates"),
        "{}",
        first_auto.hint
    );

    p.allow();
    let allowed = after_install(RT, &p.rt_status());
    assert!(allowed.approved && !allowed.ask_user);
    assert!(
        allowed.hint.contains("already allowed exactly these files"),
        "{}",
        allowed.hint
    );

    p.rt_write("index.html", b"<!doctype html><p>an update</p>");
    let update_auto = after_install(RT, &p.rt_status());
    assert!(
        update_auto.approved && !update_auto.ask_user,
        "{}",
        update_auto.hint
    );
    assert!(
        update_auto.hint.contains("auto-approval is on"),
        "{}",
        update_auto.hint
    );

    p.auto(false);
    let update = after_install(RT, &p.rt_status());
    assert!(!update.approved && update.ask_user);
    assert!(
        update.hint.contains("auto-approval is off"),
        "{}",
        update.hint
    );
}
