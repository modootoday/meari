# Library development rules

- Implement library, tests, benchmarks and development tools in Rust. TSX fixtures/output are conversion artifacts.
- Keep domain policy/IR independent of parser, renderer, filesystem and OS types. Application defines ports; infrastructure implements them. CLI calls the same API.
- Keep this library independent of any consuming product name, SDK, media service or domain.
- Reject unsupported syntax/behavior with source diagnostics; never silently claim arbitrary Rust/React equivalence.
- Store specifications in `.spec/sots`, decisions in `.spec/adrs`, remaining work in `.spec/plans`, observations in `.spec/evidence`, and fixtures/design assets under `.spec/artifacts` by responsibility. Link documents instead of duplicating policy.
- Use published Rust crates first. Separate registry publication, upstream claims, local compile, actual execution and unverified behavior.
- Run `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets -- -D warnings` for changed Rust workspaces. Record actual scope and results.
- This directory owns its Git history, including `.spec`. The consumer repository must reference a pinned gitlink, never stage this library's source files. Do not invent a remote URL or describe unregistered nested source as a completed submodule.
