# CI Workflow Map

This document describes the workflows that still matter on the `jasperan/zerooraclaw` fork.

The fork intentionally dropped a chunk of upstream-only or permanently skipped CI so the normal path stays small and local-friendly.

## Active Routine Workflows

### PR / push hygiene

- `.github/workflows/workflow-sanity.yml`
  - Purpose: lint workflow files (`actionlint`, tab checks)
  - Trigger: PR/push when workflow files change
- `.github/workflows/ci-change-audit.yml`
  - Purpose: audit CI/security workflow changes for unpinned actions, risky permissions, and similar policy drift
  - Trigger: PR/push when CI/security files change
- `.github/workflows/pr-intake-checks.yml`
  - Purpose: intake validation and early sticky feedback on PRs
  - Trigger: `pull_request_target`
- `.github/workflows/pr-labeler.yml`
  - Purpose: path/size/risk labels
  - Trigger: `pull_request_target`
- `.github/workflows/pr-auto-response.yml`
  - Purpose: first-interaction and label-driven automation
  - Trigger: `issues`, `pull_request_target`
- `.github/workflows/pr-label-policy-check.yml`
  - Purpose: validate shared label policy wiring
  - Trigger: PR/push when label policy automation files change

### Main branch confidence lanes

- `.github/workflows/test-e2e.yml` (`Main Smoke`)
  - Purpose: cheap hosted Rust smoke check for `main`
  - Command: `cargo check --locked --workspace --lib --bins`
  - Trigger: push to `main`, manual dispatch
- `.github/workflows/pages-deploy.yml`
  - Purpose: build and publish the GitHub Pages frontend
  - Trigger: docs/site/README/workflow changes, manual dispatch
- `.github/workflows/docs-deploy.yml`
  - Purpose: docs quality + preview/deploy bundle
  - Trigger: docs/README/workflow changes, manual dispatch

## Active Manual / Scheduled Workflows

These remain in the repo for occasional operations work:

- `.github/workflows/pub-release.yml`
- `.github/workflows/pub-prerelease.yml`
- `.github/workflows/ci-canary-gate.yml`
- `.github/workflows/ci-rollback.yml`
- `.github/workflows/ci-supply-chain-provenance.yml`
- `.github/workflows/ci-connectivity-probes.yml`
- `.github/workflows/nightly-all-features.yml`
- `.github/workflows/sec-vorpal-reviewdog.yml`
- `.github/workflows/test-benchmarks.yml`
- `.github/workflows/test-fuzz.yml`
- `.github/workflows/test-rust-build.yml`
- `.github/workflows/pr-check-stale.yml`
- `.github/workflows/pr-check-status.yml`
- `.github/workflows/sync-contributors.yml`

## Practical Trigger Map

- PR automation: `pr-intake-checks.yml`, `pr-labeler.yml`, `pr-auto-response.yml`
- PR/push workflow governance: `workflow-sanity.yml`, `ci-change-audit.yml`, `pr-label-policy-check.yml`
- Push to `main`: `Main Smoke`, `Deploy GitHub Pages`, `Docs Deploy` (path-scoped)
- Release / maintenance: manual or scheduled workflows listed above

## Fast Triage Guide

1. Workflow files fail lint: check `workflow-sanity.yml`.
2. CI/security policy complaint: check `ci-change-audit.yml`.
3. Docs/site problem on `main`: check `docs-deploy.yml` and `pages-deploy.yml`.
4. Local confidence after merge: check `Main Smoke`.
5. Release trouble: check `pub-release.yml` / `pub-prerelease.yml`.

## Notes

- This fork favors short hosted checks over long self-hosted lanes.
- If a heavy workflow becomes useful again, add it back deliberately instead of keeping it around in a permanently skipped state.
- Translated CI docs may lag this file. Treat this English map as the current fork source of truth.
