// SearchPanel.tsx — project-wide search in the side column.
//
// A query box with case / whole-word / regex toggles searches the project
// index as you type (debounced). Results are grouped by file, matches
// highlighted; a file read from an unsaved buffer says so. Click a row to
// open the file at the match.
//
// Replace: a second box previews every replacement (old struck, new
// marked) before Replace All applies the plan; the last replace can be
// undone in one step from here or the Edit menu.

import { useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import List from '@mui/material/List';
import ListItemButton from '@mui/material/ListItemButton';
import TextField from '@mui/material/TextField';
import ToggleButton from '@mui/material/ToggleButton';
import Typography from '@mui/material/Typography';
import { alpha } from '@mui/material/styles';
import CloseIcon from '@mui/icons-material/Close';
import RefreshIcon from '@mui/icons-material/Refresh';
import FindReplaceIcon from '@mui/icons-material/FindReplace';
import type {
  Hit,
  Query,
  ReplaceApplied,
  ReplacePreview,
  SearchResult,
} from '../lib/generated/index';
import { hitParts, replaceParts, replaceSummary, searchSummary } from '../lib/search.view';
import { typeScale } from '../lib/theme';

const SEARCH_DEBOUNCE_MS = 250;

export interface SearchPanelProps {
  /** Runs one search; rejects with the reason (e.g. an invalid regex). */
  runSearch: (q: Query) => Promise<SearchResult>;
  onOpenHit: (rel: string, hit: Hit) => void;
  /** Plans a replace; writes nothing. */
  runPreview: (q: Query, replacement: string) => Promise<ReplacePreview>;
  /** Applies a plan by token; rejects when it went stale. */
  runApply: (token: string) => Promise<ReplaceApplied>;
  /** The last replace, while it can still be undone. */
  lastReplace: { replacements: number; files: number } | null;
  onUndoReplace: () => void;
  onClose: () => void;
  /** Bumped to focus the query box (e.g. the command ran again). */
  focusKey: number;
}

function Toggle({
  on,
  label,
  title,
  onChange,
}: {
  on: boolean;
  label: string;
  title: string;
  onChange: (v: boolean) => void;
}) {
  return (
    <ToggleButton
      size="small"
      value={label}
      selected={on}
      onChange={() => onChange(!on)}
      aria-label={title}
      title={title}
      sx={{ px: 0.75, py: 0.25, fontSize: 11, fontFamily: 'monospace', minWidth: 28 }}
    >
      {label}
    </ToggleButton>
  );
}

export default function SearchPanel({
  runSearch,
  onOpenHit,
  runPreview,
  runApply,
  lastReplace,
  onUndoReplace,
  onClose,
  focusKey,
}: SearchPanelProps) {
  const [pattern, setPattern] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [regex, setRegex] = useState(false);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [replaceOpen, setReplaceOpen] = useState(false);
  const [replacement, setReplacement] = useState('');
  const [plan, setPlan] = useState<ReplacePreview | null>(null);
  const [applying, setApplying] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const seq = useRef(0);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [focusKey]);

  // A plan belongs to the exact query and replacement it was made from.
  useEffect(() => {
    setPlan(null);
  }, [pattern, regex, caseSensitive, wholeWord, replacement]);

  const query = (): Query => ({ pattern, regex, caseSensitive, wholeWord });
  const preview = () => {
    runPreview(query(), replacement).then(
      (p) => {
        setPlan(p);
        setError(null);
      },
      (e: unknown) => setError(String(e)),
    );
  };
  const applyPlan = () => {
    if (!plan) return;
    setApplying(true);
    runApply(plan.token)
      .then(
        () => {
          setPlan(null);
          setError(null);
          setRefreshKey((k) => k + 1);
        },
        (e: unknown) => {
          setPlan(null);
          setError(String(e));
        },
      )
      .finally(() => setApplying(false));
  };

  useEffect(() => {
    if (!pattern) {
      setResult(null);
      setError(null);
      return;
    }
    const mine = ++seq.current;
    const t = setTimeout(() => {
      runSearch({ pattern, regex, caseSensitive, wholeWord }).then(
        (r) => {
          if (mine !== seq.current) return;
          setResult(r);
          setError(null);
        },
        (e: unknown) => {
          if (mine !== seq.current) return;
          setResult(null);
          setError(String(e));
        },
      );
    }, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [pattern, regex, caseSensitive, wholeWord, runSearch, refreshKey]);

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', minHeight: 0, flex: 1 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', px: 1, pb: 0.5 }}>
        <Typography variant="caption" color="text.secondary" sx={{ flex: 1 }}>
          Search
        </Typography>
        <IconButton
          size="small"
          aria-label="Replace"
          title="Replace"
          color={replaceOpen ? 'primary' : 'default'}
          onClick={() => setReplaceOpen((v) => !v)}
        >
          <FindReplaceIcon fontSize="small" />
        </IconButton>
        <IconButton
          size="small"
          aria-label="Search again"
          title="Search again"
          onClick={() => setRefreshKey((k) => k + 1)}
        >
          <RefreshIcon fontSize="small" />
        </IconButton>
        <IconButton size="small" aria-label="Close search" title="Close search" onClick={onClose}>
          <CloseIcon fontSize="small" />
        </IconButton>
      </Box>
      <Box sx={{ display: 'flex', gap: 0.5, px: 1, pb: 0.5 }}>
        <TextField
          size="small"
          fullWidth
          placeholder="Search project"
          inputRef={inputRef}
          value={pattern}
          onChange={(e) => setPattern(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') onClose();
          }}
          slotProps={{ htmlInput: { 'aria-label': 'Search project' } }}
          sx={{ '& .MuiInputBase-input': { py: 0.5, fontSize: typeScale.dense } }}
        />
        <Toggle on={caseSensitive} label="Aa" title="Match case" onChange={setCaseSensitive} />
        <Toggle on={wholeWord} label="ab" title="Whole word" onChange={setWholeWord} />
        <Toggle on={regex} label=".*" title="Regular expression" onChange={setRegex} />
      </Box>
      {replaceOpen ? (
        <Box sx={{ display: 'flex', gap: 0.5, px: 1, pb: 0.5 }}>
          <TextField
            size="small"
            fullWidth
            placeholder={regex ? 'Replace with ($1 for groups)' : 'Replace with'}
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && pattern) preview();
              if (e.key === 'Escape') onClose();
            }}
            slotProps={{ htmlInput: { 'aria-label': 'Replace with' } }}
            sx={{ '& .MuiInputBase-input': { py: 0.5, fontSize: typeScale.dense } }}
          />
          <Button size="small" variant="outlined" disabled={!pattern} onClick={preview}>
            Preview
          </Button>
        </Box>
      ) : null}
      {lastReplace ? (
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, px: 1, pb: 0.5 }}>
          <Typography variant="caption" color="text.secondary" sx={{ flex: 1 }}>
            Replaced {lastReplace.replacements} in {lastReplace.files} file
            {lastReplace.files === 1 ? '' : 's'}
          </Typography>
          <Button size="small" onClick={onUndoReplace}>
            Undo
          </Button>
        </Box>
      ) : null}
      {error ? (
        <Typography variant="caption" color="error" sx={{ px: 1 }}>
          {error}
        </Typography>
      ) : result ? (
        <Typography variant="caption" color="text.secondary" sx={{ px: 1 }}>
          {searchSummary(result)}
        </Typography>
      ) : null}
      {plan ? (
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, px: 1, py: 0.5 }}>
          <Typography variant="caption" sx={{ flex: 1 }}>
            {replaceSummary(plan)}
          </Typography>
          <Button size="small" onClick={() => setPlan(null)}>
            Cancel
          </Button>
          <Button
            size="small"
            variant="contained"
            disabled={plan.replacements === 0 || applying}
            onClick={applyPlan}
          >
            Replace All
          </Button>
        </Box>
      ) : null}
      {plan ? (
        <List dense disablePadding sx={{ overflow: 'auto', flex: 1, minHeight: 0 }}>
          {plan.files.map((f) => (
            <Box key={f.rel}>
              <Box sx={{ display: 'flex', gap: 1, px: 1, pt: 0.75 }} title={f.rel}>
                <Typography variant="body2" noWrap sx={{ fontWeight: 600, flex: 1, minWidth: 0 }}>
                  {f.rel}
                </Typography>
                {f.source === 'buffer' ? (
                  <Typography variant="caption" color="warning.main">
                    unsaved
                  </Typography>
                ) : null}
                <Typography variant="caption" color="text.secondary">
                  {f.replacements}
                </Typography>
              </Box>
              {f.hunks.map((h, i) => {
                const r = replaceParts(h);
                return (
                  <Box
                    key={`${h.line}:${h.col}:${i}`}
                    sx={{ display: 'flex', gap: 1, pl: 2, pr: 1 }}
                    title={`${f.rel}:${h.line}`}
                  >
                    <Typography
                      variant="caption"
                      color="text.secondary"
                      sx={{ minWidth: 28, textAlign: 'right', flexShrink: 0 }}
                    >
                      {h.line}
                    </Typography>
                    <Typography
                      variant="body2"
                      noWrap
                      sx={{ fontFamily: 'monospace', fontSize: typeScale.dense, minWidth: 0 }}
                    >
                      {r.pre.trimStart()}
                      <Box
                        component="del"
                        sx={{ bgcolor: (t) => alpha(t.palette.error.main, 0.25) }}
                      >
                        {r.old}
                      </Box>
                      <Box
                        component="ins"
                        sx={{
                          bgcolor: (t) => alpha(t.palette.success.main, 0.25),
                          textDecoration: 'none',
                        }}
                      >
                        {r.new}
                      </Box>
                      {r.post}
                    </Typography>
                  </Box>
                );
              })}
            </Box>
          ))}
        </List>
      ) : null}
      <List
        dense
        disablePadding
        sx={{ overflow: 'auto', flex: 1, minHeight: 0, display: plan ? 'none' : undefined }}
      >
        {result?.files.map((f) => (
          <Box key={f.rel}>
            <Box
              sx={{ display: 'flex', alignItems: 'baseline', gap: 1, px: 1, pt: 0.75 }}
              title={f.rel}
            >
              <Typography variant="body2" noWrap sx={{ fontWeight: 600, minWidth: 0 }}>
                {f.rel.slice(f.rel.lastIndexOf('/') + 1)}
              </Typography>
              <Typography
                variant="caption"
                color="text.secondary"
                noWrap
                sx={{ flex: 1, minWidth: 0 }}
              >
                {f.rel.includes('/') ? f.rel.slice(0, f.rel.lastIndexOf('/')) : ''}
              </Typography>
              {f.source === 'buffer' ? (
                <Typography
                  variant="caption"
                  color="warning.main"
                  title="Searched the unsaved editor text"
                >
                  unsaved
                </Typography>
              ) : null}
              <Typography variant="caption" color="text.secondary">
                {f.hits.length}
              </Typography>
            </Box>
            {f.hits.map((h, i) => {
              const [before, match, after] = hitParts(h);
              return (
                <ListItemButton
                  key={`${h.line}:${h.col}:${i}`}
                  onClick={() => onOpenHit(f.rel, h)}
                  title={`${f.rel}:${h.line}`}
                  sx={{ py: 0, pl: 2, gap: 1 }}
                >
                  <Typography
                    variant="caption"
                    color="text.secondary"
                    sx={{ minWidth: 28, textAlign: 'right', flexShrink: 0 }}
                  >
                    {h.line}
                  </Typography>
                  <Typography
                    variant="body2"
                    noWrap
                    sx={{ fontFamily: 'monospace', fontSize: typeScale.dense, minWidth: 0 }}
                  >
                    {before.trimStart()}
                    <Box
                      component="mark"
                      sx={{
                        bgcolor: (t) => alpha(t.palette.warning.main, 0.35),
                        color: 'inherit',
                        px: 0,
                      }}
                    >
                      {match}
                    </Box>
                    {after}
                  </Typography>
                </ListItemButton>
              );
            })}
          </Box>
        ))}
      </List>
    </Box>
  );
}
