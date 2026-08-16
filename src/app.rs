use crate::catalog::{
    FEATURED_VOICE, InstalledVoice, VoiceCatalog, VoiceEntry, format_size, scan_installed,
};
use crate::download;
use crate::engine::{self, EngineSpec, SynthesisSettings};
use crate::paths::AppPaths;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const APP_STORAGE_KEY: &str = "chess-voice-studio-state-v1";
const BUNDLED_CATALOG: &str = include_str!("../assets/voices.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    text: String,
    search: String,
    language: String,
    installed_only: bool,
    selected_voice: String,
    speaker_ids: BTreeMap<String, u32>,
    synthesis: SynthesisSettings,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            text: "White plays knight to f three. Black replies knight to f six.".to_owned(),
            search: String::new(),
            language: "English".to_owned(),
            installed_only: false,
            selected_voice: FEATURED_VOICE.to_owned(),
            speaker_ids: BTreeMap::new(),
            synthesis: SynthesisSettings::default(),
        }
    }
}

enum JobEvent {
    Status(String),
    CatalogUpdated(VoiceCatalog),
    VoiceInstalled(String),
    EngineInstalled,
    AudioReady { path: PathBuf, autoplay: bool },
    Exported(PathBuf),
    Failed(String),
    Finished,
}

enum RenderTarget {
    Preview,
    Wav(PathBuf),
    Mp3(PathBuf),
}

pub struct ChessVoiceApp {
    paths: AppPaths,
    catalog: VoiceCatalog,
    installed: BTreeMap<String, InstalledVoice>,
    engine: Option<EngineSpec>,
    preferences: Preferences,
    status: String,
    busy: bool,
    job_receiver: Option<Receiver<JobEvent>>,
    last_audio: Option<PathBuf>,
    license_text: String,
    show_license: bool,
    show_setup: bool,
    startup_warning: Option<String>,
}

impl ChessVoiceApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>, paths: AppPaths) -> Self {
        configure_style(&creation_context.egui_ctx);
        let mut warnings = Vec::new();
        if let Err(error) = paths.ensure() {
            warnings.push(error);
        }

        let catalog = match VoiceCatalog::load(&paths.catalog_path, BUNDLED_CATALOG) {
            Ok(catalog) => catalog,
            Err(error) => {
                warnings.push(error);
                VoiceCatalog::default()
            }
        };
        let installed = match scan_installed(&paths.voices_dir, &catalog) {
            Ok(installed) => installed,
            Err(error) => {
                warnings.push(error);
                BTreeMap::new()
            }
        };
        let engine = EngineSpec::detect(&paths);
        let mut preferences: Preferences = creation_context
            .storage
            .and_then(|storage| eframe::get_value(storage, APP_STORAGE_KEY))
            .unwrap_or_default();

        if catalog.get(&preferences.selected_voice).is_none() {
            preferences.selected_voice = installed
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| FEATURED_VOICE.to_owned());
        }

        let status = if engine.is_some() {
            "Ready — choose an installed voice and start writing".to_owned()
        } else {
            "Install the local Piper engine, then install a voice pack".to_owned()
        };

        Self {
            paths,
            catalog,
            installed,
            engine,
            preferences,
            status,
            busy: false,
            job_receiver: None,
            last_audio: None,
            license_text: String::new(),
            show_license: false,
            show_setup: false,
            startup_warning: (!warnings.is_empty()).then(|| warnings.join("\n")),
        }
    }

    fn poll_jobs(&mut self, context: &egui::Context) {
        let mut events = Vec::new();
        if let Some(receiver) = &self.job_receiver {
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
        }

        for event in events {
            match event {
                JobEvent::Status(message) => self.status = message,
                JobEvent::CatalogUpdated(catalog) => {
                    self.catalog = catalog;
                    self.reload_installed();
                    self.status = format!(
                        "Voice catalog updated — {} packs available",
                        self.catalog.len()
                    );
                }
                JobEvent::VoiceInstalled(key) => {
                    self.reload_installed();
                    self.preferences.selected_voice = key.clone();
                    self.status = if key == FEATURED_VOICE {
                        "904 English voices are ready offline".to_owned()
                    } else {
                        "Voice pack installed and ready offline".to_owned()
                    };
                }
                JobEvent::EngineInstalled => {
                    self.engine = EngineSpec::detect(&self.paths);
                    self.status = "Piper engine installed and ready offline".to_owned();
                }
                JobEvent::AudioReady { path, autoplay } => {
                    self.last_audio = Some(path.clone());
                    if autoplay {
                        match engine::play_wav(&path) {
                            Ok(()) => self.status = "Generated and playing preview".to_owned(),
                            Err(error) => self.status = error,
                        }
                    } else {
                        self.status = "Audio generated".to_owned();
                    }
                }
                JobEvent::Exported(path) => {
                    self.status = format!("Saved {}", path.display());
                }
                JobEvent::Failed(error) => self.status = error,
                JobEvent::Finished => {
                    self.busy = false;
                    self.job_receiver = None;
                }
            }
        }

        if self.busy {
            context.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn reload_installed(&mut self) {
        match scan_installed(&self.paths.voices_dir, &self.catalog) {
            Ok(installed) => self.installed = installed,
            Err(error) => self.status = error,
        }
    }

    fn start_job(
        &mut self,
        context: &egui::Context,
        job: impl FnOnce(Sender<JobEvent>) + Send + 'static,
    ) {
        if self.busy {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.job_receiver = Some(receiver);
        self.busy = true;
        thread::spawn(move || job(sender));
        context.request_repaint();
    }

    fn refresh_catalog(&mut self, context: &egui::Context) {
        let destination = self.paths.catalog_path.clone();
        self.start_job(context, move |sender| {
            let _ = sender.send(JobEvent::Status(
                "Refreshing the official voice catalog".to_owned(),
            ));
            match download::refresh_catalog(&destination) {
                Ok(catalog) => {
                    let _ = sender.send(JobEvent::CatalogUpdated(catalog));
                }
                Err(error) => {
                    let _ = sender.send(JobEvent::Failed(error));
                }
            }
            let _ = sender.send(JobEvent::Finished);
        });
    }

    fn install_engine(&mut self, context: &egui::Context) {
        let paths = self.paths.clone();
        self.start_job(context, move |sender| {
            let status_sender = sender.clone();
            match engine::install_managed_engine(&paths, move |message| {
                let _ = status_sender.send(JobEvent::Status(message));
            }) {
                Ok(()) => {
                    let _ = sender.send(JobEvent::EngineInstalled);
                }
                Err(error) => {
                    let _ = sender.send(JobEvent::Failed(error));
                }
            }
            let _ = sender.send(JobEvent::Finished);
        });
    }

    fn install_selected_voice(&mut self, context: &egui::Context) {
        let Some(entry) = self.catalog.get(&self.preferences.selected_voice).cloned() else {
            self.status = "Select a catalog voice first".to_owned();
            return;
        };
        let voices_dir = self.paths.voices_dir.clone();
        self.start_job(context, move |sender| {
            let status_sender = sender.clone();
            match download::install_voice(&entry, &voices_dir, move |message| {
                let _ = status_sender.send(JobEvent::Status(message));
            }) {
                Ok(()) => {
                    let _ = sender.send(JobEvent::VoiceInstalled(entry.key));
                }
                Err(error) => {
                    let _ = sender.send(JobEvent::Failed(error));
                }
            }
            let _ = sender.send(JobEvent::Finished);
        });
    }

    fn render(&mut self, context: &egui::Context, target: RenderTarget) {
        let Some(engine) = self.engine.clone() else {
            self.status = "Install the Piper engine first".to_owned();
            self.show_setup = true;
            return;
        };
        let Some(voice) = self
            .installed
            .get(&self.preferences.selected_voice)
            .cloned()
        else {
            self.status = "Install the selected voice pack first".to_owned();
            return;
        };
        let text = self.preferences.text.trim().to_owned();
        if text.is_empty() {
            self.status = "Enter some text before generating audio".to_owned();
            return;
        }
        let speaker_id = self.selected_speaker_id(&voice.key, voice.num_speakers);
        let settings = self.preferences.synthesis;
        let cache_key = engine::cache_key(&voice.key, speaker_id, &text, settings);
        let preview_dir = self.paths.preview_dir();
        let cached_wav = preview_dir.join(format!("{cache_key}.wav"));

        if cached_wav.is_file() {
            match target {
                RenderTarget::Preview => match engine::play_wav(&cached_wav) {
                    Ok(()) => {
                        self.last_audio = Some(cached_wav);
                        self.status = "Playing cached preview".to_owned();
                    }
                    Err(error) => self.status = error,
                },
                RenderTarget::Wav(destination) => {
                    self.export_existing(context, cached_wav, destination, false);
                }
                RenderTarget::Mp3(destination) => {
                    self.export_existing(context, cached_wav, destination, true);
                }
            }
            return;
        }

        self.start_job(context, move |sender| {
            let _ = sender.send(JobEvent::Status(format!(
                "Generating with voice {}",
                speaker_id + 1
            )));
            let result =
                engine::synthesize(&engine, &voice, speaker_id, &text, settings, &cached_wav)
                    .and_then(|()| match target {
                        RenderTarget::Preview => {
                            let _ = sender.send(JobEvent::AudioReady {
                                path: cached_wav.clone(),
                                autoplay: true,
                            });
                            Ok(())
                        }
                        RenderTarget::Wav(destination) => {
                            engine::export_wav(&cached_wav, &destination)?;
                            let _ = sender.send(JobEvent::AudioReady {
                                path: cached_wav.clone(),
                                autoplay: false,
                            });
                            let _ = sender.send(JobEvent::Exported(destination));
                            Ok(())
                        }
                        RenderTarget::Mp3(destination) => {
                            engine::export_mp3(&cached_wav, &destination)?;
                            let _ = sender.send(JobEvent::AudioReady {
                                path: cached_wav.clone(),
                                autoplay: false,
                            });
                            let _ = sender.send(JobEvent::Exported(destination));
                            Ok(())
                        }
                    });

            if let Err(error) = result {
                let _ = sender.send(JobEvent::Failed(error));
            }
            let _ = sender.send(JobEvent::Finished);
        });
    }

    fn export_existing(
        &mut self,
        context: &egui::Context,
        cached_wav: PathBuf,
        destination: PathBuf,
        mp3: bool,
    ) {
        self.start_job(context, move |sender| {
            let _ = sender.send(JobEvent::Status(if mp3 {
                "Encoding MP3".to_owned()
            } else {
                "Saving WAV".to_owned()
            }));
            let result = if mp3 {
                engine::export_mp3(&cached_wav, &destination)
            } else {
                engine::export_wav(&cached_wav, &destination)
            };
            match result {
                Ok(()) => {
                    let _ = sender.send(JobEvent::Exported(destination));
                }
                Err(error) => {
                    let _ = sender.send(JobEvent::Failed(error));
                }
            }
            let _ = sender.send(JobEvent::Finished);
        });
    }

    fn selected_speaker_id(&self, key: &str, num_speakers: u32) -> u32 {
        self.preferences
            .speaker_ids
            .get(key)
            .copied()
            .unwrap_or_default()
            .min(num_speakers.saturating_sub(1))
    }

    fn show_voice_sidebar(&mut self, root_ui: &mut egui::Ui) {
        egui::Panel::left("voice_library")
            .resizable(true)
            .default_size(330.0)
            .size_range(285.0..=450.0)
            .show(root_ui, |ui| {
                ui.add_space(8.0);
                ui.heading("Voice library");
                let ready_voice_count: u64 = self
                    .installed
                    .values()
                    .map(|voice| u64::from(voice.num_speakers))
                    .sum();
                ui.label(
                    egui::RichText::new(format!(
                        "{} model packs · {} downloaded · {} voices ready",
                        self.catalog.len(),
                        self.installed.len(),
                        ready_voice_count,
                    ))
                    .weak(),
                );
                ui.add_space(8.0);

                if let Some(featured) = self.catalog.get(FEATURED_VOICE).cloned() {
                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_rgb(31, 48, 43))
                        .inner_margin(egui::Margin::same(9))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Recommended · 904 English voices").strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "One {} download · works offline afterward",
                                    format_size(featured.model_size_bytes())
                                ))
                                .weak(),
                            );
                            let installed = self.installed.contains_key(FEATURED_VOICE);
                            let label = if installed {
                                "Open the 904-voice collection"
                            } else {
                                "Show the 904-voice download"
                            };
                            if ui.small_button(label).clicked() {
                                self.preferences.selected_voice = FEATURED_VOICE.to_owned();
                                self.preferences.search.clear();
                                self.preferences.language = "English".to_owned();
                                self.preferences.installed_only = false;
                            }
                        });
                    ui.add_space(8.0);
                }

                ui.add(
                    egui::TextEdit::singleline(&mut self.preferences.search)
                        .hint_text("Search names, languages, or “904”…"),
                );
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("language_filter")
                        .selected_text(if self.preferences.language.is_empty() {
                            "All languages"
                        } else {
                            &self.preferences.language
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.preferences.language,
                                String::new(),
                                "All languages",
                            );
                            for language in self.catalog.languages() {
                                ui.selectable_value(
                                    &mut self.preferences.language,
                                    language.clone(),
                                    language,
                                );
                            }
                        });
                    ui.checkbox(&mut self.preferences.installed_only, "Installed");
                    let filters_active = !self.preferences.search.trim().is_empty()
                        || self.preferences.language != "English"
                        || self.preferences.installed_only;
                    if ui
                        .add_enabled(filters_active, egui::Button::new("Clear filters"))
                        .clicked()
                    {
                        self.preferences.search.clear();
                        self.preferences.language = "English".to_owned();
                        self.preferences.installed_only = false;
                    }
                });

                ui.separator();
                let search = self.preferences.search.trim().to_lowercase();
                let mut visible: Vec<VoiceEntry> = self
                    .catalog
                    .entries()
                    .filter(|entry| {
                        let language_matches = self.preferences.language.is_empty()
                            || entry.language.name_english == self.preferences.language;
                        let installed_matches = !self.preferences.installed_only
                            || self.installed.contains_key(&entry.key);
                        language_matches && installed_matches && entry.matches_search(&search)
                    })
                    .cloned()
                    .collect();
                visible.sort_by(|left, right| {
                    (right.key == FEATURED_VOICE)
                        .cmp(&(left.key == FEATURED_VOICE))
                        .then_with(|| {
                            self.installed
                                .contains_key(&right.key)
                                .cmp(&self.installed.contains_key(&left.key))
                        })
                        .then_with(|| left.language.name_english.cmp(&right.language.name_english))
                        .then_with(|| left.name.cmp(&right.name))
                        .then_with(|| left.quality.cmp(&right.quality))
                });

                egui::ScrollArea::vertical()
                    .id_salt("voice_catalog_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if visible.is_empty() {
                            ui.add_space(20.0);
                            ui.label("No voices match these filters.");
                        }
                        for entry in visible {
                            let selected = self.preferences.selected_voice == entry.key;
                            let installed = self.installed.contains_key(&entry.key);
                            let title = if installed {
                                format!("●  {}", entry.display_name())
                            } else {
                                entry.display_name()
                            };
                            let response =
                                ui.selectable_label(selected, egui::RichText::new(title).strong());
                            if response.clicked() {
                                self.preferences.selected_voice = entry.key.clone();
                            }
                            ui.indent(format!("voice_details_{}", entry.key), |ui| {
                                ui.label(egui::RichText::new(entry.compact_details()).weak());
                            });
                            ui.add_space(4.0);
                        }
                    });
            });
    }

    fn show_editor(&mut self, root_ui: &mut egui::Ui) {
        let context = root_ui.ctx().clone();
        egui::CentralPanel::default().show(root_ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("Chess Voice Studio");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Setup & storage").clicked() {
                        self.show_setup = true;
                    }
                });
            });
            ui.label(
                egui::RichText::new(
                    "Type narration, choose an offline voice, then generate or export.",
                )
                .weak(),
            );
            ui.add_space(8.0);

            if self.engine.is_none() {
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(46, 39, 25))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new("Local voice engine required").strong(),
                                );
                                ui.label(
                                    egui::RichText::new(
                                        "One-time installation; speech remains offline afterward.",
                                    )
                                    .weak(),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            !self.busy,
                                            egui::Button::new("Install Piper engine"),
                                        )
                                        .clicked()
                                    {
                                        self.install_engine(&context);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(8.0);
            }

            self.show_selected_voice_card(ui, &context);
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Script").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!(
                        "{} characters",
                        self.preferences.text.chars().count()
                    ));
                });
            });
            let editor_height = (ui.available_height() - 165.0).max(180.0);
            ui.add_sized(
                [ui.available_width(), editor_height],
                egui::TextEdit::multiline(&mut self.preferences.text)
                    .hint_text("Example: White castles kingside. Black plays queen to e seven.")
                    .desired_width(f32::INFINITY)
                    .lock_focus(true),
            );

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let can_render = !self.busy
                    && self.engine.is_some()
                    && self
                        .installed
                        .contains_key(&self.preferences.selected_voice)
                    && !self.preferences.text.trim().is_empty();
                if ui
                    .add_enabled(can_render, egui::Button::new("▶  Generate & play"))
                    .clicked()
                {
                    self.render(&context, RenderTarget::Preview);
                }
                if ui
                    .add_enabled(can_render, egui::Button::new("Save WAV…"))
                    .clicked()
                    && let Some(path) = choose_save_file("wav")
                {
                    self.render(&context, RenderTarget::Wav(path));
                }
                if ui
                    .add_enabled(can_render, egui::Button::new("Save MP3…"))
                    .clicked()
                    && let Some(path) = choose_save_file("mp3")
                {
                    self.render(&context, RenderTarget::Mp3(path));
                }
                if ui
                    .add_enabled(
                        !self.busy && self.last_audio.as_ref().is_some_and(|path| path.is_file()),
                        egui::Button::new("Replay last"),
                    )
                    .clicked()
                    && let Some(path) = &self.last_audio
                {
                    match engine::play_wav(path) {
                        Ok(()) => self.status = "Replaying the last preview".to_owned(),
                        Err(error) => self.status = error,
                    }
                }
                if self.busy {
                    ui.spinner();
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&self.status).weak());
            });
        });
    }

    fn show_selected_voice_card(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let entry = self.catalog.get(&self.preferences.selected_voice).cloned();
        let installed_voice = self
            .installed
            .get(&self.preferences.selected_voice)
            .cloned();

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                let Some(entry) = entry else {
                    ui.label("No voice selected.");
                    return;
                };

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(entry.display_name()).size(18.0).strong());
                        let featured = if entry.key == FEATURED_VOICE {
                            " · recommended many-voice pack"
                        } else {
                            ""
                        };
                        ui.label(format!(
                            "{} · {}{featured}",
                            entry.compact_details(),
                            format_size(entry.model_size_bytes())
                        ));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if installed_voice.is_some() {
                            ui.label(
                                egui::RichText::new("● Downloaded · Offline")
                                    .color(egui::Color32::from_rgb(74, 180, 120)),
                            );
                        } else if ui
                            .add_enabled(
                                !self.busy,
                                egui::Button::new(if entry.key == FEATURED_VOICE {
                                    format!(
                                        "Install {} English voices · {}",
                                        entry.num_speakers,
                                        format_size(entry.model_size_bytes())
                                    )
                                } else {
                                    "Install voice pack".to_owned()
                                }),
                            )
                            .clicked()
                        {
                            self.install_selected_voice(context);
                        }
                    });
                });

                if let Some(installed_voice) = installed_voice {
                    ui.add_space(8.0);
                    if installed_voice.num_speakers > 1 {
                        let mut speaker_id = self.selected_speaker_id(
                            &installed_voice.key,
                            installed_voice.num_speakers,
                        );
                        let voice_label = |id: u32| {
                            if entry.key == FEATURED_VOICE {
                                format!("Voice {:03}", id + 1)
                            } else {
                                entry.speaker_label(id)
                            }
                        };
                        let can_test_voice = !self.busy
                            && self.engine.is_some()
                            && !self.preferences.text.trim().is_empty();
                        let mut test_voice = false;

                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("Choose a voice").strong());
                            egui::ComboBox::from_id_salt(format!(
                                "speaker_picker_{}",
                                installed_voice.key
                            ))
                            .selected_text(voice_label(speaker_id))
                            .width(145.0)
                            .height(360.0)
                            .show_ui(ui, |ui| {
                                for candidate in 0..installed_voice.num_speakers {
                                    ui.selectable_value(
                                        &mut speaker_id,
                                        candidate,
                                        voice_label(candidate),
                                    );
                                }
                            });
                            if ui.small_button("Previous").clicked() {
                                speaker_id = speaker_id.saturating_sub(1);
                            }
                            if ui.small_button("Next").clicked() {
                                speaker_id = (speaker_id + 1)
                                    .min(installed_voice.num_speakers.saturating_sub(1));
                            }
                            if ui.small_button("Random voice").clicked() {
                                speaker_id = random_speaker(installed_voice.num_speakers);
                            }
                            if ui
                                .add_enabled(
                                    can_test_voice,
                                    egui::Button::new("▶ Test selected voice"),
                                )
                                .clicked()
                            {
                                test_voice = true;
                            }
                        });
                        ui.label(
                            egui::RichText::new(format!(
                                "{} choices are installed. Open the list, select a numbered voice, then test it with your script.",
                                installed_voice.num_speakers
                            ))
                            .weak(),
                        );
                        self.preferences
                            .speaker_ids
                            .insert(installed_voice.key.clone(), speaker_id);
                        if test_voice {
                            self.render(context, RenderTarget::Preview);
                        }
                    }

                    ui.horizontal_wrapped(|ui| {
                        ui.label("Speed");
                        ui.add(
                            egui::Slider::new(&mut self.preferences.synthesis.speed, 0.6..=1.6)
                                .suffix("×"),
                        );
                        ui.label("Sentence pause");
                        ui.add(
                            egui::Slider::new(
                                &mut self.preferences.synthesis.sentence_silence,
                                0.0..=1.0,
                            )
                            .suffix(" s"),
                        );
                        ui.label("Volume");
                        ui.add(
                            egui::Slider::new(&mut self.preferences.synthesis.volume, 0.5..=1.5)
                                .suffix("×"),
                        );
                    });

                    if installed_voice.model_card_path.is_some()
                        && ui.small_button("View this model's license and attribution").clicked()
                    {
                        self.open_model_card(&installed_voice);
                    }
                } else {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "Downloads once; generation is private and offline afterward. Each model has its own license.",
                        )
                        .weak(),
                    );
                }
            });
    }

    fn open_model_card(&mut self, voice: &InstalledVoice) {
        let Some(path) = &voice.model_card_path else {
            return;
        };
        match fs::read_to_string(path) {
            Ok(contents) => {
                self.license_text = contents;
                self.show_license = true;
            }
            Err(error) => {
                self.status = format!("Could not read {}: {error}", path.display());
            }
        }
    }

    fn show_windows(&mut self, context: &egui::Context) {
        if self.show_setup {
            let mut open = self.show_setup;
            egui::Window::new("Setup & storage")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(560.0)
                .show(context, |ui| {
                    ui.heading("Offline engine");
                    if let Some(engine) = &self.engine {
                        ui.label(
                            egui::RichText::new(format!("●  {}", engine.label))
                                .color(egui::Color32::from_rgb(74, 180, 120)),
                        );
                    } else {
                        ui.label("Piper is not installed yet.");
                        if ui
                            .add_enabled(!self.busy, egui::Button::new("Install Piper engine"))
                            .clicked()
                        {
                            self.install_engine(context);
                        }
                        ui.label(
                            egui::RichText::new(
                                "This performs a one-time download into the engine folder beside the app. No administrator access is needed.",
                            )
                            .weak(),
                        );
                    }

                    ui.separator();
                    ui.heading("Voice catalog");
                    ui.label(format!(
                        "{} packs are available in the bundled catalog.",
                        self.catalog.len()
                    ));
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Refresh catalog from Piper"))
                        .clicked()
                    {
                        self.refresh_catalog(context);
                    }

                    ui.separator();
                    ui.heading("Storage locations");
                    ui.label(
                        egui::RichText::new(
                            "Everything is kept beside this executable. Back up the entire directory to keep the app, voices, engine, settings, and generated preview cache together.",
                        )
                        .color(egui::Color32::from_rgb(74, 180, 120)),
                    );
                    location_row(ui, "App directory", &self.paths.data_dir);
                    location_row(ui, "Voices", &self.paths.voices_dir);
                    location_row(ui, "Engine", &self.paths.engine_dir);
                    location_row(ui, "Preview cache", &self.paths.preview_dir());
                    location_row(ui, "Settings", &self.paths.state_path);

                    ui.separator();
                    ui.label(
                        egui::RichText::new(
                            "Piper and this application are GPL-3.0-or-later. Individual voice model licenses vary; review each downloaded MODEL_CARD.",
                        )
                        .weak(),
                    );
                });
            self.show_setup = open;
        }

        if self.show_license {
            let mut open = self.show_license;
            egui::Window::new("Voice model license & attribution")
                .open(&mut open)
                .default_size([620.0, 500.0])
                .show(context, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.license_text)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    });
                });
            self.show_license = open;
        }

        if let Some(warning) = self.startup_warning.clone() {
            egui::Window::new("Startup warning")
                .collapsible(false)
                .resizable(true)
                .show(context, |ui| {
                    ui.label(warning);
                    if ui.button("Dismiss").clicked() {
                        self.startup_warning = None;
                    }
                });
        }
    }
}

impl eframe::App for ChessVoiceApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll_jobs(&context);
        self.show_voice_sidebar(ui);
        self.show_editor(ui);
        self.show_windows(&context);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, APP_STORAGE_KEY, &self.preferences);
    }
}

fn configure_style(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.selection.bg_fill = egui::Color32::from_rgb(66, 105, 92);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(52, 115, 93);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(55, 72, 68);
    context.set_visuals(visuals);

    let mut style = (*context.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    context.set_style_of(egui::Theme::Dark, style);
}

fn choose_save_file(extension: &str) -> Option<PathBuf> {
    let suggested = format!("chess-voice.{extension}");
    let output = Command::new("zenity")
        .arg("--file-selection")
        .arg("--save")
        .arg("--confirm-overwrite")
        .arg(format!("--filename={suggested}"))
        .arg(format!("--file-filter=*.{extension}"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        return None;
    }
    let mut path = PathBuf::from(value);
    if path.extension().and_then(|value| value.to_str()) != Some(extension) {
        path.set_extension(extension);
    }
    Some(path)
}

fn random_speaker(count: u32) -> u32 {
    if count <= 1 {
        return 0;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_nanos() % count as u128) as u32)
        .unwrap_or_default()
}

fn location_row(ui: &mut egui::Ui, label: &str, path: &Path) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).strong());
        ui.monospace(path.display().to_string());
    });
}
