# On the Origin of Objects

Yegor Bugayenko and Maxim Trunnikov. Main file: `paper.tex`.

- **Source:** [objectionary/on-the-origin-of-objects](https://github.com/objectionary/on-the-origin-of-objects) at `045c151302783d2d2a61b0b122210f3404c9391b`. The bibliography comes from its submodule [yegor256/bibliography](https://github.com/yegor256/bibliography) at `cb985c0d44af12d801241213a47fc555ad575746`.
- **License:** MIT (`LICENSE.txt`), for the paper and the bibliography.
- **Changes:** `bibliography/main.bib` keeps only the 23 entries the paper cites (the full file is 270 KB). Everything else is verbatim: `paper.tex`, `sections/`, `.latexmkrc`, `LICENSE.txt`. Repo tooling (CI, Makefile, spell-check lists) is left out.

## What it exercises

- acmart (`acmsmall,nonacm`), natbib with `ACM-Reference-Format`, cleveref, babel with Russian, `T2A` font encoding
- 18 `\input` sections and a bibliography in a subfolder
- The author's own packages: `ffcode` for code listings and `eolang` for φ-calculus

## Builds today

No. `ffcode` loads `minted`, which runs Pygments through shell escape. The authors build with `pdflatex --shell-escape` (`.latexmkrc`). The app's pre-compile check reports `missing: minted (shell-escape-required)`.

With shell escape forced on (`tectonic -Z shell-escape`), the next stop is `paper.tex:11: Undefined control sequence` at `\lst@AddToHook`. The paper targets a newer `ffcode` (built on `listings`) than the TeX Live 2022 bundle has.
