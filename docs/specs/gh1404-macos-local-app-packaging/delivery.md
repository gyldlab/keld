# GH-1404 packaging delivery
Part of: [index](index.md)
Owns: sections 5–7 and 9–10

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

T0 completed: the exact proposal received human specification approval. The PR remains draft/open.

### T-1: build the minimal local app
- **Blocked by:** none
- Build through the shipping Host.
- Include bundle admission, absolute bundled Bun, existing Guard and an ephemeral profile.
- Prove durable cleanup and installed launch facts through Pack signing.
- Run failure-first controls before implementation. Do not add draw.io branches.

### T-2: complete macOS CLI, configuration and DMG output
- **Blocked by:** T-1
- Complete the macOS CLI and configuration path.
- Produce the DMG, receipts and failure cleanup.
- Integrate only committed, qualified graph/provider sources under their owners.

### T-3: qualify installed apps and original workflows
- **Blocked by:** T-2
- Install and relocate both apps independently.
- Run original draw.io workflows and all required security/lifecycle controls.
- Run current-head quality and review gates.
- Publish only verified local artifacts and exact limitations.

Track meaningful PR-sized subunits on GH-1404 before their first edit. Do not create dozens of empty issues. Future Windows/Linux records remain deferred. Preserve the original mission inventory when a predecessor is blocked.

### Original owner task sequence (historical)


- [x] T0: independently review this exact proposal, reconcile owners and obtain
  exact-content human specification approval. The specification PR remains draft/open.
- [ ] T1: minimal real local app through the shipping Host: bundle admission,
  absolute bundled Bun, existing Guard, ephemeral profile, durable cleanup and
  installed launch facts, using Pack's native app/signing path. Fail-first controls
  precede the implementation. No draw.io hardcoding.
- [ ] T2: complete the macOS CLI/config and DMG output, receipt and failure cleanup.
  Integrate only committed qualified graph/provider source under its owners.
- [ ] T3: independently install/relocate both apps, run original draw.io workflows,
  all required security/lifecycle controls and current-head quality/review gates;
  publish only verified local artifacts and exact limitations.

Track meaningful PR-sized subunits on GH-1404 before their first edit; do not create
dozens of empty issues. Future Windows/Linux records remain deferred. Preserve the
original mission inventory when any predecessor is blocked.

## 7. Test plan & evidence

Use independent OS/process/file/wire observations. A positive local signature test
does not prove publisher identity; an API return does not prove installed behavior.

| --- | --- |

- **AC-1/AC-2 controls:** Build two differently configured apps. Delete/alter one required input, supply duplicate/unknown config, wrong target/arch, escaping path, symlink, overlapping roots or generated/output collision: typed refusal and no published output. Remove Host/Bun input execute bits: reject independently of unchanged content/signature.

- **AC-3–AC-4 controls:** Copy Host outside bundle, remove/change boot/policy/resource/Bun, mismatch ID, or supply absent/duplicate/unknown/wrongly typed local metadata: failure before app resource acquisition. No development fallback. Execute through Launch Services from an independent copied location.

- **AC-5 controls:** Validly re-sign a package requesting Strict/publisher persistence/update authority: still refuse those privileges. Whole-app replacement/re-signing is a documented same-user non-guarantee, never a passed authenticity test.

- **AC-6 controls:** Use an authenticated denied capability and verify typed denial plus absent external side effect. Separately disclose Legacy ambient authority.

- **AC-7 controls:** Inspect main facts before first app import. Wrong kind/schema/path/defaultApp/argv, public env forgery and missing facts cannot establish packaged state.

- **AC-8 controls:** Hostile PATH Bun writes a marker; missing installed Bun must fail without that marker. Replace Bun with differently hashed validly signed code meeting the same designated requirement: sealed digest still refuses. Preserve preload/ambient Bun option negative controls and generation revalidation.

- **AC-9 controls:** Independently mutate copied Host, Bun, boot, policy and app resource bytes after
  signing; signature verification fails. Separately chmod copied Host/Bun to `0644`: record signature-
  still-valid as the control, then require Pack rejection, installed Bun admission/generation refusal
  before app effects, or Launch Services refusal for the non-executable Host. Re-signing is a separate
  trust-control case.

- **AC-10 controls:** Verify/mount/copy/detach exact DMG device; corrupted disposable image fails integrity; unrelated mounts and existing files remain untouched.

- **AC-11 controls:** Rename and move the copied app, clear development dependencies, use unrelated cwd, inspect actual Host/Bun/resource paths. No checkout-based rescue.

- **AC-12 controls:** Original pinned editor, native interaction, saved XML/content readback, veto/Cancel/Save/Discard/Quit ordering. A window or mock editor is insufficient.

- **AC-13 controls:** Isolated subprocess controls for Bun and Host death; independent PID/start identity and descendant census, bounded observation deadlines, cleanup then relaunch. Bundle/document hashes remain unchanged. Record detached/simultaneous-death outcomes separately; an unmet required condition blocks closure.

- **AC-14–AC-15 controls:** Change one qualification pin or final artifact; receipt cannot transfer. Two-build comparison separates input/content/signed/image identities.

- **AC-16 controls:** Exact-head gates, independent hostile review/refutation, tracker body/status readback and no Windows/Linux implementation diff.

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

## 9. Perf impact

Packaging/signing/DMG work is cold tooling. Runtime bundle and graph validation adds
startup I/O; measure actual installed startup and memory for the same inputs before
making claims. Stream large file hashes/copies, reuse existing bounded parsers and
inventory limits, and avoid allocating file contents wholesale. No language rewrite,
new async runtime or performance budget waiver is proposed.

## 10. Open questions, blockers & unknowns

**Recorded approval:** The user approved the exact local-development contract.
The contract selects ad-hoc self-consistency without publisher authenticity.
It selects an ephemeral browser profile and existing explicit Legacy authority.
It excludes Strict, updater and publisher-profile privileges.
The existing Host launches the exact bundled Bun. Cleanup uses provenance.
The original-content package caller needs separate qualification.
This approval does not replace implementation review or native acceptance.

**Future only:** Windows/Linux installer formats, installed trust, signing,
update ownership and supported native cells remain under #1405/#1406.
They do not block macOS work after its source dependencies are satisfied.

### B-1: source and provider qualification
- **Status:** open
- **Owner:** GH-499, GH-502, GH-553, GH-452 and GH-570 owners
- **Impact:** The installed graph cannot be qualified without committed source/provider receipts.
- **Check:** Supply exact committed source/provider identities and qualification handoff for T-1/T-2.

### B-2: installed original workflow and cleanup
- **Status:** open
- **Owner:** GH-589 / native acceptance owner
- **Impact:** Current bounded test integration does not prove installed release acceptance.
- **Check:** Complete installed draw.io workflows, resource replacement controls, descendant cleanup and exact artifact readback.

### Original owner evidence and open uncertainties

**Recorded approval:** the user authorized this exact local-development contract:
ad-hoc self-consistency without publisher authenticity; an ephemeral browser profile;
existing explicit Legacy authority; no Strict/updater/publisher-profile privileges;
the existing Host as launcher; exact bundled Bun; provenance-scoped cleanup; and a
separately qualified original-content package caller. The approval provenance is
recorded above; implementation review and native acceptance remain separate gates.

**Integration evidence needed:** owners of #499/#502/#553/#452/#570 must supply the
exact committed source/provider and applicable qualification handoff required by
T1/T2. Current bounded test integration is real evidence but is not installed release
approval. #589 records the resulting end-to-end consumer evidence.

**Execution uncertainties:** preserved pinned-Bun operation in the installed layout, resource
replacement controls, original draw.io packaged-path behavior and required descendant
cleanup are tests, not assumed passes. Preserve a named blocker and owner if a
required predicate fails. Stronger same-user replacement protection would require
an independently approved trust anchor and cannot be promised by this artifact.

**Future only:** Windows/Linux installer format, installed trust details, signing,
update ownership and supported native cells remain in #1405/#1406. They do not delay
macOS implementation once its own approval and source dependencies are satisfied.

### Q-1: installed replacement protection
A stronger same-user replacement guarantee needs an independently approved trust anchor.
The current local artifact does not claim that protection.
