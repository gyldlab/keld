# W1 decision packet: source authority and integration

Status: **PROPOSED owner decisions; no production admission or boundary change.** This packet accompanies the bounded feasibility fixtures. It neither replaces nor approves [W1 r17][w1]. The owning specification and its exact acceptance criteria remain authoritative.

- Task branch: `agent/gh-1184-w1-feasibility-20261105`.
- Base and inspected main: `df99602ef10abd2cd6f3b5c633ba6d1cffee97bc`.
- Governing W1 draft: PR1307, r17, `20a798976d03e76056c5edf4c0d6fb5775c5659c`; live `OPEN / REVIEW_REQUIRED`, GitHub draft flag false. The specification remains draft.
- Shared-head refresh: `2026-11-04T23:45:00Z`–`23:45:08Z`, from GitHub on Ramani. Re-read before adoption.
- Native feasibility, B7 forcing, independent review and product acceptance: **pending; this packet records no result**. A fixture/probe pass does not approve Route D.

## Current owner pins

All are open. `draft` below is GitHub's flag, not architectural approval. Prior source-contract approval on a subset does not approve Windows composition.

| Contract | PR | Exact head | Review decision | draft |
|---|---|---|---|---|
| GH552 exact grants | [1191](https://github.com/gyldlab/keld/pull/1191) | `a73f301cfe7027ad3e1bd700800e9abe60c66bc7` | REVIEW_REQUIRED | false |
| GH542 document identity/generation | [1192](https://github.com/gyldlab/keld/pull/1192) | `c889a0f054b89de9b838a1a3451238cf638f86e2` | CHANGES_REQUESTED | true |
| GH467 facade | [1154](https://github.com/gyldlab/keld/pull/1154) | `b8b15392cce51fbf848f5a47841250100df7535f` | CHANGES_REQUESTED | false |
| GH544 value codec | [1190](https://github.com/gyldlab/keld/pull/1190) | `0437c9104077e6a52c9128e4367e91c57937b22a` | CHANGES_REQUESTED | true |
| GH484 preload/source | [1155](https://github.com/gyldlab/keld/pull/1155) | `0ad15d8df9897620bcad9c923b9b3f6aa224a982` | CHANGES_REQUESTED | false |
| GH540 transport | [1122](https://github.com/gyldlab/keld/pull/1122) | `16b00a2e388449c4525a30913937bc2f6e11c472` | CHANGES_REQUESTED | true |
| GH541 manifest/navigation | [1123](https://github.com/gyldlab/keld/pull/1123) | `3b879e40ff33aaf1b27ef49153f5e97aeb50cc68` | REVIEW_REQUIRED | true |
| GH497 migration output | [1172](https://github.com/gyldlab/keld/pull/1172) | `b7816c4c2ba2a53c4800904601b7e2b5a703a34c` | REVIEW_REQUIRED | true |
| GH499 draw.io migration | [1170](https://github.com/gyldlab/keld/pull/1170) | `1fbfe3a6a4fccd2f6f661de47efb683a9513baf0` | REVIEW_REQUIRED | true |
| W3 file/resource/navigation | [1311](https://github.com/gyldlab/keld/pull/1311) | `aa00b50ecafa7454bc950f7f8487cc28a68df186` | CHANGES_REQUESTED | true |

No qualified shared preload/facade implementation was found in these relevant Ramani refs. Main's [BrowserWindow implementation](../../../../../packages/@keld/electron/src/browser-window.ts) still [refuses preload execution][facade]. GH484's [latest handoff][latest484] describes unpushed local Mac source-contract work at `22f845714515f337c3d51b82c04335afa7d6f827`, no integrated runtime candidate and no original-app acceptance. That is neither a reusable qualified dependency nor Windows proof.

## T0 decisions still requiring owner evidence

This is a checklist of r17 §6/§10, not revised semantics. [The r16 review disposition][review] expressly approves no PR, grant enablement or merge. B2's three proposed tracker passages were already reconciled to r17; repeating that metadata work cannot close the remaining gates.

| Gate | Exact missing record or proof | Owner and first completion check |
|---|---|---|
| B1 | Independent KEL142 §5.1 Windows security amendment for a shared-world bridge and page-readable document nonce. Q1(c) is not this approval. | KEL142 security owner: exact-content sign-off on PR1307; `contextIsolated=false`, no private Host object or foreign live epoch exposed. |
| B2 | Metadata reconciliation is complete; its proposed text becomes effective only with exact-content approval. | GH1184 owner: retain the existing reconciled passages and explicit conditional status. No further metadata rewrite requested. |
| B3 | Named Windows application-document oracle based on verified resource identity; decision on allowed non-entry in-root documents. | GH542/PR1192 with W3: record oracle and eligibility choice. URL text alone does not qualify a document. |
| B4 | Named Windows source-trust owner and design; no existing admitted source carrier meets the inherited requirement. | GH1182/GH484 appoint the owner with KEL102/package owners; record one carrier and interface below. Implementation is a later gate. |
| B5 | Approvals for every reused contract and compatible composition. Open/draft state or an empty review-thread list is insufficient. | Each shared owner above: exact-content approval record; refresh heads and dependencies at approval. |
| B6 | Exact loading path (`file:` under verified root or owned virtual host), resource identity and navigation allow forms. | W3/GH542/KEL102/GH541: record choice; W3 owns any parser/identity function. Q22(a,b) and Q27 remain later device gates, not backwards T0 dependencies. |
| B7 | A discriminating admission probe, not an empty-risk observation. | W1: r17 Arm F needs a nonempty late-A set with full-rule denial and weaker-rule admissions; Arm H separates pre/post-navigation subsets and the stalled-navigation variant. Native results remain pending. Empty sets mean rule not exercised. |
| B8 | Warning-free completed OCR review at the then-current exact head, reconciled with review threads. | W1/review owner: record job, head and successful terminal status. An older head's completed review does not satisfy it. |
| B9 | GH540 record of selected bounded re-answer branch (ii), including readiness/announcement and typed-failure residuals. | GH540: accept/correct r17 sequence and two-post budget. Actual AC29(i–k) proof is T2, not T0; no new bind field or reminting is implied. |
| B10 | Owner record of remaining closed-output decisions, entry source, supported static syntax/root agreement and re-map rule. | GH497 with GH541, GH484/467, resource/boot owners: record each output row and its reader. T5 stays blocked; AC43–46 are later tests. No config execution or generated key is admitted here. |
| B11 | Approved carrier and evaluation rule for independently reviewed computed-channel literals. | GH541 schema, GH552 evaluator, GH497 NOT WRITTEN row: record answer on PR1307. Current rule remains grants subset of literal reachable sinks; attestation cannot add channels. T6 workflow qualification remains separate. |

T0 additionally needs the owner records for **Q2** (creation-site/manifest grammar, `Create.site`, immutable row and adoption), **Q14** (shared Renderer/Eval ticket and two short fence checks around Guard), **Q17** (kind/state response matrix, quiescence charges and staging), and **Q18** (page-side suppression as an explicit availability limit). GH541/GH552/GH531/W3/GH497/KEL19 own Q2; GH542/GH540 own Q14; GH540/GH1095 own Q17; GH467/GH484/KEL142 own Q18. The r16 targets are proposals pending those records.

Q1's unchanged CompatV2 carriage decision is resolved, but not an implementation approval. Q15's single KEL19 input census and Q16's registry-owned codes/generated Rust mapping remain later T5/T4 gates. This packet allocates no error code.

## B4: evidence and alternatives

**Known:** an integrity hash proves equality to an expected value. It does not authenticate who supplied that value. The [GH484 source draft][preload-spec] AC07/08 requires an independently authenticated expected digest outside the same-user-writable package tree, one bounded read and immutable retained bytes. That Windows carrier is absent.

| Existing owner/source | Verified boundary and missing capability |
|---|---|
| [Core boot](../../../../../crates/keld-core/src/app_session.rs), [pinned source][boot] | `ValidatedBootSelection` holds the app selection, opened permissions file and permissions digest. `validate_windows_boot_files` opens fixed boot targets; there is no preload/main inventory or admitted source image. |
| [Boot schema](../../../../../crates/keld-core/src/app_session/boot_contract.rs), [pin][boot-schema] | Closed schemas 1/2 carry `permissions.content_sha256`; no source-catalog key. A new key needs schema/security approval. |
| [Guard verified loader](../../../../../crates/keld-guard/src/verified_manifest.rs), [pin][verified] | One-handle bounded hash-before-parse reuses the exact buffer. Its caller supplies the expected policy hash; it is not a source trust root. |
| [Installed selection](../../../../../crates/keld-update/src/windows_baseline.rs), [pin][selection] | `ActivePackageSelection` pins tree/ancestry and expressly does not authenticate boot files. Ordinary `select_committed_package` checks completion metadata/policy, not the full member-byte verifier. |
| [Embedded expectation](../../../../../crates/keld-update/src/provenance.rs), [pin][expectation] | `ExpectedAppIdentity` binds app/channel/target/signing-key ID, not a current source/content digest. Baseline/profile data comes from the protected record. Per-user same-user mutation is outside that boundary. |
| [Updater member verifier](../../../../../crates/keld-update/src/windows_baseline/load/version_contents.rs), [pin][archive] | Existing private `validate_version_contents` compares authenticated archive members with pinned files byte-for-byte. Writer/recovery `pin_versions` uses it; ordinary source admission has no public snapshot API. Reuse this owner. |
| [Dev staging](../../../../../crates/keld-cli/src/boot.rs), [pin][devstage] | Stages entry, optional renderer and transport, with Host-copy checks. No complete app/preload census; Windows uses fixed permissions bytes. It cannot be presented as the missing externally anchored source contract. |

| Candidate, all unapproved | Consequence |
|---|---|
| **Recommended:** read-only selected-artifact/source snapshot through existing package/signature/archive owners. | Authenticate the exact selected app/channel/target/version/key and archive bytes outside the mutable tree, then derive immutable admitted members. Existing `verify_manifest` selects updates above a floor; a current-boot API and retention contract need owner review, not bypassing that selection rule. |
| Embed a finite source-catalog digest in an independently signed Host. | Direct external anchor, but requires a `keld-pack`/signed-identity extension. Current `ExpectedAppIdentity` contains no such digest. |
| Owner-private stage plus adjacent hashes only. | Useful development/probe boundary. Does not meet inherited AC08 against same-user substitution; production use would require an explicit security-boundary amendment. |

**Requested source decision:** name one owner and select one carrier; specify its private construction authority, same retained root/handle binding, exact byte/aggregate limits, lifetime and registered failure categories. W1 should consume an opaque immutable result containing admitted preload bytes/digest and creation-file identity. A public arbitrary `(bytes, expectedHash)` constructor would not establish trust. This describes the requested boundary, not a new Rust API.

First source-owner falsifiers: absent external anchor despite matching adjacent hashes; changed source alone; source plus adjacent-manifest substitution; wrong selected identity; in-place/retained-handle race; byte-identical outside-root/reparse/root substitution; size/capacity +1. Each refuses before executing preload bytes. Navigation reuses the snapshot and document state is renewed through the existing owner. No new parser or source registry is proposed.

## Digest and main-source limits

**Known r17 contract:** preload and creation-file digests hash exact bytes. `mainDigest` covers KEL19's read-only pre-bundler input census plus every declared readable preload, excluding only `keld.permissions.jsonc`. Migration and release recompute share one function; release uses the private staged inputs actually read by the bundler. No whole-tree/VCS hash or new boot-time main-census hashing is proposed.

**Unresolved exact bytes:** r17 §4.8 says sorted UTF-8-byte-order lines of relative `/` path and file SHA-256. It does not define delimiter, raw/hex digest, hex case/prefix, newline/final newline, duplicate/conflicting paths or serialization version. KEL19 and GH541/GH552 must approve exact encoding and golden vectors before a fixture algorithm becomes canonical. Fixture lists and serialization remain explicitly experimental.

**Main-source drift limit:** r17 §4.8 at lines 1717–1724 says the Host hashes the file on disk, not the bytes Bun evaluated. External/linked source maps and stale bundles can name a reviewed source while executing other output. Matching a creation-file digest or paired attestation is not proof of actual main execution. KEL19/boot owners must define the build-input-to-runtime binding; retain this residual in the receipt.

Draw.io's unchanged preload alone cannot qualify the product: r17's literal-only generated grant is `el:rendererReq`; the other 23 channels need B11's approved review path. The main-window site through `Object.assign`/a variable remains UNRESOLVED under r17's proposed static grammar. No broader scanner, grant, filesystem scope or dialog permission is introduced here.

## Narrow implementation handoff after owner decisions

| Owned slice / existing symbols | Proposed handoff and acceptance boundary |
|---|---|
| Source owner + Core: `ValidatedBootSelection`, private `AppBootSelection`, `validate_windows_boot_files`, `validate_installed_from_verified` | Retain the approved authority and immutable source result before window resources. Exact constructor and error contract first; extend existing owner modules only. No edits to active `app_session.rs` by W1. |
| KEL19/CLI: `stage_dev_boot_platform_with_owner_check` and approved input census | Reuse one delivered-input list for staging, migration and release recompute. Resolve emitted preload bytes and per-file census mapping. No second recursive walker. |
| T2a: GH531 `WindowRequest::Create`, GH541 row lookup, W3 opener | Owner-approved bounded `site` field; one `preload_rel`/component rule; per-file and row-preload digest checks complete before controller/script/navigation. Unknown, duplicate or mismatching site binds no row and no grant. AC13/14 are the first checks. |
| T2b: W3/GH542/GH540 shared admission fence/ticket | Extend the existing approved fence once, only if W3 has not already landed it. Q14 records Renderer/Eval work and Guard placement; AC33 independent zero-write races. No second epoch counter or transport writer. |
| W2: existing `AppWindowCommand::CreateWindow`, deferred-create and per-build/window state | Agree exact owning fields/hunks for the immutable source and row. Current `Copy` command cannot silently carry owning bytes; W2 chooses the lifecycle change. No second registry. W1 leaves `webview2/mod.rs`, `app_session.rs` and Host `main.rs` to the lead. |
| T2c/T3: existing Windows bridge plus GH540 decoder/ledger and GH552 guard | Consume W3's sole file parser, resource identity, source attribution and navigation eligibility, coordinated with GH542's document generation and GH540's role retirement through one fence/epoch authority. Reuse canonical CompatV2, exact channel evaluator and revocation. Current main lacks this Windows route-D implementation. |
| T4/T5/T6: GH467/484 facade, GH544 codec, migration and original application | Reuse qualified shared facade code when supplied. Honest `contextIsolated=false`; approved filtered/omitted/transformed-input classifier and paired binding; exact grants only. First-inline execution, effects, recovery and original draw.io workflows remain independent gates. |

Staging request to GH540: record whether T2/T3 infrastructure with **zero enabled Route-D grants** may merge after its own AC33/35/37/38 controls, with AC40–42 gating T4/facade release, or retain the stricter stacked T2/T3/T4 gate. The draft recommends the former but has no owner record; this packet changes neither rule nor refs.

**Evidence still required:** isolated one-fault wrong-digest, ungranted-channel, missing-epoch, stale Host-to-renderer and omitted-argument classifier controls, each producing its forbidden Bun-handler/page-delivery effect in the mutant and zero in the baseline; then healthy native recovery. Count actual handler entry/channel/arguments and raw page receipt, independently of refusal logs. Natural B7 race attribution, synthetic injection, native probe, component test and product acceptance stay separately labeled.

**Exact review requests:** (1) GH1182/GH484 appoint B4 owner and return the selected carrier/interface contract; (2) KEL142 approve or reject B1 independently; (3) GH541/GH552/GH531/W3/KEL19 record Q2, B3/B6, shared fence Q14 and exact digest schema; (4) GH540 record B9, Q17 response matrix and staging; (5) GH467/GH484 record Q18; (6) GH497/GH541/GH552 record B10/B11. Record decisions on the existing owning issues/PR1307, bound to exact content. Keep PRs open; no self-approval, main merge or automatic grant enablement follows.

[w1]: https://github.com/gyldlab/keld/blob/20a798976d03e76056c5edf4c0d6fb5775c5659c/docs/specs/gh1184-windows-preload-facade.md
[review]: https://github.com/gyldlab/keld/pull/1307#issuecomment-6546139287
[latest484]: https://github.com/gyldlab/keld/issues/484#issuecomment-6560285327
[preload-spec]: https://github.com/gyldlab/keld/blob/0ad15d8df9897620bcad9c923b9b3f6aa224a982/docs/specs/gh484-macos-app-preload.md
[boot]: https://github.com/gyldlab/keld/blob/df99602ef10abd2cd6f3b5c633ba6d1cffee97bc/crates/keld-core/src/app_session.rs#L385
[boot-schema]: https://github.com/gyldlab/keld/blob/df99602ef10abd2cd6f3b5c633ba6d1cffee97bc/crates/keld-core/src/app_session/boot_contract.rs
[verified]: https://github.com/gyldlab/keld/blob/df99602ef10abd2cd6f3b5c633ba6d1cffee97bc/crates/keld-guard/src/verified_manifest.rs#L43
[selection]: https://github.com/gyldlab/keld/blob/df99602ef10abd2cd6f3b5c633ba6d1cffee97bc/crates/keld-update/src/windows_baseline.rs#L191
[expectation]: https://github.com/gyldlab/keld/blob/df99602ef10abd2cd6f3b5c633ba6d1cffee97bc/crates/keld-update/src/provenance.rs#L398
[archive]: https://github.com/gyldlab/keld/blob/df99602ef10abd2cd6f3b5c633ba6d1cffee97bc/crates/keld-update/src/windows_baseline/load/version_contents.rs#L15
[devstage]: https://github.com/gyldlab/keld/blob/df99602ef10abd2cd6f3b5c633ba6d1cffee97bc/crates/keld-cli/src/boot.rs#L357
[facade]: https://github.com/gyldlab/keld/blob/df99602ef10abd2cd6f3b5c633ba6d1cffee97bc/packages/@keld/electron/src/browser-window.ts#L198
