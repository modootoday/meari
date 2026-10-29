# Meari Builder

Host-side Rust app compiler and CLI. Portable build/state contracts belong to `meari`; native CSS/window adapters belong to `meari-native`. Generated applications depend only on the runtime for their selected target.

Compiler adapters and real bundle acceptance are in development. Tool registration is trusted host configuration, separate from editable component/CSS input. Generated output and tool downloads do not belong in public source.

`meari-builder build REQUEST.json` calls `application::build::execute`. The request declares project/framework/work/target/store roots, the source file graph, Native/Web targets and trusted executable SHA256 registrations. App configuration supplies language, title, initial Native viewport, props, original CSS/Tailwind files and theme. Tool paths belong to the host request, never component source. Current builds use the observed native host target and `wasm32-unknown-unknown`, debug profile, offline dependencies and one build job. Additional modules/assets and native cross compilation require registered adapters.

Generated Native apps use one document factory for the visible window and `--snapshot --action NODE_PATH` CLI. Web apps retain an owned Rust DOM mount; Trunk generates the WASM loader. Both include the same source graph/generation stamp. All requested targets must prepare before the immutable store publishes a new last-good pointer. Publication alone is not an HMR frame acknowledgement.

`meari-builder serve STORE_ROOT PORT SECONDS` serves the verified published Web bundle on loopback for a bounded period. It fixes that publication for the process lifetime. Automatic rebuilding, editor transport, synchronized host replacement and complete widget/accessibility parity remain unfinished.
