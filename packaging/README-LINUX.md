# Running Chess Voice Studio on Linux

The included executable targets 64-bit Linux (`x86_64`) and uses the system's glibc. Start it
from a terminal with:

```bash
chmod +x chess-voice-studio
./chess-voice-studio
```

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
2. Select **English Mega Pack · 904 voices** and click **Install voice pack**.
3. After those one-time downloads, speech generation works offline.

The application stores engines and voices under the standard XDG user-data location. Nothing is
installed system-wide, and administrator access is not needed inside the application.

