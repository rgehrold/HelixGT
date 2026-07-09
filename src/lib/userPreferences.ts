import { invoke } from "@tauri-apps/api/core";
import type { JobPreset, ToolMode, UserPreferences } from "$lib/types";

export type { UserPreferences };

const DEFAULT_FILES_PANE_WIDTH = 58;

let cache: UserPreferences = {};
let loadPromise: Promise<UserPreferences> | null = null;
let saveTimer: ReturnType<typeof setTimeout> | null = null;

export function defaultFilesPaneWidth() {
  return DEFAULT_FILES_PANE_WIDTH;
}

export async function loadUserPreferences(): Promise<UserPreferences> {
  if (loadPromise) return loadPromise;

  loadPromise = invoke<UserPreferences>("load_user_preferences")
    .then((prefs) => {
      cache = prefs ?? {};
      return cache;
    })
    .catch((error) => {
      console.warn("Failed to load user preferences:", error);
      return cache;
    });

  return loadPromise;
}

export function patchUserPreferences(patch: Partial<UserPreferences>) {
  cache = { ...cache, ...patch };
  scheduleSave();
}

export function saveUserPreferencesNow() {
  if (saveTimer) {
    clearTimeout(saveTimer);
    saveTimer = null;
  }
  void invoke("save_user_preferences", { prefs: cache }).catch((error) => {
    console.warn("Failed to save user preferences:", error);
  });
}

function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    saveUserPreferencesNow();
  }, 400);
}

export function rememberMode(mode: ToolMode) {
  patchUserPreferences({ activeMode: mode });
}

export function rememberOutputDir(mode: ToolMode, path: string) {
  if (!path.trim()) return;
  if (mode === "convert") patchUserPreferences({ convertOutputDir: path });
  if (mode === "merge") patchUserPreferences({ mergeOutputDir: path });
  if (mode === "align") patchUserPreferences({ alignOutputDir: path });
}

export function outputDirForMode(prefs: UserPreferences, mode: ToolMode) {
  if (mode === "merge") return prefs.mergeOutputDir ?? "";
  if (mode === "align") return prefs.alignOutputDir ?? "";
  return prefs.convertOutputDir ?? "";
}

export function saveJobPreset(preset: JobPreset) {
  const presets = [...(cache.jobPresets ?? [])];
  const index = presets.findIndex((item) => item.id === preset.id);
  if (index >= 0) presets[index] = preset;
  else presets.unshift(preset);
  patchUserPreferences({ jobPresets: presets.slice(0, 32) });
  saveUserPreferencesNow();
}

export function deleteJobPreset(id: string) {
  patchUserPreferences({
    jobPresets: (cache.jobPresets ?? []).filter((preset) => preset.id !== id),
  });
  saveUserPreferencesNow();
}