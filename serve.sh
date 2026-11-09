#!/usr/bin/env bash
# serve.sh — serve this project on its registered port.
#
# The number lives in the registry, never in this
# file: $PORT when the hub started us, otherwise `proj port`, which resolves
# this directory to the project registered for it. There is deliberately no
# fallback default — a project that cannot find its port should say so rather
# than quietly bind whatever looked free.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

if [ -z "${PORT:-}" ]; then
  command -v proj >/dev/null || { echo "serve.sh: \$PORT unset and 'proj' not on PATH; cannot resolve port" >&2; exit 1; }
  PORT="$(proj port)"
fi

exec python3 -m http.server "$PORT" --bind 0.0.0.0
