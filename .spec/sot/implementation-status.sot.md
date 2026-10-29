---
id: sot:meari/implementation-status
title: Implementation status (React + Tailwind on Native/Web)
status: draft
verify: cargo xtask visual-test
created: 2026-10-29
updated: 2026-10-29
rel:
  governs: [crates/meari-visual]
---

# Implementation status (React + Tailwind on Native/Web)

## Rule

이 표가 React + Tailwind 기능별 Native/Web 지원 현황의 정본이다. 기능을 추가·변경하면 visual-test 픽스처와 함께 이 표를 갱신한다.


Evidence: `cargo xtask visual-test` (Native blitz/vello_cpu offscreen vs headless Chrome, same Tailwind v4.3.3 CSS).
Fixtures in `crates/meari-visual/fixtures/{supported,gaps}`. Last run 2026-10-29: 32 MATCH, 0 MISMATCH, 4 KNOWN-GAP.
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
| scrollbars | 구현됨 (overlay, Chromium fade timing, thumb drag via blitz `scrollbars`); visual test compares the idle frame with Chrome `--hide-scrollbars` — classic 15px Chrome scrollbars are not reproduced | overflow_scroll.rs (opacity) |
| dark mode | 구현됨 | dark-tw-dark-mode, dark-scheme |
| state variants (hover/focus/disabled) | 부분 (input needs explicit width) | tw-state-variants |
| generic `sans-serif` family | 미구현 (Native→Noto Sans, Chrome→Arial-like) | gaps/generic-font-family |

### React / TSX

| Feature | Status | Evidence |
|---|---|---|
| useState + event handlers | 구현됨 | app-counter, state-counter |
| design components Card/Button with variant/size | 구현됨 (`asChild` rejected) | app-card-dashboard, tests/dom.rs |
| `cond && <El/>` | 구현됨 (lowered to `hidden`) | react-conditional, tests/jsx_text.rs |
| literal array `.map` | 부분 (static string arrays, unique keys, ≤1024) | react-static-list-map |
| dynamic `.map` over state | 미구현 | — |
| useEffect | 미구현 | gaps/react-use-effect |
| HTML form/a/code/blockquote/figure/small etc. | 구현됨 | form-tag, html-* |
| input `defaultValue` | 구현됨 | app-form |
| input placeholder (`::placeholder` color) | 구현됨 (meari-native projects `::placeholder{color}` onto the host as `text-decoration-color`, UA default #757575; meari-paint lays out the attribute with the editor's style when empty). Limits: only `color` from `::placeholder` (no font/opacity), placeholder layout uses system fonts only (no `@font-face`) | input-placeholder (0.349%, glyph AA), tests/input_placeholder.rs |
| input intrinsic width | 미구현 (blitz min(avail,300) shrinkable vs Chrome size=20) | gaps/input-intrinsic-width |
| `<progress>` | 미구현 | gaps/html-progress |

### Open decisions (user)
- NOTICE copy mismatch (tests/package_boundary.rs) and mixed_slots.rs:100 failure: untouched pending decision.

## Why

meari의 목표는 같은 TSX가 Native와 Web에서 같아 보이는 것이므로, 지원 경계를 증거(픽스처)와 함께 한 곳에 둔다.

## Verification

`cargo xtask visual-test` 결과 index.html. 자동 픽셀 비교일 뿐 사람 눈 검증이 아니다.
