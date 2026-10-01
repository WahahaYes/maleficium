import { useState, type ComponentProps } from 'react';
import Box from '@mui/material/Box';
import Chip from '@mui/material/Chip';
import Menu from '@mui/material/Menu';
import MenuItem from '@mui/material/MenuItem';
import Typography from '@mui/material/Typography';
import BinaryPreview from './BinaryPreview';
import BufferTabs from './BufferTabs';
import EditorViewport from './EditorViewport';

// The editor column: file caption + main-file chip, buffer tabs, the editor
// (or a placeholder for a large or binary file), and the compile log line.
export default function EditorPane({
  fileLabel,
  dirty,
  mainLabel,
  mainTip,
  mainFile,
  mainCandidates,
  onPickMain,
  labelOf,
  tabs,
  largeFile,
  previewFile,
  viewport,
  log,
  logTitle,
  fileName,
}: {
  fileName: string;
  fileLabel: string;
  dirty: boolean;
  mainLabel: string;
  mainTip: string;
  mainFile: string | null;
  mainCandidates: string[];
  onPickMain: (path: string) => void;
  labelOf: (path: string) => string;
  tabs: ComponentProps<typeof BufferTabs>;
  largeFile: string | null;
  previewFile: string | null;
  viewport: ComponentProps<typeof EditorViewport>;
  log: string;
  logTitle?: string;
}) {
  // Anchor for the main-file tie-break menu.
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <Box
      sx={{ p: 2, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, mb: 1, minWidth: 0 }}>
        <Typography
          variant="caption"
          noWrap
          sx={{ minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis' }}
          title={fileName}
        >
          {fileLabel}
          {dirty ? ' ●' : ''}
        </Typography>
        <Chip
          size="small"
          label={`main: ${mainLabel}`}
          title={mainTip}
          onClick={mainCandidates.length > 1 ? (e) => setAnchor(e.currentTarget) : undefined}
          sx={{ height: 18, maxWidth: 220 }}
        />
        {mainCandidates.length > 1 ? (
          <Menu
            open={anchor != null}
            anchorEl={anchor}
            onClose={() => setAnchor(null)}
            slotProps={{ list: { 'aria-label': 'Choose main file' } }}
          >
            {mainCandidates.map((c) => (
              <MenuItem
                key={c}
                selected={c === mainFile}
                onClick={() => {
                  setAnchor(null);
                  onPickMain(c);
                }}
              >
                <Typography variant="body2" noWrap>
                  {labelOf(c)}
                </Typography>
              </MenuItem>
            ))}
          </Menu>
        ) : null}
      </Box>
      <BufferTabs {...tabs} />
      {largeFile ? (
        <Typography variant="body2" sx={{ mt: 1 }}>
          Large file — not loaded into the editor ({largeFile}). Open externally to edit.
        </Typography>
      ) : previewFile ? (
        <Box
          sx={{
            flex: 1,
            minHeight: 0,
            display: 'flex',
            flexDirection: 'column',
            overflow: 'hidden',
          }}
        >
          <BinaryPreview key={previewFile} path={previewFile} />
        </Box>
      ) : (
        <Box sx={{ flex: 1, overflow: 'auto' }}>
          <EditorViewport {...viewport} />
        </Box>
      )}
      <Typography variant="caption" title={logTitle} sx={{ display: 'block', mt: 1 }}>
        {log}
      </Typography>
    </Box>
  );
}
