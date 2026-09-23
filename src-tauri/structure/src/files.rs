//! Which paths hold editable text: the one list the editor, history and the
//! project index share (the frontend receives it as a generated constant).

/// Extensions (lowercase, with the dot) of files that open as text.
pub const TEXT_EXTENSIONS: &[&str] = &[
    ".tex",
    ".bib",
    ".sty",
    ".cls",
    ".md",
    ".markdown",
    ".txt",
    ".log",
    ".aux",
    ".toc",
    ".lof",
    ".lot",
    ".out",
    ".fls",
    ".json",
    ".yaml",
    ".yml",
    ".toml",
    ".xml",
    ".html",
    ".htm",
    ".css",
    ".js",
    ".ts",
    ".tsx",
    ".jsx",
    ".py",
    ".sh",
    ".csv",
    ".r",
    ".jl",
    ".bst",
    ".dtx",
    ".ins",
    ".ltx",
    ".bbx",
    ".cbx",
    ".def",
    ".cfg",
    ".clo",
    ".lbx",
    ".tikz",
    ".pgf",
];

/// Lowercase extension of the last path segment, with the dot; empty when
/// the name has none.
pub fn ext_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(i) => name[i..].to_lowercase(),
        None => String::new(),
    }
}

/// True for a path that opens as text: a listed extension, or none at all.
pub fn is_text_path(path: &str) -> bool {
    let ext = ext_of(path);
    ext.is_empty() || TEXT_EXTENSIONS.contains(&ext.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_last_segment() {
        assert!(is_text_path("chapters/intro.tex"));
        assert!(is_text_path("Makefile"));
        assert!(is_text_path("dir.v2/README"));
        assert!(is_text_path("refs.BIB"));
        assert!(!is_text_path("fig/plot.png"));
        assert!(!is_text_path("paper.pdf"));
        assert_eq!(ext_of("a/b.TeX"), ".tex");
    }
}
