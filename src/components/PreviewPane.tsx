// The preview column: PDF and Article tabs sharing one pane.
//
// The PDF tab is the SyncTeX bitmap view. The Article tab renders the last
// compile's reader bundle in a sandboxed frame (no bridge inside). Sync is
// forward-only: the caret's section posts its anchor into the frame and the
// bytes scroll there; the article never reports back. The bytes' Approve
// button opens the Widgets panel; an approval settled there re-exports and
// re-renders the article. For now the Article tab loads on demand,
// reloads by hand, and its reload stays off while a compile runs. The
// article has no pages, so only zoom carries over.

import type { ComponentProps } from 'react';
import { useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Tab from '@mui/material/Tab';
import Tabs from '@mui/material/Tabs';
import Typography from '@mui/material/Typography';
import ZoomInIcon from '@mui/icons-material/ZoomIn';
import ZoomOutIcon from '@mui/icons-material/ZoomOut';
import RefreshIcon from '@mui/icons-material/Refresh';
import Preview from './Preview';
import ArticleView from './ArticleView';
import {
  articleZoomLabel,
  loadArticleMode,
  resolveArticleMode,
  saveArticleMode,
  stepArticleZoom,
  type ArticleMode,
} from '../lib/article';
import { useTheme } from '@mui/material/styles';
import ToggleButton from '@mui/material/ToggleButton';
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup';
import BrightnessAutoIcon from '@mui/icons-material/BrightnessAuto';
import LightModeIcon from '@mui/icons-material/LightMode';
import DarkModeIcon from '@mui/icons-material/DarkMode';
import { emit } from '../lib/events';

type PreviewTab = 'pdf' | 'article';

export interface PreviewPaneProps extends Omit<ComponentProps<typeof Preview>, 'onZoom'> {
  /** True while a compile runs (article reload stays off). */
  compiling?: boolean;
  articleHtml: string | null;
  articleLoading: boolean;
  articleError: string | null;
  /** False with no compiled output to read. */
  canLoadArticle: boolean;
  onLoadArticle: () => void;
  /** The caret section's anchor, scrolled to inside the article frame. */
  articleAnchor: string | null;
  /** Fires when the caret section is posted into the article frame. */
  onArticleSync: (anchorId: string) => void;
  /** Fires when the bytes' Approve button asks for a widget. */
  onApproveArticle: (widgetId: string) => void;
  /** Pending-approval sentence naming the held items, or null when none. */
  approvalText: string | null;
  /** Opens View > Widgets to review the held items. */
  onReviewApprovals: () => void;
}

export default function PreviewPane({
  compiling = false,
  articleHtml,
  articleLoading,
  articleError,
  canLoadArticle,
  onLoadArticle,
  articleAnchor,
  onArticleSync,
  onApproveArticle,
  approvalText,
  onReviewApprovals,
  ...pdf
}: PreviewPaneProps) {
  const [tab, setTab] = useState<PreviewTab>('pdf');
  const [articleZoom, setArticleZoom] = useState(100);
  const [articleMode, setArticleMode] = useState<ArticleMode>(loadArticleMode);
  const appDark = useTheme().palette.mode === 'dark';

  const select = (next: PreviewTab) => {
    setTab(next);
    // First visit loads the article; later visits keep what is shown.
    if (
      next === 'article' &&
      canLoadArticle &&
      !compiling &&
      !articleHtml &&
      !articleLoading &&
      !articleError
    ) {
      onLoadArticle();
    }
  };

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        minHeight: 0,
        overflow: 'hidden',
      }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
        <Tabs
          aria-label="Preview type"
          value={tab}
          onChange={(_, v: PreviewTab) => select(v)}
          sx={{ minHeight: 36 }}
        >
          <Tab label="PDF" value="pdf" sx={{ minHeight: 36, py: 0.5 }} />
          <Tab label="Article" value="article" sx={{ minHeight: 36, py: 0.5 }} />
        </Tabs>
        {tab === 'article' ? (
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, ml: 'auto' }}>
            {compiling ? (
              <Typography variant="caption" color="text.secondary">
                Compiling…
              </Typography>
            ) : null}
            <Button
              size="small"
              aria-label="Reload article"
              title={compiling ? 'Unavailable while compiling' : 'Reload article'}
              startIcon={<RefreshIcon fontSize="small" />}
              onClick={onLoadArticle}
              disabled={compiling || articleLoading || !canLoadArticle}
            >
              Reload
            </Button>
            <ToggleButtonGroup
              size="small"
              exclusive
              aria-label="Article colour mode"
              value={articleMode}
              onChange={(_, v: ArticleMode | null) => {
                if (!v) return;
                setArticleMode(v);
                saveArticleMode(v);
              }}
            >
              <ToggleButton value="system" aria-label="Follow the app" title="Follow the app">
                <BrightnessAutoIcon fontSize="small" />
              </ToggleButton>
              <ToggleButton value="light" aria-label="Light" title="Light">
                <LightModeIcon fontSize="small" />
              </ToggleButton>
              <ToggleButton value="dark" aria-label="Dark" title="Dark">
                <DarkModeIcon fontSize="small" />
              </ToggleButton>
            </ToggleButtonGroup>
            <Button
              size="small"
              aria-label="Zoom article out"
              onClick={() => setArticleZoom((z) => stepArticleZoom(z, 'out'))}
            >
              <ZoomOutIcon fontSize="small" />
            </Button>
            <Typography variant="caption" aria-label="Article zoom">
              {articleZoomLabel(articleZoom)}
            </Typography>
            <Button
              size="small"
              aria-label="Zoom article in"
              onClick={() => setArticleZoom((z) => stepArticleZoom(z, 'in'))}
            >
              <ZoomInIcon fontSize="small" />
            </Button>
          </Box>
        ) : null}
      </Box>
      {tab === 'pdf' ? (
        <Preview
          {...pdf}
          onZoom={(mode, percent) =>
            emit({
              scope: 'preview',
              kind: 'info',
              actor: 'user',
              message: `preview zoom ${mode.kind === 'percent' ? '' : mode.kind + ' '}${Math.round(percent)}%`,
              event: { action: 'preview.zoom', mode: mode.kind, percent: Math.round(percent) },
            })
          }
        />
      ) : (
        <ArticleView
          html={articleHtml}
          loading={articleLoading}
          error={articleError}
          canLoad={canLoadArticle}
          compiling={compiling}
          zoomPercent={articleZoom}
          onLoad={onLoadArticle}
          anchorId={articleAnchor}
          onSync={onArticleSync}
          onApproveRequest={onApproveArticle}
          approvalText={approvalText}
          onReviewApprovals={onReviewApprovals}
          mode={resolveArticleMode(articleMode, appDark)}
        />
      )}
    </Box>
  );
}
