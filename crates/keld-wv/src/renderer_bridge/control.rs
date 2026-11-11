//! Native CONTROL storage is leased from Core's existing CompatLedger.

/// Opaque accounting custody implemented by the existing Core ledger owner.
/// Its destructor reports actual storage release; it carries no role, document,
/// route, source or effect authority. Native owners retain it while an operation
/// is pending, posting, outcome-unknown, or awaiting proved quiescence.
pub trait NativeControlCharge: Send + Sync {
    /// Extend this same live ledger record before preparing more storage.
    /// Failure preserves its previous charge and does not authorize allocation.
    fn try_extend(&self, encoded_bytes: usize, accounted_bytes: usize) -> bool;
    /// Atomically add simultaneous native storage to this same actual record.
    /// On failure no prior bytes, slot, or owner custody are changed.
    fn try_extend_storage(&self, encoded_bytes: usize, additional_accounted_bytes: usize) -> bool;
}

/// One non-clone CONTROL charge, acquired before any JSON/UTF-16 allocation.
/// The native operation owns this value until actual completion or proved
/// never-written cancellation. Dropping it never releases a DATA consumer half.
#[must_use = "retain the charge with its native operation until quiescence"]
pub struct NativeControlReservation {
    _charge: Box<dyn NativeControlCharge>,
}
impl std::fmt::Debug for NativeControlReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeControlReservation")
            .finish_non_exhaustive()
    }
}
impl NativeControlReservation {
    /// Grow this exact reservation before allocating native encodings. Core
    /// checks both existing ceilings and retains the previous charge on failure.
    /// Call outside Core/ledger locks; this is storage, never final authority.
    #[must_use]
    pub fn try_extend(&self, encoded_bytes: usize, accounted_bytes: usize) -> bool {
        self._charge.try_extend(encoded_bytes, accounted_bytes)
    }

    /// Add native JSON/UTF-16/scratch storage to the same queued DTO charge.
    /// Call once before allocation; failure retains the prior token/charge.
    #[must_use]
    pub fn try_extend_storage(
        &self,
        encoded_bytes: usize,
        additional_accounted_bytes: usize,
    ) -> bool {
        self._charge
            .try_extend_storage(encoded_bytes, additional_accounted_bytes)
    }

    /// Construct only after the canonical Core owner has reserved one of its
    /// four existing CONTROL slots and every accounted backing allocation.
    #[must_use]
    pub fn from_owner(charge: impl NativeControlCharge + 'static) -> Self {
        Self {
            _charge: Box::new(charge),
        }
    }
}

/// Captured actual Core session/caller storage owner. This is not a posting
/// permit: a final canonical caller/native fence remains mandatory after encode.
pub trait RendererControlOwner: Send + Sync {
    /// `encoded_bytes` is the exact bounded UTF-8 envelope. `accounted_bytes`
    /// includes all retained backing capacities, UTF-16 units at two bytes each,
    /// the terminating unit where present, and 1024 fixed bytes per record.
    /// Refuse before allocation if the same session's existing four slots or
    /// 65536 aggregate accounted bytes are unavailable, including retired work.
    fn try_reserve_native_control(
        &self,
        encoded_bytes: usize,
        accounted_bytes: usize,
    ) -> Option<NativeControlReservation>;
}
