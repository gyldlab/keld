# W4 current Windows recovery test packet

Dated 2026-11-06. This packet characterizes the existing W2 implementation and supplies an unapplied Core regression. No production boundary change or shared-contract approval is made.

## Source binding

Original immutable W2 startup source: `cd87be23ea375118224f9687894ad24e7179cc73`.
The newer startup successor `37da8f08c3738de9dcde0eb3b777564800a17e33` was clean at readback. Seven relevant source files, including the producer, capture identity, Core router, registry and test helpers, were independently read with Git and are byte-identical across these heads.

The producer driver refuses input that does not match both pinned SHA-256 values before compilation:

- `crates/keld-wv/src/webview2/desktop_session.rs`: `5d5fcfa0288c257b78f3fb13943be5607e66673193289cc45917b9820d7f257a`
- `crates/keld-wv/src/engine/desktop.rs`: `822fd08542962b3d5b888c64d5172f0030ced6f8146ac06b70c0fae26cc607ed`

## Executed producer proof

Native macOS, Rust 1.99.1. Both original modules compile unchanged with `rustc --edition=2024 --test -Dwarnings`. Only OS acquisition, the enclosing event enum/error and the simple Init-forwarding seam are supplied by the harness. Acquisition values and failures are explicitly synthetic. The real capture identity, atomics, producer orchestration and standard-library channel execute.

Six tests passed. Five isolated semantic mutations each compiled successfully and produced exactly one intended failed assertion, exit 101:

| Mutation | Detection |
| --- | --- |
| Remove replacement refresh | Replacement must actually request another acquisition |
| Accept a retired bind | A delayed old bind must not displace B |
| Remove reader-pointer equality | An equal attempt number must not authorize a different reader |
| Replace refresh with a no-op | Live B must still publish after retired A is ignored |
| Suppress unavailable observations | B's failed acquisition must publish B-owned absence, not silently retain a prior value |

The final execution receipt SHA-256 is `5da935aaa90583bf22abbba5fe815acd4bc12c59bd4aff0e922798c0453f0e51`. A separate one-byte source corruption was refused before output-directory creation. Rustfmt check and Python syntax validation passed.

The earlier receipt `aff15c6fc721e88ac3590937c5fbae879ef950611ff0e89739e3142a51301522` used the machine's default Rust 1.97.1; its original logs remain retained. The final run explicitly selected repository Rust 1.99.1. Independent review also found that optimized Python bypassed the driver's assertions. The old driver actually accepted a mismatched source under `PYTHONOPTIMIZE=1`; the corrected driver rejects optimized execution before reading inputs or starting a compiler. Both optimized healthy/corrupt inputs and normal corrupt input now refuse with exit 1. Exact transported log bytes are hash-checked against the native receipts, including final newlines.

Reproduce against an export of the two exact paths:

```sh
RUSTUP_TOOLCHAIN=1.99.1 python3 -B tools/windows_desktop_facts/verify_recovery_session.py \
  --source-root /path/to/exact-source-export \
  --harness tools/windows_desktop_facts/recovery_session_harness.rs \
  --out /path/to/new-owned-evidence
```

The output directory must not already exist. All negative results remain separate from the passing current-source run. The driver's successful aggregate status means the expected negative tests failed; it does not label those mutants successful.

## Unapplied Windows Core regression

`recovery-contract.patch` adds 167 lines to one existing Core test module and changes no production file. Its SHA-256 is `a7289719b50ad18fba6d44d6335ab5ce65f1aa5720864df6e089f7d397001e9c`. The original target SHA-256 is `9ad05a6ea25acb414f7dad988421e63794524c026b0cdcbde937a6fda7b17a68`; the formatted patched target is `abe77fa9e93093d73d49649262fdbd2bedfab361cdd0e8da096b6b74c80f89f8`.

Actual `git apply --check --whitespace=error`, application to a private pristine copy, byte comparison and Rustfmt parsing passed. **Windows compilation and execution of this new Core test have not run.**

The test reserves A's Subscribe before its first synthetic sample, creates a survivor, holds complete A results before retirement, and lets B reuse the same correlation. It then checks current-policy correlated 049 errors without desktop/survivor replay, healthy B recovery in desktop → Created → Subscribed order, and stale-A refusal before and after B succeeds. Correlated replies form ordered barriers. Assertions use owned state snapshots after releasing locks; successor-reader retirement/join runs before propagating assertion failures.

Only W2 should apply and execute it on its current startup tree after confirming source hashes and serialized machine admission:

```powershell
git apply --check --whitespace=error C:\path\to\recovery-contract.patch
git apply C:\path\to\recovery-contract.patch
cargo test --locked -p keld-core --lib replacement_query_failure_refuses_replay_until_new_valid_capture -- --test-threads=1 --nocapture
```

Require exactly one selected test; zero tests is invalid evidence. The patch is a characterization of current behavior, not an instruction to approve that behavior.

## Exact decision and limits

The current native producer re-samples for the replacement and carries its failure as a B-owned unavailable observation. The Core source clears the previous cache on retirement. The separate Core patch is required to execute the full reply/replay consequence.

GH481 must explicitly select Host-cache retention with defined invalidation semantics, or the existing replacement-resample behavior including its transient-failure consequence. D13 selection on GH446 is already approved and must not be requested again. A valid producer test does not select between those policies.

This packet does not execute Windows APIs, the real Core writer, authenticated Subscribe, public Ready, the Init deadline, GUI/native cleanup, or a real display/theme transition. It does not requalify historical native04. No Ramani build/native/GUI process, global setting, W2 source or shared entrypoint was changed. The W2 owner retains integration and full Windows acceptance.
