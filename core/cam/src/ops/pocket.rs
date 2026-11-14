// SPDX-License-Identifier: MIT
//! Offset pocketing: a region (an outline with islands) is cleared down
//! to its floor by loops of the tool center at equal offsets from the
//! walls, from the middle outwards, so that every loop but the first cuts
//! at most the stepover. The last loop is the wall's (the tool radius
//! plus the stock to leave from it).
//!
//! Coverage: two loops `stepover` apart leave nothing between them while
//! `stepover` is at most the tool's diameter; at a corner of angle `a`
//! the loops are `stepover / sin(a / 2)` apart along its bisector, so the
//! default stepover (40 % of the diameter) clears corners down to about
//! 47 degrees. When the next loop at a full stepover vanishes, loops at
//! half the radius follow until nothing is left farther than that from
//! the last one: the middle is cleared too.

use cavalier_contours::polyline::{PlineSource, Polyline};
use cavalier_contours::shape_algorithms::{Shape, ShapeOffsetOptions};
use serde::{Deserialize, Serialize};

use super::adaptive::{Adaptive, adaptive_pocket};
use super::contour::shape_loops;
use super::entry::{Entry, EntrySpace, enter};
use super::path::{self, Path};
use super::{Depths, Direction, Heights, check_mill};
use crate::CamError;
use crate::geom::{Contour, POS_EPS, Region};
use crate::tool::{Cutting, Tool};
use crate::toolpath::{PathBuilder, Toolpath};

/// A pocket operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PocketOp {
    pub region: Region,
    /// The distance between loops (mm); at most the tool's diameter.
    /// `None`: 40 % of the diameter.
    #[serde(default)]
    pub stepover: Option<f64>,
    #[serde(default)]
    pub direction: Direction,
    pub depths: Depths,
    pub heights: Heights,
    /// Material left on the walls and islands (mm).
    #[serde(default)]
    pub stock_to_leave: f64,
    pub cutting: Cutting,
    /// How the tool goes down where a cut starts in the material.
    #[serde(default)]
    pub entry: Entry,
    /// Clear with an engagement-limited path instead of the offset
    /// loops (`stepover` is then not used).
    #[serde(default)]
    pub adaptive: Option<Adaptive>,
}

impl PocketOp {
    pub fn stepover_for(&self, tool: &Tool) -> f64 {
        self.stepover.unwrap_or(tool.diameter * 0.4)
    }
}

/// One loop of the pocket at one depth, with its distance from the walls.
#[derive(Clone, Debug, PartialEq)]
pub struct PocketLoop {
    pub contour: Contour,
    /// The offset from the region's outlines (mm).
    pub offset: f64,
}

/// The loops of a pocket at one depth in cutting order (the innermost
/// first), each oriented for the operation's direction.
pub fn pocket_loops(op: &PocketOp, tool: &Tool) -> Result<Vec<PocketLoop>, CamError> {
    op.region.validate().map_err(CamError::new)?;
    let stepover = op.stepover_for(tool);
    if !(stepover.is_finite() && stepover > 0.0) {
        return Err(CamError::new("The stepover must be more than zero."));
    }
    if stepover > tool.diameter + 1e-9 {
        return Err(CamError::new(
            "The stepover must not be more than the tool's diameter.",
        ));
    }
    if !(op.stock_to_leave.is_finite() && op.stock_to_leave >= 0.0) {
        return Err(CamError::new("The stock to leave must be zero or more."));
    }
    let shape = region_shape(&op.region);
    let options = ShapeOffsetOptions::default();
    let first = tool.radius() + op.stock_to_leave;
    let mut rings: Vec<(f64, Shape<f64>)> = Vec::new();
    let mut d = first;
    let small = tool.radius() * 0.5;
    loop {
        let s = shape.parallel_offset(d, &options);
        if is_empty(&s) {
            break;
        }
        rings.push((d, s));
        d += stepover;
    }
    if rings.is_empty() {
        return Err(CamError::new("The tool does not fit into the pocket."));
    }
    // The middle: loops at half the radius until nothing is left beyond
    // the reach of the last one.
    let mut last = rings.last().expect("a ring").0;
    loop {
        let s = shape.parallel_offset(last + small, &options);
        if is_empty(&s) || small < POS_EPS {
            break;
        }
        last += small;
        rings.push((last, s));
    }
    // The pocket may fall into parts (a waist narrower than the tool): each
    // part, the area within one of the wall loops, is cleared on its own
    // before the next.
    let walls: Vec<Contour> = rings[0]
        .1
        .ccw_plines
        .iter()
        .map(|p| Contour::from_pline(&p.polyline))
        .collect();
    rings.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut parts: Vec<Vec<PocketLoop>> = vec![Vec::new(); walls.len().max(1)];
    for (offset, s) in &rings {
        // Cutting outwards leaves the material outside each loop round the
        // middle and inside each loop round an island: the shape's own
        // orientations put it on the right (climb).
        for contour in shape_loops(s, false) {
            let probe = contour.vertices[0].pos();
            let part = walls
                .iter()
                .position(|w| w.contains(probe) || on_outline(w, probe))
                .unwrap_or(0);
            let contour = if op.direction == Direction::Conventional {
                contour.reversed()
            } else {
                contour
            };
            parts[part].push(PocketLoop {
                contour,
                offset: *offset,
            });
        }
    }
    Ok(parts.into_iter().flatten().collect())
}

fn on_outline(c: &Contour, p: [f64; 2]) -> bool {
    c.vertices
        .iter()
        .any(|v| (v.x - p[0]).hypot(v.y - p[1]) <= 1e-6)
}

pub(crate) fn region_shape(region: &Region) -> Shape<f64> {
    let mut plines: Vec<Polyline<f64>> = vec![region.outer.oriented(true).to_pline()];
    plines.extend(region.islands.iter().map(|i| i.oriented(false).to_pline()));
    Shape::from_plines(plines)
}

pub(crate) fn is_empty(s: &Shape<f64>) -> bool {
    s.ccw_plines.iter().all(|p| p.polyline.vertex_count() < 2)
}

/// Whether the tool center can go straight from `a` to `b` inside the
/// region kept `clear` from its walls (the wall loop's offset less a
/// hair), without crossing any of its outlines.
pub(crate) fn link_inside(allowed: &Shape<f64>, a: [f64; 2], b: [f64; 2]) -> bool {
    let mut seg = Polyline::new();
    cavalier_contours::polyline::PlineSourceMut::add(&mut seg, a[0], a[1], 0.0);
    cavalier_contours::polyline::PlineSourceMut::add(&mut seg, b[0], b[1], 0.0);
    let all = allowed.ccw_plines.iter().chain(allowed.cw_plines.iter());
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let inside = |p: [f64; 2]| {
        let outer = allowed
            .ccw_plines
            .iter()
            .any(|o| Contour::from_pline(&o.polyline).contains(p));
        let hole = allowed
            .cw_plines
            .iter()
            .any(|h| Contour::from_pline(&h.polyline).contains(p));
        outer && !hole
    };
    if !(inside(a) && inside(b) && inside(mid)) {
        return false;
    }
    for p in all {
        let found = seg.find_intersects(&p.polyline);
        if !found.basic_intersects.is_empty() || !found.overlapping_intersects.is_empty() {
            return false;
        }
    }
    true
}

/// Whether a circle of the tool center (`c`, `radius`) stays inside the
/// region and `wall` from its outlines.
pub(crate) fn pocket_fits(region: &Region, wall: f64, c: [f64; 2], radius: f64) -> bool {
    path::inside_region(region, c) && path::region_distance(region, c) >= wall + radius - 1e-6
}

/// The toolpath of a pocket operation.
pub fn pocket(op: &PocketOp, tool: &Tool) -> Result<Toolpath, CamError> {
    if let Some(a) = &op.adaptive {
        return adaptive_pocket(op, tool, a);
    }
    check_mill(tool, &op.cutting, &op.depths)?;
    op.heights.validate(op.depths.top)?;
    op.entry.validate()?;
    let levels = op.depths.levels()?;
    let loops = pocket_loops(op, tool)?;
    let stepover = op.stepover_for(tool);
    let wall = tool.radius() + op.stock_to_leave;
    let allowed =
        region_shape(&op.region).parallel_offset(wall - 1e-4, &ShapeOffsetOptions::default());
    let feed = op.cutting.feed;
    let plunge = op.cutting.plunge_feed;
    let fits = |c: [f64; 2], radius: f64| pocket_fits(&op.region, wall, c, radius);
    let space = EntrySpace {
        helix_fits: &fits,
        min_ramp: tool.radius(),
    };
    let mut b = PathBuilder::new();
    let mut entered = false;
    for (li, &z) in levels.iter().enumerate() {
        let top = if li == 0 {
            op.depths.top
        } else {
            levels[li - 1]
        };
        let mut first = true;
        for l in &loops {
            let at = b
                .at()
                .map(|p| [p[0], p[1]])
                .unwrap_or(l.contour.vertices[0].pos());
            let c = l.contour.started_near(at);
            let start = c.vertices[0].pos();
            let here = b.at();
            // A short link inside the pocket is cut at the floor: it never
            // crosses more than two stepovers of material and never a wall.
            let linked = !first
                && here.is_some_and(|h| {
                    let gap = (h[0] - start[0]).hypot(h[1] - start[1]);
                    gap <= stepover * 2.0 + POS_EPS && link_inside(&allowed, [h[0], h[1]], start)
                });
            if linked {
                b.line([start[0], start[1], z], feed);
            } else {
                // Up and over, then down by the entry (a straight plunge
                // needs a center-cutting end mill; a ramp runs along the
                // loop, a helix beside its start). Inside the pocket the
                // retract height is clear of everything (nothing stands
                // above the top there); the way in comes down from the
                // clearance.
                let hop = if entered {
                    op.heights.retract
                } else {
                    op.heights.clearance
                };
                b.retract_to(start, hop);
                b.rapid([start[0], start[1], op.heights.retract]);
                enter(&mut b, &op.entry, &Path::of(&c), top, z, plunge, &space);
                entered = true;
            }
            b.follow(&c, z, feed);
            first = false;
        }
    }
    let end = b.at().expect("the tool is placed");
    b.rapid([end[0], end[1], op.heights.clearance]);
    Ok(b.finish())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::toolpath::{Move, move_points};

    fn op(region: Region) -> PocketOp {
        PocketOp {
            region,
            stepover: None,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -4.0,
                step_down: 2.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 2.0,
            },
            stock_to_leave: 0.0,
            cutting: Cutting {
                spindle_rpm: 18000.0,
                feed: 1000.0,
                plunge_feed: 300.0,
            },
            entry: Entry::Plunge,
            adaptive: None,
        }
    }

    /// Points the tool center visits at the floor, arcs sampled.
    pub(crate) fn floor_points(p: &Toolpath, z: f64) -> Vec<[f64; 2]> {
        let mut out = Vec::new();
        let mut at = [0.0; 3];
        for m in &p.moves {
            let pts = move_points(at, m, 0.01);
            if !matches!(m, Move::Rapid { .. }) {
                let mut prev = at;
                for q in &pts {
                    if (q[2] - z).abs() < 1e-9 && (prev[2] - z).abs() < 1e-9 {
                        // Sample along the straight pieces too.
                        let n = (((q[0] - prev[0]).hypot(q[1] - prev[1]) / 0.25).ceil() as usize)
                            .max(1);
                        for k in 0..=n {
                            let t = k as f64 / n as f64;
                            out.push([
                                prev[0] + (q[0] - prev[0]) * t,
                                prev[1] + (q[1] - prev[1]) * t,
                            ]);
                        }
                    }
                    prev = *q;
                }
            }
            at = m.to();
        }
        out
    }

    /// Every grid point of the region farther than the tool's radius from
    /// its walls must be within the radius of a visited point.
    pub(crate) fn assert_cleared(region: &Region, tool: &Tool, p: &Toolpath, z: f64) {
        let pts = floor_points(p, z);
        let wall =
            region_shape(region).parallel_offset(tool.radius(), &ShapeOffsetOptions::default());
        let inside = |q: [f64; 2]| {
            wall.ccw_plines
                .iter()
                .any(|o| Contour::from_pline(&o.polyline).contains(q))
                && !wall
                    .cw_plines
                    .iter()
                    .any(|h| Contour::from_pline(&h.polyline).contains(q))
        };
        let (min, max) = p.bounds().unwrap();
        let mut checked = 0;
        let mut x = min[0];
        while x <= max[0] {
            let mut y = min[1];
            while y <= max[1] {
                if inside([x, y]) {
                    checked += 1;
                    let near = pts
                        .iter()
                        .any(|q| (q[0] - x).hypot(q[1] - y) <= tool.radius() + 1e-6);
                    assert!(near, "material left at ({x}, {y})");
                }
                y += 0.5;
            }
            x += 0.5;
        }
        assert!(checked > 0);
    }

    #[test]
    fn a_rectangular_pocket_is_cleared_inside_out_with_its_wall_last() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let region = Region::new(Contour::rectangle([0.0, 0.0], [50.0, 30.0]), vec![]);
        let o = op(region.clone());
        let loops = pocket_loops(&o, &tool).unwrap();
        assert!(loops.len() >= 5);
        assert!(loops.windows(2).all(|w| w[0].offset >= w[1].offset));
        let wall = loops.last().unwrap();
        assert!((wall.offset - 3.0).abs() < 1e-12);
        assert!(
            wall.contour.is_ccw(),
            "climb from the inside out runs counter-clockwise"
        );
        assert!((wall.contour.area() - 44.0 * 24.0).abs() < 1e-6);
        let p = pocket(&o, &tool).unwrap();
        assert_cleared(&region, &tool, &p, -2.0);
        assert_cleared(&region, &tool, &p, -4.0);
        let (min, max) = p.bounds().unwrap();
        assert!(min[0] >= 3.0 - 1e-6 && max[0] <= 47.0 + 1e-6 && min[2] == -4.0);
    }

    #[test]
    fn a_pocket_with_an_island_keeps_off_it_and_clears_round_it() {
        let tool = Tool::flat_end_mill(1, 4.0);
        let island = Contour::circle([25.0, 15.0], 5.0);
        let region = Region::new(
            Contour::rectangle([0.0, 0.0], [50.0, 30.0]),
            vec![island.clone()],
        );
        let p = pocket(&op(region.clone()), &tool).unwrap();
        assert_cleared(&region, &tool, &p, -4.0);
        let mut at = [0.0; 3];
        for m in &p.moves {
            let mut prev = at;
            for q in move_points(at, m, 1e-5) {
                // The closest point of the piece to the island's center.
                let d = [q[0] - prev[0], q[1] - prev[1]];
                let len2 = d[0] * d[0] + d[1] * d[1];
                let t = if len2 > 0.0 {
                    (((25.0 - prev[0]) * d[0] + (15.0 - prev[1]) * d[1]) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let c = [prev[0] + d[0] * t, prev[1] + d[1] * t];
                let low = prev[2].min(q[2]) < 0.0;
                assert!(
                    !low || (c[0] - 25.0).hypot(c[1] - 15.0) >= 5.0 + 2.0 - 1e-4,
                    "gouges the island at {c:?} in {m:?} from {prev:?}"
                );
                prev = q;
            }
            at = m.to();
        }
        let loops = pocket_loops(&op(region), &tool).unwrap();
        let round_island: Vec<&PocketLoop> = loops.iter().filter(|l| !l.contour.is_ccw()).collect();
        assert!(!round_island.is_empty(), "the island's loops run clockwise");
    }

    #[test]
    fn a_circular_pocket_keeps_its_arcs() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let region = Region::new(Contour::circle([0.0, 0.0], 20.0), vec![]);
        let p = pocket(&op(region.clone()), &tool).unwrap();
        assert!(p.moves.iter().all(|m| !matches!(m, Move::Line { to, feed } if *feed == 1000.0 && to[0].hypot(to[1]) > 17.0 + 1e-6)));
        assert!(p.moves.iter().any(|m| matches!(m, Move::Arc { .. })));
        assert_cleared(&region, &tool, &p, -4.0);
    }

    #[test]
    fn a_narrow_waist_splits_the_pocket_and_both_halves_are_cleared() {
        let tool = Tool::flat_end_mill(1, 6.0);
        // Two 30 mm squares joined by a 4 mm neck the tool cannot pass.
        let outer = Contour::polygon(&[
            [0.0, 0.0],
            [30.0, 0.0],
            [30.0, 13.0],
            [40.0, 13.0],
            [40.0, 0.0],
            [70.0, 0.0],
            [70.0, 30.0],
            [40.0, 30.0],
            [40.0, 17.0],
            [30.0, 17.0],
            [30.0, 30.0],
            [0.0, 30.0],
        ]);
        let region = Region::new(outer, vec![]);
        let p = pocket(&op(region.clone()), &tool).unwrap();
        assert_cleared(&region, &tool, &p, -4.0);
        // Going from one half to the other lifts the tool to the retract
        // height: a plunge per half and level, and every rapid across is
        // above the top.
        let plunges = p
            .moves
            .iter()
            .filter(|m| matches!(m, Move::Line { feed, .. } if *feed == 300.0))
            .count();
        assert!(plunges >= 4);
        let mut at = [0.0, 0.0, 10.0];
        for m in &p.moves {
            if let Move::Rapid { to } = m
                && (to[0] != at[0] || to[1] != at[1])
            {
                assert!(to[2] >= 2.0 && at[2] >= 2.0);
            }
            at = m.to();
        }
        // In and out at the clearance height.
        assert_eq!(p.moves[0].to()[2], 10.0);
        assert_eq!(p.moves.last().unwrap().to()[2], 10.0);
    }

    #[test]
    fn a_pocket_smaller_than_the_tool_is_refused() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let e = pocket(
            &op(Region::new(
                Contour::rectangle([0.0, 0.0], [5.0, 5.0]),
                vec![],
            )),
            &tool,
        )
        .unwrap_err();
        assert_eq!(e.message, "The tool does not fit into the pocket.");
        let mut o = op(Region::new(
            Contour::rectangle([0.0, 0.0], [50.0, 50.0]),
            vec![],
        ));
        o.stepover = Some(7.0);
        assert!(pocket(&o, &tool).is_err());
    }

    #[test]
    fn loops_are_linked_by_feed_moves_inside_the_pocket() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let p = pocket(
            &op(Region::new(
                Contour::rectangle([0.0, 0.0], [50.0, 30.0]),
                vec![],
            )),
            &tool,
        )
        .unwrap();
        // One plunge per level: the loops of a level are joined at the floor.
        let plunges = p
            .moves
            .iter()
            .filter(|m| matches!(m, Move::Line { feed, .. } if *feed == 300.0))
            .count();
        assert_eq!(plunges, 2);
    }

    /// Every point of the cuts below the top stays inside the region and
    /// `wall` from its outlines (1e-6 mm allowed); arcs are sampled on
    /// the arc itself.
    pub(crate) fn assert_no_gouge(region: &Region, wall: f64, p: &Toolpath, top: f64) {
        let mut at = [0.0, 0.0, 100.0];
        let mut checked = 0;
        for m in &p.moves {
            let pts: Vec<[f64; 3]> = match *m {
                Move::Rapid { .. } => Vec::new(),
                Move::Arc { .. } => move_points(at, m, 1e-7),
                Move::Line { to, .. } => {
                    let n = ((to[0] - at[0]).hypot(to[1] - at[1]) / 0.05)
                        .ceil()
                        .max(1.0) as usize;
                    (0..=n)
                        .map(|k| {
                            let t = k as f64 / n as f64;
                            [
                                at[0] + (to[0] - at[0]) * t,
                                at[1] + (to[1] - at[1]) * t,
                                at[2] + (to[2] - at[2]) * t,
                            ]
                        })
                        .collect()
                }
            };
            for q in pts.into_iter().filter(|q| q[2] < top) {
                checked += 1;
                let xy = [q[0], q[1]];
                assert!(
                    path::inside_region(region, xy),
                    "outside the pocket at {q:?}"
                );
                let d = path::region_distance(region, xy);
                assert!(d >= wall - 1e-6, "the tool comes {d} from a wall at {q:?}");
            }
            at = m.to();
        }
        assert!(checked > 0);
    }

    #[test]
    fn ramps_and_helixes_enter_the_pocket_without_a_plunge_to_the_floor() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let region = Region::new(Contour::rectangle([0.0, 0.0], [50.0, 30.0]), vec![]);
        for entry in [
            Entry::Ramp { angle: 3.0 },
            Entry::Helix {
                radius: 2.0,
                angle: 3.0,
            },
        ] {
            let mut o = op(region.clone());
            o.entry = entry;
            let p = pocket(&o, &tool).unwrap();
            assert_cleared(&region, &tool, &p, -2.0);
            assert_cleared(&region, &tool, &p, -4.0);
            assert_no_gouge(&region, 3.0, &p, 0.0);
            // The only straight moves down end at the top of a level
            // (through the air); the floors are reached by the entry.
            let mut at = [0.0, 0.0, 100.0];
            for m in &p.moves {
                let to = m.to();
                if matches!(m, Move::Line { .. })
                    && to[0] == at[0]
                    && to[1] == at[1]
                    && to[2] < at[2]
                {
                    assert!(
                        to[2] == 0.0 || to[2] == -2.0,
                        "{entry:?}: a plunge to {to:?}"
                    );
                    assert!(
                        to[2] != -2.0 || at[2] > 0.0,
                        "{entry:?}: a plunge into the first level"
                    );
                }
                at = to;
            }
            let arcs_of_radius_two = p
                .moves
                .iter()
                .filter(|m| matches!(m, Move::Arc { to, center, .. } if ((to[0] - center[0]).hypot(to[1] - center[1]) - 2.0).abs() < 1e-9))
                .count();
            assert_eq!(
                arcs_of_radius_two > 0,
                matches!(entry, Entry::Helix { .. }),
                "{entry:?}"
            );
        }
    }
}
