use crate::catalog::{CATALOG_URL, FILE_BASE_URL, VoiceCatalog, VoiceEntry, VoiceFile};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub fn refresh_catalog(destination: &Path) -> Result<VoiceCatalog, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "The catalog destination has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let partial = destination.with_extension("json.partial");

    download_url(CATALOG_URL, &partial)?;
    let contents = fs::read_to_string(&partial)
        .map_err(|error| format!("Could not read the downloaded catalog: {error}"))?;
    let catalog = VoiceCatalog::parse(&contents)?;
    replace_file(&partial, destination)?;
    Ok(catalog)
}

pub fn install_voice(
    entry: &VoiceEntry,
    voices_dir: &Path,
    mut progress: impl FnMut(String),
) -> Result<(), String> {
    let target_dir = voices_dir.join(&entry.key);
    fs::create_dir_all(&target_dir)
        .map_err(|error| format!("Could not create {}: {error}", target_dir.display()))?;

    let files = entry.model_files();
    if !files.iter().any(|(path, _)| path.ends_with(".onnx"))
        || !files.iter().any(|(path, _)| path.ends_with(".onnx.json"))
    {
        return Err("This catalog entry does not contain a model and configuration".to_owned());
    }

    for (index, (relative_path, metadata)) in files.iter().enumerate() {
        let file_name = safe_file_name(relative_path)?;
        let destination = target_dir.join(file_name);
        progress(format!(
            "Downloading {} ({}/{})",
            file_name.to_string_lossy(),
            index + 1,
            files.len()
        ));

        if destination.is_file() && verify_file(&destination, metadata).is_ok() {
            continue;
        }

        let partial = destination.with_extension(format!(
            "{}.partial",
            destination
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("download")
        ));
        let url = format!("{FILE_BASE_URL}/{relative_path}?download=true");
        if let Err(error) = download_url(&url, &partial)
            .and_then(|_| verify_file(&partial, metadata))
            .and_then(|_| replace_file(&partial, &destination))
        {
            let _ = fs::remove_file(&partial);
            return Err(error);
        }
    }

    Ok(())
}

fn download_url(url: &str, destination: &Path) -> Result<(), String> {
    let output = Command::new("curl")
        .arg("--fail")
        .arg("--location")
        .arg("--silent")
        .arg("--show-error")
        .arg("--output")
        .arg(destination)
        .arg(url)
        .output()
        .map_err(|error| format!("Could not start curl: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "Download failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn safe_file_name(relative_path: &str) -> Result<&std::ffi::OsStr, String> {
    let path = Path::new(relative_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("Unsafe catalog path: {relative_path}"));
    }
    path.file_name()
        .ok_or_else(|| format!("Catalog path has no filename: {relative_path}"))
}

fn verify_file(path: &Path, expected: &VoiceFile) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if metadata.len() != expected.size_bytes {
        return Err(format!(
            "{} has the wrong size (expected {}, received {})",
            path.display(),
            expected.size_bytes,
            metadata.len()
        ));
    }

    let actual_digest = md5_file(path)?;
    if !actual_digest.eq_ignore_ascii_case(&expected.md5_digest) {
        return Err(format!("{} failed its integrity check", path.display()));
    }
    Ok(())
}

fn md5_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let mut context = md5::Context::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        context.consume(&buffer[..count]);
    }
    Ok(format!("{:x}", context.finalize()))
}

fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("Could not replace {}: {error}", destination.display()))?;
    }
    fs::rename(source, destination).map_err(|error| {
        format!(
            "Could not move {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })
}

#[allow(dead_code)]
fn write_small_file(path: &Path, contents: &[u8]) -> Result<PathBuf, String> {
    let mut file = File::create(path)
        .map_err(|error| format!("Could not create {}: {error}", path.display()))?;
    file.write_all(contents)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_catalog_path_traversal() {
        assert!(safe_file_name("../voice.onnx").is_err());
        assert!(safe_file_name("/tmp/voice.onnx").is_err());
        assert!(safe_file_name("en/en_US/voice.onnx").is_ok());
    }
}
