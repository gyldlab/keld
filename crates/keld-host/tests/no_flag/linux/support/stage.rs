//! Independent filesystem observations of Linux stages.

use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

/// Fails if `src/main.ts` imports `./kipc-transport.ts` but the sidecar is
/// missing. The Linux death test remaps the entry to `/code/main.ts`; Bun
/// then resolves that import at `/code/kipc-transport.ts`.
pub(crate) fn assert_imported_kipc_sidecar_exists(root: &Path) {
    let main = fs::read_to_string(root.join("src").join("main.ts")).expect("main.ts");
    assert!(
        main.contains("from \"./kipc-transport.ts\""),
        "entry must import ./kipc-transport.ts so Linux /code/main.ts can resolve the sidecar: {main}"
    );
    assert!(
        root.join("src").join("kipc-transport.ts").is_file(),
        "entry imports ./kipc-transport.ts but src/kipc-transport.ts is missing under {}",
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
