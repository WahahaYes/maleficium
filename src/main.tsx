import React from "react";
import ReactDOM from "react-dom/client";
import { ThemeProvider, CssBaseline } from "@mui/material";
import App from "./App";
import { createAppTheme } from "./lib/theme";
import "./App.css";

function Root() {
  const [mode, setMode] = React.useState<'dark' | 'light'>(() => {
    try {
      return localStorage.getItem('maleficium.theme') === 'light' ? 'light' : 'dark';
    } catch {
      return 'dark';
    }
  });
  const theme = React.useMemo(() => createAppTheme(mode), [mode]);
  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <App
        themeMode={mode}
        onThemeMode={(m) => {
          setMode(m);
          try {
            localStorage.setItem('maleficium.theme', m);
          } catch { /* private mode */ }
        }}
      />
    </ThemeProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
