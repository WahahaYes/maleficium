//! The reader of an exported bundle: `index.html`, a companion page with
//! the paper's title and metadata, a link to and embed of `paper.pdf`, and
//! each widget in manifest order as a sandboxed frame with its poster as the
//! pre-load and no-script fallback. One small static page: inline css and
//! script, no CDN, no request beyond the bundle's own files.
//!
//! Frames are `sandbox="allow-scripts"` and nothing else, never
//! `allow-same-origin` (the 2026-09-30 browser export spike). A single-file
//! bundle mounts each folded widget document as `srcdoc`; a folder bundle
//! points the frame at `widgets/<id>/index.html` (http or https only: over
//! file:// the page says so instead of showing a broken view).

use super::fold;
use base64::Engine;

const CSS: &str = include_str!("reader.css");
const JS: &str = include_str!("reader.js");

/// One widget as the page shows it.
pub(super) struct ReaderWidget<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    /// The rendered figure number or label, when the document gave one.
    pub figure: Option<&'a str>,
    pub label: Option<&'a str>,
    pub alt: &'a str,
    /// The widget's rect on its pdf page, in points.
    pub width: f64,
    pub height: f64,
    /// An `<img src>`: a bundled path or a `data:` url.
    pub poster: String,
}

pub(super) struct Reader<'a> {
    pub title: &'a str,
    pub authors: &'a [String],
    pub abstract_text: Option<&'a str>,
    pub folder: bool,
    pub widgets: Vec<ReaderWidget<'a>>,
    /// The pdf bytes of a single-file bundle (a folder links `paper.pdf`).
    pub pdf: Option<&'a [u8]>,
    /// The `<script type="application/json">` islands the page reads.
    pub islands: &'a str,
    /// Every frame origin a widget declared, sorted and deduplicated.
    pub frames: &'a [String],
}

/// The reader page's policy. A single-file widget is a `srcdoc` document,
/// which inherits this policy, so a frame it embeds must pass the reader's
/// `frame-src` as well as its own: that directive is exactly the union of
/// the declared frame origins, and absent (`default-src 'none'`) when none
/// is declared. Each widget's own policy still names only its origins, and
/// `reader.js` mounts each widget inside a wrapper document whose
/// `frame-src` names only that widget's, so a widget cannot navigate its own
/// frame to another widget's origin. A folder widget is a document of its own and does not inherit, so the
/// folder reader stays `frame-src 'self'`.
pub(super) fn policy(folder: bool, frames: &[String]) -> String {
    if folder {
        return fold::FOLDER_READER_POLICY.to_string();
    }
    if frames.is_empty() {
        return fold::SINGLE_FILE_READER_POLICY.to_string();
    }
    format!(
        "{}; frame-src {}",
        fold::SINGLE_FILE_READER_POLICY,
        frames.join(" ")
    )
}

/// The token names the house theme defines, so the page hands a widget
/// exactly the contract's values.
fn token_names(css: &str) -> Vec<String> {
    let first = css.split('}').next().unwrap_or("");
    let re = regex::Regex::new(r"(--m-[a-z0-9-]+)\s*:").unwrap();
    re.captures_iter(first).map(|c| c[1].to_string()).collect()
}

pub(super) fn render(r: &Reader, theme_css: &str) -> String {
    let policy = policy(r.folder, r.frames);
    let by = r
        .authors
        .iter()
        .filter(|a| !a.is_empty())
        .map(|a| fold::text(a))
        .collect::<Vec<_>>()
        .join(", ");
    let href = match r.pdf {
        Some(b) if !r.folder => format!(
            "data:application/pdf;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b)
        ),
        _ => "paper.pdf".to_string(),
    };
    let object_data = if r.folder { " data=\"paper.pdf\"" } else { "" };
    let mut page = String::new();
    page.push_str("<!doctype html><html lang=\"en\"><head><title>");
    page.push_str(&fold::text(r.title));
    page.push_str(
        "</title><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>",
    );
    page.push_str(theme_css);
    page.push_str(CSS);
    page.push_str("</style></head><body class=\"m-reader\"><main><header><h1>");
    page.push_str(&fold::text(r.title));
    page.push_str("</h1>");
    if !by.is_empty() {
        page.push_str(&format!("<p class=\"authors\">{by}</p>"));
    }
    if let Some(a) = r.abstract_text.filter(|a| !a.is_empty()) {
        page.push_str(&format!("<p class=\"abstract\">{}</p>", fold::text(a)));
    }
    page.push_str("<noscript><p class=\"note\">Scripts are off, so the interactive figures show their posters. The paper itself is the link below.</p></noscript></header>");
    page.push_str(&format!(
        "<section id=\"paper\"><h2>Paper</h2><p><a id=\"pdf-link\" href=\"{href}\" download=\"paper.pdf\">paper.pdf</a> is the version of record.</p><object id=\"pdf\" type=\"application/pdf\"{object_data}><p class=\"note\">This browser cannot show the pdf here. Use the link above.</p></object></section>"
    ));
    if !r.widgets.is_empty() {
        page.push_str("<section id=\"widgets\"><h2>Interactive figures</h2>");
        for w in &r.widgets {
            let ar = if w.width > 0.0 && w.height > 0.0 {
                format!("{:.2} / {:.2}", w.width, w.height)
            } else {
                "4 / 3".to_string()
            };
            let name = w
                .figure
                .filter(|f| !f.is_empty())
                .or(w.label.filter(|l| !l.is_empty()));
            let cap = match name {
                Some(n) => format!("<strong>{}</strong> {}", fold::text(n), fold::text(w.alt)),
                None => fold::text(w.alt),
            };
            page.push_str(&format!(
                "<figure id=\"{id}\" data-widget=\"{id}\" data-type=\"{kind}\" style=\"--ar:{ar}\"><div class=\"frame\"><img class=\"poster\" src=\"{poster}\" alt=\"{alt}\"></div><figcaption>{cap}</figcaption></figure>",
                id = fold::attr(w.id),
                kind = fold::attr(w.kind),
                poster = fold::attr(&w.poster),
                alt = fold::attr(w.alt),
            ));
        }
        page.push_str("</section>");
    }
    page.push_str("</main>");
    page.push_str(r.islands);
    let tokens = serde_json::to_string(&token_names(theme_css)).unwrap_or_else(|_| "[]".into());
    let js = JS
        .replace("__FOLDER__", if r.folder { "true" } else { "false" })
        .replace("__TOKENS__", &tokens);
    page.push_str(&format!("<script>{js}</script></body></html>\n"));
    fold::with_policy(&page, &policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(folder: bool) -> String {
        let authors = vec!["Ada <Lovelace>".to_string()];
        let widgets = vec![
            ReaderWidget {
                id: "fig-a",
                kind: "model",
                figure: Some("Figure 1"),
                label: Some("fig:a"),
                alt: "A mesh & more",
                width: 200.0,
                height: 100.0,
                poster: if folder {
                    "assets/aa.png".into()
                } else {
                    "data:image/png;base64,AAAA".into()
                },
            },
            ReaderWidget {
                id: "fig-b",
                kind: "table",
                figure: None,
                label: None,
                alt: "Rows",
                width: 0.0,
                height: 0.0,
                poster: "assets/bb.png".into(),
            },
        ];
        render(
            &Reader {
                title: "T & <title>",
                authors: &authors,
                abstract_text: Some("An abstract."),
                folder,
                widgets,
                pdf: if folder { None } else { Some(b"%PDF-1.4 x") },
                islands: "<script type=\"application/json\" id=\"mfw-manifest\">{}</script>",
                frames: &[],
            },
            super::super::THEME_CSS,
        )
    }

    #[test]
    fn widgets_follow_in_order_with_caption_and_poster() {
        for folder in [false, true] {
            let h = page(folder);
            let a = h.find("data-widget=\"fig-a\"").unwrap();
            let b = h.find("data-widget=\"fig-b\"").unwrap();
            assert!(h.find("id=\"pdf\"").unwrap() < a && a < b);
            assert!(h.contains("<strong>Figure 1</strong> A mesh &amp; more"));
            assert!(h.contains("<figcaption>Rows</figcaption>"));
            assert_eq!(h.matches("class=\"poster\"").count(), 2);
            assert!(h.contains("<noscript>"));
            assert!(h.contains("<h1>T &amp; &lt;title&gt;</h1>"));
            assert!(h.contains("Ada &lt;Lovelace&gt;"));
            assert!(h.contains("--ar:4 / 3"), "no rect falls back to 4:3");
        }
    }

    #[test]
    fn frames_are_sandboxed_to_scripts_only() {
        let h = page(false);
        assert!(h.contains("setAttribute('sandbox', 'allow-scripts')"));
        assert!(!h.contains("allow-same-origin"));
        assert!(
            !h.contains("<iframe"),
            "frames are made by the page's own script, after its listener"
        );
    }

    #[test]
    fn the_reader_policy_is_the_first_element_and_matches_the_profile() {
        let single = page(false);
        let folder = page(true);
        for h in [&single, &folder] {
            assert!(h.starts_with("<!doctype html><html lang=\"en\"><head><meta http-equiv=\"Content-Security-Policy\""));
        }
        assert!(single.contains(&format!("content=\"{}\"", fold::SINGLE_FILE_READER_POLICY)));
        assert!(folder.contains(&format!("content=\"{}\"", fold::FOLDER_READER_POLICY)));
        assert!(!single.contains("frame-src"));
        assert!(folder.contains("frame-src 'self'"));
        assert!(single.contains("default-src 'none'") && single.contains("connect-src 'none'"));
        assert!(single.contains("object-src blob:"));
    }

    #[test]
    fn the_single_file_frame_src_is_exactly_the_declared_union() {
        let frames = [
            "https://a.org".to_string(),
            "https://b.org:8443".to_string(),
        ];
        assert_eq!(policy(false, &[]), fold::SINGLE_FILE_READER_POLICY);
        let p = policy(false, &frames);
        assert!(p.starts_with(fold::SINGLE_FILE_READER_POLICY));
        assert!(p.ends_with("; frame-src https://a.org https://b.org:8443"));
        assert_eq!(p.matches("frame-src").count(), 1);
        // A folder widget is its own document: the reader adds nothing.
        assert_eq!(policy(true, &frames), fold::FOLDER_READER_POLICY);
    }

    #[test]
    fn single_file_carries_the_pdf_once_and_folder_links_it() {
        let single = page(false);
        assert!(single.contains("id=\"pdf-link\" href=\"data:application/pdf;base64,"));
        let folder = page(true);
        assert!(folder.contains("href=\"paper.pdf\"") && folder.contains("data=\"paper.pdf\""));
        assert!(
            folder.contains("file:"),
            "the unsupported-over-file message is in the script"
        );
    }

    #[test]
    fn nothing_is_loaded_from_another_origin() {
        let url =
            regex::Regex::new(r#"(?i)(?:src|href|data|action)\s*=\s*["']?(?:https?:)?//"#).unwrap();
        for folder in [false, true] {
            let h = page(folder);
            assert!(!url.is_match(&h));
            assert!(!h.contains("@import") && !h.contains("<link "));
        }
    }

    #[test]
    fn token_names_come_from_the_theme_css() {
        let t = token_names(super::super::THEME_CSS);
        assert!(
            t.contains(&"--m-color-bg".to_string()) && t.contains(&"--m-figure-bg".to_string())
        );
        assert!(t.len() > 20);
    }
}
