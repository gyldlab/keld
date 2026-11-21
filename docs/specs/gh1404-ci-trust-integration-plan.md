# GH-1404 — proposed trusted CI integration sequence

Status: **PROPOSED — REQUIRES INDEPENDENT MAINTAINER AND SECURITY REVIEW**.
Tracking: [GH-1404](https://github.com/gyldlab/keld/issues/1404).
Original implementation: [draft PR #1447](https://github.com/gyldlab/keld/pull/1447).
Source candidate: [draft PR #1469](https://github.com/gyldlab/keld/pull/1469).
Full-history CI evidence: [draft PR #1472](https://github.com/gyldlab/keld/pull/1472).
Companion [single-graph amendment](macos-original-single-graph-amendment.md)
is also **not approved for signed release**.

This is a sequencing and decision document, **not** a CI policy update,
authorization to merge, or exception to provenance, Gitleaks, CODEOWNERS,
native signing or historical credential handling.

## 1. Verified baseline and failures

The original #1447 remains a draft and its history is preserved. Its
historical ancestor `69ce0730` includes a generic-api-key match in a test
source. Neither the current source location nor a test filename proves that
the value is not a real credential. The value MUST NOT be disclosed in an
issue comment, copied into an allowlist or removed by rewriting shared
history to obtain a green check. The authorized security owner MUST
privately determine whether rotation/revocation or another independently
approved remediation is required.

On source branch `72648e94`, [hosted Windows run
42832632882](https://github.com/gyldlab/keld/actions/runs/42832632882)
passed 257 Bun/API tests and failed one. The supported Pack-time generated
static literal single-graph bootstrap passes real Host HELLO,
RuntimeProcessInit ACK, both subscriptions, Ready, exactly one authenticated
link and clean exit. The remaining case is the **owner-controlled**
`packages/@keld/api/src/runtime-preload.test.ts` positive that still
uses Bun 1.4.4 Windows' unsupported CLI `--preload` + main dual-phase
cache; it times out at `KELD-IPC-006`. The equivalent independently owned
test is already migrated with all lifecycle assertions preserved; the
original controlled file remains dirty in another worktree.

The full-history main-based #1472 includes 451 changed paths against main.
The existing trusted [CI provenance workflow](../../.github/workflows/ci-provenance.yml)
refuses PRs with 300 or more changed files because GitHub's immutable
compare result may truncate file enumeration. It also unconditionally
refuses protected CI authority inputs, regardless of PR size. VeriSentry
correctly publishes a blocking failure when provenance admission fails.
The main-based full-history CI also correctly reports the historical
Gitleaks finding and the old Windows test failure. These are **distinct**
failures; retrying the same head cannot resolve them.

## 2. Exact protected authority surface

A read-only enumeration of #1472 relative to main, at head `03545b27`,
found **451 changed files: 16 protected CI/configuration authority inputs
and 435 other paths**. The protected list under the current trusted-base
policy is:

- `.github/workflows/ci.yml`
- Root `Cargo.toml`
- `crates/keld-cli/Cargo.toml`
- `crates/keld-core/Cargo.toml`
- `crates/keld-native/Cargo.toml`
- `crates/keld-pack/Cargo.toml`
- `crates/keld-wv/Cargo.toml`
- `packages/@keld/api/package.json`
- `packages/@keld/electron-updater/fixtures/package.json`
- `packages/@keld/electron-updater/package.json`
- `packages/@keld/electron/package.json`
- `packages/@keld/kipc/package.json`
- `packages/@keld/kipc/scripts/echo-codegen.ts`
- `tools/ci_workflow_security.test.ts`
- `tools/ci_workflow_security.ts`
- `tools/test_ci_local.py`

Changing these files in an ordinary feature PR still fails trusted
provenance after the changed-file count falls below 300. A feature PR
MUST NOT edit, skip, relax or impersonate that policy to approve itself.
Only the authorized trusted-base maintainers can design and approve a
revocation-aware policy update, or another explicitly trusted staged
integration path, **before** protected-input changes enter main.

## 3. Proposed dependency-aware review stages

Each stage MUST be independently reviewable, under the then-current
provenance limit, and must pass all required checks against its own
current main-based head. A maintainer's explicit merge of one stage into
main (when authorized) is a prerequisite for retargeting the next stage.
Do not cherry-pick, squash or drop the historical finding merely to evade
Gitleaks; keep the original source/commit provenance available for review.
The exact dependency graph and source census MUST be checked when each
stage is assembled; 435 non-authority files cannot safely be split using
filename count alone.

1. **Independent design and acceptance contract** — a main-based docs-only
   draft containing this plan and the unapproved signed-original
   single-graph amendment. This stage includes no compiled code or
   protected CI input. It requests independent security, packaging,
   original-app and CODEOWNERS decisions. A green CI check proves only the
   safety of the docs-only diff, not the implementation.
2. **Historical security adjudication** — the security owner privately
   inspects the ancestor's 32-character test literal, verifies legitimacy,
   and records a nonsecret decision. If it was real, rotate/revoke and
   follow the organization's incident and repository-history policy. A
   revised source integration plan must not claim Gitleaks green without an
   actual passing scanner on the submitted head.
3. **Trusted CI authority rollout** — maintainers review a **separate**
   trusted-base change that can authorize protected input mutations,
   verifying exact immutable base/head, complete file enumeration,
   up-to-date CODEOWNERS approval/revocation and publisher identity.
   Until such a review is approved, the 16 authority paths MUST remain
   blocked from untrusted feature PRs. No feature branch may self-approve.
4. **Foundational platform and package changes** — integrate only
   coherent, buildable and independently tested Core/KIPC/API/SDK slices,
   with any protected manifests handled by the *approved* authority
   mechanism. Verify core boot/graph, fixed-source registry and provider
   ABI before consumers depend on them. Review new package exports and
   security wrappers with their actual release tarballs.
5. **Host/Guardian and signed-original source** — after the foundation
   is merged, integrate the app-root bootstrap, deterministic Pack-time
   literal, independent installed no-follow byte comparison and Host
   original-entry selection. Preserve non-graph/dev direct-entry
   behavior and the original generated-launch-facts preload.
   Independently qualify changed `import.meta.main`, `Bun.main`
   and `process.argv[1]` semantics; never spoof them with globals.
6. **Test migration and final CI closure** — the current test owner
   commits or explicitly transfers the old `runtime-preload.test.ts`
   Windows migration, retaining real Init/ACK, Window/Compat
   subscriptions, Ready, one link, strict timeout, tamper and refusal
   assertions and the portable tar extraction correction. CI must
   actually run both the migrated legacy acceptance and other supported
   cross-platform controls; no skip or deadline inflation is acceptable.
   Keep signed-original product acceptance separate and mandatory.
7. **Signed installed release validation** — an exclusively authorized
   native owner tests the real pristine original app and signed,
   copied/re-located bundle and DMG. Verify Open/Edit/Save with XML
   readback, Cancel/Discard/dirty close/reopen, Quit, no orphaned
   sessions and all eight generated-source removal/tamper refusals.
   Only a final independently approved main-based PR with passing
   Gitleaks, provenance, CODEOWNERS, Rust, Bun, and aggregate
   `CI required` can be called merge-ready.

## 4. Immutable review/evidence obligations

For each stage, record the exact source commit SHA, parent/merge ancestry,
changed-file count, changed protected inputs, CI run IDs, failed job links,
original Mac executable identity and explicit missing coverage. Keep
the original #1447 branch and other owners' five uncommitted files
unchanged unless an independently recorded ownership transfer authorizes
one exact file. Do not mutate signed .app/DMG artifacts or native GUI state
without the native owner's approval.

The current staged draft #1472 intentionally preserves complete history
and remains **blocked, not ready to merge**. Neither its failed Gitleaks
check nor its rejected provenance check can be described as a
temporary infrastructure error or treated as approved just because
a narrower source-only bootstrap test passes.

## 5. Decisions requested

- **Security owner:** classify the historical test token privately and
  prescribe compliant credential and repository-history handling.
- **Trusted-base maintainers:** approve a revocation-safe protected-input
  path and dependency-ordered, fully enumerable main-based merge sequence.
- **Windows regression file owner:** commit the strict supported-startup
  migration or explicitly transfer just that path; preserve their four
  other dirty files and pending portable tar correction.
- **Signed-original product and CODEOWNERS reviewers:** explicitly accept
  or reject changed main/argv semantics after actual pristine draw.io,
  signed bundle, DMG and tamper acceptance evidence.

Until these recorded decisions and exact-head green checks exist, no
assistant or contributor should call the original PR or its integrated
source **release-ready**.
