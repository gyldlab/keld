# GH623 proposal evidence — 23 November 2026

This directory preserves the KIPC resource investigation used to amend draft
[PR1413](https://github.com/gyldlab/keld/pull/1413).
The canonical tracker is [GH623](https://github.com/gyldlab/keld/issues/623).

**Status: historical baseline evidence and proposed acceptance design.**
This is scoped proposal evidence, not a registered audit certification or
acceptance of a bounded implementation. B-1, B-2 and B-3 remain open.

## Scope and integrity

- Audited KELD source: `269d51528137faf574cfd3548586ee3ada1acfda`.
- Proposal reviewed during capture: `870d1c693214e5b20bc8cdc900afa3ded67e2823`.
- [Complete structured audit](audit.json): 14 defects, 12 stage inventories,
  24 regression designs, 39 source references and all report blocks.
- [Manifest](manifest.json): original-artifact and published-file SHA-256 values.
- [Original architecture proposal](proposed-architecture-02.patch): historical
  audit output, re-emitted with zero context. The edited architecture02 also
  clarifies subsystem ownership.

The original PDF and archive remain preserved separately, without modification.
The manifest records their hashes. Public receipts redact temporary machine paths
and connector routing metadata. They are not all byte-identical raw originals.
Each manifest entry identifies its exact transformation. Measurements, source pins,
runtime versions, test outcomes and repository-relative locations are preserved.
Redundant final blank lines are removed from public receipts. The historical
architecture patch requires `git apply --unidiff-zero`; its result is unchanged.

The three public TypeScript probes are Bun-formatted derivatives. Their original
executed bytes remain in the preserved archive; the manifest binds both hashes.
The [formatted-source replay](receipts/formatted-probes-replay.json) separately
records five successful executions on the exact baseline and the formatted-source
hashes. It reproduced the same defect observations without replacing any receipt.
The edited architecture also narrows the historical patch's decode allocation
sentence: allocation depends on the schema; unit variants do not require owned
string/vector allocation, and caller-storage encoding is available.

The report blocks describe the audit session before this publication.
Their statement that no branch, issue or PR changed applies to that capture session.
This publication changes documentation and evidence only.

The existing public-audit registry remains unchanged. Its verifier has a different
full-spectrum schema. Running that verifier is a regression check, not validation
of this directory's scope or empirical claims.

## Executed observations

Environment: macOS 26.5.1 build 25F80, Darwin 25.5.0, arm64; Bun 1.4.4 build
38afee6c8; Rust and Cargo 1.99.1. Native transport probes used Unix sockets.

| Witness | Observed result | Receipt |
|---|---|---|
| Injected stalled writer | 2,048 retained 64 KiB payloads; 134,217,728 bytes; zero completed writes; one write attempt. All completed after release. | [large resource probe](receipts/ts-resource-probe-large-results.json) |
| Real WorkerLink, paused peer | 512 EVENTs / 33,554,432 payload bytes accepted; peer read 48 HELLO bytes. | [native pipeline](receipts/ts-native-probe-results.txt) |
| Handler retention | Eight ring slots admitted 512 unresolved handlers. All continuations resumed after close when released. | [native pipeline](receipts/ts-native-probe-results.txt) |
| Backing and mutation | A one-byte view cloned an 8 MiB backing. Direct queued mutation changed the later wire byte. | [resource probe](receipts/ts-resource-probe-large-results.json) |
| Shared input | Shared backing remained shared; receiver observed post-send value 66. | [SAB probe](receipts/ts-sab-probe-result.json) |
| Generated encoder | Allocated 16,777,217 UTF-8 bytes before the over-limit refusal. | [resource probe](receipts/ts-resource-probe-large-results.json) |
| Host UI component | 4,096 Accepted setter replies; 4,096 retained commands; one window; no consumer progress. | [Rust output](receipts/rust_probe_output.json) |
| AuthRequest decoder | 262,151 wire bytes; 262,144 empty strings; vector capacity 349,520; vector backing 8,388,480 bytes. | [Rust output](receipts/rust_probe_output.json) |
| Default FS receive profile | Valid 2 MiB content produced a 2,097,173-byte envelope and KELD-IPC-026 in the default 1 MiB ring. | [FS capacity](receipts/ts-fs-capacity-result.txt) |
| Existing tests | 44 transport tests passed. All ten Worker tests passed after correcting subprocess PATH. | [initial run](receipts/ts-existing-tests-initial.txt), [Worker rerun](receipts/ts-worker-tests-rerun.txt) |

These probes demonstrate baseline defects. A passing witness is not a positive
boundedness result. No throughput, latency improvement, RSS ceiling, mailbox/kernel
split, full workspace acceptance, or Windows/Linux qualification is claimed.
The two Rust witnesses exercised Core component dispatch and literal decoding.
They did not exercise AppKit or native auth policy. The FS fixture exercised a real
WorkerLink and Unix socket, but did not invoke the actual Host filesystem handler.

## Existing finding owners

| Findings | Canonical work owner | First remaining acceptance check |
|---|---|---|
| D01, D02, D03, D04, D07, encoder part of D08, D11 | [GH623](https://github.com/gyldlab/keld/issues/623) / existing PR1413 | Approve B-1/B-2; implement first-retention byte/record accounting and run independent mutants. |
| Receive part of D08, D09, D10 | [GH528](https://github.com/gyldlab/keld/issues/528) | Account actual reader storage; qualify legal FS response capacity and an absolute Rust frame-write clock. |
| D05, D06 | [GH449](https://github.com/gyldlab/keld/issues/449) | Stall physical UI consumption; preserve permits through Tao and bound native facts before filtering. |
| Rust/schema part of D08 | [GH824](https://github.com/gyldlab/keld/issues/824), with existing schema/Core owners | Reject cardinality/decoded-byte excess before allocation; retain the literal fixture and independent allocation oracle. |
| D12 | [GH1097](https://github.com/gyldlab/keld/issues/1097) | Keep the link alive while a renderer call goes unanswered; require an absolute response deadline and no replay. |
| D13 | [GH1239](https://github.com/gyldlab/keld/issues/1239) | Reap exited readers without blocking the guardian acknowledgement owner; bound handles and stop registrations. |
| D14 | [GH824](https://github.com/gyldlab/keld/issues/824), architecture02 amendment in PR1413 | Review 022/024 and Current/Target corrections against the approved GH527 contract and GH1132 retry owner. |

[GH540](https://github.com/gyldlab/keld/issues/540) has concurrent transport work.
This baseline does not qualify that candidate. Its owner must reconcile actual
ingress storage, large response capacity and terminal behavior at its own source pin.
No production path from that workstream is modified here.

## Reproduce on the exact baseline

Use a disposable checkout at the audited commit. Set the variables to the local
repository and this evidence directory. Ensure `bun` is in PATH because the existing
Worker tests spawn it by name. The offline Cargo command needs the repository's
normal platform dependencies and dependency cache.

```bash
kipc_snapshot=$(mktemp -d)
git -C "$kipc_repo" archive 269d51528137faf574cfd3548586ee3ada1acfda | tar -x -C "$kipc_snapshot"
mkdir "$kipc_snapshot/audit-ts"
cp "$kipc_evidence"/probes/*.ts "$kipc_snapshot/audit-ts/"
cd "$kipc_snapshot"
bun audit-ts/ts-resource-probe.ts
KIPC_AUDIT_FRAMES=2048 KIPC_AUDIT_BUDGET=67108928 bun audit-ts/ts-resource-probe.ts
bun audit-ts/ts-native-pipeline-probe.ts outbound
bun audit-ts/ts-native-pipeline-probe.ts handlers
bun audit-ts/ts-fs-capacity-probe.ts
git apply --unidiff-zero --check "$kipc_evidence/probes/rust_resource_probes.patch"
git apply --unidiff-zero "$kipc_evidence/probes/rust_resource_probes.patch"
cargo test --offline -p keld-core --lib resource_audit_ -- --nocapture
bun test packages/@keld/kipc/src/transport.test.ts packages/@keld/kipc/src/worker-link.test.ts
```

The Rust patch has zero context. Check the exact source pin before applying it.
Expected Rust witness totals are two passed, zero failed, and 267 filtered out.
The original initial TS run retained four missing-PATH failures. The corrected
Worker run passed all ten tests. This yields 54 distinct passing existing tests
across the corrected runs, not 54 new boundedness tests.

The separate SAB observation came from an inline command. Equivalent reproduction:

```bash
bun -e 'import {MessageChannel,receiveMessageOnPort} from "node:worker_threads"; const c=new MessageChannel(); const a=new Uint8Array(new SharedArrayBuffer(1)); a[0]=65; c.port1.postMessage(a); a[0]=66; const b=receiveMessageOnPort(c.port2).message; console.log(JSON.stringify({shared:b.buffer instanceof SharedArrayBuffer,received:b[0]})); c.port1.close(); c.port2.close();'
```

## Qualification remains open

The complete audit defines T01 through T24 with required positive observations,
single-fault mutants and execution status. These proposed implementation/mutant
pairs remain unrun. Physical UI, native Rust drip-write, actual Host FS compatibility,
new-ledger Worker death, and restart qualification also remain open.

- Handler-count isolation needs at least two answer tickets, enough answer bytes,
  and input capacity for two handlers. Otherwise another guard masks the mutant.
- Atomicity requires a real main-thread reservation versus Worker release race.
  Concurrent Promises on one JavaScript agent are insufficient.
- CALLs prepay abandon metadata. Terminal retirement must need no fresh admission.
- The parked-main test releases credit in the exact partition needed for progress.
- The numeric budget/clock profile in the audit is a test profile. It does not select
  shipping defaults or establish measured copy overlap.

The local `KELD-IPC-005` capacity extension and shared-input treatment remain B-2
proposals. No error code alone authorizes replay. Local accounting changes no v2
wire kind. GRANT and remote cancellation remain separately reviewed work.
