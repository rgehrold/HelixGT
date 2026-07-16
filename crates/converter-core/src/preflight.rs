use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cancel::CancelToken;
use crate::format::{infer_format, FileFormat};
use crate::tools::ToolPaths;

#[derive(Debug, Clone)]
pub struct PreflightIssue {
    pub severity: PreflightSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct PreflightReport {
    pub ok: bool,
    pub issues: Vec<PreflightIssue>,
    pub estimated_output_bytes: u64,
}

pub struct PreflightRequest {
    pub mode: PreflightMode,
    pub input_paths: Vec<PathBuf>,
    pub output_dir: PathBuf,
    pub output_format: Option<FileFormat>,
    pub reference_path: Option<PathBuf>,
    pub tool_paths: ToolPaths,
    pub needs_samtools: bool,
    pub needs_minimap2: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightMode {
    Convert,
    Merge,
    Align,
}

pub fn run_preflight(request: &PreflightRequest) -> Result<PreflightReport> {
    let mut issues = Vec::new();
    let mut estimated = 0u64;

    if request.input_paths.is_empty() {
        issues.push(issue(
            PreflightSeverity::Error,
            "Select at least one input file.",
        ));
    }

    for path in &request.input_paths {
        if !path.is_file() {
            issues.push(issue(
                PreflightSeverity::Error,
                format!("Input file not found: {}", path.display()),
            ));
            continue;
        }
        if let Ok(meta) = std::fs::metadata(path) {
            estimated = estimated.saturating_add(meta.len());
        }
        if infer_format(path).is_err() {
            issues.push(issue(
                PreflightSeverity::Error,
                format!("Unsupported or unknown format: {}", path.display()),
            ));
        }
    }

    if request.output_dir.as_os_str().is_empty() {
        issues.push(issue(
            PreflightSeverity::Error,
            "Choose an output folder.",
        ));
    } else if !request.output_dir.exists() {
        issues.push(issue(
            PreflightSeverity::Warning,
            format!(
                "Output folder does not exist yet; it will be created: {}",
                request.output_dir.display()
            ),
        ));
    }

    if request.needs_samtools && request.tool_paths.resolve_samtools().is_none() {
        issues.push(issue(
            PreflightSeverity::Error,
            "samtools is required but was not found (bundle it or add to PATH).",
        ));
    }

    if request.needs_minimap2 && request.tool_paths.resolve_minimap2().is_none() {
        issues.push(issue(
            PreflightSeverity::Error,
            "minimap2 is required but was not found (bundle it or add to PATH).",
        ));
    }

    if let Some(format) = request.output_format {
        if matches!(format, FileFormat::Cram) {
            match &request.reference_path {
                Some(path) if path.is_file() => {}
                _ => issues.push(issue(
                    PreflightSeverity::Error,
                    "CRAM conversion requires a reference FASTA.",
                )),
            }
        }
    }

    if request.mode == PreflightMode::Merge && request.input_paths.len() < 2 {
        issues.push(issue(
            PreflightSeverity::Error,
            "Merge requires at least two input files.",
        ));
    }

    if request.mode == PreflightMode::Align {
        // Frontend passes a reference cache id (e.g. "escherichia_coli_k12" or
        // "local:C:\path\to\ref.fa"), not always a filesystem path. The align
        // command resolves ids via the reference store; only require a non-empty value here.
        let has_reference = request
            .reference_path
            .as_ref()
            .is_some_and(|path| !path.as_os_str().is_empty());
        if !has_reference {
            issues.push(issue(
                PreflightSeverity::Error,
                "Alignment requires a loaded reference FASTA.",
            ));
        }
    }

    let ok = !issues.iter().any(|item| item.severity == PreflightSeverity::Error);
    Ok(PreflightReport {
        ok,
        issues,
        estimated_output_bytes: estimated,
    })
}

fn issue(severity: PreflightSeverity, message: impl Into<String>) -> PreflightIssue {
    PreflightIssue {
        severity,
        message: message.into(),
    }
}

pub fn check_cancel(token: Option<&CancelToken>) -> Result<()> {
    if let Some(token) = token {
        token.check()?;
    }
    Ok(())
}

pub fn available_disk_bytes(_path: &Path) -> Option<u64> {
    None
}