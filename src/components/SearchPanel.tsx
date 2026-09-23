// SearchPanel.tsx — project-wide search in the side column.
//
// A query box with case / whole-word / regex toggles searches the project
// index as you type (debounced). Results are grouped by file, matches
// highlighted; a file read from an unsaved buffer says so. Click a row to
// open the file at the match.

import { useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import IconButton from '@mui/material/IconButton';
import List from '@mui/material/List';
import ListItemButton from '@mui/material/ListItemButton';
import TextField from '@mui/material/TextField';
import ToggleButton from '@mui/material/ToggleButton';
import Typography from '@mui/material/Typography';
import { alpha } from '@mui/material/styles';
import CloseIcon from '@mui/icons-material/Close';
import RefreshIcon from '@mui/icons-material/Refresh';
import type { Hit, Query, SearchResult } from '../lib/generated/index';
import { hitParts, searchSummary } from '../lib/search.view';
import { typeScale } from '../lib/theme';

const SEARCH_DEBOUNCE_MS = 250;

export interface SearchPanelProps {
  /** Runs one search; rejects with the reason (e.g. an invalid regex). */
  runSearch: (q: Query) => Promise<SearchResult>;
  onOpenHit: (rel: string, hit: Hit) => void;
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

export default function SearchPanel({ runSearch, onOpenHit, onClose, focusKey }: SearchPanelProps) {
  const [pattern, setPattern] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [regex, setRegex] = useState(false);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const seq = useRef(0);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [focusKey]);

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
      {error ? (
        <Typography variant="caption" color="error" sx={{ px: 1 }}>
          {error}
        </Typography>
      ) : result ? (
        <Typography variant="caption" color="text.secondary" sx={{ px: 1 }}>
          {searchSummary(result)}
        </Typography>
      ) : null}
      <List dense disablePadding sx={{ overflow: 'auto', flex: 1, minHeight: 0 }}>
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
