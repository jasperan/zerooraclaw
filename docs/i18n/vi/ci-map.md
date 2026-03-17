# Bản đồ CI Workflow

Tài liệu này chỉ mô tả các workflow còn thực sự hoạt động và hữu ích trên fork `jasperan/zerooraclaw`.

## Workflow cốt lõi đang còn dùng

### PR / workflow hygiene

- **Workflow Sanity (`workflow-sanity.yml`)**
  - Lint file workflow (`actionlint`, tab check)
- **CI/CD Change Audit (`ci-change-audit.yml`)**
  - Kiểm tra thay đổi CI / security policy
- **PR Intake Checks (`pr-intake-checks.yml`)**
  - Intake PR và phản hồi sớm
- **PR Labeler (`pr-labeler.yml`)**
  - Gắn nhãn size / risk / path tự động
- **PR Auto Responder (`pr-auto-response.yml`)**
  - Chào contributor mới và automation theo nhãn
- **Label Policy Sanity (`pr-label-policy-check.yml`)**
  - Kiểm tra tính nhất quán của chính sách label dùng chung

### Tín hiệu cho `main`

- **Main Smoke (`test-e2e.yml`)**
  - Smoke check Rust nhẹ, chạy hosted cho `main`
  - Lệnh: `cargo check --locked --workspace --lib --bins`
- **Deploy GitHub Pages (`pages-deploy.yml`)**
  - Build và publish frontend GitHub Pages
- **Docs Deploy (`docs-deploy.yml`)**
  - Kiểm tra docs + preview/deploy bundle

## Workflow thủ công / theo lịch

Các workflow này vẫn còn trong repo cho nhu cầu vận hành khi thật sự cần:

- `pub-release.yml`
- `pub-prerelease.yml`
- `ci-canary-gate.yml`
- `ci-rollback.yml`
- `ci-supply-chain-provenance.yml`
- `ci-connectivity-probes.yml`
- `nightly-all-features.yml`
- `sec-vorpal-reviewdog.yml`
- `test-benchmarks.yml`
- `test-fuzz.yml`
- `test-rust-build.yml`
- `pr-check-stale.yml`
- `pr-check-status.yml`
- `sync-contributors.yml`

## Triage nhanh

1. Lỗi workflow file: xem `workflow-sanity.yml`.
2. Lỗi policy CI/security: xem `ci-change-audit.yml`.
3. Lỗi docs/site trên `main`: xem `docs-deploy.yml` và `pages-deploy.yml`.
4. Muốn kiểm tra nhanh sau merge: xem `Main Smoke`.
5. Lỗi release: xem `pub-release.yml` / `pub-prerelease.yml`.

> [!IMPORTANT]
> Fork này ưu tiên các kiểm tra hosted ngắn, tránh giữ lại các lane upstream-only nặng mà không giúp ích cho local zerooraclaw usage.
