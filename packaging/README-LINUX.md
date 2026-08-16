# Compile and Run Chess Voice Studio on Linux

Compile the source, then move the finished executable into its own writable directory:

```bash
cd /path/to/chess-voice-studio-source
cargo build --release
mkdir -p /path/to/ChessVoiceStudio
cp target/release/chess-voice-studio /path/to/ChessVoiceStudio/
cd /path/to/ChessVoiceStudio
./chess-voice-studio
```

The executable targets 64-bit Linux (`x86_64`) and uses the system's glibc.

For complete functionality, install these runtime tools through your distribution:

- `python3` (one-time Piper engine installation)
- `curl` (voice-pack downloads)
- `ffmpeg` and `ffplay` (MP3 export and playback)
- `zenity` (save dialogs)

On Fedora:

```bash
sudo dnf install python3 curl ffmpeg-free zenity
```

If your Fedora repositories provide the full `ffmpeg` package instead of `ffmpeg-free`, use that.
`paplay` or `aplay` can provide preview playback when `ffplay` is unavailable.

At first launch:

1. Click **Install Piper engine**.
2. Open **Recommended · 904 English voices** and install the collection.
3. After those one-time downloads, speech generation works offline.

The app automatically creates `voices/`, `engine/`, and `cache/` beside itself and stores its
catalog and settings there too. Nothing is installed system-wide. Back up the entire application
directory to preserve all downloaded voice models. A copied Python engine may need to be installed
again on a different computer, but the voice models themselves can be restored directly.
