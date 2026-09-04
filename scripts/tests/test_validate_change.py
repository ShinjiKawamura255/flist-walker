from __future__ import annotations

import importlib.util
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "validate_change.py"
SPEC = importlib.util.spec_from_file_location("validate_change", MODULE_PATH)
assert SPEC and SPEC.loader
VALIDATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATE)


class ValidateChangeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.rules = VALIDATE.load_rules(ROOT / "scripts" / "validation-rules.json")

    def test_classifies_docs_and_specialized_code_paths(self) -> None:
        cases = {
            "docs/INDEX.md": {"VM-001"},
            "rust/src/app/render.rs": {"VM-002"},
            "rust/src/indexer/walker.rs": {"VM-003"},
            "rust/src/query.rs": {"VM-004"},
            "rust/src/updater.rs": {"VM-005"},
            "scripts/gui-headful-smoke.sh": {"VM-006"},
            ".github/ISSUE_TEMPLATE/bug_report.yml": {"VM-007"},
            "rust/src/runtime_config.rs": {"VM-008"},
            "skills/flistwalker-pr-lifecycle/SKILL.md": {"VM-009"},
            "rust/src/app/tests/stateful_endurance/harness.rs": {"VM-010"},
            "rust/Cargo.toml": {"VM-005", "VM-009"},
            "rust/Cargo.lock": {"VM-005", "VM-009"},
            ".github/workflows/ci-cross-platform.yml": {"VM-006", "VM-009"},
            "THIRD_PARTY_NOTICES.txt": {"VM-001", "VM-005"},
            "skills/flistwalker-release-preflight/SKILL.md": {"VM-005"},
            "rust/src/process_entry.rs": {"VM-005"},
            "rust/src/windows_console.rs": {"VM-005"},
            "rust/src/gui_launch.rs": {"VM-005"},
            "rust/src/launch_path.rs": {"VM-005"},
            "rust/src/search/config.rs": {"VM-008"},
            "rust/src/app/index_worker.rs": {"VM-003", "VM-008"},
            "rust/src/app/shell_support.rs": {"VM-002", "VM-008"},
            "rust/src/app/session.rs": {"VM-002", "VM-008"},
            "rust/src/updater.rs": {"VM-005", "VM-008"},
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                selected = {item["id"] for item in VALIDATE.classify_paths([path], self.rules)}
                self.assertTrue(expected <= selected, selected)

    def test_unknown_non_document_path_fails_closed_to_general_code(self) -> None:
        selected = {
            item["id"]
            for item in VALIDATE.classify_paths(["tools/new-unknown-file.xyz"], self.rules)
        }
        self.assertEqual({"VM-002"}, selected)

    def test_rename_diff_includes_old_and_new_paths(self) -> None:
        rows = "R100\tdocs/old.md\tdocs/new.md\nM\trust/src/query.rs\n"

        paths = VALIDATE.parse_name_status(rows)

        self.assertEqual(["docs/old.md", "docs/new.md", "rust/src/query.rs"], paths)

    def test_plan_is_stable_and_contains_detail_links(self) -> None:
        plan = VALIDATE.build_plan(
            ["rust/src/query.rs", "docs/SPEC.md"], self.rules
        )

        self.assertEqual(["VM-001", "VM-004"], [item["id"] for item in plan["validations"]])
        self.assertEqual(sorted(plan["changed_paths"]), plan["changed_paths"])
        self.assertTrue(
            all(item["detail"].startswith("docs/testplan/validation/") for item in plan["validations"])
        )
        self.assertTrue(
            all(
                item["checklists"]
                and all(
                    checklist.startswith("docs/testplan/validation-matrix.md#")
                    for checklist in item["checklists"]
                )
                for item in plan["validations"]
            )
        )

    def test_rules_have_all_vm_ids_and_existing_detail_documents(self) -> None:
        expected = {f"VM-{number:03d}" for number in range(1, 11)}

        self.assertEqual(expected, {rule["id"] for rule in self.rules})
        self.assertTrue(
            all((ROOT / rule["detail"]).is_file() for rule in self.rules)
        )
        self.assertTrue(all(rule["checklists"] for rule in self.rules))

    def test_rule_checklist_anchors_exist(self) -> None:
        anchors_by_path: dict[str, set[str]] = {}
        for rule in self.rules:
            for pointer in rule["checklists"]:
                relative, anchor = pointer.split("#", 1)
                if relative not in anchors_by_path:
                    text = (ROOT / relative).read_text(encoding="utf-8-sig")
                    headings = re.findall(r"^#{1,6}\s+(.+?)\s*$", text, re.MULTILINE)
                    anchors_by_path[relative] = {
                        re.sub(r"\s+", "-", re.sub(r"[^\w\s-]", "", heading.lower())).strip("-")
                        for heading in headings
                    }
                self.assertIn(anchor, anchors_by_path[relative], pointer)


if __name__ == "__main__":
    unittest.main()
