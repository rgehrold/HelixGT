pub mod align;
pub mod convert;
pub mod ensembl;
pub mod external;
pub mod format;
pub mod merge;
pub mod reference_io;

pub use align::{
    align_reads_to_reference, output_alignment_path, AlignOptions, AlignOutputFormat, AlignResult,
    Minimap2Options, Minimap2Preset,
};
pub use external::{
    guess_reference_for_cram, index_alignment_output_if_needed, index_path_for, minimap2_available,
    resolve_cram_reference, resolve_minimap2_path, resolve_samtools_path, samtools_available,
    ToolLogSink,
};
pub use reference_io::{
    is_gzip_bytes, is_gzip_path, load_reference_fasta_bytes, load_reference_fasta_file,
    reference_total_bytes, validate_reference_file, ReferenceSequenceRecord,
};
pub use convert::{batch_convert, convert_file, ConvertOptions, ConvertedFile};
pub use ensembl::{find_reference, list_references, EnsemblReference, REFERENCE_SOURCE};
pub use format::{infer_format, is_compatible_file, output_filename, FileFormat};
pub use merge::{
    merge_files, suggest_merge_filename, validate_merge_inputs, MergeOptions, MergeResult,
    MergeSuggestion, MergeValidation,
};