//! Independent filesystem observations of Linux stages.

use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

/// Confirms the KEL-142 fixture is self-contained before Linux strict remaps
/// `src/main.ts` to `/code/main.ts`. The canonical `@keld/api` owner is
/// bundled into that entry, so no stale compatibility sidecar may remain.
pub(crate) fn assert_self_contained_kipc_entry(root: &Path) {
    let main = fs::read_to_string(root.join("src").join("main.ts")).expect("main.ts");
    assert!(
        !main.contains("kipc-transport.ts"),
        "bundled Linux entry retained a local transport import: {main}"
    );
    assert!(
        !root.join("src").join("kipc-transport.ts").exists(),
        "self-contained Linux entry retained an obsolete src/kipc-transport.ts under {}",
        root.display()
    );
}

pub(crate) fn dev_stage_count(project: &Path) -> usize {
    fs::read_dir(project.join(".keld/dev"))
        .map_or(0, |entries| entries.filter_map(Result::ok).count())
}

pub(crate) fn wait_for_dev_stage_count(project: &Path, expected: usize, deadline: Instant) {
    loop {
        let observed = dev_stage_count(project);
        if observed == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected} dev stages, observed {observed}"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}
