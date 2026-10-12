# Templates

Each folder here is a built-in template: a `template.json` manifest plus the project files it creates. The manifest names the template, its gallery category and its main file. Built-in templates are embedded in the app at build time and appear under **Built-in** in the New Project from Template gallery.

Templates users save or import are kept in the app's data folder, never here, and appear under **Your templates**.

The `welcome` template is the first-run tour. It opens on first launch, when there are no recent projects, and from **Help > Welcome**. The gallery never lists it: `list` in `src-tauri/core/src/templates.rs` skips the `WELCOME` id.

## License

Every built-in template is dedicated to the public domain under [CC0 1.0](LICENSE). A project made from one belongs to its author, with no credit or notice required. Each template's main file starts with a two-line CC0 header so this travels with the copy.

This covers the template files only. The rest of Maleficium is AGPL-3.0, except [embed-runtime/](../../embed-runtime/README.md), which is MIT-0.

The classes, packages and fonts a template loads are not shipped with Maleficium. The engine downloads them from the TeX bundle at compile time, each under its own licence. Loading a class or package puts no terms on the document that uses it.

A template adapted from outside work names its source in the header of its main file. Only adapt work whose licence allows release under CC0.

## Shared files

Style files used by more than one template live once in `shared/`. `shared/overlay.txt` says which templates get each file. `core/build.rs` copies them into each template's embedded files, so a new project still holds its own copies and stays self-contained. `e2e/templates.py` reads the same map for scripts that build template projects outside the app.

Every template gets `maleficium-footer.sty`, which draws the "Made with Maleficium" mark at the bottom right of each page. To remove the mark, delete the `\usepackage{maleficium-footer}` line from the project.

## Adding a built-in template

1. Create `<id>/` with a `template.json` and the project files. Copy the fields from an existing template.
2. If it loads a shared file, add the template's id to that file's line in `shared/overlay.txt`. Never copy a shared file into the template folder; the build refuses it.
3. Start the main file with the same two-line CC0 header the others use, plus the source if the template is adapted.
4. Check it compiles offline in the app.
