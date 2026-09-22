import React from 'react';
import ReactDOM from 'react-dom/client';
import { ThemeProvider, CssBaseline } from '@mui/material';
import { alpha } from '@mui/material/styles';
import App from './App';
import { createAppTheme } from './lib/theme';
import { DEFAULT_PREFS } from './lib/appearance';
import { DEVICE_PREF_KEYS, setAppStore, store } from './lib/app-store';
import { localAppStore } from './lib/app-store.web';
import { setProviders } from './lib/fs-provider';
import { desktopFs, desktopDialog } from './lib/fs-provider.tauri';
import './App.css';

setAppStore(localAppStore);
setProviders({ fs: desktopFs, dialog: desktopDialog });

export function Root() {
  const [mode, setMode] = React.useState<'dark' | 'light'>(() =>
    store().get(DEVICE_PREF_KEYS.theme) === 'light' ? 'light' : 'dark',
  );
  const [density, setDensity] = React.useState<'comfortable' | 'compact'>(() =>
    store().get(DEVICE_PREF_KEYS.density) === 'compact' ? 'compact' : 'comfortable',
  );
  const theme = React.useMemo(
    () => createAppTheme({ ...DEFAULT_PREFS, mode, density }),
    [mode, density],
  );
  // Flash colors follow the warning token in both modes.
  React.useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty('--syn-flash', alpha(theme.palette.warning.main, 0.55));
    root.style.setProperty('--syn-hit', alpha(theme.palette.warning.main, 0.8));
  }, [theme]);
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <App
        themeMode={mode}
        onThemeMode={(m) => {
          setMode(m);
          store().set(DEVICE_PREF_KEYS.theme, m);
        }}
        density={density}
        onDensityMode={(d) => {
          setDensity(d);
          store().set(DEVICE_PREF_KEYS.density, d);
        }}
      />
    </ThemeProvider>
  );
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
