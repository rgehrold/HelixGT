use std::io::{BufRead, Write};
use std::path::Path;

use anyhow::{Context, Result};
use noodles::fasta;
use noodles::fasta::record::{Definition as FastaDefinition, Sequence};

use super::{finish_gzip_writer, open_buf_reader, open_buf_writer, ConvertOptions};
use crate::format::FileFormat;

pub fn convert_genbank(
    input_path: &Path,
    output_path: &Path,
    input_format: FileFormat,
    output_format: FileFormat,
    options: &ConvertOptions,
) -> Result<u64> {
    match (input_format, output_format) {
        (FileFormat::GenBank, FileFormat::GenBank) => copy_genbank(input_path, output_path, options),
        (FileFormat::GenBank, FileFormat::Fasta) => genbank_to_fasta(input_path, output_path, options),
        (FileFormat::GenBank, FileFormat::Fastq) => {
            let temp = crate::tools::helixgt_temp_file("genbank", "fasta");
            genbank_to_fasta(
                input_path,
                &temp,
                &ConvertOptions {
                    output_format: FileFormat::Fasta,
                    compress: false,
                    ..Default::default()
                },
            )?;
            let count = super::sequence::convert_sequence(
                &temp,
                output_path,
                FileFormat::Fasta,
                &ConvertOptions {
                    output_format: FileFormat::Fastq,
                    compress: options.compress,
                    ..Default::default()
                },
            )?;
            let _ = std::fs::remove_file(temp);
            Ok(count)
        }
        (FileFormat::Fasta, FileFormat::GenBank) => {
            anyhow::bail!("use sequence_to_genbank for FASTA → GenBank")
        }
        _ => anyhow::bail!("unsupported GenBank conversion: {input_format} → {output_format}"),
    }
}

pub fn sequence_to_genbank(
    input_path: &Path,
    output_path: &Path,
    input_format: FileFormat,
    options: &ConvertOptions,
) -> Result<u64> {
    match input_format {
        FileFormat::Fasta => fasta_to_genbank(input_path, output_path, options),
        FileFormat::Fastq => {
            let temp = crate::tools::helixgt_temp_file("genbank", "fasta");
            super::sequence::convert_sequence(
                input_path,
                &temp,
                FileFormat::Fastq,
                &ConvertOptions {
                    output_format: FileFormat::Fasta,
                    compress: false,
                    ..Default::default()
                },
            )?;
            let count = fasta_to_genbank(&temp, output_path, options)?;
            let _ = std::fs::remove_file(temp);
            Ok(count)
        }
        _ => anyhow::bail!("only FASTA and FASTQ can be converted to GenBank (got {input_format})"),
    }
}

pub fn copy_genbank(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let reader = open_buf_reader(input_path)?;
    let mut writer = open_buf_writer(output_path, options.compress)?;
    let mut count = 0u64;

    for result in reader.lines() {
        let line_content = result.context("failed to read GenBank line")?;
        writeln!(writer, "{line_content}").context("failed to write GenBank line")?;
        if line_content.starts_with("//") {
            count += 1;
        }
    }

    finish_gzip_writer(writer)?;
    Ok(count.max(1))
}

fn genbank_to_fasta(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let reader = open_buf_reader(input_path)?;
    let mut writer = fasta::io::writer::Builder::default()
        .build_from_writer(open_buf_writer(output_path, options.compress)?);

    let mut locus_name = "sequence".to_string();
    let mut sequence = String::new();
    let mut in_origin = false;
    let mut count = 0u64;

    for result in reader.lines() {
        let line = result.context("failed to read GenBank line")?;
        let trimmed = line.trim();

        if trimmed.starts_with("LOCUS") {
            locus_name = trimmed
                .split_whitespace()
                .nth(1)
                .unwrap_or("sequence")
                .to_string();
            continue;
        }

        if trimmed == "ORIGIN" {
            in_origin = true;
            sequence.clear();
            continue;
        }

        if trimmed.starts_with("//") {
            if in_origin && !sequence.is_empty() {
                let definition = FastaDefinition::new(locus_name.clone(), None);
                let fasta_record =
                    fasta::Record::new(definition, Sequence::from(sequence.as_bytes().to_vec()));
                writer
                    .write_record(&fasta_record)
                    .context("failed to write FASTA record")?;
                count += 1;
            }
            in_origin = false;
            locus_name = "sequence".to_string();
            sequence.clear();
            continue;
        }

        if in_origin {
            let bases: String = trimmed
                .split_whitespace()
                .skip(1)
                .collect::<Vec<_>>()
                .join("");
            sequence.push_str(&bases);
        }
    }

    finish_gzip_writer(writer.into_inner())?;
    Ok(count)
}

fn fasta_to_genbank(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = fasta::io::reader::Builder::default()
        .build_from_reader(open_buf_reader(input_path)?)
        .context("failed to create FASTA reader")?;
    let mut writer = open_buf_writer(output_path, options.compress)?;
    let mut count = 0u64;

    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTA record #{}", index + 1))?;
        let name = String::from_utf8_lossy(record.name()).to_string();
        let sequence = record.sequence().as_ref();
        let len = sequence.len();
        let now = chrono_lite_date();

        writeln!(writer, "LOCUS       {name:<16} {len} bp    DNA     linear   UNK {now}")
            .context("failed to write GenBank LOCUS line")?;
        writeln!(writer, "DEFINITION  {name}.")
            .context("failed to write GenBank DEFINITION line")?;
        writeln!(writer, "ACCESSION   .")
            .context("failed to write GenBank ACCESSION line")?;
        writeln!(writer, "VERSION     .")
            .context("failed to write GenBank VERSION line")?;
        writeln!(writer, "KEYWORDS    .")
            .context("failed to write GenBank KEYWORDS line")?;
        writeln!(writer, "SOURCE      .")
            .context("failed to write GenBank SOURCE line")?;
        writeln!(writer, "  ORGANISM  .")
            .context("failed to write GenBank ORGANISM line")?;
        writeln!(writer, "FEATURES             Location/Qualifiers")
            .context("failed to write GenBank FEATURES line")?;
        writeln!(writer, "     source          1..{len}")
            .context("failed to write GenBank source feature")?;
        writeln!(writer, "                     /organism=\"unknown\"")
            .context("failed to write GenBank organism qualifier")?;
        writeln!(writer, "ORIGIN")
            .context("failed to write GenBank ORIGIN line")?;

        let bases = sequence
            .iter()
            .map(|base| base.to_ascii_uppercase())
            .collect::<Vec<_>>();
        for (line_index, chunk) in bases.chunks(60).enumerate() {
            let position = line_index * 60 + 1;
            write!(writer, "{position:>9} ").context("failed to write GenBank position")?;
            for (group_index, group) in chunk.chunks(10).enumerate() {
                if group_index > 0 {
                    write!(writer, " ").context("failed to write GenBank spacing")?;
                }
                for base in group {
                    write!(writer, "{}", *base as char).context("failed to write GenBank base")?;
                }
            }
            writeln!(writer).context("failed to write GenBank line break")?;
        }

        writeln!(writer, "//").context("failed to write GenBank record terminator")?;
        count += 1;
    }

    finish_gzip_writer(writer)?;
    Ok(count)
}

fn chrono_lite_date() -> String {
    // Simple stable date placeholder without adding a chrono dependency.
    "01-JAN-2026".to_string()
}