//! Source-only observation of a DEV D-RECOVERY two-section PE image.
//!
//! This is a bounded byte-shape reader, NOT a signature verifier, canonical
//! JSON verifier, `RecoveryPolicy` check, or recovery authority. The bytes are
//! untrusted until the original retained hFile has been checked by Guard and
//! a separate D-61/D-62 owner authenticates the directive and installed policy.

#[cfg(windows)]
use super::super::Handle;
use super::super::{
    Bytes, CONTAINER_CHARACTERISTICS, ImageSource, PADDING_CHUNK_BYTES, PackError, SECTION_NAME,
    admit_structure, container_invalid, read_exact, round_up,
};
use super::{RECOVERY_SECTION, SECTION_V2, V2ByteReader, read_v2_final_section};
use crate::expected_identity_v2::ExpectedAppIdentityV2;

const MAX_UNQUALIFIED_DIRECTIVE_BYTES: usize = 65_536;

/// Raw, untrusted observations, not an authenticated `RecoveryImage`.
/// There is no conversion to a Guard role or one-use installation permit.
#[derive(Debug)]
pub(in crate::host_identity) struct UnqualifiedDevRecovery {
    pub(in crate::host_identity) expected_destination: ExpectedAppIdentityV2,
    pub(in crate::host_identity) directive_bytes: Vec<u8>,
}

/// Observe the two PE sections from bytes, without granting a recovery role.
pub(in crate::host_identity) fn observe_local_development_recovery_bytes(
    image: &[u8],
) -> Result<UnqualifiedDevRecovery, PackError> {
    read_recovery(&Bytes(image))
}

/// Observe both PE sections on one BORROWED Windows file handle. The caller
/// serializes access because Windows positioned reads can change its cursor.
#[cfg(windows)]
pub(in crate::host_identity) fn observe_local_development_recovery_handle(
    image: &std::fs::File,
) -> Result<UnqualifiedDevRecovery, PackError> {
    read_recovery(&Handle(image))
}

fn read_recovery(source: &impl ImageSource) -> Result<UnqualifiedDevRecovery, PackError> {
    // Same single PE structural owner and same D4 final-section decoder as the
    // ordinary APP observer. Neither is a native Authenticode authority.
    let layout = admit_structure(source)?;
    let sections = layout.sections();
    if sections.iter().any(|section| section.name == SECTION_NAME) {
        return Err(PackError::IdentityContainerDuplicate);
    }
    let mut expected = sections
        .iter()
        .enumerate()
        .filter(|(_, section)| section.name == SECTION_V2);
    let Some((expected_index, _)) = expected.next() else {
        return Err(container_invalid("the recovery D4 section is absent"));
    };
    if expected.next().is_some() {
        return Err(PackError::IdentityContainerDuplicate);
    }
    let mut directives = sections
        .iter()
        .enumerate()
        .filter(|(_, section)| section.name == RECOVERY_SECTION);
    let Some((directive_index, directive_section)) = directives.next() else {
        return Err(container_invalid(
            "the recovery directive section is absent",
        ));
    };
    if directives.next().is_some() {
        return Err(PackError::IdentityContainerDuplicate);
    }
    if expected_index < 2
        || expected_index + 1 != sections.len()
        || directive_index >= expected_index
    {
        return Err(container_invalid(
            "recovery directive must precede final D4 section",
        ));
    }
    let directive_bytes = read_bounded_directive(source, directive_section, layout.file_alignment)?;
    let expected_destination = read_v2_final_section(
        source,
        &layout,
        expected_index,
        V2ByteReader::LocalDevelopmentObservation,
    )?;
    Ok(UnqualifiedDevRecovery {
        expected_destination,
        directive_bytes,
    })
}

fn read_bounded_directive(
    source: &impl ImageSource,
    section: &super::super::Section,
    file_alignment: u32,
) -> Result<Vec<u8>, PackError> {
    if section.characteristics != CONTAINER_CHARACTERISTICS {
        return Err(container_invalid(
            "recovery section is not read-only initialized data",
        ));
    }
    if section.relocations_pointer != 0
        || section.line_numbers_pointer != 0
        || section.relocation_count != 0
        || section.line_number_count != 0
    {
        return Err(container_invalid(
            "recovery section has relocation or line-number data",
        ));
    }
    let length = usize::try_from(section.virtual_size).unwrap_or(usize::MAX);
    if !(1..=MAX_UNQUALIFIED_DIRECTIVE_BYTES).contains(&length) {
        return Err(container_invalid(
            "recovery section size is outside directive bounds",
        ));
    }
    if u64::from(section.raw_size) != round_up(u64::from(section.virtual_size), file_alignment) {
        return Err(container_invalid(
            "recovery section raw size is not aligned",
        ));
    }
    let raw_start = u64::from(section.raw_pointer);
    let raw_end = section.raw_end();
    let mut payload = vec![0_u8; length]; // bounded at 65,536 bytes
    read_exact(
        source,
        raw_start,
        &mut payload,
        container_invalid("recovery directive exceeds retained image"),
    )?;
    let mut buffer = [0_u8; PADDING_CHUNK_BYTES];
    let mut cursor = raw_start + length as u64;
    while cursor < raw_end {
        let count = usize::try_from(raw_end - cursor)
            .unwrap_or(PADDING_CHUNK_BYTES)
            .min(PADDING_CHUNK_BYTES);
        let (slice, _) = buffer.split_at_mut(count);
        read_exact(
            source,
            cursor,
            slice,
            container_invalid("recovery padding exceeds retained image"),
        )?;
        if slice.iter().any(|&b| b != 0) {
            return Err(container_invalid("recovery section padding is nonzero"));
        }
        cursor += count as u64;
    }
    // D-61/D-62 must still authenticate canonical JSON, signed original hFile,
    // independent recovery signer/role and immutable installed RecoveryPolicy.
    Ok(payload)
}
