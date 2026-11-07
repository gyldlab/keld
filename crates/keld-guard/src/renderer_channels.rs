//! Exact renderer-channel queries in the single immutable Guard policy owner.

use serde::{Deserialize, Deserializer, de::Error as _};

use crate::{Decision, DenyReason, PermissionsManifest, Principal, ScopePermit, evaluate};

pub(super) fn deserialize_channels<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    let channels = Vec::<String>::deserialize(deserializer)?;
    if channels.iter().any(|channel| {
        channel
            .strip_prefix("el:")
            .is_none_or(|topic| !valid_topic(topic))
    }) {
        return Err(D::Error::custom(
            "window channels require exact el: literals with a nonempty suffix of at most 256 UTF-8 bytes and no wildcard or control characters",
        ));
    }
    Ok(channels)
}

fn valid_topic(topic: &str) -> bool {
    !topic.is_empty()
        && topic.len() <= 256
        && !topic.chars().any(|character| {
            character == '*' || matches!(character, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}')
        })
}

/// Borrowed window grants selected from one immutable session manifest.
///
/// Core resolves the key from its verified source/window association. This handle
/// proves only that selection: it does not authenticate a `WebView` or its document.
/// Missing keys select no grants. No page data can deserialize or rename a handle.
#[derive(Clone, Copy, Debug)]
pub struct BorrowedHostWindowGrants<'manifest> {
    manifest: &'manifest PermissionsManifest,
    entry: Option<(&'manifest str, &'manifest crate::window_csp::WindowPolicy)>,
}

impl BorrowedHostWindowGrants<'_> {
    /// Actual key borrowed from the snapshot, or `None` for a missing selection.
    #[must_use]
    pub fn window_key(&self) -> Option<&str> {
        self.entry.map(|(key, _)| key)
    }
}

impl PermissionsManifest {
    /// Selects the Host-resolved window's grants without allocating or copying.
    ///
    /// Production callers obtain `window_key` from Core's verified source and
    /// current native registry association, never from renderer-supplied metadata.
    #[must_use]
    pub fn window_channel_grants(&self, window_key: &str) -> BorrowedHostWindowGrants<'_> {
        BorrowedHostWindowGrants {
            manifest: self,
            entry: self
                .windows
                .get_key_value(window_key)
                .map(|(key, value)| (key.as_str(), value)),
        }
    }
}

/// Native capability inputs. The corresponding callback receives `ScopePermit`.
#[derive(Clone, Copy, Debug)]
pub struct NativeCapabilityQuery<'request> {
    /// Existing dotted native operation.
    pub operation: &'request str,
    /// Resource argument, including the existing native filesystem path grammar.
    pub path: &'request str,
}

/// Exact renderer topic inputs; no native capability or resource interpretation.
#[derive(Clone, Copy, Debug)]
pub struct RendererChannelQuery<'request, 'manifest> {
    /// Unprefixed decoded channel from the canonical bounded IPC codec.
    pub channel: &'request str,
    /// Selection resolved through Core's trusted source/window association.
    pub current_window: BorrowedHostWindowGrants<'manifest>,
}

/// Opaque channel-only proof, distinct from a native [`ScopePermit`].
///
/// It exposes no native scope index, conversion, clone or public constructor.
/// Final dispatch lends the proof only during its single effect callback.
///
/// A native permit cannot be manufactured from a channel permit:
///
/// ```compile_fail
/// fn into_native(channel: &keld_guard::RendererChannelPermit) -> &keld_guard::ScopePermit {
///     channel
/// }
/// ```
///
/// Nor can a channel permit be manufactured from a native permit:
///
/// ```compile_fail
/// fn into_channel(native: &keld_guard::ScopePermit) -> &keld_guard::RendererChannelPermit {
///     native
/// }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct RendererChannelPermit {
    _private: (),
}

mod sealed {
    pub trait Sealed {}
}

/// Closed set of pure Guard queries and their disjoint permit kinds.
///
/// Only the two owning query types implement this trait. It exports no effect
/// callback or dispatch method; privileged effects remain in `guard_dispatch`.
pub trait GuardQuery: sealed::Sealed + Copy {
    /// Opaque proof for this query kind only.
    type Permit;

    /// Pure decision implementation used by [`evaluate_query`].
    #[doc(hidden)]
    fn evaluate(
        self,
        manifest: &PermissionsManifest,
        principal: Principal,
    ) -> Decision<Self::Permit>;

    /// Native path inputs, if this is a native query. Renderer topics have none.
    #[doc(hidden)]
    fn native_capability(&self) -> Option<NativeCapabilityQuery<'_>>;
}

impl sealed::Sealed for NativeCapabilityQuery<'_> {}

impl GuardQuery for NativeCapabilityQuery<'_> {
    type Permit = ScopePermit;

    fn evaluate(self, manifest: &PermissionsManifest, principal: Principal) -> Decision {
        evaluate(manifest, principal, self.operation, self.path)
    }

    fn native_capability(&self) -> Option<NativeCapabilityQuery<'_>> {
        Some(*self)
    }
}

impl sealed::Sealed for RendererChannelQuery<'_, '_> {}

impl GuardQuery for RendererChannelQuery<'_, '_> {
    type Permit = RendererChannelPermit;

    fn evaluate(
        self,
        manifest: &PermissionsManifest,
        principal: Principal,
    ) -> Decision<RendererChannelPermit> {
        let nonzero_webview = matches!(
            principal,
            Principal::Webview { id, generation } if id != 0 && generation != 0
        );
        if nonzero_webview
            && std::ptr::eq(manifest, self.current_window.manifest)
            && valid_topic(self.channel)
            && self.current_window.entry.is_some_and(|(_, window)| {
                window
                    .channels
                    .iter()
                    .any(|literal| literal.strip_prefix("el:") == Some(self.channel))
            })
        {
            Decision::Allow(RendererChannelPermit { _private: () })
        } else {
            Decision::Deny(DenyReason::ChannelForbidden {
                channel: format!("el:{}", self.channel),
            })
        }
    }

    fn native_capability(&self) -> Option<NativeCapabilityQuery<'_>> {
        None
    }
}

/// Evaluates either typed query using its own opaque permit kind.
///
/// An Allow performs no heap allocation. A renderer principal's numeric shape
/// is not authentication: Core must prove native identity and current document/
/// role leases before preflight and immediately before the final effect dispatch.
#[must_use]
pub fn evaluate_query<Q: GuardQuery>(
    manifest: &PermissionsManifest,
    principal: Principal,
    query: Q,
) -> Decision<Q::Permit> {
    query.evaluate(manifest, principal)
}
