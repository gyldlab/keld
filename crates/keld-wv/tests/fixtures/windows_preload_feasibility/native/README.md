# GH1184 Windows preload feasibility probe

This standalone test crate measures selected Win32/WebView2 behavior on Ramani.
It is not the KELD Host, bridge, parser, permission system, or production preload
loader. Route D and PR1307 remain subject to exact-content owner review.

## Reused inputs and ownership

The initial Win32/COM patterns and exact dependency versions came from the
preserved E5 source:

    docs/keld-research-264-preload-isolation/artifacts/runs/gh1184-route-d-host-rules-2026-10-29/route-d-probe

The original driver was not run. Its source and cache were not edited.

- Initial worktree base: df99602ef10abd2cd6f3b5c633ba6d1cffee97bc.
- Original E5 src/main.rs SHA-256:
  a295b6704a6f3e6e39760970007a5102eb57e59326a39455583309e6ee9b69df.
- Original E5 Cargo.lock SHA-256:
  acb56069352171e9eb68ab9c60ca23e9cef25580e4814188e4639d767930c300.
- Cargo.lock.e5-original preserves that exact lock. The current lock adds the
  existing repository-pinned sha2 0.10.9 dependency and its locked closure.
  Original E5 package entries are unchanged.
- Rust toolchain: repository rust-toolchain.toml, 1.99.1.
- webview2-com 0.38.2 and webview2-com-sys 0.38.2: MIT.
- windows 0.61.3, windows-core 0.61.2, serde_json 1.0.150, sha2 0.10.9:
  MIT OR Apache-2.0.
- No production manifest, integration entry point, or shared registry is changed.

Native source, manifest, lock, executable, fixture bundle, and fixture pages must
be hashed by the outer run receipt. The binary independently hashes the exact
UTF-8 bundle string that it will register. This is a caller-supplied experimental
pin. It is not an approved production trust root or a main/preload pair policy.

## Build evidence

The compact evidence-index.json binds the final executable to exact source and
lock hashes. Raw build logs, the original compile/format failures, historical
Cargo metadata, and byte-verified custody receipts are retained under the common
Git directory at keld-closeout/chatgpt-w1-feasibility-20261105/native-build.
They are not source files for the proposed PR. Native runs remain separately
gated by the shared Windows slot and do not follow from a successful build.

## Build and run

Build only this crate, with a private generated target directory:

    cargo build --locked --offline --jobs 1 --manifest-path native/Cargo.toml --target-dir native/target

Launch only after obtaining the shared Windows native/GUI slot:

    native/target/debug/keld-w1-native-probe.exe --config <absolute-config.json>

The output directory may contain the outer driver's page and bundle. It must not
already contain result.json, trace.jsonl, or profile. These are created
exclusively. The harness never clears a pre-existing profile.

The outer driver must quarantine a hung or incompletely cleaned case and stop
the sequential matrix. A timeout does not authorize killing another process,
imply cancellation, or count as browser release.

### Config

The JSON object accepts only:

| Field            | Meaning                                                      |
| ---------------- | ------------------------------------------------------------ |
| mode             | order, stale-delivery, or contentloading-race                |
| variant          | Values below                                                 |
| url              | Explicit first fixture URL                                   |
| successorUrl     | Explicit second URL for the two navigation modes             |
| bundlePath       | UTF-8 bundle file, required for order                        |
| bundleSha256     | 64 hex digits, required when bundlePath is supplied          |
| outDir           | Absolute fresh case directory                                |
| timeoutMs        | Each event wait; default 15000, maximum 60000                |
| cleanupTimeoutMs | Browser-exit observation limit; default 20000, maximum 60000 |
| visible          | Whether to show the one fixture window; default false        |
| busyMs           | 50, 200, or 1000; default 200                                |
| burst            | 1..4096; default 256                                         |
| observeMs        | Measured bounded exposure window; 1..5000, default 1500      |

url and successorUrl must be distinct strings. Query tags may distinguish
same-origin fixture documents. These strings are observational matching keys;
there is no URL membership parser or product navigation eligibility claim.

### order

Variants: document-created and dom-content-loaded.

Both register the independent observer and await its completion callback before
Navigate. The healthy variant also registers the exact pinned bundle and awaits
that registration callback. The single-fault variant executes the same pinned
bundle from the native DOMContentLoaded callback.

The supplied page records its first inline snapshot in the title:

    W1_FIRST_INLINE:<JSON snapshot>

It also posts a JSON string with kind first-inline and an observed object. The
bundle must post kind preload-complete when execution finishes. The early page
must produce both preload-sink-observation channels rendererReq and fixture-only.
The harness awaits these independent page observations, the DOM-ready marker,
and the native late ExecuteScript completion where applicable.

A late-order result is useful only when its first-inline API snapshot is absent
and its eventual preload-complete marker is present. The outer driver asserts
that distinction. A failed injection is not a valid ordering-only control.

Changing only bundleSha256 to an incorrect digest rejects before any COM
apartment, HWND, profile, WebView, script registration, or navigation is created.
The result is status source-rejected, reason bundle-sha256-mismatch, exit code 2,
nativeCreated false, registrations 0, navigations 0, messages [], and
cleanup.required false / cleanup.complete true. Browser-exit booleans remain
false because no browser was created.

### stale-delivery

Variants: guarded and unfenced. Both pages may be inert ordinary HTML.

The document-created observer emits:

    {kind:"probe-created", documentId, initialHref}
    {kind:"probe-dom-ready", documentId, initialHref, title}

Its captured raw chrome.webview message listener emits:

    {kind:"probe-raw-delivery", documentId, initialHref,
     value, stampedEpoch, stale}

The documentId is a page-generated observation tag. It is never a grant,
credential, product epoch, authority, or input to a production admission rule.
The observer is in the shared page world. It offers no isolation from page code.

After A is ready, the harness captures a synthetic held reply stamped with A's
tag and the fixture's current ContentLoading serial. It replaces A with B,
awaits B's ContentLoading and raw observer readiness, and attempts to release
the held reply. The guarded fixture refuses a changed serial. The unfenced
mutant sends the held value through native PostWebMessageAsString.

Both variants send a current-document marker and require raw page-visible
receipt. The unfenced mutant additionally requires old reply delivery. The
guarded variant measures its bounded exposure interval. Absence is reported only
for that interval. It is not a universal no-late-delivery guarantee.

The raw observer, not the refusal log, supplies the stale delivery oracle.
scenario.heldReplyAttempted is true in both variants. The scenario reports
rawStaleDeliveries and native posts separately.

This is a synthetic post-subset held-reply control. It does not exercise the
natural pending-navigation/pre-post race, delayed native Bun completion, the
approved product's grant/generation fence, or the full B7 Arm H.

### contentloading-race

Variants: busy, pagehide, unload, and no-force.

The host queues the forcing script and waits for its independent forcing-start
message. It does not wait for ExecuteScript completion before Navigate.
busy emits that marker, occupies the renderer for busyMs, then sends a bounded
tagged burst. pagehide/unload install a bounded burst on their named event.
no-force emits the burst before the start marker as a negative control.

After successor ContentLoading, nativeSeq identifies old tagged messages received
after that event. A zero cohort is RULE_NOT_EXERCISED. A nonempty cohort is
ARRIVAL_COHORT_OBSERVED; no full-rule or weaker-rule admission comparison is
implemented. The page tag is strictly an oracle label.

Native browser/runtime and process census observations are preserved. Different
URLs do not prove renderer replacement. Same-origin COOP forcing, cross-site
forcing, BFCache coverage, natural Arm H, maximum-rate Host posts, and stalled
navigation beyond a hold bound require their own explicit fixtures and runs.
No result from this subset closes those requirements.

## Result and cleanup

result.json contains finalTitle, runtimeVersion, messages, sourcePin, scenario,
specScope, browserExited, and cleanup. Each native message retains the browser
event Source separately from currentSource and any body claims, plus nativeSeq
and monotonic elapsedMs. Those are callback receipt ordering observations, not
renderer linearization timestamps. Browser Source does not identify a preload
function, a JavaScript world, or a same-world caller.

For a created native session, cleanup includes:

- controllerClosed: native Controller.Close returned success.
- browserExitEventObserved: matching normal BrowserProcessExited was received.
- browserHandleSignaled: the acquired browser process HANDLE was signaled.
- profileRemoved: the exclusively owned case profile was removed after exit.
- windowDestroyed: the owned HWND no longer existed after DestroyWindow.
- complete: all required observations, class release, environment callback removal,
  and owned process-handle releases succeeded.

The environment is retained through the exit event. Process HANDLEs are acquired
before cleanup, so a reused PID cannot masquerade as process exit. The census
retains handles to old renderer processes across replacement. It uses the native
GetProcessInfos API, which excludes crashpad; it is not a global machine process
census. The harness never terminates processes.

Failed setup cannot claim verified browser release. Its cleanup.complete is
false and nativeCreated is null because allocation may have partially happened.
Destructor fallback is not a cleanup PASS.

The exit code is 0 when the scenario's mechanical observations and required
cleanup completed, 1 for an unexpected error or incomplete cleanup, and 2 for
the expected digest refusal path. Code 0 does not mean production acceptance.
The outer driver must assert the actual first-inline, stale-delivery, negative
control, and source binding conditions.

## Design atoms and review boundaries

1. Pinned in-memory bytes and registration completion precede navigation.
2. First inline page title independently reveals early API availability.
3. Raw page receipt independently reveals Host-to-page value delivery.
4. Tagged old-page messages expose whether a B7 natural arrival rule was exercised.
5. Explicit resource ownership and observed browser exit precede profile removal.

No active mod.rs, app_session.rs, or Host main entry point is modified. Production
integration belongs to the shared owners after actual contract approval.

## Primary API evidence

Consulted on 2026-11-05 through current Microsoft documentation and Context7.
The runtime used for a case is reported by BrowserVersionString, independently
of the documentation version.

- ICoreWebView2 document-created registration, async delivery and navigation:
  https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2
- Browser-authenticated message Source:
  https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2webmessagereceivedeventargs
- Browser exit event:
  https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2environment5
- Profile-related process collection and crashpad exclusion:
  https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2environment8
- Explicit controller Close and callback release:
  https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller
- WebView2 security guidance:
  https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security

The checked public document-start API offers no WKContentWorld-equivalent world
selector. This fixture always reports contextIsolated false. Native macOS
isolation evidence is not Windows evidence.
