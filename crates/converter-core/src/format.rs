use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileFormat {
    Fasta,
    Fastq,
    GenBank,
    Gff,
    Bed,
    Sam,
    Bam,
    Cram,
    Vcf,
}

#[derive(Debug, Error)]
#[error("unsupported file format for '{0}'")]
pub struct UnknownFormatError(pub String);

impl FileFormat {
    pub const ALL: [FileFormat; 9] = [
        FileFormat::Fasta,
        FileFormat::Fastq,
        FileFormat::GenBank,
        FileFormat::Gff,
        FileFormat::Bed,
        FileFormat::Sam,
        FileFormat::Bam,
        FileFormat::Cram,
        FileFormat::Vcf,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fasta => "fasta",
            Self::Fastq => "fastq",
            Self::GenBank => "genbank",
            Self::Gff => "gff",
            Self::Bed => "bed",
            Self::Sam => "sam",
            Self::Bam => "bam",
            Self::Cram => "cram",
            Self::Vcf => "vcf",
        }
    }

    pub fn default_extension(self) -> &'static str {
        match self {
            Self::Fasta => ".fasta",
            Self::Fastq => ".fastq",
            Self::GenBank => ".gbk",
            Self::Gff => ".gff",
            Self::Bed => ".bed",
            Self::Sam => ".sam",
            Self::Bam => ".bam",
            Self::Cram => ".cram",
            Self::Vcf => ".vcf",
        }
    }

    pub fn is_textual(self) -> bool {
        matches!(
            self,
            Self::Fasta | Self::Fastq | Self::GenBank | Self::Gff | Self::Bed | Self::Sam | Self::Vcf
        )
    }

    pub fn is_alignment(self) -> bool {
        matches!(self, Self::Sam | Self::Bam | Self::Cram)
    }

    pub fn is_sequence(self) -> bool {
        matches!(self, Self::Fasta | Self::Fastq)
    }

    pub fn is_annotation(self) -> bool {
        matches!(self, Self::Gff | Self::Bed)
    }
}

pub fn is_gzipped(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(".gz"))
}

impl fmt::Display for FileFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FileFormat {
    type Err = UnknownFormatError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "fasta" | "fa" | "fna" => Ok(Self::Fasta),
            "fastq" | "fq" => Ok(Self::Fastq),
            "genbank" | "gb" | "gbk" => Ok(Self::GenBank),
            "gff" | "gff3" => Ok(Self::Gff),
            "bed" => Ok(Self::Bed),
            "sam" => Ok(Self::Sam),
            "bam" => Ok(Self::Bam),
            "cram" => Ok(Self::Cram),
            "vcf" => Ok(Self::Vcf),
            _ => Err(UnknownFormatError(value.to_string())),
        }
    }
}

pub fn strip_gzip_suffix(name: &str) -> &str {
    name.strip_suffix(".gz").unwrap_or(name)
}

pub fn infer_format(path: &Path) -> Result<FileFormat, UnknownFormatError> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| UnknownFormatError(path.display().to_string()))?;
    let lowered = file_name.to_ascii_lowercase();
    let name = strip_gzip_suffix(&lowered);

    if name.ends_with(".fasta")
        || name.ends_with(".fa")
        || name.ends_with(".fna")
        || name.ends_with(".ffn")
        || name.ends_with(".frn")
    {
        return Ok(FileFormat::Fasta);
    }
    if name.ends_with(".fastq") || name.ends_with(".fq") {
        return Ok(FileFormat::Fastq);
    }
    if name.ends_with(".gb") || name.ends_with(".gbk") || name.ends_with(".genbank") {
        return Ok(FileFormat::GenBank);
    }
    if name.ends_with(".gff") || name.ends_with(".gff3") {
        return Ok(FileFormat::Gff);
    }
    if name.ends_with(".bed") {
        return Ok(FileFormat::Bed);
    }
    if name.ends_with(".sam") {
        return Ok(FileFormat::Sam);
    }
    if name.ends_with(".bam") {
        return Ok(FileFormat::Bam);
    }
    if name.ends_with(".cram") {
        return Ok(FileFormat::Cram);
    }
    if name.ends_with(".vcf") {
        return Ok(FileFormat::Vcf);
    }

    Err(UnknownFormatError(path.display().to_string()))
}

pub fn is_compatible_file(path: &Path) -> bool {
    infer_format(path).is_ok()
}

pub fn suggest_output_format(paths: &[PathBuf]) -> Option<FileFormat> {
    if paths.is_empty() {
        return None;
    }

    let mut counts = std::collections::HashMap::<FileFormat, usize>::new();
    for path in paths {
        if let Ok(format) = infer_format(path) {
            *counts.entry(format).or_default() += 1;
        }
    }

    let (dominant, _) = counts.iter().max_by_key(|(_, count)| *count)?;
    let dominant = *dominant;

    let suggestion = match dominant {
        FileFormat::Fastq => FileFormat::Fasta,
        FileFormat::Fasta => FileFormat::Fastq,
        FileFormat::Sam => FileFormat::Bam,
        FileFormat::Bam => FileFormat::Cram,
        FileFormat::Cram => FileFormat::Bam,
        FileFormat::Gff => FileFormat::Bed,
        FileFormat::Bed => FileFormat::Gff,
        FileFormat::GenBank => FileFormat::Fasta,
        other => other,
    };

    Some(suggestion)
}

pub fn output_filename(input_path: &Path, out_format: FileFormat, compress: bool) -> String {
    let mut name = input_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output")
        .to_string();

    if name.to_ascii_lowercase().ends_with(".gz") {
        name.truncate(name.len() - 3);
    }

    for ext in [
        ".fastq", ".fq", ".fasta", ".fa", ".fna", ".ffn", ".frn", ".gb", ".gbk", ".genbank",
        ".gff", ".gff3", ".bed", ".sam", ".bam", ".cram", ".vcf",
    ] {
        if name.to_ascii_lowercase().ends_with(ext) {
            name.truncate(name.len() - ext.len());
            break;
        }
    }

    let mut result = format!("{}{}", name, out_format.default_extension());
    if compress && out_format.is_textual() {
        result.push_str(".gz");
    }
    result
}