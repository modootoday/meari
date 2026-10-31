# Releasing meari

The procedure and its prerequisites live in `guide:meari/crates-io-trusted-publishing`
(`.spec/guide/crates-io-trusted-publishing.guide.md`). In short: crates.io needs a
first, manually authenticated publish per crate before Trusted Publishing can be attached;
after that, a `v*` tag runs `.github/workflows/release.yml` and a reviewer approves the
`crates` environment. meari ships through cargo only (`cargo install meari-cli`)
(adr:meari/20261031023000).

## Versions published manually during bootstrap

The `crates` job runs `scripts/publish-unpublished.sh`, which asks crates.io about each
publishable crate's current version and publishes only the ones that are missing. A tag whose
version was already uploaded by hand (the 0.1.0 bootstrap, which crates.io's new-crate rate
limit forced into several sessions) therefore skips those crates instead of failing the job. Run it locally with `scripts/publish-unpublished.sh --dry-run`.


Package verification builds `yeslogic-fontconfig-sys`, so the `crates` job installs
`libfontconfig1-dev libfreetype-dev pkg-config` before publishing (the first v0.2.0 attempt
stopped at verification for lack of them; nothing was uploaded).
