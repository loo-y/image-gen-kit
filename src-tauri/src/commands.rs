use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose, Engine as _};

use crate::app_paths;
use crate::db;
use crate::providers::openai;
use crate::secrets;
use crate::types::{
    AppBootstrap, GenerateImageRequest, GenerationDetail, ListGenerationsRequest, ProviderProfile,
    SaveProviderProfileRequest,
};

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

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
pub fn reveal_image(path: String) -> Result<(), String> {
    let path = app_paths::ensure_path_in_images_dir(Path::new(&path))?;
    reveal_path(&path)
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
        "openai" => openai::generate(request, profile, api_key),
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
