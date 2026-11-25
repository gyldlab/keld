//! Private D-4 PE identity v2 container trial.
//!
//! Shares the v1 PE structural admission and positioned-read owner.
//! Neither this opt-in byte writer nor reader authenticates `WinTrust` or a Host,
//! and no public producer, reader, or recovery permission is added.
//! Recovery section `.keldrec` belongs to the separate D-61/D-62 `RecoveryImage`
//! owner; this ordinary Host path rejects it rather than adopting its role.

#[cfg(windows)]
use super::Handle;
use super::{
    Bytes, CHECKSUM, COFF_END, CONTAINER_CHARACTERISTICS, ImageSource, LOADER_MAX_SECTIONS, Layout,
    NUMBER_OF_SECTIONS, PADDING_CHUNK_BYTES, PackError, SECTION_HEADER_BYTES, SECTION_NAME,
    SIZE_OF_IMAGE, SIZE_OF_INITIALIZED_DATA, admit_structure, container_invalid, invalid, no_room,
    not_pristine, patch, read_exact, round_up, section_header, to_index,
};
use crate::expected_identity_v2::{ExpectedAppIdentityV2, MAX_PAYLOAD_BYTES, MIN_PAYLOAD_BYTES};

const SECTION_V2: [u8; 8] = *b".keldea2";
const RECOVERY_SECTION: [u8; 8] = *b".keldrec";

// Source-only D-RECOVERY PE observation. This has no signature, policy, or
// installation authority and is not reachable from the production Pack API.
#[allow(dead_code)]
pub(super) mod recovery;

/// Private codec selection fixed by the caller's compiled entry point, never
/// by the PE, a request, an argv flag, or an observed signature.
#[derive(Clone, Copy)]
enum V2ByteReader {
    Production,
    LocalDevelopmentObservation,
}

/// Add only an ordinary v2 section to an unsigned, unmodified prebuilt PE.
/// The existing v1 writer remains byte-for-byte unchanged.
pub(super) fn embed_ordinary_v2(
    host: &[u8],
    payload: &ExpectedAppIdentityV2,
) -> Result<Vec<u8>, PackError> {
    let layout = admit_structure(&Bytes(host))?;
    let encoded = payload.encode()?;
    if layout
        .sections()
        .iter()
        .any(|section| matches!(section.name, SECTION_NAME | SECTION_V2 | RECOVERY_SECTION))
    {
        return Err(PackError::IdentityContainerDuplicate);
    }
    if layout.certificate != (0, 0) {
        return Err(not_pristine("the Certificate Table entry is present"));
    }
    if layout.len != layout.raw_end {
        return Err(not_pristine("bytes follow the last section's raw data"));
    }
    if layout.count >= LOADER_MAX_SECTIONS {
        return Err(no_room("the section table already holds 96 sections"));
    }
    let slack_start = layout.table_end();
    let slack_end = slack_start + SECTION_HEADER_BYTES as u64;
    if slack_end > u64::from(layout.size_of_headers) {
        return Err(no_room("no 40-byte zero section-table header slot"));
    }
    let Some(slack) = host.get(to_index(slack_start)..to_index(slack_end)) else {
        return Err(no_room("section-table slot lies beyond the file"));
    };
    if slack.iter().any(|&value| value != 0) {
        return Err(no_room("section-table slot is not zero"));
    }
    if layout.bound_import {
        return Err(no_room("the bound-import directory is present"));
    }

    let length = u64::try_from(encoded.len()).map_err(|_| invalid("v2 payload length"))?;
    let raw_size = round_up(length, layout.file_alignment);
    let size_of_image = round_up(
        u64::from(layout.size_of_image).saturating_add(length),
        layout.section_alignment,
    );
    let initialized = u64::from(layout.size_of_initialized_data).saturating_add(raw_size);
    let (Ok(virtual_size), Ok(raw_size_field), Ok(raw_pointer), Ok(image_size), Ok(initialized)) = (
        u32::try_from(length),
        u32::try_from(raw_size),
        u32::try_from(layout.raw_end),
        u32::try_from(size_of_image),
        u32::try_from(initialized),
    ) else {
        return Err(invalid("a v2 section header field would overflow 32 bits"));
    };
    let section_count = u16::try_from(layout.count + 1).map_err(|_| invalid("v2 section count"))?;
    let mut header = section_header(
        virtual_size,
        layout.size_of_image,
        raw_size_field,
        raw_pointer,
    );
    header[..SECTION_V2.len()].copy_from_slice(&SECTION_V2);
    let total = host
        .len()
        .checked_add(to_index(raw_size))
        .ok_or_else(|| invalid("v2 image length overflow"))?;
    let mut image = Vec::with_capacity(total);
    image.extend_from_slice(host);
    let optional = layout.nt + COFF_END as u64;
    patch(
        &mut image,
        layout.nt + NUMBER_OF_SECTIONS as u64,
        &section_count.to_le_bytes(),
    );
    patch(
        &mut image,
        optional + SIZE_OF_INITIALIZED_DATA as u64,
        &initialized.to_le_bytes(),
    );
    patch(
        &mut image,
        optional + SIZE_OF_IMAGE as u64,
        &image_size.to_le_bytes(),
    );
    patch(&mut image, optional + CHECKSUM as u64, &0_u32.to_le_bytes());
    patch(&mut image, slack_start, &header);
    image.extend_from_slice(&encoded);
    image.resize(total, 0);
    match read_ordinary_v2(&Bytes(&image)) {
        Ok(actual) if actual == *payload => Ok(image),
        _ => Err(container_invalid(
            "v2 writer readback differs from its payload",
        )),
    }
}

/// Read exact ordinary v2 identity from source bytes. Not a signature or role
/// check; the native adapter must first prove the same image handle with Guard.
pub(super) fn read_ordinary_v2_bytes(image: &[u8]) -> Result<ExpectedAppIdentityV2, PackError> {
    read_ordinary_v2(&Bytes(image))
}

/// Read the same container from one retained native image handle, without
/// reopening its pathname. The caller still needs Guard authorization.
#[cfg(windows)]
pub(super) fn read_ordinary_v2_handle(
    image: &std::fs::File,
) -> Result<ExpectedAppIdentityV2, PackError> {
    read_ordinary_v2(&Handle(image))
}

/// Observe only the DEV APP identity from synthetic or already-retained bytes.
/// This parses a container, not an Authenticode signature or trusted publisher.
/// The caller must independently retain and authenticate its source and policy.
/// This ORDINARY APP path intentionally refuses `.keldrec`. A dedicated
/// `RecoveryImage` requires a separately qualified D-RECOVERY reader that binds
/// both signed sections from the same retained file and the independent role
/// policy; DEV APP observation alone cannot authorize recovery.
pub(super) fn observe_local_development_v2_bytes(
    image: &[u8],
) -> Result<ExpectedAppIdentityV2, PackError> {
    read_v2(&Bytes(image), V2ByteReader::LocalDevelopmentObservation)
}

/// Observe DEV bytes on the SAME borrowed Windows file object using positioned
/// reads. Never opens or reopens a path and never produces signer/role authority.
/// Windows `FileExt::seek_read` changes this shared handle's cursor. The
/// caller must serialize access; only the explicit read offsets are stable.
#[cfg(windows)]
pub(super) fn observe_local_development_v2_handle(
    image: &std::fs::File,
) -> Result<ExpectedAppIdentityV2, PackError> {
    read_v2(&Handle(image), V2ByteReader::LocalDevelopmentObservation)
}

fn read_ordinary_v2(source: &impl ImageSource) -> Result<ExpectedAppIdentityV2, PackError> {
    read_v2(source, V2ByteReader::Production)
}

fn read_v2(
    source: &impl ImageSource,
    byte_reader: V2ByteReader,
) -> Result<ExpectedAppIdentityV2, PackError> {
    let layout = admit_structure(source)?;
    let sections = layout.sections();
    // Cross-version and recovery-role ambiguity always refuses, even if a
    // valid v2 identity happens to be the final section.
    if sections.iter().any(|section| section.name == SECTION_NAME) {
        return Err(PackError::IdentityContainerDuplicate);
    }
    if sections
        .iter()
        .any(|section| section.name == RECOVERY_SECTION)
    {
        return Err(container_invalid(
            "a RecoveryImage cannot be read as an ordinary Host",
        ));
    }
    let mut matching = sections
        .iter()
        .enumerate()
        .filter(|(_, section)| section.name == SECTION_V2);
    let Some((index, _)) = matching.next() else {
        return Err(container_invalid("the v2 identity section is absent"));
    };
    if matching.next().is_some() {
        return Err(PackError::IdentityContainerDuplicate);
    }
    read_v2_final_section(source, &layout, index, byte_reader)
}

// Both the ordinary and the deliberately nonauthorizing recovery observer
// share the exact final D4 section validator and byte codec. The caller must
// have separately established its allowed operation and section census.
fn read_v2_final_section(
    source: &impl ImageSource,
    layout: &Layout,
    index: usize,
    byte_reader: V2ByteReader,
) -> Result<ExpectedAppIdentityV2, PackError> {
    let sections = layout.sections();
    if index == 0 || index + 1 != sections.len() {
        return Err(container_invalid(
            "the v2 identity must be the final PE section",
        ));
    }
    let container = sections
        .get(index)
        .ok_or_else(|| container_invalid("the v2 identity section is absent"))?;
    if container.characteristics != CONTAINER_CHARACTERISTICS {
        return Err(container_invalid(
            "the v2 section attributes are not read-only initialized data",
        ));
    }
    if container.relocations_pointer != 0
        || container.line_numbers_pointer != 0
        || container.relocation_count != 0
        || container.line_number_count != 0
    {
        return Err(container_invalid(
            "the v2 section has relocation or line-number data",
        ));
    }
    let length = usize::try_from(container.virtual_size).unwrap_or(usize::MAX);
    if !(MIN_PAYLOAD_BYTES..=MAX_PAYLOAD_BYTES).contains(&length) {
        return Err(container_invalid(
            "v2 VirtualSize is outside payload bounds",
        ));
    }
    if u64::from(container.raw_size)
        != round_up(u64::from(container.virtual_size), layout.file_alignment)
    {
        return Err(container_invalid(
            "v2 raw size does not match aligned payload",
        ));
    }
    let raw_start = u64::from(container.raw_pointer);
    let raw_end = container.raw_end();
    if sections
        .get(..index)
        .unwrap_or_default()
        .iter()
        .any(|previous| previous.raw_end() > raw_start)
    {
        return Err(container_invalid("v2 section is not the last PE raw data"));
    }
    let (cert_start, cert_size) = layout.certificate;
    if (cert_start == 0) != (cert_size == 0) {
        return Err(container_invalid(
            "v2 certificate directory has only one of offset and size",
        ));
    }
    if (cert_start, cert_size) != (0, 0)
        && (u64::from(cert_start) < raw_end
            || u64::from(cert_start) + u64::from(cert_size) > layout.len)
    {
        return Err(container_invalid(
            "certificate table overlaps or follows image end",
        ));
    }
    read_v2_payload_and_padding(source, raw_start, raw_end, length, byte_reader)
}

/// Reads only the v2 raw payload and zero padding, never a certificate table.
fn read_v2_payload_and_padding(
    source: &impl ImageSource,
    raw_start: u64,
    raw_end: u64,
    length: usize,
    byte_reader: V2ByteReader,
) -> Result<ExpectedAppIdentityV2, PackError> {
    let mut payload = [0_u8; MAX_PAYLOAD_BYTES];
    let (payload, _) = payload.split_at_mut(length);
    read_exact(
        source,
        raw_start,
        payload,
        container_invalid("v2 payload raw range exceeds the image"),
    )?;
    // Padding can be up to FileAlignment-1 bytes; do not allocate it from
    // file-controlled lengths or scan a signed certificate/overlay.
    let mut chunk = [0_u8; PADDING_CHUNK_BYTES];
    let mut offset = raw_start + length as u64;
    while offset < raw_end {
        let step = usize::try_from(raw_end - offset)
            .unwrap_or(PADDING_CHUNK_BYTES)
            .min(PADDING_CHUNK_BYTES);
        let (padding, _) = chunk.split_at_mut(step);
        read_exact(
            source,
            offset,
            padding,
            container_invalid("v2 padding extends beyond the image"),
        )?;
        if padding.iter().any(|&byte| byte != 0) {
            return Err(container_invalid("v2 padding is nonzero"));
        }
        offset += step as u64;
    }
    match byte_reader {
        V2ByteReader::Production => ExpectedAppIdentityV2::decode(payload),
        V2ByteReader::LocalDevelopmentObservation => {
            ExpectedAppIdentityV2::decode_local_development(payload)
        }
    }
}
