---
title: The crates
description: "Which Meari packages to depend on, and what each one is for."
---

# The crates

Meari is split into small packages so that an app only pulls in what it renders. All of them share one version number, so upgrading means moving them together. Most apps need just one or two.

## What most apps need

- **`meari`** is the core and the only crate every user needs. It holds the typed UI model, Rust/TSX conversion, the shared media and hot-reload contracts, the font manifest, and the browser adapter behind the `wasm` feature.
- **`meari-native`** renders that model in a native window: CSS, layout and paint, widget interactions, window actions such as `BeginResize`, accessibility, and the OS adapters.
- **`meari-cli`** installs the `meari` command line.

## Tools and building

- **`meari-builder`** loads projects on the host, runs the registered compiler adapters, and produces immutable app bundles. It also has a build and serve command line.
- **`meari-native-tools`** ships two programs: `meari-native`, a showcase host, and `meari-kitchen-sink`, a batch verifier for native rendering.
- **`meari-platform`** is where operating-system differences live. Every OS-specific choice is made here, once, so the rest of the code stays portable.

## Rendering engine

Meari's native renderer is built on the Blitz engine. To fix layout and paint details without asking you to patch dependencies, Meari publishes its own copies:

- **`meari-dom`** and **`meari-paint`** are forks of `blitz-dom` and `blitz-paint` 0.3.0-beta.4 with Meari's layout and paint corrections.
- **`meari-html`** and **`meari-shell`** are unmodified copies of `blitz-html` and `blitz-shell`, republished so they build against those forks without `[patch]` sections.

> [!TIP]
> Since 0.2.0, `meari-shell` turns on no features by default. If you use it directly, enable `accessibility`, `clipboard` or `file-dialog` as you need them.

## Published crates

| Crate | Version | What it is | Links |
| --- | --- | --- | --- |
| [`meari`](https://crates.io/crates/meari) | 0.3.0 | The core: typed UI model, Rust/TSX conversion, media and hot-reload contracts, fonts, and the browser adapter. | [docs.rs](https://docs.rs/meari) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari) |
| [`meari-native`](https://crates.io/crates/meari-native) | 0.3.0 | Renders the model in a native window: CSS, layout, paint, interaction and accessibility. | [docs.rs](https://docs.rs/meari-native) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-native) |
| [`meari-cli`](https://crates.io/crates/meari-cli) | 0.3.0 | The `meari` command line. | [docs.rs](https://docs.rs/meari-cli) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-cli) |
| [`meari-builder`](https://crates.io/crates/meari-builder) | 0.3.0 | Loads projects, runs compiler adapters and produces app bundles. | [docs.rs](https://docs.rs/meari-builder) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-builder) |
| [`meari-native-tools`](https://crates.io/crates/meari-native-tools) | 0.3.0 | The `meari-native` showcase host and the `meari-kitchen-sink` verifier. | [docs.rs](https://docs.rs/meari-native-tools) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-native-tools) |
| [`meari-platform`](https://crates.io/crates/meari-platform) | 0.3.0 | Every operating-system difference, in one place. | [docs.rs](https://docs.rs/meari-platform) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-platform) |
| [`meari-dom`](https://crates.io/crates/meari-dom) | 0.3.0 | Fork of `blitz-dom` with Meari's layout fixes. | [docs.rs](https://docs.rs/meari-dom) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-dom) |
| [`meari-paint`](https://crates.io/crates/meari-paint) | 0.3.0 | Fork of `blitz-paint` with Meari's paint fixes. | [docs.rs](https://docs.rs/meari-paint) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-paint) |
| [`meari-html`](https://crates.io/crates/meari-html) | 0.3.0 | Unmodified `blitz-html`, built against the forks. | [docs.rs](https://docs.rs/meari-html) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-html) |
| [`meari-shell`](https://crates.io/crates/meari-shell) | 0.3.0 | Unmodified `blitz-shell`, built against the forks. | [docs.rs](https://docs.rs/meari-shell) · [Source](https://github.com/modootoday/meari/tree/main/crates/meari-shell) |

## Links

Every crate is on [crates.io](https://crates.io/crates/meari) with API documentation on [docs.rs](https://docs.rs/meari). The source is a single Cargo workspace on [GitHub](https://github.com/modootoday/meari) under `crates/`.
