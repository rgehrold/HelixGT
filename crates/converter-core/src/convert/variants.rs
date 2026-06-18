use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use noodles::vcf;

use super::{flush_dyn_writer, ConvertOptions};

pub fn convert_vcf(input_path: &Path, output_path: &Path, options: &ConvertOptions) -> Result<u64> {
    let mut reader = vcf::io::reader::Builder::default()
        .build_from_path(input_path)
        .with_context(|| format!("failed to open VCF '{}'", input_path.display()))?;
    let header = reader
        .read_header()
        .context("failed to read VCF header")?;

    let out_file = File::create(output_path)
        .with_context(|| format!("failed to create VCF '{}'", output_path.display()))?;
    let mut builder = vcf::io::writer::Builder::default();
    if options.compress {
        builder = builder.set_compression_method(vcf::io::CompressionMethod::Bgzf);
    } else {
        builder = builder.set_compression_method(vcf::io::CompressionMethod::None);
    }
    let mut writer = builder.build_from_writer(out_file);
    writer
        .write_header(&header)
        .context("failed to write VCF header")?;

    let mut count = 0u64;
    for (index, result) in reader.records().enumerate() {
        let record = result.with_context(|| format!("invalid VCF record #{}", index + 1))?;
        writer
            .write_record(&header, &record)
            .with_context(|| format!("failed to write VCF record #{}", index + 1))?;
        count += 1;
    }
    flush_dyn_writer(writer.into_inner())?;
    Ok(count)
}