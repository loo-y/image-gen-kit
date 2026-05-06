use std::path::Path;
use std::process::Command;

#[cfg(not(target_os = "macos"))]
use std::collections::HashMap;

const SERVICE: &str = "Image Gen Kit";

#[cfg(target_os = "macos")]
pub fn save_api_key(_app_data_dir: &Path, profile_id: &str, api_key: &str) -> Result<(), String> {
    let account = account_name(profile_id);
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            SERVICE,
            "-a",
            &account,
            "-w",
            api_key,
        ])
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(target_os = "macos")]
pub fn get_api_key(_app_data_dir: &Path, profile_id: &str) -> Result<Option<String>, String> {
    let account = account_name(profile_id);
    let output = Command::new("security")
        .args(["find-generic-password", "-s", SERVICE, "-a", &account, "-w"])
        .output()
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((!value.is_empty()).then_some(value))
    } else {
        Ok(None)
    }
}

#[cfg(target_os = "macos")]
pub fn delete_api_key(_app_data_dir: &Path, profile_id: &str) -> Result<(), String> {
    let account = account_name(profile_id);
    let _ = Command::new("security")
        .args(["delete-generic-password", "-s", SERVICE, "-a", &account])
        .output();
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn save_api_key(app_data_dir: &Path, profile_id: &str, api_key: &str) -> Result<(), String> {
    let mut secrets = read_file_secrets(app_data_dir)?;
    secrets.insert(profile_id.to_string(), api_key.to_string());
    write_file_secrets(app_data_dir, &secrets)
}

#[cfg(not(target_os = "macos"))]
pub fn get_api_key(app_data_dir: &Path, profile_id: &str) -> Result<Option<String>, String> {
    Ok(read_file_secrets(app_data_dir)?.remove(profile_id))
}

#[cfg(not(target_os = "macos"))]
pub fn delete_api_key(app_data_dir: &Path, profile_id: &str) -> Result<(), String> {
    let mut secrets = read_file_secrets(app_data_dir)?;
    secrets.remove(profile_id);
    write_file_secrets(app_data_dir, &secrets)
}

fn account_name(profile_id: &str) -> String {
    format!("profile:{profile_id}")
}

#[cfg(not(target_os = "macos"))]
fn secrets_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("secrets.json")
}

#[cfg(not(target_os = "macos"))]
fn read_file_secrets(app_data_dir: &Path) -> Result<HashMap<String, String>, String> {
    let path = secrets_path(app_data_dir);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&content).map_err(|err| err.to_string())
}

#[cfg(not(target_os = "macos"))]
fn write_file_secrets(
    app_data_dir: &Path,
    secrets: &HashMap<String, String>,
) -> Result<(), String> {
    let path = secrets_path(app_data_dir);
    let content = serde_json::to_string_pretty(secrets).map_err(|err| err.to_string())?;
    std::fs::write(path, content).map_err(|err| err.to_string())
}
