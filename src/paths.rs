use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub voices_dir: PathBuf,
    pub engine_dir: PathBuf,
    pub catalog_path: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Self {
        let data_base = xdg_path("XDG_DATA_HOME", ".local/share");
        let cache_base = xdg_path("XDG_CACHE_HOME", ".cache");
        let data_dir = data_base.join("chess-voice-studio");
        let cache_dir = cache_base.join("chess-voice-studio");

        Self {
            voices_dir: data_dir.join("voices"),
            engine_dir: data_dir.join("engine"),
            catalog_path: data_dir.join("voices.json"),
            data_dir,
            cache_dir,
        }
    }

    pub fn ensure(&self) -> Result<(), String> {
        for directory in [
            &self.data_dir,
            &self.cache_dir,
            &self.voices_dir,
            &self.engine_dir,
        ] {
            fs::create_dir_all(directory)
                .map_err(|error| format!("Could not create {}: {error}", directory.display()))?;
        }
        Ok(())
    }

    pub fn preview_dir(&self) -> PathBuf {
        self.cache_dir.join("previews")
    }
}

fn xdg_path(variable: &str, home_suffix: &str) -> PathBuf {
    if let Some(value) = nonempty_env(variable) {
        return PathBuf::from(value);
    }

    if let Some(user_home) = nonempty_env("HOME") {
        return Path::new(&user_home).join(home_suffix);
    }

    std::env::temp_dir().join("chess-voice-studio-user")
}

fn nonempty_env(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}
