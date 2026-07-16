//! Windowed sequence / annotation / alignment viewing for HelixGT View mode.
//!
//! Coordinates in this module are **0-based, half-open** `[start, end)`.
//! The UI should display positions as 1-based for users.

mod alignment;
mod annotation;
mod sequence;

pub use alignment::{
    get_coverage_bins, get_reads_in_range, get_reads_in_range_filtered, open_alignment_document,
    AlignmentContig, AlignmentDocument, AlignmentRead, CigarOp, CoverageBin, CoverageWindow,
    ReadQueryOptions, ReadsWindow, DEFAULT_COVERAGE_BINS, MAX_READS_PER_WINDOW,
};
pub use annotation::{
    clear_annotation_cache, get_features_in_range, open_annotation_document, AnnotationContigSpan,
    AnnotationDocument, AnnotationFeature, FeatureWindow, MAX_FEATURES_PER_WINDOW,
};
pub use sequence::{
    clear_sequence_cache, get_sequence_window, open_sequence_document, ContigInfo, SequenceDocument,
    SequenceSlice, MAX_SEQUENCE_WINDOW,
};
