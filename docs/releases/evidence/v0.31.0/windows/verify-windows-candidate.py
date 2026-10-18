#!/usr/bin/env python3
"""Owned Windows candidate alias checks and TC-193; never updates an installation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import tempfile
import time


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--binding", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "nt" or platform.machine().lower() not in ("amd64", "x86_64"):
        parser.error("TC-193 requires native x86_64 Windows, not a cross-platform timing substitute")
    binding = json.loads(args.binding.read_text(encoding="utf-8"))
    if binding.get("version") != "0.31.0" or not re.fullmatch(r"[0-9a-f]{40}", binding.get("source_sha", "")):
        parser.error("An exact v0.31.0 candidate binding from the release operator is required")
    if (binding.get("source_sha"), binding.get("run_id"), binding.get("attempt")) != (
        "3194f03c27f9a72f06a836dbd02e9141d8740a26", 39087986481, 1
    ):
        parser.error("Only the exact reviewed candidate source/run/attempt is allowed")
    report = {
        "version": "0.31.0", "source_sha": binding["source_sha"],
        "run_id": binding["run_id"], "attempt": binding["attempt"],
        "artifact": binding["artifact"], "result": "FAIL", "alias_checks": [],
        "environment": {"system": platform.system(), "release": platform.release(),
                        "version": platform.version(), "machine": platform.machine(),
                        "python": platform.python_version(), "cpu_count": os.cpu_count()},
    }
    if args.output.exists():
        parser.error("Refusing to overwrite an existing result; preserve the first run")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    raw_path = args.output.with_suffix(".raw.jsonl")
    if raw_path.exists():
        parser.error("Refusing to overwrite existing raw observations")
    raw = raw_path.open("x", encoding="utf-8", buffering=1)
    report["raw_file"] = raw_path.name
    started = time.monotonic()
    try:
        if sha256(args.bundle / "SHA256SUMS") != binding["manifest_sha256"]:
            raise ValueError("Candidate manifest digest mismatch")
        if sha256(args.bundle / "SHA256SUMS.sig") != binding["signature_sha256"]:
            raise ValueError("Candidate signature-file digest mismatch (crypto verification is separate)")
        manifest = {}
        for row in (args.bundle / "SHA256SUMS").read_text(encoding="ascii").splitlines():
            digest, name = row.split("  ", 1)
            if name in manifest or len(digest) != 64 or any(c not in "0123456789abcdefABCDEF" for c in digest):
                raise ValueError("Invalid/duplicate manifest entry")
            manifest[name] = digest.lower()
        if len(manifest) != 26:
            raise ValueError("Exact 26-entry manifest required")
        binaries = {}
        report["binaries"] = {}
        for role in ("universal", "fw"):
            identity = binding["windows"][role]
            expected_name = "FlistWalker-0.31.0-windows-x86_64.exe" if role == "universal" else "fw-0.31.0-windows-x86_64.exe"
            if identity["filename"] != expected_name:
                raise ValueError("Candidate role must use its exact release basename")
            path = args.bundle / identity["filename"]
            digest = sha256(path)
            if digest != identity["sha256"] or digest != manifest.get(path.name):
                raise ValueError(f"Candidate {role} binary digest mismatch")
            binaries[role] = path.resolve()
            report["binaries"][role] = {"filename": path.name, "sha256": digest}
        with tempfile.TemporaryDirectory(prefix="fw-v0310-windows-") as temporary:
            root = Path(temporary)
            profile = root / "profile"
            fixture = root / "shallow200"
            profile.mkdir()
            fixture.mkdir()
            for index in range(200):
                (fixture / f"file_{index:03d}.txt").write_bytes(b"owned TC-193 fixture\n")
            env = {key: value for key, value in os.environ.items()
                   if not key.startswith("FLISTWALKER_")}
            for key in ("HOME", "USERPROFILE", "LOCALAPPDATA", "APPDATA"):
                env[key] = str(profile)
            env["FLISTWALKER_DISABLE_SELF_UPDATE"] = "1"
            env.pop("RUST_LOG", None)

            report["fixture"] = [{"name": path.name, "size": path.stat().st_size,
                                  "sha256": sha256(path)} for path in sorted(fixture.iterdir())]

            def run(role, arguments):
                if time.monotonic() - started > 600:
                    raise TimeoutError("Verification exceeded its bounded ten-minute deadline")
                before = time.perf_counter_ns()
                completed = subprocess.run(
                    [str(binaries[role]), *arguments], env=env, cwd=str(root),
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
                    shell=False, creationflags=subprocess.CREATE_NO_WINDOW,
                )
                elapsed = (time.perf_counter_ns() - before) / 1_000_000
                # Capture after the timer ends; preserve every warmup and partial failure.
                raw.write(json.dumps({"role": role,
                    "argv": [part.replace(str(root), "<owned-test-root>") for part in arguments],
                    "elapsed_ms": elapsed, "exit_code": completed.returncode,
                    "stdout": completed.stdout.decode("utf-8", errors="replace"),
                    "stderr": completed.stderr.decode("utf-8", errors="replace"),
                    "stdout_sha256": hashlib.sha256(completed.stdout).hexdigest()}) + "\n")
                return completed, elapsed

            for role in binaries:
                output, _ = run(role, ["--version"])
                if output.returncode != 0 or not output.stdout.decode("utf-8").strip().endswith(" 0.31.0"):
                    raise ValueError(f"{role} candidate version probe failed")
                report["binaries"][role]["version_output"] = output.stdout.decode("utf-8").strip()
                for update in ("--check-update", "--update"):
                    for color in (["--color"], ["--color", "auto"], ["--color", "always"],
                                  ["--color", "never"], ["--color=auto"]):
                        for update_first in (False, True):
                            arguments = ([update] + color) if update_first else (color + [update])
                            output, _ = run(role, arguments)
                            expected = (0, b"Update checks are disabled.", b"") if update == "--check-update" else (1, b"", b"Automatic updates are disabled.")
                            actual = (output.returncode, output.stdout.strip(), output.stderr.strip())
                            report["alias_checks"].append({"role": role, "argv": arguments,
                                                          "exit_code": output.returncode, "pass": actual == expected})
                            if actual != expected:
                                raise ValueError(f"{role} disabled-update alias contract failed: {arguments}")

            common = ["--root", str(fixture), "--source", "walker", "--type", "file",
                      "--no-ignore", "--color", "never", "--limit", "200"]
            samples = {"universal": [], "fw": []}
            baseline = None
            for index in range(30):
                order = ("universal", "fw") if index % 2 == 0 else ("fw", "universal")
                for role in order:
                    argv = (["--cli"] if role == "universal" else []) + common
                    output, milliseconds = run(role, argv)
                    lines = sorted(output.stdout.decode("utf-8").splitlines())
                    expected_names = [f"file_{number:03d}.txt" for number in range(200)]
                    if output.returncode != 0 or lines != expected_names or output.stderr:
                        raise ValueError(f"{role} TC-193 output/exit contract failed")
                    if baseline is None:
                        baseline = output.stdout
                    elif output.stdout != baseline:
                        raise ValueError("Direct-process variants produced different output order/framing")
                    if index >= 5:
                        samples[role].append(milliseconds)
            medians = {role: statistics.median(values) for role, values in samples.items()}
            ratio = medians["fw"] / medians["universal"]
            report["tc193"] = {
                "warmup_per_variant": 5, "samples_per_variant": 25, "fixture_files": 200,
                "raw_ms": samples, "median_ms": medians, "ratio_fw_over_universal": ratio,
                "required_max_ratio": 0.70, "pass": ratio <= 0.70,
                "stdout_paths": 200, "stderr_empty": True,
                "argv_common": [part.replace(str(root), "<owned-test-root>") for part in common],
            }
            if ratio > 0.70:
                raise ValueError("TC-193 ratio exceeds unchanged 0.70 threshold; retain result, do not retry to obtain PASS")
        report["result"] = "PASS"
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
    raw.close()
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"result": report["result"], "source_sha": report["source_sha"],
                      "tc193": report.get("tc193"), "error": report.get("error")}, ensure_ascii=False))
    return 0 if report["result"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
