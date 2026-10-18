from __future__ import annotations

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("coverage_core_diagnosis", ROOT / "scripts/coverage-core-diagnosis.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CoreReportTests(unittest.TestCase):
    def test_mapped_core_is_reported_without_header_and_removed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "flist_walker-fixture"
            binary.write_bytes(b"test binary")
            core = root / "core-123"
            core.write_bytes(b"synthetic core")
            index = root / "index-123.json"
            index.write_text(json.dumps({"binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}))
            safe = 'coverage_diagnostic_stop={"signal_number": 11}\n#0 diagnostic_crash (...)\n'
            with patch.object(MODULE.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, safe, "")) as run, \
                 contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(MODULE.report_cores(root), 1)
            self.assertIn(safe, output.getvalue())
            self.assertEqual(run.call_args.kwargs["env"]["COVERAGE_DIAGNOSTIC_CORE"], str(core))
            self.assertIn("--init-eval-command=set auto-load off", run.call_args.args[0])
            self.assertFalse(core.exists())
            self.assertFalse(index.exists())

    def test_unmapped_core_is_never_loaded_or_published(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            core = root / "core-123"
            core.write_bytes(b"PRIVATE_CORE_CONTENT")
            with patch.object(MODULE.subprocess, "run") as run, contextlib.redirect_stdout(io.StringIO()) as output:
                with self.assertRaises(RuntimeError):
                    MODULE.report_cores(root)
            run.assert_not_called()
            self.assertNotIn("PRIVATE_CORE_CONTENT", output.getvalue())
            self.assertTrue(core.exists())


@unittest.skipUnless(MODULE.resource, "POSIX resource limits required")
class CoreCampaignTests(unittest.TestCase):
    def exercise(self, codes, report_error=False, restore_error=False, interrupt=False):
        with tempfile.TemporaryDirectory() as temporary:
            calls = []
            results = iter(codes)
            def run(command, **kwargs):
                calls.append((command, kwargs))
                if command[:1] == ["sudo"] and command[-1] == "kernel.core_pattern=original" and restore_error:
                    raise subprocess.CalledProcessError(1, command)
                if command[:1] == ["timeout"]:
                    if interrupt:
                        raise SystemExit(143)
                    return subprocess.CompletedProcess(command, next(results))
                if command[:1] == [MODULE.sys.executable]:
                    return subprocess.CompletedProcess(command, -11)
                return subprocess.CompletedProcess(command, 0)
            def report(directory, smoke=False):
                if smoke:
                    # The mocked compiler leaves no actual fixture.
                    (directory / "flist_walker-core-smoke").write_bytes(b"fixture")
                    return 1
                if report_error:
                    raise RuntimeError("synthetic extractor failure")
                return 1
            with patch.dict(os.environ, GITHUB_ACTIONS="true", RUNNER_TEMP=temporary), \
                 patch.object(MODULE.platform, "system", return_value="Linux"), \
                 patch.object(MODULE.platform, "machine", return_value="x86_64"), \
                 patch.object(MODULE.resource, "getrlimit", return_value=(0, -1)), \
                 patch.object(MODULE.resource, "setrlimit") as limits, \
                 patch.object(MODULE.subprocess, "check_output", return_value="original\n"), \
                 patch.object(MODULE.subprocess, "run", side_effect=run), \
                 patch.object(MODULE, "report_cores", side_effect=report), \
                 contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                if interrupt:
                    with self.assertRaises(SystemExit) as error:
                        MODULE.main(MODULE.EXPECTED)
                    code = error.exception.code
                else:
                    code = MODULE.main(MODULE.EXPECTED)
            self.assertEqual(limits.call_args_list[0].args[1], (2 * 1024 ** 3, -1))
            self.assertIn((["sudo", "-n", "sysctl", "-q", "-w", "kernel.core_pattern=original"], {"check": True, "timeout": 15}), calls)
            coverage = [command for command, _ in calls if command[:1] == ["timeout"]]
            self.assertTrue(all(command[4:] == MODULE.EXPECTED for command in coverage))
            return code, len(coverage)

    def test_fixed_five_successes_are_all_observed(self):
        self.assertEqual(self.exercise([0] * 5), (0, 5))

    def test_first_failure_stops_and_survives_extractor_or_restore_error(self):
        self.assertEqual(self.exercise([0, 101, 0]), (101, 2))
        self.assertEqual(self.exercise([101], report_error=True), (101, 1))
        self.assertEqual(self.exercise([101], restore_error=True), (101, 1))

    def test_restore_failure_cannot_turn_into_success(self):
        self.assertEqual(self.exercise([0] * 5, restore_error=True), (2, 5))

    def test_interruption_still_restores_core_pattern(self):
        self.assertEqual(self.exercise([], interrupt=True), (143, 1))


if __name__ == "__main__":
    unittest.main()
