//! gh566 X01-T4: every registered corpus is validated, admitted and census-checked here.
//!
//! Spec: `docs/specs/gh566-corpus-manifest-owner.md` §4.2 D8 and §7. `registry` runs
//! the shared owner over every committed corpus in `REGISTRY`; `rules` holds the
//! negative controls for each rule on in-memory mutations of the committed bytes.
//! A consumer that registers a corpus gets these checks with no further code.

#![allow(clippy::expect_used, clippy::panic)]
// test-only assertion context
// The owner's typed `CorpusError` is large on purpose (see the owner's module note).
#![allow(clippy::result_large_err)]

#[path = "../support/corpus_admission.rs"]
mod corpus_admission;
#[path = "../support/corpus_census.rs"]
mod corpus_census;
#[path = "../support/corpus_manifest.rs"]
mod corpus_manifest;
mod registry;
mod rules;
