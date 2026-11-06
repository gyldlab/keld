# W4 integration proposal — unapplied owner handoff

Status: draft executable proposal. The current conditional v3 supersedes the historical fragments below. No Windows product activation or shared-owner approval is implied.

Owning issue: [GH1186](https://github.com/gyldlab/keld/issues/1186). Shared contract owners: [GH481](https://github.com/gyldlab/keld/issues/481), [GH446](https://github.com/gyldlab/keld/issues/446), and the existing GH531 window subscription. The GH1188/W2 lead owns application of any native-loop or Core-coordinator change.

## Current conditional v3

Actual tested immutable W2 PR1403 input: `fe9a4ab8d69823c8ead58281ab1715a811a75835`, `webview2/mod.rs` blob `ec493b0bc16250a19dd4cc0d8615d7870a364748`. The current `integration.patch` has 20 additions/5 removals in five hunks; SHA-256 `e0b5ceca8c735e57d03f4088550d4de40cfebbc719572c16985fcc6543a76a27`. It remains unapplied and is not independently buildable: the portable desktop types/event, callers and Core admission are absent from this base.

At 2026-11-05 11:36:57Z, current pushed PR1403 was OPEN/draft at `f7b5848556154aa1c356a214270e342369267c0e`. Its native-loop source has the identical blob and SHA-256 `03bc98199f767d24cce99e9f1db3c242301491ad07ddca7fb91d3d2e8a53c370`. The unchanged patch targets only that file; the existing application and bounded helper proof carry forward by exact input equivalence, not a relabeled rerun.

The private optional Host callback is taken once under the existing genuine first-Init guard. Its result preserves three states: no callback means unselected/no desktop event; a callback returning `None` publishes a genuine unavailable observation; populated facts publish the snapshot. The callback runs synchronously on the UI owner and must not re-enter that owner's borrowed context.

The corrected order is **capture completes, timestamp is recorded, DesktopObserved is sent, then HostInitialized carries that timestamp**. A scheduling pause after the first enqueue must not make timely completed capture late. Genuinely slow capture must still expire, so recording time before capture is also wrong. This follows the retained e09b S1 producer and owner specification, without a new clock/deadline exception.

[The exact-source v3 review](https://github.com/gyldlab/keld/issues/1186#issuecomment-6656342047) records isolated check/apply success and unchanged failure-to-fatal/Option behavior. The source-derived helper fixture and its actual commands, results and limitations are described in EVIDENCE.md. They do not establish Windows UI execution, Core admission or old-generation completion fencing.

The preceding v2 (SHA-256 `ba87decaf3157ba07be47f6edb02c657e3cf240fa43fc09f0a6ac253593ab25a`, 15 additions/4 removals) applies to fe9a but timestamps after the first enqueue. It is retained only as the negative input `integration-v2-negative.patch`. Before production application, W2 must recheck its then-current source and qualify the complete approved composition. Do not apply this fragment to the W4 product tree to manufacture compilation.

## Historical source bases

| Source | Inspected revision | Meaning |
| --- | --- | --- |
| Current main / this task worktree | `df99602ef10abd2cd6f3b5c633ba6d1cffee97bc` | No shared desktop extension is present on this base. |
| Lead PR1397 | `50dcc36afc0a10fcfe97846d244ff63342de35c0` on `agent/gh-1188-release-receipt-drain-20261104` | Exact source for the native seam patch; PR was open/draft when inspected. |
| Shared S1 implementation | `e09b520eaee1e4bd69bd0bde2137ec52f0fed18a` | Immutable existing implementation, inspected with `git show` on the Mac owner repository. It is not on main or PR1397. |
| Branch containing S1 | `agent/gh-502-macos-mvp-composition-20261101`, inspected head `22f845714515f337c3d51b82c04335afa7d6f827` | The inspected desktop mirror, codec, router, and canonical specification are unchanged from e09b. |
| Published GH481 draft PR1164 | `5de312ec0c8517cbb57733baaebc63e5d9ccd343` | This published specification draft is not the S1 implementation or a Windows approval. |

The complete e09b commit changes 50 files. Do not cherry-pick that entire composition into the W4 tree. The shared owner must publish or explicitly compose the portable subset onto the approved lead baseline. W4 feasibility uses immutable existing shared-source inputs; it must not create another maintained mirror.

## Historical v1 fragment (superseded)

The original v1 patch contained five hunks in `crates/keld-wv/src/webview2/mod.rs`: 17 additions and 4 removals. This section is historical; current v3 ordering is specified above.

The only proposed entry-point parameter is an optional, Host-supplied first-Init capture:

```rust
Option<Box<dyn FnOnce() -> Option<crate::AppDesktopFacts>>>
```

The outer option is selection: `None` keeps the current unselected path. The inner option is the actual native result: `Some(capture)` returning `None` emits the existing S1 unavailable observation. These meanings must not be collapsed. The callback is a private Rust composition input, never an application-provided callback or a new transport operation. It must own or reference only the native sampler's ordinary Host-session state and execute synchronously on the UI owner.

On the actual Tao first Init turn, the selected callback is taken and invoked exactly once. The helper sends the existing S1 `DesktopObserved { facts }`, then sends `HostInitialized { observed_at: Instant::now() }`. Thus the native timestamp follows completion of capture, validation, and enqueueing of the desktop result. The patch leaves the existing fatal return path, release-receipt handling, and loop-entry watch in place. The two existing helper tests are adjusted to pass `None`; no claimed ordering tests are supplied by those trivial argument changes.

The patch deliberately does not choose a Windows selector encoding, add a sampler stub, manufacture launch facts, add a public Ready event, or implement post-Init refreshes. It depends on the owner work described below. All callers of the changed method require coordinated updates by the W2/Core owner. Existing/unqualified paths must pass `None`; only an approved selected Host path may pass the native capture.

### Historical mechanical checks

- All five old hunk ranges match the exact fetched PR1397 `50dcc36a` source, including original line numbers. The source blob is `83ddb34abf2dfc78a685324af44c44912d31658f`.
- On Ramani, `git apply --check tools/windows_desktop_facts/integration.patch` passed against this worktree's `df99602e` source. This is a context/applicability check, not application or compilation.
- `git apply --numstat` reported `17 4 crates/keld-wv/src/webview2/mod.rs`.
- After the check, the actual product file and HEAD both had Git blob `6e00f50b0d1d009b48cd1f103c7433aa9509430e`; the product file was unchanged.
- Patch SHA-256: `14ffc09e134dfee41d2038a44d6942128b1f7f20ae576b98ae659ef011c4664d`.
- No Cargo/Bun build, GUI run, native process, or production patch application was performed for this artifact.

The later PR1397 head `a12b996bb75105da8e2f1b54eb8a8cbe309d5dbb` was fetched by exact Git object for a second check. Its `webview2/mod.rs` blob was `831bed27fb3f2324a8465fcfb1423fff0521c927`. The unchanged patch passed `git apply --check` in an isolated temporary Git fixture containing those exact bytes; the live lead worktree was not used or modified. Receipt: `D:\\WORK\\keld\\.git\\keld-closeout\\chatgpt-w4-desktop-20261105\\seam-current\\receipt.json`.

The lead's subsequent draft PR1400 was also checked at `2aed1f944a072d2c84469924c107b3ca5df9bc13` on `agent/gh-1188-exit-wake-retirement-20261105` (base: the PR1397 branch). Its native-loop blob was `922c33231a1c96298cf0a48f0806ba8958dcfd22`. The identical patch passed another isolated context check, with an empty product diff. This records applicability to that exact evolving head, not approval of its lifecycle changes or authority to apply the seam.

The current fragment must be checked against its reviewed W2 source in an isolated directory, as the helper driver does. Do not replay the historical v1 command against the W4 product tree. A context check is not a compiling integrated slice.

## Required shared-owner composition, split from the native seam

The following is an exact dependency map, not authorization to edit these files from W4. A blanket macOS-to-Windows cfg replacement is unsafe because the same files also contain unrelated macOS authentication/dialog paths.

| Owner-controlled path / symbol | Required narrow delta and retained contract |
| --- | --- |
| `crates/keld-wv/src/engine.rs`: `AppDesktopRect`, `AppDesktopDisplay`, `AppDesktopFacts`, `AppWindowEvent::DesktopObserved` | Compose the existing e09b owned-data definitions. Widen only these desktop definitions from `#[cfg(target_os = "macos")]` to `#[cfg(any(target_os = "macos", windows))]` after Windows contract review. Retain S1's removal of `Copy/Eq` from the event enum. Amend AppKit-only unit/identity comments to the reviewed platform contract. Do not restore the older macOS-only HostInitialized cfg over the lead's Windows work. |
| `crates/keld-wv/src/lib.rs`: desktop reexports | Apply the same narrow desktop cfg widening. The seam's `crate::AppDesktopFacts` resolves only after this prerequisite. |
| `crates/keld-ipc/src/window.rs`, its codec fixtures, `docs/engineering/keld-error-codes.md` | Reuse S1 tag 7, the existing complete optional snapshot schema, checked u64 epoch, and typed 049 unavailable result emitted by `window_router.rs::desktop_unavailable`. No new channel, frame version, authority, or substitute error number. |
| `crates/keld-core/src/app_session/window_router.rs`: desktop fields/helpers and tests | Compose the existing `DesktopEpoch`, pending Subscribe state, qualification, completion, observation, and retirement logic; widen only their cfgs. Keep the existing current-generation/writer/window lock order. Reuse one ordered desktop → survivor windows → correlated Subscribed batch. No subscriber bit or replay while pending, no event to unadmitted/stale roles, and no replacement map/router. |
| `crates/keld-core/src/app_session.rs`: `PrimaryRouter::finish_start` and caller options | Select `WindowSession::with_desktop` only for the approved immutable Host-selected Windows path. The e09b source currently selects it under a macOS cfg; widening that branch to all Windows sessions would incorrectly change existing Windows Ready. The native callback, Core requirement, and facade selection must come from the same owner-controlled decision. Its encoding remains an approval gate. |
| `crates/keld-core/src/app_session.rs`: Init receiver/coordinator | Port the S1 one-preceding-desktop deadline exception onto the lead's current Windows coordinator, without replacing its release/lifecycle work. Require the selected desktop sample before HostInitialized, reject duplicate pre-Init observations, and preserve both the 5,000-ms caller deadline and 15-second Init watchdog. A late receiver may inspect the authentic on-time Init timestamp after at most one preceding desktop event; no event stream may renew the deadline. Unselected Windows sessions retain the current path. |
| `packages/@keld/api/src/window-codec.ts`, `window-source.ts`; `packages/@keld/kipc/src/transport.ts` | Reuse the existing S1 decoder/mirror and ordered application point. Retain full-snapshot validation, equal-epoch consistency, increasing epochs, explicit invalidation, and retired-source refusal. Local screen/theme getters must issue no transport frames. |
| `packages/@keld/api/src/window-close-receipts.ts`, `packages/@keld/electron/src/browser-window.ts` | Compose both S1 fixes for windowless desktop events before dereferencing `fact.window`. A codec/mirror-only test cannot prove these real consumers safe. Coordinate BrowserWindow changes with the W2 lead. |
| `packages/@keld/api/src/app.ts`, canonical preload/transport producer, `screen.ts`, `native-theme.ts` | Preserve S1's single public-Ready/mirror source and refusal surface. The current `nativeFacadeBootSelected && hostLaunchFacts !== undefined` selector is approved only for macOS development, because only that Host emits the qualifying launch record. Windows activation needs a separately reviewed immutable Host-owned selection carried through the canonical preload. Do not synthesize macOS launch facts, read mutable app environment, or widen the selector by OS string. |

The W4 fixture's unavailable control exposed the missing `KELD-CORE-049` catalog entry in main. This draft copies only that already approved S1 entry verbatim under its [narrow claim](https://github.com/gyldlab/keld/issues/1186#issuecomment-6646655855); all nine error-registry tests then passed. The original macOS S1 scope remains in the row. The required rebuilt `just llms` output adds exactly the same seven lines to `llms-full.txt`; the index stays byte-identical, all six generator tests pass, and `llms-check` passes. The actual portable Core error producer and Windows selection remain part of the owner composition above.

### The post-Init update dependency is separate

In the exact e09b source, the router can accept later desktop observations, but the Core coordinator rejects `DesktopObserved` after Init: its branch checks `init_seen` and `desktop_seen`, and its receiver rejects an additional desktop event after the first allowance. The macOS native producer contains only the Init emission. Therefore ordered-update tests at the router/mirror boundary do not prove production native refresh delivery.

The owner proposal must distinguish two states: the single required pre-Init sample, and ordinary selected native refresh observations after Init. A reviewed post-Init path would route later genuine observations through the same router/writer without rearming Ready, resetting the epoch, or giving them a watchdog exception. Its invalidation source and application on Windows remain unapproved. The supplied patch implements only the first sample seam. It does not conceal this missing dependency.

## Windows identity mapping proposal

**Recommendation:** a bounded Host-session map from a complete observed native target-set identity to the existing nonzero u32 mirror ID. This is an explicit proposed Windows interpretation of S1's ID field, pending GH481 review.

The key for one logical monitor is the canonical sorted set of all clone-target tuples in its validated `nativePaths` records:

```text
[
  adapterLuid.high,
  adapterLuid.low,
  targetId,
  targetDevicePath.toLowerCase()
]
```

The adapter LUID is Windows' locally unique adapter identifier. Preserve the exact signed high word and unsigned low word as separate integers; do not round a combined 64-bit value through a JavaScript number. Retain the raw source/target tuple as evidence. Source IDs, GDI names, HMONITOR pointers, enumeration positions, dimensions, and theme values are not identity keys.

The canonical set must be nonempty, sorted by explicit lexical comparison of the serialized full tuples, and contain no ambiguous duplicate target records. Each native target path must be the complete validated nonempty path from the native API. The case-normalization and clone-group interpretation are part of the pending contract, not a claim of cross-reboot hardware identity. A changed target-set membership is a different logical identity. A missing or ambiguous native match yields unavailable.

Proposed bounds and lifetime:

1. The current feasibility adapter admits exactly one logical display. More than one display is captured as raw native evidence but yields unavailable for the shared logical projection until mixed-DPI layout is independently qualified.
2. At most 32 unique native target tuples per logical identity. Larger or malformed sets yield unavailable.
3. At most 32 distinct canonical identities for the entire Host session. IDs are allocated once as checked nonzero u32 values, starting at 1, after complete fact validation. Every accepted tuple maps to its own retained ID.
4. Retain removed identities; do not evict or reuse IDs. Returning identities, reordered native records, a new primary role, and Bun recovery reuse the same map. Host restart may create a new map and different IDs.
5. The 33rd distinct identity yields an unavailable observation without partially changing the map. A later return to a retained identity can again resolve its existing ID; this does not revive a retired role or an expired Ready waiter.
6. Forced-colors/high-contrast observations currently yield unavailable for the effective-dark projection until the appearance policy is separately qualified. The native probe still records those facts.
7. Preserve the shared bounds: no zero ID, exactly one valid primary, at most 32 active displays in the shared codec, checked positive rectangles, finite positive scale, and a complete encoded event no larger than 32 KiB. The stricter one-display feasibility projection is stated separately.

A session allocation number is an opaque reference to a verified native tuple. It is not the index of the current enumeration, an Electron display ID, a truncated pointer, or a constant singleton display. Since the observed machine can have one display, actual live evidence may naturally show ID 1. Contract controls must prove that a different complete tuple receives a different ID and that the original tuple keeps its ID after reorder/removal/recovery.

### Alternative evaluated: keyed deterministic u32 hash

| Property | Host-session map | Keyed u32 hash plus full-identity collision guard |
| --- | --- | --- |
| Native input | Same complete validated tuple set | Same complete validated tuple set |
| Mapping | Retained one-to-one lookup; allocate next unused ID | Hash with one fixed Host-session key; reduce to u32 |
| Refusal | Missing/ambiguous identity or 32-entry exhaustion | Same failures, plus zero hash or collision |
| History needed | All previously allocated identities, up to 32 | All previously assigned IDs and full identities, up to 32; checking only currently connected displays is insufficient |
| Reorder/recovery | Stable within Host session | Stable only while the same key and history survive |
| Additional machinery | Checked counter and bounded map | Chosen keyed algorithm, key initialization/lifetime, truncation rule, and collision/refusal implementation |
| Authorization meaning | None | None |

Hashing does not remove the retained map: without full historical collision checks, a disconnected monitor's ID can later alias another native identity. A per-session key cannot promise stable IDs across Host restarts; persisting a key would add a new storage contract. The bounded allocation map has the same required identity evidence and less machinery. Recommend it for this bounded slice, subject to owner agreement. No identity choice may be treated as a permission or principal.

## Review and evidence required before owner application

The existing approvals are [the bounded S1 decision](https://github.com/gyldlab/keld/issues/481#issuecomment-6556306946), [its macOS selection/watchdog follow-up](https://github.com/gyldlab/keld/issues/481#issuecomment-6557264649), and [the GH446 Ready contract](https://github.com/gyldlab/keld/issues/446#issuecomment-6371650353). They do not approve Windows activation or complete global freshness.

Request the GH481/GH531 owner, acting under the recorded 0monish delegation, to review the exact Windows identity, units, appearance/unavailable policy, bounded projection, and portable S1 composition. Request the GH446/GH1188 owner to review the single Host selector, first-sample ordering, fixed deadlines, post-Init admission distinction, and application of this seam after the lead's approved baseline. Review must point to exact proposal/test bytes or the resulting commit. The nearest native-code instructions and independent review must also cover any new Windows unsafe API boundary.

Required falsifiers for integrated owner work include a valid authenticated Subscribe before any sample, unavailable with zero replay, ordered update/invalidation, latest-facts-plus-survivors recovery replay, retired role/document refusal, equal-epoch contradiction and regression, identical-correlation duplicate rejection, and stale late-native completion. Check both real window-only consumers. Check selected first-Init ordering and both deadline boundaries through the real coordinator, including a duplicate-desktop attempt to postpone expiry. Keep unselected Windows behavior as a negative control.

Native observations and synthetic identity/codec controls must be reported separately. Real global display/theme-change qualification remains a separate serialized native slot; do not change global settings just to produce evidence. No statement in this handoff authorizes W4 to modify the lead's shared files, merge main, or enable auto-merge.
The executable single-primary proposal also refuses a physical work area outside its monitor before identity allocation. This is a conservative qualification rule, not a universal MONITORINFO guarantee. GH481 must approve or amend it before production integration.
