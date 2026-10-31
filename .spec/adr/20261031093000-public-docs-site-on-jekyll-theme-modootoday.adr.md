---
id: adr:meari/20261031093000
title: "Public docs site on jekyll-theme-modootoday"
status: draft
created: 2026-10-31T09:30:00Z
updated: 2026-10-31T09:45:00Z
rel:
  governs: [docs]
---

# Public docs site on jekyll-theme-modootoday

## Context

Meari's public explanation lived only in README.md. Readers (first-time Rust developers, people evaluating Meari, contributors) need a site written for them, in English.

## Decision

- `docs/` is a Jekyll site using `remote_theme: modootoday/jekyll-theme-modootoday@v1.4.1` on the classic GitHub Pages stack (`github-pages` 232). Bodies are plain GFM; `_site/` and caches are ignored.
- Pages: home, get started, using Meari in a Rust app, the crates, how Meari is tested, contributing, changelog. English only.
- Facts come from Cargo manifests, README.md, CHANGELOG.md, the CI workflow and public `.spec` documents; pages are written for readers, not copied from decision records.
- Every page has a Markdown twin stub (`<page>.md.txt`, served at `<page>/index.md`), and `llms.txt` / `llms-full.txt` use the kit's `llms` and `llms-full` layouts, so readers and AI tools get the source text, including the crates page's `packages:` table.
- `modootoday-check . _site --twins --strict` must pass.
- Deployment: GitHub Pages classic build from `main` branch, `/docs` path, on the default address `https://modootoday.github.io/meari/` (`url: https://modootoday.github.io`, `baseurl: /meari`). Every push to `main` that changes `docs/` republishes the site; no custom domain.
