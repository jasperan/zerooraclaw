from __future__ import annotations

import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"
UPSTREAM_REPO_GUARD = "github.repository == 'zeroclaw-labs/zeroclaw'"
SAME_REPO_PR_GUARD = "github.event.pull_request.head.repo.full_name == github.repository"

HOSTED_PR_WORKFLOWS = {
    "ci-change-audit.yml",
    "docs-deploy.yml",
    "pages-deploy.yml",
    "pr-auto-response.yml",
    "pr-intake-checks.yml",
    "pr-label-policy-check.yml",
    "pr-labeler.yml",
    "workflow-sanity.yml",
}

UPSTREAM_ONLY_SELF_HOSTED_WORKFLOWS = {
    "ci-build-fast.yml",
    "ci-provider-connectivity.yml",
    "ci-reproducible-build.yml",
    "ci-run.yml",
    "main-promotion-gate.yml",
    "pub-docker-img.yml",
    "sec-codeql.yml",
}


def _indentation(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def _is_meaningful(line: str) -> bool:
    stripped = line.strip()
    return bool(stripped) and not stripped.startswith("#")


def _parse_job_block(lines: list[str], job_indent: int) -> dict[str, str]:
    property_indent: int | None = None
    for line in lines:
        if not _is_meaningful(line):
            continue
        indent = _indentation(line)
        if indent > job_indent:
            property_indent = indent
            break

    if property_indent is None:
        return {}

    job: dict[str, str] = {}
    index = 0
    while index < len(lines):
        line = lines[index]
        if not _is_meaningful(line):
            index += 1
            continue

        indent = _indentation(line)
        if indent != property_indent:
            index += 1
            continue

        key, _, value = line.strip().partition(":")
        key = key.strip()
        value = value.strip()
        if key not in {"if", "runs-on"}:
            index += 1
            continue

        if value in {">", ">-", "|", "|-"}:
            parts: list[str] = []
            index += 1
            while index < len(lines):
                continuation = lines[index]
                if not continuation.strip():
                    index += 1
                    continue
                if _indentation(continuation) <= property_indent:
                    break
                parts.append(continuation.strip())
                index += 1
            job[key] = " ".join(parts)
            continue

        job[key] = value
        index += 1

    return job


def _parse_jobs(text: str) -> dict[str, dict[str, str]]:
    lines = text.splitlines()
    jobs_index: int | None = None
    for index, line in enumerate(lines):
        if line.strip() == "jobs:":
            jobs_index = index
            break

    if jobs_index is None:
        return {}

    jobs_indent = _indentation(lines[jobs_index])
    job_indent: int | None = None
    for line in lines[jobs_index + 1 :]:
        if not _is_meaningful(line):
            continue
        indent = _indentation(line)
        if indent <= jobs_indent:
            return {}
        job_indent = indent
        break

    if job_indent is None:
        return {}

    jobs: dict[str, dict[str, str]] = {}
    index = jobs_index + 1
    while index < len(lines):
        line = lines[index]
        if not _is_meaningful(line):
            index += 1
            continue

        indent = _indentation(line)
        if indent <= jobs_indent:
            break

        stripped = line.strip()
        key, _, value = stripped.partition(":")
        if indent == job_indent and not value.strip() and not stripped.startswith("-"):
            block_start = index + 1
            block_end = block_start
            while block_end < len(lines):
                next_line = lines[block_end]
                if not _is_meaningful(next_line):
                    block_end += 1
                    continue
                next_indent = _indentation(next_line)
                next_stripped = next_line.strip()
                if next_indent <= jobs_indent:
                    break
                if next_indent == job_indent and next_stripped.endswith(":") and not next_stripped.startswith("-"):
                    break
                block_end += 1

            jobs[key.strip()] = _parse_job_block(lines[block_start:block_end], job_indent)
            index = block_end
            continue

        index += 1

    return jobs


class ForkWorkflowPolicyTests(unittest.TestCase):
    def workflow_text(self, filename: str) -> str:
        path = WORKFLOWS_DIR / filename
        return path.read_text(encoding="utf-8")

    def load_workflow(self, filename: str) -> dict[str, dict[str, dict[str, str]]]:
        return {"jobs": _parse_jobs(self.workflow_text(filename))}

    def has_pr_trigger(self, filename: str) -> bool:
        text = self.workflow_text(filename)
        return "pull_request:" in text or "pull_request_target:" in text

    def test_self_hosted_pr_workflow_inventory_is_tracked(self) -> None:
        actual = set()
        for path in sorted(WORKFLOWS_DIR.glob("*.yml")):
            if not self.has_pr_trigger(path.name):
                continue
            workflow = self.load_workflow(path.name)
            if any("self-hosted" in str(job.get("runs-on")) for job in workflow.get("jobs", {}).values()):
                actual.add(path.name)
        self.assertEqual(UPSTREAM_ONLY_SELF_HOSTED_WORKFLOWS, actual)

    def test_hosted_pr_workflows_do_not_require_self_hosted_runners(self) -> None:
        offenders: list[str] = []
        for filename in sorted(HOSTED_PR_WORKFLOWS):
            workflow = self.load_workflow(filename)
            for job_id, job in workflow.get("jobs", {}).items():
                if "self-hosted" in str(job.get("runs-on")):
                    offenders.append(f"{filename}:{job_id}")
        self.assertEqual([], offenders)

    def test_upstream_only_self_hosted_workflows_are_guarded(self) -> None:
        missing_guards: list[str] = []
        for filename in sorted(UPSTREAM_ONLY_SELF_HOSTED_WORKFLOWS):
            workflow = self.load_workflow(filename)
            for job_id, job in workflow.get("jobs", {}).items():
                if "self-hosted" not in str(job.get("runs-on")):
                    continue
                job_if = str(job.get("if") or "")
                needs_same_repo_pr_guard = "github.event_name == 'push'" not in job_if or "pull_request" in job_if
                if UPSTREAM_REPO_GUARD not in job_if:
                    missing_guards.append(f"{filename}:{job_id}")
                    continue
                if needs_same_repo_pr_guard and SAME_REPO_PR_GUARD not in job_if:
                    missing_guards.append(f"{filename}:{job_id}")
        self.assertEqual([], missing_guards)


if __name__ == "__main__":
    unittest.main()
