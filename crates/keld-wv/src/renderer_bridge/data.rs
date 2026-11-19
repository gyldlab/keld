//! Opaque storage custody in the existing Core DATA record.
//! No native, Function or consumer completion is inferred from storage release.

/// The exact existing Core DATA record owns this storage accounting capability.
/// Its destructor reports only disposal of retained native representation storage.
/// It must not set `native_done`, `consumer_done`, `FunctionFinished` or Core release.
pub trait NativeDataCharge: Send + Sync {
    /// Add all simultaneously retained native representations before allocation.
    /// Core validates the sole serializer's existing DATA envelope ceiling and
    /// the same record's existing 262144-byte total. Failure preserves its charge.
    fn try_extend_storage(&self, encoded_bytes: usize, additional_accounted_bytes: usize) -> bool;
}

/// Non-clone storage custody of the exact original Core DATA record.
///
/// Native owners declare this field after their owned payload/backing so those
/// buffers are dropped first. Retain the whole operation after an unknown outcome.
/// Dropping this token cannot establish native or consumer completion.
#[must_use = "retain the DATA storage token until all its owned backing is disposed"]
pub struct NativeDataReservation {
    charge: Box<dyn NativeDataCharge>,
}
impl std::fmt::Debug for NativeDataReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeDataReservation")
            .finish_non_exhaustive()
    }
}
impl NativeDataReservation {
    /// Add simultaneous JSON/UTF16/scratch to the SAME actual DATA record.
    /// This storage capability conveys no source, document, role or effect grant.
    #[must_use]
    pub fn try_extend_storage(
        &self,
        encoded_bytes: usize,
        additional_accounted_bytes: usize,
    ) -> bool {
        self.charge
            .try_extend_storage(encoded_bytes, additional_accounted_bytes)
    }

    /// Construct only from the existing Core DATA owner after its original
    /// record and all queued DTO capacities are charged. The opaque allocation
    /// itself belongs to that record's existing fixed metadata charge.
    pub fn from_owner(charge: impl NativeDataCharge + 'static) -> Self {
        Self {
            charge: Box::new(charge),
        }
    }
}
