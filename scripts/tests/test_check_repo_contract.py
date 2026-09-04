from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check_repo_contract.py"
SPEC = importlib.util.spec_from_file_location("check_repo_contract", MODULE_PATH)
assert SPEC and SPEC.loader
CONTRACT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONTRACT)


class RepoContractTests(unittest.TestCase):
    def write(self, root: Path, relative: str, text: str) -> None:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def minimal_repo(self, root: Path) -> None:
        self.write(root, "AGENTS.md", "# Agents\n\nSee [docs](docs/INDEX.md).\n")
        self.write(root, "README.md", "# Readme\n")
        self.write(root, "README-ja.md", "# Readme JA\n")
        self.write(root, "docs/INDEX.md", "# Index\n")
        self.write(
            root,
            "docs/CURRENT_STATUS.md",
            "# Current Status\n\nThis document does not claim validation for the current HEAD.\n",
        )

    def test_missing_local_markdown_link_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.minimal_repo(root)
            self.write(root, "docs/INDEX.md", "# Index\n\n[missing](missing.md)\n")

            violations = CONTRACT.collect_violations(root)

            self.assertTrue(any("missing local link target" in item for item in violations))

    def test_local_markdown_link_cannot_escape_repository(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "repo"
            root.mkdir()
            self.minimal_repo(root)
            self.write(
                root,
                "docs/INDEX.md",
                "# Index\n\n[absolute](/definitely/missing.md)\n"
                "[traversal](../../outside.md)\n",
            )

            violations = CONTRACT.collect_violations(root)

            escaped = [item for item in violations if "outside repository" in item]
            self.assertEqual(2, len(escaped), escaped)

    def test_missing_local_markdown_heading_fragment_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.minimal_repo(root)
            self.write(
                root,
                "docs/INDEX.md",
                "# Index\n\n[missing section](guide.md#missing-section)\n",
            )
            self.write(root, "docs/guide.md", "# Guide\n\n## Existing section\n")

            violations = CONTRACT.collect_violations(root)

            self.assertTrue(any("missing local link fragment" in item for item in violations))

    def test_current_status_rejects_transient_evidence_and_current_head_claim(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.minimal_repo(root)
            self.write(
                root,
                "docs/CURRENT_STATUS.md",
                "# Current Status\n\n## Current HEAD Validation\n"
                "Evidence: `rust/target/gui-smoke/evidence/run.local.md`.\n",
            )

            violations = CONTRACT.collect_violations(root)

            self.assertTrue(any("Current HEAD Validation" in item for item in violations))
            self.assertTrue(any("transient rust/target evidence" in item for item in violations))

    def test_current_status_rejects_raw_commit_sha(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.minimal_repo(root)
            self.write(
                root,
                "docs/CURRENT_STATUS.md",
                "# Current Status\n\nLast checked at `0123456789abcdef0123456789abcdef01234567`.\n",
            )

            violations = CONTRACT.collect_violations(root)

            self.assertTrue(any("raw commit SHA" in item for item in violations))

    def test_skill_name_and_references_are_validated(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.minimal_repo(root)
            self.write(
                root,
                "skills/example/SKILL.md",
                "---\nname: wrong-name\ndescription: Example.\n---\n\n"
                "# Example\n\nRead [rules](references/rules.md).\n",
            )

            violations = CONTRACT.collect_violations(root)

            self.assertTrue(any("skill name" in item for item in violations))
            self.assertTrue(any("missing local link target" in item for item in violations))

    def test_long_agent_entrypoint_line_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.minimal_repo(root)
            self.write(root, "AGENTS.md", "# Agents\n" + "x" * 501 + "\n")

            violations = CONTRACT.collect_violations(root)

            self.assertTrue(any("line exceeds 500 characters" in item for item in violations))

    def test_skill_over_500_lines_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.minimal_repo(root)
            body = "---\nname: example\ndescription: Example.\n---\n" + "line\n" * 497
            self.write(root, "skills/example/SKILL.md", body)

            violations = CONTRACT.collect_violations(root)

            self.assertTrue(any("exceeds 500 lines" in item for item in violations))

    def test_skill_at_500_lines_is_allowed(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.minimal_repo(root)
            body = "---\nname: example\ndescription: Example.\n---\n" + "line\n" * 496
            self.write(root, "skills/example/SKILL.md", body)

            violations = CONTRACT.collect_violations(root)

            self.assertFalse(any("exceeds 500 lines" in item for item in violations))

    def test_current_repository_satisfies_contract(self) -> None:
        self.assertEqual([], CONTRACT.collect_violations(ROOT))


if __name__ == "__main__":
    unittest.main()
