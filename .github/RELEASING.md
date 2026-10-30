# Releasing meari

The procedure and its prerequisites live in `guide:meari/crates-io-npm-trusted-publishing`
(`.spec/guide/crates-io-npm-trusted-publishing.guide.md`). In short: crates.io and npm
both need a first, manually authenticated publish per package before Trusted Publishing
can be attached; after that, a `v*` tag runs `.github/workflows/release.yml` and a
reviewer approves the `crates` (and later `npm`) environment.
