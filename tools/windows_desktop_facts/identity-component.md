# Executed Windows display-identity component controls

Date: 2026-11-06. Owner: GH1186 (W4). No product boundary change.

## Result and scope

The exact production `DesktopSampler::display_id` method, its sampler declaration,
the real four-field `Target` with derived equality, and `unavailable()` execute as
verbatim extracted fragments. The existing reviewed three-test module is unchanged.

The component campaign passed on macOS with Rust 1.99.1, host
`aarch64-apple-darwin`, using warnings-denied compilation:

| Variant | Actual result |
| --- | --- |
| Original fragments | All 3 selected tests pass, exit 0 |
| Constant ID | Named distinct-identity assertion fails, exit 101 |
| Ignore signed adapter high word | Named high-word assertion fails, exit 101 |
| Ignore adapter low word | Named low-word assertion fails, exit 101 |
| Ignore target ID | Named target-ID assertion fails, exit 101 |
| Ignore complete device path | Named full-path assertion fails, exit 101 |
| Forget an already-known identity | Named history assertion fails, exit 101 |
| Admit a 33rd identity | Named capacity assertion fails, exit 101 |
| Mutate history before refusing capacity | Named full-history assertion fails, exit 101 |
| Restore original fragments | All 3 selected tests pass, exit 0 |

Every controlled defect compiled. Each negative control ran exactly one intended
test (two filtered), and its failure message and generated source line were verified.
The original and restored generated files are byte-identical. These are eight
semantic failures, not failed compiles or skipped tests.

The campaign receipt SHA256 is
`2bdbfaa4fe40871720ce495b316db7b4052ad759ce9cf0f2db34eabe08bc9ba6`.
Its manifest covers 132 payloads, independently checked after execution.
The driver SHA256 is
`5c2cc8916e41164dc5eed9a9518db0ed6930695f85b582e37f7b6a57bb05f220`.

## Exact source binding

Source is the privately retained W2 startup commit
`37da8f08c3738de9dcde0eb3b777564800a17e33`. That SHA is not represented
as a publicly fetchable GitHub commit.

| Input under the source export | SHA256 |
| --- | --- |
| `desktop_sample.rs` | `f25b0d7ca75c7a12fdfe4354a26ddbc375b57ea51ac9e3f22d41b0d375112743` |
| `desktop_sample/target.rs` | `59444b0aa68ca8630b7ef32666217df7ac3a33eefecb717363ac0b439d466122` |
| `production_identity_tests.rs` | `b946e759f00711589fa11d19f65b5f6b8f3e0ec07114647d525aaea364300f2b` |

The driver pins complete files and all four extracted fragments. It records byte
spans, hashes, test discovery, commands, compiler version, binary hashes and raw
outputs. It adds only module framing and an explicitly labeled
`WvError::Window(String)` type scaffold. It does not add another allocator,
reimplement equality, widen production visibility or refactor native code.

A separate complete text comparison at 16:17:57 UTC found these two files unchanged
in the live W2 startup tree at branch ref `4cc0f187`. W2's tree has ongoing
integration changes. That observation is normalized text equality, not a new
raw-byte hash measurement, clean-tree claim or current-head native qualification.

## Reproduce the component campaign

Use the retained exact source export. The source tree is read-only to this driver.
Use a fresh output directory and the actual Rust 1.99.1 toolchain.

```sh
RUSTUP_TOOLCHAIN=1.99.1 python3 tools/windows_desktop_facts/verify_display_identity.py \
  --source-root <source-export> \
  --tests tools/windows_desktop_facts/production_identity_tests.rs \
  --out <new-evidence-directory> \
  --rustc <rustc-path>
```

Run through the existing managed task runner when used inside KELD. No Cargo
workspace dependencies or native APIs are required for this component proof.

Five refusal controls separately passed: modified sampler, modified target,
modified tests, an existing output directory and optimized Python. None invoked
the compiler sentinel. A sixth correct-input control reached that sentinel,
proving the invocation detector was live. Existing output was preserved.
Admission receipt SHA256:
`a30249387b0b84a61571439fe836b50ec5e389f0b91f81d681f123629c41dfdf`.

## Minimal native integration patch

`production-allocator-tests.patch` is the existing +109/-0 test-only addition
to `crates/keld-wv/src/webview2/desktop_sample/target.rs`.
Its SHA256 remains
`221b38499ad105c597d6f0a2c9bf913d6557b54284bf65aca92641d35cce1835`.
The three tested bodies match its exact appended module; reconstructed postimage:
`de4ddc3ec777ca6a1974bcb030081ba0ba76a9c6b892ad10eddfbb23eb372846`.

W2 remains the native integration/execution owner. Revalidate the actual current
preimage before applying. The Windows selector prefix is
`webview2::desktop_sample::target::tests::desktop_identity_` under
`cargo test --locked --offline -p keld-wv --lib`; require exactly three tests.
Do not treat a quiet machine or an expired observation as a new execution grant.

## Qualification that remains open

This result is **production allocator component evidence off Windows**.
The Windows crate tests are still unapplied and unrun. The proof does not execute
native target acquisition, UTF-16/path normalization, failed capture before
allocation, physical display changes, appearance, Core subscription/replay,
public Ready, cleanup, or the original draw.io app.

The existing single mirror and getter behavior are unchanged. No production
source, global display/theme setting, foreign process, cache, or W2 worktree was
modified. No new owner approval or full-workspace/CI PASS is claimed.

The [exact GH481 decision request](https://github.com/gyldlab/keld/issues/481#issuecomment-6686490913)
still owns recovery/identity/units/appearance acceptance. The
[W2 patch handoff](https://github.com/gyldlab/keld/issues/1188#issuecomment-6686761728)
owns native application and execution. Prior actual Core and native observations
retain their original scope. Do not rerun this unchanged component campaign as a
substitute for those remaining product checks.
