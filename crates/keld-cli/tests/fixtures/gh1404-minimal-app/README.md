# GH-1404 B-3 generic minimal application (source candidate)

This is a committed second application, independent of draw.io. It has a real
Electron-compatible entry, a local renderer, an explicit Legacy permission
manifest, and a matched npm lockfile v3 with the three required first-party
package identities. It does not contain, install or approve provider archives.

**Qualification is pending.** Every Host, Bun and provider input in
`keld.build.ts` is a deliberately non-hex `UNQUALIFIED_*` sentinel. The
shipping build parser must reject these inputs *before* packaging or starting
application code. No SHA-256 obtained by hashing this candidate grants
independent source or provider approval.

The immutable source candidates for review are `package.json`,
`package-lock.json`, `keld.config.ts`, `keld.permissions.jsonc`,
`src/main.mjs`, `src/preload.mjs` and `web/index.html`. The npm lock records
the selected first-party **names and versions** only; no producer archive,
resolved URL or integrity is asserted. It cannot be used to install a
qualified provider graph until the GH-502/API/KIPC owners deliver their
independently reviewed artifacts and regenerate/verify a real install lock.

Run `node --test verify.test.mjs` for exact-file and negative-control checks.
An actual Keld packaging-preflight refusal can be checked separately with
`KELD_B3_CLI=/absolute/path/to/keld node --test verify.test.mjs`, using a
binary built from the GH-1404 implementation revision. The verifier copies
only this fixture into a disposable directory, supplies syntactically valid
test-only Host/Bun hashes, and requires the **provider** sentinel to refuse
with `KELD-CLI-052`; it never creates an approved source receipt.

Native installed launch, Guard positive and denied operation, recovery,
relocation and signed-tamper tests are owned by the existing GH-1404 native
handoff. This source fixture does not close B-3 or authorize an app/DMG.
