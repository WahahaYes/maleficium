use super::*;

fn mounts(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

fn clean(html: &str) -> String {
    sanitize(html, &mounts(&["fig-a", "fig-b"]))
}

/// Nothing that runs, loads or navigates survives, and the output is a fixed
/// point: parsing and sanitizing it again changes nothing, so a browser's
/// re-parse cannot mutate it into something the sanitizer did not see.
fn assert_inert(input: &str) -> String {
    let out = clean(input);
    // What a browser builds from the output: every element and attribute.
    let doc = dom_query::Document::fragment(out.as_str());
    let all = doc.root().descendants();
    for n in all.iter().filter(|n| n.is_element()) {
        let q = n.qual_name_ref().unwrap();
        let (ns, name) = (q.ns.to_string(), q.local.to_string());
        drop(q);
        assert!(
            (ns == HTML_NS && (HTML_KEEP.contains(&name.as_str()) || name == "html"))
                || (ns == MATHML_NS && MATHML_KEEP.contains(&name.as_str())),
            "<{name}> in {ns} survived {input:?}: {out}"
        );
        for a in n.attrs() {
            let k = a.name.local.to_string();
            let v = a.value.to_string();
            assert!(
                a.name.ns.is_empty(),
                "namespaced {k} survived {input:?}: {out}"
            );
            assert!(!k.starts_with("on"), "{k} survived {input:?}: {out}");
            match k.as_str() {
                "href" => assert!(name == "a" && safe_href(&v), "href {v:?} in {out}"),
                "src" => assert!(name == "img" && safe_src(&v), "src {v:?} in {out}"),
                "style" => assert!(name == "figure" && aspect_style(&v), "style {v:?} in {out}"),
                _ => {}
            }
        }
    }
    assert!(
        !all.iter().any(|n| n.is_comment()),
        "a comment survived {input:?}: {out}"
    );
    assert_eq!(clean(&out), out, "not a fixed point: {input:?}");
    out
}

#[test]
fn scripts_and_their_text_are_dropped_in_any_case_and_nesting() {
    for input in [
        "<script>alert(1)</script>",
        "<SCRIPT>alert(1)</SCRIPT>",
        "<ScRiPt src=//evil.example/x.js></ScRiPt>",
        "<script><script>alert(1)</script></script>",
        "<div><script>alert(1)</script></div>",
        "<p><script>/*</p>*/alert(1)</script></p>",
        "<script>document.write('<img src=x onerror=alert(1)>')</script>",
    ] {
        let out = assert_inert(input);
        assert!(!out.contains("alert"), "script text survived: {out}");
    }
    // A broken tag name is an unknown element: unwrapped, its text escaped.
    assert_eq!(
        assert_inert("<scr<script>ipt>alert(1)</script>"),
        "ipt&gt;alert(1)"
    );
}

#[test]
fn event_handlers_and_inline_style_are_dropped() {
    let out = assert_inert(
        "<p onclick=\"alert(1)\" ONMOUSEOVER=\"alert(2)\" style=\"background:url(javascript:alert(3))\" class=\"ltx_p\">x</p>",
    );
    assert_eq!(out, "<p class=\"ltx_p\">x</p>");
    assert_eq!(
        assert_inert("<img src=x onerror=alert(1)>"),
        "<img>",
        "a bad src is dropped and the handler with it"
    );
    assert_inert("<a/href=\"javascript:alert(1)\"/onclick=alert(1)>x</a>");
    assert_inert("<p\nonclick=alert(1)>x</p>");
    assert_inert("<p o\u{0}nclick=alert(1)>x</p>");
    assert_inert("<details open ontoggle=alert(1)>x</details>");
}

#[test]
fn only_fragment_and_web_links_survive() {
    let keep = |h: &str| clean(&format!("<a href=\"{h}\">x</a>")).contains("href=");
    for ok in [
        "#S1",
        "#bib.bib1",
        "http://example.org/",
        "HTTPS://example.org/a?b=c#d",
    ] {
        assert!(keep(ok), "{ok} was dropped");
    }
    for bad in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        " javascript:alert(1)",
        "\u{1}javascript:alert(1)",
        "java\tscript:alert(1)",
        "java&#x09;script:alert(1)",
        "&#106;avascript:alert(1)",
        "&#0000106&#0000097vascript:alert(1)",
        "&#x6A;avascript&colon;alert(1)",
        "javascript&colon;alert(1)",
        "\u{ff4a}avascript:alert(1)",
        "vbscript:msgbox(1)",
        "data:text/html,<script>alert(1)</script>",
        "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
        "file:///etc/passwd",
        "//evil.example/",
        "main.html#S1",
        "",
    ] {
        assert!(!keep(bad), "{bad:?} was kept");
        assert_inert(&format!("<a href=\"{bad}\">x</a>"));
    }
    // The link text stays.
    assert_eq!(clean("<a href=\"javascript:x\">see</a>"), "<a>see</a>");
}

#[test]
fn images_load_only_inline_data_or_bundle_files() {
    let keep = |s: &str| clean(&format!("<img src=\"{s}\" alt=\"a\">")).contains("src=");
    for ok in [
        "data:image/png;base64,iVBORw0KGgo=",
        "DATA:IMAGE/JPEG;BASE64,/9j/4A==",
        "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=",
        "figures/0a1b2c.png",
        "assets/abc-poster.webp",
    ] {
        assert!(keep(ok), "{ok} was dropped");
    }
    for bad in [
        "https://evil.example/pixel.png",
        "//evil.example/pixel.png",
        "data:text/html;base64,PHNjcmlwdD4=",
        "data:image/svg+xml,<svg onload=alert(1)>",
        "data:image/png;base64,AAAA BBBB",
        "data:image/png;charset=utf-8;base64,AAAA",
        "figures/../secret.png",
        "figures/.hidden",
        "figures/",
        "figures/a/b.png",
        "/figures/a.png",
        "javascript:alert(1)",
        "x",
    ] {
        assert!(!keep(bad), "{bad:?} was kept");
    }
    // A quote ends the value: the parser sees a valid src and a handler,
    // and only the src is written.
    assert_eq!(
        assert_inert("<img src=\"data:image/png;base64,AAAA\"onerror=\"alert(1)\">"),
        "<img src=\"data:image/png;base64,AAAA\">"
    );
}

#[test]
fn svg_and_foreign_content_are_dropped_whole() {
    for input in [
        "<svg onload=alert(1)><circle r=1></circle></svg>",
        "<svg><script>alert(1)</script></svg>",
        "<svg><a xlink:href=\"javascript:alert(1)\"><text>x</text></a></svg>",
        "<svg><foreignObject><img src=x onerror=alert(1)></foreignObject></svg>",
        "<math><annotation-xml encoding=\"text/html\"><img src=x onerror=alert(1)></annotation-xml></math>",
        "<math><mglyph src=\"javascript:alert(1)\"></mglyph></math>",
        "<math><mi xlink:href=\"javascript:alert(1)\" href=\"javascript:alert(2)\">x</mi></math>",
    ] {
        let out = assert_inert(input);
        assert!(!out.contains("alert") && !out.contains("img"), "{out}");
    }
}

#[test]
fn mutation_xss_shapes_are_fixed_points() {
    for input in [
        // The DOMPurify 2.0.0 bypass and its relatives.
        "<math><mtext><table><mglyph><style><img src=x onerror=alert(1)>",
        "<math><mtext><table><mglyph><style><!--</style><img title=\"--&gt;&lt;/mglyph&gt;&lt;img&Tab;src=1&Tab;onerror=alert(1)&gt;\">",
        "<form><math><mtext></form><form><mglyph><style></math><img src onerror=alert(1)>",
        "<svg></p><style><a id=\"</style><img src=1 onerror=alert(1)>\">",
        "<noscript><p title=\"</noscript><img src=x onerror=alert(1)>\">",
        "<xmp><p title=\"</xmp><img src=x onerror=alert(1)>\">",
        "<textarea><p title=\"</textarea><img src=x onerror=alert(1)>\">",
        "<template><img src=x onerror=alert(1)></template>",
        "<table><tr><td><math><mi><table></table></mi></math></td></tr></table>",
        "<p><figure><p>a</figure></p>",
        "<math><mi><b>bold <img src=x onerror=alert(1)></b></mi></math>",
        "<select><template><style><!--</style><a rel=\"--></style></template></select><img src=x onerror=alert(1)>\">",
    ] {
        assert_inert(input);
    }
}

#[test]
fn comment_hidden_payloads_do_not_survive() {
    for input in [
        "<!--<script>alert(1)</script>-->",
        "<!--><img src=x onerror=alert(1)>-->",
        "<!-- --!><script>alert(1)</script> -->",
        "<!--mount:fig-a--><p>x</p>",
        "<![CDATA[<script>alert(1)</script>]]>",
        "<?php echo '<script>alert(1)</script>'; ?>",
        "<!doctype html><p>x</p>",
    ] {
        let out = assert_inert(input);
        assert!(!out.contains("alert(1)</"), "{out}");
    }
}

#[test]
fn page_owned_ids_cannot_be_shadowed() {
    assert_eq!(
        clean("<span id=\"mfw-manifest\">{}</span><span id=\"MFW-assets\"></span><a id=\"pdf-link\">x</a><p id=\"S1.p1\">y</p>"),
        "<span>{}</span><span></span><a>x</a><p id=\"S1.p1\">y</p>"
    );
}

#[test]
fn a_mount_unit_survives_once_per_known_widget() {
    let unit = |id: &str, ar: &str| {
        format!("<figure id=\"{id}\" data-widget=\"{id}\" data-type=\"model\" style=\"--ar:{ar}\"><div class=\"frame\"><img class=\"poster\" src=\"assets/aa.png\" alt=\"A\"></div><figcaption>c</figcaption></figure>")
    };
    let good = unit("fig-a", "200.00 / 100.00");
    assert_eq!(clean(&good), good, "the mount unit passes unchanged");
    let boxed = unit("fig-a", "4.00 / 3.00; --aw:200.00pt; --ah:150.00pt");
    assert_eq!(clean(&boxed), boxed, "the author's box passes unchanged");
    // A second copy of the same widget, an unknown id, a widened style.
    let out = clean(&[good.clone(), unit("fig-z", "4 / 3")].concat());
    assert_eq!(out.matches("data-widget=").count(), 1, "{out}");
    let out = clean(&unit(
        "fig-b",
        "1 / 1; background:url(https://evil.example/)",
    ));
    assert!(
        !out.contains("style=") && out.contains("data-widget=\"fig-b\""),
        "{out}"
    );
    let out = clean(&good.replace("data-type=\"model\"", "data-type=\"script\""));
    assert!(
        !out.contains("data-widget") && !out.contains("style="),
        "{out}"
    );
    // Mount attributes on anything but a figure are dropped.
    assert_eq!(
        clean("<div data-widget=\"fig-a\" data-type=\"model\" style=\"--ar:1 / 1\">x</div>"),
        "<div>x</div>"
    );
    assert!(aspect_style("--ar:4 / 3") && aspect_style("--ar:595.28 / 841.89"));
    // The author's box follows the aspect, --aw then --ah, numbers only.
    assert!(aspect_style(
        "--ar:4.00 / 3.00; --aw:200.00pt; --ah:150.00pt"
    ));
    assert!(aspect_style("--ar:4 / 3; --aw:200pt"));
    for bad in [
        "--ar:4/3",
        "--ar:4 / 3; --ah:150.00pt; --aw:200.00pt",
        "--ar:4 / 3; --aw:200.00pt; --aw:200.00pt",
        "--ar:4 / 3; --aw:200.00pt; --ah:150.00pt; --aw:1pt",
        "--ar:4 / 3; color:red",
        "--ar:4 / 3; --aw:200.00",
        "--ar:4 / 3; --aw:200.00px",
        "--ar:4 / 3; --aw:expression(alert(1))",
        "--ar:4 / 3;",
        "--ar: 4 / 3",
        "--ar:-4 / 3",
        "--ar:1e9 / 1",
        "--ar:4 / 3 / 2",
    ] {
        assert!(!aspect_style(bad), "{bad}");
    }
}

#[test]
fn latexml_structure_math_and_tables_pass_with_classes() {
    let input = concat!(
        "<section id=\"S1\" class=\"ltx_section\"><h2 class=\"ltx_title ltx_title_section\"><span class=\"ltx_tag ltx_tag_section\">1 </span>Intro</h2>",
        "<div class=\"ltx_para\"><p class=\"ltx_p\">Text <cite class=\"ltx_cite\">[<a href=\"#bib.bib1\" class=\"ltx_ref\">1</a>]</cite> ",
        "<math id=\"m1\" class=\"ltx_Math\" alttext=\"x^2\" display=\"inline\"><msup><mi>x</mi><mn>2</mn></msup></math>",
        " <span class=\"ltx_ERROR undefined\">\\foo</span> a &lt;b&gt; &amp; c</p></div>",
        "<table class=\"ltx_tabular\"><tbody><tr><td class=\"ltx_td\" colspan=\"2\" rowspan=\"1\">a</td></tr></tbody></table>",
        "<ol start=\"3\"><li>i</li></ol><pre class=\"ltx_verbatim\">\n\nfirst line</pre></section>"
    );
    let out = assert_inert(input);
    for want in [
        "<section id=\"S1\" class=\"ltx_section\">",
        "<a href=\"#bib.bib1\" class=\"ltx_ref\">1</a>",
        "<math id=\"m1\" class=\"ltx_Math\" alttext=\"x^2\" display=\"inline\"><msup><mi>x</mi><mn>2</mn></msup></math>",
        "<span class=\"ltx_ERROR undefined\">\\foo</span>",
        "a &lt;b&gt; &amp; c",
        "<td class=\"ltx_td\" colspan=\"2\" rowspan=\"1\">a</td>",
        "<ol start=\"3\">",
        "<pre class=\"ltx_verbatim\">\n\nfirst line</pre>",
    ] {
        assert!(out.contains(want), "{want} missing from {out}");
    }
}

#[test]
fn unknown_elements_are_unwrapped_and_dangerous_ones_removed_whole() {
    assert_eq!(clean("<font color=red><blink>kept</blink></font>"), "kept");
    assert_eq!(clean("<main><center>x</center></main>"), "x");
    assert_eq!(
        clean("<form action=\"https://evil.example/\"><input name=a value=b><button formaction=x>go</button></form>after"),
        "after"
    );
    assert_eq!(
        clean("<iframe srcdoc=\"<script>alert(1)</script>\">fallback</iframe>"),
        ""
    );
    assert_eq!(clean("<object data=x><embed src=y>fallback</object>"), "");
    assert_eq!(clean("<base href=\"https://evil.example/\"><link rel=stylesheet href=x><meta http-equiv=refresh content=0;url=x>z"), "z");
}

#[test]
fn attribute_values_are_escaped_and_never_break_out() {
    let out = clean("<p title='\"&gt;&lt;img src=x onerror=alert(1)&gt;'>x</p>");
    assert_eq!(
        out,
        "<p title=\"&quot;&gt;&lt;img src=x onerror=alert(1)&gt;\">x</p>"
    );
    assert_eq!(clean(&out), out);
}

#[test]
fn deep_nesting_does_not_overflow() {
    let depth = 20_000;
    let html = format!("{}x{}", "<span>".repeat(depth), "</span>".repeat(depth));
    let out = clean(&html);
    assert!(out.contains('x'));
    assert_eq!(
        out.matches("<span>").count(),
        out.matches("</span>").count()
    );
}

#[test]
fn a_custom_mount_unit_survives_once_for_a_known_id_and_never_otherwise() {
    let unit = |id: &str, kind: &str| {
        format!("<figure id=\"{id}\" data-widget=\"{id}\" data-type=\"{kind}\" style=\"--ar:4.00 / 3.00\"><div class=\"frame\"><img class=\"poster\" src=\"assets/aa.png\" alt=\"A\"></div><figcaption>c</figcaption></figure>")
    };
    let good = unit("fig-a", "custom");
    assert_eq!(clean(&good), good, "a custom mount unit passes unchanged");
    // Twice: mounted once.
    let out = clean(&[good.clone(), good.clone()].concat());
    assert_eq!(out.matches("data-widget=").count(), 1, "{out}");
    assert_eq!(out.matches("data-type=\"custom\"").count(), 1, "{out}");
    // An unknown id, or a kind no reader knows: no mount attributes at all.
    for bad in [
        unit("fig-z", "custom"),
        unit("fig-b", "custom-runtime"),
        unit("fig-b", "stl-viewer@1"),
        unit("fig-b", "Custom"),
    ] {
        let out = clean(&bad);
        assert!(
            !out.contains("data-widget") && !out.contains("data-type") && !out.contains("style="),
            "{out}"
        );
    }
}

#[test]
fn a_fallback_note_survives_as_text_only() {
    let note = |text: &str| {
        format!("<figure id=\"fig-a\" data-widget=\"fig-a\" data-type=\"custom\" style=\"--ar:4.00 / 3.00\"><div class=\"frame\"><img class=\"poster\" src=\"assets/aa.png\" alt=\"A\"></div><figcaption>c</figcaption><p class=\"m-widget-note\">{text}</p></figure>")
    };
    let plain =
        note("Interactive version not included in this copy: runtime heat@1 is not installed.");
    assert_eq!(clean(&plain), plain, "the note passes unchanged");
    // Markup in a note is escaped by the exporter (fold::text); were it not,
    // nothing that runs or loads would survive the sanitizer either.
    let escaped = note(&crate::bundle::fold::text(
        "<img src=x onerror=alert(1)><script>alert(2)</script>",
    ));
    let out = clean(&escaped);
    assert_eq!(out, escaped);
    assert!(!out.contains("<img src=x") && !out.contains("<script"));
    let raw = assert_inert(&note(
        "<img src=x onerror=alert(1)><script>alert(2)</script><a href=\"javascript:x\">y</a>",
    ));
    assert!(raw.contains("<p class=\"m-widget-note\">"), "{raw}");
    assert!(
        !raw.contains("onerror") && !raw.contains("<script") && !raw.contains("javascript:"),
        "{raw}"
    );
}

#[test]
fn data_approval_passes_only_as_required_on_a_mount_figure() {
    let unit = |approval: &str| {
        format!("<figure id=\"fig-a\" data-widget=\"fig-a\" data-type=\"html\" data-approval=\"{approval}\" style=\"--ar:4.00 / 3.00\"><div class=\"frame\"><img class=\"poster\" src=\"assets/aa.png\" alt=\"A\"></div><figcaption>c</figcaption></figure>")
    };
    let good = unit("required");
    assert_eq!(clean(&good), good, "the held-back marker passes unchanged");
    for bad in [
        "yes",
        "Required",
        "REQUIRED",
        "",
        " required",
        "required ",
        "true",
    ] {
        let out = clean(&unit(bad));
        assert!(!out.contains("data-approval"), "{bad:?} was kept: {out}");
        assert!(
            out.contains("data-widget=\"fig-a\""),
            "the mount itself stays: {out}"
        );
    }
    // Off a mount figure the marker never passes.
    assert!(
        !clean("<figure data-approval=\"required\"><p>x</p></figure>").contains("data-approval")
    );
    assert_eq!(
        clean("<div data-widget=\"fig-a\" data-type=\"html\" data-approval=\"required\">x</div>"),
        "<div>x</div>"
    );
    // An unknown widget or kind drops the mount and the marker with it.
    let out = clean(&good.replace("fig-a", "fig-z"));
    assert!(
        !out.contains("data-approval") && !out.contains("data-widget"),
        "{out}"
    );
    let out = clean(&good.replace("data-type=\"html\"", "data-type=\"script\""));
    assert!(
        !out.contains("data-approval") && !out.contains("data-widget"),
        "{out}"
    );
}
