# `simple` fixture (test fixture, NOT app code)

Open in the app via **File → Open Project…** on a copy (`sh scripts/playground.sh` makes one in `playground/simple/`), or open single files directly.

## Files

- `main.tex` — root document (`\documentclass`, `\input` chapters, `\bibliography{refs}`, two `\includegraphics`, `\label`/`\ref`).
- `chapters/background.tex` — carries `%!TEX root = ../main.tex` magic. Open THIS file and compile: the app must build `main.tex`.
- `chapters/method.tex` — cross-references labels in other files.
- `refs.bib` — three entries (`\cite`d from main + chapters).
- `figs/diagram.png`, `figs/photo.png` — valid 64×64 RGB PNGs. REGENERATE ONLY with `python3 e2e/fixtures/simple/gen-figs.py` (or the same zlib recipe) — never write them through a text tool or editor: UTF-8 mangling corrupts the signature and breaks `\includegraphics` with `Unable to load picture or PDF file`. If `file figs/*.png` ever reports `data` instead of `PNG image data`, rerun the script.
- `stress/big.tex` — generated 50-page doc (section + `\newpage` loop) for pager jumps, Cancel mid-compile, and viewport smoothness.

## What to exercise here

1. Open project → tree shows `main.tex` with the `main` chip (scan finds the single `\documentclass`); Set-as-main persists across restart.
2. Open `background.tex` → compile → `main.pdf` builds (magic-comment path).
3. Break something (`\badcommand` in `method.tex`) → failure + red `file:line` rows → click row → caret lands.
4. Delete a figure → Undo restores it; `git status --porcelain` in a scratch copy stays clean of app footprint.
5. `stress/big.tex` → compile → Cancel mid-run; pager jump to page 40+.

Regenerate: PNGs via `e2e/fixtures/simple/gen-figs.py`, `stress/big.tex` via a section + `\newpage` loop.
