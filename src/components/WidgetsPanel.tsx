// WidgetsPanel.tsx — View > Widgets: the project's html widgets, whether the
// user has approved the code they run, a read-only diff against the last
// approved version, and the project's auto-approval switch.
//
// Presentational: the model decides what each action does. Approve passes
// the digest that was reviewed and is refused if the folder changed since.

import Alert from '@mui/material/Alert';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Chip from '@mui/material/Chip';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import FormControlLabel from '@mui/material/FormControlLabel';
import Switch from '@mui/material/Switch';
import Typography from '@mui/material/Typography';
import type { WidgetReview } from '../lib/generated/api';
import type { RuntimeModel, RuntimeState, WidgetsModel, WidgetsState } from '../lib/widgetApproval';
import {
  AUTO_APPROVE_WARNING,
  buildRows,
  buildRuntimeRows,
  reviewFiles,
  RUNTIME_SECTION_LABEL,
  runtimeUnavailable,
  shortDigest,
  WIDGETS_EMPTY,
  WIDGETS_NEEDS_MAIN,
  WIDGETS_PANEL_TITLE,
  type DiffLine,
  type RowState,
  type RuntimeRow,
  type RuntimeSection,
  type RuntimeUnavailable,
  type WidgetRow,
} from '../lib/widgets.view';

const CHIP_COLOR: Record<RowState, 'success' | 'warning' | 'default' | 'error'> = {
  approved: 'success',
  changed: 'warning',
  pending: 'default',
  revoked: 'error',
};

const LINE_BG: Record<DiffLine['kind'], string> = {
  same: 'transparent',
  add: 'rgba(46,160,67,0.22)',
  del: 'rgba(248,81,73,0.22)',
  gap: 'transparent',
};
const LINE_MARK: Record<DiffLine['kind'], string> = { same: ' ', add: '+', del: '-', gap: '' };

function Diff({ testId, review }: { testId: string; review: WidgetReview | undefined }) {
  if (!review) return null;
  const files = reviewFiles(review.files);
  if (files.length === 0) {
    return (
      <Typography variant="caption" data-testid={testId}>
        No file differs from the last approved version.
      </Typography>
    );
  }
  return (
    <Box data-testid={testId} sx={{ mt: 1 }}>
      {files.map((f) => (
        <Box key={f.path} sx={{ mb: 1 }}>
          <Typography variant="caption" sx={{ fontWeight: 600 }}>
            {f.path} ({f.change})
          </Typography>
          {f.note ? (
            <Typography variant="caption" component="div">
              {f.note}
            </Typography>
          ) : (
            <Box
              component="pre"
              sx={{
                m: 0,
                fontFamily: 'monospace',
                fontSize: 12,
                overflowX: 'auto',
                border: 1,
                borderColor: 'divider',
              }}
            >
              {f.lines.map((l, i) => (
                <Box
                  key={i}
                  data-diff={l.kind}
                  sx={{
                    bgcolor: LINE_BG[l.kind],
                    px: 1,
                    color: l.kind === 'gap' ? 'text.secondary' : undefined,
                  }}
                >
                  {l.kind === 'gap' ? `… ${l.text}` : `${LINE_MARK[l.kind]} ${l.text}`}
                </Box>
              ))}
            </Box>
          )}
        </Box>
      ))}
    </Box>
  );
}

function Row({ row, model, state }: { row: WidgetRow; model: WidgetsModel; state: WidgetsState }) {
  const busy = state.busy.includes(row.id);
  // A review is open exactly while its source is loaded; approving, revoking
  // or a change on disk drops it.
  const open = state.reviews[row.id] !== undefined;
  return (
    <Box
      data-testid={`widget-row-${row.id}`}
      data-state={row.state}
      sx={{ py: 1.5, borderBottom: 1, borderColor: 'divider' }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap' }}>
        <Typography sx={{ fontWeight: 600 }}>{row.id}</Typography>
        <Chip size="small" label={row.type} variant="outlined" />
        <Chip
          size="small"
          color={CHIP_COLOR[row.state]}
          label={row.viaAuto ? `${row.stateLabel} (auto)` : row.stateLabel}
          data-testid={`widget-state-${row.id}`}
        />
        <Box sx={{ flex: 1 }} />
        {row.hasApproved || row.canApprove ? (
          <Button
            size="small"
            disabled={busy}
            data-testid={`widget-review-${row.id}`}
            onClick={() => {
              if (open) model.hideReview(row.id);
              else void model.loadReview(row.id);
            }}
          >
            {open ? 'Hide source' : row.hasApproved ? 'Review changes' : 'Review source'}
          </Button>
        ) : null}
        <Button
          size="small"
          variant="contained"
          disabled={busy || !row.canApprove}
          data-testid={`widget-approve-${row.id}`}
          onClick={() => void model.approve(row.id)}
        >
          Approve
        </Button>
        <Button
          size="small"
          color="error"
          disabled={busy || !row.canRevoke}
          data-testid={`widget-revoke-${row.id}`}
          onClick={() => void model.revoke(row.id)}
        >
          Revoke
        </Button>
      </Box>
      <Typography variant="caption" color="text.secondary" component="div">
        {row.path} · digest <span title={row.digest}>{shortDigest(row.digest)}</span>
      </Typography>
      {row.detail ? (
        <Typography variant="caption" component="div">
          {row.detail}
        </Typography>
      ) : null}
      <Typography variant="caption" component="div" data-testid={`widget-origins-${row.id}`}>
        Declared origins: {row.origins.length ? row.origins.join('; ') : 'none'}
        {row.approvedOrigins
          ? ` (approved for: ${row.approvedOrigins.length ? row.approvedOrigins.join('; ') : 'none'})`
          : ''}
      </Typography>
      {open ? <Diff testId={`widget-diff-${row.id}`} review={state.reviews[row.id]} /> : null}
    </Box>
  );
}

const RUNTIME_CHIP_COLOR: Record<RuntimeSection, 'success' | 'warning' | 'default' | 'error'> = {
  needs: 'default',
  allowed: 'success',
  'allowed-auto': 'success',
  changed: 'warning',
  denied: 'error',
};

function RuntimeFacts({ row }: { row: RuntimeRow }) {
  return (
    <>
      <Typography variant="caption" color="text.secondary" component="div">
        {row.path} · version {row.version || 'unknown'} · digest{' '}
        <span title={row.digest}>{shortDigest(row.digest)}</span> · licence{' '}
        {row.license || 'unknown'} · WebGL {row.webgl ? 'yes' : 'no'}
      </Typography>
      <Typography variant="caption" component="div">
        Used by: {row.widgets.join(', ')}
      </Typography>
      {row.vendored.length ? (
        <Typography variant="caption" component="div">
          Vendored: {row.vendored.map((v) => `${v.name} ${v.version} (${v.license})`).join('; ')}
        </Typography>
      ) : null}
      {row.warnings.length ? (
        <Typography variant="caption" component="div">
          Validator warnings: {row.warnings.join('; ')}
        </Typography>
      ) : null}
      {row.detail ? (
        <Typography variant="caption" component="div">
          {row.detail}
        </Typography>
      ) : null}
    </>
  );
}

function RuntimeRowView({
  row,
  model,
  state,
}: {
  row: RuntimeRow;
  model: RuntimeModel;
  state: RuntimeState;
}) {
  const busy = state.busy.includes(row.ref);
  // A review is open exactly while its source is loaded; allowing, denying
  // or a change on disk drops it.
  const open = state.reviews[row.ref] !== undefined;
  return (
    <Box
      data-testid={`runtime-row-${row.ref}`}
      data-state={row.section}
      sx={{ py: 1.5, borderBottom: 1, borderColor: 'divider' }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap' }}>
        <Typography sx={{ fontWeight: 600 }}>{row.title}</Typography>
        <Chip size="small" label={row.ref} variant="outlined" />
        <Chip
          size="small"
          color={RUNTIME_CHIP_COLOR[row.section]}
          label={row.viaAuto ? `${row.sectionLabel} (auto)` : row.sectionLabel}
          data-testid={`runtime-state-${row.ref}`}
        />
        <Box sx={{ flex: 1 }} />
        {row.section !== 'allowed-auto' ? (
          <Button
            size="small"
            disabled={busy}
            data-testid={`runtime-review-${row.ref}`}
            onClick={() => {
              if (open) model.hideReview(row.ref);
              else void model.loadReview(row.ref);
            }}
          >
            {open ? 'Hide source' : row.hasApproved ? 'Review diff' : 'Review source'}
          </Button>
        ) : null}
        <Button
          size="small"
          variant="contained"
          disabled={busy || !row.canAllow}
          data-testid={`runtime-allow-${row.ref}`}
          onClick={() => void model.allow(row.ref)}
        >
          Allow
        </Button>
        <Button
          size="small"
          color="error"
          disabled={busy || !row.canDeny}
          data-testid={`runtime-deny-${row.ref}`}
          onClick={() => void model.deny(row.ref)}
        >
          Deny
        </Button>
      </Box>
      <RuntimeFacts row={row} />
      {open ? <Diff testId={`runtime-diff-${row.ref}`} review={state.reviews[row.ref]} /> : null}
    </Box>
  );
}

function UnavailableRuntime({ row }: { row: RuntimeUnavailable }) {
  return (
    <Alert
      key={row.ref}
      severity="warning"
      sx={{ mt: 1 }}
      data-testid={`runtime-unavailable-${row.ref}`}
    >
      {row.ref} ({row.widget}): {row.notInstalled ? 'Not installed' : `Invalid: ${row.error}`}
    </Alert>
  );
}

export default function WidgetsPanel({
  open,
  onClose,
  model,
  state,
  runtimeModel,
  runtimeState,
}: {
  open: boolean;
  onClose: () => void;
  /** Null without a project and a main file. */
  model: WidgetsModel | null;
  state: WidgetsState | null;
  runtimeModel: RuntimeModel | null;
  runtimeState: RuntimeState | null;
}) {
  const rows = state?.status ? buildRows(state.status) : [];
  const status = state?.status ?? null;
  const runtimeRows = runtimeState?.status ? buildRuntimeRows(runtimeState.status) : [];
  const runtimeMissing = runtimeState?.status ? runtimeUnavailable(runtimeState.status) : [];
  const sections: RuntimeSection[] = ['needs', 'allowed', 'allowed-auto', 'changed', 'denied'];
  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth data-testid="widgets-panel">
      <DialogTitle>{WIDGETS_PANEL_TITLE}</DialogTitle>
      <DialogContent dividers>
        {!model || !state ? (
          <Typography>{WIDGETS_NEEDS_MAIN}</Typography>
        ) : (
          <>
            <FormControlLabel
              control={
                <Switch
                  checked={status?.autoApprove ?? false}
                  disabled={!status}
                  onChange={(e) => void model.setAutoApprove(e.target.checked)}
                  slotProps={{
                    input: { 'aria-label': 'Auto-approve widget changes in this project' },
                  }}
                  data-testid="widgets-auto-approve"
                />
              }
              label="Auto-approve widget changes in this project"
            />
            <Alert
              severity={status?.autoApprove ? 'warning' : 'info'}
              sx={{ mb: 1 }}
              data-testid="widgets-auto-warning"
            >
              {AUTO_APPROVE_WARNING}
            </Alert>
            {state.notice ? (
              <Alert severity="error" sx={{ mb: 1 }} data-testid="widgets-notice">
                {state.notice}
              </Alert>
            ) : null}
            {status && rows.length === 0 && status.unavailable.length === 0 ? (
              <Typography data-testid="widgets-empty">{WIDGETS_EMPTY}</Typography>
            ) : null}
            {rows.map((r) => (
              <Row key={r.id} row={r} model={model} state={state} />
            ))}
            {status?.unavailable
              .filter((u) => !u.path.startsWith('runtimes/'))
              .map((u) => (
                <Alert key={u.widget} severity="warning" sx={{ mt: 1 }}>
                  {u.widget} ({u.path}): {u.error}
                </Alert>
              ))}
            {status && status.exempt.length > 0 ? (
              <Typography variant="caption" color="text.secondary" component="div" sx={{ mt: 1 }}>
                No approval needed (built-in): {status.exempt.join(', ')}
              </Typography>
            ) : null}
            {runtimeModel && runtimeState ? (
              <>
                <Typography variant="h6" sx={{ mt: 2 }}>
                  Custom runtimes
                </Typography>
                {runtimeState.notice ? (
                  <Alert severity="error" sx={{ mb: 1 }} data-testid="runtimes-notice">
                    {runtimeState.notice}
                  </Alert>
                ) : null}
                {sections.map((s) => {
                  const inSection = runtimeRows.filter((r) => r.section === s);
                  if (inSection.length === 0) return null;
                  return (
                    <Box key={s} data-testid={`runtime-section-${s}`} sx={{ mt: 1 }}>
                      <Typography variant="subtitle2">
                        {RUNTIME_SECTION_LABEL[s]} ({inSection.length})
                      </Typography>
                      {inSection.map((r) => (
                        <RuntimeRowView
                          key={r.ref}
                          row={r}
                          model={runtimeModel}
                          state={runtimeState}
                        />
                      ))}
                    </Box>
                  );
                })}
                {runtimeMissing.map((r) => (
                  <UnavailableRuntime key={r.ref} row={r} />
                ))}
              </>
            ) : null}
          </>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Close</Button>
      </DialogActions>
    </Dialog>
  );
}
