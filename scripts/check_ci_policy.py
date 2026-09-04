#!/usr/bin/env python3
"""Validate repository CI reliability and least-privilege contracts."""

from __future__ import annotations

import re
import sys
from pathlib import Path


PINNED_RUST = "1.97.1"
PINNED_CARGO_AUDIT = "0.22.2"
PINNED_CARGO_LLVM_COV = "0.8.7"
REQUIRED_WORKFLOWS = {
    "ci-cross-platform.yml",
    "ci-policy-guardian.yml",
    "ci-canary.yml",
    "dependabot-auto-merge.yml",
    "security-audit.yml",
    "perf-regression.yml",
    "stateful-endurance.yml",
    "release-tagged.yml",
}
REQUIRED_FILES = {".github/dependabot.yml"}
ALLOWED_WRITE_PERMISSIONS = {
    "ci-canary.yml": {"issues"},
    "dependabot-auto-merge.yml": {"contents", "pull-requests"},
    "release-tagged.yml": {"contents"},
    "security-audit.yml": {"issues"},
}
MONITOR_ISSUE_TITLES = {
    "ci-canary.yml": "[ci-canary] Latest environment compatibility failed",
    "security-audit.yml": "[security] Scheduled cargo audit failed",
}
MONITOR_NEEDS = {
    "ci-canary.yml": "latest-compatibility",
    "security-audit.yml": "audit",
}
LATEST_RUNNER_RE = re.compile(r"\b(?:ubuntu|windows|macos)-latest\b")
ACTION_RE = re.compile(r"^\s*(?:-\s*)?uses:\s*([^@\s]+)@([^\s#]+)", re.MULTILINE)
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
PIN_ASSIGNMENT_RE = re.compile(
    r'(?m)^(PINNED_(?:RUST|CARGO_AUDIT|CARGO_LLVM_COV) = ")[^"]+("\s*)$'
)
ACTION_LINE_RE = re.compile(
    r"(?m)^(\s*uses:\s*[^@\s]+@)[0-9a-f]{40}(\s+#.*)?$"
)
RUNNER_GENERATION_RE = re.compile(r"\b(ubuntu|windows|macos)-[0-9][A-Za-z0-9.-]*\b")
WORKFLOW_VERSION_LINE_RE = re.compile(
    r"(?m)^(\s*(?:RUST_VERSION|CARGO_AUDIT_VERSION|CARGO_LLVM_COV_VERSION|toolchain):\s*)"
    r"[0-9]+\.[0-9]+\.[0-9]+(\s*)$"
)
TOOLCHAIN_CHANNEL_RE = re.compile(
    r'(?m)^(\s*channel\s*=\s*")[0-9]+\.[0-9]+\.[0-9]+("\s*)$'
)
PIN_NAMES = ("PINNED_RUST", "PINNED_CARGO_AUDIT", "PINNED_CARGO_LLVM_COV")
AUDIT_RELEVANT_PATH_RE = re.compile(
    r"(^|/)Cargo\.(toml|lock)$|^rust/\.cargo/audit\.toml$|"
    r"^\.github/workflows/(ci-cross-platform|security-audit)\.yml$|"
    r"^scripts/(check_ci_policy\.py|tests/test_check_ci_policy\.py)$"
)
DOCS_ONLY_PATH_RE = re.compile(
    r"^(?:docs/.*|README\.md|README-ja\.md|CHANGELOG\.md|AGENTS\.md|LICENSE|"
    r"THIRD_PARTY_NOTICES\.txt|\.github/ISSUE_TEMPLATE/.*|"
    r"\.github/release-template\.md)$"
)


def _job_blocks(text: str) -> list[tuple[str, str]]:
    marker = "\njobs:\n"
    if marker not in text:
        return []
    jobs_text = text.split(marker, 1)[1]
    return re.findall(
        r"(?ms)^  ([A-Za-z0-9_-]+):\n(.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        jobs_text,
    )


def _named_step_blocks(job_block: str, step_name: str) -> list[str]:
    return re.findall(
        rf"(?ms)(^      - name: {re.escape(step_name)}\n.*?)(?=^      - |\Z)",
        job_block,
    )


def _validate_blocking_clippy_step(
    text: str,
    *,
    workflow_name: str,
    job_name: str,
    step_name: str,
    expected_if: str | None,
) -> list[str]:
    violation = (
        f"{workflow_name}: {job_name} must contain exactly one blocking native clippy step"
    )
    job_blocks = [block for name, block in _job_blocks(text) if name == job_name]
    if len(job_blocks) != 1:
        return [violation]

    job_block = job_blocks[0]
    steps = _named_step_blocks(job_block, step_name)
    if len(steps) != 1:
        return [violation]

    step = steps[0]
    required_properties = (
        "        working-directory: ${{ matrix.workdir }}",
        "        shell: bash",
        "        run: cargo clippy --locked --all-targets -- -D warnings",
    )
    step_lines = step.splitlines()
    if any(step_lines.count(line) != 1 for line in required_properties):
        return [violation]
    expected_property_keys = ["working-directory", "shell", "run"]
    if expected_if is not None:
        expected_property_keys.insert(0, "if")
    actual_property_keys = re.findall(r"(?m)^        ([A-Za-z0-9_-]+):", step)
    if actual_property_keys != expected_property_keys:
        return [violation]

    if expected_if is None:
        if re.search(r"(?m)^        if:", step):
            return [violation]
    elif step_lines.count(f"        if: {expected_if}") != 1 or len(
        re.findall(r"(?m)^        if:", step)
    ) != 1:
        return [violation]

    if re.search(r"(?m)^    continue-on-error:", job_block) or re.search(
        r"(?m)^        continue-on-error:", step
    ):
        return [violation]
    return []


def is_audit_relevant_path(path: str) -> bool:
    return AUDIT_RELEVANT_PATH_RE.search(path) is not None


def audit_result_is_acceptable(cargo_changed: bool, audit_result: str) -> bool:
    expected = "success" if cargo_changed else "skipped"
    return audit_result == expected


def heavy_ci_required_for_changes(
    changes: list[tuple[str, str]],
    *,
    base_known: bool = True,
    diff_succeeded: bool = True,
) -> bool:
    if not base_known or not diff_succeeded or not changes:
        return True
    # Regression guard: skip is an allowlist of ordinary documentation edits.
    # Renames, deletes, and unknown paths must run heavy CI rather than guessing.
    return any(
        status not in {"A", "M"} or DOCS_ONLY_PATH_RE.fullmatch(path) is None
        for status, path in changes
    )


def heavy_ci_results_are_acceptable(
    heavy_ci_required: bool, results: dict[str, str]
) -> bool:
    expected = "success" if heavy_ci_required else "skipped"
    required_jobs = {
        "rust-test-build",
        "windows-gnu-updater-build",
        "windows-gnu-update-e2e",
        "lint-and-coverage",
    }
    return results.keys() == required_jobs and all(
        result == expected for result in results.values()
    )


def normalize_trusted_policy(name: str, text: str) -> str:
    if name == "scripts/check_ci_policy.py":
        return PIN_ASSIGNMENT_RE.sub(r'\1<PIN>\2', text)
    if name.startswith(".github/workflows/"):
        normalized = ACTION_LINE_RE.sub(r"\1<ACTION_SHA>", text)
        normalized = RUNNER_GENERATION_RE.sub(r"\1-<GENERATION>", normalized)
        return WORKFLOW_VERSION_LINE_RE.sub(r"\1<VERSION>\2", normalized)
    if name == "rust/rust-toolchain.toml":
        return TOOLCHAIN_CHANNEL_RE.sub(r'\1<VERSION>\2', text)
    return text


def trusted_policy_paths(root: Path) -> set[str]:
    workflows = root / ".github" / "workflows"
    paths = {
        path.relative_to(root).as_posix()
        for pattern in ("*.yml", "*.yaml")
        for path in workflows.glob(pattern)
    }
    paths.update(
        {
            ".github/dependabot.yml",
            "rust/.cargo/audit.toml",
            "rust/rust-toolchain.toml",
            "scripts/check_ci_policy.py",
            "scripts/tests/test_check_ci_policy.py",
        }
    )
    return paths


def read_proposed_pins(root: Path) -> tuple[dict[str, str], list[str]]:
    text = (root / "scripts" / "check_ci_policy.py").read_text(encoding="utf-8")
    pins: dict[str, str] = {}
    violations: list[str] = []
    for name in PIN_NAMES:
        match = re.search(rf'(?m)^{name} = "([0-9]+\.[0-9]+\.[0-9]+)"\s*$', text)
        if match is None:
            violations.append(f"scripts/check_ci_policy.py: invalid {name} version pin")
        else:
            pins[name] = match.group(1)
    return pins, violations


def validate_trusted_policy_update(trusted_root: Path, proposed_root: Path) -> list[str]:
    violations: list[str] = []
    trusted_paths = trusted_policy_paths(trusted_root)
    proposed_paths = trusted_policy_paths(proposed_root)
    if trusted_paths != proposed_paths:
        added = sorted(proposed_paths - trusted_paths)
        removed = sorted(trusted_paths - proposed_paths)
        violations.append(
            f"immutable trusted policy file set changed; added={added}, removed={removed}"
        )
    for relative in sorted(trusted_paths | proposed_paths):
        trusted = trusted_root / relative
        proposed = proposed_root / relative
        if not trusted.is_file() or not proposed.is_file():
            violations.append(f"{relative}: immutable trusted policy file is missing")
            continue
        trusted_text = normalize_trusted_policy(relative, trusted.read_text(encoding="utf-8"))
        proposed_text = normalize_trusted_policy(relative, proposed.read_text(encoding="utf-8"))
        if trusted_text != proposed_text:
            violations.append(
                f"{relative}: immutable trusted policy changed outside approved runner/action/tool pins"
            )
    return violations


def validate_workflow(name: str, text: str) -> list[str]:
    violations: list[str] = []
    is_canary = name == "ci-canary.yml"
    is_guardian = name == "ci-policy-guardian.yml"

    uses_pull_request_target = re.search(r"\bpull_request_target\b", text) is not None
    if uses_pull_request_target and not is_guardian:
        violations.append(f"{name}: pull_request_target is guardian-only")
    if is_guardian and not uses_pull_request_target:
        violations.append(f"{name}: guardian must use pull_request_target")
    if "permissions:" not in text:
        violations.append(f"{name}: explicit permissions are required")
    for permission_line in re.findall(
        r"(?m)^[ \t]*permissions:[ \t]*([^\r\n]*)$", text
    ):
        if permission_line.strip():
            if "write-all" in permission_line:
                violations.append(f"{name}: permissions write-all is forbidden")
            else:
                violations.append(f"{name}: permissions must use an explicit block")
    allowed_writes = ALLOWED_WRITE_PERMISSIONS.get(name, set())
    for permission in re.findall(r"(?m)^\s+([a-z-]+):\s*write\s*$", text):
        if permission not in allowed_writes:
            violations.append(f"{name}: {permission}: write is not allowed")
    if "concurrency:" not in text:
        violations.append(f"{name}: workflow concurrency is required")
    if "ImageVersion" not in text:
        violations.append(f"{name}: hosted runner image version must be recorded")
    if not is_canary and LATEST_RUNNER_RE.search(text):
        violations.append(f"{name}: latest runner aliases are canary-only")

    for action, ref in ACTION_RE.findall(text):
        if not SHA_RE.fullmatch(ref):
            violations.append(f"{name}: action {action}@{ref} is not pinned to a full SHA")

    for job_name, block in _job_blocks(text):
        if "runs-on:" in block and "timeout-minutes:" not in block:
            violations.append(f"{name}: job {job_name} needs timeout-minutes")

    for cache_block in (
        block
        for block in re.split(r"(?m)^[ \t]*- name:", text)
        if re.search(r"(?m)^\s*uses:\s*actions/cache@", block)
    ):
        if "~/.cargo/bin" in cache_block:
            violations.append(f"{name}: cache must not include ~/.cargo/bin")
        if "rust/target" in cache_block:
            violations.append(f"{name}: cache must not include rust/target")

    if not is_canary and "rust-toolchain@" in text and f"toolchain: {PINNED_RUST}" not in text:
        violations.append(f"{name}: Rust must be pinned to {PINNED_RUST}")
    if "cargo install cargo-audit" in text and (
        f"cargo-audit --version {PINNED_CARGO_AUDIT}" not in text
    ):
        violations.append(f"{name}: cargo-audit must be pinned to {PINNED_CARGO_AUDIT}")
    if "cargo install cargo-llvm-cov" in text and (
        f"cargo-llvm-cov --version {PINNED_CARGO_LLVM_COV}" not in text
    ):
        violations.append(f"{name}: cargo-llvm-cov must be pinned to {PINNED_CARGO_LLVM_COV}")

    return violations


def validate_guardian_contract(text: str) -> list[str]:
    violations: list[str] = []
    required_tokens = {
        "pull request target trigger": "pull_request_target:",
        "master branch scope": "      - master",
        "read-only contents permission": "permissions:\n  contents: read",
        "trusted base checkout": "ref: ${{ github.event.pull_request.base.sha }}",
        "checkout credential isolation": "persist-credentials: false",
        "PR blobs as data": "git/blobs/$blob_sha",
        "recursive PR tree as data": "git/trees/$HEAD_SHA?recursive=1",
        "trusted checker execution": 'python scripts/check_ci_policy.py --guardian "$PROPOSED_ROOT"',
        "immutable trusted policy": "--guardian",
    }
    for label, token in required_tokens.items():
        if token not in text:
            violations.append(f"ci-policy-guardian.yml: missing {label}")
    forbidden_tokens = {
        "PR code checkout": "ref: ${{ github.event.pull_request.head.sha }}",
        "secret access": "secrets.",
        "cache": "actions/cache@",
        "artifact upload": "actions/upload-artifact@",
        "artifact download": "actions/download-artifact@",
    }
    for label, token in forbidden_tokens.items():
        if token in text:
            violations.append(f"ci-policy-guardian.yml: forbidden {label}")
    return violations


def validate_dependabot_contract(text: str) -> list[str]:
    violations: list[str] = []
    required_tokens = {
        "workflow_run trigger": "workflow_run:",
        "trusted completed CI workflow": "      - CI Cross Platform",
        "completed event": "      - completed",
        "pull request event": "github.event.workflow_run.event == 'pull_request'",
        "successful CI conclusion": "github.event.workflow_run.conclusion == 'success'",
        "trusted Dependabot actor": "github.event.workflow_run.actor.login == 'dependabot[bot]'",
    }
    for label, token in required_tokens.items():
        if token not in text:
            violations.append(f"dependabot-auto-merge.yml: missing {label}")
    expected = (
        'gh pr merge "$PR_NUMBER" --repo "$GITHUB_REPOSITORY" '
        "--auto --rebase --delete-branch"
    )
    merge_mentions = [line for line in text.splitlines() if "gh pr merge" in line]
    exact_pattern = re.compile(rf"^\s*run:\s+{re.escape(expected)}\s*$")
    exact_commands = [line for line in merge_mentions if exact_pattern.fullmatch(line)]
    if len(merge_mentions) != 1 or len(exact_commands) != 1:
        violations.append(
            "dependabot-auto-merge.yml: exactly one gh pr merge command is required"
        )
    if len(exact_commands) != 1:
        violations.append(
            "dependabot-auto-merge.yml: missing exact rebase auto-merge command"
        )
    if any(
        re.search(r"(?:^|\s)--(?:merge|squash)(?:\s|$)", line)
        for line in merge_mentions
    ):
        violations.append(
            "dependabot-auto-merge.yml: merge and squash modes are forbidden"
        )
    return violations


def validate_ci_contract(text: str) -> list[str]:
    violations: list[str] = []
    required_tokens = {
        "CI Gate job": "name: CI Gate",
        "always aggregation": "if: ${{ always() }}",
        "Cargo change output": "cargo_changed",
        "heavy CI change output": "heavy_ci_required: ${{ steps.changes.outputs.heavy_ci_required }}",
        "heavy CI default": "heavy_ci_required=true",
        "documentation-only skip": "heavy_ci_required=false",
        "rename/delete fail closed": 'if [[ "$status" != "A" && "$status" != "M" ]]',
        "workspace Cargo manifests": "Cargo\\.(toml|lock)$",
        "audit configuration": r"rust/\.cargo/audit\.toml",
        "audit workflow": r".github/workflows/security-audit\.yml",
        "policy implementation": r"scripts/check_ci_policy\.py",
        "policy tests": r"scripts/tests/test_check_ci_policy\.py",
        "audit result in gate": "AUDIT_RESULT",
        "Cargo safe-skip gate": "CARGO_CHANGED",
        "required audit success check": '[[ "$AUDIT_RESULT" == "success" ]]',
        "non-Cargo audit skip check": '[[ "$AUDIT_RESULT" == "skipped" ]]',
        "master push trigger": "      - master",
        "pull request trigger": "  pull_request:",
        "policy gate dependency": "      - ci-policy",
        "test gate dependency": "      - rust-test-build",
        "Windows GNU producer job": "windows-gnu-updater-build:",
        "Windows GNU producer gate dependency": "      - windows-gnu-updater-build",
        "Windows GNU producer gate result": "WINDOWS_GNU_BUILD_RESULT",
        "Windows GNU updater E2E job": "windows-gnu-update-e2e:",
        "Windows GNU updater gate dependency": "      - windows-gnu-update-e2e",
        "Windows GNU updater gate result": "WINDOWS_GNU_UPDATE_RESULT",
        "Windows GNU producer dependency": "needs: [detect-changes, windows-gnu-updater-build]",
        # Regression guard: Universal and fw are independently self-updatable;
        # dropping either artifact, marker, or loop variant must fail VM-009.
        "Windows GNU Universal artifact path": "rust/target/x86_64-pc-windows-gnu/release/FlistWalker.exe",
        "Windows GNU fw artifact path": "rust/target/x86_64-pc-windows-gnu/release/fw.exe",
        "manifest signer public key": "FLISTWALKER_UPDATE_PUBLIC_KEY_HEX: 79b5562e8fe654f94078b112e8a98ba7901f853ae695bed7e0e3910bad049664",
        "test-channel public key": "FLISTWALKER_UPDATE_TEST_CHANNEL",
        "Windows GNU Universal updater variant": "Variant = 'Universal'",
        "Windows GNU fw updater variant": "Variant = 'Fw'",
        "sandbox updater invocation": "-Automated -CleanupSandbox",
        "Universal updater payload marker": "FLISTWALKER_UPDATE_E2E_PAYLOAD_Universal_V1",
        "fw updater payload marker": "FLISTWALKER_UPDATE_E2E_PAYLOAD_Fw_V1",
        "sandbox updater variant argument": "-Variant $case.Variant",
        "distinct updater payload invocation": "-AppPath $artifact -UpdateBinaryPath $updatePayload",
        "Windows GNU caller signing key snapshot": "$callerSigningKey = $env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX",
        "Windows GNU caller signing key preservation": "if ($env:FLISTWALKER_UPDATE_SIGNING_KEY_HEX -cne $callerSigningKey)",
        "audit gate dependency": "      - cargo-audit",
        "lint gate dependency": "      - lint-and-coverage",
        "heavy CI gate input": "HEAVY_CI_REQUIRED: ${{ needs.detect-changes.outputs.heavy_ci_required }}",
        "heavy test skip check": '[[ "$TEST_RESULT" == "skipped" ]]',
        "heavy GNU build skip check": '[[ "$WINDOWS_GNU_BUILD_RESULT" == "skipped" ]]',
        "heavy updater skip check": '[[ "$WINDOWS_GNU_UPDATE_RESULT" == "skipped" ]]',
        "heavy lint skip check": '[[ "$LINT_RESULT" == "skipped" ]]',
    }
    for label, token in required_tokens.items():
        if token not in text:
            violations.append(f"ci-cross-platform.yml: missing {label}")
    for family in ("ubuntu", "windows", "macos"):
        if re.search(rf"\b{family}-[0-9][A-Za-z0-9.-]*\b", text) is None:
            violations.append(f"ci-cross-platform.yml: missing {family} runner generation")
    blocks = dict(_job_blocks(text))
    violations.extend(
        _validate_blocking_clippy_step(
            text,
            workflow_name="ci-cross-platform.yml",
            job_name="rust-test-build",
            step_name="Run platform clippy",
            expected_if="${{ matrix.label != 'linux-native' }}",
        )
    )
    detect_block = blocks.get("detect-changes", "")
    if "fetch-depth: 0" not in detect_block:
        violations.append(
            "ci-cross-platform.yml: change detection must fetch the event base commit"
        )
    heavy_condition = (
        "if: ${{ needs.detect-changes.outputs.heavy_ci_required == 'true' }}"
    )
    for job in ("rust-test-build", "windows-gnu-updater-build", "lint-and-coverage"):
        block = blocks.get(job, "")
        if "needs: detect-changes" not in block or heavy_condition not in block:
            violations.append(
                f"ci-cross-platform.yml: {job} must depend on fail-closed heavy CI detection"
            )
    e2e_block = blocks.get("windows-gnu-update-e2e", "")
    if (
        "needs: [detect-changes, windows-gnu-updater-build]" not in e2e_block
        or heavy_condition not in e2e_block
    ):
        violations.append(
            "ci-cross-platform.yml: Windows GNU updater E2E must depend only on its producer and change detection"
        )
    public_key = (
        "FLISTWALKER_UPDATE_PUBLIC_KEY_HEX: "
        "79b5562e8fe654f94078b112e8a98ba7901f853ae695bed7e0e3910bad049664"
    )
    if public_key not in blocks.get("windows-gnu-updater-build", ""):
        violations.append(
            "ci-cross-platform.yml: Windows GNU producer is missing the manifest signer public key"
        )
    if public_key not in e2e_block:
        violations.append(
            "ci-cross-platform.yml: Windows GNU updater E2E is missing the test public key"
        )
    if "cargo-audit:" not in text or "needs: detect-changes" not in text:
        violations.append("ci-cross-platform.yml: Cargo audit must depend on change detection")
    if text.count("WINDOWS_GNU_UPDATE_RESULT") != 3:
        violations.append(
            "ci-cross-platform.yml: Windows GNU updater gate result must be wired into env and assertion"
        )
    if text.count("WINDOWS_GNU_BUILD_RESULT") != 3:
        violations.append(
            "ci-cross-platform.yml: Windows GNU producer gate result must be wired into env and assertion"
        )
    release_block = text.split("windows-gnu-update-e2e:", 1)[0]
    if "FLISTWALKER_UPDATE_SIGNING_KEY_HEX" in release_block:
        violations.append("ci-cross-platform.yml: test signing key must not enter build jobs")
    return violations


def validate_release_contract(text: str) -> list[str]:
    violations: list[str] = []
    forbidden_test_material = {
        "test-channel compile flag": "FLISTWALKER_UPDATE_TEST_CHANNEL",
        "test-channel public key": "79b5562e8fe654f94078b112e8a98ba7901f853ae695bed7e0e3910bad049664",
        "test-channel signing key": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
    }
    for label, token in forbidden_test_material.items():
        if token in text:
            violations.append(f"release-tagged.yml: contains forbidden {label}")
    violations.extend(
        _validate_blocking_clippy_step(
            text,
            workflow_name="release-tagged.yml",
            job_name="release-preflight",
            step_name="Run clippy",
            expected_if=None,
        )
    )
    required_candidate_contract = {
        "candidate workflow_dispatch trigger": "  workflow_dispatch:",
        "candidate version input": "      version:",
        "candidate required version input": "        required: true",
        "candidate typed version input": "        type: string",
        "candidate release context job": "  release-context:",
        "candidate validated tag resolution": "requested_tag=",
        "candidate default branch context": (
            "DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}"
        ),
        "candidate default branch guard": (
            'if [[ "$REF_NAME" != "$DEFAULT_BRANCH" ]]; then'
        ),
        "candidate bundle assembly job": "  assemble-release-bundle:",
        "candidate artifact upload": "      - name: Upload validated candidate bundle",
        "candidate artifact retention": "          retention-days: 14",
        "draft publication job": "  create-draft-release:",
        "draft push-only condition": (
            "    if: ${{ github.event_name == 'push' && "
            "startsWith(github.ref, 'refs/tags/v') }}"
        ),
    }
    for label, token in required_candidate_contract.items():
        if token not in text:
            violations.append(f"release-tagged.yml: missing {label}")
    dispatch_input = (
        "  workflow_dispatch:\n"
        "    inputs:\n"
        "      version:\n"
        '        description: "Candidate version tag (vX.Y.Z)"\n'
        "        required: true\n"
        "        type: string"
    )
    if dispatch_input not in text:
        violations.append(
            "release-tagged.yml: candidate dispatch input contract is incomplete"
        )
    if (
        '            requested_tag="$INPUT_VERSION"' not in text
        or '            requested_tag="$REF_NAME"' not in text
    ):
        violations.append(
            "release-tagged.yml: candidate/tag release context must resolve both events"
        )

    blocks = _job_blocks(text)
    assembly_blocks = [block for name, block in blocks if name == "assemble-release-bundle"]
    publication_blocks = [block for name, block in blocks if name == "create-draft-release"]
    if len(assembly_blocks) != 1:
        violations.append(
            "release-tagged.yml: candidate bundle assembly job must be unique"
        )
    else:
        assembly = assembly_blocks[0]
        if "contents: write" in assembly or "gh release create" in assembly:
            violations.append(
                "release-tagged.yml: candidate bundle assembly must not publish"
            )
    if len(publication_blocks) != 1:
        violations.append("release-tagged.yml: draft publication job must be unique")
    else:
        publication = publication_blocks[0]
        push_only_condition = (
            "    if: ${{ github.event_name == 'push' && "
            "startsWith(github.ref, 'refs/tags/v') }}"
        )
        if push_only_condition not in publication:
            violations.append(
                "release-tagged.yml: draft publication must be push-tag-only"
            )
        if "contents: write" not in publication or "gh release create" not in publication:
            violations.append(
                "release-tagged.yml: draft publication must own the only release write"
            )
    if text.count("contents: write") != 1 or text.count("gh release create") != 1:
        violations.append(
            "release-tagged.yml: draft publication must be the unique write path"
        )

    required_n_minus_one = {
        "latest published release lookup": (
            'previous_tag="$(gh api "repos/${GITHUB_REPOSITORY}/releases/latest" '
            "--jq .tag_name)\""
        ),
        # Regression guard: validate the contiguous trusted invocation, not loose
        # argument tokens or a compatibility acknowledgement that may occur in
        # unrelated release steps. N-1 incompatibility is always release-blocking.
        "N-1 compatibility checker invocation": (
            "python3 ./scripts/check-updater-n-minus-one-compatibility.py \\\n"
            '            --previous-version "$previous_version" \\\n'
            '            --candidate-version "$candidate_version" \\\n'
            "            --manifest release-bundle/SHA256SUMS"
        ),
    }
    for label, token in required_n_minus_one.items():
        if token not in text:
            violations.append(f"release-tagged.yml: missing {label}")
    for forbidden_bridge in (
        "acknowledge-v0243-manual-update",
        "bridge=()",
        '"${bridge[@]}"',
    ):
        if forbidden_bridge in text:
            violations.append(
                "release-tagged.yml: N-1 compatibility bypass is forbidden"
            )
    return violations


def validate_monitor_issue_contract(name: str, text: str, title: str) -> list[str]:
    violations: list[str] = []
    blocks = _job_blocks(text)
    report_blocks = [block for job, block in blocks if job == "report-failure"]
    recovery_blocks = [
        block for job, block in blocks if job == "resolve-recovered-issue"
    ]
    if len(report_blocks) != 1:
        violations.append(f"{name}: exactly one report-failure job is required")
    if len(recovery_blocks) != 1:
        violations.append(f"{name}: exactly one resolve-recovered-issue job is required")
    if len(report_blocks) != 1 or len(recovery_blocks) != 1:
        return violations

    report = report_blocks[0]
    recovery = recovery_blocks[0]
    needs = MONITOR_NEEDS[name]
    safe_query = (
        '--json number,title,author --jq "[.[] | select(.title == '
        '\\"$TITLE\\" and .author.login == \\"app/github-actions\\")] '
        '| first | .number // empty"'
    )
    default_branch_guard = (
        "github.ref_name == github.event.repository.default_branch"
    )
    def exact_line(value: str) -> str:
        return rf"(?m)^{re.escape(value)}$"

    safe_assignment = (
        '          issue_number="$(gh issue list --repo "$GITHUB_REPOSITORY" '
        '--state open --search "in:title $TITLE" --limit 100 '
        + safe_query
        + ')"'
    )
    issue_only_permissions = (
        r"(?m)^    permissions:\n      issues: write\n    steps:$"
    )
    required_by_job = {
        "report-failure": (
            report,
            {
                "monitor dependency": exact_line(f"    needs: {needs}"),
                "failure-only default-branch condition": exact_line(
                    "    if: ${{ failure() && " + default_branch_guard + " }}"
                ),
                "least-privilege issue permission": issue_only_permissions,
                "exact issue title": exact_line(f'          TITLE: "{title}"'),
                "safe exact-title bot-owner issue assignment": exact_line(
                    safe_assignment
                ),
                "failure issue-number comment target": exact_line(
                    '            gh issue comment "$issue_number" --repo '
                    '"$GITHUB_REPOSITORY" --body "$body"'
                ),
                "failure issue creation": exact_line(
                    '            gh issue create --repo "$GITHUB_REPOSITORY" '
                    '--title "$TITLE" --body "$body"'
                ),
            },
        ),
        "resolve-recovered-issue": (
            recovery,
            {
                "monitor dependency": exact_line(f"    needs: {needs}"),
                "success-only default-branch condition": exact_line(
                    "    if: ${{ success() && " + default_branch_guard + " }}"
                ),
                "least-privilege issue permission": issue_only_permissions,
                "exact issue title": exact_line(f'          TITLE: "{title}"'),
                "safe exact-title bot-owner issue assignment": exact_line(
                    safe_assignment
                ),
                "issue-number recovery close target": exact_line(
                    '            gh issue close "$issue_number" --repo '
                    '"$GITHUB_REPOSITORY" --reason completed '
                    '--comment "Recovered: $RUN_URL"'
                ),
            },
        ),
    }
    for job, (block, required_patterns) in required_by_job.items():
        for label, pattern in required_patterns.items():
            if re.search(pattern, block) is None:
                violations.append(f"{name}: {job} missing {label}")
        if len(re.findall(r"(?m)^\s+issue_number=", block)) != 1:
            violations.append(
                f"{name}: {job} must contain exactly one issue_number assignment"
            )
        if len(re.findall(r"(?m)^    permissions:$", block)) != 1:
            violations.append(
                f"{name}: {job} must contain exactly one permissions block"
            )

    if re.search(r"(?m)^\s+gh issue close ", report):
        violations.append(f"{name}: report-failure must not close issues")
    if len(re.findall(r"(?m)^\s+gh issue close ", recovery)) != 1:
        violations.append(
            f"{name}: resolve-recovered-issue must contain exactly one issue close command"
        )
    if re.search(r"(?m)^\s+gh issue (?:create|comment) ", recovery):
        violations.append(
            f"{name}: resolve-recovered-issue must not create or separately comment on issues"
        )
    return violations


def collect_violations(root: Path) -> list[str]:
    workflows = root / ".github" / "workflows"
    violations: list[str] = []

    workflow_paths = sorted({*workflows.glob("*.yml"), *workflows.glob("*.yaml")})
    existing = {path.name for path in workflow_paths}
    for missing in sorted(REQUIRED_WORKFLOWS - existing):
        violations.append(f"missing workflow: {missing}")

    for path in workflow_paths:
        text = path.read_text(encoding="utf-8")
        violations.extend(validate_workflow(path.name, text))
        if path.name == "ci-cross-platform.yml":
            violations.extend(validate_ci_contract(text))
        elif path.name == "release-tagged.yml":
            violations.extend(validate_release_contract(text))
        elif path.name == "ci-policy-guardian.yml":
            violations.extend(validate_guardian_contract(text))
        elif path.name == "dependabot-auto-merge.yml":
            violations.extend(validate_dependabot_contract(text))
        if path.name in MONITOR_ISSUE_TITLES:
            violations.extend(
                validate_monitor_issue_contract(
                    path.name, text, MONITOR_ISSUE_TITLES[path.name]
                )
            )

        if path.name != "ci-cross-platform.yml" and "name: CI Gate" in text:
            violations.append(f"{path.name}: CI Gate name is reserved")
        if path.name != "ci-policy-guardian.yml" and "name: CI Policy Guardian" in text:
            violations.append(f"{path.name}: CI Policy Guardian name is reserved")

    for required in sorted(REQUIRED_FILES):
        if not (root / required).is_file():
            violations.append(f"missing required file: {required}")

    toolchain = root / "rust" / "rust-toolchain.toml"
    if not toolchain.exists() or f'channel = "{PINNED_RUST}"' not in toolchain.read_text(
        encoding="utf-8"
    ):
        violations.append(f"rust/rust-toolchain.toml: channel must be {PINNED_RUST}")

    audit_config = root / "rust" / ".cargo" / "audit.toml"
    if not audit_config.exists():
        violations.append("rust/.cargo/audit.toml: centralized advisory exceptions are required")

    return violations


def main() -> int:
    trusted_root = Path(__file__).resolve().parents[1]
    if len(sys.argv) == 3 and sys.argv[1] == "--guardian":
        root = Path(sys.argv[2]).resolve()
        violations = validate_trusted_policy_update(trusted_root, root)
        pins, pin_violations = read_proposed_pins(root)
        violations.extend(pin_violations)
        for name, value in pins.items():
            globals()[name] = value
        violations.extend(collect_violations(root))
    elif len(sys.argv) <= 2:
        root = Path(sys.argv[1]).resolve() if len(sys.argv) == 2 else trusted_root
        violations = collect_violations(root)
    else:
        print("usage: check_ci_policy.py [repository-root] | --guardian repository-root")
        return 2
    if violations:
        for violation in violations:
            print(f"ERROR: {violation}")
        return 1
    print("CI policy check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
