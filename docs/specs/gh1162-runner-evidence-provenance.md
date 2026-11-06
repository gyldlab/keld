# Spec: Intended Bun case identity and runner evidence provenance
Status: draft
Linear: GH-1162 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Bun source receipt: 1.4.4 at `38afee6c84fcf48c0edb2466aa4b27d287a59c30`
Publication: proposed GH566 amendment; documentation only, do not merge.

## 1. Goal & non-goals

Admit a mapped Bun case only when the intended test ran once, passed under the declared semantics,
and has attributable runner evidence. Keep four decisions separate: output visibility, canonical
test identity, artifact provenance and expected-error meaning. A fix to one does not establish
the others.

GH566 remains the sole admission owner; GH663 consumes its validated result. This is a follow-on
contract decision, not a reopening of GH1158's narrower status-collision fix. Non-goals: modifying
frozen lifecycle-v0, invalidating historical receipts, changing the Electron pin, implementing
a second test framework/parser or claiming a production security vulnerability.

## 2. Spec refs

- [GH1162](https://github.com/gyldlab/keld/issues/1162), [owner-attributed Windows observation](https://github.com/gyldlab/keld/issues/1162#issuecomment-6462269327) and [source proposal](https://github.com/gyldlab/keld/issues/1162#issuecomment-6464755495).
- [GH566 D8/D13](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh566-corpus-manifest-owner.md) and [current admission code](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-compat/tests/support/corpus_admission.rs).
- [GH532](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh532-first-proof-evidence-rules.md), [KEL74](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel74-compat-evidence-schema.md) and [GH663](https://github.com/gyldlab/keld/issues/663).
- Pinned Bun [agent detection](https://github.com/oven-sh/bun/blob/38afee6c84fcf48c0edb2466aa4b27d287a59c30/src/bun_core/output.rs).
- Pinned Bun [JUnit construction, XML encoding and write lifecycle](https://github.com/oven-sh/bun/blob/38afee6c84fcf48c0edb2466aa4b27d287a59c30/src/runtime/cli/test_command.rs).
- Pinned Bun [expected-failure exception path](https://github.com/oven-sh/bun/blob/38afee6c84fcf48c0edb2466aa4b27d287a59c30/src/runtime/test_runner/Execution.rs).
- [Current CI pin](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/.github/workflows/ci.yml) selects Bun 1.4.4; [architecture 04](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md) governs evidence honesty.

No production boundary change. The issue's original Bun 1.4.2 reference stays historical; this
draft explicitly reasons from current 1.4.4 source. Source inspection is not native reproduction.
The Windows measurements remain attributed to their author; no fresh Bun execution occurred here.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given the declared Bun executable, the owner captures exact revision, invocation, target,
   working directory and allowlisted nonsecret settings. Unsupported revisions or behavior-
   changing reporter/config overrides fail qualification instead of silently changing admission.
2. Given a mapped Bun identity, it contains registered file, ordered enclosing named scopes
   and exact leaf. Exactly one corresponding executed result is required; a matching leaf in
   another scope/file, duplicate identity or missing intended case fails CaseNotAdmitted.
3. Given a mapping Bun cannot represent injectively, registration rejects it before admission:
   unnamed/empty scope, omitted scope beyond the 64-name limit, control-character loss, unnamed
   test or duplicate full identity. A unique XML row alone cannot prove that no collision occurred.
4. Given a structured report, the owner reads nested suite structure and exact file/leaf after
   bounded XML decoding. It never recovers scopes by splitting classname on a delimiter.
   Malformed, partial, unknown-shape or conflicting status output fails closed.
5. Given process success with missing, stale, unwritable, substituted, symlinked or truncated
   report, admission fails. A fresh invocation-bound regular artifact, successful termination
   and completed validated report are all independently necessary.
6. Given arbitrary stdout/stderr text resembling a runner pass, it cannot create an admitted
   result. A fresh report path or SHA-256 hash is not claimed to authenticate a producer against
   malicious code with the same privileges; that stronger claim requires an approved mechanism.
7. Given ordinary passing assertions of an expected error, admission requires the intended
   contract assertion to pass. A .failing test's generic pass after any exception cannot prove
   the intended error and is excluded from mapped conformance admission in the proposed policy.
8. Given fail, skip, todo, ignored/missing or unknown status, no passing admitted case is emitted.
   A nonzero process exit rejects the invocation even if the intended row looks passing.
   A passing regression may still map to expected conformance fail or unknown under GH532.
9. Given fresh child processes with AGENT/CLAUDECODE/REPL_ID/verbose/reporter combinations,
   qualification distinguishes display visibility from report identity/provenance. Parent
   environment is unchanged; console suppression cannot become success or a source-text fallback.
10. Given a cell declared for the host, missing intended evidence is CaseNotAdmitted. A cell
    not declaring the host remains in the unknown list. GH663 consumes only the approved
    admission object and cannot strengthen its trust by serializing it.
11. Given migration to this policy, a reviewed GH566 amendment and explicit registration opt-in
    preserve frozen v0/history and GH1158's status controls. Every control has a healthy unique
    positive follow-up; no new timing allowance, silent skip or global test weakening is added.

## 4. Design

### 4.1 Source facts and independent atoms

| Atom | Current source fact | Consequence |
|---|---|---|
| Visibility | AGENT is checked first; only literal 1 enables it; otherwise verbose/CLAUDECODE/REPL_ID affect agent detection; AI_AGENT is not read there | Child AGENT=0 is a candidate display override, not a provenance proof |
| Current admission | bun_case_passed strips optional timing and all enclosing display scopes, then requires one pass and no second same-leaf status | GH1158 closes status collision, but unrelated same-leaf identity remains a separate question |
| Structured identity | TestCaseReport gathers nonempty named scopes, caps them at 64 and then reverses to outermost first | A full-looking result may already have lost identity |
| JUnit structure | record_test_case emits nested testsuite elements; classname separately joins reversed scopes with a delimiter | Read nested structure, not delimiter-split classname |
| XML encoding | Forbidden C0 controls are dropped; tab/LF/CR are numeric references | Restrict mapped identifiers to lossless representable names |
| Report lifecycle | Empty report can avoid a write; open/write failures log and return Ok | Successful process termination alone does not prove a report exists |
| Expected failure | A pending .failing callback exception maps to Pass without matching an intended error there | Wrong throw can satisfy mode status; use ordinary explicit assertions |

These facts come from the exact Bun source linked above. No assumption about another Bun
version is inherited. GH566 D8 currently preserves command lines, environment and parsers;
even a child-only visibility override therefore requires an approved amendment.

### 4.2 Proposed canonical identity and representability

Propose an owner-managed mapped identity:
`{ file, scopes: [outermost, ..., innermost], leaf }`.
It is a structured tuple, not one delimiter-joined string. Keep existing CellKey ownership
unchanged. The GH566 amendment must name the exact manifest/registration representation and
migration; a leaf-only legacy mapping cannot be silently reinterpreted as this tuple.

The registered target path is the authority. Require that the runner-reported file resolves
to that exact repository file under the invocation root. Normalize platform separators only
after proving that relationship. Reject absolute/foreign paths, traversal, symlink substitution
and unreviewed case-folding aliases. Do not trim, normalize Unicode or strip prefixes blindly.

The proposed supported mapped subset uses literal, nonempty scope/leaf names with no ASCII
control characters, no unnamed tests/scopes and at most 64 named enclosing scopes. Names may
contain the display delimiter; structured nesting must retain their boundaries. XML predefined
and numeric character references decode normally; unsupported/lossy code points are rejected.

Require a complete reviewed identity census for the registered target, including nonmapped
tests that could collide. Reject dynamic/parameter-generated or otherwise unprovable identities
until an owner-approved mechanism establishes uniqueness and representability.
Source/registration metadata constrains identity; it never proves execution.

JUnit's optional source line is diagnostic, not a unique identity. Equal name paths at different
source lines still collide under this proposal. Repeated scopes coalesced by the reporter must
not be used to make those cases appear distinguishable.

This is a constrained proposal rather than a claim that current registration already has this
census. The exact census mechanism and schema amendment are approval gates. A custom regex
invented to recognize every possible TypeScript declaration is rejected.

### 4.3 Reporter choice and closed parsing

Recommend evaluating Bun 1.4.4's explicit JUnit output under the supported subset above. It is
constructed independently of the human only-failures branch and carries nested suite structure.
Acceptance still requires executed positive/negative qualification before selecting it.

The existing GH566 admission module owns the replacement adapter and its result. There must
not be a new corpus-specific parser or a second policy table. If an XML parser dependency is
needed, select a maintained existing dependency or pass the dependency gate; do not hand-roll XML.

Proposed parser limits: 16 MiB input, 100,000 test cases, 64 named scope levels plus the bounded
document/file/case wrapper depth, and 16 KiB per identifier. Reject excess before proportional
allocation. Review the concrete bounds with actual report sizes before approval.

Reject DTDs, external/entity declarations, network resolution, duplicate attributes, incomplete
documents, unexpected nodes/status forms and inconsistent file/suite identity.
Allow the ordinary predefined XML references and valid numeric character references needed to
decode the pinned writer; do not disable decoding and accidentally compare escaped names.

A successful result is exactly one intended testcase with the pinned ordinary-pass shape, no
failure/error/skipped/todo evidence, a successful complete process and approved provenance.
Aggregate totals and assertion counts alone cannot substitute. Unknown reporter format fails
qualification; there is no fallback to stderr, source-name discovery or expected totals.

### 4.4 Invocation and artifact lifecycle

Create a fresh private per-invocation directory outside the fixture tree and give the pinned
runner one explicit report destination. The report must not pre-exist. Record selected executable,
exact revision, argv, registered target, working root and reviewed reporter/config inputs.

The parent owns directory/report identity. Require a regular file under that directory, refuse
links/reparse-point escapes and substitution, and read through the validated opened handle.
After process completion, seal/copy the exact accepted bytes into the run evidence before
computing its digest. Do not reopen an untrusted path and assume it still names those bytes.

Missing, empty-with-no-required-cases, stale, malformed, partial, over-limit or unwriteable output
fails even with exit zero. The parent must separately establish successful child termination and
cleanup. Time stamps alone are not freshness evidence; a retained prior report cannot satisfy
a new invocation.

Bun can return Ok after report write errors, so exit status and artifact validation are separate.
Do not depend on the runner propagating filesystem failure as a nonzero exit.

### 4.5 Provenance threat model and the unresolved stronger guarantee

A dedicated structured artifact prevents ordinary console logging from being mistaken for a
runner result. Its fresh path, source binding and exact bytes also detect many stale/substituted
artifact mistakes. These are useful, limited properties.

They do not authenticate the producer against malicious test code with the same filesystem and
process privileges. Such code may discover a path or overwrite artifacts. Hashing attacker-chosen
bytes or storing a nonce visible to that code does not solve that problem.

Proposed initial trust scope is reviewed repository fixtures without malicious same-privilege
artifact forgery. Under that scope, the owner must still reject console spoofing, stale/foreign
artifacts and all identity/status ambiguity. The human must explicitly approve this scope before
the policy is described as trustworthy for GH663.

If GH663 requires resistance to malicious same-privilege fixtures, this proposal is blocked
until an independently protected runner-origin witness is demonstrated at the pinned version.
Do not invent a security boundary, add ambient authority or claim that JUnit alone provides one.
A hostile report-forgery experiment must be reported as an unverified limitation if it succeeds,
not hidden by weakening the test or relabeling authority_profile.

KEL74's authority_profile describes the runtime's permission containment. It must not be
repurposed as an evidence-authenticity flag. Until the provenance decision is approved, do not
issue a stronger admitted object to GH663 or claim retroactive trust for historical runs.

### 4.6 Expected-error and status semantics

Propose excluding .failing-mapped tests from conformance admission. Instead, an ordinary test
invokes the operation, asserts the exact intended error/code/side effect, and fails when a wrong
exception occurs. This separates runner success from the behavior the cell is meant to prove.

The source path reviewed for .failing maps an uncaught callback exception to ordinary Pass
while pending; it does not compare the intended error contract there. A report alone cannot
recover that intent or even reliably classify the test mode after it has become Pass.
The registration/census must therefore enforce the permitted mode before execution.

Skip, todo, failed, timed-out, missing, ambiguous and unknown-format results cannot admit a
declared-host mapped case. All process-level failures fail closed. A passing ordinary test is
then mapped to its manifest expected_verdict by GH566/GH663, not automatically to conformance pass.

### 4.7 Environment and migration

Pin the executable and explicitly select the qualified reporter. Record only allowlisted
nonsecret effective settings. Child-only AGENT=0 is the preferred visibility experiment because
the inspected detector checks it before CLAUDECODE/REPL_ID, but it remains subject to qualification.
AI_AGENT removal alone has no demonstrated effect on that detector.

The detection state is process-cached; run environment matrix controls in fresh child processes.
Reporter CLI/config precedence is a separate input and must be verified, not inferred from
the agent detector. Never mutate global parent settings or weaken assertions to repair visibility.

Amend GH566's exact D8 command/environment/parser preservation clause with an explicit new
registered admission mode. Existing frozen v0 and historical receipts retain their old reader
and honest limitations. New GH663 claims require the new qualified mode; no silent fallback.
GH1158's same-leaf status-collision controls remain regression tests inside the owner.

Runtime seam: existing owner starts Bun and owns output/artifact/process lifetime. No product
IPC, authority, native callback or public API changes. Compatibility is an explicit registration
migration, not a second standalone admission framework.

## 5. Boundaries

Future edits: approved GH566 amendment, its existing admission/registration code and tests,
mapped-identity metadata and narrowly reviewed runner reporter options. This draft adds one file.
Do not edit GH1158's held work, frozen lifecycle corpus, Electron pin, KEL74 schema, product
permission labels, global environment or unrelated test assertions.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Approve the intended trust scope, structured identity/census representation and exact
  GH566 migration; record version/CLI/config qualification without changing existing behavior.
- [ ] T2 Add owner-level identity/representability controls, including same leaf in another
  scope, delimiter names, unnamed scopes, C0 loss, duplicates and 63/64/65 named levels.
- [ ] T3 Qualify structured reporter parsing and artifact lifecycle with fresh/stale/missing/
  unwritable/substituted/truncated reports and console-spoof controls.
- [ ] T4 Enforce ordinary expected-error semantics and full status rules; qualify fresh-process
  environment/reporter combinations and preserve GH1158 regression controls.
- [ ] T5 Migrate only explicitly approved registrations, return provenance-bearing admission to
  GH663 and independently validate one real producer run without rewriting historical receipts.

## 7. Test plan

| AC | Positive and decisive negative control |
|---|---|
| 1 | Exact pinned invocation; wrong revision, foreign target or unreviewed config |
| 2 | Unique full identity; intended absent while unrelated matching leaf passes |
| 3 | Representable tuple; empty/unnamed scope, C0-loss pair, duplicate or 65th named scope |
| 4 | Nested suites including delimiter text; split classname, corrupt XML or unknown status |
| 5 | Fresh complete report; stale, missing, symlink/reparse, replacement, short write or truncation |
| 6 | Console imitation ignored; hostile same-privilege report forgery exposes the stated limit |
| 7 | Ordinary exact-error assertion; wrong throw under .failing cannot count as intended proof |
| 8 | Ordinary pass; fail/skip/todo/timeout or nonzero exit despite an apparent pass row |
| 9 | Fresh-process environment matrix and explicit reporter options; no console fallback |
| 10 | Declared-host missing case fails; undeclared-host cell stays unknown |
| 11 | New opt-in plus unchanged frozen corpus checks; remove GH1158 status-collision protection |

All controls are planned. Source inspection and the attributed Windows observation are the
current evidence. Run relevant platform report/path controls before claiming their qualification;
do not infer Windows filesystem identity handling from Unix success.

## 8. Review gates triggered

Documentation: none. Implementation requires exact human approval of the GH566 amendment,
identity/census contract and provenance threat model. Any XML dependency and public document
shape change needs its own review. No production permission or wire change is planned.
Bun version changes require requalification of all source-dependent assumptions.

## 9. Perf impact

No measured claim. One runner per registered target remains the goal; parsing/artifact memory
is bounded. Measure report size, parse time and retained bytes on actual targets. Do not trade
identity completeness or required case execution for a faster green admission.

## 10. Open questions (block implementation)

- Approve trusted-fixture scope, or demonstrate protected producer authenticity before stronger claims.
- Select the exact GH566 tuple/census representation and enforceable supported-source subset.
- Qualify Bun 1.4.4 reporter CLI/config precedence, XML shape and actual cross-platform artifact handling.
- Approve bounds and any parser dependency; execute negative controls before selecting the adapter.
