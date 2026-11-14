//! The existing pre-loop attempt seal also owns its actual native builder.
//! It never copies a candidate into another builder or resets an attempted seal.
use super::super::ICoreWebView2Environment;
use super::super::view_build::NativeBuild;
use crate::WvError;

#[derive(Default)]
pub(in crate::webview2) struct NativeViewBuild {
    attempted: bool,
    builder: Option<NativeBuild>,
}

impl NativeViewBuild {
    pub(in crate::webview2) fn not_attempted() -> Self {
        Self::default()
    }

    pub(in crate::webview2) fn was_attempted(&self) -> bool {
        self.attempted
    }

    pub(in crate::webview2) fn mark_attempted(&mut self) {
        self.attempted = true;
    }

    pub(in crate::webview2) fn builder(
        &mut self,
        environment: &ICoreWebView2Environment,
        next_id: u32,
        saved_permissions_reconciled: bool,
    ) -> &mut NativeBuild {
        self.builder.get_or_insert_with(|| {
            NativeBuild::new(environment.clone(), next_id, saved_permissions_reconciled)
        })
    }

    /// Called only while transferring the real prepared engine into its one
    /// `AppRunCustody`. No native/fallible work may intervene after this take and
    /// `ManagedWindows` installation. A retained candidate refuses the transfer.
    pub(in crate::webview2) fn take_ready_builder(
        &mut self,
        environment: &ICoreWebView2Environment,
        next_id: u32,
        saved_permissions_reconciled: bool,
    ) -> Result<NativeBuild, WvError> {
        if !self
            .builder(environment, next_id, saved_permissions_reconciled)
            .candidate_is_empty()
        {
            return Err(super::super::profile_failure_with(
                crate::profile::ProfileErrorKind::LifecycleUnproven,
                "application loop setup cannot move an unresolved construction candidate",
            ));
        }
        self.builder.take().ok_or_else(|| {
            super::super::profile_failure_with(
                crate::profile::ProfileErrorKind::LifecycleUnproven,
                "the checked original native builder disappeared before transfer",
            )
        })
    }
}
