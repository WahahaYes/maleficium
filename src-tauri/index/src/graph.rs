//! The document as the engine sees it from a main file: every file
//! reachable over `\input`/`\include`/`\subfile` edges, resolved against the
//! main file's directory (the directory the engine runs in).

use std::collections::{HashSet, VecDeque};

use maleficium_structure as ms;
use serde::Serialize;

use crate::ProjectIndex;

/// One input edge.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: String,
    pub line: u32,
    pub command: String,
    /// The argument as written.
    pub target: String,
    /// Root-relative file it resolves to; `None` when `external`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub external: bool,
}

/// Lexically join `target` onto root-relative `dir`; `None` when it is
/// absolute or climbs past the root.
pub fn join_rel(dir: &str, target: &str) -> Option<String> {
    if target.starts_with('/') || target.contains('\0') {
        return None;
    }
    let mut out: Vec<&str> = Vec::new();
    for part in dir.split('/').chain(target.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            s => out.push(s),
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join("/"))
    }
}

/// TeX's name rule: `\include` always adds `.tex`; `\input`/`\subfile` add it
/// when the name has no extension.
pub fn with_tex(command: &str, target: &str) -> String {
    let name = target.rsplit('/').next().unwrap_or(target);
    if command == "include" || !name.contains('.') {
        format!("{target}.tex")
    } else {
        target.to_string()
    }
}

/// Directory part of a root-relative path (`""` at the root).
pub fn dir_of(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// Where an input edge written in the document of `main_dir` points.
pub fn resolve_input(main_dir: &str, command: &str, target: &str) -> Option<String> {
    join_rel(main_dir, &with_tex(command, target))
}

/// The reachable files of one main file, in graph (breadth-first) order.
#[derive(Debug, Clone)]
pub struct Document {
    pub main: String,
    pub main_dir: String,
    /// Graph order. A file the index does not hold is still listed.
    pub files: Vec<String>,
    pub edges: Vec<GraphEdge>,
}

impl Document {
    pub fn walk(index: &ProjectIndex, main: &str) -> Document {
        let main_dir = dir_of(main).to_string();
        let mut doc = Document {
            main: main.to_string(),
            main_dir,
            files: Vec::new(),
            edges: Vec::new(),
        };
        let mut seen: HashSet<String> = HashSet::from([main.to_string()]);
        let mut queue = VecDeque::from([main.to_string()]);
        while let Some(rel) = queue.pop_front() {
            if let Some(s) = index.get(&rel).and_then(|f| f.symbols) {
                for input in &s.inputs {
                    let to = resolve_input(&doc.main_dir, &input.command, &input.target);
                    if let Some(to) = &to {
                        if seen.insert(to.clone()) {
                            queue.push_back(to.clone());
                        }
                    }
                    doc.edges.push(GraphEdge {
                        from: rel.clone(),
                        line: input.line,
                        command: input.command.clone(),
                        target: input.target.clone(),
                        external: to.is_none(),
                        to,
                    });
                }
            }
            doc.files.push(rel);
        }
        doc
    }

    /// `(rel, text)` of every reachable file the index holds text for.
    pub fn texts<'a>(
        &'a self,
        index: &'a ProjectIndex,
    ) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.files.iter().filter_map(|rel| {
            index
                .get(rel)
                .and_then(|f| f.text)
                .map(|t| (rel.as_str(), t))
        })
    }

    /// Content revision over the reachable texts, then `extra` in order.
    pub fn revision(&self, index: &ProjectIndex, extra: &[(&str, &str)]) -> String {
        ms::revision(self.texts(index).chain(extra.iter().copied()))
    }

    /// True when a reachable file's text came from a buffer.
    pub fn from_buffer(&self, index: &ProjectIndex) -> bool {
        self.files.iter().any(|r| {
            index
                .get(r)
                .is_some_and(|f| f.source == crate::Source::Buffer)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Disk;

    #[test]
    fn join_rel_stays_inside_the_root() {
        assert_eq!(
            join_rel("paper", "chapters/a.tex").as_deref(),
            Some("paper/chapters/a.tex")
        );
        assert_eq!(join_rel("paper", "../x.tex").as_deref(), Some("x.tex"));
        assert_eq!(join_rel("", "../x.tex"), None);
        assert_eq!(join_rel("", "/etc/passwd"), None);
    }

    #[test]
    fn walks_inputs_from_the_main_dir_in_graph_order() {
        let mut i = ProjectIndex::new();
        let put = |i: &mut ProjectIndex, r: &str, t: &str| i.set_disk(r, Disk::Text(t.into()));
        put(
            &mut i,
            "p/main.tex",
            "\\input{ch/a}\n\\include{ch/b}\n\\input{missing}\n\\input{../../x}",
        );
        put(&mut i, "p/ch/a.tex", "\\input{ch/a}\\input{ch/c}");
        put(&mut i, "p/ch/b.tex", "b");
        put(&mut i, "p/ch/c.tex", "c");
        let d = Document::walk(&i, "p/main.tex");
        assert_eq!(
            d.files,
            [
                "p/main.tex",
                "p/ch/a.tex",
                "p/ch/b.tex",
                "p/missing.tex",
                "p/ch/c.tex"
            ]
        );
        assert!(d.edges.iter().any(|e| e.external && e.target == "../../x"));
        assert_eq!(d.texts(&i).count(), 4);
        assert!(!d.from_buffer(&i));
        i.set_overlay("p/ch/b.tex", Some("edited".into()));
        assert!(d.from_buffer(&i));
    }
}
