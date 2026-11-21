# Spec: Two-arm Electron differential oracle harness
Status: draft
Linear: GH-494 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: future implementation proposal; documentation only, do not merge.

## 1. Goal & non-goals

Run the same small fixture under verified Electron and Keld, preserve attributable observations,
and compare only the behaviors a cell declares. A matching transcript is an observation; a cited
upstream contract, executed mapped test and independent negative control establish conformance.

Non-goals: implementing the recorder here, repairing runtime behavior, running Electron's test
suite, introducing product percentages, or enabling a schedule. GH495 owns lifecycle recording
and replay; GH537 and other family owners own their fixtures; GH585 owns a parked drift lane.
This draft does not supply the missing GH411 nondeterminism receipt.

## 2. Spec refs

- [GH494](https://github.com/gyldlab/keld/issues/494) and [GH411 decision packet](https://github.com/gyldlab/keld/issues/411#issuecomment-6022089419).
- [GH532 Rules 1–7](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh532-first-proof-evidence-rules.md) own pin, citations, verdicts, digest classes and labels.
- [GH566 D2/D3/D7/D8](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/gh566-corpus-manifest-owner.md) owns registration, parsing, denominator and run validation.
- [KEL74](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel74-compat-evidence-schema.md) owns the evidence format and scoring.
- [KEL77 §4.3](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel77-bun-child-process-differential.md) and [live private helper](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/tests/child_process_differential.rs).
- [Architecture 01](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/01-overview.md) and [06](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md) govern ownership and runtime testing.
- [Pinned Electron ESM rules](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/tutorial/esm.md), [release metadata](https://api.github.com/repos/electron/electron/releases/394303164) and [test workflow](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/.github/workflows/pipeline-segment-electron-test.yml).

No shipped runtime boundary changes in this publication. Section 4 proposes a future corpus
format amendment under GH532/GH566; the currently approved v1 contract remains unchanged.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a packaged fixture, both arms use its identical committed JavaScript and assets; ESM
   main listeners register before any await, and a renderer fixture has a CommonJS `.cjs`
   preload and static page. A missing family export fails that fixture without hiding others.
2. Given a record invocation, the selected executable comes from the matching verified archive
   outside the fixture tree; missing archive, wrong digest or wrong runtime identity fails
   before any golden is accepted. Tag, npm and archive identities are recorded separately.
3. Given parsed rows, each process has contiguous increasing sequence numbers and a declared
   stable identity; invalid kinds, duplicate sequence numbers and malformed arguments fail.
4. Given two transcripts, comparison preserves per-process event order and declared causal
   edges, with timestamps ignored only as diagnostic fields. Swapped names/order or a violated
   edge fails even if all observed events remain present.
5. Given a normalizer, it is cell-local and names the GH411 raw observation that justifies it.
   Without that receipt the normalizer set is empty. Event name, kind, order, Error.message and
   exit status are never normalized; an undeclared varying field fails deterministically.
6. Given fixture bytes, one shared KEL77-compatible digest owner computes the length-framed
   set digest. The KEL77 caller migrates to it with byte-identical historical results and no
   second framing implementation. Rename/content-shuffle and length mutations are rejected.
7. Given a future differential corpus, its admitted shape declares `fixture_set`; records
   and denominator bind that set, while a separate exact manifest digest binds receipt policy.
   Existing v0/v1 corpora retain `manifest_bytes`; a v1 manifest requesting `fixture_set` fails.
8. Given a completed arm, timeout, parse failure or parent unwinding, owned children are killed
   when necessary and reaped before return. An output limit also terminates the arm. A fixture
   module-link failure is a failed observation carrying the selected existing KELD diagnostic.
9. Given an uncited observation, emitted KEL74 verdict is `unknown` even when transcripts match.
   A documented pending cell retains its implementing owner and declared expected failure.
   Ladder labels never become record verdicts; an arm failure never becomes silent success.
10. Given ordinary PR validation, it replays committed evidence offline without downloading or
    starting Electron. Recording is an explicit developer command; changed evidence enters
    through one reviewed fixture/golden/receipt/cell update, never a Keld-generated baseline.

## 4. Design

### 4.1 Owners, trust and current gaps

GH532 owns evidence meaning; GH566 remains the one parser and validator; this spec owns the
differential fixture and transcript contract. GH495 owns the command and child lifetime adapter.
Observation bytes are untrusted input. A JSON row, correct digest or exit zero alone is not proof
that the intended runtime or test ran. GH1162 owns Bun admission provenance separately.

At the source baseline, GH566 only accepts `artifact_digest: manifest_bytes`; its denominator
digest is the exact manifest bytes. KEL77 has the separate private `digest_named_blobs` helper.
There is no GH411 execution receipt in the reviewed thread. Its decision packet specifies
experiments; it does not establish deterministic ordering or any allowed normalizer.

Compatibility fallback: not required for the new harness. Existing KEL77 records, frozen
lifecycle v0 and registered showcase/product v1 corpora retain their meaning.

### 4.2 Fixture package and transcript

Use `package.json` with `type: module`, a plain `main.js`, and when needed
`preload.cjs` plus a static nonempty HTML page. Keep each family's imports isolated.
Register lifecycle and event listeners synchronously before the first top-level await.
No TypeScript compiler, remote page, lazy dependency fetch or timing sleep is part of a fixture.

Proposed row fields are `process`, `seq`, `kind`, `name`, `args`, plus optional diagnostic
timing outside comparison. `process` is a declared logical role, not a PID. `seq` starts at one
and is contiguous per process. The closed `kind` vocabulary is `event|call|resolve|reject|exit`.
Arguments use exact JSON values with deterministic object-key serialization; unsupported values
must have an explicitly reviewed tagged encoding, never lossy string coercion.

An Error projection retains exact name/message and any contract-relevant code. A dynamic numeric
argument, PID or platform string cannot be discarded simply because it differs. Prefer fixtures
that avoid irrelevant volatile fields. Any transformation still needs the receipt in AC5.

Causal edges name two exact logical-row identities and the fixture operation that causes them,
for example a request observation preceding its correlated reply. Compare each process sequence
first, then edge obligations; do not infer a global order from wall clocks or pipe arrival.
A missing edge endpoint fails. The GH411 renderer arm must qualify any additional ordering claim.

### 4.3 Electron identity and recorder preparation

| Identity | Value at the reviewed pin | Evidence status |
|---|---|---|
| Electron version | `44.4.5` | Release/tag metadata read |
| Tag commit | `694f45852a0f1726cd23bfd379854de489cccb65` | Annotated tag peeled through GitHub API |
| npm dist shasum | `0be094f8e783febba75d591106f5c61c1e511b70` | Owner-recorded GH411 packet; fresh npm receipt still required |
| darwin-arm64 archive SHA-256 | `a212eee63ba2f45fd83bd28f77a3e3313a336ad17a4c25adf617942eef5e0e2c` | Official release asset digest read |
| linux-x64 archive SHA-256 | `04586a0ec46c3283fbdaef85530f561f71f0b5e136ad0cb9ef63683615609780` | Official release asset digest read |
| win32-x64 archive SHA-256 | `11c395820a5aaa8ebcc0686b476d0ac98a730274ebfbdc8cf5538a7c2815cb5d` | Official release asset digest read |

Before approval, compare each archive digest with the exact matching line of the release's
`SHASUMS256.txt` and retain those raw bytes. Metadata inspection is not a downloaded-archive
verification. The raw checksum file could not be retrieved in this drafting session; no binary
was downloaded or executed. A runtime startup witness also checks `process.versions.electron`
and actual main-process API shape. GH495 specifies child-only environment control.

The pinned upstream test workflow sets CrashReporter `DialogType=server` and Electron
`ApplePersistenceIgnoreState=YES` on macOS. Reproduce the relevant settings on a dedicated,
authorized recorder account, capture previous values and restore them on all exits.
Do not copy unrelated setup-assistant termination, screenshot permission or Xcode steps into
a user's machine. macOS arm64 is the first recorder lane; other display qualifications are future.

### 4.4 One fixture-set digest and an explicit owner amendment

Promote the exact KEL77 framing function into proposed test-support owner
`crates/keld-compat/tests/support/fixture_digest.rs`; both test crates consume that source.
Keep enumeration local to each caller, but pass one ordered set of UTF-8 relative POSIX paths
and bytes to the shared pure helper. Reject duplicate paths, escapes, symlinks and empty sets
at the new fixture intake. Preserve the existing KEL77 corpus byte-for-byte during migration.

Framing is SHA-256 over each byte-sorted relative path, a NUL separator, little-endian u64 byte
length and raw file bytes. The fixture root contains only inputs: package, JavaScript, preload,
static page and referenced assets. Archives, installed runtimes, goldens, manifests and receipts
live outside it; otherwise recording changes its own input identity.

For **new differential harness corpora only**, propose registration-owned
`Shape::DifferentialV2`, document tag `keld.compat.corpus/v2`, and
`artifact_digest: fixture_set`. It lowers into GH566's existing Corpus owner.
This is the future class already identified by GH532 Rule 5, not an accepted v1 spelling.

| Binding | Proposed differential value | Current showcase v1 value |
|---|---|---|
| Record `artifact.sha256` | Fixture-set digest | Exact manifest digest |
| Denominator `corpus_sha256` | Same fixture-set digest | Same manifest digest |
| Receipt `manifest_sha256` | Exact raw manifest bytes | GH663 receipt proposal |
| Receipt fixture binding | Exact same fixture-set digest | Optional source evidence, not artifact meaning |

The owner validates registration, manifest shape, exact cell set, document snapshots, fixture
digest and receipt binding before scoring. A one-byte manifest edit invalidates its receipt even
when fixture inputs are unchanged. A one-byte fixture edit invalidates artifact and denominator.
Do not add a second score path or silently loosen `Shape::V1`. The amendment must update GH532's
closed-set explanation and GH566 D2/D3/D7/consumer protocol in one approved change.

Rejected for this differential class: assigning `artifact.sha256` to manifest bytes, installed
Electron binaries, a JSON reserialization, or an arbitrary ZIP. Each has a separate named digest.
GH663's existing v1 showcase producer continues to use manifest bytes.

### 4.5 Failure and lanes

Arm state is prepared → running → terminal → reaped. Timeout, output overflow, parse rejection,
startup mismatch and panic/unwind all reach the owned reap path; terminal observation is not proof
of OS quiescence. Bound output and parsing before allocating from child-controlled lengths.
A run receipt is accepted only after required cleanup succeeds.

A Keld missing-export error is recorded as an arm failure with the relevant existing diagnostic;
it never suppresses neighboring fixtures. For an uncited cell, GH532 still forces evidence
`unknown`; the separate run failure stays visible and fails the harness. No invented KELD code
is allocated by this spec; GH1101 and the implementation owner must approve any unmet code need.

PRs replay committed goldens offline. Developer recording uses the verified Electron arm.
GH585 may later compare fresh recordings with those goldens; it cannot rewrite them.

Rejected alternatives: running or translating Electron's spec tree; the golden transcript as
sole oracle; wall-clock timing comparison; re-recording goldens from the Keld arm; a CDP/Playwright
driver; TypeScript fixtures behind a transpile step; lazy npx download at record time.
These either change the measured subject, add an unnecessary driver or weaken the source contract.

Runtime seam: test parent owns arm processes, pipe buffers and cleanup; no production callback,
principal or capability changes. New types are test-only rows, causal edges and the proposed
GH566 shape. No kipc wire, permissions or shipped API changes. Public document-format review is
required before the future shape or shared test-source migration is implemented.

## 5. Boundaries

Future work may touch keld-compat test support/fixtures, the KEL77 test caller and the exact
GH532/GH566 amendment. This publication adds only this spec file.
Do not modify runtime behavior, product corpus receipts, frozen v0, existing v1 digest meaning,
CI schedules, dependency manifests or architecture policy without the separate applicable gate.
Root owns catalog edits; other family authors own their fixture implementations.

## 6. Tasks (ordered, each independently reviewable)

- [ ] T1 Complete GH411 execution receipt and checksum/npm identity receipt; qualify macOS prep
  and independent normalizer controls, then obtain exact spec/owner-amendment approval.
- [ ] T2 Promote the digest helper with KEL77 byte-equivalence and collision-framing controls;
  admit only the reviewed new differential shape through GH566, preserving all old corpora.
- [ ] T3 Implement package/row/edge validation and comparator with malformed, order and
  undeclared-field controls; feed existing evidence validation.
- [ ] T4 Integrate GH495 process ownership and developer recording with identity, cleanup and
  per-fixture failure controls; qualify one real macOS fixture.
- [ ] T5 Add offline PR replay and atomic evidence-update review, keeping GH585 parked.

## 7. Test plan

| AC | Independent positive and negative proof |
|---|---|
| 1 | ESM/main and CJS/renderer fixture runs; move listener after await or import absent export |
| 2 | Verified archive startup; change one checksum digit, absent archive, Node-mode child |
| 3 | Valid rows; duplicate/missing sequence, malformed argument, unknown kind |
| 4 | Same per-process sequence; swap events, remove an endpoint or reverse a causal edge |
| 5 | Receipt-backed declaration; remove citation or normalize a forbidden field |
| 6 | Known KEL77 digest vectors; delete length framing, rename/content shuffle, retain duplicate owner |
| 7 | New shape receipt/denominator binding; mutate manifest, fixture or v1 digest selector separately |
| 8 | Owned successful child; force timeout, overflow, parser error and unwind, then census live children |
| 9 | Cited/pending/uncited cells; mutate unknown to pass or suppress a module-link failure |
| 10 | Network-disabled replay; recorder invocation in PR route or Keld-generated golden is rejected |

No listed experiment ran in this drafting session. Use handshakes and process-exit evidence,
not sleeps. Run per-platform cleanup controls before claiming that lane.

## 8. Review gates triggered

Documentation publication: none. Implementation: human approval of this exact spec plus GH411;
GH532/GH566 public document-format amendment; cross-crate test-support migration review; dependency
review only if the chosen implementation adds a manifest entry. Existing permission and wire
owners are unchanged. Native recording needs an actual capable platform owner.

## 9. Perf impact

No measured change. Proposed work adds test processes and bounded transcript storage only.
Record raw run time, peak retained rows and artifact size during qualification; do not claim an
improvement or relax a production budget. Output/fixture limits must be chosen before approval.

## 10. Open questions (block implementation)

- GH411 raw repeated-run/renderer receipt, including any allowed normalizer, is still absent.
- Approve the explicit DifferentialV2/digest amendment and the shared test-source location.
- Verify raw SHASUMS lines and fresh npm receipt; qualify recorder OS preparation and limits.
- Select existing typed diagnostics for malformed rows, undeclared variability and child cleanup,
  or obtain any necessary allocation through GH1101 before emitting a new identifier.
