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
- 빌드 산출물 관리(디스크, 사용자 승인 2026-10-29): 루트 `Cargo.toml`의 `[profile.dev]`·`[profile.test]`는 `debug = "line-tables-only"`, `split-debuginfo = "unpacked"`, `incremental = false`, 의존성(`[profile.dev.package."*"]`)은 `debug = false`. `CARGO_INCREMENTAL=0`은 `runtime/env.sh`, `.cargo/config.toml`(`build.incremental = false`)에 둔다. 빌드는 `-j 1`, `nice`.
- 정리: 검사 사이(빌드 중이 아닐 때) `cargo xtask clean-cache [--max-gb 40] [--older-days 7] [--dry-run]` — `incremental/`, `--older-days`보다 오래 쓰이지 않은 `deps/` 산출물, 그다음 상한을 넘으면 가장 오래된 산출물 순으로 지운다(Cargo는 재사용 시 mtime을 갱신하지 않으므로 오래된 의존성도 지워져 재빌드될 수 있다). 소스·git·`.spec`은 지우지 않는다.
- kinekino와 target 공유: Cargo.lock 기준 meari 505개 패키지 중 462개(이름+버전)가 kinekino와 겹친다. 공유 `CARGO_TARGET_DIR`은 디스크를 줄이지만 (1) 동시 빌드 시 target 디렉터리 잠금으로 직렬화되고 (2) 기능 플래그·프로필·`[patch]`(blitz-paint) 차이로 실제 재사용은 일부이며 (3) 한쪽 정리가 다른 쪽 재빌드를 부른다. 기본 적용하지 않는다.

## Why

사용자 지시(2026-10-29): Meari도 kinekino와 같은 컨벤션으로 리팩터링한다.

## Verification

`cargo xtask lint-structure --strict`(플랫 나열·`x.rs`/`x/` 공존·금지 이름). 네이밍과 SRP 일부는 사람이 검토한다.
