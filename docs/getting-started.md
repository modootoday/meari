---
title: Get started
description: "Install the Meari CLI, fetch the pinned fonts and add Meari to a Rust project."
---

# Get started

This page takes you from nothing to a working Meari setup. You need Rust and Cargo; the repository itself pins Rust 1.99.1 (edition 2024).

## 1. Install the command line

The `meari` command is the one entry point for converting sources, running the native host, building apps and managing fonts.

```sh
cargo install meari-cli
meari --help
```

## 2. Fetch the fonts once

Meari draws text with the Noto families (SIL Open Font License). They are not bundled into any binary, so you download them explicitly, once:

```sh
meari fonts fetch
```

Every file is pinned to a fixed revision and checked for length and SHA-256 before it is kept. Fonts land in `$MEARI_FONT_CACHE` if you set it, otherwise in the nearest `runtime/fonts` directory. A cached file works offline.

> [!NOTE]
> An app built with Meari never downloads fonts on its own. If the cache is empty it prints one line pointing at `meari fonts fetch` and falls back to the system fonts, so you see a degraded look rather than a crash. A corrupted file is reported as an error, never silently skipped.

You can fetch only the scripts you need, for example `meari fonts fetch latin,hangul,kana`.

## 3. Prepare a Linux machine

On Linux the native crates link against fontconfig and FreeType. On Debian or Ubuntu:

```sh
sudo apt install libfontconfig1-dev libfreetype-dev pkg-config
```


## 4. Add Meari to your project

`meari` is the portable core. Add `meari-native` when you want to render in a native window.

```toml
[dependencies]
meari = "0.2"
meari-native = "0.2"
```

The browser adapter is part of the core behind the `wasm` feature:

```sh
cargo check -p meari --target wasm32-unknown-unknown --features wasm
```

## Next steps

- [Using Meari in a Rust app](guide.md) explains how TSX, generated Rust and your own application code fit together.
- [The crates](crates.md) helps you decide which packages you actually need.
- The API reference lives on [docs.rs](https://docs.rs/meari).
