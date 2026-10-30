#!/usr/bin/env bash
# Builds the app: the core as WASM, then the page into app/dist/ (served at /app/dist/ by the
# project's serve.sh, and at the same path on GitHub Pages). app/examples.sh refreshes the
# example boards it opens with no server.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
nice -n 15 wasm-pack build core --release --target web --out-dir ../web/src/core --out-name composer_core --no-pack -- --features wasm
cd web
[ -d node_modules ] || npm ci --no-audit --no-fund
nice -n 15 npx vite build
