use std::fs;
use std::path::{Path, PathBuf};

use crate::filesystem::default_browse_root;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const USER_FILE_NAME: &str = "helixgt-user.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browse_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded_dirs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_pane_width: Option<f64>,
}

pub fn default_preferences() -> UserPreferences {
    UserPreferences {
        browse_root: Some(default_browse_root().display().to_string()),
        expanded_dirs: None,
        files_pane_width: Some(58.0),
    }
}

fn user_preferences_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    Ok(dir.join(USER_FILE_NAME))
}

fn is_valid_directory(path: &str) -> bool {
    Path::new(path).is_dir()
}

fn sanitize(mut prefs: UserPreferences) -> UserPreferences {
    if let Some(width) = prefs.files_pane_width {
        prefs.files_pane_width = Some(width.clamp(32.0, 78.0));
    }

    if let Some(root) = prefs.browse_root.as_ref() {
        if !is_valid_directory(root) {
            prefs.browse_root = None;
        }
    }

    if let Some(dirs) = prefs.expanded_dirs.as_mut() {
        dirs.retain(|dir| is_valid_directory(dir));
        if dirs.is_empty() {
            prefs.expanded_dirs = None;
        }
    }

    prefs
}

pub fn load_user_preferences(app: &AppHandle) -> UserPreferences {
    let defaults = default_preferences();
    let path = match user_preferences_path(app) {
        Ok(value) => value,
        Err(_) => return defaults,
    };

    let raw = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(_) => return defaults,
    };

    let parsed: UserPreferences = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return defaults,
    };

    let parsed = sanitize(parsed);

    UserPreferences {
        browse_root: parsed
            .browse_root
            .or(defaults.browse_root),
        expanded_dirs: parsed.expanded_dirs,
        files_pane_width: parsed
            .files_pane_width
            .or(defaults.files_pane_width),
    }
}

pub fn save_user_preferences(app: &AppHandle, prefs: UserPreferences) -> Result<(), String> {
    let path = user_preferences_path(app)?;
    let prefs = sanitize(prefs);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let existing = load_user_preferences(app);
    let merged = UserPreferences {
        browse_root: prefs.browse_root.or(existing.browse_root),
        expanded_dirs: prefs.expanded_dirs.or(existing.expanded_dirs),
        files_pane_width: prefs.files_pane_width.or(existing.files_pane_width),
    };

    let json = serde_json::to_string_pretty(&merged).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())?;
    Ok(())
}