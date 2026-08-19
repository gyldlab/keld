//! keld-compat — host-side Electron emulation.
//!
//! Layer 1–2 (KEL-72): the TypeScript shim lives in `packages/@keld/electron`
//! and maps `app.whenReady` / `app.quit` / `window-all-closed` onto the
//! generic host-lifecycle kipc channel (`keld_ipc::LIFECYCLE_CHANNEL`,
//! served by `keld_core::LifecycleSession`). Electron names stay out of
//! `keld-core` / `keld-ipc`.
//!
//! This crate still owns the semantics JS cannot fake later: custom
//! `protocol` schemes, `session` cookie/proxy subsets, `webContents`
//! routing identity, window parenting/modals, `nativeImage` codecs.
//! Normative spec: `docs/architecture/04-electron-compat.md` §3.
//!
//! Generic compatibility evidence (KEL-74) lives in [`evidence`]: a versioned
//! record + committed-denominator scorer. That module is not an Electron shim
//! and does not encode VS Code or package names.

pub mod evidence;

#[cfg(test)]
mod scoreboard_seal {
    use super::evidence::{CivilDate, parse_denominator, parse_evidence, score};

    const AS_OF: CivilDate = CivilDate {
        year: 2026,
        month: 8,
        day: 19,
    };

    /// This module is outside `evidence`. A `Scoreboard { complete: true, … }`
    /// literal would not compile here (private fields). The public surface is
    /// [`score`](crate::evidence::score) plus accessors.
    #[test]
    fn product_uncommitted_complete_is_false_via_public_accessors() {
        let evidence = br#"{
  "schema": "keld.compat.evidence/v1",
  "artifact": {
    "sha256": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "platform": "macos",
    "arch": "aarch64"
  },
  "revisions": { "keld": "67f39cdc898254f1e0c9cd50800f242ae7a4c493", "bun": "1.3.14", "engine": "wkwebview" },
  "authority_profile": "strict_bun",
  "operation": {
    "id": "hello.window.open",
    "kind": "primary_workflow",
    "oracle": { "id": "hello-window-visible", "revision": "kel26-macos" }
  },
  "result": "pass",
  "evidence_uri": "https://github.com/gyldlab/keld/commit/67f39cdc898254f1e0c9cd50800f242ae7a4c493"
}"#;
        let denom = br#"{
  "schema": "keld.compat.denominator/v1",
  "panel": "product",
  "corpus_id": "toy-uncommitted",
  "corpus_sha256": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "kind": "primary_workflow",
  "cells": [{ "operation_id": "hello.window.open", "oracle_id": "hello-window-visible" }]
}"#;
        let record = parse_evidence(evidence).expect("evidence");
        let denominator = parse_denominator(denom).expect("denom");
        let board = score(&denominator, &[record], AS_OF).expect("score");
        assert!(!board.complete());
        assert_eq!(board.unweighted_percent(), None);
        assert!(!board.claim().contains("100%"));
    }
}

/// Compat tiers, mirrored on the public scoreboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Lifecycle, windows, IPC, dialogs, menus, tray, clipboard, notifications.
    One,
    /// Shortcuts, power, safeStorage, session/protocol subsets, updater bridge.
    Two,
    /// `<webview>` mapping, capture, net module.
    Three,
}
