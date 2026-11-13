//! Exact fixed-width Windows lifecycle Job-handoff and receipt frame encoding.
//!
//! The parent remains the sole owner of Win32 Job/peer HANDLE operations and
//! release/termination authority. This private module serializes already
//! authenticated attempt/channel IDs and attenuated numeric handle values.

use keld_ipc::{WindowsLifecycleBinding, WindowsLifecyclePurpose};

use super::{
    LIFECYCLE_ATTEMPT_ACK_LEN, LIFECYCLE_ATTEMPT_HANDOFF_LEN, LIFECYCLE_QUERY_HANDOFF_LEN,
    WindowsHostJobError,
};

pub(super) fn encode_lifecycle_attempt_handoff(
    binding: WindowsLifecycleBinding,
    remote_job: usize,
    remote_lease: usize,
) -> Result<[u8; LIFECYCLE_ATTEMPT_HANDOFF_LEN], WindowsHostJobError> {
    let remote_job = u64::try_from(remote_job).map_err(|_| {
        WindowsHostJobError::contract(
            "lifecycle attempt handoff record",
            "remote Job handle exceeds the wire integer width",
        )
    })?;
    let remote_lease = u64::try_from(remote_lease).map_err(|_| {
        WindowsHostJobError::contract(
            "lifecycle attempt handoff record",
            "remote lease handle exceeds the wire integer width",
        )
    })?;
    let mut record = [0_u8; LIFECYCLE_ATTEMPT_HANDOFF_LEN];
    record[..8].copy_from_slice(b"KELD-HO1");
    record[8] = WindowsLifecyclePurpose::CoordinatorToKeeper as u8;
    record[9..41].copy_from_slice(binding.attempt_id());
    record[41..73].copy_from_slice(binding.lifecycle_channel_id());
    record[73..81].copy_from_slice(&remote_job.to_le_bytes());
    record[81..89].copy_from_slice(&remote_lease.to_le_bytes());
    Ok(record)
}

pub(super) fn encode_lifecycle_query_handoff(
    binding: WindowsLifecycleBinding,
    remote_query: usize,
) -> Result<[u8; LIFECYCLE_QUERY_HANDOFF_LEN], WindowsHostJobError> {
    let remote_query = u64::try_from(remote_query).map_err(|_| {
        WindowsHostJobError::contract(
            "lifecycle successor query offer",
            "remote query handle exceeds the wire integer width",
        )
    })?;
    let mut record = [0_u8; LIFECYCLE_QUERY_HANDOFF_LEN];
    record[..8].copy_from_slice(b"KELD-QO1");
    record[8] = WindowsLifecyclePurpose::KeeperToSuccessor as u8;
    record[9..41].copy_from_slice(binding.attempt_id());
    record[41..73].copy_from_slice(binding.lifecycle_channel_id());
    record[73..81].copy_from_slice(&remote_query.to_le_bytes());
    Ok(record)
}

pub(super) fn encode_lifecycle_query_receipt(
    magic: [u8; 8],
    binding: WindowsLifecycleBinding,
    active_processes: u32,
) -> [u8; LIFECYCLE_ATTEMPT_ACK_LEN] {
    let mut receipt = [0_u8; LIFECYCLE_ATTEMPT_ACK_LEN];
    receipt[..8].copy_from_slice(&magic);
    receipt[8..40].copy_from_slice(binding.attempt_id());
    receipt[40..72].copy_from_slice(binding.lifecycle_channel_id());
    receipt[72..76].copy_from_slice(&active_processes.to_le_bytes());
    receipt
}

pub(super) fn encode_lifecycle_attempt_ack(
    binding: WindowsLifecycleBinding,
    active_processes: u32,
) -> [u8; LIFECYCLE_ATTEMPT_ACK_LEN] {
    let mut acknowledgement = [0_u8; LIFECYCLE_ATTEMPT_ACK_LEN];
    acknowledgement[..8].copy_from_slice(b"KELD-HR1");
    acknowledgement[8..40].copy_from_slice(binding.attempt_id());
    acknowledgement[40..72].copy_from_slice(binding.lifecycle_channel_id());
    acknowledgement[72..76].copy_from_slice(&active_processes.to_le_bytes());
    acknowledgement
}

#[cfg(test)]
mod tests {
    use super::{
        encode_lifecycle_attempt_ack, encode_lifecycle_attempt_handoff,
        encode_lifecycle_query_handoff, encode_lifecycle_query_receipt,
    };
    use keld_ipc::{WindowsLifecycleBinding, WindowsLifecyclePurpose};

    fn binding(purpose: WindowsLifecyclePurpose) -> WindowsLifecycleBinding {
        WindowsLifecycleBinding::new([0x11; 32], [0x22; 32], [0x33; 32], purpose)
            .expect("distinct nonzero install/attempt/channel identifiers")
    }

    #[test]
    fn exact_attempt_handoff_frame_preserves_binding_purpose_and_handle_order() {
        let record = encode_lifecycle_attempt_handoff(
            binding(WindowsLifecyclePurpose::CoordinatorToKeeper),
            0x1234,
            0x5678,
        )
        .expect("bounded numeric remote handles");
        let mut expected = vec![b'K', b'E', b'L', b'D', b'-', b'H', b'O', b'1', 1];
        expected.extend([0x22; 32]); // authenticated attempt, not installation
        expected.extend([0x33; 32]); // authenticated lifecycle channel
        expected.extend([0x34, 0x12, 0, 0, 0, 0, 0, 0]); // job u64 little-endian
        expected.extend([0x78, 0x56, 0, 0, 0, 0, 0, 0]); // lease u64 little-endian
        assert_eq!(record.as_slice(), expected);
    }

    #[test]
    fn exact_query_handoff_frame_has_distinct_magic_purpose_and_query_handle() {
        let record = encode_lifecycle_query_handoff(
            binding(WindowsLifecyclePurpose::KeeperToSuccessor),
            0x9abc,
        )
        .expect("bounded numeric remote query handle");
        let mut expected = vec![b'K', b'E', b'L', b'D', b'-', b'Q', b'O', b'1', 2];
        expected.extend([0x22; 32]);
        expected.extend([0x33; 32]);
        expected.extend([0xbc, 0x9a, 0, 0, 0, 0, 0, 0]);
        assert_eq!(record.as_slice(), expected);
    }

    #[test]
    fn exact_query_and_attempt_receipts_preserve_literal_magic_nonce_and_count() {
        let binding = binding(WindowsLifecyclePurpose::CoordinatorToKeeper);
        let mut expected = Vec::from(*b"KELD-HR1");
        expected.extend([0x22; 32]);
        expected.extend([0x33; 32]);
        expected.extend([0x78, 0x56, 0x34, 0x12]);
        assert_eq!(
            encode_lifecycle_attempt_ack(binding, 0x1234_5678).as_slice(),
            expected
        );

        let mut final_expected = expected.clone();
        final_expected[..8].copy_from_slice(b"KELD-QF1");
        assert_eq!(
            encode_lifecycle_query_receipt(*b"KELD-QF1", binding, 0x1234_5678).as_slice(),
            final_expected
        );
        final_expected[..8].copy_from_slice(b"KELD-QA1");
        assert_eq!(
            encode_lifecycle_query_receipt(*b"KELD-QA1", binding, 0x1234_5678).as_slice(),
            final_expected
        );
    }
}
