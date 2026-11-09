// SPDX-License-Identifier: MIT
//! The closed loop a sketch curve bounds (mitcad#1541): what a double-click
//! on a curve selects. The curves of the picked one's kind (construction or
//! not, never both) are arranged as profile regions are (`regions.rs`), and
//! of the regions whose outer loop runs along the picked curve the one with
//! the smallest outline gives the loop: its curves, through points where
//! more than two curves meet and through crossings. A curve that bounds no
//! region (a dangling or bridging curve) has no loop; the app then takes
//! the open chain through points where exactly two curves meet.

use crate::def::SketchDef;
use crate::geometry::polygon_area;
use crate::ids::EntityUid;
use crate::regions::{RegionCurve, regions};
use crate::solve::Solved;
use crate::topo::CurveId;

/// The curves of the smallest region's outer loop that runs along
/// `picked`, each once, in the loop's order; None when no region's outer
/// loop does.
pub fn smallest_loop(curves: &[RegionCurve], picked: EntityUid) -> Option<Vec<EntityUid>> {
    let target = CurveId::Curve(picked);
    let mut best: Option<(f64, Vec<EntityUid>)> = None;
    for region in regions(curves) {
        let Some(outer) = region.profile.loops.first() else {
            continue;
        };
        if !outer.segments.iter().any(|s| s.key.curve == target) {
            continue;
        }
        // The area inside the outer loop, holes included: the loop's size,
        // not the region's.
        let area = polygon_area(&region.outline).abs();
        if best.as_ref().is_some_and(|(smallest, _)| *smallest <= area) {
            continue;
        }
        let mut ids: Vec<EntityUid> = Vec::new();
        for segment in &outer.segments {
            if let CurveId::Curve(id) = segment.key.curve
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        best = Some((area, ids));
    }
    best.map(|(_, ids)| ids)
}

/// [`smallest_loop`] of a sketch's solved curves: those of the picked
/// curve's kind (construction or not), without the sketch's axes and
/// texts. None for a point, an axis or an unknown id.
pub fn loop_of<P>(
    def: &SketchDef<P>,
    solved: &Solved,
    picked: EntityUid,
) -> Option<Vec<EntityUid>> {
    let entity = def
        .entities
        .iter()
        .find(|e| e.id == picked && !e.is_point() && e.axis.is_none())?;
    let construction = entity.construction;
    let curves: Vec<RegionCurve> = def
        .entities
        .iter()
        .filter(|e| !e.is_point() && e.axis.is_none() && e.construction == construction)
        .filter_map(|e| Some(RegionCurve::new(e.id, solved.curves.get(&e.id)?.clone())))
        .collect();
    smallest_loop(&curves, picked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Curve2;

    fn line(id: u32, a: [f64; 2], b: [f64; 2]) -> RegionCurve {
        RegionCurve::new(EntityUid(id), Curve2::Line { a, b })
    }

    fn ids(list: &[u32]) -> Vec<EntityUid> {
        list.iter().copied().map(EntityUid).collect()
    }

    fn sorted(found: Option<Vec<EntityUid>>) -> Option<Vec<EntityUid>> {
        found.map(|mut v| {
            v.sort();
            v
        })
    }

    /// A 20 x 10 rectangle (c1..c4) with c5 leaving its corner (0, 0).
    fn rectangle_with_tail() -> Vec<RegionCurve> {
        vec![
            line(1, [0.0, 0.0], [20.0, 0.0]),
            line(2, [20.0, 0.0], [20.0, 10.0]),
            line(3, [20.0, 10.0], [0.0, 10.0]),
            line(4, [0.0, 10.0], [0.0, 0.0]),
            line(5, [0.0, 0.0], [-10.0, -10.0]),
        ]
    }

    #[test]
    fn a_side_of_a_rectangle_gives_its_four_lines_past_a_branch() {
        let curves = rectangle_with_tail();
        for side in 1..=4 {
            assert_eq!(
                sorted(smallest_loop(&curves, EntityUid(side))),
                Some(ids(&[1, 2, 3, 4]))
            );
        }
    }

    #[test]
    fn a_dangling_line_has_no_loop() {
        assert_eq!(smallest_loop(&rectangle_with_tail(), EntityUid(5)), None);
    }

    #[test]
    fn an_open_polyline_has_no_loop() {
        let curves = vec![
            line(1, [0.0, 0.0], [10.0, 0.0]),
            line(2, [10.0, 0.0], [10.0, 10.0]),
            line(3, [10.0, 10.0], [20.0, 10.0]),
        ];
        assert_eq!(smallest_loop(&curves, EntityUid(2)), None);
    }

    #[test]
    fn a_shared_side_takes_the_smaller_loop() {
        // Two rectangles side by side sharing c4 (x = 10): the left one
        // 10 x 10 (c1, c2, c3, c4), the right one 30 x 10 (c5, c6, c7, c4).
        let curves = vec![
            line(1, [0.0, 0.0], [10.0, 0.0]),
            line(2, [10.0, 10.0], [0.0, 10.0]),
            line(3, [0.0, 10.0], [0.0, 0.0]),
            line(4, [10.0, 0.0], [10.0, 10.0]),
            line(5, [10.0, 0.0], [40.0, 0.0]),
            line(6, [40.0, 0.0], [40.0, 10.0]),
            line(7, [40.0, 10.0], [10.0, 10.0]),
        ];
        assert_eq!(
            sorted(smallest_loop(&curves, EntityUid(4))),
            Some(ids(&[1, 2, 3, 4]))
        );
        assert_eq!(
            sorted(smallest_loop(&curves, EntityUid(6))),
            Some(ids(&[4, 5, 6, 7]))
        );
    }

    #[test]
    fn a_loop_with_a_hole_is_measured_by_its_outline() {
        // A 30 x 10 rectangle with a large hole (c9..c12) beside a 10 x 10
        // one, sharing c4: the smaller outline wins although the holed
        // region's area is smaller.
        let curves = vec![
            line(1, [0.0, 0.0], [10.0, 0.0]),
            line(2, [10.0, 10.0], [0.0, 10.0]),
            line(3, [0.0, 10.0], [0.0, 0.0]),
            line(4, [10.0, 0.0], [10.0, 10.0]),
            line(5, [10.0, 0.0], [40.0, 0.0]),
            line(6, [40.0, 0.0], [40.0, 10.0]),
            line(7, [40.0, 10.0], [10.0, 10.0]),
            line(9, [11.0, 1.0], [39.0, 1.0]),
            line(10, [39.0, 1.0], [39.0, 9.0]),
            line(11, [39.0, 9.0], [11.0, 9.0]),
            line(12, [11.0, 9.0], [11.0, 1.0]),
        ];
        assert_eq!(
            sorted(smallest_loop(&curves, EntityUid(4))),
            Some(ids(&[1, 2, 3, 4]))
        );
    }

    #[test]
    fn a_lone_circle_is_its_own_loop() {
        let curves = vec![RegionCurve::new(
            EntityUid(1),
            Curve2::Circle {
                center: [0.0, 0.0],
                radius: 5.0,
                start: 0.0,
            },
        )];
        assert_eq!(smallest_loop(&curves, EntityUid(1)), Some(ids(&[1])));
    }
}
