//! Auth request, response, provider, and operator-configuration contracts.
//!
//! Spec: `docs/specs/keld-auth.md` §4. Payload declaration order is the
//! postcard contract; use `keld_ipc::codec` for encoding and decoding.
//! Configuration supplies credentials for the closed host-side profile table.
//! It cannot add a provider, change an issuer, or authorize a call.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Default auth.begin caller deadline in milliseconds, including owner cleanup margin.
/// Generated TypeScript reads this Native constant; it never copies a literal.
pub const AUTH_BEGIN_CALL_DEADLINE_MS: u32 = 330_000;
/// Default auth.token caller deadline in milliseconds, including owner cleanup margin.
/// Generated TypeScript reads this Native constant; it never copies a literal.
pub const AUTH_TOKEN_CALL_DEADLINE_MS: u32 = 60_000;

// One Native auth policy owner. Each phase captures absolute bounds once.
#[cfg(target_os = "macos")]
const BEGIN_HOST_TIMEOUT: std::time::Duration = std::time::Duration::from_mins(5);
#[cfg(target_os = "macos")]
const TOKEN_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
#[cfg(target_os = "macos")]
const OWNER_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(7);
#[cfg(target_os = "macos")]
const BROWSER_LAUNCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(target_os = "macos")]
mod macos_http;
#[cfg(target_os = "macos")]
mod macos_session;
#[cfg(target_os = "macos")]
pub use macos_session::{
    AsMainOwner, AsMainProgress, AsMainStop, AsWorkerPort, prepare_as_presentation,
};

#[cfg(target_os = "macos")]
mod registry;

#[cfg(target_os = "macos")]
mod primitives;

#[cfg(target_os = "macos")]
mod loopback;

#[cfg(target_os = "macos")]
mod browser;

#[cfg(target_os = "macos")]
mod controller;
#[cfg(target_os = "macos")]
mod errors;
#[cfg(target_os = "macos")]
mod token_reply;
#[cfg(target_os = "macos")]
pub use controller::{
    AuthBroker, HandleState, HostFault, OperationId, Progress, ReadyResponse, ResponseEvent,
};

/// Request payload for the auth channel.
///
/// These values are child-supplied. The dispatcher resolves the issuer and
/// authorizes the call before validating the other request fields or starting
/// side effects. `Debug` redacts the payload, including the login hint.
#[derive(Serialize, Deserialize)]
pub enum AuthRequest {
    /// Start an interactive flow at one compiled-in provider profile.
    Begin {
        /// Exact provider-profile key; never the guard's issuer resource.
        provider: String,
        /// Requested OAuth scopes, passed through without host policy filtering.
        scopes: Option<Vec<String>>,
        /// Advisory OIDC login hint, validated after Allow and never logged.
        login_hint: Option<String>,
    },
    /// Obtain an access token for a live host-session account.
    Token {
        /// Host-minted opaque session selector; never an authorization scope.
        account_id: String,
        /// Requested scopes, not compared against the establishing grant.
        scopes: Option<Vec<String>>,
    },
}

impl std::fmt::Debug for AuthRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Begin { .. } => "AuthRequest::Begin",
            Self::Token { .. } => "AuthRequest::Token",
        };
        formatter.debug_struct(name).finish_non_exhaustive()
    }
}

/// Successful auth response payload.
///
/// Refresh credentials, host handles, and session policy are never fields.
/// `Debug` redacts the payload, including access tokens.
#[derive(Clone, Serialize, Deserialize)]
pub enum AuthResponse {
    /// A successful interactive flow created a live host-session account.
    Begin {
        /// Opaque host-minted selector for this host session.
        account_id: String,
        /// Scopes granted by the provider during this flow.
        granted_scopes: Vec<String>,
    },
    /// A successful exchange returned a short-lived access token.
    Token {
        /// Access credential for the authorized issuer; never a refresh token.
        access_token: String,
        /// Expiry as unsigned whole UTC seconds since the UNIX epoch.
        ///
        /// T3's explicit public-contract choice: compute using checked addition
        /// of provider `expires_in` seconds to successful token-receipt `SystemTime`
        /// relative to `UNIX_EPOCH`. Do not infer JWT exp, saturate overflow, or
        /// default an invalid clock/expiry to zero. Internal token freshness
        /// uses a separate monotonic deadline. Account-session liveness is
        /// bounded by host exit or terminal `invalid_grant`, not access expiry.
        expires_at: u64,
        /// Provider-returned token type.
        token_type: String,
    },
}

impl std::fmt::Debug for AuthResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Begin { .. } => "AuthResponse::Begin",
            Self::Token { .. } => "AuthResponse::Token",
        };
        formatter.debug_struct(name).finish_non_exhaustive()
    }
}

/// Loopback redirect hostname selected by a compiled-in provider profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectForm {
    /// Default literal IPv4 loopback form, 127.0.0.1.
    Ipv4Loopback,
    /// Provider-declared localhost exception to the default form.
    Localhost,
}

/// Fixed issuer and transport parameters shared by every app at a profile.
///
/// Per-app client ids and fixed redirects belong in `AuthConfiguration`.
/// The auth dispatcher obtains profiles only through `provider_profile`.
#[derive(Debug)]
pub struct ProviderProfile {
    /// Exact lowercase ASCII kebab-case provider key.
    pub key: &'static str,
    /// Authorable issuer literal used for both auth guard resources.
    pub issuer: &'static str,
    /// Explicit authorization endpoint; never derived from the issuer.
    pub authorization_endpoint: &'static str,
    /// Explicit token endpoint; never derived from another endpoint.
    pub token_endpoint: &'static str,
    /// Recorded provider selection for the loopback redirect hostname.
    pub redirect_form: RedirectForm,
}

/// The closed v0 provider table, owned by the approved auth spec.
///
/// Adding or changing a row requires a spec and permission-model review.
pub const PROVIDER_PROFILES: &[ProviderProfile] = &[ProviderProfile {
    key: "entra-common",
    issuer: "https://login.microsoftonline.com/common",
    authorization_endpoint: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
    token_endpoint: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
    redirect_form: RedirectForm::Localhost,
}];

/// Resolves an exact key against the closed host-side profile table.
///
/// Unknown keys return `None`; operator configuration cannot introduce them.
#[must_use]
pub fn provider_profile(key: &str) -> Option<&'static ProviderProfile> {
    PROVIDER_PROFILES.iter().find(|profile| profile.key == key)
}

/// Per-application operator credentials from `auth.<profileKey>`.
///
/// Configuration fields are `clientId`, `fixedRedirect`, and `asRedirect` in
/// `keld.config.ts` and `keld.boot.json`. They are distinct from the
/// `snake_case` auth CALL/REPLY payload fields.
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthProfileConfiguration {
    /// Public OAuth client id; absence is checked only after guard Allow.
    #[serde(rename = "clientId", default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Operator-registered exact redirect; fixed-port validation is post-Allow.
    #[serde(
        rename = "fixedRedirect",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fixed_redirect: Option<String>,
    /// Operator-registered macOS AS return binding; validation is post-Allow.
    /// Omission or boot JSON null retains the existing loopback configuration.
    #[serde(
        rename = "asRedirect",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub as_redirect: Option<String>,
}

impl std::fmt::Debug for AuthProfileConfiguration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthProfileConfiguration")
            .finish_non_exhaustive()
    }
}

/// Operator-authored credentials keyed by the closed profile keys.
///
/// An absent boot auth field uses the default empty map. The boot loader
/// invokes `retain_known_profiles` once before exposing the configuration.
pub type AuthConfiguration = BTreeMap<String, AuthProfileConfiguration>;

/// Deserializes operator configuration without accepting repeated profile keys.
///
/// This is shape validation only: unknown profile keys and absent client ids
/// remain data. The Native load owner filters/warns once before request-time
/// content checks, which still follow guard Allow. The carrier type is unchanged.
/// Use with `serde(default, deserialize_with = "...deserialize_auth_configuration")`
/// on the optional boot auth field; an absent field uses the empty-map default.
///
/// # Errors
/// Returns the deserializer's error for a non-object, duplicate profile key, or
/// invalid profile-field shape. The duplicate-key error includes no credentials
/// or key contents; other shape diagnostics belong to the deserializer.
pub fn deserialize_auth_configuration<'de, D>(
    deserializer: D,
) -> Result<AuthConfiguration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct UniqueConfiguration;

    impl<'de> serde::de::Visitor<'de> for UniqueConfiguration {
        type Value = AuthConfiguration;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an auth configuration object with unique profile keys")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let mut configuration = AuthConfiguration::new();
            while let Some(key) = access.next_key::<String>()? {
                match configuration.entry(key) {
                    std::collections::btree_map::Entry::Occupied(_) => {
                        return Err(serde::de::Error::custom("duplicate auth profile key"));
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        let profile = access.next_value::<AuthProfileConfiguration>()?;
                        entry.insert(profile);
                    }
                }
            }
            Ok(configuration)
        }
    }

    deserializer.deserialize_map(UniqueConfiguration)
}

/// Ignores unknown profile keys and reports each to the load-time warning sink.
///
/// The sink receives the original untrusted key and must quote/escape it if
/// displayed. Known profiles, including ones with no client id, remain intact:
/// this is load-time profile filtering, not request admission or validation.
pub fn retain_known_profiles(
    configuration: &mut AuthConfiguration,
    mut warn_unknown_profile: impl FnMut(&str),
) {
    configuration.retain(|key, _| {
        if provider_profile(key).is_some() {
            true
        } else {
            warn_unknown_profile(key);
            false
        }
    });
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenDenial {
    NotGranted,
    OutOfScope,
}

#[cfg(target_os = "macos")]
impl TokenDenial {
    // Classify only an actual dispatch_privileged Err. No manifest inspection,
    // scope matching, evaluate call, retained requested/scope/account fields.
    fn from_guard(reason: &keld_guard::DenyReason) -> Option<Self> {
        // This checks the returned error's renderer applicability, not a
        // manifest/grant/scope rule or a second authorization decision.
        let (keld_guard::DenyReason::NotGranted { capability, .. }
        | keld_guard::DenyReason::OutOfScope { capability, .. }) = reason
        else {
            return None;
        };
        if capability != keld_guard::capability::AUTH_TOKEN {
            return None;
        }
        match reason.code() {
            "KELD-GUARD001" => Some(Self::NotGranted),
            "KELD-GUARD002" => Some(Self::OutOfScope),
            _ => None,
        }
    }

    // Sole code-to-constant renderer for approved AC13(c,d).
    // OutOfScope also represents AC13(d)'s synthetic wire category after
    // actual Allow with no session. That path has no DenyReason: its emitter
    // is keld-native while GUARD002 stays registered under keld-guard.
    // Root's dispatcher must separately surface the Native host-only fault.
    fn parts(self) -> (&'static str, &'static str) {
        match self {
            Self::NotGranted => (
                "KELD-GUARD001",
                "KELD-GUARD001: auth.token is not granted. Add the intended declared issuer to /app/auth/token in keld.permissions.jsonc.",
            ),
            Self::OutOfScope => (
                "KELD-GUARD002",
                "KELD-GUARD002: auth.token is outside its grant. Add the intended declared issuer to /app/auth/token in keld.permissions.jsonc.",
            ),
        }
    }
}

#[cfg(target_os = "macos")]
impl std::fmt::Display for TokenDenial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.parts().1)
    }
}

#[cfg(target_os = "macos")]
impl std::error::Error for TokenDenial {}

#[cfg(target_os = "macos")]
impl From<TokenDenial> for keld_ipc::CallError {
    fn from(denial: TokenDenial) -> Self {
        Self {
            code: denial.parts().0.to_owned(),
            message: denial.to_string(),
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "privacy_tests.rs"]
mod privacy_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use keld_ipc::codec::{decode, encode};

    #[test]
    fn debug_does_not_emit_authentication_payloads() {
        let request = AuthRequest::Begin {
            provider: "private-provider-marker".to_owned(),
            scopes: Some(vec!["private-scope-marker".to_owned()]),
            login_hint: Some("private-login-hint-marker".to_owned()),
        };
        let token_request = AuthRequest::Token {
            account_id: "private-account-marker".to_owned(),
            scopes: None,
        };
        let response = AuthResponse::Token {
            access_token: "private-access-token-marker".to_owned(),
            expires_at: 300,
            token_type: "Bearer".to_owned(),
        };
        let begin_response = AuthResponse::Begin {
            account_id: "private-account-marker".to_owned(),
            granted_scopes: vec!["private-scope-marker".to_owned()],
        };
        let config = AuthProfileConfiguration {
            client_id: Some("private-configuration-marker".to_owned()),
            fixed_redirect: None,
            as_redirect: None,
        };
        assert_eq!(format!("{request:?}"), "AuthRequest::Begin { .. }");
        assert_eq!(format!("{token_request:?}"), "AuthRequest::Token { .. }");
        assert_eq!(format!("{response:?}"), "AuthResponse::Token { .. }");
        assert_eq!(format!("{begin_response:?}"), "AuthResponse::Begin { .. }");
        assert_eq!(format!("{config:?}"), "AuthProfileConfiguration { .. }");
    }

    #[test]
    fn request_payloads_are_literal_postcard_bytes() {
        let begin = AuthRequest::Begin {
            provider: "entra-common".to_owned(),
            scopes: None,
            login_hint: None,
        };
        let begin_bytes = [
            0, 12, b'e', b'n', b't', b'r', b'a', b'-', b'c', b'o', b'm', b'm', b'o', b'n', 0, 0,
        ];
        assert_eq!(encode(&begin).expect("encode Begin"), begin_bytes);
        match decode::<AuthRequest>(&begin_bytes).expect("decode Begin") {
            AuthRequest::Begin {
                provider,
                scopes,
                login_hint,
            } => {
                assert_eq!(provider, "entra-common");
                assert!(scopes.is_none());
                assert!(login_hint.is_none());
            }
            AuthRequest::Token { .. } => panic!("Begin bytes decoded as Token"),
        }

        let token = AuthRequest::Token {
            account_id: "a".to_owned(),
            scopes: None,
        };
        let token_bytes = [1, 1, b'a', 0];
        assert_eq!(encode(&token).expect("encode Token"), token_bytes);
        match decode::<AuthRequest>(&token_bytes).expect("decode Token") {
            AuthRequest::Token { account_id, scopes } => {
                assert_eq!(account_id, "a");
                assert!(scopes.is_none());
            }
            AuthRequest::Begin { .. } => panic!("Token bytes decoded as Begin"),
        }
        assert!(decode::<AuthRequest>(&[1, 1, b'a']).is_err());
        assert!(decode::<AuthRequest>(&[1, 1, b'a', 0, 0]).is_err());
        assert!(decode::<AuthRequest>(&[2]).is_err());
    }

    #[test]
    fn optional_request_fields_keep_declared_order() {
        let begin = AuthRequest::Begin {
            provider: "p".to_owned(),
            scopes: Some(vec!["s".to_owned()]),
            login_hint: Some("h".to_owned()),
        };
        assert_eq!(
            encode(&begin).expect("encode Begin options"),
            [0, 1, b'p', 1, 1, 1, b's', 1, 1, b'h']
        );
        let token = AuthRequest::Token {
            account_id: "a".to_owned(),
            scopes: Some(vec!["s".to_owned()]),
        };
        assert_eq!(
            encode(&token).expect("encode Token options"),
            [1, 1, b'a', 1, 1, 1, b's']
        );
    }

    #[test]
    fn response_payloads_are_literal_postcard_bytes() {
        let begin = AuthResponse::Begin {
            account_id: "a".to_owned(),
            granted_scopes: vec!["s".to_owned()],
        };
        let begin_bytes = [0, 1, b'a', 1, 1, b's'];
        assert_eq!(encode(&begin).expect("encode Begin response"), begin_bytes);
        match decode::<AuthResponse>(&begin_bytes).expect("decode Begin response") {
            AuthResponse::Begin {
                account_id,
                granted_scopes,
            } => {
                assert_eq!(account_id, "a");
                assert_eq!(granted_scopes, ["s"]);
            }
            AuthResponse::Token { .. } => panic!("Begin response bytes decoded as Token"),
        }

        let token = AuthResponse::Token {
            access_token: "t".to_owned(),
            expires_at: 300,
            token_type: "Bearer".to_owned(),
        };
        let token_bytes = [
            1, 1, b't', 0xac, 0x02, 6, b'B', b'e', b'a', b'r', b'e', b'r',
        ];
        assert_eq!(encode(&token).expect("encode Token response"), token_bytes);
        match decode::<AuthResponse>(&token_bytes).expect("decode Token response") {
            AuthResponse::Token {
                access_token,
                expires_at,
                token_type,
            } => {
                assert_eq!(access_token, "t");
                assert_eq!(expires_at, 300);
                assert_eq!(token_type, "Bearer");
            }
            AuthResponse::Begin { .. } => panic!("Token response bytes decoded as Begin"),
        }
        // 2^32 has an independent five-byte unsigned LEB128 encoding.
        // This fixture rejects an accidental u32 expiry contract.
        let wide_expiry_bytes = [
            1, 1, b't', 0x80, 0x80, 0x80, 0x80, 0x10, 6, b'B', b'e', b'a', b'r', b'e', b'r',
        ];
        match decode::<AuthResponse>(&wide_expiry_bytes).expect("decode u64 expiry response") {
            AuthResponse::Token {
                access_token,
                expires_at,
                token_type,
            } => {
                let decoded_expiry: u64 = expires_at;
                assert_eq!(decoded_expiry, 4_294_967_296_u64);
                assert_eq!(access_token, "t");
                assert_eq!(token_type, "Bearer");
            }
            AuthResponse::Begin { .. } => panic!("wide expiry bytes decoded as Begin"),
        }
        // A trailing serialized credential is not part of any v0 reply.
        let mut credential_suffix = token_bytes.to_vec();
        credential_suffix.extend([1, b'r']);
        assert!(decode::<AuthResponse>(&credential_suffix).is_err());
        assert!(decode::<AuthResponse>(&[2]).is_err());
    }

    #[test]
    fn closed_profile_has_exact_approved_literals() {
        assert_eq!(PROVIDER_PROFILES.len(), 1);
        let profile = provider_profile("entra-common").expect("approved profile");
        assert_eq!(profile.key, "entra-common");
        assert_eq!(profile.issuer, "https://login.microsoftonline.com/common");
        assert_eq!(
            profile.authorization_endpoint,
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
        );
        assert_eq!(
            profile.token_endpoint,
            "https://login.microsoftonline.com/common/oauth2/v2.0/token"
        );
        assert_eq!(profile.redirect_form, RedirectForm::Localhost);
        for key in [
            "google",
            "Entra-common",
            "entra-common/",
            "https://login.microsoftonline.com/common",
        ] {
            assert!(provider_profile(key).is_none(), "{key}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn configuration_rejects_unknown_inner_fields_without_validating_values() {
        for value in [
            r#"{"clientID":"operator-client"}"#,
            r#"{"clientId":"operator-client","fixedRedirct":"http://localhost:53161/callback"}"#,
            r#"{"clientId":"operator-client","issuer":"https://unconfigured.invalid"}"#,
        ] {
            let error = serde_json::from_str::<AuthProfileConfiguration>(value)
                .expect_err("unknown profile field must refuse");
            assert!(error.to_string().contains("unknown field"));
        }
        let absent: AuthProfileConfiguration =
            serde_json::from_str("{}").expect("missing client remains post-Allow data");
        assert!(absent.client_id.is_none());
        let unvalidated: AuthProfileConfiguration =
            serde_json::from_str(r#"{"clientId":"","fixedRedirect":"not-a-uri"}"#)
                .expect("value validation remains post-Allow");
        assert_eq!(unvalidated.client_id.as_deref(), Some(""));
        assert_eq!(unvalidated.fixed_redirect.as_deref(), Some("not-a-uri"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn decoded_unknown_outer_profile_is_filtered_once_after_shape_parse() {
        let mut configuration: AuthConfiguration =
            serde_json::from_str(r#"{"entra-common":{},"future-provider":{"clientId":"unused"}}"#)
                .expect("outer profile names are not a schema rejection");
        assert_eq!(configuration.len(), 2);
        let mut warnings = Vec::new();
        retain_known_profiles(&mut configuration, |key| warnings.push(key.to_owned()));
        assert_eq!(warnings, ["future-provider"]);
        assert_eq!(configuration.len(), 1);
        assert!(
            configuration
                .get("entra-common")
                .expect("known profile")
                .client_id
                .is_none()
        );
    }

    #[test]
    fn configuration_ignores_unknown_keys_with_one_warning_each() {
        let mut configuration = AuthConfiguration::from([
            (
                "entra-common".to_owned(),
                AuthProfileConfiguration {
                    client_id: Some("operator-client".to_owned()),
                    fixed_redirect: Some("http://localhost:4321/callback".to_owned()),
                    as_redirect: None,
                },
            ),
            ("google".to_owned(), AuthProfileConfiguration::default()),
            ("unknown".to_owned(), AuthProfileConfiguration::default()),
        ]);
        let mut warnings = Vec::new();
        retain_known_profiles(&mut configuration, |key| warnings.push(key.to_owned()));
        assert_eq!(warnings, ["google", "unknown"]);
        assert_eq!(configuration.len(), 1);
        let known = configuration
            .get("entra-common")
            .expect("known config retained");
        assert_eq!(known.client_id.as_deref(), Some("operator-client"));
        assert_eq!(
            known.fixed_redirect.as_deref(),
            Some("http://localhost:4321/callback")
        );

        let mut absent = AuthConfiguration::default();
        retain_known_profiles(&mut absent, |_| panic!("empty config warns"));
        assert!(absent.is_empty());
        let mut missing_client = AuthConfiguration::from([(
            "entra-common".to_owned(),
            AuthProfileConfiguration::default(),
        )]);
        retain_known_profiles(&mut missing_client, |_| panic!("known profile warns"));
        assert!(
            missing_client
                .get("entra-common")
                .expect("known config retained")
                .client_id
                .is_none()
        );
    }

    #[test]
    fn as_redirect_shape_accepts_literal_operator_binding() {
        let value = serde_json::json!({
            "clientId": "operator-client",
            "fixedRedirect": "http://localhost:4321/callback",
            "asRedirect": "msaloperator-client://auth"
        });
        let config: AuthProfileConfiguration = serde_json::from_value(value.clone())
            .expect("registered AS binding is configuration data");
        assert_eq!(
            serde_json::to_value(config).expect("serialize config"),
            value
        );
    }

    #[test]
    fn as_redirect_shape_preserves_null_absence_and_closed_fields() {
        for value in [
            serde_json::json!({"clientId": "operator-client"}),
            serde_json::json!({"clientId": "operator-client", "asRedirect": null}),
        ] {
            let config: AuthProfileConfiguration = serde_json::from_value(value)
                .expect("absent AS binding preserves existing configuration");
            assert_eq!(
                serde_json::to_value(config).expect("serialize config"),
                serde_json::json!({"clientId": "operator-client"})
            );
        }
        for value in [
            serde_json::json!({"asRedirect": true}),
            serde_json::json!({"asRedirect": []}),
            serde_json::json!({"asRedirect": 42}),
            serde_json::json!({"asRedirect": "value", "extra": "value"}),
        ] {
            assert!(serde_json::from_value::<AuthProfileConfiguration>(value).is_err());
        }
    }

    #[test]
    fn as_redirect_shape_defers_registration_semantics() {
        let value = serde_json::json!({"asRedirect": "not-a-valid-registration"});
        let config: AuthProfileConfiguration = serde_json::from_value(value.clone())
            .expect("registration semantics follow guard Allow, not shape loading");
        assert_eq!(
            serde_json::to_value(config).expect("serialize config"),
            value
        );
    }
}
