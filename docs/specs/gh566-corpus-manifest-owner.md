# Spec: shared corpus-manifest owner (X01-T4)

Status: draft
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

Observable outcome: gh532 AC1–AC12, AC16 and AC17, plus this spec's C1–C8, pass as tests
in `crates/keld-compat/tests/`, each with its named negative control. And
`git diff origin/main -- crates/keld-compat/fixtures/` is empty.

Non-goals:

- Every gh532 §1 non-goal and every item in the issue's Out of scope. That covers
  re-recording `electron-lifecycle-v0`, the KEL-77 fixture-set digest, any production
  keld-compat API or dependency, and the CLAUDECODE Bun reporter fix.
- Changing a gh532 rule, AC, field name or file location. Where this spec is more
  specific than gh532, it narrows a gh532 choice and says so.
- The product receipt schema and the KEL-78 evidence that decides a profile state. Both
  belong to X02-T5.
- Committing a v1 corpus, a doc snapshot or a `.gitattributes` rule. The first consumer
  does these (§4.4).
- Two items left outside this spec. One is the KEL-77 producer constant in `keld-runtime`
  (the second item parked from #637). The other is the private `workspace_root` copy in
  `electron_lifecycle.rs` (§4.2 D10).

## 2. Spec refs

- gh532, which this spec consumes unchanged:
  - §3 AC1–AC12, AC16 and AC17.
  - §4.1, its facts and atoms.
  - §4.2 rules 1–8, the v1 example manifest and the owner sketch.
  - §4.5, the migration unit.
  - §5, the X01-T4 boundary.
  - §6 T3, §7 (test plan), and the §10 Q1 and Q2 draft decisions.
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

gh532 AC1–AC12, AC16 and AC17 are criteria of this spec as written there, with their
negative controls. §6 assigns each one to a task, and §7 assigns each one to a test. The
issue's own criteria and controls map onto them as follows:

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
   `cargo test --offline -p keld-compat --test <target> -- --list`. *Negative
   control:* the test fails if one `lifecycle_corpus` case is renamed, or moved into a
   module (which gives it a `module::` prefix). `rust_case_listed` rejects a missing,
   duplicated or prefixed name.
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
   - each cell has at most one record in the run;
   - each record has the manifest `kind`;
   - each record's `result` is the cell's `expected_verdict` or `unknown`, and only
     `unknown` for an uncited v1 cell;
   - `waived` is rejected.

   *Negative control:* each of these is rejected: a `pass` record for a red cell, a
   `fail` record for a `pass` cell, and a second record for one cell. *Positive
   control:* two cells that share an `operation_id` but have different oracle ids each
   take their own record.
6. **C6 — Product records fail closed (D7).** Given a registered product-panel corpus
   with files under `evidence/`, when the registry test runs, then it fails with
   `ProductRecordsNeedReceipt`. This holds until X02-T5's receipt reader supplies a
   `ProductReceipt`. *Negative control:* a synthetic product corpus with one record is
   rejected, and it is never scored as a harness run.
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
   `CellKey`.

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
  `windows-x86_64`). A file with CR line endings is normalised when it is committed.
- **F7.** Fetched 2026-10-08 at the pin `694f4585`: `docs/api/app.md` (78,086 bytes,
  digest `sha256:49238ddf…`, which is gh532's example value) and
  `docs/api/browser-window.md` (63,122 bytes, `sha256:49061d5e…`). Neither contains a CR
  byte or a `mermaid` fence. `tools/mermaid_docs.rs` scans every tracked `*.md`, a
  snapshot included.
- **F8.** PR #638 (#637, gh532 T2; open at `3cbf300d`) adds `AuthorityProfile::Unverified`,
  `const fn AuthorityProfile::as_str` and
  `Scoreboard::authority_profile() -> Option<AuthorityProfile>`.
- **F9.** KEL-74's `parse_platform`, `parse_verdict`, `parse_panel`, `parse_kind` and
  `parse_evidence_uri` are private. The report test already mirrors three of these maps
  (`platform_token`, `arch_token` and `verdict_token`).
- **F10.** Open PR #609 edits four `assert_receipt` lines in
  `lifecycle_evidence_report.rs`. The overlap is textual only.

Decision atoms. Each is falsified by its own observable:

| Atom | Owner | Input → output | Failure mode | Observable |
|---|---|---|---|---|
| Owner shape | `tests/support/corpus_manifest.rs` | sources → one parser and one digest helper | a second copy drifts | gh532 AC10 |
| Digest | `sha256_uri` | bytes read → `sha256:` + hex | normalised or re-serialised bytes | gh532 AC7 and AC11 |
| Shape and parse | `Registration::shape` and `V0_FROZEN` | registration + bytes → v0 or v1, with a denominator that agrees | a manifest chooses its own rules; a duplicate key; a cell missing from the denominator | gh532 AC11, C4 and C8 |
| Pin | `V1_ADMITTED_PINS` and `V0_FROZEN.pin` | upstream, `oracle_id` and record revision → consistent | mixed pins in one corpus | gh532 AC1, AC2 and AC11 |
| Citation and snapshot | `doc_citation` and `doc_snapshots` via one read seam | cell + pinned page → quote found | a fabricated quote with a matching digest | gh532 AC3 and AC16 |
| Verdict keys | the v1 cell rules | cell → `pass`, `fail` + one key, or `unknown` | pending work read as ▲; an uncited `pass` | gh532 AC4–AC6 |
| Record results | the run validator | record + cell → admitted | a `pass` record scored on a red cell | C5 |
| Authority | the run validator | panel or receipt state → one label | `unverified` relabelled | gh532 AC8 and AC12, C6 and C7 |
| Engine | the manifest `engine` map, or the v0 facade token | platform → identity | mixed engines | gh532 AC9 |
| Registry | the `REGISTRY` code | `test_path` → an admitted oracle | a self-admitted, recursive or unrouted target | C2 and C3 |
| Admission | the libtest and Bun parsers | runner output → exactly one passing case | a removed, skipped or source-only test | the live controls |
| Pending split | `Run::fail_split` | manifest key + `fail` record → two counts | lumped into ▲ | gh532 AC17 |
| Frozen migration | the `V0_FROZEN` table and the receipt guard | committed bytes and names → identical and live | drift, or orphaned receipt cases | gh532 AC11 and C1 |

Edges between atoms are explicit, never hidden:

- Citation and snapshot consumes Pin, because the commit is part of both the URL and the
  snapshot path.
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

Rejected alternatives:

- Hashing re-serialised JSON. That changes the frozen digest, and it gives two byte forms
  one digest, which contradicts gh532 AC7.
- Normalising CRLF in the test. That passes on bytes that differ from the committed blob.
- The KEL-77 length-framed set digest. It exists for sets of several files. v1 binds each
  snapshot through its `doc_snapshots` digest inside the manifest bytes (gh532 rule 2),
  so the framing pitfall in the 2026-08-19 [runtime] learning does not apply.
- A `-text` attribute now. F6 shows that LF blobs already round-trip, and a CR-bearing
  page fails closed instead (D5).

*Falsifier:* on some CI OS, the digest of the bytes read differs from
`git show origin/main:<path> | shasum -a 256` for an unchanged file. Add a path-scoped
`-text` attribute; never normalise in the test.

**D3 — Parsing and shape admission.** The registration, which is code, declares the
shape. A manifest cannot choose its own.

- `Shape::FrozenV0` is admitted only when the registration's `corpus_id` is
  `V0_FROZEN.corpus_id` (`electron-lifecycle-v0`). Any other id fails with
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
- The manifest's `panel` and `kind` strings must equal the corresponding strings in the
  denominator bytes. The KEL-74 parser validates them, and the typed `Panel` comes from
  `parse_denominator`. That way no second panel or kind map exists (F9).
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

**D4 — Pin consistency.** The admitted pins are code:

- `V1_ADMITTED_PINS` is one entry, `44.4.5` @
  `694f45852a0f1726cd23bfd379854de489cccb65`.
- `V0_FROZEN.pin` is `44.3.0` @ `07e460719c75b2ec5ee4893f7d2192ef31c7b8c2`, admitted only
  for the frozen id.

The manifest `upstream` pair must equal an admitted pair exactly, version and commit
together. Otherwise it fails with `UnadmittedPin`.

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
frozen bytes satisfy it. The lifecycle receipt and report checks read `V0_FROZEN.pin`
instead of literals.

Rejected alternatives:

- A pin field in the registration, which copies manifest data.
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

- A cited page whose raw bytes contain CR is normalised when it is committed (F6), so its
  digest check fails. The fix is a `-text` attribute scoped to that path, in that
  consumer's PR.
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

- `Corpus::validate_harness_run(records)`, for the showcase panel;
- `Corpus::validate_product_run(&ProductReceipt, records)`, for the product panel.

Each refuses the other panel. Both apply the same record checks:

- C5 membership, uniqueness, kind and result;
- `artifact.sha256` equals the corpus digest (gh532 AC7);
- `operation.oracle.revision` equals `oracle_revision()` (gh532 AC2);
- `revisions.engine` is the platform's token, then `@`, then a non-empty revision with no
  whitespace and no `@` (gh532 AC9). The token comes from the v1 manifest `engine` map,
  whose keys are `platform_token` values. For v0 it comes from `V0_FROZEN.engine_token`
  (`headless-lifecycle-conformance`). A platform absent from the map is rejected.

Labels then follow the panel:

- **Harness runs** need exactly `AuthorityProfile::LegacySandboxOff`. This is gh532
  rule 6 and the §10 Q2 draft decision, held in one constant, `HARNESS_PROFILE`, so a
  later owner reading of Q2 changes one line.
- **Product runs** follow gh532 rule 6. `ProductReceipt { state: ProfileState, cells }`
  is the owner's minimal view of a run receipt. `cells` holds the KEL-74 `CellKey`s the
  run covered.

  | `ProfileState` | Required label |
  |---|---|
  | `Unverified` | `AuthorityProfile::Unverified` |
  | `Legacy` | `LegacySandboxOff` |
  | `Strict` | `StrictBun` |

  Each receipt cell needs exactly one record (`MissingProductRecord`, so the run is not
  dropped). A wrong label fails with `LabelMismatch`, which names both the receipt
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
`[LIFECYCLE_V0]`. A `Registration` has four fields: `corpus_id`; `fixture_dir`, relative
to `crates/keld-compat`; `shape`; and `targets: &[TestTarget]`. A `TestTarget` is a
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

Admission keeps the live command lines, environment and exact-case parsers unchanged:

- `admit_libtest` runs each distinct libtest target once, over any set of corpora.
- `admit_bun` does the same for each distinct Bun file.
- Every mapped cell must have exactly one passing case.

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
   host OS's receipt by platform (F2). It runs one libtest `--list` per target named
   there (F4). Following `.agents/test-layout.md`, it also preserves every
   `lifecycle_corpus` name at the crate root, keeps the target names and keeps the live
   negative-control input tables byte-identical.
5. **Mutation runs in the T2 PR.** The PR records the four issue controls and the C1
   rename as temporary mutations. Each one fails its named test and is then restored.
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
  - the `CORPUS_SHA` and `ELECTRON_COMMIT` constants and the `44.3.0` literals (now read
    from `V0_FROZEN`);
  - the `include_bytes!` of `denominator.json` (now read from `Corpus::denominator()`);
  - the generic per-record assertions, which become `validate_harness_run`;
  - the literal authority line, which becomes the C7 line helper.
- **Kept:** the receipt structs and `assert_receipt`, because
  `keld.lifecycle.ci-receipt/v1` is a lifecycle receipt, not a manifest. Also kept are
  `PUBLISHED`, the report prose, `operation_meaning`, the score assertions, and the
  lifecycle-only record checks: the keld, Bun and engine revision equals the tested
  commit, and `evidence_uri` equals the receipt digest.

Two items stay where they are:

- `electron_lifecycle.rs` keeps its private `workspace_root`. It is an oracle target, so
  C3 forbids it from including the owner. Sharing one 6-line function would need a
  second support file, which YAGNI rules out.
- The KEL-77 producer constant is outside keld-compat. It was parked from #637 and is
  recorded on #566.

*Falsifier:* the gh532 AC10 census finds `fn sha256_uri`, `Sha256`, `corpus.json"` or
`denominator.json"` in any keld-compat test source other than the owner.

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

The v0 report's "Intentional divergence" column then renders
`split.divergence().len()` instead of `board.failed()`, with its header taken from
`DIVERGENCE_LABEL`. The render refuses when the pending set is non-empty (v0 carries no
pending key) or when `pending + divergence != board.failed()`. Its bytes are unchanged:
the column still reads `1`.

Rejected alternatives:

- Counting from `board.failed()`. That is exactly the live lump AC17 targets.
- A new verdict, or a new record field (gh532 §4.3).
- Deriving the split from records alone, which cannot tell the two cases apart.

*Falsifier:* one pending and one divergence `fail` record render as a single count, or
the pending cell is labelled "Intentional divergence".

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
git merge-base --is-ancestor <#638 merge commit> HEAD && grep -n -E 'Unverified|const fn as_str|fn authority_profile' crates/keld-compat/src/evidence.rs
```

The work splits into two PRs:

- **T2** is the behaviour-preserving migration plus its guards.
- **T3** is the new v1 rules.

Rejected alternatives:

- **One PR.** A refactor and new rules in one diff make the byte-identity review harder
  (`.agents/test-layout.md`: separate moves from product changes).
- **Migrating before #637.** That would leave a second variant-to-string map for the
  authority label, and it would diverge from gh532 §6.

*Falsifier:* T2 cannot pass without a v1 rule. In that case T2 and T3 merge into one PR.

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

pub const V1_ADMITTED_PINS: &[Pin] =
    &[Pin { version: "44.4.5", commit: "694f45852a0f1726cd23bfd379854de489cccb65" }];
pub const V0_CORPUS_SHA256: &str =
    "sha256:badc0aaf3619168927cf464e2dd0006a599b5614a35b84960c59984b18e0e8b2";
/// Frozen v0 facade (gh532 AC11). Removal: KEL-237 re-records the corpus as v1.
pub const V0_FROZEN: FrozenV0 = FrozenV0 {
    corpus_id: "electron-lifecycle-v0",
    pin: Pin { version: "44.3.0", commit: "07e460719c75b2ec5ee4893f7d2192ef31c7b8c2" },
    engine_token: "headless-lifecycle-conformance",
    files: &[("corpus.json", V0_CORPUS_SHA256) /* 14 more, pinned at the T2 base */],
};

pub enum Shape { FrozenV0, V1 }
pub enum Runner { Libtest { target: &'static str }, Bun }
pub struct TestTarget { pub path: &'static str, pub runner: Runner }
pub struct Registration {
    pub corpus_id: &'static str,
    pub fixture_dir: &'static str,
    pub shape: Shape,
    pub targets: &'static [TestTarget],
}
pub const LIFECYCLE_V0: Registration = Registration {
    corpus_id: "electron-lifecycle-v0",
    fixture_dir: "fixtures/lifecycle-corpus",
    shape: Shape::FrozenV0,
    targets: &[
        TestTarget { path: "crates/keld-compat/tests/electron_lifecycle.rs",
                     runner: Runner::Libtest { target: "electron_lifecycle" } },
        TestTarget { path: "packages/@keld/electron/src/app.test.ts", runner: Runner::Bun },
    ],
};
/// Every committed corpus. A consumer appends one entry (§4.4).
pub const REGISTRY: &[Registration] = &[LIFECYCLE_V0];

impl Corpus {
    pub fn parse(reg: &Registration, manifest: &[u8], denominator: &[u8],
                 read_snapshot: &dyn Fn(&str) -> io::Result<Vec<u8>>)
                 -> Result<Self, CorpusError>;
    pub fn load(reg: &Registration) -> Result<Self, CorpusError>; // reads the fixture dir
    pub fn denominator(&self) -> &Denominator;
    pub fn validate_harness_run<'a>(&'a self, records: &'a [EvidenceRecord])
        -> Result<Run<'a>, CorpusError>;
    pub fn validate_product_run<'a>(&'a self, receipt: &ProductReceipt<'_>,
                                    records: &'a [EvidenceRecord])
        -> Result<Run<'a>, CorpusError>;
}
pub enum ProfileState { Unverified, Legacy, Strict }
pub struct ProductReceipt<'a> { pub state: ProfileState, pub cells: &'a [CellKey] } // KEL-74 CellKey
impl Run<'_> { pub fn fail_split(&self) -> FailSplit; }

pub fn rust_case_passed(stdout: &str, name: &str) -> bool; // moved unchanged
pub fn bun_case_passed(stderr: &str, name: &str) -> bool;  // moved unchanged
pub fn rust_case_listed(stdout: &str, name: &str) -> bool; // one `<name>: test` line (C1)
pub fn admit_libtest(corpora: &[&Corpus]) -> Result<(), CorpusError>;
pub fn admit_bun(corpora: &[&Corpus]) -> Result<(), CorpusError>;

#[derive(Debug, PartialEq, Eq)]
pub enum CorpusError {
    V0ShapeNotAdmitted { corpus_id: String },
    DuplicateKey { corpus_id: String, field: &'static str, key: String },
    UnadmittedPin { corpus_id: String, version: String, commit: String },
    PinMismatch { corpus_id: String, cell: String, found: String, expected: String },
    UnregisteredTarget { corpus_id: String, cell: String, test_path: String },
    QuoteAbsent { corpus_id: String, cell: String, page: String },
    LabelMismatch { corpus_id: String, cell: String, receipt_state: &'static str,
                    label: &'static str },
    // … one variant per rejection named in §3 and gh532 §3
}
```

### 4.4 Consumer interface (#445, #448, and later X02-T4)

To add a v1 corpus, a consumer changes only data and one registry line:

1. **The fixture directory.** Add `crates/keld-compat/fixtures/<dir>/` containing:
   - `corpus.json`, in gh532's §4.2 example shape;
   - `denominator.json` (KEL-74), whose `corpus_sha256` is the `sha256_uri` of the exact
     `corpus.json` bytes;
   - `doc-snapshots/694f45852a0f1726cd23bfd379854de489cccb65/<page path>`, the raw
     upstream bytes of each cited page, with its digest in `doc_snapshots` and checked
     by the gh532 rule 2 reviewer command;
   - optionally `evidence/*.json`, harness records only, until X02-T5 (C6).
2. **The registry entry.** Append one `Registration` to `REGISTRY` in
   `tests/support/corpus_manifest.rs`. It names `Shape::V1` and the targets the corpus
   maps. Those are keld-compat libtest targets or Bun `*.test.ts` files under
   `packages/` (C3).
3. **The mapped tests.** Put them in the registered targets. A red cell's test asserts
   today's behaviour and passes (gh532 rule 3). No mapped test is skipped, ignored or
   retried.

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

- **Implement in (T1, this PR):** `docs/specs/gh566-corpus-manifest-owner.md` only.
- **Implement in (T2 and T3):**
  - `crates/keld-compat/tests/support/corpus_manifest.rs` (new);
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
  - `.gitattributes`, `.editorconfig` and `tools/`;
  - gh532 itself.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] **T1** This spec, reviewed to `Status: approved`.
- [ ] **T2** Owner core and byte-identical lifecycle migration. It branches from a main
      that contains #638 (D12).
  - Scope: the owner with v0 parsing, digest, pin, registry, admission, harness runs and
    the census; the `corpus_registry` target; and the D10 removals.
  - Criteria: gh532 AC10 and AC11, AC2 and AC7 over v0 cells and records, the gh532 AC8
    clause that rejects `strict_bun` on a conformance-harness record, and C1, C2, C3, C7
    and C8.
  - The PR carries the D9 diff, inventory and mutation evidence.
- [ ] **T3** v1 manifest and rules, also branched from a main that contains #638.
  - Scope: `ManifestV1` with `UniqueMap`, citations and snapshots, verdict keys, the
    `engine` map, product runs, and `FailSplit`, including its use in the v0 report.
  - Criteria: gh532 AC1, AC2 for v1, AC3–AC6, AC7's `artifact_digest` clause, AC8's
    product clauses, AC9, AC12, AC16 and AC17, plus C4, C5 and C6.

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
| gh532 AC10 | `owner_census_finds_one_parser_and_one_digest_helper`. It runs `cargo metadata` to read the `sha2` dependency kind, and its synthetic negative inputs are built with `concat!`, so the census never sees its own tokens. | registry | T2 | integration |
| gh532 AC11 | `lifecycle_corpus_fixture_bytes_match_origin_main`; `rules::v0_shape_rejects_new_ids_and_foreign_pins` | lifecycle_corpus, rules | T2 | integration |
| C8 | `rules::denominator_must_match_manifest_cells_and_id`, which mutates the committed lifecycle denominator in memory | rules | T2 | integration |
| gh532 AC2 and AC7 (v0), AC8 harness clause | `rules::harness_run_rejects_digest_revision_and_label_mutations`, which mutates one committed record in memory | rules | T2 | integration |
| Live admission controls | `rust_case_results_reject_…` and `bun_case_results_reject_…`, both moved unchanged; `registered_corpora_{libtest,bun}_oracles_execute` | lifecycle_corpus, registry | T2 | integration |
| C1 | `published_receipt_cases_remain_live_tests`, plus `rust_case_listed` rows | lifecycle_evidence_report | T2 | integration |
| C2 | `every_committed_corpus_is_registered`, with synthetic negative controls | registry | T2 | integration |
| C3 | `rules::targets_reject_unregistered_recursive_foreign_and_missing` | rules | T2 | integration |
| C7 | `report_authority_line_comes_from_the_board`, with in-test boards from `score()` | lifecycle_evidence_report | T2 | integration |
| gh532 AC1–AC7 and AC9 (v1) | `rules::pin_*`, `citation_*`, `red_cell_*`, `uncited_*`, `digest_*` and `engine_*`: one accept case plus each named mutation, on in-test v1 manifests | rules | T3 | integration |
| gh532 AC8 and AC12 | `rules::product_run_*`: every state × label pair, and a receipt with no record | rules | T3 | integration |
| gh532 AC16 | `rules::snapshot_*` over an in-memory read seam | rules | T3 | integration |
| gh532 AC17 | `rules::fail_split_counts_pending_apart_from_divergence`; the v0 report stays byte-identical | rules, lifecycle_evidence_report | T3 | integration |
| C4, C5, C6 | `rules::duplicate_keys_*`, `rules::records_*` (including two cells that share an `operation_id`), `rules::product_records_need_receipt` | rules | T3 | integration |

Anti-flake: gh532 §7 applies. There is no clock and no network, and there are no ports
beyond the live `electron_lifecycle` ones. Rule cases use in-memory bytes and snapshots,
so no temporary directories are needed. The nested `cargo` and `bun` runs keep the live
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
rejected alternatives and its falsifier. Two gh532 draft decisions stay with gh532, not
here: §10 Q1 (citation kinds) and Q2 (the harness label). This spec implements each of
them as one switch: the D5 field set and the D7 `HARNESS_PROFILE` constant.
