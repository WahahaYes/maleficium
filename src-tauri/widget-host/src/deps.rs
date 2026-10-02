//! The widget crates pull in no Tauri: walked over the committed lockfile,
//! so the check covers every feature and target the lock resolves.

use std::collections::{BTreeMap, BTreeSet};

/// Package name to the names it depends on, from `Cargo.lock`.
fn graph(lock: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for block in lock.split("[[package]]").skip(1) {
        let mut name = None;
        let mut deps = Vec::new();
        let mut in_deps = false;
        for line in block.lines() {
            let t = line.trim();
            if let Some(n) = t.strip_prefix("name = \"") {
                name = Some(n.trim_end_matches('"').to_string());
            } else if t.starts_with("dependencies = [") {
                in_deps = true;
            } else if in_deps && t == "]" {
                in_deps = false;
            } else if in_deps {
                // `"name"`, `"name version"` or `"name version (source)"`.
                let d = t.trim_matches(|c| c == '"' || c == ',');
                if let Some(n) = d.split_whitespace().next() {
                    deps.push(n.trim_matches('"').to_string());
                }
            }
        }
        if let Some(n) = name {
            out.entry(n).or_default().extend(deps);
        }
    }
    out
}

fn closure(g: &BTreeMap<String, Vec<String>>, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(n) = stack.pop() {
        if seen.insert(n.clone()) {
            stack.extend(g.get(&n).into_iter().flatten().cloned());
        }
    }
    seen
}

fn coupled(name: &str) -> bool {
    name == "tauri" || name.starts_with("tauri-") || name == "tao" || name == "wry"
}

const LOCK: &str = include_str!("../../Cargo.lock");

fn untied(g: &BTreeMap<String, Vec<String>>, root: &str) {
    assert!(g.contains_key(root), "{root} is not in the lockfile");
    let bad: Vec<String> = closure(g, root)
        .into_iter()
        .filter(|n| coupled(n))
        .collect();
    assert!(bad.is_empty(), "{root} depends on {bad:?}");
}

#[test]
fn widget_crates_pull_in_no_tauri() {
    let g = graph(LOCK);
    untied(&g, "maleficium-widget-host");
    untied(&g, "maleficium-widget-helper");
}

#[test]
fn the_check_sees_tauri_in_the_app() {
    // Red control: the app crate does depend on tauri, so the walk works.
    let g = graph(LOCK);
    let app = closure(&g, "maleficium");
    assert!(app.contains("tauri") && app.contains("wry"));
}
