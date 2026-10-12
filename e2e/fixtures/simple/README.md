# `simple` fixture

A small multi-file test project: a main file with `\input` chapters, a chapter carrying a `%!TEX root` magic comment, cross-file `\label`/`\ref`, a BibTeX bibliography, two PNG figures, and a long generated document under `stress/` for paging and cancel tests.

Open a copy in the app with File > Open Project. `sh scripts/playground.sh` makes one in `playground/simple/`.

The PNGs are binary. Regenerate them with `python3 e2e/fixtures/simple/gen-figs.py`, and never write them through a text editor or text tool: re-encoding as UTF-8 corrupts them, and `\includegraphics` then fails with `Unable to load picture or PDF file`. If `file figs/*.png` reports `data` instead of `PNG image data`, rerun the script.

## Things to try

1. Open `chapters/background.tex` and compile: the app builds `main.tex`.
2. Put `\badcommand` in `chapters/method.tex` and compile: the error rows link to the line.
3. Compile `stress/big.tex`, cancel mid-run, and jump to a late page.
