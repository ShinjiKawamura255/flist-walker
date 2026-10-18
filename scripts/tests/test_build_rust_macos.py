from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "POSIX bash build entrypoint")
class MacosBuildToolchainTests(unittest.TestCase):
    def exercise(self, *, target=None, channel="1.97.1", rustup=True, exit_code=0):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            scripts = root / "scripts"
            scripts.mkdir()
            script = scripts / "build-rust-macos.sh"
            shutil.copyfile(ROOT / "scripts/build-rust-macos.sh", script)
            (root / "rust").mkdir()
            (root / "rust/rust-toolchain.toml").write_text(
                f'[toolchain]\nchannel = "{channel}"\n', encoding="utf-8"
            )
            home = root / "profile"
            tools = home / ".cargo/bin"
            tools.mkdir(parents=True)
            # Limit PATH so an installed host toolchain cannot mask a missing rustup.
            for name in ("dirname", "sed"):
                (tools / name).symlink_to(shutil.which(name))
            cargo = tools / "cargo"
            cargo.write_text('#!/bin/bash\nprintf "fallback cargo\\n" >> "$BUILD_TEST_LOG"\nexit 99\n')
            cargo.chmod(0o755)
            if rustup:
                command = tools / "rustup"
                command.write_text(
                    '#!/bin/bash\nprintf "%s\\n" "$PWD" "$@" > "$BUILD_TEST_LOG"\n'
                    f'exit {exit_code}\n', encoding="utf-8"
                )
                command.chmod(0o755)
            log = root / "command.log"
            env = dict(os.environ, HOME=str(home), PATH=str(tools),
                       RUSTUP_TOOLCHAIN="stable", BUILD_TEST_LOG=str(log))
            result = subprocess.run(
                [shutil.which("bash"), str(script), *([] if target is None else [target])],
                cwd=temporary, env=env, capture_output=True, text=True, check=False,
            )
            calls = log.read_text().splitlines() if log.exists() else []
            return result, calls, str(root / "rust")

    def test_implicit_target_uses_project_pin_even_with_stable_environment(self):
        result, calls, rust = self.exercise()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [rust, "run", "1.97.1", "cargo", "build", "--release"])

    def test_explicit_target_and_pin_are_forwarded_from_project(self):
        result, calls, rust = self.exercise(target="x86_64-apple-darwin", channel="1.96.0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [rust, "run", "1.96.0", "cargo", "build", "--release",
                                 "--target", "x86_64-apple-darwin"])

    def test_mutable_or_missing_pin_fails_before_build(self):
        for channel in ("stable", "", "1.97"):
            with self.subTest(channel=channel):
                result, calls, _ = self.exercise(channel=channel)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])

    def test_missing_rustup_does_not_fall_back_to_unpinned_cargo(self):
        result, calls, _ = self.exercise(rustup=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])

    def test_failed_fixed_toolchain_does_not_fall_back(self):
        result, calls, _ = self.exercise(exit_code=42)
        self.assertEqual(result.returncode, 42)
        self.assertNotIn("fallback cargo", calls)


if __name__ == "__main__":
    unittest.main()
