# Windows v2 local development MVP

Status: proposed design supplement for independent review. Not a native owner grant.
Tracker: KEL-270. Source: `7f48875e82caea45e596fada4a3f387f220fbe0f`.

## Scope

The owner requires a locally signed Windows x64 installer for internal testing.
Public distribution, paid signing, and production Public Trust commissioning are deferred.
The production design and its existing evidence remain unchanged.
Local signing does not grant installation authority or remove a native security check.

## Proposed minimal design delta

Keep the existing Pack codec, Guard verifier, TUF client, and KEL-53 transaction owner.
Do not introduce another installer writer or signature algorithm.

| Boundary | Internal development profile | Production profile |
| --- | --- | --- |
| Build selection | Explicit development build with independent immutable trust pins | Existing production selection |
| D-4 provider/root policy | Proposed pair `(2, 3)`, `local-development-authenticode` | Existing pair `(1, 2)` and exact bytes |
| Publisher identifier | Exact 64 lowercase hexadecimal SHA-256 of APP leaf SPKI in the existing lp16 slot | Existing canonical full profile OID |
| Issuer policy | Pinned local CA certificate and exact role leaf certificate/SPKI | Existing Public Trust issuer policy |
| Windows trust | Successful retained-handle verification in the isolated test user | Existing full verification |
| Installation identity | Separate development application ID, repository ID, browser identity and protected namespace | Existing production identity |

The development publisher policy uses schema `keld.dev-publisher-policy/v1`.
Use these ordered fields: `schema`, `provider`, `role`, `leafSpkiSha256`, `issuerPolicySha256`.
The provider is `local-development-authenticode`. The role is `app` or `recovery`.
The development issuer policy uses schema `keld.dev-issuer-policy/v1`.
Use these ordered fields: `schema`, `chainPolicy`, `rootCertificateSha256`, `leafIssuerSpkiSha256`, `leafCertificateSha256`.
The chain policy is `windows-local-dev-pinned/v1`. Every digest is exactly 64 lowercase hexadecimal characters.
Pin the leaf SPKI and issuer policy independently for each role. The issuer is the pinned CA, not another candidate certificate.
Use the existing closed canonical JSON and bounded decoding rules. Do not assign a Microsoft profile OID to a local certificate.
The proposed publisher scope is SHA-256 of ASCII `keld.publisher.windows/dev/v2`, one zero byte, provider byte `2`, and the 32-byte APP SPKI hash.
The typed decoder accepts only the exact provider and root-policy pair. It retains the existing v2 domain and field order.
Production parses the lp16 field as `profileOid`. Development parses it as `appSpkiSha256`, never as an OID.
The ordinary identity names the APP signer. The recovery policy pins the distinct RECOVERY signer.
The recovery image `.keldea2` still names the destination APP SPKI. The signed `.keldrec` binds the protected recovery policy.

| Signed image role | Required development signer |
| --- | --- |
| Application Host, CLI, role launcher, updater helper | APP |
| Dedicated recovery Host | RECOVERY only |
| Final NSIS installer | APP |

Pin each final image independently in the authenticated build and catalogue records. Do not infer roles from filenames or a candidate certificate.

The trusted build policy fixes the profile, application ID, repository ID, and independent trust pins before signing.
Do not obtain those expected pins from a candidate image, mutable request, environment variable, or first-use observation.
An independent build receipt pins the completed signed image after signing.
The production build refuses provider 2. It does not embed development anchors.
The development build refuses provider 1 and production state.
Do not permit runtime switching or fallback through arguments, environment, registry, request files, or the legacy identity verifier.

Require Windows trust success on the original retained image, code-signing EKU, exact signer role, and one primary signature with no secondary signature.
The development verifier refuses the Microsoft Public Trust EKU. An authenticated operation selects the expected role independently of the image.
Keep production timestamp and revocation checks unchanged. Do not fall back to cache-only preflight as authority.
The native owner must determine the exact local certificate expiry and revocation policy before enabling development admission.
If a local CA has no revocation endpoint, review a separate pinned development policy. Do not silently ignore a trust failure.
The application and recovery leaf keys MUST differ. Reject a signer outside its exact role policy.
Use one local test CA and two distinct leaf certificates. Retain the keys only in an authorized signing account.
Trust the CA only in a dedicated medium-integrity ordinary test user's CurrentUser store. This restriction also applies in an isolated test VM.
Do not import a development CA into LocalMachine or another person's trust store.

The proposed development root is actual OS LocalAppData / GYLDLAB / KeldDev.
Derive `v2` and `acquisition-v1` below that root. Keep browser and provider identities separate from production.
The development namespace must not select production install roots, repository records, browser stores or provider bindings.
Use the existing owner APIs to derive those locations from the approved development identity and actual test SID.
Do not use arbitrary path arguments or a parallel writer. Test real permanent CLI/provider binding within that SID.

## Remaining implementation and acceptance

1. Obtain exact-version review of the development wire, identity, permission, and owner delta. Keep production vectors byte-identical.
2. Generate and independently pin local CA/APP/RECOVERY identities for one development test cohort. Do not regenerate the keys per artifact.
3. Use the existing TUF owner with independently anchored signed root, timestamp, snapshot and targets. Preserve expiry, clock, replay and durable floors.
4. Complete T82 and D-42/D-45/D-90/D-46 on retained native objects. Keep D-64/D-68 journals, lease ordering, the 120-second bound, and one-use permit.
5. Embed identity before signing. Sign the actual Host, required helper images and NSIS installer. Verify the final image hashes.
6. Install on the isolated ordinary SID. Verify the installed Host, Bun, WebView2, permanent CLI/provider binding and no-flag launch.
7. Exercise all ten draw.io workflows, interruption recovery, refusal controls and cleanup.

Production-only commissioning and public release evidence are separate future gates. B82 trusted-base CI and native owner reviews remain required.
No development result qualifies public distribution.
The loss-only recovery distribution ceremony is a separate support cell. APP signing must still never grant RECOVERY authority.

## Independent falsifiers

- Production rejects every development image and policy, even when Windows trusts its root. Development rejects production state and identity.
- Mutate the provider, signer, CA pin, app ID or repository ID independently. Refuse before protected effects.
- Present an APP signer to RECOVERY and the reverse. Present a third leaf from the same CA. Refuse even with a valid chain.
- Supply an unsigned, modified, multi-signed or expired image. Refuse before admission. Verify every required helper and installer role independently.
- Replace the signed catalogue, replay old metadata, or expire a role. Refuse without advancing protected history.
- Substitute the native worker, retained object, lease or journal attempt. Refuse before an installation effect.
- Exercise real successful installation and cleanup on the ordinary SID. A signing probe is not this result.

## Observed signing mechanics

On Windows, the SDK SignTool signed two existing debug Host copies with distinct disposable self-signed certificates.
Both embedded CMS signatures verified, and both modified PE copies returned native `HashMismatch`.
An independent local CA experiment also signed two distinct code-signing leaves and verified both chains against the pinned CA file.
The local certificates lack the Microsoft Public Trust EKU. Windows correctly refused their unprovisioned trust chains.
The original binaries remained unchanged. The temporary private keys were deleted.
CurrentUser and LocalMachine Root and TrustedPublisher inventories remained unchanged.
The observations prove signing mechanics only. They do not qualify the v2 Host, trust policy, installation, or recovery authority.

The KEL-270 decision thread records the exact local receipt locations, image and certificate hashes, signer SPKI pins, and native command results.

The current execution token cannot create the required ordinary test user. The owner must provision that test environment before the positive installed run.
No purchase or Azure account is required.
