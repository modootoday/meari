---
title: Contributing
description: "How to build Meari from source, run its checks, and where things live."
---

# Contributing

Thank you for looking at the code. This page gets you from a fresh clone to a passing check, so a change you send can be reviewed quickly.

## Building from source

Meari is one Cargo workspace. The repository pins its Rust toolchain, so `rustup` installs the right version for you. On Linux, install the fontconfig and FreeType development packages first (see [Get started](getting-started.md)).

```sh
git clone https://github.com/modootoday/meari.git
cd meari
cargo run -p meari-cli -- --help
```

The default member is the portable `meari` core; native dependencies are only built when you ask for them.

## Checks before you send a change

These are the same checks CI runs:

```sh
cargo test -j1
cargo clippy -j1 --workspace --all-targets -- -D warnings
cargo test -j1 -p meari-native -- --test-threads=1
cargo check -p meari --target wasm32-unknown-unknown --features wasm
cargo xtask visual-test --gate strict
```

If a visual comparison fails, fix the cause rather than raising the fixture's tolerance.

## Where things live

All packages are under `crates/`. `meari-visual` and `meari-demo` are workspace-only tools: the first is the visual test harness, the second opens one fixture side by side in a native window and in Chrome so you can compare by eye. `xtask` holds project automation such as the visual test and font fetching. [The crates](crates.md) describes the published packages.

## Fonts and assets

No font files are committed. Tests and the visual test download the pinned Noto files they need and verify their checksums. Please keep it that way: do not commit fonts or generated evidence.

