//! Failed socket/command owners retain teardown and correlation custody.
use super::{Fixture, TestResult, fact, install_successor, reply, verdict};
use keld_ipc::codec::{decode, encode};
use keld_ipc::link::{AppLinkDeadlines as _, read_frame};
use keld_ipc::window::{CloseVerdict, WindowEvent, WindowRequest, WindowResponse};
use keld_ipc::{CorrelationId, FrameKind};
use keld_wv::{AppWindowCommand, AppWindowEvent, WebviewId};
use std::time::Duration;

fn shutdown_host_writer(fixture: &Fixture) -> TestResult {
    // Inject the fault at the owned writer, keeping registry/caller admission
    // live. Peer shutdown alone does not establish a failed host-write premise.
    let current = fixture
        .handle
        .current
        .lock()
        .map_err(|_| std::io::Error::other("host writer fixture lock poisoned"))?;
    current
        .as_ref()
        .ok_or_else(|| std::io::Error::other("host writer fixture is absent"))?
        .writer
        .shutdown_app_link()?;
    Ok(())
}

#[test]
fn destroy_correlation_reuse_retires_the_link_before_a_second_effect() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    fixture.call(&WindowRequest::Destroy { window }, 2)?;
    assert_eq!(
        fixture.commands.recv_timeout(Duration::from_secs(3))?,
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    let failure = fixture
        .call(&WindowRequest::RequestClose { window }, 2)
        .expect_err("pending Destroy owns correlation");
    assert!(failure.to_string().contains("KELD-IPC-005"));
    assert!(
        read_frame(&mut fixture.peer).is_err(),
        "actual retirement wakes pending caller by link loss"
    );
    assert!(fixture.commands.try_recv().is_err());
    install_successor(&mut fixture)?;
    fixture.subscribe()?;
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowReleased { view: WebviewId(7) })?;
    // A new admitted request fences the writer after the old Released input.
    fixture.call(&WindowRequest::RequestClose { window }, 3)?;
    fixture.error(3, "KELD-CORE-038")?;
    fixture.retire_reader(2)?;
    Ok(())
}

#[test]
fn teardown_enqueue_failure_remains_an_error_after_the_real_allow_reply() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    fixture.call(&WindowRequest::RequestClose { window }, 2)?;
    fact(
        &mut fixture,
        WindowEvent::CloseRequested {
            window,
            close_seq: 1,
        },
    )?;
    reply(
        &mut fixture,
        2,
        WindowResponse::CloseRequested { close_seq: 1 },
    )?;
    let caller = fixture.caller()?;
    drop(fixture.commands);
    let failure = fixture
        .handle
        .dispatch_window(
            &caller,
            CorrelationId(3),
            &encode(&WindowRequest::CloseReply {
                window,
                close_seq: 1,
                verdict: CloseVerdict::Allow,
            })?,
        )
        .expect_err("native owner loss remains observable");
    assert!(
        failure
            .to_string()
            .contains("UI command owner is unavailable")
    );
    let (header, bytes) = read_frame(&mut fixture.peer)?;
    assert_eq!(
        (header.kind, header.corr),
        (FrameKind::Reply, CorrelationId(3))
    );
    assert_eq!(
        decode::<WindowResponse>(&bytes)?,
        WindowResponse::ReplyApplied
    );
    Ok(())
}

#[test]
fn failed_allow_acknowledgement_still_queues_the_committed_teardown() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    fixture.call(&WindowRequest::RequestClose { window }, 2)?;
    fact(
        &mut fixture,
        WindowEvent::CloseRequested {
            window,
            close_seq: 1,
        },
    )?;
    reply(
        &mut fixture,
        2,
        WindowResponse::CloseRequested { close_seq: 1 },
    )?;
    shutdown_host_writer(&fixture)?;
    let failure = verdict(&fixture, window, 1, CloseVerdict::Allow, 3)
        .expect_err("the actual failed socket write remains observable");
    assert!(
        failure.to_string().contains("window ordered writer"),
        "{failure}"
    );
    assert_eq!(
        fixture.commands.try_recv().expect(
            "accepted Allow must release native handles even when its acknowledgement fails"
        ),
        AppWindowCommand::DestroyWindow { view: WebviewId(7) }
    );
    assert!(fixture.commands.try_recv().is_err());
    fixture
        .handle
        .window_native_event(AppWindowEvent::WindowReleased { view: WebviewId(7) })?;
    assert!(
        fixture
            .handle
            .current
            .lock()
            .expect("retired writer")
            .is_none()
    );
    Ok(())
}

#[test]
fn failed_allow_acknowledgement_and_enqueue_preserve_both_independent_faults() -> TestResult {
    let mut fixture = Fixture::new(Some(true))?;
    fixture.subscribe()?;
    fixture.ready()?;
    let window = fixture.build(1, 7)?;
    fixture.call(&WindowRequest::RequestClose { window }, 2)?;
    fact(
        &mut fixture,
        WindowEvent::CloseRequested {
            window,
            close_seq: 1,
        },
    )?;
    reply(
        &mut fixture,
        2,
        WindowResponse::CloseRequested { close_seq: 1 },
    )?;
    let caller = fixture.caller()?;
    shutdown_host_writer(&fixture)?;
    drop(fixture.commands);
    let failure = fixture
        .handle
        .dispatch_window(
            &caller,
            CorrelationId(3),
            &encode(&WindowRequest::CloseReply {
                window,
                close_seq: 1,
                verdict: CloseVerdict::Allow,
            })?,
        )
        .expect_err("reply delivery and committed teardown are separate failing effects");
    let detail = failure.to_string();
    assert!(detail.contains("window ordered writer"), "{detail}");
    assert!(
        detail.contains("UI command owner is unavailable"),
        "{detail}"
    );
    assert!(
        fixture
            .handle
            .current
            .lock()
            .expect("retired writer")
            .is_none()
    );
    Ok(())
}
