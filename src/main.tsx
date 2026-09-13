import React from "react";
import ReactDOM from "react-dom/client";
import { ThemeProvider, createTheme, CssBaseline } from "@mui/material";
import App from "./App";
import "./App.css";

const theme = createTheme({
  palette: {
    mode: "dark",
    primary: { main: "#7aa2f7" },
    secondary: { main: "#bb9af7" },
    success: { main: "#9ece6a" },
    warning: { main: "#e0af68" },
    error: { main: "#f7768e" },
    background: { default: "#1a1b26", paper: "#24283b" },
  },
  typography: {
    fontSize: 13,
    fontFamily: "Inter, system-ui, sans-serif",
  },
  components: {
    MuiButton: { defaultProps: { size: "small" } },
    MuiChip: { defaultProps: { size: "small" } },
    MuiToolbar: { defaultProps: { variant: "dense" } },
  },
});

/** Light palette factory ships now so light later is a mode flip, not a rewrite. */
export function createAppTheme(mode: "dark" | "light") {
  return createTheme({
    palette: {
      mode,
      ...(mode === "light"
        ? {
            primary: { main: "#34548a" },
            background: { default: "#fafafa", paper: "#ffffff" },
          }
        : {
            primary: { main: "#7aa2f7" },
            secondary: { main: "#bb9af7" },
            success: { main: "#9ece6a" },
            warning: { main: "#e0af68" },
            error: { main: "#f7768e" },
            background: { default: "#1a1b26", paper: "#24283b" },
          }),
    },
    typography: { fontSize: 13, fontFamily: "Inter, system-ui, sans-serif" },
  });
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <App />
    </ThemeProvider>
  </React.StrictMode>,
);
