# Spec: macOS synchronous message box: Performance and open acceptance

Part of: [index](index.md)
Owns: Performance constraints and remaining native blockers

## 9. Perf impact

A synchronous user prompt legitimately parks Bun's main JS, but the host
and transport Worker remain responsive. No RTT, speedup or RSS benefit is
claimed. Measure retained ring records/bytes, native panel lifetime,
cleanup tail and Host Quit latency with the real Mac workload. Do not
increase existing deadlines or activate extra GH527 T4 credit machinery
without the owner’s measured evidence. The transport still uses one
bounded queue and record order, no second buffering layer.

## 10. Open questions, blockers & unknowns

The full target remains draft. For §1.1, the selected design reuses the
approved GH478 v2 clocks, Guard, Message/Cancel schema, registered native
errors and source options. These are existing owner decisions, not new
questions for repeat approval. Independent review of this canonical
bounded amendment and the recorded owner receipt are still pending.
The historical questions below remain full-target acceptance work where
not answered by that owning contract; they do not authorize new Native,
protocol or application-recovery work.

1. **Shared dialog interaction and cancellation bounds:** reuse GH478's
   approved **600,000 ms** distinct human UI budget, **10 s live-link
   local-timeout native close** and **5 s Host-observed role/link-loss
   native close**. Validate the existing same-channel
   `Cancel { nonce }`, distinct Cancel and original CALL terminal frames,
   257 sequential abandoned-correlation release control, bounded
   tombstone and late-admission ordering on real macOS with GH478/GH527/Core
   owners. Local Worker `abandon` is
   insufficient; never replace or extend Host20/Guardian/KIPC clocks.
2. **GH478 platform-native completion:** the bounded consumer uses the
   current supplied-parent sheet. Parentless and synchronous hidden-parent
   fallback remain open in their native owner, with measured UI-loop cleanup
   and any required independent unsafe review. No visibility inference or
   unqualified CFRunLoop scheduling claim fills that gap.
3. **Pre-Ready synchronous API behavior:** confirm the proposed *synchronous
   typed not-ready refusal* before the existing singleton is connected,
   rather than allowing a call to invent a second connection or return a
   Promise. The first-proof normal draw.io caller runs after Ready.
4. **Native error-code allocation:** consume GH478's existing
   `KELD-NATIVE-009` through `KELD-NATIVE-016` outcomes. They are registered
   in `docs/engineering/keld-error-codes.md` at the owning PR1306 source
   `16e519091992d551853b6d0434b85000499a3d3c`. The current integrating
   tree contains all eight registry sections; the earlier missing-row
   blocker no longer applies. Registry presence alone does not establish
   code/spec consistency or native behavior. Independent owner review and
   full-target native acceptance remain required. This consumer allocates
   no code and does not edit the separately owned registry.
5. **Effect-unknown application recovery:** the **source-backed first
   proof limitation is explicit**: pinned draw.io's app-owned
   `modifiedModalOpen` can remain true after a thrown dialog failure,
   and repeatable Close is **unsupported** then. Preflight avoids
   deterministic fixture misconfiguration; it cannot repair a later
   runtime error. The normal 0/1/2 path is required. The GH564/compat
   owner must open and approve a separate, source-exact app-recovery
   ticket before claiming repeatable Close after such errors; it may
   not synthesize Cancel, clear app-private state from Host or silently
   count this as a pass.
6. **Pinned message-argument compatibility:** reuse GH478's approved
   admission of `type:'question'`, `title:'Confirm'` and macOS-inert
   `noLink:true` alongside the exact draw.io buttons and indexes.
   Run unchanged caller and invalid-type/unknown-field controls; until
   both specs agree and a native Mac panel reproduces the result,
   keep the GH477 synchronous cell expected-fail.

A bounded receipt under §1.1 does **not** approve the full GH564 target,
complete GH450 native close, qualify Windows/Linux, flip a corpus cell,
resolve formal review or authorize a merge. It permits only the three-file
consumer after independent exact-content approval, the existing approved
dependency contract and winning source/OS assignments. Native acceptance,
generated-doc gates and broader issue completion remain separately required.

### B-1: Native original dialog acceptance remains open

- **Status:** open
- **Owner:** GH-564 implementation and independent native reviewer
- **Impact:** A source-level transport test cannot qualify original dialog behavior in the installed application.
- **Check:** Run the signed installed original app, drive a real native dialog, and record the exact response and failure controls.
