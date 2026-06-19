use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use noodles::bam;
use noodles::fasta;
use noodles::fasta::record::{Definition as FastaDefinition, Sequence};
use noodles::fastq;
use noodles::fastq::record::Definition as FastqDefinition;
use noodles::sam;
use noodles::sam::alignment::io::Write as AlignmentWrite;
use noodles::sam::alignment::record::Flags;
use noodles::sam::alignment::Record as AlignmentRecord;

use super::{finish_gzip_writer, flush_dyn_writer, open_buf_reader, open_buf_writer, ConvertOptions};
use crate::external::{
    resolve_cram_reference, resolve_samtools_path, samtools_view_from_cram, samtools_view_to_cram,
};
use crate::format::FileFormat;

pub fn convert_alignment(
    input_path: &Path,
    output_path: &Path,
    input_format: FileFormat,
    output_format: FileFormat,
    options: &ConvertOptions,
) -> Result<u64> {
    match (input_format, output_format) {
        (FileFormat::Sam, FileFormat::Sam) => transcode_sam(input_path, output_path, options),
        (FileFormat::Bam, FileFormat::Bam) => copy_bam(input_path, output_path),
        (FileFormat::Cram, FileFormat::Cram) => copy_cram(input_path, output_path, options),
        (FileFormat::Sam, FileFormat::Bam) => sam_to_bam(input_path, output_path),
        (FileFormat::Bam, FileFormat::Sam) => bam_to_sam(input_path, output_path, options),
        (FileFormat::Cram, FileFormat::Sam) => cram_to_sam(input_path, output_path, options),
        (FileFormat::Cram, FileFormat::Bam) => cram_to_bam(input_path, output_path, options),
        (FileFormat::Sam, FileFormat::Cram) | (FileFormat::Bam, FileFormat::Cram) => {
            alignment_to_cram(input_path, output_path, options)
        }
        _ => anyhow::bail!("unsupported alignment conversion: {input_format} → {output_format}"),
    }
}

pub fn to_sequence(
    input_path: &Path,
    output_path: &Path,
    input_format: FileFormat,
    output_format: FileFormat,
    options: &ConvertOptions,
) -> Result<u64> {
    match (input_format, output_format) {
        (FileFormat::Sam, FileFormat::Fasta) => sam_to_fasta(input_path, output_path, options),
        (FileFormat::Sam, FileFormat::Fastq) => sam_to_fastq(input_path, output_path, options),
        (FileFormat::Bam, FileFormat::Fasta) => bam_to_fasta(input_path, output_path, options),
        (FileFormat::Bam, FileFormat::Fastq) => bam_to_fastq(input_path, output_path, options),
        (FileFormat::Cram, FileFormat::Fasta) => cram_to_fasta(input_path, output_path, options),
        (FileFormat::Cram, FileFormat::Fastq) => cram_to_fastq(input_path, output_path, options),
        _ => anyhow::bail!(
            "alignment to sequence conversion not supported: {input_format} → {output_format}"
        ),
    }
}

pub fn from_sequence(
    input_path: &Path,
    output_path: &Path,
    input_format: FileFormat,
    output_format: FileFormat,
    options: &ConvertOptions,
) -> Result<u64> {
    match (input_format, output_format) {
        (FileFormat::Fasta, FileFormat::Sam) => fasta_to_sam(input_path, output_path, options),
        (FileFormat::Fastq, FileFormat::Sam) => fastq_to_sam(input_path, output_path, options),
        (FileFormat::Fasta, FileFormat::Bam) => {
            let temp = std::env::temp_dir().join(format!("ugt_sam_{}.sam", std::process::id()));
            fasta_to_sam(
                input_path,
                &temp,
                &ConvertOptions {
                    output_format: FileFormat::Sam,
                    compress: false,
                    prefix: String::new(),
                    reference_path: None,
                },
            )?;
            let count = sam_to_bam(&temp, output_path)?;
            let _ = std::fs::remove_file(temp);
            Ok(count)
        }
        (FileFormat::Fastq, FileFormat::Bam) => {
            let temp = std::env::temp_dir().join(format!("ugt_sam_{}.sam", std::process::id()));
            fastq_to_sam(
                input_path,
                &temp,
                &ConvertOptions {
                    output_format: FileFormat::Sam,
                    compress: false,
                    prefix: String::new(),
                    reference_path: None,
                },
            )?;
            let count = sam_to_bam(&temp, output_path)?;
            let _ = std::fs::remove_file(temp);
            Ok(count)
        }
        _ => anyhow::bail!(
            "unsupported sequence to alignment path: {input_format} → {output_format}"
        ),
    }
}

pub fn transcode_sam(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = open_sam_reader(input_path)?;
    let header = reader.read_header().context("failed to read SAM header")?;
    let mut writer = open_sam_writer(output_path, options.compress)?;
    writer
        .write_header(&header)
        .context("failed to write SAM header")?;

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid SAM record #{}", index + 1))?;
        writer
            .write_record(&header, &record)
            .with_context(|| format!("failed to write SAM record #{}", index + 1))?;
        count += 1;
    }
    flush_dyn_writer(writer.into_inner())?;
    Ok(count)
}

pub fn copy_bam(input_path: &Path, output_path: &Path) -> Result<u64> {
    let file = File::open(input_path)
        .with_context(|| format!("failed to open BAM '{}'", input_path.display()))?;
    let mut reader = bam::io::Reader::new(file);
    let header = reader.read_header().context("failed to read BAM header")?;
    let out_file = File::create(output_path)
        .with_context(|| format!("failed to create BAM '{}'", output_path.display()))?;
    let mut writer = bam::io::Writer::new(out_file);
    writer
        .write_header(&header)
        .context("failed to write BAM header")?;

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid BAM record #{}", index + 1))?;
        writer
            .write_record(&header, &record)
            .with_context(|| format!("failed to write BAM record #{}", index + 1))?;
        count += 1;
    }
    Ok(count)
}

pub fn copy_cram(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let samtools = resolve_samtools_path(&[]).context(
        "samtools is required for CRAM conversion. Place samtools.exe in src-tauri/binaries/ or add it to PATH.",
    )?;
    let reference = resolve_cram_reference(input_path, options.reference_path.as_deref())?;
    let input = input_path.to_str().context("invalid CRAM input path")?;
    let output = output_path.to_str().context("invalid CRAM output path")?;
    let reference_str = reference.to_str().context("invalid reference path")?;
    let mut command = crate::external::new_command(&samtools);
    command.args([
        "view",
        "-C",
        "-T",
        reference_str,
        "-o",
        output,
        input,
    ]);
    crate::external::run_command(command, "samtools view")?;
    crate::external::samtools_view_count(&samtools, output_path, &[])
}

fn sam_to_bam(input_path: &Path, output_path: &Path) -> Result<u64> {
    let mut reader = open_sam_reader(input_path)?;
    let header = reader.read_header().context("failed to read SAM header")?;
    let out_file = File::create(output_path)
        .with_context(|| format!("failed to create BAM '{}'", output_path.display()))?;
    let mut writer = bam::io::Writer::new(out_file);
    writer
        .write_header(&header)
        .context("failed to write BAM header")?;

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid SAM record #{}", index + 1))?;
        writer
            .write_alignment_record(&header, &record)
            .with_context(|| format!("failed to write BAM record #{}", index + 1))?;
        count += 1;
    }
    Ok(count)
}

fn bam_to_sam(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let file = File::open(input_path)
        .with_context(|| format!("failed to open BAM '{}'", input_path.display()))?;
    let mut reader = bam::io::Reader::new(file);
    let header = reader.read_header().context("failed to read BAM header")?;
    let mut writer = open_sam_writer(output_path, options.compress)?;
    writer
        .write_header(&header)
        .context("failed to write SAM header")?;

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid BAM record #{}", index + 1))?;
        writer
            .write_alignment_record(&header, &record)
            .with_context(|| format!("failed to write SAM record #{}", index + 1))?;
        count += 1;
    }
    flush_dyn_writer(writer.into_inner())?;
    Ok(count)
}

fn cram_to_sam(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let samtools = resolve_samtools_path(&[]).context(
        "samtools is required for CRAM conversion. Place samtools.exe in src-tauri/binaries/ or add it to PATH.",
    )?;
    let reference = resolve_cram_reference(input_path, options.reference_path.as_deref())?;

    if options.compress {
        let temp = std::env::temp_dir().join(format!("ugt_cram_sam_{}.sam", std::process::id()));
        let count = samtools_view_from_cram(&samtools, input_path, &temp, &reference, false)?;
        transcode_sam(&temp, output_path, options)?;
        let _ = std::fs::remove_file(temp);
        Ok(count)
    } else {
        samtools_view_from_cram(&samtools, input_path, output_path, &reference, false)
    }
}

fn cram_to_bam(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let samtools = resolve_samtools_path(&[]).context(
        "samtools is required for CRAM conversion. Place samtools.exe in src-tauri/binaries/ or add it to PATH.",
    )?;
    let reference = resolve_cram_reference(input_path, options.reference_path.as_deref())?;
    samtools_view_from_cram(&samtools, input_path, output_path, &reference, true)
}

fn sam_to_fasta(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = open_sam_reader(input_path)?;
    let _header = reader.read_header().context("failed to read SAM header")?;
    write_alignment_records_as_fasta(reader.records(), output_path, options)
}

fn sam_to_fastq(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = open_sam_reader(input_path)?;
    let _header = reader.read_header().context("failed to read SAM header")?;
    write_alignment_records_as_fastq(reader.records(), output_path, options)
}

fn bam_to_fasta(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let file = File::open(input_path)
        .with_context(|| format!("failed to open BAM '{}'", input_path.display()))?;
    let mut reader = bam::io::Reader::new(file);
    let _header = reader.read_header().context("failed to read BAM header")?;
    write_alignment_records_as_fasta(reader.records(), output_path, options)
}

fn bam_to_fastq(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let file = File::open(input_path)
        .with_context(|| format!("failed to open BAM '{}'", input_path.display()))?;
    let mut reader = bam::io::Reader::new(file);
    let _header = reader.read_header().context("failed to read BAM header")?;
    write_alignment_records_as_fastq(reader.records(), output_path, options)
}

fn cram_to_fasta(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let temp = std::env::temp_dir().join(format!("ugt_cram_sam_{}.sam", std::process::id()));
    cram_to_sam(
        input_path,
        &temp,
        &ConvertOptions {
            output_format: FileFormat::Sam,
            compress: false,
            prefix: String::new(),
            reference_path: options.reference_path.clone(),
        },
    )?;
    let count = sam_to_fasta(&temp, output_path, options)?;
    let _ = std::fs::remove_file(temp);
    Ok(count)
}

fn cram_to_fastq(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let temp = std::env::temp_dir().join(format!("ugt_cram_sam_{}.sam", std::process::id()));
    cram_to_sam(
        input_path,
        &temp,
        &ConvertOptions {
            output_format: FileFormat::Sam,
            compress: false,
            prefix: String::new(),
            reference_path: options.reference_path.clone(),
        },
    )?;
    let count = sam_to_fastq(&temp, output_path, options)?;
    let _ = std::fs::remove_file(temp);
    Ok(count)
}

fn fasta_to_sam(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = fasta::io::reader::Builder::default()
        .build_from_reader(open_buf_reader(input_path)?)
        .context("failed to create FASTA reader")?;
    let header = sam::Header::default();
    let mut writer = open_sam_writer(output_path, options.compress)?;
    writer
        .write_header(&header)
        .context("failed to write SAM header")?;

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTA record #{}", index + 1))?;
        let alignment = sam::alignment::RecordBuf::builder()
            .set_name(record.name().to_vec())
            .set_flags(Flags::UNMAPPED)
            .set_sequence(record.sequence().as_ref().into())
            .build();
        writer
            .write_alignment_record(&header, &alignment)
            .with_context(|| format!("failed to write SAM record #{}", index + 1))?;
        count += 1;
    }
    flush_dyn_writer(writer.into_inner())?;
    Ok(count)
}

fn fastq_to_sam(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = fastq::io::Reader::new(open_buf_reader(input_path)?);
    let header = sam::Header::default();
    let mut writer = open_sam_writer(output_path, options.compress)?;
    writer
        .write_header(&header)
        .context("failed to write SAM header")?;

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTQ record #{}", index + 1))?;
        let alignment = sam::alignment::RecordBuf::builder()
            .set_name(record.name().to_vec())
            .set_flags(Flags::UNMAPPED)
            .set_sequence(record.sequence().into())
            .set_quality_scores(record.quality_scores().to_vec().into())
            .build();
        writer
            .write_alignment_record(&header, &alignment)
            .with_context(|| format!("failed to write SAM record #{}", index + 1))?;
        count += 1;
    }
    flush_dyn_writer(writer.into_inner())?;
    Ok(count)
}

type SamReader = sam::io::Reader<Box<dyn std::io::BufRead>>;
type SamWriter = sam::io::Writer<Box<dyn Write>>;

fn open_sam_reader(path: &Path) -> Result<SamReader> {
    sam::io::reader::Builder::default()
        .build_from_path(path)
        .with_context(|| format!("failed to open SAM '{}'", path.display()))
}

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

fn write_alignment_records_as_fasta<I, R>(
    records: I,
    output_path: &Path,
    options: &ConvertOptions,
) -> Result<u64>
where
    I: Iterator<Item = Result<R, std::io::Error>>,
    R: AlignmentRecord,
{
    let mut writer = fasta::io::writer::Builder::default()
        .build_from_writer(open_buf_writer(output_path, options.compress)?);
    let mut count = 0u64;

    for (index, result) in records.enumerate() {
        let record = result
            .map_err(|error| anyhow::anyhow!(error))
            .with_context(|| format!("invalid alignment record #{}", index + 1))?;
        if write_alignment_as_fasta(&mut writer, &record, index)? {
            count += 1;
        }
    }
    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

fn write_alignment_records_as_fastq<I, R>(
    records: I,
    output_path: &Path,
    options: &ConvertOptions,
) -> Result<u64>
where
    I: Iterator<Item = Result<R, std::io::Error>>,
    R: AlignmentRecord,
{
    let mut writer = fastq::io::Writer::new(open_buf_writer(output_path, options.compress)?);
    let mut count = 0u64;

    for (index, result) in records.enumerate() {
        let record = result
            .map_err(|error| anyhow::anyhow!(error))
            .with_context(|| format!("invalid alignment record #{}", index + 1))?;
        if write_alignment_as_fastq(&mut writer, &record, index)? {
            count += 1;
        }
    }
    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

fn write_alignment_as_fasta<W, R>(writer: &mut fasta::io::Writer<W>, record: &R, index: usize) -> Result<bool>
where
    W: std::io::Write,
    R: AlignmentRecord + ?Sized,
{
    let sequence = record.sequence();
    if sequence.is_empty() {
        return Ok(false);
    }
    let name = record
        .name()
        .map(|value| value.to_vec())
        .unwrap_or_else(|| format!("read_{}", index + 1).into_bytes());
    let seq_bytes: Vec<u8> = sequence.iter().collect();
    let fasta_record = fasta::Record::new(
        FastaDefinition::new(name, None),
        Sequence::from(seq_bytes),
    );
    writer
        .write_record(&fasta_record)
        .with_context(|| format!("failed to write FASTA record #{}", index + 1))?;
    Ok(true)
}

fn write_alignment_as_fastq<W, R>(writer: &mut fastq::io::Writer<W>, record: &R, index: usize) -> Result<bool>
where
    W: std::io::Write,
    R: AlignmentRecord + ?Sized,
{
    let sequence = record.sequence();
    if sequence.is_empty() {
        return Ok(false);
    }
    let name = record
        .name()
        .map(|value| value.to_vec())
        .unwrap_or_else(|| format!("read_{}", index + 1).into_bytes());
    let seq_bytes: Vec<u8> = sequence.iter().collect();
    let mut qualities = Vec::new();
    for score in record.quality_scores().iter() {
        qualities.push(score? as u8);
    }
    if qualities.len() != seq_bytes.len() {
        qualities = vec![b'I'; seq_bytes.len()];
    }
    let fastq_record = fastq::Record::new(
        FastqDefinition::new(name, Vec::new()),
        seq_bytes,
        qualities,
    );
    writer
        .write_record(&fastq_record)
        .with_context(|| format!("failed to write FASTQ record #{}", index + 1))?;
    Ok(true)
}

fn alignment_to_cram(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let reference = options
        .reference_path
        .as_deref()
        .context("CRAM conversion requires a reference FASTA. Choose a reference file first.")?;
    let samtools = resolve_samtools_path(&[]).context(
        "samtools is required for CRAM conversion. Place samtools.exe in src-tauri/binaries/ or add it to PATH.",
    )?;
    samtools_view_to_cram(&samtools, input_path, output_path, reference)
}