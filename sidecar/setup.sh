#!/bin/bash
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENV="$ROOT/.venv-sidecar"

if command -v uv >/dev/null 2>&1; then
  uv python install 3.11
  uv venv --python 3.11 "$VENV"
  uv pip install --python "$VENV/bin/python" -r "$ROOT/sidecar/requirements.txt"
else
  python3 -m venv "$VENV"
  "$VENV/bin/python" -m pip install -r "$ROOT/sidecar/requirements.txt"
fi

echo "Sidecar setup complete"
