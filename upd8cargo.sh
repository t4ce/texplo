#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

FLAKE_DIR="./flake"
NIX_FILE="default.nix"

get_attr() {
  local name="$1" file="$2"
  sed -n "s/^[[:space:]]*${name}[[:space:]]*=[[:space:]]*\"\([^\"]*\)\"[[:space:]]*;/\1/p" "$file"
}

update_hash_attr() {
  local attr="$1" hash="$2"
  sed -i "s|\(^[[:space:]]*${attr}[[:space:]]*=[[:space:]]*\"\)[^\"]*\([[:space:]]*\"[[:space:]]*;\)|\1${hash}\2|" "$NIX_FILE"
  echo "updated ${attr} = ${hash}"
}

if [ ! -f "$FLAKE_DIR/Cargo.lock" ]; then
  echo "error: $FLAKE_DIR/Cargo.lock not found" >&2
  exit 1
fi

echo "copying $FLAKE_DIR/Cargo.lock over ./Cargo.lock"
cp "$FLAKE_DIR/Cargo.lock" ./Cargo.lock

hash=$(get_attr cargoHash "$FLAKE_DIR/default.nix")
if [ -z "$hash" ]; then
  echo "error: could not read cargoHash from $FLAKE_DIR/default.nix" >&2
  exit 1
fi

update_hash_attr "cargoHash" "$hash"

echo "done."
