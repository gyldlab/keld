//! D-48 native object and destination bindings as validated read-only data.
//! File identifiers and string encodings never prove retained OS custody.

use super::{UpdateError, encode_bounded, invalid};
use serde::{Deserialize, Serialize};

const OBJECT_SCHEMA: &str = "keld.windows-object-observation/v1";
const ROOT_SCHEMA: &str = "keld.windows-destination-binding/v1";
const MAX_OBJECT_BYTES: usize = 512;
const MAX_ROOT_BYTES: usize = 4_096;

/// D-48's observed kind; actual retained handles must be checked separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ObjectKind {
    Directory,
    RegularFile,
}

/// D-48's native volume/file observation, without rights or identity authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeObjectObservation {
    pub(crate) volume_guid: String,
    pub(crate) device: u64,
    pub(crate) file: u64,
    pub(crate) kind: ObjectKind,
}

/// D-48's checked type of destination-root binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RootBindingKind {
    Existing,
    Absent,
}

/// Native-derived destination root and remaining child names, without OS grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DestinationRootBinding {
    pub(crate) kind: RootBindingKind,
    pub(crate) anchor: NativeObjectObservation,
    pub(crate) remaining: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct WireObject {
    schema: String,
    volume_guid: String,
    device: String,
    file: String,
    kind: ObjectKind,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireRoot {
    schema: String,
    kind: RootBindingKind,
    anchor: WireObject,
    remaining: Vec<String>,
}

pub(super) fn validate_object(observed: &NativeObjectObservation) -> Result<(), UpdateError> {
    let Some(guid) = observed
        .volume_guid
        .strip_prefix(r"\\?\Volume{")
        .and_then(|rest| rest.strip_suffix(r"}\"))
    else {
        return Err(invalid(
            "native observation has no canonical volume-GUID root",
        ));
    };
    if guid.len() != 36
        || guid.bytes().enumerate().any(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte != b'-'
            } else {
                !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase()
            }
        })
    {
        return Err(invalid(
            "native observation has malformed lowercase volume GUID",
        ));
    }
    Ok(())
}

pub(super) fn wire_object(observed: &NativeObjectObservation) -> Result<WireObject, UpdateError> {
    validate_object(observed)?;
    let wire = WireObject {
        schema: OBJECT_SCHEMA.to_owned(),
        volume_guid: observed.volume_guid.clone(),
        device: super::super::wire_h16(observed.device),
        file: super::super::wire_h16(observed.file),
        kind: observed.kind,
    };
    let _bytes = encode_bounded(&wire, MAX_OBJECT_BYTES)?;
    Ok(wire)
}

pub(super) fn object_from_wire(wire: WireObject) -> Result<NativeObjectObservation, UpdateError> {
    super::super::schema(&wire.schema, OBJECT_SCHEMA)?;
    let observed = NativeObjectObservation {
        volume_guid: wire.volume_guid,
        device: super::super::h16(&wire.device)?,
        file: super::super::h16(&wire.file)?,
        kind: wire.kind,
    };
    validate_object(&observed)?;
    Ok(observed)
}

pub(super) fn validate_root(root: &DestinationRootBinding) -> Result<(), UpdateError> {
    if root.anchor.kind != ObjectKind::Directory {
        return Err(invalid("destination anchor must be a directory"));
    }
    validate_object(&root.anchor)?;
    match root.kind {
        RootBindingKind::Existing if !root.remaining.is_empty() => {
            return Err(invalid(
                "existing destination cannot have a remaining suffix",
            ));
        }
        RootBindingKind::Absent if !(1..=6).contains(&root.remaining.len()) => {
            return Err(invalid("absent destination needs 1..=6 derived components"));
        }
        _ => {}
    }
    for part in &root.remaining {
        if part.len() > 255 {
            return Err(invalid("derived root component exceeds 255 UTF-8 bytes"));
        }
        keld_guard::validate_fs_component(part).map_err(invalid)?;
    }
    Ok(())
}

pub(super) fn wire_root(root: &DestinationRootBinding) -> Result<WireRoot, UpdateError> {
    validate_root(root)?;
    let wire = WireRoot {
        schema: ROOT_SCHEMA.to_owned(),
        kind: root.kind,
        anchor: wire_object(&root.anchor)?,
        remaining: root.remaining.clone(),
    };
    let _bytes = encode_bounded(&wire, MAX_ROOT_BYTES)?;
    Ok(wire)
}

pub(super) fn root_from_wire(wire: WireRoot) -> Result<DestinationRootBinding, UpdateError> {
    super::super::schema(&wire.schema, ROOT_SCHEMA)?;
    let root = DestinationRootBinding {
        kind: wire.kind,
        anchor: object_from_wire(wire.anchor)?,
        remaining: wire.remaining,
    };
    validate_root(&root)?;
    Ok(root)
}
