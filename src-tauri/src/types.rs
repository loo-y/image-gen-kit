use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model_default: String,
    pub network_timeout_minutes: i64,
    pub api_key_saved: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderProfileRequest {
    pub id: Option<String>,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model_default: String,
    pub network_timeout_minutes: Option<i64>,
    pub api_key: Option<String>,
    pub save_api_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub profiles: Vec<ProviderProfile>,
    pub generations: Vec<GenerationDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Generation {
    pub id: String,
    pub prompt: String,
    pub provider_id: String,
    pub provider_type: String,
    pub provider_name: String,
    pub model: String,
    pub status: String,
    pub size: String,
    pub quality: String,
    pub output_format: String,
    pub params_json: String,
    pub response_json: Option<String>,
    pub error_message: Option<String>,
    pub revised_prompt: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationOutput {
    pub id: i64,
    pub generation_id: String,
    pub path: String,
    pub format: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: i64,
    pub output_index: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationInputImage {
    pub id: i64,
    pub generation_id: String,
    pub path: String,
    pub content_hash: String,
    pub name: String,
    pub mime_type: String,
    pub file_size: i64,
    pub input_index: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationDetail {
    pub generation: Generation,
    pub outputs: Vec<GenerationOutput>,
    pub input_images: Vec<GenerationInputImage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedGeneration {
    pub generation_id: String,
    pub generation: GenerationDetail,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListGenerationsRequest {
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateImageRequest {
    pub provider_id: String,
    pub api_key_override: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub prompt: String,
    pub size: String,
    pub quality: String,
    pub xai_resolution: Option<String>,
    pub n: Option<i64>,
    pub output_format: String,
    pub output_compression: Option<i64>,
    pub moderation: Option<String>,
    pub debug_mode: Option<bool>,
    pub network_timeout_minutes: Option<i64>,
    pub input_images: Option<Vec<InputImageRequest>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputImageRequest {
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub data_url: String,
}

#[derive(Debug, Clone)]
pub struct StoredProviderProfile {
    pub profile: ProviderProfile,
    pub api_key_ref: Option<String>,
}
