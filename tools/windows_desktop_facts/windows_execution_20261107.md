# Windows Core and display-identity execution

Date: 2026-11-07. Owner: GH1186 (W4). PR1401 remains open and draft.

## Result

The real Windows Core reader-identity regression and three production allocator
tests now compile and run on Ramani. Four deliberate source defects produce their
intended assertion failures. Both source areas pass after exact restoration.

This is actual Windows crate execution. The allocator tests use controlled Target
values and call no native display APIs. The Core test uses its existing fake UI and
same-process pipe fixture with controlled caller identities. Neither test group
proves live screen geometry,
effective theme, Runtime admission, or the original application workflow.

## Source and ownership

Windows managed task:
`agent/gh-1186-w4-reader-identity-win-20261107`.

Final Windows HEAD: `624e0e21c81c48038e2c178e8e592e3c9c682125`.
Its two new commits are:

| Commit | Focused change |
| --- | --- |
| `38f1a07ad4b1edb8abfd2f8473d1d5046a528682` | Existing Core desktop test, +145/-66 |
| `624e0e21c81c48038e2c178e8e592e3c9c682125` | Existing Windows target module, test-only +109/-0 |

Retained base: `413c79baa962344652c68592a7f4fd73e9076e95`.
Combined change from that base: two test files, +254/-66. Its previously executed
recovery test remains intact. Production bytes match their original hashes.
The final task checkout is clean. These commits retain private W2 ancestry and
were not pushed as an integration branch.

The public minimal patches remain separately reviewable:

- [Core test and controls](core_reader_identity.md), published at
  `5a3537fba9f432f258a242172b0bd6aff3c23459`.
- [Allocator test patch](https://github.com/gyldlab/keld/blob/ad6d25aee604c1460ee875c1ab7798f5caa08423/tools/windows_desktop_facts/production-allocator-tests.patch),
  published at `ad6d25aee604c1460ee875c1ab7798f5caa08423`.

The exact ownership amendments are
[Core execution](https://github.com/gyldlab/keld/issues/1186#issuecomment-6699800973),
[allocator execution](https://github.com/gyldlab/keld/issues/1186#issuecomment-6700050115),
and [corrected equality control](https://github.com/gyldlab/keld/issues/1186#issuecomment-6700107275).
The [W2 coordination record](https://github.com/gyldlab/keld/issues/1188#issuecomment-6700043528)
preserves W5's separate native-dialog ownership.

## Core reader-identity result

The selected regression holds a previous reader capture and also creates a
different live reader with the current numerical attempt. Each is refused before
and after the successor's first valid sample. It checks state after every input,
the existing ordered snapshot/window/subscription output, and subsequent updates
from the legitimate current reader. Assertions release the registry lock before
failing; the successor reader is retired and joined before a panic is resumed.

```text
cargo test --locked --offline -p keld-core --lib app_session::window_router::tests::desktop::late_primary_capture_cannot_settle_successor_or_change_its_snapshot -- --exact --test-threads=1 --nocapture
```

| Phase | Actual output | Cargo exit |
| --- | --- | ---: |
| Healthy | 1 passed, 0 failed, 239 filtered | 0 |
| Remove reader pointer equality | Expected `foreign capture cannot advance pending desktop epoch`; left 2, right 1 | 101 |
| Reject the current reader | Expected `current reader must settle pending desktop sample`; left 1, right 2 | 101 |
| Restore original matcher | 1 passed, 0 failed, 239 filtered | 0 |

Each fault compiled, selected exactly one test, and failed at the intended state
assertion before the next blocking frame read. The matcher was restored byte for
byte between controls. The same positive selector passed again at final Windows
HEAD `624e0e21`, after both commits existed.

## Production allocator result

The tests call the actual `DesktopSampler::display_id` method. They distinguish all
four complete target fields, retain IDs through intervening inputs and reordered
revisits, refuse a thirty-third distinct identity without changing history, and
retain access to all previously known identities after capacity refusal.

```text
cargo test --locked --offline -p keld-wv --lib webview2::desktop_sample::target::tests::desktop_identity_ -- --test-threads=1 --nocapture
```

Controls use the exact selector
`webview2::desktop_sample::target::tests::desktop_identity_uses_each_complete_target_field`
and add `--exact` to the test arguments.

| Phase | Actual output | Cargo exit |
| --- | --- | ---: |
| Healthy | All 3 named tests pass; 177 filtered | 0 |
| Always return display ID 1 | Expected `signed adapter high`; left 1, right 2; exactly 1 test, 179 filtered | 101 |
| Ignore complete device path in Target equality | Expected `complete device path`; left 1, right 2; exactly 1 test, 179 filtered | 101 |
| Restore original allocator and equality | All 3 named tests pass; 177 filtered | 0 |

The constant-ID mutation retains allocator execution and changes only its final
ID expression. The path mutation implements equality inside Target's own module,
preserving high/low adapter and target-ID comparisons while omitting device path.
Its effect on capture's other equality use was not exercised. Both mutations were
restored exactly. All three tests passed again at final Windows HEAD `624e0e21`.

## Excluded attempts and independent review

The first Core healthy runner printed a passing test but failed to retain the
process exit code. That attempt is preserved and excluded. The corrected driver
owns `System.Diagnostics.Process`, drains both streams asynchronously, waits for
termination, and records the numeric exit before disposal.

The first path-equality control tried to read private Target fields from the
parent module. Rust rejected it with six E0616 errors; no test ran. The independent
reviewer identified the ownership defect. The attempt is preserved and excluded.
The corrected child-module control compiled and reached the intended assertion.

Independent reviewer `w4_reader_identity_review` inspected source, driver logic,
complete stdout/stderr, numeric exits, source manifests and restoration evidence.
It accepted both four-phase campaigns within the scopes above. No failed compile,
zero-test run, missing exit, unrelated panic or timeout counts as a semantic
negative control.

## Exact final byte bindings

| File or artifact | SHA256 |
| --- | --- |
| Retained Core test after patch | `39cdf8600a3214b74d0b085216ce976a6b0f8416d91f2f05980c2c81dd1e3072` |
| Original/restored reader matcher | `822fd08542962b3d5b888c64d5172f0030ced6f8146ac06b70c0fae26cc607ed` |
| Original/restored sampler | `f25b0d7ca75c7a12fdfe4354a26ddbc375b57ea51ac9e3f22d41b0d375112743` |
| Target with reviewed tests | `de4ddc3ec777ca6a1974bcb030081ba0ba76a9c6b892ad10eddfbb23eb372846` |
| Core test patch | `6ec6d8b6c3abcfae9618da59034fddb47a4cf99ed6ff271cf08ac58296aac539` |
| Allocator test patch | `221b38499ad105c597d6f0a2c9bf913d6557b54284bf65aca92641d35cce1835` |

## Evidence and cleanup

Windows evidence is retained under
`D:\WORK\keld\.git\keld-closeout\w4-reader-identity-win-20261107-0045`.
Core phase directories are `healthy_v2`, `identity_removed`, `current_rejected`
and `restored`. Allocator phases are beneath `allocator`: `healthy`,
`constant_id`, `ignore_path_v2` and `restored`. `final-head/manifest.json` records
the two final positive commands at the exact committed HEAD. All phases retain
source bindings, commands, full output, numeric exits and restoration records.

Rust and Cargo were 1.99.1. Execution used the retained
`E:\keld-w4-core-execution-20261106\target` cache, one builder and at most two jobs;
allocator and final-head checks used one job. Focused Rust formatting and Git diff
checks passed. All commands ended normally. No process was forcibly terminated.
After each phase, no executable remained beneath that owned target-cache path.
This scoped census is not a system-wide process or native-handle audit.

No native capture, GUI/window, Host/CLI/Bun launch, global display/theme change,
foreign process action, or W2/W5 worktree change occurred in these campaigns.
Existing worktrees, caches, deadlines and default-deny behavior are preserved.

## Remaining product acceptance

The live capture-refusal test from `edf052c8` still needs a coordinated native slot.
Actual acquisition, work-area/scale/theme evidence, native changes and cleanup
remain separate from these controlled-input tests. W2 must revalidate its current
source before applying the minimal test patches into its integration tree.

The [exact GH481 proposal](https://github.com/gyldlab/keld/issues/481#issuecomment-6686490913)
still requires an explicit contract disposition; this execution does not choose
new recovery or Ready semantics. D13 is already approved. Protected-tools admission
under GH1061 remains a separate gate. No merge or auto-merge is authorized here.

The associated Mac artifact session is not globally green: its workspace test run
stopped at `created_app_native_formatter_checks_repairs_and_is_idempotent`
(`unformatted app passed`), with 65 passed, 1 failed, 17 skipped and 1624 unrun.
The cause and baseline status of that failure remain unverified. Its CI run also
encountered unavailable Docker and five foreign stale workspace registrations.
Those findings do not invalidate the executed focused Windows results and are not
silently waived as full-workspace acceptance.
