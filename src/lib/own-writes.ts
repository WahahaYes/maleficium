// own-writes.ts — tell our own filesystem writes apart from external ones.
//
// Each own write records what the path should now hold: a content hash, or
// "absent" for a path we removed. A watcher event is an echo exactly while
// the disk still matches that record; the first mismatch is a real change,
// reported and the record dropped. No time window: a save can echo late or
// repeatedly without false alarms, and an external edit landing right after
// a save is never swallowed.

import { fs } from './fs-provider';
import { hashBytes } from './history';

/** What a path should hold after our write: a content hash, or null (absent). */
type Signature = string | null;

export interface OwnWritesIo {
  /** Bytes on disk now, or null when the path is absent or unreadable. */
  read(path: string): Promise<Uint8Array | null>;
}

export interface OwnWrites {
  /** We wrote `content` to `path` (null: we removed `path`). */
  wrote(path: string, content: string | null): void;
  /** Record whatever `path` holds now, after an own change we did not write byte by byte. */
  settled(path: string): void;
  /** True while `path` on disk is still exactly what we left there. */
  isEcho(path: string): Promise<boolean>;
}

const enc = new TextEncoder();

async function signatureOf(bytes: Uint8Array | null): Promise<Signature> {
  return bytes === null ? null : hashBytes(bytes);
}

const diskIo: OwnWritesIo = {
  read: (path) =>
    fs()
      .readBytes(path)
      .catch(() => null),
};

export function createOwnWrites(io: OwnWritesIo = diskIo): OwnWrites {
  const records = new Map<string, Promise<Signature>>();
  return {
    wrote(path, content) {
      records.set(path, signatureOf(content === null ? null : enc.encode(content)));
    },
    settled(path) {
      records.set(path, io.read(path).then(signatureOf));
    },
    async isEcho(path) {
      const record = records.get(path);
      if (!record) return false;
      const [want, have] = await Promise.all([record, io.read(path).then(signatureOf)]);
      if (want === have) return true;
      // A newer own write may have replaced the record meanwhile: keep that one.
      if (records.get(path) === record) records.delete(path);
      return false;
    },
  };
}
