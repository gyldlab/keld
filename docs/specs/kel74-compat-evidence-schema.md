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
4. Given an evidence URI that is a `file:`, an absolute temp path (`/tmp/…`),
   or an opaque turn citation (`turn0…`), when `parse_evidence` **or** `score`
   runs, then `KELD-COMPAT-007`. https URLs are not rejected by a `/tmp/`
   substring; they use host and content-address rules instead.
5. Given a denominator with zero cells or duplicate `(operation_id, oracle_id)`
   cells, when `parse_denominator` **or** `score` runs, then `KELD-COMPAT-008`.
   `complete` requires `N > 0`; empty cells MUST NOT yield `complete` or a `0/0`
   claim. Duplicate cells MUST NOT count one Pass twice.
6. Given a committed N-cell denominator and only `M < N` matching records, when
   `score` runs, then `unweighted_percent` is `None`, `complete` is false, and
   `claim` is `{passed}/{N} of …` with no "100% compatible" wording.
7. Given a **showcase** denominator where every cell `pass`es with matching
   artifact digest, authority profile, and engine, when `score` runs, then
   `complete` is true and `unweighted_percent` is `Some(100)` — the claim still
   names panel, corpus id, corpus digest, and kind. Product panel T1 never
   publishes `unweighted_percent` or `complete` (no committed product corpus id).
8. Given an expired waiver relative to `as_of`, two records for the same cell,
   **or** a `Pass` (or any non-`waived` verdict) paired with a waiver object,
   when `score` runs, then a typed error (no silent last-write-wins, no constructed
   Pass+waiver counted as pass).
9. Given Mach-O thin/fat (`FAT_MAGIC`, `FAT_CIGAM`, `FAT_MAGIC_64`, `FAT_CIGAM_64`)
   / PE / ELF / WASM magic prefixes (and empty/unknown bytes), when
   `classify_artifact` runs, then it returns the matching class. Import success is
   not a verdict this function can produce.
10. Given `https://example.com/foo` or a `/blob/main/`-style branch URL
    (including `/blob/main/<40-hex>`, `/tree/main/…`, case variants, and
    `raw.githubusercontent.com/{owner}/{repo}/main/…`), when `parse_evidence`
    **or** `score` runs, then `KELD-COMPAT-007`. A later hex path segment does
    not pin a live ref. An allowed URI is `sha256:<64 lowercase hex>` or an
    `https://` URL whose path contains a `/`-delimited git object id (40 or 64
    lowercase hex) and whose `blob`/`tree`/`raw` ref (or GitHub raw CDN ref
    segment) is itself that object id. Host checks parse authority (not
    `starts_with` after `https://`) and reject userinfo, loopback, IPv4-mapped
    loopback, unspecified addresses, RFC1918 private, IPv4 link-local, IPv6
    unique-local (`fc00::/7`), IPv6 link-local, and IPv4-mapped,
    IPv4-compatible (`::a.b.c.d`), or IPv4-translated (`::ffff:0:a.b.c.d`)
    copies of those. A trailing FQDN dot (`10.0.0.1.`, `localhost.`) is the
    same host.
11. Given two pass records that fill a 2-cell denom but disagree on artifact
    SHA-256, authority profile, or engine, when `score` runs, then `complete`
    is false and `unweighted_percent` is `None`.
12. Given `panel: product` and corpus id `toy-uncommitted` (or any id not on the
    committed-product list — T1: empty), when every cell passes, then
    `unweighted_percent` is `None`, `complete` is false, and `claim` does not
    contain `100%`.

## 4. Design

- First-principles:
  - **Ownership:** the caller owns the JSON bytes and artifact prefix; the parser
    is pure and mints no principals, files, or percentages for unpublished corpora.
  - **Trust:** a record is untrusted input. Opaque model-session citations and
    sandbox paths are not evidence. An immutable location is `sha256:<64 hex>`
    or an `https://` URL whose authority parses as a public host (no userinfo,
    loopback, IPv4-mapped / IPv4-compatible / IPv4-translated loopback,
    unspecified, RFC1918, link-local, unique-local, or the FQDN-dot form of
    those) and whose path contains a full git object id (40 or
    64 lowercase hex). A live branch/tag
    path such as `/blob/main/` is a lead, not a pin, even when another path
    segment is 40- or 64-hex; commit-pinned `/blob/<object-id>/` is allowed.
    `/tmp/` is a path-prefix check on non-https URIs, not a substring search
    on https URLs.
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
| `evidence_uri` | `sha256:<64 hex>`, or `https://` with a parsed public host (no userinfo / loopback / unspecified / RFC1918 / link-local / unique-local; trailing FQDN dots and IPv4-mapped/compatible/translated embeddings count as that address) and a 40- or 64-hex git object id path segment; `blob`/`tree`/`raw` (and GitHub raw CDN) refs MUST be that object id, not a branch name |

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

1. Scoring requires a parsed denominator with at least one cell. Empty `cells`
   is `KELD-COMPAT-008` (same as `parse_denominator`); there is no implicit
   “all records I have,” and `0/0` is not a complete measurement.
2. Extra records whose cell is not in the denominator are ignored (they cannot
   shrink the denominator).
3. `missing` = denominator cells with no record. `unknown` counts recorded unknowns.
4. `unweighted_percent` is `None` when `missing > 0`, `unknown > 0`, contributing
   records disagree on artifact digest / authority profile / engine, or the
   panel is `product` and `corpus_id` is not a documented committed product
   corpus (T1: none). An incomplete or uncommitted measurement MUST NOT become
   a percentage, including 100.
5. Otherwise `unweighted_percent = floor(100 * passed / N)`. Waived and failed
   cells stay in N and are not passes.
6. `complete` is true only when `N > 0`, `passed == N`, contributing
   records share artifact digest, authority profile, and engine, and the
   panel is not `product` unless `corpus_id` is a documented committed
   product corpus (T1: none, so product `complete` is always false).
   `Scoreboard` fields are private; only `score` constructs the type.
7. `claim` is always `{passed}/{N} of {panel} corpus {id}@{digest} ({kind})`.
   It MUST NOT contain the phrase `100% compatible` or `fully compatible`.
8. A waiver object is valid only with `result: waived`. `score` rejects
   constructed `Pass`+waiver (and any other non-waived pairing) as
   `KELD-COMPAT-006` even when the waiver has not expired. `score` also
   re-validates `evidence_uri`.
9. Duplicate `cells` in a hand-built denominator are `KELD-COMPAT-008`.

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
