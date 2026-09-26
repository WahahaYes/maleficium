// FindBar.tsx — MUI in-file find/replace bar for the editor.
//
// Docked below the editor pane (outside its scroll region), so content
// scrolling never hides it. Drives @codemirror/search state and commands
// on the live view; the stock CodeMirror search panel is never opened.

import { useEffect, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Checkbox from '@mui/material/Checkbox';
import FormControlLabel from '@mui/material/FormControlLabel';
import IconButton from '@mui/material/IconButton';
import TextField from '@mui/material/TextField';
import Tooltip from '@mui/material/Tooltip';
import ArrowDropDownIcon from '@mui/icons-material/ArrowDropDown';
import ArrowRightIcon from '@mui/icons-material/ArrowRight';
import CloseIcon from '@mui/icons-material/Close';
import KeyboardArrowDownIcon from '@mui/icons-material/KeyboardArrowDown';
import KeyboardArrowUpIcon from '@mui/icons-material/KeyboardArrowUp';
import type { EditorView } from '@codemirror/view';
import {
  SearchQuery,
  findNext,
  findPrevious,
  replaceAll,
  replaceNext,
  setSearchQuery,
} from '@codemirror/search';

export interface FindBarProps {
  /** Live view accessor; null before mount. */
  viewOf: () => EditorView | null;
  /** Seed query (usually the current selection). */
  seed: string;
  /** Bar type size in px (editor size minus two, floored). */
  fontSizePx: number;
  onClose: () => void;
}

export default function FindBar({ viewOf, seed, fontSizePx, onClose }: FindBarProps) {
  const [query, setQuery] = useState(seed);
  const [replace, setReplace] = useState('');
  const [matchCase, setMatchCase] = useState(false);
  const [regex, setRegex] = useState(false);
  const [word, setWord] = useState(false);
  const [replaceOpen, setReplaceOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Push the query into search state; an invalid regex flags inline.
  useEffect(() => {
    const view = viewOf();
    if (!view) return;
    try {
      view.dispatch({
        effects: setSearchQuery.of(
          new SearchQuery({
            search: query,
            caseSensitive: matchCase,
            regexp: regex,
            wholeWord: word,
            replace,
          }),
        ),
      });
      setError(null);
    } catch {
      setError('invalid regex');
    }
  }, [query, replace, matchCase, regex, word, viewOf]);

  const run = (fn: (view: EditorView) => boolean) => {
    const view = viewOf();
    if (!view) return;
    fn(view);
    view.focus();
  };

  const close = () => {
    viewOf()?.focus();
    onClose();
  };

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        gap: 1,
        p: 1,
        '& .MuiInputBase-input, & .MuiFormControlLabel-label, & .MuiButton-root': {
          fontSize: `${fontSizePx}px`,
        },
      }}
    >
      <Box
        sx={{ display: 'flex', gap: 1, alignItems: 'center', flexWrap: 'wrap' }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') close();
        }}
      >
        <Tooltip title={replaceOpen ? 'Hide replace' : 'Show replace'}>
          <Button
            size="small"
            aria-label="toggle replace"
            onClick={() => setReplaceOpen((v) => !v)}
            endIcon={replaceOpen ? <ArrowDropDownIcon /> : <ArrowRightIcon />}
            sx={{ minWidth: 104 }}
          >
            Replace
          </Button>
        </Tooltip>
        <TextField
          size="small"
          label="Find"
          value={query}
          autoFocus
          error={error != null}
          helperText={error ?? undefined}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') run(findNext);
          }}
          sx={{ minWidth: 160 }}
        />
        <Tooltip title="Previous match (Shift+F3)">
          <IconButton size="small" aria-label="previous match" onClick={() => run(findPrevious)}>
            <KeyboardArrowUpIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <Tooltip title="Next match (F3)">
          <IconButton size="small" aria-label="next match" onClick={() => run(findNext)}>
            <KeyboardArrowDownIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <FormControlLabel
          control={
            <Checkbox
              size="small"
              checked={matchCase}
              onChange={(e) => setMatchCase(e.target.checked)}
            />
          }
          label="Match case"
        />
        <FormControlLabel
          control={
            <Checkbox size="small" checked={regex} onChange={(e) => setRegex(e.target.checked)} />
          }
          label="Regex"
        />
        <FormControlLabel
          control={
            <Checkbox size="small" checked={word} onChange={(e) => setWord(e.target.checked)} />
          }
          label="Whole word"
        />
        <Box sx={{ flex: 1 }} />
        <Tooltip title="Close find bar (Esc)">
          <IconButton size="small" aria-label="close find bar" onClick={close}>
            <CloseIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </Box>
      {replaceOpen ? (
        <Box
          sx={{ display: 'flex', gap: 1, alignItems: 'center', flexWrap: 'wrap' }}
          onKeyDown={(e) => {
            if (e.key === 'Escape') close();
          }}
        >
          <Box sx={{ minWidth: 104 }} />
          <TextField
            size="small"
            label="Replace"
            value={replace}
            onChange={(e) => setReplace(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') run(replaceNext);
            }}
            sx={{ minWidth: 160 }}
          />
          <Tooltip title="Replace current match">
            <Button size="small" onClick={() => run(replaceNext)}>
              Replace
            </Button>
          </Tooltip>
          <Tooltip title="Replace all matches">
            <Button size="small" onClick={() => run(replaceAll)}>
              Replace all
            </Button>
          </Tooltip>
        </Box>
      ) : null}
    </Box>
  );
}
