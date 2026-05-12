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
root_dir="$base_dir/root"
evidence_dir="$base_dir/evidence"
log_path="$evidence_dir/gui-headful-smoke.log"
report_path="$evidence_dir/GUI-HEADFUL-SMOKE.local.md"

"$script_dir/gui-smoke-fixture.sh" "$base_dir" >/dev/null

if [[ "$no_build" -eq 0 ]]; then
  (cd "$repo_root/rust" && cargo build --bin flistwalker)
fi

app_path="$repo_root/rust/target/debug/flistwalker"
if [[ ! -x "$app_path" ]]; then
  echo "Missing executable: $app_path" >&2
  exit 1
fi

: >"$log_path"
FLISTWALKER_DISABLE_SELF_UPDATE=1 \
FLISTWALKER_RESTORE_TABS=0 \
  "$app_path" --root "$root_dir" --limit 1000 >"$log_path" 2>&1 &
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

cat >"$report_path" <<EOF
# GUI HEADFUL SMOKE

## Summary
- Status: $status
- Duration: ${duration_seconds}s
- Root: \`$root_dir\`
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
