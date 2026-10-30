---
id: guide:meari/ignored-tests
title: "#[ignore] 테스트: 사유와 실행 방법"
status: active
created: 2026-10-30T04:15:55Z
updated: 2026-10-30T04:15:57Z
---

# #[ignore] 테스트: 사유와 실행 방법

`#[ignore]` 23건은 기본 `cargo test`(게이트)에서 빠진다. 외부 도구·Windows 실제 창·GL 드라이버·준비된 픽스처가 필요하기 때문이며 실패를 숨기는 용도가 아니다. 실행할 때는 하나씩, `nice` + `-j1`로, 창·타이머 슬롯 테스트는 단독(`--test-threads=1`)으로 돌린다.

공통 형식: `cargo test -p <crate> --test <target> <test_name> -- --ignored --exact --test-threads=1`

| 그룹 | 테스트(파일) | 사유 | 준비/실행 |
|---|---|---|---|
| 공식 Tailwind 컴파일러 (5) | icons.rs `icon_size_tokens_produce_native_geometry_with_the_official_compiler`; custom_styles.rs `official_custom_theme_utilities_variants_apply_reference_and_imports_render`; engine.rs `official_tailwind_output_runs_through_native_css_and_paint`, `first_showcase_capture_contains_real_content_without_a_state_change`; widgets/groups.rs `official_tailwind_renders_button_group_horizontal_and_vertical_geometry` (모두 meari-native/tests) | 고정 버전 Tailwind standalone 실행 파일 필요 | `BRIDGE_TAILWIND=<tailwindcss 4.3.3 실행 파일 절대경로>`를 설정하고 `-p meari-native --test <pages\|rendering\|widgets> ... -- --ignored` |
| 외부 shadcn 코퍼스 (2) | meari-native/tests/pages/kitchen_sink.rs 2건 | 저장소 밖 공식 코퍼스/영수증 | `MEARI_KITCHEN_SINK=<코퍼스 루트>` |
| 실제 앱 빌드 (3) | meari-builder/tests/real_apps/{main,modules,assets}.rs | Native+Web 실제 빌드, 등록된 도구와 빌드 슬롯 필요(무거움) | 다른 빌드가 없을 때 `-p meari-builder --test real_apps -- --ignored --test-threads=1` |
| 패키지 경계 (2) | meari/tests/platform/package_boundary.rs `prepare_isolated_cargo_package_boundary_inputs`, `actual_package_lists_preserve_sources_and_reject_private_sentinels` | 테스트가 Cargo를 실행하지 않도록 준비·검사를 분리 | 1) `MEARI_PACKAGE_BOUNDARY_FIXTURE=<새 빈 절대경로>`로 prepare 실행 2) 그 픽스처에서 `cargo package --list` 결과 파일 생성 3) 같은 변수로 두 번째 테스트 실행 |
| Windows 실제 창/루프/타이머 슬롯 (9) | native_child_window/lifecycle.rs, parent_client.rs; windowing/window_user_close_os.rs; startup_window/frame_stamps.rs 5건; src/infrastructure/window/startup/tests/timer_os.rs | 실제 OS 창·이벤트 루프 필요, 헤드리스/Linux 불가, 서로 간섭 | Windows(`C:\workspaces\meari`)에서 테스트 하나씩 `--exact --test-threads=1`; 그동안 다른 창 테스트 금지. timer_os는 lib 테스트: `-p meari-native --lib original_os_timer_one_shot_rearm_cancel_and_destroy_are_causal -- --ignored --exact` |
| Windows GL 드라이버 (2) | meari-native/src/docs/suites/gl_suite.rs 2건 | 실제 GL 드라이버 필요 | Windows에서 `cargo test -p meari-native --features gl --lib gl_suite -- --ignored --test-threads=1` |

합계 5+2+3+2+9+2 = 23. 목록 갱신: `rg -n '#\[ignore' -g '*.rs'`.
