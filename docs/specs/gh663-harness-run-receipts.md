# Spec: Harness-run receipt and KEL74 record producer
Status: draft
Linear: GH-663 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: proposed GH566 amendment; documentation only, do not merge.

## 1. Goal & non-goals

Turn one real registered showcase run into reproducible, receipt-bound KEL74 records.
The owner must prove which exact mapped cases passed on the actual host, then derive each
record's declared conformance verdict. No record is written by hand or inferred from a green
test-process exit, source text or a matching display name.

This is an additive amendment proposal to GH566 D7/D8/D13. It does not modify GH566 here.
Non-goals: a second validator, product receipts (GH587), differential Electron recording
(GH495), frozen lifecycle-v0 re-recording, a KEL74 schema change or native acceptance by proxy.

## 2. Spec refs

- [GH663](https://github.com/gyldlab/keld/issues/663) and [earlier preparation](https://github.com/gyldlab/keld/issues/663#issuecomment-6102147630).
- [GH566 D7/D8/D13](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh566-corpus-manifest-owner.md).
- [GH532](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh532-first-proof-evidence-rules.md) and [KEL74](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel74-compat-evidence-schema.md).
- Current [admission owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-compat/tests/support/corpus_admission.rs),
  [run validator](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-compat/tests/support/corpus_runs.rs) and
  [manifest/registry owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-compat/tests/support/corpus_manifest.rs).
- [Current app corpus](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-compat/fixtures/app-corpus/corpus.json), [GH445](https://github.com/gyldlab/keld/issues/445) and [GH448](https://github.com/gyldlab/keld/issues/448).
- [GH1162 provenance amendment](gh1162-runner-evidence-provenance.md) supplies Bun admission requirements.
- [Architecture 04](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md) governs honest compatibility reporting.

No production boundary change. The proposed receipt is a new owner-managed document contract
and requires explicit approval before implementation.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given an approved GH566 amendment, one existing owner parses the closed harness receipt,
   verifies its exact raw bytes and joins it to records before scoring. Unknown fields, duplicate
   keys, bad paths and a missing or mismatched receipt fail closed; there is no second validator.
2. Given one producer invocation, all output names one actual platform/architecture, immutable
   tested Keld commit, exact Bun revision and registered corpus. Any mixed commit, runtime,
   receipt or platform inside its run is rejected.
3. Given a host-declared cell, the producer writes a record only after the owner returns its
   exact intended admitted passing case and validated runner evidence. Missing, skipped,
   ignored, cfg-gated, ambiguous or failed cases produce no successful run artifact.
4. Given admitted mapped tests, each record's result equals its manifest expected_verdict,
   including expected fail or unknown. A passing regression is never automatically relabeled
   conformance pass. Missing or extra declared-host cells fail the receipt/record join.
5. Given an undeclared-host cell, the receipt records it as unrun and the producer emits unknown
   for that cell. That is not a successful execution. Cross-platform records never borrow a
   macOS host identity or imply that another engine ran.
6. Given every record, authority_profile is legacy_sandbox_off, oracle revision is the corpus
   pin, artifact.sha256 is exact manifest bytes and revisions name tested Keld/Bun/engine.
   evidence_uri equals the SHA-256 of the committed raw receipt bytes.
7. Given one byte changed in a receipt, every record still referencing its former hash fails.
   Re-serializing or recomputing only a record digest cannot bypass the other identity/case checks.
8. Given the registered electron-app-v1 corpus, one real macOS run produces its complete macOS
   receipt and records; actual Linux/Windows producer-accounting runs may emit their unknown
   rows. Until those runs exist, the report preserves missing/unknown coverage and GH445 AC4
   is not declared complete.
9. Given committed evidence, existing committed-runs validation groups by platform/architecture
   and now enforces one complete current receipt/run identity per group. Mixed old/new records,
   duplicates, unreferenced receipt substitution and unexpected missing records fail.
10. Given a failed or interrupted producer, staged output never replaces the last committed
    complete set. Successful publication includes receipt, records, owner validation and
    provenance together; no filesystem or CI success claim is inferred from this draft.

## 4. Design

### 4.1 Verified gaps and current inventory

The live AdmissionReport contains only the undeclared-host unknown list. The owner checks
whether cases passed but does not return admitted case identities and their provenance to a
producer. validate_harness_run checks records and labels, not receipt file bytes. Run identity
currently groups platform/architecture without proving common tested commit or Bun revision.

These remain distinct gaps. A correct hash cannot fix ambiguous admission; exact admission
cannot fix a substituted receipt; consistent records cannot prove that a real run produced them.

The old preparation comment said electron-app-v1 was unregistered at its earlier main.
That statement is stale: current REGISTRY includes lifecycle-v0, window-v1, app-v1 and native-v1.
App-v1 maps macOS cells to app-surface.test.ts and declares engine tokens for all three platforms.
There are no committed app/window/native evidence files in the reviewed baseline tree.

Reuse GH566's hashing, closed parsing, exact cell keys, admission, record checks and single score
call. Extend its result and run-validation seam. Compatibility fallback: frozen v0 remains on
its existing admitted historical path; migration cannot re-label or rewrite its receipts.

### 4.2 Proposed receipt contract

Proposed schema tag: `keld.compat.harness-run/v1`, owned entirely by GH566.
Commit one current receipt at `<fixture_dir>/receipts/<platform>-<arch>.json`.
The filename is an index, never authority: verify it against parsed fields and registration.
Reject symlink traversal, absolute paths, escapes, unknown keys and duplicate object members.

| Field | Meaning and check |
|---|---|
| schema | Exact proposed tag |
| corpus_id | Registered showcase v1 corpus, never a product/differential/frozen-v0 substitute |
| manifest_sha256 | SHA-256 of exact corpus.json bytes, also current artifact/denominator digest |
| oracle_revision | Exact corpus Electron commit |
| platform, arch | Actual producer host; existing KEL74 vocabulary |
| tested_commit | Full immutable Keld commit actually tested; dirty inputs are rejected |
| bun_revision | Exact executable revision used, not a requested version string |
| engine_revision | Declared platform engine token at the tested Keld commit |
| invocation | Local host label or CI run/job identity, recorder/producer revision and nonsecret effective options |
| admission_policy | Exact approved GH566/GH1162 policy revision used |
| runner_reports | Registered target, runner kind, report identity/hash and validated provenance references |
| admitted | Exact CellKey, intended case identity, target, runner and manifest expected verdict |
| unknown | Exact CellKeys not declaring this host; reason is undeclared_platform |

The invocation union is closed: CI contains provider/run/job identifiers; local contains a
nonsecret operator-selected host label and run identifier. Do not dump machine secrets or
pretend that a label authenticates execution. Each runner report belongs to this invocation.

Case identity is supplied by the admission owner. For Bun it is the approved GH1162 canonical
file/scopes/leaf identity, not a copied leaf display name. Libtest retains its existing exact
registered case contract and must have a separately reviewed provenance limit if stronger
authenticity is claimed. This issue does not upgrade libtest trust by hashing its stdout.

Proposed initial limits are 16 MiB raw receipt and 100,000 case entries, with length/entry checks
before proportional allocation. The owner must review these against real corpus size before
approval; oversize is a named failure, never truncation to an apparently complete receipt.

### 4.3 Extended admission result

Propose extending the existing AdmissionReport with owner-constructed admitted records carrying
`corpus_id, CellKey, target, runner, canonical_case, report_ref`.
Construction occurs only after successful runner termination, exact intended-case admission
and all GH1162 requirements applicable to that runner. Callers cannot mint this result from
plain strings or aggregate test counts.

Combine the distinct registered runner kinds for the one producer invocation. Every declared
host cell must have exactly one admitted case; unknown entries equal exactly the cells whose
platform set excludes the host. Reject duplicate, missing, unexpected or cross-corpus identities.

The receipt copies these validated identities and expected verdicts. During offline validation,
the owner rechecks the declared map, report hashes and approved provenance contract; it does
not re-run an old command or assume that a matching digest makes a report authentic.
GH1162's unresolved same-privilege producer threat model remains a gate to stronger trust claims.

### 4.4 Record derivation and hash graph

For each admitted cell, derive the existing KEL74 record from the registered manifest and actual
run identity. Do not accept caller-supplied verdicts or engine strings.
Expected fail means the mapped regression asserts pending or divergent current behavior and
passes. Expected unknown stays unknown even if the mapped test passed.

For a cell not declaring the actual host, emit an unknown record and include it only in the
receipt's unknown set. It must not appear in admitted. A Linux or Windows producer run that
returns only unknown cells establishes accounting on that host, not native behavior execution.

Use existing `evidence/*.json` storage with one KEL74 record per file. Proposed filenames are
`<platform>-<arch>-<six-digit-index>.json`, indexed by byte-sorted CellKey tuple for that complete
run. Validate membership from contents, never from ordinal. Publish the entire group atomically.

The receipt contains input/report hashes and admitted identities, but neither its own digest
nor hashes of the record files that point back to it. Records reference exact raw receipt bytes
through evidence_uri. This acyclic graph avoids an impossible self-hashing document.

For this v1 showcase producer, artifact.sha256 and denominator corpus_sha256 remain exact
manifest bytes. GH494's proposed differential fixture-set format is a different registration
and cannot be selected by changing this receipt's string fields.

### 4.5 Run validation before one score call

Within the existing GH566 validator:

1. Load the registered corpus and its exact manifest/snapshot/denominator bindings.
2. Resolve the platform/arch receipt beneath that corpus, parse bounded closed fields and hash
   raw bytes without JSON normalization.
3. Check corpus, platform/arch, pin, manifest, tested Keld/Bun/engine and admission-policy identity.
4. Validate runner report references and exact declared-host admission/undeclared-host unknown sets.
5. Parse every record through KEL74 and apply existing owner membership, result, label and engine checks.
6. Require each expected record exactly once, its receipt hash exact and its revisions identical
   to the same run. Reject an admitted cell record whose result differs from manifest expectation.
7. Score once through existing GH566/KEL74 and preserve pending/divergence/unknown accounting.

A proposed owner API adds a validated HarnessReceipt view to validate_harness_run.
Do not duplicate checks in the producer and call that independent validation; the committed
reader must independently derive expected values from the approved owner and bytes.

Existing current-format behavior allows unknown on an unrun declared lane. This new complete
producer receipt promises exact declared-host admission, so it cannot use that allowance to
hide a missed required test. Historical/frozen validation keeps its own approved rules.

### 4.6 Publication, process ownership and trust limits

Produce in a fresh private staging directory. Validate inputs before running; bind the tested
commit and reject uncommitted source/fixture changes. Capture bounded runner outputs and wait
for owned process completion/cleanup. A partial write or failed validation leaves staging
diagnostics but cannot replace the current evidence group.

A producer is not a cryptographic attestation service. A SHA-256 hash detects changed bytes;
a CI URL or local host label describes origin; neither prevents malicious same-privilege test
code from forging artifacts. GH1162 must approve the actual trust scope before GH663 presents
Bun records as independently trustworthy. KEL74 authority_profile describes permission
containment, not test-evidence provenance; do not misuse it to encode this distinction.

No production principal, OS grant, IPC channel or native callback changes. The execution seam
is the existing test owner supervising registered runners. No generic tool framework is added.

## 5. Boundaries

Future edits: GH566 D7/D8/D13 amendment, existing corpus admission/run/manifest support, committed
reader tests and new per-corpus receipt/record outputs. This publication creates this file only.
Do not change KEL74 record fields, product GH587 receipts, Electron goldens, frozen lifecycle-v0,
unrelated corpus policies, runtime implementations or existing pending PR ownership.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Obtain approval for the exact GH566 receipt amendment, GH1162 admission policy and
  limits; reconcile current registered corpora and all frozen compatibility checks.
- [ ] T2 Extend existing owner admission results and receipt parser/join with independent
  malformed, identity, skipped-case and hash controls; retain one validator and score path.
- [ ] T3 Implement staged producer derivation with real tested-revision capture, complete
  set checks, bounded process/output ownership and failure-safe publication.
- [ ] T4 Execute one real macOS app-v1 run; commit exact receipt/records and negative-control
  evidence, then independently validate the committed bytes.
- [ ] T5 Run actual Linux/Windows producer accounting for unknown coverage, verify complete
  platform groups and hand the exact results to GH445/GH448 without claiming native execution.

## 7. Test plan

| AC | Independent test and falsifier |
|---|---|
| 1 | Existing owner admits closed receipt; unknown/duplicate key, missing file or escaped path |
| 2 | One run; substitute another tested commit, Bun revision or platform in one record |
| 3 | Intended admitted case; missing/skip/cfg/foreign-leaf/provenance failure |
| 4 | Pending regression expected fail; change its record to pass or omit one admitted cell |
| 5 | Undeclared-host unknown; insert its key into admitted or emit pass |
| 6 | Owner-derived fields; wrong label, engine, pin, manifest digest or receipt URI |
| 7 | One whitespace-byte receipt change invalidates every old URI |
| 8 | Real app-v1 macOS run; synthetic native receipt or macOS-host fields in Windows output |
| 9 | Existing committed grouping; combine two individually valid runs or old/new record subsets |
| 10 | Interrupt/error during staging; last committed complete group remains independently valid |

All listed tests and producer/native runs are planned. The publication performs source review
and documentation checks only. The first real receipt must include executed negative controls,
not just names of proposed tests.

## 8. Review gates triggered

Documentation: none. Future implementation needs exact human approval of this GH566 document
contract and GH1162's policy before trusting Bun admission. Any new parser dependency needs its
own dependency gate. Real macOS execution requires a capable owner; unknown other-host accounting
does not replace native acceptance. No KEL74/wire/permission schema change is authorized.

## 9. Perf impact

No measured cost. One existing target execution per distinct target, bounded receipt/report
storage and one score call are the intended design. Measure actual size/runtime at qualification;
do not skip a required target or silently truncate evidence to fit a budget.

## 10. Open questions (block implementation)

- Approve exact receipt shape, owner API, size bounds and atomic publication behavior.
- GH1162 must settle supported canonical identities and the claimed producer trust model.
- Real macOS app-v1 run and Linux/Windows unknown accounting remain unexecuted.
- Determine the approved provenance scope for libtest before applying any stronger claim to it.
