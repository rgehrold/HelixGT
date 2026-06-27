use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{copy, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use converter_core::{
    find_reference, is_gzip_bytes, is_gzip_path, list_references, validate_reference_file,
    EnsemblReference,
};
use flate2::read::MultiGzDecoder;
use reqwest::blocking::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsemblReferenceInfo {
    pub id: String,
    pub label: String,
    pub species: String,
    pub assembly: String,
    pub release: String,
    pub compressed_size_mb: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedReferenceInfo {
    pub id: String,
    pub label: String,
    pub size_bytes: usize,
    pub sequence_bytes: usize,
    pub gzipped: bool,
    pub in_memory_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceFetchResult {
    pub id: String,
    pub label: String,
    pub size_bytes: usize,
    pub sequence_bytes: usize,
    pub gzipped: bool,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceDownloadProgress {
    pub id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<f32>,
}

struct CachedReference {
    label: String,
    cache_path: PathBuf,
    gzipped: bool,
    sequence_bytes: usize,
}

pub struct ReferenceStore {
    client: Client,
    entries: Mutex<HashMap<String, CachedReference>>,
}

impl ReferenceStore {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("HelixGT/0.1")
                .timeout(Duration::from_secs(3600))
                .connect_timeout(Duration::from_secs(60))
                .build()
                .expect("failed to build HTTP client"),
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn list_catalog() -> Vec<EnsemblReferenceInfo> {
        list_references()
            .into_iter()
            .map(reference_to_info)
            .collect()
    }

    pub fn cache_status(&self) -> Vec<CachedReferenceInfo> {
        let entries = self.entries.lock().expect("reference cache lock poisoned");
        entries
            .iter()
            .map(|(id, entry)| CachedReferenceInfo {
                id: id.clone(),
                label: entry.label.clone(),
                size_bytes: entry.cache_path.metadata().map(|m| m.len() as usize).unwrap_or(0),
                sequence_bytes: entry.sequence_bytes,
                gzipped: entry.gzipped,
                in_memory_only: false,
            })
            .collect()
    }

    pub fn fetch_with_progress(
        &self,
        app: &AppHandle,
        id: &str,
    ) -> Result<ReferenceFetchResult, String> {
        {
            let entries = self.entries.lock().expect("reference cache lock poisoned");
            if let Some(entry) = entries.get(id) {
                return Ok(Self::result_from_entry(id, entry, true));
            }
        }

        let reference = find_reference(id).ok_or_else(|| format!("unknown reference '{id}'"))?;
        let cache_path = Self::cache_file_path(id, &reference);
        fs::create_dir_all(
            cache_path
                .parent()
                .ok_or_else(|| "invalid cache path".to_string())?,
        )
        .map_err(|error| format!("failed to create cache directory: {error}"))?;

        self.download_streaming(app, id, &reference.url, &cache_path)?;

        let gzipped = is_gzip_file(&cache_path);
        let sequence_bytes = validate_reference_file(&cache_path, gzipped)
            .map_err(|error| format!("downloaded reference is not valid FASTA: {error:#}"))?;

        let result = ReferenceFetchResult {
            id: id.to_string(),
            label: reference.label.clone(),
            size_bytes: cache_path.metadata().map(|m| m.len() as usize).unwrap_or(0),
            sequence_bytes,
            gzipped,
            cached: false,
        };

        let mut entries = self.entries.lock().expect("reference cache lock poisoned");
        entries.insert(
            id.to_string(),
            CachedReference {
                label: reference.label,
                cache_path,
                gzipped,
                sequence_bytes,
            },
        );

        Ok(result)
    }

    pub fn load_local_file(&self, id: &str, path: &str) -> Result<ReferenceFetchResult, String> {
        let source = PathBuf::from(path);
        let gzipped = is_gzip_path(&source);
        let sequence_bytes = validate_reference_file(&source, gzipped)
            .map_err(|error| format!("reference file is not valid FASTA: {error:#}"))?;
        let label = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Local reference")
            .to_string();

        let result = ReferenceFetchResult {
            id: id.to_string(),
            label: label.clone(),
            size_bytes: source.metadata().map(|m| m.len() as usize).unwrap_or(0),
            sequence_bytes,
            gzipped,
            cached: false,
        };

        let mut entries = self.entries.lock().expect("reference cache lock poisoned");
        entries.insert(
            id.to_string(),
            CachedReference {
                label,
                cache_path: source,
                gzipped,
                sequence_bytes,
            },
        );

        Ok(result)
    }

    pub fn reference_path(&self, id: &str) -> Result<PathBuf, String> {
        let entries = self.entries.lock().expect("reference cache lock poisoned");
        let entry = entries
            .get(id)
            .ok_or_else(|| format!("reference '{id}' is not loaded. Download or choose a file first."))?;
        Ok(entry.cache_path.clone())
    }

    pub fn save_reference(&self, id: &str, output_path: &str, decompress: bool) -> Result<(), String> {
        let entries = self.entries.lock().expect("reference cache lock poisoned");
        let entry = entries
            .get(id)
            .ok_or_else(|| format!("reference '{id}' is not loaded"))?;
        let source = entry.cache_path.clone();
        let gzipped = entry.gzipped;
        drop(entries);

        let output = PathBuf::from(output_path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("cannot create output folder: {error}"))?;
        }

        if decompress {
            if gzipped {
                let input = File::open(&source).map_err(|error| format!("failed to open cached reference: {error}"))?;
                let mut decoder = MultiGzDecoder::new(input);
                let mut output_file =
                    File::create(&output).map_err(|error| format!("failed to create output file: {error}"))?;
                copy(&mut decoder as &mut dyn Read, &mut output_file)
                    .map_err(|error| format!("failed to decompress reference: {error}"))?;
            } else {
                fs::copy(&source, &output)
                    .map_err(|error| format!("failed to copy reference file: {error}"))?;
            }
        } else if gzipped || output_path.to_ascii_lowercase().ends_with(".gz") {
            fs::copy(&source, &output)
                .map_err(|error| format!("failed to save reference file: {error}"))?;
        } else {
            let input = File::open(&source).map_err(|error| format!("failed to open cached reference: {error}"))?;
            let mut reader: Box<dyn Read> = if gzipped {
                Box::new(MultiGzDecoder::new(input))
            } else {
                Box::new(input)
            };
            let mut output_file =
                File::create(&output).map_err(|error| format!("failed to create output file: {error}"))?;
            let mut encoder = flate2::write::GzEncoder::new(&mut output_file, flate2::Compression::default());
            copy(&mut reader, &mut encoder).map_err(|error| format!("failed to compress reference: {error}"))?;
            encoder
                .finish()
                .map_err(|error| format!("failed to finalize compressed reference: {error}"))?;
        }

        Ok(())
    }

    pub fn clear(&self, id: Option<&str>) {
        let mut entries = self.entries.lock().expect("reference cache lock poisoned");
        match id {
            Some(value) => {
                if let Some(entry) = entries.remove(value) {
                    if entry.cache_path.starts_with(Self::cache_root()) {
                        let _ = fs::remove_file(entry.cache_path);
                    }
                }
            }
            None => {
                for (_, entry) in entries.drain() {
                    if entry.cache_path.starts_with(Self::cache_root()) {
                        let _ = fs::remove_file(entry.cache_path);
                    }
                }
            }
        }
    }

    fn result_from_entry(id: &str, entry: &CachedReference, cached: bool) -> ReferenceFetchResult {
        ReferenceFetchResult {
            id: id.to_string(),
            label: entry.label.clone(),
            size_bytes: entry.cache_path.metadata().map(|m| m.len() as usize).unwrap_or(0),
            sequence_bytes: entry.sequence_bytes,
            gzipped: entry.gzipped,
            cached,
        }
    }

    fn cache_root() -> PathBuf {
        std::env::temp_dir().join("ugt_reference_cache")
    }

    fn cache_file_path(id: &str, reference: &EnsemblReference) -> PathBuf {
        let file_name = reference
            .url
            .rsplit('/')
            .next()
            .unwrap_or("reference.fna.gz");
        Self::cache_root().join(format!("{id}_{file_name}"))
    }

    fn download_streaming(
        &self,
        app: &AppHandle,
        id: &str,
        url: &str,
        dest: &Path,
    ) -> Result<(), String> {
        if let Err(error) = self.download_with_reqwest(app, id, url, dest) {
            if let Err(curl_error) = Self::download_with_curl(app, id, url, dest) {
                return Err(format!("HTTP download failed ({error}); curl fallback failed ({curl_error})"));
            }
        }

        let downloaded = dest.metadata().map(|m| m.len()).unwrap_or(0);
        let _ = app.emit(
            "reference-download-progress",
            ReferenceDownloadProgress {
                id: id.to_string(),
                downloaded_bytes: downloaded,
                total_bytes: Some(downloaded),
                percent: Some(100.0),
            },
        );
        Ok(())
    }

    fn download_with_reqwest(
        &self,
        app: &AppHandle,
        id: &str,
        url: &str,
        dest: &Path,
    ) -> Result<(), String> {
        let mut attempts = 0;
        let mut last_error = String::new();
        while attempts < 3 {
            attempts += 1;
            match self.try_reqwest_download(app, id, url, dest) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    last_error = error;
                    let _ = fs::remove_file(dest);
                    std::thread::sleep(Duration::from_millis(1000 * attempts));
                }
            }
        }
        Err(last_error)
    }

    fn try_reqwest_download(
        &self,
        app: &AppHandle,
        id: &str,
        url: &str,
        dest: &Path,
    ) -> Result<(), String> {
        let mut downloaded: u64 = 0;
        if dest.exists() {
            downloaded = dest.metadata().map(|m| m.len()).unwrap_or(0);
        }

        let mut request = self.client.get(url);
        if downloaded > 0 {
            request = request.header("Range", format!("bytes={downloaded}-"));
        }

        let response = request
            .send()
            .map_err(|error| format!("network error: {error}"))?;

        let status = response.status();
        if !status.is_success() && status.as_u16() != 206 {
            return Err(format!("download failed with status {status}"));
        }

        let resume_ok = downloaded > 0 && status.as_u16() == 206;
        if downloaded > 0 && !resume_ok {
            downloaded = 0;
            let _ = fs::remove_file(dest);
        }

        let total_bytes = if resume_ok {
            response
                .content_length()
                .map(|value| value + downloaded)
                .or_else(|| {
                    response
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|value| value.to_str().ok())
                        .and_then(parse_content_range_total)
                })
        } else {
            response.content_length().or_else(|| {
                response
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse().ok())
            })
        };

        let mut file = if resume_ok {
            fs::OpenOptions::new()
                .append(true)
                .open(dest)
                .map_err(|error| format!("failed to open partial download: {error}"))?
        } else {
            File::create(dest).map_err(|error| format!("failed to create download file: {error}"))?
        };

        let mut buffer = [0u8; 1024 * 1024];
        let mut reader = response;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| format!("failed while reading download stream: {error}"))?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .map_err(|error| format!("failed while writing download file: {error}"))?;
            downloaded += read as u64;
            let percent = total_bytes.map(|total| (downloaded as f32 / total as f32) * 100.0);
            let _ = app.emit(
                "reference-download-progress",
                ReferenceDownloadProgress {
                    id: id.to_string(),
                    downloaded_bytes: downloaded,
                    total_bytes,
                    percent,
                },
            );
        }

        file.flush()
            .map_err(|error| format!("failed to flush download file: {error}"))?;

        if let Some(expected) = total_bytes {
            let actual = dest.metadata().map(|m| m.len()).unwrap_or(0);
            if actual != expected {
                return Err(format!(
                    "download incomplete: expected {expected} bytes, got {actual} bytes"
                ));
            }
        }

        Ok(())
    }

    fn download_with_curl(app: &AppHandle, id: &str, url: &str, dest: &Path) -> Result<(), String> {
        let dest_str = dest
            .to_str()
            .ok_or_else(|| "invalid destination path".to_string())?;
        if dest.exists() {
            let _ = fs::remove_file(dest);
        }

        let app_for_progress = app.clone();
        let id_for_progress = id.to_string();
        let dest_for_progress = dest.to_path_buf();
        let stop_progress = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop_progress);
        let progress_handle = std::thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
                let downloaded = dest_for_progress
                    .metadata()
                    .map(|m| m.len())
                    .unwrap_or(0);
                let _ = app_for_progress.emit(
                    "reference-download-progress",
                    ReferenceDownloadProgress {
                        id: id_for_progress.clone(),
                        downloaded_bytes: downloaded,
                        total_bytes: None,
                        percent: None,
                    },
                );
            }
        });

        let output = std::process::Command::new("curl")
            .args([
                "-L",
                "--fail",
                "--retry",
                "5",
                "--retry-delay",
                "2",
                "-C",
                "-",
                "-o",
                dest_str,
                url,
            ])
            .output()
            .map_err(|error| format!("failed to run curl: {error}"))?;

        stop_progress.store(true, Ordering::Relaxed);
        let _ = progress_handle.join();

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(())
    }
}

fn is_gzip_file(path: &Path) -> bool {
    if is_gzip_path(path) {
        return true;
    }
    let mut head = [0u8; 2];
    if let Ok(mut file) = File::open(path) {
        if file.read_exact(&mut head).is_ok() {
            return is_gzip_bytes(&head);
        }
    }
    false
}

fn parse_content_range_total(value: &str) -> Option<u64> {
    value.split('/').nth(1)?.parse().ok()
}

fn reference_to_info(reference: EnsemblReference) -> EnsemblReferenceInfo {
    EnsemblReferenceInfo {
        id: reference.id,
        label: reference.label,
        species: reference.species,
        assembly: reference.assembly,
        release: reference.release,
        compressed_size_mb: reference.compressed_size_mb,
    }
}

pub type SharedReferenceStore = Arc<ReferenceStore>;