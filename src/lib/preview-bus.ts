// preview-bus.ts — compile output signal to the preview.
//
// One channel carries the whole signal. The stamp is minted here, so callers
// cannot drift a separate generation counter out of sync with the document.

/** The main file whose output is shown, inside a granted session root. */
export interface PreviewSource {
  rootId: string;
  /** Desktop only: the root's absolute path, for mapping relative hits. */
  rootPath: string;
  mainRel: string;
}

/** A granted session root: the id commands take, and its desktop path. */
export type SessionRoot = { rootId: string; path: string };

/** The source whose root contains `abs`, or null when no root does. */
export function sourceFor(abs: string, roots: readonly SessionRoot[]): PreviewSource | null {
  for (const r of roots) {
    if (abs.startsWith(r.path + '/')) {
      return { rootId: r.rootId, rootPath: r.path, mainRel: abs.slice(r.path.length + 1) };
    }
  }
  return null;
}

export interface PreviewDoc {
  /** Desktop: absolute engine-output path. Hosted: an opaque expiring ref. */
  url: string | null;
  /** Monotonic; reopens the preview even when `url` is unchanged. */
  stamp: number;
  /** Stable identity of the source document: `${rootId}:${mainRel}`. */
  docKey: string | null;
  source: PreviewSource | null;
  /** Revision of the source that produced this output, when known. */
  revision: string | null;
}

type Cb = (doc: PreviewDoc) => void;
const set = new Set<Cb>();
let stamp = 0;

export function emitPdf(doc: Omit<PreviewDoc, 'stamp' | 'docKey'>) {
  const docKey = doc.source ? `${doc.source.rootId}:${doc.source.mainRel}` : null;
  const next: PreviewDoc = { ...doc, docKey, stamp: ++stamp };
  set.forEach((cb) => cb(next));
}

export function onPdf(cb: Cb) {
  set.add(cb);
  return () => {
    set.delete(cb);
  };
}
