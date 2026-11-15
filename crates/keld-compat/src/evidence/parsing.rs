//! Closed-vocabulary v1 compatibility evidence and denominator parsing.
//! Data types and public interfaces remain owned by the parent evidence module.

use std::collections::BTreeSet;

use serde::Deserialize;

use super::{
    Arch, ArtifactIdentity, AuthorityProfile, CellKey, CivilDate, DENOMINATOR_SCHEMA, Denominator,
    EVIDENCE_SCHEMA, EvidenceError, EvidenceRecord, MAX_DENOMINATOR_BYTES, MAX_EVIDENCE_BYTES,
    Operation, OperationKind, Panel, Platform, Revisions, SemanticOracle, Verdict, Waiver,
    parse_evidence_uri,
};

/// Parse one `keld.compat.evidence/v1` object.
///
/// # Errors
///
/// Returns [`EvidenceError`] for oversize, encoding, JSON, schema, or field
/// violations, including non-normative evidence URIs.
pub fn parse_evidence(bytes: &[u8]) -> Result<EvidenceRecord, EvidenceError> {
    let raw: RawEvidence = parse_object(bytes, MAX_EVIDENCE_BYTES)?;
    if raw.schema != EVIDENCE_SCHEMA {
        return Err(EvidenceError::UnsupportedSchema { found: raw.schema });
    }
    let artifact = ArtifactIdentity {
        sha256: parse_digest(&raw.artifact.sha256)?,
        platform: parse_platform(&raw.artifact.platform)?,
        arch: parse_arch(&raw.artifact.arch)?,
    };
    let revisions = Revisions {
        keld: pinned_revision("revisions.keld", &raw.revisions.keld)?,
        bun: pinned_revision("revisions.bun", &raw.revisions.bun)?,
        engine: pinned_revision("revisions.engine", &raw.revisions.engine)?,
    };
    let operation = Operation {
        id: closed_token("operation.id", &raw.operation.id)?,
        kind: parse_kind(&raw.operation.kind)?,
        oracle: SemanticOracle {
            id: required_token("operation.oracle.id", &raw.operation.oracle.id)?,
            revision: pinned_revision("operation.oracle.revision", &raw.operation.oracle.revision)?,
        },
    };
    let result = parse_verdict(&raw.result)?;
    let waiver = match (&result, raw.waiver) {
        (Verdict::Waived, Some(w)) => Some(parse_waiver(&w)?),
        (Verdict::Waived, None) => {
            return Err(EvidenceError::InvalidWaiver {
                detail: "result is waived but waiver is missing".to_owned(),
            });
        }
        (_, Some(_)) => {
            return Err(EvidenceError::InvalidWaiver {
                detail: "waiver is only allowed when result is waived".to_owned(),
            });
        }
        (_, None) => None,
    };
    Ok(EvidenceRecord {
        artifact,
        revisions,
        authority_profile: parse_authority(&raw.authority_profile)?,
        operation,
        result,
        waiver,
        evidence_uri: parse_evidence_uri(&raw.evidence_uri)?,
    })
}

/// Parse one `keld.compat.denominator/v1` object.
///
/// # Errors
///
/// Returns [`EvidenceError`] for oversize, encoding, JSON, schema, empty
/// cell lists, or duplicate cells.
pub fn parse_denominator(bytes: &[u8]) -> Result<Denominator, EvidenceError> {
    let raw: RawDenominator = parse_object(bytes, MAX_DENOMINATOR_BYTES)?;
    if raw.schema != DENOMINATOR_SCHEMA {
        return Err(EvidenceError::UnsupportedSchema { found: raw.schema });
    }
    let mut cells = Vec::with_capacity(raw.cells.len());
    for cell in raw.cells {
        cells.push(CellKey {
            operation_id: closed_token("cells.operation_id", &cell.operation_id)?,
            oracle_id: required_token("cells.oracle_id", &cell.oracle_id)?,
        });
    }
    validate_denominator_cells(&cells)?;
    Ok(Denominator {
        panel: parse_panel(&raw.panel)?,
        corpus_id: closed_token("corpus_id", &raw.corpus_id)?,
        corpus_sha256: parse_digest(&raw.corpus_sha256)?,
        kind: parse_kind(&raw.kind)?,
        cells,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvidence {
    schema: String,
    artifact: RawArtifact,
    revisions: RawRevisions,
    authority_profile: String,
    operation: RawOperation,
    result: String,
    #[serde(default)]
    waiver: Option<RawWaiver>,
    evidence_uri: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArtifact {
    sha256: String,
    platform: String,
    arch: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRevisions {
    keld: String,
    bun: String,
    engine: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOperation {
    id: String,
    kind: String,
    oracle: RawOracle,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOracle {
    id: String,
    revision: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWaiver {
    owner: String,
    reason: String,
    expires_on: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDenominator {
    schema: String,
    panel: String,
    corpus_id: String,
    corpus_sha256: String,
    kind: String,
    cells: Vec<RawCell>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCell {
    operation_id: String,
    oracle_id: String,
}

fn parse_object<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    max: usize,
) -> Result<T, EvidenceError> {
    if bytes.len() > max {
        return Err(EvidenceError::TooLarge {
            len: bytes.len(),
            max,
        });
    }
    if bytes.is_empty() {
        return Err(EvidenceError::InvalidJson {
            detail: "empty input".to_owned(),
        });
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Err(EvidenceError::NotUtf8);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| EvidenceError::NotUtf8)?;
    serde_json::from_str(text).map_err(|err| {
        let msg = err.to_string();
        if msg.contains("unknown field") || msg.contains("missing field") {
            EvidenceError::InvalidRecord { detail: msg }
        } else {
            EvidenceError::InvalidJson { detail: msg }
        }
    })
}

pub(super) fn parse_digest(value: &str) -> Result<String, EvidenceError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("digest `{value}` must start with sha256:"),
        });
    };
    if hex.len() != 64 || !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("digest `{value}` must be sha256: plus 64 lowercase hex chars"),
        });
    }
    Ok(value.to_owned())
}

fn parse_platform(value: &str) -> Result<Platform, EvidenceError> {
    match value {
        "macos" => Ok(Platform::Macos),
        "windows" => Ok(Platform::Windows),
        "linux" => Ok(Platform::Linux),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("platform `{other}` is not macos|windows|linux"),
        }),
    }
}

fn parse_arch(value: &str) -> Result<Arch, EvidenceError> {
    match value {
        "aarch64" => Ok(Arch::Aarch64),
        "x86_64" => Ok(Arch::X86_64),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("arch `{other}` is not aarch64|x86_64"),
        }),
    }
}

fn parse_authority(value: &str) -> Result<AuthorityProfile, EvidenceError> {
    match value {
        "strict_bun" => Ok(AuthorityProfile::StrictBun),
        "sandboxed_addon_worker" => Ok(AuthorityProfile::SandboxedAddonWorker),
        "legacy_sandbox_off" => Ok(AuthorityProfile::LegacySandboxOff),
        "user_approved_tool_child" => Ok(AuthorityProfile::UserApprovedToolChild),
        "unverified" => Ok(AuthorityProfile::Unverified),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("unknown authority_profile `{other}`"),
        }),
    }
}

fn parse_kind(value: &str) -> Result<OperationKind, EvidenceError> {
    match value {
        "install" => Ok(OperationKind::Install),
        "activation" => Ok(OperationKind::Activation),
        "primary_workflow" => Ok(OperationKind::PrimaryWorkflow),
        "full_feature" => Ok(OperationKind::FullFeature),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("kind `{other}` is not a v1 denominator kind"),
        }),
    }
}

fn parse_panel(value: &str) -> Result<Panel, EvidenceError> {
    match value {
        "product" => Ok(Panel::Product),
        "showcase" => Ok(Panel::Showcase),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("panel `{other}` is not product|showcase"),
        }),
    }
}

fn parse_verdict(value: &str) -> Result<Verdict, EvidenceError> {
    match value {
        "pass" => Ok(Verdict::Pass),
        "fail" => Ok(Verdict::Fail),
        "unknown" => Ok(Verdict::Unknown),
        "waived" => Ok(Verdict::Waived),
        other => Err(EvidenceError::InvalidRecord {
            detail: format!("result `{other}` is not pass|fail|unknown|waived"),
        }),
    }
}

fn parse_waiver(raw: &RawWaiver) -> Result<Waiver, EvidenceError> {
    let owner = raw.owner.trim();
    let reason = raw.reason.trim();
    if owner.is_empty() || reason.is_empty() {
        return Err(EvidenceError::InvalidWaiver {
            detail: "owner and reason must be non-empty".to_owned(),
        });
    }
    Ok(Waiver {
        owner: owner.to_owned(),
        reason: reason.to_owned(),
        expires_on: CivilDate::parse(&raw.expires_on)?,
    })
}

fn pinned_revision(field: &str, value: &str) -> Result<String, EvidenceError> {
    let trimmed = required_token(field, value)?;
    if trimmed.eq_ignore_ascii_case("latest") {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("{field} must be a pinned revision, not `latest`"),
        });
    }
    Ok(trimmed)
}

fn required_token(field: &str, value: &str) -> Result<String, EvidenceError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("{field} must be non-empty"),
        });
    }
    Ok(trimmed.to_owned())
}

fn closed_token(field: &str, value: &str) -> Result<String, EvidenceError> {
    let trimmed = required_token(field, value)?;
    if !trimmed
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
    {
        return Err(EvidenceError::InvalidRecord {
            detail: format!("{field} `{trimmed}` must match [a-z0-9._-]"),
        });
    }
    Ok(trimmed)
}

pub(super) fn validate_denominator_cells(cells: &[CellKey]) -> Result<(), EvidenceError> {
    if cells.is_empty() {
        return Err(EvidenceError::InvalidDenominator {
            detail: "cells must contain at least one entry".to_owned(),
        });
    }
    let mut seen = BTreeSet::new();
    for cell in cells {
        if !seen.insert(cell) {
            return Err(EvidenceError::InvalidDenominator {
                detail: format!("duplicate cell `{}`", cell.label()),
            });
        }
    }
    Ok(())
}
