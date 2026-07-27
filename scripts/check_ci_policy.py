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
    "ci-canary.yml",
    "security-audit.yml",
    "perf-regression.yml",
    "release-tagged.yml",
}
LATEST_RUNNER_RE = re.compile(r"\b(?:ubuntu|windows|macos)-latest\b")
ACTION_RE = re.compile(r"^\s*(?:-\s*)?uses:\s*([^@\s]+)@([^\s#]+)", re.MULTILINE)
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
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


def validate_workflow(name: str, text: str) -> list[str]:
    violations: list[str] = []
    is_canary = name == "ci-canary.yml"

    if "pull_request_target:" in text:
        violations.append(f"{name}: pull_request_target is forbidden")
    if "permissions:" not in text:
        violations.append(f"{name}: explicit permissions are required")
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
    }
    for label, token in required_tokens.items():
        if token not in text:
            violations.append(f"ci-cross-platform.yml: missing {label}")
    if "cargo-audit:" not in text or "needs: detect-changes" not in text:
        violations.append("ci-cross-platform.yml: Cargo audit must depend on change detection")
    return violations


def collect_violations(root: Path) -> list[str]:
    workflows = root / ".github" / "workflows"
    violations: list[str] = []

    existing = {path.name for path in workflows.glob("*.yml")}
    for missing in sorted(REQUIRED_WORKFLOWS - existing):
        violations.append(f"missing workflow: {missing}")

    for path in sorted(workflows.glob("*.yml")):
        text = path.read_text(encoding="utf-8")
        violations.extend(validate_workflow(path.name, text))
        if path.name == "ci-cross-platform.yml":
            violations.extend(validate_ci_contract(text))

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
    root = Path(__file__).resolve().parents[1]
    violations = collect_violations(root)
    if violations:
        for violation in violations:
            print(f"ERROR: {violation}")
        return 1
    print("CI policy check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
