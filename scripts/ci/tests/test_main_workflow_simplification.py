from __future__ import annotations

import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"


class MainWorkflowSimplificationTests(unittest.TestCase):
    def workflow_text(self, filename: str) -> str:
        return (WORKFLOWS_DIR / filename).read_text(encoding="utf-8")

    def block_after(self, text: str, marker: str) -> str:
        _, _, tail = text.partition(marker)
        return tail

    def test_ci_run_does_not_fire_on_main_push(self) -> None:
        text = self.workflow_text("ci-run.yml")
        push_block = self.block_after(text, "push:\n")
        push_section = push_block.split("pull_request:\n", 1)[0]
        self.assertIn("branches: [dev]", push_section)
        self.assertNotIn("branches: [dev, main]", push_section)

        pull_request_block = self.block_after(text, "pull_request:\n")
        pull_request_section = pull_request_block.split("merge_group:\n", 1)[0]
        self.assertIn("branches: [dev, main]", pull_request_section)

        merge_group_block = self.block_after(text, "merge_group:\n")
        merge_group_section = merge_group_block.split("\n\nconcurrency:", 1)[0]
        self.assertIn("branches: [dev]", merge_group_section)
        self.assertNotIn("branches: [dev, main]", merge_group_section)

    def test_ci_build_fast_does_not_fire_on_main_push(self) -> None:
        text = self.workflow_text("ci-build-fast.yml")
        push_block = self.block_after(text, "push:\n")
        push_section = push_block.split("pull_request:\n", 1)[0]
        self.assertIn("branches: [dev]", push_section)
        self.assertNotIn("branches: [dev, main]", push_section)

    def test_feature_matrix_is_manual_or_scheduled_only(self) -> None:
        text = self.workflow_text("feature-matrix.yml")
        self.assertIn("schedule:", text)
        self.assertIn("workflow_dispatch:", text)
        self.assertNotIn("pull_request:", text)
        self.assertNotIn("push:", text)
        self.assertNotIn("merge_group:", text)

    def test_sec_audit_is_manual_or_scheduled_only(self) -> None:
        text = self.workflow_text("sec-audit.yml")
        self.assertIn("schedule:", text)
        self.assertIn("workflow_dispatch:", text)
        self.assertNotIn("pull_request:", text)
        self.assertNotIn("push:", text)
        self.assertNotIn("merge_group:", text)

    def test_main_smoke_is_hosted_and_cheap(self) -> None:
        text = self.workflow_text("test-e2e.yml")
        self.assertIn("push:", text)
        self.assertIn("branches: [main]", text)
        self.assertIn("workflow_dispatch:", text)
        self.assertIn("runs-on: ubuntu-latest", text)
        self.assertIn("cargo check --locked --workspace --lib --bins", text)
        self.assertNotIn("self-hosted", text)
        self.assertNotIn("pull_request:", text)


if __name__ == "__main__":
    unittest.main()
