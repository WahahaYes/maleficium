//! One red and one green file per scan rule, plus the sample packages.

use super::{scan, Finding};
use std::collections::{BTreeMap, BTreeSet};

fn files(pairs: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
    pairs
        .iter()
        .map(|(p, c)| (p.to_string(), c.as_bytes().to_vec()))
        .collect()
}

fn none() -> BTreeSet<String> {
    BTreeSet::new()
}

fn rules(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.rule).collect()
}

fn entry(body: &str) -> String {
    format!("<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>{body}</body></html>")
}

#[test]
fn url_load_html_red() {
    let r = scan(
        &files(&[(
            "index.html",
            &entry("<img src=\"https://cdn.example.com/a.png\">"),
        )]),
        &none(),
        false,
    );
    assert_eq!(rules(&r.errors), ["url-load"]);
    assert_eq!(r.errors[0].file, "index.html");
    assert!(r.warnings.is_empty());
}

#[test]
fn url_load_html_green() {
    let r = scan(
        &files(&[
            ("index.html", &entry("<img src=\"figures/a.png\">")),
            ("figures/a.png", "bytes"),
        ]),
        &none(),
        false,
    );
    assert!(r.errors.is_empty(), "{:?}", rules(&r.errors));
}

#[test]
fn url_load_css_red() {
    let r = scan(
        &files(&[(
            "style.css",
            "p { background: url(https://cdn.example.com/a.png); }",
        )]),
        &none(),
        false,
    );
    assert_eq!(rules(&r.errors), ["url-load"]);
}

#[test]
fn url_load_css_green() {
    let r = scan(
        &files(&[
            (
                "index.html",
                &entry("<link rel=\"stylesheet\" href=\"style.css\">"),
            ),
            ("style.css", "p { background: url(figures/a.png); }"),
            ("figures/a.png", "bytes"),
        ]),
        &none(),
        false,
    );
    assert!(r.errors.is_empty(), "{:?}", rules(&r.errors));
    assert!(r.warnings.is_empty(), "{:?}", rules(&r.warnings));
}

#[test]
fn url_load_js_red() {
    let r = scan(
        &files(&[("app.js", "var u = \"https://cdn.example.com/x.js\";")]),
        &none(),
        false,
    );
    assert!(rules(&r.errors).contains(&"url-load"));
}

#[test]
fn url_load_js_green() {
    let r = scan(
        &files(&[("app.js", "var u = \"figures/a.png\";")]),
        &none(),
        false,
    );
    assert!(!rules(&r.errors).contains(&"url-load"));
}

#[test]
fn meta_refresh_red() {
    let r = scan(
        &files(&[(
            "index.html",
            &entry("<meta http-equiv=\"refresh\" content=\"0;url=a.html\">"),
        )]),
        &none(),
        false,
    );
    assert_eq!(rules(&r.errors), ["meta-refresh"]);
}

#[test]
fn meta_refresh_green() {
    let r = scan(
        &files(&[(
            "index.html",
            &entry("<meta name=\"viewport\" content=\"x\">"),
        )]),
        &none(),
        false,
    );
    assert!(!rules(&r.errors).contains(&"meta-refresh"));
}

#[test]
fn eval_red() {
    for (rule, body) in [
        ("eval call", "eval(\"1+1\");"),
        ("new Function", "var f = new Function(\"a\", \"b\");"),
        ("string setTimeout", "setTimeout(\"draw()\", 100);"),
        ("string setInterval", "setInterval('tick()', 100);"),
    ] {
        let r = scan(&files(&[("app.js", body)]), &none(), false);
        assert_eq!(rules(&r.errors), ["eval"], "{rule}");
    }
}

#[test]
fn eval_green() {
    let r = scan(
        &files(&[("app.js", "setTimeout(draw, 100); function draw() {}")]),
        &none(),
        false,
    );
    assert!(!rules(&r.errors).contains(&"eval"));
}

#[test]
fn module_import_red() {
    for (rule, body) in [
        ("static import", "import { x } from \"./x.js\";"),
        ("dynamic import", "var m = import(\"./x.js\");"),
        ("export", "export function draw() {}"),
    ] {
        let r = scan(
            &files(&[("app.js", body), ("x.js", "var x = 1;")]),
            &none(),
            false,
        );
        assert!(rules(&r.errors).contains(&"module-import"), "{rule}");
    }
}

#[test]
fn module_import_green() {
    let r = scan(
        &files(&[("app.js", "(function () { var x = 1; })();")]),
        &none(),
        false,
    );
    assert!(!rules(&r.errors).contains(&"module-import"));
}

#[test]
fn network_api_red() {
    for (rule, body) in [
        ("fetch", "fetch(\"data.csv\");"),
        ("xhr", "var q = new XMLHttpRequest();"),
        ("socket", "var s = new WebSocket(\"wss://x\");"),
        ("events", "var s = new EventSource(\"/stream\");"),
    ] {
        let r = scan(&files(&[("app.js", body)]), &none(), false);
        assert!(rules(&r.errors).contains(&"network-api"), "{rule}");
    }
}

#[test]
fn network_api_green() {
    let r = scan(&files(&[("app.js", "var x = 1;")]), &none(), false);
    assert!(!rules(&r.errors).contains(&"network-api"));
}

#[test]
fn worker_red() {
    for (rule, body) in [
        ("worker", "var w = new Worker(\"w.js\");"),
        ("importScripts", "importScripts(\"w.js\");"),
    ] {
        let r = scan(
            &files(&[("app.js", body), ("w.js", "var x = 1;")]),
            &none(),
            false,
        );
        assert!(rules(&r.errors).contains(&"worker"), "{rule}");
    }
}

#[test]
fn worker_green() {
    let r = scan(&files(&[("app.js", "var x = 1;")]), &none(), false);
    assert!(!rules(&r.errors).contains(&"worker"));
}

#[test]
fn storage_red() {
    for (rule, body) in [
        ("local", "localStorage.setItem(\"k\", \"v\");"),
        ("session", "sessionStorage.getItem(\"k\");"),
        ("idb", "var d = indexedDB;"),
        ("cookie", "document.cookie = \"k=v\";"),
    ] {
        let r = scan(&files(&[("app.js", body)]), &none(), false);
        assert!(rules(&r.errors).contains(&"storage"), "{rule}");
    }
}

#[test]
fn storage_green() {
    let r = scan(&files(&[("app.js", "var kept = {};")]), &none(), false);
    assert!(!rules(&r.errors).contains(&"storage"));
}

#[test]
fn missing_ref_red() {
    let r = scan(
        &files(&[("index.html", &entry("<img src=\"gone.png\">"))]),
        &none(),
        false,
    );
    assert_eq!(rules(&r.errors), ["missing-ref"]);
}

#[test]
fn missing_ref_green() {
    let r = scan(
        &files(&[
            ("index.html", &entry("<img src=\"figures/a.png\">")),
            ("figures/a.png", "bytes"),
        ]),
        &none(),
        false,
    );
    assert!(!rules(&r.errors).contains(&"missing-ref"));
}

#[test]
fn wasm_red_without_the_declaration() {
    // The WebAssembly identifier, in a script file and inline.
    for body in [
        "WebAssembly.instantiate(bytes).then(function (o) { o.instance.exports.add(1, 2); });",
        "var m = new WebAssembly.Module(bytes);",
    ] {
        let r = scan(&files(&[("app.js", body)]), &none(), false);
        assert_eq!(rules(&r.errors), ["wasm"], "{body}");
        assert!(r.errors[0].message.contains("capabilities.wasm"));
    }
    let r = scan(
        &files(&[(
            "index.html",
            &entry("<script>WebAssembly.compile(b);</script>"),
        )]),
        &none(),
        false,
    );
    assert_eq!(rules(&r.errors), ["wasm"]);
    // A .wasm module string in script, even without the identifier.
    let r = scan(
        &files(&[(
            "app.js",
            "fetch(\"calc.wasm\").then(function (r) { return r.arrayBuffer(); });",
        )]),
        &none(),
        false,
    );
    assert!(rules(&r.errors).contains(&"wasm"));
    // A .wasm file reference from markup: refused even when the file
    // exists (and still dangling when it does not).
    let r = scan(
        &files(&[
            ("index.html", &entry("<script src=\"calc.wasm\"></script>")),
            ("calc.wasm", "bytes"),
        ]),
        &none(),
        false,
    );
    assert_eq!(rules(&r.errors), ["wasm"]);
    let r = scan(
        &files(&[("index.html", &entry("<script src=\"gone.wasm\"></script>"))]),
        &none(),
        false,
    );
    assert_eq!(rules(&r.errors), ["missing-ref", "wasm"]);
}

#[test]
fn wasm_green_when_declared_or_absent() {
    let body = "WebAssembly.instantiate(bytes); fetch(\"calc.wasm\");";
    let r = scan(&files(&[("app.js", body)]), &none(), true);
    assert!(
        !rules(&r.errors).contains(&"wasm"),
        "{:?}",
        rules(&r.errors)
    );
    let r = scan(
        &files(&[
            ("index.html", &entry("<script src=\"calc.wasm\"></script>")),
            ("calc.wasm", "bytes"),
        ]),
        &none(),
        true,
    );
    assert!(r.errors.is_empty(), "{:?}", rules(&r.errors));
    // Plain code with no WASM in it stays silent either way.
    let r = scan(&files(&[("app.js", "var x = 1;")]), &none(), false);
    assert!(!rules(&r.errors).contains(&"wasm"));
    // A word that merely ends in wasm without the dot is not a module.
    let r = scan(&files(&[("app.js", "var wasmx = 1;")]), &none(), false);
    assert!(!rules(&r.errors).contains(&"wasm"));
}

#[test]
fn vendored_url_is_a_warning() {
    let mut vendored = BTreeSet::new();
    vendored.insert("vendor/lib.js".to_string());
    let r = scan(
        &files(&[
            (
                "index.html",
                &entry("<script src=\"vendor/lib.js\"></script>"),
            ),
            ("vendor/lib.js", "var u = \"https://cdn.example.com/x.js\";"),
        ]),
        &vendored,
        false,
    );
    assert!(r.errors.is_empty(), "{:?}", rules(&r.errors));
    assert_eq!(rules(&r.warnings), ["vendored-url"]);
    assert_eq!(r.warnings[0].file, "vendor/lib.js");
}

#[test]
fn vendored_files_get_no_other_rule() {
    let mut vendored = BTreeSet::new();
    vendored.insert("vendor/lib.js".to_string());
    let r = scan(
        &files(&[
            (
                "index.html",
                &entry("<script src=\"vendor/lib.js\"></script>"),
            ),
            ("vendor/lib.js", "eval(\"1\"); localStorage.x;"),
        ]),
        &vendored,
        false,
    );
    assert!(r.errors.is_empty(), "{:?}", rules(&r.errors));
    assert!(r.warnings.is_empty(), "{:?}", rules(&r.warnings));
}

#[test]
fn unreferenced_red() {
    let r = scan(
        &files(&[
            ("index.html", &entry("<p>hi</p>")),
            ("extra.js", "var x = 1;"),
        ]),
        &none(),
        false,
    );
    assert_eq!(rules(&r.warnings), ["unreferenced"]);
    assert_eq!(r.warnings[0].file, "extra.js");
}

#[test]
fn unreferenced_excludes_the_metadata_set() {
    let r = scan(
        &files(&[
            ("index.html", &entry("<p>hi</p>")),
            ("runtime.json", "{}"),
            ("README.md", "docs"),
            ("LICENSE", "terms"),
            ("samples/data.csv", "a,b\n1,2\n"),
        ]),
        &none(),
        false,
    );
    assert!(r.warnings.is_empty(), "{:?}", rules(&r.warnings));
}

#[test]
fn global_hook_red() {
    let r = scan(
        &files(&[
            ("index.html", &entry("<script src=\"app.js\"></script>")),
            ("app.js", "window.__ready = true;"),
        ]),
        &none(),
        false,
    );
    assert!(r.errors.is_empty());
    assert_eq!(rules(&r.warnings), ["global-hook"]);
}

#[test]
fn global_hook_green() {
    let r = scan(
        &files(&[("app.js", "window.parent.postMessage({ mfw: 1 }, \"*\");")]),
        &none(),
        false,
    );
    assert!(!rules(&r.warnings).contains(&"global-hook"));
}

#[test]
fn size_red() {
    let mut pairs: Vec<(String, Vec<u8>)> =
        vec![("index.html".to_string(), entry("<p>hi</p>").into_bytes())];
    pairs.push(("big.bin".to_string(), vec![0u8; 6 * 1024 * 1024]));
    let map: BTreeMap<String, Vec<u8>> = pairs.into_iter().collect();
    let r = scan(&map, &none(), false);
    assert!(r.errors.is_empty());
    assert!(rules(&r.warnings).contains(&"size"));
}

#[test]
fn size_green() {
    let r = scan(
        &files(&[("index.html", &entry("<p>hi</p>"))]),
        &none(),
        false,
    );
    assert!(!rules(&r.warnings).contains(&"size"));
}

/// One sample package as the scanner sees it: `runtime.json`, the entry,
// the role sample, and the licence.
fn include_sample(name: &str) -> BTreeMap<String, Vec<u8>> {
    match name {
        "heatmap@1" => BTreeMap::from([
            (
                "runtime.json".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/heatmap@1/runtime.json")
                    .to_vec(),
            ),
            (
                "index.html".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/heatmap@1/index.html")
                    .to_vec(),
            ),
            (
                "samples/data.csv".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/heatmap@1/samples/data.csv")
                    .to_vec(),
            ),
            (
                "LICENSE".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/heatmap@1/LICENSE").to_vec(),
            ),
        ]),
        "stl-viewer@1" => BTreeMap::from([
            (
                "runtime.json".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/stl-viewer@1/runtime.json")
                    .to_vec(),
            ),
            (
                "index.html".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/stl-viewer@1/index.html")
                    .to_vec(),
            ),
            (
                "samples/model.stl".to_string(),
                include_bytes!(
                    "../../../../../docs/runtimes/samples/stl-viewer@1/samples/model.stl"
                )
                .to_vec(),
            ),
            (
                "LICENSE".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/stl-viewer@1/LICENSE")
                    .to_vec(),
            ),
        ]),
        "caption-overlay@1" => BTreeMap::from([
            (
                "runtime.json".to_string(),
                include_bytes!(
                    "../../../../../docs/runtimes/samples/caption-overlay@1/runtime.json"
                )
                .to_vec(),
            ),
            (
                "index.html".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/caption-overlay@1/index.html")
                    .to_vec(),
            ),
            (
                "samples/photo.svg".to_string(),
                include_bytes!(
                    "../../../../../docs/runtimes/samples/caption-overlay@1/samples/photo.svg"
                )
                .to_vec(),
            ),
            (
                "LICENSE".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/caption-overlay@1/LICENSE")
                    .to_vec(),
            ),
        ]),
        _ => BTreeMap::from([
            (
                "runtime.json".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/bad-cdn@1/runtime.json")
                    .to_vec(),
            ),
            (
                "index.html".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/bad-cdn@1/index.html")
                    .to_vec(),
            ),
            (
                "samples/data.csv".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/bad-cdn@1/samples/data.csv")
                    .to_vec(),
            ),
            (
                "LICENSE".to_string(),
                include_bytes!("../../../../../docs/runtimes/samples/bad-cdn@1/LICENSE").to_vec(),
            ),
        ]),
    }
}

#[test]
fn good_samples_scan_clean() {
    for name in ["heatmap@1", "stl-viewer@1", "caption-overlay@1"] {
        let r = scan(&include_sample(name), &none(), false);
        assert!(r.errors.is_empty(), "{name}: {:?}", rules(&r.errors));
        assert!(r.warnings.is_empty(), "{name}: {:?}", rules(&r.warnings));
    }
}

#[test]
fn bad_cdn_fails_on_its_script_line() {
    let r = scan(&include_sample("bad-cdn@1"), &none(), false);
    assert_eq!(r.errors.len(), 1, "{:?}", rules(&r.errors));
    assert_eq!(r.errors[0].rule, "url-load");
    assert_eq!(r.errors[0].file, "index.html");
    let text = str::from_utf8(include_bytes!(
        "../../../../../docs/runtimes/samples/bad-cdn@1/index.html"
    ))
    .unwrap();
    let expected = text
        .lines()
        .position(|l| l.contains("cdn.example.com"))
        .unwrap() as u32
        + 1;
    assert_eq!(r.errors[0].line, expected);
}
