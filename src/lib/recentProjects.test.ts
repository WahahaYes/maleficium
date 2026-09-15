import { describe, it, expect, beforeEach, vi } from 'vitest';
import { getRecentProjects, touchRecentProject, pruneRecentProjects, MAX_RECENT_PROJECTS } from './recentProjects';

const store: Record<string, string> = {};
vi.stubGlobal('localStorage', {
  getItem: (k: string) => store[k] ?? null,
  setItem: (k: string, v: string) => { store[k] = v; },
  removeItem: (k: string) => { delete store[k]; },
});

beforeEach(() => {
  for (const k of Object.keys(store)) delete store[k];
});

describe('recent projects store', () => {
  it('starts empty and records opens most-recent-first', () => {
    expect(getRecentProjects()).toEqual([]);
    touchRecentProject('/a/paper');
    touchRecentProject('/b/thesis');
    expect(getRecentProjects()).toEqual(['/b/thesis', '/a/paper']);
  });
  it('re-open moves to front without duplicates', () => {
    touchRecentProject('/a');
    touchRecentProject('/b');
    touchRecentProject('/a');
    expect(getRecentProjects()).toEqual(['/a', '/b']);
  });
  it('caps at MAX and drops junk (relative, blank, non-string)', () => {
    for (let i = 0; i < MAX_RECENT_PROJECTS + 5; i++) touchRecentProject(`/p${i}`);
    const recents = getRecentProjects();
    expect(recents.length).toBe(MAX_RECENT_PROJECTS);
    expect(recents[0]).toBe(`/p${MAX_RECENT_PROJECTS + 4}`);
    touchRecentProject('relative/nope');
    touchRecentProject('   ');
    expect(getRecentProjects()).not.toContain('relative/nope');
  });
  it('prunes stale roots by caller predicate', () => {
    touchRecentProject('/gone');
    touchRecentProject('/here');
    expect(pruneRecentProjects((r) => r === '/here')).toEqual(['/here']);
    expect(getRecentProjects()).toEqual(['/here']);
  });
  it('survives corrupt storage', () => {
    store['maleficium.recentProjects.v1'] = '{nope';
    expect(getRecentProjects()).toEqual([]);
  });
});
