# Spec: generic compatibility evidence schema (KEL-74)

Status: implementing
Linear: KEL-74 · Owner: GYLDLAB · Updated: 2026-08-19

## 1. Goal & non-goals

Keld needs a framework-generic, versioned evidence record so “compatibility”
is a measured cell against a committed denominator, not a slogan. One hostile-input-safe
parser in `keld-compat` accepts a v1 JSON evidence record and a v1 denominator,
classifies a shipped artifact by magic bytes, and scores results without allowing a
partial measurement to become a 100% compatibility claim. VS Code is a later showcase
corpus consumer, not a schema input.

Non-goals:

- A VS Code, marketplace, or package-name database.
- An Electron shim, `@keld/electron` change, or new `Tier` variant.
- Full import/signing/N-API scanners, VSIX adapters, `keld doctor` UI, or published
  public percentages.
- Weighted scoreboards, notebook/IME/a11y browser splits, or a live corpus harness.
- Architecture 02/03 edits, `docs/research/`, or competing PRs.

## 2. Spec refs

- `docs/architecture/04-electron-compat.md` §4 (public scoreboard; this spec adds
  operation/workflow evidence beside API ✔/▲/✘ — same PR updates §4).
- `docs/architecture/01-overview.md` §2 (host owns OS; JS owns the app).
- `docs/architecture/07-agent-experience.md` §2 (typed `KELD-*` errors with a fix).
- `docs/engineering/compat-scoreboard.md` (denominator honesty; this PR).
- Electron API scoreboard rows stay narrative until a committed product denominator
  exists. This schema does not replace Electron docs as the API oracle.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a well-formed `keld.compat.evidence/v1` JSON object under the size cap, when
   `parse_evidence` runs, then it returns a record with artifact SHA-256, platform,
   architecture, Keld/Bun/engine revisions, authority profile, operation + semantic
   oracle, verdict, and an allowed immutable evidence URI.
2. Given bytes larger than the cap, non-UTF-8, trailing junk, unknown `schema`,
   unknown fields, or a malformed `sha256:` digest, when parse runs, then it returns
   the matching `KELD-COMPAT-*` error whose message states the fix.
3. Given `result: "waived"` without owner/reason/`expires_on`, or a waiver on any
   other verdict, when parse runs, then `KELD-COMPAT-006`.
4. Given an evidence URI that is a `file:`, sandbox path, `/tmp/` path, or opaque
   turn citation (`turn0…`), when parse runs, then `KELD-COMPAT-007`. Those strings
   remain non-normative leads only.
5. Given a denominator with zero cells or duplicate `(operation_id, oracle_id)`
   cells, when `parse_denominator` runs, then `KELD-COMPAT-008`.
6. Given a committed N-cell denominator and only M&lt;N matching records, when
   `score` runs, then `unweighted_percent` is `None`, `complete` is false, and
   `claim` is `{passed}/{N} of …` with no "100% compatible" wording.
7. Given every denominator cell `pass` and no extras colliding, when `score` runs,
   then `complete` is true and `unweighted_percent` is `Some(100)` — the claim still
   names panel, corpus id, corpus digest, and kind.
8. Given an expired waiver relative to `as_of`, or two records for the same cell,
   when `score` runs, then a typed error (no silent last-write-wins).
9. Given Mach-O / PE / ELF / WASM magic prefixes (and empty/unknown bytes), when
   `classify_artifact` runs, then it returns the matching class. Import success is
   not a verdict this function can produce.

## 4. Design

- First-principles:
  - **Ownership:** the caller owns the JSON bytes and artifact prefix; the parser
    is pure and mints no principals, files, or percentages for unpublished corpora.
  - **Trust:** a record is untrusted input. Opaque model-session citations and
    sandbox paths are not evidence. Only `https://` URLs or `sha256:<64 hex>`
    content addresses qualify as an immutable location.
  - **Lifecycle:** schema version is a closed string. Unknown versions fail closed.
  - **I/O:** no filesystem reads in this slice. Magic classification is prefix-only.
  - **Failure:** every reject is a `KELD-COMPAT-*` code plus a fix sentence.
  - **Reuse:** workspace `serde_json` (already used by `keld-guard` / `keld-cli`).
    Existing `compat-scoreboard.md` and `keld-compat` stay the homes. Rejected:
    a new crate (YAGNI); putting this in `keld-guard` (permissions, not measurement);
    hand-rolled JSON (duplicate of the workspace parser); `sha2` (hash *format*
    is validated; hashing files is a later scanner).
  - Compatibility fallback: `not required` — no prior machine ledger exists.
  - Performance claim: none.
- New types (Rust): `EvidenceRecord`, `Denominator`, `Scoreboard`, `EvidenceError`,
  `classify_artifact`. No `.k.ts` contract in this slice.
- Capabilities / manifest (spec 03): none.
- Wire/protocol (spec 02): none (kipc unchanged). The JSON ledger is a new
  **versioned document format** (review: Public API + this format).
- Platform notes: magic classes are OS-agnostic; Windows/Linux/macOS parse identically.

### 4.1 Evidence record (`keld.compat.evidence/v1`)

Closed fields only (`deny_unknown_fields`):

| Field | Rule |
|---|---|
| `schema` | exactly `keld.compat.evidence/v1` |
| `artifact.sha256` | `sha256:` + 64 lowercase hex |
| `artifact.platform` | `macos` \| `windows` \| `linux` |
| `artifact.arch` | `aarch64` \| `x86_64` |
| `revisions.keld` / `bun` / `engine` | non-empty, not `latest` |
| `authority_profile` | `strict_bun` \| `sandboxed_addon_worker` \| `legacy_sandbox_off` \| `user_approved_tool_child` |
| `operation.id` | `[a-z0-9._-]+` |
| `operation.kind` | `install` \| `activation` \| `primary_workflow` \| `full_feature` |
| `operation.oracle.id` / `revision` | non-empty; revision ≠ `latest` |
| `result` | `pass` \| `fail` \| `unknown` \| `waived` |
| `waiver` | required iff `waived`; `{owner, reason, expires_on: YYYY-MM-DD}` |
| `evidence_uri` | `https://…` (not localhost) or `sha256:<64 hex>` |

### 4.2 Denominator (`keld.compat.denominator/v1`)

| Field | Rule |
|---|---|
| `schema` | exactly `keld.compat.denominator/v1` |
| `panel` | `product` \| `showcase` |
| `corpus_id` | `[a-z0-9._-]+` |
| `corpus_sha256` | `sha256:` + 64 lowercase hex |
| `kind` | same closed set as `operation.kind` |
| `cells` | ≥1 unique `{operation_id, oracle_id}` |

Product vs showcase: a showcase corpus (e.g. a future VS Code stress set) cannot
redefine product tiers. Scoring always echoes the denominator’s `panel`.

### 4.3 Scoreboard rules (the honesty gate)

1. Scoring requires a parsed denominator. There is no implicit “all records I have.”
2. Extra records whose cell is not in the denominator are ignored (they cannot
   shrink the denominator).
3. `missing` = denominator cells with no record. `unknown` counts recorded unknowns.
4. `unweighted_percent` is `None` when `missing > 0` or `unknown > 0` — an incomplete
   or unknown measurement MUST NOT become a percentage, including 100.
5. Otherwise `unweighted_percent = floor(100 * passed / N)`. Waived and failed
   cells stay in N and are not passes.
6. `complete` is true only when `passed == N`.
7. `claim` is always `{passed}/{N} of {panel} corpus {id}@{digest} ({kind})`.
   It MUST NOT contain the phrase `100% compatible` or `fully compatible`.

## 5. Boundaries

- Implement in: `crates/keld-compat/src/evidence.rs`, crate `AGENTS.md` one-liner,
  `docs/specs/kel74-compat-evidence-schema.md`, `docs/engineering/compat-scoreboard.md`,
  `docs/architecture/04-electron-compat.md` §4, `docs/engineering/keld-error-codes.md`,
  `crates/keld-cli/tests/error_registry.rs` `SCAN_REL`.
- Must not touch: `docs/research/`, `docs/architecture/02-*.md`, `03-*.md`,
  `packages/@keld/electron`, `agent/kel-72-*` / `kel-73-*` / `kel-78-*` trees,
  workspace crate-graph additions, kipc frames.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1 Spec + v1 parser/scorer + magic classifier + scoreboard honesty + errors.
- [ ] T2 (later issues) doctor consumption, committed product corpus, weighted board,
      import/signing scanner adapters.

## 7. Test plan

Colocated unit tests in `evidence.rs` map 1:1 to §3. Oracles are exact error codes,
exact `Scoreboard` fields, and exact magic classes — not a reimplementation of
scoring in the test besides the documented numbers. Anti-flake: no clock; tests
pass `as_of` explicitly. No ports, no temp files.

## 8. Review gates triggered

- unsafe: none
- public API: yes (`keld_compat::evidence`)
- permission model: none
- dependency: yes (`serde` / `serde_json` on `keld-compat`; already workspace-pinned)
- wire protocol: none for kipc; **versioned JSON ledger format** (call it as format review)

## 9. Perf impact

none — cold parse of ≤256 KiB JSON. Not on kipc / event-loop / guard hot paths.

## 10. Open questions

none
