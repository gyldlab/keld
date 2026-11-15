//! Own-process REG/DIR object census without opening another target descriptor.
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::process::Command;

type SensorResult<T> = Result<T, &'static str>;

fn decimal(value: &str) -> SensorResult<u64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("nondecimal or empty identity");
    }
    value.parse().map_err(|_| "decimal identity overflow")
}

fn device(value: &str) -> SensorResult<u64> {
    let hex = value
        .strip_prefix("0x")
        .ok_or("missing device hex prefix")?;
    if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("nonhex or empty device");
    }
    u64::from_str_radix(hex, 16).map_err(|_| "device overflow")
}

fn known_type(value: &str) -> bool {
    // Installed lsof4.91 TYPE table; unknown/NOFD reports are unavailable.
    matches!(
        value,
        "REG"
            | "DIR"
            | "CHR"
            | "BLK"
            | "FIFO"
            | "PIPE"
            | "KQUEUE"
            | "IPv4"
            | "IPv6"
            | "unix"
            | "inet"
            | "rte"
            | "sock"
            | "PSXSEM"
            | "PSXSHM"
            | "LINK"
    )
}

fn record_matches(
    fields: &[&str],
    identity: (u64, u64),
    expected_kind: &str,
    descriptors: &mut BTreeSet<u64>,
) -> SensorResult<bool> {
    let name = fields[0]
        .strip_prefix('f')
        .ok_or("file set lacks FD start")?;
    let number = if name
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        let number = decimal(name)?;
        if !descriptors.insert(number) {
            return Err("duplicate numeric descriptor");
        }
        Some(number)
    } else if matches!(name, "cwd" | "rtd" | "txt" | "mem" | "mmap" | "pd" | "ltx") {
        None
    } else {
        return Err("unknown or unavailable descriptor");
    };
    let (mut kind, mut dev, mut inode) = (None, None, None);
    for field in &fields[1..] {
        let (tag, value) = field.split_at(1);
        match tag {
            "t" if kind.is_none() && known_type(value) => kind = Some(value),
            "D" if dev.is_none() => dev = Some(device(value)?),
            "i" if inode.is_none() => inode = Some(decimal(value)?),
            _ => return Err("unknown or repeated file field"),
        }
    }
    let kind = kind.ok_or("descriptor lacks type")?;
    if kind == expected_kind {
        let observed = (
            dev.ok_or("selected object lacks device")?,
            inode.ok_or("selected object lacks inode")?,
        );
        return Ok(number.is_some() && observed == identity);
    }
    Ok(false)
}

fn parse_object_count(
    pid: u32,
    identity: (u64, u64),
    expected_kind: &str,
    bytes: &[u8],
) -> SensorResult<usize> {
    if bytes.is_empty() || !bytes.ends_with(b"\0\n") {
        return Err("empty or truncated census");
    }
    let mut process_seen = false;
    let mut descriptors = BTreeSet::new();
    let mut count = 0;
    for set in bytes.split_inclusive(|byte| *byte == b'\n') {
        let fields = set.strip_suffix(b"\0\n").ok_or("bad set terminator")?;
        let fields: Vec<_> = fields
            .split(|byte| *byte == 0)
            .map(|field| {
                let text = std::str::from_utf8(field).map_err(|_| "non-ASCII field")?;
                if text.len() < 2 || !text.is_ascii() {
                    return Err("empty or non-ASCII field");
                }
                Ok(text)
            })
            .collect::<SensorResult<_>>()?;
        let first = fields[0];
        if let Some(process) = first.strip_prefix('p') {
            if process_seen || fields.len() != 1 || decimal(process)? != u64::from(pid) {
                return Err("wrong or repeated process header");
            }
            process_seen = true;
        } else if process_seen {
            count += usize::from(record_matches(
                &fields,
                identity,
                expected_kind,
                &mut descriptors,
            )?);
        } else {
            return Err("file set precedes own process");
        }
    }
    if !process_seen || descriptors.is_empty() {
        return Err("own numeric descriptor table absent");
    }
    Ok(count)
}

#[cfg(not(keld_core_native_renderer_main))]
fn parse_count(pid: u32, identity: (u64, u64), bytes: &[u8]) -> SensorResult<usize> {
    parse_object_count(pid, identity, "REG", bytes)
}

#[cfg(not(keld_core_native_renderer_main))]
fn reported_count(
    success: bool,
    stderr: &[u8],
    stdout: &[u8],
    pid: u32,
    identity: (u64, u64),
) -> SensorResult<usize> {
    if !success || !stderr.is_empty() {
        return Err("lsof status or stderr makes census unavailable");
    }
    parse_count(pid, identity, stdout)
}

/// Reports numeric REG descriptors for the exact target device and inode.
pub(in crate::app_session) fn object_handle_count(target: &Path) -> usize {
    census_object(target, "REG")
}

pub(in crate::app_session) fn directory_handle_count(target: &Path) -> usize {
    assert!(fs::metadata(target).expect("directory identity").is_dir());
    census_object(target, "DIR")
}

fn census_object(target: &Path, expected_kind: &str) -> usize {
    let identity = fs::metadata(target).expect("target object identity");
    let pid = std::process::id();
    // The existing exact-self45s owned-group watchdog bounds query and capture.
    // lsof reads the table; it does not open or retain a target-copy descriptor.
    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-a", "-p", &pid.to_string(), "-F0pftDi"])
        .output()
        .expect("own-process lsof census unavailable");
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "own object census unavailable: output={output:?}"
    );
    parse_object_count(
        pid,
        (identity.dev(), identity.ino()),
        expected_kind,
        &output.stdout,
    )
    .unwrap_or_else(|fault| panic!("own object census unavailable: {fault}; output={output:?}"))
}

/// Calibrates distinct READ and WRITE controls; closes both before native entry.
pub(in crate::app_session) fn calibrate_object_sensor(target: &Path) {
    for write_only in [false, true] {
        assert_eq!(object_handle_count(target), 0, "pre-call target handle");
        let deliberate = OpenOptions::new()
            .read(!write_only)
            .write(write_only)
            .open(target)
            .expect("deliberate access-mode sensor control");
        assert_eq!(object_handle_count(target), 1, "one actual control handle");
        let duplicate = deliberate.try_clone().expect("calibration-only clone");
        assert_eq!(object_handle_count(target), 2, "distinct duplicate handles");
        drop(deliberate);
        assert_eq!(object_handle_count(target), 1, "one control remains");
        drop(duplicate);
        assert_eq!(object_handle_count(target), 0, "all controls closed");
        println!("KELD_AC7_SENSOR_CALIBRATED write_only={write_only} counts=0,1,2,1,0");
    }
}

/// Closes every calibration handle before the broker prepares this object.
pub(in crate::app_session) fn calibrate_directory_sensor(target: &Path) {
    assert_eq!(
        directory_handle_count(target),
        0,
        "pre-prepare root baseline"
    );
    let first = File::open(target).expect("deliberate directory control");
    assert_eq!(
        directory_handle_count(target),
        1,
        "one numeric directory descriptor"
    );
    let duplicate = first
        .try_clone()
        .expect("distinct calibration directory duplicate");
    assert_eq!(
        directory_handle_count(target),
        2,
        "two numeric directory descriptors"
    );
    drop(first);
    assert_eq!(
        directory_handle_count(target),
        1,
        "one directory descriptor remains"
    );
    drop(duplicate);
    assert_eq!(
        directory_handle_count(target),
        0,
        "all directory controls closed"
    );
}

#[cfg(not(keld_core_native_renderer_main))]
#[cfg(test)]
mod tests {
    use super::{parse_count, parse_object_count, reported_count};

    #[test]
    fn directory_parser_requires_numeric_matching_object_identity() {
        let bytes =
            b"p4008\0\nf7\0tDIR\0D0x10\0i42\0\nf8\0tREG\0D0x10\0i42\0\nfcwd\0tDIR\0D0x10\0i42\0\n";
        assert_eq!(parse_object_count(4008, (16, 42), "DIR", bytes), Ok(1));
        assert_eq!(parse_count(4008, (16, 42), bytes), Ok(1));
        assert_eq!(parse_object_count(4008, (16, 43), "DIR", bytes), Ok(0));
        assert!(parse_object_count(4008, (16, 42), "DIR", b"p4008\0\nf7\0tDIR\0\n").is_err());
    }

    const PID: u32 = 4008;
    const IDENTITY: (u64, u64) = (16_777_230, 228_942_197);

    // These are byte-exact installed4.91 own-kernel captures. Both access-mode
    // traces have identical bytes at each phase; the shared prefix is unchanged.
    #[test]
    fn sensor_parser_replays_actual_read_write_kernel_captures() {
        const PREFIX: &[u8] = b"p4008\0\nfcwd\0tDIR\0D0x100000e\0i228916006\0\nftxt\0tREG\0D0x100000e\0i6091265\0\nftxt\0tREG\0D0x100000e\0i6091366\0\nftxt\0tREG\0D0x100000e\0i6091356\0\nftxt\0tREG\0D0x100000e\0i6091335\0\nftxt\0tREG\0D0x100000e\0i6091307\0\nftxt\0tREG\0D0x100000e\0i6091308\0\nftxt\0tREG\0D0x100000e\0i6091295\0\nftxt\0tREG\0D0x100000e\0i1152921500312573277\0\nftxt\0tREG\0D0x100000e\0i6091357\0\nftxt\0tREG\0D0x100000e\0i6091299\0\nftxt\0tREG\0D0x100000e\0i6091347\0\nftxt\0tREG\0D0x100000e\0i6091324\0\nftxt\0tREG\0D0x100000e\0i6091317\0\nftxt\0tREG\0D0x100000e\0i6091325\0\nftxt\0tREG\0D0x100000e\0i6091309\0\nftxt\0tREG\0D0x100000e\0i6094010\0\nftxt\0tREG\0D0x100000e\0i6091281\0\n";
        let phases: &[(&[u8], usize)] = &[
            (b"f0\0tREG\0D0x100000e\0i228942195\0\nf1\0tPIPE\0\nf2\0tPIPE\0\nf3\0tPIPE\0\nf5\0tPIPE\0\n", 0),
            (b"f0\0tREG\0D0x100000e\0i228942195\0\nf1\0tPIPE\0\nf2\0tPIPE\0\nf3\0tREG\0D0x100000e\0i228942197\0\nf4\0tPIPE\0\nf6\0tPIPE\0\n", 1),
            (b"f0\0tREG\0D0x100000e\0i228942195\0\nf1\0tPIPE\0\nf2\0tPIPE\0\nf3\0tREG\0D0x100000e\0i228942197\0\nf4\0tREG\0D0x100000e\0i228942197\0\nf5\0tPIPE\0\nf7\0tPIPE\0\n", 2),
            (b"f0\0tREG\0D0x100000e\0i228942195\0\nf1\0tPIPE\0\nf2\0tPIPE\0\nf3\0tPIPE\0\nf4\0tREG\0D0x100000e\0i228942197\0\nf6\0tPIPE\0\n", 1),
            (b"f0\0tREG\0D0x100000e\0i228942195\0\nf1\0tPIPE\0\nf2\0tPIPE\0\nf3\0tPIPE\0\nf5\0tPIPE\0\n", 0),
        ];
        for mode in ["read-only", "write-only"] {
            for (tail, expected) in phases {
                let capture = [PREFIX, *tail].concat();
                assert_eq!(
                    parse_count(PID, IDENTITY, &capture),
                    Ok(*expected),
                    "{mode}"
                );
            }
        }
    }

    #[test]
    fn sensor_parser_accepts_unordered_fields_and_known_nonreg() {
        let bytes = b"p4008\0\nfcwd\0tDIR\0\nftxt\0tREG\0D0x100000e\0i228942197\0\nf0\0tPIPE\0\nf1\0tunix\0\nf2\0tIPv4\0\nf3\0i228942197\0tREG\0D0x100000e\0\n";
        assert_eq!(parse_count(PID, IDENTITY, bytes), Ok(1));
        assert_eq!(parse_count(PID, (16_777_230, 0), bytes), Ok(0));
        assert_eq!(parse_count(PID, (0, 228_942_197), bytes), Ok(0));
    }

    #[test]
    fn sensor_parser_rejects_unavailable_and_malformed_records() {
        let invalid: &[&[u8]] = &[
            b"",
            b"p4008\0\n",
            b"p4008\0\nfcwd\0tDIR\0\n",
            b"p4009\0\nf0\0tPIPE\0\n",
            b"f0\0tPIPE\0\n",
            b"p4008\0\np4008\0\nf0\0tPIPE\0\n",
            b"p4008\0\nf0\0tNOFD\0\n",
            b"p4008\0\nfNOFD\0tREG\0\n",
            b"p4008\0\nf0\0\n",
            b"p4008\0\nf0\0tMYSTERY\0\n",
            b"p4008\0\nf0r\0tPIPE\0\n",
            b"p4008\0\nf-1\0tPIPE\0\n",
            b"p4008\0\nf0\0tREG\0i1\0\n",
            b"p4008\0\nftxt\0tREG\0\nf0\0tPIPE\0\n",
            b"p4008\0\nf0\0t\xff\0\n",
            b"p4008\0\nf0\0tREG\0D0x1\0\n",
            b"p4008\0\nf0\0tREG\0D0xg\0i1\0\n",
            b"p4008\0\nf0\0tREG\0D1\0i1\0\n",
            b"p4008\0\nf0\0tREG\0D0x1\0i1x\0\n",
            b"p4008\0\nf0\0tREG\0D0x1\0i18446744073709551616\0\n",
            b"p4008\0\nf0\0tREG\0D0x10000000000000000\0i1\0\n",
            b"p4008\0\nf18446744073709551616\0tPIPE\0\n",
            b"p4008\0\nf0\0tPIPE\0tPIPE\0\n",
            b"p4008\0\nf0\0tREG\0D0x1\0D0x1\0i1\0\n",
            b"p4008\0\nf0\0tREG\0D0x1\0i1\0i1\0\n",
            b"p4008\0\nf0\0tPIPE\0\nf0\0tPIPE\0\n",
            b"p4008\0\nf0\0tPIPE\0xunexpected\0\n",
            b"p4008\0\nf0\0tPIPE\0\0\n",
            b"p4008\0\nf0\0tPIPE\n",
            b"p4008\0\nf0\0tREG\0D0x1\0i1\0",
            b"p4008\0\nf0\0tPIPE\0\n\n",
            b"p4008\0\nf0\0tPIPE\0\nf1\0tREG\0D0x1\0i1",
        ];
        for bytes in invalid {
            assert!(
                parse_count(PID, IDENTITY, bytes).is_err(),
                "accepted {bytes:?}"
            );
        }
    }

    #[test]
    fn sensor_parser_rejects_failed_tool_even_with_valid_table() {
        let bytes = b"p4008\0\nf0\0tPIPE\0\n";
        assert_eq!(reported_count(true, b"", bytes, PID, IDENTITY), Ok(0));
        assert!(reported_count(false, b"", bytes, PID, IDENTITY).is_err());
        assert!(reported_count(true, b"warning", bytes, PID, IDENTITY).is_err());
        assert!(reported_count(true, b"", b"", PID, IDENTITY).is_err());
    }
}
