---
id: adr:meari/20261030065055
title: "독립 워크스페이스: 벤더 포크를 meari-* 이름으로 게시하고 [patch] 제거"
status: active
created: 2026-10-30T06:50:55Z
updated: 2026-10-30T06:50:59Z
---

# 독립 워크스페이스: 벤더 포크를 meari-* 이름으로 게시하고 [patch] 제거

## Context

루트 `[patch.crates-io]`가 업스트림 `blitz-dom`·`blitz-paint`를 `crates/meari-dom`·`crates/meari-paint`로 바꿔 끼웠다([[adr:meari/20261030035135]]). 게시된 크레이트에서는 `[patch]`가 무시되므로 crates.io의 `meari-native`는 수정 전 업스트림을 받는다. 또 업스트림 `blitz-html`·`blitz-shell`도 `blitz-dom`에 의존하므로 patch를 빼면 같은 타입이 두 벌 생긴다. specfold처럼 각 크레이트가 자기 이름과 path+version 의존만 쓰는 일반 워크스페이스로 만든다([[guide:meari/crates-io-trusted-publishing]]).

## Decision

- 패키지 이름: `meari-dom`(blitz-dom), `meari-paint`(blitz-paint), 신규 벤더 `meari-html`(blitz-html), `meari-shell`(blitz-shell). 모두 0.3.0-beta.4, DioxusLabs/blitz `92b11f1bd8ab7975411328b0406a95a09445b4e7`. 버전은 워크스페이스와 같은 0.1.0. 2026-10-30 crates.io 조회에서 네 이름 모두 404(미사용).
- 라이브러리 이름(`blitz_dom` 등)은 유지하고, 소비 쪽은 `blitz-dom = { package = "meari-dom", path, version }`처럼 의존 키를 유지한다. 소스의 `use`와 기능 이름은 바뀌지 않는다.
- 루트 `[patch.crates-io]`를 지운다. meari-builder가 생성하던 앱 Cargo.toml의 `[patch]`도 지운다(meari-native가 path로 포크를 끌어온다).
- 게시 대상: meari-platform, meari, meari-dom, meari-paint, meari-html, meari-shell, meari-native, meari-builder, meari-cli, meari-native-tools. meari-visual, meari-demo, xtask는 `publish = false` 유지.
- `[workspace.package]`에 version, edition, license, repository를 모으고 1st-party 크레이트가 상속한다.
- 린트 예외(원 님 승인 대기): 수정 없이 가져온 `meari-html`, `meari-shell`의 `src/`만 lint-structure 파일 길이·OS cfg 규칙에서 제외한다. 근거: `window.rs` 879줄과 android/ios/macOS winit 확장 트레이트의 컴파일 타임 cfg 19곳은 런타임 `CurrentOs`로 옮길 수 없고, 고치면 원본 대비 포크가 된다. 조건: `VENDORED.sha256`(crates.io `.crate` sha256과 rev `92b11f1` 기록, `src/` 전 파일 해시)과 일치하고 미등재 파일이 없을 때만 예외. 불일치는 `vendored drift` 위반으로 `--strict`를 실패시키고 예외도 풀린다. `scripts/vendored-upstream-diff.sh`가 crates.io 원본과 다시 대조한다. 원본을 고쳐야 하면 meari-dom처럼 포크로 전환하고 매니페스트를 지운다.

## Consequences

- 업스트림 라이선스와 출처는 각 디렉터리 LICENSE, NOTICE, THIRD_PARTY_NOTICES.md에 남긴다. meari-html·meari-shell 소스는 매니페스트 외 무수정.
- blitz 업그레이드 시 네 포크를 함께 맞춘다.
- 크레이트 이름은 게시와 함께 영구 점유된다.
