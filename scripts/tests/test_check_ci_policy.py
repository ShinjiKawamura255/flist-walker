from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check_ci_policy.py"
SPEC = importlib.util.spec_from_file_location("check_ci_policy", MODULE_PATH)
assert SPEC and SPEC.loader
POLICY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POLICY)


class CiPolicyTests(unittest.TestCase):
    def test_repository_satisfies_ci_policy(self) -> None:
        self.assertEqual([], POLICY.collect_violations(ROOT))

    def test_mutable_required_workflow_is_rejected(self) -> None:
        workflow = """
name: Example
on: push
permissions:
  contents: read
concurrency:
  group: example
jobs:
  test:
    runs-on: ubuntu-latest
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@stable
"""
        violations = POLICY.validate_workflow("example.yml", workflow)
        self.assertTrue(any("latest runner aliases" in item for item in violations))
        self.assertTrue(any("full SHA" in item for item in violations))
        self.assertTrue(any("Rust must be pinned" in item for item in violations))

    def test_ci_contract_requires_audit_skip_safety(self) -> None:
        violations = POLICY.validate_ci_contract("name: CI Gate\nif: ${{ always() }}")
        self.assertTrue(any("Cargo change output" in item for item in violations))
        self.assertTrue(any("audit result in gate" in item for item in violations))
        self.assertTrue(any("Cargo safe-skip gate" in item for item in violations))

    def test_audit_change_paths_cover_nested_workspace_and_policy(self) -> None:
        relevant = [
            "rust/Cargo.toml",
            "rust/Cargo.lock",
            "tools/helper/Cargo.toml",
            "rust/.cargo/audit.toml",
            ".github/workflows/security-audit.yml",
            "scripts/check_ci_policy.py",
            "scripts/tests/test_check_ci_policy.py",
        ]
        for path in relevant:
            with self.subTest(path=path):
                self.assertTrue(POLICY.is_audit_relevant_path(path))
        self.assertFalse(POLICY.is_audit_relevant_path("docs/CI_OPERATIONS.md"))

    def test_audit_skip_truth_table(self) -> None:
        self.assertTrue(POLICY.audit_result_is_acceptable(True, "success"))
        self.assertFalse(POLICY.audit_result_is_acceptable(True, "skipped"))
        self.assertTrue(POLICY.audit_result_is_acceptable(False, "skipped"))
        self.assertFalse(POLICY.audit_result_is_acceptable(False, "success"))


if __name__ == "__main__":
    unittest.main()
