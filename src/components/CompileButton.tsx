// CompileButton.tsx — minimal icon-first compile affordance.
//
// An icon (Play while idle/success, Stop while compiling) with a hover
// tooltip naming the actual compile target. Cancel rides the same button
// while compiling: visible exactly while compiling.

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
