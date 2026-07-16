use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::external::{resolve_minimap2_path, resolve_samtools_path};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

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

/// Unique temp path under the HelixGT temp directory. Creates the directory if needed.
/// Safe for concurrent/parallel conversions (process id + counter + timestamp).
pub fn helixgt_temp_file(stem: &str, extension: &str) -> PathBuf {
    let _ = ensure_temp_dir();
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let safe_stem: String = stem
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' { ch } else { '_' })
        .take(64)
        .collect();
    helixgt_temp_dir().join(format!(
        "helixgt_{safe_stem}_{}_{counter}_{nanos}.{}",
        std::process::id(),
        extension.trim_start_matches('.')
    ))
}

pub fn ensure_temp_dir() -> std::io::Result<PathBuf> {
    let dir = helixgt_temp_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}