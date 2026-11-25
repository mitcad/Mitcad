// SPDX-License-Identifier: MIT
//! The mill-turn's stock in 3D (mitcad#2319; docs/cam.md, "The multi-axis
//! stock"): the bar as cubes (voxels) of one size, cut by the whole
//! program in its order. A turning operation keeps what the turned section
//! after it keeps, revolved about the spindle axis (turning only takes
//! material away, so the section of the turning before and up to it is
//! what is left of its turning); a live tool takes away what its cutter
//! covers at points along its moves, on the face, the diameter, a plane or
//! a tilted plane, its axis the side's. The sub-spindle's operations turn
//! the part's back in their coordinates, the stock turned round, and the
//! remnant the part-off leaves on the main spindle goes; its live tools
//! (mitcad#2375) cut where their sub-spindle coordinates are in the main
//! spindle's.
//!
//! Its surface is the cubes' faces towards no material, a mesh for the
//! view; compared with the part (its mesh in work coordinates) it gives the
//! material left over the part and the cubes cut out of it (gouges).

use serde::{Deserialize, Serialize};

use super::live::LiveOp;
use super::{TurnPart, TurningProgram, simulate_turning};
use crate::collision::TriMesh;
use crate::machine::Finding;
use crate::tool::{Tool, ToolShape};
use crate::toolpath::{Move, P3, move_points};

/// The stock as cubes of `step`, `n` of them along work X, Y and Z from
/// `origin` (the corner of the first), filled where there is material.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Voxels {
    pub origin: P3,
    pub step: f64,
    pub n: [usize; 3],
    pub filled: Vec<bool>,
}

impl Voxels {
    /// The box from `lo` to `hi` in cubes no smaller than `finest` and at
    /// most `max_cubes` of them, all empty.
    pub fn new(lo: P3, hi: P3, finest: f64, max_cubes: usize) -> Self {
        let size: [f64; 3] = std::array::from_fn(|k| (hi[k] - lo[k]).max(1e-6));
        let volume = size[0] * size[1] * size[2];
        let step = finest
            .max((volume / max_cubes.max(1) as f64).cbrt())
            .max(1e-3);
        let n: [usize; 3] = std::array::from_fn(|k| ((size[k] / step).ceil() as usize).max(1));
        Voxels {
            origin: lo,
            step,
            n,
            filled: vec![false; n[0] * n[1] * n[2]],
        }
    }

    fn index(&self, i: usize, j: usize, k: usize) -> usize {
        (k * self.n[1] + j) * self.n[0] + i
    }

    /// The centre of a cube.
    pub fn center(&self, i: usize, j: usize, k: usize) -> P3 {
        let s = self.step;
        [
            self.origin[0] + (i as f64 + 0.5) * s,
            self.origin[1] + (j as f64 + 0.5) * s,
            self.origin[2] + (k as f64 + 0.5) * s,
        ]
    }

    /// The material's volume (mm³).
    pub fn volume(&self) -> f64 {
        self.filled.iter().filter(|f| **f).count() as f64 * self.step.powi(3)
    }

    /// Keeps the material inside the turned section (`stock` revolved about
    /// the spindle axis).
    pub fn keep_turned(&mut self, stock: &super::TurnStock) {
        let (Some(&z0), true) = (stock.z.first(), stock.z.len() >= 2) else {
            self.filled.iter_mut().for_each(|f| *f = false);
            return;
        };
        let dz = stock.z[1] - z0;
        for k in 0..self.n[2] {
            for j in 0..self.n[1] {
                for i in 0..self.n[0] {
                    let index = self.index(i, j, k);
                    if !self.filled[index] {
                        continue;
                    }
                    let c = self.center(i, j, k);
                    let node = ((c[2] - z0) / dz).round();
                    let keep = node >= 0.0 && (node as usize) < stock.z.len() && {
                        let node = node as usize;
                        let r = c[0].hypot(c[1]);
                        stock.inner[node] <= r && r < stock.outer[node]
                    };
                    self.filled[index] = keep;
                }
            }
        }
    }

    /// Takes away what a milling tool covers with its tip at `tip` and its
    /// shank along `axis` (a unit vector from the tip up); gives how many
    /// cubes it took.
    pub fn cut_tool(&mut self, tool: &Tool, tip: P3, axis: P3) -> usize {
        let r = 0.5 * tool.diameter;
        let length = tool.flute_length.max(r);
        let top: P3 = std::array::from_fn(|k| tip[k] + axis[k] * length);
        let s = self.step;
        let range = |k: usize| {
            let lo = tip[k].min(top[k]) - r - s;
            let hi = tip[k].max(top[k]) + r + s;
            let a = ((lo - self.origin[k]) / s).floor().max(0.0) as usize;
            let b = (((hi - self.origin[k]) / s).ceil().max(0.0) as usize).min(self.n[k]);
            a..b
        };
        let (ri, rj, rk) = (range(0), range(1), range(2));
        let mut taken = 0;
        for k in rk {
            for j in rj.clone() {
                for i in ri.clone() {
                    let index = self.index(i, j, k);
                    if !self.filled[index] {
                        continue;
                    }
                    let c = self.center(i, j, k);
                    let d: P3 = std::array::from_fn(|q| c[q] - tip[q]);
                    let h = d[0] * axis[0] + d[1] * axis[1] + d[2] * axis[2];
                    if !(-1e-9..=length).contains(&h) {
                        continue;
                    }
                    let radial = (0..3)
                        .map(|q| (d[q] - h * axis[q]).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    if radial <= cutter_reach(&tool.shape, r, h) {
                        self.filled[index] = false;
                        taken += 1;
                    }
                }
            }
        }
        taken
    }

    /// The material's surface: the cubes' faces towards no material, each
    /// two triangles turning their front outwards.
    pub fn mesh(&self) -> TriMesh {
        let mut vertices: Vec<P3> = Vec::new();
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        let mut corners: std::collections::HashMap<[usize; 3], u32> =
            std::collections::HashMap::new();
        let mut corner = |c: [usize; 3], vertices: &mut Vec<P3>| -> u32 {
            *corners.entry(c).or_insert_with(|| {
                vertices.push(std::array::from_fn(|k| {
                    self.origin[k] + c[k] as f64 * self.step
                }));
                (vertices.len() - 1) as u32
            })
        };
        for k in 0..self.n[2] {
            for j in 0..self.n[1] {
                for i in 0..self.n[0] {
                    if !self.filled[self.index(i, j, k)] {
                        continue;
                    }
                    let cell = [i, j, k];
                    for a in 0..3 {
                        for up in [false, true] {
                            let mut next = cell;
                            let open = if up {
                                next[a] += 1;
                                next[a] >= self.n[a]
                            } else if next[a] == 0 {
                                true
                            } else {
                                next[a] -= 1;
                                false
                            };
                            if !open && self.filled[self.index(next[0], next[1], next[2])] {
                                continue;
                            }
                            // The face's corners round its normal.
                            let (u, v) = ((a + 1) % 3, (a + 2) % 3);
                            let mut quad = [[0usize; 3]; 4];
                            for (q, (du, dv)) in
                                [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate()
                            {
                                let mut c = cell;
                                if up {
                                    c[a] += 1;
                                }
                                c[u] += du;
                                c[v] += dv;
                                quad[q] = c;
                            }
                            if !up {
                                quad.reverse();
                            }
                            let ids: Vec<u32> =
                                quad.iter().map(|c| corner(*c, &mut vertices)).collect();
                            triangles.push([ids[0], ids[1], ids[2]]);
                            triangles.push([ids[0], ids[2], ids[3]]);
                        }
                    }
                }
            }
        }
        TriMesh {
            vertices,
            triangles,
        }
    }

    /// Which cubes' centres are inside a closed mesh, by the crossings of
    /// a line along Z through each column.
    pub fn inside(&self, mesh: &TriMesh) -> Vec<bool> {
        let mut crossings: Vec<Vec<f64>> = vec![Vec::new(); self.n[0] * self.n[1]];
        let s = self.step;
        for t in &mesh.triangles {
            let Some(p) = t
                .iter()
                .map(|&v| mesh.vertices.get(v as usize).copied())
                .collect::<Option<Vec<P3>>>()
            else {
                continue;
            };
            let lo = |k: usize| p.iter().map(|q| q[k]).fold(f64::INFINITY, f64::min);
            let hi = |k: usize| p.iter().map(|q| q[k]).fold(f64::NEG_INFINITY, f64::max);
            let cells = |k: usize| {
                let a = ((lo(k) - self.origin[k]) / s - 0.5).ceil().max(0.0) as usize;
                let b = (((hi(k) - self.origin[k]) / s - 0.5).floor() + 1.0).max(0.0) as usize;
                a..b.min(self.n[k])
            };
            let det = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
            if det.abs() < 1e-18 {
                continue;
            }
            for j in cells(1) {
                for i in cells(0) {
                    let c = self.center(i, j, 0);
                    // Barycentric coordinates in the XY plane, a centre on
                    // an edge counted once (half-open).
                    let l1 = ((c[0] - p[0][0]) * (p[2][1] - p[0][1])
                        - (p[2][0] - p[0][0]) * (c[1] - p[0][1]))
                        / det;
                    let l2 = ((p[1][0] - p[0][0]) * (c[1] - p[0][1])
                        - (c[0] - p[0][0]) * (p[1][1] - p[0][1]))
                        / det;
                    let l0 = 1.0 - l1 - l2;
                    if l0 < 0.0 || l1 < 0.0 || l2 <= 0.0 || l0 >= 1.0 {
                        continue;
                    }
                    let z = l0 * p[0][2] + l1 * p[1][2] + l2 * p[2][2];
                    crossings[j * self.n[0] + i].push(z);
                }
            }
        }
        let mut out = vec![false; self.filled.len()];
        for j in 0..self.n[1] {
            for i in 0..self.n[0] {
                let column = &mut crossings[j * self.n[0] + i];
                column.sort_by(f64::total_cmp);
                for pair in column.as_chunks::<2>().0 {
                    for k in 0..self.n[2] {
                        let z = self.center(i, j, k)[2];
                        if z > pair[0] && z < pair[1] {
                            let index = self.index(i, j, k);
                            out[index] = true;
                        }
                    }
                }
            }
        }
        out
    }
}

/// How far from its axis a cutter reaches at `h` above its tip.
fn cutter_reach(shape: &ToolShape, r: f64, h: f64) -> f64 {
    match *shape {
        ToolShape::FlatEndMill => r,
        ToolShape::BallEndMill => {
            if h >= r {
                r
            } else {
                (r * r - (r - h).powi(2)).max(0.0).sqrt()
            }
        }
        ToolShape::BullNose { corner_radius } => {
            let cr = corner_radius.clamp(0.0, r);
            if h >= cr {
                r
            } else {
                r - cr + (cr * cr - (cr - h).powi(2)).max(0.0).sqrt()
            }
        }
        ToolShape::Drill { point_angle } => {
            let t = (point_angle.to_radians() / 2.0).tan();
            if t <= 1e-9 { r } else { (h * t).min(r) }
        }
        ToolShape::VBit {
            angle,
            tip_diameter,
        } => {
            let t = (angle.to_radians() / 2.0).tan();
            (0.5 * tip_diameter + h * t).min(r)
        }
    }
}

/// What the 3D simulation found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolidSim {
    /// The stock as the program leaves it.
    pub voxels: Voxels,
    /// The bar's volume before the program (mm³).
    pub initial: f64,
    /// The volume the live tools took away (mm³).
    pub milled: f64,
    /// The material left over the part and the part's cubes cut away
    /// (mm³), with a few of the latter's centres; zero without the part.
    pub left: f64,
    pub gouge: f64,
    pub gouges: Vec<P3>,
    /// Rapid moves that took a tool into the stock, one per operation.
    pub rapid_cuts: Vec<Finding>,
}

/// Points along a live operation's moves at most `spacing` apart (in its
/// side's frame), each with whether it is a rapid's.
fn live_samples(live: &LiveOp, spacing: f64) -> Vec<(P3, bool)> {
    let mut out = Vec::new();
    let mut at: Option<P3> = None;
    for m in &live.path.moves {
        let rapid = matches!(m, Move::Rapid { .. });
        match at {
            None => out.push((m.to(), rapid)),
            Some(from) => {
                let mut prev = from;
                for q in move_points(from, m, spacing / 4.0) {
                    let d = ((q[0] - prev[0]).powi(2)
                        + (q[1] - prev[1]).powi(2)
                        + (q[2] - prev[2]).powi(2))
                    .sqrt();
                    let n = ((d / spacing).ceil() as usize).clamp(1, 1_000_000);
                    for k in 1..=n {
                        let t = k as f64 / n as f64;
                        out.push((
                            std::array::from_fn(|i| prev[i] + (q[i] - prev[i]) * t),
                            rapid,
                        ));
                    }
                    prev = q;
                }
            }
        }
        at = Some(m.to());
    }
    out
}

/// Cuts the program out of the bar in 3D (the module's documentation):
/// the bar from `z_min` to the stock's face in cubes no smaller than
/// `finest`, at most `max_cubes` of them; the turning on nodes `step`
/// apart as [`simulate_turning`] takes them; compared with `part_mesh`
/// (work coordinates) when given.
pub fn simulate_solid(
    part: &TurnPart,
    program: &TurningProgram,
    z_min: f64,
    step: f64,
    finest: f64,
    max_cubes: usize,
    part_mesh: Option<&TriMesh>,
) -> SolidSim {
    let r = part.stock_radius;
    let mut voxels = Voxels::new([-r, -r, z_min], [r, r, part.stock_face], finest, max_cubes);
    // The bar (or the tube).
    let bare = TurningProgram {
        operations: Vec::new(),
        hand_over: None,
        ..program.clone()
    };
    let full = simulate_turning(part, &bare, z_min, step, 0.0);
    voxels.filled.iter_mut().for_each(|f| *f = true);
    voxels.keep_turned(&full.stock);
    let initial = voxels.volume();
    let cube = voxels.step.powi(3);
    let mut milled = 0.0;
    let mut rapid_cuts = Vec::new();
    let first_sub = program.first_sub();
    let hand_over = program.hand_over.as_ref();
    for (index, op) in program.operations.iter().enumerate() {
        // At the hand-over the remnant the part-off leaves stays on the
        // main spindle (mitcad#2375: a live operation may come first on the
        // sub-spindle).
        if Some(index) == first_sub
            && let Some(cut) = super::stock::parted_at(program, index)
        {
            for k in 0..voxels.n[2] {
                if voxels.center(0, 0, k)[2] < cut {
                    for j in 0..voxels.n[1] {
                        for i in 0..voxels.n[0] {
                            let at = voxels.index(i, j, k);
                            voxels.filled[at] = false;
                        }
                    }
                }
            }
        }
        // A live tool on the part's back cuts in the sub-spindle's work
        // coordinates (mitcad#2375).
        let sub = hand_over.filter(|_| op.sub);
        match &op.live {
            Some(live) => {
                let mut reported = false;
                for (q, rapid) in live_samples(live, voxels.step / 2.0) {
                    let (tip, axis) = match sub {
                        Some(h) => (
                            h.main_point(live.side.to_work(q)),
                            h.main_direction(live.side.tool_axis(q)),
                        ),
                        None => (live.side.to_work(q), live.side.tool_axis(q)),
                    };
                    let taken = voxels.cut_tool(&live.cutter, tip, axis);
                    milled += taken as f64 * cube;
                    if rapid && taken > 0 && !reported {
                        reported = true;
                        rapid_cuts.push(Finding {
                            message: format!(
                                "{}: a rapid move takes the tool into the stock.",
                                op.name
                            ),
                            operation: index,
                            at: Some(tip),
                        });
                    }
                }
            }
            None if op.path.moves.is_empty() => {}
            None => {
                // What the turning up to here keeps.
                let before = TurningProgram {
                    operations: program.operations[..=index]
                        .iter()
                        .filter(|o| o.live.is_none())
                        .cloned()
                        .collect(),
                    ..program.clone()
                };
                let turned = simulate_turning(part, &before, z_min, step, 0.0);
                voxels.keep_turned(&turned.stock);
                for cut in turned.rapid_cuts {
                    let same = |c: &Finding| c.message == cut.message;
                    if !rapid_cuts.iter().any(same) {
                        let at = cut.at;
                        rapid_cuts.push(Finding {
                            operation: index,
                            at,
                            ..cut
                        });
                    }
                }
            }
        }
    }
    let (mut left, mut gouge, mut gouges) = (0.0, 0.0, Vec::new());
    if let Some(mesh) = part_mesh {
        let inside = voxels.inside(mesh);
        let bar = {
            let mut b = voxels.clone();
            b.filled.iter_mut().for_each(|f| *f = true);
            b.keep_turned(&full.stock);
            b.filled
        };
        // Behind the part's back is the bar the chuck holds.
        let back = part.back_z();
        for k in 0..voxels.n[2] {
            if voxels.center(0, 0, k)[2] < back {
                continue;
            }
            for j in 0..voxels.n[1] {
                for i in 0..voxels.n[0] {
                    let index = voxels.index(i, j, k);
                    if voxels.filled[index] && !inside[index] {
                        left += cube;
                    } else if !voxels.filled[index] && inside[index] && bar[index] {
                        gouge += cube;
                        if gouges.len() < 20 {
                            gouges.push(voxels.center(i, j, k));
                        }
                    }
                }
            }
        }
    }
    SolidSim {
        voxels,
        initial,
        milled,
        left,
        gouge,
        gouges,
        rapid_cuts,
    }
}
