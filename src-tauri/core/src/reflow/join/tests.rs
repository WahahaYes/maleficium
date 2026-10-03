use super::*;

fn span(kind: &str) -> String {
    format!("<span class=\"ltx_text m-widget m-widget-{kind}\"></span>")
}

/// A sidecar with one record per `(id, kind)`.
fn sidecar(rows: &[(&str, &str)]) -> String {
    let mut s = String::from("mfw 1\n");
    for (id, kind) in rows {
        let rt = if *kind == "html" {
            String::new()
        } else {
            format!("{kind}@1")
        };
        s.push_str(&format!(
            "widget|{id}|{kind}|{rt}|||house|figures/{id}.png||| Alt for {id}\n"
        ));
    }
    s
}

fn page(body: &str) -> String {
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><title>T</title></head><body>\n<article class=\"ltx_document\">\n{body}\n</article></body></html>"
    )
}

fn ids(j: &[Joined]) -> Vec<&str> {
    j.iter().map(|j| j.record.id.as_str()).collect()
}

#[test]
fn no_widgets_joins_nothing() {
    let html = page("<p class=\"ltx_p\">Plain text.</p>");
    assert_eq!(extract_placeholders(&html).unwrap(), vec![]);
    let j = join_sidecar(&html, "mfw 1\n").unwrap();
    assert!(j.is_empty());
    assert_eq!(apply(&html, &j).unwrap(), html);
}

#[test]
fn one_widget_joins_and_is_replaced() {
    let html = page(&format!(
        "<figure id=\"S2.F1\" class=\"ltx_figure\">\n<figcaption class=\"ltx_caption\">A model.</figcaption>\n<p class=\"ltx_p ltx_align_center\">{}</p>\n</figure>",
        span("model")
    ));
    let j = join_sidecar(&html, &sidecar(&[("fig-model", "model")])).unwrap();
    assert_eq!(ids(&j), ["fig-model"]);
    assert_eq!(j[0].placeholder.kind, WidgetType::Model);
    assert_eq!(j[0].placeholder.index, 0);
    let out = apply(&html, &j).unwrap();
    assert!(out.contains("<p class=\"ltx_p ltx_align_center\"><!--mount:fig-model--></p>"));
    assert!(!out.contains("m-widget"));
}

#[test]
fn several_widgets_join_in_document_order() {
    let body = format!(
        "<p>{}</p><p>{}</p><p>{}</p>",
        span("chart"),
        span("table"),
        span("html")
    );
    let html = page(&body);
    let rows = [
        ("a-chart", "chart"),
        ("b-table", "table"),
        ("c-html", "html"),
    ];
    let j = join_sidecar(&html, &sidecar(&rows)).unwrap();
    assert_eq!(ids(&j), ["a-chart", "b-table", "c-html"]);
    assert!(j
        .windows(2)
        .all(|w| w[0].placeholder.end <= w[1].placeholder.start));
    let out = apply(&html, &j).unwrap();
    assert!(out.contains(
        "<p><!--mount:a-chart--></p><p><!--mount:b-table--></p><p><!--mount:c-html--></p>"
    ));
}

#[test]
fn more_placeholders_than_records_is_refused_with_both_counts() {
    let html = page(&format!("<p>{}{}</p>", span("model"), span("video")));
    let e = join_sidecar(&html, &sidecar(&[("only", "model")])).unwrap_err();
    assert_eq!(
        e,
        JoinError::CountMismatch {
            placeholders: 2,
            records: 1
        }
    );
    assert!(e.to_string().contains("2 widget placeholders"));
    assert!(e.to_string().contains("records 1"));
}

#[test]
fn more_records_than_placeholders_is_refused_with_both_counts() {
    let html = page(&format!("<p>{}</p>", span("model")));
    let rows = [("one", "model"), ("two", "video"), ("three", "chart")];
    let e = join_sidecar(&html, &sidecar(&rows)).unwrap_err();
    assert_eq!(
        e,
        JoinError::CountMismatch {
            placeholders: 1,
            records: 3
        }
    );
}

#[test]
fn a_widget_less_article_against_a_populated_sidecar_is_refused() {
    let e = join_sidecar(&page("<p>x</p>"), &sidecar(&[("one", "model")])).unwrap_err();
    assert_eq!(
        e,
        JoinError::CountMismatch {
            placeholders: 0,
            records: 1
        }
    );
}

#[test]
fn equal_counts_with_a_swapped_type_is_refused() {
    let html = page(&format!("<p>{}{}</p>", span("model"), span("video")));
    let e = join_sidecar(&html, &sidecar(&[("m", "model"), ("v", "chart")])).unwrap_err();
    assert_eq!(
        e,
        JoinError::KindMismatch {
            index: 1,
            id: "v".into(),
            placeholder: WidgetType::Video,
            record: WidgetType::Chart
        }
    );
}

#[test]
fn widgets_inside_figures_nested_blocks_and_inline_markup_count() {
    let html = page(&format!(
        concat!(
            "<figure class=\"ltx_figure\"><figcaption>c</figcaption><p>{a}</p></figure>\n",
            "<figure class=\"ltx_figure\"><p class=\"ltx_p ltx_minipage\" style=\"width:138.0pt;\">{b}</p></figure>\n",
            "<p>Text <span class=\"ltx_text ltx_font_bold\">bold {c}</span> and <em>{d}</em>.</p>\n",
            "<span class=\"ltx_note ltx_role_footnote\"><span class=\"ltx_note_outer\"><span class=\"ltx_note_content\">in note {e}</span></span></span>"
        ),
        a = span("model"),
        b = span("html"),
        c = span("chart"),
        d = span("video"),
        e = span("table"),
    ));
    let rows = [
        ("a", "model"),
        ("b", "html"),
        ("c", "chart"),
        ("d", "video"),
        ("e", "table"),
    ];
    let j = join_sidecar(&html, &sidecar(&rows)).unwrap();
    assert_eq!(ids(&j), ["a", "b", "c", "d", "e"]);
    let out = apply(&html, &j).unwrap();
    assert!(out.contains("bold <!--mount:c--></span>"));
    assert!(out.contains("<em><!--mount:d--></em>"));
    assert!(out.contains("in note <!--mount:e--></span>"));
}

#[test]
fn placeholder_lookalikes_in_text_code_comments_and_attributes_do_not_count() {
    let lookalike = "<span class=\"ltx_text m-widget m-widget-model\"></span>";
    let escaped = lookalike.replace('<', "&lt;").replace('>', "&gt;");
    let html = page(&format!(
        concat!(
            "<p>The macro emits {esc} in the output.</p>\n",
            "<pre class=\"ltx_verbatim\">{esc}</pre>\n",
            "<code class=\"ltx_verbatim\">{esc}</code>\n",
            "<!-- {raw} -->\n",
            "<script>var s = '{raw}';</script>\n",
            "<style>/* {raw} */</style>\n",
            "<img alt='{raw}' src=\"x.png\">\n",
            "<a title=\"a > b {raw}\" href=\"#\">link</a>\n",
            "<span class=\"ltx_text m-widgets\"></span>\n",
            "<span class=\"x-m-widget\"></span>\n",
            "<div class=\"m-widget m-widget-model\"></div>\n",
            "<p>{real}</p>"
        ),
        esc = escaped,
        raw = lookalike,
        real = span("chart"),
    ));
    let p = extract_placeholders(&html).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].kind, WidgetType::Chart);
    assert_eq!(&html[p[0].start..p[0].end], span("chart"));
}

#[test]
fn class_tokens_and_quoting_variants_are_read() {
    for tag in [
        "<span class='m-widget m-widget-video'></span>",
        "<span id=\"x\" class=\"m-widget-video m-widget\" ></span>",
        "<SPAN CLASS=\"m-widget m-widget-video\"> \n</SPAN>",
        "<span class=\"m-widget m-widget-video\" data-k=m-widget-model></span>",
    ] {
        let p = extract_placeholders(&format!("<p>{tag}</p>")).unwrap();
        assert_eq!(p.len(), 1, "{tag}");
        assert_eq!(p[0].kind, WidgetType::Video, "{tag}");
    }
}

#[test]
fn unreadable_placeholders_are_malformed_not_skipped() {
    for (html, why) in [
        (
            "<p><span class=\"m-widget\"></span></p>",
            "no m-widget-KIND",
        ),
        (
            "<p><span class=\"m-widget m-widget-gizmo\"></span></p>",
            "unknown widget kind",
        ),
        (
            "<p><span class=\"m-widget m-widget-model m-widget-chart\"></span></p>",
            "more than one",
        ),
        (
            "<p><span class=\"m-widget m-widget-model\">alt text</span></p>",
            "not empty",
        ),
        (
            "<p><span class=\"m-widget m-widget-model\" /></p>",
            "self-closed",
        ),
        ("<p><span class=\"m-widget m-widget-model\"", "not empty"),
    ] {
        match extract_placeholders(html) {
            Err(JoinError::Malformed { offset, reason }) => {
                assert_eq!(offset, 3, "{html}");
                assert!(reason.contains(why), "{html}: {reason}");
            }
            other => panic!("{html}: {other:?}"),
        }
    }
}

#[test]
fn a_malformed_sidecar_is_a_sidecar_error() {
    let e = join_sidecar(&page(""), "not a sidecar\n").unwrap_err();
    assert!(matches!(e, JoinError::Sidecar(m) if m.contains("unsupported widget sidecar")));
}

#[test]
fn apply_refuses_placeholders_from_another_document() {
    let html = page(&format!("<p>{}</p>", span("model")));
    let j = join_sidecar(&html, &sidecar(&[("m", "model")])).unwrap();
    assert_eq!(apply("<p>short</p>", &j), Err(JoinError::Stale));
}

#[test]
fn the_real_playground_article_and_sidecar_join() {
    // The shape latexml emits for the playground paper: one placeholder per
    // widget in a float or a centered paragraph, plain figures between them.
    let body = format!(
        concat!(
            "<figure id=\"S2.F1\" class=\"ltx_figure\"><figcaption class=\"ltx_caption ltx_centering\">M</figcaption>\n<p class=\"ltx_p ltx_align_center\">{}</p></figure>\n",
            "<figure id=\"S2.F2\" class=\"ltx_figure\"><p class=\"ltx_p ltx_align_center\">{}</p></figure>\n",
            "<figure id=\"S2.T1\" class=\"ltx_table\"><p class=\"ltx_p ltx_align_center\">{}</p></figure>\n",
            "<figure id=\"S2.F3\" class=\"ltx_figure\"><p class=\"ltx_p ltx_align_center\">{}</p></figure>\n",
            "<figure id=\"S2.F4\" class=\"ltx_figure\"><p class=\"ltx_p ltx_align_center\">{}</p></figure>\n",
            "<figure class=\"ltx_figure\"><img src=\"figures/plain.png\" class=\"ltx_graphics ltx_centering\" alt=\"Refer to caption\"></figure>"
        ),
        span("model"),
        span("video"),
        span("table"),
        span("chart"),
        span("html"),
    );
    let sc = include_str!("../../../testdata/interactive/main.mfw");
    let j = join_sidecar(&page(&body), sc).unwrap();
    assert_eq!(
        ids(&j),
        [
            "fig-mesh",
            "fig-clip",
            "tab-results",
            "fig-chart",
            "fig-demo"
        ]
    );
}
