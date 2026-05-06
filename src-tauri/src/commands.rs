use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;

use crate::app_paths;
use crate::db;
use crate::providers::openai;
use crate::secrets;
use crate::types::{
    AppBootstrap, GenerateImageRequest, GenerationDetail, ListGenerationsRequest, ProviderProfile,
    SaveProviderProfileRequest, StartedGeneration,
};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);
const MAX_INPUT_IMAGE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputImageDataUrl {
    name: String,
    mime_type: String,
    data_url: String,
    size: u64,
}

#[tauri::command]
pub fn init_app() -> Result<AppBootstrap, String> {
    db::init_database()?;
    Ok(AppBootstrap {
        profiles: db::list_profiles()?,
        generations: db::list_generation_details(None, 40, 0)?,
    })
}

#[tauri::command]
pub fn list_provider_profiles() -> Result<Vec<ProviderProfile>, String> {
    db::list_profiles()
}

#[tauri::command]
pub fn save_provider_profile(
    request: SaveProviderProfileRequest,
) -> Result<ProviderProfile, String> {
    let now = now_millis();
    let id = request.id.unwrap_or_else(|| make_id("profile"));
    let app_data_dir = app_paths::app_data_dir()?;
    let existing = db::get_profile(&id)?;
    let api_key = request.api_key.unwrap_or_default();
    let api_key_ref = if request.save_api_key {
        if !api_key.trim().is_empty() {
            secrets::save_api_key(&app_data_dir, &id, api_key.trim())?;
            Some(format!("profile:{id}"))
        } else {
            existing
                .and_then(|profile| profile.api_key_ref)
                .filter(|value| !value.is_empty())
        }
    } else {
        secrets::delete_api_key(&app_data_dir, &id)?;
        None
    };

    db::upsert_profile(
        &id,
        request.name.trim(),
        request.provider_type.trim(),
        request.base_url.trim(),
        request.model_default.trim(),
        normalize_timeout_minutes(request.network_timeout_minutes),
        api_key_ref.as_deref(),
        now,
    )
}

#[tauri::command]
pub fn list_generations(
    request: Option<ListGenerationsRequest>,
) -> Result<Vec<GenerationDetail>, String> {
    let request = request.unwrap_or(ListGenerationsRequest {
        query: None,
        limit: Some(40),
        offset: Some(0),
    });
    let limit = request.limit.unwrap_or(40).clamp(1, 200);
    let offset = request.offset.unwrap_or(0).max(0);
    db::list_generation_details(request.query.as_deref(), limit, offset)
}

#[tauri::command]
pub fn get_generation(id: String) -> Result<Option<GenerationDetail>, String> {
    db::get_generation_detail(&id)
}

#[tauri::command]
pub async fn generate_image(request: GenerateImageRequest) -> Result<GenerationDetail, String> {
    tauri::async_runtime::spawn_blocking(move || generate_image_blocking(request))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn start_generation(request: GenerateImageRequest) -> Result<StartedGeneration, String> {
    let job = prepare_openai_job(request)?;
    let generation_id = job.generation.id.clone();
    let generation = job.generation.clone();
    db::insert_generation(&generation)?;
    let initial = GenerationDetail {
        generation,
        outputs: Vec::new(),
        input_images: Vec::new(),
    };

    tauri::async_runtime::spawn_blocking(move || {
        let _ = openai::run_existing_job(job);
    });

    Ok(StartedGeneration {
        generation_id,
        generation: initial,
    })
}

#[tauri::command]
pub fn read_image_data_url(path: String) -> Result<String, String> {
    let path = app_paths::ensure_path_in_images_dir(Path::new(&path))?;
    let bytes = std::fs::read(&path).map_err(|err| err.to_string())?;
    let mime = match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub fn read_input_image_data_urls(paths: Vec<String>) -> Result<Vec<InputImageDataUrl>, String> {
    paths
        .into_iter()
        .map(|path| read_input_image_data_url(PathBuf::from(path)))
        .collect()
}

#[tauri::command]
pub fn reveal_image(path: String) -> Result<(), String> {
    let path = app_paths::ensure_path_in_images_dir(Path::new(&path))?;
    reveal_path(&path)
}

fn read_input_image_data_url(path: PathBuf) -> Result<InputImageDataUrl, String> {
    let metadata = std::fs::metadata(&path).map_err(|err| err.to_string())?;
    if !metadata.is_file() {
        return Err(format!("Dropped path is not a file: {}", path.display()));
    }
    if metadata.len() > MAX_INPUT_IMAGE_BYTES {
        return Err("Each input image must be 50MB or smaller".to_string());
    }

    let mime_type = input_image_mime_type(&path)?;
    let bytes = std::fs::read(&path).map_err(|err| err.to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("input")
        .to_string();

    Ok(InputImageDataUrl {
        name,
        mime_type: mime_type.to_string(),
        data_url: format!(
            "data:{mime_type};base64,{}",
            general_purpose::STANDARD.encode(bytes)
        ),
        size: metadata.len(),
    })
}

fn input_image_mime_type(path: &Path) -> Result<&'static str, String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        _ => Err("Input images must be PNG, JPEG, or WebP".to_string()),
    }
}

#[tauri::command]
pub fn open_image(path: String) -> Result<(), String> {
    let path = app_paths::ensure_path_in_images_dir(Path::new(&path))?;
    open_path(&path)
}

#[tauri::command]
pub fn open_images_dir() -> Result<(), String> {
    let path = app_paths::images_dir()?;
    open_path(&path)
}

#[tauri::command]
pub fn reveal_debug_dir() -> Result<(), String> {
    let path = app_paths::debug_dir()?;
    reveal_path(&path)
}

#[tauri::command]
pub fn delete_generation(id: String) -> Result<(), String> {
    let paths = db::delete_generation(&id)?;
    for path in paths {
        let path = PathBuf::from(path);
        if let Ok(path) = app_paths::ensure_path_in_images_dir(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(())
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn make_id(prefix: &str) -> String {
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{counter}", now_millis())
}

fn generate_image_blocking(request: GenerateImageRequest) -> Result<GenerationDetail, String> {
    let job = prepare_openai_job(request)?;
    openai::run_job(job)
}

fn normalize_timeout_minutes(value: Option<i64>) -> i64 {
    value.unwrap_or(15).clamp(1, 120)
}

fn prepare_openai_job(request: GenerateImageRequest) -> Result<openai::OpenAiJob, String> {
    let mut profile = db::get_profile(&request.provider_id)?
        .or_else(|| db::first_profile().ok())
        .ok_or_else(|| "Provider profile was not found".to_string())?;

    if let Some(base_url) = request
        .base_url
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        profile.profile.base_url = base_url.trim().to_string();
    }

    let app_data_dir = app_paths::app_data_dir()?;
    let api_key = match request
        .api_key_override
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(value) => value,
        None => secrets::get_api_key(&app_data_dir, &profile.profile.id)?
            .ok_or_else(|| "API key is required".to_string())?,
    };

    match profile.profile.provider_type.as_str() {
        "openai" => openai::create_job(request, profile, api_key),
        other => Err(format!("Provider type '{other}' is not implemented yet")),
    }
}

fn reveal_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|err| err.to_string())?;

    #[cfg(target_os = "windows")]
    let status = Command::new("explorer")
        .arg(format!("/select,{}", path.to_string_lossy()))
        .status()
        .map_err(|err| err.to_string())?;

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let status = Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .status()
        .map_err(|err| err.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err("Unable to reveal image in the file manager".to_string())
    }
}

fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open")
        .arg(path)
        .status()
        .map_err(|err| err.to_string())?;

    #[cfg(target_os = "windows")]
    let status = Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .status()
        .map_err(|err| err.to_string())?;

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let status = Command::new("xdg-open")
        .arg(path)
        .status()
        .map_err(|err| err.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err("Unable to open path".to_string())
    }
}
