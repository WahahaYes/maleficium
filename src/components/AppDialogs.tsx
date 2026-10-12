import type { ComponentProps } from 'react';
import type { Dialogs } from '../hooks/useDialogs';
import type { useExport } from '../hooks/useExport';
import type { useRevisionHistory } from '../hooks/useRevisionHistory';
import type { useWidgetApproval } from '../hooks/useWidgetApproval';
import type { PrecheckFindings } from '../hooks/useCompileRunner';
import type { AppearancePrefs } from '../lib/appearance';
import { baseName } from '../lib/paths';
import ExportProgress from './ExportProgress';
import ExportReportDialog from './ExportReportDialog';
import ExternalChangeDialog from './ExternalChangeDialog';
import HistoryDialog from './HistoryDialog';
import OpenRequestPrompt from './OpenRequestPrompt';
import PaletteDialog from './PaletteDialog';
import PrecheckPanel from './PrecheckPanel';
import SettingsDialog from './SettingsDialog';
import ShortcutsDialog from './ShortcutsDialog';
import { AboutDialog, GoToLineDialog, RenameDialog } from './SimpleDialogs';
import TemplateDialogs from './TemplateDialogs';
import WidgetApprovalPrompt from './WidgetApprovalPrompt';
import WidgetsPanel from './WidgetsPanel';

type PaletteProps = ComponentProps<typeof PaletteDialog>;

// Every modal, panel, and prompt layered over the shell. Each group is the
// state App already holds for it, mostly a hook's return value.
export default function AppDialogs({
  dialogs,
  file,
  palette,
  settings,
  history,
  exporter,
  widgets,
  templates,
  conflicts,
  precheck,
  openForAgent,
}: {
  dialogs: Dialogs;
  /** The active file: rename and go-to-line act on it. */
  file: {
    path: string;
    rel: string | null;
    line: number;
    rename: (name: string) => void;
    goToLine: (n: number) => void;
  };
  palette: Pick<PaletteProps, 'findFiles' | 'rank' | 'commands' | 'onOpenFile'>;
  settings: {
    prefs: AppearancePrefs;
    onPrefs: (p: AppearancePrefs) => void;
    precheckPopup: boolean;
    onPrecheckPopup: (on: boolean) => void;
  };
  history: ReturnType<typeof useRevisionHistory>;
  exporter: ReturnType<typeof useExport>;
  widgets: ReturnType<typeof useWidgetApproval>;
  templates: Omit<ComponentProps<typeof TemplateDialogs>, 'mode' | 'onClose'>;
  /** Files changed on disk under unsaved edits, project-relative labels. */
  conflicts: { labels: string[]; resolve: (choice: 'reload' | 'keep') => void };
  precheck: {
    findings: PrecheckFindings | null;
    open: boolean;
    popup: boolean;
    close: (dontShowAgain: boolean) => void;
    onJump: (rootPath: string, rel: string, line: number) => void;
  };
  openForAgent: ComponentProps<typeof OpenRequestPrompt>['onOpen'];
}) {
  const { current, close } = dialogs;
  return (
    <>
      <ShortcutsDialog open={current === 'shortcuts'} onClose={close} />
      <OpenRequestPrompt onOpen={openForAgent} />
      <SettingsDialog
        open={current === 'settings'}
        onClose={close}
        prefs={settings.prefs}
        onChange={settings.onPrefs}
        precheckPopup={settings.precheckPopup}
        onPrecheckPopup={settings.onPrecheckPopup}
      />
      <RenameDialog
        open={current === 'rename'}
        title={baseName(file.path) || file.path}
        initial={baseName(file.path)}
        onClose={close}
        onRename={file.rename}
      />
      <GoToLineDialog
        open={current === 'goToLine'}
        initial={String(file.line)}
        onClose={close}
        onGo={file.goToLine}
      />
      <AboutDialog open={current === 'about'} onClose={close} />
      <PaletteDialog
        open={dialogs.palettePrefix != null}
        initial={dialogs.palettePrefix ?? ''}
        onClose={() => dialogs.palette(null)}
        {...palette}
      />
      <HistoryDialog
        open={history.historyOpen}
        onClose={() => history.setHistoryOpen(false)}
        fileLabel={file.rel}
        availability={history.historyAvail}
        rows={history.historyRows}
        summary={history.historySummary}
        notice={history.historyNotice}
        restoringRev={history.restoringRev}
        onRestore={(rev) => void history.restoreRevision(rev)}
      />
      <ExportProgress run={exporter.run} onCancel={() => void exporter.cancelExport()} />
      <ExportReportDialog report={exporter.bundleReport} onClose={exporter.closeBundleReport} />
      <TemplateDialogs
        mode={dialogs.templateMode}
        onClose={() => dialogs.template(null)}
        {...templates}
      />
      <WidgetsPanel
        open={widgets.panelOpen}
        onClose={widgets.closePanel}
        model={widgets.model}
        state={widgets.state}
        runtimeModel={widgets.runtimeModel}
        runtimeState={widgets.runtimeState}
      />
      <WidgetApprovalPrompt
        prompt={widgets.prompts?.current() ?? null}
        failure={widgets.prompts?.failure() ?? null}
        onApprove={() => void widgets.prompts?.approve()}
        onSkip={() => widgets.prompts?.skip()}
        onDismissFailure={() => widgets.prompts?.clearFailure()}
      />
      {conflicts.labels.length > 0 ? (
        <ExternalChangeDialog
          path={conflicts.labels[0]}
          more={conflicts.labels.length - 1}
          onReload={() => conflicts.resolve('reload')}
          onKeep={() => conflicts.resolve('keep')}
        />
      ) : null}
      {precheck.findings && precheck.open ? (
        <PrecheckPanel
          key={`${precheck.findings.target}:${precheck.findings.findings.length}`}
          precheck={precheck.findings}
          popupEnabled={precheck.popup}
          onJump={precheck.onJump}
          onClose={precheck.close}
        />
      ) : null}
    </>
  );
}
