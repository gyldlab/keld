//! AC1 on the workspace-built release `keld-host.exe` (KEL-19 container spec §5, §7 row 1).
//!
//! Ignored by default. The Windows CI step builds `cargo build --release -p keld-host`
//! and runs this test with `KELD_PACK_REAL_HOST` naming that executable; the test fails,
//! never skips, when the variable is absent.
#![cfg(windows)]

use keld_pack::{
    EXPECTED_APP_IDENTITY_KEY_BYTES, ExpectedAppIdentityPayload, embed_host_identity,
    read_host_identity, read_host_identity_bytes,
};
use std::io::{Seek, SeekFrom, Write};
use std::ops::Range;

#[test]
#[ignore = "needs KELD_PACK_REAL_HOST naming a workspace-built release keld-host.exe"]
fn real_release_host_round_trip() {
    let u16_at = |image: &[u8], at: usize| {
        usize::from(u16::from_le_bytes(
            image[at..at + 2].try_into().expect("two bytes"),
        ))
    };
    let u32_at = |image: &[u8], at: usize| {
        usize::try_from(u32::from_le_bytes(
            image[at..at + 4].try_into().expect("four bytes"),
        ))
        .expect("u32 fits usize")
    };
    let path = std::env::var_os("KELD_PACK_REAL_HOST")
        .expect("KELD_PACK_REAL_HOST must name the release keld-host.exe; this test never skips");
    let host = std::fs::read(&path).expect("read the release host");
    let mut key = [0_u8; EXPECTED_APP_IDENTITY_KEY_BYTES];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = u8::try_from(index).expect("key index fits");
    }
    let payload = ExpectedAppIdentityPayload::new("com.example.app", "stable", "windows-x64", key)
        .expect("canonical payload");
    let output = embed_host_identity(&host, &payload).expect("the release host is admissible");

    // Byte-diff oracle from the PE Format layout of the input, independent of the writer.
    let nt = u32_at(&host, 0x3C);
    let sections = u16_at(&host, nt + 6);
    let optional = nt + 24;
    let file_alignment = u32_at(&host, optional + 36);
    let table_end = optional + 240 + sections * 40;
    let raw_size = payload.encode().len().next_multiple_of(file_alignment);
    assert_eq!(output.len(), host.len() + raw_size);
    let changed: [Range<usize>; 6] = [
        nt + 6..nt + 8,
        optional + 8..optional + 12,
        optional + 56..optional + 60,
        optional + 64..optional + 68,
        table_end..table_end + 40,
        host.len()..output.len(),
    ];
    let mut differing = 0_usize;
    for (offset, (before, after)) in host.iter().zip(&output).enumerate() {
        if before != after {
            differing += 1;
            assert!(
                changed.iter().any(|range| range.contains(&offset)),
                "byte {offset:#x} changed outside the listed ranges"
            );
        }
    }
    assert_eq!(u16_at(&output, nt + 6), sections + 1);
    assert_eq!(&output[table_end..table_end + 8], b".keldeai");
    assert_eq!(
        u32_at(&output, table_end + 20),
        host.len(),
        "PointerToRawData is E"
    );

    assert_eq!(
        read_host_identity_bytes(&output).expect("bytes reader"),
        payload
    );
    let mut file = tempfile::tempfile().expect("anonymous temporary file");
    file.write_all(&output).expect("write embedded host");
    file.seek(SeekFrom::End(0)).expect("cursor to end of file");
    assert_eq!(read_host_identity(&file).expect("handle reader"), payload);
    println!(
        "real host: {} input bytes, {} output bytes, {sections} -> {} sections, \
         {differing} header bytes differ before the appended {raw_size}-byte container",
        host.len(),
        output.len(),
        sections + 1
    );
}
