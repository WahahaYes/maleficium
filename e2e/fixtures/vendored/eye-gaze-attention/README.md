# Eye Gaze as a Signal for Conveying User Attention in Contextual AI Systems

Ethan Wilson, Naveen Sendhilnathan, Charlie S. Burlingham, Yusuf Mansour, Robert Cavin, Sai Deep Tetali, Ajoy Savio Fernandes, and Michael J. Proulx. ETRA '25, [doi:10.1145/3715669.3727349](https://doi.org/10.1145/3715669.3727349). Main file: `main.tex`.

- **Source:** the arXiv source of [arXiv:2501.13878v3](https://arxiv.org/abs/2501.13878).
- **License:** [CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/), as posted on arXiv.
- **Changes:**
  - `figures/query_example.png` and `figures/segment_illustration_flat.png` are full-resolution, verbatim from the arXiv source (2331 and 8258 px wide).
  - Left out: the unused figures, `planning.tex` (its `\input` is commented out), and the arXiv build output `main.bbl`.
  - Everything else is verbatim: `main.tex`, `sections/`, `biblio_baggins.bib`, `acmart.cls`, and all four figures.

## What it exercises

- acmart (`sigconf,nonacm,authorversion`) from a class file shipped beside the paper, overriding the bundle's copy
- Eight `\author` blocks with affiliations, a CC copyright block, and ACM conference metadata
- Seven `\input` sections, `ACM-Reference-Format` BibTeX, and large PNG figures

## Builds today

Yes, with 74 warnings. Many of them are fragments (`''`, `-> MinLibReg-ot1`) of Tectonic's multi-line font-substitution notes, split into separate diagnostics.
