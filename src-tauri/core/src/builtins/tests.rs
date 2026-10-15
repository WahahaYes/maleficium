use super::*;
use crate::runtimes::{OptionFormat, PosterStrategy};
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
fn manifests_describe_what_the_exporter_does_today() {
    let want = [
        ("model", "model", PosterStrategy::Render),
        ("video", "video", PosterStrategy::Author),
        ("table", "data", PosterStrategy::Crop),
        ("chart", "spec", PosterStrategy::Render),
    ];
    for (name, primary, poster) in want {
        let m = &by_name(name).unwrap().manifest;
        assert_eq!(m.primary_role(), primary, "{name}");
        assert_eq!(m.poster, poster, "{name}");
    }
    assert!(by_name("chart").unwrap().manifest.sources["data"].repeat);
    assert!(by_name("video").unwrap().manifest.sources["video"].remote);
    assert!(by_name("model").unwrap().manifest.capabilities.webgl);
}

#[test]
fn option_formats_match_the_poster_parameters() {
    let sample = |f: OptionFormat| match f {
        OptionFormat::Camera => "1 0 0 0 0 1 0 0 0 0 1 0 0 0 5 1",
        OptionFormat::Size => "800x600",
        OptionFormat::Background => "ffffff",
        OptionFormat::Scale => "2",
    };
    let params = ["camera", "size", "background", "scale"];
    for b in all() {
        let props = b.manifest.options.as_ref().map(|o| &o.properties);
        for key in params {
            let declared = props.and_then(|p| p.get(key)).and_then(|o| o.format);
            let applies = crate::widgets::params::canonical(WidgetType::Builtin(b), key, "x")
                .is_err_and(|e| !e.contains("does not apply"));
            assert_eq!(declared.is_some(), applies, "{}: {key}", b.reference);
            if let Some(f) = declared {
                crate::widgets::params::canonical(WidgetType::Builtin(b), key, sample(f))
                    .unwrap_or_else(|e| panic!("{}: {e}", b.reference));
            }
        }
    }
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
    let reader = quoted(capture(READER, r"var KINDS = \[([^\]]*)\]"));
    assert_eq!(reader, kinds(), "embed-runtime/reader/reader.js KINDS");

    let mount: BTreeSet<String> = crate::reflow::sanitize::MOUNT_KINDS
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(mount, kinds(), "reflow::sanitize::MOUNT_KINDS");

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
    assert_eq!(rhai, kinds(), ".sty.rhai widget constructors");

    let vite = quoted(capture(VITE, r"export const RUNTIMES = \[([^\]]*)\]"));
    assert_eq!(vite, names(), "embed-runtime/vite.config.ts RUNTIMES");

    let schema: serde_json::Value = serde_json::from_str(crate::bundle::MANIFEST_SCHEMA).unwrap();
    let types: BTreeSet<String> = schema["$defs"]["widget"]["properties"]["type"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(types, kinds(), "bundle schema widget types");
}
