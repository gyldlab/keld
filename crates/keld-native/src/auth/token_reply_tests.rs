//! Literal JSON/status/clock contracts. No actual provider/HTTP/OS claims.
use super::*;
use std::time::{Duration, UNIX_EPOCH};
fn decode(status: u16, json: &[u8], mode: ExchangeMode) -> Result<ParsedToken, TokenReplyError> {
    decode_token_reply(
        status,
        json,
        UNIX_EPOCH + Duration::from_secs(4_294_967_296),
        Instant::now(),
        Some(&["a".to_owned(), "b".to_owned()]),
        mode,
    )
}
#[test]
fn real_json_shape_valid_success_has_literal_expiry_and_host_only_refresh() {
    let value = decode(200, br#"{"access_token":"access-private","token_type":"Bearer","expires_in":300,"refresh_token":"refresh-private","scope":"a b","iss":"https://untrusted-claim.test"}"#, ExchangeMode::AuthorizationCode).expect("literal parser result, not provider proof");
    assert_eq!(value.refresh.as_deref(), Some("refresh-private"));
    assert_eq!(value.granted_scopes, ["a", "b"]);
    match value.access.response().expect("fresh final conversion") {
        super::super::AuthResponse::Token {
            access_token,
            token_type,
            expires_at,
        } => {
            assert_eq!(access_token, "access-private");
            assert_eq!(token_type, "Bearer");
            assert_eq!(expires_at, 4_294_967_596);
        }
        super::super::AuthResponse::Begin { .. } => panic!("wrong DTO"),
    }
    assert!(!format!("{value:?}").contains("private"));
}
#[test]
fn omitted_optional_fields_remain_valid_and_scope_fallback_is_requested_set() {
    let value = decode(
        200,
        br#"{"access_token":"a","token_type":"Bearer","expires_in":300}"#,
        ExchangeMode::AuthorizationCode,
    )
    .expect("valid omissions");
    assert!(value.refresh.is_none());
    assert_eq!(value.granted_scopes, ["a", "b"]);
}
#[test]
fn actual_error_field_classifies_raw_exact_refresh_only_and_sanitizes_afterward() {
    let error = decode(
        400,
        br#"{"error":"invalid_grant","error_description":"bounded\nprovider refusal"}"#,
        ExchangeMode::Refresh,
    )
    .expect_err("actual structured refusal fixture");
    assert!(error.terminal_invalid_grant());
    assert_eq!(error.code(), "KELD-AUTH-009");
    let not_refresh = decode(
        400,
        br#"{"error":"invalid_grant"}"#,
        ExchangeMode::AuthorizationCode,
    )
    .expect_err("Begin exchange refusal");
    assert!(!not_refresh.terminal_invalid_grant());
    assert_eq!(not_refresh.code(), "KELD-AUTH-013");
    let not_exact = decode(
        400,
        br#"{"error":"invalid_grant\n"}"#,
        ExchangeMode::Refresh,
    )
    .expect_err("raw field is not exact");
    assert!(!not_exact.terminal_invalid_grant());
    assert_eq!(not_exact.code(), "KELD-AUTH-013");
    let interaction = decode(
        400,
        br#"{"error":"interaction_required"}"#,
        ExchangeMode::Refresh,
    )
    .expect_err("HTTP needs interaction");
    assert_eq!(interaction.code(), "KELD-AUTH-013");
    assert!(!interaction.terminal_invalid_grant());
    assert!(!error.to_string().contains('\n'));
    assert!(error.to_string().starts_with("KELD-AUTH-009:"));
}
#[test]
fn known_duplicate_wrong_type_and_mixed_success_error_cannot_complete() {
    for (status, body) in [
        (200, br#"{"access_token":"a","access_token":"b","token_type":"Bearer","expires_in":300}"#.as_slice()),
        (200, br#"{"access_token":"a","\u0061ccess_token":"b","token_type":"Bearer","expires_in":300}"#.as_slice()),
        (200, br#"{"access_token":"a","token_type":"Bearer","expires_in":"300"}"#.as_slice()),
        (400, br#"{"access_token":"a","token_type":"Bearer","expires_in":300,"error":"invalid_grant"}"#.as_slice()),
        (400, br#"{"error":"invalid_scope","error":"invalid_grant"}"#.as_slice()),
        (400, br#"{"error":"invalid_scope","\u0065rror":"invalid_grant"}"#.as_slice()),
        (200, br#"{"access_token":"a","token_type":"Bearer","expires_in":300,"scope":"a  b"}"#.as_slice()),
    ] {
        let error = decode(status, body, ExchangeMode::Refresh).expect_err("malformed token response");
        assert!(matches!(error, TokenReplyError::Malformed), "a different failure cannot prove duplicate/type/grammar rejection: {error:?}");
        assert_eq!(error.code(), "KELD-AUTH-013"); assert!(!error.terminal_invalid_grant());
    }
}
// Precise failure-first candidates: preserve original Option<T>/status parser
// until Root observes the red cases. These are not run by this writer.
#[test]
fn present_null_known_fields_are_malformed_rather_than_absent() {
    for (status, body) in [
        (200, br#"{"access_token":"a","token_type":"Bearer","expires_in":300,"scope":null}"#.as_slice()),
        (200, br#"{"access_token":"a","token_type":"Bearer","expires_in":300,"refresh_token":null}"#.as_slice()),
        (200, br#"{"access_token":"a","token_type":"Bearer","expires_in":300,"error":null}"#.as_slice()),
        (200, br#"{"access_token":"a","token_type":"Bearer","expires_in":300,"error_description":null}"#.as_slice()),
        (400, br#"{"error":"invalid_grant","access_token":null}"#.as_slice()),
        (400, br#"{"error":"invalid_grant","token_type":null}"#.as_slice()),
        (400, br#"{"error":"invalid_grant","expires_in":null}"#.as_slice()),
        (400, br#"{"error":"invalid_grant","refresh_token":null}"#.as_slice()),
    ] {
        let error = decode(status, body, ExchangeMode::Refresh).expect_err("present-null known field is malformed");
        assert_eq!(error.code(), "KELD-AUTH-013"); assert!(!error.terminal_invalid_grant());
    }
}
#[test]
fn success_or_server_fault_status_invalid_grant_cannot_terminally_evict() {
    for status in [200, 500] {
        let error = decode(
            status,
            br#"{"error":"invalid_grant"}"#,
            ExchangeMode::Refresh,
        )
        .expect_err("status disagrees with OAuth refusal");
        assert_eq!(error.code(), "KELD-AUTH-013");
        assert!(!error.terminal_invalid_grant());
    }
    let actual_refusal = decode(400, br#"{"error":"invalid_grant"}"#, ExchangeMode::Refresh)
        .expect_err("valid OAuth refusal fixture");
    assert_eq!(actual_refusal.code(), "KELD-AUTH-009");
    assert!(actual_refusal.terminal_invalid_grant());
}

#[test]
fn token_endpoint_reply_requires_root_object_before_struct_fields() {
    let positive = decode(
        200,
        br#"{"access_token":"a","token_type":"Bearer","expires_in":300}"#,
        ExchangeMode::AuthorizationCode,
    );
    assert!(
        positive.is_ok(),
        "actual object positive must precede negative shape oracle"
    );
    for body in [
        br#"["a","Bearer",300]"#.as_slice(),
        b"null".as_slice(),
        b"true".as_slice(),
    ] {
        let error = decode(200, body, ExchangeMode::AuthorizationCode)
            .expect_err("token endpoint reply root must be object");
        assert!(matches!(error, TokenReplyError::Malformed));
        assert_eq!(error.code(), "KELD-AUTH-013");
    }
}
#[test]
fn unauthorized_status_exception_is_only_structured_invalid_client() {
    let refused = decode(401, br#"{"error":"invalid_client"}"#, ExchangeMode::Refresh)
        .expect_err("standard client-authentication exception");
    assert_eq!(refused.code(), "KELD-AUTH-013");
    assert!(!refused.terminal_invalid_grant());
    let wrong = decode(401, br#"{"error":"invalid_grant"}"#, ExchangeMode::Refresh)
        .expect_err("invalid grant has no401 exception");
    assert!(matches!(wrong, TokenReplyError::Malformed));
    assert!(!wrong.terminal_invalid_grant());
}
