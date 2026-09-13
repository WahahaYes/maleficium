// CompileButton.tsx — minimal icon-first compile affordance.
//
// First pass at the minimal-button interface: an icon (Play while idle/success,
// Stop while compiling) with a hover tooltip `Compile <working document>`
// naming the ACTUAL compile target (mainFile ?? active file). Cancel rides the
// same button while compiling (one control, phase-driven) — the 01 rule that
// Cancel is visible exactly while compiling is preserved, and the registry
// (`tools.compile` / `tools.cancel`) stays the source of truth: this button
// calls the same bound actions menus and chords call.

import IconButton from '@mui/material/IconButton';
import Tooltip from '@mui/material/Tooltip';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import StopIcon from '@mui/icons-material/Stop';

export default function CompileButton({ targetLabel, compiling, onCompile, onCancel }: {
  /** Human label of the working document (repo-relative basename chain). */
  targetLabel: string;
  compiling: boolean;
  onCompile: () => void;
  onCancel: () => void;
}) {
  return (
    <Tooltip title={compiling ? `Cancel ${targetLabel}` : `Compile ${targetLabel}`} placement="bottom">
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
