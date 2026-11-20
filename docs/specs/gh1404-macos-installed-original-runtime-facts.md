# Spec: Installed original-application runtime facts

- Status: implementing
- Tracker: GH-1404
- Owner: Core installed session and existing launch-facts codec
- Updated: 2026-11-19
- Pin: d98675c1a3b2eabb19e3000705e178f84fd169b4

Execution record: [GH-1404](https://github.com/gyldlab/keld/issues/1404).
[Independent approval 6692318686](https://github.com/gyldlab/keld/issues/1404#issuecomment-6692318686)
approved the prior exact design at SHA-256
`921cd2e27a364f4bbc74213813850880e8ed9d2d869679df75e4102b7c968280`.
The bounded implementation-review corrections below distinguish authentication
from child-group acknowledgment, preserve the native preferred-language spelling,
and make directory cardinality and retained-lock failure states explicit. The
updated content requires its own recorded review digest; the earlier hash is not
approval of changed bytes. This status claims neither implementation completion
nor native acceptance.

## 1. Goal & non-goals

An original application may need a native user-data directory and locale before
its first import, then the existing cooperative single-instance verdict at Ready.
The qualified draw.io main calls `app.getPath('userData')`, `app.getLocale()` and
`app.requestSingleInstanceLock()`; fabricating these values, skipping the calls,
or labeling an installed app as a development stage is not acceptable.

The existing GH-452/GH-453 composition has safe Foundation directory/language
capture and a durable Host-owned kernel file-lock owner. Its developer-only
facts schemas 4–5 and private graph launch route are not installed provenance.
Installed schema 6 already has a closed different meaning. Preserve it unchanged.

For the **installed original-graph cell only**, this amendment supersedes the
[graph-source amendment](gh1404-macos-installed-graph-source.md) §4.4 sentence reusing
launch-facts schema 6 and `local-bundle-v1`. That cell uses launch-facts schema 7
and the twelve-argument `local-bundle-v2` role defined here. Its sealed **boot
schema 6** remains the installed graph selector; boot and launch-facts schema
numbers are separate contracts. Non-graph installed launches retain launch-facts
schema 6 and the eleven-argument `local-bundle-v1` role. Developer schemas and the
seven-argument role are not reinterpreted.

## 2. Spec refs

- GH-1404 owns the installed original-application integration.
- GH-452/GH-453 own native directory/language capture and the durable Host-owned
  kernel file-lock owner.
- The [installed graph-source amendment](gh1404-macos-installed-graph-source.md)
  owns the sealed graph and selected original-source descriptor.
- Installed launch-facts schema 6 has a closed existing meaning and remains unchanged.
- Apple's NSLocale preferred-language contract is documented at
  https://developer.apple.com/documentation/foundation/nslocale/preferredlanguages.
- Independent approval 6692318686 records the prior exact design at the SHA-256
  receipt in the header. It does not approve the implementation-review corrections
  below; those require their own recorded digest.

## 3. Acceptance criteria

### AC-1: authoring and sealed boot remain closed

- **Given:** The installed original-graph cell and the existing optional declarations
  `singleInstance:{id}` and `applicationNameForUserAgent`.
- **When:** The declarations are validated and sealed into boot schema 6.
- **Then:** The instance ID equals the canonical application ID; the user-agent
  suffix is bounded visible ASCII and is not an authority input. Boot schemas 1–4
  and their producer/parser behavior remain unchanged.
- **Negative control:**
  - **Single fault:** Supply a different ID, malformed suffix or undeclared field.
  - **At-risk precondition:** The installed graph boot is being prepared.
  - **Expected RED:** The closed authoring validator refuses it.

### AC-2: one durable Host resource owner controls directories and lock

- **Given:** Installed admission and immutable Guard preflight have succeeded.
- **When:** The original Host captures native paths and a declared single-instance lock.
- **Then:** It uses GH-453's existing Application Support directory and retained
  no-follow lock owner. The lock is acquired once for the Host session, before Bun,
  and retained through normal, recovery and fatal cleanup. Contention is `contended`;
  native query, unsafe directory or lock errors refuse. No lock descriptor is passed to Bun.
- **Negative control:**
  - **Single fault:** Remove the retained owner or treat contention as acquisition.
  - **At-risk precondition:** A second Host starts with the same declared identity.
  - **Expected RED:** Both Hosts must not report acquired; lease-retention control fails.

### AC-3: launch-facts schema 7 is closed and source-derived

- **Given:** Native directory/language observations and the verified selected source.
- **When:** The Host creates and the Guardian decodes installed launch-facts schema 7.
- **Then:** The record has exact name/version/app/resource roots, selected source,
  bounded native paths, unchanged first native locale or omission, and the declared
  `acquired|contended` verdict. Missing facts stay omitted or typed unavailable.
  No environment, working directory, package source, `Intl` result or app-supplied
  path substitutes for a native fact.
- **Negative control:**
  - **Single fault:** Add duplicate/unknown fields, present null, multiple directories,
    malformed locale or an inconsistent declaration/verdict pair.
  - **At-risk precondition:** A schema-7 record is parsed before Bun startup.
  - **Expected RED:** The canonical Rust/TypeScript codec refuses the record.

### AC-4: the private v2 handoff authenticates before consuming facts

- **Given:** The existing liveness bootstrap and one-use Host registration.
- **When:** The authenticated per-generation command factory consumes the
  twelve-argument `local-bundle-v2` record.
- **Then:** Existing seven-argument developer and eleven-argument `local-bundle-v1`
  shapes remain unchanged. Guardian independently admits its signed bundle, graph,
  source and policy; durable fields must equal those facts. The actual kernel
  child-group identity is registered and acknowledged before Bun executes.
- **Negative control:**
  - **Single fault:** Parse/consume raw JSON before authentication or withhold
    `CONTROL_REGISTERED`.
  - **At-risk precondition:** A raw or partially authenticated invocation is attempted.
  - **Expected RED:** No command admission or Bun effect occurs.

### AC-5: facts grant no new native or renderer authority

- **Given:** Packaged state, user paths, resource descriptor, locale, lock verdict
  and user-agent text.
- **When:** Bun and the renderer use those facts.
- **Then:** Existing Guard permits, document-currentness checks, authenticated
  channels, preload budgets and selected resource root remain independently required.
  The current-process bundle factory remains the only installed provenance constructor.
  A same-user replaced and re-signed local app is not publisher-authenticated.
- **Negative control:**
  - **Single fault:** Treat metadata or a lock verdict as a filesystem/renderer grant.
  - **At-risk precondition:** A role attempts a native operation outside its existing grants.
  - **Expected RED:** The existing Guard/document checks refuse the operation.

## 4. Design

1. **Authoring and sealed boot.** Only the installed graph cell accepts the two
   existing optional application declarations: `singleInstance:{id}` and
   `applicationNameForUserAgent`. The instance identifier must equal the canonical
   application `id`; do not introduce another independently chosen identity.
   Reuse their existing literal validators. The user-agent suffix is bounded
   visible ASCII, not an authority input. Serialize the same optional values in
   closed boot 6. Boot 1–4 and their producer/parser behavior remain unchanged.

2. **Durable resource owner.** After installed admission and immutable Guard
   preflight, the original Host uses GH-453's existing native Application Support
   directory and retained no-follow lock owner. No lock is acquired without the
   explicit declaration. Acquire once for the complete Host session, before Bun;
   retain its non-clone owner through normal, recovery and fatal cleanup. A
   contended lock yields `contended`, never assumed success. A failed native query,
   unsafe directory or lock error refuses; never remove a potentially live lock.
   Bun recovery reuses the same verdict. No lock descriptor is passed to Bun.

3. **One closed data codec.** Add installed **launch-facts schema 7** alongside
   unchanged installed 6 and developer 1–5. It requires `kind:local-development`,
   exact name/version/app/resource roots and the verified selected original-source
   descriptor. Native directory facts are omitted for an empty native result;
   multiple directory results refuse startup. Locale is the unchanged first
   value in the native ordered preferred-language list, or omitted when that
   list is empty. A consumer still receives a typed unavailable error for an
   omitted fact. A declared `singleInstance` requires exactly the
   Host's `acquired|contended` verdict. No environment, working directory, package
   source, `Intl` result or app-supplied path substitutes for an absent native fact.
   The same canonical JavaScript codec is embedded into the standalone transport
   by the existing generator and shipped in the fixed pre-entry bootstrap.

4. **Versioned private handoff, not new admission.** Keep existing seven-argument
   developer and eleven-argument `local-bundle-v1` shapes unchanged. The original
   graph cell uses a distinct closed `local-bundle-v2` suffix: the same three
   Bun/boot/policy digest pins plus one bounded schema-7 JSON record. The Guardian
   must validate the Host-exclusive liveness bootstrap and authenticate the
   existing one-use Host registration before parsing/consuming it in the
   authenticated per-generation command factory. It independently admits its enclosing signed
   bundle, graph, selected source and policy; every durable field must equal those
   independent facts. The native directory/locale shape and declaration/verdict
   relation are revalidated. The single-instance verdict is data from that
   authenticated Host, not an authority granted by a raw argument. No new public
   KIPC channel, listener, trust service, developer lease or test credential exists.
   After command construction, the existing pre-exec gate must register the
   actual kernel child-group identity and receive the Host's matching
   `CONTROL_REGISTERED` retention acknowledgment before Bun can execute. That
   group acknowledgment is not a prerequisite for decoding the command data:
   the child group does not exist at that earlier point.

5. **Native effect boundary unchanged.** Packaged state, user paths, resource
   descriptor, locale, lock verdict and user-agent text grant no filesystem or
   renderer capability. Existing Guard permits, document-currentness checks,
   authenticated channels, preload budgets and selected resource root remain
   independently required. The current-process bundle factory is still the only
   installed provenance constructor. A same-user wholly replaced and re-signed
   local app is not represented as publisher authenticated.

### 4.1 Exact closed record and framing

The JSON record is a UTF-8 object, at most **16,384 bytes**. Every ordinary string
is nonempty, NUL-free and at most **4,096 UTF-8 bytes**; tighter digest,
identifier and authoring bounds below also apply. Duplicate keys, unknown fields,
arrays in place of objects, present `null`, proxy values and accessor fields are
refused before any getter/application code or resource effect. Optional means
**omitted**, never `null` or an inferred fallback. Rust and the canonical JavaScript
codec share these literal contract fixtures, not separate inferred shapes.

- **`schema`:** Exactly integer `7`; no coercion.
- **`kind`:** Exactly `"local-development"`; never relabel as development.
- **`name`:** Exact sealed `CFBundleName` and parsed boot name.
- **`version`:** Exact sealed `CFBundleShortVersionString`.
- **`root`:** Absolute verified `Contents/Resources/app`, equal to the Guardian's
  independent retained root.
- **`resourcesRoot`:** Absolute verified `Contents/Resources`; appending literal
  `app` equals `root`.
- **`paths`:** Required closed object with zero or more `appData`, `userData`
  and `documents` own string fields.
  - Each value is an absolute native path.
  - Each native directory query admits exactly one result, omits an empty result,
    and refuses multiple results.
  - `userData` may exist only with `appData`, and equals `appData/name` with
    `name` exactly one safe path component.
  - No `..`, environment expansion or app-provided directory.
- **`locale`:** Optional native preferred-language string: the exact first value
  from `NSLocale.preferredLanguages`, subject to the ordinary 4,096-byte,
  nonempty, NUL-free UTF-8 string bound.
  - Preserve spelling, case and separators; do not normalize or impose an ASCII-only rule.
  - An empty native list omits it. Multiple ordered preferences do not make the
    first choice ambiguous; a malformed first value fails rather than selecting a later value.
  - No `Intl`, environment, renderer negotiation or default locale.
- **`singleInstance`:** Optional closed object `{ "verdict": "acquired" | "contended" }`.
  - Required if and only if the verified boot contains `singleInstance`; its
    declaration ID already equals the verified canonical application ID.
  - Either verdict requires an available `paths.appData` native namespace.
    Available `appData` alone does not imply a declaration or verdict.
  - No ID, PID, path, lock descriptor or arbitrary field is accepted in this record.
- **`originalResourcePolicy`:** Required closed object with exactly the five
  string fields below. It describes the prepared source; it grants no permission
  and is not native load evidence.

`originalResourcePolicy` has exactly:

- `contract`: `"gh473-original-editor-offline/v1"`.
- `graphSha256`: `"sha256:"` followed by exactly 64 lowercase hexadecimal digits;
  equal to the verified boot-bound graph digest.
- `policySha256`: the same digest representation; equal to the exact opened and
  verified permission bytes.
- `windowKey`: the sole immutable Guard key selecting the verified original-source
  policy. The Guardian independently resolves this key and compares equality.
- `selector`: exactly `"original-editor-offline"`. Guard still owns its complete
  response policy and native application of that policy.

`applicationNameForUserAgent` is **not** copied into the facts record. It remains
the optional sealed boot declaration passed directly to the existing native view
configuration owner, avoiding a second authority or spelling. Its authoring value
is 1–256 visible ASCII bytes. `singleInstance.id` uses the existing canonical app-ID
grammar and must equal the sealed ID; omission keeps legacy behavior, not an
implicit successful lock.

For this new route the existing private Guardian invocation has exactly twelve
arguments total: existing executable/role/root/entry/device/inode/mode at indexes
0–6; `local-bundle-v2` at 7; lowercase 64-hex Bun/boot/policy digests at 8–10; the
one bounded raw JSON record at 11. The original-graph mode at index 6 is exactly
`2`. Existing local-v1 and developer role shapes continue to accept only their
previous `0|1` modes and lengths. Unsupported mode/suffix/length is refused. The
pre-auth parser may check raw argument count/length and digit/hex representation;
it must not parse or consume JSON/native observations before the validated
liveness bootstrap and authenticated one-use Host registration. The existing
authenticated `PREPARE`/`PREPARED` exchange precedes the command factory. The
separate `SPAWNED`/`CONTROL_REGISTERED` exchange retains the actual kernel child
group before the pre-exec permit releases Bun. Neither an argv shape nor a
successful JSON decode grants admission or replaces that execution gate.

The Guardian compares all sealed fields, unique Guard key, descriptor digests,
declaration/verdict relation and derived `userData` before command creation. Native
`appData`/`documents`/locale are authenticated Host observations, not independently
requeried Guardian observations: a native locale preference may change between
captures, so equality to a second query is not the contract. They remain frozen
for the Host session and never grant native access. A mismatch in durable fields,
unknown acquisition state, malformed native path or missing required declaration
verdict refuses before Bun, without synthesizing success.

### 4.2 Acquisition and failure states

Native directory/language selection is captured once after immutable preflight.
For each native directory query, zero results means an omitted optional fact,
one bounded canonical absolute path is available, and more than one result is
a typed startup failure. Never select the first directory from an ambiguous set.
Language preferences are ordered: retain the first native value unchanged when
present, omit an empty list, and refuse a malformed selected value. A native API
failure is a typed startup failure, not permission to consult another source. Native
`appData` must be available when the lock is declared. The lock has exactly four
internal outcomes: `not-declared` (no owner and omitted record), `acquired` (one
retained Host file-lock owner), `contended` (no acquired owner, observed kernel
contention), or `failed` (typed startup failure, no child). Only the middle two
appear as the public non-authority verdict. The Host retains an acquired owner
until all owned children, native work and session cleanup have retired, including
error paths; a logical session close is not a release receipt. The same verdict
is reused after Bun recovery. These resources are never cleaned by deleting the
installed application or an existing lock file.

When unresolved main-thread custody returns a fatal `HostRunFailure`, the
non-clone lock owner must move into the same `RetainedMacSession` payload as the
Guardian, renderer and native owners. It must not drop with `run_app`'s stack
while those owners remain retained. The shipping Host reports the failure by
borrow and keeps the payload alive through its fatal process exit. A test must
hold that failed Host alive before exit and observe a same-identity contender
remain contended; removing the retained lease must make that test fail.

This is a cooperative **Host-lifetime** lock. No lock descriptor is passed to
Guardian or Bun. Forced Host termination closes its descriptors even if Guardian
cleanup is still in progress, so this contract does not promise exclusion during
that post-Host-death interval. Native Host-death cleanup and a fresh acquisition
after observed retirement remain separate required checks. A stronger exclusion
guarantee would change resource custody and needs its own reviewed design; this
limitation does not waive the parent specification's owned-process cleanup gates.

### 4.3 Implementation-review evidence and changed atoms

Authentication and process custody are independent gates. The existing runtime
validates liveness and authenticates registration in
`macos_guardian/supervision_runner.rs::run_guarded_primary`; its
`guarded_lease.rs` command factory follows `PREPARED`, while
`spawn_admission.rs` releases the pre-exec child only after the exact group's
`CONTROL_REGISTERED` acknowledgment. Requiring that acknowledgment before
command decoding would make the existing startup sequence circular. Withholding
the acknowledgment must still prevent every Bun/application effect.

Apple's [NSLocale preferredLanguages contract](https://developer.apple.com/documentation/foundation/nslocale/preferredlanguages)
describes an ordered preference list. The first item is selected even with
secondary languages; preserving its native spelling is distinct from choosing
an application localization. Primary documentation retrieved 2026-11-06;
Context7 discovery used `/websites/developer_apple_foundation` with the narrow
ordered-preferences query. The directory zero/one/multiple-result policy and
bounded raw-string policy above are this contract's explicit admission choices.

The former normalized, 128-byte ASCII locale rule is replaced only for schema 7.
Core's native producer/authenticated decoder, the canonical TypeScript codec,
its generated standalone transport and their shared tests must change together.
Raw `en_US`, non-ASCII UTF-8 and a value between 129 and 4,096 bytes are accepted
as bounded data; an empty, NUL-containing or 4,097-byte value refuses. These
controls test representation, not the language values normally returned by macOS.
Unchanged schema-6 and developer fixtures remain required compatibility controls.

### 4.4 Alternatives and decision tests

Deriving directories or a successful lock from configuration/environment is
rejected: it has no OS evidence. Calling the developer facts factory for an
installed app is rejected: it erases provenance. Changing existing schema 6 or
silently extending its eleven-argument role is rejected: old closed readers
cannot distinguish the new contract. A new synchronous app IPC operation for the
lock is larger and changes the established GH-453 lifetime/dispatch contract.
The versioned private record reuses the already authenticated startup and existing
Host resource owner, with no new transport or supervisor.

## 5. Boundaries

- macOS installed original-graph cell only.
- Existing installed schema 6 and developer schemas 1–5 keep their meanings.
- No lock descriptor passes to Guardian or Bun.
- No new public KIPC channel, listener, trust service, developer lease or test credential.
- Native facts grant no filesystem or renderer authority.
- Windows/Linux packaging, lock semantics and native directory acceptance are not
  implemented or claimed by this addition.
- Host-death cleanup and fresh acquisition after observed retirement remain separate
  checks; forced Host death does not promise exclusion after the lock-owning Host dies.

## 6. Tasks

### T-1: implement the schema-7 producer and canonical codec

- **Blocked by:** none
- [ ] Implement the closed schema-7 record and canonical Rust/TypeScript fixtures.
- [ ] Preserve installed schema 6 and developer schema 1–5 compatibility controls.

### T-2: implement native directory and retained lock ownership

- **Blocked by:** T-1
- [ ] Capture zero/one/multiple native directory results and ordered locale facts.
- [ ] Retain the non-clone Host lock owner through normal, recovery and fatal cleanup.

### T-3: implement authenticated private handoff and execution gate

- **Blocked by:** T-2
- [ ] Add the graph-cell-only twelve-argument v2 route.
- [ ] Authenticate before parsing/consuming facts and require actual group
  registration acknowledgment before Bun execution.

### T-4: complete native acceptance and exact-source review

- **Blocked by:** T-3
- [ ] Run failure-first schema, lock, lifecycle and native installed-original controls.
- [ ] Obtain exact-source security, architecture and wire review.

## 7. Test plan & evidence

- Existing schema-6 fixtures and seven/eleven-argument role tests remain unchanged.
  Mixed records, unsupported versions, duplicates, accessors, unknown fields,
  wrong root/id/profile, missing verdict and over-bound records refuse before Bun.
- A raw twelve-argument private invocation with plausible facts cannot authenticate
  or create a child. The genuine authenticated route checks independent signed
  roots/digests before consuming any Host-carried observation.
- The raw record's 16,384-byte bound is enforced before authentication without
  JSON decoding. Forged registration cannot enter the command factory. After
  authenticated command construction, withholding or mismatching the actual
  group's `CONTROL_REGISTERED` acknowledgment prevents Bun execution.
- Two actual installed Hosts of one declared identity cannot both acquire the
  same lock. A different identity does not contend. Lock failure cannot return
  acquired. After the real owner retires, a fresh launch acquires without deleting
  the persistent lock file. Bun replacement keeps the same owner/verdict.
- A retained fatal session keeps its acquired lease while the Host remains alive;
  the negative control drops only that lease and must expose premature acquisition.
  Forced Host death separately proves owned cleanup and later reacquisition,
  without claiming exclusion after the lock-owning Host has died.
- App imports observe native, frozen facts before dependencies. Decoy environment
  values do not affect them. File paths and unavailable fields are checked against
  an independent Foundation observer; user-agent text is checked in the real view.
- Directory results exercise zero (omitted), one (exact value) and multiple
  (typed failure). Language lists exercise empty (omitted), one and multiple
  ordered preferences (unchanged first value), malformed-first refusal and the
  raw-string bounds in both Core and canonical TypeScript.
- Installed original draw.io still must pass its real Open/Edit/Save/unsaved
  Cancel/Discard/Quit workflow, file readback, permission rejections and owned
  cleanup. These data changes or passing codec tests cannot satisfy that gate.

## 8. Review gates triggered

- **Public API:** schema-7 data and graph-cell-only private v2 handoff require
  independent architecture review.
- **Permission model:** exact installed Legacy/Guard boundary requires security review.
- **Wire protocol:** no public KIPC channel changes; private Host/Guardian reader
  and stale-generation behavior require review.
- **Dependency:** none is added by this amendment.
- **Unsafe:** no new unsafe operation is proposed.

## 9. Perf impact

No performance improvement is claimed. Native directory capture and the retained
lock add bounded startup work. The lock is held for the Host session and no
descriptor is passed to Bun.

## 10. Open questions, blockers & unknowns

### B-1: native installed acceptance remains open

- **Status:** open
- **Owner:** GH-1404 native acceptance owner
- **Impact:** Codec and modeled lifecycle tests do not qualify the installed original app.
- **Check:** Complete real Open/Edit/Save/unsaved Cancel/Discard/Quit, file readback,
  permission rejection and owned cleanup on the installed original.

### B-2: independent review of corrected schema and lifetime contract

- **Status:** open
- **Owner:** Core installed-session owner and independent security reviewer
- **Impact:** The prior approval digest does not approve the changed review corrections.
- **Check:** Record the exact new review digest and verify the authenticated handoff,
  lock retention and child-group acknowledgment.

### B-3: Host-death cleanup and reacquisition remain separate

- **Status:** open
- **Owner:** runtime/Guardian owner
- **Impact:** Host-death cleanup does not promise exclusion after the lock-owning Host dies.
- **Check:** Prove owned-process cleanup and fresh acquisition after observed retirement.

Platform scope is macOS only. No main merge is authorized. Final source, wire/data,
lifetime and native review remain separate required evidence.
