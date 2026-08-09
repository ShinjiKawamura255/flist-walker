from __future__ import annotations

import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check_ci_policy.py"
SPEC = importlib.util.spec_from_file_location("check_ci_policy", MODULE_PATH)
assert SPEC and SPEC.loader
POLICY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(POLICY)


class CiPolicyTests(unittest.TestCase):
    def test_repository_satisfies_ci_policy(self) -> None:
        self.assertEqual([], POLICY.collect_violations(ROOT))

    def test_pr_ci_runs_release_search_performance_gate(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "ci-cross-platform.yml").read_text(
            encoding="utf-8"
        )
        self.assertEqual(
            1,
            workflow.count(
                "cargo test --release --locked perf_search_100k_cold_warm_query_shapes --lib -- --ignored --nocapture"
            ),
        )

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

    def test_guardian_and_dependabot_automation_are_required(self) -> None:
        self.assertIn("ci-policy-guardian.yml", POLICY.REQUIRED_WORKFLOWS)
        self.assertIn("dependabot-auto-merge.yml", POLICY.REQUIRED_WORKFLOWS)
        self.assertIn(".github/dependabot.yml", POLICY.REQUIRED_FILES)
        trusted_paths = POLICY.trusted_policy_paths(ROOT)
        self.assertIn("rust/.cargo/audit.toml", trusted_paths)
        self.assertIn("scripts/tests/test_check_ci_policy.py", trusted_paths)

    def test_write_all_and_unapproved_pull_request_target_are_rejected(self) -> None:
        excessive = """
name: Excessive
on: push
permissions: write-all
concurrency:
  group: excessive
jobs:
  test:
    runs-on: ubuntu-24.04
    timeout-minutes: 5
"""
        violations = POLICY.validate_workflow("excessive.yml", excessive)
        self.assertTrue(any("write-all" in item for item in violations))

        inline = excessive.replace("permissions: write-all", "permissions: {contents: write}")
        violations = POLICY.validate_workflow("inline.yml", inline)
        self.assertTrue(any("explicit block" in item for item in violations))

        untrusted_target = excessive.replace("on: push", "on: pull_request_target")
        violations = POLICY.validate_workflow("not-guardian.yml", untrusted_target)
        self.assertTrue(any("pull_request_target is guardian-only" in item for item in violations))

    def test_guardian_contract_is_fail_closed(self) -> None:
        violations = POLICY.validate_guardian_contract("name: CI Policy Guardian\n")
        self.assertTrue(any("trusted base checkout" in item for item in violations))
        self.assertTrue(any("PR blobs as data" in item for item in violations))
        self.assertTrue(any("immutable trusted policy" in item for item in violations))

    def test_dependabot_contract_requires_trusted_completed_ci(self) -> None:
        violations = POLICY.validate_dependabot_contract("name: Dependabot Auto Merge\n")
        self.assertTrue(any("workflow_run" in item for item in violations))
        self.assertTrue(any("trusted Dependabot actor" in item for item in violations))
        self.assertTrue(any("successful CI conclusion" in item for item in violations))

    def test_dependabot_contract_requires_exact_rebase_auto_merge(self) -> None:
        workflow_path = ROOT / ".github" / "workflows" / "dependabot-auto-merge.yml"
        current = workflow_path.read_text(encoding="utf-8")
        rebase = current.replace(
            "--auto --merge", "--auto --rebase --delete-branch"
        )
        self.assertEqual([], POLICY.validate_dependabot_contract(rebase))

        for disallowed in ("--merge", "--squash"):
            with self.subTest(disallowed=disallowed):
                candidate = rebase.replace(
                    "--rebase --delete-branch", f"{disallowed} --delete-branch"
                )
                violations = POLICY.validate_dependabot_contract(candidate)
                self.assertTrue(any("rebase auto-merge" in item for item in violations))

        duplicate = rebase + (
            '\n      - run: gh pr merge "$PR_NUMBER" '
            '--repo "$GITHUB_REPOSITORY" --auto --rebase --delete-branch\n'
        )
        violations = POLICY.validate_dependabot_contract(duplicate)
        self.assertTrue(any("exactly one" in item for item in violations))

        exact_line = (
            '        run: gh pr merge "$PR_NUMBER" '
            '--repo "$GITHUB_REPOSITORY" --auto --rebase --delete-branch'
        )
        for lookalike in (
            exact_line.replace("run: gh", "run: echo gh"),
            exact_line.replace("run: gh", "# gh"),
        ):
            with self.subTest(lookalike=lookalike.strip()):
                candidate = rebase.replace(exact_line, lookalike)
                violations = POLICY.validate_dependabot_contract(candidate)
                self.assertTrue(any("rebase auto-merge" in item for item in violations))

    def test_trusted_policy_update_allows_only_version_pins(self) -> None:
        checker = MODULE_PATH.read_text(encoding="utf-8")
        self.assertEqual(
            POLICY.normalize_trusted_policy("scripts/check_ci_policy.py", checker),
            POLICY.normalize_trusted_policy(
                "scripts/check_ci_policy.py",
                checker.replace('PINNED_RUST = "1.97.1"', 'PINNED_RUST = "1.98.0"'),
            ),
        )
        weakened = checker.replace(
            'violations.append(f"{name}: permissions write-all is forbidden")',
            "pass",
        )
        self.assertNotEqual(
            POLICY.normalize_trusted_policy("scripts/check_ci_policy.py", checker),
            POLICY.normalize_trusted_policy("scripts/check_ci_policy.py", weakened),
        )
        invalid_pin = checker.replace('PINNED_RUST = "1.97.1"', 'PINNED_RUST = "stable"')
        with mock.patch.object(Path, "read_text", return_value=invalid_pin):
            _, violations = POLICY.read_proposed_pins(ROOT)
        self.assertTrue(any("invalid PINNED_RUST" in item for item in violations))

        guardian_path = ROOT / ".github" / "workflows" / "ci-policy-guardian.yml"
        guardian = guardian_path.read_text(encoding="utf-8")
        updated_action = guardian.replace(
            "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1",
            "actions/checkout@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa # candidate",
        )
        self.assertEqual(
            POLICY.normalize_trusted_policy(
                ".github/workflows/ci-policy-guardian.yml", guardian
            ),
            POLICY.normalize_trusted_policy(
                ".github/workflows/ci-policy-guardian.yml", updated_action
            ),
        )
        weakened_guardian = guardian.replace("contents: read", "contents: write")
        self.assertNotEqual(
            POLICY.normalize_trusted_policy(
                ".github/workflows/ci-policy-guardian.yml", guardian
            ),
            POLICY.normalize_trusted_policy(
                ".github/workflows/ci-policy-guardian.yml", weakened_guardian
            ),
        )

    def test_trusted_policy_set_rejects_structural_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            proposed_root = Path(temp_dir)
            for relative in POLICY.trusted_policy_paths(ROOT):
                source = ROOT / relative
                destination = proposed_root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)
            self.assertEqual(
                [], POLICY.validate_trusted_policy_update(ROOT, proposed_root)
            )

            audit_path = proposed_root / "rust" / ".cargo" / "audit.toml"
            audit_text = audit_path.read_text(encoding="utf-8")
            audit_path.write_text(
                audit_text.replace(
                    "ignore = [",
                    'ignore = [\n    "RUSTSEC-2099-9999",',
                    1,
                ),
                encoding="utf-8",
            )
            violations = POLICY.validate_trusted_policy_update(ROOT, proposed_root)
            self.assertTrue(any("rust/.cargo/audit.toml" in item for item in violations))
            audit_path.write_text(audit_text, encoding="utf-8")

            ci_path = proposed_root / ".github" / "workflows" / "ci-cross-platform.yml"
            ci_text = ci_path.read_text(encoding="utf-8")
            ci_path.write_text(
                ci_text.replace("  contents: read", "  contents: write", 1),
                encoding="utf-8",
            )
            violations = POLICY.validate_trusted_policy_update(ROOT, proposed_root)
            self.assertTrue(any("ci-cross-platform.yml" in item for item in violations))

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
