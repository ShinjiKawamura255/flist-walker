#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${REPO_DIR}"

if [[ -f ".venv/bin/activate" ]]; then
  # Prefer the project's virtual environment when available.
  # shellcheck disable=SC1091
  source .venv/bin/activate
fi

if ! python -c "import PySide6" >/dev/null 2>&1; then
  echo "PySide6 が見つかりません。先に 'pip install -e .[dev,gui]' を実行してください。" >&2
  exit 1
fi

export PYTHONPATH="${REPO_DIR}/src${PYTHONPATH:+:${PYTHONPATH}}"

exec python -m fast_file_finder --gui "$@"
