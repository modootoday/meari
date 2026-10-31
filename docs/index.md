---
title: "Meari: write your UI once, render it natively and on the web"
description: "Meari is a Rust UI framework. One typed UI declaration drives a native renderer and a browser adapter, and the CLI uses the same APIs."
hero: { c: hero }
sections:
  - { c: features, cols: 3 }
  - { c: tabs }
  - { c: features, cols: 2 }
  - { c: cta }
---

# Write once, echo everywhere.

Meari (메아리, Korean for "echo") is a Rust UI framework. You describe a screen once, as bounded TSX or plain Rust, and Meari renders that same typed description in a native window and in the browser.

[Get started](getting-started.md) [Read the guide](guide.md)

## Why Meari

Most cross-platform UI stacks ask you to pick a runtime first. Meari starts from a shared contract instead: the UI is data that every target understands.

- **One declaration, two targets** — the same component tree runs in the native renderer and, through the `wasm` feature, as a persistent browser document.
- **Rust and TSX, both ways** — bounded TSX converts to Rust and back, with source mappings and clear diagnostics when something is outside the supported profile.
- **Familiar styling** — Tailwind CSS and shadcn/ui are the default design profile, and native windows consume the compiled CSS.
- **Checked against a browser** — the native renderer is compared pixel by pixel with headless Chrome on Linux, Windows and macOS.
- **No hidden network access** — fonts are pinned Noto files you download once on purpose; an app never fetches anything at run time.
- **Dual licensed** — MIT or Apache-2.0, published on crates.io.

## Install

The `meari` command bundles conversion, the native host, the builder and font tools. Libraries go into your own `Cargo.toml`.

### CLI

```sh
cargo install meari-cli
meari fonts fetch
```

### Library

```toml
[dependencies]
meari = "0.2"
meari-native = "0.2"
```

## Where it stands

Meari is young and honest about it. Version 0.2.0 is a working foundation, not a finished framework.

- **What works today** — parsing, conversion and native preparation; typed string, bool and array props; `useId` and primitive `useState`; keyed lists; persistent input and focus; custom title bars with OS-driven window resizing.
- **What is still open** — general React hooks and effects, complete CSS parity, a full Native+Web builder and full per-OS acceptance. The [guide](guide.md) marks these boundaries as you go.

## Try it

Install the CLI, fetch the fonts and open the native showcase in a few minutes.

[Get started](getting-started.md) [View on GitHub](https://github.com/modootoday/meari)
