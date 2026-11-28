# Spec: Windows W08 Host-to-Guard diagnostic evidence

- Status: draft
- Tracker: GH-1182
- Owner: GYLDLAB (KELD specification owners)
- Updated: 2026-11-28
- Pin: 2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a

## 1. Goal & non-goals

Define a bounded diagnostic record for one Windows Host attempt and its matching Guard-side consumer. The record MUST prove production, independent receipt, and matching-generation consumption. It MUST NOT grant authority or change product lifecycle. The actual source seam, receiver, consumer owner, and existing cold scheduling boundary remain unselected until their owners provide evidence.

Non-goals:

- Selecting or implementing a native endpoint, receiver, transport, or sampling timer.
- Changing Guard admission, permission evaluation, principals, capabilities, signatures, image pins, or OS access.
- Adding a permission, channel, wire frame, public API, dependency, or background service.
- Making product Ready, startup, recovery, shutdown, or retirement depend on diagnostic availability.
- Qualifying B2, N7, P6, A00, W09, or any authentic Windows workflow.
- Treating inherited logs, producer-side success, post-return output, or a model as delivery proof.

## 2. Spec refs

- FACT: KEL-96 assigns the shipping Host ownership of the UI event loop, windows, authenticated app-link, and supervised Bun lifecycle. Its approved lifecycle requires Ready after authenticated HELLO and native readiness. Source: `docs/specs/kel96-no-flag-host-boot.md:10-16,153-164,188-194@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
- FACT: KEL-102 owns one immutable host-session policy snapshot and the sole production Guard evaluation and privileged dispatch boundary. Source: `docs/specs/kel102-host-guard-enforcement.md:84-90,212-218@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
- FACT: Architecture 02 binds lifecycle traffic to an authenticated generation. A replacement receives Ready only after fresh HELLO. The old endpoint is revoked before successor acceptance. Source: `docs/architecture/02-ipc.md:169-176@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
- FACT: Architecture 02 defines the existing lifecycle channel and its I/O deadlines. It does not select a W08 diagnostic receiver. Source: `docs/architecture/02-ipc.md:314-331@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.
- FACT: The prior P1 Host/Guard/independent council reviewed packet SHA-256 `202a39dae69e9c66cc64102e0edd1d9100113295236d7ad107a5e7682592208a`. Its decision selects a prospective logical reporter-to-receiver shape only. See [KEL-102 decision](https://linear.app/gyldlab-keld/issue/KEL-102#comment-da02c3f9-0dd7-4178-a79f-f2e909339a3c).
- FACT: The existing KEL-96 owner request requires a concrete source seam, independent attempt-scoped receiver, eligible cold scheduling boundary, measured cadence, and consumer mapping. See [KEL-96 owner request](https://linear.app/gyldlab-keld/issue/KEL-96#comment-f986e989-9e40-4dd8-87af-0feaa2be5ffd).
- FACT: GH446 requires separate producer, attributable receiver, and actual consumer evidence. It rejects producer-side success as proof of consumption. See [GH446 A04 handoff](https://github.com/gyldlab/keld/issues/446#issuecomment-7181775532).
- UNKNOWN: No source-reviewed, owner-selected W08 receiver or measured cold cadence is recorded in these references. This specification therefore selects no implementation seam.

## 3. Acceptance criteria

### AC-1: The selected producer has an accountable source owner

- **Given:** The KEL-96 Host owner presents a candidate source seam.
- **When:** An independent reviewer binds the exact source, commit, owner, event, and call path.
- **Then:** The record names the Host producer and its existing cold scheduling opportunity. It also records an actual measured cadence and the earliest existing cutoff. Missing evidence remains UNKNOWN and W08 stays HOLD.
- **Negative control:**
  - **Single fault:** Replace the live source event with an inherited or post-return log line.
  - **At-risk precondition:** All other source identity and attempt fields are valid.
  - **Expected RED:** The evidence review rejects the candidate as a producer-only observation.

### AC-2: Receipt and consumption bind to the same attempt and generation

- **Given:** A Host producer emits one bounded observation for an admitted attempt.
- **When:** The independent receiver records receipt and the actual consumer records consumption.
- **Then:** All records bind one Host image, Bun runtime, attempt, run, interface, and role generation. They also bind sequence and observation time. Existing Host admission owns these values. A caller or renderer cannot choose them.
- **Negative control:**
  - **Single fault:** Change only the receiver's attempt or generation binding.
  - **At-risk precondition:** Producer and consumer evidence otherwise matches.
  - **Expected RED:** The correlator refuses to join the records and leaves A04 UNKNOWN.

### AC-2a: Evidence identifies the exact supervised Bun runtime

- **Given:** B2 has selected and identified the supervised Bun executable for this attempt.
- **When:** The Host producer creates an observation and the receiver verifies its attempt tuple.
- **Then:** The tuple carries the exact runtime identity supplied by B2. It includes the canonical selected executable identity, measured executable digest, and pinned runtime version. The existing Host supervisor binds these facts to the spawned child and generation. Child-reported version text alone is not proof.
- **Then:** W08 consumes B2's runtime evidence. W08 does not create a signature, publisher, trust pin, or new authenticity claim. Missing or ambiguous B2 identity makes the attempt UNKNOWN and blocks adoption.
- **Negative control:**
  - **Single fault:** Swap only the selected Bun executable or its measured digest between otherwise matching attempts.
  - **At-risk precondition:** Host, attempt, run, interface, and generation identities match.
  - **Expected RED:** The verifier rejects the runtime mismatch. It cannot accept child-reported version text as a substitute.

### AC-3: Receiver and consumer evidence are independent

- **Given:** The source producer and candidate receiver are identified.
- **When:** A receiver accepts a record and the owning consumer processes the same generation.
- **Then:** The evidence shows separate producer, receiver, and consumer observations. The receiver has its own attributable receipt. The consumer records the actual matching-generation outcome. A producer acknowledgement alone does not pass this criterion.
- **Negative control:**
  - **Single fault:** Remove the independent consumer observation while preserving producer and receiver records.
  - **At-risk precondition:** Both earlier stages have valid identities and timestamps.
  - **Expected RED:** A04 remains UNKNOWN and the report cannot claim consumption.

### AC-4: Observation uses an existing measured cold schedule and fixed budgets

- **Given:** The owners identify an eligible existing cold scheduling opportunity and measure its cadence on the supported Windows system.
- **When:** The reporter observes one admitted attempt.
- **Then:** Observation starts at validated attempt admission. It uses one qualified monotonic clock domain and measured cadence.
  It adds no timer or poll loop. It adds no catch-up, retry, or product deadline.
  The attempt owner binds the original cutoff to that clock domain and epoch.
  The reporter and receiver compare timestamps only within that domain.
  A different receiver clock requires an owner-approved, bounded, measured mapping.
  Missing, ambiguous, expired, or out-of-error-bound mapping makes evidence UNKNOWN.
  It stops at the earliest original cutoff, attempt termination, revocation, refusal, or budget exhaustion.
  At most 66 opportunities are allowed, including the initial sample.
  The limits remain 64 slots, 66 frames, and 887 bytes. Every observation and terminal record counts toward its applicable ceiling.
  Overflow or truncation marks evidence incomplete. It cannot extend the attempt.
- **Negative control:**
  - **Single fault:** Change the receiver timestamp epoch or allow one observation after the original cutoff.
  - **At-risk precondition:** Source and generation match, and the record is otherwise valid.
  - **Expected RED:** The verifier refuses the incomparable or late record. Evidence becomes UNKNOWN. The product cutoff remains unchanged.

### AC-5: Mismatch, duplication, lateness, and reordering never produce a pass

- **Given:** A bounded set of producer, receiver, and consumer records is available.
- **When:** An independent verifier correlates them.
- **Then:** The verifier rejects wrong run, attempt, Host image, Bun runtime, interface, or generation. It also rejects post-cutoff, duplicate, late, missing, or out-of-order evidence. It never repairs identity from a PID, display name, caller label, or log text.
- **Negative control:**
  - **Single fault:** Duplicate one otherwise valid sequence number.
  - **At-risk precondition:** A valid producer, receiver, and consumer chain exists.
  - **Expected RED:** The verifier rejects the duplicate and does not issue a passing receipt.

### AC-6: Diagnostics cannot change authority or product lifecycle

- **Given:** The normal Host, Guard, and SDK lifecycle runs with diagnostic delivery unavailable.
- **When:** The receiver refuses, expires, overflows, or loses its consumer.
- **Then:** Existing authorization, Guard decisions, Ready publication, startup, recovery, shutdown, and retirement follow their approved contracts. Diagnostic failure yields UNKNOWN evidence only. No role gains authority and no product path waits for diagnostic success.
- **Negative control:**
  - **Single fault:** Make diagnostic receiver delivery unavailable while keeping the normal lifecycle inputs unchanged.
  - **At-risk precondition:** The product session is otherwise eligible to reach its existing lifecycle outcome.
  - **Expected RED:** Any changed permission result, lifecycle transition, deadline, or process retirement fails this criterion.

## 4. Design

### D-1: Keep the source seam unselected until the Host owner proves it

**UNKNOWN:** The existing Host source seam is not selected by this specification. The KEL-96 owner MUST provide its exact path, lines, commit, owning function, and call chain before adoption.

Any accepted reporter MUST run in the native Windows `keld-host` process. This names the required reporter owner, not its source seam or receiver.

Candidate for owner review:

1. Existing Host-owned native readiness events and their Core publication path. The owner must show where a sample can be attributed without changing Ready behavior.

The existing app-link is NOT a W08 receiver candidate. Its approved contract carries endpoint plus possession secret. Its Host router serves echo and lifecycle traffic. KEL-72 lifecycle exposes Ready, LastWindowClosed, and Quit. No approved contract defines a W08 diagnostic payload or independent W08 consumer. MUST NOT repurpose lifecycle frames, add diagnostic payloads to lifecycle events, or infer W08 delivery from echo. Source: `docs/architecture/02-ipc.md:43-49,72-87,169-178,314-331@2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`.

If native readiness cannot provide independent receipt and consumption, record NO-REUSE. A new receiver then requires a separate approved transport and wire contract. This draft does not select or authorize that contract.

Reject inherited stdout, historical logs, producer-side return values, and post-return output as the independent receiver. They cannot prove timely receipt and consumption by the matching generation.

If no existing seam meets AC-1 through AC-3, the KEL-96 owner MUST record NO-REUSE. The feature owner must then seek exact-content approval for a separate implementation specification.

### D-2: Separate the three evidence owners

The Host owner controls the producer and its immutable attempt context. The producer reports only facts already available at the selected source seam.

The independent receiver owns receipt time, sequence validation, bounded storage, and its own identity. It MUST NOT mint Host identity or Guard authority.

The actual consumer owner records whether the matching generation consumed the observation. The consumer evidence MUST identify the receiver record and its own generation context.

An independent verifier correlates the three records. The verifier MUST preserve unknown and missing fields. It MUST NOT infer consumption from delivery, or delivery from production.

### D-3: Bind every record to one immutable attempt tuple

The future record schema MUST bind these values:

| Field | Owner | Required meaning |
|---|---|---|
| Host image digest | Host admission owner | Exact measured Host bytes for the attempt |
| Source commit and source path | Host build owner | Source identity that produced those bytes |
| Attempt and run identifiers | Existing Host attempt owner | Fresh identity for this diagnostic attempt |
| Bun runtime identity | B2 runtime owner and Host supervisor | Exact selected canonical executable identity, measured executable digest, and pinned version for this attempt |
| Interface identity | KEL-96 owner | Exact approved source/receiver contract identity |
| Role generation | Existing authenticated generation owner | Host-authenticated generation; never caller-selected |
| Sequence | Producer and receiver | Strictly increasing within the one attempt |
| Clock domain and epoch | Existing Host attempt/clock owner | Qualified monotonic source and epoch for observation and cutoff |
| Observation time | Existing Host monotonic clock owner | Timestamp from the selected existing schedule in the bound domain |
| Receive time | Independent receiver | Receipt time in that domain or through a bounded measured mapping |
| Consume time and result | Actual consumer owner | Consumption time and outcome in that domain or through a bounded measured mapping |

The W08 qualification run and any later A00 run MUST have different run identities. The same W08 result cannot be replayed as A00 evidence.

B2 owns the selected runtime evidence and its admission semantics. W08 MUST consume that exact identity without asserting a signature or publisher proof that B2 does not provide. A digest identifies measured bytes. It is not, by itself, proof of publisher or authenticity. A self-reported Bun version cannot override the selected executable identity.

### D-3a: Use one comparable monotonic clock and owner-defined cutoff

The KEL-96 attempt owner MUST name the monotonic clock source, domain, epoch, start, and original cutoff. The same qualified clock MUST timestamp producer observation, receiver receipt, and consumer receipt where the platform permits it.

If any stage uses a different monotonic domain, its owner MUST provide a bounded, measured mapping to the selected attempt domain. The record MUST bind mapping identity, calibration evidence, validity interval, and maximum error. The verifier MUST include that error when deciding whether receipt preceded cutoff.

No wall-clock conversion, guessed offset, unmeasured calibration, or fresh timer can make incomparable timestamps valid. A missing, stale, ambiguous, or out-of-bound mapping makes the evidence UNKNOWN and blocks adoption. A timestamp that may fall after cutoff under the mapping error is late and MUST be refused. The original product cutoff remains authoritative.

### D-4: Preserve combined bounded observation and existing scheduling

The accepted P1 ceilings are 64 retained slots, 66 emitted frames, 887 serialized bytes, and at most 66 observation opportunities. They apply together to one attempt. They are not independent allowances that a producer may spend separately.

Before approval, the source owner MUST define one combined worst-case ledger. It MUST enumerate every record kind, observation, receiver receipt, consumer receipt, and terminal frame. It MUST state which retained objects consume slots, which serialized outputs consume frames, and how every byte is counted. The 887-byte ceiling applies to the aggregate serialized evidence for one attempt, including all headers and terminal records. No uncounted side buffer or second terminal path is permitted.

Each observation admission MUST reserve all required slot, frame, byte, and opportunity capacity atomically. A record is refused when any one aggregate ceiling would be exceeded. A partial reservation MUST be rolled back without an observation. Terminal output consumes the same frame and byte budget. If terminal output cannot fit, evidence remains incomplete. It MUST NOT exceed a ceiling to report exhaustion.

The owner MUST provide a worst-case composition that accounts for all 66 opportunities, the retained-slot ceiling, all emitted frames, and the complete 887-byte encoding. The composition MUST demonstrate the maximum combination and remaining headroom, if any. The current P1 summary does not provide this accounting. The exact per-kind encoding and slot ownership remain UNKNOWN until the source owner records them. No new cap or additional allowance is inferred here.

The owner MUST measure the existing cold scheduling cadence on the exact supported Windows build. The measurement must bind the source commit, Host image, schedule, sample count, and raw observations. A synthetic clock or a model cannot qualify the Windows cadence.

Sampling MUST use the selected existing cold scheduling opportunity. It MUST NOT add a timer, poll loop, catch-up, retry, or wait on the product path. It MUST stop on the earliest original cutoff, termination, revocation, refusal, or aggregate budget exhaustion.

Required combined-boundary tests MUST exhaust each resource alone and in combination. They MUST cover a terminal frame at the frame ceiling, aggregate bytes at 887 and 888, the final retained slot combined with the final opportunity, and one additional record that would cross two or more ceilings. They MUST prove atomic refusal and no hidden reservation. They MUST prove terminal output never exceeds the aggregate frame or byte ceiling. A missing worst-case composition or any unclear accounting keeps this draft unapproved.

### D-5: Keep Guard authorization and transport unchanged

KEL-102 remains the sole owner of policy loading, evaluation, and privileged dispatch. This diagnostic record does not enter `evaluate`, `dispatch_privileged`, or a native handler as authority.

This specification allocates no channel, frame type, message, permission, capability, public API, or authentication method. It does not change the identity or signing rules for Host, CLI, provider, or Bun. The existing app-link lifecycle frames MUST NOT carry W08 evidence.

If the selected receiver needs a new transport or privilege, work MUST stop. The responsible owners must approve a separate wire and permission specification first. The eventual implementation must retain every existing default-deny and lifecycle check.

### D-6: Preserve normal product behavior

Diagnostics are best-effort evidence. The Host and Guard MUST NOT wait for a receiver or consumer before publishing normal readiness. A diagnostic timeout MUST NOT extend a product deadline.

Receiver refusal, overflow, missing evidence, or consumer loss MUST NOT alter startup, permissions, role lifecycle, recovery, shutdown, or retirement. The evidence result becomes UNKNOWN or REFUSED with its reason.

## 5. Boundaries

- Implement in: none in this specification PR. Later implementation requires a separately approved owner-scoped PR.
- Must not touch: `crates/keld-host`, `crates/keld-core`, `crates/keld-guard`, `crates/keld-ipc`, `packages/@keld/api`, approved KEL-96/KEL-102 specifications, existing owner branches, permissions, channel tables, or trust pins.
- This PR changes one new draft specification only. It does not select a source seam or create execution authority.

## 6. Tasks

### T-1: Identify or reject an existing Host producer seam

- **Blocked by:** none
- KEL-96 Host owner supplies the exact source identity, owner, event path, attempt binding, and existing cold scheduling opportunity. The owner records measured cadence or an explicit NO-REUSE decision.

### T-2: Identify the independent receiver and actual consumer

- **Blocked by:** T-1
- KEL-96 and GH446 owners name the receiver owner, receiver identity, consumer owner, matching-generation evidence, and cleanup owner. If no eligible receiver exists, record that fact and stop implementation planning.

### T-3: Record independent Guard disposition

- **Blocked by:** T-1, T-2
- The KEL-102 owner appoints an independent Guard reviewer. That reviewer confirms the diagnostic cannot alter Guard policy, principal, dispatch, or lifecycle behavior.

### T-4: Approve exact content before implementation

- **Blocked by:** T-1, T-2, T-3
- Owners reconcile all open questions and budget accounting. Independent architecture and Guard review the exact spec head. The authorized human specification owner records exact-content approval. Implementation starts only after Status becomes approved.

### T-5: Qualify a separately approved implementation

- **Blocked by:** T-4
- The implementation owner runs the nonlaunching controls first. The owner then obtains separate native qualification on the approved Windows system. No product workflow or A00 credit comes from this specification alone.

## 7. Test plan & evidence

No implementation or test surface is selected by this draft. No native test ran for this specification.

The approved implementation MUST map every AC to an independent test. The nonlaunching surface must exercise the actual selected receiver and consumer. A harness fence and independent witness must prove zero real spawns.

Required controls include:

- Matched producer, receiver, and consumer records for one attempt and generation.
- Wrong run, attempt, Host image, selected Bun executable, runtime digest, runtime version, interface, and generation, each changed alone.
- Different clock domains, stale mapping, mapping error that overlaps cutoff, missing cutoff owner, and evidence after cutoff, termination, revocation, or refusal.
- Missing receiver, missing consumer, delayed receiver, duplicate sequence, and out-of-order sequence.
- Zero, 64, 65 retained slots; 66 and 67 aggregate frames; 887 and 888 aggregate bytes; 66 and 67 opportunities.
- Combined exhaustion of slots, frames, bytes, and opportunities, including terminal frames and worst-case composition.
- Receiver loss and diagnostic overflow while the normal product lifecycle continues unchanged.
- A second run using the same source but a new attempt and generation; no cross-run correlation is accepted.

### E-1: Existing owner evidence and source selection

- **Run:** Existing KEL-96/KEL-102/GH446 owner records and source review; no W08 receiver is selected by this draft.
- **n:** One published P1 council decision and the current pinned owner specifications.
- **Environment:** Current `main` source at `2c17ea7d8479ce1fef7c2ff51ecf82a214966f8a`; Windows native evidence remains unrun.
- **Limits:** Does not prove an actual receiver, measured cadence, native delivery, consumer receipt, or implementation behavior.

### E-2: Future native cadence and delivery qualification

- **Run:** Exact owner-approved Windows nonlaunch evidence path, followed by separately authorized native qualification.
- **n:** At most 66 observation opportunities per attempt, including the first sample.
- **Environment:** Exact supported Windows device, source commit, Host image, receiver, and consumer identities recorded by the owners.
- **Limits:** No result is valid until owners select the source, receiver, consumer, and existing cold schedule. No native result is claimed here.

## 8. Review gates triggered

- unsafe: none proposed
- public API: none proposed
- permission model: no change proposed; independent Guard review required
- dependency: none proposed
- wire protocol: no change proposed; any future wire allocation requires a separate approved spec
- architecture: independent Host and Guard review required because ownership and evidence boundaries are under review

## 9. Perf impact

No product performance change is proposed. The diagnostic keeps 66 opportunities, 64 retained slots, 66 frames, and the 887-byte limit. The owners must define its byte-accounting scope and measure cold cadence and overhead before approval.

## 10. Open questions, blockers & unknowns

### Q-1: Which existing Host-owned cold scheduling opportunity can carry the observation?

Options are the existing native readiness path, the authenticated app-link lifecycle path, or NO-REUSE. Recommendation: NO-REUSE until the KEL-96 owner proves source ownership, independent receipt, matching-generation consumption, and measured cadence. No option is selected by this draft.

### B-1: Host source and cold scheduling boundary are unselected

- **Status:** open
- **Owner:** KEL-96 Host/Runtime owner
- **Impact:** No producer can be adopted or sampled safely. The W08 evidence path remains HOLD.
- **Check:** Record the exact source path, lines, commit, event owner, existing cold scheduling opportunity, raw cadence measurements, and explicit REUSE or NO-REUSE decision.

### B-2: Independent receiver and consumer are unselected

- **Status:** open
- **Owner:** KEL-96 Host/Runtime owner and GH446/GH449/GH540 consumer owners
- **Impact:** Producer output cannot prove delivery or matching-generation consumption. A04 remains UNKNOWN.
- **Check:** Name the receiver and consumer owners. Supply exact source identities, attempt binding, receive evidence, consume evidence, and cleanup responsibility.

The existing authenticated app-link and KEL-72 lifecycle path do not satisfy this blocker. They have no approved W08 receiver payload or independent W08 consumer. Reuse is blocked. The owner must identify a genuinely existing receiver with its own contract, or seek a separately approved transport and wire specification.

### B-3: Independent Guard disposition is outstanding

- **Status:** open
- **Owner:** KEL-102 owner and authorized independent Guard reviewer
- **Impact:** The proposal has no approval to reuse an existing interface or introduce a future diagnostic transport. Guard authority remains unchanged.
- **Check:** Record a review of the exact draft head and confirm that diagnostics cannot influence policy, principal, dispatch, or lifecycle behavior.

### B-4: Budget accounting and cadence measurements are incomplete

- **Status:** open
- **Owner:** KEL-96 Host owner and future receiver owner
- **Impact:** The numeric limits cannot be implemented or verified without exact accounting and measured schedule evidence.
- **Check:** Define slot, frame, and byte accounting from the reviewed P1 packet. Measure the existing cold cadence on the exact supported Windows build.

### B-5: Runtime identity and clock-domain proof are incomplete

- **Status:** open
- **Owner:** B2 runtime owner and KEL-96 Host attempt/clock owner
- **Impact:** A Bun swap or incomparable timestamps could be accepted as valid evidence. Runtime identity and cutoff ordering remain UNKNOWN.
- **Check:** Bind the exact selected Bun executable identity, measured digest, and pinned version from B2. Name the monotonic source, domain, epoch, and original cutoff, or provide a bounded measured mapping.

### B-6: Combined resource ledger is incomplete

- **Status:** open
- **Owner:** KEL-96 Host source owner and future receiver owner
- **Impact:** Individual ceilings do not prove the combined worst case. A terminal frame or aggregate record could exceed the total budget.
- **Check:** Publish the complete per-kind slot, frame, opportunity, and aggregate-byte ledger. Prove atomic admission and refusal at every combined boundary, including terminal output.

### U-1: Actual source, receiver, consumer, and schedule

No owner has selected these elements in the cited records. The exact source identity, receiver identity, consumer mapping, cleanup mechanism, measured cadence, runtime identity input, clock mapping, and combined budget ledger remain UNKNOWN. The owners must provide these facts before this draft can become approved.
