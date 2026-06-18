use std::path::Path;

use anyhow::{Context, Result};
use noodles::bed;
use noodles::fasta;
use noodles::fasta::record::{Definition as FastaDefinition, Sequence};
use noodles::fastq;
use noodles::fastq::record::Definition as FastqDefinition;
use noodles::gff;

use super::{finish_gzip_writer, open_buf_reader, open_buf_writer, ConvertOptions};
use crate::format::FileFormat;

pub fn convert_sequence(
    input_path: &Path,
    output_path: &Path,
    input_format: FileFormat,
    options: &ConvertOptions,
) -> Result<u64> {
    match (input_format, options.output_format) {
        (FileFormat::Fasta, FileFormat::Fastq) => fasta_to_fastq(input_path, output_path, options),
        (FileFormat::Fastq, FileFormat::Fasta) => fastq_to_fasta(input_path, output_path, options),
        (FileFormat::Fasta, FileFormat::Fasta) => copy_fasta(input_path, output_path, options),
        (FileFormat::Fastq, FileFormat::Fastq) => copy_fastq(input_path, output_path, options),
        _ => anyhow::bail!("unsupported sequence conversion: {input_format} → {}", options.output_format),
    }
}

pub fn copy_fasta(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = fasta::io::reader::Builder::default()
        .build_from_reader(open_buf_reader(input_path)?)
        .context("failed to create FASTA reader")?;
    let mut writer = fasta::io::writer::Builder::default()
        .build_from_writer(open_buf_writer(output_path, options.compress)?);

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTA record #{}", index + 1))?;
        writer
            .write_record(&record)
            .with_context(|| format!("failed to write FASTA record #{}", index + 1))?;
        count += 1;
    }
    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

pub fn copy_fastq(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = fastq::io::Reader::new(open_buf_reader(input_path)?);
    let mut writer = fastq::io::Writer::new(open_buf_writer(output_path, options.compress)?);

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTQ record #{}", index + 1))?;
        writer
            .write_record(&record)
            .with_context(|| format!("failed to write FASTQ record #{}", index + 1))?;
        count += 1;
    }
    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

pub fn copy_gff(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = gff::io::Reader::new(open_buf_reader(input_path)?);
    let mut writer = gff::io::Writer::new(open_buf_writer(output_path, options.compress)?);

    let mut count = 0u64;
    for (index, result) in reader.record_bufs().enumerate() {
        let record = result.with_context(|| format!("invalid GFF record #{}", index + 1))?;
        writer
            .write_record(&record)
            .with_context(|| format!("failed to write GFF record #{}", index + 1))?;
        count += 1;
    }
    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

pub fn copy_bed(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = bed::io::Reader::<3, _>::new(open_buf_reader(input_path)?);
    let mut writer = bed::io::Writer::<3, _>::new(open_buf_writer(output_path, options.compress)?);
    let mut record = bed::Record::default();

    let mut count = 0u64;
    loop {
        let bases = reader
            .read_record(&mut record)
            .context("failed to read BED record")?;
        if bases == 0 {
            break;
        }
        writer
            .write_record(&record)
            .with_context(|| format!("failed to write BED record #{}", count + 1))?;
        count += 1;
    }
    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

fn fasta_to_fastq(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = fasta::io::reader::Builder::default()
        .build_from_reader(open_buf_reader(input_path)?)
        .context("failed to create FASTA reader")?;
    let mut writer = fastq::io::Writer::new(open_buf_writer(output_path, options.compress)?);

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTA record #{}", index + 1))?;
        let sequence = record.sequence().as_ref();
        let description = record
            .description()
            .map(|value| value.to_vec())
            .unwrap_or_default();
        let definition = FastqDefinition::new(record.name().to_vec(), description);
        let quality = vec![b'I'; sequence.len()];
        let fastq_record = fastq::Record::new(definition, sequence, quality);
        writer
            .write_record(&fastq_record)
            .with_context(|| format!("failed to write FASTQ record #{}", index + 1))?;
        count += 1;
    }
    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

fn fastq_to_fasta(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = fastq::io::Reader::new(open_buf_reader(input_path)?);
    let mut writer = fasta::io::writer::Builder::default()
        .build_from_writer(open_buf_writer(output_path, options.compress)?);

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTQ record #{}", index + 1))?;
        let description = record.description();
        let definition = FastaDefinition::new(
            record.name().to_vec(),
            if description.is_empty() {
                None
            } else {
                Some(description.to_vec())
            },
        );
        let fasta_record = fasta::Record::new(
            definition,
            Sequence::from(record.sequence().to_vec()),
        );
        writer
            .write_record(&fasta_record)
            .with_context(|| format!("failed to write FASTA record #{}", index + 1))?;
        count += 1;
    }
    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}