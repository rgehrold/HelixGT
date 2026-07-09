use std::io::{BufRead, BufReader, Cursor, Read};
use std::path::Path;

use anyhow::{Context, Result};
use flate2::read::MultiGzDecoder;
use noodles::fasta;

use crate::convert::open_buf_reader;

#[derive(Debug, Clone)]
pub struct ReferenceSequenceRecord {
    pub name: String,
    pub sequence: Vec<u8>,
}

pub fn load_reference_fasta_bytes(data: &[u8], gzipped: bool) -> Result<Vec<ReferenceSequenceRecord>> {
    let reader: Box<dyn Read> = if gzipped {
        Box::new(MultiGzDecoder::new(Cursor::new(data)))
    } else {
        Box::new(Cursor::new(data))
    };

    read_fasta_records(BufReader::new(reader))
}

pub fn load_reference_fasta_file(path: &Path) -> Result<Vec<ReferenceSequenceRecord>> {
    read_fasta_records(open_buf_reader(path)?)
}

pub fn validate_reference_file(path: &Path, gzipped: bool) -> Result<usize> {
    let reader: Box<dyn Read> = if gzipped {
        Box::new(MultiGzDecoder::new(std::fs::File::open(path)?))
    } else {
        Box::new(std::fs::File::open(path)?)
    };
    let mut parser = fasta::io::reader::Builder::default()
        .build_from_reader(BufReader::new(reader))
        .context("failed to open reference FASTA")?;

    let mut total = 0usize;
    let mut record_count = 0usize;
    for (index, result) in parser.records().enumerate() {
        let record = result.with_context(|| format!("invalid reference FASTA record #{}", index + 1))?;
        total += record.sequence().len();
        record_count += 1;
    }
    if record_count == 0 {
        anyhow::bail!("reference FASTA contains no sequences");
    }
    Ok(total)
}

fn read_fasta_records<R: BufRead>(reader: R) -> Result<Vec<ReferenceSequenceRecord>> {
    let mut parser = fasta::io::reader::Builder::default()
        .build_from_reader(reader)
        .context("failed to create FASTA reader for reference")?;

    let mut records = Vec::new();
    for (index, result) in parser.records().enumerate() {
        let record = result.with_context(|| format!("invalid reference FASTA record #{}", index + 1))?;
        records.push(ReferenceSequenceRecord {
            name: String::from_utf8_lossy(record.name()).to_string(),
            sequence: record.sequence().as_ref().to_vec(),
        });
    }

    if records.is_empty() {
        anyhow::bail!("reference FASTA contains no sequences");
    }
    Ok(records)
}

pub fn reference_total_bytes(records: &[ReferenceSequenceRecord]) -> usize {
    records.iter().map(|record| record.sequence.len()).sum()
}

pub fn is_gzip_bytes(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
}

pub fn ensure_reference_index(samtools: &std::path::Path, reference_path: &Path) -> Result<()> {
    crate::external::samtools_faidx(samtools, reference_path)
}

pub fn is_gzip_path(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(".gz")
}