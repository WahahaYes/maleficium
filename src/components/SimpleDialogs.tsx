import { useEffect, useState } from 'react';
import Button from '@mui/material/Button';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import TextField from '@mui/material/TextField';
import Typography from '@mui/material/Typography';

// The draft resets to `initial` each time the dialog opens.
function useDraft(open: boolean, initial: string) {
  const [draft, setDraft] = useState(initial);
  useEffect(() => {
    if (open) setDraft(initial);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reset on open only
  }, [open]);
  return [draft, setDraft] as const;
}

export function RenameDialog({
  open,
  title,
  initial,
  onClose,
  onRename,
}: {
  open: boolean;
  title: string;
  initial: string;
  onClose: () => void;
  onRename: (name: string) => void;
}) {
  const [draft, setDraft] = useDraft(open, initial);
  const name = draft.trim();
  const submit = () => {
    onClose();
    if (name) onRename(name);
  };
  return (
    <Dialog open={open} onClose={onClose} maxWidth="xs" fullWidth>
      <DialogTitle>Rename {title}</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          fullWidth
          size="small"
          variant="outlined"
          aria-label="New file name"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && name) submit();
          }}
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="contained" disabled={!name} onClick={submit}>
          Rename
        </Button>
      </DialogActions>
    </Dialog>
  );
}

export function GoToLineDialog({
  open,
  initial,
  onClose,
  onGo,
}: {
  open: boolean;
  initial: string;
  onClose: () => void;
  onGo: (line: number) => void;
}) {
  const [draft, setDraft] = useDraft(open, initial);
  const submit = () => {
    const n = parseInt(draft, 10);
    if (Number.isFinite(n)) onGo(n);
    onClose();
  };
  return (
    <Dialog open={open} onClose={onClose} maxWidth="xs">
      <DialogTitle>Go to Line</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          fullWidth
          size="small"
          variant="outlined"
          aria-label="Line number"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submit();
          }}
          slotProps={{ htmlInput: { inputMode: 'numeric' } }}
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="contained" onClick={submit}>
          Go
        </Button>
      </DialogActions>
    </Dialog>
  );
}

export function ConfirmReplaceDialog({
  open,
  file,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  file: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog open={open} onClose={onCancel} maxWidth="xs">
      <DialogTitle>Replace {file}?</DialogTitle>
      <DialogContent>
        <Typography variant="body2">
          The project already has a modified {file}. Replacing it discards those changes.
        </Typography>
      </DialogContent>
      <DialogActions>
        <Button onClick={onCancel}>Cancel</Button>
        <Button variant="contained" color="warning" onClick={onConfirm}>
          Replace
        </Button>
      </DialogActions>
    </Dialog>
  );
}

export function AboutDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  return (
    <Dialog open={open} onClose={onClose} maxWidth="xs">
      <DialogTitle>About Maleficium</DialogTitle>
      <DialogContent>
        <Typography variant="body2">
          Maleficium — desktop-native LaTeX editor (Tauri 2 + React + Tectonic sidecar).
        </Typography>
        <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>
          Version {__APP_VERSION__} · offline-first.
        </Typography>
        <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>
          SyncTeX navigation by Jérôme Laurens (MIT) — bundled sidecar.
        </Typography>
      </DialogContent>
      <DialogActions>
        <Button variant="contained" onClick={onClose}>
          Close
        </Button>
      </DialogActions>
    </Dialog>
  );
}
