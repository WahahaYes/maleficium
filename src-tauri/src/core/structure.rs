//! Document structure of a project on disk, addressed by session root and
//! root-relative paths. Parsing is `maleficium_structure` (pure, shared with
//! the desktop buffer path); this module owns the file graph: walking
//! `\input`/`\include` from a main file inside the granted root.
//!
//! Every result is root-relative and carries `source` ("disk": saved files,
//! never unsaved buffers) plus a `revision` over the bytes it read.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use maleficium_structure as ms;
use serde::Serialize;

/// Files one graph walk will read.
const MAX_FILES: usize = 500;
/// Rows per list; the rest are counted in `truncated`.
const MAX_ROWS: usize = 1000;
const SOURCE: &str = "disk";

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutlineDoc {
    pub source: String,
    pub revision: String,
    pub rel: String,
    pub entries: Vec<ms::OutlineEntry>,
    pub truncated: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GraphFile {
    pub rel: String,
    pub exists: bool,
}

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

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileGraph {
    pub source: String,
    pub revision: String,
    pub main: String,
    pub files: Vec<GraphFile>,
    pub edges: Vec<GraphEdge>,
    pub truncated: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelDef {
    pub key: String,
    pub rel: String,
    pub line: u32,
    /// Another definition of the same key exists in the document.
    pub duplicate: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeyUse {
    pub key: String,
    pub rel: String,
    pub line: u32,
    pub resolved: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelsRefs {
    pub source: String,
    pub revision: String,
    pub main: String,
    pub labels: Vec<LabelDef>,
    pub refs: Vec<KeyUse>,
    pub truncated: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BibFile {
    /// The resource as written (`.bib` added for `\bibliography`).
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,
    pub exists: bool,
    pub external: bool,
    pub entries: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Citations {
    pub source: String,
    pub revision: String,
    pub main: String,
    pub bib_files: Vec<BibFile>,
    pub cites: Vec<KeyUse>,
    pub truncated: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub source: String,
    pub revision: String,
    pub main: String,
    pub diagnostics: Vec<ms::Diagnostic>,
    pub truncated: usize,
    /// The dependency the last compile lacked, and why.
    pub missing: Option<ms::MissingDependency>,
}

/// Root-relative `/`-string of a path already inside `root`.
fn rel_of(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Lexically join `target` onto root-relative `dir`; `None` when it is
/// absolute or climbs past the root.
fn join_rel(dir: &str, target: &str) -> Option<String> {
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
fn with_tex(command: &str, target: &str) -> String {
    let name = target.rsplit('/').next().unwrap_or(target);
    if command == "include" || !name.contains('.') {
        format!("{target}.tex")
    } else {
        target.to_string()
    }
}

/// Text of a root-relative file; `None` when it cannot be read
/// (missing, escaping via symlink, not UTF-8).
fn read_rel(root_id: &str, rel: &str) -> Option<String> {
    let abs = super::fs::resolve_in(root_id, rel).ok()?;
    std::fs::read_to_string(abs).ok()
}

/// The document as the engine sees it from `main_rel`: every file reachable
/// over input edges, resolved against the main file's directory (the
/// directory the engine runs in).
struct Document {
    main: String,
    main_dir: String,
    /// Graph order; `None` text = unreadable.
    files: Vec<(String, Option<String>)>,
    edges: Vec<GraphEdge>,
    truncated: usize,
}

impl Document {
    fn load(root_id: &str, main_rel: &str) -> Result<Self, String> {
        let root = super::fs::session_root(root_id)?;
        let main_abs = super::fs::resolve_in(root_id, main_rel)?;
        if !main_abs.is_file() {
            return Err(format!("not a file: {}", main_rel));
        }
        let main = rel_of(&root, &main_abs);
        let main_dir = main
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        let mut doc = Document {
            main: main.clone(),
            main_dir,
            files: Vec::new(),
            edges: Vec::new(),
            truncated: 0,
        };
        let mut seen: HashSet<String> = HashSet::from([main.clone()]);
        let mut queue = VecDeque::from([main]);
        while let Some(rel) = queue.pop_front() {
            if doc.files.len() == MAX_FILES {
                doc.truncated += 1;
                continue;
            }
            let text = read_rel(root_id, &rel);
            if let Some(t) = &text {
                for input in ms::symbols(t).inputs {
                    let to = join_rel(&doc.main_dir, &with_tex(&input.command, &input.target));
                    if let Some(to) = &to {
                        if seen.insert(to.clone()) {
                            queue.push_back(to.clone());
                        }
                    }
                    doc.edges.push(GraphEdge {
                        from: rel.clone(),
                        line: input.line,
                        command: input.command,
                        target: input.target,
                        external: to.is_none(),
                        to,
                    });
                }
            }
            doc.files.push((rel, text));
        }
        Ok(doc)
    }

    fn texts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files
            .iter()
            .filter_map(|(rel, t)| t.as_deref().map(|t| (rel.as_str(), t)))
    }

    fn revision<'a>(&'a self, extra: &'a [(String, String)]) -> String {
        ms::revision(
            self.texts()
                .chain(extra.iter().map(|(r, t)| (r.as_str(), t.as_str()))),
        )
    }
}

/// Keep the first `MAX_ROWS`; return how many were dropped.
fn cap<T>(v: &mut Vec<T>) -> usize {
    let over = v.len().saturating_sub(MAX_ROWS);
    v.truncate(MAX_ROWS);
    over
}

/// Outline of one saved file.
pub fn outline_of(root_id: &str, rel: &str) -> Result<OutlineDoc, String> {
    let root = super::fs::session_root(root_id)?;
    let abs = super::fs::resolve_in(root_id, rel)?;
    let text = std::fs::read_to_string(&abs).map_err(|e| format!("read failed: {}", e))?;
    let rel = rel_of(&root, &abs);
    let o = ms::outline(&text);
    Ok(OutlineDoc {
        source: SOURCE.into(),
        revision: ms::revision([(rel.as_str(), text.as_str())]),
        rel,
        entries: o.entries,
        truncated: o.truncated,
    })
}

/// Files and input edges reachable from a main file.
pub fn file_graph(root_id: &str, main_rel: &str) -> Result<FileGraph, String> {
    let doc = Document::load(root_id, main_rel)?;
    let mut edges = doc.edges.clone();
    let mut files: Vec<GraphFile> = doc
        .files
        .iter()
        .map(|(rel, t)| GraphFile {
            rel: rel.clone(),
            exists: t.is_some(),
        })
        .collect();
    let truncated = doc.truncated + cap(&mut files) + cap(&mut edges);
    Ok(FileGraph {
        source: SOURCE.into(),
        revision: doc.revision(&[]),
        main: doc.main,
        files,
        edges,
        truncated,
    })
}

/// Label definitions and reference uses across the document.
pub fn labels_refs(root_id: &str, main_rel: &str) -> Result<LabelsRefs, String> {
    let doc = Document::load(root_id, main_rel)?;
    let mut labels = Vec::new();
    let mut refs = Vec::new();
    for (rel, text) in doc.texts() {
        let s = ms::symbols(text);
        labels.extend(s.labels.into_iter().map(|k| LabelDef {
            key: k.key,
            rel: rel.to_string(),
            line: k.line,
            duplicate: false,
        }));
        refs.extend(s.refs.into_iter().map(|k| (k, rel.to_string())));
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for l in &labels {
        *counts.entry(l.key.as_str()).or_default() += 1;
    }
    let dup: HashSet<String> = counts
        .into_iter()
        .filter(|&(_, n)| n > 1)
        .map(|(k, _)| k.to_string())
        .collect();
    let defined: HashSet<&str> = labels.iter().map(|l| l.key.as_str()).collect();
    let mut refs: Vec<KeyUse> = refs
        .into_iter()
        .map(|(k, rel)| KeyUse {
            resolved: defined.contains(k.key.as_str()),
            key: k.key,
            rel,
            line: k.line,
        })
        .collect();
    for l in &mut labels {
        l.duplicate = dup.contains(&l.key);
    }
    let truncated = doc.truncated + cap(&mut labels) + cap(&mut refs);
    Ok(LabelsRefs {
        source: SOURCE.into(),
        revision: doc.revision(&[]),
        main: doc.main,
        labels,
        refs,
        truncated,
    })
}

/// Citation uses across the document, checked against its bibliographies.
pub fn citations(root_id: &str, main_rel: &str) -> Result<Citations, String> {
    let doc = Document::load(root_id, main_rel)?;
    let mut cites = Vec::new();
    let mut targets: Vec<String> = Vec::new();
    for (rel, text) in doc.texts() {
        let s = ms::symbols(text);
        cites.extend(s.cites.into_iter().map(|k| (k, rel.to_string())));
        for b in s.bibliographies {
            if !targets.contains(&b.key) {
                targets.push(b.key);
            }
        }
    }
    let mut keys: HashSet<String> = HashSet::new();
    let mut bib_texts: Vec<(String, String)> = Vec::new();
    let bib_files: Vec<BibFile> = targets
        .into_iter()
        .map(|target| {
            let rel = join_rel(&doc.main_dir, &target);
            let text = rel.as_deref().and_then(|r| read_rel(root_id, r));
            let entries = text.as_deref().map(ms::bib_keys).unwrap_or_default();
            let n = entries.len();
            keys.extend(entries.into_iter().map(|k| k.key));
            if let (Some(r), Some(t)) = (&rel, text.clone()) {
                bib_texts.push((r.clone(), t));
            }
            BibFile {
                target,
                external: rel.is_none(),
                exists: text.is_some(),
                rel,
                entries: n,
            }
        })
        .collect();
    let mut cites: Vec<KeyUse> = cites
        .into_iter()
        .map(|(k, rel)| KeyUse {
            resolved: keys.contains(&k.key),
            key: k.key,
            rel,
            line: k.line,
        })
        .collect();
    let truncated = doc.truncated + cap(&mut cites);
    Ok(Citations {
        source: SOURCE.into(),
        revision: doc.revision(&bib_texts),
        main: doc.main,
        bib_files,
        cites,
        truncated,
    })
}

/// Structured diagnostics from the last compile's engine log.
pub fn diagnostics(root_id: &str, main_rel: &str, max: usize) -> Result<Diagnostics, String> {
    let root = super::fs::session_root(root_id)?;
    let o = super::outputs_of(root_id, main_rel)?;
    let log = super::engine_log(root_id, main_rel)?;
    let main = rel_of(&root, &o.dir.join(&o.main_file));
    let mut diagnostics = ms::diagnostics(&log, &root.to_string_lossy(), &o.dir.to_string_lossy());
    let over = diagnostics.len().saturating_sub(max);
    diagnostics.truncate(max);
    Ok(Diagnostics {
        source: SOURCE.into(),
        revision: ms::revision([("log", log.as_str())]),
        main,
        diagnostics,
        truncated: over,
        missing: missing_of(&log),
    })
}

/// What the compile that wrote `log` lacked: read from its console lines,
/// else from the cache (an empty log with nothing cached means the compile
/// stopped before spawning; a drifted digest distrusts a clean run).
fn missing_of(log: &str) -> Option<ms::MissingDependency> {
    use super::engine::{self, DigestCheck};
    let cache = engine::cache_dir();
    let lines: Vec<&str> = log.lines().collect();
    let failed = lines.iter().any(|l| l.starts_with("error:"));
    let in_bundle = |f: &str| engine::bundle_files(&cache).is_none_or(|n| n.contains(f));
    let reason = match engine::check_digest(&cache) {
        DigestCheck::Unresolved if lines.is_empty() => Some(ms::MissingReason::CacheEmpty),
        DigestCheck::Changed(_) => Some(ms::MissingReason::BundleChanged),
        _ => None,
    };
    ms::missing_dependency(&lines, !failed, &in_bundle)
        .or(reason.map(|reason| ms::MissingDependency { file: None, reason }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project(name: &str, files: &[(&str, &str)]) -> (String, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("maleficium-st-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        for (rel, text) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let canon = dir.canonicalize().unwrap();
        let id = format!("st-{}", name);
        super::super::grant_root(&id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    const MAIN: &str = "\\documentclass{article}\n\\begin{document}\n\\section{Intro}\\label{sec:intro}\n\\input{chapters/a}\n\\include{chapters/b}\n\\input{../escape}\n\\input{missing}\nSee \\ref{sec:a} \\ref{nope} \\cite{knuth, ghost}\n\\bibliography{refs}\n\\end{document}\n";

    fn sample(name: &str) -> (String, PathBuf) {
        project(
            name,
            &[
                ("paper/main.tex", MAIN),
                (
                    "paper/chapters/a.tex",
                    "\\section{A}\\label{sec:a}\\label{sec:intro}\n\\input{chapters/a}\n",
                ),
                ("paper/chapters/b.tex", "\\cite{lamport}\n"),
                ("paper/refs.bib", "@book{knuth,\n}\n@book{lamport,\n}\n"),
            ],
        )
    }

    #[test]
    fn join_rel_stays_inside_the_root() {
        assert_eq!(
            join_rel("paper", "chapters/a.tex").as_deref(),
            Some("paper/chapters/a.tex")
        );
        assert_eq!(join_rel("paper", "../x.tex").as_deref(), Some("x.tex"));
        assert_eq!(join_rel("", "../x.tex"), None);
        assert_eq!(join_rel("paper", "/etc/passwd"), None);
        assert_eq!(with_tex("input", "a"), "a.tex");
        assert_eq!(with_tex("input", "a.sty"), "a.sty");
        assert_eq!(with_tex("include", "ch.1"), "ch.1.tex");
    }

    #[test]
    fn graph_walks_inputs_from_the_main_dir() {
        let (id, _) = sample("graph");
        let g = file_graph(&id, "paper/main.tex").unwrap();
        assert_eq!(g.source, "disk");
        assert_eq!(g.main, "paper/main.tex");
        let files: Vec<(&str, bool)> = g.files.iter().map(|f| (f.rel.as_str(), f.exists)).collect();
        assert_eq!(
            files,
            vec![
                ("paper/main.tex", true),
                ("paper/chapters/a.tex", true),
                ("paper/chapters/b.tex", true),
                ("escape.tex", false),
                ("paper/missing.tex", false),
            ]
        );
        // The self-input is an edge but not a second visit.
        assert_eq!(
            g.edges
                .iter()
                .filter(|e| e.from == "paper/chapters/a.tex")
                .count(),
            1
        );
        let json = serde_json::to_string(&g).unwrap();
        assert!(
            !json.contains(&std::env::temp_dir().to_string_lossy().to_string()),
            "{json}"
        );
    }

    #[test]
    fn labels_refs_resolve_across_files() {
        let (id, _) = sample("labels");
        let lr = labels_refs(&id, "paper/main.tex").unwrap();
        let dup: Vec<(&str, &str, bool)> = lr
            .labels
            .iter()
            .map(|l| (l.key.as_str(), l.rel.as_str(), l.duplicate))
            .collect();
        assert_eq!(
            dup,
            vec![
                ("sec:intro", "paper/main.tex", true),
                ("sec:a", "paper/chapters/a.tex", false),
                ("sec:intro", "paper/chapters/a.tex", true),
            ]
        );
        let refs: Vec<(&str, bool)> = lr
            .refs
            .iter()
            .map(|r| (r.key.as_str(), r.resolved))
            .collect();
        assert_eq!(refs, vec![("sec:a", true), ("nope", false)]);
    }

    #[test]
    fn citations_check_the_bibliography() {
        let (id, _) = sample("cites");
        let c = citations(&id, "paper/main.tex").unwrap();
        assert_eq!(c.bib_files.len(), 1);
        assert_eq!(c.bib_files[0].rel.as_deref(), Some("paper/refs.bib"));
        assert_eq!(c.bib_files[0].entries, 2);
        let cites: Vec<(&str, &str, bool)> = c
            .cites
            .iter()
            .map(|k| (k.key.as_str(), k.rel.as_str(), k.resolved))
            .collect();
        assert_eq!(
            cites,
            vec![
                ("knuth", "paper/main.tex", true),
                ("ghost", "paper/main.tex", false),
                ("lamport", "paper/chapters/b.tex", true),
            ]
        );
    }

    #[test]
    fn revision_tracks_content() {
        let (id, root) = sample("rev");
        let a = labels_refs(&id, "paper/main.tex").unwrap().revision;
        assert_eq!(a, labels_refs(&id, "paper/main.tex").unwrap().revision);
        std::fs::write(root.join("paper/chapters/b.tex"), "changed\n").unwrap();
        assert_ne!(a, labels_refs(&id, "paper/main.tex").unwrap().revision);
    }

    #[test]
    fn outline_and_escapes() {
        let (id, _) = sample("outline");
        let o = outline_of(&id, "paper/chapters/a.tex").unwrap();
        assert_eq!(o.rel, "paper/chapters/a.tex");
        assert_eq!(o.entries[0].title, "A");
        assert!(outline_of(&id, "../x.tex").is_err());
        assert!(file_graph(&id, "paper").is_err());
        assert!(file_graph("nope", "paper/main.tex").is_err());
    }

    #[test]
    fn diagnostics_rebase_the_engine_log() {
        let (id, root) = sample("diag");
        let o = super::super::outputs_of(&id, "paper/main.tex").unwrap();
        std::fs::create_dir_all(&o.outdir).unwrap();
        let log = format!(
            "error: main.tex:3: Undefined control sequence\n{}/paper/chapters/a.tex:1: x\n/usr/share/x.tex:2: y\n",
            root.to_string_lossy()
        );
        std::fs::write(o.outdir.join("main.log"), log).unwrap();
        let d = diagnostics(&id, "paper/main.tex", 2).unwrap();
        let _ = std::fs::remove_dir_all(&o.outdir);
        let got: Vec<(Option<&str>, u32, bool)> = d
            .diagnostics
            .iter()
            .map(|x| (x.path.as_deref(), x.line, x.external))
            .collect();
        assert_eq!(
            got,
            vec![
                (Some("paper/main.tex"), 3, false),
                (Some("paper/chapters/a.tex"), 1, false)
            ]
        );
        assert_eq!(d.truncated, 1);
    }
}
