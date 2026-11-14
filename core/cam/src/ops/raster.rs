// SPDX-License-Identifier: MIT
//! Raster finishing (docs/cam.md, "3-axis machining"): parallel passes
//! across a boundary at a stepover, the tool dropped onto the bodies'
//! surfaces along each pass (the exact drop-cutter of [`super::surface`]).
//!
//! - The passes run at `angle` degrees from X, back and forth, spread
//!   evenly across the boundary at most the stepover apart, the first and
//!   the last on its edges. The boundary is where the tool's center goes
//!   (the box round the bodies when none is given), its islands left out.
//! - Along a pass the tool is dropped at points at most a quarter of its
//!   radius apart, and between two points again at the middle while the
//!   middle is farther than the tolerance from the line between them (to
//!   1/256 of the first spacing); the points the line passes within the
//!   tolerance of are then left out. Where nothing is under the tool it
//!   runs at the bottom.
//! - **Stock to leave:** the tool is dropped grown by it all round and
//!   raised by it, so it stays that far off the surfaces.
//! - **Slope:** with `max_slope` only the stretches where the surface the
//!   tool rests on is at most that steep are cut (the steep walls are
//!   left to waterline finishing); the tool goes up to the retract height
//!   over the rest.
//! - The end of a pass is joined to the start of the next along the
//!   surface when they are at most two stepovers apart and the way stays
//!   inside the boundary (and on surfaces flat enough); else the tool goes
//!   up to the retract height and comes down at the plunge feed.

use serde::{Deserialize, Serialize};

use super::Heights;
use super::path::Area;
use super::surface::{Surface, SurfaceCutter};
use super::zlevel::{TOLERANCE, check_slope, check_surface_op, polygon, simplified, travel};
use crate::CamError;
use crate::geom::{Contour, Region};
use crate::tool::{Cutting, Tool};
use crate::toolpath::{PathBuilder, Toolpath};

/// A raster finishing operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RasterOp {
    /// Where the tool's center goes; the box round the bodies when none.
    #[serde(default)]
    pub boundary: Option<Region>,
    /// The passes' direction (degrees from X, counter-clockwise).
    #[serde(default)]
    pub angle: f64,
    /// The distance between passes (mm), at most the tool's diameter;
    /// `None`: a tenth of the diameter.
    #[serde(default)]
    pub stepover: Option<f64>,
    /// Cut only where the surface is at most this steep (degrees from
    /// the horizontal); every slope when none.
    #[serde(default)]
    pub max_slope: Option<f64>,
    /// The stock's top: the retract height is above it.
    pub top: f64,
    /// The lowest the tip goes, where nothing is under the tool.
    pub bottom: f64,
    pub heights: Heights,
    #[serde(default)]
    pub stock_to_leave: f64,
    pub cutting: Cutting,
    /// How far the path may be from the exact one between its points
    /// (mm); 0.01 when none.
    #[serde(default)]
    pub tolerance: Option<f64>,
}

impl RasterOp {
    pub fn stepover_for(&self, tool: &Tool) -> f64 {
        self.stepover.unwrap_or(tool.diameter * 0.1)
    }
}

/// A point of a profile: how far along it, where, the tip's height and
/// whether it is cut (the surface flat enough).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    u: f64,
    xy: [f64; 2],
    z: f64,
    keep: bool,
}

/// The tool dropped along straight lines.
struct Probe<'a> {
    surface: &'a Surface,
    cutter: SurfaceCutter,
    lift: f64,
    floor: f64,
    max_slope: Option<f64>,
    tolerance: f64,
    spacing: f64,
}

impl Probe<'_> {
    fn point(&self, a: [f64; 2], d: [f64; 2], u: f64) -> Point {
        let xy = [a[0] + d[0] * u, a[1] + d[1] * u];
        let (z, slope) = match self.surface.drop_cutter(&self.cutter, xy) {
            Some(c) if c.z + self.lift > self.floor => (c.z + self.lift, c.slope()),
            _ => (self.floor, 0.0),
        };
        Point {
            u,
            xy,
            z,
            keep: self.max_slope.is_none_or(|m| slope <= m + 1e-9),
        }
    }

    /// The points of the line from `a` to `b` (see the module
    /// documentation), its ends included.
    fn profile(&self, a: [f64; 2], b: [f64; 2]) -> Vec<Point> {
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let d = if len > 0.0 {
            [(b[0] - a[0]) / len, (b[1] - a[1]) / len]
        } else {
            [0.0, 0.0]
        };
        let n = ((len / self.spacing - 1e-9).ceil() as usize).max(1);
        let base: Vec<Point> = (0..=n)
            .map(|i| self.point(a, d, len * i as f64 / n as f64))
            .collect();
        let mut out = vec![base[0]];
        for w in base.windows(2) {
            self.refine(a, d, w[0], w[1], 0, &mut out);
        }
        out
    }

    /// Pushes the points after `lo` up to `hi`, the middle again where the
    /// line from `lo` to `hi` is off by more than the tolerance or the
    /// slope's verdict changes.
    fn refine(
        &self,
        a: [f64; 2],
        d: [f64; 2],
        lo: Point,
        hi: Point,
        depth: u32,
        out: &mut Vec<Point>,
    ) {
        if depth < 8 {
            let mid = self.point(a, d, (lo.u + hi.u) / 2.0);
            let off = (mid.z - (lo.z + hi.z) / 2.0).abs();
            if off > self.tolerance || mid.keep != lo.keep || mid.keep != hi.keep {
                self.refine(a, d, lo, mid, depth + 1, out);
                self.refine(a, d, mid, hi, depth + 1, out);
                return;
            }
        }
        out.push(hi);
    }
}

/// The stretches of a profile that are cut, each drawn within the
/// tolerance.
fn runs(points: &[Point], tolerance: f64) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < points.len() {
        if !points[k].keep {
            k += 1;
            continue;
        }
        let start = k;
        while k < points.len() && points[k].keep {
            k += 1;
        }
        let run = &points[start..k];
        if run.len() >= 2 {
            let shape: Vec<[f64; 2]> = run.iter().map(|p| [p.u, p.z]).collect();
            out.push(
                simplified(&shape, tolerance)
                    .into_iter()
                    .map(|i| run[i])
                    .collect(),
            );
        }
    }
    out
}

/// Where the passes at `v` across the pass direction cross the boundary's
/// polygons (in pass coordinates), as the stretches inside it: even-odd,
/// each edge counted on its upper side only.
fn stretches(polygons: &[Vec<[f64; 2]>], v: f64) -> Vec<(f64, f64)> {
    let mut ts = Vec::new();
    for poly in polygons {
        for k in 0..poly.len() {
            let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
            if (a[1] > v) != (b[1] > v) {
                ts.push(a[0] + (v - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.as_chunks::<2>()
        .0
        .iter()
        .filter(|w| w[1] - w[0] > 1e-9)
        .map(|w| (w[0], w[1]))
        .collect()
}

/// The toolpath of a raster finishing.
pub fn raster(op: &RasterOp, surface: &Surface, tool: &Tool) -> Result<Toolpath, CamError> {
    let cutter = SurfaceCutter::of_tool(tool)?;
    op.cutting.validate().map_err(CamError::new)?;
    op.heights.validate(op.top)?;
    if !(op.bottom.is_finite() && op.bottom <= op.top) {
        return Err(CamError::new("The bottom must be below the top."));
    }
    let tolerance = op.tolerance.unwrap_or(TOLERANCE);
    check_surface_op(surface, tolerance, op.stock_to_leave, op.heights.retract)?;
    check_slope(op.max_slope)?;
    if !op.angle.is_finite() {
        return Err(CamError::new("The angle must be a number."));
    }
    let stepover = op.stepover_for(tool);
    if !(stepover.is_finite() && stepover > 0.0) {
        return Err(CamError::new("The stepover must be more than zero."));
    }
    if stepover > tool.diameter + 1e-9 {
        return Err(CamError::new(
            "The stepover must not be more than the tool's diameter.",
        ));
    }
    let (smin, smax) = surface.bounds();
    let boundary = match &op.boundary {
        Some(region) => {
            region.validate().map_err(CamError::new)?;
            region.clone()
        }
        None => Region::new(
            Contour::rectangle([smin[0], smin[1]], [smax[0], smax[1]]),
            Vec::new(),
        ),
    };
    let area = Area::of_region(&boundary);
    let (sin, cos) = op.angle.to_radians().sin_cos();
    // Pass coordinates: t along the passes, v across them.
    let to_pass = |p: [f64; 2]| [p[0] * cos + p[1] * sin, -p[0] * sin + p[1] * cos];
    let to_xy = |t: f64, v: f64| [t * cos - v * sin, t * sin + v * cos];
    let polygons: Vec<Vec<[f64; 2]>> = std::iter::once(&boundary.outer)
        .chain(&boundary.islands)
        .map(|c| {
            polygon(c, tolerance / 10.0)
                .into_iter()
                .map(to_pass)
                .collect()
        })
        .collect();
    let (vmin, vmax) = polygons
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
    let passes = ((vmax - vmin) / stepover - 1e-9).ceil().max(1.0);
    if passes > 100_000.0 {
        return Err(CamError::new("The boundary is too wide for the stepover."));
    }
    let passes = passes as usize;
    let probe = Probe {
        surface,
        cutter: cutter.grown(op.stock_to_leave),
        lift: op.stock_to_leave,
        floor: op.bottom,
        max_slope: op.max_slope,
        tolerance,
        spacing: (cutter.radius / 4.0).clamp(1e-3, 1.0),
    };
    let feed = op.cutting.feed;
    let mut b = PathBuilder::new();
    // The end of the last run: its pass and point.
    let mut last: Option<(usize, Point)> = None;
    for k in 0..=passes {
        let v = vmin + (vmax - vmin) * k as f64 / passes as f64;
        let edge = 1e-9_f64.min((vmax - vmin) / 4.0);
        let mut pieces = stretches(&polygons, v.clamp(vmin + edge, vmax - edge));
        let forward = k.is_multiple_of(2);
        if !forward {
            pieces.reverse();
        }
        let mut first_in_pass = true;
        for (t0, t1) in pieces {
            let (from, to) = if forward { (t0, t1) } else { (t1, t0) };
            let points = probe.profile(to_xy(from, v), to_xy(to, v));
            for run in runs(&points, tolerance) {
                let start = run[0];
                // Joined to the last pass's end along the surface, or up
                // and over.
                let joined = match last {
                    Some((pass, end))
                        if pass + 1 == k
                            && first_in_pass
                            && (end.xy[0] - start.xy[0]).hypot(end.xy[1] - start.xy[1])
                                <= 2.0 * stepover + 1e-9 =>
                    {
                        let link = probe.profile(end.xy, start.xy);
                        let inside = link[1..link.len() - 1]
                            .iter()
                            .all(|p| p.keep && area.contains(p.xy));
                        if inside && link.iter().all(|p| p.keep) {
                            for p in &link[1..] {
                                b.line([p.xy[0], p.xy[1], p.z], feed);
                            }
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if !joined {
                    if b.at().is_none() {
                        travel(&mut b, start.xy, op.heights.clearance);
                    }
                    travel(&mut b, start.xy, op.heights.retract);
                    b.line([start.xy[0], start.xy[1], start.z], op.cutting.plunge_feed);
                }
                for p in &run[1..] {
                    b.line([p.xy[0], p.xy[1], p.z], feed);
                }
                last = Some((k, *run.last().expect("a run has points")));
                first_in_pass = false;
            }
        }
    }
    let Some(end) = b.at() else {
        return Err(CamError::new(
            "The raster finishing has nothing to cut within its boundary.",
        ));
    };
    b.rapid([end[0], end[1], op.heights.clearance]);
    Ok(b.finish())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::collision::TriMesh;
    use crate::tool::ToolShape;
    use crate::toolpath::{Move, P3};

    /// The upper half of a sphere of `radius` about `center`, its rings
    /// at the polar angles `phis` (from 0, the pole, to 90 degrees, in
    /// radians), `segments` round; every corner on the sphere.
    pub(crate) fn hemisphere(center: P3, radius: f64, phis: &[f64], segments: usize) -> TriMesh {
        let mut mesh = TriMesh::default();
        for &phi in phis {
            for k in 0..segments {
                let theta = std::f64::consts::TAU * k as f64 / segments as f64;
                mesh.vertices.push([
                    center[0] + radius * phi.sin() * theta.cos(),
                    center[1] + radius * phi.sin() * theta.sin(),
                    center[2] + radius * phi.cos(),
                ]);
            }
        }
        let at = |i: usize, k: usize| (i * segments + k % segments) as u32;
        for i in 0..phis.len() - 1 {
            for k in 0..segments {
                mesh.triangles
                    .push([at(i, k), at(i + 1, k), at(i + 1, k + 1)]);
                mesh.triangles
                    .push([at(i, k), at(i + 1, k + 1), at(i, k + 1)]);
            }
        }
        mesh
    }

    pub(crate) fn ball(number: u32, diameter: f64) -> Tool {
        let mut t = Tool::flat_end_mill(number, diameter);
        t.shape = ToolShape::BallEndMill;
        t
    }

    pub(crate) fn cutting() -> Cutting {
        Cutting {
            spindle_rpm: 18000.0,
            feed: 1500.0,
            plunge_feed: 400.0,
        }
    }

    /// The tip's height where a path's cutting moves cross `x` at `y`.
    fn height_at(p: &Toolpath, x: f64, y: f64) -> Vec<f64> {
        let mut out = Vec::new();
        let mut at: Option<P3> = None;
        for m in &p.moves {
            let to = m.to();
            if let (Some(from), Move::Line { .. }) = (at, m)
                && (from[1] - y).abs() < 1e-9
                && (to[1] - y).abs() < 1e-9
                && (from[0] < x) != (to[0] < x)
            {
                let t = (x - from[0]) / (to[0] - from[0]);
                out.push(from[2] + (to[2] - from[2]) * t);
            }
            at = Some(to);
        }
        out
    }

    /// A ball end mill's passes 1 mm apart either side of a sphere's top
    /// leave the scallop of two balls on a sphere: the passes rest on the
    /// sphere where their balls touch a corner of the mesh (the ring at
    /// the passes' contact), and the material between them is cut down to
    /// where the two balls meet.
    #[test]
    fn raster_over_a_hemisphere_leaves_the_expected_scallop() {
        let (s, r, p) = (20.0_f64, 3.0_f64, 1.0_f64);
        let center = [0.0, 0.0, -s];
        // The ring whose corners at 90 and 270 degrees the passes touch.
        let contact = (p / 2.0 / (s + r)).asin();
        let mut phis: Vec<f64> = (0..=36).map(|i| (i as f64 * 2.5).to_radians()).collect();
        phis.push(contact);
        phis.sort_by(f64::total_cmp);
        let mesh = hemisphere(center, s, &phis, 72);
        let surface = Surface::new(&mesh).unwrap();
        let op = RasterOp {
            boundary: Some(Region::new(
                Contour::rectangle([-2.0, -p / 2.0], [2.0, p / 2.0]),
                Vec::new(),
            )),
            angle: 0.0,
            stepover: Some(p),
            max_slope: None,
            top: 0.0,
            bottom: -s,
            heights: Heights {
                clearance: 10.0,
                retract: 5.0,
            },
            stock_to_leave: 0.0,
            cutting: cutting(),
            tolerance: Some(1e-7),
        };
        let path = raster(&op, &surface, &ball(1, 2.0 * r)).unwrap();
        let below = height_at(&path, 0.0, -p / 2.0);
        let above = height_at(&path, 0.0, p / 2.0);
        assert_eq!((below.len(), above.len()), (1, 1), "one pass each side");
        // The balls' centers on the sphere grown by the ball's radius.
        let want = center[2] + ((s + r).powi(2) - (p / 2.0).powi(2)).sqrt();
        for z in [below[0], above[0]] {
            assert!((z + r - want).abs() < 1e-6, "{} not {want}", z + r);
        }
        // Where the two balls meet above the sphere's top (z = 0).
        let cusp = (below[0] + above[0]) / 2.0 + r - (r * r - (p / 2.0).powi(2)).sqrt();
        let scallop = (s + r).powi(2) - (p / 2.0).powi(2);
        let expected = scallop.sqrt() - (r * r - (p / 2.0).powi(2)).sqrt() - s;
        assert!((cusp - expected).abs() < 1e-6, "{cusp} not {expected}");
        // Lower than on a flat surface: the sphere falls away under it.
        assert!(expected < r - (r * r - (p / 2.0).powi(2)).sqrt() - 1e-3);
        // Every cutting point of the passes on or above the ball's offset
        // of the mesh: the path between its points within the tolerance.
        for m in &path.moves {
            if let Move::Line { to, .. } = m
                && to[2] < 4.0
            {
                let c = [to[0], to[1], to[2] + r];
                let d = ((c[0] - center[0]).powi(2)
                    + (c[1] - center[1]).powi(2)
                    + (c[2] - center[2]).powi(2))
                .sqrt();
                assert!(d <= s + r + 1e-6, "above the sphere at {to:?}");
            }
        }
    }

    /// With a slope limit only the top of the hemisphere is cut: where the
    /// surface is at most 30 degrees steep (the mesh's own facets, a ring
    /// of 2.5 degrees off).
    #[test]
    fn raster_keeps_to_the_shallow_surfaces() {
        let (s, r) = (20.0_f64, 3.0_f64);
        let center = [0.0, 0.0, -s];
        let phis: Vec<f64> = (0..=36).map(|i| (i as f64 * 2.5).to_radians()).collect();
        let surface = Surface::new(&hemisphere(center, s, &phis, 72)).unwrap();
        // Within the hemisphere's outline: the ball always rests on it.
        let mut op = RasterOp {
            boundary: Some(Region::new(Contour::circle([0.0, 0.0], s), Vec::new())),
            angle: 30.0,
            stepover: Some(2.0),
            max_slope: Some(30.0),
            top: 0.0,
            bottom: -s,
            heights: Heights {
                clearance: 10.0,
                retract: 5.0,
            },
            stock_to_leave: 0.5,
            cutting: cutting(),
            tolerance: None,
        };
        let path = raster(&op, &surface, &ball(1, 2.0 * r)).unwrap();
        let mut nearest = f64::INFINITY;
        let mut farthest: f64 = 0.0;
        for m in &path.moves {
            if let Move::Line { to, .. } = m
                && to[2] < 4.0
            {
                let d = to[0].hypot(to[1]);
                nearest = nearest.min(d);
                farthest = farthest.max(d);
                // The tip `stock` off the sphere grown by the ball.
                let rho = ((s + r + 0.5).powi(2) - d * d).sqrt();
                assert!(to[2] + r <= center[2] + rho + 1e-6);
            }
        }
        let limit = (s + r + 0.5) * 35.0_f64.to_radians().sin();
        assert!(
            nearest < 2.0 && farthest > 10.0 && farthest < limit,
            "{nearest} {farthest}"
        );
        // Every slope: the passes run out to the hemisphere's edge.
        op.max_slope = None;
        let path = raster(&op, &surface, &ball(1, 2.0 * r)).unwrap();
        let reach = path
            .moves
            .iter()
            .filter(|m| matches!(m, Move::Line { .. }))
            .map(|m| m.to()[0].hypot(m.to()[1]))
            .fold(0.0, f64::max);
        assert!(reach > s - 1.0, "{reach}");
    }

    #[test]
    fn raster_refuses_what_it_cannot_cut() {
        let surface = Surface::new(&hemisphere([0.0; 3], 5.0, &[0.0, 0.5, 1.0], 12)).unwrap();
        let op = RasterOp {
            boundary: None,
            angle: 0.0,
            stepover: None,
            max_slope: None,
            top: 5.0,
            bottom: 0.0,
            heights: Heights {
                clearance: 10.0,
                retract: 5.0,
            },
            stock_to_leave: 0.0,
            cutting: cutting(),
            tolerance: None,
        };
        // The sphere's top is at the retract height.
        let e = raster(&op, &surface, &ball(1, 4.0)).unwrap_err();
        assert!(e.message.contains("retract"), "{e}");
        let mut drill = ball(2, 4.0);
        drill.shape = ToolShape::Drill { point_angle: 118.0 };
        assert!(raster(&op, &surface, &drill).is_err());
    }
}
