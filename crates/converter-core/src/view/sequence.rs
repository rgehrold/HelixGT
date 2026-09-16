use std::collections::HashMap;
use std::ffi::OsString;
#[cfg(test)]
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Context, Result};
use noodles::core::{Position, Region};
use serde::Serialize;

use crate::convert::open_buf_reader;
use crate::format::{infer_format, is_gzipped, FileFormat};

use super::contig::{contig_name_aliases, find_named};

/// Maximum bases returned in one window (keeps IPC payloads bounded).
pub const MAX_SEQUENCE_WINDOW: u64 = 2_000_000;

/// Refuse to fully scan unindexed files larger than this when opening (bytes on disk).
const MAX_OPEN_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// If total bases across contigs are at or below this, keep the whole file in memory after open.
/// Raised for desktop machines with ample RAM (smooth multi-contig / large-ref sessions).
const FULL_CACHE_TOTAL_BASES: u64 = 512 * 1024 * 1024;

/// If a single contig/read is at or below this, cache its full sequence on first access.
const FULL_CACHE_CONTIG_BASES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContigInfo {
    pub name: String,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceDocument {
    pub path: String,
    pub format: String,
    pub gzipped: bool,
    pub contigs: Vec<ContigInfo>,
    pub total_bases: u64,
    /// True when the viewer has the full sequences in memory for smooth panning.
    pub fully_cached: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceSlice {
    pub contig: String,
    /// 0-based inclusive start of returned bases.
    pub start: u64,
    /// 0-based exclusive end of returned bases.
    pub end: u64,
    pub sequence: String,
    pub contig_length: u64,
    pub reverse_complemented: bool,
}

struct CachedDocument {
    format: FileFormat,
    contigs: Vec<ContigInfo>,
    /// Contig name → full sequence (only those loaded so far).
    sequences: HashMap<String, String>,
    fully_cached: bool,
    /// When set, random-access windows use this FASTA index (`.fai`).
    fai_path: Option<PathBuf>,
}

fn document_cache() -> &'static Mutex<HashMap<String, CachedDocument>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedDocument>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_key(path: &Path) -> String {
    // Prefer canonical path so relative/absolute open the same cache entry.
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Sidecar path used by samtools / noodles: `ref.fa` → `ref.fa.fai`.
fn fai_sidecar_path(fasta: &Path) -> PathBuf {
    let mut s = OsString::from(fasta.as_os_str());
    s.push(".fai");
    PathBuf::from(s)
}

fn find_contig<'a>(contigs: &'a [ContigInfo], requested: &str) -> Option<&'a ContigInfo> {
    find_named(contigs, requested, |c| c.name.as_str())
}

fn fai_index_cache() -> &'static Mutex<HashMap<String, std::sync::Arc<noodles::fasta::fai::Index>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, std::sync::Arc<noodles::fasta::fai::Index>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load_fai_cached(fai_path: &Path) -> Result<std::sync::Arc<noodles::fasta::fai::Index>> {
    let key = cache_key(fai_path);
    if let Ok(cache) = fai_index_cache().lock() {
        if let Some(idx) = cache.get(&key) {
            return Ok(idx.clone());
        }
    }
    let index = noodles::fasta::fai::read(fai_path)
        .with_context(|| format!("failed to read FASTA index '{}'", fai_path.display()))?;
    let arc = std::sync::Arc::new(index);
    if let Ok(mut cache) = fai_index_cache().lock() {
        if cache.len() >= 16 && !cache.contains_key(&key) {
            cache.clear();
        }
        cache.insert(key, arc.clone());
    }
    Ok(arc)
}

fn contigs_from_fai(index: &noodles::fasta::fai::Index) -> Vec<ContigInfo> {
    index
        .as_ref()
        .iter()
        .map(|record| ContigInfo {
            name: String::from_utf8_lossy(record.name()).into_owned(),
            length: record.length(),
        })
        .collect()
}

fn read_fai_contigs(fai_path: &Path) -> Result<Vec<ContigInfo>> {
    let index = load_fai_cached(fai_path)?;
    let contigs = contigs_from_fai(index.as_ref());
    if contigs.is_empty() {
        bail!("FASTA index '{}' contains no sequences", fai_path.display());
    }
    Ok(contigs)
}

/// Build a `.fai` next to an uncompressed FASTA (one sequential pass).
/// Not used on the hot open path (too slow for human genomes); tests only.
#[cfg(test)]
fn build_fai_sidecar(path: &Path) -> Result<PathBuf> {
    if is_gzipped(path) {
        bail!(
            "cannot build a .fai for gzipped FASTA '{}'. \
             Decompress it, or use bgzip and provide indexes, or open a smaller reference.",
            path.display()
        );
    }
    let fai_path = fai_sidecar_path(path);
    let index = noodles::fasta::io::index(path)
        .with_context(|| format!("failed to index FASTA '{}'", path.display()))?;
    let file = std::fs::File::create(&fai_path)
        .with_context(|| format!("failed to create FASTA index '{}'", fai_path.display()))?;
    let mut writer = noodles::fasta::fai::io::Writer::new(BufWriter::new(file));
    writer
        .write_index(&index)
        .with_context(|| format!("failed to write FASTA index '{}'", fai_path.display()))?;
    writer
        .into_inner()
        .flush()
        .with_context(|| format!("failed to flush FASTA index '{}'", fai_path.display()))?;
    Ok(fai_path)
}



fn open_via_fai(path: &Path, fai_path: &Path) -> Result<SequenceDocument> {
    let contigs = read_fai_contigs(fai_path)?;
    let total_bases = contigs.iter().map(|c| c.length).sum();
    let key = cache_key(path);

    if let Ok(mut cache) = document_cache().lock() {
        // Keep more open documents warm (desktop RAM is plentiful).
        if cache.len() >= 32 && !cache.contains_key(&key) {
            let keys: Vec<String> = cache.keys().cloned().take(16).collect();
            for k in keys {
                cache.remove(&k);
            }
        }
        cache.insert(
            key,
            CachedDocument {
                format: FileFormat::Fasta,
                contigs: contigs.clone(),
                sequences: HashMap::new(),
                fully_cached: false,
                fai_path: Some(fai_path.to_path_buf()),
            },
        );
    }

    Ok(SequenceDocument {
        path: path.display().to_string(),
        format: FileFormat::Fasta.as_str().to_string(),
        gzipped: is_gzipped(path),
        contigs,
        total_bases,
        fully_cached: false,
    })
}

/// Query a window via `.fai` (0-based half-open → 1-based inclusive region).
fn extract_fasta_window_fai(
    path: &Path,
    fai_path: &Path,
    contig: &str,
    start: u64,
    end: u64,
) -> Result<String> {
    let index = load_fai_cached(fai_path)?;

    // Resolve contig against the index names (aliases).
    let index_contigs = contigs_from_fai(index.as_ref());
    let resolved = find_contig(&index_contigs, contig)
        .map(|c| c.name.as_str())
        .with_context(|| {
            format!(
                "contig '{contig}' not found in FASTA index '{}' ({} contigs). \
                 Check chr vs no-chr naming between the alignment and reference.",
                fai_path.display(),
                index_contigs.len()
            )
        })?;

    let start_1 = (start.saturating_add(1) as usize).max(1);
    let end_1 = (end as usize).max(start_1);
    let start_pos = Position::try_from(start_1).context("invalid region start")?;
    let end_pos = Position::try_from(end_1).context("invalid region end")?;
    let region = Region::new(resolved, start_pos..=end_pos);

    let mut reader = noodles::fasta::io::indexed_reader::Builder::default()
        .set_index((*index).clone())
        .build_from_path(path)
        .with_context(|| format!("failed to open indexed FASTA '{}'", path.display()))?;

    let record = reader
        .query(&region)
        .with_context(|| format!("failed to query {resolved}:{start_1}-{end_1} in '{}'", path.display()))?;

    Ok(String::from_utf8_lossy(record.sequence().as_ref()).into_owned())
}

pub fn open_sequence_document(path: &Path) -> Result<SequenceDocument> {
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }

    let format = infer_format(path)
        .with_context(|| format!("cannot infer format for '{}'", path.display()))?;

    if !matches!(format, FileFormat::Fasta | FileFormat::Fastq) {
        bail!(
            "View mode currently supports FASTA and FASTQ sequence files (got {format}). \
             GFF/BED and BAM overlays are planned next."
        );
    }

    let meta = std::fs::metadata(path)
        .with_context(|| format!("cannot read metadata for '{}'", path.display()))?;
    let key = cache_key(path);

    // Reuse warm cache if already opened.
    if let Ok(cache) = document_cache().lock() {
        if let Some(cached) = cache.get(&key) {
            let total_bases = cached.contigs.iter().map(|c| c.length).sum();
            return Ok(SequenceDocument {
                path: path.display().to_string(),
                format: format.as_str().to_string(),
                gzipped: is_gzipped(path),
                contigs: cached.contigs.clone(),
                total_bases,
                fully_cached: cached.fully_cached,
            });
        }
    }

    // FASTA: prefer .fai random access for huge genomes (hg38 etc.).
    if format == FileFormat::Fasta {
        let fai_path = fai_sidecar_path(path);
        if fai_path.is_file() {
            return open_via_fai(path, &fai_path);
        }

        // Large FASTA without .fai: fail fast. Auto-building a human-genome .fai
        // can take many minutes and freezes the whole app (blocks the worker pool).
        if meta.len() > MAX_OPEN_FILE_BYTES {
            bail!(
                "reference is too large to open without an index ({:.1} MB). \
                 Run `samtools faidx` next to the FASTA (creates {}.fai) and try again. \
                 Max unindexed open size is {} MB.",
                meta.len() as f64 / (1024.0 * 1024.0),
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("reference.fa"),
                MAX_OPEN_FILE_BYTES / (1024 * 1024)
            );
        }
    } else if meta.len() > MAX_OPEN_FILE_BYTES {
        bail!(
            "file is too large to open without an index ({:.1} MB). \
             Max open size is {} MB.",
            meta.len() as f64 / (1024.0 * 1024.0),
            MAX_OPEN_FILE_BYTES / (1024 * 1024)
        );
    }

    // Load contig index + optionally full sequences in one streaming pass.
    let (contigs, sequences, fully_cached) = match format {
        FileFormat::Fasta => load_fasta_for_view(path)?,
        FileFormat::Fastq => load_fastq_for_view(path)?,
        _ => unreachable!(),
    };

    if contigs.is_empty() {
        bail!("no sequences found in '{}'", path.display());
    }

    let total_bases = contigs.iter().map(|c| c.length).sum();

    if let Ok(mut cache) = document_cache().lock() {
        // Bound cache size: drop other entries when opening a new file (simple LRU-less policy).
        // Keep more open documents warm (desktop RAM is plentiful).
        if cache.len() >= 32 && !cache.contains_key(&key) {
            let keys: Vec<String> = cache.keys().cloned().take(16).collect();
            for k in keys {
                cache.remove(&k);
            }
        }
        cache.insert(
            key,
            CachedDocument {
                format,
                contigs: contigs.clone(),
                sequences,
                fully_cached,
                fai_path: None,
            },
        );
    }

    Ok(SequenceDocument {
        path: path.display().to_string(),
        format: format.as_str().to_string(),
        gzipped: is_gzipped(path),
        contigs,
        total_bases,
        fully_cached,
    })
}

pub fn get_sequence_window(
    path: &Path,
    contig: &str,
    start: u64,
    end: u64,
    reverse_complement: bool,
) -> Result<SequenceSlice> {
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }
    if start >= end {
        bail!("invalid window: start ({start}) must be < end ({end})");
    }

    let key = cache_key(path);

    // Ensure document is open/cached.
    let needs_open = document_cache()
        .lock()
        .map(|cache| !cache.contains_key(&key))
        .unwrap_or(true);
    if needs_open {
        open_sequence_document(path)?;
    }

    let (contig_length, resolved_contig, fai_path) = {
        let cache = document_cache()
            .lock()
            .map_err(|_| anyhow::anyhow!("sequence cache lock poisoned"))?;
        let cached = cache
            .get(&key)
            .with_context(|| format!("document not in cache: {}", path.display()))?;
        let info = find_contig(&cached.contigs, contig).with_context(|| {
            format!(
                "contig '{contig}' not found in '{}' ({} contigs). \
                 Check that the reference uses the same chromosome names as the alignment \
                 (e.g. chr1 vs 1).",
                path.display(),
                cached.contigs.len()
            )
        })?;
        (
            info.length,
            info.name.clone(),
            cached.fai_path.clone(),
        )
    };

    let start = start.min(contig_length);
    let end = end.min(contig_length);
    if start >= end {
        return Ok(SequenceSlice {
            contig: resolved_contig,
            start,
            end: start,
            sequence: String::new(),
            contig_length,
            reverse_complemented: reverse_complement,
        });
    }

    let span = end - start;
    if span > MAX_SEQUENCE_WINDOW {
        bail!(
            "requested window is {span} bp; maximum is {MAX_SEQUENCE_WINDOW} bp. Zoom in further."
        );
    }

    // Serve from memory when possible (try requested name and resolved name).
    if let Some(mut sequence) = slice_from_cache(&key, &resolved_contig, start, end)? {
        if reverse_complement {
            sequence = revcomp(&sequence);
        }
        return Ok(SequenceSlice {
            contig: resolved_contig,
            start,
            end,
            sequence,
            contig_length,
            reverse_complemented: reverse_complement,
        });
    }

    // FAI-backed random access for large genomes (hg38, etc.).
    if let Some(fai_path) = fai_path.as_ref() {
        let mut sequence = extract_fasta_window_fai(path, fai_path, &resolved_contig, start, end)?;
        if reverse_complement {
            sequence = revcomp(&sequence);
        }
        return Ok(SequenceSlice {
            contig: resolved_contig,
            start,
            end,
            sequence,
            contig_length,
            reverse_complemented: reverse_complement,
        });
    }

    // Contig not fully cached yet — load it once (gzip stream), then slice.
    // Oversized contigs are not stored; we extract the window directly.
    let format = {
        let cache = document_cache()
            .lock()
            .map_err(|_| anyhow::anyhow!("sequence cache lock poisoned"))?;
        cache
            .get(&key)
            .map(|c| c.format)
            .unwrap_or(FileFormat::Fasta)
    };

    if contig_length <= FULL_CACHE_CONTIG_BASES {
        ensure_contig_cached(path, &key, &resolved_contig)?;
        if let Some(mut sequence) = slice_from_cache(&key, &resolved_contig, start, end)? {
            if reverse_complement {
                sequence = revcomp(&sequence);
            }
            return Ok(SequenceSlice {
                contig: resolved_contig,
                start,
                end,
                sequence,
                contig_length,
                reverse_complemented: reverse_complement,
            });
        }
    }

    let mut sequence = get_window_uncached(path, format, &resolved_contig, start, end)?;
    if reverse_complement {
        sequence = revcomp(&sequence);
    }

    Ok(SequenceSlice {
        contig: resolved_contig,
        start,
        end,
        sequence,
        contig_length,
        reverse_complemented: reverse_complement,
    })
}

/// Drop cached sequences for a path (optional housekeeping).
pub fn clear_sequence_cache(path: Option<&Path>) {
    let Ok(mut cache) = document_cache().lock() else {
        return;
    };
    match path {
        Some(path) => {
            cache.remove(&cache_key(path));
        }
        None => cache.clear(),
    }
}

fn slice_from_cache(key: &str, contig: &str, start: u64, end: u64) -> Result<Option<String>> {
    let cache = document_cache()
        .lock()
        .map_err(|_| anyhow::anyhow!("sequence cache lock poisoned"))?;
    let Some(cached) = cache.get(key) else {
        return Ok(None);
    };
    let Some(full) = cached.sequences.get(contig) else {
        return Ok(None);
    };
    let s = start as usize;
    let e = (end as usize).min(full.len());
    if s >= full.len() {
        return Ok(Some(String::new()));
    }
    Ok(Some(full[s..e].to_string()))
}

fn ensure_contig_cached(path: &Path, key: &str, contig: &str) -> Result<()> {
    {
        let cache = document_cache()
            .lock()
            .map_err(|_| anyhow::anyhow!("sequence cache lock poisoned"))?;
        if let Some(cached) = cache.get(key) {
            if cached.sequences.contains_key(contig) {
                return Ok(());
            }
            let length = find_contig(&cached.contigs, contig)
                .map(|c| c.length)
                .unwrap_or(0);
            if length > FULL_CACHE_CONTIG_BASES {
                // Too large for full-contig cache; fall through to one-shot extract without storing.
                return Ok(());
            }
            // FAI-backed large refs should not full-scan; skip memory cache for those.
            if cached.fai_path.is_some() && length > FULL_CACHE_CONTIG_BASES {
                return Ok(());
            }
        }
    }

    let format = infer_format(path)?;
    let sequence = match format {
        FileFormat::Fasta => {
            // Prefer fai extract of full contig when available.
            let fai_path = document_cache()
                .lock()
                .ok()
                .and_then(|c| c.get(key).and_then(|d| d.fai_path.clone()));
            if let Some(fai_path) = fai_path {
                let len = {
                    let cache = document_cache()
                        .lock()
                        .map_err(|_| anyhow::anyhow!("sequence cache lock poisoned"))?;
                    find_contig(
                        &cache
                            .get(key)
                            .map(|c| c.contigs.clone())
                            .unwrap_or_default(),
                        contig,
                    )
                    .map(|c| c.length)
                    .unwrap_or(0)
                };
                if len > 0 && len <= FULL_CACHE_CONTIG_BASES {
                    extract_fasta_window_fai(path, &fai_path, contig, 0, len)?
                } else {
                    return Ok(());
                }
            } else {
                extract_full_fasta_contig(path, contig)?
            }
        }
        FileFormat::Fastq => extract_full_fastq_contig(path, contig)?,
        _ => bail!("unsupported format"),
    };

    if sequence.len() as u64 > FULL_CACHE_CONTIG_BASES {
        // Don't store huge contigs; store nothing and extract window on demand below.
        return Ok(());
    }

    if let Ok(mut cache) = document_cache().lock() {
        if let Some(cached) = cache.get_mut(key) {
            cached.sequences.insert(contig.to_string(), sequence);
        }
    }
    Ok(())
}

/// Load contig metadata; if total size is modest, also load all sequences (one gzip pass).
fn load_fasta_for_view(path: &Path) -> Result<(Vec<ContigInfo>, HashMap<String, String>, bool)> {
    let mut reader = noodles::fasta::io::reader::Builder::default()
        .build_from_reader(open_buf_reader(path)?)
        .with_context(|| format!("failed to open FASTA '{}'", path.display()))?;

    let mut contigs = Vec::new();
    let mut sequences = HashMap::new();
    let mut total = 0u64;

    // First pass materializes records as we go — for full cache we keep sequences.
    // We decide whether to keep sequences after seeing total; to avoid double scan of gzip,
    // we always collect sequences while streaming, then drop them if too large.
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTA record #{}", index + 1))?;
        let name = String::from_utf8_lossy(record.name()).to_string();
        let seq = String::from_utf8_lossy(record.sequence().as_ref()).into_owned();
        let length = seq.len() as u64;
        total = total.saturating_add(length);
        contigs.push(ContigInfo {
            name: name.clone(),
            length,
        });
        sequences.insert(name, seq);
    }

    let fully_cached = total <= FULL_CACHE_TOTAL_BASES;
    if !fully_cached {
        // Keep only contigs that are individually small (still helps multi-contig FA with one huge chrom).
        sequences.retain(|_, seq| (seq.len() as u64) <= FULL_CACHE_CONTIG_BASES);
        // If we retained all after filter, treat as fully cached.
        let retained_total: u64 = sequences.values().map(|s| s.len() as u64).sum();
        let fully = retained_total == total && !sequences.is_empty();
        return Ok((contigs, sequences, fully));
    }

    Ok((contigs, sequences, true))
}

fn load_fastq_for_view(path: &Path) -> Result<(Vec<ContigInfo>, HashMap<String, String>, bool)> {
    let mut reader = noodles::fastq::io::Reader::new(open_buf_reader(path)?);

    let mut contigs = Vec::new();
    let mut sequences = HashMap::new();
    let mut total = 0u64;

    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTQ record #{}", index + 1))?;
        let name = String::from_utf8_lossy(record.name()).to_string();
        let seq = String::from_utf8_lossy(record.sequence()).into_owned();
        let length = seq.len() as u64;
        total = total.saturating_add(length);
        contigs.push(ContigInfo {
            name: name.clone(),
            length,
        });
        sequences.insert(name, seq);
    }

    let fully_cached = total <= FULL_CACHE_TOTAL_BASES;
    if !fully_cached {
        sequences.retain(|_, seq| (seq.len() as u64) <= FULL_CACHE_CONTIG_BASES);
        let retained_total: u64 = sequences.values().map(|s| s.len() as u64).sum();
        let fully = retained_total == total && !sequences.is_empty();
        return Ok((contigs, sequences, fully));
    }

    Ok((contigs, sequences, true))
}

fn extract_full_fasta_contig(path: &Path, contig: &str) -> Result<String> {
    let mut reader = noodles::fasta::io::reader::Builder::default()
        .build_from_reader(open_buf_reader(path)?)
        .with_context(|| format!("failed to open FASTA '{}'", path.display()))?;

    let aliases = contig_name_aliases(contig);
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTA record #{}", index + 1))?;
        let name = String::from_utf8_lossy(record.name());
        if aliases.iter().any(|a| a == name.as_ref()) || name.eq_ignore_ascii_case(contig) {
            return Ok(String::from_utf8_lossy(record.sequence().as_ref()).into_owned());
        }
    }
    bail!("contig '{contig}' not found");
}

fn extract_full_fastq_contig(path: &Path, contig: &str) -> Result<String> {
    let mut reader = noodles::fastq::io::Reader::new(open_buf_reader(path)?);
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTQ record #{}", index + 1))?;
        let name = String::from_utf8_lossy(record.name());
        if name == contig {
            return Ok(String::from_utf8_lossy(record.sequence()).into_owned());
        }
    }
    bail!("read '{contig}' not found");
}

/// Fallback extract when contig is too large to cache fully.
fn extract_fasta_window(path: &Path, contig: &str, start: u64, end: u64) -> Result<String> {
    // Prefer fai when available (even if open path didn't store it — e.g. race).
    let fai = fai_sidecar_path(path);
    if fai.is_file() {
        return extract_fasta_window_fai(path, &fai, contig, start, end);
    }
    let full = extract_full_fasta_contig(path, contig)?;
    let s = start as usize;
    let e = (end as usize).min(full.len());
    if s >= full.len() {
        return Ok(String::new());
    }
    Ok(full[s..e].to_string())
}

fn extract_fastq_window(path: &Path, contig: &str, start: u64, end: u64) -> Result<String> {
    let full = extract_full_fastq_contig(path, contig)?;
    let s = start as usize;
    let e = (end as usize).min(full.len());
    if s >= full.len() {
        return Ok(String::new());
    }
    Ok(full[s..e].to_string())
}

fn get_window_uncached(
    path: &Path,
    format: FileFormat,
    contig: &str,
    start: u64,
    end: u64,
) -> Result<String> {
    match format {
        FileFormat::Fasta => extract_fasta_window(path, contig, start, end),
        FileFormat::Fastq => extract_fastq_window(path, contig, start, end),
        _ => bail!("unsupported"),
    }
}

fn revcomp(seq: &str) -> String {
    seq.chars()
        .rev()
        .map(|base| match base {
            'A' => 'T',
            'T' => 'A',
            'G' => 'C',
            'C' => 'G',
            'a' => 't',
            't' => 'a',
            'g' => 'c',
            'c' => 'g',
            'U' => 'A',
            'u' => 'a',
            other => other,
        })
        .collect()
}

/// Helper for tests / tooling: resolve a path relative to the repo test fixtures when present.
#[allow(dead_code)]
pub fn sample_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test")
        .join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn opens_multi_contig_fasta() {
        clear_sequence_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.fasta");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, ">chr1\nACGTACGT").unwrap();
        writeln!(file, ">chr2\nTT").unwrap();

        let doc = open_sequence_document(&path).unwrap();
        assert_eq!(doc.contigs.len(), 2);
        assert_eq!(doc.contigs[0].name, "chr1");
        assert_eq!(doc.contigs[0].length, 8);
        assert_eq!(doc.contigs[1].length, 2);
        assert_eq!(doc.total_bases, 10);
        assert!(doc.fully_cached);
    }

    #[test]
    fn extracts_window_half_open() {
        clear_sequence_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.fasta");
        std::fs::write(&path, b">chr1\nACGTACGT\n").unwrap();

        let slice = get_sequence_window(&path, "chr1", 2, 6, false).unwrap();
        assert_eq!(slice.sequence, "GTAC");
        assert_eq!(slice.start, 2);
        assert_eq!(slice.end, 6);
    }

    #[test]
    fn repeated_windows_use_cache() {
        clear_sequence_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.fasta");
        std::fs::write(&path, b">chr1\nACGTACGTACGT\n").unwrap();

        let _ = open_sequence_document(&path).unwrap();
        let a = get_sequence_window(&path, "chr1", 0, 4, false).unwrap();
        let b = get_sequence_window(&path, "chr1", 4, 8, false).unwrap();
        assert_eq!(a.sequence, "ACGT");
        assert_eq!(b.sequence, "ACGT");
    }

    #[test]
    fn reverse_complements_window() {
        clear_sequence_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.fasta");
        std::fs::write(&path, b">chr1\nACGT\n").unwrap();

        let slice = get_sequence_window(&path, "chr1", 0, 4, true).unwrap();
        assert_eq!(slice.sequence, "ACGT"); // revcomp of ACGT
        assert!(slice.reverse_complemented);
    }

    #[test]
    fn rejects_oversized_window() {
        clear_sequence_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.fasta");
        let bases = "A".repeat((MAX_SEQUENCE_WINDOW + 10) as usize);
        std::fs::write(&path, format!(">chr1\n{bases}\n")).unwrap();

        let err = get_sequence_window(&path, "chr1", 0, MAX_SEQUENCE_WINDOW + 1, false).unwrap_err();
        assert!(err.to_string().contains("maximum"));
    }

    #[test]
    fn rejects_unsupported_format() {
        clear_sequence_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.bed");
        std::fs::write(&path, b"chr1\t0\t10\n").unwrap();
        assert!(open_sequence_document(&path).is_err());
    }

    #[test]
    fn opens_via_fai_and_queries_window() {
        clear_sequence_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ref.fa");
        // Fixed-width lines so fai math is correct.
        std::fs::write(&path, b">chr1\nACGTACGTAC\nGTACGTACGT\n>chr2\nTTTT\n").unwrap();
        let fai = build_fai_sidecar(&path).unwrap();
        assert!(fai.is_file());

        // Clear any stream-cache from index build path, open fresh via fai only.
        clear_sequence_cache(None);
        let doc = open_sequence_document(&path).unwrap();
        assert_eq!(doc.contigs.len(), 2);
        assert_eq!(doc.contigs[0].length, 20);
        assert!(!doc.fully_cached);

        let slice = get_sequence_window(&path, "chr1", 0, 4, false).unwrap();
        assert_eq!(slice.sequence, "ACGT");
        let slice2 = get_sequence_window(&path, "chr1", 8, 12, false).unwrap();
        assert_eq!(slice2.sequence, "ACGT");
        // Alias: request without chr prefix.
        let slice3 = get_sequence_window(&path, "1", 0, 4, false).unwrap();
        assert_eq!(slice3.sequence, "ACGT");
        assert_eq!(slice3.contig, "chr1");
    }

    #[test]
    fn contig_aliases_chr_and_bare() {
        let contigs = vec![
            ContigInfo {
                name: "chr1".into(),
                length: 100,
            },
            ContigInfo {
                name: "chrM".into(),
                length: 50,
            },
        ];
        assert_eq!(find_contig(&contigs, "1").unwrap().name, "chr1");
        assert_eq!(find_contig(&contigs, "chr1").unwrap().name, "chr1");
        assert_eq!(find_contig(&contigs, "MT").unwrap().name, "chrM");
    }
}
