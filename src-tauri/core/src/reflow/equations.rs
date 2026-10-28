//! Live equations in the article: the PDF's values into the converted math.
//!
//! The converter binding (`engine/embedded/maleficium-interactive.sty.rhai`)
//! turns each `liveequation` into an ordinary equation whose params and
//! results are number tokens with stand-in text:
//!
//! ```html
//! <math class="ltx_Math" alttext="...\mathrm{\mfwleqtok{k}{0.5}}...\mathrm{\mfwleqres}">
//!   ... <mn class="m-param m-param-k">0.5</mn> ... <mn class="m-result">0</mn> ...
//! ```
//!
//! The converter cannot evaluate a formula, so the values come from the
//! sidecar (`crate::liveeq`): the i-th `<math>` holding an `m-param` token is
//! the i-th live equation (each has at least one param). [`apply`] writes the
//! shown values into its tokens, numbers its results (`m-result-<n>`),
//! rewrites its `alttext` to the same values, and tags the `<math>` with
//! `m-liveeq m-liveeq-<id>` for the reader. When the article and the sidecar
//! disagree it changes nothing but the results' stand-ins, which become `?`,
//! and says why.
//!
//! Like [`super::join`], this reads latexml's own serialization, not
//! arbitrary HTML: a `<math>` never nests, and the binding's tokens are
//! written exactly as above.

use crate::liveeq::Equation;

const PARAM_OPEN: &str = "<mn class=\"m-param m-param-";
const RESULT_TOKEN: &str = "<mn class=\"m-result\">0</mn>";
const ALT_PARAM: &str = "\\mathrm{\\mfwleqtok{";
const ALT_RESULT: &str = "\\mathrm{\\mfwleqres}";

/// A `<math>` element's byte range in the html.
struct MathSpan {
    start: usize,
    end: usize,
}

/// Every `<math>...</math>` that holds a param token, in document order.
fn live_maths(html: &str) -> Vec<MathSpan> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = html[i..].find("<math") {
        let start = i + off;
        let after = &html[start + 5..];
        if !after.starts_with([' ', '>', '\t', '\n']) {
            i = start + 5;
            continue;
        }
        let Some(close) = html[start..].find("</math>") else {
            break;
        };
        let end = start + close + "</math>".len();
        if html[start..end].contains(PARAM_OPEN) {
            out.push(MathSpan { start, end });
        }
        i = end;
    }
    out
}

/// The article text of a shown value: a math minus for the sign.
fn text(shown: &str) -> String {
    match shown.strip_prefix('-') {
        Some(rest) => format!("\u{2212}{rest}"),
        None => shown.to_string(),
    }
}

/// `html` with the live equations' values in place, or the reason it could
/// not join them (with the results' stand-ins replaced by `?`).
pub fn apply(html: &str, eqs: &[Equation]) -> Result<String, (String, String)> {
    let maths = live_maths(html);
    let unknown = |why: String| {
        (
            html.replace(RESULT_TOKEN, "<mn class=\"m-result\">?</mn>"),
            why,
        )
    };
    if maths.len() != eqs.len() {
        return Err(unknown(format!(
            "the article has {} live equations but the sidecar records {}; recompile the document",
            maths.len(),
            eqs.len()
        )));
    }
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    for (m, eq) in maths.iter().zip(eqs) {
        out.push_str(&html[at..m.start]);
        let math = fill(&html[m.start..m.end], eq)
            .map_err(|e| unknown(format!("equation {}: {e}", eq.id)))?;
        out.push_str(&math);
        at = m.end;
    }
    out.push_str(&html[at..]);
    Ok(out)
}

/// One `<math>` element with `eq`'s values.
fn fill(math: &str, eq: &Equation) -> Result<String, String> {
    let tag_end = math.find('>').ok_or("unreadable math start tag")? + 1;
    let (tag, body) = math.split_at(tag_end);
    let shown_of = |name: &str| {
        eq.params
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.shown.as_str())
            .ok_or_else(|| {
                format!("the article names param `{name}`, which the sidecar does not record")
            })
    };

    // Tokens: each param's text, each result's number and text.
    let mut out = String::with_capacity(math.len());
    let mut rest = body;
    let mut results = 0;
    loop {
        let p = rest.find(PARAM_OPEN);
        let r = rest.find(RESULT_TOKEN);
        match (p, r) {
            (Some(p), r) if r.is_none_or(|r| p < r) => {
                out.push_str(&rest[..p]);
                let after = &rest[p + PARAM_OPEN.len()..];
                let q = after.find("\">").ok_or("unreadable param token")?;
                let name = &after[..q];
                let close = after[q..].find("</mn>").ok_or("unclosed param token")? + q;
                out.push_str(&format!(
                    "{PARAM_OPEN}{name}\">{}</mn>",
                    text(shown_of(name)?)
                ));
                rest = &after[close + "</mn>".len()..];
            }
            (_, Some(r)) => {
                out.push_str(&rest[..r]);
                let res = eq
                    .results
                    .get(results)
                    .ok_or("the article has more results than the sidecar")?;
                results += 1;
                out.push_str(&format!(
                    "<mn class=\"m-result m-result-{}\">{}</mn>",
                    res.n,
                    text(&res.shown)
                ));
                rest = &rest[r + RESULT_TOKEN.len()..];
            }
            _ => break,
        }
    }
    out.push_str(rest);
    if results != eq.results.len() {
        return Err(format!(
            "the article has {results} results but the sidecar records {}",
            eq.results.len()
        ));
    }

    Ok(format!("{}{out}", start_tag(tag, eq)?))
}

/// The `<math>` start tag with the reader's class tokens and the alttext's
/// macros replaced by the values.
fn start_tag(tag: &str, eq: &Equation) -> Result<String, String> {
    let mut tag = tag.to_string();
    let class = "class=\"";
    match tag.find(class) {
        Some(c) => tag.insert_str(c + class.len(), &format!("m-liveeq m-liveeq-{} ", eq.id)),
        None => tag.insert_str(5, &format!(" class=\"m-liveeq m-liveeq-{}\"", eq.id)),
    }
    let alt = "alttext=\"";
    let Some(a) = tag.find(alt) else {
        return Ok(tag);
    };
    let vs = a + alt.len();
    let ve = vs + tag[vs..].find('"').ok_or("unclosed alttext")?;
    let mut value = String::new();
    let mut rest = &tag[vs..ve];
    let mut results = eq.results.iter();
    loop {
        let p = rest.find(ALT_PARAM);
        let r = rest.find(ALT_RESULT);
        match (p, r) {
            (Some(p), r) if r.is_none_or(|r| p < r) => {
                value.push_str(&rest[..p]);
                let after = &rest[p + ALT_PARAM.len()..];
                let name_end = after.find('}').ok_or("unreadable alttext param")?;
                let name = &after[..name_end];
                let close = after.find("}}").ok_or("unreadable alttext param")?;
                value.push_str(
                    eq.params
                        .iter()
                        .find(|q| q.name == name)
                        .map_or("?", |q| q.shown.as_str()),
                );
                rest = &after[close + 2..];
            }
            (_, Some(r)) => {
                value.push_str(&rest[..r]);
                value.push_str(results.next().map_or("?", |x| x.shown.as_str()));
                rest = &rest[r + ALT_RESULT.len()..];
            }
            _ => break,
        }
    }
    value.push_str(rest);
    tag.replace_range(vs..ve, &value);
    Ok(tag)
}

#[cfg(test)]
mod tests;
