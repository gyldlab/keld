//! Missing production sampler controls named by the approved GH481 proposal.
//! These exercise identity/history and capture ordering, not native SDK results.
use std::cell::Cell;

use super::super::{DesktopSampler, display::Geometry, unavailable};
use super::Target;

fn target(high: i32, low: u32, id: u32, path: &str) -> Target {
    Target {
        adapter_high: high,
        adapter_low: low,
        display_config_id: id,
        device_path: path.to_owned(),
    }
}

fn geometry() -> Geometry {
    Geometry {
        source_name: vec![68],
        bounds: [0, 0, 1001, 801],
        work_area: [1, 25, 999, 800],
        dpi: 120,
    }
}

fn qualified(sampler: &mut DesktopSampler) -> crate::AppDesktopFacts {
    sampler
        .capture_with(
            || Ok(geometry()),
            |_| Ok(target(-1, 2, 3, "full-device-path")),
            || Ok((1.0, false)),
        )
        .expect("qualified injected native observations")
}

#[test]
fn full_target_tuple_has_distinct_nonzero_ids_and_retains_the_original() {
    let mut sampler = DesktopSampler::default();
    for (expected, (high, low, id, path)) in [
        (-1, 2, 3, "complete-path-a"),
        (1, 2, 3, "complete-path-a"),
        (-1, 7, 3, "complete-path-a"),
        (-1, 2, 9, "complete-path-a"),
        (-1, 2, 3, "complete-path-b"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            sampler.display_id(target(high, low, id, path)).unwrap(),
            u32::try_from(expected + 1).unwrap()
        );
    }
    assert_eq!(
        sampler
            .display_id(target(-1, 2, 3, "complete-path-a"))
            .unwrap(),
        1,
        "temporary absence or a role replacement does not replace this history"
    );
    assert_eq!(sampler.known.len(), 5);
}

#[test]
fn thirty_third_identity_refuses_without_eviction_or_reuse() {
    let mut sampler = DesktopSampler::default();
    for id in 0..32 {
        assert_eq!(
            sampler
                .display_id(target(-1, 2, id, "same-complete-path"))
                .unwrap(),
            id + 1
        );
    }
    assert!(
        sampler
            .display_id(target(-1, 2, 32, "same-complete-path"))
            .is_err()
    );
    assert_eq!(sampler.known.len(), 32);
    for id in 0..32 {
        assert_eq!(
            sampler
                .display_id(target(-1, 2, id, "same-complete-path"))
                .unwrap(),
            id + 1,
            "capacity refusal must not evict or relabel any existing tuple"
        );
    }
}

#[test]
fn every_failed_native_read_precedes_identity_allocation() {
    for failed_step in 0..5 {
        let mut sampler = DesktopSampler::default();
        let geometry_reads = Cell::new(0);
        let target_reads = Cell::new(0);
        let result = sampler.capture_with(
            || {
                let read = geometry_reads.get();
                geometry_reads.set(read + 1);
                if (failed_step == 0 && read == 0) || (failed_step == 3 && read == 1) {
                    return Err(unavailable("injected geometry read failure"));
                }
                Ok(geometry())
            },
            |_| {
                let read = target_reads.get();
                target_reads.set(read + 1);
                if (failed_step == 1 && read == 0) || (failed_step == 4 && read == 1) {
                    return Err(unavailable("injected target read failure"));
                }
                Ok(target(-1, 2, 3, "full-device-path"))
            },
            || {
                if failed_step == 2 {
                    Err(unavailable("injected appearance failure"))
                } else {
                    Ok((1.0, false))
                }
            },
        );
        assert!(result.is_err(), "read {failed_step} must refuse");
        assert!(sampler.known.is_empty(), "failure cannot consume an ID");
        assert_eq!(qualified(&mut sampler).primary_id, 1);
        assert_eq!(sampler.known.len(), 1);
    }
}

#[test]
fn changed_geometry_or_complete_target_refuses_before_id_allocation() {
    for change_target in [false, true] {
        let mut sampler = DesktopSampler::default();
        let geometry_reads = Cell::new(0);
        let target_reads = Cell::new(0);
        let result = sampler.capture_with(
            || {
                let read = geometry_reads.get();
                geometry_reads.set(read + 1);
                let mut observed = geometry();
                if !change_target && read == 1 {
                    observed.dpi = 144;
                }
                Ok(observed)
            },
            |_| {
                let read = target_reads.get();
                target_reads.set(read + 1);
                Ok(target(
                    -1,
                    2,
                    3,
                    if change_target && read == 1 {
                        "changed-complete-path"
                    } else {
                        "full-device-path"
                    },
                ))
            },
            || Ok((1.0, false)),
        );
        assert!(result.is_err());
        assert!(sampler.known.is_empty());
        assert_eq!(qualified(&mut sampler).primary_id, 1);
    }
}

#[test]
fn qualified_capture_uses_approved_enclosing_edge_geometry() {
    let facts = qualified(&mut DesktopSampler::default());
    let display = &facts.displays[0];
    assert_eq!(facts.displays.len(), 1);
    assert_eq!(facts.primary_id, display.id);
    assert_eq!(display.bounds.x, 0);
    assert_eq!(display.bounds.y, 0);
    assert_eq!(display.bounds.width, 801);
    assert_eq!(display.bounds.height, 641);
    assert_eq!(display.work_area.x, 0);
    assert_eq!(display.work_area.y, 20);
    assert_eq!(display.work_area.width, 800);
    assert_eq!(display.work_area.height, 620);
    assert_eq!(display.scale_factor.to_bits(), 1.25_f64.to_bits());
    assert!(!facts.should_use_dark_colors);
}
