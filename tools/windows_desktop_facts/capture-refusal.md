# Failed-capture identity preservation

Date: 2026-11-06. Owner: GH1186 (W4).
This is a test-only follow-up to the executed identity artifact at
`ad6d25aee604c1460ee875c1ab7798f5caa08423`. No production behavior changes.

## What the new native test checks

`production_capture_refusal_test.rs` adds one test inside the existing
`desktop_sample::target::tests` module. It reuses the original `target()`,
`history()`, `DesktopSampler::capture()` and `display_id()`.

A fresh, joined thread seeds two identities, attempts capture twice, and checks:
the exact unavailable error class; complete unchanged identity history after each
attempt; both retained IDs; and the next unused ID. The thread creates no window,
changes no DPI context and changes no display/theme setting.

The native test is explicitly opt-in:
`#[ignore = "requires a coordinated native Windows test slot"]`.
An independent review confirmed that ordinary Windows test runs cannot assume the
required display configuration. With PMv2 process awareness, the production code
checks monitor count and geometry before looking for the thread's windows. A valid
unsupported-topology refusal must not become an ordinary unit-suite failure.

The coordinated fixture must satisfy the existing single-primary conditions when
its process is PMv2 aware. The test accepts only the existing awareness-guard or
zero-window-enumeration refusal. Unexpected errors remain test failures.
A pass alone does not identify which of the two admitted branches ran.

## Executed component controls

The final campaign ran with Rust 1.99.1, warnings denied, on
`aarch64-apple-darwin`. It used the exact production capture method and the
previously pinned allocator/type fragments from retained private startup
`37da8f08c3738de9dcde0eb3b777564800a17e33`.

**Native acquisition was substituted.** The first `display::read()` returns the
selected error; later acquisition functions are unreachable tripwires.
AppDesktop/error/Geometry declarations are explicit type scaffolding.

A component-only wrapper invokes the exact native-test body and separately
requires two acquisition-boundary calls. This makes a capture function that simply
returns the expected error detectable.

| Case | Actual result |
| --- | --- |
| Substituted awareness refusal | Selected test passes |
| Substituted enumeration refusal | Selected test passes |
| Clear history before refused acquisition | Compiles; history assertion fails, exit 101 |
| Return expected error without acquisition | Compiles; call-count assertion fails, exit 101 |
| Return an unrelated error | Compiles; error assertion fails, exit 101 |
| Restore original source | Selected test passes; source identical to first baseline |

Every case discovered five tests and executed only the new component wrapper.
Positive cases report 1 passed, 0 failed, 4 filtered. Negative controls report
0 passed, 1 failed, 4 filtered, with their exact assertion message and source line.
The three older allocator tests were discovered but were not rerun.

Final receipt SHA256:
`857546dab5f290c1617cf079100f91e084b5e9dbd79df73edc81e80378163308`.
The manifest covers 85 payloads. The controller, original inputs, generated
variants, binaries and raw logs are retained under the managed session evidence
root `w4-capture-refusal-20261106-1742`, campaign-02. Campaign-01 is preserved;
the final run incorporates the review's fixture-precondition clarification.
The private controller SHA256 is
`d29bad4b4ace6229587c4ac123ee6abfc34b9efc6ddd6a255ad72993ae649326`.

Capture fragment SHA256:
`0a0ed674d057daed250dfee1174d9cd6253933fc9cde7b76f63d93bdb28f6d2a`;
1074 verbatim bytes beginning at original source line 51.

## Apply and execute on Windows

W2 owns application and the Ramani execution slot. First revalidate the current
source. Apply the existing `production-allocator-tests.patch` from ad6, then the
new `production-capture-refusal.patch`. The follow-on patch changes only the test
module and preserves all three existing tests.

| Bound artifact | SHA256 |
| --- | --- |
| Prior allocator patch | `221b38499ad105c597d6f0a2c9bf913d6557b54284bf65aca92641d35cce1835` |
| Follow-on patch preimage | `de4ddc3ec777ca6a1974bcb030081ba0ba76a9c6b892ad10eddfbb23eb372846` |
| Follow-on patch | `b645fff6749b015c4ae84d670398144c0ec419db1a048e1150cb3f1ae70a7e16` |
| Final native postimage | `7a3ad603b1c3b570999aa8fbb40f9a6beffa938b65f64ca8f0097a3b7e4875fa` |
| New test fragment | `366eb6596515ee8b249f1f3217886dd0612eeb83841e958489bea587fd4830bc` |

Actual local patch check/application reproduced that postimage. Rust formatting
checks passed. This is not Windows crate compilation or native execution.

Run through the existing managed Windows task with the explicit selector:

```text
cargo test --locked --offline -p keld-wv --lib webview2::desktop_sample::target::tests::desktop_capture_refusal_preserves_identity_history -- --exact --ignored --test-threads=1 --nocapture
```

Require exactly **one executed passing test**. A zero-test or ignored-only result
does not satisfy this criterion. The native failure-first history mutation and
restored native run must be coordinated separately; the portable controls above
do not supply those native results.

## Remaining qualification

The Windows patch is **unapplied and unrun**. This campaign proves only the
initial-refusal state path with substituted acquisition. It does not prove a
failed physical display query, successful acquisition, later target/appearance/
consistency failures, native updates, Core replay, Ready or the original app.
Full-workspace and hosted checks were not run for this isolated test artifact.

The [exact GH481 proposal](https://github.com/gyldlab/keld/issues/481#issuecomment-6686490913)
still requires owner acceptance. D13 is already approved. The
[W2 integration request](https://github.com/gyldlab/keld/issues/1188#issuecomment-6689787192)
and current native reservation remain authoritative. Do not change global
settings or repeat this unchanged component campaign as a substitute for those gates.

## Native documentation

Current Microsoft documentation confirms [EnumThreadWindows](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumthreadwindows)
enumerates the specified thread's nonchild windows and returns FALSE when none
exist. [GetThreadDpiAwarenessContext](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getthreaddpiawarenesscontext)
uses the process default if the thread has not changed its context.
Both were retrieved on 2026-11-06; Context7 discovery was confirmed against these
official sources. These documented semantics do not replace execution on Ramani.
