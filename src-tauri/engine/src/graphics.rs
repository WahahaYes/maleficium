//! Figure paths for the reader. With graphics conversion off, latexml's HTML
//! drops every `\includegraphics` path (`<img src="" class="ltx_graphics ...">`).
//! The path as written survives in the LaTeXML XML as the `graphic` attribute
//! of each `<graphics>` element, so `inject` copies the i-th one onto the i-th
//! `ltx_graphics` img of the HTML as `data-graphic`. When the two counts
//! disagree nothing is injected rather than misaligned.

/// The `graphic` attribute of every `<graphics>` element, in document order,
/// with XML entities decoded.
fn xml_graphics(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = xml[at..].find('<') {
        let start = at + i;
        let Some(end) = tag_end(xml, start) else {
            break;
        };
        let tag = &xml[start..end];
        let name = tag[1..]
            .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .next()
            .unwrap_or("");
        if name == "graphics" || name == "ltx:graphics" {
            if let Some(v) = attr(tag, "graphic") {
                out.push(unescape(v));
            }
        }
        at = end;
    }
    out
}

/// The index just past the `>` closing the tag that opens at `start`,
/// skipping `>` inside quoted values.
fn tag_end(s: &str, start: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (i, b) in s.bytes().enumerate().skip(start) {
        match (quote, b) {
            (Some(q), b) if b == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(b),
            (None, b'>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// The raw value of attribute `name` in a start tag.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for q in ['"', '\''] {
        let pat = format!("{name}={q}");
        let mut from = 0;
        while let Some(i) = tag[from..].find(&pat) {
            let at = from + i;
            let boundary = tag[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
            let v = at + pat.len();
            if boundary {
                return tag[v..].find(q).map(|e| &tag[v..v + e]);
            }
            from = v;
        }
    }
    None
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `html` with `data-graphic="<path>"` added to each `ltx_graphics` img, or
/// the reason nothing was added.
pub fn inject(xml: &str, html: &str) -> Result<String, String> {
    let paths = xml_graphics(xml);
    let mut out = String::with_capacity(html.len() + paths.len() * 48);
    let mut at = 0;
    let mut n = 0;
    let mut spots: Vec<usize> = Vec::new();
    while let Some(i) = html[at..].find("<img") {
        let start = at + i;
        let Some(end) = tag_end(html, start) else {
            break;
        };
        let is_img = html[start + 4..].starts_with(|c: char| c.is_whitespace());
        if is_img
            && attr(&html[start..end], "class")
                .is_some_and(|c| c.split_whitespace().any(|w| w == "ltx_graphics"))
        {
            spots.push(start + 4);
            n += 1;
        }
        at = end;
    }
    if n != paths.len() {
        return Err(format!(
            "skipped: the XML has {} graphics but the HTML has {n} ltx_graphics images",
            paths.len()
        ));
    }
    let mut last = 0;
    for (spot, path) in spots.into_iter().zip(&paths) {
        out.push_str(&html[last..spot]);
        out.push_str(&format!(" data-graphic=\"{}\"", escape(path)));
        last = spot;
    }
    out.push_str(&html[last..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<document><figure><graphics graphic="../figures/a &amp; b.png" candidates="x" xml:id="g1"/></figure><graphics options="w" graphic='c"d.jpg'/><p graphic="no"/></document>"#;
    const HTML: &str = r#"<p><img src="" id="g1" class="ltx_graphics ltx_centering ltx_missing" alt="x > y"><img src="l.png" class="logo"><img src="" class="ltx_graphics"></p>"#;

    #[test]
    fn paths_are_read_in_order_and_decoded() {
        assert_eq!(xml_graphics(XML), ["../figures/a & b.png", "c\"d.jpg"]);
    }

    #[test]
    fn the_ith_graphics_img_gets_the_ith_path() {
        let html = inject(XML, HTML).unwrap();
        assert_eq!(html.matches("data-graphic=").count(), 2);
        assert!(html.contains(r#"<img data-graphic="../figures/a &amp; b.png" src="" id="g1""#));
        assert!(html.contains(r#"<img data-graphic="c&quot;d.jpg" src="" class="ltx_graphics">"#));
        assert!(html.contains(r#"<img src="l.png" class="logo">"#));
    }

    #[test]
    fn a_document_without_figures_is_unchanged() {
        let html = "<p>hi</p>";
        assert_eq!(inject("<document/>", html).unwrap(), html);
    }

    #[test]
    fn mismatched_counts_inject_nothing() {
        assert!(inject(XML, r#"<img class="ltx_graphics">"#).is_err());
    }
}
