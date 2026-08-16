#!/usr/bin/env bash
set -euo pipefail

app_data_base="${XDG_DATA_HOME:-${HOME}/.local/share}"
engine_dir="${app_data_base}/chess-voice-studio/engine"

python3 -m venv "${engine_dir}"
"${engine_dir}/bin/python" -m pip install --upgrade 'piper-tts>=1.6,<2'
"${engine_dir}/bin/python" -c 'import piper; print("Piper engine installed successfully")'

