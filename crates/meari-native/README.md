# Meari Native

Native adapters for the portable `meari` core, using pinned Blitz/Stylo/Taffy/Parley, Vello CPU, optional Skia OpenGL, Winit and AccessKit. There is no WebView or JavaScript runtime in native rendering.

From the repository root:

```text
cargo test -j1 -p meari-native -- --test-threads=1
cargo run -p meari-native-tools --bin meari-native -- --help
cargo run -p meari-native-tools --bin meari-native -- styles packages/meari-native/fixtures/custom-css/raw.json NEW_OUTPUT_DIRECTORY
cargo run -p meari-native-tools --bin meari-kitchen-sink -- screen-check NEW_REPORT.json
cargo run -p meari-native-tools --features gl --bin meari-kitchen-sink -- library-check-gl TAILWIND_EXE NEW_OUTPUT_DIRECTORY
cargo run -p meari-native-tools --features gl --bin meari-kitchen-sink -- gl-check NEW_REPORT.json
```

The executable hosts belong to `meari-native-tools`; their executable names and arguments are unchanged. Library dependencies, public `meari_native` APIs and native library tests remain in this package. See the [tools package](../meari-native-tools/README.md) for installation and host feature selection.

`InteractiveDocument` preserves DOM/input/focus during supported patches. The shared FilePicker port supports injected tests and a Windows rfd adapter. Source-owned Tabs, Accordion, Dialog, Popover, Select, DropdownMenu, Field/ButtonGroup, Checkbox/Switch and Icon contracts are bounded subsets of upstream behavior.

제어형 Dialog·AlertDialog·Switch·Checkbox의 공개 module factory, 승인·거부와 callback export 수명은 [Native Boolean 위젯 계약](../../docs/native-controlled-widgets.md)을 참고하세요. Web/전체 shadcn 지원과 구분합니다.

Primitive source `useState` and typed literal/functional Button setters execute in the persistent document. Mixed text runs and disabled state patch existing nodes. `library-check[-gl]` runs library-owned stories, including the source-state fixture, without requiring a downloaded official corpus; it does not prove upstream or OS accessibility completeness.

CSS-sized screens use `screen_element` in styles requests or `InteractiveDocument::resize_screen`. Media queries evaluate the final viewport. This native host extension does not imply CSS can resize a browser window or monitor. Current sizing uses integer CSS pixels and has explicit resource bounds; DPR/zoom/visual viewport integration remains incomplete.

GL is an opt-in host-owned vector backend on Windows; failures never count CPU fallback as GL success. Context generations, last-good readback and bounded Icon GPU cache have separate contracts. Embedding into an existing consumer GL host, forced device loss, complete provider/CSS parity and OS input/screen-reader acceptance remain unfinished.

See [framework architecture](../../docs/architecture.md) and [custom CSS fixtures](fixtures/custom-css/README.md). External corpus tests require `MEARI_KITCHEN_SINK`; Tailwind tests require `BRIDGE_TAILWIND`. Use explicit ignored-test opt-in with the required data/hardware.
