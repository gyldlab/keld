# macOS packaging tasks and oracles
Part of: [index](index.md)
Owns: Boundaries, tasks and test plan

## 5. Boundaries
Proposed implementation ownership, after approval:

- `crates/keld-pack`: macOS-only assembly, tool invocation, explicit nested-code
  inventory, signing, output receipt and DMG. Reuse existing error conventions;
  no Windows producer edits or speculative platform enum expansion.
- `crates/keld-cli`: the implemented macOS build verb, shared typed config consumption
  and existing boot-producer integration. No window/link/principal/supervisor owner.
- `crates/keld-core`: existing config extraction and closed boot owner, opaque bundle
  provenance, same opened policy input, local ephemeral profile selection and
  existing launch-facts owner. The published Apple identity path remains intact.
- `crates/keld-wv`: expose the existing pure canonical-ID validator if necessary;
  do not add a local publisher identity or use the default WebView store.
- `crates/keld-runtime` and `crates/keld-host`: verified bundled Bun selection and
  provenance-scoped cleanup through existing Guardian/session owners.
- Existing `@keld/api`, `@keld/kipc`, `@keld/electron` bootstrap/fact consumers:
  accept the reviewed closed installed-facts variant; generated transport is updated
  only through its generator and stamp owner.
- Framework-owned minimal fixture/tests; draw.io application config and immutable
  resource selection belong in `gyldlab/keld-patched-apps` under its instructions.

Must not touch Windows/Linux production paths, shared IPC protocol, permission
schema, unrelated active worktrees, workspace dependencies, global build coordination
or root agent rules merely to make this slice compile. Any required owner/unsafe
allowlist change is explicit issue-scoped work with independent review, not an
implicit grant. Refresh claims and source pins before each implementation slice.

Runtime seam: before, a private development stage and lease feed the shipping Host;
after, an admitted durable local bundle feeds the same session. Host still owns UI,
Guard and IPC; Guardian still owns Bun generations; the application never mints
local provenance, grants, publisher identity or cleanup rights. Existing value/error/
event ordering survives. No new application IPC channel is proposed; the private
launch-fact codec extension must be versioned and reject unknown/mixed records.

## 6. Tasks
### T-1: Approve exact packaging contract (original T0)
- **Blocked by:** none
- **Work:** Independently review this exact proposal, reconcile owners and obtain
  exact-content human specification approval. The specification PR remains draft/open.

### T-2: Build the local installed app (original T1)
- **Blocked by:** T-1
- **Work:** Minimal real local app through the shipping Host: bundle admission,
  absolute bundled Bun, existing Guard, ephemeral profile, durable cleanup and
  installed launch facts, using Pack's native app/signing path. Fail-first controls
  precede the implementation. No draw.io hardcoding.

### T-3: Produce local DMG and receipts (original T2)
- **Blocked by:** T-2
- **Work:** Complete the macOS CLI/config and DMG output, receipt and failure cleanup.
  Integrate only committed qualified graph/provider source under its owners.

### T-4: Qualify installed app acceptance (original T3)
- **Blocked by:** T-3
- **Work:** Independently install/relocate both apps, run original draw.io workflows,
  all required security/lifecycle controls and current-head quality/review gates;
  publish only verified local artifacts and exact limitations.

Track meaningful PR-sized subunits on GH-1404 before their first edit; do not create
dozens of empty issues. Future Windows/Linux records remain deferred. Preserve the
original mission inventory when any predecessor is blocked.

## 7. Test plan & evidence
Use independent OS/process/file/wire observations. A positive local signature test
does not prove publisher identity; an API return does not prove installed behavior.

- **AC-1–AC-2**
  - **Single-fault control and oracle:** Build two differently configured apps. Delete/alter one required input, supply
    duplicate/unknown config, wrong target/arch, escaping path, symlink, overlapping roots or generated/output collision: typed
    refusal and no published output. Remove Host/Bun input execute bits: reject independently of unchanged content/signature.
- **AC-3–AC-4**
  - **Single-fault control and oracle:** Copy Host outside bundle, remove/change boot/policy/resource/Bun, mismatch ID, or supply
    absent/duplicate/unknown/wrongly typed local metadata: failure before app resource acquisition. No development fallback.
    Execute through Launch Services from an independent copied location.
- **AC-5**
  - **Single-fault control and oracle:** Validly re-sign a package requesting Strict/publisher persistence/update authority: still
    refuse those privileges. Whole-app replacement/re-signing is a documented same-user non-guarantee, never a passed authenticity
    test.
- **AC-6**
  - **Single-fault control and oracle:** Use an authenticated denied capability and verify typed denial plus absent external side
    effect. Separately disclose Legacy ambient authority.
- **AC-7**
  - **Single-fault control and oracle:** Inspect main facts before first app import. Wrong kind/schema/path/defaultApp/argv,
    public env forgery and missing facts cannot establish packaged state.
- **AC-8**
  - **Single-fault control and oracle:** Hostile PATH Bun writes a marker; missing installed Bun must fail without that marker.
    Replace Bun with differently hashed validly signed code meeting the same designated requirement: sealed digest still refuses.
    Preserve preload/ambient Bun option negative controls and generation revalidation.
- **AC-9**
  - **Single-fault control and oracle:** Independently mutate copied Host, Bun, boot, policy and app resource bytes after signing;
    signature verification fails. Separately chmod copied Host/Bun to `0644`: record signature-still-valid as the control, then
    require Pack rejection, installed Bun admission/generation refusal before app effects, or Launch Services refusal for the
    non-executable Host. Re-signing is a separate trust-control case.
- **AC-10**
  - **Single-fault control and oracle:** Verify/mount/copy/detach exact DMG device; corrupted disposable image fails integrity;
    unrelated mounts and existing files remain untouched.
- **AC-11**
  - **Single-fault control and oracle:** Rename and move the copied app, clear development dependencies, use unrelated cwd,
    inspect actual Host/Bun/resource paths. No checkout-based rescue.
- **AC-12**
  - **Single-fault control and oracle:** Original pinned editor, native interaction, saved XML/content readback,
    veto/Cancel/Save/Discard/Quit ordering. A window or mock editor is insufficient.
- **AC-13**
  - **Single-fault control and oracle:** Isolated subprocess controls for Bun and Host death; independent PID/start identity and
    descendant census, bounded observation deadlines, cleanup then relaunch. Bundle/document hashes remain unchanged. Record
    detached/simultaneous-death outcomes separately; an unmet required condition blocks closure.
- **AC-14–AC-15**
  - **Single-fault control and oracle:** Change one qualification pin or final artifact; receipt cannot transfer. Two-build
    comparison separates input/content/signed/image identities.
- **AC-16**
  - **Single-fault control and oracle:** Exact-head gates, independent hostile review/refutation, tracker body/status readback and
    no Windows/Linux implementation diff.


Reuse existing repository tests and unsafe/native owners. Do not create mock-only
process or WebView proof, sleep-based synchronization, retries of deterministic
failures, weakened assertions or helper-only installation claims.

Implementation gates: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo nextest run --workspace --profile ci`, relevant strict TypeScript/transport
checks, `just ci`, applicable generated-document checks and required GitHub CI.
A documentation-only draft records its own applicable checks separately; it cannot
report those implementation gates or native acceptance as passed.

Native installation commands are executed only against owned artifact/copy paths.
Use `open -n` to avoid reusing an existing development instance; its status is not a
workflow oracle. Do not remove quarantine or bypass Gatekeeper to claim public
installation success. Ad-hoc, Developer ID, notarization and Gatekeeper results are
separate fields.
