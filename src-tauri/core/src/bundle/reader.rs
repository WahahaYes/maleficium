//! The reader of an exported bundle: `index.html`, the paper itself as a
//! reflowed article (built by [`crate::reflow::article`]: title, authors,
//! abstract, contents, sections, math, figures, bibliography) with each
//! widget mounted where its figure sits, a sandboxed frame over its poster,
//! the poster being the pre-load and no-script view. `paper.pdf` is the
//! version of record, offered as a download. One static page: inline css and
//! script, no CDN, no request beyond the bundle's own files.
//!
//! Frames are `sandbox="allow-scripts"` and nothing else, never
//! `allow-same-origin` (the 2026-09-30 browser export spike). A single-file
//! bundle mounts each folded widget document as `srcdoc`; a folder bundle
//! points the frame at `widgets/<id>/index.html` (http or https only: over
//! file:// the page says so instead of showing a broken view).

use super::flags::Measure;
use super::fold;
use base64::Engine;

const CSS: &str = include_str!("reader.css");
const JS: &str = include_str!("reader.js");

pub(super) struct Reader<'a> {
    pub title: &'a str,
    pub folder: bool,
    /// The sanitized `<article>`, mount units in place.
    pub article: &'a str,
    /// The article column width (`paper.reader.measure`): a closed token,
    /// rendered as a `data-measure` override of the theme's `--m-measure`.
    pub measure: Measure,
    /// The pdf bytes of a single-file bundle (a folder links `paper.pdf`).
    pub pdf: Option<&'a [u8]>,
    /// The `<script type="application/json">` islands the page reads.
    pub islands: &'a str,
    /// Every frame origin a widget declared, sorted and deduplicated.
    pub frames: &'a [String],
    /// The paper's theme, and its CSS ([`crate::theme::Theme::css`]): the
    /// page is styled by the one and hands the other's values to every
    /// widget, so chrome and widgets agree.
    pub theme: &'a crate::theme::Theme,
    pub theme_css: &'a str,
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

pub(super) fn render(r: &Reader) -> String {
    let policy = policy(r.folder, r.frames);
    let href = match r.pdf {
        Some(b) if !r.folder => format!(
            "data:application/pdf;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b)
        ),
        _ => "paper.pdf".to_string(),
    };
    let mut page = String::new();
    page.push_str("<!doctype html><html lang=\"en\"><head><title>");
    page.push_str(&fold::text(r.title));
    page.push_str(
        "</title><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>",
    );
    page.push_str(r.theme_css);
    page.push_str(CSS);
    page.push_str("</style></head><body class=\"m-reader\"");
    // Only a non-default measure rides along, as its closed token (the
    // widths live in reader.css): `default` is the theme's 68ch.
    if r.measure.width().is_some() {
        page.push_str(&format!(" data-measure=\"{}\"", r.measure.token()));
    }
    page.push_str("><main>");
    page.push_str("<noscript><p class=\"note\">Scripts are off, so the interactive figures show their posters.</p></noscript>");
    page.push_str(&format!(
        "<p class=\"m-version\"><a id=\"pdf-link\" href=\"{href}\" download=\"paper.pdf\">paper.pdf</a> is the version of record.</p>"
    ));
    page.push_str(r.article);
    page.push_str("</main>");
    page.push_str(r.islands);
    page.push_str(&super::island("mfw-theme", &r.theme.json()));
    let js = JS.replace("__FOLDER__", if r.folder { "true" } else { "false" });
    page.push_str(&format!("<script>{js}</script></body></html>\n"));
    fold::with_policy(&page, &policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = "<article class=\"ltx_document\"><h1 class=\"ltx_title ltx_title_document\">T</h1><figure id=\"fig-a\" data-widget=\"fig-a\" data-type=\"model\" style=\"--ar:2.00 / 1.00\"><div class=\"frame\"><img class=\"poster\" src=\"assets/aa.png\" alt=\"A\"></div><figcaption>A</figcaption></figure></article>";

    fn page(folder: bool) -> String {
        page_with(folder, Measure::Default)
    }

    fn page_with(folder: bool, measure: Measure) -> String {
        let theme = crate::theme::Theme::house();
        render(&Reader {
            title: "T & <title>",
            folder,
            article: ARTICLE,
            measure,
            pdf: if folder { None } else { Some(b"%PDF-1.4 x") },
            islands: "<script type=\"application/json\" id=\"mfw-manifest\">{}</script>",
            frames: &[],
            theme: &theme,
            theme_css: &theme.css().unwrap(),
        })
    }

    #[test]
    fn the_measure_flag_overrides_the_article_column_with_a_closed_token() {
        let default = page(false);
        assert!(
            !default.contains("<body class=\"m-reader\" data-measure"),
            "default is the theme's 68ch"
        );
        for (measure, width) in [(Measure::Narrow, "56ch"), (Measure::Wide, "80ch")] {
            let h = page_with(false, measure);
            let token = measure.token();
            assert!(
                h.contains(&format!(
                    "<body class=\"m-reader\" data-measure=\"{token}\">"
                )),
                "{token}"
            );
            assert!(
                h.contains(&format!(".m-reader[data-measure='{token}']")),
                "{token} has its override rule"
            );
            assert!(
                h.contains(&format!("--m-measure: {width};")),
                "{token} maps to its closed width"
            );
        }
    }

    #[test]
    fn the_page_is_the_article_with_the_pdf_offered_and_not_embedded() {
        for folder in [false, true] {
            let h = page(folder);
            let main = &h[h.find("<main>").unwrap()..h.find("</main>").unwrap()];
            assert!(main.contains(ARTICLE), "the article goes in as given");
            assert!(h.contains("<title>T &amp; &lt;title&gt;</title>"));
            // A browser reads a page with no charset in its head as
            // windows-1252 and the math turns to mojibake; the wrapper markup
            // the script carries names one, which must not count.
            let head = &h[..h.find("</head>").unwrap()];
            assert!(head.contains("<meta charset=\"utf-8\">"), "{head}");
            assert!(main.find("id=\"pdf-link\"").unwrap() < main.find("<article").unwrap());
            assert!(h.contains("<noscript>"));
            for gone in ["<object", "id=\"pdf\"", "id=\"widgets\"", "<embed"] {
                assert!(!h.contains(gone), "{gone}");
            }
        }
    }

    #[test]
    fn frames_are_sandboxed_to_scripts_only() {
        let h = page(false);
        assert!(h.contains("setAttribute('sandbox', 'allow-scripts')"));
        assert!(!h.contains("allow-same-origin"));
        let (markup, script) = h.split_once("<script>").unwrap();
        assert!(
            !markup.contains("<iframe"),
            "frames are made by the page's own script, after its listener"
        );
        // A single-file widget's wrapper frames it with the same sandbox and
        // names only its own frame origins.
        assert!(script.contains("<iframe sandbox=\"allow-scripts\" referrerpolicy=\"no-referrer\""));
        assert!(script.contains("var WRAP_DIRECTIVE = 'frame-src';"));
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
        assert!(!fold::policy_of(&single).contains("frame-src"));
        assert!(fold::policy_of(&folder).contains("frame-src 'self'"));
        assert!(single.contains("default-src 'none'") && single.contains("connect-src 'none'"));
        assert!(single.contains("object-src 'none'") && folder.contains("object-src 'none'"));
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
        assert_eq!(single.matches("data:application/pdf").count(), 1);
        let folder = page(true);
        assert!(folder.contains("id=\"pdf-link\" href=\"paper.pdf\" download=\"paper.pdf\""));
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
    fn plain_figures_fill_their_float_whatever_the_source_pixels() {
        let h = page(false);
        assert!(
            h.contains("figure img.ltx_graphics") && h.contains("width: 100%"),
            "a figure's image fills its float, however small its source"
        );
    }

    #[test]
    fn figures_go_live_on_painted_not_on_init() {
        let h = page(false);
        // The init send only marks loading; the widget's painted report
        // swaps the poster for the frame, so a screenshot never catches a
        // mounted-but-blank widget.
        assert!(h.contains("rec.fig.setAttribute('data-state', 'loading')"));
        assert!(h.contains("d.state === 'loaded'"));
        assert!(
            !h.contains("rec.fig.classList.add('live')"),
            "live is set on the loaded report, never on the init send"
        );
    }

    #[test]
    fn the_page_css_and_the_widget_tokens_come_from_one_theme() {
        let h = page(false);
        let theme = crate::theme::Theme::house();
        assert!(h.contains(&theme.css().unwrap()));
        let island = format!(
            "<script type=\"application/json\" id=\"mfw-theme\">{}</script>",
            serde_json::to_string(&theme.json())
                .unwrap()
                .replace('<', "\\u003c")
        );
        assert!(
            h.contains(&island),
            "the reader hands widgets the record's values"
        );
        // reader.js reads the island; it no longer scrapes computed styles.
        assert!(h.contains("island('mfw-theme')"));
        assert!(!h.contains("getPropertyValue"));
    }
}
