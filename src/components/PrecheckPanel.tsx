// PrecheckPanel.tsx — non-modal popout listing a run's pre-compile findings.
//
// Sits above the status bar; the compile keeps running underneath. Each row
// jumps to where the document asks for the dependency. The checkbox turns
// the automatic popup off (the Settings toggle turns it back on). Escape
// closes it unless something else took the key or a dialog is open.

import { useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import ButtonBase from '@mui/material/ButtonBase';
import Checkbox from '@mui/material/Checkbox';
import FormControlLabel from '@mui/material/FormControlLabel';
import IconButton from '@mui/material/IconButton';
import Paper from '@mui/material/Paper';
import Typography from '@mui/material/Typography';
import CloseIcon from '@mui/icons-material/Close';
import type { CheckKind } from '../lib/generated/structure';
import { findingNeedsInstall } from '../lib/interactiveInstall';
import type { PrecheckFindings } from '../hooks/useCompileRunner';

const KIND_LABEL: Record<CheckKind, string> = {
  'not-in-bundle': 'Missing package',
  'external-tool': 'External program',
  'shell-escape': 'Needs shell escape',
  'system-font': 'Missing font',
};

export default function PrecheckPanel({
  precheck,
  popupEnabled,
  onJump,
  onInstallInteractive,
  onClose,
}: {
  precheck: PrecheckFindings;
  /** Whether the panel opens on its own; the checkbox shows only then. */
  popupEnabled: boolean;
  onJump: (rootPath: string, rel: string, line: number) => void;
  /** Put maleficium-interactive.sty into the project (offered on that finding). */
  onInstallInteractive: () => void;
  onClose: (dontShowAgain: boolean) => void;
}) {
  const [dontShow, setDontShow] = useState(false);
  const closeRef = useRef(() => onClose(dontShow));
  closeRef.current = () => onClose(dontShow);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      if (document.querySelector('.MuiDialog-root')) return;
      closeRef.current();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
  const n = precheck.findings.length;
  return (
    <Paper
      role="dialog"
      aria-modal={false}
      aria-labelledby="precheck-title"
      data-testid="precheck-panel"
      elevation={8}
      sx={{
        position: 'fixed',
        right: 16,
        bottom: 40,
        width: 'min(440px, calc(100vw - 32px))',
        maxHeight: '50vh',
        display: 'flex',
        flexDirection: 'column',
        zIndex: (t) => t.zIndex.snackbar,
        border: 1,
        borderColor: 'divider',
      }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', px: 1.5, pt: 1, pb: 0.5 }}>
        <Typography id="precheck-title" variant="subtitle2" sx={{ flex: 1 }}>
          Pre-compile warnings ({n})
        </Typography>
        <IconButton size="small" aria-label="Close" onClick={() => onClose(dontShow)}>
          <CloseIcon fontSize="small" />
        </IconButton>
      </Box>
      <Typography variant="caption" color="text.secondary" sx={{ px: 1.5, pb: 1 }}>
        The compile still runs; these may stop it or change its output.
      </Typography>
      <Box sx={{ overflowY: 'auto', borderTop: 1, borderBottom: 1, borderColor: 'divider' }}>
        {precheck.findings.map((f) => (
          <Box key={`${f.kind}|${f.name}|${f.path}|${f.line}`}>
            <ButtonBase
              onClick={() => onJump(precheck.rootPath, f.path, f.line)}
              title={`Go to ${f.path}:${f.line}`}
              sx={{
                display: 'block',
                width: '100%',
                textAlign: 'left',
                px: 1.5,
                py: 0.75,
                '&:hover': { bgcolor: 'action.hover' },
              }}
            >
              <Typography variant="caption" color="warning.main" component="div">
                {KIND_LABEL[f.kind]}
              </Typography>
              <Typography variant="body2" component="div" noWrap>
                {f.name}
                <Typography component="span" variant="caption" color="text.secondary">
                  {'  '}
                  {f.path}:{f.line}
                </Typography>
              </Typography>
              {f.suggestion ? (
                <Typography variant="caption" color="text.secondary" component="div">
                  Try: {f.suggestion}
                </Typography>
              ) : null}
            </ButtonBase>
            {findingNeedsInstall(f) ? (
              <Box sx={{ px: 1.5, pb: 0.75 }}>
                <Button
                  size="small"
                  variant="outlined"
                  data-testid="install-interactive-fix"
                  onClick={onInstallInteractive}
                >
                  Install maleficium-interactive.sty
                </Button>
              </Box>
            ) : null}
          </Box>
        ))}
      </Box>
      <Box sx={{ display: 'flex', alignItems: 'center', px: 1.5, py: 0.5 }}>
        {popupEnabled ? (
          <FormControlLabel
            sx={{ flex: 1, '& .MuiFormControlLabel-label': { typography: 'caption' } }}
            control={
              <Checkbox
                size="small"
                checked={dontShow}
                onChange={(e) => setDontShow(e.target.checked)}
              />
            }
            label="Don't show this again"
          />
        ) : (
          <Box sx={{ flex: 1 }} />
        )}
        <Button size="small" onClick={() => onClose(dontShow)}>
          Close
        </Button>
      </Box>
    </Paper>
  );
}
