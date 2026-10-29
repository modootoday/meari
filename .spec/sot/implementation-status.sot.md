---
id: sot:meari/implementation-status
title: Implementation status (React + Tailwind on Native/Web)
status: draft
verify: cargo xtask visual-test
created: 2026-10-29
updated: 2026-10-30
rel:
  governs: [crates/meari-visual]
---

# Implementation status (React + Tailwind on Native/Web)

## Rule

이 표가 React + Tailwind 기능별 Native/Web 지원 현황의 정본이다. 기능을 추가·변경하면 visual-test 픽스처와 함께 이 표를 갱신한다.


Evidence: `cargo xtask visual-test` (Native blitz/vello_cpu offscreen vs headless Chrome, same Tailwind v4.3.3 CSS).
Fixtures in `crates/meari-visual/fixtures/{supported,gaps}`. Last run 2026-10-30: 42 MATCH, 0 MISMATCH, 1 KNOWN-GAP (43 fixtures).
This is automated pixel comparison only; **no human visual check, no Windows, no real wasm-bindgen glue run**.

### Tailwind

| Feature | Status | Evidence |
|---|---|---|
| spacing / sizing | 구현됨 | tw-spacing-sizing |
| flex / grid layout | 구현됨 | tw-flex-layout, tw-grid-layout, flex-row, grid-cards |
| color, border, radius, shadow | 구현됨 | tw-color-border-shadow, rounded-border, solid-box |
| gradients (`in oklab`) | 구현됨 | peniko interpolation_cs mapping |
| typography | 부분 (text AA tolerance 1.6%) | tw-typography.tolerance |
| theme tokens | 구현됨 (AA tolerance) | tw-theme-tokens |
| responsive breakpoints | 구현됨 | tw-responsive |
| overflow clip (`overflow-hidden`) | 구현됨 | tw-scroll-overflow |
| internal scroll (`overflow-{x,y}-{auto,scroll,hidden}`, pinned scroll offsets) | 구현됨 (clip + offset pixel-matched; idle frame) | tw-overflow-scroll-offset + `.scroll` |
| wheel scroll, clamping, `overflow:hidden` ignores wheel | 구현됨 (blitz scroll chain) | meari-native tests/overflow_scroll.rs |
| ring / divide / space-x | 구현됨 (spread-only box-shadow painted as solid fill; 0.161%) | supported/tw-ring-divide-space, tests/ring_shadow.rs |
| transforms (rotate/translate/scale) | 구현됨 (0.000%) | supported/tw-transform |
| truncate / aspect-ratio / opacity | 구현됨 (0.348%) | supported/tw-truncate-aspect |
| scrollbars | 구현됨 (overlay, Chromium fade timing, thumb drag tested); visual test compares the idle frame with Chrome `--hide-scrollbars` — classic 15px Chrome scrollbars are not reproduced | overflow_scroll.rs (opacity, thumb drag) |
| dark mode | 구현됨 | dark-tw-dark-mode, dark-scheme |
| state variants (hover/focus/disabled) | 구현됨 | tw-state-variants |
| generic `sans-serif`/`serif` family | 구현됨 (Chromium Linux defaults: Arimo/Tinos, 0.866% glyph AA) | supported/generic-font-family |

### React / TSX

| Feature | Status | Evidence |
|---|---|---|
| useState + event handlers | 구현됨 | app-counter, state-counter |
| design components Card/Button with variant/size | 구현됨 (`asChild` rejected) | app-card-dashboard, tests/dom.rs |
| `cond && <El/>` | 구현됨 (lowered to `hidden`) | react-conditional, tests/jsx_text.rs |
| literal array `.map` | 부분 (static string arrays, unique keys, ≤1024) | react-static-list-map |
| dynamic `.map` over state | 부분 (dense non-empty string-array state, keyed; setter with full array literal re-expands; spread/append `[...xs, x]` and object items rejected; 0.424%) | supported/react-state-list-map, tests/state_list_map.rs |
| ternary `cond ? <A/> : <B/>` | 구현됨 (both branches JSX elements, lowered to `hidden`; 0.040%) | supported/react-ternary, tests/ternary.rs |
| controlled input (`value` + `onChange`) | 구현됨 (0.277%) | supported/react-controlled-input |
| useReducer | 미구현 (import rejected) | gaps/react-use-reducer |
| useEffect | 부분 (mount-only `[]`, setter literal/toggle/add, 0.008%) | supported/react-use-effect, meari/tests/use_effect.rs, meari-native/tests/use_effect.rs |
| HTML form/a/code/blockquote/figure/small etc. | 구현됨 | form-tag, html-* |
| input `defaultValue` | 구현됨 | app-form |
| input placeholder (`::placeholder` color) | 구현됨 (meari-native projects `::placeholder{color}` onto the host as `text-decoration-color`, UA default #757575; meari-paint lays out the attribute with the editor's style when empty). Limits: only `color` from `::placeholder` (no font/opacity), placeholder layout uses system fonts only (no `@font-face`) | input-placeholder (0.349%, glyph AA), tests/input_placeholder.rs |
| input intrinsic width | 구현됨 (Chromium size=20, 0.244%) | supported/input-intrinsic-width |
| `<progress>` | 구현 (native/fallback 두 모양, 0.000%) | supported/html-progress, tests/html_progress.rs |


### Examples status (every fixture, run 2026-10-30)

| Example (fixture) | Set | Status | Diff % | Threshold | Evidence test |
|---|---|---|---|---|---|
| app-card-dashboard | supported | MATCH | 0.608% | 0.800% | tests/dom.rs |
| app-counter | supported | MATCH | 0.124% | 0.500% | visual-test pixel diff |
| app-form | supported | MATCH | 0.867% | 1.000% | visual-test pixel diff |
| app-list | supported | MATCH | 0.350% | 0.500% | visual-test pixel diff |
| button-input | supported | MATCH | 0.311% | 0.500% | visual-test pixel diff |
| dark-scheme | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| dark-tw-dark-mode | supported | MATCH | 0.117% | 0.500% | visual-test pixel diff |
| flex-row | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| form-tag | supported | MATCH | 0.311% | 0.500% | visual-test pixel diff |
| generic-font-family | supported | MATCH | 0.866% | 1.000% | visual-test pixel diff |
| grid-cards | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| heading-text | supported | MATCH | 0.764% | 0.900% | visual-test pixel diff |
| html-a | supported | MATCH | 0.157% | 0.500% | visual-test pixel diff |
| html-blockquote | supported | MATCH | 0.319% | 0.500% | visual-test pixel diff |
| html-code | supported | MATCH | 0.144% | 0.500% | visual-test pixel diff |
| html-figure | supported | MATCH | 0.126% | 0.500% | visual-test pixel diff |
| html-progress | supported | MATCH | 0.000% | 0.500% | tests/html_progress.rs |
| html-small | supported | MATCH | 0.113% | 0.500% | visual-test pixel diff |
| input-intrinsic-width | supported | MATCH | 0.244% | 0.500% | visual-test pixel diff |
| input-placeholder | supported | MATCH | 0.349% | 0.500% | tests/input_placeholder.rs |
| react-conditional | supported | MATCH | 0.175% | 0.500% | tests/jsx_text.rs |
| react-controlled-input | supported | MATCH | 0.277% | 0.500% | visual-test pixel diff |
| react-state-list-map | supported | MATCH | 0.424% | 0.500% | meari+meari-native tests/state_list_map.rs |
| react-static-list-map | supported | MATCH | 0.029% | 0.500% | visual-test pixel diff |
| react-ternary | supported | MATCH | 0.040% | 0.500% | meari+meari-native tests/ternary.rs |
| react-use-effect | supported | MATCH | 0.008% | 0.500% | meari+meari-native tests/use_effect.rs |
| rounded-border | supported | MATCH | 0.104% | 0.500% | visual-test pixel diff |
| solid-box | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| state-counter | supported | MATCH | 0.117% | 0.500% | visual-test pixel diff |
| tw-color-border-shadow | supported | MATCH | 0.190% | 0.500% | visual-test pixel diff |
| tw-flex-layout | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| tw-grid-layout | supported | MATCH | 0.014% | 0.500% | visual-test pixel diff |
| tw-overflow-scroll-offset | supported | MATCH | 0.015% | 0.500% | tests/overflow_scroll.rs (wheel, clamp, thumb drag) |
| tw-responsive | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| tw-ring-divide-space | supported | MATCH | 0.161% | 0.500% | meari-native tests/ring_shadow.rs |
| tw-scroll-overflow | supported | MATCH | 0.008% | 0.500% | visual-test pixel diff |
| tw-spacing-sizing | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| tw-state-variants | supported | MATCH | 0.726% | 0.900% | visual-test pixel diff |
| tw-theme-tokens | supported | MATCH | 0.757% | 0.900% | visual-test pixel diff |
| tw-transform | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| tw-truncate-aspect | supported | MATCH | 0.348% | 0.500% | visual-test pixel diff |
| tw-typography | supported | MATCH | 1.430% | 1.600% | visual-test pixel diff |
| react-use-reducer | gap | KNOWN-GAP | - | 0.500% | visual-test pixel diff; parse: unregistered, aliased, type-only or duplicate component import |

### Official React / Tailwind docs examples not covered
- React: useReducer (gap fixture), useContext/createContext, useRef/forwardRef, custom hooks, list state with spread append / object items / filter, effects with dependencies or cleanup, async data fetching, Suspense/lazy, portals.
- Tailwind: transitions/animations (`transition`, `animate-*`; static frame only, no animation timing test), `backdrop-blur`/filters, `group-*`/`peer-*` variants, container queries (`@container`), arbitrary-value variants, real 15px classic scrollbars, print/motion-reduce media.
- The web reference is meari's own static render projected in headless Chrome, not live React DOM; useEffect/state behaviour equals React's committed state by construction, not by running React.

## Why

meari의 목표는 같은 TSX가 Native와 Web에서 같아 보이는 것이므로, 지원 경계를 증거(픽스처)와 함께 한 곳에 둔다.

## Verification

`cargo xtask visual-test` 결과 index.html. 자동 픽셀 비교일 뿐 사람 눈 검증이 아니다.
