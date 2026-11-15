//! Host-owned macOS OAuth code-flow primitives (KEL-89/T3).
//!
//! Call only after the owning dispatcher has obtained guard Allow.
//! This module owns no socket, browser, account registry, or permission policy.
//! The integration owns socket deadlines and verifies the HTTP callback path
//! against `LoopbackRedirect::callback_path` before passing the query here.

#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::fmt;

use objc2_foundation::{NSString, NSURLComponents};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest as _, Sha256};

use super::{ProviderProfile, RedirectForm};

/// Encoded URL/form/query envelope reusing the existing KIPC frame cap.
///
/// This is a cold processing bound, not an OAuth-scope authorization policy.
pub(super) const MAX_OAUTH_TEXT_BYTES: usize = keld_ipc::MAX_FRAME_LEN;
const MAX_LOGIN_HINT_BYTES: usize = 256;
const MAX_PROVIDER_TEXT_BYTES: usize = 256;

/// Local preparation/correlation failure; diagnostics never retain secrets.
///
/// `AUTH-016` implements the independently reviewed local-processing contract
/// recorded with the spec and registry. Source review does not prove physical
/// reachability of every condition.
pub(super) enum PrimitiveError {
    /// OS cryptographic entropy was unavailable.
    Entropy,
    /// Bounded encoding/decoding could not service this input.
    InputLimit,
    /// The fixed redirect has no usable exact loopback port/URI.
    RedirectMode,
    /// The fixed URI is invalid or does not name an HTTP loopback destination.
    RedirectConstruction,
    /// The actual bound port is inconsistent with the selected plan.
    BoundPort,
    /// The accepted base64 facility could not encode the fixed 32-byte input.
    Encoding,
    /// The login hint exceeds its UTF-8 byte bound.
    LoginHintLength,
    /// The login hint contains a Unicode control.
    LoginHintControl,
    /// The callback is malformed, ambiguous, or fails exact state correlation.
    Correlation,
    /// A correlated callback carries the provider's sanitized OAuth refusal.
    ProviderCallback {
        /// Sanitized, bounded provider error; never selects a Keld code path.
        error: String,
        /// Sanitized, bounded optional description.
        description: Option<String>,
    },
}

impl PrimitiveError {
    /// Registered failure code within the reviewed local-processing condition union.
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::RedirectMode => "KELD-AUTH-003",
            Self::LoginHintLength | Self::LoginHintControl => "KELD-AUTH-010",
            Self::Correlation => "KELD-AUTH-011",
            Self::ProviderCallback { .. } => "KELD-AUTH-012",
            Self::Entropy
            | Self::InputLimit
            | Self::BoundPort
            | Self::Encoding
            | Self::RedirectConstruction => "KELD-AUTH-016",
        }
    }
}

impl fmt::Debug for PrimitiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrimitiveError")
            .field("code", &self.code())
            .finish_non_exhaustive()
    }
}

impl fmt::Display for PrimitiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RedirectMode => f.write_str(
                "KELD-AUTH-003: exact redirect mode requires one registered HTTP loopback URI with an explicit nonzero fixed port. Configure fixedRedirect with that port; bind-0 is not a fallback.",
            ),
            Self::LoginHintLength => f.write_str(
                "KELD-AUTH-010: login_hint exceeds 256 UTF-8 bytes. Supply a hint within this bound.",
            ),
            Self::LoginHintControl => f.write_str(
                "KELD-AUTH-010: login_hint contains a Unicode control character. Remove control characters from the hint.",
            ),
            Self::Correlation => f.write_str(
                "KELD-AUTH-011: callback correlation failed; the callback was discarded. Restart with a fresh user-initiated auth.begin.",
            ),
            Self::ProviderCallback { error, description } => {
                write!(f, "KELD-AUTH-012: the IdP refused authorization with error {error:?}")?;
                if let Some(description) = description {
                    write!(f, ", description {description:?}")?;
                }
                f.write_str(". This is the IdP's decision. Start a fresh user-initiated auth.begin after resolving the refusal.")
            }
            Self::Entropy => f.write_str(
                "KELD-AUTH-016: host auth processing failed because OS randomness is unavailable. Restore the host random source, then start a fresh user-initiated auth.begin.",
            ),
            Self::InputLimit => f.write_str(
                "KELD-AUTH-016: host auth processing failed because input exceeds the existing KIPC frame envelope. Supply a bounded auth request, then start a fresh user-initiated auth.begin.",
            ),
            Self::BoundPort => f.write_str(
                "KELD-AUTH-016: host auth processing failed because the listener port is inconsistent with its redirect plan. Correct host bind/redirect construction before starting a fresh sign-in.",
            ),
            Self::Encoding => f.write_str(
                "KELD-AUTH-016: host auth processing failed during fixed-size secret encoding. Correct the host encoder before starting a fresh sign-in.",
            ),
            Self::RedirectConstruction => f.write_str(
                "KELD-AUTH-016: host auth processing failed because fixedRedirect is not a valid HTTP loopback URI. Configure a valid loopback registration without user info or fragment, then start a fresh sign-in.",
            ),
        }
    }
}

impl std::error::Error for PrimitiveError {}

/// Sensitive outgoing URL/form, whose `Debug` never prints its bytes.
pub(super) struct OutgoingText(String);

impl OutgoingText {
    /// Borrow for the actual browser/HTTPS owner; never log this value.
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OutgoingText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OutgoingText([redacted])")
    }
}

/// Host-minted single-use verifier/state for one pending authorization.
pub(super) struct PendingAuthorization {
    verifier: String,
    state: String,
    challenge: String,
}

impl fmt::Debug for PendingAuthorization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PendingAuthorization([redacted])")
    }
}

impl PendingAuthorization {
    /// Mint two independent 256-bit secrets with one fallible OS entropy call.
    pub(super) fn generate() -> Result<Self, PrimitiveError> {
        let mut random = [0_u8; 64];
        getrandom::fill(&mut random).map_err(|_| PrimitiveError::Entropy)?;
        let mut verifier_bytes = [0_u8; 32];
        let mut state_bytes = [0_u8; 32];
        verifier_bytes.copy_from_slice(&random[..32]);
        state_bytes.copy_from_slice(&random[32..]);
        Self::from_entropy(verifier_bytes, state_bytes)
    }

    fn from_entropy(
        verifier_bytes: [u8; 32],
        state_bytes: [u8; 32],
    ) -> Result<Self, PrimitiveError> {
        let verifier = base64_secret(&verifier_bytes)?;
        let state = base64_secret(&state_bytes)?;
        let challenge = s256_challenge(&verifier)?;
        Ok(Self {
            verifier,
            state,
            challenge,
        })
    }

    /// Construct the profile's authorization URL after Allow and actual binding.
    ///
    /// Scope order/duplicates are preserved; no guard or OAuth maximum-scope
    /// policy is implemented here. Validation of `login_hint` is post-Allow.
    pub(super) fn authorization_url(
        &self,
        profile: &ProviderProfile,
        client_id: &str,
        redirect: &LoopbackRedirect,
        scopes: Option<&[String]>,
        login_hint: Option<&str>,
    ) -> Result<OutgoingText, PrimitiveError> {
        self.authorization_uri(profile, client_id, &redirect.uri, scopes, login_hint)
    }
    /// Same OAuth form owner for the reviewed AS registration literal.
    pub(super) fn authorization_uri(
        &self,
        profile: &ProviderProfile,
        client_id: &str,
        redirect_uri: &str,
        scopes: Option<&[String]>,
        login_hint: Option<&str>,
    ) -> Result<OutgoingText, PrimitiveError> {
        validate_login_hint(login_hint)?;
        let mut form = FormBuilder::new();
        form.field("response_type", "code")?;
        form.field("client_id", client_id)?;
        form.field("redirect_uri", redirect_uri)?;
        if let Some(scopes) = scopes
            && !scopes.is_empty()
        {
            form.scopes(scopes)?;
        }
        if let Some(hint) = login_hint {
            form.field("login_hint", hint)?;
        }
        form.field("state", &self.state)?;
        form.field("code_challenge", &self.challenge)?;
        form.field("code_challenge_method", "S256")?;
        let form = form.finish();
        let length = profile
            .authorization_endpoint
            .len()
            .checked_add(1)
            .and_then(|length| length.checked_add(form.len()))
            .ok_or(PrimitiveError::InputLimit)?;
        if length > MAX_OAUTH_TEXT_BYTES {
            return Err(PrimitiveError::InputLimit);
        }
        let mut url = String::with_capacity(length);
        url.push_str(profile.authorization_endpoint);
        url.push('?');
        url.push_str(&form);
        Ok(OutgoingText(url))
    }

    /// Consume one flow and accept only its exactly correlated callback query.
    ///
    /// Missing/mismatched/duplicate state discards this flow. Even an error
    /// callback must correlate before any provider diagnostic is returned.
    /// Unknown response parameters are ignored per RFC 6749 §4.1.2.
    pub(super) fn correlate_callback(
        self,
        query: &str,
    ) -> Result<CorrelatedAuthorization, PrimitiveError> {
        let parameters = callback_parameters(query)?;
        let received_state = parameters.get("state").ok_or(PrimitiveError::Correlation)?;
        if !equal_state(received_state.as_bytes(), self.state.as_bytes()) {
            return Err(PrimitiveError::Correlation);
        }
        match (parameters.get("code"), parameters.get("error")) {
            (Some(code), None) if !code.is_empty() => Ok(CorrelatedAuthorization {
                code: code.clone(),
                verifier: self.verifier,
            }),
            (None, Some(error)) if !error.is_empty() => Err(PrimitiveError::ProviderCallback {
                error: sanitize_provider_text(error),
                description: parameters
                    .get("error_description")
                    .map(|value| sanitize_provider_text(value)),
            }),
            _ => Err(PrimitiveError::Correlation),
        }
    }
}

/// Code and matching verifier obtained by consuming a correlated pending flow.
///
/// No `Clone`/`Serialize` implementation can turn this into replayable shared state.
pub(super) struct CorrelatedAuthorization {
    code: String,
    verifier: String,
}

impl fmt::Debug for CorrelatedAuthorization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CorrelatedAuthorization([redacted])")
    }
}

impl CorrelatedAuthorization {
    /// Consume the code/verifier once into the authorization-code token form.
    pub(super) fn token_form(
        self,
        client_id: &str,
        redirect: &LoopbackRedirect,
    ) -> Result<OutgoingText, PrimitiveError> {
        self.token_form_uri(client_id, &redirect.uri)
    }
    /// Consume the same code/verifier with the exact reviewed AS redirect value.
    pub(super) fn token_form_uri(
        self,
        client_id: &str,
        redirect_uri: &str,
    ) -> Result<OutgoingText, PrimitiveError> {
        let mut form = FormBuilder::new();
        form.field("grant_type", "authorization_code")?;
        form.field("client_id", client_id)?;
        form.field("code", &self.code)?;
        form.field("redirect_uri", redirect_uri)?;
        form.field("code_verifier", &self.verifier)?;
        Ok(OutgoingText(form.finish()))
    }
}

/// Exact optional operator registration; no provider selection or URI synthesis.
pub(super) struct AsRedirect {
    uri: String,
    scheme: String,
}
impl fmt::Debug for AsRedirect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AsRedirect([redacted])")
    }
}
impl AsRedirect {
    pub(super) fn new(value: &str, client_id: &str) -> Option<Self> {
        if value.len() > MAX_OAUTH_TEXT_BYTES || client_id.len() > MAX_OAUTH_TEXT_BYTES {
            return None;
        }
        let scheme = format!("msal{client_id}");
        if !scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
            || value != format!("{scheme}://auth")
        {
            return None;
        }
        Some(Self {
            uri: value.to_owned(),
            scheme,
        })
    }
    pub(super) fn uri(&self) -> &str {
        &self.uri
    }
    pub(super) fn scheme(&self) -> &str {
        &self.scheme
    }
    pub(super) fn correlate(
        &self,
        pending: PendingAuthorization,
        callback: &str,
    ) -> Result<CorrelatedAuthorization, PrimitiveError> {
        if callback.contains('#') {
            return Err(PrimitiveError::Correlation);
        }
        let (base, query) = callback
            .split_once('?')
            .ok_or(PrimitiveError::Correlation)?;
        // Entra may append exactly one slash to its empty-path AS return.
        // Only this exact registered destination representation is compatible;
        // query/state/response correlation below remains strict and single-use.
        if base != self.uri && base.strip_suffix('/') != Some(self.uri.as_str()) {
            return Err(PrimitiveError::Correlation);
        }
        pending.correlate_callback(query)
    }
}

/// Pre-bind selection of dynamic or exact fixed loopback registration.
pub(super) struct RedirectPlan {
    fixed: Option<(LoopbackRedirect, u16)>,
    dynamic_form: RedirectForm,
}

impl RedirectPlan {
    /// Validate fixed registration before any socket is attempted.
    pub(super) fn new(
        profile: &ProviderProfile,
        fixed: Option<&str>,
    ) -> Result<Self, PrimitiveError> {
        let fixed = fixed.map(parse_fixed_redirect).transpose()?;
        Ok(Self {
            fixed,
            dynamic_form: profile.redirect_form,
        })
    }

    /// Port the real socket owner must request: zero only for dynamic mode.
    pub(super) fn bind_port(&self) -> u16 {
        self.fixed.as_ref().map_or(0, |(_, port)| *port)
    }

    /// Record the actual bound port and form the URI sent on both OAuth legs.
    ///
    /// The socket owner separately maps an actual bind failure to `AUTH-004`.
    pub(super) fn complete(self, bound_port: u16) -> Result<LoopbackRedirect, PrimitiveError> {
        if bound_port == 0 {
            return Err(PrimitiveError::BoundPort);
        }
        match self.fixed {
            Some((redirect, port)) if port == bound_port => Ok(redirect),
            Some(_) => Err(PrimitiveError::BoundPort),
            None => {
                let host = match self.dynamic_form {
                    RedirectForm::Ipv4Loopback => "127.0.0.1",
                    RedirectForm::Localhost => "localhost",
                };
                Ok(LoopbackRedirect {
                    uri: format!("http://{host}:{bound_port}/callback"),
                    path: "/callback".to_owned(),
                    query: None,
                    host: self.dynamic_form,
                })
            }
        }
    }
}

/// Exact supplied redirect plus `Foundation`-parsed encoded callback components.
pub(super) struct LoopbackRedirect {
    uri: String,
    path: String,
    query: Option<String>,
    host: RedirectForm,
}

impl LoopbackRedirect {
    /// Exact URI the authorization URL and token form share.
    #[cfg(test)]
    pub(super) fn uri(&self) -> &str {
        &self.uri
    }

    /// Percent-encoded path for the owning HTTP callback reader.
    ///
    /// Do not percent-decode or normalize it into a different route.
    #[cfg(test)]
    pub(super) fn callback_path(&self) -> &str {
        &self.path
    }

    /// Parsed loopback hostname kind; binding still targets the loopback address.
    pub(super) fn host(&self) -> RedirectForm {
        self.host
    }

    /// Extract the callback query from an exact encoded HTTP request target.
    ///
    /// A registered static query prefix must survive unchanged. The entire
    /// query is returned, so duplicate state/result names across static and
    /// provider parameters remain subject to the one correlation parser.
    pub(super) fn callback_query<'a>(
        &self,
        request_target: &'a str,
    ) -> Result<&'a str, PrimitiveError> {
        if request_target.len() > MAX_OAUTH_TEXT_BYTES {
            return Err(PrimitiveError::InputLimit);
        }
        let (path, query) = request_target
            .split_once('?')
            .ok_or(PrimitiveError::Correlation)?;
        if path != self.path || request_target.contains('#') {
            return Err(PrimitiveError::Correlation);
        }
        if let Some(registered) = self.query.as_deref().filter(|query| !query.is_empty()) {
            let remainder = query
                .strip_prefix(registered)
                .ok_or(PrimitiveError::Correlation)?;
            if !remainder.is_empty() && !remainder.starts_with('&') {
                return Err(PrimitiveError::Correlation);
            }
        }
        Ok(query)
    }
}

fn parse_fixed_redirect(value: &str) -> Result<(LoopbackRedirect, u16), PrimitiveError> {
    validate_raw_uri(value)?;
    let text = NSString::from_str(value);
    // The older safe Foundation parser avoids imposing a macOS 14 baseline.
    // Lexical validation above rejects raw invalid URI bytes/percent escapes
    // before Foundation could repair them; Foundation owns all component parsing.
    let components =
        NSURLComponents::componentsWithString(&text).ok_or(PrimitiveError::RedirectConstruction)?;
    // Reject parser repairs rather than replacing the registered literal.
    if components
        .string()
        .is_none_or(|parsed| parsed.to_string() != value)
    {
        return Err(PrimitiveError::RedirectConstruction);
    }
    let scheme = components
        .scheme()
        .ok_or(PrimitiveError::RedirectConstruction)?;
    let host = components
        .host()
        .ok_or(PrimitiveError::RedirectConstruction)?;
    if !scheme.to_string().eq_ignore_ascii_case("http")
        || components.user().is_some()
        || components.password().is_some()
        || components.fragment().is_some()
        || components.URL().is_none()
    {
        return Err(PrimitiveError::RedirectConstruction);
    }
    let host = match host.to_string().as_str() {
        value if value.eq_ignore_ascii_case("localhost") => RedirectForm::Localhost,
        "127.0.0.1" => RedirectForm::Ipv4Loopback,
        _ => return Err(PrimitiveError::RedirectConstruction),
    };
    let port = components.port().ok_or(PrimitiveError::RedirectMode)?;
    let port =
        u16::try_from(port.longLongValue()).map_err(|_| PrimitiveError::RedirectConstruction)?;
    if port == 0 {
        return Err(PrimitiveError::RedirectMode);
    }
    let path = components
        .percentEncodedPath()
        .map_or_else(|| "/".to_owned(), |value| value.to_string());
    let path = if path.is_empty() {
        "/".to_owned()
    } else {
        path
    };
    let query = components
        .percentEncodedQuery()
        .map(|value| value.to_string());
    Ok((
        LoopbackRedirect {
            uri: value.to_owned(),
            path,
            query,
            host,
        },
        port,
    ))
}

/// Validate only URI lexical bytes/escapes; `Foundation` remains the full parser.
fn validate_raw_uri(value: &str) -> Result<(), PrimitiveError> {
    if value.len() > MAX_OAUTH_TEXT_BYTES {
        return Err(PrimitiveError::InputLimit);
    }
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            if bytes.next().and_then(hex_value).is_none()
                || bytes.next().and_then(hex_value).is_none()
            {
                return Err(PrimitiveError::RedirectConstruction);
            }
        } else if !byte.is_ascii_alphanumeric() && !b":/?#[]@!$&'()*+,;=-._~".contains(&byte) {
            return Err(PrimitiveError::RedirectConstruction);
        }
    }
    Ok(())
}

fn base64_secret(bytes: &[u8; 32]) -> Result<String, PrimitiveError> {
    let mut encoded = [0_u8; 43];
    let written = URL_SAFE_NO_PAD
        .encode_slice(bytes, &mut encoded)
        .map_err(|_| PrimitiveError::Encoding)?;
    let text = std::str::from_utf8(&encoded[..written]).map_err(|_| PrimitiveError::Encoding)?;
    Ok(text.to_owned())
}

fn s256_challenge(verifier: &str) -> Result<String, PrimitiveError> {
    let digest: [u8; 32] = Sha256::digest(verifier.as_bytes()).into();
    base64_secret(&digest)
}

/// Validate the hint after guard Allow and before entropy or OS side effects.
pub(super) fn validate_login_hint(value: Option<&str>) -> Result<(), PrimitiveError> {
    if let Some(value) = value {
        if value.len() > MAX_LOGIN_HINT_BYTES {
            return Err(PrimitiveError::LoginHintLength);
        }
        if value.chars().any(char::is_control) {
            return Err(PrimitiveError::LoginHintControl);
        }
    }
    Ok(())
}

fn equal_state(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

struct FormBuilder(String);

/// Encode one post-Allow refresh request using the single shared form owner.
///
/// Scope order and duplicates are passed through unchanged. The controller's
/// separate canonical single-flight key must never rewrite this vector.
/// Refresh material stays host-owned; the resulting text is never logged.
pub(super) fn refresh_token_form(
    client_id: &str,
    refresh_token: &str,
    scopes: Option<&[String]>,
) -> Result<OutgoingText, PrimitiveError> {
    let mut form = FormBuilder::new();
    form.field("grant_type", "refresh_token")?;
    form.field("client_id", client_id)?;
    form.field("refresh_token", refresh_token)?;
    if let Some(scopes) = scopes
        && !scopes.is_empty()
    {
        form.scopes(scopes)?;
    }
    Ok(OutgoingText(form.finish()))
}

impl FormBuilder {
    fn new() -> Self {
        Self(String::new())
    }

    fn start_field(&mut self, key: &str) -> Result<(), PrimitiveError> {
        if !self.0.is_empty() {
            append_bounded(&mut self.0, "&")?;
        }
        append_encoded(&mut self.0, key)?;
        append_bounded(&mut self.0, "=")
    }

    fn field(&mut self, key: &str, value: &str) -> Result<(), PrimitiveError> {
        self.start_field(key)?;
        append_encoded(&mut self.0, value)
    }

    fn scopes(&mut self, scopes: &[String]) -> Result<(), PrimitiveError> {
        self.start_field("scope")?;
        for (index, scope) in scopes.iter().enumerate() {
            if index != 0 {
                append_bounded(&mut self.0, "+")?;
            }
            append_encoded(&mut self.0, scope)?;
        }
        Ok(())
    }

    fn finish(self) -> String {
        self.0
    }
}

fn append_bounded(output: &mut String, value: &str) -> Result<(), PrimitiveError> {
    if output
        .len()
        .checked_add(value.len())
        .is_none_or(|length| length > MAX_OAUTH_TEXT_BYTES)
    {
        return Err(PrimitiveError::InputLimit);
    }
    output.push_str(value);
    Ok(())
}

fn append_encoded(output: &mut String, value: &str) -> Result<(), PrimitiveError> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                append_bounded(
                    output,
                    std::str::from_utf8(&[byte]).map_err(|_| PrimitiveError::Encoding)?,
                )?;
            }
            b' ' => append_bounded(output, "+")?,
            _ => {
                let escaped = [
                    b'%',
                    HEX[usize::from(byte >> 4)],
                    HEX[usize::from(byte & 0x0f)],
                ];
                append_bounded(
                    output,
                    std::str::from_utf8(&escaped).map_err(|_| PrimitiveError::Encoding)?,
                )?;
            }
        }
    }
    Ok(())
}

fn callback_parameters(query: &str) -> Result<BTreeMap<String, String>, PrimitiveError> {
    if query.len() > MAX_OAUTH_TEXT_BYTES {
        return Err(PrimitiveError::InputLimit);
    }
    if query.is_empty() || query.contains('#') {
        return Err(PrimitiveError::Correlation);
    }
    let mut parameters = BTreeMap::new();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_component(key)?;
        let value = decode_component(value)?;
        if key.is_empty() || parameters.insert(key, value).is_some() {
            return Err(PrimitiveError::Correlation);
        }
    }
    Ok(parameters)
}

fn decode_component(value: &str) -> Result<String, PrimitiveError> {
    let mut decoded = Vec::with_capacity(value.len());
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        decoded.push(match byte {
            b'+' => b' ',
            b'%' => {
                let high = bytes
                    .next()
                    .and_then(hex_value)
                    .ok_or(PrimitiveError::Correlation)?;
                let low = bytes
                    .next()
                    .and_then(hex_value)
                    .ok_or(PrimitiveError::Correlation)?;
                (high << 4) | low
            }
            _ => byte,
        });
    }
    String::from_utf8(decoded).map_err(|_| PrimitiveError::Correlation)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Sanitize one `IdP` diagnostic using the shared D18 control/truncation policy.
pub(super) fn sanitize_provider_text(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars().filter(|character| !character.is_control()) {
        if output.len() + character.len_utf8() > MAX_PROVIDER_TEXT_BYTES {
            while output.len() > MAX_PROVIDER_TEXT_BYTES - 3 {
                output.pop();
            }
            output.push_str("...");
            break;
        }
        output.push(character);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::provider_profile;

    fn fixture() -> PendingAuthorization {
        PendingAuthorization::from_entropy(
            [
                116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22,
                212, 37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
            ],
            [0; 32],
        )
        .expect("RFC verifier entropy, fixed test state")
    }

    fn redirect() -> LoopbackRedirect {
        RedirectPlan::new(provider_profile("entra-common").expect("profile"), None)
            .expect("dynamic plan")
            .complete(4321)
            .expect("actual bound port")
    }

    #[test]
    fn as_registration_keeps_one_literal_across_both_oauth_legs() {
        let raw = "msal12345678-1234-1234-1234-123456789abc://auth";
        let client = "12345678-1234-1234-1234-123456789abc";
        let redirect = AsRedirect::new(raw, client).expect("literal registered binding");
        let profile = provider_profile("entra-common").expect("closed profile");
        let pending = fixture();
        let url = pending
            .authorization_uri(profile, client, redirect.uri(), None, None)
            .expect("same form owner");
        assert!(
            url.as_str()
                .contains("redirect_uri=msal12345678-1234-1234-1234-123456789abc%3A%2F%2Fauth")
        );
        let callback =
            format!("{raw}?code=literal-code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        let form = redirect
            .correlate(pending, &callback)
            .expect("exact state/base")
            .token_form_uri(client, redirect.uri())
            .expect("same token form owner");
        assert!(
            form.as_str()
                .contains("redirect_uri=msal12345678-1234-1234-1234-123456789abc%3A%2F%2Fauth")
        );
        assert!(
            form.as_str()
                .contains("code_verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
        );
    }
    #[test]
    fn as_return_empty_path_slash_keeps_state_and_raw_oauth_binding() {
        let client = "12345678-1234-1234-1234-123456789abc";
        let raw = "msal12345678-1234-1234-1234-123456789abc://auth";
        let redirect = AsRedirect::new(raw, client).expect("literal registration");
        let pending = fixture();
        let authorization = pending
            .authorization_uri(
                provider_profile("entra-common").expect("closed profile"),
                client,
                redirect.uri(),
                None,
                None,
            )
            .expect("existing authorization form");
        assert!(
            authorization
                .as_str()
                .contains("redirect_uri=msal12345678-1234-1234-1234-123456789abc%3A%2F%2Fauth")
        );
        let callback =
            format!("{raw}/?code=literal-code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        let form = redirect
            .correlate(pending, &callback)
            .expect("provider-added empty-path slash")
            .token_form_uri(client, redirect.uri())
            .expect("same raw registration");
        assert!(
            form.as_str()
                .contains("redirect_uri=msal12345678-1234-1234-1234-123456789abc%3A%2F%2Fauth&")
        );
        assert!(
            !form
                .as_str()
                .contains("redirect_uri=msal12345678-1234-1234-1234-123456789abc%3A%2F%2Fauth%2F")
        );
        assert!(
            form.as_str()
                .contains("code_verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
        );
        for callback in [
            format!("{raw}/?code=code&state=wrong"),
            format!("{raw}/?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            format!("{raw}/?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA#fragment"),
            format!("{raw}//?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            format!("{raw}/other?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            "msalother://auth/?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
            "msal12345678-1234-1234-1234-123456789abc://auth.evil/?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
            "msal12345678-1234-1234-1234-123456789abc://user@auth/?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
        ] {
            assert!(matches!(redirect.correlate(fixture(), &callback), Err(PrimitiveError::Correlation)));
        }
        assert!(
            AsRedirect::new(&format!("{raw}/"), client).is_none(),
            "outgoing registration itself stays exact"
        );
    }

    #[test]
    fn as_registration_and_callback_reject_aliases_and_wrong_state() {
        let client = "12345678-1234-1234-1234-123456789abc";
        let raw = "msal12345678-1234-1234-1234-123456789abc://auth";
        for invalid in [
            "",
            "https://example.invalid/auth",
            "msalother://auth",
            "msal12345678-1234-1234-1234-123456789abc://auth/",
        ] {
            assert!(AsRedirect::new(invalid, client).is_none());
        }
        let redirect = AsRedirect::new(raw, client).expect("binding");
        for callback in [
            format!("{raw}?code=code&state=wrong"),
            format!("{raw}/other?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            format!("{raw}?code=code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA#fragment"),
        ] {
            assert!(matches!(
                redirect.correlate(fixture(), &callback),
                Err(PrimitiveError::Correlation)
            ));
        }
    }

    #[test]
    fn rfc7636_appendix_b_verifier_and_s256_challenge_are_literal() {
        let entropy = [
            116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212,
            37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
        ];
        let pending = PendingAuthorization::from_entropy(entropy, [0; 32]).expect("RFC entropy");
        assert_eq!(
            pending.verifier,
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        );
        assert_eq!(
            pending.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(pending.state, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    }

    #[test]
    fn actual_entropy_mints_independent_secret_shapes() {
        let first = PendingAuthorization::generate().expect("actual OS entropy");
        let second = PendingAuthorization::generate().expect("actual OS entropy");
        assert_ne!(first.state, first.verifier);
        assert_ne!(first.state, second.state);
        assert_ne!(first.verifier, second.verifier);
        assert_ne!(first.challenge, second.challenge);
        for value in [&first.state, &first.verifier, &first.challenge] {
            assert_eq!(value.len(), 43);
            assert!(
                value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            );
        }
        assert_eq!(format!("{first:?}"), "PendingAuthorization([redacted])");
    }

    #[test]
    fn rfc6749_appendix_b_form_encoding_is_literal_and_bounded() {
        let mut output = String::new();
        append_encoded(&mut output, " %&+£€").expect("RFC vector");
        assert_eq!(output, "+%25%26%2B%C2%A3%E2%82%AC");
        assert_eq!(
            decode_component("+%25%26%2B%C2%A3%E2%82%AC").expect("RFC decode"),
            " %&+£€"
        );
        assert_eq!(MAX_OAUTH_TEXT_BYTES, keld_ipc::MAX_FRAME_LEN);
        assert_eq!(MAX_OAUTH_TEXT_BYTES, 16_777_216);
        let mut full = FormBuilder::new();
        full.field("a", &"x".repeat(16_777_214))
            .expect("exact envelope");
        assert_eq!(full.finish().len(), 16_777_216);
        let mut over = FormBuilder::new();
        assert!(matches!(
            over.field("a", &"x".repeat(16_777_215)),
            Err(PrimitiveError::InputLimit)
        ));
        for malformed in ["%", "%0", "%GG", "%C0%AF", "%ED%A0%80", "%FF"] {
            assert!(
                matches!(
                    decode_component(malformed),
                    Err(PrimitiveError::Correlation)
                ),
                "{malformed}"
            );
        }
    }

    #[test]
    fn profile_url_has_exact_redirect_s256_and_encoded_hint_without_scope_filter() {
        let pending = fixture();
        let scopes = [
            "scope.b".to_owned(),
            "scope.a".to_owned(),
            "scope.b".to_owned(),
        ];
        let url = pending
            .authorization_url(
                provider_profile("entra-common").expect("profile"),
                "client&x=1",
                &redirect(),
                Some(&scopes),
                Some("ñ +&"),
            )
            .expect("authorization URL");
        assert_eq!(
            url.as_str(),
            concat!(
                "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?",
                "response_type=code&client_id=client%26x%3D1&redirect_uri=http%3A%2F%2Flocalhost%3A4321%2Fcallback",
                "&scope=scope.b+scope.a+scope.b&login_hint=%C3%B1+%2B%26",
                "&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "&code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
                "&code_challenge_method=S256",
            )
        );
        assert_eq!(format!("{url:?}"), "OutgoingText([redacted])");
        assert!(validate_login_hint(Some(&"é".repeat(128))).is_ok());
        assert!(matches!(
            validate_login_hint(Some(&"é".repeat(129))),
            Err(PrimitiveError::LoginHintLength)
        ));
        for scalar in (0_u32..=31).chain(127..=159) {
            let hint = char::from_u32(scalar).expect("C0/DEL/C1").to_string();
            let error = validate_login_hint(Some(&hint)).expect_err("control hint");
            assert_eq!(error.code(), "KELD-AUTH-010");
            assert!(!error.to_string().chars().any(char::is_control));
        }
    }

    #[test]
    fn redirects_bind_dynamic_zero_or_exact_fixed_port_without_fallback() {
        let profile = provider_profile("entra-common").expect("profile");
        let dynamic = RedirectPlan::new(profile, None).expect("dynamic");
        assert_eq!(dynamic.bind_port(), 0);
        let bound = dynamic.complete(4321).expect("actual port");
        assert_eq!(bound.uri(), "http://localhost:4321/callback");
        assert_eq!(bound.callback_path(), "/callback");
        let fixed =
            RedirectPlan::new(profile, Some("http://localhost:5432/registered")).expect("fixed");
        assert_eq!(fixed.bind_port(), 5432);
        let bound = fixed.complete(5432).expect("fixed port");
        assert_eq!(bound.uri(), "http://localhost:5432/registered");
        assert_eq!(bound.callback_path(), "/registered");
        let mismatch =
            RedirectPlan::new(profile, Some("http://localhost:5432/registered")).expect("fixed");
        assert!(matches!(
            mismatch.complete(4321),
            Err(PrimitiveError::BoundPort)
        ));
        for no_port in ["http://localhost/callback", "http://localhost:0/callback"] {
            let error = RedirectPlan::new(profile, Some(no_port))
                .err()
                .expect("fixed port required");
            assert!(matches!(error, PrimitiveError::RedirectMode));
            assert_eq!(error.code(), "KELD-AUTH-003");
        }
        for invalid in [
            "http://localhost:65536/callback",
            "https://localhost:4321/callback",
            "http://evil.test:4321/callback",
            "http://localhost@evil.test:4321/callback",
            "http://[::1]:4321/callback",
            "http://localhost:4321/callback#x",
            "http://localhost:4321/a%",
            "http://localhost:4321/a%0",
            "http://localhost:4321/a%GG",
            "http://localhost:4321/a b",
        ] {
            let error = RedirectPlan::new(profile, Some(invalid))
                .err()
                .expect("invalid fixed URI");
            assert!(
                matches!(error, PrimitiveError::RedirectConstruction),
                "{invalid}"
            );
            assert_eq!(error.code(), "KELD-AUTH-016");
        }
        let synthetic = ProviderProfile {
            redirect_form: RedirectForm::Ipv4Loopback,
            ..*profile
        };
        let ipv4 = RedirectPlan::new(&synthetic, None)
            .expect("synthetic default")
            .complete(4321)
            .expect("actual port");
        assert_eq!(ipv4.uri(), "http://127.0.0.1:4321/callback");
    }

    #[test]
    fn foundation_preserves_registered_percent_path_query_and_both_oauth_legs() {
        let profile = provider_profile("entra-common").expect("profile");
        let registered = "http://localhost:5432/a%2Fb;v=1/c?tenant=a%26b";
        let plan = RedirectPlan::new(profile, Some(registered)).expect("valid registered URI");
        assert_eq!(plan.bind_port(), 5432);
        let bound = plan.complete(5432).expect("actual fixed bind port");
        assert_eq!(bound.host(), RedirectForm::Localhost);
        assert_eq!(bound.uri(), registered);
        assert_eq!(bound.callback_path(), "/a%2Fb;v=1/c");
        assert_eq!(bound.query.as_deref(), Some("tenant=a%26b"));
        let target = concat!(
            "/a%2Fb;v=1/c?tenant=a%26b&code=C",
            "&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        let query = bound.callback_query(target).expect("exact encoded target");
        assert!(
            bound
                .callback_query("/a/b;v=1/c?tenant=a%26b&code=C")
                .is_err()
        );
        assert!(
            bound
                .callback_query("/a%2Fb;v=1/c?tenant=other&code=C")
                .is_err()
        );
        assert!(
            bound
                .callback_query("/a%2Fb;v=1/c?tenant=a%26bad&code=C")
                .is_err()
        );
        let pending = fixture();
        let expected_redirect_parameter = concat!(
            "redirect_uri=http%3A%2F%2Flocalhost%3A5432%2Fa%252Fb%3Bv%3D1%2Fc",
            "%3Ftenant%3Da%2526b",
        );
        let url = pending
            .authorization_url(profile, "client", &bound, None, None)
            .expect("fixed authorization URL");
        assert!(url.as_str().contains(expected_redirect_parameter));
        let correlated = pending
            .correlate_callback(query)
            .expect("same pending state");
        let form = correlated
            .token_form("client", &bound)
            .expect("same exact token redirect");
        assert!(form.as_str().contains(expected_redirect_parameter));
        // Dot segments are accepted as URI syntax and retained, never rewritten.
        let dot = RedirectPlan::new(profile, Some("http://localhost:4321/a/../callback"))
            .expect("valid dot-segment URI")
            .complete(4321)
            .expect("fixed port");
        assert_eq!(dot.uri(), "http://localhost:4321/a/../callback");
        assert_eq!(dot.callback_path(), "/a/../callback");
        for exact in [
            "HTTP://LOCALHOST:4321/a%2fb;v=1?x=a%2bb&y=/?:@!$'()*+,;",
            "http://127.0.0.1:4321/a%2Fb",
            "http://localhost:4321?tenant=a%26b",
        ] {
            let redirect = RedirectPlan::new(profile, Some(exact))
                .expect("valid exact registered URI")
                .complete(4321)
                .expect("fixed port");
            assert_eq!(redirect.uri(), exact);
        }
    }

    #[test]
    fn fixed_bare_unknown_query_keeps_code_and_required_fields_strict() {
        let profile = provider_profile("entra-common").expect("profile");
        let registered = "http://localhost:5432/callback?tenant";
        let redirect = RedirectPlan::new(profile, Some(registered))
            .expect("registered bare query")
            .complete(5432)
            .expect("fixed port");
        assert_eq!(redirect.uri(), registered);
        assert_eq!(redirect.query.as_deref(), Some("tenant"));
        let query = redirect
            .callback_query(concat!(
                "/callback?tenant&code=C",
                "&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ))
            .expect("raw callback query retained");
        let correlated = fixture()
            .correlate_callback(query)
            .expect("ignore bare unknown tenant");
        assert_eq!(correlated.code, "C");
        for rejected in [
            "tenant&code=C&state",
            "tenant&code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "tenant&code=C&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&state",
            "tenant&code=C&code&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "tenant&bad=%GG&code=C&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "tenant&bad=%FF&code=C&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ] {
            let error = fixture()
                .correlate_callback(rejected)
                .expect_err("strict callback refusal");
            assert_eq!(error.code(), "KELD-AUTH-011");
        }
    }

    #[test]
    fn refresh_form_preserves_requested_vector_and_redacts_secret() {
        let scopes = [
            "scope.b".to_owned(),
            "scope.a".to_owned(),
            "scope.b".to_owned(),
        ];
        let form = refresh_token_form("client&x=1", "r+&", Some(&scopes)).expect("refresh form");
        assert_eq!(
            form.as_str(),
            concat!(
                "grant_type=refresh_token&client_id=client%26x%3D1&refresh_token=r%2B%26",
                "&scope=scope.b+scope.a+scope.b",
            )
        );
        assert_eq!(format!("{form:?}"), "OutgoingText([redacted])");
        for scopes in [None, Some(&[] as &[String])] {
            assert_eq!(
                refresh_token_form("client", "r", scopes)
                    .expect("empty scopes")
                    .as_str(),
                "grant_type=refresh_token&client_id=client&refresh_token=r"
            );
        }
    }

    #[test]
    fn callback_requires_one_exact_state_and_one_terminal_result() {
        let state = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for query in [
            "code=x".to_owned(),
            "code=x&state=unknown".to_owned(),
            format!("code=x&state={state}&state={state}"),
            format!("code=x&state={state}&st%61te={state}"),
            format!("code=x&code=y&state={state}"),
            format!("code=x&error=access_denied&state={state}"),
            format!("code=&state={state}"),
            format!("state={state}"),
            format!("code=x&state={state}&bad=%GG"),
            format!("code=x&state={state}#fragment"),
        ] {
            let error = fixture()
                .correlate_callback(&query)
                .expect_err("callback rejection");
            assert_eq!(error.code(), "KELD-AUTH-011", "{query}");
            assert!(!error.to_string().contains(state));
            assert!(!error.to_string().contains("unknown"));
        }
        let correlated = fixture()
            .correlate_callback(
                "unknown=ignored&code=a%2Bb%26c&state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            )
            .expect("correlated code");
        assert_eq!(
            format!("{correlated:?}"),
            "CorrelatedAuthorization([redacted])"
        );
        let form = correlated
            .token_form("client", &redirect())
            .expect("single-use exchange form");
        assert_eq!(
            form.as_str(),
            concat!(
                "grant_type=authorization_code&client_id=client&code=a%2Bb%26c",
                "&redirect_uri=http%3A%2F%2Flocalhost%3A4321%2Fcallback",
                "&code_verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
            )
        );
        assert_eq!(format!("{form:?}"), "OutgoingText([redacted])");
        assert!(matches!(
            callback_parameters(&"x".repeat(16_777_217)),
            Err(PrimitiveError::InputLimit)
        ));
    }

    #[test]
    fn correlated_provider_refusal_is_bounded_quoted_and_never_selects_own_code() {
        let query = concat!(
            "state=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "&error=KELD-AUTH-009%0Aaccess_denied&error_description=%C2%80bad%0Dtext",
        );
        let error = fixture()
            .correlate_callback(query)
            .expect_err("provider refusal");
        assert_eq!(error.code(), "KELD-AUTH-012");
        let text = error.to_string();
        assert!(text.starts_with("KELD-AUTH-012:"));
        assert!(text.contains("\"KELD-AUTH-009access_denied\""));
        assert!(text.contains("\"badtext\""));
        assert!(!text.chars().any(char::is_control));
        let truncated = sanitize_provider_text(&"é".repeat(200));
        assert!(truncated.len() <= 256);
        assert!(truncated.ends_with("..."));
        assert_eq!(
            format!("{error:?}"),
            "PrimitiveError { code: \"KELD-AUTH-012\", .. }"
        );
    }
}
