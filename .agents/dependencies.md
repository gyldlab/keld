# Dependency verification playbook

Load this playbook for every dependency add, bump, removal, API migration, or claim
about the current version of a Cargo crate, Bun package, or Bun itself. Dependency
additions trigger the root gate and its independent evidence requirement.

## Authoritative checks

1. Query the authoritative registry. For Rust use
   `cargo info <crate> --registry crates-io`; for packages managed by Bun use
   `bun pm view <package> version dist-tags time repository`. For Bun itself, use the
   official Bun release feed. A local `<tool> --version` proves only what is installed,
   never what is current upstream.
2. Read the selected version's upstream release notes or changelog and source tag.
   For a major bump, read the official migration guide before editing manifests.
3. Retrieve current, version-specific API documentation with Context7 when available.
   Context7 aids retrieval; it does not replace registry metadata, upstream releases,
   source, or a migration guide.
4. Check Keld's Rust toolchain/MSRV, Bun/runtime assumptions, supported targets,
   enabled features, license, maintenance state, and relevant advisories.
5. Record the candidate version, release date, primary URLs, breaking changes,
   alternatives, and why `std` or an existing dependency is insufficient.

## Change discipline

- A version bump MUST be its own scoped task, not drive-by cleanup in another feature.
- Update manifests and lockfiles together using Cargo or Bun's package manager; do not
  hand-select a version from memory.
- Compile and test every affected target. Failures introduced by the bump are migration
  work to complete or a reason to reject the bump; they MUST NOT be dismissed as noise,
  hidden with skips, or deferred behind a knowingly broken lockfile.
- Report platform or API paths that could not be verified. Do not infer compatibility
  from a successful local `--version`, install, or single-platform build.
