#!/usr/bin/env python3
"""One-shot local experiment; no CI activation, acceptance change or retries.

Run from the repository root with --binary <exact libtest executable> --output
<new directory>. Controls deliberately FAIL the original 100ms assertion.
SIGSTOP targets only our disposable child; finally always resumes/kills/reaps.
"""
import argparse
import concurrent.futures
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import time

PREFIX = "app::tests::tab_background_responses::"
CASES = [
    PREFIX + "tc_207_background_finished_commits_100k_default_snapshot_within_ui_budget",
    PREFIX + "tc_207_background_finished_mixed_owners_settles_incrementally",
    PREFIX + "tc_207_background_finished_file_filter_settles_incrementally",
    PREFIX + "tc_207_background_finished_walker_kind_queue_settles_incrementally",
]
LIMIT_NS = 100_000_000


def install_termination_handler():
    def terminate(signum, _frame):
        # A second TERM must not interrupt the finally block reaping our child.
        signal.signal(signum, signal.SIG_IGN)
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, terminate)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(binary, output, label, control=None):
    env = os.environ.copy()
    env.pop("FLISTWALKER_TEST_TC207_CONTROL", None)
    env["FLISTWALKER_TEST_TC207_TRACE"] = "1"
    if control:
        env["FLISTWALKER_TEST_TC207_CONTROL"] = control
    cases = CASES[-1:] if control else CASES
    argv = [str(binary), "--exact", *cases, "--nocapture", "--test-threads=1"]
    log = output / (label + ".log")
    stopped = None
    started = time.monotonic_ns()
    with log.open("wb") as stream:
        child = subprocess.Popen(argv, env=env, stdout=stream, stderr=subprocess.STDOUT)
        try:
            if control == "stop":
                deadline = time.monotonic() + 15
                while True:
                    pid, status = os.waitpid(child.pid, os.WNOHANG | os.WUNTRACED)
                    if pid:
                        if not os.WIFSTOPPED(status):
                            child.returncode = os.waitstatus_to_exitcode(status)
                            raise RuntimeError("child exited before observed SIGSTOP")
                        if os.WSTOPSIG(status) != signal.SIGSTOP:
                            raise RuntimeError("unexpected child stop signal")
                        begin = time.monotonic_ns()
                        # This sleep is the specified experiment stimulus, not
                        # a guess that an asynchronous product state is ready.
                        time.sleep(0.150)
                        end = time.monotonic_ns()
                        os.kill(child.pid, signal.SIGCONT)
                        stopped = {"waitpid_stop_signal": os.WSTOPSIG(status),
                                   "observed_stopped_lower_ns": end - begin}
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("no observed SIGSTOP within 15 seconds")
                    time.sleep(0.001)
            code = child.wait(timeout=30)
        finally:
            if child.poll() is None:
                # SIGKILL terminates even a stopped process; send SIGCONT first
                # for the normal cleanup path, and always wait/reap it ourselves.
                try:
                    os.kill(child.pid, signal.SIGCONT)
                except ProcessLookupError:
                    pass
                child.kill()
                child.wait(timeout=5)
    text = log.read_text()
    frames = [json.loads(m.group(1)) for m in re.finditer(r"TC207_FRAME (\{[^\n]*\})", text)]
    if not frames:
        raise RuntimeError(f"{label}: missing raw observations; see {log}")
    if {f["test"] for f in frames} != set(cases):
        raise RuntimeError(f"{label}: wrong test identity/cardinality")
    for test in cases:
        rows = [f for f in frames if f["test"] == test]
        if [f["frame"] for f in rows] != list(range(len(rows))):
            raise RuntimeError(f"{label}: frame sequence missing or duplicated")
        if any(f["frames_observed"] != len(rows) for f in rows):
            raise RuntimeError(f"{label}: incomplete frame dump")
    failures = [f for f in frames if not f["wall_under_100ms"]]
    receipt = {"label": label, "argv": argv, "control": control, "exit_code": code,
               "elapsed_ns": time.monotonic_ns() - started, "pid": child.pid,
               "observed_stop": stopped, "frame_count": len(frames),
               "failing_frames": len(failures), "log_sha256": sha(log),
               "max_wall": max(frames, key=lambda f: f["wall_ns"]),
               "max_cpu": max(frames, key=lambda f: f["cpu_upper_ns"] or 0),
               "max_off_cpu": max(frames, key=lambda f: f["off_cpu_lower_ns"] or 0)}
    (output / (label + ".receipt.json")).write_text(json.dumps(receipt, indent=2) + "\n")
    # Preserve every line, including failure/panic text, without a huge text diff.
    with gzip.open(output / (label + ".log.gz"), "wb") as compressed:
        compressed.write(log.read_bytes())
    log.unlink()
    expected = 101 if control else 0
    valid = code == expected
    if control:
        valid &= len(failures) == 1 and "background finalization frame exceeded 100ms" in text
        row = failures[0] if failures else {}
        if control == "cpu":
            valid &= (row.get("cpu_lower_ns") or 0) >= 100_000_000
        else:
            valid &= (row.get("off_cpu_lower_ns") or 0) >= 100_000_000
        if control == "stop":
            valid &= stopped is not None and stopped["observed_stopped_lower_ns"] >= 150_000_000
    else:
        valid &= not failures and "test result: ok. 4 passed; 0 failed" in text
    receipt["experiment_valid"] = bool(valid)
    (output / (label + ".receipt.json")).write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"label": label, "exit": code, "frames": len(frames),
                      "experiment_valid": bool(valid)}, separators=(",", ":")), flush=True)
    return receipt


def main():
    install_termination_handler()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    provenance = {"base": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                  "platform": platform.platform(), "cpu_count": os.cpu_count(),
                  "binary_sha256": sha(binary), "binary": str(binary),
                  "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
                  "profile": "test debug, cargo test --locked --offline --lib",
                  "wall_limit_ns": LIMIT_NS, "retries": 0,
                  "source_sha256": {str(p): sha(p) for p in [
                      Path("rust/src/app/tests/finalization_timing.rs"),
                      Path("rust/src/app/tests/tab_background_responses.rs"),
                      Path("rust/src/app/tests/mod.rs"), Path(__file__)]},
                  "plan": {"controls": ["cpu", "sleep", "stop"], "serial_rounds": 3,
                           "parallel_processes": [4, 12], "cases_per_process": 4,
                           "test_threads_per_process": 1, "child_timeout_seconds": 30}}
    (args.output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    receipts = []
    for control in ["cpu", "sleep", "stop"]:
        receipts.append(run(binary, args.output, "control-" + control, control))
    for n in range(3):
        receipts.append(run(binary, args.output, f"serial-{n}"))
    for processes in [4, 12]:
        with concurrent.futures.ThreadPoolExecutor(max_workers=processes) as pool:
            # Submit exactly this predeclared cohort. No replacement on failure.
            futures = [pool.submit(run, binary, args.output, f"parallel-{processes}-{n}")
                       for n in range(processes)]
            receipts.extend(f.result() for f in futures)
    assert sha(binary) == provenance["binary_sha256"], "binary changed during collection"
    summary = {"receipts": receipts, "all_experiments_valid": all(r["experiment_valid"] for r in receipts)}
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    raise SystemExit(0 if summary["all_experiments_valid"] else 1)


if __name__ == "__main__":
    main()
