# Windows desktop facts: standalone W4 feasibility

This directory contains executable native evidence and a tested contract proposal for [GH1186](https://github.com/gyldlab/keld/issues/1186). It does not enable a shipping Windows screen/theme feature. [GH481](https://github.com/gyldlab/keld/issues/481) owns the shared desktop mirror; [GH446](https://github.com/gyldlab/keld/issues/446) owns Ready ordering; the [GH1188/W2 lead](https://github.com/gyldlab/keld/issues/1188) owns the native-loop and Core entry points.

| Component | Purpose and boundary |
| --- | --- |
| `native/` | Standalone Rust probe: actual Windows display paths, physical rectangles, DPI, work area, and theme inputs. It creates no window and writes no global setting. |
| `text_scale.ps1` | Reads actual Windows Runtime (WinRT) UISettings.TextScaleFactor in Windows PowerShell Desktop, on a single-threaded apartment (STA). This is a native sample and needs the same serialized slot. |
| `native_adapter.ts` | Test-only conversion of validated native evidence to the existing shared schema. Its bounded identity association is not a desktop mirror. |
| `native_adapter.test.ts` | Synthetic conversion, identity, refusal, and existing-mirror controls. Synthetic values are not native receipts. |
| `subscribe_contract.test.ts`, `subscribe_support/` | Existing S1 LifecycleLink, WorkerLink, codec, and WindowSource over a real Windows named pipe. The Host peer is scripted. One separately gated case takes the first native sample after a valid held Subscribe. |
| `shared-pin.json`, `prepare_shared.py` | Verify and export exact immutable shared source. No shared implementation copy is committed here. |
| `run_native.py`, `verify_native.test.ts`, `oracle.cjs` | Owned native run, fresh receipt assertions and preserved independent Electron oracle. The held Subscribe test is the first native query. |
| `owned_process.py`, `test_owned_process.py` | Test-only Windows Job ownership, bounded cleanup, and actual harmless process regressions. No production Host or security policy is replaced. |
| `integration.patch`, `integration-notes.md` | Unapplied shared-owner handoff, dependencies, and review decisions. |

## 1. Prepare the existing shared implementation

Use Python 3.10 or later, Git, and the repository-selected Bun/Rust tools. The helper uses only the Python standard library. On Windows it uses Windows PowerShell to create a directory junction, which lets the exported source resolve `@keld/kipc`.

The required shared commit is exactly:

```text
e09b520eaee1e4bd69bd0bde2137ec52f0fed18a
```

The helper verifies every path and Git blob ID in `shared-pin.json`, then verifies the returned bytes using Git's blob hash formula. It reads immutable Git objects directly, so checkout line endings, local filters, or modified owner files cannot affect the export. All source hashes are checked before it creates output. `--out` must be absent or an empty real directory; an existing populated directory or link is refused.

Run from this task worktree when the source repository contains the commit:

```powershell
$sourceRepo = 'D:\WORK\keld'
$shared = 'D:\WORK\keld\.git\keld-closeout\chatgpt-w4-desktop-20261105\shared-s1-fresh'
python -B tools/windows_desktop_facts/prepare_shared.py --repo $sourceRepo --out $shared
if ($LASTEXITCODE -ne 0) { throw 'Shared export failed; do not run the fixture against other bytes.' }
$env:W4_SHARED_ROOT = $shared
```

Successful output is a JSON receipt containing the immutable commit, file/byte counts, output directory, and package-link kind. The helper creates `node_modules/@keld/kipc` as a junction on Windows or a directory symlink elsewhere. It does not fetch, change branches, or write to the source repository. An I/O failure can leave a partial destination; use a new empty destination after diagnosing the error.

### Current missing-object dependency

At this task's inspection, Ramani's `D:\WORK\keld` did **not** contain e09b. The implementation existed in the Mac owner's repository. A different commit, a new mirror, or source pasted from an unverified branch is not a substitute.

The owner can export from the repository that has the immutable object, with the helper and pin file supplied as standalone inputs. For example, this is an **owner-run command**, not a request to modify its checkout:

```sh
python3 -B /path/to/prepare_shared.py \
  --pin /path/to/shared-pin.json \
  --repo /Users/centillionaire/WORK/keld \
  --out /tmp/gh1186-shared-s1-fresh
```

Transfer only the regular exported files with their bytes unchanged. Recreate the package link on Windows rather than transferring an absolute Mac symlink. After placing those files at the chosen `$shared` path:

```powershell
New-Item -ItemType Directory -Path (Join-Path $shared 'node_modules\@keld') | Out-Null
New-Item -ItemType Junction -Path (Join-Path $shared 'node_modules\@keld\kipc') -Value (Join-Path $shared 'packages\@keld\kipc') | Out-Null
$env:W4_SHARED_ROOT = $shared
bun test tools/windows_desktop_facts/native_adapter.test.ts
if ($LASTEXITCODE -ne 0) { throw 'Transferred shared source or adapter verification failed.' }
```

The adapter's loader checks all 11 pinned blobs before importing shared code. A missing or changed dependency fails the test.

The helper itself was tested on Ramani using an isolated Git object fixture under the task's closeout directory. The fixture contains the exact read-only commit/tree objects and already verified shared blobs; it does not alter the live Git repository or the Mac owner. Its receipt is:

```text
D:\WORK\keld\.git\keld-closeout\chatgpt-w4-desktop-20261105\prepare-shared-check-20261105\receipt.json
```

Executed helper controls were: changed transport dependency hash rejected before output creation; 11-file, 211,352-byte exact export accepted; Windows junction verified; nonempty destination refused; and actual Ramani repository's missing commit refused before output creation. The non-Windows symlink branch was also exercised in a temporary directory and cleaned up.

## 2. Build and run tests that do not sample native state

The native crate is standalone and outside the KELD workspace. Use the task-owned target directory; preserve existing caches. These build commands and its pure unit tests do not require the GUI/native slot:

```powershell
$commonGit = git rev-parse --path-format=absolute --git-common-dir
$target = Join-Path ([IO.Path]::GetDirectoryName($commonGit)) '.keld-work\build\gh1186-w4-native-20261105'
cargo build --manifest-path tools/windows_desktop_facts/native/Cargo.toml --locked --offline --target-dir $target
if ($LASTEXITCODE -ne 0) { throw 'Native fixture build failed.' }
cargo test --manifest-path tools/windows_desktop_facts/native/Cargo.toml --locked --offline --target-dir $target
if ($LASTEXITCODE -ne 0) { throw 'Pure native-crate tests failed.' }
```

The native crate uses the repository's pinned dependencies and local caches. An offline dependency failure remains a dependency failure; do not substitute a different version or another agent's target directory.

Set `W4_SHARED_ROOT` to the verified export, leave native sampling disabled, and run each boundary separately:

```powershell
Remove-Item Env:W4_RUN_NATIVE -ErrorAction SilentlyContinue

bun test tools/windows_desktop_facts/native_adapter.test.ts
if ($LASTEXITCODE -ne 0) { throw 'Adapter/shared-mirror controls failed.' }

bun test "$env:W4_SHARED_ROOT/packages/@keld/api/src/window-desktop-source.test.ts"
if ($LASTEXITCODE -ne 0) { throw 'Pinned shared baseline failed.' }

bun test tools/windows_desktop_facts/subscribe_contract.test.ts
if ($LASTEXITCODE -ne 0) { throw 'Real named-pipe client contract failed.' }
```

The real-pipe cases run only on Windows with a shared export selected. They exercise the original 5,000-ms caller timeout; allow that case to run to its actual deadline. The separate native case is intentionally skipped while `W4_RUN_NATIVE` is absent. A skipped Windows/client/native case is not a pass.

These client cases exercise valid Subscribe before a first sample, correlated replay, ordered updates, fresh-role replay, retired-source refusal, wrong HELLO token, and the existing receive-policy denial. They use the actual shared client implementation. The scripted Host does not establish production Core admission, Core generation installation, or the Windows Host Ready coordinator.

The process ownership regressions launch only harmless Python children and do not query the desktop. They may run alongside code work:

```powershell
python -B -m unittest discover -s tools/windows_desktop_facts -p test_owned_process.py -v
if ($LASTEXITCODE -ne 0) { throw 'Owned-process regression failed; retain its evidence.' }
```

On non-Windows hosts these actual Job Object tests explicitly skip; a skip is not Windows execution evidence. The final Windows suite covers ordinary and failed launch, enrollment refusal, timeout before enrollment, descendants retaining output handles, ordinary child shutdown tails, forced cleanup, runner death, and cleanup-query failure.

## 3. Reserve the Ramani native-test slot

Every execution of the probe binary or `text_scale.ps1` reads native desktop state. Reserve a serialized slot with the GH1188 lead before these commands, including controls. Check current ownership and running tests; do not terminate another agent's process.

`W4_RUN_NATIVE=1` is only a test fixture's opt-in. It is not a slot reservation, permission grant, production selector, or Windows feature activation.

After the slot is confirmed, run the complete owned sequence with fresh evidence output. The first native query occurs inside the held-Subscribe test. The runner then reacquires facts in another process, executes the unaware and three invalid-input controls, runs the oracle and verifies the receipts.

The oracle executable must be the preserved Electron 44.4.5 binary with SHA-256 `bd14928e0728366fd3f41499cb398ff3f4304dab259a3e605077899a6f8c748e`. For this run it was extracted into an owned directory from the existing official archive cache, verified as SHA-256 `11c395820a5aaa8ebcc0686b476d0ac98a730274ebfbdc8cf5538a7c2815cb5d`. The original cache was left intact. The runtime uses a private profile and removes `ELECTRON_RUN_AS_NODE` only from its child environment.

```powershell
$env:W4_RUN_NATIVE = '1'
$probe = Join-Path $target 'debug\keld-windows-desktop-facts-probe.exe'
$oracle = 'D:\WORK\keld\.git\keld-closeout\chatgpt-w4-desktop-20261105\electron-44.4.5\electron.exe'
$out = 'D:\WORK\keld\.git\keld-closeout\chatgpt-w4-desktop-20261105\native-run-01'
python -B tools/windows_desktop_facts/run_native.py --probe $probe --shared $env:W4_SHARED_ROOT --oracle-exe $oracle --out $out
if ($LASTEXITCODE -ne 0) { throw 'Native fixture failed; preserve all outputs before diagnosing.' }
```

The runner requires a new output directory. It preserves file-backed stdout/stderr, input hashes, actual target and bootstrap process IDs, separately observed exit status, and ownership receipts. It retains the existing 5,000-ms client deadline and the runner's 25-second native-Subscribe, 10-second probe, 65-second oracle and 15-second receipt-check execution deadlines. The oracle's existing 60-second JavaScript timer is cooperative; the process owner supplies the external bound. Each invocation has one additional fixed 3-second cleanup budget. Production's separate 15-second Init watchdog is unchanged.

A trusted Python bootstrap joins a fresh unnamed Windows Job Object before it starts the actual command, then closes its inherited Job handle. The parent keeps the sole handle; ordinary CreateProcess descendants remain in that Job. Output files remove the inherited-pipe drain hazard. Success requires an observed empty Job and an exited bootstrap without timeout, forced cleanup or setup error. Cleanup failure remains a failure even after an abort attempt. The implementation uses only the exact owned Job and process handles, never a PID/name scan to terminate processes. The caller still records a post-slot census.

For diagnosis within a reserved slot, the native CLI is `--mode pmv2|unaware`, optionally `--control invalid-monitor|failed-dpi|high-contrast-size`. The individual native client selection is `bun test tools/windows_desktop_facts/subscribe_contract.test.ts -t 'W4 native first sample'`, with `W4_NATIVE_PROBE`, `W4_TEXT_SCALE_SCRIPT`, and `W4_SUBSCRIBE_NATIVE_RECEIPT` set to owned paths. Do not run preliminary native queries when collecting the first-sample proof.

Use fresh receipt filenames for a new evidence run rather than overwriting an earlier receipt. Read each probe JSON result and its exit code. Success exits 0, expected native errors exit 1, and a control that unexpectedly succeeds exits 3. A nonzero code alone is not sufficient evidence:

| Case | Required observation |
| --- | --- |
| `pmv2` | Exact per-monitor-aware-v2 context; actual native sample; physical-mode and independent work-area cross-checks. |
| `unaware` | DPI-unaware/virtualized units declared. The bounded adapter refuses these as physical facts. This control does not change machine settings. |
| `invalid-monitor` | Error source `GetMonitorInfoW`; no sample/default facts. |
| `failed-dpi` | Error source `GetDpiForMonitor`; no sample/default DPI. |
| `high-contrast-size` | Error source `SystemParametersInfoW(SPI_GETHIGHCONTRAST)`; no sample/default appearance. |
| Native Subscribe test | Actual native and WinRT sampling starts after the valid held Subscribe, and the shared desktop is applied before correlated acceptance and facade Ready in the test peer. |

After the owned tests finish, release the coordinated slot and remove the fixture opt-in:

```powershell
Remove-Item Env:W4_RUN_NATIVE -ErrorAction SilentlyContinue
```

The probe creates no HWND, retains no owned monitor handle, and installs no observer. It changes only its own new process's DPI awareness. Its HIGHCONTRAST scheme buffer is released before output. The client fixture cleans up its own child and pipe. Confirm the recorded cleanup and owned-process completion; never use a broad process kill as cleanup. See [native/README.md](native/README.md) for API references and detailed native result semantics.

## 4. Scope and integration gates

The adapter currently accepts exactly one logical primary display, a validated complete target identity, equal positive effective DPI axes, actual WinRT text scale, independent work-area/physical-unit checks, and non-forced-colors appearance. Multiple displays, forced colors, missing identity, invalid text scale, and native failures remain unavailable. Raw probe data retains native paths and raw appearance inputs even where the bounded conversion refuses a projected fact.

The proposed nonzero u32 display ID is an opaque Host-session association with the full canonical native target set. It is stable across removal/reorder/role recovery within that session; its 32-entry lifetime bound refuses a 33rd identity without eviction. It does not claim Electron numeric identity or cross-reboot identity.

This one-shot native evidence does not prove global notification delivery, mixed-DPI multi-monitor layout, both physical appearance modes, production startup integration, or production Core recovery. Ordered updates and replay at the shared client boundary use clearly labeled scripted facts. No display, theme, registry, or accessibility setting is changed to manufacture evidence.

The current conditional `integration.patch` has 20 additions and 5 removals. It was tested against PR1403 `fe9a4ab8d69823c8ead58281ab1715a811a75835` and bound to current head `f7b5848556154aa1c356a214270e342369267c0e` by identical native-loop source bytes. The patch remains unapplied and depends on shared S1 composition, narrow desktop cfg changes, immutable Windows selection, and the approved first-sample ordering. W2 must recheck its source before applying it.

The exact S1 coordinator admits only its initial desktop event; its router's later-update support is not proof of a production post-Init delivery path. That path, Windows identity/units/appearance conversion, and Windows activation require the shared owners' review. Read [integration-notes.md](integration-notes.md) for the precise dependency map. Keep the work draft/open; no merge or auto-merge is part of this fixture.
## 5. Replay the conditional first-Init helper proof

`verify_init_seam.py` extracts the exact private helper from a pinned Git source after applying each actual patch in an isolated directory. `init_seam_harness.rs` compiles it with a deterministic clock and Rust's real channel. The retained `integration-v2-negative.patch` is an intentionally failing historical proposal, never a production patch.

With a source repository containing the reviewed immutable object, run:

```powershell
python -B tools/windows_desktop_facts/verify_init_seam.py --repo D:\WORK\keld --source-ref ec493b0bc16250a19dd4cc0d8615d7870a364748 --expected-source-sha256 03bc98199f767d24cce99e9f1db3c242301491ad07ddca7fb91d3d2e8a53c370 --patch tools/windows_desktop_facts/integration.patch --red-patch tools/windows_desktop_facts/integration-v2-negative.patch --harness tools/windows_desktop_facts/init_seam_harness.rs --out D:\WORK\keld\.git\keld-closeout\chatgpt-w4-desktop-20261105\init-seam-fresh
if ($LASTEXITCODE -ne 0) { throw 'First-Init helper proof failed; preserve its receipts.' }
```

Use a new output directory. The driver requires the exact reviewed source hash and records the compiler, patched input, generated source, binary, commands and test results. Expected outcomes are v2: one failed timing assertion; v3: eight passing cases; timestamp-before-capture mutation: one failed slow-capture assertion; no-op mutation: all eight cases fail. A compiler or selection error is not an expected negative result.

The proof establishes capture completion before timestamping, then desktop enqueue before Init enqueue. It also distinguishes an omitted callback from unavailable facts and refuses disconnected receivers. It injects time to expose the queue boundary; it does not claim real scheduler preemption. The native first-Init guard and `.take()` call site are inspected as source, not executed by this harness. Windows APIs, Core generation admission, facade Ready and the real deadlines require their separate owner-integrated tests.
