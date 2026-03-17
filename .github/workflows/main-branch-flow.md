# Main Branch Delivery Flows

This document explains the practical delivery flow for this fork.

Use this with:

- [`docs/ci-map.md`](../../docs/ci-map.md)
- [`docs/pr-workflow.md`](../../docs/pr-workflow.md)
- [`docs/release-process.md`](../../docs/release-process.md)

## Fork Override for `jasperan/zerooraclaw`

This fork keeps the normal PR/push path intentionally lean.

- `sec-audit.yml` and `feature-matrix.yml` are not part of the routine PR/push hot path here.
- `.github/workflows/test-e2e.yml` was repurposed into a cheap hosted `Main Smoke` check on `main` push + manual dispatch.
- `pull_request_target` workflows still execute from the default branch definition, so automation changes only take effect after they land on `main`.

## Event Summary

| Event | Main workflows |
| --- | --- |
| PR activity (`pull_request_target`) | `pr-intake-checks.yml`, `pr-labeler.yml`, `pr-auto-response.yml` |
| PR activity (`pull_request`) | `ci-run.yml`, `main-promotion-gate.yml` (for `main` PRs), plus path-scoped hosted workflows |
| Push to `main` | `test-e2e.yml` (`Main Smoke`), `docs-deploy.yml`, `pages-deploy.yml`, and other path-scoped hosted workflows |
| Tag push (`v*`) | `pub-release.yml` publish mode, `pub-docker-img.yml` publish job |
| Scheduled/manual | `pub-release.yml`, `sec-codeql.yml`, `sec-audit.yml`, `feature-matrix.yml`, `test-fuzz.yml`, `pr-check-stale.yml`, `pr-check-status.yml`, `sync-contributors.yml`, `test-benchmarks.yml`, and manual `test-e2e.yml` smoke |

## Practical Flow

### PRs

1. `pull_request_target` automation handles intake, labels, and canned responses.
2. `pull_request` workflows handle the normal CI and path-scoped checks.
3. Heavy matrix/security maintenance lanes are intentionally absent from the routine PR hot path on this fork.

### Merge to `main`

1. The merge lands on `main`.
2. `ci-run.yml` is still triggerable on `main`, but on this fork it remains an upstream-only guarded lane and skips.
3. `test-e2e.yml` runs as a cheap hosted smoke check using `cargo check --locked --workspace --lib --bins`.
4. Docs/pages and other path-scoped workflows run when matching files changed.
5. Heavy maintenance workflows remain manual or scheduled.

### Maintenance Lanes

Use these outside the normal hot path:

- `feature-matrix.yml` for upstream-only matrix compile/nightly sweeps
- `sec-audit.yml` for upstream-only security baseline audits
- `sec-codeql.yml` for static analysis
- `pub-release.yml` / `pub-docker-img.yml` for release publishing

## Quick Troubleshooting

1. If PR automation looks stale, remember `pull_request_target` reads from the default branch.
2. If `main` feels slow, check whether a path-scoped workflow, not the smoke lane, is the culprit.
3. If you need deeper confidence than the cheap smoke check on this fork, use equivalent local commands or intentionally remove the upstream-only guard before trying to run `feature-matrix.yml` or `sec-audit.yml` here.
4. If Docker artifacts are missing, confirm a `v*` tag push happened for publish lanes.
