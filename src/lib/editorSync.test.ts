import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { externalSync, hasUserEdit } from './editorSync';

describe('hasUserEdit', () => {
  const state = EditorState.create({ doc: 'one' });

  it('counts a typed change as an edit', () => {
    const tr = state.update({ changes: { from: 3, insert: '!' } });
    expect(hasUserEdit([tr])).toBe(true);
  });

  it('does not count a loaded document replacement', () => {
    const tr = state.update({
      changes: { from: 0, to: 3, insert: 'two' },
      annotations: externalSync.of(true),
    });
    expect(hasUserEdit([tr])).toBe(false);
  });

  it('ignores transactions that change nothing', () => {
    expect(hasUserEdit([state.update({ selection: { anchor: 1 } })])).toBe(false);
  });

  it('counts a batch that holds one real edit', () => {
    const load = state.update({
      changes: { from: 0, to: 3, insert: 'two' },
      annotations: externalSync.of(true),
    });
    const typed = load.state.update({ changes: { from: 3, insert: '!' } });
    expect(hasUserEdit([load, typed])).toBe(true);
  });
});
