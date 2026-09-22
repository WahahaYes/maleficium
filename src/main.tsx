import React from 'react';
import ReactDOM from 'react-dom/client';
import { ThemeProvider, CssBaseline } from '@mui/material';
import { alpha } from '@mui/material/styles';
import App from './App';
import { createAppTheme } from './lib/theme';
import { loadAppearance, saveAppearance } from './lib/appearance';
import type { AppearancePrefs } from './lib/appearance';
import { setAppStore } from './lib/app-store';
import { localAppStore } from './lib/app-store.web';
import { setProviders } from './lib/fs-provider';
import { desktopFs, desktopDialog } from './lib/fs-provider.tauri';
import { setStructure } from './lib/structure';
import { desktopStructure } from './lib/structure.tauri';
import './App.css';

setAppStore(localAppStore);
setProviders({ fs: desktopFs, dialog: desktopDialog });
setStructure(desktopStructure);

export function Root() {
  const [prefs, setPrefs] = React.useState<AppearancePrefs>(loadAppearance);
  const theme = React.useMemo(() => createAppTheme(prefs), [prefs]);
  // Flash colors follow the warning token in both modes.
  React.useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty('--syn-flash', alpha(theme.palette.warning.main, 0.55));
    root.style.setProperty('--syn-hit', alpha(theme.palette.warning.main, 0.8));
  }, [theme]);
  const updatePrefs = (p: AppearancePrefs) => {
    setPrefs(p);
    saveAppearance(p);
  };
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <App
        themeMode={
          prefs.accent === 'default'
            ? prefs.mode
            : theme.palette.mode === 'light'
              ? 'light'
              : 'dark'
        }
        onThemeMode={(m) => updatePrefs({ ...prefs, accent: 'default', mode: m })}
        density={prefs.density}
        onDensityMode={(d) => updatePrefs({ ...prefs, density: d })}
        prefs={prefs}
        onPrefs={updatePrefs}
      />
    </ThemeProvider>
  );
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
