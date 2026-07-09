use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPreset {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub output_format: Option<String>,
    pub output_dir: Option<String>,
    pub compress: Option<bool>,
    pub reference_id: Option<String>,
    pub align_preset: Option<String>,
    pub sort_output: Option<bool>,
    pub index_output: Option<bool>,
    pub filter_unmapped: Option<bool>,
    pub mark_duplicates: Option<bool>,
}

pub fn sanitize_presets(mut presets: Vec<JobPreset>) -> Vec<JobPreset> {
    presets.retain(|preset| !preset.name.trim().is_empty() && !preset.mode.trim().is_empty());
    presets.truncate(32);
    presets
}