// EditorToolbar.tsx — constant-height toolbar (Compile + file name).
//
// Growth cap: constant ~48px; compile state contract owned by 01
// (CompileStatus internals untouched — placement only).

import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Typography from '@mui/material/Typography';
import CompileStatus from './CompileStatus';

export default function EditorToolbar({ fileName, dirty, onOpen, onSave, onCompile }: {
  fileName: string;
  dirty: boolean;
  onOpen: () => void;
  onSave: () => void;
  onCompile: () => void;
}) {
  return (
    <Box sx={{ display: 'flex', gap: 1, alignItems: 'center', minHeight: 48, flexWrap: 'wrap' }}>
      <Button variant="outlined" onClick={onOpen}>Open Project</Button>
      <Button variant="outlined" onClick={onSave}>Save</Button>
      <Button variant="contained" onClick={onCompile}>Compile</Button>
      <Typography variant="body2" sx={{ ml: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis' }} noWrap>{fileName}{dirty ? ' ●' : ''}</Typography>
      <CompileStatus />
    </Box>
  );
}
