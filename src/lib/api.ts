import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AlignSummary,
  AlignmentDocument,
  AnnotationDocument,
  ConvertProgress,
  ConvertSummary,
  CoverageWindow,
  FastqQcSummary,
  FeatureWindow,
  FormatInfo,
  MergeProgress,
  MergeSuggestion,
  MergeSummary,
  MergeValidation,
  PreflightResponse,
  ReadsWindow,
  ReferenceFetchResult,
  SequenceDocument,
  SequenceSlice,
  ToolLogEvent,
  UserPreferences,
} from "$lib/types";

export async function getSupportedFormats() {
  return invoke<FormatInfo[]>("get_supported_formats");
}

export async function suggestOutputFormat(paths: string[]) {
  return invoke<string | null>("suggest_output_format_for_paths", { paths });
}

export async function runPreflightCheck(request: {
  mode: string;
  inputPaths: string[];
  outputDir: string;
  outputFormat?: string | null;
  referencePath?: string | null;
}) {
  return invoke<PreflightResponse>("run_preflight_check", { request });
}

export async function runFastqQc(path: string) {
  return invoke<FastqQcSummary>("run_fastq_qc", { path });
}

export async function cancelJob(jobId: string) {
  return invoke<boolean>("cancel_job", { jobId });
}

export async function runConversion(request: {
  inputPaths: string[];
  outputDir: string;
  outputFormat: string;
  compress: boolean;
  prefix?: string | null;
  referencePath?: string | null;
  jobId?: string | null;
}) {
  return invoke<ConvertSummary>("run_conversion", { request });
}

export async function runMerge(request: {
  inputPaths: string[];
  outputDir: string;
  outputName: string;
  compress?: boolean | null;
  referencePath?: string | null;
  jobId?: string | null;
}) {
  return invoke<MergeSummary>("run_merge", { request });
}

export async function runAlignment(request: Record<string, unknown>) {
  return invoke<AlignSummary>("run_alignment", { request });
}

export async function validateMergePaths(paths: string[]) {
  return invoke<MergeValidation>("validate_merge_paths", { paths });
}

export async function suggestMergedFilename(paths: string[]) {
  return invoke<MergeSuggestion>("suggest_merged_filename", { paths });
}

export async function loadUserPreferences() {
  return invoke<UserPreferences>("load_user_preferences");
}

export async function saveUserPreferences(prefs: UserPreferences) {
  return invoke("save_user_preferences", { prefs });
}

export function onConvertProgress(handler: (payload: ConvertProgress) => void) {
  return listen<ConvertProgress>("convert-progress", (event) => handler(event.payload));
}

export function onMergeProgress(handler: (payload: MergeProgress) => void) {
  return listen<MergeProgress>("merge-progress", (event) => handler(event.payload));
}

export function onToolLog(handler: (payload: ToolLogEvent) => void) {
  return listen<ToolLogEvent>("tool-log", (event) => handler(event.payload));
}

export async function loadLocalReference(id: string, path: string) {
  return invoke<ReferenceFetchResult>("load_local_reference", { id, path });
}

export async function viewOpenDocument(path: string) {
  return invoke<SequenceDocument>("view_open_document", { path });
}

export async function viewGetSequenceWindow(request: {
  path: string;
  contig: string;
  start: number;
  end: number;
  reverseComplement?: boolean;
}) {
  return invoke<SequenceSlice>("view_get_sequence_window", { request });
}

export async function viewOpenAnnotation(path: string) {
  return invoke<AnnotationDocument>("view_open_annotation", { path });
}

export async function viewGetFeaturesInRange(request: {
  path: string;
  contig: string;
  start: number;
  end: number;
}) {
  return invoke<FeatureWindow>("view_get_features_in_range", { request });
}

export async function viewOpenAlignment(request: {
  path: string;
  referencePath?: string | null;
}) {
  return invoke<AlignmentDocument>("view_open_alignment", { request });
}

export async function viewGetCoverageBins(request: {
  path: string;
  contig: string;
  start: number;
  end: number;
  binCount?: number;
  referencePath?: string | null;
}) {
  return invoke<CoverageWindow>("view_get_coverage_bins", { request });
}

export async function viewGetReadsInRange(request: {
  path: string;
  contig: string;
  start: number;
  end: number;
  referencePath?: string | null;
  includeSecondary?: boolean;
  includeSupplementary?: boolean;
  includeDuplicates?: boolean;
  minMapq?: number;
  /** Omit base/quality strings when false (faster for zoomed-out pileups). */
  includeSequences?: boolean;
}) {
  return invoke<ReadsWindow>("view_get_reads_in_range", { request });
}