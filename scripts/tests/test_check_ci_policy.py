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
        self.assertIn("stateful-endurance.yml", POLICY.REQUIRED_WORKFLOWS)
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

    def test_ci_contract_requires_both_windows_gnu_updater_variants_regression(self) -> None:
        text = (ROOT / ".github" / "workflows" / "ci-cross-platform.yml").read_text(
            encoding="utf-8"
        )
        self.assertEqual([], POLICY.validate_ci_contract(text))
        for token in (
            "windows-gnu-update-e2e:",
            "      - windows-gnu-update-e2e",
            "WINDOWS_GNU_UPDATE_RESULT",
            "artifact_dir: rust",
            "${{ matrix.artifact_dir }}/target/x86_64-pc-windows-gnu/release/FlistWalker.exe",
            "${{ matrix.artifact_dir }}/target/x86_64-pc-windows-gnu/release/fw.exe",
            "FLISTWALKER_UPDATE_PUBLIC_KEY_HEX: 79b5562e8fe654f94078b112e8a98ba7901f853ae695bed7e0e3910bad049664",
            "Variant = 'Universal'",
            "Variant = 'Fw'",
            "FLISTWALKER_UPDATE_E2E_PAYLOAD_Universal_V1",
            "FLISTWALKER_UPDATE_E2E_PAYLOAD_Fw_V1",
            "-Variant $case.Variant",
            "-Automated -CleanupSandbox",
            "-AppPath $artifact -UpdateBinaryPath $updatePayload",
            "$callerSigningKey = $env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX",
            "if ($env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX -cne $callerSigningKey)",
        ):
            with self.subTest(token=token):
                self.assertIn(token, text)
                mutated = text.replace(token, "removed-contract", 1)
                violations = POLICY.validate_ci_contract(mutated)
                self.assertTrue(
                    any(
                        "Windows GNU" in item
                        or "artifact" in item
                        or "manifest signer" in item
                        or "sandbox updater" in item
                        or "updater payload" in item
                        for item in violations
                    )
                )

    def test_release_contract_rejects_test_channel_material(self) -> None:
        release_text = (ROOT / ".github" / "workflows" / "release-tagged.yml").read_text(
            encoding="utf-8"
        )
        self.assertEqual([], POLICY.validate_release_contract(release_text))
        for token in (
            "FLISTWALKER_UPDATE_TEST_CHANNEL",
            "79b5562e8fe654f94078b112e8a98ba7901f853ae695bed7e0e3910bad049664",
            "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
        ):
            with self.subTest(token=token):
                violations = POLICY.validate_release_contract(release_text + "\n" + token)
                self.assertTrue(any("forbidden" in item for item in violations))

    def test_release_contract_requires_n_minus_one_latest_release_and_checker_regression(self) -> None:
        release_text = (ROOT / ".github" / "workflows" / "release-tagged.yml").read_text(
            encoding="utf-8"
        )
        self.assertEqual([], POLICY.validate_release_contract(release_text))
        invocation_at = release_text.index(
            "./scripts/check-updater-n-minus-one-compatibility.py"
        )
        for token in (
            'releases/latest" --jq .tag_name',
            "./scripts/check-updater-n-minus-one-compatibility.py",
            "--previous-version",
            "--candidate-version",
            "--manifest",
        ):
            with self.subTest(token=token):
                self.assertIn(token, release_text)
                if token.startswith("--"):
                    before = release_text[:invocation_at]
                    after = release_text[invocation_at:].replace(
                        token, "removed-n-minus-one-contract", 1
                    )
                    mutated = before + after
                else:
                    mutated = release_text.replace(token, "removed-n-minus-one-contract", 1)
                violations = POLICY.validate_release_contract(mutated)
                self.assertTrue(
                    any("N-1" in item or "latest published release" in item for item in violations),
                    violations,
                )

    def test_manual_self_update_cleanup_and_signing_environment_are_fail_closed(self) -> None:
        text = (ROOT / "scripts" / "manual-self-update-test.ps1").read_text(
            encoding="utf-8-sig"
        )
        required = (
            "SandboxDir must not already exist",
            ".flistwalker-update-sandbox-owner",
            "Assert-OwnedSandboxForCleanup",
            "Test-PathIsSameOrAncestor",
            "sandbox ownership sentinel does not match this run",
            "sandbox contains unexpected entries",
            "refusing cleanup of reparse-point sandbox",
            "sandbox contains reparse points",
            "Get-ProcessesForExecutablePath",
            "[switch]$Automated",
            "installed sandbox binary hash mismatch",
            "update transaction artifacts did not settle",
            "request escaped content root",
            "[System.Net.Sockets.TcpListener]::new",
            "$requestParts.Count -ge 2 -and $requestParts[0] -eq 'GET'",
            "[System.Text.Encoding]::UTF8.GetBytes('invalid request')",
            "$client.Dispose()",
            "$listener.Stop()",
            "$psi.EnvironmentVariables.Remove('FLISTWALKER_UPDATE_SIGNING_KEY_HEX')",
            "automated update payload must differ from the initial sandbox binary",
            "mixed-family updater payload discriminator requires different valid payload hashes",
            "counterpart family changed during $Variant update",
            "FLISTWALKER_UPDATE_E2E_OTHER_FAMILY_${Variant}_V1",
            "loopback update feed did not become ready within 5 seconds",
            "$psi.RedirectStandardError = $true",
            "[System.Collections.Generic.Stack[string]]::new()",
            "elseif ($entry.PSIsContainer)",
        )
        for token in required:
            with self.subTest(token=token):
                self.assertIn(token, text)
        cleanup = "Remove-Item -LiteralPath $cleanupPath -Recurse -Force"
        self.assertEqual(1, text.count(cleanup))
        self.assertNotIn("Remove-Item Env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX", text)
        self.assertNotIn("[System.Net.HttpListener]", text)
        self.assertLess(text.index("Assert-OwnedSandboxForCleanup"), text.rindex(cleanup))
        self.assertLess(
            text.index(
                "$psi.EnvironmentVariables.Remove('FLISTWALKER_UPDATE_SIGNING_KEY_HEX')"
            ),
            text.index("[System.Diagnostics.Process]::Start($psi)"),
        )
        self.assertLess(
            text.index("$MixedFamilyPayloadsAreDistinct = $AssetHash -ne $OtherAssetHash"),
            text.index("[System.Diagnostics.Process]::Start($psi)"),
        )
        self.assertLess(
            text.index("installed sandbox binary hash mismatch"),
            text.index("counterpart family changed during $Variant update"),
        )

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

    def test_monitor_workflows_reconcile_only_exact_bot_owned_issues(self) -> None:
        cases = {
            "security-audit.yml": "[security] Scheduled cargo audit failed",
            "ci-canary.yml": "[ci-canary] Latest environment compatibility failed",
        }
        for name, title in cases.items():
            with self.subTest(name=name):
                text = (ROOT / ".github" / "workflows" / name).read_text(
                    encoding="utf-8"
                )
                self.assertEqual(
                    [], POLICY.validate_monitor_issue_contract(name, text, title)
                )

    def test_monitor_issue_contract_rejects_tokens_moved_outside_recovery_job(self) -> None:
        name = "security-audit.yml"
        title = "[security] Scheduled cargo audit failed"
        text = (ROOT / ".github" / "workflows" / name).read_text(encoding="utf-8")
        success_condition = (
            "    if: ${{ success() && github.ref_name == "
            "github.event.repository.default_branch }}"
        )
        safe_selector = (
            '[.[] | select(.title == \\"$TITLE\\" and '
            '.author.login == \\"app/github-actions\\")] | first | .number // empty'
        )
        mutated = text.replace(
            success_condition,
            "    if: ${{ failure() }}",
            1,
        )
        head, separator, tail = mutated.rpartition(safe_selector)
        self.assertTrue(separator)
        mutated = head + ".[0].number // empty" + tail
        mutated += f"\n# {success_condition}\n# {safe_selector}\n"

        violations = POLICY.validate_monitor_issue_contract(name, mutated, title)
        self.assertTrue(
            any("success-only default-branch condition" in item for item in violations)
        )
        self.assertTrue(
            any(
                "safe exact-title bot-owner issue assignment" in item
                for item in violations
            )
        )

    def test_monitor_issue_contract_rejects_non_default_branch_mutation(self) -> None:
        cases = {
            "security-audit.yml": "[security] Scheduled cargo audit failed",
            "ci-canary.yml": "[ci-canary] Latest environment compatibility failed",
        }
        for name, title in cases.items():
            with self.subTest(name=name):
                text = (ROOT / ".github" / "workflows" / name).read_text(
                    encoding="utf-8"
                )
                mutated = text.replace(
                    "    if: ${{ success() && github.ref_name == "
                    "github.event.repository.default_branch }}",
                    "    if: ${{ success() }}",
                    1,
                )
                violations = POLICY.validate_monitor_issue_contract(
                    name, mutated, title
                )
                self.assertTrue(
                    any(
                        "success-only default-branch condition" in item
                        for item in violations
                    )
                )

    def test_monitor_issue_contract_rejects_recovery_tokens_hidden_in_comments(self) -> None:
        name = "security-audit.yml"
        title = "[security] Scheduled cargo audit failed"
        text = (ROOT / ".github" / "workflows" / name).read_text(encoding="utf-8")

        def replace_last(source: str, old: str, new: str) -> str:
            head, separator, tail = source.rpartition(old)
            self.assertTrue(separator)
            return head + new + tail

        needs = "    needs: audit"
        permission = "    permissions:\n      issues: write"
        title_line = f'          TITLE: "{title}"'
        close_line = next(
            line
            for line in text.splitlines()
            if line.lstrip().startswith("gh issue close")
        )
        mutated = replace_last(text, needs, "    # needs moved out of the job")
        mutated = replace_last(
            mutated, permission, "    permissions:\n      contents: read"
        )
        mutated = replace_last(mutated, title_line, "          # title removed")
        mutated = replace_last(mutated, close_line, "            # close removed")
        mutated += (
            f"\n# {needs}\n# {permission}\n# {title_line}\n# {close_line}\n"
        )

        violations = POLICY.validate_monitor_issue_contract(name, mutated, title)
        for expected in (
            "monitor dependency",
            "least-privilege issue permission",
            "exact issue title",
            "exactly one issue close command",
        ):
            with self.subTest(expected=expected):
                self.assertTrue(any(expected in item for item in violations))

    def test_monitor_issue_contract_rejects_query_override_wrong_close_and_extra_permission(
        self,
    ) -> None:
        name = "security-audit.yml"
        title = "[security] Scheduled cargo audit failed"
        text = (ROOT / ".github" / "workflows" / name).read_text(encoding="utf-8")
        close_line = next(
            line
            for line in text.splitlines()
            if line.lstrip().startswith("gh issue close")
        )
        mutated = text.replace(
            close_line,
            close_line.replace('"$issue_number"', '"$victim"'),
            1,
        )
        assignment = next(
            line
            for line in reversed(mutated.splitlines())
            if line.lstrip().startswith('issue_number="$(gh issue list')
        )
        head, separator, tail = mutated.rpartition(assignment)
        self.assertTrue(separator)
        mutated = head + assignment + "; issue_number=1" + tail
        recovery_permission = "    permissions:\n      issues: write\n    steps:"
        head, separator, tail = mutated.rpartition(recovery_permission)
        self.assertTrue(separator)
        mutated = (
            head
            + "    permissions:\n      issues: write\n      actions: read\n    steps:"
            + tail
        )

        violations = POLICY.validate_monitor_issue_contract(name, mutated, title)
        for expected in (
            "safe exact-title bot-owner issue assignment",
            "issue-number recovery close target",
            "least-privilege issue permission",
        ):
            with self.subTest(expected=expected):
                self.assertTrue(any(expected in item for item in violations))


if __name__ == "__main__":
    unittest.main()
