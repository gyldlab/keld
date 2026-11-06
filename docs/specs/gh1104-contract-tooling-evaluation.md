# Spec: Evidence-based contract tooling and drift enforcement
Status: draft
Linear: GH-1104 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Choose the least costly correct enforcement method for each cross-consumer contract family, using
measured evidence and drift mutations. The decision must make one authored definition discoverable
without adding a central runtime registry or changing any wire or public contract rule.

Non-goals: preselecting code generation or a data format, building @keld/schema/keld gen, adding
runtime dependencies, redesigning contract semantics, rewriting the transport, or treating a
successful generator as proof of semantic compatibility. GH1101–GH1103 retain those rule owners.

## 2. Spec refs

- [Owning issue GH1104](https://github.com/gyldlab/keld/issues/1104).
- [Family/copy census owner GH1100](https://github.com/gyldlab/keld/issues/1100).
- [Error governance GH1101](https://github.com/gyldlab/keld/issues/1101).
- [Message/outcome governance GH1102](https://github.com/gyldlab/keld/issues/1102).
- [Lifecycle governance GH1103](https://github.com/gyldlab/keld/issues/1103).
- [Current Rust error-registry checks](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-cli/tests/error_registry.rs).
- [Current channel/capability literal controls](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-cli/tests/channel_table_scan.rs).
- [Canonical channel definitions](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/channel_table.rs).
- [Existing cold generator](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/scripts/echo-codegen.ts).
- [Generator mutation/roundtrip tests](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/scripts/echo-codegen.test.ts).
- [TypeScript literal/ownership checks](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/scripts/channel-table.test.ts).
- [Generator CLI/scripts](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/package.json).
- [Canonical transport, generated regions and stamp](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/kipc/src/transport.ts).
- [Consumer catalog](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/onboarding/04-wire-formats-and-contracts.md).
- [Architecture 02 current generator versus destination schema model](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/02-ipc.md).
- [Generated-document source allowlist](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/tools/llms_docs.rs).

Verified baseline: the existing generator reads constrained Rust source for Echo, channel metadata,
filesystem and authentication declarations, writes declared TypeScript outputs/regions and verifies
freshness. Its CLI takes the positional command generate or check, not a --check option. It also
maintains the transport stamp. A changed generated file and an edited stamp alone are not a new
authored contract. Architecture 02 says the .k.ts/@keld/schema/keld gen destination is not live.

The current registry check is lexical plus structural; it does not prove one semantic meaning.
Channel source scanners have deliberately stronger ownership/mutation checks. Their different
proof strength must be measured per family rather than described collectively as "CI catches drift."

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given the approved GH1100 census, when the decision matrix is produced, then every family/copy has
one authored owner, real consumers and a generator, named parity check or justified hand-written
disposition. New families discovered during evaluation return to that census.

Independent negative control: omit a template copy or a hand-written lifecycle/window consumer,
or claim the error registry owns all code semantics merely because it lists identifiers.

### AC2

Given each family, when alternatives are compared, then at least options a–d and one data-file
option are evaluated against the same semantic proof obligations, costs and migration inputs.
The destination schema model is explicitly considered without being presumed live or preferred.

Independent negative control: compare a generator that checks only values with a parity method
required to prove effects, or omit the unchanged status quo from the comparison.

### AC3

Given each proposed enforcement check, when one deliberate owner/copy drift is introduced, then
that check fails. A changed source with regenerated output still requires independent contract
vectors when semantics/bytes are meant to stay fixed.

Independent negative control: update producer and consumer together and let a self-generated
expected file pass; independent old-contract literals must detect the changed bytes.

### AC4

Given the selected family migration, when actual old/new producers and consumers run, then wire
bytes, public values/errors and ordering remain equivalent within the approved migration scope.

Independent negative control: regenerate declarations while changing enum order, CallError field
order, error prefix, receive-policy admission or lifecycle terminal semantics.

### AC5

Given a performance or maintenance-cost claim, when the evidence is inspected, then its source
revision, tool/runtime versions, command, raw repetitions, clock/statistic and scope reproduce it.
Absent measurements are unknown and cannot select the winner.

Independent negative control: label an unrun option faster, compare a warm run to a cold run,
include queue waiting in one arm only, or publish a median without the raw measurements.

### AC6

Given the candidate's runtime footprint, when compiled paths and dependencies are reviewed, then
kipc, guard and event-loop paths retain compiled constants/current codecs with no new runtime
registry lookup, async mechanism or steady-state allocation.

Independent negative control: add a runtime-loaded JSON map to simplify tooling and show it
through dependency/call-path review plus an actual hot-path allocation/lookup probe.

### AC7

Given fresh isolated checkouts and the current generator, when generation and checking run, then
the declared outputs are deterministic, confined to the allowlist and fresh. Unrelated source and
unknown regions remain unchanged; malformed input fails explicitly.

Independent negative control: alter one generated region, use a stale generator, emit outside the
declared outputs, silently drop an unsupported Rust construct, or accept a wrong transport stamp.

### AC8

Given a selected migration, when follow-up work is created, then each issue has one coherent
vertical family slice, its approved contract, copied-consumer inventory, rollback and first
independent acceptance check. No implementation starts merely because this evaluation was published.

Independent negative control: create a broad "generate everything" implementation without owner
decisions, or treat an approval of one family's tooling as approval of another family's semantics.

### AC9

Given a proposed dependency or schema-authoring move, when the decision is reviewed, then its
named unmet requirement, measured costs, rejected reuse alternatives and gate evidence are explicit.
The original correct path remains usable until equivalent replacement proof exists.

Independent negative control: justify a rewrite only by language, familiarity or fewer authored
lines, or remove a compatibility path before its actual consumers pass.

## 4. Design

### 4.1 Independent decision units

GH1100 owns census; GH1101 error rules; GH1102 message/outcome rules; GH1103 lifecycle taxonomy.
This issue owns the cold enforcement/tooling decision only. A tool passing its own fixtures does
not prove completeness of the census or correctness of those semantics.

For every family, separate source ownership, generated/copied artifacts, actual consumer behavior,
mutation detection, tooling/runtime costs and migration compatibility. Each has its own witness.
The only coupling is an explicit dependency: changing an authored definition requires checking
every listed consumer. A missing consumer blocks the claim of complete enforcement.

Existing runtime identity, principal minting, process/crash ownership, handle lifetimes and I/O
paths stay unchanged. The evaluator reads source in cold tooling; it cannot introduce runtime
authority or decide a guard rule. No boundary change in this documentation publication.

### 4.2 Alternatives to compare

| Option | Concrete candidate | Required evidence and likely limitation to test |
| --- | --- | --- |
| a | Current domain definitions plus targeted parity tests | Lowest-change baseline; prove every copied value/shape and semantic mutation is covered rather than assuming set equality is sufficient. |
| b | Extend existing registry/channel source scanners | Test source grammar, false positives/negatives, actual emitter discovery, comment-only references and unsupported constructs; do not confuse lexical tokens with meaning. |
| c | Extend existing KEL98 cold Rust-to-TypeScript generator | Reuse output allowlists, constrained parser, freshness checks and transport stamping; measure grammar growth and consumer coverage. |
| d | Domain-owned Rust constant tables with derived TS | Preserve compile-time use and one owner; compare table expressiveness for each family, avoiding a giant cross-domain module. |
| e | Domain data file: evaluate at least one of TOML/JSON/YAML | Measure parser/dependency/build costs, validation, generated Rust/TS equivalence and versioning. State why the chosen prototype format represents this alternative. |
| f | Future .k.ts/@keld/schema/keld gen destination | Record missing implementation/required capabilities and migration work; do not present it as a current available tool or build it in this evaluation. |

An option can be ineligible for a specific family when it cannot express the approved rule or
detect a required mutation. Record the failing predicate and evidence. "Not built" is a legitimate
unknown/ineligible current option, not a fabricated measurement or an excuse to ignore an option.

Do not force every family to use the same mechanism. A generated numeric table may fit constants;
a semantic phase/no-effect rule may need an explicit contract and behavior test. Fewer files or
one shared module are not correctness criteria.

### 4.3 Minimum family matrix

| Family | Current owner/consumer starting point | Proof the chosen enforcement must add or preserve |
| --- | --- | --- |
| Error identifiers/meanings | Registry + domain emitters + GH1034 semantic check | Exact spelling, allocation/reservation state and one meaning; no duplicate semantic scanner. |
| Logical error carriers | Display/CallError/KeldErrorObject/TS/renderer boundaries | Presence/trust/redaction and actual application-visible code; no new wire field. |
| Frame kinds/constants | Rust frame/lib + canonical transport/template | Every constant/discriminant, exact header literals, live/reserved status and unchanged v2 bytes. |
| Channel allocation/classes | Existing Rust table, history and generated TS | Unique append-only ownership and receive admission; channel id never authority. |
| Payloads and codecs | Domain Rust schemas + generated declarations + real TS codecs | Exact bytes and malformed cases; declarations alone do not prove serializers. |
| Lifecycle/terminal/exit taxonomy | GH1103's owners, shared TS lifecycle/state consumers | Direction, cause, terminal ordering and exit vocabulary; no accidental synonym or renumbering. |

Use the approved GH1100 catalog for the complete row set and copy locations. This table is an
evaluation checklist, not another authoritative registry and not a claim the census has finished.

### 4.4 Measurement protocol

Freeze one source revision, consumer set, fixture corpus and toolchain for every candidate.
Keep implementation correctness/refusal tests outside generated expectations. Record source and
artifact hashes and preserve all raw command results with the owning decision.

Measure separately: clean build/check duration, warm check duration, peak tool memory, authored
tooling/dependency delta, generated artifact size and migration change set. Build wait/queue time,
source scanning, generation, formatting, compilation and test execution need separate attribution.
Use the same platform, process environment and concurrency policy for both arms.

Proposed minimum reproducibility plan: five independent clean runs and twenty warm runs per
eligible prototype after its correctness controls pass. Preserve every raw sample and define the
median and percentile convention before comparing. Repeat on the relevant CI environment before
calling a local difference a CI result. No absolute budget or winning threshold is invented here;
the owner may approve one with the measured baseline.

A small source-size delta does not imply lower maintenance cost. Record unsupported syntax,
diagnostic clarity, author workflow, number of generated regions and concrete changes needed for
a new family member. Keep those observed facts separate from a recommendation.

### 4.5 Current reusable command seams

Run from packages/@keld/kipc using the pinned package environment:

- `bun run scripts/echo-codegen.ts check` checks the actual declared outputs and transport stamp.
- `bun run scripts/echo-codegen.ts generate` regenerates them from the current owning source.
- `bun test scripts/echo-codegen.test.ts scripts/channel-table.test.ts src/transport.test.ts`
  exercises the existing generator, ownership and transport contracts.

Existing Rust checks include the error_registry and channel_table_scan integration tests under
keld-cli. Extend the owning tests, with current repository routing, rather than add a parallel
generic scanner. These commands are verified source entry points, not commands executed by this
drafting session. Exact future execution output belongs in the decision evidence.

### 4.6 Deterministic generation and migration

Generation is cold, from the current source. Each output has one owner and explicit boundaries.
Check mode must recompute expected bytes; a timestamp, comment hash or edited manifest does not
prove freshness. Preserve existing format normalization and transport-stamp ownership.

For one migration, inventory callers, decoders, generated declarations, embedded templates,
source scanners and persisted fixtures. Preserve permanent public facades. Move authoring and all
derived artifacts together; any temporary adapter has one owner and a named removal predicate.

Maintain independent old-contract literals and real consumer tests while replacing a hand-written
copy. Do not regenerate the acceptance oracle from the new implementation. Roll back the scoped
tooling/source migration and its derived outputs together if equivalence or drift detection fails;
do not roll back unrelated concurrent work.

### 4.7 Decision record and approval

For each family, record candidates, measured evidence, failed predicates, chosen source/check,
rejected alternatives, dependency/runtime cost, migration issue and rollback. Recommend the
simplest option that passes all owned correctness and mutation predicates at acceptable measured
cost. Pending data leaves the choice pending; the document must not imply a selected tool.

Only the approved decision authorizes a later implementation claim. Approval of tooling does not
approve a new error meaning, frame kind, lifecycle event, exit class or schema. Those owners must
approve their exact contract independently.

## 5. Boundaries

Future evaluation may use isolated prototypes of existing cold tooling and checked fixtures.
No runtime integration, workspace dependency change, central registry, generated-output hand edit,
schema redesign, .k.ts language implementation or competing source owner belongs in this batch.

This PR changes only this new spec and grouped PR/issue metadata. GH1100–GH1103 and active feature
implementers keep their files and decisions. New dependencies or changed generation contracts need
the explicit gate and a coherent follow-up issue.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Obtain approved GH1100–GH1103 inputs; pin full family/copy census and independent semantic/
byte controls, marking every missing input instead of assuming it.

### T2

- [ ] Exercise existing baseline checks and build the minimum isolated a–e prototypes needed
for comparable evidence; record f's exact availability gap without building a speculative stack.

### T3

- [ ] Run individual source/copy/drift mutations and real consumer equivalence before collecting
costs; discard or repair candidates that cannot detect the required fault.

### T4

- [ ] Collect reproducible cold/warm/memory/size/dependency measurements, publish per-family
comparisons and an evidence-based recommendation with rejected alternatives.

### T5

- [ ] Obtain exact decision approval, create coherent family implementation follow-ups with
rollback/first acceptance, and migrate only under those separately verified claims.

## 7. Test plan

AC1–AC3 compare the actual census and deliberate one-at-a-time changes. Include an omitted copy,
comment-only pseudo-emitter, unsupported source construct, stale generated region and a coordinated
producer/consumer mutation. Each check must fail for its assigned fault; another check's pass
cannot substitute for it.

AC4 uses independent complete bytes and actual callers/decoders, with error and ordering controls.
AC5 follows the frozen measurement protocol and raw evidence. AC6 combines source/dependency review
with the actual runtime observable for any proposed hot-path impact. AC7 runs fresh generation
twice, current check mode, allowlist comparisons and transport-stamp negatives. AC8–AC9 verify
follow-up owner/gate/equivalence records before removing an existing path.

This document makes no test/benchmark run claim. GitHub readback and documentation CI validate
publication only. Local just ci, generator execution, mutations, runtime/native work and machine
session-closeout were not run under the user's GitHub-only restriction.

## 8. Review gates triggered

Documentation publication: none. Future tooling migration needs independent public API and wire
equivalence review for affected families. Dependency addition is gated if a candidate requires one;
unsafe and permission model remain none unless a concrete later change proves otherwise.

All family semantics and tooling choices remain draft pending their exact owner approvals.

## 9. Perf impact

No runtime or CI workflow change in this publication. Future tool costs remain unmeasured and
unknown. Architecture 01 section 5 hot-path budgets stay intact; no runtime allocation/lookup or
claimed speedup is accepted merely to simplify generation.

## 10. Open questions (block implementation)

1. Approve the complete GH1100–GH1103 input revisions, family boundaries and missing-copy dispositions.
2. Approve the per-family measured evidence and simplest passing enforcement choice; none is selected here.
3. Approve any explicit cost budget or new dependency after seeing comparable raw measurements.
4. Assign coherent migration issues and exact review/rollback gates before production changes.
