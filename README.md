# Chess Voice Studio

Chess Voice Studio is a Linux-first desktop application for turning typed text into offline
speech. The interface is written in Rust with `egui`/`eframe`; speech is generated locally by
[Piper](https://github.com/OHF-Voice/piper1-gpl), and MP3 export uses FFmpeg.

The featured `en_US-libritts_r-medium` pack exposes **904 selectable offline speakers from one
model**. You can also browse and install the broader Piper catalog. Voice packs are downloaded
once, stored under the standard XDG data directory, and work without a network afterward.

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
cargo build --release
```

Run it with:

```bash
cargo run --release
```

The application expects these runtime tools:

- `python3` for the managed Piper installation
- `curl` for voice downloads and catalog refreshes
- `ffmpeg`/`ffplay` for MP3 export and preferred playback
- `zenity` for Linux save dialogs (`paplay` or `aplay` can also handle playback)

## First run

1. Click **Install Piper engine**. This creates an isolated Python environment under
   `$XDG_DATA_HOME/chess-voice-studio/engine` and installs `piper-tts` there.
2. Select the featured English 904-voice pack and click **Install voice pack**.
3. Move the voice-number control to try different speakers.
4. Enter text, click **Generate & play**, then export WAV or MP3.

After steps 1–2, synthesis and export are offline.

## Storage

```text
$XDG_DATA_HOME/chess-voice-studio/voices/   downloaded models and model cards
$XDG_DATA_HOME/chess-voice-studio/engine/   isolated Piper installation
$XDG_CACHE_HOME/chess-voice-studio/         generated preview cache
```

If the XDG variables are unset, the standard `~/.local/share` and `~/.cache` locations are used.

## Licensing

Chess Voice Studio is GPL-3.0-or-later. Piper is also GPL-3.0-or-later. Piper voice models have
individual licenses; review the `MODEL_CARD` retained beside every installed model before
redistributing generated audio or a model.

