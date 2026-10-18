# Meari

Rust UI framework and WASM-based Native+Web builder under development. Meari shares typed UI contracts across source conversion, native rendering and browser adapters. CLI tools exercise those same APIs.

## Packages

| Package | Responsibility |
| --- | --- |
| `meari` | Portable domain/application, bounded Rust/TSX conversion, shared media/HMR contracts, optional `wasm` adapter |
| `meari-native` | Native CSS/layout/paint, widget interactions, accessibility and OS adapters |

Both live under `packages/` in one Cargo workspace. The default member is `meari`; native dependencies are opt-in. Web adapters currently use `meari`'s `wasm` feature. A separate `meari-web` package is reserved for a future independent Web runtime, not an empty placeholder.

```text
cargo test -j1
cargo clippy -j1 --all-targets -- -D warnings
cargo test -j1 -p meari-native -- --test-threads=1
cargo check -p meari --target wasm32-unknown-unknown --features wasm
cargo run -p meari --bin meari-cli -- --help
cargo run -p meari-native --bin meari-native -- --help
```

## Current support

The core exposes `parse`, `emit`, `convert`, `prepare_native` and injectable application ports. Generated Rust imports `meari::runtime`. Typed string/bool props, bounded `useId`, source mappings and diagnostics are supported. General React hooks/effects/events/control flow and arbitrary Rust/React equivalence remain incomplete.

Tailwind CSS and shadcn/ui are the default design profile. Native adapters consume compiled CSS and source-owned widget contracts. Direct custom CSS, CSS-first Tailwind configuration, semantic SVG Icon tokens, persistent input/focus and selected composite interactions are implemented. Windows has the verified native host; complete CSS/provider parity and Linux/macOS native acceptance remain unfinished.

The `wasm` feature exposes conversion APIs and browser media-query evaluation. It does not yet provide a complete Web DOM runtime or Native+Web builder. Shared dual-target HMR coordination exists; browser editor transport and both real runtime commit paths remain unfinished. See [architecture and support boundaries](docs/architecture.md).

## Native verification

`meari-kitchen-sink` is a batch native verifier, with CPU and optional Windows `gl` paths. `screen-check REPORT.json` verifies CSS canvas sizing/media changes; `screen-check-gl` requires `--features gl`. Source-owned fixtures ship with the native package. Official documentation corpora, compiler downloads and generated evidence are separate local inputs, not public source assets.

External compiler tests require the pinned Tailwind standalone executable via `BRIDGE_TAILWIND`. Official corpus tests additionally require `MEARI_KITCHEN_SINK`. These tests are explicitly ignored by default; use `--include-ignored` only with those inputs. Hardware GL tests require an actual compatible context. Skipped tests are not compatibility evidence.

## Distribution

License: MIT OR Apache-2.0. See LICENSE-MIT, LICENSE-APACHE and NOTICE. The intended repository is `modootoday/meari`; remote publication is being prepared. Cargo publication remains disabled while packaging and framework acceptance are incomplete.
