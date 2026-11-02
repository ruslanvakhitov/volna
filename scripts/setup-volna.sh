#!/bin/bash
set -euo pipefail
ollama_binary=${VOLNA_OLLAMA:-/Applications/Ollama.app/Contents/Resources/ollama}
if [ ! -x "$ollama_binary" ]; then
  echo 'Install Ollama, or set VOLNA_OLLAMA to its executable.' >&2
  exit 1
fi
export OLLAMA_HOST=127.0.0.1:11435
export OLLAMA_MODELS="$HOME/Library/Application Support/volna/context-models"
export OLLAMA_NO_CLOUD=1
mkdir -p "$OLLAMA_MODELS"
volna_server_pid=
if ! curl --silent --fail "$OLLAMA_HOST/api/version" >/dev/null; then
  "$ollama_binary" serve >/dev/null 2>&1 &
  volna_server_pid=$!
  trap 'if [ -n "$volna_server_pid" ]; then kill "$volna_server_pid" 2>/dev/null || true; fi' EXIT
  for attempt in {1..30}; do
    if curl --silent --fail "$OLLAMA_HOST/api/version" >/dev/null; then break; fi
    sleep 0.1
  done
fi
"$ollama_binary" pull qwen3.5:0.8b
echo 'Volna local correction model is ready.'
