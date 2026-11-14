// SPDX-License-Identifier: MIT
//! The spindle assembly's collisions along a program (docs/cam.md,
//! "Spindle assembly"): each part of the spindle with its tool
//! ([`SpindleAssembly`]) swept along the moves of each operation against
//! the stock, the fixtures and the machine's keep-out zones
//! ([`Obstacle`]), with the first contact of each part with each obstacle
//! in an operation and the smallest clearance of the whole program.
//!
//! - The stock is taken as the program has cut it so far
//!   ([`StockModel::AsCut`], the default): a height map ([`crate::stock`])
//!   that each move cuts after it is checked, its material a vertical
//!   column over each node. A part that does not cut meets the material
//!   left before the move; a part as wide as the cutter or wider reaches
//!   any place before the cutter does, so the material the move itself
//!   cuts away is still there for it. The flutes may be in the stock at a
//!   feed; at a rapid they cut it when the simulation takes more than
//!   [`crate::stock::RAPID_TOLERANCE`] off it.
//! - Or, conservatively ([`StockModel::BeforeCut`]), as it is before the
//!   cut: the tool's flutes may be in it at a feed, never at a rapid,
//!   except where the rapid goes straight up (through the column the tool
//!   already fills) or runs back down along a plunge the operation already
//!   cut (a drill's pecks). Every other part touching the stock is a
//!   collision.
//! - Every part, the flutes too, touching a fixture or a zone at any speed
//!   is a collision.
//! - A rapid moves as the post writes it, up first when it rises and
//!   across first when it sinks; an operation starts above its first point
//!   at the program's safe height (after a tool change) or where the last
//!   operation ended (the same tool).
//! - The distance between a part and an obstacle at a place of the tool's
//!   tip is the distance from that place to a convex set, so it changes no
//!   faster than the tip moves; the check needs no samples: the sweep of a
//!   convex piece along a line is convex, and its distance to a convex
//!   piece of an obstacle is found exactly (`gjk`, within 1e-10 of it, never
//!   above it). An arc is cut into pieces held by the triangles of their
//!   end tangents (prisms between the heights of a helix), refined only
//!   where they come near an obstacle, until a piece is within the arc
//!   tolerance of its arc: the clearance on arcs is never above the true
//!   one and at most the tolerance below it.
//! - Touching counts as a contact (a clearance up to [`TOUCH`]).
//!
//! The results say what in words (no names, ids or coordinates of the
//! design); the place is in the operation's and the move's index, the move's
//! line in the posted program ([`CollisionReport::set_lines`]) and the tip's
//! position `at` (work coordinates).

mod gjk;
#[cfg(test)]
mod tests;
// The tool changes: a macro changer's own moves and the travel after a
// change (mitcad#1013, phase 3).
mod changer;
pub use changer::{ChangeMove, check_tool_changes, check_with_tool_changes};

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

use self::gjk::{Convex, distance};
use crate::machine::{Bounds, Machine, Placement};
use crate::post::MoveLines;
use crate::program::Program;
use crate::spindle::{Frustum, PartKind, PartShape, SpindleAssembly, SpindleModel};
use crate::stock::{Cutter, Grid, HeightMap, MaxPyramid, RAPID_TOLERANCE, Removal};
use crate::toolpath::{Move, P3, arc_sweep};
use mitcad_vec3::{add, cross, dot, sub};

/// A part this near an obstacle (mm) touches it.
pub const TOUCH: f64 = 1e-9;

/// What an obstacle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObstacleKind {
    /// The material being cut.
    Stock,
    /// What holds the stock: clamps, a vise, screws, a jig.
    Fixture,
    /// A keep-out zone of the machine.
    Zone,
}

/// A triangle mesh (mm): the surface of a fixture or of a stock that is a
/// body.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TriMesh {
    pub vertices: Vec<P3>,
    /// Indices into `vertices`.
    pub triangles: Vec<[u32; 3]>,
}

impl TriMesh {
    fn faces(&self) -> Vec<[P3; 3]> {
        self.triangles
            .iter()
            .filter(|t| t.iter().all(|&i| (i as usize) < self.vertices.len()))
            .map(|t| t.map(|i| self.vertices[i as usize]))
            .collect()
    }

    /// Whether every edge is shared by exactly two triangles: the surface
    /// of a solid, which has an inside.
    fn closed(&self) -> bool {
        let mut edges: std::collections::BTreeMap<(u32, u32), usize> = Default::default();
        for t in &self.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        !edges.is_empty() && edges.values().all(|&n| n == 2)
    }
}

/// The shape of an obstacle in the work coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObstacleShape {
    Box { bounds: Bounds },
    Mesh { mesh: TriMesh },
}

/// Something the spindle assembly must not run into.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Obstacle {
    pub kind: ObstacleKind,
    /// In words, for messages: "the stock", "the vise", "the left clamp".
    pub name: String,
    pub shape: ObstacleShape,
}

impl Obstacle {
    /// The stock as a box (work coordinates).
    pub fn stock_box(bounds: Bounds) -> Obstacle {
        Obstacle {
            kind: ObstacleKind::Stock,
            name: "the stock".into(),
            shape: ObstacleShape::Box { bounds },
        }
    }

    /// The stock as a body's mesh (work coordinates).
    pub fn stock_mesh(mesh: TriMesh) -> Obstacle {
        Obstacle {
            kind: ObstacleKind::Stock,
            name: "the stock".into(),
            shape: ObstacleShape::Mesh { mesh },
        }
    }

    /// A fixture's mesh (work coordinates), named in words ("the vise").
    pub fn fixture(name: &str, mesh: TriMesh) -> Obstacle {
        Obstacle {
            kind: ObstacleKind::Fixture,
            name: name.to_string(),
            shape: ObstacleShape::Mesh { mesh },
        }
    }

    /// The machine's keep-out zones in the work coordinates of the
    /// placement.
    pub fn zones(machine: &Machine, placement: &Placement) -> Vec<Obstacle> {
        let o = placement.work_origin;
        machine
            .zones
            .iter()
            .map(|z| Obstacle {
                kind: ObstacleKind::Zone,
                name: z.name.clone(),
                shape: ObstacleShape::Box {
                    bounds: Bounds {
                        min: sub(z.bounds.min, o),
                        max: sub(z.bounds.max, o),
                    },
                },
            })
            .collect()
    }
}

/// How the check takes the stock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockModel {
    /// As the program has cut it so far: a height map ([`crate::stock`])
    /// cut by each move after the move is checked. The parts that do not
    /// cut meet the material left; the cutter at a rapid cuts the stock
    /// when the simulation takes more than
    /// [`crate::stock::RAPID_TOLERANCE`] off it.
    #[default]
    AsCut,
    /// As it is before the cut (conservative): the stock's box or mesh
    /// whole throughout.
    BeforeCut,
}

/// Settings of the check.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CollisionOptions {
    /// How far the pieces that hold an arc may stray from it (mm): the
    /// clearance found on an arc is at most this below the true one.
    #[serde(default = "default_arc_tolerance")]
    pub arc_tolerance: f64,
    /// The stock as cut so far (the default) or as before the cut.
    #[serde(default)]
    pub stock: StockModel,
    /// The step of the stock's height map (mm) with the stock as cut; none:
    /// an eighth of the smallest tool's diameter.
    #[serde(default)]
    pub resolution: Option<f64>,
    /// The most nodes of the stock's height map.
    #[serde(default = "default_max_nodes")]
    pub max_nodes: usize,
}

fn default_arc_tolerance() -> f64 {
    0.001
}

fn default_max_nodes() -> usize {
    crate::stock::MAX_NODES
}

impl Default for CollisionOptions {
    fn default() -> Self {
        CollisionOptions {
            arc_tolerance: default_arc_tolerance(),
            stock: StockModel::default(),
            resolution: None,
            max_nodes: default_max_nodes(),
        }
    }
}

impl CollisionOptions {
    /// The conservative check: the stock as it is before the cut.
    pub fn before_cut() -> Self {
        CollisionOptions {
            stock: StockModel::BeforeCut,
            ..CollisionOptions::default()
        }
    }
}

/// A part of the spindle assembly running into an obstacle: the first
/// contact of the part with the obstacle in the operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Collision {
    /// In words for people: "Pocket 1: the collet nut runs into the
    /// stock."
    pub message: String,
    /// The operation's index in the program.
    pub operation: usize,
    /// The move's index in the operation's toolpath.
    #[serde(rename = "move")]
    pub move_index: usize,
    /// Which line of the move: a rapid is written as up to two lines (up
    /// then across, or across then down); 0 otherwise.
    pub leg: usize,
    /// The move's line in the posted program (1-based), when known.
    #[serde(default)]
    pub line: Option<usize>,
    pub part: PartKind,
    /// The part's name (its component in the spindle design), for agents.
    pub part_name: String,
    pub obstacle: ObstacleKind,
    /// The obstacle's index in the obstacles checked.
    pub obstacle_index: usize,
    /// Whether the move is a rapid.
    pub rapid: bool,
    /// The tool tip's position at the contact (work coordinates).
    pub at: P3,
    /// On the tool change before the operation (its `move` is then 0 and
    /// its `line` the changer's line or the travel's in the posted
    /// program).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<ChangeMove>,
}

/// The smallest distance between a part and an obstacle along the program
/// (mm), and where.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clearance {
    pub distance: f64,
    pub operation: usize,
    #[serde(rename = "move")]
    pub move_index: usize,
    pub part: PartKind,
    pub obstacle_index: usize,
    /// The tool tip's position there (work coordinates).
    pub at: P3,
    /// On the tool change before the operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<ChangeMove>,
}

/// What the check found.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CollisionReport {
    /// In the program's order.
    pub collisions: Vec<Collision>,
    /// The smallest clearance of the parts that are checked (none: nothing
    /// was checked); 0 when something collides.
    pub clearance: Option<Clearance>,
}

impl CollisionReport {
    /// Fills in the collisions' lines from the posted program's lines of
    /// the moves ([`crate::post::post_with_lines`]).
    pub fn set_lines(&mut self, lines: &[Vec<MoveLines>]) {
        // A tool change's contacts carry their own lines.
        for c in self.collisions.iter_mut().filter(|c| c.change.is_none()) {
            c.line = lines
                .get(c.operation)
                .and_then(|op| op.get(c.move_index))
                .filter(|l| l.count > 0)
                .map(|l| l.first + c.leg.min(l.count - 1));
        }
    }
}

// Convex pieces of the parts and obstacles.

/// An upright frustum about the axis through the origin of its frame.
#[derive(Clone, Copy, Debug)]
struct Cone(Frustum);

fn disc_support(z: f64, r: f64, d: P3) -> P3 {
    let l = d[0].hypot(d[1]);
    if l > 0.0 && r > 0.0 {
        [r * d[0] / l, r * d[1] / l, z]
    } else {
        [0.0, 0.0, z]
    }
}

impl Convex for Cone {
    fn support(&self, d: P3) -> (P3, P3) {
        let f = self.0;
        let a = disc_support(f.z0, f.r0, d);
        let b = disc_support(f.z1, f.r1, d);
        (if dot(b, d) > dot(a, d) { b } else { a }, [0.0; 3])
    }
}

#[derive(Clone, Copy, Debug)]
struct Triangle([P3; 3]);

impl Convex for Triangle {
    fn support(&self, d: P3) -> (P3, P3) {
        let t = &self.0;
        let mut best = t[0];
        for p in &t[1..] {
            if dot(*p, d) > dot(best, d) {
                best = *p;
            }
        }
        (best, [0.0; 3])
    }
}

#[derive(Clone, Copy, Debug)]
struct Cuboid(Bounds);

impl Convex for Cuboid {
    fn support(&self, d: P3) -> (P3, P3) {
        let b = &self.0;
        (
            std::array::from_fn(|k| if d[k] >= 0.0 { b.max[k] } else { b.min[k] }),
            [0.0; 3],
        )
    }
}

/// The material over a node of the stock's height map: a vertical line
/// from the stock's bottom up to the node's height (the map knows nothing
/// between its nodes).
#[derive(Clone, Copy, Debug)]
struct Column([P3; 2]);

impl Convex for Column {
    fn support(&self, d: P3) -> (P3, P3) {
        let [a, b] = self.0;
        (if dot(b, d) > dot(a, d) { b } else { a }, [0.0; 3])
    }
}

#[derive(Clone, Copy, Debug)]
enum Piece {
    Cone(Cone),
    Triangle(Triangle),
    Cuboid(Cuboid),
    Column(Column),
}

impl Piece {
    fn convex(&self) -> &dyn Convex {
        match self {
            Piece::Cone(c) => c,
            Piece::Triangle(t) => t,
            Piece::Cuboid(b) => b,
            Piece::Column(c) => c,
        }
    }

    fn aabb(&self) -> Bounds {
        match *self {
            Piece::Cone(Cone(f)) => {
                let r = f.r0.max(f.r1);
                Bounds {
                    min: [-r, -r, f.z0],
                    max: [r, r, f.z1],
                }
            }
            Piece::Triangle(Triangle(t)) => points_aabb(&t),
            Piece::Cuboid(Cuboid(b)) => b,
            Piece::Column(Column(c)) => points_aabb(&c),
        }
    }
}

fn points_aabb(points: &[P3]) -> Bounds {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in points {
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    Bounds { min, max }
}

/// The distance between two boxes (0 when they overlap).
fn aabb_gap(a: &Bounds, b: &Bounds) -> f64 {
    let mut s = 0.0;
    for k in 0..3 {
        let g = (a.min[k] - b.max[k]).max(b.min[k] - a.max[k]).max(0.0);
        s += g * g;
    }
    s.sqrt()
}

/// A part's piece (in the tip's frame) swept over the convex hull of the
/// tip's positions `hull`.
struct Swept<'a> {
    part: &'a dyn Convex,
    hull: &'a [P3],
}

impl Convex for Swept<'_> {
    fn support(&self, d: P3) -> (P3, P3) {
        let (p, _) = self.part.support(d);
        let mut best = self.hull[0];
        for h in &self.hull[1..] {
            if dot(*h, d) > dot(best, d) {
                best = *h;
            }
        }
        (add(p, best), best)
    }
}

// The tip's path.

/// A leg of the tip's path: a straight line (a point when its ends meet)
/// or an arc about a vertical axis, a helix when its height changes.
#[derive(Clone, Copy, Debug)]
enum Leg {
    Line {
        a: P3,
        b: P3,
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        start: f64,
        sweep: f64,
        z0: f64,
        z1: f64,
    },
}

impl Leg {
    fn point(&self, s: f64) -> P3 {
        match *self {
            Leg::Line { a, b } => [
                a[0] + (b[0] - a[0]) * s,
                a[1] + (b[1] - a[1]) * s,
                a[2] + (b[2] - a[2]) * s,
            ],
            Leg::Arc {
                center,
                radius,
                start,
                sweep,
                z0,
                z1,
            } => {
                let a = start + sweep * s;
                [
                    center[0] + radius * a.cos(),
                    center[1] + radius * a.sin(),
                    z0 + (z1 - z0) * s,
                ]
            }
        }
    }

    /// The part of the leg from `s0` to `s1` (fractions of it).
    fn part(&self, s0: f64, s1: f64) -> Leg {
        match *self {
            Leg::Line { .. } => Leg::Line {
                a: self.point(s0),
                b: self.point(s1),
            },
            Leg::Arc {
                center,
                radius,
                start,
                sweep,
                z0,
                z1,
            } => Leg::Arc {
                center,
                radius,
                start: start + sweep * s0,
                sweep: sweep * (s1 - s0),
                z0: z0 + (z1 - z0) * s0,
                z1: z0 + (z1 - z0) * s1,
            },
        }
    }

    /// Points whose convex hull holds the leg: a line's ends; an arc (of a
    /// quarter turn at most) its ends and where their tangents meet, at
    /// both its heights.
    fn hull(&self) -> Vec<P3> {
        match *self {
            Leg::Line { a, b } => vec![a, b],
            Leg::Arc {
                center,
                radius,
                start,
                sweep,
                z0,
                z1,
            } => {
                let half = sweep / 2.0;
                let mid = start + half;
                let apex = radius / half.cos().max(1e-12);
                let xy = [
                    [
                        center[0] + radius * start.cos(),
                        center[1] + radius * start.sin(),
                    ],
                    [center[0] + apex * mid.cos(), center[1] + apex * mid.sin()],
                    [
                        center[0] + radius * (start + sweep).cos(),
                        center[1] + radius * (start + sweep).sin(),
                    ],
                ];
                let mut pts = Vec::with_capacity(6);
                for z in [z0, z1] {
                    for p in &xy {
                        pts.push([p[0], p[1], z]);
                    }
                    if z1 == z0 {
                        break;
                    }
                }
                pts
            }
        }
    }

    /// How far the hull may be from the leg: 0 for a line; for an arc the
    /// tangents' meeting point's distance from the arc, or half its rise.
    fn looseness(&self) -> f64 {
        match *self {
            Leg::Line { .. } => 0.0,
            Leg::Arc {
                radius,
                sweep,
                z0,
                z1,
                ..
            } => {
                let bulge = radius * (1.0 / (sweep / 2.0).cos().max(1e-12) - 1.0);
                bulge.max((z1 - z0).abs() / 2.0)
            }
        }
    }

    fn length(&self) -> f64 {
        match *self {
            Leg::Line { a, b } => mitcad_vec3::distance(a, b),
            Leg::Arc {
                radius,
                sweep,
                z0,
                z1,
                ..
            } => (radius * sweep).hypot(z1 - z0),
        }
    }
}

/// The legs of a move from `from`, as the post writes it: a rapid up first
/// when it rises and across first when it sinks; an arc in pieces of a
/// quarter turn at most. Each leg with its line in the move (0 or 1).
fn legs(from: P3, m: &Move) -> Vec<(Leg, usize)> {
    match *m {
        Move::Rapid { to } => {
            let corner = if to[2] > from[2] + 1e-9 {
                [from[0], from[1], to[2]]
            } else {
                [to[0], to[1], from[2]]
            };
            let mut out = Vec::new();
            let mut line = 0;
            for (a, b) in [(from, corner), (corner, to)] {
                if mitcad_vec3::distance(a, b) > 1e-12 {
                    out.push((Leg::Line { a, b }, line));
                    line += 1;
                }
            }
            out
        }
        Move::Line { to, .. } => vec![(Leg::Line { a: from, b: to }, 0)],
        Move::Arc {
            to, center, ccw, ..
        } => {
            let radius = (from[0] - center[0]).hypot(from[1] - center[1]);
            let sweep = arc_sweep([from[0], from[1]], [to[0], to[1]], center, ccw);
            let start = (from[1] - center[1]).atan2(from[0] - center[0]);
            if radius <= 1e-12 {
                return vec![(Leg::Line { a: from, b: to }, 0)];
            }
            let whole = Leg::Arc {
                center,
                radius,
                start,
                sweep,
                z0: from[2],
                z1: to[2],
            };
            let n = ((sweep.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
            (0..n)
                .map(|i| {
                    (
                        whole.part(i as f64 / n as f64, (i + 1) as f64 / n as f64),
                        0,
                    )
                })
                .collect()
        }
    }
}

// The check.

/// A part's convex piece with its box (tip frame).
struct PartPiece {
    piece: Piece,
    aabb: Bounds,
}

struct PreparedPart {
    kind: PartKind,
    name: String,
    cutting: bool,
    margin: f64,
    pieces: Vec<PartPiece>,
    /// A point inside the part (tip frame), for the inside test.
    anchor: P3,
}

struct PreparedObstacle {
    kind: ObstacleKind,
    name: String,
    pieces: Vec<(Piece, Bounds)>,
    /// The triangles of a closed mesh, for the inside test.
    closed: Option<Vec<[P3; 3]>>,
    /// The stock as cut: its height map's index in the check's fields (no
    /// `pieces` then).
    field: Option<usize>,
}

/// The stock as cut so far (`StockModel::AsCut`): its height map, the
/// highest node of each block for finding the columns near a part, and the
/// obstacle it stands for.
struct Field {
    map: HeightMap,
    pyramid: MaxPyramid,
    obstacle: usize,
}

impl Field {
    fn new(map: HeightMap, obstacle: usize) -> Field {
        Field {
            pyramid: MaxPyramid::new(&map),
            map,
            obstacle,
        }
    }

    /// The stock's height map of an obstacle: a box, or a body's mesh seen
    /// from above (its highest surface over each node, on its lowest
    /// point).
    fn of(obstacle: &Obstacle, finest: f64, max_nodes: usize) -> Option<HeightMap> {
        match &obstacle.shape {
            ObstacleShape::Box { bounds } => Some(HeightMap::of_box(bounds, finest, max_nodes)),
            ObstacleShape::Mesh { mesh } => {
                let used: Vec<P3> = mesh
                    .triangles
                    .iter()
                    .flatten()
                    .filter_map(|&i| mesh.vertices.get(i as usize).copied())
                    .collect();
                if used.is_empty() {
                    return None;
                }
                let b = points_aabb(&used);
                let grid = Grid::for_stock(
                    [b.min[0], b.min[1]],
                    [b.max[0], b.max[1]],
                    finest,
                    max_nodes,
                );
                Some(HeightMap::top_of_mesh(grid, b.min[2], mesh))
            }
        }
    }

    /// Whether a point is in the material.
    fn inside(&self, p: P3) -> bool {
        self.map
            .height_at(p[0], p[1])
            .is_some_and(|z| z > self.map.bottom && p[2] < z && p[2] > self.map.bottom)
    }

    /// Cuts the map by the cutter's end along a leg of the tip, arcs as
    /// chords within a quarter of the grid's step.
    fn cut(&mut self, cutter: &Cutter, leg: &Leg) -> Removal {
        let removal = match *leg {
            Leg::Line { a, b } => self.map.cut_line(cutter, a, b),
            Leg::Arc { radius, sweep, .. } => {
                let g = self.map.grid;
                let tol = (g.step[0].min(g.step[1]) / 4.0).clamp(1e-4, radius.max(1e-4));
                let step = 2.0 * (1.0 - tol / radius.max(1e-12)).clamp(-1.0, 1.0).acos();
                let n = ((sweep.abs() / step.max(1e-3)).ceil() as usize).clamp(1, 1 << 16);
                let mut out = Removal::default();
                let mut at = leg.point(0.0);
                for k in 1..=n {
                    let s = k as f64 / n as f64;
                    let p = leg.point(s);
                    let piece = self.map.cut_line(cutter, at, p);
                    out.add(piece, (k - 1) as f64 / n as f64, s);
                    at = p;
                }
                out
            }
        };
        if let Some(nodes) = removal.nodes {
            self.pyramid.update(&self.map, nodes);
        }
        removal
    }

    /// Calls `visit` with the columns of the map that a part's piece swept
    /// over `swept` (its box) may come within the smallest clearance so far
    /// of, or touch, nearest first: the blocks of the pyramid by their
    /// boxes' distance, until the nearest left is farther than that.
    fn near(
        &self,
        swept: &Bounds,
        margin: f64,
        best: &mut Best,
        mut visit: impl FnMut(&Piece, &Bounds, &mut Best),
    ) {
        let g = self.map.grid;
        let bottom = self.map.bottom;
        let block_box = |level: usize, i: usize, j: usize| -> Option<Bounds> {
            let top = self.pyramid.max(level, i, j);
            if top <= bottom + 1e-9 {
                return None;
            }
            let [i0, i1, j0, j1] = self.pyramid.block(level, i, j);
            Some(Bounds {
                min: [g.x(i0), g.y(j0), bottom],
                max: [g.x(i1), g.y(j1), top],
            })
        };
        let mut heap: BinaryHeap<Near> = BinaryHeap::new();
        let top = self.pyramid.top();
        if let Some(b) = block_box(top, 0, 0) {
            heap.push(Near {
                gap: aabb_gap(swept, &b) - margin,
                level: top,
                i: 0,
                j: 0,
            });
        }
        while let Some(n) = heap.pop() {
            if n.gap > TOUCH && n.gap >= best.value() {
                break;
            }
            if n.level == 0 {
                let (x, y) = (g.x(n.i), g.y(n.j));
                let column = Piece::Column(Column([[x, y, bottom], [x, y, self.map.at(n.i, n.j)]]));
                let b = column.aabb();
                visit(&column, &b, best);
                continue;
            }
            let (nx, ny, _) = &self.pyramid.levels[n.level - 1];
            for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (i, j) = (2 * n.i + di, 2 * n.j + dj);
                if i >= *nx || j >= *ny {
                    continue;
                }
                if let Some(b) = block_box(n.level - 1, i, j) {
                    heap.push(Near {
                        gap: aabb_gap(swept, &b) - margin,
                        level: n.level - 1,
                        i,
                        j,
                    });
                }
            }
        }
    }
}

/// A block of a field's pyramid by its box's distance from a swept piece,
/// the nearest first out of the heap.
struct Near {
    gap: f64,
    level: usize,
    i: usize,
    j: usize,
}

impl PartialEq for Near {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Near {}

impl PartialOrd for Near {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Near {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed: the max-heap gives the smallest gap first; then the
        // finer level, then the place, for a fixed order.
        other
            .gap
            .total_cmp(&self.gap)
            .then(other.level.cmp(&self.level))
            .then(other.j.cmp(&self.j))
            .then(other.i.cmp(&self.i))
    }
}

fn prepare_part(p: &crate::spindle::SpindlePart) -> PreparedPart {
    let pieces: Vec<PartPiece> = match &p.shape {
        PartShape::Revolved { sections } => sections
            .iter()
            .filter(|s| s.z1 > s.z0)
            .map(|s| {
                let piece = Piece::Cone(Cone(*s));
                PartPiece {
                    aabb: piece.aabb(),
                    piece,
                }
            })
            .collect(),
        PartShape::Mesh { triangles } => triangles
            .iter()
            .map(|t| {
                let piece = Piece::Triangle(Triangle(*t));
                PartPiece {
                    aabb: piece.aabb(),
                    piece,
                }
            })
            .collect(),
    };
    let anchor = match &p.shape {
        PartShape::Revolved { sections } => sections
            .iter()
            .find(|s| s.z1 > s.z0)
            .map(|s| [0.0, 0.0, (s.z0 + s.z1) / 2.0])
            .unwrap_or([0.0; 3]),
        PartShape::Mesh { triangles } => triangles.first().map(|t| t[0]).unwrap_or([0.0; 3]),
    };
    PreparedPart {
        kind: p.kind,
        name: p.name.clone(),
        cutting: p.cutting,
        margin: p.margin.max(0.0),
        pieces,
        anchor,
    }
}

fn prepare_obstacle(o: &Obstacle) -> PreparedObstacle {
    let (pieces, closed) = match &o.shape {
        ObstacleShape::Box { bounds } => (vec![(Piece::Cuboid(Cuboid(*bounds)), *bounds)], None),
        ObstacleShape::Mesh { mesh } => {
            let faces = mesh.faces();
            let pieces = faces
                .iter()
                .map(|t| {
                    let piece = Piece::Triangle(Triangle(*t));
                    (piece, piece.aabb())
                })
                .collect();
            (pieces, mesh.closed().then_some(faces))
        }
    };
    PreparedObstacle {
        kind: o.kind,
        name: o.name.clone(),
        pieces,
        closed,
        field: None,
    }
}

/// Whether `p` is inside the closed surface: the triangles a ray from it
/// crosses, odd. The ray's direction is chosen off the axes so that it
/// does not run along the edges of meshes made on a grid.
fn inside(p: P3, faces: &[[P3; 3]]) -> bool {
    let d = [0.8723_f64, 0.3961, 0.2867];
    let mut crossings = 0usize;
    for t in faces {
        let e1 = sub(t[1], t[0]);
        let e2 = sub(t[2], t[0]);
        let h = cross(d, e2);
        let det = dot(e1, h);
        if det.abs() < 1e-15 {
            continue;
        }
        let s = sub(p, t[0]);
        let u = dot(s, h) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = cross(s, e1);
        let v = dot(d, q) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        if dot(e2, q) / det > 0.0 {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

/// The smallest clearance found so far and where.
struct Best {
    clearance: Option<Clearance>,
}

impl Best {
    fn value(&self) -> f64 {
        self.clearance
            .as_ref()
            .map_or(f64::INFINITY, |c| c.distance)
    }
}

/// The lower bound of the clearance between a part's piece swept along
/// `leg` and an obstacle's piece, and the tip's place there: one distance
/// for a line, for an arc refined where it may come below `best` until its
/// pieces are within `tolerance` of it.
fn leg_clearance(
    leg: &Leg,
    part: &Piece,
    margin: f64,
    obstacle: &Piece,
    tolerance: f64,
    best: f64,
    depth: usize,
) -> (f64, P3) {
    let hull = leg.hull();
    let swept = Swept {
        part: part.convex(),
        hull: &hull,
    };
    let d = distance(&swept, obstacle.convex());
    let lower = (d.lower - margin).max(0.0);
    let loose = leg.looseness();
    if loose <= tolerance || (lower > TOUCH && lower >= best) || depth >= 40 {
        let at = if loose == 0.0 { d.tag } else { leg.point(0.5) };
        return (lower, at);
    }
    let first = leg_clearance(
        &leg.part(0.0, 0.5),
        part,
        margin,
        obstacle,
        tolerance,
        best,
        depth + 1,
    );
    let second = leg_clearance(
        &leg.part(0.5, 1.0),
        part,
        margin,
        obstacle,
        tolerance,
        best.min(first.0),
        depth + 1,
    );
    if second.0 < first.0 { second } else { first }
}

/// Whether the piece swept along the leg touches the obstacle's piece.
fn touches(leg: &Leg, part: &Piece, margin: f64, obstacle: &Piece, tolerance: f64) -> bool {
    leg_clearance(leg, part, margin, obstacle, tolerance, TOUCH, 0).0 <= TOUCH
}

/// The first place along the leg (a fraction of it) where the piece
/// touches the obstacle's piece, the leg known to touch it somewhere.
fn first_touch(leg: &Leg, part: &Piece, margin: f64, obstacle: &Piece, tolerance: f64) -> f64 {
    if touches(&leg.part(0.0, 0.0), part, margin, obstacle, tolerance) {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    // To a nanometre of the leg's length, or 60 halvings.
    let length = leg.length().max(1e-9);
    for _ in 0..60 {
        if (hi - lo) * length <= 1e-9 {
            break;
        }
        let mid = (lo + hi) / 2.0;
        if touches(&leg.part(0.0, mid), part, margin, obstacle, tolerance) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// A plunge at a feed: x, y, its lowest and its highest z.
type Plunge = (f64, f64, f64, f64);

/// Where the program goes, operation by operation, for [`check_collisions`].
struct LegAt {
    operation: usize,
    move_index: usize,
    line: usize,
    rapid: bool,
    leg: Leg,
}

/// Checks the program's moves for collisions of the spindle assembly (the
/// spindle `model` with each operation's tool) with the obstacles (the
/// module's documentation). One collision per operation, part and
/// obstacle: the first.
pub fn check_collisions(
    program: &Program,
    model: &SpindleModel,
    obstacles: &[Obstacle],
    options: &CollisionOptions,
) -> CollisionReport {
    let tolerance = if options.arc_tolerance.is_finite() && options.arc_tolerance > 0.0 {
        options.arc_tolerance
    } else {
        default_arc_tolerance()
    };
    // The stock as cut: each stock obstacle as a height map, on a grid as
    // fine as an eighth of the smallest tool (or the resolution asked).
    let as_cut = options.stock == StockModel::AsCut;
    let finest = options
        .resolution
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or_else(|| crate::stock::finest_step(program));
    let mut fields: Vec<Field> = Vec::new();
    let prepared: Vec<PreparedObstacle> = obstacles
        .iter()
        .enumerate()
        .map(|(bi, o)| {
            if as_cut
                && o.kind == ObstacleKind::Stock
                && let Some(map) = Field::of(o, finest, options.max_nodes)
            {
                fields.push(Field::new(map, bi));
                return PreparedObstacle {
                    kind: o.kind,
                    name: o.name.clone(),
                    pieces: Vec::new(),
                    closed: None,
                    field: Some(fields.len() - 1),
                };
            }
            prepare_obstacle(o)
        })
        .collect();
    let mut report = CollisionReport::default();
    let mut best = Best { clearance: None };
    let mut previous: Option<(u32, P3)> = None;
    for (oi, op) in program.operations.iter().enumerate() {
        let Some(first) = op.toolpath.moves.first() else {
            continue;
        };
        let assembly = SpindleAssembly::new(model, &op.tool);
        let parts: Vec<PreparedPart> = assembly.parts.iter().map(prepare_part).collect();
        let cutter = Cutter::of(&op.tool);
        let mut done = vec![vec![false; prepared.len()]; parts.len()];
        let mut plunges: Vec<Plunge> = Vec::new();
        let first_to = first.to();
        let mut at = match previous {
            // The same tool goes on from where the last operation left it.
            Some((tool, end)) if tool == op.tool.number => end,
            _ => {
                // After a tool change: above the first point at the safe
                // height, checked there before it moves.
                let above = [first_to[0], first_to[1], first_to[2].max(program.safe_z)];
                let start = LegAt {
                    operation: oi,
                    move_index: 0,
                    line: 0,
                    rapid: true,
                    leg: Leg::Line { a: above, b: above },
                };
                check_leg(
                    &start,
                    &parts,
                    &prepared,
                    &fields,
                    &plunges,
                    tolerance,
                    &mut done,
                    &mut best,
                    &mut report,
                    op,
                    true,
                );
                cut_fields(
                    &start,
                    &cutter,
                    &parts,
                    &prepared,
                    &mut fields,
                    &mut done,
                    &mut best,
                    &mut report,
                    op,
                );
                above
            }
        };
        for (mi, m) in op.toolpath.moves.iter().enumerate() {
            let rapid = matches!(m, Move::Rapid { .. });
            for (leg, line) in legs(at, m) {
                let leg_at = LegAt {
                    operation: oi,
                    move_index: mi,
                    line,
                    rapid,
                    leg,
                };
                check_leg(
                    &leg_at,
                    &parts,
                    &prepared,
                    &fields,
                    &plunges,
                    tolerance,
                    &mut done,
                    &mut best,
                    &mut report,
                    op,
                    false,
                );
                // Then the move cuts the stock for what comes after it.
                cut_fields(
                    &leg_at,
                    &cutter,
                    &parts,
                    &prepared,
                    &mut fields,
                    &mut done,
                    &mut best,
                    &mut report,
                    op,
                );
            }
            if let Move::Line { to, .. } = *m
                && (to[0] - at[0]).hypot(to[1] - at[1]) <= 1e-9
            {
                plunges.push((at[0], at[1], at[2].min(to[2]), at[2].max(to[2])));
            }
            at = m.to();
        }
        previous = Some((op.tool.number, at));
    }
    report.clearance = best.clearance;
    report
}

/// Whether a leg goes straight up: the cutter then moves only through
/// the column the tool above it already fills, which holds no stock that
/// the rest of the tool would not have run into first.
fn rises_straight_up(leg: &Leg) -> bool {
    matches!(*leg, Leg::Line { a, b } if (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-9 && b[2] > a[2])
}

/// Whether a rapid leg runs only where a plunge of the operation already
/// cut: straight up or down within one.
fn on_a_plunge(leg: &Leg, plunges: &[Plunge]) -> bool {
    let Leg::Line { a, b } = *leg else {
        return false;
    };
    if (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-9 {
        return false;
    }
    let (lo, hi) = (a[2].min(b[2]), a[2].max(b[2]));
    plunges.iter().any(|&(x, y, z0, z1)| {
        (a[0] - x).hypot(a[1] - y) <= 1e-6 && lo >= z0 - 1e-6 && hi <= z1 + 1e-6
    })
}

/// A part's piece swept along a leg against an obstacle's piece: the
/// clearance kept when it is the smallest so far, and the first touch
/// along the leg in `contact`; with `clearance_only`, a touch is neither
/// (the stock as cut decides the cutter's contacts at a rapid).
#[allow(clippy::too_many_arguments)]
fn examine(
    at: &LegAt,
    part: &PreparedPart,
    pp: &PartPiece,
    swept_box: &Bounds,
    obstacle: (usize, &Piece, &Bounds),
    tolerance: f64,
    clearance_only: bool,
    best: &mut Best,
    contact: &mut Option<f64>,
) {
    let (bi, piece, piece_box) = obstacle;
    let gap = aabb_gap(swept_box, piece_box) - part.margin;
    if gap > TOUCH && gap >= best.value() {
        return;
    }
    let (lower, place) = leg_clearance(
        &at.leg,
        &pp.piece,
        part.margin,
        piece,
        tolerance,
        best.value(),
        0,
    );
    if clearance_only && lower <= TOUCH {
        return;
    }
    if lower < best.value() {
        best.clearance = Some(Clearance {
            distance: lower,
            operation: at.operation,
            move_index: at.move_index,
            part: part.kind,
            obstacle_index: bi,
            at: place,
            change: None,
        });
    }
    if lower <= TOUCH {
        let s = first_touch(&at.leg, &pp.piece, part.margin, piece, tolerance);
        *contact = Some(contact.map_or(s, |c| c.min(s)));
    }
}

#[allow(clippy::too_many_arguments)]
fn check_leg(
    at: &LegAt,
    parts: &[PreparedPart],
    obstacles: &[PreparedObstacle],
    fields: &[Field],
    plunges: &[Plunge],
    tolerance: f64,
    done: &mut [Vec<bool>],
    best: &mut Best,
    report: &mut CollisionReport,
    op: &crate::program::ProgramOp,
    start: bool,
) {
    let hull_box = points_aabb(&at.leg.hull());
    // (where along the leg, part, obstacle)
    let mut contacts: Vec<(f64, usize, usize)> = Vec::new();
    for (pi, part) in parts.iter().enumerate() {
        for (bi, obstacle) in obstacles.iter().enumerate() {
            if done[pi][bi] {
                continue;
            }
            if obstacle.kind == ObstacleKind::Stock
                && part.cutting
                && (!at.rapid || rises_straight_up(&at.leg) || on_a_plunge(&at.leg, plunges))
            {
                continue;
            }
            let field = obstacle.field.and_then(|f| fields.get(f));
            // The cutter at a rapid against the stock as cut: the
            // simulation says whether it cuts (`cut_fields`); its distance
            // here only counts for the clearance.
            let clearance_only = field.is_some() && part.cutting;
            let mut contact: Option<f64> = None;
            if start
                && let Some(faces) = &obstacle.closed
                && inside(add(part.anchor, at.leg.point(0.0)), faces)
            {
                contact = Some(0.0);
            }
            if start
                && !clearance_only
                && let Some(f) = field
                && f.inside(add(part.anchor, at.leg.point(0.0)))
            {
                contact = Some(0.0);
            }
            for pp in &part.pieces {
                let swept_box = Bounds {
                    min: add(pp.aabb.min, hull_box.min),
                    max: add(pp.aabb.max, hull_box.max),
                };
                match field {
                    None => {
                        for (piece, piece_box) in &obstacle.pieces {
                            examine(
                                at,
                                part,
                                pp,
                                &swept_box,
                                (bi, piece, piece_box),
                                tolerance,
                                clearance_only,
                                best,
                                &mut contact,
                            );
                        }
                    }
                    Some(f) => f.near(&swept_box, part.margin, best, |piece, piece_box, best| {
                        examine(
                            at,
                            part,
                            pp,
                            &swept_box,
                            (bi, piece, piece_box),
                            tolerance,
                            clearance_only,
                            best,
                            &mut contact,
                        )
                    }),
                }
            }
            if let Some(s) = contact {
                contacts.push((s, pi, bi));
            }
        }
    }
    contacts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (s, pi, bi) in contacts {
        done[pi][bi] = true;
        let part = &parts[pi];
        let obstacle = &obstacles[bi];
        let place = at.leg.point(s);
        let message = if part.cutting && obstacle.kind == ObstacleKind::Stock {
            format!(
                "{}: a rapid move takes the cutter into {}.",
                op.name, obstacle.name
            )
        } else {
            format!(
                "{}: {} runs into {}.",
                op.name,
                part.kind.words(),
                obstacle.name
            )
        };
        if best.value() > 0.0 {
            best.clearance = Some(Clearance {
                distance: 0.0,
                operation: at.operation,
                move_index: at.move_index,
                part: part.kind,
                obstacle_index: bi,
                at: place,
                change: None,
            });
        }
        report.collisions.push(Collision {
            message,
            operation: at.operation,
            move_index: at.move_index,
            leg: at.line,
            line: None,
            part: part.kind,
            part_name: part.name.clone(),
            obstacle: obstacle.kind,
            obstacle_index: bi,
            rapid: at.rapid,
            at: place,
            change: None,
        });
    }
}

/// Cuts the stock as cut by the cutter along a leg just checked; a rapid
/// (or the start above an operation) that takes more than
/// [`RAPID_TOLERANCE`] off it is the cutter running into the stock, at the
/// place along the leg where it first cuts.
#[allow(clippy::too_many_arguments)]
fn cut_fields(
    at: &LegAt,
    cutter: &Cutter,
    parts: &[PreparedPart],
    obstacles: &[PreparedObstacle],
    fields: &mut [Field],
    done: &mut [Vec<bool>],
    best: &mut Best,
    report: &mut CollisionReport,
    op: &crate::program::ProgramOp,
) {
    let cutting = parts.iter().position(|p| p.cutting);
    for field in fields.iter_mut() {
        let removal = field.cut(cutter, &at.leg);
        let (Some(pi), true) = (cutting, at.rapid && removal.cut(RAPID_TOLERANCE)) else {
            continue;
        };
        let bi = field.obstacle;
        if done[pi][bi] {
            continue;
        }
        done[pi][bi] = true;
        let part = &parts[pi];
        let place = at.leg.point(removal.first.unwrap_or(0.0));
        if best.value() > 0.0 {
            best.clearance = Some(Clearance {
                distance: 0.0,
                operation: at.operation,
                move_index: at.move_index,
                part: part.kind,
                obstacle_index: bi,
                at: place,
                change: None,
            });
        }
        report.collisions.push(Collision {
            message: format!(
                "{}: a rapid move takes the cutter into {}.",
                op.name, obstacles[bi].name
            ),
            operation: at.operation,
            move_index: at.move_index,
            leg: at.line,
            line: None,
            part: part.kind,
            part_name: part.name.clone(),
            obstacle: ObstacleKind::Stock,
            obstacle_index: bi,
            rapid: true,
            at: place,
            change: None,
        });
    }
}
