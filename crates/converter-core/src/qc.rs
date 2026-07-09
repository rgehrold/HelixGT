use std::path::Path;

use anyhow::{Context, Result};

use crate::convert::open_buf_reader;
use crate::format::{infer_format, FileFormat};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FastqQcSummary {
    pub read_count: u64,
    pub total_bases: u64,
    pub mean_read_length: f64,
    pub min_read_length: u32,
    pub max_read_length: u32,
    pub n_content_fraction: f64,
}

pub fn fastq_qc(path: &Path) -> Result<FastqQcSummary> {
    let format = infer_format(path).with_context(|| format!("cannot infer format for '{}'", path.display()))?;
    if format != FileFormat::Fastq {
        anyhow::bail!("FASTQ QC requires a FASTQ file (got {format})");
    }

    let mut reader = noodles::fastq::io::Reader::new(open_buf_reader(path)?);
    let mut read_count = 0u64;
    let mut total_bases = 0u64;
    let mut n_bases = 0u64;
    let mut min_len = u32::MAX;
    let mut max_len = 0u32;

    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid FASTQ record #{}", index + 1))?;
        let sequence = record.sequence();
        let len = sequence.len() as u32;
        read_count += 1;
        total_bases += len as u64;
        min_len = min_len.min(len);
        max_len = max_len.max(len);
        for base in sequence.iter() {
            if matches!(base, b'N' | b'n') {
                n_bases += 1;
            }
        }
    }

    if read_count == 0 {
        anyhow::bail!("FASTQ file contains no reads");
    }

    Ok(FastqQcSummary {
        read_count,
        total_bases,
        mean_read_length: total_bases as f64 / read_count as f64,
        min_read_length: if min_len == u32::MAX { 0 } else { min_len },
        max_read_length: max_len,
        n_content_fraction: if total_bases == 0 {
            0.0
        } else {
            n_bases as f64 / total_bases as f64
        },
    })
}