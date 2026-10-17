# Eye gaze as a signal for user attention: the interactive paper

Ethan Wilson, Naveen Sendhilnathan, Charlie S. Burlingham, Yusuf Mansour, Robert Cavin, Sai Deep Tetali, Ajoy Savio Fernandes, and Michael J. Proulx. _Eye Gaze as a Signal for Conveying User Attention in Contextual AI Systems._ ETRA '25, [doi:10.1145/3715669.3727349](https://doi.org/10.1145/3715669.3727349), [arXiv:2501.13878](https://arxiv.org/abs/2501.13878).

One LaTeX source, two outputs. The compiled PDF is pixel-identical to the published paper. The exported reader shows the same paper reflowed for the screen, with four interactive figures. This folder is the source vendored in [e2e/fixtures/vendored/eye-gaze-attention/](../../e2e/fixtures/vendored/eye-gaze-attention/) plus the widgets.

## What is live in the reader

| Figure | In the PDF | In the reader | Widget |
| --- | --- | --- | --- |
| 1 `fig:et_relationship` | `segment_illustration_flat.png` | Eye-tracking error slider over an apple and a pencil, with accuracy bounds, tracker presets and simulated gaze samples | html widget `widgets/et-error/` |
| 2 `fig:visual_sizes` | `vis_size.png` | Measure switch, tracker-error slider, click to pick a coverage percentile | runtime `gaze-curves@1`, `view=coverage` |
| 3 `fig:e1e2` | `e1e2.png` | Task switch, interval and legend toggles, click to pick a context size | runtime `gaze-curves@1`, `view=context` |
| 4 `fig:query_example` | `query_example.png` | One context slider drives the scanpath, the prompt it builds and the accuracy curve | runtime `gaze-timeline@1` |

PNGs are in `figures/`; runtimes are in `runtimes/`.

The only changes to the source are the `\usepackage{maleficium-interactive}` block (with its reader settings) in `main.tex` and four `\includegraphics` lines in `sections/eval_object_size.tex` and `sections/eval_vlm.tex`, each replaced by a widget macro. Every widget uses the published PNG as its `poster=`, so the PDF is unchanged. Its larger `height=` only sizes the reader's frame to fit the controls.

## Open it

1. Open this folder in Maleficium and compile `main.tex`. The PDF needs no approval.
2. In View > Widgets, approve the html widget and allow the two custom runtimes. Until you do, the reader shows their posters.
3. File > Preview in Browser shows the reader. File > Export Paper Bundle saves it as a folder for web hosting or as one file.

## Where the numbers come from

The raw trial logs and per-object measurements are not available, so the chart data is **digitized from the published figures** by `tools/digitize_e1e2.py` and `tools/digitize_vis_size.py`, calibrated on each plot's own gridlines. Rerunning them reproduces `data/` exactly (Python 3 with numpy and Pillow).

- `data/e1e2.csv` has one row per experiment and context size. Three points use the values stated in the paper's text (`source=text`): E1 with no context 10.3% [8.3, 12.3], the E1 peak 24.8% [22.1, 27.7] at 6, and the E2 peak 49.5% [43, 56.1] at 6. Every other value is a pixel reading (`source=figure`), and the chart's tooltip says so. Pixel readings land within about 0.1 points of the stated values.
- `data/vis_size.csv` has each curve at every whole percentile from 95 to 5. The 50% readings match all eight values the paper prints within 0.03°.
- `scanpath/sample.json` is the published E1 example. Labels, order, ground truth and response are as published. Positions on the frame are placed by hand on the visible objects, for illustration. Frisbee and Chopping board are listed as not in this view, because the frame does not show them. Fixation durations are not published and are not shown.
- The objects in the Figure 1 widget are illustrative shapes. The apple is scaled to the median fixated object's circular bound (4.07°).

## Licences

- The paper, its figures, and everything derived from them (`data/`, `scanpath/`, the runtimes' `samples/`) are [CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/), as posted on arXiv. Credit the authors above.
- `acmart.cls` is the ACM class, under the LaTeX Project Public License.
- The widget, the two runtimes and the tools are MIT-0 (see each runtime's `LICENSE`), so you can copy them into your own papers with no obligations. `maleficium-interactive.sty` is Maleficium's package, also MIT-0.
