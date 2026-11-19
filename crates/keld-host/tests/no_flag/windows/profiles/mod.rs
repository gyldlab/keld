//! Signed Windows profile acceptance scenarios.

mod cross_user;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod facade_browser;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod facade_browser_crash;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod facade_browser_fixture;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod facade_browser_observation;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod facade_failed_build;
mod facade_lifecycle;
mod handover;
mod installed;
mod lifecycle;
mod purge;
mod release_witness;
mod sessions;
mod startup;
mod storage;
mod storage_contract;
