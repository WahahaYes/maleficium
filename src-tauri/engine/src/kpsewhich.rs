//! The engine as `kpsewhich`. latexml, built without a linked libkpathsea,
//! finds every TeX file by running a `kpsewhich` executable; `convert` and
//! `dump` put a link to this binary under that name first on the child's
//! `PATH`, so a paper converts with no TeX installed. The binary also answers
//! as `maleficium-engine kpsewhich ...` for tests.
//!
//! Protocol (all latexml uses of it):
//! - `--miktex-disable-installer` anywhere: exit 1 (this is not MiKTeX).
//! - `--version`: `kpathsea version 6.3.4`.
//! - `-var-value=SELFAUTOPARENT` (or `--var-value`): the pinned TeX Live tree
//!   `/nonexistent/texlive/2022`; its last component is the year that picks
//!   latexml's format dump. Any other variable is unset: one blank line.
//! - `--expand-var ...`: two blank lines (no ls-R cache to report).
//! - `[--format=X] name...`: the first path a name resolves to, exit 0, or
//!   exit 1.
//!
//! A name (tried as given, then with `.tex` appended) resolves, in order, to
//! (a) a file embedded in this binary from `embedded/`, written to the scratch
//! dir on demand, (b) the Tectonic cache, `bundles/data/<digest>/<name>`, the
//! same flat directory `compile` fills, then (c) a fetch through the
//! `tectonic_bundles` API, which fills that cache in Tectonic's own format. A
//! name the bundle index does not list answers without touching the network,
//! and `MALEFICIUM_CACHED_ONLY=1` never fetches.
//!
//! Environment (set by `session::prepare`): `MALEFICIUM_BUNDLE_URL` (the
//! bundle to resolve against), `TECTONIC_CACHE_DIR`, `MALEFICIUM_SCRATCH`,
//! `MALEFICIUM_CACHED_ONLY`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use tectonic_bundles::detect_bundle;
use tectonic_io_base::app_dirs;
use tectonic_io_base::OpenResult;
use tectonic_status_base::NoopStatusBackend;

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// What `--version` prints; latexml only checks that the call succeeds.
const VERSION: &str = "kpathsea version 6.3.4";
/// The TeX Live tree the pinned bundle stands in for.
const SELF_AUTO_PARENT: &str = "/nonexistent/texlive/2022";

/// Where names resolve from.
pub struct Resolver<'a> {
    /// Embedded files by name.
    pub embedded: &'a [(&'a str, &'a [u8])],
    /// Directory the embedded files are written under, on demand.
    pub scratch: PathBuf,
    /// Tectonic's `bundles` cache directory (holds `data/` and `hashes/`).
    pub bundles: Option<PathBuf>,
    /// The bundle to resolve against; without one nothing is fetched and the
    /// cache cannot be located.
    pub bundle_url: Option<String>,
    pub cached_only: bool,
    index: Option<Option<String>>,
}

impl<'a> Resolver<'a> {
    pub fn new(
        embedded: &'a [(&'a str, &'a [u8])],
        scratch: PathBuf,
        bundles: Option<PathBuf>,
        bundle_url: Option<String>,
        cached_only: bool,
    ) -> Self {
        Resolver {
            embedded,
            scratch,
            bundles,
            bundle_url,
            cached_only,
            index: None,
        }
    }

    /// The resolver the process environment describes.
    pub fn from_env() -> Resolver<'static> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let scratch = var("MALEFICIUM_SCRATCH")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("maleficium-kpsewhich"));
        Resolver::new(
            EMBEDDED,
            scratch,
            app_dirs::get_user_cache_dir("bundles").ok(),
            var("MALEFICIUM_BUNDLE_URL"),
            var("MALEFICIUM_CACHED_ONLY").is_some_and(|v| v == "1"),
        )
    }

    /// The first path `name` resolves to, trying the name and then the name
    /// with `.tex`. Only the last path component is looked up: the bundle is
    /// one flat directory.
    pub fn resolve(&mut self, name: &str) -> Option<PathBuf> {
        let base = name.rsplit(['/', '\\']).next().filter(|b| !b.is_empty())?;
        let with_tex = format!("{base}.tex");
        let found = [base, with_tex.as_str()]
            .into_iter()
            .find_map(|cand| self.resolve_exact(cand));
        found
    }

    fn resolve_exact(&mut self, name: &str) -> Option<PathBuf> {
        if let Some(path) = self.embedded_path(name) {
            return Some(path);
        }
        if let Some(path) = self.cached_path(name) {
            return Some(path);
        }
        self.fetch(name)
    }

    /// Writes the embedded file (when it is missing or differs) and returns
    /// its path.
    fn embedded_path(&self, name: &str) -> Option<PathBuf> {
        let (_, bytes) = self.embedded.iter().find(|(n, _)| *n == name)?;
        let dir = self.scratch.join("embedded");
        let path = dir.join(name);
        if fs::read(&path).is_ok_and(|have| have == *bytes) {
            return Some(path);
        }
        fs::create_dir_all(&dir).ok()?;
        let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
        fs::write(&tmp, bytes).ok()?;
        fs::rename(&tmp, &path).ok()?;
        Some(path)
    }

    /// The directory holding the bundle's cached files: the digest the engine
    /// recorded for the URL, under `data/`.
    fn data_dir(&self) -> Option<PathBuf> {
        let bundles = self.bundles.as_ref()?;
        let key = app_dirs::sanitize(self.bundle_url.as_ref()?);
        let digest = fs::read_to_string(bundles.join("hashes").join(key)).ok()?;
        let digest = digest.trim();
        (!digest.is_empty()).then(|| bundles.join("data").join(digest))
    }

    fn cached_path(&self, name: &str) -> Option<PathBuf> {
        let path = self.data_dir()?.join(name);
        path.is_file().then_some(path)
    }

    /// The bundle index text, read once. `None` while the bundle has never
    /// been resolved into this cache.
    fn index(&mut self) -> Option<&str> {
        if self.index.is_none() {
            let text = self.data_dir().and_then(|d| {
                let file = format!("{}.index", d.file_name()?.to_str()?);
                fs::read_to_string(d.parent()?.join(file)).ok()
            });
            self.index = Some(text);
        }
        self.index.as_ref()?.as_deref()
    }

    /// Whether the bundle lists `name`. With no cached index the answer is
    /// `true`: only the fetch itself can tell, and it downloads the index.
    fn listed(&mut self, name: &str) -> bool {
        match self.index() {
            Some(text) => text
                .lines()
                .any(|l| l.split(' ').next().is_some_and(|n| n == name)),
            None => true,
        }
    }

    /// Opens `name` through the bundle so Tectonic's cache writes it (and the
    /// index and digest on a cold cache), then returns the cached path. The
    /// API has no single-file call: opening also replays Tectonic's recorded
    /// working set into a cold cache and records `name` in it.
    fn fetch(&mut self, name: &str) -> Option<PathBuf> {
        if self.cached_only || !self.listed(name) {
            return None;
        }
        let url = self.bundle_url.clone()?;
        let mut bundle = detect_bundle(url, false, None).ok()??;
        match bundle.input_open_name(name, &mut NoopStatusBackend::default()) {
            OpenResult::Ok(_) => {}
            _ => return None,
        }
        self.index = None;
        self.cached_path(name)
    }
}

/// Whether this process was started as `kpsewhich`: a link or copy of the
/// engine with that file stem.
pub fn invoked_as_kpsewhich(argv0: Option<&OsString>) -> bool {
    argv0
        .map(Path::new)
        .and_then(Path::file_stem)
        .is_some_and(|s| s == "kpsewhich")
}

/// Answers one kpsewhich call; the exit code and stdout lines.
pub fn answer(args: &[String], resolver: &mut Resolver) -> (i32, Vec<String>) {
    if args.iter().any(|a| a == "--miktex-disable-installer") {
        return (1, vec![]);
    }
    if args.iter().any(|a| a == "--version") {
        return (0, vec![VERSION.to_string()]);
    }
    let var_value = args
        .iter()
        .position(|a| a.starts_with("-var-value") || a.starts_with("--var-value"));
    if let Some(i) = var_value {
        let arg = &args[i];
        let name = match arg.split_once('=') {
            Some((_, v)) => Some(v),
            None => args.get(i + 1).map(String::as_str),
        };
        let value = if name == Some("SELFAUTOPARENT") {
            SELF_AUTO_PARENT
        } else {
            ""
        };
        return (0, vec![value.to_string()]);
    }
    if args.iter().any(|a| a == "--expand-var") {
        return (0, vec![String::new(), String::new()]);
    }
    for name in args.iter().filter(|a| !a.starts_with('-')) {
        if let Some(path) = resolver.resolve(name) {
            return (0, vec![path.to_string_lossy().into_owned()]);
        }
    }
    (1, vec![])
}

/// `kpsewhich` entry point: the arguments after the program name.
pub fn run(args: Vec<OsString>) -> i32 {
    let args: Vec<String> = args
        .into_iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // Building the resolver only reads the environment; the cache is not
    // touched until a name needs it, so the probes stay trivial.
    let mut resolver = Resolver::from_env();
    let (code, lines) = answer(&args, &mut resolver);
    for line in lines {
        println!("{line}");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &[(&str, &[u8])] = &[
        ("hook.sty.rhai", b"hello"),
        ("both", b"x"),
        ("both.tex", b"y"),
    ];

    struct Dirs(PathBuf);
    impl Dirs {
        fn new(tag: &str) -> Dirs {
            let d = std::env::temp_dir().join(format!("kpse-test-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&d);
            fs::create_dir_all(&d).unwrap();
            Dirs(d)
        }
    }
    impl Drop for Dirs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const URL: &str = "https://example.invalid/bundle.tar";
    const DIGEST: &str = "abc123";

    /// A cache holding `files` for URL, with an index listing `listed`.
    fn cache(d: &Dirs, files: &[(&str, &str)], listed: &[&str]) -> PathBuf {
        let bundles = d.0.join("bundles");
        let hashes = bundles.join("hashes");
        let data = bundles.join("data").join(DIGEST);
        fs::create_dir_all(&hashes).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(hashes.join(app_dirs::sanitize(URL)), format!("{DIGEST}\n")).unwrap();
        for (n, c) in files {
            fs::write(data.join(n), c).unwrap();
        }
        let index: String = listed.iter().map(|n| format!("{n} 0 1\n")).collect();
        fs::write(bundles.join("data").join(format!("{DIGEST}.index")), index).unwrap();
        bundles
    }

    fn resolver<'a>(d: &Dirs, bundles: Option<PathBuf>, cached_only: bool) -> Resolver<'a> {
        Resolver::new(
            TABLE,
            d.0.join("scratch"),
            bundles,
            Some(URL.to_string()),
            cached_only,
        )
    }

    fn call(r: &mut Resolver, args: &[&str]) -> (i32, Vec<String>) {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        answer(&args, r)
    }

    #[test]
    fn probes_answer_like_kpsewhich() {
        let d = Dirs::new("probes");
        let mut r = resolver(&d, None, true);
        assert_eq!(call(&mut r, &["--version"]), (0, vec![VERSION.into()]));
        assert_eq!(
            call(&mut r, &["--miktex-disable-installer", "--version"]).0,
            1
        );
        assert_eq!(
            call(&mut r, &["-var-value=SELFAUTOPARENT"]),
            (0, vec![SELF_AUTO_PARENT.into()])
        );
        assert_eq!(
            call(&mut r, &["--var-value", "SELFAUTOPARENT"]),
            (0, vec![SELF_AUTO_PARENT.into()])
        );
        assert_eq!(
            call(&mut r, &["-var-value=TEXMFHOME"]),
            (0, vec![String::new()])
        );
        assert_eq!(
            call(&mut r, &["--expand-var", "$TEXMF", "--show-path", "tex"]),
            (0, vec![String::new(), String::new()])
        );
    }

    #[test]
    fn year_comes_from_the_parent_name() {
        assert_eq!(Path::new(SELF_AUTO_PARENT).file_name().unwrap(), "2022");
    }

    #[test]
    fn embedded_files_are_extracted_on_demand() {
        let d = Dirs::new("embedded");
        let mut r = resolver(&d, None, true);
        let (code, out) = call(&mut r, &["--format=tex", "hook.sty.rhai"]);
        assert_eq!(code, 0);
        assert_eq!(fs::read(&out[0]).unwrap(), b"hello");
        assert!(out[0].starts_with(d.0.join("scratch").to_str().unwrap()));
        // A stale copy is rewritten.
        fs::write(&out[0], "stale").unwrap();
        let (_, again) = call(&mut r, &["hook.sty.rhai"]);
        assert_eq!(fs::read(&again[0]).unwrap(), b"hello");
        // Directories in the name are ignored.
        assert_eq!(call(&mut r, &["some/dir/hook.sty.rhai"]).0, 0);
    }

    #[test]
    fn the_name_is_tried_before_name_dot_tex() {
        let d = Dirs::new("order");
        let mut r = resolver(&d, None, true);
        let (_, out) = call(&mut r, &["both"]);
        assert_eq!(fs::read(&out[0]).unwrap(), b"x");
        let c = cache(&d, &[("only.tex", "z")], &["only.tex"]);
        let mut r = resolver(&d, Some(c), true);
        let (code, out) = call(&mut r, &["only"]);
        assert_eq!(code, 0);
        assert_eq!(fs::read(&out[0]).unwrap(), b"z");
    }

    #[test]
    fn embedded_wins_over_the_cache() {
        let d = Dirs::new("wins");
        let c = cache(&d, &[("both", "from cache")], &["both"]);
        let mut r = resolver(&d, Some(c), true);
        let (_, out) = call(&mut r, &["both"]);
        assert_eq!(fs::read(&out[0]).unwrap(), b"x");
    }

    #[test]
    fn cached_files_resolve_without_the_network() {
        let d = Dirs::new("cached");
        let c = cache(&d, &[("article.cls", "cls")], &["article.cls", "other.sty"]);
        let mut r = resolver(&d, Some(c.clone()), true);
        let (code, out) = call(&mut r, &["--format=tex", "article.cls"]);
        assert_eq!(code, 0);
        assert_eq!(
            PathBuf::from(&out[0]),
            c.join("data").join(DIGEST).join("article.cls")
        );
        // Listed but not cached: cached-only never fetches.
        assert_eq!(call(&mut r, &["other.sty"]), (1, vec![]));
    }

    #[test]
    fn names_the_index_lacks_miss_without_fetching() {
        let d = Dirs::new("absent");
        let c = cache(&d, &[], &["listed.sty"]);
        // Not cached-only, and the URL cannot be reached: an unlisted name
        // must still answer (fast) as a miss rather than try the network.
        let mut r = resolver(&d, Some(c), false);
        assert!(!r.listed("nope.sty"));
        assert!(r.listed("listed.sty"));
        assert_eq!(call(&mut r, &["nope.sty"]), (1, vec![]));
    }

    #[test]
    fn invoked_as_kpsewhich_reads_the_file_stem() {
        for (arg, want) in [
            ("/x/bin/kpsewhich", true),
            ("kpsewhich", true),
            ("C:\\x\\kpsewhich.exe", cfg!(windows)),
            ("/x/bin/maleficium-engine", false),
        ] {
            assert_eq!(
                invoked_as_kpsewhich(Some(&OsString::from(arg))),
                want,
                "{arg}"
            );
        }
        assert!(!invoked_as_kpsewhich(None));
    }
}
