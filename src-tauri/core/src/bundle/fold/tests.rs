use super::*;

/// A bundle as the fold receives it: files by path, read once.
fn files(pairs: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
    pairs
        .iter()
        .map(|(p, b)| (p.to_string(), b.to_vec()))
        .collect()
}

fn fold(f: &BTreeMap<String, Vec<u8>>) -> Folded {
    fold_bundle(f, &widget_policy(None)).unwrap()
}

#[test]
fn scripts_styles_and_images_are_inlined_and_files_nothing_read_are_listed() {
    let f = fold(&files(&[
        (
            "index.html",
            br#"<!doctype html><html><head><title>t</title>
<link rel="stylesheet" href="css/a.css"><script src="js/app.js" defer></script></head>
<body><img src="img/p.png" alt="p"/><video poster='img/p.png' src="v.mp4"></video></body></html>"#,
        ),
        ("css/a.css", b"body{background:url(../img/p.png)}"),
        ("js/app.js", b"document.title='</script>x';"),
        ("img/p.png", b"\x89PNG"),
        ("data/extra.json", b"{}"),
        ("widget.json", b"{}"),
    ]));
    assert!(!f.html.contains("href=\"css/a.css\""));
    assert!(!f.html.contains("src=\"js/app.js\""));
    assert!(
        f.html
            .contains("<style>body{background:url(\"data:image/png;base64,"),
        "{}",
        f.html
    );
    assert!(
        f.html.contains("<\\/script>x"),
        "script content cannot close its element"
    );
    assert!(
        f.html.contains("<script defer>"),
        "other attributes survive: {}",
        f.html
    );
    assert!(
        f.html
            .contains("<img alt=\"p\" src=\"data:image/png;base64,"),
        "{}",
        f.html
    );
    assert!(f.html.contains("poster=\"data:image/png"));
    assert_eq!(
        f.unfolded,
        ["data/extra.json"],
        "v.mp4 is not a file, so it is not listed"
    );
    assert!(f.external.is_empty());
}

#[test]
fn the_policy_is_the_first_element_whatever_the_document_looks_like() {
    for (html, label) in [
        (
            "<!doctype html><html><head><title>x</title></head><body></body></html>",
            "head",
        ),
        ("<!doctype html><html><body>hi</body></html>", "no head"),
        (
            "<!doctype html><title>demo</title><p>demo widget</p>",
            "bare",
        ),
        ("<p>fragment</p>", "fragment"),
        ("<HEAD><meta charset=\"utf-8\"></HEAD>", "uppercase"),
    ] {
        let out = with_policy(html, "default-src 'none'");
        let first = out
            .find("<meta http-equiv=\"Content-Security-Policy\"")
            .unwrap();
        let other = out
            .find("<title")
            .or_else(|| out.find("<p>"))
            .or_else(|| out.find("<body"));
        if let Some(o) = other {
            assert!(first < o, "{label}: {out}");
        }
        assert_eq!(out.matches("Content-Security-Policy").count(), 1, "{label}");
    }
    // A document that names its charset is not given a second one.
    let out = with_policy("<head><meta charset=\"utf-8\"></head>", "p");
    assert_eq!(out.matches("charset").count(), 1);
}

#[test]
fn a_widget_document_reports_its_size_behind_the_policy_and_charset() {
    for (html, label) in [
        (
            "<!doctype html><html><head><title>t</title></head><body>x</body></html>",
            "full",
        ),
        (
            "<head><meta charset=\"utf-8\"><script>own()</script></head>",
            "own charset",
        ),
        ("<p>fragment</p>", "fragment"),
    ] {
        let out = widget_document(html, "p");
        assert_eq!(out.matches(SIZE_REPORTER).count(), 1, "{label}: {out}");
        let reporter = out.find(SIZE_REPORTER).unwrap();
        assert!(
            out.find("Content-Security-Policy").unwrap() < reporter,
            "{label}"
        );
        // Never between the policy and an added charset, and before the
        // document's own scripts.
        if let Some(c) = out.find(CHARSET_META) {
            if label != "own charset" {
                assert!(c < reporter, "{label}: {out}");
            }
        }
        if let Some(own) = out.find("own()") {
            assert!(reporter < own, "{label}");
        }
    }
    assert!(SIZE_REPORTER.contains("type: 'size'") && !SIZE_REPORTER.contains('"'));
}

#[test]
fn declared_origins_widen_only_the_directives_they_name() {
    let none = widget_policy(None);
    assert!(none.contains("connect-src 'none'") && !none.contains("frame-src"));
    assert!(!none.contains("'self'") && !none.contains("unsafe-eval"));
    let csp = WidgetCsp {
        connect_domains: vec!["https://api.example.org".into()],
        resource_domains: vec!["https://cdn.example.org".into()],
        frame_domains: vec!["https://embed.example.org".into()],
    };
    let p = widget_policy(Some(&csp));
    assert!(p.contains("connect-src https://api.example.org;"));
    assert!(p.contains("script-src 'unsafe-inline' https://cdn.example.org;"));
    assert!(p.contains("frame-src https://embed.example.org;"));
    assert!(p.contains("form-action 'none'") && p.contains("base-uri 'none'"));
}

#[test]
fn wasm_unsafe_eval_is_scoped_to_declaring_runtimes() {
    let none = widget_policy(None);
    assert!(!none.contains("wasm-unsafe-eval"));
    assert!(!none.contains("unsafe-eval"));
    let declared = widget_policy_for(None, true);
    assert!(
        declared.contains("script-src 'unsafe-inline' 'wasm-unsafe-eval';"),
        "{declared}"
    );
    // Only the script directive widens: every other directive is intact.
    for directive in [
        "style-src 'unsafe-inline';",
        "img-src data: blob:;",
        "connect-src 'none';",
        "form-action 'none';",
        "base-uri 'none'",
    ] {
        assert!(declared.contains(directive), "{directive} in {declared}");
    }
    // Origins and the flag compose.
    let csp = WidgetCsp {
        connect_domains: vec!["https://api.example.org".into()],
        resource_domains: vec!["https://cdn.example.org".into()],
        frame_domains: vec![],
    };
    let p = widget_policy_for(Some(&csp), true);
    assert!(
        p.contains("script-src 'unsafe-inline' 'wasm-unsafe-eval' https://cdn.example.org;"),
        "{p}"
    );
    let q = widget_policy_for(Some(&csp), false);
    assert!(!q.contains("wasm-unsafe-eval"), "{q}");
    assert!(
        q.contains("script-src 'unsafe-inline' https://cdn.example.org;"),
        "{q}"
    );
}

#[test]
fn nothing_outside_the_given_files_is_read() {
    let f = fold(&files(&[(
        "index.html",
        b"<script src=\"../secret.js\"></script><img src=\"/etc/hostname\"><img src=\"sub/../../x.png\">",
    )]));
    assert!(
        f.html.contains("src=\"../secret.js\""),
        "left as written, not inlined"
    );
    assert!(!f.html.contains("base64,"));
    assert_eq!(join("", "../x"), None);
    assert_eq!(join("a", "../b/./c.js"), Some("b/c.js".into()));
    assert_eq!(join("", "/etc/hostname"), None);
    assert_eq!(join("", "C:/x"), None);
    let e = fold_bundle(&files(&[("other.html", b"x")]), "p").unwrap_err();
    assert!(e.contains("no index.html"), "{e}");
    let e = fold_bundle(&files(&[("index.html", b"\xff")]), "p").unwrap_err();
    assert!(e.contains("not UTF-8"), "{e}");
}

#[test]
fn external_references_and_module_imports_are_reported() {
    let f = fold(&files(&[
        (
            "index.html",
            b"<script src=\"https://cdn.example.org/x.js\"></script><script type=\"module\" src=\"m.js\"></script><style>@import 'x.css';</style>",
        ),
        ("m.js", b"import {a} from './a.js'; a();"),
    ]));
    assert_eq!(f.external, ["https://cdn.example.org/x.js"]);
    assert!(
        f.notes.iter().any(|n| n.contains("imports")),
        "{:?}",
        f.notes
    );
    assert!(
        f.notes.iter().any(|n| n.contains("@import")),
        "{:?}",
        f.notes
    );
    assert!(f.html.contains("src=\"https://cdn.example.org/x.js\""));
}

#[test]
fn text_and_attribute_escapes_cover_markup_characters() {
    assert_eq!(text("a<b>&"), "a&lt;b&gt;&amp;");
    assert_eq!(attr("a\"<b>&"), "a&quot;&lt;b&gt;&amp;");
    assert_eq!(percent_decode("a%20b%zz%"), "a b%zz%");
    assert_eq!(mime_for("GLB"), "model/gltf-binary");
    assert_eq!(mime_for("weird"), "application/octet-stream");
}
