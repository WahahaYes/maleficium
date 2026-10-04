use super::*;
use crate::reflow::figures::FigureReason;
use std::path::PathBuf;

/// A latexml-shaped conversion of e2e/fixtures/interactive (placeholders as
/// the widget binding emits them, figures with `data-graphic`), plus hostile
/// markup the sanitizer must remove.
const FIXTURE: &str = include_str!("../fixtures/interactive-converted.frag");
const SIDECAR: &str = include_str!("../../../testdata/interactive/main.mfw");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../e2e/fixtures/interactive")
}

fn mounts() -> Vec<Mount<'static>> {
    [
        ("fig-mesh", "model", Some("1")),
        ("fig-clip", "video", None),
        ("tab-results", "table", None),
        ("fig-chart", "chart", None),
        ("fig-demo", "html", None),
    ]
    .into_iter()
    .map(|(id, kind, figure)| Mount {
        id,
        kind,
        figure,
        label: None,
        alt: "Alt & text",
        width: 200.0,
        height: 100.0,
        poster: format!("assets/{id}.png"),
    })
    .collect()
}

fn build_with(html: &str, sidecar: Option<&str>, mode: figures::Mode) -> Article {
    let root = root();
    let m = mounts();
    build(Input {
        html,
        root: &root,
        mode,
        graphics_paths: Vec::new(),
        sidecar,
        mounts: &m,
        title: "Fallback <Title>",
    })
    .unwrap()
}

#[test]
fn the_mount_unit_is_the_markup_the_page_script_mounts() {
    let m = &mounts()[0];
    assert_eq!(
        mount_unit(m),
        "<figure id=\"fig-mesh\" data-widget=\"fig-mesh\" data-type=\"model\" style=\"--ar:200.00 / 100.00\"><div class=\"frame\"><img class=\"poster\" src=\"assets/fig-mesh.png\" alt=\"Alt &amp; text\"></div><figcaption><strong>1</strong> Alt &amp; text</figcaption></figure>"
    );
    let bare = Mount {
        width: 0.0,
        figure: None,
        ..mounts().remove(1)
    };
    assert!(mount_unit(&bare).contains("--ar:4 / 3"));
    assert!(mount_unit(&bare).contains("<figcaption>Alt &amp; text</figcaption>"));
}

#[test]
fn the_whole_pipeline_mounts_every_widget_in_place_and_sanitizes_last() {
    let a = build_with(FIXTURE, Some(SIDECAR), figures::Mode::SingleFile);
    let h = &a.html;
    assert!(h.starts_with("<article class=\"ltx_document\">") && h.ends_with("</article>"));

    // Every widget in document order, inside its float when it has one.
    let pos: Vec<usize> = [
        "fig-mesh",
        "fig-clip",
        "tab-results",
        "fig-chart",
        "fig-demo",
    ]
    .iter()
    .map(|id| h.find(&format!("data-widget=\"{id}\"")).expect(id))
    .collect();
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "{pos:?}");
    assert!(h.find("id=\"S1.F1\"").unwrap() < pos[0] && pos[0] < h.find("id=\"S1.F2\"").unwrap());
    assert_eq!(h.matches("class=\"poster\"").count(), 5);
    assert!(!h.contains("m-unplaced") && !h.contains("m-widget"));
    // A captioned float's caption is the widget's caption; a bare widget
    // keeps its own.
    let mesh = &h[pos[0]..h[pos[0]..].find("</figure>").unwrap() + pos[0]];
    assert!(!mesh.contains("<figcaption>"), "{mesh}");
    let table = &h[pos[2]..h[pos[2]..].find("</figure>").unwrap() + pos[2]];
    assert!(
        table.contains("<figcaption>Alt &amp; text</figcaption>"),
        "{table}"
    );

    // Figures: the mesh is inline data, the missing file a visible placeholder.
    assert!(h.contains("<img src=\"data:image/png;base64,"), "{h}");
    assert!(h.contains(
        "id=\"S1.F3.g1\" class=\"ltx_graphics ltx_centering\" alt=\"Refer to caption\">"
    ));
    assert!(h.contains("class=\"ltx_missing_figure\" role=\"img\""));
    assert!(a.issues.iter().any(|i| matches!(i, Issue::Figure(w) if w.file == "figures/absent.png" && w.reason == FigureReason::Missing)));

    // Links: the citation reaches the bibliography entry in the page, the
    // figure reference stays, a dead fragment is unlinked, the web stays.
    assert!(h.contains("<a href=\"#bib.bib1\" title=\"\" class=\"ltx_ref\">1</a>"));
    assert!(h.contains("<a href=\"#S1.F3\" class=\"ltx_ref\">"));
    assert!(h.contains("<a class=\"ltx_ref\">dead link</a>"));
    assert!(h.contains("<a href=\"https://example.org/\""));

    // The undefined macro stays visible and is counted.
    assert!(h.contains("<span class=\"ltx_ERROR undefined\">\\undefinedmacro</span>"));
    assert!(a.issues.contains(&Issue::Errors(1)));

    // Contents after the abstract: sections, the subsection, the references.
    let nav = h.find("<nav class=\"m-contents\"").unwrap();
    assert!(h.find("ltx_abstract").unwrap() < nav && nav < h.find("id=\"S1\"").unwrap());
    assert!(h.contains("<li><a href=\"#S1\">1 Widgets</a><ol><li><a href=\"#S1.SS1\">1.1 Plain figures</a></li></ol></li><li><a href=\"#bib\">References</a></li>"));

    // One title (the article's own); no page chrome, no comments, nothing hostile.
    assert_eq!(h.matches("<h1").count(), 1);
    assert!(h.contains(">Interactive Fixture</h1>"));
    for gone in [
        "<!--",
        "ltx_page_footer",
        "dlmf",
        "<script",
        "onclick",
        "onerror",
        "style=\"color",
        "javascript:",
        "alert(4)",
        "mfw-manifest",
        "<link",
        "<meta",
        "<title",
    ] {
        assert!(!h.contains(gone), "{gone} survived");
    }
    assert!(!a.issues.iter().any(|i| matches!(i, Issue::Join(_))));
}

#[test]
fn folder_figures_are_files_beside_the_page() {
    let dir = crate::test_scratch::dir("article-folder");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let a = build_with(
        FIXTURE,
        Some(SIDECAR),
        figures::Mode::Folder { dir: dir.clone() },
    );
    let re = regex::Regex::new(r#"src="(figures/[0-9a-f]+\.png)""#).unwrap();
    let rel = re.captures(&a.html).expect("a figures/ src")[1].to_string();
    assert!(dir.join(&rel).is_file());
    assert!(!a.html.contains("data:image"));
}

#[test]
fn a_join_error_keeps_the_article_and_lists_the_widgets_at_the_end() {
    let four: String = SIDECAR.lines().take(5).collect::<Vec<_>>().join("\n") + "\n";
    let a = build_with(FIXTURE, Some(&four), figures::Mode::SingleFile);
    assert!(a.issues.iter().any(|i| matches!(
        i,
        Issue::Join(JoinError::CountMismatch {
            placeholders: 5,
            records: 4
        })
    )));
    let section = a
        .html
        .find("<section class=\"m-unplaced\">")
        .expect("unplaced section");
    assert_eq!(a.html[section..].matches("data-widget=").count(), 5);
    assert!(
        a.html.contains("Widgets</h2>"),
        "the article is still there"
    );
    // No sidecar but placeholders: the same.
    let a = build_with(FIXTURE, None, figures::Mode::SingleFile);
    assert!(a.issues.iter().any(|i| matches!(i, Issue::Join(_))));
}

#[test]
fn an_article_without_a_title_gets_the_page_title_and_no_contents_for_one_section() {
    let html = "<html><body><article class=\"ltx_document\"><section id=\"S1\" class=\"ltx_section\"><h2>1 Only</h2><p>x</p></section></article></body></html>";
    let a = build_with(html, Some("mfw 1\n"), figures::Mode::SingleFile);
    assert!(a
        .html
        .contains("<h1 class=\"ltx_title ltx_title_document\">Fallback &lt;Title&gt;</h1>"));
    assert!(!a.html.contains("m-contents"));
    // Widgets the article has no placeholders for are still on the page.
    assert_eq!(a.html.matches("data-widget=").count(), 5);
}

#[test]
fn a_document_that_is_not_latexml_is_wrapped_as_the_article() {
    let a = build_with(
        "<p>plain <b>text</b></p>",
        Some("mfw 1\n"),
        figures::Mode::SingleFile,
    );
    assert!(a.html.starts_with("<article class=\"ltx_document\"><h1"));
    assert!(a.html.contains("<p>plain <b>text</b></p>"));
}

#[test]
fn a_proof_final_qed_becomes_a_drawn_mark() {
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><h6 class=\"ltx_title ltx_runin\">Proof.</h6><div class=\"ltx_para\"><p class=\"ltx_p\">Jensen gives it.\n∎</p></div></div></article>",
        "T",
        &[],
    );
    assert!(html.contains("<span class=\"m-qed\" role=\"img\" aria-label=\"End of proof\"></span>"));
    assert!(!html.contains('∎'));
}

#[test]
fn a_non_trailing_qed_stays_text() {
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><div class=\"ltx_para\"><p class=\"ltx_p\">∎ marks the spot, and text follows.</p></div></div></article>",
        "T",
        &[],
    );
    assert!(html.contains('∎'));
    assert!(!html.contains("m-qed"));
}
