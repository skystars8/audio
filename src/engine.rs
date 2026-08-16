use crate::catalog::InstalledVoice;
use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_TEXT_BYTES: usize = 80_000;

#[derive(Clone, Debug)]
pub struct EngineSpec {
    program: PathBuf,
    prefix_args: Vec<String>,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct SynthesisSettings {
    pub speed: f32,
    pub sentence_silence: f32,
    pub volume: f32,
}

impl Default for SynthesisSettings {
    fn default() -> Self {
        Self {
            speed: 1.0,
            sentence_silence: 0.18,
            volume: 1.0,
        }
    }
}

impl EngineSpec {
    pub fn detect(paths: &AppPaths) -> Option<Self> {
        let local_python = paths.engine_dir.join("bin/python");
        if local_python.is_file() && python_has_piper(&local_python) {
            return Some(Self {
                program: local_python,
                prefix_args: vec!["-m".to_owned(), "piper".to_owned()],
                label: "Managed Piper engine".to_owned(),
            });
        }

        if let Some(piper) = find_in_path("piper") {
            return Some(Self {
                program: piper,
                prefix_args: Vec::new(),
                label: "Piper from PATH".to_owned(),
            });
        }

        let python = find_in_path("python3")?;
        if python_has_piper(&python) {
            return Some(Self {
                program: python,
                prefix_args: vec!["-m".to_owned(), "piper".to_owned()],
                label: "Piper Python module".to_owned(),
            });
        }
        None
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.prefix_args);
        command
    }
}

pub fn install_managed_engine(
    paths: &AppPaths,
    mut progress: impl FnMut(String),
) -> Result<(), String> {
    let python = find_in_path("python3")
        .ok_or_else(|| "python3 is required to install the Piper engine".to_owned())?;
    progress("Creating an isolated Piper environment".to_owned());
    let create = Command::new(&python)
        .arg("-m")
        .arg("venv")
        .arg(&paths.engine_dir)
        .output()
        .map_err(|error| format!("Could not create the Piper environment: {error}"))?;
    require_success(create, "Creating the Piper environment")?;

    let managed_python = paths.engine_dir.join("bin/python");
    progress("Downloading and installing Piper (one-time setup)".to_owned());
    let install = Command::new(&managed_python)
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg("--upgrade")
        .arg("piper-tts>=1.6,<2")
        .output()
        .map_err(|error| format!("Could not start pip: {error}"))?;
    require_success(install, "Installing Piper")?;

    if !python_has_piper(&managed_python) {
        return Err("Piper was installed, but its module could not be loaded".to_owned());
    }
    Ok(())
}

pub fn synthesize(
    engine: &EngineSpec,
    voice: &InstalledVoice,
    speaker_id: u32,
    text: &str,
    settings: SynthesisSettings,
    output_wav: &Path,
) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Enter some text before generating audio".to_owned());
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "The text is too long for one render (maximum {MAX_TEXT_BYTES} UTF-8 bytes)"
        ));
    }
    if speaker_id >= voice.num_speakers {
        return Err(format!(
            "Voice {} is outside this model's range of 1–{}",
            speaker_id + 1,
            voice.num_speakers
        ));
    }

    if let Some(parent) = output_wav.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    let partial = output_wav.with_extension("wav.partial");
    let length_scale = 1.0 / settings.speed.clamp(0.5, 2.0);

    let mut command = engine.command();
    command
        .arg("--model")
        .arg(&voice.model_path)
        .arg("--config")
        .arg(&voice.config_path)
        .arg("--output-file")
        .arg(&partial)
        .arg("--speaker")
        .arg(speaker_id.to_string())
        .arg("--length-scale")
        .arg(format!("{length_scale:.4}"))
        .arg("--sentence-silence")
        .arg(format!("{:.3}", settings.sentence_silence.clamp(0.0, 2.0)))
        .arg("--volume")
        .arg(format!("{:.3}", settings.volume.clamp(0.2, 2.0)))
        .arg("--")
        .arg(text)
        .stdin(Stdio::null());

    let output = command
        .output()
        .map_err(|error| format!("Could not start Piper: {error}"))?;
    require_success(output, "Generating speech")?;

    let size = fs::metadata(&partial)
        .map_err(|error| format!("Piper did not create audio: {error}"))?
        .len();
    if size < 44 {
        let _ = fs::remove_file(&partial);
        return Err("Piper produced an empty audio file".to_owned());
    }
    if output_wav.exists() {
        fs::remove_file(output_wav)
            .map_err(|error| format!("Could not replace {}: {error}", output_wav.display()))?;
    }
    fs::rename(&partial, output_wav)
        .map_err(|error| format!("Could not finalize {}: {error}", output_wav.display()))?;
    Ok(())
}

pub fn export_wav(source: &Path, destination: &Path) -> Result<(), String> {
    atomic_copy(source, destination)
}

pub fn export_mp3(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "The MP3 destination has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let partial = parent.join(format!(
        ".{}.{}.partial",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("chess-voice.mp3"),
        unique_stamp()
    ));

    let output = Command::new("ffmpeg")
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(source)
        .arg("-map_metadata")
        .arg("-1")
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("192k")
        .arg("-f")
        .arg("mp3")
        .arg(&partial)
        .output()
        .map_err(|error| format!("Could not start ffmpeg: {error}"))?;
    if let Err(error) = require_success(output, "Encoding MP3") {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    replace_destination(&partial, destination)
}

pub fn play_wav(path: &Path) -> Result<(), String> {
    let (program, arguments): (&str, &[&str]) = if find_in_path("ffplay").is_some() {
        ("ffplay", &["-nodisp", "-autoexit", "-loglevel", "error"])
    } else if find_in_path("paplay").is_some() {
        ("paplay", &[])
    } else if find_in_path("aplay").is_some() {
        ("aplay", &[])
    } else {
        return Err("No audio player was found (install ffplay, paplay, or aplay)".to_owned());
    };

    Command::new(program)
        .args(arguments)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start {program}: {error}"))?;
    Ok(())
}

pub fn cache_key(
    voice_key: &str,
    speaker_id: u32,
    text: &str,
    settings: SynthesisSettings,
) -> String {
    let input = format!(
        "v1\0{voice_key}\0{speaker_id}\0{:.4}\0{:.4}\0{:.4}\0{text}",
        settings.speed, settings.sentence_silence, settings.volume
    );
    format!("{:x}", md5::compute(input.as_bytes()))
}

fn python_has_piper(python: &Path) -> bool {
    Command::new(python)
        .arg("-c")
        .arg("import piper")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

fn require_success(output: std::process::Output, action: &str) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let details = if !stderr.trim().is_empty() {
        stderr.trim()
    } else if !stdout.trim().is_empty() {
        stdout.trim()
    } else {
        "the process exited without an error message"
    };
    Err(format!("{action} failed: {details}"))
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "The destination has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let partial = parent.join(format!(
        ".{}.{}.partial",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("chess-voice.wav"),
        unique_stamp()
    ));
    fs::copy(source, &partial)
        .map_err(|error| format!("Could not copy {}: {error}", source.display()))?;
    replace_destination(&partial, destination)
}

fn replace_destination(partial: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("Could not replace {}: {error}", destination.display()))?;
    }
    fs::rename(partial, destination).map_err(|error| {
        format!(
            "Could not move {} to {}: {error}",
            partial.display(),
            destination.display()
        )
    })
}

fn unique_stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{FEATURED_VOICE, VoiceCatalog, scan_installed};
    use crate::download;

    #[test]
    fn cache_key_changes_with_voice_and_settings() {
        let settings = SynthesisSettings::default();
        let first = cache_key("voice-a", 0, "Knight to f three", settings);
        let second = cache_key("voice-a", 1, "Knight to f three", settings);
        let third = cache_key("voice-b", 0, "Knight to f three", settings);
        assert_ne!(first, second);
        assert_ne!(first, third);
        assert_eq!(
            first,
            cache_key("voice-a", 0, "Knight to f three", settings)
        );
    }

    #[test]
    #[ignore = "downloads a real voice model and requires a Piper Python environment"]
    fn real_piper_download_synthesis_and_mp3_pipeline() {
        let smoke_dir = std::env::var_os("CHESS_VOICE_SMOKE_DIR")
            .map(PathBuf::from)
            .expect("set CHESS_VOICE_SMOKE_DIR to a writable test directory");
        let python = std::env::var_os("CHESS_VOICE_TEST_PYTHON")
            .map(PathBuf::from)
            .expect("set CHESS_VOICE_TEST_PYTHON to the Piper environment's Python executable");
        let voices_dir = smoke_dir.join("voices");
        fs::create_dir_all(&voices_dir).expect("create smoke-test voice directory");

        let catalog = VoiceCatalog::parse(include_str!("../assets/voices.json"))
            .expect("parse bundled catalog");
        let featured = catalog
            .get(FEATURED_VOICE)
            .expect("featured voice must be in the catalog");
        download::install_voice(featured, &voices_dir, |_| {})
            .expect("download and validate featured voice");
        let installed = scan_installed(&voices_dir, &catalog).expect("scan installed voice");
        let voice = installed
            .get(FEATURED_VOICE)
            .expect("featured voice should be installed");
        let test_engine = EngineSpec {
            program: python,
            prefix_args: vec!["-m".to_owned(), "piper".to_owned()],
            label: "Smoke-test Piper".to_owned(),
        };
        let wav_path = smoke_dir.join("knight-to-f-three.wav");
        let mp3_path = smoke_dir.join("knight-to-f-three.mp3");

        synthesize(
            &test_engine,
            voice,
            0,
            "Knight to f three. Black replies knight to f six.",
            SynthesisSettings::default(),
            &wav_path,
        )
        .expect("generate WAV with Piper");
        export_mp3(&wav_path, &mp3_path).expect("encode generated speech as MP3");
        assert!(fs::metadata(&wav_path).expect("WAV metadata").len() > 44);
        assert!(fs::metadata(&mp3_path).expect("MP3 metadata").len() > 128);
    }
}
