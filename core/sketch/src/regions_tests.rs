// SPDX-License-Identifier: MIT
//! Regions of many loops (mitcad#678): the nesting test of the loops of
//! disjoint parts tries only the faces whose bounds hold the probe.

use std::time::{Duration, Instant};

use super::EntityUid;
use super::geometry::{Curve2, P2};
use super::regions::{Region, RegionCurve, regions};

/// `side` × `side` disjoint circles of radius 2 on a 10 mm grid, the
/// curves c1, c2, ... row by row.
fn circle_grid(side: usize) -> Vec<RegionCurve> {
    (0..side * side)
        .map(|i| {
            let (x, y) = ((i % side) as f64 * 10.0, (i / side) as f64 * 10.0);
            RegionCurve::new(
                EntityUid(i as u32 + 1),
                Curve2::Circle {
                    center: [x, y],
                    radius: 2.0,
                    start: 0.0,
                },
            )
        })
        .collect()
}

/// A rectangle of four lines (curves `first`..`first + 3`) from `lo` to `hi`.
fn rectangle(first: u32, lo: [f64; 2], hi: [f64; 2]) -> Vec<RegionCurve> {
    let corners = [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]];
    (0..4)
        .map(|k| {
            RegionCurve::new(
                EntityUid(first + k as u32),
                Curve2::Line {
                    a: corners[k],
                    b: corners[(k + 1) % 4],
                },
            )
        })
        .collect()
}

fn keys(regions: &[Region]) -> Vec<String> {
    regions.iter().map(|r| r.profile.key.to_string()).collect()
}

fn sorted_circle_keys(n: usize) -> Vec<String> {
    let mut keys: Vec<String> = (1..=n).map(|i| format!("r{{c{i}}}")).collect();
    keys.sort();
    keys
}

#[test]
fn four_hundred_disjoint_circles_are_four_hundred_regions() {
    let found = regions(&circle_grid(20));
    assert_eq!(found.len(), 400);
    let mut names = keys(&found);
    names.sort();
    assert_eq!(names, sorted_circle_keys(400));
    for region in &found {
        assert_eq!(region.profile.loops.len(), 1, "{}", region.profile.key);
        assert!(region.holes.is_empty(), "{}", region.profile.key);
        assert!((region.area - 4.0 * std::f64::consts::PI).abs() < 1e-6);
    }
}

#[test]
fn circles_inside_a_rectangle_are_its_holes() {
    let mut curves = circle_grid(20);
    curves.extend(rectangle(401, [-5.0, -5.0], [195.0, 195.0]));
    // A circle around one circle of the grid: that circle is its hole,
    // not the rectangle's.
    curves.push(RegionCurve::new(
        EntityUid(405),
        Curve2::Circle {
            center: [50.0, 50.0],
            radius: 4.0,
            start: 0.0,
        },
    ));
    let found = regions(&curves);
    assert_eq!(found.len(), 402);
    let outer = found
        .iter()
        .find(|r| r.profile.loops[0].segments.len() == 4)
        .expect("the rectangle's region");
    assert_eq!(outer.profile.loops.len(), 1 + 399 + 1);
    assert!(
        (outer.area
            - (200.0 * 200.0 - 399.0 * 4.0 * std::f64::consts::PI - 16.0 * std::f64::consts::PI))
            .abs()
            < 1e-6
    );
    let ring = found
        .iter()
        .find(|r| r.profile.key.to_string() == "r{c405}")
        .expect("the ring");
    assert_eq!(ring.profile.loops.len(), 2);
    assert_eq!(ring.profile.loops[1].segments[0].key.to_string(), "c106");
}

/// The time of the regions of `side` × `side` circles, the median of
/// `runs`.
fn median_time(side: usize, runs: usize) -> Duration {
    let curves = circle_grid(side);
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            let found = regions(&curves);
            let time = start.elapsed();
            assert_eq!(found.len(), side * side);
            time
        })
        .collect();
    times.sort();
    times[runs / 2]
}

/// A guard against the nesting test growing with the square of the loops
/// again (mitcad#678): 400 circles take about 5 ms in a release build
/// (800 ms before), the limits leave room for a loaded machine.
#[test]
fn the_regions_of_many_circles_are_fast() {
    let limit = if cfg!(debug_assertions) {
        Duration::from_millis(2000)
    } else {
        Duration::from_millis(100)
    };
    let time = median_time(20, 3);
    assert!(time < limit, "400 circles took {time:?}");
}

/// Prints the times of 25, 100 and 400 circles:
/// `cargo test --release -p mitcad-model regions_timings -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement"]
fn regions_timings() {
    for side in [5, 10, 20] {
        let time = median_time(side, 3);
        println!(
            "regions of {} circles: {:.3} ms",
            side * side,
            time.as_secs_f64() * 1000.0
        );
    }
}

/// `count` disjoint 4 × 4 rectangles in a row, each of four lines, the
/// curves from c`first` on.
fn rectangle_row(first: u32, count: u32) -> Vec<RegionCurve> {
    (0..count)
        .flat_map(|k| {
            let x = 10.0 * f64::from(k);
            rectangle(first + 4 * k, [x, 0.0], [x + 4.0, 4.0])
        })
        .collect()
}

/// The region keys and areas, sorted by key.
fn keys_and_areas(regions: &[Region]) -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = regions
        .iter()
        .map(|r| (r.profile.key.to_string(), (r.area * 1e6).round() as i64))
        .collect();
    out.sort();
    out
}

/// A line drawn twice among many rectangles (mitcad#889: the repeats are
/// found among the pieces with the same ends): the lower curve id stays,
/// as when it is the only one.
#[test]
fn a_line_drawn_twice_among_many_rectangles_keeps_the_lower_id() {
    let plain = rectangle_row(10, 60);
    // Rectangle 7's bottom line again with a higher id: it is left out.
    let mut higher = plain.clone();
    let bottom = higher[4 * 7].curve.clone();
    higher.push(RegionCurve::new(EntityUid(1000), bottom));
    assert_eq!(
        keys_and_areas(&regions(&higher)),
        keys_and_areas(&regions(&plain))
    );
    // Rectangle 30's top line again with a lower id (the other way round):
    // it stays and the higher one is left out.
    let mut lower = plain.clone();
    let top = lower[4 * 30 + 2].curve.clone();
    let reversed = match top {
        Curve2::Line { a, b } => Curve2::Line { a: b, b: a },
        other => other,
    };
    lower.push(RegionCurve::new(EntityUid(1), reversed.clone()));
    let mut expected = plain.clone();
    expected[4 * 30 + 2] = RegionCurve::new(EntityUid(1), reversed);
    let found = regions(&lower);
    assert_eq!(found.len(), 60);
    assert_eq!(keys_and_areas(&found), keys_and_areas(&regions(&expected)));
}

/// A line that ends inside many rectangles' region cuts a side in two
/// pieces; the line dangles and is dropped, and the pieces of the side are
/// one again (mitcad#889: each vertex looks only at its own edges).
#[test]
fn a_dangling_line_leaves_the_side_it_cut_whole() {
    let plain = rectangle_row(10, 60);
    for k in [0_u32, 41, 59] {
        let x = 10.0 * f64::from(k);
        let mut cut = plain.clone();
        // From the middle of the bottom side up into the rectangle.
        cut.push(RegionCurve::new(
            EntityUid(1000),
            Curve2::Line {
                a: [x + 2.0, 0.0],
                b: [x + 2.0, 1.5],
            },
        ));
        let found = regions(&cut);
        assert_eq!(found.len(), 60);
        assert_eq!(keys_and_areas(&found), keys_and_areas(&regions(&plain)));
    }
}

/// A ring between two circles cut by `count` pairs of radial lines turned
/// about `pivot` (the inner circle's centre), the lines from the inner
/// circle to the outer one, which is centred at the origin: the lines'
/// outer ends lie on the outer circle when `pivot` is its centre, else up
/// to about twice the pivot's offset off it (mitcad#2557).
fn patterned_ring(pivot: P2, count: u32) -> Vec<RegionCurve> {
    let (inner, outer) = (15.75, 17.45);
    let mut curves = vec![
        RegionCurve::new(
            EntityUid(1),
            Curve2::Circle {
                center: [0.0, 0.0],
                radius: outer,
                start: 0.0,
            },
        ),
        RegionCurve::new(
            EntityUid(2),
            Curve2::Circle {
                center: pivot,
                radius: inner,
                start: 0.0,
            },
        ),
    ];
    // The originals about the y axis, their ends on both circles.
    let originals: Vec<(P2, P2)> = [-0.6_f64, 0.6]
        .iter()
        .map(|&x| {
            let end = [x, (outer * outer - x * x).sqrt()];
            let dir = [end[0] - pivot[0], end[1] - pivot[1]];
            let len = dir[0].hypot(dir[1]);
            let start = [
                pivot[0] + dir[0] * inner / len,
                pivot[1] + dir[1] * inner / len,
            ];
            (start, end)
        })
        .collect();
    let turn = |p: P2, a: f64| {
        let (s, c) = a.sin_cos();
        let d = [p[0] - pivot[0], p[1] - pivot[1]];
        [
            pivot[0] + d[0] * c - d[1] * s,
            pivot[1] + d[0] * s + d[1] * c,
        ]
    };
    let mut id = 3;
    for k in 0..count {
        let a = std::f64::consts::TAU * f64::from(k) / f64::from(count);
        for (start, end) in &originals {
            curves.push(RegionCurve::new(
                EntityUid(id),
                Curve2::Line {
                    a: turn(*start, a),
                    b: turn(*end, a),
                },
            ));
            id += 1;
        }
    }
    curves
}

/// The lines' ends that miss the outer circle, and by how much at most.
fn misses(curves: &[RegionCurve]) -> (usize, f64) {
    let mut worst: f64 = 0.0;
    let mut short = 0;
    for c in curves {
        if let Curve2::Line { b, .. } = c.curve {
            let gap = 17.45 - b[0].hypot(b[1]);
            if gap > 1e-6 {
                short += 1;
            }
            worst = worst.max(gap);
        }
    }
    (short, worst)
}

/// An imported sketch's pattern about a centre a nanometre off the circle
/// its lines end on (mitcad#2557, a ring gear's cut: the inner circle
/// concentric with an edge of the body 1.06e-6 mm off the outer circle's
/// centre): the copies' outer ends fall up to 1.1e-6 mm short of the outer
/// circle, within the designs' resolution, and still cut the ring: 32
/// pieces and the disc inside, as with the centre exact.
#[test]
fn pattern_lines_a_nanometre_short_of_a_circle_still_cut_the_ring() {
    let exact = regions(&patterned_ring([0.0, 0.0], 16));
    assert_eq!(exact.len(), 33, "{:?}", keys(&exact));
    let curves = patterned_ring([-1.0625e-6, 0.0], 16);
    let (short, worst) = misses(&curves);
    // As in the design: four ends more than 1e-6 mm short, 1.1e-6 at most.
    assert!(
        short == 4 && worst < 1.2e-6,
        "{short} ends short, {worst} mm"
    );
    let found = regions(&curves);
    assert_eq!(found.len(), 33, "{:?}", keys(&found));
    let mut areas: Vec<f64> = found.iter().map(|r| r.area).collect();
    let mut expected: Vec<f64> = exact.iter().map(|r| r.area).collect();
    areas.sort_by(f64::total_cmp);
    expected.sort_by(f64::total_cmp);
    for (a, e) in areas.iter().zip(&expected) {
        assert!((a - e).abs() < 1e-3, "{areas:?} against {expected:?}");
    }
}

/// Ends farther off than the resolution still leave a line dangling: a
/// line 2e-5 mm short of the outer circle does not cut the ring.
#[test]
fn a_line_short_of_a_circle_by_more_than_the_resolution_dangles() {
    let mut curves = patterned_ring([0.0, 0.0], 2);
    // The ring cut twice by each pair: four pieces and the disc.
    assert_eq!(regions(&curves).len(), 5);
    for c in curves.iter_mut().skip(2) {
        if let Curve2::Line { b, .. } = &mut c.curve {
            let r = b[0].hypot(b[1]);
            let shorter = (r - 2.0 * super::regions::RESOLUTION) / r;
            *b = [b[0] * shorter, b[1] * shorter];
        }
    }
    let found = regions(&curves);
    assert_eq!(found.len(), 2, "{:?}", keys(&found));
}

/// The end points of a line's or an arc's profile geometry; none for a
/// closed curve.
fn segment_ends(geometry: &crate::profile::SegmentGeometry) -> Option<(P2, P2)> {
    use crate::profile::SegmentGeometry;
    match *geometry {
        SegmentGeometry::Line { start, end } => Some((start, end)),
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let at = |a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
            Some((at(start_angle), at(end_angle)))
        }
        _ => None,
    }
}

/// The largest gap between consecutive segments of the region's loops,
/// each segment taken in whichever direction continues its loop (as the
/// profile face's loop check does, `geometry/src/profile.cpp`).
fn largest_loop_gap(region: &Region) -> f64 {
    let dist = |a: P2, b: P2| (a[0] - b[0]).hypot(a[1] - b[1]);
    let mut worst: f64 = 0.0;
    for lp in &region.profile.loops {
        let ends: Vec<(P2, P2)> = lp
            .segments
            .iter()
            .filter_map(|s| segment_ends(&s.geometry))
            .collect();
        if ends.len() < 2 {
            continue;
        }
        let walk = |reversed: bool| {
            let start = if reversed { ends[0].1 } else { ends[0].0 };
            let mut current = if reversed { ends[0].0 } else { ends[0].1 };
            let mut gap: f64 = 0.0;
            for &(a, b) in &ends[1..] {
                let (forward, backward) = (dist(current, a), dist(current, b));
                gap = gap.max(forward.min(backward));
                current = if forward <= backward { b } else { a };
            }
            gap.max(dist(current, start))
        };
        worst = worst.max(walk(false).min(walk(true)));
    }
    worst
}

/// The pieces of the ring patterned about a centre off its circle meet
/// within the designs' resolution, the gap the profile faces heal
/// (mitcad#2567): the lines' ends meet the circle there, and no points
/// farther apart are taken as one.
#[test]
fn pattern_lines_short_of_a_circle_leave_no_gap_over_the_resolution() {
    let found = regions(&patterned_ring([-1.0625e-6, 0.0], 16));
    assert_eq!(found.len(), 33, "{:?}", keys(&found));
    for region in &found {
        let gap = largest_loop_gap(region);
        assert!(
            gap <= super::regions::RESOLUTION,
            "{}: a gap of {gap} mm",
            region.profile.key
        );
    }
}

/// A rectangle whose top line ends `gap` mm short of the right one's end
/// (its corner at (10, 10)).
fn rectangle_open_at_a_corner(gap: f64) -> Vec<RegionCurve> {
    let mut curves = rectangle(1, [0.0, 0.0], [10.0, 10.0]);
    // The top line runs from (10, 10) to (0, 10).
    curves[2].curve = Curve2::Line {
        a: [10.0 - gap, 10.0],
        b: [0.0, 10.0],
    };
    curves
}

/// Points over the resolution apart stay apart (mitcad#2567: with the
/// arrangement's tolerance at the resolution, points up to ten times
/// farther apart were one vertex, and a profile loop of the pieces had a
/// gap the profile face does not heal, "it does not connect to the
/// previous segment of its loop"). Within the resolution the corner closes.
#[test]
fn a_corner_open_by_more_than_the_resolution_bounds_no_region() {
    let closed = regions(&rectangle_open_at_a_corner(5e-6));
    assert_eq!(closed.len(), 1, "{:?}", keys(&closed));
    assert!(largest_loop_gap(&closed[0]) <= super::regions::RESOLUTION);
    for gap in [2e-5, 5e-5, 9e-5] {
        let open = regions(&rectangle_open_at_a_corner(gap));
        assert!(open.is_empty(), "a gap of {gap} mm: {:?}", keys(&open));
    }
}
