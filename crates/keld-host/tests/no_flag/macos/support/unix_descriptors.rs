use crate::support::process::process_state;
use std::process::Command;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;

/// One Unix-domain descriptor: its number, the kernel socket behind it, and the
/// name `lsof` prints for it.
pub(crate) struct UnixDescriptor {
    pub(crate) descriptor: String,
    /// `lsof`'s `d` field: the address of the socket object itself. Two live
    /// sockets never share one, and `exec` preserves it, so this is what
    /// identifies a descriptor across processes.
    pub(crate) socket: String,
    /// `lsof`'s `n` field: the bound `sun_path` for a listener and for each peer
    /// it accepted, `->0x<address>` naming the *peer* socket for a connected or
    /// paired endpoint, or `->(none)` when there is neither. Human-readable, and
    /// deliberately not used to decide identity.
    pub(crate) identity: String,
}

/// Whether `lsof` can still see any open descriptor at all on `pid`.
///
/// Not restricted to Unix sockets, and not parsed: `cwd` and the mapped executable
/// count, so any process whose descriptor table exists answers yes. That makes this
/// the corroboration an empty Unix census needs — see [`unix_descriptors`].
pub(crate) fn process_has_open_descriptors(pid: u32) -> bool {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-p", &pid.to_string(), "-Ff"])
        .output()
        .expect("enumerate open descriptors");
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.starts_with('f'))
}

/// Every Unix-domain descriptor open on `pid`.
///
/// `lsof` exits 1 when it cannot report on the pid at all. Measured on macOS `lsof`
/// 4.91, that covers a pid that never existed, a zombie, and a pid owned by another
/// user — the last with empty stderr, so an uninspectable process cannot masquerade
/// as a clean one.
///
/// Exiting 0 with no rows does *not* mean the same thing twice over, and that is the
/// trap this function has to close. A live process owning no Unix socket reports it
/// that way, and so does a process in the window after `exit` but before it becomes
/// a zombie, when its descriptor table is already gone but its proc entry is not.
/// Zero is the *passing* value for the CLI, so the two must not be conflated:
/// measured, an unguarded census returned an empty result in 32 of 300 racing trials
/// for a target that provably owned a listener it had opened itself — a silent false
/// pass of exactly the assertion this oracle exists to make.
///
/// So an empty census has to prove the process still has a table to be empty of.
/// [`process_has_open_descriptors`] is that proof, and it is not circular: it asks a
/// wider question than the census does, and `cwd` and the mapped executable answer
/// it for any live process. Measured across 83 racing trials that produced an empty
/// Unix census, it rejected all 83, and it stayed silent for live processes both with
/// and without Unix sockets. The race cannot be reproduced deterministically, so
/// [`unix_descriptor_census_requires_a_live_descriptor_table`] pins the corroboration
/// itself, and
/// [`empty_unix_census_of_a_process_without_a_descriptor_table_is_rejected`] pins the
/// census applying it. The exiting window still cannot be entered on demand, so a
/// reaped pid with an empty report stands in for it: same absent table, same
/// earlier `lsof` exit-1 assert, not this empty-result path.
pub(crate) fn unix_descriptors(pid: u32) -> Vec<UnixDescriptor> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-a", "-p", &pid.to_string(), "-U", "-Fdfn"])
        .output()
        .expect("enumerate Unix descriptors");
    assert!(
        output.status.success(),
        "lsof cannot report Unix descriptors for {pid} in state {}: {output:?}",
        process_state(pid)
    );
    let rendered = String::from_utf8(output.stdout).expect("lsof output UTF-8");
    unix_descriptors_reported(pid, &rendered)
}

/// The descriptors `lsof` reported for `pid` in `rendered`, or a panic if what it
/// reported cannot be trusted.
///
/// Split from [`unix_descriptors`] at the process boundary so every decision this
/// census makes is reachable from a test that supplies the output itself. Without
/// that seam the empty-result reject below was unpinned: deleting it left every
/// census test green, because the only input that distinguishes teardown from a
/// clean process is a pid whose table is gone, and a census of one cannot be
/// arranged on demand.
pub(crate) fn unix_descriptors_reported(pid: u32, rendered: &str) -> Vec<UnixDescriptor> {
    let mut descriptor = None;
    let mut socket = None;
    let mut descriptors = Vec::new();
    for line in rendered.lines() {
        if let Some(fd) = line.strip_prefix('f') {
            // A record `lsof` starts and never names would otherwise be dropped
            // without a word, and dropping a target's descriptor is the direction
            // that passes. No real `lsof` output has done this; it is not left to
            // chance because the cost of being wrong is silence.
            assert!(
                descriptor.is_none(),
                "lsof left descriptor {descriptor:?} of {pid} unnamed: {rendered:?}"
            );
            descriptor = Some(fd.to_owned());
            socket = None;
        } else if let Some(address) = line.strip_prefix('d') {
            // Two empty addresses would compare equal and excuse each other.
            assert!(
                address.starts_with("0x"),
                "lsof gave {pid} a socket address that is not one: {address:?}"
            );
            socket = Some(address.to_owned());
        } else if let Some(name) = line.strip_prefix('n') {
            descriptors.push(UnixDescriptor {
                descriptor: descriptor
                    .take()
                    .expect("lsof numbers a descriptor it names"),
                socket: socket.take().expect("lsof addresses a descriptor it names"),
                identity: name.to_owned(),
            });
        }
    }
    assert!(
        descriptor.is_none(),
        "lsof left the last descriptor of {pid} unnamed: {rendered:?}"
    );
    if descriptors.is_empty() {
        assert!(
            process_has_open_descriptors(pid),
            "{pid} has no descriptor table in state {}, so an empty Unix census is \
             teardown rather than evidence",
            process_state(pid)
        );
    }
    descriptors
}

/// One printable name per Unix-domain descriptor open on `pid`.
pub(crate) fn unix_socket_identities(pid: u32) -> Vec<String> {
    unix_descriptors(pid)
        .into_iter()
        .map(|descriptor| descriptor.identity)
        .collect()
}

/// Serialises the census fixtures against each other.
///
/// macOS `std` has no atomic `SOCK_CLOEXEC`: it creates a socket and then sets
/// `FD_CLOEXEC` in a second call, so a `posix_spawn` on another thread inside that
/// window captures the descriptor by number. A socket that leaks into another test's
/// child that way is charged to that child as soon as its real owner closes it,
/// because the census can no longer attribute it to the harness. Measured on these
/// tests sharing one process, that is one failure in 300 runs, and the panic names a
/// socket from a different fixture's temporary directory.
///
/// The mandated gate runs every test in its own process, where the interleaving
/// cannot happen at all; this lock buys a shared-process run the same isolation. It
/// is uncontended under the gate. Poisoning is ignored deliberately: a panicking
/// census test has already failed the run, and refusing the lock afterwards would
/// replace that failure with a less informative one.
static CENSUS_ISOLATION: Mutex<()> = Mutex::new(());

pub(crate) fn census_isolation() -> MutexGuard<'static, ()> {
    CENSUS_ISOLATION
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Every Unix-domain socket open on this harness, by kernel address.
pub(crate) fn harness_unix_sockets() -> Vec<String> {
    unix_descriptors(std::process::id())
        .into_iter()
        .map(|descriptor| descriptor.socket)
        .collect()
}

/// The Unix-domain descriptors open on `pid` that it did not inherit from this
/// harness, named for a human.
///
/// A raw `lsof -U` count is not an oracle for app-link ownership. A Unix socket
/// the launching shell left without `FD_CLOEXEC` reaches every process under test
/// through `exec`, so the raw count charges the CLI for descriptors it never
/// opened (KEL-222). Subtracting the harness's own table removes them.
///
/// Keyed on the socket object's own address, never on the printable name, because
/// names are not identities. Measured on macOS `lsof` 4.91: every socket that is
/// neither bound nor connected is named `->(none)`, and a socket keeps reporting
/// its `sun_path` after that path is unlinked, so a replacement bound to the same
/// path reports the same name. Either would let a name-keyed subtraction excuse a
/// descriptor `pid` opened itself — the silent false pass this oracle exists to
/// prevent. An address is unique among live sockets and `exec` preserves it, so a
/// match means the same kernel object.
///
/// One harness descriptor excuses one target descriptor, not every copy of it.
/// Counting is not a refinement here, it is the whole choice: a target holding two
/// descriptors on a socket the harness holds once is the same picture to `lsof`
/// whether the target `dup`ed something it inherited or opened the socket itself
/// and passed a copy up. Both are constructible, and neither happens in this
/// product — nothing here passes descriptors over `SCM_RIGHTS`, and a shell leak
/// reaches harness and child alike so their counts match and nothing is charged.
/// Given an ambiguous picture the count rule takes the reading that fails loudly:
/// excusing every copy silently absolves a socket the target may have opened, and
/// that silence is the defect KEL-222 exists to remove. It costs a loud
/// over-charge in the `dup` reading, which is investigable.
///
/// The harness table is read on both sides of the target census, and only what
/// both readings agree on can excuse anything. macOS reuses freed socket addresses,
/// at a rate that varies far too much between sessions to quote as a figure —
/// repeated measurements of the same 90 stream-pair ends have returned anywhere
/// from 15 to 74 of them — so reuse has to be assumed rather than treated as rare.
/// One reading would then excuse a socket `pid` opened at an address whose previous
/// tenant the harness had just closed.
///
/// The cost is that a harness close between the two readings over-charges a
/// genuinely inherited descriptor. That is not hypothetical, and an earlier version
/// of this comment was wrong to say no thread here closes a Unix socket: when these
/// tests share one process they close each other's, and it cost one failure in 300
/// runs until [`CENSUS_ISOLATION`] serialised them. It is still the loud direction,
/// which is why the reading stays doubled: the alternative excuses a socket the
/// target may have opened and says nothing. Neither behaviour can be pinned without
/// a test whose timing decides the result, so both are argued from measurement.
///
/// What survives is what `pid` did not inherit *from this harness*, which is why
/// the name says that and not "opened itself". The two coincide only for a direct
/// child, whose whole ancestry is the harness: that is the CLI, and the CLI owning
/// none is the assertion this oracle exists for. For a deeper descendant they
/// diverge — Bun is the guardian's child, so an intermediate ancestor can pass
/// down a socket the harness never held, and a grandchild handed a `socketpair`
/// that way is charged both of its ends. That over-charges, which is safe for "the
/// CLI owns none" and unsafe for the positive "this process owns one" assertions,
/// since an injected descriptor satisfies them. Closing it needs those assertions
/// to name the app link instead of counting anything, and is recorded on KEL-222
/// rather than half-done here.
pub(crate) fn unix_sockets_not_inherited_from_harness(pid: u32) -> Vec<String> {
    let before = harness_unix_sockets();
    let observed = unix_descriptors(pid);
    let mut after = harness_unix_sockets();
    // One entry per socket both readings agree the harness held, and no more
    // copies of it than the second reading still shows.
    let mut inheritable = Vec::new();
    for address in before {
        if let Some(index) = after.iter().position(|held| *held == address) {
            after.swap_remove(index);
            inheritable.push(address);
        }
    }
    let mut charged = Vec::new();
    for descriptor in observed {
        match inheritable
            .iter()
            .position(|held| *held == descriptor.socket)
        {
            Some(index) => {
                inheritable.swap_remove(index);
            }
            None => charged.push(descriptor.identity),
        }
    }
    charged
}
