// SyncTeX/pager jump lock for the windowed PDF preview.
//
// A jump is a lock (`{target, until}`) plus a pending scroll: the bitmap
// window moves at once and the scroll lands once the target bitmap exists.
// Programmatic scrolls carry a flag so a user grab mid-jump cancels the jump
// instead of fighting the hand on the scrollbar.

import { useCallback, useEffect, useLayoutEffect, useRef, type RefObject } from 'react';
import { compensateScrollTop } from '../lib/previewNav';

/** Top padding of the scroll container (matches py:1) for jump math. */
const SCROLL_PAD_TOP = 8;

/** Jump lock lifetime: the observer landing clears earlier; this is the backstop. */
export const JUMP_LOCK_MS = 1500;

export interface SyncLockParams {
  /** Page from the pager prop, clamped to the document. */
  page: number;
  docKey: string;
  dimsVersion: number;
  scrollRef: RefObject<HTMLDivElement | null>;
  shellRefs: RefObject<Map<number, HTMLDivElement>>;
  lockRef: RefObject<{ target: number; until: number } | null>;
  pendingScrollRef: RefObject<number | null>;
  compRef: RefObject<{ page: number; delta: number } | null>;
  visibleRef: RefObject<number>;
  setVisible: (n: number) => void;
}

export interface SyncLock {
  /** Scroll the container so page `n` sits at the top of the viewport. */
  scrollToShell: (n: number) => void;
  /** Arm the backstop that drops a lock the observer never cleared. */
  armJumpTimeout: (target: number) => void;
  /** onScroll handler: cancels an in-flight jump on a user scroll. */
  handleScroll: () => void;
}

export function useSyncLock({
  page,
  docKey,
  dimsVersion,
  scrollRef,
  shellRefs,
  lockRef,
  pendingScrollRef,
  compRef,
  visibleRef,
  setVisible,
}: SyncLockParams): SyncLock {
  const programmaticRef = useRef(false);
  const jumpTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (jumpTimerRef.current) clearTimeout(jumpTimerRef.current);
    },
    [],
  );

  // Jump the viewport to a shell via rect deltas (offsetTop is
  // offsetParent-relative, so rect math against the container is exact).
  // The flag is set ONLY when scrollTop actually moves: a no-op scroll fires
  // no event, and a stale flag would eat the next genuine user scroll.
  const scrollToShell = useCallback(
    (n: number) => {
      const box = scrollRef.current;
      const shell = shellRefs.current.get(n);
      if (!box || !shell) return;
      const delta =
        shell.getBoundingClientRect().top - box.getBoundingClientRect().top - SCROLL_PAD_TOP;
      const top = box.scrollTop + delta;
      if (Math.abs(top - box.scrollTop) > 0.5) {
        programmaticRef.current = true;
        box.scrollTop = top;
      }
    },
    [scrollRef, shellRefs],
  );

  const armJumpTimeout = useCallback(
    (target: number) => {
      if (jumpTimerRef.current) clearTimeout(jumpTimerRef.current);
      jumpTimerRef.current = setTimeout(() => {
        if (lockRef.current?.target === target) lockRef.current = null;
        if (pendingScrollRef.current === target) pendingScrollRef.current = null;
      }, JUMP_LOCK_MS);
    },
    [lockRef, pendingScrollRef],
  );

  // Pager/SyncTeX jump: a prop page that differs from the visible page is an
  // EXTERNAL jump (our own scroll reports echo back equal and no-op here).
  // The bitmap window moves at once so the target renders; the scroll lands
  // in the render effect once that bitmap exists. User scrolls mid-jump
  // cancel the lock via the onScroll handler below — the scroll wins, no
  // second programmatic scroll fires after it.
  useEffect(() => {
    if (!docKey) return;
    if (page === visibleRef.current) return;
    visibleRef.current = page;
    setVisible(page);
    pendingScrollRef.current = page;
    lockRef.current = { target: page, until: Date.now() + JUMP_LOCK_MS };
    armJumpTimeout(page);
  }, [page, docKey, armJumpTimeout, lockRef, pendingScrollRef, setVisible, visibleRef]);

  // Probe updates give non-uniform shells their true heights after mount;
  // re-anchor the SAME page the snapshot named so content doesn't drift under
  // the reader. A jump in flight re-lands exact instead.
  useLayoutEffect(() => {
    const c = compRef.current;
    compRef.current = null;
    if (!c) return;
    const pending = pendingScrollRef.current;
    if (pending != null) {
      scrollToShell(pending);
      return;
    }
    const box = scrollRef.current;
    const shell = shellRefs.current.get(c.page);
    if (!box || !shell) return;
    const shift = shell.getBoundingClientRect().top - box.getBoundingClientRect().top - c.delta;
    if (Math.abs(shift) <= 0.5) return;
    // Growth (content above gained height) pushes down by exactly the added
    // height; refinement shrink pulls up signed — same anchor, both ways.
    const top = shift > 0 ? compensateScrollTop(box.scrollTop, shift) : box.scrollTop + shift;
    if (Math.abs(top - box.scrollTop) > 0.5) {
      programmaticRef.current = true;
      box.scrollTop = top;
    }
  }, [dimsVersion, scrollToShell, compRef, pendingScrollRef, scrollRef, shellRefs]);

  // Picking comes from the observer (rAF-throttled). Scroll events only
  // separate programmatic jumps from user scrolls: a user grab mid-jump
  // cancels it so the viewport never fights the hand on the scrollbar.
  const handleScroll = useCallback(() => {
    if (programmaticRef.current) {
      programmaticRef.current = false;
      return;
    }
    if (lockRef.current) {
      lockRef.current = null;
      pendingScrollRef.current = null;
    }
  }, [lockRef, pendingScrollRef]);

  return { scrollToShell, armJumpTimeout, handleScroll };
}
