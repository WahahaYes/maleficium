# playground/ — committed test fixtures (NOT app code)

One directory per fixture; the harness and manual testing pick a fixture by path. Nothing here ships in the product bundle.

- `simple/` — the original playground project: multi-file `\input`, bibliography, figures, labels, `%!TEX root` magic, plus a `stress/` 50-page doc for pager/Cancel/viewport tests.

Small sources only, committed; megabytes generate into OS tmp at runtime. PNGs: regenerate only via each fixture's `gen-figs.py` — never write them through a text tool or editor (UTF-8 mangling corrupts the signature).
