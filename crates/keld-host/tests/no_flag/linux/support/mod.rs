//! Narrow Linux acceptance mechanisms; no scenario orchestration.

pub(super) mod control;
pub(super) mod process;
pub(super) mod project;
pub(super) mod renderer;
pub(super) mod stage;

use std::time::Duration;
pub(crate) const PRODUCT_DEADLINE: Duration = Duration::from_secs(20);
