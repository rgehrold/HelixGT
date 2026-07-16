use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use flate2::read::MultiGzDecoder;

use noodles::fasta;
use noodles::fastq;
use noodles::sam;



use noodles::bam;
use noodles::cram;

use crate::cancel::CancelToken;
use crate::convert::{finish_gzip_writer, open_buf_reader, open_buf_writer, open_text_writer};
use crate::format::{infer_format, is_gzipped, FileFormat};
use crate::preflight::check_cancel;
use crate::tools::ToolPaths;

#[derive(Debug, Clone)]
pub struct MergeValidation {
    pub extension: String,
    pub format: Option<FileFormat>,
    pub is_valid: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct MergeSuggestion {
    pub suggested_name: String,
    pub common_prefix: String,
    pub common_suffix: String,
    pub similar: bool,
}

#[derive(Debug, Clone)]
pub struct MergeOptions {
    pub output_name: String,
    pub compress: Option<bool>,
    pub tool_paths: ToolPaths,
    pub reference_path: Option<PathBuf>,
    pub cancel: Option<CancelToken>,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub output_path: PathBuf,
    pub records: u64,
    pub input_count: usize,
}

pub fn validate_merge_inputs(paths: &[PathBuf]) -> MergeValidation {
    if paths.is_empty() {
        return MergeValidation {
            extension: String::new(),
            format: None,
            is_valid: false,
            message: "Select at least two files to merge.".into(),
        };
    }

    if paths.len() < 2 {
        return MergeValidation {
            extension: canonical_extension(paths[0].as_path()),
            format: infer_format(paths[0].as_path()).ok(),
            is_valid: false,
            message: "Select at least two files to merge.".into(),
        };
    }

    let first_ext = canonical_extension(paths[0].as_path());
    if paths
        .iter()
        .skip(1)
        .any(|path| canonical_extension(path.as_path()) != first_ext)
    {
        return MergeValidation {
            extension: first_ext,
            format: infer_format(paths[0].as_path()).ok(),
            is_valid: false,
            message: "All files must share the same extension (including .gz).".into(),
        };
    }

    let format = infer_format(paths[0].as_path()).ok();
    MergeValidation {
        extension: first_ext.clone(),
        format,
        is_valid: true,
        message: format!("Ready to merge {} {} file(s).", paths.len(), first_ext),
    }
}

pub fn suggest_merge_filename(paths: &[PathBuf]) -> MergeSuggestion {
    if paths.is_empty() {
        return MergeSuggestion {
            suggested_name: "merged.fasta".into(),
            common_prefix: String::new(),
            common_suffix: String::new(),
            similar: false,
        };
    }

    let extension = canonical_extension(paths[0].as_path());
    let stems: Vec<String> = paths
        .iter()
        .map(|path| stem_without_extension(path.as_path()))
        .collect();
    let tokenized: Vec<TokenizedStem> = stems.iter().map(|stem| tokenize_stem(stem)).collect();

    let prefix_count = common_token_prefix_len(&tokenized);
    let suffix_count = common_token_suffix_len(&tokenized);
    let min_tokens = tokenized
        .iter()
        .map(|stem| stem.tokens.len())
        .min()
        .unwrap_or(0);
    let suffix_count = if prefix_count + suffix_count > min_tokens {
        min_tokens.saturating_sub(prefix_count)
    } else {
        suffix_count
    };

    let prefix = extract_token_region(&stems[0], &tokenized[0], 0, prefix_count);
    let suffix = if suffix_count > 0 {
        extract_token_region(
            &stems[0],
            &tokenized[0],
            tokenized[0].tokens.len() - suffix_count,
            tokenized[0].tokens.len(),
        )
    } else {
        String::new()
    };

    let similar = prefix_count > 0 || suffix_count >= 2;

    let body = if prefix.is_empty() && suffix.is_empty() {
        format!("merged_{}", paths.len())
    } else if prefix.is_empty() {
        format!("merged_{suffix}")
    } else if suffix.is_empty() {
        format!("{prefix}_merged")
    } else if prefix_count + suffix_count >= min_tokens {
        format!("{prefix}_merged")
    } else {
        format!("{prefix}_merged_{suffix}")
    };

    let suggested_name = format!("{body}{extension}");
    MergeSuggestion {
        suggested_name,
        common_prefix: prefix,
        common_suffix: suffix,
        similar,
    }
}

pub fn merge_files(
    input_paths: &[PathBuf],
    output_dir: &Path,
    options: &MergeOptions,
    mut on_progress: Option<&mut dyn FnMut(u64, &str)>,
) -> Result<MergeResult> {
    let validation = validate_merge_inputs(input_paths);
    if !validation.is_valid {
        anyhow::bail!("{}", validation.message);
    }

    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("cannot create output directory '{}'", output_dir.display()))?;

    let output_path = output_dir.join(&options.output_name);
    let format = infer_format(input_paths[0].as_path())
        .with_context(|| format!("cannot infer format for '{}'", input_paths[0].display()))?;

    let compress = options.compress.unwrap_or_else(|| {
        input_paths.iter().all(|path| is_gzipped(path.as_path()))
    });

    check_cancel(options.cancel.as_ref())?;

    let records = match format {
        FileFormat::Fasta => {
            merge_fasta(input_paths, &output_path, compress, options, &mut on_progress)?
        }
        FileFormat::Fastq => {
            merge_fastq(input_paths, &output_path, compress, options, &mut on_progress)?
        }
        FileFormat::Sam => {
            merge_sam(input_paths, &output_path, compress, options, &mut on_progress)?
        }
        FileFormat::Bam => merge_bam(input_paths, &output_path, options, &mut on_progress)?,
        FileFormat::Cram => merge_cram(input_paths, &output_path, options, &mut on_progress)?,
        FileFormat::Gff | FileFormat::Bed | FileFormat::Vcf | FileFormat::GenBank => {
            merge_text_lines(input_paths, &output_path, compress, format, options, &mut on_progress)?
        }
    };

    Ok(MergeResult {
        output_path,
        records,
        input_count: input_paths.len(),
    })
}

fn emit_progress(
    on_progress: &mut Option<&mut dyn FnMut(u64, &str)>,
    count: u64,
    message: &str,
) {
    if let Some(callback) = on_progress.as_mut() {
        callback(count, message);
    }
}

fn merge_fasta(
    paths: &[PathBuf],
    output_path: &Path,
    compress: bool,
    options: &MergeOptions,
    on_progress: &mut Option<&mut dyn FnMut(u64, &str)>,
) -> Result<u64> {
    let mut writer = fasta::io::writer::Builder::default()
        .build_from_writer(open_buf_writer(output_path, compress)?);
    let mut count = 0u64;

    for input_path in paths {
        check_cancel(options.cancel.as_ref())?;
        let mut reader = fasta::io::reader::Builder::default()
            .build_from_reader(open_buf_reader(input_path)?)
            .with_context(|| format!("failed to open FASTA '{}'", input_path.display()))?;
        for (index, result) in reader.records().enumerate() {
            check_cancel(options.cancel.as_ref())?;
            let record = result.with_context(|| {
                format!(
                    "invalid FASTA record #{} in '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            writer.write_record(&record).with_context(|| {
                format!(
                    "failed to write FASTA record #{} from '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            count += 1;
            if count % 1000 == 0 {
                emit_progress(
                    on_progress,
                    count,
                    &format!("merged {count} FASTA records"),
                );
            }
        }
    }

    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

fn merge_fastq(
    paths: &[PathBuf],
    output_path: &Path,
    compress: bool,
    options: &MergeOptions,
    on_progress: &mut Option<&mut dyn FnMut(u64, &str)>,
) -> Result<u64> {
    let mut writer = fastq::io::Writer::new(open_buf_writer(output_path, compress)?);
    let mut count = 0u64;

    for input_path in paths {
        check_cancel(options.cancel.as_ref())?;
        let mut reader = fastq::io::Reader::new(open_buf_reader(input_path)?);
        for (index, result) in reader.records().enumerate() {
            check_cancel(options.cancel.as_ref())?;
            let record = result.with_context(|| {
                format!(
                    "invalid FASTQ record #{} in '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            writer.write_record(&record).with_context(|| {
                format!(
                    "failed to write FASTQ record #{} from '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            count += 1;
            if count % 1000 == 0 {
                emit_progress(
                    on_progress,
                    count,
                    &format!("merged {count} FASTQ records"),
                );
            }
        }
    }

    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

fn merge_sam(
    paths: &[PathBuf],
    output_path: &Path,
    compress: bool,
    options: &MergeOptions,
    on_progress: &mut Option<&mut dyn FnMut(u64, &str)>,
) -> Result<u64> {
    let mut writer = open_sam_writer(output_path, compress)?;
    let mut count = 0u64;

    for (file_index, input_path) in paths.iter().enumerate() {
        check_cancel(options.cancel.as_ref())?;
        let mut reader = sam::io::reader::Builder::default()
            .build_from_path(input_path)
            .with_context(|| format!("failed to open SAM '{}'", input_path.display()))?;
        let header = reader
            .read_header()
            .with_context(|| format!("failed to read SAM header in '{}'", input_path.display()))?;

        if file_index == 0 {
            writer
                .write_header(&header)
                .context("failed to write merged SAM header")?;
        }

        for (index, result) in reader.records().enumerate() {
            check_cancel(options.cancel.as_ref())?;
            let record = result.with_context(|| {
                format!(
                    "invalid SAM record #{} in '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            writer.write_record(&header, &record).with_context(|| {
                format!(
                    "failed to write SAM record #{} from '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            count += 1;
            if count % 1000 == 0 {
                emit_progress(on_progress, count, &format!("merged {count} SAM records"));
            }
        }
    }

    crate::convert::flush_dyn_writer(writer.into_inner())?;
    Ok(count)
}

fn merge_bam(
    paths: &[PathBuf],
    output_path: &Path,
    options: &MergeOptions,
    on_progress: &mut Option<&mut dyn FnMut(u64, &str)>,
) -> Result<u64> {
    let out_file = File::create(output_path)
        .with_context(|| format!("failed to create BAM '{}'", output_path.display()))?;
    let mut writer = bam::io::Writer::new(out_file);
    let mut count = 0u64;

    for (file_index, input_path) in paths.iter().enumerate() {
        check_cancel(options.cancel.as_ref())?;
        let file = File::open(input_path)
            .with_context(|| format!("failed to open BAM '{}'", input_path.display()))?;
        let mut reader = bam::io::Reader::new(file);
        let header = reader
            .read_header()
            .with_context(|| format!("failed to read BAM header in '{}'", input_path.display()))?;

        if file_index == 0 {
            writer
                .write_header(&header)
                .context("failed to write merged BAM header")?;
        }

        for (index, result) in reader.records().enumerate() {
            check_cancel(options.cancel.as_ref())?;
            let record = result.with_context(|| {
                format!(
                    "invalid BAM record #{} in '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            writer.write_record(&header, &record).with_context(|| {
                format!(
                    "failed to write BAM record #{} from '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            count += 1;
            if count % 1000 == 0 {
                emit_progress(on_progress, count, &format!("merged {count} BAM records"));
            }
        }
    }

    emit_progress(on_progress, count, &format!("merged {count} BAM records"));
    Ok(count)
}

fn merge_cram(
    paths: &[PathBuf],
    output_path: &Path,
    options: &MergeOptions,
    on_progress: &mut Option<&mut dyn FnMut(u64, &str)>,
) -> Result<u64> {
    let out_file = File::create(output_path)
        .with_context(|| format!("failed to create CRAM '{}'", output_path.display()))?;
    let mut writer = cram::io::Writer::new(out_file);
    let mut count = 0u64;

    for (file_index, input_path) in paths.iter().enumerate() {
        check_cancel(options.cancel.as_ref())?;
        let file = File::open(input_path)
            .with_context(|| format!("failed to open CRAM '{}'", input_path.display()))?;
        let mut reader = cram::io::Reader::new(file);
        let header = reader
            .read_header()
            .with_context(|| format!("failed to read CRAM header in '{}'", input_path.display()))?;

        if file_index == 0 {
            writer
                .write_header(&header)
                .context("failed to write merged CRAM header")?;
        }

        for (index, result) in reader.records(&header).enumerate() {
            check_cancel(options.cancel.as_ref())?;
            let record = result.with_context(|| {
                format!(
                    "invalid CRAM record #{} in '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            writer.write_record(&header, record).with_context(|| {
                format!(
                    "failed to write CRAM record #{} from '{}'",
                    index + 1,
                    input_path.display()
                )
            })?;
            count += 1;
            if count % 1000 == 0 {
                emit_progress(on_progress, count, &format!("merged {count} CRAM records"));
            }
        }
    }

    emit_progress(on_progress, count, &format!("merged {count} CRAM records"));
    Ok(count)
}

fn merge_text_lines(
    paths: &[PathBuf],
    output_path: &Path,
    compress: bool,
    format: FileFormat,
    options: &MergeOptions,
    on_progress: &mut Option<&mut dyn FnMut(u64, &str)>,
) -> Result<u64> {
    let mut writer = BufWriter::new(open_text_writer(output_path, compress)?);
    let mut count = 0u64;

    for (file_index, input_path) in paths.iter().enumerate() {
        check_cancel(options.cancel.as_ref())?;
        let reader = open_line_reader(input_path)?;
        let mut saw_header = false;

        for (line_index, line) in reader.lines().enumerate() {
            check_cancel(options.cancel.as_ref())?;
            let line = line.with_context(|| {
                format!(
                    "failed to read line #{} in '{}'",
                    line_index + 1,
                    input_path.display()
                )
            })?;

            if file_index > 0 && should_skip_merged_line(format, &line, &mut saw_header) {
                continue;
            }

            if format == FileFormat::Vcf && line.starts_with("#CHROM") {
                saw_header = true;
            }

            writeln!(writer, "{line}")
                .with_context(|| format!("failed to write merged line to '{}'", output_path.display()))?;
            if !line.starts_with('#') || format == FileFormat::GenBank {
                count += 1;
            }
            if count % 1000 == 0 {
                emit_progress(on_progress, count, &format!("merged {count} records"));
            }
        }
    }

    finish_gzip_writer(writer)?;
    Ok(count)
}

fn should_skip_merged_line(format: FileFormat, line: &str, saw_header: &mut bool) -> bool {
    match format {
        FileFormat::Vcf => line.starts_with("##") || line.starts_with("#CHROM"),
        FileFormat::Gff | FileFormat::Bed => line.starts_with('#'),
        FileFormat::GenBank => {
            if line.starts_with("LOCUS") {
                false
            } else {
                false
            }
        }
        _ => {
            let _ = saw_header;
            false
        }
    }
}

fn open_line_reader(path: &Path) -> Result<BufReader<Box<dyn Read>>> {
    let file = File::open(path)
        .with_context(|| format!("cannot open '{}' for reading", path.display()))?;
    let reader: Box<dyn Read> = if is_gzipped(path) {
        Box::new(MultiGzDecoder::new(file))
    } else {
        Box::new(file)
    };
    Ok(BufReader::new(reader))
}

type SamWriter = sam::io::Writer<Box<dyn Write>>;

fn open_sam_writer(path: &Path, compress: bool) -> Result<SamWriter> {
    let file = File::create(path)
        .with_context(|| format!("failed to create SAM '{}'", path.display()))?;
    let mut builder = sam::io::writer::Builder::default();
    if compress {
        builder = builder.set_compression_method(sam::io::CompressionMethod::Bgzf);
    } else {
        builder = builder.set_compression_method(sam::io::CompressionMethod::None);
    }
    Ok(builder.build_from_writer(file))
}

pub fn canonical_extension(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if file_name.ends_with(".gz") {
        let without_gz = file_name.strip_suffix(".gz").unwrap_or(&file_name);
        if let Some(dot) = without_gz.rfind('.') {
            return format!("{}{}", &without_gz[dot..], ".gz");
        }
        return ".gz".to_string();
    }

    if let Some(dot) = file_name.rfind('.') {
        file_name[dot..].to_string()
    } else {
        String::new()
    }
}

fn stem_without_extension(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let mut name = file_name.to_string();
    if name.to_ascii_lowercase().ends_with(".gz") {
        name.truncate(name.len() - 3);
    }
    if let Some(dot) = name.rfind('.') {
        name.truncate(dot);
    }
    name
}

#[derive(Debug, Clone)]
struct TokenizedStem {
    tokens: Vec<String>,
    tokens_lower: Vec<String>,
    boundaries: Vec<(usize, usize)>,
}

fn is_delimiter(ch: char) -> bool {
    matches!(ch, '_' | '-' | '.')
}

fn tokenize_stem(stem: &str) -> TokenizedStem {
    let mut tokens = Vec::new();
    let mut tokens_lower = Vec::new();
    let mut boundaries = Vec::new();
    let mut start = 0;
    let mut in_token = false;

    for (index, ch) in stem.char_indices() {
        if is_delimiter(ch) {
            if in_token {
                boundaries.push((start, index));
                let token = &stem[start..index];
                tokens.push(token.to_string());
                tokens_lower.push(token.to_ascii_lowercase());
                in_token = false;
            }
        } else if !in_token {
            start = index;
            in_token = true;
        }
    }

    if in_token {
        boundaries.push((start, stem.len()));
        let token = &stem[start..];
        tokens.push(token.to_string());
        tokens_lower.push(token.to_ascii_lowercase());
    }

    TokenizedStem {
        tokens,
        tokens_lower,
        boundaries,
    }
}

fn common_token_prefix_len(tokenized: &[TokenizedStem]) -> usize {
    if tokenized.is_empty() {
        return 0;
    }
    let min_len = tokenized
        .iter()
        .map(|stem| stem.tokens.len())
        .min()
        .unwrap_or(0);
    let mut count = 0;
    for index in 0..min_len {
        let expected = &tokenized[0].tokens_lower[index];
        if tokenized
            .iter()
            .all(|stem| stem.tokens_lower.get(index) == Some(expected))
        {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn common_token_suffix_len(tokenized: &[TokenizedStem]) -> usize {
    if tokenized.is_empty() {
        return 0;
    }
    let mut count = 0;
    loop {
        let mut expected: Option<&str> = None;
        for stem in tokenized {
            if count >= stem.tokens.len() {
                return count;
            }
            let index = stem.tokens.len() - 1 - count;
            let token = &stem.tokens_lower[index];
            match expected {
                None => expected = Some(token.as_str()),
                Some(value) if value == token.as_str() => {}
                _ => return count,
            }
        }
        count += 1;
    }
}

fn extract_token_region(
    _stem: &str,
    tokenized: &TokenizedStem,
    start_token: usize,
    end_token: usize,
) -> String {
    if start_token >= end_token || start_token >= tokenized.boundaries.len() {
        return String::new();
    }
    let end_index = end_token.min(tokenized.boundaries.len());
    let (start, _) = tokenized.boundaries[start_token];
    let (_, end) = tokenized.boundaries[end_index - 1];
    _stem[start..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_common_suffix_for_similar_names() {
        let paths = vec![
            PathBuf::from("fghj_PSPS6-BamHI-MBN.plus.rawreads.fasta.gz"),
            PathBuf::from("fghjzzzz_PSPS6-BamHI-MBN.plus.rawreads.fasta.gz"),
        ];
        let suggestion = suggest_merge_filename(&paths);
        assert!(suggestion.similar);
        assert!(
            suggestion
                .suggested_name
                .to_ascii_lowercase()
                .contains("psps6-bamhi-mbn.plus.rawreads")
        );
        assert!(suggestion.suggested_name.ends_with(".fasta.gz"));
    }

    #[test]
    fn tokenizes_case_insensitive_prefix_and_suffix() {
        let paths = vec![
            PathBuf::from("Sample_PSPS6-BamHI-MBN.plus.rawreads.fasta.gz"),
            PathBuf::from("sample2_PSPS6-BamHI-MBN.plus.rawreads.fasta.gz"),
        ];
        let suggestion = suggest_merge_filename(&paths);
        assert!(suggestion.similar);
        assert!(suggestion.common_suffix.contains("PSPS6"));
        assert!(suggestion.suggested_name.ends_with(".fasta.gz"));
    }

    #[test]
    fn rejects_mismatched_extensions() {
        let paths = vec![
            PathBuf::from("a.fasta"),
            PathBuf::from("b.fastq"),
        ];
        let validation = validate_merge_inputs(&paths);
        assert!(!validation.is_valid);
    }
}