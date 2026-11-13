# Spec: Dropped-file identity for webUtils.getPathForFile

- Status: draft
- Tracker: GH-550
- Owner: 0monish (tracker)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Expose the real path synchronously to an authorized preload for the exact engine File backed by a user drop. Preserve the page application's choice of wrapper. File pickers, folder traversal, remote promises, clipboard and granting filesystem operations are outside this first slice.

## 2. Spec refs

- FACT: [Electron v44.4.5 renderer API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/web-utils.md#L17-L23), retrieved 2026-11-06: “The file system path that this `File` object points to.”
- FACT: [Upstream binding](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/renderer/api/electron_api_web_utils.cc#L14-L22) converts the supplied value into a Blink WebBlob and queries its backing path. An unconvertible value throws.
- FACT: [Electron's tests](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/spec/api-web-utils-spec.ts#L12-L58) expect empty for bare Blob and cover a file input with isolation disabled. Neither tests OS drag into an isolated preload.
- FACT: Zettlr v4.9.0's [page caller](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/win-main/index.ts#L52-L68) passes each drop File. Its [preload wrapper](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/common/modules/preload/index.ts#L94-L104) turns an empty path or exception into undefined.
- FACT: `docs/architecture/05-webview-and-native.md:191` binds runtime routing to Host WebView/navigation identity. #467 owns bridge isolation. #549 owns synchronous return semantics. #1354 owns Zettlr product evidence.
- FACT: WebKit's [content world](https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/Cocoa/WKContentWorld.h#L32-L59)
  shares DOM access between namespaces.
  Its [script message](https://github.com/WebKit/WebKit/blob/febeda30215e0030c6279cc8a6fdbddd81ec8d31/Source/WebKit/UIProcess/API/Cocoa/WKScriptMessage.h#L42-L49)
  describes Foundation value types.
  These headers do not identify the backing path of a DOM File.

## 3. Acceptance criteria

### AC-1: Keep the synchronous API and distinct invalid cases

- **Given:** Run the pinned Electron method and the actual Keld preload method in separate arms. Use the disabled shipping adapter for Keld.
- **When:** Call each raw method before the application wrapper. Supply a script-created File, bare Blob, ordinary object, null and a genuine dropped file.
- **Then:** Both raw methods return empty strings for the script-created File and bare Blob. Ordinary objects and null throw synchronous TypeError. The genuine drop returns its path synchronously. Record each arm separately. Check Zettlr's wrapper conversion afterward.
- **Negative control:**
  - **Single fault:** Return an empty string instead of throwing for an ordinary non-Blob object in the Keld method.
  - **At-risk precondition:** The actual Keld method is installed. Observe its raw synchronous result before Zettlr catches errors or converts empty strings.
  - **Expected RED:** The raw Keld arm lacks the required TypeError even if the Electron and wrapped application arms remain green.

### AC-2: Bind the chosen object to its own physical file

- **Given:** Drop two same-name files from different directories. Give both matching contents, size, type and timestamps.
- **When:** Call the synchronous wrapper for the second then the first, then repeat both. Interleave a same-metadata script-created File.
- **Then:** Each real object yields its own full path. The constructed File yields empty. The wrapper maps that result to undefined.
- **Negative control:**
  - **Single fault:** Select a path by filename.
  - **At-risk precondition:** Both real files and the script-created File share their basename.
  - **Expected RED:** At least one actual path or the synthetic-file result differs.

### AC-3: Deny fabricated capture and stale authority

- **Given:** Hold a genuine drop in window A. Window B is also live. Each document has a Host-minted identity.
- **When:** Replay path strings, stale records and a synthetic DataTransfer. Navigate or destroy A before delivering its delayed callback.
- **Then:** No forged object or callback produces a path in B or the replacement document. Prevent script from injecting lookup records or selecting another view.
- **Negative control:**
  - **Single fault:** Skip generation validation on late capture.
  - **At-risk precondition:** Reuse the WebView and force the newly active document to receive the old callback with a deterministic barrier.
  - **Expected RED:** The protected callback accepts a retired generation. Count accepted records, even if a later lookup rejects them.

### AC-4: Keep identity across the real bridge

- **Given:** The original Zettlr page calls the exposed preload function with the actual drop object.
- **When:** Run once with an honest file and once with an object containing page-authored name, path or token fields.
- **Then:** The genuine argument yields its path before the call returns. The fake cannot cause path disclosure through properties or getters. Preserve wrapper error handling.
- **Negative control:**
  - **Single fault:** Replace the native object with a JSON dictionary in transit.
  - **At-risk precondition:** Use a verified native drop; check returned type and value in the same page turn.
  - **Expected RED:** The genuine path disappears or the test receives a Promise or forged string.

### AC-5: Keep disclosure separate from file access

- **Given:** The path lookup succeeds for a genuine file outside the test principal's filesystem grants.
- **When:** Request an actual read through the governed broker under the same unchanged policy.
- **Then:** Path knowledge adds zero permission. The broker denies and the independent read marker stays absent.
- **Negative control:**
  - **Single fault:** Seed a read grant during drop capture.
  - **At-risk precondition:** Select a real file impossible to read through any other granted root; inspect the Guard input and result.
  - **Expected RED:** The policy changes or the protected read succeeds.

## 4. Design

### D-1: Keep the engine as the identity owner

Native drop handling supplies trusted OS paths. The renderer engine owns the File backing identity. A platform adapter must prove that the specific engine object passed into the preload resolves to the same native item. Capture and lookup are independent obligations.

A native path list paired by index, filename, size, timestamp or content hash cannot establish that relationship. Neither event.isTrusted nor a page-visible bearer token establishes it. Page code may reorder or replace files before calling. Capturing a genuine event cannot authenticate an arbitrary later argument.

### D-2: Preserve isolation and timing

The declared preload exports the app's narrow function. The framework does not inject raw paths, a path list, a filesystem handle or native messaging into the page. A page-derived path property, prototype or string is never input to resolution.

Use an engine-private backing identity with trusted native linkage if a supported public mechanism exists. World-local WeakMap keys are insufficient unless the adapter proves the page wrapper and isolated preload argument refer to the same backing object. An asynchronous round trip cannot implement the synchronous signature.

Bind any retained record to the Host-minted view, document generation, origin/frame policy and genuine native drop. Duplicate observations of one physical object must agree. Bound retention before enabling a backend. Proposed first slice allows 256 items per drop and 1 MiB of captured path data per document.

Refuse an over-budget capture as a unit. Never accidentally associate the first 256 candidates after truncation. Clear records on document retirement, destruction and crash. For long-lived documents use weak lifetime tracking, or refuse further captures without guessing. A new drop must not map an older still-live object to a newer path.

### D-3: Admit support per engine

A macOS WKWebView positive requires a public, supported path from actual drag delivery to the exact DOM backing identity in an isolated preload. A native pasteboard list plus serialized script messages is insufficient. Do not add private API, expose caller-provided OS paths or block the UI thread awaiting another process.

UNKNOWN: No qualified native binding was established in this source audit. Keep the member explicitly unsupported on an unqualified backend with a compatibility diagnostic. Never return empty for every genuine drop and mark the member supported. Derive Windows/Linux eligibility from separate engine tests.

## 5. Boundaries

The renderer compatibility owner, #467 and #549 own facade calls. The native adapter owns native drop provenance. The Host owns document identity and Guard policy. No new public IPC message, path token, default grant or Zettlr source edit is authorized by this draft.

## 6. Tasks

### T-1: Prepare the conformance rows and failing fixtures

- **Blocked by:** none

- Obtain separate fixture scope before test changes or device runs. Record raw pinned Electron outcomes and unmodified Zettlr wrapper outcomes separately.
- Register AC-1 through AC-5 with independent expectations. Keep unimplemented Keld outcomes explicitly expected-fail. Fixture registration cannot qualify the actual Keld method.

### T-2: Qualify the native mechanism without product wiring

- **Blocked by:** T-1

- Obtain a separate test-only native safety approval and macOS operator claim before running the mechanism fixture.
- Use actual OS drops and isolated worlds to prove the proposed public backing-object primitive. Exercise duplicate basenames, constructed objects, synchronous return, revocation and capacity refusal.
- This isolated fixture produces B-1 evidence. It requires neither the shipping GH550 adapter nor the installed Zettlr path. If the primitive cannot qualify, stop here.

### T-3: Integrate the approved adapter with activation disabled

- **Blocked by:** T-2, B-1, B-2

- Wire the approved mechanism through the actual restricted preload and shared bridge in an isolated acceptance build. Keep production activation disabled.
- Run AC-1 through AC-5 and their individual controls on this build, including the raw Keld method and actual Zettlr wrapper. Publish the integrated evidence for B-3.

### T-4: Qualify the installed consumer before activation

- **Blocked by:** T-3, B-3

- Pin the installed macOS artifact, editor fixture, native engine and operator. Exercise the real page drop caller and unmodified preload while production activation remains disabled.
- Give the product owner the observed genuine and forged-file results, application effect, and failed attempts. A visible method name or a native-only receipt cannot qualify the app.
- Activate only after the integrated and installed evidence passes, the owners approve the exact implementation, and the shipping configuration preserves those outcomes. Otherwise keep support unavailable.

## 7. Test plan & evidence

Use an actual OS drop on two files with equal metadata and distinct absolute paths. Reverse lookup order. Reuse saved objects after another drop. Pair positive and negative probes in the same window and cross-window tests. Exercise 0, 256 and 257 items, the byte limit and limit plus one. No sleeps or path disclosure in production logs.

### E-1: Pinned source audit

- **Run:** Read the linked Electron docs, native implementation and tests, Zettlr v4.9.0 caller and wrapper, WebKit headers and pinned KELD architecture.
- **n:** One pinned Electron release, one Zettlr tag, one WebKit source revision and one KELD main revision.
- **Environment:** GitHub connector, 2026-11-06. Source inspection only.
- **Limits:** No OS drop, isolated-world native mapping, framework implementation or Zettlr acceptance ran.

## 8. Review gates triggered

Public API and permission model. Platform unsafe code or a new dependency requires separate implementation review. No new unsafe, dependency or wire change in this document.

## 9. Perf impact

No measured result. The proposed lookup must complete before returning to page code. Table memory has explicit bounds. Record end-to-end same-turn behavior on the qualifying engine.

## 10. Open questions, blockers & unknowns

### B-1: Qualify one native object association

- **Status:** open
- **Owner:** Platform adapter and isolated-preload owners under #550/#467/#549.
- **Impact:** Blocks T-3 and positive mechanism claims. It does not block the separately approved T-2 experiment.
- **Check:** T-2 proves the native primitive with real drops, same-name collisions, constructed File refusal, same-turn isolated lookup and revocation. No GH550 production wiring or installed Zettlr result is needed to close this prerequisite.

### B-2: Approve the exact contract and integration prerequisites

- **Status:** open
- **Owner:** Monish Khandelwal with the platform, preload, security and compatibility owners.
- **Impact:** Blocks T-3, production adapter implementation, and new API or permission exposure.
- **Check:** After B-1, approve the exact spec text, supported primitive, existing bridge interfaces, disclosure boundary, retention limits and native safety scope. Pin independent shared-owner prerequisites. This approval does not require T-3 integration or T-4 app evidence.

### B-3: Verify the actual disabled adapter

- **Status:** open
- **Owner:** GH550 implementation owner with the platform, security and compatibility reviewers.
- **Impact:** Blocks T-4 and any production activation. It does not block the disabled T-3 integration that produces this evidence.
- **Check:** T-3 passes all five criteria and controls on its exact implementation head. Include raw Keld invalid-input outcomes, synchronous isolated delivery, independent filesystem denial and retained-resource boundaries. The installed-app result remains T-4's responsibility.
