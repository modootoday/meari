---
id: adr:meari/20261030035135
title: "blitz-dom 벤더링(crates/meari-dom): 인라인 폭 1/64px LayoutUnit 올림"
status: active
created: 2026-10-30T03:51:35Z
updated: 2026-10-30T03:51:37Z
---

# blitz-dom 벤더링(crates/meari-dom): 인라인 폭 1/64px LayoutUnit 올림

## Context

visual-test `app-list`가 Native/Web LAYOUT-MISMATCH였다. 원인은 blitz-dom 0.3.0-beta.4 `layout/inline.rs`가 인라인 콘텐츠 폭(`computed_width`)과 측정 폭(`measured_size.width`)을 정수 px로 `.ceil()` 하는 것이다. Chrome은 `LayoutUnit::FromFloatCeil`로 1/64px 단위로 올린다. 예: Noto Sans 16px "2"는 9.152px → Chrome 9.15625px, blitz 10px. 그래서 오른쪽 정렬 숫자가 약 0.85px 왼쪽으로 밀렸다. Meari 쪽 CSS로는 우회할 수 없다.

## Decision

`blitz-paint` → `crates/meari-paint` 선례와 같은 방식으로 published `blitz-dom` 0.3.0-beta.4(DioxusLabs/blitz `92b11f1bd8ab7975411328b0406a95a09445b4e7`, `packages/blitz-dom`)를 `crates/meari-dom`에 벤더링한다.

- (갱신: [[adr:meari/20261030065055]]) 패키지 이름은 `meari-dom`으로 바뀌고 `[patch]`는 없어졌다. 원본 LICENSE-MIT/LICENSE-APACHE 유지.
- 패치: 두 반올림 지점을 `layout_unit_ceil(px) = ceil(px*64)/64`로 바꾼다(`layout/inline/` 의 `// Meari:` 주석, 단위 테스트 `layout_unit_tests`).
- lint-structure는 예외·완화 없이 통과시킨다. OS `cfg`(macOS 단축키·Backspace·시스템 폰트 별칭)는 `meari_platform::current().os_family()` 질의로 바꿨다. 600줄 초과 파일은 항목 단위로 `x/mod.rs` + 하위 모듈로 나눴고(동작 불변), 8파일 초과 디렉터리는 `#[path]`로 묶음 폴더(`styling/`, `content/`, `tree_access/`)에 담아 모듈 경로를 유지했다. 자세한 내용은 `crates/meari-dom/README.md`.
- 허용치 상향·비교 완화는 하지 않는다.

## Consequences

- blitz-dom 업그레이드 시 이 포크를 다시 맞춰야 한다. 대상 버전은 0.3.0-beta.4로 고정한다.
- iOS(`target_vendor = "apple"`)의 시스템 폰트 별칭은 macOS 판정으로 근사한다(Meari는 iOS를 대상으로 하지 않는다).
- 라이선스 고지는 `NOTICE`, `THIRD_PARTY_NOTICES.md`에 있다.
