use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use anyhow::{Context, Result};

use crate::tools::ToolPaths;

pub trait ToolLogSink: Send + Sync {
    fn log_line(&self, tool: &str, stream: &str, line: &str);
}

pub fn index_path_for(alignment_path: &Path) -> PathBuf {
    let path = alignment_path.to_string_lossy();
    if path.ends_with(".bam") {
        PathBuf::from(format!("{path}.bai"))
    } else if path.ends_with(".cram") {
        PathBuf::from(format!("{path}.crai"))
    } else {
        alignment_path.with_extension("bai")
    }
}

pub fn resolve_tool_path(tool_name: &str, extra_paths: &[PathBuf]) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = extra_paths.to_vec();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(tool_name));
            candidates.push(dir.join("binaries").join(tool_name));
            candidates.push(dir.join("resources").join(tool_name));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri").join("binaries").join(tool_name));
        candidates.push(cwd.join("binaries").join(tool_name));
    }

    candidates.push(PathBuf::from(tool_name));

    for candidate in candidates {
        if tool_runs(&candidate) {
            return Some(candidate);
        }
    }
    None
}

pub fn resolve_minimap2_path(extra_paths: &[PathBuf]) -> Option<PathBuf> {
    let name = if cfg!(windows) { "minimap2.exe" } else { "minimap2" };
    resolve_tool_path(name, extra_paths)
}

pub fn resolve_samtools_path(extra_paths: &[PathBuf]) -> Option<PathBuf> {
    let name = if cfg!(windows) { "samtools.exe" } else { "samtools" };
    resolve_tool_path(name, extra_paths)
}

pub fn minimap2_available() -> bool {
    resolve_minimap2_path(&[]).is_some()
}

pub fn samtools_available() -> bool {
    resolve_samtools_path(&[]).is_some()
}

pub fn new_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    hide_console_window(&mut command);
    command
}

fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn tool_runs(path: &Path) -> bool {
    new_command(path)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn run_command(command: Command, tool: &str) -> Result<()> {
    run_command_logged(command, tool, None)
}

pub fn run_command_logged(
    command: Command,
    tool: &str,
    log: Option<&dyn ToolLogSink>,
) -> Result<()> {
    let mut command = command;
    hide_console_window(&mut command);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run {tool}"))?;

    stream_process_output(child.stdout.take(), tool, "stdout", log);
    stream_process_output(child.stderr.take(), tool, "stderr", log);

    let status = child.wait().with_context(|| format!("failed while waiting for {tool}"))?;
    if !status.success() {
        anyhow::bail!("{tool} failed with exit code {:?}", status.code());
    }
    Ok(())
}

fn stream_process_output(
    stream: Option<impl Read + Send + 'static>,
    tool: &str,
    stream_name: &str,
    log: Option<&dyn ToolLogSink>,
) {
    let Some(stream) = stream else { return };
    let Some(log) = log else { return };

    let tool_name = tool.to_string();
    let stream_label = stream_name.to_string();
    let (tx, rx) = mpsc::channel::<String>();

    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });

    for line in rx {
        log.log_line(&tool_name, &stream_label, &line);
    }
}

pub fn samtools_view_count(samtools: &Path, alignment_path: &Path, extra_args: &[&str]) -> Result<u64> {
    let path_str = alignment_path
        .to_str()
        .context("alignment path is not valid UTF-8")?;
    let mut command = new_command(samtools);
    command.arg("view").arg("-c");
    command.args(extra_args);
    command.arg(path_str);
    let output = command
        .output()
        .context("failed to run samtools view -c")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("samtools view -c failed: {stderr}");
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .context("could not parse samtools view -c output")
}

pub fn samtools_flagstat(samtools: &Path, alignment_path: &Path) -> Result<(u64, u64)> {
    let total = samtools_view_count(samtools, alignment_path, &[])?;
    let mapped = samtools_view_count(samtools, alignment_path, &["-F", "4"])?;
    Ok((mapped, total))
}

pub fn guess_reference_for_cram(cram_path: &Path) -> Option<PathBuf> {
    let parent = cram_path.parent()?;
    let stem = cram_path.file_stem()?.to_str()?;

    for ext in [
        "fasta", "fa", "fna", "fasta.gz", "fa.gz", "fna.gz", "ffn", "ffn.gz",
    ] {
        let candidate = parent.join(format!("{stem}.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    for name in [
        "reference.fasta",
        "reference.fa",
        "reference.fna",
        "reference.fasta.gz",
        "ref.fasta",
        "genome.fasta",
        "genome.fa",
    ] {
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

pub fn resolve_cram_reference(cram_path: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        anyhow::bail!("reference file '{}' does not exist", path.display());
    }
    guess_reference_for_cram(cram_path).ok_or_else(|| {
        anyhow::anyhow!(
            "CRAM decoding requires the reference FASTA used during encoding. \
             Choose a reference file or place a matching FASTA (e.g. {}.fasta) next to the CRAM.",
            cram_path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("reference")
        )
    })
}

pub fn ensure_cram_index(samtools: &Path, cram_path: &Path, reference: &Path) -> Result<()> {
    let index_path = index_path_for(cram_path);
    if index_path.is_file() {
        return Ok(());
    }
    let cram = cram_path.to_str().context("invalid CRAM path")?;
    let reference_str = reference.to_str().context("invalid reference path")?;
    let mut command = new_command(samtools);
    command.args(["index", "-X", "cram", "-T", reference_str, cram]);
    let _ = run_command(command, "samtools index");
    Ok(())
}

pub fn samtools_view_from_cram(
    samtools: &Path,
    input_path: &Path,
    output_path: &Path,
    reference_path: &Path,
    write_bam: bool,
) -> Result<u64> {
    ensure_cram_index(samtools, input_path, reference_path)?;

    let input = input_path.to_str().context("invalid CRAM input path")?;
    let output = output_path.to_str().context("invalid output path")?;
    let reference = reference_path.to_str().context("invalid reference path")?;

    let mut command = new_command(samtools);
    command.args(["view", "-h", "-T", reference]);
    if write_bam {
        command.arg("-b");
    }
    command.args(["-o", output, input]);
    run_command(command, "samtools view")?;

    samtools_view_count(samtools, output_path, &[])
}

pub fn samtools_index(samtools: &Path, alignment_path: &Path) -> Result<()> {
    let path_str = alignment_path
        .to_str()
        .context("alignment path is not valid UTF-8")?;
    let mut command = new_command(samtools);
    command.args(["index", path_str]);
    run_command(command, "samtools index")
}

pub fn samtools_view_to_cram(
    samtools: &Path,
    input_path: &Path,
    output_path: &Path,
    reference_path: &Path,
) -> Result<u64> {
    let input = input_path.to_str().context("invalid input path")?;
    let output = output_path.to_str().context("invalid output path")?;
    let reference = reference_path.to_str().context("invalid reference path")?;

    let mut command = new_command(samtools);
    command.args([
        "view",
        "-C",
        "-T",
        reference,
        "-o",
        output,
        input,
    ]);
    run_command(command, "samtools view")?;
    samtools_index(samtools, output_path)?;

    samtools_flagstat(samtools, output_path).map(|(_, total)| total)
}

pub fn index_alignment_output_if_needed(output_path: &Path, tool_paths: &ToolPaths) -> Result<()> {
    let extension = output_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension != "bam" && extension != "cram" {
        return Ok(());
    }
    let samtools = tool_paths.resolve_samtools().context(
        "samtools is required to index BAM/CRAM output. Place samtools.exe in src-tauri/binaries/ or add it to PATH.",
    )?;
    samtools_index(&samtools, output_path)
}

pub fn samtools_merge(
    samtools: &Path,
    output_path: &Path,
    input_paths: &[PathBuf],
    reference_path: Option<&Path>,
) -> Result<()> {
    if input_paths.is_empty() {
        anyhow::bail!("samtools merge requires at least one input file");
    }

    let output = output_path
        .to_str()
        .context("merge output path is not valid UTF-8")?;
    let mut command = new_command(samtools);
    command.arg("merge").arg("-f").arg("-o").arg(output);
    if let Some(reference) = reference_path {
        if output_path.extension().and_then(|value| value.to_str()) == Some("cram") {
            command.args(["-T", reference.to_str().context("invalid reference path")?]);
        }
    }
    for input in input_paths {
        command.arg(input);
    }
    run_command(command, "samtools merge")
}

pub fn samtools_faidx(samtools: &Path, reference_path: &Path) -> Result<()> {
    let index_path = reference_path.with_extension("fai");
    if index_path.is_file() {
        return Ok(());
    }
    let reference = reference_path
        .to_str()
        .context("reference path is not valid UTF-8")?;
    let mut command = new_command(samtools);
    command.args(["faidx", reference]);
    run_command(command, "samtools faidx")
}