# Changelog

## Unreleased

Keld is in pre-alpha development. No alpha/dev-preview release is announced here;
the Cargo package version is not a release record. Future releases will link their
actual tag, release notes, tested platforms, and known limitations.

### Current development baseline

- A source-built Rust host and CLI provide project scaffolding, diagnostics, and the
  current native-window/Bun/app-link demo.
- kipc framing, codecs, authentication, permission evaluation, and scoped runtime
  supervision have implementations and tests.
- `@keld/electron` provides a limited lifecycle facade with recorded divergences.
- Public architecture, CI, and evidence scoreboards describe the intended contracts
  and measured slices.

The [product-status ledger](docs/engineering/product-status.md) is the authoritative
inventory and links code/tests for these statements. Migration, broad Electron
compatibility, installers, and signed updates remain future work. The
[commit history](https://github.com/gyldlab/keld/commits/main/) records individual
changes; the [roadmap](ROADMAP.md) explains prerequisites.
