# GH1404 original draw.io source-only review proposal

STATUS: UNAPPROVED, INCOMPLETE, NOT INSTALLABLE. DO NOT BUILD, MERGE, OR RELEASE.

This bounded source producer transforms pristine jgraph/drawio-desktop commit 2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e using ONLY the exact separately published #1506 original argv parser adapter at 1ff2c9f68f22e4cf0c8ef9ec7dde046f63cf69fa. The dependent webapp gitlink remains jgraph/drawio commit 24b76c2cbd55d88e354042e8d329a2e4708972bc. No historical installed V9 bytes are sourced or copied.

Six core files are compared against immutable original SHA-256 and Git blob hashes: src/main/electron.js, args.js, electron-preload.js, package.json, package-lock.json and LICENSE. The generated actual modified electron.js only changes the argument parsing seam through #1506. It does not rewrite real process.argv, Bun.main, import.meta.main, original dirty-document dialogs or file-grant handlers. Native correctness has not been tested.

Run on an already verified immutable pristine original source root with a distinct NEW scratch output path:

GH1404_PINNED_PRISTINE_DESKTOP=/approved/pristine GH1404_PINNED_1506_ADAPTER=/approved/pr1506/tools/gh1404-original-argv/patch-original-main.mjs GH1404_WORKER17_SOURCE_OUTPUT=/own/scratch/new-output node --test tools/gh1404-original-source-w17/source-review.test.mjs

Generated files are deliberately NOT a runnable application: package and lock are pristine with UPSTREAM_NOT_MIGRATED suffixes and cannot be installed as migrated dependencies; upstream GPL license is a source reference, not a distribution approval; permissions are a text-only missing-input record. An exact modified main SHA is recorded for owner review, but it is not a qualified source selection. The full renderer, providers and native runtime remain absent.

Owner decisions required: (1) verified GPL LICENSE; (2) approved Legacy permissions/CSP; (3-4) migrated solver-produced lock and manifest; (5) accepted migrated original main patch; (6-8) actual API/electron/KIPC archive and unpacked-tree identities; (9-10) signed graph and current Host/Bun pairing review; (11) GPLv3 corresponding-source distribution review; (12) complete original source+patch review; (13) packed provider receipts; (14) current reviewed CLI source head; (15) full isolated desktop + pinned webapp source graph; (16) compiled CLI executable SHA bound to head; (17) actual approved preload leaf. The canonical list is https://github.com/gyldlab/keld/issues/1404#issuecomment-6818891971 . All 17 current-final approvals remain outstanding.

GPL-3.0-only original Desktop copyright and LICENSE text must be preserved. Before distribution, original app and license owners must decide notice-of-modification dates, corresponding-source of the modified covered software and build/install scripts, delivery method under GPLv3 Sections 1/5/6, KELD/provider notices, and separate renderer/icon/stencil/third-party/trademark obligations. This producer conveys no object code or distribution authorization.

Review requested: GH-499/GH-589 original owner @0monish selects exact final changes, including accept/reject #1506; independent keld-maintainers/Neelkanth238 reviews source/security/Guard; SDK/API/electron/KIPC owners qualify actual archives and lock; CLI/Host/Bun owners qualify exact current graph; release counsel reviews GPL. Native owner must separately test new signed macOS original Open/Edit/Save/Cancel/Discard/Reopen/Quit, one real diagram and zero incidental grants, all eight valid signed tamper/removal refusals, second app, and full final-head gates. A guarded source build requires >=8 GiB free and an exclusive Mac lease.
