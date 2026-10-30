---
id: adr:meari/20261030065055
title: "독립 워크스페이스: 벤더 포크를 meari-* 이름으로 게시하고 [patch] 제거"
status: active
created: 2026-10-30T06:50:55Z
updated: 2026-10-30T06:50:59Z
---

# 독립 워크스페이스: 벤더 포크를 meari-* 이름으로 게시하고 [patch] 제거

## Context

루트 `[patch.crates-io]`가 업스트림 `blitz-dom`·`blitz-paint`를 `crates/meari-dom`·`crates/meari-paint`로 바꿔 끼웠다([[adr:meari/20261030035135]]). 게시된 크레이트에서는 `[patch]`가 무시되므로 crates.io의 `meari-native`는 수정 전 업스트림을 받는다. 또 업스트림 `blitz-html`·`blitz-shell`도 `blitz-dom`에 의존하므로 patch를 빼면 같은 타입이 두 벌 생긴다. specfold처럼 각 크레이트가 자기 이름과 path+version 의존만 쓰는 일반 워크스페이스로 만든다([[guide:meari/crates-io-npm-trusted-publishing]]).

## Decision

- 패키지 이름: `meari-dom`(blitz-dom), `meari-paint`(blitz-paint), 신규 벤더 `meari-html`(blitz-html), `meari-shell`(blitz-shell). 모두 0.3.0-beta.4, DioxusLabs/blitz `92b11f1bd8ab7975411328b0406a95a09445b4e7`. 버전은 워크스페이스와 같은 0.1.0. 2026-10-30 crates.io 조회에서 네 이름 모두 404(미사용).
- 라이브러리 이름(`blitz_dom` 등)은 유지하고, 소비 쪽은 `blitz-dom = { package = "meari-dom", path, version }`처럼 의존 키를 유지한다. 소스의 `use`와 기능 이름은 바뀌지 않는다.
- 루트 `[patch.crates-io]`를 지운다. meari-builder가 생성하던 앱 Cargo.toml의 `[patch]`도 지운다(meari-native가 path로 포크를 끌어온다).
- 게시 대상: meari-platform, meari, meari-dom, meari-paint, meari-html, meari-shell, meari-native, meari-builder, meari-cli, meari-native-tools. meari-visual, meari-demo, xtask는 `publish = false` 유지.
- `[workspace.package]`에 version, edition, license, repository를 모으고 1st-party 크레이트가 상속한다.

## Consequences

- 업스트림 라이선스와 출처는 각 디렉터리 LICENSE, NOTICE, THIRD_PARTY_NOTICES.md에 남긴다. meari-html·meari-shell 소스는 매니페스트 외 무수정.
- blitz 업그레이드 시 네 포크를 함께 맞춘다.
- 크레이트 이름은 게시와 함께 영구 점유된다.
