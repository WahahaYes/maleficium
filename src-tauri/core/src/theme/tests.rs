// The records under testdata/theme/ are real: each `.mfw` is what the
// engine wrote compiling the `.tex` beside it with the package
// (`maleficium-engine compile <name>.tex`, the package copied beside it).
// Recompile them whenever the package's theme tables change.
use super::*;
use crate::widgets::parse_sidecar;

const HOUSE: &str = include_str!("../../testdata/theme/house.mfw");
const DARK_PAGE: &str = include_str!("../../testdata/theme/dark-page.mfw");
const CREAM: &str = include_str!("../../testdata/theme/cream-times.mfw");
const STY: &str = include_str!("../../../../embed-runtime/tex/maleficium-interactive.sty");

fn theme_of(sidecar: &str) -> Theme {
    parse_sidecar(sidecar).unwrap().theme.unwrap()
}

#[test]
fn the_house_theme_is_what_the_package_records_on_a_white_page_in_the_house_fonts() {
    assert_eq!(theme_of(HOUSE), Theme::house());
}

#[test]
fn the_package_tables_are_the_house_values() {
    let hex = |s: &str| format!("#{s}");
    let pat = regex::Regex::new(
        r"(?m)^\\mfw@(tok|col|mtok|fig)\{(--m-[a-z0-9-]+)\}\{([^}]*)\}(?:\{([^}]*)\})?",
    )
    .unwrap();
    let mut seen = 0;
    for c in pat.captures_iter(STY) {
        let t = token(&c[2]).unwrap_or_else(|| panic!("{} is not a token", &c[2]));
        let (l, d) = match &c[1] {
            "tok" => (c[3].replace("\\mfw@pct", "%"), None),
            "mtok" => (c[3].to_string(), Some(c[4].to_string())),
            // A figure token's house light value is its light-plate value,
            // its dark one the dark-plate value (the house dark plate is dark).
            _ => (hex(&c[3]), Some(hex(&c[4]))),
        };
        assert_eq!(t.light, l, "{}", t.name);
        assert_eq!(t.dark.map(str::to_string), d, "{}", t.name);
        seen += 1;
    }
    // The fonts and the plate itself (bg, surface) are computed, not tabled.
    assert_eq!(seen, TOKENS.len() - 5);
}

#[test]
fn a_dark_page_is_the_plate_in_both_modes_with_light_ink_on_it() {
    let t = theme_of(DARK_PAGE);
    for m in [Mode::Light, Mode::Dark] {
        assert_eq!(t.tokens(m)["--m-figure-bg"], "#1A1F33");
        assert_eq!(t.tokens(m)["--m-figure-ink"], "#DFE1E6");
    }
    assert_eq!(
        t.tokens(Mode::Light)["--m-font-body"],
        "\"Latin Modern Roman\", Georgia, serif"
    );
    // The chrome keeps the house light page.
    assert_eq!(t.tokens(Mode::Light)["--m-color-bg"], "#FBFAF7");
}

#[test]
fn a_light_page_colour_is_the_light_plate_and_the_fonts_follow_the_document() {
    let t = theme_of(CREAM);
    assert_eq!(t.tokens(Mode::Light)["--m-figure-bg"], "#FFF8E7");
    assert_eq!(t.tokens(Mode::Light)["--m-figure-ink"], "#1E1B24");
    assert_eq!(t.tokens(Mode::Dark)["--m-figure-bg"], "#1C1F25");
    assert!(t.tokens(Mode::Light)["--m-font-body"].starts_with("\"TeX Gyre Termes\""));
    assert!(t.css().unwrap().contains("--m-paper:#FFF8E7}"));
}

fn lum(hex: &str) -> f64 {
    let h = hex.trim_start_matches('#');
    let c = |i: usize| {
        let v = f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap()) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * c(0) + 0.7152 * c(2) + 0.0722 * c(4)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

#[test]
fn text_on_the_plate_and_the_chrome_reads_in_every_record_and_mode() {
    for (name, t) in [
        ("house", Theme::house()),
        ("dark page", theme_of(DARK_PAGE)),
        ("cream", theme_of(CREAM)),
    ] {
        for m in [Mode::Light, Mode::Dark] {
            let v = t.tokens(m);
            let pairs = [
                ("--m-figure-ink", "--m-figure-bg", 4.5),
                ("--m-figure-ink", "--m-figure-surface", 4.5),
                ("--m-figure-accent", "--m-figure-bg", 3.0),
                ("--m-tooltip-fg", "--m-tooltip-bg", 4.5),
                ("--m-color-text", "--m-color-bg", 4.5),
                ("--m-color-error", "--m-color-bg", 4.5),
                ("--m-color-error", "--m-color-error-bg", 4.5),
                ("--m-color-warning", "--m-color-bg", 4.5),
            ];
            for (fg, bg, min) in pairs {
                let r = contrast(&v[fg], &v[bg]);
                assert!(r >= min, "{name} {m:?}: {fg} on {bg} is {r:.2}");
            }
        }
    }
}

#[test]
fn the_css_carries_every_token_and_the_dark_overrides() {
    let css = Theme::house().css().unwrap();
    let (light, dark) = css.split_once("\n.m-reader[data-theme=\"dark\"]").unwrap();
    for t in TOKENS {
        assert!(
            light.contains(&format!("{}:{};", t.name, t.light)),
            "{}",
            t.name
        );
        if let Some(d) = t.dark {
            assert!(dark.contains(&format!("{}:{d}", t.name)), "{}", t.name);
        } else {
            assert!(!dark.contains(t.name), "{}", t.name);
        }
    }
    assert!(light.contains("--m-paper:#FFFFFF}"));
}

const HOSTILE: &[&str] = &[
    "url(https://example.org/x)",
    "url(#x)",
    "expression(alert(1))",
    "#fff;}body{display:none",
    "#fff}",
    "{",
    "'Libertinus'",
    "\"a\";x:y",
    "\"a\\\"b\"",
    "\"</style><script>alert(1)</script>\"",
    "</style>",
    "#fff\n.x{}",
    "1rem\n",
    "rgb(1,2,url(x))",
    "rgb(1, 2, 3);",
    "var(--m-color-bg)",
    "calc(1px + 1px)",
    "min(1px, url(x))",
    "#ffg",
    "#12345",
    "/* */",
    "@import",
    "\u{a0}1rem",
    "",
];

#[test]
fn hostile_values_are_refused_for_every_kind() {
    for kind in [
        Kind::Color,
        Kind::Font,
        Kind::Length,
        Kind::Number,
        Kind::Shadow,
    ] {
        for v in HOSTILE {
            assert!(!value_ok(kind, v), "{kind:?} accepted {v:?}");
        }
        assert!(!value_ok(kind, &"1".repeat(MAX_VALUE_LEN + 1)));
    }
    assert!(!value_ok(
        Kind::Font,
        "\"a\", \"b\", \"c\", \"d\", \"e\", \"f\", \"g\", \"h\", i"
    ));
    assert!(
        !value_ok(Kind::Color, "red"),
        "named colours are not in the list"
    );
    assert!(!value_ok(Kind::Shadow, "0 0 url(#x)"));
    assert!(!value_ok(Kind::Shadow, "0 #fff"));
    assert!(!value_ok(Kind::Shadow, "0 0 0 0 0 #fff"));
}

#[test]
fn the_allowed_forms_pass() {
    for v in [
        "#fff",
        "#ffff",
        "#1E1B24",
        "#1e1b2480",
        "rgb(1, 2, 3)",
        "rgba(1,2,3,0.5)",
        "rgb(10% 20% 30% / 50%)",
        "hsl(210 50% 40%)",
        "hsla(210deg, 50%, 40%, .5)",
    ] {
        assert!(value_ok(Kind::Color, v), "{v}");
    }
    for v in [
        "\"Libertinus Serif\", Georgia, serif",
        "serif",
        "\"TeX Gyre Termes\", \"Times New Roman\", Times",
    ] {
        assert!(value_ok(Kind::Font, v), "{v}");
    }
    for v in [
        "0",
        "1.0625rem",
        "68ch",
        "-2px",
        "min(100%, 64rem)",
        "min(1px,2px)",
    ] {
        assert!(value_ok(Kind::Length, v), "{v}");
    }
    for v in ["1.2", "1", ".5"] {
        assert!(value_ok(Kind::Number, v), "{v}");
    }
    for v in ["0 2px 8px rgba(30, 27, 36, 0.16)", "1px 1px #000", "none"] {
        assert!(value_ok(Kind::Shadow, v), "{v}");
    }
    for t in TOKENS {
        assert!(value_ok(t.kind, t.light), "{}", t.name);
        assert!(t.dark.is_none_or(|d| value_ok(t.kind, d)), "{}", t.name);
    }
}

fn record_with(line: &str) -> String {
    format!("{HOUSE}{line}\n")
}

#[test]
fn malformed_theme_lines_are_errors_naming_the_line() {
    let cases = [
        ("theme|light|--m-color-bg", "fields"),
        ("theme|light|--m-color-bg|#fff|x", "fields"),
        ("theme|sepia|--m-color-bg|#fff", "unknown theme mode"),
        ("theme|light|--m-colour-bg|#fff", "unknown theme token"),
        ("theme|light|color|red", "unknown theme token"),
        ("theme|dark|--m-font-body|serif", "has no dark value"),
        ("theme|light|--m-color-bg|#fff", "given twice"),
        ("theme|dark|--m-color-bg|#000", "given twice"),
    ];
    for (line, want) in cases {
        let e = parse_sidecar(&record_with(line)).unwrap_err();
        assert!(e.contains(want) && e.contains("line"), "{line}: {e}");
    }
    for v in HOSTILE {
        let text = HOUSE.replace(
            "theme|light|--m-color-bg|#FBFAF7",
            &format!("theme|light|--m-color-bg|{v}"),
        );
        if text.lines().count() != HOUSE.lines().count() {
            // A newline in the value splits the line: still refused.
            assert!(parse_sidecar(&text).is_err(), "{v:?}");
            continue;
        }
        let e = parse_sidecar(&text).unwrap_err();
        assert!(
            e.contains("--m-color-bg") || e.contains("fields"),
            "{v:?}: {e}"
        );
    }
    let e = parse_sidecar(&HOUSE.replace("theme|dark|--m-figure-ink|#DFE1E6\n", "")).unwrap_err();
    assert!(e.contains("lacks --m-figure-ink"), "{e}");
    assert_eq!(parse_sidecar("mfw 1\n").unwrap().theme, None);
}

#[test]
fn a_widget_plate_is_its_backdrop_in_both_modes_with_ink_that_reads_on_it() {
    let house = Theme::house();
    let light = house.with_plate("#faf3e8").unwrap();
    for mode in [Mode::Light, Mode::Dark] {
        let t = light.tokens(mode);
        assert_eq!(t["--m-figure-bg"], "#FAF3E8");
        assert_eq!(
            t["--m-figure-ink"],
            house.tokens(Mode::Light)["--m-figure-ink"]
        );
    }
    assert_eq!(light.tokens(Mode::Light)["--m-figure-surface"], "#EBE4DA");
    let dark = house.with_plate("#101820").unwrap();
    assert_eq!(
        dark.tokens(Mode::Light)["--m-figure-ink"],
        house.tokens(Mode::Dark)["--m-figure-ink"]
    );
    assert_eq!(dark.tokens(Mode::Dark)["--m-figure-bg"], "#101820");
    assert!(dark.css().is_ok(), "every value still passes the checks");
    for bad in ["faf3e8", "#fff", "#gggggg", "red"] {
        assert!(house.with_plate(bad).is_err(), "{bad}");
    }
}
