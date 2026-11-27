use serde_json::{Value, json};

#[derive(Debug)]
pub(crate) struct ProbeError {
    pub(crate) kind: &'static str,
    pub(crate) source: &'static str,
    pub(crate) status: Option<i64>,
    pub(crate) detail: String,
}

impl ProbeError {
    pub(crate) fn native(source: &'static str, status: i64) -> Self {
        Self {
            kind: "native-failure",
            source,
            status: Some(status),
            detail: "Native API reported failure; no fact was substituted.".into(),
        }
    }

    pub(crate) fn invalid(source: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind: "invalid-native-result",
            source,
            status: None,
            detail: detail.into(),
        }
    }

    pub(crate) fn unexpected(source: &'static str) -> Self {
        Self { kind: "unexpected-control-success", source, status: None, detail: "The deliberately invalid native request succeeded; this is not evidence of refusal.".into() }
    }

    pub(crate) fn json(&self) -> Value {
        json!({"kind": self.kind, "source": self.source, "status": self.status, "detail": self.detail})
    }
}

pub(crate) type ProbeResult<T> = Result<T, ProbeError>;

pub(crate) fn utf16(value: &[u16], source: &'static str) -> ProbeResult<String> {
    let end = value
        .iter()
        .position(|v| *v == 0)
        .ok_or_else(|| ProbeError::invalid(source, "Missing UTF-16 terminator"))?;
    String::from_utf16(&value[..end]).map_err(|_| ProbeError::invalid(source, "Invalid UTF-16"))
}

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
