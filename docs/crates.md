---
title: The crates
description: "Which Meari packages to depend on, and what each one is for."
layout: reference
packages_title: "Published crates"
packages_compact: true
packages_position: after
packages:
  - {name: "meari", version: "0.2.0", text: "The core: typed UI model, Rust/TSX conversion, media and hot-reload contracts, fonts, and the browser adapter.", url: "https://crates.io/crates/meari", links: [{label: "docs.rs", url: "https://docs.rs/meari"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari"}]}
  - {name: "meari-native", version: "0.2.0", text: "Renders the model in a native window: CSS, layout, paint, interaction and accessibility.", url: "https://crates.io/crates/meari-native", links: [{label: "docs.rs", url: "https://docs.rs/meari-native"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-native"}]}
  - {name: "meari-cli", version: "0.2.0", text: "The `meari` command line.", url: "https://crates.io/crates/meari-cli", links: [{label: "docs.rs", url: "https://docs.rs/meari-cli"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-cli"}]}
  - {name: "meari-builder", version: "0.2.0", text: "Loads projects, runs compiler adapters and produces app bundles.", url: "https://crates.io/crates/meari-builder", links: [{label: "docs.rs", url: "https://docs.rs/meari-builder"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-builder"}]}
  - {name: "meari-native-tools", version: "0.2.0", text: "The `meari-native` showcase host and the `meari-kitchen-sink` verifier.", url: "https://crates.io/crates/meari-native-tools", links: [{label: "docs.rs", url: "https://docs.rs/meari-native-tools"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-native-tools"}]}
  - {name: "meari-platform", version: "0.2.0", text: "Every operating-system difference, in one place.", url: "https://crates.io/crates/meari-platform", links: [{label: "docs.rs", url: "https://docs.rs/meari-platform"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-platform"}]}
  - {name: "meari-dom", version: "0.2.0", text: "Fork of `blitz-dom` with Meari's layout fixes.", url: "https://crates.io/crates/meari-dom", links: [{label: "docs.rs", url: "https://docs.rs/meari-dom"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-dom"}]}
  - {name: "meari-paint", version: "0.2.0", text: "Fork of `blitz-paint` with Meari's paint fixes.", url: "https://crates.io/crates/meari-paint", links: [{label: "docs.rs", url: "https://docs.rs/meari-paint"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-paint"}]}
  - {name: "meari-html", version: "0.2.0", text: "Unmodified `blitz-html`, built against the forks.", url: "https://crates.io/crates/meari-html", links: [{label: "docs.rs", url: "https://docs.rs/meari-html"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-html"}]}
  - {name: "meari-shell", version: "0.2.0", text: "Unmodified `blitz-shell`, built against the forks.", url: "https://crates.io/crates/meari-shell", links: [{label: "docs.rs", url: "https://docs.rs/meari-shell"}, {label: "Source", url: "https://github.com/modootoday/meari/tree/main/crates/meari-shell"}]}
---

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

## Links

Every crate is on [crates.io](https://crates.io/crates/meari) with API documentation on [docs.rs](https://docs.rs/meari). The source is a single Cargo workspace on [GitHub](https://github.com/modootoday/meari) under `crates/`.
