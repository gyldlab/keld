//! Single-flight SDK test admission and native physical-operation custody.
//! Scenario assertions and the real HTTP peer are independent responsibility owners.
#[path = "singleflight/peer.rs"]
mod peer;
#[path = "singleflight/scenario.rs"]
mod scenario;

use super::super::{
    AuthBroker, AuthRequest, CallError, HandleState, HttpOperation, Job, OperationId, Principal,
    Progress, ReadyResponse, ResponseEvent,
};
use super::{broker, granted_manifest};
use crate::auth::macos_http;
use peer::SingleflightPeer;
use std::time::Instant;

pub(super) use scenario::run_sdk_case;

// The fixture owns only a returned generic admission witness; Core lease/fanout
// is not executed here. Its lifetime follows the real Native Drained boundary.
struct SingleflightAdmission(std::rc::Rc<std::cell::Cell<usize>>);
impl Drop for SingleflightAdmission {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
struct SingleflightAdmissions {
    count: std::cell::Cell<usize>,
    drops: std::rc::Rc<std::cell::Cell<usize>>,
}
impl SingleflightAdmissions {
    fn new() -> Self {
        Self {
            count: std::cell::Cell::new(0),
            drops: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }
}

fn singleflight_request(
    owner: &mut AuthBroker,
    account: &str,
    scopes: Vec<String>,
    admissions: &SingleflightAdmissions,
) -> HandleState<SingleflightAdmission> {
    owner.handle_manifest_request(
        &granted_manifest(),
        Principal::AppProcess,
        AuthRequest::Token {
            account_id: account.to_owned(),
            scopes: Some(scopes),
        },
        || {
            admissions.count.set(admissions.count.get() + 1);
            Some(SingleflightAdmission(std::rc::Rc::clone(&admissions.drops)))
        },
    )
}

struct SingleflightOwnedOperation {
    operation: OperationId,
    leases: Vec<SingleflightAdmission>,
    custody: Option<macos_http::TestHttpOwner>,
    drained: bool,
}
enum SingleflightRequest {
    Pending {
        operation: OperationId,
        admitted: bool,
    },
    Ready(ReadyResponse),
    Other,
}

// One fixed test owns all resources before its first admission/poll assertion.
// This is not a broker Drop or a general cleanup framework.
struct SingleflightCase {
    owner: Option<AuthBroker>,
    peer: Option<SingleflightPeer>,
    admissions: SingleflightAdmissions,
    operations: Vec<SingleflightOwnedOperation>,
    events: Vec<ResponseEvent>,
    observation_fault: Option<&'static str>,
    deadline: Instant,
    cleaned: bool,
}
impl SingleflightCase {
    fn start() -> Self {
        let deadline = Instant::now() + std::time::Duration::from_secs(8);
        let mut owner = broker();
        let peer = SingleflightPeer::start(deadline);
        owner.test_token_endpoint = Some(peer.endpoint.clone());
        Self {
            owner: Some(owner),
            peer: Some(peer),
            admissions: SingleflightAdmissions::new(),
            operations: Vec::new(),
            events: Vec::new(),
            observation_fault: None,
            deadline,
            cleaned: false,
        }
    }

    fn request(&mut self, account: &str, scopes: Vec<String>) -> SingleflightRequest {
        let state = singleflight_request(
            self.owner.as_mut().expect("owned broker"),
            account,
            scopes,
            &self.admissions,
        );
        match state {
            HandleState::Pending {
                operation,
                admission,
            } => {
                let admitted = admission.is_some();
                let index = if let Some(index) = self
                    .operations
                    .iter()
                    .position(|owned| owned.operation == operation)
                {
                    index
                } else {
                    self.operations.push(SingleflightOwnedOperation {
                        operation: operation.clone(),
                        leases: Vec::new(),
                        custody: None,
                        drained: false,
                    });
                    self.operations.len() - 1
                };
                if let Some(lease) = admission {
                    self.operations[index].leases.push(lease);
                }
                SingleflightRequest::Pending {
                    operation,
                    admitted,
                }
            }
            HandleState::Ready(response) => SingleflightRequest::Ready(response),
            _ => SingleflightRequest::Other,
        }
    }

    fn poll(&mut self, operation: &OperationId) -> Progress {
        let events = &mut self.events;
        let fault = &mut self.observation_fault;
        let owner = self.owner.as_mut().expect("owned broker");
        let progress = owner.poll(operation, |event| {
            if events.len() < 4 {
                events.push(event);
            } else {
                *fault = Some("unexpected extra canonical events");
            }
        });
        if let Some(owned) = self
            .operations
            .iter_mut()
            .find(|owned| owned.operation == *operation)
        {
            owned.drained = progress == Progress::Drained;
            if owned.custody.is_none() {
                owned.custody = owner.jobs.iter().find_map(|(serial, job)| match job {
                    Some(Job::Token(job)) if *serial == operation.serial => {
                        job.http.as_ref().map(HttpOperation::test_owner)
                    }
                    _ => None,
                });
            }
        }
        if matches!(progress, Progress::HostFault(_)) {
            self.observation_fault = Some("host cleanup fault observed");
        }
        progress
    }

    fn release_drained_leases(&mut self) -> Result<(), &'static str> {
        if self.operations.iter().any(|owned| {
            !owned.drained
                || owned
                    .custody
                    .as_ref()
                    .is_some_and(|custody| !custody.released())
        }) {
            return Err("physical operation custody not drained");
        }
        if !self
            .owner
            .as_ref()
            .ok_or("broker owner missing")?
            .jobs
            .is_empty()
        {
            return Err("broker still owns native jobs");
        }
        for owned in &mut self.operations {
            owned.leases.clear();
        }
        Ok(())
    }

    fn cleanup(&mut self, abort: bool) -> Result<usize, &'static str> {
        if self.cleaned {
            return Ok(0);
        }
        if let Some(peer) = &self.peer {
            peer.release(abort);
        }
        self.owner
            .as_mut()
            .ok_or("broker owner missing")?
            .stop_for_host_tail(|_| {});
        let ids: Vec<_> = self
            .operations
            .iter()
            .map(|owned| owned.operation.clone())
            .collect();
        for id in &ids {
            self.owner
                .as_mut()
                .ok_or("broker owner missing")?
                .cancel(id, CallError::generation_retired());
        }
        while self.operations.iter().any(|owned| !owned.drained) {
            if Instant::now() >= self.deadline {
                return Err("native drain unproved by same clock");
            }
            for id in &ids {
                if !self
                    .operations
                    .iter()
                    .any(|owned| owned.operation == *id && owned.drained)
                {
                    let _ = self.poll(id);
                }
            }
            std::thread::yield_now();
        }
        self.release_drained_leases()?;
        let count = self
            .peer
            .as_mut()
            .ok_or("peer owner missing")?
            .join(self.deadline)?;
        if Instant::now() >= self.deadline {
            return Err("cleanup completed after same clock");
        }
        self.peer.take();
        self.owner.take();
        self.cleaned = true;
        println!("KELD_AUTH_SINGLEFLIGHT_CLEANUP_PROVED native_drained=true peer_joined=true");
        Ok(count)
    }
}
impl Drop for SingleflightCase {
    fn drop(&mut self) {
        if !self.cleaned
            && let Err(reason) = self.cleanup(true)
        {
            eprintln!("KELD_AUTH_SINGLEFLIGHT_UNPROVEN_TAIL {reason}");
            // The already-isolated child must FAIL. Retain any unproved SDK
            // or peer owner until process exit/backstop instead of calling an
            // emergency HttpOperation Drop a proved normal drain.
            if let Some(owner) = self.owner.take() {
                std::mem::forget(owner);
            }
            if let Some(peer) = self.peer.take() {
                std::mem::forget(peer);
            }
            // Returned admission witnesses remain retained with unproved
            // native owners until the bounded child's exit/backstop.
            std::mem::forget(std::mem::take(&mut self.operations));
            // The original assertion/unwind is already a failure. On a normal
            // unproved Drop, also fail rather than return the child PASS marker.
            assert!(std::thread::panicking(), "singleflight tail unproved");
        }
    }
}
