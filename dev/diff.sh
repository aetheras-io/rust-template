#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="$ROOT/target/demo_base"
EDIT="$ROOT/target/demo_edit"
PATCH_OUT="$ROOT/target/demo.patch"

if [ ! -d "$BASE" ] || [ ! -d "$EDIT" ]; then
  echo "Baseline or edit directory missing. Run ./test.sh first." >&2
  exit 1
fi

diff -ruN \
  --exclude target \
  --exclude .git \
  --exclude .gitignore \
  --exclude Cargo.lock \
  "$BASE" "$EDIT" >"$PATCH_OUT" || true

if [ -s "$PATCH_OUT" ]; then
  cat $PATCH_OUT
  echo "Diff written to $PATCH_OUT"
else
  rm -f "$PATCH_OUT"
  echo "No diff; $PATCH_OUT is empty"
fi
