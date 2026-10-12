use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    MacOS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameType {
    Ets2,
    Ats,
}

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub game_type: GameType,
    pub name: &'static str,
    pub steam_app_id: &'static str,
}

static SUPPORTED_GAMES: &[GameConfig] = &[
    GameConfig {
        game_type: GameType::Ets2,
        name: "Euro Truck Simulator 2",
        steam_app_id: "227300",
    },
    GameConfig {
        game_type: GameType::Ats,
        name: "American Truck Simulator",
        steam_app_id: "270880",
    },
];

impl GameConfig {
    pub fn get_paths(&self, platform: Platform) -> Vec<PathBuf> {
        match platform {
            Platform::Windows => self.get_windows_paths(),
            Platform::Linux => self.get_linux_paths(),
            Platform::MacOS => self.get_macos_paths(),
        }
    }

    fn folder_name(&self) -> &'static str {
        match self.game_type {
            GameType::Ets2 => "Euro Truck Simulator 2",
            GameType::Ats => "American Truck Simulator",
        }
    }

    fn get_windows_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(docs) = dirs::document_dir() {
            let path = docs.join(self.folder_name());
            if path.exists() {
                paths.push(path);
            }

            let onedrive = docs.join("OneDrive/Documents").join(self.folder_name());
            if onedrive.exists() {
                paths.push(onedrive);
            }
        }

        if let Some(user_profile) = dirs::home_dir() {
            let steam_dirs = [
                PathBuf::from("C:/Program Files (x86)/Steam/userdata"),
                PathBuf::from("C:/Program Files/Steam/userdata"),
                user_profile.join("Steam/userdata"),
            ];

            for steam_dir in &steam_dirs {
                if steam_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(steam_dir) {
                        for entry in entries.flatten() {
                            let game_path = entry.path().join(self.steam_app_id).join("remote");
                            if game_path.exists() {
                                paths.push(game_path);
                            }
                        }
                    }
                }
            }
        }

        paths
    }

    fn get_linux_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(data_dir) = dirs::data_dir() {
            let local = data_dir.join(self.folder_name());
            if local.exists() {
                paths.push(local);
            }
        }

        if let Some(home) = dirs::home_dir() {
            let steam_dirs = [
                home.join(".steam/steam/userdata"),
                home.join(".local/share/Steam/userdata"),
                home.join(".var/app/com.valvesoftware.Steam/.steam/steam/userdata"),
            ];

            for steam_dir in &steam_dirs {
                if steam_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(steam_dir) {
                        for entry in entries.flatten() {
                            let game_path = entry.path().join(self.steam_app_id).join("remote");
                            if game_path.exists() {
                                paths.push(game_path);
                            }
                        }
                    }
                }
            }

            // Proton (Steam Play): the Windows build stores its files inside
            // the Wine prefix, e.g. <library>/steamapps/compatdata/227300/pfx/
            // drive_c/users/steamuser/My Documents/Euro Truck Simulator 2.
            // The native ~/.local/share path above does NOT apply to Proton.
            paths.extend(self.get_proton_paths(&home));
        }

        paths
    }

    /// Find game dirs inside Proton prefixes for all known Steam libraries.
    fn get_proton_paths(&self, home: &std::path::Path) -> Vec<PathBuf> {
        let roots = steam_install_roots(home);
        proton_paths_for_roots(&roots, self.steam_app_id, self.folder_name())
    }

    fn get_macos_paths(&self) -> Vec<PathBuf> {
        let path = dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Application Support")
            .join(self.folder_name());

        if path.exists() {
            vec![path]
        } else {
            vec![]
        }
    }
}

pub fn get_current_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOS
    } else {
        Platform::Linux
    }
}

pub fn detect_installed_games() -> Vec<&'static GameConfig> {
    let platform = get_current_platform();
    SUPPORTED_GAMES
        .iter()
        .filter(|g| !g.get_paths(platform).is_empty())
        .collect()
}

pub fn all_games() -> &'static [GameConfig] {
    SUPPORTED_GAMES
}

pub fn find_config_files(custom_path: Option<&std::path::Path>) -> Vec<PathBuf> {
    let platform = get_current_platform();
    let mut configs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(cp) = custom_path {
        if cp.exists() {
            let candidate1 = cp.join("config.cfg");
            if candidate1.exists() && seen.insert(dedupe_key(&candidate1)) {
                configs.push(candidate1);
            }
            if let Some(parent) = cp.parent() {
                let candidate2 = parent.join("config.cfg");
                if candidate2.exists() && seen.insert(dedupe_key(&candidate2)) {
                    configs.push(candidate2);
                }
            }
        }
    }

    for game in SUPPORTED_GAMES {
        for path in game.get_paths(platform) {
            let config = path.join("config.cfg");
            if config.exists() && seen.insert(dedupe_key(&config)) {
                configs.push(config);
            }
        }
    }

    configs
}

/// Candidate Steam installation roots for a given home directory.
/// Covers native installs, `~/.local/share/Steam`, and the Flatpak layout.
/// Overlapping roots (e.g. `.steam/root` symlinking to `.steam/steam`)
/// are merged via canonical paths.
fn steam_install_roots(home: &std::path::Path) -> Vec<PathBuf> {
    let candidates = [
        home.join(".steam/steam"),
        home.join(".steam/root"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.steam/steam"),
    ];
    let mut roots = Vec::new();
    for candidate in candidates.into_iter().filter(|p| p.exists()) {
        let key = dedupe_key(&candidate);
        if roots.iter().all(|p: &PathBuf| dedupe_key(p) != key) {
            roots.push(candidate);
        }
    }
    roots
}

/// Identity key for dedup: resolves symlinks so the same directory reached
/// via overlapping roots (Steam symlinks, extra libraries, Documents vs
/// My Documents) is only listed once. Falls back to the path itself when
/// it cannot be resolved.
pub fn dedupe_key(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Parse the `path` entries out of a `libraryfolders.vdf`.
/// Kept dependency-free: only lines shaped `"path" "<dir>"` are collected.
fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in vdf.lines() {
        let line = line.trim().trim_matches(',');
        let Some(rest) = line.strip_prefix("\"path\"") else {
            continue;
        };
        let rest = rest.trim();
        if rest.len() >= 2 && rest.starts_with('"') && rest.ends_with('"') {
            let dir = rest[1..rest.len() - 1].trim();
            if !dir.is_empty() {
                paths.push(PathBuf::from(dir));
            }
        }
    }
    paths
}

/// All Steam library dirs: the install root itself plus every extra library
/// from its `libraryfolders.vdf` (games on other drives). The default
/// library is usually listed in the VDF as well — canonical dedup keeps it
/// exactly once.
fn steam_library_dirs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut libs: Vec<PathBuf> = Vec::new();
    let mut push_unique = |dir: PathBuf| {
        let key = dedupe_key(&dir);
        if libs.iter().all(|p: &PathBuf| dedupe_key(p) != key) {
            libs.push(dir);
        }
    };
    for root in roots {
        push_unique(root.clone());
        let vdf_path = root.join("steamapps/libraryfolders.vdf");
        let Ok(content) = std::fs::read_to_string(&vdf_path) else {
            continue;
        };
        for extra in parse_library_paths(&content) {
            if extra.exists() {
                push_unique(extra);
            }
        }
    }
    libs
}

/// Locate `<game>` dirs inside Proton prefixes:
/// `<library>/steamapps/compatdata/<app_id>/pfx/drive_c/users/*/<Documents|My Documents>/<game>`.
/// Both `Documents` and `My Documents` are checked because the Wine shell
/// folder name differs between Proton versions.
fn proton_paths_for_roots(
    roots: &[PathBuf],
    steam_app_id: &str,
    folder_name: &str,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for lib in steam_library_dirs(roots) {
        let users_dir = lib
            .join("steamapps/compatdata")
            .join(steam_app_id)
            .join("pfx/drive_c/users");
        let Ok(users) = std::fs::read_dir(&users_dir) else {
            continue;
        };
        for user in users.flatten() {
            for docs in ["My Documents", "Documents"] {
                let candidate = user.path().join(docs).join(folder_name);
                if candidate.exists() {
                    let key = dedupe_key(&candidate);
                    if paths.iter().all(|p: &PathBuf| dedupe_key(p) != key) {
                        paths.push(candidate);
                    }
                }
            }
        }
    }
    paths
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_library_paths() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"/home/user/.steam/steam"
	}
	"1"
	{
		"path"		"/mnt/games/Steam"
	}
}"#;
        let paths = parse_library_paths(vdf);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/home/user/.steam/steam"),
                PathBuf::from("/mnt/games/Steam"),
            ]
        );
    }

    #[test]
    fn test_parse_library_paths_ignores_other_keys() {
        let vdf = "\"label\"\t\t\"Default\"\n\"path\"\t\t\"\"\n\"contentid\"\t\t\"123\"\n";
        assert!(parse_library_paths(vdf).is_empty());
    }

    fn make_proton_prefix(
        lib: &std::path::Path,
        app_id: &str,
        user: &str,
        docs: &str,
        game: &str,
    ) -> PathBuf {
        let game_dir = lib
            .join("steamapps/compatdata")
            .join(app_id)
            .join("pfx/drive_c/users")
            .join(user)
            .join(docs)
            .join(game);
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::write(game_dir.join("config.cfg"), "uset g_console \"0\"\n").unwrap();
        game_dir
    }

    #[test]
    fn test_proton_paths_my_documents() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".steam/steam");
        std::fs::create_dir_all(&root).unwrap();
        let expected = make_proton_prefix(
            &root,
            "227300",
            "steamuser",
            "My Documents",
            "Euro Truck Simulator 2",
        );

        let found = proton_paths_for_roots(&[root], "227300", "Euro Truck Simulator 2");
        assert_eq!(found, vec![expected]);
    }

    #[test]
    fn test_proton_paths_documents_variant() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".steam/steam");
        std::fs::create_dir_all(&root).unwrap();
        let expected = make_proton_prefix(
            &root,
            "227300",
            "steamuser",
            "Documents",
            "Euro Truck Simulator 2",
        );

        let found = proton_paths_for_roots(&[root], "227300", "Euro Truck Simulator 2");
        assert_eq!(found, vec![expected]);
    }

    #[test]
    fn test_proton_paths_extra_library_from_vdf() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".steam/steam");
        let extra = dir.path().join("games/Steam");
        std::fs::create_dir_all(root.join("steamapps")).unwrap();
        let vdf = format!(
            "\"libraryfolders\"\n{{\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}",
            extra.display()
        );
        std::fs::write(root.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        let expected = make_proton_prefix(
            &extra,
            "270880",
            "steamuser",
            "My Documents",
            "American Truck Simulator",
        );

        let found = proton_paths_for_roots(&[root], "270880", "American Truck Simulator");
        assert_eq!(found, vec![expected]);
    }

    #[test]
    fn test_proton_paths_wrong_app_id_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".steam/steam");
        std::fs::create_dir_all(&root).unwrap();
        make_proton_prefix(
            &root,
            "999999",
            "steamuser",
            "My Documents",
            "Euro Truck Simulator 2",
        );

        let found = proton_paths_for_roots(&[root], "227300", "Euro Truck Simulator 2");
        assert!(found.is_empty());
    }

    #[test]
    fn test_steam_install_roots_skips_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".steam/steam");
        std::fs::create_dir_all(&root).unwrap();
        let roots = steam_install_roots(dir.path());
        assert_eq!(roots, vec![root]);
    }

    #[test]
    fn test_dedupe_key_same_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(dedupe_key(&sub), dedupe_key(&dir.path().join("sub")));
    }

    #[cfg(unix)]
    #[test]
    fn test_proton_paths_symlinked_library_listed_once() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".steam/steam");
        std::fs::create_dir_all(&root).unwrap();
        make_proton_prefix(
            &root,
            "227300",
            "steamuser",
            "My Documents",
            "Euro Truck Simulator 2",
        );
        // Second root is a symlink to the same Steam installation
        // (like ~/.steam/root -> ~/.steam/steam).
        let alias = dir.path().join("steam-alias");
        symlink(&root, &alias).unwrap();

        let found = proton_paths_for_roots(&[root, alias], "227300", "Euro Truck Simulator 2");
        assert_eq!(found.len(), 1);
    }
}
