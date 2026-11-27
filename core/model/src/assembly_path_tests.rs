// SPDX-License-Identifier: MIT
//! The search of `assembly_path.rs` (mitcad#1737) on paths whose
//! clearance is known exactly: a point moving along a line past solid
//! intervals (a slide: speed 1), and an angle (a turn: speed the body's
//! reach). The exact shapes on the OCCT kernel are
//! `tools/cli/tests/assembly_path.json`.

use std::f64::consts::FRAC_PI_2;
use std::time::Duration;

use crate::assembly_path::{Limit, MOST_HALVINGS, Motion, PathSettings, Probe, search};

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
        Ok(Probe::new(clearance, overlap))
    }
}

fn slide(length: f64, steps: usize) -> PathSettings {
    PathSettings {
        length,
        steps,
        tolerance: 0.001,
        speed: 1.0,
        most_probes: None,
        time_limit: None,
    }
}

#[test]
fn a_clear_path_reports_its_clearance_and_is_certified() {
    // A peg 0.1 mm from its hole's wall all the way.
    let settings = slide(10.0, 10);
    let found = search(&settings, |_| Ok::<_, String>(Probe::new(0.1, false))).unwrap();
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
        most_probes: None,
        time_limit: None,
    };
    let found = search(&settings, |angle| {
        Ok::<_, String>(Probe::new(
            (reach * (contact - angle)).max(0.0),
            angle > contact,
        ))
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
    assert!(
        PathSettings {
            most_probes: Some(0),
            ..slide(10.0, 10)
        }
        .check()
        .is_err()
    );
    assert!(slide(10.0, 10).check().is_ok());
}

/// A bracket flush against its neighbours (mitcad#2577) slid along them:
/// it touches them all the way (clearance 0, no overlap), and the nearest
/// other body is `apart` mm away. As contacts (`contacts`) or, as the
/// search measured them before, among the others.
fn flush(apart: f64, contacts: bool) -> impl FnMut(f64) -> Result<Probe, String> {
    move |_| {
        Ok(if contacts {
            Probe {
                clearance: apart,
                overlap: false,
                contact: 0.0,
            }
        } else {
            Probe::new(0.0, false)
        })
    }
}

#[test]
fn a_body_flush_against_its_contacts_is_not_halved_and_answers_as_the_full_search() {
    // 20 steps of 1 mm along the faces it touches, the next body 5 mm off.
    let settings = slide(20.0, 20);
    let full = search(&settings, flush(5.0, false)).unwrap();
    let found = search(&settings, flush(5.0, true)).unwrap();
    // The full search halves every step to its limit: 8 positions a step.
    assert_eq!(full.probes, 20 * (1 << MOST_HALVINGS) + 1);
    // As contacts: each step once, nothing else.
    assert_eq!(found.probes, 21);
    // The same answer: no hit, not certified (no clearance rules a span out
    // where the body touches), the minimum 0 at the start.
    assert!(full.hit.is_none() && found.hit.is_none());
    assert!(!full.certified && !found.certified);
    assert_eq!(found.min_clearance, full.min_clearance);
    let min = found.min_clearance.unwrap();
    assert_eq!((min.at, min.clearance), (0.0, 0.0));
    assert!(!found.touches_at_end && found.stopped.is_none());
    assert_eq!(found.steps.len(), 21);
    assert!(found.steps.iter().all(|s| s.clearance == 0.0));
}

#[test]
fn a_contact_the_body_runs_into_is_hit_at_its_face() {
    // Flush along a neighbour that has a step at 5.5 mm: overlapping past
    // it. The others are 5 mm off.
    let settings = slide(10.0, 10);
    let found = search(&settings, |t| {
        Ok::<_, String>(Probe {
            clearance: 5.0,
            overlap: t > 5.5,
            contact: 0.0,
        })
    })
    .unwrap();
    let hit = found.hit.unwrap();
    assert!(hit.clear <= 5.5 && hit.clear >= 5.5 - 0.001, "{hit:?}");
    assert_eq!(hit.step, 6);
    assert!(!found.certified);
}

#[test]
fn a_contact_the_body_leaves_is_ruled_out_by_its_clearance() {
    // A lug resting on a rim lifted off it: the contact's clearance grows
    // with the motion, which rules every span out.
    let settings = slide(5.0, 5);
    let found = search(&settings, |t| {
        Ok::<_, String>(Probe {
            clearance: f64::INFINITY,
            overlap: false,
            contact: t,
        })
    })
    .unwrap();
    assert!(found.hit.is_none());
    assert!(found.certified);
    assert_eq!(found.probes, 6);
    assert_eq!(found.min_clearance.unwrap().clearance, 0.0);
}

#[test]
fn a_contact_does_not_stop_the_halving_for_the_others() {
    // Flush along its contacts, past a wall of another body 0.02 mm thick
    // at 3.30 mm: the others' clearances still lead to it.
    let walls = [(3.30, 3.32)];
    let mut others = against(&walls);
    let found = search(&slide(10.0, 40), |t| {
        let other = others(t)?;
        Ok::<_, String>(Probe {
            contact: 0.0,
            ..other
        })
    })
    .unwrap();
    let hit = found.hit.unwrap();
    assert!(hit.clear <= 3.3 && hit.clear >= 3.3 - 0.001, "{hit:?}");
}

#[test]
fn the_probe_limit_stops_the_search_with_a_partial_answer() {
    // 10 mm from everything: one position a step. Five positions: the
    // start and steps 1 to 4.
    let settings = PathSettings {
        most_probes: Some(5),
        ..slide(10.0, 10)
    };
    let found = search(&settings, |_| Ok::<_, String>(Probe::new(10.0, false))).unwrap();
    let stopped = found.stopped.unwrap();
    assert_eq!(stopped.limit, Limit::Probes);
    assert_eq!(stopped.checked_to, 4.0);
    assert_eq!(found.probes, 5);
    assert!(!found.certified);
    assert!(found.hit.is_none() && !found.touches_at_end);
    assert_eq!(found.steps.len(), 5);
    let min = found.min_clearance.unwrap();
    assert_eq!((min.at, min.clearance), (0.0, 10.0));
    // Without the limit the same path is certified.
    let whole = search(&slide(10.0, 10), |_| {
        Ok::<_, String>(Probe::new(10.0, false))
    })
    .unwrap();
    assert!(whole.certified && whole.stopped.is_none());
}

#[test]
fn the_time_limit_stops_the_search_after_the_start() {
    let settings = PathSettings {
        time_limit: Some(Duration::ZERO),
        ..slide(10.0, 10)
    };
    let found = search(&settings, |_| Ok::<_, String>(Probe::new(10.0, false))).unwrap();
    let stopped = found.stopped.unwrap();
    assert_eq!(stopped.limit, Limit::Time);
    assert_eq!(stopped.checked_to, 0.0);
    assert_eq!(found.probes, 1);
    assert_eq!(found.steps.len(), 1);
    assert!(!found.certified);
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
