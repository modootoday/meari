# Meari 구조 리팩터링 로그 (2026-10-29 KST, box Linux)

승인: 사용자 "권고대로 진행" (Q1 import 커밋 1개, Q2 wip patch 별도 브랜치 검토, Q3 meari-cli 단일 바이너리, Q4 공통 lint는 우선 meari xtask → 추후 컨벤션 크레이트 분리).
원격 push 없음(push URL DISABLED 유지). Windows 빌드/실행은 **미검증** — 추후 GitHub Actions 매트릭스로 검증.

## 브랜치/커밋
- `import/canonical-from-kinekino` @ ea81d39: B(/workspace/oink-player/packages/meari) 564개 파일을 A(26805a9) 위 import 커밋 1개로 반영. HANDOFF 기준 base 0844f6c(이 저장소 이력에 없음). A 전용 meari-native/src/{main.rs,bin} 삭제.
- `refactor/crates-layout`:
  - 4baaa1e packages/ → crates/ (git mv), 경로 참조 갱신, meari-paint 정식 멤버화(독립 [workspace] 제거, blitz-paint patch 유지).
  - dc53ada Linux rfd 백엔드(xdg-portal) 선택, 비루트 resolver 제거 → Linux `cargo check --workspace` 통과.
  - 895ef80 모듈 스타일 `foo/mod.rs` 통일(77곳 중 67곳). tests/ 최상위 테스트 타깃 10곳은 Cargo 자동탐색 때문에 보류(→ tests/<맥락>/main.rs 재편 단계에서 처리).
  - (이 커밋) xtask lint-structure 초안, rust-toolchain.toml, 본 로그.
- `wip/review-tracked-patch` = import 커밋. 패치 미적용.

## 검증 (Linux, toolchain 1.99.1, jobs=1)
- fmt --check: 통과 / check --workspace(--all-targets): 통과 / clippy --workspace --all-targets: 통과(경고 소수, -D 미적용)
- test --workspace: 실패 2건(구조 변경과 무관, import 원본 상태 그대로):
  - meari `package_boundary::package_license_and_notice_copies_preserve_original_bytes` — crates/meari/NOTICE가 루트 NOTICE와 다름(Lucide/Pretendard 문구 추가분)
  - meari `mixed_slots::source_reference_changes_are_not_stable_placements...` — child_placement::plan이 Err를 기대했으나 Ok
- Windows: 미실행.

## box 환경 메모 (저장소 외부)
- apt 저장소 접근 불가(500/unsigned) → pkg-config, *-dev 미설치. `runtime/sysshim`(git 제외)에 pkg-config 셈과 lib*.so 심볼릭 링크로 우회. `. runtime/env.sh` 후 빌드.
- .cargo/config.toml target-dir이 ../../runtime(저장소 밖)을 가리키므로 CARGO_TARGET_DIR=runtime/build-cache/target 로 덮어씀.

## wip patch (docs/continuation/wip/meari-tracked.patch, oink-player 루트에 위치)
- `git apply --check`: 27/27 파일 실패. `git apply --check -R`: 성공 → **이미 B(import 커밋)에 반영된 상태**. 추가 적용 불필요, 담당 확인만 필요.

## 남은 위반 (cargo xtask lint-structure)
- 직속 .rs > 8: 8곳 (meari/tests 50, meari-native/tests 29, native/infrastructure 24, meari/domain 22, native/docs 15, web_document 12, meari/application 11, meari-paint/src 9)
- 600줄 초과: 21개 (최대 meari-paint render/mod.rs 1395)
- 금지 이름: 1 (meari-native/tests/shared_font_catalog.rs)
- x.rs + x/ 병존: 10 (tests 최상위 타깃)

## 다음 단계 (미착수)
meari-platform 분리(cfg 격리), core/web/builder 분할, meari-cli 통합(Q3), tests/<맥락>/main.rs 재편, 800줄 분해, xtask --strict CI 강제.
Actions 매트릭스(문서만): ubuntu-latest + windows-latest × {fmt --check, clippy -D warnings, test, xtask lint-structure}.
Q4 분리 계획: xtask/src/structure → 별도 `modoo-conventions`(가칭) 크레이트로 추출, kinekino·meari가 동일 git rev 고정.

## Visual test (refactor/visual-test)

- New crate `crates/meari-visual` (publish=false; deps meari, meari-native, png 0.18.1, all already in Cargo.lock) and `cargo xtask visual-test [--out DIR --chrome PATH --channel-tolerance N --max-ratio R]`.
- The same 8 TSX+CSS fixtures, 480x320: Native = InteractiveDocument -> blitz -> anyrender_vello_cpu `rasterize()` (offscreen CPU, **no window/GUI/Xvfb needed**). Web = the WebDocument::mount_internal pipeline (bind_with_scope -> StateProgram -> dom::prepare) serialized to HTML and captured by headless Chrome over CDP (`scripts/cdp-screenshot.mjs`; `chrome --screenshot` hangs in this box).
- Tolerance: a pixel counts as different if any channel differs by >16; a fixture passes if ≤0.5% of pixels differ. Per-fixture native.png / web.png / web.html / diff.png (magenta = differs) and report.md.
- Before (/workspace/meari-visual/before): 6 MATCH, heading-text MISMATCH 2.311% (generic `sans-serif` resolves to Noto Sans in Native but an Arial-like face in Chrome), button-input ERROR (`form` has no Native widget).
- After (/workspace/meari-visual/after): fixtures pin `font-family:"Noto Sans"`; button-input uses `div` instead of `form`. 7 MATCH, heading-text MISMATCH 0.764% (glyph antialiasing/positioning, vello vs Skia; left unresolved, tolerance not raised).
- Open gaps: Native `form` support; `sans-serif` generic-family mapping; the wasm-bindgen glue/Frame::build is not run (wasm-bindgen-cli unavailable, the harness mirrors it); Windows not verified.

## 구조 리팩터링 단계 (refactor/structure-phase, worktree /workspace/meari-refactor, 기준 f6a36d7)
기능 executor(/workspace/meari)와 병렬. 렌더러·스타일·Tailwind·React 런타임 파일은 최소 변경.
clippy는 `-D warnings -A deprecated`로 실행: 의존 크레이트 blitz-paint(meari-paint) `fetch_update` deprecated 경고(toolchain 1.99.1) 1건이 기존부터 존재, 페인트 코드는 기능 쪽 소유라 미수정.

### 3단계: 단일 `meari` CLI
- 신규 `crates/meari-cli`(바이너리 `meari`): `meari convert|native|build ...` → 각 크레이트 presentation `run_with(args)`로 위임. 테스트 `tests/dispatch.rs`.
- `meari::presentation`(변환 CLI)과 `meari_builder::presentation`을 bin 전용 모듈에서 lib 모듈로 이동(추가 공개 경로만, 기존 API 불변).
- 기존 바이너리 `meari-cli`, `meari-native`, `meari-builder`는 호환 별칭으로 유지(동작 동일, 테스트가 CARGO_BIN_EXE_*로 참조).
- `has_pending_turn`: window feature 없을 때 dead_code → `cfg_attr(not(feature="window"), allow(dead_code))`.

### 1단계: 플랫폼 분리 (cfg 격리)
- `meari/infrastructure/platform/{unix,portable}.rs`: 아티팩트 실행 권한 검사·설정(`executable_mode_matches`, `mark_executable`). artifact_store.rs의 `cfg(unix)` 블록 제거.
- `meari-builder/infrastructure/platform/{unix,windows}.rs`: `is_link`(Windows reparse point / Unix symlink). files.rs 의 cfg 제거.
- `meari-native/infrastructure/window/actions.rs` → `actions/mod.rs` + `actions/observation/{windows,portable}.rs`(Win32 창 상태 관찰, 코드 그대로 이동).
- 남은 OS cfg(의도적 유지): native `uia_probe` 모듈 게이트, `parent_client` 좌표 변환 블록, `startup/owner.rs` winit any_thread, native CLI `probe` 명령 — 창 호스트(기능 쪽 영역)라 이번엔 미변경.
- 기존 문제(미수정): `--features window` 테스트 빌드에서 timers/testing.rs dead_code 3건(clippy -D 시 실패), 본 변경과 무관.
