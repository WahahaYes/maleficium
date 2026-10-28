//! Live equations: the sidecar records `liveequation` writes, and the app's
//! check of their formulas.
//!
//! The package (`maleficium-interactive.sty`, "live equations") typesets
//! each equation at its params' defaults and records, after it:
//!
//! ```text
//! liveeq|<id>|<label>|<number>|<mode>|<alt>
//! param|<id>|<name>|<default>|<min>|<max>|<step>|<places>|<shown>
//! result|<id>|<n>|<places>|<shown>|<formula>
//! ```
//!
//! `shown` is the text the PDF typeset, which the exporter puts in the
//! article's MathML (the converter cannot evaluate a formula). The formula
//! comes last because `||` is one of its operators.
//!
//! The formula language is the reader's (`embed-runtime/src/expr.ts`, which
//! holds the grammar). The package refuses what `l3fp` would misread, but
//! not structure `l3fp` reads anyway (arity, chained comparisons): that is
//! [`check_formula`], so a compile in the app reports every formula the
//! reader would refuse. The shared cases in `expr.vectors.json` hold the
//! three to one grammar.

#[cfg(test)]
mod tests;

/// How the reader runs an equation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The reader updates the article's equation in place.
    Inline,
    /// The equation runs as the `liveeq@1` widget.
    Widget,
}

/// A live equation as the sidecar records it.
#[derive(Debug, Clone, PartialEq)]
pub struct Equation {
    pub id: String,
    pub label: Option<String>,
    /// The typeset number; None for `liveequation*`.
    pub number: Option<String>,
    pub mode: Mode,
    pub alt: Option<String>,
    pub params: Vec<Param>,
    pub results: Vec<LiveResult>,
}

/// One param: its numbers as the author wrote them, parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// Decimals shown.
    pub places: u32,
    /// The default as the PDF shows it.
    pub shown: String,
}

/// One `\liveresult`, numbered from 1 in its equation.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveResult {
    pub n: u32,
    pub places: u32,
    pub shown: String,
    pub formula: String,
}

impl Equation {
    /// The bundle manifest's `equations` entry (schema `$defs/equation`).
    pub fn to_json(&self) -> serde_json::Value {
        let mut e = serde_json::Map::new();
        e.insert("id".into(), self.id.clone().into());
        for (k, v) in [
            ("label", &self.label),
            ("number", &self.number),
            ("alt", &self.alt),
        ] {
            if let Some(v) = v {
                e.insert(k.into(), v.clone().into());
            }
        }
        let mode = match self.mode {
            Mode::Inline => "inline",
            Mode::Widget => "widget",
        };
        e.insert("mode".into(), mode.into());
        e.insert(
            "params".into(),
            self.params
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "name": p.name, "default": p.default, "min": p.min, "max": p.max,
                        "step": p.step, "places": p.places,
                    })
                })
                .collect(),
        );
        e.insert(
            "results".into(),
            self.results
                .iter()
                .map(|r| serde_json::json!({ "n": r.n, "places": r.places, "formula": r.formula }))
                .collect(),
        );
        e.into()
    }
}

/// Most decimals a param or result may show.
pub const MAX_PLACES: u32 = 99;

/// Whether a sidecar record kind is a live equation's.
pub(crate) fn is_record(kind: &str) -> bool {
    matches!(kind, "liveeq" | "param" | "result")
}

/// Adds sidecar line `n` (a `liveeq`, `param` or `result` record) to `out`.
/// Params and results follow their equation's line.
pub(crate) fn add_record(out: &mut Vec<Equation>, n: usize, line: &str) -> Result<(), String> {
    let at = |e: String| format!("widget sidecar line {n}: {e}");
    let kind = line.split('|').next().unwrap_or("");
    match kind {
        "liveeq" => {
            let f: Vec<&str> = line.split('|').collect();
            if f.len() != 6 {
                return Err(at(format!(
                    "{} fields in a liveeq record, expected 6",
                    f.len()
                )));
            }
            let id = f[1];
            if !crate::widgets::valid_id(id) {
                return Err(at(format!("invalid equation id `{id}`")));
            }
            if out.iter().any(|e| e.id == id) {
                return Err(at(format!("duplicate equation id `{id}`")));
            }
            let mode = match f[4] {
                "inline" => Mode::Inline,
                "widget" => Mode::Widget,
                m => {
                    return Err(at(format!(
                        "equation {id}: mode `{m}` is not inline or widget"
                    )))
                }
            };
            out.push(Equation {
                id: id.to_string(),
                label: opt(f[2]),
                number: opt(f[3]),
                mode,
                alt: opt(f[5]),
                params: Vec::new(),
                results: Vec::new(),
            });
            Ok(())
        }
        "param" => {
            let f: Vec<&str> = line.split('|').collect();
            if f.len() != 9 {
                return Err(at(format!(
                    "{} fields in a param record, expected 9",
                    f.len()
                )));
            }
            let eq = last(out, f[1]).map_err(at)?;
            let name = f[2];
            if !valid_param_name(name) {
                return Err(at(format!(
                    "equation {}: invalid param name `{name}`",
                    eq.id
                )));
            }
            if eq.params.iter().any(|p| p.name == name) {
                return Err(at(format!("equation {}: param `{name}` twice", eq.id)));
            }
            let num = |s: &str, what: &str| {
                plain_decimal(s)
                    .ok_or_else(|| at(format!("param {name}: {what} `{s}` is not a plain decimal")))
            };
            let p = Param {
                name: name.to_string(),
                default: num(f[3], "default")?,
                min: num(f[4], "min")?,
                max: num(f[5], "max")?,
                step: num(f[6], "step")?,
                places: places(f[7]).map_err(at)?,
                shown: shown(f[8]).map_err(at)?,
            };
            if !(p.min < p.max && p.step > 0.0 && p.min <= p.default && p.default <= p.max) {
                return Err(at(format!(
                    "param {name}: needs min < max, step > 0 and the default in [min, max]"
                )));
            }
            eq.params.push(p);
            Ok(())
        }
        "result" => {
            let f: Vec<&str> = line.splitn(6, '|').collect();
            if f.len() != 6 {
                return Err(at(format!(
                    "{} fields in a result record, expected 6",
                    f.len()
                )));
            }
            let eq = last(out, f[1]).map_err(at)?;
            let want = eq.results.len() + 1;
            if f[2] != want.to_string() {
                return Err(at(format!(
                    "equation {}: result `{}`, expected {want}",
                    eq.id, f[2]
                )));
            }
            let names: Vec<&str> = eq.params.iter().map(|p| p.name.as_str()).collect();
            check_formula(f[5], &names).map_err(|e| {
                at(format!(
                    "equation {} result {want}: {e} in `{}`",
                    eq.id, f[5]
                ))
            })?;
            eq.results.push(LiveResult {
                n: want as u32,
                places: places(f[3]).map_err(at)?,
                shown: shown(f[4]).map_err(at)?,
                formula: f[5].to_string(),
            });
            Ok(())
        }
        _ => Err(at(format!("unknown record `{kind}`"))),
    }
}

/// Checks the finished list: every equation has a param (the article finds
/// live equations by their param tokens), and no id is also a widget's.
pub(crate) fn check_all(eqs: &[Equation], widget_ids: &[&str]) -> Result<(), String> {
    for e in eqs {
        if e.params.is_empty() {
            return Err(format!("equation {}: the sidecar records no param", e.id));
        }
        if widget_ids.contains(&e.id.as_str()) {
            return Err(format!("equation {}: the id is also a widget's", e.id));
        }
    }
    Ok(())
}

/// The equation a param or result line names: the last one, which it must
/// follow.
fn last<'a>(out: &'a mut [Equation], id: &str) -> Result<&'a mut Equation, String> {
    match out.last_mut() {
        Some(e) if e.id == id => Ok(e),
        _ => Err(format!("record for `{id}` does not follow its liveeq line")),
    }
}

fn opt(s: &str) -> Option<String> {
    (!s.trim().is_empty()).then(|| s.to_string())
}

fn places(s: &str) -> Result<u32, String> {
    s.parse::<u32>()
        .ok()
        .filter(|p| *p <= MAX_PLACES && s.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| format!("places `{s}` is not a whole number up to {MAX_PLACES}"))
}

/// A shown value is the article's text: digits, one point, a leading minus.
fn shown(s: &str) -> Result<String, String> {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = body.split_once('.').unwrap_or((body, "0"));
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if digits(int) && digits(frac) {
        Ok(s.to_string())
    } else {
        Err(format!("shown value `{s}` is not a decimal"))
    }
}

/// `12`, `-1.5`, `.5`, `+2.`: the package's plain decimals, no exponent.
fn plain_decimal(s: &str) -> Option<f64> {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let ok = !(int.is_empty() && frac.is_empty())
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| s.parse::<f64>().ok()).flatten()
}

// -- the formula language ----------------------------------------------------

/// Longest formula, in bytes, and deepest nesting: as the reader's.
pub const MAX_LENGTH: usize = 1000;
pub const MAX_DEPTH: usize = 64;

/// Each function's arity: (name, min, max) arguments.
const FUNCTIONS: &[(&str, usize, usize)] = &[
    ("abs", 1, 1),
    ("exp", 1, 1),
    ("ln", 1, 1),
    ("sqrt", 1, 1),
    ("sin", 1, 1),
    ("cos", 1, 1),
    ("tan", 1, 1),
    ("asin", 1, 1),
    ("acos", 1, 1),
    ("atan", 1, 2),
    ("floor", 1, 1),
    ("ceil", 1, 1),
    ("trunc", 1, 1),
    ("sign", 1, 1),
    ("round", 1, 2),
    ("min", 1, usize::MAX),
    ("max", 1, usize::MAX),
];

fn function(name: &str) -> Option<(usize, usize)> {
    FUNCTIONS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, lo, hi)| (*lo, *hi))
}

/// Letters, then letters or digits; not a function or `pi`.
pub fn valid_param_name(name: &str) -> bool {
    let mut b = name.bytes();
    b.next().is_some_and(|c| c.is_ascii_alphabetic())
        && b.all(|c| c.is_ascii_alphanumeric())
        && name != "pi"
        && function(name).is_none()
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num,
    Name(String),
    Op(&'static str),
}

const OPS: &[&str] = &[
    "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "^", "<", ">", "!", "?", ":", "(", ")",
    ",",
];

/// Byte length of the number at the start of `s`: `12`, `1.5`, `.5`,
/// `1e-3`. Zero when none starts there.
fn number_len(s: &[u8]) -> usize {
    let digits = |from: usize| s[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    let mut i = digits(0);
    if s.get(i) == Some(&b'.') {
        let frac = digits(i + 1);
        if i == 0 && frac == 0 {
            return 0;
        }
        i += 1 + frac;
    }
    if i == 0 {
        return 0;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(s.get(i + 1), Some(b'+' | b'-')));
        let exp = digits(i + 1 + sign);
        if exp > 0 {
            i += 1 + sign + exp;
        }
    }
    i
}

fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, String> {
    let s = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let n = number_len(&s[i..]);
        if n > 0 {
            out.push((Tok::Num, i));
            i += n;
            continue;
        }
        if c.is_ascii_alphabetic() {
            let n = s[i..]
                .iter()
                .take_while(|b| b.is_ascii_alphanumeric())
                .count();
            out.push((Tok::Name(src[i..i + n].to_string()), i));
            i += n;
            continue;
        }
        if s[i..].starts_with(b"**") {
            return Err("use ^ for powers, not **".to_string());
        }
        let Some(op) = OPS.iter().find(|o| s[i..].starts_with(o.as_bytes())) else {
            return Err(if c == b'=' {
                "use == to compare, not =".to_string()
            } else {
                format!(
                    "unexpected character `{}`",
                    src[i..].chars().next().unwrap_or('?')
                )
            });
        };
        out.push((Tok::Op(op), i));
        i += op.len();
    }
    // Two operands side by side: an implicit product, or spaces inside a number.
    for w in out.windows(2) {
        let ends = !matches!(w[0].0, Tok::Op(o) if o != ")");
        let starts = !matches!(w[1].0, Tok::Op(o) if o != "(");
        let call =
            matches!(&w[0].0, Tok::Name(n) if function(n).is_some()) && w[1].0 == Tok::Op("(");
        if ends && starts && !call {
            return Err("write * between factors; implicit multiplication is not allowed".into());
        }
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<(Tok, usize)>,
    i: usize,
    depth: usize,
    names: &'a [&'a str],
}

impl Parser<'_> {
    fn peek(&self, op: &str) -> bool {
        matches!(self.toks.get(self.i), Some((Tok::Op(o), _)) if *o == op)
    }

    fn take(&mut self, op: &str) -> bool {
        let hit = self.peek(op);
        if hit {
            self.i += 1;
        }
        hit
    }

    fn nest<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("formula is nested too deeply".to_string());
        }
        let r = f(self);
        self.depth -= 1;
        r
    }

    fn expr(&mut self) -> Result<(), String> {
        self.nest(|p| {
            p.or()?;
            if p.take("?") {
                p.expr()?;
                if !p.take(":") {
                    return Err("expected `:`".to_string());
                }
                p.expr()?;
            }
            Ok(())
        })
    }

    fn left(
        &mut self,
        ops: &[&str],
        next: fn(&mut Self) -> Result<(), String>,
    ) -> Result<(), String> {
        next(self)?;
        while ops.iter().any(|o| self.take(o)) {
            next(self)?;
        }
        Ok(())
    }

    fn or(&mut self) -> Result<(), String> {
        self.left(&["||"], Self::and)
    }

    fn and(&mut self) -> Result<(), String> {
        self.left(&["&&"], Self::cmp)
    }

    fn cmp(&mut self) -> Result<(), String> {
        const CMPS: [&str; 6] = ["<=", ">=", "==", "!=", "<", ">"];
        self.add()?;
        if CMPS.iter().any(|o| self.take(o)) {
            self.add()?;
            if CMPS.iter().any(|o| self.peek(o)) {
                return Err("chained comparisons are not allowed; join them with &&".to_string());
            }
        }
        Ok(())
    }

    fn add(&mut self) -> Result<(), String> {
        self.left(&["+", "-"], Self::mul)
    }

    fn mul(&mut self) -> Result<(), String> {
        self.left(&["*", "/"], Self::unary)
    }

    fn unary(&mut self) -> Result<(), String> {
        self.nest(|p| {
            if p.take("-") || p.take("+") || p.take("!") {
                return p.unary();
            }
            p.atom()?;
            if p.take("^") {
                p.unary()?;
            }
            Ok(())
        })
    }

    fn atom(&mut self) -> Result<(), String> {
        let Some((tok, _)) = self.toks.get(self.i).cloned() else {
            return Err("formula ends too soon".to_string());
        };
        self.i += 1;
        match tok {
            Tok::Num => Ok(()),
            Tok::Name(n) => {
                if let Some((lo, hi)) = function(&n) {
                    return self.call(&n, lo, hi);
                }
                if n == "pi" || self.names.contains(&n.as_str()) {
                    Ok(())
                } else {
                    Err(format!("unknown name `{n}`"))
                }
            }
            Tok::Op("(") => {
                self.expr()?;
                if self.take(")") {
                    Ok(())
                } else {
                    Err("expected `)`".to_string())
                }
            }
            Tok::Op(o) => Err(format!("unexpected `{o}`")),
        }
    }

    fn call(&mut self, name: &str, lo: usize, hi: usize) -> Result<(), String> {
        if !self.take("(") {
            return Err(format!("expected `(` after {name}"));
        }
        let mut args = 1;
        self.expr()?;
        while self.take(",") {
            args += 1;
            self.expr()?;
        }
        if !self.take(")") {
            return Err("expected `)`".to_string());
        }
        if args < lo || args > hi {
            let want = if lo == hi {
                lo.to_string()
            } else if hi == usize::MAX {
                format!("at least {lo}")
            } else {
                format!("{lo} or {hi}")
            };
            return Err(format!(
                "{name} takes {want} argument{}",
                if hi == 1 { "" } else { "s" }
            ));
        }
        Ok(())
    }
}

/// Checks `src` against the formula language, with `names` (its equation's
/// params) and `pi` as its only names.
pub fn check_formula(src: &str, names: &[&str]) -> Result<(), String> {
    if src.len() > MAX_LENGTH {
        return Err("formula is too long".to_string());
    }
    let mut p = Parser {
        toks: tokenize(src)?,
        i: 0,
        depth: 0,
        names,
    };
    p.expr()?;
    if p.i < p.toks.len() {
        return Err("unexpected input".to_string());
    }
    Ok(())
}
