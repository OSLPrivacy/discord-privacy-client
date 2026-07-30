#!/usr/bin/env bash
# Build "instance B" -- a second, distinctly-identified OSL Hub -- from WSL.
#
# WHY THIS EXISTS ALONGSIDE osl-instance-b-build.ps1
#   The PowerShell sibling drives `tauri build --config <overlay>` with the
#   Windows MSVC toolchain. That is not the toolchain this machine's current
#   instance A was produced with: A's binary carries WSL debuginfo paths
#   ($HOME/.cargo/registry/...) and this checkout's only Windows target
#   directory is target/x86_64-pc-windows-gnu. Building B with a different
#   toolchain than A would make every cross-instance difference ambiguous --
#   toolchain or product? -- so B is cross-compiled the same way A was.
#
# HOW THE IDENTIFIER IS CHANGED WITHOUT EDITING A TRACKED FILE
#   tauri-build reads the TAURI_CONFIG environment variable and json_patch-
#   merges it over apps/osl-hub/tauri.conf.json
#   (tauri-build-2.6.1/src/lib.rs:487). Setting only `identifier` changes the
#   single-instance mutex/marker class, %APPDATA%\<id> identity root and
#   %LOCALAPPDATA%\<id> WebView2 folder together, and nothing else. productName
#   and window title are deliberately left alone: both instances stay titled
#   "OSL Privacy" and are told apart by the -sic marker class, which is the
#   only identity that was ever reliable.
#
# WHAT THIS DOES NOT DO
#   It does not run the result, create an identity, or contact the keyserver.
#   Starting a discord-qa-shell build does the last two; that is the owner's
#   decision and it happens in osl-launch-instance-b.ps1.
#
# The frontend is NOT rebuilt. apps/osl-hub-ui/dist is embedded at compile
# time, and reusing the exact dist instance A embedded is what makes a
# renderer difference impossible to confuse with a product difference. The
# script refuses if a non-test frontend source is newer than dist.
set -uo pipefail

if [ "${1:-}" = "--self-test" ]; then
  instance_b_build_requires_distinct_identifier() {
    local tmp json log out rc
    tmp="$(mktemp -d)"
    json="$tmp/result.json"
    log="$tmp/build.log"
    out="$tmp/output.txt"

    JSON_OUT="$json" LOG="$log" BUNDLE_A="org.oslprivacy.hub" \
      bash "$0" "org.oslprivacy.hub" >"$out" 2>&1
    rc=$?

    if [ "$rc" -ne 2 ]; then
      printf 'expected exit 2, got %s\n' "$rc" >&2
      return 1
    fi
    python3 - "$json" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    receipt = json.load(handle)
assert receipt["identifier"] == "org.oslprivacy.hub"
assert receipt["overall"]["verdict"] == "blocked"
assert "Identifier equals instance A" in receipt["overall"]["diagnosis"]
assert receipt["diffKey"].startswith("BLOCKED:")
PY
  }

  instance_b_build_requires_distinct_identifier
  exit $?
fi

REPO="${REPO:-$HOME/discord-privacy-client}"
IDENTIFIER="${1:-org.oslprivacy.hubqab}"
BUNDLE_A="${BUNDLE_A:-org.oslprivacy.hub}"
STAGE_WIN="${STAGE_WIN:-C:\\OSL-QA-B}"
STAGE="${STAGE:-/mnt/c/OSL-QA-B}"
OSL_WIN_TEMP_ROOT="${OSL_WIN_TEMP_ROOT:-/mnt/c/OSL-QA-Temp}"
TARGET="x86_64-pc-windows-gnu"
JSON_OUT="${JSON_OUT:-$OSL_WIN_TEMP_ROOT/osl-instance-b-build.json}"
LOG="${LOG:-/tmp/osl-instance-b-build.log}"
RUN_START="$(date -Is)"

say() { printf '%s\n' "$*" >&2; }
fail() {
  local diagnosis="$1" remedy="$2"
  python3 - "$JSON_OUT" "$RUN_START" "$IDENTIFIER" "$diagnosis" "$remedy" <<'PY'
import json, sys
out, start, ident, diag, remedy = sys.argv[1:6]
json.dump({
  "schemaVersion": 1, "tool": "osl-instance-b-build-wsl", "runStartedAt": start,
  "identifier": ident,
  "overall": {"verdict": "blocked", "diagnosis": diag, "remedy": remedy},
  "diffKey": "BLOCKED:" + diag,
}, open(out, "w"), indent=2)
PY
  say "BUILD-B VERDICT: BLOCKED -- $diagnosis"
  say "next: $remedy"
  exit 2
}

[ "$IDENTIFIER" = "$BUNDLE_A" ] && fail \
  "-Identifier equals instance A's ($BUNDLE_A); that build can never run beside A." \
  "Pick a different identifier, e.g. org.oslprivacy.hubqab."
[[ "$IDENTIFIER" =~ ^[A-Za-z0-9][A-Za-z0-9.-]{2,}$ ]] || fail \
  "'$IDENTIFIER' is not a usable bundle identifier (it becomes a window-class and a directory name)." \
  "Use plain alphanumerics, dots and hyphens."

cd "$REPO" || fail "Repository root $REPO not found." "Set REPO."
[ -f apps/osl-hub/tauri.conf.json ] || fail "apps/osl-hub/tauri.conf.json not found under $REPO." "Set REPO."

# dist freshness: only non-test frontend sources count.
newest_src=$(find apps/osl-hub-ui/src apps/osl-hub-ui/*.html -type f \
  ! -name '*.test.ts' ! -name '*.test.tsx' -printf '%T@\n' 2>/dev/null | sort -rn | head -1)
newest_dist=$(find apps/osl-hub-ui/dist -type f -printf '%T@\n' 2>/dev/null | sort -rn | head -1)
[ -n "$newest_dist" ] || fail "apps/osl-hub-ui/dist is missing or empty." \
  "Run 'npm --prefix apps/osl-hub-ui run build' first; Tauri embeds dist at compile time."
awk -v a="$newest_src" -v b="$newest_dist" 'BEGIN{exit !(a>b)}' && fail \
  "A non-test frontend source is newer than apps/osl-hub-ui/dist, so B would embed a stale renderer." \
  "Rebuild dist, then rebuild instance A too -- or the two instances differ in the renderer."

say "=== OSL instance-B cross-build (WSL -> $TARGET) ==="
say "identifier : $IDENTIFIER  (A stays $BUNDLE_A)"
say "log        : $LOG"

export CARGO_BUILD_JOBS=4
build_start=$(date +%s)
flock /tmp/osl-cargo.lock -c "cd '$REPO/apps/osl-hub' && \
  TAURI_CONFIG='{\"identifier\":\"$IDENTIFIER\"}' \
  cargo build --features desktop,discord-qa-shell --bin osl-privacy-hub --target $TARGET" \
  >"$LOG" 2>&1
rc=$?
build_secs=$(( $(date +%s) - build_start ))
[ $rc -eq 0 ] || fail "cargo build exited $rc after ${build_secs}s. Tail: $(tail -3 "$LOG" | tr '\n' ' ')" \
  "Read $LOG."

# apps/osl-hub is NOT a member of the root workspace, so its cargo invocation
# writes to apps/osl-hub/target, not $REPO/target. Looking only in the repo
# root made a SUCCESSFUL build report "cargo reported success but the exe does
# not exist" -- a false blocked verdict on a binary that was sitting on disk.
# The hub-local directory is preferred and the workspace one kept as a fallback
# in case the hub is ever adopted into the workspace.
EXE=""
for candidate in \
  "$REPO/apps/osl-hub/target/$TARGET/debug/osl-privacy-hub.exe" \
  "$REPO/target/$TARGET/debug/osl-privacy-hub.exe"
do
  [ -f "$candidate" ] && { EXE="$candidate"; break; }
done
[ -n "$EXE" ] || fail \
  "cargo reported success but no osl-privacy-hub.exe exists under either $REPO/apps/osl-hub/target/$TARGET/debug or $REPO/target/$TARGET/debug." \
  "Read $LOG."

# The identifier is embedded at compile time; prove it is in the artefact
# before anything is staged. A silently-ignored TAURI_CONFIG would otherwise
# produce a second binary that races A for one mutex.
if ! grep -qa -- "$IDENTIFIER" "$EXE"; then
  fail "The built exe does not contain the string '$IDENTIFIER', so the TAURI_CONFIG overlay did not take." \
       "Check TAURI_CONFIG handling in tauri-build; do not run this binary."
fi

mkdir -p "$STAGE" || fail "Cannot create $STAGE." "Check permissions."
A_EXE="$(ls -1 "$OSL_WIN_TEMP_ROOT"/osl-*/osl-privacy-hub.exe 2>/dev/null | head -1)"
if [ -n "$A_EXE" ] && [ "$(readlink -f "$A_EXE")" = "$(readlink -f "$STAGE/osl-privacy-hub.exe")" ]; then
  fail "Refusing to stage over instance A's executable." "Pick another STAGE."
fi
cp -f "$EXE" "$STAGE/osl-privacy-hub.exe" || fail "Staging copy failed." "Check $STAGE."

# The exe alone hangs before main() with no window and no trace file at all.
DLL_SRC="${DLL_SRC:-}"
if [ -z "$DLL_SRC" ]; then
  DLL_SRC="$(ls -1t "$OSL_WIN_TEMP_ROOT"/osl-*/WebView2Loader.dll 2>/dev/null | head -1)"
fi
[ -n "$DLL_SRC" ] && [ -f "$DLL_SRC" ] || fail \
  "WebView2Loader.dll was not found next to any staged OSL build." \
  "Set DLL_SRC to a WebView2Loader.dll; the exe alone hangs pre-main."
cp -f "$DLL_SRC" "$STAGE/WebView2Loader.dll" || fail "DLL staging copy failed." "Check $STAGE."

exe_sha=$(sha256sum "$STAGE/osl-privacy-hub.exe" | cut -d' ' -f1)
dll_sha=$(sha256sum "$STAGE/WebView2Loader.dll" | cut -d' ' -f1)
a_sha=""
[ -n "$A_EXE" ] && a_sha=$(sha256sum "$A_EXE" | cut -d' ' -f1)

python3 - "$JSON_OUT" "$RUN_START" "$IDENTIFIER" "$STAGE_WIN" "$exe_sha" "$dll_sha" \
         "$a_sha" "$A_EXE" "$build_secs" "$LOG" "$TARGET" <<'PY'
import json, sys
(out, start, ident, stage, exe_sha, dll_sha, a_sha, a_exe, secs, log, target) = sys.argv[1:12]
json.dump({
  "schemaVersion": 1, "tool": "osl-instance-b-build-wsl", "runStartedAt": start,
  "identifier": ident, "target": target, "profile": "debug",
  "stageDir": stage, "buildSeconds": int(secs), "log": log,
  "exeSha256": exe_sha, "webview2LoaderSha256": dll_sha,
  "instanceA": {"exe": a_exe, "sha256": a_sha},
  "distinctFromA": bool(a_sha) and a_sha != exe_sha,
  "overall": {"verdict": "ok",
              "diagnosis": "instance B staged to %s" % stage,
              "remedy": "osl-launch-instance-b.ps1 -ExeB %s\\osl-privacy-hub.exe -BundleB %s -ConfirmCreatesIdentity" % (stage, ident)},
  "diffKey": "OK:%s:%s" % (ident, exe_sha[:12]),
}, open(out, "w"), indent=2)
PY

say ""
say "BUILD-B VERDICT: OK -- staged to $STAGE_WIN in ${build_secs}s"
say "exe sha256 : $exe_sha"
say "A  sha256  : ${a_sha:-<none found>}"
say "JSON       : $JSON_OUT"
