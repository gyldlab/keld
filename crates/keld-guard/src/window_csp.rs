//! The sole closed window response-policy catalog and immutable verified result.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, de::Error as _};

/// A named response policy. This is policy data, not native-effect qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCspProfile {
    /// Admitted local editor resources; no inline script/style or remote origin.
    OriginalEditorOffline,
    /// Explicit empty-stylesheet creation; later CSSOM rules remain ordinary
    /// script-controlled CSS, while all resource destinations stay restricted.
    OriginalEditorOfflineEmptyStyle,
}

impl WindowCspProfile {
    /// Exact selector accepted at `windows.<key>.web.csp`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OriginalEditorOffline => "original-editor-offline",
            Self::OriginalEditorOfflineEmptyStyle => "original-editor-offline-empty-style",
        }
    }

    const fn response_header(self) -> &'static str {
        match self {
            Self::OriginalEditorOffline => {
                "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'"
            }
            Self::OriginalEditorOfflineEmptyStyle => {
                "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='; font-src 'self' data:; media-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; form-action 'none'"
            }
        }
    }
}

impl<'de> Deserialize<'de> for WindowCspProfile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // String-only authoring rejects null and externally tagged enum objects.
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            value if value == Self::OriginalEditorOffline.as_str() => {
                Ok(Self::OriginalEditorOffline)
            }
            value if value == Self::OriginalEditorOfflineEmptyStyle.as_str() => {
                Ok(Self::OriginalEditorOfflineEmptyStyle)
            }
            _ => Err(D::Error::custom(
                "unknown window CSP selector; explicitly select a qualified original-editor-offline policy from the closed catalog",
            )),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct WindowPolicy {
    #[serde(
        default,
        deserialize_with = "super::renderer_channels::deserialize_channels"
    )]
    pub(super) channels: Vec<String>,
    #[serde(default)]
    pub(super) web: WindowWebPolicy,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WindowWebPolicy {
    #[serde(default, deserialize_with = "deserialize_present_csp")]
    pub(super) csp: Option<WindowCspProfile>,
}

fn deserialize_present_csp<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<WindowCspProfile>, D::Error> {
    WindowCspProfile::deserialize(deserializer).map(Some)
}

pub(super) fn validate_structure(value: &serde_json::Value) -> Result<(), &'static str> {
    let Some(windows) = value.get("windows") else {
        return Ok(());
    };
    let windows = windows
        .as_object()
        .ok_or("/windows must be an object keyed by trusted window policy names")?;
    for policy in windows.values() {
        // Serde also accepts a sequence for a struct; an empty sequence could
        // otherwise silently become an empty all-default window or web policy.
        let policy = policy
            .as_object()
            .ok_or("each /windows entry must be a policy object")?;
        if policy.get("web").is_some_and(|web| !web.is_object()) {
            return Err("window web policy must be an object with a qualified CSP selector");
        }
    }
    Ok(())
}

pub(super) fn validate_keys(windows: &BTreeMap<String, WindowPolicy>) -> Result<(), String> {
    if windows.keys().any(String::is_empty) {
        return Err(
            "window policy key is empty; name the trusted window policy under /windows".to_owned(),
        );
    }
    Ok(())
}

/// A window key and catalog policy resolved only from a digest-verified manifest.
///
/// This immutable value proves policy provenance, not native installation, current
/// document identity or physical CSP enforcement. Consumers retain their existing
/// native source/window lifecycle checks.
///
/// Arbitrary response text cannot construct a verified policy:
///
/// ```compile_fail
/// use keld_guard::{ResolvedWindowCsp, WindowCspProfile};
/// let _forged = ResolvedWindowCsp {
///     window_key: "main".into(),
///     verified_sha256: [0; 32],
///     profile: WindowCspProfile::OriginalEditorOffline,
/// };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedWindowCsp {
    window_key: String,
    verified_sha256: [u8; 32],
    profile: WindowCspProfile,
}

impl ResolvedWindowCsp {
    pub(super) fn new(
        window_key: &str,
        verified_sha256: [u8; 32],
        profile: WindowCspProfile,
    ) -> Self {
        Self {
            window_key: window_key.to_owned(),
            verified_sha256,
            profile,
        }
    }

    /// Actual manifest key selected by the trusted Host/Core mapping.
    #[must_use]
    pub fn window_key(&self) -> &str {
        &self.window_key
    }

    /// Digest verified over the exact immutable source manifest bytes.
    #[must_use]
    pub const fn verified_sha256(&self) -> [u8; 32] {
        self.verified_sha256
    }

    /// Exact immutable catalog choice from the verified manifest; not a setter.
    #[must_use]
    pub const fn profile(&self) -> WindowCspProfile {
        self.profile
    }

    /// The single catalog's exact response-header value, before content parsing.
    #[must_use]
    pub const fn response_header(&self) -> &'static str {
        self.profile.response_header()
    }
}
