//! The one kipc channel table (GH-508).
//!
//! Spec: `docs/specs/gh508-kipc-channel-table.md`. Every production use of a
//! kipc channel id derives from an entry in this file. The TypeScript transport
//! constants are generated from it by `packages/@keld/kipc/scripts/echo-codegen.ts`
//! (`bun run echo:generate`), so the entry shape below is that generator's
//! admitted grammar: one `pub const NAME: ChannelEntry` per entry in the
//! rustfmt-normalized form, listed once in [`CHANNEL_TABLE`].
//!
//! Allocation is append-only (spec §4.4). A new entry takes the current maximum
//! id plus one and appends the same `name id` line to
//! `crates/keld-ipc/channel_allocations.txt`. Ids are never reused, renumbered,
//! filled into gaps, reserved ahead of time or grouped by family, and an
//! existing entry is never removed: a retired channel keeps its entry.
//!
//! A channel id is a routing selector, not authority. [`Authority`] declares
//! what authorizes a call; `keld-guard` still evaluates every guarded request.
//! A receive policy for a privileged channel is built from an entry, never
//! from a raw id:
//!
//! ```compile_fail
//! use keld_ipc::{ChannelId, ReceivePolicy};
//! let _ = ReceivePolicy::privileged_call_receiver(ChannelId(4));
//! ```
//!
//! ```
//! use keld_ipc::{ReceivePolicy, channel_table};
//! let policy = ReceivePolicy::privileged_call_receiver(&channel_table::FS)
//!     .expect("fs is a guarded CALL channel");
//! assert_eq!(policy.channel, channel_table::FS.id());
//! ```

use keld_guard::capability::{FS_READ, FS_WRITE};

use crate::frame::ChannelId;

/// Channel id reserved for `HELLO` (architecture 02 §2). Never allocatable.
pub const HANDSHAKE_CHANNEL: ChannelId = ChannelId(0);

/// Which KEL-133 receive-policy family may name a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReceiveClass {
    /// App-to-host `CALL`, answered by `REPLY` or a declared `ERR`; not
    /// guard-routed (KEL-133 rows: host echo receiver, echo caller waiter,
    /// primary app receiver).
    HostCall,
    /// [`Self::HostCall`] plus host-to-app `EVENT`s on the same id (rows: host
    /// lifecycle receiver, app lifecycle event receiver, app lifecycle reply
    /// waiter).
    HostCallWithEvents,
    /// App-to-host `CALL` routed through `guard_dispatch`, whose `ERR` is a
    /// `CallError` (row: privileged receiver).
    GuardedCall,
}

/// What authorizes a call on a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Session control on the host-minted link; never evaluated by `keld-guard`.
    HostInternal,
    /// Every call is evaluated by `keld-guard` against one of these manifest
    /// capability names. They are `keld_guard::capability` constants, never
    /// string literals here.
    Guarded(&'static [&'static str]),
}

/// One allocated kipc channel.
///
/// Only this module constructs entries, so [`CHANNEL_TABLE`] is the only list
/// of allocated ids:
///
/// ```compile_fail
/// use keld_ipc::channel_table::{Authority, ChannelEntry, ReceiveClass};
/// const PROBE: ChannelEntry =
///     ChannelEntry::new("probe", 4, ReceiveClass::HostCall, Authority::HostInternal);
/// ```
///
/// ```
/// use keld_ipc::channel_table::{Authority, ChannelEntry, ECHO, ReceiveClass};
/// const ENTRY: ChannelEntry = ECHO;
/// assert_eq!(ENTRY.name(), "echo");
/// assert_eq!(ENTRY.id().0, 1);
/// assert_eq!(ENTRY.class(), ReceiveClass::HostCall);
/// assert_eq!(ENTRY.authority(), Authority::HostInternal);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelEntry {
    name: &'static str,
    id: ChannelId,
    class: ReceiveClass,
    authority: Authority,
}

impl ChannelEntry {
    const fn new(name: &'static str, id: u16, class: ReceiveClass, authority: Authority) -> Self {
        Self {
            name,
            id: ChannelId(id),
            class,
            authority,
        }
    }

    /// Entry name: `[a-z][a-z0-9-]{0,31}`, never an Electron API or family name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Wire id carried in the frame header's `channel` field.
    #[must_use]
    pub const fn id(&self) -> ChannelId {
        self.id
    }

    /// Receive-policy family that may name this channel.
    #[must_use]
    pub const fn class(&self) -> ReceiveClass {
        self.class
    }

    /// What authorizes a call on this channel.
    #[must_use]
    pub const fn authority(&self) -> Authority {
        self.authority
    }
}

/// Generic session echo (KEL-30).
pub const ECHO: ChannelEntry =
    ChannelEntry::new("echo", 1, ReceiveClass::HostCall, Authority::HostInternal);
/// Retained filesystem broker (KEL-130).
pub const FS: ChannelEntry = ChannelEntry::new(
    "fs",
    2,
    ReceiveClass::GuardedCall,
    Authority::Guarded(&[FS_READ, FS_WRITE]),
);
/// Host session lifecycle (KEL-72).
pub const LIFECYCLE: ChannelEntry = ChannelEntry::new(
    "lifecycle",
    3,
    ReceiveClass::HostCallWithEvents,
    Authority::HostInternal,
);

/// Every allocated channel, in strictly increasing id order.
pub const CHANNEL_TABLE: &[ChannelEntry] = &[ECHO, FS, LIFECYCLE];

// An invalid table does not build (spec §3 criteria 2-5).
const _: () = assert!(validate_table(CHANNEL_TABLE).is_ok());

/// A defect that makes a channel table invalid (spec §4.2).
///
/// The real table is checked at compile time, so this surfaces only in
/// `cargo build` or a fixture test, never at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableDefect {
    /// An entry uses id `0`, which [`HANDSHAKE_CHANNEL`] reserves for `HELLO`.
    ReservedId,
    /// Ids do not rise strictly in declaration order.
    NotIncreasing,
    /// Two entries share this id.
    DuplicateId(u16),
    /// A name breaks `[a-z][a-z0-9-]{0,31}`, has an `el` hyphen segment, or
    /// contains `electron`.
    InvalidName,
    /// Two entries share a name.
    DuplicateName,
    /// A guarded entry names no capability.
    EmptyCapabilityList,
}

impl core::fmt::Display for TableDefect {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let defect = match self {
            Self::ReservedId => "an entry uses id 0, which is reserved for HELLO",
            Self::NotIncreasing => "entry ids do not rise strictly in declaration order",
            Self::DuplicateId(id) => {
                return write!(
                    f,
                    "invalid kipc channel table: id {id} is allocated twice. Give the newest \
                     entry the current maximum id plus one; never reuse an id \
                     (docs/specs/gh508-kipc-channel-table.md §4.4)."
                );
            }
            Self::InvalidName => {
                "an entry name breaks [a-z][a-z0-9-]{0,31} or names an Electron family"
            }
            Self::DuplicateName => "two entries share a name",
            Self::EmptyCapabilityList => {
                "a guarded entry names no capability; reference keld_guard::capability \
                 constants or declare Authority::HostInternal"
            }
        };
        write!(
            f,
            "invalid kipc channel table: {defect}. Append new entries at the end with the \
             current maximum id plus one and never edit an existing entry \
             (docs/specs/gh508-kipc-channel-table.md §4.4)."
        )
    }
}

impl core::error::Error for TableDefect {}

/// Validates one table: ids nonzero, unique and strictly increasing; names
/// unique and well formed; every guarded entry names a capability.
///
/// It sees only the given entries, so it cannot detect renumbering; the
/// committed allocation baseline does (spec §3 criterion 3a).
///
/// # Errors
///
/// Returns the first [`TableDefect`] in declaration order.
pub const fn validate_table(entries: &[ChannelEntry]) -> Result<(), TableDefect> {
    let mut index = 0;
    while index < entries.len() {
        let entry = &entries[index];
        if entry.id.0 == HANDSHAKE_CHANNEL.0 {
            return Err(TableDefect::ReservedId);
        }
        if !valid_name(entry.name.as_bytes()) {
            return Err(TableDefect::InvalidName);
        }
        if let Authority::Guarded(names) = entry.authority
            && names.is_empty()
        {
            return Err(TableDefect::EmptyCapabilityList);
        }
        let mut earlier = 0;
        while earlier < index {
            if entries[earlier].id.0 == entry.id.0 {
                return Err(TableDefect::DuplicateId(entry.id.0));
            }
            if bytes_equal(entries[earlier].name.as_bytes(), entry.name.as_bytes()) {
                return Err(TableDefect::DuplicateName);
            }
            earlier += 1;
        }
        if index > 0 && entries[index - 1].id.0 >= entry.id.0 {
            return Err(TableDefect::NotIncreasing);
        }
        index += 1;
    }
    Ok(())
}

/// The allocated entry for `id`, or `None` for an unallocated id (including
/// [`HANDSHAKE_CHANNEL`]).
#[must_use]
pub const fn entry(id: ChannelId) -> Option<&'static ChannelEntry> {
    let mut index = 0;
    while index < CHANNEL_TABLE.len() {
        if CHANNEL_TABLE[index].id.0 == id.0 {
            return Some(&CHANNEL_TABLE[index]);
        }
        index += 1;
    }
    None
}

const fn valid_name(name: &[u8]) -> bool {
    if name.is_empty() || name.len() > 32 || !name[0].is_ascii_lowercase() {
        return false;
    }
    let mut index = 0;
    let mut segment = 0;
    while index <= name.len() {
        if index == name.len() || name[index] == b'-' {
            if index - segment == 2 && name[segment] == b'e' && name[segment + 1] == b'l' {
                return false;
            }
            segment = index + 1;
        } else if !(name[index].is_ascii_lowercase() || name[index].is_ascii_digit()) {
            return false;
        }
        index += 1;
    }
    !contains(name, b"electron")
}

const fn bytes_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    let mut start = 0;
    while start + needle.len() <= haystack.len() {
        let mut offset = 0;
        while offset < needle.len() && haystack[start + offset] == needle[offset] {
            offset += 1;
        }
        if offset == needle.len() {
            return true;
        }
        start += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        Authority, CHANNEL_TABLE, ChannelEntry, ECHO, FS, HANDSHAKE_CHANNEL, LIFECYCLE,
        ReceiveClass, TableDefect, entry, validate_table,
    };
    use crate::frame::{ChannelId, CorrelationId, FrameHeader, FrameKind};
    use crate::link::read_validated_frame;
    use crate::receive::{
        ReceivePolicy, validate_primary_app_header,
        validate_primary_app_header_with_privileged_call, validate_received_header,
    };
    use crate::{IpcError, MAX_FRAME_LEN, PROTOCOL_VERSION};

    /// The committed allocation baseline (spec §3 criterion 3a).
    const BASELINE: &str = include_str!("../channel_allocations.txt");

    const fn fixture(name: &'static str, id: u16) -> ChannelEntry {
        ChannelEntry::new(name, id, ReceiveClass::HostCall, Authority::HostInternal)
    }

    /// Why a table does not match the committed allocation baseline.
    #[derive(Debug, PartialEq, Eq)]
    enum AllocationDefect<'a> {
        /// A baseline line is not exactly `name id` with a decimal `u16` id.
        Malformed { line: usize },
        /// A baseline name is absent from the table.
        Removed { name: &'a str, id: u16 },
        /// A baseline name has another id, or another position, in the table.
        Changed {
            name: &'a str,
            baseline: u16,
            table: u16,
        },
        /// A table entry beyond the baseline: the appending change must also
        /// append its line.
        Unrecorded { name: &'static str, id: u16 },
    }

    fn parse_baseline(baseline: &str) -> Result<Vec<(&str, u16)>, AllocationDefect<'_>> {
        baseline
            .lines()
            .enumerate()
            .map(|(index, line)| {
                let malformed = AllocationDefect::Malformed { line: index + 1 };
                let (name, id) = line.split_once(' ').ok_or(malformed)?;
                let decimal = !id.is_empty()
                    && id.bytes().all(|byte| byte.is_ascii_digit())
                    && (id == "0" || !id.starts_with('0'));
                if name.is_empty() || !decimal {
                    return Err(AllocationDefect::Malformed { line: index + 1 });
                }
                let id = id
                    .parse()
                    .map_err(|_| AllocationDefect::Malformed { line: index + 1 })?;
                Ok((name, id))
            })
            .collect()
    }

    /// Every baseline pair is in `table` with the same id at the same
    /// position, and `table` has no entry the baseline does not list.
    fn check_allocations<'a>(
        table: &[ChannelEntry],
        baseline: &'a str,
    ) -> Result<(), AllocationDefect<'a>> {
        let allocations = parse_baseline(baseline)?;
        for (position, &(name, id)) in allocations.iter().enumerate() {
            let Some(found) = table.iter().position(|entry| entry.name() == name) else {
                return Err(AllocationDefect::Removed { name, id });
            };
            let actual = table[found].id().0;
            if actual != id || found != position {
                return Err(AllocationDefect::Changed {
                    name,
                    baseline: id,
                    table: actual,
                });
            }
        }
        if let Some(extra) = table.get(allocations.len()) {
            return Err(AllocationDefect::Unrecorded {
                name: extra.name(),
                id: extra.id().0,
            });
        }
        Ok(())
    }

    /// Criterion 1: the table lists exactly the three live channels, with the
    /// spec §4.2 classes and authorities.
    #[test]
    fn table_lists_exactly_echo_fs_and_lifecycle() {
        let listed: Vec<_> = CHANNEL_TABLE
            .iter()
            .map(|entry| (entry.name(), entry.id().0, entry.class(), entry.authority()))
            .collect();
        assert_eq!(
            listed,
            [
                ("echo", 1, ReceiveClass::HostCall, Authority::HostInternal),
                (
                    "fs",
                    2,
                    ReceiveClass::GuardedCall,
                    Authority::Guarded(&["fs.read", "fs.write"]),
                ),
                (
                    "lifecycle",
                    3,
                    ReceiveClass::HostCallWithEvents,
                    Authority::HostInternal,
                ),
            ]
        );
        assert_eq!(validate_table(CHANNEL_TABLE), Ok(()));
        assert_eq!(HANDSHAKE_CHANNEL, ChannelId(0));
    }

    /// Criterion 2: a duplicate id names the id.
    #[test]
    fn duplicate_id_is_rejected_with_the_id() {
        let table = [ECHO, FS, fixture("probe", 2)];
        assert_eq!(validate_table(&table), Err(TableDefect::DuplicateId(2)));
        assert!(
            TableDefect::DuplicateId(2)
                .to_string()
                .contains("id 2 is allocated twice")
        );
    }

    /// Criterion 3: id 0 is reserved and ids rise strictly in declaration order.
    #[test]
    fn reserved_or_unordered_ids_are_rejected() {
        assert_eq!(
            validate_table(&[fixture("probe", 0)]),
            Err(TableDefect::ReservedId)
        );
        assert_eq!(
            validate_table(&[FS, ECHO, LIFECYCLE]),
            Err(TableDefect::NotIncreasing)
        );
        assert_eq!(
            validate_table(&[ECHO, LIFECYCLE, FS]),
            Err(TableDefect::NotIncreasing)
        );
        assert_eq!(
            validate_table(&[ECHO, FS, LIFECYCLE, fixture("probe", 9)]),
            Ok(())
        );
        assert_eq!(validate_table(&[]), Ok(()));
    }

    /// Criterion 4: every entry is host-internal or names a capability.
    #[test]
    fn guarded_entry_without_capability_is_rejected() {
        let empty = ChannelEntry::new(
            "probe",
            4,
            ReceiveClass::GuardedCall,
            Authority::Guarded(&[]),
        );
        assert_eq!(
            validate_table(&[ECHO, FS, LIFECYCLE, empty]),
            Err(TableDefect::EmptyCapabilityList)
        );
        let named = ChannelEntry::new(
            "probe",
            4,
            ReceiveClass::GuardedCall,
            Authority::Guarded(&["dialog"]),
        );
        assert_eq!(validate_table(&[ECHO, FS, LIFECYCLE, named]), Ok(()));
    }

    /// Criterion 5: the name grammar admits no Electron family and no range.
    #[test]
    fn names_follow_the_grammar_and_never_name_electron() {
        let thirty_two = "a234567890123456789012345678901b";
        let thirty_three = "a2345678901234567890123456789012c";
        for invalid in [
            "el-ipc",
            "electron",
            "ipcMain",
            "a:b",
            "el-reserved",
            "x-el",
            "my-electron-app",
            "el",
            "",
            "4ever",
            "-a",
            "a_b",
            "Echo",
            thirty_three,
        ] {
            assert_eq!(
                validate_table(&[fixture(invalid, 4)]),
                Err(TableDefect::InvalidName),
                "{invalid:?}"
            );
        }
        for valid in [
            "a",
            "echo",
            "compat-control",
            "window-state",
            "elk",
            "del",
            "el2",
            thirty_two,
        ] {
            assert_eq!(validate_table(&[fixture(valid, 4)]), Ok(()), "{valid:?}");
        }
        assert_eq!(
            validate_table(&[ECHO, fixture("echo", 2)]),
            Err(TableDefect::DuplicateName)
        );
    }

    /// Criterion 3a: the real table matches the committed baseline.
    #[test]
    fn real_table_matches_the_committed_allocation_baseline() {
        assert_eq!(BASELINE, "echo 1\nfs 2\nlifecycle 3\n");
        assert_eq!(check_allocations(CHANNEL_TABLE, BASELINE), Ok(()));
    }

    /// Criterion 3a negative controls: a rising renumber passes
    /// `validate_table` but not the baseline; so do removal and an
    /// unrecorded append.
    #[test]
    fn baseline_rejects_renumbered_removed_moved_and_unrecorded_entries() {
        let baseline = "echo 1\nfs 2\nlifecycle 3\nprobe 4\n";
        let appended = [ECHO, FS, LIFECYCLE, fixture("probe", 4)];
        assert_eq!(check_allocations(&appended, baseline), Ok(()));

        let renumbered = [ECHO, FS, LIFECYCLE, fixture("probe", 5)];
        assert_eq!(validate_table(&renumbered), Ok(()));
        assert_eq!(
            check_allocations(&renumbered, baseline),
            Err(AllocationDefect::Changed {
                name: "probe",
                baseline: 4,
                table: 5
            })
        );

        assert_eq!(
            check_allocations(&[ECHO, LIFECYCLE, fixture("probe", 4)], baseline),
            Err(AllocationDefect::Removed { name: "fs", id: 2 })
        );
        // Same id, moved position: an entry inserted before an allocated one.
        assert_eq!(
            check_allocations(
                &[fixture("a", 1), fixture("c", 2), fixture("b", 3)],
                "a 1\nb 3\n"
            ),
            Err(AllocationDefect::Changed {
                name: "b",
                baseline: 3,
                table: 3
            })
        );
        assert_eq!(
            check_allocations(&appended, BASELINE),
            Err(AllocationDefect::Unrecorded {
                name: "probe",
                id: 4
            })
        );
        for (malformed, line) in [
            ("echo 1\n\nfs 2\n", 2),
            ("echo  1\n", 1),
            ("echo 0x1\n", 1),
            ("echo 01\n", 1),
            ("echo\n", 1),
            ("echo 1 \n", 1),
            ("echo 70000\n", 1),
        ] {
            assert_eq!(
                check_allocations(CHANNEL_TABLE, malformed),
                Err(AllocationDefect::Malformed { line }),
                "{malformed:?}"
            );
        }
    }

    /// Criterion 11: the public constants are the table entries, and keep the
    /// wire values the golden vectors pin.
    #[test]
    fn public_channel_constants_are_the_table_entries() {
        assert_eq!(crate::ECHO_CHANNEL, ECHO.id());
        assert_eq!(crate::LIFECYCLE_CHANNEL, LIFECYCLE.id());
        assert_eq!(
            (crate::ECHO_CHANNEL.0, FS.id().0, crate::LIFECYCLE_CHANNEL.0),
            (1, 2, 3)
        );
        assert_eq!(PROTOCOL_VERSION, 2);
    }

    /// The id lookup resolves only allocated ids.
    #[test]
    fn entry_lookup_resolves_only_allocated_ids() {
        for allocated in CHANNEL_TABLE {
            assert_eq!(entry(allocated.id()), Some(allocated));
        }
        for unallocated in [0, 4, 9, u16::MAX] {
            assert_eq!(entry(ChannelId(unallocated)), None, "{unallocated}");
        }
    }

    /// Criterion 10: only a `GuardedCall` entry admits the privileged policy.
    #[test]
    fn privileged_policy_requires_a_guarded_call_entry() {
        let fs =
            ReceivePolicy::privileged_call_receiver(&FS).expect("fs is a guarded CALL channel");
        assert_eq!(fs.channel, FS.id());
        for host_internal in [&ECHO, &LIFECYCLE] {
            let Err(IpcError::Protocol { detail }) =
                ReceivePolicy::privileged_call_receiver(host_internal)
            else {
                panic!("{host_internal:?} must not admit the privileged policy");
            };
            assert_eq!(detail, "channel class does not admit this policy");
        }
        let error = ReceivePolicy::privileged_call_receiver(&ECHO).expect_err("echo is HostCall");
        assert!(error.to_string().starts_with("KELD-IPC-005"), "{error}");
    }

    fn header(kind: FrameKind, channel: u16, corr: u32, len: u32) -> FrameHeader {
        FrameHeader {
            kind,
            flags: 0,
            channel: ChannelId(channel),
            corr: CorrelationId(corr),
            len,
        }
    }

    fn assert_wrong_channel(result: &Result<crate::ValidatedFrameHeader, IpcError>, label: &str) {
        let Err(IpcError::Protocol { detail }) = result else {
            panic!("{label}: an unallocated channel must be KELD-IPC-005, got {result:?}");
        };
        assert_eq!(*detail, "wrong channel for the session policy", "{label}");
    }

    /// Criterion 10: a frame on unallocated id 4, otherwise exactly what each
    /// live policy admits, is `KELD-IPC-005` from the header alone. The reader
    /// rejects it with no payload bytes present, so nothing was read or
    /// allocated for the payload.
    #[test]
    fn unallocated_channel_is_rejected_by_every_live_policy_before_payload() {
        let unallocated = 4;
        assert_eq!(entry(ChannelId(unallocated)), None);
        let corr = CorrelationId(7);
        let fs = ReceivePolicy::privileged_call_receiver(&FS).expect("fs policy");
        let live = [
            (
                "server-pre-auth-hello",
                ReceivePolicy::server_pre_auth_hello(),
                FrameKind::Hello,
                0,
                32,
            ),
            (
                "client-await-hello",
                ReceivePolicy::client_await_hello(),
                FrameKind::Hello,
                0,
                32,
            ),
            (
                "echo-receiver",
                ReceivePolicy::echo_receiver(),
                FrameKind::Call,
                7,
                4,
            ),
            (
                "echo-reply-waiter",
                ReceivePolicy::echo_reply_waiter(corr),
                FrameKind::Reply,
                7,
                4,
            ),
            (
                "lifecycle-receiver",
                ReceivePolicy::lifecycle_receiver(),
                FrameKind::Call,
                7,
                4,
            ),
            (
                "lifecycle-event-receiver",
                ReceivePolicy::lifecycle_event_receiver(),
                FrameKind::Event,
                0,
                4,
            ),
            (
                "lifecycle-reply-waiter",
                ReceivePolicy::lifecycle_reply_waiter(corr),
                FrameKind::Reply,
                7,
                4,
            ),
            (
                "primary-app-receiver",
                ReceivePolicy::primary_app_receiver(),
                FrameKind::Call,
                7,
                4,
            ),
            (
                "primary-echo-reply-waiter",
                ReceivePolicy::primary_echo_reply_waiter(corr),
                FrameKind::Reply,
                7,
                4,
            ),
            ("privileged-fs-receiver", fs, FrameKind::Call, 7, 4),
        ];
        for (label, policy, kind, frame_corr, len) in live {
            // Prerequisite: the same header on the policy's own channel admits.
            validate_received_header(&policy, header(kind, policy.channel.0, frame_corr, len))
                .unwrap_or_else(|error| panic!("{label}: own-channel control rejected: {error}"));
            assert_wrong_channel(
                &validate_received_header(&policy, header(kind, unallocated, frame_corr, len)),
                label,
            );
            let declared = u32::try_from(MAX_FRAME_LEN).expect("cap fits u32");
            let wire = header(
                kind,
                unallocated,
                frame_corr,
                if len == 32 { 32 } else { declared },
            );
            let result = read_validated_frame(&mut Cursor::new(wire.encode()), &policy);
            let Err(IpcError::Protocol { detail }) = result else {
                panic!("{label}: reader must reject before reading the payload, got {result:?}");
            };
            assert_eq!(detail, "wrong channel for the session policy", "{label}");
        }
        let call = header(FrameKind::Call, unallocated, 7, 4);
        assert_wrong_channel(&validate_primary_app_header(None, call), "primary dispatch");
        assert_wrong_channel(
            &validate_primary_app_header_with_privileged_call(None, Some(&FS), call),
            "primary dispatch with the fs channel selected",
        );
    }
}
