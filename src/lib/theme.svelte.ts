import { browser } from "$app/environment";

export type ThemePalette = "blue" | "orange";
export type ThemeMode = "dark" | "light";

const STORAGE_KEY = "helixgt-theme";

function loadSaved(): { palette: ThemePalette; mode: ThemeMode } {
  if (!browser) {
    return { palette: "blue", mode: "dark" };
  }

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return { palette: "blue", mode: "dark" };
    }

    const parsed = JSON.parse(raw) as { palette?: string; mode?: string };
    return {
      palette: parsed.palette === "orange" ? "orange" : "blue",
      mode: parsed.mode === "light" ? "light" : "dark",
    };
  } catch {
    return { palette: "blue", mode: "dark" };
  }
}

const saved = loadSaved();

export const theme = $state({
  palette: saved.palette,
  mode: saved.mode,
});

export function applyTheme() {
  if (!browser) return;

  document.documentElement.dataset.palette = theme.palette;
  document.documentElement.dataset.mode = theme.mode;
  localStorage.setItem(
    STORAGE_KEY,
    JSON.stringify({ palette: theme.palette, mode: theme.mode }),
  );
}

export function setPalette(next: ThemePalette) {
  theme.palette = next;
  applyTheme();
}

export function setMode(next: ThemeMode) {
  theme.mode = next;
  applyTheme();
}

export function togglePalette() {
  setPalette(theme.palette === "blue" ? "orange" : "blue");
}

export function toggleMode() {
  setMode(theme.mode === "dark" ? "light" : "dark");
}

export function wordmarkSrc() {
  return `/branding/logo-${theme.palette}-${theme.mode}.svg`;
}