// surface.ts — the widget-host seam. The preview drives live widgets through
// this interface and never through a platform: the desktop impl talks to the
// app's widget host (a helper process per widget); a platform without one
// leaves the surface unset, and every widget stays its poster.

import type { WidgetSession } from '../generated/api';
import type { BusEvent, WidgetApprovalScope } from '../generated/events';
import type { DeviceRect } from './overlay';

/** Where one widget sits, in device px of the page's viewport. */
export interface ViewItem {
  id: string;
  slot: DeviceRect;
  clip: DeviceRect;
  visible: boolean;
  /** In or near the pane: a candidate for a live slot. */
  inView: boolean;
  /** Lower ranks win a live slot first. */
  rank: number;
}

export interface WidgetSurface {
  /** Opens the paper's widgets; the live ones start as they come into view. */
  open(rootId: string, mainRel: string, dark: boolean): Promise<WidgetSession>;
  /** The user granted or revoked an approval; applied at once. */
  approve(
    rootId: string,
    mainRel: string,
    scope: WidgetApprovalScope,
    granted: boolean,
  ): Promise<WidgetSession>;
  view(items: ViewItem[]): void;
  activate(id: string, on: boolean): void;
  /** Takes input back from every widget and gives it to the editor. */
  deactivateAll(): void;
  /** Hides every live widget while a menu or dialog covers the preview. */
  setHidden(hidden: boolean): void;
  setTheme(dark: boolean): void;
  reload(id: string): void;
  /** Ends every widget. */
  close(): void;
  /** Every host event as a bus event; returns the unsubscribe. */
  onEvent(cb: (e: BusEvent) => void): () => void;
}

let impl: WidgetSurface | null = null;

/** Register the platform implementation. Called once at boot. */
export function setWidgetSurface(next: WidgetSurface | null) {
  impl = next;
}

/** The platform's widget host, or null where widgets stay posters. */
export function widgetSurface(): WidgetSurface | null {
  return impl;
}
