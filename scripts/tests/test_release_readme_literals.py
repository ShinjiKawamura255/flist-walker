from pathlib import Path
import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[2]


class ReleaseReadmeLiteralTests(unittest.TestCase):
    def test_packaging_keeps_markdown_literals_without_executing_them(self):
        for platform, script, target, universal, fw in (
            ("windows-x86_64", "prepare-release.sh", "x86_64-pc-windows-gnu", "FlistWalker.exe", "fw.exe"),
            ("linux-x86_64", "prepare-release-linux.sh", "x86_64-unknown-linux-gnu", "flistwalker", "fw"),
        ):
            with self.subTest(platform=platform), tempfile.TemporaryDirectory(prefix="fw-readme-") as temporary:
                repo = Path(temporary)
                (repo / "scripts").mkdir()
                shutil.copyfile(ROOT / "scripts" / script, repo / "scripts" / script)
                (repo / "LICENSE").write_text("owned license\n")
                (repo / "THIRD_PARTY_NOTICES.txt").write_text("owned notices\n")
                binaries = repo / "rust" / "target" / target / "release"
                binaries.mkdir(parents=True)
                for name in (universal, fw):
                    (binaries / name).write_bytes(b"owned inert packaging fixture\n")
                commands = repo / "canaries"
                commands.mkdir()
                marker = repo / "unexpected-command"
                for name in ("flistwalker.ignore.txt.example", "flistwalker.ignore.txt"):
                    command = commands / name
                    command.write_text('#!/bin/bash\nprintf "unexpected\\n" >> "$README_TEST_MARKER"\nprintf "unexpected"\n')
                    command.chmod(0o700)
                env = {"PATH": str(commands) + os.pathsep + os.environ.get("PATH", "/usr/bin:/bin"),
                       "HOME": str(repo), "README_TEST_MARKER": str(marker)}
                arguments = ["bash", str(repo / "scripts" / script), "v0.31.0"]
                if platform.startswith("linux"):
                    arguments.append(target)
                result = subprocess.run(arguments, cwd=repo, env=env, text=True,
                                        capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stderr, "")
                self.assertFalse(marker.exists(), "README generation executed Markdown code")
                output = repo / "dist" / "v0.31.0"
                base = "FlistWalker-0.31.0-" + platform
                readme = (output / (base + ".README.txt")).read_bytes()
                self.assertIn(b"`flistwalker.ignore.txt.example`", readme)
                self.assertIn(b"`flistwalker.ignore.txt`", readme)
                self.assertIn(b"FlistWalker v0.31.0", readme)
                self.assertIn(("fw-0.31.0-" + platform + (".exe" if platform.startswith("windows") else "")).encode(), readme)
                if platform.startswith("windows"):
                    with zipfile.ZipFile(output / (base + ".zip")) as archive:
                        self.assertEqual(archive.read("README.txt"), readme)
                else:
                    with tarfile.open(output / (base + ".tar.gz")) as archive:
                        self.assertEqual(archive.extractfile("README.txt").read(), readme)


if __name__ == "__main__":
    unittest.main()
