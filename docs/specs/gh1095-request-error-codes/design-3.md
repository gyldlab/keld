# Design 3: Backward compatibility

Part of: [index](index.md)
Owns: Design 3: Backward compatibility

## 4. Design: Backward compatibility

- **Still `KELD-WV-011`:** S1, S2, S3b and S4–S7. Every one of these returns
  before the host attempts a write. The registry meaning "rejected before
  application dispatch" becomes true again of every outcome the renderer can
  see.
  - `WvError::RendererBridge` (Rust side) also uses `KELD-WV-011` for
    bridge-install and reply-encode failures (`macos_bridge.rs:511-618`, `:843`).
    Those are construction or host faults, not call outcomes, and this
    revision leaves them unchanged.
- **No longer `KELD-WV-011`:**
  - S8 moves to WV-012;
  - S9, S10 and S16 move to WV-013.
- **Still `KELD-API-001`:**
  - S12–S14;
  - every registration-time error that `channels.handle` throws synchronously
    to app code: reserved lifecycle channel, undeclared descriptor, a handler
    that is not a function, and a duplicate handler. These errors are not call
    outcomes.
- **No longer `KELD-API-001`:** S11 moves to API-002.
- **Shape is unchanged** (§3 criterion 10). Detail strings are diagnostic, not
  part of the contract; only the code prefix is. No rule in this spec, the
  KEL-142 note or the registry branches on a detail string.
- **Effect on existing callers:**
  - A caller that treats every rejection as "handle the error" sees no
    difference.
  - A caller that matched WV-011 as "safe to resend" was wrong for S9, S10 and
    S16. After this revision, those cases carry WV-013, which such a caller does
    not match, so it stops resending them. The change makes callers safer, not
    less safe.
  - `@keld/electron` has no `ipcRenderer`/`ipcMain` today, so no Electron
    caller exists.
- **No dual emission and no adapter.** Each call gets exactly one code.
- **Migration unit.** Each implementing task updates, in the same PR, the
  in-repo assertions on the old strings:
  - `crates/keld-host/tests/no_flag/macos/public_fs.rs:755-768@516644b0e1c086baa08ff8e8b52667f4bc97144a`;
  - `crates/keld-host/tests/no_flag/macos/public_fs/outcomes.rs:88@516644b0e1c086baa08ff8e8b52667f4bc97144a`;
  - `crates/keld-core/src/app_session/tests/native_renderer_main_entry.rs:162-167@516644b0e1c086baa08ff8e8b52667f4bc97144a`;
  - `packages/@keld/api/src/echo-call.test.ts`;
  - `packages/@keld/api/src/channels.test.ts`;
  - the `macos_bridge.rs` unit tests.

  The `KELD-WV-011` registry `fix` and the `WvError::RendererBridge` `Display`
  text stop recommending a document reload. The `Display` pin at
  `crates/keld-wv/src/error.rs:196@516644b0e1c086baa08ff8e8b52667f4bc97144a` changes with it.
- **Interim mismatch (blocker owned by T-4).** T-1 already removes the reload
  advice from the registry `fix`. `error.rs:109-110` still says "Reload the
  current main-frame document" until T-4 lands, because changing it is
  production code.
