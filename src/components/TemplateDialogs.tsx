// TemplateDialogs.tsx — New Project from Template (the gallery), Save Project
// as Template, and Import Folder as Template.
//
// Every outcome is reported on the bus; a new project opens through the
// caller's open path with its main file associated.

import { useEffect, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import ButtonBase from '@mui/material/ButtonBase';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import TextField from '@mui/material/TextField';
import Typography from '@mui/material/Typography';
import { emit } from '../lib/events';
import { dialog } from '../lib/fs-provider';
import { setMainFileFor } from '../lib/mainFile.store';
import { hashRoot } from '../lib/paths';
import type { SessionRoot } from '../lib/preview-bus';
import {
  groupTemplates,
  importFolderAsTemplate,
  instantiateTemplate,
  listTemplates,
  saveProjectAsTemplate,
  templateId,
  USER_CATEGORY,
  type TemplateInfo,
} from '../lib/templates';

export type TemplateDialogMode = 'gallery' | 'save' | 'import' | null;

export default function TemplateDialogs({
  mode,
  onClose,
  project,
  mainRel,
  openRoot,
}: {
  mode: TemplateDialogMode;
  onClose: () => void;
  /** The open project, for Save Project as Template. */
  project: SessionRoot | null;
  /** The open project's main file, root-relative. */
  mainRel: string | null;
  openRoot: (root: string) => Promise<void>;
}) {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [picked, setPicked] = useState<TemplateInfo | null>(null);
  const [folder, setFolder] = useState('');
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [main, setMain] = useState('main.tex');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (mode == null) return;
    setBusy(false);
    setDescription('');
    if (mode === 'gallery') {
      setPicked(null);
      setFolder('');
      listTemplates().then(
        (l) => {
          setTemplates(l.templates);
          if (l.unreadable.length > 0) {
            emit({
              scope: 'app',
              kind: 'warn',
              actor: 'system',
              message: 'some of your templates could not be read: ' + l.unreadable.join('; '),
              event: { action: 'template.list-failed', error: l.unreadable.join('; ') },
            });
          }
        },
        (e: unknown) =>
          emit({
            scope: 'app',
            kind: 'error',
            actor: 'system',
            message: 'templates unavailable: ' + String(e).slice(0, 200),
            event: { action: 'template.list-failed', error: String(e).slice(0, 200) },
          }),
      );
    } else {
      setName('');
      setMain(mode === 'save' ? (mainRel ?? 'main.tex') : 'main.tex');
    }
  }, [mode, mainRel]);

  async function create() {
    if (!picked) return;
    const parent = await dialog().openDirectory({ title: 'Create the project in…' });
    if (!parent) return;
    setBusy(true);
    try {
      const c = await instantiateTemplate(picked.id, parent, folder.trim() || picked.id);
      setMainFileFor(hashRoot(c.root), c.main);
      emit({
        scope: 'app',
        kind: 'success',
        actor: 'user',
        message: `new project from ${picked.name}: ${c.root}`,
        event: { action: 'template.create', template: picked.id, root: c.root },
      });
      onClose();
      await openRoot(c.root);
    } catch (e) {
      emit({
        scope: 'app',
        kind: 'error',
        actor: 'user',
        message: `could not create from ${picked.name}: ` + String(e).slice(0, 200),
        event: {
          action: 'template.create-failed',
          template: picked.id,
          error: String(e).slice(0, 200),
        },
      });
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    const info: TemplateInfo = {
      id: templateId(name),
      name: name.trim(),
      description: description.trim() || 'A template of your own.',
      category: USER_CATEGORY,
      main: main.trim(),
      user: true,
    };
    let source: Promise<TemplateInfo>;
    if (mode === 'save') {
      if (!project) return;
      source = saveProjectAsTemplate(project.rootId, info);
    } else {
      const dir = await dialog().openDirectory({ title: 'Import this folder as a template' });
      if (!dir) return;
      source = importFolderAsTemplate(dir, info);
    }
    setBusy(true);
    try {
      const t = await source;
      emit({
        scope: 'app',
        kind: 'success',
        actor: 'user',
        message: `saved template ${t.name}`,
        event: { action: 'template.save', id: t.id, name: t.name },
      });
      onClose();
    } catch (e) {
      emit({
        scope: 'app',
        kind: 'error',
        actor: 'user',
        message: `could not save template ${info.name}: ` + String(e).slice(0, 200),
        event: { action: 'template.save-failed', name: info.name, error: String(e).slice(0, 200) },
      });
    } finally {
      setBusy(false);
    }
  }

  if (mode === 'gallery') {
    return (
      <Dialog open onClose={onClose} maxWidth="md" fullWidth aria-label="New project from template">
        <DialogTitle>New Project from Template</DialogTitle>
        <DialogContent dividers>
          {groupTemplates(templates).map((g) => (
            <Box key={g.category} sx={{ mb: 2 }}>
              <Typography variant="overline" color="text.secondary">
                {g.category}
              </Typography>
              <Box
                sx={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))',
                  gap: 1,
                }}
              >
                {g.items.map((t) => (
                  <ButtonBase
                    key={t.id}
                    data-template={t.id}
                    onClick={() => {
                      setPicked(t);
                      setFolder(t.id);
                    }}
                    onDoubleClick={() => void create()}
                    sx={{
                      display: 'block',
                      textAlign: 'left',
                      p: 1.5,
                      border: 1,
                      borderRadius: 1,
                      borderColor: picked?.id === t.id ? 'primary.main' : 'divider',
                      bgcolor: picked?.id === t.id ? 'action.selected' : 'transparent',
                      '&:hover': { bgcolor: 'action.hover' },
                    }}
                  >
                    <Typography variant="subtitle2">{t.name}</Typography>
                    <Typography variant="caption" color="text.secondary">
                      {t.description}
                    </Typography>
                  </ButtonBase>
                ))}
              </Box>
            </Box>
          ))}
        </DialogContent>
        <DialogActions sx={{ gap: 1 }}>
          <TextField
            size="small"
            label="Folder name"
            value={folder}
            onChange={(e) => setFolder(e.target.value)}
            disabled={!picked}
            sx={{ mr: 'auto', minWidth: 240 }}
          />
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="contained" disabled={!picked || busy} onClick={() => void create()}>
            Choose Location and Create…
          </Button>
        </DialogActions>
      </Dialog>
    );
  }

  if (mode === 'save' || mode === 'import') {
    const valid = templateId(name) !== '' && main.trim() !== '';
    return (
      <Dialog open onClose={onClose} maxWidth="xs" fullWidth>
        <DialogTitle>
          {mode === 'save' ? 'Save Project as Template' : 'Import Folder as Template'}
        </DialogTitle>
        <DialogContent sx={{ display: 'flex', flexDirection: 'column', gap: 2, pt: 1 }}>
          <TextField
            autoFocus
            margin="dense"
            label="Template name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            helperText={name ? `id: ${templateId(name) || '(needs a letter or digit)'}` : ' '}
          />
          <TextField
            label="Description"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            multiline
            minRows={2}
          />
          <TextField label="Main file" value={main} onChange={(e) => setMain(e.target.value)} />
        </DialogContent>
        <DialogActions>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="contained" disabled={!valid || busy} onClick={() => void save()}>
            {mode === 'save' ? 'Save' : 'Choose Folder and Import…'}
          </Button>
        </DialogActions>
      </Dialog>
    );
  }

  return null;
}
