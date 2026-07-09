mod alignment;
mod annotation;
mod genbank;
mod sequence;
mod variants;

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

pub(crate) enum TextWriter {
    Plain(File),
    Gzip(GzEncoder<File>),
}

impl Write for TextWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(file) => file.write(buf),
            Self::Gzip(encoder) => encoder.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(file) => file.flush(),
            Self::Gzip(encoder) => encoder.flush(),
        }
    }
}

use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::cancel::CancelToken;
use crate::external::index_alignment_output_if_needed;
use crate::format::{infer_format, is_gzipped, output_filename, FileFormat};
use crate::preflight::check_cancel;
use crate::tools::{default_thread_count, helixgt_temp_file, ToolPaths};

pub struct ConvertOptions {
    pub output_format: FileFormat,
    pub compress: bool,
    pub prefix: String,
    pub reference_path: Option<PathBuf>,
    pub tool_paths: ToolPaths,
    pub cancel: Option<CancelToken>,
    pub thread_count: Option<usize>,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            output_format: FileFormat::Fasta,
            compress: false,
            prefix: String::new(),
            reference_path: None,
            tool_paths: ToolPaths::default(),
            cancel: None,
            thread_count: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConvertedFile {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub records: u64,
}

#[derive(Debug, Clone)]
pub struct BatchConvertFailure {
    pub input_path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct BatchConvertResult {
    pub files: Vec<ConvertedFile>,
    pub failures: Vec<BatchConvertFailure>,
}

pub fn convert_file(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let input_format = infer_format(input_path)
        .with_context(|| format!("cannot infer input format for '{}'", input_path.display()))?;
    let output_format = options.output_format;

    if input_format == output_format {
        return transcode_same_format(input_path, output_path, input_format, options);
    }

    if input_format.is_sequence() && output_format.is_sequence() {
        return sequence::convert_sequence(input_path, output_path, input_format, options)
            .with_context(|| format!("sequence conversion ({input_format} → {output_format})"));
    }

    if input_format.is_alignment() && output_format.is_alignment() {
        return alignment::convert_alignment(input_path, output_path, input_format, output_format, options)
            .with_context(|| format!("alignment conversion ({input_format} → {output_format})"));
    }

    if input_format.is_alignment() && output_format.is_sequence() {
        return alignment::to_sequence(input_path, output_path, input_format, output_format, options)
            .with_context(|| format!("alignment to sequence ({input_format} → {output_format})"));
    }

    if input_format.is_sequence() && output_format.is_alignment() {
        return alignment::from_sequence(input_path, output_path, input_format, output_format, options)
            .with_context(|| format!("sequence to alignment ({input_format} → {output_format})"));
    }

    if input_format == FileFormat::GenBank {
        return genbank::convert_genbank(
            input_path,
            output_path,
            input_format,
            output_format,
            options,
        )
        .with_context(|| format!("genbank conversion (genbank → {output_format})"));
    }

    if output_format == FileFormat::GenBank && input_format.is_sequence() {
        return genbank::sequence_to_genbank(input_path, output_path, input_format, options)
            .with_context(|| format!("sequence to genbank ({input_format} → genbank)"));
    }

    if input_format == FileFormat::Vcf && output_format == FileFormat::Vcf {
        return variants::convert_vcf(input_path, output_path, options)
            .with_context(|| "vcf transcode");
    }

    if input_format == FileFormat::Gff && output_format == FileFormat::Gff {
        return sequence::copy_gff(input_path, output_path, options).with_context(|| "gff transcode");
    }

    if input_format == FileFormat::Bed && output_format == FileFormat::Bed {
        return sequence::copy_bed(input_path, output_path, options).with_context(|| "bed transcode");
    }

    if matches!(
        (input_format, output_format),
        (FileFormat::Gff, FileFormat::Bed) | (FileFormat::Bed, FileFormat::Gff)
    ) {
        return annotation::convert_annotation(input_path, output_path, input_format, output_format, options)
            .with_context(|| format!("annotation conversion ({input_format} → {output_format})"));
    }

    // Permissive bridges through FASTA.
    if output_format == FileFormat::Fasta {
        let intermediate = bridge_through_fasta(input_path, input_format, options)?;
        return sequence::copy_fasta(&intermediate, output_path, options)
            .with_context(|| format!("writing bridged fasta ({input_format} → fasta)"))
            .and_then(|count| {
                let _ = std::fs::remove_file(&intermediate);
                Ok(count)
            });
    }

    if output_format == FileFormat::Fastq {
        let intermediate = bridge_through_fasta(input_path, input_format, options)?;
        let count = sequence::convert_sequence(
            &intermediate,
            output_path,
            FileFormat::Fasta,
            &ConvertOptions {
                output_format: FileFormat::Fastq,
                compress: options.compress,
                tool_paths: options.tool_paths.clone(),
                cancel: options.cancel.clone(),
                thread_count: options.thread_count,
                ..Default::default()
            },
        )
        .with_context(|| format!("writing bridged fastq ({input_format} → fastq)"))?;
        let _ = std::fs::remove_file(intermediate);
        return Ok(count);
    }

    anyhow::bail!(
        "no conversion path from {input_format} to {output_format}.\n\
         Try an intermediate format (e.g. alignment → fasta, fasta → fastq, sequence → sam)."
    );
}

fn transcode_same_format(
    input_path: &Path,
    output_path: &Path,
    format: FileFormat,
    options: &ConvertOptions,
) -> Result<u64> {
    match format {
        FileFormat::Fasta => sequence::copy_fasta(input_path, output_path, options),
        FileFormat::Fastq => sequence::copy_fastq(input_path, output_path, options),
        FileFormat::Gff => sequence::copy_gff(input_path, output_path, options),
        FileFormat::Bed => sequence::copy_bed(input_path, output_path, options),
        FileFormat::Sam => alignment::transcode_sam(input_path, output_path, options),
        FileFormat::Bam => alignment::copy_bam(input_path, output_path),
        FileFormat::Cram => alignment::copy_cram(input_path, output_path, options),
        FileFormat::Vcf => variants::convert_vcf(input_path, output_path, options),
        FileFormat::GenBank => genbank::copy_genbank(input_path, output_path, options),
    }
    .with_context(|| {
        format!(
            "transcoding {} → {} (input gzip: {}, output gzip: {})",
            format,
            format,
            is_gzipped(input_path),
            options.compress && format.is_textual()
        )
    })
}

fn bridge_through_fasta(
    input_path: &Path,
    input_format: FileFormat,
    _options: &ConvertOptions,
) -> Result<PathBuf> {
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("bridge");
    let _ = crate::tools::ensure_temp_dir();
    let temp_path = helixgt_temp_file(stem, "fasta");

    if input_format.is_alignment() {
        alignment::to_sequence(
            input_path,
            &temp_path,
            input_format,
            FileFormat::Fasta,
            &ConvertOptions {
                output_format: FileFormat::Fasta,
                compress: false,
                tool_paths: _options.tool_paths.clone(),
                cancel: _options.cancel.clone(),
                thread_count: _options.thread_count,
                ..Default::default()
            },
        )?;
    } else if input_format == FileFormat::GenBank {
        genbank::convert_genbank(
            input_path,
            &temp_path,
            FileFormat::GenBank,
            FileFormat::Fasta,
            &ConvertOptions {
                output_format: FileFormat::Fasta,
                compress: false,
                tool_paths: _options.tool_paths.clone(),
                cancel: _options.cancel.clone(),
                thread_count: _options.thread_count,
                ..Default::default()
            },
        )?;
    } else {
        anyhow::bail!("cannot bridge {input_format} to fasta");
    }

    Ok(temp_path)
}

pub fn batch_convert(
    input_paths: &[PathBuf],
    output_dir: &Path,
    options: &ConvertOptions,
    on_progress: impl Fn(usize, usize, &str) + Sync,
) -> Result<BatchConvertResult> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("cannot create output directory '{}'", output_dir.display()))?;

    let total = input_paths.len();
    check_cancel(options.cancel.as_ref())?;

    let thread_count = options.thread_count.unwrap_or_else(default_thread_count);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(thread_count)
        .build()
        .context("failed to build conversion thread pool")?;

    let completed = AtomicUsize::new(0);
    let errors: std::sync::Mutex<Vec<BatchConvertFailure>> = std::sync::Mutex::new(Vec::new());

    let mut results: Vec<ConvertedFile> = pool.install(|| {
        input_paths
            .par_iter()
            .filter_map(|input_path| {
                if let Err(error) = check_cancel(options.cancel.as_ref()) {
                    if let Ok(mut failures) = errors.lock() {
                        failures.push(BatchConvertFailure {
                            input_path: input_path.clone(),
                            message: format!("{error:#}"),
                        });
                    }
                    return None;
                }

                let file_name = input_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");

                let mut out_name =
                    output_filename(input_path, options.output_format, options.compress);
                if !options.prefix.is_empty() {
                    let prefix = options.prefix.trim_end_matches('_');
                    out_name = format!("{prefix}_{out_name}");
                }

                let output_path = output_dir.join(out_name);
                match convert_file(input_path, &output_path, options) {
                    Ok(records) => {
                        if matches!(options.output_format, FileFormat::Bam | FileFormat::Cram) {
                            if let Err(error) = index_alignment_output_if_needed(
                                &output_path,
                                &options.tool_paths,
                            ) {
                                if let Ok(mut failures) = errors.lock() {
                                    failures.push(BatchConvertFailure {
                                        input_path: input_path.clone(),
                                        message: format!("{error:#}"),
                                    });
                                }
                                return None;
                            }
                        }
                        let current = completed.fetch_add(1, Ordering::SeqCst) + 1;
                        on_progress(current, total, file_name);
                        Some(ConvertedFile {
                            input_path: input_path.clone(),
                            output_path,
                            records,
                        })
                    }
                    Err(error) => {
                        if let Ok(mut failures) = errors.lock() {
                            failures.push(BatchConvertFailure {
                                input_path: input_path.clone(),
                                message: format!("{error:#}"),
                            });
                        }
                        None
                    }
                }
            })
            .collect()
    });

    let failures = errors.into_inner().unwrap_or_default();
    if results.is_empty() && !failures.is_empty() {
        let summary = failures
            .iter()
            .map(|failure| format!("{}: {}", failure.input_path.display(), failure.message))
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!("batch conversion failed:\n{summary}");
    }

    results.sort_by(|left, right| left.input_path.cmp(&right.input_path));
    Ok(BatchConvertResult { files: results, failures })
}

pub(crate) fn open_text_reader(path: &Path) -> Result<Box<dyn Read>> {
    let file = File::open(path)
        .with_context(|| format!("cannot open '{}' for reading", path.display()))?;
    if is_gzipped(path) {
        Ok(Box::new(MultiGzDecoder::new(file)))
    } else {
        Ok(Box::new(file))
    }
}

pub(crate) fn open_text_writer(path: &Path, compress: bool) -> Result<TextWriter> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(path)
        .with_context(|| format!("cannot open '{}' for writing", path.display()))?;
    if compress {
        Ok(TextWriter::Gzip(GzEncoder::new(file, Compression::default())))
    } else {
        Ok(TextWriter::Plain(file))
    }
}

const READER_BUFFER_CAPACITY: usize = 1024 * 1024;

pub(crate) fn open_buf_reader(path: &Path) -> Result<BufReader<Box<dyn Read>>> {
    Ok(BufReader::with_capacity(
        READER_BUFFER_CAPACITY,
        open_text_reader(path)?,
    ))
}

pub(crate) fn open_buf_writer(path: &Path, compress: bool) -> Result<BufWriter<TextWriter>> {
    Ok(BufWriter::new(open_text_writer(path, compress)?))
}

pub(crate) fn finish_gzip_writer(mut writer: BufWriter<TextWriter>) -> Result<()> {
    writer
        .flush()
        .context("failed to flush output buffer")?;
    match writer
        .into_inner()
        .map_err(|_| anyhow::anyhow!("failed to finalize output buffer"))?
    {
        TextWriter::Gzip(encoder) => {
            encoder
                .finish()
                .map_err(|error| anyhow::anyhow!("failed to finish gzip file: {error}"))?;
        }
        TextWriter::Plain(mut file) => {
            file.flush().context("failed to flush output file")?;
        }
    }
    Ok(())
}

pub(crate) fn flush_dyn_writer(mut writer: Box<dyn Write>) -> Result<()> {
    writer
        .flush()
        .context("failed to flush output stream")?;
    Ok(())
}