# Vendored papers

Real papers, vendored so the app and its harnesses meet the LaTeX people actually write: journal and conference classes, long preambles, split sources, big bibliographies, and packages the bundled engine may not handle. A paper that fails to compile is still useful: it records something the app needs to handle for real work.

Each folder is one paper with its own `README.md`: where it came from, its license, what was changed, and how it builds today.

To add a paper:

- Its license must allow redistribution and changes: MIT, CC-BY, CC0, or similar. Not CC-BY-NC or CC-BY-ND, and not arXiv's default license.
- Vendor the sources the paper builds from, not build output (`.bbl`, `.aux`, `.pdf`), repo tooling, or unused files.
- Keep every file under 500 KB (the commit hook's limit): downscale large figures and keep their file names so `\includegraphics` paths still resolve.
- Record every change in the paper's README; CC-BY requires it.
- Record how it builds in the app today: success, warnings worth noting, or the first blocker.

| Paper | Class | License | Builds today |
| --- | --- | --- | --- |
| [`on-the-origin-of-objects/`](on-the-origin-of-objects/) | acmart (acmsmall) | MIT | No: needs shell escape (`minted`) |
| [`eye-gaze-attention/`](eye-gaze-attention/) | acmart (sigconf), class file shipped with the paper | CC-BY 4.0 | Yes, with warnings |
