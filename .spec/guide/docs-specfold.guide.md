---
id: guide:meari/docs-specfold
title: 문서 관리(specfold) 사용법
status: active
created: 2026-10-29
updated: 2026-10-29
---

# 문서 관리(specfold) 사용법

Meari의 설계·결정·계획·근거 문서는 저장소 루트 `.spec/`에서 specfold(`spec` CLI, 0.2.6 이상)로 관리한다. 옛 `docs/*.md` 경로는 `.spec/_aliases.tsv`로 계속 해석된다. 크레이트 README는 패키지 안내로 남긴다.

- 규칙(sot): [[sot:meari/conventions]], [[sot:meari/project-status]], [[sot:meari/architecture-and-support-boundaries]]
- 사용 안내(guide): [[guide:meari/consuming-meari-from-a-rust-application]]
- 코드를 바꾸기 전: `spec context <path>`
- 새 문서: `spec new <type> --title "…"`
- 커밋 전: `spec check --staged` 또는 `cargo xtask spec-check`
