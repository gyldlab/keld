# Spec: first-proof evidence rules (X01-T3)

Status: approved
Linear: GH-532 (#517) · Owner: @0monish · Updated: 2026-10-08

Amended by gh566 (#639), [`gh566-corpus-manifest-owner.md`](gh566-corpus-manifest-owner.md)
§2, in four places, each marked in the text:

- A1, the subject of AC17.
- A2, the v1 cell field `platforms`, in rule 8, the §4.2 example and the owner sketch.
- A3, the §4.5 migration unit.
- A4, the §5 X01-T4 file list.

Amended by gh445 (#445), gh566 §4.2 D5 A5, in one place: the rule 2 snapshot location
(one shared store), marked in the text.

Amended again in rule 8, under gh566 D1:

- By gh566 T2 (#640): execution admission and the censuses moved to the sibling
  support modules `corpus_admission.rs` and `corpus_census.rs`.
- By gh566 T3 (#646): the error vocabulary, citations (including `DocCitation`) and
  record runs moved to the owner's child modules `corpus_error.rs`,
  `corpus_citation.rs` and `corpus_runs.rs`.

## 1. Goal & non-goals

The first-proof conformance entries (F01-T1, F03-T1, F04-T1 and the other direct
consumers listed on gyldlab/keld#532) and the `electron-apps-v0` product corpus
(X02-T4, X02-T5, X02-T6) all emit KEL-74 evidence records against committed corpus
manifests. Today the only manifest contract is test-local to the KEL-237 lifecycle
validator (`crates/keld-compat/tests/lifecycle_corpus.rs`). It hard-codes one pin,
two test targets and a two-state verdict rule, and it has no red-until-implemented
state and no rule for uncited cells. This spec fixes eight evidence rules once,
before any new corpus lands. The observable outcome is a rule set that the X01-T4
validator enforces with the negative controls in §3. Under it, every new corpus
carries one Electron pin and per-cell pinned citations. Its pending cells are
red-until-implemented rather than hidden. Its uncited cells can never score `pass`.
Its `artifact.sha256`, authority label and engine identity each have one declared
meaning, and a run without verified containment is recorded as `unverified`, never
as strict or legacy evidence.

Non-goals:

- The two-arm recorder, normalisers and the KEL-77 fixture-set digest (X01-T1).
- Implementing the shared owner or migrating the lifecycle test (X01-T4, #566).
- Re-recording `electron-lifecycle-v0` at v44.4.5 (a KEL-237 decision).
- Adding a product corpus id to `DOCUMENTED_COMMITTED_PRODUCT_CORPORA` (X02-T6, in
  the same reviewed change that flips the product cells).
- Any change to the KEL-74 denominator format, the verdict set, the `schema` id, or
  the four existing authority values. The one KEL-74 change is the added
  `unverified` authority value (§4.4, task T2).
- Implementing the KEL-78 OS sandbox, or changing how KEL-78 assigns a profile state.
- The Zettlr pin question (its `^43.6.0` range excludes 44.4.5). It is parked until
  Zettlr work starts.

## 2. Spec refs

- `docs/architecture/04-electron-compat.md` §4 (operation evidence ledger, product
  versus showcase panels) and §6 (migration corpus). This spec deviates from neither,
  and architecture 04 is unchanged.
- `docs/specs/kel74-compat-evidence-schema.md` §4.1–§4.3: record fields, denominator,
  the `score()` honesty gate. This PR amends only the §4.1 `authority_profile` row,
  which adds `unverified`. The rest is consumed unchanged.
- `docs/engineering/compat-scoreboard.md`: denominator honesty rules. Consumed unchanged.
- `docs/specs/kel77-bun-child-process-differential.md` §4.3 (fixture-set digest) and
  §4.5 (record mapping and the `legacy_sandbox_off` rationale for a test-process
  harness).
- `docs/specs/kel78-strict-profile-sandbox.md` § Profile states (`unverified`,
  `legacy`, `strict`; "Reporting must say `unverified`").
- Owner decision on Linear KEL-78 (@0monish, 2026-10-07): option (b), an explicit
  `unverified` authority value (§4.4).
- `docs/engineering/electron-compat-reference.md` §20 (L0–L4 maturity levels) and §25
  (status vocabulary, non-normative). The v44.4.5 census pin is the `electron-api.json`
  release asset.
- Epic gyldlab/keld#493 § Maturity ladder (BEHAVIOR_MATCH, CONFORMANCE_PASS,
  CORPUS_VERIFIED as report vocabulary).

## 3. Acceptance criteria (binary, each becomes a test)

Each criterion names the one mutation that must fail it (its negative control).
"Validator" means the X01-T4 owner in §4.2 rule 8. AC13–AC15 test the KEL-74
scorer and parser (task T2). "v1 manifest" means a manifest carrying
`schema: keld.compat.corpus/v1` (§4.2).

1. **One pin.** Given a v1 manifest with a single `upstream` object whose
   `electron_version` is `44.4.5` and whose `electron_commit` is
   `694f45852a0f1726cd23bfd379854de489cccb65`, when the validator runs, then it
   accepts the pin. *Negative control:* a second pin fails with a typed rejection. That
   can be a second upstream object, an array of pins, or a cell-level
   `electron_commit` (unknown field). So does a new v1 corpus whose
   `electron_commit` is `07e460719c75b2ec5ee4893f7d2192ef31c7b8c2`.
2. **Oracle matches pin.** Given a v1 manifest, when the validator runs, then every
   cell's `oracle_id` starts with `electron-v44.4.5.`. Every evidence record for the
   corpus has `operation.oracle.revision` equal to
   `electron-v44.4.5@694f45852a0f1726cd23bfd379854de489cccb65`. *Negative control:*
   one cell's `oracle_id` changed to the `electron-v44.3.0.` prefix, or one record's
   revision changed to the 44.3.0 commit, is rejected.
3. **Per-cell citation.** Given a v1 cell whose `expected_verdict` is `pass` or
   `fail`, when the validator runs, then the cell carries `doc_citation`. Its `url`
   is an `https://github.com/electron/electron/blob/` URL at exactly the corpus
   `electron_commit` under `docs/`, and its `quote_sha256` equals `sha256:` plus the
   SHA-256 of the UTF-8 bytes of `quote`. *Negative control:* a one-byte edit to
   `quote` without a new digest is rejected, and so is a `url` at any other commit.
4. **Red-until-implemented.** Given a v1 cell with `expected_verdict: fail`, when the
   validator runs, then the cell carries exactly one of `intentional_divergence` and
   `implementing_ticket`. `implementing_ticket` matches `GH-` or `KEL-` plus decimal
   digits. The mapped test passes execution admission. *Negative control:* deleting
   `implementing_ticket` from a red fixture cell is rejected (a `fail` cell with
   neither key), and so is a red cell that carries both keys. *Positive control:* a
   red cell with only `intentional_divergence` and no implementing-ticket key is
   accepted as a divergence cell and reported as a scoreboard ▲, never as pending
   work.
5. **Flip.** Given the implementing change for a red cell, when the validator runs,
   then the cell has `expected_verdict: pass`, has no `implementing_ticket`, and its
   mapped test asserts the cited behaviour. *Negative control:* a `pass` cell that
   keeps `implementing_ticket` is rejected.
6. **Uncited means unknown.** Given a v1 cell with no `doc_citation`, when the
   validator runs, then its `expected_verdict` is `unknown` and every record for the
   cell has `result: unknown`. *Negative control:* the same uncited cell with
   `expected_verdict: pass` (or `fail`) is rejected, and so is a `result: pass`
   record for it.
7. **One digest meaning.** Given a v1 manifest declaring
   `artifact_digest: manifest_bytes`, when the validator runs, then every record's
   `artifact.sha256` and the denominator's `corpus_sha256` equal `sha256:` plus the
   SHA-256 of the exact committed manifest bytes. *Negative control:* flipping one
   manifest byte fails the digest assertion (the live KEL-237 control). Omitting
   `artifact_digest`, or giving any other value, is rejected.
8. **Authority label.** Given a product-panel (corpus-app) run whose receipt records
   the KEL-78 `unverified` state, when the validator runs, then the run has a record
   and that record has `authority_profile: unverified`. Given a corpus-app record with
   `authority_profile: legacy_sandbox_off`, it is admitted only if the run's receipt
   records the KEL-78 `legacy` state from the explicit Keld legacy declaration.
   *Negative control:* an `unverified` receipt with no record for its cell is
   rejected, because the run must not be dropped. A `legacy_sandbox_off` corpus-app
   record whose receipt does not record the `legacy` state is also rejected. So is a
   `strict_bun` label on a conformance-harness record.
9. **Engine identity.** Given a v1 manifest whose `engine` maps each admitted
   platform to one identity, when the validator runs, then every record's
   `revisions.engine` equals that identity, then `@`, then a non-empty pinned
   revision, for the record's `artifact.platform`. *Negative control:* a record whose
   engine identity differs from the declared one is rejected. So is a record for a
   platform absent from the map.
10. **One owner.** Given the keld-compat test sources after X01-T4, then exactly one
    manifest-parsing definition and one exact-bytes digest helper exist, both in
    `crates/keld-compat/tests/support/corpus_manifest.rs`. Both lifecycle test targets
    use them, `crates/keld-compat/src/lib.rs` exports no new module, and `sha2` stays
    in `[dev-dependencies]`. *Negative control:* a second `sha256_uri` definition in
    `crates/keld-compat/tests/` fails the check, and so does a `pub mod` manifest
    module in `src/`.
11. **Frozen corpus.** Given the X01-T4 migration, then `electron-lifecycle-v0`'s
    `corpus.json` bytes, its denominator digest
    `sha256:badc0aaf3619168927cf464e2dd0006a599b5614a35b84960c59984b18e0e8b2` and
    its nine evidence records are byte-identical to `origin/main`. The v0 manifest
    shape (no `schema` field) is admitted only for that corpus id. *Negative
    control:* a new corpus id using the v0 shape is rejected. Changing one v0 cell's
    `oracle_id` to `electron-v44.4.5.` inside the 44.3.0 corpus is also rejected.
12. **No relabelling.** Given a corpus-app record bound to a receipt whose profile
    state is `unverified`, when the validator runs, then it is admitted only with
    `authority_profile: unverified`. *Negative control:* the same record labelled
    `legacy_sandbox_off` is rejected, and so is the same record labelled
    `strict_bun`. Each rejection names the receipt state and the record label.
13. **No aggregation merge.** Given a two-cell showcase denominator filled by two
    `pass` records that agree on artifact digest and engine, where one record is
    `unverified` and the other is `legacy_sandbox_off`, when `score` runs, then
    `complete` is false and `unweighted_percent` is `None`. The same holds when the
    other record is `strict_bun`, `sandboxed_addon_worker` or
    `user_approved_tool_child`. *Negative control:* a `parse_authority` that maps
    `"unverified"` to `LegacySandboxOff` (or to `StrictBun`) makes this test fail.
    So does a profile comparison in the `contributing_identity_consistent` check
    (`evidence.rs:1415` at 9d26d488) that treats `Unverified` as equal to another
    variant.
14. **No scoring promotion.** Given a scoreboard whose contributing records are all
    `unverified`, when `score` runs, then `Scoreboard::authority_profile()` is
    `Some(Unverified)` and renders as `unverified`. Given a scoreboard whose records
    mix `unverified` with any other value, it is `None`. The `claim` string contains
    neither `strict_bun` nor `legacy_sandbox_off`, and its format is unchanged (KEL-74
    §4.3 rule 7). *Negative control:* an accessor that returns the first record's
    profile for a mixed board fails the test. So does one that reports
    `LegacySandboxOff` or `StrictBun` for an all-`unverified` board, and so does a
    claim or report line that names `strict_bun` or `legacy_sandbox_off` for an
    `unverified` run.
15. **Distinct vocabulary.** Given each of the five authority strings, when a v1
    record carrying it is parsed, then `parse_authority` returns its own variant,
    `AuthorityProfile::as_str` returns the same string, and `Unverified` is unequal
    to each of the other four variants. The record `schema` stays
    `keld.compat.evidence/v1`. *Negative control:* a sixth, unknown string still
    fails with `KELD-COMPAT-005`, and swapping any two `as_str` arms fails the round
    trip.
16. **Quote is in the pinned page.** Given a v1 cell with `doc_citation`, when the
    validator runs offline, then the snapshot at
    `doc-snapshots/<electron_commit>/<page path>` has the SHA-256 declared in
    `doc_snapshots`, and `quote` is a byte-exact substring of it. The substring check
    runs before the `quote_sha256` check (§4.2 rule 2). *Negative control:* a cell
    whose `quote` is absent from the pinned page is rejected with a quote-absent
    error that names the cell and the page, even when its `quote_sha256` is the
    correct digest of that fabricated quote. So is a snapshot whose bytes differ from
    its `doc_snapshots` digest, a cited page with no `doc_snapshots` entry or no
    snapshot file, and a snapshot filed under another commit's directory.
17. **Pending is reported apart from divergence.** Given a corpus with one cell that
    carries `implementing_ticket` and one that carries `intentional_divergence`
    (both `expected_verdict: fail`, so both records say `result: fail`), when the
    report renders through the shared `FailSplit` renderer, then it counts and labels
    them separately, as "Pending implementation" (listing the ticket keys) and
    "Intentional divergence". Every v1 report renders its fail counts through
    `FailSplit`, and a source census enforces that. The frozen v0 lifecycle report
    cannot carry a pending cell, so it keeps its own column (amended by gh566 (#639),
    A1). The split comes from the manifest key, because the records cannot
    tell the two apart. *Negative control:* a report that lumps them into one count
    or one label fails, and so does one that labels a pending cell "Intentional
    divergence" or shows it as ▲.

## 4. Design

### 4.1 First-principles and reuse decision

No boundary change. This spec moves no handle ownership, crash ownership or
principal minting. It adds no production dependency and no wire format. The manifest
is a test-fixture format, read only by keld-compat integration tests. The one
public-contract change is task T2: the `unverified` value in the KEL-74 authority
vocabulary, a `Scoreboard::authority_profile()` accessor, and no `schema` change
(§4.4).

Live facts (origin/main at 9d26d488):

- FACT: the manifest is parsed by a test-local `Manifest` / `Upstream` /
  `CorpusCell` struct set with `deny_unknown_fields`. Its fields are `corpus_id`,
  `scope`, `panel`, `kind`, `upstream` (`electron_version`, `electron_commit`,
  `app_docs`) and `cells`. The cell fields are `operation_id`, `oracle_id`,
  `expected_verdict`, `test_path`, `test_name`, `negative_control`, and optional
  `intentional_divergence`.
- FACT: the validator hard-asserts `44.3.0` @ `07e460719c75…`. It admits
  `RUST_TEST_PATH` and `TS_TEST_PATH` only. `expected_verdict` is `pass` (no
  divergence) or `fail` (with a non-empty `intentional_divergence`), and anything
  else panics.
- FACT: the KEL-74 verdict set is closed at `pass | fail | unknown | waived`, and
  records reject unknown fields. The authority set is `strict_bun |
  sandboxed_addon_worker | legacy_sandbox_off | user_approved_tool_child`, and any
  other string fails `parse_authority` with `KELD-COMPAT-005`.
- FACT: `AuthorityProfile` is an exhaustive public enum with a derived `PartialEq`. It
  has no `as_str` and no `Serialize`, because records are parsed and never written by
  keld-compat. Records are written as JSON text by their producers, for example the
  `AUTHORITY_PROFILE` constant in `keld-runtime/tests/child_process_differential.rs`.
  `score()` compares profiles only in `contributing_identity_consistent`
  (`evidence.rs:1415`). Outside `evidence.rs`, the only uses of a variant are two test
  assertions on `LegacySandboxOff`. One is in `child_process_differential.rs:854` and
  the other is in `lifecycle_evidence_report.rs:454`.
- FACT: KEL-74 versions its ledger with a closed `schema` string
  (`EVIDENCE_SCHEMA = "keld.compat.evidence/v1"`, `evidence.rs:17`). An unknown
  `schema` fails with `KELD-COMPAT-004`. Every value vocabulary inside v1 is a closed
  `match`.
- FACT: KEL-78 § Profile states makes `unverified` the default state, and it is the
  live state on macOS, Windows and Linux. Its table says "Reporting must say
  `unverified`". `legacy` comes only from an explicit Keld profile key.
- FACT: `artifact.sha256` in the lifecycle records equals the denominator's
  `corpus_sha256`, which is the SHA-256 of the exact `corpus.json` bytes. KEL-77 §4.5
  uses the length-framed fixture-set digest of §4.3 instead.
- FACT: `sha256_uri` is defined twice, in `tests/lifecycle_corpus.rs` and in
  `tests/lifecycle_evidence_report.rs`. This is a live duplicate that X01-T4 removes.
- FACT: the lifecycle records are bound per OS to the PR #242 hosted-CI receipts
  (`receipts/*.json`, `evidence_uri` is their digest). A re-pin needs fresh receipts
  on three OS, so it is not a text edit.
- FACT: the Electron `v44.4.5` tag object `399d8d62…` peels to commit
  `694f45852a0f1726cd23bfd379854de489cccb65` (GitHub API, fetched 2026-10-07).
  `docs/api/app.md` at that commit contains the sentence used in §4.2's example.

Atoms (each falsified independently by the §3 control named):

| Atom | Owner | Input → output | Failure mode | Observable |
|---|---|---|---|---|
| Pin | manifest `upstream` | manifest → one pin | two pins feed one claim | AC1, AC2 |
| Citation | cell `doc_citation` | cell → pinned page + quote digest | quote drifts from pin | AC3 |
| Quote source | manifest `doc_snapshots` | cell + committed snapshot → quote found or rejected | fabricated quote with a matching digest | AC16 |
| Red cell | cell `implementing_ticket` | pending cell → `fail` + key | pending work read as ▲ or hidden | AC4, AC5 |
| Pending report | report renderer | manifest key + `fail` record → one labelled count | pending work lumped into ▲ | AC17 |
| Observed-only | cell without citation | cell → `unknown` | uncited cell inflates score | AC6 |
| Digest meaning | manifest `artifact_digest` | bytes → `artifact.sha256` | two meanings in one corpus | AC7 |
| Authority | receipt profile state | run → one label | `unverified` run dropped or relabelled | AC8, AC12 |
| Aggregation | `score()` identity check | records → consistent or not | `unverified` merged with another profile | AC13 |
| Reporting | `Scoreboard::authority_profile()` | board → one profile or `None` | `unverified` board reported as strict or legacy | AC14 |
| Vocabulary | `parse_authority` and `as_str` | string ↔ variant | `unverified` aliased to another variant | AC15 |
| Engine | manifest `engine` | platform → identity | mixed engines in one row | AC9 |
| Owner | test support module | all corpora → one parser | parallel validators drift | AC10, AC11 |

Reuse: the eight rules extend the KEL-237 manifest and reuse KEL-74's parser,
`score()` and its closed vocabularies. The one widening is the `unverified` value,
which is added to the owning `parse_authority` table rather than to a parallel
mapping. Nothing is rewritten. Compatibility fallback:
the v0 manifest shape stays readable for `electron-lifecycle-v0` only, until KEL-237
re-records it. Performance claim: none.

### 4.2 The eight rules

Field names in `code` that already exist are live. Those marked *(addition)* are new
in the v1 manifest and are introduced by X01-T4.

**Rule 1 — Pin.** A corpus manifest carries exactly one Electron upstream pin:
`upstream.electron_version` plus `upstream.electron_commit`, a full 40-hex commit.
Every new corpus pins `44.4.5` @ `694f45852a0f1726cd23bfd379854de489cccb65`. The
validator holds the admitted-pin list, and a different pin needs a reviewed
amendment to this spec. A cell's `oracle_id` starts with `electron-v` plus the
corpus `electron_version` plus `.`. Each record's `operation.oracle.revision` is
`electron-v` plus the version, `@`, then the commit. That is the live lifecycle form
(`electron-v44.3.0@07e46071…`). A pin bump is one reviewed change. It re-cites every
cell (new `url` commit, re-verified `quote`, new `quote_sha256`, new committed
snapshot and `doc_snapshots` digest), renames every
`oracle_id`, and regenerates the denominator, every receipt and every record. A
partial bump fails AC2 and AC3, and stale records fail AC7, because the manifest
bytes changed. `electron-lifecycle-v0` stays frozen at `44.3.0` @ `07e46071…` until
KEL-237 deliberately re-records it. New v44.4.5 lifecycle cells go in a new corpus
id, chosen by the first consumer that adds them.

**Rule 2 — Doc citation.** Each cited cell carries `doc_citation` *(addition)* with:

- `url`: the doc page at the pinned commit. Its prefix is
  `https://github.com/electron/electron/blob/`, then the corpus `electron_commit`,
  then `/docs/`, and it may end with an anchor. It satisfies the KEL-74 immutable-URI
  rule.
- `quote`: the cited sentence as a byte-exact substring of that page's Markdown
  source, with LF line breaks kept.
- `quote_sha256`: `sha256:` plus the lowercase hex SHA-256 of the UTF-8 bytes of
  `quote`.

The page text at the pin is available offline from a committed snapshot. Each v1
manifest declares `doc_snapshots` *(addition)*, a map from every cited page path
(for example `docs/api/app.md`) to `sha256:` plus the SHA-256 of that page's exact
bytes at the pin. The snapshot file sits in the one store that every corpus reads,
`crates/keld-compat/fixtures/doc-snapshots/<electron_commit>/<page path>`, so a page
cited by two corpora at one pin is committed once (amended by gh445 (#445), gh566 A5).
The commit is part of its path, so a
snapshot from another pin cannot satisfy this one. It holds the cited pages only, not
the docs tree. Because `doc_snapshots` is in the manifest, AC7 binds the snapshot
digests to the manifest bytes. The validator checks each citation in this order, with
no network:

1. Take the page path from `url`, without the anchor, and read the snapshot file.
2. Assert the snapshot's SHA-256 equals its `doc_snapshots` entry.
3. Assert `quote` is a byte-exact substring of the snapshot.
4. Only then assert `quote_sha256` (AC3).

The one online step is review. At the citing change, and again at every pin bump, the
reviewer compares each `doc_snapshots` digest with the upstream file at the pin
(`curl -sL https://raw.githubusercontent.com/electron/electron/<electron_commit>/<page path> | shasum -a 256`).
That is one reproducible digest per page and pin. Every later check is mechanical, so
a fabricated quote cannot pass by carrying a matching `quote_sha256`. The corpus-level
`upstream.app_docs` is a v0 field and is not part of v1.

**Rule 3 — Red-until-implemented.** A pending cell is `expected_verdict: fail`
together with `implementing_ticket` *(addition)*, which is the key of the ticket
whose change implements it (`GH-` or `KEL-` plus digits). It carries no
`intentional_divergence`. A `fail` cell has exactly one of the two keys:
`intentional_divergence` is permanent and reported ▲, and `implementing_ticket` is
pending and reported as failing work owned by that ticket. Both records say
`result: fail`, so the report takes the split from the manifest key and renders
"Pending implementation" and "Intentional divergence" as separate counts (AC17). The
mapped test asserts
Keld's current behaviour and passes, and execution admission still requires it to
run and pass. The implementing change, in one PR, makes four edits: it inverts the
mapped test to assert the cited sentence, sets `expected_verdict: pass`, removes
`implementing_ticket`, and regenerates the denominator, receipts and records. This
is the live pattern of a passing test that asserts a divergence, and it adds no new
verdict.

**Rule 4 — Observed-only is `unknown`.** A cell without `doc_citation` has
`expected_verdict: unknown`, which is an existing KEL-74 verdict and is newly
admitted in the manifest. Every record for it is `result: unknown`, it carries no
`intentional_divergence` or `implementing_ticket`, and `score()` then withholds
`unweighted_percent`. Ladder rungs are report vocabulary over the KEL-74 verdicts.
They never appear in a manifest or record, and both reject unknown fields.

| Rung (report vocabulary) | Record verdict | Counts toward |
|---|---|---|
| BEHAVIOR_MATCH (transcript or observation match, cited or not) | `unknown` | no family exit |
| CONFORMANCE_PASS (cited sentence, Keld-owned test, named negative control, per OS lane) | `pass` | family **L2** exit |
| CORPUS_VERIFIED (same observable in a committed product corpus via migrate + dev smoke) | `pass` on panel `product` | family **L3** exit |

A family L2 exit requires every cell the family epic names for L2 to be
CONFORMANCE_PASS on each declared OS lane. A family L3 exit requires CORPUS_VERIFIED
rows, which exist only after X02-T6 documents the corpus id as committed. Red,
divergence (▲), waived, `unknown` and missing cells hold no rung and satisfy no exit.
An unrun lane emits no record (missing) or `unknown`. L0 and L1 stay family-owned
and need no rung.

**Rule 5 — `artifact.sha256` meaning.** Each v1 manifest declares
`artifact_digest` *(addition)*. The v1 closed set is `manifest_bytes`.

| Corpus kind | `artifact.sha256` carries | Milestone |
|---|---|---|
| Conformance corpus (panel `showcase`, Keld-owned tests; `electron-lifecycle-v0` and new v44.4.5 corpora) | SHA-256 of the exact committed manifest bytes (= denominator `corpus_sha256`) | first-proof (live KEL-237 convention) |
| Product corpus (panel `product`, corpus-app rows; `electron-apps-v0`) | SHA-256 of the exact committed manifest bytes | first-proof |
| Differential fixture corpus (KEL-77, X01-T1) | KEL-77 §4.3 fixture-set digest | next. X01-T1 adds its declared value in its own spec, and v1 rejects it |

**Rule 6 — Authority label.** A corpus-app (product-panel) run carries a KEL-78
profile state in its receipt, and its record's label follows that state exactly:

| Receipt profile state (KEL-78) | `authority_profile` | Admitted when |
|---|---|---|
| `unverified` | `unverified` | always. The run is recorded, never dropped |
| `legacy` | `legacy_sandbox_off` | the receipt records the explicit Keld legacy declaration |
| `strict` | `strict_bun` | the receipt records KEL-78 admission plus the complete OS-containment archive |

`unverified` means the run had no verified containment. It is never strict or legacy
evidence, and no validator, scorer or report may promote it to another value, merge
it with another value or relabel it (AC12–AC15). A conformance-harness record keeps
the live `legacy_sandbox_off` label. It comes from an ordinary test process with no
Keld session and so has no KEL-78 state (KEL-237 report, KEL-77 §4.5), and it is
never `strict_bun` (§10 Q2 records the scope of this exception). The product receipt
schema (X02-T5) names the receipt field. KEL-74 task T2 adds the `unverified` value
(§4.4).

**Rule 7 — Engine identity.** Each v1 manifest declares `engine` *(addition)*. It
maps each admitted `artifact.platform` value (`macos`, `windows`, `linux`) to one
identity token, for example `headless-lifecycle-conformance` (the live lifecycle
identity) or `wkwebview`. A record's `revisions.engine` is that token, then `@`, then
a pinned revision, matching the live `headless-lifecycle-conformance@38db257b…` form.
Each platform is scored separately, which is the live report practice, so `score()`'s
engine-consistency check compares like with like.

**Rule 8 — One owner.** One module,
`crates/keld-compat/tests/support/corpus_manifest.rs`, owns manifest parsing (v1,
plus the frozen v0 shape for `electron-lifecycle-v0` only), the exact-bytes digest
helper with its one-byte mutation control, and execution admission (the Rust libtest
and Bun case-result checks). It also owns pin, citation, red-cell and uncited-cell
validation, and the per-corpus registry of admitted test targets. The registry is
code, so a manifest cannot admit its own targets. Test targets include it with
`#[path = "support/corpus_manifest.rs"] mod corpus_manifest;`, which is the
keld-ipc convention. X01-T4 (#566) implements it and migrates `lifecycle_corpus.rs`
and `lifecycle_evidence_report.rs` byte-for-byte. Under gh566 D1's review condition,
the owner's responsibility is split across five modules (amended by gh566 T2, #640,
and T3, #646):

- Two sibling support modules: execution admission in
  `tests/support/corpus_admission.rs`, and the owner and fixture censuses in
  `tests/support/corpus_census.rs`.
- Three child modules that the owner declares itself:
  - the `CorpusError` vocabulary, in `tests/support/corpus_error.rs`;
  - citations and snapshots, in `tests/support/corpus_citation.rs`, which defines the
    nested `DocCitation` cell shape;
  - record runs and `FailSplit`, in `tests/support/corpus_runs.rs`.

Manifest bytes are deserialized only in `corpus_manifest.rs`, which includes the
`DocCitation` nested in each v1 cell, and only that file holds the digest helper.

Each v1 cell also declares `platforms`, a non-empty set of distinct platform tokens
(`macos`, `windows`, `linux`). The set is bounded by the platforms the code registry
admits for the corpus. Execution admission requires exactly one passing mapped case
on each declared platform. On an undeclared platform the cell is an unrun lane: it is
reported `unknown` (rule 4), and any record for it says `result: unknown` (amended by
gh566 (#639), A2).

Example v1 manifest with one cited, passing cell. The pin, URL, quote, digest and
test name are real. The corpus id is illustrative, because the first consumer
chooses it:

```json
{
  "schema": "keld.compat.corpus/v1",
  "corpus_id": "electron-lifecycle-v1",
  "scope": "@keld/electron app lifecycle cells at Electron 44.4.5; not median-app product compatibility",
  "panel": "showcase",
  "kind": "primary_workflow",
  "artifact_digest": "manifest_bytes",
  "engine": { "macos": "headless-lifecycle-conformance" },
  "doc_snapshots": {
    "docs/api/app.md": "sha256:49238ddf50585d8a581bd7b5141ec825216cd9b82e6bc82bd97cad4200593ebf"
  },
  "upstream": {
    "electron_version": "44.4.5",
    "electron_commit": "694f45852a0f1726cd23bfd379854de489cccb65"
  },
  "cells": [
    {
      "operation_id": "app.when-ready.host-ready-gate",
      "oracle_id": "electron-v44.4.5.app.when-ready-initialized",
      "doc_citation": {
        "url": "https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md#appwhenready",
        "quote": "Returns `Promise<void>` - fulfilled when Electron is initialized.",
        "quote_sha256": "sha256:da3a396b4213c28480a7800d50256736f7fcc6d5dda4bb0cb3fa570e1d63291a"
      },
      "expected_verdict": "pass",
      "platforms": ["macos"],
      "test_path": "crates/keld-compat/tests/electron_lifecycle.rs",
      "test_name": "when_ready_does_not_resolve_before_host_ready_event",
      "negative_control": "Replacing whenReady with Promise.resolve() makes the mapped conformance test fail before host Ready."
    }
  ]
}
```

A red-until-implemented cell differs only in `"expected_verdict": "fail"` plus
`"implementing_ticket"`, which names the ticket that will flip it. Its `test_name`
names the passing test that asserts the current behaviour.

Owner sketch (test-only Rust; X01-T4 owns the final form):

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellV1 {
    pub operation_id: String,
    pub oracle_id: String,
    #[serde(default)]
    pub doc_citation: Option<DocCitation>,   // addition
    pub expected_verdict: String,            // "pass" | "fail" | "unknown"
    #[serde(default)]
    pub intentional_divergence: Option<String>,
    #[serde(default)]
    pub implementing_ticket: Option<String>, // addition
    pub platforms: Vec<String>,              // addition (amended by gh566 (#639), A2)
    pub test_path: String,
    pub test_name: String,
    pub negative_control: String,
}

/// The one exact-bytes helper (today duplicated in two test targets).
pub fn sha256_uri(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
```

### 4.3 Rejected alternatives

- **Per-family pins.** Families on different Electron commits would feed one claim
  line with sentences that may differ between pins (the falsifier on #532). One pin
  per corpus, with v44.4.5 as the default, keeps every cell in a corpus on one text.
- **Re-pinning `electron-lifecycle-v0` in place without fresh receipts.** Its records
  are bound per OS to PR #242's hosted-CI receipts and to the manifest digest. Editing
  the pin changes the digest and orphans those receipts, and it would publish
  44.4.5 records that no run produced. KEL-237 owns a deliberate re-record.
- **A public keld-compat manifest module.** It would add public API and move `sha2`
  into production dependencies, which the crate manifest records as rejected (KEL-74
  T1). Only tests read manifests.
- **Checking `quote` only in review, or fetching the page in the PR lane.** The first
  is not repeatable after a later corpus change, and the second makes CI depend on
  the network. A committed snapshot with its own digest gives an offline,
  commit-keyed check (AC16).
- **A new verdict value for pending cells** (for example `pending`). The KEL-74
  parser closes the set at four values, and a fifth would change the versioned ledger
  format and `score()`. `fail` plus `implementing_ticket` expresses the same state.
- **Reusing `intentional_divergence` for pending cells.** That would report pending
  work as a permanent ▲ and drop the implementing owner.
- **Admitting an uncited cell as `pass`, or rungs as verdicts.** Either inflates the
  score past what a pinned sentence supports.
- **Two owners**, one in test support and one public. That is parallel validators,
  which root `AGENTS.md` names as a defect.
- **Dropping `unverified` runs** (option (a), the earlier draft of rule 6). It hides
  the live state of every OS behind `missing` and contradicts KEL-78's "Reporting
  must say `unverified`". The owner rejected it (§4.4).
- **Labelling `unverified` runs `legacy_sandbox_off`.** `legacy` needs the explicit
  Keld declaration and prints a forfeit, so the label would claim a declaration that
  no one made.
- **A `keld.compat.evidence/v2` schema for the new value.** The reasons are in §4.4.

### 4.4 Owner decision and schema version

Decision: the repository owner (@0monish) chose option (b) on Linear KEL-78 on
2026-10-07. The KEL-74 authority vocabulary gets an explicit `unverified` value.
Runs whose KEL-78 profile state is `unverified` are recorded with that value and are
not dropped. `unverified` stays distinct from `strict_bun`,
`sandboxed_addon_worker`, `legacy_sandbox_off` and `user_approved_tool_child`, and
in particular from strict and legacy. Negative controls prove that it is never
promoted, merged or relabelled in the row label (AC12), in aggregation (AC13), in
scoring and reporting (AC14) or in the vocabulary (AC15). The KEL-78 OS sandbox is
not part of this work.

Schema version: no bump. The record `schema` stays `keld.compat.evidence/v1`, and
T2 adds `unverified` as a fifth arm of the closed v1 `authority_profile` set. The
reasons are these:

- The change is additive. Every v1 record that parses today still parses, with the
  same variant and the same meaning, including the frozen lifecycle records (AC11)
  and the KEL-77 records.
- A parser built before T2 rejects an `unverified` record with `KELD-COMPAT-005`.
  It fails closed and cannot misread the value as another profile.
- A v2 id would force a dual-version reader for those frozen v1 records with no
  change of meaning for any of them, which is a second parse path for one format.

Falsifier: a reader that must tell a four-value v1 record from a five-value one, and
cannot treat `KELD-COMPAT-005` as the answer. In that case T2 bumps to v2 with a
v1 fallback reader instead. The format review gate on T2 (§8) checks this decision.

### 4.5 Other template items

- Capabilities / manifest (spec 03): none.
- Wire/protocol (spec 02): none.
- Platform notes: the rules are platform-neutral, and the first consumer is macOS
  first-proof cells. Records are scored per platform.
- Runtime seam: none.
- Migration unit: the callers are `tests/lifecycle_corpus.rs`,
  `tests/lifecycle_evidence_report.rs` and the new `tests/corpus_registry/main.rs`
  (amended by gh566 (#639), A3). Persisted state is the committed
  `fixtures/lifecycle-corpus/*` bytes, which stay unchanged. There is no temporary
  adapter. The v0-shape reader is the retained compatibility facade, and its removal
  condition is KEL-237 re-recording that corpus in the v1 shape with fresh receipts.

## 5. Boundaries

- Implement in (this spec): `docs/specs/gh532-first-proof-evidence-rules.md` and the
  `authority_profile` row of `docs/specs/kel74-compat-evidence-schema.md` §4.1. No
  Rust changes. Neither file is an `llms.txt` source, so the generated files stay
  unchanged.
- Implement in (T2): `crates/keld-compat/src/evidence.rs` (the `AuthorityProfile`
  variant, `parse_authority`, `as_str`, the `Scoreboard` accessor and the colocated
  tests) and `docs/engineering/compat-scoreboard.md` (the reporting rule). The
  scoreboard is an `llms.txt` source, so T2 also regenerates `llms-full.txt` with
  `just llms` and passes `just llms-check`.
- Implement in (X01-T4): `crates/keld-compat/tests/support/corpus_manifest.rs`,
  `crates/keld-compat/tests/lifecycle_corpus.rs`,
  `crates/keld-compat/tests/lifecycle_evidence_report.rs`, and
  `crates/keld-compat/tests/corpus_registry/` (amended by gh566 (#639), A4). The `doc-snapshots/` files
  are committed by the change that adds a v1 corpus, not by X01-T4, whose AC16 cases
  use in-test snapshots.
- Must not touch: the rest of `crates/keld-compat/src/` (other vocabularies, the
  `schema` ids, `DOCUMENTED_COMMITTED_PRODUCT_CORPORA`), `crates/keld-compat/Cargo.toml`
  dependencies, the bytes under `crates/keld-compat/fixtures/lifecycle-corpus/`,
  `docs/architecture/`, `docs/research/`, and the KEL-78 sandbox code.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 This spec (X01-T3, #532): rules, ACs and rejected alternatives, reviewed to
      `Status: approved`.
- [ ] T2 KEL-74 `unverified` authority value (tracked on #532, decision on KEL-78).
      Add `AuthorityProfile::Unverified` with a doc comment ("run without verified
      containment; never strict or legacy evidence"). Add the `"unverified"` arm to
      `parse_authority`. Add `AuthorityProfile::as_str` as the one variant-to-string
      map, which records, reports and the AC14 render use. Add
      `Scoreboard::authority_profile()`, which is `Some` only when every contributing
      record shares one profile. Keep `schema` at `keld.compat.evidence/v1` (§4.4) and
      add the AC13–AC15 colocated unit tests, each with its named mutation. Add the
      reporting rule to `compat-scoreboard.md`. It lands before T3, because the T3
      validator must parse `unverified` records.
- [ ] T3 X01-T4 (#566): its own spec gate, then the shared owner, with AC1–AC12,
      AC16 and AC17 as tests and the byte-identical lifecycle migration.

The consumer edges are tracker actions, not PRs, and the F01–F09 and X02 repairs own
them.

## 7. Test plan

| AC | Test (X01-T4 in `crates/keld-compat/tests/` unless marked T2) | Kind |
|---|---|---|
| 1–7, 9 | validator unit cases on in-test v1 fixture manifests and records, one accept case plus the named mutation each | integration (test-only) |
| 8, 12 | product receipt fixtures with `unverified` and `legacy` profile states, each paired with every label, plus a receipt with no record | integration (X01-T4) |
| 13, 14 | `score()` unit cases in `evidence.rs`: `unverified` paired with each other profile, and an all-`unverified` board | unit (T2) |
| 15 | `parse_authority` and `as_str` round trip over all five strings, plus an unknown string | unit (T2) |
| 16 | validator cases on an in-test snapshot: a fabricated quote with its correct `quote_sha256`, a tampered snapshot, a missing entry or file, and a snapshot under another commit | integration (X01-T4) |
| 17 | report render over one pending and one divergence cell: assert the two labelled counts, then the lumped-report mutation | integration (X01-T4) |
| 10 | source census over `crates/keld-compat/tests/**/*.rs` and `src/lib.rs` exports; `cargo metadata` dependency kind for `sha2` | integration |
| 11 | digest and byte equality of the committed lifecycle fixtures against constants pinned from `origin/main` | integration |

Anti-flake: the tests use no clock (`as_of` stays explicit), no network (citations
are checked against committed snapshots, AC16, and only the snapshot-versus-upstream
digest comparison is a review step), and no ports. Execution
admission keeps the live Rust and Bun exact-case controls. Run with `CLAUDECODE` and
`AI_AGENT` unset (KEL-237 comment 6964244b).

## 8. Review gates triggered

This PR (T1) is documentation only and triggers none.

- T2: public API yes, because `keld_compat::evidence` gains a variant on an
  exhaustive public enum and the `Scoreboard::authority_profile()` accessor. A
  downstream exhaustive `match` breaks, and the workspace has none. Format review
  yes, because the versioned JSON ledger vocabulary widens without a `schema` bump
  (§4.4). `unsafe`, permission model, dependency addition and kipc wire protocol:
  none.
- T3 (X01-T4): none. It is test-only, `sha2` stays a dev-dependency, and it has no
  wire protocol change.

## 9. Perf impact

none. These are cold test-time JSON checks, off every budgeted path.

## 10. Open questions

1. **Source-receipt oracles.** Some planned cells have a pinned source receipt as
   their oracle rather than a doc sentence: F02-T1 cell 8, the F06-T1 role-fallback
   cell, the F07-T4 require-map cell, and the F09-T3 electron-updater cells. Draft
   decision (delegated, reversible): v1 admits only Electron doc citations, so these
   cells stay `unknown` under rule 4. A later amendment may add a second citation kind
   at the same pin. electron-updater is outside the Electron repository, so it would
   also need its own pin rule. Falsifier: a first-proof exit that cannot be met
   without one of these cells reaching `pass`.
2. **Scope of the harness label.** The owner decision (§4.4) restricts
   `legacy_sandbox_off` to runs with the explicit legacy declaration. Draft decision
   (delegated, reversible): it governs runs that carry a KEL-78 profile state, which
   are the corpus-app runs. Conformance-harness records keep `legacy_sandbox_off`,
   because they come from a plain test process with no KEL-78 state, the frozen
   lifecycle records use it (AC11), and KEL-77 §4.5 relies on it. Falsifier: the
   owner reads the decision as covering harness records. In that case new harness
   corpora are labelled `unverified`, and `electron-lifecycle-v0` stays frozen until
   KEL-237 re-records it.
