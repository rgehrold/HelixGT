use std::path::PathBuf;

use crate::external::{resolve_minimap2_path, resolve_samtools_path};

#[derive(Debug, Clone, Default)]
pub struct ToolPaths {
    pub minimap2: Option<PathBuf>,
    pub samtools: Option<PathBuf>,
}

impl ToolPaths {
    pub fn resolve_minimap2(&self) -> Option<PathBuf> {
        if let Some(path) = &self.minimap2 {
            return Some(path.clone());
        }
        resolve_minimap2_path(&[])
    }

    pub fn resolve_samtools(&self) -> Option<PathBuf> {
        if let Some(path) = &self.samtools {
            return Some(path.clone());
        }
        resolve_samtools_path(&[])
    }

    pub fn samtools_extra(&self) -> Vec<PathBuf> {
        self.samtools.iter().cloned().collect()
    }

    pub fn minimap2_extra(&self) -> Vec<PathBuf> {
        self.minimap2.iter().cloned().collect()
    }
}

pub fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).max(1))
        .unwrap_or(1)
}

pub fn helixgt_temp_dir() -> PathBuf {
    std::env::temp_dir().join("helixgt")
}

pub fn helixgt_temp_file(stem: &str, extension: &str) -> PathBuf {
    helixgt_temp_dir().join(format!(
        "helixgt_{stem}_{}.{}",
        std::process::id(),
        extension.trim_start_matches('.')
    ))
}

pub fn ensure_temp_dir() -> std::io::Result<PathBuf> {
    let dir = helixgt_temp_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}