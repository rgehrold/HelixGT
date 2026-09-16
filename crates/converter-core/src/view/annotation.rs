use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::convert::open_buf_reader;
use crate::format::{infer_format, is_gzipped, FileFormat};

use super::contig::resolve_contig_name;

/// Hard cap on features returned for one viewport request (keeps IPC small).
pub const MAX_FEATURES_PER_WINDOW: usize = 2_500;

/// Refuse to load annotation files larger than this (bytes on disk).
const MAX_ANNOTATION_FILE_BYTES: u64 = 150 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationContigSpan {
    pub name: String,
    /// Exclusive max coordinate (0-based) of any feature on this contig.
    pub length: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationDocument {
    pub path: String,
    pub format: String,
    pub gzipped: bool,
    pub feature_count: u64,
    pub contigs: Vec<String>,
    /// Per-contig span so the viewer can pan without a FASTA.
    pub contig_spans: Vec<AnnotationContigSpan>,
}

/// Interval feature in **0-based half-open** coordinates `[start, end)`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationFeature {
    pub id: u64,
    pub contig: String,
    pub start: u64,
    pub end: u64,
    pub name: String,
    pub feature_type: String,
    pub strand: String,
    pub source: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureWindow {
    pub contig: String,
    pub start: u64,
    pub end: u64,
    pub features: Vec<AnnotationFeature>,
    pub truncated: bool,
    pub total_in_range: u64,
}

struct CachedAnnotations {
    features: Vec<AnnotationFeature>,
    /// contig → sorted feature indices by start
    by_contig: HashMap<String, Vec<usize>>,
    /// contig → max feature length (for overlap binary search)
    max_len: HashMap<String, u64>,
    contigs: Vec<String>,
    contig_spans: Vec<AnnotationContigSpan>,
}

fn annotation_cache() -> &'static Mutex<HashMap<String, CachedAnnotations>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedAnnotations>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

pub fn clear_annotation_cache(path: Option<&Path>) {
    let Ok(mut cache) = annotation_cache().lock() else {
        return;
    };
    match path {
        Some(path) => {
            cache.remove(&cache_key(path));
        }
        None => cache.clear(),
    }
}

pub fn open_annotation_document(path: &Path) -> Result<AnnotationDocument> {
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }

    let format = infer_format(path)
        .with_context(|| format!("cannot infer format for '{}'", path.display()))?;
    if !matches!(format, FileFormat::Gff | FileFormat::Bed) {
        bail!("annotation tracks support GFF/GFF3 and BED (got {format})");
    }

    let meta = std::fs::metadata(path)
        .with_context(|| format!("cannot read metadata for '{}'", path.display()))?;
    if meta.len() > MAX_ANNOTATION_FILE_BYTES {
        bail!(
            "annotation file is too large ({:.1} MB; max {} MB)",
            meta.len() as f64 / (1024.0 * 1024.0),
            MAX_ANNOTATION_FILE_BYTES / (1024 * 1024)
        );
    }

    let key = cache_key(path);
    if let Ok(cache) = annotation_cache().lock() {
        if let Some(cached) = cache.get(&key) {
            return Ok(AnnotationDocument {
                path: path.display().to_string(),
                format: format.as_str().to_string(),
                gzipped: is_gzipped(path),
                feature_count: cached.features.len() as u64,
                contigs: cached.contigs.clone(),
                contig_spans: cached.contig_spans.clone(),
            });
        }
    }

    let features = match format {
        FileFormat::Gff => load_gff_features(path)?,
        FileFormat::Bed => load_bed_features(path)?,
        _ => unreachable!(),
    };

    let mut by_contig: HashMap<String, Vec<usize>> = HashMap::new();
    let mut max_end: HashMap<String, u64> = HashMap::new();
    let mut max_len: HashMap<String, u64> = HashMap::new();
    for (index, feature) in features.iter().enumerate() {
        by_contig
            .entry(feature.contig.clone())
            .or_default()
            .push(index);
        let entry = max_end.entry(feature.contig.clone()).or_insert(0);
        *entry = (*entry).max(feature.end);
        let span = feature.end.saturating_sub(feature.start);
        let len_ent = max_len.entry(feature.contig.clone()).or_insert(0);
        *len_ent = (*len_ent).max(span);
    }
    for indices in by_contig.values_mut() {
        indices.sort_by_key(|&i| features[i].start);
    }

    let mut contigs: Vec<String> = by_contig.keys().cloned().collect();
    contigs.sort();
    let contig_spans: Vec<AnnotationContigSpan> = contigs
        .iter()
        .map(|name| AnnotationContigSpan {
            name: name.clone(),
            length: *max_end.get(name).unwrap_or(&0),
        })
        .collect();

    let feature_count = features.len() as u64;
    let contigs_out = contigs.clone();
    let spans_out = contig_spans.clone();

    if let Ok(mut cache) = annotation_cache().lock() {
        if cache.len() >= 6 && !cache.contains_key(&key) {
            cache.clear();
        }
        cache.insert(
            key,
            CachedAnnotations {
                features,
                by_contig,
                max_len,
                contigs,
                contig_spans,
            },
        );
    }

    Ok(AnnotationDocument {
        path: path.display().to_string(),
        format: format.as_str().to_string(),
        gzipped: is_gzipped(path),
        feature_count,
        contigs: contigs_out,
        contig_spans: spans_out,
    })
}

pub fn get_features_in_range(
    path: &Path,
    contig: &str,
    start: u64,
    end: u64,
) -> Result<FeatureWindow> {
    get_features_in_range_filtered(path, contig, start, end, None)
}

/// `feature_types` is a case-insensitive allow-list of GFF `type` / BED feature types.
/// Empty / None = all types.
pub fn get_features_in_range_filtered(
    path: &Path,
    contig: &str,
    start: u64,
    end: u64,
    feature_types: Option<&[String]>,
) -> Result<FeatureWindow> {
    if start >= end {
        bail!("invalid window: start ({start}) must be < end ({end})");
    }

    let key = cache_key(path);
    let needs_open = annotation_cache()
        .lock()
        .map(|cache| !cache.contains_key(&key))
        .unwrap_or(true);
    if needs_open {
        open_annotation_document(path)?;
    }

    let cache = annotation_cache()
        .lock()
        .map_err(|_| anyhow::anyhow!("annotation cache lock poisoned"))?;
    let cached = cache
        .get(&key)
        .with_context(|| format!("annotation not in cache: {}", path.display()))?;

    let resolved = resolve_contig_name(cached.contigs.iter().map(|s| s.as_str()), contig)
        .unwrap_or_else(|| contig.to_string());

    let Some(indices) = cached.by_contig.get(&resolved) else {
        return Ok(FeatureWindow {
            contig: resolved,
            start,
            end,
            features: Vec::new(),
            truncated: false,
            total_in_range: 0,
        });
    };

    let type_filter: Option<Vec<String>> = feature_types.and_then(|types| {
        if types.is_empty() {
            None
        } else {
            Some(types.iter().map(|t| t.to_ascii_lowercase()).collect())
        }
    });

    let max_len = cached.max_len.get(&resolved).copied().unwrap_or(0);
    // First index whose start could still overlap [start, end).
    let min_start = start.saturating_sub(max_len);
    let lo = indices.partition_point(|&i| cached.features[i].start < min_start);
    let hi = indices.partition_point(|&i| cached.features[i].start < end);

    let mut matches = Vec::new();
    let mut total_in_range = 0u64;
    for &idx in &indices[lo..hi] {
        let feature = &cached.features[idx];
        if feature.end <= start {
            continue;
        }
        if let Some(ref allow) = type_filter {
            if !allow.iter().any(|t| t.eq_ignore_ascii_case(&feature.feature_type)) {
                continue;
            }
        }
        total_in_range += 1;
        if matches.len() < MAX_FEATURES_PER_WINDOW {
            matches.push(feature.clone());
        }
    }

    let truncated = total_in_range > matches.len() as u64;

    Ok(FeatureWindow {
        contig: resolved,
        start,
        end,
        features: matches,
        truncated,
        total_in_range,
    })
}

fn load_gff_features(path: &Path) -> Result<Vec<AnnotationFeature>> {
    let mut reader = noodles::gff::io::Reader::new(open_buf_reader(path)?);
    let mut features = Vec::new();
    let mut next_id = 1u64;

    for (index, result) in reader.record_bufs().enumerate() {
        let record = result.with_context(|| format!("invalid GFF record #{}", index + 1))?;
        // GFF 1-based inclusive → 0-based half-open
        let start = (record.start().get().saturating_sub(1)) as u64;
        let end = (record.end().get() as u64).max(start.saturating_add(1));
        let name = record
            .attributes()
            .get("ID")
            .or_else(|| record.attributes().get("Name"))
            .and_then(|value| value.as_string())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("feature_{next_id}"));
        let strand = match record.strand() {
            noodles::gff::record::Strand::Forward => "+",
            noodles::gff::record::Strand::Reverse => "-",
            noodles::gff::record::Strand::Unknown => "?",
            noodles::gff::record::Strand::None => ".",
        }
        .to_string();
        let feature_type = record.ty().to_string();
        let source = record.source().to_string();
        let score = record.score().map(|s| f64::from(s));

        features.push(AnnotationFeature {
            id: next_id,
            contig: record.reference_sequence_name().to_string(),
            start,
            end,
            name,
            feature_type,
            strand,
            source,
            score,
        });
        next_id += 1;
    }

    Ok(features)
}

fn load_bed_features(path: &Path) -> Result<Vec<AnnotationFeature>> {
    let reader = crate::convert::open_text_reader(path)?;
    let reader = std::io::BufReader::new(reader);
    let mut features = Vec::new();
    let mut next_id = 1u64;
    let mut line_index = 0usize;

    for line in reader.lines() {
        line_index += 1;
        let line = line.with_context(|| format!("failed to read BED line #{line_index}"))?;
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("track") || line.starts_with("browser")
        {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 3 {
            continue;
        }
        let contig = fields[0].to_string();
        let start: u64 = fields[1]
            .parse()
            .with_context(|| format!("invalid BED start on line #{line_index}"))?;
        let end: u64 = fields[2]
            .parse()
            .with_context(|| format!("invalid BED end on line #{line_index}"))?;
        if end <= start {
            continue;
        }
        let name = fields
            .get(3)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("feature_{next_id}"));
        let score = fields.get(4).and_then(|s| s.parse::<f64>().ok());
        let strand = fields
            .get(5)
            .map(|s| s.to_string())
            .unwrap_or_else(|| ".".to_string());

        features.push(AnnotationFeature {
            id: next_id,
            contig,
            start,
            end,
            name,
            feature_type: "region".into(),
            strand,
            source: "bed".into(),
            score,
        });
        next_id += 1;
    }

    Ok(features)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn capped_query_preserves_total_and_type_filter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dense.bed");
        let total = MAX_FEATURES_PER_WINDOW + 123;
        let mut file = std::fs::File::create(&path).unwrap();
        for i in 0..total {
            writeln!(file, "chr1\t{i}\t{}\tf{i}", i + 10).unwrap();
        }
        drop(file);
        let result = get_features_in_range(&path, "chr1", 0, total as u64 + 10).unwrap();
        assert_eq!(result.features.len(), MAX_FEATURES_PER_WINDOW);
        assert_eq!(result.total_in_range, total as u64);
        assert!(result.truncated);
        let filtered = get_features_in_range_filtered(&path, "chr1", 0,
            total as u64 + 10, Some(&["gene".to_string()])).unwrap();
        assert_eq!(filtered.total_in_range, 0);
        assert!(!filtered.truncated);
    }

    #[test]
    fn loads_gff_and_queries_range() {
        clear_annotation_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.gff");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "##gff-version 3").unwrap();
        writeln!(file, "chr1\t.\tgene\t1\t10\t.\t+\t.\tID=gene1;Name=gene1").unwrap();
        writeln!(file, "chr1\t.\texon\t20\t30\t.\t-\t.\tID=exon1").unwrap();
        writeln!(file, "chr2\t.\tgene\t1\t5\t.\t+\t.\tID=gene2").unwrap();

        let doc = open_annotation_document(&path).unwrap();
        assert_eq!(doc.feature_count, 3);
        assert!(doc.contigs.contains(&"chr1".to_string()));

        // GFF 1-10 → [0,10); query [0,15) should hit gene1 only if end<=20
        let window = get_features_in_range(&path, "chr1", 0, 15).unwrap();
        assert_eq!(window.features.len(), 1);
        assert_eq!(window.features[0].name, "gene1");
        assert_eq!(window.features[0].start, 0);
        assert_eq!(window.features[0].end, 10);

        let window2 = get_features_in_range(&path, "chr1", 15, 40).unwrap();
        assert_eq!(window2.features.len(), 1);
        assert_eq!(window2.features[0].name, "exon1");
    }

    #[test]
    fn loads_bed_zero_based() {
        clear_annotation_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.bed");
        std::fs::write(&path, b"chr1\t0\t10\tfeature1\t100\t+\nchr2\t20\t40\tfeature2\t200\t-\n")
            .unwrap();

        let doc = open_annotation_document(&path).unwrap();
        assert_eq!(doc.feature_count, 2);

        let window = get_features_in_range(&path, "chr1", 0, 10).unwrap();
        assert_eq!(window.features.len(), 1);
        assert_eq!(window.features[0].start, 0);
        assert_eq!(window.features[0].end, 10);
        assert_eq!(window.features[0].strand, "+");
    }

    #[test]
    fn contig_alias_and_type_filter() {
        clear_annotation_cache(None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.gff");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "##gff-version 3").unwrap();
        writeln!(file, "chr1\t.\tgene\t1\t10\t.\t+\t.\tID=g1").unwrap();
        writeln!(file, "chr1\t.\texon\t2\t8\t.\t+\t.\tID=e1").unwrap();
        writeln!(file, "chr1\t.\tCDS\t3\t7\t.\t+\t.\tID=c1").unwrap();

        open_annotation_document(&path).unwrap();
        let via_alias = get_features_in_range(&path, "1", 0, 15).unwrap();
        assert_eq!(via_alias.features.len(), 3);

        let genes = get_features_in_range_filtered(
            &path,
            "1",
            0,
            15,
            Some(&["gene".to_string()]),
        )
        .unwrap();
        assert_eq!(genes.features.len(), 1);
        assert_eq!(genes.features[0].feature_type, "gene");
    }
}
