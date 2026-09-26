# Templates

Each folder here is a built-in template: a `template.json` manifest plus the project files it creates. They are embedded in the app at build time and appear under **Built-in** in the New Project from Template gallery.

Templates users save or import are kept in the app's data folder, never here, and appear under **Your templates**.

## License

Every built-in template is dedicated to the public domain under [CC0 1.0](LICENSE). A project made from one belongs to its author, with no credit or notice required. Each template's main file carries a one-line CC0 header so this travels with the copy.

This covers the template files only. The rest of Maleficium is Apache 2.0.

## Built-in templates

| Template | Folder | Origin | Class and packages it loads |
| --- | --- | --- | --- |
| Article | `article/` | Written for Maleficium | article; amsmath, biblatex, booktabs, geometry, hyperref, tikz |
| Assignment | `assignment/` | Written for Maleficium | article; amsmath, amssymb, amsthm, enumitem, geometry |
| Book | `book/` | Written for Maleficium | book; geometry, hyperref |
| Curriculum Vitae | `cv/` | Written for Maleficium | moderncv; geometry |
| Journal Paper (IEEEtran) | `journal/` | Written for Maleficium | IEEEtran; amsmath, cite, graphicx |
| Letter | `letter/` | Written for Maleficium | letter; geometry |
| Presentation | `beamer/` | Written for Maleficium | beamer (Singapore theme); amsmath |
| Report | `report/` | Written for Maleficium | report; amsmath, geometry, graphicx, hyperref |
| Resume | `resume/` | Written for Maleficium | article; enumitem, fontawesome5, fontspec, geometry, hyperref, titlesec, xcolor; TeX Gyre Heros font |
| Welcome tour | `welcome/` | Written for Maleficium | article; geometry, hyperref |

The welcome tour opens on first launch and is not listed in the gallery.

### Classes, packages, and fonts

The templates only name these; Maleficium does not ship them. The engine downloads them from the Tectonic bundle at compile time, each under its own license (mostly the LaTeX Project Public License). The fonts are TeX Gyre Heros (GUST Font License) and Font Awesome (SIL Open Font License). Loading a class or package puts no terms on the document that uses it.

## Adding a built-in template

1. Create `<id>/` with a `template.json` (`id`, `name`, `description`, `category`, `main`) and the project files.
2. Start the main file with the same two-line CC0 header the others use.
3. Add a row to the table above. If the template is adapted from outside work, name the source in **Origin**, and only adapt work whose license allows release under CC0.
4. Check it compiles offline in the app.
