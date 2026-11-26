use super::{
    CAPTURE_HEAD_BYTES, CAPTURE_MAX_RETAINED_BYTES, CAPTURE_RETENTION_BYTES, CapturedOutput,
};

/// KEL-225: pin the public ceiling *value*. Boundedness tests compare
/// retained length to [`CAPTURE_MAX_RETAINED_BYTES`] and size fixtures from
/// the same symbols, so scaling the constants used to stay green. This
/// literal cannot track the production definition.
#[test]
fn capture_max_retained_bytes_is_the_documented_host_ceiling() {
    assert_eq!(
        CAPTURE_MAX_RETAINED_BYTES, 327_680,
        "CAPTURE_MAX_RETAINED_BYTES is the host-memory budget for one \
         untrusted child stream: 64 KiB pinned head + 192 KiB sliding tail \
         + 64 KiB compaction slack (KEL-134). Changing the public CAPTURE_* \
         constants is a contract change and must be an explicit decision; \
         the self-referential `<= CAPTURE_MAX_RETAINED_BYTES` assertions \
         still pass if the ceiling is silently doubled"
    );
}

/// KEL-134 storage bound, isolated from process spawning.
///
/// This is the negative control for the retention cap: raising
/// `CAPTURE_RETENTION_BYTES`, or deleting the compaction branch in
/// `CapturedOutput::append`, makes the ceiling assertions below fail
/// deterministically on every OS row.
#[test]
fn bounded_capture_keeps_a_fixed_tail_and_counts_every_byte() {
    let mut captured = CapturedOutput::default();
    let chunk = "x".repeat(4096);
    let chunks = (CAPTURE_MAX_RETAINED_BYTES / chunk.len()) * 4;
    for _ in 0..chunks {
        captured.push_chunk(&chunk, chunk.len(), true);
        captured.push_chunk(&chunk, chunk.len(), false);
    }
    let written = chunk.len() * chunks;

    assert!(
        captured.stdout.len() <= CAPTURE_MAX_RETAINED_BYTES,
        "retained stdout must never exceed the documented ceiling: {} > {}",
        captured.stdout.len(),
        CAPTURE_MAX_RETAINED_BYTES
    );
    assert!(
        captured.stderr.len() <= CAPTURE_MAX_RETAINED_BYTES,
        "retained stderr must never exceed the documented ceiling: {} > {}",
        captured.stderr.len(),
        CAPTURE_MAX_RETAINED_BYTES
    );
    assert_eq!(
        captured.stdout_total_bytes, written,
        "every written byte must be counted even when it is not retained"
    );
    assert_eq!(captured.stderr_total_bytes, written);
    assert!(
        captured.stdout_dropped_bytes > 0,
        "dropping output must be disclosed, not silent"
    );
    assert_eq!(
        captured.stdout_total_bytes - captured.stdout_dropped_bytes
            + captured.stdout_separator_bytes,
        captured.stdout.len(),
        "total, dropped, separators and retained must describe one stream"
    );
    assert_eq!(
        captured.stderr_total_bytes - captured.stderr_dropped_bytes
            + captured.stderr_separator_bytes,
        captured.stderr.len()
    );
    assert!(
        captured.stdout.len() >= CAPTURE_RETENTION_BYTES,
        "compaction must keep the retention target, not empty the tail"
    );
}

#[test]
fn capture_late_newline_cannot_defeat_the_retention_ceiling() {
    for is_stdout in [true, false] {
        let mut captured = CapturedOutput::default();
        let chunk = "x".repeat(4096);
        for _ in 0..(CAPTURE_MAX_RETAINED_BYTES / chunk.len()) {
            captured.push_chunk(&chunk, chunk.len(), is_stdout);
        }
        let terminated = format!("{}\n", "y".repeat(4095));
        for _ in 0..100 {
            captured.push_chunk(&terminated, terminated.len(), is_stdout);
            let (stream, total, dropped, separators) = if is_stdout {
                (
                    &captured.stdout,
                    captured.stdout_total_bytes,
                    captured.stdout_dropped_bytes,
                    captured.stdout_separator_bytes,
                )
            } else {
                (
                    &captured.stderr,
                    captured.stderr_total_bytes,
                    captured.stderr_dropped_bytes,
                    captured.stderr_separator_bytes,
                )
            };
            assert!(
                stream.len() <= CAPTURE_MAX_RETAINED_BYTES,
                "retained {} exceeds {}",
                stream.len(),
                CAPTURE_MAX_RETAINED_BYTES
            );
            assert_eq!(total - dropped + separators, stream.len());
            assert!(stream.starts_with(&"x".repeat(CAPTURE_HEAD_BYTES)));
            assert!(stream.ends_with(&terminated));
            assert!(dropped > 0);
        }
    }
}

/// Splicing the head's trailing partial line onto the tail's leading
/// partial line would render a line the child never printed. Every
/// retained line must be one the child actually wrote.
#[test]
fn elision_never_fabricates_a_line() {
    let mut captured = CapturedOutput::default();
    // Distinct, self-identifying lines: any concatenation of two halves is
    // detectable because a well-formed line has exactly one "|end" marker.
    let mut written = 0_usize;
    for index in 0..40_000 {
        let line = format!("line-{index:06}-payload|end\n");
        let raw_len = line.len();
        written += raw_len;
        captured.push_chunk(&line, raw_len, true);
    }
    assert!(captured.stdout_dropped_bytes > 0, "the test must elide");
    for line in captured.stdout.lines() {
        if line.is_empty() {
            continue;
        }
        assert_eq!(
            line.matches("|end").count(),
            1,
            "a spliced line was fabricated: {line:?}"
        );
        assert!(
            line.starts_with("line-") && line.ends_with("|end"),
            "retained line is not one the child wrote: {line:?}"
        );
    }
    assert_eq!(
        captured.stdout_total_bytes, written,
        "the total must equal what this test actually wrote, or the \
         accounting under test is being measured against a fiction"
    );
    assert_eq!(
        captured.stdout_total_bytes - captured.stdout_dropped_bytes
            + captured.stdout_separator_bytes,
        captured.stdout.len(),
        "total, dropped, separators and retained must describe one stream"
    );
}

/// Output with no newline in range has no line structure to cut on, so the
/// character-boundary fallback would splice two fragments into one
/// apparent line. A separator must keep them distinct. The realistic
/// trigger is a single huge `console.log(JSON.stringify(...))`.
#[test]
fn newline_free_output_is_separated_rather_than_spliced() {
    let mut captured = CapturedOutput::default();
    // One continuous line, far larger than the retained region.
    let head_mark = "HEADSTART";
    captured.push_chunk(head_mark, head_mark.len(), true);
    let chunk = "a".repeat(4096);
    for _ in 0..((CAPTURE_MAX_RETAINED_BYTES / chunk.len()) * 4) {
        captured.push_chunk(&chunk, chunk.len(), true);
    }
    assert!(captured.stdout_dropped_bytes > 0, "the test must elide");
    assert!(
        captured.stdout_separator_bytes > 0,
        "a newline-free elision must insert a separator: {captured:?}"
    );
    assert!(
        captured.stdout.len() <= CAPTURE_MAX_RETAINED_BYTES,
        "separators must not defeat the ceiling: {}",
        captured.stdout.len()
    );
    assert!(
        captured.stdout.starts_with(head_mark),
        "the pinned head must survive"
    );
    assert_eq!(
        captured.stdout_total_bytes - captured.stdout_dropped_bytes
            + captured.stdout_separator_bytes,
        captured.stdout.len(),
        "separators are the only slack in the invariant"
    );
}

/// `\r`-only progress output has no `\n` at all, so it takes the same
/// fallback path.
#[test]
fn carriage_return_only_output_is_separated_rather_than_spliced() {
    let mut captured = CapturedOutput::default();
    let chunk = "progress\r".repeat(456);
    for _ in 0..((CAPTURE_MAX_RETAINED_BYTES / chunk.len()) * 4) {
        captured.push_chunk(&chunk, chunk.len(), true);
    }
    assert!(captured.stdout_dropped_bytes > 0, "the test must elide");
    assert!(
        captured.stdout_separator_bytes > 0,
        "\\r is not a line terminator for this cut: {captured:?}"
    );
    assert!(
        captured.stdout.contains('\n'),
        "the separator must be present"
    );
}

/// A single line longer than the retained region must not defeat the
/// memory ceiling. This exercises the `tail_start <= head_end` guard's
/// neighbourhood, which is otherwise unreachable.
#[test]
fn one_enormous_line_still_respects_the_ceiling() {
    let mut captured = CapturedOutput::default();
    let chunk = "z".repeat(4096);
    for _ in 0..((CAPTURE_MAX_RETAINED_BYTES / chunk.len()) * 16) {
        captured.push_chunk(&chunk, chunk.len(), true);
    }
    // No newline was ever written, so every compaction took the fallback.
    assert!(
        captured.stdout.len() <= CAPTURE_MAX_RETAINED_BYTES,
        "a newline-free flood must not grow without bound: {} retained after {} written",
        captured.stdout.len(),
        captured.stdout_total_bytes
    );
    assert!(
        captured.stdout_total_bytes > CAPTURE_MAX_RETAINED_BYTES * 8,
        "the test must actually flood"
    );
}

/// The total counts bytes the child wrote, not the length of the lossy
/// UTF-8 decoding. An invalid byte expands to a three-byte replacement
/// character, which would otherwise inflate every published offset.
#[test]
fn totals_count_raw_bytes_not_lossy_decoded_length() {
    let mut captured = CapturedOutput::default();
    let decoded = String::from_utf8_lossy(&[0xff, 0xfe, 0xfd]).into_owned();
    assert!(
        decoded.len() > 3,
        "precondition: lossy decoding must expand these bytes"
    );
    captured.push_chunk(&decoded, 3, true);
    assert_eq!(
        captured.stdout_total_bytes, 3,
        "the child wrote three bytes, whatever the decoding cost"
    );
}

/// A child announces itself before it floods. `keld-host` forwards the
/// retained transcript, so evicting the pinned head would lose the ready
/// marker that `no_flag_macos`/`no_flag_linux` assert is forwarded after
/// the fixture writes 1 MiB of noise.
#[test]
fn bounded_capture_pins_the_head_against_a_later_flood() {
    let mut captured = CapturedOutput::default();
    let marker = "KELD_EARLY_MARKER\n";
    captured.push_chunk(marker, marker.len(), true);
    let noise = "x".repeat(4096);
    for _ in 0..((CAPTURE_MAX_RETAINED_BYTES / noise.len()) * 8) {
        captured.push_chunk(&noise, noise.len(), true);
    }
    assert!(
        captured.stdout_dropped_bytes > 0,
        "the flood must have forced an elision, or this proves nothing"
    );
    assert!(
        captured.stdout.starts_with(marker),
        "the pinned head must survive an arbitrarily long later flood"
    );
    assert!(
        captured.stdout.len() <= CAPTURE_MAX_RETAINED_BYTES,
        "pinning the head must not defeat the ceiling: {}",
        captured.stdout.len()
    );
    assert!(
        captured.stdout.ends_with('x'),
        "the sliding tail must still carry the most recent output"
    );
}

/// The retained transcript is a `String`, so a cut that split a multi-byte
/// character would be unrepresentable. Both cuts must land on boundaries,
/// and the drop count must equal the bytes removed.
#[test]
fn bounded_capture_cuts_on_a_utf8_character_boundary() {
    let mut captured = CapturedOutput::default();
    // Three-byte characters do not align with the byte overflow.
    let chunk = "\u{20ac}".repeat(4096);
    let chunks = (CAPTURE_MAX_RETAINED_BYTES / chunk.len()) * 4;
    for _ in 0..chunks {
        captured.push_chunk(&chunk, chunk.len(), true);
    }
    assert!(captured.stdout_dropped_bytes > 0, "the test must truncate");
    assert!(
        captured
            .stdout
            .chars()
            .all(|c| c == '\u{20ac}' || c == '\n'),
        "a boundary-splitting cut would leave replacement characters; the \
         only newline permitted is the inserted elision separator"
    );
    assert_eq!(
        captured.stdout.matches('\n').count(),
        captured.stdout_separator_bytes,
        "this input has no newlines of its own, so every newline must be an \
         accounted separator"
    );
    assert_eq!(
        captured.stdout_total_bytes - captured.stdout_dropped_bytes
            + captured.stdout_separator_bytes,
        captured.stdout.len()
    );
}
