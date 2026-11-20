# GH541 references and approvals

Part of: [index](index.md)
Owns: §2 and historical decisions

## 2. Spec refs: approval provenance

| Reference | Verified relationship |
|---|---|
| [KEL-102 pinned approved spec](https://github.com/gyldlab/keld/blob/442f3ebcef777f204cec7073932dc06edfab3a2e/docs/specs/kel102-host-guard-enforcement.md) | Immutable verified policy, trusted principal selection, shared dispatch, and separate OS containment. Original approval `b330c2ec-2193-4aa6-b324-67187cdda4d5`; its digest is not approval of this amendment. |
| [KEL-142 pinned approved spec](https://github.com/gyldlab/keld/blob/442f3ebcef777f204cec7073932dc06edfab3a2e/docs/specs/kel142-macos-renderer-bridge-api.md) | Existing main-frame isolated-world bridge and bounded Echo invoke. Original approval `53510bc6-66e0-441c-8992-010b6a8719e6`, approved content `2e8da1b7ba19783eceb671093620419664537415`. |
| [#398 decision](https://github.com/gyldlab/keld/issues/398#issuecomment-6022113039) | Adopted KEL-102 amendment, exact `el:<channel>` literals, existing GUARD003, and separate exact-content approval. |
| [#400 resolution](https://github.com/gyldlab/keld/issues/400#issuecomment-6022099642) and [packet](https://github.com/gyldlab/keld/issues/400#issuecomment-6022069479) | Reuse the existing per-WebView mint; renderer commit rotation, earlier host-load invalidation, same-document preservation, checked u32. |
| [#541 brief](https://github.com/gyldlab/keld/issues/541#issuecomment-6026146702) | Owns this contract and the architecture 03 example correction. |
| [#552 brief](https://github.com/gyldlab/keld/issues/552#issuecomment-6026147923) | Owns generic channel evaluation, before-Bun enforcement, zero-allocation allow, and explain parity. |
| [#542 brief](https://github.com/gyldlab/keld/issues/542#issuecomment-6026149009) | Owns the unified navigation mint and real macOS tests. |
| [Architecture 03](https://github.com/gyldlab/keld/blob/442f3ebcef777f204cec7073932dc06edfab3a2e/docs/architecture/03-security.md) | Authority model, immutable grants, default-deny remote windows, and native capability checks. |
| [#1096 exact r2 approval](https://github.com/gyldlab/keld/pull/1096#issuecomment-6364741222) | Approved error/document-result amendment at `60280baa8cbce4b58755679a42b54bd57df66299`; its documentation merged through `f19c8847787aaf0e9a14e07c0af008fc5a4c2a1f`. Implementation #1105 remains unmerged. Preserve its owner; landed documentation does not establish implemented behavior. |

KEL-102/T3 is already landed at `66ccbbc68c3dd44be55878bb0c14e9788e25a4b1` through #357, terminal comment `cde25f5e-db85-4d22-9349-553469fecea0`. Its unchecked task in the source spec is documentation drift. KEL-142 is Done and landed through #351. Neither completion authorizes the new channel schema.

### 2.1 Explicit amendments to earlier wording

1. Remove only the exact Electron channel-grant slice from KEL-102's window/channel non-goals after this content is approved. General window/web, role, and native capability grants remain outside the slice. Keep the old approval JSON and digest unchanged as historical provenance.
2. Qualify KEL-102 D5 and the core routing paragraph: the new registered **renderer-to-Bun forwarding adapter in keld-core** may call the
   existing `dispatch_privileged` around forwarding. Core still does not call `evaluate`, match grants, or mint permits. Native brokers and
   media retain their existing designated ownership. This narrow new caller of the shared dispatcher is an explicit proposed ownership
   change, not an inference from the old approval.
3. Qualify KEL-142 §5.2 item 6, §6, and KEL-142 AC-5: renderer provisional navigation alone does not revoke the current document; cross-document commit does. Host-initiated full-document load/reload revokes earlier. Same-document navigation never rotates. A failed host load does not restore old authority.
4. Preserve KEL-143's recovery rule: Bun recovery does not rotate navigation identity or replace the surviving document nonce. App-link/Bun generation is a separate lifetime axis.
5. Preserve the approved but unmerged #1096 error owner. Its `WV-012`, `WV-013`, `API-002`, and document-scoped-result behavior must be integrated under that approval; this amendment allocates no competing numeric errors.

### KEL-102 companion proposal

FACT: The KEL-102 companion at the P01 decision tree records the following conditional proposal. The P01 decision approves deliberate adoption in the selected Windows Legacy artifact.

> The pending GH541/GH552 amendment preserves the existing five-argument native function.
> It delegates to the typed dispatch body in the same IPC module.
> Upon exact approval, this call chain supersedes the corresponding D5, section 4 and section 7 sole-caller assertions.
> That body calls Guard-owned `evaluate_query` and alone invokes the effect callback.
> The native query delegates its pure decision to `evaluate`.
> This proposal requires its own exact-content decision.
