use std::fs;
use std::path::{Path, PathBuf};

use crate::filesystem::default_browse_root;
use crate::presets::{sanitize_presets, JobPreset};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convert_output_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_output_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_output_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_reference_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_output_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_presets: Option<Vec<JobPreset>>,
}

pub fn default_preferences() -> UserPreferences {
    UserPreferences {
        browse_root: Some(default_browse_root().display().to_string()),
        expanded_dirs: None,
        files_pane_width: Some(58.0),
        active_mode: Some("convert".into()),
        convert_output_dir: None,
        merge_output_dir: None,
        align_output_dir: None,
        output_format: Some("fasta".into()),
        align_reference_id: Some("escherichia_coli_k12".into()),
        align_preset: Some("general".into()),
        align_output_format: Some("cram".into()),
        recent_files: None,
        job_presets: None,
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

    if let Some(files) = prefs.recent_files.as_mut() {
        files.retain(|path| Path::new(path).is_file());
        files.truncate(24);
        if files.is_empty() {
            prefs.recent_files = None;
        }
    }

    if let Some(presets) = prefs.job_presets.take() {
        prefs.job_presets = Some(sanitize_presets(presets));
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
        browse_root: parsed.browse_root.or(defaults.browse_root),
        expanded_dirs: parsed.expanded_dirs,
        files_pane_width: parsed.files_pane_width.or(defaults.files_pane_width),
        active_mode: parsed.active_mode.or(defaults.active_mode),
        convert_output_dir: parsed.convert_output_dir,
        merge_output_dir: parsed.merge_output_dir,
        align_output_dir: parsed.align_output_dir,
        output_format: parsed.output_format.or(defaults.output_format),
        align_reference_id: parsed.align_reference_id.or(defaults.align_reference_id),
        align_preset: parsed.align_preset.or(defaults.align_preset),
        align_output_format: parsed.align_output_format.or(defaults.align_output_format),
        recent_files: parsed.recent_files,
        job_presets: parsed.job_presets,
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
        active_mode: prefs.active_mode.or(existing.active_mode),
        convert_output_dir: prefs.convert_output_dir.or(existing.convert_output_dir),
        merge_output_dir: prefs.merge_output_dir.or(existing.merge_output_dir),
        align_output_dir: prefs.align_output_dir.or(existing.align_output_dir),
        output_format: prefs.output_format.or(existing.output_format),
        align_reference_id: prefs.align_reference_id.or(existing.align_reference_id),
        align_preset: prefs.align_preset.or(existing.align_preset),
        align_output_format: prefs.align_output_format.or(existing.align_output_format),
        recent_files: prefs.recent_files.or(existing.recent_files),
        job_presets: prefs.job_presets.or(existing.job_presets),
    };

    let json = serde_json::to_string_pretty(&merged).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn record_recent_files(app: &AppHandle, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let mut prefs = load_user_preferences(app);
    let mut recent = prefs.recent_files.unwrap_or_default();
    for path in paths {
        recent.retain(|existing| existing != path);
        recent.insert(0, path.clone());
    }
    recent.truncate(24);
    prefs.recent_files = Some(recent);
    let _ = save_user_preferences(app, prefs);
}