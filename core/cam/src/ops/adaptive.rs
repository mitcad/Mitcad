// SPDX-License-Identifier: MIT
//! Adaptive (constant engagement) clearing of pockets and faces
//! (docs/cam.md, "Adaptive clearing").
//!
//! The engagement is the arc of the tool's circumference in the material.
//! On a straight wall a cut `e` deep engages `acos(1 - e / r)`; a sharp
//! turn of an offset loop with material outside it engages far more, so
//! offset pocketing loads the tool unevenly. Adaptive clearing keeps the
//! material each stretch of the path removes at most what a straight cut
//! at the maximum engagement removes:
//!
//! - **Pockets** are cleared from the middle outwards by fronts of equal
//!   distance from the middle *inside the pocket* (geodesic distance, so
//!   that the fronts go round islands and into side arms instead of
//!   through them): a helix opens the middle, small rings round it grow
//!   by the step that keeps their larger outer sweep within the limit,
//!   and each front only cuts what the one before left. The fronts are
//!   level lines of a distance field on a grid finer than the step,
//!   kept a little off the walls; the walls are then finished by loops of
//!   the exact offsets, each no deeper than the step.
//! - **Faces** are cleared from the outside in: the tool runs round the
//!   stock at the step's depth, the corners rounded about the material's
//!   corners, so that no corner loads it more than a straight side.
//!
//! Between cuts the tool stays down at the feed where the straight way
//! runs through what is cleared, and goes up to the retract height
//! where it does not. Each path is measured afterwards ([`engagement`])
//! and made again with a smaller step if any stretch of a tool radius
//! is above the limit.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::f64::consts::PI;

use cavalier_contours::shape_algorithms::ShapeOffsetOptions;
use serde::{Deserialize, Serialize};

use super::contour::shape_loops;
use super::entry::{Entry, EntrySpace, Helix, enter};
use super::face::FaceOp;
use super::path::{self, Area, Path, dist2};
use super::pocket::{PocketOp, is_empty, pocket_fits, region_shape};
use super::{Direction, Heights, check_mill};
use crate::CamError;
use crate::geom::{Contour, Region, Segment, Vertex};
use crate::tool::{Cutting, Tool};
use crate::toolpath::{Move, PathBuilder, Toolpath, move_points};

/// Adaptive clearing's setting; `{}` in JSON takes the defaults.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Adaptive {
    /// The largest arc of the tool's circumference in the material
    /// (degrees), measured as the material a stretch of one tool radius
    /// removes against a straight cut at this engagement. 45 by default
    /// (a cut of about 15 % of the diameter on a straight wall).
    #[serde(default = "default_engagement")]
    pub max_engagement: f64,
}

fn default_engagement() -> f64 {
    45.0
}

impl Default for Adaptive {
    fn default() -> Self {
        Adaptive {
            max_engagement: default_engagement(),
        }
    }
}

impl Adaptive {
    pub fn validate(&self) -> Result<(), CamError> {
        if !(self.max_engagement.is_finite()
            && self.max_engagement > 0.0
            && self.max_engagement < 180.0)
        {
            return Err(CamError::new(
                "The maximum engagement must be more than 0 and less than 180 degrees.",
            ));
        }
        Ok(())
    }

    /// The depth of a straight cut at the maximum engagement (mm).
    pub fn radial_depth(&self, radius: f64) -> f64 {
        radius * (1.0 - self.max_engagement.to_radians().cos())
    }
}

/// The engagement (degrees) of a straight cut `depth` deep by a tool of
/// `radius`.
pub fn engagement_of_depth(depth: f64, radius: f64) -> f64 {
    (1.0 - depth / radius).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The finest grid the measure and the fronts use, in nodes.
const MAX_NODES: usize = 6_000_000;
/// The grid the fronts aim at, in nodes, on large pockets.
const SOFT_NODES: f64 = 1_500_000.0;
/// A value no distance reaches: outside the area.
const BIG: f64 = 1e18;

/// The highest engagement (degrees) of the cuts of `path` at the floor
/// `z` through `material`, the material each stretch of one tool radius
/// removes taken as a straight cut's. The material starts whole; the
/// moves down to the floor (plunges, ramps, helixes) clear what is under
/// them without being measured. `None` when the material is too large to
/// measure on a grid of a fortieth of the radius.
pub fn engagement(path: &Toolpath, material: &Region, radius: f64, z: f64) -> Option<f64> {
    engagement_trace(path, material, radius, z).map(|t| t.iter().map(|s| s.1).fold(0.0, f64::max))
}

/// The engagement measured along the cuts at the floor: per step of an
/// eighth of the radius, where the tool is, the engagement of the last
/// stretch of a radius (once there is one) and the move's index.
fn engagement_trace(
    path: &Toolpath,
    material: &Region,
    radius: f64,
    z: f64,
) -> Option<Vec<([f64; 2], f64, usize)>> {
    let mut sim = Sim::new(material, radius, z, false)?;
    let mut at: Option<[f64; 3]> = None;
    for m in &path.moves {
        if let Some(from) = at {
            sim.feed(from, m);
        }
        sim.moves += 1;
        at = Some(m.to());
    }
    Some(sim.trace)
}

/// The material at a floor on a grid of a fortieth of the tool's radius,
/// and the engagement of the cuts through it (see [`engagement`]).
#[derive(Clone)]
struct Sim {
    x0: f64,
    y0: f64,
    hm: f64,
    nx: usize,
    ny: usize,
    mat: Vec<bool>,
    radius: f64,
    z: f64,
    window: VecDeque<(f64, f64)>,
    wl: f64,
    wa: f64,
    /// The cells cleared, in order, when they may be put back.
    undo: Option<Vec<usize>>,
    trace: Vec<([f64; 2], f64, usize)>,
    /// The highest engagement since the last checkpoint.
    peak: f64,
    /// The index of the move being fed.
    moves: usize,
}

/// A state of a [`Sim`] to go back to.
struct Checkpoint {
    undo: usize,
    window: VecDeque<(f64, f64)>,
    wl: f64,
    wa: f64,
    trace: usize,
    moves: usize,
}

impl Sim {
    fn new(material: &Region, radius: f64, z: f64, undoable: bool) -> Option<Sim> {
        let area = Area::of_region(material);
        let (min, max) = area.bounds()?;
        let hm = radius / 40.0;
        let x0 = min[0] - hm;
        let y0 = min[1] - hm;
        let nx = ((max[0] - min[0]) / hm).ceil() as usize + 3;
        let ny = ((max[1] - min[1]) / hm).ceil() as usize + 3;
        if nx.saturating_mul(ny) > MAX_NODES {
            return None;
        }
        let mut sim = Sim {
            x0,
            y0,
            hm,
            nx,
            ny,
            mat: Vec::new(),
            radius,
            z,
            window: VecDeque::new(),
            wl: 0.0,
            wa: 0.0,
            undo: undoable.then(Vec::new),
            trace: Vec::new(),
            peak: 0.0,
            moves: 0,
        };
        let mat: Vec<bool> = (0..nx * ny)
            .map(|n| area.contains(sim.center(n % nx, n / nx)))
            .collect();
        sim.mat = mat;
        Some(sim)
    }

    fn center(&self, i: usize, j: usize) -> [f64; 2] {
        [
            self.x0 + (i as f64 + 0.5) * self.hm,
            self.y0 + (j as f64 + 0.5) * self.hm,
        ]
    }

    /// Clears the tool's disk at `p`; the area it removed.
    fn clear(&mut self, p: [f64; 2]) -> f64 {
        let (radius, hm) = (self.radius, self.hm);
        let i0 = ((p[0] - radius - self.x0) / hm).floor().max(0.0) as usize;
        let j0 = ((p[1] - radius - self.y0) / hm).floor().max(0.0) as usize;
        let i1 = (((p[0] + radius - self.x0) / hm).ceil().max(0.0) as usize).min(self.nx);
        let j1 = (((p[1] + radius - self.y0) / hm).ceil().max(0.0) as usize).min(self.ny);
        let r2 = radius * radius;
        let mut count = 0usize;
        for j in j0..j1 {
            for i in i0..i1 {
                let n = j * self.nx + i;
                if self.mat[n] {
                    let c = self.center(i, j);
                    if (c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2) <= r2 {
                        self.mat[n] = false;
                        if let Some(u) = &mut self.undo {
                            u.push(n);
                        }
                        count += 1;
                    }
                }
            }
        }
        count as f64 * hm * hm
    }

    /// A move from `from`: a cut at the floor is measured, a move down to
    /// it (a plunge, a ramp, a helix) only clears what is under it.
    fn feed(&mut self, from: [f64; 3], m: &Move) {
        if matches!(m, Move::Rapid { .. }) {
            return;
        }
        let (z, radius) = (self.z, self.radius);
        let to = m.to();
        let floor = (from[2] - z).abs() < 1e-6 && (to[2] - z).abs() < 1e-6;
        let descent = to[2] < from[2] - 1e-9 && to[2] >= z - 1e-6;
        if !(floor || descent) {
            return;
        }
        let step = radius / 8.0;
        let mut prev = [from[0], from[1]];
        for q in move_points(from, m, radius * 1e-3) {
            let q2 = [q[0], q[1]];
            let len = dist2(prev, q2);
            let n = ((len / step).ceil() as usize).max(1);
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let p = [
                    prev[0] + (q2[0] - prev[0]) * t,
                    prev[1] + (q2[1] - prev[1]) * t,
                ];
                let removed = self.clear(p);
                if floor {
                    let l = len / n as f64;
                    self.window.push_back((l, removed));
                    self.wl += l;
                    self.wa += removed;
                    while self.window.len() > 1
                        && self.window.front().is_some_and(|f| self.wl - f.0 >= radius)
                    {
                        let f = self.window.pop_front().expect("a stretch");
                        self.wl -= f.0;
                        self.wa -= f.1;
                    }
                    if self.wl >= radius * (1.0 - 1e-9) {
                        let e = engagement_of_depth(self.wa / self.wl, radius);
                        self.peak = self.peak.max(e);
                        self.trace.push((p, e, self.moves));
                    }
                }
            }
            prev = q2;
        }
    }

    /// Feeds moves from `at`; where the tool is after them.
    fn feed_all(&mut self, mut at: Option<[f64; 3]>, moves: &[Move]) -> Option<[f64; 3]> {
        for m in moves {
            if let Some(from) = at {
                self.feed(from, m);
            }
            self.moves += 1;
            at = Some(m.to());
        }
        at
    }

    fn checkpoint(&mut self) -> Checkpoint {
        self.peak = 0.0;
        Checkpoint {
            undo: self.undo.as_ref().map_or(0, Vec::len),
            window: self.window.clone(),
            wl: self.wl,
            wa: self.wa,
            trace: self.trace.len(),
            moves: self.moves,
        }
    }

    fn rollback(&mut self, cp: &Checkpoint) {
        if let Some(u) = &mut self.undo {
            for n in u.drain(cp.undo..) {
                self.mat[n] = true;
            }
        }
        self.window = cp.window.clone();
        self.wl = cp.wl;
        self.wa = cp.wa;
        self.trace.truncate(cp.trace);
        self.moves = cp.moves;
        self.peak = 0.0;
    }
}

/// How the tool gets to the start of a run.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Link {
    /// Down a helix round the middle of a part, at the run's start.
    Helix(Helix),
    /// Down by the operation's entry along the run.
    Enter,
    /// To the start at the feed, at the floor (along the run's `via`,
    /// then straight).
    Feed,
    /// Up to the retract height, over, straight down at the plunge feed.
    Lift,
}

/// A stretch of the path at the floor and the way to its start.
#[derive(Clone, Debug)]
struct Run {
    start: [f64; 2],
    path: Path,
    link: Link,
    /// With [`Link::Feed`]: the way at the floor, from where the tool is,
    /// before the straight line to the start (back along a cut already
    /// made, where the straight way is not clear).
    via: Path,
}

/// What every run of a level is cut with.
struct Cuts<'a> {
    heights: Heights,
    cutting: Cutting,
    entry: Entry,
    space: &'a EntrySpace<'a>,
}

/// The runs at every level.
fn emit(runs: &[Run], levels: &[f64], top: f64, c: &Cuts) -> Toolpath {
    let mut b = PathBuilder::new();
    let mut entered = false;
    for (li, &z) in levels.iter().enumerate() {
        let top = if li == 0 { top } else { levels[li - 1] };
        for run in runs {
            emit_run(&mut b, run, top, z, entered, c);
            entered = true;
        }
    }
    if let Some(end) = b.at() {
        b.rapid([end[0], end[1], c.heights.clearance]);
    }
    b.finish()
}

/// One run at the floor `z` (the level above at `top`): the way to its
/// start, then its cut.
fn emit_run(b: &mut PathBuilder, run: &Run, top: f64, z: f64, entered: bool, c: &Cuts) {
    let s = run.start;
    let plunge = c.cutting.plunge_feed;
    let hop = if entered {
        c.heights.retract
    } else {
        c.heights.clearance
    };
    match run.link {
        Link::Helix(h) => {
            b.retract_to(s, hop);
            b.rapid([s[0], s[1], c.heights.retract]);
            h.cut(b, s, top, z, plunge);
        }
        Link::Enter => {
            b.retract_to(s, hop);
            b.rapid([s[0], s[1], c.heights.retract]);
            enter(b, &c.entry, &run.path, top, z, plunge, c.space);
        }
        Link::Feed => {
            run.via.cut(b, z, z, c.cutting.feed);
            b.line([s[0], s[1], z], c.cutting.feed);
        }
        Link::Lift => {
            b.retract_to(s, c.heights.retract);
            b.line([s[0], s[1], z], plunge);
        }
    }
    run.path.cut(b, z, z, c.cutting.feed);
}

/// The step from a ring of radius `prev` round a cleared middle to the
/// next, so that the next ring's outer sweep (the tool's radius `r`
/// farther out) removes no more per length than a straight cut `e` deep:
/// `((p + s + r)^2 - (p + r)^2) / (2 (p + s)) = e`. Far from the middle
/// it tends to `e`.
fn ring_step(prev: f64, r: f64, e: f64) -> f64 {
    let a = prev + r - e;
    let q = 2.0 * e * prev;
    let s = if q > 0.0 {
        q / (a + (a * a + q).sqrt())
    } else {
        0.0
    };
    s.clamp(0.25 * e, e)
}

/// A grid of nodes over an area, labelled by the parts of the area they
/// are in.
struct Grid {
    x0: f64,
    y0: f64,
    h: f64,
    nx: usize,
    ny: usize,
    /// The part of the area each node is in; `usize::MAX` outside.
    comp: Vec<usize>,
}

const OUTSIDE: usize = usize::MAX;
const UNLABELLED: usize = usize::MAX - 1;

const STEPS: [(i64, i64); 16] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (2, 1),
    (2, -1),
    (-2, 1),
    (-2, -1),
    (1, 2),
    (-1, 2),
    (1, -2),
    (-1, -2),
];

impl Grid {
    fn new(area: &Area, h: f64) -> Option<Grid> {
        let (min, max) = area.bounds()?;
        let x0 = min[0] - 2.0 * h;
        let y0 = min[1] - 2.0 * h;
        let nx = ((max[0] - min[0]) / h).ceil() as usize + 5;
        let ny = ((max[1] - min[1]) / h).ceil() as usize + 5;
        if nx.saturating_mul(ny) > MAX_NODES {
            return None;
        }
        let mut g = Grid {
            x0,
            y0,
            h,
            nx,
            ny,
            comp: Vec::new(),
        };
        let comp: Vec<usize> = (0..nx * ny)
            .map(|n| {
                if area.contains(g.pos(n)) {
                    UNLABELLED
                } else {
                    OUTSIDE
                }
            })
            .collect();
        g.comp = comp;
        Some(g)
    }

    fn pos(&self, n: usize) -> [f64; 2] {
        [
            self.x0 + (n % self.nx) as f64 * self.h,
            self.y0 + (n / self.nx) as f64 * self.h,
        ]
    }

    fn node(&self, i: i64, j: i64) -> Option<usize> {
        (i >= 0 && j >= 0 && (i as usize) < self.nx && (j as usize) < self.ny)
            .then_some(j.max(0) as usize * self.nx + i.max(0) as usize)
    }

    fn in_comp(&self, i: i64, j: i64, c: usize) -> bool {
        self.node(i, j).is_some_and(|n| self.comp[n] == c)
    }

    /// Labels the parts (nodes joined side by side); their number.
    fn label(&mut self) -> usize {
        let mut count = 0;
        for start in 0..self.comp.len() {
            if self.comp[start] != UNLABELLED {
                continue;
            }
            self.comp[start] = count;
            let mut queue = vec![start];
            while let Some(n) = queue.pop() {
                let (i, j) = ((n % self.nx) as i64, (n / self.nx) as i64);
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if let Some(m) = self.node(i + dx, j + dy)
                        && self.comp[m] == UNLABELLED
                    {
                        self.comp[m] = count;
                        queue.push(m);
                    }
                }
            }
            count += 1;
        }
        count
    }

    /// The nodes of part `c` next to a node outside it.
    fn boundary(&self, c: usize) -> Vec<usize> {
        (0..self.comp.len())
            .filter(|&n| {
                self.comp[n] == c && {
                    let (i, j) = ((n % self.nx) as i64, (n / self.nx) as i64);
                    [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .any(|&(dx, dy)| !self.in_comp(i + dx, j + dy, c))
                }
            })
            .collect()
    }

    /// The shortest distances within part `c` from the sources (a node
    /// and its starting distance), by steps to the sixteen nearest
    /// directions (within a few percent of the straight distance, never
    /// less). A step only passes between nodes of the part.
    fn distances(&self, c: usize, sources: &[(usize, f64)]) -> Vec<f64> {
        let mut d = vec![f64::INFINITY; self.comp.len()];
        let mut heap = BinaryHeap::new();
        for &(n, v) in sources {
            if self.comp[n] == c && v < d[n] {
                d[n] = v;
                heap.push(Reverse((v.to_bits(), n)));
            }
        }
        while let Some(Reverse((bits, n))) = heap.pop() {
            let v = f64::from_bits(bits);
            if v > d[n] {
                continue;
            }
            let (i, j) = ((n % self.nx) as i64, (n / self.nx) as i64);
            for (dx, dy) in STEPS {
                if !self.in_comp(i + dx, j + dy, c) {
                    continue;
                }
                let passes = match (dx.abs(), dy.abs()) {
                    (1, 1) => self.in_comp(i + dx, j, c) && self.in_comp(i, j + dy, c),
                    (2, 1) => self.in_comp(i + dx / 2, j, c) && self.in_comp(i + dx / 2, j + dy, c),
                    (1, 2) => self.in_comp(i, j + dy / 2, c) && self.in_comp(i + dx, j + dy / 2, c),
                    _ => true,
                };
                if !passes {
                    continue;
                }
                let t = self.node(i + dx, j + dy).expect("a node of the part");
                let nv = v + self.h * ((dx * dx + dy * dy) as f64).sqrt();
                if nv < d[t] {
                    d[t] = nv;
                    heap.push(Reverse((nv.to_bits(), t)));
                }
            }
        }
        d
    }

    /// The distance at `p` from the nodes of part `c` round it, `None`
    /// where none of them is in the part.
    fn value_at(&self, d: &[f64], c: usize, p: [f64; 2]) -> Option<f64> {
        let fx = (p[0] - self.x0) / self.h;
        let fy = (p[1] - self.y0) / self.h;
        let (i, j) = (fx.floor() as i64, fy.floor() as i64);
        let (u, v) = (fx - i as f64, fy - j as f64);
        let mut sum = 0.0;
        let mut weight = 0.0;
        for (di, dj, w) in [
            (0, 0, (1.0 - u) * (1.0 - v)),
            (1, 0, u * (1.0 - v)),
            (1, 1, u * v),
            (0, 1, (1.0 - u) * v),
        ] {
            if let Some(n) = self.node(i + di, j + dj)
                && self.comp[n] == c
                && d[n].is_finite()
                && w > 1e-12
            {
                sum += w * d[n];
                weight += w;
            }
        }
        (weight > 1e-12).then_some(sum / weight)
    }

    /// The new stretches of the level line `rho` of part `c`'s distances:
    /// its fronts, and the stretches along the part's edge that were
    /// beyond the level `prev`. Each runs with the inside (`d <= rho`) on
    /// its left; `true` for a closed one (its last point is its first).
    fn level_runs(&self, d: &[f64], c: usize, rho: f64, prev: f64) -> Vec<(Vec<[f64; 2]>, bool)> {
        let val = |n: usize| {
            if self.comp[n] == c && d[n].is_finite() {
                d[n]
            } else {
                BIG
            }
        };
        let nx = self.nx;
        // The point on an edge (2 n: from node n to its right, 2 n + 1:
        // to the node above) where the values pass `rho`.
        let edge_point = |e: usize| {
            let a = e / 2;
            let b = if e.is_multiple_of(2) { a + 1 } else { a + nx };
            let (va, vb) = (val(a), val(b));
            let t = ((rho - va) / (vb - va)).clamp(0.0, 1.0);
            let (pa, pb) = (self.pos(a), self.pos(b));
            [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t]
        };
        struct Piece {
            from: usize,
            to: usize,
            new: bool,
        }
        let mut pieces: Vec<Piece> = Vec::new();
        let mut by_from: HashMap<usize, usize> = HashMap::new();
        for j in 0..self.ny - 1 {
            for i in 0..nx - 1 {
                let n = j * nx + i;
                let corners = [n, n + 1, n + 1 + nx, n + nx];
                let edges = [2 * n, 2 * (n + 1) + 1, 2 * (n + nx), 2 * n + 1];
                let v = corners.map(val);
                let inside = v.map(|x| x <= rho);
                if inside.iter().all(|&x| x) || inside.iter().all(|&x| !x) {
                    continue;
                }
                // Crossings round the cell counter-clockwise: leaving the
                // inside (an exit) or coming into it.
                let crossings: Vec<(usize, bool)> = (0..4)
                    .filter(|&k| inside[k] != inside[(k + 1) % 4])
                    .map(|k| (k, inside[k]))
                    .collect();
                let m = crossings.len();
                let middle_inside = (v[0] + v[1] + v[2] + v[3]) / 4.0 <= rho;
                for (p, &(ke, exit)) in crossings.iter().enumerate() {
                    if !exit {
                        continue;
                    }
                    // A saddle joins the inside corners when its middle is
                    // inside: an exit goes to the next entry, else to the
                    // one before.
                    let q = if middle_inside {
                        (1..m).map(|o| (p + o) % m).find(|&q| !crossings[q].1)
                    } else {
                        (1..m).map(|o| (p + m - o) % m).find(|&q| !crossings[q].1)
                    };
                    let Some(q) = q else {
                        continue;
                    };
                    let kn = crossings[q].0;
                    let touches_outside =
                        |k: usize| v[k] >= BIG / 2.0 || v[(k + 1) % 4] >= BIG / 2.0;
                    let along_edge = touches_outside(ke) || touches_outside(kn);
                    let new = !along_edge
                        || [ke, kn].iter().any(|&k| {
                            [v[k], v[(k + 1) % 4]]
                                .iter()
                                .any(|&x| x < BIG / 2.0 && x > prev)
                        });
                    by_from.insert(edges[ke], pieces.len());
                    pieces.push(Piece {
                        from: edges[ke],
                        to: edges[kn],
                        new,
                    });
                }
            }
        }
        let mut used = vec![false; pieces.len()];
        let mut runs = Vec::new();
        for first in 0..pieces.len() {
            if used[first] {
                continue;
            }
            let mut chain = Vec::new();
            let mut k = first;
            loop {
                if used[k] {
                    break;
                }
                used[k] = true;
                chain.push(k);
                match by_from.get(&pieces[k].to) {
                    Some(&next) => k = next,
                    None => break,
                }
            }
            let last = *chain.last().expect("a piece");
            let closed = by_from.get(&pieces[last].to) == Some(&chain[0]);
            let n = chain.len();
            if chain.iter().all(|&k| pieces[k].new) {
                let mut pts: Vec<[f64; 2]> =
                    chain.iter().map(|&k| edge_point(pieces[k].from)).collect();
                pts.push(edge_point(pieces[last].to));
                runs.push((pts, closed));
                continue;
            }
            // Start just after an old piece, so that every new stretch is
            // whole.
            let begin = if closed {
                (0..n)
                    .find(|&k| !pieces[chain[k]].new && pieces[chain[(k + 1) % n]].new)
                    .map(|k| (k + 1) % n)
                    .unwrap_or(0)
            } else {
                0
            };
            let mut current: Vec<[f64; 2]> = Vec::new();
            for o in 0..n {
                let k = chain[(begin + o) % n];
                if pieces[k].new {
                    if current.is_empty() {
                        current.push(edge_point(pieces[k].from));
                    }
                    current.push(edge_point(pieces[k].to));
                } else if !current.is_empty() {
                    runs.push((std::mem::take(&mut current), false));
                }
            }
            if !current.is_empty() {
                runs.push((current, false));
            }
        }
        runs
    }
}

/// The points of a polyline with those closer than `tol` to the line
/// through their neighbours left out (Douglas and Peucker).
fn simplify(points: &[[f64; 2]], tol: f64) -> Vec<[f64; 2]> {
    let n = points.len();
    if n <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0, n - 1)];
    while let Some((a, b)) = stack.pop() {
        let chord = Segment {
            start: points[a],
            end: points[b],
            arc: None,
        };
        let mut far = 0.0;
        let mut at = a;
        for (k, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = path::seg_distance(&chord, *p);
            if d > far {
                far = d;
                at = k;
            }
        }
        if far > tol {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// A closed polyline (its last point its first) simplified in two halves.
fn simplify_closed(points: &[[f64; 2]], tol: f64) -> Vec<[f64; 2]> {
    let n = points.len();
    if n < 5 {
        return points.to_vec();
    }
    let mid = n / 2;
    let mut a = simplify(&points[..=mid], tol);
    let b = simplify(&points[mid..], tol);
    a.pop();
    a.extend(b);
    a
}

/// The index of the point of a closed polyline (`body`, its first point
/// not repeated) `dist` along it from point `k`.
fn advance(body: &[[f64; 2]], k: usize, dist: f64) -> usize {
    let n = body.len();
    let mut along = 0.0;
    let mut i = k;
    for _ in 0..n {
        let next = (i + 1) % n;
        along += dist2(body[i], body[next]);
        i = next;
        if along >= dist {
            break;
        }
    }
    i
}

/// Whether the straight way from `a` to `b` keeps inside the region and
/// `clear` from its outlines, checked every `step` (between the checks a
/// way passing a corner comes closer by less than `step^2 / (8 clear)`,
/// which the checks inside the way allow for).
fn way_clear(region: &Region, clear: f64, a: [f64; 2], b: [f64; 2], step: f64) -> bool {
    let n = ((dist2(a, b) / step).ceil() as usize).max(1);
    let between = clear + step * step / (4.0 * clear.max(step));
    (0..=n).all(|k| {
        let t = k as f64 / n as f64;
        let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let need = if k == 0 || k == n {
            clear - 1e-6
        } else {
            between
        };
        path::inside_region(region, p) && path::region_distance(region, p) >= need
    })
}

/// Where an adaptive pocket's plan is measured: its first level (`z`,
/// the material's top `top`), cut with `cuts`, against `limit` degrees.
struct Floor<'a> {
    cuts: &'a Cuts<'a>,
    top: f64,
    z: f64,
    limit: f64,
}

/// Emits `runs` from `pos` (`entered`: the tool has been down before)
/// and feeds their moves to `sim`; where the tool is after them.
fn simulate(
    sim: &mut Sim,
    runs: &[Run],
    pos: Option<[f64; 3]>,
    entered: bool,
    floor: &Floor,
) -> Option<[f64; 3]> {
    let mut b = match pos {
        Some(p) => PathBuilder::starting_at(p),
        None => PathBuilder::new(),
    };
    let mut entered = entered;
    for run in runs {
        emit_run(&mut b, run, floor.top, floor.z, entered, floor.cuts);
        entered = true;
    }
    sim.feed_all(pos, &b.finish().moves)
}

/// What the order of a part's fronts keeps to: the way between them clear
/// of the walls (`wall` from the region's outlines, checked every `step`),
/// and every front after those within `near` of it planned before it.
struct Sides<'a> {
    region: &'a Region,
    wall: f64,
    step: f64,
    near: f64,
}

/// Orders a part's fronts (planned ring by ring) so that each side of the
/// part is cut ring after ring while the tool can stay down, instead of
/// going from one side to the other at every ring: once the fronts split
/// round an island, each side is cut on its own. A front waits for every
/// front planned before it that comes within `near` (the material it cuts
/// depends on them). Of the fronts that may come next, the nearest one
/// the tool can reach at the feed within the limit (the way checked on
/// `sim`) is taken; else the nearest one reached through the air; else
/// the first planned. Each front is measured on `sim` as it is taken.
fn schedule(
    fronts: Vec<Run>,
    sim: &mut Sim,
    pos: &mut Option<[f64; 3]>,
    entered: bool,
    sides: &Sides,
    floor: &Floor,
) -> Vec<Run> {
    let n = fronts.len();
    // The points of each front a step of `near / 8` apart (two fronts
    // closer than `near` have points closer than `near` and the step),
    // and their box.
    let step = sides.near / 8.0;
    let points: Vec<Vec<[f64; 2]>> = fronts
        .iter()
        .map(|f| {
            let mut pts = vec![f.start];
            pts.extend(f.path.samples(step));
            pts
        })
        .collect();
    let boxes: Vec<([f64; 2], [f64; 2])> = points
        .iter()
        .map(|pts| {
            pts.iter().fold(
                ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
                |(lo, hi), p| {
                    (
                        [lo[0].min(p[0]), lo[1].min(p[1])],
                        [hi[0].max(p[0]), hi[1].max(p[1])],
                    )
                },
            )
        })
        .collect();
    let near = sides.near + step;
    let close = |a: usize, b: usize| {
        let (la, ha) = boxes[a];
        let (lb, hb) = boxes[b];
        if la[0] > hb[0] + near
            || lb[0] > ha[0] + near
            || la[1] > hb[1] + near
            || lb[1] > ha[1] + near
        {
            return false;
        }
        points[a]
            .iter()
            .any(|p| points[b].iter().any(|q| dist2(*p, *q) < near))
    };
    let mut then: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut waits: Vec<usize> = (0..n)
        .map(|b| {
            let before: Vec<usize> = (0..b).filter(|&a| close(a, b)).collect();
            for &a in &before {
                then[a].push(b);
            }
            before.len()
        })
        .collect();
    let mut ready: Vec<usize> = (0..n).filter(|&q| waits[q] == 0).collect();
    let mut out = Vec::with_capacity(n);
    let mut entered = entered;
    let mut taken = vec![false; n];
    while !ready.is_empty() {
        let cur = pos.map(|p| [p[0], p[1]]);
        // A part's first front enters by itself.
        let first = ready.iter().position(|&q| fronts[q].link == Link::Enter);
        let mut by_distance = ready.clone();
        if let Some(c) = cur {
            by_distance
                .sort_by(|&a, &b| dist2(fronts[a].start, c).total_cmp(&dist2(fronts[b].start, c)));
        }
        let try_link = |q: usize, link: Link, via: Path, sim: &mut Sim| {
            let mut run = fronts[q].clone();
            run.link = link;
            run.via = via;
            let mark = sim.checkpoint();
            let after = simulate(sim, std::slice::from_ref(&run), *pos, entered, floor);
            if sim.peak <= floor.limit {
                Some((q, run, after))
            } else {
                sim.rollback(&mark);
                None
            }
        };
        let mut chosen = None;
        if let Some(i) = first {
            chosen = try_link(ready[i], Link::Enter, Path::default(), sim);
        }
        if chosen.is_none()
            && let Some(c) = cur
        {
            // Back along the last cut, which is clear, when the straight
            // way is not: the next ring's start is next to the last's.
            let back = out
                .last()
                .filter(|l: &&Run| {
                    !l.path.is_empty() && l.path.end().is_some_and(|e| dist2(e, c) < 1e-9)
                })
                .map(|l: &Run| (l.path.reversed(), l.start));
            for &q in by_distance.iter().take(8) {
                if fronts[q].link == Link::Enter {
                    continue;
                }
                let target = fronts[q].start;
                if way_clear(sides.region, sides.wall, c, target, sides.step) {
                    chosen = try_link(q, Link::Feed, Path::default(), sim);
                }
                if chosen.is_none()
                    && let Some((via, from)) = &back
                    && way_clear(sides.region, sides.wall, *from, target, sides.step)
                {
                    chosen = try_link(q, Link::Feed, via.clone(), sim);
                }
                if chosen.is_some() {
                    break;
                }
            }
        }
        if chosen.is_none() {
            for &q in &by_distance {
                let link = if fronts[q].link == Link::Enter {
                    Link::Enter
                } else {
                    Link::Lift
                };
                chosen = try_link(q, link, Path::default(), sim);
                if chosen.is_some() {
                    break;
                }
            }
        }
        let (q, run, after) = match chosen {
            Some(c) => c,
            None => {
                // Over the limit whichever way: the first planned, as
                // planned (the final measure of the path decides).
                let q = *ready.iter().min().expect("a front");
                let run = fronts[q].clone();
                let after = simulate(sim, std::slice::from_ref(&run), *pos, entered, floor);
                (q, run, after)
            }
        };
        *pos = after;
        entered = true;
        taken[q] = true;
        ready.retain(|&x| x != q);
        for &b in &then[q] {
            waits[b] -= 1;
            if waits[b] == 0 && !taken[b] {
                ready.push(b);
            }
        }
        out.push(run);
    }
    out
}

/// The runs of an adaptive pocket at one level, `e` the depth a front
/// may cut far from the middle. Each ring of fronts is measured on the
/// material the runs before it left and made again closer to the last
/// (a smaller step) while a stretch of it is over the limit: where a
/// front ends at a wall or an island it also takes the strip beyond its
/// end, and in a channel narrower than the tool the material across is
/// wider than the way the tool center can go, so there the fronts come
/// closer together.
fn plan_pocket(
    op: &PocketOp,
    tool: &Tool,
    wall: f64,
    e: f64,
    floor: &Floor,
) -> Result<Vec<Run>, CamError> {
    let r = tool.radius();
    let mut sim = Sim::new(&op.region, r, floor.z, true);
    let mut pos: Option<[f64; 3]> = None;
    let options = ShapeOffsetOptions::default();
    let shape = region_shape(&op.region);
    let walls = shape.parallel_offset(wall, &options);
    if is_empty(&walls) {
        return Err(CamError::new("The tool does not fit into the pocket."));
    }
    let (wmin, wmax) = Area::of_shape(&walls)
        .bounds()
        .ok_or_else(|| CamError::new("The tool does not fit into the pocket."))?;
    let span = ((wmax[0] - wmin[0]) * (wmax[1] - wmin[1])).max(1e-6);
    let h = (e / 4.0).max((span / SOFT_NODES).sqrt()).max(1e-3);
    // The fronts keep this far inside the walls' loop, so that the grid's
    // coarseness never takes them into a wall.
    let margin = 2.0 * h;
    let climb = op.direction == Direction::Climb;
    let orient = |mut pts: Vec<[f64; 2]>| {
        if !climb {
            pts.reverse();
        }
        pts
    };
    let mut runs: Vec<Run> = Vec::new();
    let mut cur = [0.0, 0.0];
    let middle = shape.parallel_offset(wall + margin, &options);
    if !is_empty(&middle) {
        let area = Area::of_shape(&middle);
        let mut grid = Grid::new(&area, h).ok_or_else(|| {
            CamError::new("The pocket is too large for adaptive clearing with this tool.")
        })?;
        let parts = grid.label();
        for c in 0..parts {
            let edge: Vec<(usize, f64)> = grid.boundary(c).into_iter().map(|n| (n, 0.0)).collect();
            let from_edge = grid.distances(c, &edge);
            let Some(seed) = (0..grid.comp.len())
                .filter(|&n| grid.comp[n] == c && from_edge[n].is_finite())
                .max_by(|&a, &b| from_edge[a].total_cmp(&from_edge[b]))
            else {
                continue;
            };
            let m = grid.pos(seed);
            let room = path::region_distance(&op.region, m) - wall;
            let (radius, angle) = match op.entry {
                Entry::Helix { radius, angle } => (radius.min(room * 0.9), angle),
                _ => (0.0, 0.0),
            };
            let radius = if radius >= 0.05 * r { radius } else { 0.0 };
            let reach = radius.max(1.5 * h);
            let mut sources: Vec<(usize, f64)> = (0..grid.comp.len())
                .filter(|&n| grid.comp[n] == c && dist2(grid.pos(n), m) <= reach)
                .map(|n| (n, dist2(grid.pos(n), m)))
                .collect();
            if sources.is_empty() {
                sources.push((seed, 0.0));
            }
            let d = grid.distances(c, &sources);
            let dmax = (0..d.len())
                .filter(|&n| grid.comp[n] == c && d[n].is_finite())
                .map(|n| d[n])
                .fold(0.0, f64::max);
            let mut first = true;
            if radius > 0.0 {
                let start = [m[0] + radius, m[1]];
                let helix = Run {
                    start,
                    path: Path::default(),
                    link: Link::Helix(Helix {
                        center: m,
                        radius,
                        angle,
                        ccw: climb,
                    }),
                    via: Path::default(),
                };
                if let Some(sm) = &mut sim {
                    pos = simulate(
                        sm,
                        std::slice::from_ref(&helix),
                        pos,
                        !runs.is_empty(),
                        floor,
                    );
                }
                runs.push(helix);
                cur = start;
                first = false;
            }
            // The runs of the ring at `rho` (the one before at `prev`), from
            // `cur`; with where they end and whether the first still enters.
            let ring_at = |rho: f64, prev: f64, mut cur: [f64; 2], mut first: bool| {
                let mut out: Vec<Run> = Vec::new();
                let mut ring: Vec<(Vec<[f64; 2]>, bool)> = grid
                    .level_runs(&d, c, rho, prev)
                    .into_iter()
                    .map(|(pts, closed)| {
                        let pts = if closed {
                            simplify_closed(&pts, h / 4.0)
                        } else {
                            simplify(&pts, h / 4.0)
                        };
                        (orient(pts), closed)
                    })
                    .filter(|(pts, _)| pts.len() >= 2)
                    .collect();
                while !ring.is_empty() {
                    // The nearest run next; a closed one started at its
                    // point nearest the tool.
                    let mut best = (0, 0, f64::INFINITY);
                    for (i, (pts, closed)) in ring.iter().enumerate() {
                        let candidates = if *closed { pts.len() - 1 } else { 1 };
                        for (k, p) in pts.iter().enumerate().take(candidates.max(1)) {
                            let dd = dist2(*p, cur);
                            if dd < best.2 {
                                best = (i, k, dd);
                            }
                        }
                    }
                    let (pts, closed) = ring.remove(best.0);
                    let pts = if closed {
                        // Into a closed ring a diameter along from its
                        // nearest point: the link then comes in slantwise
                        // and the cut deepens gradually.
                        let body = &pts[..pts.len() - 1];
                        let k = advance(body, best.1, 2.0 * r);
                        let mut rotated: Vec<[f64; 2]> =
                            body[k..].iter().chain(body[..k].iter()).copied().collect();
                        rotated.push(rotated[0]);
                        rotated
                    } else {
                        pts
                    };
                    let start = pts[0];
                    let link = if first {
                        Link::Enter
                    } else {
                        let stays = way_clear(&op.region, wall, cur, start, h / 2.0) && {
                            let n = ((dist2(cur, start) / (h / 2.0)).ceil() as usize).max(1);
                            (0..=n).all(|k| {
                                let t = k as f64 / n as f64;
                                let p = [
                                    cur[0] + (start[0] - cur[0]) * t,
                                    cur[1] + (start[1] - cur[1]) * t,
                                ];
                                grid.value_at(&d, c, p).is_some_and(|v| v <= rho + h)
                            })
                        };
                        if stays { Link::Feed } else { Link::Lift }
                    };
                    first = false;
                    let p = Path::polyline(&pts);
                    cur = p.end().unwrap_or(start);
                    out.push(Run {
                        start,
                        path: p,
                        link,
                        via: Path::default(),
                    });
                }
                (out, cur, first)
            };
            // The rings are planned in their order, then cut side by side.
            let fronts_from = runs.len();
            let before = sim.as_mut().map(|sm| (sm.checkpoint(), pos));
            let mut prev = radius;
            let mut last_step: Option<f64> = None;
            for _ in 0..100_000 {
                let mut step = ring_step(prev, r, e);
                if let Some(l) = last_step {
                    step = step.min(l * 1.5);
                }
                let entered = !runs.is_empty();
                let (rho, ring, cur_after, first_after) = loop {
                    let rho = prev + step;
                    let (ring, cur_after, first_after) = ring_at(rho, prev, cur, first);
                    let Some(sm) = &mut sim else {
                        break (rho, ring, cur_after, first_after);
                    };
                    let mark = sm.checkpoint();
                    let after = simulate(sm, &ring, pos, entered, floor);
                    if sm.peak <= floor.limit || step < 0.02 * e {
                        pos = after;
                        break (rho, ring, cur_after, first_after);
                    }
                    sm.rollback(&mark);
                    step *= 0.6;
                };
                runs.extend(ring);
                cur = cur_after;
                first = first_after;
                last_step = Some(step);
                prev = rho;
                if prev >= dmax {
                    break;
                }
            }
            if let (Some(sm), Some((mark, pos_before))) = (&mut sim, before) {
                sm.rollback(&mark);
                pos = pos_before;
                let fronts = runs.split_off(fronts_from);
                let sides = Sides {
                    region: &op.region,
                    wall,
                    step: h / 2.0,
                    near: 2.0 * r,
                };
                let entered = !runs.is_empty();
                runs.extend(schedule(fronts, sm, &mut pos, entered, &sides, floor));
                if let Some(p) = pos {
                    cur = [p[0], p[1]];
                }
            }
        }
    }
    // The walls: loops of the exact offsets from the fronts' edge out to
    // the walls, each no deeper than a front; one over the limit (at a
    // corner, or where the fronts left more) gets another halfway first.
    let band = margin + 1.5 * h;
    let count = ((band / (0.6 * e)).ceil() as usize).max(1);
    let wall_runs = |offset: f64, mut cur: [f64; 2], mut first: bool| {
        let mut out: Vec<Run> = Vec::new();
        let mut loops: Vec<Contour> = shape_loops(&shape.parallel_offset(offset, &options), false)
            .into_iter()
            .map(|l| if climb { l } else { l.reversed() })
            .collect();
        while !loops.is_empty() {
            let i = (0..loops.len())
                .min_by(|&a, &b| {
                    let da = dist2(loops[a].started_near(cur).vertices[0].pos(), cur);
                    let db = dist2(loops[b].started_near(cur).vertices[0].pos(), cur);
                    da.total_cmp(&db)
                })
                .expect("a loop");
            let l = loops.remove(i).started_near(cur);
            let start = l.vertices[0].pos();
            let link = if first {
                Link::Enter
            } else if way_clear(&op.region, wall, cur, start, h / 2.0) {
                Link::Feed
            } else {
                Link::Lift
            };
            first = false;
            out.push(Run {
                start,
                path: Path::of(&l),
                link,
                via: Path::default(),
            });
            cur = start;
        }
        (out, cur)
    };
    // Offsets still to cut, the innermost last in the list.
    let mut pending: Vec<f64> = (0..count)
        .map(|j| wall + band * j as f64 / count as f64)
        .collect();
    let mut cut_to = wall + band;
    while let Some(offset) = pending.pop() {
        let (ring, cur_after) = wall_runs(offset, cur, runs.is_empty());
        if let Some(sm) = &mut sim {
            let mark = sm.checkpoint();
            let after = simulate(sm, &ring, pos, !runs.is_empty(), floor);
            if sm.peak > floor.limit && cut_to - offset > 0.02 * e {
                sm.rollback(&mark);
                pending.push(offset);
                pending.push((offset + cut_to) / 2.0);
                continue;
            }
            pos = after;
        }
        runs.extend(ring);
        cur = cur_after;
        cut_to = offset;
    }
    Ok(runs)
}

/// The toolpath of a pocket cleared adaptively.
pub fn adaptive_pocket(op: &PocketOp, tool: &Tool, a: &Adaptive) -> Result<Toolpath, CamError> {
    check_mill(tool, &op.cutting, &op.depths)?;
    op.heights.validate(op.depths.top)?;
    op.entry.validate()?;
    a.validate()?;
    op.region.validate().map_err(CamError::new)?;
    if !(op.stock_to_leave.is_finite() && op.stock_to_leave >= 0.0) {
        return Err(CamError::new("The stock to leave must be zero or more."));
    }
    let levels = op.depths.levels()?;
    let r = tool.radius();
    let wall = r + op.stock_to_leave;
    let fits = |c: [f64; 2], radius: f64| pocket_fits(&op.region, wall, c, radius);
    let space = EntrySpace {
        helix_fits: &fits,
        min_ramp: r,
    };
    let cuts = Cuts {
        heights: op.heights,
        cutting: op.cutting,
        entry: op.entry,
        space: &space,
    };
    let floor = Floor {
        cuts: &cuts,
        top: op.depths.top,
        z: levels[0],
        limit: a.max_engagement - 0.01,
    };
    let mut e = a.radial_depth(r) * 0.9;
    for _ in 0..4 {
        let runs = plan_pocket(op, tool, wall, e, &floor)?;
        let path = emit(&runs, &levels, op.depths.top, &cuts);
        match engagement(&path, &op.region, r, levels[0]) {
            Some(max) if max > a.max_engagement => e *= 0.7,
            _ => return Ok(path),
        }
    }
    Err(CamError::new(
        "The pocket cannot be cleared within the maximum engagement; allow more engagement or clear it with offset loops.",
    ))
}

/// A closed outline round the rectangle `[ax, bx] x [ay, by]` at
/// `radius`, counter-clockwise, its corners quarter circles about the
/// rectangle's corners (a stadium or a circle where it has no width).
fn rounded_rectangle(ax: f64, bx: f64, ay: f64, by: f64, radius: f64) -> Contour {
    let t = (PI / 8.0).tan();
    let corners = [
        Vertex::new(ax, ay - radius, 0.0),
        Vertex::new(bx, ay - radius, t),
        Vertex::new(bx + radius, ay, 0.0),
        Vertex::new(bx + radius, by, t),
        Vertex::new(bx, by + radius, 0.0),
        Vertex::new(ax, by + radius, t),
        Vertex::new(ax - radius, by, 0.0),
        Vertex::new(ax - radius, ay, t),
    ];
    // A side of no length is left out.
    let vertices = (0..8)
        .filter(|&i| {
            let (v, next) = (corners[i], corners[(i + 1) % 8]);
            v.bulge != 0.0 || (v.x - next.x).hypot(v.y - next.y) > 1e-9
        })
        .map(|i| corners[i])
        .collect();
    Contour::closed(vertices)
}

/// The facing loops from the outside in, each `step` deeper into the
/// rectangle, oriented for the direction.
fn face_loops(min: [f64; 2], max: [f64; 2], r: f64, step: f64, climb: bool) -> Vec<Contour> {
    let half = (max[0] - min[0]).min(max[1] - min[1]) / 2.0;
    let mid = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let mut out = Vec::new();
    for k in 0..1_000_000 {
        let a = k as f64 * step;
        if k > 0 && a >= half - 1e-9 {
            break;
        }
        let l = rounded_rectangle(
            (min[0] + a).min(mid[0]),
            (max[0] - a).max(mid[0]),
            (min[1] + a).min(mid[1]),
            (max[1] - a).max(mid[1]),
            r - step,
        );
        // Climb milling with the material inside: clockwise.
        out.push(if climb { l.reversed() } else { l });
    }
    out
}

/// The toolpath of a facing cleared adaptively: loops round the stock
/// from the outside in, coming down outside it.
pub fn adaptive_face(op: &FaceOp, tool: &Tool, a: &Adaptive) -> Result<Toolpath, CamError> {
    check_mill(tool, &op.cutting, &op.depths)?;
    op.heights.validate(op.depths.top)?;
    a.validate()?;
    let levels = op.depths.levels()?;
    let min = [op.min[0].min(op.max[0]), op.min[1].min(op.max[1])];
    let max = [op.min[0].max(op.max[0]), op.min[1].max(op.max[1])];
    if max[0] - min[0] <= 0.0 || max[1] - min[1] <= 0.0 {
        return Err(CamError::new("The area to face is empty."));
    }
    let r = tool.radius();
    let material = Region::new(Contour::rectangle(min, max), vec![]);
    let climb = op.direction == Direction::Climb;
    let mut step = (a.radial_depth(r) * 0.9).min(r * 0.9);
    for _ in 0..4 {
        let loops = face_loops(min, max, r, step, climb);
        let first = loops[0].started_near([min[0] - 2.0 * r - 1.0, (min[1] + max[1]) / 2.0]);
        let s0 = first.vertices[0].pos();
        // Down beside the stock, clear of it, then in slantwise along the
        // first loop's way, the cut deepening over a diameter.
        let t = Path::of(&first).tangent_at(0.0);
        let outside = [
            s0[0] - 2.0 * r * t[0] - (step + 1.0),
            s0[1] - 2.0 * r * t[1],
        ];
        let mut b = PathBuilder::new();
        for &z in &levels {
            b.retract_to(outside, op.heights.clearance);
            b.rapid([outside[0], outside[1], op.heights.retract]);
            b.line([outside[0], outside[1], z], op.cutting.plunge_feed);
            let mut cur = s0;
            for (k, l) in loops.iter().enumerate() {
                let c = if k == 0 {
                    first.clone()
                } else {
                    // The next loop a diameter along from where it is
                    // nearest: the way there runs slantwise.
                    let near = l.started_near(cur);
                    let lp = Path::of(&near);
                    let ahead = lp
                        .point_at((2.0 * r).min(lp.length() / 2.0))
                        .unwrap_or(near.vertices[0].pos());
                    near.started_near(ahead)
                };
                let start = c.vertices[0].pos();
                b.line([start[0], start[1], z], op.cutting.feed);
                Path::of(&c).cut(&mut b, z, z, op.cutting.feed);
                cur = start;
            }
            b.rapid([cur[0], cur[1], op.heights.clearance]);
        }
        let path = b.finish();
        match engagement(&path, &material, r, levels[0]) {
            Some(m) if m > a.max_engagement => step *= 0.7,
            _ => return Ok(path),
        }
    }
    Err(CamError::new(
        "The face cannot be cleared within the maximum engagement; allow more engagement.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::pocket::tests::{assert_cleared, assert_no_gouge};
    use crate::ops::{Depths, Direction};

    /// The straight plunges from the retract height to the floor (the way
    /// to a run through the air).
    fn lifts(p: &Toolpath, retract: f64, floor: f64) -> usize {
        p.moves
            .windows(2)
            .filter(|w| {
                let (a, b) = (w[0].to(), w[1].to());
                matches!(w[1], Move::Line { .. })
                    && a[0] == b[0]
                    && a[1] == b[1]
                    && (a[2] - retract).abs() < 1e-9
                    && (b[2] - floor).abs() < 1e-9
            })
            .count()
    }

    fn cutting() -> Cutting {
        Cutting {
            spindle_rpm: 18000.0,
            feed: 1500.0,
            plunge_feed: 300.0,
        }
    }

    fn pocket_op(region: Region, max_engagement: f64) -> PocketOp {
        PocketOp {
            region,
            stepover: None,
            direction: Direction::Climb,
            depths: Depths {
                top: 0.0,
                bottom: -4.0,
                step_down: 4.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 2.0,
            },
            stock_to_leave: 0.0,
            cutting: cutting(),
            entry: Entry::Helix {
                radius: 2.0,
                angle: 3.0,
            },
            adaptive: Some(Adaptive { max_engagement }),
        }
    }

    #[test]
    fn a_straight_cut_measures_its_depth() {
        // A 6 mm tool along the edge of a plate, 0.6 mm into it.
        let material = Region::new(Contour::rectangle([0.0, 0.0], [60.0, 30.0]), vec![]);
        let path = Toolpath {
            moves: vec![
                Move::Rapid {
                    to: [-5.0, -2.4, 5.0],
                },
                Move::Line {
                    to: [-5.0, -2.4, -1.0],
                    feed: 300.0,
                },
                Move::Line {
                    to: [65.0, -2.4, -1.0],
                    feed: 1000.0,
                },
            ],
            ..Toolpath::default()
        };
        let e = engagement(&path, &material, 3.0, -1.0).unwrap();
        let expected = engagement_of_depth(0.6, 3.0);
        assert!((e - expected).abs() < 2.0, "{e} against {expected}");
        assert!((expected - 36.87).abs() < 0.01);
    }

    #[test]
    fn ring_steps_grow_to_the_depth() {
        let (r, e) = (3.0, 0.8);
        assert!((ring_step(0.0, r, e) - 0.2).abs() < 1e-12);
        let s = ring_step(1.0, r, e);
        // The ring's outer sweep removes e per length.
        let removed = ((1.0 + s + r).powi(2) - (1.0 + r).powi(2)) / (2.0 * (1.0 + s));
        assert!((removed - e).abs() < 1e-9);
        assert!(ring_step(1e6, r, e) > 0.99 * e);
    }

    #[test]
    fn an_adaptive_pocket_with_an_island_stays_within_the_engagement() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let island = Contour::circle([20.0, 12.0], 4.0);
        let region = Region::new(Contour::rectangle([0.0, 0.0], [40.0, 25.0]), vec![island]);
        let o = pocket_op(region.clone(), 45.0);
        let p = adaptive_pocket(&o, &tool, o.adaptive.as_ref().unwrap()).unwrap();
        let e = engagement(&p, &region, 3.0, -4.0).unwrap();
        assert!(e <= 45.0, "engagement {e}");
        assert!(e > 20.0, "the tool cuts: {e}");
        assert_cleared(&region, &tool, &p, -4.0);
        assert_no_gouge(&region, 3.0, &p, 0.0);
        // Each side of the island is cut ring after ring: the tool goes up
        // only where the fronts split and join, not at every ring.
        let lifts = lifts(&p, 2.0, -4.0);
        assert!(lifts <= 10, "{lifts} lifts");
        // It comes down by a helix, never by a plunge into the floor.
        assert!(
            p.moves
                .iter()
                .any(|m| matches!(m, Move::Arc { to, .. } if to[2] < -1e-9 && to[2] > -4.0))
        );
        // The same pocket by offset loops engages far more at its corners.
        let mut offset = o.clone();
        offset.adaptive = None;
        offset.stepover = Some(3.0);
        let q = super::super::pocket::pocket(&offset, &tool).unwrap();
        assert!(engagement(&q, &region, 3.0, -4.0).unwrap() > 60.0);
    }

    #[test]
    fn an_adaptive_pocket_runs_conventional_and_keeps_stock() {
        let tool = Tool::flat_end_mill(1, 6.0);
        let region = Region::new(Contour::rectangle([0.0, 0.0], [30.0, 20.0]), vec![]);
        let mut o = pocket_op(region.clone(), 60.0);
        o.direction = Direction::Conventional;
        o.stock_to_leave = 0.5;
        let p = adaptive_pocket(&o, &tool, o.adaptive.as_ref().unwrap())
            .unwrap_or_else(|e| panic!("{}", e.message));
        assert_no_gouge(&region, 3.5, &p, 0.0);
        assert!(engagement(&p, &region, 3.0, -4.0).unwrap() <= 60.0);
    }

    #[test]
    fn adaptive_facing_comes_in_from_outside_within_the_engagement() {
        let tool = Tool::flat_end_mill(1, 10.0);
        let op = FaceOp {
            min: [0.0, 0.0],
            max: [60.0, 30.0],
            depths: Depths {
                top: 1.0,
                bottom: 0.0,
                step_down: 1.0,
            },
            heights: Heights {
                clearance: 10.0,
                retract: 3.0,
            },
            stepover: None,
            cutting: cutting(),
            direction: Direction::Climb,
            adaptive: Some(Adaptive {
                max_engagement: 40.0,
            }),
        };
        let p = adaptive_face(&op, &tool, op.adaptive.as_ref().unwrap()).unwrap();
        let material = Region::new(Contour::rectangle([0.0, 0.0], [60.0, 30.0]), vec![]);
        let e = engagement(&p, &material, 5.0, 0.0).unwrap();
        assert!(e <= 40.0 && e > 25.0, "engagement {e}");
        // The plunge is beside the stock, clear of it.
        let plunge = p
            .moves
            .iter()
            .find(|m| matches!(m, Move::Line { to, .. } if to[2] == 0.0))
            .unwrap()
            .to();
        assert!(plunge[0] <= -5.0 - 1.0 + 1e-9);
        // Nothing is left: every point of the area within the tool's
        // radius of the path at the floor.
        let pts = crate::ops::pocket::tests::floor_points(&p, 0.0);
        let mut x = 0.0;
        while x <= 60.0 {
            let mut y = 0.0;
            while y <= 30.0 {
                assert!(
                    pts.iter().any(|q| (q[0] - x).hypot(q[1] - y) <= 5.0 + 1e-6),
                    "material left at ({x}, {y})"
                );
                y += 0.5;
            }
            x += 0.5;
        }
    }

    #[test]
    fn a_rounded_rectangle_without_width_is_a_stadium() {
        let c = rounded_rectangle(5.0, 15.0, 3.0, 3.0, 2.0);
        assert!(c.validate().is_ok());
        assert!((c.area() - (20.0 * 2.0 + PI * 4.0)).abs() < 1e-9);
        let o = rounded_rectangle(5.0, 5.0, 3.0, 3.0, 2.0);
        assert!((o.area() - PI * 4.0).abs() < 1e-9);
    }
}
