pub(crate) mod admission;
pub(crate) mod control;
#[cfg(feature = "profile-test-hooks")]
pub(crate) mod cross_user;
pub(crate) mod dev_cycle;
pub(crate) mod invalid_stage;
pub(crate) mod lease_descriptors;
pub(crate) mod native_window;
pub(crate) mod process;
pub(crate) mod product;
#[cfg(feature = "profile-test-hooks")]
pub(crate) mod profile_evidence;
#[cfg(feature = "profile-test-hooks")]
pub(crate) mod profile_origin;
#[cfg(feature = "profile-test-hooks")]
pub(crate) mod profile_renderer;
pub(crate) mod recovery_cycle;
pub(crate) mod renderer;
#[cfg(feature = "profile-test-hooks")]
pub(crate) mod signed_app;
pub(crate) mod unix_descriptors;

use std::time::Duration;

/// Dark background for fixture renderers, so a test run does not flash
/// white windows across the operator's desktop. Cosmetic only: no test
/// asserts on it, and the beacon/marker contracts are unchanged.
pub(crate) const DARK_BG: &str = "<style>html,body{background:#111;color:#eee}</style>";

pub(crate) const TITLE: &str = "KEL96 T1b Fixture";

pub(crate) const MARKER: &str = "KEL96_T1B_EXACT_RENDERER_7e2d9b";

pub(crate) const FORWARDED_LOG: &str = "KEL96_T2_FORWARDED_LOG";

pub(crate) const EVENT_DEADLINE: Duration = Duration::from_secs(15);

#[cfg(feature = "profile-test-hooks")]
pub(crate) const MEDIA_PROMPT_DEADLINE: Duration = Duration::from_mins(2);

pub(crate) const PROCESS_DEADLINE: Duration = Duration::from_secs(5);
