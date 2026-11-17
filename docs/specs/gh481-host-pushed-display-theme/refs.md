# References and current-source facts

Part of: [index](index.md)
Owns: section 2

Tracker context: Linear GH-481 is tracked under GitHub issue #517; KEL-102 is its reference.

## 2. Spec refs

- `docs/architecture/03-security.md` §2, `docs/architecture/04-electron-compat.md`
  §3–4 and `docs/architecture/05-webview-and-native.md` §3: app principal,
  host-owned UI handles, platform-native facts and provenance.
- **Approved** `docs/specs/gh531-window-registry-close-state-machine.md`,
  especially §4.d `MirrorSource<Fact>`/`attachMirror`, the *single* authenticated
  window subscription (#531 AC-20) and retained window/generation/replay contract.
  Source of truth: `@keld/api` mirror is host-pushed, not a native-provider copy
  in Electron facade. The spec explicitly already says F06-T5 reuses it.
- **Approved technical design** in [GH446 draft PR #657](https://github.com/gyldlab/keld/pull/657),
  exact approval/evidence on [GH445](https://github.com/gyldlab/keld/issues/445#issuecomment-6371519139):
  raw Host Ready alone is insufficient; public `app.whenReady()/isReady()` and
  `ready` wait for **accepted** window Subscribe, full replay and the correlated
  `Subscribed` REPLY. Its source implementation is still **unmerged**. A fake
  ready-before-subscription is an independent negative control.
- `docs/specs/gh527-worker-owned-blocking-call-transport.md` §4.6: one
  `setStateApplier` per channel; host facts before blocking REPLY apply before Bun
  synchronous continuation, but user listeners run after that continuation.
- [GH449 active window registry/consumer stack](https://github.com/gyldlab/keld/issues/449),
  including [draft #1153](https://github.com/gyldlab/keld/pull/1153). **Not on
  main:** there is no live screen/theme mirror or compatible facade today.
- Pinned upstream Electron **v44.4.5** tag commit
  [`694f45852a0f1726cd23bfd379854de489cccb65`](https://github.com/electron/electron/tree/694f45852a0f1726cd23bfd379854de489cccb65),
  exact [`docs/api/screen.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/screen.md),
  [`docs/api/native-theme.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/native-theme.md)
  and [`docs/api/structures/display.md`](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/structures/display.md).
- [GH477 test-only draft #1156](https://github.com/gyldlab/keld/pull/1156)
  already has four GH481 **expected-fail**, macOS-only cells for the two getters,
  `Display.workArea` as DIP Rectangle and dark-mode boolean (current head
  `260974757fe92490d11bbcbf579901a263af32a4`), *not* KELD native
  acceptance. GH-566 is the sole corpus manifest/parser/runner owner.

**Spec relationship:** This draft proposes extending the **same** GH-531 window
EVENT vocabulary with *one new global desktop fact*; GH531 currently lists
per-window event discriminants 0–6 and limits those to their owner generation.
No production code may append discriminants or change the replay grammar until
its exact owner approves the **narrow global-fact amendment**. Merely declaring
GH481 approved without that independent GH531 composition review is not enough.
