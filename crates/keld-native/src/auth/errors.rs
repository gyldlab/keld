//! One private controller error adapter; component code/Display owners reused.
use super::browser::BrowserError;
use super::loopback::LoopbackError;
use super::macos_http::HttpError;
use super::primitives::{PrimitiveError, sanitize_provider_text};
use super::registry::StateFault;
use super::token_reply::TokenReplyError;
use super::{PROVIDER_PROFILES, ProviderProfile};
use keld_ipc::CallError;
use std::fmt;
use std::sync::Arc;

pub(super) enum AuthError {
    Deadline,
    UnknownProvider(String),
    MissingClient(&'static ProviderProfile),
    InvalidAsRedirect(&'static ProviderProfile),
    AsCanceled,
    AsPresentationFailed,
    DeviceComplianceUnsupported,
    Primitive(PrimitiveError),
    State(StateFault),
    Http(HttpError),
    Loopback(LoopbackError),
    Browser(BrowserError),
    TokenReply(TokenReplyError),
}
impl AuthError {
    pub(super) fn unknown_provider(key: &str) -> Self {
        // Reuse the same bounded/control-free quoting input owner. The key is
        // still untrusted; Debug never exposes it or any payload value.
        Self::UnknownProvider(sanitize_provider_text(key))
    }
    pub(super) fn code(&self) -> &'static str {
        match self {
            Self::Deadline => "KELD-AUTH-008",
            Self::UnknownProvider(_) => "KELD-AUTH-002",
            Self::MissingClient(_) | Self::InvalidAsRedirect(_) => "KELD-AUTH-015",
            Self::AsCanceled => "KELD-AUTH-007",
            Self::AsPresentationFailed => "KELD-AUTH-017",
            Self::DeviceComplianceUnsupported => "KELD-AUTH-006",
            Self::Primitive(e) => e.code(),
            Self::State(e) => e.code(),
            Self::Http(_) => "KELD-AUTH-013",
            Self::Loopback(e) => e.code(),
            Self::Browser(e) => e.code(),
            Self::TokenReply(e) => e.code(),
        }
    }
    pub(super) fn wire(&self) -> Arc<CallError> {
        Arc::new(CallError {
            code: self.code().to_owned(),
            message: self.to_string(),
        })
    }
}
impl fmt::Debug for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthError")
            .field("code", &self.code())
            .finish_non_exhaustive()
    }
}
impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deadline => f.write_str("KELD-AUTH-008: the admitted authorization step did not complete within its absolute host bound. Start a fresh user-initiated auth.begin; no automatic retry occurs."),
            Self::UnknownProvider(key) => {
                write!(f, "KELD-AUTH-002: provider key {key:?} is not compiled into this host. Select one of")?;
                for profile in PROVIDER_PROFILES { write!(f, " {:?}", profile.key)?; }
                f.write_str(" in auth.begin; configuration cannot add a provider.")
            }
            Self::MissingClient(profile) => write!(f, "KELD-AUTH-015: provider profile {:?} has no clientId configured. Add auth.{}.clientId to keld.config.ts and rebuild the boot descriptor before starting a fresh sign-in.", profile.key, profile.key),
            Self::InvalidAsRedirect(profile) => write!(f, "KELD-AUTH-015: provider profile {:?} has an invalid asRedirect registration. Configure auth.{}.asRedirect as the exact msal clientId custom-scheme callback registration, rebuild, then start a fresh sign-in.", profile.key, profile.key),
            Self::AsCanceled => f.write_str("KELD-AUTH-007: the user canceled the authentication session. Start a fresh user-initiated auth.begin when ready to sign in."),
            Self::AsPresentationFailed => f.write_str("KELD-AUTH-017: macOS could not complete the authentication presentation. Ensure the Keld window remains available, then start a new user-initiated auth.begin; no automatic retry occurs."),
            Self::DeviceComplianceUnsupported => f.write_str("KELD-AUTH-006: Entra device-compliance Conditional Access requires the missing Intune Company Portal / Enterprise SSO broker. This v0 host does not integrate that broker and makes no device-compliance claim. Use a broker-capable managed-device application; an AS or external-browser fallback cannot satisfy this flow."),
            Self::Primitive(e) => fmt::Display::fmt(e, f), Self::State(e) => fmt::Display::fmt(e, f),
            Self::Http(e) => fmt::Display::fmt(e, f), Self::Loopback(e) => fmt::Display::fmt(e, f),
            Self::Browser(e) => fmt::Display::fmt(e, f), Self::TokenReply(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl std::error::Error for AuthError {}
