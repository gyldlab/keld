//! `keld-attempt` endpoint creation, readback and client refusal on real
//! Windows pipes (KEL-53 §4 "Candidate connect-back", "Machine-UAC bootstrap"
//! item 1; §7 "8 (endpoint squatting)" and "17 (argument shape)").
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
//! identification level and squatted descriptor facts), and `oracle` the
//! shared independent Win32 oracle both depend on, never the reverse.

mod client;
mod endpoint;
mod oracle;
