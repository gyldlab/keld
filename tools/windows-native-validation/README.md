# Reviewed Windows Guard source preview

This transport branch carries reviewed source deltas and a mirror-only preparation driver. It is not a merge-ready change or proof of Windows build, installed-app, signing, containment or cleanup success. Native Rust formatting, compilation and tests remain pending.

Run `python preview_guard.py <absolute-original-worktree>` on the authorized Windows machine. The driver validates all recorded source hashes or required absence, refuses links/reparse points and mismatched contexts, and writes a fresh preview plus `native-guard.patch` under `D:/KeldBuild`. It never applies the patch to the checkout, executes a process, changes ACLs/trust, creates keys, installs or launches an app. Preserve the emitted manifest and inspect the exact patch before separately authorized adoption. Do not replace unknown native files from a reconstructed checkout.

The deployment authority table remains empty. Catalogs authenticate assertions only; prepared resources are not Strict/spawn permits. The source union includes same-current-Host ownership, immutable movable catalog custody, single-registration resource claims and opaque post-close grant receipts. Source-only review does not qualify unrun native controls.

`python -m unittest -v test_preview` runs the portable preview-driver controls. Pin mismatches fail closed; never change a pin merely to match arbitrary current data.
