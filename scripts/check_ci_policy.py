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
    "release-tagged.yml",
}
REQUIRED_FILES = {".github/dependabot.yml"}
ALLOWED_WRITE_PERMISSIONS = {
    "ci-canary.yml": {"issues"},
    "dependabot-auto-merge.yml": {"contents", "pull-requests"},
    "release-tagged.yml": {"contents"},
    "security-audit.yml": {"issues"},
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


def _job_blocks(text: str) -> list[tuple[str, str]]:
    marker = "\njobs:\n"
    if marker not in text:
        return []
    jobs_text = text.split(marker, 1)[1]
    return re.findall(
        r"(?ms)^  ([A-Za-z0-9_-]+):\n(.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        jobs_text,
    )


def is_audit_relevant_path(path: str) -> bool:
    return AUDIT_RELEVANT_PATH_RE.search(path) is not None


def audit_result_is_acceptable(cargo_changed: bool, audit_result: str) -> bool:
    expected = "success" if cargo_changed else "skipped"
    return audit_result == expected


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

    for cache_block in re.findall(
        r"(?ms)^\s*- name:.*?\n.*?uses:\s*actions/cache@.*?(?=^\s*- name:|\Z)",
        text,
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
        "auto-merge without bypass": 'gh pr merge "$PR_NUMBER" --repo "$GITHUB_REPOSITORY" --auto --merge',
    }
    for label, token in required_tokens.items():
        if token not in text:
            violations.append(f"dependabot-auto-merge.yml: missing {label}")
    return violations


def validate_ci_contract(text: str) -> list[str]:
    violations: list[str] = []
    required_tokens = {
        "CI Gate job": "name: CI Gate",
        "always aggregation": "if: ${{ always() }}",
        "Cargo change output": "cargo_changed",
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
        "audit gate dependency": "      - cargo-audit",
        "lint gate dependency": "      - lint-and-coverage",
    }
    for label, token in required_tokens.items():
        if token not in text:
            violations.append(f"ci-cross-platform.yml: missing {label}")
    for family in ("ubuntu", "windows", "macos"):
        if re.search(rf"\b{family}-[0-9][A-Za-z0-9.-]*\b", text) is None:
            violations.append(f"ci-cross-platform.yml: missing {family} runner generation")
    if "cargo-audit:" not in text or "needs: detect-changes" not in text:
        violations.append("ci-cross-platform.yml: Cargo audit must depend on change detection")
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
        elif path.name == "ci-policy-guardian.yml":
            violations.extend(validate_guardian_contract(text))
        elif path.name == "dependabot-auto-merge.yml":
            violations.extend(validate_dependabot_contract(text))

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
