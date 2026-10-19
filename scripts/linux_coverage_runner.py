#!/usr/bin/env python3
"""Cargo runner: preserve argv/signals while recording immutable ELF identities."""
import os
from pathlib import Path
import sys

from linux_coverage_diagnostics import DiagnosticError, register_binary


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    root = os.environ.get("FLISTWALKER_COVERAGE_DIRECTORY")
    if sys.platform != "linux" or not root or not argv:
        return 2
    try:
        path = Path(argv[0]).resolve(strict=True)
        register_binary(path, Path(root))
        # Integration tests may start the workspace CLI binaries directly.
        # These fixed companions are built by this same cargo invocation.
        if path.parent.name == "deps":
            for name in ("fw", "flistwalker"):
                companion = path.parent.parent / name
                if companion.is_file() and os.access(companion, os.X_OK):
                    register_binary(companion, Path(root))
        os.execv(str(path), argv)
    except (DiagnosticError, OSError):
        print("coverage_runner_error=binary_identity_or_exec", file=sys.stderr, flush=True)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
