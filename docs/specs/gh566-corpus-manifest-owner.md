# Spec: shared corpus-manifest owner (X01-T4)

Status: approved
Linear: GH-566 (#517) · Owner: @0monish · Updated: 2026-10-08

## 1. Goal & non-goals

The approved governing spec,
[`gh532-first-proof-evidence-rules.md`](gh532-first-proof-evidence-rules.md) (merged in
#612, "gh532" below), fixes eight evidence rules for committed corpus manifests. It also
names their one owner (gh532 §4.2 rule 8). Today the only implementation is test-local to
the two KEL-237 lifecycle targets: `sha256_uri` is defined twice, the pin is a hard-coded
`44.3.0` assertion, and the admitted targets are two constants. Nothing parses a v1
manifest.

This spec decides how X01-T4 builds that owner and how it migrates the lifecycle tests
onto it without changing a committed byte. It also decides how #445 and #448 add v1
corpora. Each adds data, one registry entry and its own conformance tests. It adds no
new parser, digest helper, validator or validator test target.

Observable outcome: gh532 AC1–AC12, AC16 and AC17 (as amended by A1), plus this spec's
C1–C10, pass as tests in `crates/keld-compat/tests/`, each with its named negative
control. And
`git diff origin/main -- crates/keld-compat/fixtures/` is empty.

Non-goals:

- Every gh532 §1 non-goal and every item in the issue's Out of scope. That covers
  re-recording `electron-lifecycle-v0`, the KEL-77 fixture-set digest, any production
  keld-compat API or dependency, and the CLAUDECODE Bun reporter fix.
- Changing a gh532 rule, AC, field name or file location beyond the four recorded
  amendments A1–A4 (§2). This PR amends gh532 in place for each one, and each place is
  marked "amended by gh566 (#639)":
  - AC17's subject (A1);
  - the `platforms` cell field, in rule 8, the §4.2 example manifest and the owner
    sketch (A2);
  - the §4.5 callers (A3);
  - the §5 file list (A4).

  Anywhere else that this spec is more specific than gh532, it narrows a gh532 choice
  and says so.
- The product receipt schema and the KEL-78 evidence that decides a profile state. Both
  belong to X02-T5.
- Committing a v1 corpus, a doc snapshot or a `.gitattributes` rule. The first consumer
  does these (§4.4).
- Two items left outside this spec. One is the KEL-77 producer constant in `keld-runtime`
  (the second item parked from #637). The other is the private `workspace_root` copy in
  `electron_lifecycle.rs` (§4.2 D10).

## 2. Spec refs

- The parts of gh532 that this spec consumes without amendment:
  - §3 AC1–AC12 and AC16.
  - §4.1, its facts and atoms.
  - §4.2 rules 1–8, the v1 example manifest and the owner sketch, except the cell field
    that A2 adds.
  - §6 T3, §7 (test plan), and the §10 Q1 and Q2 draft decisions.
- Recorded gh532 amendments. This PR applies each one to gh532 in place, the same way
  gh532 amended the kel74 §4.1 `authority_profile` row. gh532's header and each changed
  passage say "amended by gh566 (#639)":
  - **A1 — the subject of AC17.** gh532 AC17 binds "the lifecycle evidence report". The
    frozen v0 report cannot carry a pending cell, so this spec binds AC17 to `FailSplit`
    and its `Display` (D11). C9 keeps AC17's strength: a census forces every v1 report
    to render its fail counts through `FailSplit`, and a negative control proves that a
    report which formats its own counts fails.
  - **A2 — a v1 cell field, `platforms`.** It adds one required field to the gh532
    rule 8 cell shape (D13, C10).
  - **A3 — the §4.5 migration unit.** Three including targets instead of two (D8, §4.5).
  - **A4 — the §5 X01-T4 boundary.** One added test target (§5).
- [`kel74-compat-evidence-schema.md`](kel74-compat-evidence-schema.md) §4.1–§4.3: the
  record, the denominator and `score()`. This spec consumes them only through the public
  `parse_evidence`, `parse_denominator` and `score`.
- `docs/engineering/compat-scoreboard.md`, rule 7 as added by #638: a report names a
  board's profile only through `Scoreboard::authority_profile()` and
  `AuthorityProfile::as_str`.
- `docs/architecture/04-electron-compat.md` §4 and §6. Unchanged, and this spec does not
  deviate from either.
- `crates/keld-compat/AGENTS.md`, `.agents/testing.md` and `.agents/test-layout.md`. The
  last supplies the exact-name inventory and the cohesion triggers.
- Issue #566 (the Agent Brief) and the comment parked there from #637
  (issuecomment-6051941996).

## 3. Acceptance criteria (binary, each becomes a test)

### 3.1 Adopted from gh532

gh532 AC1–AC12 and AC16 are criteria of this spec as written there, with their negative
controls. AC17 is a criterion with the subject amendment A1, which C9 enforces. §6
assigns each criterion to a task, and §7 assigns each one to a test. The issue's own
criteria and controls map onto them as follows:

| Issue criterion or negative control | Proved by |
|---|---|
| One manifest parser and one digest helper, used by the lifecycle tests | gh532 AC10 |
| The lifecycle manifest, denominator digest and records are byte-identical | gh532 AC11, plus C1 for the receipt-mapped cases |
| A cell whose oracle revision differs from its pin is rejected | gh532 AC2 and the AC11 control (D4) |
| `pass` without a citation, `fail` with neither key, and a red cell without a ticket are rejected | gh532 AC3 and AC4 |
| A cell naming a target not registered for its corpus is rejected | C3 |
| A removed, skipped or source-only mapped test fails admission | the live Rust and Bun case controls, moved byte-identical (D9) |
| One flipped byte in the lifecycle manifest | gh532 AC7 and AC11 |
| The 44.4.5 commit inside the 44.3.0 corpus | the gh532 AC11 control and AC2 |
| Deleting `implementing_ticket` from a red fixture cell | gh532 AC4 |
| A mapped Bun test changed to a skipped test | the live `(skip)` row, plus the mutation run in T2 (§7) |

### 3.2 Criteria for the decisions in §4.2

1. **C1 — Receipt cases stay live (D9).** Given the migrated tree, when
   `published_receipt_cases_remain_live_tests` runs on an OS, then every `mapped_cases`
   entry `<target>::<name>` in that OS's committed receipt is listed exactly once, as
   `<name>: test`, by
   `cargo test --offline -p keld-compat --test <target> -- --list`. It must also be
   absent from `-- --list --ignored`, so an ignored test, or a name that exists only in
   the receipt, is never admitted (F4). *Negative control:* the test fails if one
   `lifecycle_corpus` case is renamed, moved into a module (which gives it a `module::`
   prefix) or marked `#[ignore]`. `rust_case_listed` rejects a missing, duplicated or
   prefixed name.
2. **C2 — Every committed corpus is registered (D8).** Given
   `crates/keld-compat/fixtures/`, when the census runs, then the set of directories that
   hold a `corpus.json` equals the set of `REGISTRY` fixture directories. Corpus ids and
   directories are each unique. *Negative control:* each of these synthetic inputs is
   rejected: an unregistered directory, a registration whose directory is absent, and two
   registrations that share an id.
3. **C3 — Admitted targets are registered oracles (D8).** Given a corpus, when it is
   validated:
   - each cell's `test_path` equals a target path registered for that corpus;
   - each libtest target is `crates/keld-compat/tests/<target>.rs`, exists, and does not
     include `support/corpus_manifest.rs`;
   - each Bun target starts with `packages/`, ends with `.test.ts`, and exists.

   *Negative control:* each of these is rejected, with `UnregisteredTarget` or
   `InvalidTarget`:
   - a registration that drops the Bun target (its two Bun cells become unregistered);
   - registering `lifecycle_corpus`, because admission would recurse;
   - a path in another package (`crates/keld-host/tests/x.rs`);
   - a missing file.
4. **C4 — No silent duplicate keys (D3).** Given a v1 manifest whose `engine` or
   `doc_snapshots` object repeats a key, when it is parsed, then it is rejected with
   `DuplicateKey`, which names the field and the key. *Negative control:* the test fails
   if the duplicate-rejecting map is replaced by a plain `BTreeMap`, which keeps the last
   value (F3).
5. **C5 — Records agree with their cell (D6).** Given a run, when it is validated, then:
   - each record names a manifest cell by its full `CellKey`: `operation.id` together
     with `operation.oracle.id`;
   - each cell has at most one record in the run. This reuses `score()`'s
     `EvidenceError::DuplicateCell` (F11) instead of adding a second uniqueness check;
   - a record for a platform the cell does not declare has `result: unknown` (C10);
   - each record has the manifest `kind`;
   - each record's `result` is the cell's `expected_verdict` or `unknown`, and only
     `unknown` for an uncited v1 cell;
   - `waived` is rejected.

   *Negative control:* each of these is rejected.
   - On the committed v0 records (T2): a `pass` record for the divergence cell, a `fail`
     record for a `pass` cell, a second record for one cell, and a `waived` record.
   - On in-test v1 corpora (T3): a `pass` record for a red cell, and a `pass` record for
     an uncited cell.

   *Positive control (T3):* two cells that share an `operation_id` but have different
   oracle ids each take their own record.
6. **C6 — Product records fail closed (D7).** Given a registered product-panel corpus
   with files under `evidence/`, when the registry test runs, then it fails with
   `ProductRecordsNeedReceipt`. This holds until X02-T5's receipt reader supplies a
   `ProductReceipt`. *Negative control:* a synthetic product corpus with one record is
   rejected, and it is never scored as a harness run. Given an in-test `ProductReceipt`
   over cell A only and passing records for cells A and B, `validate_product_run` fails
   with `UncoveredProductRecord` before scoring. *Negative control:* without the
   set-equality check, B scores as a pass (D7).
7. **C7 — The report's authority line is derived (D10).** Given the migrated report,
   when it renders, then its authority line names
   `board.authority_profile().map(AuthorityProfile::as_str)`, which must be one shared
   value across the three platform boards. The render refuses on `None` or on a
   disagreement. This closes item 1 of the comment parked on #566 from #637. *Negative
   control:* a board scored from two records with different profiles makes the line
   helper refuse, and an all-`unverified` board renders `unverified`, not the committed
   bytes.
8. **C8 — The denominator agrees with the manifest (D3).** Given a corpus, when it is
   parsed, then:
   - the denominator's `corpus_id` equals the manifest's;
   - its cell set equals the manifest's cell set exactly, where a cell is a KEL-74
     `CellKey` (`operation_id`, `oracle_id`);
   - manifest cells are unique by `CellKey`.

   *Negative control:* each of these is rejected with `DenominatorMismatch`: a
   denominator missing one manifest cell (for example the only `fail` cell), one with an
   extra cell, one with a different `corpus_id`, and a manifest that repeats a
   `CellKey`. Each mutation recomputes the denominator digest in memory, so it reaches
   this check rather than `DigestMismatch` (§7, "Negative-control routing").
9. **C9 — Every v1 report renders its fail counts through `FailSplit` (D11, A1).**
   Given the keld-compat test sources, when the census runs, then the label strings
   `Pending implementation` and `Intentional divergence`, and every `.failed()` call,
   appear only in the owner and in the frozen v0 report. The v0 report,
   `tests/lifecycle_evidence_report.rs`, is admitted by exact path until KEL-237
   re-records the corpus. *Negative control:* each of these synthetic report sources is
   rejected with `ReportBypassesFailSplit`: one that renders `board.failed()`, and one
   that writes either label itself. A synthetic source that renders through `FailSplit`
   is accepted.
10. **C10 — Declared platforms (D13, A2).** Given a v1 cell, when it is parsed, then its
    `platforms` is a non-empty set of distinct `platform_token` values, each also in
    its registration's `platforms`. When admission runs on a host platform:
    - each cell that declares the host platform has exactly one passing mapped case
      there;
    - each cell that does not declare it appears in the admission report's `unknown`
      list, never as passed and never silently skipped.

    A record for an undeclared platform has `result: unknown`. *Negative control:*
    each of these is rejected:
    - an empty `platforms` (`EmptyPlatforms`), or a repeated, unknown or unregistered
      entry (`InvalidPlatforms`);
    - a declared platform whose case is missing because a `cfg` gate compiled the test
      out (`CaseNotAdmitted`);
    - a declared platform whose Bun case reports `(skip)` from `skipIf`
      (`CaseNotAdmitted`);
    - a `pass` record for an undeclared platform (`RecordRule`).

    *Positive control:* on an undeclared host, the cell is in the report's `unknown`
    list and admission passes.

## 4. Design

### 4.1 First-principles and reuse decision

No boundary change, as gh532 §4.1 already establishes. The change is test-only. It moves
no handle ownership, crash ownership or principal minting, and it adds no production
dependency, public API or wire format. `sha2` stays in `[dev-dependencies]`.

Live facts. They hold at origin/main `c1673d83` unless the entry says otherwise, and they
are additional to gh532 §4.1:

- **F1.** `crates/keld-compat/tests/support/` does not exist yet. `keld-ipc` already
  includes `tests/support/*.rs` through `#[path]`, with an inner `#![allow(dead_code)]`
  justified by consumers that use different subsets (`receiver_corpus.rs`). Nothing under
  `tests/support/` is built as its own target.
- **F2.** The macOS and Linux receipts each list 9 `test.mapped_cases`, 6 of them
  `lifecycle_corpus::*`. The Windows receipt lists 10, adding the `#[cfg(windows)]` case
  `electron_lifecycle::electron_main_retains_decimal_diagnostic_compatibility`.
  `assert_receipt` checks only that the frozen bytes contain 5 of these names. Nothing
  checks that the named cases still exist.
- **F3.** Probe against the workspace pins (serde 1.0.228, serde_json 1.0.150) on
  2026-10-08. A derived struct rejects a repeated field (`duplicate field 'upstream'`).
  A `BTreeMap<String, String>` field silently keeps the last of two equal keys.
- **F4.** Probe with rustc 1.97.1. Libtest `--list` prints one `<path>: test` line per
  test, `#[ignore]` tests included, and nested tests carry their `module::` prefix.
- **F5.** The CI router routes a `packages/<pkg>` change to every crate whose sources name
  that directory (`ts_package_consumer_packages` in `tools/ci_changes.sh`). Edges between
  crates come only from cargo metadata. keld-compat does not depend on keld-host, so a
  change to a keld-host test does not re-run keld-compat's validator.
- **F6.** `.gitattributes` is `* text=auto eol=lf`. An LF blob is checked out unchanged on
  all three OS, and the live digest test passes on the Windows lane (receipt
  `windows-x86_64`). CRLF line endings are normalised to LF when a file is committed. A
  lone CR makes git treat the file as binary, so it is kept as-is.
- **F7.** Fetched 2026-10-08 at the pin `694f4585`: `docs/api/app.md` (78,086 bytes,
  digest `sha256:49238ddf…`, which is gh532's example value) and
  `docs/api/browser-window.md` (63,122 bytes, `sha256:49061d5e…`). Neither contains a CR
  byte or a `mermaid` fence. `tools/mermaid_docs.rs` scans every tracked `*.md`, a
  snapshot included.
- **F8.** PR #638 (#637, gh532 T2; open at `86304945`) adds `AuthorityProfile::Unverified`,
  `const fn AuthorityProfile::as_str` and
  `Scoreboard::authority_profile() -> Option<AuthorityProfile>`.
- **F9.** KEL-74's `parse_platform`, `parse_arch`, `parse_verdict`, `parse_panel`,
  `parse_kind` and `parse_evidence_uri` are private, and so are `Panel::as_str` and
  `OperationKind::as_str`. `parse_denominator` exposes only the typed `Panel` and
  `OperationKind`. The report test already mirrors three of these maps
  (`platform_token`, `arch_token` and `verdict_token`).
- **F10.** Open PR #609 edits four `assert_receipt` lines in
  `lifecycle_evidence_report.rs`. The overlap is textual only.
- **F11.** `score()` returns `EvidenceError::DuplicateCell` when two records name one
  `CellKey`. It ignores, without rejecting, a record whose cell is outside the
  denominator or whose `kind` differs (`evidence.rs` `score`, rustdoc and body).
- **F12.** Probe with git 2.51.0 on 2026-10-08, using a snapshot-like path under
  `text=auto eol=lf`. `git hash-object --path=<path> <file>`, which applies the
  attribute filters, differs from `git hash-object --no-filters <file>` for CRLF bytes.
  The two are equal for LF bytes and for a lone CR. A CRLF page therefore matches its
  digest in the working tree, then fails after commit and re-checkout, for example in
  CI.
- **F13.** #445 and #448 are macOS-only first-proof cells, and they record Windows and
  Linux as `unknown`.

Decision atoms. Each is falsified by its own observable:

| Atom | Owner | Input → output | Failure mode | Observable |
|---|---|---|---|---|
| Owner shape | `tests/support/corpus_manifest.rs` | sources → one parser and one digest helper | a second copy drifts | gh532 AC10 |
| Digest | `sha256_uri` | bytes read → `sha256:` + hex | normalised or re-serialised bytes | gh532 AC7 and AC11 |
| Shape and parse | `Registration::shape` and `V0_FROZEN` | registration + bytes → v0 or v1, with a denominator that agrees | a manifest chooses its own rules; a duplicate key; a cell missing from the denominator | gh532 AC11, C4 and C8 |
| Pin | the one `ADMITTED_PINS` table, scoped per entry | upstream, `oracle_id` and record revision → consistent | mixed pins in one corpus | gh532 AC1, AC2 and AC11 |
| Platforms | the cell `platforms` field, bounded by the registration | host platform → admitted, or listed `unknown` | a macOS-only cell fails on Linux, or an unrun lane reads as pass | C10 |
| Citation and snapshot | `doc_citation` and `doc_snapshots` via one read seam | cell + pinned page → quote found | a fabricated quote with a matching digest | gh532 AC3 and AC16 |
| Verdict keys | the v1 cell rules | cell → `pass`, `fail` + one key, or `unknown` | pending work read as ▲; an uncited `pass` | gh532 AC4–AC6 |
| Record results | the run validator | record + cell → admitted | a `pass` record scored on a red cell | C5 |
| Authority | the run validator | panel or receipt state → one label | `unverified` relabelled | gh532 AC8 and AC12, C6 and C7 |
| Engine | the manifest `engine` map, or the v0 facade token | platform → identity | mixed engines | gh532 AC9 |
| Registry | the `REGISTRY` code | `test_path` → an admitted oracle | a self-admitted, recursive or unrouted target | C2 and C3 |
| Admission | the libtest and Bun parsers, plus `check_admission` | runner output on the host → exactly one passing case for each declared cell | a removed, skipped, cfg-gated or source-only test | the live controls and C10 |
| Pending split | `Run::fail_split` and the report census | manifest key + `fail` record → two counts, rendered only through `FailSplit` | lumped into ▲; a report that counts by itself | gh532 AC17 and C9 |
| Frozen migration | the `V0_FROZEN` table and the receipt guard | committed bytes and names → identical and live | drift, or orphaned receipt cases | gh532 AC11 and C1 |

Edges between atoms are explicit, never hidden:

- Pin, Shape and parse, Verdict keys, Citation and snapshot, and Registry all consume
  Digest. `Corpus::parse` checks the digest first, on the raw bytes (D2). Every
  negative control that mutates the manifest for another atom therefore recomputes the
  denominator digest in memory, so it reaches its own check (§7, "Negative-control
  routing").
- Citation and snapshot consumes Pin, because the commit is part of both the URL and the
  snapshot path.
- Admission and Record results consume Platforms. The host platform decides which cells
  are admitted and which are listed `unknown`.
- Record results, Authority and Engine consume the parsed `Corpus`, which carries the
  digest, pin and cells. Their tests therefore start from a valid corpus and mutate one
  record field.
- Pending split consumes only records that already passed validation. The types enforce
  this: `fail_split` exists only on the `Run` value that validation returns.

Reuse: KEL-74's public parsers and `score()` are reused unchanged, and #638's `as_str`
and `authority_profile()` as soon as #638 lands. The live `rust_case_passed`,
`bun_case_passed` and runner command lines move unchanged. The `#[path]` include follows
the keld-ipc convention. Nothing is rewritten. The X01-T4 change consolidates eight
existing test-local definitions into the owner (D10).

One option is rejected: the GH-508 Rust lexer (`crates/keld-cli/tests/support/rust_source_scan.rs`) for the
gh532 AC10 census. Including it across crates is not re-routed by CI (F5), and its scan
inputs and exclusions belong to GH-508. Compatibility fallback: the v0 facade for
`electron-lifecycle-v0` (gh532 §4.5). Performance claim: none.

### 4.2 Decisions

The owner delegated these architecture decisions. Each one records the chosen option,
the rejected alternatives and a falsifier.

**D1 — Owner module and API shape.** The module lives where gh532 rule 8 puts it,
`crates/keld-compat/tests/support/corpus_manifest.rs`. Each consumer includes it with
`#[path = "…/support/corpus_manifest.rs"] mod corpus_manifest;` and the right relative
prefix. It contains no `#[test]`.

- **Parsing.** Parsing yields one validated `Corpus` value, and every later step consumes
  it: record runs, admission and the split. Nothing re-parses manifest bytes.
- **Errors.** Every check returns `Result<_, CorpusError>`. `CorpusError` is a
  `Debug + PartialEq + Eq` enum with one variant per rejection the criteria name. Its
  hand-written `Display` names the corpus id, the cell's `operation_id`, the violated
  gh532 rule or criterion, and the fix. It has no `KELD-*` code, because it is
  test-only. Negative controls match the variant with `matches!`.
- **Consumers.** Three targets include the module: `lifecycle_corpus.rs`,
  `lifecycle_evidence_report.rs` and the new `corpus_registry/main.rs` (D8). An oracle
  target never includes it (C3).
- **Cohesion exception.** The module will pass the `.agents/test-layout.md` 400-line /
  16 KiB support-module trigger. This is a reviewed exception:
  - owner: @0monish, at this spec's approval;
  - reason: gh532 rule 8 requires one module;
  - tracking: #566;
  - review condition: a non-corpus consumer of execution admission, or a module over
    1,500 lines. In either case admission moves to its own support module.

  T2 met the condition twice, and each time it moved a cohesive responsibility that
  has its own invariant (amended in T2, #640):
  - execution admission, to `tests/support/corpus_admission.rs`;
  - the owner and fixture censuses (gh532 AC10, C2), to
    `tests/support/corpus_census.rs`.

  The owner keeps manifest parsing, the digest, the pin table, the cell rules, the
  registry and record runs. Admission fails closed on a cell that maps no registered
  target (`check_cells`). A new crossing of the condition moves another such
  responsibility; it never splits one arbitrarily.

Rejected alternatives:

- A public `src/` module (gh532 §4.3).
- `#[test]` functions inside the support module. Each would run once in every including
  target, under that target's name.
- Panicking asserts as the API, which is the live style. A negative control that only
  expects a panic also passes on an unrelated panic.
- A test-support crate. It would add a workspace member, edit the shared `Cargo.toml` and
  add a crate edge, all for one module.
- Several modules, which conflicts with gh532 rule 8.

*Falsifier:* a consumer needs a manifest fact that `Corpus` does not expose, and can get
it only by re-parsing the bytes. The answer is to extend the owner, not to add a second
parser.

**D2 — Exact-bytes digest.** `sha256_uri(bytes)` returns `sha256:` plus the lowercase hex
SHA-256 of exactly the bytes the test read from the checkout. That is the gh532 sketch,
moved unchanged.

- No JSON parse, re-serialisation, line-ending or whitespace normalisation, or BOM
  stripping.
- The helper never reads a file. It serves the manifest digest, `quote_sha256`, the
  snapshot digests and the receipt `evidence_uri`.
- The owner's `check_manifest_digest` asserts that the computed digest equals the
  denominator's `corpus_sha256`. It then runs the live one-byte mutation control
  unchanged: the first `e` becomes `E`, and the digest must differ.
- `artifact_digest: manifest_bytes` (gh532 rule 5) selects this one meaning. v0 implies
  it.
- `Corpus::parse` runs its checks in a fixed order:
  1. digest, on the raw manifest bytes;
  2. parse and shape (D3);
  3. pin (D4);
  4. denominator agreement (C8);
  5. cell keys and platforms (D6, D13);
  6. citations and snapshots (D5);
  7. registry and targets (D8).

  The digest comes first because it needs no parse. A one-byte flip therefore fails as
  `DigestMismatch`, even when the flip also breaks a later rule.

Rejected alternatives:

- Hashing re-serialised JSON. That changes the frozen digest, and it gives two byte forms
  one digest, which contradicts gh532 AC7.
- Normalising CRLF in the test. That passes on bytes that differ from the committed blob.
- The KEL-77 length-framed set digest. It exists for sets of several files. v1 binds each
  snapshot through its `doc_snapshots` digest inside the manifest bytes (gh532 rule 2),
  so the framing pitfall recorded at `docs/agents/learnings.md:118` does not apply.
- A `-text` attribute now. F6 shows that LF blobs already round-trip, and a CRLF page
  fails closed instead (D5).

*Falsifier:* on some CI OS, the digest of the bytes read differs from
`git show origin/main:<path> | shasum -a 256` for an unchanged file. Add a path-scoped
`-text` attribute; never normalise in the test.

**D3 — Parsing and shape admission.** The registration, which is code, declares the
shape. A manifest cannot choose its own.

- `Shape::FrozenV0` is admitted only when the registration's `corpus_id` is
  `V0_FROZEN_CORPUS_ID` (`electron-lifecycle-v0`). Any other id fails with
  `V0ShapeNotAdmitted`.
- `Shape::V1` parses `ManifestV1`. It requires
  `"schema": "keld.compat.corpus/v1"`: a missing field is a parse error, and any other
  value fails with `UnknownSchema`.
- The manifest's `corpus_id` must equal the registration's.
- Both shapes are derived structs with `deny_unknown_fields`. Serde rejects both an
  unknown field and a repeated struct field (F3). The `engine` and `doc_snapshots`
  objects deserialize through a `UniqueMap`, which rejects a repeated key with
  `DuplicateKey` (C4).
- Both shapes lower into one `Corpus`. Digest, denominator, pin, registry and admission
  are therefore written once. The facts v0 lacks (the engine token and the digest
  meaning) come from `V0_FROZEN` constants, never from the bytes.
- The manifest's `panel` and `kind` strings must equal `panel_token` and `kind_token`
  of the typed `Panel` and `OperationKind` that `parse_denominator` returns. No
  existing mapping can be reused. KEL-74's own `as_str` and `parse_*` are private
  (F9), and making them public is a public-API change outside X01-T4's boundary
  (gh532 §5). The denominator bytes are parsed only by `parse_denominator`.

  These two maps are therefore new, and this module owns them. They join the three
  maps moved from the report test (`platform_token`, `arch_token` and
  `verdict_token`), so the test tree holds exactly one copy of each. Each is an
  exhaustive `match`, so a new KEL-74 variant breaks compilation instead of passing
  silently.

  Tracking: #566 records all five as one residual. The T2 PR opens a follow-up GitHub
  issue, linked from #566. Its removal condition is that KEL-74 publishes `as_str` for
  `Panel`, `OperationKind`, `Platform`, `Arch` and `Verdict`, following #638's
  `AuthorityProfile::as_str`. The owner then calls those methods and deletes its five
  maps.
- The denominator must agree with the manifest (C8), keeping both live lifecycle checks.
  Its `corpus_id` equals the manifest's. Its cell set equals the manifest's cell set
  exactly, where a cell is the KEL-74 `CellKey` (`operation_id`, `oracle_id`). Manifest
  cells are unique by `CellKey`. A cell dropped from the denominator would make
  `score()` ignore its records, so a missing cell, an extra cell and a different id
  each fail with `DenominatorMismatch`.

Rejected alternatives:

- Choosing the shape by whether `schema` is present. Deleting `schema` would then
  downgrade a v1 manifest to the v0 rules.
- Rewriting the lifecycle manifest as v1. That changes its bytes (gh532 AC11).
- Separate v0 and v1 validators, which gh532 §4.3 rejects as "two owners".
- Accepting the last value of a repeated key, which leaves the meaning ambiguous.

*Falsifier:* a v1 registration whose bytes omit `schema` is accepted.

**D4 — Pin consistency.** The admitted pins are code, in one table, `ADMITTED_PINS`.
Each entry carries a scope:

| Pin | Scope |
|---|---|
| `44.3.0` @ `07e460719c75b2ec5ee4893f7d2192ef31c7b8c2` | `PinScope::FrozenV0`, for `V0_FROZEN_CORPUS_ID` only |
| `44.4.5` @ `694f45852a0f1726cd23bfd379854de489cccb65` | `PinScope::V1` |

The manifest `upstream` pair must equal the pair of one entry, version and commit
together, whose scope admits the registration's shape and id. Otherwise it fails with
`UnadmittedPin`. No other copy of a pin exists.

`Pin` derives four strings, and they are never retyped:

| Method | Value |
|---|---|
| `oracle_prefix()` | `electron-v<version>.` |
| `oracle_revision()` | `electron-v<version>@<commit>` |
| `doc_blob_prefix()` | `https://github.com/electron/electron/blob/<commit>/` |
| `snapshot_dir()` | `doc-snapshots/<commit>/` |

Each cell's `oracle_id` must start with the prefix and have a non-empty remainder. Each
record's `operation.oracle.revision` must equal the revision. Both fail with
`PinMismatch`, which names what it found and what it expected.

This replaces the hard-coded `44.3.0` assertions. The v0 `upstream.app_docs` must start
with `doc_blob_prefix()`. That is stricter than the live `contains(commit)`, and the
frozen bytes satisfy it.

The lifecycle receipt checks and the v0 report read the pin from the parsed `Corpus`
(`corpus.pin()`), never from a constant. This matches D1, where every step consumes
the parsed value.

Rejected alternatives:

- A pin field in the registration, which copies manifest data.
- Separate v0 and v1 pin constants. That is two copies of one admission rule.
- Comparing only the version, or only the commit.
- Checking `oracle_id` with a pattern that ignores the commit.
- Per-corpus hard-coded assertions, which is the live approach.

*Falsifier:* the issue's mutation is accepted. That is a record revision of
`electron-v44.3.0@694f4585…`, or a cell `oracle_id` of `electron-v44.4.5.app.quit-void`,
inside the 44.3.0 corpus.

**D5 — Citation, snapshot storage and verification (gh532 AC3, AC16).**

*Storage*, as gh532 rule 2 places it:

- The snapshot path is `<fixture dir>/doc-snapshots/<electron_commit>/<page path>`.
- Its bytes are the raw upstream bytes from
  `https://raw.githubusercontent.com/electron/electron/<commit>/<page path>`, verified by
  the reviewer command in gh532 rule 2.
- The change that first cites a page commits that page. The tree holds cited pages only.

*Verification* uses one read seam: `Corpus::parse` takes
`read_snapshot: &dyn Fn(&str) -> io::Result<Vec<u8>>`, keyed by the path relative to the
fixture directory.

- Committed corpora pass `std::fs::read` over the fixture directory. The path is joined
  one `/`-separated component at a time, which is safe on Windows.
- gh532 AC16 cases pass an in-memory map (gh532 §5). `NotFound` maps to
  `MissingSnapshotFile`. Any other error is a typed I/O error and is never swallowed.

The page path is taken from `url`:

1. The URL must start with `doc_blob_prefix()` followed by `docs/`.
2. The page path is everything after `doc_blob_prefix()`, so it begins with `docs/`
   (for example `docs/api/app.md`). It runs up to an optional `#anchor`, and any `?` is
   rejected.
3. Every segment is non-empty, is not `.` or `..`, and uses only `[A-Za-z0-9._-]`. The
   page path ends in `.md`.
4. An anchor, if present, is a non-empty `[A-Za-z0-9_-]` string. It is used for
   navigation only and is never checked against the page.

The checks then run in gh532 rule 2 order:

1. The page has a `doc_snapshots` entry (`MissingSnapshotEntry`).
2. The snapshot file exists.
3. Its digest matches the entry (`SnapshotDigestMismatch`).
4. `quote` is a byte-exact substring of the page (`QuoteAbsent`, which names the cell
   and the page).
5. Only then, `quote_sha256` matches.

Each page is read and digest-checked once, then searched for each cell that cites it.

Further rules:

- `quote` is non-empty.
- `doc_snapshots` keys are exactly the set of cited pages. An extra key fails with
  `UncitedSnapshotEntry`.
- A cell with `expected_verdict: unknown` may carry a citation. If it does, the citation
  is verified the same way.
- `doc_citation` denies unknown fields. A "source receipt" citation kind is therefore
  rejected, and such cells stay `unknown` (the gh532 §10 Q1 draft decision).

Rejected alternatives:

- A list of `include_bytes!` snapshots in the registration. Its path string and its
  include path could diverge, which would let a snapshot filed under another commit
  satisfy this pin.
- Fetching from the network in CI (gh532 §4.3).
- A sidecar digest file per snapshot, which would be a second digest source.
- Walking `doc-snapshots/` for stray files. A stray file satisfies no check, because the
  lookup is keyed by the manifest commit and the cited page. YAGNI.
- Verifying anchors, which would re-implement GitHub's heading slugs. The quote already
  binds the sentence to the page.
- A copy of the whole docs tree.

*Falsifiers:*

- A fabricated quote with its correct `quote_sha256` passes.
- A page filed only under `doc-snapshots/07e46071…/` satisfies a cell pinned at
  `694f4585`.
- A `..` segment reads outside the fixture directory.

Two outcomes are known and fail closed:

- **A CRLF page.** A cited page whose raw bytes contain CRLF matches its digest in the
  working tree, so it passes locally. It is normalised when committed (F6, F12), so it
  fails after re-checkout, in CI. The owner closes that gap for committed corpora:
  `Corpus::load` requires that `git hash-object --path=<snapshot path> <file>` equal
  `git hash-object --no-filters <file>`. Otherwise it fails with
  `SnapshotWouldBeNormalised`, which names the page and the fix: a `-text` attribute
  scoped to that path, in that consumer's PR. *Negative control:* CRLF bytes written to
  a temporary file and checked under a snapshot path are rejected. LF bytes are
  accepted.
- A cited page with a `mermaid` fence would be held to Keld's diagram policy (F7). The
  consumer then scopes `tools/mermaid_docs.rs`, which is a CI-tool change with its own
  review. Neither page the first consumers cite has one (F7).

**D6 — Cell keys and record results (gh532 AC4–AC6, C5).** Under the v1 cell rules
(gh532 rules 3 and 4):

- `expected_verdict` is one of `pass`, `fail` or `unknown`. Strings map through the
  owner's single `verdict_token`, and `waived` is not admitted.
- `pass` needs `doc_citation` and neither key. `fail` needs `doc_citation` and exactly
  one key. `unknown` needs neither key.
- `intentional_divergence` is non-empty after trimming.
- `implementing_ticket` matches `(GH|KEL)-[1-9][0-9]*`. This narrows gh532's "decimal
  digits" to one canonical spelling per ticket, so the AC17 key list deduplicates.
- `negative_control` and `test_name` keep the live rules: non-empty, and a single line.
- For v0 cells, the live KEL-237 rules stay unchanged. `pass` has no divergence; `fail`
  has a non-empty divergence; anything else is rejected.

For a record (C5), `result` equals the cell's `expected_verdict`, or `unknown` for an
unrun lane (gh532 rule 4). An uncited v1 cell's record is `unknown` (gh532 AC6). A
`waived` record and a record that carries a waiver are rejected.

This generalises the live `expected_verdict()` table in the report test, and that table
is removed. gh532 AC5's "the mapped test asserts the cited behaviour" stays a review
duty, because no mechanical oracle exists for it.

Rejected alternatives:

- Keeping the hard-coded table. It mirrors the manifest, which gh532 AC11 already pins
  by digest.
- Admitting `waived`, which no manifest field owns.
- Accepting `GH-0445` beside `GH-445`.

*Falsifier:* a `pass` record on a red cell counts toward `Scoreboard::passed`.

**D7 — Records and authority labels (gh532 AC2, AC7, AC8, AC9, AC12; C6).**

A run is the set of records for one `(platform, arch)`. A slice that mixes runs fails
with `MixedRun`. The owner has two run validators:

- `Corpus::validate_harness_run(records, as_of)`, for the showcase panel;
- `Corpus::validate_product_run(&ProductReceipt, records, as_of)`, for the product panel.

Each refuses the other panel. Both call `score(corpus.denominator(), records, as_of)`
once. That reuses KEL-74's `DuplicateCell` for uniqueness (F11), wrapped as
`CorpusError::Evidence`. The returned `Run` keeps the `Scoreboard`, so reports and
`fail_split` consume it and never score twice. `as_of` stays explicit (gh532 §7). The
registry test passes one committed constant: v1 records carry no waiver (D6), so the
date changes no result.

`score()` ignores, without rejecting, a record outside the denominator or of a foreign
kind (F11). The owner therefore keeps its own checks for those two cases. Both run
validators apply the same record checks:

- C5 membership, kind and result, plus the C10 rule that a record for an undeclared
  platform is `unknown`;
- `artifact.sha256` equals the corpus digest (gh532 AC7);
- `operation.oracle.revision` equals `oracle_revision()` (gh532 AC2);
- `revisions.engine` is the platform's token, then `@`, then a non-empty revision with no
  whitespace and no `@` (gh532 AC9). The token comes from the v1 manifest `engine` map,
  whose keys are `platform_token` values. For v0 it comes from `V0_FROZEN.engine_token`
  (`headless-lifecycle-conformance`). A platform absent from the map is rejected.

Labels then follow the panel:

- **Harness runs** need exactly `AuthorityProfile::LegacySandboxOff` (gh532 rule 6 and
  the §10 Q2 draft decision). Two constants hold it. `V0_FROZEN.harness_profile` is fixed
  for the frozen lifecycle records. `HARNESS_PROFILE` governs v1 harness corpora. If the
  owner reads Q2 the other way, the Q2 falsifier relabels new harness corpora
  `unverified` while `electron-lifecycle-v0` stays frozen. That changes only
  `HARNESS_PROFILE`, and the frozen records stay admitted.
- **Product runs** follow gh532 rule 6. `ProductReceipt { state: ProfileState, cells }`
  is the owner's minimal view of a run receipt. `cells` holds the KEL-74 `CellKey`s the
  run covered.

  | `ProfileState` | Required label |
  |---|---|
  | `Unverified` | `AuthorityProfile::Unverified` |
  | `Legacy` | `LegacySandboxOff` |
  | `Strict` | `StrictBun` |

  Each receipt cell needs exactly one record (`MissingProductRecord`, so the run is not
  dropped), and no record may name a cell outside the receipt (`UncoveredProductRecord`,
  checked before scoring). The record `CellKey` set must equal the receipt's `cells` set
  exactly. Otherwise a manifest cell the run never covered could still be counted as a
  pass, because it belongs to the denominator. *Negative control:* a manifest with cells
  A and B, a receipt covering only A, and passing records for both. The check must
  reject it with `UncoveredProductRecord`, and removing the check lets B score as a pass. A wrong label fails with `LabelMismatch`, which names both the receipt
  state and the record label (gh532 AC12). X02-T5's receipt parser builds the view. It
  owns the receipt schema and the legacy-declaration and strict-archive evidence that
  decides the state.
- **The registry test (D8)** validates committed `evidence/*.json` as harness runs only.
  A product corpus with committed records fails with `ProductRecordsNeedReceipt` until
  that reader exists (C6).

Rejected alternatives:

- Defining the product receipt schema here, which belongs to X02-T5.
- Inferring a product label from the panel, which drops the receipt state.
- Accepting `unverified` on harness records, which contradicts gh532 §10 Q2 as drafted.

*Falsifier:* X02-T5's receipt cannot be reduced to one KEL-78 state plus the cells it
ran. In that case the view is amended; no second validator is added.

**D8 — Registry, targets, coverage and census.** The registry is
`pub const REGISTRY: &[Registration]` in the owner. X01-T4 ships it as
`[LIFECYCLE_V0]`. A `Registration` has five fields:

- `corpus_id`;
- `fixture_dir`, relative to `crates/keld-compat`;
- `shape`;
- `platforms: &[Platform]`, the lanes this corpus may declare (D13);
- `targets: &[TestTarget]`. A `TestTarget` is a
`path` plus `Runner::Libtest { target }` or `Runner::Bun`. This is the gh532 rule 8 data
registry of admitted targets. It is not a scenario wrapper registry in the
`.agents/test-layout.md` sense.

Target rules (C3):

- **Libtest targets** must be keld-compat targets. Under F5, only these re-run when the
  validator's inputs change, so a renamed mapped test elsewhere would go unseen. The
  target must exist, and it must not include the owner. The live rule "never this
  validator recursively" becomes a check instead of a comment.
- **Bun targets** live under `packages/`. The literal path in this module is what makes
  the CI router re-run keld-compat when that package changes (F5).

Admission keeps the live command lines, environment and exact-case parsers unchanged,
and splits in two:

- `admit_libtest` runs each distinct libtest target once, over any set of corpora, on
  the host. `admit_bun` does the same for each distinct Bun file.
- A pure `check_admission(corpora, host: Platform, outputs)` then decides. Each mapped
  cell that declares the host platform needs exactly one passing case in that host's
  output. Each cell that does not declare it goes into the returned
  `AdmissionReport`'s `unknown` list (D13). No cell is skipped silently.
- The registry test asserts that the `unknown` list equals the cells whose
  `platforms` exclude the host. Negative controls feed `check_admission` synthetic
  outputs for any host.

Coverage comes from one new integration target, `tests/corpus_registry/main.rs`, with
`mod registry; mod rules;` and the owner include. It runs these tests over `REGISTRY`:

- the C2 census;
- `registered_corpora_validate_with_their_records`, covering the static rules plus
  committed `evidence/*.json`, grouped into runs;
- `registered_corpora_libtest_oracles_execute`;
- `registered_corpora_bun_oracles_execute`;
- the gh532 AC10 owner census.

Today it covers `electron-lifecycle-v0`, so no test in it is vacuous. Each v1 corpus a
consumer registers is then validated and admitted with no further code.

The receipt-bound `lifecycle_corpus` tests stay, because the published receipts name
them (F2). They call the same owner functions on `LIFECYCLE_V0`. The two lifecycle oracle
targets therefore run twice in one test job. That is accepted until KEL-237 re-records
the corpus and `lifecycle_corpus.rs` retires.

Rejected alternatives:

- A test target per corpus. Each consumer must remember to write it, so "registered ⇒
  validated" would not hold structurally.
- Targets declared in the manifest, which gh532 rule 8 forbids.
- The generic target added by the first consumer. Admission tests shipped now without a
  v1 corpus would assert nothing, and leaving them all to a consumer splits the owner
  across tickets.
- Rust oracle targets in other crates (F5).
- Directory oracle targets (`tests/<t>/main.rs`). YAGNI until a consumer needs one.

*Falsifier:* a consumer whose mapped test must live outside keld-compat and `packages/`,
for example a keld-host GUI harness. That consumer first adds a routing edge and amends
this rule.

**D9 — Proving the byte-identical migration.** Five proofs:

1. **The PR diff.** `git diff --exit-code origin/main --` is empty over
   `crates/keld-compat/fixtures/`, `crates/keld-compat/tests/electron_lifecycle.rs` and
   `packages/@keld/electron/src/app.test.ts`.
2. **A permanent frozen table (gh532 AC11).** `V0_FROZEN.files` lists every file under
   `fixtures/lifecycle-corpus/`: `corpus.json`, `denominator.json`, `report.md`, 9
   evidence records and 3 receipts, 15 in all. Each carries its digest, computed once
   with `git show origin/main:<path> | shasum -a 256` at the T2 base. The corpus entry
   reuses the one `V0_CORPUS_SHA256` constant (`sha256:badc0aaf…`).
   `lifecycle_corpus_fixture_bytes_match_origin_main` asserts that the files on disk are
   exactly the table's keys and that each digest matches. *Negative control:* a copy
   with one byte flipped, and a sixteenth file, each fail.
3. **The report.** The live `lifecycle_report_is_a_deterministic_view_of_canonical_records`
   test passes unchanged against `report.md`.
4. **The receipt names (C1).** `published_receipt_cases_remain_live_tests` checks the
   host OS's receipt by platform (F2). It runs one libtest `--list` and one
   `--list --ignored` per target named there (F4). A name is admitted only if the first
   lists it once and the second does not list it. Following `.agents/test-layout.md`, it also preserves every
   `lifecycle_corpus` name at the crate root, keeps the target names and keeps the live
   negative-control input tables byte-identical.
5. **Mutation runs.** The T2 PR records three of the issue's controls as temporary
   on-disk mutations, plus the C1 rename. Each one fails its named test with the
   `CorpusError` variant listed here, and is then restored:
   - the one-byte manifest flip, which yields `DigestMismatch`;
   - the 44.4.5 version inside the 44.3.0 corpus. One v0 cell's `oracle_id` prefix
     becomes `electron-v44.4.5.`, which yields `PinMismatch`. The run also rewrites
     `denominator.json`'s `corpus_sha256` to the digest of the mutated manifest, so it
     passes the digest gate (D2) and reaches the pin check. Changing the manifest's
     `upstream.electron_commit` instead is a different control, and it yields
     `UnadmittedPin` (D4);
   - a mapped Bun test changed to `test.skip`, which yields `CaseNotAdmitted`;
   - the C1 rename, which makes `published_receipt_cases_remain_live_tests` fail and
     name the missing case.

   The fourth issue control, deleting `implementing_ticket` from a red fixture cell,
   needs the v1 parser. The T3 PR runs it, and it yields `VerdictRule`.
   The PR also lists `cargo test -p keld-compat -- --list` before and after, as an
   inventory diff: additions only, no omissions or renames.

Rejected alternatives:

- Re-recording the corpus (gh532 §4.3).
- A one-time `--list` diff as the only name proof. It covers the migration but not the
  frozen receipts afterwards.
- One framed digest over the whole directory. It would hide which file drifted.
- Generic test names inside `lifecycle_corpus.rs`, which would orphan the receipts.

*Falsifier:* after T2, some receipt-mapped case is not listed by its target on that
receipt's platform.

**D10 — Removing the duplicates.**

From `lifecycle_corpus.rs`, these move into the owner:

- the `Manifest`, `Upstream` and `CorpusCell` structs, and `manifest()`;
- `sha256_uri`, `workspace_root`, `rust_case_passed` and `bun_case_passed`;
- `RUST_TEST_PATH` and `TS_TEST_PATH`;
- the pin and verdict assertions.

The six test functions keep their names and call the owner. The facts specific to the
lifecycle (its id, `showcase`, `primary_workflow`, and the "not median-app product
compatibility" scope) stay as assertions on the parsed `Corpus`.

From `lifecycle_evidence_report.rs`:

- **Moved into the owner:** `sha256_uri`, `platform_token`, `arch_token` and
  `verdict_token`, the owner's one copy of each map (F9).
- **Removed:**
  - `expected_verdict()` (D6);
  - the `CORPUS_SHA` and `ELECTRON_COMMIT` constants and the `44.3.0` literals. These
    now come from `V0_CORPUS_SHA256` and from the parsed `corpus.pin()` (D4);
  - the `include_bytes!` of `denominator.json` (now read from `Corpus::denominator()`);
  - the generic per-record assertions, which become `validate_harness_run`;
  - the literal authority line, which becomes the C7 line helper.
- **Kept:** the receipt structs and `assert_receipt`, because
  `keld.lifecycle.ci-receipt/v1` is a lifecycle receipt, not a manifest. Also kept are
  `PUBLISHED`, the report prose, `operation_meaning`, the score assertions, and the
  lifecycle-only record checks. The keld revision equals the tested commit, the Bun
  revision equals `1.4.2+744846f84`, the engine revision equals
  `headless-lifecycle-conformance@<tested commit>`, and `evidence_uri` equals the
  receipt digest.

Two items stay where they are:

- `electron_lifecycle.rs` keeps its private `workspace_root`. It is an oracle target, so
  C3 forbids it from including the owner. Sharing one 6-line function would need a
  second support file, which YAGNI rules out.
- The KEL-77 producer constant is outside keld-compat. It was parked from #637 and is
  recorded on #566.

The gh532 AC10 census (`owner_census`) checks five rules. Each has a synthetic
negative input, rejected with `CensusViolation`, which names the rule and the file:

1. Outside the owner, no keld-compat test source contains `fn sha256_uri`, `Sha256`,
   `corpus.json"` or `denominator.json"`.
2. Outside the owner, no struct that derives `Deserialize` declares a manifest field:
   `corpus_id`, `cells`, `upstream`, `oracle_id`, `expected_verdict` or `test_path`.
   The census tracks this with a line state machine. It starts at a `#[derive(…)]`
   line that names `Deserialize` and ends where that struct's body closes. This
   catches a second manifest parser under any name. The receipt structs pass, because
   none of their fields has one of those names.
3. Inside the owner, everything appears exactly once:
   - one `fn sha256_uri`;
   - one `Sha256::digest` call site;
   - one deserialisation site per shape that exists. T2 checks
     `serde_json::from_slice::<ManifestV0>`. T3, which adds `ManifestV1`, extends the
     rule to `serde_json::from_slice::<ManifestV1>`;
   - no `#[test]`.
4. `src/lib.rs` declares exactly one public module, `pub mod evidence;`, with no other
   `pub mod` or `pub use` (gh532 §7). *Negative control:* a synthetic `lib.rs` that adds
   `pub mod corpus_manifest;` is rejected.
5. `cargo metadata` lists `sha2` for keld-compat with kind `dev` only.

C9 adds a sixth rule, for reports (D11).

The census patterns and the C2 fixture walk live in `tests/support/corpus_census.rs`,
split out under D1 (amended in T2, #640). The fixture file names live in the owner.
Rules 1, 2 and 6 scan everything except the owner, so the census module is scanned too;
it builds every pattern with `concat!`. Rule 3 counts inside the owner, and no
`tests/support/` file may hold a test. Test files only call support functions, and
they build their synthetic negative inputs with `concat!`, so no scanned file carries a
pattern.

*Falsifier:* any of the five rules, or C9's, accepts its own synthetic negative input.

**D11 — Pending versus divergence (gh532 AC17).**

`Run::fail_split()` returns
`FailSplit { pending: BTreeMap<CellKey, ticket>, divergence: BTreeSet<CellKey> }`. Both
are keyed by the full KEL-74 `CellKey`, never by `operation_id` alone, so two cells that
share an operation are counted apart. It classifies each `fail` record by its cell's
manifest key (gh532 rule 3). `unknown` records count in neither set.

`Display` always renders both lines, with labels from the owner constants
`PENDING_LABEL` and `DIVERGENCE_LABEL`. Ticket keys are sorted and deduplicated, and a
zero count has no parenthesis:

```text
Pending implementation: 1 (GH-445)
Intentional divergence: 1
```

This is recorded gh532 amendment A1 (§2). gh532 AC17 names "the lifecycle evidence
report". The frozen v0 report cannot carry a pending cell: the v0 shape has no
`implementing_ticket` field, and it denies unknown fields. A split there would always
equal `board.failed()`, so no control on it could fail. AC17 therefore binds
`FailSplit` and its `Display`.

C9 keeps the original strength, so that every v1 report renders its counts through
them. That is census rule 6, `report_census`, in the owner. Outside the owner, the
label strings `Pending implementation` and `Intentional divergence` and any `.failed()`
call may appear only in `tests/lifecycle_evidence_report.rs`. That file is the frozen
v0 report, admitted by exact path. On origin/main `c1673d83`, all five such
occurrences are in that file. The exemption ends when KEL-237 re-records the corpus.

A v1 report therefore cannot count fails, or write either label, by itself. It must
call `FailSplit`.

The census scans the rules module too. The AC17 test's exact `Display` assertion is
therefore built from the owner constants `PENDING_LABEL` and `DIVERGENCE_LABEL`, never
from literal labels. The C9 synthetic report sources, which do contain a label or
`.failed()`, are assembled with `concat!`. The v0 report keeps `board.failed()` in its "Intentional divergence"
column, and its bytes are unchanged.

Rejected alternatives:

- Counting from `board.failed()` in a v1 report. That is exactly the live lump AC17
  targets. Only the v0 report keeps it, because v0 has no pending key.
- A new verdict, or a new record field (gh532 §4.3).
- Deriving the split from records alone, which cannot tell the two cases apart.

*Falsifier:* one pending and one divergence `fail` record render as a single count, or
the pending cell is labelled "Intentional divergence". C9 falsifier: a synthetic
report outside the exempt path that renders `board.failed()` passes the census.

**D12 — Sequencing with #637, and the task split.**

Every X01-T4 implementation PR branches from an `origin/main` that contains #638's merge
(the #637 work). The reasons:

- T2 renders the authority line and the label errors through `AuthorityProfile::as_str`
  and `Scoreboard::authority_profile()` (C7).
- T3 maps `ProfileState::Unverified` to `AuthorityProfile::Unverified`, and it parses
  `unverified` records (gh532 AC8 and AC12).

This keeps gh532 §6's order (T2 before T3) for all X01-T4 code. Only this spec PR
proceeds in parallel.

Before each first edit, run this check:

```sh
git merge-base --is-ancestor <#638 merge commit> HEAD && grep -n -E 'Unverified|-> Option<AuthorityProfile>' crates/keld-compat/src/evidence.rs
```

On an `origin/main` without #638, the `grep` finds nothing and exits 1.

The work splits into two PRs:

- **T2** is the behaviour-preserving migration plus its guards.
- **T3** is the new v1 rules.

Rejected alternatives:

- **One PR.** A refactor and new rules in one diff make the byte-identity review harder
  (`.agents/test-layout.md`: separate moves from product changes).
- **Migrating before #637.** That would leave a second variant-to-string map for the
  authority label, and it would diverge from gh532 §6.

*Falsifier:* T2 cannot pass without a v1 rule. In that case T2 and T3 merge into one PR.

**D13 — Declared platforms per cell (C10; gh532 amendment A2).** #445 and #448 are
macOS-only first-proof cells (F13). Without a per-cell platform set, admission would
require a passing case on every lane, so those cells would fail on Linux and Windows.

Delegated owner decision: every v1 cell carries an explicit `platforms` field. It is a JSON array of `platform_token` values. The
array must be non-empty and hold no repeats, and every value must be in the
registration's `platforms` (code, so a manifest cannot widen its own lanes).

- **Declared platform.** On a declared platform, admission requires exactly one
  passing mapped case. A test that a `cfg` gate compiles out has no case, and a Bun
  `skipIf` reports `(skip)`. The existing exact-case parsers reject both, so each fails
  admission as `CaseNotAdmitted`.
- **Undeclared platform.** On an undeclared platform, the cell goes into the
  `AdmissionReport`'s `unknown` list. That is the unrun lane of gh532 rule 4: never
  passed, never silently skipped.
- **Records.** A record for an undeclared platform must say `result: unknown`
  (`RecordRule`), and the AC9 engine map still has to cover that platform.

v0 cells have no `platforms` field, because the bytes are frozen. They take the
`LIFECYCLE_V0` registration's `platforms`, which is all three. The Windows-only
`#[cfg(windows)]` test is not a corpus cell, so nothing changes for v0.

Rejected alternatives:

- **Platforms per registration only.** That cannot express one macOS-only cell inside
  a corpus that also has three-platform cells.
- **Inferring platforms from `cfg` attributes or `skipIf` in test source.** That
  treats source text as admission evidence, which the live controls forbid.
- **Treating a missing case on any platform as `unknown`.** That would hide a removed
  or skipped test on a declared platform, which is exactly what the admission controls
  exist to catch.

*Falsifier:* a declared-platform cell is admitted with no passing case on that host,
or an undeclared-platform cell is neither in the `unknown` list nor rejected.

### 4.3 Owner sketch

This is test-only Rust and shows the shape, not every variant. The final form belongs to
T2 and T3.

```rust
//! One owner for committed compatibility corpus manifests (gh532 rule 8, X01-T4).
// Each including target uses a different subset of this module (keld-ipc precedent).
#![allow(dead_code)]

pub fn sha256_uri(bytes: &[u8]) -> String { format!("sha256:{:x}", Sha256::digest(bytes)) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin { pub version: &'static str, pub commit: &'static str }
impl Pin { /* oracle_prefix, oracle_revision, doc_blob_prefix, snapshot_dir (D4) */ }

pub const V0_FROZEN_CORPUS_ID: &str = "electron-lifecycle-v0";
pub enum PinScope { FrozenV0 { corpus_id: &'static str }, V1 }
pub struct AdmittedPin { pub pin: Pin, pub scope: PinScope }
/// The one admitted-pin table (D4). A new pin needs a reviewed spec amendment.
pub const ADMITTED_PINS: &[AdmittedPin] = &[
    AdmittedPin { pin: Pin { version: "44.3.0", commit: "07e460719c75b2ec5ee4893f7d2192ef31c7b8c2" },
                  scope: PinScope::FrozenV0 { corpus_id: V0_FROZEN_CORPUS_ID } },
    AdmittedPin { pin: Pin { version: "44.4.5", commit: "694f45852a0f1726cd23bfd379854de489cccb65" },
                  scope: PinScope::V1 },
];
pub const V0_CORPUS_SHA256: &str =
    "sha256:badc0aaf3619168927cf464e2dd0006a599b5614a35b84960c59984b18e0e8b2";
/// Frozen v0 facade (gh532 AC11). Removal: KEL-237 re-records the corpus as v1.
pub const V0_FROZEN: FrozenV0 = FrozenV0 {
    engine_token: "headless-lifecycle-conformance",
    harness_profile: AuthorityProfile::LegacySandboxOff, // fixed; v1 uses HARNESS_PROFILE
    files: &[("corpus.json", V0_CORPUS_SHA256) /* 14 more, pinned at the T2 base */],
};
pub const HARNESS_PROFILE: AuthorityProfile = AuthorityProfile::LegacySandboxOff; // v1 only

pub enum Shape { FrozenV0, V1 }
pub enum Runner { Libtest { target: &'static str }, Bun }
pub struct TestTarget { pub path: &'static str, pub runner: Runner }
pub struct Registration {
    pub corpus_id: &'static str,
    pub fixture_dir: &'static str,
    pub shape: Shape,
    pub platforms: &'static [Platform], // upper bound for each cell's `platforms` (D13)
    pub targets: &'static [TestTarget],
}
pub const LIFECYCLE_V0: Registration = Registration {
    corpus_id: V0_FROZEN_CORPUS_ID,
    fixture_dir: "fixtures/lifecycle-corpus",
    shape: Shape::FrozenV0,
    platforms: &[Platform::Macos, Platform::Linux, Platform::Windows],
    targets: &[
        TestTarget { path: "crates/keld-compat/tests/electron_lifecycle.rs",
                     runner: Runner::Libtest { target: "electron_lifecycle" } },
        TestTarget { path: "packages/@keld/electron/src/app.test.ts", runner: Runner::Bun },
    ],
};
/// Every committed corpus. A consumer appends one entry (§4.4).
pub const REGISTRY: &[Registration] = &[LIFECYCLE_V0];

impl Corpus {
    /// Checks run in the D2 order: digest first, then parse, pin, denominator, cells,
    /// citations and targets.
    pub fn parse(reg: &Registration, manifest: &[u8], denominator: &[u8],
                 read_snapshot: &dyn Fn(&str) -> io::Result<Vec<u8>>)
                 -> Result<Self, CorpusError>;
    /// Reads the fixture dir; adds the D5 `git hash-object` normalisation check.
    pub fn load(reg: &Registration) -> Result<Self, CorpusError>;
    /// The committed manifest and denominator bytes, for in-memory mutation controls.
    pub fn fixture_bytes(reg: &Registration) -> Result<(Vec<u8>, Vec<u8>), CorpusError>;
    pub fn pin(&self) -> Pin;
    pub fn denominator(&self) -> &Denominator;
    /// Calls `score()` once; its `DuplicateCell` is the uniqueness check (C5).
    pub fn validate_harness_run<'a>(&'a self, records: &'a [EvidenceRecord], as_of: CivilDate)
        -> Result<Run<'a>, CorpusError>;
    pub fn validate_product_run<'a>(&'a self, receipt: &ProductReceipt<'_>,
                                    records: &'a [EvidenceRecord], as_of: CivilDate)
        -> Result<Run<'a>, CorpusError>;
}
pub enum ProfileState { Unverified, Legacy, Strict }
pub struct ProductReceipt<'a> { pub state: ProfileState, pub cells: &'a [CellKey] } // KEL-74 CellKey
impl Run<'_> {
    pub fn board(&self) -> &Scoreboard;
    pub fn fail_split(&self) -> FailSplit; // the only fail-count renderer (C9)
}

pub fn rust_case_passed(stdout: &str, name: &str) -> bool; // moved unchanged
pub fn bun_case_passed(stderr: &str, name: &str) -> bool;  // moved unchanged
/// One `<name>: test` line in `--list`, and none in `--list --ignored` (C1).
pub fn rust_case_listed(list: &str, ignored: &str, name: &str) -> bool;
pub fn admit_libtest(corpora: &[&Corpus]) -> Result<RunnerOutputs, CorpusError>;
pub fn admit_bun(corpora: &[&Corpus]) -> Result<RunnerOutputs, CorpusError>;
/// Pure: admitted cells for `host`, and the `unknown` list for undeclared cells (D13).
pub fn check_admission(corpora: &[&Corpus], host: Platform, outputs: &RunnerOutputs)
    -> Result<AdmissionReport, CorpusError>;
pub fn owner_census(tests: &Sources, lib_rs: &str, sha2_kinds: &[Option<String>])
    -> Result<(), CorpusError>; // D10 rules 1–5 and C9's rule 6

#[derive(Debug, PartialEq, Eq)]
pub enum CorpusError {
    Evidence(EvidenceError), // KEL-74 parse or score failure, e.g. DuplicateCell
    DigestMismatch { corpus_id: String, declared: String, computed: String },
    V0ShapeNotAdmitted { corpus_id: String },
    DuplicateKey { corpus_id: String, field: &'static str, key: String },
    UnadmittedPin { corpus_id: String, version: String, commit: String },
    PinMismatch { corpus_id: String, cell: String, found: String, expected: String },
    DenominatorMismatch { corpus_id: String, detail: String },
    EmptyPlatforms { corpus_id: String, cell: String },
    UnregisteredTarget { corpus_id: String, cell: String, test_path: String },
    QuoteAbsent { corpus_id: String, cell: String, page: String },
    SnapshotWouldBeNormalised { corpus_id: String, page: String },
    CaseNotAdmitted { corpus_id: String, cell: String, target: String, host: &'static str },
    LabelMismatch { corpus_id: String, cell: String, receipt_state: &'static str,
                    label: &'static str },
    ReportBypassesFailSplit { file: String, line: usize },
    CensusViolation { rule: u8, file: String, line: usize },
    // … one variant per rejection named in §3 and gh532 §3
}
```

### 4.4 Consumer interface (#445, #448, and later X02-T4)

To add a v1 corpus, a consumer changes only data and one registry line:

1. **The fixture directory.** Add `crates/keld-compat/fixtures/<dir>/` containing:
   - `corpus.json`, in gh532's §4.2 example shape plus each cell's `platforms` (A2).
     #445's and #448's cells declare `["macos"]`;
   - `denominator.json` (KEL-74), whose `corpus_sha256` is the `sha256_uri` of the exact
     `corpus.json` bytes;
   - `doc-snapshots/694f45852a0f1726cd23bfd379854de489cccb65/<page path>`, the raw
     upstream bytes of each cited page, with its digest in `doc_snapshots` and checked
     by the gh532 rule 2 reviewer command;
   - optionally `evidence/*.json`, harness records only, until X02-T5 (C6).
2. **The registry entry.** Append one `Registration` to `REGISTRY` in
   `tests/support/corpus_manifest.rs`. It names `Shape::V1`, the `platforms` its cells
   may declare, and the targets the corpus maps. Those targets are keld-compat libtest
   targets or Bun `*.test.ts` files under `packages/` (C3).
3. **The mapped tests.** Put them in the registered targets. A red cell's test asserts
   today's behaviour and passes (gh532 rule 3). On each declared platform, no mapped test
   is skipped, ignored, `cfg`-gated out or retried. On an undeclared platform, the
   admission report lists the cell as `unknown`, and any record for it says `unknown`
   (C10).

The consumer may add oracle targets for its conformance tests and register them. It adds
no parser, digest helper, validator or validator test target. The `corpus_registry`
target validates, admits and runs the census over the new entry automatically, and the C2
census fails if step 2 is missing. Concurrent consumers each
append one line to `REGISTRY`, so the first PR to go green wins and the later ones
rebase (`docs/agents/workflow.md` § Parallelism rules).

Error messages name the corpus, the cell, the rule and the fix. For example:
`QuoteAbsent: cell app.when-ready.host-ready-gate cites docs/api/app.md; copy the quote byte-for-byte from doc-snapshots/694f…/docs/api/app.md`.

Two consumer-specific notes:

- #448's "labelled v44.4.5 source receipt" cells stay `unknown` under the gh532 §10 Q1
  draft decision. D5 rejects any second citation kind.
- #445's Windows and Linux `unknown` records need an `engine` entry for those platforms
  (gh532 AC9).

### 4.5 Other template items

- **Capabilities and manifest (spec 03):** none.
- **Wire and protocol (spec 02):** none.
- **Platform notes:** the tests run on the existing three-OS PR matrix. Paths are joined
  by component, so they work on Windows. C1 selects the receipt by host platform, so the
  `#[cfg(windows)]` case is checked only on Windows (F2). The Bun reporter caveat still
  applies: run with `CLAUDECODE` and `AI_AGENT` unset (gh532 §7).
- **Runtime seam:** none.
- **Migration unit:**
  - callers: the three including targets (D8);
  - persisted state: the lifecycle fixtures, byte-unchanged (D9);
  - temporary adapter: none;
  - retained facade: `V0_FROZEN`, until KEL-237 re-records the corpus (gh532 §4.5).

## 5. Boundaries

- **Implement in (T1, this PR):**
  - `docs/specs/gh566-corpus-manifest-owner.md`;
  - the in-place amendments A1–A4 to `docs/specs/gh532-first-proof-evidence-rules.md`:
    the header note, AC17's subject, the rule 8 `platforms` paragraph, the `platforms`
    field in the §4.2 example manifest and the owner sketch, the §4.5 callers and the
    §5 file list. Nothing else in gh532 changes.
- **Implement in (T2 and T3):**
  - `crates/keld-compat/tests/support/corpus_manifest.rs` (new);
  - `crates/keld-compat/tests/support/corpus_admission.rs` (new in T2). It holds
    execution admission, split from the owner under the D1 review condition, because
    the owner passed 1,500 lines;
  - `crates/keld-compat/tests/support/corpus_census.rs` (new in T2). It holds the
    owner and fixture censuses, which were split for the same reason when the owner
    crossed 1,500 lines again;
  - `crates/keld-compat/tests/support/corpus_{error,citation,runs}.rs` (new in T3).
    These are child modules of the owner, declared and included by the owner itself,
    so consumers still include only the owner. T3 places each new responsibility that
    has its own invariant in one of them, planned up front to keep the owner under
    1,500 lines (D1):
    - the `CorpusError` vocabulary;
    - citations and snapshots (D5);
    - record runs and `FailSplit` (D7, D11).

    Manifest parsing (both shapes), the digest, the pin table, the cell rules and the
    registry stay in the owner (amended in T3);
  - `crates/keld-compat/tests/corpus_registry/rules/v1_{fixture,manifest,snapshots,runs}.rs`
    (new in T3). The v1 rule cases are submodules of `rules`, so their names read
    `rules::v1_manifest::…`, `rules::v1_snapshots::…` and `rules::v1_runs::…`. The C9
    report census lives with the other censuses, as
    `registry::report_census_rejects_reports_that_count_fails_themselves`;
  - `crates/keld-compat/tests/corpus_registry/{main,registry,rules}.rs` (new);
  - `crates/keld-compat/tests/lifecycle_corpus.rs`;
  - `crates/keld-compat/tests/lifecycle_evidence_report.rs`.

  The `corpus_registry` target is the one addition to gh532 §5's X01-T4 file list.
  The gh532 rule cases and the registry coverage need a target that the published
  receipts do not bind to lifecycle names (D8). gh532 §7 already places these tests
  under `crates/keld-compat/tests/` without naming a file.
- **Must not touch:** everything gh532 §5 lists for X01-T4. In addition:
  - `crates/keld-compat/tests/electron_lifecycle.rs`;
  - `packages/@keld/electron/src/app.test.ts`;
  - `crates/keld-compat/src/` and `crates/keld-compat/Cargo.toml`;
  - `.gitattributes`, `.editorconfig` and `tools/`. There is one exception:
    `tools/ci-inputs.json` registers the support modules as Rust readers and rebinds
    its digests. Its `rust_source_census` fails closed on any new crate file, so T2
    needs this edit (amended in T2, #640). In T3, the `corpus_citation.rs` reader also
    gains the `.gitattributes` input edge, because its D5 check runs
    `git hash-object`;
  - any part of gh532 other than the A1–A4 passages named above.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] **T1** This spec, reviewed to `Status: approved`.
- [ ] **T2** Owner core and byte-identical lifecycle migration. It branches from a main
      that contains #638 (D12).
  - Scope: the owner with v0 parsing, digest, pin, registry, admission, harness runs and
    the census; the `corpus_registry` target; and the D10 removals.
  - Criteria: gh532 AC10 and AC11; AC2, AC7 and AC9 over v0 cells and records (AC9
    through the `V0_FROZEN` engine token); the gh532 AC8 clause that rejects `strict_bun`
    on a conformance-harness record; C1 (including the `--list --ignored` exclusion),
    C2, C3, C7 and C8; and C5's v0 controls. T2 moves the live per-record result check
    into `validate_harness_run`, so its replacement is tested in the same PR. In T2,
    admission admits every v0 cell on every host, because v0 cells have no `platforms`.
  - The PR carries the D9 diff, inventory and mutation evidence.
- [ ] **T3** v1 manifest and rules, also branched from a main that contains #638.
  - Scope:
    - `ManifestV1` with `UniqueMap`;
    - citations and snapshots, including the D5 normalisation check;
    - verdict keys and the `engine` map;
    - the `platforms` field on cells and on `Registration`, and `check_admission`'s
      `unknown` list (D13);
    - product runs;
    - `FailSplit`, with census rule 6.
  - Criteria: gh532 AC1; AC2 for v1; AC3–AC6; AC7's `artifact_digest` clause; AC8's
    product clauses; AC9's manifest `engine` map; AC12, AC16 and AC17; C4, C6, C9 and
    C10; and C5's v1 controls.
  - The T3 PR runs the fourth issue control as a mutation: deleting
    `implementing_ticket` from a red fixture cell.

Two tracker actions, which are not PRs:

- At approval, the orchestrator adds #637 as a native blocked-by link on #566, unless
  #638 has already merged.
- The consumer edges stay with their tickets. #445 and #448 already list #566 as a
  blocker.

## 7. Test plan

All targets are under `crates/keld-compat/tests/`. "rules" and "registry" are modules of
the `corpus_registry` target.

| Criterion | Test | Target | Task | Kind |
|---|---|---|---|---|
| gh532 AC10 | `owner_census_finds_one_parser_and_one_digest_helper`, covering D10 rules 1–5: tokens outside the owner, `Deserialize` manifest structs, exactly-one inside the owner (the `ManifestV0` site in T2; T3 adds the `ManifestV1` site), the `src/lib.rs` `pub mod` control, and the `sha2` kind via `cargo metadata`. Each rule has a synthetic negative input built with `concat!`. | registry | T2, extended in T3 | integration |
| gh532 AC11 | `lifecycle_corpus_fixture_bytes_match_origin_main`; `rules::v0_shape_rejects_new_ids_and_foreign_pins` | lifecycle_corpus, rules | T2 | integration |
| C8 | `rules::denominator_must_match_manifest_cells_and_id`, which mutates the committed lifecycle denominator in memory | rules | T2 | integration |
| gh532 AC2, AC7 and AC9 (v0), AC8 harness clause | `rules::harness_run_rejects_digest_revision_engine_and_label_mutations`, which mutates one committed record in memory | rules | T2 | integration |
| C5 (v0 controls) | `rules::records_must_match_their_v0_cell`, covering result against expected, duplicate, `waived`, kind and membership | rules | T2 | integration |
| Committed corpora | `registered_corpora_validate_with_their_records`: static rules plus `evidence/*.json` grouped into runs, on `electron-lifecycle-v0` | registry | T2 | integration |
| Live admission controls | `rust_case_results_reject_…` and `bun_case_results_reject_…`, both moved unchanged; `registered_corpora_{libtest,bun}_oracles_execute` | lifecycle_corpus, registry | T2 | integration |
| C1 | `published_receipt_cases_remain_live_tests`, plus `rust_case_listed` rows: missing, duplicated, prefixed, and listed under `--ignored` | lifecycle_evidence_report | T2 | integration |
| C2 | `every_committed_corpus_is_registered`, with synthetic negative controls | registry | T2 | integration |
| C3 | `rules::targets_reject_unregistered_recursive_foreign_and_missing` | rules | T2 | integration |
| C7 | `report_authority_line_comes_from_the_board`, with in-test boards from `score()` | lifecycle_evidence_report | T2 | integration |
| gh532 AC1–AC7 and AC9 (v1) | `rules::pin_*`, `citation_*`, `red_cell_*`, `uncited_*`, `digest_*` and `engine_*`: one accept case plus each named mutation, on in-test v1 manifests | rules | T3 | integration |
| gh532 AC8 and AC12 | `rules::product_run_*`: every state × label pair, and a receipt with no record | rules | T3 | integration |
| gh532 AC16 | `rules::snapshot_*` over an in-memory read seam, plus `rules::snapshot_would_be_normalised_rejects_crlf` (D5) over one temporary file | rules | T3 | integration |
| C9 | `rules::report_census_rejects_reports_that_count_fails_themselves`: synthetic report sources that use `board.failed()` or a label are rejected, and a `FailSplit` source is accepted | rules | T3 | integration |
| C10 | `rules::platforms_*`: empty, repeated, unknown and unregistered entries; `check_admission` with synthetic outputs for a declared host whose case is missing (`cfg`) or `(skip)`; the `unknown` list on an undeclared host; and a `pass` record for an undeclared platform | rules | T3 | integration |
| gh532 AC17 | `rules::fail_split_counts_pending_apart_from_divergence`: the exact two-line `Display`, built from `PENDING_LABEL` and `DIVERGENCE_LABEL` so the C9 census passes on the test itself; then the lumped-renderer mutation and the pending-as-divergence mutation | rules | T3 | integration |
| C4, C5 (v1 controls), C6 | `rules::duplicate_keys_*`, `rules::records_*_v1` (including two cells that share an `operation_id`), `rules::product_records_need_receipt` | rules | T3 | integration |

**Negative-control routing.** `Corpus::parse` checks the digest first (D2). Every
control that mutates the manifest for another atom therefore rewrites the in-memory
denominator's `corpus_sha256` to the mutated manifest's digest, using the rules
module's `rehash_denominator`. That helper calls the owner's `sha256_uri`. Each control
asserts the exact `CorpusError` variant:

| Control | Mutation | Rehash | Expected variant |
|---|---|---|---|
| One-byte flip (issue; gh532 AC7, AC11) | first `e` → `E` in the manifest | no; the digest is the target | `DigestMismatch` |
| 44.4.5 inside the 44.3.0 corpus (issue; gh532 AC11) | one v0 `oracle_id` prefix → `electron-v44.4.5.` | yes | `PinMismatch` |
| Unadmitted pin (gh532 AC1, AC11) | the manifest's `upstream.electron_commit` → `694f4585…` | yes | `UnadmittedPin` |
| Record revision (gh532 AC2) | one record's revision → `…@694f4585…` | no; record only | `PinMismatch` |
| New id in the v0 shape (gh532 AC11) | registration id `electron-lifecycle-v1` | no; bytes unchanged | `V0ShapeNotAdmitted` |
| C8 missing, extra or renamed cell or id | edit the denominator only | no; manifest unchanged | `DenominatorMismatch` |
| C8 repeated `CellKey` | duplicate one manifest cell | yes | `DenominatorMismatch` |
| C4 repeated key | repeat one `engine` key | yes | `DuplicateKey` |
| Deleted ticket (issue; gh532 AC4) | remove `implementing_ticket` from a red cell | yes | `VerdictRule` |
| Fabricated quote (gh532 AC16) | quote absent from the page, with its correct `quote_sha256` | yes | `QuoteAbsent` |
| Empty `platforms` (C10) | `"platforms": []` | yes | `EmptyPlatforms` |
| C3 unregistered or recursive target | edit the registration only | no | `UnregisteredTarget` / `InvalidTarget` |
| Skipped Bun test (issue) | `(skip)` row, or `test.skip` on disk in the T2 run | no | `CaseNotAdmitted` |
| C5 duplicate record | a second record for one cell | no | `Evidence(DuplicateCell)` |

Anti-flake: gh532 §7 applies. There is no clock and no network, and there are no ports
beyond the live `electron_lifecycle` ones. Rule cases use in-memory bytes and snapshots.
The one temporary file, for D5's CRLF control, sits under `std::env::temp_dir()`, and a
drop guard removes it. The nested `cargo` and `bun` runs keep the live
`--offline` mode, environment and exact-case parsers. Concurrent runs of the oracle
targets are isolated by `electron_lifecycle`'s per-process session directories.

Each PR runs:

- `cargo fmt --all --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo nextest run --workspace --profile ci`;
- the D9 mutations, in T2.

Run all of them with `CLAUDECODE` and `AI_AGENT` unset.

## 8. Review gates triggered

This PR (T1) is docs-only and triggers none. T2 and T3 trigger none either. They are
test-only. `sha2` stays a dev-dependency, `src/` gains no public item, and there is no
wire or permission change (gh532 §8).

## 9. Perf impact

None on any budgeted path. These are cold test-time checks. D8 runs the two lifecycle
oracle targets a second time in each test job, until KEL-237 retires
`lifecycle_corpus.rs`. The T2 PR reports the nextest durations of both admission pairs
instead of claiming a number.

## 10. Open questions

None. Every design point above is an owner-delegated decision, recorded with its
rejected alternatives and its falsifier. The four gh532 amendments A1–A4 (§2) are part
of what this spec's approval approves. Two gh532 draft decisions stay with gh532, not
here: §10 Q1 (citation kinds) and Q2 (the harness label). This spec implements each of
them as one switch: the D5 field set and the D7 `HARNESS_PROFILE` constant, which
leaves the frozen v0 label unchanged.
