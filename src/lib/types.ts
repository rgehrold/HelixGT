export interface FormatInfo {
  id: string;
  label: string;
  category: "sequence" | "alignment" | "annotation" | "variants";
}

export interface ConvertProgress {
  current: number;
  total: number;
  fileName: string;
}

export interface ConvertResultItem {
  inputPath: string;
  outputPath: string;
  records: number;
}

export interface ConvertSummary {
  files: ConvertResultItem[];
  totalRecords: number;
  partialFailure?: boolean;
  errorMessage?: string | null;
}

export interface DirEntry {
  name: string;
  path: string;
  isDir: boolean;
}

export interface MergeValidation {
  extension: string;
  format: string | null;
  isValid: boolean;
  message: string;
}

export interface MergeSuggestion {
  suggestedName: string;
  commonPrefix: string;
  commonSuffix: string;
  similar: boolean;
}

export interface MergeSummary {
  outputPath: string;
  records: number;
  inputCount: number;
}

export interface MergeProgress {
  records: number;
  message: string;
}

export interface EnsemblReferenceInfo {
  id: string;
  label: string;
  species: string;
  assembly: string;
  release: string;
  compressedSizeMb: number;
}

export interface ReferenceFetchResult {
  id: string;
  label: string;
  sizeBytes: number;
  sequenceBytes: number;
  gzipped: boolean;
  cached: boolean;
}

export interface AlignSummary {
  outputPath: string;
  indexPath?: string | null;
  mappedReads: number;
  totalReads: number;
  aligner: string;
}

export type ToolMode = "convert" | "merge" | "align" | "view" | "analyze";

/** Contig/chromosome in a sequence document (View mode). */
export interface ContigInfo {
  name: string;
  length: number;
}

/** Opened sequence file metadata for View mode. */
export interface SequenceDocument {
  path: string;
  format: string;
  gzipped: boolean;
  contigs: ContigInfo[];
  totalBases: number;
  /** When true, sequences are held in memory for smooth panning. */
  fullyCached?: boolean;
}

/**
 * Sequence slice for a genomic window.
 * Coordinates are 0-based half-open [start, end).
 */
export interface SequenceSlice {
  contig: string;
  start: number;
  end: number;
  sequence: string;
  contigLength: number;
  reverseComplemented: boolean;
}

export interface AnnotationContigSpan {
  name: string;
  length: number;
}

export interface AnnotationDocument {
  path: string;
  format: string;
  gzipped: boolean;
  featureCount: number;
  contigs: string[];
  contigSpans: AnnotationContigSpan[];
}

/** Feature interval; coordinates 0-based half-open [start, end). */
export interface AnnotationFeature {
  id: number;
  contig: string;
  start: number;
  end: number;
  name: string;
  featureType: string;
  strand: string;
  source: string;
  score?: number | null;
}

export interface FeatureWindow {
  contig: string;
  start: number;
  end: number;
  features: AnnotationFeature[];
  truncated: boolean;
  totalInRange: number;
}

export interface AlignmentContig {
  name: string;
  length: number;
}

export interface AlignmentDocument {
  path: string;
  format: string;
  indexed: boolean;
  contigs: AlignmentContig[];
  requiresReference: boolean;
}

export interface CoverageBin {
  start: number;
  end: number;
  depth: number;
}

export interface CoverageWindow {
  contig: string;
  start: number;
  end: number;
  bins: CoverageBin[];
  maxDepth: number;
}

export interface CigarOp {
  op: string;
  length: number;
}

export interface AlignmentRead {
  name: string;
  start: number;
  end: number;
  strand: string;
  mapq: number;
  cigar?: string;
  cigarOps?: CigarOp[];
  flags: number;
  sequence?: string;
  qualities?: string;
  isPaired: boolean;
  isProperPair: boolean;
  isUnmapped: boolean;
  isMateUnmapped: boolean;
  isReverse: boolean;
  isSecondary: boolean;
  isSupplementary: boolean;
  isDuplicate: boolean;
  isQcFail: boolean;
  isFirstInPair: boolean;
  isSecondInPair: boolean;
  templateLength: number;
  mateContig?: string;
  mateStart?: number | null;
}

export interface AlignmentWindow {
  coverage: CoverageWindow;
  reads: ReadsWindow;
}

export interface ReadsWindow {
  contig: string;
  start: number;
  end: number;
  reads: AlignmentRead[];
  truncated: boolean;
  totalInRange: number;
}

export type LogLevel = "info" | "warn" | "error";

export interface LogEntry {
  level: LogLevel;
  message: string;
  /** Local wall-clock time when the line was recorded (HH:MM:SS). */
  time?: string;
}

export interface ReferenceDownloadProgress {
  id: string;
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number | null;
}

export interface ToolLogEvent {
  tool: string;
  stream: string;
  line: string;
}

export interface PreflightIssue {
  severity: "error" | "warning";
  message: string;
}

export interface PreflightResponse {
  ok: boolean;
  issues: PreflightIssue[];
  estimatedOutputBytes: number;
}

export interface FastqQcSummary {
  readCount: number;
  totalBases: number;
  meanReadLength: number;
  minReadLength: number;
  maxReadLength: number;
  nContentFraction: number;
}

export interface JobPreset {
  id: string;
  name: string;
  mode: ToolMode;
  outputFormat?: string;
  outputDir?: string;
  compress?: boolean;
  referenceId?: string;
  alignPreset?: string;
  sortOutput?: boolean;
  indexOutput?: boolean;
  filterUnmapped?: boolean;
  markDuplicates?: boolean;
}

export interface UserPreferences {
  browseRoot?: string;
  expandedDirs?: string[];
  filesPaneWidth?: number;
  /** When true, the input browser is stashed (hidden). */
  filesPaneCollapsed?: boolean;
  activeMode?: ToolMode;
  convertOutputDir?: string;
  mergeOutputDir?: string;
  alignOutputDir?: string;
  outputFormat?: string;
  alignReferenceId?: string;
  alignPreset?: string;
  alignOutputFormat?: string;
  recentFiles?: string[];
  jobPresets?: JobPreset[];
}