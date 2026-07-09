use std::path::PathBuf;

use converter_core::{suggest_merge_filename, suggest_output_format, validate_merge_inputs};

#[test]
fn suggests_fastq_to_fasta_conversion() {
    let paths = vec![PathBuf::from("reads.fastq")];
    assert_eq!(suggest_output_format(&paths), Some(converter_core::FileFormat::Fasta));
}

#[test]
fn merge_validation_requires_matching_extensions() {
    let paths = vec![PathBuf::from("a.fasta"), PathBuf::from("b.fastq")];
    let validation = validate_merge_inputs(&paths);
    assert!(!validation.is_valid);
}

#[test]
fn suggests_merge_name_from_similar_stems() {
    let paths = vec![
        PathBuf::from("sample_A_reads.fastq"),
        PathBuf::from("sample_B_reads.fastq"),
    ];
    let suggestion = suggest_merge_filename(&paths);
    assert!(suggestion.suggested_name.ends_with(".fastq"));
}