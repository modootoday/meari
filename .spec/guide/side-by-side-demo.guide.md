---
id: guide:meari/side-by-side-demo
title: Side-by-side demo (cargo xtask demo)
status: active
created: 2026-10-29
updated: 2026-10-29
rel:
  governs: [crates/meari-demo]
---

# Side-by-side demo (cargo xtask demo)
같은 TSX 픽스처를 **meari-native 창**(왼쪽)과 **Chrome 창**(오른쪽)에 나란히 띄워 사람이 눈으로 비교하는 도구다. 픽스처는 visual-test와 같은 `crates/meari-visual/fixtures/{supported,gaps}`를 쓴다.

## 실행 (Linux / X11, 예: Grok Bot 데스크탑 DISPLAY=:7)

```sh
export PATH=$HOME/.cargo/bin:$PATH
export CARGO_TARGET_DIR=$PWD/runtime/build-cache/target TMP=$PWD/runtime/build-cache/tmp TEMP=$PWD/runtime/build-cache/tmp
DISPLAY=:7 cargo xtask demo                    # 첫 픽스처부터
DISPLAY=:7 cargo xtask demo app-form           # 이름(부분 일치) 또는 번호로 시작
cargo xtask demo --list                        # 목록만 출력
DISPLAY=:7 cargo xtask demo 3 --seconds 60     # 60초 뒤 자동 종료
```

터미널에 명령을 입력하고 Enter: `n` 다음, `p` 이전, `l` 목록, 번호/이름 이동, `r` 다시 열기, `q` 종료.

## 동작

- Native: 데모 바이너리가 자신을 `native-window` 모드로 다시 실행해 meari-native 창(blitz + vello CPU)을 띄운다. 창 크기 = 렌더 viewport.
- Web: 픽스처의 Web DOM 계획(visual-test와 동일한 `bind_with_scope -> StateProgram::project -> dom::prepare`)을 HTML로 쓰고, 데모 프로세스 안의 정적 서버(127.0.0.1, 임의 포트)로 서빙해 `chrome --app` 창으로 연다. 프로필은 `runtime/demo/.chrome-profile`.
- 배치: `xdotool getdisplaygeometry`로 화면 크기를 읽어 두 창이 겹치지 않게 좌우로 나눈다(창당 최대 900×900). Chrome은 `--window-position/--window-size`, Native는 `xdotool windowmove/windowsize`.
- Tailwind CSS는 고정된 v4.3.3 바이너리(`runtime/tools/tailwind`)로 컴파일하고, 산출물은 `runtime/demo/<fixture>/`에 남는다.
- 종료(`q`, `--seconds`, 픽스처 전환) 시 이전 창 프로세스는 SIGTERM 후 3초 안에 안 끝나면 kill. Ctrl+C는 같은 프로세스 그룹의 Chrome과 Native 창에도 전달된다.

## 한계

- Web 창은 정적 DOM이다. wasm-bindgen 글루(`WebDocument`/`Frame::build`)를 실행하지 않으므로 클릭·입력 같은 상호작용은 Web 쪽에서 동작하지 않는다. Native 창은 상호작용한다.
- 실제 React 런타임(react-dom)으로 빌드한 결과가 아니다. meari의 Web 어댑터 경로다.
- 창 배치는 X11(xdotool) 전용이다. Wayland 순수 세션, Windows, macOS는 미지원·미검증.
- 비교는 사람 눈이 기준이다. 자동 픽셀 비교는 `cargo xtask visual-test`.
