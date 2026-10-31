//! Separate on-wire refusal oracle: a stalled authenticated Guardian role
//! cannot indefinitely hang the parent test after connecting to the witness.
use std::io::{self, Read as _};
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::Instant;

pub(super) fn await_denial_witness(listener: &UnixListener, deadline: Instant) -> String {
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(true).expect("bounded KGC1 witness");
                let mut bytes = Vec::with_capacity(40);
                loop {
                    let mut byte = [0_u8; 1];
                    match stream.read(&mut byte) {
                        Ok(1) => {
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
