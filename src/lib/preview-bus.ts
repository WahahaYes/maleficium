// preview-bus.ts — compile output signal to the preview.
//
// One channel carries the whole signal. The stamp is minted here, so callers
// cannot drift a separate generation counter out of sync with the document.

export interface PreviewDoc {
  /** Desktop: absolute engine-output path. Hosted: an opaque expiring ref. */
  url: string | null;
  /** Monotonic; reopens the preview even when `url` is unchanged. */
  stamp: number;
  /** Stable identity of the source document: `${projectId}:${relPath}`. */
  docKey: string | null;
  /** Revision of the source that produced this output, when known. */
  revision: string | null;
}

type Cb = (doc: PreviewDoc) => void;
const set = new Set<Cb>();
let stamp = 0;

export function emitPdf(doc: Omit<PreviewDoc, 'stamp'>) {
  const next: PreviewDoc = { ...doc, stamp: ++stamp };
  set.forEach((cb) => cb(next));
}

export function onPdf(cb: Cb) {
  set.add(cb);
  return () => {
    set.delete(cb);
  };
}
