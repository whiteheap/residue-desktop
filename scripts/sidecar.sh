#!/bin/sh
# Place the residue CLI where Tauri's externalBin expects it:
# src-tauri/binaries/residue-cli-<target>, for aarch64-apple-darwin,
# x86_64-apple-darwin and universal-apple-darwin (the two joined with lipo).
# Only this script fills src-tauri/binaries/: it is ignored, and every mode
# refuses a file there that git tracks, since `git add -f` can commit one.
#
#   sh scripts/sidecar.sh
#       Place the residue-cli release that sidecar.lock pins: download its
#       two macOS archives and check each one's sha256 against the lock and,
#       when gh is logged in, its build-provenance attestation, then each
#       binary inside against the sha256 the lock pins for it. A release
#       already placed is kept while every binary is still the one the lock
#       pins, so a second run downloads nothing. `tauri dev` and `tauri
#       build` run this first.
#   sh scripts/sidecar.sh --require-attestation
#       Download and check again, whatever is placed, and fail when the
#       attestation cannot be checked. CI and the release run this.
#   sh scripts/sidecar.sh --check
#       Change nothing; fail unless the pinned release is placed with its
#       attestation checked and every binary is the one the lock pins.
#       scripts/check-app.sh runs this.
#   RESIDUE_CLI_SOURCE=DIR sh scripts/sidecar.sh
#       Build residue from the residue-cli checkout in DIR instead, for both
#       architectures, to try the app with a change to the CLI. `pnpm tauri
#       dev` and `pnpm tauri build` pass the variable on. --check, and so CI
#       and the release, accept only the pinned release.
#
# Each way checks that each binary holds its one architecture, the universal
# one both, and that it prints the version it should. src-tauri/binaries/.placed
# then records what is there: the release, and whether its attestation was
# checked, or the checkout.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
lock="$root/sidecar.lock"
out="$root/src-tauri/binaries"
placed="$out/.placed"
targets="aarch64-apple-darwin x86_64-apple-darwin"

fail() {
  echo "sidecar: $*" >&2
  exit 1
}

usage() {
  echo "usage: sh scripts/sidecar.sh [--require-attestation | --check]" >&2
  exit 2
}

value() {
  v=$(sed -n "s/^$1=//p" "$lock")
  [ -n "$v" ] || fail "$1 is missing from sidecar.lock"
  case "$v" in
    *"
"*) fail "$1 is set more than once in sidecar.lock" ;;
  esac
  printf '%s' "$v"
}

arch_is() {
  got=$(lipo -archs "$1" 2>/dev/null) || got="(not a Mach-O binary)"
  [ "$got" = "$2" ] || fail "$1 holds $got, expected $2"
}

sha() {
  shasum -a 256 "$1" | cut -d' ' -f1
}

# Succeeds, and prints nothing, when each binary in src-tauri/binaries is the
# one sidecar.lock pins: each architecture's by its sha256, and the universal
# one holding exactly those two.
pinned() {
  for t in $targets; do
    [ -x "$out/residue-cli-$t" ] || return 1
    [ "$(sha "$out/residue-cli-$t")" = "$(value "binary_sha256_$t")" ] || return 1
  done
  u="$out/residue-cli-universal-apple-darwin"
  [ -x "$u" ] && [ "$(lipo -archs "$u" 2>/dev/null)" = "x86_64 arm64" ] || return 1
  slices=$(mktemp -d)
  ok=1
  for pair in arm64:aarch64-apple-darwin x86_64:x86_64-apple-darwin; do
    if ! lipo -thin "${pair%%:*}" "$u" -output "$slices/${pair%%:*}" 2>/dev/null ||
      [ "$(sha "$slices/${pair%%:*}")" != "$(value "binary_sha256_${pair#*:}")" ]; then
      ok=0
    fi
  done
  rm -rf "$slices"
  [ "$ok" = 1 ]
}

# Succeeds, and prints nothing, when .placed's first line is $1 and every
# binary is the one sidecar.lock pins.
kept() {
  [ -f "$placed" ] && [ "$(sed -n 1p "$placed")" = "$1" ] && pinned
}

# Install the arm64 binary $1 and the x86_64 binary $2, and join them.
place() {
  arch_is "$1" arm64
  arch_is "$2" x86_64
  mkdir -p "$out"
  rm -f "$placed"
  install -m 0755 "$1" "$out/residue-cli-aarch64-apple-darwin"
  install -m 0755 "$2" "$out/residue-cli-x86_64-apple-darwin"
  rm -f "$out/residue-cli-universal-apple-darwin"
  lipo -create -output "$out/residue-cli-universal-apple-darwin" "$1" "$2"
  chmod 0755 "$out/residue-cli-universal-apple-darwin"
  arch_is "$out/residue-cli-universal-apple-darwin" "x86_64 arm64"
}

case "$#:${1:-}" in
  0:) mode=place ;;
  1:--require-attestation) mode=require ;;
  1:--check) mode=check ;;
  *) usage ;;
esac

[ "$(uname -s)" = Darwin ] || fail "the app builds on macOS only"
# A commit can hold files here only through `git add -f`. A checkout would
# then put them in place, so refuse them in every mode. Where git cannot list
# the tree, only a local placement goes on, since it downloads and checks.
if tracked=$(git -C "$root" ls-files -- src-tauri/binaries 2>/dev/null); then
  [ -z "$tracked" ] ||
    fail "git tracks files in src-tauri/binaries, which only this script fills; remove them with git rm -r --cached src-tauri/binaries"
elif [ "$mode" != place ]; then
  fail "git cannot list the files in $root, so this cannot check that no commit put any in src-tauri/binaries"
fi
# One value per assignment, so set -e stops at the first one missing.
version=$(value version)
sha_arm64=$(value sha256_aarch64-apple-darwin)
sha_x86_64=$(value sha256_x86_64-apple-darwin)
tag="v$version"
release="release $version $sha_arm64 $sha_x86_64"

if [ "$mode" = check ]; then
  kept "$release attested" ||
    fail "src-tauri/binaries does not hold residue-cli $version as sidecar.lock pins it, with its attestation checked; run sh scripts/sidecar.sh --require-attestation"
  echo "sidecar: residue-cli $version is in src-tauri/binaries (sha256 and attestation checked)"
  exit 0
fi

if [ -n "${RESIDUE_CLI_SOURCE:-}" ]; then
  [ "$mode" = place ] || fail "RESIDUE_CLI_SOURCE is set, and a build from a checkout has no attestation"
  src=$(cd "$RESIDUE_CLI_SOURCE" 2>/dev/null && pwd) || fail "RESIDUE_CLI_SOURCE=$RESIDUE_CLI_SOURCE is not a folder"
  [ -f "$src/Cargo.toml" ] || fail "$src has no Cargo.toml; set RESIDUE_CLI_SOURCE to a residue-cli checkout"
  # cargo runs here, so rustup takes this repository's rust-toolchain.toml,
  # which has both macOS targets.
  (cd "$root" && cargo build --locked --release \
    --manifest-path "$src/Cargo.toml" --target-dir "$src/target" \
    --target aarch64-apple-darwin --target x86_64-apple-darwin)
  place "$src/target/aarch64-apple-darwin/release/residue" \
    "$src/target/x86_64-apple-darwin/release/residue"
  reported=$("$out/residue-cli-universal-apple-darwin" --version)
  case "$reported" in
    "residue "*) ;;
    *) fail "the build from $src prints '$reported' for --version" ;;
  esac
  printf 'source %s %s\n' "${reported#residue }" "$src" > "$placed"
  echo "sidecar: placed $reported built from $src, not the pinned residue-cli $version"
  exit 0
fi

# CI and the release check again whatever is placed: only a local placement
# keeps one.
if [ "$mode" = place ] && kept "$release attested"; then
  echo "sidecar: residue-cli $version is in src-tauri/binaries (sha256 and attestation checked)"
  exit 0
fi

gh_ready=0
if command -v gh >/dev/null 2>&1 && gh auth status --hostname github.com >/dev/null 2>&1; then
  gh_ready=1
elif [ "$mode" = require ]; then
  fail "gh is not installed or not logged in, so the attestation cannot be checked; run gh auth login, or set GH_TOKEN"
elif kept "$release unattested"; then
  echo "sidecar: residue-cli $version is in src-tauri/binaries (sha256 checked; attestation not checked, gh is not logged in)"
  exit 0
fi

attested_as=$(value attested_as)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
trap 'exit 1' INT TERM
base="https://github.com/whiteheap/residue-cli/releases/download/$tag"

for t in $targets; do
  name="residue-$tag-$t.tar.gz"
  want=$(value "sha256_$t")
  mkdir "$work/$t"
  curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "$work/$t/$name" "$base/$name" ||
    fail "could not download $base/$name"
  got=$(shasum -a 256 "$work/$t/$name" | cut -d' ' -f1)
  [ "$got" = "$want" ] || fail "$name has sha256 $got, and sidecar.lock pins $want"
  if [ "$gh_ready" = 1 ]; then
    # Releases up to 0.0.10 were built before the repository was renamed,
    # so their attestations name whiteheap/residue (sidecar.lock's attested_as).
    gh attestation verify "$work/$t/$name" \
      --repo "$attested_as" \
      --signer-workflow "$attested_as/.github/workflows/release.yml" \
      --source-ref "refs/tags/$tag" \
      --deny-self-hosted-runners >/dev/null ||
      fail "$name: no build-provenance attestation from $attested_as's release workflow at $tag verified"
    echo "sidecar: $name: sha256 and attestation checked"
  else
    echo "sidecar: $name: sha256 checked; attestation not checked, gh is not logged in"
  fi
  tar -xzf "$work/$t/$name" -C "$work/$t" "residue-$tag-$t/residue" ||
    fail "$name holds no residue-$tag-$t/residue"
  got=$(sha "$work/$t/residue-$tag-$t/residue")
  want=$(value "binary_sha256_$t")
  [ "$got" = "$want" ] ||
    fail "the residue in $name has sha256 $got, and sidecar.lock pins $want (for a new release, set binary_sha256_$t=$got once the archive's checks above pass)"
done

place "$work/aarch64-apple-darwin/residue-$tag-aarch64-apple-darwin/residue" \
  "$work/x86_64-apple-darwin/residue-$tag-x86_64-apple-darwin/residue"
reported=$("$out/residue-cli-universal-apple-darwin" --version)
[ "$reported" = "residue $version" ] ||
  fail "the release's binary prints '$reported' for --version, and sidecar.lock pins $version"
if [ "$gh_ready" = 1 ]; then state=attested; else state=unattested; fi
pinned || fail "the binaries placed in src-tauri/binaries are not the ones sidecar.lock pins"
printf '%s %s\n' "$release" "$state" > "$placed"
echo "sidecar: placed residue-cli $version in src-tauri/binaries"
