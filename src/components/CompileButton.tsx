// CompileButton.tsx — the compile/cancel icon button.
//
// Play starts a compile; while one runs the same button shows Stop and
// cancels it. The tooltip names the file that will be compiled.

import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import StopIcon from '@mui/icons-material/Stop';

export default function CompileButton({
  targetLabel,
  compiling,
  onCompile,
  onCancel,
}: {
  /** Human label of the working document (repo-relative basename chain). */
  targetLabel: string;
  compiling: boolean;
  onCompile: () => void;
  onCancel: () => void;
}) {
  return (
    <Tooltip
      title={compiling ? `Cancel ${targetLabel}` : `Compile ${targetLabel}`}
      placement="bottom"
    >
      <IconButton
        size="small"
        color={compiling ? 'error' : 'primary'}
        aria-label={compiling ? `Cancel ${targetLabel}` : `Compile ${targetLabel}`}
        onClick={() => (compiling ? onCancel() : onCompile())}
      >
        {compiling ? <StopIcon fontSize="small" /> : <PlayArrowIcon fontSize="small" />}
      </IconButton>
    </Tooltip>
  );
}
