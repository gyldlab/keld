//! Separate on-wire refusal oracle: a stalled authenticated Guardian role
//! cannot indefinitely hang the parent test after connecting to the witness.
use std::io::{self, Read as _};
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::Instant;

pub(super) fn await_denial_witness(listener: &UnixListener, deadline: Instant) -> String {
    await_denial_witness_with_hook(listener, deadline, |_| {})
}

#[derive(Clone, Copy, Debug)]
enum WitnessReadStage {
    AfterNativeAccept,
    AcceptedWithinDeadline,
    BeforeNativeRead(usize),
    AfterNativeByte(u8),
}

// The original UnixListener and actual kernel byte, not a forged event, is
// observed in each test. The default hook is inert and carries no authority.
fn await_denial_witness_with_hook(
    listener: &UnixListener,
    deadline: Instant,
    mut observe: impl FnMut(WitnessReadStage),
) -> String {
    loop {
        assert!(
            Instant::now() < deadline,
            "real KGC1 denial witness accept exceeded original deadline"
        );
        match listener.accept() {
            Ok((mut stream, _)) => {
                observe(WitnessReadStage::AfterNativeAccept);
                assert!(
                    Instant::now() < deadline,
                    "native KGC1 denial witness accepted after original deadline"
                );
                observe(WitnessReadStage::AcceptedWithinDeadline);
                stream.set_nonblocking(true).expect("bounded KGC1 witness");
                let mut bytes = Vec::with_capacity(40);
                loop {
                    assert!(
                        Instant::now() < deadline,
                        "native KGC1 denial witness byte timed out before read"
                    );
                    observe(WitnessReadStage::BeforeNativeRead(bytes.len()));
                    let mut byte = [0_u8; 1];
                    match stream.read(&mut byte) {
                        Ok(1) => {
                            observe(WitnessReadStage::AfterNativeByte(byte[0]));
                            assert!(
                                Instant::now() < deadline,
                                "native KGC1 denial witness byte arrived after original deadline"
                            );
                            bytes.push(byte[0]);
                            assert!(bytes.len() <= 80, "bounded KGC1 denial witness");
                            if byte[0] == b'\n' {
                                break;
                            }
                        }
                        Ok(0) => panic!("Guardian truncated independent denial witness"),
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                            ) => {}
                        Err(error) => panic!("read Guardian refusal witness: {error}"),
                        Ok(_) => unreachable!("one-byte read"),
                    }
                    assert!(
                        Instant::now() < deadline,
                        "stalled Guardian denied registration without complete witness"
                    );
                    thread::yield_now();
                }
                assert!(
                    Instant::now() < deadline,
                    "native KGC1 denial witness decoded after original deadline"
                );
                return String::from_utf8(bytes).expect("Guardian witness is UTF-8");
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "real KGC1 denial witness absent");
                thread::yield_now();
            }
            Err(error) => panic!("accept KGC1 denial witness: {error}"),
        }
    }
}

#[cfg(test)]
#[path = "witness/deadline_tests.rs"]
mod deadline_tests;
