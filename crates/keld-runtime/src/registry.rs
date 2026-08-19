//! Host-owned lifecycle registry for one primary role and one app-bound role.
//!
//! KEL-75 T2: each slot is an independent [`RoleSupervisor`]. A primary crash
//! or restart MUST NOT revoke, stop, or re-provision the app-bound role, and
//! the reverse. Window-bound roles, virtual ports, Electron facades, sandbox
//! admission and bulk mapping are later slices.

use crate::RuntimeError;

pub use crate::unix_role::{
    RoleConfig, RoleEvent, RoleGeneration, RoleOwner, RoleRevocationCause, RoleSupervisor,
};

/// Host-owned pair of independently supervised Unix Bun roles.
#[derive(Debug)]
pub struct RoleRegistry {
    primary: RoleSupervisor,
    app_bound: RoleSupervisor,
}

impl RoleRegistry {
    /// Starts one `primary` coordinator and one `app-bound` coordinator.
    ///
    /// Each coordinator reuses KEL-70 supervision and the T1b Unix bootstrap
    /// generation lease. The registry does not couple their restart loops.
    ///
    /// If the app-bound role fails to start after the primary is live, the
    /// primary is shut down before this function returns.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::Lifecycle`] when the configs are not exactly
    /// one primary plus one app-bound owner, or when either role cannot be
    /// provisioned or spawned.
    pub fn start(primary: RoleConfig, app_bound: RoleConfig) -> Result<Self, RuntimeError> {
        if primary.owner() != RoleOwner::Primary {
            return Err(owner_error(
                "first registry slot must be a primary lifecycle owner",
            ));
        }
        if app_bound.owner() != RoleOwner::AppBound {
            return Err(owner_error(
                "second registry slot must be an app-bound lifecycle owner",
            ));
        }
        let primary = RoleSupervisor::start(primary)?;
        match RoleSupervisor::start(app_bound) {
            Ok(app_bound) => Ok(Self { primary, app_bound }),
            Err(error) => {
                primary.shutdown();
                let _ = primary.wait_for_outcome();
                Err(error)
            }
        }
    }

    /// Independently supervised primary role.
    #[must_use]
    pub fn primary(&self) -> &RoleSupervisor {
        &self.primary
    }

    /// Independently supervised app-bound role.
    #[must_use]
    pub fn app_bound(&self) -> &RoleSupervisor {
        &self.app_bound
    }

    /// Stops both roles. Application-session stop, not window close (T4).
    pub fn shutdown(&self) {
        self.primary.shutdown();
        self.app_bound.shutdown();
    }
}

fn owner_error(message: &'static str) -> RuntimeError {
    RuntimeError::Lifecycle {
        phase: "role registry owner",
        source: std::io::Error::other(message),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{RoleConfig, RoleEvent, RoleOwner, RoleRegistry, RoleRevocationCause};
    use crate::unix_role::fixture::{FamilyFixture, assert_ready_line, connect_with_foreign_token};
    use crate::{RestartPolicy, RuntimeError, SupervisorOutcome};

    #[test]
    fn registry_rejects_mismatched_lifecycle_owners() {
        let err = RoleRegistry::start(RoleConfig::app_bound("bun"), RoleConfig::primary("bun"))
            .expect_err("swapped owners must fail closed");
        let msg = err.to_string();
        assert!(msg.contains("KELD-RUNTIME-003"), "{msg}");
        assert!(msg.contains("role registry owner"), "{msg}");
        assert!(matches!(
            err,
            RuntimeError::Lifecycle {
                phase: "role registry owner",
                ..
            }
        ));
    }

    #[test]
    fn registry_rejects_second_primary_in_app_bound_slot() {
        let err = RoleRegistry::start(RoleConfig::primary("bun"), RoleConfig::primary("bun"))
            .expect_err("two primaries must fail closed");
        assert!(err.to_string().contains("KELD-RUNTIME-003"));
    }

    #[test]
    fn role_config_constructors_set_distinct_owners() {
        assert_eq!(RoleConfig::new("bun").owner(), RoleOwner::Primary);
        assert_eq!(RoleConfig::primary("bun").owner(), RoleOwner::Primary);
        assert_eq!(RoleConfig::app_bound("bun").owner(), RoleOwner::AppBound);
    }

    #[test]
    fn real_bun_app_bound_isolates_crash_restart_and_rejects_foreign_token() {
        let fixture = FamilyFixture::new();
        let policy = RestartPolicy {
            max_crashes: 3,
            window_secs: 30,
        };
        let (primary_probe_tx, primary_probe_rx) = mpsc::channel();
        let (app_probe_tx, app_probe_rx) = mpsc::channel();
        let registry = RoleRegistry::start(
            bun_role(
                RoleConfig::primary("bun"),
                &fixture,
                fixture.primary_control_path(),
                policy,
            )
            .with_probe(primary_probe_tx),
            bun_role(
                RoleConfig::app_bound("bun"),
                &fixture,
                fixture.app_bound_control_path(),
                policy,
            )
            .with_probe(app_probe_tx),
        )
        .expect("primary and app-bound roles must spawn under Bun");

        let primary_g1 = recv_probe(&primary_probe_rx, "primary g1");
        let app_g1 = recv_probe(&app_probe_rx, "app-bound g1");
        assert_ne!(primary_g1.app_link, app_g1.app_link);

        let mut primary_control =
            bind_attempt(registry.primary(), &primary_g1, 1, fixture.accept_primary());
        let mut app_control =
            bind_attempt(registry.app_bound(), &app_g1, 1, fixture.accept_app_bound());

        app_control.write_line("CRASH");
        expect_child_exit_revoke(registry.app_bound(), &app_g1, 1, "app-bound g1 Revoked");
        let app_g2 = recv_probe(&app_probe_rx, "app-bound g2");
        assert_ne!(app_g1.generation, app_g2.generation);
        assert_ne!(app_g1.app_link, app_g2.app_link);
        assert_no_revoked(
            registry.primary(),
            "primary stays bound across app-bound restart",
        );
        expect_provisioned_spawned(registry.app_bound(), &app_g2, 2);

        let mut app_control = fixture.accept_app_bound();
        assert_ready_line(&mut app_control, &app_g2.app_link);
        expect_foreign_reject(&app_g1.app_link, &app_g2, registry.app_bound(), "stale g1");
        expect_foreign_reject(
            &primary_g1.app_link,
            &app_g2,
            registry.app_bound(),
            "primary token on app-bound",
        );
        assert_no_revoked(
            registry.primary(),
            "cross-principal reject must not revoke primary",
        );
        complete_bind(registry.app_bound(), &app_g2, &mut app_control);

        primary_control.write_line("CRASH");
        expect_child_exit_revoke(registry.primary(), &primary_g1, 1, "primary g1 Revoked");
        let primary_g2 = recv_probe(&primary_probe_rx, "primary g2");
        expect_provisioned_spawned(registry.primary(), &primary_g2, 2);
        assert_no_revoked(
            registry.app_bound(),
            "app-bound stays live across primary restart",
        );
        let primary_g2_control =
            bind_generation(registry.primary(), &primary_g2, fixture.accept_primary());

        registry.shutdown();
        expect_shutdown_revoke(registry.primary(), &primary_g2, 2, "primary shutdown");
        expect_shutdown_revoke(registry.app_bound(), &app_g2, 2, "app-bound shutdown");
        assert_stopped(registry.primary(), "primary");
        assert_stopped(registry.app_bound(), "app-bound");
        drop(primary_g2_control);
        drop(app_control);
    }

    fn bun_role(
        config: RoleConfig,
        fixture: &FamilyFixture,
        control: &std::path::Path,
        policy: RestartPolicy,
    ) -> RoleConfig {
        config
            .arg(FamilyFixture::script_path())
            .arg(control)
            .current_dir(fixture.dir())
            .restart_policy(policy)
            .admission_timeout(Duration::from_secs(5))
    }

    fn recv_probe(
        rx: &mpsc::Receiver<crate::unix_role::ProvisionedProbe>,
        label: &str,
    ) -> crate::unix_role::ProvisionedProbe {
        rx.recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| panic!("{label} app link"))
    }

    fn bind_attempt(
        supervisor: &super::RoleSupervisor,
        probe: &crate::unix_role::ProvisionedProbe,
        attempt: u32,
        control: crate::unix_role::fixture::ControlPeer,
    ) -> crate::unix_role::fixture::ControlPeer {
        expect_provisioned_spawned(supervisor, probe, attempt);
        bind_generation(supervisor, probe, control)
    }

    fn expect_provisioned_spawned(
        supervisor: &super::RoleSupervisor,
        probe: &crate::unix_role::ProvisionedProbe,
        attempt: u32,
    ) {
        assert_next(
            supervisor,
            |event| matches!(event, RoleEvent::Provisioned { generation, attempt: a } if *generation == probe.generation && *a == attempt),
            "Provisioned",
        );
        assert_next(
            supervisor,
            |event| matches!(event, RoleEvent::Spawned { generation, attempt: a, .. } if *generation == probe.generation && *a == attempt),
            "Spawned",
        );
    }

    fn bind_generation(
        supervisor: &super::RoleSupervisor,
        probe: &crate::unix_role::ProvisionedProbe,
        mut control: crate::unix_role::fixture::ControlPeer,
    ) -> crate::unix_role::fixture::ControlPeer {
        assert_ready_line(&mut control, &probe.app_link);
        complete_bind(supervisor, probe, &mut control);
        control
    }

    fn complete_bind(
        supervisor: &super::RoleSupervisor,
        probe: &crate::unix_role::ProvisionedProbe,
        control: &mut crate::unix_role::fixture::ControlPeer,
    ) {
        control.write_line("BIND");
        assert_next(
            supervisor,
            |event| matches!(event, RoleEvent::LinkBound { generation, .. } if *generation == probe.generation),
            "LinkBound",
        );
        assert_eq!(control.read_line(), "BOUND");
    }

    fn expect_foreign_reject(
        token_link: &str,
        endpoint: &crate::unix_role::ProvisionedProbe,
        supervisor: &super::RoleSupervisor,
        label: &str,
    ) {
        connect_with_foreign_token(token_link, &endpoint.app_link);
        assert_next(
            supervisor,
            |event| {
                matches!(
                    event,
                    RoleEvent::BootstrapRejected {
                        generation,
                        attempt: 2,
                        code: "KELD-IPC-007"
                    } if *generation == endpoint.generation
                )
            },
            label,
        );
    }

    fn expect_child_exit_revoke(
        supervisor: &super::RoleSupervisor,
        probe: &crate::unix_role::ProvisionedProbe,
        attempt: u32,
        label: &str,
    ) {
        assert_next(
            supervisor,
            |event| {
                matches!(
                    event,
                    RoleEvent::Revoked {
                        generation,
                        attempt: a,
                        cause: RoleRevocationCause::ChildExited
                    } if *generation == probe.generation && *a == attempt
                )
            },
            label,
        );
    }

    fn expect_shutdown_revoke(
        supervisor: &super::RoleSupervisor,
        probe: &crate::unix_role::ProvisionedProbe,
        attempt: u32,
        label: &str,
    ) {
        assert_next(
            supervisor,
            |event| {
                matches!(
                    event,
                    RoleEvent::Revoked {
                        generation,
                        attempt: a,
                        cause: RoleRevocationCause::Shutdown
                    } if *generation == probe.generation && *a == attempt
                )
            },
            label,
        );
    }

    fn assert_stopped(supervisor: &super::RoleSupervisor, label: &str) {
        match supervisor.wait_for_outcome() {
            SupervisorOutcome::Stopped => {}
            other => panic!("{label} shutdown should stop cleanly, got {other:?}"),
        }
    }

    fn assert_next(
        supervisor: &super::RoleSupervisor,
        predicate: impl Fn(&RoleEvent) -> bool,
        label: &str,
    ) -> RoleEvent {
        let event = supervisor
            .recv_event(Duration::from_secs(2))
            .unwrap_or_else(|| panic!("missing event: {label}"));
        if predicate(&event) {
            event
        } else {
            panic!("expected next event {label}, got {event:?}");
        }
    }

    fn assert_no_revoked(supervisor: &super::RoleSupervisor, label: &str) {
        match supervisor.try_recv_event() {
            Some(RoleEvent::Revoked { .. }) => panic!("{label}: unexpected Revoked"),
            Some(other) => panic!("{label}: unexpected queued event {other:?}"),
            None => {}
        }
    }
}
