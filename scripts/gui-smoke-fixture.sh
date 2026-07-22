#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd "$script_dir/.." && pwd -P)"
base_dir="$repo_root/rust/target/gui-smoke"
verify_only=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-dir)
      base_dir="$2"
      shift 2
      ;;
    --verify-only)
      verify_only=1
      shift
      ;;
    -h|--help)
      echo "Usage: scripts/gui-smoke-fixture.sh [--base-dir PATH] [--verify-only]"
      exit 0
      ;;
    *)
      # Preserve the original positional BaseDir contract.
      base_dir="$1"
      shift
      ;;
  esac
done
root_dir="$base_dir/root"
evidence_dir="$base_dir/evidence"
fixture_dir="$repo_root/rust/tests/fixtures/gui-smoke"
manifest_path="$fixture_dir/SHA256SUMS"
spec_path="$fixture_dir/fixture-spec.tsv"

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "Neither sha256sum nor shasum is available" >&2
    return 1
  fi
}

spec_value() {
  local key="$1"
  awk -F '\t' -v key="$key" '$1 == key { print $2 }' "$spec_path"
}

validate_fixture() {
  local expected_hash relative_path actual_hash expected_root expected_nested
  while read -r expected_hash relative_path; do
    [[ -n "$expected_hash" && -n "$relative_path" ]] || continue
    [[ -f "$base_dir/$relative_path" ]] || {
      echo "Missing GUI fixture file: $base_dir/$relative_path" >&2
      return 1
    }
    actual_hash="$(sha256_file "$base_dir/$relative_path")"
    [[ "$actual_hash" == "$expected_hash" ]] || {
      echo "GUI fixture hash mismatch: $relative_path" >&2
      return 1
    }
  done <"$manifest_path"

  expected_root="$(spec_value root_filelist_entries)"
  expected_nested="$(spec_value nested_filelist_entries)"
  [[ -n "$expected_root" && -n "$expected_nested" ]] || {
    echo "GUI fixture spec is incomplete" >&2
    return 1
  }
  [[ "$(awk 'NF { count++ } END { print count + 0 }' "$root_dir/FileList.txt")" == "$expected_root" ]] || {
    echo "Root FileList entry count mismatch" >&2
    return 1
  }
  [[ "$(awk 'NF { count++ } END { print count + 0 }' "$root_dir/nested/FileList.txt")" == "$expected_nested" ]] || {
    echo "Nested FileList entry count mismatch" >&2
    return 1
  }
}

[[ -f "$manifest_path" && -f "$spec_path" ]] || {
  echo "Canonical GUI fixture metadata is missing" >&2
  exit 1
}

if [[ "$verify_only" -eq 0 ]]; then
  mkdir -p "$root_dir" "$root_dir/empty-dir" "$evidence_dir"
  cp -R "$fixture_dir/root/." "$root_dir/"
  cp "$fixture_dir/flistwalker.ignore.txt" "$base_dir/flistwalker.ignore.txt"
fi

validate_fixture

if [[ "$verify_only" -eq 0 && ! -f "$evidence_dir/GUI-TESTREPORT.local.md" ]]; then
  cp "$repo_root/docs/GUI-TESTREPORT.template.md" "$evidence_dir/GUI-TESTREPORT.local.md"
fi

cat <<EOF
GUI smoke fixture prepared.
Root: $root_dir
Evidence: $evidence_dir
Canonical hashes and FileList counts: PASS

Record results in:
  $evidence_dir/GUI-TESTREPORT.local.md
EOF
