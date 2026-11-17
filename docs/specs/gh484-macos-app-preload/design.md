# Design

Part of: [index](index.md)
Owns: section 4

## 4. Design

### 4.1 Atomic ownership and reuse

### World (W)

- **Owner / input ? output:** #484, per-view WKWebView config → third named world + app handler + main-frame script

- **Failure and falsifier:** Wrong-world app script can read trusted nonce/handler; swap app/trusted world to make native test RED.

### Ordering (O)

- **Owner / input ? output:** #484 document-start static wrapper + app preload registry + #467 bounded method/callback path → exactly four callable page methods/frozen process facts before first inline

- **Failure and falsifier:** Delay wrapper until microtask → first-inline RED; omit actual matching app-private registration → zero real effects despite visible page methods. Static timing is test-only feasible; Host-qualified method binding remains blocked.

### Runtime (R)

- **Owner / input ? output:** #484 exact Electron preload wrapper + #452/#570 process facts → app-only globals

- **Failure and falsifier:** Insert `fs` require, clone role env, hardcode versions (separate REDs).

### Seed (A)

- **Owner / input ? output:** Unmodified draw.io `additionalArguments` and existing `.configuration` → optional first-inline setting

- **Failure and falsifier:** Invalid/missing flag overwrites storage; existing explicit preference is replaced; malformed JSON aborts preload/page. Separate REDs.

### Source (S)

- **Owner / input ? output:** #484 retained stage root + externally authenticated digest → bounded immutable Host-owned preload image

- **Failure and falsifier:** Same-UID in-place writer, spoofed adjacent digest or stage/root substitution yields an unapproved executable byte; independently anchored digest and hostile-write oracle RED.

### Document (D)

- **Owner / input ? output:** #542 proposed single native view/generation owner → current-document admission after its own approval/implementation

- **Failure and falsifier:** Old document call reaches new Bun handler, or same-document hash change spuriously rotates.

### Bind (B)

- **Owner / input ? output:** #540's approved v2 contract + #542's proposed native owner + world identity → realm-scoped current-document admission, design pending

- **Failure and falsifier:** App handler accepts trusted nonce, page-forged credential or stale bind; test one mutation each before enabling v2.

### Grant (G)

- **Owner / input ? output:** #540/#465 canonical v2 decoder + #541/#552 enumerated `el:` permissions + #484 handler → bounded guard-approved app call

- **Failure and falsifier:** V1 numeric invoke is misread as named send, or missing exact grant produces host effect.

### Metric (M)

- **Owner / input ? output:** Existing keld-benches KEL-90 registry + #484 handler callback → diagnostic only

- **Failure and falsifier:** Change class to publishable or add threshold; registry gate must reject.

These atoms have independent oracles: `require('fs')` refusal does not prove grants; copied process values do not prove world isolation; descriptor identity does not prove atomic contents under concurrent in-place writes. The host's OS/WebView handles stay on the AppKit main thread; no Bun call, disk read or pending async handler may block that thread.

Reuse the already owned KEL-142 **per-view `WKUserContentController`**, current `BridgeState` view checks and v1 `decode_wv_link`, then consume the **single canonical v2 decoder extension** under #540/#465 for named send/event. Consume #542's checked generation/eligibility owner when landed, one app-link writer/reader, and #541/#552 native guard. Add a new
named app-world handler, **never** a second parser, nonce counter, grant registry, KIPC reader or privileged page relay. Until v2 and exact grants are implemented, named app messages **remain refused**; do not reinterpret a numeric v1 invoke as an `el:` channel. The current Wry scaffold does not expose a named-world selector through its ordinary user-script
helper; use its existing sanctioned `objc2` macOS configuration seam or fail closed.

### 4.2 Sandboxed preload wrapper and object shapes

Execute the admitted file's **function body** with ordered parameter names `require`, `process`, `exports`, `module`, `Buffer`, `global`, `setImmediate`, `clearImmediate`. `module`/`exports` are per-execution local objects; `global` aliases app-world `globalThis`, never page or trusted globals. Do not expose global Node `require`, Node filesystem/process
privileges, role tokens or `process._linkedBinding`.

| Allowed specifiers | Resolved object |
| --- | --- |
| `electron`, `electron/common`, `electron/renderer` | One restricted renderer Electron object |
| `events`, `node:events` | One bounded `events` shim |
| `timers`, `node:timers` | One bounded `timers` shim |
| `url`, `node:url` | One bounded `url` shim |

Every other specifier throws synchronously with `new Error("module not found: " + name)`; no fallback path or Node/relative resolution. The Electron object has precisely the seven names in AC-4. Calls to unsupported members throw a registered typed compatibility error; the #467 owner delivers usable `contextBridge` and `ipcRenderer` surfaces (including IPC
listener setup required by unmodified draw.io).

Preload `process` exposes only the needed first-proof subset: `type`, `sandboxed`, `contextIsolated`, `versions`, `argv`, `env`. `versions` is copied from canonical session facts; if facts are missing, fail preload initialization rather than synthesize. `argv` captures window `additionalArguments` at window creation. `env` must have zero own keys; never copy
`KELD_APP_LINK` or other Bun role environment into this realm. Draw.io deliberately exports only `process.type` and `.versions` to the **page** via its `contextBridge`.

The [pinned unmodified draw.io preload](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron-preload.js) seeds `.configuration.defaultAdaptiveColors` only when **both** a recognized `--initial-adaptive-colors=` argument exists and its parsed object's `defaultAdaptiveColors == null`. It does not override a
preference selected by
the user. The seed operation is wrapped in `try/catch`; malformed JSON must not cancel page navigation. The native acceptance fixture varies these inputs independently:

| Argument / existing storage | Expected first-inline result |
| --- | --- |
| Valid `auto`, `simple` or `none`; key absent, or parsed JSON object with absent/`null` value | Write the chosen mode once and preserve unrelated object fields. |
| No `--initial-adaptive-colors=` argument, or an invalid value (for example `bogus`) | Do not read/overwrite storage for seeding; preserve original value exactly. |
| Valid argument, but valid JSON with non-nullish existing mode (including `false` or empty string) | Preserve that explicit value; do not write again. |
| Valid argument and malformed `.configuration` JSON | Catch/log the seed error, leave stored bytes unchanged and allow page **navigation** to finish; this does not prove the later editor app can parse malformed preferences. |

An admitted JS top-level exception logs Electron's exact prefix and error, then permits page load to finish. A source/path/read error fails **window construction** before execution. Any prior localStorage effect of a preload that later throws stays visible; there is no transactional rollback.

### 4.3 Ordering and the feasibility hold

1. **Before constructing the WKWebView or navigating:** capture an authorized snapshot and capture window config; install trusted KEL-142 and app-world handlers plus scripts on the one dedicated `WKUserContentController`. Scripts are `WKUserScriptInjectionTime::AtDocumentStart` and `forMainFrameOnly=true`.
2. **At navigation eligibility boundaries:** consume #541/#542's proposed single native generation owner: a host-initiated load invalidates the previous document **before commit**, a cross-document commit rotates the generation, a same-document change does **not** rotate, and canceled renderer-initiated navigation preserves the eligible document. The
original trusted script requests a bind with a native receipt that can be asynchronous; first-inline eligibility is unproved. **Owner gate:** KEL-142 currently accepts one trusted nonce and rejects a repeated bind; #540/#542 must approve **two independently minted, non-reused per-world credentials under one document generation**, with explicit native
handler/controller/realm registration and revoke/rebind ownership, or reapprove a changed v2 envelope. **Initial** app/trusted bind is authenticated by native world/frame/qualified document and mints only its own credential once; later effect-bearing messages require both a live stored binding **and** supplied matching credential. The app/page never gets the
trusted nonce or mints generation; until an approved app-role bind exists, refuse app v2 dispatch.
3. **Install Option A page shape and run real preload in app world (candidate only):**
   a fixed `AtDocumentStart` **page-world** `WKUserScript` could predeclare
   exactly four callable names and copied/frozen canonical `process` facts
   **before the first inline script**, but the **page→app ingress is NOT yet
   authorized**: the F0 raw page-native handler conflicts with current AC-17.
   The GH484 owner has selected Q-1 A2 as the carrier direction; A1 is not
   selected. This direction does not amend AC-17. GH484/KEL-142 must formally
   approve the narrow A2 data-only untrusted-ingress amendment and qualify
   the native receiver before it can be installed. Until then, **no production
   page-world endpoint or effect-bearing bridge is allowed**. Independently
   execute the **unmodified** admitted preload in the separate app world,
   record its actual
   `contextBridge.exposeInMainWorld('electron', actualMethods)` registration
   and prove its applicable adaptive-colors `localStorage` mutation **also
   completed before first inline**, despite native script ordering. Wrapper
   visibility cannot confer app-function identity: calls to
   unregistered/renamed/stale functions must fail closed without native/Bun
   effect. A sentinel/late fill, fabricated result or
   `evaluateJavaScript` completion callback is not first-inline evidence.
   **Arbitrary dynamic contextBridge publication remains deferred.**
4. **Early calls:** a callable page API may exist while host document bind is pending. Consume #467's selected **zero-queue early refusal**: public void method returns undefined, native rejects before reservation/dispatch, emits a bounded private typed diagnostic and retains **no** work for a future document or Bun role. If required draw.io startup traffic
occurs before eligibility, application acceptance stays BLOCKED until earlier safe bind/order is proved or #540/#542 approve an amended policy. No silent queue or successor replay.
5. **After bind:** app-world requests pass view/world/frame/generation checks, then parser and per-window grants. Navigation and destroy invalidate old requests before any new document can dispatch. Recovery cannot reset a surviving renderer.

**BLOCKED production publication and app-method binding proof:** the
independent [Mac fixed-wrapper experiment](https://github.com/gyldlab/keld/issues/467#issuecomment-6388049733)
observed a predeclared page method at first inline in **3/3** runs and
absence when omitted in **2/2**, using the actual pinned preload under
**synthetic IPC**. **Its page-native ingress conflicts with AC-17**; it
cannot be adopted on this evidence. Earlier 16/16 registration-order and
6/6 nested callback fixtures used **page-forgeable shared DOM** and simulated
IPC. None proves the original method's current-document identity or final
Host grant. The [native two-realm fixture](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584)
supports realm feasibility but is not #542/#552.

The owner has selected **A2** in the [GH484 decision](https://github.com/gyldlab/keld/issues/484#issuecomment-6560086477)
and [correction handoff](https://github.com/gyldlab/keld/issues/484#issuecomment-6560285327);
A1 is not selected. This resolves the carrier-direction choice, not the
formal AC-17/architecture-05 amendment or native qualification. Before
production, GH484/KEL-142 must approve the narrowly scoped untrusted
data-only page handler; until then the existing AC-17 prohibition remains
in force. Then prove fixed document-start wrapper plus real admitted
app-world registration, no shared-DOM authority, correct bind and teardown;
GH-467 proves mutable copies and nested callbacks through genuine app methods.
Native/Host/Bun witnesses must show two real separately released granted
requests and zero **forbidden** unexposed/wrong-world/stale/source/denied-grant
effects. A direct page message to an already exposed method cannot be
privileged compared with the normal wrapper; eliminating only the allowlist
or final Host guard must yield RED. If unproven, **do not ship**. GH441's
separate-world choice remains CLOSED; no fabricated result, self-derived
source hash or bypass.

**Native F0 chronology and limits (not KEL-142 AC-2):** [direct app-world assignment](https://github.com/gyldlab/keld/issues/484#issuecomment-6382021064) was invisible to first inline; [the follow-up with the unmodified draw.io preload](https://github.com/gyldlab/keld/issues/484#issuecomment-6382401714) saw nondeterministic app/page script order under naive
registration, then **16/16** correct first-inline callable API shapes with an order-independent test-only handshake and separate missing-exposure/`AtDocumentEnd` RED controls. [The 6/6 local proxy fixture with nested callback RED](https://github.com/gyldlab/keld/issues/467#issuecomment-6382468009) proved the *actual preload's* top-level and nested callback
paths under simulated IPC. The common DOM/event carrier is forgeable by the page. A private app exposure table blocked an **unexposed** method in that fixture but did **not** authenticate exposed calls or prove original-document, Host/Bun, grant, teardown or release semantics. No production `contextBridge`, wire parser or principal is approved by these
results.

### 4.4 Preload source authority and retained snapshot

Draw.io's main role computes an **absolute** `webPreferences.preload`
path. That path is a **selector, never filesystem authority**. The Host
must retain the previously admitted package/stage root and an immutable
trusted package-image manifest whose exact *relative* entry for the
preload carries a SHA-256 expected-byte digest. This expected digest
must be authenticated **outside any same-UID-writable package tree**
(for example by a separately verified signed package artifact or a
digest embedded in an independently trusted Host build). Computing
both the file hash and expected hash from mutable package files, or
trusting an adjacent editable JSON manifest, is insufficient. **No
such Host/package trust-anchor integration has been qualified for this
preload; source admission remains BLOCKED until its owner proves it.**

At window construction, map the absolute source to **exactly one**
manifest entry by checking complete path components against the
captured trusted staged-root identity. Reject any lexical prefix alias,
unmapped path, `.`/`..`, empty/invalid component, path changing under
normalization, alternate stage root, page `file://` root or ambient CWD.
The unmodified draw.io path must map as-is; a rewritten preload path
does not qualify. Traverse starting at the retained root descriptor
using directory-relative, no-follow operations. **First-proof policy:
reject symlinks at every component, including apparently in-root
ones.** This deliberately narrows the earlier proposal; if pinned
staged draw.io requires a symlink, leave AC-7/AC-8 BLOCKED pending
a separately reviewed, manifest-bound, race-safe alias design.
Do not reopen the validated entry using a display path.

**Proposed Rust signatures (illustrative only):**

```rust
struct RetainedPackageRoot; // admitted root fd, stage identity, trust anchor
struct TrustedPreloadEntry; // exact relative path + independently trusted digest
struct WindowPreloadSnapshot; // immutable checked bytes + manifest entry identity
fn prepare_preload(
    root: &RetainedPackageRoot,
    expected: &TrustedPreloadEntry,
    app_absolute_preload: &Path,
    host_total_bytes: usize,
) -> Result<WindowPreloadSnapshot, PreloadSourceError>;
```

Open **one** regular-file descriptor after safe component admission;
check its kind and bounded size, then reserve Host/window capacity
before copying. Away from the UI thread, read at most cap+1 bytes from
that descriptor and require EOF within the cap. Compute SHA-256 over
the **entire owned byte image**, then compare it with the separately
authenticated manifest digest **before any JS preload executes**.
On mismatch, read error, path race, no trusted digest, or exceeded
capacity, reject the window construction with a distinct registered
source-admission reason. No partial execution, silent truncation,
retry via ambient path or fallback to mutable source bytes. The
successful checked image is retained as immutable Host-owned memory
for the window, while the source descriptor may close; navigation
executes that exact snapshot in a fresh app-world JS context.

**Proposed initial limits for approval:** 512 KiB per window, 8 MiB combined per host; reserve budget before source acceptance and release exactly once on view destruction. Oversized files and budget exhaustion are typed construction refusals. Each later qualified navigation uses the **same source snapshot** but gets new JS variables and listeners. Bun
recovery never rereads that source and never reloads an otherwise surviving page.

**Same-UID hostile writer contract:** an independent process may
rename/recreate path components, replace symlink aliases and write
**in place** through an already-open source descriptor while Host
copies. Retained fds, file ownership/permissions and pre/post
`fstat` do not establish coherent or authentic bytes. Only a
complete byte-for-byte match to an **independently trusted digest**
can become the executable snapshot; a mixed image fails that check
except for a cryptographic hash collision. A writer that restores
identical trusted bytes before the read does not change the executed
image. Metadata rechecks are optional diagnostics, never the
authenticity oracle. The trust anchor, verified manifest and actual
snapshot implementation remain **UNIMPLEMENTED / BLOCKED**. This
design claims neither atomic filesystem reads nor existing KELD
package-signature verification.

### 4.5 Native admission and lifecycle

The new `WKScriptMessage` handler verifies the exact per-view
controller, **actual source `WKContentWorld`**, main-frame status,
live WebView and **#542's implemented** checked document before
decoding or causing an effect. Proposed #542 custody is one checked
navigation generation with **two separately minted random 128-bit
credentials** held in distinct App and Trusted binding slots. Each
credential is delivered only to its matching isolated world.
**Initial bind** is admitted solely from the correct native controller,
main frame, content world and qualified document; it creates at most
one credential for that world. A later effect-bearing send/invoke
requires both an **existing live stored world binding** and a
present matching credential; missing plus missing is always a deny.
Neither app nor page sees the KEL-142 trusted nonce, and neither can
select a role by message field. Host full-document load/replacement,
destruction and exhaustion revoke both slots through the single
generation owner; same-document changes and cancelled renderer
provisional navigation do not rotate either. Native dispatch rejects
duplicate bind, role-operation mismatch, wrong controller/world/frame,
stale or swapped credentials, and unqualified resource documents.
The [33-receipt test-only native experiment](https://github.com/gyldlab/keld/issues/467#issuecomment-6383445584)
is a feasibility input, **not** this binding implementation or approval.
GH-540/#542 must freeze the precise v2 credential encoding and
single-owner lifetime in independently reviewed specs before use.

The app handler has its own world/name on the **same per-view
controller**. Current `keld.wv-link/v1` accepts only `bind` and
numeric-u16 `invoke`, so named Electron send/event must remain
disabled until #540's v2 schema, #465's **same** canonical decoder,
#543 outbound event delivery, #464 conformance, and #541/#552 exact
grants are implemented. Final forwarding includes the current native
document, captured Bun link identity, and the existing
`dispatch_privileged` allow closure. A page-selected URL, nonce,
role/window, DOM event or preload-provided manifest entry cannot
mint authority. Invalid or retired work uses #540's bounded
settlement; it cannot replay or target a successor. No second
parser, navigation counter, privileged page relay or transport.

An app preload has ordinary DOM APIs and can intentionally call public
`window.postMessage`; that act is not intrinsically a native
isolation breach. **Production `contextBridge` method identities,
callbacks, bind credentials, readiness receipts and app handler
traffic cannot use shared DOM events/descriptors as an authority or
transport.** The separate existing KEL-142 page facade relay keeps
its original owner but cannot authorize an Electron route. If the
runtime cannot securely predeclare and bind the **four** actual current
app functions before the first inline without a forgeable DOM carrier,
**do not enable Option A preload compatibility** until the named
GH467/GH484 security and platform owners approve a sound alternative.
General dynamic `contextBridge` parity remains outside this first proof.

#### 4.5.1 Selected A2 data-only page carrier

The owner selected A2 as the carrier direction in [GH-484
decision](https://github.com/gyldlab/keld/issues/484#issuecomment-6560086477) and [correction
handoff](https://github.com/gyldlab/keld/issues/484#issuecomment-6560285327). A1 direct cross-world callable publication is not
selected.

The page-world carrier transports untrusted data only. Its event source, origin, DOM token, payload or claimed method cannot
authenticate a sender, create a principal, mint a realm credential, register a privileged method or widen a grant. Treat every
carrier event as hostile input, including events produced by the real page.

The original app-world preload and native receiver are installed in the named `WKContentWorld` on the same per-view
`WKUserContentController`. The receiver accepts only the current main-frame committed document qualified by GH-542. It uses native
world, frame and document facts, not page event fields. It resolves methods only through the original app-world registry, checks
exact method/action allowlists, then enters the single GH-552 Host Guard. A page event cannot create or mutate that registry.

A2 does not use tokenless Bind to mint dual-world credentials or emit `bound.document`. The current-document stamp and
document-correlated Bind remain GH-540/GH-542 owner gates. The static page facade is visible before first inline, but it has no
authority. If the native receiver cannot be installed and tied to the committed document before first inline, A2 is a feasibility
NO-GO. Do not fall back to F0 or page-world authentication.

This is the selected design direction, not native proof. The first-inline hostile-page test, actual original-method invocation,
zero forged Bun effects and independent custody receipt remain mandatory.

### 4.6 One diagnostic owner

#484 records per-call **handler-local** cost from callback entry through validation/handoff, with a declared monotonic clock, case census (allowed/refused/malformed), statistic (count/median/p95) and source commit. The **single** metric ID lives in the existing `gyldlab/keld-benches` KEL-90 metric registry (candidate `macos.app_preload.handler_callback_ns`,
uniqueness/naming awaiting registry-owner check), with class `diagnostic`, no threshold and no publishable acceptance score. X05-T1 is parked and may not duplicate this ID. This excludes Bun latency, queueing, file I/O and renderer JS exposure; do not infer comparative performance from the sample.


### 4.7 Error allocation and typed refusal

**Error allocation is proposed, not approved.** KELD-WV-011 remains the KEL-142 native rejection code. KELD-WV-012 is already reserved by approved GH540 for **no admitted Bun generation with zero attempted write**. KELD-WV-013 is reserved for **outcome-unknown**. Neither code means a preload file-admission failure, and neither may be reused for that failure.
Registered KELD-COMPAT-001 through KELD-COMPAT-009 remain unchanged.

Before F1 implementation, the error-registry owner must allocate a **distinct, noncolliding preload source-admission code** and approve its structured reason set: unmapped, outside, file-kind, oversize, root-unavailable, I/O, capacity, symlink, trust-anchor-missing and digest-mismatch. The GH467 facade owner independently allocates its graph and
unsupported-member codes. This draft allocates none of those codes and does not alter existing registry entries.

**Negative control:** an untrusted manifest or changed/in-place-raced source must remain distinguishable from absent Bun generation and cannot execute even partial preload effects.
