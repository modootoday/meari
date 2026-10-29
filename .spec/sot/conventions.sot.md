---
id: sot:meari/conventions
title: Meari 코드 구조·네이밍 컨벤션
status: active
created: 2026-10-29
updated: 2026-10-29
verify: cargo xtask lint-structure --strict
---

# Meari 코드 구조·네이밍 컨벤션

## Rule

- 우선순위: Rust/Cargo 표준 > 4원칙(단일책임, 플랫파일나열금지, 모듈화, 맥락기반 디렉토리) > 네이밍 > AGENTS.md 운영 규칙. 충돌하면 표준이 이긴다.
- 네이밍: 환경변수 `MEARI_*`, 바이너리 `meari`(하위 명령 `convert|native|build`).
- 이 규칙은 kinekino 저장소의 `sot:kinekino/conventions`와 같은 원칙을 Meari에 적용한 것이다. 저장소가 달라 링크 대신 요약만 둔다. 세부 목표 구조는 재설계 계획에 있다.

## Why

사용자 지시(2026-10-29): Meari도 kinekino와 같은 컨벤션으로 리팩터링한다.

## Verification

`cargo xtask lint-structure --strict`(플랫 나열·`x.rs`/`x/` 공존·금지 이름). 네이밍과 SRP 일부는 사람이 검토한다.
