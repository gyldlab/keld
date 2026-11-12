# Spec: SDK package artifacts
Status: approved
Linear: GH-1287 (#517) · Owner: @0monish · Updated: 2026-10-28
Authority: Root's delegated decision recorded in GH-1287, under the user's explicit MVP decision delegation. The local-link correction was approved in GH-1287 comment 6470455185.

## 1. Goal & non-goals

Produce installable API and canonical KIPC artifacts from their actual sources, with an explicit package dependency and a single transport Worker file. An offline consumer must exercise the actual tarballs. Node cold import and installed-graph Source qualification are unresolved gates, not waived by this design.

Non-goals: registry publication; Electron member implementation; canonical transport runtime changes; a Node shim; new permissions, wire protocol, process owner, or root CI policy; Draw.io or whole-app performance claims.

## 2. Spec refs

Architecture 02 §1 owns the one transport Worker and its self-entry; architecture 03 owns default-deny; architecture 04 owns the Electron facade; architecture 01 §5 owns budgets. GH-527 owns Worker lifecycle. No architecture deviation is selected.

## 3. Acceptance criteria

1. Actual source manifests and pinned tools produce API and KIPC tarballs; an offline installed consumer resolves the declared exact KIPC edge without source adjacency. Removing the edge or an emitted module fails the consumer.
2. Strict TypeScript emission provides JavaScript and declarations for the API root and only internal/app, internal/link and internal/window-codec; a packed consumer typechecks and public/internal and ESM/CJS app identity agree. Public root runtime keys remain unchanged.
3. KIPC's existing release build and stamp owner produce one standalone self-Worker file. An intact installed artifact reaches a real private socket; a corrupt stamp prevents any accepted connection.
4. Source and test imports resolve one package transport identity; generated adapters come from their owner, and the existing staged sidecar preserves its qualified canonical external mapping for the bare import.
5. Repeated builds from identical inputs yield matching artifact file maps and hashes. Manifest derivation preserves source development dependencies and normalizes only the API internal runtime edge to the actual KIPC version.
6. Installed-graph lifecycle, protected hooks, terminal/recovery and Source retention are qualified with independent security/memory evidence. Source-only results do not prove this criterion.
7. Actual Node cold import remains a failing separately owned prerequisite until canonical transport portability is corrected and requalified. No fake pass, shim, or alternate transport is permitted.

## 4. Design

API source uses `@keld/kipc`, with `link:../kipc` as its real local development dependency. API source exports enumerate its four entries. A KIPC-owned cold producer invokes the existing pinned compiler with strict options, unbundled JS, declarations and relative extension rewrite. It derives distribution manifests from source, changes the one API dependency to the actual KIPC version, and packs real output. KIPC emits declarations and uses the existing standalone Bun release build and exported stamp/verification helpers. It remains one readable Worker self-entry; the transport is not bundled into API.

Ownership, trust and lifecycle remain with the existing API and Worker. This changes package resolution and distribution boundaries, not handle ownership, principal minting, wire protocol or permission model. Existing source-relative imports were rejected because they escape an installed package. A bundle-time resolver plugin was rejected because standard emission works and source dependency ownership is the root cause. The ordinary local Bun link resolves to the canonical source realpath, avoiding the distinct module identity observed with `file:../kipc`; normal and frozen installs and source namespace/class identity are required. The distribution manifest still declares the exact KIPC version. Compatibility fallback: source development uses the local declared dependency. No performance improvement is claimed.

Types/channels: no wire types or channels change. Capabilities and permission manifest: none. Runtime seam: package loading changes from repository-relative to declared package resolution; the process, grants, callback lifetimes and failure semantics remain owned by existing implementations. Migration includes source callers, generated adapters, mocks and staged-sidecar mapping; no permanent adapter is added. Platform behavior is unqualified beyond recorded Bun probes; Node 26.10 fails at canonical MessageChannel descriptor capture.

## 5. Boundaries

Implement in API package manifests, compiler config, transport imports, relevant tests and staged fixture; KIPC build config, package manifest, generator and artifact producer/tests; this spec. Do not edit canonical transport runtime, Electron implementation, root CI/policy, Rust or unrelated owners.

## 6. Tasks

- [ ] T1 Land actual-pack failure-first regression and implement the declared package graph and artifact producer.
- [ ] T2 Qualify artifact closure, reproducibility, Worker integrity, lifecycle and Source representation with negative controls and independent review.
- [ ] T3 Reconcile separately owned Node prerequisite, routed gates and integration evidence before merge.

## 7. Test plan

Pack and offline install actual tarballs; execute Bun import/require, root/internal identity and missing-edge controls; typecheck installed declarations; compare two builds; run generated checks and source suites. Exercise installed Worker against a private socket and corrupt the stamp before connection. Run lifecycle/retention witnesses on emitted installed paths and independent security/memory review. Node's actual cold import remains RED until its separate owner fixes it. Tests isolate temporary paths and processes rather than relying on timing or shared module mocks. OS acceptance is not inferred from CI-only evidence.

## 8. Review gates triggered

Public API and dependency graph: named independent architecture/security review on the exact final diff. Unsafe, permission model and wire protocol: none.

## 9. Perf impact

No performance claim. Architecture 01 §5 Source and memory budgets require qualification of the emitted graph; prior source evidence is not transferable proof.

## 10. Open questions

None for the admitted design. The Node portability prerequisite and Source qualification are acceptance holds, not design decisions.
