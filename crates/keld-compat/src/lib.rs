//! keld-compat — compatibility evidence and Electron lifecycle oracle.
//!
//! The TypeScript facade lives in `packages/@keld/electron`; Electron names stay out of
//! `keld-core` and `keld-ipc`. Architecture assigns future host-side semantics such as
//! custom `protocol` schemes, session policy, and `webContents` routing to this crate.
//! Normative target spec: `docs/architecture/04-electron-compat.md` §3.
//!
//! Generic compatibility evidence (KEL-74) lives in [`evidence`]: a versioned
//! record + committed-denominator scorer. That module is not an Electron shim
//! and does not encode VS Code or package names. Repository maturity and evidence live
//! in `docs/engineering/product-status.tsv`.

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
