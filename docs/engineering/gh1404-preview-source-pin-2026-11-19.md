# GH-1404 macOS developer-preview source pin — 2026-11-19

This record pins the native macOS arm64 source input for an ad-hoc developer-preview app/DMG. It is not a packaged artifact receipt, Gatekeeper/publisher proof or installed GUI acceptance. No product or trust-boundary change is made by this report.

## Exact GitHub source chain

| Scope | PR | Commit SHA | Git tree SHA |
| --- | --- | --- | --- |
| Host, original graph and security integration | [#1507](https://github.com/gyldlab/keld/pull/1507) | c4493456429950d38da26e5306bccffcc1f572b0 | 7b2c0b2ba7e6686b7d7b66041da153a200e84281 |
| Sealed original Bun main, installed bootstrap, preload | [#1510](https://github.com/gyldlab/keld/pull/1510) | beff4c720efeb8a6e9aed06a932de2cf088b1eef | 09febb8cb910a40c64a1ee2a80b259de5d5d8ab0 |
| Native Quit retention and JS will-quit veto | [#1576](https://github.com/gyldlab/keld/pull/1576) | **b9eb5dba583d5075fdb5e43aa1e4ef255be9ac86** | **26264aa87fd31de5816cbca0241dbb15289d5cd7** |

On 19 November 2026, live GitHub PR head SHAs and origin branch SHAs agreed with the table. The direct ancestry checks all returned exit 0: c4493456 is an ancestor of beff4c72, beff4c72 is an ancestor of b9eb5dba, and c4493456 is an ancestor of b9eb5dba. The then-current origin/main at 16a9545ccc417b72b2f0508520c360f9fd5e2ced is also an ancestor of b9eb5dba. The task-owned clean worktree resolved HEAD b9eb5dba and tree 26264aa8, with no unmerged index entries.

Relative to #1510, #1576 contains exactly two commits: 9d0d4666fb89c2cda3436c80cfc67d18dc10a0d7 for native Quit retention, and b9eb5dba583d5075fdb5e43aa1e4ef255be9ac86 for JS will-quit veto/retry. Their changes are confined to crates/keld-wv/AGENTS.md, the macOS termination module and its parent, the electron native-quit fixture, app.ts, and two electron Quit test files. The packaging implementation is inherited; there is no unresolved source cherry-pick or merge needed to construct a preview from this pin.

The packager must check out commit **b9eb5dba583d5075fdb5e43aa1e4ef255be9ac86**, with tree **26264aa87fd31de5816cbca0241dbb15289d5cd7**. A later task-report commit or moving branch name must not replace this source selection.

For a new task-owned checkout, use these commands with an unused, privately owned destination:

    git fetch origin
    git worktree add --detach /path/to/task-owned-source b9eb5dba583d5075fdb5e43aa1e4ef255be9ac86
    git -C /path/to/task-owned-source rev-parse HEAD
    git -C /path/to/task-owned-source rev-parse HEAD^{tree}

The printed SHAs must match the #1576 row.

## Observed source validation

- git diff --check from 16a9545c to b9eb5dba: exit 0.
- cargo fmt --all --check on macOS arm64: exit 0.
- Bun 1.4.4, after frozen dependency installs in packages/@keld/kipc, packages/@keld/api, and packages/@keld/electron: focused Bun tests of native-quit-requested.test.ts and original-quit-handoff.component.test.ts had **60 pass, 0 fail, 189 assertions**, exit 0. An earlier attempt without the local linked packages failed to resolve @keld/kipc; this setup failure cleared after the frozen installs.
- GitHub [#1576 source checks](https://github.com/gyldlab/keld/pull/1576/checks) at b9eb5dba: CI required, macOS Rust Clippy/workspace tests, Bun tests, rustfmt, CodeQL and relevant routed jobs passed (Actions run 44486548532). These are source verification, not installed-app or artifact proof.
- Separate CI provenance and KELD VeriSentry publisher checks failed. The actual provenance log on 19 November says it accepts only gyldlab/keld main as the base, while draft #1576 deliberately targets the #1510 head. The publisher then refused stale/changed PR base state. This is an existing protected merge-admission restriction for the stacked PR layout. Preserve the security gate and reconcile the integration/merge sequence rather than weakening the policy.

## External input and final artifact handoff

1. **Original draw.io app:** qualification must use the actual patched Desktop 31.7.0 main/provider/graph, renderer, resources, license materials and independent source hashes. The historical [gyldlab/keld-patched-apps snapshot at 0c2541f](https://github.com/gyldlab/keld-patched-apps/tree/0c2541f445668cb3af1cdf58f2839282654b8582/apps/drawio/snapshots/2026-11-03) is a developer snapshot, not a final app tree. The historical ZIP SHA-256 is a420b00b2450b89ef31bdff6e1ef40f93e4f6c817110fc40e09a4f39540057c7. The framework repository does not contain the full original application resource tree.
2. **Host/Bun:** independently build and qualify the shipping Host binary from the above exact source. Record its digest, executable mode, architecture, version and signing identity. Independently verify selected Bun 1.4.4 input digest, architecture, existing nested signature and version. Do not substitute a historical Host hash, historical V9 DMG, or merely the executable on a PATH.
3. **Closed project configuration:** supply exact keld.config.ts, keld.permissions.jsonc and keld.build.ts, plus graph, bootstrap, entry/renderer and license resources from the qualified application source. Use target macos-arm64, local ad-hoc signing and a new task-owned output directory. The application and source owners provide independent qualification facts.
4. **Packaging and native witnesses:** run the real keld build --target macos-arm64, preserving the verified Bun nested signature and signing the containing app ad hoc. Verify code signatures, digests, permission/mode checks, content inventory, DMG integrity, and an external receipt binding all input and output hashes. The separate GUI owner then runs the installed original draw.io Launch/Open/Edit/Save/Cancel/Discard/Quit flow with file readback and process cleanup, and records the exact artifact it tested.

The governing specification is in docs/specs/gh1404-macos-local-app-packaging/{design,implementation,acceptance}.md. This is an authorized developer demonstration with local ad-hoc bundle consistency. Apple Developer ID, notarization, App Store/publisher trust and Gatekeeper acceptance are outside this source pin and remain unproved.

The next acceptance checkpoint is a manifest/receipt joining **b9eb5dba and tree 26264aa8** to independently qualified application, Host and Bun inputs and to the signed artifact/DMG hashes. The artifact and native owners make their own acceptance reports. This document introduces no product-source changes and makes no installed-app acceptance claim.
