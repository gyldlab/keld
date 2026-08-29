//! Executable KEL-75/T4a contract for window-bound lifecycle traces.
//!
//! This portable oracle validates future product observations. It does not add
//! shipping role/window wiring or claim real renderer/reaper acceptance.
#![allow(clippy::expect_used, clippy::panic)] // Integration-test assertions are the oracle.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const ROLE: Role = Role("window-role");
const APP_ROLE: Role = Role("app-role");
const OTHER_ROLE: Role = Role("other-window-role");
const WINDOW: Window = Window(7);
const OTHER_WINDOW: Window = Window(8);
const G1: Generation = Generation(1);
const G2: Generation = Generation(2);
const H1: Handle = Handle(101);
const H2: Handle = Handle(102);
const APP_H1: Handle = Handle(201);
const OTHER_H1: Handle = Handle(301);
const NAVIGATION: Navigation = Navigation(11);
const DOCUMENT: Nonce = Nonce("document-a");
const CHILD_ROLE_ENV: &str = "KELD_T4_CONTRACT_CHILD_ROLE";
const CHILD_TEST: &str = "hostile_shutdown_contract_child";
const SUBPROCESS_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Role(&'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Window(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Generation(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Handle(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Navigation(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Nonce(&'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    App,
    Window(Window),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Authority {
    Locator,
    Link,
    Dispatch,
    Grant,
    Port,
    Mapping,
    PendingCall,
}

impl Authority {
    const ALL: [Self; 7] = [
        Self::Locator,
        Self::Link,
        Self::Dispatch,
        Self::Grant,
        Self::Port,
        Self::Mapping,
        Self::PendingCall,
    ];
    const PRE_DRAIN: [Self; 5] = [
        Self::Dispatch,
        Self::Grant,
        Self::Port,
        Self::Mapping,
        Self::PendingCall,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExitCause {
    Crash,
    PreReadyFailure,
    AdmissionFailure,
    ProtocolFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Provisioned,
    Spawned,
    LinkBound,
    Ready,
    Crashed,
    Quiesced,
    Drained,
    Retired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Declare(Role, Owner),
    CoordinatorRecreated(Role),
    Provision(Role, Generation),
    Spawn(Role, Generation, Handle, u32),
    LinkBound(Role, Generation),
    Ready(Role, Generation),
    Beacon(Role, Generation, Window, Navigation, Nonce, u64),
    Crash(Role, Generation),
    FailBeforeReady(Role, Generation),
    AdmissionFailed(Role, Generation),
    ProtocolFailed(Role, Generation),
    Revoke(Role, Generation, Authority),
    RejectStale(Role, Generation),
    WorkAdmitted(Role, Generation, u64),
    WorkCompleted(Role, Generation, u64),
    WindowClosing(Window),
    Quiesce(Role, Generation),
    Drain(Role, Generation),
    Reap(Role, Generation, Handle),
    RetireUnspawned(Role, Generation),
    ReapByPid(Role, Generation, u32),
    WindowClosed(Window),
    CallCompleted(Role, Generation, u64),
    CleanupFailed(Role),
    SessionQuiescing,
    SessionStopped,
    HostDied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Error {
    GenerationRotation,
    RevokeBeforeSuccessor,
    NaturalExitOrder,
    ReadyOrder,
    StaleGeneration,
    CloseBinding,
    CloseOrder,
    RendererContinuity,
    HandleReap,
    HostDeath,
    Shutdown,
}

#[derive(Debug)]
struct GenerationState {
    generation: Generation,
    handle: Option<Handle>,
    phase: Phase,
    seen: BTreeSet<Phase>,
    exit: Option<ExitCause>,
    revoked: BTreeSet<Authority>,
    admitted_work: BTreeSet<u64>,
}

impl GenerationState {
    fn new(generation: Generation) -> Self {
        Self {
            generation,
            handle: None,
            phase: Phase::Provisioned,
            seen: BTreeSet::from([Phase::Provisioned]),
            exit: None,
            revoked: BTreeSet::new(),
            admitted_work: BTreeSet::new(),
        }
    }

    fn advance(&mut self, phase: Phase) {
        self.phase = phase;
        self.seen.insert(phase);
    }

    fn fully_revoked(&self) -> bool {
        Authority::ALL
            .into_iter()
            .all(|authority| self.revoked.contains(&authority))
    }
}

#[derive(Debug)]
struct RoleState {
    owner: Owner,
    generations: Vec<GenerationState>,
}

impl RoleState {
    fn current(&self) -> Result<&GenerationState, Error> {
        self.generations.last().ok_or(Error::GenerationRotation)
    }

    fn current_mut(&mut self) -> Result<&mut GenerationState, Error> {
        self.generations.last_mut().ok_or(Error::GenerationRotation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Beacon {
    role: Role,
    generation: Generation,
    window: Window,
    navigation: Navigation,
    nonce: Nonce,
    sequence: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Session {
    #[default]
    Running,
    Quiescing,
    Stopped,
    HostDead,
}

#[derive(Debug, Default)]
struct Model {
    roles: BTreeMap<Role, RoleState>,
    handles: BTreeSet<Handle>,
    closing: BTreeSet<Window>,
    closed: BTreeSet<Window>,
    stale: BTreeSet<(Role, Generation)>,
    calls: BTreeSet<(Role, Generation, u64, u64)>,
    beacons: Vec<Beacon>,
    failed_roles: BTreeSet<Role>,
    cleanup_failures: BTreeSet<Role>,
    closed_at: BTreeMap<Window, u64>,
    step: u64,
    session: Session,
}

impl Model {
    fn apply(&mut self, event: Event) -> Result<(), Error> {
        self.step = self.step.saturating_add(1);
        match event {
            Event::Declare(role, owner) => self.declare(role, owner),
            Event::CoordinatorRecreated(role) => self.coordinator_recreated(role),
            Event::Provision(role, generation) => self.provision(role, generation),
            Event::Spawn(role, generation, handle, pid) => {
                self.spawn(role, generation, handle, pid)
            }
            Event::LinkBound(role, generation) => self.link_bound(role, generation),
            Event::Ready(role, generation) => self.ready(role, generation),
            Event::Beacon(role, generation, window, navigation, nonce, sequence) => {
                self.beacon(Beacon {
                    role,
                    generation,
                    window,
                    navigation,
                    nonce,
                    sequence,
                })
            }
            Event::Crash(role, generation) => self.fail(role, generation, ExitCause::Crash),
            Event::FailBeforeReady(role, generation) => {
                self.fail(role, generation, ExitCause::PreReadyFailure)
            }
            Event::AdmissionFailed(role, generation) => self.admission_failed(role, generation),
            Event::ProtocolFailed(role, generation) => self.protocol_failed(role, generation),
            Event::Revoke(role, generation, authority) => self.revoke(role, generation, authority),
            Event::RejectStale(role, generation) => self.reject_stale(role, generation),
            Event::WorkAdmitted(role, generation, work) => self.admit_work(role, generation, work),
            Event::WorkCompleted(role, generation, work) => {
                self.complete_work(role, generation, work)
            }
            Event::WindowClosing(window) => {
                self.closing.insert(window);
                Ok(())
            }
            Event::Quiesce(role, generation) => self.quiesce(role, generation),
            Event::Drain(role, generation) => self.drain(role, generation),
            Event::Reap(role, generation, handle) => self.reap(role, generation, handle),
            Event::RetireUnspawned(role, generation) => self.retire_unspawned(role, generation),
            Event::ReapByPid(role, generation, _) => {
                self.current(role, generation)?;
                Err(Error::HandleReap)
            }
            Event::WindowClosed(window) => self.finish_close(window),
            Event::CallCompleted(role, generation, id) => self.call_completed(role, generation, id),
            Event::CleanupFailed(role) => self.cleanup_failed(role),
            Event::SessionQuiescing => {
                self.session = Session::Quiescing;
                Ok(())
            }
            Event::SessionStopped => self.stop_session(),
            Event::HostDied => {
                self.session = Session::HostDead;
                Ok(())
            }
        }
    }

    fn declare(&mut self, role: Role, owner: Owner) -> Result<(), Error> {
        if self.roles.contains_key(&role) {
            return Err(Error::GenerationRotation);
        }
        self.roles.insert(
            role,
            RoleState {
                owner,
                generations: Vec::new(),
            },
        );
        Ok(())
    }

    fn coordinator_recreated(&self, role: Role) -> Result<(), Error> {
        self.roles
            .contains_key(&role)
            .then_some(())
            .ok_or(Error::GenerationRotation)
    }

    fn provision(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        if self.session != Session::Running {
            return Err(if self.session == Session::HostDead {
                Error::HostDeath
            } else {
                Error::Shutdown
            });
        }
        let state = self.roles.get_mut(&role).ok_or(Error::GenerationRotation)?;
        if self.failed_roles.contains(&role) {
            return Err(Error::ReadyOrder);
        }
        if matches!(state.owner, Owner::Window(window) if self.closing.contains(&window) || self.closed.contains(&window))
        {
            return Err(Error::CloseBinding);
        }
        if let Some(previous) = state.generations.last() {
            if generation <= previous.generation {
                return Err(Error::GenerationRotation);
            }
            if !previous.fully_revoked() {
                return Err(Error::RevokeBeforeSuccessor);
            }
            if previous.phase != Phase::Retired {
                return Err(Error::HandleReap);
            }
        }
        state.generations.push(GenerationState::new(generation));
        Ok(())
    }

    fn spawn(
        &mut self,
        role: Role,
        generation: Generation,
        handle: Handle,
        _: u32,
    ) -> Result<(), Error> {
        self.ensure_running_owner(role)?;
        if !self.handles.insert(handle) {
            return Err(Error::HandleReap);
        }
        let current = self.current_mut(role, generation)?;
        if current.phase != Phase::Provisioned {
            return Err(Error::GenerationRotation);
        }
        current.handle = Some(handle);
        current.advance(Phase::Spawned);
        Ok(())
    }

    fn link_bound(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        self.ensure_running_owner(role)?;
        let current = self.current_mut(role, generation)?;
        if current.phase != Phase::Spawned {
            return Err(Error::ReadyOrder);
        }
        current.advance(Phase::LinkBound);
        Ok(())
    }

    fn ready(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        self.ensure_running_owner(role)?;
        let current = self.current_mut(role, generation)?;
        if current.phase != Phase::LinkBound {
            return Err(Error::ReadyOrder);
        }
        current.advance(Phase::Ready);
        Ok(())
    }

    fn beacon(&mut self, beacon: Beacon) -> Result<(), Error> {
        let role = self
            .roles
            .get(&beacon.role)
            .ok_or(Error::RendererContinuity)?;
        if role.owner != Owner::Window(beacon.window)
            || role.current()?.generation != beacon.generation
            || role.current()?.phase != Phase::Ready
        {
            return Err(Error::RendererContinuity);
        }
        self.beacons.push(beacon);
        Ok(())
    }

    fn fail(&mut self, role: Role, generation: Generation, cause: ExitCause) -> Result<(), Error> {
        let current = self.current_mut(role, generation)?;
        let valid = match cause {
            ExitCause::Crash => current.phase == Phase::Ready,
            ExitCause::PreReadyFailure => current.phase == Phase::LinkBound,
            ExitCause::AdmissionFailure | ExitCause::ProtocolFailure => false,
        };
        if !valid {
            return Err(Error::ReadyOrder);
        }
        current.exit = Some(cause);
        current.advance(Phase::Crashed);
        Ok(())
    }

    fn admission_failed(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        let current = self.current_mut(role, generation)?;
        if !matches!(current.phase, Phase::Provisioned | Phase::Spawned) {
            return Err(Error::ReadyOrder);
        }
        current.exit = Some(ExitCause::AdmissionFailure);
        current.advance(Phase::Crashed);
        self.failed_roles.insert(role);
        Ok(())
    }

    fn protocol_failed(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        let current = self.current_mut(role, generation)?;
        if !matches!(current.phase, Phase::LinkBound | Phase::Ready) {
            return Err(Error::ReadyOrder);
        }
        current.exit = Some(ExitCause::ProtocolFailure);
        current.advance(Phase::Crashed);
        self.failed_roles.insert(role);
        Ok(())
    }

    fn revoke(
        &mut self,
        role: Role,
        generation: Generation,
        authority: Authority,
    ) -> Result<(), Error> {
        let owner = self.roles.get(&role).ok_or(Error::CloseOrder)?.owner;
        if let Owner::Window(window) = owner
            && self.session == Session::Running
            && self.current(role, generation)?.exit.is_none()
            && (!self.window_unavailable(window)
                || (Authority::PRE_DRAIN.contains(&authority)
                    && self.current(role, generation)?.phase != Phase::Quiesced)
                || (matches!(authority, Authority::Link | Authority::Locator)
                    && self.current(role, generation)?.phase != Phase::Drained))
        {
            return Err(Error::CloseOrder);
        }
        let session = self.session;
        let current = self.current_mut(role, generation)?;
        if session == Session::Running
            && matches!(
                current.exit,
                Some(ExitCause::Crash | ExitCause::PreReadyFailure)
            )
            && current.phase != Phase::Retired
        {
            return Err(Error::NaturalExitOrder);
        }
        if current.phase == Phase::Retired
            && !(session == Session::Running
                && matches!(
                    current.exit,
                    Some(ExitCause::Crash | ExitCause::PreReadyFailure)
                ))
        {
            return Err(Error::Shutdown);
        }
        current.revoked.insert(authority);
        Ok(())
    }

    fn reject_stale(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        let state = self.roles.get(&role).ok_or(Error::StaleGeneration)?;
        let retired = state
            .generations
            .iter()
            .find(|candidate| candidate.generation == generation)
            .ok_or(Error::StaleGeneration)?;
        if retired.phase != Phase::Retired || !retired.fully_revoked() {
            return Err(Error::StaleGeneration);
        }
        self.stale.insert((role, generation));
        Ok(())
    }

    fn admit_work(&mut self, role: Role, generation: Generation, work: u64) -> Result<(), Error> {
        self.ensure_running_owner(role)
            .map_err(|_| Error::CloseOrder)?;
        let current = self.current_mut(role, generation)?;
        if current.phase != Phase::Ready || !current.admitted_work.insert(work) {
            return Err(Error::CloseOrder);
        }
        Ok(())
    }

    fn complete_work(
        &mut self,
        role: Role,
        generation: Generation,
        work: u64,
    ) -> Result<(), Error> {
        if self
            .current_mut(role, generation)?
            .admitted_work
            .remove(&work)
        {
            Ok(())
        } else {
            Err(Error::CloseOrder)
        }
    }

    fn quiesce(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        let owner = self.roles.get(&role).ok_or(Error::CloseBinding)?.owner;
        let allowed = matches!(owner, Owner::Window(window) if self.window_unavailable(window))
            || matches!(self.session, Session::Quiescing | Session::HostDead);
        if !allowed {
            return Err(Error::CloseOrder);
        }
        self.current_mut(role, generation)?.advance(Phase::Quiesced);
        Ok(())
    }

    fn drain(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        let current = self.current_mut(role, generation)?;
        if current.phase != Phase::Quiesced
            || !Authority::PRE_DRAIN
                .into_iter()
                .all(|authority| current.revoked.contains(&authority))
            || current.revoked.contains(&Authority::Link)
            || current.revoked.contains(&Authority::Locator)
            || !current.admitted_work.is_empty()
        {
            return Err(Error::CloseOrder);
        }
        current.advance(Phase::Drained);
        Ok(())
    }

    fn reap(&mut self, role: Role, generation: Generation, handle: Handle) -> Result<(), Error> {
        let host_dead = self.session == Session::HostDead;
        let shutdown = self.session == Session::Quiescing;
        let closing = self
            .role_window(role)
            .is_some_and(|window| self.window_unavailable(window));
        let current = self.current_mut(role, generation)?;
        if current.handle != Some(handle) {
            return Err(Error::HandleReap);
        }
        Self::terminal_order(current, host_dead, shutdown, closing)?;
        current.handle = None;
        current.advance(Phase::Retired);
        Ok(())
    }

    fn retire_unspawned(&mut self, role: Role, generation: Generation) -> Result<(), Error> {
        let shutdown = self.session == Session::Quiescing;
        let closing = self
            .role_window(role)
            .is_some_and(|window| self.window_unavailable(window));
        let current = self.current_mut(role, generation)?;
        if current.handle.is_some() {
            return Err(Error::HandleReap);
        }
        Self::terminal_order(current, false, shutdown, closing)?;
        current.advance(Phase::Retired);
        Ok(())
    }

    fn terminal_order(
        current: &GenerationState,
        host_dead: bool,
        shutdown: bool,
        closing: bool,
    ) -> Result<(), Error> {
        if matches!(
            current.exit,
            Some(ExitCause::AdmissionFailure | ExitCause::ProtocolFailure)
        ) && !current.fully_revoked()
        {
            return Err(Error::RevokeBeforeSuccessor);
        }
        if host_dead && !current.fully_revoked() {
            return Err(Error::HostDeath);
        }
        if shutdown && (current.phase != Phase::Drained || !current.fully_revoked()) {
            return Err(Error::Shutdown);
        }
        if closing && (current.phase != Phase::Drained || !current.fully_revoked()) {
            return Err(Error::CloseOrder);
        }
        Ok(())
    }

    fn finish_close(&mut self, window: Window) -> Result<(), Error> {
        if !self.closing.contains(&window)
            || self.roles.values().any(|role| {
                role.owner == Owner::Window(window)
                    && role
                        .generations
                        .last()
                        .is_some_and(|generation| generation.phase != Phase::Retired)
            })
        {
            return Err(Error::CloseOrder);
        }
        self.closed.insert(window);
        self.closed_at.insert(window, self.step);
        Ok(())
    }

    fn call_completed(&mut self, role: Role, generation: Generation, id: u64) -> Result<(), Error> {
        let current = self.current(role, generation)?;
        if current.phase != Phase::Ready || !current.revoked.is_empty() {
            return Err(Error::CloseBinding);
        }
        self.calls.insert((role, generation, id, self.step));
        Ok(())
    }

    fn cleanup_failed(&mut self, role: Role) -> Result<(), Error> {
        if self.session != Session::Quiescing || !self.roles.contains_key(&role) {
            return Err(Error::Shutdown);
        }
        self.cleanup_failures.insert(role);
        Ok(())
    }

    fn stop_session(&mut self) -> Result<(), Error> {
        let all_clean = self.roles.values().all(|role| {
            role.generations
                .iter()
                .all(|generation| generation.phase == Phase::Retired && generation.fully_revoked())
        });
        if self.session != Session::Quiescing || !all_clean || !self.cleanup_failures.is_empty() {
            return Err(Error::Shutdown);
        }
        self.session = Session::Stopped;
        Ok(())
    }

    fn ensure_running_owner(&self, role: Role) -> Result<(), Error> {
        if self.session != Session::Running {
            return Err(if self.session == Session::HostDead {
                Error::HostDeath
            } else {
                Error::Shutdown
            });
        }
        if self
            .role_window(role)
            .is_some_and(|window| self.window_unavailable(window))
        {
            return Err(Error::CloseBinding);
        }
        Ok(())
    }

    fn current(&self, role: Role, generation: Generation) -> Result<&GenerationState, Error> {
        let current = self
            .roles
            .get(&role)
            .ok_or(Error::GenerationRotation)?
            .current()?;
        (current.generation == generation)
            .then_some(current)
            .ok_or(Error::StaleGeneration)
    }

    fn current_mut(
        &mut self,
        role: Role,
        generation: Generation,
    ) -> Result<&mut GenerationState, Error> {
        let current = self
            .roles
            .get_mut(&role)
            .ok_or(Error::GenerationRotation)?
            .current_mut()?;
        (current.generation == generation)
            .then_some(current)
            .ok_or(Error::StaleGeneration)
    }

    fn role_window(&self, role: Role) -> Option<Window> {
        match self.roles.get(&role)?.owner {
            Owner::Window(window) => Some(window),
            Owner::App => None,
        }
    }

    fn window_unavailable(&self, window: Window) -> bool {
        self.closing.contains(&window) || self.closed.contains(&window)
    }
}

#[derive(Clone, Copy)]
enum Expectation {
    Restart,
    PreReady,
    WindowClose(&'static [(Role, Generation)]),
    HostDeath,
    Shutdown,
}

fn verify(events: &[Event], expected: Expectation) -> Result<(), Error> {
    let mut model = Model::default();
    for event in events {
        model.apply(*event)?;
    }
    match expected {
        Expectation::Restart => verify_restart(&model),
        Expectation::PreReady => verify_pre_ready(&model),
        Expectation::WindowClose(unaffected) => verify_close(&model, unaffected),
        Expectation::HostDeath => verify_terminal(&model, Session::HostDead, Error::HostDeath),
        Expectation::Shutdown => verify_terminal(&model, Session::Stopped, Error::Shutdown),
    }
}

fn verify_restart(model: &Model) -> Result<(), Error> {
    let role = model.roles.get(&ROLE).ok_or(Error::GenerationRotation)?;
    if role.generations.len() != 2 {
        return Err(Error::GenerationRotation);
    }
    let first = &role.generations[0];
    let second = &role.generations[1];
    if first.exit != Some(ExitCause::Crash)
        || !first.fully_revoked()
        || second.phase != Phase::Ready
    {
        return Err(Error::GenerationRotation);
    }
    if !model.stale.contains(&(ROLE, G1)) {
        return Err(Error::StaleGeneration);
    }
    let beacons = model
        .beacons
        .iter()
        .filter(|beacon| beacon.role == ROLE && beacon.window == WINDOW)
        .collect::<Vec<_>>();
    if beacons.len() != 2 {
        return Err(Error::RendererContinuity);
    }
    let (before, after) = (beacons[0], beacons[1]);
    if before.generation != G1
        || after.generation != G2
        || before.navigation != after.navigation
        || before.nonce != after.nonce
        || after.sequence <= before.sequence
    {
        return Err(Error::RendererContinuity);
    }
    Ok(())
}

fn verify_pre_ready(model: &Model) -> Result<(), Error> {
    let role = model.roles.get(&ROLE).ok_or(Error::GenerationRotation)?;
    if role.generations.len() != 2 {
        return Err(Error::GenerationRotation);
    }
    let first = &role.generations[0];
    let second = &role.generations[1];
    if first.exit != Some(ExitCause::PreReadyFailure)
        || first.seen.contains(&Phase::Ready)
        || first.phase != Phase::Retired
        || !first.fully_revoked()
        || second.phase != Phase::Ready
    {
        return Err(Error::ReadyOrder);
    }
    Ok(())
}

fn verify_close(model: &Model, unaffected: &[(Role, Generation)]) -> Result<(), Error> {
    let closed_at = *model.closed_at.get(&WINDOW).ok_or(Error::CloseBinding)?;
    for role in model
        .roles
        .values()
        .filter(|role| role.owner == Owner::Window(WINDOW))
    {
        let Some(current) = role.generations.last() else {
            continue;
        };
        if current.phase == Phase::Retired
            && current.exit == Some(ExitCause::Crash)
            && current.fully_revoked()
        {
            continue;
        }
        if current.phase != Phase::Retired
            || !current.seen.contains(&Phase::Quiesced)
            || !current.seen.contains(&Phase::Drained)
            || !current.fully_revoked()
        {
            return Err(Error::CloseOrder);
        }
    }
    for (role, generation) in unaffected {
        let current = model.current(*role, *generation)?;
        if current.phase != Phase::Ready
            || !current.revoked.is_empty()
            || !model
                .calls
                .iter()
                .any(|(call_role, call_generation, _, call_step)| {
                    call_role == role && call_generation == generation && *call_step > closed_at
                })
        {
            return Err(Error::CloseBinding);
        }
    }
    Ok(())
}

fn verify_terminal(model: &Model, session: Session, error: Error) -> Result<(), Error> {
    if model.session != session
        || model.roles.values().any(|role| {
            role.generations
                .iter()
                .any(|generation| generation.phase != Phase::Retired || !generation.fully_revoked())
        })
    {
        return Err(error);
    }
    Ok(())
}

fn start(
    events: &mut Vec<Event>,
    role: Role,
    owner: Owner,
    generation: Generation,
    handle: Handle,
) {
    events.extend([
        Event::Declare(role, owner),
        Event::Provision(role, generation),
        Event::Spawn(role, generation, handle, handle.0),
        Event::LinkBound(role, generation),
        Event::Ready(role, generation),
    ]);
}

fn revoke_all(events: &mut Vec<Event>, role: Role, generation: Generation) {
    events.extend(
        Authority::ALL
            .into_iter()
            .map(|authority| Event::Revoke(role, generation, authority)),
    );
}

fn revoke_for_drain(events: &mut Vec<Event>, role: Role, generation: Generation) {
    events.extend(
        Authority::PRE_DRAIN
            .into_iter()
            .map(|authority| Event::Revoke(role, generation, authority)),
    );
}

fn finish_drain(
    events: &mut Vec<Event>,
    role: Role,
    generation: Generation,
    handle: Option<Handle>,
) {
    events.extend([
        Event::Drain(role, generation),
        Event::Revoke(role, generation, Authority::Link),
        Event::Revoke(role, generation, Authority::Locator),
    ]);
    events.push(match handle {
        Some(handle) => Event::Reap(role, generation, handle),
        None => Event::RetireUnspawned(role, generation),
    });
}

fn retired_g1() -> Vec<Event> {
    let mut events = Vec::new();
    start(&mut events, ROLE, Owner::Window(WINDOW), G1, H1);
    events.extend([
        Event::Beacon(ROLE, G1, WINDOW, NAVIGATION, DOCUMENT, 1),
        Event::Crash(ROLE, G1),
        Event::Reap(ROLE, G1, H1),
    ]);
    revoke_all(&mut events, ROLE, G1);
    events
}

fn restart_trace() -> Vec<Event> {
    let mut events = retired_g1();
    events.extend([
        Event::Provision(ROLE, G2),
        Event::RejectStale(ROLE, G1),
        Event::Spawn(ROLE, G2, H2, H2.0),
        Event::LinkBound(ROLE, G2),
        Event::Ready(ROLE, G2),
        Event::Beacon(ROLE, G2, WINDOW, NAVIGATION, DOCUMENT, 2),
    ]);
    events
}

fn window_close_trace() -> Vec<Event> {
    let mut events = Vec::new();
    start(&mut events, ROLE, Owner::Window(WINDOW), G1, H1);
    start(&mut events, APP_ROLE, Owner::App, G1, APP_H1);
    start(
        &mut events,
        OTHER_ROLE,
        Owner::Window(OTHER_WINDOW),
        G1,
        OTHER_H1,
    );
    events.extend([
        Event::WorkAdmitted(ROLE, G1, 1),
        Event::WindowClosing(WINDOW),
        Event::Quiesce(ROLE, G1),
    ]);
    revoke_for_drain(&mut events, ROLE, G1);
    events.push(Event::WorkCompleted(ROLE, G1, 1));
    finish_drain(&mut events, ROLE, G1, Some(H1));
    events.extend([
        Event::WindowClosed(WINDOW),
        Event::CallCompleted(APP_ROLE, G1, 91),
        Event::CallCompleted(OTHER_ROLE, G1, 92),
    ]);
    events
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ClosePoint {
    BeforeProvision,
    Provisioned,
    Spawned,
    LinkBound,
    Ready,
}

fn close_race_trace(point: ClosePoint) -> Vec<Event> {
    let mut events = retired_g1();
    if point >= ClosePoint::Provisioned {
        events.push(Event::Provision(ROLE, G2));
    }
    if point >= ClosePoint::Spawned {
        events.push(Event::Spawn(ROLE, G2, H2, H2.0));
    }
    if point >= ClosePoint::LinkBound {
        events.push(Event::LinkBound(ROLE, G2));
    }
    if point >= ClosePoint::Ready {
        events.push(Event::Ready(ROLE, G2));
    }
    events.push(Event::WindowClosing(WINDOW));
    if point >= ClosePoint::Provisioned {
        events.push(Event::Quiesce(ROLE, G2));
        revoke_for_drain(&mut events, ROLE, G2);
        finish_drain(
            &mut events,
            ROLE,
            G2,
            (point >= ClosePoint::Spawned).then_some(H2),
        );
    }
    events.push(Event::WindowClosed(WINDOW));
    events
}

fn host_death_trace() -> Vec<Event> {
    let mut events = Vec::new();
    start(&mut events, ROLE, Owner::Window(WINDOW), G1, H1);
    start(&mut events, APP_ROLE, Owner::App, G1, APP_H1);
    events.push(Event::HostDied);
    for (role, handle) in [(ROLE, H1), (APP_ROLE, APP_H1)] {
        revoke_all(&mut events, role, G1);
        events.push(Event::Reap(role, G1, handle));
    }
    events
}

fn shutdown_trace() -> Vec<Event> {
    let roles = [
        (ROLE, Owner::Window(WINDOW), H1),
        (APP_ROLE, Owner::App, APP_H1),
        (OTHER_ROLE, Owner::Window(OTHER_WINDOW), OTHER_H1),
    ];
    let mut events = Vec::new();
    for (role, owner, handle) in roles {
        start(&mut events, role, owner, G1, handle);
    }
    events.push(Event::SessionQuiescing);
    for (role, _, handle) in roles {
        events.push(Event::Quiesce(role, G1));
        revoke_for_drain(&mut events, role, G1);
        finish_drain(&mut events, role, G1, Some(handle));
    }
    events.push(Event::SessionStopped);
    events
}

fn assert_error(events: &[Event], expectation: Expectation, error: Error) {
    assert_eq!(verify(events, expectation), Err(error));
}

#[test]
fn restart_rotates_generation_after_full_authority_revocation() {
    verify(&restart_trace(), Expectation::Restart).expect("complete restart trace");

    let mut missing = restart_trace();
    missing.retain(|event| {
        !matches!(
            event,
            Event::Provision(ROLE, G2)
                | Event::Spawn(ROLE, G2, ..)
                | Event::LinkBound(ROLE, G2)
                | Event::Ready(ROLE, G2)
                | Event::Beacon(ROLE, G2, ..)
        )
    });
    assert_error(&missing, Expectation::Restart, Error::GenerationRotation);

    let mut reused = restart_trace();
    *reused
        .iter_mut()
        .find(|event| matches!(event, Event::Provision(ROLE, G2)))
        .expect("successor provision") = Event::Provision(ROLE, G1);
    assert_error(&reused, Expectation::Restart, Error::GenerationRotation);
}

#[test]
fn coordinator_recreation_cannot_reset_role_generation() {
    let mut events = retired_g1();
    events.push(Event::CoordinatorRecreated(ROLE));
    let mut model = Model::default();
    for event in events {
        model.apply(event).expect("retire g1 before recreation");
    }
    assert_eq!(
        model.apply(Event::Provision(ROLE, G1)),
        Err(Error::GenerationRotation)
    );
}

#[test]
fn restart_rejects_successor_before_full_authority_revocation() {
    for missing in Authority::ALL {
        let mut events = restart_trace();
        events.retain(
            |event| !matches!(event, Event::Revoke(ROLE, G1, authority) if *authority == missing),
        );
        assert_error(&events, Expectation::Restart, Error::RevokeBeforeSuccessor);
    }
}

#[test]
fn natural_exit_is_observed_and_reaped_before_revocation() {
    let mut events = restart_trace();
    let reap = events
        .iter()
        .position(|event| matches!(event, Event::Reap(ROLE, G1, _)))
        .expect("natural reap");
    let revoke = events
        .iter()
        .position(|event| matches!(event, Event::Revoke(ROLE, G1, _)))
        .expect("first revoke");
    events.swap(reap, revoke);
    assert_error(&events, Expectation::Restart, Error::NaturalExitOrder);
}

#[test]
fn ready_requires_spawn_authenticated_link_and_live_owner() {
    let cases = [
        vec![
            Event::Declare(ROLE, Owner::Window(WINDOW)),
            Event::Provision(ROLE, G1),
            Event::LinkBound(ROLE, G1),
        ],
        vec![
            Event::Declare(ROLE, Owner::Window(WINDOW)),
            Event::Provision(ROLE, G1),
            Event::Spawn(ROLE, G1, H1, H1.0),
            Event::Ready(ROLE, G1),
        ],
    ];
    for events in cases {
        assert_error(&events, Expectation::Restart, Error::ReadyOrder);
    }
    let events = [
        Event::Declare(ROLE, Owner::Window(WINDOW)),
        Event::Provision(ROLE, G1),
        Event::Spawn(ROLE, G1, H1, H1.0),
        Event::LinkBound(ROLE, G1),
        Event::WindowClosing(WINDOW),
        Event::Ready(ROLE, G1),
    ];
    assert_error(&events, Expectation::WindowClose(&[]), Error::CloseBinding);
}

#[test]
fn post_bind_pre_ready_failure_revokes_before_recovery() {
    let mut events = vec![
        Event::Declare(ROLE, Owner::Window(WINDOW)),
        Event::Provision(ROLE, G1),
        Event::Spawn(ROLE, G1, H1, H1.0),
        Event::LinkBound(ROLE, G1),
        Event::FailBeforeReady(ROLE, G1),
        Event::Reap(ROLE, G1, H1),
    ];
    revoke_all(&mut events, ROLE, G1);
    events.extend([
        Event::Provision(ROLE, G2),
        Event::Spawn(ROLE, G2, H2, H2.0),
        Event::LinkBound(ROLE, G2),
        Event::Ready(ROLE, G2),
    ]);
    verify(&events, Expectation::PreReady).expect("pre-ready recovery");

    let mut failed = Model::default();
    for event in [
        Event::Declare(ROLE, Owner::Window(WINDOW)),
        Event::Provision(ROLE, G1),
        Event::Spawn(ROLE, G1, H1, H1.0),
        Event::LinkBound(ROLE, G1),
        Event::FailBeforeReady(ROLE, G1),
    ] {
        failed.apply(event).expect("pre-ready failure setup");
    }
    assert_eq!(failed.apply(Event::Ready(ROLE, G1)), Err(Error::ReadyOrder));

    for missing in Authority::ALL {
        let mut incomplete = events.clone();
        incomplete.retain(
            |event| !matches!(event, Event::Revoke(ROLE, G1, authority) if *authority == missing),
        );
        assert_error(
            &incomplete,
            Expectation::PreReady,
            Error::RevokeBeforeSuccessor,
        );
    }
}

#[test]
fn admission_failure_revokes_before_reap_and_is_terminal() {
    let mut events = vec![
        Event::Declare(ROLE, Owner::Window(WINDOW)),
        Event::Provision(ROLE, G1),
        Event::Spawn(ROLE, G1, H1, H1.0),
        Event::AdmissionFailed(ROLE, G1),
    ];
    revoke_all(&mut events, ROLE, G1);
    events.push(Event::Reap(ROLE, G1, H1));

    let mut model = Model::default();
    for event in events {
        model.apply(event).expect("admission failure cleanup");
    }
    let retired = model
        .current(ROLE, G1)
        .expect("retired admission generation");
    assert_eq!(retired.phase, Phase::Retired);
    assert!(!retired.seen.contains(&Phase::Ready));
    assert_eq!(
        model.apply(Event::Provision(ROLE, G2)),
        Err(Error::ReadyOrder)
    );

    let mut wrong_order = Model::default();
    for event in [
        Event::Declare(ROLE, Owner::Window(WINDOW)),
        Event::Provision(ROLE, G1),
        Event::Spawn(ROLE, G1, H1, H1.0),
        Event::AdmissionFailed(ROLE, G1),
    ] {
        wrong_order.apply(event).expect("admission failure setup");
    }
    assert_eq!(
        wrong_order.apply(Event::Reap(ROLE, G1, H1)),
        Err(Error::RevokeBeforeSuccessor)
    );
}

#[test]
fn protocol_failure_revokes_before_terminate_and_is_terminal() {
    let mut events = Vec::new();
    start(&mut events, ROLE, Owner::Window(WINDOW), G1, H1);
    events.push(Event::ProtocolFailed(ROLE, G1));
    revoke_all(&mut events, ROLE, G1);
    events.push(Event::Reap(ROLE, G1, H1));

    let mut model = Model::default();
    for event in events {
        model.apply(event).expect("protocol failure cleanup");
    }
    assert_eq!(
        model.apply(Event::Provision(ROLE, G2)),
        Err(Error::ReadyOrder)
    );

    let mut wrong_order = Model::default();
    let mut setup = Vec::new();
    start(&mut setup, ROLE, Owner::Window(WINDOW), G1, H1);
    setup.push(Event::ProtocolFailed(ROLE, G1));
    for event in setup {
        wrong_order.apply(event).expect("protocol failure setup");
    }
    assert_eq!(
        wrong_order.apply(Event::Reap(ROLE, G1, H1)),
        Err(Error::RevokeBeforeSuccessor)
    );
}

#[test]
fn stale_generation_is_rejected_after_rotation() {
    let mut events = restart_trace();
    events.retain(|event| !matches!(event, Event::RejectStale(ROLE, G1)));
    assert_error(&events, Expectation::Restart, Error::StaleGeneration);
}

#[test]
fn closing_one_window_revokes_only_its_bound_role() {
    const UNAFFECTED: &[(Role, Generation)] = &[(APP_ROLE, G1), (OTHER_ROLE, G1)];
    verify(&window_close_trace(), Expectation::WindowClose(UNAFFECTED)).expect("owner-bound close");
    for authority in Authority::ALL {
        let mut events = window_close_trace();
        events.push(Event::Revoke(APP_ROLE, G1, authority));
        assert_error(
            &events,
            Expectation::WindowClose(UNAFFECTED),
            Error::CloseBinding,
        );
    }

    let mut early_calls = window_close_trace();
    early_calls.retain(|event| !matches!(event, Event::CallCompleted(..)));
    let closed = early_calls
        .iter()
        .position(|event| matches!(event, Event::WindowClosed(WINDOW)))
        .expect("window closed");
    early_calls.splice(
        closed..closed,
        [
            Event::CallCompleted(APP_ROLE, G1, 91),
            Event::CallCompleted(OTHER_ROLE, G1, 92),
        ],
    );
    assert_error(
        &early_calls,
        Expectation::WindowClose(UNAFFECTED),
        Error::CloseBinding,
    );
}

#[test]
fn close_tombstone_blocks_successor_provisioning() {
    let mut events = window_close_trace();
    events.push(Event::Provision(ROLE, G2));
    assert_error(&events, Expectation::WindowClose(&[]), Error::CloseBinding);

    let mut backoff = retired_g1();
    backoff.extend([Event::WindowClosing(WINDOW), Event::Provision(ROLE, G2)]);
    assert_error(&backoff, Expectation::WindowClose(&[]), Error::CloseBinding);
}

#[test]
fn close_wins_at_every_successor_boundary() {
    for point in [
        ClosePoint::BeforeProvision,
        ClosePoint::Provisioned,
        ClosePoint::Spawned,
        ClosePoint::LinkBound,
        ClosePoint::Ready,
    ] {
        verify(&close_race_trace(point), Expectation::WindowClose(&[]))
            .unwrap_or_else(|error| panic!("close at {point:?}: {error:?}"));
    }

    let mut spawn_after_close = retired_g1();
    spawn_after_close.extend([
        Event::Provision(ROLE, G2),
        Event::WindowClosing(WINDOW),
        Event::Spawn(ROLE, G2, H2, H2.0),
    ]);
    assert_error(
        &spawn_after_close,
        Expectation::WindowClose(&[]),
        Error::CloseBinding,
    );

    let mut bind_after_close = retired_g1();
    bind_after_close.extend([
        Event::Provision(ROLE, G2),
        Event::Spawn(ROLE, G2, H2, H2.0),
        Event::WindowClosing(WINDOW),
        Event::LinkBound(ROLE, G2),
    ]);
    assert_error(
        &bind_after_close,
        Expectation::WindowClose(&[]),
        Error::CloseBinding,
    );
}

#[test]
fn window_close_revokes_routes_before_drain_and_link_before_reap() {
    let mut before_quiesce = window_close_trace();
    let quiesce = before_quiesce
        .iter()
        .position(|event| matches!(event, Event::Quiesce(ROLE, G1)))
        .expect("quiesce");
    let port = before_quiesce
        .iter()
        .position(|event| matches!(event, Event::Revoke(ROLE, G1, Authority::Port)))
        .expect("port revoke");
    before_quiesce.swap(quiesce, port);
    assert_error(
        &before_quiesce,
        Expectation::WindowClose(&[]),
        Error::CloseOrder,
    );

    let mut before_link = window_close_trace();
    let link = before_link
        .iter()
        .position(|event| matches!(event, Event::Revoke(ROLE, G1, Authority::Link)))
        .expect("link revoke");
    let reap = before_link
        .iter()
        .position(|event| matches!(event, Event::Reap(ROLE, G1, _)))
        .expect("reap");
    before_link.swap(link, reap);
    assert_error(
        &before_link,
        Expectation::WindowClose(&[]),
        Error::CloseOrder,
    );

    let mut closed_early = window_close_trace();
    let closed = closed_early
        .iter()
        .position(|event| matches!(event, Event::WindowClosed(WINDOW)))
        .expect("closed");
    let reap = closed_early
        .iter()
        .position(|event| matches!(event, Event::Reap(ROLE, G1, _)))
        .expect("reap");
    closed_early.swap(closed, reap);
    assert_error(
        &closed_early,
        Expectation::WindowClose(&[]),
        Error::CloseOrder,
    );
}

#[test]
fn drain_completes_admitted_work_and_rejects_new_work_after_quiesce() {
    let mut unfinished = window_close_trace();
    unfinished.retain(|event| !matches!(event, Event::WorkCompleted(ROLE, G1, 1)));
    assert_error(
        &unfinished,
        Expectation::WindowClose(&[]),
        Error::CloseOrder,
    );

    let mut late = window_close_trace();
    let quiesce = late
        .iter()
        .position(|event| matches!(event, Event::Quiesce(ROLE, G1)))
        .expect("quiesce");
    late.insert(quiesce + 1, Event::WorkAdmitted(ROLE, G1, 2));
    assert_error(&late, Expectation::WindowClose(&[]), Error::CloseOrder);
}

#[test]
fn continuity_requires_same_document_nonce_and_post_restart_beacon() {
    for mutation in ["nonce", "navigation", "window", "generation", "sequence"] {
        let mut events = restart_trace();
        let beacon = events
            .iter_mut()
            .find(|event| matches!(event, Event::Beacon(ROLE, G2, ..)))
            .expect("g2 beacon");
        *beacon = match mutation {
            "nonce" => Event::Beacon(ROLE, G2, WINDOW, NAVIGATION, Nonce("reloaded"), 2),
            "navigation" => Event::Beacon(ROLE, G2, WINDOW, Navigation(12), DOCUMENT, 2),
            "window" => Event::Beacon(ROLE, G2, OTHER_WINDOW, NAVIGATION, DOCUMENT, 2),
            "generation" => Event::Beacon(ROLE, G1, WINDOW, NAVIGATION, DOCUMENT, 2),
            "sequence" => Event::Beacon(ROLE, G2, WINDOW, NAVIGATION, DOCUMENT, 1),
            _ => unreachable!(),
        };
        assert_error(&events, Expectation::Restart, Error::RendererContinuity);
    }

    let mut missing = restart_trace();
    missing.retain(|event| !matches!(event, Event::Beacon(ROLE, G2, ..)));
    assert_error(&missing, Expectation::Restart, Error::RendererContinuity);

    let mut early = restart_trace();
    let ready = early
        .iter()
        .position(|event| matches!(event, Event::Ready(ROLE, G2)))
        .expect("ready");
    let beacon = early
        .iter()
        .position(|event| matches!(event, Event::Beacon(ROLE, G2, ..)))
        .expect("beacon");
    early.swap(ready, beacon);
    assert_error(&early, Expectation::Restart, Error::RendererContinuity);
}

#[test]
fn host_death_requires_full_revocation_and_handle_bound_reap() {
    verify(&host_death_trace(), Expectation::HostDeath).expect("host-death trace");

    let mut pid_only = host_death_trace();
    let reap = pid_only
        .iter_mut()
        .find(|event| matches!(event, Event::Reap(ROLE, G1, _)))
        .expect("reap");
    *reap = Event::ReapByPid(ROLE, G1, H1.0);
    assert_error(&pid_only, Expectation::HostDeath, Error::HandleReap);

    for missing in Authority::ALL {
        let mut events = host_death_trace();
        events.retain(|event| !matches!(event, Event::Revoke(APP_ROLE, G1, authority) if *authority == missing));
        assert_error(&events, Expectation::HostDeath, Error::HostDeath);
    }

    let mut duplicate = host_death_trace();
    duplicate.push(Event::Reap(ROLE, G1, H1));
    assert_error(&duplicate, Expectation::HostDeath, Error::HandleReap);

    let mut observed_last = host_death_trace();
    observed_last.retain(|event| !matches!(event, Event::HostDied));
    observed_last.push(Event::HostDied);
    assert!(verify(&observed_last, Expectation::HostDeath).is_err());

    let mut successor = host_death_trace();
    successor.push(Event::Provision(ROLE, G2));
    assert_error(&successor, Expectation::HostDeath, Error::HostDeath);
}

#[test]
fn application_shutdown_revokes_and_reaps_every_role_before_stop() {
    verify(&shutdown_trace(), Expectation::Shutdown).expect("shutdown trace");

    let mut leaked = shutdown_trace();
    leaked.retain(|event| !matches!(event, Event::Reap(APP_ROLE, G1, _)));
    assert_error(&leaked, Expectation::Shutdown, Error::Shutdown);

    let mut successor = retired_g1();
    successor.extend([Event::SessionQuiescing, Event::Provision(ROLE, G2)]);
    assert_error(&successor, Expectation::Shutdown, Error::Shutdown);

    assert_eq!(
        Model::default().apply(Event::SessionStopped),
        Err(Error::Shutdown)
    );
}

#[test]
fn application_shutdown_continues_after_cleanup_failure() {
    let mut events = shutdown_trace();
    let quiescing = events
        .iter()
        .position(|event| matches!(event, Event::SessionQuiescing))
        .expect("session quiescing");
    events.insert(quiescing + 1, Event::CleanupFailed(ROLE));

    let mut model = Model::default();
    for event in events {
        if event == Event::SessionStopped {
            assert_eq!(model.apply(event), Err(Error::Shutdown));
        } else {
            model.apply(event).expect("continue cleanup after failure");
        }
    }
    for role in [APP_ROLE, OTHER_ROLE] {
        let current = model.current(role, G1).expect("later role cleaned");
        assert_eq!(current.phase, Phase::Retired);
        assert!(current.fully_revoked());
    }
}

#[test]
fn hostile_shutdown_contract_child() {
    let Some(role) = env::var_os(CHILD_ROLE_ENV) else {
        return;
    };
    match role.to_str().expect("UTF-8 child role") {
        "hostile" => {
            println!("READY opaque_handle={}", H1.0);
            std::io::stdout().flush().expect("flush readiness");
            std::io::stdin()
                .read_to_end(&mut Vec::new())
                .expect("observe stdin EOF");
            println!("IGNORED_EOF opaque_handle={}", H1.0);
            std::io::stdout().flush().expect("flush EOF marker");
            thread::park();
        }
        "next" => println!("NEXT_READY opaque_handle={}", H2.0),
        other => panic!("unknown contract child role: {other}"),
    }
}

#[test]
fn hostile_shutdown_is_subprocess_isolated_and_next_cycle_succeeds() {
    let executable = env::current_exe().expect("contract test executable");
    let mut hostile = Command::new(&executable)
        .args(["--exact", CHILD_TEST, "--nocapture"])
        .env(CHILD_ROLE_ENV, "hostile")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn hostile child");
    let (markers, reader) = marker_reader(hostile.stdout.take().expect("hostile stdout"));
    await_marker(&mut hostile, &markers, "READY opaque_handle=101");
    drop(hostile.stdin.take().expect("hostile stdin"));
    await_marker(&mut hostile, &markers, "IGNORED_EOF opaque_handle=101");
    hostile.kill().expect("kill uncooperative child");
    let status = wait_child_before(&mut hostile);
    reader.join().expect("hostile reader joins");
    assert!(!status.success(), "hostile child exited cleanly");

    let mut next = Command::new(executable)
        .args(["--exact", CHILD_TEST, "--nocapture"])
        .env(CHILD_ROLE_ENV, "next")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn next child");
    let (markers, reader) = marker_reader(next.stdout.take().expect("next stdout"));
    await_marker(&mut next, &markers, "NEXT_READY opaque_handle=102");
    let status = wait_child_before(&mut next);
    reader.join().expect("next reader joins");
    assert!(status.success(), "next cycle failed: {status:?}");
}

fn marker_reader(stdout: ChildStdout) -> (Receiver<String>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (rx, reader)
}

fn await_marker(child: &mut Child, markers: &Receiver<String>, expected: &str) {
    let deadline = Instant::now() + SUBPROCESS_DEADLINE;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match markers.recv_timeout(remaining) {
            Ok(line) if line.contains(expected) => return,
            Ok(_) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("child missed marker {expected}: {error}");
            }
        }
    }
}

fn wait_child_before(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + SUBPROCESS_DEADLINE;
    loop {
        match child.try_wait().expect("poll child") {
            Some(status) => return status,
            None if Instant::now() < deadline => thread::yield_now(),
            None => {
                let _ = child.kill();
                let status = child.wait().expect("reap timed-out child");
                panic!("child exceeded kill-switch deadline: {status:?}");
            }
        }
    }
}
