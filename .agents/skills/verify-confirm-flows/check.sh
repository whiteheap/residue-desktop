#!/usr/bin/env bash
# Model-check the app's two confirm flows with TLC: moving worktrees to the
# trash (model/RemoveFlow.tla) and turning on, saving or turning off
# automatic cleanup (model/SaveFlow.tla).
#
#   bash .agents/skills/verify-confirm-flows/check.sh
#
# Checks each model in the setups below, then runs each mutant in
# model/mutants, a model with one guard of the code removed (or each guard
# a line joins with +), which must fail its rule. The result must match
# model/expected: every rule holds except the known breaks listed there,
# each with its shortest path.
#
# Exit status: 0 the result matches model/expected, 1 it does not, 2 the
# tools could not be set up or TLC did not run.
#
# The first run downloads TLC and a Java runtime, both pinned by version
# and SHA-256, into ${RESIDUE_TLA_TOOLS:-~/.cache/residue-tla}, the cache
# residue-cli's model check uses too. Set TLA2TOOLS to a tla2tools.jar and
# JAVA to a java binary (Java 11 or later) to use your own; the run says
# when they differ from the pinned ones.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
model=$here/model
tools=${RESIDUE_TLA_TOOLS:-${XDG_CACHE_HOME:-$HOME/.cache}/residue-tla}

# TLC 2.19 (tla2tools v1.7.4, 2024-08-08).
tlc_url=https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar
tlc_sha=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
# Eclipse Temurin JRE 25.0.5+8.
jre_base=https://github.com/adoptium/temurin25-binaries/releases/download/jdk-25.0.5%2B8
jre_dir=jdk-25.0.5+8-jre

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

fetch() { # URL SHA256 FILE
  local tmp=$3.part
  if [ -f "$3" ] && [ "$(sha256 "$3")" = "$2" ]; then return 0; fi
  echo "downloading $1" >&2
  curl -fsSL --retry 3 -o "$tmp" "$1" || { echo "check.sh: cannot download $1" >&2; exit 2; }
  if [ "$(sha256 "$tmp")" != "$2" ]; then
    echo "check.sh: $1 has SHA-256 $(sha256 "$tmp"), not the pinned $2" >&2
    rm -f "$tmp"; exit 2
  fi
  mv "$tmp" "$3"
}

setup_tools() {
  mkdir -p "$tools"
  if [ -n "${TLA2TOOLS:-}" ]; then
    jar=$TLA2TOOLS
    [ "$(sha256 "$jar")" = "$tlc_sha" ] || echo "note: $jar is not the pinned tla2tools.jar v1.7.4" >&2
  else
    jar=$tools/tla2tools.jar
    fetch "$tlc_url" "$tlc_sha" "$jar"
  fi
  if [ -n "${JAVA:-}" ]; then
    java=$JAVA
    "$java" -version 2>&1 | grep -q 'Temurin-25\.0\.5+8' ||
      echo "note: $java is not the pinned Temurin 25.0.5+8" >&2
    return
  fi
  local os arch sha bin
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) os=mac; arch=aarch64
      sha=72ebd76b502c849e88dea39afae51baf5f82f2b5b1b6389c28ea707b20451bce
      bin=Contents/Home/bin/java ;;
    Darwin-x86_64) os=mac; arch=x64
      sha=806902c796c670969b7b27df73b8f0fe0924a61bd65083aa53064115e49cab2d
      bin=Contents/Home/bin/java ;;
    Linux-x86_64) os=linux; arch=x64
      sha=6fac98f0c8202efc3135a9e9e42aa61963ec42d1c5cbb40caf231e7b14a24ef6
      bin=bin/java ;;
    Linux-aarch64) os=linux; arch=aarch64
      sha=963931723891302ebb2687178532c3d793d71e2577cd058061c7efb396140a86
      bin=bin/java ;;
    *) echo "check.sh: no pinned Java for $(uname -s) $(uname -m); set JAVA to a java binary" >&2; exit 2 ;;
  esac
  java=$tools/$arch-$os/$jre_dir/$bin
  if [ ! -x "$java" ]; then
    local tarball=$tools/jre-$arch-$os.tar.gz
    fetch "$jre_base/OpenJDK25U-jre_${arch}_${os}_hotspot_25.0.5_8.tar.gz" "$sha" "$tarball"
    mkdir -p "$tools/$arch-$os"
    tar -xzf "$tarball" -C "$tools/$arch-$os"
    # Only the unpacked runtime is used from here on.
    rm -f "$tarball"
  fi
}

work=$(mktemp -d "${TMPDIR:-/tmp}/residue-ui-tla.XXXXXX")
trap 'rm -rf "$work"' EXIT
# TLC looks up this machine's name once per run. On macOS, a name that is
# not in /etc/hosts makes Java wait 5 s for that lookup. A hosts file read
# only by this JVM answers it at once.
printf '127.0.0.1 localhost %s\n' "$(hostname)" > "$work/hosts"

# The setups. Each names its model and its constants; model/mutants names
# them too. Bounds: two worktrees, two dialog openings, two changes outside
# the app, and at most one fault in a behavior.
setups="remove/no-faults remove/reload remove/draw save/outside save/reload"

module_of() {
  case $1 in
    remove/*) echo RemoveFlow ;;
    save/*) echo SaveFlow ;;
    *) echo "check.sh: no setup $1" >&2; exit 2 ;;
  esac
}

rules_of() {
  case $1 in
    RemoveFlow) echo "TypeOK OnlyListedWorktreesMove ConfirmSendsTheList OpensOnlyForCanGo ListIsForYourChoice NothingMovedIsTrue OneRemovalAtATime" ;;
    SaveFlow) echo "TypeOK SaveIsWhatTheDialogShowed ReplacesOnlyWhatItShowed NeverDropsAnArgument WarnedBeforeItDeletes PreviewIsOfTheTickedFolders SaveIsOfTheTickedFolders TheDialogStaysWhileItSaves OneChangeAtATime" ;;
  esac
}

constants_of() {
  case $1 in
    remove/*)
      local faults
      case $1 in
        remove/no-faults) faults='{}' ;;
        remove/reload) faults='{"reload"}' ;;
        remove/draw) faults='{"draw"}' ;;
        *) echo "check.sh: no setup $1" >&2; exit 2 ;;
      esac
      printf '  %s\n' 'W = {"a", "b"}' "Faults = $faults" 'MaxOpens = 2' 'MaxChanges = 2' ;;
    save/outside)
      printf '  %s\n' 'Folders = {"A", "B"}' 'AppRoots = {"A"}' 'MaxOpens = 2' 'MaxOutside = 2' \
        'World = {"on", "off", "folders", "mode", "options", "argument", "brew"}' 'Faults = {}' ;;
    save/reload)
      # The window starts afresh while a save or turn-off runs: two calls of
      # the app, one of them from a dialog that is gone. Which folders they
      # save does not change whether they overlap, so one folder is enough;
      # with two, every rule held too, in 11,489,267 states (3,437,405
      # distinct), against 77,392 with one. A terminal and brew change the
      # service only while no call of the app runs, so they add nothing here
      # but states.
      printf '  %s\n' 'Folders = {"A"}' 'AppRoots = {"A"}' 'MaxOpens = 2' 'MaxOutside = 0' \
        'World = {}' 'Faults = {"reload"}' ;;
    *) echo "check.sh: no setup $1" >&2; exit 2 ;;
  esac
}

# One TLC run. Prints "holds N states, M distinct" (every state checked),
# "breaks RULE: ACTION ACTION ...", or "error: ...", and on standard error
# the path of TLC's output.
tlc() { # SETUP BROKEN RULES...
  local setup=$1 broken=$2; shift 2
  local module; module=$(module_of "$setup")
  local dir; dir=$(mktemp -d "$work/run.XXXXXX")
  cp "$model/$module.tla" "$dir/"
  {
    echo "SPECIFICATION Spec"
    echo "CONSTANTS"
    constants_of "$setup"
    echo "  Broken = $broken"
    echo "INVARIANTS"
    for r in "$@"; do echo "  $r"; done
  } > "$dir/run.cfg"
  local code=0
  (cd "$dir" && "$java" -XX:+UseParallelGC -Djdk.net.hosts.file="$work/hosts" \
    -cp "$jar" tlc2.TLC -workers 1 -fp 0 \
    -config run.cfg -metadir "$dir/states" "$module.tla" > tlc.out 2>&1) || code=$?
  local out=$dir/tlc.out
  if [ "$code" = 0 ] && grep -q "No error has been found" "$out"; then
    echo "holds $(sed -n 's/^\([0-9]*\) states generated, \([0-9]*\) distinct states found, 0 states left on queue.*/\1 states, \2 distinct/p' "$out" | tail -1)"
  elif [ "$code" = 12 ] && grep -q "^Error: Invariant .* is violated" "$out"; then
    local rule path
    rule=$(sed -n 's/^Error: Invariant \(.*\) is violated.*/\1/p' "$out" | head -1)
    path=$(grep -v '^State 1: ' "$out" | sed -n 's/^State [0-9]*: <\([A-Za-z]*\).*/\1/p' | tr '\n' ' ' | sed 's/ $//')
    echo "breaks $rule: $path"
  else
    echo "error: TLC exited $code; $(grep -m1 -E 'Error|Exception|deadlock' "$out" || tail -1 "$out")"
  fi
  echo "$out" >&2
}

setup_tools
"$java" -version > /dev/null 2>&1 || { echo "check.sh: $java does not run" >&2; exit 2; }

found=$work/found
holds=$work/holds
: > "$found"
: > "$holds"
failed=0
start_time=$(date +%s)

# Each setup: check every rule; when one breaks, record its shortest path
# and check the rest again without it. TLC stops at a rule's first break,
# so a rule that breaks is not checked further in that setup.
for setup in $setups; do
  all=$(rules_of "$(module_of "$setup")")
  left=$all
  while :; do
    # shellcheck disable=SC2086
    result=$(tlc "$setup" "{}" $left 2>"$work/last")
    case $result in
      holds*)
        if [ "$left" = "$all" ]; then
          echo "$setup: every rule holds (${result#holds })"
        else
          echo "$setup: the other rules hold (${result#holds })"
        fi
        for r in $left; do echo "$setup $r" >> "$holds"; done
        break ;;
      breaks*)
        rule=${result#breaks }; rule=${rule%%:*}
        echo "$setup: breaks ${result#breaks }"
        echo "$setup ${result#breaks }" >> "$found"
        left=$(echo " $left " | sed "s/ $rule / /; s/^ //; s/ $//")
        [ -n "$left" ] || break ;;
      *)
        echo "$setup: $result (output: $(cat "$work/last"))" >&2
        exit 2 ;;
    esac
  done
done

# Each mutant must break the rule it names in a setup where the code, with
# no guard removed, holds that rule: the setup loop above is that control.
while read -r mutant setup rule; do
  case $mutant in ''|\#*) continue ;; esac
  if ! grep -qx "$setup $rule" "$holds"; then
    echo "mutant $mutant: no control, $rule does not hold in $setup"
    failed=1
    continue
  fi
  # A+B removes both guards: Broken = {"A", "B"}.
  result=$(tlc "$setup" "{\"$(echo "$mutant" | sed 's/+/", "/g')\"}" "$rule" 2>"$work/last")
  case $result in
    "breaks $rule"*) echo "mutant $mutant: caught by ${result#breaks }" ;;
    holds*) echo "mutant $mutant: NOT caught, $rule still holds"; failed=1 ;;
    *) echo "mutant $mutant: $result (output: $(cat "$work/last"))" >&2; exit 2 ;;
  esac
done < "$model/mutants"

# The found paths against the known ones.
{ grep -v -e '^#' -e '^$' "$model/expected" || true; } | sort > "$work/expected"
sort "$found" > "$work/found.sorted"
if ! diff -u "$work/expected" "$work/found.sorted" > "$work/diff"; then
  echo
  echo "The result differs from model/expected (- expected, + found):"
  sed -n '3,$p' "$work/diff" | grep '^[-+]' || true
  echo "A + line is a new break or a changed path: read it with /statechart and replay it against the code."
  echo "A - line alone is a known break that no longer happens: a fix, or a model that lost the path."
  failed=1
fi

echo
echo "$(grep -m1 -o 'TLC2 Version [^ ]*' "$(cat "$work/last")"), $("$java" -version 2>&1 | head -1); $(( $(date +%s) - start_time ))s"
if [ "$failed" = 0 ]; then
  known=$(wc -l < "$work/expected" | tr -d ' ')
  if [ "$known" = 0 ]; then
    echo "matches model/expected: every rule holds, and every mutant is caught"
  else
    echo "matches model/expected: every rule holds except the $known known breaks listed there, and every mutant is caught"
  fi
fi
exit "$failed"
