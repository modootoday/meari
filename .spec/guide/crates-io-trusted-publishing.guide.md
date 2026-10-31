---
id: guide:meari/crates-io-trusted-publishing
title: "릴리스: crates.io Trusted Publishing"
status: active
created: 2026-10-30T06:43:25Z
updated: 2026-10-31T06:20:00Z
---

# 릴리스: crates.io Trusted Publishing

## Overview

`v*` 태그 push가 `.github/workflows/release.yml`을 실행한다. 토큰을 저장하지 않고 OIDC(Trusted Publishing)로 게시한다. 구조는 modootoday/specfold를 따른다: `check` → `crates`(environment `crates`). environment는 필수 리뷰어와 `v*` 태그 정책을 가진다. 액션은 커밋 SHA로 고정하고 기본 권한은 `contents: read`다. 워크스페이스 구조는 [[adr:meari/20261030065055]]를 따른다.

배포 경로는 cargo뿐이다. 사용자는 `cargo install meari-cli`로 CLI를 설치하고, 라이브러리는 `Cargo.toml` 의존으로 쓴다.

게시 대상: crates.io `meari-platform`, `meari`, `meari-dom`, `meari-paint`, `meari-html`, `meari-shell`, `meari-native`, `meari-builder`, `meari-cli`, `meari-native-tools`(`cargo publish --workspace`가 순서를 정한다). `meari-visual`, `meari-demo`, `meari-xtask`는 `publish = false`.

제약(2026-10 확인): crates.io는 OIDC로 새 이름을 처음 게시할 수 없다(crates.io docs/trusted-publishing, RFC 3691). 따라서 첫 버전은 수동 부트스트랩이다.

## Steps

1. 사전 점검: `scripts/release-preflight.sh`(publish = false, `[patch]`, 버전 없는 path 의존, 레지스트리 blitz 포크 사용을 거부)와 `cargo publish --workspace --dry-run`.
2. GitHub environment `crates` 생성: 리뷰어 지정, deployment policy 태그 `v*`.
3. 첫 게시(수동, 1회): crates.io 토큰으로 `cargo publish --workspace`. 게시 후 토큰은 삭제한다.
4. Trusted Publisher 등록: crates.io 각 크레이트 Settings → Trusted Publishing(repo `modootoday/meari`, workflow `release.yml`, environment `crates`).
5. 이후 릴리스: `[workspace.package] version`과 워크스페이스 내부 path 의존의 `version`(vendored 크레이트 Cargo.toml 포함)을 같이 올리고 `CHANGELOG.md`를 갱신한 뒤, `ci.yml`을 Linux·Windows·macOS 모두 켠 `workflow_dispatch`로 릴리스 브랜치에서 통과시키고, `vX.Y.Z` 태그를 push한 뒤 environment를 승인한다. `crates` 잡은 `scripts/publish-unpublished.sh`로 아직 없는 버전만 게시한다.
6. `workflow_dispatch`(dry-run)는 레지스트리에 쓰지 않는다. `cargo publish --workspace --dry-run`은 워크스페이스 크레이트를 임시 레지스트리로 검증하므로 첫 게시 전에도 통과한다(2026-10-30 로컬 확인: 10개 모두 verify 후 upload 중단).
