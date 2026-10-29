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
Fixtures in `crates/meari-visual/fixtures/{supported,gaps}`. Last run 2026-10-30: 48 MATCH, 0 MISMATCH, 0 KNOWN-GAP (48 fixtures).
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
| wheel scroll, clamping, `overflow:hidden` ignores wheel | 구현됨 (blitz scroll chain) | meari-native tests/rendering/overflow_scroll.rs |
| ring / divide / space-x | 구현됨 (spread-only box-shadow painted as solid fill; 0.161%) | supported/tw-ring-divide-space, tests/rendering/ring_shadow.rs |
| transforms (rotate/translate/scale) | 구현됨 (0.000%) | supported/tw-transform |
| truncate / aspect-ratio / opacity | 구현됨 (0.348%) | supported/tw-truncate-aspect |
| scrollbars | 구현됨 (overlay, Chromium fade timing, thumb drag tested); visual test compares the idle frame with Chrome `--hide-scrollbars` — classic 15px Chrome scrollbars are not reproduced. 방침(2026-10-30): 비교 기준은 overlay(Chrome `--hide-scrollbars` idle frame)로 고정한다. 근거: meari native는 overlay 스크롤바(Chromium fade)를 정본으로 구현하고, classic 15px gutter는 layout width를 바꾸는 별도 모드라 정본이 아니다. classic 모드는 미구현으로 남긴다 | overflow_scroll.rs (opacity, thumb drag) |
| dark mode | 구현됨 | dark-tw-dark-mode, dark-scheme |
| state variants (hover/focus/disabled) | 구현됨 | tw-state-variants |
| `group-*` / `peer-*` variants | 부분: `group-hover:*` (idle frame), `group-[.cls]:*`, `peer-[.cls]:*` MATCH (1.271%, glyph AA tolerance 1.3% with bbox evidence). `peer-checked:*` with host `<input defaultChecked>` MATCH 0.350% at default 0.5% (defaultChecked → `checked` attribute seeds `:checked`) | supported/tw-group-peer, supported/tw-peer-checked |
| transition / animation | 방침: 정적 비교만. Static render has no running timeline, so the comparison is the start state (= end state for hover-triggered transitions with no hover); `transition-*`/`duration-*` parse and are inert. `animate-spin/ping/pulse/bounce`: native does not run keyframes; not pixel-compared (frame 0 vs Chrome frame depends on capture time) — 미구현으로 기록 | (no fixture by design) |
| indeterminate `<progress>` (no `value`) | 방침: 미지원 비교. Chrome animates the indeterminate bar, so no deterministic frame exists; native paints the empty track. Only determinate `value`/`max` is MATCH | supported/html-progress |
| generic `sans-serif`/`serif` family | 구현됨: 정본은 Pretendard Variable (+JP Regular/Bold) / Source Serif 4 + Source Han Serif KR (Web: Meari Serif KR slices) (sot:meari/fonts); Chrome 비교 경로만 Chromium Linux defaults (Arimo/Tinos 호스트 폴백, 0.866% glyph AA) | supported/generic-font-family, supported/canonical-font |

### React / TSX

| Feature | Status | Evidence |
|---|---|---|
| useState + event handlers | 구현됨 | app-counter, state-counter |
| design components Card/Button with variant/size | 구현됨 (`asChild` rejected) | app-card-dashboard, tests/environment/dom.rs |
| `cond && <El/>` | 구현됨 (lowered to `hidden`) | react-conditional, tests/sources/jsx_text.rs |
| literal array `.map` | 부분 (static string arrays, unique keys, ≤1024) | react-static-list-map |
| dynamic `.map` over state | 부분 (dense non-empty string-array state, keyed; setter with full array literal re-expands; `[...xs, "lit"]` append and `xs.filter(x => x !== "lit")` remove (direct or updater) on the setter's own state; duplicate keys after append are rejected at dispatch with state unchanged; object items, computed items (e.g. input text) and other filter predicates rejected; 0.424% / append-filter 0.794% glyph AA tolerance 0.9%) | supported/react-state-list-map, supported/react-list-append-filter, tests/hooks/state_list_map.rs |
| ternary `cond ? <A/> : <B/>` | 구현됨 (both branches JSX elements, lowered to `hidden`; 0.040%) | supported/react-ternary, tests/hooks/ternary.rs |
| controlled input (`value` + `onChange`) | 구현됨 (0.277%) | supported/react-controlled-input |
| useReducer | 구현됨 (부분): module-level `function reducer(state, action)`; branch chosen at compile time by literal action (`"x"` or `{ type: "x" }`) via ternary chains, `if` returns, `switch(action / action.type)` with fall-through/default; branch lowered to a state updater. Payload actions (`action.by`), non-literal dispatch, reducers reading free variables, and actions reaching `throw` are rejected (0.042% / 0.410%) | supported/react-use-reducer, supported/react-use-reducer-switch, meari-native/tests/hooks/use_reducer.rs |
| useEffect with deps `[x]` or cleanup | 미구현 (거부: "only mount-only useEffect(..., []) supported" / "effect body supports only state setter calls"). 설계 한계: effects are folded into initial state at compile time; there is no runtime effect scheduler to re-run on dep change or call cleanup | parse rejection |
| useRef / useContext / createContext | 미구현 (import rejected). 설계 한계: no ref objects (DOM handles) and no provider tree in the static lowering; needs a runtime hook layer | parse rejection |
| useEffect | 부분 (mount-only `[]`, setter literal/toggle/add, 0.008%) | supported/react-use-effect, meari/tests/hooks/use_effect.rs, meari-native/tests/hooks/use_effect.rs |
| HTML form/a/code/blockquote/figure/small etc. | 구현됨 | form-tag, html-* |
| input `defaultValue` | 구현됨 | app-form |
| input placeholder (`::placeholder` color) | 구현됨 (meari-native projects `::placeholder{color}` onto the host as `text-decoration-color`, UA default #757575; meari-paint lays out the attribute with the editor's style when empty). Limits: only `color` from `::placeholder` (no font/opacity), placeholder layout uses system fonts only (no `@font-face`) | input-placeholder (0.349%, glyph AA), tests/pages/input_placeholder.rs |
| input intrinsic width | 구현됨 (Chromium size=20, 0.244%) | supported/input-intrinsic-width |
| `<progress>` | 구현 (native/fallback 두 모양, 0.000%) | supported/html-progress, tests/rendering/html_progress.rs |


### Examples status (every fixture, run 2026-10-30)

| Example (fixture) | Set | Status | Diff % | Threshold | Evidence test |
|---|---|---|---|---|---|
| app-card-dashboard | supported | MATCH | 0.608% | 0.800% | tests/environment/dom.rs |
| app-counter | supported | MATCH | 0.124% | 0.500% | visual-test pixel diff |
| app-form | supported | MATCH | 0.867% | 1.000% | visual-test pixel diff |
| app-list | supported | MATCH | 0.350% | 0.500% | visual-test pixel diff |
| button-input | supported | MATCH | 0.311% | 0.500% | visual-test pixel diff |
| dark-scheme | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| dark-tw-dark-mode | supported | MATCH | 0.117% | 0.500% | visual-test pixel diff |
| flex-row | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| form-tag | supported | MATCH | 0.311% | 0.500% | visual-test pixel diff |
| generic-font-family | supported | MATCH | 0.866% | 1.000% | visual-test pixel diff |
| canonical-font | supported | MATCH | 1.597% | 1.700% | visual-test pixel diff + per-line ink bbox |
| grid-cards | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| heading-text | supported | MATCH | 0.764% | 0.900% | visual-test pixel diff |
| html-a | supported | MATCH | 0.157% | 0.500% | visual-test pixel diff |
| html-blockquote | supported | MATCH | 0.319% | 0.500% | visual-test pixel diff |
| html-code | supported | MATCH | 0.144% | 0.500% | visual-test pixel diff |
| html-figure | supported | MATCH | 0.126% | 0.500% | visual-test pixel diff |
| html-progress | supported | MATCH | 0.000% | 0.500% | tests/rendering/html_progress.rs |
| html-small | supported | MATCH | 0.113% | 0.500% | visual-test pixel diff |
| input-intrinsic-width | supported | MATCH | 0.244% | 0.500% | visual-test pixel diff |
| input-placeholder | supported | MATCH | 0.349% | 0.500% | tests/pages/input_placeholder.rs |
| react-conditional | supported | MATCH | 0.175% | 0.500% | tests/sources/jsx_text.rs |
| react-controlled-input | supported | MATCH | 0.277% | 0.500% | visual-test pixel diff |
| react-state-list-map | supported | MATCH | 0.424% | 0.500% | meari+meari-native tests/hooks/state_list_map.rs |
| react-static-list-map | supported | MATCH | 0.029% | 0.500% | visual-test pixel diff |
| react-ternary | supported | MATCH | 0.040% | 0.500% | meari+meari-native tests/hooks/ternary.rs |
| react-use-effect | supported | MATCH | 0.008% | 0.500% | meari+meari-native tests/hooks/use_effect.rs |
| rounded-border | supported | MATCH | 0.104% | 0.500% | visual-test pixel diff |
| solid-box | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| state-counter | supported | MATCH | 0.117% | 0.500% | visual-test pixel diff |
| tw-color-border-shadow | supported | MATCH | 0.190% | 0.500% | visual-test pixel diff |
| tw-flex-layout | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| tw-grid-layout | supported | MATCH | 0.014% | 0.500% | visual-test pixel diff |
| tw-overflow-scroll-offset | supported | MATCH | 0.015% | 0.500% | tests/rendering/overflow_scroll.rs (wheel, clamp, thumb drag) |
| tw-responsive | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| tw-ring-divide-space | supported | MATCH | 0.161% | 0.500% | meari-native tests/rendering/ring_shadow.rs |
| tw-scroll-overflow | supported | MATCH | 0.008% | 0.500% | visual-test pixel diff |
| tw-spacing-sizing | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| tw-state-variants | supported | MATCH | 0.726% | 0.900% | visual-test pixel diff |
| tw-theme-tokens | supported | MATCH | 0.757% | 0.900% | visual-test pixel diff |
| tw-transform | supported | MATCH | 0.000% | 0.500% | visual-test pixel diff |
| tw-truncate-aspect | supported | MATCH | 0.348% | 0.500% | visual-test pixel diff |
| tw-typography | supported | MATCH | 1.430% | 1.600% | visual-test pixel diff |
| react-list-append-filter | supported | MATCH | 0.794% | 0.900% | tests/hooks/state_list_map.rs |
| react-use-reducer | supported | MATCH | 0.042% | 0.500% | meari-native tests/hooks/use_reducer.rs |
| react-use-reducer-switch | supported | MATCH | 0.410% | 0.500% | meari-native tests/hooks/use_reducer.rs |
| tw-group-peer | supported | MATCH | 1.271% | 1.300% | visual-test pixel diff (bbox equal) |
| tw-peer-checked | supported | MATCH | 0.350% | 0.500% | fixed 2026-10-30 |

### Official React / Tailwind docs examples not covered
Inventory 2026-10-30 (react.dev Learn/Reference, tailwindcss.com docs). Order = 영향 큰 순, status as of this run:
- React react.dev "Extracting State Logic into a Reducer" / useReducer reference counter: 처리됨 (string + `{type}` actions; payload actions 미구현).
- React "Updating Arrays in State" (add with spread, remove with filter): 처리됨 for string literals; object items / map-replace / insert-at-index 미구현.
- React "Rendering Lists" with object items (`people.map(person => <li key={person.id}>{person.name}</li>)`): 미구현 — static object-array literal and object-list state rejected ("only string/bool literals…", "repeat state source requires a non-empty string array literal").
- React "Synchronizing with Effects" (deps, cleanup): 미구현 (see table).
- React useRef (focus input, counter ref), useContext ("Passing Data Deeply with Context"): 미구현.
- React custom hooks, async data fetching, Suspense/lazy, portals, forwardRef: 미구현.
- Tailwind `group-*`/`peer-*`: 처리됨 (`peer-checked` 포함, 2026-10-30).
- Tailwind transitions/animations, indeterminate progress, 15px classic scrollbars: 방침 기록 (see table), not pixel-compared.
- Tailwind `backdrop-blur`/filters, container queries (`@container`), print/motion-reduce media: 미확인 (no fixture yet).
- The web reference is meari's own static render projected in headless Chrome, not live React DOM; useEffect/state behaviour equals React's committed state by construction, not by running React.

## Why

meari의 목표는 같은 TSX가 Native와 Web에서 같아 보이는 것이므로, 지원 경계를 증거(픽스처)와 함께 한 곳에 둔다.

## Verification

`cargo xtask visual-test` 결과 index.html. 자동 픽셀 비교일 뿐 사람 눈 검증이 아니다.
