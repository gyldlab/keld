# Spec: ipcMain registration table without delivery (GH-466 registration slice)
Status: draft
Linear: GH-466 (#517) · Owner: @0monish · Updated: 2026-10-31

Tracker and claim: [GH-466](https://github.com/gyldlab/keld/issues/466), claim comment
`6547652253` (2026-10-31). This document owns the registration half of the GH-466
`ipcMain` facade: the listener registry and the invoke-handler table. It consumes GH540
(#1122) for the delivery contract it does not implement and GH-547 for invoke dispatch.
The implementation sits behind this spec on the same branch; `Status: draft` means it
awaits the owner's exact-text approval and the branch is not merged.

## 1. Goal & non-goals

Problem (FACT, pinned drawio-desktop `src/main/electron.js` at
`2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`; census §A.4 rows 56–59 and 63): the main
module registers 26 `ipcMain.on`/`ipcMain.once` listeners on 24 distinct string-literal
channels and removes 5 of them with `ipcMain.removeListener`. Two registrations run at
module top level (`:3426` `export`, `:4431` `rendererReq`); the rest run inside
`whenReady` (`:987–2187`), `createWindow` (`:702–982`) and `exportDiagram`
(`:3058–3424`). electron-store 11.0.2 registers `electron-store-get-data` at import
(`index.js:25`), and electron-log 5.4.4 registers `__ELECTRON_LOG__` through
`ipcMain.on` and `ipcMain.handle` (`src/main/index.js:16` and `:28`, via
`ElectronExternalApi.onIpc`/`onIpcInvoke`) because its `src/index.js` takes the main
branch when `process.type` is `browser`, which `@keld/electron` sets at import.
`@keld/electron` exports no `ipcMain`, so module evaluation stops at `:3426` with a
TypeError (census row 56) before `whenReady` ever runs.

Outcome: `import { ipcMain } from "electron"` binds on both the named and the default
export; every registration above succeeds and is observable through the EventEmitter
listener array (`listenerCount`, `listeners`) or through the invoke-handler table's
one-per-channel rule; nothing else happens. **No message is delivered by this slice.**
No `IpcMainEvent` or `IpcMainInvokeEvent` is minted; there is no sender, no `reply`, no
`webContents.send`, no grant evaluation and no invoke dispatch. A registered listener or
handler is therefore never invoked by Keld: not because it is dropped, but because no
sender exists yet. The GH-466 delivery slice adds the sender route behind these tables,
which do not change.

Non-goals:

- message delivery, `IpcMainEvent` instances, `sender`, `senderFrame`, `reply` and
  `webContents.send` (GH-466 delivery slice under the GH540 contract; #543 Deliver);
- `el:<channel>` grant evaluation and the navigation-generation mint (#541, #552, #542):
  registration authorizes nothing;
- invoke dispatch, `IpcMainInvokeEvent` instances and Electron's message-only error
  transport (GH-547);
- `event.returnValue`/`sendSync` (GH-548) and `ports` (GH-468): absent from the exported
  types until their owners add them;
- the other named exports draw.io imports (#565) and `electron` specifier resolution
  (#502/#569);
- any `keld migrate` grant enumeration (X02-T3, #499): literal-ness is a source property
  only the migrate scan can judge.

## 2. Spec refs

- [Architecture 04 §3](../architecture/04-electron-compat.md) layer 2: main-process
  modules are TypeScript over `@keld/api` with event semantics replicated against the
  conformance suite; §3 already names `ipcMain` as a later slice. This adds its
  registration half.
- [Architecture 02 §5](../architecture/02-ipc.md): `ipcRenderer.send/on` ⇄ EVENT frames
  on the `el:*` compat namespace; `ipcRenderer.invoke`/`ipcMain.handle` ⇄ CALL/REPLY.
  Nothing here touches a frame; the tables are the endpoint state those routes consume.
- GH540 §4.1 and §4.10 ([#1122](https://github.com/gyldlab/keld/pull/1122), draft):
  EventEmitter registry for on/once/off, a separate invoke-handler map, Electron's
  internal no-op `error` listener, listener returns never become replies. This spec
  implements exactly that registration shape and none of §4.10's transport, codec or
  sender rules.
- [GH508 §4.5](gh508-kipc-channel-table.md) (approved): one family-neutral
  `compat-control` entry; Electron channel strings stay in the payload. No channel-table
  change.
- [gh532](gh532-first-proof-evidence-rules.md) and [gh566](gh566-corpus-manifest-owner.md):
  one corpus `electron-ipc-v1`, red-until-implemented cells, byte-exact pinned citations.
- Electron pin v44.4.5 `694f45852a0f1726cd23bfd379854de489cccb65`: `docs/api/ipc-main.md`
  (cells), `lib/browser/ipc-main-impl.ts` (handler semantics),
  `docs/api/structures/ipc-main-event.md` and `ipc-main-invoke-event.md` (types).
- Tickets consumed, not owned: #464 (cells), #541/#552/#542 (grants, identity), #547
  (invoke), #548, #468, #565, and #589's acceptance that every later `ipcMain`
  registration is present.

No deviation from the architecture documents; no architecture file changes.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given `@keld/electron`, when a module runs `import electron, { ipcMain } from "electron"`,
   then `ipcMain` is an instance of `node:events` `EventEmitter`,
   `electron.ipcMain === ipcMain`, and `ipcMain.listenerCount("error")` is 1 (Electron's
   no-op listener).
2. Given a fixture that replays draw.io's 26 `on`/`once` sites, 5 `removeListener` sites
   and the two library registrations with `KELD_APP_LINK` unset, when it runs, then it
   exits 0 with empty stderr and prints exact counts: 24 channels, `app-load-finished` 3
   and every other channel 1 (26 in total); after the removals the five named channels
   are 0 and 21 listeners remain; `electron-store-get-data` and `__ELECTRON_LOG__` are 1;
   every registered function was called 0 times.
3. Given listeners a, b, c registered with `on`, when `off(channel, b)` runs, then
   `listeners(channel)` equals `[a, c]`, and `off` for an unregistered function changes
   nothing. Given `[a, a, once]`, `removeListener(channel, a)` leaves `[a, once]` and
   `removeListener(channel, once)` removes the once registration by its original
   function. `addListener` interleaved with `on` yields the registration order with
   duplicates kept.
4. Given listeners on two channels, when `removeAllListeners(one)` runs, then only that
   channel is empty; when `removeAllListeners()` runs with no channel (observer child),
   then every channel including `error` is empty and `eventNames()` is empty.
5. Error case: given `handle(channel, fn)`, when `handle(channel, fn2)` runs, then it
   throws an `Error` whose message starts
   `KELD-COMPAT-098: Attempted to register a second handler for '<channel>'` and names
   `removeHandler` as the fix. Given an occupied channel and a non-function handler, the
   error is still `KELD-COMPAT-098` (duplicate check first, as in the pinned source).
   Given a free channel and a non-function handler, `handle` throws a `TypeError` whose
   message starts `KELD-COMPAT-099: Expected handler to be a function, but found type`
   followed by the `typeof` result, and records nothing.
6. Given `handleOnce(channel, fn)`, when `handle(channel, fn2)` runs, then it throws
   `KELD-COMPAT-098` (a self-removing wrapper is recorded); `handleOnce` with a
   non-function argument does not throw at registration (pinned source parity).
   `removeHandler(channel)` lets `handle` register again; `removeHandler` on an absent
   channel is a no-op.
7. Given a listener and a handler on one channel, `removeHandler` leaves the listener and
   `removeAllListeners(channel)` leaves the handler (registry separation).
8. Given registration of any kind, nothing is invoked: every callback counter stays 0;
   `electron.IpcMainEvent` and `electron.IpcMainInvokeEvent` are `undefined` at runtime
   (types only); the fixture of criterion 2 opens no link (a link attempt without
   `KELD_APP_LINK` is `KELD-IPC-007` on stderr).
9. Given the package typecheck, then `IpcMain`, `IpcMainEvent`, `IpcMainInvokeEvent`,
   `IpcMainListener`, `IpcMainInvokeHandler` and `WebFrameMain` are exported types;
   listener arguments are `unknown[]`; channel parameters are `string`; `returnValue`
   and `ports` are absent from `IpcMainEvent`; `new IpcMainEvent()` does not compile.
10. Given the committed corpus `electron-ipc-v1`, when
    `cargo test -p keld-compat --test corpus_registry` runs on macOS, then its 7
    registration cells pass with byte-exact citations and its 4 delivery cells
    (`ipc-main.on`, `ipc-main.once` → GH-466; `ipc-main.handle.invoked`,
    `ipc-main.handle-once` → GH-547) stay `expected_verdict: fail` with mapped tests
    that assert today's behaviour.
11. Given `docs/engineering/keld-error-codes.md`, when
    `cargo test -p keld-cli --test error_registry` runs, then `KELD-COMPAT-098` and
    `KELD-COMPAT-099` are registered and emitted by
    `packages/@keld/electron/src/ipc-main.ts`.

## 4. Design

### 4.1 First-principles and reuse decision

Facts. The Bun main role owns application listeners (GH540 §4.1); the host owns routing
and authorization. Registration touches no handle, process, socket or file, so there is
no process-boundary, OS-grant or crash-domain change: `No boundary change` in the root
`AGENTS.md` sense (no handle ownership, crash ownership or principal minting moves).
Trust: registering a channel grants nothing; the only authorization is the #552 guard on
delivery, which this slice never reaches. Failure: registration can fail only on a
duplicate invoke handler or a non-function handler, both synchronous, typed and
application-visible.

Reuse. Electron's own `IpcMainImpl` (pinned `lib/browser/ipc-main-impl.ts`) is
`EventEmitter` plus one `Map` of invoke handlers and a no-op `error` listener. The
listener registry is the runtime `node:events` EventEmitter, which the `BrowserWindow`
facade already sits on; the handler table is the framework-owned `stateMap` the `app`
facade already uses for its tables. The only unmet requirement is the handler table's
one-per-channel rule, which EventEmitter does not provide and Electron implements in
nine lines; those nine lines are reproduced, not redesigned.

Rejected alternatives:

- Duplicating `app.ts`'s private `stateMap` listener list for `ipcMain`: rejected. It
  would hand-write once-wrapper removal, duplicate preservation and one-occurrence
  removal that the upstream EventEmitter already provides, and `app.ts`'s table is
  deliberately special-cased for the one-shot `ready` gate and is not an EventEmitter.
- Reusing `@keld/api`'s single-handler channel map for listeners: rejected (GH540
  §4.10). One handler per channel is the invoke table's rule, and sharing one registry
  is the named registry-separation fault.
- A runtime literal or grant allowlist at registration: rejected. Registration is not
  authorization (one rule, one owner: the #552 guard), literal-ness is unobservable at
  runtime, and the library channels `electron-store-get-data` and `__ELECTRON_LOG__` are
  excluded from grants by GH499 A05, so refusing them at registration would break
  import.
- A typed early `TypeError` in `handleOnce`: rejected. Pinned Electron wraps without
  checking, and mirroring the source avoids an undocumented divergence; the GH-547 owner
  may revisit this with a scoreboard entry.
- Exporting no event types: rejected. The listener type needs an event parameter,
  exported interfaces cost no runtime, and a TypeScript app compiles against exactly the
  member subset GH-466 owns. Members other tickets own are left off the type so their
  absence is a compile error, not a silent `undefined`.

Compatibility fallback: not required (the surface was absent). Performance: none
claimed; registration is cold application code.

### 4.2 Decisions recorded

| Decision | Selected | Why |
|---|---|---|
| Channel-name validation | Any `string` channel registers; no runtime literal, grant or emptiness check. Non-string channels are a compile error on the typed surface and otherwise follow EventEmitter and `Map` behaviour, as in Electron | Literal-ness belongs to the migrate scan (#499, X02-T3) and authorization to the #552 guard (§4.1) |
| Duplicate `handle` | Throws `Error` carrying `KELD-COMPAT-098`, Electron's exact sentence and the fix, before the type check | Mirrors the pinned source; the code prefix follows the `KELD-COMPAT-011` message convention |
| Non-function `handle` argument | Throws `TypeError` carrying `KELD-COMPAT-099` and Electron's exact sentence | Mirrors the pinned source |
| `once` semantics | EventEmitter's: the wrapper is recorded, `listeners()` unwraps it, removal works by the original function, and the wrapper is removed before the callback runs. Nothing in this slice calls it | Upstream facility, verified differentially on Bun 1.4.4 |
| `IpcMainEvent` types | Exported now as interfaces with the GH-466 members (`type`, `processId`, `frameId`, `sender`, `senderFrame`, `reply`); no value binding; `returnValue` and `ports` deferred to GH-548 and GH-468 | §4.1 |
| Error codes | `KELD-COMPAT-098` and `KELD-COMPAT-099`, allocated from the high end of the free range because #565 allocates from `KELD-COMPAT-013` in parallel | The registry test accepts non-contiguous codes |

### 4.3 Types and shape (sketch of the real code)

```ts
export interface WebFrameMain {
  readonly url: string;
  readonly origin: string;
}
export interface IpcMainEvent {
  readonly type: "frame";
  readonly processId: number; // host-provenance opaque id, never an OS pid
  readonly frameId: number;
  readonly sender: WebContents;
  readonly senderFrame: WebFrameMain | null;
  reply(channel: string, ...args: unknown[]): void;
}
export interface IpcMainInvokeEvent {
  readonly type: "frame";
  readonly processId: number;
  readonly frameId: number;
  readonly sender: WebContents;
  readonly senderFrame: WebFrameMain | null;
}
export type IpcMainListener = (event: IpcMainEvent, ...args: unknown[]) => void;
export type IpcMainInvokeHandler = (event: IpcMainInvokeEvent, ...args: unknown[]) => unknown;
export interface IpcMain extends EventEmitter {
  on(channel: string, listener: IpcMainListener): this; // once, addListener, off, removeListener alike
  removeAllListeners(channel?: string): this;
  handle(channel: string, listener: IpcMainInvokeHandler): void;
  handleOnce(channel: string, listener: IpcMainInvokeHandler): void;
  removeHandler(channel: string): void;
}
class IpcMainImpl extends EventEmitter implements IpcMain {
  readonly #invokeHandlers = stateMap<string, IpcMainInvokeHandler>();
  constructor() {
    super();
    this.on("error", () => {}); // Electron: a message on channel "error" must not throw
  }
  handle = (channel: string, listener: IpcMainInvokeHandler): void => {
    if (stateMapHas(this.#invokeHandlers, channel)) {
      throw new Error(`KELD-COMPAT-098: Attempted to register a second handler for '${channel}'. …`);
    }
    if (typeof listener !== "function") {
      throw new TypeError(`KELD-COMPAT-099: Expected handler to be a function, but found type '${typeof listener}'. …`);
    }
    stateMapSet(this.#invokeHandlers, channel, listener);
  };
  handleOnce = (channel: string, listener: IpcMainInvokeHandler): void => {
    this.handle(channel, (event, ...args) => {
      this.removeHandler(channel);
      return listener(event, ...args);
    });
  };
  removeHandler(channel: string): void {
    stateMapDelete(this.#invokeHandlers, channel);
  }
}
export const ipcMain: IpcMain = new IpcMainImpl();
```

`ipcMain.emit` stays the application's own EventEmitter method, as in Electron; Keld
never calls it, and it is not delivery.

- Capabilities required; manifest changes (spec 03): none.
- Wire/protocol changes (spec 02): none.
- Platform notes: none; pure JavaScript state. Corpus cells declare `macos` as the
  first-proof lane (gh566 D13), so other hosts list them `unknown`.
- Runtime seam: none.
- Migration unit: none; no persisted state. The delivery slice adds the sender route and
  mints events behind these tables; the tables and their public methods do not change.

## 5. Boundaries

- Implement in: `packages/@keld/electron/src/ipc-main.ts` (new), `index.ts` (export
  lines only), `ipc-main.test.ts`, `ipc.test.ts`, `ipc-main.typecheck.ts`,
  `fixtures/ipc_main_registration.ts`, `fixtures/ipc_main_remove_all.ts`;
  `crates/keld-compat/fixtures/ipc-corpus/`, the shared snapshot
  `crates/keld-compat/fixtures/doc-snapshots/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/ipc-main.md`,
  one `REGISTRY` entry in `crates/keld-compat/tests/support/corpus_manifest.rs`;
  `docs/engineering/keld-error-codes.md`, `compat-scoreboard.md`, `product-status.tsv`
  and their generated views.
- Must not touch: `@keld/api`, the `@keld/kipc` transport, `keld-guard`, `keld-ipc`,
  `keld-wv`, `keld-core`, workspace `Cargo.toml`, CI workflows, root `AGENTS.md`,
  architecture documents, or any sender, codec or grant path.
- Overlap declared: PR #1130 (`agent/gh-464-ipc-baseline`) adds the same corpus
  directory, snapshot and `ipc.test.ts` as absence baselines. This branch re-records
  those three cells: `ipc-main.remove-listener` flips to pass because its cited sentence
  is registration-only, while `ipc-main.on` and `ipc-main.once` stay red. The later PR
  rebases. PRs #1306/#1308 and #565 also touch `index.ts`; that overlap is one export
  line each.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [ ] T1 — this spec at `Status: draft`; exact-text approval by the owner.
- [ ] T2 — conformance entry first: corpus `electron-ipc-v1`, shared snapshot, `REGISTRY`
      line, fixtures, mapped tests and the compile-only type probe (red at that commit;
      proven failing before T3).
- [ ] T3 — `ipc-main.ts`, `index.ts` exports, error-code registry entries, regenerated
      `llms-full.txt` (green).
- [ ] T4 — scoreboard rows and the product-status ledger row with the T3 commit as
      `last_verified_sha`.

T1–T4 land on one branch as one draft PR; no merge before approval.

## 7. Test plan

| AC | Test | Negative control (the one mutation that fails it) |
|---|---|---|
| 1 | `ipc-main.test.ts` `ipc-main.event-emitter`; fixture line `GH466_BINDINGS` | export a plain object with hand-written `on`: `instanceof` fails |
| 2 | `fixtures/ipc_main_registration.ts` through `observe()` with `KELD_APP_LINK` unset; literal expected lines | remove the `ipcMain` export: Bun export-not-found SyntaxError; add `app.whenReady()` to the fixture: `KELD-IPC-007` on stderr |
| 3 | `ipc-main.off.removes-listener`, `ipc-main.remove-listener`, `ipc-main.add-listener.alias` | clear the channel in `off`, or deduplicate in `addListener`: the exact array comparison fails |
| 4 | `ipc-main.remove-all-listeners` plus observer `fixtures/ipc_main_remove_all.ts` | leave the `error` listener in the no-channel form: `names=0` fails |
| 5 | `ipc-main.handle.registers`; parity test for `KELD-COMPAT-099` | check the type before the duplicate: an occupied channel plus `42` yields `-099`, and the assertion fails |
| 6 | `ipc-main.remove-handler`; parity test for `handleOnce` | record nothing in `handleOnce`: the following `handle` succeeds |
| 7 | registry-separation parity test | share one table: `removeAllListeners` deletes the handler |
| 8 | `ipc.test.ts` red cells (zero calls, no event value); fixture line `GH466_DELIVERED calls=0` | export an `IpcMainEvent` value or invoke a listener: the red test fails, which is the gh532 rule 3 flip trigger |
| 9 | `ipc-main.typecheck.ts` `@ts-expect-error` probes under `bun run typecheck` | widen a parameter to `any` or `string`: an unused directive fails the typecheck |
| 10 | `cargo test -p keld-compat --test corpus_registry` | edit one quote byte: `QuoteAbsent`; drop `implementing_ticket` from a red cell: rule 3 rejection |
| 11 | `cargo test -p keld-cli --test error_registry` | emit a `KELD-COMPAT-100` without a heading: registry mismatch |

Anti-flake: no timing dependence; observer children exit on their own and the 4 s
timeout is a kill switch only; channel names are unique per test and cleaned up in
`afterEach`; the no-channel `removeAllListeners()` runs only in a child because
`bun test` shares one module cache across files. Differential verification of Bun 1.4.4
`node:events` (duplicates, one-occurrence removal, once unwrapping and pre-callback
removal, `error` throw without a listener) ran before the tests were written.

## 8. Review gates triggered

Public API: new exports `ipcMain`, `IpcMain`, `IpcMainEvent`, `IpcMainInvokeEvent`,
`IpcMainListener`, `IpcMainInvokeHandler`, `WebFrameMain`, and codes `KELD-COMPAT-098`
and `KELD-COMPAT-099`. unsafe: none. Permission model: none (registration grants nothing
and no guard path is touched). Dependency addition: none. Wire protocol: none.

## 9. Perf impact

None. No budget of architecture 01 §5 moves; registration is cold application code with
no steady-state allocation.

## 10. Open questions

1. Exact-text approval of this spec by the owner (`Status: draft` until then; no merge).
2. Confirm the `KELD-COMPAT-098`/`KELD-COMPAT-099` allocation against #565's parallel
   allocation from `KELD-COMPAT-013`.
3. Reconciliation order with PR #1130: whichever lands second rebases onto the other's
   `ipc-corpus` and `ipc.test.ts`; this branch's re-recorded red cells are the intended
   end state.
