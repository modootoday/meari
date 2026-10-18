# Architecture and support boundaries

Meari is a Rust UI framework and Native+Web builder in development. It is maintained in an independent Git repository, intended for consumption through a pinned dependency/submodule. A nested checkout without a registered remote/gitlink is not a completed submodule distribution.

## Dependency direction

`meari` owns portable domain values, source IR, validation and application ports. Parser/code-generation and browser WASM bindings are adapters. `meari-native` depends on the core and owns native CSS/layout/paint/window/file-picker/accessibility adapters. Neither owns consuming-product business policies. CLI and GUI hosts call the same application APIs.

The default workspace build includes only the core. Native engine dependencies are selected explicitly. Web currently uses the core's `wasm` feature; a dedicated Web package will be introduced when DOM/runtime and delivery responsibilities justify it. Package separation does not require immediate independent registry releases.

## Source and runtime limits

Conversion preserves only its diagnosed subset: string/bool props, bounded single-root `useId`, source-owned UI elements and source maps. Unsupported state, hooks, expressions, conditions/repeats or events are rejected rather than silently discarded. `useState`/effects and a general instance/key lifecycle remain future work.

Native widget state and persistent DOM are implemented for selected source-owned contracts. They are not complete upstream React/shadcn provider execution. Full Tailwind/CSS rendering, all widgets, actual OS input and cross-platform accessibility are ongoing acceptance work.

WASM conversion and browser `matchMedia` are implemented. Browser DOM mount/patch/events, complete Native/Web runtime parity, build-target/asset bundles and automatic responsive scheduling remain incomplete.

## Revision and resource lifetimes

The shared HMR coordinator publishes one revision only after both targets prepare, and accepts synchronized success only after both frame acknowledgements. Failure retains the last common successful publication and requires both rollback acknowledgements. Browser editor/source transport, actual two-host commit and state migration still need integration.

GL contexts/resources belong to native hosts. Context generation, scene revision, component/build revision and asset revision are distinct. Icon cache keys include host/generation, canonical asset hash, resolved style, token specification and scale. Native DLL reload proof, GPU lifecycle proof and integrated Native/Web HMR are separate scopes.

## Verification and publication

Public fixtures and tests are source-owned. Third-party corpora/compiler downloads/hardware tests are explicit external inputs. Passing conversion tests or offscreen rendering does not prove complete Web runtime, visual parity or OS accessibility.

Internal specifications, research and captures stay local. Public source is allowlisted. Existing private local Git history must not be pushed wholesale; initial public history/export requires its own review. Executable bundles require dependency/font/asset license review in addition to the project's dual license.
