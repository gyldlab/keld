//! Host-owned lifecycle registry for one primary role and one app-bound role.
//!
//! KEL-75 T2: each slot is an independent [`RoleSupervisor`]. A primary crash
//! or restart MUST NOT revoke, stop, or re-provision the app-bound role, and
//! the reverse. KEL-75 T3 adds host-owned bounded virtual ports between live
//! authenticated role generations.

use std::collections::VecDeque;

use crate::RuntimeError;

pub use crate::unix_role::{
    RoleConfig, RoleEvent, RoleGeneration, RoleOwner, RoleRevocationCause, RoleSupervisor,
};
use crate::virtual_port::VirtualPortRegistry;
pub use crate::virtual_port::{
    PortCapability, PortDisconnectReason, PortEnd, PortMessage, RolePrincipal, VirtualPortError,
    VirtualPortGeneration,
};

/// Host-owned pair of independently supervised Unix Bun roles.
#[derive(Debug)]
pub struct RoleRegistry {
    primary: RoleSupervisor,
    app_bound: RoleSupervisor,
    virtual_ports: VirtualPortRegistry,
    /// Lifecycle events buffered during [`Self::sync_role_events`].
    primary_buffered_events: VecDeque<RoleEvent>,
    app_bound_buffered_events: VecDeque<RoleEvent>,
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
            Ok(app_bound) => Ok(Self {
                primary,
                app_bound,
                virtual_ports: VirtualPortRegistry::new(),
                primary_buffered_events: VecDeque::new(),
                app_bound_buffered_events: VecDeque::new(),
            }),
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

    /// Mints a port pair between two authenticated role principals.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] when either generation is stale in the
    /// port registry.
    pub fn create_role_port_pair(
        &mut self,
        owner_a: RolePrincipal,
        owner_b: RolePrincipal,
    ) -> Result<(PortCapability, PortCapability), VirtualPortError> {
        self.sync_role_events();
        self.virtual_ports.create_pair(owner_a, owner_b)
    }

    /// Sends one bounded message through a live host-owned role port.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] when lifecycle revocation, ownership,
    /// closure, queue bounds, or message length reject the operation.
    pub fn send_role_port(
        &mut self,
        from: PortCapability,
        sender: RolePrincipal,
        payload: &[u8],
    ) -> Result<(), VirtualPortError> {
        self.with_synchronized_virtual_ports(|ports| ports.send(from, sender, payload))
    }

    /// Transfers one role-port end once to a live host-approved principal.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] when lifecycle revocation, ownership, or
    /// the one-shot transfer rules reject the operation.
    pub fn transfer_role_port(
        &mut self,
        capability: PortCapability,
        from: RolePrincipal,
        target: RolePrincipal,
    ) -> Result<PortCapability, VirtualPortError> {
        self.with_synchronized_virtual_ports(|ports| ports.transfer(capability, from, target))
    }

    /// Closes one live role-port end.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] when lifecycle revocation, ownership, or
    /// port state rejects the operation.
    pub fn close_role_port(
        &mut self,
        capability: PortCapability,
        owner: RolePrincipal,
    ) -> Result<(), VirtualPortError> {
        self.with_synchronized_virtual_ports(|ports| ports.close(capability, owner))
    }

    /// Receives the next queued role-port message for `owner`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] when lifecycle revocation, ownership, or
    /// port state rejects the operation.
    pub fn recv_role_port(
        &mut self,
        capability: PortCapability,
        owner: RolePrincipal,
    ) -> Result<Option<PortMessage>, VirtualPortError> {
        self.with_synchronized_virtual_ports(|ports| ports.recv(capability, owner))
    }

    /// Observes one pending disconnect for a live role-port end, if any.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] when lifecycle revocation, ownership, or
    /// port state rejects the operation.
    pub fn poll_role_port_disconnect(
        &mut self,
        capability: PortCapability,
        owner: RolePrincipal,
    ) -> Result<Option<PortDisconnectReason>, VirtualPortError> {
        self.with_synchronized_virtual_ports(|ports| ports.poll_disconnect(capability, owner))
    }

    /// Revokes virtual port routes for one role generation.
    pub fn revoke_role_ports(&mut self, principal: RolePrincipal) {
        self.sync_role_events();
        self.virtual_ports.revoke_generation(principal);
    }

    /// Stops both roles and revokes every live virtual port route.
    ///
    /// Application-session stop, not window close (T4).
    ///
    /// Shutdown requests are asynchronous. Observe the resulting lifecycle
    /// revocations through [`Self::try_recv_primary_event`] and
    /// [`Self::try_recv_app_bound_event`], which retain events while
    /// synchronizing virtual port state.
    pub fn shutdown(&mut self) {
        self.sync_role_events();
        for principal in self.virtual_ports.live_principals() {
            self.virtual_ports.revoke_generation(principal);
        }
        self.primary.shutdown();
        self.app_bound.shutdown();
        self.sync_role_events();
    }

    /// Records one authenticated live generation for virtual port admission.
    pub fn register_bound_principal(&mut self, principal: RolePrincipal) {
        self.virtual_ports.register_generation(principal);
    }

    /// Drains pending role lifecycle events into virtual port state.
    pub fn poll_role_events(&mut self) {
        self.sync_role_events();
    }

    /// Returns the next buffered or queued primary lifecycle event.
    ///
    /// Events observed during virtual port synchronization are retained here
    /// so callers can still observe the public lifecycle feed.
    #[must_use]
    pub fn try_recv_primary_event(&mut self) -> Option<RoleEvent> {
        self.sync_role_events();
        self.primary_buffered_events.pop_front()
    }

    /// Returns the next buffered or queued app-bound lifecycle event.
    #[must_use]
    pub fn try_recv_app_bound_event(&mut self) -> Option<RoleEvent> {
        self.sync_role_events();
        self.app_bound_buffered_events.pop_front()
    }

    fn sync_role_events(&mut self) {
        drain_supervisor_role_events(
            RoleOwner::Primary,
            &self.primary,
            &mut self.virtual_ports,
            &mut self.primary_buffered_events,
        );
        drain_supervisor_role_events(
            RoleOwner::AppBound,
            &self.app_bound,
            &mut self.virtual_ports,
            &mut self.app_bound_buffered_events,
        );
    }

    fn with_synchronized_virtual_ports<T>(
        &mut self,
        operation: impl FnOnce(&mut VirtualPortRegistry) -> Result<T, VirtualPortError>,
    ) -> Result<T, VirtualPortError> {
        self.sync_role_events();
        operation(&mut self.virtual_ports)
    }
}

fn drain_supervisor_role_events(
    owner: RoleOwner,
    supervisor: &RoleSupervisor,
    virtual_ports: &mut VirtualPortRegistry,
    buffered_events: &mut VecDeque<RoleEvent>,
) {
    while let Some(event) = supervisor.try_recv_event() {
        if let RoleEvent::Revoked { generation, .. } = &event {
            virtual_ports.revoke_generation(RolePrincipal::new(owner, *generation));
        }
        buffered_events.push_back(event);
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

    use super::{
        RoleConfig, RoleEvent, RoleGeneration, RoleOwner, RolePrincipal, RoleRegistry,
        RoleRevocationCause,
    };
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
        let mut registry = RoleRegistry::start(
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

        let mut primary_control = bind_attempt(
            &mut registry,
            RoleOwner::Primary,
            &primary_g1,
            1,
            fixture.accept_primary(),
        );
        let mut app_control = bind_attempt(
            &mut registry,
            RoleOwner::AppBound,
            &app_g1,
            1,
            fixture.accept_app_bound(),
        );

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
        complete_bind(
            &mut registry,
            RoleOwner::AppBound,
            &app_g2,
            &mut app_control,
        );

        primary_control.write_line("CRASH");
        expect_child_exit_revoke(registry.primary(), &primary_g1, 1, "primary g1 Revoked");
        let primary_g2 = recv_probe(&primary_probe_rx, "primary g2");
        expect_provisioned_spawned(registry.primary(), &primary_g2, 2);
        assert_no_revoked(
            registry.app_bound(),
            "app-bound stays live across primary restart",
        );
        let primary_g2_control = bind_generation(
            &mut registry,
            RoleOwner::Primary,
            &primary_g2,
            fixture.accept_primary(),
        );

        registry.shutdown();
        assert_stopped(registry.primary(), "primary");
        assert_stopped(registry.app_bound(), "app-bound");
        assert!(matches!(
            registry.try_recv_primary_event(),
            Some(RoleEvent::Revoked {
                generation,
                attempt: 2,
                cause: RoleRevocationCause::Shutdown,
            }) if generation == primary_g2.generation
        ));
        assert!(matches!(
            registry.try_recv_app_bound_event(),
            Some(RoleEvent::Revoked {
                generation,
                attempt: 2,
                cause: RoleRevocationCause::Shutdown,
            }) if generation == app_g2.generation
        ));
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

    fn supervisor_for(registry: &RoleRegistry, owner: RoleOwner) -> &super::RoleSupervisor {
        match owner {
            RoleOwner::Primary => registry.primary(),
            RoleOwner::AppBound => registry.app_bound(),
        }
    }

    fn bind_attempt(
        registry: &mut RoleRegistry,
        owner: RoleOwner,
        probe: &crate::unix_role::ProvisionedProbe,
        attempt: u32,
        control: crate::unix_role::fixture::ControlPeer,
    ) -> crate::unix_role::fixture::ControlPeer {
        let supervisor = supervisor_for(registry, owner);
        expect_provisioned_spawned(supervisor, probe, attempt);
        bind_generation(registry, owner, probe, control)
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
        registry: &mut RoleRegistry,
        owner: RoleOwner,
        probe: &crate::unix_role::ProvisionedProbe,
        mut control: crate::unix_role::fixture::ControlPeer,
    ) -> crate::unix_role::fixture::ControlPeer {
        assert_ready_line(&mut control, &probe.app_link);
        complete_bind(registry, owner, probe, &mut control);
        control
    }

    fn complete_bind(
        registry: &mut RoleRegistry,
        owner: RoleOwner,
        probe: &crate::unix_role::ProvisionedProbe,
        control: &mut crate::unix_role::fixture::ControlPeer,
    ) {
        let supervisor = supervisor_for(registry, owner);
        control.write_line("BIND");
        assert_next(
            supervisor,
            |event| matches!(event, RoleEvent::LinkBound { generation, .. } if *generation == probe.generation),
            "LinkBound",
        );
        registry.register_bound_principal(RolePrincipal::new(owner, probe.generation));
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

    #[test]
    fn registry_virtual_port_pair_routes_between_bound_roles() {
        let fixture = FamilyFixture::new();
        let policy = RestartPolicy {
            max_crashes: 3,
            window_secs: 30,
        };
        let (primary_probe_tx, primary_probe_rx) = mpsc::channel();
        let (app_probe_tx, app_probe_rx) = mpsc::channel();
        let mut registry = RoleRegistry::start(
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
        .expect("roles spawn");

        let primary_g1 = recv_probe(&primary_probe_rx, "primary g1");
        let app_g1 = recv_probe(&app_probe_rx, "app-bound g1");
        let primary_control = bind_attempt(
            &mut registry,
            RoleOwner::Primary,
            &primary_g1,
            1,
            fixture.accept_primary(),
        );
        let app_control = bind_attempt(
            &mut registry,
            RoleOwner::AppBound,
            &app_g1,
            1,
            fixture.accept_app_bound(),
        );

        let primary_principal = RolePrincipal::new(RoleOwner::Primary, primary_g1.generation);
        let app_principal = RolePrincipal::new(RoleOwner::AppBound, app_g1.generation);
        let (cap_primary, cap_app) = registry
            .create_role_port_pair(primary_principal, app_principal)
            .expect("mint pair");

        registry
            .send_role_port(cap_primary, primary_principal, b"ping")
            .expect("primary send");
        let msg = registry
            .recv_role_port(cap_app, app_principal)
            .expect("app recv")
            .expect("payload");
        assert_eq!(msg.as_bytes(), b"ping");

        registry.revoke_role_ports(primary_principal);
        let err = registry
            .send_role_port(cap_primary, primary_principal, b"stale")
            .expect_err("stale send");
        assert!(err.to_string().contains("KELD-RUNTIME-005"), "{err}");
        registry.shutdown();
        assert_stopped(registry.primary(), "primary");
        assert_stopped(registry.app_bound(), "app-bound");
        drop(primary_control);
        drop(app_control);
    }

    #[test]
    fn role_port_operations_sync_revocation_and_preserve_lifecycle_event() {
        let fixture = FamilyFixture::new();
        let policy = RestartPolicy {
            max_crashes: 3,
            window_secs: 30,
        };
        let (primary_probe_tx, primary_probe_rx) = mpsc::channel();
        let (app_probe_tx, app_probe_rx) = mpsc::channel();
        let mut registry = RoleRegistry::start(
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
        .expect("roles spawn");

        let primary_g1 = recv_probe(&primary_probe_rx, "primary g1");
        let app_g1 = recv_probe(&app_probe_rx, "app-bound g1");
        let mut primary_control = bind_attempt(
            &mut registry,
            RoleOwner::Primary,
            &primary_g1,
            1,
            fixture.accept_primary(),
        );
        let app_control = bind_attempt(
            &mut registry,
            RoleOwner::AppBound,
            &app_g1,
            1,
            fixture.accept_app_bound(),
        );

        let primary_principal = RolePrincipal::new(RoleOwner::Primary, primary_g1.generation);
        let app_principal = RolePrincipal::new(RoleOwner::AppBound, app_g1.generation);
        let (cap_primary, cap_app) = registry
            .create_role_port_pair(primary_principal, app_principal)
            .expect("mint pair");

        primary_control.write_line("CRASH");
        let primary_g2 = recv_probe(&primary_probe_rx, "primary g2 after crash");

        let err = registry
            .send_role_port(cap_primary, primary_principal, b"stale")
            .expect_err("synchronized send must reject the revoked generation");
        assert!(err.to_string().contains("KELD-RUNTIME-005"), "{err}");
        let err = registry
            .recv_role_port(cap_app, app_principal)
            .expect_err("synchronized receive must observe the revoked pair");
        assert!(err.to_string().contains("KELD-RUNTIME-006"), "{err}");
        let err = registry
            .transfer_role_port(cap_app, app_principal, primary_principal)
            .expect_err("synchronized transfer must observe the revoked target");
        assert!(err.to_string().contains("KELD-RUNTIME-005"), "{err}");
        let err = registry
            .close_role_port(cap_app, app_principal)
            .expect_err("synchronized close must observe the revoked pair");
        assert!(err.to_string().contains("KELD-RUNTIME-006"), "{err}");
        let disconnect = registry
            .poll_role_port_disconnect(cap_app, app_principal)
            .expect("synchronized disconnect polling");
        assert!(matches!(
            disconnect,
            Some(super::PortDisconnectReason::GenerationRevoked)
        ));
        assert!(matches!(
            registry.try_recv_primary_event(),
            Some(RoleEvent::Revoked {
                generation,
                attempt: 1,
                cause: RoleRevocationCause::ChildExited,
            }) if generation == primary_g1.generation
        ));
        assert!(matches!(
            registry.try_recv_primary_event(),
            Some(RoleEvent::Provisioned {
                generation,
                attempt: 2,
            }) if generation == primary_g2.generation
        ));
        assert!(matches!(
            registry.try_recv_primary_event(),
            Some(RoleEvent::Spawned {
                generation,
                attempt: 2,
                ..
            }) if generation == primary_g2.generation
        ));

        registry.shutdown();
        assert_stopped(registry.primary(), "primary");
        assert_stopped(registry.app_bound(), "app-bound");
        drop(primary_control);
        drop(app_control);
    }

    #[test]
    fn registry_shutdown_revokes_live_virtual_port_routes() {
        let fixture = FamilyFixture::new();
        let policy = RestartPolicy {
            max_crashes: 3,
            window_secs: 30,
        };
        let (primary_probe_tx, primary_probe_rx) = mpsc::channel();
        let (app_probe_tx, app_probe_rx) = mpsc::channel();
        let mut registry = RoleRegistry::start(
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
        .expect("roles spawn");

        let primary_g1 = recv_probe(&primary_probe_rx, "primary g1");
        let app_g1 = recv_probe(&app_probe_rx, "app-bound g1");
        let primary_control = bind_attempt(
            &mut registry,
            RoleOwner::Primary,
            &primary_g1,
            1,
            fixture.accept_primary(),
        );
        let app_control = bind_attempt(
            &mut registry,
            RoleOwner::AppBound,
            &app_g1,
            1,
            fixture.accept_app_bound(),
        );

        let primary_principal = RolePrincipal::new(RoleOwner::Primary, primary_g1.generation);
        let app_principal = RolePrincipal::new(RoleOwner::AppBound, app_g1.generation);
        let (cap_primary, cap_app) = registry
            .create_role_port_pair(primary_principal, app_principal)
            .expect("mint pair");
        registry
            .send_role_port(cap_primary, primary_principal, b"pre-shutdown")
            .expect("send before shutdown");

        registry.shutdown();
        let err = registry
            .send_role_port(cap_app, app_principal, b"after-shutdown")
            .expect_err("shutdown must revoke routes");
        assert!(
            err.to_string().contains("KELD-RUNTIME-005")
                || err.to_string().contains("KELD-RUNTIME-006"),
            "{err}"
        );
        assert_stopped(registry.primary(), "primary");
        assert_stopped(registry.app_bound(), "app-bound");
        drop(primary_control);
        drop(app_control);
    }

    #[test]
    fn create_role_port_pair_rejects_unregistered_generation() {
        let fixture = FamilyFixture::new();
        let policy = RestartPolicy {
            max_crashes: 3,
            window_secs: 30,
        };
        let (primary_probe_tx, primary_probe_rx) = mpsc::channel();
        let (app_probe_tx, app_probe_rx) = mpsc::channel();
        let mut registry = RoleRegistry::start(
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
        .expect("roles spawn");

        let primary_g1 = recv_probe(&primary_probe_rx, "primary g1");
        let app_g1 = recv_probe(&app_probe_rx, "app-bound g1");
        let _primary_control = bind_attempt(
            &mut registry,
            RoleOwner::Primary,
            &primary_g1,
            1,
            fixture.accept_primary(),
        );
        let _app_control = bind_attempt(
            &mut registry,
            RoleOwner::AppBound,
            &app_g1,
            1,
            fixture.accept_app_bound(),
        );

        let bogus_primary =
            RolePrincipal::new(RoleOwner::Primary, RoleGeneration::from_test_counter(999));
        let app_principal = RolePrincipal::new(RoleOwner::AppBound, app_g1.generation);
        let err = registry
            .create_role_port_pair(bogus_primary, app_principal)
            .expect_err("unregistered primary generation");
        assert!(err.to_string().contains("KELD-RUNTIME-005"), "{err}");
        registry.shutdown();
        assert_stopped(registry.primary(), "primary");
        assert_stopped(registry.app_bound(), "app-bound");
    }
}
