# Identity formats and planned transitions
Part of: [index](index.md)
Owns: D-WIRE. KEL-135 owns identity. KEL-254 owns signed-image expectations. KEL-53 and KEL-270 own installation state.

## 4. Design: Identity

### D-1: Reuse and ownership

Keep one PE parser and payload codec in `keld-pack`.
Keep one Authenticode observation producer in `keld-guard`.
Keep one installation writer, record codec, and activation journal in `keld-update`.
Keep browser identity and exclusive store ownership with KEL-135.
Do not create a parallel installation writer, profile registry, manifest decoder, or freshness protocol.
The new contract separates persistent identity from an artifact's observed signing key.
No performance improvement is claimed.

### D-2: Common wire grammar

All new JSON objects are closed. Reject unknown, missing, duplicate, or misordered members.
Reject BOMs, invalid UTF-8, floating-point numbers, negative integers, trailing bytes, and trailing whitespace.
Use compact UTF-8 JSON in the field order shown below. Add no final newline.
Compare input with the owning typed encoder's output after parsing.
The encoder uses existing `serde_json::to_vec` conventions. It is not a new canonical-JSON standard.
Sign literal input bytes before granting authority. Canonical decoding is an additional check.
All network-policy strings are ASCII. Protected native path strings retain the existing record codec's rules.
Each JSON input has a 65,536-byte cap unless this part gives a smaller cap.
Reject recursive input deeper than 16 containers. Reject strings over their stated byte bounds before copying them.

| Type | Exact rule |
|---|---|
| `h32` | Exactly 64 lowercase hexadecimal ASCII characters, representing 32 bytes |
| `key32` | `h32`, then strict Ed25519 verification-key validation, including weak-key refusal |
| `n53` | JSON integer from 1 through 9,007,199,254,740,991 |
| `app` | Existing KEL-135 grammar, 1..255 ASCII bytes |
| `channel` | Exactly `stable`, `beta`, or `canary` |
| `target` | Exactly `windows-x64` in this qualification cell |
| `version` | Existing strict SemVer spelling, 1..255 ASCII bytes |
| `oid` | Canonical dotted decimal, 3..255 ASCII bytes, at most 64 arcs |
| `lp8(x)` | One unsigned length byte followed by exactly that many UTF-8 bytes |
| `lp16(x)` | Two unsigned big-endian length bytes followed by exactly that many ASCII bytes |

An OID has at least two arcs. Each arc fits an unsigned 32-bit integer.
The first arc is 0, 1, or 2. A first arc below 2 limits the second arc to 39.
Reject signs, spaces, empty arcs, leading-zero aliases, overflow, and textual normalization.
Reject a profile OID that equals a generic usage OID.
The Public Trust profile must match the full commissioned OID. A common prefix grants no authority.

### D-3: Publisher identity and policy

The provider enum value `1` means `microsoft-artifact-signing-public-trust`.
The provider requires the code-signing EKU `1.3.6.1.5.5.7.3.3` and Public Trust EKU `1.3.6.1.4.1.311.97.1.0`.
For an ordinary application or candidate Host, the same verified primary
signer supplies the complete application profile OID, leaf SPKI, and
authenticated app carrier. A dedicated Recovery Host instead uses D-61's
separately pinned recovery-only profile for signer authorization; its signed
expected-identity section still names the destination application. It is
not an ordinary application Host, and its recovery signer MUST NOT acquire
ordinary launch or catalogue permission.
Retain the existing one-primary/zero-secondary signature rule and Authenticode trust, timestamp, and revocation policy.
Unrelated additional EKUs do not count as extra signers. The separately
commissioned recovery profile OID is **not** unrelated. An ordinary
application/candidate Host MUST match the full authorized application profile
OID and MUST NOT contain the corresponding full recovery-only profile OID
in its verified primary signer leaf. A RecoveryImage MUST match the recovery
profile OID and MUST NOT contain the application profile OID. Reject a
single valid leaf carrying both role-specific EKUs before either role grants
authority; chain success, the generic Public Trust marker and the
one-primary-signature rule do not override role exclusivity.

Select the complete authorized policy cohort by operation, before granting candidate authority:

| Operation | Policy cohort |
|---|---|
| Fresh admission | Complete signed Host-pinned D-9 cohort after D-45 full verification and fresh admission checks |
| Ordinary launch or unchanged-policy update | Complete admitted installed cohort |
| D-7 planned-transition candidate | Exact authenticated destination cohort, after independent source-authorized transition validation |
| D-61 loss-only recovery candidate | Complete unchanged installed application cohort |
| Dedicated RecoveryImage | Separate installed recovery-only cohort under D-61 and D-62 |

D-7 selects candidate verification policy from the exact source-authorized transition and its authenticated destination policy.
Require the admitted source, installed U1 signature, applicable U2 signature, and fresh independent metadata authorization.
Then require full candidate verification and exact destination catalogue, manifest, signed images, policy bytes, and package agreement.
All checks MUST pass before candidate admission and journal publication.
The candidate signer MUST exclude both authenticated source and destination recovery-only profile OIDs, counting equal OIDs once.
An active source recovery profile cannot become the destination application profile during that transition.
A candidate's signed fields alone MUST NOT authorize a different policy cohort.
Destination authority is attempt-bound. It cannot replace current provenance, source/LKG verification, or ordinary-launch policy before D-9 commit.
Missing transition authority MUST refuse, without falling back to fresh admission or recovery.

`PublisherPolicy` has these ordered fields:

| Field | Value |
|---|---|
| `schema` | `keld.publisher-policy/v2` |
| `provider` | `microsoft-artifact-signing-public-trust` |
| `profileOid` | `oid`, exact commissioned profile |
| `issuerPolicySha256` | `h32`, SHA-256 of exact `IssuerPolicy` bytes |

`IssuerPolicy` has these ordered fields:

| Field | Value |
|---|---|
| `schema` | `keld.public-trust-issuer-policy/v1` |
| `chainPolicy` | `windows-generic-verify-v2` |
| `rootCertificateSha256` | 1..8 distinct `h32`, sorted bytewise |
| `leafIssuerSpkiSha256` | 1..8 distinct `h32`, sorted bytewise |

Accept only a Windows-validated chain that satisfies both issuer lists for the same verified leaf.
These lists add restrictions. They cannot bypass Windows chain policy or replace an unsuccessful chain result.
Record actual commissioned certificate evidence before enabling the production cell.
Policy changes require the complete authenticated successor contract. A matching stable identity alone never authorizes new issuer policy.

Derive the stable publisher scope as follows:

```text
SHA256(ASCII("keld.publisher.windows/v2") || 0x00 ||
       lp16("microsoft-artifact-signing-public-trust") || lp16(profileOid))
```

Issuer policy is outside this stable digest. Its exact digest remains authenticated separately.
Derive a fresh browser identity as follows:

```text
SHA256(ASCII("keld.profile.identity/v2") || 0x00 ||
       publisherScope[32] || lp16(appId))
```

Do not include channel, current SPKI, current update key, or metadata version in browser identity.
Profile recreation changes the profile OID. Treat that event as a publisher transition.
The v1 publisher and profile formulas remain unchanged.

### D-4: Expected identity v2

The section name is exactly the eight ASCII bytes `.keldea2`.
Preserve existing PE alignment, final-section, zero-padding, no-overlap, and Authenticode-covered container rules.
Place a dedicated recovery Host's `.keldrec` section before `.keldea2`. Keep `.keldea2` as the final section.
Verify both sections through the same retained signed image. D-RECOVERY owns the recovery-section payload and authorization.
Exactly one supported identity section is permitted. Reject a PE that contains both `.keldeai` and `.keldea2`.
Do not change `.keldeai` decoding to accept v2 payloads.
The v2 payload has the following exact order:

| Field | Bytes |
|---|---:|
| Domain `keld.expected-app-identity/v2` followed by NUL | 30 |
| `appId` | `lp8`, 1..255 |
| `channel` | `lp8`, 1..16 at the byte codec, then `channel` validation |
| `target` | `lp8`, 1..64 at the byte codec, then `target` validation |
| `updatePublicKeyEd25519` | 32 |
| `provider` | 1, value `0x01` |
| `profileOid` | `lp16`, 3..255 |
| `issuerPolicySha256` | 32 |
| `repositoryId` | 32 |
| `repositoryPolicySha256` | 32 |
| `discoveryPolicySha256` | 32 |
| `owner` | 1, value `0x01` means direct |
| `installMode` | 1, value `0x01` means per-user-direct |
| `principalModel` | 1, value `0x01` means strict-distinct-os-principals |
| `rootPolicy` | 1, value `0x02` means keld-localappdata-app-v2 |

The byte codec accepts no trailing byte. Its derived payload range is 206..790 bytes.
Semantic validation applies after byte decoding and before any authority construction.
The companion verifier checks the exact payload from the same retained Authenticode-verified image.
Produce the payload before signing and timestamping. Hash the final Host only afterward.
Issue the detached catalogue last. Do not embed that catalogue into the Host.

### D-5: Repository policy and bounded discovery

The repository ID is an opaque commissioned 32-byte value.
This profile assigns exactly one canonical app ID to each repository ID. Multi-app repositories are unsupported.
Keep it stable across key, channel, profile, origin, and policy changes.
Never derive a new repository namespace from an origin or policy digest.
Require the same app ID in Host expectations, repository policy, catalogue, discovery policy, and enrollment anchor.
Refuse reuse of an enrolled repository ID for another app. Commission a distinct repository ID for each distinct app.
`RepositoryPolicy` has the following ordered fields:

| Field | Value |
|---|---|
| `schema` | `keld.repository-policy/v1` |
| `repositoryId` | `h32` |
| `appId` | `app`, permanently bound to this repository ID |
| `metadataBaseUrl`, `targetsBaseUrl` | Each a canonical HTTPS base URL, 1..2,048 ASCII bytes |
| `bootstrapRootSha256` | `h32` |
| `bootstrapRootVersion` | `n53`, commissioning floor |
| `metadataProfile` | Integer `1`, the single metadata profile from D-METADATA |
| `target` | `target` |
| `catalogueKeys` | 1..4 records, ordered by `keyId`, distinct IDs and public keys |
| `publisherPolicySha256` | `h32`, exact canonical `PublisherPolicy` digest |
| `discoveryPolicySha256` | `h32` |
| `recoveryPolicySha256` | `h32`, exact independent recovery policy from D-RECOVERY |

Each catalogue-key record has exactly `keyId`, then `publicKeyEd25519`.
`keyId` uses existing `SigningKeyId`: BLAKE3 of the raw Ed25519 public key.
The catalogue role still accepts exactly one selected signature. A root quorum does not change that envelope.
URLs have lowercase ASCII hostnames, no user information, no query, no fragment, and no non-default port.
URL paths end with `/`. Reject percent escapes, dot segments, backslashes, empty internal segments, or redirects.
Each nonempty path segment contains only ASCII letters, digits, `.`, `_`, `~`, or `-`.
The admitted repository ledger retains its floors when a valid successor changes these URLs or policy bytes.
The recovery policy digest adds no planned-transition signature exception. D-RECOVERY owns its separate signed-bootstrap procedure.

`DiscoveryPolicy` has ordered fields `schema`, `repositoryId`, `appId`, `entries`.
Its schema is `keld.windows.discovery-policy/v1`. Its byte cap is 16,384.
The list has 1..32 entries. Include the current-v2 root and every supported predecessor root.
Each entry has ordered fields `identityScheme`, `publisherScope`, `appId`, `channel`, `target`, `rootPolicy`, `ownerProducer`.
Supported scheme/root pairs are `spki-v1`/`keld-localappdata-app-v1` and `public-trust-profile-v2`/`keld-localappdata-app-v2`.
`ownerProducer` is `keld-provenance-v2` or `keld-provenance-v3`, matching the supported record decoder.
Sort entries by the complete tuple in field order. Reject duplicate tuples.
Every app equals the policy app. Every root derives from the admitted user and the stated exact identity.
The native part owns each root formula. No entry contains a caller-selected absolute path.
Read discovery bytes from the retained acquisition bundle before network access.
Verify their SHA-256 against the verified Host. Missing or unreadable discovery is a refusal.
Discovery data authorizes bounded lookup only. It never grants installation authority.
Known managed ownership refuses before network access. Unsupported ownership evidence also refuses.

### D-6: Catalogue v2

The catalogue target path is `catalogues/<appId>/<channel>/windows-x64/current.json`.
Build the path from verified Host fields. Do not search archived catalogue targets for a match.
The detached catalogue signature is exactly 64 raw Ed25519 bytes.
Its message is `ASCII("KELD-WINDOWS-INITIAL-CATALOG-v2") || 0x00 || literal_catalogue_bytes`.
The selected key must occur in the Host-pinned repository policy.
Complete the standard metadata target verification before parsing or using the catalogue.

The catalogue has ordered fields `schema`, `repositoryId`, `repositoryPolicySha256`, `catalogueKeyId`, `appId`, `channel`, `target`, `publisherPolicy`, `discoveryPolicySha256`, `host`, `package`, `updatePublicKeyEd25519`, `installIntent`.
`schema` is `keld.windows.initial-catalogue/v2`.
`publisherPolicy` is the complete object from D-3. `host` has `sha256`, then `bytes`.
`package` has ordered fields `version`, `manifestSha256`, `manifestBytes`, `manifestSignatureSha256`, `manifestSignatureBytes`, `transportBlake3`, `transportBytes`, `contentBlake3`, `contentBytes`, `profileDigest`.
Digest fields use `h32`. Length fields use `n53` and existing stricter package/manifest bounds.
`profileDigest` means the Guard strict-principal policy digest. It is not browser identity.
`installIntent` has ordered fields `owner`, `mode`, `principalModel`, `rootPolicy`.
Its values are `direct`, `per-user-direct`, `strict-distinct-os-principals`, `keld-localappdata-app-v2`.
Every duplicated expected field must equal the signed Host and authenticated policy.
The existing manifest verifier still checks its unchanged literal-byte signature format and exact package selection.
A valid catalogue or TUF signature cannot replace the manifest signature or actual Guard policy verification.

### D-7: Planned transition document

The target path is `transitions/<appId>/<channel>/windows-x64/<installedKeyId>/<sourceContentBlake3>.json`.
Derive all segments from the admitted source. Refresh and authenticate that exact target through D-METADATA.
The document has ordered fields `schema`, `repositoryId`, `appId`, `channel`, `target`, `source`, `destination`, `rollback`, `browserStore`, `installMode`, `principalModel`.
`schema` is `keld.windows.identity-transition/v1`.
`installMode` and `principalModel` keep their direct/strict values. A transition cannot change either value.
`source` and `rollback` use the complete `ArtifactBinding` below.
`destination` has `catalogueSha256`, followed by the same `ArtifactBinding` fields.
`browserStore` has `mode`, then `profileIdentity`. The only mode is `retain-source`.
The profile identity must equal the exact admitted physical predecessor store.

This target selects one exact transition document per authenticated metadata
revision, not every installation history of the source artifact. Distinct
installations can share all target-path segments while retaining different
browser identities: a fresh P2 installation uses P2's derived identity, while
an installation migrated from P1 to the same P2 artifact may retain P1's
identity. The source artifact and update key do not distinguish those cohorts.
The release owner MUST identify the exact retained-binding cohort served by
the published document. Only a source whose complete artifact, rollback and
browser bindings match that document may enter this transition. Other cohorts
remain on their existing admitted state and are deferred from this transition;
never report their transition as qualified or complete. They retain only the
ordinary operation permitted by their existing authority, not an exemption
from any revocation, floor or admission predicate.

This MVP does not define authenticated variant selection at this target.
Do not search archived targets, accept a caller-selected variant, wildcard the
profile binding, derive a replacement profile from the current publisher, or
copy/reset retained browser data to make a mismatching document pass. Serving
additional cohorts requires a separately authenticated matching publication
under the existing refresh rules, or a separately specified and approved
variant protocol. A publication for one cohort does not prove simultaneous
rollout support for all histories of that artifact.

Qualification MUST include two independent sole-channel installations of the
same source artifact and update key: one fresh under P2 and one migrated from
P1 with its distinct retained profile. Publish a genuine otherwise-valid
transition for one cohort. Require its matching-cohort positive and the
other cohort's refusal before transition preparation or candidate effects,
with both original stores and bindings unchanged by refusal. Independently
omit only exact browser-binding comparison and require that refusal oracle
to fail. Repeat with the other cohort's authenticated document. These are
NOT_RUN implementation cases, not current native qualification.

`ArtifactBinding` has ordered fields `version`, `contentBlake3`, `hostSha256`, `helperSha256`, `manifestSha256`, `updatePublicKeyEd25519`, `hostSpkiSha256`, `identity`.
Its digest fields use `h32`. Its key uses `key32`.
`identity` is one of these closed objects:

| Tag | Exact ordered members |
|---|---|
| `spki-v1` | `scheme`, `publisherScope` |
| `public-trust-profile-v2` | `scheme`, `publisherScope`, `publisherPolicySha256` |

The source must match the admitted current artifact and complete identity.
The rollback binding must match the admitted LKG artifact. No range, wildcard, arbitrary path, or unconstrained key set is permitted.
The destination must match its authenticated catalogue, manifest, signed Host, helper, policy, and package.
The destination scheme is v2. The source is v2 or a specifically qualified v1 bridge.
Refuse a no-op transition. Ordinary unchanged-key renewal uses the existing update path.

The signature message is `ASCII("KELD-WINDOWS-IDENTITY-TRANSITION-v1") || 0x00 || literal_transition_bytes`.
Require one 64-byte raw signature from installed U1.
When U2 differs, require a second 64-byte raw signature from U2 over the same message.
When U2 equals U1, require exactly one signature and reject an additional signature file.
Require fresh standard metadata authorization independently of those signatures.
Catalogue-key rotation does not satisfy U1 authorization.
Lost-U1 recovery is a different operation. The recovery part defines its independent authority and ceremony.
Never turn a missing planned-transition signature into recovery authorization.
Manual recovery supports only the defined v2 loss-only cell under the same application publisher and issuer policies.
It cannot change publisher, profile, identity scheme, discovery policy, or root policy.
A dedicated RecoveryImage is distinct from the candidate Host. It cannot satisfy the candidate catalogue's Host digest.
In a RecoveryImage, the signed `.keldea2` application profile and issuer
identify the destination application; they do not authorize its distinct
recovery-signing credential. D-61 binds the primary signer separately.
The recovery role MUST NOT use a matching destination application profile
as a fallback when any recovery-only signer pin fails.

### D-8: Protected provenance and lifecycle

The new record schema is `keld.install-provenance/v3`. Its maximum size remains 65,536 bytes.
Its ordered fields are `schema`, `owner`, `protection_profile`, `identity`, `publisher`, `volume_guid`, `repository_id`, `repository_policy_sha256`, `discovery_policy_sha256`, `browser_binding`, `baseline_signers`, `recovery_bindings`.
`owner` is `direct`. `protection_profile` keeps the existing `windows-per-user-role-rx-v1` value.
Retain the exact existing `WireIdentity` fields, order, validation, and path rules.
Its fields are `install_mode`, `app_id`, `channel`, `target`, `install_root`, `update_root`, `signing_key_id`, `baseline`, `profile_digest`, `principal_model`.
Its baseline fields are `app_id`, `channel`, `target`, `version`, `content_blake3`.
`identity.signing_key_id` names the current update key. `identity.baseline` names the complete baseline for this identity generation.
`publisher` has `scheme`, `scope`, `policy`. These contain the v2 scheme, derived scope, and full `PublisherPolicy`.
`baseline_signers` has exactly `host`, then `helper`. Each has `sha256`, then `spki_sha256`.
These observations describe the recorded baseline. They do not pin every future artifact to those SPKIs.
Guard rechecks each current/candidate artifact against its authorized publisher policy and exact authenticated package.
The actual artifact SPKI remains evidence. It is never substituted for stable publisher scope.

`browser_binding` is a closed tagged union.
Fresh binding fields are `kind`, `profile_identity`. Its kind is `fresh-v2`.
Retained binding fields are `kind`, `profile_identity`, `owner_record_sha256`, `predecessor`. Its kind is `retained-predecessor`.
The retained predecessor is one exact discovery entry. Its owner-record digest comes from protected readback.
The fresh binding creates no browser store during installation. KEL-135 creates that store at first admitted boot.
A fresh binding uses the derived v2 profile identity. A retained binding uses the exact admitted predecessor profile identity.
Acquire the exclusive lease by that physical-store identity in both old and new views.
Do not copy, merge, delete, rename, or transform browser data during identity transition.

`recovery_bindings` contains zero or one record, with ordered fields `artifact`, `prior_provenance_sha256`, `authorization`, `profile_identity`.
`authorization` has ordered fields `kind`, `sha256`. Its kind is exactly `planned-transition` or `manual-recovery`.
The digest names the exact transition or recovery directive. The kind selects its distinct authorization checks.
`artifact` uses `ArtifactBinding`. Retain the authenticated authorization, required signature evidence, and prior provenance as protected evidence.
An eligible binding authorizes only the named LKG artifact through the existing recovery transaction.
It does not give U1 authority to select or publish new releases after U2 commits.
Remove the binding only when no current, LKG, previous-LKG, or pending attempt needs it.
Refuse a second transition while a previous required binding occupies the single recovery slot.
V1 Hosts cannot read v3. A legacy bridge must understand the restricted recovery view before migration.

Define lifecycle v2 as `BLAKE3(ASCII("keld.windows.lifecycle-installation/v2") || 0x00 || u64le(record_length) || canonical_v3_provenance)`.
Keep the existing lifecycle domain for provenance v2 unchanged.
One transition journal binds both lifecycle IDs and one existing attempt ID.
Old process birth, challenge, nonce, or health evidence cannot authorize the new lifecycle ID.

### D-9: One transaction and supported migration cells

A changed repository-policy digest additionally requires integration D-84 through D-87.
Those cuts govern its metadata-policy switch and post-commit finite rollback view.

Keep the existing KEL-53 phase owner. Extend its journal with a separately versioned transition case.
The journal references complete before/after provenance, exact artifacts, transition bytes, signature evidence, and admitted metadata revision.
Do not embed those complete records inside the bounded journal.
The complete before/after objects include actual local roots. The globally published transition contains no per-user paths.
The journal binds each provenance object's SHA-256 and lifecycle ID.
The metadata owner supplies the revision and expiry receipt. The native owner supplies SID, volume, handles, and leases.

Each evidence reference is a closed object with ordered fields `kind`, `sha256`, `bytes`.
`sha256` uses `h32`. `bytes` is an integer from 1 through 65,536.
The closed kinds are `provenance`, `transition`, `signature-u1`, `signature-u2`, `metadata-admission`, `repository-policy`, `recovery-policy`, `publisher-policy`, `issuer-policy`, `discovery-policy`, and `recovery-directive`.
The kind selects one decoder and its stated schema. Signature kinds contain exactly 64 raw bytes.
`metadata-admission` uses D-27's canonical `keld.metadata-admission/v1`
codec, not a private in-memory ReadyRevision or a self-signed permission.
Store each object at `evidence/objects/<kind>/<sha256>` beneath the protected repository store.
The exact kind and digest determine its location. No attempt ID or discovery search participates in lookup.
All path components derive from validated typed fields. No reference contains an arbitrary path.
The existing KEL-53 writer retains validated parents during object creation, publication, and readback.
Create objects without replacement. Verify exact length, digest, protection, type, and schema before referencing an object.
Publish and verify every required object before publishing the journal reference that consumes it.
An existing object must pass the same readback checks. Digest equality alone does not prove protected custody.
Fresh admission publishes the complete seven logical policy references
before its admission record or provenance can reference that set.
The set contains repository policy, application publisher policy,
application issuer policy, discovery policy, recovery policy, recovery
publisher policy and recovery issuer policy, each with its exact authenticated
digest. The installed repository policy pins the recovery policy; the latter
pins its recovery-only publisher and issuer objects through D-61. If the
application and recovery issuer bytes are identical, one content-addressed
issuer object can satisfy both references, but a shared issuer never
authorizes the wrong signer profile.
This publication uses D-64 and the same retained-parent writer. A partial policy set grants no installation authority.

The current policy cohort has exactly seven logical references and at most
seven distinct objects, each at most 65,536 bytes. Retain this cohort
independently of transition and recovery evidence.
Each pending transition or retained recovery set has a cap of **24 distinct
objects** and **1,310,720 aggregate bytes**. A set includes every
required policy object for both the source and destination, not only
its non-policy receipts. Apply those bounds to each set independently.

For a v2 U1-to-U2 transition with different issuer policies,
the worst-case required set has 20 distinct objects:

| Evidence category | Maximum distinct objects |
|---|---:|
| Source policy cohort: repository, app publisher, app issuer, discovery, recovery, recovery publisher, recovery issuer | 7 |
| Destination policy cohort, when every policy digest differs | 7 |
| Exact before and after provenance | 2 |
| One signed transition document | 1 |
| Distinct U1 and U2 signature evidence | 2 |
| One metadata-admission receipt | 1 |
| **Required total** | **20** |

The 24-object cap leaves four additional objects for required owner-local
evidence. Each additional object MUST use an already defined kind,
decoder, digest and length. The complete set MUST pass both caps before
the transaction publishes its consuming journal. A 25th distinct object
or byte count above 1,310,720 MUST refuse with no partial authority.

Count a repeated exact `(kind, sha256)` reference once within each set.
The repeated reference MUST have an identical length and authenticated
bytes. Across current, pending and retained sets, store identical
objects once at D-9's kind-and-digest-derived path. Do not count the
same physical object twice for disk retention, but never exclude a
referenced object when checking a set's complete membership.
Permit at most one pending set and one retained recovery set per installation.
Including the current cohort, retention is bounded by at most
55 distinct objects and 3,080,192 aggregate evidence bytes before
cross-set deduplication. Do not delete any current, previous-LKG or
pending-authority object needed by those sets.

A missing recovery-only signing policy MUST fail the commissioned
profile; never fall back to the application publisher policy.
Reject admission when a required set exceeds its object or byte cap.
Do not omit required evidence merely to satisfy a bound.
Keep all objects needed by current, LKG, previous-LKG, or pending state.
Current provenance and a durable initial-admission record also root policy retention, even when `recovery_bindings` is empty.
Garbage collection cannot delete those objects. Refuse a new transition if safe retention would exceed the set limit.
Protected provenance holds authenticated policy references. The referenced immutable objects retain the exact policy bytes.
Missing, corrupt, or wrongly protected objects require diagnosis or recovery. They never authorize reconstruction from feed or caller input.

| State | Rule |
|---|---|
| Stable old | Use only the complete old identity |
| Prepared | Hold metadata, installation, then physical-profile leases in that order |
| Journaled | Durable pending state blocks ordinary launch |
| Candidate | Give only the exact candidate an attempt-bound after-state view |
| Health accepted | Complete process retirement and browser release barriers |
| Commit pending | Publish after-state while the journal still blocks ordinary launch |
| Stable new | Protected readback matches the complete after-state before journal finalization |
| Rollback | Retire candidate first, restore the complete permitted old state, retain the required floor |

Never fetch replacement metadata halfway through commit or rollback.
Never lower the application floor to enable metadata recovery, identity rollback, or LKG execution.
Qualify `1 -> failed 2 -> restored 1 with floor 2 -> eligible 3` with the identity transition in the same test.
Fresh-v2 and v2-to-v2 tests do not depend on a legacy bridge.
Enable v1-to-v2 only after an authentic original-signer/U1 bridge exists and passes joined AC-4.
An unavailable old signer blocks that legacy cell. It does not block pure v2 codec or native v2 qualification.
