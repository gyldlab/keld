# Spec: Corpus-demanded native-addon policy records
Status: draft
Linear: GH-503 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Make each corpus-demanded native-addon decision traceable to a real operation, an exact runtime/artifact and the existing
KEL-78 policy. Publish truthful unqualified rows where evidence is missing; a successful import is not feature support.

No addon worker, in-primary Strict load, Rust replacement implementation, ecosystem package survey, doctor surface or new
compatibility evidence framework. Implementation remains parked until the issue's pull trigger or a recorded scheduling
exception applies. The present user request authorizes this future document.

## 2. Spec refs

- [Owning issue GH-503](https://github.com/gyldlab/keld/issues/503); current public owner decisions remain authoritative.
- [KEL-78 policy and profile states](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel78-strict-profile-sandbox.md).
- [Evidence schema owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel74-compat-evidence-schema.md).
- [Existing corpus manifest](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-compat/fixtures/app-corpus/corpus.json).
- [Zettlr lock](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/yarn.lock).
- [Zettlr declared dependencies](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/package.json).
- [Actual raw Nodehun import](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/app/service-providers/dictionary/index.ts).
- [Zettlr bundling rules](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/webpack.rules.js).
- [Forge preload and fsevents configuration](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/forge.config.js).
- [Legacy watcher qualification](https://github.com/gyldlab/keld/blob/62653e00c951570565728db1d8527f93e607c935/docs/specs/gh1355-legacy-watcher-qualification.md).

Verified source baseline: At the pinned Zettlr revision, the dictionary provider imports nodehun/build/Release/Nodehun.node
directly; the lock resolves nodehun 3.0.2 and fsevents 2.3.3. package.json declares fsevents as optional. Both chokidar 3.6.0
and 5.0.0 occur in the lock, while the direct dependency is ^5.0.0. A lock entry, Forge's BUNDLE_FSEVENTS flag and relocation
comments do not establish that a particular installed app path loads fsevents. This session did not execute an addon or trace
the bundled import graph.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given the selected corpus commit, frozen lock and actual installed import graph, when demand is recorded, then Nodehun's raw
import and fsevents' declared-only status have distinct provenance and no transitive runtime use is invented.

Independent negative control: Mark fsevents as executed solely because it occurs in the lock; the independent
installed-import witness is absent and must reject the promotion.

### AC2

Given a package/version/OS/architecture/Bun-revision/artifact-digest/operation tuple, when a row is called qualified, then an
immutable receipt proves that exact operation and reports the result and profile; otherwise the row says unqualified —
evidence required.

Independent negative control: Replace an operation-bearing receipt with a require-success receipt or change the artifact
digest while retaining the pass.

### AC3

Given Strict policy evaluation, when one decision is recorded, then it is exactly one of KEL-78's four outcomes and never an
in-primary native load.

Independent negative control: Insert an in-primary Strict outcome; the existing evidence admission/schema owner must reject
it.

### AC4

Given a selected worker or guarded-service outcome whose mechanism is not implemented or qualified, when the policy row is
published, then it remains a selected plan with unsupported/unqualified execution status.

Independent negative control: Treat policy selection as proof of a working worker or permitted native syscalls.

### AC5

Given existing evidence for another package, platform, architecture or Bun revision, when this row is evaluated, then that
receipt is a lead only and cannot satisfy its acceptance.

Independent negative control: Use a Windows better-sqlite3 result to mark macOS Nodehun supported.

### AC6

Given the current corpus scope, when records are enumerated, then nodehun and fsevents each have one demand record with
separate per-platform evidence cells; chokidar is linked to the watcher owner, not labeled as a native-addon package.

Independent negative control: Add chokidar 5 to the addon count or add a second nodehun demand row for the same source
identity.

### AC7

Given a changed corpus/lock/artifact pin, when a prior record is reconsidered, then qualification becomes stale until the
operation is rerun or the evidence owner approves a documented equivalence rule.

Independent negative control: Carry a pass forward after changing the native binary digest.

### AC8

Given Strict versus explicitly selected Legacy, when evidence is displayed, then containment and compatibility are separate
observations and a Legacy success never claims Strict eligibility.

Independent negative control: Copy a Legacy functional pass into the Strict containment column.

## 4. Design

### 4.1 Decision-bearing owners

GH503 owns demand-to-policy records only. KEL-78 owns the four outcomes, profile admission and any addon-worker process;
KEL-74 owns evidence admission; GH590 owns dependency/import discovery; GH1355 owns Legacy watcher behavior. A row contains
data, not authority to load code. The unknown active fsevents path is a demand finding, independent of its ABI or functional
qualification.

### 4.2 One record per source demand

Use a stable demand identity derived from corpus repository/commit plus resolved package entry and call site. Fields are
package and locked version; declared-only or resolved-operation demand; corpus/lock/bundle references; chosen policy outcome;
and per-platform evidence cells. Each cell names OS/architecture, exact Bun revision and executable digest, native artifact
digest, declared profile, operation, expected observable, actual outcome, receipt and limitations. Unknown fields stay
explicit, not empty strings interpreted as a pass.

### 4.3 Reuse and safe qualification

Extend the existing evidence/manifest owner only where its approved schema lacks necessary metadata; do not create another
validator or redefine outcome enums. T1 freezes that extension before records are implemented. A pure import may be a
prerequisite but cannot satisfy dictionary spell/suggest/add/remove behavior. Fixtures use disposable dictionary resources
and inspect actual result values or typed refusals, then retirement; loader success and role cleanup have separate oracles.

### 4.4 Four outcomes without a fifth

Reuse in worker needs KEL-78 T7 plus that worker's capability and containment qualification. Replacement belongs behind a
guarded host service with its own approved feature contract. Upstream fix is parked and cannot ship as Strict. Reject/exclude
remains an honest result. The approved KEL-78 text contains a wider AC6 sentence and a Strict-qualified design; GH503 does
not resolve that owner's Legacy interpretation by editing policy. Record the exact profile and keep the ambiguity as an owner
gate.

### 4.5 Lifecycle and failure

Evidence is immutable and bound to the actual installed/native bytes. Rebuild, runtime change or platform change invalidates
eligibility until the existing owner admits replacement evidence. Never run arbitrary addon code inside the privileged host
just to produce a row. An unavailable machine or operation is unqualified; a recorded failure remains failure. Diagnostic
logs must not leak private paths or corpus user data.

### 4.6 Migration and compatibility

The initial named set is nodehun and fsevents. The existing Node-pty load-without-function observation cited by GH503 is a
historical reason to require operations, not a new executed result. No ecosystem inventory is duplicated from PANEL-P3. No
handle, principal, process or wire boundary changes in this publication.

## 5. Boundaries

Future work belongs in the existing corpus evidence records and their validator/consumer, with only owner-approved schema
metadata extensions. Preserve KEL-78 policy, worker implementation, packaging, runtime sources and the watcher contract. No
new native dependency or platform proof is added here.

This publication changes only `docs/specs/gh503-corpus-addon-policy-record.md` and related grouped-PR/issue handoffs. Shared
architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze the selected corpus/lock/bundle provenance and one demand identity per package; identify the current evidence schema extension owner, then add truthful pending/unqualified rows and malformed-row controls.

### T2

- [ ] Add one operation-bearing Nodehun evidence slice on an actually available declared profile/platform, or retain unqualified with the exact missing artifact/owner; qualify return values separately from load and shutdown.

### T3

- [ ] Resolve fsevents' installed import path with the analyzer/bundler owner, then retain declared-only/reject or run its actual operation under its approved policy; route all watcher conclusions to GH1355.

### T4

- [ ] Validate tuple mismatches, Strict in-primary rejection, stale evidence and profile separation through the existing consumer; publish exact rows without broad ecosystem support claims.

## 7. Test plan

AC1 and AC6 use an independent import/bundle trace plus a literal expected demand set. AC2–5 and AC7–8 mutate one field or
evidence kind at a time in the record consumer. An executed native operation requires real platform subprocess evidence and a
clean follow-up; no test described here has been run. Source presence and a declared-only row can be established without
loading a library.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Public evidence-contract review if metadata changes. Permission/unsafe/dependency/wire gates are none for this document; any
future worker or guarded replacement needs its own existing owner gates.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

Documentation and finite evidence records only. No throughput, memory, compatibility-rate or containment claim. Operation
benchmark numbers are outside this record's acceptance.

## 10. Open questions (block implementation)

1. Schedule the parked implementation only after the issue's corpus/G2 pull trigger or an explicit exception; documentation
publication is not that scheduling decision.

2. Approve the existing evidence-owner metadata mapping and resolve the KEL-78 AC6/Legacy wording before publishing a policy
exception.

3. Identify the exact staged fsevents path, if any. Until observed, its row remains declared-only and unqualified.

4. Provide the actual runtime/native artifact and per-OS operator for each proposed supported operation; an unavailable lane
stays unqualified.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.

