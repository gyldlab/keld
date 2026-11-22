# Design 9: Structured error representation (audit and the scoped fix)

Part of: [index](index.md)
Owns: Design 9: Structured error representation (audit and the scoped fix)

## 4. Design: Structured error representation (audit and the scoped fix)

The question for each surface: can an app read a machine-readable `error.code`
without parsing the message string? The audit is at `516644b0`.


- Surface: WebView JS API (`window.keld.invoke` rejection)
- Code readable without parsing?: **No.** The page gets an `Error` whose `message` is `"<code>: <detail>"` and that has no `code` property
- Evidence: `fail` `macos_bridge.rs:46`, result rejection `:57-59`, `pagehide` `:67`

- Surface: Page-world relay (`result-v1` `postMessage`, internal)
- Code readable without parsing?: Yes, but internal: `code` and `detail` fields, observable by page scripts, not a contract
- Evidence: `__keldBridgeNativeResult` `:137-147`; isolated-world busy result `:159-165`

- Surface: Native renderer bridge
- Code readable without parsing?: Yes, inside Rust: `PageOutcome.code: Option<String>`; `RendererBridgeOutcome::Error.code: String`
- Evidence: `macos_bridge.rs:653-661`, `:234-259`

- Surface: Host
- Code readable without parsing?: Yes, inside Rust: renderer outcomes carry `code: String`; `HostAppError::code()`
- Evidence: `app_session.rs:4219`, `4235`, `4276`, `4283`; `crates/keld-core/src/app_session/error.rs:88@516644b0e1c086baa08ff8e8b52667f4bc97144a`

- Surface: IPC `CallError` (wire)
- Code readable without parsing?: **Yes:** `code: String` beside `message`
- Evidence: `crates/keld-ipc/src/call_error.rs:59-65@516644b0e1c086baa08ff8e8b52667f4bc97144a`

- Surface: `@keld/kipc`
- Code readable without parsing?: **Mixed.** ERR replies and link-terminal codes reject with an own enumerable `code` (`codedError`, `linkError`; `isCallError`). Local validation
  and decode failures use `kipcError`, a plain `Error` with the code only in `message`, so `isCallError` is false for them
- Evidence: `transport.ts:1254-1262`, `1388-1390`, `1118-1135` (`errorFromErrFrame`), `1067-1074`; `kipcError` `227-229`, used at `1052`, `1059`. A malformed ERR, empty or not a
  `CallError`, also becomes a plain `kipcError` with no `code` (`errorFromErrFrame`, `:1126`, `:1132`). On the `WorkerLink` path the same malformation ends the link instead
  (`:2311`)

- Surface: `@keld/api`
- Code readable without parsing?: **Mixed.** Host ERRs (fs, auth) reach the app as `KeldCallError` with `code`, re-exported with `isCallError`. Local failures are plain `Error`s
  with the code in `message`: an unset link (`KELD-IPC-007`), `apiError` for `channels.handle` registration, and the fs response-variant mismatch
- Evidence: `index.ts` exports; `app.ts:76-82`; `channels.ts:28-30`; `fs.ts:16`, `26`

- Surface: Rust error types
- Code readable without parsing?: **Mixed.** `code()` exists on `HostAppError`, the keld-guard refusal types and `keld-ipc` admission. `WvError` and `IpcError` expose the code only
  through `Display`
- Evidence: `crates/keld-core/src/app_session/error.rs:88@516644b0e1c086baa08ff8e8b52667f4bc97144a`; `crates/keld-guard/src/lib.rs:347@516644b0e1c086baa08ff8e8b52667f4bc97144a`,
  `507`; `crates/keld-ipc/src/admission.rs:63@516644b0e1c086baa08ff8e8b52667f4bc97144a`; `crates/keld-wv/src/error.rs:55-110@516644b0e1c086baa08ff8e8b52667f4bc97144a`;
  `crates/keld-ipc/src/lib.rs:114-138@516644b0e1c086baa08ff8e8b52667f4bc97144a`

- Surface: CLI/MCP `KeldErrorObject`
- Code readable without parsing?: **Yes:** `code`, `message`, `cause`, `fix`, `docs` fields
- Evidence: `crates/keld-cli/src/error_object.rs:11-22@516644b0e1c086baa08ff8e8b52667f4bc97144a`

**Finding.** The surface this spec revises, the `window.keld.invoke`
rejection, is the one where an app must parse `message` to tell
`KELD-WV-012` (resend allowed) from `KELD-WV-013` (resend forbidden). That
parsing is exactly the decision [historical §4.4](design-4.md) depends on, so a public improvement is
justified here.

**Design: an additive `code` property (T-4, keld-wv only).**

- **Prefix rule (r2, one rule, one owner).** For a rejection with code `c`
  and detail `d`:

  ```text
  message = d               if d starts with c + ": "
          = c + ": " + d    otherwise
  ```

  - The rule is idempotent. It is applied **once**, by the page facade's
    coded-error helper, which is the rule's single owner.
  - The host forwards a Bun ERR's `CallError.message` as the detail
    **unchanged**. It MUST NOT prefix it, so the prefix logic at
    `app_session.rs:5789` on #1105 is removed.
  - The host's own refusals and retirements send a bare detail, and the
    facade prefixes it.
  - Today the facade's test is `detail.startsWith(code)`. r2 tightens it to
    `startsWith(code + ": ")`. Every message Keld emits already has that form,
    so their bytes are unchanged (criterion 10).
  - A Bun-supplied detail like `"KELD-API-001 failed"` (code but no `": "`)
    becomes `"KELD-API-001: KELD-API-001 failed"`. That is the only byte
    change.
- The page facade gets one coded-error helper. It is the only place in
  `PAGE_FACADE_SCRIPT` that constructs an `Error`. It builds the `message` by
  the prefix rule:
  - `fail(detail)` gives `"KELD-WV-011: " + detail`, unchanged;
  - a result goes through the rule;
  - `pagehide` gives its existing text under its [historical §4.2](design-2.md) code.
- The helper then defines an own `code` property with the same attributes
  `@keld/kipc` `codedError` uses (`enumerable`, `writable` and `configurable`
  all true). A page rejection and a Bun `KeldCallError` therefore share one
  shape, `Error & { code: string }`.
- The value is the result's `code`, or `KELD-WV-011` for local facade
  refusals. That is the same code as the message prefix.
- No `detail` property is added, because the detail is not contract ([historical §4.3](design-3.md)).
- **Spec-level type.** The rejection type below is recorded for the future
  generated web client. No TypeScript declaration ships today; no package
  declares `window.keld`.

  ```ts
  interface KeldInvokeError extends Error { readonly code: string }
  ```
- **Backward compatibility.**
  - `message`, `name`, `instanceof Error` and `String(err)` are unchanged.
  - `code` is enumerable, so `JSON.stringify(err)`, `Object.keys(err)` and
    object spread (`{ ...err }`) now include it. This is intended: `code` is a
    stable public field, enumerable exactly like `@keld/kipc` `codedError`'s.
    These are the only visible differences.
- **Scope.**
  - The property is set at the facade's single constructor, so it covers every
    invoke rejection.
  - The values this spec defines are `KELD-WV-011/012/013` and
    `KELD-API-001/002`.
  - Any other code passed through from Bun (after the S10b rewrite) appears
    verbatim, and its meaning belongs to its owning spec.
- **Tests:** AT-EC (§7).

**Hand-off: the wider migration (out of scope for #1096).** The rest of this
belongs to the separate contract-architecture program. Its issue is being
created by another worker; no number exists yet. It covers:

- giving `kipcError` local failures and `@keld/api` local failures a `code`
  property, so `isCallError` is true for them;
- adding `code()` to `WvError` and `IpcError`;
- one error-object shape across JS and Rust, aligned with `KeldErrorObject`'s
  `fix` and `docs`;
- a diagnostic detail for `KELD-API-001` causes (Q-2);
- a generated `window.keld` TypeScript declaration.
