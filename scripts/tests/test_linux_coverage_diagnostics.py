from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import runpy
import signal
import struct
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("linux_coverage_diagnostics", ROOT / "scripts/linux_coverage_diagnostics.py")
DIAG = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DIAG)


def elf_header(kind, entry, count):
    return struct.pack("<16sHHIQQQIHHHHHH", b"\x7fELF\x02\x01\x01" + b"\0" * 9,
                       kind, 62, 1, entry, 64, 0, 0, 64, 56, count, 0, 0, 0)


def note(kind, data):
    return (struct.pack("<III", 5, len(data), kind) + b"CORE\0\0\0\0" + data
            + b"\0" * (-len(data) % 4))


def binary(path):
    path.write_bytes(elf_header(3, 0x1000, 1)
                     + struct.pack("<IIQQQQQQ", 1, 5, 0, 0, 0, 120, 0x3000, 4096))
    return path.resolve()


def core(path, executable, entry=0x401000, executable_mapping=True):
    mappings = struct.pack("<QQQQQ", 1, 4096, 0x400000, 0x403000, 0) + bytes(executable) + b"\0"
    notes = note(0x46494C45, mappings) + note(6, struct.pack("<QQQQ", 9, entry, 0, 0))
    path.write_bytes(elf_header(4, 0, 2)
                     + struct.pack("<IIQQQQQQ", 4, 0, 176, 0, 0, len(notes), len(notes), 4)
                     + struct.pack("<IIQQQQQQ", 1, 5 if executable_mapping else 4,
                                   0, 0x400000, 0, 0, 0x3000, 4096) + notes)
    return path


class CoreIdentityTests(unittest.TestCase):
    def test_direct_child_core_is_identified_without_pid_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = DIAG.prepare_directory(Path(tmp))
            program = binary(Path(tmp) / "flist_walker-fixture")
            expected = DIAG.register_binary(program, root)
            dumped = core(root / "cores/core-321-11", program)
            actual = DIAG.identify_core(dumped, DIAG.registry(root))
            self.assertEqual(actual["sha256"], expected["sha256"])
            self.assertEqual(actual["snapshot"], expected["snapshot"])
            self.assertEqual(actual["pid"], 321)

    def test_path_in_data_or_wrong_entry_does_not_identify_main_executable(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = DIAG.prepare_directory(Path(tmp))
            program = binary(Path(tmp) / "flist_walker-fixture")
            DIAG.register_binary(program, root)
            for entry, execute in ((0x900000, True), (0x401000, False), (0x401001, True)):
                with self.subTest(entry=entry, executable=execute):
                    with self.assertRaises(DIAG.DiagnosticError):
                        DIAG.identify_core(core(root / "cores/core-321-11", program, entry, execute), DIAG.registry(root))

    def test_binary_revision_cannot_replace_first_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = DIAG.prepare_directory(Path(tmp))
            program = binary(Path(tmp) / "flist_walker-fixture")
            original = DIAG.register_binary(program, root)
            program.write_bytes(program.read_bytes() + b"changed")
            with self.assertRaises(DIAG.DiagnosticError):
                DIAG.register_binary(program, root)
            self.assertEqual(DIAG.registry(root)[str(program.resolve())], original)
            self.assertNotEqual(Path(original["snapshot"]).read_bytes(), program.read_bytes())
            with self.assertRaises(DIAG.DiagnosticError):
                DIAG.identify_core(core(root / "cores/core-321-11", program), DIAG.registry(root))

    def test_malformed_and_oversized_note_are_rejected_without_bulk_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = DIAG.prepare_directory(Path(tmp))
            program = binary(Path(tmp) / "flist_walker-fixture")
            DIAG.register_binary(program, root)
            dumped = core(root / "cores/core-321-11", program)
            content = dumped.read_bytes()
            for invalid in (b"PRIVATE_CORE_CONTENT", content[:180],
                            content[:64] + struct.pack("<IIQQQQQQ", 4, 0, 176, 0, 0, 2**40, 2**40, 4) + content[120:]):
                dumped.write_bytes(invalid)
                with self.assertRaises(DIAG.DiagnosticError):
                    DIAG.identify_core(dumped, DIAG.registry(root))

    def test_invalid_file_note_and_elf_header_fields_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = DIAG.prepare_directory(Path(tmp))
            program = binary(Path(tmp) / "flist_walker-fixture")
            DIAG.register_binary(program, root)
            dumped = core(root / "cores/core-321-11", program)
            original = dumped.read_bytes()
            mutations = [(18, "H", 183), (54, "H", 0), (56, "H", 65535),
                         (196, "Q", 2**63), (204, "Q", 0), (204, "Q", 3)]
            for offset, fmt, value in mutations:
                with self.subTest(offset=offset, value=value):
                    changed = bytearray(original)
                    struct.pack_into("<" + fmt, changed, offset, value)
                    dumped.write_bytes(changed)
                    with self.assertRaises(DIAG.DiagnosticError):
                        DIAG.identify_core(dumped, DIAG.registry(root))

    def test_oversize_sparse_core_is_not_read_or_loaded(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = DIAG.prepare_directory(Path(tmp))
            dumped = root / "cores/core-321-11"
            with dumped.open("wb") as output:
                output.truncate(DIAG.CORE_LIMIT + 1)
            with patch.object(DIAG, "core_identity") as inspect:
                with self.assertRaisesRegex(DIAG.DiagnosticError, "core_size_limit"):
                    DIAG.identify_core(dumped, {})
            inspect.assert_not_called()


class CoreReportingTests(unittest.TestCase):
    def test_unknown_or_failed_core_does_not_hide_later_parent_stack_and_all_raw_is_removed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = DIAG.prepare_directory(Path(tmp))
            program = binary(Path(tmp) / "flist_walker-fixture")
            DIAG.register_binary(program, root)
            (root / "cores/core-1-6").write_bytes(b"PRIVATE_UNKNOWN_CORE")
            core(root / "cores/core-2-11", program)
            core(root / "cores/core-3-11", program)
            def extract(identity, deadline, *args):
                if identity["pid"] == 2:
                    raise DIAG.DiagnosticError("extractor_failed")
                return {"status": "CAPTURED", "stack": "#0 real_fault (...)\n"}
            with patch.object(DIAG, "extract_stack", side_effect=extract), contextlib.redirect_stdout(io.StringIO()) as output:
                observations = DIAG.report_cores(root, DIAG.deadline(30), "coverage")
            self.assertEqual([r["status"] for r in observations], ["NOT_RUN", "NOT_RUN", "CAPTURED"])
            self.assertEqual(observations[-1]["pid"], 3)
            self.assertEqual(list((root / "cores").iterdir()), [])
            self.assertNotIn("PRIVATE_UNKNOWN_CORE", output.getvalue())
            self.assertEqual(DIAG.result_code(101, observations, []), 101)

    def test_child_sigsegv_cannot_hide_behind_parent_success(self):
        self.assertEqual(DIAG.result_code(0, [{"status": "CAPTURED", "signal": 11}], []), 2)
        self.assertEqual(DIAG.result_code(0, [{"status": "CAPTURED", "signal": 6, "expected_abort": True}], []), 0)
        self.assertEqual(DIAG.result_code(0, [{"status": "PARTIAL", "signal": 6, "expected_abort": True}], []), 2)
        self.assertEqual(DIAG.result_code(101, [], ["restore_failed"]), 101)
        self.assertEqual(DIAG.result_code(0, [], ["restore_failed"]), 2)

    def test_unstructured_gdb_header_never_reaches_output(self):
        wire = 'Core was generated by PRIVATE_ARGUMENT_PROBE\n' + json.dumps({"coverage_record": "#0 real_fault (...)"}) + "\n"
        self.assertEqual(DIAG.structured_output(wire), "#0 real_fault (...)\n")
        with self.assertRaises(DIAG.DiagnosticError):
            DIAG.structured_output(json.dumps({"coverage_record": "PRIVATE_ARGUMENT_PROBE"}))

    def test_extractor_timeout_preserves_already_captured_fault_stack(self):
        for interrupted in (False, True):
            with self.subTest(interrupted=interrupted):
                now = [0]
                cancellation = DIAG.Cancellation()
                class Process:
                    killed = False
                    def wait(self, timeout):
                        if self.killed:
                            return -9
                        if interrupted:
                            cancellation.receive(signal.SIGTERM, None)
                        else:
                            now[0] = 100
                        raise subprocess.TimeoutExpired("gdb", timeout)
                    def kill(self):
                        self.killed = True
                    def poll(self):
                        return -9 if self.killed else None
                process = Process()
                def launch(command, **kwargs):
                    for line in ("coverage_identity_verified=true", "#0 actual_fault (...)"):
                        kwargs["stdout"].write((json.dumps({"coverage_record": line}) + "\n").encode())
                    return process
                with patch.object(DIAG.subprocess, "Popen", side_effect=launch), \
                     patch.object(DIAG.time, "monotonic", side_effect=lambda: now[0]):
                    result = DIAG.extract_stack({"core": "/owned/core-123-11", "pid": 123, "signal": 11,
                                                "snapshot": "/owned/binary", "truncated": False}, 1000, cancellation)
                self.assertTrue(process.killed)
                self.assertEqual(result["gdb_interrupted"], interrupted)
                self.assertEqual(result["gdb_timeout"], not interrupted)
                self.assertEqual(result["status"], "PARTIAL")
                self.assertIn("actual_fault", result["stack"])

    def test_symlink_core_is_unlinked_without_reading_its_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = DIAG.prepare_directory(Path(tmp))
            outside = Path(tmp) / "outside"
            outside.write_bytes(b"PRIVATE_UNRELATED_FILE")
            (root / "cores/core-1-11").symlink_to(outside)
            rows = DIAG.report_cores(root, DIAG.deadline(30), "coverage")
            self.assertEqual(rows[0]["status"], "NOT_RUN")
            self.assertEqual(outside.read_bytes(), b"PRIVATE_UNRELATED_FILE")
            self.assertEqual(list((root / "cores").iterdir()), [])


class ControllerTests(unittest.TestCase):
    def exercise(self, code, restore_error=False, extractor_error=False, selftest_error=False, interrupted=False,
                 analysis_signal=False, restore_signal=False):
        import resource
        with tempfile.TemporaryDirectory() as tmp:
            settings = {"pattern": "|original command with trailing spaces  ", "limits": (0, -1)}
            initial = dict(settings)
            calls = []
            def set_limit(kind, limits):
                if restore_error and limits == initial["limits"]:
                    raise OSError("synthetic restore error")
                settings["limits"] = limits
            def run(command, **kwargs):
                calls.append((command, kwargs))
                self.assertEqual(kwargs["timeout"], 15)
                settings["pattern"] = command[-1].split("=", 1)[1]
                if restore_signal and settings["pattern"] == initial["pattern"]:
                    os.kill(os.getpid(), signal.SIGTERM)
                return subprocess.CompletedProcess(command, 0)
            def selftest(*args):
                if selftest_error:
                    raise DIAG.DiagnosticError("native_selftest_failed")
            def execute(command, env, end, *args):
                self.assertEqual(command, DIAG.COMMAND)
                self.assertIn("linux_coverage_runner.py", env["CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER"])
                if interrupted:
                    os.kill(os.getpid(), signal.SIGTERM)
                    return 143
                if analysis_signal:
                    root = Path(env["FLISTWALKER_COVERAGE_DIRECTORY"])
                    program = binary(root / "fixture")
                    DIAG.register_binary(program, root)
                    core(root / "cores/core-1-11", program)
                    core(root / "cores/core-2-11", program)
                return code
            actual_report = DIAG.report_cores
            def report(root, end, phase, *args, **kwargs):
                if extractor_error and phase == "coverage":
                    raise RuntimeError("synthetic extractor error")
                if analysis_signal:
                    return actual_report(root, end, phase, *args, **kwargs)
                return []
            def extract(identity, end, *args):
                if identity["pid"] == 2:
                    journal = json.loads((Path(tmp) / "flistwalker-linux-coverage/report.json").read_text())
                    self.assertIn("actual_fault", journal["cores"][0]["stack"])
                    os.kill(os.getpid(), signal.SIGTERM)
                    return {"status": "PARTIAL", "stack": "#0 second_fault (...)\n"}
                return {"status": "CAPTURED", "stack": "#0 actual_fault (...)\n"}
            with patch.dict(DIAG.os.environ, GITHUB_ACTIONS="true", RUNNER_ENVIRONMENT="github-hosted", RUNNER_TEMP=tmp), \
                 patch("platform.system", return_value="Linux"), patch("platform.machine", return_value="x86_64"), \
                 patch.object(resource, "getrlimit", side_effect=lambda kind: settings["limits"]), \
                 patch.object(resource, "setrlimit", side_effect=set_limit), \
                 patch.object(DIAG.subprocess, "check_output", side_effect=lambda *a, **k: settings["pattern"] + "\n"), \
                 patch.object(DIAG.subprocess, "run", side_effect=run), \
                 patch.object(DIAG, "host_metadata", return_value={"fixture": True}), \
                 patch.object(DIAG, "native_selftest", side_effect=selftest), \
                 patch.object(DIAG, "execute", side_effect=execute) as launch, \
                 patch.object(DIAG, "report_cores", side_effect=report), \
                 patch.object(DIAG, "extract_stack", side_effect=extract), \
                 contextlib.redirect_stdout(io.StringIO()):
                result = DIAG.main(DIAG.COMMAND)
                saved = json.loads((Path(tmp) / "flistwalker-linux-coverage/report.json").read_text())
            self.assertEqual(settings["pattern"], initial["pattern"])
            if not restore_error:
                self.assertEqual(settings, initial)
                self.assertEqual(saved["core_settings"], "RESTORED")
            self.assertEqual(saved["exit"], result)
            self.assertFalse(list(Path(tmp).glob("flistwalker-coverage-*")))
            self.assertEqual(launch.call_count, int(not selftest_error))
            self.assertTrue(any(command[-1] == "kernel.core_pattern=" + initial["pattern"] for command, _ in calls))
            return result, saved

    def test_original_exit_survives_extraction_and_restoration_failures(self):
        for options in ({}, {"restore_error": True}, {"extractor_error": True}):
            with self.subTest(options=options):
                code, saved = self.exercise(101, **options)
                self.assertEqual(code, 101)
                self.assertEqual(saved["coverage_exit"], 101)

    def test_restore_failure_is_not_success_and_interruption_restores(self):
        self.assertEqual(self.exercise(0)[0], 0)
        self.assertEqual(self.exercise(0, restore_error=True)[0], 2)
        self.assertEqual(self.exercise(0, interrupted=True)[0], 143)

    def test_failed_native_pretest_blocks_real_coverage(self):
        code, saved = self.exercise(0, selftest_error=True)
        self.assertEqual(code, 2)
        self.assertIsNone(saved["coverage_exit"])

    def test_signal_during_second_analysis_preserves_first_stack_and_original_exit(self):
        code, saved = self.exercise(101, analysis_signal=True)
        self.assertEqual(code, 101)
        self.assertEqual(saved["coverage_exit"], 101)
        self.assertIn("actual_fault", saved["cores"][0]["stack"])
        self.assertIn("second_fault", saved["cores"][1]["stack"])
        self.assertIn("interrupted", saved["errors"])

    def test_signal_during_restore_finishes_both_restorations_and_readback(self):
        code, saved = self.exercise(101, restore_signal=True)
        self.assertEqual(code, 101)
        self.assertEqual(saved["core_settings"], "RESTORED")
        self.assertIn("interrupted", saved["errors"])

    def test_non_linux_or_changed_command_cannot_mutate_host(self):
        with patch.object(DIAG.subprocess, "run") as run, contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(DIAG.main(["cargo", "test"]), 2)
        run.assert_not_called()


class GdbProtocolTests(unittest.TestCase):
    def exercise(self, pid=123, private=False, count=2):
        commands = []
        class Thread:
            def __init__(self, num):
                self.num = num
                self.ptid = (pid, pid + num - 1, 0)
            def switch(self):
                pass
        threads = tuple(Thread(i + 1) for i in range(count))
        def execute(command, **kwargs):
            commands.append(command)
            self.assertTrue(kwargs.get("to_string"))
            if command.startswith("core-file "):
                print("Core was generated by PRIVATE_ARGUMENT_PROBE")
                return "PRIVATE_ARGUMENT_PROBE"
            if command == "bt 48":
                return "PRIVATE_ARGUMENT_PROBE" if private else "#0 diagnostic_fault (...)\n"
            return "bounded metadata\n"
        gdb = types.SimpleNamespace(execute=execute, error=RuntimeError,
                                    selected_inferior=lambda: types.SimpleNamespace(pid=pid, threads=lambda: threads),
                                    selected_thread=lambda: threads[0], parse_and_eval=lambda expression: 11)
        with patch.dict(sys.modules, gdb=gdb), \
             patch.dict(os.environ, FLISTWALKER_COVERAGE_CORE="/owned/core-123-11",
                        FLISTWALKER_COVERAGE_PID="123", FLISTWALKER_COVERAGE_SIGNAL="11"), \
             contextlib.redirect_stdout(io.StringIO()) as output:
            try:
                runpy.run_path(str(ROOT / "scripts/linux_coverage_gdb.py"))
            except SystemExit:
                pass
        return commands, DIAG.structured_output(output.getvalue())

    def test_fault_is_first_and_all_threads_have_no_values_or_header(self):
        commands, output = self.exercise()
        self.assertIn("set print frame-arguments none", commands)
        self.assertIn("set auto-load off", commands)
        self.assertIn("core-file /owned/core-123-11", commands)
        self.assertEqual(commands.count("bt 48"), 2)
        self.assertLess(output.index("#0 diagnostic_fault"), output.index("coverage_thread=2"))
        self.assertIn("coverage_threads_complete=true", output)
        self.assertNotIn("PRIVATE_ARGUMENT_PROBE", output)
        self.assertFalse(any(c in ("info args", "info locals", "bt full", "show environment") for c in commands))

    def test_pid_mismatch_and_private_output_fail_closed(self):
        commands, output = self.exercise(pid=124)
        self.assertNotIn("bt 48", commands)
        self.assertIn("coverage_unavailable=core_pid_or_signal_mismatch", output)
        _, output = self.exercise(private=True)
        self.assertNotIn("PRIVATE_ARGUMENT_PROBE", output)
        self.assertNotIn("coverage_threads_complete=true", output)

    def test_thread_limit_is_explicit_and_bounded(self):
        commands, output = self.exercise(count=257)
        self.assertEqual(commands.count("bt 48"), 256)
        self.assertIn("coverage_unavailable=thread_count_limit", output)
        self.assertNotIn("coverage_threads_complete=true", output)


if __name__ == "__main__":
    unittest.main()
