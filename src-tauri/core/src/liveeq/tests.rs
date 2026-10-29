use super::*;
use crate::widgets::parse_sidecar;

/// The formula language's shared cases (vitest and e2e/liveeq-run.sh run
/// the same file): every value's formula is accepted, every reject refused.
#[test]
fn formulas_follow_the_shared_cases() {
    let v: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../embed-runtime/src/expr.vectors.json"
    ))
    .unwrap();
    for c in v["values"].as_array().unwrap() {
        let src = c["expr"].as_str().unwrap();
        let names: Vec<&str> = c["vars"]
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(check_formula(src, &names), Ok(()), "{src}");
    }
    for c in v["rejects"].as_array().unwrap() {
        let src = c["expr"].as_str().unwrap();
        let names: Vec<&str> = c["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        assert!(check_formula(src, &names).is_err(), "{src} was accepted");
    }
}

#[test]
fn hostile_formulas_are_refused_without_overflowing() {
    assert!(check_formula(&"1+".repeat(MAX_LENGTH), &[]).is_err());
    let deep = format!(
        "{}1{}",
        "(".repeat(MAX_DEPTH + 1),
        ")".repeat(MAX_DEPTH + 1)
    );
    assert_eq!(
        check_formula(&deep, &[]),
        Err("formula is nested too deeply".into())
    );
    assert!(check_formula(&format!("{}1", "-".repeat(MAX_DEPTH + 1)), &[]).is_err());
}

const DECAY: &str = "mfw 1
liveeq|eq-decay|eq:decay|1|inline|Exponential decay with a live rate k
param|eq-decay|k|0.5|0|2|0.05|2|0.50
result|eq-decay|1|1|22.3|100*exp(-3*k)
liveeq|eq-2|||widget|
param|eq-2|x|1|0|10|1|0|1
result|eq-2|1|2|0.00|x<1 || x>2
";

#[test]
fn the_sidecar_carries_live_equations() {
    let s = parse_sidecar(DECAY).unwrap();
    assert_eq!(s.equations.len(), 2);
    let e = &s.equations[0];
    assert_eq!((e.id.as_str(), e.mode), ("eq-decay", Mode::Inline));
    assert_eq!(e.label.as_deref(), Some("eq:decay"));
    assert_eq!(e.number.as_deref(), Some("1"));
    assert_eq!(e.params[0].shown, "0.50");
    assert_eq!((e.params[0].default, e.params[0].places), (0.5, 2));
    assert_eq!(e.results[0].formula, "100*exp(-3*k)");
    let e = &s.equations[1];
    assert_eq!(
        (e.number.clone(), e.alt.clone(), e.mode),
        (None, None, Mode::Widget)
    );
    // The formula is the last field: its || stays whole.
    assert_eq!(e.results[0].formula, "x<1 || x>2");
}

#[test]
fn malformed_live_equation_records_are_refused() {
    let widget = "widget|eq-decay|model|model@1|||house|figures/mesh.png|model=models/mesh.glb|height=1pt|Alt";
    for (bad, why) in [
        ("liveeq|Eq|||inline|", "invalid equation id"),
        ("liveeq|eq-a|||popup|", "mode `popup`"),
        ("liveeq|eq-a|||inline", "expected 6"),
        (
            "param|eq-a|k|0.5|0|2|0.05|2|0.50",
            "does not follow its liveeq",
        ),
        (
            "liveeq|eq-a|||inline|\nparam|eq-a|exp|1|0|2|1|0|1",
            "invalid param name",
        ),
        (
            "liveeq|eq-a|||inline|\nparam|eq-a|k|1e2|0|200|1|0|100",
            "not a plain decimal",
        ),
        (
            "liveeq|eq-a|||inline|\nparam|eq-a|k|3|0|2|1|0|3",
            "default in [min, max]",
        ),
        (
            "liveeq|eq-a|||inline|\nparam|eq-a|k|1|0|2|0|0|1",
            "step > 0",
        ),
        (
            "liveeq|eq-a|||inline|\nparam|eq-a|k|1|0|2|1|0|1<b>",
            "not a decimal",
        ),
        (
            "liveeq|eq-a|||inline|\nparam|eq-a|k|1|0|2|1|0|1\nresult|eq-a|2|0|1|k",
            "expected 1",
        ),
        (
            "liveeq|eq-a|||inline|\nparam|eq-a|k|1|0|2|1|0|1\nresult|eq-a|1|0|1|1<k<2",
            "chained",
        ),
        (
            "liveeq|eq-a|||inline|\nparam|eq-a|k|1|0|2|1|0|1\nresult|eq-a|1|0|1|abs(k,1)",
            "abs takes 1",
        ),
        (
            "liveeq|eq-a|||inline|\nresult|eq-a|1|0|1|1",
            "records no param",
        ),
        (
            "liveeq|eq-a|||inline|\nliveeq|eq-a|||inline|",
            "duplicate equation id",
        ),
    ] {
        let e = parse_sidecar(&format!("mfw 1\n{bad}\n")).unwrap_err();
        assert!(e.contains(why), "{bad}: {e}");
    }
    let clash =
        format!("mfw 1\n{widget}\nliveeq|eq-decay|||inline|\nparam|eq-decay|k|1|0|2|1|0|1\n");
    assert!(parse_sidecar(&clash)
        .unwrap_err()
        .contains("also a widget's"));
}
