use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::CoreError;
use crate::save_parser::compression::{detect_format, SaveFormat};

/// 1:1 file taxonomy for ETS2 user folders.
///
/// - **GlobalConfig**: `<game>/config.cfg` — engine/hardware settings
///   (`g_console`, `g_save_format`, graphics…).
/// - **ProfileConfig / ProfileLocalConfig**: `profiles/<id>/config.cfg` and
///   `config_local.cfg` — per-profile gameplay settings.
/// - **Controls**: `controls.sii` — input bindings.
/// - **ProfileData / ProfileBackup**: `profile.sii` (mods, progress meta)
///   and the game's own `profile.bak.sii`.
/// - **SaveData / SaveMeta / SavePreview**: `save/<slot>/game.sii`,
///   `info.sii` (mod/DLC dependencies) and `preview.*` thumbnails.
/// - **MediaConfig**: `live_streams.sii` (internet radio list).
/// - **LogFile**: `game.log.txt`, `game.crash.txt` — never backed up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    GlobalConfig,
    ProfileConfig,
    ProfileLocalConfig,
    Controls,
    ProfileData,
    ProfileBackup,
    SaveData,
    SaveMeta,
    SavePreview,
    MediaConfig,
    LogFile,
    Unknown,
}

impl FileKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::GlobalConfig => "Global config",
            Self::ProfileConfig => "Profile config",
            Self::ProfileLocalConfig => "Profile local config",
            Self::Controls => "Controls",
            Self::ProfileData => "Profile data",
            Self::ProfileBackup => "Profile backup (by game)",
            Self::SaveData => "Save data",
            Self::SaveMeta => "Save metadata",
            Self::SavePreview => "Save preview",
            Self::MediaConfig => "Media config",
            Self::LogFile => "Log file",
            Self::Unknown => "Unknown",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::GlobalConfig => "Engine & hardware settings (config.cfg at game root)",
            Self::ProfileConfig => "Per-profile settings (config.cfg in profile folder)",
            Self::ProfileLocalConfig => "Per-profile local overrides (config_local.cfg)",
            Self::Controls => "Input bindings (controls.sii)",
            Self::ProfileData => "Profile progress & active mods (profile.sii)",
            Self::ProfileBackup => "Automatic backup written by the game",
            Self::SaveData => "Savegame payload (game.sii)",
            Self::SaveMeta => "Save metadata & mod dependencies (info.sii)",
            Self::SavePreview => "Save thumbnail shown in the load screen",
            Self::MediaConfig => "Internet radio stream list (live_streams.sii)",
            Self::LogFile => "Diagnostic log, not part of backups",
            Self::Unknown => "Unrecognized file",
        }
    }

    /// Files that must never end up in a profile backup.
    pub fn excluded_from_backup(&self) -> bool {
        matches!(self, Self::LogFile)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub kind: FileKind,
    pub label: String,
    /// True when the file lives under a Steam `userdata/.../remote` tree.
    pub cloud_copy: bool,
    pub size_bytes: u64,
    /// On-disk container for `.sii` files (ScsC/SiiN/BSII), else None.
    pub container: Option<String>,
}

/// Classify a file by its name and surrounding directory layout.
pub fn classify(path: &Path) -> (FileKind, bool) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let ancestors: Vec<String> = path
        .ancestors()
        .skip(1)
        .filter_map(|a| a.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .collect();

    let cloud_copy = ancestors.iter().any(|a| a == "remote");
    let in_profiles = ancestors
        .iter()
        .any(|a| a == "profiles" || a == "steam_profiles");

    let kind = if name == "config.cfg" && in_profiles {
        FileKind::ProfileConfig
    } else if name == "config.cfg" {
        FileKind::GlobalConfig
    } else if name == "config_local.cfg" {
        FileKind::ProfileLocalConfig
    } else if name == "controls.sii" {
        FileKind::Controls
    } else if name == "profile.sii" {
        FileKind::ProfileData
    } else if name == "profile.bak.sii" {
        FileKind::ProfileBackup
    } else if name == "game.sii" {
        FileKind::SaveData
    } else if name == "info.sii" {
        FileKind::SaveMeta
    } else if name.starts_with("preview") {
        FileKind::SavePreview
    } else if name == "live_streams.sii" {
        FileKind::MediaConfig
    } else if name == "game.log.txt" || name == "game.crash.txt" {
        FileKind::LogFile
    } else {
        FileKind::Unknown
    };

    (kind, cloud_copy)
}

/// Read at most the 4 magic bytes to label the `.sii` container.
/// Returns None for non-`.sii` files or unreadable ones.
fn sniff_container(path: &Path) -> Option<String> {
    if path.extension()?.to_string_lossy().to_lowercase() != "sii" {
        return None;
    }
    let mut buf = [0u8; 4];
    let mut file = std::fs::File::open(path).ok()?;
    {
        use std::io::Read as _;
        file.read_exact(&mut buf).ok()?;
    }
    Some(
        match detect_format(&buf) {
            SaveFormat::EncryptedScsC => "ScsC (encrypted)",
            SaveFormat::PlaintextSiiN => "SiiN (plaintext)",
            SaveFormat::BinaryBsii => "BSII (binary)",
            SaveFormat::Unknown => "unknown",
        }
        .to_string(),
    )
}

/// List every file in a profile tree with its 1:1 classification.
/// Depth-capped (profile/save/slot/files) and sorted by path.
pub fn list_profile_files(profile_path: &Path) -> Result<Vec<FileEntry>, CoreError> {
    if !profile_path.is_dir() {
        return Err(CoreError::NotFound(format!("Profile dir {profile_path:?}")));
    }
    let mut entries = Vec::new();
    walk_profile(profile_path, profile_path, 0, &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn walk_profile(
    root: &Path,
    dir: &Path,
    depth: u8,
    entries: &mut Vec<FileEntry>,
) -> Result<(), CoreError> {
    if depth > 4 {
        return Ok(());
    }
    let _ = root;
    let mut children: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .collect();
    children.sort();
    for child in children {
        if child.is_dir() {
            walk_profile(root, &child, depth + 1, entries)?;
        } else if child.is_file() {
            let (kind, cloud_copy) = classify(&child);
            let size_bytes = child.metadata().map_or(0, |m| m.len());
            entries.push(FileEntry {
                path: child.to_string_lossy().to_string(),
                kind,
                label: kind.label().to_string(),
                cloud_copy,
                size_bytes,
                container: sniff_container(&child),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_labels_cover_all_kinds() {
        for kind in [
            FileKind::GlobalConfig,
            FileKind::ProfileConfig,
            FileKind::ProfileLocalConfig,
            FileKind::Controls,
            FileKind::ProfileData,
            FileKind::ProfileBackup,
            FileKind::SaveData,
            FileKind::SaveMeta,
            FileKind::SavePreview,
            FileKind::MediaConfig,
            FileKind::LogFile,
            FileKind::Unknown,
        ] {
            assert!(!kind.label().is_empty());
            assert!(!kind.description().is_empty());
        }
    }

    #[test]
    fn test_classify_global_vs_profile_config() {
        let (global, _) = classify(Path::new(
            "/home/u/Documents/Euro Truck Simulator 2/config.cfg",
        ));
        assert_eq!(global, FileKind::GlobalConfig);

        let (profile, _) = classify(Path::new(
            "/home/u/Documents/Euro Truck Simulator 2/profiles/4142/config.cfg",
        ));
        assert_eq!(profile, FileKind::ProfileConfig);

        let (local, _) = classify(Path::new(
            "/home/u/Documents/Euro Truck Simulator 2/profiles/4142/config_local.cfg",
        ));
        assert_eq!(local, FileKind::ProfileLocalConfig);
    }

    #[test]
    fn test_classify_save_set() {
        let base = "/g/profiles/4142/save/quicksave";
        assert_eq!(
            classify(Path::new(&format!("{base}/game.sii"))).0,
            FileKind::SaveData
        );
        assert_eq!(
            classify(Path::new(&format!("{base}/info.sii"))).0,
            FileKind::SaveMeta
        );
        assert_eq!(
            classify(Path::new(&format!("{base}/preview.png"))).0,
            FileKind::SavePreview
        );
    }

    #[test]
    fn test_classify_profile_files() {
        let base = "/g/profiles/4142";
        assert_eq!(
            classify(Path::new(&format!("{base}/profile.sii"))).0,
            FileKind::ProfileData
        );
        assert_eq!(
            classify(Path::new(&format!("{base}/profile.bak.sii"))).0,
            FileKind::ProfileBackup
        );
        assert_eq!(
            classify(Path::new(&format!("{base}/controls.sii"))).0,
            FileKind::Controls
        );
        assert_eq!(
            classify(Path::new("/g/live_streams.sii")).0,
            FileKind::MediaConfig
        );
        assert_eq!(classify(Path::new("/g/game.log.txt")).0, FileKind::LogFile);
    }

    #[test]
    fn test_classify_cloud_copy_flag() {
        let (_, cloud) = classify(Path::new(
            "/home/u/.steam/steam/userdata/123/227300/remote/profiles/4142/save/quicksave/game.sii",
        ));
        assert!(cloud);

        let (kind, cloud) = classify(Path::new(
            "/home/u/.steam/steam/userdata/123/227300/remote/config.cfg",
        ));
        assert!(cloud);
        assert_eq!(kind, FileKind::GlobalConfig);
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(
            classify(Path::new("/g/profiles/4142/random.dat")).0,
            FileKind::Unknown
        );
    }

    #[test]
    fn test_list_profile_files_tree() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("profiles/4142");
        let slot = profile.join("save/quicksave");
        std::fs::create_dir_all(&slot).unwrap();
        std::fs::write(profile.join("profile.sii"), "SiiNunit\n").unwrap();
        std::fs::write(profile.join("controls.sii"), "SiiNunit\n").unwrap();
        std::fs::write(profile.join("config.cfg"), "uset g_console \"1\"\n").unwrap();
        std::fs::write(slot.join("game.sii"), "SiiNunit\n").unwrap();
        std::fs::write(slot.join("info.sii"), "SiiNunit\n").unwrap();

        let entries = list_profile_files(&profile).unwrap();
        assert_eq!(entries.len(), 5);
        let kinds: Vec<FileKind> = entries.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&FileKind::ProfileData));
        assert!(kinds.contains(&FileKind::Controls));
        assert!(kinds.contains(&FileKind::ProfileConfig));
        assert!(kinds.contains(&FileKind::SaveData));
        assert!(kinds.contains(&FileKind::SaveMeta));
        // Container sniffing works on tiny plaintext fixtures.
        let game = entries
            .iter()
            .find(|e| e.kind == FileKind::SaveData)
            .unwrap();
        assert_eq!(game.container.as_deref(), Some("SiiN (plaintext)"));
        // Sorted output.
        let mut sorted = entries.clone();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(
            entries.iter().map(|e| &e.path).collect::<Vec<_>>(),
            sorted.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_list_profile_files_missing_dir_errors() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_profile_files(&dir.path().join("nope")).is_err());
    }

    #[test]
    fn test_logs_excluded_from_backup() {
        assert!(FileKind::LogFile.excluded_from_backup());
        assert!(!FileKind::SaveData.excluded_from_backup());
    }
}
