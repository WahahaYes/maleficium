// maleficium-interactive.sty.rhai -- latexml-oxide runtime binding for the
// Maleficium interactive-paper package (embed-runtime/tex/
// maleficium-interactive.sty). It replaces raw loading of the .sty, which
// needs expl3 and writes PDF specials and a sidecar that mean nothing in HTML.
//
// Placeholder contract (the reader joins on it; see core/src/reflow/join.rs):
// every widget macro emits exactly one empty inline element
//
//     <span class="ltx_text m-widget m-widget-KIND"></span>
//
// with KIND html (\interactive) or custom (\interactiveruntime, built-in
// runtimes included). `ltx:text` takes no
// arbitrary attributes, so the kind rides in a class token and the element
// itself is the mount point. The element carries no id, options or path: the
// i-th placeholder in document order joins the i-th `widget|` record of the
// <jobname>.mfw sidecar the PDF compile writes, and ids, labels, posters and
// figure numbers come from there, never from this binding.
//
// Macros of the .sty and what they become here:
//   \interactive, \interactiveruntime   placeholder, arguments read
//                       verbatim and dropped (custom: any runtime's widget)
//   \maleficiumcontents{on|off}             one empty flag marker per call:
//                       <ltx:text class="m-flag m-flag-contents-on|off"/>
//   \maleficiummeasure{narrow|default|wide|full}  one empty flag marker per call:
//                       <ltx:text class="m-flag m-flag-measure-*"/>
//   \maleficiumtheme, \maleficiumsetup,
//   \maleficiumreaderstyle   no output (their keys are
//                       consumed, never surfaced in the article)
//   \mfwSlash           no output (the PDF's brand mark)
//   liveequation(*)     equation (displaymath), options dropped
//   \maleficiumparam    one number token <mn class="m-param m-param-NAME">,
//                       the default (or 0 when repeated); the range dropped
//   \liveresult         one number token <mn class="m-result">0</mn>
// Live equations: the converter cannot evaluate a formula, so every value
// token's text is a stand-in. The exporter joins each <math> holding
// m-param tokens to the i-th liveeq record of the sidecar, writes the values
// the PDF typeset into the tokens and numbers the results (see
// core/src/reflow/equations.rs). It also rewrites the alttext, which holds
// these macros' expansions: \mathrm{\mfwleqtok{NAME}{X}} and
// \mathrm{\mfwleqres}. \mathrm keeps the numbers upright.
// A flag marker is the reader-flag analog of a widget placeholder: one empty
// element, the value riding in a closed class token (`ltx:text` takes no
// value attributes). The PDF compile already rejected anything outside the
// closed sets, so anything else here is a conversion error, never an open
// class token. The exporter joins the markers in document order.
// The package's colors are defined so \textcolor{mfwaccent} still works.
//
// Packages: the .sty loads graphicx, xcolor and keyval, and hyperref when the
// document has not; this binding does the same, so `\includegraphics` and
// `\textcolor` keep working in documents that rely on the package to load
// them. tikz, xkeyval and pgfplotstable (PDF poster drawing and error
// wording only) are not loaded: a document that draws with tikz must load
// it itself.

RequirePackage("keyval");
RequirePackage("graphicx");
RequirePackage("xcolor");
RequirePackage("hyperref", ["hidelinks"]);

RawTeX("\\definecolor{mfwaccent}{HTML}{5B2A86}\\definecolor{mfwink}{HTML}{1E1B24}\\definecolor{mfwmuted}{HTML}{6B6676}");

DefMacro("\\maleficiumtheme Semiverbatim", |x| "");
DefMacro("\\maleficiumsetup Semiverbatim", |x| "");
// The reader stylesheet rides the sidecar (a style line), never the article.
DefMacro("\\maleficiumreaderstyle Semiverbatim", |x| "");
DefMacro("\\mfwSlash", || "");

DefConstructor("\\interactive [Semiverbatim] Semiverbatim", "<ltx:text class=\"m-widget m-widget-html\"/>");
DefConstructor("\\interactiveruntime [Semiverbatim] Semiverbatim", "<ltx:text class=\"m-widget m-widget-custom\"/>");

DefConstructor("\\maleficiumcontents Semiverbatim", |document, v| {
  let s = ToString(v);
  if s == "on" || s == "off" {
    document.openElement("ltx:text");
    document.setAttribute("class", "m-flag m-flag-contents-" + s);
    document.closeElement("ltx:text");
  } else {
    Error("maleficium", "maleficiumcontents", "contents flag `" + s + "' must be on or off");
  }
});

DefConstructor("\\maleficiummeasure Semiverbatim", |document, v| {
  let s = ToString(v);
  if s == "narrow" || s == "default" || s == "wide" || s == "full" {
    document.openElement("ltx:text");
    document.setAttribute("class", "m-flag m-flag-measure-" + s);
    document.closeElement("ltx:text");
  } else {
    Error("maleficium", "maleficiummeasure", "measure flag `" + s + "' must be narrow, default, wide or full");
  }
});

DefConstructor("\\mfwleqtok Semiverbatim Semiverbatim", "<ltx:XMTok role=\"NUMBER\" meaning=\"#2\" class=\"m-param m-param-#1\">#2</ltx:XMTok>");
DefConstructor("\\mfwleqres", "<ltx:XMTok role=\"NUMBER\" meaning=\"0\" class=\"m-result\">0</ltx:XMTok>");
RawTeX("\\def\\maleficiumparam#1{\\@ifnextchar\\bgroup{\\mfwleqpd{#1}}{\\mathrm{\\mfwleqtok{#1}{0}}}}\\def\\mfwleqpd#1#2{\\@ifnextchar[{\\mfwleqpr{#1}{#2}}{\\mathrm{\\mfwleqtok{#1}{#2}}}}\\def\\mfwleqpr#1#2[#3]{\\mathrm{\\mfwleqtok{#1}{#2}}}");
DefMacro("\\liveresult[]{}", "\\mathrm{\\mfwleqres}");
DefMacro("\\liveequation[]", "\\begin{equation}");
DefMacro("\\endliveequation", "\\end{equation}");
DefMacro("\\csname liveequation*\\endcsname[]", "\\begin{displaymath}");
DefMacro("\\csname endliveequation*\\endcsname", "\\end{displaymath}");
