/// Opaque digest of a source already admitted by the frozen graph/Host owner.
/// A digest on its own grants no filesystem, IPC or navigation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedSourceId(pub(crate) [u8; 32]);
