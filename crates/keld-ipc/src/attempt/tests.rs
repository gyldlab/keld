//! `keld-attempt` endpoint creation, readback and client refusal, and the
//! claim and health exchange, on real Windows pipes (KEL-53 §4 "Candidate
//! connect-back", "Machine-UAC bootstrap" item 1; §7 "8 (endpoint
//! squatting)", "8 (claimant binding)", "8 (keld-attempt codec)", "8 (health
//! sequence)" and "17 (argument shape)").
//!
//! Oracles: hand-written SDDL literals, test-local Win32 reads of handle and
//! pipe flags and of a client token's impersonation level, and squatter pipes
//! that `oracle` creates directly with `CreateNamedPipeW`, never through the
//! code under test. This process's own token facts come from the shared reader,
//! which `windows_named_pipe::token_facts_tests` checks against independent
//! token reads.
//!
//! The suite is grouped by contract: `endpoint` is the owner side (descriptor
//! forms, first-instance creation and readback, and error text), `client` the
//! claimant side (rendezvous-name shape, server session, exact-form admission,
//! identification level and squatted descriptor facts), `claim` the owner's
//! admission of a claim (refusal and re-arm, the one-shot, a failed revert),
//! `deadline` the per-connection and claim deadlines, `claimant` the claim's
//! binding and the claimant's checks (the name from the IDs, the locator and
//! server-process checks before `KELD-AA1`, the receipt), `health` the records
//! and the window at the owner, and `outcome` `KELD-AK1`, the close wait and
//! the rollback. `oracle` is the shared independent Win32 oracle and `peer`
//! the exchange fixtures; the scenarios depend on them, never the reverse.

mod claim;
mod claimant;
mod client;
mod deadline;
mod endpoint;
mod health;
mod oracle;
mod outcome;
mod peer;
