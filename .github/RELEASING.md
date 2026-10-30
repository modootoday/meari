# Releasing meari

The procedure and its prerequisites live in `guide:meari/crates-io-npm-trusted-publishing`
(`.spec/guide/crates-io-npm-trusted-publishing.guide.md`). In short: crates.io and npm
both need a first, manually authenticated publish per package before Trusted Publishing
can be attached; after that, a `v*` tag runs `.github/workflows/release.yml` and a
reviewer approves the `crates` (and later `npm`) environment.

## Versions published manually during bootstrap

The `crates` job runs `scripts/publish-unpublished.sh`, which asks crates.io about each
publishable crate's current version and publishes only the ones that are missing. A tag whose
version was already uploaded by hand (the 0.1.0 bootstrap, which crates.io's new-crate rate
limit forced into several sessions) therefore skips those crates instead of failing the job and
blocking the `npm` job. Run it locally with `scripts/publish-unpublished.sh --dry-run`.
