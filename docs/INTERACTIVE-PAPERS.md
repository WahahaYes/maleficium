# Interactive papers

A paper stays a normal LaTeX document that compiles to the PDF a venue wants. It can also export a self-contained web bundle where figures are live. [examples/eye-gaze-interactive/](../examples/eye-gaze-interactive/) is a worked example.

## Setup

Projects made from a template already hold `maleficium-interactive.sty`. It is an ordinary project file. For an older project, copy [the package](../embed-runtime/tex/maleficium-interactive.sty) next to your main file.

## Widgets

`\interactive[...]{widgets/demo/}` embeds a folder of your own HTML. Every other widget is `\interactiveruntime[runtime=..., ...]{file}`, where `runtime=` names what shows the file:

| `runtime=`       | Shows                                         |
| ---------------- | --------------------------------------------- |
| `model@1`        | a glTF or GLB 3D model                        |
| `chart@1`        | a Vega-Lite chart                             |
| `table@1`        | a sortable table from CSV                     |
| `video@1`        | a local video file                            |
| `<name>@<major>` | a custom runtime ([RUNTIMES.md](RUNTIMES.md)) |

Each needs `alt=` text. The package's header comments show their options by example. A built-in runtime also takes the options its runtime declares, such as `loop=true` for a video or `sort={score desc}` and `columns={name;score}` for a table. Separate the items of a list with `;`.

## Posters

In the PDF, each widget is a still poster. Give one with `poster=`, or let Maleficium render it when you compile (models, charts, and HTML widgets). A video needs `poster=` or `at=`: `\interactiveruntime[runtime=video@1, at=2.5, alt={...}]{media/clip.mp4}` makes the frame 2.5 seconds in its poster, and the live video starts there too. A frame that renders blank is refused and leaves a placeholder; H.264 MP4 renders on every platform.

Rendered posters are cached in `.maleficium/` beside your main file. It is safe to delete. Commit it if the paper should compile with its posters without Maleficium.

## Approving your own widgets

An HTML widget or custom runtime is code, so it runs in the app only after you approve its exact content in **View > Widgets**. Any edit asks again. An agent cannot approve.

Auto-approval is a per-project setting, off by default. It approves content changes and new widgets that declare no origins. It never approves a new origin.

A widget may declare `https` origins it frames or loads from, such as a YouTube player: see [VIDEO-EMBEDS.md](VIDEO-EMBEDS.md).

## Preview and export

**File > Preview in Browser** opens the paper in your browser. **File > Export Paper Bundle** writes it as a folder (for a web server) or one file (for email or opening from disk). Neither writes into your project.

The bundle's `index.html` is the paper reflowed for the screen, with each widget in place. `paper.pdf` comes along as the version of record. LaTeX the converter can't read shows as raw source and is listed in the export's warnings.

## Styling the reader

The reader has light and dark modes. To restyle it, name a stylesheet in your source:

```latex
\maleficiumreaderstyle{reader.css}
```

```css
/* reader.css: the light mode, then what changes in dark mode */
.m-reader {
  --m-color-bg: #faf3e8;
  --m-color-accent: #7a4a1e;
}
.m-reader[data-theme='dark'] {
  --m-color-bg: #221c17;
  --m-color-accent: #d2a679;
}
```

It applies after the reader's own rules. Set the `--m-*` variables for both modes. `url()`s may name fonts and images in the project; they are inlined so the page works offline. `@import`, remote URLs, and anything that runs script are refused with a warning. The PDF never changes.

## Limits

- Interactive papers are tested on Linux. Poster rendering on macOS and Windows is untested.
- Preview in Browser and exported bundles run every widget without asking. Approval gates code inside the app only.
- A bundle's content security policy limits what widgets load, not every connection a browser makes ([details](VIDEO-EMBEDS.md#before-you-publish)).
- In a single-file bundle, a widget can use only its declared frame origins ([details](VIDEO-EMBEDS.md#what-the-declaration-does)).
