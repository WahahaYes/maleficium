import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { eventOf } from './events';

const { mockGetDocument } = vi.hoisted(() => ({ mockGetDocument: vi.fn() }));

vi.mock('pdfjs-dist/legacy/build/pdf.mjs', () => ({
  GlobalWorkerOptions: {},
  TextLayer: class FakeTextLayer {},
  getDocument: (...args: unknown[]) => mockGetDocument(...args),
}));

type ErrorListener = (e: Event) => void;

// The module worker's load failure: constructed, then a bare 'error'.
class FakeWorker {
  static instances: FakeWorker[] = [];
  private listeners = new Map<string, ErrorListener[]>();
  terminated = false;
  constructor(
    readonly url: URL,
    readonly options?: WorkerOptions,
  ) {
    FakeWorker.instances.push(this);
  }
  addEventListener(type: string, cb: ErrorListener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), cb]);
  }
  removeEventListener(type: string, cb: ErrorListener): void {
    this.listeners.set(type, (this.listeners.get(type) ?? []).filter((c) => c !== cb));
  }
  terminate(): void {
    this.terminated = true;
  }
  fireError(e: Event): void {
    [...(this.listeners.get('error') ?? [])].forEach((cb) => cb(e));
  }
}

// A module reset gives pdfjs a fresh worker cache — and a fresh transport
// singleton alongside it, which the test must read through.
async function freshPdfjs() {
  vi.resetModules();
  FakeWorker.instances.length = 0;
  const { transport } = await import('./event-transport');
  transport().clear();
  const pdfjs = await import('./pdfjs');
  const loadFailedEvents = () =>
    transport()
      .snapshot()
      .filter((e) => e.event.action === 'preview.load-failed');
  return { pdfjs, loadFailedEvents };
}

beforeEach(() => {
  vi.stubGlobal('Worker', FakeWorker);
  mockGetDocument.mockReset();
  mockGetDocument.mockReturnValue({ promise: new Promise(() => {}) });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('pdf worker load failure', () => {
  it('emits preview.load-failed and rejects the open instead of hanging', async () => {
    const { pdfjs, loadFailedEvents } = await freshPdfjs();
    const pending = pdfjs.openPdfFromBytes(new Uint8Array([1, 2, 3]));
    await vi.waitFor(() => expect(FakeWorker.instances.length).toBe(1));
    FakeWorker.instances[0].fireError(new Event('error'));
    // What usePdfDocument renders as `Failed to load PDF: <this>`.
    await expect(pending).rejects.toThrow('pdf worker failed to load');
    const failed = loadFailedEvents();
    expect(failed.length).toBe(1);
    expect(failed[0].scope).toBe('preview');
    expect(failed[0].kind).toBe('error');
    expect(eventOf(failed[0], 'preview.load-failed')).toEqual({
      action: 'preview.load-failed',
      error: 'pdf worker failed to load',
    });
    expect(FakeWorker.instances[0].terminated).toBe(true);
  });

  it('shares one worker between concurrent callers and reports once', async () => {
    const { pdfjs, loadFailedEvents } = await freshPdfjs();
    const handle = pdfjs.getPdfJs();
    const pending = pdfjs.openPdf('asset:///x.pdf');
    await vi.waitFor(() => expect(FakeWorker.instances.length).toBe(1));
    FakeWorker.instances[0].fireError(new Event('error'));
    await expect(handle).resolves.toBeDefined();
    await expect(pending).rejects.toThrow('pdf worker failed to load');
    expect(FakeWorker.instances.length).toBe(1);
    expect(loadFailedEvents().length).toBe(1);
  });

  it('retries with a fresh worker after a failure', async () => {
    const { pdfjs, loadFailedEvents } = await freshPdfjs();
    const first = pdfjs.openPdfFromBytes(new Uint8Array([9]));
    await vi.waitFor(() => expect(FakeWorker.instances.length).toBe(1));
    FakeWorker.instances[0].fireError(new Event('error'));
    await expect(first).rejects.toThrow('pdf worker failed to load');
    mockGetDocument.mockReturnValue({ promise: Promise.resolve({ numPages: 3 }) });
    const doc = await pdfjs.openPdfFromBytes(new Uint8Array([9]));
    expect(doc.numPages).toBe(3);
    expect(FakeWorker.instances.length).toBe(2);
    expect(loadFailedEvents().length).toBe(1);
  });

  it('opens cleanly with no bus event when the worker loads', async () => {
    const { pdfjs, loadFailedEvents } = await freshPdfjs();
    mockGetDocument.mockReturnValue({ promise: Promise.resolve({ numPages: 1 }) });
    const doc = await pdfjs.openPdf('https://x/y.pdf');
    expect(doc.numPages).toBe(1);
    expect(loadFailedEvents().length).toBe(0);
  });
});
