#!/usr/bin/env bash
set -euo pipefail

app_dir="${1:-.}"
app_dir="$(cd -- "${app_dir}" && pwd)"
engine_dir="${app_dir}/engine"

python3 -m venv "${engine_dir}"
"${engine_dir}/bin/python" -m pip install --upgrade 'piper-tts>=1.6,<2'
"${engine_dir}/bin/python" -c 'import piper; print("Piper engine installed successfully")'
