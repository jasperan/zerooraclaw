# Required Check Mapping

This document maps the fork's practical merge and post-merge signals.

## PR / merge-time checks

These are the checks that still matter routinely on this fork:

| Check / signal | Source workflow | Scope |
| --- | --- | --- |
| `Workflow Sanity` | `.github/workflows/workflow-sanity.yml` | workflow syntax and lint |
| `CI Change Audit` | `.github/workflows/ci-change-audit.yml` | CI/security workflow policy drift |
| `contributor-tier-consistency` | `.github/workflows/pr-label-policy-check.yml` | label policy sanity |
| path-scoped docs/pages checks | `.github/workflows/docs-deploy.yml`, `.github/workflows/pages-deploy.yml` | docs/site only when matching files changed |

## Post-merge `main` signal

| Signal | Source workflow | Scope |
| --- | --- | --- |
| `Rust Smoke Check` | `.github/workflows/test-e2e.yml` | cheap hosted post-merge sanity build |

## Notes

- The fork intentionally removed several upstream-only workflows instead of keeping them around in a permanently skipped state.
- There is no separate universal heavy Rust/security merge gate on this fork now.
- If workflow names change, update this file so operators know what “normal” looks like.
