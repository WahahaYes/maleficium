# Templates

Each folder here is a built-in template: a `template.json` manifest plus the project files it creates. They are embedded in the app at build time and appear under **Built-in** in the New Project from Template gallery.

Templates users save or import are kept in the app's data folder, never here, and appear under **Your templates**.

## License

Every built-in template is dedicated to the public domain under [CC0 1.0](LICENSE). A project made from one belongs to its author, with no credit or notice required. Each template's main file carries a one-line CC0 header so this travels with the copy.

This covers the template files only. The rest of Maleficium is Apache 2.0.

## Built-in templates

| Template | Folder | Origin | Class and packages it loads |
| --- | --- | --- | --- |
| Article | `article/` | Written for Maleficium | article; `maleficium-doc.sty` (amsmath, biblatex, booktabs, tikz, hyperref); Libertinus fonts |
| Assignment | `assignment/` | Written for Maleficium | article; `maleficium-doc.sty` (amsmath, amsthm, enumitem); Libertinus fonts |
| Book | `book/` | Written for Maleficium | book; `maleficium-doc.sty` (hyperref); Libertinus fonts |
| Curriculum Vitae | `cv/` | Written for Maleficium | article; shared `maleficium.sty` and `maleficium-mark.pdf` (fontspec, titlesec, enumitem, fancyhdr, tikz, hyperref, xcolor); Libertinus font |
| Journal Paper (IEEEtran) | `journal/` | Written for Maleficium | IEEEtran; amsmath, cite, graphicx |
| Letter | `letter/` | Written for Maleficium | letter; `maleficium-doc.sty`; Libertinus fonts |
| Presentation | `beamer/` | Written for Maleficium | beamer with the bundled `beamerthememaleficium.sty` (amsmath); Libertinus fonts |
| Report | `report/` | Written for Maleficium | report; `maleficium-doc.sty` (amsmath, graphicx, hyperref); Libertinus fonts |
| Resume | `resume/` | Written for Maleficium | article; shared `maleficium.sty` and `maleficium-mark.pdf` (same as the CV); Libertinus font |
| Welcome tour | `welcome/` | Written for Maleficium | article; `maleficium-doc.sty` (hyperref); Libertinus fonts |

The welcome tour opens on first launch and is not listed in the gallery.

### Classes, packages, and fonts

The templates only name these; Maleficium does not ship them. The engine downloads them from the Tectonic bundle at compile time, each under its own license (mostly the LaTeX Project Public License). The font is Libertinus (SIL Open Font License). Every template also carries `maleficium-footer.sty` and `maleficium-mark.pdf`, which draw the "Made with Maleficium" mark bottom right on each page; delete the `\usepackage{maleficium-footer}` line from a project to remove it. The look is shared: the Resume and CV carry `maleficium.sty`, every other template except the journal carries `maleficium-doc.sty` (the Presentation carries `beamerthememaleficium.sty`), each folder with its own copy so a project stays self-contained. The journal keeps plain IEEEtran formatting, because venues require it. Loading a class or package puts no terms on the document that uses it.

## Adding a built-in template

1. Create `<id>/` with a `template.json` (`id`, `name`, `description`, `category`, `main`) and the project files.
2. Start the main file with the same two-line CC0 header the others use.
3. Add a row to the table above. If the template is adapted from outside work, name the source in **Origin**, and only adapt work whose license allows release under CC0.
4. Check it compiles offline in the app.
