# Spec: echo Rust-schema to TypeScript contract codegen
Status: approved
Linear: KEL-98 · Owner: GYLDLAB · Updated: 2026-09-19
Approval: repository-owner maintainer session · approved content head `1be5ed88ccb1035df4e7c8fe2d5e6c98a2b80bd8` · [public receipt](https://github.com/gyldlab/keld/pull/257#issuecomment-5741478846) · decision SHA-256 `f1583577d2edcc3a1dc351415f675d8ec6e22103353a170920b865e4b9570ec5`

{"schema":"keld.kel98-approval/v1","decision":"approved","approved_content_head":"1be5ed88ccb1035df4e7c8fe2d5e6c98a2b80bd8","approver_id":"amishabenramani","source":"active-maintainer-session-2026-09-19","public_receipt":"https://github.com/gyldlab/keld/pull/257#issuecomment-5741478846"}

## 1. Goal & non-goals

Eliminate the hand-written TypeScript copy of Keld's already-live echo request/response
shape. The existing Rust `keld_ipc::echo::{EchoRequest, EchoResponse}` definitions are
the proposed canonical source for the compile-time payload declarations in this bounded
Phase 2 slice. The existing TypeScript postcard codec remains a separately tested runtime
conformance implementation. A deterministic maintainer generator will emit checked-in
TypeScript payload declarations consumed by the hello scaffold, and falsifiable drift
tests will prove that supported Rust field changes reach the declarations and that the
retained codec still emits the Rust-owned wire bytes.

This is a bootstrap for the existing echo vertical slice, not Keld's general contract
authoring model. The destination `@keld/schema` / `keld gen` design remains separately
specified as TypeScript-native.

Non-goals:

- no general Rust parser, derive/proc macro, IDL, or arbitrary schema generator;
- no live `keld gen` command or `@keld/schema` package;
- no new Rust crate, Cargo/Bun dependency, wire version/opcode, or permission concept;
- no generated runtime codec, transport, authorization, or hostile-byte validator;
- no Rust-only/native-role implementation or KEL-75 runtime generalization;
- no streaming, cancellation, MessagePort, shared-memory, or performance claim.

## 2. Spec refs

- `docs/architecture/02-ipc.md` §2: postcard remains the live runtime codec and wire owner.
- `docs/architecture/02-ipc.md` §4: general destination contract/schema generation.
- `docs/architecture/06-runtime-and-tooling.md` §2: `keld gen` is target-only; current
  hello transport is the actual shipped vertical slice.
- `docs/specs/kel136-generated-ts-app-link-transport.md`: one canonical TypeScript
  transport owner and dependency-free scaffold embedding.
- `crates/keld-ipc/src/echo.rs`: current Rust echo schema and runtime handler.
- Linear KEL-13: accepted RFC direction for Rust-derived TS in the original echo work.
- Linear KEL-98: implementation owner and compile-drift acceptance.

Architecture 02's general destination remains TypeScript-native. This spec proposes one
explicit bounded exception: because the echo Rust structs shipped before general schema
authoring, KEL-98 will derive the first echo TypeScript payload declarations from those
existing Rust structs to remove today's duplicate hand-written interfaces. The approval
PR records that proposal without changing current implementation status. It does not
change the eventual `@keld/schema` authoring direction.

KEL-98's original “TS types + client stub” acceptance will be satisfied by the
implementation PR binding the already-live `AppLinkSession.echo` client stub to the
generated Rust-owned payload types.
The generator must not emit a second client method/interface whose operation name or
request/response pairing is absent from its canonical Rust input.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given current `EchoRequest` / `EchoResponse`, when the generator runs twice, then the
   generated TypeScript bytes are identical and exactly match the committed artifact.
2. Given a supported Rust field rename/addition/type change, when regeneration runs,
   then the TypeScript contract changes deterministically; stale output makes freshness
   checking fail.
3. Given the generated current declarations, the pinned TypeScript compiler accepts the
   exact hello source composition that ships as `src/main.ts` (`kipc.ts` +
   `main-body.ts`) together with the canonical transport; given `EchoRequest.count`
   renamed to `attempts` in an isolated Rust-source mutation and regenerated output, that
   same production composition fails compilation at its stale field use.
4. Given an unsupported Rust field type or unsupported echo-struct syntax, when codegen
   runs, then it fails closed with an actionable generator error instead of guessing a
   TypeScript representation.
5. Given the generated artifact, the hello adapter imports and re-exports only generated
   `EchoRequest` / `EchoResponse` payload types through `import type` / `export type`;
   the already-live `AppLinkSession.echo(request: EchoRequest): Promise<EchoResponse>` is
   the client stub and is typed directly by those generated payloads. No generated
   `EchoClient` and no second hand-written payload declaration exist.
6. Given `keld create`, the scaffold contains `src/echo.generated.ts` byte-identical to
   the committed generated artifact so editors/typecheckers can resolve the type edge.
7. Given `keld dev`, the owner-private runtime stage does not need to copy
   `src/echo.generated.ts`; the pinned Bun runtime executes the staged main successfully
   with that type-only file absent. Static ownership checks require `import type` /
   `export type`; the runtime negative control adds a real value use of a generated-only
   symbol and must fail module resolution when the staged file is absent.
8. Given malformed or lossy wire input, existing postcard/golden/receiver tests remain
   the runtime rejection oracle. Generated TypeScript types alone never make hostile
   bytes valid and never bypass `KELD-IPC-*` validation.
9. Given the existing `echo-call-valid` row in
   `crates/keld-ipc/tests/fixtures/receiver-semantics-v0.tsv`, the Rust postcard encoder
   and retained TypeScript echo codec both produce its exact non-symmetric payload bytes;
   swapping field order in either implementation as an isolated mutation makes that
   implementation's fixture assertion fail.
10. `keld gen` remains `KELD-CLI-045` and `@keld/schema` remains specified-only after
    this slice. The product-status ledger and generated view describe bounded echo
    declaration generation as live while general schema codegen remains absent.
11. Existing echo wire bytes for the current schema remain unchanged; the implementation
    adds no frame/opcode/version/capability/dependency.
12. The approval PR continues to describe the live seven-file scaffold and hand-written
    TypeScript payload mirror as current. Only the implementation PR, after the generated
    artifact is live, updates every exact current scaffold-file inventory/adapter description
    to the new eight-file/generated-type state.

## 4. Design

### First-principles and reuse decision

**Ownership.** Rust `EchoRequest` and `EchoResponse` already define the handler's
accepted postcard shape. For this bounded bridge they become the single owner of the
compile-time payload declarations; TypeScript receives generated declarations rather
than a manually repeated interface. The hand-written TypeScript postcard codec remains
a runtime conformance implementation and independently repeats the required field order,
so it stays bound to Rust through the shared `echo-call-valid` wire fixture. Generated
interfaces are not a wire-order oracle. The generator does not own the wire, runtime
validation, permissions, process identity, or lifecycle.

**No boundary change.** This slice changes no handle ownership, crash ownership, or
principal minting. It adds no runtime file or authority edge to the staged process.

**Reuse.** Reuse the existing `@keld/kipc` Bun/TypeScript package, its pinned Bun 1.4.2
and TypeScript 7.0.2 graph, the existing hello scaffold, the existing postcard codec and
golden vectors, and the repository's deterministic generate/check/test pattern. A new
crate, proc macro, `syn`, `ts-rs`, `schemars`, or universal IDL is unnecessary for two
small existing structs and is rejected for this slice. The existing
`receiver-semantics-v0.tsv` `echo-call-valid` row is reused as the one shared byte oracle
instead of introducing another echo-vector fixture.

**Generator boundary.** `packages/@keld/kipc/scripts/echo-codegen.ts` is cold maintainer
tooling. It reads only `crates/keld-ipc/src/echo.rs`, recognizes the named public
`EchoRequest` and `EchoResponse` struct bodies, preserves declaration order, and maps
only the field types required by the current slice:

- Rust `String` → TypeScript `string`;
- Rust `u32` → TypeScript `number`, documented as a Rust-u32 domain whose runtime bound
  remains enforced by the existing postcard codec.

The parser is intentionally not a Rust grammar. It accepts the rustfmt-normalized public
field form used by these two structs and explicitly admits `///` field-doc lines inside
their bodies. It ignores derive/doc material outside the bodies, but rejects in-body
field attributes, arbitrary non-field lines, generics, tuple/private fields, and every
unsupported type. Expanding that admitted syntax is a future reviewed schema change, not
an implicit best-effort conversion.

**Generated artifact.** The sole output is
`crates/keld-cli/templates/hello/src/echo.generated.ts`, marked `@generated`. It contains
only the two Rust-owned compile-time payload declarations:

```ts
export interface EchoRequest { message: string; count: number; }
export interface EchoResponse { message: string; count: number; }
```

The exact formatting is generator-owned and deterministic. The generated file contains
no client interface, operation name, codec, import, runtime value, permission metadata,
channel table, or transport logic. `kipc.ts` uses `import type` / `export type` for these
declarations; the existing `AppLinkSession.echo` method remains the client stub and is
typed directly with `EchoRequest` / `EchoResponse`. Its existing `encodeEchoRequest` and
`decodeEchoResponse` stay runtime code and therefore become compile-time consumers of
the generated field names/types.

**Source-time versus runtime.** `keld create` will emit `src/echo.generated.ts`. The
staged runtime will not copy it because production references must be syntactically
`import type` / `export type` and the generated declarations have no runtime values. This
keeps the KEL-136 runtime file surface unchanged: staged `main.ts` plus
`kipc-transport.ts`. Bun 1.4.2 erases imports whose bindings are used only as types even
when ordinary import syntax is used, so runtime absence alone is not an oracle for the
syntax rule. A static ownership assertion pins `import type` / `export type`; the
separate runtime negative control introduces a real value use of a generated-only symbol
and proves module resolution fails when the source-time file is absent. Any real runtime
value dependency on the generated file reopens staging and strict-profile review.

**Freshness and compile-drift oracle.** The generator exposes `generate` and `check`
commands through package scripts. Its Bun test imports the same generator functions,
compares current render bytes to the committed artifact, and exercises a supported
source mutation. TypeScript 7.0.2's package does not expose the JavaScript compiler API
on the pinned graph, so the compile-drift test runs the already-installed local compiler
through pinned Bun (`process.execPath`, `run`, `tsc`) from the package directory. It
builds an isolated copy of the exact shipping source composition: canonical transport,
`kipc.ts`, `main-body.ts`, and generated payload declarations. The baseline must compile;
a generated `count`→`attempts` mutation must fail that production composition. A
negative-control variant that reintroduces local hand-written payload declarations would
make the mutation compile and therefore must be detected by the separate one-owner
source assertion. No `bunx`, global `tsc`, compiler API, or network access is admitted.

**Wire-order conformance oracle.** The Rust `positive_vectors_match_the_live_codec_bytes`
test already binds `EchoRequest { message: "kipc", count: 3 }` to the shared
`echo-call-valid` payload. The TypeScript echo test will read that same row and require
`encodeEchoRequest({ message: "kipc", count: 3 })` to produce identical bytes. Two
independent temporary mutations are required: swap the Rust declaration order while
leaving the TypeScript codec unchanged and require the Rust fixture assertion to fail;
then restore Rust, swap the TypeScript encode/decode order, and require the TypeScript
fixture assertion to fail. This proves the retained codec's field order without claiming
that interface declaration order controls postcard.

**Trust and failure.** Generated declarations are developer compile-time assistance only.
They are not authentication, authorization or payload validation. Existing
`validateReceivedHeader`, postcard decode, Unicode/domain checks and guard dispatch stay
load-bearing and must remain independently tested.

Capabilities / manifest changes: none.

Wire/protocol changes: none. `PROTOCOL_VERSION`, frame kinds, channel numbers, postcard
field order and existing current-schema bytes remain unchanged.

Platform notes: generator and typecheck are platform-neutral repository tooling. Runtime
acceptance relies on the already-pinned Bun behavior and existing macOS/Windows/Linux
product tests; no new OS support cell is created.

## 5. Boundaries

Implement in:

- `packages/@keld/kipc/scripts/echo-codegen.ts` and focused generator tests;
- `packages/@keld/kipc/{package.json,tsconfig.json}` only to expose/typecheck the scripts;
- `crates/keld-cli/templates/hello/src/echo.generated.ts` (generated, never hand-edited);
- `crates/keld-cli/templates/hello/src/kipc.ts` only to consume generated types, plus
  focused `kipc.test.ts` ownership and shared-wire-fixture assertions;
- `crates/keld-cli/src/template.rs` and existing create/template tests to emit the
  source-time artifact;
- `docs/engineering/product-status.tsv` plus generated `product-status.md`, recording
  live bounded echo declaration generation while general `@keld/schema` / `keld gen`
  codegen remains absent;
- architecture 02/onboarding 04 current-vs-target wording and generated `llms-full.txt`;
- `docs/onboarding/03-api-and-cli-surface.md` plus any exact current scaffold-file/staging
  mirror that becomes stale when the generated source is emitted;
- `crates/keld-cli/src/verb.rs` only if its stale diagnostic must point from KEL-13/30 to
  KEL-98; `Gen` remains reserved.

Must not touch:

- workspace `Cargo.toml`, KIPC frame/opcode/version, `keld-guard`, permission manifests,
  role/principal ownership, runtime supervisor, Linux strict mounts, webview backends,
  native-role implementation, `@keld/electron` compatibility semantics, or CI workflow;
- current postcard/golden-vector behavior except compile-time type imports needed to
  consume the generated contract.

## 6. Tasks

- [ ] T1 — approval PR: land this spec plus the bounded architecture/onboarding
  current-vs-target clarification. No implementation starts while status is `draft`.
- [ ] T2 — one vertical implementation PR: add the dependency-free Bun generator and
  deterministic artifact; make the existing hello client consume and `keld create` emit
  it without staging it at runtime; land freshness, unsupported-shape, exact-production
  compile-drift, one-owner/type-import, generated-value runtime, and shared-wire-order
  tests with those changes; update the product-status source ledger/generated view and
  every stale current-state document; then run exact-tip review/gates and close KEL-98
  only when all original acceptance rows pass. The generator, consumer, and load-bearing
  tests must not land in separate PRs.

## 7. Test plan

| Criterion | Proof |
|---|---|
| AC1–2 | generator deterministic/freshness Bun tests using current and supported-mutated Rust source |
| AC3 | pinned local `bun run tsc` over the exact composed hello `src/main.ts` + transport: baseline exits 0; regenerated rename mutation exits nonzero at production field use |
| AC4 | current Rust source with in-body `///` docs succeeds; unsupported `u64`, in-body attribute/arbitrary line, private/tuple/generic or malformed field fixtures fail explicitly |
| AC5 | repository search/source assertion proves generated `EchoRequest`/`EchoResponse` are the sole TS payload declarations, no generated `EchoClient` exists, and `AppLinkSession.echo` is typed directly by imported generated payload types; a local-declaration mutation must trip this ownership assertion |
| AC6 | `template.rs` byte-equality test plus `keld create` integration asserts `src/echo.generated.ts` exists |
| AC7 | static source assertion pins `import type`/`export type`; Bun 1.4.2 staged fixture runs with the generated file absent, while a mutation that performs a real runtime value use of a generated-only symbol fails module resolution; existing staged/no-flag tests remain green |
| AC8 | existing Unicode, numeric-domain, hostile receiver and Rust postcard golden-vector suites remain unchanged/green |
| AC9 | Rust and TypeScript tests consume the same `receiver-semantics-v0.tsv` `echo-call-valid` payload; independent Rust-declaration-order and TypeScript-codec-order mutations each fail their consumer |
| AC10 | CLI reserved-verb tests still require `keld gen` → `KELD-CLI-045`; `product-status.tsv` and generated `product-status.md` distinguish live bounded echo declaration generation from absent general `@keld/schema` codegen |
| AC11 | existing Rust/TS literal wire vectors remain byte-identical; `PROTOCOL_VERSION` unchanged |
| AC12 | approval diff preserves current seven-file/hand-written status; implementation diff search updates every exact scaffold-file inventory and handwritten-interface description only after the generated artifact exists |

Failure-first / negative controls:

- alter one supported Rust request field and regenerate: the exact shipping hello source
  composition must fail TS compile at its stale field use;
- restore local hand-written payload declarations in the adapter: the one-owner assertion
  must fail even though that mutation could make the compile-drift fixture green;
- bypass freshness comparison: stale-artifact test must fail;
- change a generated-only symbol from type-only use to real runtime value use while
  withholding the generated file from the staged fixture: Bun runtime must fail module
  resolution; ordinary-import syntax alone is not an accepted negative control;
- change generated output by hand: freshness check must fail;
- substitute unsupported Rust `u64` or an in-body field attribute: generator must refuse
  rather than guess a TypeScript representation;
- swap the Rust echo field declaration order while leaving the TypeScript codec unchanged:
  the Rust consumer of the shared `echo-call-valid` bytes must fail;
- restore Rust, then swap the TypeScript echo codec field order: the TypeScript consumer
  of the same shared bytes must fail.

No sleeps or network access. Temporary compile fixtures use isolated temporary paths and
the repository-pinned Bun + local TypeScript 7.0.2 compiler executable.

## 8. Review gates triggered

- unsafe: none.
- public API: yes — the generated scaffold gains `src/echo.generated.ts`, and the hello
  TypeScript source exposes generated request/response payload types used by its existing
  client stub.
- permission model: none; generated types convey no authority and no runtime file is
  added to strict containment.
- dependency addition: none.
- wire protocol: none; runtime bytes and KIPC v2 are unchanged.

Independent architecture/public-contract review is required on the exact final diff.

## 9. Perf impact

None claimed. The generator and compile-drift proof are cold tooling. Runtime code keeps
the existing postcard codec and transport path, and the generated declarations erase at
runtime. No benchmark is required unless implementation introduces a runtime value edge
or changes generated-app startup work; either event reopens this section before merge.

## 10. Open questions

None. The implementation intentionally leaves the general `@keld/schema` source-of-truth
and live `keld gen` product surface to a later approved slice; this spec only removes the
current echo interface duplication and proves compile-time drift.
