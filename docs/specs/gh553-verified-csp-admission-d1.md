# D1 source contract: verified CSP catalog and cold Host/package pairing
Status: approved (bounded source slice; facade/native production activation held)
Linear: GH-553 (#517), consuming KEL102/GH473/GH499 · Owner: @0monish through explicit delegated in-scope agent decisions · Updated: 2026-11-01

Approval source: direct human steering, relayed by root, delegates these in-scope
owner decisions/approvals and says no further approval asks. This records the
bounded choice before production source edits. Root owns tracker publication,
canonical integration and actual execution gates; no publication is fabricated.

## 1. Goal & non-goals

Implement the one Guard-owned named response-CSP catalog and verified result,
plus cold-staging enforcement of a qualified copied Host and the existing three
package provider receipts. Keep COMPAT013 unchanged. No accepting JS, native
activation, original-source rewrite, additional transport/ID/grant, OS-containment
claim, broad/inline policy fallback, or general package release is authorized here.

## 2. Spec refs

Architecture 03's windows.<key>.web.csp named selector and default deny; KEL102's
single retained verified parser/snapshot; GH473 S1's source/response ownership and
physical controls; GH499's existing graph/provider admission; GH565's deliberate
typed defaultSession refusal. IMPLEMENTATION-ATOMS.md separates syntax, provenance,
pair admission, native effects and evidence before implementation.

## 3. Acceptance criteria

1. The existing unique-key 64 KiB JSONC parser retains typed window policy.
   Null/malformed/unknown window-web metadata, including web.hid and unimplemented
   camera/microphone keys, refuses as KELD-GUARD005; no new media permission exists.
2. The sole selector is original-editor-offline. Unknown selectors including the
   destination-only default/static-only examples refuse. Unrelated root/window
   fields keep existing behavior, while absent windows remains valid all-deny.
3. Only VerifiedManifest creates ResolvedWindowCsp. It binds exact verified digest,
   actual manifest key and one closed catalog entry. Missing/ambiguous key refuses;
   there is no raw header constructor or caller setter. Direct parsing is data.
4. The catalog header is exactly H2 below, with no unsafe-inline/unsafe-eval, remote
   source, app.net projection or fallback. App.net changes cannot alter H2.
5. Selected window CSP at cold staging requires explicit Legacy facade, approved
   graph/provider inputs and an independent expected Host digest. Ordinary staging
   and graph staging without that digest refuse before publication/child/window.
6. The digest returned by the existing executable-copy operation must match the
   expected qualified Host digest; three existing provider checks remain mandatory.
   Old Host or changed package fails. Existing StageGuard removes a failed launch.
7. Empty launches without selected policy stay valid and confer no eligibility.
   Core/native consumers still refuse actual missing/stale policy loads before
   bytes. COMPAT013 stays until approved activation plus real physical controls.

## 4. Design

Reuse one parse_manifest_at/from_value path, verified_manifest loader and catalog
module; no second JSON parser. Retain typed selector data in PermissionsManifest.
Opaque policy fields are private and have no Deserialize or raw constructor.
VerifiedManifest resolves an explicit key or the sole key matching a closed
profile. The profile catalog owns this one exact H2 header:

```text
default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'
```

H2 reuses H1's admitted-local/data-image/font/wasm resource scope and removes its
inline-style allowance because the human explicitly forbids unsafe-inline.
Actual unchanged-original style/bootstrap/wasm success is UNKNOWN; any failure
holds production and is reported, not repaired by relaxing policy or source edits.
This is source-contract approval, not a physical-effect or app-success receipt.

Extend stage_dev_boot_with_approved_graph with a fourth Option<[u8;32]> independent
qualified Host digest. None keeps existing non-policy graph callers usable but
cannot stage selected window CSP. Ordinary staging always passes None internally.
Reuse stage_developer_executable's returned copied-byte digest and exactly three
ApprovedGraphCaptureInput package-tree fingerprints. No graph/schema/wire field,
environment boolean, fourth provider or new staging helper is added. Compare
before graph capture/descriptor publication; existing StageGuard owns rollback.

Core owns trusted window mapping: every relevant WindowRef/incarnation/native view
consumes the one resolved key/source association, not just the first window.
Native owner consumes ResolvedWindowCsp instead of its candidate public raw header.
Those separately owned files are not edited by this worker. No handle/crash/
principal ownership changes occur in this bounded source slice.

## 5. Boundaries

Worker edits scratch copies of guard lib/verified_manifest/new policy owner and
tests, CLI boot and nearby pairing tests. Root applies the patch. Core registry/
router and native source signatures go through root to their current owners.
No Cargo, original app, production JS, shared wire allocation or instruction edits.

## 6. Tasks

- [x] Record this scoped delegated contract and independent atoms.
- [ ] Supply Guard/catalog and staging source with direct parser/verified-loader/
  real-filesystem staging tests and named one-fault mutations.
- [ ] Root integrates disjoint consumers and runs actual format, warning-denied
  clippy, workspace/tests and native qualification before any accepting JS.

## 7. Test plan

Parser regressions use the current existing API so accepting ignored policy is a
semantic RED, not a missing-symbol compile error. Verified-policy tests exercise
retained file/digest, missing/ambiguous keys, immutable header independent of app.net
and privacy compile-fail docs. Staging tests use isolated actual source/copy files,
independent fixture Host digest, old-host mutation, each changed provider and actual
failed-stage cleanup. All are UNRUN by worker: root is sole builder. Physical CSP,
wasm/confinement, stale view and original application remain separate unrun gates.

## 8. Review gates triggered

Public API and permission model. No unsafe, dependency addition or wire protocol.
Independent final-diff gates remain root-owned; this worker does no review.

## 9. Perf impact

No performance claim. Catalog resolution, parsing and Host/package checks are cold
preflight; no new kipc/event-loop allocation, async runtime, queue or native call.

## 10. Open questions

None for this bounded source contract. Physical native efficacy, actual original
styling/bootstrap/wasm, final qualified binary/package digests, consumers' per-view
mapping and full-distribution launch coverage are unresolved execution/integration
gates. They hold production; they do not request design approval again. Ordinary
unqualified selected-policy staging explicitly refuses in this source slice.
