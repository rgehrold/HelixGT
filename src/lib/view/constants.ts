/** Shared constants for the IGV-style View browser (performance-first). */

/**
 * Font for nucleotide letters (reference track + mismatch labels).
 */
export const SEQ_LETTER_FONT =
  '"JetBrains Mono", ui-monospace, "Cascadia Mono", Consolas, monospace';

/** Min visible span (bp). */
export const MIN_VISIBLE_BP = 20;

/** Default window when opening large contigs. */
export const DEFAULT_VISIBLE_BP_LARGE = 50_000;

/** Max sequence window request (matches converter-core MAX_SEQUENCE_WINDOW). */
export const MAX_SEQUENCE_WINDOW_BP = 500_000;

/** Draw nucleotide letters on the reference track at or below this width. */
export const BASE_LETTERS_MAX_BP = 250;

/**
 * Fetch read sequences/qualities only when this zoomed-in.
 * Sequences dominate IPC + paint cost — keep this tight.
 */
export const READ_SEQUENCES_MAX_BP = 1_500;

/**
 * Coverage histogram: always ~1 bin per pixel (never per-base engine scans).
 * Kept for API compat with fetch helpers.
 */
export const PER_BASE_COVERAGE_MAX_BP = 0;

/** Hard cap on coverage bins per request (~screen width). */
export const MAX_COVERAGE_BINS = 1_024;

/**
 * Floor for auto coverage Y-scale (viewport-local max, at least this).
 * Low-depth samples still show visible bars.
 */
export const COVERAGE_Y_FLOOR = 8;

/**
 * Fallback / lock scale when auto max is unavailable.
 * barHeight = min(trackH, depth / scale * trackH)
 */
export const COVERAGE_FIXED_SCALE = 30;

/** Whole-contig overview sparkline bins. */
export const OVERVIEW_BINS = 240;

/** Compact alignments strip when zoomed out past pileup. */
export const DENSITY_STRIP_H = 28;

/** Prefetch pad for reference sequence (smaller = snappier pan). */
export const SEQ_PREFETCH_PAD_BP = 40_000;

/** Preferred sequence cache chunk (must stay modest for IPC). */
export const SEQ_PREFETCH_CHUNK_BP = 120_000;

/** Pad as fraction of visible span each side (reads / features). */
export const VIEW_SIDE_PAD_FRACTION = 0.5;

/**
 * How many viewport-widths of coverage to keep cached for pan.
 * At default 50 kb open this is ~200 kb of cheap pixel bins — not full reads.
 */
export const COV_CACHE_VIEWPORTS = 4;

/** Pad when caching alignment reads (each side). Only used when pileup is active. */
export const READ_CACHE_PAD_BP = 6_000;

/** Max genomic span for a single read fetch. */
export const READ_FETCH_CHUNK_MAX_BP = 24_000;

/**
 * Only request pileup reads when viewport is at most this wide.
 * Default open is 50 kb → coverage-only (IGV-like). Individual reads at
 * 50 kb are a few px wide and cost thousands of IPC records for little gain.
 */
export const READ_FETCH_MAX_BP = 12_000;

/** Absolute floor for coverage pad each side (bp). */
export const COV_CACHE_PAD_BP = 80_000;

/** Max genomic span for a single coverage fetch (bins stay ~1/pixel). */
export const COV_FETCH_CHUNK_MAX_BP = 2_000_000;

/** Debounce while panning / idle. */
export const FETCH_DEBOUNCE_PAN_MS = 150;
/** After continuous nav (wheel/keys) stops, wait this long before any disk fetch. */
export const NAV_IDLE_MS = 220;
export const FETCH_DEBOUNCE_IDLE_MS = 80;
export const FEATURE_FETCH_DEBOUNCE_MS = 150;

export const GUTTER_WIDTH = 108;
export const PAD_L = 2;
export const PAD_R = 4;

/**
 * Cap pileup lanes (and canvas height). Tall canvases destroy paint FPS.
 * Extra reads are reported as “+N hidden”.
 */
export const MAX_READ_LANES = 80;

/** Below this px/bp, always solid bars (no CIGAR). */
export const READ_DETAIL_MIN_PX_PER_BP = 0.8;

/**
 * Mismatch ticks/letters on reads only at/below this span,
 * and only when sequences are loaded.
 */
export const READ_MISMATCH_MAX_BP = 1_200;

export const READ_LANE_H = 16;
export const READ_BAR_H = 12;
export const READS_HEADER_H = 16;

export const FEATURE_LANE_H = 11;
export const FEATURE_BAR_H = 8;
export const MAX_FEATURE_LANES = 8;

export const OVERVIEW_H = 10;
export const RULER_H = 20;
export const SEQ_BAND_H = 24;
export const COV_BAND_H = 44;
export const ANN_BAND_PAD = 6;
export const COLLAPSED_TRACK_H = 16;

export const ZOOM_IN_FACTOR = 0.5;
export const ZOOM_OUT_FACTOR = 2;
export const KEY_PAN_FRACTION = 0.12;
