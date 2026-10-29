# Custom stylesheet fixtures

These JSON files are library-owned inputs for the native styles SDK/CLI.

From the repository root:

```text
cargo run -p meari-native-tools --bin meari-native -- styles packages/meari-native/fixtures/custom-css/raw.json NEW_OUTPUT_DIRECTORY
cargo run -p meari-native-tools --bin meari-native -- styles packages/meari-native/fixtures/custom-css/tailwind.json NEW_OUTPUT_DIRECTORY ABSOLUTE_TAILWIND_EXE
```

- `raw.json`: direct multi-file CSS, variables, nesting and media queries.
- `tailwind.json`: CSS-first theme, breakpoint, utilities, variants, apply/reference and imports through the pinned compiler.
- `screen.json`: CSS sizing of the actual native canvas, followed by final-viewport media evaluation.

Output includes original source metadata/hashes, compiled CSS, PNG, layout and AccessKit state. Screen sizing and complete browser/native CSS equivalence are separate acceptance scopes.
