# Spec: SDK package artifacts
Status: approved
Linear: GH-1287 (#517) · Owner: @0monish · Updated: 2026-10-31
Authority: Root's delegated decision recorded in GH-1287, under the user's explicit MVP decision delegation. The local-link correction was approved in GH-1287 comment 6470455185. The 2026-10-31 reconciliation (§1, criterion 7 and its follow-up, §4–§8, §10) records the landed branch diff and the current Node result; it is not self-approved and needs owner confirmation with the §8 gates.

## 1. Goal & non-goals

Produce installable API and canonical KIPC artifacts from their actual sources, with an explicit package dependency and a single transport Worker file. An offline consumer must exercise the actual tarballs. Node cold import of the installed artifacts now passes (criterion 7); installed-graph Source qualification remains an unresolved gate, not waived by this design.

Non-goals: registry publication; Electron member implementation; canonical transport runtime changes; a Node shim; new permissions, wire protocol or process owner; root CI policy other than the GH1301 dependency-preparation prerequisite and its policy bindings recorded in §5; Draw.io or whole-app performance claims.

## 2. Spec refs

Architecture 02 §1 owns the one transport Worker and its self-entry; architecture 03 owns default-deny; architecture 04 owns the Electron facade; architecture 01 §5 owns budgets. GH-527 owns Worker lifecycle. No architecture deviation is selected.

## 3. Acceptance criteria

1. Actual source manifests and pinned tools produce API and KIPC tarballs; an offline installed consumer resolves the declared exact KIPC edge without source adjacency. Removing the edge or an emitted module fails the consumer.
2. Strict TypeScript emission provides JavaScript and declarations for the API root and only internal/app, internal/link and internal/window-codec; a packed consumer typechecks and public/internal and ESM/CJS app identity agree. Public root runtime keys remain unchanged.
3. KIPC's existing release build and stamp owner produce one standalone self-Worker file. An intact installed artifact reaches a real private socket; a corrupt stamp prevents any accepted connection.
4. Source and test imports resolve one package transport identity; generated adapters come from their owner, and the existing staged sidecar preserves its qualified canonical external mapping for the bare import.
5. Repeated builds from identical inputs yield matching artifact file maps and hashes. Manifest derivation preserves source development dependencies and normalizes only the API internal runtime edge to the actual KIPC version.
6. Installed-graph lifecycle, protected hooks, terminal/recovery and Source retention are qualified with independent security/memory evidence. Source-only results do not prove this criterion.
7. Actual Node cold import of the installed artifacts succeeds: under Node 26.10 the offline consumer imports the API root, its internal entries and KIPC, `require` of the API root yields the same app, and API and KIPC share one transport identity (main fcf013a8, PR #1289, merged into this branch). Opening an app link under Node remains unqualified: `WorkerLink.open` refuses with typed `KELD-IPC-005` before reading options or constructing a MessageChannel, Worker or SharedArrayBuffer. No fake pass, shim, or alternate transport is permitted.

Open follow-up for the GH1301 CI owner: criterion 1 requires pinned tools, but the hosted Bun test lane runs this packed-consumer test with the runner-provided `node` and `npm`; no workflow pins Node (`setup-node` and `node-version` appear nowhere in `.github/workflows/*.yml`). Bun (1.4.4) and the compiler (lockfile `typescript` 7.0.2) are pinned. Hosted Node and npm evidence is therefore unpinned and is not claimed pinned here until that owner pins Node with a SHA-pinned setup step or scopes Node out of the lane.

## 4. Design

API source uses `@keld/kipc`, with `link:../kipc` as its real local development dependency. API source exports enumerate its four entries. A KIPC-owned cold producer invokes the existing pinned compiler with strict options, unbundled JS, declarations and relative extension rewrite. It derives distribution manifests from source, changes the one API dependency to the actual KIPC version, and packs real output. KIPC emits declarations and uses its one standalone Bun release build, `buildReleaseTransport` in `scripts/package-artifacts.ts`, with the exported stamp/verification helpers; `src/release-build.test.ts` checks that same function, so hook stripping and Node import are proven on the build that ships. It remains one readable Worker self-entry; the transport is not bundled into API.

Ownership, trust and lifecycle remain with the existing API and Worker. This changes package resolution and distribution boundaries, not handle ownership, principal minting, wire protocol or permission model. Existing source-relative imports were rejected because they escape an installed package. A bundle-time resolver plugin was rejected because standard emission works and source dependency ownership is the root cause. The ordinary local Bun link resolves to the canonical source realpath, avoiding the distinct module identity observed with `file:../kipc`; normal and frozen installs and source namespace/class identity are required. The distribution manifest still declares the exact KIPC version. Compatibility fallback: source development uses the local declared dependency. No performance improvement is claimed.

Caller-directory reproducibility: Bun writes a caller-relative source-path comment into the bundled Worker, so the stamped bytes and their SHA-256 would depend on the producer's working directory. `buildReleaseTransport` therefore runs `process.chdir` to the canonical KIPC directory and restores the caller's directory in `finally`. Rejected alternative: `Bun.build`'s `root:` option does not fix the comment. A Bun 1.4.4 probe on 2026-10-31 with `root` set to the KIPC directory still emitted `// ../../…/packages/@keld/kipc/src/transport.ts` from a scratch directory and `// Users/…/src/transport.ts` from `/`; only a build run from the KIPC directory emitted `// src/transport.ts`.

Types/channels: no wire types or channels change. Capabilities and permission manifest: none. Runtime seam: package loading changes from repository-relative to declared package resolution; the process, grants, callback lifetimes and failure semantics remain owned by existing implementations. Migration includes source callers, generated adapters, mocks and staged-sidecar mapping; no permanent adapter is added. Platform behavior is unqualified beyond recorded Bun probes and the Node 26.10 cold-import result in criterion 7; Node link opening is refused, not qualified.

## 5. Boundaries

Implement in API package manifests, compiler config, transport imports, relevant tests and staged fixture; KIPC build config, package manifest, generator and artifact producer/tests; this spec.

The branch also changes these shared files, without which hosted CI cannot pass this design (review gates in §8):

- GH1301 CI dependency preparation, carried from d181830c and 4b5c2ade (neither is on main yet). `.github/workflows/ci.yml` installs the frozen kipc, api and electron graphs before the native `check` job's clippy and tests, because the Rust-hosted Bun corpus executes actual package sources that now resolve `@keld/kipc` through the declared edge and a fresh hosted checkout has no package-local `node_modules`. The `bun-test` job installs every selected package before any package's typecheck or test, because the API suite runs KIPC's pinned compiler and package graph and no `node_modules` survives between jobs. `tools/ci_workflow_security.ts` binds the `bun-test` job's execution context (exact keys, name, runner, timeout, needs, condition, OS matrix) and freezes the `check` and `bun-test` step inventories by SHA-256; `tools/ci_workflow_security.test.ts` covers both contracts. `.agents/ci.md` makes CI routing an independently reviewed shared-file concern owned by its own issue (GH1301); if GH1301 lands first, these hunks should become empty on rebase.
- `tools/ci-inputs.json` rebinds the reader digests whose readers this branch changes (Bun package, Rust test and policy readers).
- `.gitleaksignore` admits one fingerprint-specific finding: the SHA-256 literal in `window-source-semantic-release.test-role.ts` is a hash oracle, not a credential.
- Staged-source canonical KIPC resolution (test-only Rust, macOS): `crates/keld-host/tests/no_flag/macos/renderer_bridge.rs` stages byte-identical package manifests plus the one API-to-staged-KIPC package link, and `staged_source_packages_resolve_one_canonical_kipc_and_refuse_a_missing_edge` proves the staged API consumes canonical KIPC and fails when only that edge is removed.
- `docs/agents/learnings.md` gains one test-area entry.

Do not edit canonical transport runtime, Electron implementation, production Rust, root CI/policy beyond the files above, or unrelated owners.

## 6. Tasks

- [x] T1 Land actual-pack failure-first regression and implement the declared package graph and artifact producer. Evidence: regression commit 6faae784 precedes the graph and producer commits bda41b1f and d5cf1f2d (its RED run is not re-executed here); on d8e49206, `env -u CLAUDECODE bun test src/package-artifact.test.ts` in `packages/@keld/api` passes 2/0 and logs `"runtimes":{"bun":"1.4.4","node":"v26.10.0"}` (darwin arm64, 2026-10-31).
- [ ] T2 Qualify artifact closure, reproducibility, Worker integrity, lifecycle and Source representation with negative controls and independent review. Landed in `package-artifact.test.ts`: missing-edge repack and removed installed KIPC package controls, byte-equal tarballs from two producer directories, a corrupted stamp with zero accepted connections, installed initial and recovery lifecycle with two `link.js` mutations, and Source witnesses on installed emitted JS; the staged-source Rust test in §5 (not run in this reconciliation). Open: independent security/memory review on the exact final diff (criterion 6).
- [ ] T3 Reconcile separately owned Node prerequisite, routed gates and integration evidence before merge. Done: the Node prerequisite (fcf013a8, PR #1289) is merged here and criterion 7 records its result. Open: the §8 gates, hosted `CI required` at the final head, and the unpinned Node/npm follow-up under §3.

## 7. Test plan

Pack and offline install actual tarballs; execute Bun import/require, root/internal identity and missing-edge controls; typecheck installed declarations; compare two builds; run generated checks and source suites. Exercise installed Worker against a private socket and corrupt the stamp before connection; the installed packed `transport.js` must name none of KIPC's `TEST_HOOK_NAMES`. Run lifecycle/retention witnesses on emitted installed paths and independent security/memory review. Node cold import is required GREEN: the packed-consumer probe runs under Bun and Node and must exit 0, and KIPC `src/release-build.test.ts` proves Node link opening is refused before options are read. The macOS host suite runs the staged-source resolution test; `tools/ci_workflow_security.test.ts` runs the CI policy contracts. Tests isolate temporary paths and processes rather than relying on timing or shared module mocks. OS acceptance is not inferred from CI-only evidence.

## 8. Review gates triggered

Public API and dependency graph: named independent architecture/security review on the exact final diff. CI and shared policy (`.github/workflows/ci.yml`, `tools/ci_workflow_security.ts`, `tools/ci-inputs.json`, `.gitleaksignore`): named independent CI/security review on the exact final diff, per `.agents/ci.md`. Unsafe, permission model and wire protocol: none; the Rust change is test-only and adds no `unsafe`.

## 9. Perf impact

No performance claim. Architecture 01 §5 Source and memory budgets require qualification of the emitted graph; prior source evidence is not transferable proof.

## 10. Open questions

None for the admitted design. Installed-graph Source qualification (criterion 6) is an acceptance hold, not a design decision. Pinning Node in hosted CI (§3 follow-up) belongs to the GH1301 CI owner.
