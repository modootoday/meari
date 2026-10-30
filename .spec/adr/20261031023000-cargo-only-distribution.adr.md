---
id: adr:meari/20261031023000
title: "배포는 cargo 전용: npm 배포 중단·제거"
status: active
created: 2026-10-31T02:30:00Z
updated: 2026-10-31T02:30:00Z
---

# 배포는 cargo 전용: npm 배포 중단·제거

## Context

[[adr:meari/20261030065055]]와 함께 `@modootoday/meari-cli`와 플랫폼 바이너리 패키지 5개(`linux-x64`, `linux-arm64`, `darwin-x64`, `darwin-arm64`, `win32-x64`)를 npm으로 내는 릴리스 초안을 두었다. crates.io 0.1.0 게시는 끝났지만 npm 첫 게시는 계정 2FA(보안 키 전용) 문제로 보류됐다. 2026-10-31 원 님 결정: meari는 cargo 환경에서만 쓸 수 있으면 된다.

## Decision

- 배포 경로는 crates.io뿐이다. CLI는 `cargo install meari-cli`, 라이브러리는 crates.io 의존으로 쓴다.
- `npm/` 디렉터리(메인 패키지, 런처 `bin/meari.js`, 플랫폼 패키지 생성 스크립트와 테스트)를 지운다.
- `release.yml`에서 npm 전용 `binaries`(5개 타깃 빌드·아티팩트)와 `npm`(environment `npm`) 잡을 지운다. `check` → `crates`만 남는다.
- 릴리스 가이드를 [[guide:meari/crates-io-trusted-publishing]]로 바꾼다(이전 id `guide:meari/crates-io-npm-trusted-publishing`).

## Consequences

- npm에는 아무 패키지도 게시된 적이 없으므로 회수할 버전은 없다.
- 이미 push된 `v0.1.0` 태그의 워크플로에는 `npm` 잡이 남아 있다. 그 실행의 npm 승인 대기는 취소하고, GitHub `npm` environment는 정리한다(저장소 설정 작업, 원 님 승인 필요).
- 사전 빌드 바이너리가 필요해지면 GitHub Release 자산 등 별도 결정으로 다시 다룬다.
