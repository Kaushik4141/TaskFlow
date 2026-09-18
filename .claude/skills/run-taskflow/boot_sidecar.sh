#!/usr/bin/env bash
# Boot the TaskFlow sidecar and verify /health responds.
#
# NOT RUN by default during skill generation — the sidecar's pinned Python deps
# (torch/transformers/sentence-transformers/sumy, and numpy<2) were not installed
# in this environment. Run this AFTER completing the install step in SKILL.md.
#
# Usage (from repo root):
#   bash .claude/skills/run-taskflow/boot_sidecar.sh
# It boots uvicorn in the background, polls /health, prints the result, then
# leaves the server running on http://127.0.0.1:7878 (Ctrl-C or pkill to stop).

set -u
PORT=7878
BASE="http://127.0.0.1:${PORT}"

if ! command -v python >/dev/null 2>&1; then
  echo "python not found on PATH" >&2; exit 3
fi

cd "$(dirname "$0")/../../sidecar" || { echo "sidecar dir not found" >&2; exit 3; }

# Fail fast if the import-time deps are missing (sumy is the usual blocker).
if ! python -c "import sumy, fastapi, uvicorn, httpx, sklearn, sentence_transformers" 2>/dev/null; then
  echo "ERROR: sidecar Python deps missing. Run the pinned-install step from SKILL.md first:" >&2
  echo "  uv pip install -r sidecar/requirements.txt   # in an activated venv" >&2
  exit 4
fi

# Boot in background, log to a temp file so smoke + driver can read stderr on crash.
LOG="$(mktemp -t taskflow-sidecar.XXXX.log)"
echo "booting uvicorn main:app on :${PORT}  (log: ${LOG})"
# Use --app-dir so uvicorn finds main:app while cwd is repo root.
( python -m uvicorn main:app --port "${PORT}" --host 127.0.0.1 >"${LOG}" 2>&1 & echo $! > /tmp/taskflow_sidecar.pid )

# Poll /health for up to ~40s (the embedder lazy-loads on first request, which
# can take a while if sentence-transformers pulls model weights on cold start).
ok=0
for i in $(seq 1 40); do
  if curl -sf "${BASE}/health" >/tmp/taskflow_health.json 2>/dev/null; then
    ok=1; break
  fi
  sleep 1
  # Surface a boot crash early instead of waiting the full 40s.
  if ! kill -0 "$(cat /tmp/taskflow_sidecar.pid 2>/dev/null)" 2>/dev/null; then
    echo "ERROR: uvicorn exited during boot. Last log lines:" >&2; tail -n 20 "${LOG}" >&2; exit 5
  fi
done

if [ "${ok}" = "1" ]; then
  echo "sidecar healthy:"; cat /tmp/taskflow_health.json; echo
  echo "PID=$(cat /tmp/taskflow_sidecar.pid)  (kill it with: kill \$(cat /tmp/taskflow_sidecar.pid))"
  exit 0
fi
echo "ERROR: /health never came up in 40s. Last log lines:" >&2; tail -n 25 "${LOG}" >&2; exit 6
