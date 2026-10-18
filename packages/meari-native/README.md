# Meari Native

Native adapters for the portable `meari` core, using pinned Blitz/Stylo/Taffy/Parley, Vello CPU, optional Skia OpenGL, Winit and AccessKit. There is no WebView or JavaScript runtime in native rendering.

From the repository root:

```text
cargo test -j1 -p meari-native -- --test-threads=1
cargo run -p meari-native --bin meari-native -- --help
cargo run -p meari-native --bin meari-native -- styles packages/meari-native/fixtures/custom-css/raw.json NEW_OUTPUT_DIRECTORY
cargo run -p meari-native --bin meari-kitchen-sink -- screen-check NEW_REPORT.json
cargo run -p meari-native --features gl --bin meari-kitchen-sink -- gl-check NEW_REPORT.json
```

`InteractiveDocument` preserves DOM/input/focus during supported patches. The shared FilePicker port supports injected tests and a Windows rfd adapter. Source-owned Tabs, Accordion, Dialog, Popover, Select, DropdownMenu, Field/ButtonGroup, Checkbox/Switch and Icon contracts are bounded subsets of upstream behavior.

CSS-sized screens use `screen_element` in styles requests or `InteractiveDocument::resize_screen`. Media queries evaluate the final viewport. This native host extension does not imply CSS can resize a browser window or monitor. Current sizing uses integer CSS pixels and has explicit resource bounds; DPR/zoom/visual viewport integration remains incomplete.

GL is an opt-in host-owned vector backend on Windows; failures never count CPU fallback as GL success. Context generations, last-good readback and bounded Icon GPU cache have separate contracts. Embedding into an existing consumer GL host, forced device loss, complete provider/CSS parity and OS input/screen-reader acceptance remain unfinished.

See [framework architecture](../../docs/architecture.md) and [custom CSS fixtures](fixtures/custom-css/README.md). External corpus tests require `MEARI_KITCHEN_SINK`; Tailwind tests require `BRIDGE_TAILWIND`. Use explicit ignored-test opt-in with the required data/hardware.
