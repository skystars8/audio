use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    /// The directory containing the standalone executable.
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub voices_dir: PathBuf,
    pub engine_dir: PathBuf,
    pub catalog_path: PathBuf,
    pub state_path: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, String> {
        let data_dir = launcher_directory().ok_or_else(|| {
            "Could not locate the executable. Put the app in its own writable directory and run it again."
                .to_owned()
        })?;
        Ok(Self::from_data_dir(data_dir))
    }

    fn from_data_dir(data_dir: PathBuf) -> Self {
        Self {
            cache_dir: data_dir.join("cache"),
            voices_dir: data_dir.join("voices"),
            engine_dir: data_dir.join("engine"),
            catalog_path: data_dir.join("voices.json"),
            state_path: data_dir.join("app.ron"),
            data_dir,
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

fn launcher_directory() -> Option<PathBuf> {
    if let Some(app_image) = nonempty_env("APPIMAGE") {
        let app_image = PathBuf::from(app_image);
        if app_image.is_absolute() {
            return app_image.parent().map(Path::to_path_buf);
        }
    }

    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
}

fn nonempty_env(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_runtime_files_live_beside_the_executable() {
        let paths = AppPaths::from_data_dir(PathBuf::from("/home/user/audio"));

        assert_eq!(paths.data_dir, Path::new("/home/user/audio"));
        assert_eq!(paths.voices_dir, Path::new("/home/user/audio/voices"));
        assert_eq!(paths.engine_dir, Path::new("/home/user/audio/engine"));
        assert_eq!(paths.cache_dir, Path::new("/home/user/audio/cache"));
        assert_eq!(
            paths.catalog_path,
            Path::new("/home/user/audio/voices.json")
        );
        assert_eq!(paths.state_path, Path::new("/home/user/audio/app.ron"));
    }

    #[test]
    fn paths_do_not_depend_on_the_working_directory() {
        let first = AppPaths::from_data_dir(PathBuf::from("/opt/voice one"));
        let second = AppPaths::from_data_dir(PathBuf::from("/opt/voice one"));

        assert_eq!(first.voices_dir, second.voices_dir);
        assert_eq!(first.engine_dir, second.engine_dir);
        assert_eq!(first.state_path, second.state_path);
    }
}
