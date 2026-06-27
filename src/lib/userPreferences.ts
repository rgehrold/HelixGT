import { invoke } from "@tauri-apps/api/core";

export type UserPreferences = {
  browseRoot?: string;
  expandedDirs?: string[];
  filesPaneWidth?: number;
};

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