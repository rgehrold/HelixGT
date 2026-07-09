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

export type ToolMode = "convert" | "merge" | "align";

export type LogLevel = "info" | "warn" | "error";

export interface LogEntry {
  level: LogLevel;
  message: string;
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