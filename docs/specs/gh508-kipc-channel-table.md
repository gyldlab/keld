# Spec: one keld-ipc-owned kipc channel table (minimum slice of architecture 02 §4)
Status: approved
Linear: GH-508 (#517) · Owner: @0monish · Updated: 2026-10-07

## 1. Goal & non-goals

Keld allocates kipc channel ids today by hand, in five places, with no list of what is
allocated. FACT (this branch, based on `9d26d488`):

| Id | Production definition | Hand copies |
|---:|---|---|
| 1 | `crates/keld-ipc/src/echo.rs:10` `ECHO_CHANNEL` | `packages/@keld/kipc/src/transport.ts:19`; `crates/keld-wv/src/wkwebview/macos_bridge.rs:28` (Rust) plus two `channel !== 1` checks in its injected page and isolated-world scripts |
| 2 | `crates/keld-native/src/fs.rs:37` `FS_CHANNEL` | none |
| 3 | `crates/keld-ipc/src/lifecycle.rs:17` `LIFECYCLE_CHANNEL` | `packages/@keld/kipc/src/transport.ts:21` |

The only cross-language guard is a source-text parity test
(`packages/@keld/kipc/src/transport.test.ts:88-110`) that checks `echo.rs` contains
`ChannelId(1)` and `lifecycle.rs` contains `ChannelId(3)`. `keld-wv` has no `keld-ipc`
dependency and no parity test. Root `AGENTS.md` forbids mirrored constants, and four
first-proof consumers are about to add channels.

This spec defines one channel table owned by `keld-ipc`. Every production use of a
channel id derives from it, the TypeScript constants are generated from it by the
existing KEL-98 cold tooling with a drift check, and new channels obtain ids by one
append-only rule. The observable outcome after X05-T4 (#597): one Rust file lists every
allocated id; deleting or changing any copy anywhere else fails a test; wire bytes and
protocol version 2 are unchanged.

Non-goals:

- no implementation in this PR. X05-T4 (#597) implements the table, the generated
  constants and the retirement of the hand copies;
- no handshake-time channel resolution, no `HELLO` change and no protocol bump. These
  stay destination (architecture 02 §2 and §4);
- no general schema codegen, no `@keld/schema`, no live `keld gen` (stays `KELD-CLI-045`);
- no new IDL. The table is a Rust source file and the generator is the existing KEL-98
  owner;
- no per-Electron-family id ranges, reserved blocks or id spaces. Electron channel
  strings travel as payload data under `el:` window grants (architecture 03 §2);
- no capability-name registry. Capability names stay owned by `keld-guard` and the
  architecture 03 §2 manifest vocabulary; the table only references them;
- no new channel id in this slice. Each first-proof consumer adds its own entry under §4.4
  when it lands (§4.5 lists them);
- no `el:` grant shape (F04-T3, X04-T1) and no change to guard evaluation.

## 2. Spec refs

- `docs/architecture/02-ipc.md` §2: header `channel:u16`; HELLO uses channel `0`.
  **Amended in this PR** (a code/spec mismatch is fixed in the same PR). The
  "Correlation ids" bullet said channels "are u16 handles resolved at handshake from
  schema names (string names never travel per-call)", but no handshake resolution is
  live. It now says that ids come from the static `keld-ipc` channel table (§4; live with
  X05-T4, #597), and that resolving ids at handshake is destination work.
- `docs/architecture/02-ipc.md` §4: the destination `keld gen` channel table and
  per-channel capability declaration. **Deviation, amended in this PR:** §4 now states the
  split as a short pointer to this spec. A static, append-only `keld-ipc` table is the
  live slice (it goes live with X05-T4); handshake resolution and full `keld gen` stay
  destination. The details (the allocation baseline, §4.4 and the subscription rule for
  unprompted `EVENT`s, §4.4 step 6) are owned here, not repeated in architecture 02.
- `docs/architecture/01-overview.md` §3: `keld-ipc` owns the "channel registry";
  `keld-wv` depends on `keld-guard` only; crates never depend upward.
- `docs/architecture/03-security-model.md` §2: manifest vocabulary (`app.fs.read`,
  `app.system` literals, `windows.<w>.channels`).
- `docs/specs/kel133-kipc-receiver-semantics.md` §4 "v0 semantic table": the receive
  policy families the receive-policy class refers to.
- `docs/specs/kel98-echo-codegen.md`: the cold generator, generate/check, drift and
  fail-closed parser pattern this spec reuses.
- `docs/specs/kel136-generated-ts-app-link-transport.md`: one TypeScript transport owner,
  embedded byte-for-byte by `keld create` and staged as the only transport file.
- Draft `docs/specs/gh527-worker-owned-blocking-call-transport.md` (branch
  `agent/gh-527-worker-link-spec`): its T4 credit lane fixes "credited channels ... by
  the host-declared channel table at open". That spec consumes this table; it does not
  define a second one.
- `crates/keld-ipc/AGENTS.md`: frame and HELLO changes bump `PROTOCOL_VERSION`.

## 3. Acceptance criteria (binary, each becomes a test)

These are the spec's contract for X05-T4. Each names its negative control.

1. **One owner.** Given the repository, when the table test enumerates
   `keld_ipc::channel_table::CHANNEL_TABLE`, then it is the only definition of a
   `ChannelEntry`. The test lists ids `1 = echo`, `2 = fs`, `3 = lifecycle` and nothing
   else. `ChannelEntry` has no public constructor outside `channel_table.rs`.
   *Negative control:* a second `ChannelEntry` constant defined in `keld-native` fails to
   compile, because the constructor is private.
2. **Duplicate id.** Given a table fixture with two entries at id `2`, when
   `validate_table` runs, then it returns `TableDefect::DuplicateId(2)`. The real table is
   checked by the same function in a `const` assertion, so the duplicate also fails
   `cargo build`. *Negative control:* swapping `validate_table` for a function that always
   returns `Ok` makes the fixture test fail.
3. **Reserved and ordered ids.** Given a fixture entry at id `0`, or ids that do not rise
   strictly in declaration order, when `validate_table` runs, then it returns
   `ReservedId` or `NotIncreasing`. *Negative control:* inserting `fs` before `echo` fails.
   Ordering alone does not prove append-only, because a renumbered entry can still rise
   (criterion 3a).

   **3a. Append-only against the committed baseline.** Two independent checks, each
   with its own negative control.
   - *Table matches baseline (in-crate unit test).* The baseline is the committed file
     `crates/keld-ipc/channel_allocations.txt`: one `name id` line per allocation ever
     made, in allocation order, decimal ids, no other content. It is covered by the
     existing `/crates/keld-ipc/` CODEOWNERS line. A `#[cfg(test)]` module inside
     `channel_table.rs` reads it with `include_str!` and runs the crate-private
     `check_allocations(table, baseline) -> Result<(), AllocationDefect>`. Being in the
     crate, its fixtures can use the private `ChannelEntry::new`. Every baseline pair
     must be in the table with the same id at the same position, and the table has no
     entry the baseline does not list. It returns `AllocationDefect::Removed { name,
     id }` for a baseline name absent from the table, `Changed { name, baseline, table }`
     for a baseline name whose id or position differs, and `Unrecorded { name, id }` for
     a table entry beyond the baseline (so an appending PR also appends its line).
     *Negative controls:* with a fixture table `echo 1, fs 2, lifecycle 3, probe 4` and
     the matching baseline, renumbering `probe` to `5` passes `validate_table` (ids still
     rise) but fails with `Changed { name: "probe", baseline: 4, table: 5 }`; deleting
     `fs` from the table fails with `Removed`.
   - *Baseline is append-only (CI hygiene rule).* `tools/ci_hygiene.rs` gains
     `check_channel_allocations_append_only`. It resolves the merge base with
     `origin/main` the same way `tools/ci_changes.sh` does, reads the baseline at that
     base with `git show`, and passes only if the base file's lines are an exact prefix
     of the current file's lines. A base without the file (the PR that introduces it)
     passes; an unresolvable base fails closed with a named error, never passes.
     *Negative controls:* renumbering `fs` from 2 to 4 in both the table and the
     baseline passes the unit test but fails this rule; deleting the `fs` line from the
     baseline fails it too.
4. **Authority field.** Given a fixture entry with `Authority::Guarded(&[])` (an empty
   list), when `validate_table` runs, then it returns `EmptyCapabilityList`. Every
   `Authority` value is either `HostInternal` or a non-empty list of `keld-guard`-owned
   capability constants. *Negative control:* removing the `authority` field from
   `ChannelEntry` fails to compile, because both the table and the tests name it.
5. **Name rule.** Given a fixture entry named `el-ipc`, `electron`, `ipcMain` or `a:b`,
   when `validate_table` runs, then it returns `InvalidName`. Names match
   `[a-z][a-z0-9-]{0,31}`. No hyphen segment equals `el`, and no name contains
   `electron`. The type has no range field. *Negative control:* adding an entry named
   `el-reserved` fails.
6. **Hand-written constant (Rust).** The scan input is every `.rs` file under
   `crates/*/src` and `crates/keld-ipc/fuzz/fuzz_targets`, except
   `crates/keld-ipc/src/channel_table.rs`. `tests/` directories are never scanned.
   Inside a file, only an item that a test-only attribute directly gates is excluded.
   A test-only attribute is `#[cfg(test)]` or `#[cfg(all(test, <predicates>))]`, where
   `test` is a top-level conjunct. For an inline `mod <name> {`, the exclusion is the
   module body up to its brace-matched close. For a single-line item, such as a `use`
   line, it is that one line. For `mod <name>;`, it is the file that the declaration
   resolves to under Rust's module rules. From `lib.rs`, `main.rs` or `mod.rs` that
   is `<name>.rs` or `<name>/mod.rs` beside it. From any other `<parent>.rs` it is
   `<parent>/<name>.rs` or `<parent>/<name>/mod.rs`. A `#[path]` attribute overrides
   both, and its target is used. For example, `bootstrap.rs:203` resolves to
   `src/bootstrap/admission_deadline_tests.rs`. Any other
   form is scanned, not skipped, including `cfg(any(test, …))` and a non-test `cfg`, so
   the scan fails closed. The test passes if and only if the remaining text has zero matches of either
   regex: `ChannelId\(\s*[0-9]` and
   `const\s+[A-Z0-9_]*CHANNEL[A-Z0-9_]*\s*:\s*u16\s*=\s*[0-9]`.
   *Negative controls:* putting back `const ECHO_CHANNEL: u16 = 1;` in
   `macos_bridge.rs` fails the scan. Putting back `ChannelId(0)` in `link.rs`'s
   `write_hello` also fails, although `link.rs` gates a `use` with `#[cfg(test)]` at
   line 5. So does a `ChannelId(2)` placed after the `#[cfg(test)]` item at `fs.rs:591`.
   A test-gated module that contains `ChannelId(9)` passes. So do the live test-only
   literals in `bootstrap.rs`'s `#[cfg(all(test, windows))] mod named_pipe_tests` and in
   `bootstrap/admission_deadline_tests.rs`, which `bootstrap.rs:203` declares behind a test gate.
   On origin/main the rule leaves exactly seven hits, all of them production literals that X05-T4
   replaces: `receive.rs:169`, `link.rs:633`, `echo.rs:10`, `lifecycle.rs:17`, `fs.rs:37`,
   the fuzz target `raw_receive.rs:27` and `macos_bridge.rs:28`.
   The same literal under `#[cfg(any(test, windows))]` fails.
7. **Hand-written constant (TypeScript and injected script).** Given production
   TypeScript under `packages/*/src` and `crates/keld-cli/templates/*/src`, and the macOS
   bridge's injected scripts, when the scan runs, then no numeric channel-id literal
   exists outside the generated region (§4.3). *Negative control:* changing the generated
   `ECHO_CHANNEL` line into hand-written code outside the region fails the scan, and so
   does putting back `channel !== 1` in the page script.
8. **Generated drift.** Given the committed `transport.ts`, when `bun test` runs in
   `packages/@keld/kipc`, then the generated region is byte-identical to the generator's
   render of `channel_table.rs`. *Negative control:* hand-editing `LIFECYCLE_CHANNEL = 3`
   to `4` inside the region fails with "channel table region is stale; run bun run
   echo:generate". Changing a Rust id without regenerating fails the same way.
9. **Generator fails closed.** Given a `channel_table.rs` entry outside the admitted
   rustfmt-normalized form, for example an id written as an expression, when the
   generator runs, then it exits non-zero and names the source line. It never guesses.
10. **Unregistered channel is not admitted.** Given an authenticated host session, when a
    peer sends a structured `CALL` on id `4` (unallocated), then every live receive
    policy returns `KELD-IPC-005` before any payload is allocated and before any handler,
    guard or broker runs. This is the existing KEL-133 wrong-channel behaviour (corpus
    rows `call-wrong-channel` and `primary-undeclared-channel`), restated against the
    table. A new `keld-ipc` unit test applies id `4` to each live constructor; the corpus
    itself is not edited. Separately, a `ReceivePolicy` cannot be built for an id that is
    not in the table: `privileged_call_receiver` takes `&'static ChannelEntry`, not
    `ChannelId`, and returns `Result<ReceivePolicy, IpcError>`. *Negative controls:*
    calling it with a raw `ChannelId(4)` fails to compile. Passing an entry whose class is
    not `GuardedCall` returns `KELD-IPC-005` with detail "channel class does not admit
    this policy". Deleting that class check makes the unit test fail.
11. **Consumers derive their ids.** Given the built workspace, the following are
    expressions over table entries, not literals: `keld_ipc::ECHO_CHANNEL`,
    `keld_ipc::LIFECYCLE_CHANNEL`, `keld_native::fs::FS_CHANNEL`, the macOS bridge's
    admitted id, and the generated TypeScript constants. Proof is automated, with no
    hand-run mutation: (i) criteria 6 and 7 prove that no literal exists outside the
    table; (ii) a `keld-ipc` unit test asserts `ECHO_CHANNEL == channel_table::ECHO.id()`
    and `LIFECYCLE_CHANNEL == channel_table::LIFECYCLE.id()`, and a `keld-native` unit
    test asserts the same for `FS_CHANNEL`; (iii) the existing golden vectors
    (`receiver-semantics-v0.tsv` `echo-call-valid` and the echo link tests) pin the ids
    on the wire. *Negative controls:* the generator test renders a fixture table with
    `echo` at id `4` and asserts that the region reads `ECHO_CHANNEL = 4`; criterion 12
    builds the bridge with id `7`; replacing any Rust consumer with a literal fails (i).
12. **macOS bridge id from the host.** Given `keld-core` building the renderer bridge,
    when it calls `RendererBridgeEndpoint::new`, then it passes the admitted channel id
    from `channel_table::ECHO`. The bridge renders that id into its injected scripts at
    construction. `crates/keld-wv/Cargo.toml` gains no `keld-ipc` dependency. *Negative
    control:* a bridge built with admitted id `7` rejects a page `invoke(1, ...)` with
    `KELD-WV-011` "renderer channel is not declared by this build". So the id is not
    hard-coded.
13. **Wire unchanged.** Given the change, `PROTOCOL_VERSION` stays `2`. Ids 1, 2 and 3
    keep their values. The canonical corpus digest and every golden vector pass
    unmodified. *Negative control:* criterion 11's automated controls; any renumbering
    also fails criterion 3a.
14. **Parity test retired.** Given X05-T4, `transport.test.ts` no longer contains the
    `ChannelId(1)` / `ChannelId(3)` source-text expectations. Criterion 8 replaces them.
15. **Permission literals have one owner.** Given X05-T4, `keld-guard` exports
    `keld_guard::capability::FS_READ` and `FS_WRITE`. Its own filesystem predicate
    (`crates/keld-guard/src/lib.rs:1029`), the `fs` table entry and
    `keld_native::fs::FS_READ_CAPABILITY` / `FS_WRITE_CAPABILITY` all reference those
    constants. A scan of production Rust outside `crates/keld-guard/src` finds no string
    literal equal to an exported `keld-guard` capability name. *Negative control:* writing
    `Authority::Guarded(&["fs.read"])` in `channel_table.rs`, or any `"fs.read"` literal
    in `keld-ipc` production source, fails the scan.
16. **Unprompted EVENTs only reach subscribed roles.** This is a rule for consumers;
    X05-T4 adds no event channel. The host writes an unprompted `EVENT` on a table entry
    only to a role link that has subscribed to that entry. A role subscribes with one
    `CALL` on the `lifecycle` channel, `LifecycleRequest::Subscribe { channel: u16 }`,
    naming the target entry's id (§4.4 step 6). Registration therefore never needs a
    `CALL` on the target channel, so an `EVENT`-only channel such as `window-state` can
    be subscribed to. Calling on a channel does not subscribe. The one baseline
    exception is `lifecycle` itself, whose `Ready` / `LastWindowClosed` events every
    version-2 peer admits (`lifecycle_event_receiver`). **Decided for this slice:**
    `Subscribe` is admitted only for an entry with `Authority::HostInternal` and an
    event-bearing class. A `Guarded` entry is refused with `GuardedNotSupported`;
    guarded event subscription is future work that needs its own spec and a
    `keld-guard` API. A host-internal subscription binds to the authenticated principal
    of the link it arrived on (the payload names no principal), and is admitted only if
    that role's KEL-75 role declaration (host configuration, never child-supplied)
    admits the channel. `window-state` is host-internal and admitted for the `primary`
    lifecycle owner, so the first proof is unaffected. The first consumer that adds a
    host-initiated channel (F02-T2 `window-state`) lands the `Subscribe` variant, its
    admission and the per-link subscription state in `crates/keld-core/src/app_session.rs`,
    together with these tests: (i) given a role that never subscribed to `window-state`,
    when a window event occurs, then zero frames are written to that link; (ii) given a
    role that subscribed, then exactly one `EVENT` frame per window event is written;
    (iii) `Subscribe` naming an unallocated id (`4` before F02-T2's id) is refused
    `UnknownChannel`, and naming `echo` or `lifecycle` is refused `NotSubscribable`;
    (iv) `Subscribe` naming a `Guarded` fixture entry is refused `GuardedNotSupported`;
    (v) an `app-bound` role link naming `window-state` (declared for `primary` only) is
    refused `RoleNotAdmitted`; (vi) after role A subscribes, role B's link still
    receives zero frames. Every refusal changes no subscription state. *Negative
    controls:* removing the subscription check makes (i) and (vi) observe a frame;
    removing the class check makes (iii) admit `echo`; removing the authority check
    makes (iv) admit the `Guarded` entry; removing the declaration check makes (v)
    admit. Subscription is per link, so a supervised restart starts unsubscribed.

## 4. Design

### 4.1 First-principles and reuse decision

Atomic decomposition (each atom has its own observable; none relies on the synthesis):

| Atom | Owner and boundary | Input → output | Failure mode | Observable |
|---|---|---|---|---|
| A1 Allocation owner | `keld-ipc` `channel_table.rs` | the reviewed entry list → `CHANNEL_TABLE` | a second owner (copy) drifts | criterion 1, 6, 7 |
| A2 Id uniqueness and order | `validate_table` (const fn) | entries → `Ok` or `TableDefect` | two meanings for one wire id | criteria 2, 3 |
| A2b Allocation stability | `check_allocations` (test) against the committed `(name, id)` baseline | table + baseline → `Ok` or `AllocationDefect` | a renumbered, renamed or removed id that still rises | criterion 3a |
| A3 Authority reference | `keld-guard` names; table holds references | entry → `HostInternal` or guard capability list | a second capability-name owner, or an undeclared guarded channel | criterion 4 |
| A4 Receive-class binding | KEL-133 `ReceivePolicy` constructors | entry class → which policy constructors may name it | a policy built for the wrong class or an unknown id | criterion 10 |
| A5 TypeScript generation | KEL-98 cold generator | `channel_table.rs` text → generated region in `transport.ts` | stale or hand-edited constant | criteria 8, 9 |
| A6 Rust consumer derivation | each consumer crate | table entry → existing public constant | literal reintroduced | criteria 6, 11 |
| A7 macOS bridge id source | `keld-core` (constructs) → `keld-wv` (enforces) | admitted `u16` at construction → bridge admission and scripts | an upward or sideways crate edge, or a hard-coded id | criterion 12 |
| A8 Wire invariance | `keld-ipc` frame/HELLO | ids and bytes before → the same after | silent renumbering breaks scaffolds already created | criteria 11, 13 |
| A9 Admission of unknown ids | KEL-133 validator | frame on unallocated id → `KELD-IPC-005` | handler effect on an unregistered channel | criterion 10 |
| A10 EVENT subscription | `keld-core` `app_session.rs` link owner (state and admission); `lifecycle` codec in `keld-ipc` (request); KEL-75 role declaration (which roles may subscribe) | `Subscribe { channel }` on a link → per-link subscription bit, or refusal | an unprompted `EVENT` reaches a peer that cannot decode it, or an `EVENT`-only channel cannot be subscribed | criterion 16 (lands with F02-T2) |

Edges between atoms are explicit. A5 reads only A1's source text. A6 and A7 read only
A1's constants. A3 validation is local to the table, so authorization (guard
evaluation per request) is not changed by any atom. A9 already exists and is only
re-stated. A2 is enforced at compile time and by a unit fixture, independently. A2b
reads A1's table and a separately committed baseline; A2 passing says nothing about
A2b, because rising ids can still be renumbered. A10 reads A1 (the target entry's class
and authority) and the link's KEL-75 declaration; it refuses every `Guarded` entry, so
it never asks `keld-guard`, and it adds no X05-T4 code.

Ownership, trust and lifecycle facts:

- **Handle and crash ownership:** none change. The table is `const` data in `keld-ipc`.
  No socket, process or window lifetime moves. **No boundary change** in the architecture
  sense (no handle, crash or principal-minting change).
- **Trust:** a channel id is a routing selector, not authority. The table's `Authority`
  field is a declaration. Authorization stays per request in `guard_dispatch` /
  `keld-guard`, and a peer can never choose its own policy (KEL-133). The security atoms
  are separate: identity (unchanged, host-minted principals); authentication (unchanged,
  HELLO token); authorization (unchanged, guard evaluation; the table only declares which
  capability a guarded channel's broker evaluates); containment (unchanged); lifecycle
  and revocation (unchanged); evidence (the table test plus corpus replay).
- **I/O and memory:** none. The entries are `const`; the hot path compares the same
  `u16` it compares today.
- **Failure:** table defects fail at compile time (const assertion) and in unit tests.
  Frames on unallocated ids fail at runtime with the existing `KELD-IPC-005`.

Reuse evaluated:

- `ChannelId(pub u16)` (`frame.rs:18`) stays the wire type. The table wraps it and does
  not replace it.
- The KEL-133 `ReceivePolicy` named constructors stay the only way to build a policy.
  The class field selects among them; it adds no policy.
- The KEL-98 generator (`packages/@keld/kipc/scripts/echo-codegen.ts`) is extended with a
  second render target, using the same strict line grammar, fail-closed errors,
  `generate`/`check` commands and Bun freshness test. No new script package, IDL,
  `syn`, `ts-rs` or proc macro. Renaming the script to a non-echo name is parked cleanup.
- The KEL-136 rule that `transport.ts` is the single staged transport file (and is
  embedded byte-for-byte by `keld create`, `crates/keld-cli/src/template.rs:28`) is kept.
  The constants are therefore generated *into* that file (§4.3), not into a new runtime
  module.
- `keld-guard` already uses `"fs.read"` / `"fs.write"` as literals
  (`crates/keld-guard/src/lib.rs:1029`), and `keld-native` repeats them as
  `FS_READ_CAPABILITY` / `FS_WRITE_CAPABILITY`. X05-T4 adds one pair of exported
  constants in `keld-guard` (the existing vocabulary owner). The guard predicate, the
  `keld-native` constants and the table's `fs` entry all reference that pair. This
  removes a duplication instead of adding a third copy. It is not a registry: it exports
  names that `keld-guard` already owns, and adds only those the table references.
  **Decided** (AGENTS.md engineering rule 3: one owner); enforced by criterion 15.
  *Falsifier:* `keld-guard` being unable to export the names without a dependency it
  does not already have. It has none to add, since `keld-ipc` and `keld-native` already
  depend on it.

Rewrite justification: none needed. Nothing is rewritten. The hand constants become
derived expressions with the same public names and values.

Compatibility fallback: not required. Public names (`ECHO_CHANNEL`, `LIFECYCLE_CHANNEL`,
`FS_CHANNEL`, TypeScript `ECHO_CHANNEL` / `LIFECYCLE_CHANNEL`) keep their names, types
and values.

Performance baseline: not applicable. No performance claim is made.

### 4.2 The table (Rust sketch for X05-T4)

```rust
// crates/keld-ipc/src/channel_table.rs
//! The one kipc channel table (GH-508). Append-only; see docs/specs/gh508-kipc-channel-table.md §4.4.

use crate::frame::ChannelId;

/// Channel id reserved for `HELLO` (architecture 02 §2). Never allocatable.
pub const HANDSHAKE_CHANNEL: ChannelId = ChannelId(0);

/// Which KEL-133 receive-policy family may name a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReceiveClass {
    /// App-to-host `CALL`, answered by `REPLY` or a declared `ERR`; not guard-routed
    /// (KEL-133 rows: host echo receiver, echo caller waiter, primary app receiver).
    HostCall,
    /// `HostCall` plus host-to-app `EVENT` on the same id (rows: host lifecycle
    /// receiver, app lifecycle event receiver, app lifecycle reply waiter).
    HostCallWithEvents,
    /// App-to-host `CALL` routed through `guard_dispatch`; `ERR` is `CallError`
    /// (row: privileged receiver).
    GuardedCall,
}

/// What authorizes a call on the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Session control on the host-minted link; never evaluated by `keld-guard`.
    HostInternal,
    /// Every call is evaluated by `keld-guard` against one of these manifest
    /// capability names. The names are `keld-guard` constants, never literals here.
    Guarded(&'static [&'static str]),
}

/// One allocated kipc channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelEntry {
    name: &'static str,
    id: ChannelId,
    class: ReceiveClass,
    authority: Authority,
}

impl ChannelEntry {
    const fn new(name: &'static str, id: u16, class: ReceiveClass, authority: Authority) -> Self {
        Self { name, id: ChannelId(id), class, authority }
    }
    /// Wire id.
    #[must_use]
    pub const fn id(&self) -> ChannelId { self.id }
    // name(), class(), authority(): same shape.
}

/// Generic session echo (KEL-30).
pub const ECHO: ChannelEntry =
    ChannelEntry::new("echo", 1, ReceiveClass::HostCall, Authority::HostInternal);
/// Retained filesystem broker (KEL-130).
pub const FS: ChannelEntry = ChannelEntry::new(
    "fs", 2, ReceiveClass::GuardedCall,
    Authority::Guarded(&[keld_guard::capability::FS_READ, keld_guard::capability::FS_WRITE]),
);
/// Host session lifecycle (KEL-72).
pub const LIFECYCLE: ChannelEntry =
    ChannelEntry::new("lifecycle", 3, ReceiveClass::HostCallWithEvents, Authority::HostInternal);

/// Every allocated channel, in strictly increasing id order.
pub const CHANNEL_TABLE: &[ChannelEntry] = &[ECHO, FS, LIFECYCLE];

const _: () = assert!(validate_table(CHANNEL_TABLE).is_ok());
```

`validate_table` is a `const fn` returning `Result<(), TableDefect>`. Its variants are
`ReservedId`, `NotIncreasing`, `DuplicateId(u16)`, `InvalidName`, `DuplicateName` and
`EmptyCapabilityList`. Strictly increasing ids imply uniqueness; `DuplicateId` is still
reported separately so that the error names the cause. `validate_table` sees only the
current table, so it cannot detect renumbering. Append-only is checked separately
(criterion 3a) against the committed baseline:

```text
# crates/keld-ipc/channel_allocations.txt (the whole file)
echo 1
fs 2
lifecycle 3
```

`check_allocations` and `AllocationDefect` are `#[cfg(test)]` items inside
`channel_table.rs`, so they add no public API. The baseline and the hygiene rule are
the only places, outside the table, where an allocated id is written down. The exact line formatting is
whatever `cargo fmt` produces. The generator admits that form only (§4.3).

Initial entries:

| Name | Id | Receive class | Authority | Owner of handler |
|---|---:|---|---|---|
| `echo` | 1 | `HostCall` | `HostInternal` | `keld-ipc` echo; `keld-core` primary link; macOS renderer bridge |
| `fs` | 2 | `GuardedCall` | `Guarded(fs.read, fs.write)` | `keld-native` retained filesystem broker |
| `lifecycle` | 3 | `HostCallWithEvents` | `HostInternal` | `keld-ipc` lifecycle; `keld-core` session |

Reserved: id `0` (`HANDSHAKE_CHANNEL`). No range is reserved for anything else.

Derived consumers (X05-T4 rewrites these, keeping their names and values):

- `crates/keld-ipc/src/echo.rs`: `pub const ECHO_CHANNEL: ChannelId = channel_table::ECHO.id();`
- `crates/keld-ipc/src/lifecycle.rs`: the same for `LIFECYCLE`.
- `crates/keld-ipc/src/receive.rs`: `hello()` uses `HANDSHAKE_CHANNEL`. The echo and
  lifecycle constructors use the entries. `privileged_call_receiver(entry: &'static
  ChannelEntry)` checks `entry.class() == ReceiveClass::GuardedCall`. It returns
  `Result<ReceivePolicy, IpcError>` and fails `KELD-IPC-005` for any other class. The
  check runs at cold construction, never per frame, and it does not panic. This
  signature is the public-API delta. The crate-private
  `validate_primary_app_header_with_privileged_call` (`pub(crate)`) changes its
  parameter the same way; that is not public API.
- `crates/keld-ipc/src/link.rs`: `write_hello` (`link.rs:633`) passes
  `HANDSHAKE_CHANNEL` in place of the literal `ChannelId(0)`. This is its only change,
  with no behaviour change; the corpus `hello-*` rows and the HELLO golden bytes prove
  it.
- `crates/keld-ipc/fuzz/fuzz_targets/raw_receive.rs`: builds its policy from
  `channel_table::FS` in place of `ChannelId(2)`.
- Id lookup: `pub const fn channel_table::entry(id: ChannelId) -> Option<&'static
  ChannelEntry>`, a linear scan of `CHANNEL_TABLE`. The KEL-133 corpus harness
  (`crates/keld-ipc/tests/receiver_corpus.rs`) maps `privileged-fs-receiver:<id>` through
  it and then calls `privileged_call_receiver`; an id with no entry fails the row's
  policy construction, so the TSV stays unedited. `keld-core`'s `Subscribe` admission
  uses the same lookup.
- `crates/keld-native/src/fs.rs`: `pub const FS_CHANNEL: ChannelId = keld_ipc::channel_table::FS.id();`
  `FS_READ_CAPABILITY` and `FS_WRITE_CAPABILITY` re-export the `keld-guard` constants.
- `crates/keld-core/src/app_session.rs`: unchanged call sites (it already imports
  `ECHO_CHANNEL`, `FS_CHANNEL` and `LIFECYCLE_CHANNEL`). It passes
  `channel_table::ECHO.id().0` to the renderer bridge (§4.6).
- TypeScript: the generated region (§4.3). `@keld/api`, `@keld/electron` and the hello
  scaffold keep importing `ECHO_CHANNEL` / `LIFECYCLE_CHANNEL` from `transport.ts`.

### 4.3 Generated TypeScript constants (existing KEL-98 owner)

`packages/@keld/kipc/scripts/echo-codegen.ts` gains a second render target. It reads
`crates/keld-ipc/src/channel_table.rs` and rewrites one delimited region of
`packages/@keld/kipc/src/transport.ts`:

```ts
// @generated-begin channel-table: packages/@keld/kipc/scripts/echo-codegen.ts from
// crates/keld-ipc/src/channel_table.rs. Do not edit by hand; run bun run echo:generate.
/** Channel `echo` (`keld_ipc::channel_table::ECHO`). */
export const ECHO_CHANNEL = 1;
/** Channel `fs` (`keld_ipc::channel_table::FS`). */
export const FS_CHANNEL = 2;
/** Channel `lifecycle` (`keld_ipc::channel_table::LIFECYCLE`). */
export const LIFECYCLE_CHANNEL = 3;
// @generated-end channel-table
```

- **Names:** the TypeScript name is the entry name uppercased, with `-` mapped to `_`,
  plus `_CHANNEL`. **Decided:** every entry is emitted, with no per-entry opt-in. There
  is one source, drift is checked against the full table, and a constant with no
  TypeScript consumer yet (`FS_CHANNEL`) is harmless. The new export is the public-API
  delta of this target. *Falsifier:* a table entry whose id must not be visible to Bun
  roles. No such entry exists, because ids are routing selectors, not authority.
- **Also generated:** `HANDSHAKE_CHANNEL = 0`. `RECEIVE_POLICIES.serverPreAuthHello` and
  `clientAwaitHello` use it instead of the literal `0`.
- **Parser:** the same rules as KEL-98. It accepts only the rustfmt-normalized
  `pub const NAME: ChannelEntry = ChannelEntry::new("name", N, ReceiveClass::X,
  Authority::Y)` forms committed by X05-T4 (single-line or rustfmt-wrapped). It rejects
  expressions, duplicate names and ids that are not decimal `u16` literals. It reads
  `CHANNEL_TABLE` to check that every constant is listed exactly once. It does not
  re-implement `validate_table`: Rust compilation is the validator of record. The
  generator fails if it cannot read the shape.
- **Freshness:** `check` re-renders and byte-compares the region. The Bun test in
  `scripts/echo-codegen.test.ts` calls the same function, so `just ci`'s
  `cd packages/@keld/kipc && bun test` is the drift check. A missing or duplicated
  `@generated-begin` / `@generated-end` marker fails.
- **Why a region, not a new file:** `transport.ts` is the only transport file that
  `keld create` embeds, the boot compiler stages and the Linux strict profile binds
  (KEL-136 criterion 13). A new runtime-value file would reopen staging and strict-profile
  review (KEL-98 §4 "Source-time versus runtime"). The region keeps that file surface
  unchanged, and the `template.rs` byte-identity test still holds.
- **No hand-edited generated output:** criterion 7 rejects numeric channel literals
  outside the region, and criterion 8 rejects edits inside it.

### 4.4 Admission and registration rule

A new kipc channel gets an id and an entry only like this:

1. **Spec first.** The consumer's approved spec names the entry: a name that follows the
   §3 criterion 5 grammar and does not name an Electron API, a receive class, and an
   authority. The authority is `HostInternal`, or `Guarded` with capability names that
   `keld-guard` / architecture 03 §2 already own. A capability name that does not exist
   yet is added through the `keld-guard` owner and the permission-model gate, never in
   this table. The consumer spec MAY state the id it expects. That id is advisory.
2. **Append at merge.** The consumer's implementation PR appends one entry with id equal
   to the current maximum plus one, and appends the same `(name, id)` pair to the
   committed baseline (criterion 3a). Ids are append-only: never filled into gaps, never
   reused, never renumbered, never reserved ahead of time, and never grouped by family.
   An existing baseline line is never edited or deleted. If two PRs race, the second to
   merge rebases and takes the next id. The uniqueness assertion, the baseline check
   and the drift check turn a missed rebase into a deterministic failure.
3. **Same PR regenerates** the TypeScript region and adds the consumer's KEL-133 policy
   rows. If no existing `ReceiveClass` fits (for example a host-to-app `EVENT`-only
   channel), the same PR adds the variant and its `ReceivePolicy` constructor in
   `receive.rs`, plus corpus rows under the KEL-133 owner.
4. **Retirement.** A retired channel keeps its entry and its baseline line, with
   `Authority::HostInternal` and a doc comment naming the retiring PR. No policy constructor accepts it, so frames on it
   fail `KELD-IPC-005`. Its id is never reused. X05-T4 retires nothing.
5. **Gates.** Every new entry triggers wire-protocol review. A `Guarded` entry also
   triggers permission-model review.
6. **Peer reach (decided).** An app created by `keld create` carries a copy of
   `transport.ts` from its creation date, so a host can meet a peer whose table is
   shorter. The host therefore sends unprompted `EVENT` frames on a channel only to a
   role link that subscribed to it (criterion 16). The subscription is one `CALL` on
   `lifecycle`: `LifecycleRequest::Subscribe { channel: u16 }`, replied with
   `LifecycleResponse::Subscribed` or `LifecycleResponse::SubscribeRefused { reason:
   SubscribeRefusal }`. `SubscribeRefusal` is a closed enum, `UnknownChannel`,
   `NotSubscribable`, `GuardedNotSupported`, `RoleNotAdmitted`, with pinned
   discriminants; a new reason is a public-API and vector change. It names the target
   by id, so it works for `EVENT`-only entries that admit no app `CALL`, and one codec
   and one handler serve every event-bearing entry (no per-channel subscribe message).
   Admission is default-deny, in this order: the id must resolve through
   `channel_table::entry` (else `UnknownChannel`); the class must carry host `EVENT`s
   (`HostEvent` or `HostCallWithEvents`) and the entry must not be `lifecycle` (else
   `NotSubscribable`); the authority must be `HostInternal` (else
   `GuardedNotSupported`: guarded event subscription is future work with its own spec
   and `keld-guard` API); and the link's authenticated role must be admitted for the
   channel by its KEL-75 role declaration (else `RoleNotAdmitted`). The subscription
   binds to that link's principal only. Every refusal changes no state. **Handler
   owner:** the production primary router in `crates/keld-core/src/app_session.rs`
   (the `(FrameKind::Call, LIFECYCLE_CHANNEL)` arm, `app_session.rs:5730`), which owns
   the per-link state. The KEL-72 `LifecycleSession` (`crates/keld-core/src/lifecycle.rs:231`)
   serves no event entry, keeps no subscription state and answers every `Subscribe`
   with `SubscribeRefused { reason: NotSubscribable }`. Repeating a
   subscription is idempotent. The state is one bit per table entry per link, sized at
   link open and dropped with the link; there is no unsubscribe (YAGNI). A template
   that predates the channel never subscribes, so it never receives frames on it, and
   no protocol bump is needed: adding a `lifecycle` payload variant is a public-API and
   wire review item, not a frame or `HELLO` change (`crates/keld-ipc/AGENTS.md`).
   `lifecycle` is the baseline exception. App-initiated `CALL` channels need nothing
   further, because an older peer never sends on them. *Falsifier:* a consumer whose
   host must push an `EVENT` before the role can subscribe (for example, a fact the role
   needs in order to know the channel exists). That consumer needs a `PROTOCOL_VERSION`
   bump instead, and its spec must say so. A peer newer than its host (a downgrade) gets
   the host's existing terminal decode failure for the unknown `lifecycle` variant, the
   same skew as calling any channel the host lacks.

### 4.5 First-proof consumers and the entries they will add

No id below is allocated by this spec. Each row is the consumer's obligation under §4.4,
recorded so that their specs cite one rule. The name and class are proposals; the
consumer's approved spec owns the final values.

| Consumer | Proposed entry name | Direction and class | Authority | Notes |
|---|---|---|---|---|
| F02-T2 (#449) host window registry | `window-state` | host-to-app `EVENT` only: needs a new `HostEvent` class (admits no app `CALL`) | `HostInternal` (window facts about windows the app created) | lands `LifecycleRequest::Subscribe`, the per-link subscription state and the criterion 16 tests (§4.4 step 6); the role subscribes on `lifecycle`, never on `window-state`. The gh531 draft (#614) currently proposes `window` with app `CALL`s plus host `EVENT`s; either shape subscribes the same way |
| F04-T3 (#466) ipcMain facade over the control channel (structs owned by F04-T6, #540) | `compat-control`: one entry for both directions (decided, §4.5 note) | both directions | per-message checks are `windows.<w>.channels` exact-literal grants (F04-T8), which are not `app.*` capabilities; F04-T6 chooses `HostInternal` plus guard evaluation in the router, or a new `Authority` variant through the `keld-guard` owner | the name must not be `el-*` (criterion 5); Electron channel strings stay in the payload |
| F06-T2 (#478) dialog | `dialog` | app-to-host `GuardedCall` | `Guarded(dialog)` against the `app.system` `dialog` literal (closed decision F06-D3 A) | the `keld-guard` constant for `dialog` is added by F06-T2, not by X05-T4 |
| F06-T4 (#480) application menu | `menu` | app-to-host `GuardedCall` for the tree submission; menu activation back to the app needs an `EVENT` path (`HostCallWithEvents` or a second entry) | `Guarded(menu)` against the `app.system` `menu` literal (F06-D3 A) | activation EVENTs on a `Guarded` entry cannot be subscribed in this slice (`GuardedNotSupported`, criterion 16): F06-T4 either carries activation on a separate `HostInternal` event entry or first lands the guarded-subscription spec and `keld-guard` API |

**F04 control channel (decided, YAGNI):** one `compat-control` entry carries both
directions, distinguished by frame kind and direction, as `lifecycle` already does. A
later split takes a new id, which is purely additive under §4.4. *Falsifier:* F04-T6's
spec needs distinct admission per direction, meaning one `ReceiveClass` cannot express
both directions' policies. Then F04-T6 adds the second entry.

Next and parked dependants (F06-T12, F08-T1, F08-T4, X04-T2, F08-T7, F08-T8, F09-T5)
follow the same rule when they consume an entry. The gh527 draft's T4 credit lane would
add a per-entry "credited" property. That property is added to this table by gh527's
own PR, not to a parallel table.

### 4.6 macOS renderer bridge id source: the host, at construction

Decision: `keld-core` passes the admitted channel id to
`RendererBridgeEndpoint::new(requests, outcomes, admitted_channel: u16)`.
`crates/keld-core/src/app_session.rs:1867` already constructs the endpoint, so the
change is confined to that call. The bridge stores the id. It renders the two injected
scripts (`PAGE_FACADE_SCRIPT`, `ISOLATED_BRIDGE_SCRIPT`) at construction with that
number, replacing the literal `1`, and its Rust admission compares against it.
`keld-core` keeps its own second check (`app_session.rs:3322`) against `ECHO_CHANNEL`.

Reasons:

- Architecture 01 §3 gives `keld-wv` the single dependency `keld-guard`. A crate edge
  `keld-wv → keld-ipc` would change that normative table and make the webview
  abstraction depend on the wire crate for one number.
- Which channels a renderer may invoke is host policy. `keld-core` owns privileged
  routing, and per-window `windows.<w>.channels` grants (F04-T8) will make the admitted
  set per window. The id has to arrive from the host at construction in any case.
- The injected scripts are built once per bridge, off the hot path. Rendering a `u16`
  into them costs one allocation at window creation.

Rejected: a `keld-wv → keld-ipc` crate edge (architecture 01 change; wrong owner of the
policy); a keld-wv-local constant with a parity test (the mirror this spec removes).

### 4.7 Wire and version handling

- **Wire bytes: unchanged.** Ids 1, 2 and 3 keep their values. No frame, kind, flag,
  HELLO, payload or codec changes. `PROTOCOL_VERSION` stays `2`. The canonical corpus
  (`receiver-semantics-v0.tsv`) and its digest stay unmodified.
- **What would need a bump:** renumbering or reusing an allocated id; changing
  `HANDSHAKE_CHANNEL`; carrying a channel table in `HELLO` (destination handshake
  resolution). Each needs a `PROTOCOL_VERSION` bump, an architecture 02 update and
  wire review (`crates/keld-ipc/AGENTS.md`). §4.4 forbids the first two outright.
- **Additive entries** need no bump. §4.4 step 6 (subscription before unprompted
  `EVENT`s) is the reason, and its falsifier names the case that would need one.
- **Electron names:** no entry carries an Electron API or family name. Electron channel
  strings are payload data inside the control channel's codec (F04-T6, F04-T11).

### 4.8 Remaining template items

- **New types and channels:** §4.2. No new channel.
- **Capabilities and manifest (spec 03):** no manifest change. X05-T4 exports two
  existing `keld-guard` names as constants (§4.1); evaluation is unchanged.
- **Platform notes:** OS-independent. The macOS bridge is the only platform consumer.
  WebKitGTK and WebView2 have no renderer bridge in-tree.
- **Runtime seam:** none. Execution owners, OS grants, crash domains, handle lifetimes,
  ordering and config capture are unchanged.
- **Migration unit:** all callers are in-tree and listed in §4.2 and §4.6. There is no
  temporary adapter. The source-text parity test is deleted (criterion 14).
  `crates/keld-host/tests/fixtures/t1b_harness.ts` (`KEL96_ECHO_CHANNEL = 1`) is a test
  fixture outside criterion 7's production scope. X05-T4 MAY re-point it to the
  generated constant.

### 4.9 Rejected alternatives

- **Parity-test-only mirror.** Keep hand constants in each language and assert equality
  by source text. This is today's state. It covers only two of the five copies, it
  passes when both sides drift together in the parsed text, and root `AGENTS.md`
  forbids mirrored constants.
- **Per-family id ranges.** Reserve id blocks per Electron family (for example ipc,
  dialog, menu). They allocate speculatively (YAGNI) and put Electron family names into
  the host wire table. They also contradict F04-EPIC (#463)'s one fixed control channel
  with Electron channel strings as data. They are cut, not parked.
- **Handshake-time resolution now.** Exchange the table in `HELLO` and resolve ids from
  names. That changes `HELLO`, needs `PROTOCOL_VERSION` 3, a corpus change and wire
  review, and no first-proof consumer needs dynamic ids. It stays destination.
- **Full `keld gen` / `@keld/schema` now.** Out of scope for a three-entry table; it stays
  destination (architecture 02 §4, 06 §2).
- **New IDL or table file format (TOML/JSON) as source.** That would add a third
  language and a parser. Rust `const` data is already compile-checked, and the KEL-98
  generator already reads Rust.
- **A separate generated runtime file** (`channels.generated.ts`). It would add a staged
  runtime file and a Linux strict-profile bind, reopening KEL-136/KEL-98 staging review
  (§4.3).
- **A `keld-wv → keld-ipc` crate edge for the bridge.** See §4.6.
- **Generating the table from a Rust build step** (a test binary printing JSON). That
  requires a cargo build in the TypeScript lane and does not follow the KEL-98
  source-parse pattern.

## 5. Boundaries

- Implement in (X05-T4): `crates/keld-ipc/src/{channel_table.rs,lib.rs,echo.rs,lifecycle.rs,receive.rs,link.rs}`
  (`link.rs`: the `write_hello` literal only); `crates/keld-ipc/channel_allocations.txt`
  (new; already under the `/crates/keld-ipc/` CODEOWNERS line);
  `crates/keld-ipc/fuzz/fuzz_targets/raw_receive.rs`; `crates/keld-ipc/tests/receiver_corpus.rs`
  (policy lookup through `channel_table::entry`); `tools/ci_hygiene.rs` (the append-only
  rule);
  `crates/keld-guard/src/lib.rs` (two exported constants, re-pointed predicate);
  `crates/keld-native/src/fs.rs`; `crates/keld-core/src/app_session.rs` (bridge
  construction and privileged-receiver call sites);
  `crates/keld-wv/src/wkwebview/{macos_bridge.rs,mod.rs}`;
  `packages/@keld/kipc/{scripts/echo-codegen.ts,scripts/echo-codegen.test.ts,src/transport.ts,src/transport.test.ts}`;
  the no-literal scan test (Rust: `crates/keld-ipc/tests/`; TypeScript: next to the
  generator test).
- This PR (spec): `docs/specs/gh508-kipc-channel-table.md`;
  `docs/architecture/02-ipc.md` §2 (the "Correlation ids" bullet) and §4; generated `llms.txt` / `llms-full.txt`.
- Must not touch: workspace `Cargo.toml` and any crate's `Cargo.toml` dependency list
  (no new edge); `PROTOCOL_VERSION`; `receiver-semantics-v0.tsv`; no behaviour change
  on the HELLO path;
  `keld-guard` evaluation logic; architecture 01 §3 crate table; any generated file by
  hand.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 (X05-T4, #597, Rust half): `channel_table.rs` with the three entries,
  `HANDSHAKE_CHANNEL`, `validate_table` with its const assertion and fixtures, the
  `keld-guard` constants, derived constants in `keld-ipc` / `keld-native`, the
  entry-typed `privileged_call_receiver`, macOS bridge construction from the host, and
  the Rust no-literal and permission-literal scans, `check_allocations`, the committed
  baseline file and the `tools/ci_hygiene.rs` append-only rule. Criteria 1–6 (with 3a), 10–13 and 15.
  Criterion 16 belongs to F02-T2 (#449).
- [ ] T2 (X05-T4, #597, TypeScript half): the generator target, the generated region,
  `HANDSHAKE_CHANNEL` in `RECEIVE_POLICIES`, the TypeScript and injected-script
  no-literal scan, and deletion of the parity test. Criteria 7–9 and 14. X05-T4 MAY land
  T1 and T2 as one PR. T2 depends on T1's file.

## 7. Test plan

| Criterion | Test | Independent oracle |
|---|---|---|
| 1 | `channel_table` unit test listing `(name, id)` pairs; compile-fail doc test for the private constructor | the literal expected list in the test |
| 2–5 | `validate_table` fixtures: duplicate id, id 0, out of order, empty list, bad names; plus the const assertion on the real table | each fixture's expected `TableDefect` |
| 3a | in-crate `#[cfg(test)]` module of `channel_table.rs`: the real table against `channel_allocations.txt`; fixtures for a rising renumber (`probe` 4 → 5), a removed entry and an unrecorded append. `tools/ci_hygiene.rs` self-tests: a both-sides `fs` 2 → 4 renumber and a deleted line against a fixture base | the committed baseline file at the merge base; each fixture's expected `AllocationDefect` or hygiene error |
| 6 | Rust source scan over `crates/*/src` and `crates/keld-ipc/fuzz/fuzz_targets`, with the exact exclusions and two regexes of criterion 6 | zero matches; seeded `macos_bridge.rs` and `link.rs` literals each fail |
| 7 | Bun source scan over production TypeScript and the two injected scripts, for numeric channel comparisons and constants outside the generated region | a seeded-literal mutation |
| 8–9 | `echo-codegen.test.ts`: freshness, stale-region mutation, malformed-source mutation | committed bytes; exact error text |
| 10 | existing corpus rows `call-wrong-channel` and `primary-undeclared-channel` (unchanged); a new unit test applying id 4 to every live constructor; compile-fail doc test for `privileged_call_receiver(ChannelId(4))`; unit test for the class mismatch | `KELD-IPC-005` and zero handler effects; error detail |
| 11, 13 | existing golden vectors and `echo_link` / `raw_bytes` tests, unmodified; consumer-equals-entry unit tests in `keld-ipc` and `keld-native`; the generator fixture with `echo` at 4 | the corpus digest `375f50c4...` unchanged; rendered `ECHO_CHANNEL = 4` |
| 12 | `macos_bridge.rs` unit test: `BridgeState` built with admitted id 7 rejects 1 and admits 7; the rendered script contains `7` and no `channel !== 1` | the `KELD-WV-011` detail text |
| 14 | grep assertion in the TypeScript scan that the parity expectations are gone | none needed beyond the scan |
| 15 | Rust source scan of production code outside `crates/keld-guard/src` for string literals equal to any `keld_guard::capability` constant | a seeded `"fs.read"` literal in `channel_table.rs` |
| 16 | lands with F02-T2 (#449), not X05-T4: `keld-core` link tests for an unsubscribed role (zero frames), a subscribed role (one frame per event), refused `Subscribe` targets (unallocated id, `echo`, `lifecycle`, a `Guarded` fixture entry, an undeclared `app-bound` role), and a second role's link staying silent; pinned postcard bytes for `Subscribe` in `lifecycle.rs` | frame counts on the link; removing the subscription or class check fails |

Anti-flake: every test is a pure source or const check, or an existing deterministic
fixture. None depends on timing, ports or platform. The macOS bridge test is a
`BridgeState` unit test (no WKWebView). Platform-only paths: none new.

## 8. Review gates triggered

- **wire protocol:** yes. This is the channel-id allocation rule and the table of record
  for `channel:u16`. Values and bytes are unchanged.
- **public API:** yes. `keld_ipc::channel_table` (new: `ChannelEntry`, `ReceiveClass`,
  `Authority`, `TableDefect`, `validate_table`, `entry`, `HANDSHAKE_CHANNEL`,
  `CHANNEL_TABLE` and the entry constants); `privileged_call_receiver` parameter and
  return type; `RendererBridgeEndpoint::new` gains a parameter; TypeScript gains
  `FS_CHANNEL` and `HANDSHAKE_CHANNEL`. `check_allocations` and `AllocationDefect` are
  `#[cfg(test)]` and not public. F02-T2 later adds `LifecycleRequest::Subscribe`,
  `LifecycleResponse::{Subscribed, SubscribeRefused}` and `SubscribeRefusal`.
- **permission model:** yes (listed by #508's body). The `Authority` declaration
  references `keld-guard` names, and X05-T4 adds two exported `keld-guard` constants.
  Guard evaluation is unchanged.
- **unsafe:** none. **dependency addition:** none (no new crate edge or package).

## 9. Perf impact

None. Entries are `const`; every receive-path comparison is the same `u16` comparison
as today. Bridge script rendering happens once per window construction, outside every
architecture 01 §5 budget path. No bench is required.

## 10. Open questions

None. The five earlier questions were decided under the owner's delegation, recorded
2026-10-07. Each decision is reversible, and each falsifier is stated where it applies:
1. capability literals: §4.1 and criterion 15;
2. the F04 single entry: §4.5;
3. EVENT registration (a `lifecycle` `Subscribe` call naming the target id): §4.4
   step 6 and criterion 16;
4. the architecture 02 §2 amendment: §2;
5. emitting every entry: §4.3.
