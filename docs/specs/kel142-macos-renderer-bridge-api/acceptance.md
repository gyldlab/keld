# Spec: KEL-142: acceptance criteria

Part of: [index](index.md)
Owns: section 3, AC-1 to AC-8

## 3. Acceptance criteria

The list items are the approved text. Given and When restate each approved lead-in, and
are INFERRED where the approved text has none.

### AC-1: package ownership

- **Given:** the package slices of this spec have landed.
- **When:** the package gates run.
- **Then:**
  - `packages/@keld/api` exists and exports exactly the approved lifecycle/channel
    floor.
  - `@keld/electron` delegates lifecycle behavior to it instead of maintaining a
    second transport owner.
  - `tsc --strict`, no-public-`any`, Bun tests, and the canonical hostile KIPC
    corpus pass.
- **Negative control:**
  - **Single fault:** UNKNOWN: the approved text names none
  - **At-risk precondition:** UNKNOWN: the approved text names none
  - **Expected RED:** UNKNOWN: the approved text names none

### AC-2: trusted macOS injection

- **Given:** a real macOS WKWebView.
- **When:** a page loads.
- **Then:**
  - page code observes `window.keld.invoke` before its first script runs;
  - the facade is frozen/non-replaceable;
  - an iframe has no own `window.keld`;
  - popup/new-window escape is not created by this slice;
  - page world cannot access the isolated native handler;
  - raw endpoint/token/principal/generation/document nonce are absent from page
    globals and request arguments.
- **Negative control:**
  - **Single fault:** deleting content-world isolation or changing the trusted script to
    page-world only.
  - **At-risk precondition:** INFERRED: a page runs on that WKWebView.
  - **Expected RED:** must fail a named test.

### AC-3: one real renderer -> Bun roundtrip

- **Given:** using an OS-visible pointer event, not `element.click()`.
- **When:** the user and the document act:
  1. user activates the fixture button;
  2. same document calls `window.keld.invoke(1, encodedEchoRequest)`;
- **Then:**
  3. host observes exactly one renderer admission;
  4. host mints exactly one nonzero KIPC correlation;
  5. one admitted Bun `@keld/api.channels.handle` handler receives the decoded
     EchoRequest;
  6. one matching reply returns;
  7. the same document renders the typed EchoResponse.

  No stdout-grep or JS-only click stands in for any limb.
- **Negative control:**
  - **Single fault:** UNKNOWN: the approved text names none
  - **At-risk precondition:** UNKNOWN: the approved text names none
  - **Expected RED:** UNKNOWN: the approved text names none

### AC-4: host lifecycle

- **Given:** the same `@keld/api` session.
- **When:** the application uses the lifecycle surface.
- **Then:** the session proves:
  - `app.whenReady()` does not resolve before real host Ready;
  - `app.isReady()` reflects that state;
  - `app.onLastWindowClosed` subscribes/unsubscribes without duplicate callbacks;
  - `app.quit()` sends one correlated Quit and resolves only from host Reply;
  - `@keld/electron` conformance remains behaviorally unchanged.
- **Negative control:**
  - **Single fault:** UNKNOWN: the approved text names none
  - **At-risk precondition:** UNKNOWN: the approved text names none
  - **Expected RED:** UNKNOWN: the approved text names none

### AC-5: malformed/forged/stale input

- **Given:** a live renderer bridge.
- **When:** each of these inputs arrives:
  - malformed JSON/envelope;
  - unknown or extra field;
  - zero/out-of-range request/channel;
  - channel other than declared renderer Echo channel;
  - unsupported `opts`;
  - caller mutates the original `Uint8Array` after `invoke` (host still sees
    the snapshotted original bytes);
  - >4096-byte payload;
  - forged internal relay message cannot widen the facade contract;
  - wrong/forged document nonce;
  - old nonce after navigation begins;
  - subframe/native-handler attempt;
  - message after view destruction.
- **Then:** each independently proves zero application-handler effect.
- **Negative control:**
  - **Single fault:** UNKNOWN: the approved text names none
  - **At-risk precondition:** UNKNOWN: the approved text names none
  - **Expected RED:** UNKNOWN: the approved text names none

### AC-6: slow consumer and disconnect

- **Given:** a renderer invoke is admitted.
- **When:** the test acts:
  - Hold the one admitted Bun handler pending.
- **Then:**
  - A second renderer invoke fails immediately as busy; memory/queue count does not
    grow with repeated attempts.
  - Navigation or app-link disconnect settles the first renderer promise exactly
    once with a terminal error.
  - A late old reply cannot settle it again or reach the new document.
  - A fresh ordinary call succeeds on a fresh non-recovery session afterward.
- **Negative control:**
  - **Single fault:** UNKNOWN: the approved text names none
  - **At-risk precondition:** UNKNOWN: the approved text names none
  - **Expected RED:** UNKNOWN: the approved text names none

### AC-7: fuzz boundary

- **Given:** the repository fuzz gate.
- **When:** that gate runs.
- **Then:**
  The renderer-envelope cargo-fuzz target runs under the repository fuzz gate;
  its retained corpus includes malformed UTF-8/JSON, duplicate/unknown fields,
  boundary integers, 4096/4097 payloads, and stale-shape samples.
- **Negative control:**
  - **Single fault:** UNKNOWN: the approved text names none
  - **At-risk precondition:** UNKNOWN: the approved text names none
  - **Expected RED:** UNKNOWN: the approved text names none

### AC-8: real-device evidence

- **Given:** a physical Mac run.
- **When:** the run ends.
- **Then:** record:
  - exact Keld SHA/tree;
  - exact macOS build, architecture, and WKWebView/WebKit facts available to the
    fixture;
  - exact Bun version;
  - WebView/process identity;
  - document nonce only as a redacted/hashable observation, never raw in logs;
  - OS-visible input evidence;
  - one renderer admission, one KIPC corr, one Bun handler, one reply;
  - negative-control outcomes from AC-2/AC-5/AC-6.

  This macOS row says nothing about Windows/Linux parity.
- **Negative control:**
  - **Single fault:** UNKNOWN: the approved text names none
  - **At-risk precondition:** UNKNOWN: the approved text names none
  - **Expected RED:** UNKNOWN: the approved text names none
