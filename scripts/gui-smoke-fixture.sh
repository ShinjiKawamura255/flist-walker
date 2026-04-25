#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd "$script_dir/.." && pwd -P)"
base_dir="${1:-"$repo_root/rust/target/gui-smoke"}"
root_dir="$base_dir/root"
evidence_dir="$base_dir/evidence"
debug_dir="$repo_root/rust/target/debug"

mkdir -p \
  "$root_dir/docs" \
  "$root_dir/nested" \
  "$root_dir/ignored" \
  "$root_dir/actions" \
  "$root_dir/empty-dir" \
  "$evidence_dir" \
  "$debug_dir"

cat >"$root_dir/README-preview.txt" <<'EOF'
FlistWalker GUI smoke fixture

This file is used for preview, search highlighting, and action routing checks.
Search terms: alpha beta gamma exact-start final-end
EOF

cat >"$root_dir/docs/alpha-report.md" <<'EOF'
# Alpha Report

Use this file for non-empty query, preview, and sort checks.
EOF

cat >"$root_dir/docs/beta_notes.txt" <<'EOF'
beta note
This file is intentionally plain text for preview checks.
EOF

cat >"$root_dir/actions/open-target.txt" <<'EOF'
Open action target for manual GUI smoke.
EOF

cat >"$root_dir/actions/space name.txt" <<'EOF'
Path with a space for copy/open path checks.
EOF

cat >"$root_dir/ignored/old.tmp" <<'EOF'
This file should be hidden when the executable-local ignore list is enabled.
EOF

cat >"$root_dir/nested/child-one.txt" <<'EOF'
nested child one
EOF

cat >"$root_dir/nested/child-two.log" <<'EOF'
nested child two
EOF

cat >"$root_dir/nested/FileList.txt" <<'EOF'
child-one.txt
child-two.log
EOF

cat >"$root_dir/FileList.txt" <<'EOF'
README-preview.txt
docs/alpha-report.md
docs/beta_notes.txt
actions/open-target.txt
actions/space name.txt
ignored/old.tmp
nested/FileList.txt
empty-dir
EOF

cat >"$debug_dir/flistwalker.ignore.txt" <<'EOF'
old.tmp
ignored
EOF

cat >"$evidence_dir/GUI-TESTREPORT.local.md" <<EOF
# GUI TESTREPORT

## Summary
- Date:
- Tester:
- Build/version:
- OS/display:
- Fixture root: \`$root_dir\`
- Evidence dir: \`$evidence_dir\`
- Command: \`cd "$repo_root/rust" && cargo run -- --root "$root_dir" --limit 1000\`

## Results
| ID | Status | Notes | Evidence |
| --- | --- | --- | --- |
| GSM-001 | NOT RUN | Startup/indexing | |
| GSM-002 | NOT RUN | Search/highlight/operators | |
| GSM-003 | NOT RUN | Preview and selection movement | |
| GSM-004 | NOT RUN | Open/copy action routing | |
| GSM-005 | NOT RUN | Sort modes | |
| GSM-006 | NOT RUN | FileList source and Create File List dialog | |
| GSM-007 | NOT RUN | Tabs and per-tab state | |
| GSM-008 | NOT RUN | Dialog cancel/failure handling | |
| GSM-009 | NOT RUN | Light/dark theme visual pass | |
| GSM-010 | NOT RUN | Responsiveness during indexing/search | |

## Defects
- None recorded.

## Follow-ups
- Replace NOT RUN with PASS/FAIL/SKIPPED and record evidence before release.
EOF

cat <<EOF
GUI smoke fixture prepared.
Root: $root_dir
Evidence: $evidence_dir
Executable-local ignore list for cargo run: $debug_dir/flistwalker.ignore.txt

Run:
  cd "$repo_root/rust"
  cargo run -- --root "$root_dir" --limit 1000

Record results in:
  $evidence_dir/GUI-TESTREPORT.local.md
EOF
