# Windows admission and source custody

Part of: [index](index.md)
Owns: D-ADMISSION, actual-worker tokens, discovery, protected namespaces, source custody, one-shot admission, and native oracles.

## 4. Design: Windows admission

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
