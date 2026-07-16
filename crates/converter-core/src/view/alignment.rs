use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use noodles::bam;
use noodles::core::{Position, Region};
use noodles::sam::alignment::record::cigar::op::Kind as CigarKind;
use noodles::sam::alignment::Record as AlignmentRecord;
use noodles::sam::Header as SamHeader;
use serde::Serialize;

use crate::external::{index_path_for, new_command, resolve_samtools_path, samtools_index};
use crate::format::{infer_format, FileFormat};
use crate::tools::ToolPaths;

/// Max reads returned for a pileup window (keeps IPC + paint responsive).
pub const MAX_READS_PER_WINDOW: usize = 8_000;

/// Default number of coverage bins if client does not specify.
pub const DEFAULT_COVERAGE_BINS: usize = 200;

/// Hard cap on how many alignments we scan for coverage in one request.
/// Prevents pathological stalls on ultra-deep WGS regions while still filling bins.
const MAX_COVERAGE_SCAN_READS: usize = 80_000;

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
    pub cigar: String,
    pub cigar_ops: Vec<CigarOp>,
    pub flags: u16,
    pub sequence: String,
    /// ASCII Phred+33 qualities when present, else empty.
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
    pub mate_contig: String,
    /// 0-based mate start when available, else None.
    pub mate_start: Option<u64>,
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

    let mut indexed = index_path_for(path).is_file();
    if !indexed {
        let samtools = resolve_samtools(tool_paths)?;
        match samtools_index(&samtools, path) {
            Ok(()) => indexed = index_path_for(path).is_file(),
            Err(error) => {
                bail!(
                    "alignment is not indexed and automatic indexing failed ({error:#}). \
                     Sort and index the BAM/CRAM (samtools sort | index)."
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

pub fn get_coverage_bins(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    bin_count: usize,
    reference_path: Option<&Path>,
) -> Result<CoverageWindow> {
    if start >= end {
        bail!("invalid window: start ({start}) must be < end ({end})");
    }
    let bins_n = bin_count.clamp(8, 2_000);

    if is_bam(path) {
        return get_coverage_bins_bam(path, contig, start, end, bins_n);
    }

    // CRAM (and rare non-BAM): stream samtools view and bin from CIGAR spans.
    // Avoids `samtools depth -a` which emits one line per base.
    get_coverage_bins_via_samtools_view(
        path,
        tool_paths,
        contig,
        start,
        end,
        bins_n,
        reference_path,
    )
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
            include_sequences: true,
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

// ── Native BAM (noodles + BAI) ──────────────────────────────────────────────

fn get_coverage_bins_bam(
    path: &Path,
    contig: &str,
    start: u64,
    end: u64,
    bins_n: usize,
) -> Result<CoverageWindow> {
    let mut reader = open_indexed_bam(path)?;
    let header = reader
        .read_header()
        .with_context(|| format!("failed to read BAM header: {}", path.display()))?;
    let region = make_region(contig, start, end)?;

    let span = (end - start) as f64;
    let bin_width = span / bins_n as f64;
    let mut depths = vec![0.0f64; bins_n];
    let mut scanned = 0usize;

    let query = reader
        .query(&header, &region)
        .with_context(|| format!("BAM query failed for {contig}:{start}-{end}"))?;

    for result in query {
        let record = result.context("invalid BAM record during coverage scan")?;
        if scanned >= MAX_COVERAGE_SCAN_READS {
            break;
        }
        let flags = record.flags();
        if flags.is_unmapped() {
            continue;
        }
        scanned += 1;

        let Some(Ok(aln_start)) = record.alignment_start() else {
            continue;
        };
        let ref_start0 = (usize::from(aln_start) as u64).saturating_sub(1);
        // Span-based depth is O(1) per read and matches IGV closely for DNA-seq.
        // (CIGAR walk was far too expensive on deep windows and froze the UI thread
        // when commands ran synchronously.)
        let span = AlignmentRecord::alignment_span(&record)
            .and_then(|r| r.ok())
            .unwrap_or(1)
            .max(1) as u64;
        add_depth_range(
            ref_start0,
            ref_start0.saturating_add(span),
            start,
            end,
            bin_width,
            bins_n,
            &mut depths,
        );
    }

    Ok(finalize_coverage_bins(contig, start, end, bins_n, bin_width, depths))
}

fn get_reads_bam(
    path: &Path,
    contig: &str,
    start: u64,
    end: u64,
    options: &ReadQueryOptions,
) -> Result<ReadsWindow> {
    let mut reader = open_indexed_bam(path)?;
    let header = reader
        .read_header()
        .with_context(|| format!("failed to read BAM header: {}", path.display()))?;
    let region = make_region(contig, start, end)?;

    let mut reads = Vec::with_capacity(1024.min(MAX_READS_PER_WINDOW));
    let mut total = 0u64;

    let query = reader
        .query(&header, &region)
        .with_context(|| format!("BAM query failed for {contig}:{start}-{end}"))?;

    for result in query {
        let record = result.context("invalid BAM record during read fetch")?;
        let Some(read) = record_to_alignment_read(&record, &header, options.include_sequences) else {
            continue;
        };
        if !passes_filters(&read, options) {
            continue;
        }
        if read.end <= start || read.start >= end {
            continue;
        }
        total += 1;
        if reads.len() < MAX_READS_PER_WINDOW {
            reads.push(read);
        }
        // Keep counting a bit past the cap so total_in_range is useful, then stop.
        if total as usize > MAX_READS_PER_WINDOW.saturating_mul(4) {
            break;
        }
    }

    reads.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.name.cmp(&b.name)));
    let truncated = total as usize > reads.len();
    Ok(ReadsWindow {
        contig: contig.to_string(),
        start,
        end,
        reads,
        truncated,
        total_in_range: total,
    })
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

    let (cigar_ops, cigar_str) = cigar_from_record(record);
    let name = record
        .name()
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_else(|| "*".to_string());

    let sequence = if include_sequences {
        sequence_from_record(record)
    } else {
        String::new()
    };
    // Qualities are only used in the inspector mean-Q display; skip bulk transfer.
    let qualities = String::new();

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

fn add_depth_range(
    s: u64,
    e: u64,
    window_start: u64,
    window_end: u64,
    bin_width: f64,
    bins_n: usize,
    depths: &mut [f64],
) {
    if e <= window_start || s >= window_end || bins_n == 0 || bin_width <= 0.0 {
        return;
    }
    let from = s.max(window_start);
    let to = e.min(window_end);
    if to <= from {
        return;
    }
    // Always step by bin (never per-base). O(bins touched) per read.
    let mut p = from;
    while p < to {
        let mut idx = ((p - window_start) as f64 / bin_width).floor() as usize;
        if idx >= bins_n {
            idx = bins_n - 1;
        }
        let bin_end = window_start + (((idx + 1) as f64) * bin_width).floor() as u64;
        let next = to.min(bin_end.max(p + 1));
        depths[idx] += (next - p) as f64;
        p = next;
    }
}

fn finalize_coverage_bins(
    contig: &str,
    start: u64,
    end: u64,
    bins_n: usize,
    bin_width: f64,
    depths: Vec<f64>,
) -> CoverageWindow {
    let mut bins = Vec::with_capacity(bins_n);
    let mut max_depth = 0.0f64;
    for i in 0..bins_n {
        let b_start = start + ((i as f64) * bin_width).floor() as u64;
        let b_end = if i + 1 == bins_n {
            end
        } else {
            start + (((i + 1) as f64) * bin_width).floor() as u64
        };
        let width = (b_end.saturating_sub(b_start)).max(1) as f64;
        // depths holds sum of per-base counts; average across bin.
        let depth = depths[i] / width;
        max_depth = max_depth.max(depth);
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

// ── CRAM / samtools fallback ────────────────────────────────────────────────

fn get_coverage_bins_via_samtools_view(
    path: &Path,
    tool_paths: &ToolPaths,
    contig: &str,
    start: u64,
    end: u64,
    bins_n: usize,
    reference_path: Option<&Path>,
) -> Result<CoverageWindow> {
    let samtools = resolve_samtools(tool_paths)?;
    ensure_indexed(path, &samtools)?;

    let region = format_region(contig, start, end);
    let mut command = new_command(&samtools);
    command.arg("view").args(["-F", "4"]);
    if let Some(reference) = reference_path {
        if is_cram(path) {
            command.args(["-T", reference.to_str().context("invalid reference path")?]);
        }
    }
    command.arg(path).arg(&region);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn().context("failed to run samtools view")?;
    let stdout = child.stdout.take().context("missing samtools stdout")?;
    let reader = BufReader::with_capacity(256 * 1024, stdout);

    let span = (end - start) as f64;
    let bin_width = span / bins_n as f64;
    let mut depths = vec![0.0f64; bins_n];
    let mut scanned = 0usize;

    for line in reader.lines() {
        let line = line.context("failed reading samtools view output")?;
        if line.starts_with('@') || line.is_empty() {
            continue;
        }
        if scanned >= MAX_COVERAGE_SCAN_READS {
            break;
        }
        let Some(parsed) = parse_sam_light(&line) else {
            continue;
        };
        scanned += 1;
        accumulate_coverage_from_ops(
            &parsed.ops,
            parsed.start,
            start,
            end,
            bin_width,
            bins_n,
            &mut depths,
        );
    }

    // Drain / kill child if we broke early.
    let _ = child.kill();
    let _ = child.wait();

    Ok(finalize_coverage_bins(contig, start, end, bins_n, bin_width, depths))
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

    let region = format_region(contig, start, end);
    let mut command = new_command(&samtools);
    command.arg("view");
    command.args(["-F", "4"]);
    if let Some(reference) = reference_path {
        if is_cram(path) {
            command.args(["-T", reference.to_str().context("invalid reference path")?]);
        }
    }
    command.arg(path).arg(&region);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn().context("failed to run samtools view")?;
    let stdout = child.stdout.take().context("missing samtools stdout")?;
    let reader = BufReader::with_capacity(256 * 1024, stdout);

    let mut reads = Vec::new();
    let mut total = 0u64;
    for line in reader.lines() {
        let line = line.context("failed reading samtools view output")?;
        if line.starts_with('@') || line.is_empty() {
            continue;
        }
        let Some(read) = parse_sam_alignment_line(&line) else {
            continue;
        };
        if !passes_filters(&read, options) {
            continue;
        }
        if read.end <= start || read.start >= end {
            continue;
        }
        total += 1;
        if reads.len() < MAX_READS_PER_WINDOW {
            reads.push(strip_read_detail(read, options.include_sequences));
        } else if total as usize > MAX_READS_PER_WINDOW.saturating_mul(4) {
            break;
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    reads.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.name.cmp(&b.name)));
    let truncated = total as usize > reads.len();
    Ok(ReadsWindow {
        contig: contig.to_string(),
        start,
        end,
        reads,
        truncated,
        total_in_range: total,
    })
}

struct LightSam {
    start: u64,
    ops: Vec<CigarOp>,
}

fn parse_sam_light(line: &str) -> Option<LightSam> {
    let mut fields = line.splitn(12, '\t');
    let _qname = fields.next()?;
    let _flag = fields.next()?;
    let rname = fields.next()?;
    if rname == "*" {
        return None;
    }
    let pos_1based: u64 = fields.next()?.parse().ok()?;
    let _mapq = fields.next()?;
    let cigar = fields.next()?;
    if cigar == "*" {
        return None;
    }
    let ops = parse_cigar_ops(cigar);
    Some(LightSam {
        start: pos_1based.saturating_sub(1),
        ops,
    })
}

fn accumulate_coverage_from_ops(
    ops: &[CigarOp],
    mut ref_pos: u64,
    window_start: u64,
    window_end: u64,
    bin_width: f64,
    bins_n: usize,
    depths: &mut [f64],
) {
    for op in ops {
        let len = op.length as u64;
        match op.op.as_str() {
            "M" | "D" | "=" | "X" => {
                add_depth_range(
                    ref_pos,
                    ref_pos + len,
                    window_start,
                    window_end,
                    bin_width,
                    bins_n,
                    depths,
                );
                ref_pos += len;
            }
            "N" => {
                ref_pos += len;
            }
            _ => {}
        }
    }
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
    if index_path_for(path).is_file() {
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
    } else {
        // Still drop qualities to shrink IPC; inspector can live without mean Q.
        read.qualities.clear();
    }
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
    fn accumulate_ops_into_bins() {
        let ops = parse_cigar_ops("10M");
        let mut depths = vec![0.0; 2];
        accumulate_coverage_from_ops(&ops, 0, 0, 10, 5.0, 2, &mut depths);
        assert!((depths[0] - 5.0).abs() < 1e-6);
        assert!((depths[1] - 5.0).abs() < 1e-6);
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
