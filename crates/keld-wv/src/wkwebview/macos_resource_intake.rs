//! GH473: bounded, authority-free accounting for a native `WebKit` resource task.
//! This module grants no `WebView` filesystem, manifest, CSP, or Bun authority.
//! Future native callbacks must reject methods and body presence before copying
//! native `NSString` values or opening an `HTTPBodyStream`.

use std::collections::BTreeMap;

#[cfg(test)]
mod native_pending_policy_contract;

/// Limits come from verified Host policy, never a page, Wry, or a global fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IntakeLimits {
    pub max_url_utf16_units: usize,
    pub max_header_fields: usize,
    pub max_header_utf16_units: usize,
    pub max_asset_bytes: usize,
    /// All retained native tasks, including metadata waiting for the reader.
    pub task_slots: usize,
    pub max_retained_bytes: usize,
}

/// GH473 approved *candidate* caps for the pinned 23,118,872-byte app asset.
/// The native high-water and cancellation proof must qualify these values.
pub(super) const ORIGINAL_WEBAPP_LIMITS: IntakeLimits = IntakeLimits {
    max_url_utf16_units: 2_048,
    max_header_fields: 32,
    max_header_utf16_units: 4_096,
    max_asset_bytes: 32 * 1024 * 1024,
    task_slots: 64,
    max_retained_bytes: 256 * 1024 * 1024,
};

/// Metadata observed without turning the native request into an `http::Request`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NativeRequestShape {
    pub is_get: bool,
    pub has_body: bool,
    pub has_body_stream: bool,
    pub url_utf16_units: usize,
    pub header_fields: usize,
    pub header_utf16_units: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IntakeReject {
    WrongMethod,
    BodyOrStream,
    MissingUrl,
    MetadataLimit,
    AssetLimit,
    NoTaskCapacity,
    RetainedByteLimit,
    DuplicateNativeTask,
    TaskIncarnationExhausted,
    StoppedOrFinished,
    WorkNotQuiescent,
}

/// Shape-only admission. Not permission to open or deliver source bytes.
pub(super) fn check_shape(
    shape: NativeRequestShape,
    limits: IntakeLimits,
) -> Result<(), IntakeReject> {
    // Order matters: an invalid method fails before header/body conversions.
    if !shape.is_get {
        return Err(IntakeReject::WrongMethod);
    }
    if shape.has_body || shape.has_body_stream {
        return Err(IntakeReject::BodyOrStream);
    }
    if shape.url_utf16_units == 0 {
        return Err(IntakeReject::MissingUrl);
    }
    if shape.url_utf16_units > limits.max_url_utf16_units
        || shape.header_fields > limits.max_header_fields
        || shape.header_utf16_units > limits.max_header_utf16_units
    {
        return Err(IntakeReject::MetadataLimit);
    }
    Ok(())
}

/// Conservative ownership charge BEFORE the worker reads source bytes. The
/// four copies cover the bounded Rust read, response data, Cocoa ownership and
/// one pending-delivery representation; the exact high-water needs native
/// qualification. Metadata includes native UTF-16 URL/header lengths and
/// bounded MIME/CSP response headers. No allocation based on untrusted size.
pub(super) fn worst_case_charge(
    asset_size: u64,
    native_metadata_units: usize,
    policy_response_bytes: usize,
    limits: IntakeLimits,
) -> Result<usize, IntakeReject> {
    let bytes = usize::try_from(asset_size).map_err(|_| IntakeReject::AssetLimit)?;
    if bytes > limits.max_asset_bytes {
        return Err(IntakeReject::AssetLimit);
    }
    let copies = bytes
        .checked_mul(4)
        .ok_or(IntakeReject::RetainedByteLimit)?;
    // Each native UTF-16 unit may expand to multiple UTF-8 bytes. Four is
    // conservative conversion headroom, not a loss-prone two-byte ceiling.
    let request = native_metadata_units
        .checked_mul(4)
        .ok_or(IntakeReject::RetainedByteLimit)?;
    let charge = copies
        .checked_add(request)
        .and_then(|n| n.checked_add(policy_response_bytes))
        .ok_or(IntakeReject::RetainedByteLimit)?;
    if charge > limits.max_retained_bytes {
        return Err(IntakeReject::RetainedByteLimit);
    }
    Ok(charge)
}

/// Checked pending metadata ceiling under the approved source-owner policy.
/// Opaque native allocator memory still requires separate OS qualification.
const MAX_PENDING_METADATA_BYTES: usize = 40 * 1024;

pub(super) fn pending_metadata_charge(
    native_units: usize,
    selected_path_capacity: usize,
    record_bytes: usize,
    response_policy_bytes: usize,
    limits: IntakeLimits,
) -> Result<usize, IntakeReject> {
    let response = response_policy_bytes
        .checked_add(4096)
        .ok_or(IntakeReject::MetadataLimit)?;
    let charge = worst_case_charge(0, native_units, response, limits)?
        .checked_add(selected_path_capacity)
        .and_then(|bytes| bytes.checked_add(record_bytes))
        .ok_or(IntakeReject::MetadataLimit)?;
    if charge > MAX_PENDING_METADATA_BYTES {
        return Err(IntakeReject::MetadataLimit);
    }
    Ok(charge)
}

/// Task incarnation is NOT GH542's document generation. It prevents a native
/// task-pointer reuse from resurrecting cancellation or retained accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TaskTicket {
    native_identity: usize,
    incarnation: u64,
}

impl TaskTicket {
    pub(super) const fn native_identity(self) -> usize {
        self.native_identity
    }

    /// Read-only existing resource-ticket ordinal for dedicated Root diagnostics.
    /// It is neither a document identifier nor a new counter or authority.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) const fn diagnostic_incarnation(self) -> u64 {
        self.incarnation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Live,
    Stopped,
    Terminal,
}

#[derive(Clone, Copy, Debug)]
struct Task {
    incarnation: u64,
    phase: Phase,
    retained_bytes: usize,
    worker_active: bool,
    queued_read: bool,
    delivery_active: bool,
}

/// UI-executor-only ledger. Native task stop tombstones first; work and memory
/// reservations remain charged until the true worker/buffer owner is quiescent.
/// No Objective-C reference or filesystem handle is captured in this state.
#[derive(Debug)]
pub(super) struct TaskLedger {
    limits: IntakeLimits,
    next_incarnation: u64,
    tasks: BTreeMap<usize, Task>,
    retained_bytes: usize,
}

impl TaskLedger {
    pub(super) fn new(limits: IntakeLimits) -> Self {
        Self {
            limits,
            next_incarnation: 0,
            tasks: BTreeMap::new(),
            retained_bytes: 0,
        }
    }

    pub(super) fn begin(&mut self, native_identity: usize) -> Result<TaskTicket, IntakeReject> {
        if self.tasks.contains_key(&native_identity) {
            return Err(IntakeReject::DuplicateNativeTask);
        }
        if self.tasks.len() >= self.limits.task_slots {
            return Err(IntakeReject::NoTaskCapacity);
        }
        let incarnation = self
            .next_incarnation
            .checked_add(1)
            .ok_or(IntakeReject::TaskIncarnationExhausted)?;
        self.next_incarnation = incarnation;
        self.tasks.insert(
            native_identity,
            Task {
                incarnation,
                phase: Phase::Live,
                retained_bytes: 0,
                worker_active: false,
                queued_read: false,
                delivery_active: false,
            },
        );
        Ok(TaskTicket {
            native_identity,
            incarnation,
        })
    }

    fn current(&self, ticket: TaskTicket) -> Result<&Task, IntakeReject> {
        self.tasks
            .get(&ticket.native_identity)
            .filter(|task| task.incarnation == ticket.incarnation)
            .ok_or(IntakeReject::StoppedOrFinished)
    }

    fn current_mut(&mut self, ticket: TaskTicket) -> Result<&mut Task, IntakeReject> {
        self.tasks
            .get_mut(&ticket.native_identity)
            .filter(|task| task.incarnation == ticket.incarnation)
            .ok_or(IntakeReject::StoppedOrFinished)
    }

    pub(super) fn reserve_bytes(
        &mut self,
        ticket: TaskTicket,
        additional: usize,
    ) -> Result<(), IntakeReject> {
        if self.current(ticket)?.phase != Phase::Live {
            return Err(IntakeReject::StoppedOrFinished);
        }
        let next = self
            .retained_bytes
            .checked_add(additional)
            .ok_or(IntakeReject::RetainedByteLimit)?;
        if next > self.limits.max_retained_bytes {
            return Err(IntakeReject::RetainedByteLimit);
        }
        let entry = self.current_mut(ticket)?;
        entry.retained_bytes = entry
            .retained_bytes
            .checked_add(additional)
            .ok_or(IntakeReject::RetainedByteLimit)?;
        self.retained_bytes = next;
        Ok(())
    }

    /// Mark qualified metadata ready without reserving a body or worker.
    pub(super) fn queue_read(&mut self, ticket: TaskTicket) -> Result<(), IntakeReject> {
        let entry = self.current_mut(ticket)?;
        if entry.phase != Phase::Live || entry.worker_active || entry.queued_read {
            return Err(IntakeReject::StoppedOrFinished);
        }
        entry.queued_read = true;
        Ok(())
    }

    /// One outstanding read/result uses the existing single worker and bounded
    /// channel. A terminal delivery also holds the slot across native reentry.
    /// Admission incarnation orders service, never native pointer value.
    pub(super) fn next_queued_read(&self) -> Option<TaskTicket> {
        if self
            .tasks
            .values()
            .any(|task| task.worker_active || task.delivery_active || task.phase == Phase::Terminal)
        {
            return None;
        }
        self.tasks
            .iter()
            .filter(|(_, task)| task.phase == Phase::Live && task.queued_read)
            .min_by_key(|(_, task)| task.incarnation)
            .map(|(native_identity, task)| TaskTicket {
                native_identity: *native_identity,
                incarnation: task.incarnation,
            })
    }

    pub(super) fn worker_started(&mut self, ticket: TaskTicket) -> Result<(), IntakeReject> {
        if self
            .tasks
            .values()
            .any(|task| task.worker_active || task.delivery_active || task.phase == Phase::Terminal)
        {
            return Err(IntakeReject::NoTaskCapacity);
        }
        let entry = self.current_mut(ticket)?;
        if entry.phase != Phase::Live {
            return Err(IntakeReject::StoppedOrFinished);
        }
        entry.queued_read = false;
        entry.worker_active = true;
        Ok(())
    }

    /// The native stopURLSchemeTask callback tombstones the associated task.
    pub(super) fn stop(&mut self, native_identity: usize) {
        if let Some(entry) = self.tasks.get_mut(&native_identity) {
            entry.phase = Phase::Stopped;
        }
    }

    /// Destroy/Quit also covers a delivery temporarily owned outside the task map.
    pub(super) fn stop_all(&mut self) {
        for task in self.tasks.values_mut() {
            task.phase = Phase::Stopped;
        }
    }

    /// Take the terminal callback lease before reentering Objective-C. Do not
    /// retain a `RefCell` borrow or Rust lock while invoking a `WebKit` callback.
    pub(super) fn begin_terminal_callback(&mut self, ticket: TaskTicket) -> bool {
        let Ok(entry) = self.current_mut(ticket) else {
            return false;
        };
        if entry.phase != Phase::Live {
            return false;
        }
        entry.phase = Phase::Terminal;
        true
    }

    /// The UI consumes the worker result but still owns response/native buffers.
    pub(super) fn delivery_started(&mut self, ticket: TaskTicket) -> Result<(), IntakeReject> {
        let entry = self.current_mut(ticket)?;
        if !entry.worker_active || entry.delivery_active {
            return Err(IntakeReject::StoppedOrFinished);
        }
        entry.worker_active = false;
        entry.delivery_active = true;
        Ok(())
    }

    pub(super) fn has_active_delivery(&self) -> bool {
        self.tasks.values().any(|task| task.delivery_active)
    }

    /// Called only after the complete UI response and its owned credit drop.
    pub(super) fn delivery_quiesced(&mut self, ticket: TaskTicket) -> Result<(), IntakeReject> {
        self.current_mut(ticket)?.delivery_active = false;
        Ok(())
    }

    pub(super) fn worker_quiesced(&mut self, ticket: TaskTicket) -> Result<(), IntakeReject> {
        self.current_mut(ticket)?.worker_active = false;
        Ok(())
    }

    /// Release a terminal/stopped incarnation only after work is quiescent.
    pub(super) fn release(&mut self, ticket: TaskTicket) -> Result<(), IntakeReject> {
        let entry = self.current(ticket)?;
        if entry.phase == Phase::Live || entry.worker_active || entry.delivery_active {
            return Err(IntakeReject::WorkNotQuiescent);
        }
        let charged = entry.retained_bytes;
        self.retained_bytes -= charged;
        self.tasks.remove(&ticket.native_identity);
        Ok(())
    }

    /// Called after each native didReceiveResponse / didReceiveData boundary.
    /// A reentrant WK stop can tombstone even a previously Terminal task.
    pub(super) fn may_continue_native_delivery(&self, ticket: TaskTicket) -> bool {
        self.current(ticket)
            .is_ok_and(|task| task.phase == Phase::Terminal && !task.worker_active)
    }

    #[cfg(test)]
    fn live_count(&self) -> usize {
        self.tasks.len()
    }

    #[cfg(test)]
    fn retained(&self) -> usize {
        self.retained_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> IntakeLimits {
        // Fixture inputs only, not production-approved limits.
        IntakeLimits {
            max_url_utf16_units: 32,
            max_header_fields: 2,
            max_header_utf16_units: 12,
            max_asset_bytes: 32,
            task_slots: 2,
            max_retained_bytes: 100,
        }
    }

    fn get() -> NativeRequestShape {
        NativeRequestShape {
            is_get: true,
            has_body: false,
            has_body_stream: false,
            url_utf16_units: 8,
            header_fields: 1,
            header_utf16_units: 6,
        }
    }

    #[test]
    fn rejects_post_before_header_and_body_accounting() {
        let mut req = get();
        req.is_get = false;
        req.has_body = true;
        req.has_body_stream = true;
        req.url_utf16_units = usize::MAX;
        assert_eq!(check_shape(req, limits()), Err(IntakeReject::WrongMethod));
        req.is_get = true;
        assert_eq!(check_shape(req, limits()), Err(IntakeReject::BodyOrStream));
        req.has_body = false;
        assert_eq!(check_shape(req, limits()), Err(IntakeReject::BodyOrStream));
    }

    #[test]
    fn detects_exact_metadata_max_plus_one() {
        assert_eq!(check_shape(get(), limits()), Ok(()));
        for (mut req, code) in [
            (
                NativeRequestShape {
                    url_utf16_units: 0,
                    ..get()
                },
                IntakeReject::MissingUrl,
            ),
            (
                NativeRequestShape {
                    url_utf16_units: 33,
                    ..get()
                },
                IntakeReject::MetadataLimit,
            ),
            (
                NativeRequestShape {
                    header_fields: 3,
                    ..get()
                },
                IntakeReject::MetadataLimit,
            ),
            (
                NativeRequestShape {
                    header_utf16_units: 13,
                    ..get()
                },
                IntakeReject::MetadataLimit,
            ),
        ] {
            assert_eq!(check_shape(req, limits()), Err(code));
            req.url_utf16_units = 32;
            req.header_fields = 2;
            req.header_utf16_units = 12;
            assert_eq!(check_shape(req, limits()), Ok(()));
        }
    }

    #[test]
    fn stopped_work_keeps_reservation_charged_until_quiescent() {
        let mut ledger = TaskLedger::new(limits());
        let a = ledger.begin(1).unwrap();
        ledger.reserve_bytes(a, 90).unwrap();
        ledger.worker_started(a).unwrap();
        ledger.stop(1);
        assert!(!ledger.begin_terminal_callback(a));
        assert_eq!(
            ledger.reserve_bytes(a, 1),
            Err(IntakeReject::StoppedOrFinished)
        );
        assert_eq!(ledger.release(a), Err(IntakeReject::WorkNotQuiescent));
        assert_eq!(ledger.retained(), 90);
        let b = ledger.begin(2).unwrap();
        assert_eq!(
            ledger.reserve_bytes(b, 11),
            Err(IntakeReject::RetainedByteLimit)
        );
        ledger.worker_quiesced(a).unwrap();
        ledger.release(a).unwrap();
        assert_eq!(ledger.retained(), 0);
        ledger.reserve_bytes(b, 11).unwrap();
        assert!(!ledger.begin_terminal_callback(a));
        assert!(ledger.begin_terminal_callback(b));
        ledger.release(b).unwrap();
        assert_eq!(ledger.release(b), Err(IntakeReject::StoppedOrFinished));
    }

    #[test]
    fn pointer_reuse_and_incarnation_cannot_resurrect_old_task() {
        let mut ledger = TaskLedger::new(limits());
        let old = ledger.begin(7).unwrap();
        assert_eq!(ledger.begin(7), Err(IntakeReject::DuplicateNativeTask));
        assert!(ledger.begin_terminal_callback(old));
        assert!(!ledger.begin_terminal_callback(old));
        ledger.release(old).unwrap();
        let new = ledger.begin(7).unwrap();
        assert_ne!(new, old);
        ledger.stop(7);
        assert!(!ledger.begin_terminal_callback(new));
        assert!(!ledger.begin_terminal_callback(old));
        ledger.release(new).unwrap();
        assert_eq!(ledger.live_count(), 0);
    }

    #[test]
    fn active_work_cannot_release_or_overbook_slot_and_bytes() {
        let mut ledger = TaskLedger::new(limits());
        let a = ledger.begin(9).unwrap();
        let b = ledger.begin(10).unwrap();
        assert_eq!(ledger.begin(11), Err(IntakeReject::NoTaskCapacity));
        assert_eq!(ledger.release(a), Err(IntakeReject::WorkNotQuiescent));
        ledger.reserve_bytes(a, 100).unwrap();
        assert_eq!(
            ledger.reserve_bytes(b, 1),
            Err(IntakeReject::RetainedByteLimit)
        );
        assert!(ledger.begin_terminal_callback(a));
        ledger.release(a).unwrap();
        ledger.reserve_bytes(b, 100).unwrap();
        assert!(ledger.begin_terminal_callback(b));
        ledger.release(b).unwrap();
    }

    #[test]
    fn checked_arithmetic_refuses_overflow() {
        let mut ledger = TaskLedger::new(IntakeLimits {
            max_retained_bytes: usize::MAX,
            ..limits()
        });
        let ticket = ledger.begin(1).unwrap();
        ledger.reserve_bytes(ticket, usize::MAX).unwrap();
        assert_eq!(
            ledger.reserve_bytes(ticket, 1),
            Err(IntakeReject::RetainedByteLimit)
        );
        assert!(ledger.begin_terminal_callback(ticket));
        ledger.release(ticket).unwrap();
        assert_eq!(ledger.retained(), 0);

        ledger.next_incarnation = u64::MAX;
        assert_eq!(ledger.begin(2), Err(IntakeReject::TaskIncarnationExhausted));
    }

    #[test]
    fn original_max_asset_and_32mib_limit_charge_before_open() {
        let source = 23_118_872_u64;
        let charge = worst_case_charge(
            source,
            ORIGINAL_WEBAPP_LIMITS.max_url_utf16_units
                + ORIGINAL_WEBAPP_LIMITS.max_header_utf16_units,
            4_096,
            ORIGINAL_WEBAPP_LIMITS,
        )
        .expect("required original 23 MB asset");
        assert!(charge < ORIGINAL_WEBAPP_LIMITS.max_retained_bytes);
        let mut ledger = TaskLedger::new(ORIGINAL_WEBAPP_LIMITS);
        let a = ledger.begin(1).expect("first native asset");
        let b = ledger.begin(2).expect("second native asset");
        ledger.reserve_bytes(a, charge).expect("source charge");
        ledger.reserve_bytes(b, charge).expect("two source copies");
        assert_eq!(ORIGINAL_WEBAPP_LIMITS.task_slots, 64);
        let pending: Vec<_> = (3..=64)
            .map(|identity| ledger.begin(identity).expect("bounded pending metadata"))
            .collect();
        assert_eq!(ledger.begin(65), Err(IntakeReject::NoTaskCapacity));
        for ticket in pending {
            assert!(ledger.begin_terminal_callback(ticket));
            ledger.release(ticket).expect("metadata retired");
        }
        assert_eq!(
            worst_case_charge(
                ORIGINAL_WEBAPP_LIMITS.max_asset_bytes as u64 + 1,
                0,
                0,
                ORIGINAL_WEBAPP_LIMITS
            ),
            Err(IntakeReject::AssetLimit)
        );
        assert_eq!(
            worst_case_charge(u64::MAX, usize::MAX, usize::MAX, ORIGINAL_WEBAPP_LIMITS),
            Err(IntakeReject::AssetLimit)
        );
        assert!(ledger.begin_terminal_callback(a));
        ledger.release(a).expect("release once");
        assert_eq!(ledger.release(a), Err(IntakeReject::StoppedOrFinished));
        assert!(ledger.begin_terminal_callback(b));
        ledger.release(b).expect("release two");
        assert_eq!(ledger.retained(), 0);
    }

    #[test]
    fn native_stop_after_terminal_callback_prevents_next_native_method() {
        let mut ledger = TaskLedger::new(limits());
        let ticket = ledger.begin(5).expect("task");
        ledger.reserve_bytes(ticket, 12).expect("charge");
        assert!(ledger.begin_terminal_callback(ticket));
        assert!(ledger.may_continue_native_delivery(ticket));
        ledger.stop(5);
        assert!(!ledger.may_continue_native_delivery(ticket));
        ledger.release(ticket).expect("quiescent stopped ticket");
        assert!(!ledger.may_continue_native_delivery(ticket));
        assert_eq!(ledger.retained(), 0);
    }
    #[test]
    fn queued_fanout_is_bounded_fifo_and_never_parallel_reader_work() {
        let mut ledger = TaskLedger::new(IntakeLimits {
            task_slots: 5,
            ..limits()
        });
        // Native pointer order is deliberately unrelated to admission order.
        let ids = [90, 5, 70, 2, 10];
        let tickets: Vec<_> = ids
            .into_iter()
            .map(|id| {
                let ticket = ledger.begin(id).expect("bounded parallel metadata");
                ledger.reserve_bytes(ticket, 1).expect("metadata only");
                ledger.queue_read(ticket).expect("qualified pending read");
                ticket
            })
            .collect();
        assert_eq!(ledger.begin(100), Err(IntakeReject::NoTaskCapacity));
        assert_eq!(ledger.retained(), 5);
        for (index, ticket) in tickets.into_iter().enumerate() {
            assert_eq!(ledger.next_queued_read(), Some(ticket));
            ledger.worker_started(ticket).expect("one reader slot");
            assert_eq!(ledger.next_queued_read(), None);
            assert_eq!(
                ledger.worker_started(ticket),
                Err(IntakeReject::NoTaskCapacity)
            );
            assert_eq!(ledger.release(ticket), Err(IntakeReject::WorkNotQuiescent));
            ledger
                .worker_quiesced(ticket)
                .expect("actual result consumed");
            assert!(ledger.begin_terminal_callback(ticket));
            // Native callback reentry must not start another body while the
            // previous terminal response still owns its delivery reservation.
            assert_eq!(ledger.next_queued_read(), None);
            ledger.release(ticket).expect("terminal retired");
            assert_eq!(ledger.retained(), 4 - index);
        }
        assert_eq!(ledger.live_count(), 0);
        assert_eq!(ledger.next_queued_read(), None);
    }

    #[test]
    fn queued_stop_needs_no_worker_and_cannot_revive_reused_pointer() {
        let mut ledger = TaskLedger::new(limits());
        let first = ledger.begin(8).expect("active");
        ledger.queue_read(first).expect("queued");
        ledger.worker_started(first).expect("worker");
        let stopped = ledger.begin(7).expect("pending");
        ledger.reserve_bytes(stopped, 9).expect("metadata");
        ledger.queue_read(stopped).expect("queued");
        ledger.stop(7);
        ledger
            .release(stopped)
            .expect("pending has no worker or body");
        assert_eq!(ledger.retained(), 0);
        let reused = ledger.begin(7).expect("new native incarnation");
        ledger.queue_read(reused).expect("queued");
        assert_ne!(stopped, reused);
        assert_eq!(
            ledger.queue_read(stopped),
            Err(IntakeReject::StoppedOrFinished)
        );
        assert_eq!(ledger.next_queued_read(), None);
        ledger.stop(8);
        assert_eq!(ledger.release(first), Err(IntakeReject::WorkNotQuiescent));
        ledger.worker_quiesced(first).expect("actual work complete");
        ledger.release(first).expect("stopped old worker");
        assert_eq!(ledger.next_queued_read(), Some(reused));
        ledger.stop(7);
        assert_eq!(ledger.next_queued_read(), None);
        ledger.release(reused).expect("pending shutdown");
        assert_eq!(ledger.live_count(), 0);
    }
    #[test]
    fn ui_response_custody_blocks_dispatch_and_release_until_actual_drop() {
        let mut ledger = TaskLedger::new(limits());
        let first = ledger.begin(1).expect("first");
        let next = ledger.begin(2).expect("pending");
        ledger.queue_read(first).expect("first queued");
        ledger.queue_read(next).expect("next queued");
        ledger.reserve_bytes(first, 80).expect("body");
        ledger.worker_started(first).expect("worker");
        ledger
            .delivery_started(first)
            .expect("actual worker result");
        assert_eq!(
            ledger.delivery_started(first),
            Err(IntakeReject::StoppedOrFinished)
        );
        assert!(ledger.begin_terminal_callback(first));
        ledger.stop(1);
        assert_eq!(ledger.next_queued_read(), None);
        assert_eq!(
            ledger.worker_started(next),
            Err(IntakeReject::NoTaskCapacity)
        );
        assert_eq!(ledger.release(first), Err(IntakeReject::WorkNotQuiescent));
        assert_eq!(ledger.retained(), 80);
        ledger
            .delivery_quiesced(first)
            .expect("all UI references dropped");
        ledger.release(first).expect("terminal exact incarnation");
        assert_eq!(ledger.retained(), 0);
        assert_eq!(ledger.next_queued_read(), Some(next));
    }

    #[test]
    fn pending_metadata_charge_has_checked_exact_40kib_ceiling() {
        assert_eq!(
            pending_metadata_charge(6144, 8192, 256, 220, ORIGINAL_WEBAPP_LIMITS),
            Ok(37340)
        );
        // 4096 response overhead + 36KiB owned path/record is exactly 40KiB.
        assert_eq!(
            pending_metadata_charge(0, 36 * 1024, 0, 0, ORIGINAL_WEBAPP_LIMITS),
            Ok(40 * 1024)
        );
        assert_eq!(
            pending_metadata_charge(0, 36 * 1024, 1, 0, ORIGINAL_WEBAPP_LIMITS),
            Err(IntakeReject::MetadataLimit)
        );
        assert_eq!(
            pending_metadata_charge(0, usize::MAX, 1, 0, ORIGINAL_WEBAPP_LIMITS),
            Err(IntakeReject::MetadataLimit)
        );
        assert_eq!(
            pending_metadata_charge(0, 0, 0, usize::MAX, ORIGINAL_WEBAPP_LIMITS),
            Err(IntakeReject::MetadataLimit)
        );
    }
    #[test]
    fn shutdown_tombstones_delivery_even_when_native_map_owner_is_local() {
        let mut ledger = TaskLedger::new(limits());
        let delivering = ledger.begin(1).expect("delivery ticket");
        let pending = ledger.begin(2).expect("pending ticket");
        ledger.queue_read(delivering).expect("qualified");
        ledger.queue_read(pending).expect("qualified");
        ledger.worker_started(delivering).expect("reader");
        ledger.delivery_started(delivering).expect("result on UI");
        assert!(ledger.begin_terminal_callback(delivering));
        assert!(ledger.may_continue_native_delivery(delivering));
        ledger.stop_all();
        assert!(!ledger.may_continue_native_delivery(delivering));
        assert!(!ledger.begin_terminal_callback(pending));
        assert!(ledger.has_active_delivery());
        assert_eq!(
            ledger.release(delivering),
            Err(IntakeReject::WorkNotQuiescent)
        );
        ledger.release(pending).expect("pending metadata quiescent");
        ledger
            .delivery_quiesced(delivering)
            .expect("actual UI references dropped");
        ledger
            .release(delivering)
            .expect("stopped delivery retired");
        assert!(!ledger.has_active_delivery());
        assert_eq!(ledger.live_count(), 0);
    }
}
