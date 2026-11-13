# W0 Windows draw.io acceptance preparation — GH-1183

Status: **PREPARED / native product execution UNKNOWN**. This packet and its executable
tests are preparation inputs. They are not a product corpus registration, receipt
format, trusted manifest, compatibility score, installed-app result or native PASS.

The owner remains [GH-1183](https://github.com/gyldlab/keld/issues/1183), under
[program GH-1182](https://github.com/gyldlab/keld/issues/1182). GH-496/500/587/589
retain artifact, corpus, receipt and acceptance ownership. This directory neither
parses product provenance nor replaces their authorities.

## 1. Reproduce the preparation

From this branch's repository root, using the existing test target:

```text
cargo test --offline -p keld-compat --test corpus_registry gh1183_ -- --nocapture --test-threads=1
cargo test --offline -p keld-compat --test corpus_registry
cargo clippy --offline -p keld-compat --all-targets -- -D warnings
cargo fmt --all --check
```

The first command executes seven reader/fixture tests and prints ten named
preparation cases. Each case stays UNKNOWN and must be refused by the existing
application-target admission and product-record gate. The remaining commands check
the surrounding corpus owner, warning-denied crate compilation and formatting.
A zero-test selector, ignored case or a successful metadata test is not app evidence.

The implementation is a child of the existing GH-500 `gh500_partitions` test family.
It reuses `shape`, `PARTITIONS`, `registration`, `v1_fixture`, `Corpus`,
`parse_evidence`, `check_cells`, `validate_product_run` (structural checks only)
and `validate_committed_runs`. It adds no parser, registry, profile map, scorer,
transport, product ID or production source.

## 2. Exact source and environment freeze

Read on Ramani on 2026-11-04 UTC / 2026-11-05 Europe/Berlin. These are the selected
demand inputs, verified again; no claim is made that they are the latest draw.io release.
An actual integrated run must record its own images and inputs before reuse.

| Input | Exact identity / disposition |
|---|---|
| draw.io Desktop | 31.7.0; `2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`; tree `023eebe9676b307d6344e6c642f8d7b802357fb2` |
| Actual `drawio` gitlink | `24b76c2cbd55d88e354042e8d329a2e4708972bc`; webapp 31.7.0; tree `dded4c5aaecbc5a7fb87e92d9363b80a36329488` |
| App lock | `package-lock.json`, lockfileVersion 3; Electron 44.5.1 from package range `^44.2.0` |
| Normative API oracle | Electron 44.4.5; documentation commit `694f45852a0f1726cd23bfd379854de489cccb65` |
| Oracle annotated tag object | `399d8d620aaa1852ccea4e901472ff3ed1da9efa`, which peels to the preceding commit; not itself a source commit |
| App Electron release source | 44.5.1, commit `19c601167d4ae75989938416c18ed81eb21c6020`; distinct from the normative oracle |
| Current main at preparation | `df99602ef10abd2cd6f3b5c633ba6d1cffee97bc` |
| Reused GH-500 fixture source | PR1168 `94f19de3c70306a1fa4d260d85efe10a42fe49f8`; ordinary helper input, not a qualified product build |
| Preparation integration base | `a0076898a069e7b96c132f7fb6f5058625eca888`; parents are exact main and GH-500 fixture source above |
| Existing GH-587 reader branch | PR1147 `61c181f4d82af730e7e2aed3933dff2453429737`; read as an ownership reference, not imported or changed |
| Windows | Ramani; Windows 11 Home Single Language; AMD64; build 26300.9711, display version 26H2 |
| WebView2 environment | 156.0.4314.58; machine-wide installed runtime, also present in existing processes; not a loaded original-draw.io observation |
| WebView2 launcher SHA-256 | `8b6fabdf9f4ebb331cb7325bb9947e4e3f9a2b5345602feeacf277b1f9f355be`; executable hash alone does not freeze every runtime DLL |
| Bun | `1.4.4+38afee6c8`; executable SHA-256 `cb1d8e6564e655e49d30cead359750a1cbbd12f42651caa4b62244d7bd93c244` |
| Rust | 1.99.1; `58cf8f9c35b355199727627d07ae34a4e2846d67`; target `x86_64-pc-windows-msvc` |
| Requested future run/profile | Explicit Legacy for the existing `keld dev` / non-direct lane, subject to the W6 owner contract; no implicit Legacy fallback |
| Observed effective profile | UNKNOWN: no product role was launched by this lane. Synthetic tests use `unverified`; that is not a measured app profile |
| Migrated app/config/build | Not materialized or qualified here. No Windows app artifact, Host image digest, installed distribution or effective config receipt is asserted |
| Source edits | This lane made zero upstream app/main/preload edits and zero KELD production edits. Total required migration edits remain UNKNOWN until #499/source owner supplies an actual prepared artifact and complete patch census |

The original handoff's WebView2 156.0.4314.50 is stale; the fresh environment read is
156.0.4314.58. Never use the old environment receipt for a new native candidate.

### App bytes, distinct from corpus-manifest bytes

These hashes cover decoded immutable GitHub blob bytes, not CRLF-converted checkouts.

| File at Desktop pin | Bytes | Git blob | SHA-256 |
|---|---:|---|---|
| `src/main/electron.js` | 119176 | `b57b5e2047710e829bfc25ab09f32b09ad2114d7` | `55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d` |
| `src/main/electron-preload.js` | 3201 | `44f2f22f159a8a845ff7e573dd10b816383703ec` | `1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98` |
| `package.json` | 1977 | `95a7e931fc43abc9b42dd3ef0007a1b9c1296c7f` | `c3612ea5534ab3a2be739b2ae40752d13cf497ee401af84e72a0c8c18ac7d077` |
| `package-lock.json` | 160985 | `aab2e8562ea5fd1b46e6a9cbc2a432feb78de550` | `b99f2d8b1a166f84495252b4d0cb82da02ec4aa57aef005e50f6ff81cf669f26` |

Electron 44.5.1 lock integrity:
`sha512-lx7AYoFIiyvEvgvpyWO9kOC60xtNPpiPZAth6Fmg9NutzpCAMLwpUlaBY2EXfZyIgUdzEmLMj9KvK0Vrokl1yQ==`.

[Immutable app tree](https://github.com/jgraph/drawio-desktop/tree/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e),
[lockfile](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/package-lock.json),
[webapp](https://github.com/jgraph/drawio/tree/24b76c2cbd55d88e354042e8d329a2e4708972bc).

### Resolved decision and remaining owner amendment

[Wayfinder #391 ruling](https://github.com/gyldlab/keld/issues/391#issuecomment-6547439829)
and [W0 readback](https://github.com/gyldlab/keld/issues/1183#issuecomment-6547440737)
keep corpus `upstream` at the 44.4.5 documentation commit and declare the app's
lock-resolved Electron separately. There is no remaining choice to silently repin.

The GH-532/GH-566 owner amendment is still necessary: current `ManifestV1` and
`UpstreamV1` reject unknown fields. This preparation tests that adding an
unowned app-lock key or replacing the oracle version is refused. It deliberately
does not implement that amendment.

The [official 44.4.5-to-44.5.1 comparison](https://github.com/electron/electron/compare/694f45852a0f1726cd23bfd379854de489cccb65...19c601167d4ae75989938416c18ed81eb21c6020)
and [44.5.0 release](https://github.com/electron/electron/releases/tag/v44.5.0)
include Windows dialog/typing, recent-document Unicode, readonly-file deletion,
remote-session content protection and file-hash-navigation zoom changes.
App behavior measured with 44.5.1 is never relabelled as 44.4.5 execution.
Unchanged documentation text would not establish equal runtime behavior.

## 3. Prepared document inputs and operation binding

| File | Purpose | SHA-256 of exact LF bytes |
|---|---|---|
| `input.drawio` | Uncompressed valid XML; diagram `w0-main`, vertex `w0-box`, label `W0 original 1183` | `6cb57b16f9560d9538581070501ec13303d84ef24932e9eacdd654eb5c4fec6a` |
| `edited.drawio` | Reader fixture with the same document/vertex and label `W0 edited 1183` | `0127174c89fb39fccadff7e419b87ee4a2d14d66f0aee60adaeafcbc0d41058b` |
| `other.drawio` | Wrong-document control: `w0-other` / `other-box` / `OTHER DOCUMENT` | `57f7aa70b4fbe5c32fa14ee07067f2af5c5741b70b091dc0d0a2da3c3744d14b` |

The edited file is a prepared byte fixture, **not a prediction of native draw.io
serialization**. A real run must separately prove the requested model edit, capture
the actual serialized Save payload belonging to the original caller, and compare an
independent disk read byte-for-byte with that payload. Compression, timestamps or
metadata must not be silently normalized by a new parser or by editing a golden
after seeing the output. Save payload equality alone does not prove the intended edit;
the document/model observable is independent.

Every future operation must bind these facts through the existing owners:

- Complete attempt census, candidate/build/source/config identity and original attempt
  start; preserve failed first launches and retries instead of selecting a later green.
- Original caller's `rendererReq` / `mainResp` `reqId`, role principal and Host
  generation; dirty-close additionally uses draw.io's sender and `uniqueId`.
- Current window/native parent and WebView/navigation/document generation at admission
  and completion, with frozen requested profile plus independently observed effective
  profile. A delayed G1 write must not be credited to G2.
- Actual input/model/save-request/persisted/reopened bytes, with the corresponding
  separate digests. The corpus's `artifact.sha256` remains the exact manifest digest.
- Native source of the observation, its collector, OS/engine/build identity and
  original operation's terminal result. Console success text is not native proof.

These are consumption requirements, **not new receipt field names**. The approved
[GH-587 design](https://github.com/gyldlab/keld/blob/16e15d42f833ca2942e4312b80987c6e541c6c71/docs/specs/gh587-electron-apps-v0-corpus-cells.md)
owns the reader contract; #588 and #541/#552/#542 own its trusted producers.

The [pinned original main](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/src/main/electron.js)
uses theme/display facts at lines 717 and 738–741, dirty-close sender/uniqueId and
Windows button mapping at 849–876, native close veto at 947–962, and rendererReq /
mainResp request correlation at 4431–4537. Windows is Save=0, Discard=1, Cancel=2;
the Mac ordering differs. The
[pinned Electron dialog contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/dialog.md#dialogshowmessageboxsyncwindow-options)
is the normative synchronous button-index oracle.

## 4. Operation-by-operation integration matrix

All native observations below are **NOT EXECUTED / UNKNOWN**. The executable plan
maps them into GH-500's existing activation and four workflow operations.
Cleanup/relaunch and the three close decisions are required subcases, not new
scoreboard cells. Install remains UNKNOWN pending a real distribution artifact.

| Prepared case | Actual positive observable and required negative | Present blocking integration |
|---|---|---|
| Launch | Original app's own page initializes on first attempt, original main/preload run, current role/window and effective profile are proved. Suppress actual page initialization: no PASS | W2 T3 managed Create; W1 verified preload/Route D; W3 B22 entry/policy and native source; W4 first native mirror sample; W6 Windows profile composition |
| Open | Native Open chooses `input.drawio`; matching original reqId loads its bytes into `w0-main`. Open another diagram with same success text: reject | Launch gates; W5 parented native Open and caller custody; W1 app IPC; W6 real file access. Opening a diagram is not top-level renderer navigation |
| Edit | Actual `w0-box` model value changes and same document is dirty; saved input remains unchanged. Keyboard input with no model change: reject | W1 real preload/dirty-state path; W3 attributed settled evaluation when used; actual renderer resource load |
| Save existing | Original request settles; independent disk bytes equal actual captured save payload; reopening proves model persistence. Return success without writing, or write other bytes: reject | W1 save/callback routing, W6 write/profile, W3 B21 if evaluation participates. W5 is not needed for a previously named path |
| Save As | Parent-owned native Save produces destination; original request writes correct bytes and fresh open reads them. Return another operation's path: reject | Save-existing gates plus W5 Save panel, parent generation, cancellation and terminal custody |
| Dirty close / Cancel | Button 2; same HWND/controller/window/profile remains, dirty edit survives, subsequent normal Save works. A Cancel ACK with destroyed window: reject | W5 real sync prompt/index; W2 T3 veto and identity retention; W1 current close continuation |
| Dirty close / Save | Button 0; correct document is saved before accepted destroy, exactly once. For an unnamed document, cancelled/failed Save must retain dirty state and allow healthy follow-up | W5 prompt and required Save panel; W1/W3 original-caller result; W6 write; W2 T3 save-before-destroy ordering |
| Dirty close / Discard | Button 1; matching draft cleanup settles, original saved bytes remain unchanged, one accepted destroy. Unexpected overwrite or wrong-document cleanup: reject | W5 prompt, W1 discard continuation, W6 draft/backup and descendant operations, W2 T3 final release |
| Cleanup | Owned roles/descendants, native window/controller and profile lease actually release; no late callback/write. Observe before any harness rescue; timeout is not cancellation | W2 managed final-window path and lead release qualification; W6 app descendants/profile. GH1073 fix is consumed |
| Fresh relaunch | New process/session/generation reopens independently checked saved bytes; no old receipt is reused; another healthy operation succeeds | Cleanup plus W1-W4 fresh initialization and W6 new admission; preserve GH814 first-attempt evidence |

Common bindings in section 3 apply separately to every row; a startup receipt does
not stand in for a later Save or close receipt.

### Refreshed dependencies and exact heads

| Owner | Head / source | Precise gap or consumed result |
|---|---|---|
| W1 #1184 | PR1307 `20a798976d03e76056c5edf4c0d6fb5775c5659c`, spec r17 | Draft design; B1 security amendment, B3 eligibility, B4 Windows preload trust root, B6 file attribution and shared carrier/grant/native falsifiers remain. Non-draft PR UI does not approve the spec |
| W2 #1188 | PR1395 `5e13e6199812202b40785e0ab87a6d9fdcaed2cf`; PR1397 refreshed to `a12b996bb75105da8e2f1b54eb8a8cbe309d5dbb` | Existing-view/release foundation; `supports_managed_windows()` still returns false at the refreshed PR1397 source. Lead owns T3/integration; neither is an original-app candidate |
| W3 #1185 | PR1311 `aa00b50ecafa7454bc950f7f8487cc28a68df186`, r18 | B21 decisions exist; successful ordered enqueue before terminal commit and proof remain. B11 capacity and B22 entry/window-key/navigation-policy source remain owner gates |
| W4 #1186 | Main base `df99602e`; shared mirror reference `e09b520eaee1e4bd69bd0bde2137ec52f0fed18a` | Reuse existing mirror. Windows identity/units, subscribe-before-first-sample, updates/recovery and native product integration unqualified |
| W5 #1189 | No qualified Windows implementation identified in current owner issue | Native Open/Save/sync message box, physical cancellation and original-call terminal proof. Mac dialog PR1308 supplies no Windows run |
| W6 #1187 | Shared PR1319 `ae4403e0e8a0df80149c44aea906b2e73bce51df` | Windows explicit-profile initial/recovered admission and real draw.io descendant custody unqualified. Mac proof is not transferable |
| #1073 | Closed; merged #1279 `6dffd7de98ea369eefce642609a8105ad24e0492` and #1282 `460815b32557f06450fba4a05517389ac8cdead3` | Answer preservation and post-Quit drain implemented. Rerun applicable regressions for composed candidate; do not list as missing feature |
| #1114 | Tracker open; repair #1281 merged as `a866a7c5adbd9a3c9849af84daf76e743c5304ad` | 34 actual formerly skipped cases; old 35 counted a suite hook. Infrastructure implemented, new-candidate execution still required |
| #814 | Open | First-attempt startup uncertainty; later passing images refute deterministic failure. Preserve original failures |
| #532/#566 | Current closed v1 schema | Separate app-lock field and app-only citation semantics require owning amendment; no local exception |
| #587/#588/#541 | Existing structural reader and independent producers | No authenticated saved-byte/current-generation product collector yet; retain refusal |

[W6's correction](https://github.com/gyldlab/keld/issues/1187#issuecomment-6492238239)
limits the Legacy direction to development/non-direct runs. KEL-254 AC13 still
refuses direct installed Legacy boot. Process Job membership is lifetime custody,
not filesystem/network containment. Missing profile declaration is not automatic Legacy.

## 5. Negative controls and their honest limits

| Executable control | Existing discriminator exercised | Meaning |
|---|---|---|
| Wrong manifest digest; app-document hash substituted for corpus digest | `RecordMismatch(artifact.sha256)` | Specific metadata rejection; distinct manifest/app/image/document hash meanings preserved |
| Mac or mock engine presented for Windows | `RecordMismatch(revisions.engine)` | Specific declared-engine rejection; not authentication of an arbitrary external attachment |
| App-only PASS on uncited UNKNOWN cell | `RecordMismatch(result)` | No metadata-only app PASS |
| Extra generation/origin/document-digest field in KEL-74 record | `InvalidRecord`, code `KELD-COMPAT-005`, unknown field | No new authority channel through self-declared JSON |
| App-lock 44.5.1 replaces normative version; extra app-lock key | `UnadmittedPin`; `Parse` | Existing owner pin/schema stay closed pending amendment |
| Wrong saved bytes (old input or other diagram), stale-generation attachment | `ProductRecordsNeedReceipt` | Blanket product refusal. Specific document/generation semantics are NOT IMPLEMENTED by the reader |
| Identical success line from unqualified Windows, Mac, mock, hello-world | `ProductRecordsNeedReceipt` | Text-only runner matching can be imitated; it cannot cross the product admission gate |

The tests intentionally show that the structural-only `ProductReceipt` path can
accept an UNKNOWN record carrying an opaque locator for wrong bytes or stale text.
That is **not** a successful product test. The actual committed-product boundary
refuses every such record, including the healthy-looking synthetic control. A field-
specific byte/generation rejection plus an authentic native positive cannot be claimed
until #587 consumes qualified Host/document/OS observations.

A one-fault sensitivity run must remove the product refusal on the owned temporary
worktree and make these tests RED; then restore exact source bytes and rerun GREEN.
Digest and engine mutants must likewise make their specific controls fail. Retain
every original run, including test authoring failures. Healthy structural recovery is
not healthy native recovery; the latter stays unrun.

## 6. Candidate handoff and resource discipline

Do not request a GUI slot until the lead supplies one immutable composed candidate
with approved W1/W3 sources, enabled/proved W2 managed windows, W4 native mirror,
W5 dialogs, W6 run-mode/profile composition and the existing qualified receipt reader.
Then post the exact candidate/build/source/config/fixture hashes and obtain the
serialized Ramani native claim. No Mac run can fill a Windows cell.

Use a fresh owned operation workspace per close decision. Preserve original file
bytes, failed attempts, profiles and process identities. Do not terminate pre-existing
WebView2 or another agent's processes. Observe product release before emergency
cleanup and distinguish requested Quit from harness/lease termination.

This preparation opens no windows, dialogs, native test app or profile. Its compiler
and test processes must end normally. The managed source tree, cache and bounded
run evidence are retained for the open draft and owner handoff. Full workspace/native
gates and healthy product recovery are separately reported when not run; a crate
test result is never substituted for them.
