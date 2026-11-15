//! Existing profile-state HTTP responses, request parser and atom validation.

use super::profile_observation::{ProfileStateObservation, ProfileStorageState};
use crate::DARK_BG;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};

pub(crate) fn state_redirect_html(
    address: SocketAddr,
    case_name: &str,
    nonce: &str,
    value: &str,
) -> String {
    format!(
        "<!doctype html>{DARK_BG}<script>location.replace('http://127.0.0.1:{}/app?case={}&nonce={}&value={}')</script>\n",
        address.port(),
        case_name,
        nonce,
        value
    )
}

pub(crate) struct ProfileStateResponse {
    status: &'static str,
    content_type: &'static str,
    body: String,
    pub(crate) observation: Option<ProfileStateObservation>,
}

pub(crate) fn renderer_request_path(request: &[u8]) -> Result<&str, String> {
    let request = std::str::from_utf8(request)
        .map_err(|error| format!("profile state request line is not UTF-8: {error}"))?;
    let mut fields = request.split_whitespace();
    if fields.next() != Some("GET") {
        return Err(format!("profile state request is not GET: {request}"));
    }
    let path = fields
        .next()
        .ok_or_else(|| format!("profile state request has no path: {request}"))?;
    if fields.next() != Some("HTTP/1.1") || fields.next().is_some() {
        return Err(format!("profile state request shape is invalid: {request}"));
    }
    Ok(path)
}

pub(crate) fn write_profile_state_response(
    stream: &mut TcpStream,
    response: &ProfileStateResponse,
) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|()| stream.write_all(response.body.as_bytes()))
        .map_err(|error| format!("write profile state response: {error}"))
}

pub(crate) fn profile_state_response(
    path: &str,
    expected_case: &str,
    last_observation: Option<&ProfileStateObservation>,
) -> Result<ProfileStateResponse, String> {
    if path == "/favicon.ico" {
        return Ok(ProfileStateResponse {
            status: "204 No Content",
            content_type: "text/plain",
            body: String::new(),
            observation: None,
        });
    }
    if let Some(query) = path.strip_prefix("/state?") {
        let fields = profile_state_query(query)?;
        let case_name = required_profile_state_field(&fields, "case")?;
        if let Some((_, error)) = fields.iter().find(|(key, _)| *key == "error") {
            return Err(format!(
                "browser storage case `{case_name}` failed: {error}"
            ));
        }
        if let Some((_, progress)) = fields.iter().find(|(key, _)| *key == "progress") {
            if case_name != expected_case {
                return Err("browser storage progress belongs to the wrong case".to_owned());
            }
            eprintln!("KELD_KEL135_STORAGE_PROGRESS case={case_name} phase={progress}");
            return Ok(empty_profile_state_response());
        }
        let observation = ProfileStateObservation {
            case_name: case_name.to_owned(),
            nonce: required_profile_state_field(&fields, "nonce")?.to_owned(),
            before: ProfileStorageState::from_fields(&fields, "before")?,
            after: ProfileStorageState::from_fields(&fields, "after")?,
        };
        if case_name != expected_case {
            if last_observation == Some(&observation) {
                return Ok(empty_profile_state_response());
            }
            return Err(format!(
                "profile state report case `{case_name}` did not match `{expected_case}`"
            ));
        }
        return Ok(ProfileStateResponse {
            status: "204 No Content",
            content_type: "text/plain",
            body: String::new(),
            observation: Some(observation),
        });
    }
    if let Some(query) = path.strip_prefix("/profile-worker.js?") {
        let fields = profile_state_query(query)?;
        let nonce = required_profile_state_field(&fields, "nonce")?;
        let value = required_profile_state_field(&fields, "value")?;
        validate_profile_state_atom(nonce, false)?;
        validate_profile_state_atom(value, false)?;
        let value = serde_json::to_string(value).expect("serialize fixture worker nonce");
        return Ok(ProfileStateResponse {
            status: "200 OK",
            content_type: "application/javascript",
            body: format!(
                "const nonce={value};self.addEventListener('install',e=>e.waitUntil(self.skipWaiting()));self.addEventListener('message',e=>{{if(e.data==='read-nonce'&&e.ports[0]){{e.ports[0].postMessage(nonce);e.ports[0].close();}}}});"
            ),
            observation: None,
        });
    }
    let Some(query) = path.strip_prefix("/app?") else {
        return Ok(ProfileStateResponse {
            status: "404 Not Found",
            content_type: "text/plain",
            body: String::new(),
            observation: None,
        });
    };
    let fields = profile_state_query(query)?;
    let case_name = required_profile_state_field(&fields, "case")?;
    let nonce = required_profile_state_field(&fields, "nonce")?;
    let value = required_profile_state_field(&fields, "value")?;
    validate_profile_state_atom(case_name, false)?;
    validate_profile_state_atom(nonce, false)?;
    validate_profile_state_atom(value, true)?;
    if case_name != expected_case {
        if last_observation.is_some_and(|prior| {
            prior.case_name == case_name && prior.nonce == nonce && prior.after.is_uniform(value)
        }) {
            return Ok(empty_profile_state_response());
        }
        return Err(format!(
            "profile state app case `{case_name}` did not match `{expected_case}`"
        ));
    }
    let config = serde_json::json!({ "caseName": case_name, "nonce": nonce, "value": value });
    let browser = include_str!("../../../fixtures/profile_state.js");
    let body = format!(
        "<!doctype html>{DARK_BG}<script>globalThis.keldProfileState={config};\n{browser}</script>"
    );
    Ok(ProfileStateResponse {
        status: "200 OK",
        content_type: "text/html; charset=utf-8",
        body,
        observation: None,
    })
}

pub(crate) fn validate_profile_state_atom(value: &str, empty_allowed: bool) -> Result<(), String> {
    if (value.is_empty() && !empty_allowed)
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("profile state fixture identifier is invalid".to_owned());
    }
    Ok(())
}

fn empty_profile_state_response() -> ProfileStateResponse {
    ProfileStateResponse {
        status: "204 No Content",
        content_type: "text/plain",
        body: String::new(),
        observation: None,
    }
}

fn profile_state_query(query: &str) -> Result<Vec<(&str, &str)>, String> {
    let mut fields = Vec::new();
    for pair in query.split('&') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("profile state query pair is malformed: {pair}"))?;
        if fields.iter().any(|(found, _)| *found == key) {
            return Err(format!("profile state query duplicates `{key}`"));
        }
        fields.push((key, value));
    }
    Ok(fields)
}

pub(crate) fn required_profile_state_field<'a>(
    fields: &'a [(&str, &str)],
    expected: &str,
) -> Result<&'a str, String> {
    fields
        .iter()
        .find_map(|(key, value)| (*key == expected).then_some(*value))
        .ok_or_else(|| format!("profile state query omits `{expected}`"))
}
