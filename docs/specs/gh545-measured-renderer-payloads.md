# Spec: measured renderer payload profile for draw.io open and save
Status: draft
Linear: GH-545 (#517) · Owner: Monish Khandelwal · Updated: 2026-11-06

## 1. Goal & non-goals

Choose a finite renderer payload profile from actual pinned draw.io open/save measurements, then prove
a document larger than 4096 bytes survives open, edit and save with independently verified output bytes.
The raw distribution and corpus identity must precede selection of a bound or chunking design.

Non-goals: export images, the parked bulk-reference lane, unlimited payloads, invented percentiles,
concurrency expansion, claiming all draw.io workflows, or changing filesystem authority.
This draft defines a measurement and decision gate. It does not select an unmeasured new limit.

## 2. Spec refs

- [GH545](https://github.com/gyldlab/keld/issues/545) and [preserved larger-file gate](https://github.com/gyldlab/keld/issues/545#issuecomment-6371999345) own measurement and the approved profile.
- [Architecture 01](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/01-overview.md) §5 supplies resource/performance budgets.
- [Architecture 02](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/02-ipc.md) §§2,3,6 governs finite frames, bulk ownership and copies.
- [Native decoder](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wv_link.rs) caps payload at 4096 and envelope at 65536 bytes.
- [Native scripts](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-wv/src/wkwebview/macos_bridge.rs) duplicate the current 4096-byte floor; a decoder-only change is insufficient.
- [GH540 profile](https://github.com/gyldlab/keld/blob/16b00a2e388449c4525a30913937bc2f6e11c472/docs/specs/gh540-compat-ipc.md) §§4.3,4.8 owns all-hop admission and physical release.
- [GH467 boundary](https://github.com/gyldlab/keld/blob/b8b15392cce51fbf848f5a47841250100df7535f/docs/specs/gh467-contextbridge-ipcrenderer.md) owns cross-world copy limits and actual preload behavior.
- [draw.io main](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js) reads files at readFile, writes in saveFile and wraps replies in mainResp.
- [draw.io preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js) mutates reqId, sends rendererReq and dispatches mainResp to its callback.
- [Electron v44.4.5 IPC](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/ipc-renderer.md) documents upstream argument serialization, not Keld's smaller codec.

GH420 owns the boot/authority trace; GH544 owns the sole value codec; GH543 owns addressed delivery;
GH623 owns aggregate transport budgets and GH532/GH566 evidence publication. Existing upstream save verification compares
data after reading with its chosen encoding. This spec additionally requires an independent byte/digest witness.

Current GH540's two controlled small round trips do not satisfy this issue. All predecessor draft approvals remain separate.
A larger profile must amend its owning GH540/GH544/KEL-142 contracts before implementation; this draft changes no architecture or constants.

## 3. Acceptance criteria (binary, each becomes a test)

### AC-1: The distribution is reproducible

Commit a raw measurement artifact with corpus ID, immutable source/fixture digests, runtime versions, instrumentation revision,
command/workflow, sample count and every observed request/response size.
Recompute its reported distribution from the rows. Record an empty/failed measurement as missing evidence, never zero-size success.

### AC-2: Measure complete representations

For each readFile/mainResp and saveFile/rendererReq exchange, record document bytes, endpoint codec bytes,
private UTF-8 envelope bytes, app-link body bytes and retained framework representations at each owner.
Include topic, reqId, success/error wrappers, paths and metadata. UTF-16 character count is not UTF-8 byte count.

### AC-3: Selection follows evidence and all-hop proof

The decision record identifies the measured workload and either a finite inline limit or a separately approved bounded chunk contract.
For inline, every maximum serialized representation fits its owning ceiling and total-retention budget.
For chunking, bounded partial assembly, identity, timeout, ordering, cancellation and no-partial-effect rules must be approved first.
No profile can be selected merely because one larger JSON object happens to fit a frame.

### AC-4: Actual larger-file open/edit/save is byte equal

On real macOS, open a valid pinned fixture whose admitted endpoint payload is above 4096 bytes.
Make a deterministic edit through the real application route and save to a disposable test destination.
Independently read the resulting file as bytes and compare its length and digest with the declared edited-output oracle.
A synthetic callback or successful write return without byte equality fails.

### AC-5: Exact limit and limit-plus-one are distinguishable

An approved-profile maximum payload succeeds with complete bytes. A request one byte above that limit receives the
approved typed refusal naming the limit before excess framework retention or application-handler/filesystem dispatch.
An oversize response is refused by its owning sender without partial delivery/decoding; its handler may already have acted.
Preserve the committed or unknown effect outcome and never infer rollback or safe replay from response refusal.
The link remains usable after a recoverable refusal, with the diagnostic observed at the correct internal/public owner.

### AC-6: Encoding and wrapper expansion cannot truncate data

Valid fixtures include ASCII, non-ASCII Unicode, quote/backslash-heavy strings and newline variants within the selected text workflow.
Each decoded and saved value retains its expected bytes under the declared file encoding.
Lossy transcoding, truncation or counting only source characters must fail independently of the transport's own success flag.

### AC-7: Retirement retains bounded ownership

Navigate or retire the route during a maximal admitted transfer.
Old data cannot reach a successor, and every active representation remains charged until its owner releases or proves quiescence.
If chunking is selected later, missing/duplicate/out-of-order/wrong-document pieces cannot commit a partial application value.

### AC-8: The scoring boundary stays honest

The evidence names exact app, document, platform, profile, grants and workflow steps.
No export, arbitrary binary format, large image, concurrent-invoke or full-product pass follows from this text-only open/save slice.
The larger-file evidence gate remains linked to the product workflow owner.

## 4. Design

Start with measurement against the existing Electron app, independently of new Keld IPC.
Instrument the actual rendererReq/mainResp boundaries without replacing the application's readFile/saveFile handlers.
Use a disposable corpus with checked-in manifest and digests; do not collect private user documents or publish their contents.
A raw row records direction, action, request identity, corpus item, encoding, byte counts, success/failure and artifact digest.

The corpus must include the real first-proof workflow's representative small and large text documents plus adversarial
serialization expansion. Report sample count, min/max and explicit percentile method; do not imply a population guarantee.
Separate original file size, edited output bytes and full IPC value size. A >4096-byte file alone need not prove >4096 codec bytes.

| Representation | Current owner/bound | Required selection evidence |
| --- | --- | --- |
| Endpoint opaque bytes | GH540/GH544: 4096 | Exact encoder output including argument wrapper |
| Private JSON DATA | KEL-142/GH540: 65536 encoded bytes | Actual UTF-8 serialization, escaping and byte-array expansion |
| Compat CALL/EVENT body | GH540: 16384 bytes | Complete canonical control wrapper and payload |
| Canonical compat REPLY/ERR/control operation | GH540: 4096 bytes | Bounded diagnostics and terminal bookkeeping |
| Per-hop DATA retention | GH540: 262144 accounted bytes | Owned copies, backing capacities, metadata and retired work |
| Cross-world values | GH467: separate graph/string/proxy budgets | Full copy/freeze/accounting census; no implicit waiver |
| Aggregate transport | GH623 owning implementation | All producers plus DATA/CONTROL reservations |

The selected inline maximum is constrained by all rows, codec string/visit bounds, scratch overlap and existing frame validation.
Compute actual worst-case encodings for the admitted grammar; do not infer a safe payload bound from an average expansion ratio.
The app-link global frame ceiling does not override its smaller compat body limit.
The table's canonical REPLY/ERR/control ceiling is not a 4096-byte JSON limit for every native result.
GH540 classifies a successful private invoke-result as DATA; apply its own payload and private-envelope limits.
Do not advertise these accounting ceilings as bounds on engine-internal allocations made before framework admission.

Prefer reusing the existing inline format if measurements and independently reviewed worst-case bounds fit the required corpus.
If they do not, the decision must explicitly reject inline or narrow the qualified corpus.
Chunking is an alternative to evaluate, not an authorized implementation hidden inside this spec:
it needs one canonical assembly owner, finite total bytes/parts, authenticated document identity, no interleaving ambiguity,
physical-retention accounting and complete-value dispatch only. It must not add a second socket or unbounded buffer.

The first profile retains existing sequential admission. Increasing payload size does not increase the number of pending requests.
Unknown retained copies or a stalled native callback prevent a bounded-memory qualification.
Transport refusal cannot follow dispatch to saveFile if the request was already known to exceed the profile.
Existing saveFile durability/backup behavior remains owned upstream; this spec does not promise rollback or atomic saves.

Capabilities: existing declared file and el:rendererReq/mainResp permissions only. Measurement needs the separately authorized
test corpus, not wider app grants. No new runtime permission or persisted product schema is selected.
Wire: unresolved until profile choice. A value-cap amendment or chunk contract follows the canonical wire review/version rules.
Migration unit: encoder checks, page/isolated/native limits, core and Worker budgets, tests and the approved profile move together.
A partial limit change must fail consistency checks. Old admitted messages retain existing semantics; oversize remains explicit refusal.

## 5. Boundaries

Future work belongs in the existing draw.io trace/fixture owner, GH544 codec, keld-wv bridge, core delivery and canonical transport.
Coordinate owner-approved profile changes rather than editing parallel copies opportunistically.
Do not touch bulk/shared-memory transport, export handlers, public scoring denominator, filesystem grant policy or concurrency ownership.
This draft PR adds only this specification; no measured corpus or bound is claimed to exist.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

### T-1: Publish the raw size census

Create the pinned disposable text corpus and instrument actual Electron open/save messages.
Publish rows, digests, replay instructions and full representation accounting for AC-1/AC-2/AC-6.

### T-2: Approve a finite profile using the measured record

Evaluate inline versus bounded chunks against every owner ceiling and native copy census.
Publish the selected profile and exact predecessor amendments, or a documented rejection, for AC-3.
Do not start limit implementation while this decision or its wire review is open.

### T-3: Implement profile checks across the complete route

After approval, update the existing owners together and prove exact maximum, maximum-plus-one and retirement behavior.
Cover AC-5/AC-7 with pre-dispatch request refusal, no partial response delivery, and preserved post-refusal link health.
Record effect-before-response-overflow separately; do not add rollback or automatic retry.

### T-4: Qualify the real larger-file workflow

Run native open/edit/save with independent byte/digest comparison and publish source-pinned evidence for AC-4/AC-8.
Retain unqualified formats, platforms and product cells.

## 7. Test plan

| Criteria | Independent witness | Single mutation that must fail |
| --- | --- | --- |
| AC-1 | Recomputed distribution plus corpus/source digests | Drop the largest raw row from the summary |
| AC-2/AC-3 | Actual encoder bytes and owner high-water census | Count only document text and omit wrapper |
| AC-4 | Independent raw file read versus expected edited bytes | Truncate payload while returning success |
| AC-5 request | Valid maximum and plus-one pair, zero rejected-handler effects, then healthy small message | Dispatch oversize request before size refusal |
| AC-5 response | Handler commits a distinct effect before producing oversize result; no partial result reaches receiver | Treat response overflow as definitely-not-dispatched or replay the effect |
| AC-6 | Non-ASCII and escaping fixtures with byte digests | Use UTF-16 length as UTF-8 size |
| AC-7 | Held completion barrier and successor document marker | Release old charge at timeout alone |
| AC-8 | Workflow/corpus/authority denominator audit | Label text-only evidence as all-format pass |

Negative controls must pass identity, grant and valid-file preconditions before exercising size or lifetime.
Use native barriers instead of sleep-based navigation races. No size census, native run or performance measurement was executed here.

## 8. Review gates triggered

Wire protocol, resource accounting, native lifecycle and security review.
Any new chunk framing or protocol version requires the existing wire owner gate; any new dependency requires its own review.
App, codec, bridge, aggregate admission and evidence owners must review the exact selected profile.
Official primary sources above were read 2026-11-06; GitHub-only scope makes Context7 not applicable.

## 9. Perf impact

Larger messages affect copies, conversion scratch, native evaluation duration, queue retention and head-of-line delay.
Report measured per-representation sizes and high-water counts before estimating memory.
Benchmark the selected profile against the current 4096-byte sequential baseline after implementation, without claiming a speed target.

## 10. Open questions

- GH420/GH545 owner: supply the raw pinned corpus/distribution; no new numerical bound can be approved without it.
- GH540/GH544/KEL-142/GH623 owners: approve one consistent finite profile and its all-hop accounting.
- Wire owner: decide whether evidence supports inline or requires a separately specified chunk contract.
- Native operator and product owner: qualify the larger-file route and preserve its workflow scoring dependency.
