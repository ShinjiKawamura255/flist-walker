from __future__ import annotations

import contextlib
import importlib.util
import io
from pathlib import Path
import runpy
import platform
import shutil
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]


class CoverageDiagnosticRunnerTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location(
            "coverage_diagnostic_runner", ROOT / "scripts/coverage-diagnostic-runner.py"
        )
        self.runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.runner)

    def exercise(self, code, name="flist_walker-test", linux=True):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / name
            binary.write_bytes(b"synthetic test executable")
            argv = [str(binary), "--exact", "test with spaces"]
            with patch.object(self.runner.platform, "system", return_value="Linux" if linux else "Darwin"), \
                 patch.object(self.runner.shutil, "which", side_effect=lambda name: "/tools/" + name), \
                 patch.object(self.runner.subprocess, "run", return_value=subprocess.CompletedProcess([], code)) as run, \
                 contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                result = self.runner.main(argv)
            return result, run.call_args, argv

    def test_gdb_preserves_argv_and_child_success_failure(self):
        for code in (0, 1, 139, 124):
            with self.subTest(code=code):
                result, call, argv = self.exercise(code)
                self.assertEqual(result, code)
                command = call.args[0]
                self.assertEqual(command[:6], ["/tools/timeout", "--signal=TERM", "--kill-after=10s", "600s", "/tools/gdb", "--nx"])
                self.assertIn("--batch", command)
                self.assertIn("--return-child-result", command)
                self.assertEqual(command[command.index("--args") + 1:], argv)
                self.assertFalse(call.kwargs["shell"])

    def test_other_cargo_executables_are_not_wrapped(self):
        result, call, argv = self.exercise(7, "fw-test")
        self.assertEqual(result, 7)
        self.assertEqual(call.args[0], argv)

    def test_non_linux_does_not_launch_a_child(self):
        result, call, _ = self.exercise(0, linux=False)
        self.assertEqual(result, 2)
        self.assertIsNone(call)


class GdbDiagnosticCommandsTests(unittest.TestCase):
    def exercise(self, signal=None):
        commands = []
        callback = []

        class SignalEvent:
            def __init__(self, name):
                self.stop_signal = name

        class Quit(Exception):
            pass

        def execute(command):
            commands.append(command)
            if command == "run" and signal:
                callback[0](SignalEvent(signal))
            if command.startswith("quit "):
                raise Quit(command)

        gdb = types.SimpleNamespace(
            execute=execute, SignalEvent=SignalEvent, error=RuntimeError,
            events=types.SimpleNamespace(stop=types.SimpleNamespace(connect=callback.append)),
        )
        with patch.dict(sys.modules, gdb=gdb), contextlib.redirect_stdout(io.StringIO()):
            try:
                runpy.run_path(str(ROOT / "scripts/coverage-diagnostic-gdb.py"))
            except Quit:
                pass
        return commands

    def test_fault_collects_bounded_stacks_without_values_and_fails(self):
        commands = self.exercise("SIGSEGV")
        self.assertIn("set print frame-arguments none", commands)
        self.assertIn("set auto-load off", commands)
        self.assertIn("set debuginfod enabled off", commands)
        self.assertIn("set disable-randomization off", commands)
        self.assertIn("set startup-with-shell off", commands)
        self.assertIn("thread apply all bt 48", commands)
        self.assertIn("info registers", commands)
        self.assertEqual(commands[-1], "quit 139")
        self.assertFalse(any("bt full" in command or "generate-core" in command or "environ" in command for command in commands))

    def test_normal_exit_is_left_to_gdb_return_child_result(self):
        commands = self.exercise()
        self.assertEqual(commands[-1], "run")
        self.assertFalse(any(command.startswith("quit ") for command in commands))


class NativeGdbSmokeTests(unittest.TestCase):
    @unittest.skipUnless(platform.system() == "Linux" and shutil.which("gdb") and shutil.which("cc"),
                         "native Linux GDB and C compiler required")
    def test_actual_gdb_status_and_fault_stack_without_argument_values(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "fixture.c"
            source.write_text('''#include <string.h>
__attribute__((noinline)) void diagnostic_crash(const char *private_value) {
    volatile int *address = 0;
    *address = 1;
}
int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "fault") == 0) diagnostic_crash(argv[2]);
    return argc > 1 && strcmp(argv[1], "fail") == 0 ? 7 : 0;
}
''')
            binary = root / "flist_walker-smoke"
            subprocess.run(["cc", "-g", "-O0", str(source), "-o", str(binary)], check=True,
                           capture_output=True, timeout=30)
            for mode, code in (("pass", 0), ("fail", 7), ("fault", 139)):
                with self.subTest(mode=mode):
                    result = subprocess.run(
                        [sys.executable, str(ROOT / "scripts/coverage-diagnostic-runner.py"),
                         str(binary), mode, "PRIVATE_ARGUMENT_PROBE"],
                        capture_output=True, text=True, timeout=45,
                    )
                    self.assertEqual(result.returncode, code, result.stdout + result.stderr)
                    if mode == "fault":
                        self.assertIn('"signal": "SIGSEGV"', result.stdout)
                        self.assertIn("diagnostic_crash", result.stdout)
                        self.assertNotIn("PRIVATE_ARGUMENT_PROBE", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
