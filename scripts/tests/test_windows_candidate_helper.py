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
import types
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "docs/releases/evidence/v0.31.0/windows/verify-windows-candidate.py"
SPEC = importlib.util.spec_from_file_location("windows_candidate_helper", HELPER)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class WindowsCandidateHelperTests(unittest.TestCase):
    def exercise(self, *, timeout=None, ratio=0.5, invalid_output=False):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / "bundle"
            bundle.mkdir()
            binding = dict(version="0.31.0", source_sha="3194f03c27f9a72f06a836dbd02e9141d8740a26",
                           run_id=39087986481, attempt=1, artifact={"id": 12122471715}, windows={})
            rows = []
            for role, name in (("universal", "FlistWalker-0.31.0-windows-x86_64.exe"),
                               ("fw", "fw-0.31.0-windows-x86_64.exe")):
                data = role.encode()
                (bundle / name).write_bytes(data)
                digest = hashlib.sha256(data).hexdigest()
                rows.append(f"{digest}  {name}")
                binding["windows"][role] = dict(filename=name, sha256=digest)
            rows.extend(f"{index:064x}  unused{index}" for index in range(24))
            (bundle / "SHA256SUMS").write_text("\n".join(rows) + "\n", encoding="ascii")
            (bundle / "SHA256SUMS.sig").write_bytes(b"synthetic signature")
            binding.update(manifest_sha256=MODULE.sha256(bundle / "SHA256SUMS"),
                           signature_sha256=MODULE.sha256(bundle / "SHA256SUMS.sig"))
            binding_path = root / "binding.json"
            binding_path.write_text(json.dumps(binding), encoding="utf-8")
            output = root / "result.json"
            args = ["verify", "--bundle", str(bundle), "--binding", str(binding_path),
                    "--output", str(output)]
            ticks = [0]

            def run(argv, **kwargs):
                self.assertFalse(kwargs["shell"])
                self.assertEqual(kwargs["timeout"], 30)
                self.assertIs(kwargs["stdout"], subprocess.PIPE)
                self.assertIs(kwargs["stderr"], subprocess.PIPE)
                self.assertEqual(kwargs["env"]["FLISTWALKER_DISABLE_SELF_UPDATE"], "1")
                self.assertNotEqual(kwargs["env"]["HOME"], os.environ.get("HOME"))
                role = "fw" if Path(argv[0]).name.startswith("fw-") else "universal"
                ticks[0] += int(100_000_000 * (ratio if role == "fw" else 1))
                if "--version" in argv:
                    return subprocess.CompletedProcess(argv, 0, b"fw 0.31.0\n", b"")
                if "--check-update" in argv:
                    return subprocess.CompletedProcess(argv, 0, b"Update checks are disabled.\n", b"")
                if "--update" in argv:
                    return subprocess.CompletedProcess(argv, 1, b"", b"Automatic updates are disabled.\n")
                self.assertEqual("--cli" in argv, role == "universal")
                self.assertEqual(len(list(Path(argv[argv.index("--root") + 1]).iterdir())), 200)
                if timeout is not None:
                    raise subprocess.TimeoutExpired(argv, 30, output=timeout[0], stderr=timeout[1])
                data = b"WRONG\n" if invalid_output else "".join(f"file_{n:03d}.txt\n" for n in range(200)).encode()
                return subprocess.CompletedProcess(argv, 0, data, b"")

            native = types.SimpleNamespace(name="nt", environ=os.environ, cpu_count=os.cpu_count)
            with patch.object(MODULE, "os", native), \
                 patch.object(MODULE.platform, "machine", return_value="AMD64"), \
                 patch.object(MODULE.subprocess, "CREATE_NO_WINDOW", 0, create=True), \
                 patch.object(MODULE.subprocess, "run", side_effect=run), \
                 patch.object(MODULE.time, "perf_counter_ns", side_effect=lambda: ticks[0]), \
                 patch("sys.argv", args), contextlib.redirect_stdout(io.StringIO()):
                code = MODULE.main()
            result = json.loads(output.read_text())
            raw = [json.loads(row) for row in output.with_suffix(".raw.jsonl").read_text().splitlines()]
            before = output.read_bytes(), output.with_suffix(".raw.jsonl").read_bytes()
            with patch.object(MODULE, "os", native), \
                 patch.object(MODULE.platform, "machine", return_value="AMD64"), \
                 patch("sys.argv", args), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    MODULE.main()
                self.assertEqual(error.exception.code, 2)
            self.assertEqual(before, (output.read_bytes(), output.with_suffix(".raw.jsonl").read_bytes()))
            return code, result, raw

    def test_timeout_retains_partial_output_and_previous_observations(self):
        for partial in ((b"first path\n", b"worker pending\n"), (None, None)):
            with self.subTest(partial=partial):
                code, result, raw = self.exercise(timeout=partial)
                self.assertEqual(code, 1)
                self.assertEqual(result["result"], "FAIL")
                self.assertIn("TimeoutExpired", result["error"])
                self.assertEqual(len(raw), 43)
                timed_out = raw[-1]
                self.assertEqual(timed_out["role"], "universal")
                self.assertTrue(any("<owned-test-root>" in part for part in timed_out["argv"]))
                self.assertTrue(timed_out["timed_out"])
                self.assertEqual(timed_out["timeout_seconds"], 30)
                self.assertEqual(timed_out["elapsed_ms"], 100)
                self.assertIsNone(timed_out["exit_code"])
                self.assertEqual(timed_out["stdout"], (partial[0] or b"").decode())
                self.assertEqual(timed_out["stderr"], (partial[1] or b"").decode())
                self.assertEqual(timed_out["stdout_sha256"], hashlib.sha256(partial[0] or b"").hexdigest())
                self.assertNotIn("tc193", result)

    def test_synthetic_success_keeps_all_samples_and_output(self):
        code, result, raw = self.exercise()
        self.assertEqual(code, 0)
        self.assertEqual(len(raw), 102)
        self.assertEqual(result["tc193"]["ratio_fw_over_universal"], 0.5)
        self.assertEqual(result["tc193"]["warmup_per_variant"], 5)
        self.assertEqual(result["tc193"]["samples_per_variant"], 25)

    def test_equal_timing_fails_unchanged_threshold(self):
        code, result, raw = self.exercise(ratio=1)
        self.assertEqual(code, 1)
        self.assertEqual(len(raw), 102)
        self.assertEqual(result["tc193"]["required_max_ratio"], 0.70)

    def test_first_bad_output_is_preserved(self):
        code, result, raw = self.exercise(invalid_output=True)
        self.assertEqual(code, 1)
        self.assertIn("output/exit contract failed", result["error"])
        self.assertEqual(len(raw), 43)
        self.assertEqual(raw[-1]["stdout"], "WRONG\n")

    @unittest.skipIf(os.name == "nt", "requires a non-Windows host")
    def test_non_windows_execution_is_refused(self):
        with patch("sys.argv", ["verify", "--bundle", "x", "--binding", "x", "--output", "x"]), \
             contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                MODULE.main()
            self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
