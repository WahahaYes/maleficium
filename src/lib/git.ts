import { invoke } from '@tauri-apps/api/core';

export type GitBadge = 'M' | 'A' | 'D' | 'U' | 'R';

/** Never-throw git state: non-repos degrade honestly instead of throwing. */
export type GitState =
  | { ok: true; badges: Record<string, GitBadge>; branch: string | null; raw: string }
  | {
      ok: false;
      reason: 'not-a-repo' | 'no-git-binary';
      badges: Record<string, GitBadge>;
      branch: null;
      raw: string;
    };

/**
 * Parse `git status --porcelain=v1 -b` output into per-file badges.
 * `??` → A (untracked shown as added), `UU`/`AA`/`DD` → U, `R` → R.
 * Branch comes from the `## branch...upstream` header line.
 */
export function parseGitPorcelain(text: string): {
  badges: Record<string, GitBadge>;
  branch: string | null;
} {
  const badges: Record<string, GitBadge> = {};
  let branch: string | null = null;
  for (const line of text.split('\n')) {
    if (line.startsWith('## ')) {
      const m = line.slice(3).match(/^([^\s.]+)/);
      if (m) branch = m[1] === 'HEAD' ? null : m[1];
      continue;
    }
    if (line.length < 4) continue;
    const xy = line.slice(0, 2);
    let path = line.slice(3).trim();
    // Rename entries: `R  old -> new` — badge the new path.
    const arrow = path.indexOf(' -> ');
    if (arrow >= 0) path = path.slice(arrow + 4);
    // Quoted paths from core.quotePath; strip quotes.
    if (path.startsWith('"') && path.endsWith('"')) path = path.slice(1, -1);
    if (!path) continue;
    if (xy === '??') badges[path] = 'A';
    else if (xy.includes('U') || xy === 'AA' || xy === 'DD') badges[path] = 'U';
    else if (xy.includes('R')) badges[path] = 'R';
    else if (xy.includes('M')) badges[path] = 'M';
    else if (xy.includes('A')) badges[path] = 'A';
    else if (xy.includes('D')) badges[path] = 'D';
  }
  return { badges, branch };
}

export function emptyGitState(reason: 'not-a-repo' | 'no-git-binary'): GitState {
  return { ok: false, reason, badges: {}, branch: null, raw: '' };
}

export async function gitStatusState(root: string): Promise<GitState> {
  let text: string;
  try {
    text = await invoke<string>('git_status', { root });
  } catch (e) {
    const msg = String(e);
    if (msg.includes('not a git repository')) return emptyGitState('not-a-repo');
    return emptyGitState(msg.includes('failed') ? 'no-git-binary' : 'not-a-repo');
  }
  const { badges, branch } = parseGitPorcelain(text);
  return { ok: true, badges, branch, raw: text };
}

export async function gitStatus(root: string): Promise<{ ok: boolean; text: string }> {
  try {
    const text = await invoke<string>('git_status', { root });
    return { ok: true, text };
  } catch {
    return { ok: false, text: '' };
  }
}

export async function gitShowHead(
  root: string,
  file: string,
): Promise<{ ok: boolean; text: string }> {
  try {
    const text = await invoke<string>('git_show_head', { root, file });
    return { ok: true, text };
  } catch {
    return { ok: false, text: '' };
  }
}
