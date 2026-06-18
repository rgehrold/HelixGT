use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use converter_core::{convert_file, ConvertOptions, FileFormat};
use flate2::read::GzDecoder;

const TEST_FASTQ_GZ: &str =
    r"C:\Users\Robin\Projects\grok-dna-fc\test\PSPS6-BamHI-MBN.plus.rawreads.fastq.gz";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_output(extension: &str) -> PathBuf {
    let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "ugt_test_{}_{id}{extension}",
        std::process::id()
    ));
    let _ = fs::remove_file(&path);
    path
}

fn is_gzip_bytes(path: &PathBuf) -> bool {
    let mut file = fs::File::open(path).unwrap();
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).unwrap();
    magic == [0x1f, 0x8b]
}

#[test]
fn fastq_gz_to_fastq_decompresses() {
    let input = PathBuf::from(TEST_FASTQ_GZ);
    if !input.exists() {
        eprintln!("skipping: test file not found at {TEST_FASTQ_GZ}");
        return;
    }

    let output = temp_output(".fastq");
    let records = convert_file(
        &input,
        &output,
        &ConvertOptions {
            output_format: FileFormat::Fastq,
            compress: false,
            prefix: String::new(),
        },
    )
    .expect("fastq.gz → fastq conversion should succeed");

    assert!(records > 0, "expected records to be converted");
    assert!(!is_gzip_bytes(&output), "output must be plain text, not gzip");

    let content = fs::read_to_string(&output).expect("output should be valid UTF-8 text");
    assert!(content.starts_with('@'), "FASTQ should start with @ header");
    let _ = fs::remove_file(output);
}

fn count_fasta_records(path: &PathBuf) -> usize {
    let content = fs::read_to_string(path).expect("read fasta");
    content.lines().filter(|line| line.starts_with('>')).count()
}

#[test]
fn fastq_gz_to_fasta_converts_all_records() {
    let input = PathBuf::from(TEST_FASTQ_GZ);
    if !input.exists() {
        return;
    }

    let plain_fastq = temp_output(".fastq");
    let expected = convert_file(
        &input,
        &plain_fastq,
        &ConvertOptions {
            output_format: FileFormat::Fastq,
            compress: false,
            prefix: String::new(),
        },
    )
    .expect("decompress fastq");

    let output = temp_output(".fasta");
    let records = convert_file(
        &input,
        &output,
        &ConvertOptions {
            output_format: FileFormat::Fasta,
            compress: false,
            prefix: String::new(),
        },
    )
    .expect("fastq.gz → fasta conversion should succeed");

    let fasta_count = count_fasta_records(&output);
    assert!(
        expected > 100,
        "sanity check: test FASTQ should contain many reads (got {expected})"
    );
    assert_eq!(
        records, expected,
        "reported record count should match decompressed FASTQ records"
    );
    assert_eq!(
        fasta_count, expected as usize,
        "FASTA output should contain one record per FASTQ read (expected {expected}, got {fasta_count})"
    );

    let _ = fs::remove_file(plain_fastq);
    let _ = fs::remove_file(output);
}

#[test]
fn multi_member_gzip_reads_all_fastq_records() {
    let input = PathBuf::from(TEST_FASTQ_GZ);
    if !input.exists() {
        return;
    }

    let output = temp_output(".fasta");
    let records = convert_file(
        &input,
        &output,
        &ConvertOptions {
            output_format: FileFormat::Fasta,
            compress: false,
            prefix: String::new(),
        },
    )
    .expect("multi-member gzip fastq.gz → fasta should succeed");

    assert!(
        records > 100,
        "concatenated gzip members must be read fully (got {records} records)"
    );

    let _ = fs::remove_file(output);
}

#[test]
fn fastq_gz_to_fasta_works() {
    let input = PathBuf::from(TEST_FASTQ_GZ);
    if !input.exists() {
        return;
    }

    let output = temp_output(".fasta");
    let records = convert_file(
        &input,
        &output,
        &ConvertOptions {
            output_format: FileFormat::Fasta,
            compress: false,
            prefix: String::new(),
        },
    )
    .expect("fastq.gz → fasta conversion should succeed");

    assert!(records > 0);
    let content = fs::read_to_string(&output).expect("FASTA output should be text");
    assert!(content.starts_with('>'));
    let _ = fs::remove_file(output);
}

#[test]
fn fastq_to_fastq_can_recompress() {
    let input = PathBuf::from(TEST_FASTQ_GZ);
    if !input.exists() {
        return;
    }

    let plain = temp_output(".fastq");
    convert_file(
        &input,
        &plain,
        &ConvertOptions {
            output_format: FileFormat::Fastq,
            compress: false,
            prefix: String::new(),
        },
    )
    .unwrap();

    let output = temp_output(".fastq.gz");
    convert_file(
        &plain,
        &output,
        &ConvertOptions {
            output_format: FileFormat::Fastq,
            compress: true,
            prefix: String::new(),
        },
    )
    .expect("plain fastq → gzipped fastq should succeed");

    assert!(is_gzip_bytes(&output));
    let mut decoder = GzDecoder::new(fs::File::open(&output).unwrap());
    let mut decoded = String::new();
    decoder.read_to_string(&mut decoded).unwrap();
    assert!(decoded.starts_with('@'));

    let _ = fs::remove_file(plain);
    let _ = fs::remove_file(output);
}