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
        fraction: None,
        poster: format!("assets/{id}.png"),
        note: None,
        approval_required: false,
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
fn frames_keep_the_content_aspect_and_give_the_author_width_as_a_fraction() {
    // The aspect is the poster as placed; the author's height never enters.
    assert_eq!(
        frame("chart", None, Some(100.0), Some(200.0), (60.0, 30.0)),
        ((60.0, 30.0), Some(0.5))
    );
    // A model is its size=, else 4:3.
    assert_eq!(
        frame("model", Some((800, 600)), None, Some(200.0), (9.0, 9.0)),
        ((800.0, 600.0), None)
    );
    assert_eq!(
        frame("model", None, None, None, (9.0, 9.0)),
        ((4.0, 3.0), None)
    );
    // The full line, or wider, is the full column: no fraction.
    assert_eq!(
        frame("video", None, Some(200.0), Some(200.0), (9.0, 9.0)),
        ((9.0, 9.0), None)
    );
    assert_eq!(
        frame("video", None, Some(300.0), Some(200.0), (9.0, 9.0)),
        ((9.0, 9.0), None)
    );
    // Without the line (an older sidecar) or a width, no fraction.
    assert_eq!(
        frame("chart", None, Some(100.0), None, (60.0, 30.0)),
        ((60.0, 30.0), None)
    );
    assert_eq!(
        frame("chart", None, None, Some(200.0), (60.0, 30.0)),
        ((60.0, 30.0), None)
    );
    // A degenerate rect falls back to 4:3, never a zero aspect.
    assert_eq!(
        frame("chart", None, None, None, (0.0, 0.0)),
        ((4.0, 3.0), None)
    );
}

#[test]
fn a_mount_unit_with_an_author_width_carries_the_fraction() {
    let mut m = mounts().remove(0);
    m.width = 4.0;
    m.height = 3.0;
    m.fraction = Some(0.5);
    let html = mount_unit(&m);
    assert!(
        html.contains("style=\"--ar:4.00 / 3.00; --fw:0.500\""),
        "{html}"
    );
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

    // The undefined macro stays visible and is grouped by name.
    assert!(h.contains("<span class=\"ltx_ERROR undefined\">\\undefinedmacro</span>"));
    assert_eq!(
        a.issues.iter().find_map(|i| match i {
            Issue::Undefined(g) => Some(g.clone()),
            _ => None,
        }),
        Some(vec![UndefinedMacro {
            name: String::from("\\undefinedmacro"),
            spots: 1,
        }])
    );

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
fn undefined_macros_group_by_name_with_spots() {
    // The shape the origin paper (e2e/fixtures/vendored/
    // on-the-origin-of-objects) converts to today: every `ltx_ERROR` span
    // shows the same undefined macro.
    let mut html = String::from(
        "<html><body><article class=\"ltx_document\"><section id=\"S1\"><h2>1 One</h2><p>",
    );
    for _ in 0..11 {
        html.push_str("<span class=\"ltx_ERROR undefined\">\\eolang</span> ");
    }
    html.push_str(
        "<span class=\"ltx_ERROR undefined\">\\ff{code}</span> \
         <span class=\"ltx_ERROR undefined\">{ffcode}</span> \
         </p></section><section id=\"S2\"><h2>2 Two</h2></section></article></body></html>",
    );
    let a = build_with(&html, Some("mfw 1\n"), figures::Mode::SingleFile);
    let groups = a.issues.iter().find_map(|i| match i {
        Issue::Undefined(g) => Some(g.clone()),
        _ => None,
    });
    assert_eq!(
        groups,
        Some(vec![
            UndefinedMacro {
                name: String::from("\\eolang"),
                spots: 11,
            },
            UndefinedMacro {
                name: String::from("\\ff"),
                spots: 1,
            },
            UndefinedMacro {
                name: String::from("{ffcode}"),
                spots: 1,
            },
        ])
    );
    assert!(a.html.contains("\\eolang"), "the article keeps the raw TeX");
}

#[test]
fn macro_names_come_from_the_first_macro_or_the_raw_text() {
    assert_eq!(macro_name("\\eolang"), "\\eolang");
    assert_eq!(macro_name("\\ff{code} and more"), "\\ff");
    assert_eq!(macro_name("{ffcode}"), "{ffcode}");
    assert_eq!(macro_name("  "), "(unreadable TeX)");
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

#[test]
fn a_doubled_final_qed_becomes_one_drawn_mark() {
    // An explicit `\qed` plus latexml's own mark (the shape an author-added
    // `\qed` converts to): every trailing mark goes, one mark is drawn.
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><div class=\"ltx_para\"><p class=\"ltx_p\">Explicit qed at end ∎∎</p></div></div></article>",
        "T",
        &[],
    );
    assert_eq!(html.matches("m-qed").count(), 1);
    assert!(!html.contains('∎'));
}

#[test]
fn a_lone_paragraph_qed_becomes_a_drawn_mark() {
    // A proof ending in display math: latexml parks its mark in its own
    // paragraph after the equation.
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><div class=\"ltx_para\"><p class=\"ltx_p\">Text then display math:</p><table class=\"ltx_equation ltx_eqn_table\"><tbody><tr><td class=\"ltx_eqn_cell ltx_align_center\"><math display=\"block\"><mrow><msup><mi>x</mi><mn>2</mn></msup></mrow></math></td></tr></tbody></table><p class=\"ltx_p\">∎</p></div></div></article>",
        "T",
        &[],
    );
    assert!(html.contains("<span class=\"m-qed\" role=\"img\" aria-label=\"End of proof\"></span>"));
    assert!(!html.contains('∎'));
}

#[test]
fn a_qedhere_in_display_math_moves_the_mark_out() {
    // `\qedhere` inside display math: latexml wraps the mark in an `mo`
    // inside the formula. The wrapper goes; the drawn mark follows it.
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><div class=\"ltx_para\"><p class=\"ltx_p\">Qedhere inside display math:</p><table class=\"ltx_equation ltx_eqn_table\"><tbody><tr><td class=\"ltx_eqn_cell ltx_align_center\"><math display=\"block\"><mrow><msup><mi>x</mi><mn>2</mn></msup><mo class=\"ltx_mathvariant_italic\" mathvariant=\"italic\" separator=\"true\">∎</mo></mrow></math></td></tr></tbody></table></div></div></article>",
        "T",
        &[],
    );
    assert!(!html.contains('∎'));
    assert!(!html.contains("<mo"));
    let math = html.find("</math>").expect("the formula stays");
    assert!(
        html[math..].contains("m-qed"),
        "the mark follows the formula"
    );
}

#[test]
fn a_qedhere_at_the_end_of_inline_math_moves_the_mark_out() {
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><div class=\"ltx_para\"><p class=\"ltx_p\">Ends with inline math <math display=\"inline\"><mrow><msup><mi>x</mi><mn>2</mn></msup><mo class=\"ltx_mathvariant_italic\" mathvariant=\"italic\" separator=\"true\">∎</mo></mrow></math></p></div></div></article>",
        "T",
        &[],
    );
    assert!(!html.contains('∎'));
    let math = html.find("</math>").expect("the formula stays");
    assert!(
        html[math..].contains("m-qed"),
        "the mark follows the formula"
    );
}

#[test]
fn a_mid_proof_math_mark_stays_text() {
    // `\qedhere` inside math with text after it is not the proof end: the
    // wrapped mark stays, and no drawn mark appears.
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><div class=\"ltx_para\"><p class=\"ltx_p\">Qedhere in inline math <math display=\"inline\"><mrow><msup><mi>x</mi><mn>2</mn></msup><mo class=\"ltx_mathvariant_italic\" mathvariant=\"italic\" separator=\"true\">∎</mo></mrow></math> done.</p></div></div></article>",
        "T",
        &[],
    );
    assert!(html.contains('∎'));
    assert!(!html.contains("m-qed"));
}

#[test]
fn an_equation_number_after_the_mark_does_not_hide_it() {
    // `\qedhere` in a numbered equation: the `(1)` tag follows the mark in
    // the tree, but it is chrome, so the drawn mark still replaces the mark.
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><div class=\"ltx_para\"><p class=\"ltx_p\">Qedhere inside equation:</p><table class=\"ltx_equation ltx_eqn_table\"><tbody><tr><td class=\"ltx_eqn_cell ltx_align_center\"><math display=\"block\"><mrow><msup><mi>x</mi><mn>2</mn></msup><mo class=\"ltx_mathvariant_italic\" mathvariant=\"italic\" separator=\"true\">∎</mo></mrow></math></td><td class=\"ltx_eqn_cell ltx_eqn_eqno ltx_align_right\"><span class=\"ltx_tag ltx_tag_equation ltx_align_right\">(1)</span></td></tr></tbody></table></div></div></article>",
        "T",
        &[],
    );
    assert!(!html.contains('∎'));
    assert!(html.contains("(1)"), "the equation number stays");
    let math = html.find("</math>").expect("the formula stays");
    assert!(
        html[math..].contains("m-qed"),
        "the mark follows the formula"
    );
}
#[test]
fn anchors_follow_the_contents_flattened_and_ignore_outside_markup() {
    let a = build_with(FIXTURE, Some(SIDECAR), figures::Mode::SingleFile);
    // A sibling section and a section-shaped JSON island: neither is the
    // article, so neither contributes an anchor.
    let page = format!(
        "<!doctype html><html><head><title>t</title></head><body>\
        <section id=\"S9\"><h2>9 Elsewhere</h2></section>\
        <script id=\"mfw-manifest\" type=\"application/json\">{{\"note\":\"<section id=S9><h2>9 Island</h2></section>\"}}</script>\
        {}</body></html>",
        a.html
    );
    assert_eq!(
        anchors_of(&page),
        vec![
            Anchor {
                id: "S1".into(),
                text: "1 Widgets".into()
            },
            Anchor {
                id: "S1.SS1".into(),
                text: "1.1 Plain figures".into()
            },
            Anchor {
                id: "bib".into(),
                text: "References".into()
            },
        ]
    );
    assert!(anchors_of("<p>no article here</p>").is_empty());
}

#[test]
fn a_mount_unit_held_back_for_approval_carries_the_marker_and_its_note() {
    let mut all = mounts();
    let held = &mut all[4];
    held.approval_required = true;
    held.note = Some("This interactive figure needs your approval before it runs here.".into());
    let html = mount_unit(held);
    assert!(html.contains(" data-approval=\"required\""), "{html}");
    assert!(html.contains("<p class=\"m-widget-note\">"), "{html}");
    let plain = mount_unit(&mounts()[4]);
    assert!(!plain.contains("data-approval"), "{plain}");
}

#[test]
fn a_proof_folds_into_open_details_with_its_title_as_the_summary() {
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><h6 class=\"ltx_title ltx_runin ltx_font_italic ltx_title_proof\">Proof of Theorem 3.</h6><div class=\"ltx_para\"><p class=\"ltx_p\">Jensen gives it.\n∎</p></div></div></article>",
        "T",
        &[],
    );
    assert!(
        html.contains("<details class=\"ltx_proof\" open=\"\"><summary class=\"m-proof-summary\"><h6 class=\"ltx_title ltx_runin ltx_font_italic ltx_title_proof\">Proof of Theorem 3.</h6></summary><div class=\"ltx_para\">"),
        "{html}"
    );
    assert!(!html.contains("<div class=\"ltx_proof\""));
    // The drawn end mark stays inside the folded body.
    let body = &html[html.find("</summary>").unwrap()..html.find("</details>").unwrap()];
    assert!(body.contains("m-qed"), "{body}");
}

#[test]
fn a_proof_without_a_title_gets_the_plain_one() {
    let (html, _) = post_process(
        "<article class=\"ltx_document\"><div class=\"ltx_proof\"><div class=\"ltx_para\"><p class=\"ltx_p\">Obvious.</p></div></div></article>",
        "T",
        &[],
    );
    assert!(
        html.contains("<summary class=\"m-proof-summary\"><h6 class=\"ltx_title ltx_runin ltx_font_italic ltx_title_proof\">Proof.</h6></summary><div class=\"ltx_para\">"),
        "{html}"
    );
}

#[test]
fn a_folded_proof_survives_the_sanitizer_open() {
    let html = "<html><body><article class=\"ltx_document\"><section id=\"S1\" class=\"ltx_section\"><h2>1 One</h2><div class=\"ltx_proof\"><h6 class=\"ltx_title ltx_runin ltx_title_proof\">Proof.</h6><div class=\"ltx_para\"><p class=\"ltx_p\">Done.</p></div></div></section></article></body></html>";
    let a = build_with(html, Some("mfw 1\n"), figures::Mode::SingleFile);
    assert!(
        a.html.contains("<details class=\"ltx_proof\" open=\"\"><summary class=\"m-proof-summary\"><h6 class=\"ltx_title ltx_runin ltx_title_proof\">Proof.</h6></summary>"),
        "{}",
        a.html
    );
}

#[test]
fn the_page_colour_leaves_the_math_but_a_boxed_highlight_stays() {
    // latexml's shape for `\pagecolor{coffeepaper}` and one `\colorbox` of
    // the same colour: the root carries the page colour, every math token
    // restates it.
    let html = "<html><body><article class=\"ltx_document\" style=\"--ltx-bg-color:#FAF3E8;\">\
        <p><math class=\"ltx_Math\" display=\"inline\"><mi mathbackground=\"#FAF3E8\">x</mi>\
        <mo mathbackground=\"#faf3e8\">=</mo><mn mathbackground=\"#FFFF00\">2</mn></math>\
        <span class=\"ltx_text\" style=\"--ltx-bg-color:#FAF3E8;\"><math class=\"ltx_Math\">\
        <mi mathbackground=\"#FAF3E8\">y</mi></math></span></p></article></body></html>";
    let a = build_with(html, Some("mfw 1\n"), figures::Mode::SingleFile);
    assert_eq!(
        a.html.matches("mathbackground").count(),
        2,
        "only the other colour and the boxed one stay: {}",
        a.html
    );
    assert!(a.html.contains("mathbackground=\"#FFFF00\""));
}
