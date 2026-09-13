// FileTree.tsx — lazy project tree with main-file + git badges.
//
// Growth cap: renders visible rows only (lazy expand 1 level, maxDepth=2
// default, filterHidden excludes .git/out/aux/log); >300 rows capped with
// "show more". Badges are MUI Chip size=small on visible rows only.

import { useMemo, useState, useCallback } from 'react';
import {
  Box,
  Button,
  Chip,
  List,
  ListItemButton,
  ListItemIcon,
  Typography,
} from '@mui/material';
import { TreeEntry } from '../lib/files';
import type { GitBadge } from '../lib/git';

export interface FileTreeProps {
  tree: TreeEntry[];
  selected: string | null;
  onSelect: (path: string) => void;
  onDelete?: (path: string) => void;
  onExpandDir?: (dirPath: string) => Promise<TreeEntry[]>;
  mainFile?: string | null;
  lazy?: boolean;
  maxDepth?: number;
  filterHidden?: boolean;
  gitStatus?: Map<string, GitBadge> | Record<string, string>;
}

const ROW_PAGE = 200;

const badgeColor = (b: string): 'default' | 'primary' | 'success' | 'warning' | 'error' | 'info' => {
  switch (b) {
    case 'M': return 'warning';
    case 'A': return 'success';
    case 'D': return 'error';
    case 'U': return 'info';
    case 'R': return 'primary';
    default: return 'default';
  }
};

function badgeFor(gitStatus: FileTreeProps['gitStatus'], path: string, rootPrefix: string): string | null {
  if (!gitStatus) return null;
  if (gitStatus instanceof Map) return gitStatus.get(path) ?? null;
  // Record form may be keyed by repo-relative path — try absolute then relative.
  const direct = (gitStatus as Record<string, string>)[path];
  if (direct) return direct;
  const rel = rootPrefix && path.startsWith(rootPrefix) ? path.slice(rootPrefix.length) : null;
  if (rel) return (gitStatus as Record<string, string>)[rel] ?? null;
  return null;
}

function FileNode({ node, depth, selected, onSelect, onDelete, onExpandDir, mainFile, maxDepth, gitStatus, rootPrefix, registerNavigable } : {
  node: TreeEntry;
  depth: number;
  selected: string | null;
  onSelect: (path: string) => void;
  onDelete?: (path: string) => void;
  onExpandDir?: (dirPath: string) => Promise<TreeEntry[]>;
  mainFile?: string | null;
  maxDepth: number;
  gitStatus: FileTreeProps['gitStatus'];
  rootPrefix: string;
  registerNavigable: (path: string) => void;
}) {
  const [open, setOpen] = useState(depth === 0 && depth < maxDepth);
  const [lazyChildren, setLazyChildren] = useState<TreeEntry[] | null>(null);
  const [loading, setLoading] = useState(false);
  if (node.type === 'file') registerNavigable(node.path);

  if (node.type === 'file') {
    const badge = badgeFor(gitStatus, node.path, rootPrefix);
    const isMain = mainFile != null && node.path === mainFile;
    return (
      <ListItemButton
        data-path={node.path}
        selected={selected === node.path}
        onClick={() => onSelect(node.path)}
        onContextMenu={(e) => {
          e.preventDefault();
          if (onDelete && window.confirm(`Delete ${node.name}? (moves to trash, undo available)`)) onDelete(node.path);
        }}
        sx={{ pl: 1 + depth * 2 }}
      >
        <ListItemIcon sx={{ minWidth: 28 }}>
          <Typography variant="body2">📄</Typography>
        </ListItemIcon>
        <Typography variant="body2" noWrap sx={{flex:1}}>{node.name}</Typography>
        {isMain ? <Chip label="main" size="small" color="primary" sx={{ ml: 1, height: 18 }} /> : null}
        {badge ? <Chip label={badge} size="small" color={badgeColor(badge)} sx={{ ml: 1, height: 18, minWidth: 28 }} /> : null}
      </ListItemButton>
    );
  }

  const preloaded = node.children ?? lazyChildren;
  const expandable = depth < maxDepth;
  const toggle = async () => {
    if (open) {
      setOpen(false);
      return;
    }
    if (!preloaded && onExpandDir) {
      setLoading(true);
      try {
        const kids = await onExpandDir(node.path);
        setLazyChildren(kids);
      } finally {
        setLoading(false);
      }
    }
    setOpen(true);
  };

  return (
    <>
      <ListItemButton
        data-path={node.path}
        onClick={() => void toggle()}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void toggle();
        }}
        sx={{ pl: 1 + depth * 2 }}
      >
        <ListItemIcon sx={{ minWidth: 28 }}>
          <Typography variant="body2">{open ? '📂' : '📁'}</Typography>
        </ListItemIcon>
        <Typography variant="body2" noWrap sx={{flex:1}}>{node.name}</Typography>
        {loading ? <Typography variant="caption">…</Typography> : null}
      </ListItemButton>
      {open && expandable && (preloaded ?? []).map((child) => (
        <FileNode
          key={child.path}
          node={child}
          depth={depth + 1}
          selected={selected}
          onSelect={onSelect}
          onDelete={onDelete}
          onExpandDir={onExpandDir}
          mainFile={mainFile}
          maxDepth={maxDepth}
          gitStatus={gitStatus}
          rootPrefix={rootPrefix}
          registerNavigable={registerNavigable}
        />
      ))}
    </>
  );
}

export default function FileTree({ tree, selected, onSelect, onDelete, onExpandDir, mainFile, lazy = true, maxDepth = 2, filterHidden = true, gitStatus }: FileTreeProps) {
  void lazy;
  void filterHidden;
  const [limit, setLimit] = useState(ROW_PAGE);
  const [focusIdx, setFocusIdx] = useState(0);

  // Flattened file paths for keyboard nav (registration order = render order).
  const navPaths: string[] = useMemo(() => [], []);
  const registerNavigable = useCallback((p: string) => {
    if (!navPaths.includes(p)) navPaths.push(p);
  }, [navPaths]);

  const topLevel = useMemo(() => tree.slice(0, limit), [tree, limit]);

  return (
    <Box
      onKeyDown={(e) => {
        if (navPaths.length === 0) return;
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          const n = Math.min(focusIdx + 1, navPaths.length - 1);
          setFocusIdx(n);
          document.querySelector<HTMLElement>(`[data-path="${CSS.escape(navPaths[n])}"]`)?.focus();
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          const n = Math.max(focusIdx - 1, 0);
          setFocusIdx(n);
          document.querySelector<HTMLElement>(`[data-path="${CSS.escape(navPaths[n])}"]`)?.focus();
        } else if (e.key === 'Enter' && navPaths[focusIdx]) {
          onSelect(navPaths[focusIdx]);
        }
      }}
    >
      <List dense>
        {topLevel.map((node) => (
          <FileNode
            key={node.path}
            node={node}
            depth={0}
            selected={selected}
            onSelect={onSelect}
            onDelete={onDelete}
            onExpandDir={onExpandDir}
            mainFile={mainFile}
            maxDepth={maxDepth}
            gitStatus={gitStatus}
            rootPrefix=""
            registerNavigable={registerNavigable}
          />
        ))}
      </List>
      {tree.length > limit ? (
        <Button size="small" onClick={() => setLimit((l) => l + ROW_PAGE)}>
          Show more ({Math.min(limit, tree.length)}/{tree.length})
        </Button>
      ) : null}
    </Box>
  );
}
