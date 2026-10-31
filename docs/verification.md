---
title: How Meari is tested
description: "How Meari compares its native renderer with a real browser on Linux, Windows and macOS."
---

# How Meari is tested

If you are deciding whether to rely on Meari, you will want to know how its claims are checked. The short answer: the native renderer is compared with a real browser, on three operating systems, and tolerances are never loosened to make a change pass.

## Native against the browser

The visual test, PixelGate, renders the same TSX fixture twice: once with Meari's native renderer, off screen, and once as a web page in headless Chrome. It then compares the two images pixel by pixel, with a tolerance that belongs to each fixture.

Two policies decide what a difference means:

- **Strict** fails on any difference beyond the fixture's tolerance. Linux is the Strict baseline.
- **Advisory** reports differences that come only from anti-aliasing, but still fails on render errors and layout mismatches. Windows and macOS run with this policy.

## Three operating systems

The CI workflow builds the workspace, runs Clippy with warnings as errors and runs the tests on Linux, Windows and macOS, then runs PixelGate on each. A release is tagged only after all three pass.

Making that true took real fixes rather than retries. Browser text rendering was pinned to grayscale anti-aliasing so references no longer depend on how a machine is set up. Input widths now follow the formula Chrome uses on macOS. A duplicate accessibility adapter that crashed Windows was removed. The details are in the [0.2.0 changelog](changelog.md).

## Run it yourself

From a clone of the repository:

```sh
cargo xtask visual-test --gate strict
```

Some tests need external inputs, such as a pinned Tailwind executable or the official documentation corpus. They are ignored by default, and a skipped test is never counted as evidence of compatibility.
