//! The project index: every file under a project root, the text of each
//! text file (from disk, or from an unsaved editor buffer laid over it), the
//! symbols of each TeX file, and the project-wide maps built from them
//! (label, bib entry and macro definitions).
//!
//! The one index every project feature reads: search, replace, the file
//! finder, go-to-definition, the structure tools, completion and
//! diagnostics. Pure by contract: callers feed it paths and text, it never
//! touches the fs. Paths are root-relative with `/` separators. The shape is
//! documented in `notes/search-index/INDEX-SHAPE.md`.

pub mod graph;
pub mod replace;
pub mod search;

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use maleficium_structure::{self as ms, KeyAt, MacroAt, Symbols};
use serde::Serialize;
use ts_rs::TS;

/// Where a file's indexed text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Disk,
    /// An unsaved editor buffer laid over the file.
    Buffer,
}

/// Why a listed file has no indexed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Unindexed {
    /// Not a text file by extension.
    NotText,
    /// Larger than the caller's per-file cap.
    TooLarge,
    /// A text extension whose bytes are not UTF-8.
    NotUtf8,
    /// The project's total text budget was spent before this file.
    OverBudget,
}

/// What the caller found on disk for one path.
#[derive(Debug, Clone, PartialEq)]
pub enum Disk {
    Text(String),
    Listed { bytes: u64, reason: Unindexed },
}

/// A place in the project: root-relative file and 1-based line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
pub struct Loc {
    pub rel: String,
    pub line: u32,
}

/// One macro definition and where it is.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
pub struct MacroDef {
    pub rel: String,
    pub line: u32,
    pub command: String,
    pub params: Option<u8>,
    pub body: String,
}

/// Project-wide definition maps, keyed as written: label keys, bib entry
/// keys, macro names with their backslash. Each key lists every definition
/// in path order, so a duplicate shows as a second entry.
#[derive(Debug, Default)]
pub struct Maps {
    pub labels: HashMap<String, Vec<Loc>>,
    pub bib_entries: HashMap<String, Vec<Loc>>,
    pub macros: HashMap<String, Vec<MacroDef>>,
}

/// Extensions whose text is parsed for symbols.
const TEX_EXTENSIONS: &[&str] = &[
    ".tex", ".sty", ".cls", ".ltx", ".dtx", ".def", ".clo", ".bbx", ".cbx", ".lbx", ".tikz", ".pgf",
];

/// True for a file parsed for TeX symbols.
pub fn is_tex_path(rel: &str) -> bool {
    TEX_EXTENSIONS.contains(&ms::ext_of(rel).as_str())
}

/// True for a bibliography database.
pub fn is_bib_path(rel: &str) -> bool {
    ms::ext_of(rel) == ".bib"
}

#[derive(Debug)]
struct Derived {
    revision: String,
    symbols: Option<Symbols>,
    bib: Vec<KeyAt>,
}

#[derive(Debug)]
struct Record {
    disk: Option<Disk>,
    overlay: Option<String>,
    derived: Derived,
}

impl Record {
    fn text(&self) -> Option<(&str, Source)> {
        match (&self.overlay, &self.disk) {
            (Some(t), _) => Some((t, Source::Buffer)),
            (None, Some(Disk::Text(t))) => Some((t, Source::Disk)),
            _ => None,
        }
    }
}

fn derive(rel: &str, text: Option<&str>) -> Derived {
    let revision = ms::revision([(rel, text.unwrap_or(""))]);
    let symbols = text.filter(|_| is_tex_path(rel)).map(ms::symbols);
    let bib = text
        .filter(|_| is_bib_path(rel))
        .map(ms::bib_keys)
        .unwrap_or_default();
    Derived {
        revision,
        symbols,
        bib,
    }
}

/// One indexed file as readers see it: the buffer text when one is laid
/// over the file, else the disk text.
#[derive(Debug, Clone, Copy)]
pub struct FileView<'a> {
    pub rel: &'a str,
    /// `None` for a listed file with no text (see `unindexed`).
    pub text: Option<&'a str>,
    pub source: Source,
    /// Content revision of `text` (FNV over path and text).
    pub revision: &'a str,
    pub symbols: Option<&'a Symbols>,
    /// Bib entry keys of a `.bib` file.
    pub bib: &'a [KeyAt],
    pub unindexed: Option<Unindexed>,
    /// Size of the text, or of the file on disk when it has none.
    pub bytes: u64,
}

/// Files, texts and symbols of one project.
#[derive(Debug, Default)]
pub struct ProjectIndex {
    files: BTreeMap<String, Record>,
    generation: u64,
    maps: OnceLock<Maps>,
}

impl ProjectIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bumped on every change; readers compare it to know their view is stale.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn changed(&mut self, rel: &str) {
        self.generation += 1;
        self.maps = OnceLock::new();
        if let Some(r) = self.files.get_mut(rel) {
            let text = r.text().map(|(t, _)| t);
            r.derived = derive(rel, text);
        }
    }

    /// Record what disk holds for `rel`. An overlay on it stays.
    pub fn set_disk(&mut self, rel: &str, disk: Disk) {
        match self.files.get_mut(rel) {
            Some(r) => r.disk = Some(disk),
            None => {
                self.files.insert(
                    rel.to_string(),
                    Record {
                        disk: Some(disk),
                        overlay: None,
                        derived: derive(rel, None),
                    },
                );
            }
        }
        self.changed(rel);
    }

    /// Forget `rel` on disk. A file with an overlay stays, as buffer text
    /// the user has not saved anywhere yet.
    pub fn remove(&mut self, rel: &str) {
        let keep = match self.files.get_mut(rel) {
            Some(r) => {
                r.disk = None;
                r.overlay.is_some()
            }
            None => return,
        };
        if keep {
            self.changed(rel);
        } else {
            self.files.remove(rel);
            self.generation += 1;
            self.maps = OnceLock::new();
        }
    }

    /// Forget every path under the directory `dir` (root-relative).
    pub fn remove_under(&mut self, dir: &str) {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        let gone: Vec<String> = self
            .files
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .map(|(k, _)| k.clone())
            .collect();
        for rel in gone {
            self.remove(&rel);
        }
    }

    /// Lay unsaved buffer text over `rel`, or lift it (`None`).
    pub fn set_overlay(&mut self, rel: &str, text: Option<String>) {
        match (self.files.get_mut(rel), text) {
            (Some(r), t) => {
                if r.overlay == t {
                    return;
                }
                r.overlay = t;
                if r.disk.is_none() && r.overlay.is_none() {
                    self.files.remove(rel);
                    self.generation += 1;
                    self.maps = OnceLock::new();
                    return;
                }
            }
            (None, Some(t)) => {
                self.files.insert(
                    rel.to_string(),
                    Record {
                        disk: None,
                        overlay: Some(t),
                        derived: derive(rel, None),
                    },
                );
            }
            (None, None) => return,
        }
        self.changed(rel);
    }

    /// Paths that carry an overlay.
    pub fn overlays(&self) -> impl Iterator<Item = &str> {
        self.files
            .iter()
            .filter(|(_, r)| r.overlay.is_some())
            .map(|(k, _)| k.as_str())
    }

    fn view<'a>(&'a self, rel: &'a str, r: &'a Record) -> FileView<'a> {
        let text = r.text();
        let (unindexed, disk_bytes) = match &r.disk {
            Some(Disk::Listed { bytes, reason }) => (Some(*reason), *bytes),
            _ => (None, 0),
        };
        FileView {
            rel,
            text: text.map(|(t, _)| t),
            source: text.map(|(_, s)| s).unwrap_or(Source::Disk),
            revision: &r.derived.revision,
            symbols: r.derived.symbols.as_ref(),
            bib: &r.derived.bib,
            unindexed: if text.is_some() { None } else { unindexed },
            bytes: text.map(|(t, _)| t.len() as u64).unwrap_or(disk_bytes),
        }
    }

    pub fn get(&self, rel: &str) -> Option<FileView<'_>> {
        self.files.get_key_value(rel).map(|(k, r)| self.view(k, r))
    }

    /// Every file, in path order.
    pub fn iter(&self) -> impl Iterator<Item = FileView<'_>> {
        self.files.iter().map(|(k, r)| self.view(k, r))
    }

    /// The definition maps, rebuilt on first read after a change.
    pub fn maps(&self) -> &Maps {
        self.maps.get_or_init(|| {
            let mut m = Maps::default();
            for f in self.iter() {
                if let Some(s) = f.symbols {
                    for l in &s.labels {
                        m.labels.entry(l.key.clone()).or_default().push(Loc {
                            rel: f.rel.to_string(),
                            line: l.line,
                        });
                    }
                    for d in &s.macros {
                        m.macros
                            .entry(d.name.clone())
                            .or_default()
                            .push(macro_def(f.rel, d));
                    }
                }
                for k in f.bib {
                    m.bib_entries.entry(k.key.clone()).or_default().push(Loc {
                        rel: f.rel.to_string(),
                        line: k.line,
                    });
                }
            }
            m
        })
    }
}

fn macro_def(rel: &str, d: &MacroAt) -> MacroDef {
    MacroDef {
        rel: rel.to_string(),
        line: d.line,
        command: d.command.clone(),
        params: d.params,
        body: d.body.clone(),
    }
}

/// The generated TypeScript module for the types the frontend receives
/// (`src/lib/generated/index.ts`).
pub fn typescript() -> String {
    use ts_rs::Config;
    let cfg = Config::new().with_large_int("number");
    let decls = [
        Source::decl(&cfg),
        Unindexed::decl(&cfg),
        Loc::decl(&cfg),
        MacroDef::decl(&cfg),
        search::Query::decl(&cfg),
        search::Hit::decl(&cfg),
        search::FileHits::decl(&cfg),
        search::SearchResult::decl(&cfg),
        search::FileMatch::decl(&cfg),
        replace::ReplaceHunk::decl(&cfg),
        replace::ReplaceFile::decl(&cfg),
        replace::ReplacePreview::decl(&cfg),
        replace::BufferEdit::decl(&cfg),
        replace::ReplaceApplied::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from src-tauri/index (maleficium-index). Do not edit:\n\
         // change the Rust types, then run\n\
         //   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace\n",
    );
    for d in decls {
        out.push_str("\nexport ");
        out.push_str(&d);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Disk {
        Disk::Text(s.to_string())
    }

    #[test]
    fn typescript_bindings_are_fresh() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/lib/generated/index.ts");
        let want = typescript();
        if std::env::var_os("MALEFICIUM_WRITE_TS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &want).unwrap();
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            have == want,
            "{} is stale: run MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace",
            path.display()
        );
    }

    #[test]
    fn overlay_wins_over_disk_and_lifts_back() {
        let mut i = ProjectIndex::new();
        i.set_disk("main.tex", text("\\label{disk}"));
        let disk_rev = i.get("main.tex").unwrap().revision.to_string();
        i.set_overlay("main.tex", Some("\\label{buf}".into()));
        let v = i.get("main.tex").unwrap();
        assert_eq!(v.source, Source::Buffer);
        assert_eq!(v.text, Some("\\label{buf}"));
        assert_ne!(v.revision, disk_rev);
        assert!(i.maps().labels.contains_key("buf"));
        assert!(!i.maps().labels.contains_key("disk"));
        // A save lands on disk while the overlay is still up: the buffer wins.
        i.set_disk("main.tex", text("\\label{saved}"));
        assert_eq!(i.get("main.tex").unwrap().text, Some("\\label{buf}"));
        i.set_overlay("main.tex", None);
        let v = i.get("main.tex").unwrap();
        assert_eq!((v.source, v.text), (Source::Disk, Some("\\label{saved}")));
    }

    #[test]
    fn an_overlay_keeps_a_file_deleted_on_disk() {
        let mut i = ProjectIndex::new();
        i.set_disk("a.tex", text("x"));
        i.set_overlay("a.tex", Some("y".into()));
        i.remove("a.tex");
        assert_eq!(i.get("a.tex").unwrap().text, Some("y"));
        i.set_overlay("a.tex", None);
        assert!(i.get("a.tex").is_none());
    }

    #[test]
    fn maps_collect_every_definition_across_files() {
        let mut i = ProjectIndex::new();
        i.set_disk(
            "a.tex",
            text("\\label{sec:x}\n\\newcommand{\\R}{\\mathbb{R}}"),
        );
        i.set_disk("b/c.tex", text("\n\\label{sec:x}"));
        i.set_disk("refs.bib", text("@book{knuth,\n}\n"));
        i.set_disk("notes.md", text("\\label{not-tex}"));
        let m = i.maps();
        assert_eq!(
            m.labels["sec:x"],
            vec![
                Loc {
                    rel: "a.tex".into(),
                    line: 1
                },
                Loc {
                    rel: "b/c.tex".into(),
                    line: 2
                }
            ]
        );
        assert!(!m.labels.contains_key("not-tex"));
        assert_eq!(
            m.bib_entries["knuth"],
            vec![Loc {
                rel: "refs.bib".into(),
                line: 1
            }]
        );
        assert_eq!(m.macros["\\R"][0].body, "\\mathbb{R}");
    }

    #[test]
    fn maps_follow_changes() {
        let mut i = ProjectIndex::new();
        i.set_disk("a.tex", text("\\label{one}"));
        assert!(i.maps().labels.contains_key("one"));
        let g = i.generation();
        i.set_disk("a.tex", text("\\label{two}"));
        assert!(i.generation() > g);
        assert!(!i.maps().labels.contains_key("one"));
        i.remove("a.tex");
        assert!(i.maps().labels.is_empty());
    }

    #[test]
    fn listed_files_carry_why_they_have_no_text() {
        let mut i = ProjectIndex::new();
        i.set_disk(
            "fig.png",
            Disk::Listed {
                bytes: 9,
                reason: Unindexed::NotText,
            },
        );
        let v = i.get("fig.png").unwrap();
        assert_eq!(
            (v.text, v.unindexed, v.bytes),
            (None, Some(Unindexed::NotText), 9)
        );
    }

    #[test]
    fn remove_under_drops_a_directory() {
        let mut i = ProjectIndex::new();
        for p in ["ch/a.tex", "ch/b/c.tex", "chx.tex", "main.tex"] {
            i.set_disk(p, text("x"));
        }
        i.remove_under("ch");
        let left: Vec<&str> = i.iter().map(|f| f.rel).collect();
        assert_eq!(left, ["chx.tex", "main.tex"]);
    }
}
