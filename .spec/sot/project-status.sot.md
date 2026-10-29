---
id: sot:meari/project-status
title: Meari 현재 상태와 우선순위
status: active
created: 2026-10-29
updated: 2026-10-29
verify: manual
---

# Meari 현재 상태와 우선순위

## Rule

Meari의 현재 상태를 적는 단일 정본이다. 기준은 커밋 bcd4693(2026-10-29 KST)이다.

- **우선순위(사용자 지시 2026-10-29):** Meari 기능 완성이 최우선이다(UI 중심 프레임워크). React+TailwindCSS 코드를 Rust Native에서 Web과 같게 렌더하고 시각 테스트로 확인한다. Linux 동작 우선, Windows 검증은 보류.
- **시각 테스트(bcd4693, 자동 픽셀 비교):** 일치 16, 불일치 5, 오류 2, 알려진 미지원 11.
- **알려진 실패 2건:** `mixed_slots.rs:100`, NOTICE 차이. 둘 다 사용자 결정 대기.
- **리팩터링:** 브랜치 `refactor/structure-phase`에서 진행. 기록은 [[plan:meari/20261029124634]], 목표 구조는 [[plan:meari/20261029135912]], 규칙은 [[sot:meari/conventions]].
- 구현 현황은 [[sot:meari/implementation-status]]가 정본이다 (docs/specs에서 이전됨).

## Why

상태를 한 곳에만 적어 문서끼리 어긋나지 않게 한다.

## Verification

사람이 검토한다. 숫자는 `cargo xtask visual-test` 보고서로 확인한다.
