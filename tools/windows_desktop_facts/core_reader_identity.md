# Core desktop capture reader identity

This patch strengthens the existing Core test
`late_primary_capture_cannot_settle_successor_or_change_its_snapshot`.
It does not change production behavior or the shared desktop contract.
The owning tracker is [GH1186](https://github.com/gyldlab/keld/issues/1186).
The integration PR remains draft [PR1401](https://github.com/gyldlab/keld/pull/1401).

## Scope

`core_reader_identity.patch` changes only
`crates/keld-core/src/app_session/window_router/tests/desktop.rs` in the
Windows integration source. It reuses the existing socket-backed Core fixture,
actual native-event handler, subscription state, writer, and exact frame readers.
There is no second mirror, getter RPC, new test harness, or production fix.

The original test retires reader A and installs reader B with a different attempt
number. That does not isolate the reader-pointer check: an attempt-only matcher
can still reject A. The extension supplies a separate live reader with B's
attempt number and checks both prerequisites explicitly. It does not revive A.

Before B's pending subscription is settled, every stale/foreign payload must
leave the epoch, empty cache, unseen state, reservation owner/correlation, and
subscriber unchanged. After B receives a current sample, every stale/foreign
payload must preserve the entire cached display geometry, identity, scale, theme,
epoch, seen state, and subscription. Each phase includes unavailable, identical
or previously observed, and changed payloads. Only the subscribed phase has an
identical-current-cache case; the pending phase has no B cache to match.

Current B must then publish successfully in both phases. State assertions precede
pipe reads so rejecting all captures fails at a semantic assertion. Exact next
frames check desktop snapshot, surviving-window replay, and subscription reply
order. A correlated unsubscribed-call refusal fences the pending phase. An
idempotent Subscribe acknowledgement fences the subscribed phase and the final
current update. Extra queued frames cannot be mistaken for these correlated
replies. No sleep or guessed Ready error code is used as a silence check.

State checks copy owned values while holding the registry lock, then release the
lock before asserting. The proof catches assertion failures and I/O errors so the
existing `retire_reader` helper can stop and join the real successor reader before
the result is returned or the assertion panic resumes. A negative control must
not poison the lock needed for that cleanup.

The fixture uses fixed data to test Core routing. Those values do not prove actual
display enumeration, DPI conversion, effective theme, authenticated Runtime
admission, native windows, or native cleanup. Those remain separate Windows
acceptance requirements. No global settings change is part of this test.

## Source binding

Prepared from complete direct-text reads of the W2 integration tree
`D:\WORK\keld\.keld-work\worktrees\gh-1188-first-window-desktop-20261106`.
The branch ref observed during the read was
`4cc0f187a935b5a21969ff651eecb2adbea7a00e`; the tree also contains private
integration changes. This is not a clean-tree or atomic-snapshot claim.

| File | SHA256 of LF UTF-8 export |
| --- | --- |
| Original Core desktop test | `9ad05a6ea25acb414f7dad988421e63794524c026b0cdcbde937a6fda7b17a68` |
| Patched Core desktop test | `ea1430e3715c99dafe2c829335bdd81cb48deee00b12d2ae52c6c57495a5f9fd` |
| Original `crates/keld-wv/src/engine/desktop.rs` | `822fd08542962b3d5b888c64d5172f0030ced6f8146ac06b70c0fae26cc607ed` |
| Retained W4 Core test at `413c79b` | `abe77fa9e93093d73d49649262fdbd2bedfab361cdd0e8da096b6b74c80f89f8` |
| Patched retained W4 Core test | `39cdf8600a3214b74d0b085216ce976a6b0f8416d91f2f05980c2c81dd1e3072` |

These export hashes are not fresh raw Windows-byte hashes. The execution owner
must bind the actual source, patch bytes, and selected test before applying the
patch. A mismatch needs inspection and renewed review; do not force application
or silently normalize the source.

The retained W4 file contains the exact original test prefix plus the earlier
recovery test. A plain patch application fails because that file has added text
after the original end-of-file. The patch includes exact Git base/result blob
identities for a three-way application on that known base. Mac verification
applied and reversed it cleanly and compared the complete result with the new
test plus the byte-identical retained recovery test. Any other source difference
or merge conflict requires review. No automatic conflict resolution is approved.

## Windows execution

Coordinate the Ramani slot on
[GH1188](https://github.com/gyldlab/keld/issues/1188#issuecomment-6699319769).
Use an isolated W4 test tree or have the W2 owner apply the test patch. W4 must not
edit the lead's worktree. Preserve existing evidence and build caches. The batch
starts no Host/CLI app and changes no desktop settings.

For the original 219-line test file, use `git apply --check` and then apply
`core_reader_identity.patch`. For the retained W4 `413c79b` file, verify the
recorded source hash and base blob `12c5cfa511e8a2c0243a1e146fa574074c9ea2c3`,
then use `git apply --3way --check` and `git apply --3way`. This stages the test
change. Require the exact resulting hash above before execution. Do not touch
other staged changes. Both control patches still use strict ordinary application.

Run this one exact selector in the Windows integration workspace:

```text
cargo test --locked --offline -p keld-core --lib app_session::window_router::tests::desktop::late_primary_capture_cannot_settle_successor_or_change_its_snapshot -- --exact --test-threads=1 --nocapture
```

Retain the complete command, source manifest, exit code, test name/count, and
output for each stage. Zero selected tests is not a pass.

1. Healthy source: exactly this test must pass.
2. Apply `core_reader_identity_identity_removed.control.patch`. It removes only
   reader-pointer equality from `AppDesktopCapture::matches`; the attempt and
   live-reader requirements remain. Compilation must succeed and the selected
   test must fail with `foreign capture cannot advance pending desktop epoch`
   (observed epoch 2, expected 1). Reverse the control and verify byte-exact
   restoration before the next stage.
3. Apply `core_reader_identity_current_rejected.control.patch`. It adds an opaque
   false predicate to the same matcher. Compilation must succeed and the selected
   test must fail with `current reader must settle pending desktop sample`
   (observed epoch 1, expected 2), before the next pipe read. Reverse the control
   and verify byte-exact restoration.
4. Restored source: exactly this test must pass again. Verify the capture source
   equals the recorded original bytes and retain only the reviewed test change.

A build failure, timeout, unrelated panic, zero-test selection, or failure in a
different test does not satisfy either negative control. Do not commit control
mutations into integration source. If a stage cannot finish, restore only the
owned mutation, preserve the failure evidence, and identify the unfinished stage.

## Evidence and remaining acceptance

Mac preparation passed strict application, exact patched hash, Rust formatting,
strict reverse application, and byte-exact restoration for all three patches.
Evidence is retained in the managed session
`w4-reader-identity-20261107-0047`, including `patch-manifest.json` and
`patch-verification.json`. The retained-tree merge/restoration proof is recorded
in `owned-tree-patch-verification.json`. This verifies the artifacts; it is not a
Rust typecheck or a Windows test result.

Actual Windows execution and both behavioral negative controls are pending a
coordinated slot. The earlier Windows Core recovery test at commit
`413c79baa962344652c68592a7f4fd73e9076e95` remains separate, valid evidence for
its recorded scope. This patch neither reruns nor replaces that result.

The exact shared recovery-policy disposition in GH481, existing GH446 startup
ordering, protected-tools admission in GH1061, and complete native display/theme
integration keep their own owners and gates. This test does not approve or alter
them. Production capture/identity artifacts and the source-name fixture fix
remain preserved on their previously published branches.
