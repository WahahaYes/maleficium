use super::*;
use crate::widgets::parse_sidecar;

/// The engine's conversion of two live equations (liveequation with a
/// declared k, liveequation* with a result before x and x repeated), from
/// the binding in engine/embedded; one \eqref paragraph trimmed.
const CONVERTED: &str = include_str!("../fixtures/liveeq-converted.frag");
const SIDECAR: &str = "mfw 1
liveeq|eq-decay|eq:decay|1|inline|Exponential decay
param|eq-decay|k|0.5|0|2|0.05|2|0.50
result|eq-decay|1|1|22.3|100*exp(-3*k)
liveeq|eq-2|||inline|
param|eq-2|x|1|0|10|1|0|1
result|eq-2|1|2|-0.25|ln(x)-0.25
";

fn eqs(sidecar: &str) -> Vec<Equation> {
    parse_sidecar(sidecar).unwrap().equations
}

#[test]
fn the_pdf_values_replace_the_converter_stand_ins() {
    let html = apply(CONVERTED, &eqs(SIDECAR)).unwrap();
    assert!(
        html.contains("<mn class=\"m-param m-param-k\">0.50</mn>"),
        "{html}"
    );
    assert!(
        html.contains("<mn class=\"m-result m-result-1\">22.3</mn>"),
        "{html}"
    );
    // A repeated param shows its value too; a sign is a math minus.
    assert_eq!(
        html.matches("<mn class=\"m-param m-param-x\">1</mn>")
            .count(),
        2,
        "{html}"
    );
    assert!(
        html.contains("<mn class=\"m-result m-result-1\">\u{2212}0.25</mn>"),
        "{html}"
    );
    assert!(!html.contains(RESULT_TOKEN), "{html}");
}

#[test]
fn the_math_is_tagged_and_its_alttext_shows_the_values() {
    let html = apply(CONVERTED, &eqs(SIDECAR)).unwrap();
    assert!(
        html.contains("class=\"m-liveeq m-liveeq-eq-decay ltx_Math\""),
        "{html}"
    );
    assert!(
        html.contains("class=\"m-liveeq m-liveeq-eq-2 ltx_Math\""),
        "{html}"
    );
    assert!(
        html.contains("alttext=\"N(t)=N_{0}e^{-0.50\\,t},\\qquad N(3)=22.3\""),
        "{html}"
    );
    assert!(
        html.contains("alttext=\"y=-0.25\\quad\\mbox{at}\\quad x=1+1\""),
        "{html}"
    );
    assert!(!html.contains("mfwleq"), "{html}");
}

#[test]
fn a_disagreeing_sidecar_leaves_question_marks_and_says_why() {
    let one: String = SIDECAR.lines().take(4).collect::<Vec<_>>().join("\n");
    let (html, why) = apply(CONVERTED, &eqs(&one)).unwrap_err();
    assert!(
        why.contains("2 live equations but the sidecar records 1"),
        "{why}"
    );
    assert_eq!(html.matches("<mn class=\"m-result\">?</mn>").count(), 2);
    let renamed = SIDECAR.replace("|x|", "|y|").replace("ln(x)", "ln(y)");
    let (_, why) = apply(CONVERTED, &eqs(&renamed)).unwrap_err();
    assert!(
        why.contains("equation eq-2: the article names param `x`"),
        "{why}"
    );
}

#[test]
fn an_article_without_live_equations_is_unchanged() {
    let plain = include_str!("../fixtures/interactive-converted.frag");
    assert_eq!(apply(plain, &[]).unwrap(), plain);
}
