---
id: adr:meari/20261031070804
title: "Cross-OS CI failures: root causes and fixes"
status: draft
created: 2026-10-31T07:08:04Z
updated: 2026-10-31T07:08:04Z
rel:
  governs: [.github/workflows/ci.yml, crates/meari-visual/src/host.rs, crates/meari-shell/Cargo.toml]
---

# Cross-OS CI failures: root causes and fixes

## Context

`ci.yml`(workflow_dispatch, Linux Strict / Windows·macOS Advisory) 실행 기록의 모든 실패를 `gh run view --log-failed`로 분류했다.

| 실행 | 커밋 | OS·단계 | 분류 | 원인 | 상태 |
|---|---|---|---|---|---|
| 41005132053 | dfd0fbb | Windows clippy | 러너 환경 | `.cargo/config.toml`의 TMP가 체크아웃에 없는 디렉터리 → MSVC LNK1104 | a726ff8에서 해소 |
| 41005132053 | dfd0fbb | Linux·macOS test | 러너 환경 | 폰트 캐시가 비어 있을 때 여러 테스트가 같은 Noto 파일을 동시에 받다 경합 | a726ff8(`fonts-fetch` 선행)에서 해소 |
| 41007117127 | 9f29d96 | Windows test | 플랫폼 차이 | runtime root 비교에 Windows verbatim(`\\?\`) 접두 | 819591f에서 해소 |
| 41013921431, 41175589026(1·2차) | dab9c74, 4cec371 | Linux visual (Strict) | 시각 기준(렌더러 환경) | 아래 1 | 이 ADR |
| 41013921431, 41175589026(1·2차) | dab9c74, 4cec371 | macOS visual (Advisory) | 플랫폼 차이(Chrome) | 아래 3 | 이 ADR |
| 41175589026(1·2차) | 4cec371 | Windows test | 코드 결함(워크스페이스 기능 통합) | 아래 2 | 이 ADR |

1. **Linux Strict 시각 비교.** 박스와 러너가 같은 Chrome 156.0.8078.47을 쓰고 native.png는 바이트 단위로 같은데, web.png만 달랐다(heading-text 2443px, canonical-font 6964px). Linux Chrome은 글리프 래스터 매개변수(안티앨리어싱, 힌팅, 서브픽셀, LCD 필터, 임베디드 비트맵)를 호스트 fontconfig에서 패밀리 이름으로 가져온다. 기존 하네스는 호스트 `fonts.conf`를 include한 위에 `rgba=none`만 덮어써서, 러너의 conf.d 규칙과 설치 폰트 차이가 그대로 기준 이미지에 섞였다. 박스에서 호스트 설정을 빼고(기본 fontconfig) 렌더하면 heading-text가 1.184%로 같은 종류의 실패가 재현된다.
2. **Windows `actual_os_driver_presents_reprojects_drains_capacity_and_retries_close`.** `accesskit_windows` 패닉 "subclassing adapter already instantiated" 뒤 프로브 타임아웃. 78536e9(독립 워크스페이스)에서 meari-shell이 워크스페이스 멤버가 되자, `cargo test --workspace`가 멤버의 기본 기능(upstream: accessibility, clipboard, file-dialog)을 켜고 Cargo가 이를 meari-native가 쓰는 같은 meari-shell에 통합했다. meari-native는 창마다 자체 AccessKit 어댑터를 만들므로(`host_refresh.rs`) 셸 쪽 어댑터와 둘이 되어 Windows 서브클래싱이 패닉한다(Linux AT-SPI는 중복을 허용해 드러나지 않음). `cargo tree --workspace -e features -i meari-shell`로 박스에서도 확인된다.
3. **macOS input-intrinsic-width LAYOUT-MISMATCH.** macOS Chrome이 `size=20` 텍스트 입력의 고유 폭을 204px(Linux 255px)로 잡는다. native는 Linux Chromium 공식(OS/2 xAvgCharWidth)을 따른다. Windows처럼 Skia 백엔드(CoreText)가 avg/max char width를 다르게 보고하는 플랫폼 차이로 보이며, 공식을 확정하려고 `cdp-screenshot.mjs`가 size 1·2·10·20의 콘텐츠 폭을 `web-metrics.json`에 남긴다.

## Decision

1. Chrome 기준 렌더용 fontconfig를 **호스트와 독립**으로 만든다: include 없음, 폰트 디렉터리는 고정 Noto 캐시(`runtime/fonts`)만, 렌더 매개변수는 모두 명시 대입(antialias=true, hinting=true, hintstyle=hintslight, autohint=false, rgba=none, lcdfilter=lcddefault, embeddedbitmap=false, embolden=false). 이는 허용 오차를 측정한 박스 기준과 같은 값이며, 박스에서 결과 이미지가 이전과 픽셀 단위로 같다. 허용 오차는 올리지 않는다.
2. meari-shell의 `default` 기능을 비운다(vendored 해시 대상은 `src/`뿐이고 Cargo.toml은 meari가 관리). 사용처가 필요한 기능을 명시한다(meari-native는 `clipboard`만). 접근성 어댑터의 소유자는 meari-native 하나다.
3. macOS 입력 고유 폭은 CI 측정값으로 공식을 정한 뒤 `AvgCharWidthSource`의 macOS 어댑터로 반영한다(진행 중, 결과를 이 ADR에 갱신).

## Consequences

- Linux 기준 이미지는 러너 이미지 갱신·폰트 설치와 무관해진다. Chrome 버전만 남은 변수다.
- meari-shell을 직접 쓰는 외부 사용자는 accessibility/clipboard/file-dialog를 명시해야 한다(0.2.0 CHANGELOG에 기록).
- 재시도나 Advisory 무시로 가리지 않는다. 세 OS 모두 통과해야 릴리스 태그를 만든다.
