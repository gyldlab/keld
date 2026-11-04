# Spec: draw.io first-proof product corpus physical partitions (GH-500)

Status: approved
Human approval: [GH #500 exact-source implementation approval](https://github.com/gyldlab/keld/issues/500#issuecomment-6383291604), for design commit `284498123e80161042fdf362a62b57f7305fec5b`, original SHA-256 `eb9f2e3fdfcf5d197d09111a2468588d802b446545b06969748436c965ff4ff4`. This status-only amendment changes no design decision.
Linear: GH-500 (#517 GitHub tracker of record; KEL-237 reference only) · Owner: @0monish / GYLDLAB · Updated: 2026-10-24
Disposition: OPEN / DRAFT / DO NOT MERGE. No product runtime, executable receipt, scored draw.io result, committed product registry entry, or merge is authorized by this proposal.
Basis: `origin/main b4e9f4f2e0dc88ccb14407e4e839b50fabb6be50`. GH-587's approved contract is the design authority; its PR #1146 and test-only shape PR #1147 remain unmerged. [GH-500 handoff](https://github.com/gyldlab/keld/issues/500#issuecomment-6381951609).

## 1. Goal & non-goals

Define the **physical corpus registration, fixture and runner contract** for the first macOS draw.io product proof under KELD. Keep the exact, already approved GH-587 logical `electron-apps-v0` family, but split it into three **kind-homogeneous** `keld.compat.corpus/v1` manifests, each with an independent committed-byte denominator and operation cells. Preserve the one existing GH-532/GH-566 manifest reader, KEL-74 scorer, approved GH-589 acceptance specification, and strict interpretation of observed versus merely expected outcomes.

Non-goals:
- This documentation change is **not** a product execution, a new v1 `app_run` operation, a new source-citation grammar, an authenticated Host-origin receipt format, an additional scorer/denominator owner, or a product-ID registration.
- Do not add product `pass`, `fail`, a completion percentage, a bundled app, or an installed-app claim based on source analysis, mocks, a placeholder page, a C15 test-local receipt or a successful Bun/Host broker request.
- Do not change GH-587's approved three-partition contract, GH-588/KEL-78 profile enforcement, #446/#449 Ready/window, #541/#552/#542 navigation-generation/channel security, #499 migration output, or the existing KEL-237 lifecycle corpus.
- No Windows/Linux/Strict lane extrapolation from macOS Legacy; no assumption that uncontained same-user Legacy evidence is tamper-proof against a hostile app.

## 2. Spec refs

- `docs/architecture/04-electron-compat.md` §§4,6, including the reviewed but **not yet merged** GH-587 architecture §6 correction in draft [#1146](https://github.com/gyldlab/keld/pull/1146).
- `docs/specs/kel74-compat-evidence-schema.md`, `crates/keld-compat/src/evidence.rs`, `src/evidence/scoring.rs`: closed `OperationKind` (`install`, `activation`, `primary_workflow`, `full_feature`), profile and output honesty.
- `docs/specs/gh532-first-proof-evidence-rules.md` and `docs/specs/gh566-corpus-manifest-owner.md`: accepted Electron 44.4.5 normative reference, `expected_verdict` + implementing ticket, exact committed manifest SHA-256, version-pinned doc snapshots, single parser/test ownership, red/fail/unknown admission and code-owned registration.
- **Approved GH-587** `docs/specs/gh587-electron-apps-v0-corpus-cells.md`, exact source approval [GH #587](https://github.com/gyldlab/keld/issues/587#issuecomment-6376877459): C01–C24, three partitions, actual Host and WebView generation, app source/patch, attempt census and bounded cooperative-app provenance. Draft test-only [#1147](https://github.com/gyldlab/keld/pull/1147) fixes duplicate receipt-cell admission, but `validate_committed_runs` still fails closed with `ProductRecordsNeedReceipt` until a genuine collector is qualified.
- Approved GH-589 scoring-design decision [#589](https://github.com/gyldlab/keld/issues/589#issuecomment-6382252527) and current [draft #1157](https://github.com/gyldlab/keld/pull/1157): actual Mac application workflow and immutable evidence are distinct later stages. #499 owns migrated app/configuration/patch. #588 owns actual Host effective-state witnesses; #541/#552/#542 own document generation. KEL-141 owns real installation.
- Current implementation seams: `crates/keld-compat/tests/support/corpus_manifest.rs` `REGISTRY`, `V1_SCHEMA`, `ADMITTED_PINS`, `Corpus::load`; `tests/support/corpus_runs.rs` `validate_committed_runs`; existing `tests/corpus_registry` suites; `crates/keld-compat/fixtures/window-corpus/{corpus.json,denominator.json}` demonstrates the v1 JSON structure but is a **showcase**, not the draw.io product corpus.
- Pinned two-source closure: `jgraph/drawio-desktop@2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e` contains the actual `drawio` gitlink to `jgraph/drawio@24b76c2cbd55d88e354042e8d329a2e4708972bc`; both version 31.7.0. Upstream `package-lock.json` resolves **Electron 44.5.1**, whereas the approved normative KELD oracle remains **44.4.5** at `694f45852a0f1726cd23bfd379854de489cccb65`. Never conflate the observed dependency version with the normative comparison pin.

## 3. Acceptance criteria (binary, each becomes a test)

| ID | Case / owner | Required observable and independent falsifier |
|---|---|---|
| M01 Single-kind schema / GH-566 | Exactly three physical `keld.compat.corpus/v1` manifests, with one top-level `kind` per manifest; a mixed install/activation/workflow file or per-cell kind override rejects |
| M02 Fixed identity / GH-500 | Logical report grouping `electron-apps-v0`; physical IDs exactly `electron-apps-v0-install`, `electron-apps-v0-activation`, `electron-apps-v0-primary-workflow`; an alias or unregistered product id cannot substitute |
| M03 Six cells / GH-500 | Install one, activation one, workflow four (`open`, `edit`, `save`, `close-unsaved-prompt`), each `(operation_id,oracle_id)` unique, named target/case and independent negative control; omitted/synthetic seventh or duplicate full `CellKey` rejects |
| M04 Canonical bytes / GH-532 | `artifact_digest: manifest_bytes` and denominator `corpus_sha256` = exact committed `corpus.json` bytes; mutate one byte or substitute Host/Bun/app-source SHA and validator rejects |
| M05 Pin and engine / GH-566 | Three physical manifests carry Electron v44.4.5 peeled commit, `engine.macos=wkwebview`, `platforms:[macos]`; wrong oracle prefix, revision, engine or added unsupported lane rejects |
| M06 Real source closure / #499 consumer | App source receipt/code registration proves desktop commit, actual matching webapp gitlink, original locked dependency tree, prepared application SHA and optional **one** reviewed behavioral patch/path/hunk census; missing webapp pin, self-consistent wrong source pair or second undeclared hunk rejects |
| M07 Version separation / #499/#589 | Report actual Host/KELD/Bun/WKWebView image/version/OS identities and actual installed Electron reference separately from the 44.4.5 normative documentation; claiming 44.4.5 for a measured 44.5.1 executable refuses |
| M08 Unknown vs genuinely red / GH-532 | An uncited, unexecuted/app-only cell has `expected_verdict: unknown` and no success/failure record; `fail` is admitted only with actual mapped RED conformance, applicable byte-checked normative Electron documentation and exactly one `implementing_ticket` or `intentional_divergence`; replacing uncertainty with fabricated fail/pass rejects |
| M09 Citation exactness / GH-532 | Every normative `pass`/`fail` uses already permitted immutable Electron docs `doc_citation` and byte-checked shared snapshot; citing arbitrary app JavaScript or inventing `source_receipt` field fails the closed parser |
| M10 Executable registered case / GH-566 | Every manifested `test_path` and `test_name` resolves to an actual compiled or Bun test selected by existing `Runner`/registered target; fictitious cases, a stub with no independent assertion, or a source path not in registry fail |
| M11 Real product observations / GH-589 | Real pinned macOS launch/Open/Edit/Save/dirty-close outcomes are backed by actual app page/model, OS file effect and prompt/window evidence; a placeholder renderer, console `save`, or test-owned imitation dialog cannot pass |
| M12 Trusted current-generation proof / GH-587/#588 | Product run receipt binds frozen verified policy, genuine effective state, RolePrincipal/current/reader-installed Host generation and active WebView/navigation/document identity at action time; stale G1, forged G2, missing producer, foreign page and app-printed receipt refuse |
| M13 Causal attempts / GH-589 | Every physical run retains stable attempt and full initial/failure/unknown/retry census, unique request-generation initiation and independently observed effect completion; remove an earlier failed attempt or attribute delayed G1 write to G2 => reject score |
| M14 No receipt bypass / existing corpus reader | Until real Host-origin+OS collector is qualified, nonempty product-panel committed records still return typed `ProductRecordsNeedReceipt`, regardless of a syntactically valid test-local ProductReceipt; replacing refusal with a naked JSON acceptance branch turns test RED |
| M15 No install fiction / KEL-141 | `install` remains `unknown` until actual built distribution and real OS installation/image verified; substituting `keld dev` or a compiled staged Host must not PASS |
| M16 No unauthorized product claim / GH-589 | #500 does **not** add IDs to `DOCUMENTED_COMMITTED_PRODUCT_CORPORA` or print percentages; later #589 X02-T6 adds independently reviewed code+docs+manifest+receipt/attempt gate and rejects whitelist-only, docs-only or missing named-reviewer admission |
| M17 Platform/profile truth / GH-588 | Real Mac explicit Legacy only, `legacy_sandbox_off` from existing owned mapping and genuine effective-state receipt; no Strict or Windows/Linux success and no same-user hostile attestation inference; self-labelled Strict/unverified-as-Legacy rejects |
| M18 Source-owner reuse / GH-500 | One GH-566 manifest parser/registry, one KEL-74 scorer, one GH-587 receipt consumer, same pin table and GH-588 Host-generation authority; duplicate parser, independent label map, new wire auth frame or migrated app special case under framework-generic scorer rejected |

An `expected_verdict` is not a predicted result. It is the exact state admitted by existing citation/test machinery. A test can PASS because it correctly asserts a pending implementation's fail; that is **not** a draw.io product PASS. The actual product result remains **UNKNOWN** when no real and admissible KELD observation exists. Existing issue #500's older wording assumed `expected_verdict:fail` for every noninstall cell before execution. That assumption is **not automatically valid** for app-specific uncited workflows under GH-532/566; reconcile the actual native citation/mapped-test evidence instead of fabricating a citation or weakening the schema.

## 4. Design

### 4.1 Atomic ownership, reuse and boundaries

| Atom (one owner) | Inputs -> output | First falsifier and relation |
|---|---|---|
| Complete source / #499 | Approved immutable app-id→desktop+webapp+lock/patch closure -> reproducibly prepared files | Alter upstream gitlink or hidden extra hunk; do not mint runtime authority |
| Static manifest / GH-566 + #500 | Code-owned registered product IDs, v1 parser, exact committed bytes -> 3 physical validated `Corpus` objects | Mixed kind, duplicate key, phantom target or altered byte refused; source digest distinct |
| Red/unknown citation / GH-532 | Electron 44.4.5 immutable documentation and actual conformance -> cited expected verdict | Forged/app-only citation or unjustified `fail` rejected; exact normative behavior separate from app workflow |
| Actual Host/GUI / GH-588 and GH-541/#552/#542 | Verified profile + active role/WebView/document and independent OS effect -> authenticated product-cell receipt | Child spoof, stale role/page or late G1 operation refuses; parser cannot invent this |
| Scoreboard / KEL-74 and GH-589 | Qualified physically kind-compatible records + denominator + attempts + reviewed registration -> honest per-partition score | Partial/cherry-picked/uncommitted score withheld; no global cross-kind percentage |
| Distribution / KEL-141 | Built package + installed image identity -> install | Development-staged artifact remains UNKNOWN |

This is **no runtime, public API, permission, ownership, IPC or process boundary change**. Reuse is mandatory: no new OperationKind, `app_run` enum, inline manifest parser, SHA helper, profile map or scorer. The v1 static `ManifestV1` accepts only `schema,corpus_id,scope,panel,kind,artifact_digest,engine,doc_snapshots,upstream,cells`. Draw.io-specific source closure, patch, runtime and attempt provenance are separately verified in the approved GH-587 receipt/collector—not smuggled into a closed v1 manifest.

### 4.2 Proposed files and physical manifest/denominator contract

After separate approval and registration of real target tests, use **one** `crates/keld-compat/fixtures/<physical-corpus-id>/` directory for each of:

| Logical first-proof family partition | Exact kind | Cell identities (proposed app-id `drawio-desktop`) | Initial disposition |
|---|---|---|---|
| `electron-apps-v0-install` | `install` | `drawio-desktop.install` | `unknown` until KEL-141 |
| `electron-apps-v0-activation` | `activation` | `drawio-desktop.activate` | `unknown` without real native acceptance |
| `electron-apps-v0-primary-workflow` | `primary_workflow` | `drawio-desktop.open`, `drawio-desktop.edit`, `drawio-desktop.save`, `drawio-desktop.close-unsaved-prompt` | `unknown` until applicable citation/test and real OS evidence |

Each directory holds a committed `corpus.json`, a matching `denominator.json` (`keld.compat.denominator/v1`) and only qualified evidence records in the existing owner-approved evidence layout. The physical corpus manifests use `panel:product`, one listed `kind`, `artifact_digest:manifest_bytes`, `engine:{macos:wkwebview}`, Electron 44.4.5 normative `upstream` and only actually cited version-pinned shared `doc_snapshots` pages. Each denominator carries exact full `CellKey` set and SHA-256 of *those actual committed manifest bytes*. Altered whitespace counts as an altered corpus digest. Do not store the app source revision as the corpus digest.

The only existing `REGISTRY` in `tests/support/corpus_manifest.rs` initially lists `LIFECYCLE_V0` and `WINDOW_V1` (**both showcase**) and keeps those identities unchanged. GH-500's later PR may add exactly three registrations **only after** mapped target source/tests, physical manifests/denominators and independent acceptance controls exist. Registry rows bind each exact `corpus_id`, `fixture_dir`, `Shape::V1`, `platforms:[Macos]`, and the actual paths with `Runner::Libtest` or `Runner::Bun`. A multi-operation real Mac runner may feed three partitions but **each corpus registration/mapped case and score is still kind-homogeneous**; do not create fake extra test targets to satisfy the registration check.

### 4.3 Exact citation / red-upgrade policy

The GH-532/GH-566 parser currently permits `doc_citation` **only** into the pinned Electron `docs/*.md` source, with literal quote, SHA-256, shared checked-out snapshot, and exact `oracle_id` version prefix. It cannot accept the draw.io upstream `src/main/electron.js` as a `doc_citation` even though that source informs the separate product-behavior oracle. Hence a real app-specific open/edit/save effect **does not automatically admit** an Electron-normative `pass`/ `fail` cell.

Before converting an app-only cell from `unknown`, obtain a **reviewed GH-532/GH-566 owner amendment** if the existing grammar cannot cite the actual normative behavior. Until then retain `expected_verdict:unknown` and no fabricated record. A source-pinned application receipt is supporting independent behavioral evidence; it is not a counterfeit Electron documentation quote. GH-500 does not invent a new citation key, parser branch or version/pin exception.

A proposed red-until-implemented `fail` cell must first have a real mapped conformance test that is RED for the cited normative behavior, an authentic pin snapshot, and exactly one canonical `GH-n` or `KEL-n` implementing ticket (e.g. #446/#449/#466/#478 where applicable). Keep those negative-test source locations and no-skips. When implementation genuinely becomes green and product observations qualify, the consuming issue updates that exact cell's `expected_verdict` and records in the *same reviewed change*; no silent bulk-flip of the denominator to make CI green. A genuine failed expected-pass test remains RED until its evidence/manifest is reviewed.

### 4.4 Execution and submission boundaries

#500 may compile the **real Mac product fixture/test harness** in its own approved implementation worktree and connect each registered test to its physical manifest; however the actual app workflow evidence and final claim belong to #589, which consumes the approved #587 receipt collector. #500 must not disable `ProductRecordsNeedReceipt`, write a synthetic `origin:host` receipt, bypass channel permission, or promote a screenshot/log to Host proof.

Test fixture input must be source-pinned valid draw.io XML with unique per-attempt shape content and a distinct wrong-document negative control; independently read back target bytes and reopen to verify persistence. A large case is valid only after measuring the **encoded wire size**, not extrapolating from an on-disk file size. The actual Mac dirty-close prompt must exercise Save/Cancel/Discard and failed/cancelled saves. Retain all attempt records, G1/G2 and navigation/document attribution, zero `will-attach-webview` events and both webRequest startup registrations (GH-589), not a selected later green retry.

The app is a **cooperating Legacy** test subject; uncontained Bun and macOS same-user writable files alone cannot authenticate against an actively malicious same-UID adversary. The independent collector/Host in-process+OS control makes only the approved bounded behavioral claim. All unqualified OS/profile/source/run combinations remain `unknown`.

## 5. Boundaries

**This GH-500 spec-only PR may edit:** `docs/specs/gh500-drawio-product-corpus-partitions.md` plus mechanically generated `llms.txt`/`llms-full.txt` if applicable. No production, current corpus fixtures, `tests/support`, KEL-74 scorer/registry, GH-587 spec, source lockfiles, Host/Core/Guard, IPC, other worktrees or CI workflows are touched.

**After exact-content human approval, separate implementation PR:** bounded changes under `crates/keld-compat/fixtures/electron-apps-v0-{install,activation,primary-workflow}/`, existing `tests/support/corpus_manifest.rs` registry and relevant `tests/corpus_registry/` with executable mapped tests. This is a **proposed scope**, not authorization to overlap GH-587's `tests/support/corpus_runs.rs` receipt owner or GH-589 scoring/publish owner. T3 real macOS Host/product acceptance remains separately gated. Do not update `src/evidence/scoring.rs` product whitelist or `docs/engineering/compat-scoreboard.md` under GH-500; GH-589's named-reviewer X02-T6 admission owns those after qualified evidence.

## 6. Tasks (ordered, each one reviewable PR)

- [ ] T0 — Publish this GH-500 *specification* in a managed worktree as OPEN/DRAFT/DO NOT MERGE, review exact v1 schema/pins/closed citation grammar, run relevant doc gates and independent source review, then obtain exact-content human approval before implementation.
- [ ] T1 — Add failing-first actual registered test/case mappings and negative controls for three physical partitions, preserving GH-532/GH-566 v1 test support and exact pin; no no-op/mocked product success. Resolve any app-only citation contract gap under its existing owner before claiming a cited `pass` or `fail`.
- [ ] T2 — Commit exact three distinct product manifests/denominators, digest checks, macOS platform declaration and registration with the existing GH-566 `REGISTRY`; install/cells remain honestly `unknown` where execution or normative citation is missing. No product registry/percentage.
- [ ] T3 — Reuse GH-499 approved source/config/patch and integrate the active #588 Host and #541/#552/#542 navigation receipt producers when available; run native Mac fixture with real application effects and complete attempt census. Preserve `ProductRecordsNeedReceipt` until fully authenticated producer and consumer are qualified.
- [ ] T4 — GH-589 (separate owner) performs final native product scoring, named reviewer code/docs registration and any claim. KEL-141 installer and Windows/Linux/Strict lanes remain future independent acceptance tasks.

## 7. Test plan

| AC | Test owner and independent negative control |
|---|---|
| M01–M03 | Existing `Corpus::load`/v1 registry parser + new three case mappings; mixed kind, duplicate/missing cell, fake test path/name and physical ID alias RED |
| M04–M05 | Existing GH-532/GH-566 pinned manifest/denominator validators; single manifest byte flip, wrong pin, wrong engine, undeclared Windows lane RED |
| M06–M07 | #499 prepared source/effective runtime identity fixture; change webapp gitlink, lock, allowed source hunk or actual executable version while keeping record self-consistent => admission RED |
| M08–M09 | Existing GH-532 citation/expected verdict cases; uncited app-only `fail`/`pass`, wrong quote/pinned source or `source_receipt` field RED |
| M10 | GH-566 registered targets/runner; fictitious test name, uncompiled case, invariant-free stub or wrong target path RED |
| M11 | GH-589 real Mac app/file/UI; open dialog but no loaded content, input without model mutation, save success without changed disk bytes, fake prompt RED |
| M12–M14 | #588 Host-origin and GH-587 test/collector; forged stdout, dead G1, mismatched G2 effective state, stale navigation, ambiguous delayed effect, missing failed attempt and committed product record without trusted receipt RED |
| M15–M16 | KEL-141 install and GH-589 product admission; dev stage claimed as installed, whitelist-only product registration and wrong cross-kind percentage RED |
| M17–M18 | GH-588 profile authority and static owner census; mislabeled Legacy as Strict, platform reuse, second parser/scorer/authority mapping or new receipt IPC frame RED |

The parser/harness test may execute under CI without a real draw.io result; it cannot manufacture the real Mac OS witness. Each fault control must actually turn the named test RED before GREEN restoration. No wall-clock sleeps as proof, no unbounded resource lifetime, no red-test deletion, no mock-only native acceptance, no return of `Result::Ok` without an independently observed file/UI/state effect. Preserve `ProductRecordsNeedReceipt` until the authenticated producer and collector are verified.

## 8. Review gates triggered

- Public API: **later** code registration/product corpus admission consumes existing KEL-74; independent GH-532/GH-566/KEL-74 review required. This spec edits no public API.
- Permission model: **later** Host/profile/renderer evidence consumes #588/#541; independent security review needed for real product acceptance. This spec grants no authority.
- Wire protocol: none; never invent a receipt IPC event.
- Unsafe: none.
- Dependency addition: none.
- Named independent reviewer and exact-head approval needed for the later GH-589 product registration, not satisfied by a general spec/source GO. Explicit human approval of *this spec* is required before its T1/T2 implementation. Keep DRAFT/DO NOT MERGE even when CI passes.

## 9. Perf impact

None from this documentation-only specification. No new runtime per-frame observer or local hot-path serialization is authorized. Real Mac corpus overhead/performance, if later measured, must name workload/clock/artifact and preserve semantic parity; no speed claim follows from the parser or Rust host architecture.

## 10. Open questions

**Human spec gate resolved:** the exact GH-500 *physical corpus implementation* plan was approved for implementation on 2026-10-24, at the source bytes and receipt cited in the header. This is not merge, public scoring, product-result, native OS or installer approval. The already-approved GH-587 three-kind design and GH-589 scoring specification are consumed unchanged; no new choice of single mixed-kind manifest is reopened.

**External acceptance blockers, with owners rather than unbounded questions:** GH-532/GH-566 approved means of citing app-only Open/Edit/Save beyond Electron-only `doc_citation`; genuine current G1/G2 Host profile/reader receipt from #588, GUI document generation from #541/#552/#542, real #446→#449 facade boot and #499 reviewed migration/one-edit patch, actual native Mac workflow and all-attempt evidence under #589, and KEL-141 installer for install. If a prerequisite has no implemented source/evidence, leave that cell `unknown` and the task blocked; do not invent a product PASS or silently waive the criterion.
