//! Host-owned bounded virtual port pairs between authenticated roles (KEL-75 T3).
//!
//! The host mints each pair, mediates every send and one-shot transfer, and
//! invalidates routes when a role generation is revoked. There is no direct
//! peer transport — delivery always passes through [`VirtualPortRegistry`].

use std::collections::{HashMap, HashSet, VecDeque};

use crate::unix_role::{RoleGeneration, RoleOwner};

/// Default maximum queued messages per port end before send is rejected.
pub const DEFAULT_PORT_QUEUE_CAPACITY: usize = 64;

/// Maximum inline payload bytes per port message in T3.
pub const MAX_PORT_MESSAGE_LEN: usize = 4096;

/// Host-minted principal for one authenticated role generation.
///
/// A generation is not a PID, socket path, token, or wire field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RolePrincipal {
    owner: RoleOwner,
    generation: RoleGeneration,
}

impl RolePrincipal {
    /// Creates a principal for one live role generation.
    #[must_use]
    pub const fn new(owner: RoleOwner, generation: RoleGeneration) -> Self {
        Self { owner, generation }
    }

    /// Lifecycle owner category.
    #[must_use]
    pub const fn owner(self) -> RoleOwner {
        self.owner
    }

    /// Host-minted generation for this coordinator instance.
    #[must_use]
    pub const fn generation(self) -> RoleGeneration {
        self.generation
    }
}

/// Monotonic host-only port-pair generation.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct VirtualPortGeneration(u64);

impl std::fmt::Debug for VirtualPortGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VirtualPortGeneration(..)")
    }
}

/// One end of a host-owned port pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortEnd {
    /// First end minted for a pair.
    A,
    /// Second end minted for a pair.
    B,
}

/// Capability referencing one live end of a port pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortCapability {
    generation: VirtualPortGeneration,
    end: PortEnd,
}

impl PortCapability {
    /// Pair generation this capability belongs to.
    #[must_use]
    pub const fn generation(self) -> VirtualPortGeneration {
        self.generation
    }

    /// Which end of the pair this capability names.
    #[must_use]
    pub const fn end(self) -> PortEnd {
        self.end
    }
}

/// One bounded inline message routed through the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMessage(Vec<u8>);

impl PortMessage {
    /// Wraps an inline payload copy.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    /// Payload bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Why a port end observed disconnect exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDisconnectReason {
    /// The owning principal closed this end.
    LocalClose,
    /// The paired end was closed by its owner.
    PeerClose,
    /// A role generation owning this end was revoked.
    GenerationRevoked,
}

/// Virtual port routing and transfer failures. `KELD-RUNTIME-004`–`KELD-RUNTIME-011`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtualPortError {
    /// The caller does not own the referenced end.
    NotOwner {
        /// End that was referenced.
        capability: PortCapability,
        /// Principal presented by the caller.
        presented: RolePrincipal,
    },
    /// The presented principal generation is no longer live for this pair.
    StaleGeneration {
        /// End that was referenced.
        capability: PortCapability,
        /// Principal presented by the caller.
        presented: RolePrincipal,
    },
    /// The end or pair is closed or revoked.
    Closed {
        /// End that was referenced.
        capability: PortCapability,
    },
    /// Transfer target is the current owner.
    SelfTransfer {
        /// End that was referenced.
        capability: PortCapability,
        /// Principal presented as transfer target.
        target: RolePrincipal,
    },
    /// This end was already transferred once.
    DuplicateTransfer {
        /// End that was referenced.
        capability: PortCapability,
    },
    /// The original owner attempted transfer after transferring away.
    SourceTransfer {
        /// End that was referenced.
        capability: PortCapability,
        /// Principal that minted the end but no longer owns it.
        source: RolePrincipal,
    },
    /// The receiver queue is full.
    QueueFull {
        /// End whose peer queue is full.
        capability: PortCapability,
        /// Configured capacity.
        capacity: usize,
    },
    /// Payload exceeds the configured 4 KiB maximum message length.
    InvalidMessageLen {
        /// Attempted length in bytes.
        len: usize,
        /// Maximum permitted length.
        max: usize,
    },
    /// A foreign principal would have received a message or disconnect.
    ForeignDelivery {
        /// End that would have received data.
        capability: PortCapability,
        /// Principal that attempted receive.
        presented: RolePrincipal,
    },
}

impl std::fmt::Display for VirtualPortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOwner { .. } => write!(
                f,
                "KELD-RUNTIME-004: virtual port end is not owned by the presented principal. \
                 Use the capability minted for the live role generation."
            ),
            Self::StaleGeneration { .. } => write!(
                f,
                "KELD-RUNTIME-005: virtual port principal generation is stale. \
                 Provision a fresh role generation before routing or transferring ports."
            ),
            Self::Closed { .. } => write!(
                f,
                "KELD-RUNTIME-006: virtual port end is closed or its pair was revoked. \
                 Create a new host-owned pair for the live role generations."
            ),
            Self::SelfTransfer { .. } => write!(
                f,
                "KELD-RUNTIME-007: virtual port transfer target is the current owner. \
                 Choose a different authenticated role generation."
            ),
            Self::DuplicateTransfer { .. } => write!(
                f,
                "KELD-RUNTIME-008: virtual port end was already transferred once. \
                 Port transfer is one-shot per end generation."
            ),
            Self::SourceTransfer { .. } => write!(
                f,
                "KELD-RUNTIME-009: original virtual port owner cannot transfer after \
                 relinquishing the end. Use the current owner's capability."
            ),
            Self::QueueFull { capacity, .. } => write!(
                f,
                "KELD-RUNTIME-010: virtual port queue is full at {capacity} messages. \
                 Drain the peer or close the end before sending more."
            ),
            Self::InvalidMessageLen { len, max } => write!(
                f,
                "KELD-RUNTIME-011: virtual port message length {len} exceeds inline limit {max}. \
                 Split the payload or use a later bulk lane when available."
            ),
            Self::ForeignDelivery { .. } => write!(
                f,
                "KELD-RUNTIME-004: foreign principal cannot receive on this virtual port end. \
                 Use the capability minted for the live role generation."
            ),
        }
    }
}

impl std::error::Error for VirtualPortError {}

/// Host-owned registry of bounded virtual port pairs.
#[derive(Debug)]
pub(crate) struct VirtualPortRegistry {
    next_generation: u64,
    queue_capacity: usize,
    pairs: HashMap<VirtualPortGeneration, PairState>,
    live_generations: HashSet<RolePrincipal>,
    revoked_generations: HashSet<RolePrincipal>,
}

#[derive(Debug)]
struct PairState {
    end_a: EndState,
    end_b: EndState,
    revoked: bool,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)] // transfer, close, and disconnect are distinct AC5 states.
struct EndState {
    initial_owner: RolePrincipal,
    owner: RolePrincipal,
    transferred: bool,
    closed: bool,
    disconnect_pending: bool,
    disconnect_observed: bool,
    queue: VecDeque<PortMessage>,
}

impl VirtualPortRegistry {
    /// Creates a registry with the default queue capacity.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::with_queue_capacity(DEFAULT_PORT_QUEUE_CAPACITY)
    }

    /// Creates a registry with a custom per-end queue bound.
    #[must_use]
    pub(crate) fn with_queue_capacity(queue_capacity: usize) -> Self {
        Self {
            next_generation: 1,
            queue_capacity,
            pairs: HashMap::new(),
            live_generations: HashSet::new(),
            revoked_generations: HashSet::new(),
        }
    }

    /// Records one live role generation that may mint or receive port pairs.
    pub(crate) fn register_generation(&mut self, principal: RolePrincipal) {
        if !self.is_generation_revoked(principal) {
            self.live_generations.insert(principal);
        }
    }

    /// Snapshot of principals currently registered as live.
    #[must_use]
    pub(crate) fn live_principals(&self) -> Vec<RolePrincipal> {
        self.live_generations.iter().copied().collect()
    }

    /// Mints a fresh pair between two authenticated role principals.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError::StaleGeneration`] when either principal is
    /// unregistered, not live, or was previously revoked in this registry.
    pub(crate) fn create_pair(
        &mut self,
        owner_a: RolePrincipal,
        owner_b: RolePrincipal,
    ) -> Result<(PortCapability, PortCapability), VirtualPortError> {
        if let Some(presented) = stale_principal(owner_a, owner_b, self) {
            return Err(stale_generation_at_create(presented));
        }
        if !self.is_generation_live(owner_a) {
            return Err(stale_generation_at_create(owner_a));
        }
        if !self.is_generation_live(owner_b) {
            return Err(stale_generation_at_create(owner_b));
        }
        let generation = self.mint_generation()?;
        let cap_a = PortCapability {
            generation,
            end: PortEnd::A,
        };
        let cap_b = PortCapability {
            generation,
            end: PortEnd::B,
        };
        self.pairs.insert(
            generation,
            PairState {
                end_a: EndState::new(owner_a, self.queue_capacity),
                end_b: EndState::new(owner_b, self.queue_capacity),
                revoked: false,
            },
        );
        Ok((cap_a, cap_b))
    }

    /// Sends one inline message from `from` through the host to the peer end.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] when ownership, closure, queue bounds, or
    /// generation validity fail.
    pub(crate) fn send(
        &mut self,
        from: PortCapability,
        sender: RolePrincipal,
        payload: &[u8],
    ) -> Result<(), VirtualPortError> {
        if self.is_generation_revoked(sender) {
            return Err(VirtualPortError::StaleGeneration {
                capability: from,
                presented: sender,
            });
        }
        if payload.len() > MAX_PORT_MESSAGE_LEN {
            return Err(VirtualPortError::InvalidMessageLen {
                len: payload.len(),
                max: MAX_PORT_MESSAGE_LEN,
            });
        }
        let capacity = self.queue_capacity;
        let pair = self
            .pairs
            .get(&from.generation)
            .ok_or(VirtualPortError::Closed { capability: from })?;
        if pair.revoked {
            return Err(VirtualPortError::Closed { capability: from });
        }
        let (from_end, peer_end) = pair.ends_ref(from.end);
        check_end_owner(from, sender, from_end)?;
        if from_end.closed || peer_end.closed {
            return Err(VirtualPortError::Closed { capability: from });
        }
        if peer_end.queue.len() >= capacity {
            return Err(VirtualPortError::QueueFull {
                capability: from,
                capacity,
            });
        }
        let pair = self
            .pairs
            .get_mut(&from.generation)
            .ok_or(VirtualPortError::Closed { capability: from })?;
        let (_, peer_end) = pair.ends(from.end);
        peer_end.queue.push_back(PortMessage::from_bytes(payload));
        Ok(())
    }

    /// Transfers one end once to a host-approved target principal.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] for duplicate, self, source, closed, or
    /// stale-generation transfers.
    pub(crate) fn transfer(
        &mut self,
        capability: PortCapability,
        from: RolePrincipal,
        target: RolePrincipal,
    ) -> Result<PortCapability, VirtualPortError> {
        if self.is_generation_revoked(from) {
            return Err(VirtualPortError::StaleGeneration {
                capability,
                presented: from,
            });
        }
        if self.is_generation_revoked(target) {
            return Err(VirtualPortError::StaleGeneration {
                capability,
                presented: target,
            });
        }
        if !self.is_generation_live(target) {
            return Err(VirtualPortError::StaleGeneration {
                capability,
                presented: target,
            });
        }
        let pair = self.pair_mut(capability.generation)?;
        if pair.revoked {
            return Err(VirtualPortError::Closed { capability });
        }
        let (end, peer) = match capability.end {
            PortEnd::A => (&mut pair.end_a, &pair.end_b),
            PortEnd::B => (&mut pair.end_b, &pair.end_a),
        };
        if end.closed || peer.closed {
            return Err(VirtualPortError::Closed { capability });
        }
        if end.owner != from {
            if end.initial_owner == from {
                return Err(VirtualPortError::SourceTransfer {
                    capability,
                    source: from,
                });
            }
            return Err(VirtualPortError::NotOwner {
                capability,
                presented: from,
            });
        }
        if target == end.owner {
            return Err(VirtualPortError::SelfTransfer { capability, target });
        }
        if end.transferred {
            return Err(VirtualPortError::DuplicateTransfer { capability });
        }
        end.owner = target;
        end.transferred = true;
        Ok(capability)
    }

    /// Closes one end owned by `owner`.
    ///
    /// The peer observes exactly one disconnect through [`Self::poll_disconnect`].
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError`] when the end is already closed, revoked, or
    /// not owned by `owner`.
    pub(crate) fn close(
        &mut self,
        capability: PortCapability,
        owner: RolePrincipal,
    ) -> Result<(), VirtualPortError> {
        let pair = self
            .pairs
            .get(&capability.generation)
            .ok_or(VirtualPortError::Closed { capability })?;
        if pair.revoked {
            return Err(VirtualPortError::Closed { capability });
        }
        let (end, _peer) = pair.ends_ref(capability.end);
        check_end_owner(capability, owner, end)?;
        if end.closed {
            return Err(VirtualPortError::Closed { capability });
        }
        let pair = self
            .pairs
            .get_mut(&capability.generation)
            .ok_or(VirtualPortError::Closed { capability })?;
        let (end, peer) = pair.ends(capability.end);
        end.closed = true;
        if !peer.closed && !peer.disconnect_observed {
            peer.disconnect_pending = true;
        }
        Ok(())
    }

    /// Receives the next queued message for `owner`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError::ForeignDelivery`] when `owner` is not the
    /// live holder of the end.
    pub(crate) fn recv(
        &mut self,
        capability: PortCapability,
        owner: RolePrincipal,
    ) -> Result<Option<PortMessage>, VirtualPortError> {
        if self.is_generation_revoked(owner) {
            return Err(VirtualPortError::StaleGeneration {
                capability,
                presented: owner,
            });
        }
        let pair = self
            .pairs
            .get(&capability.generation)
            .ok_or(VirtualPortError::Closed { capability })?;
        if pair.revoked {
            return Err(VirtualPortError::Closed { capability });
        }
        let end = pair.end_ref(capability.end);
        check_end_owner(capability, owner, end)?;
        let pair = self
            .pairs
            .get_mut(&capability.generation)
            .ok_or(VirtualPortError::Closed { capability })?;
        Ok(pair.end_mut(capability.end).queue.pop_front())
    }

    /// Observes disconnect exactly once for `owner`.
    ///
    /// # Errors
    ///
    /// Returns [`VirtualPortError::ForeignDelivery`] when `owner` is not the
    /// live holder of the end.
    pub(crate) fn poll_disconnect(
        &mut self,
        capability: PortCapability,
        owner: RolePrincipal,
    ) -> Result<Option<PortDisconnectReason>, VirtualPortError> {
        if self.is_generation_revoked(owner) {
            return Err(VirtualPortError::StaleGeneration {
                capability,
                presented: owner,
            });
        }
        let pair = self
            .pairs
            .get(&capability.generation)
            .ok_or(VirtualPortError::Closed { capability })?;
        let revoked = pair.revoked;
        let end = pair.end_ref(capability.end);
        check_end_owner(capability, owner, end)?;
        let pair = self
            .pairs
            .get_mut(&capability.generation)
            .ok_or(VirtualPortError::Closed { capability })?;
        let end = pair.end_mut(capability.end);
        if end.disconnect_observed {
            return Ok(None);
        }
        if revoked {
            end.disconnect_observed = true;
            return Ok(Some(PortDisconnectReason::GenerationRevoked));
        }
        if end.disconnect_pending {
            end.disconnect_observed = true;
            end.disconnect_pending = false;
            return Ok(Some(PortDisconnectReason::PeerClose));
        }
        if end.closed {
            end.disconnect_observed = true;
            return Ok(Some(PortDisconnectReason::LocalClose));
        }
        Ok(None)
    }

    /// Revokes every route owned by one role generation.
    pub(crate) fn revoke_generation(&mut self, principal: RolePrincipal) {
        self.mark_generation_revoked(principal);
        self.live_generations.remove(&principal);
        for pair in self.pairs.values_mut() {
            if pair.revoked {
                continue;
            }
            for end in [&mut pair.end_a, &mut pair.end_b] {
                if end.owner == principal {
                    end.closed = true;
                }
                if (end.owner == principal || end.initial_owner == principal)
                    && !end.disconnect_observed
                {
                    end.disconnect_pending = true;
                }
            }
            if pair.end_a.initial_owner == principal
                || pair.end_b.initial_owner == principal
                || pair.end_a.owner == principal
                || pair.end_b.owner == principal
            {
                pair.revoked = true;
                pair.end_a.queue.clear();
                pair.end_b.queue.clear();
            }
        }
    }

    fn mint_generation(&mut self) -> Result<VirtualPortGeneration, VirtualPortError> {
        let generation = VirtualPortGeneration(self.next_generation);
        self.next_generation =
            self.next_generation
                .checked_add(1)
                .ok_or(VirtualPortError::Closed {
                    capability: PortCapability {
                        generation: VirtualPortGeneration(0),
                        end: PortEnd::A,
                    },
                })?;
        Ok(generation)
    }

    fn pair_mut(
        &mut self,
        generation: VirtualPortGeneration,
    ) -> Result<&mut PairState, VirtualPortError> {
        self.pairs
            .get_mut(&generation)
            .ok_or(VirtualPortError::Closed {
                capability: PortCapability {
                    generation,
                    end: PortEnd::A,
                },
            })
    }

    fn is_generation_live(&self, principal: RolePrincipal) -> bool {
        self.live_generations.contains(&principal)
    }

    fn is_generation_revoked(&self, principal: RolePrincipal) -> bool {
        self.revoked_generations.contains(&principal)
    }

    fn mark_generation_revoked(&mut self, principal: RolePrincipal) {
        self.revoked_generations.insert(principal);
    }
}

impl Default for VirtualPortRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EndState {
    fn new(owner: RolePrincipal, _queue_capacity: usize) -> Self {
        Self {
            initial_owner: owner,
            owner,
            transferred: false,
            closed: false,
            disconnect_pending: false,
            disconnect_observed: false,
            queue: VecDeque::new(),
        }
    }
}

impl PairState {
    fn ends(&mut self, end: PortEnd) -> (&mut EndState, &mut EndState) {
        match end {
            PortEnd::A => (&mut self.end_a, &mut self.end_b),
            PortEnd::B => (&mut self.end_b, &mut self.end_a),
        }
    }

    fn ends_ref(&self, end: PortEnd) -> (&EndState, &EndState) {
        match end {
            PortEnd::A => (&self.end_a, &self.end_b),
            PortEnd::B => (&self.end_b, &self.end_a),
        }
    }

    fn end_mut(&mut self, end: PortEnd) -> &mut EndState {
        match end {
            PortEnd::A => &mut self.end_a,
            PortEnd::B => &mut self.end_b,
        }
    }

    fn end_ref(&self, end: PortEnd) -> &EndState {
        match end {
            PortEnd::A => &self.end_a,
            PortEnd::B => &self.end_b,
        }
    }
}

fn check_end_owner(
    capability: PortCapability,
    presented: RolePrincipal,
    end: &EndState,
) -> Result<(), VirtualPortError> {
    if end.owner != presented {
        return Err(VirtualPortError::ForeignDelivery {
            capability,
            presented,
        });
    }
    Ok(())
}

fn stale_principal(
    owner_a: RolePrincipal,
    owner_b: RolePrincipal,
    registry: &VirtualPortRegistry,
) -> Option<RolePrincipal> {
    if registry.is_generation_revoked(owner_a) {
        Some(owner_a)
    } else if registry.is_generation_revoked(owner_b) {
        Some(owner_b)
    } else {
        None
    }
}

fn stale_generation_at_create(presented: RolePrincipal) -> VirtualPortError {
    VirtualPortError::StaleGeneration {
        capability: PortCapability {
            generation: VirtualPortGeneration(0),
            end: if presented.owner() == RoleOwner::AppBound {
                PortEnd::B
            } else {
                PortEnd::A
            },
        },
        presented,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(owner: RoleOwner, generation: u64) -> RolePrincipal {
        RolePrincipal::new(owner, RoleGeneration::from_test_counter(generation))
    }

    fn primary(g: u64) -> RolePrincipal {
        principal(RoleOwner::Primary, g)
    }

    fn app_bound(g: u64) -> RolePrincipal {
        principal(RoleOwner::AppBound, g)
    }

    fn register_pair(
        registry: &mut VirtualPortRegistry,
        owner_a: RolePrincipal,
        owner_b: RolePrincipal,
    ) {
        registry.register_generation(owner_a);
        registry.register_generation(owner_b);
    }

    #[test]
    fn contract_fixture_ordered_delivery_and_fifo() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        let (cap_p, cap_a) = registry.create_pair(p1, a1).expect("create pair");
        registry.send(cap_p, p1, b"one").expect("send one");
        registry.send(cap_p, p1, b"two").expect("send two");
        registry.send(cap_p, p1, b"three").expect("send three");
        let first = registry.recv(cap_a, a1).expect("recv one").expect("one");
        let second = registry.recv(cap_a, a1).expect("recv two").expect("two");
        let third = registry
            .recv(cap_a, a1)
            .expect("recv three")
            .expect("three");
        assert_eq!(first.as_bytes(), b"one");
        assert_eq!(second.as_bytes(), b"two");
        assert_eq!(third.as_bytes(), b"three");
        assert!(
            registry.recv(cap_a, a1).expect("empty recv").is_none(),
            "queue drained"
        );
    }

    #[test]
    fn peer_observes_generation_revoked_once_after_revoke() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        let (_, cap_a) = registry.create_pair(p1, a1).expect("create pair");
        registry.revoke_generation(p1);
        let reason = registry
            .poll_disconnect(cap_a, a1)
            .expect("poll disconnect")
            .expect("generation revoked");
        assert_eq!(reason, PortDisconnectReason::GenerationRevoked);
        assert!(
            registry
                .poll_disconnect(cap_a, a1)
                .expect("second poll")
                .is_none(),
            "revocation disconnect must be observed exactly once"
        );
    }

    #[test]
    fn transfer_rejects_unregistered_target() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        let (_, cap_a) = registry.create_pair(p1, a1).expect("create pair");
        let unregistered = primary(99);
        let err = registry
            .transfer(cap_a, a1, unregistered)
            .expect_err("unregistered transfer target");
        assert!(err.to_string().contains("KELD-RUNTIME-005"), "{err}");
    }

    #[test]
    fn contract_fixture_exactly_once_disconnect_on_peer_close() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        let (cap_p, cap_a) = registry.create_pair(p1, a1).expect("create pair");
        registry.close(cap_p, p1).expect("close primary end");
        let first = registry
            .poll_disconnect(cap_a, a1)
            .expect("peer disconnect")
            .expect("one disconnect");
        assert_eq!(first, PortDisconnectReason::PeerClose);
        assert!(
            registry
                .poll_disconnect(cap_a, a1)
                .expect("second poll")
                .is_none(),
            "disconnect must be observed exactly once"
        );
        assert!(
            registry
                .poll_disconnect(cap_a, a1)
                .expect("third poll")
                .is_none(),
            "disconnect must stay exhausted"
        );
    }

    #[test]
    fn contract_fixture_no_foreign_delivery_on_send_or_recv() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        let (cap_p, cap_a) = registry.create_pair(p1, a1).expect("create pair");
        let err = registry
            .send(cap_p, a1, b"hostile")
            .expect_err("foreign send");
        assert!(err.to_string().contains("KELD-RUNTIME-004"), "{err}");
        registry.send(cap_p, p1, b"ok").expect("owner send");
        let err = registry.recv(cap_a, p1).expect_err("foreign recv");
        assert!(err.to_string().contains("KELD-RUNTIME-004"), "{err}");
        let msg = registry.recv(cap_a, a1).expect("owner recv").expect("msg");
        assert_eq!(msg.as_bytes(), b"ok");
    }

    #[test]
    fn transfer_moves_ownership_once_and_rejects_invalid_targets() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        let p2 = primary(2);
        register_pair(&mut registry, p1, a1);
        registry.register_generation(p2);
        let (cap_p, cap_a) = registry.create_pair(p1, a1).expect("create pair");
        let transferred = registry.transfer(cap_a, a1, p2).expect("transfer to p2");
        assert_eq!(transferred, cap_a);
        let err = registry
            .transfer(cap_a, a1, p2)
            .expect_err("source transfer after relinquish");
        assert!(err.to_string().contains("KELD-RUNTIME-009"), "{err}");
        let err = registry
            .transfer(cap_a, p2, p1)
            .expect_err("duplicate transfer");
        assert!(err.to_string().contains("KELD-RUNTIME-008"), "{err}");
        let err = registry.transfer(cap_a, p2, p2).expect_err("self transfer");
        assert!(err.to_string().contains("KELD-RUNTIME-007"), "{err}");
        registry
            .send(cap_a, p2, b"after-transfer")
            .expect("new owner send");
        let msg = registry
            .recv(cap_p, p1)
            .expect("primary recv")
            .expect("payload");
        assert_eq!(msg.as_bytes(), b"after-transfer");
    }

    #[test]
    fn stale_generation_transfer_and_create_fail_closed() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        let (cap_p, _) = registry.create_pair(p1, a1).expect("create pair");
        registry.revoke_generation(p1);
        let err = registry
            .transfer(cap_p, p1, app_bound(2))
            .expect_err("stale transfer");
        assert!(err.to_string().contains("KELD-RUNTIME-005"), "{err}");
        let err = registry
            .create_pair(p1, app_bound(2))
            .expect_err("stale create");
        assert!(err.to_string().contains("KELD-RUNTIME-005"), "{err}");
    }

    #[test]
    fn closed_end_rejects_send_transfer_and_duplicate_close() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        let (cap_p, cap_a) = registry.create_pair(p1, a1).expect("create pair");
        registry.close(cap_a, a1).expect("close app end");
        let err = registry
            .send(cap_p, p1, b"x")
            .expect_err("send after peer close");
        assert!(err.to_string().contains("KELD-RUNTIME-006"), "{err}");
        let a2 = app_bound(2);
        registry.register_generation(a2);
        let err = registry
            .transfer(cap_p, p1, a2)
            .expect_err("transfer after peer close");
        assert!(err.to_string().contains("KELD-RUNTIME-006"), "{err}");
        let err = registry.close(cap_a, a1).expect_err("duplicate close");
        assert!(err.to_string().contains("KELD-RUNTIME-006"), "{err}");
    }

    #[test]
    fn queue_full_rejects_without_delivery() {
        let mut registry = VirtualPortRegistry::with_queue_capacity(2);
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        let (cap_p, cap_a) = registry.create_pair(p1, a1).expect("create pair");
        registry.send(cap_p, p1, b"1").expect("first");
        registry.send(cap_p, p1, b"2").expect("second");
        let err = registry.send(cap_p, p1, b"3").expect_err("queue full");
        assert!(err.to_string().contains("KELD-RUNTIME-010"), "{err}");
        assert_eq!(
            registry
                .recv(cap_a, a1)
                .expect("one")
                .expect("one")
                .as_bytes(),
            b"1"
        );
        assert!(
            registry
                .recv(cap_a, a1)
                .expect("two")
                .expect("two")
                .as_bytes()
                == b"2",
            "third message must not have been delivered"
        );
    }

    #[test]
    fn create_pair_stale_generation_reports_revoked_owner_b() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        register_pair(&mut registry, p1, a1);
        registry.revoke_generation(a1);
        let err = registry.create_pair(p1, a1).expect_err("stale owner b");
        match err {
            VirtualPortError::StaleGeneration { presented, .. } => {
                assert_eq!(presented, a1, "stale principal must name the revoked end");
            }
            other => panic!("expected StaleGeneration, got {other:?}"),
        }
    }

    #[test]
    fn create_pair_rejects_unregistered_generation() {
        let mut registry = VirtualPortRegistry::new();
        let p1 = primary(1);
        let a1 = app_bound(1);
        registry.register_generation(a1);
        let err = registry
            .create_pair(p1, a1)
            .expect_err("unregistered primary");
        match err {
            VirtualPortError::StaleGeneration { presented, .. } => {
                assert_eq!(presented, p1);
            }
            other => panic!("expected StaleGeneration, got {other:?}"),
        }
        assert!(err.to_string().contains("KELD-RUNTIME-005"), "{err}");
    }
}
