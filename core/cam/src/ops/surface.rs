// SPDX-License-Identifier: MIT
//! Freeform surfaces for 3-axis machining (docs/cam.md, "3-axis
//! machining"): the bodies' triangle meshes in the work coordinates,
//! indexed in XY, and the drop-cutter, which finds how low a cutter can
//! come down at a place before it touches the surface.
//!
//! **Cutters.** A flat, ball or bull nose end mill is a cylinder of its
//! radius whose bottom edge is rounded by its corner radius ([`SurfaceCutter`]):
//! a flat end mill's corner is zero, a ball's is the radius. Its end is
//! the flat disc of `radius - corner` at the tip and the torus of the
//! corner round it.
//!
//! **The drop-cutter** is exact for the triangles: the cutter comes down
//! its axis and stops at the highest of its contacts with each triangle's
//! corners, edges and inside ([`Surface::drop_cutter`]):
//!
//! - a corner at `d` from the axis touches the end where the end is that
//!   high (`SurfaceCutter::height_at`);
//! - the inside of a triangle touches the cutter where the cutter's normal
//!   is the triangle's: the corner circle's point toward the uphill side
//!   and down the triangle's normal by the corner radius, in closed form;
//!   it counts when that point is inside the triangle;
//! - along an edge the tip's height `z(s) - h(d(s))` is concave in the
//!   edge's parameter (the end's height is convex and growing in the
//!   distance, which is convex along a line), so its highest point is the
//!   only one: in closed form for a ball (a cylinder round the edge) and a
//!   flat end mill (the end of the chord the disc cuts), by golden section
//!   for a bull nose (to 1e-13 of the edge's parameter).
//!
//! Each contact gives the surface's normal there (the triangle's, or the
//! cutter's at the contact turned round), so that the finishing operations
//! can split their work by the surface's slope.

use serde::{Deserialize, Serialize};

use crate::CamError;
use crate::collision::TriMesh;
use crate::tool::{Tool, ToolShape};
use crate::toolpath::P3;

/// The end of a milling cutter: a cylinder of `radius` whose bottom edge
/// is rounded by `corner` (0 for a flat end mill, the radius for a ball).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceCutter {
    pub radius: f64,
    pub corner: f64,
}

/// How far beyond the radius a point still counts as on the rim (mm).
const RIM: f64 = 1e-9;

impl SurfaceCutter {
    pub fn flat(radius: f64) -> SurfaceCutter {
        SurfaceCutter {
            radius,
            corner: 0.0,
        }
    }

    pub fn ball(radius: f64) -> SurfaceCutter {
        SurfaceCutter {
            radius,
            corner: radius,
        }
    }

    pub fn bull(radius: f64, corner: f64) -> SurfaceCutter {
        SurfaceCutter { radius, corner }
    }

    /// The cutter of a flat, ball or bull nose end mill.
    pub fn of_tool(tool: &Tool) -> Result<SurfaceCutter, CamError> {
        tool.validate().map_err(CamError::new)?;
        match tool.shape {
            ToolShape::FlatEndMill => Ok(SurfaceCutter::flat(tool.radius())),
            ToolShape::BallEndMill => Ok(SurfaceCutter::ball(tool.radius())),
            ToolShape::BullNose { corner_radius } => {
                Ok(SurfaceCutter::bull(tool.radius(), corner_radius))
            }
            ToolShape::Drill { .. } | ToolShape::VBit { .. } => Err(CamError::new(
                "The operation needs a flat, ball or bull nose end mill.",
            )),
        }
    }

    /// The cutter grown by `by` all round: its surface `by` farther out,
    /// its tip `by` lower. A tip of the grown cutter at a height puts the
    /// cutter's own tip `by` higher, `by` off the surface the grown one
    /// touches (the stock to leave).
    pub fn grown(&self, by: f64) -> SurfaceCutter {
        SurfaceCutter {
            radius: self.radius + by,
            corner: self.corner + by,
        }
    }

    /// The radius of the flat disc of the end.
    pub fn flat_radius(&self) -> f64 {
        (self.radius - self.corner).max(0.0)
    }

    fn is_ball(&self) -> bool {
        self.corner >= self.radius - 1e-12
    }

    fn is_flat(&self) -> bool {
        self.corner <= 1e-12
    }

    /// How far above the tip the end is at `d` from the axis; none
    /// beyond the radius.
    pub fn height_at(&self, d: f64) -> Option<f64> {
        if d.is_nan() || d > self.radius + RIM {
            return None;
        }
        let a = self.flat_radius();
        if d <= a {
            return Some(0.0);
        }
        let r = self.corner;
        let e = (d - a).min(r);
        Some(r - (r * r - e * e).max(0.0).sqrt())
    }

    pub fn validate(&self) -> Result<(), CamError> {
        if !(self.radius.is_finite() && self.radius > 0.0) {
            return Err(CamError::new("The tool has no diameter."));
        }
        if !(self.corner.is_finite() && self.corner >= 0.0 && self.corner <= self.radius + 1e-12) {
            return Err(CamError::new(
                "The tool's corner radius must be within its radius.",
            ));
        }
        Ok(())
    }
}

/// Where a dropped cutter rests: the tip's height and the surface's
/// normal at the contact (a unit vector, up out of the part).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub z: f64,
    pub normal: [f64; 3],
}

impl Contact {
    /// The angle of the surface touched from the horizontal (degrees).
    pub fn slope(&self) -> f64 {
        self.normal[2].clamp(-1.0, 1.0).acos().to_degrees()
    }
}

#[derive(Clone, Debug)]
struct Facet {
    p: [P3; 3],
    /// The unit normal turned up; none for a triangle without area.
    n: Option<[f64; 3]>,
    lo: [f64; 2],
    hi: [f64; 2],
    top: f64,
}

/// A triangle mesh in the work coordinates, indexed in XY for the
/// drop-cutter.
#[derive(Clone, Debug)]
pub struct Surface {
    facets: Vec<Facet>,
    min: P3,
    max: P3,
    cell: f64,
    nx: usize,
    ny: usize,
    bins: Vec<Vec<u32>>,
}

fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

impl Surface {
    /// The surface of a mesh's triangles (those with an index past its
    /// points are left out).
    pub fn new(mesh: &TriMesh) -> Result<Surface, CamError> {
        let mut facets = Vec::new();
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for t in &mesh.triangles {
            if t.iter().any(|&i| i as usize >= mesh.vertices.len()) {
                continue;
            }
            let p = t.map(|i| mesh.vertices[i as usize]);
            if p.iter().flatten().any(|v| !v.is_finite()) {
                return Err(CamError::new(
                    "A surface of the operation has a point that is not a number.",
                ));
            }
            let c = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            let n = (len > 1e-18).then(|| {
                let s = if c[2] < 0.0 { -1.0 / len } else { 1.0 / len };
                [c[0] * s, c[1] * s, c[2] * s]
            });
            let mut lo = [f64::INFINITY; 2];
            let mut hi = [f64::NEG_INFINITY; 2];
            let mut top = f64::NEG_INFINITY;
            for q in &p {
                for k in 0..2 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
                top = top.max(q[2]);
                for k in 0..3 {
                    min[k] = min[k].min(q[k]);
                    max[k] = max[k].max(q[k]);
                }
            }
            facets.push(Facet { p, n, lo, hi, top });
        }
        if facets.is_empty() {
            return Err(CamError::new(
                "The operation's bodies have no surfaces to machine.",
            ));
        }
        let ex = (max[0] - min[0]).max(1e-9);
        let ey = (max[1] - min[1]).max(1e-9);
        let cell = (2.0 * (ex * ey / facets.len() as f64).sqrt())
            .max(ex.max(ey) / 1024.0)
            .max(1e-6);
        let nx = (ex / cell).floor() as usize + 1;
        let ny = (ey / cell).floor() as usize + 1;
        let mut bins: Vec<Vec<u32>> = vec![Vec::new(); nx * ny];
        let mut surface = Surface {
            facets: Vec::new(),
            min,
            max,
            cell,
            nx,
            ny,
            bins: Vec::new(),
        };
        for (index, f) in facets.iter().enumerate() {
            let (i0, j0) = surface.bin_of(f.lo);
            let (i1, j1) = surface.bin_of(f.hi);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    bins[j * nx + i].push(index as u32);
                }
            }
        }
        surface.facets = facets;
        surface.bins = bins;
        Ok(surface)
    }

    fn bin_of(&self, p: [f64; 2]) -> (usize, usize) {
        let i = ((p[0] - self.min[0]) / self.cell).floor();
        let j = ((p[1] - self.min[1]) / self.cell).floor();
        (
            (i.max(0.0) as usize).min(self.nx - 1),
            (j.max(0.0) as usize).min(self.ny - 1),
        )
    }

    /// The box round the triangles: `(min, max)`.
    pub fn bounds(&self) -> (P3, P3) {
        (self.min, self.max)
    }

    /// The heights of the level triangles (the floors of the part),
    /// highest first, each once.
    pub fn flats(&self) -> Vec<f64> {
        let mut out: Vec<f64> = self
            .facets
            .iter()
            .filter(|f| f.n.is_some_and(|n| n[2] > 1.0 - 1e-9))
            .map(|f| (f.p[0][2] + f.p[1][2] + f.p[2][2]) / 3.0)
            .collect();
        out.sort_by(|a, b| b.total_cmp(a));
        out.dedup_by(|a, b| (*a - *b).abs() <= 1e-6);
        out
    }

    /// The triangles whose boxes meet the box `[lo, hi]` in XY.
    fn candidates(&self, lo: [f64; 2], hi: [f64; 2]) -> Vec<u32> {
        let (i0, j0) = self.bin_of(lo);
        let (i1, j1) = self.bin_of(hi);
        let mut out = Vec::new();
        for j in j0..=j1 {
            for i in i0..=i1 {
                out.extend_from_slice(&self.bins[j * self.nx + i]);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Where `cutter` comes to rest when it comes down its axis at `xy`:
    /// the highest of its contacts with the triangles' corners, edges and
    /// insides; none when nothing is under it.
    pub fn drop_cutter(&self, cutter: &SurfaceCutter, xy: [f64; 2]) -> Option<Contact> {
        let [x, y] = xy;
        let rr = cutter.radius + RIM;
        if x + rr < self.min[0]
            || x - rr > self.max[0]
            || y + rr < self.min[1]
            || y - rr > self.max[1]
        {
            return None;
        }
        let mut best: Option<Contact> = None;
        for index in self.candidates([x - rr, y - rr], [x + rr, y + rr]) {
            let f = &self.facets[index as usize];
            if f.hi[0] < x - rr || f.lo[0] > x + rr || f.hi[1] < y - rr || f.lo[1] > y + rr {
                continue;
            }
            // No contact with a triangle can be above its highest corner.
            if best.is_some_and(|b| f.top < b.z - TIE) {
                continue;
            }
            for k in 0..3 {
                better(&mut best, vertex_contact(cutter, xy, f.p[k]));
                better(
                    &mut best,
                    edge_contact(cutter, xy, f.p[k], f.p[(k + 1) % 3]),
                );
            }
            better(&mut best, facet_contact(cutter, xy, f));
        }
        best
    }
}

/// Contacts this close in height are the same height (mm).
const TIE: f64 = 1e-12;

/// Of two contacts at the same height, one is steeper when the Z of its
/// normal is this much smaller: a bull nose's edge contact a micrometre
/// into its corner rises as little as the tie, and is the flat's.
const STEEPER: f64 = 1e-9;

/// Keeps the higher contact; of two at the same height the steeper surface
/// (a flat end mill's rim on a wall's top edge rather than its flat on the
/// top beside it, so that a waterline along the wall sees the wall); of
/// two as steep, the one whose normal points more nearly up (a facet's
/// exact normal rather than an edge's contact found a hair into a bull
/// nose's corner).
fn better(best: &mut Option<Contact>, found: Option<Contact>) {
    if let Some(c) = found
        && best.is_none_or(|b| {
            let tie = (c.z - b.z).abs() <= TIE;
            let steeper = c.normal[2] < b.normal[2] - STEEPER;
            let as_steep = (c.normal[2] - b.normal[2]).abs() <= STEEPER;
            c.z > b.z + TIE || (tie && (steeper || (as_steep && c.normal[2] > b.normal[2])))
        })
    {
        *best = Some(c);
    }
}

/// How far inside a flat end mill's rim a contact still counts as on the
/// rim (mm): the waterlines' points are found that close to it.
const RIM_BAND: f64 = 1e-6;

/// The surface's normal at a contact point `p`, `d` from the axis at `xy`,
/// with the tip at `tip`: up where the flat of the end touches, else from
/// the contact to the center of the corner's circle there (level, toward
/// the axis, on a flat end mill's sharp rim: a wall it runs along).
fn contact_normal(c: &SurfaceCutter, xy: [f64; 2], p: P3, d: f64, tip: f64) -> [f64; 3] {
    let a = c.flat_radius();
    let sharp = c.corner <= 1e-12;
    let on_flat = if sharp {
        d < c.radius - RIM_BAND
    } else {
        d <= a + 1e-12
    };
    if on_flat || d < 1e-15 {
        return [0.0, 0.0, 1.0];
    }
    let ux = (p[0] - xy[0]) / d;
    let uy = (p[1] - xy[1]) / d;
    if sharp {
        return [-ux, -uy, 0.0];
    }
    let q = [xy[0] + a * ux, xy[1] + a * uy, tip + c.corner];
    let v = sub(q, p);
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-15 {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

/// The cutter on a corner of a triangle.
fn vertex_contact(c: &SurfaceCutter, xy: [f64; 2], v: P3) -> Option<Contact> {
    let d = (v[0] - xy[0]).hypot(v[1] - xy[1]);
    let h = c.height_at(d)?;
    let tip = v[2] - h;
    Some(Contact {
        z: tip,
        normal: contact_normal(c, xy, v, d, tip),
    })
}

/// The cutter on the inside of a triangle (see the module documentation).
fn facet_contact(c: &SurfaceCutter, xy: [f64; 2], f: &Facet) -> Option<Contact> {
    let n = f.n?;
    if n[2] < 1e-9 {
        return None;
    }
    let s = n[0].hypot(n[1]);
    let a = c.flat_radius();
    let r = c.corner;
    // Uphill, where the end touches a tilted plane first.
    let (ux, uy) = if s > 1e-12 {
        (-n[0] / s, -n[1] / s)
    } else {
        (0.0, 0.0)
    };
    let plane = n[0] * f.p[0][0] + n[1] * f.p[0][1] + n[2] * f.p[0][2];
    let z = (plane - n[0] * xy[0] - n[1] * xy[1] + a * s + r * (1.0 - n[2])) / n[2];
    let cc = [xy[0] + a * ux - r * n[0], xy[1] + a * uy - r * n[1]];
    inside_triangle(&f.p, cc).then_some(Contact { z, normal: n })
}

/// Whether `q` is inside the triangle seen from above (its edges count).
fn inside_triangle(p: &[P3; 3], q: [f64; 2]) -> bool {
    let side = |a: P3, b: P3| (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0]);
    let d0 = side(p[0], p[1]);
    let d1 = side(p[1], p[2]);
    let d2 = side(p[2], p[0]);
    let tol = 1e-12;
    (d0 >= -tol && d1 >= -tol && d2 >= -tol) || (d0 <= tol && d1 <= tol && d2 <= tol)
}

/// The cutter on an edge from `p0` to `p1` (its ends are corners, found
/// on their own): the highest point of the concave tip height along the
/// edge inside it.
fn edge_contact(c: &SurfaceCutter, xy: [f64; 2], p0: P3, p1: P3) -> Option<Contact> {
    let ex = p1[0] - p0[0];
    let ey = p1[1] - p0[1];
    let lh = ex.hypot(ey);
    if lh < 1e-12 {
        return None;
    }
    let (ux, uy) = (ex / lh, ey / lh);
    let ax = xy[0] - p0[0];
    let ay = xy[1] - p0[1];
    // Along the edge from p0 to the foot of the axis, and across to it.
    let s0 = ax * ux + ay * uy;
    let w = (ax * uy - ay * ux).abs();
    let reach = c.radius + RIM;
    if w > reach {
        return None;
    }
    let rho = (reach * reach - w * w).max(0.0).sqrt();
    let lo = (s0 - rho).max(0.0);
    let hi = (s0 + rho).min(lh);
    if lo > hi {
        return None;
    }
    let m = (p1[2] - p0[2]) / lh;
    let tip_at = |s: f64| {
        let ds = s - s0;
        c.height_at((w * w + ds * ds).sqrt())
            .map_or(f64::NEG_INFINITY, |h| p0[2] + m * s - h)
    };
    let s = if c.is_ball() {
        // The center of the ball on the cylinder of its radius round the
        // edge: highest where the slope along the edge meets the circle's.
        let rr = (c.radius * c.radius - w * w).max(0.0).sqrt();
        s0 + m * rr / (1.0 + m * m).sqrt()
    } else if c.is_flat() {
        // The disc's chord along the edge: its higher end.
        if m > 0.0 {
            s0 + rho
        } else if m < 0.0 {
            s0 - rho
        } else {
            s0.clamp(lo, hi)
        }
    } else {
        golden_max(lo, hi, tip_at)
    };
    if s.is_nan() || s < lo || s > hi {
        // The highest point is past an end of the edge: a corner's.
        return None;
    }
    let z = tip_at(s);
    if !z.is_finite() {
        return None;
    }
    let p = [p0[0] + ux * s, p0[1] + uy * s, p0[2] + m * s];
    let ds = s - s0;
    let d = (w * w + ds * ds).sqrt();
    Some(Contact {
        z,
        normal: contact_normal(c, xy, p, d, z),
    })
}

/// The highest point of a concave function on `[lo, hi]` by golden
/// section.
fn golden_max(mut lo: f64, mut hi: f64, f: impl Fn(f64) -> f64) -> f64 {
    const G: f64 = 0.618_033_988_749_894_9;
    let mut a = hi - G * (hi - lo);
    let mut b = lo + G * (hi - lo);
    let mut fa = f(a);
    let mut fb = f(b);
    for _ in 0..200 {
        if hi - lo <= 1e-13 * (1.0 + hi.abs().max(lo.abs())) {
            break;
        }
        if fa < fb {
            lo = a;
            a = b;
            fa = fb;
            b = lo + G * (hi - lo);
            fb = f(b);
        } else {
            hi = b;
            b = a;
            fb = fa;
            a = hi - G * (hi - lo);
            fa = f(a);
        }
    }
    let mut best = (lo, f(lo));
    for t in [a, b, hi] {
        let v = f(t);
        if v > best.1 {
            best = (t, v);
        }
    }
    best.0
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A mesh of the triangles given as corner triples.
    pub(crate) fn mesh_of(triangles: &[[P3; 3]]) -> TriMesh {
        let mut mesh = TriMesh::default();
        for t in triangles {
            let base = mesh.vertices.len() as u32;
            mesh.vertices.extend_from_slice(t);
            mesh.triangles.push([base, base + 1, base + 2]);
        }
        mesh
    }

    /// The plane `z = z0 + gx x + gy y` over `[-size, size]` squared, as
    /// two triangles.
    fn plane(z0: f64, gx: f64, gy: f64, size: f64) -> TriMesh {
        let p = |x: f64, y: f64| [x, y, z0 + gx * x + gy * y];
        mesh_of(&[
            [p(-size, -size), p(size, -size), p(size, size)],
            [p(-size, -size), p(size, size), p(-size, size)],
        ])
    }

    /// A sphere of `radius` about `center`, `rings` from pole to pole and
    /// `segments` round, every vertex on the sphere (the mesh inside it).
    pub(crate) fn sphere(center: P3, radius: f64, rings: usize, segments: usize) -> TriMesh {
        let mut mesh = TriMesh::default();
        let point = |i: usize, k: usize| {
            let phi = std::f64::consts::PI * i as f64 / rings as f64;
            let theta = std::f64::consts::TAU * k as f64 / segments as f64;
            [
                center[0] + radius * phi.sin() * theta.cos(),
                center[1] + radius * phi.sin() * theta.sin(),
                center[2] + radius * phi.cos(),
            ]
        };
        for i in 0..=rings {
            for k in 0..segments {
                mesh.vertices.push(point(i, k));
            }
        }
        let at = |i: usize, k: usize| (i * segments + k % segments) as u32;
        for i in 0..rings {
            for k in 0..segments {
                mesh.triangles
                    .push([at(i, k), at(i + 1, k), at(i + 1, k + 1)]);
                mesh.triangles
                    .push([at(i, k), at(i + 1, k + 1), at(i, k + 1)]);
            }
        }
        mesh
    }

    /// The tip's height of a cutter on a plane with the unit normal `n`
    /// through the origin's height `z0`, in closed form.
    fn on_plane(c: &SurfaceCutter, n: P3, z0: f64, xy: [f64; 2]) -> f64 {
        let s = n[0].hypot(n[1]);
        (n[2] * z0 - n[0] * xy[0] - n[1] * xy[1] + c.flat_radius() * s + c.corner * (1.0 - n[2]))
            / n[2]
    }

    fn cutters() -> [SurfaceCutter; 3] {
        [
            SurfaceCutter::flat(3.0),
            SurfaceCutter::ball(3.0),
            SurfaceCutter::bull(3.0, 1.0),
        ]
    }

    #[test]
    fn a_cutter_rests_on_a_tilted_plane_as_its_normal_says() {
        let (gx, gy) = (0.3, -0.2);
        let surface = Surface::new(&plane(1.5, gx, gy, 40.0)).unwrap();
        let len = (gx * gx + gy * gy + 1.0_f64).sqrt();
        let n = [-gx / len, -gy / len, 1.0 / len];
        for c in cutters() {
            for xy in [[0.0, 0.0], [7.3, -4.1], [-12.0, 9.5], [20.0, 20.0]] {
                let got = surface.drop_cutter(&c, xy).unwrap();
                let want = on_plane(&c, n, 1.5, xy);
                assert!(
                    (got.z - want).abs() < 1e-9,
                    "{c:?} at {xy:?}: {} not {want}",
                    got.z
                );
                assert!((got.slope() - n[2].acos().to_degrees()).abs() < 1e-6);
            }
        }
        // Level: every cutter's tip on the plane, its normal up.
        let surface = Surface::new(&plane(-2.0, 0.0, 0.0, 10.0)).unwrap();
        for c in cutters() {
            let got = surface.drop_cutter(&c, [1.0, 2.0]).unwrap();
            assert!((got.z + 2.0).abs() < 1e-12);
            assert!(got.slope() < 1e-9, "{c:?}: {got:?}");
        }
        // Beyond the plane's edge by more than the radius: nothing under it.
        assert!(
            surface
                .drop_cutter(&SurfaceCutter::flat(3.0), [13.1, 0.0])
                .is_none()
        );
        // Within the radius of the edge: the rim (flat) or the corner on the
        // edge.
        let z = surface
            .drop_cutter(&SurfaceCutter::ball(3.0), [12.0, 0.0])
            .unwrap()
            .z;
        assert!((z - (-2.0 - 3.0 + (9.0_f64 - 4.0).sqrt())).abs() < 1e-12);
        let z = surface
            .drop_cutter(&SurfaceCutter::flat(3.0), [12.5, 0.0])
            .unwrap()
            .z;
        assert!((z + 2.0).abs() < 1e-12);
    }

    /// A ball resting on a point is a sphere of its radius about the point,
    /// and a bull nose's tip over a point follows its torus.
    #[test]
    fn a_cutter_over_a_point_follows_its_end() {
        let v = [1.0, 2.0, 3.0];
        let mesh = mesh_of(&[[v, v, v]]);
        let surface = Surface::new(&mesh).unwrap();
        for c in cutters() {
            for d in [0.0, 0.5, 1.9, 2.5, 2.999] {
                let got = surface.drop_cutter(&c, [1.0 + d, 2.0]).unwrap().z;
                let want = 3.0 - c.height_at(d).unwrap();
                assert!((got - want).abs() < 1e-12, "{c:?} at {d}");
            }
            assert!(surface.drop_cutter(&c, [4.1, 2.0]).is_none());
        }
        let ball = surface
            .drop_cutter(&SurfaceCutter::ball(3.0), [3.0, 2.0])
            .unwrap()
            .z;
        assert!((ball - (3.0 - 3.0 + (9.0_f64 - 4.0).sqrt())).abs() < 1e-12);
    }

    /// Over a sloping edge the ball's center is on the cylinder of its
    /// radius round the edge; the bull nose's tip is the highest of the
    /// torus' heights along the edge.
    #[test]
    fn a_cutter_over_an_edge_rests_at_its_highest_point() {
        let (p0, p1) = ([-10.0, 0.0, 0.0], [10.0, 0.0, 5.0]);
        // A sliver under the edge: its inside is only the edge itself.
        let mesh = mesh_of(&[[p0, p1, [10.0, 0.0, 5.0 - 1e-12]]]);
        let surface = Surface::new(&mesh).unwrap();
        let (r, w) = (3.0_f64, 1.2_f64);
        // The ball: its center `r` from the edge's line, `w` across.
        let got = surface
            .drop_cutter(&SurfaceCutter::ball(r), [0.0, w])
            .unwrap()
            .z;
        let slope: f64 = 5.0 / 20.0;
        let along = 1.0 / (1.0 + slope * slope).sqrt();
        // The line at x: z = 2.5 + slope x; the center at height
        // z(x) + sqrt(r^2 - w^2) / cos(angle) above the line at x = 0.
        let want = 2.5 + (r * r - w * w).sqrt() / along - r;
        assert!((got - want).abs() < 1e-9, "{got} not {want}");
        // The bull nose against a brute-force search along the edge.
        let c = SurfaceCutter::bull(3.0, 1.0);
        let got = surface.drop_cutter(&c, [0.0, w]).unwrap().z;
        let mut want = f64::NEG_INFINITY;
        for i in 0..=2_000_000 {
            let x = -10.0 + 20.0 * i as f64 / 2_000_000.0;
            if let Some(h) = c.height_at(x.hypot(w)) {
                want = want.max(2.5 + slope * x - h);
            }
        }
        assert!(got >= want - 1e-12 && got - want < 1e-9, "{got} not {want}");
    }

    /// On a sphere's mesh (every corner on the sphere): where the cutter's
    /// corner circle points at a corner of the mesh, through the sphere's
    /// center, the cutter rests on the sphere exactly; elsewhere never
    /// above it, and below it by at most the mesh's deviation.
    #[test]
    fn a_cutter_rests_on_a_sphere() {
        let (center, radius) = ([2.0, -1.0, -20.0], 10.0);
        let (rings, segments) = (24, 48);
        let mesh = sphere(center, radius, rings, segments);
        let surface = Surface::new(&mesh).unwrap();
        // The largest distance of a triangle from the sphere (the sag of
        // the longest chord).
        let step = std::f64::consts::PI / rings as f64;
        let deviation = radius * (1.0 - (step / 2.0_f64.sqrt()).cos()) * 2.0;
        for c in cutters() {
            let a = c.flat_radius();
            let r = c.corner;
            // At the mesh's corners on the upper half.
            for i in 1..rings / 2 {
                for k in [0, 7, 19, 30] {
                    let v = mesh.vertices[i * segments + k];
                    let u = sub(v, center).map(|q| q / radius);
                    let horizontal = u[0].hypot(u[1]);
                    // The corner circle's center along the normal at v,
                    // and the axis `a` farther out.
                    let q = [
                        center[0] + (radius + r) * u[0],
                        center[1] + (radius + r) * u[1],
                        center[2] + (radius + r) * u[2],
                    ];
                    let xy = [q[0] + a * u[0] / horizontal, q[1] + a * u[1] / horizontal];
                    let got = surface.drop_cutter(&c, xy).unwrap().z;
                    let want = q[2] - r;
                    assert!(
                        (got - want).abs() < 1e-6,
                        "{c:?} at ring {i}: {got} not {want}"
                    );
                }
            }
            // Between the corners: within the deviation below the sphere.
            for xy in [[2.3, -0.4], [5.0, 3.0], [-4.0, -6.5]] {
                let got = surface.drop_cutter(&c, xy).unwrap().z;
                let dx = xy[0] - center[0];
                let dy = xy[1] - center[1];
                let d = dx.hypot(dy);
                let want = if d <= a {
                    center[2] + radius
                } else {
                    center[2] + ((radius + r).powi(2) - (d - a).powi(2)).sqrt() - r
                };
                assert!(got <= want + 1e-9, "{c:?} above the sphere at {xy:?}");
                assert!(got >= want - deviation, "{c:?} too low at {xy:?}");
            }
        }
    }

    #[test]
    fn only_milling_cutters_drop() {
        let mut t = Tool::flat_end_mill(1, 6.0);
        assert_eq!(
            SurfaceCutter::of_tool(&t).unwrap(),
            SurfaceCutter::flat(3.0)
        );
        t.shape = ToolShape::BallEndMill;
        assert_eq!(
            SurfaceCutter::of_tool(&t).unwrap(),
            SurfaceCutter::ball(3.0)
        );
        t.shape = ToolShape::BullNose { corner_radius: 0.5 };
        assert_eq!(
            SurfaceCutter::of_tool(&t).unwrap(),
            SurfaceCutter::bull(3.0, 0.5)
        );
        t.shape = ToolShape::Drill { point_angle: 118.0 };
        assert!(SurfaceCutter::of_tool(&t).is_err());
        assert!(Surface::new(&TriMesh::default()).is_err());
    }
}
