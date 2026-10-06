// maleficium-interactive.sty.rhai -- latexml-oxide runtime binding for the
// Maleficium interactive-paper package (src-tauri/interactive/
// maleficium-interactive.sty). It replaces raw loading of the .sty, which
// needs expl3 and writes PDF specials and a sidecar that mean nothing in HTML.
//
// Placeholder contract (the reader joins on it; see core/src/reflow/join.rs):
// every widget macro emits exactly one empty inline element
//
//     <span class="ltx_text m-widget m-widget-KIND"></span>
//
// with KIND one of model, video, table, chart, html, custom. `ltx:text` takes no
// arbitrary attributes, so the kind rides in a class token and the element
// itself is the mount point. The element carries no id, options or path: the
// i-th placeholder in document order joins the i-th `widget|` record of the
// <jobname>.mfw sidecar the PDF compile writes, and ids, labels, posters and
// figure numbers come from there, never from this binding.
//
// Macros of the .sty and what they become here:
//   \interactive, \interactivemodel, \interactivevideo, \interactivetable,
//   \interactivechart, \interactiveruntime   placeholder, arguments read
//                       verbatim and dropped (custom: the runtime's own widget)
//   \maleficiumcontents{on|off}             one empty flag marker per call:
//                       <ltx:text class="m-flag m-flag-contents-on|off"/>
//   \maleficiummeasure{narrow|default|wide}  one empty flag marker per call:
//                       <ltx:text class="m-flag m-flag-measure-*"/>
//   \maleficiumtheme, \maleficiumsetup   no output (their keys are
//                       consumed, never surfaced in the article)
//   \mfwSlash           no output (the PDF's brand mark)
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
DefMacro("\\mfwSlash", || "");

DefConstructor("\\interactive [Semiverbatim] Semiverbatim", "<ltx:text class=\"m-widget m-widget-html\"/>");
DefConstructor("\\interactivemodel [Semiverbatim] Semiverbatim", "<ltx:text class=\"m-widget m-widget-model\"/>");
DefConstructor("\\interactivevideo [Semiverbatim] Semiverbatim", "<ltx:text class=\"m-widget m-widget-video\"/>");
DefConstructor("\\interactivetable [Semiverbatim] Semiverbatim", "<ltx:text class=\"m-widget m-widget-table\"/>");
DefConstructor("\\interactivechart [Semiverbatim] Semiverbatim", "<ltx:text class=\"m-widget m-widget-chart\"/>");
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
  if s == "narrow" || s == "default" || s == "wide" {
    document.openElement("ltx:text");
    document.setAttribute("class", "m-flag m-flag-measure-" + s);
    document.closeElement("ltx:text");
  } else {
    Error("maleficium", "maleficiummeasure", "measure flag `" + s + "' must be narrow, default or wide");
  }
});
