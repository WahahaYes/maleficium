// FileTree.tsx — project tree: 1-level root, expand-on-demand, search, CRUD.
//
// Growth cap: root renders 1 level only (O(depth 1) open); dirs expand via
// onExpandDir (metadata only); visible-rows paging ("show more", 200/page);
// filterHidden excludes .git/out/aux/log. Main-file Chip visible rows only.
// Search filters LOADED rows (substring; full index deferred).

import { useMemo, useState } from 'react';
import {
  Box,
  Button,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  List,
  ListItemButton,
  ListItemIcon,
  Menu,
  MenuItem,
  TextField,
  Typography,
} from '@mui/material';
import AddIcon from '@mui/icons-material/Add';
import SearchIcon from '@mui/icons-material/Search';
import DescriptionIcon from '@mui/icons-material/Description';
import FolderIcon from '@mui/icons-material/Folder';
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import { TreeEntry } from '../lib/files';

export interface FileTreeProps {
  tree: TreeEntry[];
  selected: string | null;
  onSelect: (path: string) => void;
  onDelete?: (path: string) => void;
  onCreate?: (dirPath: string, name: string) => void;
  onRename?: (path: string, newName: string) => void;
  onExpandDir?: (dirPath: string) => Promise<TreeEntry[]>;
  /** Project root dir (enables root-level New when tree is empty/filtered). */
  rootDir?: string | null;
  mainFile?: string | null;
  lazy?: boolean;
  maxDepth?: number;
  filterHidden?: boolean;
}

const ROW_PAGE = 200;

function FileNode({ node, depth, selected, onSelect, onDelete, onCreate, onRename, onExpandDir, mainFile, maxDepth } : {
  node: TreeEntry;
  depth: number;
  selected: string | null;
  onSelect: (path: string) => void;
  onDelete?: (path: string) => void;
  onCreate?: (dirPath: string, name: string) => void;
  onRename?: (path: string, newName: string) => void;
  onExpandDir?: (dirPath: string) => Promise<TreeEntry[]>;
  mainFile?: string | null;
  maxDepth: number;
}) {
  const [open, setOpen] = useState(depth === 0 && depth < maxDepth);
  const [lazyChildren, setLazyChildren] = useState<TreeEntry[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [nameDraft, setNameDraft] = useState<string | null>(null);

  if (node.type === 'file') {
    const isMain = mainFile != null && node.path === mainFile;
    return (
      <>
      <ListItemButton
        data-path={node.path}
        selected={selected === node.path}
        onClick={() => onSelect(node.path)}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
        sx={{ pl: 1 + depth * 2 }}
      >
        <ListItemIcon sx={{ minWidth: 28 }}>
          <DescriptionIcon fontSize="small" color="action" />
        </ListItemIcon>
        <Typography variant="body2" noWrap sx={{flex:1}}>{node.name}</Typography>
        {isMain ? <Chip label="main" size="small" color="primary" sx={{ ml: 1, height: 18 }} /> : null}
      </ListItemButton>
      <Menu open={menu != null} onClose={() => setMenu(null)} anchorReference="anchorPosition" anchorPosition={menu ? { top: menu.y, left: menu.x } : undefined}>
        {onRename ? <MenuItem onClick={() => { setMenu(null); setNameDraft(node.name); }}>Rename</MenuItem> : null}
        {onDelete ? <MenuItem onClick={() => { setMenu(null); setConfirmDelete(true); }}>Delete</MenuItem> : null}
      </Menu>
      <Dialog open={nameDraft != null} onClose={() => setNameDraft(null)} maxWidth="xs" fullWidth>
        <DialogTitle>Rename {node.name}</DialogTitle>
        <DialogContent>
          <TextField autoFocus fullWidth size="small" value={nameDraft ?? ''} onChange={(e) => setNameDraft(e.target.value)} />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setNameDraft(null)}>Cancel</Button>
          <Button variant="contained" onClick={() => { if (nameDraft?.trim()) onRename?.(node.path, nameDraft.trim()); setNameDraft(null); }}>Rename</Button>
        </DialogActions>
      </Dialog>
      <Dialog open={confirmDelete} onClose={() => setConfirmDelete(false)} maxWidth="xs">
        <DialogTitle>Delete {node.name}?</DialogTitle>
        <DialogContent><Typography variant="body2">Moves to trash — Undo restores it.</Typography></DialogContent>
        <DialogActions>
          <Button onClick={() => setConfirmDelete(false)}>Cancel</Button>
          <Button variant="contained" color="error" onClick={() => { setConfirmDelete(false); onDelete?.(node.path); }}>Delete</Button>
        </DialogActions>
      </Dialog>
      </>);
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
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
        sx={{ pl: 1 + depth * 2 }}
      >
        <ListItemIcon sx={{ minWidth: 28 }}>
          {open ? <FolderOpenIcon fontSize="small" color="action" /> : <FolderIcon fontSize="small" color="action" />}
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
          onCreate={onCreate}
          onRename={onRename}
          onExpandDir={onExpandDir}
          mainFile={mainFile}
          maxDepth={maxDepth}
        />
      ))}
      <Menu open={menu != null && node.type === 'dir'} onClose={() => setMenu(null)} anchorReference="anchorPosition" anchorPosition={menu ? { top: menu.y, left: menu.x } : undefined}>
        {onCreate ? <MenuItem onClick={() => { setMenu(null); setNameDraft(''); }}>New file here</MenuItem> : null}
      </Menu>
      <Dialog open={nameDraft != null && node.type === 'dir'} onClose={() => setNameDraft(null)} maxWidth="xs" fullWidth>
        <DialogTitle>New file in {node.name}</DialogTitle>
        <DialogContent>
          <TextField autoFocus fullWidth size="small" placeholder="name.tex" value={nameDraft ?? ''} onChange={(e) => setNameDraft(e.target.value)} />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setNameDraft(null)}>Cancel</Button>
          <Button variant="contained" onClick={() => { if (nameDraft?.trim()) onCreate?.(node.path, nameDraft.trim()); setNameDraft(null); }}>Create</Button>
        </DialogActions>
      </Dialog>
    </>
  );
}

export default function FileTree({ tree, selected, onSelect, onDelete, onCreate, onRename, onExpandDir, rootDir, mainFile, lazy = true, maxDepth = 2, filterHidden = true }: FileTreeProps) {
  // NOTE (P-06): `lazy`/`filterHidden` are honored by the DATA layer: App opens
  // with a 1-level root (`listDir1Level`) and expands via `onExpandDir`
  // (filtering in `files.ts`).
  void lazy;
  void filterHidden;
  const [limit, setLimit] = useState(ROW_PAGE);
  const [focusIdx, setFocusIdx] = useState(0);
  const [query, setQuery] = useState('');
  const [rootDraft, setRootDraft] = useState<string | null>(null);
  const rootCreate = onCreate && rootDir ? rootDir : null;

  const topLevel = useMemo(() => tree.slice(0, limit), [tree, limit]);

  // Substring search over LOADED rows (cheap; full-project index deferred).
  const searched = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return topLevel;
    const match = (n: TreeEntry): TreeEntry | null => {
      if (n.name.toLowerCase().includes(q)) return n;
      if (n.type === 'dir' && n.children) {
        const kids = n.children.map(match).filter((k): k is TreeEntry => k != null);
        if (kids.length > 0) return { ...n, children: kids };
      }
      return null;
    };
    return topLevel.map(match).filter((n): n is TreeEntry => n != null);
  }, [topLevel, query]);

  // Flat file list for keyboard nav - pure derivation, no render side effects.
  // Walks rendered rows (incl. lazily expanded); collapsed subtrees excluded.
  const navPaths = useMemo(() => {
    const out: string[] = [];
    const walk = (nodes: TreeEntry[], depth: number) => {
      for (const n of nodes) {
        if (n.type === 'file') out.push(n.path);
        else if (depth < maxDepth && n.children) walk(n.children, depth + 1);
      }
    };
    walk(searched, 0);
    return out;
  }, [searched, maxDepth]);
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
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, px: 1, pb: 0.5 }}>
        <SearchIcon fontSize="small" color="action" />
        <TextField
          size="small"
          fullWidth
          placeholder="Filter files"
          aria-label="Filter files"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {rootCreate ? (
          <Button size="small" aria-label="New file in project root" onClick={() => setRootDraft('')} sx={{ minWidth: 0, px: 1 }}>
            <AddIcon fontSize="small" />
          </Button>
        ) : null}
      </Box>
      <List dense>
        {searched.map((node) => (
          <FileNode
            key={node.path}
            node={node}
            depth={0}
            selected={selected}
            onSelect={onSelect}
            onDelete={onDelete}
            onCreate={onCreate}
            onRename={onRename}
            onExpandDir={onExpandDir}
            mainFile={mainFile}
            maxDepth={maxDepth}
          />
        ))}
      </List>
      <Dialog open={rootDraft != null && !!rootCreate} onClose={() => setRootDraft(null)} maxWidth="xs" fullWidth>
        <DialogTitle>New file in project</DialogTitle>
        <DialogContent>
          <TextField autoFocus fullWidth size="small" placeholder="name.tex" value={rootDraft ?? ''} onChange={(e) => setRootDraft(e.target.value)} />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setRootDraft(null)}>Cancel</Button>
          <Button variant="contained" onClick={() => { if (rootDraft?.trim() && rootCreate) onCreate?.(rootCreate, rootDraft.trim()); setRootDraft(null); }}>Create</Button>
        </DialogActions>
      </Dialog>
      {tree.length > limit ? (
        <Button size="small" onClick={() => setLimit((l) => l + ROW_PAGE)}>
          Show more ({Math.min(limit, tree.length)}/{tree.length})
        </Button>
      ) : null}
    </Box>
  );
}
