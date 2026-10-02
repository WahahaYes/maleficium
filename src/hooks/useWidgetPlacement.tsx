// useWidgetPlacement.tsx — interactive widgets over the preview's PDF pages.
//
// Each widget gets a slot box inside its page box, positioned in percent of
// the page so it follows every zoom. Live widgets are drawn by the widget
// host over their slot; this hook tells the host where each slot is on
// every scroll, zoom and resize, which widget has input, and when a menu or
// dialog covers the preview. Posters are the PDF itself: a slot adds only a
// short note when the user needs one.

import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';
import type { ReactNode, RefObject } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Typography from '@mui/material/Typography';
import { emit } from '../lib/events';
import type { WidgetSlotLayout } from '../lib/generated/events';
import type { WidgetsModel } from './useWidgetSession';
import {
  centreDistance,
  nearPane,
  placement,
  slotPercent,
  toDevice,
  type CssBox,
  type PageSize,
} from '../lib/widgets/overlay';
import { noteFor, wantsSlot } from '../lib/widgets/state';
import type { ViewItem } from '../lib/widgets/surface';

/** Selector for anything that draws over the preview: live widgets hide under it. */
const POPUPS = '.MuiPopover-root, .MuiModal-root, .MuiPopper-root';
/** How far outside the pane a widget starts, in panes. */
const NEAR = 0.5;
/** Quiet time before the settled layout is reported. */
const LAYOUT_MS = 400;

const boxOf = (r: DOMRect): CssBox => ({
  left: r.left,
  top: r.top,
  width: r.width,
  height: r.height,
});

export interface PlacementArgs {
  model: WidgetsModel;
  scrollRef: RefObject<HTMLDivElement | null>;
  shellRefs: RefObject<Map<number, HTMLDivElement>>;
  pageSize: (n: number) => PageSize;
  /** The zoom percent shown, for the layout report. */
  percent: number;
  /** Anything that moves pages: a change re-places every widget. */
  layoutKey: string;
}

/** Places live widgets and returns the slots to render inside page `n`. */
export function useWidgetPlacement({
  model,
  scrollRef,
  shellRefs,
  pageSize,
  percent,
  layoutKey,
}: PlacementArgs): (page: number) => ReactNode {
  const { surface, widgets, session, states } = model;
  const slots = useRef(new Map<string, HTMLDivElement>());
  const lastView = useRef('');
  const lastLayout = useRef('');
  const raf = useRef(0);
  const layoutTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const live = useRef({ widgets, states, percent, pageSize });
  live.current = { widgets, states, percent, pageSize };
  const active = Object.values(states).some((s) => s.active);

  const report = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const pane = boxOf(el.getBoundingClientRect());
    const dpr = window.devicePixelRatio || 1;
    const out: WidgetSlotLayout[] = [];
    for (const w of live.current.widgets) {
      const slot = slots.current.get(w.id);
      const shell = shellRefs.current?.get(w.page);
      if (!slot || !shell) continue;
      const box = boxOf(slot.getBoundingClientRect());
      if (!placement(box, pane, dpr).visible) continue;
      const size = live.current.pageSize(w.page);
      out.push({
        id: w.id,
        page: w.page,
        slot: toDevice(box, dpr),
        pageBox: toDevice(boxOf(shell.getBoundingClientRect()), dpr),
        pageWidthPt: size.width,
        pageHeightPt: size.height,
      });
    }
    const key = JSON.stringify(out);
    if (key === lastLayout.current) return;
    lastLayout.current = key;
    const pct = Math.round(live.current.percent);
    emit({
      scope: 'preview',
      kind: 'info',
      actor: 'system',
      message: `widget layout at ${pct}%: ${out.length} in view`,
      event: { action: 'widgets.layout', percent: pct, slots: out },
    });
  }, [scrollRef, shellRefs]);

  const compute = useCallback(() => {
    raf.current = 0;
    const el = scrollRef.current;
    if (!el || !surface) return;
    const r = el.getBoundingClientRect();
    // The pane without its scrollbars.
    const pane: CssBox = {
      left: r.left,
      top: r.top,
      width: el.clientWidth,
      height: el.clientHeight,
    };
    const dpr = window.devicePixelRatio || 1;
    const items: ViewItem[] = [];
    for (const w of live.current.widgets) {
      const st = live.current.states[w.id];
      const slot = slots.current.get(w.id);
      if (!st || !wantsSlot(st.phase) || !slot) continue;
      const box = boxOf(slot.getBoundingClientRect());
      const p = placement(box, pane, dpr);
      items.push({
        id: w.id,
        slot: p.slot,
        clip: p.clip,
        visible: p.visible,
        inView: nearPane(box, pane, pane.height * NEAR),
        rank: Math.round(centreDistance(box, pane)),
      });
    }
    const key = JSON.stringify(items);
    if (key !== lastView.current) {
      lastView.current = key;
      surface.view(items);
    }
    if (layoutTimer.current) clearTimeout(layoutTimer.current);
    layoutTimer.current = setTimeout(report, LAYOUT_MS);
  }, [scrollRef, surface, report]);

  const schedule = useCallback(() => {
    if (!raf.current) raf.current = requestAnimationFrame(compute);
  }, [compute]);

  // Any layout change re-places; so do scroll and resize.
  useLayoutEffect(() => {
    schedule();
  }, [schedule, layoutKey, widgets, states, session]);
  // A new session (an approval changed) restarts widgets host-side, and a
  // restarted widget only starts again on a view it has not already been sent.
  useLayoutEffect(() => {
    lastView.current = '';
  }, [session]);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule);
    const ro = new ResizeObserver(schedule);
    ro.observe(el);
    return () => {
      el.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
      ro.disconnect();
      if (raf.current) cancelAnimationFrame(raf.current);
      raf.current = 0;
      if (layoutTimer.current) clearTimeout(layoutTimer.current);
    };
    // `layoutKey` re-runs this when the scroll pane mounts after the hook (it is not
    // there on the first render), so the listener is never missed.
  }, [scrollRef, schedule, layoutKey]);

  // Native views draw above the page: hide them under any menu or dialog.
  useEffect(() => {
    if (!surface) return;
    let hidden = false;
    const check = () => {
      const h = document.querySelector(POPUPS) != null;
      if (h !== hidden) {
        hidden = h;
        surface.setHidden(h);
      }
    };
    const mo = new MutationObserver(check);
    mo.observe(document.body, { childList: true });
    check();
    return () => {
      mo.disconnect();
      if (hidden) surface.setHidden(false);
    };
  }, [surface]);

  // A click anywhere but a widget gives input back to the editor.
  useEffect(() => {
    if (!surface || !active) return;
    const down = (e: MouseEvent) => {
      const t = e.target as Element | null;
      if (!t?.closest?.('[data-widget-slot]')) surface.deactivateAll();
    };
    document.addEventListener('mousedown', down, true);
    return () => document.removeEventListener('mousedown', down, true);
  }, [surface, active]);

  const entryOf = (id: string) => session?.entries.find((e) => e.id === id);

  return (page: number) => {
    const here = widgets.filter((w) => w.page === page);
    if (here.length === 0 || !session) return null;
    const size = pageSize(page);
    return here.map((w) => {
      const st = states[w.id];
      if (!st) return null;
      const pct = slotPercent(w.rect, size);
      const runs = wantsSlot(st.phase);
      const note = noteFor(st.phase, entryOf(w.id));
      return (
        <div
          key={w.id}
          data-widget-slot={w.id}
          data-widget-phase={st.phase}
          ref={(el) => {
            if (el) slots.current.set(w.id, el);
            else slots.current.delete(w.id);
          }}
          onMouseDown={() => {
            if (runs && surface) surface.activate(w.id, true);
          }}
          onClick={(e) => {
            if (runs) e.stopPropagation();
          }}
          style={{
            position: 'absolute',
            left: `${pct.left}%`,
            top: `${pct.top}%`,
            width: `${pct.width}%`,
            height: `${pct.height}%`,
            cursor: runs ? 'pointer' : undefined,
            outline: st.active ? '2px solid currentColor' : undefined,
            pointerEvents: runs ? 'auto' : 'none',
          }}
        >
          {note ? (
            <Box
              sx={{
                position: 'absolute',
                left: 0,
                bottom: '100%',
                mb: 0.25,
                px: 0.75,
                borderRadius: 1,
                bgcolor: 'background.paper',
                border: 1,
                borderColor: 'divider',
                display: 'flex',
                alignItems: 'center',
                gap: 0.5,
                pointerEvents: 'auto',
                whiteSpace: 'nowrap',
              }}
            >
              <Typography variant="caption">{note}</Typography>
              {st.phase === 'stopped' ? (
                <Button
                  size="small"
                  sx={{ minWidth: 0, py: 0 }}
                  onClick={(e) => {
                    e.stopPropagation();
                    model.reload(w.id);
                  }}
                >
                  Reload
                </Button>
              ) : null}
            </Box>
          ) : null}
        </div>
      );
    });
  };
}
