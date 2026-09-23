//! Project search over the live index: text search and the file finder,
//! addressed by session root. Every result is root-relative and names the
//! source and revision of each file it read.

use maleficium_index::definition::{self, Lookup, RefAt};
use maleficium_index::search::{self, FileMatch, Query, SearchResult};

/// Hits one search returns by default.
pub const MAX_HITS: usize = 1000;
/// Files one finder query returns by default.
pub const MAX_FILE_MATCHES: usize = 50;

/// Search the project; files of `main_rel`'s document rank first.
pub fn search(
    root_id: &str,
    q: &Query,
    main_rel: Option<&str>,
    max: usize,
) -> Result<SearchResult, String> {
    super::index::with(root_id, |l| search::search(&l.index, q, main_rel, max))?
}

/// The best files for a finder query.
pub fn find_files(root_id: &str, query: &str, max: usize) -> Result<Vec<FileMatch>, String> {
    super::index::with(root_id, |l| search::find_files(&l.index, query, max))
}

/// What UTF-16 column `col` of `line` (text the caller holds) refers to,
/// and where it is defined; `None` when nothing sits there.
pub fn definition_at(
    root_id: &str,
    line: &str,
    col: u32,
    main_rel: Option<&str>,
) -> Result<Option<Lookup>, String> {
    let Some(r) = definition::ref_at(line, col) else {
        return Ok(None);
    };
    super::index::with(root_id, |l| {
        Some(definition::lookup(&l.index, &r, main_rel))
    })
}

/// Where a reference is defined: a known ref, or the one at a 1-based line
/// and UTF-16 column of an indexed file.
pub fn definition(
    root_id: &str,
    target: DefinitionTarget,
    main_rel: Option<&str>,
) -> Result<Option<Lookup>, String> {
    super::index::with(root_id, |l| {
        let r = match target {
            DefinitionTarget::Ref(r) => Some(r),
            DefinitionTarget::At { rel, line, col } => {
                let f = l
                    .index
                    .get(&rel)
                    .ok_or_else(|| format!("not an indexed file: {}", rel))?;
                let text = f.text.ok_or_else(|| format!("no text for {}", rel))?;
                let line_text = text
                    .lines()
                    .nth(line.saturating_sub(1) as usize)
                    .unwrap_or("");
                definition::ref_at(line_text, col)
            }
        };
        Ok(r.map(|r| definition::lookup(&l.index, &r, main_rel)))
    })?
}

/// What `definition` looks up.
pub enum DefinitionTarget {
    Ref(RefAt),
    At { rel: String, line: u32, col: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn searches_a_granted_root_and_rejects_others() {
        let dir = std::env::temp_dir().join(format!("maleficium-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ch")).unwrap();
        std::fs::write(dir.join("main.tex"), "\\input{ch/a}\nneedle\n").unwrap();
        std::fs::write(dir.join("ch/a.tex"), "a needle here\n").unwrap();
        let canon = dir.canonicalize().unwrap();
        super::super::grant_root("search-t", &canon.to_string_lossy()).unwrap();
        let q = Query {
            pattern: "needle".into(),
            ..Default::default()
        };
        let r = search("search-t", &q, Some("main.tex"), MAX_HITS).unwrap();
        let files: Vec<&str> = r.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(files, ["main.tex", "ch/a.tex"]);
        assert_eq!(find_files("search-t", "cha", 5).unwrap()[0].rel, "ch/a.tex");
        assert!(search("nope", &q, None, 10).is_err());
        let d = definition_at("search-t", "see \\input{ch/a}", 8, Some("main.tex"))
            .unwrap()
            .unwrap();
        assert_eq!(d.definitions[0].rel, "ch/a.tex");
        assert!(definition_at("search-t", "plain words", 3, None)
            .unwrap()
            .is_none());
        let at = DefinitionTarget::At {
            rel: "main.tex".into(),
            line: 1,
            col: 3,
        };
        assert_eq!(
            definition("search-t", at, Some("main.tex"))
                .unwrap()
                .unwrap()
                .definitions[0]
                .rel,
            "ch/a.tex"
        );
    }

    /// Budget pin: a regex search over a synthetic 3000-file project.
    #[test]
    fn budget_regex_search() {
        let dir =
            std::env::temp_dir().join(format!("maleficium-search-{}-budget", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in 0..3000 {
            let body = format!(
                "\\section{{S{n}}}\\label{{sec:{n}}}\n{}See \\ref{{sec:{}}}.\n",
                "Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n".repeat(40),
                n / 2
            );
            std::fs::write(dir.join(format!("f{n:04}.tex")), body).unwrap();
        }
        let canon = dir.canonicalize().unwrap();
        super::super::grant_root("search-budget", &canon.to_string_lossy()).unwrap();
        super::super::index::open("search-budget").unwrap();
        super::super::index::set_watched("search-budget", true).unwrap();
        let q = Query {
            pattern: r"\\ref\{sec:\d+\}".into(),
            regex: true,
            ..Default::default()
        };
        let t = Instant::now();
        let r = search("search-budget", &q, None, MAX_HITS).unwrap();
        let ms = t.elapsed().as_millis();
        println!(
            "search budget: regex over {} files, {} hits (+{} capped) in {ms} ms",
            r.searched, r.hits, r.truncated
        );
        assert_eq!(r.hits as usize + r.truncated as usize, 3000);
        assert!(ms < 3_000, "regex search {ms} ms over the 3 s cap");
        let t = Instant::now();
        let f = find_files("search-budget", "f29", MAX_FILE_MATCHES).unwrap();
        println!(
            "search budget: finder over 3000 files in {} ms",
            t.elapsed().as_millis()
        );
        assert!(!f.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
