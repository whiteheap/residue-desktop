#!/bin/sh
# Check a built residue.app. CI runs this after every build, and the release
# runs it on the app inside the DMG it publishes.
#
#   sh scripts/check-app.sh PATH/residue.app
#
# - The bundle: identifier com.whiteheap.residue, name and executable
#   residue, and the version in src-tauri/tauri.conf.json.
# - The app's executable and its residue-cli each hold both architectures.
# - residue-cli is the release sidecar.lock pins: `sh scripts/sidecar.sh
#   --check` passes (the release's archives and their attestation were
#   checked, no file in src-tauri/binaries is tracked by git, and each binary
#   there has the sha256 sidecar.lock pins), it prints that version (on Apple
#   silicon, its x86_64 slice too, where Rosetta is installed), and each
#   architecture's UUID is that of the binary placed. Signing rewrites the
#   signature, never the UUID.
# - The ad-hoc signature verifies.
set -eu

[ "$#" -eq 1 ] || {
  echo "usage: sh scripts/check-app.sh PATH/residue.app" >&2
  exit 2
}
root=$(cd "$(dirname "$0")/.." && pwd)
app=$1
plist="$app/Contents/Info.plist"
bin="$root/src-tauri/binaries"

fail() {
  echo "check-app: $*" >&2
  exit 1
}

# LABEL GOT WANT
expect() {
  [ "$2" = "$3" ] || fail "$1 is '$2', expected '$3'"
  echo "check-app: $1: $2"
}

field() {
  plutil -extract "$1" raw -o - "$plist"
}

[ -f "$plist" ] || fail "$app is not an app bundle"
version=$(plutil -extract version raw -o - "$root/src-tauri/tauri.conf.json")
cli_version=$(sed -n 's/^version=//p' "$root/sidecar.lock")

expect CFBundleIdentifier "$(field CFBundleIdentifier)" com.whiteheap.residue
expect CFBundleName "$(field CFBundleName)" residue
expect CFBundleShortVersionString "$(field CFBundleShortVersionString)" "$version"
# The app's executable is residue, the name macOS shows. The CLI inside it
# is residue-cli.
main=$(field CFBundleExecutable)
expect CFBundleExecutable "$main" residue
expect "the app's architectures" "$(lipo -archs "$app/Contents/MacOS/$main")" "x86_64 arm64"
cli="$app/Contents/MacOS/residue-cli"
expect "residue-cli's architectures" "$(lipo -archs "$cli")" "x86_64 arm64"
sh "$root/scripts/sidecar.sh" --check
expect "residue-cli --version" "$("$cli" --version)" "residue $cli_version"
# On Apple silicon, Rosetta runs the x86_64 slice, the one Intel Macs run.
if [ "$(uname -m)" = arm64 ]; then
  if arch -x86_64 /usr/bin/true 2>/dev/null; then
    expect "residue-cli --version, x86_64" "$(arch -x86_64 "$cli" --version)" "residue $cli_version"
  else
    echo "check-app: Rosetta is not installed, so the x86_64 slice did not run"
  fi
fi

uuids() {
  dwarfdump --uuid "$@" | awk '{ print $2, $3 }' | sort
}
placed=$(uuids "$bin/residue-cli-aarch64-apple-darwin" "$bin/residue-cli-x86_64-apple-darwin")
for a in arm64 x86_64; do
  case "$placed" in
    *"($a)"*) ;;
    *) fail "dwarfdump read no $a UUID in src-tauri/binaries" ;;
  esac
done
bundled=$(uuids "$cli")
[ "$bundled" = "$placed" ] || fail "residue-cli's UUIDs are
$bundled
and the placed release's are
$placed"
echo "check-app: residue-cli's UUIDs are the placed release's:"
echo "$bundled"

codesign --verify --deep --strict "$app"
codesign --verify --strict "$cli"
codesign -dv "$app" 2>&1 | grep -qx 'Signature=adhoc' || fail "$app is not signed ad hoc"
echo "check-app: signed ad hoc, and the signature verifies"
