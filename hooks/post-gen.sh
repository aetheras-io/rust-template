#!/usr/bin/env bash
set -euo pipefail

# cargo-generate post-hook: format the freshly rendered project
if command -v cargo >/dev/null 2>&1; then
  cargo fmt
fi
