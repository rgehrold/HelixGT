use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use super::{finish_gzip_writer, open_buf_reader, open_buf_writer, ConvertOptions};
use crate::format::FileFormat;

pub fn convert_annotation(
    input_path: &Path,
    output_path: &Path,
    input_format: FileFormat,
    output_format: FileFormat,
    options: &ConvertOptions,
) -> Result<u64> {
    match (input_format, output_format) {
        (FileFormat::Gff, FileFormat::Bed) => gff_to_bed(input_path, output_path, options),
        (FileFormat::Bed, FileFormat::Gff) => bed_to_gff(input_path, output_path, options),
        _ => anyhow::bail!("unsupported annotation conversion: {input_format} → {output_format}"),
    }
}

fn gff_to_bed(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = noodles::gff::io::Reader::new(open_buf_reader(input_path)?);
    let mut writer = open_buf_writer(output_path, options.compress)?;
    let mut count = 0u64;

    for (index, result) in reader.record_bufs().enumerate() {
        let record = result.with_context(|| format!("invalid GFF record #{}", index + 1))?;
        let reference_sequence_name = record.reference_sequence_name().to_string();
        let start = record.start().get();
        let end = record.end().get();
        let name = record
            .attributes()
            .get("ID")
            .or_else(|| record.attributes().get("Name"))
            .and_then(|value| value.as_string())
            .unwrap_or(".");
        writeln!(
            writer,
            "{reference_sequence_name}\t{start}\t{end}\t{name}\t0\t+"
        )
        .with_context(|| format!("failed to write BED record #{}", index + 1))?;
        count += 1;
    }

    finish_gzip_writer(writer)?;
    Ok(count)
}

fn bed_to_gff(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let reader = super::open_text_reader(input_path)?;
    let mut reader = std::io::BufReader::new(reader);
    let mut writer = open_buf_writer(output_path, options.compress)?;
    writeln!(writer, "##gff-version 3")?;
    let mut count = 0u64;
    let mut line_index = 0usize;

    for line in std::io::BufRead::lines(&mut reader) {
        line_index += 1;
        let line = line.with_context(|| format!("failed to read BED line #{line_index}"))?;
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("track") {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 3 {
            continue;
        }
        let seq = fields[0];
        let start = fields[1];
        let end = fields[2];
        let name = fields.get(3).copied().unwrap_or("feature");
        let strand = fields.get(5).copied().unwrap_or(".");
        writeln!(
            writer,
            "{seq}\thelixgt\tregion\t{start}\t{end}\t.\t{strand}\t.\tID={name}"
        )
        .with_context(|| format!("failed to write GFF record from BED line #{line_index}"))?;
        count += 1;
    }

    finish_gzip_writer(writer)?;
    Ok(count)
}