# Main Branch Delivery Flows

This document explains the practical delivery flow for `jasperan/zerooraclaw`.

Use this with:

- [`docs/ci-map.md`](../../docs/ci-map.md)
- [`docs/pr-workflow.md`](../../docs/pr-workflow.md)
- [`docs/release-process.md`](../../docs/release-process.md)

## Fork Policy

This fork keeps the routine path lean.

- PR automation stays on hosted GitHub checks.
- `main` gets a cheap hosted Rust smoke check.
- Pages/docs deploy lanes stay active.
- heavy upstream-only lanes were removed instead of being left around to skip forever.

## Event Summary

| Event | Main workflows |
| --- | --- |
| PR activity (`pull_request_target`) | `pr-intake-checks.yml`, `pr-labeler.yml`, `pr-auto-response.yml` |
| PR activity (`pull_request`) | `workflow-sanity.yml`, `ci-change-audit.yml`, `pr-label-policy-check.yml`, plus path-scoped docs/pages checks |
| Push to `main` | `test-e2e.yml` (`Main Smoke`), `docs-deploy.yml`, `pages-deploy.yml`, plus path-scoped governance checks |
| Tag push (`v*`) | `pub-release.yml`, `pub-prerelease.yml` as applicable |
| Scheduled/manual | maintenance workflows such as `ci-canary-gate.yml`, `ci-rollback.yml`, `ci-connectivity-probes.yml`, `nightly-all-features.yml`, `sec-vorpal-reviewdog.yml`, `test-benchmarks.yml`, `test-fuzz.yml`, `test-rust-build.yml`, `sync-contributors.yml` |

## Practical Flow

### PRs

1. `pull_request_target` automation handles intake, labels, and canned responses.
2. Hosted governance workflows (`workflow-sanity`, `ci-change-audit`, `pr-label-policy-check`) run when their file scopes match.
3. Docs/pages checks run only when relevant files changed.

### Merge to `main`

1. The merge lands on `main`.
2. `Main Smoke` runs with:
   - `cargo check --locked --workspace --lib --bins`
3. Docs/pages deploy lanes run when matching files changed.
4. No removed upstream-only CI lanes are expected to appear.

### Release / maintenance

Use the manual/scheduled workflows only when you actually need them.

## Quick Troubleshooting

1. If PR automation looks stale, remember `pull_request_target` reads from the default branch.
2. If `main` feels slow, inspect path-scoped docs/pages workflows first.
3. If you need deeper confidence than `Main Smoke`, use local commands or a manual maintenance workflow intentionally.
4. If release artifacts are missing, check the release workflows directly.
