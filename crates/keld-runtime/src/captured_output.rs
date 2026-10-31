//! Bounded, generation-monotonic captured transcripts (KEL-134).

/// Bytes retained from the **start** of each stream, never evicted.
///
/// A child announces itself early — ready markers, link banners, the first
/// error — and the host forwards what the supervisor retained. A pure sliding
/// tail would let a later flood evict exactly the lines a supervisor exists to
/// report, so the head is pinned and only the middle is elided (KEL-134).
pub const CAPTURE_HEAD_BYTES: usize = 64 * 1024;

/// Bytes retained from the **end** of each stream, sliding as output arrives.
///
/// The tail carries the most recent output, which is what the stderr tail
/// carried by a crash diagnostic needs.
pub const CAPTURE_TAIL_BYTES: usize = 192 * 1024;

/// Bytes retained per stream once compaction has run: head plus tail.
///
/// A supervised child is untrusted and may print without bound. Retaining
/// every byte let one faulty or hostile generation exhaust the host while
/// crash isolation was supposed to keep the window alive (KEL-134).
pub const CAPTURE_RETENTION_BYTES: usize = CAPTURE_HEAD_BYTES + CAPTURE_TAIL_BYTES;

/// Slack allowed above [`CAPTURE_RETENTION_BYTES`] before compaction runs.
///
/// Compaction moves the retained tail, so compacting on every 4 KiB read would
/// move the whole retained region per read. Draining only once the buffer
/// exceeds retention *plus* this slack bounds that to one move per `SLACK`
/// bytes written, while keeping the steady-state ceiling fixed and the hot
/// path allocation-free.
pub const CAPTURE_COMPACTION_SLACK_BYTES: usize = 64 * 1024;

/// Hard ceiling on bytes retained per stream at any instant.
pub const CAPTURE_MAX_RETAINED_BYTES: usize =
    CAPTURE_RETENTION_BYTES + CAPTURE_COMPACTION_SLACK_BYTES;

/// Stdout/stderr captured from the currently or most-recently supervised child.
///
/// `stdout` and `stderr` are **bounded transcripts**, not whole streams: at
/// most [`CAPTURE_MAX_RETAINED_BYTES`] per stream is retained, as a pinned
/// head of [`CAPTURE_HEAD_BYTES`] followed by a sliding tail. Only the middle
/// is elided. Text wholly inside the pinned head survives later floods, but
/// arbitrary markers may be dropped. Use [`crate::Supervisor::start_with_stdout_markers`]
/// for diagnostic markers whose first observation must survive elision.
///
/// The `*_total_bytes` counters record everything the child wrote and never
/// reset across restart generations, so an ordering fact taken from them stays
/// comparable for the life of the supervisor. `*_dropped_bytes` states how
/// much of the middle was elided, so a consumer can tell "nothing was printed"
/// from "it was dropped" (KEL-134).
///
/// Once `*_dropped_bytes` is non-zero the retained bytes after the head are no
/// longer at their stream offsets; use the counters, not string indices, for
/// ordering.
#[derive(Debug, Default, Clone)]
pub struct CapturedOutput {
    /// Retained stdout across every spawn attempt: pinned head, elided middle,
    /// sliding tail. Bounded by [`CAPTURE_MAX_RETAINED_BYTES`]; see
    /// [`Self::stdout_dropped_bytes`].
    pub stdout: String,
    /// Retained stderr across every spawn attempt: pinned head, elided middle,
    /// sliding tail. Bounded by [`CAPTURE_MAX_RETAINED_BYTES`]; see
    /// [`Self::stderr_dropped_bytes`].
    pub stderr: String,
    /// Total stdout bytes written by every generation, including bytes no
    /// longer retained. Never reset by a restart; monotonic non-decreasing.
    pub stdout_total_bytes: usize,
    /// Total stderr bytes written by every generation, including bytes no
    /// longer retained. Never reset by a restart; monotonic non-decreasing.
    pub stderr_total_bytes: usize,
    /// Stdout bytes elided from the middle to honour the retention bound.
    /// Invariant while the child writes valid UTF-8:
    /// `stdout_total_bytes - stdout_dropped_bytes + stdout_separator_bytes == stdout.len()`.
    pub stdout_dropped_bytes: usize,
    /// Stderr bytes elided from the middle to honour the retention bound.
    /// Invariant while the child writes valid UTF-8:
    /// `stderr_total_bytes - stderr_dropped_bytes + stderr_separator_bytes == stderr.len()`.
    pub stderr_dropped_bytes: usize,
    /// `\n` separators inserted into `stdout` at an elision whose span held no
    /// newline, so two line fragments cannot merge into one apparent line.
    pub stdout_separator_bytes: usize,
    /// `\n` separators inserted into `stderr` at an elision whose span held no
    /// newline, so two line fragments cannot merge into one apparent line.
    pub stderr_separator_bytes: usize,
}

impl CapturedOutput {
    fn tail(s: &str, max_chars: usize) -> String {
        let char_count = s.chars().count();
        let skip = char_count.saturating_sub(max_chars);
        s.chars().skip(skip).collect()
    }

    pub(crate) fn stderr_tail(&self, max_chars: usize) -> String {
        Self::tail(&self.stderr, max_chars)
    }

    /// Appends `chunk`, counts `raw_len` bytes against the total, and elides
    /// the middle within [`CAPTURE_MAX_RETAINED_BYTES`] once the buffer exceeds
    /// that ceiling. Line-boundary preservation may consume compaction slack.
    ///
    /// `raw_len` is the byte count the child actually wrote. It is passed
    /// separately because `chunk` has already been through
    /// `String::from_utf8_lossy`, which expands each invalid byte into a
    /// three-byte replacement character; counting the decoded length would
    /// overstate every ordering offset the ledger publishes.
    ///
    /// Both cut points land on a **line** boundary where one exists in range,
    /// so the pinned head never has its trailing partial line concatenated
    /// onto the tail's leading partial line — that would render a line the
    /// child never printed.
    ///
    /// Output with no newline in range (a single very long line, or `\r`-only
    /// progress output) has no line structure to cut on. There the cut falls
    /// back to a UTF-8 character boundary and a single `\n` separator is
    /// inserted, so the two fragments still cannot merge into one apparent
    /// line. Separator bytes are counted in `separators` and are the only
    /// reason `stream.len()` can exceed `total - dropped`.
    ///
    /// The pinned head is never evicted, so output a child printed before a
    /// flood still reaches the host.
    fn append(
        stream: &mut String,
        total: &mut usize,
        dropped: &mut usize,
        separators: &mut usize,
        chunk: &str,
        raw_len: usize,
    ) {
        *total = total.saturating_add(raw_len);
        stream.push_str(chunk);
        if stream.len() <= CAPTURE_MAX_RETAINED_BYTES {
            return;
        }
        let (head_end, head_is_line_aligned) = Self::head_cut(stream);
        let tail_start = Self::tail_cut(stream);
        // The head search is capped at MAX - TAIL. Since compaction only
        // runs above MAX, tail_cut starts strictly after that cap; these cuts
        // cannot overlap. A line-free head falls back to its UTF-8 boundary.
        stream.drain(head_end..tail_start);
        *dropped = dropped.saturating_add(tail_start - head_end);
        if !head_is_line_aligned {
            stream.insert(head_end, '\n');
            *separators = separators.saturating_add(1);
        }
    }

    /// End of the pinned head, and whether it lands just after a newline.
    ///
    /// The character boundary is resolved *before* slicing: `CAPTURE_HEAD_BYTES`
    /// can land inside a multi-byte character, and slicing there panics.
    fn head_cut(stream: &str) -> (usize, bool) {
        let from = Self::boundary_at_or_after(stream, CAPTURE_HEAD_BYTES);
        let limit = CAPTURE_MAX_RETAINED_BYTES - CAPTURE_TAIL_BYTES;
        // Search bytes so the upper bound need not be a UTF-8 boundary. A
        // newline is ASCII; its following byte is always a character boundary.
        stream.as_bytes()[from..limit]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or((from, false), |offset| (from + offset + 1, true))
    }

    /// Start of the sliding tail: the byte after the first nonterminal newline
    /// at or after the tail window, else the character boundary there. Same boundary-first
    /// rule as [`Self::head_cut`].
    fn tail_cut(stream: &str) -> usize {
        let from =
            Self::boundary_at_or_after(stream, stream.len().saturating_sub(CAPTURE_TAIL_BYTES));
        stream[from..]
            .find('\n')
            // Cutting after an EOF newline would discard the entire recent
            // tail. Preserve its character-aligned suffix in that case.
            .filter(|offset| from + offset + 1 < stream.len())
            .map_or(from, |offset| from + offset + 1)
    }

    /// Smallest UTF-8 character boundary at or after `index`, clamped to the
    /// end of `stream`.
    fn boundary_at_or_after(stream: &str, index: usize) -> usize {
        if index >= stream.len() {
            return stream.len();
        }
        let mut boundary = index;
        while boundary < stream.len() && !stream.is_char_boundary(boundary) {
            boundary += 1;
        }
        boundary
    }

    /// One line naming how much output was elided, for a consumer that renders
    /// the transcript to a human. `None` when nothing was dropped.
    #[must_use]
    pub fn elision_notice(dropped: usize) -> Option<String> {
        (dropped > 0).then(|| {
            format!("[keld: {dropped} bytes of child output elided between the retained head and tail (KELD-RUNTIME capture bound)]\n")
        })
    }

    /// Records a captured chunk against the stream `is_stdout` selects.
    ///
    /// `raw_len` is the number of bytes read from the child, before lossy
    /// UTF-8 decoding.
    pub(super) fn push_chunk(&mut self, chunk: &str, raw_len: usize, is_stdout: bool) {
        if is_stdout {
            Self::append(
                &mut self.stdout,
                &mut self.stdout_total_bytes,
                &mut self.stdout_dropped_bytes,
                &mut self.stdout_separator_bytes,
                chunk,
                raw_len,
            );
        } else {
            Self::append(
                &mut self.stderr,
                &mut self.stderr_total_bytes,
                &mut self.stderr_dropped_bytes,
                &mut self.stderr_separator_bytes,
                chunk,
                raw_len,
            );
        }
    }
}

#[cfg(test)]
mod tests;
