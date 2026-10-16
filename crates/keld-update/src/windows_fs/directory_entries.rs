//! Bounded, safe parser of Windows directory enumeration records.
//!
//! Treat returned enumeration bytes as untrusted until all offsets, UTF-16
//! lengths and path components are checked. Kernel queries, retained handles,
//! and privileged deletion authority remain owned by `windows_fs.rs`.

use std::io;

use windows_sys::Win32::Storage::FileSystem::FILE_ID_BOTH_DIR_INFO;

/// One child listed from a directory handle: its exact name and listed attributes.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct DirectoryChild {
    pub(super) name: Vec<u16>,
    pub(super) attributes: u32,
}

/// Parses a filled `FILE_ID_BOTH_DIR_INFO` chain, checking every offset and length
/// against the buffer and requiring each record to contain its own name before its
/// successor. `.` and `..` are omitted; any other name must be one component.
pub(super) fn parse_directory_entries(bytes: &[u8]) -> io::Result<Vec<DirectoryChild>> {
    let malformed = || io::Error::other("malformed directory enumeration record");
    let read_u32 = |offset: usize| -> io::Result<u32> {
        let end = offset.checked_add(4).ok_or_else(malformed)?;
        let field = bytes.get(offset..end).ok_or_else(malformed)?;
        Ok(u32::from_ne_bytes(
            field.try_into().map_err(|_| malformed())?,
        ))
    };
    let field = |entry: usize, at: usize| entry.checked_add(at).ok_or_else(malformed);
    let name_offset = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
    let mut children = Vec::new();
    let mut entry = 0_usize;
    loop {
        let next = read_u32(field(
            entry,
            std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset),
        )?)?;
        let attributes = read_u32(field(
            entry,
            std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileAttributes),
        )?)?;
        let name_bytes = read_u32(field(
            entry,
            std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength),
        )?)?;
        let start = field(entry, name_offset)?;
        let length = usize::try_from(name_bytes).map_err(|_| malformed())?;
        let end = start.checked_add(length).ok_or_else(malformed)?;
        let raw = bytes.get(start..end).ok_or_else(malformed)?;
        if length % 2 != 0 {
            return Err(malformed());
        }
        let name: Vec<u16> = raw
            .chunks_exact(2)
            .map(|unit| u16::from_ne_bytes([unit[0], unit[1]]))
            .collect();
        if name != [u16::from(b'.')] && name != [u16::from(b'.'), u16::from(b'.')] {
            if name.is_empty()
                || name
                    .iter()
                    .any(|unit| matches!(*unit, 0 | 0x2f | 0x3a | 0x5c))
            {
                return Err(io::Error::other("directory entry is not one component"));
            }
            children.push(DirectoryChild { name, attributes });
        }
        if next == 0 {
            return Ok(children);
        }
        let step = usize::try_from(next).map_err(|_| malformed())?;
        if step < name_offset.checked_add(length).ok_or_else(malformed)? {
            return Err(malformed());
        }
        entry = entry.checked_add(step).ok_or_else(malformed)?;
    }
}
