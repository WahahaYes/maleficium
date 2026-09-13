// EditorToolbar.tsx — constant-height toolbar (Compile + file name).
//
// Growth cap: constant ~48px; compile state contract owned by 01
// (CompileStatus internals untouched — placement only).

import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import Typography from '@mui/material/Typography';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import CleaningServicesIcon from '@mui/icons-material/CleaningServices';
import HelpOutlineIcon from '@mui/icons-material/HelpOutlineOutlined';
import CompileStatus from './CompileStatus';

export default function EditorToolbar({ fileName, dirty, onOpen, onSave, onCompile, onClean, onShortcuts }: {
  fileName: string;
  dirty: boolean;
  onOpen: () => void;
  onSave: () => void;
  onCompile: () => void;
  onClean: () => void;
  onShortcuts: () => void;
}) {
  return (
    <Box sx={{ display: 'flex', gap: 1, alignItems: 'center', minHeight: 48, flexWrap: 'wrap' }}>
      <Button variant="outlined" onClick={onOpen}>Open Project</Button>
      <Button variant="outlined" onClick={onSave}>Save</Button>
      <Button variant="contained" startIcon={<PlayArrowIcon fontSize="small" />} onClick={onCompile}>Compile</Button>
      <IconButton size="small" aria-label="Clean build output" title="Clean build output (wipe out/)" onClick={onClean}>
        <CleaningServicesIcon fontSize="small" />
      </IconButton>
      <IconButton size="small" aria-label="Keyboard shortcuts" title="Keyboard shortcuts (?)" onClick={onShortcuts}>
        <HelpOutlineIcon fontSize="small" />
      </IconButton>
      <Typography variant="body2" sx={{ ml: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis' }} noWrap>{fileName}{dirty ? ' ●' : ''}</Typography>
      <CompileStatus />
    </Box>
  );
}
