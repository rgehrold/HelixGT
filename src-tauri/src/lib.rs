mod filesystem;
mod reference;

use std::path::PathBuf;
use std::sync::Arc;

use converter_core::{
    align_reads_to_reference, batch_convert, is_compatible_file, merge_files,
    output_alignment_path, resolve_minimap2_path, resolve_samtools_path, suggest_merge_filename,
    validate_merge_inputs, AlignOptions, AlignOutputFormat, ConvertOptions, ConvertedFile,
    FileFormat, MergeOptions, Minimap2Options, Minimap2Preset, ToolLogSink,
};
use reference::{
    CachedReferenceInfo, EnsemblReferenceInfo, ReferenceFetchResult, ReferenceStore,
    SharedReferenceStore,
};
use filesystem::{
    collect_compatible_files, create_folder, default_browse_root, delete_path,
    folder_child_count, folder_has_compatible_files, list_directory, rename_path, DirEntry,
};
use serde::Serialize;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FormatInfo {
    id: String,
    label: String,
    category: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertProgress {
    current: usize,
    total: usize,
    file_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertResultItem {
    input_path: String,
    output_path: String,
    records: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertSummary {
    files: Vec<ConvertResultItem>,
    total_records: u64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConvertRequest {
    input_paths: Vec<String>,
    output_dir: String,
    output_format: String,
    compress: bool,
    prefix: Option<String>,
    reference_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeValidationResponse {
    extension: String,
    format: Option<String>,
    is_valid: bool,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeSuggestionResponse {
    suggested_name: String,
    common_prefix: String,
    common_suffix: String,
    similar: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeSummary {
    output_path: String,
    records: u64,
    input_count: usize,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeRequest {
    input_paths: Vec<String>,
    output_dir: String,
    output_name: String,
    compress: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlignSummary {
    output_path: String,
    index_path: Option<String>,
    mapped_reads: u64,
    total_reads: u64,
    aligner: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolLogEvent {
    tool: String,
    stream: String,
    line: String,
}

struct TauriToolLog {
    app: AppHandle,
}

impl ToolLogSink for TauriToolLog {
    fn log_line(&self, tool: &str, stream: &str, line: &str) {
        let _ = self.app.emit(
            "tool-log",
            ToolLogEvent {
                tool: tool.to_string(),
                stream: stream.to_string(),
                line: line.to_string(),
            },
        );
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlignRequest {
    read_paths: Vec<String>,
    output_dir: String,
    output_stem: String,
    output_format: String,
    compress: bool,
    reference_id: String,
    preset: String,
    index_reference: bool,
    sort_output: bool,
    secondary_alignments: bool,
    index_output: bool,
}

#[tauri::command]
fn get_supported_formats() -> Vec<FormatInfo> {
    FileFormat::ALL
        .into_iter()
        .map(|format| FormatInfo {
            id: format.as_str().to_string(),
            label: format.as_str().to_uppercase(),
            category: if format.is_alignment() {
                "alignment".into()
            } else if format == FileFormat::Vcf {
                "variants".into()
            } else if matches!(format, FileFormat::Gff | FileFormat::Bed) {
                "annotation".into()
            } else {
                "sequence".into()
            },
        })
        .collect()
}

#[tauri::command]
fn get_default_browse_root() -> String {
    default_browse_root().display().to_string()
}

#[tauri::command]
fn list_browse_directory(path: String) -> Result<Vec<DirEntry>, String> {
    list_directory(PathBuf::from(path).as_path())
}

#[tauri::command]
fn collect_compatible_files_under(path: String) -> Result<Vec<String>, String> {
    collect_compatible_files(PathBuf::from(path).as_path())
}

#[tauri::command]
fn browse_folder_has_compatible_files(path: String) -> Result<bool, String> {
    folder_has_compatible_files(PathBuf::from(path).as_path())
}

#[tauri::command]
fn rename_browse_path(path: String, new_name: String) -> Result<String, String> {
    rename_path(PathBuf::from(path).as_path(), &new_name)
}

#[tauri::command]
fn delete_browse_path(path: String) -> Result<(), String> {
    delete_path(PathBuf::from(path).as_path())
}

#[tauri::command]
fn browse_folder_child_count(path: String) -> Result<u64, String> {
    folder_child_count(PathBuf::from(path).as_path())
}

#[tauri::command]
fn create_browse_folder(parent_path: String, name: Option<String>) -> Result<String, String> {
    create_folder(
        PathBuf::from(parent_path).as_path(),
        name.as_deref(),
    )
}

#[tauri::command]
fn validate_input_paths(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| is_compatible_file(PathBuf::from(path).as_path()))
        .collect()
}

#[tauri::command]
async fn run_conversion(app: AppHandle, request: ConvertRequest) -> Result<ConvertSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let output_format = request
            .output_format
            .parse::<FileFormat>()
            .map_err(|error| format!("{error:#}"))?;

        let input_paths: Vec<PathBuf> = request
            .input_paths
            .into_iter()
            .map(PathBuf::from)
            .collect();

        if input_paths.is_empty() {
            return Err("no input files selected".into());
        }

        let options = ConvertOptions {
            output_format,
            compress: request.compress,
            prefix: request.prefix.unwrap_or_default(),
            reference_path: request.reference_path.map(PathBuf::from),
        };

        let output_dir = PathBuf::from(&request.output_dir);

        let results = batch_convert(&input_paths, &output_dir, &options, |current, total, name| {
            let _ = app.emit(
                "convert-progress",
                ConvertProgress {
                    current,
                    total,
                    file_name: name.to_string(),
                },
            );
        })
        .map_err(|error| format!("{error:#}"))?;

        Ok(summary_from_results(results))
    })
    .await
    .map_err(|error| format!("conversion task failed: {error}"))?
}

#[tauri::command]
fn validate_merge_paths(paths: Vec<String>) -> MergeValidationResponse {
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let validation = validate_merge_inputs(&path_bufs);
    MergeValidationResponse {
        extension: validation.extension,
        format: validation.format.map(|format| format.as_str().to_string()),
        is_valid: validation.is_valid,
        message: validation.message,
    }
}

#[tauri::command]
fn suggest_merged_filename(paths: Vec<String>) -> MergeSuggestionResponse {
    let path_bufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let suggestion = suggest_merge_filename(&path_bufs);
    MergeSuggestionResponse {
        suggested_name: suggestion.suggested_name,
        common_prefix: suggestion.common_prefix,
        common_suffix: suggestion.common_suffix,
        similar: suggestion.similar,
    }
}

#[tauri::command]
async fn run_merge(request: MergeRequest) -> Result<MergeSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let input_paths: Vec<PathBuf> = request
            .input_paths
            .into_iter()
            .map(PathBuf::from)
            .collect();
        let result = merge_files(
            &input_paths,
            PathBuf::from(&request.output_dir).as_path(),
            &MergeOptions {
                output_name: request.output_name,
                compress: request.compress,
            },
        )
        .map_err(|error| format!("{error:#}"))?;

        Ok(MergeSummary {
            output_path: result.output_path.display().to_string(),
            records: result.records,
            input_count: result.input_count,
        })
    })
    .await
    .map_err(|error| format!("merge task failed: {error}"))?
}

#[tauri::command]
fn list_ensembl_references() -> Vec<EnsemblReferenceInfo> {
    ReferenceStore::list_catalog()
}

#[tauri::command]
async fn fetch_ensembl_reference(
    app: AppHandle,
    store: tauri::State<'_, SharedReferenceStore>,
    id: String,
) -> Result<ReferenceFetchResult, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.fetch_with_progress(&app, &id))
        .await
        .map_err(|error| format!("download task failed: {error}"))?
}

#[tauri::command]
async fn load_local_reference(
    store: tauri::State<'_, SharedReferenceStore>,
    id: String,
    path: String,
) -> Result<ReferenceFetchResult, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.load_local_file(&id, &path))
        .await
        .map_err(|error| format!("load task failed: {error}"))?
}

#[tauri::command]
fn get_reference_cache(store: tauri::State<'_, SharedReferenceStore>) -> Result<Vec<CachedReferenceInfo>, String> {
    Ok(store.cache_status())
}

#[tauri::command]
fn clear_reference_cache(
    store: tauri::State<'_, SharedReferenceStore>,
    id: Option<String>,
) -> Result<(), String> {
    store.clear(id.as_deref());
    Ok(())
}

#[tauri::command]
async fn save_reference(
    store: tauri::State<'_, SharedReferenceStore>,
    id: String,
    path: String,
    decompress: bool,
) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.save_reference(&id, &path, decompress))
        .await
        .map_err(|error| format!("save task failed: {error}"))?
}

fn bundled_tool_candidates(app: &AppHandle, file_name: &str) -> Vec<PathBuf> {
    let rel = format!("binaries/{file_name}");
    match app.path().resolve(&rel, BaseDirectory::Resource) {
        Ok(path) => vec![path],
        Err(_) => Vec::new(),
    }
}

fn resolve_minimap2_for_app(app: &AppHandle) -> Option<PathBuf> {
    let name = if cfg!(windows) { "minimap2.exe" } else { "minimap2" };
    resolve_minimap2_path(&bundled_tool_candidates(app, name))
}

fn resolve_samtools_for_app(app: &AppHandle) -> Option<PathBuf> {
    let name = if cfg!(windows) { "samtools.exe" } else { "samtools" };
    resolve_samtools_path(&bundled_tool_candidates(app, name))
}

#[tauri::command]
fn minimap2_is_available(app: AppHandle) -> bool {
    resolve_minimap2_for_app(&app).is_some()
}

#[tauri::command]
fn samtools_is_available(app: AppHandle) -> bool {
    resolve_samtools_for_app(&app).is_some()
}

#[tauri::command]
async fn run_alignment(
    app: AppHandle,
    store: tauri::State<'_, SharedReferenceStore>,
    request: AlignRequest,
) -> Result<AlignSummary, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_alignment_inner(&app, &store, request))
        .await
        .map_err(|error| format!("alignment task failed: {error}"))?
}

fn run_alignment_inner(
    app: &AppHandle,
    store: &ReferenceStore,
    request: AlignRequest,
) -> Result<AlignSummary, String> {
    let read_paths: Vec<PathBuf> = request.read_paths.into_iter().map(PathBuf::from).collect();
    if read_paths.is_empty() {
        return Err("select at least one FASTA or FASTQ read file".into());
    }

    let minimap2_exe = resolve_minimap2_for_app(app)
        .ok_or_else(|| "minimap2 not found. Place minimap2.exe in src-tauri/binaries/ or add it to PATH.".to_string())?;
    let samtools_exe = resolve_samtools_for_app(app)
        .ok_or_else(|| "samtools not found. Place samtools.exe in src-tauri/binaries/ or add it to PATH.".to_string())?;

    let output_format = AlignOutputFormat::from_str(&request.output_format)
        .map_err(|error| format!("{error:#}"))?;
    let output_dir = PathBuf::from(&request.output_dir);
    std::fs::create_dir_all(&output_dir).map_err(|error| format!("cannot create output folder: {error}"))?;

    let reference_path = store.reference_path(&request.reference_id)?;

    let output_path = output_alignment_path(
        output_dir.as_path(),
        &request.output_stem,
        output_format,
        request.compress,
    );

    let tool_log = Arc::new(TauriToolLog { app: app.clone() });
    let result = align_reads_to_reference(
        &read_paths,
        &reference_path,
        &output_path,
        &AlignOptions {
            output_format,
            compress: request.compress,
            minimap2_exe,
            minimap2: Minimap2Options {
                preset: Minimap2Preset::from_str(&request.preset),
                index_reference: request.index_reference,
                sort_output: request.sort_output,
                secondary_alignments: request.secondary_alignments,
                index_output: request.index_output,
            },
            samtools_exe,
            tool_log: Some(tool_log),
        },
    )
    .map_err(|error| format!("{error:#}"))?;

    Ok(AlignSummary {
        output_path: result.output_path.display().to_string(),
        index_path: result
            .index_path
            .map(|path| path.display().to_string()),
        mapped_reads: result.mapped_reads,
        total_reads: result.total_reads,
        aligner: result.aligner,
    })
}

fn summary_from_results(results: Vec<ConvertedFile>) -> ConvertSummary {
    let total_records = results.iter().map(|item| item.records).sum();
    ConvertSummary {
        files: results
            .into_iter()
            .map(|item| ConvertResultItem {
                input_path: item.input_path.display().to_string(),
                output_path: item.output_path.display().to_string(),
                records: item.records,
            })
            .collect(),
        total_records,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(ReferenceStore::new()))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_supported_formats,
            get_default_browse_root,
            list_browse_directory,
            collect_compatible_files_under,
            browse_folder_has_compatible_files,
            rename_browse_path,
            delete_browse_path,
            browse_folder_child_count,
            create_browse_folder,
            validate_input_paths,
            run_conversion,
            validate_merge_paths,
            suggest_merged_filename,
            run_merge,
            list_ensembl_references,
            fetch_ensembl_reference,
            load_local_reference,
            get_reference_cache,
            clear_reference_cache,
            save_reference,
            minimap2_is_available,
            samtools_is_available,
            run_alignment
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}