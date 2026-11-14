//! Read-only retained-source input to the existing Windows role preparer.
use super::RoleConfig;
use std::sync::Arc;

/// Recheck the exact source already selected by the Host before each generation.
/// Runtime retains the same source owner; it does not select files, interpret
/// graph policy, create a second restart gate, or treat this as readiness.
pub trait RoleSourceRecheck: std::fmt::Debug + Send + Sync {
    /// Verify the retained input before Job/listener/child preparation.
    ///
    /// # Errors
    /// Any refusal ends the existing attempt without provisioning a role.
    fn recheck(&self) -> std::io::Result<()>;
}

impl RoleConfig {
    /// Retain the Host's exact source check in this existing role configuration.
    /// The sole preparer invokes it on every attempt before acquiring launch
    /// inputs or provisioning a Job/listener/child. It neither grants a role
    /// nor changes the existing supervisor's restart/shutdown decision.
    #[must_use]
    pub fn source_recheck(mut self, source: Arc<dyn RoleSourceRecheck>) -> Self {
        self.source_recheck = Some(source);
        self
    }
}

#[cfg(test)]
mod tests;
