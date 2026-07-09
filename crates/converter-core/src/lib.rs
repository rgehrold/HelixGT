pub mod align;
pub mod cancel;
pub mod convert;
pub mod ensembl;
pub mod external;
pub mod format;
pub mod merge;
pub mod preflight;
pub mod qc;
pub mod reference_io;
pub mod tools;

pub use align::{
    align_reads_to_reference, output_alignment_path, AlignOptions, AlignOutputFormat, AlignResult,
    Minimap2Options, Minimap2Preset,
};
pub use cancel::CancelToken;
pub use external::{
    guess_reference_for_cram, index_alignment_output_if_needed, index_path_for, minimap2_available,
    resolve_cram_reference, resolve_minimap2_path, resolve_samtools_path, samtools_available,
    ToolLogSink,
};
pub use reference_io::{
    ensure_reference_index, is_gzip_bytes, is_gzip_path, load_reference_fasta_bytes,
    load_reference_fasta_file, reference_total_bytes, validate_reference_file,
    ReferenceSequenceRecord,
};
pub use convert::{
    batch_convert, convert_file, BatchConvertFailure, BatchConvertResult, ConvertOptions,
    ConvertedFile,
};
pub use ensembl::{find_reference, list_references, EnsemblReference, REFERENCE_SOURCE};
pub use format::{
    infer_format, is_compatible_file, output_filename, suggest_output_format, FileFormat,
};
pub use merge::{
    merge_files, suggest_merge_filename, validate_merge_inputs, MergeOptions, MergeResult,
    MergeSuggestion, MergeValidation,
};
pub use preflight::{
    run_preflight, PreflightIssue, PreflightMode, PreflightReport, PreflightRequest,
    PreflightSeverity,
};
pub use qc::{fastq_qc, FastqQcSummary};
pub use tools::{default_thread_count, helixgt_temp_dir, ToolPaths};