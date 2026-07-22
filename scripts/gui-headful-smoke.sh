#!/usr/bin/env bash
set -euo pipefail

duration_seconds=10
no_build=0
base_dir=""

usage() {
  cat <<'EOF'
Usage: scripts/gui-headful-smoke.sh [--duration SECONDS] [--base-dir PATH] [--no-build]

Launches the native GUI against the GUI smoke fixture, waits briefly, and fails
if the app exits before the wait period. This is a headful release/nightly smoke,
not a normal PR test.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --duration)
      duration_seconds="$2"
      shift 2
      ;;
    --base-dir)
      base_dir="$2"
      shift 2
      ;;
    --no-build)
      no_build=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd "$script_dir/.." && pwd -P)"
base_dir="${base_dir:-"$repo_root/rust/target/gui-smoke"}"
mkdir -p "$base_dir"
base_dir="$(cd "$base_dir" && pwd -P)"
root_dir="$base_dir/root"
evidence_dir="$base_dir/evidence"

"$script_dir/gui-smoke-fixture.sh" --base-dir "$base_dir" >/dev/null

if [[ "$no_build" -eq 0 ]]; then
  if ! command -v cargo >/dev/null 2>&1 && [[ -n "${HOME:-}" && -f "$HOME/.cargo/env" ]]; then
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
  command -v cargo >/dev/null 2>&1 || {
    echo "cargo is unavailable; install Rust or expose the existing toolchain in PATH" >&2
    exit 1
  }
  (cd "$repo_root/rust" && cargo build --bin flistwalker)
fi

source_app_path="$repo_root/rust/target/debug/flistwalker"
if [[ ! -x "$source_app_path" ]]; then
  echo "Missing executable: $source_app_path" >&2
  exit 1
fi

run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_dir="$base_dir/runs/$run_id"
app_dir="$run_dir/app"
mkdir -p "$base_dir/runs"
mkdir "$run_dir"
mkdir "$app_dir"

app_path="$app_dir/flistwalker"
cp "$source_app_path" "$app_path"
cp "$base_dir/flistwalker.ignore.txt" "$app_dir/flistwalker.ignore.txt"
cp "$repo_root/flistwalker.ignore.txt.example" "$app_dir/flistwalker.ignore.txt.example"
chmod +x "$app_path"

assert_staged_app() {
  local entries expected name
  shopt -s nullglob dotglob
  entries=("$app_dir"/*)
  shopt -u nullglob dotglob
  [[ "${#entries[@]}" -eq 3 ]] || {
    echo "Staged app allowlist mismatch: expected 3 entries, found ${#entries[@]}" >&2
    return 1
  }
  expected=$'flistwalker\nflistwalker.ignore.txt\nflistwalker.ignore.txt.example'
  name="$(printf '%s\n' "${entries[@]##*/}" | LC_ALL=C sort)"
  [[ "$name" == "$expected" ]] || {
    echo "Staged app allowlist mismatch" >&2
    return 1
  }
  if compgen -G "$app_dir/.flistwalker-update*" >/dev/null; then
    echo "Updater transaction artifact found in staged app directory" >&2
    return 1
  fi
}

assert_staged_app

log_path="$evidence_dir/gui-headful-smoke-$run_id.log"
report_path="$evidence_dir/GUI-HEADFUL-SMOKE-$run_id.local.md"
: >"$log_path"
FLISTWALKER_DISABLE_SELF_UPDATE=1 \
FLISTWALKER_DISABLE_HISTORY_PERSIST=1 \
FLISTWALKER_RESTORE_TABS=0 \
  env -u HOME "$app_path" --root "$root_dir" --limit 1000 >"$log_path" 2>&1 &
pid=$!

status="PASS"
notes="Process stayed alive for ${duration_seconds}s and was stopped by the smoke script."
for _ in $(seq 1 "$duration_seconds"); do
  if ! kill -0 "$pid" 2>/dev/null || ps -p "$pid" -o stat= 2>/dev/null | grep -q "Z"; then
    set +e
    wait "$pid"
    exit_code=$?
    set -e
    status="FAIL"
    notes="Process exited early with code ${exit_code}. See log: $log_path"
    break
  fi
  sleep 1
done

if kill -0 "$pid" 2>/dev/null; then
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
fi

assert_staged_app

cat >"$report_path" <<EOF
# GUI HEADFUL SMOKE

## Summary
- Status: $status
- Duration: ${duration_seconds}s
- Root: \`$root_dir\`
- Staged executable: \`$app_path\`
- Settings isolation: child HOME removed; no settings or legacy home is available
- Pre/post staged allowlist: PASS (binary + ignore + sample)
- Pre/post updater artifacts: PASS (no \`.flistwalker-update*\`)
- Log: \`$log_path\`

## Notes
- $notes
EOF

cat <<EOF
GUI headful smoke: $status
Report: $report_path
Log: $log_path
EOF

[[ "$status" == "PASS" ]]
