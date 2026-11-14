//! Shared test-only immutable manifest fixture; no Runtime/native authority.
#![allow(clippy::expect_used)]

use super::{CompatSession, RendererBridgeOutcome};

pub(super) fn availability_session() -> (
    CompatSession,
    std::sync::mpsc::Receiver<RendererBridgeOutcome>,
) {
    use sha2::{Digest as _, Sha256};
    let temp = tempfile::tempdir().expect("availability manifest root");
    let path = temp.path().join("keld.permissions.jsonc");
    let text = r#"{"windows":{"editor":{"channels":["el:rendererReq"]}}}"#;
    std::fs::write(&path, text).expect("write actual manifest");
    let verified = super::super::load_verified_manifest(
        std::fs::File::open(&path).expect("open actual manifest"),
        path,
        Sha256::digest(text.as_bytes()).into(),
    )
    .expect("verify same immutable manifest");
    let (outcomes, received) = std::sync::mpsc::channel();
    (CompatSession::new(verified, outcomes), received)
}
