# GH-1404 — native Intel and Universal CI qualification proposal

**Status:** review proposal and preparatory preflight only. No protected workflow or CI-router edits. No release, merge, AOT, or new DMG artifact.

**Source:** W1 `d9805ea48cb4beba9e26d29c46b0b6519b7e62dc`. Owner: independent macOS CI lane. New isolated branch `agent/gh-1404-universal-ci-lane2-20261124` was created from that exact commit. The W1 tree remains unchanged.

## Native runner evidence

The official [`actions/runner-images` runner-label table](https://github.com/actions/runner-images/blob/6d72cf29b31dca29cc53e9d4046505fbdfd32210/README.md) (retrieved 2026-11-24) lists these GitHub-hosted labels:

| Native check | GitHub-hosted runner | `RUNNER_ARCH` | `uname -m` | macOS major |
| --- | --- | --- | --- | --- |
| Intel Sequoia | `macos-15-intel` | `X64` | `x86_64` | `15` |
| Apple Silicon Sequoia | `macos-15` | `ARM64` | `arm64` | `15` |
| Intel Tahoe | `macos-26-intel` | `X64` | `x86_64` | `26` |
| Apple Silicon Tahoe | `macos-26` | `ARM64` | `arm64` | `26` |

The preparatory Python preflight requires an observed Darwin OS, exact version/architecture and real `GITHUB_ACTIONS=true`, `RUNNER_OS=macOS`, and matching `RUNNER_ARCH`. It rejects a translated Rosetta process. A synthetic environment in unit tests only proves parameter validation; it is **not** hosted Intel execution.

## Existing owners and hard gates

The Core slice reader at `crates/keld-core/src/app_session/macos_installed/macho.rs` is W1's authoritative bounds/subtype/read-in-place implementation. Pack owns target-to-slice selection and signed inputs in `crates/keld-pack/src/macos/{assemble.rs,tests.rs}`. The metadata owner at `crates/keld-pack/src/macos_metadata.rs` specifies `Contents/MacOS/keld-host`, `Contents/Helpers/bun` and the closed `KELDLocalBundle.target=macos-universal` and `bunSha256` in `Contents/Info.plist`. A `lipo` preflight is evidence only; it does not replace retained-file Core admission.

GitHub CI policy (`.agents/ci.md`) requires: PR and push workflows always exist; no workflow-level path filters; job-level selection uses `needs` without `matrix`; all Actions have immutable SHA pins; gitleaks stays unconditional; `CI required` consumes and rejects selected missing/skipped/failed/cancelled results; and conditional router test cases cover relevant, unrelated and unknown inputs. No `continue-on-error`, waivers, retries or failed-status suppression is acceptable. CI edits need a separate independently reviewed CI-owner change.

## Prepared preflight and how to invoke after qualification

`tools/gh1404_macos_universal_qualify.py` checks a **signed `.app` copy** against an independently authenticated qualification record outside the app. It is intentionally not a signer, builder, downloader, installer, or acceptance UI driver. It reads the 64-KiB bounded XML Info.plist, checks the exact independently qualified final Host and Bun SHA-256, compares the sealed Bun digest, verifies exactly two Host/Bun architectures via Apple's `lipo`, and runs native `codesign` verification. It also pins the outer bundle identifier and Bun's TeamIdentifier, signing identifier, CodeDirectory flags and exact entitlements bytes. All executable parents must be real directories; symlinks and missing executable bits fail.

The external JSON record uses closed schema 2. The record fields are `schema`, `sourceCommit` (40 lowercase hex), `target` (`macos-universal`), `bundleIdentifier`, `hostInputSha256`, `hostFinalSha256`, `bunSha256`, `bunSignatureIdentifier`, `bunTeamIdentifier`, `bunSigningFlags`, `bunEntitlementsSha256`. The independent owner attests the input Host and separately measured final signed Host in the same authenticated record, bound to the exact source/build and bundle identifier. The input digest is historical provenance, not an installed-file measurement. The final Host bytes must match `hostFinalSha256` exactly; the qualified Bun input remains byte-identical. No schema 1 fallback. An app-generated receipt is not sufficient. The producer/reviewer must separately authenticate the input record **and its independent SHA-256 pin** before passing it to the preflight. A pin computed from PR-controlled input is not an authentication. This tool cannot authenticate the provenance of the pin or prove publisher trust.

Native leg command, with all values supplied by the trusted producer/runner (not by user-supplied PR metadata):

```bash
python3 -B tools/gh1404_macos_universal_qualify.py \
  --runner-label "$QUALIFIED_RUNNER_LABEL" \
  --app "$INSTALLED_SIGNED_APP" \
  --qualification "$QUALIFIED_INPUT_RECORD" \
  --qualification-sha256 "$TRUSTED_QUALIFICATION_SHA256" \
  --source-commit "$EXACT_CHECKOUT_COMMIT" \
  --output "$RUNNER_TEMP/gh1404-macos-native-preflight.json"
```

The command fails closed if GitHub's runner environment, physical CPU architecture, independently pinned record, signed bytes, architectures, or pinned signing fields disagree. The external record and digest must come from separately authenticated owner custody. On success, the new exclusive JSON receipt states `signed-bundle-preflight-only`, `installedLaunch: UNVERIFIED`, and `nativeEditorWorkflows: UNVERIFIED`.

Before a qualified universal `.app` exists, use only the preparatory test command:

```bash
python3 -B tools/test_gh1404_macos_universal_qualify.py -v
```

These tests cover four runner cells; wrong architecture/OS/Rosetta; missing/unknown/duplicate record fields and external digest; source commit mismatch; wrong target or sealed Bun pin; distinct input/final Host pins and modified Host/Bun; missing/invalid XML; symlink/non-executable inputs; failed code signing; incorrect signer identity/flags/entitlements; and signing-time substitution. Subprocess fakes test wrapper failure handling; **real OS signing and hosted execution require separate native receipts**.

## Signed Bun identity prerequisite — observed local diagnostic

On the Mac mini at this checkpoint, `/Users/centillionaire/.bun/bin/bun` (Bun 1.4.4) passed `/usr/bin/codesign --verify --strict --verbose=2`. `codesign --display --verbose=4` returned `Identifier=bun`, `TeamIdentifier=7FRXF46ZSN`, and `CodeDirectory flags=0x10000(runtime)`. Entitlements-output SHA-256 was `9518845bdc234b3cd8eed21fb7ce61a507c2a7fbd2ff6e0526a78dcf4611e7f9`. The binary SHA-256 was `0a2237cdcfa0d16ef501d9acca9f0026309ef348bea7393a64e8d1e3609565cc`, and `lipo -archs` yielded **arm64 only**.

These are independent read-only observations of one local Bun input, not a qualified Universal artifact. There is no matching signed Intel+Universal Bun identity receipt in this lane. Signed Intel source and final Universal input must be independently qualified. Combining signed thin slices with `lipo` does not itself prove the **resulting** Universal signature, entitlements, TeamIdentifier or byte identity. The final nested Bun must pass both exact SHA and signature checks; Pack must preserve the qualified input unchanged. Do not re-sign a vendor runtime as an unreviewed fallback.

## Protected workflow proposal for the CI owner — do not apply here

Proposed exact files:

- `.github/workflows/ci.yml`: add `macos-universal-native` job with four native runner matrix legs above, conditional on a fail-closed app/pack/source-related router output. Keep all existing required lanes and gitleaks unchanged. Use pinned checkout and no persisted credentials. Download a uniquely bound signed app archive built from independently qualified Host/Bun; compare archive/output hashes and preserve executable modes before running this preflight. Follow with the real no-flag installed Host, Guardian/Bun lifetime probes and GH-1404 AC-12 editor workflows. No success from the preflight alone.
- `tools/ci_changes.sh`, `tools/ci-inputs.json` and `tools/ci_changes_test.sh`: add the new output/classification and negative cases only through the existing router owner. Unknown inputs must select all possibly affected native jobs. A skipped selected job is a hard failure.
- `.github/workflows/ci.yml` `required.needs`/environment plus `tools/ci_required.sh` and its regression tests: route and require the new aggregate matrix result. Verify selected, missing, failed and cancelled states under the existing checker. A native leg cannot report green based solely on a build checksum or mocked result.

The CI owner must first decide artifact custody, signed-input acquisition, OS availability and how exact PR HEAD bytes are selected. Check the actual account's runner access with a real hosted dispatch; an upstream label is not proof this repository has run on it. The first production acceptance requires a signed installed bundle on native Intel and arm64, matching Host/Bun target slices, Security.framework code identity, one-use KGR1 and canonical transport authentication, real window/editor Save–Cancel–Discard/reopen/export, per-role death cleanup, and relaunch. Do not claim a qualified native workflow until these checks run and report on the exact source/installer bytes.

## First observable next checks

The script/test branch is ready for independent CI-owner review. Actual native execution and exact signed-input qualification are still missing. The first reviewer should run the local Python cases, inspect the security diff, verify a real signed universal Bun + Host qualification record and authenticated record SHA-256, then design the four-job release candidate without changing current required gates. Only after the CI owner approves the exact workflow/router/required diff should a separate protected-CI PR be prepared. No push or merge is authorized from this lane.
