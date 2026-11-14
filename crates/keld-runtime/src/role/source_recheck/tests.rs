//! Actual `RolePreparer` and bootstrap provisioning, without executing a child.
#![allow(clippy::expect_used, clippy::panic)]
use super::RoleSourceRecheck;
use crate::ChildPreparer;
use crate::role::{RoleConfig, RoleEvent, RoleGenerationOwner, RoleOwner, RolePreparer};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

#[derive(Debug)]
struct Source {
    refused: AtomicBool,
    checks: AtomicUsize,
}
impl RoleSourceRecheck for Source {
    fn recheck(&self) -> std::io::Result<()> {
        self.checks.fetch_add(1, Ordering::AcqRel);
        if self.refused.load(Ordering::Acquire) {
            Err(std::io::Error::other(
                "exact retained fixture input changed",
            ))
        } else {
            Ok(())
        }
    }
}
fn source(refused: bool) -> Arc<Source> {
    Arc::new(Source {
        refused: AtomicBool::new(refused),
        checks: AtomicUsize::new(0),
    })
}
fn preparer(config: RoleConfig) -> (RolePreparer, mpsc::Receiver<RoleEvent>) {
    let (events, observed) = mpsc::channel();
    (
        RolePreparer {
            config,
            generation_owner: RoleGenerationOwner::new(
                RoleOwner::Primary,
                Duration::from_secs(1),
                events,
                None,
            ),
            recovery_decision: None,
        },
        observed,
    )
}

#[test]
fn initial_recheck_refusal_cannot_provision_a_bootstrap_generation() {
    let source = source(true);
    let (mut owner, observed) =
        preparer(RoleConfig::new("unused-no-child").source_recheck(source.clone()));
    let Err(error) = owner.prepare(1) else {
        panic!("changed input must refuse");
    };
    assert!(
        error
            .to_string()
            .contains("exact retained fixture input changed")
    );
    assert_eq!(source.checks.load(Ordering::Acquire), 1);
    assert!(
        observed.try_iter().next().is_none(),
        "no role was provisioned"
    );
}

#[test]
fn repeated_actual_preparation_rechecks_before_any_successor_provision() {
    let source = source(false);
    let (mut owner, observed) =
        preparer(RoleConfig::new("unused-no-child").source_recheck(source.clone()));
    let first = owner.prepare(1).expect("actual first bootstrap listener");
    assert!(matches!(
        observed.try_recv(),
        Ok(RoleEvent::Provisioned { attempt: 1, .. })
    ));
    drop(first);
    // Consume only the first lease's synchronous retirement observations.
    assert!(
        observed
            .try_iter()
            .all(|event| !matches!(event, RoleEvent::Provisioned { .. }))
    );
    source.refused.store(true, Ordering::Release);
    let Err(error) = owner.prepare(2) else {
        panic!("successor must recheck the same source");
    };
    assert!(
        error
            .to_string()
            .contains("exact retained fixture input changed")
    );
    assert_eq!(source.checks.load(Ordering::Acquire), 2);
    assert!(
        observed
            .try_iter()
            .all(|event| !matches!(event, RoleEvent::Provisioned { .. })),
        "a failed check must occur before any real second listener/token generation"
    );
}

#[test]
fn absent_original_source_preserves_generic_preparation() {
    let (mut owner, observed) = preparer(RoleConfig::new("unused-no-child"));
    let prepared = owner.prepare(1).expect("ordinary legacy preparation");
    assert!(matches!(
        observed.try_recv(),
        Ok(RoleEvent::Provisioned { attempt: 1, .. })
    ));
    drop(prepared);
}
