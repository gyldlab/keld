# W1 Windows preload feasibility fixtures

This directory supplies bounded, independently reviewable preparation for
[GH1184](https://github.com/gyldlab/keld/issues/1184). The governing contract is
[PR1307 r17 at 20a798976d03e76056c5edf4c0d6fb5775c5659c](https://github.com/gyldlab/keld/blob/20a798976d03e76056c5edf4c0d6fb5775c5659c/docs/specs/gh1184-windows-preload-facade.md).
Route D remains proposed. These fixtures do not implement a production Windows
bridge, classifier, source authority, grant writer, parser, or document epoch.

## Review map

| Entry                                                              | What it supplies                                                                                          |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| [DECISIONS.md](./DECISIONS.md)                                     | Exact unresolved owner decisions, inspected shared pins and a narrow existing-symbol handoff              |
| [source_binding/README.md](./source_binding/README.md)             | Authentic input identities, seven manual source sites, bounded input census, behavior tests and mutations |
| [native/README.md](./native/README.md)                             | Standalone Windows API harness, source/binary custody, result schema and lifecycle limits                 |
| `prepare_probe.py`, `preload_adapter.js`, `first_inline.html`      | Digest-checked unchanged preload bytes inside a test adapter; independent first-inline observations       |
| `run_native_matrix.py`, `matrix_assertions.py`, `native_census.py` | Serialized native cases, page/title effect oracles and an independent owned-process/profile census        |

All product integration files remain outside this slice. W2 owns native window
creation/release and shared entry points. W3 owns the sole resource/file parser,
navigation eligibility and source identity, with the shared generation/transport
owners. No duplicate production authority is introduced here.

## Authentic inputs and observation boundary

Upstream draw.io Desktop is pinned at
`2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e`. The unchanged preload has 3,201 bytes,
SHA256 `1091fd21ef727a52ef89bde814aa662eed4a0c3ce32beb858a068a96b285bf98`.
The unchanged main file has SHA256
`55505e690d37070344be172a68ad997ab48c687302dc5dee30a988d4f153770d`.
The complete application/package census and production aggregate `mainDigest`
remain unresolved.

Preparation checks the same in-memory preload bytes before inserting them.
The native harness independently hashes the loaded bundle and compares it with
the configured fixture digest before allocating a native session. This
caller-supplied test pin is not an authenticated production trust root.

The adapter supplies only the test surfaces needed to execute this preload.
Its local `ipcRenderer.send` records sink observations; it does not call Bun,
evaluate a grant or implement KIPC. No arbitrary native Host object is exposed.
The world is shared and `contextIsolated` is false. The authentic preload is
never rewritten to claim isolation. Its original comments describe its upstream
Electron environment; they do not establish a Windows isolation guarantee.

## Native matrix

The driver writes a fresh directory per case and stops on unexpected output,
an outer timeout or unconfirmed resource cleanup. It never treats timeout as
cancellation and does not kill a process or delete a profile after a timeout.
Native cleanup must observe controller close, browser-exit notification, held
process handles and profile/window release. A separate Windows process census
includes command lines for the owned profile, including crashpad.

| Cases                         | Independent observation and limit                                                                                                                                                       |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Wrong bundle digest           | A single expected-digest change produces a specific pre-native source rejection, no registrations/navigation/messages and no native allocation                                          |
| Delayed injection             | First-inline title and message see no facade, while eventual preload completion proves the bundle itself still executes                                                                 |
| Document-created recovery     | The same bundle is present before first inline code, seeds configuration and produces authentic sink observations                                                                       |
| Existing configuration        | An explicit existing `none` value survives the authentic preload's migration                                                                                                            |
| Unfenced / guarded held reply | A deliberately held old observation-tagged reply is delivered visibly to the successor in the mutant; the synthetic fence suppresses it while a current-document barrier still delivers |
| Natural forcing subset        | One trial each: unforced, busy 50/200/1000 ms, pagehide and unload; 256 requested posts per trial, 1,500 ms observation interval                                                        |
| Final recovery                | A fresh native session repeats the healthy document-created observation                                                                                                                 |

The held-reply pair is synthetic. Its page-generated observation tag and native
commit serial are not product authority, a nonce contract or a second epoch
implementation. The natural cases only measure candidate arrivals relative to
successor ContentLoading. An empty at-risk set is `RULE_NOT_EXERCISED`.
Neither case closes B7. COOP/process-swap, BFCache, full-rule/weaker-rule
discrimination, pre/post NavigationStarting reply subsets and stalled-navigation
controls remain separate unexecuted criteria.

These experiments do not supply the required product controls for an ungranted
`el:` channel, missing product epoch, authenticated resource eligibility or
actual Bun handler admission. Existing shared tests may prove their own
transport/principal behavior; that is not W1 product acceptance.

## Reproduction

Use Ramani and obtain the serialized native slot recorded on GH1188. Build only
the standalone fixture using its locked dependencies; see its README for exact
commands and source/binary hashes.

From the Keld primary checkout, after the owned task source is committed:

```powershell
just work-run gh-1184-w1-feasibility-20261105 --session chatgpt-w1-feasibility-20261105 -- python -B crates/keld-wv/tests/fixtures/windows_preload_feasibility/test_prepare_probe.py
just work-run gh-1184-w1-feasibility-20261105 --session chatgpt-w1-feasibility-20261105 -- python -B crates/keld-wv/tests/fixtures/windows_preload_feasibility/run_native_matrix.py --binary crates/keld-wv/tests/fixtures/windows_preload_feasibility/native/target/debug/keld-w1-native-probe.exe --slot-receipt <actual-GH1188-acquisition-URL>
```

The native driver's default output uses the workspace tool's private temporary
directory. Its `matrix.json`, each native `result.json` / `trace.jsonl`,
process exit and post-exit process census are retained. Run commands and their
exit status are also captured by the existing workspace runner.

## Evidence and open gates

The checked-in `source_binding/evidence/20261104T234229Z-*` records are immutable
historical evidence for the source snapshot published in
`cf4a499ae198fd10cf690971f7ae22ef93be3265`. Later formatting of the two authored
MJS harness files changes their hashes; those earlier receipts do not attest to
the later bytes. Fresh input hashes and actual run results belong to the
immutable research run linked from GH1184, alongside original failures,
independent reviews and healthy recovery.

Initial hosted CI failures are retained and remain explicit: the protected-base
provenance policy rejects fixture package/Cargo/ignore authority paths; global
formatting includes authentic upstream JavaScript and byte-sensitive census
inputs. Three CodeQL annotations in the retained upstream main file remain open.
No fixture imports or executes that main file; independent source review found
no newly reachable path to those routines. This does not dismiss the alerts or
approve a policy exception. Authored executable harness files are formatted;
immutable inputs keep their original audited bytes.

Production admission requires the actual shared source/security/public-interface
contracts, qualified shared implementation, every applicable native acceptance
cell, and exact-head independent review. Keep the evidence PR draft/open.
