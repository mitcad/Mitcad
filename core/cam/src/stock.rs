// SPDX-License-Identifier: MIT
//! The stock as it is cut (docs/cam.md, "Stock simulation"): a height map
//! of the stock (a Z per node of a grid in the work plane, dexels standing
//! up from the stock's bottom), lowered by each move of a program swept by
//! the tool's cutting end.
//!
//! - **The grid** ([`Grid`]) spans the stock's box in X and Y with nodes on
//!   its edges, as fine as an eighth of the smallest tool's diameter
//!   ([`NODES_ACROSS_TOOL`]) and no finer than [`MIN_STEP`], coarsened until
//!   it has at most `max_nodes` ([`MAX_NODES`] by default). What lies between
//!   two nodes is not seen: a wall thinner than the grid's step may be
//!   missed, and every height is within the step of the true surface along
//!   a slope.
//! - **The tool** ([`Cutter`]) is its cutting end as a height above the tip
//!   at each distance from its axis: flat, a ball, a bull nose's rounded
//!   corner, a drill's point and a V-bit's cone (with its flat tip). Every
//!   such profile is convex and rises outwards, so the lowest the tool
//!   reaches over a node along a straight move is found exactly: at an end
//!   of the stretch where the node is under the tool (flat ends), at the
//!   point of the move nearest the node (level moves), else by a golden
//!   section search of the convex height along the stretch. Arcs are cut as
//!   chords within a quarter of the grid's step of the arc.
//! - **Rapids** move as the posts write them (up first when they rise,
//!   across first when they sink); a rapid that lowers the stock by more
//!   than [`RAPID_TOLERANCE`] cuts it, which [`simulate`] reports
//!   ([`RapidCut`]).
//! - **Remaining material** ([`remaining`]): the stock as cut against the
//!   part's own height map (the highest of its surface over each node,
//!   [`HeightMap::top_of_mesh`]): the thickness left over the part, the
//!   places of the thickest, and the gouges (the part cut into, measured
//!   against the lowest of each node and its neighbours, so that a node on
//!   a wall the tool runs along is no gouge).
//!
//! The crate's collision check ([`crate::check_collisions`]) runs the same
//! simulation alongside its sweep, so that the spindle meets the stock as
//! it is cut so far (`CollisionOptions::stock`).

use serde::{Deserialize, Serialize};

use crate::collision::TriMesh;
use crate::machine::Bounds;
use crate::program::Program;
use crate::tool::{Tool, ToolShape};
use crate::toolpath::{Move, P3, move_points};

/// The most nodes a grid has by default.
pub const MAX_NODES: usize = 1_000_000;

/// The finest step of a grid (mm).
pub const MIN_STEP: f64 = 0.01;

/// Nodes across the smallest tool's diameter.
pub const NODES_ACROSS_TOOL: f64 = 8.0;

/// A rapid that lowers the stock by more than this (mm) cuts it.
pub const RAPID_TOLERANCE: f64 = 0.001;

/// Heights are compared and written to the micrometre.
const MICRONS: f64 = 1000.0;

/// The nodes of a height map: `nx` by `ny` from `origin` (work X and Y of
/// node 0, 0), `step` apart.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub origin: [f64; 2],
    pub step: [f64; 2],
    pub nx: usize,
    pub ny: usize,
}

impl Grid {
    /// Nodes from `min` to `max` (both on nodes), at most `step` apart.
    pub fn covering(min: [f64; 2], max: [f64; 2], step: f64) -> Grid {
        let step = if step.is_finite() && step > 0.0 {
            step.max(MIN_STEP)
        } else {
            1.0
        };
        let axis = |lo: f64, hi: f64| -> (usize, f64) {
            let span = hi - lo;
            if span.is_nan() || span <= 1e-12 {
                return (1, step);
            }
            let n = ((span / step - 1e-9).ceil().max(1.0) as usize).saturating_add(1);
            (n, span / (n - 1) as f64)
        };
        let (nx, sx) = axis(min[0], max[0]);
        let (ny, sy) = axis(min[1], max[1]);
        Grid {
            origin: min,
            step: [sx, sy],
            nx,
            ny,
        }
    }

    /// The grid of a stock from `min` to `max`, `finest` apart when it can
    /// be, coarser until it has at most `max_nodes` nodes.
    pub fn for_stock(min: [f64; 2], max: [f64; 2], finest: f64, max_nodes: usize) -> Grid {
        let mut step = if finest.is_finite() && finest > 0.0 {
            finest.max(MIN_STEP)
        } else {
            1.0
        };
        let max_nodes = max_nodes.max(4);
        loop {
            let grid = Grid::covering(min, max, step);
            if grid.nodes() <= max_nodes {
                return grid;
            }
            step *= 1.1;
        }
    }

    pub fn nodes(&self) -> usize {
        self.nx * self.ny
    }

    pub fn x(&self, i: usize) -> f64 {
        self.origin[0] + self.step[0] * i as f64
    }

    pub fn y(&self, j: usize) -> f64 {
        self.origin[1] + self.step[1] * j as f64
    }

    pub fn index(&self, i: usize, j: usize) -> usize {
        j * self.nx + i
    }

    /// The area a node stands for (mm²).
    pub fn cell_area(&self) -> f64 {
        self.step[0] * self.step[1]
    }

    /// The nodes from `lo` to `hi` along X (`axis` 0) or Y (1), both
    /// included; none there.
    pub fn span(&self, axis: usize, lo: f64, hi: f64) -> Option<(usize, usize)> {
        let (n, o, s) = if axis == 0 {
            (self.nx, self.origin[0], self.step[0])
        } else {
            (self.ny, self.origin[1], self.step[1])
        };
        let a = ((lo - o) / s - 1e-9).ceil().max(0.0);
        let b = ((hi - o) / s + 1e-9).floor().min((n - 1) as f64);
        if a.is_nan() || b.is_nan() || a > b {
            return None;
        }
        Some((a as usize, b as usize))
    }

    /// The node nearest a point of the work plane, when it is on the grid
    /// (within half a step of its edge).
    pub fn nearest(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        let i = ((x - self.origin[0]) / self.step[0]).round();
        let j = ((y - self.origin[1]) / self.step[1]).round();
        if !(i >= 0.0 && j >= 0.0 && i < self.nx as f64 && j < self.ny as f64) {
            return None;
        }
        Some((i as usize, j as usize))
    }
}

/// The finest step for a program's tools: an eighth of the smallest
/// tool's diameter.
pub fn finest_step(program: &Program) -> f64 {
    let smallest = program
        .operations
        .iter()
        .map(|op| op.tool.diameter)
        .filter(|d| d.is_finite() && *d > 0.0)
        .fold(f64::INFINITY, f64::min);
    if smallest.is_finite() {
        (smallest / NODES_ACROSS_TOOL).max(MIN_STEP)
    } else {
        1.0
    }
}

/// The cutting end's profile above its tip.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Profile {
    Flat,
    Ball,
    /// A flat middle to `flat` from the axis, then a quarter round of
    /// `corner`.
    Bull {
        flat: f64,
        corner: f64,
    },
    /// Flat to `tip` from the axis, then rising `slope` mm per mm.
    Cone {
        tip: f64,
        slope: f64,
    },
}

/// A tool's cutting end as the simulation sweeps it (the module's
/// documentation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cutter {
    pub radius: f64,
    profile: Profile,
}

impl Cutter {
    pub fn of(tool: &Tool) -> Cutter {
        let radius = tool.radius().max(0.0);
        let cone = |tip: f64, angle: f64| {
            let half = (angle.clamp(1.0, 179.0) / 2.0).to_radians();
            Profile::Cone {
                tip: tip.clamp(0.0, radius),
                slope: 1.0 / half.tan(),
            }
        };
        let profile = match tool.shape {
            ToolShape::FlatEndMill => Profile::Flat,
            ToolShape::BallEndMill => Profile::Ball,
            ToolShape::BullNose { corner_radius } => {
                let corner = corner_radius.clamp(0.0, radius);
                if corner > 0.0 {
                    Profile::Bull {
                        flat: radius - corner,
                        corner,
                    }
                } else {
                    Profile::Flat
                }
            }
            ToolShape::Drill { point_angle } => cone(0.0, point_angle),
            ToolShape::VBit {
                angle,
                tip_diameter,
            } => cone(tip_diameter / 2.0, angle),
        };
        Cutter { radius, profile }
    }

    /// The cutting end's height above the tip at `r` from the axis (`r`
    /// taken within the radius).
    pub fn height(&self, r: f64) -> f64 {
        let r = r.clamp(0.0, self.radius);
        match self.profile {
            Profile::Flat => 0.0,
            Profile::Ball => self.radius - (self.radius * self.radius - r * r).max(0.0).sqrt(),
            Profile::Bull { flat, corner } => {
                if r <= flat {
                    0.0
                } else {
                    let d = r - flat;
                    corner - (corner * corner - d * d).max(0.0).sqrt()
                }
            }
            Profile::Cone { tip, slope } => (r - tip).max(0.0) * slope,
        }
    }

    fn flat(&self) -> bool {
        matches!(self.profile, Profile::Flat)
    }
}

/// What a sweep took off the stock.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Removal {
    /// The most it lowered a node (mm).
    pub depth: f64,
    /// That node, at its new height.
    pub at: Option<P3>,
    /// The volume taken off (mm³; each node's drop times its cell).
    pub volume: f64,
    /// Where along the sweep (a fraction of it) the tool first took
    /// something off.
    pub first: Option<f64>,
    /// The nodes lowered: `[i0, i1, j0, j1]`, both ends included.
    pub nodes: Option<[usize; 4]>,
}

impl Removal {
    /// Adds the removal of a part of the sweep from `s0` to `s1` (fractions
    /// of the whole).
    pub(crate) fn add(&mut self, other: Removal, s0: f64, s1: f64) {
        if other.depth > self.depth {
            self.depth = other.depth;
            self.at = other.at;
        }
        self.volume += other.volume;
        if let Some(f) = other.first {
            let f = s0 + f * (s1 - s0);
            self.first = Some(self.first.map_or(f, |x| x.min(f)));
        }
        if let Some(n) = other.nodes {
            self.nodes = Some(match self.nodes {
                None => n,
                Some(m) => [
                    m[0].min(n[0]),
                    m[1].max(n[1]),
                    m[2].min(n[2]),
                    m[3].max(n[3]),
                ],
            });
        }
    }

    /// Whether it took off more than `tolerance` anywhere.
    pub fn cut(&self, tolerance: f64) -> bool {
        self.depth > tolerance
    }
}

/// The ways of a rapid from `from` to `to` as the posts write it: up first
/// when it rises, across first when it sinks (one or two lines).
pub fn rapid_legs(from: P3, to: P3) -> Vec<(P3, P3)> {
    let corner = if to[2] > from[2] + 1e-9 {
        [from[0], from[1], to[2]]
    } else {
        [to[0], to[1], from[2]]
    };
    [(from, corner), (corner, to)]
        .into_iter()
        .filter(|(a, b)| crate::toolpath::dist3(*a, *b) > 1e-12)
        .collect()
}

/// A straight move of the tip for the sweep: from `a` by `u`.
struct Stretch {
    a: P3,
    u: P3,
    /// `u`'s length in the plane, squared.
    uu: f64,
}

impl Stretch {
    fn z(&self, t: f64) -> f64 {
        self.a[2] + t * self.u[2]
    }

    /// The lowest the cutter's end reaches over the node `q` (relative to
    /// `a` in the plane) while the tip goes from `t0` to `t1`, where the
    /// node is under the tool, and where along the stretch.
    fn lowest(&self, cutter: &Cutter, q: [f64; 2], t0: f64, t1: f64) -> (f64, f64) {
        let dist = |t: f64| (q[0] - t * self.u[0]).hypot(q[1] - t * self.u[1]);
        let f = |t: f64| self.z(t) + cutter.height(dist(t));
        if cutter.flat() {
            // Level under the whole end: the lower end of the stretch.
            let (z0, z1) = (self.z(t0), self.z(t1));
            return if z0 <= z1 { (z0, t0) } else { (z1, t1) };
        }
        if self.uu <= 1e-18 {
            // Straight up or down: the same distance throughout.
            let t = if self.u[2] <= 0.0 { t1 } else { t0 };
            return (f(t), t);
        }
        if self.u[2].abs() <= 1e-12 {
            // Level: nearest the node.
            let t = ((q[0] * self.u[0] + q[1] * self.u[1]) / self.uu).clamp(t0, t1);
            return (f(t), t);
        }
        // Convex along the stretch: a golden section search.
        const G: f64 = 0.618_033_988_749_894_9;
        let (mut lo, mut hi) = (t0, t1);
        let mut x1 = hi - G * (hi - lo);
        let mut x2 = lo + G * (hi - lo);
        let (mut f1, mut f2) = (f(x1), f(x2));
        for _ in 0..60 {
            if hi - lo <= 1e-12 {
                break;
            }
            if f1 <= f2 {
                hi = x2;
                x2 = x1;
                f2 = f1;
                x1 = hi - G * (hi - lo);
                f1 = f(x1);
            } else {
                lo = x1;
                x1 = x2;
                f1 = f2;
                x2 = lo + G * (hi - lo);
                f2 = f(x2);
            }
        }
        let mid = (lo + hi) / 2.0;
        [(f(mid), mid), (f(t0), t0), (f(t1), t1)].into_iter().fold(
            (f64::INFINITY, mid),
            |best, c| if c.0 < best.0 { c } else { best },
        )
    }
}

/// The stock as a height map: the top of the material over each node of
/// `grid`, standing on `bottom`; a node at `bottom` has none.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightMap {
    pub grid: Grid,
    pub bottom: f64,
    /// Row by row (`grid.index`).
    pub z: Vec<f64>,
}

impl HeightMap {
    /// A block from `bottom` up to `top` over the whole grid.
    pub fn flat(grid: Grid, bottom: f64, top: f64) -> HeightMap {
        HeightMap {
            grid,
            bottom,
            z: vec![top.max(bottom); grid.nodes()],
        }
    }

    /// A box of stock on a grid covering it, `finest` apart when it can be
    /// with at most `max_nodes` nodes.
    pub fn of_box(bounds: &Bounds, finest: f64, max_nodes: usize) -> HeightMap {
        let grid = Grid::for_stock(
            [bounds.min[0], bounds.min[1]],
            [bounds.max[0], bounds.max[1]],
            finest,
            max_nodes,
        );
        HeightMap::flat(grid, bounds.min[2], bounds.max[2])
    }

    /// The highest of a mesh's triangles over each node of `grid` (a body
    /// seen from above), `bottom` where there is none.
    pub fn top_of_mesh(grid: Grid, bottom: f64, mesh: &TriMesh) -> HeightMap {
        let mut map = HeightMap::flat(grid, bottom, bottom);
        for t in &mesh.triangles {
            if t.iter().any(|&i| i as usize >= mesh.vertices.len()) {
                continue;
            }
            let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
            map.raise_to_triangle(a, b, c);
        }
        map
    }

    /// Raises the nodes under the triangle to its height there.
    fn raise_to_triangle(&mut self, a: P3, b: P3, c: P3) {
        let g = self.grid;
        let det = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if det.abs() <= 1e-15 {
            // Seen edge on from above: its neighbours hold it.
            return;
        }
        let lo = |k: usize| a[k].min(b[k]).min(c[k]);
        let hi = |k: usize| a[k].max(b[k]).max(c[k]);
        let Some((i0, i1)) = g.span(0, lo(0), hi(0)) else {
            return;
        };
        let Some((j0, j1)) = g.span(1, lo(1), hi(1)) else {
            return;
        };
        // Barycentric weights with a little slack, so that nodes on a
        // shared edge belong to both triangles.
        let eps = 1e-9;
        for j in j0..=j1 {
            let y = g.y(j);
            for i in i0..=i1 {
                let x = g.x(i);
                let wb = ((x - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (y - a[1])) / det;
                let wc = ((b[0] - a[0]) * (y - a[1]) - (x - a[0]) * (b[1] - a[1])) / det;
                let wa = 1.0 - wb - wc;
                if wa < -eps || wb < -eps || wc < -eps {
                    continue;
                }
                let z = wa * a[2] + wb * b[2] + wc * c[2];
                let k = g.index(i, j);
                if z > self.z[k] {
                    self.z[k] = z;
                }
            }
        }
    }

    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.z[self.grid.index(i, j)]
    }

    /// The height at the node nearest a point, when it is on the grid.
    pub fn height_at(&self, x: f64, y: f64) -> Option<f64> {
        self.grid.nearest(x, y).map(|(i, j)| self.at(i, j))
    }

    /// The highest node.
    pub fn top(&self) -> f64 {
        self.z.iter().copied().fold(self.bottom, f64::max)
    }

    /// The lowest node.
    pub fn lowest(&self) -> f64 {
        self.z.iter().copied().fold(f64::INFINITY, f64::min)
    }

    /// Sweeps the cutter's end along a straight move of its tip from `a`
    /// to `b`, lowering the nodes it passes over.
    pub fn cut_line(&mut self, cutter: &Cutter, a: P3, b: P3) -> Removal {
        let r = cutter.radius;
        let mut out = Removal::default();
        if r.is_nan() || r <= 0.0 || self.z.is_empty() {
            return out;
        }
        // Long moves in pieces, so that the nodes looked at stay near them.
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let limit = (4.0 * r).max(16.0 * self.grid.step[0].min(self.grid.step[1]));
        let n = ((length / limit).ceil() as usize).clamp(1, 1 << 20);
        for k in 0..n {
            let s0 = k as f64 / n as f64;
            let s1 = (k + 1) as f64 / n as f64;
            let p = |s: f64| -> P3 { std::array::from_fn(|m| a[m] + (b[m] - a[m]) * s) };
            let piece = self.cut_stretch(cutter, p(s0), p(s1));
            out.add(piece, s0, s1);
        }
        out
    }

    fn cut_stretch(&mut self, cutter: &Cutter, a: P3, b: P3) -> Removal {
        let r = cutter.radius;
        let g = self.grid;
        let mut out = Removal::default();
        let Some((i0, i1)) = g.span(0, a[0].min(b[0]) - r, a[0].max(b[0]) + r) else {
            return out;
        };
        let Some((j0, j1)) = g.span(1, a[1].min(b[1]) - r, a[1].max(b[1]) + r) else {
            return out;
        };
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let stretch = Stretch {
            a,
            u,
            uu: u[0] * u[0] + u[1] * u[1],
        };
        let lowest_tip = a[2].min(b[2]);
        let rr = r * r;
        let area = g.cell_area();
        for j in j0..=j1 {
            let qy = g.y(j) - a[1];
            for i in i0..=i1 {
                let k = g.index(i, j);
                let old = self.z[k];
                // Nothing there, or nothing the tip's height can reach.
                if old <= self.bottom || old <= lowest_tip {
                    continue;
                }
                let qx = g.x(i) - a[0];
                let qq = qx * qx + qy * qy;
                // Where along the stretch the node is under the tool.
                let (t0, t1) = if stretch.uu <= 1e-18 {
                    if qq > rr * (1.0 + 1e-12) {
                        continue;
                    }
                    (0.0, 1.0)
                } else {
                    let qu = qx * u[0] + qy * u[1];
                    let disc = qu * qu - stretch.uu * (qq - rr * (1.0 + 1e-12));
                    if disc < 0.0 {
                        continue;
                    }
                    let s = disc.sqrt();
                    let lo = ((qu - s) / stretch.uu).max(0.0);
                    let hi = ((qu + s) / stretch.uu).min(1.0);
                    if lo > hi {
                        continue;
                    }
                    (lo, hi)
                };
                let (low, t) = stretch.lowest(cutter, [qx, qy], t0, t1);
                let new = low.max(self.bottom);
                if new >= old {
                    continue;
                }
                self.z[k] = new;
                let drop = old - new;
                out.volume += drop * area;
                if drop > out.depth {
                    out.depth = drop;
                    out.at = Some([g.x(i), g.y(j), new]);
                }
                // Where the tool first takes something off the node: as it
                // comes over it, or, going down, where the tip passes the
                // node's old height.
                let mut first = t0;
                if u[2] < 0.0 {
                    first = first.max(((old - a[2]) / u[2]).clamp(t0, t1)).min(t);
                }
                out.first = Some(out.first.map_or(first, |f| f.min(first)));
                out.nodes = Some(match out.nodes {
                    None => [i, i, j, j],
                    Some(m) => [m[0].min(i), m[1].max(i), m[2].min(j), m[3].max(j)],
                });
            }
        }
        out
    }

    /// Sweeps the cutter along a move of its tip from `from`: a line, an
    /// arc in chords within a quarter of the grid's step, a rapid as the
    /// posts write it. One removal per line the move is written as (a
    /// rapid's way up or across, and down).
    pub fn cut_move(&mut self, cutter: &Cutter, from: P3, m: &Move) -> Vec<Removal> {
        match *m {
            Move::Rapid { to } => rapid_legs(from, to)
                .into_iter()
                .map(|(a, b)| self.cut_line(cutter, a, b))
                .collect(),
            Move::Line { to, .. } => vec![self.cut_line(cutter, from, to)],
            Move::Arc { .. } => {
                let tolerance = (self.grid.step[0].min(self.grid.step[1]) / 4.0).clamp(1e-4, 0.05);
                let points = move_points(from, m, tolerance);
                let n = points.len().max(1) as f64;
                let mut out = Removal::default();
                let mut at = from;
                for (k, p) in points.into_iter().enumerate() {
                    let piece = self.cut_line(cutter, at, p);
                    out.add(piece, k as f64 / n, (k + 1) as f64 / n);
                    at = p;
                }
                vec![out]
            }
        }
    }

    /// The heights to the micrometre as runs over the nodes in their order:
    /// pairs of `height` (µm) and `count`.
    pub fn runs(&self) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        for &z in &self.z {
            let v = microns(z);
            let n = out.len();
            if n >= 2 && out[n - 2] == v && out[n - 1] > 0 {
                out[n - 1] += 1;
            } else {
                out.push(v);
                out.push(1);
            }
        }
        out
    }

    /// The heights changed since `before` (the same grid) as runs: pairs of
    /// `height` (µm) and `count`, or `0` and `-count` for nodes unchanged.
    pub fn runs_since(&self, before: &HeightMap) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        for (k, &z) in self.z.iter().enumerate() {
            let v = microns(z);
            let same = before.z.get(k).is_some_and(|&b| microns(b) == v);
            let n = out.len();
            if same {
                if n >= 2 && out[n - 1] < 0 {
                    out[n - 1] -= 1;
                } else {
                    out.push(0);
                    out.push(-1);
                }
            } else if n >= 2 && out[n - 2] == v && out[n - 1] > 0 {
                out[n - 1] += 1;
            } else {
                out.push(v);
                out.push(1);
            }
        }
        out
    }

    /// Applies runs of [`HeightMap::runs`] or [`HeightMap::runs_since`].
    pub fn apply_runs(&mut self, runs: &[i64]) -> Result<(), String> {
        if !runs.len().is_multiple_of(2) {
            return Err("The runs come in pairs.".into());
        }
        let mut k = 0usize;
        for pair in runs.chunks(2) {
            let (v, count) = (pair[0], pair[1]);
            let n = count.unsigned_abs() as usize;
            if k + n > self.z.len() {
                return Err("The runs are longer than the grid.".into());
            }
            if count > 0 {
                for z in &mut self.z[k..k + n] {
                    *z = v as f64 / MICRONS;
                }
            }
            k += n;
        }
        if k != self.z.len() {
            return Err("The runs are shorter than the grid.".into());
        }
        Ok(())
    }
}

fn microns(z: f64) -> i64 {
    (z * MICRONS).round() as i64
}

/// A place in a program: an operation's index and a move's index in its
/// toolpath.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Place {
    pub operation: usize,
    #[serde(rename = "move")]
    pub move_index: usize,
}

/// A rapid that cuts the stock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RapidCut {
    /// In words: "Pocket 1: a rapid move takes the cutter into the stock."
    pub message: String,
    pub operation: usize,
    #[serde(rename = "move")]
    pub move_index: usize,
    /// Which line of the rapid: up or across first (0), then the other (1).
    pub leg: usize,
    /// The most it takes off (mm).
    pub depth: f64,
    /// Where the tip is when it first cuts (work coordinates).
    pub at: P3,
}

/// What a simulation did.
#[derive(Clone, Debug, PartialEq)]
pub struct Simulation {
    /// The stock as cut.
    pub stock: HeightMap,
    pub rapid_cuts: Vec<RapidCut>,
    /// The volume taken off (mm³).
    pub removed: f64,
    /// The last place simulated; none when nothing was.
    pub last: Option<Place>,
}

/// The message of a rapid that cuts the stock in an operation.
pub fn rapid_cut_message(operation: &str) -> String {
    format!("{operation}: a rapid move takes the cutter into the stock.")
}

/// Cuts `stock` by the program's moves, up to and with the move `until`
/// (all when none), calling `after` with each move's place and the stock
/// after it. The tool starts an operation above its first point at the
/// program's safe height after a tool change, or where the last operation
/// ended with the same tool (as the collision check takes it).
pub fn simulate(
    program: &Program,
    stock: HeightMap,
    until: Option<Place>,
    mut after: impl FnMut(Place, &HeightMap),
) -> Simulation {
    let mut sim = Simulation {
        stock,
        rapid_cuts: Vec::new(),
        removed: 0.0,
        last: None,
    };
    let mut previous: Option<(u32, P3)> = None;
    'ops: for (oi, op) in program.operations.iter().enumerate() {
        if until.is_some_and(|u| oi > u.operation) {
            break;
        }
        let Some(first) = op.toolpath.moves.first() else {
            continue;
        };
        let cutter = Cutter::of(&op.tool);
        let first_to = first.to();
        let mut at = match previous {
            Some((tool, end)) if tool == op.tool.number => end,
            _ => [first_to[0], first_to[1], first_to[2].max(program.safe_z)],
        };
        for (mi, m) in op.toolpath.moves.iter().enumerate() {
            let removals = sim.stock.cut_move(&cutter, at, m);
            let rapid = matches!(m, Move::Rapid { .. });
            let legs = if rapid {
                rapid_legs(at, m.to())
            } else {
                vec![(at, m.to())]
            };
            for (leg, removal) in removals.iter().enumerate() {
                sim.removed += removal.volume;
                if rapid && removal.cut(RAPID_TOLERANCE) {
                    let (a, b) = legs.get(leg).copied().unwrap_or((at, m.to()));
                    let s = removal.first.unwrap_or(0.0);
                    sim.rapid_cuts.push(RapidCut {
                        message: rapid_cut_message(&op.name),
                        operation: oi,
                        move_index: mi,
                        leg,
                        depth: removal.depth,
                        at: std::array::from_fn(|k| a[k] + (b[k] - a[k]) * s),
                    });
                }
            }
            at = m.to();
            let place = Place {
                operation: oi,
                move_index: mi,
            };
            sim.last = Some(place);
            after(place, &sim.stock);
            if until == Some(place) {
                break 'ops;
            }
        }
        previous = Some((op.tool.number, at));
    }
    sim
}

/// A place of material left or of a gouge: where (the node, at the part's
/// height there) and how thick or deep (mm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thickness {
    pub at: P3,
    pub thickness: f64,
}

/// The material a setup leaves (the module's documentation): over the
/// part (the nodes where the part is) and beside it (the rest of the
/// stock, which should all be cut away).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Remaining {
    /// Thicknesses up to this count as none (mm).
    pub tolerance: f64,
    /// The thickest material left over the part (mm).
    pub max: f64,
    /// The mean thickness over the part (mm).
    pub mean: f64,
    /// The volume left over the part (mm³).
    pub volume: f64,
    /// The area of the part with more than the tolerance left (mm²).
    pub area: f64,
    /// The thickest places over the part, thickest first, apart from each
    /// other.
    pub thickest: Vec<Thickness>,
    /// The deepest cut into the part beyond the tolerance (mm).
    pub gouge: f64,
    pub gouges: Vec<Thickness>,
    /// The thickest material left beside the part (mm) and its volume.
    pub beside: f64,
    pub beside_volume: f64,
    /// The nodes over the part and beside it.
    pub part_nodes: usize,
    pub beside_nodes: usize,
}

/// How many places [`remaining`] gives of each kind.
pub const PLACES: usize = 5;

/// The material `stock` (as cut) leaves over `part` (its height map on the
/// same grid, at the stock's bottom where it is not), counting thicknesses
/// up to `tolerance` as none; the places at least `spacing` apart.
pub fn remaining(stock: &HeightMap, part: &HeightMap, tolerance: f64, spacing: f64) -> Remaining {
    let tolerance = tolerance.max(0.0);
    let g = stock.grid;
    let area = g.cell_area();
    let mut out = Remaining {
        tolerance,
        ..Remaining::default()
    };
    let mut left: Vec<(f64, usize)> = Vec::new();
    let mut cut_in: Vec<(f64, usize)> = Vec::new();
    let mut total = 0.0;
    for (k, (&s, &p)) in stock.z.iter().zip(&part.z).enumerate() {
        let over = p > part.bottom + 1e-9;
        let thickness = (s - p.max(stock.bottom)).max(0.0);
        if over {
            out.part_nodes += 1;
            total += thickness;
            out.volume += thickness * area;
            if thickness > tolerance {
                out.area += area;
                left.push((thickness, k));
            }
            out.max = out.max.max(thickness);
            // Against the part's lowest neighbour: a node on a wall the
            // tool runs along is cut to the wall's foot, which is no gouge.
            let (i, j) = (k % g.nx, k / g.nx);
            let mut low = p;
            for jj in j.saturating_sub(1)..=(j + 1).min(g.ny - 1) {
                for ii in i.saturating_sub(1)..=(i + 1).min(g.nx - 1) {
                    low = low.min(part.z[g.index(ii, jj)]);
                }
            }
            let gouge = low - s;
            if gouge > tolerance {
                out.gouge = out.gouge.max(gouge);
                cut_in.push((gouge, k));
            }
        } else {
            out.beside_nodes += 1;
            out.beside = out.beside.max(thickness);
            out.beside_volume += thickness * area;
        }
    }
    if out.part_nodes > 0 {
        out.mean = total / out.part_nodes as f64;
    }
    let places = |mut found: Vec<(f64, usize)>| -> Vec<Thickness> {
        found.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut picked: Vec<Thickness> = Vec::new();
        for (thickness, k) in found {
            if picked.len() >= PLACES {
                break;
            }
            let (i, j) = (k % g.nx, k / g.nx);
            let at = [g.x(i), g.y(j), part.z[k]];
            if picked
                .iter()
                .all(|p| (p.at[0] - at[0]).hypot(p.at[1] - at[1]) >= spacing)
            {
                picked.push(Thickness { at, thickness });
            }
        }
        picked
    };
    out.thickest = places(left);
    out.gouges = places(cut_in);
    out
}

/// The highest node over each block of a height map, level by level (a
/// block of level `l` holds `2^l` by `2^l` nodes), for finding the stock
/// near a part fast (the collision check).
#[derive(Clone, Debug)]
pub(crate) struct MaxPyramid {
    /// `levels[l]`: `(nx, ny, max)` of level `l`; level 0 the nodes.
    pub levels: Vec<(usize, usize, Vec<f64>)>,
}

impl MaxPyramid {
    pub fn new(map: &HeightMap) -> MaxPyramid {
        let mut levels = vec![(map.grid.nx, map.grid.ny, map.z.clone())];
        loop {
            let (nx, ny, prev) = levels.last().expect("level 0");
            if *nx <= 1 && *ny <= 1 {
                break;
            }
            let (px, py) = (*nx, *ny);
            let (qx, qy) = (px.div_ceil(2), py.div_ceil(2));
            let mut next = vec![f64::NEG_INFINITY; qx * qy];
            for j in 0..py {
                for i in 0..px {
                    let k = (j / 2) * qx + i / 2;
                    next[k] = next[k].max(prev[j * px + i]);
                }
            }
            levels.push((qx, qy, next));
        }
        MaxPyramid { levels }
    }

    /// Takes the map's new heights over the nodes `[i0, i1, j0, j1]`.
    pub fn update(&mut self, map: &HeightMap, nodes: [usize; 4]) {
        let [mut i0, mut i1, mut j0, mut j1] = nodes;
        for j in j0..=j1 {
            for i in i0..=i1 {
                let k = map.grid.index(i, j);
                self.levels[0].2[k] = map.z[k];
            }
        }
        for l in 1..self.levels.len() {
            let (lower, upper) = self.levels.split_at_mut(l);
            let (px, py, prev) = &lower[l - 1];
            let (qx, _, next) = &mut upper[0];
            i0 /= 2;
            i1 /= 2;
            j0 /= 2;
            j1 /= 2;
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let mut m = f64::NEG_INFINITY;
                    for dj in 0..2 {
                        for di in 0..2 {
                            let (x, y) = (2 * i + di, 2 * j + dj);
                            if x < *px && y < *py {
                                m = m.max(prev[y * px + x]);
                            }
                        }
                    }
                    next[j * *qx + i] = m;
                }
            }
        }
    }

    /// The nodes of a block: `[i0, i1, j0, j1]`, both ends included.
    pub fn block(&self, level: usize, i: usize, j: usize) -> [usize; 4] {
        let (nx, ny, _) = &self.levels[0];
        let size = 1usize << level;
        [
            i * size,
            ((i + 1) * size).min(*nx) - 1,
            j * size,
            ((j + 1) * size).min(*ny) - 1,
        ]
    }

    pub fn max(&self, level: usize, i: usize, j: usize) -> f64 {
        let (nx, _, m) = &self.levels[level];
        m[j * nx + i]
    }

    pub fn top(&self) -> usize {
        self.levels.len() - 1
    }
}

#[cfg(test)]
mod tests;
