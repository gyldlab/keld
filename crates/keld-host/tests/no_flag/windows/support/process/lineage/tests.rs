use super::{
    GenerationEdge, ProcessGeneration, capture_candidate_rows, select_descendant_generations,
    terminal_descendant_candidates,
};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

const fn process(pid: u32, birth: u64) -> ProcessGeneration {
    ProcessGeneration { pid, birth }
}

const fn edge(pid: u32, parent: u32, birth: u64) -> GenerationEdge {
    GenerationEdge {
        process: process(pid, birth),
        parent,
    }
}

#[test]
fn installed_lineage_rejects_the_captured_old_process_under_a_reused_parent_pid() {
    let root = process(22_356, 134_383_366_230_671_280);
    // The first entry is the actual unrelated vctip process from row02. Its
    // image is deliberately not a selector input: ancestry is generation-based.
    let edges = [
        edge(21_492, 1_096, 134_383_347_509_864_179),
        edge(17_644, root.pid, 134_383_366_231_432_717),
        edge(18_636, root.pid, 134_383_366_232_336_893),
        edge(1_096, 18_636, 134_383_366_233_771_572),
    ];
    let actual = select_descendant_generations(root, root, &edges).unwrap();
    assert_eq!(
        actual,
        BTreeSet::from([edges[1].process, edges[2].process, edges[3].process]),
        "an older process attached through a recycled parent PID is not a fixture descendant",
    );
}

#[test]
fn installed_lineage_checks_each_edge_not_only_the_host_birth() {
    let root = process(1, 100);
    let edges = [
        edge(4, 3, 450),
        edge(3, 2, 250),
        edge(2, 1, 300),
        edge(5, 2, 400),
    ];
    let actual = select_descendant_generations(root, root, &edges).unwrap();
    assert_eq!(
        actual,
        BTreeSet::from([edges[2].process, edges[3].process]),
        "a rejected intermediate generation must not admit its later descendants",
    );
}

#[test]
fn installed_lineage_refuses_a_different_root_generation() {
    let root = process(1, 100);
    let edges = [edge(2, 1, 300)];
    for observed in [process(1, 200), process(9, 100)] {
        assert!(
            select_descendant_generations(root, observed, &edges).is_err(),
            "the census must remain anchored to the original retained Host",
        );
    }
}

#[test]
fn installed_lineage_retains_all_valid_branches_in_any_snapshot_order() {
    let root = process(1, 100);
    let edges = [
        edge(7, 6, 400),
        edge(4, 3, 350),
        edge(2, 1, 110),
        edge(6, 1, 250),
        edge(3, 2, 300),
    ];
    let expected: BTreeSet<_> = edges.iter().map(|entry| entry.process).collect();
    assert_eq!(
        select_descendant_generations(root, root, &edges).unwrap(),
        expected,
        "all real descendants, including nonbrowser images, remain obligations",
    );
}

#[test]
fn installed_lineage_allows_equal_creation_ticks_and_an_empty_root() {
    let root = process(1, 100);
    let edges = [edge(3, 2, 100), edge(2, 1, 100)];
    assert_eq!(
        select_descendant_generations(root, root, &edges).unwrap(),
        BTreeSet::from([edges[0].process, edges[1].process]),
    );
    assert!(
        select_descendant_generations(root, root, &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn installed_lineage_rejects_missing_or_ambiguous_identity_evidence() {
    let root = process(1, 100);
    for edges in [
        vec![edge(2, 1, 200), edge(2, 1, 300)],
        vec![edge(2, 1, 0)],
        vec![edge(2, 9, 200)],
        vec![edge(1, 1, 100)],
    ] {
        assert!(
            select_descendant_generations(root, root, &edges).is_err(),
            "unproved lineage cannot produce a successful retained census",
        );
    }
}

fn census(hints: &str, rows: &str) -> String {
    format!(
        "process tree of 1: host_alive=True browser_processes=0 creation_filetimes={hints} descendants={rows}"
    )
}

#[test]
fn installed_lineage_rejects_an_ancient_unretained_process_before_open() {
    let root = process(1, 100_000_000);
    let tree = census(
        "2:110000000,6880:1,9:130000000",
        "old.exe:6880<2,leaf.exe:9<6880,parent.exe:2<1",
    );
    let mut handles = BTreeMap::new();
    let mut opened = Vec::new();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        capture_candidate_rows(
            &tree,
            root,
            &mut handles,
            |pid| {
                opened.push(pid);
                assert_eq!(
                    pid, 2,
                    "simulated access denied for an impossible candidate"
                );
                110_000_000
            },
            |birth| *birth,
        )
    }));
    assert!(
        outcome.is_ok(),
        "an impossible unretained process reached the strict native opener",
    );
    let (rows, changed) = outcome.unwrap();
    assert!(changed);
    assert_eq!(
        opened,
        [2],
        "only the plausible parent needs a native handle"
    );
    assert_eq!(rows, vec![("parent.exe".to_owned(), 2, 1)]);
    assert_eq!(handles.keys().copied().collect::<Vec<_>>(), [2]);
}

#[test]
fn installed_lineage_checks_the_opened_parent_before_a_later_stale_child() {
    let root = process(1, 100_000_000);
    // The child is newer than the Host, but older than the actual parent.
    // The parent's CIM hint is deliberately insufficient: only its kernel
    // birth, supplied by the retained fake handle, can reject this edge.
    let tree = census(
        "6880:300000000,2:200000000",
        "old.exe:6880<2,parent.exe:2<1",
    );
    let mut handles = BTreeMap::new();
    let mut opened = Vec::new();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        capture_candidate_rows(
            &tree,
            root,
            &mut handles,
            |pid| {
                opened.push(pid);
                assert_eq!(pid, 2, "simulated access denied after PID reuse");
                500_000_000
            },
            |birth| *birth,
        )
    }));
    assert!(
        outcome.is_ok(),
        "root-age filtering opened a child older than its retained parent",
    );
    let (rows, changed) = outcome.unwrap();
    assert!(changed);
    assert_eq!(opened, [2]);
    assert_eq!(rows, vec![("parent.exe".to_owned(), 2, 1)]);
}

#[test]
fn installed_lineage_preserves_denial_for_unknown_or_plausible_creation() {
    let root = process(1, 100_000_000);
    for hint in [
        "",
        "2:",
        "2",
        "2:0",
        "2:invalid",
        "2:18446744073709551616",
        "2:18446744073709551615",
        "2:1,2:1",
        "2:1,2:invalid",
        "3:1",
        "2:90000000",
        "2:99999999",
        "2:100000000",
        "2:110000000",
    ] {
        let tree = census(hint, "candidate.exe:2<1");
        let mut handles = BTreeMap::<u32, u64>::new();
        let mut opened = Vec::new();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            capture_candidate_rows(
                &tree,
                root,
                &mut handles,
                |pid| {
                    opened.push(pid);
                    panic!("simulated access denied remains fatal");
                },
                |birth| *birth,
            )
        }));
        assert!(outcome.is_err(), "creation hint {hint:?} hid native denial");
        assert_eq!(opened, [2], "unknown evidence must still reach the opener");
    }
}

#[test]
fn installed_lineage_never_replaces_a_retained_handle_with_a_creation_hint() {
    let root = process(1, 100_000_000);
    let mut handles = BTreeMap::from([(2, 110_000_000)]);
    let (rows, opened) = capture_candidate_rows(
        &census("2:1", "retained.exe:2<1"),
        root,
        &mut handles,
        |_| panic!("a retained native handle must not be reopened"),
        |birth| *birth,
    );
    assert!(!opened);
    assert_eq!(rows, vec![("retained.exe".to_owned(), 2, 1)]);
    assert_eq!(handles[&2], 110_000_000);
}

#[test]
fn installed_lineage_reconsiders_a_rejected_pid_in_the_next_snapshot() {
    let root = process(1, 100_000_000);
    let mut handles = BTreeMap::<u32, u64>::new();
    let mut opened = Vec::new();
    let first = catch_unwind(AssertUnwindSafe(|| {
        capture_candidate_rows(
            &census("2:1", "old.exe:2<1"),
            root,
            &mut handles,
            |_| panic!("the first snapshot proves an impossible edge"),
            |birth| *birth,
        )
    }));
    assert!(
        first.is_ok(),
        "a rejected snapshot attempted native opening"
    );
    assert_eq!(first.unwrap(), (Vec::new(), false));
    let (rows, changed) = capture_candidate_rows(
        &census("2:110000000", "new.exe:2<1"),
        root,
        &mut handles,
        |pid| {
            opened.push(pid);
            110_000_000
        },
        |birth| *birth,
    );
    assert!(changed);
    assert_eq!(opened, [2]);
    assert_eq!(rows, vec![("new.exe".to_owned(), 2, 1)]);
}

#[test]
fn installed_lineage_creation_hint_cannot_grant_native_ownership() {
    let root = process(1, 100_000_000);
    let mut handles = BTreeMap::new();
    let (rows, opened) = capture_candidate_rows(
        &census("2:110000000", "candidate.exe:2<1"),
        root,
        &mut handles,
        |_| 90_000_000,
        |birth| *birth,
    );
    assert!(opened);
    assert_eq!(rows, vec![("candidate.exe".to_owned(), 2, 1)]);
    let edges = [edge(rows[0].1, rows[0].2, handles[&2])];
    assert!(
        select_descendant_generations(root, root, &edges)
            .unwrap()
            .is_empty(),
        "a plausible metadata date must not override the retained kernel generation",
    );
}

#[test]
fn installed_lineage_does_not_erase_a_missing_parent_with_metadata() {
    let root = process(1, 100_000_000);
    let mut handles = BTreeMap::from([(2, 200_000_000)]);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let (rows, _) = capture_candidate_rows(
            &census("3:1", "candidate.exe:3<2"),
            root,
            &mut handles,
            |_| 50_000_000,
            |birth| *birth,
        );
        let edges = rows
            .iter()
            .map(|(_, pid, parent)| edge(*pid, *parent, handles[pid]))
            .collect::<Vec<_>>();
        select_descendant_generations(root, root, &edges)
    }));
    let reason = match outcome {
        Ok(result) => result
            .expect_err("metadata erased the missing-parent failure")
            .to_owned(),
        Err(payload) => payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .expect("the ancestry guard returns a diagnostic string"),
    };
    assert_eq!(
        reason,
        "native descendant parent is missing from the snapshot",
    );
}

#[test]
fn installed_terminal_lineage_rejects_the_captured_old_asus_process() {
    let tree = "process tree of 5540: host_alive=False browser_processes=0 \
        creation_filetimes=18024:134383662447869630 descendants=AsusOSD.exe:18024<5540";
    assert!(
        terminal_descendant_candidates(tree, 5_540, 134_383_911_943_824_983).is_empty(),
        "the captured process predates the retained Host by hours",
    );
}

#[test]
fn installed_terminal_lineage_keeps_newer_unknown_and_rejected_branch_children() {
    let tree = census(
        "2:1,3:110000000,5:120000000",
        "old.exe:2<1,other.exe:3<1,unknown.exe:4<1,leaf.exe:5<2",
    );
    assert_eq!(
        terminal_descendant_candidates(&tree, 1, 100_000_000),
        vec![
            ("other.exe".to_owned(), 3, 1),
            ("unknown.exe".to_owned(), 4, 1),
            ("leaf.exe".to_owned(), 5, 2),
        ],
        "rejection grants no ownership or exemption to other rows",
    );
}

#[test]
fn installed_terminal_lineage_preserves_uncertain_creation_obligations() {
    for hint in [
        "",
        "2:",
        "2",
        "2:0",
        "2:invalid",
        "2:18446744073709551616",
        "2:18446744073709551615",
        "2:1,2:1",
        "2:1,2:invalid",
        "3:1",
        "2:90000000",
        "2:99999999",
        "2:100000000",
        "2:110000000",
    ] {
        assert_eq!(
            terminal_descendant_candidates(&census(hint, "candidate.exe:2<1"), 1, 100_000_000),
            vec![("candidate.exe".to_owned(), 2, 1)],
            "uncertain hint {hint:?} must not erase a cleanup obligation",
        );
    }
}

#[test]
fn installed_terminal_lineage_does_not_hide_malformed_ancestry() {
    for rows in [
        "first.exe:2<1,second.exe:2<1",
        "root.exe:1<1",
        "orphan.exe:2<9",
    ] {
        assert!(
            catch_unwind(|| terminal_descendant_candidates(
                &census("1:1,2:1", rows),
                1,
                100_000_000
            ))
            .is_err(),
            "old metadata must not erase an ambiguous or incomplete ancestry report",
        );
    }
    assert!(
        catch_unwind(|| terminal_descendant_candidates(&census("", ""), 1, 0)).is_err(),
        "an empty diagnostic report cannot supply the retained Host generation",
    );
}

#[test]
fn installed_terminal_lineage_reconsiders_the_same_pid_in_each_snapshot() {
    assert_eq!(
        terminal_descendant_candidates(&census("2:1", "old.exe:2<1"), 1, 100_000_000),
        Vec::<(String, u32, u32)>::new(),
    );
    assert_eq!(
        terminal_descendant_candidates(&census("2:110000000", "new.exe:2<1"), 1, 100_000_000),
        vec![("new.exe".to_owned(), 2, 1)],
        "an earlier rejection cannot exempt a later process generation",
    );
}
