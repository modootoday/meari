# Native Rust code HMR verification

Owned native verification fixture, independent of any consuming product. This is not a completed component workbench. The [acceptance contract](../../.spec/sots/integration/verification-gates.md) and [observations](../../.spec/evidence/verification/native-hmr-2026-10-18.md) live in the library's `.spec`.

Build both native executables, then choose a **new** session directory and a position wholly inside an existing nonprimary monitor:

```text
cargo build --bins
target/debug/hmr-verify NEW_SESSION_DIRECTORY X Y
target/debug/hmr-report NEW_SESSION_DIRECTORY/report.json
```

The Rust runner owns one `hmr-host` child and cleans it up on success/error. It compiles 21 compatible Rust UI generations, injects one compile error and ABI/schema mismatches, recovers, and shuts down. The host acknowledges only after actual renderer screenshot events. Host state comes from its typed API; this does not test OS keyboard/pointer input. Module bytes, source hashes, PID/HWND, state, screenshots, memory samples and timings are recorded.

The fixture's C ABI allows copied UTF-8 frame labels and a computed scalar. It does not pass egui values, Rust closures, allocators, vtables, or arbitrary application code across the DLL boundary. Only locally compiled trusted fixture DLLs in the owned session directory are loaded. Changes to unsupported ABI/schema are rejected.

Checks: `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`. Windows MSVC is the verified native target.
