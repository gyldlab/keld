//! Read-only diagnostics and the unchanged shared real-pointer program run in one process.
use crate::renderer_bridge::CLICK_SCRIPT;

pub(super) fn composed_click_script() -> String {
    format!(
        "{}\n{CLICK_SCRIPT}",
        include_str!("../../../fixtures/public_fs_preclick.swift")
    )
}
