"""Preparation is testable without launching an application or changing OS permissions."""
import importlib.util
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest import mock

MODULE = Path(__file__).resolve().parents[1] / "gui-macos-dev-app.py"


class MacDevAppTests(unittest.TestCase):
    def test_fixed_bundle_reuses_identity_and_keeps_profile(self):
        spec = importlib.util.spec_from_file_location("mac_dev_app", MODULE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / "source"
            source.write_bytes(b"first binary")
            destination = base / "gui smoke"
            app = module.prepare_bundle(source, destination)
            profile = destination / "profile"
            (profile / "keep.json").write_text("isolated setting")
            source.write_bytes(b"second binary")
            self.assertEqual(module.prepare_bundle(source, destination), app)
            info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
            self.assertEqual(info["CFBundleIdentifier"], "local.flistwalker.development")
            self.assertEqual(info["CFBundleName"], "FlistWalker Development")
            self.assertEqual((app / "Contents/MacOS/flistwalker").read_bytes(), b"second binary")
            self.assertEqual((profile / "keep.json").read_text(), "isolated setting")
            self.assertFalse((destination / "evidence/native.log").exists())

    def test_foreign_or_symlinked_bundle_is_not_overwritten(self):
        spec = importlib.util.spec_from_file_location("mac_dev_app", MODULE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            source = base / "source"
            source.write_bytes(b"binary")
            app = module.prepare_bundle(source, base / "smoke")
            info_path = app / "Contents/Info.plist"
            info = plistlib.loads(info_path.read_bytes())
            info["CFBundleIdentifier"] = "foreign.application"
            info_path.write_bytes(plistlib.dumps(info))
            with self.assertRaises(ValueError):
                module.prepare_bundle(source, base / "smoke")

    def test_main_rejects_linked_output_before_fixture_or_build(self):
        spec = importlib.util.spec_from_file_location("mac_dev_app", MODULE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for relative in ["rust/target", "rust/target/gui-smoke/root", "rust/target/gui-smoke/evidence",
                         "rust/target/gui-smoke/root/nested"]:
            with self.subTest(path=relative), tempfile.TemporaryDirectory() as temp:
                repo = Path(temp) / "repo"
                repo.mkdir()
                outside = Path(temp) / "outside"
                outside.mkdir()
                marker = outside / "keep"
                marker.write_bytes(b"untouched")
                link = repo / relative
                link.parent.mkdir(parents=True)
                link.symlink_to(outside, target_is_directory=True)
                fixture = repo / "rust/tests/fixtures/gui-smoke/root/nested"
                fixture.mkdir(parents=True, exist_ok=True)
                (fixture / "sample.txt").write_text("fixture")
                with mock.patch.object(module, "__file__", str(repo / "scripts/gui-macos-dev-app.py")), \
                     mock.patch.object(module.sys, "platform", "darwin"), \
                     mock.patch.object(module.sys, "argv", ["prepare", "--no-build"]), \
                     mock.patch.object(module.subprocess, "run") as run:
                    with self.assertRaises((ValueError, SystemExit)):
                        module.main()
                    run.assert_not_called()
                self.assertEqual(marker.read_bytes(), b"untouched")
                self.assertEqual(sorted(p.name for p in outside.iterdir()), ["keep"])
