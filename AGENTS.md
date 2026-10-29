# Library development rules

- Implement library, tests, benchmarks and development tools in Rust. TSX fixtures/output are conversion artifacts.
- Keep domain policy/IR independent of parser, renderer, filesystem and OS types. Application defines ports; infrastructure implements them. CLI calls the same API.
- Keep this library independent of any consuming product name, SDK, media service or domain.
- Reject unsupported syntax/behavior with source diagnostics; never silently claim arbitrary Rust/React equivalence.
- Manage every design, planning and maintenance document only with the specfold `spec` CLI in the root `.spec/` (sot = specifications, adr = decisions, plan = remaining work, evidence = observations, design = design assets). Before changing code run `spec context <path>`; create documents with `spec new <type> --title ...`, change status with `spec set`, never hand-write spec file names; run `spec check --staged` (or `cargo xtask spec-check`) before committing. Link documents instead of duplicating policy. See `guide:meari/docs-specfold`.
- Use published Rust crates first. Separate registry publication, upstream claims, local compile, actual execution and unverified behavior.
- Run `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets -- -D warnings` for changed Rust workspaces. Record actual scope and results.
- This directory owns its Git history, including `.spec`. The consumer repository must reference a pinned gitlink, never stage this library's source files. Do not invent a remote URL or describe unregistered nested source as a completed submodule.
