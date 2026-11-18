//! The single explicit security-profile declaration and its authoring boundary.

use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Value;

use crate::ProfileState;

/// Required disclosure for an explicit Legacy request; this is not a receipt.
pub const LEGACY_PROFILE_FORFEIT: &str = "legacy — zero-authority claim forfeited";

/// A requested security profile from the root permissions-manifest property.
///
/// This is policy data, not evidence that the requested runtime was admitted.
/// Omission is represented separately by `None`; `unverified` is not configurable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaredProfile {
    /// Require the existing fail-closed OS-containment admission checks.
    Strict,
    /// Explicitly forfeit Keld's zero-ambient-authority containment claim.
    Legacy,
}

impl From<DeclaredProfile> for ProfileState {
    fn from(declared: DeclaredProfile) -> Self {
        match declared {
            DeclaredProfile::Strict => Self::Strict,
            DeclaredProfile::Legacy => Self::Legacy,
        }
    }
}

pub(super) fn deserialize_present<'de, D>(
    deserializer: D,
) -> Result<Option<DeclaredProfile>, D::Error>
where
    D: Deserializer<'de>,
{
    // Serde's field default handles omission. Do not deserialize Option here:
    // doing so would silently treat an explicit null as missing policy.
    // String deserialization also rejects an externally tagged enum object
    // such as {"legacy":null}, which is not this authoring grammar.
    let value = String::deserialize(deserializer)?;
    match value.as_str() {
        "strict" => Ok(Some(DeclaredProfile::Strict)),
        "legacy" => Ok(Some(DeclaredProfile::Legacy)),
        _ => Err(D::Error::custom(
            "root /profile must be exactly \"strict\" or \"legacy\"",
        )),
    }
}

pub(super) fn validate_root(value: &Value) -> Result<(), &'static str> {
    let Some(root) = value.as_object() else {
        return Err(
            "the permissions manifest must be a JSON object, not a positional array or scalar",
        );
    };
    if root
        .get("app")
        .and_then(Value::as_object)
        .is_some_and(|app| app.contains_key("profile"))
    {
        return Err("/app/profile is not a security-profile location; use only root /profile");
    }
    for (collection, detail) in [
        (
            "windows",
            "/windows/<window>/profile is not a security-profile location; use only root /profile",
        ),
        (
            "roles",
            "/roles/<role>/profile is not a security-profile location; use only root /profile",
        ),
    ] {
        if root
            .get(collection)
            .and_then(Value::as_object)
            .is_some_and(|members| {
                members.values().any(|member| {
                    member
                        .as_object()
                        .is_some_and(|object| object.contains_key("profile"))
                })
            })
        {
            return Err(detail);
        }
    }
    Ok(())
}
