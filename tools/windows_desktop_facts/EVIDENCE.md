# W4 measured desktop-facts feasibility

Status: tested native feasibility and draft contract; production Windows activation remains blocked on shared-owner decisions.

Owning issue: [GH1186](https://github.com/gyldlab/keld/issues/1186). The task branch is `agent/gh-1186-windows-desktop-feasibility-20261105`, based on main `df99602ef10abd2cd6f3b5c633ba6d1cffee97bc`. All production files remain unchanged. The [draft specification](../../docs/specs/gh1186-windows-desktop-facts.md) and [unapplied integration notes](integration-notes.md) define the proposal and exact dependencies.

## Final reviewed-wrapper native rerun

The final committed fixture at `29f99d62cf227e5244672cef6094c6a31a240e94` was executed again on 2026-11-05 from 01:51:19Z to 01:51:23Z through managed work-run `run-15f25933a14e4f5c97c78421eafc75ae`. The [bounded acquisition](https://github.com/gyldlab/keld/issues/1188#issuecomment-6647411719) followed W3's explicit cleanup/yield and W2's queue agreement. W4 [released the device](https://github.com/gyldlab/keld/issues/1188#issuecomment-6647424705) after actual cleanup at 01:51:28Z; W2 retained immediate next priority.

The final wrapper sequence passed in 4.416388 seconds. The held-Subscribe native test passed once and all six fresh receipt/oracle assertions passed. The probe and oracle executable hashes are unchanged from those below. The complete native identity, physical rectangles, DPI, actual WinRT text scale, appearance and qualified shared facts matched the original independently acquired values; they were read again rather than taken from an old receipt.

| Final-run boundary | Monotonic time in the test process |
| --- | ---: |
| Valid Subscribe observed | 94.3557 ms |
| First native sample started | 96.1543 ms |
| Native and WinRT sampling completed | 609.8055 ms |
| Desktop applied and Subscribe settled | 612.5453 ms |
| Facade Ready observed | 613.1395 ms |

Sampling took 513.6512 ms. The original 5,000-ms caller deadline remained active. The scripted-Host/client boundary is unchanged and `coreAdmissionProven` remains false.

All 8 invocation Jobs were observed empty with no timeout, setup error or forced cleanup. Pre/post census recorded zero owned native processes and preserved all 13 baseline WebView PID/creation identities. The private oracle profile's measured file identity and creation time were revalidated; all 67 entries were checked for reparse points, the profile was removed and absence verified. The outside cleanup sentinel stayed byte-identical. No global setting, foreign process or existing cache was changed.

[Fresh native receipts and source/cleanup bindings](evidence/20261105-0151) preserve this execution separately from the original run and all failure-first controls.

## Original native run and custody

The lead released Ramani at [23:58:07Z](https://github.com/gyldlab/keld/issues/1188#issuecomment-6645606390). W4 [acquired the next bounded slot](https://github.com/gyldlab/keld/issues/1188#issuecomment-6645615070), then [released it to W3](https://github.com/gyldlab/keld/issues/1188#issuecomment-6645637829) after verification and cleanup. No competing agent's process was signalled.

The run began 2026-11-04T23:59:16Z and completed at 23:59:18Z. The machine reported RAMANI, x86_64, Windows version 10.0 build 26300 through RtlGetVersion. The exact probe executable SHA-256 was:

```text
947ed40f2c9be6dca101b5feb9b626eac11611c93d7767185aa88da60c6336f0
```

The separate Electron oracle was version 44.4.5, executable SHA-256 `bd14928e0728366fd3f41499cb398ff3f4304dab259a3e605077899a6f8c748e`. It was extracted from the existing checksum-verified official cache. It used a fresh private profile and one hidden window. ELECTRON_RUN_AS_NODE was removed only from that child environment.

Raw records, native failures, process exits, the measured first-subscription order and the source binding are committed in [evidence/20261104-2359](evidence/20261104-2359). [SHA256.json](evidence/20261104-2359/SHA256.json) hashes each evidence file. The source manifest records the exact fixture bytes at execution, including files whose documentation later gains this report. Published text uses the repository's LF line separators. `CAPTURE_SHA256.json` and `line-ending-receipt.json` retain the original capture hashes and whether each capture used CRLF; reconstructing those separators reproduces the original bytes. The published-file manifest was also checked against the staged Git blobs. No observation, status, timestamp or output text was changed by this encoding-only publication step.

## Measured values

These are observations of the unchanged machine, not test constants.

| Fact | Native observation | Qualified shared projection / oracle |
| --- | --- | --- |
| Primary display bounds | (0,0), 1920 × 1080 physical pixels | (0,0), 1536 × 864 device-independent pixels |
| Primary work area | (0,0), 1920 × 1020 physical pixels | (0,0), 1536 × 816 device-independent pixels |
| Effective monitor DPI | 120 × 120 dots per inch | Device scale 1.25 |
| WinRT UISettings.TextScaleFactor | 1.0, actual Windows PowerShell 5.1.26100.9711 / STA observation | Effective scale 1.25 |
| Application appearance | AppsUseLightTheme status 0, value 1; high contrast false | `should_use_dark_colors=false`; Electron independently agreed |
| Physical mode | 1920 × 1080, 144 Hz | Independent EnumDisplaySettingsW dimensions matched the physical monitor query |
| Primary work-area cross-check | SystemParametersInfoW matched GetMonitorInfoW | No substituted rectangle |
| Native target identity | Adapter LUID high 0 / low 58488; target ID 512; full DISPLAY#CMN153C device path | Proposed session ID 1, derived from the retained full target key |

The complete native target path is retained in [native-first-sample.json](evidence/20261104-2359/native-first-sample.json). HMONITOR `0x10001`, source name `\\\\.\\DISPLAY1`, enumeration index and geometry are not identity keys. Session ID 1 alone is not treated as proof of identity: distinct-target and retained-history tests exercise the mapping separately.

The oracle read screen/theme immediately after its Ready and before any window, then after constructing a hidden window. Both observations agreed. Its physical↔logical rectangle conversion matched the native rectangles. The positive sample and a second native process reacquired the same complete identity and geometry; no previous receipt was used as the source.

Only the actual single-primary, non-forced-colors configuration was qualified. No physical dark-mode transition, display reconnection, mixed-DPI arrangement or global notification delivery was manufactured or claimed.

## Subscribe before the first native sample

The first desktop query in the run was inside the real shared-client test. It used the original S1 LifecycleLink, WorkerLink, codec and WindowSource across an actual Windows named pipe. The Host peer was scripted and is explicitly not production Core.

The test observed an authenticated HELLO and valid Subscribe, used an Echo fence, and asserted no settled subscription, no Ready and no fabricated desktop before invoking the sampler. The original 5,000-ms caller deadline remained active.

| Boundary | Monotonic time in the test process |
| --- | ---: |
| Valid Subscribe observed | 112.0353 ms |
| Native sampling started, after the pending assertion | 114.3311 ms |
| Native and WinRT sampling completed | 745.0534 ms |
| Desktop applied and correlated Subscribe settled | 747.6756 ms |
| Facade Ready observed | 748.2295 ms |

The native sample took 630.7223 ms in this run. These timestamps establish order for this fixture; they are not a production performance budget. The test checked exact native facts before subscription settlement and before Ready.

## Failure-first and negative controls

| Control | Executed result | What it distinguishes |
| --- | --- | --- |
| DPI-unaware process, unchanged desktop | Reported 96 DPI and virtualized 1536 × 864 / 1536 × 816 rectangles; independent physical mode remained 1920 × 1080. Adapter refused it as physical input. | A successful Windows call can still use the wrong coordinate units. |
| Physical pixels presented as logical pixels | The independent oracle assertion failed as expected at the observed non-unit scale. | A missing scale conversion cannot pass using the current machine's values. |
| Null monitor handle | Actual GetMonitorInfoW failure, status 1461, exit 1, no sample. | No successful zero-rectangle fallback. |
| Invalid DPI enum on a real monitor | Actual GetDpiForMonitor failure, HRESULT 0x800700a0, exit 1, no sample. | No successful DPI 1 fallback. |
| Zero high-contrast structure size | Actual SystemParametersInfoW returned failure; last-error value was 0, exit 1, no sample. | The Boolean API result is authoritative; a zero last-error value does not turn failure into a light-theme fact. |
| Copied S1 replay flag initialized true | The real-client test failed: Ready count 1, required 0. | Pending replay cannot be bypassed. |
| Copied Subscribe timeout shortened to 1,000 ms | The real measured five-second deadline assertion failed. | The fixture does not merely assert a constant or bless a changed deadline. |
| Changed pinned transport blob | The export helper refused before creating output. | Another shared implementation cannot silently enter the fixture. |
| Distinct identities, reordered clone targets, removal/recovery and 33rd lifetime identity | Synthetic deterministic tests passed, including distinct IDs and refusal without eviction or partial allocation. | A constant display ID or current-enumeration index is inadequate. |

Both deliberate shared-source mutations were confined to owned copies. The original shared dependency stayed untouched; the copied files were restored to Git blob `9287ad6d61c783e2e4356ffd72e3f4a2a8a0aa51`. The original cases passed again after restoration. No compiler/import failure is counted as a semantic falsifier.

## Executed checks

| Check | Result |
| --- | --- |
| Standalone native Cargo build, locked/offline | Pass |
| Standalone native Rust tests | 5 pass |
| Standalone native format and all-targets warning-denied Clippy | Pass, including the final full-packet pointer correction |
| Exact unmodified S1 desktop codec/mirror suite | 27 pass, 186 expectations |
| W4 conversion/identity/refusal tests with shared export selected | 18 pass, 82 expectations |
| Real Windows named-pipe client contract | 8 pass; current combined non-native run 26 pass, 7 explicitly gated native cases skipped |
| Native pending-Subscribe test | 1 pass in each of two fresh native runs; 8 non-native cases filtered per run |
| Fresh native receipt/oracle assertions | 6 pass in each fresh native run |
| Restored Ready/deadline controls | 2 pass after the two expected mutant failures |
| Shared export helper | Exact 11-file / 211,352-byte export, real Windows junction and Unix symlink checks pass; changed pin, nonempty output and missing immutable commit refused |
| Root Cargo format, documentation-placeholder check, hygiene and llms check | Pass |
| Full workspace `cargo clippy --workspace --all-targets -- -D warnings` | Pass on Ramani |
| Full workspace execution through managed work-run | 1,820 executed: 1,706 passed, 114 failed, 82 skipped; exact failure groups below. This is not a passing workspace gate. |
| Reserved S1 error catalog, after exact entry insertion | 9 passed, including the previously failing source-code scan |
| Final Windows process custody suite | 11 passed by the author and 11 passed independently; final actual-caller timeout and healthy follow-up passed |

The initial client harness expected empty stderr even for intentional protocol rejection. Those two harness failures are retained in the session evidence; the tests now check the exact expected diagnostic. Subsequent complete client runs passed.

The managed workspace guard also refused a root check while the fixture's newly created build cache was nested under the linked worktree. That refusal was retained. The owned cache was moved intact into the primary managed workspace, its executable hash was verified unchanged, and the empty nested parents were removed. Existing caches were not deleted. The workspace check then passed.

## Review and cleanup

Independent native review inspected all nine Rust files, the installed windows-sys 0.61.2 layouts and Microsoft API ownership rules. No blocking native API, pointer-lifetime or resource-ownership finding remains. A separate refuter classified the original subfield-pointer concern as unresolved rather than confirmed undefined behavior. The code now derives three pointers from the complete initialized packet, eliminating that specific uncertainty. See the final review record in the PR.

The probe created zero windows and retained no owned native handles. The returned high-contrast scheme allocation was released. The oracle's hidden window was destroyed and its process exited normally. The client fixture asserted child/pipe cleanup. Post-run census at 23:59:53Z found zero owned processes; all 13 pre-existing SearchHost/WhatsApp WebView2 processes remained present. The fresh oracle profile was removed. Existing caches, foreign processes, deadlines and global settings were preserved.

## Runner failure-first correction

An independent review found an unbounded post-timeout pipe drain in the original test runner. A separate refuter reproduced it with harmless owned Python processes: for a 1-second deadline, the direct parent exited at 1.019 seconds while the descendant remained alive; the command returned only at 2.697 seconds when the descendant exited naturally. That finite experiment proves an overrun and surviving descendant. It does not claim an indefinitely hung process was executed. The successful original desktop run is retained separately.

The corrected runner uses file-backed output and one exact Windows Job Object per invocation. A trusted bootstrap enrolls before starting the target and closes the inherited Job handle, leaving the parent as sole handle owner. Actual target and bootstrap identities/exits are separate. Eleven focused Windows process tests passed; they include independent retained process-handle exit observations, actual Job accounting, timeout before enrollment, runner death, and a surviving child after parent exit. A constant-zero accounting mutation failed on the independent OS census. A cleanup-query failure also exposed an unenrolled-bootstrap cleanup gap before correction; its failing receipt is retained, and the corrected case preserves the cleanup refusal while aborting and observing the exact owned bootstrap. No process name or ambient PID scan selects a termination target.

A further independent review found a stale-zero observation race: the helper read the Job count before checking bootstrap completion. A real zero could become stale while the bootstrap enrolled, spawned a held descendant and exited. The failure-first test reached that schedule through the actual timeout path and observed one live Job member despite a returned zero. The final helper polls bootstrap completion before the accepting census. The caller already refused timeout/setup-error flags; this finding was false cleanup evidence, not demonstrated acceptance of a healthy run or escape from the Job. The final independent caller exercise refused a 1-second timeout after 1.022756 seconds, observed both own processes exited near 1.014 seconds, and passed a healthy target-exit-7 follow-up with an empty Job. A separate independent refuter then replayed the actual `run_owned` timeout regression: the old helper failed with one Job member before close; the final helper passed with an independently observed zero. The refuter confirmed the failure-path-only classification and gave GO for the minimal fix. [Published review and regression receipts](evidence/20261105-review) preserve the before/after evidence.

## Full-workspace gate and environment findings

The serialized [W4 gate lease](https://github.com/gyldlab/keld/issues/1188#issuecomment-6646479768) ended with [explicit cleanup and release](https://github.com/gyldlab/keld/issues/1188#issuecomment-6646633466). The first attempt stopped after 45 passes and one formatter failure. The preserved `--no-fail-fast` attempt executed all 1,820 selected tests: 1,706 passed, 114 failed, and 82 additional cases were skipped by the existing configuration. No test expectation, platform selection, timeout or guard was weakened. Product source remained equal to main.

The source-code registry failure was caused by this fixture using S1's already reserved 049 result while main lacked its catalog row. The exact canonical row was copied after a narrow ownership claim; the complete nine-test registry binary then passed. This fixes that identified gate without changing the error number, meaning or production selection. A focused pass is not relabeled as a fresh passing whole-workspace run. The catalog addition made the generated corpus stale, and that original check failure is retained. Rebuilding the current generator adds exactly the same seven-line entry to `llms-full.txt`; `llms.txt` is unchanged. All six generator tests and the subsequent freshness check pass.

The remaining 113 failures are outside the W4 source scope, with two updater causes still unverified. The generated-app formatter sees the ancestor `/.keld-work/` ignore rule in managed temporary storage and silently selects zero files. Identical app bytes and the same pinned Bun 1.4.4 outside that ancestor correctly fail, repair, then pass; an own-app Git boundary control isolates the cause. Of the 112 Windows updater failures, 110 explicitly refuse their root because the D: volume anchor grants mutation to authenticated users (SID `S-1-5-11`). That refusal preserves the existing guard. The other two fail because `QF1_COORDINATOR_READY` was not observed; their underlying cause was not separately verified, and they are not counted as proven instances of the volume-anchor refusal. The current workspace manager fixes scratch beneath the primary checkout and provides no supported task-local scratch-root override. No ACL, global environment, ignore rule, guard, or test was altered to hide either failure. The workspace owner must reconcile the qualified fixture contexts before this local gate can pass.

After that execution, the W2 lead identified two existing corrections on its unmerged stack: [984a5d59](https://github.com/gyldlab/keld/commit/984a5d59e7a088c306c4dfa6dc9a33dc87db4448) gives the generated-app formatter fixture an explicit Git boundary plus an ignored-parent control; [05f87d5d](https://github.com/gyldlab/keld/commit/05f87d5d23770119f751edc2cffb657249339331) isolates the separate Python formatter-order fixture from an ancestor Cargo workspace. W4 read both immutable diffs and confirmed that they are in PR1400/2aed but absent from this branch's main base. Reconcile those existing corrections through their owner; no duplicate W4 source fix is included. Their targeted W2 results do not replace this branch's failed full-suite result and do not address the volume-anchor or unverified QF1 causes. The context handoff and correction are recorded on [KEL-245](https://linear.app/gyldlab-keld/issue/KEL-245/process-contain-agent-worktrees-evidence-and-scratch-under-one-local).

The final process wrapper passed both the independent harmless process checks and the fresh native sequence above. The earlier successful run and every failure-first receipt remain preserved; the final wrapper result is not inferred from the old runner.

## Exact remaining dependency

The executable evidence establishes native acquisition and the shared client boundary. It does not establish Windows Core admission or the real Host coordinator.

The shared owners must review and publish the portable S1 subset, Windows ID/units/appearance interpretation, one immutable Host-owned selection, and the separate post-Init update/invalidation path. The exact S1 router supports updates, but its coordinator currently rejects DesktopObserved after Init or a previous desktop observation.

Core must also refuse completion captured under an old role before it can reach a successor writer or pending Subscribe. Each WorkerLink starts its correlation counter at 1, so a wrong-correlation test cannot prove this generation fence. The current tests prove retired-source refusal, epoch regression rejection and fresh-client replay separately. The genuine Core desktop suite at S1 blob `61f71ac2e24591c0b6919b7f4e211cfffe4c03cf` remains unexecuted here and macOS-gated.

After those approvals, the W2 lead owns application of the 17-addition/4-removal first-Init seam and coordinated shared callers/consumer guards. The patch passed isolated context checks against PR1397 `a12b996bb75105da8e2f1b54eb8a8cbe309d5dbb` and PR1400 `2aed1f944a072d2c84469924c107b3ca5df9bc13`; it is still unapplied and not independently compilable without the dependency. Ready, authorization, 5,000-ms caller deadline and 15-second Init watchdog must retain their existing semantics. The PR remains draft/open.

## Follow-up: projected rectangle representability (2026-11-05)

An independent source-level negative control found a gap in the **test-only Windows adapter** after the original `6a92d4dd` evidence publication. The native probe and original S1 codec were not changed. A constructed physical RECT of width 100,000,000 with a positive effective DPI of 1 yields a logical width of 9,600,000,000. This width exceeds the original shared codec's `u32` dimension bound. Before correction, `nativeFacts` returned that unencodable object and consumed Host-session identity ID 1 before the codec rejected it with `KELD-IPC-003`. This is a deterministic malformed-input boundary; it does not assert that the observed Ramani monitor or Windows DPI produced this value.

The separate scratch failing-first control under the W4 session reported `allocatedIdentityCount=1` and exited 1; then the in-repository named regression failed (**0 pass, 1 fail, 18 filtered**) because no exception was raised. The narrow correction rejects nonrepresentable signed-32-bit logical origins or nonpositive/overlarge unsigned-32-bit logical dimensions inside `enclosingDip`, **before** `ids.id` may allocate an identity. It does not widen codec fields, add per-getter IPC, change native measurements, or modify shared source.

With the exact pinned shared export, the corrected adapter suite passes **19/19, 84 expectations**. The combined non-native adapter + real Windows named-pipe client suite passes **27 tests, 1 deliberately disabled native case, 0 failures**; `just fmt-check` and `git diff --check` pass. These are **new** results for the amended adapter. The two previously reported native runs remain genuine measurements of the unchanged native probe and earlier adapter; they are not relabeled as reruns of this revised adapter. The original checked-count table above is historical for the preceding published revision. Production P1–P5 and shared-owner Windows selection remain unapproved.
