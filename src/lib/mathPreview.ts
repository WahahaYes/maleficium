// mathPreview.ts — the editor's math preview: with the caret in a formula,
// or the pointer over one, a tooltip above it shows the formula rendered
// (Temml, to MathML), with the project's own macros. Escape hides the
// caret's preview until the caret leaves the formula.
//
// A macro Temml does not know renders in the error colour: the preview
// reads like the paper only where the renderer knows what TeX would do.

import { StateEffect, StateField, Prec, type Extension } from '@codemirror/state';
import { EditorView, hoverTooltip, keymap, showTooltip, type Tooltip } from '@codemirror/view';
import type { ProjectMacro } from './generated/index';
import { mathAt, mathSpans, type MathSpan } from './mathSpans';

export interface MathPreviewSource {
  /** The project's macros; resolved when a preview opens. */
  macros: () => Promise<ProjectMacro[]>;
  /** Colour for TeX the renderer cannot read. */
  errorColor: () => string;
}

/** Temml's macro table from the project's definitions. An operator is
 *  `\operatorname{…}`; every other body is its expansion (`#1`… are the
 *  arguments, counted by Temml). */
export function temmlMacros(list: readonly ProjectMacro[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of list) {
    out[m.name] = m.command === 'DeclareMathOperator' ? `\\operatorname{${m.body}}` : m.body;
  }
  return out;
}

const hidePreview = StateEffect.define<null>();

const spansField = StateField.define<MathSpan[]>({
  create: (state) => mathSpans(state.doc.toString()),
  update: (spans, tr) => (tr.docChanged ? mathSpans(tr.state.doc.toString()) : spans),
});

interface Preview {
  span: MathSpan | null;
  /** Start of the formula Escape hid, mapped through edits. */
  hiddenAt: number | null;
  tooltip: Tooltip | null;
}

/** The math preview extension. */
export function mathPreview(source: MathPreviewSource): Extension {
  const tooltipOf = (span: MathSpan): Tooltip => ({
    pos: span.from,
    end: span.to,
    above: true,
    create: () => {
      const dom = document.createElement('div');
      dom.className = 'cm-math-preview';
      void render(dom, span, source);
      return { dom };
    },
  });

  const previewField = StateField.define<Preview>({
    create: () => ({ span: null, hiddenAt: null, tooltip: null }),
    update(v, tr) {
      const sel = tr.state.selection.main;
      const span = sel.empty ? mathAt(tr.state.field(spansField), sel.head) : null;
      if (!span) return { span: null, hiddenAt: null, tooltip: null };
      let hiddenAt = v.hiddenAt === null ? null : tr.changes.mapPos(v.hiddenAt);
      if (tr.effects.some((e) => e.is(hidePreview))) hiddenAt = span.from;
      if (hiddenAt === span.from) return { span, hiddenAt, tooltip: null };
      const same = v.span && v.tooltip && v.span.from === span.from && v.span.tex === span.tex;
      return { span, hiddenAt: null, tooltip: same ? v.tooltip : tooltipOf(span) };
    },
    provide: (f) => showTooltip.from(f, (v) => v.tooltip),
  });

  return [
    spansField,
    previewField,
    // The pointer over a formula the caret's preview is not already showing.
    hoverTooltip((view, pos) => {
      const span = mathAt(view.state.field(spansField), pos);
      if (!span || view.state.field(previewField).tooltip?.pos === span.from) return null;
      return tooltipOf(span);
    }),
    Prec.high(
      keymap.of([
        {
          key: 'Escape',
          run: (view) => {
            if (!view.state.field(previewField).tooltip) return false;
            view.dispatch({ effects: hidePreview.of(null) });
            return true;
          },
        },
      ]),
    ),
    EditorView.baseTheme({
      '.cm-tooltip .cm-math-preview': {
        padding: '4px 10px',
        maxWidth: '80ch',
        overflowX: 'auto',
      },
      '.cm-math-preview math': {
        fontFamily: '"Libertinus Math", "STIX Two Math", "Cambria Math", math',
        fontSize: '1.25em',
      },
      '.cm-math-preview math[display="block"]': { display: 'block math' },
    }),
  ];
}

async function render(dom: HTMLElement, span: MathSpan, source: MathPreviewSource) {
  const [{ default: temml }, macros] = await Promise.all([
    import('temml'),
    source.macros().then(temmlMacros, () => ({})),
  ]);
  temml.render(span.tex, dom, {
    displayMode: span.display,
    throwOnError: false,
    errorColor: source.errorColor(),
    macros,
  });
}
