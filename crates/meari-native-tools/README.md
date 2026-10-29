# Meari Native Tools

Executable hosts for the `meari-native` library. This package owns the existing
`meari-native` and `meari-kitchen-sink` executable targets. Their argument syntax,
exit codes, rendering and verification APIs remain in the native library.

From the repository root:

```text
cargo run -p meari-native-tools --bin meari-native -- --help
cargo run -p meari-native-tools --bin meari-kitchen-sink -- --help
cargo run -p meari-native-tools --features window --bin meari-native -- show TAILWIND_EXE
cargo run -p meari-native-tools --features gl --bin meari-kitchen-sink -- gl-check NEW_REPORT.json
cargo test -p meari-native-tools --test cli_entrypoints -- --test-threads=1
```

The Cargo package selector for the executable hosts changed from `meari-native`
to `meari-native-tools`. Installed executable names and arguments are unchanged.
Use `cargo install --path packages/meari-native-tools --bins` for both executables;
add `--features window`, `--features gl` or both when those adapters are needed.
Cargo publication remains disabled during framework acceptance.

Library users keep the `meari-native` package and `meari_native` crate name.
`cargo test -p meari-native --test source_callbacks` selects library integration
tests without selecting this host package. A full workspace test includes the
tools and their CLI entrypoint tests. Source-owned fixtures remain in the native
library package; external compilers, official corpora and evidence are local inputs.

GL, real windows, full upstream coverage and platform accessibility still require
their own explicit verification. CLI help tests do not establish those capabilities.
