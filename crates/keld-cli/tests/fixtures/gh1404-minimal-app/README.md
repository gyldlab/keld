# GH-1404 B-3 generic minimal application (source candidate)

This is a committed second application, independent of draw.io. It has a real
Electron-compatible entry, a local renderer, an explicit Legacy permission
manifest, and a matched npm lockfile v3 with the three required first-party
package identities. It does not contain, install or approve provider archives.
The only current closed offline CSP catalog accepts
`original-editor-offline`; the generic HTML requires no inline resources,
and exactly one named window selects that restrictive response policy.
This selection remains subject to independent Guard/native acceptance.

**Qualification is pending.** Every Host, Bun and provider input in
`keld.build.ts` is a deliberately non-hex `UNQUALIFIED_*` sentinel. The
shipping build parser must reject these inputs *before* packaging or starting
application code. No SHA-256 obtained by hashing this candidate grants
independent source or provider approval.

The nested selected renderer is `web/pages/index.html`, not a root-level
page. The immutable source candidates for review are `package.json`,
`package-lock.json`, `keld.config.ts`, `keld.permissions.jsonc`,
`src/main.mjs`, `src/preload.mjs` and `web/pages/index.html`. The npm lock records
the selected first-party **names and versions** only; no producer archive,
resolved URL or integrity is asserted. It cannot be used to install a
qualified provider graph until the GH-502/API/KIPC owners deliver their
independently reviewed artifacts and regenerate/verify a real install lock.

Run `node --test verify.test.mjs` for exact-file and negative-control checks.
The dedicated Rust integration test `gh1404_minimal_fixture` runs this
source-only Node verifier as part of Cargo's normal test discovery on CI.
Its optional Pack/CLI native checks are skipped in that source-only invocation
and require the independently supplied binaries described below.
An actual Keld packaging-preflight refusal can be checked separately with
`KELD_B3_CLI=/absolute/path/to/keld node --test verify.test.mjs`, using a
binary built from the GH-1404 implementation revision. The verifier copies
only this fixture into a disposable directory, supplies syntactically valid
test-only Host/Bun hashes, and requires the **provider** sentinel to refuse
with `KELD-CLI-052`; it never creates an approved source receipt.

For a real Pack capture-contract check, set
`KELD_B3_PACK_RLIB=/absolute/path/to/libkeld_pack-<hash>.rlib` using the
GH-1404 implementation's compiled macOS keld-pack library, then run the
same Node test command. The test compiles `capture-contract-probe.rs` in an
isolated temporary directory and invokes the actual
`produce_macos_local_with_graph` copied-tree callback. It creates only an
**empty** temporary `node_modules` directory, never an SDK/provider,
verifies both root manifests and that directory in Pack's copied inventory,
and deliberately refuses graph compilation before signing or publication.
The `resources` declaration includes these roots for the future
independently qualified installed provider graph; an empty directory is
strictly a capture-topology test, never graph/provider qualification.

Native installed launch, Guard positive and denied operation, recovery,
relocation and signed-tamper tests are owned by the existing GH-1404 native
handoff. This source fixture does not close B-3 or authorize an app/DMG.
