//! Static checks over a custom widget runtime package (lane C).
//!
//! The sandbox content security policy enforces the real boundary at
//! runtime; these checks catch honest mistakes early, so they stay simple
//! regex scans, not parsers. Every known gap is noted on the rule that
//! carries it. The interface is frozen by the custom-runtimes contract:
//! lane A calls [`scan`] with the package files, the vendored paths, and
//! whether the manifest declared `capabilities.wasm` (undeclared
//! WebAssembly use is a `wasm` error).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[cfg(test)]
mod tests;

pub struct Finding {
    pub rule: &'static str,
    pub file: String,
    pub line: u32,
    pub message: String,
    pub hint: String,
}

pub struct Report {
    pub errors: Vec<Finding>,
    pub warnings: Vec<Finding>,
}

/// Package metadata, never folded into the widget document: `runtime.json`,
/// everything under `samples/`, and any readme/licence file (top level or
/// a vendored `licenseFile`). Metadata is not scanned and never
/// `unreferenced`.
fn is_metadata(path: &str) -> bool {
    if path == "runtime.json" || path.starts_with("samples/") {
        return true;
    }
    let base = path.rsplit('/').next().unwrap_or(path);
    base.starts_with("README") || base.starts_with("LICENSE") || base.starts_with("LICENCE")
}

/// Packages over this many bytes earn a `size` warning.
const SIZE_LIMIT: usize = 5 * 1024 * 1024;

fn re<'a>(cell: &'a OnceLock<regex::Regex>, pattern: &str) -> &'a regex::Regex {
    cell.get_or_init(|| regex::Regex::new(pattern).expect("a static pattern compiles"))
}

/// 1-based line number of the byte offset in the text.
fn line_of(text: &str, offset: usize) -> u32 {
    text[..offset].bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

/// An absolute remote URL: any scheme plus `://`, or protocol-relative.
fn is_absolute_url(value: &str) -> bool {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    re(&CELL, r"^(?:[a-zA-Z][a-zA-Z0-9+.-]*://|//)").is_match(value.trim())
}

/// Values that never name a package file: fragments, `data:`/`blob:`
/// payloads, and non-http schemes.
fn is_non_ref(value: &str) -> bool {
    let v = value.trim();
    v.is_empty()
        || v.starts_with('#')
        || v.starts_with("data:")
        || v.starts_with("blob:")
        || v.starts_with("mailto:")
        || v.starts_with("tel:")
        || v.starts_with("javascript:")
}

/// Resolve a relative reference against the referencing file's directory.
/// A leading `/` counts as package-root relative. Returns `None` when
/// `..` escapes the package root. Gap: query strings and fragments stay
/// part of the path, so `other.html?v=2` reports as missing.
fn resolve_ref(from: &str, href: &str) -> Option<String> {
    let href = href.trim();
    let href = href.strip_prefix('/').unwrap_or(href);
    let mut parts: Vec<&str> = Vec::new();
    if !href.starts_with("./") && !href.starts_with("../") && from.contains('/') {
        let dir = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        if !dir.is_empty() {
            parts.extend(dir.split('/'));
        }
    }
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

fn finding(rule: &'static str, file: &str, line: u32, message: String, hint: &str) -> Finding {
    Finding {
        rule,
        file: file.to_string(),
        line,
        message,
        hint: hint.to_string(),
    }
}

/// One remote URL load: an error in first-party code, a `vendored-url`
/// warning in vendored code. Gap: only quoted strings are seen, so
/// template literals and split (`"https:" + "//..."`) strings are missed;
/// quoted prose that merely mentions a URL is flagged.
fn check_js_string_urls(text: &str, base_line: u32, out: &mut Vec<(u32, String)>) {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    let pattern = re(
        &CELL,
        r#"['"]((?:[a-zA-Z][a-zA-Z0-9+.-]*://|//)[^'"]*)['"]"#,
    );
    for m in pattern.captures_iter(text) {
        let url = m.get(1).map(|g| g.as_str()).unwrap_or("");
        let line = base_line + line_of(text, m.get(0).map(|g| g.start()).unwrap_or(0)) - 1;
        out.push((line, url.to_string()));
    }
}

/// Remote loads in HTML attributes: `src href srcset poster data
/// action`. Returns loaded URLs and relative references as
/// `(line, url_or_href, is_url)`. Gap: unquoted values ending in `/`
/// (self-closed tags) keep the slash; `longdesc` and SVG `xlink:href`
/// are not watched.
fn scan_html_attrs(text: &str) -> Vec<(u32, String, bool)> {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    let pattern = re(
        &CELL,
        r#"(?i)[\s>](src|href|srcset|poster|data|action)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#,
    );
    let mut out: Vec<(u32, String, bool)> = Vec::new();
    for c in pattern.captures_iter(text) {
        let name = c
            .get(1)
            .map(|g| g.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        let value = c
            .get(2)
            .or_else(|| c.get(3))
            .or_else(|| c.get(4))
            .map(|g| g.as_str())
            .unwrap_or("");
        let line = line_of(text, c.get(0).map(|g| g.start()).unwrap_or(0));
        let mut values: Vec<String> = Vec::new();
        if name == "srcset" {
            for item in value.split(',') {
                if let Some(first) = item.split_whitespace().next() {
                    values.push(first.to_string());
                }
            }
        } else {
            values.push(value.to_string());
        }
        for v in values {
            if is_absolute_url(&v) {
                out.push((line, v, true));
            } else if !is_non_ref(&v) {
                out.push((line, v, false));
            }
        }
    }
    out
}

/// Remote loads and references in CSS: `url()` and `@import`. Gap: HTML
/// `<style>` bodies are scanned by the caller; `@use` is not watched.
fn scan_css_urls(text: &str, base_line: u32) -> Vec<(u32, String, bool)> {
    static URL: OnceLock<regex::Regex> = OnceLock::new();
    static IMPORT: OnceLock<regex::Regex> = OnceLock::new();
    let url = re(
        &URL,
        r#"(?i)url\(\s*(?:"([^"]*)"|'([^']*)'|([^)'"\s]+))\s*\)"#,
    );
    let import = re(&IMPORT, r#"(?i)@import\s+("[^"]*"|'[^']*')"#);
    let mut out = Vec::new();
    let mut push = |raw: &str, start: usize| {
        let v = raw.trim().trim_matches(|c| c == '"' || c == '\'');
        let line = base_line + line_of(text, start) - 1;
        if is_absolute_url(v) {
            out.push((line, v.to_string(), true));
        } else if !is_non_ref(v) {
            out.push((line, v.to_string(), false));
        }
    };
    for c in url.captures_iter(text) {
        let v = c
            .get(1)
            .or_else(|| c.get(2))
            .or_else(|| c.get(3))
            .map(|g| g.as_str())
            .unwrap_or("");
        push(v, c.get(0).map(|g| g.start()).unwrap_or(0));
    }
    for c in import.captures_iter(text) {
        let v = c.get(1).map(|g| g.as_str()).unwrap_or("");
        push(v, c.get(0).map(|g| g.start()).unwrap_or(0));
    }
    out
}

/// Relative module references in JS (`from "..."`, `import("...")`).
/// Absolute URLs are left to the string scan above.
fn scan_js_refs(text: &str, base_line: u32) -> Vec<(u32, String)> {
    static FROM: OnceLock<regex::Regex> = OnceLock::new();
    static DYNAMIC: OnceLock<regex::Regex> = OnceLock::new();
    let from = re(
        &FROM,
        r#"(?m)\b(?:import\s+(?:[^;'"]*?\bfrom\s+)?|export\s+[^;]*?\bfrom\s+)(['"])([^'"]+)(['"])"#,
    );
    let dynamic = re(&DYNAMIC, r#"\bimport\s*\(\s*(['"])([^'"]+)(['"])"#);
    let mut out = Vec::new();
    for c in from.captures_iter(text).chain(dynamic.captures_iter(text)) {
        // No backreferences in this regex engine: skip mismatched quotes.
        let open = c.get(1).map(|g| g.as_str()).unwrap_or("");
        let close = c.get(3).map(|g| g.as_str()).unwrap_or("");
        if open != close {
            continue;
        }
        let v = c.get(2).map(|g| g.as_str()).unwrap_or("");
        if !is_absolute_url(v) && !is_non_ref(v) {
            let line = base_line + line_of(text, c.get(0).map(|g| g.start()).unwrap_or(0)) - 1;
            out.push((line, v.to_string()));
        }
    }
    out
}

/// Inline `<script>` bodies as `(attributes, body, body_start_offset)`.
/// Gap: a literal `</script>` inside a script string ends the block early.
fn script_blocks(text: &str) -> Vec<(String, String, usize)> {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    let pattern = re(
        &CELL,
        r"(?is)<script(?P<attrs>[^>]*)>(?P<body>.*?)</script\s*>",
    );
    pattern
        .captures_iter(text)
        .map(|c| {
            (
                c.name("attrs")
                    .map(|g| g.as_str())
                    .unwrap_or("")
                    .to_string(),
                c.name("body").map(|g| g.as_str()).unwrap_or("").to_string(),
                c.name("body").map(|g| g.start()).unwrap_or(0),
            )
        })
        .collect()
}

/// Inline `<style>` bodies as `(body, body_start_offset)`.
fn style_blocks(text: &str) -> Vec<(String, usize)> {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    let pattern = re(&CELL, r"(?is)<style[^>]*>(?P<body>.*?)</style\s*>");
    pattern
        .captures_iter(text)
        .map(|c| {
            (
                c.name("body").map(|g| g.as_str()).unwrap_or("").to_string(),
                c.name("body").map(|g| g.start()).unwrap_or(0),
            )
        })
        .collect()
}

fn has_src_attr(attrs: &str) -> bool {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    re(&CELL, r"(?i)\bsrc\s*=").is_match(attrs)
}

/// One `(pattern, rule, message)` scan over JS text. Each pattern carries
/// its own gap: matches inside comments and strings are flagged, and
/// renamed (`window["local" + "Storage"]`) uses are missed.
struct JsRule {
    pattern: &'static str,
    rule: &'static str,
    message: &'static str,
    hint: &'static str,
    cell: OnceLock<regex::Regex>,
}

impl JsRule {
    const fn new(
        pattern: &'static str,
        rule: &'static str,
        message: &'static str,
        hint: &'static str,
    ) -> Self {
        Self {
            pattern,
            rule,
            message,
            hint,
            cell: OnceLock::new(),
        }
    }
}

fn js_rules() -> Vec<JsRule> {
    vec![
        JsRule::new(
            r"\beval\s*\(",
            "eval",
            "uses eval(); the sandbox blocks it",
            "parse data with JSON.parse or a small reader instead",
        ),
        JsRule::new(
            r"\bnew\s+Function\s*\(",
            "eval",
            "uses new Function(); the sandbox blocks it",
            "write a plain function instead",
        ),
        JsRule::new(
            r#"\bsetTimeout\s*\(\s*['"`]"#,
            "eval",
            "passes a string to setTimeout(); the sandbox treats it as eval",
            "pass a function instead of a string",
        ),
        JsRule::new(
            r#"\bsetInterval\s*\(\s*['"`]"#,
            "eval",
            "passes a string to setInterval(); the sandbox treats it as eval",
            "pass a function instead of a string",
        ),
        JsRule::new(
            r"(?m)^\s*import\b",
            "module-import",
            "uses a module import; runtimes are classic scripts",
            "use a classic <script> with no import or export",
        ),
        JsRule::new(
            r"\bimport\s*\(",
            "module-import",
            "uses a dynamic import(); runtimes are classic scripts",
            "use a classic <script> with no import or export",
        ),
        JsRule::new(
            r"(?m)(?:^|[;{}])\s*export\b",
            "module-import",
            "uses a module export; runtimes are classic scripts",
            "use a classic <script> with no import or export",
        ),
        JsRule::new(
            r"\bfetch\s*\(",
            "network-api",
            "uses fetch(); the sandbox allows no network",
            "take source bytes from the init message instead",
        ),
        JsRule::new(
            r"\bXMLHttpRequest\b",
            "network-api",
            "uses XMLHttpRequest; the sandbox allows no network",
            "take source bytes from the init message instead",
        ),
        JsRule::new(
            r"\bWebSocket\b",
            "network-api",
            "uses WebSocket; the sandbox allows no network",
            "take source bytes from the init message instead",
        ),
        JsRule::new(
            r"\bEventSource\b",
            "network-api",
            "uses EventSource; the sandbox allows no network",
            "take source bytes from the init message instead",
        ),
        JsRule::new(
            r"\bnew\s+Worker\s*\(",
            "worker",
            "uses new Worker(); the sandbox allows no workers",
            "do the work on the main thread instead",
        ),
        JsRule::new(
            r"\bimportScripts\s*\(",
            "worker",
            "uses importScripts(); the sandbox allows no workers",
            "inline the script instead",
        ),
        JsRule::new(
            r"\bSharedWorker\b",
            "worker",
            "uses SharedWorker; the sandbox allows no workers",
            "do the work on the main thread instead",
        ),
        JsRule::new(
            r"\bserviceWorker\b",
            "worker",
            "uses a service worker; the sandbox allows no workers",
            "do the work on the main thread instead",
        ),
        JsRule::new(
            r"\blocalStorage\b",
            "storage",
            "uses localStorage; the sandbox keeps no storage",
            "keep state in memory instead",
        ),
        JsRule::new(
            r"\bsessionStorage\b",
            "storage",
            "uses sessionStorage; the sandbox keeps no storage",
            "keep state in memory instead",
        ),
        JsRule::new(
            r"\bindexedDB\b",
            "storage",
            "uses indexedDB; the sandbox keeps no storage",
            "keep state in memory instead",
        ),
        JsRule::new(
            r"\bdocument\s*\.\s*cookie\b",
            "storage",
            "uses document.cookie; the sandbox keeps no storage",
            "keep state in memory instead",
        ),
        JsRule::new(
            r"\bWebAssembly\b",
            "wasm",
            "uses WebAssembly; the runtime must declare capabilities.wasm",
            "declare `\"wasm\": true` under `capabilities` in runtime.json",
        ),
        JsRule::new(
            r"\.wasm\b",
            "wasm",
            "references a .wasm module; the runtime must declare capabilities.wasm",
            "declare `\"wasm\": true` under `capabilities` in runtime.json",
        ),
        JsRule::new(
            r"window\s*\.\s*__[A-Za-z0-9_$]*",
            "global-hook",
            "assigns a window.__ hook; runtimes must not touch shared globals",
            "keep state in a closure instead",
        ),
        JsRule::new(
            r#"window\s*\[\s*['"]__"#,
            "global-hook",
            "assigns a window.__ hook; runtimes must not touch shared globals",
            "keep state in a closure instead",
        ),
    ]
}

/// Run the JS content rules over one script text. `base_line` is the
/// 1-based line the text starts on. URL loads go to `urls`, references
/// to `refs`, the rest to findings (errors, except `global-hook`; the
/// `wasm` rule stays silent when the runtime declared `capabilities.wasm`).
#[allow(clippy::too_many_arguments)]
fn scan_script(
    file: &str,
    text: &str,
    base_line: u32,
    wasm: bool,
    errors: &mut Vec<Finding>,
    warnings: &mut Vec<Finding>,
    urls: &mut Vec<(u32, String)>,
    refs: &mut Vec<(String, u32, String)>,
    rules: &[JsRule],
) {
    let at = |offset: usize| base_line + line_of(text, offset) - 1;
    let mut url_list = Vec::new();
    check_js_string_urls(text, base_line, &mut url_list);
    for (line, url) in url_list {
        urls.push((line, url));
    }
    for (line, href) in scan_js_refs(text, base_line) {
        refs.push((file.to_string(), line, href));
    }
    for rule in rules {
        if wasm && rule.rule == "wasm" {
            continue;
        }
        let pattern = re(&rule.cell, rule.pattern);
        for m in pattern.find_iter(text) {
            let f = finding(
                rule.rule,
                file,
                at(m.start()),
                rule.message.to_string(),
                rule.hint,
            );
            if rule.rule == "global-hook" {
                warnings.push(f);
            } else {
                errors.push(f);
            }
        }
    }
}

/// Whether a relative reference names a WebAssembly module: its path
/// part (before any query or fragment) ends in `.wasm`.
fn is_wasm_ref(href: &str) -> bool {
    let path = href.split(['?', '#']).next().unwrap_or(href);
    path.to_ascii_lowercase().ends_with(".wasm")
}

pub fn scan(files: &BTreeMap<String, Vec<u8>>, vendored: &BTreeSet<String>, wasm: bool) -> Report {
    let mut errors: Vec<Finding> = Vec::new();
    let mut warnings: Vec<Finding> = Vec::new();
    // (referencing file, line, raw href) for `missing-ref`;
    // (line, url) collected per file for `url-load` / `vendored-url`.
    let mut refs: Vec<(String, u32, String)> = Vec::new();
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    let rules = js_rules();

    static META: OnceLock<regex::Regex> = OnceLock::new();
    let meta_refresh = re(
        &META,
        r#"(?i)<meta\b[^>]*?http-equiv\s*=\s*["']?\s*refresh"#,
    );

    for (path, bytes) in files {
        if is_metadata(path) {
            continue;
        }
        // Gap: non-UTF-8 files scan through a lossy view; offsets still
        // line up because every check runs on this same text.
        let text = String::from_utf8_lossy(bytes);
        let is_vendored = vendored.contains(path);
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let mut urls: Vec<(u32, String)> = Vec::new();

        match ext.as_str() {
            "html" | "htm" => {
                for (line, value, is_url) in scan_html_attrs(&text) {
                    if is_url {
                        urls.push((line, value));
                    } else {
                        refs.push((path.clone(), line, value));
                    }
                }
                for (body, start) in style_blocks(&text) {
                    let base = line_of(&text, start);
                    for (line, value, is_url) in scan_css_urls(&body, base) {
                        if is_url {
                            urls.push((line, value));
                        } else {
                            refs.push((path.clone(), line, value));
                        }
                    }
                }
                if !is_vendored {
                    for m in meta_refresh.find_iter(&text) {
                        errors.push(finding(
                            "meta-refresh",
                            path,
                            line_of(&text, m.start()),
                            "uses <meta http-equiv=\"refresh\">; the sandbox page never reloads"
                                .to_string(),
                            "remove the tag",
                        ));
                    }
                }
                for (attrs, body, start) in script_blocks(&text) {
                    if has_src_attr(&attrs) {
                        continue;
                    }
                    if is_vendored {
                        let base = line_of(&text, start);
                        let mut inline_urls = Vec::new();
                        check_js_string_urls(&body, base, &mut inline_urls);
                        urls.extend(inline_urls);
                    } else {
                        let base = line_of(&text, start);
                        scan_script(
                            path,
                            &body,
                            base,
                            wasm,
                            &mut errors,
                            &mut warnings,
                            &mut urls,
                            &mut refs,
                            &rules,
                        );
                    }
                }
            }
            "css" => {
                for (line, value, is_url) in scan_css_urls(&text, 1) {
                    if is_url {
                        urls.push((line, value));
                    } else if !is_vendored {
                        refs.push((path.clone(), line, value));
                    }
                }
            }
            "js" | "mjs" | "cjs" => {
                if is_vendored {
                    let mut inline_urls = Vec::new();
                    check_js_string_urls(&text, 1, &mut inline_urls);
                    urls.extend(inline_urls);
                } else {
                    scan_script(
                        path,
                        &text,
                        1,
                        wasm,
                        &mut errors,
                        &mut warnings,
                        &mut urls,
                        &mut refs,
                        &rules,
                    );
                }
            }
            _ => {}
        }

        for (line, url) in urls {
            if is_vendored {
                warnings.push(finding(
                    "vendored-url",
                    path,
                    line,
                    format!("loads a remote URL ({url}); even vendored code runs offline"),
                    "vendor the file itself under vendor/ or inline it",
                ));
            } else {
                errors.push(finding(
                    "url-load",
                    path,
                    line,
                    format!("loads a remote URL ({url}); runtimes must be self-contained"),
                    "vendor the file under vendor/ or inline it",
                ));
            }
        }
    }

    for (from, line, href) in &refs {
        // A WebAssembly module reference without the declaration is
        // refused even when the file exists; the dangling check below
        // still applies on top.
        if !wasm && is_wasm_ref(href) {
            errors.push(finding(
                "wasm",
                from,
                *line,
                format!("references \"{href}\", a WebAssembly module the runtime did not declare"),
                "declare `\"wasm\": true` under `capabilities` in runtime.json",
            ));
        }
        match resolve_ref(from, href) {
            Some(target) if files.contains_key(&target) => {
                referenced.insert(target);
            }
            _ => {
                errors.push(finding(
                    "missing-ref",
                    from,
                    *line,
                    format!("references \"{href}\", which is no file in the package"),
                    "add the file or fix the path",
                ));
            }
        }
    }

    if files.contains_key("index.html") {
        referenced.insert("index.html".to_string());
    }
    for path in files.keys() {
        if is_metadata(path) || vendored.contains(path) || referenced.contains(path) {
            continue;
        }
        warnings.push(finding(
            "unreferenced",
            path,
            1,
            "is folded into the widget but nothing references it".to_string(),
            "remove it or reference it from index.html",
        ));
    }

    let total: usize = files.values().map(|b| b.len()).sum();
    if total > SIZE_LIMIT {
        warnings.push(finding(
            "size",
            "package",
            1,
            format!("the package is {total} bytes, over the 5 MiB budget"),
            "shrink vendored code or assets",
        ));
    }

    errors.sort_by(|a, b| (&a.file, a.line, a.rule).cmp(&(&b.file, b.line, b.rule)));
    warnings.sort_by(|a, b| (&a.file, a.line, a.rule).cmp(&(&b.file, b.line, b.rule)));
    // One line can match a rule twice (e.g. two fetch( calls); keep one.
    errors.dedup_by(|a, b| {
        a.rule == b.rule && a.file == b.file && a.line == b.line && a.message == b.message
    });
    warnings.dedup_by(|a, b| {
        a.rule == b.rule && a.file == b.file && a.line == b.line && a.message == b.message
    });
    Report { errors, warnings }
}
