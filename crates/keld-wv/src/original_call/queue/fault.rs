/// One existing control fault latch. Payloads stay in the queue's same backing,
/// so a fatal callback error needs no owned-payload exception slot or new queue.
#[derive(Default)]
pub(crate) enum OriginalControlFault {
    #[default]
    Clear,
    Latched,
}
impl OriginalControlFault {
    pub(crate) const fn is_faulted(&self) -> bool {
        matches!(self, Self::Latched)
    }
    pub(crate) fn latch(&mut self) {
        *self = Self::Latched;
    }
}
