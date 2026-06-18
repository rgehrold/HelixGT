use std::fs;
use std::path::{Path, PathBuf};

use converter_core::is_compatible_file;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

pub fn default_browse_root() -> PathBuf {
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        let documents = home.join("Documents");
        if documents.is_dir() {
            return documents;
        }
        return home;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("C:\\"))
}

pub fn list_directory(path: &Path) -> Result<Vec<DirEntry>, String> {
    if !path.is_dir() {
        return Err(format!("'{}' is not a directory", path.display()));
    }

    let mut entries = Vec::new();
    let read_dir = fs::read_dir(path).map_err(|error| format!("{error:#}"))?;

    for entry in read_dir {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };

        let entry_path = entry.path();
        let metadata = match entry.metadata() {
            Ok(value) => value,
            Err(_) => continue,
        };

        let is_dir = metadata.is_dir();
        if !is_dir && !is_compatible_file(&entry_path) {
            continue;
        }

        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        entries.push(DirEntry {
            name,
            path: entry_path.display().to_string(),
            is_dir,
        });
    }

    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()))
    });

    Ok(entries)
}

pub fn collect_compatible_files(root: &Path) -> Result<Vec<String>, String> {
    if !root.is_dir() {
        return Err(format!("'{}' is not a directory", root.display()));
    }

    let mut files = Vec::new();
    collect_compatible_files_rec(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_compatible_files_rec(dir: &Path, files: &mut Vec<String>) -> Result<(), String> {
    let read_dir = match fs::read_dir(dir) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    for entry in read_dir {
        let entry = match entry {
            Ok(value) => value,
            Err(_) => continue,
        };

        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(value) => value,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            collect_compatible_files_rec(&path, files)?;
        } else if is_compatible_file(&path) {
            files.push(path.display().to_string());
        }
    }
    Ok(())
}

pub fn folder_has_compatible_files(root: &Path) -> Result<bool, String> {
    if !root.is_dir() {
        return Ok(false);
    }
    Ok(has_compatible_files_rec(root)?)
}

fn has_compatible_files_rec(dir: &Path) -> Result<bool, String> {
    let read_dir = fs::read_dir(dir).map_err(|error| format!("{error:#}"))?;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if has_compatible_files_rec(&path)? {
                return Ok(true);
            }
        } else if is_compatible_file(&path) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn rename_path(old_path: &Path, new_name: &str) -> Result<String, String> {
    if !old_path.exists() {
        return Err(format!("'{}' does not exist", old_path.display()));
    }

    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("new name cannot be empty".into());
    }
    if trimmed.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("new name contains invalid characters".into());
    }

    let parent = old_path
        .parent()
        .ok_or_else(|| "cannot rename root path".to_string())?;
    let new_path = parent.join(trimmed);
    if new_path == old_path {
        return Ok(old_path.display().to_string());
    }
    if new_path.exists() {
        return Err(format!("'{}' already exists", new_path.display()));
    }

    fs::rename(old_path, &new_path).map_err(|error| format!("{error:#}"))?;
    Ok(new_path.display().to_string())
}

pub fn folder_child_count(path: &Path) -> Result<u64, String> {
    if !path.is_dir() {
        return Ok(0);
    }

    let read_dir = fs::read_dir(path).map_err(|error| format!("{error:#}"))?;
    let mut count = 0u64;
    for entry in read_dir {
        if entry.is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

pub fn create_folder(parent: &Path, name: Option<&str>) -> Result<String, String> {
    if !parent.is_dir() {
        return Err(format!("'{}' is not a directory", parent.display()));
    }

    let base_name = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("New folder");

    if base_name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("folder name contains invalid characters".into());
    }

    let mut candidate = parent.join(base_name);
    let mut counter = 2u32;
    while candidate.exists() {
        candidate = parent.join(format!("{base_name} ({counter})"));
        counter += 1;
    }

    fs::create_dir(&candidate).map_err(|error| format!("{error:#}"))?;
    Ok(candidate.display().to_string())
}

pub fn delete_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("'{}' does not exist", path.display()));
    }

    if path.is_dir() {
        trash::delete(path).map_err(|error| format!("{error:#}"))?;
    } else {
        trash::delete(path).map_err(|error| format!("{error:#}"))?;
    }
    Ok(())
}