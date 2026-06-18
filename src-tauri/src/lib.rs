mod filesystem;

use std::path::PathBuf;

use converter_core::{
    batch_convert, is_compatible_file, ConvertOptions, ConvertedFile, FileFormat,
};
use filesystem::{
    collect_compatible_files, create_folder, default_browse_root, delete_path,
    folder_child_count, folder_has_compatible_files, list_directory, rename_path, DirEntry,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

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
fn run_conversion(app: AppHandle, request: ConvertRequest) -> Result<ConvertSummary, String> {
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
            run_conversion
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}