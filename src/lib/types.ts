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