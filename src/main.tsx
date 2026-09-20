import React from 'react';
import ReactDOM from 'react-dom/client';
import { ThemeProvider, CssBaseline } from '@mui/material';
import App from './App';
import { createAppTheme } from './lib/theme';
import { DEFAULT_PREFS } from './lib/appearance';
import './App.css';

export function Root() {
  const [mode, setMode] = React.useState<'dark' | 'light'>(() => {
    try {
      return localStorage.getItem('maleficium.theme') === 'light' ? 'light' : 'dark';
    } catch {
      return 'dark';
    }
  });
  const [density, setDensity] = React.useState<'comfortable' | 'compact'>(() => {
    try {
      return localStorage.getItem('maleficium.density') === 'compact' ? 'compact' : 'comfortable';
    } catch {
      return 'comfortable';
    }
  });
  const theme = React.useMemo(
    () => createAppTheme({ ...DEFAULT_PREFS, mode, density }),
    [mode, density],
  );
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <App
        themeMode={mode}
        onThemeMode={(m) => {
          setMode(m);
          try {
            localStorage.setItem('maleficium.theme', m);
          } catch {
            /* private mode */
          }
        }}
        density={density}
        onDensityMode={(d) => {
          setDensity(d);
          try {
            localStorage.setItem('maleficium.density', d);
          } catch {
            /* private mode */
          }
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
