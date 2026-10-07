//! `keld-attempt` claim and health record fuzz target (KEL-53 §4 "Candidate
//! connect-back" *Messages* and *Health sequence*; §6 S4b; §7 "8
//! (keld-attempt codec)").
//!
//! The codec is crate-private; its property checks live beside it in
//! `keld_ipc::fuzz_attempt_records`, behind the non-product `fuzzing` feature.
//! At every read position: decoding arbitrary bytes terminates without panic
//! and either refuses with a classified `KELD-IPC-*` error or admits a record
//! whose canonical encoding is the whole input; the stream reader agrees with
//! the slice decoder, consumes exactly one encoded record and never reads past
//! a refused magic. A violated property is a crash.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(violation) = keld_ipc::fuzz_attempt_records(data) {
        panic!("{violation}");
    }
});
