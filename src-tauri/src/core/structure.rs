//! Document structure of a project, addressed by session root and
//! root-relative paths, read from the project index (`core::index`): the
//! file graph from a main file, labels and references, citations, and
//! pre-compile checks across it.
//!
//! Every result is root-relative and carries `source` ("disk" for saved
//! files, "buffer" when an unsaved editor buffer was read) plus a `revision`
//! over the text it read.

use std::collections::{HashMap, HashSet};
use std::path::Path;

pub use maleficium_index::graph::GraphEdge;
use maleficium_index::graph::{join_rel, Document};
use maleficium_index::ProjectIndex;
use maleficium_structure as ms;
use serde::Serialize;

/// Rows per list; the rest are counted in `truncated`.
const MAX_ROWS: usize = 1000;

fn source_of(doc: &Document, index: &ProjectIndex) -> String {
    if doc.from_buffer(index) {
        "buffer"
    } else {
        "disk"
    }
    .to_string()
}

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

/// The canonical root-relative path of an existing main file.
fn main_of(root_id: &str, main_rel: &str) -> Result<String, String> {
    let root = super::fs::session_root(root_id)?;
    let abs = super::fs::resolve_in(root_id, main_rel)?;
    if !abs.is_file() {
        return Err(format!("not a file: {}", main_rel));
    }
    Ok(rel_of(&root, &abs))
}

/// Symbols of every reachable TeX file, in graph order.
fn doc_symbols<'a>(
    doc: &'a Document,
    index: &'a ProjectIndex,
) -> impl Iterator<Item = (&'a str, &'a ms::Symbols)> {
    doc.files.iter().filter_map(|rel| {
        index
            .get(rel)
            .and_then(|f| f.symbols)
            .map(|s| (rel.as_str(), s))
    })
}

/// Keep the first `MAX_ROWS`; return how many were dropped.
fn cap<T>(v: &mut Vec<T>) -> usize {
    let over = v.len().saturating_sub(MAX_ROWS);
    v.truncate(MAX_ROWS);
    over
}

/// Outline of one file.
pub fn outline_of(root_id: &str, rel: &str) -> Result<OutlineDoc, String> {
    let root = super::fs::session_root(root_id)?;
    let abs = super::fs::resolve_in(root_id, rel)?;
    let rel = rel_of(&root, &abs);
    super::index::with(root_id, |l| {
        let f = l
            .index
            .get(&rel)
            .ok_or_else(|| format!("not a file: {}", rel))?;
        let text = f.text.ok_or_else(|| format!("no text for {}", rel))?;
        let o = ms::outline(text);
        Ok(OutlineDoc {
            source: if f.source == maleficium_index::Source::Buffer {
                "buffer"
            } else {
                "disk"
            }
            .into(),
            revision: f.revision.to_string(),
            rel: rel.clone(),
            entries: o.entries,
            truncated: o.truncated,
        })
    })?
}

/// Files and input edges reachable from a main file.
pub fn file_graph(root_id: &str, main_rel: &str) -> Result<FileGraph, String> {
    let main = main_of(root_id, main_rel)?;
    super::index::with(root_id, |l| {
        let ix = &l.index;
        let doc = Document::walk(ix, &main);
        let mut edges = doc.edges.clone();
        let mut files: Vec<GraphFile> = doc
            .files
            .iter()
            .map(|rel| GraphFile {
                rel: rel.clone(),
                exists: ix.get(rel).is_some(),
            })
            .collect();
        let truncated = cap(&mut files) + cap(&mut edges);
        FileGraph {
            source: source_of(&doc, ix),
            revision: doc.revision(ix, &[]),
            main: doc.main.clone(),
            files,
            edges,
            truncated,
        }
    })
}

/// Label definitions and reference uses across the document.
pub fn labels_refs(root_id: &str, main_rel: &str) -> Result<LabelsRefs, String> {
    let main = main_of(root_id, main_rel)?;
    super::index::with(root_id, |l| {
        let ix = &l.index;
        let doc = Document::walk(ix, &main);
        let mut labels = Vec::new();
        let mut refs = Vec::new();
        for (rel, s) in doc_symbols(&doc, ix) {
            labels.extend(s.labels.iter().map(|k| LabelDef {
                key: k.key.clone(),
                rel: rel.to_string(),
                line: k.line,
                duplicate: false,
            }));
            refs.extend(s.refs.iter().map(|k| (k, rel)));
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
                key: k.key.clone(),
                rel: rel.to_string(),
                line: k.line,
            })
            .collect();
        for l in &mut labels {
            l.duplicate = dup.contains(&l.key);
        }
        let truncated = cap(&mut labels) + cap(&mut refs);
        LabelsRefs {
            source: source_of(&doc, ix),
            revision: doc.revision(ix, &[]),
            main: doc.main.clone(),
            labels,
            refs,
            truncated,
        }
    })
}

/// Citation uses across the document, checked against its bibliographies.
pub fn citations(root_id: &str, main_rel: &str) -> Result<Citations, String> {
    let main = main_of(root_id, main_rel)?;
    super::index::with(root_id, |l| {
        let ix = &l.index;
        let doc = Document::walk(ix, &main);
        let mut cites = Vec::new();
        let mut targets: Vec<String> = Vec::new();
        for (rel, s) in doc_symbols(&doc, ix) {
            cites.extend(s.cites.iter().map(|k| (k, rel)));
            for b in &s.bibliographies {
                if !targets.contains(&b.key) {
                    targets.push(b.key.clone());
                }
            }
        }
        let mut keys: HashSet<&str> = HashSet::new();
        let mut bib_texts: Vec<(&str, &str)> = Vec::new();
        let bib_files: Vec<BibFile> = targets
            .into_iter()
            .map(|target| {
                let rel = join_rel(&doc.main_dir, &target);
                let f = rel
                    .as_deref()
                    .and_then(|r| ix.get(r))
                    .filter(|f| f.text.is_some());
                let n = f.map(|f| f.bib.len()).unwrap_or(0);
                if let Some(f) = f {
                    keys.extend(f.bib.iter().map(|k| k.key.as_str()));
                    bib_texts.push((f.rel, f.text.unwrap_or("")));
                }
                BibFile {
                    target,
                    external: rel.is_none(),
                    exists: f.is_some(),
                    rel,
                    entries: n,
                }
            })
            .collect();
        let mut cites: Vec<KeyUse> = cites
            .into_iter()
            .map(|(k, rel)| KeyUse {
                resolved: keys.contains(k.key.as_str()),
                key: k.key.clone(),
                rel: rel.to_string(),
                line: k.line,
            })
            .collect();
        let truncated = cap(&mut cites);
        Citations {
            source: source_of(&doc, ix),
            revision: doc.revision(ix, &bib_texts),
            main: doc.main.clone(),
            bib_files,
            cites,
            truncated,
        }
    })
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Precheck {
    pub source: String,
    pub revision: String,
    pub main: String,
    pub findings: Vec<ms::Finding>,
    /// Packages were checked against the cached bundle index (false until
    /// the first compile has cached it).
    pub bundle_checked: bool,
    /// Fonts were checked against this machine's font database.
    pub fonts_checked: bool,
}

/// Installed font families, lowercased, from fontconfig; `None` where
/// fontconfig is absent.
fn font_families() -> Option<HashSet<String>> {
    let out = super::quiet_command("fc-list")
        .args([":", "family"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .flat_map(|l| l.split(','))
            .map(|f| f.trim().to_lowercase())
            .filter(|f| !f.is_empty())
            .collect(),
    )
}

/// Dependency checks over the whole document before compiling: every
/// package or class neither the project nor the bundle provides, biblatex
/// needing biber, shell escape, and fontspec fonts this machine lacks.
pub fn precompile_checks(root_id: &str, main_rel: &str) -> Result<Precheck, String> {
    let main = main_of(root_id, main_rel)?;
    let bundle = super::engine::bundle_files(&super::engine::cache_dir());
    super::index::with(root_id, |l| {
        let ix = &l.index;
        let doc = Document::walk(ix, &main);
        let files: Vec<(String, &ms::Symbols)> = doc_symbols(&doc, ix)
            .map(|(r, s)| (r.to_string(), s))
            .collect();
        let in_bundle = |f: &str| bundle.as_ref().is_some_and(|b| b.contains(f));
        let in_project = |f: &str| {
            !f.contains('/') && join_rel(&doc.main_dir, f).is_some_and(|r| ix.get(&r).is_some())
        };
        let wants_fonts = files.iter().any(|(_, s)| !s.fonts.is_empty());
        let families = if wants_fonts { font_families() } else { None };
        let font_installed = |f: &str| {
            families
                .as_ref()
                .is_some_and(|fs| fs.contains(&f.to_lowercase()))
        };
        let env = ms::CheckEnv {
            in_bundle: bundle
                .is_some()
                .then_some(&in_bundle as &dyn Fn(&str) -> bool),
            in_project: &in_project,
            font_installed: families
                .is_some()
                .then_some(&font_installed as &dyn Fn(&str) -> bool),
        };
        let findings = ms::precompile_checks(&files, &env);
        Precheck {
            source: source_of(&doc, ix),
            revision: doc.revision(ix, &[]),
            main: doc.main.clone(),
            findings,
            bundle_checked: bundle.is_some(),
            fonts_checked: !wants_fonts || families.is_some(),
        }
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
        source: "disk".into(),
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
        .map(ms::offline_reading)
        .or(reason.map(|reason| ms::MissingDependency { file: None, reason }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project(name: &str, files: &[(&str, &str)]) -> (String, PathBuf) {
        let dir = crate::test_scratch::dir(&format!("st-{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        for (rel, text) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let canon = dunce::canonicalize(&dir).unwrap();
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
    fn buffers_laid_over_the_index_are_read_and_named() {
        let (id, _) = sample("overlay");
        assert_eq!(labels_refs(&id, "paper/main.tex").unwrap().source, "disk");
        super::super::index::overlay(&id, "paper/chapters/b.tex", Some("\\label{fresh}".into()))
            .unwrap();
        let lr = labels_refs(&id, "paper/main.tex").unwrap();
        assert_eq!(lr.source, "buffer");
        assert!(lr.labels.iter().any(|l| l.key == "fresh"));
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
