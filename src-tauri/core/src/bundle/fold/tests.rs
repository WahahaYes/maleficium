use super::*;

fn write(dir: &Path, rel: &str, body: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn bundle(name: &str) -> PathBuf {
    let d = crate::test_scratch::dir(&format!("fold-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    dunce::canonicalize(&d).unwrap()
}

fn fold(dir: &Path) -> Folded {
    fold_bundle(dir, &dir.join("index.html"), &widget_policy(None)).unwrap()
}

#[test]
fn scripts_styles_and_images_are_inlined_and_files_nothing_read_are_listed() {
    let d = bundle("basic");
    write(
        &d,
        "index.html",
        br#"<!doctype html><html><head><title>t</title>
<link rel="stylesheet" href="css/a.css"><script src="js/app.js" defer></script></head>
<body><img src="img/p.png" alt="p"/><video poster='img/p.png' src="v.mp4"></video></body></html>"#,
    );
    write(&d, "css/a.css", b"body{background:url(../img/p.png)}");
    write(&d, "js/app.js", b"document.title='</script>x';");
    write(&d, "img/p.png", b"\x89PNG");
    write(&d, "data/extra.json", b"{}");
    write(&d, "widget.json", b"{}");
    let f = fold(&d);
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
fn nothing_outside_the_bundle_folder_is_read() {
    let root = bundle("escape");
    let d = root.join("w");
    write(&root, "secret.js", b"alert('secret')");
    write(
        &d,
        "index.html",
        b"<script src=\"../secret.js\"></script><img src=\"/etc/hostname\">",
    );
    let f = fold(&d);
    assert!(
        f.html.contains("src=\"../secret.js\""),
        "left as written, not inlined"
    );
    assert!(!f.html.contains("alert('secret')"));
    assert!(!f.html.contains("base64,"));
    // A symlink pointing out is not followed either.
    std::os::unix::fs::symlink(root.join("secret.js"), d.join("link.js")).unwrap();
    write(&d, "index.html", b"<script src=\"link.js\"></script>");
    let f = fold(&d);
    assert!(!f.html.contains("secret"), "{}", f.html);
}

#[test]
fn external_references_and_module_imports_are_reported() {
    let d = bundle("external");
    write(
        &d,
        "index.html",
        b"<script src=\"https://cdn.example.org/x.js\"></script><script type=\"module\" src=\"m.js\"></script><style>@import 'x.css';</style>",
    );
    write(&d, "m.js", b"import {a} from './a.js'; a();");
    let f = fold(&d);
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
