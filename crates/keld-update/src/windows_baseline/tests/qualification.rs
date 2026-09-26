//! Actual SYSTEM transaction and process-crash acceptance. Not a power-loss test.

use std::fs;

use super::support::{self, CASE_ENV, CUT_ENV, baseline, child, provision, trust_for};
use crate::windows_baseline::initialize::{BaselineBoundary, initialize_with_observer};
use crate::windows_baseline::{initialize_windows_baseline, load_windows_baseline};

const CRASH_CHILD: &str = "windows_baseline::tests::qualification::system_crash_child";
const COMPETING_CHILD: &str = "windows_baseline::tests::qualification::system_competing_child";

#[test]
#[ignore = "requires reviewed operator helper running this exact selector as LocalSystem"]
fn system_baseline_qualification() {
    let root = support::create_suite();
    let trust = provision(&root, "success");
    let receipt = initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
        .expect("SYSTEM initializes exact baseline");
    assert_eq!(receipt.loaded().version_floor(), "1.0.0");
    assert_eq!(receipt.loaded().publisher_scope(), &[0x26; 32]);
    assert_eq!(receipt.loaded().identity(), &trust.installation);
    drop(receipt);
    let loaded = load_windows_baseline(&trust).expect("fresh production read-only load");
    assert_eq!(loaded.version_floor(), "1.0.0");
    drop(loaded);
    assert!(
        initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust).is_err(),
        "committed installation cannot be reseeded"
    );
    let mut wrong = trust.clone();
    wrong.publisher_scope[0] ^= 1;
    assert!(load_windows_baseline(&wrong).is_err(), "publisher mismatch");
    wrong = trust.clone();
    wrong.volume_guid.replace_range(11..12, "0");
    if wrong.volume_guid == trust.volume_guid {
        wrong.volume_guid.replace_range(11..12, "1");
    }
    assert!(
        load_windows_baseline(&wrong).is_err(),
        "trusted volume mismatch"
    );

    super::substitutions::run(&root);

    let concurrent = provision(&root, "concurrent");
    initialize_with_observer(
        &baseline(&concurrent),
        &root.join("source.tar"),
        &concurrent,
        |boundary| {
            if boundary == BaselineBoundary::LockCreated {
                let output = child(COMPETING_CHILD, &root, "concurrent", "", 0);
                assert!(output.contains("KELD_KEL266_COMPETITOR_REFUSED"));
            }
            Ok(())
        },
    )
    .expect("first initializer succeeds while second actual process is refused");

    let cuts = [
        BaselineBoundary::LockCreated,
        BaselineBoundary::StageCreated,
        BaselineBoundary::StagePopulated,
        BaselineBoundary::CompletePublished,
        BaselineBoundary::VersionPublished,
        BaselineBoundary::FloorPublished,
        BaselineBoundary::CurrentPublished,
        BaselineBoundary::LastKnownGoodPublished,
        BaselineBoundary::RootsSealed,
        BaselineBoundary::ProvenancePublished,
    ];
    for (index, cut) in cuts.into_iter().enumerate() {
        let case = format!("cut-{index}");
        let cut_trust = provision(&root, &case);
        let output = child(CRASH_CHILD, &root, &case, &format!("{cut:?}"), 91);
        assert!(
            output.contains("KELD_KEL266_CRASH_CUT"),
            "child reached requested boundary"
        );
        let provenance = cut_trust
            .installation
            .install_root
            .join("install-provenance");
        if cut == BaselineBoundary::ProvenancePublished {
            assert!(provenance.is_file());
            let loaded = load_windows_baseline(&cut_trust)
                .expect("provenance-last cut has committed records");
            assert_eq!(loaded.version_floor(), "1.0.0");
            super::super::load::validate_initial_seed(&loaded.roots)
                .expect("complete initial seed after final cut");
        } else {
            assert!(
                !provenance.exists(),
                "precommit cut never publishes provenance"
            );
            assert!(
                load_windows_baseline(&cut_trust).is_err(),
                "partial state is never admitted"
            );
        }
        assert!(
            initialize_windows_baseline(
                &baseline(&cut_trust),
                &root.join("source.tar"),
                &cut_trust
            )
            .is_err(),
            "neither stale lock nor partial state triggers automatic repair"
        );
        println!("KELD_KEL266_CUT_OK={cut:?}");
    }
    // Preserve the explicit success fixture for the separate ordinary-user process.
    // Its read/write denial cannot be inferred from this SYSTEM process's success.
    fs::write(
        root.join("system-finished.txt"),
        b"SYSTEM transaction and process crash cuts passed\n",
    )
    .expect("qualification marker");
    println!("KELD_KEL266_SYSTEM_FINISHED={}", root.display());
}

#[test]
#[ignore = "private subprocess endpoint of SYSTEM qualification"]
fn system_crash_child() {
    keld_guard::require_windows_system_token().expect("actual SYSTEM child");
    let root = support::suite_root();
    let case = std::env::var(CASE_ENV).expect("private scenario selector");
    assert!(case.starts_with("cut-") && !case.contains(['/', '\\']));
    let trust = trust_for(&root.join(case));
    let cut = std::env::var(CUT_ENV).expect("requested persisted boundary");
    let result = initialize_with_observer(
        &baseline(&trust),
        &root.join("source.tar"),
        &trust,
        |boundary| {
            if format!("{boundary:?}") == cut {
                use std::io::Write as _;
                println!("KELD_KEL266_CRASH_CUT={cut}");
                std::io::stdout()
                    .flush()
                    .expect("cut observation before process exit");
                std::process::exit(91);
            }
            Ok(())
        },
    );
    panic!("requested crash boundary was not reached: {result:?}");
}

#[test]
#[ignore = "private subprocess endpoint of SYSTEM qualification"]
fn system_competing_child() {
    keld_guard::require_windows_system_token().expect("actual SYSTEM competitor");
    let root = support::suite_root();
    assert_eq!(
        std::env::var(CASE_ENV).expect("private scenario"),
        "concurrent"
    );
    let trust = trust_for(&root.join("concurrent"));
    let error = initialize_windows_baseline(&baseline(&trust), &root.join("source.tar"), &trust)
        .expect_err("second process must not enter active initializer");
    assert!(matches!(
        error,
        crate::UpdateError::Baseline {
            step: "fresh update" | "exclusive bootstrap lock",
            ..
        }
    ));
    println!("KELD_KEL266_COMPETITOR_REFUSED");
}
