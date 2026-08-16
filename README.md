# Chess Voice Studio

Chess Voice Studio is a Linux-first desktop application for turning typed text into offline
speech. The interface is written in Rust with `egui`/`eframe`; speech is generated locally by
[Piper](https://github.com/OHF-Voice/piper1-gpl), and MP3 export uses FFmpeg.

The recommended **English Voice Collection** exposes **904 selectable offline voices from one
79 MB download**. It is pinned at the top of the app, so no technical model name or search term is
needed. You can also browse and install the broader Piper catalog. Voice packs are downloaded once
and work without a network afterward.

## Features

- Large searchable Piper voice catalog, bundled for offline browsing
- Multi-speaker models displayed as individual selectable voice numbers
- Large text editor suitable for chess narration and move descriptions
- Speed, sentence-pause, and volume controls
- Generate, preview, replay, and export WAV or 192 kbps MP3
- One-click isolated Piper engine installation
- Voice downloads verified against the official catalog size and MD5 metadata
- Each downloaded `MODEL_CARD` is retained so its license and attribution stay with the model

## Build on Linux

Install a Rust toolchain and the desktop development libraries required by eframe. On Fedora:

```bash
sudo dnf install gcc-c++ libxcb-devel libxkbcommon-devel openssl-devel
cd /path/to/chess-voice-studio-source
cargo build --release
```

After compiling, move the one finished executable into its own writable directory. Run it from
there—not from `target/release`—so its voices, engine, settings, and cache stay together:

```bash
mkdir -p /path/to/ChessVoiceStudio
cp target/release/chess-voice-studio /path/to/ChessVoiceStudio/
cd /path/to/ChessVoiceStudio
./chess-voice-studio
```

The executable automatically creates every directory it needs beside itself. No installer, marker
file, environment variable, or administrator access is required. You can move the complete app by
moving its directory.

The application expects these runtime tools:

- `python3` for the managed Piper installation
- `curl` for voice downloads and catalog refreshes
- `ffmpeg`/`ffplay` for MP3 export and preferred playback
- `zenity` for Linux save dialogs (`paplay` or `aplay` can also handle playback)

## First run

1. Click **Install Piper engine**. This creates an isolated Python environment in the local
   `engine/` directory and installs `piper-tts` there.
2. Open the permanent **Recommended · 904 English voices** card and click its install button.
3. Move the voice-number control to try different speakers.
4. Enter text, click **Generate & play**, then export WAV or MP3.

After steps 1–2, synthesis and export are offline.

## Storage

On its first launch, the app creates and uses this self-contained layout:

```text
ChessVoiceStudio/
├── chess-voice-studio   executable
├── voices/              downloaded models and model cards
├── engine/              isolated Piper installation
├── cache/               generated preview audio
├── voices.json          refreshed catalog, when requested
└── app.ron              settings and window state
```

Back up the entire directory to preserve the app and all downloaded voices. Voice models are
portable. The Python engine may need to be installed again after restoring onto another computer
or a substantially different Linux installation; if so, click **Install Piper engine** again.

`cargo run --release` is useful during development, but it places these runtime directories beside
the development executable under `target/release`. For normal use, compile first and then move the
executable into its own directory as shown above.

## Licensing

Chess Voice Studio is GPL-3.0-or-later. Piper is also GPL-3.0-or-later. Piper voice models have
individual licenses; review the `MODEL_CARD` retained beside every installed model before
redistributing generated audio or a model.
