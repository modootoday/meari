---
id: guide:meari/crates-io-npm-trusted-publishing
title: "릴리스: crates.io·npm Trusted Publishing"
status: active
created: 2026-10-30T06:43:25Z
updated: 2026-10-30T07:10:00Z
---

# 릴리스: crates.io·npm Trusted Publishing

## Overview

`v*` 태그 push가 `.github/workflows/release.yml`을 실행한다. 토큰을 저장하지 않고 OIDC(Trusted Publishing)로 게시한다. 구조는 modootoday/specfold를 따른다: `check` → `crates`(environment `crates`) / `binaries`(5개 타깃) → `npm`(environment `npm`). 두 environment는 필수 리뷰어와 `v*` 태그 정책을 가진다. 액션은 커밋 SHA로 고정하고 기본 권한은 `contents: read`다. 워크스페이스 구조는 [[adr:meari/20261030065055]]를 따른다.

게시 대상: crates.io `meari-platform`, `meari`, `meari-dom`, `meari-paint`, `meari-html`, `meari-shell`, `meari-native`, `meari-builder`, `meari-cli`, `meari-native-tools`(`cargo publish --workspace`가 순서를 정한다). npm `@modootoday/meari-cli`와 플랫폼 패키지 `@modootoday/meari-cli-{linux-x64,linux-arm64,darwin-x64,darwin-arm64,win32-x64}`(`npm/`). `meari-visual`, `meari-demo`, `meari-xtask`는 `publish = false`.

제약(2026-10 확인): crates.io와 npm 모두 OIDC로 새 이름을 처음 게시할 수 없다(crates.io docs/trusted-publishing, RFC 3691; npm `npm trust`는 "Package must exist", npm ≥ 11.15.0, 계정 2FA 필요). 따라서 첫 버전은 수동 부트스트랩이다.

## Steps

1. 사전 점검: `scripts/release-preflight.sh`(publish = false, `[patch]`, 버전 없는 path 의존, 레지스트리 blitz 포크 사용을 거부)와 `cargo publish --workspace --dry-run`.
2. GitHub environments `crates`, `npm` 생성: 리뷰어 지정, deployment policy 태그 `v*`.
3. 첫 게시(수동, 1회): crates.io 토큰으로 `cargo publish --workspace`; npm은 2FA 계정으로 플랫폼 패키지 5개 후 `@modootoday/meari-cli`를 `npm publish --access public`.
4. Trusted Publisher 등록: crates.io 각 크레이트 Settings → Trusted Publishing(repo `modootoday/meari`, workflow `release.yml`, environment `crates`). npm은 `npm trust github <pkg> --repo modootoday/meari --file release.yml --env npm` 또는 웹 UI. 새 설정은 2일 안에 첫 OIDC 게시가 있어야 한다.
5. 이후 릴리스: `[workspace.package] version`을 올리고 `vX.Y.Z` 태그를 push한 뒤 environment를 승인한다.
6. `workflow_dispatch`(dry-run)는 레지스트리에 쓰지 않는다. `cargo publish --workspace --dry-run`은 워크스페이스 크레이트를 임시 레지스트리로 검증하므로 첫 게시 전에도 통과한다(2026-10-30 로컬 확인: 10개 모두 verify 후 upload 중단). `npm` 단계는 `npm publish --dry-run`이다.
