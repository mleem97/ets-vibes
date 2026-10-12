use std::fs;
use std::path::{Path, PathBuf};

use crate::core::backup::{backup_dir_tree, backup_file, backup_files, write_file_atomic};
use crate::core::error::CoreError;
use crate::save_parser::compression::{compress_save, decompress_save, detect_format, SaveFormat};

fn is_valid_save_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn update_info_sii_name(save_path: &Path, new_name: &str) -> Result<(), CoreError> {
    update_info_sii_name_inner(save_path, new_name, true)
}

fn update_info_sii_name_inner(
    save_path: &Path,
    new_name: &str,
    backup: bool,
) -> Result<(), CoreError> {
    let info_path = save_path.join("info.sii");
    if !info_path.exists() {
        return Ok(());
    }
    // info.sii is usually plaintext, but Steam-synced saves may carry it
    // ScsC-encrypted — never assume UTF-8 (that was the "stream did not
    // contain valid UTF-8" clone failure).
    let raw = fs::read(&info_path)?;
    let (content, was_compressed) = match detect_format(&raw) {
        SaveFormat::EncryptedScsC => (
            decompress_save(&raw).map_err(|e| CoreError::Validation(format!("info.sii: {e}")))?,
            true,
        ),
        SaveFormat::BinaryBsii => {
            return Err(CoreError::Validation(
                "info.sii is in binary BSII format and cannot be renamed. \
                 Set g_save_format 2 in config.cfg, load the save in-game and \
                 save again."
                    .into(),
            ));
        }
        SaveFormat::PlaintextSiiN | SaveFormat::Unknown => (
            String::from_utf8(raw).map_err(|_| {
                CoreError::Validation("info.sii is not a readable text file.".into())
            })?,
            false,
        ),
    };
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut changed = false;

    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with("name:") || trimmed.starts_with("name :") {
            let Some(colon_pos) = trimmed.find(':') else {
                continue;
            };
            // Build new line keeping spacing: "  name: " + new_name
            let prefix = &line[..line.find(trimmed).unwrap_or(0) + colon_pos + 1];
            *line = format!("{} \"{}\"", prefix, new_name);
            changed = true;
        }
    }

    if changed {
        // Backup before overwriting, then write atomically,
        // preserving the original format (ScsC stays ScsC).
        if backup {
            backup_file(&info_path)?;
        }
        let new_content = lines.join("\n") + "\n";
        let bytes = if was_compressed {
            compress_save(&new_content)
                .map_err(|e| CoreError::Validation(format!("info.sii: {e}")))?
        } else {
            new_content.into_bytes()
        };
        write_file_atomic(&info_path, &bytes)?;
    }
    Ok(())
}

/// Rename a save. `info.sii`/`game.sii` are backed up BEFORE any write.
/// Returns the new save path plus the backup directory (remapped to the new
/// location, since the backup moves along with the rename).
pub fn rename_save(
    save_path: &Path,
    new_name: &str,
) -> Result<(PathBuf, Option<PathBuf>), CoreError> {
    if !is_valid_save_name(new_name) {
        return Err(CoreError::Validation(
            "Save name must only contain letters, numbers, underscores, and hyphens".into(),
        ));
    }
    let parent = save_path
        .parent()
        .ok_or_else(|| CoreError::NotFound("parent directory".into()))?;
    let new_path = parent.join(new_name);

    if new_path.exists() {
        return Err(CoreError::Validation(format!(
            "Save '{}' already exists",
            new_name
        )));
    }

    // Backup BEFORE touching anything. The backup dir moves along with the
    // rename, so remap the returned path to the new location.
    let backup = if save_path.join("game.sii").exists() || save_path.join("info.sii").exists() {
        let dir = backup_files(save_path, &["game.sii", "info.sii"])?;
        let stamp = dir
            .file_name()
            .ok_or_else(|| CoreError::NotFound("backup timestamp".into()))?
            .to_os_string();
        Some((dir, stamp))
    } else {
        None
    };

    // Update info.sii name field BEFORE renaming (already backed up above).
    update_info_sii_name_inner(save_path, new_name, false)?;

    fs::rename(save_path, &new_path)?;

    let backup = backup.map(|(_, stamp)| new_path.join("backup").join(stamp));
    Ok((new_path, backup))
}

pub fn clone_save(save_path: &Path, new_name: &str) -> Result<PathBuf, CoreError> {
    if !is_valid_save_name(new_name) {
        return Err(CoreError::Validation(
            "Save name must only contain letters, numbers, underscores, and hyphens".into(),
        ));
    }
    let parent = save_path
        .parent()
        .ok_or_else(|| CoreError::NotFound("parent directory".into()))?;
    let new_path = parent.join(new_name);

    if new_path.exists() {
        return Err(CoreError::Validation(format!(
            "Save '{}' already exists",
            new_name
        )));
    }

    copy_dir_all(save_path, &new_path)?;

    // Update info.sii name in the clone. The source is untouched and the
    // clone is reproducible from it, so no backup is needed — but the
    // write itself is still atomic.
    update_info_sii_name_inner(&new_path, new_name, false)?;

    Ok(new_path)
}

/// Delete a save. The whole save directory is backed up OUTSIDE of itself
/// first, so the backup survives the delete. Returns the backup location.
pub fn delete_save(save_path: &Path) -> Result<PathBuf, CoreError> {
    let backup = backup_dir_tree(save_path)?;
    fs::remove_dir_all(save_path)?;
    Ok(backup)
}

/// Delete a profile. The whole profile tree is backed up OUTSIDE of itself
/// first (all saves + profile.sii), so the backup survives the delete.
/// Returns the backup location.
pub fn delete_profile(profile_path: &Path) -> Result<PathBuf, CoreError> {
    let backup = backup_dir_tree(profile_path)?;
    fs::remove_dir_all(profile_path)?;
    Ok(backup)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), CoreError> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_is_valid_save_name_valid() {
        assert!(is_valid_save_name("autosave"));
        assert!(is_valid_save_name("quicksave_1"));
        assert!(is_valid_save_name("my-save-123"));
        assert!(is_valid_save_name("a"));
    }

    #[test]
    fn test_is_valid_save_name_invalid() {
        assert!(!is_valid_save_name(""));
        assert!(!is_valid_save_name("save name"));
        assert!(!is_valid_save_name("save/name"));
        assert!(!is_valid_save_name("save.name"));
        assert!(!is_valid_save_name("save\nname"));
    }

    #[test]
    fn test_is_valid_save_name_unicode() {
        // "salvar" is alphanumeric -> valid
        assert!(is_valid_save_name("salvar"));
    }

    #[test]
    fn test_update_info_sii_name_changes_name() {
        let dir = tempfile::tempdir().unwrap();
        let info_path = dir.path().join("info.sii");
        fs::write(
            &info_path,
            "SiiNunit\n{\n name: \"old_name\"\n version: 10\n}\n",
        )
        .unwrap();
        update_info_sii_name(dir.path(), "new_name").unwrap();
        let content = fs::read_to_string(&info_path).unwrap();
        assert!(content.contains("\"new_name\""));
        assert!(!content.contains("old_name"));
    }

    #[test]
    fn test_update_info_sii_name_no_name_field() {
        let dir = tempfile::tempdir().unwrap();
        let info_path = dir.path().join("info.sii");
        fs::write(&info_path, "SiiNunit\n{\n version: 10\n}\n").unwrap();
        update_info_sii_name(dir.path(), "new_name").unwrap();
        let content = fs::read_to_string(&info_path).unwrap();
        assert_eq!(content, "SiiNunit\n{\n version: 10\n}\n");
    }

    #[test]
    fn test_update_info_sii_name_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        // no info.sii created — should not error
        update_info_sii_name(dir.path(), "new_name").unwrap();
    }

    #[test]
    fn test_rename_save_invalid_name() {
        let dir = tempfile::tempdir().unwrap();
        let save_dir = dir.path().join("saves").join("valid_save");
        fs::create_dir_all(&save_dir).unwrap();
        let result = rename_save(&save_dir, "invalid name");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CoreError::Validation(_)));
    }

    #[test]
    fn test_rename_save_success() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let old_path = saves_dir.join("old_save");
        fs::create_dir_all(&old_path).unwrap();
        let info_path = old_path.join("info.sii");
        fs::write(&info_path, "SiiNunit\n{\n name: \"old_save\"\n}\n").unwrap();

        let (new_path, backup) = rename_save(&old_path, "new_save").unwrap();
        assert_eq!(new_path, saves_dir.join("new_save"));
        assert!(new_path.exists());
        assert!(!old_path.exists());

        let content = fs::read_to_string(new_path.join("info.sii")).unwrap();
        assert!(content.contains("\"new_save\""));

        // Backup traveled with the rename and holds the original name.
        let backup = backup.unwrap();
        assert!(backup.starts_with(&new_path));
        let backed_info = fs::read_to_string(backup.join("info.sii")).unwrap();
        assert!(backed_info.contains("\"old_save\""));
    }

    #[test]
    fn test_rename_save_encrypted_info_sii() {
        // Regression: Steam-synced saves may carry ScsC-encrypted info.sii.
        // Rename must not fail with "stream did not contain valid UTF-8".
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let old_path = saves_dir.join("old_save");
        fs::create_dir_all(&old_path).unwrap();
        let encrypted =
            compress_save("SiiNunit\n{\n name: \"old_save\"\n version: 10\n}\n").unwrap();
        fs::write(old_path.join("info.sii"), &encrypted).unwrap();

        let (new_path, _) = rename_save(&old_path, "new_save").unwrap();
        assert!(new_path.exists());

        // Still encrypted, and decrypts to the new name.
        let raw = fs::read(new_path.join("info.sii")).unwrap();
        assert_eq!(detect_format(&raw), SaveFormat::EncryptedScsC);
        let text = decompress_save(&raw).unwrap();
        assert!(text.contains("\"new_save\""));
        assert!(!text.contains("old_save"));
    }

    #[test]
    fn test_clone_save_encrypted_info_sii() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let src_path = saves_dir.join("original");
        fs::create_dir_all(&src_path).unwrap();
        fs::write(src_path.join("game.sii"), "content").unwrap();
        let encrypted = compress_save("SiiNunit\n{\n name: \"original\"\n}\n").unwrap();
        fs::write(src_path.join("info.sii"), &encrypted).unwrap();

        let cloned_path = clone_save(&src_path, "clone_name").unwrap();
        let raw = fs::read(cloned_path.join("info.sii")).unwrap();
        let text = decompress_save(&raw).unwrap();
        assert!(text.contains("\"clone_name\""));
    }

    #[test]
    fn test_rename_save_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let old_path = saves_dir.join("save_a");
        let existing_path = saves_dir.join("save_b");
        fs::create_dir_all(&old_path).unwrap();
        fs::create_dir_all(&existing_path).unwrap();

        let result = rename_save(&old_path, "save_b");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CoreError::Validation(_)));
    }

    #[test]
    fn test_clone_save_success() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let src_path = saves_dir.join("original");
        fs::create_dir_all(&src_path).unwrap();
        fs::write(src_path.join("game.sii"), "content").unwrap();
        let info_path = src_path.join("info.sii");
        fs::write(&info_path, "SiiNunit\n{\n name: \"original\"\n}\n").unwrap();

        let cloned_path = clone_save(&src_path, "clone_name").unwrap();
        assert_eq!(cloned_path, saves_dir.join("clone_name"));
        assert!(cloned_path.exists());
        assert!(cloned_path.join("game.sii").exists());
        let content = fs::read_to_string(cloned_path.join("info.sii")).unwrap();
        assert!(content.contains("\"clone_name\""));
    }

    #[test]
    fn test_clone_save_invalid_name() {
        let dir = tempfile::tempdir().unwrap();
        let result = clone_save(&dir.path().join("src"), "invalid/name");
        assert!(result.is_err());
    }

    #[test]
    fn test_clone_save_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let src = saves_dir.join("src");
        let dst = saves_dir.join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        let result = clone_save(&src, "dst");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_save_creates_backup_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let saves_dir = dir.path().join("saves");
        let save_dir = saves_dir.join("save");
        fs::create_dir_all(&save_dir).unwrap();
        fs::write(save_dir.join("game.sii"), "data").unwrap();
        fs::write(save_dir.join("info.sii"), "info").unwrap();

        let backup = delete_save(&save_dir).unwrap();
        assert!(!save_dir.exists());

        // Backup lives next to the deleted save and holds its files.
        assert!(backup.starts_with(&saves_dir));
        assert_eq!(fs::read_to_string(backup.join("game.sii")).unwrap(), "data");
        assert_eq!(fs::read_to_string(backup.join("info.sii")).unwrap(), "info");
    }

    #[test]
    fn test_delete_profile_removes_dir() {
        let dir = tempfile::tempdir().unwrap();
        let profile_dir = dir.path().join("profile");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(profile_dir.join("profile.sii"), "data").unwrap();

        let saves_dir = profile_dir.join("save");
        fs::create_dir_all(&saves_dir).unwrap();
        fs::write(saves_dir.join("game.sii"), "data").unwrap();

        let backup = delete_profile(&profile_dir).unwrap();
        assert!(!profile_dir.exists());

        // Whole tree (profile.sii + saves) survived outside the deleted dir.
        assert_eq!(
            fs::read_to_string(backup.join("profile.sii")).unwrap(),
            "data"
        );
        assert_eq!(
            fs::read_to_string(backup.join("save").join("game.sii")).unwrap(),
            "data"
        );
    }
}
