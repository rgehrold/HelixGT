/** Shared constants for the IGV-style View browser. */

/** Min visible span (bp). */
export const MIN_VISIBLE_BP = 20;

/** Default window when opening large contigs. */
export const DEFAULT_VISIBLE_BP_LARGE = 50_000;

/** Max sequence window request (matches converter-core MAX_SEQUENCE_WINDOW). */
export const MAX_SEQUENCE_WINDOW_BP = 2_000_000;

/** Draw nucleotide letters on ref / reads at or below this width. */
export const BASE_LETTERS_MAX_BP = 200;

/** Per-base mismatch profile for coverage (heavier). */
export const MISMATCH_PROFILE_MAX_BP = 1_500;

/** Prefetch pad around viewport for sequence buffer. */
export const SEQ_PREFETCH_PAD_BP = 80_000;

/** Prefer at least this much sequence cached for smooth pan. */
export const SEQ_PREFETCH_CHUNK_BP = 200_000;

/** Pad when caching alignment reads (each side of viewport). */
export const READ_CACHE_PAD_BP = 40_000;

/** Max genomic span for a single read fetch. */
export const READ_FETCH_CHUNK_MAX_BP = 200_000;

/** Only request alignment reads when the viewport is at most this wide. */
export const READ_FETCH_MAX_BP = 500_000;

/** Pad when caching coverage bins (each side of viewport). */
export const COV_CACHE_PAD_BP = 80_000;

/** Max genomic span for a single coverage fetch. */
export const COV_FETCH_CHUNK_MAX_BP = 2_000_000;

/** Debounce feature/align refetch while panning (ms). */
export const FETCH_DEBOUNCE_PAN_MS = 120;
export const FETCH_DEBOUNCE_IDLE_MS = 30;
export const FEATURE_FETCH_DEBOUNCE_MS = 80;

/** HTML track-name gutter width (px). */
export const GUTTER_WIDTH = 108;

/** Plot area left/right padding inside canvas. */
export const PAD_L = 2;
export const PAD_R = 4;

/** Max squished read lanes before "+N more". */
export const MAX_READ_LANES = 200;

/** Below this px/bp, draw reads as solid bars (skip CIGAR detail). */
export const READ_DETAIL_MIN_PX_PER_BP = 0.35;

/** Read lane pitch and bar height. */
export const READ_LANE_H = 14;
export const READ_BAR_H = 10;
export const READS_HEADER_H = 18;

/** Feature lane pitch. */
export const FEATURE_LANE_H = 11;
export const FEATURE_BAR_H = 8;
export const MAX_FEATURE_LANES = 12;

/** Band heights. */
export const OVERVIEW_H = 10;
export const RULER_H = 20;
export const SEQ_BAND_H = 22;
export const COV_BAND_H = 40;
export const ANN_BAND_PAD = 6;

/** Zoom factors (IGV-like). */
export const ZOOM_IN_FACTOR = 0.5;
export const ZOOM_OUT_FACTOR = 2;

/** Keyboard pan step as fraction of visible span. */
export const KEY_PAN_FRACTION = 0.12;
