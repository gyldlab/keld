# Packaging execution and evidence

Part of: [index](index.md)
Owns: Boundaries, tasks, tests, performance and open decisions

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

### T-1: Review and approval

- **Blocked by:** B-1

Independently review this exact proposal and reconcile its owners.
  Obtain exact-content human specification approval before implementing the amended contract.
  Keep the spec draft and the PR open; do not merge.


### T-2: Minimal installed app

- **Blocked by:** T-1, B-3

Build a minimal installed app through the shipping Host with independently
  qualified generic inputs. Exercise bundle admission, absolute bundled Bun,
  existing Guard, ephemeral profile, durable cleanup and installed launch facts.
  Use the native Pack signing path and fail-first controls. Do not hardcode draw.io.
  Do not require approval of the original draw.io source to build this fixture.


### T-3: CLI and DMG

- **Blocked by:** T-2, B-3

Complete the generic macOS CLI/config and DMG output, receipt and failure cleanup.
  Use only committed, independently qualified generic graph/provider inputs.
  Defer original draw.io resource integration until its separate B-2 handoff.


### T-4: Native acceptance

- **Blocked by:** T-3, B-2

Independently install and relocate the minimal and original draw.io apps.
  Integrate only qualified original draw.io inputs from the B-2 handoff.
  Run original draw.io workflows, security/lifecycle controls and current-head gates.
  Publish only verified local artifacts and exact limitations.


Track meaningful PR-sized subunits on GH-1404 before their first edit; do not create
dozens of empty issues. Future Windows/Linux records remain deferred. Preserve the
original mission inventory when any predecessor is blocked.

## 7. Test plan & evidence

Use independent OS/process/file/wire observations. A positive local signature test
does not prove publisher identity; an API return does not prove installed behavior.

### Controls for AC-1–AC-2

Build two differently configured apps. Delete/alter one required input, supply duplicate/unknown config, wrong
target/arch, escaping path, symlink, overlapping roots or generated/output collision: typed refusal and no
published output. Remove Host/Bun input execute bits: reject independently of unchanged content/signature.

### Controls for AC-3–AC-4

Copy Host outside bundle, remove/change boot/policy/resource/Bun, mismatch ID, or supply
absent/duplicate/unknown/wrongly typed local metadata: failure before app resource acquisition. No development
fallback. Execute through Launch Services from an independent copied location.

### Controls for AC-5

Use admitted local provenance and consistent signed bundle, boot, permission digest and qualified graph inputs.
Verify the ordinary launch selects a fresh ephemeral store. At the existing profile owner, a reviewed
nonpublic test probe requests publisher-backed persistence and records that owner's denial.
Do not misuse the Guard Legacy/Strict field as a store selector. Earlier integrity refusal is not this verdict.
An isolated local-restriction fault must make the forbidden-store assertion fail.
Probe Guard Strict and updater authority separately at their owners, preserving valid input identities.
Whole-app replacement/re-signing remains a same-user non-guarantee, never a passed authenticity test.

### Controls for AC-6

Through the same authenticated link and exact manifest, exercise a permitted broker operation and a denied one.
Independently read back the authorized effect and the absence of a prohibited effect on disposable resources.
The denied call also returns its expected typed refusal. Separate deny-all and allow-all fault injections
must respectively fail the permitted and denied controls. Disclose Legacy ambient authority separately.

### Controls for AC-7

Inspect main facts before first app import. Wrong kind/schema/path/defaultApp/argv, public env forgery and
missing facts cannot establish packaged state.

### Controls for AC-8

Hostile PATH Bun writes a marker; missing installed Bun must fail without that marker. Replace Bun with
differently hashed validly signed code meeting the same designated requirement: sealed digest still refuses.
Preserve preload/ambient Bun option negative controls and generation revalidation.

### Controls for AC-9

Independently mutate copied Host, Bun, boot, policy and app resource bytes after signing; signature
verification fails. Separately chmod copied Host/Bun to `0644`: record signature-still-valid as the control,
then require Pack rejection, installed Bun admission/generation refusal before app effects, or Launch Services
refusal for the non-executable Host. Re-signing is a separate trust-control case.

### Controls for AC-10

Verify/mount/copy/detach exact DMG device; corrupted disposable image fails integrity; unrelated mounts and
existing files remain untouched.

### Controls for AC-11

Rename/move the copied app and use unrelated cwd. Omit a required dependency, then independently offer marker
packages through a populated Bun cache, hostile registry/`.npmrc`, ancestor `node_modules` and `NODE_PATH`.
Exercise static/dynamic import, `require` and worker dependency paths: typed refusal before marker evaluation,
no dependency acquisition/network request, and no checkout/cache rescue. Observe requests and external marker
effects independently of the resolver. A matching dependency captured in the qualified sealed graph must still
load. Repeat on Bun recovery/restart and after relocation.

### Controls for AC-17

Require separate producer and installed-resolver verdicts. Offer the unqualified marker to Pack and record
its producer refusal. Independently construct a disposable local bundle using reviewed fixture construction
and normal signing. Keep boot, policy, runtime and graph identities valid except for the excluded marker.
The executable marker is physically present and signed but not independently qualified.
Relocate the app and require installed-resolver refusal before marker effects for static/dynamic imports,
require and worker entry points. Repeat every loading path after Bun restart and relocation.
The qualified marker counterpart must execute under otherwise identical conditions.
Record the refusing owner and external marker observations separately for each fixture.
Pack refusal is not installed coverage. An earlier refusal blocks the installed verdict until a reviewed
fixture reaches that owner; do not bypass installed admission or silently count the earlier refusal.

### Controls for AC-12

Save document state A and verify its file contents. Make unsaved edit B before the first close.
Observe the native unsaved dialog, then Cancel: the window stays open with B and disk remains A.
Save B, read back B, close and reopen B. Make unsaved edit C before the second close.
Observe the second native unsaved dialog, then Discard: disk retains B, not C.
Reopen to verify B without C, then Quit. Record native window/document state and file contents at each step.
An isolated Cancel-as-Close fault must fail independent native observation. A mock editor is insufficient.

### Controls for AC-13

Run healthy quit/relaunch as the positive control. Separately terminate Bun only, Host only and Guardian only.
For Guardian-only loss, confirm enrollment and an active Bun generation with Host initially alive.
Record exact PID/start identities, descendant census, authenticated-link revocation and owned-group retirement.
Declare observation and subsequent-launch deadlines before each run. Require successful launch within that bound.
Any surviving owned child, stale link, failed required launch or changed bundle/document hash turns the case RED.
Report detached and simultaneous-death cases separately; they do not exempt Guardian-only loss.
Keep AC-13 open with the existing runtime owner if a required result cannot be established.

### Controls for AC-14–AC-15

Change one qualification pin or final artifact; receipt cannot transfer. Two-build comparison separates
input/content/signed/image identities.

### Controls for AC-16

Exact-head gates, independent hostile review/refutation, tracker body/status readback and no Windows/Linux
implementation diff.


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

### Q-1: Approval decision

 approve or reject this exact local-development contract:
ad-hoc self-consistency without publisher authenticity; an ephemeral browser profile;
existing explicit Legacy authority; no Strict/updater/publisher-profile privileges;
the existing Host as launcher; exact bundled Bun; provenance-scoped cleanup; and a
separately qualified original-content package caller. This is the missing feature
spec approval artifact, not an optional implementation preference.

### Q-2: Integration evidence

Owners of #499/#502/#553/#452/#570 must supply committed source/provider and
qualification handoffs. B-3 covers the generic inputs for T-2/T-3; B-2 covers original
draw.io inputs for T-4. Current bounded test integration is evidence, not installed
release approval. #589 records the resulting end-to-end consumer evidence.

### U-1: Execution uncertainties

 preserved pinned-Bun operation in the installed layout, resource
replacement controls, original draw.io packaged-path behavior and required descendant
cleanup are tests, not assumed passes. Preserve a named blocker and owner if a
required predicate fails. Stronger same-user replacement protection would require
an independently approved trust anchor and cannot be promised by this artifact.

### Q-3: Future platform decisions

 Windows/Linux installer format, installed trust details, signing,
update ownership and supported native cells remain in #1405/#1406. They do not delay
macOS implementation once its own approval and source dependencies are satisfied.

### B-1: Exact specification approval

- **Status:** open
- **Owner:** GYLDLAB specification owner
- **Impact:** T-1 cannot complete and implementation under this amended contract remains gated.
- **Check:** Record exact-content approval of the complete index and all parts.

### B-2: Qualified original-application handoff

- **Status:** open
- **Owner:** Graph/provider and launch-fact owners of GH-499, GH-502, GH-553, GH-452 and GH-570
- **Impact:** T-4 cannot integrate or accept original draw.io without its independent
  committed source/provider/launch-fact qualifications. This does not block the generic fixture.
- **Check:** Record approved original-app source, provider and qualification
  handoffs for the exact draw.io implementation inputs.

### B-3: Qualified generic minimal-app handoff

- **Status:** open
- **Owner:** Generic graph/provider and launch-fact owners of GH-499, GH-502, GH-553, GH-452 and GH-570
- **Impact:** T-2 and T-3 require qualified generic Host, Bun, graph, permission
  and installed launch-fact inputs. Draw.io-specific source approval is not required.
- **Check:** Record the committed minimal-app fixture, its independently qualified
  generic inputs and the fail-closed admission checks before build or installed launch.
