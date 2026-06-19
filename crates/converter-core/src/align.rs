use std::fmt;
use std::fs::File;
use std::io::{copy, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;

use std::sync::Arc;

use crate::external::{
    index_path_for, new_command, run_command, run_command_logged, samtools_flagstat, samtools_index,
    ToolLogSink,
};

#[derive(Clone)]
pub struct AlignOptions {
    pub output_format: AlignOutputFormat,
    pub compress: bool,
    pub minimap2_exe: PathBuf,
    pub minimap2: Minimap2Options,
    pub samtools_exe: PathBuf,
    pub tool_log: Option<Arc<dyn ToolLogSink + Send + Sync>>,
}

#[derive(Debug, Clone, Default)]
pub struct Minimap2Options {
    pub preset: Minimap2Preset,
    pub index_reference: bool,
    pub sort_output: bool,
    pub secondary_alignments: bool,
    pub index_output: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Minimap2Preset {
    #[default]
    General,
    ShortReads,
    Ont,
    Hifi,
}

impl Minimap2Preset {
    pub fn from_str(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "sr" | "short" | "short_reads" => Self::ShortReads,
            "ont" | "map-ont" | "nanopore" => Self::Ont,
            "hifi" | "map-hifi" | "pacbio" => Self::Hifi,
            _ => Self::General,
        }
    }

    fn flag(&self) -> Option<&'static str> {
        match self {
            Self::General => None,
            Self::ShortReads => Some("sr"),
            Self::Ont => Some("map-ont"),
            Self::Hifi => Some("map-hifi"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignOutputFormat {
    Sam,
    Bam,
    Cram,
}

impl AlignOutputFormat {
    pub fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "sam" => Ok(Self::Sam),
            "bam" => Ok(Self::Bam),
            "cram" => Ok(Self::Cram),
            _ => anyhow::bail!("unsupported alignment output format '{value}'"),
        }
    }
}

impl fmt::Display for AlignOutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sam => f.write_str("SAM"),
            Self::Bam => f.write_str("BAM"),
            Self::Cram => f.write_str("CRAM"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlignResult {
    pub output_path: PathBuf,
    pub index_path: Option<PathBuf>,
    pub mapped_reads: u64,
    pub total_reads: u64,
    pub aligner: String,
}

pub fn align_reads_to_reference(
    read_paths: &[PathBuf],
    reference_path: &Path,
    output_path: &Path,
    options: &AlignOptions,
) -> Result<AlignResult> {
    if read_paths.is_empty() {
        anyhow::bail!("select at least one read file");
    }

    if !reference_path.is_file() {
        anyhow::bail!(
            "reference file not found at '{}'",
            reference_path.display()
        );
    }

    align_with_minimap2(read_paths, reference_path, output_path, options)
}

fn align_with_minimap2(
    read_paths: &[PathBuf],
    reference_path: &Path,
    output_path: &Path,
    options: &AlignOptions,
) -> Result<AlignResult> {
    let temp_dir = std::env::temp_dir().join(format!("ugt_align_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).context("failed to create temporary alignment directory")?;

    let reference_target = if options.minimap2.index_reference {
        let index_path = temp_dir.join("reference.mmi");
        let mut index_cmd = new_command(&options.minimap2_exe);
        index_cmd.arg("-d").arg(&index_path).arg(reference_path);
        run_logged(index_cmd, "minimap2 index", options.tool_log.clone())?;
        index_path
    } else {
        reference_path.to_path_buf()
    };

    let samtools = &options.samtools_exe;
    let reference_str = reference_path
        .to_str()
        .context("reference path is not valid UTF-8")?;

    let view_writes_bam = options.minimap2.sort_output || options.output_format != AlignOutputFormat::Sam;
    let view_output = if view_writes_bam {
        temp_dir.join("aligned.unsorted.bam")
    } else {
        temp_dir.join("aligned.sam")
    };

    pipe_minimap2_to_samtools_view(
        &options.minimap2_exe,
        samtools,
        reference_str,
        &reference_target,
        read_paths,
        &options.minimap2,
        &view_output,
        view_writes_bam,
        options.tool_log.clone(),
    )?;

    let working_output = if options.minimap2.sort_output {
        let sorted_path = match options.output_format {
            AlignOutputFormat::Sam => temp_dir.join("aligned.sorted.sam"),
            AlignOutputFormat::Bam | AlignOutputFormat::Cram => temp_dir.join("aligned.sorted.bam"),
        };
        sort_alignment(
            samtools,
            reference_str,
            &view_output,
            &sorted_path,
            options.output_format,
            options.tool_log.clone(),
        )?;
        sorted_path
    } else {
        view_output
    };

    let final_output = match options.output_format {
        AlignOutputFormat::Sam => {
            if options.compress {
                gzip_file(&working_output, output_path)?;
            } else {
                std::fs::copy(&working_output, output_path).with_context(|| {
                    format!("failed to copy SAM to '{}'", output_path.display())
                })?;
            }
            output_path.to_path_buf()
        }
        AlignOutputFormat::Bam => {
            std::fs::copy(&working_output, output_path).with_context(|| {
                format!("failed to copy BAM to '{}'", output_path.display())
            })?;
            output_path.to_path_buf()
        }
        AlignOutputFormat::Cram => {
            let mut cram_cmd = new_command(samtools);
            cram_cmd.args([
                "view",
                "-C",
                "-T",
                reference_str,
                "-o",
                output_path.to_str().context("invalid output path")?,
                working_output.to_str().context("invalid temp path")?,
            ]);
            run_logged(cram_cmd, "samtools view", options.tool_log.clone())?;
            output_path.to_path_buf()
        }
    };

    let index_path = if options.minimap2.index_output
        && matches!(
            options.output_format,
            AlignOutputFormat::Bam | AlignOutputFormat::Cram
        )
    {
        samtools_index(samtools, &final_output)?;
        Some(index_path_for(&final_output))
    } else {
        None
    };

    let (mapped, total) = samtools_flagstat(samtools, &final_output)?;
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(AlignResult {
        output_path: final_output,
        index_path,
        mapped_reads: mapped,
        total_reads: total,
        aligner: describe_aligner(options),
    })
}

fn run_logged(
    command: Command,
    tool: &str,
    log: Option<Arc<dyn ToolLogSink + Send + Sync>>,
) -> Result<()> {
    if let Some(sink) = log.as_deref() {
        run_command_logged(command, tool, Some(sink))
    } else {
        run_command(command, tool)
    }
}

fn describe_aligner(options: &AlignOptions) -> String {
    let mut parts = vec!["minimap2".to_string(), "samtools".to_string()];
    if options.minimap2.index_reference {
        parts.push("indexed".into());
    }
    if options.minimap2.sort_output {
        parts.push("sorted".into());
    }
    parts.push(options.output_format.to_string().to_lowercase());
    parts.join(" + ")
}

fn pipe_minimap2_to_samtools_view(
    minimap2_exe: &Path,
    samtools_exe: &Path,
    reference_fasta: &str,
    reference_target: &Path,
    read_paths: &[PathBuf],
    minimap2_options: &Minimap2Options,
    output_path: &Path,
    write_bam: bool,
    log: Option<Arc<dyn ToolLogSink + Send + Sync>>,
) -> Result<()> {
    let output_str = output_path
        .to_str()
        .context("output path is not valid UTF-8")?;

    let mut minimap2 = new_command(minimap2_exe);
    minimap2.arg("-a");
    if let Some(preset) = minimap2_options.preset.flag() {
        minimap2.arg("-x").arg(preset);
    }
    if !minimap2_options.secondary_alignments {
        minimap2.arg("-N").arg("0");
    }
    minimap2.arg(reference_target);
    for read_path in read_paths {
        minimap2.arg(read_path);
    }
    minimap2.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut minimap2_child = minimap2
        .spawn()
        .context("failed to spawn minimap2 (is it installed and on PATH?)")?;

    let minimap2_stdout = minimap2_child
        .stdout
        .take()
        .context("failed to capture minimap2 stdout")?;

    let mut view = new_command(samtools_exe);
    view.arg("view").args(["-h", "-T", reference_fasta]);
    if write_bam {
        view.args(["-b", "-o", output_str, "-"]);
    } else {
        view.args(["-o", output_str, "-"]);
    }
    view.stdin(Stdio::from(minimap2_stdout))
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut view_child = view.spawn().context("failed to spawn samtools view")?;
    log_stderr_stream(minimap2_child.stderr.take(), "minimap2", log.clone());
    log_stderr_stream(view_child.stderr.take(), "samtools view", log);

    let view_status = view_child
        .wait()
        .context("failed while waiting for samtools view")?;
    let minimap2_status = minimap2_child
        .wait()
        .context("failed while waiting for minimap2")?;

    if !minimap2_status.success() {
        let stderr = minimap2_child
            .stderr
            .as_mut()
            .map(read_stream_to_string)
            .transpose()?
            .unwrap_or_default();
        anyhow::bail!("minimap2 alignment failed: {stderr}");
    }
    if !view_status.success() {
        let stderr = view_child
            .stderr
            .as_mut()
            .map(read_stream_to_string)
            .transpose()?
            .unwrap_or_default();
        anyhow::bail!("samtools view failed: {stderr}");
    }

    Ok(())
}

fn sort_alignment(
    samtools_exe: &Path,
    reference_fasta: &str,
    input_path: &Path,
    output_path: &Path,
    output_format: AlignOutputFormat,
    log: Option<Arc<dyn ToolLogSink + Send + Sync>>,
) -> Result<()> {
    let input = input_path.to_str().context("invalid sort input path")?;
    let output = output_path.to_str().context("invalid sort output path")?;
    let mut command = new_command(samtools_exe);
    command.arg("sort");
    match output_format {
        AlignOutputFormat::Sam => {
            command.args(["-O", "sam"]);
        }
        AlignOutputFormat::Bam | AlignOutputFormat::Cram => {
            command.args(["-O", "bam"]);
        }
    }
    if matches!(output_format, AlignOutputFormat::Cram) {
        command.args(["-T", reference_fasta]);
    }
    command.args(["-o", output, input]);
    run_logged(command, "samtools sort", log)
}

fn log_stderr_stream(
    stream: Option<impl Read + Send + 'static>,
    tool: &str,
    log: Option<Arc<dyn ToolLogSink + Send + Sync>>,
) {
    let Some(log) = log else { return };
    let Some(stream) = stream else { return };
    let tool_name = tool.to_string();
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            log.log_line(&tool_name, "stderr", &line);
        }
    });
}

fn gzip_file(input_path: &Path, output_path: &Path) -> Result<()> {
    let input = File::open(input_path)
        .with_context(|| format!("failed to open '{}' for gzip", input_path.display()))?;
    let output = File::create(output_path)
        .with_context(|| format!("failed to create '{}'", output_path.display()))?;
    let mut encoder = GzEncoder::new(output, Compression::default());
    copy(&mut BufReader::new(input), &mut encoder)
        .with_context(|| format!("failed to gzip '{}'", input_path.display()))?;
    encoder
        .finish()
        .map_err(|error| anyhow::anyhow!("failed to finalize gzip output: {error}"))?;
    Ok(())
}

fn read_stream_to_string(stream: &mut impl Read) -> Result<String> {
    let mut buffer = String::new();
    stream
        .read_to_string(&mut buffer)
        .context("failed to read process stderr")?;
    Ok(buffer)
}

pub fn output_alignment_path(
    output_dir: &Path,
    stem: &str,
    format: AlignOutputFormat,
    compress: bool,
) -> PathBuf {
    let extension = match format {
        AlignOutputFormat::Sam if compress => "sam.gz",
        AlignOutputFormat::Sam => "sam",
        AlignOutputFormat::Bam => "bam",
        AlignOutputFormat::Cram => "cram",
    };
    output_dir.join(format!("{stem}.{extension}"))
}