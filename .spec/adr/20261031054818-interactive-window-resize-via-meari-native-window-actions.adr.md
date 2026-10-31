---
id: adr:meari/20261031054818
title: "Interactive window resize via meari-native window actions, not a meari-shell fork"
status: active
created: 2026-10-31T05:48:18Z
updated: 2026-10-31T05:48:18Z
---

# Interactive window resize via meari-native window actions, not a meari-shell fork

## Context

앱이 직접 그린 제목 줄·테두리(decorations off)를 쓰면 창 가장자리 크기 조절을 OS에 맡길 방법이 필요하다. `meari-shell`은 crates.io `blitz-shell` 0.3.0-beta.4를 바이트 그대로 둔 vendored crate([[adr:meari/20261030065055]], R-vendored)이고, `ShellProvider` 트레이트는 외부 `blitz-traits =0.3.0-beta.4`에 있어 `drag_resize_window`를 추가하려면 두 crate를 fork해야 한다. 소비 앱은 지금 X11 전용 우회를 쓴다.

## Decision

`meari-shell`/`blitz-traits`는 수정하지 않는다. `meari-native`의 창 액션 포트(`NativeWindowActions::request`)에 `WindowAction::BeginResize(ResizeDirection)`(8방향, 자체 도메인 enum)을 추가하고, 어댑터가 winit `Window::drag_resize_window`로 실행한다. 창 상태 관찰 없이도 항상 허용(capability true)하며, 호출 성공이면 `Applied`, OS가 거부하면 `Unsupported`로 실패한다. 커서 setter처럼 OS 관찰로 완료를 판정하지 않는다.

## Consequences

- 플랫폼 공통 경로: winit이 X11·Wayland·Windows를 지원하고, macOS는 winit이 미지원이라 `Unsupported`로 끝난다(명확한 실패, 무시 아님).
- 공개 API 추가만 있음(`WindowAction` 변형 추가). `WindowAction`이 `#[non_exhaustive]`가 아니므로 외부에서 이 enum을 빠짐없이 `match`하던 코드는 컴파일이 깨진다 → semver상 0.1→0.2 급 변경. 릴리스는 별도 지시.
- 검증: 모델 단위 테스트만. 실제 OS 크기 조절(X11/Wayland/Windows) 실행 검증은 미완.
