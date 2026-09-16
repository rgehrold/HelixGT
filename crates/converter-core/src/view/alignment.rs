use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use noodles::sam::alignment::record::QualityScores as QualTrait;

use anyhow::{bail, Context, Result};
use noodles::bam;
use noodles::core::{Position, Region};
use noodles::sam::alignment::record::cigar::op::Kind as CigarKind;
use noodles::sam::alignment::Record as AlignmentRecord;
use noodles::sam::Header as SamHeader;
use serde::Serialize;

use crate::external::{
    find_alignment_index, new_command, resolve_samtools_path, samtools_index,
};
use crate::format::{infer_format, FileFormat};
use crate::tools::ToolPaths;

use super::contig::resolve_contig_name;

/// Max reads returned for a pileup window (keeps IPC + paint responsive).
pub const MAX_READS_PER_WINDOW: usize = 2_500;

/// Soft cap for very small windows where users expect every read.
const MAX_READS_SMALL_WINDOW: usize = 4_000;

/// When sequences are requested, hard-cap harder (payload is O(reads × read_len)).
const MAX_READS_WITH_SEQUENCES: usize = 1_500;

/// Default number of coverage bins if client does not specify.
pub const DEFAULT_COVERAGE_BINS: usize = 200;

/// Maximum bins for a single coverage request (pixel LOD + tight per-base).
pub const MAX_COVERAGE_BINS: usize = 4_096;

/// Hard cap on how many alignments we scan for coverage in one *wide* request.
/// Difference-array is cheap; the cap is for CRAM/samtools decode time.
/// Normal View windows (≤ 2 Mb) scan every overlapping read.
const MAX_COVERAGE_SCAN_READS: usize = 200_000;
/// Whole-contig overview: reads per tile (tiles are spread across the contig).
const MAX_OVERVIEW_READS_PER_TILE: usize = 4_000;
const OVERVIEW_TILES: usize = 24;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlignmentContig {
    pub name: String,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlignmentDocument {
    pub path: String,
    pub format: String,
    pub indexed: bool,
    pub contigs: Vec<AlignmentContig>,
    pub requires_reference: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageBin {
    /// 0-based half-open start of bin on contig.
    pub start: u64,
    pub end: u64,
    pub depth: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageWindow {
    pub contig: String,
    pub start: u64,
    pub end: u64,
    pub bins: Vec<CoverageBin>,
    pub max_depth: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CigarOp {
    /// SAM CIGAR op: M I D N S H P = X
    pub op: String,
    pub length: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlignmentRead {
    pub name: String,
    /// 0-based half-open alignment span on reference (consumes M/D/N/=/X).
    pub start: u64,
    pub end: u64,
    pub strand: String,
    pub mapq: u8,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cigar: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cigar_ops: Vec<CigarOp>,
    pub flags: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sequence: String,
    /// ASCII Phred+33 qualities when present, else empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub qualities: String,
    pub is_paired: bool,
    pub is_proper_pair: bool,
    pub is_unmapped: bool,
    pub is_mate_unmapped: bool,
    pub is_reverse: bool,
    pub is_secondary: bool,
    pub is_supplementary: bool,
    pub is_duplicate: bool,
    pub is_qc_fail: bool,
    pub is_first_in_pair: bool,
    pub is_second_in_pair: bool,
    pub template_length: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mate_contig: String,
    /// 0-based mate start when available, else None.
    pub mate_start: Option<u64>,
}

/// Combined coverage + optional pileup from a single indexed scan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlignmentWindow {
    pub coverage: CoverageWindow,
    pub reads: ReadsWindow,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadsWindow {
    pub contig: String,
    pub start: u64,
    pub end: u64,
    pub reads: Vec<AlignmentRead>,
    pub truncated: bool,
    pub total_in_range: u64,
}

#[derive(Debug, Clone)]
pub struct ReadQueryOptions {
    pub include_secondary: bool,
    pub include_supplementary: bool,
    pub include_duplicates: bool,
    pub min_mapq: u8,
    /// When false, omit per-base sequence/quality strings (much smaller payloads).
    pub include_sequences: bool,
}

impl Default for ReadQueryOptions {
    fn default() -> Self {
        Self {
            include_secondary: false,
            include_supplementary: false,
            include_duplicates: false,
            min_mapq: 0,
            include_sequences: true,
        }
    }
}

pub fn open_alignment_document(
    path: &Path,
    tool_paths: &ToolPaths,
    reference_path: Option<&Path>,
) -> Result<AlignmentDocument> {
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }

    let format = infer_format(path)
        .with_context(|| format!("cannot infer format for '{}'", path.display()))?;
    if !matches!(format, FileFormat::Bam | FileFormat::Cram | FileFormat::Sam) {
        bail!("alignment tracks support BAM, CRAM, or SAM (got {format})");
    }

    let requires_reference = format == FileFormat::Cram;

    if requires_reference {
        match reference_path {
            Some(r) if r.is_file() => {}
            Some(r) => bail!("CRAM reference not found: {}", r.display()),
            None => bail!(
                "CRAM viewing requires a reference FASTA. Set a reference path (Convert reference field)."
            ),
        }
    }

    if format == FileFormat::Sam {
        bail!(
            "View mode needs indexed BAM/CRAM for region queries. \
             Convert SAM → BAM first (Convert mode), then open the BAM."
        );
    }

    let mut indexed = find_alignment_index(path).is_some();
    if !indexed {
        let samtools = resolve_samtools(tool_paths)?;
        match samtools_index(&samtools, path) {
            Ok(()) => indexed = find_alignment_index(path).is_some(),
            Err(error) => {
                bail!(
                    "alignment is not indexed and automatic indexing failed ({error:#}). \
                     Sort and index the BAM/CRAM (samtools sort | index). \
                     HelixGT looks for file.bam.bai, file.bai, or .csi / .crai sidecars."
                );
            }
        }
    }

    let contigs = if format == FileFormat::Bam {
        read_bam_contigs(path).or_else(|err| {
            // Fall back to samtools if noodles fails for an unusual BAM.
            let samtools = resolve_samtools(tool_paths)?;
            read_sq_contigs(&samtools, path, reference_path)
                .with_context(|| format!("native BAM header failed ({err:#}); samtools also failed"))
        })?
    } else {
        let samtools = resolve_samtools(tool_paths)?;
        read_sq_contigs(&samtools, path, reference_path)?
    };

    if contigs.is_empty() {
        bail!("no @SQ contigs found in '{}'", path.display());
    }

    Ok(AlignmentDocument {
        path: path.display().to_string(),
        format: format.as_str().to_string(),
        indexed,
        contigs,
        requires_reference,
    })
}

fn default_coverage_filters() -> ReadQueryOptions {
    ReadQueryOptions {
        include_secondary: true,
        include_supplementary: true,
        include_duplicates: true,
        min_mapq: 0,
        include_sequences: false,
    }
}

pub fn get_coverage_bins(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    bin_count: usize,
    reference_path: Option<&Path>,
) -> Result<CoverageWindow> {
    get_coverage_bins_filtered(
        path,
        tool_paths,
        contig,
        start,
        end,
        bin_count,
        reference_path,
        &default_coverage_filters(),
    )
}

pub fn get_coverage_bins_filtered(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    bin_count: usize,
    reference_path: Option<&Path>,
    options: &ReadQueryOptions,
) -> Result<CoverageWindow> {
    if start >= end {
        bail!("invalid window: start ({start}) must be < end ({end})");
    }
    let bins_n = bin_count.clamp(1, MAX_COVERAGE_BINS).min((end - start).min(usize::MAX as u64) as usize);

    if is_bam(path) {
        return get_coverage_bins_bam(path, contig, start, end, bins_n, options);
    }

    get_coverage_bins_via_samtools_view(
        path,
        tool_paths,
        contig,
        start,
        end,
        bins_n,
        reference_path,
        options,
    )
}

/// Single indexed scan: coverage histogram plus (optional) pileup sample.
pub fn get_alignment_window(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    bin_count: usize,
    reference_path: Option<&Path>,
    options: &ReadQueryOptions,
    include_reads: bool,
) -> Result<AlignmentWindow> {
    if start >= end {
        bail!("invalid window: start ({start}) must be < end ({end})");
    }
    let bins_n = bin_count.clamp(1, MAX_COVERAGE_BINS).min((end - start).min(usize::MAX as u64) as usize);

    if is_bam(path) {
        return get_alignment_window_bam(path, contig, start, end, bins_n, options, include_reads);
    }

    let coverage = get_coverage_bins_via_samtools_view(
        path,
        tool_paths,
        contig,
        start,
        end,
        bins_n,
        reference_path,
        options,
    )?;
    let reads = if include_reads {
        get_reads_samtools(path, tool_paths, contig, start, end, reference_path, options)?
    } else {
        empty_reads_window(contig, start, end)
    };
    Ok(AlignmentWindow { coverage, reads })
}

/// Coarse whole-contig coverage for the overview sparkline (tiled queries).
pub fn get_overview_coverage(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    contig_length: u64,
    bin_count: usize,
    reference_path: Option<&Path>,
    options: &ReadQueryOptions,
) -> Result<CoverageWindow> {
    if contig_length == 0 {
        bail!("contig length is 0");
    }
    let bins_n = bin_count.clamp(16, MAX_COVERAGE_BINS).min(contig_length.min(usize::MAX as u64) as usize);
    let tiles = OVERVIEW_TILES.min(bins_n).max(1);
    let mut bins = Vec::with_capacity(bins_n);
    let mut max_depth = 0.0f64;
    // Partition on bin boundaries and clip boundary-crossing reads per tile.
    for tile in 0..tiles {
        let first = tile * bins_n / tiles;
        let last = (tile + 1) * bins_n / tiles;
        let t0 = (first as u128 * contig_length as u128 / bins_n as u128) as u64;
        let t1 = (last as u128 * contig_length as u128 / bins_n as u128) as u64;
        let tile_bins = last - first;
        let mut tile_opts = options.clone();
        tile_opts.include_sequences = false;
        let win = if is_bam(path) {
            let mut delta = vec![0.0f64; tile_bins + 1];
            accumulate_coverage_bam(path, contig, t0, t1, tile_bins, t0, t1,
                &mut delta, &tile_opts, MAX_OVERVIEW_READS_PER_TILE)?;
            finalize_coverage_from_delta(contig, t0, t1, tile_bins, &delta)
        } else {
            get_coverage_bins_filtered(path, tool_paths, contig, t0, t1,
                tile_bins, reference_path, &tile_opts)?
        };
        max_depth = max_depth.max(win.max_depth);
        bins.extend(win.bins);
    }
    Ok(CoverageWindow { contig: contig.to_string(), start: 0,
        end: contig_length, bins, max_depth })
}

fn empty_reads_window(contig: &str, start: u64, end: u64) -> ReadsWindow {
    ReadsWindow {
        contig: contig.to_string(),
        start,
        end,
        reads: Vec::new(),
        truncated: false,
        total_in_range: 0,
    }
}

pub fn get_reads_in_range(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    reference_path: Option<&Path>,
) -> Result<ReadsWindow> {
    get_reads_in_range_filtered(
        path,
        tool_paths,
        contig,
        start,
        end,
        reference_path,
        &ReadQueryOptions {
            include_secondary: true,
            include_supplementary: true,
            include_duplicates: true,
            min_mapq: 0,
            include_sequences: false,
        },
    )
}

pub fn get_reads_in_range_filtered(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    reference_path: Option<&Path>,
    options: &ReadQueryOptions,
) -> Result<ReadsWindow> {
    if start >= end {
        bail!("invalid window: start ({start}) must be < end ({end})");
    }

    if is_bam(path) {
        return get_reads_bam(path, contig, start, end, options);
    }

    get_reads_samtools(path, tool_paths, contig, start, end, reference_path, options)
}

// ── Native BAM (noodles + BAI, cached reader) ───────────────────────────────

struct CachedBam {
    reader: bam::io::IndexedReader<noodles::bgzf::Reader<File>>,
    header: SamHeader,
}

thread_local! {
    static BAM_CACHE: RefCell<HashMap<String, CachedBam>> = RefCell::new(HashMap::new());
}

fn bam_cache_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn with_cached_bam<R>(
    path: &Path,
    f: impl FnOnce(&mut CachedBam) -> Result<R>,
) -> Result<R> {
    let key = bam_cache_key(path);
    BAM_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        if !cache.contains_key(&key) {
            if cache.len() >= 8 {
                cache.clear();
            }
            let mut reader = open_indexed_bam(path)?;
            let header = reader
                .read_header()
                .with_context(|| format!("failed to read BAM header: {}", path.display()))?;
            cache.insert(key.clone(), CachedBam { reader, header });
        }
        let handle = cache
            .get_mut(&key)
            .expect("just inserted or already present");
        f(handle)
    })
}

fn resolve_bam_contig(header: &SamHeader, requested: &str) -> Result<String> {
    let names: Vec<String> = header
        .reference_sequences()
        .iter()
        .map(|(name, _)| String::from_utf8_lossy(name).into_owned())
        .collect();
    resolve_contig_name(names.iter().map(|s| s.as_str()), requested).ok_or_else(|| {
        anyhow::anyhow!(
            "contig '{requested}' not found in BAM header ({} @SQ sequences). \
             Check chr vs no-chr naming.",
            names.len()
        )
    })
}

fn get_coverage_bins_bam(
    path: &Path,
    contig: &str,
    start: u64,
    end: u64,
    bins_n: usize,
    options: &ReadQueryOptions,
) -> Result<CoverageWindow> {
    let mut delta = vec![0.0f64; bins_n + 1];
    accumulate_coverage_bam(
        path,
        contig,
        start,
        end,
        bins_n,
        start,
        end,
        &mut delta,
        options,
        coverage_scan_cap(end.saturating_sub(start)),
    )?;
    Ok(finalize_coverage_from_delta(contig, start, end, bins_n, &delta))
}

fn coverage_scan_cap(span: u64) -> usize {
    if span <= 2_000_000 {
        usize::MAX
    } else {
        MAX_COVERAGE_SCAN_READS
    }
}

fn accumulate_coverage_bam(
    path: &Path,
    contig: &str,
    query_start: u64,
    query_end: u64,
    bins_n: usize,
    window_start: u64,
    window_end: u64,
    delta: &mut [f64],
    options: &ReadQueryOptions,
    max_scan: usize,
) -> Result<usize> {
    with_cached_bam(path, |handle| {
        let resolved = resolve_bam_contig(&handle.header, contig)?;
        let region = make_region(&resolved, query_start, query_end)?;
        let query = handle
            .reader
            .query(&handle.header, &region)
            .with_context(|| format!("BAM query failed for {resolved}:{query_start}-{query_end}"))?;
        let mut scanned = 0usize;
        for result in query {
            let record = result.context("invalid BAM record during coverage scan")?;
            if scanned >= max_scan {
                break;
            }
            if !record_passes_flag_filters(&record, options) {
                continue;
            }
            scanned += 1;
            mark_record_coverage(&record, window_start, window_end, bins_n, delta);
        }
        Ok(scanned)
    })
}

fn get_reads_bam(
    path: &Path,
    contig: &str,
    start: u64,
    end: u64,
    options: &ReadQueryOptions,
) -> Result<ReadsWindow> {
    let max_keep = max_reads_to_keep(end.saturating_sub(start), options.include_sequences);
    with_cached_bam(path, |handle| {
        let resolved = resolve_bam_contig(&handle.header, contig)?;
        let region = make_region(&resolved, start, end)?;
        let query = handle
            .reader
            .query(&handle.header, &region)
            .with_context(|| format!("BAM query failed for {resolved}:{start}-{end}"))?;

        let mut reads = Vec::with_capacity(1024.min(max_keep));
        let mut total = 0u64;

        for result in query {
            let record = result.context("invalid BAM record during read fetch")?;
            let Some(read) = record_to_alignment_read(&record, &handle.header, options.include_sequences)
            else {
                continue;
            };
            if !passes_filters(&read, options) {
                continue;
            }
            if read.end <= start || read.start >= end {
                continue;
            }
            total += 1;
            reservoir_push(&mut reads, read, total, max_keep);
        }

        finish_reads_window(&resolved, start, end, reads, total)
    })
}

fn get_alignment_window_bam(
    path: &Path,
    contig: &str,
    start: u64,
    end: u64,
    bins_n: usize,
    options: &ReadQueryOptions,
    include_reads: bool,
) -> Result<AlignmentWindow> {
    let max_keep = max_reads_to_keep(end.saturating_sub(start), options.include_sequences);
    let max_cov = coverage_scan_cap(end.saturating_sub(start));
    with_cached_bam(path, |handle| {
        let resolved = resolve_bam_contig(&handle.header, contig)?;
        let region = make_region(&resolved, start, end)?;
        let query = handle
            .reader
            .query(&handle.header, &region)
            .with_context(|| format!("BAM query failed for {resolved}:{start}-{end}"))?;

        let mut delta = vec![0.0f64; bins_n + 1];
        let mut reads = Vec::new();
        let mut total = 0u64;
        let mut scanned = 0usize;

        for result in query {
            let record = result.context("invalid BAM record during alignment window scan")?;
            if !record_passes_flag_filters(&record, options) {
                continue;
            }
            if scanned < max_cov {
                mark_record_coverage(&record, start, end, bins_n, &mut delta);
            }
            scanned += 1;

            if include_reads {
                let Some(read) =
                    record_to_alignment_read(&record, &handle.header, options.include_sequences)
                else {
                    continue;
                };
                if read.end <= start || read.start >= end {
                    continue;
                }
                total += 1;
                reservoir_push(&mut reads, read, total, max_keep);
            }
        }

        let coverage = finalize_coverage_from_delta(&resolved, start, end, bins_n, &delta);
        let reads = if include_reads {
            finish_reads_window(&resolved, start, end, reads, total)?
        } else {
            empty_reads_window(&resolved, start, end)
        };
        Ok(AlignmentWindow { coverage, reads })
    })
}

fn max_reads_to_keep(span: u64, include_sequences: bool) -> usize {
    let mut max_keep = if span <= 2_000 {
        MAX_READS_SMALL_WINDOW
    } else if span <= 8_000 {
        MAX_READS_PER_WINDOW
    } else {
        MAX_READS_PER_WINDOW.min(1_500)
    };
    if include_sequences {
        max_keep = max_keep.min(MAX_READS_WITH_SEQUENCES);
    }
    max_keep
}

fn finish_reads_window(
    contig: &str,
    start: u64,
    end: u64,
    mut reads: Vec<AlignmentRead>,
    total: u64,
) -> Result<ReadsWindow> {
    reads.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.name.cmp(&b.name)));
    Ok(ReadsWindow {
        contig: contig.to_string(),
        start,
        end,
        truncated: total as usize > reads.len(),
        total_in_range: total,
        reads,
    })
}

/// Deterministic reservoir sample so a truncated window represents the whole span.
fn reservoir_push(reads: &mut Vec<AlignmentRead>, read: AlignmentRead, total: u64, max_keep: usize) {
    if reads.len() < max_keep {
        reads.push(read);
        return;
    }
    if max_keep == 0 || total == 0 {
        return;
    }
    let j = mix64(total) % total;
    if j < max_keep as u64 {
        reads[j as usize] = read;
    }
}

fn mix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn open_indexed_bam(
    path: &Path,
) -> Result<bam::io::IndexedReader<noodles::bgzf::Reader<File>>> {
    bam::io::indexed_reader::Builder::default()
        .build_from_path(path)
        .with_context(|| {
            format!(
                "failed to open indexed BAM '{}'. Ensure a .bai (or .csi) index exists.",
                path.display()
            )
        })
}

fn record_passes_flag_filters(record: &bam::Record, options: &ReadQueryOptions) -> bool {
    let flags = record.flags();
    if flags.is_unmapped() {
        return false;
    }
    if !options.include_secondary && flags.is_secondary() {
        return false;
    }
    if !options.include_supplementary && flags.is_supplementary() {
        return false;
    }
    if !options.include_duplicates && flags.is_duplicate() {
        return false;
    }
    let mapq = record.mapping_quality().map(|mq| mq.get()).unwrap_or(255);
    if mapq != 255 && mapq < options.min_mapq {
        return false;
    }
    true
}

/// Count M/=/X/D on the difference array; skip CIGAR N (introns).
fn mark_record_coverage(
    record: &bam::Record,
    window_start: u64,
    window_end: u64,
    bins_n: usize,
    delta: &mut [f64],
) {
    let Some(Ok(aln_start)) = record.alignment_start() else {
        return;
    };
    let mut ref_pos = (usize::from(aln_start) as u64).saturating_sub(1);
    for result in record.cigar().iter() {
        let Ok(op) = result else {
            continue;
        };
        let len = op.len() as u64;
        match op.kind() {
            CigarKind::Match
            | CigarKind::SequenceMatch
            | CigarKind::SequenceMismatch
            | CigarKind::Deletion => {
                mark_read_on_bins(ref_pos, ref_pos.saturating_add(len), window_start, window_end, bins_n, delta);
                ref_pos = ref_pos.saturating_add(len);
            }
            CigarKind::Skip => {
                ref_pos = ref_pos.saturating_add(len);
            }
            CigarKind::Insertion | CigarKind::SoftClip | CigarKind::HardClip | CigarKind::Pad => {}
        }
    }

}

fn read_bam_contigs(path: &Path) -> Result<Vec<AlignmentContig>> {
    let mut reader = bam::io::reader::Builder::default()
        .build_from_path(path)
        .with_context(|| format!("failed to open BAM '{}'", path.display()))?;
    let header = reader
        .read_header()
        .with_context(|| format!("failed to read BAM header '{}'", path.display()))?;
    Ok(contigs_from_sam_header(&header))
}

fn contigs_from_sam_header(header: &SamHeader) -> Vec<AlignmentContig> {
    header
        .reference_sequences()
        .iter()
        .map(|(name, map)| AlignmentContig {
            name: String::from_utf8_lossy(name).into_owned(),
            length: map.length().get() as u64,
        })
        .collect()
}

fn make_region(contig: &str, start0: u64, end0_exclusive: u64) -> Result<Region> {
    let start_1 = (start0.saturating_add(1) as usize).max(1);
    let end_1 = (end0_exclusive.max(start0.saturating_add(1)) as usize).max(start_1);
    let start = Position::try_from(start_1).context("invalid region start")?;
    let end = Position::try_from(end_1).context("invalid region end")?;
    Ok(Region::new(contig, start..=end))
}

fn record_to_alignment_read(
    record: &bam::Record,
    header: &SamHeader,
    include_sequences: bool,
) -> Option<AlignmentRead> {
    let flags = record.flags();
    if flags.is_unmapped() {
        return None;
    }
    let aln_start = record.alignment_start()?.ok()?;
    let start = (usize::from(aln_start) as u64).saturating_sub(1);
    // Prefer CIGAR alignment span (trait method).
    let span = AlignmentRecord::alignment_span(record)
        .and_then(|r| r.ok())
        .unwrap_or(1)
        .max(1) as u64;
    let end = start.saturating_add(span);

    let mapq = record
        .mapping_quality()
        .map(|mq| mq.get())
        .unwrap_or(255);

    // Bar-only fetches skip CIGAR parse + sequence (start/end/mapq/strand enough).
    let (cigar_ops, cigar_str, sequence, qualities) = if include_sequences {
        let (ops, s) = cigar_from_record(record);
        (
            ops,
            s,
            sequence_from_record(record),
            qualities_from_record(record),
        )
    } else {
        (Vec::new(), String::new(), String::new(), String::new())
    };
    let name = record
        .name()
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_else(|| "*".to_string());

    let mate_start = record
        .mate_alignment_start()
        .and_then(|r| r.ok())
        .map(|p| (usize::from(p) as u64).saturating_sub(1));

    let mate_contig = match record.mate_reference_sequence_id() {
        Some(Ok(id)) => header
            .reference_sequences()
            .get_index(id)
            .map(|(n, _)| String::from_utf8_lossy(n).into_owned())
            .unwrap_or_default(),
        _ => String::new(),
    };

    let template_length = record.template_length();
    let flag_bits = flags.bits();

    Some(AlignmentRead {
        name,
        start,
        end,
        strand: if flags.is_reverse_complemented() {
            "-".to_string()
        } else {
            "+".to_string()
        },
        mapq,
        cigar: cigar_str,
        cigar_ops,
        flags: flag_bits,
        sequence,
        qualities,
        is_paired: flags.is_segmented(),
        is_proper_pair: flags.is_properly_segmented(),
        is_unmapped: flags.is_unmapped(),
        is_mate_unmapped: flags.is_mate_unmapped(),
        is_reverse: flags.is_reverse_complemented(),
        is_secondary: flags.is_secondary(),
        is_supplementary: flags.is_supplementary(),
        is_duplicate: flags.is_duplicate(),
        is_qc_fail: flags.is_qc_fail(),
        is_first_in_pair: flags.is_first_segment(),
        is_second_in_pair: flags.is_last_segment(),
        template_length,
        mate_contig,
        mate_start,
    })
}

fn cigar_from_record(record: &bam::Record) -> (Vec<CigarOp>, String) {
    let mut ops = Vec::new();
    let mut cigar = String::new();
    for result in record.cigar().iter() {
        let Ok(op) = result else {
            continue;
        };
        let ch = cigar_kind_char(op.kind());
        let len = op.len() as u32;
        cigar.push_str(&len.to_string());
        cigar.push(ch);
        ops.push(CigarOp {
            op: ch.to_string(),
            length: len,
        });
    }
    if cigar.is_empty() {
        cigar = "*".to_string();
    }
    (ops, cigar)
}

fn cigar_kind_char(kind: CigarKind) -> char {
    match kind {
        CigarKind::Match => 'M',
        CigarKind::Insertion => 'I',
        CigarKind::Deletion => 'D',
        CigarKind::Skip => 'N',
        CigarKind::SoftClip => 'S',
        CigarKind::HardClip => 'H',
        CigarKind::Pad => 'P',
        CigarKind::SequenceMatch => '=',
        CigarKind::SequenceMismatch => 'X',
    }
}

fn qualities_from_record(record: &bam::Record) -> String {
    let scores = record.quality_scores();
    if QualTrait::is_empty(&scores) {
        return String::new();
    }
    let mut out = String::with_capacity(QualTrait::len(&scores));
    for result in QualTrait::iter(&scores) {
        let Ok(q) = result else {
            out.push('5'); // Phred 20 fallback
            continue;
        };
        let phred = q.min(93);
        out.push(char::from(phred + 33));
    }
    out
}

fn sequence_from_record(record: &bam::Record) -> String {
    let seq = record.sequence();
    if seq.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(seq.len());
    for base in seq.iter() {
        out.push(match base {
            b'A' | b'a' => 'A',
            b'C' | b'c' => 'C',
            b'G' | b'g' => 'G',
            b'T' | b't' => 'T',
            b'N' | b'n' => 'N',
            other if other.is_ascii_alphabetic() => (other as char).to_ascii_uppercase(),
            _ => 'N',
        });
    }
    out
}



/// Map a genomic coordinate to a bin index in [0, bins_n] (bins_n = past-the-end).
fn genomic_to_bin(pos: u64, window_start: u64, window_end: u64, bins_n: usize) -> usize {
    if bins_n == 0 || window_end <= window_start {
        return 0;
    }
    if pos <= window_start {
        return 0;
    }
    let span = window_end - window_start;
    if pos >= window_end {
        return bins_n;
    }
    let off = pos - window_start;
    (((off as u128 + 1) * bins_n as u128 - 1) / span as u128) as usize
}

/// Count M/=/X/D spans; skip N (introns).
fn mark_cigar_coverage(
    ops: &[CigarOp],
    aln_start: u64,
    window_start: u64,
    window_end: u64,
    bins_n: usize,
    delta: &mut [f64],
) {
    let mut ref_pos = aln_start;
    for op in ops {
        let len = op.length as u64;
        match op.op.as_str() {
            "M" | "=" | "X" | "D" => {
                mark_read_on_bins(
                    ref_pos,
                    ref_pos.saturating_add(len),
                    window_start,
                    window_end,
                    bins_n,
                    delta,
                );
                ref_pos = ref_pos.saturating_add(len);
            }
            "N" => {
                ref_pos = ref_pos.saturating_add(len);
            }
            _ => {}
        }
    }

}

/// O(1) per aligned block: partial edge bins plus a difference-array range
/// for fully covered bins. Depth is mean aligned bases per base, independent
/// of zoom, read length, or how the CIGAR splits a match into operations.
fn mark_read_on_bins(
    s: u64, e: u64, window_start: u64, window_end: u64,
    bins_n: usize, delta: &mut [f64],
) {
    if window_end <= window_start || e <= window_start || s >= window_end
        || bins_n == 0 || delta.len() < bins_n + 1 {
        return;
    }
    let from = s.max(window_start);
    let to = e.min(window_end);
    if to <= from { return; }
    let span = window_end - window_start;
    let boundary = |i: usize| window_start
        + (i as u128 * span as u128 / bins_n as u128) as u64;
    let first = genomic_to_bin(from, window_start, window_end, bins_n);
    let last = genomic_to_bin(to - 1, window_start, window_end, bins_n);
    let mut add = |i: usize, value: f64| {
        delta[i] += value;
        delta[i + 1] -= value;
    };
    if first == last {
        add(first, (to - from) as f64 / (boundary(first + 1) - boundary(first)) as f64);
    } else {
        add(first, (boundary(first + 1) - from) as f64 / (boundary(first + 1) - boundary(first)) as f64);
        add(last, (to - boundary(last)) as f64 / (boundary(last + 1) - boundary(last)) as f64);
        delta[first + 1] += 1.0;
        delta[last] -= 1.0;
    }
}

/// Prefix-sum delta to mean depth in each non-overlapping genomic bin.
fn finalize_coverage_from_delta(
    contig: &str,
    start: u64,
    end: u64,
    bins_n: usize,
    delta: &[f64],
) -> CoverageWindow {
    let span = (end - start).max(1);
    let mut bins = Vec::with_capacity(bins_n);
    let mut max_depth = 0.0f64;
    let mut run = 0.0f64;
    for i in 0..bins_n {
        run += delta.get(i).copied().unwrap_or(0.0);
        let depth = run.max(0.0);
        max_depth = max_depth.max(depth);
        let b_start = start + (i as u64 * span) / bins_n as u64;
        let b_end = if i + 1 == bins_n {
            end
        } else {
            start + ((i as u64 + 1) * span) / bins_n as u64
        };
        bins.push(CoverageBin {
            start: b_start,
            end: b_end.max(b_start.saturating_add(1)),
            depth,
        });
    }
    CoverageWindow {
        contig: contig.to_string(),
        start,
        end,
        bins,
        max_depth,
    }
}

// CRAM uses bundled samtools for genuinely region-bounded CRAI queries.
// noodles-cram 0.73 Query::read_next_container filters only reference ID,
// decoding every indexed container on that chromosome before filtering records.
// Never put that decoder back on the interactive path without a bounded-query test.

fn resolve_path_contig(
    path: &Path,
    tool_paths: &ToolPaths,
    requested: &str,
    reference_path: Option<&Path>,
) -> Result<String> {
    if is_bam(path) {
        return with_cached_bam(path, |handle| resolve_bam_contig(&handle.header, requested));
    }
    let samtools = resolve_samtools(tool_paths)?;
    let contigs = read_sq_contigs(&samtools, path, reference_path)?;
    resolve_contig_name(contigs.iter().map(|c| c.name.as_str()), requested).ok_or_else(|| {
        anyhow::anyhow!(
            "contig '{requested}' not found in '{}' ({} @SQ sequences).",
            path.display(),
            contigs.len()
        )
    })
}

/// Stream query output while draining diagnostics concurrently. A failed tool
/// must never be reported as a successful empty (or partial) genomic window.
fn stream_samtools_query(mut command: Command, mut visit: impl FnMut(&str) -> bool) -> Result<()> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().context("failed to run samtools view")?;
    let stdout = child.stdout.take().context("missing samtools stdout")?;
    let mut stderr = child.stderr.take().context("missing samtools stderr")?;
    let diagnostics = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.by_ref().take(64 * 1024).read_to_string(&mut text);
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        text
    });
    let mut stopped = false;
    let result: Result<()> = (|| {
        for line in BufReader::with_capacity(256 * 1024, stdout).lines() {
            let line = line.context("failed reading samtools view output")?;
            if !visit(&line) { stopped = true; break; }
        }
        Ok(())
    })();
    if stopped || result.is_err() { let _ = child.kill(); }
    let status = child.wait().context("failed waiting for samtools view");
    let stderr = diagnostics.join().unwrap_or_default();
    result?;
    let status = status?;
    if !stopped && !status.success() {
        bail!("samtools view failed ({status}): {}", stderr.trim());
    }
    Ok(())
}

fn get_coverage_bins_via_samtools_view(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    bins_n: usize,
    reference_path: Option<&Path>,
    options: &ReadQueryOptions,
) -> Result<CoverageWindow> {
    let samtools = resolve_samtools(tool_paths)?;
    ensure_indexed(path, &samtools)?;
    let resolved = resolve_path_contig(path, tool_paths, contig, reference_path)?;

    let region = format_region(&resolved, start, end);
    let mut command = new_command(&samtools);
    command.arg("view").args(["-F", "4"]);
    if let Some(reference) = reference_path {
        if is_cram(path) {
            command.args(["-T", reference.to_str().context("invalid reference path")?]);
        }
    }
    command.arg(path).arg(&region);

    let mut delta = vec![0.0f64; bins_n + 1];
    let mut scanned = 0usize;
    let cap = coverage_scan_cap(end.saturating_sub(start));

    stream_samtools_query(command, |line| {
        if line.starts_with('@') || line.is_empty() {
            return true;
        }
        if scanned >= cap {
            return false;
        }
        let Some(read) = parse_sam_alignment_line(&line) else {
            return true;
        };
        if !passes_filters(&read, options) {
            return true;
        }
        scanned += 1;
        mark_cigar_coverage(&read.cigar_ops, read.start, start, end, bins_n, &mut delta);
        true
    })?;

    Ok(finalize_coverage_from_delta(&resolved, start, end, bins_n, &delta))
}

fn get_reads_samtools(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    reference_path: Option<&Path>,
    options: &ReadQueryOptions,
) -> Result<ReadsWindow> {
    let samtools = resolve_samtools(tool_paths)?;
    ensure_indexed(path, &samtools)?;
    let resolved = resolve_path_contig(path, tool_paths, contig, reference_path)?;

    let region = format_region(&resolved, start, end);
    let mut command = new_command(&samtools);
    command.arg("view");
    command.args(["-F", "4"]);
    if let Some(reference) = reference_path {
        if is_cram(path) {
            command.args(["-T", reference.to_str().context("invalid reference path")?]);
        }
    }
    command.arg(path).arg(&region);

    let max_keep = max_reads_to_keep(end.saturating_sub(start), options.include_sequences);
    let mut reads = Vec::new();
    let mut total = 0u64;
    stream_samtools_query(command, |line| {
        if line.starts_with('@') || line.is_empty() {
            return true;
        }
        let Some(read) = parse_sam_alignment_line(&line) else {
            return true;
        };
        if !passes_filters(&read, options) {
            return true;
        }
        if read.end <= start || read.start >= end {
            return true;
        }
        total += 1;
        reservoir_push(
            &mut reads,
            strip_read_detail(read, options.include_sequences),
            total,
            max_keep,
        );
        true
    })?;

    finish_reads_window(&resolved, start, end, reads, total)
}

fn passes_filters(read: &AlignmentRead, options: &ReadQueryOptions) -> bool {
    if read.is_unmapped {
        return false;
    }
    if !options.include_secondary && read.is_secondary {
        return false;
    }
    if !options.include_supplementary && read.is_supplementary {
        return false;
    }
    if !options.include_duplicates && read.is_duplicate {
        return false;
    }
    if read.mapq < options.min_mapq {
        return false;
    }
    true
}

fn resolve_samtools(tool_paths: &ToolPaths) -> Result<PathBuf> {
    tool_paths
        .resolve_samtools()
        .or_else(|| resolve_samtools_path(&[]))
        .context(
            "samtools is required for alignment viewing. Place samtools.exe in src-tauri/binaries/ or PATH.",
        )
}

fn ensure_indexed(path: &Path, samtools: &Path) -> Result<()> {
    if find_alignment_index(path).is_some() {
        return Ok(());
    }
    samtools_index(samtools, path).with_context(|| {
        format!(
            "failed to index '{}'. Sort the file first if needed.",
            path.display()
        )
    })
}

fn is_bam(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("bam"))
}

fn is_cram(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cram"))
}

fn format_region(contig: &str, start0: u64, end0_exclusive: u64) -> String {
    let start_1 = start0.saturating_add(1);
    let end_1 = end0_exclusive.max(start_1);
    format!("{contig}:{start_1}-{end_1}")
}

fn read_sq_contigs(
    samtools: &Path,
    path: &Path,
    reference_path: Option<&Path>,
) -> Result<Vec<AlignmentContig>> {
    let mut command = new_command(samtools);
    command.arg("view").arg("-H");
    if let Some(reference) = reference_path {
        if is_cram(path) {
            command.args(["-T", reference.to_str().context("invalid reference path")?]);
        }
    }
    command.arg(path);
    let output = command
        .output()
        .context("failed to run samtools view -H")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("samtools view -H failed: {stderr}");
    }

    let mut contigs = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if !line.starts_with("@SQ") {
            continue;
        }
        let mut name = None;
        let mut length = None;
        for field in line.split('\t').skip(1) {
            if let Some(value) = field.strip_prefix("SN:") {
                name = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("LN:") {
                length = value.parse::<u64>().ok();
            }
        }
        if let (Some(name), Some(length)) = (name, length) {
            contigs.push(AlignmentContig { name, length });
        }
    }
    Ok(contigs)
}

fn strip_read_detail(mut read: AlignmentRead, include_sequences: bool) -> AlignmentRead {
    if !include_sequences {
        read.sequence.clear();
        read.qualities.clear();
    }
    // When sequences are requested, keep qualities for IGV-style mismatch shading.
    read
}

fn parse_sam_alignment_line(line: &str) -> Option<AlignmentRead> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 11 {
        return None;
    }
    let name = fields[0].to_string();
    let flags: u16 = fields[1].parse().ok()?;
    let rname = fields[2];
    if rname == "*" {
        return None;
    }
    let pos_1based: u64 = fields[3].parse().ok()?;
    let mapq: u8 = fields[4].parse().unwrap_or(0);
    let cigar = fields[5].to_string();
    let rnext = fields[6];
    let pnext: u64 = fields[7].parse().unwrap_or(0);
    let tlen: i32 = fields[8].parse().unwrap_or(0);
    let sequence = fields[9].to_string();
    let qualities = fields[10].to_string();

    let cigar_ops = parse_cigar_ops(&cigar);
    let start = pos_1based.saturating_sub(1);
    let ref_len = cigar_ops_reference_length(&cigar_ops);
    let end = start.saturating_add(ref_len.max(1));
    let strand = if flags & 0x10 != 0 { "-" } else { "+" }.to_string();

    let mate_contig = if rnext == "=" {
        rname.to_string()
    } else if rnext == "*" {
        String::new()
    } else {
        rnext.to_string()
    };
    let mate_start = if pnext > 0 {
        Some(pnext.saturating_sub(1))
    } else {
        None
    };

    Some(AlignmentRead {
        name,
        start,
        end,
        strand,
        mapq,
        cigar,
        cigar_ops,
        flags,
        sequence: if sequence == "*" {
            String::new()
        } else {
            sequence
        },
        qualities: if qualities == "*" {
            String::new()
        } else {
            qualities
        },
        is_paired: flags & 0x1 != 0,
        is_proper_pair: flags & 0x2 != 0,
        is_unmapped: flags & 0x4 != 0,
        is_mate_unmapped: flags & 0x8 != 0,
        is_reverse: flags & 0x10 != 0,
        is_secondary: flags & 0x100 != 0,
        is_qc_fail: flags & 0x200 != 0,
        is_duplicate: flags & 0x400 != 0,
        is_supplementary: flags & 0x800 != 0,
        is_first_in_pair: flags & 0x40 != 0,
        is_second_in_pair: flags & 0x80 != 0,
        template_length: tlen,
        mate_contig,
        mate_start,
    })
}

fn parse_cigar_ops(cigar: &str) -> Vec<CigarOp> {
    if cigar == "*" || cigar.is_empty() {
        return Vec::new();
    }
    let mut ops = Vec::new();
    let mut num = 0u32;
    for ch in cigar.chars() {
        if ch.is_ascii_digit() {
            num = num.saturating_mul(10).saturating_add((ch as u8 - b'0') as u32);
        } else {
            ops.push(CigarOp {
                op: ch.to_string(),
                length: num,
            });
            num = 0;
        }
    }
    ops
}

fn cigar_ops_reference_length(ops: &[CigarOp]) -> u64 {
    ops.iter()
        .filter(|op| matches!(op.op.as_str(), "M" | "D" | "N" | "=" | "X"))
        .map(|op| op.length as u64)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cram_indexed_region_returns_only_requested_reads_and_depth() {
        let samtools = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/binaries/samtools.exe");
        if !samtools.is_file() { return; }
        let dir = tempfile::tempdir().unwrap();
        let reference = dir.path().join("reference.fa");
        let sam = dir.path().join("reads.sam");
        let cram = dir.path().join("reads.cram");
        std::fs::write(&reference, format!(">chr1\n{}\n>chr2\n{}\n", "A".repeat(1000), "A".repeat(1000))).unwrap();
        std::fs::write(&sam, concat!(
            "@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n@SQ\tSN:chr2\tLN:1000\n",
            "inside\t0\tchr1\t11\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tIIIIIIIIII\n",
            "outside\t0\tchr1\t101\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tIIIIIIIIII\n",
            "other\t0\tchr2\t11\t60\t10M\t*\t0\t0\tAAAAAAAAAA\tIIIIIIIIII\n"
        )).unwrap();
        let mut index_ref = new_command(&samtools);
        index_ref.arg("faidx").arg(&reference);
        crate::external::run_command(index_ref, "samtools faidx").unwrap();
        let mut encode = new_command(&samtools);
        encode.arg("view").args(["-C", "-T"]).arg(&reference).arg("-o").arg(&cram).arg(&sam);
        crate::external::run_command(encode, "samtools encode").unwrap();
        samtools_index(&samtools, &cram).unwrap();
        let tools = ToolPaths { samtools: Some(samtools), minimap2: None };
        let options = ReadQueryOptions { include_sequences: true, ..Default::default() };
        let reads = get_reads_in_range_filtered(&cram, &tools, "1", 10, 20, Some(&reference), &options).unwrap();
        assert_eq!(reads.total_in_range, 1);
        assert_eq!(reads.reads[0].name, "inside");
        assert_eq!(reads.reads[0].sequence, "AAAAAAAAAA");
        let cov = get_coverage_bins_filtered(&cram, &tools, "chr1", 0, 30, 3, Some(&reference), &options).unwrap();
        assert_eq!(cov.bins.iter().map(|b| b.depth).collect::<Vec<_>>(), vec![0.0, 1.0, 0.0]);
        assert!(get_reads_in_range_filtered(&cram, &tools, "missing", 0, 10, Some(&reference), &options).is_err());
        // The process wrapper must propagate decoder failures rather than
        // reporting an empty successful query.
        let mut invalid = new_command(tools.samtools.as_ref().unwrap());
        invalid.arg("view").arg(dir.path().join("missing.cram"));
        let error = stream_samtools_query(invalid, |_| true).unwrap_err();
        assert!(error.to_string().contains("samtools view failed"));
    }

    #[test]
    fn coverage_matches_per_base_oracle_at_all_bin_widths() {
        // Uneven bins, reads contained in one bin, clipped reads and exact
        // boundaries must preserve the same number of aligned bases.
        for span in 1..30u64 {
            for bins_n in 1..=span as usize {
                let mut delta = vec![0.0; bins_n + 1];
                let mut bases = vec![0u32; span as usize];
                for (start, end) in [(0, 3), (4, 7), (6, 11), (8, 50), (12, 12)] {
                    mark_read_on_bins(start, end, 5, 5 + span, bins_n, &mut delta);
                    for pos in start.max(5)..end.min(5 + span) {
                        bases[(pos - 5) as usize] += 1;
                    }
                }
                let cov = finalize_coverage_from_delta("chr1", 5, 5 + span, bins_n, &delta);
                for bin in cov.bins {
                    let sum: u32 = bases[(bin.start - 5) as usize..(bin.end - 5) as usize].iter().sum();
                    let expected = sum as f64 / (bin.end - bin.start) as f64;
                    assert!((bin.depth - expected).abs() < 1e-9,
                        "span={span}, bins={bins_n}, bin={bin:?}, expected={expected}");
                }
            }
        }
    }

    #[test]
    fn coverage_is_independent_of_cigar_segmentation() {
        let mut whole = vec![0.0; 4];
        let mut split = vec![0.0; 4];
        mark_cigar_coverage(&parse_cigar_ops("10M"), 1, 0, 20, 3, &mut whole);
        mark_cigar_coverage(&parse_cigar_ops("3M2I2=1X4M"), 1, 0, 20, 3, &mut split);
        for (a, b) in whole.iter().zip(&split) { assert!((a - b).abs() < 1e-9); }
    }

    #[test]
    fn introns_and_clips_do_not_contribute_depth() {
        let mut delta = vec![0.0; 11];
        mark_cigar_coverage(&parse_cigar_ops("2S2M4N2M2S"), 0, 0, 10, 10, &mut delta);
        mark_cigar_coverage(&parse_cigar_ops("10N"), 0, 0, 10, 10, &mut delta);
        let cov = finalize_coverage_from_delta("chr1", 0, 10, 10, &delta);
        let depths: Vec<_> = cov.bins.iter().map(|b| b.depth).collect();
        assert_eq!(depths, vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn cigar_ref_length_basic() {
        assert_eq!(cigar_ops_reference_length(&parse_cigar_ops("8M")), 8);
        assert_eq!(cigar_ops_reference_length(&parse_cigar_ops("4M2D4M")), 10);
        assert_eq!(cigar_ops_reference_length(&parse_cigar_ops("5S10M3I2M")), 12);
        assert_eq!(cigar_ops_reference_length(&parse_cigar_ops("*")), 0);
    }

    #[test]
    fn format_region_one_based() {
        assert_eq!(format_region("chr1", 0, 10), "chr1:1-10");
        assert_eq!(format_region("chr1", 5, 20), "chr1:6-20");
    }

    #[test]
    fn parse_sam_line() {
        let line = "read1\t0\tchr1\t1\t255\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII";
        let read = parse_sam_alignment_line(line).unwrap();
        assert_eq!(read.name, "read1");
        assert_eq!(read.start, 0);
        assert_eq!(read.end, 8);
        assert_eq!(read.strand, "+");
        assert_eq!(read.sequence, "ACGTACGT");
        assert_eq!(read.cigar_ops.len(), 1);
        assert_eq!(read.cigar_ops[0].op, "M");
        assert_eq!(read.cigar_ops[0].length, 8);
        assert!(!read.is_secondary);
    }

    #[test]
    fn parse_sam_flags_and_softclip() {
        let line = "r2\t16\tchr1\t5\t30\t2S6M1D2M\t=\t20\t30\tNNACGTACGT\t##########";
        let read = parse_sam_alignment_line(line).unwrap();
        assert!(read.is_reverse);
        assert_eq!(read.strand, "-");
        assert_eq!(read.start, 4);
        // 6M + 1D + 2M = 9 ref bases
        assert_eq!(read.end, 13);
        assert_eq!(read.mate_start, Some(19));
        assert_eq!(read.template_length, 30);
    }

    #[test]
    fn difference_array_marks_bins() {
        let mut delta = vec![0.0f64; 3];
        // Read covering [0, 10) with 2 bins over [0, 10) → both bins.
        mark_read_on_bins(0, 10, 0, 10, 2, &mut delta);
        let cov = finalize_coverage_from_delta("chr1", 0, 10, 2, &delta);
        assert_eq!(cov.bins.len(), 2);
        assert!((cov.bins[0].depth - 1.0).abs() < 1e-6);
        assert!((cov.bins[1].depth - 1.0).abs() < 1e-6);
    }

    #[test]
    fn open_sample_bam_native() {
        let bam = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/sample.bam");
        if !bam.is_file() {
            return;
        }
        let tools = ToolPaths {
            samtools: None,
            minimap2: None,
        };
        let doc = open_alignment_document(&bam, &tools, None).expect("open bam");
        assert!(!doc.contigs.is_empty());
        let cov = get_coverage_bins(&bam, &tools, "chr1", 0, 100, 20, None).expect("coverage");
        assert_eq!(cov.bins.len(), 20);
        let reads = get_reads_in_range(&bam, &tools, "chr1", 0, 100, None).expect("reads");
        // sample.bam is tiny; just ensure the path works.
        let _ = reads;
    }

    #[test]
    fn open_real_bam_if_present() {
        let bam = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/test_out/aligned_reads.bam");
        if !bam.is_file() {
            return;
        }
        let tools = ToolPaths {
            samtools: None,
            minimap2: None,
        };
        let doc = open_alignment_document(&bam, &tools, None).expect("open bam");
        assert!(!doc.contigs.is_empty());
        let contig = &doc.contigs[0].name;
        let len = doc.contigs[0].length.min(5_000);
        let cov = get_coverage_bins(&bam, &tools, contig, 0, len, 100, None).expect("coverage");
        assert_eq!(cov.bins.len(), 100);
        let reads = get_reads_in_range_filtered(
            &bam,
            &tools,
            contig,
            0,
            len.min(2_000),
            None,
            &ReadQueryOptions {
                include_secondary: false,
                include_supplementary: false,
                include_duplicates: false,
                min_mapq: 0,
                include_sequences: false,
            },
        )
        .expect("reads");
        assert!(!reads.reads.is_empty() || reads.total_in_range == 0);
    }
}
