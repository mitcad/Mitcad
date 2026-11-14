// SPDX-License-Identifier: MIT
//! Rest clean-up: what a bigger tool left in a pocket's corners (and in
//! parts it could not enter), cut by a smaller one along the walls
//! (docs/cam.md, "Rest clean-up").
//!
//! The bigger tool of radius `R`, its center kept `R + stock` from the
//! walls, cleared everything within `R` of where its center could go. The
//! smaller tool of radius `r` runs along its own wall loop; where its
//! center is within `R - r` of where the bigger one's could go it cuts
//! nothing new, so only the stretches farther out are cut: the corners
//! the bigger tool rounded, and whole loops where it never went. Each
//! stretch is lengthened a radius at both ends into what is cleared, so
//! that the tool comes in and leaves there.

use cavalier_contours::shape_algorithms::ShapeOffsetOptions;
use serde::{Deserialize, Serialize};

use super::contour::shape_loops;
use super::entry::{Entry, EntrySpace, enter};
use super::path::{Area, Path, dist2};
use super::pocket::{is_empty, pocket_fits, region_shape};
use super::{Depths, Direction, Heights, check_mill};
use crate::CamError;
use crate::geom::{Contour, Region};
use crate::tool::{Cutting, Tool};
use crate::toolpath::{PathBuilder, Toolpath};

/// A rest clean-up after a bigger tool.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RestOp {
    /// The pocket the bigger tool cleared.
    pub region: Region,
    /// The bigger tool's diameter (mm).
    pub previous_diameter: f64,
    /// The stock the bigger tool left on the walls (mm).
    #[serde(default)]
    pub previous_stock_to_leave: f64,
    #[serde(default)]
    pub direction: Direction,
    pub depths: Depths,
    pub heights: Heights,
    /// The stock this tool leaves on the walls (mm).
    #[serde(default)]
    pub stock_to_leave: f64,
    pub cutting: Cutting,
    #[serde(default)]
    pub entry: Entry,
}

/// The stretches `(from, to)` along `cut` where the point is outside
/// `cleared`, found every `step` and pinned down by halving.
fn outside_stretches(cut: &Path, cleared: &Area, step: f64) -> Vec<(f64, f64)> {
    let total = cut.length();
    let n = ((total / step).ceil() as usize).max(2);
    let outside = |t: f64| cut.point_at(t).is_some_and(|p| !cleared.contains(p));
    let mut out = Vec::new();
    let mut prev_t = 0.0;
    let mut prev = outside(0.0);
    let mut from = prev.then_some(0.0);
    for i in 1..=n {
        let t = total * i as f64 / n as f64;
        let now = outside(t);
        if now != prev {
            let (mut lo, mut hi) = (prev_t, t);
            for _ in 0..50 {
                let mid = (lo + hi) / 2.0;
                if outside(mid) == prev {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let x = (lo + hi) / 2.0;
            if now {
                from = Some(x);
            } else if let Some(a) = from.take() {
                out.push((a, x));
            }
        }
        prev_t = t;
        prev = now;
    }
    if let Some(a) = from {
        out.push((a, total));
    }
    out
}

/// The stretches of a closed loop to cut, each lengthened by `ext` at
/// both ends; those closer than that joined, round the start too.
fn pieces_of_loop(cut: &Path, stretches: &[(f64, f64)], ext: f64) -> Vec<Path> {
    let total = cut.length();
    if stretches.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for &(a, b) in stretches {
        match merged.last_mut() {
            Some(last) if a - last.1 < 2.0 * ext => last.1 = b,
            _ => merged.push((a, b)),
        }
    }
    if merged.len() > 1 {
        let first = merged[0];
        let last = *merged.last().expect("a stretch");
        if first.0 + total - last.1 < 2.0 * ext {
            merged.pop();
            merged[0] = (last.0, first.1 + total);
        }
    }
    let twice = cut.joined(cut);
    merged
        .into_iter()
        .map(|(a, b)| {
            let len = b - a + 2.0 * ext;
            if len >= total - 1e-9 {
                return cut.clone();
            }
            let mut from = a - ext;
            if from < 0.0 {
                from += total;
            }
            twice.slice(from, from + len)
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// The stretches of the walls the smaller tool cuts, in the operation's
/// direction.
pub fn rest_pieces(op: &RestOp, tool: &Tool) -> Result<Vec<Vec<[f64; 2]>>, CamError> {
    Ok(rest_paths(op, tool)?
        .iter()
        .map(|p| {
            let mut pts = vec![p.start().unwrap_or([0.0, 0.0])];
            pts.extend(p.segs.iter().map(|s| s.end));
            pts
        })
        .collect())
}

fn rest_paths(op: &RestOp, tool: &Tool) -> Result<Vec<Path>, CamError> {
    op.region.validate().map_err(CamError::new)?;
    let r = tool.radius();
    if !(op.previous_diameter.is_finite() && op.previous_diameter > tool.diameter + 1e-9) {
        return Err(CamError::new(
            "The tool for the rest must be smaller than the one before it.",
        ));
    }
    for s in [op.stock_to_leave, op.previous_stock_to_leave] {
        if !(s.is_finite() && s >= 0.0) {
            return Err(CamError::new("The stock to leave must be zero or more."));
        }
    }
    let big = op.previous_diameter / 2.0;
    let options = ShapeOffsetOptions::default();
    let shape = region_shape(&op.region);
    // Where this tool's center cuts nothing new: within `big - r` of where
    // the bigger tool's center could go (a hair more, so that a wall the
    // bigger tool ran along counts as cleared).
    let reach = shape.parallel_offset(big + op.previous_stock_to_leave, &options);
    let cleared = if is_empty(&reach) {
        None
    } else {
        Some(Area::of_shape(
            &reach.parallel_offset(-(big - r) - 1e-3, &options),
        ))
    };
    let walls = shape.parallel_offset(r + op.stock_to_leave, &options);
    if is_empty(&walls) {
        return Err(CamError::new("The tool does not fit into the pocket."));
    }
    let step = (r * 0.05).max(1e-3);
    let mut out = Vec::new();
    for l in shape_loops(&walls, false) {
        let l: Contour = if op.direction == Direction::Conventional {
            l.reversed()
        } else {
            l
        };
        let cut = Path::of(&l);
        let stretches = match &cleared {
            Some(c) => outside_stretches(&cut, c, step),
            None => vec![(0.0, cut.length())],
        };
        out.extend(pieces_of_loop(&cut, &stretches, r));
    }
    Ok(out)
}

/// The toolpath of a rest clean-up: each stretch at every level, nearest
/// first, coming down by the entry and going up to the retract height
/// after it.
pub fn rest(op: &RestOp, tool: &Tool) -> Result<Toolpath, CamError> {
    check_mill(tool, &op.cutting, &op.depths)?;
    op.heights.validate(op.depths.top)?;
    op.entry.validate()?;
    let levels = op.depths.levels()?;
    let mut left = rest_paths(op, tool)?;
    let wall = tool.radius() + op.stock_to_leave;
    let fits = |c: [f64; 2], radius: f64| pocket_fits(&op.region, wall, c, radius);
    let space = EntrySpace {
        helix_fits: &fits,
        min_ramp: tool.radius(),
    };
    // Nearest first.
    let mut ordered = Vec::new();
    let mut at = [0.0, 0.0];
    while !left.is_empty() {
        let i = (0..left.len())
            .min_by(|&a, &b| {
                let da = dist2(left[a].start().unwrap_or(at), at);
                let db = dist2(left[b].start().unwrap_or(at), at);
                da.total_cmp(&db)
            })
            .expect("a stretch");
        let p = left.remove(i);
        at = p.end().unwrap_or(at);
        ordered.push(p);
    }
    let mut b = PathBuilder::new();
    let mut entered = false;
    for (li, &z) in levels.iter().enumerate() {
        let top = if li == 0 {
            op.depths.top
        } else {
            levels[li - 1]
        };
        for p in &ordered {
            let Some(start) = p.start() else {
                continue;
            };
            let hop = if entered {
                op.heights.retract
            } else {
                op.heights.clearance
            };
            b.retract_to(start, hop);
            b.rapid([start[0], start[1], op.heights.retract]);
            enter(&mut b, &op.entry, p, top, z, op.cutting.plunge_feed, &space);
            p.cut(&mut b, z, z, op.cutting.feed);
            entered = true;
        }
    }
    if let Some(end) = b.at() {
        b.rapid([end[0], end[1], op.heights.clearance]);
    }
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::path;
    use crate::ops::pocket::tests::{assert_no_gouge, floor_points};

    fn op(region: Region, previous_diameter: f64) -> RestOp {
        RestOp {
            region,
            previous_diameter,
            previous_stock_to_leave: 0.0,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -3.0,
                step_down: 3.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 2.0,
            },
            stock_to_leave: 0.0,
            cutting: Cutting {
                spindle_rpm: 20000.0,
                feed: 800.0,
                plunge_feed: 200.0,
            },
            entry: Entry::Plunge,
        }
    }

    #[test]
    fn the_corners_a_bigger_tool_rounded_are_cleaned_up() {
        let tool = Tool::flat_end_mill(2, 4.0);
        let region = Region::new(Contour::rectangle([0.0, 0.0], [40.0, 30.0]), vec![]);
        let o = op(region.clone(), 10.0);
        let pieces = rest_paths(&o, &tool).unwrap();
        assert_eq!(pieces.len(), 4, "one stretch per corner");
        for p in &pieces {
            // About 2 x 2.92 round the corner and a radius more at each
            // end.
            assert!(p.length() > 6.0 && p.length() < 14.0, "{}", p.length());
        }
        let t = rest(&o, &tool).unwrap();
        assert_no_gouge(&region, 2.0, &t, 0.0);
        // After both tools, every point the small tool can reach is within
        // reach of one of them: the big tool's cleared area or the rest.
        let big = region_shape(&region).parallel_offset(5.0, &ShapeOffsetOptions::default());
        let big = Area::of_shape(&big.parallel_offset(-5.0, &ShapeOffsetOptions::default()));
        // What the small tool can reach: the pocket with corners of its
        // radius.
        let small = region_shape(&region).parallel_offset(2.0, &ShapeOffsetOptions::default());
        let small = Area::of_shape(&small.parallel_offset(-2.0, &ShapeOffsetOptions::default()));
        let pts = floor_points(&t, -3.0);
        let mut corners = 0;
        let mut x = 0.125;
        while x <= 40.0 {
            let mut y = 0.125;
            while y <= 30.0 {
                let q = [x, y];
                assert!(path::inside_region(&region, q));
                if small.contains(q) && !big.contains(q) {
                    corners += 1;
                    assert!(
                        pts.iter().any(|p| dist2(*p, q) <= 2.0 + 1e-6),
                        "material left at ({x}, {y})"
                    );
                }
                y += 0.25;
            }
            x += 0.25;
        }
        assert!(corners > 0);
        // Nothing is cut away from the corners.
        for p in &pts {
            let to_corner = [[0.0, 0.0], [40.0, 0.0], [40.0, 30.0], [0.0, 30.0]]
                .iter()
                .map(|c| dist2(*p, *c))
                .fold(f64::INFINITY, f64::min);
            assert!(to_corner < 12.0, "a cut away from the corners at {p:?}");
        }
    }

    #[test]
    fn a_slot_the_bigger_tool_could_not_enter_is_cut_whole() {
        let tool = Tool::flat_end_mill(2, 4.0);
        // 7 mm wide: a 10 mm tool never went in.
        let region = Region::new(Contour::rectangle([0.0, 0.0], [30.0, 7.0]), vec![]);
        let o = op(region.clone(), 10.0);
        let pieces = rest_paths(&o, &tool).unwrap();
        assert_eq!(pieces.len(), 1);
        let whole = Path::of(&Contour::rectangle([2.0, 2.0], [28.0, 5.0]));
        assert!((pieces[0].length() - whole.length()).abs() < 1e-6);
        assert_no_gouge(&region, 2.0, &rest(&o, &tool).unwrap(), 0.0);
    }

    #[test]
    fn the_rest_tool_must_be_the_smaller() {
        let tool = Tool::flat_end_mill(2, 10.0);
        let region = Region::new(Contour::rectangle([0.0, 0.0], [40.0, 30.0]), vec![]);
        assert!(rest(&op(region, 6.0), &tool).is_err());
    }
}
