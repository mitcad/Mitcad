// SPDX-License-Identifier: MIT
//! 3D roughing by levels (docs/cam.md, "3-axis machining"): the stock
//! round the bodies is cleared level by level from the top down, each
//! level a pocket of what is left of the boundary where the tool may go
//! at that height, cleared by the offset loops of [`super::pocket`] or
//! the adaptive clearing of [`super::adaptive`].
//!
//! - **Where the tool may go:** a flat or bull nose end mill is taken as
//!   the cylinder of its radius grown by the stock to leave (a bull nose
//!   lies inside it), dropped onto the bodies at the nodes of a grid over
//!   the boundary; at a level the tool's center may be where that
//!   cylinder, its end raised by the stock to leave, rests no higher than
//!   the level, inside the boundary. The edges of that area are found
//!   exactly at each crossing of the grid ([`super::zlevel`]); the grid is
//!   fine enough that a chord between two of them stays within the
//!   tolerance of the edge.
//! - **The pocket of a level:** that area grown by the tool's radius is
//!   the pocket's walls, cleared with the tolerance and a half (the chords
//!   and the points left out) kept from them, so the tool's center stays
//!   inside the area: the tool stays at least the stock to leave from the
//!   bodies, to the side and below. (Grown by the whole cylinder's radius,
//!   the area's chords round a body's corner, an arc of that radius, would
//!   meet in spikes past the corner.)
//! - **Levels:** at most the step down apart from the top to the bottom,
//!   and at each level floor of the bodies raised by the stock to leave,
//!   so flat floors keep exactly the stock to leave. Each level's pocket
//!   comes down from the level above (by the entry), whose area holds it.
//! - **Boundary:** where the tool's center goes, the stock's outline
//!   usually; the box round the bodies grown by the tool's radius when
//!   none is given.

use cavalier_contours::polyline::{PlineSource, Polyline};
use cavalier_contours::shape_algorithms::{Shape, ShapeOffsetOptions};
use serde::{Deserialize, Serialize};

use super::adaptive::Adaptive;
use super::entry::Entry;
use super::path::Area;
use super::pocket::{PocketOp, is_empty, pocket, region_shape};
use super::surface::{Surface, SurfaceCutter};
use super::zlevel::{Grid, TOLERANCE, check_surface_op, free_chains, grid_step, simplified_loop};
use super::{Depths, Direction, Heights, check_mill};
use crate::CamError;
use crate::geom::{Contour, Region, Vertex};
use crate::tool::{Cutting, Tool};
use crate::toolpath::Toolpath;

/// A 3D roughing operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Roughing3dOp {
    /// Where the tool's center goes: the stock's outline usually; the box
    /// round the bodies grown by the tool's radius when none.
    #[serde(default)]
    pub boundary: Option<Region>,
    /// The pockets' stepover; 40 % of the diameter when none.
    #[serde(default)]
    pub stepover: Option<f64>,
    #[serde(default)]
    pub direction: Direction,
    pub depths: Depths,
    pub heights: Heights,
    /// Material left on the bodies, on the walls and the floors (mm).
    #[serde(default)]
    pub stock_to_leave: f64,
    pub cutting: Cutting,
    #[serde(default)]
    pub entry: Entry,
    /// Clear each level with an engagement-limited path instead of the
    /// offset loops.
    #[serde(default)]
    pub adaptive: Option<Adaptive>,
    /// How far the levels' edges may be from the exact ones between their
    /// points (mm); 0.01 when none.
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// A closed outline through the points of `indices`, those closer than
/// 0.01 µm to the one before left out; none without area.
fn outline(points: &[[f64; 2]], indices: &[usize]) -> Option<Contour> {
    let mut vertices: Vec<Vertex> = Vec::new();
    for &i in indices {
        let p = points[i];
        if vertices
            .last()
            .is_none_or(|v| (v.x - p[0]).hypot(v.y - p[1]) > 1e-5)
        {
            vertices.push(Vertex::line(p[0], p[1]));
        }
    }
    while vertices.len() > 1 {
        let (first, last) = (vertices[0], vertices[vertices.len() - 1]);
        if (first.x - last.x).hypot(first.y - last.y) > 1e-5 {
            break;
        }
        vertices.pop();
    }
    let contour = Contour::closed(vertices);
    (contour.vertices.len() >= 3 && contour.validate().is_ok()).then_some(contour)
}

/// An offset's polyline as an outline, its repeated points left out;
/// none without area.
fn cleaned(p: &Polyline<f64>) -> Option<Contour> {
    let p = p.remove_repeat_pos(1e-5).unwrap_or_else(|| p.clone());
    let mut contour = Contour::from_pline(&p);
    while contour.vertices.len() > 1 {
        let (first, last) = (
            contour.vertices[0],
            contour.vertices[contour.vertices.len() - 1],
        );
        if (first.x - last.x).hypot(first.y - last.y) > 1e-5 {
            break;
        }
        contour.vertices.pop();
    }
    contour.validate().is_ok().then_some(contour)
}

/// The regions of a shape: each outer outline with the holes inside it
/// (in the smallest outer one that holds them).
fn regions_of(shape: &Shape<f64>) -> Vec<Region> {
    let outers: Vec<Contour> = shape
        .ccw_plines
        .iter()
        .filter_map(|p| cleaned(&p.polyline))
        .collect();
    let mut regions: Vec<Region> = outers
        .iter()
        .map(|c| Region::new(c.clone(), Vec::new()))
        .collect();
    for hole in shape.cw_plines.iter().filter_map(|p| cleaned(&p.polyline)) {
        let probe = hole.vertices[0].pos();
        let holder = outers
            .iter()
            .enumerate()
            .filter(|(_, o)| o.contains(probe))
            .min_by(|a, b| a.1.area().abs().total_cmp(&b.1.area().abs()))
            .map(|(i, _)| i);
        if let Some(i) = holder {
            regions[i].islands.push(hole);
        }
    }
    regions
}

/// The toolpath of a 3D roughing.
pub fn roughing(op: &Roughing3dOp, surface: &Surface, tool: &Tool) -> Result<Toolpath, CamError> {
    check_mill(tool, &op.cutting, &op.depths)?;
    op.heights.validate(op.depths.top)?;
    op.entry.validate()?;
    if let Some(a) = &op.adaptive {
        a.validate()?;
    }
    let tolerance = op.tolerance.unwrap_or(TOLERANCE);
    let lift = op.stock_to_leave;
    check_surface_op(surface, tolerance, lift, op.heights.retract)?;
    let envelope = SurfaceCutter::flat(tool.radius() + lift);
    let (smin, smax) = surface.bounds();
    let boundary = match &op.boundary {
        Some(region) => {
            region.validate().map_err(CamError::new)?;
            region.clone()
        }
        None => {
            let r = tool.radius();
            Region::new(
                Contour::rectangle([smin[0] - r, smin[1] - r], [smax[0] + r, smax[1] + r]),
                Vec::new(),
            )
        }
    };
    let area = Area::of_region(&boundary);
    let (bmin, bmax) = area
        .bounds()
        .ok_or_else(|| CamError::new("The boundary encloses no area."))?;
    let grid = Grid::covering(bmin, bmax, grid_step(envelope.radius, tolerance))?;
    // Where the cylinder rests at each node (above everything outside the
    // boundary).
    let rests: Vec<f64> = grid
        .nodes()
        .into_iter()
        .map(|p| {
            if area.contains(p) {
                surface
                    .drop_cutter(&envelope, p)
                    .map_or(f64::NEG_INFINITY, |c| c.z + lift)
            } else {
                f64::INFINITY
            }
        })
        .collect();
    let mut levels = op.depths.levels()?;
    for flat in surface.flats() {
        let z = flat + lift;
        if z < op.depths.top - 1e-6
            && z > op.depths.bottom + 1e-6
            && levels.iter().all(|l| (l - z).abs() > 1e-3)
        {
            levels.push(z);
        }
    }
    levels.sort_by(|a, b| b.total_cmp(a));
    let margin = 1.5 * tolerance;
    let options = ShapeOffsetOptions::default();
    let mut path = Toolpath::default();
    let mut above = op.depths.top;
    for &z in &levels {
        let blocked: Vec<bool> = rests.iter().map(|&rest| rest > z + 1e-9).collect();
        if blocked.iter().all(|&b| b) {
            // Deeper levels have less room still.
            break;
        }
        let test = |p: [f64; 2]| {
            !area.contains(p)
                || surface
                    .drop_cutter(&envelope, p)
                    .is_some_and(|c| c.z + lift > z + 1e-9)
        };
        let loops: Vec<Polyline<f64>> = free_chains(&grid, &blocked, &test, 1e-6)
            .iter()
            .filter(|c| c.closed)
            .filter_map(|c| outline(&c.points, &simplified_loop(&c.points, tolerance / 2.0)))
            .map(|c| c.to_pline())
            .collect();
        if loops.is_empty() {
            break;
        }
        // The walls the tool's own radius out from where its center may
        // go: a pocket keeps its center the radius and the margin inside
        // them. Not grown by the whole envelope and kept the stock to leave
        // off: round a body's corner the area's edge is an arc of the
        // envelope's radius in chords, and grown by that radius its chords
        // meet in spikes up to a quarter of a chord past the corner, which
        // the pocket would follow into the stock to leave.
        let walls = Shape::from_plines(loops).parallel_offset(-tool.radius(), &options);
        for region in regions_of(&walls) {
            // Room for the tool at all.
            if is_empty(&region_shape(&region).parallel_offset(tool.radius() + margin, &options)) {
                continue;
            }
            let level = PocketOp {
                region,
                stepover: op.stepover,
                direction: op.direction,
                depths: Depths {
                    top: above,
                    bottom: z,
                    step_down: above - z,
                },
                heights: op.heights,
                stock_to_leave: margin,
                cutting: op.cutting,
                entry: op.entry,
                adaptive: op.adaptive,
            };
            path.moves.extend(pocket(&level, tool)?.moves);
        }
        above = z;
    }
    if path.moves.is_empty() {
        return Err(CamError::new(
            "The 3D roughing has nothing to clear between its top and bottom.",
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::TriMesh;
    use crate::toolpath::{Move, P3, move_points};

    /// A closed box between two corners, its triangles facing out.
    fn cuboid(mesh: &mut TriMesh, lo: P3, hi: P3) {
        let base = mesh.vertices.len() as u32;
        for i in 0..8 {
            mesh.vertices.push([
                if i & 1 == 1 { hi[0] } else { lo[0] },
                if i & 2 == 2 { hi[1] } else { lo[1] },
                if i & 4 == 4 { hi[2] } else { lo[2] },
            ]);
        }
        for q in [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ] {
            mesh.triangles.push([base + q[0], base + q[1], base + q[2]]);
            mesh.triangles.push([base + q[0], base + q[2], base + q[3]]);
        }
    }

    /// How far a flat end mill of `radius` with its tip at `p` (its
    /// cylinder from the tip up) is from a box.
    fn clearance(p: P3, radius: f64, lo: P3, hi: P3) -> f64 {
        let dx = (lo[0] - p[0]).max(p[0] - hi[0]).max(0.0);
        let dy = (lo[1] - p[1]).max(p[1] - hi[1]).max(0.0);
        let side = (dx.hypot(dy) - radius).max(0.0);
        let below = (p[2] - hi[2]).max(0.0);
        side.hypot(below)
    }

    /// Points along the cutting moves, at most 0.1 mm apart.
    fn cut_points(p: &Toolpath) -> Vec<P3> {
        let mut out = Vec::new();
        let mut at: Option<P3> = None;
        for m in &p.moves {
            if let (Some(from), Move::Line { .. } | Move::Arc { .. }) = (at, m) {
                let mut prev = from;
                for q in move_points(from, m, 0.001) {
                    let n = ((((q[0] - prev[0]).powi(2)
                        + (q[1] - prev[1]).powi(2)
                        + (q[2] - prev[2]).powi(2))
                    .sqrt()
                        / 0.1)
                        .ceil() as usize)
                        .max(1);
                    for k in 1..=n {
                        let t = k as f64 / n as f64;
                        out.push([
                            prev[0] + (q[0] - prev[0]) * t,
                            prev[1] + (q[1] - prev[1]) * t,
                            prev[2] + (q[2] - prev[2]) * t,
                        ]);
                    }
                    prev = q;
                }
            }
            at = Some(m.to());
        }
        out
    }

    /// A stepped block (a 40 x 30 base 8 high under a 20 x 30 step 8
    /// higher) roughed from a 20 mm stock: the tool never comes nearer the
    /// block than the stock to leave, the shelf's floor keeps exactly the
    /// stock to leave, and the step's wall no more than that and the
    /// tolerance and a half.
    #[test]
    fn roughing_a_stepped_block_keeps_the_stock_to_leave() {
        let base = ([0.0, 0.0, -20.0], [40.0, 30.0, -12.0]);
        let step = ([0.0, 0.0, -12.0], [20.0, 30.0, -4.0]);
        let mut mesh = TriMesh::default();
        cuboid(&mut mesh, base.0, base.1);
        cuboid(&mut mesh, step.0, step.1);
        let surface = Surface::new(&mesh).unwrap();
        let tool = Tool::flat_end_mill(1, 6.0);
        let stock = 0.5;
        let op = Roughing3dOp {
            boundary: Some(Region::new(
                Contour::rectangle([-10.0, -10.0], [50.0, 40.0]),
                Vec::new(),
            )),
            stepover: None,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -20.0,
                step_down: 3.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 2.0,
            },
            stock_to_leave: stock,
            cutting: Cutting {
                spindle_rpm: 18000.0,
                feed: 1200.0,
                plunge_feed: 300.0,
            },
            entry: Entry::Plunge,
            adaptive: None,
            tolerance: None,
        };
        let path = roughing(&op, &surface, &tool).unwrap();
        assert!(matches!(path.moves[0], Move::Rapid { .. }));
        let points = cut_points(&path);
        assert!(points.len() > 1000);
        let mut nearest = f64::INFINITY;
        for p in &points {
            for (lo, hi) in [base, step] {
                let c = clearance(*p, 3.0, lo, hi);
                assert!(c >= stock - 1e-6, "{c} from the block at {p:?}");
                nearest = nearest.min(c);
            }
        }
        assert!(
            nearest <= stock + 0.05,
            "the tool keeps {nearest} off the block"
        );
        // The shelf: over the base's top beside the step, the floor at the
        // stock to leave above it.
        let shelf = points
            .iter()
            .filter(|p| p[0] > 24.0 && p[0] < 36.5 && p[1] > 3.5 && p[1] < 26.5)
            .map(|p| p[2])
            .fold(f64::INFINITY, f64::min);
        assert!(
            (shelf - (-12.0 + stock)).abs() < 1e-9,
            "the shelf cut to {shelf}"
        );
        // The step's wall: the levels between the shelf and the step's top
        // come to the stock to leave and the tolerance and a half of it.
        let wall = points
            .iter()
            .filter(|p| p[2] < -4.5 && p[2] > -11.0 && p[1] > 5.0 && p[1] < 25.0 && p[0] > 20.0)
            .map(|p| p[0] - 20.0 - 3.0)
            .fold(f64::INFINITY, f64::min);
        assert!(
            wall >= stock - 1e-6 && wall <= stock + 0.02,
            "the wall keeps {wall}"
        );
        // Round the base, down to the bottom.
        let lowest = points.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
        assert!((lowest + 20.0).abs() < 1e-9);
    }

    #[test]
    fn roughing_refuses_a_ball_and_an_empty_level() {
        let mut mesh = TriMesh::default();
        cuboid(&mut mesh, [0.0, 0.0, -10.0], [10.0, 10.0, 0.0]);
        let surface = Surface::new(&mesh).unwrap();
        let mut tool = Tool::flat_end_mill(1, 6.0);
        let op = Roughing3dOp {
            boundary: Some(Region::new(
                Contour::rectangle([0.0, 0.0], [10.0, 10.0]),
                Vec::new(),
            )),
            stepover: None,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -10.0,
                step_down: 3.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 2.0,
            },
            stock_to_leave: 0.0,
            cutting: Cutting {
                spindle_rpm: 18000.0,
                feed: 1200.0,
                plunge_feed: 300.0,
            },
            entry: Entry::Plunge,
            adaptive: None,
            tolerance: None,
        };
        // The block fills the boundary: nothing to clear.
        let e = roughing(&op, &surface, &tool).unwrap_err();
        assert!(e.message.contains("nothing to clear"), "{e}");
        tool.shape = crate::tool::ToolShape::BallEndMill;
        assert!(roughing(&op, &surface, &tool).is_err());
    }
}
