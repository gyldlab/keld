//! Private state owned by ONE existing native auth-worker command loop.
//! No shared registry, waiter queue, transport, lock, wait, or custom Drop.
//! Core retains permission, generation, correlation, admission and deadlines.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{AuthResponse, ProviderProfile, provider_profile};
use keld_ipc::CallError;

/// Internal host fault; it must not be a pre-Allow account-existence reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StateFault {
    Stopping,
    CounterExhausted,
    UnknownProfile,
    InvalidReceiptClock,
    ExpiryOverflow,
    EmptyCredential,
    ExpiredAccess,
    MissingRefreshCredential,
}

impl StateFault {
    /// Exact reviewed private producer mapping, recorded with the primitive and
    /// platform-local union in the same PR. Source review does not establish
    /// every condition's runtime reachability.
    pub(super) const fn code(self) -> &'static str {
        match self {
            // Only an unusable credential returned by the actual exchange.
            Self::EmptyCredential => "KELD-AUTH-013",
            // Missing refresh custody is local host state, not an observed
            // provider refusal, invalid_grant or interaction.
            Self::Stopping
            | Self::CounterExhausted
            | Self::UnknownProfile
            | Self::InvalidReceiptClock
            | Self::ExpiryOverflow
            | Self::ExpiredAccess
            | Self::MissingRefreshCredential => "KELD-AUTH-016",
        }
    }
    fn wire_error(self) -> Arc<CallError> {
        Arc::new(CallError {
            code: self.code().to_owned(),
            message: self.to_string(),
        })
    }
}

impl fmt::Display for StateFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (condition, fix) = match self {
            Self::Stopping => (
                "the host is ending this auth session",
                "Wait for shutdown to finish, restart the host, then start a fresh user-initiated auth.begin",
            ),
            Self::CounterExhausted => (
                "host token identity space is exhausted",
                "Restart the host to restore local identity capacity before starting a fresh user-initiated auth.begin",
            ),
            Self::UnknownProfile => (
                "the completed flow has no compiled-in profile",
                "Correct host completed-flow/profile binding before starting a fresh user-initiated auth.begin",
            ),
            Self::InvalidReceiptClock => (
                "token receipt predates the UNIX epoch",
                "Restore a valid host receipt clock before starting a fresh user-initiated auth.begin",
            ),
            Self::ExpiryOverflow => (
                "token expiry cannot be represented",
                "Correct token expiry input or the host receipt clock before starting a fresh user-initiated auth.begin",
            ),
            Self::EmptyCredential => (
                "the exchange returned an empty access credential or token type",
                "Resolve the token exchange response failure before starting a fresh user-initiated auth.begin",
            ),
            Self::ExpiredAccess => (
                "the returned access token is no longer fresh",
                "Request a fresh token explicitly",
            ),
            Self::MissingRefreshCredential => (
                "the host has no refresh credential",
                "Start a fresh user-initiated auth.begin if interaction is still needed",
            ),
        };
        write!(
            f,
            "{}: {condition}. {fix}; do not automatically retry a consumed exchange.",
            self.code()
        )
    }
}
impl std::error::Error for StateFault {}

/// Equality only: it never rewrites the leader's outbound OAuth scope vector.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ScopeKey(Vec<String>);
impl ScopeKey {
    fn new(scopes: Option<&[String]>) -> Self {
        let mut values = scopes.unwrap_or(&[]).to_vec();
        values.sort();
        values.dedup();
        Self(values)
    }
}
impl fmt::Debug for ScopeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScopeKey").finish_non_exhaustive()
    }
}

/// Host-memory custody only; no serialization and no secure-erasure claim.
#[derive(Clone)]
struct Secret(Arc<str>);
impl Secret {
    fn new(value: String) -> Self {
        Self(Arc::from(value))
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secret").finish_non_exhaustive()
    }
}

/// The public clock is UTC; freshness is an independent monotonic fact.
pub(super) struct AccessToken {
    access: Secret,
    token_type: String,
    expires_at: u64,
    fresh_until: Instant,
}
impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessToken").finish_non_exhaustive()
    }
}
impl AccessToken {
    /// Receipt clocks are captured together by the actual exchange owner.
    pub(super) fn at_receipt(
        access_token: String,
        token_type: String,
        expires_in: u64,
        wall: SystemTime,
        monotonic: Instant,
    ) -> Result<Arc<Self>, StateFault> {
        if access_token.is_empty() || token_type.is_empty() {
            return Err(StateFault::EmptyCredential);
        }
        let receipt = wall
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StateFault::InvalidReceiptClock)?
            .as_secs();
        let expires_at = receipt
            .checked_add(expires_in)
            .ok_or(StateFault::ExpiryOverflow)?;
        let fresh_until = monotonic
            .checked_add(Duration::from_secs(expires_in))
            .ok_or(StateFault::ExpiryOverflow)?;
        Ok(Arc::new(Self {
            access: Secret::new(access_token),
            token_type,
            expires_at,
            fresh_until,
        }))
    }
    /// Invoke at final reply construction, after Core caller-liveness checks.
    /// Scheduling delay never extends the receipt's monotonic token lifetime.
    pub(super) fn response(&self) -> Result<AuthResponse, StateFault> {
        self.response_at(Instant::now())
    }
    fn response_at(&self, now: Instant) -> Result<AuthResponse, StateFault> {
        if !self.fresh_at(now) {
            return Err(StateFault::ExpiredAccess);
        }
        Ok(AuthResponse::Token {
            access_token: self.access.0.to_string(),
            expires_at: self.expires_at,
            token_type: self.token_type.clone(),
        })
    }
    fn fresh_at(&self, now: Instant) -> bool {
        now < self.fresh_until
    }
}

pub(super) type TokenOutcome = Result<Arc<AccessToken>, Arc<CallError>>;
pub(super) type SharedOutcome = Arc<TokenOutcome>;

/// Opaque native-operation identity, not a principal, generation or wire value.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct FlightId {
    account_id: String,
    serial: u64,
}
impl fmt::Debug for FlightId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlightId").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    AwaitingRefresh,
    Active,
}
struct Flight {
    serial: u64,
    requested_scopes: Option<Vec<String>>,
    phase: Phase,
}
struct Account {
    profile: &'static ProviderProfile,
    refresh: Option<Secret>,
    // One shared successful cache result per account, not a per-scope archive.
    cached: Option<(ScopeKey, SharedOutcome)>,
    flights: BTreeMap<ScopeKey, Flight>,
    refresh_owner: Option<u64>,
}

/// Construct inside the native worker and invoke only from its command loop.
/// The worker separately retains the admitted physical-operation lease and
/// actual `HttpOperation`. Registry destruction does not acquire or defer work.
pub(super) struct SessionRegistry {
    accounts: BTreeMap<String, Account>,
    next_account: u64,
    next_flight: u64,
    stopping: bool,
}
impl fmt::Debug for SessionRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionRegistry").finish_non_exhaustive()
    }
}

/// `InFlight` subscribes a caller in the broker's EXISTING correlation owner.
/// Only Leader returns a real admitted lease, to be retained by that owner.
pub(super) enum TokenPlan<A> {
    Unresolved,
    AtCapacity,
    Ready(SharedOutcome),
    InFlight(FlightId),
    Leader { flight: FlightId, admission: A },
}
impl<A> fmt::Debug for TokenPlan<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Unresolved => "Unresolved",
            Self::AtCapacity => "AtCapacity",
            Self::Ready(_) => "Ready",
            Self::InFlight(_) => "InFlight",
            Self::Leader { .. } => "Leader",
        };
        f.debug_struct(name).finish_non_exhaustive()
    }
}

/// One canonical outcome for synchronous fan-out to existing correlations.
/// There is no per-follower queue or second result owner in the registry.
pub(super) struct Completion {
    pub(super) flight: FlightId,
    pub(super) result: SharedOutcome,
}
impl fmt::Debug for Completion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Completion").finish_non_exhaustive()
    }
}

/// Start is an owner-thread observation, never a wait or automatic scheduler.
pub(super) enum StartState {
    WaitingForRefresh,
    Started(ExchangeInput),
    AlreadyStarted,
    Completed(Completion),
    Retired,
}
impl fmt::Debug for StartState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::WaitingForRefresh => "WaitingForRefresh",
            Self::Started(_) => "Started",
            Self::AlreadyStarted => "AlreadyStarted",
            Self::Completed(_) => "Completed",
            Self::Retired => "Retired",
        };
        f.debug_struct(name).finish_non_exhaustive()
    }
}

/// Host-only refresh request input. The worker retains logical flight custody
/// and its admitted lease while its SEPARATE actual `HttpOperation` is active.
pub(super) struct ExchangeInput {
    profile: &'static ProviderProfile,
    refresh: Secret,
    requested_scopes: Option<Vec<String>>,
}
impl ExchangeInput {
    pub(super) fn profile(&self) -> &'static ProviderProfile {
        self.profile
    }
    pub(super) fn refresh_token(&self) -> &str {
        &self.refresh.0
    }
    pub(super) fn requested_scopes(&self) -> Option<&[String]> {
        self.requested_scopes.as_deref()
    }
}
impl fmt::Debug for ExchangeInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExchangeInput").finish_non_exhaustive()
    }
}

/// Classification comes from the broker's actual parsed provider error.
pub(super) enum ExchangeFailure {
    Failed(CallError),
    InvalidGrant(CallError),
}

// One shared identity predicate for all owner operations. The scope key is
// found only among this account's admitted flights; no parallel index/policy.
fn current_key(account: &Account, serial: u64) -> Option<ScopeKey> {
    account
        .flights
        .iter()
        .find_map(|(key, value)| (value.serial == serial).then(|| key.clone()))
}
fn remove_flight(account: &mut Account, key: &ScopeKey, serial: u64) {
    account.flights.remove(key);
    if account.refresh_owner == Some(serial) {
        account.refresh_owner = None;
    }
}

impl SessionRegistry {
    pub(super) fn new() -> Self {
        Self {
            accounts: BTreeMap::new(),
            next_account: 0,
            next_flight: 0,
            stopping: false,
        }
    }
    /// Only after a real successful Begin, with entropy from the actual host
    /// OS owner. Profile identity is immutable and resolved from the closed table.
    pub(super) fn register_completed_begin(
        &mut self,
        profile_key: &str,
        entropy: [u8; 32],
        refresh_token: Option<String>,
        requested_scopes: Option<&[String]>,
        initial_access: Option<Arc<AccessToken>>,
        granted_scopes: Vec<String>,
    ) -> Result<AuthResponse, StateFault> {
        if self.stopping {
            return Err(StateFault::Stopping);
        }
        let profile = provider_profile(profile_key).ok_or(StateFault::UnknownProfile)?;
        self.next_account = self
            .next_account
            .checked_add(1)
            .ok_or(StateFault::CounterExhausted)?;
        let mut account_id = String::with_capacity(81);
        // One immutable opaque host selector; no source-principal policy.
        for byte in entropy {
            write!(account_id, "{byte:02x}").map_err(|_| StateFault::CounterExhausted)?;
        }
        write!(account_id, "-{:016x}", self.next_account)
            .map_err(|_| StateFault::CounterExhausted)?;
        let cached = initial_access
            .filter(|token| token.fresh_at(Instant::now()))
            .map(|token| (ScopeKey::new(requested_scopes), Arc::new(Ok(token))));
        self.accounts.insert(
            account_id.clone(),
            Account {
                profile,
                refresh: refresh_token.map(Secret::new),
                cached,
                flights: BTreeMap::new(),
                refresh_owner: None,
            },
        );
        Ok(AuthResponse::Begin {
            account_id,
            granted_scopes,
        })
    }
    /// Pure issuer resolution, NOT authentication/authorization or a child reply.
    pub(super) fn admission_open(&self) -> bool {
        !self.stopping
    }
    pub(super) fn close_admission(&mut self) {
        self.stopping = true;
    }
    pub(super) fn profile_for(&self, account_id: &str) -> Option<&'static ProviderProfile> {
        self.accounts.get(account_id).map(|account| account.profile)
    }
    pub(super) fn issuer_for(&self, account_id: &str) -> Option<&'static str> {
        self.profile_for(account_id).map(|profile| profile.issuer)
    }
    /// POST-Allow only. The actual existing bounded executor supplies its lease.
    /// Single owner removes the admission race; callbacks cannot reenter &mut self.
    pub(super) fn token_request<A>(
        &mut self,
        account_id: &str,
        scopes: Option<Vec<String>>,
        admit: impl FnOnce() -> Option<A>,
    ) -> Result<TokenPlan<A>, StateFault> {
        self.token_request_at(account_id, scopes, admit, Instant::now())
    }
    fn token_request_at<A>(
        &mut self,
        account_id: &str,
        scopes: Option<Vec<String>>,
        admit: impl FnOnce() -> Option<A>,
        now: Instant,
    ) -> Result<TokenPlan<A>, StateFault> {
        let Some(account) = self.accounts.get_mut(account_id) else {
            return Ok(TokenPlan::Unresolved);
        };
        if self.stopping {
            return Err(StateFault::Stopping);
        }
        let key = ScopeKey::new(scopes.as_deref());
        if let Some((cached_key, result)) = account.cached.as_ref()
            && cached_key == &key
            && result
                .as_ref()
                .as_ref()
                .is_ok_and(|token| token.fresh_at(now))
        {
            return Ok(TokenPlan::Ready(Arc::clone(result)));
        }
        if let Some(flight) = account.flights.get(&key) {
            return Ok(TokenPlan::InFlight(FlightId {
                account_id: account_id.to_owned(),
                serial: flight.serial,
            }));
        }
        // Exhaustion is checked BEFORE reserving any physical-operation lease.
        let serial = self
            .next_flight
            .checked_add(1)
            .ok_or(StateFault::CounterExhausted)?;
        let Some(admission) = admit() else {
            return Ok(TokenPlan::AtCapacity);
        };
        self.next_flight = serial;
        account.flights.insert(
            key,
            Flight {
                serial,
                requested_scopes: scopes,
                phase: Phase::AwaitingRefresh,
            },
        );
        Ok(TokenPlan::Leader {
            flight: FlightId {
                account_id: account_id.to_owned(),
                serial,
            },
            admission,
        })
    }
    /// Explicit event-loop progress. `WaitingForRefresh` means process another
    /// existing command/deadline/completion; never block or retry the exchange.
    pub(super) fn try_start(&mut self, id: &FlightId) -> StartState {
        if self.stopping {
            return StartState::Retired;
        }
        let Some(account) = self.accounts.get_mut(&id.account_id) else {
            return StartState::Retired;
        };
        let Some(key) = current_key(account, id.serial) else {
            return StartState::Retired;
        };
        let Some(flight) = account.flights.get_mut(&key) else {
            return StartState::Retired;
        };
        if flight.phase == Phase::Active {
            return StartState::AlreadyStarted;
        }
        if account.refresh_owner.is_some() {
            return StartState::WaitingForRefresh;
        }
        let Some(refresh) = account.refresh.clone() else {
            remove_flight(account, &key, id.serial);
            return StartState::Completed(Completion {
                flight: id.clone(),
                result: Arc::new(Err(StateFault::MissingRefreshCredential.wire_error())),
            });
        };
        account.refresh_owner = Some(id.serial);
        flight.phase = Phase::Active;
        StartState::Started(ExchangeInput {
            profile: account.profile,
            refresh,
            requested_scopes: flight.requested_scopes.clone(),
        })
    }
    /// Call only from the actual terminal `HttpOperation` event. The worker must
    /// prove transport quiescence BEFORE dropping its separately-held lease.
    pub(super) fn finish_success(
        &mut self,
        id: &FlightId,
        token: Arc<AccessToken>,
        rotated_refresh: Option<String>,
    ) -> Option<Completion> {
        self.finish_success_at(id, token, rotated_refresh, Instant::now())
    }
    fn finish_success_at(
        &mut self,
        id: &FlightId,
        token: Arc<AccessToken>,
        rotated_refresh: Option<String>,
        now: Instant,
    ) -> Option<Completion> {
        if self.stopping {
            return None;
        }
        let account = self.accounts.get_mut(&id.account_id)?;
        let key = current_key(account, id.serial)?;
        if account.refresh_owner != Some(id.serial)
            || account.flights.get(&key)?.phase != Phase::Active
        {
            return None;
        }
        if let Some(refresh) = rotated_refresh {
            account.refresh = Some(Secret::new(refresh));
        }
        let result = if token.fresh_at(now) {
            Arc::new(Ok(token))
        } else {
            Arc::new(Err(StateFault::ExpiredAccess.wire_error()))
        };
        account.cached = result
            .as_ref()
            .as_ref()
            .ok()
            .map(|_| (key.clone(), Arc::clone(&result)));
        remove_flight(account, &key, id.serial);
        Some(Completion {
            flight: id.clone(),
            result,
        })
    }
    /// Only the CURRENT active owner can produce terminal `invalid_grant`. Late
    /// callback identities are retired, including after tail/rotation/eviction.
    /// emit synchronously hands each same-Arc completion to the owner; no queue.
    pub(super) fn finish_failure(
        &mut self,
        id: &FlightId,
        failure: ExchangeFailure,
        mut emit: impl FnMut(Completion),
    ) -> bool {
        if self.stopping {
            return false;
        }
        let Some(account) = self.accounts.get(&id.account_id) else {
            return false;
        };
        let Some(key) = current_key(account, id.serial) else {
            return false;
        };
        if account.refresh_owner != Some(id.serial)
            || !account
                .flights
                .get(&key)
                .is_some_and(|flight| flight.phase == Phase::Active)
        {
            return false;
        }
        match failure {
            ExchangeFailure::InvalidGrant(error) => {
                let result = Arc::new(Err(Arc::new(error)));
                if let Some(account) = self.accounts.remove(&id.account_id) {
                    for flight in account.flights.into_values() {
                        emit(Completion {
                            flight: FlightId {
                                account_id: id.account_id.clone(),
                                serial: flight.serial,
                            },
                            result: Arc::clone(&result),
                        });
                    }
                }
            }
            ExchangeFailure::Failed(error) => {
                if let Some(account) = self.accounts.get_mut(&id.account_id) {
                    remove_flight(account, &key, id.serial);
                }
                emit(Completion {
                    flight: id.clone(),
                    result: Arc::new(Err(Arc::new(error))),
                });
            }
        }
        true
    }
    /// Explicit producer retirement, not a local follower timeout/cancel.
    /// For Active work the owner MUST first quiesce its actual `HttpOperation`.
    /// Pending custody can be retired synchronously without an operation.
    pub(super) fn abandon(&mut self, id: &FlightId, error: CallError) -> Option<Completion> {
        let account = self.accounts.get_mut(&id.account_id)?;
        let key = current_key(account, id.serial)?;
        remove_flight(account, &key, id.serial);
        Some(Completion {
            flight: id.clone(),
            result: Arc::new(Err(Arc::new(error))),
        })
    }
    /// Accepted host tail closes work and synchronously completes all logical
    /// flights. The worker still owns live `HttpOperations`/leases until quiescent.
    /// Account ids remain live during the tail; ordinary owner Drop ends them.
    pub(super) fn stop_for_host_tail(&mut self, mut emit: impl FnMut(Completion)) {
        self.stopping = true;
        let stopped = Arc::new(Err(StateFault::Stopping.wire_error()));
        for (account_id, account) in &mut self.accounts {
            for flight in std::mem::take(&mut account.flights).into_values() {
                emit(Completion {
                    flight: FlightId {
                        account_id: account_id.clone(),
                        serial: flight.serial,
                    },
                    result: Arc::clone(&stopped),
                });
            }
            account.refresh_owner = None;
        }
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
