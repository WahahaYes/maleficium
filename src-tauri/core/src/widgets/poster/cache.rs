//! The project's poster cache: `.maleficium/` in the main file's folder,
//! written by a compile (the user's explicit action).
//!
//! ```text
//! .maleficium/README.md             what the folder is (written once)
//! .maleficium/posters/<sha256>.png  one auto-poster per widget state
//! .maleficium/posters/<job>.map     widget id -> poster, read by the package
//! ```
//!
//! A poster's name is [`poster_key`]: a hash of the widget's runtime (and
//! the runtime document's bytes), its canonical options, its source files'
//! bytes, the poster theme and [`RENDERER_VERSION`]. Nothing about where
//! the widget sits on the page enters it, so a poster replacing its
//! placeholder can never make the key move.
//!
//! A compile runs in two passes around the engine. [`before_compile`]
//! reads the previous compile's widget list (the document is never
//! pre-scanned), renders the auto-posters it lacks through the configured
//! [`PosterRenderer`], and writes the map; the package then typesets each
//! mapped poster, or a placeholder. A widget new in this compile is only
//! known once the engine has written its widget list, so the compile calls
//! [`before_compile`] again after the engine and, when that rendered a
//! poster, runs the engine once more: the pdf it hands back shows the new
//! widget's poster, not a placeholder. [`after_compile`]
//! rewrites the map from the new widget list and collects garbage: posters
//! no widget uses go, then the oldest beyond the byte cap.
//!
//! Each map entry carries the widget's sidecar record (runtime, sources,
//! options as the package wrote them); the package uses a poster only when
//! the record it is about to write matches, so an edited widget shows its
//! placeholder until its new poster exists, never a stale one.
//!
//! An html widget (author code) takes part only while the user's approval
//! covers its folder: its key is made from the approval digest of the
//! folder, so any edit names a poster that does not exist yet, and an
//! unapproved, revoked or changed widget gets no map entry at all, cached
//! file or not. A custom widget takes part only while its runtime is
//! approved, and its key is made from the runtime's digest with the same
//! effect. The renderer checks the approval again when it runs the
//! job (see [`super::prepare`]); the check here only decides what to ask
//! for and what to map.

use super::{
    poster_source, read_sources, PosterOutcome, PosterRequest, PosterSource, CUSTOM_TIMEOUT_MS,
    HTML_RUNTIME,
};
use crate::bundle::fold;
use crate::theme::Theme;
use crate::widget_approval::{self, ApprovalRequired, WidgetApprovalStatus, WidgetTarget};
use crate::widgets::{read, sidecar_guards, widgets, Widget, WidgetType};
use crate::Core;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

/// The project folder the app owns, beside the main file.
pub const CACHE_DIR: &str = ".maleficium";
/// The posters and maps, inside [`CACHE_DIR`].
pub const POSTERS_DIR: &str = "posters";
/// Bump whenever the renderer changes what a poster looks like for the
/// same widget: every cached poster then renders again.
pub const RENDERER_VERSION: u32 = 1;
/// The cache's byte cap when [`CAP_ENV`] does not set one.
pub const DEFAULT_CAP_BYTES: u64 = 100 * 1024 * 1024;
/// Overrides the byte cap, in MiB.
pub const CAP_ENV: &str = "MALEFICIUM_POSTER_CACHE_MIB";

const README_NAME: &str = "README.md";
const MAP_EXT: &str = "map";
/// Process start and window setup on top of the per-render limits.
const PROCESS_SLACK_MS: u64 = 30_000;
/// A compile's time limit for one html widget's poster: author code that
/// never answers the snapshot request costs each compile this long.
const HTML_TIMEOUT_MS: u64 = 5_000;

/// Renders poster requests. The desktop app renders in-process; elsewhere
/// the app binary's headless mode does ([`ProcessRenderer`]).
pub trait PosterRenderer: Send + Sync {
    /// One result per request, in order.
    fn render(&self, cx: &Core, reqs: &[PosterRequest]) -> Vec<Result<PosterOutcome, String>>;
    /// The compile's render: the renderer subprocess (if any) parks in
    /// `slot` where cancel finds it, and `cancelled` stops the run. The
    /// default runs [`render`](Self::render) to the end: renderers without
    /// a subprocess cannot stop mid-request.
    fn render_cancel(
        &self,
        cx: &Core,
        reqs: &[PosterRequest],
        slot: &Mutex<Option<Child>>,
        cancelled: &AtomicBool,
    ) -> Vec<Result<PosterOutcome, String>> {
        let _ = (slot, cancelled);
        self.render(cx, reqs)
    }
}

/// The renderer a compile uses: the one the adapter installed, else the
/// app binary beside this program, else none (posters stay placeholders).
pub(crate) fn renderer(cx: &Core) -> Option<Arc<dyn PosterRenderer>> {
    cx.poster_renderer()
        .or_else(|| ProcessRenderer::find().map(|r| Arc::new(r) as Arc<dyn PosterRenderer>))
}

/// The byte cap: [`CAP_ENV`] in MiB, or [`DEFAULT_CAP_BYTES`].
pub fn cap_bytes() -> u64 {
    std::env::var(CAP_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(DEFAULT_CAP_BYTES, |mib| mib.saturating_mul(1024 * 1024))
}

/// One source as the key sees it: role key, path, bytes.
pub(crate) type KeySource = (String, String, Vec<u8>);

fn feed(h: &mut Sha256, field: &[u8]) {
    h.update((field.len() as u64).to_le_bytes());
    h.update(field);
}

/// The key of one widget state, as lowercase sha256 hex. Every field is
/// length-prefixed; options are sorted; sources keep their role order.
pub(crate) fn digest(
    kind: &str,
    runtime: &str,
    runtime_doc: &str,
    options: &[(String, String)],
    sources: &[KeySource],
    tokens: &str,
) -> String {
    let mut h = Sha256::new();
    feed(&mut h, b"maleficium-poster");
    feed(&mut h, &RENDERER_VERSION.to_le_bytes());
    feed(&mut h, kind.as_bytes());
    feed(&mut h, runtime.as_bytes());
    feed(&mut h, &Sha256::digest(runtime_doc.as_bytes()));
    let mut opts: Vec<&(String, String)> = options.iter().collect();
    opts.sort();
    feed(&mut h, &(opts.len() as u64).to_le_bytes());
    for (k, v) in opts {
        feed(&mut h, k.as_bytes());
        feed(&mut h, v.as_bytes());
    }
    feed(&mut h, &(sources.len() as u64).to_le_bytes());
    for (key, path, bytes) in sources {
        feed(&mut h, key.as_bytes());
        feed(&mut h, path.as_bytes());
        feed(&mut h, &Sha256::digest(bytes));
    }
    feed(&mut h, tokens.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a widget's poster can come from the cache: first-party model and
/// chart runtimes, html widgets (the latter only while approved, see
/// [`keyed`]), and custom widgets (only while their runtime is approved),
/// and never when the document gives its own poster.
fn auto(w: &Widget) -> bool {
    matches!(
        w.kind,
        WidgetType::Model | WidgetType::Chart | WidgetType::Html | WidgetType::Custom
    ) && !matches!(poster_source(w, None), PosterSource::Explicit(_))
}

fn key_options(w: &Widget) -> Vec<(String, String)> {
    w.options
        .iter()
        .map(|o| (o.key.clone(), o.value.clone()))
        .collect()
}

fn key_tokens(theme: &Theme) -> String {
    let mut tokens: Vec<(String, String)> = super::poster_tokens(theme)
        .into_iter()
        .map(|(k, v)| (k, v.as_str().unwrap_or("").to_string()))
        .collect();
    tokens.sort();
    tokens
        .iter()
        .map(|(k, v)| format!("{k}:{v};"))
        .collect::<String>()
}

/// The cache key of one first-party runtime widget of `main_rel`'s last
/// compile. An html widget has none of its own: see [`html_key`].
pub fn poster_key(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    w: &Widget,
    theme: &Theme,
) -> Result<String, String> {
    let kind = match w.kind {
        WidgetType::Model => "model",
        WidgetType::Chart => "chart",
        WidgetType::Video => "video",
        WidgetType::Table => "table",
        WidgetType::Html => {
            return Err(format!(
                "widget {}: an html widget's poster is keyed by its approval digest",
                w.id
            ))
        }
        WidgetType::Custom => {
            return Err(format!(
                "widget {}: a custom widget's poster is keyed by its runtime digest",
                w.id
            ))
        }
    };
    let runtime = w.runtime.as_deref().unwrap_or("");
    let doc = super::host_document(runtime)
        .ok_or_else(|| format!("widget {}: no {runtime} runtime in this build", w.id))?;
    let sources: Vec<KeySource> = read_sources(cx, root_id, main_rel, w)?
        .into_iter()
        .map(|(k, s, b)| (k, s.path.clone(), b))
        .collect();
    Ok(digest(
        kind,
        runtime,
        doc,
        &key_options(w),
        &sources,
        &key_tokens(theme),
    ))
}

/// The cache key of an html widget whose folder hashes to `approval_digest`
/// (every file and the declared origins): any edit changes it.
pub fn html_key(w: &Widget, folder: &str, approval_digest: &str, theme: &Theme) -> String {
    let sources: Vec<KeySource> = vec![(
        "bundle".to_string(),
        folder.to_string(),
        approval_digest.as_bytes().to_vec(),
    )];
    digest(
        "html",
        HTML_RUNTIME,
        &fold::widget_policy(None),
        &key_options(w),
        &sources,
        &key_tokens(theme),
    )
}

/// The cache key material of a custom widget judged approved on `snap`:
/// the runtime's digest and ref, the bound sources and options, the poster
/// theme and the renderer version. Any change names a poster that does not
/// exist yet.
fn custom_material(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    w: &Widget,
    theme: &Theme,
    snap: &widget_approval::RuntimeSnapshot,
    manifest: &crate::runtimes::RuntimeManifest,
) -> Result<(String, String), String> {
    let reference = w.runtime.clone().unwrap_or_default();
    let bound = crate::runtimes::bind(w, manifest)?;
    let main_dir = Path::new(main_rel)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut sources: Vec<KeySource> = vec![(
        "runtime".to_string(),
        reference.clone(),
        snap.digest.as_bytes().to_vec(),
    )];
    for (role, s) in &bound.sources {
        let path = crate::fs::resolve_in(cx, root_id, &main_dir.join(&s.path).to_string_lossy())
            .map_err(|e| format!("widget {}: {}: {e}", w.id, s.path))?;
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("widget {}: cannot read {}: {e}", w.id, s.path))?;
        sources.push((role.clone(), s.path.clone(), bytes));
    }
    let options: Vec<(String, String)> = bound
        .options
        .iter()
        .map(|(k, v)| (k.clone(), v.to_string()))
        .collect();
    Ok((
        digest(
            "custom",
            &reference,
            &fold::widget_policy(None),
            &options,
            &sources,
            &key_tokens(theme),
        ),
        snap.digest.clone(),
    ))
}

/// How a widget stands with the cache.
enum Keyed {
    /// Its poster's key, and for an html widget the approval digest the key
    /// was made from.
    Key(String, Option<String>),
    /// An html widget the user has not approved: no key, no poster.
    Approval(Box<ApprovalRequired>),
}

fn keyed(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    w: &Widget,
    theme: &Theme,
) -> Result<Keyed, String> {
    if w.kind == WidgetType::Custom {
        return custom_keyed(base, cx, root_id, main_rel, w, theme);
    }
    let Some(target) = WidgetTarget::of(main_rel, w)? else {
        return poster_key(cx, root_id, main_rel, w, theme).map(|k| Keyed::Key(k, None));
    };
    let checked = widget_approval::check_at(base, cx, root_id, &target)?;
    Ok(match checked.status {
        WidgetApprovalStatus::Approved(_) => {
            let d = checked.snapshot.digest;
            Keyed::Key(html_key(w, &checked.snapshot.path, &d, theme), Some(d))
        }
        WidgetApprovalStatus::ApprovalRequired(r) => Keyed::Approval(Box::new(r)),
    })
}

/// A custom widget's standing: judged on one snapshot of its runtime
/// package, keyed only while the verdict is approved (allowed at this
/// digest, or an auto-covered change). Anything else gets no map entry.
fn custom_keyed(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    w: &Widget,
    theme: &Theme,
) -> Result<Keyed, String> {
    let reference = w.runtime.clone().unwrap_or_default();
    let list = read(cx, root_id, main_rel)?.0;
    let users: Vec<String> = list
        .widgets
        .iter()
        .filter(|v| {
            v.kind == WidgetType::Custom && v.runtime.as_deref() == Some(reference.as_str())
        })
        .map(|v| v.id.clone())
        .collect();
    let checked = widget_approval::check_runtime_at(base, cx, root_id, &reference, &users)?;
    let approved = matches!(checked.status, Some(WidgetApprovalStatus::Approved(_)));
    if !approved {
        return Ok(match checked.status {
            Some(WidgetApprovalStatus::ApprovalRequired(r)) => Keyed::Approval(Box::new(r)),
            _ => {
                let reason = checked
                    .snapshot
                    .invalid
                    .clone()
                    .unwrap_or_else(|| format!("runtime {reference} cannot be judged"));
                return Err(format!("widget {}: {reason}", w.id));
            }
        });
    }
    let snap = &checked.snapshot;
    let manifest = snap.manifest.as_ref().ok_or_else(|| {
        snap.invalid
            .clone()
            .unwrap_or_else(|| format!("runtime {reference} is not valid"))
    })?;
    let (key, digest) = custom_material(cx, root_id, main_rel, w, theme, snap, manifest)?;
    Ok(Keyed::Key(key, Some(digest)))
}

/// The compile line for an html widget that waits for the user.
pub fn approval_line(r: &ApprovalRequired) -> String {
    let cause = serde_json::to_value(r.cause)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    format!(
        "poster {}: approval_required ({cause}): {}",
        r.widget, r.message
    )
}

/// The html widgets of the last compile's widget list that wait for the
/// user's approval: the ones the window asks about. A widget whose document
/// gives its own poster is asked about too: it still runs when the app
/// renders it, and View > Widgets lists it as pending.
pub fn approvals_needed(cx: &Core, root_id: &str, main_rel: &str) -> Vec<ApprovalRequired> {
    approvals_needed_at(&widget_approval::store_base(), cx, root_id, main_rel)
}

/// [`approvals_needed`] against the approval store at `base`.
pub(crate) fn approvals_needed_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
) -> Vec<ApprovalRequired> {
    let Ok(list) = widgets(cx, root_id, main_rel) else {
        return Vec::new();
    };
    list.widgets
        .iter()
        .filter_map(|w| WidgetTarget::of(main_rel, w).ok().flatten())
        .filter_map(|t| match widget_approval::check_at(base, cx, root_id, &t) {
            Ok(c) => match c.status {
                WidgetApprovalStatus::ApprovalRequired(r) => Some(r),
                WidgetApprovalStatus::Approved(_) => None,
            },
            Err(_) => None,
        })
        .collect()
}

/// The cache folders of one main file's folder.
#[derive(Debug, Clone)]
pub struct Cache {
    pub dir: PathBuf,
    pub posters: PathBuf,
}

impl Cache {
    pub fn at(main_dir: &Path) -> Self {
        let dir = main_dir.join(CACHE_DIR);
        Cache {
            posters: dir.join(POSTERS_DIR),
            dir,
        }
    }

    /// A poster's file.
    pub fn png(&self, key: &str) -> PathBuf {
        self.posters.join(format!("{key}.png"))
    }

    fn map(&self, job: &str) -> PathBuf {
        self.posters.join(format!("{job}.{MAP_EXT}"))
    }

    /// Both folders exist as real folders (not links).
    pub fn present(&self) -> bool {
        [&self.dir, &self.posters].iter().all(|d| real_dir(d))
    }

    /// Creates the folders. A link or a file in their place is refused: the
    /// cache only ever writes inside the project's own folders.
    pub fn ensure(&self) -> Result<(), String> {
        for d in [&self.dir, &self.posters] {
            match std::fs::symlink_metadata(d) {
                Ok(m) if m.file_type().is_dir() => {}
                Ok(_) => {
                    return Err(format!(
                        "{} is not a folder; the poster cache is not written",
                        d.display()
                    ))
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(d)
                    .map_err(|e| format!("cannot create {}: {e}", d.display()))?,
                Err(e) => return Err(format!("cannot read {}: {e}", d.display())),
            }
        }
        Ok(())
    }
}

fn real_dir(d: &Path) -> bool {
    std::fs::symlink_metadata(d).is_ok_and(|m| m.file_type().is_dir())
}

/// What `.maleficium/README.md` says.
pub const README: &str = "# .maleficium

Maleficium writes this folder when it compiles a paper that uses
`maleficium-interactive.sty`. It holds auto-posters: still images of the
interactive model and chart widgets that have no `poster=` of their own,
of html widgets without one that you approved in View > Widgets, and of
custom-runtime widgets without one whose runtime you allowed there.

- `posters/<sha256>.png` is one poster. Its name is a hash of the widget's
  source files, its options, the runtime and the renderer version, so any
  change to the widget renders a new poster.
- `posters/<main>.map` tells the package which poster belongs to which
  widget of `<main>.tex`. A poster is used only while the widget's recorded
  options still match; otherwise the widget shows a placeholder.

It is safe to delete. The next compile in Maleficium renders the posters
again. A widget that is new in a compile gets its poster in the same
compile: the compile renders it after the engine's first pass and runs
the engine again, so the pdf shows the poster. A poster that cannot be
rendered stays a placeholder.

You may commit it. With the folder in place the paper compiles with its
posters anywhere the package is installed, without Maleficium; without
the folder those widgets show a placeholder box.

Cleanup: after each successful compile, posters that no widget of a
compiled main file uses are deleted. When the folder grows past its cap
(100 MiB unless MALEFICIUM_POSTER_CACHE_MIB sets another size in MiB),
the oldest posters the paper just compiled does not use go first.

Maleficium writes this README only when it is missing, so edits stay.
";

/// Writes the README when it is missing; an existing one (edited or not)
/// is never replaced. True when it was written.
pub fn write_readme(c: &Cache) -> Result<bool, String> {
    let path = c.dir.join(README_NAME);
    if std::fs::symlink_metadata(&path).is_ok() {
        return Ok(false);
    }
    std::fs::write(&path, README)
        .map(|_| true)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// One map line's content: the widget, its sidecar record, its poster key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    pub id: String,
    pub guard: String,
    pub key: String,
}

/// Whether a record can ride inside a TeX argument and read back as the
/// same characters: no escapes, groups, comments, parameters or `^^`.
fn tex_safe(s: &str) -> bool {
    !s.chars()
        .any(|c| matches!(c, '\\' | '{' | '}' | '%' | '#' | '^') || c.is_control())
}

/// Every run of white space as one space: TeX folds them when it reads the
/// map, and the package folds the widget's own record the same way.
fn fold_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The map file the package inputs: one `\mfw@postermap{id}{record}{key}`
/// per widget whose poster exists.
pub fn map_text(main_file: &str, entries: &[MapEntry]) -> String {
    let mut out = format!(
        "% Maleficium poster map for {main_file}: written by the app before and after\n\
         % each compile, read by maleficium-interactive.sty. Safe to delete.\n\
         % \\mfw@postermap{{<widget id>}}{{<runtime>|<sources>|<options>}}{{<poster sha256>}}\n"
    );
    for e in entries {
        out.push_str(&format!(
            "\\mfw@postermap{{{}}}{{{}}}{{{}}}\n",
            e.id, e.guard, e.key
        ));
    }
    out
}

fn is_key(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// The entries of a map file; lines that do not parse are skipped.
pub fn parse_map(text: &str) -> Vec<MapEntry> {
    text.lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("\\mfw@postermap{")?;
            let (id, rest) = rest.split_once("}{")?;
            let (guard, rest) = rest.split_once("}{")?;
            let key = rest.strip_suffix('}')?;
            is_key(key).then(|| MapEntry {
                id: id.to_string(),
                guard: guard.to_string(),
                key: key.to_string(),
            })
        })
        .collect()
}

/// Writes the map in one step, and only when its text changes.
fn write_map(c: &Cache, job: &str, main_file: &str, entries: &[MapEntry]) -> Result<(), String> {
    let path = c.map(job);
    let text = map_text(main_file, entries);
    if std::fs::read_to_string(&path).is_ok_and(|t| t == text) {
        return Ok(());
    }
    let tmp = path.with_extension("map.part");
    std::fs::write(&tmp, &text)
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("cannot write {}: {e}", path.display())
        })
}

/// What a garbage collection removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Collected {
    /// Posters no map or current widget used.
    pub orphans: Vec<String>,
    /// Posters evicted, oldest first, to get under the cap.
    pub evicted: Vec<String>,
    /// Bytes left in the cache.
    pub bytes: u64,
}

/// Deletes posters that neither `keep` (the compiled main file's widgets)
/// nor another main file's map uses, leftover partial writes, and maps
/// whose main file is gone; then, while the cache is over `cap`, the
/// oldest posters outside `keep`. Only `<sha256>.png` files are touched.
pub fn collect(
    c: &Cache,
    main_dir: &Path,
    job: &str,
    keep: &BTreeSet<String>,
    cap: u64,
) -> Result<Collected, String> {
    if !c.present() {
        return Ok(Collected::default());
    }
    let mut used: BTreeSet<String> = keep.clone();
    let mut posters: Vec<(String, u64, std::time::SystemTime)> = Vec::new();
    let entries = std::fs::read_dir(&c.posters)
        .map_err(|e| format!("cannot list {}: {e}", c.posters.display()))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(m) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !m.file_type().is_file() {
            continue;
        }
        if name.ends_with(".part") {
            let _ = std::fs::remove_file(e.path());
        } else if let Some(other) = name.strip_suffix(&format!(".{MAP_EXT}")) {
            if other == job {
                continue;
            }
            if main_dir.join(format!("{other}.tex")).is_file() {
                let text = std::fs::read_to_string(e.path()).unwrap_or_default();
                used.extend(parse_map(&text).into_iter().map(|m| m.key));
            } else {
                let _ = std::fs::remove_file(e.path());
            }
        } else if let Some(key) = name.strip_suffix(".png").filter(|k| is_key(k)) {
            posters.push((
                key.to_string(),
                m.len(),
                m.modified().unwrap_or(std::time::UNIX_EPOCH),
            ));
        }
    }
    let mut out = Collected::default();
    posters.retain(|(key, _, _)| {
        if used.contains(key) {
            return true;
        }
        let gone = std::fs::remove_file(c.png(key)).is_ok();
        if gone {
            out.orphans.push(key.clone());
        }
        !gone
    });
    let mut total: u64 = posters.iter().map(|p| p.1).sum();
    let mut old: Vec<&(String, u64, std::time::SystemTime)> =
        posters.iter().filter(|p| !keep.contains(&p.0)).collect();
    old.sort_by_key(|p| (p.2, p.0.clone()));
    for (key, len, _) in old {
        if total <= cap {
            break;
        }
        if std::fs::remove_file(c.png(key)).is_ok() {
            total -= len;
            out.evicted.push(key.clone());
        }
    }
    out.orphans.sort();
    out.bytes = total;
    Ok(out)
}

/// A widget whose poster the cache provides.
struct Wanted<'w> {
    widget: &'w Widget,
    key: String,
    png: PathBuf,
    /// An html widget's approval digest, or a custom widget's runtime
    /// digest, which the render must still match.
    digest: Option<String>,
}

/// The widgets the cache provides for, and the html widgets that wait for
/// the user's approval (they get no poster and no map entry). `paper` is
/// the compile's widget list and the theme its posters are keyed by.
fn wanted<'w>(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    paper: (&'w [Widget], &Theme),
    c: &Cache,
    say: &mut dyn FnMut(String),
) -> (Vec<Wanted<'w>>, Vec<ApprovalRequired>) {
    let mut want = Vec::new();
    let mut pending = Vec::new();
    let (list, theme) = paper;
    for w in list.iter().filter(|w| auto(w)) {
        match keyed(base, cx, root_id, main_rel, w, theme) {
            Ok(Keyed::Key(key, digest)) => want.push(Wanted {
                widget: w,
                png: c.png(&key),
                key,
                digest,
            }),
            Ok(Keyed::Approval(r)) => pending.push(*r),
            Err(e) => say(format!("poster {}: {e}", w.id)),
        }
    }
    (want, pending)
}

fn entries(
    want: &[Wanted],
    guards: &BTreeMap<String, String>,
    say: &mut dyn FnMut(String),
) -> Vec<MapEntry> {
    want.iter()
        .filter(|w| w.png.is_file())
        .filter_map(|w| {
            let guard = guards.get(&w.widget.id)?;
            if !tex_safe(guard) || !tex_safe(&w.widget.id) {
                say(format!(
                    "poster {}: its record holds characters TeX would read differently; it keeps the placeholder",
                    w.widget.id
                ));
                return None;
            }
            Some(MapEntry {
                id: w.widget.id.clone(),
                guard: fold_spaces(guard),
                key: w.key.clone(),
            })
        })
        .collect()
}

fn job_of(main_file: &str) -> &str {
    main_file.strip_suffix(".tex").unwrap_or(main_file)
}

/// The first pass: render the posters the previous compile's widgets lack
/// and write the map. Does nothing before a first compile (no widget list),
/// and creates the cache only when a widget needs it. Never fails the
/// compile: problems, and html widgets waiting for approval (each as an
/// `approval_required` line), are reported through `say`. Returns how many
/// posters it rendered: the compile runs the engine again when that is
/// more than zero after a compile, so the pdf shows them.
pub fn before_compile(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    say: &mut dyn FnMut(String),
) -> usize {
    before_compile_at(&widget_approval::store_base(), cx, root_id, main_rel, say)
}

/// [`before_compile`] against the approval store at `base`.
pub(crate) fn before_compile_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    say: &mut dyn FnMut(String),
) -> usize {
    before_compile_inner(base, cx, root_id, main_rel, say, None, None)
}

/// [`before_compile`] for a compile job: the renderer subprocess parks in
/// `slot` and stops on `cancelled`, so a cancel in the poster phase still
/// reaches the compile. Returns 0 once cancelled, so no second engine run
/// follows a cancelled poster phase.
pub fn before_compile_job(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    say: &mut dyn FnMut(String),
    slot: &Mutex<Option<Child>>,
    cancelled: &AtomicBool,
) -> usize {
    before_compile_inner(
        &widget_approval::store_base(),
        cx,
        root_id,
        main_rel,
        say,
        Some(slot),
        Some(cancelled),
    )
}

fn before_compile_inner(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    say: &mut dyn FnMut(String),
    slot: Option<&Mutex<Option<Child>>>,
    cancelled: Option<&AtomicBool>,
) -> usize {
    let Ok((list, theme)) = read(cx, root_id, main_rel) else {
        return 0;
    };
    let Ok(o) = crate::outputs::outputs_of(cx, root_id, main_rel) else {
        return 0;
    };
    let c = Cache::at(&o.dir);
    let (want, pending) = wanted(
        base,
        cx,
        root_id,
        main_rel,
        (&list.widgets, &theme),
        &c,
        say,
    );
    for r in &pending {
        say(approval_line(r));
    }
    if want.is_empty() && !c.present() {
        return 0;
    }
    if let Err(e) = c.ensure() {
        say(format!("posters: {e}"));
        return 0;
    }
    if let Err(e) = write_readme(&c) {
        say(format!("posters: {e}"));
    }
    let missing: Vec<&Wanted> = want
        .iter()
        .filter(|w| poster_source(w.widget, Some(&w.png)) == PosterSource::Placeholder)
        .collect();
    let mut rendered = 0;
    if !missing.is_empty() {
        let reqs: Vec<PosterRequest> = missing
            .iter()
            .map(|w| PosterRequest {
                root_id: root_id.to_string(),
                main_rel: main_rel.to_string(),
                widget_id: w.widget.id.clone(),
                out_path: w.png.to_string_lossy().into_owned(),
                timeout_ms: w.digest.as_ref().map(|_| {
                    if w.widget.kind == WidgetType::Custom {
                        CUSTOM_TIMEOUT_MS
                    } else {
                        HTML_TIMEOUT_MS
                    }
                }),
                digest: w.digest.clone(),
            })
            .collect();
        match renderer(cx) {
            None => say(format!(
                "posters: {} not rendered: no poster renderer beside this program; placeholders stay",
                reqs.len()
            )),
            Some(r) => {
                let results = match (slot, cancelled) {
                    (Some(slot), Some(cancelled)) => r.render_cancel(cx, &reqs, slot, cancelled),
                    _ => r.render(cx, &reqs),
                };
                for (w, res) in missing.iter().zip(results) {
                    match res {
                        Ok(PosterOutcome::Rendered(p)) if Path::new(&p.path) == w.png => {
                            rendered += 1;
                            say(format!(
                                "poster {}: rendered {}x{}",
                                w.widget.id, p.width, p.height
                            ));
                        }
                        Ok(PosterOutcome::Rendered(p)) => say(format!(
                            "poster {}: the renderer wrote {} instead of the cache; ignored",
                            w.widget.id, p.path
                        )),
                        Ok(PosterOutcome::ApprovalRequired(r)) => say(approval_line(&r)),
                        Err(e) => say(format!("poster {}: {e}", w.widget.id)),
                    }
                }
            }
        }
    }
    // A cancelled poster phase renders no second engine run, even when some
    // posters finished before the cancel.
    if cancelled.is_some_and(|c| c.load(Ordering::SeqCst)) {
        return 0;
    }
    let guards = sidecar_guards(cx, root_id, main_rel).unwrap_or_default();
    let map = entries(&want, &guards, say);
    if let Err(e) = write_map(&c, job_of(&o.main_file), &o.main_file, &map) {
        say(format!("posters: {e}"));
    }
    let waiting = if pending.is_empty() {
        String::new()
    } else {
        format!(", {} awaiting approval", pending.len())
    };
    say(format!(
        "posters: {} cached, {rendered} rendered, {} placeholder{waiting}",
        want.len() - missing.len(),
        want.len() - map.len()
    ));
    rendered
}

/// The second pass, after a successful compile: rewrite the map from the
/// new widget list and collect garbage. Does nothing when the cache does
/// not exist.
pub fn after_compile(cx: &Core, root_id: &str, main_rel: &str, say: &mut dyn FnMut(String)) {
    after_compile_at(&widget_approval::store_base(), cx, root_id, main_rel, say)
}

/// [`after_compile`] against the approval store at `base`.
pub(crate) fn after_compile_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    say: &mut dyn FnMut(String),
) {
    let Ok(o) = crate::outputs::outputs_of(cx, root_id, main_rel) else {
        return;
    };
    let c = Cache::at(&o.dir);
    if !c.present() {
        return;
    }
    let Ok((list, theme)) = read(cx, root_id, main_rel) else {
        return;
    };
    let (want, _) = wanted(
        base,
        cx,
        root_id,
        main_rel,
        (&list.widgets, &theme),
        &c,
        say,
    );
    let guards = sidecar_guards(cx, root_id, main_rel).unwrap_or_default();
    let map = entries(&want, &guards, say);
    let job = job_of(&o.main_file);
    if let Err(e) = write_map(&c, job, &o.main_file, &map) {
        say(format!("posters: {e}"));
    }
    let keep: BTreeSet<String> = want.iter().map(|w| w.key.clone()).collect();
    let cap = cap_bytes();
    match collect(&c, &o.dir, job, &keep, cap) {
        Ok(g) => {
            if !g.orphans.is_empty() || !g.evicted.is_empty() {
                say(format!(
                    "posters: removed {} unused, {} over the cap",
                    g.orphans.len(),
                    g.evicted.len()
                ));
            }
            if g.bytes > cap {
                say(format!(
                    "posters: this paper's posters alone ({} bytes) exceed the cache cap ({cap} bytes)",
                    g.bytes
                ));
            }
        }
        Err(e) => say(format!("posters: {e}")),
    }
}

/// The poster file of a widget the cache provides, when it exists: for the
/// bundle export and anything else that wants the PDF's picture. An html
/// widget's only while it is approved as it is now.
pub fn cached_poster(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    w: &Widget,
    theme: &Theme,
) -> Option<PathBuf> {
    cached_poster_at(
        &widget_approval::store_base(),
        cx,
        root_id,
        main_rel,
        w,
        theme,
    )
}

/// [`cached_poster`] against the approval store at `base`.
pub(crate) fn cached_poster_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    w: &Widget,
    theme: &Theme,
) -> Option<PathBuf> {
    if !auto(w) {
        return None;
    }
    let Keyed::Key(key, _) = keyed(base, cx, root_id, main_rel, w, theme).ok()? else {
        return None;
    };
    let o = crate::outputs::outputs_of(cx, root_id, main_rel).ok()?;
    let c = Cache::at(&o.dir);
    let png = c.png(&key);
    (c.present() && png.is_file()).then_some(png)
}

/// The map's key for one widget id, when the map names it.
pub(crate) fn map_key_for(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    widget_id: &str,
) -> Option<String> {
    let o = crate::outputs::outputs_of(cx, root_id, main_rel).ok()?;
    let c = Cache::at(&o.dir);
    let text = std::fs::read_to_string(c.map(job_of(&o.main_file))).ok()?;
    parse_map(&text)
        .into_iter()
        .find(|e| e.id == widget_id)
        .map(|e| e.key)
}

/// The current cache key of a custom widget, or None when it has none
/// (an explicit poster, an unapproved runtime, a missing or invalid
/// package, or a bind error).
pub(crate) fn custom_key_now_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    w: &Widget,
    theme: &Theme,
) -> Option<(String, String)> {
    let Keyed::Key(key, Some(digest)) = custom_keyed(base, cx, root_id, main_rel, w, theme).ok()?
    else {
        return None;
    };
    Some((key, digest))
}

/// Runs the app binary's headless renderer (`maleficium --render-posters
/// <root>`): requests as JSON lines on its stdin, results on its stdout.
pub struct ProcessRenderer {
    exe: PathBuf,
}

impl ProcessRenderer {
    /// The app binary: this program when it is the app (`maleficium --mcp`),
    /// else `maleficium` beside it (the MCP sidecar's install layout).
    pub fn find() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let name = if cfg!(windows) {
            "maleficium.exe"
        } else {
            "maleficium"
        };
        let app = if exe.file_name().is_some_and(|n| n == name) {
            exe
        } else {
            exe.parent()?.join(name)
        };
        app.is_file().then_some(ProcessRenderer { exe: app })
    }
}

/// Whether a window can open here. Linux needs a display server; the
/// renderer would fail at startup without one.
fn display_available() -> bool {
    if cfg!(target_os = "linux") {
        ["DISPLAY", "WAYLAND_DISPLAY"]
            .iter()
            .any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()))
    } else {
        true
    }
}

impl PosterRenderer for ProcessRenderer {
    fn render(&self, cx: &Core, reqs: &[PosterRequest]) -> Vec<Result<PosterOutcome, String>> {
        let slot = Mutex::new(None);
        let cancelled = AtomicBool::new(false);
        self.render_cancel(cx, reqs, &slot, &cancelled)
    }

    fn render_cancel(
        &self,
        cx: &Core,
        reqs: &[PosterRequest],
        slot: &Mutex<Option<Child>>,
        cancelled: &AtomicBool,
    ) -> Vec<Result<PosterOutcome, String>> {
        let all = |e: String| -> Vec<Result<PosterOutcome, String>> {
            reqs.iter().map(|_| Err(e.clone())).collect()
        };
        if reqs.is_empty() {
            return Vec::new();
        }
        if !display_available() {
            return all(String::from("no display for the poster renderer"));
        }
        let root = match crate::fs::session_root(cx, &reqs[0].root_id) {
            Ok(r) => r,
            Err(e) => return all(e),
        };
        run_process(&self.exe, &root, reqs, slot, cancelled).unwrap_or_else(all)
    }
}

fn run_process(
    exe: &Path,
    root: &Path,
    reqs: &[PosterRequest],
    slot: &Mutex<Option<Child>>,
    cancelled: &AtomicBool,
) -> Result<Vec<Result<PosterOutcome, String>>, String> {
    use std::io::{BufRead, Write};
    use std::process::Stdio;
    let mut child = crate::quiet_command(exe)
        .arg("--render-posters")
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start the poster renderer: {e}"))?;
    let mut input = String::new();
    for r in reqs {
        input.push_str(&serde_json::to_string(r).map_err(|e| e.to_string())?);
        input.push('\n');
    }
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let stdout = child
        .stdout
        .take()
        .ok_or("the poster renderer has no output")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let budget: u64 = reqs
        .iter()
        .map(|r| r.timeout_ms.unwrap_or(super::DEFAULT_TIMEOUT_MS))
        .sum::<u64>()
        + PROCESS_SLACK_MS;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(budget);
    // The renderer parks in the compile's child slot: a cancel between
    // engine runs kills it like an engine child. Plain kill and wait, as
    // for the engine.
    slot.lock().unwrap().replace(child);
    let mut out = Vec::with_capacity(reqs.len());
    let mut stopped = false;
    while out.len() < reqs.len() {
        if cancelled.load(Ordering::SeqCst) {
            stopped = true;
            break;
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            break;
        }
        match rx.recv_timeout(left.min(std::time::Duration::from_millis(50))) {
            Ok(line) => {
                let v: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
                out.push(if v["ok"] == serde_json::Value::Bool(true) {
                    serde_json::from_value::<PosterOutcome>(v["result"].clone())
                        .map_err(|e| format!("the poster renderer answered oddly: {e}"))
                } else {
                    Err(v["error"]
                        .as_str()
                        .unwrap_or("the poster renderer failed")
                        .to_string())
                });
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // A cancel takes the child from the slot and reaps it; otherwise it is
    // still parked here.
    if let Some(mut parked) = slot.lock().unwrap().take() {
        let _ = parked.kill();
        let _ = parked.wait();
    }
    let unanswered = if stopped || cancelled.load(Ordering::SeqCst) {
        "compile cancelled"
    } else {
        "the poster renderer stopped before answering"
    };
    while out.len() < reqs.len() {
        out.push(Err(String::from(unanswered)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
