#!/usr/bin/env bash
set -euo pipefail

export RUSTC_WRAPPER=
export SCCACHE_DISABLE=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="$ROOT/target/demo_base"
EDIT="$ROOT/target/demo_edit"
mkdir -p "$ROOT/target" >/dev/null

echo "Generating baseline -> $BASE"
cargo generate --path "$ROOT" --destination "$ROOT/target" --overwrite --name demo_base \
  --define org=testorg --define docker_repo=gcr.io --silent

echo "Copying editable working copy -> $EDIT"
cp -a "$BASE" "$EDIT"

echo "Done. Edit files in $EDIT. Run ./diff.sh to see changes vs baseline."
