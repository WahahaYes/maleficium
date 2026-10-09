use super::*;

const OFF: &str = "<span class=\"ltx_text m-flag m-flag-contents-off\"></span>";
const WIDE: &str = "<span class=\"ltx_text m-flag m-flag-measure-wide\"></span>";

#[test]
fn no_markers_resolve_the_defaults_and_change_nothing() {
    let html = "<article class=\"ltx_document\"><p>Text.</p></article>";
    let s = scan(html);
    assert_eq!(
        (s.flags.contents, s.flags.measure),
        (true, Measure::Default)
    );
    assert_eq!(s.html, html);
    assert!(s.warnings.is_empty());
}

#[test]
fn valid_markers_resolve_and_are_stripped() {
    let html = format!("<article><p>One.</p>{OFF}<p>Two.</p>{WIDE}</article>");
    let s = scan(&html);
    assert_eq!((s.flags.contents, s.flags.measure), (false, Measure::Wide));
    assert_eq!(s.html, "<article><p>One.</p><p>Two.</p></article>");
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    assert!(!s.html.contains("m-flag"));
}

#[test]
fn the_last_valid_marker_of_each_kind_wins() {
    let html = "<span class=\"m-flag m-flag-contents-off\"></span>\
        <span class=\"m-flag m-flag-measure-narrow\"></span>\
        <span class=\"m-flag m-flag-contents-on\"></span>\
        <span class=\"m-flag m-flag-measure-default\"></span>";
    let s = scan(html);
    assert_eq!(
        (s.flags.contents, s.flags.measure),
        (true, Measure::Default)
    );
    assert!(!s.html.contains("m-flag"));
    assert!(s.warnings.is_empty());
}

#[test]
fn an_unknown_token_falls_back_with_a_warning_and_never_coerces() {
    let html = format!(
        "<article><p>One.</p><span class=\"m-flag m-flag-contents-sideways\"></span>{OFF}</article>"
    );
    let s = scan(&html);
    // The hostile marker votes nothing; the later valid one still applies.
    assert!(!s.flags.contents);
    assert_eq!(s.flags.measure, Measure::Default);
    assert_eq!(s.warnings.len(), 1);
    assert!(
        s.warnings[0].contains("m-flag-contents-sideways"),
        "{}",
        s.warnings[0]
    );
    assert!(!s.html.contains("m-flag"));
}

#[test]
fn a_marker_naming_no_flag_warns_and_is_stripped() {
    let s = scan("<p>A.</p><span class=\"m-flag\"></span><p>B.</p>");
    assert_eq!(
        (s.flags.contents, s.flags.measure),
        (true, Measure::Default)
    );
    assert_eq!(s.html, "<p>A.</p><p>B.</p>");
    assert_eq!(s.warnings.len(), 1);
    assert!(s.warnings[0].contains("names no flag"), "{}", s.warnings[0]);
}

#[test]
fn a_marker_holding_text_sets_nothing_and_keeps_its_text() {
    let html = format!("<p>A.</p><span class=\"m-flag m-flag-contents-off\">kept</span>{WIDE}");
    let s = scan(&html);
    assert!(s.flags.contents, "a non-empty marker never votes");
    assert_eq!(s.flags.measure, Measure::Wide);
    assert_eq!(s.html, "<p>A.</p>kept");
    assert_eq!(s.warnings.len(), 1);
    assert!(s.warnings[0].contains("holds text"), "{}", s.warnings[0]);
}

#[test]
fn an_unclosed_marker_warns_and_is_left_in_place() {
    let html = "<p>A.</p><span class=\"m-flag m-flag-contents-off\">";
    let s = scan(html);
    assert!(s.flags.contents);
    assert_eq!(s.html, html);
    assert_eq!(s.warnings.len(), 1);
    assert!(s.warnings[0].contains("unclosed"), "{}", s.warnings[0]);
}

#[test]
fn markers_in_comments_and_raw_text_are_inert() {
    let html = "<!-- <span class=\"m-flag m-flag-contents-off\"></span> -->\
        <script>var m = '<span class=\"m-flag m-flag-measure-wide\"></span>';</script>\
        <p>Text.</p>";
    let s = scan(html);
    assert_eq!(
        (s.flags.contents, s.flags.measure),
        (true, Measure::Default)
    );
    assert_eq!(s.html, html);
    assert!(s.warnings.is_empty());
}

#[test]
fn quoted_brackets_in_attributes_do_not_confuse_the_scan() {
    let html = "<p title=\"a < b\">One.</p><span class=\"m-flag m-flag-contents-off\" data-x=\"a>b\"></span>";
    let s = scan(html);
    assert!(!s.flags.contents);
    assert_eq!(s.html, "<p title=\"a < b\">One.</p>");
    assert!(s.warnings.is_empty());
}

#[test]
fn omit_contents_drops_only_the_nav() {
    let article = "<article class=\"ltx_document\"><h1>T</h1>\
        <nav class=\"m-contents\" aria-label=\"Contents\"><h2 class=\"m-contents-title\">Contents</h2>\
        <ol><li><a href=\"#S1\">One</a></li></ol></nav><section id=\"S1\"><h2>One</h2></section></article>";
    let out = omit_contents(article);
    assert!(!out.contains("m-contents"));
    assert!(out.contains("<section id=\"S1\">"));
    assert!(out.contains("<h1>T</h1>"));
}

#[test]
fn omit_contents_without_a_nav_changes_nothing() {
    let article = "<article class=\"ltx_document\"><h1>T</h1></article>";
    assert_eq!(omit_contents(article), article);
}

#[test]
fn measure_tokens_map_to_closed_widths() {
    assert_eq!(Measure::Narrow.token(), "narrow");
    assert_eq!(Measure::Default.token(), "default");
    assert_eq!(Measure::Wide.token(), "wide");
    assert_eq!(Measure::Narrow.width(), Some("56ch"));
    assert_eq!(Measure::Default.width(), None);
    assert_eq!(Measure::Wide.width(), Some("80ch"));
    assert_eq!(Measure::Full.width(), Some("100%"));
    assert_eq!(Measure::Full.token(), "full");
}
