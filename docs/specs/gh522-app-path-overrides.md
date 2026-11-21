# Spec: App path overrides without changing the webview profile

- Status: draft
- Tracker: GH-522
- Owner: 0monish (tracker)
- Updated: 2026-11-06
- Pin: 0eb3ce203187f35c5276a4237189c906a5220d5a

## 1. Goal & non-goals

Support Zettlr's synchronous main-process data-directory and log-directory setters with explicit, scoped semantics. A path value records a location. Each filesystem operation still uses the caller's existing authority. Profile relocation and cross-process settings persistence are outside this proposal.

## 2. Spec refs

- FACT: [Electron v44.4.5 app setters](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md#L680-L693), retrieved 2026-11-06: “If the path specifies a directory that does not exist, an `Error` is thrown.”
- FACT: [Electron setter source](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_app.cc#L960-L973) rejects relative paths and calls PathService with is_absolute=true and create=false.
- FACT: Electron [pins Chromium 152.0.7977.130](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/DEPS#L3-L6). Its [PathService](https://github.com/chromium/chromium/blob/2c592105bbcd9490a9894df48d0fe59b2c512651/base/path_service.cc#L276-L310) skips directory creation and absolutization for those arguments before updating the override.
- INFERRED: This source path lacks a missing-directory refusal. Keep the explicit #522 refusal. Measure the exact Electron binary separately before claiming behavioral parity.
- FACT: [Electron log API](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md#L604-L611) requires an absolute explicit path and defines per-platform defaults. [macOS source](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_app_mac.mm#L20-L27) uses bundle name.
- FACT: [Windows/Linux source](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/shell/browser/api/electron_api_app.cc#L886-L913) appends logs to current userData. [Zettlr v4.9.0](https://github.com/Zettlr/Zettlr/blob/04128af42b58f7fea3f39f7b1efd597f5021f48a/source/main.ts#L56-L73) calls both setters in order.
- FACT: `packages/@keld/electron/src/app-surface.test.ts:103` records getPath's absence on pinned main. #452 owns authenticated session facts and synchronous getters. `docs/specs/kel135-persistent-profile-identity.md:31` assigns persistent profile selection to the Host.

## 3. Acceptance criteria

### AC-1: Change the facade's userData value synchronously

- **Given:** The main-role facade has #452's immutable baseline. Two distinct accessible absolute directories exist.
- **When:** Call setPath('userData', A), read immediately, then call it again with B.
- **Then:** The next synchronous getPath returns A then B. Failed setters preserve the last committed value. Neither getter supplies access, persists configuration or moves previous bytes.
- **Negative control:**
  - **Single fault:** Always read the spawn-time default.
  - **At-risk precondition:** The test chooses a directory distinct from the baseline and both calls return normally.
  - **Expected RED:** The immediate readback disagrees with the chosen absolute directory.

### AC-2: Refuse absent and invalid values without mutation

- **Given:** Set a valid override first. Observe its directory and a separately controlled nonexistent path.
- **When:** Call setPath with a missing directory, regular file, relative path, unknown name and a denied probe.
- **Then:** Return an actionable synchronous error. Preserve the prior value and leave the nonexistent directory absent. Failed inspection cannot become success. Record explicit compatibility differences against the pinned release.
- **Negative control:**
  - **Single fault:** Skip existence checking on an absolute path.
  - **At-risk precondition:** A valid test principal may inspect the parent and independent filesystem enumeration confirms absence.
  - **Expected RED:** The override changes or the setter returns normally for the missing target.

### AC-3: Keep sessionData a documented divergence

- **Given:** Capture the Host-selected profile identity and put a marker in its real engine store. Choose a distinct existing directory.
- **When:** Call setPath('sessionData', dir) before Ready and repeat after Ready. Create another window in the same profile.
- **Then:** For each valid call emit one scoped diagnostic. Keep the facade value and effective Host store unchanged. Both windows observe the original store marker. Attribute late calls to the Ready state.
- **Negative control:**
  - **Single fault:** Pass the requested directory into the engine's profile-selection input.
  - **At-risk precondition:** Create the second window after the call and observe the actual profile binding. Do not rely only on a visible getPath string.
  - **Expected RED:** The recorded store identity or engine persistence observable changes.

### AC-4: Place logs under the correct default or explicit directory

- **Given:** Use isolated platform fixtures with a known bundle name, userData override and an authorized log root.
- **When:** Call setAppLogsPath() with no argument and then with an explicit absolute log path. Repeat with a relative path and a denied creation.
- **Then:** Return platform-correct logs paths immediately. Create permitted missing log directories. Invalid or denied calls fail without changing the previous logs value. The method cannot widen the creating principal's authority.
- **Negative control:**
  - **Single fault:** Use the original userData default on Windows or Linux after overriding it.
  - **At-risk precondition:** Call the no-argument log setter only after the successful userData override.
  - **Expected RED:** The observed log directory is outside the current userData on this positive fixture.

### AC-5: Keep path facts distinct from permission and engine identity

- **Given:** Prepare a directory whose existence the principal can inspect but whose file contents the principal cannot read.
- **When:** Set an eligible path value and then attempt a real brokered read. Inject a replacement alias between validation and any later file operation.
- **Then:** The path getter may succeed. The denied operation still produces no bytes or new grant. The broker applies its own containment checks at use time.
- **Negative control:**
  - **Single fault:** Extend the manifest or broker authority from the setter.
  - **At-risk precondition:** The independent policy oracle forbids the targeted read before calling the setter.
  - **Expected RED:** The policy changes or the protected bytes become readable.

## 4. Design

### D-1: Layer explicit role-local overrides on the authenticated facts

#452 supplies the initial path facts and Ready observation. Give the existing main-role facade one bounded override record per supported name. First scope: userData, logs and the sessionData divergence. Additional names require the owning #452 getter, a consumer and their own validation rules. Unknown or out-of-scope names raise an actionable error without changing state.

Read and update the same facade state synchronously. Never publish a partial update before the final inspection or authorized creation succeeds. A new Bun generation starts from its own admitted facts. Do not place path values in environment variables, manifests or engine profile keys. Keep the KEL-135 identity immutable across role restarts and window creation.

### D-2: Apply validation at the correct authority

For ordinary directory overrides require the native absolute-path grammar, an existing directory and authorized metadata access. Do not stat arbitrary names through a privileged Host helper. For a denied probe, report an access failure without disclosing existence or implying the caller can create the directory.

Separate location selection from authorization for subsequent reads and writes. Preserve symlink, reparse and containment policy at the owning filesystem boundary. A positive directory check does not bind future file operations safely. Use the existing principal's approved filesystem mechanism for any creation. Do not silently enable broader grants in Legacy or Strict.

Synchrony is part of Electron compatibility. A Promise changes a same-turn Zettlr caller. If the profile lacks an admitted synchronous metadata or log-creation path, leave that operation unsupported for the profile. B-1 requires proof before enabling it. Do not add synchronous Host IPC or a nested event loop without a separately approved design.

### D-3: Keep native profile state and log behavior explicit

For valid sessionData calls preserve the old facade path and actual engine store. Emit one divergence diagnostic per invocation with the method, lifecycle stage and supported alternative. Before Ready, explain the managed Host profile. After Ready, also identify the expired upstream override window. Refuse malformed input before this no-op.

On macOS, derive the default from authenticated app/bundle identity and the native user log directory. Electron uses the bundle name for this path. On Windows and Linux, derive the default from the effective userData at the time of setAppLogsPath(). Capture an explicit absolute path only after authorized creation or validation.

Keep explicit log paths stable across a later userData change. Another no-argument log setter recomputes the current platform default. Distinguish an authorized directory creation from a mere string change. A partial directory or write error cannot be reported as success. Do not globally replace app.getPath defaults for unrelated names.

FACT: [Electron getPath logs behavior](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md#L654-L659) describes lazy default log creation. Coordinate that getter with #452 so it uses the same authorized default selection and creation path. Add a first-get test with no prior setter. A denied creation must preserve the old state.

## 5. Boundaries

The existing Electron app facade owns synchronous overrides. #452 owns authenticated path facts and Ready. KEL-135 owns the persistent profile and physical store. Existing filesystem owners control capability checks. This specification adds no wire message, permission key, native profile setter or override persistence format.

## 6. Tasks

### T-1: Commit fixed-pin conformance evidence

- **Blocked by:** none
- Record Electron's actual getters, errors, log creation and pre/post-Ready behavior. Mark #522's documented sessionData divergence and missing-directory requirement explicitly.

### T-2: Add ordinary path overrides behind qualified facts

- **Blocked by:** T-1, B-1
- Implement AC-1, AC-2 and AC-5 using the authorized synchronous path. Validate first and publish the bounded override only after success.

### T-3: Add profile invariance and log behavior

- **Blocked by:** T-2
- Run AC-3 and AC-4 with independent physical effects. Exercise Zettlr's two-call sequence. Publish only the verified platform/profile cells.

## 7. Test plan & evidence

Run each mode on distinct temp roots. Compare exact values in the same JS turn, filesystem census and engine identity. Test fresh startup, next role generation, before/after Ready, no-arg and explicit log paths, missing parent, existing regular file, permission denial and a replaced symlink.

On macOS identified stores use the actual store identity and persisted marker, never an invented physical path.

### E-1: Pinned source audit

- **Run:** Inspect the pinned Electron API, setter implementation, Chromium PathService and Zettlr caller. Compare #452 facts and KEL-135 profile ownership.
- **n:** One Electron release and its Chromium version, one Zettlr release, one KELD main revision.
- **Environment:** GitHub connector, 2026-11-06. Source inspection only.
- **Limits:** No Electron binary or KELD native behavior ran. Record both upstream measured behavior and the KELD requirement on each tested OS.

## 8. Review gates triggered

Public API and permission model. No new unsafe, dependency or wire change in this document. An enabled implementation must independently review actual path inspection and creation authority.

## 9. Perf impact

No measurement. Load facts through their existing source. Keep constant bounded per-role storage and benchmark synchronous I/O only if an implementation changes the critical path.

## 10. Open questions, blockers & unknowns

### B-1: Qualify trusted session facts and synchronous path access

- **Status:** open
- **Owner:** The #452 session-facts owner and the supported profile's filesystem owner.
- **Impact:** A backend cannot provide true synchronous path checks or defaults until its facts and authorized operations are available.
- **Check:** At the real Zettlr pre-ready call site show the original app identity, synchronous positive and negative directory checks, and an independent denied read.
