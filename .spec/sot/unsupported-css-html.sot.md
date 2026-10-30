---
id: sot:meari/unsupported-css-html
title: 미지원 CSS/HTML 기능과 렌더러 미구현 목록
status: draft
created: 2026-10-30T04:15:55Z
updated: 2026-10-30T04:15:57Z
---

# 미지원 CSS/HTML 기능과 렌더러 미구현 목록

## Rule

Native 렌더러(벤더링한 blitz-dom `crates/meari-dom`, blitz-paint `crates/meari-paint`)에서 Chrome과 다르게 동작하거나 구현되지 않은 CSS/HTML 기능의 정본 목록이다. 근거는 소스 TODO([[plan:meari/20261030041555]])와 visual-test([[sot:meari/implementation-status]])다. 기능을 구현하면 visual-test 픽스처를 추가하고 이 표에서 지운다. React/JS 기능 범위는 implementation-status에 있다.

### CSS

| 기능 | 상태 | 근거 |
|---|---|---|
| 3D transform, `offset` 속성 | 미지원(2D만) | styling/stylo_to_kurbo.rs |
| `vertical-align: center`, `normal`의 첫 폰트 메트릭 해석 | 근사 | stylo_to_parley/font_conversion.rs |
| `direction` 기반 인라인 측(side) | 미지원(LTR 가정) | layout/inline/inline_layout_pass.rs |
| `display: contents` | 불완전 | layout/construct/complex_children.rs |
| container queries | 비활성(무시, 패닉 없음) | stylo/element_traversal.rs |
| Shadow DOM 스타일(`:host`, shadow root 스타일시트) | 미지원 | stylo/document_styling.rs |
| `@font-face { src: local() }` | 미지원 | net/providers.rs |
| `text-transform: full-width` 공백(U+3000) | 미지원 | layout/text_transform/transforms.rs |
| `box-shadow` 모서리별 반경 | 평균 반경으로 근사 | render/effects/box_shadow.rs |
| `mask-mode: luminance` | 미지원(alpha만) | render/effects/mask.rs |
| `clip-path: inset(... round r)` | 반경 무시 | render/effects/clip_path.rs |
| `background-clip: border-area` | 미지원 | render/background/layers.rs |
| 테두리 dashed/dotted 변별 색, 기타 border-style | 부분 | render/border/dashed_edges.rs |
| `accent-color` (폼 컨트롤) | 미지원(고정색) | render/form_controls.rs |
| filter 안의 `currentColor` | 부정확 | filters.rs |
| `overflow-x`≠`overflow-y` 클립, opacity 레이어의 `overflow: visible` | 부분 | render/element_tree.rs |
| `mix-blend-mode`, `contain`, `z-index` 히트 테스트 | 히트 테스트 미반영(페인트는 별개) | node/node/hit_testing.rs |
| 서브문서(iframe) 임의 transform | 미지원 | render/replaced_content.rs |
| 큰 반경 겹침, 0에 가까운 gradient background-size | 렌더 오차 가능 | kurbo_css/css_box/paths.rs, render/background/image_layers.rs |
| 애니메이션·transition 타임라인, indeterminate `<progress>` | 정적 비교만 | implementation-status |

### HTML / DOM / 상호작용

| 기능 | 상태 | 근거 |
|---|---|---|
| Shadow DOM / custom element 폼 연동 | 미지원 | stylo/, content/form.rs |
| 폼 제출: `<select>` 값, 이름 접두, 다중 인코딩(UTF-8만), 스크립트 submit 이벤트 | 부분 | content/form.rs, events/keyboard.rs |
| `<input type=file accept>` | accept 무시 | events/pointer/pointer_release.rs |
| `input` 이벤트(값 변화 무관 발생), `keypress`, 텍스트 입력 클릭 카운트 | 부정확/미지원 | events/, node/text/ |
| 텍스트 편집 Bidi 인식 커서 이동, 결합 문자 Backspace 분해 | 미지원 | node/text/keybindings.rs |
| 컨텍스트 메뉴 | 미지원 | events/mod.rs |
| 프로그램 스크롤의 `scroll` 이벤트, 바깥 문서로 스크롤 전파 | 미지원 | scrolling/ |
| pointerenter/pointerleave 합성 | 미지원 | document/hover_viewport.rs |
| ARIA 강한 네이티브 의미 역할 덮어쓰기 금지 | 미적용 | content/accessibility.rs |
| `innerHTML` 류 HTML 파싱 설정 | textContent로 근사 | mutator/node_creation.rs |
| 위젯/서브문서 가시성 고려 페인트 | 미지원 | meari-paint lib.rs |
