# Native 제어형 Boolean 위젯

Native module 문서는 다음 프로필을 지원합니다. 전체 React/shadcn API나 Web 위젯 지원을 의미하지 않습니다.

| 위젯 | 콜백 | 명시해야 하는 상태 |
|---|---|---|
| Dialog | onOpenChange | open: bool |
| AlertDialog | onOpenChange | open: bool |
| Switch | onCheckedChange | checked: bool |
| Checkbox | onCheckedChange | checked: bool |

`defaultOpen`·`defaultChecked`만 지정하는 uncontrolled 콜백은 이 프로필에 포함되지 않습니다. Checkbox의 indeterminate, 일반 child handler 합성, anchor/nested asChild와 다른 위젯의 Boolean 이벤트는 진단합니다. Web composite, 실제 Portal host 이동·종료 animation과 전체 HMR는 별도 구현·검증 범위입니다.

## 호스트 연결

`ModuleProgram`의 root props에 `CallbackSignature::Boolean`인 `CallbackReference`와 bool 값을 전달합니다. Native 진입점은 `InteractiveDocument::from_module_program_with_callbacks`입니다. export closure가 받은 실제 mount `PluginScope`로 `HostCallbacks`와 기존 `PluginRegistry`의 등록을 연결하고, typed Boolean binding을 만든 뒤 `export_bindings()` 결과를 반환합니다. 호스트는 registry와 `HostCallbacks`를 문서보다 오래 보유해야 합니다.

문서는 callback producer나 registry를 렌더러 이벤트 처리 중에 호출하지 않습니다. `drain_callbacks()`로 꺼낸 요청의 handle과 `CallbackArguments::Boolean(value)`를 같은 호스트의 `HostCallbacks::dispatch`에 전달합니다. 이를 사용자 저장으로 자동 해석하지 않고 호스트 application 유스케이스에서 승인·거부합니다.

승인 시 `update_module_props_with_callbacks(next_props, next_export)`로 상태와 새 export를 함께 반영합니다. 요청 대기·거부 중에는 open/checked·aria/data-state·focus를 낙관 반전하지 않습니다. 승인에 실패하면 이전 props/program/DOM·포커스와 export를 유지합니다. 성공하면 이미 drain한 이전 export의 handle도 만료됩니다. callback 문서에 export 없이 일반 `update_module_props`를 호출하는 방식은 거부합니다.

## 입력과 DOM 수명

Dialog는 trigger/close·Escape·자신의 overlay 요청을 처리합니다. modal은 포커스를 콘텐츠로 옮기고 배경 입력과 Tab을 제한하며, nonmodal은 배경을 inert로 만들거나 Tab을 가두지 않습니다. AlertDialog는 action/cancel 승인을 요구하고 Escape/outside로 자동 닫히지 않습니다. Switch/Checkbox는 pointer·Space·AccessKit 요청을 처리하며 Enter로 값을 바꾸지 않습니다.

Dialog/AlertDialog의 지원된 Trigger·Close 계열은 정확히 하나의 Button/button 자식에 대한 `asChild`를 지원합니다. wrapper와 child source path는 실제 Button NodeId 하나에 매핑됩니다. child id/class·disabled와 원래 descendant path를 유지하며, 같은 id/path를 가진 다른 DOM 노드는 기존 콜백 권한을 얻지 못합니다. child 또는 primitive에 추가 이벤트 handler가 있으면 원자적 합성 계약이 없는 상태로 실행하지 않고 진단합니다.

Dialog `forceMount`는 닫힌 콘텐츠의 유지와 표시 상태를 구분하며 명시한 `hidden`을 보존합니다. Portal의 forceMount 기본값을 Content/Overlay가 상속할 수 있지만 실제 다른 host로 이동하는 Portal이나 Presence animation 전체를 구현했다는 뜻은 아닙니다. retained patch는 관련 없는 입력의 dirty 값과 NodeId를 보존합니다.

## 검증 범위

공개 module factory를 사용하는 Native CPU DOM 시험에서 원본 InspectorDock의 controlled/nonmodal/asChild/forceMount 조합, 키보드·포인터·AccessKit, dirty 슬롯, host accept/reject·stale export와 실패 복구를 확인했습니다. Checkbox의 accepted a11y 상태와 AlertDialog의 action/cancel·비자동 dismiss·focus 복귀도 별도 시험합니다. 실제 OS keyboard/IME·screen reader·제품 SDK 소비·WASM/Web·생성 Rust 실행과 공식 전체 kitchen-sink는 이 결과에 포함되지 않습니다.

module 내부 이벤트나 keyed 구조 변경이 callback path/reference까지 바꾸는 경우의 export 재구성은 추가 작업입니다. 현재 고정된 controlled Dock props 갱신을 모든 graph HMR의 원자성 보증으로 해석하지 마세요.
