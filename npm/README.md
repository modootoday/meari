# @modootoday/meari-cli

Prebuilt `meari` binaries. `npm install -g @modootoday/meari-cli` installs the launcher and,
through `optionalDependencies`, only the package for your platform
(`@modootoday/meari-cli-{linux,darwin,win32}-{x64,arm64}`).
The release workflow writes `optionalDependencies` at publish time, so this checkout never
points at versions that are not on npm yet. Without Node: `cargo install meari-cli`.
