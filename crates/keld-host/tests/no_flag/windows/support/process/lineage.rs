//! Retained native process generations for installed and facade acceptance.
//! Candidate PID graphs remain diagnostic until their kernel identities agree.

use super::{open_process_for_wait, process_times, process_tree_report};
use crate::PRODUCT_DEADLINE;
use std::collections::{BTreeMap, BTreeSet};
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::process::Child;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ProcessGeneration {
    pid: u32,
    birth: u64,
}

#[derive(Clone, Copy)]
struct GenerationEdge {
    process: ProcessGeneration,
    parent: u32,
}

pub(crate) struct DescendantObservation {
    pub(crate) image: String,
    pub(crate) pid: u32,
    pub(crate) parent: u32,
    pub(crate) birth: u64,
    pub(crate) handle: OwnedHandle,
}

/// The diagnostic graph supplies candidates, not ownership. Retain each kernel
/// object before the final graph query, then check every parent generation.
/// <https://learn.microsoft.com/windows/win32/cimwin32prov/win32-process>
/// <https://learn.microsoft.com/windows/win32/api/processthreadsapi/ns-processthreadsapi-process_information>
pub(crate) fn retain_native_descendants(
    root: &mut Child,
    expected_birth: u64,
) -> (String, Vec<DescendantObservation>) {
    let expected_root = ProcessGeneration {
        pid: root.id(),
        birth: expected_birth,
    };
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    let mut handles = BTreeMap::new();
    loop {
        let tree = process_tree_report(root.id());
        assert!(
            Instant::now() < deadline,
            "native ancestry did not stabilize within its existing observation bound"
        );
        assert!(tree.contains("host_alive=True"), "Host is not live: {tree}");
        let rows = descendants(&tree);
        let missing = rows
            .iter()
            .filter(|(_, pid, _)| !handles.contains_key(pid))
            .map(|(_, pid, _)| *pid)
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            for pid in missing {
                handles.insert(pid, open_process_for_wait(pid, false));
            }
            // An open process handle keeps that process object/identifier alive.
            // A later graph query therefore cannot substitute a recycled PID.
            continue;
        }
        let root_times = process_times(root.as_raw_handle().cast());
        assert!(
            root.try_wait().expect("query the retained Host").is_none(),
            "Host exited during native ancestry capture"
        );
        let observed_root = ProcessGeneration {
            pid: root.id(),
            birth: root_times.creation,
        };
        let edges = rows
            .iter()
            .map(|(_, pid, parent)| GenerationEdge {
                process: ProcessGeneration {
                    pid: *pid,
                    birth: process_times(
                        handles
                            .get(pid)
                            .expect("captured before this census")
                            .as_raw_handle()
                            .cast(),
                    )
                    .creation,
                },
                parent: *parent,
            })
            .collect::<Vec<_>>();
        let admitted = select_descendant_generations(expected_root, observed_root, &edges)
            .unwrap_or_else(|error| panic!("invalid native ancestry: {error}; {tree}"));
        let retained = rows
            .into_iter()
            .zip(edges)
            .filter_map(|((image, pid, parent), edge)| {
                if admitted.contains(&edge.process) {
                    Some(DescendantObservation {
                        image,
                        pid,
                        parent,
                        birth: edge.process.birth,
                        handle: handles.remove(&pid).expect("retained native generation"),
                    })
                } else {
                    println!(
                        "KELD_GH1188_NATIVE_LINEAGE_REJECT host={} host_birth={expected_birth} pid={pid} parent={parent} image={image} birth={}",
                        root.id(),
                        edge.process.birth,
                    );
                    None
                }
            })
            .collect();
        return (tree, retained);
    }
}

fn select_descendant_generations(
    expected_root: ProcessGeneration,
    observed_root: ProcessGeneration,
    edges: &[GenerationEdge],
) -> Result<BTreeSet<ProcessGeneration>, &'static str> {
    if expected_root.pid == 0 || expected_root.birth == 0 || observed_root != expected_root {
        return Err("the original retained Host generation changed");
    }
    let mut listed = BTreeMap::new();
    for edge in edges {
        if edge.process.pid == expected_root.pid
            || edge.process.birth == 0
            || listed.insert(edge.process.pid, edge.process).is_some()
        {
            return Err("ambiguous native descendant generations");
        }
    }
    if edges
        .iter()
        .any(|edge| edge.parent != expected_root.pid && !listed.contains_key(&edge.parent))
    {
        return Err("native descendant parent is missing from the snapshot");
    }
    let mut admitted = BTreeMap::from([(expected_root.pid, expected_root)]);
    loop {
        let before = admitted.len();
        for edge in edges {
            if let Some(parent) = admitted.get(&edge.parent)
                && edge.process.birth >= parent.birth
                && edge.process.pid != expected_root.pid
            {
                admitted.insert(edge.process.pid, edge.process);
            }
        }
        if admitted.len() == before {
            admitted.remove(&expected_root.pid);
            return Ok(admitted.into_values().collect());
        }
    }
}

pub(crate) fn descendants(tree: &str) -> Vec<(String, u32, u32)> {
    tree.split_once(" descendants=")
        .expect("native census descendants")
        .1
        .split(',')
        .filter(|value| !value.is_empty())
        .map(|entry| {
            let (name, rest) = entry.rsplit_once(':').expect("descendant image and PID");
            let (pid, parent) = rest.split_once('<').expect("descendant parent PID");
            (
                name.to_owned(),
                pid.parse().expect("descendant PID"),
                parent.parse().expect("parent PID"),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
