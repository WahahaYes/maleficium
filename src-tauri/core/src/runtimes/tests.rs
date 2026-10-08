use super::*;
use crate::widgets::{WidgetOption, WidgetRect, WidgetType};
use serde_json::json;

fn good() -> Value {
    json!({
        "contract": 1,
        "name": "heat",
        "version": "1.2.3",
        "title": "Heat",
        "description": "A grid.",
        "authors": ["A. Author"],
        "license": "MIT",
        "sources": {
            "data": {"primary": true, "required": true, "extensions": ["csv", "tsv"]},
            "labels": {"required": false, "extensions": ["txt"], "maxBytes": 10}
        },
        "options": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "scheme": {"type": "string", "enum": ["seq", "div"], "default": "seq"},
                "caption": {"type": "string", "maxLength": 5},
                "rows": {"type": "integer", "minimum": 1, "maximum": 10, "default": 3},
                "gain": {"type": "number", "minimum": -1.5, "maximum": 2.5},
                "grid": {"type": "boolean", "default": false}
            }
        },
        "capabilities": {"webgl": false},
        "vendored": [{
            "name": "tiny-lib",
            "version": "2.0.1",
            "license": "ISC",
            "source": "https://example.org/tiny-lib",
            "files": ["vendor/tiny/tiny.js"],
            "licenseFile": "vendor/tiny/LICENSE"
        }]
    })
}

fn parse(v: &Value) -> Result<RuntimeManifest, String> {
    parse_manifest("heat@1", &v.to_string())
}

fn err(v: Value) -> String {
    parse(&v).unwrap_err()
}

fn with(mut v: Value, path: &[&str], x: Value) -> Value {
    let mut at = &mut v;
    for p in &path[..path.len() - 1] {
        at = at.get_mut(*p).unwrap();
    }
    at.as_object_mut()
        .unwrap()
        .insert(path[path.len() - 1].to_string(), x);
    v
}

fn without(mut v: Value, path: &[&str]) -> Value {
    let mut at = &mut v;
    for p in &path[..path.len() - 1] {
        at = at.get_mut(*p).unwrap();
    }
    at.as_object_mut().unwrap().remove(path[path.len() - 1]);
    v
}

fn package(manifest: &Value) -> BTreeMap<String, Vec<u8>> {
    [
        ("runtime.json", manifest.to_string()),
        ("index.html", "<!doctype html><p>x</p>".to_string()),
        ("vendor/tiny/tiny.js", "var t=1;".to_string()),
        ("vendor/tiny/LICENSE", "ISC licence text".to_string()),
        ("samples/grid.csv", "1,2\n3,4\n".to_string()),
        ("LICENSE", "MIT licence text".to_string()),
    ]
    .into_iter()
    .map(|(p, s)| (p.to_string(), s.into_bytes()))
    .collect()
}

#[test]
fn refs_names_and_reserved_names() {
    for good in [
        "stl-viewer@1",
        "ab@9999",
        "heatmap@12",
        "x2@3",
        "custom-chart@1",
    ] {
        assert!(valid_ref(good), "{good}");
    }
    for bad in [
        "model@1",
        "video@1",
        "table@1",
        "chart@1",
        "html@1",
        "custom@1",
        "m-x@1",
        "maleficium-x@1",
        "maleficiumx@1",
        "x@0",
        "ab@0",
        "ab@10000",
        "ab@01",
        "a@1",
        "Ab@1",
        "1ab@1",
        "ab",
        "ab@",
        "@1",
        "ab@1 ",
        "ab@1.0",
        "ab_c@1",
        &format!("a{}@1", "b".repeat(40)),
    ] {
        assert!(!valid_ref(bad), "{bad}");
    }
    assert_eq!(split_ref("stl-viewer@12"), Some(("stl-viewer", 12)));
    // The package's own reserved list: the same one the .sty refuses.
    let sty = include_str!("../../../../embed-runtime/tex/maleficium-interactive.sty");
    assert!(sty.contains("(?:model|video|table|chart|html|custom)"));
    for k in RESERVED_OPTIONS {
        assert!(
            sty.contains(&format!("|{k}|"))
                || sty.contains(&format!("(?:{k}|"))
                || sty.contains(&format!("|{k})")),
            "{k}"
        );
    }
    assert!(valid_role("texture") && !valid_role("primary") && !valid_role("a-b"));
    assert!(!valid_role(&"a".repeat(17)) && valid_role(&"a".repeat(16)));
    assert!(valid_option_key("autorotate") && !valid_option_key("auto-rotate"));
    for p in ["index.html", "vendor/a/b.js", "a.b-c_d"] {
        assert!(valid_path(p), "{p}");
    }
    for p in [
        "", "/a", "a/", "a//b", "../a", "a/../b", "./a", "a/.", "a b", "a\\b", "é",
    ] {
        assert!(!valid_path(p), "{p}");
    }
}

#[test]
fn a_good_manifest_parses_with_its_defaults() {
    let m = parse(&good()).unwrap();
    assert_eq!((m.name.as_str(), m.version.as_str()), ("heat", "1.2.3"));
    assert_eq!(m.primary_role(), "data");
    assert_eq!(m.sources["data"].max_bytes, DEFAULT_MAX_BYTES);
    assert_eq!(m.sources["labels"].max_bytes, 10);
    assert!(!m.capabilities.webgl);
    assert!(!m.capabilities.wasm, "contract 1 packages omit wasm");
    assert!(m.is_metadata("runtime.json") && m.is_metadata("samples/a.csv"));
    assert!(m.is_metadata("vendor/tiny/LICENSE") && m.is_metadata("README.md"));
    assert!(!m.is_metadata("vendor/tiny/tiny.js") && !m.is_metadata("index.html"));
    // No options at all is fine.
    assert!(parse(&without(good(), &["options"]))
        .unwrap()
        .options
        .is_none());
}

#[test]
fn wasm_capability_parses_true_false_and_defaults_off() {
    assert!(
        parse(&with(good(), &["capabilities", "wasm"], json!(true)))
            .unwrap()
            .capabilities
            .wasm
    );
    assert!(
        !parse(&with(good(), &["capabilities", "wasm"], json!(false)))
            .unwrap()
            .capabilities
            .wasm
    );
    // Contract 1 packages omit the flag: they parse with WASM off.
    assert!(!parse(&good()).unwrap().capabilities.wasm);
    let e = err(with(good(), &["capabilities", "wasm"], json!("yes")));
    assert!(e.contains("invalid type"), "{e}");
}

#[test]
fn every_manifest_rule_has_its_message() {
    let cases: Vec<(Value, &str)> = vec![
        (with(good(), &["extra"], json!(1)), "runtime.json: unknown field `extra`"),
        (with(good(), &["contract"], json!(2)), "contract must be 1, found 2"),
        (with(good(), &["contract"], json!("1")), "contract must be 1, found \"1\""),
        (without(good(), &["contract"]), "missing field `contract`"),
        (without(good(), &["vendored"]), "missing field `vendored`"),
        (with(good(), &["name"], json!("other")), "name `other` does not match its folder"),
        (with(good(), &["version"], json!("2.0.0")), "version `2.0.0` is not MAJOR.MINOR.PATCH with major 1"),
        (with(good(), &["version"], json!("1.0")), "is not MAJOR.MINOR.PATCH"),
        (with(good(), &["version"], json!("1.01.0")), "is not MAJOR.MINOR.PATCH"),
        (with(good(), &["title"], json!("")), "title must be 1 to 80"),
        (with(good(), &["title"], json!("x".repeat(81))), "title must be 1 to 80"),
        (with(good(), &["title"], json!("a\u{7}b")), "title must be 1 to 80"),
        (with(good(), &["description"], json!("x".repeat(501))), "description must be at most 500"),
        (with(good(), &["authors"], json!([])), "authors must list 1 to 16"),
        (with(good(), &["authors"], json!(vec!["a"; 17])), "authors must list 1 to 16"),
        (with(good(), &["authors"], json!(["x".repeat(101)])), "each name must be 1 to 100"),
        (
            with(good(), &["license"], json!("GPL-3.0")),
            "license `GPL-3.0` is not on the allowlist (MIT, MIT-0, BSD-2-Clause, BSD-3-Clause, Apache-2.0, ISC, Zlib, 0BSD, CC0-1.0, Unlicense)",
        ),
        (with(good(), &["capabilities", "webgl"], json!("yes")), "runtime.json: invalid type"),
        (with(good(), &["sources"], json!({})), "sources: a runtime takes 1 to 8 roles"),
        (with(good(), &["sources", "Bad"], json!({"required": false, "extensions": ["a"]})), "sources: `Bad` is not a role name"),
        (with(good(), &["sources", "primary"], json!({"required": false, "extensions": ["a"]})), "sources: `primary` is not a role name"),
        (with(good(), &["sources", "labels", "primary"], json!(true)), "sources: exactly one role must be primary, found 2"),
        (with(good(), &["sources", "data", "primary"], json!(false)), "sources: exactly one role must be primary, found 0"),
        (with(good(), &["sources", "data", "required"], json!(false)), "sources: the primary role data must be required"),
        (with(good(), &["sources", "data", "extensions"], json!(["CSV"])), "sources.data: `CSV` is not a plain extension"),
        (with(good(), &["sources", "data", "extensions"], json!([])), "sources.data: extensions must list 1 to 16"),
        (with(good(), &["sources", "data", "maxBytes"], json!(0)), "sources.data: maxBytes must be 1 to 536870912"),
        (with(good(), &["sources", "data", "maxBytes"], json!(536870913u64)), "maxBytes must be 1 to"),
        (with(good(), &["sources", "data", "mimes"], json!(["text/csv"])), "unknown field `mimes`"),
        (with(good(), &["options", "type"], json!("array")), "options: type must be \"object\""),
        (with(good(), &["options", "additionalProperties"], json!(true)), "options: additionalProperties must be false"),
        (with(good(), &["options", "properties", "camera"], json!({"type": "string"})), "options: `camera` is not an allowed option name"),
        (with(good(), &["options", "properties", "height"], json!({"type": "string"})), "options: `height` is not an allowed option name"),
        (with(good(), &["options", "properties", "Caps"], json!({"type": "string"})), "options: `Caps` is not an allowed option name"),
        (with(good(), &["options", "properties", "x"], json!({"type": "object"})), "unknown variant `object`"),
        (with(good(), &["options", "properties", "x"], json!({"type": "string", "pattern": "."})), "unknown field `pattern`"),
        (with(good(), &["options", "properties", "rows", "enum"], json!(["1"])), "options.rows: enum applies to string options only"),
        (with(good(), &["options", "properties", "scheme", "enum"], json!([])), "options.scheme: enum must list 1 to 32"),
        (with(good(), &["options", "properties", "scheme", "enum"], json!(["a,b", "c"])), "options.scheme: enum value `a,b`"),
        (with(good(), &["options", "properties", "scheme", "enum"], json!(["a", "a"])), "options.scheme: enum lists a value twice"),
        (with(good(), &["options", "properties", "caption", "maxLength"], json!(0)), "options.caption: maxLength must be 1 to 200"),
        (with(good(), &["options", "properties", "caption", "maxLength"], json!(201)), "options.caption: maxLength must be 1 to 200"),
        (with(good(), &["options", "properties", "grid", "minimum"], json!(0)), "options.grid: minimum and maximum apply to number"),
        (with(good(), &["options", "properties", "rows", "minimum"], json!(11)), "options.rows: minimum is above maximum"),
        (with(good(), &["options", "properties", "rows", "minimum"], json!(0.5)), "whole for an integer"),
        (with(good(), &["options", "properties", "rows", "default"], json!(11)), "options.rows: default does not validate: option rows=11 is above 10"),
        (with(good(), &["options", "properties", "rows", "default"], json!(1.5)), "default does not validate: option rows=`1.5` is not an integer"),
        (with(good(), &["options", "properties", "scheme", "default"], json!("hot")), "default does not validate: option scheme=hot is not one of seq, div"),
        (with(good(), &["options", "properties", "caption", "default"], json!("toolong")), "is longer than 5 characters"),
        (with(good(), &["options", "properties", "grid", "default"], json!("false")), "option grid=`false` is not a boolean"),
        (with(good(), &["options", "properties", "caption", "description"], json!("x".repeat(201))), "options.caption: description must be at most 200"),
    ];
    for (v, want) in cases {
        let e = err(v);
        assert!(e.starts_with("runtime heat@1: "), "{e}");
        assert!(e.contains(want), "want `{want}` in: {e}");
    }
    let mut big = good();
    let props = big["options"]["properties"].as_object_mut().unwrap();
    for i in 0..30 {
        props.insert(format!("k{i}"), json!({"type": "boolean"}));
    }
    assert!(err(big).contains("options: at most 32 properties"));
    let mut many = good();
    let roles = many["sources"].as_object_mut().unwrap();
    for i in 0..7 {
        roles.insert(
            format!("r{i}"),
            json!({"required": false, "extensions": ["a"]}),
        );
    }
    assert!(err(many).contains("sources: a runtime takes 1 to 8 roles"));

    // Vendored entries.
    let vend = |field: &str, x: Value| {
        with(good(), &["vendored"], {
            let mut e = good()["vendored"][0].clone();
            e.as_object_mut().unwrap().insert(field.into(), x);
            json!([e])
        })
    };
    for (v, want) in [
        (
            vend("license", json!("GPL-2.0")),
            "vendored tiny-lib: license `GPL-2.0` is not on the allowlist",
        ),
        (
            vend("license", json!("AGPL-3.0-only")),
            "is not on the allowlist",
        ),
        (
            vend("name", json!("Tiny Lib")),
            "vendored `Tiny Lib` is not a plain library name",
        ),
        (
            vend("version", json!("1 0")),
            "vendored tiny-lib: version `1 0` is not plain",
        ),
        (
            vend("source", json!("x".repeat(2049))),
            "source is longer than 2048",
        ),
        (vend("files", json!([])), "files must list 1 to 256"),
        (
            vend("files", json!(["vendor/../index.html"])),
            "path `vendor/../index.html` is not a plain relative path",
        ),
        (
            vend("files", json!(["/etc/passwd"])),
            "path `/etc/passwd` is not a plain relative path",
        ),
        (
            vend("files", json!(["lib/tiny.js"])),
            "vendored tiny-lib: lib/tiny.js is not under vendor/",
        ),
        (
            vend("licenseFile", json!("../LICENSE")),
            "path `../LICENSE` is not a plain relative path",
        ),
        (vend("homepage", json!("x")), "unknown field `homepage`"),
    ] {
        let e = err(v);
        assert!(e.contains(want), "want `{want}` in: {e}");
    }

    // Size and syntax come before anything else.
    let huge = format!("{{\"pad\":\"{}\"}}", "x".repeat(64 * 1024));
    assert_eq!(
        parse_manifest("heat@1", &huge).unwrap_err(),
        "runtime heat@1: runtime.json is larger than 64 KiB"
    );
    assert!(parse_manifest("heat@1", "{")
        .unwrap_err()
        .starts_with("runtime heat@1: runtime.json is not valid JSON: "));
}

#[test]
fn every_package_rule_has_its_message() {
    let m = good();
    assert_eq!(check_package("heat@1", &package(&m)).unwrap().name, "heat");
    let drop = |p: &str| {
        let mut f = package(&m);
        f.remove(p);
        check_package("heat@1", &f).unwrap_err()
    };
    assert_eq!(drop("index.html"), "runtime heat@1: index.html is missing");
    assert_eq!(
        drop("runtime.json"),
        "runtime heat@1: runtime.json is missing"
    );
    assert_eq!(
        drop("vendor/tiny/tiny.js"),
        "runtime heat@1: vendored tiny-lib: vendor/tiny/tiny.js is missing"
    );
    assert_eq!(
        drop("vendor/tiny/LICENSE"),
        "runtime heat@1: vendored tiny-lib: vendor/tiny/LICENSE is missing"
    );
    assert_eq!(
        drop("samples/grid.csv"),
        "runtime heat@1: samples/ has no .csv or .tsv file for required role data"
    );
    // A sample of an optional role is not needed; one outside samples/ does not count.
    let mut f = package(&m);
    f.remove("samples/grid.csv");
    f.insert("grid.csv".into(), b"1".to_vec());
    assert!(check_package("heat@1", &f)
        .unwrap_err()
        .contains("samples/ has no"));
    for hostile in [
        "../escape.js",
        "a//b.js",
        "./x.js",
        "dir/../x.js",
        "sp ace.js",
    ] {
        let mut f = package(&m);
        f.insert(hostile.to_string(), b"x".to_vec());
        assert_eq!(
            check_package("heat@1", &f).unwrap_err(),
            format!("runtime heat@1: path `{hostile}` is not a plain relative path")
        );
    }
    let mut f = package(&m);
    f.insert("runtime.json".into(), vec![b' '; 64 * 1024 + 1]);
    assert_eq!(
        check_package("heat@1", &f).unwrap_err(),
        "runtime heat@1: runtime.json is larger than 64 KiB"
    );
    f.insert("runtime.json".into(), vec![0xff, 0xfe]);
    assert!(check_package("heat@1", &f)
        .unwrap_err()
        .starts_with("runtime heat@1: runtime.json is not valid JSON: "));
    // The folder name is the check, so a package copied under another name fails.
    assert_eq!(
        check_package("cold@1", &package(&m)).unwrap_err(),
        "runtime cold@1: name `heat` does not match its folder"
    );
}

fn read_dir(dir: &std::path::Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if e.file_type().unwrap().is_dir() {
            read_dir(&e.path(), &rel, out);
        } else {
            out.insert(rel, std::fs::read(e.path()).unwrap());
        }
    }
}

/// The documented samples, as lane C ships them.
pub(crate) fn sample(reference: &str) -> BTreeMap<String, Vec<u8>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../embed-runtime/samples")
        .join(reference);
    let mut out = BTreeMap::new();
    read_dir(&dir, "", &mut out);
    out
}

#[test]
fn every_documented_sample_passes_the_package_check() {
    for r in [
        "heatmap@1",
        "stl-viewer@1",
        "caption-overlay@1",
        "wasm-sum@1",
        "bad-cdn@1",
    ] {
        let m = check_package(r, &sample(r)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(format!("{}@1", m.name), r);
        assert!(m.vendored.is_empty());
    }
}

fn widget(sources: &[(&str, &str)], options: &[(&str, &str)]) -> Widget {
    Widget {
        id: "fig-h".into(),
        kind: WidgetType::Custom,
        runtime: Some("heat@1".into()),
        label: None,
        figure: None,
        theme: "house".into(),
        poster: Some("p.png".into()),
        sources: sources
            .iter()
            .map(|(r, p)| WidgetSource {
                role: r.to_string(),
                path: p.to_string(),
            })
            .collect(),
        options: options
            .iter()
            .map(|(k, v)| WidgetOption {
                key: k.to_string(),
                value: v.to_string(),
            })
            .collect(),
        alt: "a".into(),
        page: 1,
        rect: WidgetRect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        csp: None,
        plate: None,
        line_width: None,
    }
}

fn manifest() -> RuntimeManifest {
    parse(&good()).unwrap()
}

#[test]
fn bind_maps_the_primary_role_and_checks_every_source() {
    let m = manifest();
    let b = bind(
        &widget(
            &[("primary", "d/grid.CSV"), ("labels", "l.txt")],
            &[("height", "170pt")],
        ),
        &m,
    )
    .unwrap();
    assert_eq!(
        b.sources,
        vec![
            (
                "data".to_string(),
                WidgetSource {
                    role: "data".into(),
                    path: "d/grid.CSV".into()
                }
            ),
            (
                "labels".to_string(),
                WidgetSource {
                    role: "labels".into(),
                    path: "l.txt".into()
                }
            ),
        ]
    );
    let e = |s: &[(&str, &str)]| bind(&widget(s, &[]), &m).unwrap_err();
    assert_eq!(
        e(&[("primary", "g.csv"), ("texture", "t.png")]),
        "widget fig-h: runtime heat@1 has no source role `texture`"
    );
    assert_eq!(
        e(&[
            ("primary", "g.csv"),
            ("labels", "a.txt"),
            ("labels", "b.txt")
        ]),
        "widget fig-h: source role `labels` is given twice"
    );
    assert_eq!(
        e(&[("primary", "g.csv"), ("data", "h.csv")]),
        "widget fig-h: source role `data` is given twice"
    );
    assert_eq!(
        e(&[("primary", "g.json")]),
        "widget fig-h: source data: `g.json` has extension `.json`; runtime heat@1 accepts .csv, .tsv"
    );
    assert_eq!(
        e(&[("primary", "noext")]),
        "widget fig-h: source data: `noext` has extension `.`; runtime heat@1 accepts .csv, .tsv"
    );
    assert_eq!(
        e(&[("labels", "a.txt")]),
        "widget fig-h: runtime heat@1 requires source data"
    );
    // Sizes are checked once the exporter has hashed the file.
    let w = widget(&[("primary", "g.csv")], &[]);
    assert!(check_size(&w, &m, "labels", 10).is_ok());
    assert_eq!(
        check_size(&w, &m, "labels", 11).unwrap_err(),
        "widget fig-h: source labels is 11 bytes; runtime heat@1 accepts at most 10"
    );
    assert!(check_size(&w, &m, "data", DEFAULT_MAX_BYTES).is_ok());
    assert!(check_size(&w, &m, "data", DEFAULT_MAX_BYTES + 1).is_err());
}

#[test]
fn bind_types_and_defaults_every_option() {
    let m = manifest();
    let opts = |o: &[(&str, &str)]| bind(&widget(&[("primary", "g.csv")], o), &m);
    let b = opts(&[
        ("height", "170pt"),
        ("width", "300pt"),
        ("rows", "7"),
        ("gain", "-1.25e0"),
        ("caption", "a=b"),
    ])
    .unwrap();
    assert_eq!(
        Value::Object(b.options),
        json!({"rows": 7, "gain": -1.25, "caption": "a=b", "scheme": "seq", "grid": false})
    );
    // Height and width are layout; keys without a default stay out.
    let b = opts(&[]).unwrap();
    assert_eq!(
        Value::Object(b.options),
        json!({"scheme": "seq", "rows": 3, "grid": false})
    );
    let e = |o: &[(&str, &str)]| opts(o).unwrap_err();
    assert_eq!(
        e(&[("color", "red")]),
        "widget fig-h: runtime heat@1 has no option `color`"
    );
    assert_eq!(
        e(&[("rows", "2"), ("rows", "3")]),
        "widget fig-h: option rows is given twice"
    );
    assert_eq!(
        e(&[("rows", "x")]),
        "widget fig-h: option rows=`x` is not an integer"
    );
    assert_eq!(
        e(&[("rows", "1.0")]),
        "widget fig-h: option rows=`1.0` is not an integer"
    );
    assert_eq!(
        e(&[("rows", "007")]),
        "widget fig-h: option rows=`007` is not an integer"
    );
    assert_eq!(
        e(&[("rows", "0")]),
        "widget fig-h: option rows=0 is below 1"
    );
    assert_eq!(
        e(&[("rows", "11")]),
        "widget fig-h: option rows=11 is above 10"
    );
    assert_eq!(
        e(&[("gain", "NaN")]),
        "widget fig-h: option gain=`NaN` is not a number"
    );
    assert_eq!(
        e(&[("gain", "1e999")]),
        "widget fig-h: option gain=`1e999` is not a number"
    );
    assert_eq!(
        e(&[("gain", "3")]),
        "widget fig-h: option gain=3 is above 2.5"
    );
    assert_eq!(
        e(&[("gain", "-2")]),
        "widget fig-h: option gain=-2 is below -1.5"
    );
    assert_eq!(
        e(&[("grid", "yes")]),
        "widget fig-h: option grid=`yes` is not a boolean"
    );
    assert_eq!(
        e(&[("scheme", "hot")]),
        "widget fig-h: option scheme=hot is not one of seq, div"
    );
    assert_eq!(
        e(&[("caption", "sixsix")]),
        "widget fig-h: option caption is longer than 5 characters"
    );
    // Values that would reach markup or style are refused by the grammar
    // (they never reach either: options travel as JSON only).
    for hostile in ["a{b", "}", "x|y", "#f00", "100%", "a\\b", "a\u{1}"] {
        assert_eq!(
            e(&[("caption", hostile)]),
            format!("widget fig-h: option caption=`{hostile}` is not a string")
        );
    }
    let b = opts(&[("grid", "true"), ("gain", "0")]).unwrap();
    assert_eq!(b.options["grid"], json!(true));
    assert_eq!(b.options["gain"], json!(0.0));
}

#[test]
fn a_runtime_without_options_takes_none() {
    let m = parse(&without(good(), &["options"])).unwrap();
    let w = widget(&[("primary", "g.csv")], &[("height", "1pt"), ("rows", "2")]);
    assert_eq!(
        bind(&w, &m).unwrap_err(),
        "widget fig-h: runtime heat@1 has no option `rows`"
    );
    let w = widget(&[("primary", "g.csv")], &[("height", "1pt")]);
    assert!(bind(&w, &m).unwrap().options.is_empty());
}
