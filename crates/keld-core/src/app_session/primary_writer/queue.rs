//! Fixed storage and receipt custody for the one physical writer.

#[cfg(test)]
use super::worker;
use super::{
    Arc, CONTROL_BYTES, Class, Instant, POSITIONS, Packet, Position, RECORD_METADATA_BYTES,
    Reservation, Shared, WRITER_BUCKET_BYTES, WriteCompletion, WriteObservation, WriteReceipt,
    WriterLink, WriterRefusal,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Free,
    Reserved,
    Queued,
    Writing,
    Complete,
}

pub(super) struct Slot {
    serial: u64,
    pub(super) phase: Phase,
    pub(super) packet: Option<Packet>,
    pub(super) target: Option<Arc<WriterLink>>,
    receipt_live: bool,
    completion: Option<WriteCompletion>,
    written_at: Option<Instant>,
    pub(super) deadline: Option<Instant>,
}
impl Slot {
    const fn new() -> Self {
        Self {
            serial: 0,
            phase: Phase::Free,
            packet: None,
            target: None,
            receipt_live: false,
            completion: None,
            written_at: None,
            deadline: None,
        }
    }
    fn clear(&mut self) {
        self.phase = Phase::Free;
        self.packet = None;
        self.target = None;
        self.receipt_live = false;
        self.completion = None;
        self.written_at = None;
        self.deadline = None;
    }
}

pub(super) struct State {
    pub(super) slots: [Slot; POSITIONS],
    queue: [Option<usize>; POSITIONS],
    head: usize,
    len: usize,
    pub(super) closed: bool,
    interest: u8,
    pub(super) wake_pending: bool,
    #[cfg(test)]
    next_send_failure: Option<WriterRefusal>,
    #[cfg(test)]
    pub(super) after_dequeue: Option<Arc<worker::DequeueHook>>,
    #[cfg(test)]
    pub(super) capacity_wait_observer: Option<std::sync::mpsc::SyncSender<()>>,
    #[cfg(test)]
    pub(super) quiescence_wait_observer: Option<std::sync::mpsc::SyncSender<()>>,
}
impl State {
    pub(super) const fn new() -> Self {
        Self {
            slots: [const { Slot::new() }; POSITIONS],
            queue: [None; POSITIONS],
            head: 0,
            len: 0,
            closed: false,
            interest: 0,
            wake_pending: false,
            #[cfg(test)]
            next_send_failure: None,
            #[cfg(test)]
            after_dequeue: None,
            #[cfg(test)]
            capacity_wait_observer: None,
            #[cfg(test)]
            quiescence_wait_observer: None,
        }
    }

    #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn observed_value_position(&self) -> super::physical_value_hold::ValuePosition {
        let slot = &self.slots[0];
        super::physical_value_hold::ValuePosition {
            serial: slot.serial,
            phase: match slot.phase {
                Phase::Free => "free",
                Phase::Reserved => "reserved",
                Phase::Queued => "queued",
                Phase::Writing => "writing",
                Phase::Complete => "complete",
            },
            target_id: slot
                .target
                .as_ref()
                .map(|target| Arc::as_ptr(target) as usize),
            packet_in_slot: slot.packet.is_some(),
            receipt_live: slot.receipt_live,
        }
    }

    pub(super) fn reserve(&mut self, class: Class) -> Result<Position, WriterRefusal> {
        if self.closed {
            return Err(WriterRefusal::Disconnected);
        }
        let range = match class {
            Class::Value => 0..1,
            Class::Control => 1..POSITIONS,
        };
        let index = range
            .into_iter()
            .find(|index| self.slots[*index].phase == Phase::Free)
            .ok_or(WriterRefusal::Full)?;
        let slot = &mut self.slots[index];
        slot.serial = slot.serial.checked_add(1).ok_or(WriterRefusal::Exhausted)?;
        slot.phase = Phase::Reserved;
        slot.receipt_live = true;
        Ok(Position {
            index,
            serial: slot.serial,
        })
    }

    /// This is the actual final try-send seam. Fault controls choose only its
    /// existing Full/Disconnected branch; returned frames retain their allocation.
    pub(super) fn admit(&mut self, position: Position) -> Result<(), WriterRefusal> {
        if self.closed {
            return Err(WriterRefusal::Disconnected);
        }
        let slot = self
            .slots
            .get(position.index)
            .ok_or(WriterRefusal::InvalidFrame)?;
        if slot.serial != position.serial || slot.phase != Phase::Reserved {
            return Err(WriterRefusal::InvalidFrame);
        }
        #[cfg(test)]
        if let Some(failure) = self.next_send_failure.take() {
            return Err(failure);
        }
        if self.len == POSITIONS {
            return Err(WriterRefusal::Full);
        }
        Ok(())
    }

    pub(super) fn enqueue(&mut self, index: usize) {
        self.queue[(self.head + self.len) % POSITIONS] = Some(index);
        self.len += 1;
    }

    /// Dequeue does not release any payload or receipt credit. The worker must
    /// still cross Core's publication fence before entering its first write.
    pub(super) fn dequeue(&mut self) -> Result<Option<(Position, Packet)>, WriterRefusal> {
        if self.len == 0 {
            return Ok(None);
        }
        let index = self.queue[self.head]
            .take()
            .ok_or(WriterRefusal::InvalidFrame)?;
        self.head = (self.head + 1) % POSITIONS;
        self.len -= 1;
        let slot = &mut self.slots[index];
        if slot.phase != Phase::Queued {
            return Err(WriterRefusal::InvalidFrame);
        }
        let packet = slot.packet.take().ok_or(WriterRefusal::InvalidFrame)?;
        slot.phase = Phase::Writing;
        Ok(Some((
            Position {
                index,
                serial: slot.serial,
            },
            packet,
        )))
    }

    pub(super) fn target_observations(
        &self,
        target: &Arc<WriterLink>,
    ) -> [Option<Position>; POSITIONS] {
        std::array::from_fn(|index| {
            let slot = &self.slots[index];
            if !matches!(slot.phase, Phase::Queued | Phase::Writing)
                || !slot
                    .target
                    .as_ref()
                    .is_some_and(|held| Arc::ptr_eq(held, target))
            {
                return None;
            }
            Some(Position {
                index,
                serial: slot.serial,
            })
        })
    }

    /// Observation only: actual reservation and caller fences remain authority.
    /// Existing serials distinguish reuse without a new completion counter.
    pub(super) fn completed_positions(&self) -> [Option<Position>; POSITIONS] {
        std::array::from_fn(|index| {
            let slot = &self.slots[index];
            (slot.phase == Phase::Complete).then_some(Position {
                index,
                serial: slot.serial,
            })
        })
    }

    pub(super) fn pending(&self, position: Position) -> bool {
        self.slots.get(position.index).is_some_and(|slot| {
            slot.serial == position.serial && matches!(slot.phase, Phase::Queued | Phase::Writing)
        })
    }

    pub(super) fn completion(
        &self,
        position: Position,
    ) -> Result<Option<WriteCompletion>, WriterRefusal> {
        let slot = self
            .slots
            .get(position.index)
            .ok_or(WriterRefusal::InvalidFrame)?;
        if slot.serial != position.serial || !slot.receipt_live {
            return Err(WriterRefusal::InvalidFrame);
        }
        Ok(slot.completion)
    }

    /// Observation only. A released/reused serial has no retained outcome; it
    /// is never reported as Written on behalf of its unique receipt owner.
    pub(super) fn observed_completion(&self, position: Position) -> Option<WriteCompletion> {
        self.slots
            .get(position.index)
            .filter(|slot| slot.serial == position.serial)
            .and_then(|slot| slot.completion)
    }

    pub(super) fn written_at(&self, position: Position) -> Result<Option<Instant>, WriterRefusal> {
        self.completion(position)?; // exact live receipt/serial validation
        Ok(self.slots[position.index].written_at)
    }

    #[cfg(test)]
    pub(super) fn complete(&mut self, position: Position, outcome: WriteCompletion) -> bool {
        self.complete_at(position, outcome, Instant::now())
    }

    /// Called only after the worker has dropped its encoded Box and the physical
    /// write has returned. Receipt metadata remains charged until its owner drops.
    pub(super) fn complete_at(
        &mut self,
        position: Position,
        outcome: WriteCompletion,
        completed_at: Instant,
    ) -> bool {
        let Some(slot) = self.slots.get_mut(position.index) else {
            self.closed = true;
            return self.progress();
        };
        if slot.serial != position.serial || slot.phase != Phase::Writing {
            self.closed = true;
            return self.progress();
        }
        slot.phase = Phase::Complete;
        slot.completion = Some(outcome);
        slot.written_at = (outcome == WriteCompletion::Written).then_some(completed_at);
        if !slot.receipt_live {
            slot.clear();
        }
        // A correlated CONTROL may retain this exact physical observation after
        // its source/native halves ended. Wake the existing storage recheck even
        // when its prior UI unit remains outstanding; this grants no capacity.
        let native = self.native_control_progress();
        self.progress() || native
    }

    pub(super) fn release(&mut self, position: Position, reservation: bool) -> bool {
        let Some(slot) = self.slots.get_mut(position.index) else {
            return false;
        };
        if slot.serial != position.serial {
            return false;
        }
        self.interest &= !(1 << (2 + position.index));
        if reservation {
            if slot.phase != Phase::Reserved {
                self.closed = true;
                return self.progress();
            }
            slot.clear();
        } else {
            slot.receipt_live = false;
            if slot.phase == Phase::Complete {
                slot.clear();
            }
            // Queued/Writing cannot become Free merely because a waiter left.
        }
        self.progress()
    }

    pub(super) fn acknowledge_progress(&mut self) -> bool {
        self.wake_pending = false;
        self.progress()
    }

    /// A stopped worker has returned from native I/O. Retain admitted receipts
    /// but drop every never-written payload before any abandoned slot is freed.
    pub(super) fn worker_ended(&mut self) -> bool {
        self.closed = true;
        self.queue = [None; POSITIONS];
        self.len = 0;
        for slot in &mut self.slots {
            if matches!(slot.phase, Phase::Queued | Phase::Writing) {
                slot.packet = None;
                slot.phase = Phase::Complete;
                slot.completion = Some(WriteCompletion::OutcomeUnknown);
                if !slot.receipt_live {
                    slot.clear();
                }
            }
        }
        self.progress()
    }

    pub(super) fn register_interest(&mut self, mask: u8) -> bool {
        self.interest |= mask;
        self.progress()
    }

    pub(super) fn progress(&mut self) -> bool {
        let mut available = 0;
        if self.slots[0].phase == Phase::Free {
            available |= 1;
        }
        if self.slots[1..].iter().any(|slot| slot.phase == Phase::Free) {
            available |= 2;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if slot.phase == Phase::Complete {
                available |= 1 << (2 + index);
            }
        }
        if self.closed {
            available = u8::MAX;
        }
        self.publish_progress(self.interest & available)
    }

    pub(super) fn native_control_progress(&mut self) -> bool {
        let interested = self.interest & 128 != 0;
        // Consume the actual release observation even when the existing unit
        // UI marker is already outstanding. The synchronous publisher waits on
        // this SAME bit/Condvar; it must not depend on dispatching that marker.
        self.interest &= !128;
        interested && self.completed_source_progress()
    }

    pub(super) fn native_control_interest_pending(&self) -> bool {
        self.interest & 128 != 0
    }

    pub(super) fn retired(&mut self) -> bool {
        // Retirement wakes existing waiters even when all physical positions
        // remain charged. No slot is made available by this unit observation.
        self.publish_progress(self.interest)
    }

    fn publish_progress(&mut self, satisfied: u8) -> bool {
        if satisfied != 0 && self.completed_source_progress() {
            self.interest &= !satisfied;
            true
        } else {
            false
        }
    }

    /// A completed source remains with its existing owner after its receipt is
    /// freed. Reuse the same one outstanding unit marker even if the service
    /// currently running already acknowledged the preceding marker.
    pub(super) fn completed_source_progress(&mut self) -> bool {
        if self.wake_pending {
            false
        } else {
            self.wake_pending = true;
            true
        }
    }

    #[cfg(test)]
    pub(super) fn fail_next_send(&mut self, failure: WriterRefusal) {
        self.next_send_failure = Some(failure);
    }

    #[cfg(test)]
    pub(super) fn charged(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.phase != Phase::Free)
            .count()
    }
}

// Payload backing allocations are exactly bounded separately by M + 4*C.
// Include both queued and worker-local packet metadata, the reservation and
// receipt, a worst-case distinct retained link per position and Arc headers.
// Shared State/Condvar/wake sender/FIFO storage is counted here, not hidden in
// an opaque channel allocation or a per-frame callback/closure.
const _: () = assert!(
    std::mem::size_of::<Shared>()
        + POSITIONS
            * (std::mem::size_of::<Packet>()
                + std::mem::size_of::<Reservation>()
                + std::mem::size_of::<WriteReceipt>()
                + 2 * std::mem::size_of::<WriteObservation>()
                + std::mem::size_of::<super::super::compat_router::CompatRetirementReply>()
                + std::mem::size_of::<WriterLink>()
                + 4 * 2 * std::mem::size_of::<usize>())
        <= POSITIONS * RECORD_METADATA_BYTES
);
const _: () = assert!(keld_ipc::HEADER_LEN == 16);
const _: () = assert!(CONTROL_BYTES == 4096);
const _: () = assert!(WRITER_BUCKET_BYTES == 16_798_720);
