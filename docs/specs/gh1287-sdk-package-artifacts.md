# Spec: SDK package artifacts

- Status: approved
- Tracker: GH-1287
- Owner: 0monish
- Updated: 2026-11-19
- Pin: 206fd171f2f726de27842a4e28812d873cb52242

## 1. Goal & non-goals

Produce installable API and canonical KIPC artifacts from their actual sources.
Use an explicit package dependency and one transport Worker file.
An offline consumer must exercise the actual tarballs.
Node cold import of the installed artifacts passes (AC-7).
Installed-graph Source qualification remains an open gate (AC-6).

Non-goals:

- Registry publication or Electron member implementation.
- Canonical transport runtime changes or a Node shim.
- New permissions, wire protocol or process owner.
- Root CI policy outside the GH1301 dependency-preparation prerequisite.
- Draw.io or whole-app performance claims.

## 2. Spec refs

- Architecture 02 §1 owns the one transport Worker and its self-entry.
- Architecture 03 owns default-deny.
- Architecture 04 owns the Electron facade.
- Architecture 01 §5 owns budgets.
- GH-527 owns Worker lifecycle.
- No architecture deviation is selected.

**Approval authority:** Root's delegated decision is recorded in GH-1287.
The user's explicit MVP decision delegation covers the approved design.
The local-link correction was approved in GH-1287 comment 6470455185.
The 2026-10-31 reconciliation records the landed branch diff and current Node result.
It is not self-approved. The owner must confirm this exact structured restatement
with the review gates in §8.

## 3. Acceptance criteria

### AC-1: actual sources produce offline-installable artifacts

- **Given:** Actual API and KIPC source manifests and pinned build tools.
- **When:** The producer builds both real tarballs for an offline consumer.
- **Then:** The consumer resolves the exact declared KIPC dependency without source adjacency.
- **Negative control:**
  - **Single fault:** Remove the dependency edge or an emitted module.
  - **At-risk precondition:** The offline consumer installs the produced tarballs.
  - **Expected RED:** The consumer cannot resolve the exact package graph.

### AC-2: TypeScript output preserves the public and internal contract

- **Given:** The API root and its four declared export entries.
- **When:** Strict TypeScript emission and the packed consumer typecheck run.
- **Then:** JavaScript and declarations exist for the API root and only
  internal/app, internal/link and internal/window-codec. Public/internal and
  ESM/CJS app identity agree. Public root runtime keys remain unchanged.
- **Negative control:**
  - **Single fault:** Remove a required declaration or change the public root identity.
  - **At-risk precondition:** The installed consumer imports and typechecks the package.
  - **Expected RED:** The typecheck or identity oracle fails.

### AC-3: KIPC ships one integrity-checked standalone Worker

- **Given:** The existing KIPC release-build and stamp owner.
- **When:** An intact installed artifact reaches a real private socket.
- **Then:** One standalone self-Worker file accepts the connection.
- **Negative control:**
  - **Single fault:** Corrupt the artifact stamp.
  - **At-risk precondition:** The consumer attempts the real private-socket connection.
  - **Expected RED:** Zero connections are accepted.

### AC-4: all consumers resolve one canonical transport identity

- **Given:** Source/test imports, generated adapters and the staged sidecar.
- **When:** Each consumer resolves the declared package transport.
- **Then:** All imports resolve one package identity. Generated adapters come
  from their owner. The staged sidecar preserves its qualified canonical
  external mapping.
- **Negative control:**
  - **Single fault:** Remove the canonical edge or introduce a second transport path.
  - **At-risk precondition:** Source, tests and the staged consumer import KIPC.
  - **Expected RED:** The package identity oracle detects different instances.

### AC-5: artifact output is reproducible and allowlisted

- **Given:** Identical source inputs and pinned producer tools.
- **When:** The producer builds from two different working directories.
- **Then:** Artifact file maps and hashes match. Distribution manifests use
  only allowlisted source fields. Source dev dependencies remain and only the
  API internal runtime edge is normalized to the actual KIPC version. The
  manifest never ships scripts or packageManager.
- **Negative control:**
  - **Single fault:** Build from a different caller directory or add a forbidden manifest field.
  - **At-risk precondition:** Both builds use the same source inputs.
  - **Expected RED:** The hash or manifest allowlist oracle fails.

### AC-6: installed-graph Source and lifecycle are independently qualified

- **Given:** Installed artifacts, protected hooks and retained Source evidence.
- **When:** Lifecycle, terminal/recovery and Source-retention checks run with
  independent security and memory evidence.
- **Then:** The installed graph meets the required security and memory contract.
  Source-only results do not prove this criterion.
- **Negative control:**
  - **Single fault:** Remove a protected hook or alter retained Source evidence.
  - **At-risk precondition:** The installed graph reaches the relevant lifecycle path.
  - **Expected RED:** The independent security or memory oracle fails.
- **Status:** Open. The required independent review and staged-source Rust test
  are not complete in this reconciliation.

### AC-7: Node cold import succeeds and app-link opening refuses

- **Given:** Node 26.10 and the offline installed artifacts.
- **When:** The consumer imports the API root, internal entries and KIPC, then
  CommonJS require loads the API root.
- **Then:** The same app and one transport identity are observed.
  WorkerLink.open refuses with KELD-IPC-005 before reading options or
  constructing MessageChannel, Worker or SharedArrayBuffer.
- **Negative control:**
  - **Single fault:** Permit WorkerLink.open or change the package identity.
  - **At-risk precondition:** The Node consumer attempts the installed app-link path.
  - **Expected RED:** The typed refusal or identity test fails.

## 4. Design

### 4.1 Package graph and artifact producer

API source uses `@keld/kipc` with `link:../kipc` as its local development dependency.
API source exports enumerate its four entries.

A KIPC-owned cold producer invokes the existing pinned compiler.
It uses strict options, unbundled JavaScript, declarations and relative extension rewrite.
It derives distribution manifests from an allowlist:
name, version, description, license, type, engines, exports, dependencies,
peerDependencies and devDependencies.
It changes the one API dependency to the actual KIPC version.
It packs the real output.

Source scripts and packageManager are never shipped.
The scripts name files that the tarball omits.
An install script would run on consumers' machines.

KIPC emits declarations and uses one standalone Bun release build.
The producer is `buildReleaseTransport` in `scripts/package-artifacts.ts`.
It uses the exported stamp and verification helpers.
`src/release-build.test.ts` checks that same function.
This proves hook stripping and Node import on the build that ships.
The Worker remains one readable self-entry.
The transport is not bundled into API.

### 4.2 Ownership, trust and lifecycle

Existing API and Worker owners retain ownership, trust and lifecycle.
This change affects package resolution and distribution boundaries only.
It does not change handle ownership, principal minting, wire protocol or permissions.

Source-relative imports were rejected because they escape an installed package.
A bundle-time resolver plugin was rejected because standard emission works.
The root cause is source dependency ownership.
The ordinary local Bun link resolves to the canonical source realpath.
This avoids the distinct module identity observed with `file:../kipc`.
Normal and frozen installs and source namespace/class identity are required.
The distribution manifest still declares the exact KIPC version.
Source development uses the local declared dependency.
No performance improvement is claimed.

### 4.3 Caller-directory reproducibility

Bun writes a caller-relative source-path comment into the bundled Worker.
The stamped bytes and SHA-256 would depend on the producer working directory.
`buildReleaseTransport` therefore changes to the canonical KIPC directory.
It restores the caller directory in `finally`.

Rejected alternative: Bun.build's `root:` option does not fix the comment.
A Bun 1.4.4 probe ran on 2026-10-31.
With `root` set to KIPC, a scratch directory emitted
`// ../../…/packages/@keld/kipc/src/transport.ts`.
A build from `/` emitted `// Users/…/src/transport.ts`.
Only a build from the KIPC directory emitted `// src/transport.ts`.

### 4.4 Types, channels and migration

No wire types or channels change.
Capabilities and permission manifest do not change.
Package loading changes from repository-relative to declared package resolution.
Existing implementations retain process, grants, callback lifetimes and failure semantics.

Migration includes source callers, generated adapters, mocks and staged-sidecar mapping.
No permanent adapter is added.
Platform behavior is unqualified beyond recorded Bun probes and the Node 26.10
cold-import result in AC-7.
Node app-link opening is refused, not qualified.

## 5. Boundaries

Implement in:

- API package manifests, compiler config and transport imports.
- Relevant API tests and staged fixture.
- KIPC build config, package manifest, generator and artifact producer/tests.
- This specification.

The branch also changes shared files. These changes need the review gates in §8.

- **GH1301 CI dependency preparation:** carried from d181830c and 4b5c2ade.
  Neither commit is on main yet.
  - `.github/workflows/ci.yml` installs frozen kipc, api and electron graphs
    before native `check` clippy and tests.
  - The Rust-hosted Bun corpus executes package sources that resolve
    `@keld/kipc` through the declared edge.
  - A fresh hosted checkout has no package-local `node_modules`.
  - The `bun-test` job installs every selected package before typecheck or test.
  - The API suite runs KIPC's pinned compiler and package graph.
  - No `node_modules` survives between jobs.
  - `tools/ci_workflow_security.ts` binds the `bun-test` execution context:
    exact keys, name, runner, timeout, needs, condition and OS matrix.
  - It freezes the `check` and `bun-test` step inventories by SHA-256.
  - `tools/ci_workflow_security.test.ts` covers both contracts.
  - `.agents/ci.md` assigns shared CI routing to GH1301.
  - If GH1301 lands first, these hunks must become empty on rebase.
- `tools/ci-inputs.json` rebinds reader digests changed by this branch:
  Bun package, Rust test and policy readers.
- `.gitleaksignore` admits one fingerprint-specific finding.
  The SHA-256 literal in `window-source-semantic-release.test-role.ts`
  is a hash oracle, not a credential.
- Staged-source canonical KIPC resolution is test-only Rust on macOS:
  `crates/keld-host/tests/no_flag/macos/renderer_bridge.rs` stages
  byte-identical package manifests and one API-to-staged-KIPC package link.
  `staged_source_packages_resolve_one_canonical_kipc_and_refuse_a_missing_edge`
  proves the staged API consumes canonical KIPC and fails when the edge is removed.
- `docs/agents/learnings.md` gains one test-area entry.

Do not edit canonical transport runtime or Electron implementation.
Do not edit production Rust.
Do not edit root CI/policy outside the files above.
Do not edit unrelated owners.

## 6. Tasks

### T-1: build the declared package graph and artifacts

- **Blocked by:** none
- [x] Land actual-pack failure-first regression.
- [x] Implement the declared package graph and artifact producer.
- Evidence: regression commit 6faae784 precedes graph and producer commits
  bda41b1f and d5cf1f2d. Its RED run is not re-executed here.
- On d8e49206, `env -u CLAUDECODE bun test src/package-artifact.test.ts`
  in `packages/@keld/api` passes 2/0.
- The run logs Bun 1.4.4 and Node v26.10.0 on darwin arm64 (2026-10-31).

### T-2: qualify artifact closure and installed lifecycle

- **Blocked by:** T-1
- [ ] Qualify artifact closure, reproducibility, Worker integrity, lifecycle and Source.
- [ ] Add negative controls and independent security/memory review.
- Landed in `package-artifact.test.ts`: missing-edge repack and removed installed
  KIPC package controls; byte-equal tarballs from two producer directories;
  corrupted stamp with zero accepted connections; installed initial and recovery
  lifecycle with two `link.js` mutations; Source witnesses on installed emitted JS.
- The staged-source Rust test in §5 was not run in this reconciliation.
- Independent security/memory review on the exact final diff remains open (AC-6).

### T-3: close CI owner and integration gates

- **Blocked by:** T-2
- [ ] Reconcile separately owned Node prerequisite and routed gates.
- [ ] Complete integration evidence before merge.
- Done: Node prerequisite fcf013a8 (PR #1289) is merged here.
- AC-7 records its result.
- Open: §8 gates, hosted `CI required` at final head and the Node/npm follow-up in §10.

## 7. Test plan & evidence

- Pack and install actual tarballs offline.
- Execute Bun import/require, root/internal identity and missing-edge controls.
- Typecheck installed declarations.
- Compare two builds.
- Run generated checks and source suites.
- Exercise installed Worker against a private socket.
- Corrupt the stamp before connection.
- The installed packed `transport.js` must name none of KIPC's `TEST_HOOK_NAMES`.
- Run lifecycle/retention witnesses on installed paths.
- Obtain independent security/memory review.
- Node cold import is required GREEN.
- The packed-consumer probe runs under Bun and Node and must exit 0.
- KIPC `src/release-build.test.ts` proves Node link opening refuses before
  options are read.
- The macOS host suite runs the staged-source resolution test.
- `tools/ci_workflow_security.test.ts` runs CI policy contracts.
- Tests isolate temporary paths and processes rather than relying on timing or mocks.
- OS acceptance is not inferred from CI-only evidence.

### E-1: Node cold import evidence

- **Run:** `env -u CLAUDECODE bun test src/package-artifact.test.ts`
- **n:** 2 passed, 0 failed in the API package test.
- **Environment:** darwin arm64, Bun 1.4.4, Node v26.10.0, source d8e49206.
- **Limits:** This proves cold import and typed app-link refusal only.
  It does not prove installed-graph Source qualification or native OS acceptance.

## 8. Review gates triggered

- **Public API and dependency graph:** named independent architecture/security review
  on the exact final diff.
- **CI and shared policy:** named independent CI/security review on the exact final diff,
  per `.agents/ci.md`.
- **Unsafe, permission model and wire protocol:** none.
- The Rust change is test-only and adds no `unsafe`.

## 9. Perf impact

No performance claim.
Architecture 01 §5 Source and memory budgets require qualification of the emitted graph.
Prior source evidence is not transferable proof.

## 10. Open questions, blockers & unknowns

No unresolved design question remains for the admitted design.

### B-1: Hosted Node and npm are not pinned

- **Status:** open
- **Owner:** GH1301 CI owner
- **Impact:** Criterion 1 requires pinned tools, but the hosted Bun test lane
  runs the packed consumer with runner-provided Node and npm.
- **Check:** Pin Node with a SHA-pinned setup step or scope Node out of the lane.
- Bun 1.4.4 and compiler TypeScript 7.0.2 are pinned.
- No workflow pins Node; setup-node and node-version appear nowhere in
  `.github/workflows/*.yml`.
- Hosted Node/npm evidence remains unpinned until the owner resolves this blocker.

### B-2: installed-graph Source qualification is open

- **Status:** open
- **Owner:** independent security/memory reviewer
- **Impact:** AC-6 remains unqualified; Source-only results do not prove the installed graph.
- **Check:** Complete the installed staged-source test and independent security/memory
  review on the exact final diff.

### B-3: final hosted CI and shared-file review are open

- **Status:** open
- **Owner:** GH-1287 PR owner and GH1301 CI owner
- **Impact:** The shared CI and package changes are not merge-ready.
- **Check:** Obtain independent exact-head review and green hosted `CI required`.
