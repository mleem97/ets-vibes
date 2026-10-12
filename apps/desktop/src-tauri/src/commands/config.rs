use std::path::Path;

use serde::Serialize;

use crate::core::backup::{backup_file, write_file_atomic};
use crate::core::compatibility::APP_VERSION;
use crate::save_parser::config::{validate_entry, ConfigDocument, ConfigEntry, ConfigValueType};

use super::EditResult;

#[derive(Serialize)]
pub struct AppVersionInfo {
    pub app_version: String,
    pub game_version: Option<String>,
    pub tested_game_version: String,
    pub compatibility_warning: Option<String>,
}

#[tauri::command]
pub fn get_app_info(custom_path: Option<String>) -> Result<AppVersionInfo, String> {
    let custom_p = custom_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(Path::new);
    let (game_version, compatibility_warning) = super::helpers::read_game_version(custom_p);
    Ok(AppVersionInfo {
        app_version: APP_VERSION.to_string(),
        game_version,
        tested_game_version: crate::core::compatibility::TESTED_GAME_VERSION.to_display(),
        compatibility_warning,
    })
}

#[tauri::command]
pub fn list_configs(custom_path: Option<String>) -> Result<Vec<String>, String> {
    let custom_p = custom_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(Path::new);
    let configs = crate::core::detection::find_config_files(custom_p);
    Ok(configs
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

#[tauri::command]
pub fn load_config(path: String) -> Result<ConfigDocument, String> {
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let doc = ConfigDocument::parse(&content);
    Ok(doc)
}

#[tauri::command]
pub fn save_config(path: String, entries: Vec<ConfigEntry>) -> Result<EditResult, String> {
    for entry in &entries {
        if let Err(msg) = validate_entry(&entry.key, entry.val_type, &entry.value) {
            return Err(format!("Validation error for {}: {}", entry.key, msg));
        }
    }

    let file_path = Path::new(&path);
    let backup = backup_file(file_path).map_err(|e| e.to_string())?;

    let doc = ConfigDocument { entries };
    let content = doc.to_string();
    write_file_atomic(file_path, content.as_bytes()).map_err(|e| e.to_string())?;

    let message = format!("Config saved (backup: {})", backup.display());
    Ok(EditResult {
        message,
        backup: Some(backup.to_string_lossy().to_string()),
    })
}

/// One-click console activation: set `g_console 1` + `g_developer 1` in
/// EVERY detected config.cfg (global, Proton prefixes, custom path),
/// inserting the keys when missing. Each file is backed up first and
/// written atomically — it does not matter which file the game reads.
/// Only the two touched keys are validated, so unrelated entries can
/// never block the preset.
#[tauri::command]
pub fn enable_console(custom_path: Option<String>) -> Result<EditResult, String> {
    let custom_p = custom_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(Path::new);
    let configs = crate::core::detection::find_config_files(custom_p);
    if configs.is_empty() {
        return Err("No config.cfg found. Set the game directory in Settings first.".into());
    }

    let mut updated = Vec::new();
    let mut backups = Vec::new();
    for path in &configs {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut doc = ConfigDocument::parse(&content);
        if !doc.apply_console_preset() {
            continue;
        }
        for key in ["g_console", "g_developer"] {
            validate_entry(key, ConfigValueType::Int, "1")
                .map_err(|msg| format!("{}: {}", path.display(), msg))?;
        }
        let backup = backup_file(path).map_err(|e| e.to_string())?;
        write_file_atomic(path, doc.to_string().as_bytes()).map_err(|e| e.to_string())?;
        updated.push(path.to_string_lossy().to_string());
        backups.push(backup.to_string_lossy().to_string());
    }

    if updated.is_empty() {
        return Ok(EditResult {
            message: "Console already enabled in all configs.".into(),
            backup: None,
        });
    }
    Ok(EditResult {
        message: format!(
            "Console enabled in {} config(s): {}",
            updated.len(),
            updated.join(", ")
        ),
        backup: Some(backups.join("; ")),
    })
}
