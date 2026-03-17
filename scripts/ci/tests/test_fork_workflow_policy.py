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

PRIMARY_DOCS = {
    '.github/workflows/main-branch-flow.md',
    'docs/ci-map.md',
    'docs/operations/required-check-mapping.md',
    'docs/operations-runbook.md',
    'docs/cargo-slicer-speedup.md',
    'docs/audit-event-schema.md',
    'docs/operations/README.md',
}


class ForkWorkflowPolicyTests(unittest.TestCase):
    def test_upstream_only_workflow_files_are_removed(self) -> None:
        present = [
            name for name in sorted(REMOVED_UPSTREAM_ONLY_WORKFLOWS)
            if (WORKFLOWS_DIR / name).exists()
        ]
        self.assertEqual([], present)

    def test_no_pr_triggered_self_hosted_workflows_remain(self) -> None:
        offenders: list[str] = []
        for path in sorted(WORKFLOWS_DIR.glob("*.yml")):
            text = path.read_text(encoding="utf-8")
            if ("pull_request:" in text or "pull_request_target:" in text) and "self-hosted" in text:
                offenders.append(path.name)
        self.assertEqual([], offenders)

    def test_primary_docs_do_not_reference_removed_workflows(self) -> None:
        offenders: list[str] = []
        for relpath in sorted(PRIMARY_DOCS):
            text = (REPO_ROOT / relpath).read_text(encoding="utf-8")
            for workflow in sorted(REMOVED_UPSTREAM_ONLY_WORKFLOWS):
                if workflow in text:
                    offenders.append(f"{relpath}:{workflow}")
        self.assertEqual([], offenders)


if __name__ == "__main__":
    unittest.main()
