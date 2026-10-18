#!/usr/bin/env python3
"""Feature-only hosted Linux diagnosis; never publishes raw process cores."""
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile
import time

try:
    import resource
except ImportError:
    resource = None


ROOT = Path(__file__).resolve().parents[1]
LIMIT = 2 * 1024 ** 3
EXPECTED = ["cargo", "llvm-cov", "--locked", "--workspace", "--lcov", "--output-path",
            "target/llvm-cov/lcov.info", "--fail-under-lines", "75"]


def report_cores(directory, smoke=False):
    count = 0
    for core in sorted(directory.glob("core-*")):
        pid = core.name.removeprefix("core-")
        index = directory / f"index-{pid}.json"
        if not pid.isdigit() or core.is_symlink() or not index.is_file() or index.is_symlink():
            raise RuntimeError("Unmapped core retained only inside the disposable VM")
        identity = json.loads(index.read_text())
        binary = Path(identity["binary"])
        with binary.open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != identity["sha256"]:
                raise RuntimeError("Core executable identity changed")
        with core.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        print("coverage_core=" + json.dumps({"pid": int(pid), "bytes": core.stat().st_size,
              "sha256": digest, "at_size_limit": core.stat().st_size >= LIMIT, "smoke": smoke}), flush=True)
        env = dict(os.environ, COVERAGE_DIAGNOSTIC_CORE=str(core))
        result = subprocess.run(["timeout", "--kill-after=10s", "90s", "gdb", "--nx", "--batch",
                 "--init-eval-command=set auto-load off", "--command",
                 str(ROOT / "scripts/coverage-diagnostic-gdb.py"), "--args", str(binary)],
                 env=env, capture_output=True, text=True, check=False)
        # Do not print GDB stderr or the discarded core header, which can contain argv.
        if "PRIVATE_ARGUMENT_PROBE" in result.stdout:
            raise RuntimeError("Core diagnostic exposed an argument value; output withheld")
        print(result.stdout, end="", flush=True)
        if core.stat().st_size >= LIMIT or "truncat" in result.stderr.lower():
            print("coverage_core_status=PARTIAL_CORE_LIMIT_OR_TRUNCATION", flush=True)
        if result.returncode or '"signal_number":' not in result.stdout:
            raise RuntimeError("Core stack extraction failed; raw core remains VM-local")
        core.unlink()
        index.unlink()
        count += 1
    return count


def main(arguments=None):
    argv = list(sys.argv[1:] if arguments is None else arguments)
    if argv[:1] == ["--"]:
        argv = argv[1:]
    if (argv != EXPECTED or resource is None or platform.system() != "Linux" or platform.machine() != "x86_64"
            or os.environ.get("GITHUB_ACTIONS") != "true" or not os.environ.get("RUNNER_TEMP")):
        print("Only the exact hosted Linux coverage diagnosis is allowed", file=sys.stderr)
        return 2
    directory = Path(tempfile.mkdtemp(prefix="coverage-cores-", dir=os.environ["RUNNER_TEMP"]))
    directory.chmod(0o700)
    previous = subprocess.check_output(["sysctl", "-n", "kernel.core_pattern"], text=True, timeout=10).strip()
    limits = resource.getrlimit(resource.RLIMIT_CORE)
    old_mask = os.umask(0o077)
    handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM)}
    def interrupted(sig, frame):
        raise SystemExit(128 + sig)
    primary = 0
    try:
        for sig in handlers:
            signal.signal(sig, interrupted)
        resource.setrlimit(resource.RLIMIT_CORE, (LIMIT, limits[1]))
        subprocess.run(["sudo", "-n", "sysctl", "-q", "-w", f"kernel.core_pattern={directory}/core-%p"], check=True, timeout=15)
        print("coverage_core_policy=" + json.dumps({"limit_bytes": LIMIT, "attempts": 5,
              "live_ptrace": False, "raw_core_upload": False}), flush=True)
        env = dict(os.environ, COVERAGE_DIAGNOSTIC_CORE_DIR=str(directory))
        fixture = directory / "flist_walker-core-smoke"
        subprocess.run(["cc", "-g", "-O0", str(ROOT / "scripts/tests/fixtures/coverage-crash.c"),
                        "-o", str(fixture)], check=True, timeout=30)
        result = subprocess.run([sys.executable, str(ROOT / "scripts/coverage-diagnostic-runner.py"),
                                 str(fixture), "fault", "PRIVATE_ARGUMENT_PROBE"], env=env, check=False, timeout=30)
        if result.returncode != -signal.SIGSEGV or report_cores(directory, smoke=True) != 1:
            raise RuntimeError("Native core smoke failed; actual coverage was not started")
        fixture.unlink()
        started = time.monotonic()
        for attempt in range(1, 6):
            budget = max(1, int(900 - (time.monotonic() - started)))
            result = subprocess.run(["timeout", "--signal=TERM", "--kill-after=10s", f"{budget}s", *argv],
                                    env=env, shell=False, check=False)
            primary = result.returncode if result.returncode >= 0 else 128 - result.returncode
            print("coverage_observation=" + json.dumps({"attempt": attempt, "exit": primary}), flush=True)
            if primary:
                if not report_cores(directory):
                    print("coverage_core_status=NOT_RUN_no_mapped_core", flush=True)
                break
    except Exception as error:
        print("coverage_diagnosis_error=" + type(error).__name__, file=sys.stderr, flush=True)
        if not primary:
            primary = 2
    finally:
        try:
            subprocess.run(["sudo", "-n", "sysctl", "-q", "-w", "kernel.core_pattern=" + previous], check=True, timeout=15)
            resource.setrlimit(resource.RLIMIT_CORE, limits)
            print("coverage_core_pattern_restored=true", flush=True)
        except Exception:
            print("coverage_core_restore_failed=true", file=sys.stderr, flush=True)
            if not primary:
                primary = 2
        finally:
            for sig, handler in handlers.items():
                signal.signal(sig, handler)
            os.umask(old_mask)
        try:
            for index in directory.glob("index-*.json"):
                index.unlink()
            directory.rmdir()
        except OSError:
            print("coverage_core_cleanup=VM_LOCAL_REMAINDER_NO_UPLOAD", flush=True)
            if not primary:
                primary = 2
    return primary


if __name__ == "__main__":
    raise SystemExit(main())
