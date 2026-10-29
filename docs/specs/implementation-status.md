# Implementation status (React + Tailwind on Native/Web)

Evidence: `cargo xtask visual-test` (Native blitz/vello_cpu offscreen vs headless Chrome, same Tailwind v4.3.3 CSS).
Fixtures in `crates/meari-visual/fixtures/{supported,gaps}`. Last run 2026-10-29: 30 MATCH (6 via `.tolerance`), 0 MISMATCH, 5 KNOWN-GAP.
This is automated pixel comparison only; **no human visual check, no Windows, no real wasm-bindgen glue run**.

## Tailwind

| Feature | Status | Evidence |
|---|---|---|
| spacing / sizing | 구현됨 | tw-spacing-sizing |
| flex / grid layout | 구현됨 | tw-flex-layout, tw-grid-layout, flex-row, grid-cards |
| color, border, radius, shadow | 구현됨 | tw-color-border-shadow, rounded-border, solid-box |
| gradients (`in oklab`) | 구현됨 | peniko interpolation_cs mapping |
| typography | 부분 (text AA tolerance 1.6%) | tw-typography.tolerance |
| theme tokens | 구현됨 (AA tolerance) | tw-theme-tokens |
| responsive breakpoints | 구현됨 | tw-responsive |
| overflow / scroll | 구현됨 | tw-scroll-overflow |
| dark mode | 구현됨 | dark-tw-dark-mode, dark-scheme |
| state variants (hover/focus/disabled) | 부분 (input needs explicit width) | tw-state-variants |
| generic `sans-serif` family | 미구현 (Native→Noto Sans, Chrome→Arial-like) | gaps/generic-font-family |

## React / TSX

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
| input placeholder | 미구현 (blitz paints none) | gaps/input-placeholder |
| input intrinsic width | 미구현 (blitz min(avail,300) shrinkable vs Chrome size=20) | gaps/input-intrinsic-width |
| `<progress>` | 미구현 | gaps/html-progress |

## Open decisions (user)
- NOTICE copy mismatch (tests/package_boundary.rs) and mixed_slots.rs:100 failure: untouched pending decision.
