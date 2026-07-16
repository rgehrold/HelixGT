use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{copy, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use converter_core::{
    ensure_reference_index, find_reference, is_gzip_bytes, is_gzip_path, list_references,
    validate_reference_file, EnsemblReference,
};
use flate2::read::MultiGzDecoder;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedReferenceManifestEntry {
    id: String,
    label: String,
    cache_path: String,
    gzipped: bool,
    sequence_bytes: usize,
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
    cache_root: PathBuf,
}

impl ReferenceStore {
    pub fn new(cache_root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&cache_root);
        let mut entries = HashMap::new();
        Self::load_manifest(&cache_root, &mut entries);
        Self {
            client: Client::builder()
                .user_agent("HelixGT/0.2 (https://github.com/helixgt; genomics desktop app)")
                .timeout(Duration::from_secs(3600))
                .connect_timeout(Duration::from_secs(60))
                .pool_idle_timeout(Duration::from_secs(90))
                .tcp_keepalive(Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
            entries: Mutex::new(entries),
            cache_root,
        }
    }

    fn manifest_path(cache_root: &Path) -> PathBuf {
        cache_root.join("manifest.json")
    }

    fn load_manifest(cache_root: &Path, entries: &mut HashMap<String, CachedReference>) {
        let path = Self::manifest_path(cache_root);
        let raw = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(_) => return,
        };
        let manifest: Vec<CachedReferenceManifestEntry> =
            match serde_json::from_str(&raw) {
                Ok(value) => value,
                Err(_) => return,
            };
        for item in manifest {
            let cache_path = PathBuf::from(&item.cache_path);
            if !cache_path.is_file() {
                continue;
            }
            entries.insert(
                item.id,
                CachedReference {
                    label: item.label,
                    cache_path,
                    gzipped: item.gzipped,
                    sequence_bytes: item.sequence_bytes,
                },
            );
        }
    }

    /// Persist while the caller already holds `entries` (avoids re-entrant lock deadlock).
    fn persist_manifest_with(
        cache_root: &Path,
        entries: &HashMap<String, CachedReference>,
    ) -> Result<(), String> {
        Self::write_manifest(cache_root, &Self::manifest_entries(entries))
    }

    fn manifest_entries(entries: &HashMap<String, CachedReference>) -> Vec<CachedReferenceManifestEntry> {
        entries
            .iter()
            .map(|(id, entry)| CachedReferenceManifestEntry {
                id: id.clone(),
                label: entry.label.clone(),
                cache_path: entry.cache_path.display().to_string(),
                gzipped: entry.gzipped,
                sequence_bytes: entry.sequence_bytes,
            })
            .collect()
    }

    fn write_manifest(
        cache_root: &Path,
        manifest: &[CachedReferenceManifestEntry],
    ) -> Result<(), String> {
        let json = serde_json::to_string_pretty(manifest)
            .map_err(|error| format!("failed to serialize reference manifest: {error}"))?;
        fs::write(Self::manifest_path(cache_root), json)
            .map_err(|error| format!("failed to write reference manifest: {error}"))?;
        Ok(())
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
        let cache_path = self.cache_file_path(id, &reference);
        if cache_path.is_file() {
            let gzipped = is_gzip_file(&cache_path);
            match validate_reference_file(&cache_path, gzipped) {
                Ok(sequence_bytes) => {
                    let mut entries = self.entries.lock().expect("reference cache lock poisoned");
                    entries.insert(
                        id.to_string(),
                        CachedReference {
                            label: reference.label.clone(),
                            cache_path: cache_path.clone(),
                            gzipped,
                            sequence_bytes,
                        },
                    );
                    let _ = Self::persist_manifest_with(&self.cache_root, &entries);
                    return Ok(ReferenceFetchResult {
                        id: id.to_string(),
                        label: reference.label,
                        size_bytes: cache_path.metadata().map(|m| m.len() as usize).unwrap_or(0),
                        sequence_bytes,
                        gzipped,
                        cached: true,
                    });
                }
                Err(_) => {
                    // Corrupt / incomplete cache file — remove and re-download.
                    let _ = fs::remove_file(&cache_path);
                }
            }
        }
        fs::create_dir_all(
            cache_path
                .parent()
                .ok_or_else(|| "invalid cache path".to_string())?,
        )
        .map_err(|error| format!("failed to create cache directory: {error}"))?;

        let _ = app.emit(
            "reference-download-progress",
            ReferenceDownloadProgress {
                id: id.to_string(),
                downloaded_bytes: 0,
                total_bytes: None,
                percent: Some(0.0),
            },
        );

        self.download_streaming(app, id, &reference.url, &cache_path)?;

        let gzipped = is_gzip_file(&cache_path);
        let sequence_bytes = validate_reference_file(&cache_path, gzipped).map_err(|error| {
            let _ = fs::remove_file(&cache_path);
            format!("downloaded reference is not valid FASTA: {error:#}")
        })?;

        let result = ReferenceFetchResult {
            id: id.to_string(),
            label: reference.label.clone(),
            size_bytes: cache_path.metadata().map(|m| m.len() as usize).unwrap_or(0),
            sequence_bytes,
            gzipped,
            cached: false,
        };

        {
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
            Self::persist_manifest_with(&self.cache_root, &entries)?;
        }

        Ok(result)
    }

    pub fn ensure_index(&self, samtools: &Path, id: &str) -> Result<(), String> {
        let path = self.reference_path(id)?;
        ensure_reference_index(samtools, &path).map_err(|error| format!("{error:#}"))
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

        {
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
            Self::persist_manifest_with(&self.cache_root, &entries)?;
        }

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
                    if entry.cache_path.starts_with(&self.cache_root) {
                        let _ = fs::remove_file(entry.cache_path);
                    }
                }
            }
            None => {
                for (_, entry) in entries.drain() {
                    if entry.cache_path.starts_with(&self.cache_root) {
                        let _ = fs::remove_file(entry.cache_path);
                    }
                }
            }
        }
        let _ = Self::persist_manifest_with(&self.cache_root, &entries);
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

    fn cache_file_path(&self, id: &str, reference: &EnsemblReference) -> PathBuf {
        let file_name = reference
            .url
            .rsplit('/')
            .next()
            .unwrap_or("reference.fna.gz");
        self.cache_root.join(format!("{id}_{file_name}"))
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
        // 416 = range not satisfiable (file already complete or corrupt offset).
        // Restart from scratch instead of failing the whole download.
        if status.as_u16() == 416 {
            let _ = fs::remove_file(dest);
            let response = self
                .client
                .get(url)
                .send()
                .map_err(|error| format!("network error: {error}"))?;
            return self.write_response_body(app, id, response, dest, 0, false);
        }

        if !status.is_success() && status.as_u16() != 206 {
            return Err(format!("download failed with status {status}"));
        }

        let resume_ok = downloaded > 0 && status.as_u16() == 206;
        if downloaded > 0 && !resume_ok {
            downloaded = 0;
            let _ = fs::remove_file(dest);
        }

        self.write_response_body(app, id, response, dest, downloaded, resume_ok)
    }

    fn write_response_body(
        &self,
        app: &AppHandle,
        id: &str,
        response: reqwest::blocking::Response,
        dest: &Path,
        mut downloaded: u64,
        resume_ok: bool,
    ) -> Result<(), String> {

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

        // Prefer curl.exe on Windows so we don't hit PowerShell's Invoke-WebRequest alias.
        let curl_program = if cfg!(windows) { "curl.exe" } else { "curl" };
        let mut command = std::process::Command::new(curl_program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let output = command
            .args([
                "-L",
                "--fail",
                "--retry",
                "5",
                "--retry-delay",
                "2",
                "--connect-timeout",
                "30",
                "-o",
                dest_str,
                url,
            ])
            .output()
            .map_err(|error| format!("failed to run {curl_program}: {error}"))?;

        stop_progress.store(true, Ordering::Relaxed);
        let _ = progress_handle.join();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Err(if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("{curl_program} failed with status {:?}", output.status.code())
            });
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