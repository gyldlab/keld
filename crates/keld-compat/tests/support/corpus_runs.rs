//! Record runs and the pending/divergence split (gh532 AC2, AC7–AC9, AC12, AC17; gh566
//! C5, C6, D7, D11). A child module of `corpus_manifest.rs`, split out under the gh566 D1
//! review condition. Its invariant: a run's records agree with their corpus and cells,
//! carry the label their run allows, and are scored exactly once by KEL-74 `score()`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use keld_compat::evidence::{
    AuthorityProfile, CellKey, CivilDate, EvidenceRecord, Panel, Scoreboard, Verdict, score,
};

use super::{
    Cell, Corpus, CorpusError, DIVERGENCE_LABEL, PENDING_LABEL, kind_token, platform_token,
    verdict_token,
};

/// A validated run: one `(platform, arch)`, scored once by KEL-74.
#[derive(Debug, Clone)]
pub struct Run {
    board: Scoreboard,
    split: FailSplit,
}

impl Run {
    /// The KEL-74 scoreboard of this run.
    pub fn board(&self) -> &Scoreboard {
        &self.board
    }

    /// Takes the scoreboard.
    pub fn into_board(self) -> Scoreboard {
        self.board
    }

    /// The pending and divergence `fail` cells of this run (gh532 AC17).
    pub fn fail_split(&self) -> &FailSplit {
        &self.split
    }
}

/// The `fail` records of a run, split by their cell's manifest key (gh532 rule 3). The
/// records cannot tell the two apart; the manifest key does. Keyed by the full KEL-74
/// `CellKey`, so two cells that share an operation are counted apart (gh566 D11).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FailSplit {
    pending: BTreeMap<CellKey, String>,
    divergence: BTreeSet<CellKey>,
}

impl FailSplit {
    /// Red-until-implemented cells and their implementing tickets.
    pub fn pending(&self) -> &BTreeMap<CellKey, String> {
        &self.pending
    }

    /// Permanent-divergence cells, reported as ▲.
    pub fn divergence(&self) -> &BTreeSet<CellKey> {
        &self.divergence
    }
}

/// Always both lines, labelled by the owner constants. Ticket keys are sorted and
/// deduplicated, and a zero count has no parenthesis (gh566 D11).
impl fmt::Display for FailSplit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tickets: BTreeSet<&str> = self.pending.values().map(String::as_str).collect();
        if tickets.is_empty() {
            writeln!(f, "{PENDING_LABEL}: 0")?;
        } else {
            let keys: Vec<&str> = tickets.into_iter().collect();
            writeln!(
                f,
                "{PENDING_LABEL}: {} ({})",
                self.pending.len(),
                keys.join(", ")
            )?;
        }
        writeln!(f, "{DIVERGENCE_LABEL}: {}", self.divergence.len())
    }
}

/// A KEL-78 profile state recorded by a product run receipt (gh532 rule 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileState {
    /// No verified containment.
    Unverified,
    /// The explicit Keld legacy declaration.
    Legacy,
    /// KEL-78 admission plus the complete OS-containment archive.
    Strict,
}

impl ProfileState {
    /// The state's name in rejections.
    pub const fn token(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Legacy => "legacy",
            Self::Strict => "strict",
        }
    }

    /// The one label a record of this state carries (gh532 rule 6 table).
    pub const fn required_label(self) -> AuthorityProfile {
        match self {
            Self::Unverified => AuthorityProfile::Unverified,
            Self::Legacy => AuthorityProfile::LegacySandboxOff,
            Self::Strict => AuthorityProfile::StrictBun,
        }
    }
}

/// The owner's minimal view of a product run receipt; X02-T5's parser builds it.
#[derive(Clone, Copy, Debug)]
pub struct ProductReceipt<'a> {
    /// The KEL-78 state the receipt records.
    pub state: ProfileState,
    /// Every cell the run covered.
    pub cells: &'a [CellKey],
}

impl Corpus {
    /// Validates one harness (showcase) run and scores it once (gh566 D7).
    pub fn validate_harness_run(
        &self,
        records: &[EvidenceRecord],
        as_of: CivilDate,
    ) -> Result<Run, CorpusError> {
        if self.panel != Panel::Showcase {
            return Err(CorpusError::NotHarnessPanel {
                corpus_id: self.id.clone(),
            });
        }
        self.check_run_identity(records, true)?;
        for record in records {
            self.check_record(record)?;
            if record.authority_profile() != self.harness_profile {
                return Err(CorpusError::LabelMismatch {
                    corpus_id: self.id.clone(),
                    cell: label_of(record),
                    receipt_state: "conformance-harness",
                    label: record.authority_profile().as_str(),
                });
            }
        }
        self.score_run(records, as_of)
    }

    /// Validates one product run against its receipt (gh532 AC8, AC12; gh566 D7). The
    /// record cells must equal the receipt's cells exactly, checked before scoring, so
    /// a cell the run never covered cannot score.
    pub fn validate_product_run(
        &self,
        receipt: &ProductReceipt<'_>,
        records: &[EvidenceRecord],
        as_of: CivilDate,
    ) -> Result<Run, CorpusError> {
        if self.panel != Panel::Product {
            return Err(CorpusError::NotProductPanel {
                corpus_id: self.id.clone(),
            });
        }
        self.check_run_identity(records, receipt.cells.is_empty())?;
        for record in records {
            self.check_record(record)?;
            if !receipt.cells.contains(&key_of(record)) {
                return Err(CorpusError::UncoveredProductRecord {
                    corpus_id: self.id.clone(),
                    cell: label_of(record),
                });
            }
        }
        for cell in receipt.cells {
            if !records.iter().any(|record| key_of(record) == *cell) {
                return Err(CorpusError::MissingProductRecord {
                    corpus_id: self.id.clone(),
                    cell: format!("{}/{}", cell.operation_id, cell.oracle_id),
                    receipt_state: receipt.state.token(),
                });
            }
        }
        let required = receipt.state.required_label();
        if let Some(record) = records
            .iter()
            .find(|record| record.authority_profile() != required)
        {
            return Err(CorpusError::LabelMismatch {
                corpus_id: self.id.clone(),
                cell: label_of(record),
                receipt_state: receipt.state.token(),
                label: record.authority_profile().as_str(),
            });
        }
        self.score_run(records, as_of)
    }

    /// One `(platform, arch)` per run; an empty run is rejected when `empty_is_error`.
    fn check_run_identity(
        &self,
        records: &[EvidenceRecord],
        empty_is_error: bool,
    ) -> Result<(), CorpusError> {
        let Some(first) = records.first() else {
            return if empty_is_error {
                Err(CorpusError::EmptyRun {
                    corpus_id: self.id.clone(),
                })
            } else {
                Ok(())
            };
        };
        let identity = (first.artifact().platform, first.artifact().arch);
        if records
            .iter()
            .any(|record| (record.artifact().platform, record.artifact().arch) != identity)
        {
            return Err(CorpusError::MixedRun {
                corpus_id: self.id.clone(),
            });
        }
        Ok(())
    }

    /// Scores once; KEL-74's `DuplicateCell` is the uniqueness check (C5, F11).
    fn score_run(&self, records: &[EvidenceRecord], as_of: CivilDate) -> Result<Run, CorpusError> {
        let board = score(&self.denominator, records, as_of).map_err(CorpusError::Evidence)?;
        let mut split = FailSplit::default();
        for record in records {
            if record.result() != Verdict::Fail {
                continue;
            }
            if let Some(cell) = self.cell_of(record) {
                match &cell.implementing_ticket {
                    Some(ticket) => {
                        split.pending.insert(cell.key.clone(), ticket.clone());
                    }
                    None => {
                        split.divergence.insert(cell.key.clone());
                    }
                }
            }
        }
        Ok(Run { board, split })
    }

    fn cell_of(&self, record: &EvidenceRecord) -> Option<&Cell> {
        let key = key_of(record);
        self.cells.iter().find(|cell| cell.key == key)
    }

    /// The record rules shared by both run kinds (gh566 C5, C10; gh532 AC2, AC7, AC9).
    fn check_record(&self, record: &EvidenceRecord) -> Result<(), CorpusError> {
        let operation = record.operation();
        let platform = record.artifact().platform;
        let cell_label = label_of(record);
        let mismatch =
            |field: &'static str, found: String, expected: String| CorpusError::RecordMismatch {
                corpus_id: self.id.clone(),
                cell: cell_label.clone(),
                platform: platform_token(platform),
                field,
                found,
                expected,
            };
        let cell = self.cell_of(record).ok_or_else(|| {
            mismatch(
                "cell",
                cell_label.clone(),
                "a manifest cell (operation_id, oracle_id)".to_owned(),
            )
        })?;
        if operation.kind != self.kind {
            return Err(mismatch(
                "operation.kind",
                kind_token(operation.kind).to_owned(),
                kind_token(self.kind).to_owned(),
            ));
        }
        if record.artifact().sha256 != self.digest {
            return Err(mismatch(
                "artifact.sha256",
                record.artifact().sha256.clone(),
                self.digest.clone(),
            ));
        }
        let revision = self.pin.oracle_revision();
        if operation.oracle.revision != revision {
            return Err(CorpusError::PinMismatch {
                corpus_id: self.id.clone(),
                cell: format!("record {cell_label} on {}", platform_token(platform)),
                found: operation.oracle.revision.clone(),
                expected: revision,
            });
        }
        let result = record.result();
        if !cell.platforms.contains(&platform) {
            if result != Verdict::Unknown {
                return Err(mismatch(
                    "result",
                    verdict_token(result).to_owned(),
                    "unknown, because the cell does not declare this platform".to_owned(),
                ));
            }
        } else if result != cell.expected && result != Verdict::Unknown {
            return Err(mismatch(
                "result",
                verdict_token(result).to_owned(),
                format!("{} or unknown", verdict_token(cell.expected)),
            ));
        }
        if record.waiver().is_some() {
            return Err(mismatch(
                "waiver",
                "a waiver".to_owned(),
                "no waiver".to_owned(),
            ));
        }
        let engine = &record.revisions().engine;
        let declared = self.engine_token(platform);
        let engine_ok = declared.is_some_and(|token| {
            engine.split_once('@').is_some_and(|(found, rev)| {
                found == token
                    && !rev.is_empty()
                    && !rev.contains('@')
                    && !rev.chars().any(char::is_whitespace)
            })
        });
        if !engine_ok {
            return Err(mismatch(
                "revisions.engine",
                engine.clone(),
                declared.map_or_else(
                    || format!("an engine entry for {}", platform_token(platform)),
                    |token| format!("{token}@<pinned revision>"),
                ),
            ));
        }
        Ok(())
    }
}

fn key_of(record: &EvidenceRecord) -> CellKey {
    CellKey {
        operation_id: record.operation().id.clone(),
        oracle_id: record.operation().oracle.id.clone(),
    }
}

fn label_of(record: &EvidenceRecord) -> String {
    format!("{}/{}", record.operation().id, record.operation().oracle.id)
}

/// Validates every committed run of a corpus as a harness run. A product corpus with
/// committed records fails closed until X02-T5's receipt reader exists (gh566 C6).
pub fn validate_committed_runs(
    corpus: &Corpus,
    runs: &[Vec<EvidenceRecord>],
) -> Result<usize, CorpusError> {
    if corpus.panel == Panel::Product && !runs.is_empty() {
        return Err(CorpusError::ProductRecordsNeedReceipt {
            corpus_id: corpus.id.clone(),
        });
    }
    for run in runs {
        corpus.validate_harness_run(run, corpus.records_as_of)?;
    }
    Ok(runs.len())
}
