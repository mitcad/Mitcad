// SPDX-License-Identifier: MIT
//! Waterline finishing (docs/cam.md, "3-axis machining"): at each level
//! the tool runs round the constant-height contours of its offset surface
//! over the bodies, the loops along which it just touches them with its
//! tip at that height; for steep walls, where raster passes would be far
//! apart down the wall.
//!
//! - The cutter (grown by the stock to leave and raised by it) is dropped
//!   at the nodes of a grid round the bodies once; at each level a node is
//!   blocked where the cutter would rest above the level. The contours
//!   between free and blocked nodes ([`super::zlevel`]) are found exactly
//!   at each crossing of the grid by bisection, each point free (the tool
//!   there clears the surfaces), the grid fine enough that a chord between
//!   two points of a contour as curved as the cutter stays within the
//!   tolerance; the points a line passes within the tolerance of are then
//!   left out.
//! - **Direction:** climb milling (the default) runs round the bodies
//!   clockwise and round a cavity counter-clockwise, the part on the
//!   right; conventional the other way.
//! - **Slope:** with `min_slope` only where the surface the cutter rests
//!   on is at least that steep (the shallow areas are left to raster
//!   finishing).
//! - **Boundary:** only the stretches of the contours inside it are cut.
//! - Each loop or stretch is entered from the retract height at the
//!   plunge feed, the nearest next; the tool goes up to the retract height
//!   between them.

use serde::{Deserialize, Serialize};

use super::path::Area;
use super::surface::{Surface, SurfaceCutter};
use super::zlevel::{
    Grid, TOLERANCE, check_slope, check_surface_op, free_chains, grid_step, simplified,
    simplified_loop, travel,
};
use super::{Depths, Direction, Heights};
use crate::CamError;
use crate::geom::Region;
use crate::tool::{Cutting, Tool};
use crate::toolpath::{PathBuilder, Toolpath};

/// A waterline finishing operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaterlineOp {
    /// Only the contours' stretches inside it are cut; all when none.
    #[serde(default)]
    pub boundary: Option<Region>,
    /// The levels: from below `top` down to `bottom` at most `step_down`
    /// apart.
    pub depths: Depths,
    pub heights: Heights,
    /// Cut only where the surface is at least this steep (degrees from the
    /// horizontal); every slope when none.
    #[serde(default)]
    pub min_slope: Option<f64>,
    #[serde(default)]
    pub stock_to_leave: f64,
    #[serde(default)]
    pub direction: Direction,
    pub cutting: Cutting,
    /// How far the path may be from the exact one between its points
    /// (mm); 0.01 when none.
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// A loop or a stretch of one to cut at a level.
#[derive(Clone, Debug, PartialEq)]
struct Piece {
    points: Vec<[f64; 2]>,
    closed: bool,
}

/// The pieces of a contour that are cut: the points `keep` says, each
/// stretch of two points or more.
fn pieces_of(points: &[[f64; 2]], keep: &[bool], closed: bool) -> Vec<Piece> {
    let n = points.len();
    if keep.iter().all(|&k| k) {
        return vec![Piece {
            points: points.to_vec(),
            closed,
        }];
    }
    // A closed contour from a point that is not cut, round to it again.
    let start = if closed {
        keep.iter().position(|&k| !k).unwrap_or(0)
    } else {
        0
    };
    let order: Vec<usize> = (0..n).map(|k| (start + k) % n).collect();
    let mut out = Vec::new();
    let mut run: Vec<[f64; 2]> = Vec::new();
    for &k in &order {
        if keep[k] {
            run.push(points[k]);
        } else if !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        out.push(run);
    }
    out.into_iter()
        .filter(|r| r.len() >= 2)
        .map(|points| Piece {
            points,
            closed: false,
        })
        .collect()
}

/// The toolpath of a waterline finishing.
pub fn waterline(op: &WaterlineOp, surface: &Surface, tool: &Tool) -> Result<Toolpath, CamError> {
    let cutter = SurfaceCutter::of_tool(tool)?;
    op.cutting.validate().map_err(CamError::new)?;
    op.heights.validate(op.depths.top)?;
    let levels = op.depths.levels()?;
    let tolerance = op.tolerance.unwrap_or(TOLERANCE);
    check_surface_op(surface, tolerance, op.stock_to_leave, op.heights.retract)?;
    check_slope(op.min_slope)?;
    let lift = op.stock_to_leave;
    let grown = cutter.grown(lift);
    let area = match &op.boundary {
        Some(region) => {
            region.validate().map_err(CamError::new)?;
            Some(Area::of_region(region))
        }
        None => None,
    };
    let (smin, smax) = surface.bounds();
    let reach = grown.radius;
    let grid = Grid::covering(
        [smin[0] - reach, smin[1] - reach],
        [smax[0] + reach, smax[1] + reach],
        grid_step(grown.radius, tolerance),
    )?;
    let rests: Vec<f64> = grid
        .nodes()
        .into_iter()
        .map(|p| {
            surface
                .drop_cutter(&grown, p)
                .map_or(f64::NEG_INFINITY, |c| c.z + lift)
        })
        .collect();
    let precision = (tolerance / 100.0).min(1e-7);
    let mut b = PathBuilder::new();
    for &z in &levels {
        let blocked: Vec<bool> = rests.iter().map(|&rest| rest > z).collect();
        let test = |p: [f64; 2]| {
            surface
                .drop_cutter(&grown, p)
                .is_some_and(|c| c.z + lift > z)
        };
        let mut pieces = Vec::new();
        for chain in free_chains(&grid, &blocked, &test, precision) {
            // The surface where the cutter rests just inside each point.
            let keep: Vec<bool> = chain
                .points
                .iter()
                .zip(&chain.inner)
                .map(|(&p, &inner)| {
                    area.as_ref().is_none_or(|a| a.contains(p))
                        && op.min_slope.is_none_or(|m| {
                            surface
                                .drop_cutter(&grown, inner)
                                .is_some_and(|c| c.slope() >= m - 1e-9)
                        })
                })
                .collect();
            for piece in pieces_of(&chain.points, &keep, chain.closed) {
                let kept = if piece.closed {
                    simplified_loop(&piece.points, tolerance)
                } else {
                    simplified(&piece.points, tolerance)
                };
                let mut points: Vec<[f64; 2]> = kept.into_iter().map(|i| piece.points[i]).collect();
                if op.direction == Direction::Conventional {
                    points.reverse();
                }
                if points.len() >= 2 {
                    pieces.push(Piece {
                        points,
                        closed: piece.closed,
                    });
                }
            }
        }
        // The nearest piece next: a loop from its nearest point.
        while !pieces.is_empty() {
            let here = b.at().map(|p| [p[0], p[1]]).unwrap_or(pieces[0].points[0]);
            let dist = |q: [f64; 2]| (q[0] - here[0]).hypot(q[1] - here[1]);
            let mut best = (0, 0, f64::INFINITY);
            for (i, piece) in pieces.iter().enumerate() {
                let candidates = if piece.closed { piece.points.len() } else { 1 };
                for (k, &q) in piece.points.iter().enumerate().take(candidates) {
                    let d = dist(q);
                    if d < best.2 {
                        best = (i, k, d);
                    }
                }
            }
            let mut piece = pieces.swap_remove(best.0);
            piece.points.rotate_left(best.1);
            let start = piece.points[0];
            if b.at().is_none() {
                travel(&mut b, start, op.heights.clearance);
            }
            travel(&mut b, start, op.heights.retract);
            b.line([start[0], start[1], z], op.cutting.plunge_feed);
            for p in &piece.points[1..] {
                b.line([p[0], p[1], z], op.cutting.feed);
            }
            if piece.closed {
                b.line([start[0], start[1], z], op.cutting.feed);
            }
        }
    }
    let Some(end) = b.at() else {
        return Err(CamError::new(
            "The waterline finishing has nothing to cut between its top and bottom.",
        ));
    };
    b.rapid([end[0], end[1], op.heights.clearance]);
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::TriMesh;
    use crate::ops::raster::tests::{ball, cutting, hemisphere};
    use crate::toolpath::Move;

    /// A closed cylinder of `radius` about the Z axis from `bottom` to
    /// `top`, `segments` round (its corners on the circle), with its top.
    fn cylinder(radius: f64, bottom: f64, top: f64, segments: usize) -> TriMesh {
        let mut mesh = TriMesh::default();
        for z in [bottom, top] {
            for k in 0..segments {
                let a = std::f64::consts::TAU * k as f64 / segments as f64;
                mesh.vertices.push([radius * a.cos(), radius * a.sin(), z]);
            }
        }
        mesh.vertices.push([0.0, 0.0, top]);
        let middle = (2 * segments) as u32;
        let n = segments as u32;
        for k in 0..n {
            let k1 = (k + 1) % n;
            mesh.triangles.push([k, k1, n + k1]);
            mesh.triangles.push([k, n + k1, n + k]);
            mesh.triangles.push([n + k, n + k1, middle]);
        }
        mesh
    }

    fn op(top: f64, bottom: f64, step_down: f64) -> WaterlineOp {
        WaterlineOp {
            boundary: None,
            depths: Depths {
                top,
                bottom,
                step_down,
            },
            heights: Heights {
                clearance: 15.0,
                retract: 5.0,
            },
            min_slope: None,
            stock_to_leave: 0.0,
            direction: Direction::Climb,
            cutting: cutting(),
            tolerance: None,
        }
    }

    /// The cutting moves at each height: their end points.
    fn cuts(p: &Toolpath) -> Vec<[f64; 3]> {
        p.moves
            .iter()
            .filter_map(|m| match m {
                Move::Line { to, .. } if to[2] < 0.0 => Some(*to),
                _ => None,
            })
            .collect()
    }

    /// Round a cylinder a flat end mill's waterline is a circle at the
    /// cylinder's radius and the tool's: every point within the mesh's
    /// deviation of it (its sides are chords of the circle), the whole way
    /// round, clockwise (climb milling round the part).
    #[test]
    fn waterline_round_a_cylinder_is_a_circle_at_the_offset_radius() {
        let (radius, segments) = (10.0, 128);
        let surface = Surface::new(&cylinder(radius, -20.0, 0.0, segments)).unwrap();
        let tool = Tool::flat_end_mill(1, 6.0);
        let path = waterline(&op(0.0, -10.0, 5.0), &surface, &tool).unwrap();
        let inner = radius * (std::f64::consts::PI / segments as f64).cos() + 3.0;
        for level in [-5.0, -10.0] {
            let points: Vec<[f64; 3]> = cuts(&path)
                .into_iter()
                .filter(|p| (p[2] - level).abs() < 1e-12)
                .collect();
            assert!(points.len() > 20, "{level}: {}", points.len());
            let mut angles = Vec::new();
            for p in &points {
                let r = p[0].hypot(p[1]);
                assert!(
                    r >= inner - 1e-6 && r <= radius + 3.0 + 1e-6,
                    "{r} at {level}"
                );
                angles.push(p[1].atan2(p[0]));
            }
            angles.sort_by(f64::total_cmp);
            let mut gap = angles[0] + std::f64::consts::TAU - angles[angles.len() - 1];
            for w in angles.windows(2) {
                gap = gap.max(w[1] - w[0]);
            }
            assert!(
                gap < 10.0_f64.to_radians(),
                "a gap of {} degrees",
                gap.to_degrees()
            );
            // Clockwise: the signed area of the points in order.
            let area: f64 = (0..points.len())
                .map(|k| {
                    let (a, b) = (points[k], points[(k + 1) % points.len()]);
                    a[0] * b[1] - b[0] * a[1]
                })
                .sum();
            assert!(area < 0.0, "climb milling runs clockwise round the part");
        }
        // Conventional milling: the same circle the other way.
        let mut conventional = op(0.0, -10.0, 5.0);
        conventional.direction = Direction::Conventional;
        let path = waterline(&conventional, &surface, &tool).unwrap();
        let points: Vec<[f64; 3]> = cuts(&path)
            .into_iter()
            .filter(|p| (p[2] + 5.0).abs() < 1e-12)
            .collect();
        let area: f64 = (0..points.len())
            .map(|k| {
                let (a, b) = (points[k], points[(k + 1) % points.len()]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum();
        assert!(area > 0.0);
        // With stock to leave the circle is that much larger.
        let mut stock = op(0.0, -10.0, 5.0);
        stock.stock_to_leave = 0.5;
        let path = waterline(&stock, &surface, &tool).unwrap();
        for p in cuts(&path) {
            let r = p[0].hypot(p[1]);
            assert!(r >= inner + 0.5 - 1e-6 && r <= radius + 3.5 + 1e-6, "{r}");
        }
    }

    /// With a slope limit only the steep part of a hemisphere is cut: a
    /// ball's contours down the sphere, from where the surface is about 45
    /// degrees steep.
    #[test]
    fn waterline_keeps_to_the_steep_surfaces() {
        let (s, r) = (20.0_f64, 3.0_f64);
        let center = [0.0, 0.0, -s];
        let phis: Vec<f64> = (0..=36).map(|i| (i as f64 * 2.5).to_radians()).collect();
        let surface = Surface::new(&hemisphere(center, s, &phis, 72)).unwrap();
        let mut steep = op(0.0, -18.0, 2.0);
        steep.min_slope = Some(45.0);
        let path = waterline(&steep, &surface, &ball(1, 2.0 * r)).unwrap();
        let points = cuts(&path);
        // The ball's center at 45 degrees: (z + r - center) / (s + r).
        let at45 = center[2] + (s + r) * 45.0_f64.to_radians().cos() - r;
        assert!(
            points.iter().all(|p| p[2] <= at45 + 1.5),
            "a cut above the steep part"
        );
        assert!(points.iter().any(|p| (p[2] + 18.0).abs() < 1e-9));
        for p in &points {
            // The ball on the sphere's mesh: never outside the sphere grown
            // by its radius, within the mesh's deviation inside it.
            let c = [p[0], p[1], p[2] + r];
            let d = (c[0].powi(2) + c[1].powi(2) + (c[2] - center[2]).powi(2)).sqrt();
            assert!(d <= s + r + 0.02 && d >= s + r - 0.1, "{d}");
        }
        // Every slope: the shallow levels too.
        let path = waterline(&op(0.0, -18.0, 2.0), &surface, &ball(1, 2.0 * r)).unwrap();
        assert!(cuts(&path).iter().any(|p| (p[2] + 2.0).abs() < 1e-9));
    }

    #[test]
    fn a_contour_is_cut_where_it_is_kept() {
        let pts = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let all = pieces_of(&pts, &[true; 4], true);
        assert_eq!(all.len(), 1);
        assert!(all[0].closed);
        let some = pieces_of(&pts, &[true, false, true, true], true);
        assert_eq!(some.len(), 1);
        assert_eq!(some[0].points, vec![[1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]);
        assert!(!some[0].closed);
        assert!(pieces_of(&pts, &[false, true, false, true], false).is_empty());
    }
}
