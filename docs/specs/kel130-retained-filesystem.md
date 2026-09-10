# Spec: retained filesystem resources and bounded operations
Status: approved
Linear: KEL-130 · Owner: GYLDLAB · Updated: 2026-09-10
Decision state: approved; final 869 review chain passed; metadata freeze awaiting publication and landing
Authoring base: `bf78a10db350ba58830efc60a80eeb67fc996ebd`
Approval continuity: direct user approval in Linear comment
`0a5d62c4-12f5-4e63-867d-a52c222ed6d4` bound the original
`c565d143d9956657bf61c3a84da1919dfc425f350bc9e69979a0cc90f24cdf8e` payload and
`b3072e71baf7056e9a37f6acca2351c4d45aa6bc749c4eb4f7383d9cf8f21014` spec. The
current user instruction authorized the formal filesystem and cross-platform review
fixes within that folder-boundary intent. The later explicit user text
`then merge what is stoping you` authorizes publication and merge of this reviewed
T0 contract. This continuity record does not claim the user typed or separately
authenticated the revised digest, does not mark T0 landed, and does not authorize T1.
Correction provenance: Linear assignment
`b5cbd108-7a6a-42d1-818d-c2b8259bc7e5`; formal filesystem-security review
`01a0878c-2c0b-7f33-a765-0fbb1469e9d5` / SHA-256 `96e100c570cc52f864c6d6eb25c2ffa0f81806532e5a57ff342d02cc65fca136`; independent
root source and compiler refutation; formal cross-platform API review
`01a0878c-2bf1-7a53-8f64-2c1e0c69cece` / SHA-256 `82598a31fa735cfe39276e16af2f6ac83d2a3986d73b867f7f3ccb7db9d4acb8`; Linear
assignment `809b02eb-61d9-4886-94f0-c240b1eaedfb`; final f794 review artifacts:
filesystem `ea8bb81a24d400ef9da2981f1285d9d8849ce99ae1a8a38cff82a66966822aca`,
cross-platform API `98f03dd764df34871b6c4aa950c557a0b1c5581bfe205eb21efe2887b63c870a`,
and evidence oracle `947294cccb1b2b6d03b137cdf2d7d193e02bcd9725b4762990497eca56606fef`;
independent refutations `0fa66df525a64939129956e3174ab7174aa66b66c61f0881ddd5ecced1399fb1`,
`4b3b7929ef1c0db6100b3db1069a9074a863a596c9c740ba96b37fb9b99628ad`,
and `643e71b7c0e4b8c6749f2b8ec3966efd9f06741fb59cfff99ef995a4c1bebcd9`;
mode-stripping counterexample `mode-stripping-probe.json` / SHA-256 `089a4cade44541bb80486f6f9b150b555d839e4fefe0bbfc9eb6ca9a4c8b4914` and Linux `truncate(2)`.
Final 869 zero-finding review chain: filesystem security thread
`01a08832-9ae3-7a62-ae42-80a6b6598d3f` / artifact SHA-256
`a59aeaa7e7efa6db1f9f7d1275a3fcd9e14ecbec77ee0e8c27d166453cec8722`;
cross-platform API thread `01a08832-9af6-7a01-b967-e92613dd0191` / artifact
SHA-256 `10a194a20af7e9680d782019e109262dccbf26b42d103e1b01d6c1b8e81a380c`;
evidence oracle thread `01a08833-e0bb-7322-adf4-07f991cb05f6` / artifact SHA-256
`60b3a2abe4636c1bde96cfa5c033dad3fcdcab2486b3a76530f8f5b1bc37a00c`.
Root closeout evidence: review disposition SHA-256
`5a91a9c813a48bff9154a4f0aafcdb93696dc749a28dd4096f82c344a13a41ff`, source
binding SHA-256 `61aa765092f945ebe19cc5a0a714e2b34645f642ce07b4bab94bb69f6eb9d7a4`,
and validation SHA-256
`2fbc4d3d6ade75e4a271b3840afd71560ed5085eb4ed2da3928ed358f6599f31`.
Supersedes: payload `c565d143d9956657bf61c3a84da1919dfc425f350bc9e69979a0cc90f24cdf8e`, spec `b3072e71baf7056e9a37f6acca2351c4d45aa6bc749c4eb4f7383d9cf8f21014`, and local
commit `19ebf12a49f7d63c44610cdfec7dde37ef052f52`.
Decision payload SHA-256: `8738b7bb326cac5fb4fa99c635e8c2ac5dd4aa6048322c5e1c56607f71cf54ae`

Canonical decision payload (the digest covers this one minified UTF-8 line without
the code fence or trailing newline):

```json
{"schema":"keld.kel130-retained-filesystem-decisions/v1","issue_id":"KEL-130","task_id":"KEL-130/T0","status":"approved","authoring_base":"bf78a10db350ba58830efc60a80eeb67fc996ebd","approval_continuity":"original-direct-approval-linear-comment-0a5d62c4-bound-payload-c565d143-and-spec-b3072e71;current-user-instruction-authorizes-executing-formal-filesystem-and-cross-platform-review-fixes-within-approved-folder-boundary-intent;no-claim-user-typed-revised-digest;current-user-explicit-publication-and-merge-authorization-exact-text:then merge what is stoping you;authorization-applies-to-reviewed-KEL-130-T0-not-T1-implementation","supersedes":"payload-c565d143d9956657bf61c3a84da1919dfc425f350bc9e69979a0cc90f24cdf8e;spec-b3072e71baf7056e9a37f6acca2351c4d45aa6bc749c4eb4f7383d9cf8f21014;commit-19ebf12a49f7d63c44610cdfec7dde37ef052f52","correction_provenance":"linear-assignment-b5cbd108-7a6a-42d1-818d-c2b8259bc7e5;filesystem-security-review-thread-01a0878c-2c0b-7f33-a765-0fbb1469e9d5-sha256-96e100c570cc52f864c6d6eb25c2ffa0f81806532e5a57ff342d02cc65fca136;cross-platform-api-review-thread-01a0878c-2bf1-7a53-8f64-2c1e0c69cece-sha256-82598a31fa735cfe39276e16af2f6ac83d2a3986d73b867f7f3ccb7db9d4acb8;linear-assignment-809b02eb-61d9-4886-94f0-c240b1eaedfb;independent-root-source-and-compiler-refutation;mode-stripping-probe-sha256-089a4cade44541bb80486f6f9b150b555d839e4fefe0bbfc9eb6ca9a4c8b4914;linux-truncate-primary-contract;f794-filesystem-review-sha256-ea8bb81a24d400ef9da2981f1285d9d8849ce99ae1a8a38cff82a66966822aca;f794-cross-api-review-sha256-98f03dd764df34871b6c4aa950c557a0b1c5581bfe205eb21efe2887b63c870a;f794-evidence-review-sha256-947294cccb1b2b6d03b137cdf2d7d193e02bcd9725b4762990497eca56606fef;independent-root-refutation-all-distinct-findings;f794-security-refutation-sha256-0fa66df525a64939129956e3174ab7174aa66b66c61f0881ddd5ecced1399fb1;f794-api-refutation-sha256-4b3b7929ef1c0db6100b3db1069a9074a863a596c9c740ba96b37fb9b99628ad;f794-evidence-refutation-sha256-643e71b7c0e4b8c6749f2b8ec3966efd9f06741fb59cfff99ef995a4c1bebcd9;c000-cross-api-review-sha256-be2c4af712dc3dfa4475a9c0650031019bd0be0dfc912f8472c3c1d5b68bed78;c000-evidence-review-sha256-d033768546a88bab7fb8d9323e046d061636af3cddf9647262b81707f53d35a7;snapshot-oracle-independent-root-refutation;cdb-fs-review-sha256-afade63b8914bcf7c08f69ecdcbfc78fb6cf598f68d55bc9a67a9cf0bb0e9547;cdb-api-review-sha256-e41c5842020ae964f0df621d3d2b7dcf3aba5ab355b6acbb607709650f20735c;cdb-eo-review-sha256-6deef9fefd7efaf7c6aefb12ce07c2bfc010a0d1f01992d3dbbf5adf27382c6a;final869-filesystem-security-review-thread-01a08832-9ae3-7a62-ae42-80a6b6598d3f-sha256-a59aeaa7e7efa6db1f9f7d1275a3fcd9e14ecbec77ee0e8c27d166453cec8722-zero-findings;final869-cross-platform-api-review-thread-01a08832-9af6-7a01-b967-e92613dd0191-sha256-10a194a20af7e9680d782019e109262dccbf26b42d103e1b01d6c1b8e81a380c-zero-findings;final869-evidence-oracle-review-thread-01a08833-e0bb-7322-adf4-07f991cb05f6-sha256-60b3a2abe4636c1bde96cfa5c033dad3fcdcab2486b3a76530f8f5b1bc37a00c-zero-findings;root-final-review-disposition-sha256-5a91a9c813a48bff9154a4f0aafcdb93696dc749a28dd4096f82c344a13a41ff;root-final-source-binding-sha256-61aa765092f945ebe19cc5a0a714e2b34645f642ce07b4bab94bb69f6eb9d7a4;root-final-validation-sha256-2fbc4d3d6ade75e4a271b3840afd71560ed5085eb4ed2da3928ed358f6599f31","ownership":"host-session:keld-native-FsBroker-sole-value-retains-scope-roots-and-per-call-files;coordinator-owns-value;operation-and-session-views-borrow-only;actual-FsBroker-destructor+zero-owner-handle-census-defines-drop;keld-guard-matches;keld-ipc-dispatches;KEL-102/T3-integrates","scope":"absolute-utf8-literal-only;exact-or-terminal-glob;max-4096-path-bytes;max-64-per-capability;no-cwd-or-unexpanded-vars","scope_anchor":"one-ambient-open-selects-explicit-root-object;later-operations-use-retained-handle","exact_file":"retained-parent+leaf-slot;per-call-no-follow-regular-leaf;absent-may-be-created;final-symlink-or-reparse-denied;ordinary-slot-replacement-and-hardlink-object-rule-preserved","links":"subtree-only-internal-relative-symlinks-allowed-when-resolution-stays-beneath-retained-root;exact-final-links-denied;external-symlink-junction-unknown-reparse-and-mount-crossing-denied;all-platform-explicit-bounded-component-walker;linux-per-component-openat2-no-follow;windows-target-utf8-native-components-only;CurDir-noop-counted;Prefix+RootDir+drive-relative+rooted+ADS+reserved+device+nonutf8-denied-by-shared-component-rules-before-target-open","hardlinks":"object-authority:in-scope-hardlink-authorizes-shared-object;all-aliases-observe-in-place-write;no-racy-link-count-isolation-claim","io":"regular-files-only;max-content-8388608;chunk-65536;max-total-post-link-components-256;max-links-40;read-limit-plus-one;write-open-no-truncate-then-same-handle-commit","write":"preserve-inode-owner-ordinary-acl-dacl-xattrs-hardlinks-under-os-inplace-semantics;permit-os-clearing-privilege-bits-and-security-xattrs;never-restore-stripped-privilege-metadata;new-file-success-is-commit;AlreadyExists-is-noeffect-002;no-atomic-replace-or-auto-retry;post-commit-failure-is-KELD-NATIVE-007-effect-may-have-occurred;committed-bytes=sum-successful-content-write-return-lengths;excludes-create+truncate;no-durability+final-length+unknown-error-estimate","deadline":"five-second-absolute-post-Allow-cooperative-budget;no-renewal;no-hard-per-kernel-call-preemption-claim","cancellation":"observed-at-precommit-progress-check-no-write-effect;read-partials-discarded;post-commit-effect-may-have-occurred;terminal-close-drop-only-after-broker-progress-resumes;wedged-synchronous-syscall-and-T3-wait-unbounded;KEL-133-outer-expiry-remains-IPC006;post-return-effect-state-first;progress-cause-cancellation-before-deadline-before-returned-stage-error","errors":"008-prepare-or-snapshot;004-request-shape;GUARD-deny;002-escape-race;003-object;005-006-precommit;007-postcommit;001-other-preeffect","api":"Decision-Allow-carries-private-ScopePermit;dispatch_privileged-borrows-permit-for-callback;callback-result-independent;no-public-broker-permit-or-index-input;opaque-nonclone-FsBroker-replaces-bare-path-free-functions;serve-session-requires-broker+verified+cancel;narrow-KEL102-D5-shape-amendment","dependency":"cap-std+cap-fs-ext=4.0.3@5cae39826c70e7da89cc821b825885e030d38f93+workspace-rustix=1.1.4;dependency-security-msrv-transitive-review-required","review_gates":"unsafe:none;public-api:required;permission-model:required;dependency:required;wire-protocol:none","review_status":"final869-filesystem-security-thread-01a08832-9ae3-7a62-ae42-80a6b6598d3f-artifact-sha256-a59aeaa7e7efa6db1f9f7d1275a3fcd9e14ecbec77ee0e8c27d166453cec8722-zero-findings;final869-cross-platform-api-thread-01a08832-9af6-7a01-b967-e92613dd0191-artifact-sha256-10a194a20af7e9680d782019e109262dccbf26b42d103e1b01d6c1b8e81a380c-zero-findings;final869-evidence-oracle-thread-01a08833-e0bb-7322-adf4-07f991cb05f6-artifact-sha256-60b3a2abe4636c1bde96cfa5c033dad3fcdcab2486b3a76530f8f5b1bc37a00c-zero-findings;approved-local-T0-awaiting-publication-and-landing;T1-not-started","task_order":"KEL-130/T0-corrected-rereview-and-artifact->KEL-130/T1a-tests-and-gates->T1b-broker->T1c-real-three-OS->T1d-one-landed-artifact->KEL-102/T3->KEL-140","acceptance":"deterministic-CI-state-and-mutation-controls+exact-alias-denial+first-match-scope-order+borrowed-permit-escape-compilefail+conditional-cancel-drop+post-link-component-256-pass-257-deny+separate-real-macOS-Windows-Linux-rows;fresh-allowed-operation-after-every-completed-hostile-case;privilege-metadata-strip-no-restore-negative-control;windows-link-target-grammar;partial-prepare-unwind;cancel-before-deadline-and-stage-error-order;traced-successful-write-counts;per-OS-owner-handle-census;wrapper-retention-blocks-quiescence;exact-T1-artifact-validator-negative-fixtures;same-digest-distinct-object-snapshot-positive+different-digest-same-path-008-before-guard;every-native-row-source-head-equals-T1-landed-head;same-decoded-4097-byte-path-samedigest-004-vs-differentdigest-008;move-digest-check-after-validation-mutation-fails","successors":"T0-or-partial-T1-never-unblocks-shipping;only-exact-passed-landed-KEL-130/T1-artifact-may-precede-KEL-102/T3","artifact_contract":"schema-keld.execution-artifact/v1;node-retained-filesystem;issue-KEL-130;task-KEL-130/T1;status-passed;exact-landed-T0-contract-head+blob+decision-digest;T1-landed-head-descendant-and-current-main-ancestor;T1a+T1b+T1c+T1d-passed;distinct-native-macOS+Windows+Linux-provenance-rows;authenticated-repository-owner-or-standing-delegate-publisher;evidence-byte-digests;T1a-owned-validator-rejects-each-substitution;each-native-row-source-head-exactly-equals-T1-landed-head;stale-ancestor+mixed-head-negative-fixtures"}
```

## 1. Goal & non-goals

Replace the live `keld-native` filesystem broker's authorize-a-string-then-reopen
shape with one host-owned resource boundary. The host prepares retained scope root or
parent directory capabilities from the verified immutable manifest, the existing guard
returns the one matched scope with its Allow decision, and `keld-native` resolves and
acts relative to that retained resource. Reads and writes accept only regular files,
use fixed request/content/progress bounds, and observe one non-renewable operation
budget and cancellation state. A symlink, reparse, mount, or rename traversal cannot
redirect the operation outside the retained root; hard-link aliases follow the explicit
shared-object rule below. This changes handle ownership and is therefore an
architecture change; the implementation must update the current-state architecture in
the same atomic T1 change.

Non-goals:

- no shipping registration or `keld-core`/`keld-host` route; KEL-102/T3 owns that
  later integration and remains blocked on the exact passed KEL-130/T1 artifact;
- no renderer, `@keld/api`, Electron facade, dialog grant, watcher, stream, bulk lane,
  manifest generator, `$VAR` expansion, role grant, LPAC, sandbox, or product-spine
  work;
- no second guard matcher, `canonicalize`-check-path-reopen sequence, blanket rejection
  of every symlink, or retry of a failed filesystem operation;
- no claim that an ordinary synchronous regular-file syscall can be forcibly stopped
  at a wall-clock instant on every supported filesystem;
- no durability or atomic-replacement promise: successful `fs.write` has the current
  create-or-truncate contents contract and does not imply `fsync`.

## 2. Spec refs

- `docs/architecture/01-overview.md` §§1–4: the Rust host owns privileged handles;
  application principals receive ids and results, never reusable OS handles.
- `docs/architecture/02-ipc.md` §§2 and 7: `CallError`, the 16 MiB control-frame cap,
  KEL-133 receiver validation, and the distinction between admission clocks and
  post-admission filesystem work.
- `docs/architecture/03-security.md` §§1–4: host-minted principals, one default-deny
  matcher, guard-before-handler ordering, and the current literal-scope limitation.
- `docs/architecture/05-webview-and-native.md` §3: `keld-native` owns the guarded
  cross-platform filesystem broker.
- `docs/architecture/06-runtime-and-tooling.md` §1: session teardown and child handle
  inheritance rules.
- `docs/specs/kel102-host-guard-enforcement.md` D5, §§4 and 6: one
  `dispatch_privileged` boundary, immutable verified manifest, and KEL-102/T3 as the
  later shipping route/in-flight coordinator.
- `docs/specs/kel133-kipc-receiver-semantics.md` §§4 and 6: KEL-133 owns validated
  frames and transport/frame/session/call clocks; filesystem completion begins only
  after valid admission and guard Allow.
- Linear KEL-130 comments `4de93ebc-0e9c-4622-a83b-4c1c801aae47` and
  `7deffd67-a1cc-4813-94ac-4d131caca2eb`: approved owner partition and order
  KEL-133/T0 → KEL-133/T1 → KEL-130/T0 → KEL-130/T1.
- Windows research
  `artifacts/windows-isolation-20260909-followup/runs/fs-junction-observation` at
  Keld `576aaca2977b7082fc97f5a108b9476b4ba4cf57`: direct outside paths denied, but
  an in-scope NTFS junction read and changed the distinct outside sentinel. The result
  is a public-library failure, not shipping-route or LPAC evidence.

The target contract implements architecture 03's stated destination that resolution
precedes the OS effect. T1 must update architecture 03 and 05 to describe the selected
retained-resource implementation, and architecture 02/06 only where their current-state
text needs the KEL-130/KEL-102 ownership edge. The original exact candidate was directly
approved. All three admitted final lenses reviewed the unchanged normative candidate at
`869dbd9ac990a942cc0a4c592b29a479e22610a5` / spec SHA-256
`386cff74e2dc69c71d45e252925bfc3a0b142851ee739ea74200fbdbedafd19f` and returned
zero findings. This metadata-only freeze records that review chain and the explicit
publication/merge authorization. T0 remains unlanded until publication; T1 remains
gated on the landed T0 artifact.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a verified immutable manifest with no filesystem grants, broker preparation
   retains no filesystem handle and succeeds. Given one or more valid `fs.read` or
   `fs.write` scopes, `FsBroker::prepare` retains the exact root/parent directory
   capabilities, rejects duplicate scopes or more than 64 scopes per capability, and
   makes every handle close-on-exec or non-inheritable. A descendant handle census is
   zero. One ambient OS open resolves each manifest-named scope anchor; its resulting
   handle identity is the explicit grant root, even when the anchor itself is a
   symlink/reparse/mount. A substitution before or during that one open may select
   either complete object the OS returns, but later operations must remain bound to the
   recorded handle and never combine it with the old path. KEL-102/T3 separately proves
   that its shipping caller invokes this preparation before any untrusted child,
   privileged listener, or window; that later ordering does not gate the KEL-130/T1
   library artifact. Every provisional handle acquired during preparation remains
   preparation-local. If a later scope is invalid or fails to open, all provisional and
   deduplicated handle owners are synchronously released before `prepare` returns
   `Err`; the owner-process handle census returns to its pre-call baseline.
2. Given a request path longer than 4,096 UTF-8 bytes or containing NUL, request
   validation returns `KELD-NATIVE-004` before guard or filesystem entry. A `..`
   component remains the existing `KELD-GUARD002` decision; FS-specific guard
   validation also maps an empty, `.`, or non-absolute request to `KELD-GUARD002`
   without opening a resource. Given an empty/`.`/`..` scope component, a relative
   scope, a Windows device/UNC/NT namespace or alternate-data-stream scope, or a literal
   unexpanded `$VAR`, broker preparation returns `KELD-NATIVE-008` before any app
   resource. It never resolves a relative spelling against process cwd.
3. Given a valid request, the shared KEL-133 receiver validates and decodes it first;
   `dispatch_privileged` then calls the sole `keld-guard::evaluate` and borrows an
   opaque matched-scope permit into its closure. The closure result cannot borrow that
   permit; compile-fail tests reject returning or storing it, while a compile-pass
   control may read its grant index during the callback. A guard Deny preserves its
   original `KELD-GUARD-*` code/text, and resolver/open/read/write/truncate counters
   remain zero. Before request validation or guard evaluation, each call compares the
   presented verified-manifest digest with the broker's prepared digest. A separately
   verified object with identical bytes/digest is accepted; a different-digest V2 that
   otherwise grants the same requested path returns exact `KELD-NATIVE-008` with zero
   guard/resolver/open/I/O counters and unchanged sentinels.
4. Given a subtree scope and a relative symlink whose complete resolution remains
   beneath its retained root, a regular-file read and write succeed. Given an absolute
   link or junction whose target is absolute, rooted, prefixed, drive-relative,
   namespace/device/ADS-bearing, non-UTF-8, reserved, or escapes above/outside the
   retained root, the operation returns `KELD-NATIVE-002` before a target open.
   An unknown reparse tag, non-regular object, or mount/volume crossing returns
   `KELD-NATIVE-003`. Outside bytes and object identity remain unchanged. This is not
   implemented by denying every link.
5. Given authorize-then-swap, parent rename/replacement, or component rename races, the
   operation either acts on the object reached through retained parent handles and an
   exact opened leaf handle or fails `KELD-NATIVE-002`; it never follows the replacement
   ambient path. The test must observe file identity, returned bytes, and both inside
   and outside sentinels rather than infer safety from an error. An exact-file grant
   retains its parent and leaf name, not an initial leaf object: each call opens the
   final leaf no-follow and authorizes whichever regular non-reparse object then occupies
   that slot. An absent-at-preparation leaf may later be created with create-new.
   Replacing the leaf with any supported symlink/reparse alias, including an internal
   alias to a sibling, returns `KELD-NATIVE-002`; an unknown reparse tag is `003`.
   Ordinary file replacement and the
   explicit hard-link object rule remain unchanged.
6. A filesystem scope grants objects reachable through its retained namespace. A hard
   link beneath that root is the same object as every alias: reads are allowed and an
   in-place write is visible through all aliases. The broker must not promise
   outside-alias isolation from link-count inspection, because another actor can add a
   hard link after such a check. The real-OS oracle proves equal object identity and
   this documented result. If alias-path isolation is required, this decision must be
   rejected in favor of separately designed copy/replace semantics.
7. Given a directory, FIFO, socket, block/character device, Windows reserved device,
   or other non-regular target, a capability-relative metadata probe followed by an
   exact-handle type check rejects it as `KELD-NATIVE-003` before content I/O or
   truncation. Unix opens used for the race-closing check include nonblocking mode.
   A FIFO with no peer and the platform's named special-file case terminate within the
   harness kill bound; a regular-file control still succeeds.
8. `MAX_FS_CONTENT_BYTES` is 8 MiB, `MAX_FS_PATH_BYTES` is 4,096,
   `MAX_FS_COMPONENTS` is 256, `MAX_FS_SYMLINK_EXPANSIONS` is 40, and
   `FS_IO_CHUNK_BYTES` is 64 KiB. The component count covers every component actually
   processed after internal-link targets are expanded on every platform, not only the
   lexical request; a beneath-root expansion totaling 256 succeeds and 257 returns
   `KELD-NATIVE-004`. A read checks handle metadata, then reads at most limit-plus-one
   through the exact handle; zero, maximum, and a concurrently grown maximum-plus-one
   file return exact bytes/exact `KELD-NATIVE-004` without allocating or encoding the
   remainder. A write payload over the content limit fails before guard/filesystem
   entry and leaves the target unchanged. The existing 16 MiB kipc envelope remains
   the earlier outer bound.
9. Existing-file write opens the resolved regular object without truncation, verifies
   type/mount on that same handle, then uses `set_len(0)` and 64 KiB writes on that
   handle. In-place operation preserves file identity, owner, hard-link relationships,
   and ordinary DACL/ACL/xattrs only to the extent the OS preserves them. The OS may
   clear privilege bits such as set-user-ID/set-group-ID or security xattrs/capability
   metadata when size/content changes. The broker accepts that security reduction and
   must never use chown/chmod/setxattr or an equivalent to restore stripped privilege
   metadata. New-file write uses create-new relative to the retained parent; the OS-defined
   `AlreadyExists` result proves creation did not occur and returns
   `KELD-NATIVE-002` without retry. Success is the new-file commit point. Any other
   create result whose effect is not independently known uses the conservative `007`
   rule. This deliberately rejects atomic temp replacement, which would change inode
   identity and inherited security metadata.
10. `FS_OPERATION_BUDGET` is five seconds measured from one checked monotonic `Instant`
    minted immediately inside guard Allow and before the first filesystem query. The
    same absolute instant is checked before and after resolution, metadata, open,
    truncate/create, and every chunk; progress never renews it. A virtual-clock reader
    and writer cross several chunks and prove expiry at the original instant.
11. Deadline/cancellation observed at a pre-commit progress check returns
    `KELD-NATIVE-005`/`006` with no write-content effect; a read always discards partial
    bytes on either result. Once create-new succeeds or truncate/write has been invoked,
    I/O/cancellation/deadline result that is not a completed success returns
    `KELD-NATIVE-007` with committed/requested byte counts and an explicit
    `effect-may-have-occurred` fix. `committed_bytes` is only the sum of positive
    lengths returned by successful content-write calls; it excludes create/truncate,
    does not estimate bytes affected by a failed call, and promises neither durability
    nor final file length. No automatic retry, deletion, rollback, or success
    is fabricated. Completion after the deadline is also `007`, even when all bytes are
    observed, because the timing contract failed after the commit point. Create-new
    `AlreadyExists` is the one explicit pre-commit exception because that OS result
    independently proves no new file was created.
12. The five-second budget is a cooperative broker bound, not a claim that Keld can
    preempt one synchronous kernel call. Tests prove that special files are rejected
    without a blocking content call and that every broker-controlled progress point is
    bounded. A hard wall-clock guarantee for a wedged regular-file driver/filesystem
    remains outside this contract and would require a separately approved cancellable
    OS-I/O or killable-process architecture.
13. On cancellation, a KEL-130 operation observes its supplied cancellation flag at
    each broker-controlled progress point. Once control returns from any synchronous
    syscall and reaches such a point, it reports the exact terminal result and closes
    its per-call handle before return. Setting the flag does not itself bound a wedged
    syscall: while that call has not returned, its handle remains live, no terminal
    result exists, and KEL-102/T3's in-flight wait and broker/root drop are unbounded.
    No raw handle is sent to Bun or a webview. The KEL-130/T1 drop/fresh-broker oracle
    begins only after every call returned. KEL-102/T3 separately owns stopping
    admissions, setting flags, waiting for in-flight terminal results, rejecting stale
    generations, and ordering broker drop; it must not report quiescence complete while
    a call remains wedged or any wrapper retains the actual broker value. `FsBroker`
    is the sole root-handle owner moved into coordinator state; operation/session views
    borrow it and own no root. Dropping one `Rc`/`Arc` wrapper is not broker drop:
    only the actual `FsBroker` destructor plus a zero owner-process handle census
    closes the revocation oracle.
14. Real macOS, Windows, and Linux rows separately run ordinary read/new-write,
    subtree internal-link success, exact-file internal-alias denial, external
    symlink/reparse/junction escape, parent swap, mount/volume boundary where the
    platform supplies one, hard-link object semantics, special-file termination,
    maximum/maximum-plus-one I/O, observed partial cancellation, conditional drop,
    owner-process handle baselines before preparation/after each returned call/after
    actual broker destruction, and a fresh allowed operation after every completed
    hostile case. Every row's `source_head` exactly equals the terminal T1
    `landed_head`; stale ancestors and mixed row heads reject. Cross-compilation,
    hosted results from another OS, WSL, emulation, or mocks do not close another row.
15. T0 and any partial T1 work leave the filesystem channel unreachable from the
    shipping no-flag host. Only the exact KEL-130-owned validator described in section 6
    may accept one landed KEL-130/T1 artifact covering T1a–T1d, the approved T0
    head/blob/decision digest, landed ancestry, trusted publisher, evidence hashes, and
    three distinct native-OS identities whose `source_head` all equal that exact
    `landed_head` as a KEL-102/T3 predecessor. KEL-140 consumes the
    later passed KEL-102/T3 product route and replays these regressions through the
    renderer adapter; it does not duplicate the Rust broker.

## 4. Design

### First-principles model

This changes handle ownership. The host remains the only process with filesystem
authority; no new process, crash owner, or principal is introduced.

| Atom | Owner and boundary | Inputs → outputs | Failure and direct observable | Independence and first falsifier |
|---|---|---|---|---|
| Resource identity | `keld-native::FsBroker`, one host session | verified scope plus trusted absolute path → retained root/parent and per-call exact file handle | ambient reopen or replacement object → file-id/sentinel mismatch | independent of byte limits; swapping a parent after preparation must not redirect the operation |
| Authorization binding | `keld-guard::evaluate` plus `dispatch_privileged` | principal, capability, requested string → Deny or callback-borrowed matched-scope permit | second matcher, escaped permit, or permit/path mismatch → wrong root selected | resolver cannot mint or retain a permit; compile-fail escape and wrong-index overlap mutations must fail |
| Traversal | platform adapter under `FsBroker` | borrowed permit, retained root, bounded relative components → exact handle or typed escape/type failure | symlink/junction/mount escape or exact-leaf alias → outside/unmatched bytes exposed | independent of guard syntax; exact leaves never follow, while subtree links must remain beneath their retained root |
| Scope semantics | `keld-guard` syntax; KEL-130 object meaning | exact or terminal `/**` absolute UTF-8 scope → exact-file parent or subtree root capability | cwd, `$VAR`, device namespace, duplicate or unbounded scope accepted | independent of OS traversal; a relative scope must fail preparation on all platforms |
| Memory/copy bound | `keld-native` content loop plus existing kipc envelope | regular file/request bytes → at most 8 MiB content and one bounded encoded frame | whole-file allocation or max+1 reply encoding | independent of clocks; a counting reader must stop at 8 MiB + 1 |
| Operation clock | KEL-130 operation state | one post-Allow monotonic instant plus cancellation flag → success, no-effect timeout/cancel, or post-commit effect result | renewal, hidden retry, or false hard deadline | independent of KEL-133 clocks; advancing only the injected clock must expire the same operation |
| Write effect | exact opened file handle | validated regular handle plus bounded bytes → in-place create/truncate/write result under OS metadata semantics | broker replaces metadata, restores stripped privilege, ambiently reopens, or reports partial effect as no-effect | independent of resolver after open; post-`set_len(0)` failure is `007`, while set-ID/security-xattr stripping is never restored |
| Lifecycle | KEL-102/T3 coordinator owns the sole `FsBroker` value | quiesce/peer loss plus returned calls and actual broker destruction → terminal results and zero owner handles | wrapper drop mistaken for broker drop, or stale/live handle survives | operations borrow non-owning views; wedged calls or retained wrappers block quiescence, and only the last-owner destructor restores the pre-prepare census |
| Evidence/artifacts | KEL-130/T1 publisher plus KEL-102 predecessor validator | exact landed contract/T1 heads, task rows, evidence digests, trusted publisher and three distinct native OS rows → one passed artifact | stale, relabeled, unsigned-provenance, duplicate-OS or unlanded artifact accepted | T1a implements/reuses the validator and substitution fixtures before T1d may emit the terminal artifact |

Process: `keld-host` owns the verified manifest and the only `FsBroker`. Memory: broker
preflight holds at most 128 bounded grant records and deduplicated retained directory
capabilities; a read holds at most 8 MiB + 1 before encoding, while kipc owns its
separate frame buffer/copy. I/O: all namespace traversal starts at a retained capability
and all content I/O uses the returned file handle. Trust: request bytes select only a
bounded relative suffix under the guard-selected grant. Lifecycle: roots live for one
immutable policy/session generation. The coordinator owns that sole value and lends
non-owning operation/session views; wrapping it extends the same trusted-host owner
lifetime and cannot count as quiescence. After a syscall returns, per-call handles close
before its terminal result. A syscall that never returns retains its handle and blocks
drop without a time bound. Failure: every produced result declares whether write
content definitely did not change or may have changed.

### Scope compiler and guard permit

`PermissionsManifest` stays the sole parser and scope matcher. T1 extends the existing
decision without adding another authorization entry point:

```rust
pub struct ScopePermit {
    grant_index: usize, // private; minted only by keld-guard
}

impl ScopePermit {
    pub const fn grant_index(&self) -> usize {
        self.grant_index
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathScopeKind {
    Exact,
    Subtree,
}

pub struct PathScope<'manifest> {
    grant_index: usize,
    pattern: &'manifest str,
    kind: PathScopeKind,
}

impl PathScope<'_> {
    pub const fn grant_index(&self) -> usize {
        self.grant_index
    }

    pub const fn pattern(&self) -> &str {
        self.pattern
    }

    pub const fn kind(&self) -> PathScopeKind {
        self.kind
    }
}

pub struct PathScopes<'manifest> { /* private iterator state */ }

pub enum ScopeSetError {
    TooMany { actual: usize, maximum: usize },
    Duplicate { first_index: usize, duplicate_index: usize },
    InvalidPath { grant_index: usize, detail: String },
}

pub enum Decision {
    Allow(ScopePermit),
    Deny(DenyReason),
}

pub fn path_scopes<'manifest>(
    manifest: &'manifest PermissionsManifest,
    operation: &str,
) -> Result<PathScopes<'manifest>, ScopeSetError>;

pub fn dispatch_privileged<T>(
    manifest: &PermissionsManifest,
    principal: Principal,
    operation: &str,
    resource: &str,
    handler: impl FnOnce(&ScopePermit) -> T,
) -> Result<T, DenyReason>;
```

`PathScopes` implements `ExactSizeIterator<Item = PathScope<'manifest>>`; the scope
type exposes read-only accessors for its index, pattern, and kind. `evaluate` remains
the one matcher and returns the index of the first matching array entry, preserving
current manifest order. `path_scopes` exposes only those validated descriptors so
`FsBroker::prepare` can retain the same entries; it does not evaluate a request.
Duplicate strings and more than 64 entries for either
`fs.read` or `fs.write` make broker preparation fail closed. `ScopePermit` has no public
constructor. `dispatch_privileged` owns it and lends `&ScopePermit` only for the
callback; the result type is independent of that borrow. The public broker API accepts
neither a permit nor a grant index. Its private resolver reads the index only inside the
callback after the broker verifies the presented snapshot digest. Returning the numeric
index is metadata, not reusable authority. Media and test callers borrow and ignore the
permit; they do not gain a second policy path.

This is a narrow shape amendment to KEL-102 D5, not a second
authorization owner: `dispatch_privileged` remains the sole production caller of
`evaluate` and the only guard-before-handler boundary, while its already-authorized
closure additionally borrows the permit produced by that same evaluation. T1 must
update the KEL-102 spec's exact API prose in the same PR as the code so the approved
specs do not drift. It does not change KEL-102's task order or make T3 reachable.

T1 replaces the current free functions that accept a raw `PermissionsManifest` and
bare path:

```rust
pub struct FsBroker { /* non-Clone, retained grants and snapshot digest */ }

pub enum FsPrepareError {
    InvalidScope {
        capability: &'static str,
        grant_index: usize,
        detail: String,
    },
    OpenScope {
        capability: &'static str,
        grant_index: usize,
        source: io::Error,
    },
}

pub enum WriteInterruption {
    Io(io::Error),
    Deadline,
    Cancelled,
}

pub enum FsError {
    Denied(DenyReason),
    Io(io::Error),
    ResolvedOutOfScope { requested: String, detail: String },
    UnsupportedObject { requested: String, detail: String },
    LimitExceeded { limit: &'static str, actual: u64, maximum: u64 },
    Deadline,
    Cancelled,
    WriteEffect {
        cause: WriteInterruption,
        committed_bytes: u64,
        requested_bytes: u64,
    },
    SnapshotMismatch { prepared: [u8; 32], presented: [u8; 32] },
}

impl FsBroker {
    pub fn prepare(
        verified: &VerifiedManifest,
    ) -> Result<Self, FsPrepareError>;

    pub fn read(
        &self,
        verified: &VerifiedManifest,
        principal: Principal,
        path: &str,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, FsError>;

    pub fn write(
        &self,
        verified: &VerifiedManifest,
        principal: Principal,
        path: &str,
        bytes: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<(), FsError>;
}

pub fn serve_fs_session<S: Read + Write>(
    stream: &mut S,
    token: &SessionToken,
    broker: &FsBroker,
    verified: &VerifiedManifest,
    principal: Principal,
    cancelled: &AtomicBool,
) -> Result<(), IpcError>;
```

These public names, inputs, ownership rules, and variant distinctions are part of the
approval candidate. `FsPrepareError` always maps to `KELD-NATIVE-008`; its source is
retained for diagnostics. The `FsError` variants map in order to the table below.
`FsBroker` is opaque/non-cloneable and is the sole value that owns retained root
handles. KEL-102/T3 moves it into coordinator state; `read`, `write`, and
`serve_fs_session` borrow non-owning views. A trusted-host `Rc`/`Arc` wrapper does
not create a second authority, but it extends actual broker lifetime and therefore
prevents quiescence/zero-census until its last owner drops. Preparation consumes only
the verified manifest, and the broker verifies the same manifest digest at each call. A caller cannot supply a
root or deadline. The borrowed `AtomicBool` is owned and set by the harness today and
by KEL-102/T3's in-flight coordinator later; KEL-130 only reads it, so it cannot grant
or extend authority. `serve_fs_session` has no manifest-only/bare-path overload or
fallback. The request and response postcard shapes stay `FsRequest::{Read,Write}` and
`FsResponse::{Read,Write}`. A signature or variant change requires an updated payload
and review before implementation.

### Resource preparation and path semantics

Only absolute UTF-8 literal paths are serviceable in this v0 broker. The manifest parser
continues accepting `$VARS` as literal strings for its current public contract, but
`FsBroker::prepare` returns `KELD-NATIVE-008` when an actual filesystem grant contains
one, a relative path, or an unsupported Windows namespace. This is a loud gap until a
separate permission-model decision supplies host-owned variable expansion. The broker
never gives a relative spelling ambient cwd meaning.

The filesystem consumer's lexical grammar uses `/` as its only separator on every
platform and rejects `\`. Unix paths begin with one `/`; Windows paths begin with
one uppercase ASCII drive plus `:/`. Components are nonempty, not `.` or `..`, and
contain no NUL. Windows additionally rejects a colon after the drive, trailing dot or
space, every reserved device basename (including superscript-digit forms), UNC, and
Win32/NT device prefixes. Matching remains byte/case sensitive as it is today; the
operating system may resolve the allowed spelling case insensitively only after Allow.
A subtree scope is one normalized root plus terminal `/**` (`/**` and `C:/**` name
volume roots); an exact scope is one normalized file spelling. This parser is part of
`keld-guard`'s FS-specific scope owner and is consumed by preparation; native code does
not reimplement it. The implementation factors these rules into one guard-owned
component-rule source consumed by scope, request, and link-target validation; it does
not create a second native basename/device policy or a broad general-purpose parser.

For `root/**`, preparation makes one ambient OS open of the manifest-named `root` and
retains the resulting directory capability. For an exact `root/file` scope, it opens
and retains the manifest-named parent plus the validated leaf name; the file may be
absent so a later exact write can create it, and any existing file is opened only as the
operation's exact per-call handle. A symlink, reparse point, or mount in the
scope anchor is resolved by that one OS open and the resulting object becomes the
explicit grant root; there is no later canonical path check or reopen. A race during
that open may select either object the OS resolves, but preparation records one handle
identity and every later call remains bound to it. This anchor rule is distinct from an
alias below the retained root. Overlapping grants remain separate guard entries, while
identical retained directories may share one host-owned internal handle owner. The
existing first manifest entry whose lexical scope matches remains final: resolution
failure never retries or falls through to a later overlapping grant. Thus an exact
entry before a subtree entry denies a final alias, while a subtree entry selected first
may follow an internal link only when the lexical request matched that subtree and the
complete resolution stays beneath its retained root. A request uses the borrowed
`ScopePermit` index to select exactly one retained grant and derives its relative
suffix without consulting the filesystem.

The authority is object based after preparation. Renaming a retained root does not
retarget it; replacing the old path does not affect the capability. Exact-file final
leaves are always opened no-follow and reject every symlink/reparse form. Relative
symlinks beneath subtree grants may be followed when their complete resolution remains
beneath the capability. Absolute or escaping symlinks and Windows external junctions
fail.

All three platform adapters use one explicit bounded component worklist and a stack
of retained directory handles beginning at the selected grant root. Every popped
component counts toward the 256 total, including components inserted from link targets;
every consumed link counts toward 40. For each component the walker performs
capability-relative no-follow metadata. A supported subtree link is read once through
the retained parent, its relative target components are pushed onto the worklist,
`..` pops one retained directory but cannot pop the grant root, and an absolute target
is `002`. A Windows link target is decoded as UTF-8 and parsed with native
components before any target open: `Normal`, `ParentDir`, and `CurDir` are the only
accepted kinds; `CurDir` is a counted no-op, `ParentDir` cannot pop the retained
root, and each `Normal` value is revalidated by the shared trailing-dot/space,
colon/ADS, reserved-device, NUL, and basename rules before being passed as one
handle-relative name. `Prefix` and `RootDir` reject even when Rust reports the whole
path non-absolute, covering `C:escape`, `\escape`, UNC and NT/device spellings.
Non-UTF-8 targets reject. Both `/` and `\` are interpreted only by native
component parsing; no string replacement creates new separators. No platform delegates
whole-path link expansion to an opaque kernel call.

Linux opens each non-link component relative to the current retained directory with
safe `rustix::openat2`, `O_NOFOLLOW`, and
`RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV`; it then retains/checks
the returned descriptor before processing the next component. New-file creation is
relative to the retained final parent. A metadata-to-open substitution with a link
fails no-follow rather than hiding extra expansion from the counters. `ENOSYS`, seccomp
`EPERM`, or inability to enforce `NO_XDEV` is a fail-closed platform-availability
result, not a path-based fallback.

macOS and Windows implement the same worklist with safe public cap-std operations; they
do not call `Dir::canonicalize` and then trust/reopen an ambient path. A non-link
intermediate component is opened with
`cap_fs_ext::DirExt::open_dir_nofollow`, retained on the stack, and checked immediately. For a subtree grant, a supported final link is expanded
under the same bounded worklist.
Every non-link final object, and every final leaf under an exact grant, is opened with
`Dir::open_with` and the public
`cap_fs_ext::OpenOptionsFollowExt::follow(FollowSymlinks::No)`; an exact final link is
rejected rather than expanded. Unix adds nonblocking mode. If a component
changes between metadata and read/open, the captured link target or the newly opened
object is still resolved from the retained parent, or the no-follow open fails; there
is no ambient retry.

Every acquired macOS/Windows directory and final file checks
`cap_fs_ext::MetadataExt::dev` against the retained root, so a mount/volume crossing
fails when it is encountered even if a later link would return to the root device. On
Windows, `cap_std::fs::MetadataExt::file_attributes` also rejects any reparse attribute
whose form was not consumed as a supported link. cap-std's directory opens omit
`FILE_SHARE_DELETE`, so an acquired component cannot be renamed out from under the
remaining walk. The common Keld limits are 40 link expansions and 256 total processed
components; crossing either is `004`. This is the smallest extra policy cap-std does
not expose: it reuses its safe handle-relative open/read-link primitives while adding
Keld's per-component mount and unknown-reparse decisions. It permits stable internal
links only for subtree grants, rejects final aliases for exact grants, and requires no
Keld production `unsafe`.

Hard links do not traverse a path and have no portable origin. The scope therefore
grants an object that is reachable under the retained root; all names for that object
observe the same in-place write. Link-count rejection is not selected because its
check races later link creation and would advertise isolation it cannot prove. Atomic
temp replacement is also not selected: it would change the current overwrite contract,
inode identity, owner/mode/DACL/ACL/xattrs and hard-link behavior, and a new file may
inherit broader parent security metadata. In-place truncation can independently strip
OS-managed privilege bits or security xattrs; Keld preserves that reduction and never
regrants it. A future copy/replace API must be a distinct public contract rather than a
hidden security patch.

### I/O, error, deadline, and cancellation contract

The fixed values are:

```rust
pub const MAX_FS_PATH_BYTES: usize = 4 * 1024;
pub const MAX_FS_CONTENT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FS_COMPONENTS: usize = 256;
pub const MAX_FS_SYMLINK_EXPANSIONS: usize = 40;
pub const FS_IO_CHUNK_BYTES: usize = 64 * 1024;
pub const FS_OPERATION_BUDGET: Duration = Duration::from_secs(5);
```

Eight MiB leaves deterministic headroom beneath the existing 16 MiB control-frame cap
for postcard/path fields and bounds the current content-buffer plus encoded-frame copy.
This is a safety/resource ceiling, not a performance claim. A future streaming API may
raise total transfer size while retaining per-chunk credit and operation limits.

Request shape and size validate before the guard because malformed input has no
capability decision. For a valid request, guard Allow precedes the first filesystem
query. A read performs capability-relative metadata, opens the same resolved target,
checks regular type and mount identity on that exact handle, and reads no more than
limit-plus-one in fixed chunks. Metadata length is an early rejection only; it is not
trusted as the final bound. Partial bytes are never returned with an error.

A write checks the payload bound before guard, then resolves without truncation. An
existing target is opened for write with no truncate/create flag and checked on the
same handle. The first `set_len(0)` call is the commit point. A missing target is opened
with create-new under the retained parent; successful creation is the commit point,
while `AlreadyExists` independently proves no creation and returns `002`. All
subsequent writes target that handle. There is no check-path-reopen, delete-before-
rename, temp replacement, rollback, or automatic retry.

The broker converts five seconds to one checked absolute monotonic instant immediately
after Allow. One shared progress-check function is used around every broker-controlled
stage and chunk. Before a stage it loads cancellation first, then reads the clock; a
simultaneously true flag and expired deadline is cancellation (`006` before commit,
`007/WriteInterruption::Cancelled` after commit). After a stage returns, the broker
first records its success/error, acknowledged byte count, and whether create succeeded
or a mutating call crossed the effect boundary; `AlreadyExists` is the named no-effect
case. It then selects one cause in this order: observed cancellation, expired deadline,
returned stage error. The already-recorded effect state decides `005`/`006` versus
`007`, so cause precedence cannot erase an uncertain/post-commit effect. It cannot claim to interrupt a synchronous kernel call: Linux documents
that closing an fd from another thread may leave the blocked call running, and Microsoft
documents that even `CancelIoEx` may race with normal completion and requires completion
inspection. Until such a syscall returns, setting the flag creates no terminal
result, closes no call handle, and provides no bound for KEL-102/T3's wait or broker
drop. Once control reaches the next broker progress point, the broker checks the flag
and deadline before any further effect and applies the table below. KEL-133
transport/frame/session/call expiry remains outer `KELD-IPC-006`: the later session
coordinator cancels the broker and closes the link, but still waits for any in-flight
filesystem call to return before it can discard that call's internal terminal value or
drop the broker. It does not send a `KELD-NATIVE-005/006` reply in place of the outer
expiry. The native `005` is only this broker's own
five-second budget on an otherwise live admitted call; native `006` is explicit
session/quiescence cancellation before the outer owner publishes its result.
Consequently:

| Result | Code | Write-effect contract and fix |
|---|---|---|
| guard deny | existing `KELD-GUARD-*` | no filesystem entry/effect; apply the guard's exact fix |
| allowed OS failure before a mutating call | `KELD-NATIVE-001` | no confirmed content effect; repair path/access/storage and issue a fresh request |
| retained-resolution escape or namespace race | `KELD-NATIVE-002` | no content effect; move the target beneath the granted root or use a direct approved scope |
| non-regular, mount/volume crossing, or unsupported reparse object | `KELD-NATIVE-003` | no content effect; use a local regular file under one retained filesystem root |
| request path/content/component/link-expansion bound exceeded | `KELD-NATIVE-004` | no content effect; shorten the request or use at most 8 MiB |
| deadline observed at a pre-commit progress check, or any read deadline | `KELD-NATIVE-005` | no write-content effect / no read bytes exposed; diagnose the filesystem and issue a fresh request only if safe |
| cancellation observed at a pre-commit progress check, or any read cancellation | `KELD-NATIVE-006` | no write-content effect / no read bytes exposed; wait for a fresh session/generation |
| failure, cancellation, or expiry after create-new succeeds or truncate/write begins | `KELD-NATIVE-007` | target may be empty/partial/complete; message includes committed/requested counts; inspect or rewrite explicitly, never auto-retry |
| invalid/unserviceable fs grant, scope-open failure, or snapshot mismatch | `KELD-NATIVE-008` | no app resource/operation is admitted; repair the absolute scope or use the same verified snapshot, then start a fresh session |

Precedence is closed: KEL-133 frame/envelope/codec failure occurs before this API;
broker preparation and snapshot mismatch are `008`; request path/content shape is
`004` before guard; guard Deny keeps its guard code. After Allow, link target escape,
invalid target grammar, exact-leaf alias, or namespace race is `002`; unknown reparse,
non-regular object, or mount/volume crossing is `003`; component/link/content limits
are `004`. On return, effect state is fixed first: create success or invoking an
uncertain mutating call is post-commit, while `AlreadyExists` is explicitly no-effect.
Cause priority is cancellation, then deadline, then the returned stage error. The
result is `006` then `005` then exact `001`/`002`/`003`/`004` while
pre-effect, or `007/Cancelled` then `007/Deadline` then `007/Io` after effect.
KEL-133's outer `IPC006` remains the outer owner's earlier published result. Scope
count, duplicate, syntax, or open failure is only preparation `008`, never request
`004`. A create-new `AlreadyExists` race is the named `002` no-effect exception.

`committed_bytes` is derived independently from the traced sequence of successful
content-write return lengths. Truncate/create contributes zero; an immediate or later
write error contributes no estimate for that failed call; a complete write followed by
final cancellation/deadline reports the full requested count. The final file contents
remain a separate sentinel oracle and may disagree with this acknowledged-progress
count after an effect-unknown error.

Each new error keeps the existing `CallError { code, message }` wire shape and has one
registry entry plus exact code/message/fix tests. `KELD-NATIVE-007` is conservative:
successful create-new or invoking truncate/write crosses the effect boundary even if a
later syscall reports an error. Create-new `AlreadyExists` is explicitly pre-commit;
other create errors without an OS-backed no-effect contract use `007`. Success is
returned only when every requested byte completes before the budget and cancellation
remains clear at the final observation.

### Reuse and rejected alternatives

| Existing option | Evidence | Decision |
|---|---|---|
| `keld_guard::evaluate` | one allocation-free Allow matcher; currently drops the matched array entry | extend its Allow result with an opaque permit; do not add a native matcher |
| `keld_ipc::guard_dispatch::dispatch_privileged` | sole production guard-before-handler owner in KEL-102 D5 | change its closure to receive the permit; mechanically adapt consumers |
| `keld_core::app_session::open_relative_file` | retained Unix `openat/O_NOFOLLOW` loader and a narrower owner-private Windows path walk | reuse its tested invariants/oracles, not its private function: it is read-only, rejects every link, assumes an owner-private Windows tree, and lives in the upward crate |
| `std::fs::read/write` | current simple broker | refuse: whole-file read, ambient path reopen, eager truncate, no resource/deadline owner |
| canonicalize-check-reopen | produces a path, not a retained authority | refuse: rename/symlink TOCTOU remains |
| deny every symlink/reparse point | closes the observed fixture but breaks valid subtree-internal links | refuse globally: use beneath-root capability resolution for subtree grants; exact final leaves deliberately use no-follow and deny aliases |
| link-count rejection | detects some pre-existing hard links | refuse: races new hard links and confuses one object with its aliases |
| atomic temp replacement | can preserve old bytes on partial write | refuse for `fs.write`: changes inode/security/xattr/hard-link semantics; in-place OS privilege stripping remains allowed and is never restored; replacement requires a separate API contract |
| custom raw-FFI three-platform resolver | could expose every native flag | refuse: duplicates complex open/link policy and would require new `keld-native` unsafe authority |
| `cap-std` / `cap-fs-ext` 4.0.3 | Bytecode Alliance capability `Dir`, safe handle-relative open/read-link operations on Linux/macOS/Windows; Windows uses root-relative `NtCreateFile` and non-delete-shared directory handles; the extension exposes safe cross-platform handle identity | select as the primitive; Keld's shared all-platform worklist owns component/link bounds, macOS/Windows add per-component mount/reparse checks, and Linux uses per-component no-follow `openat2` with `NO_XDEV` |

The selected exact upstream tag is `bytecodealliance/cap-std@v4.0.3`
(`5cae39826c70e7da89cc821b825885e030d38f93`). Downloaded crate SHA-256 is
`c1ec78e242cfa2cfe276807ac2ecc00315a6c97786977414bcd1c3963b6c91b8`;
`cap-primitives` 4.0.3 is
`8b5f74729fd2f44701d1a8eb47e906cdb3ccd9ec0f02baad85a744b791940b18`;
`cap-fs-ext` 4.0.3 is
`56ff379b70af8e08307a8f65e7040c7301cb4a572538ade16b4984f0da77847f`.
The version is newer than the fixed `<3.4.1` Windows device-name advisory boundary.
No MSRV is declared, so the implementation gate must build the resolved dependency
graph with the workspace Rust 1.97 toolchain. The graph introduces
`cap-fs-ext`, `ambient-authority`, `cap-primitives`, `fs-set-times`, `io-extras`,
`io-lifetimes`, `ipnet`, `maybe-owned`, `rustix-linux-procfs`, `winx`, and target
Windows bindings; exact
deduplication against the existing `rustix` 1.1.4 and `windows-sys` 0.61.2 pins is part
of the dependency decision, not assumed.

A local dependency-feasibility observation on physical Windows 11 used Rust 1.97.1 and
an offline locked spike: `cap-std` + `cap-fs-ext` 4.0.3 compiled and the public
`MetadataExt::{dev,ino,nlink}`, `DirExt::open_dir_nofollow`, and
`OpenOptionsFollowExt::follow(No)` APIs accepted handle-derived metadata/options. Cargo
resolved 40 packages and three `windows-sys` versions (0.59, 0.60, 0.61), so this is
positive API/MSRV evidence and a negative size/deduplication signal, not dependency
approval. In a fresh owned NTFS fixture, direct OS read through
`allowed/link/sentinel.txt` reached the distinct outside marker, while
`cap_std::fs::Dir` rooted at `allowed` returned
`PermissionDenied: a path led outside of the filesystem`; the ordinary inside read
passed. This does not prove write, race, relative-symlink, unknown-reparse, or shipping
behavior and cannot close the real-Windows T1 row.

### Current-documentation receipt

- Applicability: applied: portable capability path resolution, Linux `openat2`
  confinement, and cancellation limits that decide the API/acceptance contract.
- Context7: not-applicable:no indexed `cap-std` library was returned; the 2026-09-09
  resolver query returned unrelated products named Cap. Primary upstream/platform
  sources were used directly.
- Official primary: [cap-std v4.0.3 source](https://github.com/bytecodealliance/cap-std/tree/v4.0.3),
  tag `5cae39826c70e7da89cc821b825885e030d38f93`, retrieved 2026-09-09;
  [Linux `openat2(2)`](https://man7.org/linux/man-pages/man2/openat2.2.html), Linux
  man-pages current page retrieved 2026-09-09;
  [Linux `truncate(2)`](https://man7.org/linux/man-pages/man2/truncate.2.html), which
  permits clearing set-user-ID/set-group-ID on size change, retrieved 2026-09-09;
  [Microsoft `CancelIoEx`](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex),
  retrieved 2026-09-09; [Linux `close(2)`](https://man7.org/linux/man-pages/man2/close.2.html),
  retrieved 2026-09-09; [cap-std Windows device advisory](https://github.com/bytecodealliance/cap-std/security/advisories/GHSA-hxf5-99xg-86hw),
  retrieved 2026-09-09.
- Supported claim: cap-std 4.0.3 provides retained-directory, beneath-root relative
  operations and explicit read-link/no-follow primitives; Linux `openat2` supplies
  per-component `BENEATH`, `NO_MAGICLINKS`, and `NO_XDEV` confinement but no
  post-expansion component count; in-place truncation may clear OS-managed privilege
  metadata and Keld must not restore it; neither cross-thread close nor a Windows
  cancellation request proves a hard completion instant for arbitrary blocked
  I/O; cap-std 4.0.3 is beyond the named Windows device-spelling fix.
- Fallback/blocker: dependency/public/permission approval and exact final dependency
  resolution remain required. Linux cannot fall back when `NO_XDEV` is unavailable.
  This contract makes no hard per-syscall cancellation claim.

### Capabilities and manifest

No manifest schema field or capability id changes. `app.fs.read` and `app.fs.write`
remain exact-string or terminal-`/**` arrays. T1 adds fail-closed preparation rules for
the live filesystem consumer: absolute UTF-8 literals only, at most 64 entries per
capability, no duplicates, and no unexpanded `$VAR`. This permission-model change is
within the original owner-intent approval and still requires exact corrected review.

### Wire/protocol changes

None. `PROTOCOL_VERSION`, `FS_CHANNEL`, frame kinds/flags, `FsRequest`, `FsResponse`,
and `CallError` encoding remain byte-compatible. New registered error code values use
the existing `CallError` fields. Any payload-shape or accepted-frame change stops for a
separate wire-protocol decision.

### Platform notes

- macOS: cap-std's component walk holds directory descriptors and resolves relative
  symlinks manually. The final exact handle must be regular and share the root's device.
  FIFO/socket/device controls use actual owned filesystem objects. No macOS pass is
  inferred from Unix code or Linux CI.
- Windows: cap-std 4.0.3's root-relative `NtCreateFile` implementation and directory
  handles without `FILE_SHARE_DELETE` are the selected traversal primitive.
  `cap_fs_ext::MetadataExt::dev` on handle-derived cap-std metadata supplies stable
  volume identity; no unstable Rust `windows_by_handle` API is assumed. Reparse
  traversal is accepted only when cap-std resolves it within the retained capability;
  an unrecognized form is an error, and the exact external-junction and unknown-tag
  cases are dependency-admission tests. Reject reserved devices, ADS, UNC and NT
  namespace inputs. The committed 2026-09-09 junction observation is the failing
  baseline only.
- Linux: the shared explicit worklist counts post-link components and uses safe
  per-component no-follow `rustix::openat2` with `RESOLVE_NO_XDEV`; it fails closed
  when unavailable. A real mount-namespace fixture supplies a bind-mount escape, a
  same-root internal-symlink control, and expanded 256-pass/257-deny rows.

## 5. Boundaries

Implement T1 in one atomic PR:

- `crates/keld-guard` for validated path-scope iteration and the opaque matched-scope
  Allow permit, with manifest/matcher regression tests;
- `crates/keld-ipc/src/guard_dispatch.rs` and its direct consumers only to pass/ignore
  the permit while retaining one enforcement owner;
- `crates/keld-native/src/fs.rs` plus private platform modules and integration tests for
  `FsBroker`, retained resource traversal, bounded regular-file I/O, operation state,
  errors, and cleanup;
- workspace and `keld-native` manifests/lockfile for exactly approved dependencies;
- `docs/engineering/keld-error-codes.md`, architecture 03/05 and only necessary 02/06
  current-state lines after behavior lands; the exact KEL-102 D5/API prose only for the
  approved permit-shape amendment; generated docs only through their owner.

Must not touch:

- `keld-core`/`keld-host` shipping routing or make `FS_CHANNEL` reachable;
- KEL-102/T3 admission/quiescence implementation, KEL-140 renderer/product adapter,
  KEL-133 receiver semantics, manifest schema/generation, role identity, OS sandboxing,
  update/install filesystem code, or webview platform behavior;
- production `unsafe` in `keld-native`; if the selected safe upstream APIs cannot meet
  an acceptance row, stop for a new unsafe owner/review decision rather than adding FFI.

## 6. Tasks (each approximately one PR; ordered; no placeholders)

- [ ] **T0 contract freeze:** this approved and reviewed local candidate records the exact
  owner/API/scope/object/I/O/deadline/error/test choices, canonical decision payload,
  and review status. All required findings/refutations cover the exact 869 normative
  bytes, and the approval-continuity provenance binds the final payload/spec blob.
  Publication/landing remains before this task can close; no product or generated-doc
  publication is claimed.
- [ ] **T1a–T1d, one atomic implementation PR and one terminal artifact:**
  - T1a: land failing Windows junction plus portable authorize-then-swap, permit-owner,
    size/special-file/deadline state tests; add the approved dependencies and public
    API under their named reviews.
  - T1b: replace bare-path `std::fs::read/write` with `FsBroker` preparation,
    capability-relative platform resolution, regular-file checks, bounded loops, exact
    error/effect semantics, and handle cleanup. Delete the unsafe lexical fallback.
  - T1c: run and record the full real macOS/Windows/Linux matrix and all mutation
    controls. Every hostile case is followed by a fresh allowed operation.
  - T1d: update current-state architecture/error registry/generated docs, run full
    gates, obtain exact-final-diff public/permission/dependency/security evidence, and
    publish one landed `keld.execution-artifact/v1` for `KEL-130/T1` whose task rows
    T1a–T1d all pass.
- [ ] **KEL-102/T3 (separate issue/PR):** consume the exact landed T1 artifact,
  separately approve the `keld-core` → `keld-native` dependency, install broker
  preparation before resources and the live in-flight/quiescence owner, and make the
  authenticated filesystem route reachable.
- [ ] **KEL-140 (separate issue/PR):** consume passed KEL-102/T3 and expose/replay one
  guarded operation through the renderer/`@keld/api` product adapter on real macOS.

No T1a–T1c partial commit, branch, or merged library state is a predecessor. Only the
single landed T1d artifact can release later routing.

The T1 terminal artifact is accepted only by the KEL-130-owned validator first added
with negative fixtures in T1a and reused by KEL-102/T3. It requires
`schema=keld.execution-artifact/v1`, `node_id=retained-filesystem`,
`issue_id=KEL-130`, `task_id=KEL-130/T1`, and `status=passed`; the exact landed T0
contract head/blob and canonical decision digest; a T1 `landed_head` descended from
that T0 head and independently proven ancestral to current Keld main; explicit passed
`T1a`, `T1b`, `T1c`, and `T1d` rows; and three evidence rows with distinct
`macOS`, `Windows`, and `Linux` native device identities, source head, OS/build,
command, exit status, observable, and raw-evidence SHA-256. Every row's
`source_head` must equal the artifact's exact `landed_head`; evidence from an
ancestor, unrelated head, or mixed row heads rejects and no tree-equivalence fallback
is allowed. Publication is trusted only
when the Linear artifact author is an authenticated repository owner or standing
delegate authorized for the winning KEL-130 claim; this is provenance validation, not
an invented PKI signature. Missing/substituted fields, evidence digests, task rows,
publisher authority, duplicated OS identity, wrong/unlanded head, or wrong contract
blob/digest all reject.

## 7. Test plan

| Acceptance | Test and independent oracle |
|---|---|
| 1–3 | Bounded manifest/scope tables, handle-inheritance child census, overlapping-scope permit index, and dispatch entry counters. Preparation with valid first/failing second scopes, plus a deduplicated-root variant, must synchronously restore the owner-process handle baseline and permit Windows rename/delete before returning `008`; leaking the first handle fails. Before/during-anchor substitutions prove one complete root identity. Compile-fail returns/stores `&ScopePermit`; compile-pass reads its index. Wrong-index or Deny-closure mutations fail. Prepare from V1, then call with a separately verified same-byte/digest object, which succeeds, and different-digest V2 that otherwise grants the same path, which returns `008` before request validation, guard, resolver, or I/O with unchanged sentinels; removing the digest comparison fails. The identical validly decoded request carrying a 4,097-byte UTF-8 path under the independently verified same digest returns `004`, while under a different digest it returns `008`; both have zero guard/resolver/I/O counters, and a validator-entry counter plus a mutation moving the digest comparison immediately after request validation must show entry/return `004` and fail. |
| 4–5 | Real temp trees with distinct bytes/identities: subtree internal link passes; absolute/rooted/prefixed/drive-relative/UNC/NT/device/ADS/reserved/non-UTF-8/above-root/outside targets return exactly `002` before target open; unknown reparse and mount/volume crossing return exactly `003`. Windows includes `C:escape`, `\escape`, `safe:stream`, `CON`, both native separators, counted `CurDir`, and zero-target-open/outside-sentinel controls. Exact-file internal/external aliases are `002`; overlap order remains first-match with no fallback. Ambient-canonicalize mutation must expose the outside sentinel and fail. |
| 6 | Real hard link with OS file-id equality; read exact bytes, write once, and observe the same bytes through both names. A test text assertion prevents an outside-alias non-effect claim. |
| 7 | Directory, FIFO, Unix socket, device where safely available, Windows reserved device and unknown intermediate/leaf reparse fixtures. Substitute a special object between metadata and open. Parent-controlled completion/status and zero content-I/O/truncate counters are the oracle; a timeout is only the kill switch. |
| 8 | Counting readers/writers at zero, 8 MiB, 8 MiB + 1, 4,096 path bytes and plus one, and 40 link expansions and plus one. On every platform, a short lexical request whose internal link expands to exactly 256 total processed components succeeds, while 257 returns `004`; bypassing the explicit worklist must fail that negative control. Record maximum allocation/read count and exact frame encoding. Removing a limit must consume/process beyond its boundary and fail. |
| 9 | Seed owner, ordinary mode/DACL/ACL/xattr, hard links, and content where the OS supports observation; write in place and assert the same file identity, owner, hard-link relation, contents, and metadata the OS preserves. A Linux negative control starts with set-ID and, where available, security-capability metadata, proves the OS strips it on truncate/write, and fails any broker mutation that restores it with chmod/chown/setxattr. New-file race inserts a leaf after NotFound and must return `002` unchanged. An atomic-replace mutation must fail identity/metadata checks. |
| 10–12 | Injected clock/progress and traced-writer adapters cover partial chunks, each adjacent precedence pair, and simultaneous conditions. After a call, effect state is recorded before cause; cause order is cancellation, deadline, returned error. A pre-set cancel at the expired instant is `006` pre-effect and `007/Cancelled` post-effect; deadline beats a simultaneous returned error without changing effect class; uncertain mutation failure remains `007`; `AlreadyExists` never becomes `007`. External write-return traces derive committed counts: zero after truncate plus first-write failure, sum after partial progress, no estimate for failed-call effects, and requested count after full writes plus late cancel/deadline. Swapped-precedence/off-by-one mutations fail; file sentinel remains independent. |
| 13 | A controlled blocked call proves no terminal/drop until released; afterward cancellation returns the exact effect class and closes the call handle. Every OS uses an isolated owner-process handle census: pre-prepare baseline; unchanged no-grant/error baseline; prepared-root baseline after every returned call; pre-prepare baseline after actual broker destruction. Windows also uses rename/delete. Retaining one non-inheritable call/root handle fails each applicable row. Rc and applicable Arc controls prove dropping one wrapper neither closes handles nor permits quiescence; last-owner drop does. |
| 14 | Three provenance-bound native rows use distinct macOS/Windows/Linux device identities and bind OS/build, exact `source_head == landed_head`, command, exit, observable, raw evidence and SHA-256. A stale ancestor, mixed heads, duplicate identity, wrong digest, or another OS cannot close a row; no cryptographic-signature claim is inferred. |
| 15 | The T1a-owned validator accepts one exact terminal artifact and rejects, one field at a time, wrong schema/node/issue/task/status, T0 head/blob/digest, T1 ancestry, missing T1a–T1d row, missing/duplicated native OS identity, any row `source_head != landed_head`, wrong evidence digest, and unauthorized publisher. A stale-ancestor or one-row mixed-head substitution fails. Route assertions prove no early host registration. |

Every critical test binds to one atom and an independent oracle. Races use barriers,
opened-handle identity and child processes rather than sleeps. Tests use fresh temp
roots and clean all resources. Write-effect tests never infer no effect from an error
alone. The Windows baseline replays both existing negative controls: an always-denying
writer and a wrong read target must make the harness fail.

T0 validation is documentary: Markdown structure/link checks and an exact query that
fails when the owner partition, borrowed-permit non-escape, absolute-scope rule,
subtree internal-link positive, closed Windows link-target grammar, exact-alias denial
and first-match ordering, partial-preparation unwind, hard-link object ruling, post-link
256/257 component boundary, other limits, write commit/effect and acknowledged-byte
semantics, total cancel/deadline/error precedence, conditional cancellation/drop,
per-OS owner-handle censuses, V1/V2 snapshot-digest binding, exact landed-head
native rows and terminal-artifact validation,
cooperative deadline limitation, three real-OS rows, atomic T1 artifact, or successor
stop is removed. T0 does not claim product tests or OS passes. T1 runs `just ci`, the full
mapped suite, dependency/security gates, and the real-OS matrix.

## 8. Review gates triggered

- unsafe: none selected for Keld production code. Upstream unsafe is reviewed through
  the dependency/security gate. Any new `keld-native` unsafe requires a separate
  owner/instruction update and direct approval.
- public API: yes. `Decision::Allow(ScopePermit)`, path-scope iteration,
  `dispatch_privileged`'s borrowed-permit closure, opaque `FsBroker`, new bounds, and
  replacement of bare-path `fs_read/fs_write/serve_fs_session` require approval and
  conformance review.
- permission model: yes. The matched grant is borrowed only during dispatch; the broker
  refuses relative/unexpanded/unsupported scopes, limits scope count, distinguishes
  exact no-follow leaves from subtree-internal links, defines mount/hard-link object
  semantics, and retains roots for one snapshot.
- dependency addition: yes. Exact `cap-std` and `cap-fs-ext` 4.0.3 plus direct use
  of the existing workspace `rustix` 1.1.4 pin require license, advisory, MSRV,
  transitive-version, target-build, size and alternatives review. No dependency is
  approved by this draft.
- wire protocol: none. Existing frame/payload/version bytes are unchanged. The new
  registered `CallError.code` values are public behavior covered by the public-API and
  error-registry review.

Required independent contract lenses and current status:

| Lens | Required reviewer evidence | Independent refuter | Status |
|---|---|---|---|
| filesystem security | final 869 thread `01a08832-9ae3-7a62-ae42-80a6b6598d3f`, artifact SHA-256 `a59aeaa7e7efa6db1f9f7d1275a3fcd9e14ecbec77ee0e8c27d166453cec8722` | preserved f794 full review and root refutation chain | zero findings on the exact 869 normative candidate; this metadata-only freeze records that result |
| cross-platform API | final 869 thread `01a08832-9af6-7a01-b967-e92613dd0191`, artifact SHA-256 `10a194a20af7e9680d782019e109262dccbf26b42d103e1b01d6c1b8e81a380c` | preserved c000 API/source-verification and root refutation chain | zero findings on the exact 869 normative candidate; this metadata-only freeze records that result |
| evidence oracle | final 869 thread `01a08833-e0bb-7322-adf4-07f991cb05f6`, artifact SHA-256 `60b3a2abe4636c1bde96cfa5c033dad3fcdcab2486b3a76530f8f5b1bc37a00c` | preserved f794/c000/cdb reviews and root refutation chain | zero findings on the exact 869 normative candidate; this metadata-only freeze records that result |

Finding/refutation ledger (historical candidate status; final admitted review status is
recorded above):

| ID | Reviewer identity/session + lens + evidence | Different refuter identity/context + evidence | Verdict |
|---|---|---|---|
| `FS-ADV-001` | `/root/fs_design_check`, 2026-09-09, filesystem-security; scope-anchor acquisition had no identity/alias rule | `/root/lpac_design_check`, fresh read of AC1 and resource preparation; one-open anchor object and substitution falsifier are now explicit | `rejected` on revised draft |
| `FS-ADV-002` | `/root/fs_design_check`, filesystem-security; final-device equality missed mount-cross-and-back history | `/root/lpac_design_check`, two refutation rounds; final round verified per-component `DirExt::open_dir_nofollow`, immediate `dev` checks and the crossing-and-return falsifier | `rejected` on revised draft |
| `FS-ADV-003` | `/root/fs_design_check`, filesystem-security; unknown intermediate Windows reparse had no safe inspection hook | `/root/lpac_design_check`, two refutation rounds; final round verified no-follow acquisition plus exact-handle reparse inspection and substitution falsifier | `rejected` on revised draft |
| `FS-ADV-004` | `/root/fs_design_check`, filesystem-security; create-new `AlreadyExists -> 002` contradicted the invoked-mutation `007` rule | `/root/lpac_design_check`, exact-error refutation; OS-proven no-creation `AlreadyExists` is now the sole named exception and other uncertain create failures remain `007` | `rejected` on revised draft |
| `FS-ADV-005` | `/root/fs_design_check`, filesystem-security; exact-file grant ambiguously mixed a retained file with an absent leaf | `/root/lpac_design_check`, authority refutation selected parent-plus-leaf slot and internal links | superseded by formal `FS-CONTRACT-001`: the internal exact-link rule widened authority and survives against the old draft |
| `API-ADV-001` | `/root/lpac_design_check`, cross-platform API; stable Windows volume/reparse API was absent | `/root/fs_design_check`, upstream-source refutation plus local Rust 1.97 compile receipt; cap-fs-ext handle metadata and no-follow APIs are named exactly | `rejected` on revised draft; behavior remains T1 evidence |
| `API-ADV-002` | `/root/lpac_design_check`, cross-platform API; future crate dependency arrow was reversed | `/root/fs_design_check`, Cargo-direction refutation; task now names `keld-core -> keld-native` | `rejected` on revised draft |
| `API-ADV-003` | `/root/lpac_design_check`, cross-platform API; session signature, prepare/snapshot failures and error precedence were not frozen | `/root/fs_design_check`, exact-signature/variant/table refutation against revised text | `rejected` on revised draft |
| `API-ADV-004` | `/root/lpac_design_check`, dependency evidence; `rustix-linux-procfs` was absent from the transitive inventory | `/root/fs_design_check`, upstream Cargo manifest refutation; inventory now names it | `rejected` on revised draft |
| `FS-CONTRACT-001` | admitted thread `01a0878c-2c0b-7f33-a765-0fbb1469e9d5`; exact-file internal alias reaches an unmatched sibling despite exact lexical grant | `/root`, guard destination rule plus overlap analysis; no-follow exact leaf is required while subtree links remain bounded | `survives` against `b3072e71…`; corrected here, closed by the final 869 zero-finding review chain |
| `FS-CONTRACT-002` | same admitted thread; owned `ScopePermit` can be returned from `FnOnce(ScopePermit) -> T` | `/root`, `permit-owned.rs` compiled while `permit-borrowed-escape.rs` failed and `permit-borrowed-valid.rs` passed | `survives` against `b3072e71…`; corrected to a callback borrow, closed by the final 869 zero-finding review chain |
| `FS-CONTRACT-003` | same admitted thread; cancellation promised terminal/drop although one synchronous syscall can remain wedged | `/root`, AC12/AC13 and lifecycle refutation; no terminal value or drop exists until control returns | `survives` against `b3072e71…`; corrected to conditional terminal/drop, closed by the final 869 zero-finding review chain |
| `API-001` | admitted thread `01a0878c-2bf1-7a53-8f64-2c1e0c69cece`; owned permit escapes the callback | `/root`, same compiler controls as `FS-CONTRACT-002` | `survives` against `b3072e71...`; duplicate corrected by the callback borrow, closed by the final 869 zero-finding review chain |
| `API-002` | same admitted thread; whole-path Linux `openat2` cannot observe components introduced by internal links | `/root`, rustix returns only an fd/error; shared explicit worklist plus per-component no-follow `openat2` preserves 256/40 and confinement | `survives` against `b3072e71...`; corrected here, closed by the final 869 zero-finding review chain |
| `META-CONTRACT-001` | `/root`, Linux `truncate(2)` plus unprivileged container counterexample: same-fd truncate/write changed mode 04755 to 0755 | `/root/fs_contract`, exact blanket-promise inspection; OS-managed privilege stripping is a security reduction and restoring it would widen authority | `survives`; corrected to preserve only OS-preserved metadata and forbid restoration, closed by the final 869 zero-finding review chain |
| `FS-SEC-001` | f794 filesystem thread; Windows link targets lacked a closed namespace grammar | `/root`, native `Path` probe proves drive-relative/rooted forms are non-absolute yet expose Prefix/RootDir; shared guard component rules close it | `survives`; corrected, closed by the final 869 zero-finding review chain |
| `FS-SEC-002` | f794 filesystem thread; `Rc`/`Arc` can extend broker allocation lifetime | `/root`, trusted host owns the only value and the direct Rc probe proves last-owner drop, not wrapper drop, is the existing census/rename boundary | explicit revocation defect `rejected`; owner/view/quiescence wording and negative control clarified |
| `FS-SEC-003` | f794 filesystem thread; partial preparation unwind was not observable | `/root`, RAII expectation is not a contract oracle; valid-first/failing-second and dedupe variants are required | `survives`; corrected, closed by the final 869 zero-finding review chain |
| `FS-SEC-004` | f794 filesystem thread; simultaneous cancellation/deadline precedence missing | `/root`, shared state transition now fixes effect first and cause as cancel > deadline > returned error | `survives`; corrected, closed by the final 869 zero-finding review chain |
| `EO-001` | f794 evidence thread; 002/003 and collision classifications were not total | `/root`, exact target/object codes plus effect-first/cause-order matrix | `survives`; corrected, closed by the final 869 zero-finding review chain |
| `EO-002` | f794 evidence thread; `committed_bytes` lacked external meaning | `/root`, successful write-return lengths are independently traceable without durability/final-length inference | `survives`; corrected, closed by the final 869 zero-finding review chain |
| `EO-003` | f794 evidence thread; descendant/rename/fresh-call checks miss Unix and non-inheritable leaks | `/root`, isolated owner-process baselines and deliberate retained-handle mutations close every OS row | `survives`; corrected, closed by the final 869 zero-finding review chain |
| `EO-004` | f794 evidence thread; terminal predecessor fields and publisher trust were incomplete | `/root`, exact T1a-owned validator contract and field-substitution fixtures now frozen | `survives`; corrected, closed by the final 869 zero-finding review chain |
| `EO-SNAPSHOT` | c000 evidence retry surfaced a missing call-time V1/V2 digest oracle but did not pass source admission | `/root`, independent contract read confirms semantics existed without a behavioral/mutation oracle | `survives`; same-digest distinct-object positive and different-digest same-path `008` negative added |
| `EO-005` | valid c000 evidence thread `01a0880e-0181-7b31-a2d3-a23e806143f0`; native row source heads were not tied to landed T1 | `/root`, exact equality is the smallest proof and avoids an unverifiable equivalence fallback | `survives`; every row now requires `source_head == landed_head` with stale/mixed-head negatives |
| `EO-006` | cdb evidence thread `01a08825-6f23-7e52-9bc7-a3d2a058bd9a`; valid-path snapshot case did not distinguish comparison-before-validation | `/root`, identical malformed request with equal versus unequal digest directly distinguishes `004` from `008` | `survives`; paired oracle and post-validation-comparison mutation appended; closed by the final 869 evidence-oracle review |

The f794 and final 869 filesystem-security, cross-platform API, and evidence-oracle
reviewers were admitted with enforced read-only filesystem, no command network,
no-push checkout, rejected approvals, zero active MCP tools, and no dynamic external
tools. The f794 chain found and independently refuted the ledger entries above; the
surviving corrections were incorporated. All three final 869 lenses returned zero
findings on the exact normative candidate. The current metadata-only freeze changes no
normative contract byte. Root independently bound the source, packet, review disposition,
and validation records named above. These static reviews did not run the future T1 native
OS matrix and do not satisfy the required landed T0 or terminal T1 artifact.
Each final finding continues to use guard/36's
schema: id; reviewer identity/session+lens+evidence; different refuter
identity/context+evidence; verdict `survives`, `rejected`, or `unresolved`.

## 9. Perf impact

No performance improvement is claimed. T1 replaces one ambient open with retained
preflight handles and capability-relative component resolution, adds fixed metadata/
clock checks, and reads/writes in 64 KiB chunks. It holds at most 128 grant records and
deduplicated roots per session. The 8 MiB content ceiling bounds the current content +
encoded-frame copy below two 16 MiB payloads. T1 must report handle count, allocation/
copy census, and existing small-message RTT only if shared guard dispatch changes its
measured hot path. A measured regression greater than 5% needs the repository waiver;
language or retained-handle claims are not performance evidence.

## 10. Open questions

There is no remaining user-owned semantic choice in these review corrections,
including the f794 ownership, path-target, unwind, precedence, progress-count,
handle-census, and artifact-validation closures. Exact-leaf
no-follow is required by default-deny destination scope; the callback borrow is the
smallest compiler-proved non-escape repair; and conditional cancellation/drop removes a
guarantee the selected cooperative architecture cannot provide.

The original direct approval remains the owner-intent source for the unchanged hard-link
object rule, in-place write compatibility including OS-managed privilege stripping
without restoration, cooperative deadline strength, absolute literal scope support,
selected dependencies/review gates, and task/OS matrix. The later user instructions
authorize the required formal review repairs and, in the exact text `then merge what is
stoping you`, publication and merge of this reviewed T0 branch. Artifact provenance
preserves both sources and does not claim the user typed the revised digest. All three
formal lenses reviewed the exact final 869 normative bytes and returned zero findings.
This metadata-only freeze is ready for publication and landing; T1 cannot start until the
required T0 artifact is actually landed.
