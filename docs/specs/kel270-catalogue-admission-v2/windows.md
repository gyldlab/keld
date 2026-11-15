# Windows admission and source custody

Part of: [index](index.md)
Owns: D-ADMISSION, actual-worker tokens, discovery, protected namespaces, source custody, one-shot admission, and native oracles.

## 2. Spec refs

FACT: The Host refuses before opening the request. Source: `crates/keld-host/src/windows_initial_install.rs:53` at the product pin.
FACT: Current per-user initialization checks only the existing non-SYSTEM token rule. Source: `crates/keld-update/src/windows_baseline/initialize.rs:224`.
FACT: Publication reopens the TAR locator before validation. Source: `crates/keld-update/src/windows_baseline/initialize/publication.rs:15`.
FACT: Source opens share only reads and refuse unsupported file kinds. Source: `crates/keld-update/src/windows_extraction.rs:668` and `:854`.
FACT: Guard owns the pinned-handle verifier and matching VERIFY/CLOSE sequence. Source: `crates/keld-guard/src/windows_authenticode.rs:204`. FACT: The token owner bounds query allocations to one MiB. Source: `crates/keld-guard/src/windows_machine/uac_token.rs:35` and `:320`.


## 3. Acceptance criteria

### AC-40: Actual-worker admission
- **Given:** A valid signed installation and a live Windows worker.
- **When:** The worker requests installation admission.
- **Then:** Only D-42's exact token profile can reach protected creation.
- **Negative control:**
  - **Single fault:** Remove the worker impersonation check.
  - **At-risk precondition:** The worker impersonates while all signatures and source inputs remain valid.
  - **Expected RED:** The OS-visible creation oracle observes the forbidden effect.

### AC-41: Managed preflight
- **Given:** Valid managed provenance at a required discovery root.
- **When:** A valid Direct candidate requests initial installation.
- **Then:** Admission refuses before every network request, namespace creation, and direct-writer acquisition.
- **Negative control:**
  - **Single fault:** Skip managed-owner precedence.
  - **At-risk precondition:** Current signatures, identities, and native protection remain valid.
  - **Expected RED:** The independent request counter, creation census, or writer-handle observer becomes nonzero.
The census starts at retained LocalAppData, above GYLDLAB, for clean-user and existing-tree fixtures.
Managed refusal creates no shared ancestor, acquisition object, or installation object.

### AC-42: Source object custody
- **Given:** A verified retained canonical TAR and a valid destination.
- **When:** Another process substitutes its diagnostic pathname.
- **Then:** Publication consumes only the original retained object or refuses before publication.
- **Negative control:**
  - **Single fault:** Reopen the pathname after verification.
  - **At-risk precondition:** Synchronize replacement after validation and before content copying.
  - **Expected RED:** The copied file digest or consumed file identity differs from the admitted object.

### AC-43: Established acquisition state
- **Given:** An established enrollment anchor and a current generation.
- **When:** The current generation becomes missing or malformed.
- **Then:** Normal refresh and installation refuse without reseeding from the embedded root.
- **Negative control:**
  - **Single fault:** Treat missing established generation as first use.
  - **At-risk precondition:** Preserve the valid enrollment anchor and independent installation reference.
  - **Expected RED:** The reset-generation or protected-write oracle observes unauthorized bootstrap.

### AC-44: First-effect authorization
- **Given:** Valid inputs and a blocked metadata lease acquisition.
- **When:** The generation changes or the 120-second admission deadline expires during the wait.
- **Then:** No installation-specific parent, scaffold, lock, or stage is created.
D-44 acquisition provisioning does not authorize those objects.
- **Negative control:**
  - **Single fault:** Omit the post-wait generation or deadline recheck in its separate control.
  - **At-risk precondition:** Keep every unrelated signature, path, and ownership check valid.
  - **Expected RED:** A native object census detects the first unauthorized creation.

### AC-45: Exact commit classification
- **Given:** A valid admitted baseline transaction.
- **When:** A child process terminates at each publication boundary, including after provenance publication.
- **Then:** Restart reports incomplete, committed, or recovery-required state from protected readback.
- **Negative control:**
  - **Single fault:** Return success before final protected readback.
  - **At-risk precondition:** Interrupt after a partial or uncertain publication.
  - **Expected RED:** A success receipt disagrees with independent protected-record inspection.

### AC-46: Offline preflight cannot retrieve network data
- **Given:** A valid signed fixture, controlled certificate endpoints, and a managed installation.
- **When:** The native adapter performs preflight and local ownership discovery.
- **Then:** Neither the process nor delegated Windows certificate retrieval emits a network request. No namespace object or writer authority exists.
Start the creation census at retained LocalAppData, above GYLDLAB, including clean-user fixtures.
- **Negative control:**
  - **Single fault:** Remove `WTD_CACHE_ONLY_URL_RETRIEVAL` from preflight.
  - **At-risk precondition:** The signed fixture requires a locally absent intermediate available through its controlled AIA endpoint.
  - **Expected RED:** Independent endpoint or OS telemetry observes retrieval. Restored preflight refuses offline without retrieval.

## 4. Design

### D-40: Scope and authority

The first supported new admission cell is fresh v2, PerUserDirect, on fixed local NTFS. Machine deployment remains with its existing authority and entry points. Existing direct installations use the existing update or recovery operation.
Managed installations refuse this direct installer before feed access.

The adapter consumes D-WIRE identity values and D-METADATA receipts. It does not define another publisher identity, metadata verifier, package selector, or installation writer. The existing KEL-53 transaction remains the only installation writer.

The supported threat boundary excludes a malicious Windows administrator, kernel, or installer-account process with unrestricted owner access. Owner-private ACLs do not stop the owner from rolling back all state.
Same-user rollback of both the enrollment anchor and repository records remains outside the claimed antirollback guarantee. LPAC or other strict app principals must remain unable to modify these objects.

### D-41: Atomic boundaries

| Atom | Owner and input | Output | Failure observable |
|---|---|---|---|
| W1 caller | Guard, actual filesystem worker | Retained query-only token context | A prohibited token reaches the first protected effect |
| W2 identity | Guard and Pack, retained signed image | Verified D-WIRE intent | Foreign image or policy supplies identity |
| W3 discovery | Deployment, signed discovery digest and protected roots | Closed owner disposition | Managed or uncertain state reaches the feed |
| W4 acquisition | KEL-53, repository identity and protected enrollment | Current immutable generation | Missing established state becomes fresh |
| W5 custody | Update, retained source objects | Bound archive handle | Different object supplies publication bytes |
| W6 admission | Update, W1-W5 and live leases | Consumed one-shot permit | Stale generation authorizes installation creation |
| W7 commit | Existing KEL-53 transaction | Protected reread receipt | Partial state is reported as committed |

W1 binds every descriptor and W6's worker. W2 binds W3 and W4's repository. W4's generation binds W6. W5's archive handle remains live throughout W7. W7 does not fetch metadata or substitute new sources.

### D-42: Actual-worker token contract

Guard admits the thread that performs filesystem effects after dispatch. The context is private, non-cloneable, non-Send, and non-Sync. The context records the creating thread identifier and retains a query-only process token.
The context owns a bounded copy of the validated TokenUser SID. The context also records TokenId, AuthenticationId, ModifiedId, and TokenSessionId.

Admission requires all of these observations:

| Predicate | Required value |
|---|---|
| Thread token | Absent. Only `ERROR_NO_TOKEN` proves absence |
| Process token type | Primary |
| TokenUser | Neither SYSTEM, LocalService, nor NetworkService |
| Elevation | `TokenIsElevated == 0` |
| Integrity | Exactly `SECURITY_MANDATORY_MEDIUM_RID`, `0x2000` |
| AppContainer | `TokenIsAppContainer == 0` |
| UI access | `TokenUIAccess == 0` |
| Filter history | `TokenHasRestrictions == 0` |
| Restricting SIDs | Empty |
| Session | Same as the initiating process session |
| Queries | Success with valid bounded structures |

Use `OpenThreadToken(QUERY, TRUE)` for the current thread. Success means impersonation and therefore refusal. Every failure except `ERROR_NO_TOKEN` refuses. Use `OpenProcessToken(QUERY)` for the retained primary token.
Do not request impersonation, token adjustment, or token duplication rights for admission. Preserve the existing one-MiB query-buffer ceiling. Validate every returned length, pointer, alignment, SID, and array count before access.

The initial profile excludes a filtered administrator token with `TokenHasRestrictions != 0`. Standard-user and filtered-administrator fixtures are separate support cells. Do not infer an unfiltered token from `IsTokenRestricted == FALSE`.
Do not query the reserved `TokenIsRestricted` information class.

Revalidate the actual worker before folder resolution, every protected creation phase, publication, and final commit. Revalidation checks the thread, absence of impersonation, primary-token identity, ModifiedId, session, and admission predicates.
A change invalidates the context and its unconsumed permit. No foreign callback or asynchronous dispatch may occur between the final recheck and its protected effect. This contract does not defend against arbitrary hostile code executing inside the trusted installer process.

Resolve `FOLDERID_LocalAppData` with `SHGetKnownFolderPath`, flags zero, and `hToken = NULL`. Resolve on the admitted worker and recheck its context after the call. Do not use an environment variable, request field, `KF_FLAG_CREATE`, or the Default User token.
Release the returned allocation on every outcome.

### D-43: Signed discovery and installation roots

D-WIRE owns `.keldea2`, publisher scope, exact field grammar, repository identity, and discovery-policy encoding. The preflight-observed signed Host section carries the discovery policy digest. D-45 permits only read-only discovery until full verification succeeds.
The acquisition bundle supplies retained discovery bytes before network access. Compare their exact SHA-256 with the signed section before decoding them. Missing or mismatched bytes refuse. A request locator supplies no authority.

The closed discovery policy has at most 16,384 bytes and 32 entries. It includes the current v2 root and every commissioned predecessor lookup. Each entry binds identity scheme, publisher scope, app, channel, target, root policy, and provenance producer.
The producer enums are `keld-provenance-v2` and `keld-provenance-v3`, as defined by D-WIRE. Unknown or unreadable producer evidence refuses. Known managed-owner evidence also refuses, independently of a candidate's Direct claim.
No policy entry executes a command or supplies an absolute destination path.

Fresh v2 root derivation is exactly:

```text
lp16(x) = u16be(byte_length(x)) || x
leaf = lowercase_hex(BLAKE3(
  ASCII("keld.windows.peruser.install-root/v2") || 0x00 ||
  verified_stable_publisher_scope[32] ||
  lp16(UTF8(app_id)) || lp16(UTF8(channel)) || lp16(ASCII(target))))
install_root = OS_LocalAppData / GYLDLAB / Keld / v2 / leaf
update_root = install_root / updates
versions_root = update_root / versions
```

Validate D-WIRE field bounds before hashing. Do not normalize, case-fold, or reinterpret the signed bytes. Retain the historical v1 root rule unchanged. A migrated installation uses its authenticated predecessor binding.
Recompute the predecessor rule and match its retained object with transition and provenance evidence. Never accept a caller-selected old path or infer permission to relocate data.

Discovery returns exactly one disposition:

| Disposition | Result |
|---|---|
| FreshAbsent | Continue without creating installation objects |
| ExactEmptyScaffold | Continue with validated retained objects |
| InstalledDirect | Refuse this entry and use existing update/recovery |
| Managed | Refuse before any network request and before direct-writer acquisition |
| PartialOrUncertain | Refuse and retain diagnosis evidence |
| ConflictingIdentity | Refuse without copying, merging, deleting, or relabeling state |

Evaluate all required discovery entries before returning FreshAbsent. An absent provenance file alone does not prove freshness. Inaccessible, corrupt, foreign, or unexpected objects do not mean absence. Recheck the disposition after obtaining the installation lease.

### D-44: Protected acquisition namespace

Use one repository namespace for one commissioned app across channels, signer rotations, and origin changes.
The repository identifier is the authenticated opaque 32-byte D-WIRE value. Its genesis and continuity authority come from the repository policy.
Require the Host, repository policy, and existing anchor to name the same appId before mutation.
First enrollment records that verified appId. A different app refuses instead of sharing or resetting the repository.
Do not derive the identifier from the current root key, profile, origin, channel, or policy digest.

```text
base = OS_LocalAppData / GYLDLAB / Keld / acquisition-v1
repo_hex = lowercase_hex(repository_id[32])
anchor = base / enrollments / (repo_hex + ".anchor")
store = base / repositories / repo_hex
```

After offline ownership discovery admits Direct and worker admission succeeds, the adapter may provision acquisition state.
Start from the existing admitted LocalAppData directory. Create only missing shared GYLDLAB and Keld ancestors and the acquisition-v1 subtree.
Apply the existing retained-parent, captured-SID, descriptor, fixed-volume, and exclusive-create checks to each component.
This narrow authority cannot create Keld/v2, any installation-specific or legacy root, or any bootstrap file.
Those objects require D-46's consumed permit. Managed or uncertain discovery permits no creation.
All D-45 signed Host and repository-policy checks still apply before acquisition mutation.
Guard applies the existing `windows-per-user-role-rx-v1` profile using W1's captured SID. Guard remains the descriptor and profile owner. All directory transitions use retained same-volume parents and one validated component.
Acquire the per-repository lease before enrollment or generation changes. The lease is private to the existing updater owner. Its creation never authorizes installation-tree creation.

D-METADATA exclusively owns anchor bytes, generation bytes, integrity checks, and atomic generation publication. The native adapter returns these states without weakening them:

| State | Required observation | Permitted operation |
|---|---|---|
| NeverEnrolled | Anchor and store absent, and no discovered installation references enrollment | Start authenticated enrollment |
| EnrollmentPending | Durable pending anchor, incomplete first generation | Explicit enrollment recovery only |
| Established | Matching anchor, current generation, descriptors, and retained identities | Normal validated refresh |
| CorruptOrMissing | Any established reference lacks required state, or validation fails | Refuse normal refresh and installation |

First enrollment writes a pending anchor before publishing its first accepted generation. It then commits established enrollment and rereads every required record. A crash leaves pending enrollment or established state. It cannot create NeverEnrolled.
Missing established generations never trigger bootstrap from the embedded root. Automatic deletion, regeneration, or fallback to a lower generation is forbidden. Process-local caches cannot override protected readback.

The `ReadyRevision` receipt includes repository ID, initial-generation digest, sequence, generation digest, and retained generation identity. The initial-generation digest identifies enrollment. D-METADATA owns its exact representation.
Generation counters use checked increment and refuse exhaustion. The metadata owner defines permitted root and role-state changes within each generation. KEL-53 application floors remain separate and cannot decrease through metadata recovery.

The local enrollment marker detects partial loss under the stated access-control boundary. It cannot prove history after an authorized actor restores or deletes every related local object. Independent durable authority is required for any stronger antirollback claim.

### D-45: Retained source and Authenticode contract

Reuse Guard's single `WindowsAuthenticodeImage` verifier and retained file handle. Its new `observe_preflight` operation returns only `WindowsPreflightImage`, with private fields and no authority conversion.
Use `WINTRUST_ACTION_GENERIC_VERIFY_V2`, `WTD_CHOICE_FILE`, and `WTD_UI_NONE`. Set `fdwRevocationChecks = WTD_REVOKE_NONE`. Set `dwProvFlags = WTD_REVOCATION_CHECK_NONE | WTD_CACHE_ONLY_URL_RETRIEVAL`, exactly `0x1010`. Do not set hash-only or policy-usage bypass flags.
Require zero status, one primary signer, valid local chain, bounded signer data, and successful CLOSE. Retain the existing 65,536-byte SPKI and 4,096-byte Opus bounds and D-WIRE's field bounds. Ambiguous or unavailable local trust refuses. It cannot trigger an online retry.
This observation supplies only signed fields and digests for local read-only discovery. It proves neither current revocation status nor installation permission. Managed or uncertain discovery ends before all network and writer acquisition.
Only an admissible Direct discovery result permits normal verification on the same retained handle. That verification uses the existing `WTD_REVOKE_WHOLECHAIN` and `WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT` policy.
It must succeed independently, recheck the same identity/policy values, and produce the existing full verified-image type. Neither a Boolean nor the preflight type can create that full type or an installation permit.
D-WIRE extends extraction under this same owner and its reviewed unsafe grant. Pass the handle through `WINTRUST_FILE_INFO.hFile` with the required non-null descriptive path. Close WinTrust state on every outcome and propagate cleanup failures.

Retain the verified Host handle through admission and commit. Read its identity section and policy bytes from that handle. Bind the exact Host bytes and actual signer observations into the permit. Certificate subject text alone cannot grant publisher continuity.

The adapter retains request, discovery, catalogue, signature, manifest, transport, and canonical TAR source objects. Small signed records use bounded immutable buffers and their original object identities.
Completed file sources open read-only with `FILE_SHARE_READ` and no write/delete sharing. Reject reparse leaves, nonregular files, and link counts other than one. Retain required source ancestors and compare volume/file identity before consuming bytes.
No verified source pathname may be reopened to obtain publication content.

Use the existing verifier to decode the transport into a private acquisition spool. Flush the spool writer, obtain its retained read handle, verify identity, and close writable access. Validate the canonical TAR through that retained handle.
`populate_stage` consumes the same handle and the same validated archive table. No caller constructs an admitted archive from a Boolean, path, or expected digest alone. Publication may reset or seek the retained handle but cannot replace the object.

Destination roots require fixed local NTFS with persistent ACL support. Reject network, removable, cross-volume ancestry, and reparse traversal. Pin each existing destination ancestor with no-delete-sharing semantics.
Do not assume `open_dir_nofollow` supplies those share flags without implementation verification. Extend the existing native opening owner if necessary. An admitted source volume may differ from the destination volume.
Each destination ancestor and publication rename remains on its admitted volume.

### D-46: One-shot admission and lock order

Proposed opaque interfaces use private fields and validated constructors:

| Type | Only constructor | Required consumer |
|---|---|---|
| `WindowsPerUserInstallContext` | Guard admits the actual worker under D-42 | Guard revalidation and captured-SID descriptor owner |
| `AdmittedWindowsArchive` | Existing verifier validates the retained canonical TAR and its custody | Existing KEL-53 stage publisher consumes that handle |
| `WindowsInitialInstallPermit` | Update joins D-42 through D-45 under the current metadata lease | Existing KEL-53 initializer consumes it once before first effect |

These names define proposed interfaces, not existing exported Rust APIs.

The permit binds these exact values:

- Worker and token identities, captured user SID, and authenticated Host digest.
- Publisher scope, app, channel, target, repository identity, and policy digests.
- Exact selected target path, catalogue digest, manifest digest, package digest, and validated content identity.
- Retained source objects, destination objects, root rule, and owner disposition.
- Enrollment identity, metadata generation, generation digest, and accepted root/role state.
- Selected refresh start, monotonic admission deadline, and metadata expiry checks.

The global order is metadata admission lease, installation bootstrap/update lease, then profile lease. A fresh destination has no bootstrap lock.
Hold the metadata lease while consuming the permit, creating its scaffold, and exclusively creating the existing KEL-53 bootstrap lock. Recheck discovery under the new bootstrap lock. Do not create a second installer mutex.
Existing installations acquire their existing update lease after the metadata lease. Never refresh while holding installation or profile leases. Use the selected refresh's 120-second monotonic deadline.
Each lock attempt shares that deadline and has no automatic retry after expiry. After any lock wait, recheck generation, expiry, caller, owner, roots, and source custody. If revalidation fails, release installation/profile leases before a new refresh.
The new refresh must independently select the same exact bytes to reuse their verification. Withdrawn or replaced selection refuses or restarts complete selection and verification.

Consume the permit on the admitted worker before the first installation-tree effect. Under the metadata lease, publish and reread D-RECOVERY's bound admission record before any installation-tree creation.
That first effect includes an installation-specific parent, Keld/v2, legacy root, scaffold, bootstrap file, or stage creation.
D-44 alone owns earlier shared-ancestor and acquisition provisioning. It cannot authorize any installation-specific effect. Hold the metadata lease until the durable admission record and exclusive bootstrap ownership are established. D-RECOVERY owns the record and its interruption semantics.
After admission, the existing transaction owns completion or recovery from its frozen inputs. Do not refresh metadata halfway through commit or rollback. Drop of an unconsumed permit releases leases and grants no effects.
Copying, serializing, or reusing a consumed permit is impossible through the safe API.

### D-47: Commit, interruption, and errors

Preserve the existing ordered baseline cuts:

```text
LockCreated -> ActivationLockCreated -> StageCreated -> StagePopulated
-> CompletePublished -> VersionPublished -> FloorPublished -> CurrentPublished
-> LastKnownGoodPublished -> RootsVerified -> ProvenancePublished
-> final protected baseline readback
```

Only the final protected reread permits a success receipt. Provenance remains the commit record. An empty bootstrap file is a lock, not an authenticated recovery journal. D-RECOVERY owns any resumable initial admission record and its exact crash behavior.
Without such a qualified record, pre-provenance interruption remains diagnosable incomplete state. Never reseed partial state automatically or call it a new install. A lost reply after commit requires read-only classification before any retry.
Process-kill evidence does not prove physical power-loss durability.

Reuse `WindowsTokenError`, `WindowsAuthenticodeError`, and `UpdateError` ownership. New error variants require the existing error registry and exact message tests in the implementation PR. The internal refusal class and effect state are distinct:

| Refusal class | Required effect statement |
|---|---|
| Caller, identity, policy, discovery, metadata, custody, or lease refusal | No installation-tree effect |
| Creation or publication failure before provenance | Incomplete installation preserved |
| Provenance published but reread or reply uncertain | Commit state unknown until protected reread |
| Successful final reread | Exact baseline committed |

Do not return success after a partial write or label ambiguous commit state unchanged. Error text includes the failing owner, operation, native status where applicable, and safe next action. It does not expose private key material or suggest bypassing verification.

### D-48: Serializable native observations
KEL-53 owns the local record codec. Existing handle observers supply its values. Use fixed-field ASCII JSON with no BOM, whitespace, or newline and exact encode/decode roundtrip. Reject duplicate, reordered, missing, or unknown fields. Each observation is at most 512 bytes.
Object fields, in order, are `schema,volumeGuid,device,file,kind`. The schema is `keld.windows-object-observation/v1`. `volumeGuid` comes from `windows_fs::qualified_volume_root` on the retained object.
Preserve its fixed prefix and suffix. Encode only its GUID hexadecimal digits in lowercase. `device` and `file` are exactly 16 lowercase hexadecimal digits from `retained_graph_metadata().dev()` and `.ino()`.
These are the existing u64 observations on supported NTFS, not 128-bit file identifiers. `kind` is exactly `directory` or `regular-file`, after the existing kind, reparse, and link checks. The object must also pass its owning descriptor and volume policy before observation.
Destination fields, in order, are `schema,kind,anchor,remaining`. The schema is `keld.windows-destination-binding/v1`. The maximum record is 4,096 bytes. `anchor` is one admitted directory observation. `kind` is exactly `existing` or `absent`.
Existing means the destination itself is the anchor and `remaining` is empty. Absent means the closest existing parent is the anchor, with one to six remaining components. Each component is a signed-root-derived, validated single Windows name of at most 255 UTF-8 bytes.
No request-supplied path, separator, traversal, alternate stream, or reserved-name alias is accepted. Use the existing namespace validator and exact D-43 root derivation.
After restart, rederive the root and reopen it through retained parents. Repeat descriptor, volume, and signed-record checks. File identifiers can be recycled. A matching recorded number alone never grants authority.
Journal every newly created component's admitted observation before creating its child. A component created before its observation was recorded is ambiguous after interruption. Recovery halts instead of inferring ownership.
FACT: Owners are `windows_fs.rs:103` and `windows_extraction.rs:733` at the index pin.

## 5. Boundaries

Owner-authorized rollback of every local record remains outside the claimed boundary. A process-kill result does not prove physical power-loss durability. A filtered administrator token can fail the conservative initial token profile.
Each support claim requires its own native cell. Guard's current unsafe grant does not authorize every new query in D-42. Implementation requires exact owner-file updates and independent unsafe review before the additional queries ship.
Offline preflight can refuse a valid image when the local certificate chain is unavailable. It cannot fall back to network before discovery. Native zero-network qualification remains required.

## 6. Tasks

### T-40: Implement the admitted worker
- **Blocked by:** T-80, T-82
Extend Guard's existing query owner and captured-SID descriptor contract. Run AC-40 on each claimed token cell.

### T-41: Implement source and acquisition custody
- **Blocked by:** T-40, T-20
Extend the existing source and metadata owners. Run AC-41, AC-42, and AC-43 against actual Windows objects.

### T-42: Integrate one-shot admission
- **Blocked by:** T-41, T-1, T-60, T-23
Join the approved codecs, metadata generation, native custody, and recovery journal. Run AC-44 and AC-45 with controlled subprocess boundaries.

### T-43: Qualify installed behavior
- **Blocked by:** T-42, T-81
Use commissioned production policy and signed artifacts. Run the full applicable acceptance matrix and independent source review. Do not enable the Host entry before its authority and qualification gates pass.

| Existing owner path | Concrete implementation change after specification approval |
|---|---|
| `crates/keld-guard/src/windows_machine/uac_token.rs` | Extend query owner and bounded parser for admitted actual-worker context |
| `crates/keld-guard/src/windows_authenticode.rs` | Extend the single verified-signature identity owner for D-WIRE policy |
| `crates/keld-pack/src/expected_identity.rs` | Add versioned codec and signed policy digests from D-WIRE |
| `crates/keld-update/src/windows_extraction.rs` | Retain admitted archive sources and verified ancestry without pathname reopen |
| `crates/keld-update/src/windows_fs.rs` | Native handle operations and exact same-volume protected publication |
| `crates/keld-update/src/windows_baseline/initialize.rs` | Consume one-shot admission before any initialization effect |
| `crates/keld-update/src/windows_baseline/initialize/publication.rs` | Accept the retained validated TAR through publication |
| Existing KEL-53 metadata integration owner | Enrollment, generation, metadata lease, and retained readback |
| `crates/keld-host/src/windows_initial_install.rs` | Thin commissioned-policy adapter after upstream gates pass |

The current Host refuses unconditionally. The new adapter cannot replace that refusal before authority commissioning and its required checks. The first code slice can strengthen token and retained-source ownership while keeping the Host closed.
That slice still needs approved specification scope and its unsafe/public-API/security review gates. No native tests, physical power-loss tests, production signatures, release approval, or merge occurred in this work.

## 7. Test plan & evidence

### E-40: Exact source inspection
- **Run:** GitHub file reads at the index source pin and current PR metadata on 2026-11-15.
- **n:** One current PR head and the listed owner sources.
- **Environment:** Remote GitHub source inspection. No Windows execution occurred.
- **Limits:** Source inspection proves the cited code shape. It does not prove native runtime behavior.

### E-41: Native acceptance
- **Run:** Not run. Execute AC-40 through AC-46 after implementation under the approved specification.
- **n:** Zero native runs in this definition task.
- **Environment:** Required real Windows supported modes and signed artifact set.
- **Limits:** No native admission, installed-product, crash, or power-loss qualification is claimed.

All rows below are NOT_RUN. Source inspection is not native test evidence. Each row requires a working positive, one targeted fault, a failing mutant, and a restored pass. Keep unrelated signature and policy layers valid so the intended predicate is reachable.

| Case | Single fault | Independent oracle |
|---|---|---|
| Offline preflight | Remove cache-only flag with an absent intermediate | Independent AIA endpoint and OS telemetry detect forbidden retrieval |
| Actual worker | Dispatch admitted intent to an impersonating worker | OS token facts and zero protected creations |
| Token errors | Return access denied instead of ERROR_NO_TOKEN | Exact refusal, no fallback token |
| Restricted token | Remove privilege without restricting SID | TokenHasRestrictions refusal despite IsTokenRestricted false |
| UAC/profile | Use elevated, filtered, low, high, service, AppContainer, or UIAccess token | Separate native result for each cell |
| Folder origin | Substitute environment or request destination | Writes remain absent at injected location |
| Managed discovery | Present valid managed provenance with valid direct candidate | Zero feed requests and no direct-writer handle |
| Established loss | Remove generation while retaining anchor | CorruptOrMissing, no embedded-root bootstrap |
| Enrollment crash | Kill after every anchor/generation publication boundary | Pending or matching established state, never false first use |
| Stale permit | Advance generation before admission lock succeeds | Old permit cannot create any installation-specific parent |
| Expiry wait | Hold lease past monotonic deadline | Refusal before first creation |
| Source replacement | Attempt rename, hardlink, reparse, or writable competing open | OS refusal or identity refusal, original object consumed |
| Archive path reopen | Replace the path after validation with equal-sized foreign bytes | Retained verified handle supplies every copied byte |
| Ancestor swap | Replace any retained destination ancestor | OS refusal or identity refusal, no foreign write |
| Concurrent installers | Synchronize two valid requests at first creation | One committed baseline, loser cannot repair or reset |
| Crash cuts | Kill worker at each listed baseline cut | No false receipt and exact protected restart classification |
| Lost response | Kill after committed provenance before response | Readback finds one committed installation, no duplicate seed |
| Joined migration | Changed SPKI and U key, existing browser data, failed candidate | Correct profile, provenance, retained floor and authorized LKG together |

Use subprocess barriers and OS-visible observations, not sleeps or log messages alone. Native logs record actual SID/token facts, file identities, exact source/artifact hashes, and child exit states. Do not publish a blanket refusal as proof that a deeper check works.

Official references retrieved 2026-11-15:

- [WINTRUST_DATA](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/ns-wintrust-wintrust_data): "Prevents revocation checks over the network." Its code-signature guidance requires `WTD_CACHE_ONLY_URL_RETRIEVAL`; `WTD_REVOKE_NONE` alone is insufficient.

- [WinVerifyTrust](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-winverifytrust): "No other value besides zero should be considered a successful return."
- [WINTRUST_FILE_INFO](https://learn.microsoft.com/en-us/windows/win32/api/wintrust/ns-wintrust-wintrust_file_info): "This parameter cannot be NULL." The page refers to `pcwszFilePath`; `hFile` accepts an open readable file.
- [OpenThreadToken](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openthreadtoken): "TRUE if the access check is to be made against the process-level security context."
- [Token information classes](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-token_information_class): "nonzero if the token has ever been filtered." `TokenIsRestricted` is reserved.
- [IsTokenRestricted](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-istokenrestricted): "checks only for the list of restricting SIDs."
- [SHGetKnownFolderPath](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath): "the function requests the known folder for the current user." This applies when `hToken` is NULL.
- [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew): "Delete access allows both delete and rename operations." Share flags remain relevant for every retained object.

Context7 discovery used `/websites/learn_microsoft_en-us_windows_win32_api` on 2026-11-15. The query covered current-user known folders and query-only thread-token behavior.
Context7 returned related known-folder APIs and OpenThreadToken. Direct Microsoft documentation confirmed the selected APIs.

## 9. Perf impact

The cold installation path adds bounded token queries and retained handles. No throughput or latency improvement is claimed. Metadata deadlines and payload bounds remain with D-METADATA.

## 10. Open questions, blockers & unknowns

### B-40: Native qualification
- **Status:** open
- **Owner:** Guard and existing Windows acceptance owners.
- **Impact:** The proposed path cannot receive production or release disposition.
- **Check:** AC-40 through AC-46 and the joined migration cell have exact-source native receipts.

### B-41: Commissioned policy
- **Status:** open
- **Owner:** Release/security with KEL-135 and KEL-254.
- **Impact:** The shipping Host remains closed until genuine authority and signed-policy evidence exists.
- **Check:** Independent review matches commissioned issuer/profile, repository, discovery, and artifact identities.
