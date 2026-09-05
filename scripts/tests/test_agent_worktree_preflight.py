from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "agent_worktree_preflight.py"
SPEC = importlib.util.spec_from_file_location("agent_worktree_preflight", MODULE_PATH)
assert SPEC and SPEC.loader
PREFLIGHT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PREFLIGHT)


class AgentWorktreePreflightTests(unittest.TestCase):
    def git(self, root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *arguments],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
        )

    def initialized_remote(self, temp: Path) -> tuple[Path, Path]:
        remote = temp / "origin.git"
        seed = temp / "seed"
        self.git(temp, "init", "--bare", str(remote))
        self.git(temp, "init", "--initial-branch=master", str(seed))
        self.git(seed, "config", "user.name", "Test Agent")
        self.git(seed, "config", "user.email", "agent@example.invalid")
        (seed / "README.md").write_text("fixture\n", encoding="utf-8")
        self.git(seed, "add", "README.md")
        self.git(seed, "commit", "-m", "fixture")
        self.git(seed, "remote", "add", "origin", str(remote))
        self.git(seed, "push", "-u", "origin", "master")
        self.git(remote, "symbolic-ref", "HEAD", "refs/heads/master")
        return remote, seed

    def state(self, **overrides: object):
        values = {
            "root": "/repo",
            "current_worktree": "/repo",
            "head": "a" * 40,
            "branch": None,
            "dirty": False,
            "refs": {"origin/master": "a" * 40, "origin/codex/parent": "a" * 40},
            "worktrees": {"/repo": None, "/primary": "codex/parent"},
            "local_branches": {"codex/parent": "a" * 40},
            "remote_branches": {"origin/codex/parent": "a" * 40},
        }
        values.update(overrides)
        return PREFLIGHT.RepositoryState(**values)

    def test_review_allows_clean_detached_head(self) -> None:
        result = PREFLIGHT.evaluate(self.state(), mode="review")
        self.assertTrue(result.ok, result.reasons)

    def test_dirty_state_fails_closed(self) -> None:
        result = PREFLIGHT.evaluate(self.state(dirty=True), mode="review")
        self.assertFalse(result.ok)
        self.assertIn("worktree_is_dirty", result.reasons)

    def test_new_change_requires_base_identity_and_unused_target(self) -> None:
        ok = PREFLIGHT.evaluate(
            self.state(
                branch="master",
                worktrees={"/repo": "master", "/primary": "codex/parent"},
                local_branches={"master": "a" * 40, "codex/parent": "a" * 40},
            ),
            mode="new-change",
            base_ref="origin/master",
            target_branch="codex/new",
        )
        self.assertTrue(ok.ok, ok.reasons)

        divergent = PREFLIGHT.evaluate(
            self.state(head="b" * 40),
            mode="new-change",
            base_ref="origin/master",
            target_branch="codex/new",
        )
        self.assertIn("head_does_not_match_base", divergent.reasons)

    def test_new_change_rejects_detached_or_different_current_branch(self) -> None:
        detached = PREFLIGHT.evaluate(
            self.state(),
            mode="new-change",
            base_ref="origin/master",
            target_branch="codex/new",
        )
        other_branch = PREFLIGHT.evaluate(
            self.state(
                branch="codex/other",
                worktrees={"/repo": "codex/other"},
                local_branches={"codex/other": "a" * 40},
            ),
            mode="new-change",
            base_ref="origin/master",
            target_branch="codex/new",
        )

        self.assertIn("current_branch_does_not_match_base", detached.reasons)
        self.assertIn("current_branch_does_not_match_base", other_branch.reasons)

    def test_new_change_rejects_non_master_base_even_when_identity_matches(self) -> None:
        result = PREFLIGHT.evaluate(
            self.state(
                branch="codex/base",
                refs={"origin/codex/base": "a" * 40},
                worktrees={"/repo": "codex/base"},
                local_branches={"codex/base": "a" * 40},
                remote_branches={"origin/codex/base": "a" * 40},
            ),
            mode="new-change",
            base_ref="origin/codex/base",
            target_branch="codex/new",
        )

        self.assertFalse(result.ok)
        self.assertIn("new_change_base_must_be_origin_master", result.reasons)

    def test_continue_pr_rejects_head_branch_owned_by_other_worktree(self) -> None:
        result = PREFLIGHT.evaluate(
            self.state(), mode="continue-pr", head_ref="origin/codex/parent"
        )

        self.assertFalse(result.ok)
        self.assertIn("head_branch_owned_by_other_worktree", result.reasons)

    def test_continue_pr_requires_expected_branch_in_current_worktree(self) -> None:
        result = PREFLIGHT.evaluate(
            self.state(
                branch="codex/unrelated",
                worktrees={"/repo": "codex/unrelated"},
            ),
            mode="continue-pr",
            head_ref="origin/codex/parent",
        )

        self.assertFalse(result.ok)
        self.assertIn("current_branch_does_not_match_pr", result.reasons)

    def test_continue_pr_accepts_expected_branch_owned_here(self) -> None:
        result = PREFLIGHT.evaluate(
            self.state(
                branch="codex/parent",
                worktrees={"/repo": "codex/parent"},
            ),
            mode="continue-pr",
            head_ref="origin/codex/parent",
        )

        self.assertTrue(result.ok, result.reasons)

    def test_stacked_change_allows_occupied_parent_but_requires_unused_target(self) -> None:
        result = PREFLIGHT.evaluate(
            self.state(),
            mode="stacked-change",
            parent_ref="origin/codex/parent",
            target_branch="codex/child",
        )
        self.assertTrue(result.ok, result.reasons)

        occupied = PREFLIGHT.evaluate(
            self.state(local_branches={"codex/child": "a" * 40}),
            mode="stacked-change",
            parent_ref="origin/codex/parent",
            target_branch="codex/child",
        )
        self.assertIn("target_branch_already_exists", occupied.reasons)

    def test_cli_continue_pr_requires_branch_identity_in_disposable_repo(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            remote, _ = self.initialized_remote(temp)
            repo = temp / "repo"
            self.git(temp, "clone", str(remote), str(repo))
            self.git(repo, "switch", "-c", "codex/pr")
            self.git(repo, "push", "-u", "origin", "codex/pr")

            accepted = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--root",
                    str(repo),
                    "continue-pr",
                    "--head-ref",
                    "origin/codex/pr",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(0, accepted.returncode, accepted.stdout + accepted.stderr)

            self.git(repo, "switch", "-c", "codex/unrelated")
            rejected = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--root",
                    str(repo),
                    "continue-pr",
                    "--head-ref",
                    "origin/codex/pr",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(1, rejected.returncode)
            self.assertIn("current_branch_does_not_match_pr", rejected.stdout)

    def test_cli_new_change_requires_current_master_in_disposable_repo(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            remote, _ = self.initialized_remote(temp)
            repo = temp / "repo"
            self.git(temp, "clone", str(remote), str(repo))

            accepted = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--root",
                    str(repo),
                    "new-change",
                    "--base-ref",
                    "origin/master",
                    "--target-branch",
                    "codex/new",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(0, accepted.returncode, accepted.stdout + accepted.stderr)

            self.git(repo, "switch", "--detach")
            detached = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--root",
                    str(repo),
                    "new-change",
                    "--base-ref",
                    "origin/master",
                    "--target-branch",
                    "codex/new",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(1, detached.returncode)
            self.assertIn("current_branch_does_not_match_base", detached.stdout)

            self.git(repo, "switch", "-c", "codex/other")
            other_branch = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--root",
                    str(repo),
                    "new-change",
                    "--base-ref",
                    "origin/master",
                    "--target-branch",
                    "codex/new",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(1, other_branch.returncode)
            self.assertIn("current_branch_does_not_match_base", other_branch.stdout)

    def test_cli_stacked_change_allows_parent_owned_by_other_worktree(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            remote, _ = self.initialized_remote(temp)
            primary = temp / "primary"
            secondary = temp / "secondary"
            self.git(temp, "clone", str(remote), str(primary))
            self.git(primary, "switch", "-c", "codex/parent")
            self.git(primary, "push", "-u", "origin", "codex/parent")
            self.git(primary, "worktree", "add", "--detach", str(secondary), "origin/codex/parent")

            result = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--root",
                    str(secondary),
                    "stacked-change",
                    "--parent-ref",
                    "origin/codex/parent",
                    "--target-branch",
                    "codex/child",
                ],
                check=False,
                capture_output=True,
                text=True,
            )

            self.assertEqual(0, result.returncode, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
