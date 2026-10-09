//! The paper's theme: the token values the reader page, its chrome and the
//! widgets all take, from one record. `maleficium-interactive.sty` defines
//! the token set with the house values, follows the document's page colour
//! and fonts, and writes the result into the `<jobname>.mfw` sidecar as
//! `theme|<mode>|<token>|<value>` lines ([`Lines`] reads them). The
//! exporter renders the page's CSS from it ([`Theme::css`]), the reader
//! hands the same values to every widget in its `init` and `theme`
//! messages ([`Theme::json`]), and the poster renderer gives a widget the
//! light set ([`Theme::tokens`]). A paper without the package has no record
//! and reads in [`Theme::house`].
//!
//! Token values land in CSS, so they are a trust boundary: every value is
//! checked against an allow-list for its token's kind ([`value_ok`]) when
//! the record is read, and again when CSS is written. Names outside
//! [`TOKENS`] are refused.

use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// What a token's value may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, or `rgb()`/`rgba()`/`hsl()`/
    /// `hsla()` of plain numbers.
    Color,
    /// A font-family list: quoted names of letters, digits, spaces and
    /// dashes, and bare family keywords.
    Font,
    /// A number with a CSS length unit, or `min(<length>, <length>)`.
    Length,
    /// A plain number (a ratio or a line height).
    Number,
    /// One box shadow: two to four lengths and a colour.
    Shadow,
}

/// One token of the set: its name, its kind, the house light value, and
/// the house dark value when the token changes with the mode.
#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub name: &'static str,
    pub kind: Kind,
    pub light: &'static str,
    pub dark: Option<&'static str>,
}

const fn shared(name: &'static str, kind: Kind, v: &'static str) -> Token {
    Token {
        name,
        kind,
        light: v,
        dark: None,
    }
}

const fn moded(name: &'static str, kind: Kind, l: &'static str, d: &'static str) -> Token {
    Token {
        name,
        kind,
        light: l,
        dark: Some(d),
    }
}

const fn col(name: &'static str, l: &'static str, d: &'static str) -> Token {
    moded(name, Kind::Color, l, d)
}

/// Every token, in the order the package writes them, with the house
/// values: what the package records for a document in the house fonts on a
/// white page (pinned against a compiled record in the tests).
pub const TOKENS: &[Token] = &[
    shared(
        "--m-font-body",
        Kind::Font,
        "\"Libertinus Serif\", Georgia, serif",
    ),
    shared(
        "--m-font-heading",
        Kind::Font,
        "\"Libertinus Sans\", system-ui, sans-serif",
    ),
    shared(
        "--m-font-mono",
        Kind::Font,
        "\"Libertinus Mono\", ui-monospace, monospace",
    ),
    shared("--m-font-math", Kind::Font, "\"Libertinus Math\", math"),
    shared("--m-size-base", Kind::Length, "1.0625rem"),
    shared("--m-scale", Kind::Number, "1.2"),
    shared("--m-size-small", Kind::Length, "0.875em"),
    shared("--m-leading", Kind::Number, "1.6"),
    shared("--m-measure", Kind::Length, "68ch"),
    shared("--m-figure-max", Kind::Length, "min(100%, 64rem)"),
    shared("--m-space-1", Kind::Length, "0.25rem"),
    shared("--m-space-2", Kind::Length, "0.5rem"),
    shared("--m-space-3", Kind::Length, "1rem"),
    shared("--m-space-4", Kind::Length, "1.5rem"),
    shared("--m-space-5", Kind::Length, "2.5rem"),
    shared("--m-space-6", Kind::Length, "4rem"),
    shared("--m-radius", Kind::Length, "6px"),
    col("--m-color-bg", "#FBFAF7", "#16181D"),
    col("--m-color-surface", "#F1EFE9", "#1F2229"),
    col("--m-color-text", "#1E1B24", "#DFE1E6"),
    col("--m-color-muted", "#6B6676", "#9AA1AD"),
    col("--m-color-rule", "#D9D5E0", "#343944"),
    col("--m-color-link", "#5B2A86", "#B79AD4"),
    col("--m-color-accent", "#5B2A86", "#B79AD4"),
    col("--m-color-target", "#FFF4C2", "#3A3420"),
    col("--m-color-error", "#B3261E", "#FF8A80"),
    col("--m-color-error-bg", "#FDECEA", "#3A2220"),
    col("--m-color-warning", "#8A5A00", "#F0B73C"),
    col("--m-tooltip-bg", "#FFFFFF", "#262A32"),
    col("--m-tooltip-fg", "#1E1B24", "#DFE1E6"),
    col("--m-tooltip-border", "#D9D5E0", "#434957"),
    moded(
        "--m-shadow",
        Kind::Shadow,
        "0 2px 8px rgba(30, 27, 36, 0.16)",
        "0 2px 10px rgba(0, 0, 0, 0.5)",
    ),
    col("--m-figure-bg", "#FFFFFF", "#1C1F25"),
    col("--m-figure-surface", "#F0F0F0", "#2A2C32"),
    col("--m-figure-ink", "#1E1B24", "#DFE1E6"),
    col("--m-figure-accent", "#5B2A86", "#B79AD4"),
    col("--m-cat-1", "#2A78D6", "#3987E5"),
    col("--m-cat-2", "#EB6834", "#D95926"),
    col("--m-cat-3", "#1BAF7A", "#199E70"),
    col("--m-cat-4", "#EDA100", "#C98500"),
    col("--m-cat-5", "#E87BA4", "#D55181"),
    col("--m-cat-6", "#008300", "#008300"),
    col("--m-cat-7", "#4A3AA7", "#9085E9"),
    col("--m-cat-8", "#E34948", "#E66767"),
    col("--m-seq-1", "#CDE2FB", "#0D366B"),
    col("--m-seq-2", "#86B6EF", "#1C5CAB"),
    col("--m-seq-3", "#3987E5", "#3987E5"),
    col("--m-seq-4", "#1C5CAB", "#86B6EF"),
    col("--m-seq-5", "#0D366B", "#CDE2FB"),
    col("--m-div-low", "#256ABF", "#3987E5"),
    col("--m-div-mid", "#F0EFEC", "#383835"),
    col("--m-div-high", "#E34948", "#E66767"),
];

/// The longest value accepted for any token.
pub const MAX_VALUE_LEN: usize = 200;

fn token(name: &str) -> Option<&'static Token> {
    TOKENS.iter().find(|t| t.name == name)
}

fn re(cell: &'static OnceLock<regex::Regex>, pattern: &str) -> &'static regex::Regex {
    cell.get_or_init(|| regex::Regex::new(pattern).expect("a static pattern compiles"))
}

const NUM: &str = r"-?(?:\d{1,4}(?:\.\d{1,4})?|\.\d{1,4})";
const UNIT: &str = r"(?:px|rem|em|ch|pt|vw|vh|%)";

fn length_ok(v: &str) -> bool {
    static ONE: OnceLock<regex::Regex> = OnceLock::new();
    static MIN: OnceLock<regex::Regex> = OnceLock::new();
    let one = format!(r"^(?:0|{NUM}{UNIT})$");
    let len = format!(r"(?:0|{NUM}{UNIT})");
    let min = format!(r"^min\({len}, ?{len}\)$");
    re(&ONE, &one).is_match(v) || re(&MIN, &min).is_match(v)
}

fn color_ok(v: &str) -> bool {
    static HEX: OnceLock<regex::Regex> = OnceLock::new();
    static FUNC: OnceLock<regex::Regex> = OnceLock::new();
    if let Some(h) = v.strip_prefix('#') {
        return re(&HEX, r"^[0-9A-Fa-f]+$").is_match(h) && matches!(h.len(), 3 | 4 | 6 | 8);
    }
    let arg = r"(?:\d{1,3}(?:\.\d{1,4})?|\.\d{1,4})(?:%|deg)?";
    let func = format!(r"^(?:rgb|rgba|hsl|hsla)\({arg}(?:(?:, ?| | ?/ ?){arg}){{2,3}}\)$");
    re(&FUNC, &func).is_match(v)
}

fn font_ok(v: &str) -> bool {
    static ITEM: OnceLock<regex::Regex> = OnceLock::new();
    let item = re(
        &ITEM,
        r#"^(?:"[A-Za-z0-9][A-Za-z0-9 -]{0,39}"|[A-Za-z][A-Za-z0-9-]{0,39})$"#,
    );
    let items: Vec<&str> = v.split(", ").collect();
    items.len() <= 8 && items.iter().all(|i| item.is_match(i))
}

fn number_ok(v: &str) -> bool {
    static N: OnceLock<regex::Regex> = OnceLock::new();
    re(&N, r"^(?:\d{1,3}(?:\.\d{1,4})?|\.\d{1,4})$").is_match(v)
}

fn shadow_ok(v: &str) -> bool {
    if v == "none" {
        return true;
    }
    // The colour is the last part: from the `#` or the colour function.
    let at = v
        .find('#')
        .or_else(|| v.find("rgb"))
        .or_else(|| v.find("hsl"));
    let Some(at) = at else { return false };
    let (lens, colour) = v.split_at(at);
    let lens: Vec<&str> = lens.split(' ').filter(|s| !s.is_empty()).collect();
    lens.len() >= 2
        && lens.len() <= 4
        && lens.iter().all(|l| length_ok(l))
        && v[..at].ends_with(' ')
        && !v[..at].contains("  ")
        && color_ok(colour)
}

/// Whether `v` is an allowed value of kind `kind`: at most
/// [`MAX_VALUE_LEN`] bytes, and of the kind's exact shape. Nothing that can
/// end a declaration or a rule, open a comment, or name a resource gets
/// through: no `;`, braces, `<`, backslash, single quote, `url(`, newline,
/// or a function outside the colour and `min()` forms.
pub fn value_ok(kind: Kind, v: &str) -> bool {
    if v.is_empty() || v.len() > MAX_VALUE_LEN || !v.is_ascii() {
        return false;
    }
    match kind {
        Kind::Color => color_ok(v),
        Kind::Font => font_ok(v),
        Kind::Length => length_ok(v),
        Kind::Number => number_ok(v),
        Kind::Shadow => shadow_ok(v),
    }
}

/// Every token's value in both modes. Built only from checked values:
/// [`Theme::house`], or a record read by [`Lines`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    light: BTreeMap<String, String>,
    dark: BTreeMap<String, String>,
    /// The author's reader stylesheet (`\maleficiumreaderstyle`), relative
    /// to the main file's folder; read and checked by the exporter
    /// ([`crate::reader_style`]), never trusted from here.
    pub style: Option<String>,
}

/// The two colour modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

impl Theme {
    /// The house values, for a paper with no theme record.
    pub fn house() -> Self {
        let light: BTreeMap<String, String> = TOKENS
            .iter()
            .map(|t| (t.name.to_string(), t.light.to_string()))
            .collect();
        let dark = TOKENS
            .iter()
            .map(|t| (t.name.to_string(), t.dark.unwrap_or(t.light).to_string()))
            .collect();
        Self {
            light,
            dark,
            style: None,
        }
    }

    /// The theme one widget gets with its own plate (`plate=` on the widget):
    /// `plate` behind it in both modes, a surface stepped from it, and the
    /// ink, accent and data colours of whichever mode reads on it (dark when
    /// its gray is under 0.45, the package's own threshold). `plate` is
    /// `#rrggbb`.
    pub fn with_plate(&self, plate: &str) -> Result<Theme, String> {
        let rgb = hex_rgb(plate).ok_or_else(|| format!("plate `{plate}` is not #rrggbb"))?;
        let dark = plate_mode(plate) == Some(Mode::Dark);
        let mut tokens = self
            .tokens(if dark { Mode::Dark } else { Mode::Light })
            .clone();
        let toward = if dark { 255.0 } else { 0.0 };
        let surface: Vec<f64> = rgb.iter().map(|c| c * 0.94 + toward * 0.06).collect();
        tokens.insert("--m-figure-bg".into(), plate.to_ascii_uppercase());
        tokens.insert("--m-figure-surface".into(), rgb_hex(&surface));
        Ok(Theme {
            light: tokens.clone(),
            dark: tokens,
            style: None,
        })
    }

    /// Every token's value in `mode`.
    pub fn tokens(&self, mode: Mode) -> &BTreeMap<String, String> {
        match mode {
            Mode::Light => &self.light,
            Mode::Dark => &self.dark,
        }
    }

    /// `{light: {token: value}, dark: {...}}`: what the reader page hands
    /// its widgets.
    pub fn json(&self) -> Value {
        let map = |m: &BTreeMap<String, String>| {
            Value::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), Value::from(v.as_str())))
                    .collect::<Map<_, _>>(),
            )
        };
        serde_json::json!({ "light": map(&self.light), "dark": map(&self.dark) })
    }

    /// The page's token CSS: every token on `.m-reader`, the ones that
    /// change in dark mode again under `[data-theme="dark"]`, and
    /// `--m-paper`, the light plate the page's own pictures (graphics and
    /// widget posters) keep in both modes. Every value is checked again
    /// here; one that fails is an error, never written.
    pub fn css(&self) -> Result<String, String> {
        let decls = |m: &BTreeMap<String, String>, only_moded: bool| -> Result<String, String> {
            let mut out = String::new();
            for t in TOKENS {
                if only_moded && t.dark.is_none() {
                    continue;
                }
                let v = m
                    .get(t.name)
                    .ok_or_else(|| format!("the theme has no value for {}", t.name))?;
                if !value_ok(t.kind, v) {
                    return Err(format!("the theme's value for {} is not allowed", t.name));
                }
                out.push_str(&format!("{}:{v};", t.name));
            }
            Ok(out)
        };
        let paper = &self.light["--m-figure-bg"];
        Ok(format!(
            ".m-reader{{{}--m-paper:{paper}}}\n.m-reader[data-theme=\"dark\"]{{{}}}\n",
            decls(&self.light, false)?,
            decls(&self.dark, true)?.trim_end_matches(';'),
        ))
    }
}

/// The theme lines of one sidecar, gathered as they are read. Light lines
/// must give every token once; dark lines every token that changes with
/// the mode, once, and nothing else.
#[derive(Debug, Default)]
pub struct Lines {
    light: BTreeMap<String, String>,
    dark: BTreeMap<String, String>,
}

impl Lines {
    /// One `theme|<mode>|<token>|<value>` line, split into its fields after
    /// the tag. `n` is its line number, for the error.
    pub fn add(&mut self, n: usize, fields: &[&str]) -> Result<(), String> {
        let [mode, name, value] = fields else {
            return Err(format!(
                "widget sidecar line {n}: a theme line has {} fields, expected 4",
                fields.len() + 1
            ));
        };
        let t = token(name)
            .ok_or_else(|| format!("widget sidecar line {n}: unknown theme token `{name}`"))?;
        let map = match *mode {
            "light" => &mut self.light,
            "dark" if t.dark.is_some() => &mut self.dark,
            "dark" => return Err(format!("widget sidecar line {n}: {name} has no dark value")),
            _ => {
                return Err(format!(
                    "widget sidecar line {n}: unknown theme mode `{mode}`"
                ))
            }
        };
        if !value_ok(t.kind, value) {
            return Err(format!(
                "widget sidecar line {n}: the value of {name} is not an allowed {:?}",
                t.kind
            ));
        }
        if map.insert(name.to_string(), value.to_string()).is_some() {
            return Err(format!(
                "widget sidecar line {n}: {name} is given twice for {mode}"
            ));
        }
        Ok(())
    }

    /// The theme, or None when the sidecar had no theme lines.
    pub fn finish(self) -> Result<Option<Theme>, String> {
        if self.light.is_empty() && self.dark.is_empty() {
            return Ok(None);
        }
        let missing: Vec<&str> = TOKENS
            .iter()
            .filter(|t| {
                !self.light.contains_key(t.name)
                    || (t.dark.is_some() && !self.dark.contains_key(t.name))
            })
            .map(|t| t.name)
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "the widget sidecar's theme lacks {}; recompile the document",
                missing.join(", ")
            ));
        }
        let mut dark = self.light.clone();
        dark.extend(self.dark);
        Ok(Some(Theme {
            light: self.light,
            dark,
            style: None,
        }))
    }
}

/// The mode whose ink reads on `plate` (`#rrggbb`): dark when its gray is
/// under 0.45, the package's own threshold.
pub fn plate_mode(plate: &str) -> Option<Mode> {
    let c = hex_rgb(plate)?;
    let gray = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
    Some(if gray < 0.45 * 255.0 {
        Mode::Dark
    } else {
        Mode::Light
    })
}

/// `#rrggbb` as three channels, 0 to 255.
fn hex_rgb(v: &str) -> Option<[f64; 3]> {
    let h = v.strip_prefix('#')?;
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let ch = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(f64::from);
    Some([ch(0)?, ch(2)?, ch(4)?])
}

fn rgb_hex(c: &[f64]) -> String {
    let b = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]))
}

#[cfg(test)]
mod tests;
