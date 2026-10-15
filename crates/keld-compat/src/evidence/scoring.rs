//! Compatibility evidence scoring and publishability admission.
//! Parsing, denominator schema and public data types remain parent-owned.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    AuthorityProfile, CellKey, CivilDate, Denominator, EvidenceError, EvidenceRecord, Panel,
    Scoreboard, Verdict, parse_evidence_uri, validate_denominator_cells,
};

/// Score records against a committed denominator.
///
/// Extra records whose cell is not in the denominator are ignored. Records
/// whose `operation.kind` differs from the denominator kind are ignored so a
/// foreign kind cannot fill a cell.
///
/// # Errors
///
/// Returns [`EvidenceError::InvalidDenominator`] when `cells` is empty or
/// contains duplicates, [`EvidenceError::DuplicateCell`] when two records
/// name the same cell, [`EvidenceError::InvalidWaiver`] when a waiver is
/// expired as of `as_of` or paired with a non-[`Verdict::Waived`] result,
/// or [`EvidenceError::NonNormativeEvidence`] when a constructed URI is a
/// lead rather than an immutable pin.
pub fn score(
    denominator: &Denominator,
    records: &[EvidenceRecord],
    as_of: CivilDate,
) -> Result<Scoreboard, EvidenceError> {
    validate_denominator_cells(&denominator.cells)?;
    let required: BTreeSet<&CellKey> = denominator.cells.iter().collect();
    let mut by_cell: BTreeMap<CellKey, &EvidenceRecord> = BTreeMap::new();
    for record in records {
        if record.operation.kind != denominator.kind {
            continue;
        }
        let key = record.cell_key();
        if !required.contains(&key) {
            continue;
        }
        validate_scored_record(record, &key, as_of)?;
        if by_cell.insert(key.clone(), record).is_some() {
            return Err(EvidenceError::DuplicateCell { cell: key.label() });
        }
    }

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut unknown = 0usize;
    let mut waived = 0usize;
    let mut missing = 0usize;
    for cell in &denominator.cells {
        match by_cell.get(cell).map(|record| record.result) {
            Some(Verdict::Pass) => passed += 1,
            Some(Verdict::Fail) => failed += 1,
            Some(Verdict::Unknown) => unknown += 1,
            Some(Verdict::Waived) => waived += 1,
            None => missing += 1,
        }
    }

    let authority_profile = shared_authority_profile(by_cell.values().copied());
    let identity_ok =
        contributing_identity_consistent(by_cell.values().copied(), authority_profile);
    let n = denominator.cells.len();
    let corpus_ok =
        product_corpus_is_documented_committed(denominator.panel, &denominator.corpus_id);
    let may_publish_percent = missing == 0 && unknown == 0 && n > 0 && identity_ok && corpus_ok;
    let unweighted_percent = if may_publish_percent {
        let pct = passed.saturating_mul(100) / n;
        u8::try_from(pct).ok()
    } else {
        None
    };
    let complete = n > 0
        && passed == n
        && missing == 0
        && unknown == 0
        && failed == 0
        && waived == 0
        && identity_ok
        && corpus_ok;
    let claim = format!(
        "{passed}/{n} of {} corpus {}@{} ({})",
        denominator.panel.as_str(),
        denominator.corpus_id,
        denominator.corpus_sha256,
        denominator.kind.as_str()
    );

    Ok(Scoreboard {
        panel: denominator.panel,
        corpus_id: denominator.corpus_id.clone(),
        corpus_sha256: denominator.corpus_sha256.clone(),
        kind: denominator.kind,
        denominator: n,
        passed,
        failed,
        unknown,
        waived,
        missing,
        authority_profile,
        unweighted_percent,
        complete,
        claim,
    })
}

fn validate_scored_record(
    record: &EvidenceRecord,
    key: &CellKey,
    as_of: CivilDate,
) -> Result<(), EvidenceError> {
    validate_scored_waiver(record, key, as_of)?;
    parse_evidence_uri(&record.evidence_uri)?;
    Ok(())
}

fn validate_scored_waiver(
    record: &EvidenceRecord,
    key: &CellKey,
    as_of: CivilDate,
) -> Result<(), EvidenceError> {
    match (record.result, record.waiver.as_ref()) {
        (Verdict::Waived, Some(waiver)) => {
            if waiver.expires_on < as_of {
                return Err(EvidenceError::InvalidWaiver {
                    detail: format!(
                        "waiver for `{}` expired on {} (as_of {as_of})",
                        key.label(),
                        waiver.expires_on
                    ),
                });
            }
            Ok(())
        }
        (Verdict::Waived, None) => Err(EvidenceError::InvalidWaiver {
            detail: "result is waived but waiver is missing".to_owned(),
        }),
        (_, Some(_)) => Err(EvidenceError::InvalidWaiver {
            detail: "waiver is only allowed when result is waived".to_owned(),
        }),
        (_, None) => Ok(()),
    }
}

/// The one profile-agreement rule: `Some` only when at least one record
/// contributes and every contributing record has the same profile. `score`
/// gates identity on it and [`Scoreboard::authority_profile`] reports it.
fn shared_authority_profile<'a>(
    mut records: impl Iterator<Item = &'a EvidenceRecord>,
) -> Option<AuthorityProfile> {
    let first = records.next()?.authority_profile;
    records
        .all(|record| record.authority_profile == first)
        .then_some(first)
}

/// Contributing records share one authority profile (`shared_profile`, from
/// [`shared_authority_profile`]), one artifact digest and one engine.
fn contributing_identity_consistent<'a>(
    mut records: impl Iterator<Item = &'a EvidenceRecord>,
    shared_profile: Option<AuthorityProfile>,
) -> bool {
    let Some(first) = records.next() else {
        return true;
    };
    shared_profile.is_some()
        && records.all(|record| {
            record.artifact.sha256 == first.artifact.sha256
                && record.revisions.engine == first.revisions.engine
        })
}

/// Documented committed product corpus ids. T1: none, so product panels
/// never publish `unweighted_percent` or `complete`.
const DOCUMENTED_COMMITTED_PRODUCT_CORPORA: &[&str] = &[];

/// Showcase may publish. Product may publish only when `corpus_id` is on
/// `DOCUMENTED_COMMITTED_PRODUCT_CORPORA` (empty today).
fn product_corpus_is_documented_committed(panel: Panel, corpus_id: &str) -> bool {
    match panel {
        Panel::Showcase => true,
        Panel::Product => DOCUMENTED_COMMITTED_PRODUCT_CORPORA.contains(&corpus_id),
    }
}
