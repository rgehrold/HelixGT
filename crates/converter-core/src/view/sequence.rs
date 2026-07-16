use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::convert::open_buf_reader;
use crate::format::{infer_format, is_gzipped, FileFormat};

/// Maximum bases returned in one window (keeps IPC payloads bounded).
pub const MAX_SEQUENCE_WINDOW: u64 = 2_000_000;

/// Refuse to scan unindexed files larger than this when opening (bytes on disk).
const MAX_OPEN_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// If total bases across contigs are at or below this, keep the whole file in memory after open.
const FULL_CACHE_TOTAL_BASES: u64 = 256 * 1024 * 1024;

/// If a single contig/read is at or below this, cache its full sequence on first access.
const FULL_CACHE_CONTIG_BASES: u64 = 64 * 1024 * 1024;

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
    if meta.len() > MAX_OPEN_FILE_BYTES {
        bail!(
            "file is too large to open without an index ({:.1} MB). \
             For huge genomes, use a plain (uncompressed) FASTA with a .fai index. \
             Max open size is {} MB.",
            meta.len() as f64 / (1024.0 * 1024.0),
            MAX_OPEN_FILE_BYTES / (1024 * 1024)
        );
    }

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
        if cache.len() >= 12 && !cache.contains_key(&key) {
            cache.clear();
        }
        cache.insert(
            key,
            CachedDocument {
                format,
                contigs: contigs.clone(),
                sequences,
                fully_cached,
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

    let contig_length = {
        let cache = document_cache()
            .lock()
            .map_err(|_| anyhow::anyhow!("sequence cache lock poisoned"))?;
        let cached = cache
            .get(&key)
            .with_context(|| format!("document not in cache: {}", path.display()))?;
        cached
            .contigs
            .iter()
            .find(|c| c.name == contig)
            .map(|c| c.length)
            .with_context(|| format!("contig '{contig}' not found in '{}'", path.display()))?
    };

    let start = start.min(contig_length);
    let end = end.min(contig_length);
    if start >= end {
        return Ok(SequenceSlice {
            contig: contig.to_string(),
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

    // Serve from memory when possible.
    if let Some(mut sequence) = slice_from_cache(&key, contig, start, end)? {
        if reverse_complement {
            sequence = revcomp(&sequence);
        }
        return Ok(SequenceSlice {
            contig: contig.to_string(),
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
        ensure_contig_cached(path, &key, contig)?;
        if let Some(mut sequence) = slice_from_cache(&key, contig, start, end)? {
            if reverse_complement {
                sequence = revcomp(&sequence);
            }
            return Ok(SequenceSlice {
                contig: contig.to_string(),
                start,
                end,
                sequence,
                contig_length,
                reverse_complemented: reverse_complement,
            });
        }
    }

    let mut sequence = get_window_uncached(path, format, contig, start, end)?;
    if reverse_complement {
        sequence = revcomp(&sequence);
    }

    Ok(SequenceSlice {
        contig: contig.to_string(),
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
            let length = cached
                .contigs
                .iter()
                .find(|c| c.name == contig)
                .map(|c| c.length)
                .unwrap_or(0);
            if length > FULL_CACHE_CONTIG_BASES {
                // Too large for full-contig cache; fall through to one-shot extract without storing.
                return Ok(());
            }
        }
    }

    let format = infer_format(path)?;
    let sequence = match format {
        FileFormat::Fasta => extract_full_fasta_contig(path, contig)?,
        FileFormat::Fastq => extract_full_fastq_contig(path, contig)?,
        _ => bail!("unsupported format"),
    };

    if sequence.len() as u64 > FULL_CACHE_CONTIG_BASES {
        // Don't store huge contigs; store nothing and extract window on demand below.
        // For oversized contigs we extract the window directly once.
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

    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTA record #{}", index + 1))?;
        let name = String::from_utf8_lossy(record.name());
        if name == contig {
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
}
