use super::*;
use crate::widgets::WidgetType;
use regex::Regex;
use std::collections::BTreeSet;

const READER: &str = include_str!("../../../../embed-runtime/reader/reader.js");
const STY: &str = include_str!("../../../../embed-runtime/tex/maleficium-interactive.sty");
const RHAI: &str = include_str!("../../../engine/embedded/maleficium-interactive.sty.rhai");
const VITE: &str = include_str!("../../../../embed-runtime/vite.config.ts");
const LOCK: &str = include_str!("../../../../package-lock.json");

fn names() -> BTreeSet<String> {
    all().iter().map(|b| b.manifest.name.clone()).collect()
}

/// The built-in names plus the two kinds no runtime package backs.
fn kinds() -> BTreeSet<String> {
    let mut k = names();
    k.extend(["html".to_string(), "custom".to_string()]);
    k
}

fn quoted(list: &str) -> BTreeSet<String> {
    Regex::new(r"'([a-z]+)'|\x22([a-z]+)\x22")
        .unwrap()
        .captures_iter(list)
        .map(|c| c.get(1).or(c.get(2)).unwrap().as_str().to_string())
        .collect()
}

fn capture<'a>(text: &'a str, pattern: &str) -> &'a str {
    Regex::new(pattern)
        .unwrap()
        .captures(text)
        .unwrap_or_else(|| panic!("no match for {pattern}"))
        .get(1)
        .unwrap()
        .as_str()
}

#[test]
fn every_embedded_manifest_parses_and_names_its_kind() {
    assert_eq!(all().len(), EMBEDDED.len());
    for b in all() {
        let name = &b.manifest.name;
        assert_eq!(b.reference, format!("{name}@1"));
        assert!(b.manifest.version.starts_with("1."), "{name}");
        let kind = WidgetType::Builtin(b);
        assert_eq!(serde_json::to_value(kind).unwrap(), name.as_str());
        assert_eq!(WidgetType::parse(name), Some(kind));
        assert_eq!(kind.name(), name);
        assert!(b.host.contains("<html"), "{name}: host is not a document");
        assert!(by_ref(b.reference).is_some_and(|x| std::ptr::eq(x, b)));
    }
    assert!(by_ref("plotly@1").is_none());
}

#[test]
fn vendored_versions_match_the_lockfile() {
    let lock: serde_json::Value = serde_json::from_str(LOCK).unwrap();
    for b in all() {
        for v in &b.manifest.vendored {
            let pkg = &lock["packages"][format!("node_modules/{}", v.name)];
            assert_eq!(
                pkg["version"],
                v.version.as_str(),
                "{}: {}",
                b.reference,
                v.name
            );
            assert_eq!(
                pkg["license"],
                v.license.as_str(),
                "{}: {}",
                b.reference,
                v.name
            );
        }
    }
}

#[test]
fn reserved_names_are_the_registry_and_its_two_kinds() {
    for k in kinds() {
        assert!(crate::runtimes::reserved_name(&k), "{k}");
        assert!(!crate::runtimes::valid_name(&k), "{k}");
    }
    assert!(crate::runtimes::valid_name("plotly"));
}

#[test]
fn every_kind_list_matches_the_registry() {
    // The reader mounts what a manifest's runtimes lists: no kind list.
    assert!(
        !READER.contains("KINDS"),
        "embed-runtime/reader/reader.js keeps a kind list"
    );

    let sty: BTreeSet<String> = capture(STY, r"\\A\(\?:([a-z|]+)\)\\Z")
        .split('|')
        .map(str::to_string)
        .collect();
    assert_eq!(sty, kinds(), ".sty reserved runtime names");

    let rhai: BTreeSet<String> = Regex::new(r"m-widget m-widget-([a-z]+)")
        .unwrap()
        .captures_iter(RHAI)
        .map(|c| c[1].to_string())
        .collect();
    // Built-ins go through \interactiveruntime: no constructor per kind.
    let macros: BTreeSet<String> = ["html", "custom"].map(String::from).into();
    assert_eq!(rhai, macros, ".sty.rhai widget constructors");

    let vite = quoted(capture(VITE, r"export const RUNTIMES = \[([^\]]*)\]"));
    assert_eq!(vite, names(), "embed-runtime/vite.config.ts RUNTIMES");

    // Format 2 leaves the widget type open: a pattern every kind fits.
    let schema: serde_json::Value = serde_json::from_str(crate::bundle::MANIFEST_SCHEMA).unwrap();
    let ty = &schema["$defs"]["widget"]["properties"]["type"];
    assert!(
        ty.get("enum").is_none(),
        "bundle schema widget type is open"
    );
    let pattern = regex::Regex::new(ty["pattern"].as_str().unwrap()).unwrap();
    for k in kinds() {
        assert!(pattern.is_match(&k), "bundle schema widget type admits {k}");
    }
}

#[test]
fn each_macro_passes_through_the_options_its_runtime_declares() {
    // \mfw@bi@options{<keys>}{<kind>}: every option the runtime.json declares
    // without a format or the pdf flag (those have keys of their own).
    let sty: std::collections::BTreeMap<String, BTreeSet<String>> =
        Regex::new(r"\\mfw@bi@options\{([a-z,]*)\}\{([a-z]+)\}")
            .unwrap()
            .captures_iter(STY)
            .map(|c| {
                let keys = c[1]
                    .split(',')
                    .filter(|k| !k.is_empty())
                    .map(str::to_string)
                    .collect();
                (c[2].to_string(), keys)
            })
            .collect();
    for b in all() {
        let want: BTreeSet<String> = b
            .manifest
            .options
            .iter()
            .flat_map(|o| &o.properties)
            .filter(|(_, o)| o.format.is_none() && !o.pdf)
            .map(|(k, _)| k.clone())
            .collect();
        assert_eq!(
            sty.get(&b.manifest.name),
            Some(&want),
            "runtime={}@1 passes through",
            b.manifest.name
        );
    }
}
