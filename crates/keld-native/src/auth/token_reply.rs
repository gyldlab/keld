//! Bounded real token-endpoint JSON interpretation; no provider abstraction.
//! Call only on the actual `HttpOperation`'s completed/quiescent response.
use super::primitives::{MAX_OAUTH_TEXT_BYTES, sanitize_provider_text};
use super::registry::{AccessToken, StateFault};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

#[derive(Clone, Copy)]
pub(super) enum ExchangeMode {
    AuthorizationCode,
    Refresh,
}
#[derive(Deserialize)]
struct WireTokenReply {
    #[serde(default, deserialize_with = "present_value")]
    access_token: Option<String>,
    #[serde(default, deserialize_with = "present_value")]
    token_type: Option<String>,
    #[serde(default, deserialize_with = "present_value")]
    expires_in: Option<u64>,
    #[serde(default, deserialize_with = "present_value")]
    refresh_token: Option<String>,
    #[serde(default, deserialize_with = "present_value")]
    scope: Option<String>,
    #[serde(default, deserialize_with = "present_value")]
    error: Option<String>,
    #[serde(default, deserialize_with = "present_value")]
    error_description: Option<String>,
}
// One presence owner for all optional known fields: absent uses serde default,
// present calls the ACTUAL value type deserializer, so null is not absence.
fn present_value<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

// Preserve one derived DTO duplicate/type/unknown-field visitor, but constrain
// its outer protocol boundary to a JSON object. No Value/reparse/key erasure.
struct ObjectReply(WireTokenReply);
impl<'de> Deserialize<'de> for ObjectReply {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = WireTokenReply;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a token-endpoint JSON object")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                WireTokenReply::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        deserializer.deserialize_map(ObjectVisitor).map(Self)
    }
}
/// Owned host-only credentials from the actual validated token receipt.
pub(super) struct ParsedToken {
    pub(super) access: Arc<AccessToken>,
    pub(super) refresh: Option<String>,
    pub(super) granted_scopes: Vec<String>,
}
impl fmt::Debug for ParsedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedToken").finish_non_exhaustive()
    }
}
/// Raw exact OAuth error chooses the class before the shared sanitizer runs.
pub(super) enum TokenReplyError {
    Malformed,
    State(StateFault),
    Provider {
        terminal_refresh: bool,
        error: String,
        description: Option<String>,
    },
}
impl TokenReplyError {
    pub(super) fn code(&self) -> &'static str {
        match self {
            Self::State(error) => error.code(),
            Self::Provider {
                terminal_refresh: true,
                ..
            } => "KELD-AUTH-009",
            Self::Malformed
            | Self::Provider {
                terminal_refresh: false,
                ..
            } => "KELD-AUTH-013",
        }
    }
    pub(super) fn terminal_invalid_grant(&self) -> bool {
        matches!(
            self,
            Self::Provider {
                terminal_refresh: true,
                ..
            }
        )
    }
}
impl fmt::Debug for TokenReplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenReplyError")
            .field("code", &self.code())
            .finish_non_exhaustive()
    }
}
impl fmt::Display for TokenReplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("KELD-AUTH-013: the token exchange returned an unusable bounded JSON response. Resolve the provider response failure, then start a fresh user-initiated auth.begin; no automatic retry occurs."),
            Self::State(error) => fmt::Display::fmt(error, f),
            Self::Provider { terminal_refresh, error, description } => {
                write!(f, "{}: the token endpoint refused the exchange with provider error {error:?}", self.code())?;
                if let Some(description) = description { write!(f, ", description {description:?}")?; }
                if *terminal_refresh {
                    f.write_str(". The host session has ended and holds no durable credential. Start a fresh user-initiated auth.begin; no automatic retry occurs.")
                } else {
                    // T3 HTTP interaction_required remains013; Windows14 is not widened.
                    f.write_str(". This is the IdP's decision. Start a fresh user-initiated auth.begin after resolving the refusal; no automatic retry or automatic UI occurs.")
                }
            }
        }
    }
}
impl std::error::Error for TokenReplyError {}

pub(super) fn decode_token_reply(
    status: u16,
    body: &[u8],
    received_at: SystemTime,
    receipt_mono: Instant,
    requested: Option<&[String]>,
    mode: ExchangeMode,
) -> Result<ParsedToken, TokenReplyError> {
    if body.len() > MAX_OAUTH_TEXT_BYTES {
        return Err(TokenReplyError::Malformed);
    }
    // Existing pinned serde_json owns JSON parsing; unknown fields (including
    // iss) are ignored, known duplicates/types are rejected by this DTO.
    let ObjectReply(reply) =
        serde_json::from_slice::<ObjectReply>(body).map_err(|_| TokenReplyError::Malformed)?;
    if let Some(error) = reply.error {
        // RFC6749 section5.2: token refusals use400;401 is the client-
        // authentication special case, not authority to terminate a session.
        if status != 400 && !(status == 401 && error == "invalid_client") {
            return Err(TokenReplyError::Malformed);
        }
        if error.is_empty()
            || reply.access_token.is_some()
            || reply.token_type.is_some()
            || reply.expires_in.is_some()
            || reply.refresh_token.is_some()
        {
            return Err(TokenReplyError::Malformed);
        }
        let terminal_refresh = matches!(mode, ExchangeMode::Refresh) && error == "invalid_grant";
        return Err(TokenReplyError::Provider {
            terminal_refresh,
            error: sanitize_provider_text(&error),
            description: reply
                .error_description
                .as_deref()
                .map(sanitize_provider_text),
        });
    }
    if !(200..300).contains(&status) || reply.error_description.is_some() {
        return Err(TokenReplyError::Malformed);
    }
    let access = reply.access_token.ok_or(TokenReplyError::Malformed)?;
    let token_type = reply.token_type.ok_or(TokenReplyError::Malformed)?;
    let expires_in = reply.expires_in.ok_or(TokenReplyError::Malformed)?;
    if reply.refresh_token.as_ref().is_some_and(String::is_empty) {
        return Err(TokenReplyError::Malformed);
    }
    let granted_scopes = match reply.scope {
        Some(value) => {
            // RFC6749 §3.3/5.1 response grammar, not a request-scope ceiling.
            if value.is_empty()
                || !value.bytes().all(|b| {
                    b == b' '
                        || b == 0x21
                        || (0x23..=0x5b).contains(&b)
                        || (0x5d..=0x7e).contains(&b)
                })
            {
                return Err(TokenReplyError::Malformed);
            }
            let mut scopes = Vec::new();
            for scope in value.split(' ') {
                if scope.is_empty() {
                    return Err(TokenReplyError::Malformed);
                }
                scopes.push(scope.to_owned());
            }
            scopes
        }
        // RFC6749 §5.1 allows omission only when identical to requested scope.
        // No requested scope and no returned scope yields no invented grant.
        None => requested.unwrap_or(&[]).to_vec(),
    };
    let access = AccessToken::at_receipt(access, token_type, expires_in, received_at, receipt_mono)
        .map_err(TokenReplyError::State)?;
    Ok(ParsedToken {
        access,
        refresh: reply.refresh_token,
        granted_scopes,
    })
}

#[cfg(test)]
#[path = "token_reply_tests.rs"]
mod tests;
