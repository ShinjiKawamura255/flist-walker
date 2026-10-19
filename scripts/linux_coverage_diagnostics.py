#!/usr/bin/env python3
"""Bounded, non-ptrace Linux coverage fault collection on an owned hosted VM."""
from __future__ import annotations

import hashlib
import json
import contextlib
import os
from pathlib import Path
import re
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
CORE_LIMIT = 2 * 1024**3
MAX_NOTES = 16 * 1024**2
MAX_CORES = 8
MAX_OUTPUT = 2 * 1024**2
SNAPSHOT_LIMIT = 4 * 1024**3
COMMAND = ["cargo", "llvm-cov", "--locked", "--workspace", "--lcov", "--output-path",
           "target/llvm-cov/lcov.info", "--fail-under-lines", "75"]


class DiagnosticError(Exception):
    """Only fixed diagnostic codes, never raw exception/argument values."""


def deadline(seconds):
    return time.monotonic() + seconds


def remaining(end, maximum):
    value = end - time.monotonic()
    if value <= 0:
        raise DiagnosticError("analysis_budget_exhausted")
    return min(value, maximum)


def sha256(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def prepare_directory(parent):
    root = Path(tempfile.mkdtemp(prefix="flistwalker-coverage-", dir=parent))
    root.chmod(0o700)
    for name in ("cores", "binaries"):
        (root / name).mkdir(mode=0o700)
    (root / "registry.json").write_text("{}")
    (root / "registry.json").chmod(0o600)
    (root / "analysis-count").write_text("0")
    return root


def registry(root):
    return json.loads((root / "registry.json").read_text())


def register_binary(path, root):
    # Only the Cargo runner writes this immutable identity registry. A child
    # started through current_exe needs no PID registration: its core maps the
    # same executable, whose entry point and immutable bytes are checked below.
    import fcntl
    path = path.resolve(strict=True)
    with (root / "registry.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        identities = registry(root)
        digest = sha256(path)
        if str(path) in identities:
            prior = identities[str(path)]
            if prior["sha256"] != digest or sha256(Path(prior["snapshot"])) != digest:
                raise DiagnosticError("binary_identity_changed")
            return prior
        image = elf(path, {2, 3})
        if (len(identities) >= 32 or path.stat().st_size > 1024**3
                or sum(record["bytes"] for record in identities.values()) + path.stat().st_size > SNAPSHOT_LIMIT):
            raise DiagnosticError("binary_snapshot_limit")
        directory = root / "binaries" / digest
        directory.mkdir(mode=0o700, exist_ok=True)
        snapshot = directory / path.name
        if not snapshot.exists():
            with snapshot.open("xb") as target, path.open("rb") as source:
                shutil.copyfileobj(source, target, 1024**2)
            snapshot.chmod(0o500)
        if sha256(snapshot) != digest or sha256(path) != digest:
            raise DiagnosticError("binary_identity_changed")
        identity = {"original": str(path), "snapshot": str(snapshot), "sha256": digest,
                    "bytes": path.stat().st_size, "entry": image["entry"], "type": image["type"]}
        identities[str(path)] = identity
        temporary = root / "registry.new"
        temporary.write_text(json.dumps(identities))
        temporary.chmod(0o600)
        temporary.replace(root / "registry.json")
        return identity


def read_at(source, offset, size, length, cap):
    if offset < 0 or size < 0 or size > cap or offset + size > length:
        raise DiagnosticError("invalid_elf_bounds")
    source.seek(offset)
    data = source.read(size)
    if len(data) != size:
        raise DiagnosticError("short_elf_read")
    return data


def elf(path, types):
    if path.is_symlink() or not path.is_file():
        raise DiagnosticError("unsafe_elf_file")
    with path.open("rb") as source:
        length = os.fstat(source.fileno()).st_size
        header = struct.unpack("<16sHHIQQQIHHHHHH", read_at(source, 0, 64, length, 64))
        ident, kind, machine, version, entry, phoff, _, _, ehsize, phsize, phcount, *_ = header
        if (ident[:7] != b"\x7fELF\x02\x01\x01" or kind not in types or machine != 62
                or version != 1 or ehsize != 64 or phsize != 56 or not 0 < phcount <= 32768):
            raise DiagnosticError("unsupported_elf")
        raw = read_at(source, phoff, phsize * phcount, length, 56 * 32768)
        segments = [struct.unpack_from("<IIQQQQQQ", raw, n * 56) for n in range(phcount)]
        notes = []
        total = 0
        if kind == 4:
            for typ, _, off, _, _, size, _, _ in segments:
                if typ != 4:
                    continue
                total += size
                if total > MAX_NOTES:
                    raise DiagnosticError("elf_notes_limit")
                data = read_at(source, off, size, length, MAX_NOTES)
                pos = 0
                while pos < len(data):
                    if len(data) - pos < 12:
                        raise DiagnosticError("invalid_note_header")
                    names, descs, note_type = struct.unpack_from("<III", data, pos)
                    start = pos + 12
                    desc = start + (names + 3) // 4 * 4
                    end = desc + (descs + 3) // 4 * 4
                    if end > len(data) or names > 256:
                        raise DiagnosticError("invalid_note_bounds")
                    if data[start:start + names] in (b"CORE\0", b"CORE"):
                        notes.append((note_type, data[desc:desc + descs]))
                    pos = end
        return {"type": kind, "entry": entry, "segments": segments, "notes": notes,
                "truncated": any(s[0] == 1 and s[2] + s[5] > length for s in segments)}


def core_identity(path, identities):
    image = elf(path, {4})
    mappings, auxv = [], {}
    for typ, data in image["notes"]:
        if typ == 0x46494C45:  # NT_FILE: ranges, file offsets (pages), NUL paths.
            if len(data) < 16:
                raise DiagnosticError("invalid_file_note")
            count, page = struct.unpack_from("<QQ", data)
            names_at = 16 + count * 24
            if count > 131072 or page == 0 or page & (page - 1) or names_at > len(data):
                raise DiagnosticError("invalid_file_note")
            names = data[names_at:].split(b"\0")
            if len(names) != count + 1 or names[-1] != b"":
                raise DiagnosticError("invalid_file_paths")
            for index, name in enumerate(names[:-1]):
                start, end, offset = struct.unpack_from("<QQQ", data, 16 + index * 24)
                if start >= end:
                    raise DiagnosticError("invalid_file_mapping")
                mappings.append((start, end, offset * page, os.fsdecode(name)))
        elif typ == 6:  # NT_AUXV: actual executable entry point, not a library.
            if len(data) % 16:
                raise DiagnosticError("invalid_auxv")
            ended = False
            for offset in range(0, len(data), 16):
                key, value = struct.unpack_from("<QQ", data, offset)
                if key == 0:
                    ended = True
                    break
                if key in auxv:
                    raise DiagnosticError("duplicate_auxv")
                auxv[key] = value
            if not ended:
                raise DiagnosticError("unterminated_auxv")
    actual_entry = auxv.get(9)
    if actual_entry is None or not any(s[0] == 1 and s[1] & 1 and s[3] <= actual_entry < s[3] + s[6]
                                       for s in image["segments"]):
        raise DiagnosticError("missing_executable_entry")
    matches = []
    for original, identity in identities.items():
        ranges = [m for m in mappings if m[3] == original]
        bases = [m[0] for m in ranges if m[2] == 0]
        expected = ([b + identity["entry"] for b in bases] if identity["type"] == 3 else [identity["entry"]])
        if actual_entry in expected and any(m[0] <= actual_entry < m[1] for m in ranges):
            if (sha256(Path(identity["snapshot"])) != identity["sha256"]
                    or sha256(Path(original)) != identity["sha256"]):
                raise DiagnosticError("snapshot_identity_changed")
            matches.append(identity)
    if len(matches) != 1:
        raise DiagnosticError("unmapped_or_ambiguous_core")
    return dict(matches[0], truncated=image["truncated"])


def identify_core(path, identities):
    match = re.fullmatch(r"core-([1-9][0-9]*)-([1-9][0-9]*)", path.name)
    if not match or path.is_symlink():
        raise DiagnosticError("unsafe_core_name")
    if path.stat().st_size > CORE_LIMIT:
        raise DiagnosticError("core_size_limit")
    identity = core_identity(path, identities)
    identity["truncated"] = identity["truncated"] or path.stat().st_size >= CORE_LIMIT
    return dict(identity, pid=int(match[1]), signal=int(match[2]), core=str(path))


def structured_output(wire):
    if len(wire.encode()) > MAX_OUTPUT:
        raise DiagnosticError("gdb_output_limit")
    records = []
    for line in wire.splitlines():
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and isinstance(value.get("coverage_record"), str):
            records.append(value["coverage_record"])
    text = "\n".join(records) + "\n"
    if "PRIVATE_ARGUMENT_PROBE" in text:
        raise DiagnosticError("private_output_rejected")
    return text


def extract_stack(identity, end):
    env = dict(os.environ, FLISTWALKER_COVERAGE_CORE=identity["core"],
               FLISTWALKER_COVERAGE_PID=str(identity["pid"]),
               FLISTWALKER_COVERAGE_SIGNAL=str(identity["signal"]))
    # stdout goes to a bounded temporary file: a wedged GDB cannot fill RAM.
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        import resource
        def limit_output():
            resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_OUTPUT, MAX_OUTPUT))
            resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        timed_out = False
        result = None
        try:
            result = subprocess.run(["gdb", "--nx", "--batch", "--init-eval-command=set auto-load off",
                                     "--command", str(ROOT / "scripts/linux_coverage_gdb.py"),
                                     "--args", identity["snapshot"]], env=env, stdout=output, stderr=errors,
                                    timeout=remaining(end, 90), check=False, preexec_fn=limit_output)
        except subprocess.TimeoutExpired:
            timed_out = True
        output.seek(0)
        raw_output = output.read(MAX_OUTPUT + 1)
        text = structured_output(raw_output.decode("ascii", "ignore"))
        errors.seek(0)
        diagnostic_errors = errors.read(MAX_OUTPUT + 1).lower()
    metadata = {"gdb_exit": result.returncode if result is not None else None, "gdb_timeout": timed_out,
                "gdb_stderr_present": bool(diagnostic_errors),
                "gdb_stderr_categories": [label for token, label in
                    ((b"traceback", "python_error"), (b"no such file", "file_not_found"),
                     (b"undefined command", "unsupported_command"), (b"truncat", "truncated"),
                     (b"may not match", "binary_mismatch")) if token in diagnostic_errors]}
    if "coverage_identity_verified=true" not in text or not re.search(r"(?m)^#0\s", text):
        return dict(metadata, status="NOT_RUN", reason="gdb_stack_unavailable", diagnostic=text)
    partial = (timed_out or result is None or result.returncode != 0 or len(raw_output) >= MAX_OUTPUT
               or identity["truncated"] or b"truncat" in diagnostic_errors or b"may not match" in diagnostic_errors
               or "coverage_threads_complete=true" not in text or any(marker in text for marker in
                   ("coverage_unavailable=", "Cannot access memory", "corrupt stack", "frame did not save the PC",
                    "More stack frames follow")))
    return dict(metadata, status="PARTIAL" if partial else "CAPTURED", stack=text,
                expected_abort=identity["signal"] == 6 and "abort_after_filelist_commit_if_requested" in text)


def report_cores(root, end, phase):
    observations = []
    identities = registry(root)
    count = int((root / "analysis-count").read_text())
    files = []
    with os.scandir(root / "cores") as entries:
        for entry in entries:
            files.append(Path(entry.path))
            if len(files) > MAX_CORES:
                break
    for core in sorted(files):
        match = re.fullmatch(r"core-([1-9][0-9]*)-([1-9][0-9]*)", core.name)
        row = {"phase": phase, "status": "NOT_RUN"}
        if match:
            row.update(pid=int(match[1]), signal=int(match[2]))
        try:
            if count >= MAX_CORES:
                raise DiagnosticError("core_count_limit")
            count += 1
            (root / "analysis-count").write_text(str(count))
            remaining(end, 90)
            identity = identify_core(core, identities)
            row.update(bytes=core.stat().st_size, sha256=identity["sha256"])
            row.update(extract_stack(identity, end))
        except Exception as error:
            row["reason"] = str(error) if isinstance(error, DiagnosticError) else "invalid_or_unavailable_core"
            row["failure_type"] = type(error).__name__
        finally:
            # This directory is created private for this command; unlink never
            # follows an unknown/symlink core and raw bytes are never published.
            try:
                core.unlink()
            except OSError:
                row.update(status="NOT_RUN", reason="raw_cleanup_failed")
        observations.append(row)
    return observations


def result_code(primary, observations, errors):
    if primary:
        return primary if primary > 0 else 128 - primary
    if errors or any(row["status"] != "CAPTURED" or not row.get("expected_abort")
                     for row in observations):
        return 2
    return 0


@contextlib.contextmanager
def core_settings(root, errors, state):
    import resource
    previous = subprocess.check_output(["sysctl", "-n", "kernel.core_pattern"], text=True, timeout=10).removesuffix("\n")
    limits = resource.getrlimit(resource.RLIMIT_CORE)
    try:
        state["status"] = "MUTATION_ATTEMPTED"
        resource.setrlimit(resource.RLIMIT_CORE, (CORE_LIMIT, limits[1]))
        pattern = str(root / "cores/core-%P-%s")
        if len(pattern.encode()) >= 128 or not re.fullmatch(r"/[A-Za-z0-9_./%\-]+", pattern):
            raise DiagnosticError("unsupported_core_directory")
        subprocess.run(["sudo", "-n", "sysctl", "-q", "-w", "kernel.core_pattern=" + pattern],
                       check=True, timeout=15, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        yield
    finally:
        # Independent restoration attempts: one failure must not skip the other.
        try:
            subprocess.run(["sudo", "-n", "sysctl", "-q", "-w", "kernel.core_pattern=" + previous],
                           check=True, timeout=15, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except Exception:
            errors.append("core_pattern_restore_failed")
        try:
            resource.setrlimit(resource.RLIMIT_CORE, limits)
        except Exception:
            errors.append("core_limit_restore_failed")
        try:
            actual = subprocess.check_output(["sysctl", "-n", "kernel.core_pattern"], text=True, timeout=10).removesuffix("\n")
            if actual != previous or resource.getrlimit(resource.RLIMIT_CORE) != limits:
                errors.append("core_settings_restore_failed_readback")
            else:
                state["status"] = "RESTORED"
        except Exception:
            errors.append("core_settings_restore_failed_readback")


def execute(command, env, end):
    process = subprocess.Popen(command, env=env, shell=False, start_new_session=True)
    try:
        while True:
            try:
                return process.wait(timeout=remaining(end, 1))
            except subprocess.TimeoutExpired:
                if time.monotonic() >= end:
                    raise
                count, size = 0, 0
                with os.scandir(Path(env["FLISTWALKER_COVERAGE_DIRECTORY"]) / "cores") as entries:
                    for entry in entries:
                        count += 1
                        size += entry.stat(follow_symlinks=False).st_size
                        if count > MAX_CORES or size > 2 * CORE_LIMIT:
                            raise DiagnosticError("raw_core_storage_limit")
    except BaseException as error:
        # Only this invocation's new process group is terminated.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=10)
        if isinstance(error, subprocess.TimeoutExpired):
            return 124
        raise


def native_selftest(root, env, end, observations):
    fixture = root / "flist_walker-diagnostic-fixture"
    subprocess.run(["cc", "-g", "-O0", str(ROOT / "scripts/tests/fixtures/linux_coverage_fault.c"),
                    "-o", str(fixture)], check=True, timeout=remaining(end, 30),
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for mode, expected, signo, frame in (("pass", 0, None, None), ("fail", 7, None, None),
            ("segv", -11, 11, "diagnostic_fault"), ("child-segv", 7, 11, "diagnostic_fault"),
            ("child-abort", 0, 6, "diagnostic_abort")):
        process = subprocess.Popen([sys.executable, str(ROOT / "scripts/linux_coverage_runner.py"),
                                    str(fixture), mode, "PRIVATE_ARGUMENT_PROBE"], env=env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        try:
            code = process.wait(timeout=remaining(end, 30))
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=10)
            raise
        rows = report_cores(root, end, "selftest-" + mode)
        observations.extend(rows)
        valid = code == expected and len(rows) == int(signo is not None)
        if rows:
            row = rows[0]
            valid = (valid and row["status"] == "CAPTURED" and row.get("signal") == signo
                     and frame in row.get("stack", "")
                     and (row.get("pid") != process.pid if mode.startswith("child-") else row.get("pid") == process.pid))
        if not valid:
            raise DiagnosticError("native_selftest_failed_" + mode)
        if rows and mode == "child-abort":
            rows[0]["expected_abort"] = True
    fixture.unlink()


def host_metadata():
    import platform
    import resource
    tools = {}
    for name, command in (("rustc", ["rustc", "-Vv"]), ("llvm_cov", ["cargo", "llvm-cov", "--version"]),
                          ("gdb", ["gdb", "--version"]),
                          ("source_commit", ["git", "-C", str(ROOT), "rev-parse", "HEAD"])):
        with tempfile.TemporaryFile() as output:
            subprocess.run(command, stdout=output, stderr=subprocess.DEVNULL, check=True, timeout=10)
            output.seek(0)
            data = output.read(65537)
            if len(data) > 65536:
                raise DiagnosticError("host_metadata_limit")
            tools[name] = data.decode("utf-8", "replace").strip()
    with Path("/proc/cpuinfo").open() as source:
        cpu = source.read(65536)
    with Path("/proc/meminfo").open() as source:
        memory = {line.split(":", 1)[0]: line.split(":", 1)[1].strip() for line in source.read(65536).splitlines()
                  if line.split(":", 1)[0] in ("MemTotal", "MemAvailable", "SwapTotal", "SwapFree")}
    model = next((line.split(":", 1)[1].strip() for line in cpu.splitlines() if line.startswith("model name")), "unknown")
    return {"kernel": platform.release(), "machine": platform.machine(), "cpu_count": os.cpu_count(),
            "cpu_model": model, "memory": memory,
            "image_os": os.environ.get("ImageOS"), "image_version": os.environ.get("ImageVersion"),
            "tools": tools, "libc_sha256": sha256(Path("/lib/x86_64-linux-gnu/libc.so.6").resolve(strict=True)),
            "limits": {name: list(resource.getrlimit(getattr(resource, name))) for name in
                       ("RLIMIT_CORE", "RLIMIT_STACK", "RLIMIT_AS", "RLIMIT_NOFILE", "RLIMIT_NPROC")}}


def main(arguments=None):
    import platform
    argv = list(sys.argv[1:] if arguments is None else arguments)
    if argv[:1] == ["--"]:
        argv = argv[1:]
    if (argv != COMMAND or platform.system() != "Linux" or platform.machine() != "x86_64"
            or os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted" or not os.environ.get("RUNNER_TEMP")):
        print("Only the exact hosted Linux coverage command is allowed", file=sys.stderr)
        return 2
    parent = Path(os.environ["RUNNER_TEMP"]).resolve(strict=True)
    publication = parent / "flistwalker-linux-coverage"
    publication.mkdir(mode=0o700)  # Preserve any previous report; never overwrite.
    root = prepare_directory(parent)
    observations, phases, errors = [], [], []
    settings = {"status": "NOT_CHANGED"}
    scope = "selftest"
    metadata, executables = {}, []
    primary = 0
    end = deadline(900)
    env = dict(os.environ, FLISTWALKER_COVERAGE_DIRECTORY=str(root),
               CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=sys.executable + " " + str(ROOT / "scripts/linux_coverage_runner.py"))
    handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGINT, signal.SIGTERM)}
    old_mask = os.umask(0o077)
    def interrupted(sig, frame):
        raise SystemExit(128 + sig)
    try:
        for sig in handlers:
            signal.signal(sig, interrupted)
        metadata = host_metadata()
        with core_settings(root, errors, settings):
            native_selftest(root, env, end, observations)
            phases.append({"phase": "native-selftest", "status": "PASS"})
            scope = "coverage"
            primary = execute(argv, env, end)
            phases.append({"phase": "coverage", "exit": primary if primary >= 0 else 128 - primary})
            rows = report_cores(root, end, "coverage")
            observations.extend(rows)
            if primary and not rows:
                phases.append({"phase": "fault-stack", "status": "NOT_RUN", "reason": "no_core_generated"})
    except SystemExit as interrupted_exit:
        primary = int(interrupted_exit.code)
        errors.append("interrupted")
    except Exception as error:
        errors.append(str(error) if isinstance(error, DiagnosticError) else scope + "_" + type(error).__name__)
    finally:
        for sig, handler in handlers.items():
            signal.signal(sig, handler)
        os.umask(old_mask)
        # Include residual/unknown cores even after setup, native selftest or
        # extractor failure. Do not let a single bad core hide another stack.
        try:
            observations.extend(report_cores(root, end, scope + "-residual"))
        except Exception:
            errors.append("residual_collection_failed")
        try:
            executables = [{"name": Path(row["original"]).name, "sha256": row["sha256"], "bytes": row["bytes"]}
                           for row in registry(root).values()]
        except Exception:
            errors.append("binary_registry_unavailable")
        try:
            shutil.rmtree(root)
        except OSError:
            errors.append("private_directory_cleanup_failed")
    actual = [row for row in observations if not row["phase"].startswith("selftest-")]
    code = result_code(primary, actual, errors)
    report = {"schema": 1, "exit": code,
              "coverage_exit": next((phase["exit"] for phase in phases if phase["phase"] == "coverage"), None),
              "phases": phases, "cores": observations, "errors": errors,
              "host": metadata, "executables": executables,
              "raw_core_upload": False, "core_limit_bytes": CORE_LIMIT,
              "core_settings": settings["status"]}
    with (publication / "report.json").open("x") as output:
        json.dump(report, output, indent=2)
    print("linux_coverage_diagnostics=" + json.dumps({"exit": code, "phases": phases,
          "cores": [{k: v for k, v in row.items() if k != "stack"} for row in observations], "errors": errors}), flush=True)
    for row in observations:
        if "stack" in row:
            print("linux_coverage_stack=" + json.dumps({"phase": row["phase"], "pid": row.get("pid"),
                  "signal": row.get("signal"), "status": row["status"]}), flush=True)
            print(row["stack"], end="", flush=True)
    return code


if __name__ == "__main__":
    raise SystemExit(main())
