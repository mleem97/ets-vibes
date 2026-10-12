use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::core::error::CoreError;

/// Create a timestamped backup of a single file.
///
/// The backup is stored at `<parent>/backup/<timestamp>/<filename>`.
/// The timestamp has microsecond resolution plus a numeric fallback suffix,
/// so two backups created within the same second never overwrite each other.
/// After copying, the file sizes are compared to verify the backup is intact.
pub fn backup_file(source: &Path) -> Result<PathBuf, CoreError> {
    let source_dir = source
        .parent()
        .ok_or_else(|| CoreError::NotFound(format!("Parent of {source:?}")))?;

    let filename = source
        .file_name()
        .ok_or_else(|| CoreError::NotFound(format!("Filename from {source:?}")))?;

    if !source.exists() {
        return Err(CoreError::NotFound(format!("Backup source {source:?}")));
    }

    let backup_dir = unique_timestamp_dir(&source_dir.join("backup"))?;
    fs::create_dir_all(&backup_dir)?;

    let backup_path = backup_dir.join(filename);
    fs::copy(source, &backup_path)?;
    verify_copy(source, &backup_path)?;

    Ok(backup_path)
}

/// Back up several files from one directory into a SINGLE timestamped
/// directory (`<source_dir>/backup/<timestamp>/`). Only files that exist
/// are copied; each copy is size-verified. Returns the backup directory,
/// or an error if none of the files exists.
pub fn backup_files(source_dir: &Path, filenames: &[&str]) -> Result<PathBuf, CoreError> {
    let backup_dir = unique_timestamp_dir(&source_dir.join("backup"))?;
    fs::create_dir_all(&backup_dir)?;

    let mut backed = 0_u32;
    for name in filenames {
        let src = source_dir.join(name);
        if src.is_file() {
            let dst = backup_dir.join(name);
            fs::copy(&src, &dst)?;
            verify_copy(&src, &dst)?;
            backed += 1;
        }
    }

    if backed == 0 {
        let _ = fs::remove_dir(&backup_dir);
        return Err(CoreError::NotFound(format!(
            "Nothing to back up in {source_dir:?}"
        )));
    }

    Ok(backup_dir)
}

/// Back up a whole directory tree (save dir, profile dir) before a
/// destructive operation such as delete.
///
/// The copy is stored OUTSIDE the source tree at
/// `<parent>/backup/<timestamp>/<dirname>/...`, so removing the source
/// afterwards cannot destroy the backup. Every copied file is size-verified.
pub fn backup_dir_tree(src_dir: &Path) -> Result<PathBuf, CoreError> {
    let parent = src_dir
        .parent()
        .ok_or_else(|| CoreError::NotFound("parent directory".into()))?;
    let dir_name = src_dir
        .file_name()
        .ok_or_else(|| CoreError::NotFound(format!("Dir name from {src_dir:?}")))?;

    if !src_dir.exists() {
        return Err(CoreError::NotFound(format!("Backup source {src_dir:?}")));
    }

    let backup_dir = unique_timestamp_dir(&parent.join("backup"))?;
    let backup_target = backup_dir.join(dir_name);
    copy_dir_verified(src_dir, &backup_target)?;

    Ok(backup_target)
}

/// Write a file atomically: data goes to a temp file in the same directory
/// (same filesystem), is fsynced, and then renamed over the target.
/// A crash can therefore never leave a half-written file behind.
pub fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::NotFound(format!("Parent of {path:?}")))?;
    let filename = path
        .file_name()
        .ok_or_else(|| CoreError::NotFound(format!("Filename from {path:?}")))?;

    let mut tmp_name = filename.to_os_string();
    tmp_name.push(format!(".tmp.{}", std::process::id()));
    let tmp_path = parent.join(tmp_name);

    let result = (|| {
        fs::write(&tmp_path, bytes)?;
        let file = fs::File::open(&tmp_path)?;
        file.sync_all()?;
        fs::rename(&tmp_path, path)?;
        Ok::<(), std::io::Error>(())
    })();

    // Never leave the temp file behind.
    let _ = fs::remove_file(&tmp_path);

    result.map_err(CoreError::Io)
}

/// Create `<base>/<timestamp>`, appending `-N` until the path is unused.
fn unique_timestamp_dir(base: &Path) -> Result<PathBuf, CoreError> {
    let stamp = Local::now().format("%d-%m-%Y_%H-%M-%S_%f").to_string();
    let mut candidate = base.join(&stamp);
    let mut counter: u32 = 1;
    while candidate.exists() {
        counter = counter
            .checked_add(1)
            .ok_or_else(|| CoreError::Validation("Too many backups".into()))?;
        candidate = base.join(format!("{stamp}-{counter}"));
    }
    Ok(candidate)
}

/// Verify a copy by comparing file sizes.
fn verify_copy(source: &Path, backup: &Path) -> Result<(), CoreError> {
    let src_len = fs::metadata(source)?.len();
    let dst_len = fs::metadata(backup)?.len();
    if src_len != dst_len {
        return Err(CoreError::Validation(format!(
            "Backup verification failed for {source:?}: expected {src_len} bytes, got {dst_len}"
        )));
    }
    Ok(())
}

fn copy_dir_verified(src: &Path, dst: &Path) -> Result<(), CoreError> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_verified(&entry.path(), &target)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), &target)?;
            verify_copy(&entry.path(), &target)?;
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
    fn test_backup_file_creates_backup() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("game.sii");
        fs::write(&src, "some save data").unwrap();

        let backup_path = backup_file(&src).unwrap();
        assert!(backup_path.exists());

        let content = fs::read_to_string(&backup_path).unwrap();
        assert_eq!(content, "some save data");
    }

    #[test]
    fn test_backup_file_returns_different_path() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("game.sii");
        fs::write(&src, "data").unwrap();

        let backup_path = backup_file(&src).unwrap();
        assert_ne!(backup_path, src);
        assert!(backup_path.to_string_lossy().contains("backup"));
    }

    #[test]
    fn test_backup_file_source_not_exists() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("nonexistent.sii");
        let result = backup_file(&src);
        assert!(result.is_err());
    }

    #[test]
    fn test_backup_file_nested_backup_dir() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("game.sii");
        fs::write(&src, "data").unwrap();

        let backup_path = backup_file(&src).unwrap();
        // Should be at <dir>/backup/<timestamp>/game.sii
        assert_eq!(backup_path.file_name().unwrap(), "game.sii");
        let parent = backup_path.parent().unwrap();
        assert!(parent.to_string_lossy().contains("backup"));
    }

    #[test]
    fn test_two_backups_never_collide() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("game.sii");
        fs::write(&src, "v1").unwrap();
        let first = backup_file(&src).unwrap();
        fs::write(&src, "v2").unwrap();
        let second = backup_file(&src).unwrap();

        assert_ne!(first, second);
        assert_eq!(fs::read_to_string(&first).unwrap(), "v1");
        assert_eq!(fs::read_to_string(&second).unwrap(), "v2");
    }

    #[test]
    fn test_backup_dir_tree_survives_source_delete() {
        let dir = tempfile::tempdir().unwrap();
        let save_dir = dir.path().join("quicksave_1");
        fs::create_dir_all(&save_dir).unwrap();
        fs::write(save_dir.join("game.sii"), "save-bytes").unwrap();
        fs::write(save_dir.join("info.sii"), "info-bytes").unwrap();

        let backup = backup_dir_tree(&save_dir).unwrap();
        // Backup must live OUTSIDE the save dir...
        assert!(!backup.starts_with(&save_dir));
        assert_eq!(
            fs::read_to_string(backup.join("game.sii")).unwrap(),
            "save-bytes"
        );

        // ...so deleting the save keeps the backup intact.
        fs::remove_dir_all(&save_dir).unwrap();
        assert!(!save_dir.exists());
        assert_eq!(
            fs::read_to_string(backup.join("game.sii")).unwrap(),
            "save-bytes"
        );
        assert_eq!(
            fs::read_to_string(backup.join("info.sii")).unwrap(),
            "info-bytes"
        );
    }

    #[test]
    fn test_backup_dir_tree_missing_source_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = backup_dir_tree(&dir.path().join("nope"));
        assert!(result.is_err());
    }

    #[test]
    fn test_backup_files_single_dir_for_all() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.sii"), "game").unwrap();
        fs::write(dir.path().join("info.sii"), "info").unwrap();

        let backup_dir = backup_files(dir.path(), &["game.sii", "info.sii"]).unwrap();
        assert_eq!(
            fs::read_to_string(backup_dir.join("game.sii")).unwrap(),
            "game"
        );
        assert_eq!(
            fs::read_to_string(backup_dir.join("info.sii")).unwrap(),
            "info"
        );
    }

    #[test]
    fn test_backup_files_skips_missing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.sii"), "game").unwrap();

        let backup_dir = backup_files(dir.path(), &["game.sii", "info.sii"]).unwrap();
        assert!(backup_dir.join("game.sii").exists());
        assert!(!backup_dir.join("info.sii").exists());
    }

    #[test]
    fn test_backup_files_none_exist_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = backup_files(dir.path(), &["game.sii"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_file_atomic_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.cfg");
        fs::write(&target, "old").unwrap();

        write_file_atomic(&target, b"new-content").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new-content");

        // No temp leftovers.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn test_write_file_atomic_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("fresh.sii");
        write_file_atomic(&target, b"data").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "data");
    }
}
