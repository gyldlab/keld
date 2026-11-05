use super::{GenerationEdge, ProcessGeneration, select_descendant_generations};
use std::collections::BTreeSet;

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
