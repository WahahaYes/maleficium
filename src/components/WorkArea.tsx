import type { ReactNode } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Pane, { PaneSplitter } from './Pane';
import type { useShellLayout } from '../hooks/useShellLayout';

// The row under the menu bar: side column, editor, splitter, preview. A
// collapsed preview leaves a "show" stub.
export default function WorkArea({
  shell,
  side,
  editor,
  preview,
}: {
  shell: ReturnType<typeof useShellLayout>;
  side: ReactNode;
  editor: ReactNode;
  preview: ReactNode;
}) {
  const { layout, previewVisible, editorVisible } = shell;
  return (
    <Box sx={{ display: 'flex', flex: 1, minHeight: 0, overflowX: 'auto' }}>
      {shell.treeVisible && side}
      {editorVisible ? (
        <Pane
          label="editor"
          ratio={previewVisible ? layout.editorRatio : 1}
          onRatio={shell.setEditorRatio}
        >
          {editor}
        </Pane>
      ) : null}
      {editorVisible && previewVisible ? (
        <PaneSplitter
          label="Resize editor and preview"
          onDrag={shell.dragSplitter}
          onKeyResize={shell.nudgeSplitter}
        />
      ) : null}
      {!previewVisible ? (
        <Box
          sx={{
            width: 48,
            flexShrink: 0,
            display: 'flex',
            alignItems: 'flex-start',
            justifyContent: 'center',
            pt: 1,
          }}
        >
          <Button size="small" aria-label="Show preview" onClick={shell.showPreview}>
            show
          </Button>
        </Box>
      ) : (
        <Pane
          label="preview"
          ratio={editorVisible ? layout.previewRatio : 1}
          onRatio={shell.setPreviewRatio}
        >
          <Box sx={{ display: 'flex', justifyContent: 'flex-end' }}>
            <Button size="small" aria-label="Hide preview" onClick={shell.collapsePreview}>
              hide
            </Button>
          </Box>
          {preview}
        </Pane>
      )}
    </Box>
  );
}
