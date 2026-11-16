// SPDX-License-Identifier: MIT
//! The search of `assembly_path.rs` (mitcad#1737) on paths whose
//! clearance is known exactly: a point moving along a line past solid
//! intervals (a slide: speed 1), and an angle (a turn: speed the body's
//! reach). The exact shapes on the OCCT kernel are
//! `tools/cli/tests/assembly_path.json`.

use std::f64::consts::FRAC_PI_2;

use crate::assembly_path::{MOST_HALVINGS, Motion, PathSettings, Probe, search};

/// A point at `t` against solid intervals: its distance to the nearest,
/// and whether it lies strictly inside one.
fn against(walls: &[(f64, f64)]) -> impl FnMut(f64) -> Result<Probe, String> + '_ {
    move |t| {
        let mut clearance = f64::INFINITY;
        let mut overlap = false;
        for &(low, high) in walls {
            if t > low && t < high {
                overlap = true;
            }
            let distance = if t < low {
                low - t
            } else if t > high {
                t - high
            } else {
                0.0
            };
            clearance = clearance.min(distance);
        }
        Ok(Probe { clearance, overlap })
    }
}

fn slide(length: f64, steps: usize) -> PathSettings {
    PathSettings {
        length,
        steps,
        tolerance: 0.001,
        speed: 1.0,
    }
}

#[test]
fn a_clear_path_reports_its_clearance_and_is_certified() {
    // A peg 0.1 mm from its hole's wall all the way.
    let settings = slide(10.0, 10);
    let found = search(&settings, |_| {
        Ok::<_, String>(Probe {
            clearance: 0.1,
            overlap: false,
        })
    })
    .unwrap();
    assert!(found.hit.is_none());
    assert!(!found.touches_at_end);
    assert!(found.certified);
    assert_eq!(found.steps.len(), 11);
    assert!(
        found
            .steps
            .iter()
            .all(|s| (s.clearance - 0.1).abs() < 1e-12)
    );
    let min = found.min_clearance.unwrap();
    assert!((min.clearance - 0.1).abs() < 1e-12);
    // Each step is measured at 2^MOST_HALVINGS + 1 positions at most, then
    // the minimum is refined.
    assert!(
        found.probes <= 10 * (1 << MOST_HALVINGS) + 1 + 40,
        "{}",
        found.probes
    );
}

#[test]
fn a_wall_ahead_is_hit_at_its_face_within_the_tolerance() {
    // The point reaches a wall at 5 mm.
    let walls = [(5.0, 6.0)];
    let settings = slide(10.0, 10);
    let found = search(&settings, against(&walls)).unwrap();
    let hit = found.hit.unwrap();
    assert!(!hit.at_start);
    assert!(hit.clear <= 5.0 && hit.clear >= 5.0 - 0.001, "{hit:?}");
    assert!(hit.contact > 5.0 && hit.contact <= 5.0 + 0.001, "{hit:?}");
    // The contact is deeper than the tolerance first past step 5.
    assert_eq!(hit.step, 6);
    assert_eq!(found.min_clearance.unwrap().clearance, 0.0);
    // The steps before the hit, from the start.
    assert_eq!(found.steps.len(), 6);
    assert!((found.steps[3].clearance - 2.0).abs() < 1e-12);
}

#[test]
fn a_contact_at_the_start_is_hit_at_the_first_step() {
    // A lug lying on the rim: touching at the start, overlapping as soon
    // as it moves.
    let walls = [(0.0, 3.0)];
    let found = search(&slide(10.0, 10), against(&walls)).unwrap();
    let hit = found.hit.unwrap();
    assert_eq!(hit.clear, 0.0);
    assert_eq!(hit.step, 1);
    assert!(!hit.at_start);
}

#[test]
fn an_overlap_before_moving_is_a_hit_at_step_zero() {
    let walls = [(-1.0, 1.0)];
    let found = search(&slide(10.0, 10), against(&walls)).unwrap();
    let hit = found.hit.unwrap();
    assert!(hit.at_start);
    assert_eq!(hit.step, 0);
    assert!(found.min_clearance.is_none());
}

#[test]
fn a_wall_thinner_than_the_halved_step_is_not_certified_until_the_steps_are_finer() {
    // 0.02 mm thick, between the positions of 1 mm steps halved three
    // times: not found, and the answer says so.
    let walls = [(3.30, 3.32)];
    let coarse = search(&slide(10.0, 10), against(&walls)).unwrap();
    assert!(coarse.hit.is_none());
    assert!(!coarse.certified);
    // At 0.25 mm steps the clearances lead to it.
    let fine = search(&slide(10.0, 40), against(&walls)).unwrap();
    let hit = fine.hit.unwrap();
    assert!(hit.clear <= 3.3 && hit.clear >= 3.3 - 0.001, "{hit:?}");
}

#[test]
fn a_graze_shallower_than_the_tolerance_is_passed() {
    // 0.0005 mm of material across step 5: the body is clear again before
    // it has gone the tolerance further.
    let walls = [(4.9998, 5.0003)];
    let found = search(&slide(10.0, 10), against(&walls)).unwrap();
    assert!(found.hit.is_none());
    assert!(!found.touches_at_end);
    assert_eq!(found.steps.len(), 11);
    // Step 5 lies inside the graze.
    assert_eq!(found.steps[5].clearance, 0.0);
    assert!((found.steps[7].clearance - (7.0 - 5.0003)).abs() < 1e-9);
}

#[test]
fn a_contact_within_the_tolerance_of_the_end_is_the_seat() {
    // The shoulder at 9.9995 mm: the body reaches it as its path ends.
    let walls = [(9.9995, 12.0)];
    let found = search(&slide(10.0, 10), against(&walls)).unwrap();
    assert!(found.hit.is_none());
    assert!(found.touches_at_end);
    let last = found.steps.last().unwrap();
    assert_eq!(last.at, 10.0);
    assert_eq!(last.clearance, 0.0);
    assert_eq!(found.min_clearance.unwrap().clearance, 0.0);
}

#[test]
fn a_turn_finds_the_angle_to_the_resolution_of_its_reach() {
    // A key 8 mm from the axis meets the slot's wall at 0.0286 rad.
    let contact = 0.0286;
    let reach = 8.0;
    let settings = PathSettings {
        length: 0.1,
        steps: 10,
        tolerance: 0.001,
        speed: reach,
    };
    let found = search(&settings, |angle| {
        Ok::<_, String>(Probe {
            clearance: (reach * (contact - angle)).max(0.0),
            overlap: angle > contact,
        })
    })
    .unwrap();
    let hit = found.hit.unwrap();
    let resolution = 0.001 / reach;
    assert!(
        hit.clear <= contact && hit.clear >= contact - resolution,
        "{hit:?}"
    );
    assert!(hit.contact - hit.clear <= resolution * (1.0 + 1e-9));
    assert_eq!(hit.step, 3);
    assert!(found.certified);
}

#[test]
fn settings_out_of_range_are_refused() {
    assert!(slide(0.0, 10).check().is_err());
    assert!(slide(10.0, 0).check().is_err());
    assert!(
        PathSettings {
            tolerance: 0.0,
            ..slide(10.0, 10)
        }
        .check()
        .is_err()
    );
    assert!(
        PathSettings {
            speed: 0.0,
            ..slide(10.0, 10)
        }
        .check()
        .is_err()
    );
    assert!(slide(10.0, 10).check().is_ok());
}

#[test]
fn a_slide_moves_along_its_direction_and_a_turn_reaches_its_farthest_corner() {
    // Against the direction for a negative distance, along a unit vector.
    let slide = Motion::slide([0.0, 0.0, 2.0], -10.0).unwrap();
    assert_eq!(slide.length(), 10.0);
    let moved = slide.at(4.0).apply_point([1.0, 2.0, 3.0]);
    assert!((moved[2] - (3.0 - 4.0)).abs() < 1e-12, "{moved:?}");
    assert_eq!(slide.speed([[100.0, 0.0, 0.0]]), Some(1.0));
    // A quarter turn about Z through (1, 0, 0), negative: clockwise.
    let turn = Motion::turn([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], -FRAC_PI_2).unwrap();
    assert_eq!(turn.length(), FRAC_PI_2);
    let turned = turn.at(FRAC_PI_2).apply_point([2.0, 0.0, 5.0]);
    assert!((turned[0] - 1.0).abs() < 1e-12 && (turned[1] + 1.0).abs() < 1e-12);
    assert!((turned[2] - 5.0).abs() < 1e-12);
    // The farthest corner from the axis, not from its origin.
    let reach = turn.speed([[1.0, 0.0, 50.0], [4.0, 4.0, -3.0]]).unwrap();
    assert!((reach - 5.0).abs() < 1e-12);
    assert_eq!(turn.speed([]), None);
    assert!(Motion::slide([0.0; 3], 1.0).is_err());
    assert!(Motion::slide([1.0, 0.0, 0.0], 0.0).is_err());
    assert!(Motion::turn([0.0; 3], [0.0, 0.0, 1.0], 0.0).is_err());
}
