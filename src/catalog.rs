use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const CATALOG_URL: &str =
    "https://huggingface.co/rhasspy/piper-voices/resolve/main/voices.json?download=true";
pub const FILE_BASE_URL: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main";
pub const FEATURED_VOICE: &str = "en_US-libritts_r-medium";

#[derive(Clone, Debug, Default)]
pub struct VoiceCatalog {
    entries: BTreeMap<String, VoiceEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VoiceEntry {
    pub key: String,
    pub name: String,
    pub language: VoiceLanguage,
    pub quality: String,
    pub num_speakers: u32,
    #[serde(default)]
    pub speaker_id_map: BTreeMap<String, u32>,
    pub files: BTreeMap<String, VoiceFile>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VoiceLanguage {
    pub code: String,
    pub family: String,
    pub region: String,
    pub name_native: String,
    pub name_english: String,
    pub country_english: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VoiceFile {
    pub size_bytes: u64,
    pub md5_digest: String,
}

#[derive(Clone, Debug)]
pub struct InstalledVoice {
    pub key: String,
    pub model_path: PathBuf,
    pub config_path: PathBuf,
    pub model_card_path: Option<PathBuf>,
    pub speaker_id_map: BTreeMap<String, u32>,
    pub num_speakers: u32,
}

#[derive(Deserialize)]
struct LocalVoiceConfig {
    #[serde(default)]
    num_speakers: u32,
    #[serde(default)]
    speaker_id_map: BTreeMap<String, u32>,
}

impl VoiceCatalog {
    pub fn parse(contents: &str) -> Result<Self, String> {
        let entries: BTreeMap<String, VoiceEntry> = serde_json::from_str(contents)
            .map_err(|error| format!("Could not parse the voice catalog: {error}"))?;
        if entries.is_empty() {
            return Err("The voice catalog is empty".to_owned());
        }
        Ok(Self { entries })
    }

    pub fn load(cached_path: &Path, bundled: &str) -> Result<Self, String> {
        if let Ok(contents) = fs::read_to_string(cached_path)
            && let Ok(catalog) = Self::parse(&contents)
        {
            return Ok(catalog);
        }
        Self::parse(bundled)
    }

    pub fn get(&self, key: &str) -> Option<&VoiceEntry> {
        self.entries.get(key)
    }

    pub fn entries(&self) -> impl Iterator<Item = &VoiceEntry> {
        self.entries.values()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn languages(&self) -> Vec<String> {
        self.entries
            .values()
            .map(|entry| entry.language.name_english.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

impl VoiceEntry {
    pub fn display_name(&self) -> String {
        if self.key == FEATURED_VOICE {
            return format!("English Mega Pack · {} voices", self.num_speakers);
        }
        let readable_name = self.name.replace('_', " ");
        if self.num_speakers > 1 {
            format!("{readable_name} · {} voices", self.num_speakers)
        } else {
            readable_name
        }
    }

    pub fn compact_details(&self) -> String {
        format!(
            "{} ({}) · {}",
            self.language.name_english,
            self.language.code.replace('_', "-"),
            self.quality.replace('_', " ")
        )
    }

    pub fn model_size_bytes(&self) -> u64 {
        self.files
            .iter()
            .find(|(path, _)| path.ends_with(".onnx"))
            .map(|(_, metadata)| metadata.size_bytes)
            .unwrap_or_default()
    }

    pub fn model_files(&self) -> Vec<(&str, &VoiceFile)> {
        let mut files: Vec<_> = self
            .files
            .iter()
            .filter(|(path, _)| {
                path.ends_with(".onnx")
                    || path.ends_with(".onnx.json")
                    || path.ends_with("MODEL_CARD")
            })
            .map(|(path, metadata)| (path.as_str(), metadata))
            .collect();
        files.sort_by_key(|(path, _)| {
            if path.ends_with(".onnx") {
                0
            } else if path.ends_with(".onnx.json") {
                1
            } else {
                2
            }
        });
        files
    }

    pub fn speaker_label(&self, speaker_id: u32) -> String {
        self.speaker_id_map
            .iter()
            .find_map(|(label, id)| (*id == speaker_id).then_some(label.clone()))
            .unwrap_or_else(|| format!("Voice {}", speaker_id + 1))
    }
}

pub fn scan_installed(
    voices_dir: &Path,
    catalog: &VoiceCatalog,
) -> Result<BTreeMap<String, InstalledVoice>, String> {
    let mut installed = BTreeMap::new();
    let directories = fs::read_dir(voices_dir)
        .map_err(|error| format!("Could not read {}: {error}", voices_dir.display()))?;

    for directory in directories.flatten() {
        let directory_path = directory.path();
        if !directory_path.is_dir() {
            continue;
        }

        let key = directory.file_name().to_string_lossy().into_owned();
        let mut model_path = None;
        let mut config_path = None;
        let mut model_card_path = None;

        if let Ok(files) = fs::read_dir(&directory_path) {
            for file in files.flatten() {
                let path = file.path();
                let name = file.file_name().to_string_lossy().into_owned();
                if name.ends_with(".onnx") {
                    model_path = Some(path);
                } else if name.ends_with(".onnx.json") {
                    config_path = Some(path);
                } else if name == "MODEL_CARD" {
                    model_card_path = Some(path);
                }
            }
        }

        let (Some(model_path), Some(config_path)) = (model_path, config_path) else {
            continue;
        };

        let local_config = fs::read_to_string(&config_path)
            .ok()
            .and_then(|contents| serde_json::from_str::<LocalVoiceConfig>(&contents).ok());
        let catalog_entry = catalog.get(&key);
        let speaker_id_map = catalog_entry
            .map(|entry| entry.speaker_id_map.clone())
            .or_else(|| {
                local_config
                    .as_ref()
                    .map(|config| config.speaker_id_map.clone())
            })
            .unwrap_or_default();
        let num_speakers = catalog_entry
            .map(|entry| entry.num_speakers)
            .or_else(|| local_config.as_ref().map(|config| config.num_speakers))
            .unwrap_or(1)
            .max(1);

        installed.insert(
            key.clone(),
            InstalledVoice {
                key,
                model_path,
                config_path,
                model_card_path,
                speaker_id_map,
                num_speakers,
            },
        );
    }

    Ok(installed)
}

pub fn format_size(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * MIB;
    if bytes as f64 >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "en_US-demo-medium": {
        "key": "en_US-demo-medium",
        "name": "demo_voice",
        "language": {
          "code": "en_US", "family": "en", "region": "US",
          "name_native": "English", "name_english": "English",
          "country_english": "United States"
        },
        "quality": "medium", "num_speakers": 2,
        "speaker_id_map": {"Alice": 0, "Bob": 1},
        "files": {
          "en/en_US/demo/medium/en_US-demo-medium.onnx": {
            "size_bytes": 100, "md5_digest": "abc"
          },
          "en/en_US/demo/medium/en_US-demo-medium.onnx.json": {
            "size_bytes": 20, "md5_digest": "def"
          },
          "en/en_US/demo/medium/MODEL_CARD": {
            "size_bytes": 10, "md5_digest": "ghi"
          }
        },
        "aliases": []
      }
    }"#;

    #[test]
    fn parses_catalog_and_speakers() {
        let catalog = VoiceCatalog::parse(SAMPLE).expect("catalog should parse");
        let voice = catalog
            .get("en_US-demo-medium")
            .expect("voice should exist");
        assert_eq!(voice.num_speakers, 2);
        assert_eq!(voice.speaker_label(1), "Bob");
        assert_eq!(voice.model_files().len(), 3);
    }

    #[test]
    fn formats_voice_for_the_picker() {
        let catalog = VoiceCatalog::parse(SAMPLE).expect("catalog should parse");
        let voice = catalog
            .get("en_US-demo-medium")
            .expect("voice should exist");
        assert_eq!(voice.display_name(), "demo voice · 2 voices");
        assert!(voice.compact_details().contains("English"));
    }
}
