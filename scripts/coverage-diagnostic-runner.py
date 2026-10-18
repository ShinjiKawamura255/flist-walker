#!/usr/bin/env python3
"""Temporary Linux coverage runner: trace libtest faults without changing test arguments."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys


def main(arguments=None):
    argv = list(sys.argv[1:] if arguments is None else arguments)
    if not argv or platform.system() != "Linux":
        print("coverage diagnostic requires a Linux executable argument", file=sys.stderr)
        return 2
    if not Path(argv[0]).name.startswith("flist_walker-"):
        result = subprocess.run(argv, shell=False, check=False)
    else:
        gdb, timeout = shutil.which("gdb"), shutil.which("timeout")
        if not gdb or not timeout:
            print("coverage diagnostic requires GDB and GNU timeout", file=sys.stderr)
            return 2
        binary = Path(argv[0])
        with binary.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        print("coverage_diagnostic_binary=" + json.dumps({
            "name": binary.name, "pid": os.getpid(), "size": binary.stat().st_size, "sha256": digest,
        }), file=sys.stderr, flush=True)
        core_dir = os.environ.get("COVERAGE_DIAGNOSTIC_CORE_DIR")
        if core_dir:
            index = Path(core_dir) / f"index-{os.getpid()}.json"
            with index.open("x", encoding="utf-8") as output:
                os.chmod(index, 0o600)
                json.dump({"binary": str(binary.resolve()), "sha256": digest}, output)
            os.execv(str(binary.resolve()), argv)
        command = [timeout, "--signal=TERM", "--kill-after=10s", "600s", gdb,
                   "--nx", "--batch", "--init-eval-command=set auto-load off", "--return-child-result", "--command",
                   str(Path(__file__).with_name("coverage-diagnostic-gdb.py")), "--args", *argv]
        result = subprocess.run(command, shell=False, check=False)
    return result.returncode if result.returncode >= 0 else 128 - result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
