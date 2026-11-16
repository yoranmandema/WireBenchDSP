//! `PEDAL_HOME` / `PEDAL_MODELS` resolution and index file paths.
//!
//! Storage contract (docs/MODEL-BROWSE-AND-CACHE.md §9.1):
//!
//! ```text
//! $PEDAL_HOME/
//! ├── index/models.json      canonical cached index
//! ├── index/models.json.tmp  in-flight write
//! ├── index/models.json.bak  previous good index (one generation of rollback)
//! └── models/<provider>/<tone_id>/<model_id>_<variant>.<ext>
//! ```
//!
//! * `PEDAL_HOME` env var always wins.
//! * Otherwise, if an ancestor of the running executable contains `presets/`,
//!   `models/` and `Cargo.toml`, that directory is used (dev checkout).
//! * Otherwise the platform default:
//!   macOS `~/Library/Application Support/app.pedal`, Linux/Pi
//!   `~/.local/share/app.pedal`, Windows `%APPDATA%\app.pedal`.
//! * `PEDAL_MODELS` overrides the `models/` directory.

use std::path::{Path, PathBuf};

/// Minimum structure that marks a dev checkout root.
pub fn detect_dev_home(executable: Option<&Path>) -> Option<PathBuf> {
    let executable = executable?;
    // The executable itself lives in <root>/target/<profile>/... so walk every
    // ancestor after the binary's own path and look for the repo marker trio.
    for dir in executable.ancestors().skip(1) {
        if dir.join("presets").is_dir()
            && dir.join("models").is_dir()
            && dir.join("Cargo.toml").is_file()
        {
            return Some(dir.to_path_buf());
        }
    }
    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn platform_default_home() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("app.pedal");
        }
        return PathBuf::from(r"C:\Users\Default\AppData\Roaming").join("app.pedal");
    }
    if cfg!(target_os = "macos") {
        if let Some(home) = home_dir() {
            return home
                .join("Library")
                .join("Application Support")
                .join("app.pedal");
        }
    } else if let Some(home) = home_dir() {
        return home.join(".local").join("share").join("app.pedal");
    }
    // Last resort: a directory relative to the executable.
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pedal-home")
}

/// Resolve `PEDAL_HOME`. Environment first, then dev checkout, then platform.
pub fn pedal_home() -> PathBuf {
    if let Some(v) = non_empty_env("PEDAL_HOME") {
        return PathBuf::from(v);
    }
    if let Some(home) = detect_dev_home(std::env::current_exe().ok().as_deref()) {
        return home;
    }
    platform_default_home()
}

/// Resolve the downloaded-models directory: `PEDAL_MODELS` or `$PEDAL_HOME/models`.
pub fn pedal_models() -> PathBuf {
    if let Some(v) = non_empty_env("PEDAL_MODELS") {
        return PathBuf::from(v);
    }
    pedal_home().join("models")
}

/// Fully resolved storage layout for one `PEDAL_HOME`.
#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    pub models: PathBuf,
    pub index_dir: PathBuf,
    pub index_file: PathBuf,
    pub tmp_file: PathBuf,
    pub bak_file: PathBuf,
    pub lock_file: PathBuf,
}

impl Paths {
    /// Resolve from the process environment (`PEDAL_HOME`, `PEDAL_MODELS`).
    pub fn from_env() -> Self {
        Self::from_home(pedal_home())
    }

    /// Resolve with an explicit home (tests, callers that manage their own root).
    /// `PEDAL_MODELS` still wins over `<home>/models`, mirroring the env rules.
    pub fn from_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let models = pedal_models_override(&home);
        let index_dir = home.join("index");
        let index_file = index_dir.join("models.json");
        Self {
            home,
            models,
            tmp_file: index_dir.join("models.json.tmp"),
            bak_file: index_dir.join("models.json.bak"),
            lock_file: index_dir.join("models.lock"),
            index_dir,
            index_file,
        }
    }

    /// Create the index and models directories if missing.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.index_dir)?;
        std::fs::create_dir_all(&self.models)?;
        Ok(())
    }
}

fn pedal_models_override(home: &Path) -> PathBuf {
    if let Some(v) = non_empty_env("PEDAL_MODELS") {
        return PathBuf::from(v);
    }
    home.join("models")
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pedal-paths-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_repo_root_from_executable_ancestor() {
        let root = unique_dir("detect");
        let bin_dir = root.join("target/debug/deps");
        std::fs::create_dir_all(bin_dir.join("nested")).unwrap();
        std::fs::create_dir_all(root.join("presets")).unwrap();
        std::fs::create_dir_all(root.join("models")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();

        assert_eq!(
            detect_dev_home(Some(&bin_dir.join("nested/app.bin"))),
            Some(root.clone())
        );
        assert_eq!(
            detect_dev_home(Some(&bin_dir.join("app.bin"))),
            Some(root.clone())
        );
        assert_eq!(detect_dev_home(None), None);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn does_not_detect_without_marker_trio() {
        let root = unique_dir("nodetect");
        let exe = root.join("bin/app");
        std::fs::create_dir_all(root.join("presets")).unwrap();
        std::fs::create_dir_all(root.join("models")).unwrap();
        // no Cargo.toml
        assert_eq!(detect_dev_home(Some(&exe)), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn paths_layout_matches_spec() {
        let home = unique_dir("layout");
        std::env::remove_var("PEDAL_MODELS");
        let paths = Paths::from_home(&home);
        assert_eq!(paths.home, home);
        assert_eq!(paths.models, home.join("models"));
        assert_eq!(paths.index_file, home.join("index/models.json"));
        assert_eq!(paths.tmp_file, home.join("index/models.json.tmp"));
        assert_eq!(paths.bak_file, home.join("index/models.json.bak"));
        assert_eq!(paths.lock_file, home.join("index/models.lock"));
        assert!(paths.ensure_dirs().is_ok());
        assert!(home.join("index").is_dir());
        assert!(home.join("models").is_dir());
        let _ = std::fs::remove_dir_all(&home);
    }
}
