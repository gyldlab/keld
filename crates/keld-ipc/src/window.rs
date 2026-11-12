//! Host-owned window payloads (GH-531 §4.a–b).
//!
//! These values use the shared [`crate::codec`] postcard encoder and strict
//! decoder. Variant and field order are wire contracts pinned by the shared
//! `tests/fixtures/window-v0.tsv` vectors. Payload decoding does not authenticate
//! a caller or authorize a window operation: the host registry validates every
//! supplied identity against its current owner and state.

use serde::{Deserialize, Serialize};

/// App-visible, host-minted window number, never reused within an app session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowId(
    /// The host registry's nonzero window number.
    pub u32,
);

/// Opaque host-minted window-incarnation identity, compared only for equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowGeneration(
    /// The host registry's nonzero incarnation value, never reused in a session.
    pub u64,
);

/// Both identity halves required by every window-addressed message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowRef {
    /// App-visible window number.
    pub id: WindowId,
    /// Opaque identity of this window incarnation.
    pub incarnation: WindowGeneration,
}

/// Outer-frame bounds in logical points, from the primary display's top-left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    /// Horizontal coordinate in logical points.
    pub x: i32,
    /// Vertical coordinate in logical points.
    pub y: i32,
    /// Outer-frame width in logical points.
    pub width: u32,
    /// Outer-frame height in logical points.
    pub height: u32,
}

/// Host-confirmed state applied to the app's local window mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    /// Current outer-frame bounds.
    pub bounds: Rect,
    /// Whether this application window has focus.
    pub focused: bool,
    /// Whether the native window is maximized.
    pub maximized: bool,
    /// Whether the native window is in full-screen mode.
    pub full_screen: bool,
}

/// One actual host-sampled desktop display. All sizes are positive logical points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopFact {
    /// Native nonzero display identifier (not a principal or OS grant).
    pub id: u32,
    /// Full display frame relative to the primary display's top-left.
    pub bounds: Rect,
    /// Native usable display frame, in the same coordinate space.
    pub work_area: Rect,
    /// Actual positive finite native-to-backing scale factor.
    pub scale_factor: f64,
}

/// Complete native desktop and appearance capture for the admitted primary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopFacts {
    /// Native display identifier matching exactly one entry in `displays`.
    pub primary_id: u32,
    /// One to 32 complete displays, never a synthetic or truncated subset.
    pub displays: Vec<DesktopFact>,
    /// Observed current effective application appearance.
    pub should_use_dark_colors: bool,
}

/// Maximum postcard storage for a qualified desktop event with at most 32 displays.
///
/// Event tag (1), epoch (10), Option (1), primary id (5), Vec length (1),
/// 32 records of id (5), two four-field rectangles (40), f64 (8), and bool (1).
/// This structural storage bound is not the separately enforced 32-KiB wire cap.
pub const DESKTOP_EVENT_STORAGE_BYTES: usize = 1 + 10 + 1 + 5 + 1 + 32 * (5 + 40 + 8) + 1;

/// Host-to-app window facts; discriminants are append-only, starting at zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WindowEvent {
    /// First fact for a window, or its complete state in a recovery replay.
    Created {
        /// Host-minted identity of the window.
        window: WindowRef,
        /// Host-confirmed initial or replayed state.
        state: WindowState,
        /// Recovery-gap close sequence; absent on every ordinary creation.
        pending_close: Option<u64>,
    },
    /// Changed outer-frame position or size.
    Bounds {
        /// Window whose bounds changed.
        window: WindowRef,
        /// Confirmed outer-frame bounds.
        bounds: Rect,
    },
    /// Changed native focus.
    Focus {
        /// Window whose focus changed.
        window: WindowRef,
        /// Confirmed focus state.
        focused: bool,
    },
    /// Changed native maximization state.
    Maximized {
        /// Window whose maximization state changed.
        window: WindowRef,
        /// Confirmed maximization state.
        maximized: bool,
    },
    /// Changed native full-screen state.
    FullScreen {
        /// Window whose full-screen state changed.
        window: WindowRef,
        /// Confirmed full-screen state.
        full_screen: bool,
    },
    /// A pending close attempt requiring a matching application verdict.
    CloseRequested {
        /// Window whose close is pending.
        window: WindowRef,
        /// Host-minted sequence identifying this close attempt.
        close_seq: u64,
    },
    /// Final fact for a window, after native release.
    Destroyed {
        /// Identity that the app must tombstone before notifying listeners.
        window: WindowRef,
    },
    /// GH481 S1: appended event tag 7. Global passive fact without a `WindowRef`.
    DesktopSnapshot {
        /// Monotonically increasing host-session observation generation, never zero.
        epoch: u64,
        /// Complete current observation or explicit invalidation.
        snapshot: Option<DesktopFacts>,
    },
    /// Actual qualified native cross-document commit for one admitted load.
    NavigationCommitted {
        /// Existing host-minted window identity.
        window: WindowRef,
        /// Generation observed from the sole native document owner.
        document_generation: u32,
        /// Actual qualified native URL, never reconstructed from the request.
        native_url: String,
    },
    /// Actual native finish for the same admitted committed document.
    NavigationFinished {
        /// Existing host-minted window identity.
        window: WindowRef,
        /// Same native-observed generation as the matching commit.
        document_generation: u32,
    },
    /// Snapshot of a still-live original document after a Bun role replacement.
    /// This is not a navigation completion or permission to route to the page.
    DocumentRecovered {
        /// Same surviving host-minted window identity and incarnation.
        window: WindowRef,
        /// Actual committed generation whose native finish was already observed.
        document_generation: u32,
        /// Retained credited native URL from that exact qualified commit.
        native_url: String,
    },
}

/// App-to-host window calls; discriminants are append-only, starting at zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowRequest {
    /// Create a native window, or adopt an eligible recovered survivor.
    Create {
        /// Requested outer-frame width in logical points.
        width: u32,
        /// Requested outer-frame height in logical points.
        height: u32,
        /// Requested horizontal position, or the platform default.
        x: Option<i32>,
        /// Requested vertical position, or the platform default.
        y: Option<i32>,
    },
    /// Request a maximization change, confirmed by a later host fact.
    SetMaximized {
        /// Window to change.
        window: WindowRef,
        /// Desired maximization state.
        maximized: bool,
    },
    /// Request a full-screen change, confirmed by a later host fact.
    SetFullScreen {
        /// Window to change.
        window: WindowRef,
        /// Desired full-screen state.
        full_screen: bool,
    },
    /// Begin or merge a cancelable close attempt; this call never blocks.
    RequestClose {
        /// Window to close.
        window: WindowRef,
    },
    /// Reply to one host-minted pending close sequence.
    CloseReply {
        /// Window whose close is pending.
        window: WindowRef,
        /// Sequence from the matching close request.
        close_seq: u64,
        /// Application decision for that attempt.
        verdict: CloseVerdict,
    },
    /// Destroy a window without a cancelable close event.
    Destroy {
        /// Window to destroy.
        window: WindowRef,
    },
    /// Create with bounded preload metadata; Host admission authenticates source.
    CreateWithPreload {
        /// Requested outer-frame width in logical points.
        width: u32,
        /// Requested outer-frame height in logical points.
        height: u32,
        /// Requested horizontal position, or the platform default.
        x: Option<i32>,
        /// Requested vertical position, or the platform default.
        y: Option<i32>,
        /// App-selected absolute selector; this is not a source authority.
        #[serde(deserialize_with = "preload_path")]
        preload_absolute: String,
        /// Ordered app arguments, retained only after Host source admission.
        #[serde(deserialize_with = "preload_arguments")]
        additional_arguments: Vec<String>,
    },
    /// Load the one original editor entry admitted by the Host source owner.
    LoadUrl {
        /// Existing host-minted window identity.
        window: WindowRef,
        /// Exact original file URI; it supplies no source or policy authority.
        uri: String,
    },
    /// Destroy only if the retained original native document still owns this window.
    DestroyOriginalDocument {
        /// Existing Host-minted pair, authenticated against the current caller.
        window: WindowRef,
        /// Existing native-observed generation, a selector rather than authority.
        document_generation: u32,
    },
}

/// Application verdict for a pending cancelable close; `Allow = 0`, `Veto = 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloseVerdict {
    /// Continue the accepted close through native release.
    Allow,
    /// Retain the native window and its document.
    Veto,
}

/// Host-to-app replies; errors use the existing [`crate::CallError`] contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowResponse {
    /// A new native window was created; its `Created` fact precedes this reply.
    Created {
        /// Host-minted identity of the new window.
        window: WindowRef,
    },
    /// A setter was accepted; confirmed state arrives as a host fact.
    Accepted,
    /// A new cancelable close attempt was started.
    CloseRequested {
        /// Host-minted pending close sequence.
        close_seq: u64,
    },
    /// A request merged into an already pending close attempt.
    Merged {
        /// Existing pending close sequence.
        close_seq: u64,
    },
    /// The window is already closing.
    AlreadyClosing,
    /// The matching close verdict was applied.
    ReplyApplied,
    /// The close verdict was stale; the host also records one diagnostic.
    ReplyStale,
    /// Native release completed; the `Destroyed` fact precedes this reply.
    Destroyed,
    /// A successor adopted an already replayed window, without a new native build.
    Adopted {
        /// Existing host identity retained by the adopted window.
        window: WindowRef,
    },
    /// The actual matching native finish was emitted before this reply.
    Loaded,
}

/// Maximum complete original-load request body before owned decoding.
pub const MAX_ORIGINAL_LOAD_REQUEST_BYTES: usize = 8192;

/// Fixed output storage for the bounded native navigation observation body.
pub const NAVIGATION_EVENT_STORAGE_BYTES: usize = 32 * 1024;

#[cfg(test)]
mod tests;

// Bounds are schema admission only. Exact retained-root/path/digest checks remain
// Host-owned. The shared postcard decoder remains the only wire parser.
fn preload_path<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<String, D::Error> {
    let value = <&str>::deserialize(decoder)?;
    if value.is_empty() || value.len() > 2048 {
        return Err(serde::de::Error::custom(
            "preload selector must contain 1..=2048 UTF-8 bytes",
        ));
    }
    Ok(value.to_owned())
}

fn preload_arguments<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Vec<String>, D::Error> {
    struct Arguments;
    impl<'de> serde::de::Visitor<'de> for Arguments {
        type Value = Vec<String>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 16 bounded preload arguments")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|count| count > 16) {
                return Err(serde::de::Error::custom("too many preload arguments"));
            }
            let mut values = Vec::new();
            let mut total = 0;
            while let Some(value) = seq.next_element::<&str>()? {
                if values.len() == 16 || value.len() > 256 {
                    return Err(serde::de::Error::custom(
                        "preload argument count or size exceeded",
                    ));
                }
                total += value.len();
                if total > 2048 {
                    return Err(serde::de::Error::custom(
                        "preload arguments exceed 2048 UTF-8 bytes",
                    ));
                }
                values.push(value.to_owned());
            }
            Ok(values)
        }
    }
    decoder.deserialize_seq(Arguments)
}

/// Encode a charged native URL by borrow, without detaching or cloning its bytes.
///
/// This is the borrowed encoder view of `WindowEvent::NavigationCommitted`, at
/// the same append-only ledger position. The owned event remains the app decoder
/// schema; shared literal vectors qualify their byte equality.
///
/// # Errors
/// Returns the owning postcard codec error when output storage is insufficient.
pub fn encode_navigation_committed_into<'buffer>(
    window: WindowRef,
    document_generation: u32,
    native_url: &str,
    buffer: &'buffer mut [u8],
) -> Result<&'buffer [u8], crate::IpcError> {
    struct BorrowedCommit<'url> {
        window: WindowRef,
        document_generation: u32,
        native_url: &'url str,
    }
    impl Serialize for BorrowedCommit<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeStructVariant as _;
            let mut value =
                serializer.serialize_struct_variant("WindowEvent", 8, "NavigationCommitted", 3)?;
            value.serialize_field("window", &self.window)?;
            value.serialize_field("document_generation", &self.document_generation)?;
            value.serialize_field("native_url", self.native_url)?;
            value.end()
        }
    }
    crate::codec::encode_into(
        &BorrowedCommit {
            window,
            document_generation,
            native_url,
        },
        buffer,
    )
}

/// Encode the retained document snapshot without copying its credited URL.
/// Tag 10 is append-only and denotes observation, never a new native finish.
///
/// # Errors
/// Returns the owning postcard codec error when output storage is insufficient.
pub fn encode_document_recovered_into<'buffer>(
    window: WindowRef,
    document_generation: u32,
    native_url: &str,
    buffer: &'buffer mut [u8],
) -> Result<&'buffer [u8], crate::IpcError> {
    struct BorrowedRecovery<'url> {
        window: WindowRef,
        document_generation: u32,
        native_url: &'url str,
    }
    impl Serialize for BorrowedRecovery<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeStructVariant as _;
            let mut value =
                serializer.serialize_struct_variant("WindowEvent", 10, "DocumentRecovered", 3)?;
            value.serialize_field("window", &self.window)?;
            value.serialize_field("document_generation", &self.document_generation)?;
            value.serialize_field("native_url", self.native_url)?;
            value.end()
        }
    }
    crate::codec::encode_into(
        &BorrowedRecovery {
            window,
            document_generation,
            native_url,
        },
        buffer,
    )
}
