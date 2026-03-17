from __future__ import annotations

import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"

REMOVED_UPSTREAM_ONLY_WORKFLOWS = {
    "ci-build-fast.yml",
    "ci-provider-connectivity.yml",
    "ci-reproducible-build.yml",
    "ci-run.yml",
    "feature-matrix.yml",
    "main-promotion-gate.yml",
    "pub-docker-img.yml",
    "sec-audit.yml",
    "sec-codeql.yml",
}


class MainWorkflowSimplificationTests(unittest.TestCase):
    def workflow_text(self, filename: str) -> str:
        return (WORKFLOWS_DIR / filename).read_text(encoding="utf-8")

    def test_removed_upstream_only_workflows_are_absent(self) -> None:
        present = [
            name for name in sorted(REMOVED_UPSTREAM_ONLY_WORKFLOWS)
            if (WORKFLOWS_DIR / name).exists()
        ]
        self.assertEqual([], present)

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
