#!/usr/bin/env bash
set -euo pipefail

base_dir=""

usage() {
  cat <<'EOF'
Usage: scripts/gui-deterministic-scenarios.sh [--base-dir PATH]

Runs the canonical deterministic GUI owner-test groups and writes a local
deterministic-axis report. It does not launch a native GUI or OS action.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-dir)
      base_dir="$2"
      shift 2
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
inventory_path="$script_dir/gui-deterministic-scenarios.tsv"
base_dir="${base_dir:-"$repo_root/rust/target/gui-smoke"}"
evidence_dir="$base_dir/evidence"
mkdir -p "$evidence_dir"
base_dir="$(cd "$base_dir" && pwd -P)"
evidence_dir="$base_dir/evidence"
cargo_target_dir="$base_dir/cargo-target-bash"
mkdir -p "$cargo_target_dir"

if ! command -v cargo >/dev/null 2>&1 && [[ -n "${HOME:-}" && -f "$HOME/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
fi
command -v cargo >/dev/null 2>&1 || {
  echo "cargo is unavailable; install Rust or expose the existing toolchain in PATH" >&2
  exit 1
}

[[ -f "$inventory_path" ]] || {
  echo "Missing deterministic GUI inventory: $inventory_path" >&2
  exit 1
}

header="$(head -n 1 "$inventory_path" | tr -d '\r')"
[[ "$header" == $'group\tfilter\tmin_discovered\tmin_executed\tskip' ]] || {
  echo "Invalid deterministic GUI inventory header" >&2
  exit 1
}

run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
list_log="$evidence_dir/gui-deterministic-list-$run_id.log"
report_path="$evidence_dir/GUI-DETERMINISTIC-$run_id.local.md"

set +e
(cd "$repo_root/rust" && CARGO_TERM_COLOR=never CARGO_TARGET_DIR="$cargo_target_dir" cargo test --locked --lib -- --list) >"$list_log" 2>&1
list_exit=$?
set -e
if [[ "$list_exit" -ne 0 ]]; then
  echo "Deterministic GUI test discovery failed; see $list_log" >&2
  exit "$list_exit"
fi

rows=()
overall="PASS"
group_count=0

while IFS=$'\t' read -r group filter min_discovered min_executed skip extra; do
  group="${group%$'\r'}"
  filter="${filter%$'\r'}"
  min_discovered="${min_discovered%$'\r'}"
  min_executed="${min_executed%$'\r'}"
  skip="${skip%$'\r'}"
  [[ -n "$group" ]] || continue
  [[ -z "${extra:-}" && "$min_discovered" =~ ^[1-9][0-9]*$ && "$min_executed" =~ ^[1-9][0-9]*$ ]] || {
    echo "Invalid deterministic GUI inventory row: $group" >&2
    exit 1
  }

  discovered="$(awk -v filter="$filter" 'index($0, filter) && /: test$/ { count++ } END { print count + 0 }' "$list_log")"
  if (( discovered < min_discovered )); then
    rows+=("| $group | FAIL | $filter | $discovered | 0 | 0 | discovery under minimum $min_discovered |")
    overall="FAIL"
    break
  fi
  if [[ "$skip" != "-" ]] && ! grep -Fq "$skip" "$list_log"; then
    rows+=("| $group | FAIL | $filter | $discovered | 0 | 0 | explicit skip not discovered |")
    overall="FAIL"
    break
  fi

  group_count=$((group_count + 1))
  group_log="$evidence_dir/gui-deterministic-$(printf '%02d' "$group_count")-$group-$run_id.log"
  command=(cargo test --locked --lib "$filter" --)
  if [[ "$skip" != "-" ]]; then
    command+=(--skip "$skip")
  fi

  set +e
  (cd "$repo_root/rust" && CARGO_TERM_COLOR=never CARGO_TARGET_DIR="$cargo_target_dir" "${command[@]}") >"$group_log" 2>&1
  exit_code=$?
  set -e

  summary="$(grep 'test result:' "$group_log" | tail -n 1 || true)"
  passed="$(printf '%s\n' "$summary" | sed -n 's/.* \([0-9][0-9]*\) passed;.*/\1/p')"
  ignored="$(printf '%s\n' "$summary" | sed -n 's/.*; \([0-9][0-9]*\) ignored;.*/\1/p')"
  passed="${passed:-0}"
  ignored="${ignored:-0}"

  if [[ "$exit_code" -ne 0 || "$passed" -lt "$min_executed" || "$ignored" -ne 0 ]]; then
    rows+=("| $group | FAIL | $filter | $discovered | $passed | $ignored | exit=$exit_code; log=$group_log |")
    overall="FAIL"
    break
  fi
  rows+=("| $group | PASS | $filter | $discovered | $passed | $ignored | $group_log |")
done < <(tail -n +2 "$inventory_path")

if [[ "$group_count" -eq 0 ]]; then
  echo "Deterministic GUI inventory has no groups" >&2
  exit 1
fi

{
  echo "# GUI DETERMINISTIC SCENARIOS"
  echo
  echo "## Summary"
  echo "- Status: $overall"
  echo "- Inventory: \`$inventory_path\`"
  echo "- Discovery log: \`$list_log\`"
  echo "- Native interaction: NOT RUN (this harness is deterministic only)"
  echo "- Liveness: NOT RUN (use an isolated headful smoke script)"
  echo
  echo "## Groups"
  echo "| Group | Status | Filter | Discovered | Passed | Ignored | Evidence |"
  echo "| --- | --- | --- | ---: | ---: | ---: | --- |"
  printf '%s\n' "${rows[@]}"
} >"$report_path"

echo "GUI deterministic scenarios: $overall"
echo "Report: $report_path"
[[ "$overall" == "PASS" ]]
