# ZeroClaw GitHub Pages Frontend (Vite)

This is the standalone frontend for GitHub Pages.

## Commands

```bash
cd site
npm install
npm run dev
```

Build for GitHub Pages:

```bash
cd site
npm run build
```

Build output is generated at:

```text
<repo-root>/gh-pages
```

Notes:

- Output directory is intentionally `gh-pages/` (not `out/`).
- Vite base path is derived from the current GitHub Pages site metadata when the workflow provides it, or falls back to the current repository slug for local builds.
- Docs source links in UI point to the current repository instead of being hardcoded to the upstream fork source.
- Docs Navigator supports:
  - keyword search with weighted ranking
  - category and level filters (`Core` / `Advanced`)
  - quick keyboard shortcuts: `/` to focus search, `Esc` to reset filters
- "Quick Start Paths" provides task-first doc flows for onboarding, channels, and hardening.
- Command palette is enabled:
  - open via `Ctrl/Cmd + K`
  - includes quick actions (jump docs, repo, theme/language switching)
  - includes direct docs fuzzy search entries
  - supports keyboard navigation (`↑` / `↓` / `Enter`) with active-item highlighting
  - supports `Tab` / `Shift+Tab` cycling and live preview panel (desktop)
- Theme system is enabled:
  - `Auto` / `Dark` / `Light`
  - preference persisted in `localStorage`
- i18n is enabled:
  - UI supports `English` and `简体中文`
  - language preference persisted in `localStorage`
  - URL language parameter (`?lang=en` / `?lang=zh`) is synchronized for shareable links
- Responsive system is deepened:
  - improved breakpoints for desktop/tablet/mobile
  - adaptive topbar controls and panel layouts
  - container query used for doc-card compact mode
  - desktop section rail + mobile quick dock for faster long-page navigation

## Deployment

The repository includes workflow:

```text
.github/workflows/pages-deploy.yml
```

Behavior:

- Trigger on pushes to `main` when `site/**`, `docs/**`, or `README.md` changes.
- Build runs in `site/` and publishes artifact from `gh-pages/`.
- Deploys with GitHub Pages official actions.
