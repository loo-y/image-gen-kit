use std::path::{Path, PathBuf};

pub fn app_data_dir() -> Result<PathBuf, String> {
    let dir = platform_data_dir()?.join("Image Gen Kit");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

pub fn database_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()?.join("image-gen-kit.sqlite3"))
}

pub fn images_dir() -> Result<PathBuf, String> {
    let dir = app_data_dir()?.join("images");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

pub fn debug_dir() -> Result<PathBuf, String> {
    let dir = app_data_dir()?.join("debug");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

pub fn generation_debug_dir(generation_id: &str) -> Result<PathBuf, String> {
    let dir = debug_dir()?.join(generation_id);
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

pub fn generation_image_dir(timestamp_ms: i64) -> Result<PathBuf, String> {
    let month = timestamp_ms / 1000 / 60 / 60 / 24 / 31;
    let dir = images_dir()?.join(format!("{month}"));
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

pub fn ensure_path_in_images_dir(path: &Path) -> Result<PathBuf, String> {
    let image_root = images_dir()?
        .canonicalize()
        .map_err(|err| format!("Unable to resolve image directory: {err}"))?;
    let target = path
        .canonicalize()
        .map_err(|err| format!("Unable to resolve image path: {err}"))?;
    if target.starts_with(&image_root) {
        Ok(target)
    } else {
        Err("Refusing to read a file outside the app image directory".to_string())
    }
}

fn platform_data_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        return Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support"));
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
        return Ok(PathBuf::from(appdata));
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(data_home));
        }
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        Ok(PathBuf::from(home).join(".local").join("share"))
    }
}
