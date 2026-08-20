#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

NIX_FILE="default.nix"
LOG="$(mktemp)"

get_attr() {
  local name="$1"
  sed -n "s/^[[:space:]]*${name}[[:space:]]*=[[:space:]]*\"\([^\"]*\)\"[[:space:]]*;/\1/p" "$NIX_FILE"
}

get_attr_ref() {
  local name="$1"
  sed -n "s/^[[:space:]]*${name}[[:space:]]*=[[:space:]]*\([^;]*\);/\1/p" "$NIX_FILE" | sed 's/[[:space:]]*//g'
}

update_hash_attr() {
  local attr="$1" hash="$2"
  sed -i "s|\(^[[:space:]]*${attr}[[:space:]]*=[[:space:]]*\"\)[^\"]*\([[:space:]]*\"[[:space:]]*;\)|\1${hash}\2|" "$NIX_FILE"
  echo "updated ${attr} = ${hash}"
}

get_rev() {
  git rev-parse HEAD
}

update_src_hash() {
  local pname version owner repo rev
  pname=$(get_attr pname)
  version=$(get_attr version)
  owner=$(get_attr owner)

  repo=$(get_attr_ref repo)
  [ -z "$repo" ] || [ "$repo" = "pname" ] && repo="$pname"

  rev=$(get_attr_ref rev)
  [ -z "$rev" ] || [ "$rev" = "version" ] && rev="$version"

  if [ -z "$owner" ] || [ -z "$repo" ] || [ -z "$rev" ]; then
    echo "error: could not parse owner/repo/rev from $NIX_FILE" >&2
    exit 1
  fi

  local tmp hash
  tmp=$(mktemp -d)
  echo "archiving: ${owner}/${repo} @ ${rev} (local, offline)"
  git archive "$rev" | tar -x -C "$tmp"
  hash=$(nix hash path "$tmp" --type sha256 --sri)
  rm -rf "$tmp"

  if [ -z "$hash" ]; then
    echo "error: failed to compute src hash" >&2
    exit 1
  fi

  update_hash_attr "sha256" "$hash"
}

update_cargo_hash() {
  echo "checking cargoHash..."
  nix build --no-link 2>&1 | tee "$LOG" >/dev/null || true

  local got
  got=$(sed -n 's/.*got:[[:space:]]*\(sha256-[A-Za-z0-9+/=]\+\).*/\1/p' "$LOG" | tail -1)

  if [ -z "$got" ] && grep -q "invalid SRI hash" "$LOG"; then
    echo "placeholder/invalid cargoHash; resetting to fake hash and rebuilding..."
    update_hash_attr "cargoHash" "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    nix build --no-link 2>&1 | tee "$LOG" >/dev/null || true
    got=$(sed -n 's/.*got:[[:space:]]*\(sha256-[A-Za-z0-9+/=]\+\).*/\1/p' "$LOG" | tail -1)
  fi

  if [ -z "$got" ]; then
    if grep -q "error:" "$LOG"; then
      echo "note: build failed for a non-hash reason (cargoHash is fine):"
      grep -m1 -B2 -A5 "error" "$LOG" | tail -10
    else
      echo "cargoHash is already up to date."
    fi
    return 0
  fi

  update_hash_attr "cargoHash" "$got"
  echo "rebuilding with new cargoHash..."
  nix build --no-link 2>&1 | tail -20 || true
}

REV="$(get_rev)"
echo "updating version to ${REV}"
update_hash_attr "version" "$REV"

echo "commenting src = ./.;"
sed -i 's|^[[:space:]]*src[[:space:]]*=[[:space:]]*\./\.;|#   src = ./.;|' "$NIX_FILE"

echo "uncommenting src = fetchFromGitHub { ... }"
sed -i '/^#   src = fetchFromGitHub {/,/^#   };/s/^#   /  /' "$NIX_FILE"

copy_flake_files() {
    # Ensure the target directory exists
    mkdir -p ./flake

    # Copy the specified files if they exist
    cp ./Cargo.lock ./Cargo.toml ./default.nix ./flake.lock ./flake.nix ./flake/ 2>/dev/null
}

update_src_hash
update_cargo_hash
copy_flake_files

echo "done."