// SPDX-License-Identifier: MIT
//! Approximate flat patterns of curved faces by triangulation
//! (mitcad#2442): Convert to Sheet Metal with `development: triangulated`.
//!
//! Faces that are not exactly developable (the warped faces of a
//! square-to-round or a rectangle-to-round transition, offset transitions,
//! a loft's faces between two sections) are laid out as duct work lays
//! them: the faces form a strip between two edges (the *rails*, a loft's
//! sections), the strip is cut by lines from one rail to the other (the
//! *rungs*, a loft's straight lines between its sections) into thin
//! quadrilaterals, each split along its shorter diagonal into two
//! triangles, and the triangles are laid flat one after another, each
//! keeping the lengths of its three sides. The user asks for it (it is
//! never used where the exact development of mitcad#2435 applies: one
//! flat, cylindrical or conical face is developed exactly, with a remark
//! that the approximation was not needed), and the pattern says how
//! approximate it is ([`SheetApproximation`]): the largest difference of a
//! length along its edges, and of a length across it, from the same length
//! on the faces, both at the neutral surface.
//!
//! - The faces: the picked face and the further `faces`, each joined to the
//!   next along a whole edge across the strip (a rung both have), in any
//!   order, each piece of a face the kernel split (a loft's side where the
//!   other section has a vertex) a face of its own; a strip that closes on
//!   itself (a transition all round, one
//!   face closed on itself) is opened at the picked face's first rung, the
//!   corner gap wide (half each side). The strip runs over each face's
//!   surface between its parameter bounds. The rungs run along the face's
//!   straighter parameter lines (a single open face), across the edges the
//!   faces share (several), or from rail to rail of a face closed on
//!   itself. The rails may meet where the strip starts or ends (a fan,
//!   mitcad#2558): the rung there is a point. Where they meet between its
//!   ends (mitcad#2574) the strip is two fans there, the next laid beside
//!   the one before, touching it at the point.
//! - Planar faces (mitcad#2574): a plane's parameter lines run along its
//!   own axes, not its edges, so a planar face's grid is built from its
//!   boundary instead ([`plane_rungs`], [`plane_grid`]): its rungs are the
//!   straight edges it shares with the faces beside it, its rails the edges
//!   between them, so that the triangles of a square-to-round built of
//!   planes and cones lie in one strip with its cones.
//! - Holes, and faces cut out of their surfaces (mitcad#2558): each point
//!   of a face's boundary is laid out with the triangle of its strip it
//!   lies in, keeping its place on the triangle's plane; a hole is a hole
//!   of the pattern, and the edges of a face cut out of its surface (an
//!   outer wire off its grid's bounding lines) make the outline with the
//!   other faces' rails, joined end to end. Each such edge is measured as
//!   the rails are.
//! - The triangles (64 by default) are shared among the faces by the
//!   lengths of their rails, at least two each. Where faces meet at an
//!   angle, each face's points at the neutral surface are moved towards
//!   the mean of the two faces' points at the shared rung, evenly along
//!   the face ([`Neutral`]), so the pattern there does not depend on the
//!   count.
//! - The neutral surface lies the rules' K-factor times the thickness from
//!   the faces' inner side (the side of the shorter surface: the front, or
//!   the surface the thickness behind it). The body must be a plate of the
//!   thickness behind the faces: its volume within 3 % of the thickness
//!   times the faces' area at the middle of the material.
//! - The sheet's one chart is the pattern, on the plane touching the
//!   picked face at the strip's first point; the folded body stays as it
//!   was. Its Flat Pattern is that chart; its fold lines (where two
//!   triangles meet at 0.01° or more) are written to the DXF on request.
//!   Sheet features refuse such a sheet ([`refuse`]), and ordinary features
//!   leave it without its sheet meaning.
//!
//! The outline's curves are generated curves `g<2000000+>` of the feature.

use mitcad_vec3::{add, cross, distance, dot, norm, scale, sub};
use serde_json::{Value, json};

use super::*;
use crate::datum::SurfaceGeometry;
use crate::features::face_refs::kernel_error;
use crate::features::sheet_convert::{DEFAULT_TRIANGLES, SheetConvertDef, SheetDevelopment};
use crate::kernel::{SheetFaceEdge, SheetFaceGrid, SheetFaceWire};
use crate::notice::NoticePlace;
use crate::topo::FaceName;

use mitcad_vec3::Vec3 as V3;

/// The outline's generated curves.
const OUTLINE: u32 = 2_000_000;
/// The steps of a face's grid between two rungs of the strip: the lengths
/// along the faces the pattern is compared with.
pub(crate) const FINE: usize = 8;
/// The steps of a face's grid across the strip, from rail to rail.
const ACROSS: usize = 16;
/// The steps of each parameter of the first look at a face.
const PROBE: usize = 16;
/// Edges of two faces meet within this (mm; the geometric tolerance of the
/// imports, mitcad#224).
const MEET: f64 = 0.01;
/// A face's grid covers the face's area within this share; less, and the
/// face is cut out of its surface.
const FILLS: f64 = 0.01;
/// The body's volume within this share of the plate's.
const PLATE: f64 = 0.03;
/// Two triangles meeting at less than this (radians: 0.01°) are no fold.
const FOLD: f64 = 1.745_329_251_994_33e-4;
/// Shorter than this (mm) is a point.
const POINT: f64 = 1e-9;
/// The steps of each edge of a face's boundary (even: its length is
/// corrected for its chords' error from every second point).
const EDGE_STEPS: usize = 64;
/// A point of a face's boundary within this share of the grid's size of
/// a line of the grid lies on it.
const ON_LINE: f64 = 1e-5;
/// The ends of two pieces of the pattern's outline join within this (mm).
const JOIN: f64 = 0.1;

/// A refusal about faces of the body: `said` in plain words for a person,
/// the places painted, `named` with their names for agents (mitcad#1612).
fn placed_at(
    places: Vec<NoticePlace>,
    said: impl Into<String>,
    named: impl Into<String>,
) -> String {
    crate::notice::FeatureError::new(said)
        .with_places(places)
        .with_detail(named)
        .text()
}

/// A sheet laid flat by triangulation is not changed by sheet features: its
/// chart is the pattern, not the folded faces.
pub(crate) fn refuse(state: &SheetState) -> Result<(), String> {
    match state.approximation {
        Some(_) => Err(
            "the sheet is laid flat approximately, by triangulation: sheet features do not \
             change it, as its pattern is not the folded faces' development"
                .into(),
        ),
        None => Ok(()),
    }
}

/// How a face's grid lies along the strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Orientation {
    /// Along the strip runs the face's second parameter (its rows).
    pub transpose: bool,
    /// Along the strip against its parameter.
    pub reverse_along: bool,
    /// From the strip's first rail to its second against its parameter.
    pub reverse_across: bool,
}

const ORIENTATIONS: [Orientation; 8] = {
    let mut all = [Orientation {
        transpose: false,
        reverse_along: false,
        reverse_across: false,
    }; 8];
    let mut k = 0;
    while k < 8 {
        all[k] = Orientation {
            transpose: k & 1 != 0,
            reverse_along: k & 2 != 0,
            reverse_across: k & 4 != 0,
        };
        k += 1;
    }
    all
};

/// A face's grid seen along the strip: point `(i, j)`, `i` along the strip
/// from its start, `j` across it from its first rail.
#[derive(Clone, Copy)]
pub(crate) struct Oriented<'a> {
    pub grid: &'a SheetFaceGrid,
    pub orientation: Orientation,
}

impl Oriented<'_> {
    fn along(&self) -> usize {
        if self.orientation.transpose {
            self.grid.rows
        } else {
            self.grid.columns
        }
    }

    fn across(&self) -> usize {
        if self.orientation.transpose {
            self.grid.columns
        } else {
            self.grid.rows
        }
    }

    fn index(&self, i: usize, j: usize) -> usize {
        let i = if self.orientation.reverse_along {
            self.along() - i
        } else {
            i
        };
        let j = if self.orientation.reverse_across {
            self.across() - j
        } else {
            j
        };
        let (column, row) = if self.orientation.transpose {
            (j, i)
        } else {
            (i, j)
        };
        row * (self.grid.columns + 1) + column
    }

    fn point(&self, i: usize, j: usize) -> V3 {
        self.grid.points[self.index(i, j)]
    }

    fn normal(&self, i: usize, j: usize) -> V3 {
        self.grid.normals[self.index(i, j)]
    }

    /// The point `depth` behind the face (against its normal).
    fn behind(&self, i: usize, j: usize, depth: f64) -> V3 {
        sub(self.point(i, j), scale(self.normal(i, j), depth))
    }

    /// The rung at `i`: its ends on the first and the second rail.
    fn rung(&self, i: usize) -> [V3; 2] {
        [self.point(i, 0), self.point(i, self.across())]
    }

    /// A place in the grid (`[x, y]` as [`SheetFaceEdge::cells`] gives
    /// it) seen along the strip: `[along, across]` in steps of the grid.
    fn place(&self, cell: [f64; 2]) -> [f64; 2] {
        let (a, b) = if self.orientation.transpose {
            (cell[1], cell[0])
        } else {
            (cell[0], cell[1])
        };
        let a = if self.orientation.reverse_along {
            self.along() as f64 - a
        } else {
            a
        };
        let b = if self.orientation.reverse_across {
            self.across() as f64 - b
        } else {
            b
        };
        [a, b]
    }

    /// The face's outer wire, when its boundary was sampled.
    fn outer(&self) -> Option<&SheetFaceWire> {
        self.grid.wires.iter().find(|w| w.outer)
    }

    /// Where an edge of the face's boundary runs in its grid.
    fn line_of(&self, edge: &SheetFaceEdge) -> Line {
        let (along, across) = (self.along() as f64, self.across() as f64);
        let places: Vec<[f64; 2]> = edge.cells.iter().map(|&c| self.place(c)).collect();
        let all = |on: &dyn Fn([f64; 2]) -> bool| places.iter().all(|&p| on(p));
        if all(&|p| p[0].abs() <= ON_LINE * along) {
            Line::Start
        } else if all(&|p| (p[0] - along).abs() <= ON_LINE * along) {
            Line::End
        } else if all(&|p| p[1].abs() <= ON_LINE * across) {
            Line::Bottom
        } else if all(&|p| (p[1] - across).abs() <= ON_LINE * across) {
            Line::Top
        } else {
            Line::Free
        }
    }

    /// The face is cut out of its surface: its outer wire has an edge off
    /// the lines bounding its grid (mitcad#2558).
    fn cut_out(&self) -> bool {
        self.outer()
            .is_some_and(|w| w.edges.iter().any(|e| self.line_of(e) == Line::Free))
    }
}

/// Where an edge of a face's boundary runs in the face's grid seen along
/// the strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    /// Along its first rung.
    Start,
    /// Along its last rung.
    End,
    /// Along its first rail.
    Bottom,
    /// Along its second rail.
    Top,
    /// Elsewhere: an edge cutting the face out of its surface, or a hole's.
    Free,
}

fn meets(a: [V3; 2], b: [V3; 2]) -> bool {
    distance(a[0], b[0]) <= MEET && distance(a[1], b[1]) <= MEET
}

/// The orientation of `grid` whose first rung (`end` false) or last rung
/// (`end` true) is `rung`.
fn rung_at(grid: &SheetFaceGrid, rung: [V3; 2], end: bool) -> Option<Orientation> {
    ORIENTATIONS.into_iter().find(|&orientation| {
        let o = Oriented { grid, orientation };
        meets(o.rung(if end { o.along() } else { 0 }), rung)
    })
}

/// Which way round the face's strip runs seen from its front near its
/// first rung (`end` false) or its last: the sign of (along x across) .
/// normal, taken one line in from the rung, which may be a point.
fn sense(face: Oriented<'_>, end: bool) -> f64 {
    let (along, across) = (face.along(), face.across());
    if along < 2 || across < 2 {
        return 0.0;
    }
    let (i, next) = if end { (along - 1, along) } else { (1, 0) };
    let mid = across / 2;
    let forward = if end {
        sub(face.point(next, mid), face.point(i, mid))
    } else {
        sub(face.point(i, mid), face.point(next, mid))
    };
    let side = sub(face.point(i, across), face.point(i, 0));
    dot(cross(forward, side), face.normal(i, mid))
}

/// The orientation of `grid` that continues the strip across `rung` (its
/// first rung, `end` false, or its last): as [`rung_at`], and where the
/// rung is a point (the rails meet there, mitcad#2574), so that any way
/// across matches it, the one that runs round as the face beside it does
/// (`beside`, its [`sense`] at that rung), keeping the rails on their sides.
fn continuing(grid: &SheetFaceGrid, rung: [V3; 2], end: bool, beside: f64) -> Option<Orientation> {
    if distance(rung[0], rung[1]) > MEET || beside.abs() < f64::MIN_POSITIVE {
        return rung_at(grid, rung, end);
    }
    let matching = |orientation: Orientation| {
        let o = Oriented { grid, orientation };
        meets(o.rung(if end { o.along() } else { 0 }), rung)
    };
    ORIENTATIONS
        .into_iter()
        .filter(|&o| matching(o))
        .find(|&orientation| sense(Oriented { grid, orientation }, end) * beside > 0.0)
        .or_else(|| rung_at(grid, rung, end))
}

/// How far the grid's parameter lines along `transpose`'s rungs are from
/// straight: the largest distance of a point from its line's chord.
fn bow(grid: &SheetFaceGrid, transpose: bool) -> f64 {
    let o = Oriented {
        grid,
        orientation: Orientation {
            transpose,
            reverse_along: false,
            reverse_across: false,
        },
    };
    let mut worst: f64 = 0.0;
    for i in 0..=o.along() {
        let [a, b] = o.rung(i);
        let d = sub(b, a);
        let length = norm(d);
        for j in 1..o.across() {
            let p = sub(o.point(i, j), a);
            let off = if length <= POINT {
                norm(p)
            } else {
                norm(cross(p, d)) / length
            };
            worst = worst.max(off);
        }
    }
    worst
}

/// The faces in the order the strip runs through them, each with how its
/// grid lies along it, and whether the strip closes on itself.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Chain {
    pub faces: Vec<(usize, Orientation)>,
    pub closed: bool,
}

/// The strip through the faces of `grids` (the picked face first): an error
/// names the face (by index) that does not join it.
pub(crate) fn chain(grids: &[SheetFaceGrid]) -> Result<Chain, (usize, String)> {
    let first = &grids[0];
    let start = |reverse_along: bool, transpose: bool| Orientation {
        transpose,
        reverse_along,
        reverse_across: false,
    };
    // A rung another face shares ends the first face; a face closed on
    // itself runs round; else the rungs run along its straighter lines.
    let shared = [
        start(false, false),
        start(false, true),
        start(true, false),
        start(true, true),
    ]
    .into_iter()
    .find(|&orientation| {
        let o = Oriented {
            grid: first,
            orientation,
        };
        let end = o.rung(o.along());
        grids[1..]
            .iter()
            .any(|g| rung_at(g, end, false).is_some() || rung_at(g, end, true).is_some())
    });
    let closed_along = |transpose: bool| {
        let o = Oriented {
            grid: first,
            orientation: start(false, transpose),
        };
        meets(o.rung(0), o.rung(o.along()))
    };
    let orientation = match shared {
        Some(o) => o,
        None if grids.len() > 1 => {
            return Err((
                1,
                "the face does not join the picked face along a whole edge across the strip".into(),
            ));
        }
        None if closed_along(false) => start(false, false),
        None if closed_along(true) => start(false, true),
        None => start(false, bow(first, true) < bow(first, false)),
    };
    let mut faces = vec![(0, orientation)];
    let mut used = vec![false; grids.len()];
    used[0] = true;
    let oriented = |(k, orientation): (usize, Orientation)| Oriented {
        grid: &grids[k],
        orientation,
    };
    let first_rung = oriented(faces[0]).rung(0);
    let mut closed = false;
    loop {
        let last = oriented(*faces.last().expect("a face"));
        let end = last.rung(last.along());
        if meets(end, first_rung) {
            closed = true;
            break;
        }
        let beside = sense(last, true);
        let next = (0..grids.len())
            .filter(|&m| !used[m])
            .find_map(|m| continuing(&grids[m], end, false, beside).map(|o| (m, o)));
        let Some((m, o)) = next else { break };
        used[m] = true;
        faces.push((m, o));
    }
    if !closed {
        loop {
            let begin = oriented(faces[0]).rung(0);
            let beside = sense(oriented(faces[0]), false);
            let previous = (0..grids.len())
                .filter(|&m| !used[m])
                .find_map(|m| continuing(&grids[m], begin, true, beside).map(|o| (m, o)));
            let Some((m, o)) = previous else { break };
            used[m] = true;
            faces.insert(0, (m, o));
        }
    }
    if let Some(m) = used.iter().position(|u| !u) {
        return Err((
            m,
            "the face does not continue the strip: the faces must follow one another, each \
             joined to the next along a whole edge across the strip"
                .into(),
        ));
    }
    Ok(Chain { faces, closed })
}

/// The length of a polyline.
fn length(points: impl IntoIterator<Item = V3>) -> f64 {
    let mut total = 0.0;
    let mut last: Option<V3> = None;
    for p in points {
        if let Some(q) = last {
            total += distance(p, q);
        }
        last = Some(p);
    }
    total
}

/// The length of the curve through `points` (equal steps of a parameter):
/// the polyline's, less the error of its chords, which falls with the
/// square of the step (from the polyline through every `step`-th point).
fn curve_length(points: &[V3], step: usize) -> f64 {
    let fine = length(points.iter().copied());
    if step < 2 || points.len() < 2 || !(points.len() - 1).is_multiple_of(step) {
        return fine;
    }
    let coarse = length(points.iter().step_by(step).copied());
    fine + (fine - coarse) / ((step * step - 1) as f64)
}

/// A face's mean rail length, for sharing the triangles out.
pub(crate) fn rail_length(face: Oriented<'_>) -> f64 {
    let rail = |j: usize| length((0..=face.along()).map(|i| face.point(i, j)));
    (rail(0) + rail(face.across())) / 2.0
}

/// The strips (two triangles each) of each face: `total` shared by the
/// faces' rail lengths, at least one each.
pub(crate) fn strips(lengths: &[f64], total: usize) -> Vec<usize> {
    let sum: f64 = lengths.iter().sum();
    lengths
        .iter()
        .map(|l| {
            let share = if sum > POINT {
                total as f64 * l / sum
            } else {
                total as f64 / lengths.len() as f64
            };
            (share.round() as usize).max(1)
        })
        .collect()
}

/// The area of a grid's surface `depth` behind it: two triangles a cell.
pub(crate) fn grid_area(grid: &SheetFaceGrid, depth: f64) -> f64 {
    let at = |i: usize, j: usize| {
        let k = j * (grid.columns + 1) + i;
        sub(grid.points[k], scale(grid.normals[k], depth))
    };
    let mut area = 0.0;
    for j in 0..grid.rows {
        for i in 0..grid.columns {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            area += norm(cross(sub(b, a), sub(c, a))) / 2.0;
            area += norm(cross(sub(c, a), sub(d, a))) / 2.0;
        }
    }
    area
}

/// A planar face is laid out between two edges of its outer wire, its rungs
/// (mitcad#2574): the edge it starts with, and an edge or a corner where
/// the strip leaves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlaneRungs {
    /// The outer wire's edge along the first rung.
    first: usize,
    /// Where the last rung is.
    last: PlaneEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaneEnd {
    /// Along this edge of the outer wire.
    Edge(usize),
    /// At this corner of the outer wire (the corner `k` where edge `k`
    /// starts): the rails meet there.
    Corner(usize),
}

/// How far a point of a planar face may lie off its plane (mm), as a face
/// within this of a plane is that plane to references.
const FLAT: f64 = 1e-6;

/// The grid lies in one plane, its normals all one.
fn planar(grid: &SheetFaceGrid) -> bool {
    let (Some(&n), Some(&o)) = (grid.normals.first(), grid.points.first()) else {
        return false;
    };
    (norm(n) - 1.0).abs() <= 1e-6
        && grid.normals.iter().all(|m| dot(*m, n) >= 1.0 - 1e-9)
        && grid.points.iter().all(|p| dot(sub(*p, o), n).abs() <= FLAT)
}

/// The edges of the face's outer wire, each running on from the one
/// before; None without such a wire of three edges or more.
fn outer_edges(grid: &SheetFaceGrid) -> Option<Vec<&SheetFaceEdge>> {
    let wire = grid.wires.iter().find(|w| w.outer)?;
    let edges: Vec<&SheetFaceEdge> = wire.edges.iter().collect();
    if edges.len() < 3 || edges.iter().any(|e| e.points.len() < 2) {
        return None;
    }
    let m = edges.len();
    let joined = (0..m).all(|k| {
        let end = *edges[k].points.last().expect("two points");
        distance(end, edges[(k + 1) % m].points[0]) <= MEET
    });
    joined.then_some(edges)
}

/// The ends of a straight edge; None for a curved one or one of no length.
fn straight(edge: &SheetFaceEdge) -> Option<[V3; 2]> {
    let (a, b) = (edge.points[0], *edge.points.last()?);
    let d = sub(b, a);
    let l = norm(d);
    if l <= MEET {
        return None;
    }
    edge.points
        .iter()
        .all(|p| norm(cross(sub(*p, a), d)) / l <= MEET)
        .then_some([a, b])
}

/// The lines a face may share with a planar face beside it as a rung: a
/// planar face's straight edges, any other face's grid's four bounding
/// lines.
fn sharable_lines(grid: &SheetFaceGrid, flat: bool) -> Vec<[V3; 2]> {
    if flat && let Some(edges) = outer_edges(grid) {
        return edges.into_iter().filter_map(straight).collect();
    }
    let mut lines = Vec::new();
    for transpose in [false, true] {
        let o = Oriented {
            grid,
            orientation: Orientation {
                transpose,
                reverse_along: false,
                reverse_across: false,
            },
        };
        lines.push(o.rung(0));
        lines.push(o.rung(o.along()));
    }
    lines
}

/// The rungs of each planar face among `grids` (the faces' first look,
/// their boundaries sampled), None for a face that is not planar or whose
/// rungs are not found (mitcad#2574).
///
/// A plane's parameter lines run along its own axes, not along its edges:
/// a triangle of a square-to-round built of planes and cones, or a
/// trapezoid of a tapered duct, lies inside its parameters' rectangle with
/// its edges across it. Its rungs are the straight edges of its outer wire
/// that it shares with the faces beside it: two, or one where the strip
/// ends with it (the corner across a triangle from it, or the edge across a
/// face of more edges).
pub(crate) fn plane_rungs(grids: &[SheetFaceGrid]) -> Vec<Option<PlaneRungs>> {
    let flat: Vec<bool> = grids
        .iter()
        .map(|g| planar(g) && outer_edges(g).is_some())
        .collect();
    let lines: Vec<Vec<[V3; 2]>> = grids
        .iter()
        .zip(&flat)
        .map(|(g, f)| sharable_lines(g, *f))
        .collect();
    (0..grids.len())
        .map(|k| {
            if !flat[k] {
                return None;
            }
            let edges = outer_edges(&grids[k])?;
            let m = edges.len();
            let shared: Vec<usize> = (0..m)
                .filter(|&e| {
                    straight(edges[e]).is_some_and(|[a, b]| {
                        (0..grids.len()).filter(|&o| o != k).any(|o| {
                            lines[o]
                                .iter()
                                .any(|&l| meets(l, [a, b]) || meets(l, [b, a]))
                        })
                    })
                })
                .collect();
            let rungs = match shared[..] {
                [first, last, ..] => PlaneRungs {
                    first,
                    last: PlaneEnd::Edge(last),
                },
                [first] if m == 3 => PlaneRungs {
                    first,
                    last: PlaneEnd::Corner((first + 2) % m),
                },
                [first] => PlaneRungs {
                    first,
                    last: PlaneEnd::Edge((first + m / 2) % m),
                },
                [] => return None,
            };
            plane_grid(&grids[k], rungs, [PROBE, PROBE]).map(|_| rungs)
        })
        .collect()
}

/// The edges from `from` on to `to` (not included), round the wire of `m`.
fn edges_between(from: usize, to: usize, m: usize) -> Vec<usize> {
    let mut list = Vec::new();
    let mut k = from % m;
    while k != to % m && list.len() < m {
        list.push(k);
        k = (k + 1) % m;
    }
    list
}

/// The points of edges joined end to end.
fn joined_points(edges: &[&SheetFaceEdge]) -> Vec<V3> {
    let mut points: Vec<V3> = Vec::new();
    for edge in edges {
        let skip = usize::from(!points.is_empty());
        points.extend(edge.points.iter().skip(skip));
    }
    points
}

/// `count + 1` points at equal steps of length along the polyline.
fn resampled(points: &[V3], count: usize) -> Vec<V3> {
    let total = length(points.iter().copied());
    if points.len() < 2 || total <= POINT {
        return vec![points[0]; count + 1];
    }
    let mut out = Vec::with_capacity(count + 1);
    let (mut k, mut walked) = (0, 0.0);
    for i in 0..=count {
        let target = total * i as f64 / count as f64;
        while k + 2 < points.len() && walked + distance(points[k], points[k + 1]) < target {
            walked += distance(points[k], points[k + 1]);
            k += 1;
        }
        let step = distance(points[k], points[k + 1]);
        let t = if step <= POINT {
            0.0
        } else {
            ((target - walked) / step).clamp(0.0, 1.0)
        };
        out.push(add(points[k], scale(sub(points[k + 1], points[k]), t)));
    }
    out
}

/// The lengths along a polyline from its first point to each point.
fn runs(points: &[V3]) -> Vec<f64> {
    let mut run = vec![0.0];
    for w in points.windows(2) {
        run.push(run[run.len() - 1] + distance(w[0], w[1]));
    }
    run
}

/// The grid of a planar face between its rungs (mitcad#2574), `along`
/// columns from the first to the last and `across` rows from the first
/// rail (the one from the first rung's start) to the second, its rails the
/// wire's edges between the rungs at equal steps of their length and its
/// lines across straight; its outer wire and its holes placed in it. None
/// where the rungs do not fit the wire.
pub(crate) fn plane_grid(
    grid: &SheetFaceGrid,
    rungs: PlaneRungs,
    [along, across]: [usize; 2],
) -> Option<SheetFaceGrid> {
    let edges = outer_edges(grid)?;
    let m = edges.len();
    let first = rungs.first;
    let corner = |k: usize| edges[k % m].points[0];
    // The second rail from the first rung's end, the first rail from its
    // start, each to the last rung (`rail_a` in the wire's direction, so
    // towards the first rung).
    let (rail_b, rail_a, last) = match rungs.last {
        PlaneEnd::Edge(b) if b != first => (
            edges_between(first + 1, b, m),
            edges_between(b + 1, first, m),
            Some(b),
        ),
        PlaneEnd::Corner(c) if c != first && c != (first + 1) % m => (
            edges_between(first + 1, c, m),
            edges_between(c, first, m),
            None,
        ),
        _ => return None,
    };
    let pick = |list: &[usize]| list.iter().map(|&k| edges[k]).collect::<Vec<_>>();
    let points_b = if rail_b.is_empty() {
        vec![corner(first + 1)]
    } else {
        joined_points(&pick(&rail_b))
    };
    let points_a: Vec<V3> = if rail_a.is_empty() {
        vec![corner(first)]
    } else {
        let mut p = joined_points(&pick(&rail_a));
        p.reverse();
        p
    };
    if distance(points_a[0], corner(first)) > MEET
        || distance(points_b[0], *edges[first].points.last()?) > MEET
    {
        return None;
    }
    let (a, b) = (resampled(&points_a, along), resampled(&points_b, along));
    let mut points = Vec::with_capacity((along + 1) * (across + 1));
    for j in 0..=across {
        let share = j as f64 / across as f64;
        for i in 0..=along {
            points.push(add(a[i], scale(sub(b[i], a[i]), share)));
        }
    }
    let normal = grid.normals[0];
    let mut out = SheetFaceGrid {
        columns: along,
        rows: across,
        normals: vec![normal; points.len()],
        points,
        area: grid.area,
        wires: Vec::new(),
    };
    // The outer wire's edges placed in the grid: the rungs at its first
    // and last line along, the rails at its first and last line across, at
    // their share of the rail's length.
    let (length_a, length_b) = (
        length(points_a.iter().copied()),
        length(points_b.iter().copied()),
    );
    let (along_f, across_f) = (along as f64, across as f64);
    let mut cells: Vec<Vec<[f64; 2]>> = vec![Vec::new(); m];
    let mut place_rail = |list: &[usize], total: f64, top: bool| {
        let mut walked = 0.0;
        for &k in list {
            let run = runs(&edges[k].points);
            cells[k] = run
                .iter()
                .map(|r| {
                    let s = if top { walked + r } else { total - walked - r };
                    let x = if total <= POINT {
                        0.0
                    } else {
                        (s / total).clamp(0.0, 1.0) * along_f
                    };
                    [x, if top { across_f } else { 0.0 }]
                })
                .collect();
            walked += run[run.len() - 1];
        }
    };
    place_rail(&rail_b, length_b, true);
    place_rail(&rail_a, length_a, false);
    let across_rung = |k: usize, from: V3, x: f64, up: bool| -> Vec<[f64; 2]> {
        let l = distance(
            edges[k].points[0],
            *edges[k].points.last().expect("two points"),
        );
        edges[k]
            .points
            .iter()
            .map(|p| {
                let share = if l <= POINT {
                    0.0
                } else {
                    (distance(*p, from) / l).clamp(0.0, 1.0)
                };
                let y = if up { share } else { 1.0 - share };
                [x, y * across_f]
            })
            .collect()
    };
    cells[first] = across_rung(first, corner(first), 0.0, true);
    if let Some(b) = last {
        cells[b] = across_rung(b, corner(b), along_f, false);
    }
    let mut wires = vec![SheetFaceWire {
        outer: true,
        edges: edges
            .iter()
            .zip(cells)
            .map(|(e, cells)| SheetFaceEdge {
                cells,
                points: e.points.clone(),
                normals: e.normals.clone(),
            })
            .collect(),
    }];
    // The holes: each point's place in the grid.
    for wire in grid.wires.iter().filter(|w| !w.outer) {
        wires.push(SheetFaceWire {
            outer: false,
            edges: wire
                .edges
                .iter()
                .map(|e| SheetFaceEdge {
                    cells: e.points.iter().map(|p| located(&out, *p)).collect(),
                    points: e.points.clone(),
                    normals: e.normals.clone(),
                })
                .collect(),
        });
    }
    out.wires = wires;
    Some(out)
}

/// The place in the grid (`[x, y]` in steps of its columns and rows) of a
/// point on its surface: through the triangle of a cell (two a cell) it
/// lies in, or nearest to.
fn located(grid: &SheetFaceGrid, p: V3) -> [f64; 2] {
    let at = |[i, j]: [usize; 2]| grid.points[j * (grid.columns + 1) + i];
    let mut best: Option<(f64, [f64; 2])> = None;
    for j in 0..grid.rows {
        for i in 0..grid.columns {
            let cell = [[i, j], [i + 1, j], [i + 1, j + 1], [i, j + 1]];
            for [x, y, z] in [[0, 1, 2], [0, 2, 3]] {
                let corners = [cell[x], cell[y], cell[z]];
                let Some(l) = barycentric(corners.map(at), p) else {
                    continue;
                };
                let worst = l[0].min(l[1]).min(l[2]);
                if best.is_none_or(|(w, _)| worst > w) {
                    let spot =
                        [0, 1].map(|k| (0..3).map(|c| l[c] * corners[c][k] as f64).sum::<f64>());
                    best = Some((worst, spot));
                }
            }
        }
    }
    best.map_or([0.0, 0.0], |(_, spot)| spot)
}

/// A rung of the strip at the neutral surface: its ends on the two rails
/// and the faces' normals there.
#[derive(Debug, Clone, Copy)]
struct Station {
    ends: [V3; 2],
    normals: [V3; 2],
}

/// A face's points at the neutral surface along the strip (mitcad#2558):
/// `depth` behind the face, its difference from the faces beside it where
/// they share a rung shared out evenly along it, and the strip's two rails
/// one point where they meet (within [`MEET`]).
///
/// Where two faces meet at an angle, their points `depth` behind the
/// shared rung lie apart (by `2 depth sin(a/2)`). The strip runs through
/// their mean; moving only the shared rung there (as before) put the
/// difference into the face's last strip, shorter the more triangles, so
/// the pattern and its measured deviation changed with the count. Shared
/// out along the whole face, the pattern near the corner no longer
/// depends on the count.
struct Neutral<'a> {
    face: Oriented<'a>,
    depth: f64,
    /// The shift at the face's first rung, on its first and its second
    /// rail.
    start: [V3; 2],
    /// The shift at its last rung.
    end: [V3; 2],
    /// Point `(i, j)` at `i * (across + 1) + j`.
    points: Vec<V3>,
}

impl Neutral<'_> {
    fn along(&self) -> usize {
        self.face.along()
    }

    fn across(&self) -> usize {
        self.face.across()
    }

    /// The shift at `[along, across]` in steps of the grid.
    fn shift(&self, [a, b]: [f64; 2]) -> V3 {
        let t = (a / self.along() as f64).clamp(0.0, 1.0);
        let share = (b / self.across() as f64).clamp(0.0, 1.0);
        let at = |ends: [V3; 2]| add(scale(ends[0], 1.0 - share), scale(ends[1], share));
        add(scale(at(self.start), 1.0 - t), scale(at(self.end), t))
    }

    fn at(&self, i: usize, j: usize) -> V3 {
        self.points[i * (self.across() + 1) + j]
    }

    /// A point of the face's boundary at `place` (`[along, across]`) at
    /// the neutral surface.
    fn sample(&self, place: [f64; 2], point: V3, normal: V3) -> V3 {
        add(sub(point, scale(normal, self.depth)), self.shift(place))
    }
}

/// The faces' points at the neutral surface ([`Neutral`]).
fn neutral<'a>(faces: &[Oriented<'a>], depth: f64) -> Vec<Neutral<'a>> {
    let mut all: Vec<Neutral<'a>> = faces
        .iter()
        .map(|&face| Neutral {
            face,
            depth,
            start: [[0.0; 3]; 2],
            end: [[0.0; 3]; 2],
            points: Vec::new(),
        })
        .collect();
    for f in 1..faces.len() {
        let (a, b) = (faces[f - 1], faces[f]);
        let before = [
            a.behind(a.along(), 0, depth),
            a.behind(a.along(), a.across(), depth),
        ];
        let after = [b.behind(0, 0, depth), b.behind(0, b.across(), depth)];
        let mean = [0, 1].map(|k| scale(add(before[k], after[k]), 0.5));
        all[f - 1].end = [0, 1].map(|k| sub(mean[k], before[k]));
        all[f].start = [0, 1].map(|k| sub(mean[k], after[k]));
    }
    for n in &mut all {
        let (along, across) = (n.along(), n.across());
        let mut points = Vec::with_capacity((along + 1) * (across + 1));
        for i in 0..=along {
            let first = points.len();
            for j in 0..=across {
                points.push(add(
                    n.face.behind(i, j, depth),
                    n.shift([i as f64, j as f64]),
                ));
            }
            // The rails meet here (a fan's tip): one point.
            let (p, q) = (points[first], points[first + across]);
            if distance(p, q) <= MEET {
                let mean = scale(add(p, q), 0.5);
                points[first] = mean;
                points[first + across] = mean;
            }
        }
        n.points = points;
    }
    all
}

/// A strip laid flat.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Layout {
    /// The outline: the first rail, then the second back (not closed
    /// again), counter-clockwise or not; of faces cut out of their
    /// surfaces, their edges laid out with the strip. Where the rails meet
    /// between the strip's ends it passes through that point twice
    /// (mitcad#2574).
    pub outline: Vec<[f64; 2]>,
    /// The faces' holes laid out with the strip (mitcad#2558).
    pub holes: Vec<Vec<[f64; 2]>>,
    /// The rung end (vertex `2 s + k` as in `flat`) the pattern's y axis
    /// runs to from vertex 0: 1, or 3 where the strip's rails meet where
    /// it starts.
    pub axis: usize,
    pub folds: Vec<SheetFold>,
    /// The stations between the strip's ends where its rails meet: the
    /// strip is split there into two fans laid side by side (mitcad#2574).
    pub pinches: Vec<usize>,
    /// The triangles that are not points or lines.
    pub triangles: u32,
    /// The rungs' ends laid flat before the seam is opened: `2 s` on the
    /// first rail, `2 s + 1` on the second.
    pub flat: Vec<[f64; 2]>,
    /// Each face's first and last rung.
    pub spans: Vec<(usize, usize)>,
    /// The rails' and the end rungs' lengths: the pattern's against the
    /// faces'.
    pub edges: f64,
    /// Lengths across: the rungs between, the diagonals not laid and the
    /// faces' lines between their rails.
    pub across: f64,
}

fn flat_distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The flat place of `c` in the triangle `(a, b, c)` whose `a` and `b` lie
/// at `fa` and `fb`: its sides' lengths kept, on the side that shows the
/// triangle as it is seen from the front (along `-normal`).
fn place([fa, fb]: [[f64; 2]; 2], [a, b, c]: [V3; 3], normal: V3) -> Result<[f64; 2], String> {
    let d = flat_distance(fa, fb);
    if d <= POINT {
        return Err(
            "the strip's edges cross between two of its lines across: use more triangles".into(),
        );
    }
    let (ac, bc) = (distance(a, c), distance(b, c));
    let e = [(fb[0] - fa[0]) / d, (fb[1] - fa[1]) / d];
    let x = (ac * ac - bc * bc + d * d) / (2.0 * d);
    let h = (ac * ac - x * x).max(0.0).sqrt();
    let side = if dot(cross(sub(b, a), sub(c, a)), normal) < 0.0 {
        -1.0
    } else {
        1.0
    };
    Ok([
        fa[0] + x * e[0] - side * h * e[1],
        fa[1] + x * e[1] + side * h * e[0],
    ])
}

/// The flat vector made a unit long; None for one of no length.
fn flat_unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let l = v[0].hypot(v[1]);
    (l > POINT).then(|| [v[0] / l, v[1] / l])
}

fn unit_or(v: V3, or: V3) -> V3 {
    let n = norm(v);
    if n <= POINT { or } else { scale(v, 1.0 / n) }
}

/// The faces' strip (fine grids along `chain`) laid flat at the neutral
/// surface `depth` behind the faces, opened at its seam by `gap` when it
/// closes on itself.
pub(crate) fn lay_flat(
    faces: &[Oriented<'_>],
    depth: f64,
    closed: bool,
    gap: f64,
) -> Result<Layout, String> {
    for face in faces {
        if face.along() % FINE != 0 || face.along() == 0 || face.across() == 0 {
            return Err("a face's grid does not fit its strips".into());
        }
    }
    let surfaces = neutral(faces, depth);
    // The rungs: every FINE-th line of each face, a rung two faces share
    // the mean of theirs; each face's first and one-past-last station.
    let mut stations: Vec<Station> = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for surface in &surfaces {
        let face = surface.face;
        let r = face.across();
        let first = stations.len().saturating_sub(1);
        for i in (0..=face.along()).step_by(FINE) {
            let station = Station {
                ends: [surface.at(i, 0), surface.at(i, r)],
                normals: [face.normal(i, 0), face.normal(i, r)],
            };
            if i == 0
                && let Some(last) = stations.last_mut()
            {
                let before = *last;
                last.ends = [0, 1].map(|k| scale(add(before.ends[k], station.ends[k]), 0.5));
                last.normals = [0, 1].map(|k| {
                    unit_or(
                        add(before.normals[k], station.normals[k]),
                        station.normals[k],
                    )
                });
                continue;
            }
            stations.push(station);
        }
        let start = if spans.is_empty() { 0 } else { first };
        spans.push((start, stations.len() - 1));
    }
    let n = stations.len() - 1;
    // Vertex 2 s is station s's end on the first rail, 2 s + 1 on the
    // second.
    let point = |v: usize| stations[v / 2].ends[v % 2];
    let normal = |v: usize| stations[v / 2].normals[v % 2];
    let mut flat: Vec<[f64; 2]> = vec![[0.0; 2]; 2 * n + 2];
    let quad_normal = |s: usize| {
        unit_or(
            (2 * s..2 * s + 4).fold([0.0; 3], |acc, v| add(acc, normal(v))),
            normal(2 * s),
        )
    };
    let first = distance(point(0), point(1));
    let mut triangles: Vec<[usize; 3]> = Vec::new();
    // Each quadrilateral's two triangles (one twice where it is a
    // triangle), for laying out points of the faces with them.
    let mut quads: Vec<[[usize; 3]; 2]> = Vec::new();
    let mut diagonals: Vec<([usize; 2], [usize; 2])> = Vec::new();
    let mut axis = 1;
    let mut from = 0;
    if first <= POINT {
        // The rails meet where the strip starts (a fan, mitcad#2558): its
        // first quadrilateral is a triangle, laid from its side to the
        // next rung's second-rail end.
        let reach = distance(point(0), point(3));
        if reach <= POINT {
            return Err(
                "the strip's edges meet along its start: it has no width there to lay flat".into(),
            );
        }
        flat[3] = [0.0, reach];
        flat[2] = place(
            [flat[0], flat[3]],
            [point(0), point(3), point(2)],
            quad_normal(0),
        )?;
        triangles.push([0, 2, 3]);
        quads.push([[0, 2, 3]; 2]);
        axis = 3;
        from = 1;
    } else {
        flat[1] = [0.0, first];
    }
    // The stations between the strip's ends where its rails meet
    // (mitcad#2574).
    let mut pinches: Vec<usize> = Vec::new();
    for s in from..n {
        let (b0, t0, b1, t1) = (2 * s, 2 * s + 1, 2 * s + 2, 2 * s + 3);
        let quad = quad_normal(s);
        if s > 0 && distance(point(b0), point(t0)) <= POINT {
            // The rails meet here, between the strip's ends: the strip is
            // two fans there, the one before ending at the point and the
            // next starting from it. The next is laid beside the one
            // before, turned about the point so that the middle of its
            // corner there points away from the middle of the other's.
            let reach = distance(point(b0), point(t1));
            if reach <= POINT {
                return Err(
                    "the strip's edges meet along a stretch of it: it has no width there to lay \
                     flat"
                        .into(),
                );
            }
            let p = flat[b0];
            flat[t0] = p;
            // The middle of the corner at `p` between the lines to `a` and
            // `b`, a unit vector.
            let middle = |a: [f64; 2], b: [f64; 2]| -> [f64; 2] {
                let u = flat_unit([a[0] - p[0], a[1] - p[1]]).unwrap_or([1.0, 0.0]);
                let w = flat_unit([b[0] - p[0], b[1] - p[1]]).unwrap_or(u);
                flat_unit([u[0] + w[0], u[1] + w[1]]).unwrap_or([-u[1], u[0]])
            };
            let before = middle(flat[b0 - 2], flat[t0 - 2]);
            let away = [-before[0], -before[1]];
            flat[t1] = [p[0] + reach * away[0], p[1] + reach * away[1]];
            flat[b1] = place([p, flat[t1]], [point(b0), point(t1), point(b1)], quad)?;
            let now = middle(flat[b1], flat[t1]);
            let turn =
                (now[0] * away[1] - now[1] * away[0]).atan2(now[0] * away[0] + now[1] * away[1]);
            let (sin, cos) = turn.sin_cos();
            for v in [b1, t1] {
                let d = [flat[v][0] - p[0], flat[v][1] - p[1]];
                flat[v] = [
                    p[0] + cos * d[0] - sin * d[1],
                    p[1] + sin * d[0] + cos * d[1],
                ];
            }
            triangles.push([b0, b1, t1]);
            quads.push([[b0, b1, t1]; 2]);
            pinches.push(s);
            continue;
        }
        let lay = |flat: &mut Vec<[f64; 2]>, [a, b, c]: [usize; 3]| -> Result<(), String> {
            flat[c] = place([flat[a], flat[b]], [point(a), point(b), point(c)], quad)?;
            Ok(())
        };
        if distance(point(b0), point(t1)) <= distance(point(t0), point(b1)) {
            // Along the diagonal from the first rail to the next rung's
            // second-rail end.
            lay(&mut flat, [b0, t0, t1])?;
            lay(&mut flat, [b0, t1, b1])?;
            triangles.push([b0, t1, t0]);
            triangles.push([b0, b1, t1]);
            quads.push([[b0, t1, t0], [b0, b1, t1]]);
            diagonals.push(([b0, t1], [t0, b1]));
        } else {
            lay(&mut flat, [b0, t0, b1])?;
            lay(&mut flat, [t0, b1, t1])?;
            triangles.push([b0, b1, t0]);
            triangles.push([t0, b1, t1]);
            quads.push([[b0, b1, t0], [t0, b1, t1]]);
            diagonals.push(([t0, b1], [b0, t1]));
        }
    }
    // How far the pattern's lengths are from the faces'. The rails and the
    // end rungs of a face cut out of its surface are no edges of it: its
    // own edges are measured instead, below.
    let cut_out: Vec<bool> = faces.iter().map(Oriented::cut_out).collect();
    let mut edges: f64 = 0.0;
    let mut across: f64 = 0.0;
    let iso = |surface: &Neutral<'_>, i: usize| -> (Vec<f64>, f64) {
        // The lengths along the face's line across at `i`, from its
        // first rail to each point, and the curve's whole length.
        let points: Vec<V3> = (0..=surface.across()).map(|j| surface.at(i, j)).collect();
        let mut run = vec![0.0];
        for j in 0..surface.across() {
            run.push(run[j] + distance(points[j], points[j + 1]));
        }
        (run, curve_length(&points, 2))
    };
    for ((surface, &(s0, s1)), &cut) in surfaces.iter().zip(&spans).zip(&cut_out) {
        let r = surface.across();
        let at = |s: usize| (s - s0) * FINE;
        // The lines along the face, the rails among them: the pattern's
        // points on each rung at the share of the line across it.
        let runs: Vec<(Vec<f64>, f64)> = (s0..=s1).map(|s| iso(surface, at(s))).collect();
        for j in 0..=r {
            let line: Vec<V3> = (0..=surface.along()).map(|i| surface.at(i, j)).collect();
            let faces_length = curve_length(&line, FINE);
            let on = |s: usize| {
                let run = &runs[s - s0].0;
                let share = if run[r] > POINT { run[j] / run[r] } else { 0.0 };
                let (b, t) = (flat[2 * s], flat[2 * s + 1]);
                [b[0] + (t[0] - b[0]) * share, b[1] + (t[1] - b[1]) * share]
            };
            let pattern: f64 = (s0..s1).map(|s| flat_distance(on(s), on(s + 1))).sum();
            let off = (pattern - faces_length).abs();
            if (j == 0 || j == r) && !cut {
                edges = edges.max(off);
            } else {
                across = across.max(off);
            }
        }
        // The rungs: the pattern's straight line against the face's line
        // across; the strip's first and last are its edges.
        for s in s0..=s1 {
            let off = (flat_distance(flat[2 * s], flat[2 * s + 1]) - runs[s - s0].1).abs();
            if (s == 0 || s == n) && !cut {
                edges = edges.max(off);
            } else {
                across = across.max(off);
            }
        }
    }
    // The diagonals not laid: the pattern's against the faces' chords.
    for (_, [p, q]) in &diagonals {
        let off = (flat_distance(flat[*p], flat[*q]) - distance(point(*p), point(*q))).abs();
        across = across.max(off);
    }
    // The fold lines: where two triangles meet at an angle.
    let triangle_normal = |[a, b, c]: [usize; 3]| -> Option<V3> {
        let m = cross(sub(point(b), point(a)), sub(point(c), point(a)));
        let size = norm(m);
        if size <= 1e-12 * first.max(1.0).powi(2) {
            return None;
        }
        let m = scale(m, 1.0 / size);
        let front = add(add(normal(a), normal(b)), normal(c));
        Some(if dot(m, front) < 0.0 {
            scale(m, -1.0)
        } else {
            m
        })
    };
    // The seam's points moved half the gap along the rails.
    let mut opened = flat.clone();
    if closed {
        let along = |from: usize, to: usize| -> Result<[f64; 2], String> {
            let (a, b) = (flat[from], flat[to]);
            let d = flat_distance(a, b);
            if d <= POINT {
                return Ok(a);
            }
            if gap / 2.0 >= d {
                return Err(
                    "the corner gap is wider than the triangles at the seam: make it smaller \
                     or use fewer triangles"
                        .into(),
                );
            }
            let t = gap / 2.0 / d;
            Ok([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
        };
        opened[0] = along(0, 2)?;
        opened[1] = along(1, 3)?;
        opened[2 * n] = along(2 * n, 2 * n - 2)?;
        opened[2 * n + 1] = along(2 * n + 1, 2 * n - 1)?;
    }
    let mut folds = Vec::new();
    let mut edge_triangles: Vec<([usize; 2], Vec<usize>)> = Vec::new();
    for (k, t) in triangles.iter().enumerate() {
        for e in [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]] {
            let key = [e[0].min(e[1]), e[0].max(e[1])];
            match edge_triangles
                .iter_mut()
                .rev()
                .take(16)
                .find(|(e, _)| *e == key)
            {
                Some((_, list)) => list.push(k),
                None => edge_triangles.push((key, vec![k])),
            }
        }
    }
    for (e, list) in &edge_triangles {
        let [k, l] = list[..] else { continue };
        let (Some(m1), Some(m2)) = (triangle_normal(triangles[k]), triangle_normal(triangles[l]))
        else {
            continue;
        };
        let angle = norm(cross(m1, m2)).atan2(dot(m1, m2));
        if angle < FOLD {
            continue;
        }
        let other = triangles[l]
            .into_iter()
            .find(|v| !e.contains(v))
            .expect("a triangle's third point");
        let up = dot(sub(point(other), point(e[0])), m1) > 0.0;
        folds.push(SheetFold {
            start: opened[e[0]],
            end: opened[e[1]],
            angle: if up { angle } else { -angle },
        });
    }
    let laid = triangles
        .iter()
        .filter(|t| triangle_normal(**t).is_some())
        .count();
    // A point of face `f` at `place` (`[along, across]`), `p` at the
    // neutral surface, laid out with the strip (mitcad#2558): a rung's end
    // where it is one, else through the triangle of its quadrilateral it
    // lies in (its place on the triangle's plane kept), opened at the seam
    // as the triangles are.
    let lay_out = |f: usize, place: [f64; 2], p: V3| -> [f64; 2] {
        let (s0, s1) = spans[f];
        let strips = s1 - s0;
        let r = surfaces[f].across() as f64;
        let q = place[0] / FINE as f64;
        let nearest = q.round().clamp(0.0, strips as f64);
        if (q - nearest).abs() <= ON_LINE * strips as f64 {
            let s = s0 + nearest as usize;
            if place[1].abs() <= ON_LINE * r {
                return opened[2 * s];
            }
            if (place[1] - r).abs() <= ON_LINE * r {
                return opened[2 * s + 1];
            }
        }
        let s = s0 + (q.floor().max(0.0) as usize).min(strips - 1);
        let mut best: Option<(f64, [f64; 2])> = None;
        for t in quads[s] {
            let Some(l) = barycentric([point(t[0]), point(t[1]), point(t[2])], p) else {
                continue;
            };
            let worst = l[0].min(l[1]).min(l[2]);
            let spot = [0, 1].map(|k| (0..3).map(|m| l[m] * opened[t[m]][k]).sum::<f64>());
            if best.is_none_or(|(w, _)| worst > w) {
                best = Some((worst, spot));
            }
        }
        best.map_or(opened[2 * s], |(_, spot)| spot)
    };
    // The faces' holes, and the edges of faces cut out of their surfaces,
    // laid out; each measured against the face's edge (its points at the
    // neutral surface) like the rails.
    let count = faces.len();
    let mut pieces: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut holes: Vec<Vec<[f64; 2]>> = Vec::new();
    for (f, (surface, &(s0, s1))) in surfaces.iter().zip(&spans).enumerate() {
        let face = surface.face;
        if !cut_out[f] {
            // The face's part of the strip's outline: its rails, and the
            // strip's first and last rung.
            pieces.push((s0..=s1).map(|s| opened[2 * s]).collect());
            pieces.push((s0..=s1).map(|s| opened[2 * s + 1]).collect());
            if s0 == 0 {
                pieces.push(vec![opened[0], opened[1]]);
            }
            if s1 == n {
                pieces.push(vec![opened[2 * n], opened[2 * n + 1]]);
            }
        }
        let along = face.along() as f64;
        for wire in &face.grid.wires {
            if wire.outer && !cut_out[f] {
                continue;
            }
            let mut hole: Vec<[f64; 2]> = Vec::new();
            for edge in &wire.edges {
                let count_points = edge
                    .cells
                    .len()
                    .min(edge.points.len())
                    .min(edge.normals.len());
                if count_points < 2 {
                    continue;
                }
                let places: Vec<[f64; 2]> = edge.cells[..count_points]
                    .iter()
                    .map(|&c| face.place(c))
                    .collect();
                let at_neutral: Vec<V3> = (0..count_points)
                    .map(|k| surface.sample(places[k], edge.points[k], edge.normals[k]))
                    .collect();
                let laid_at = |k: usize| lay_out(f, places[k], at_neutral[k]);
                let last = count_points - 1;
                let line = if wire.outer {
                    face.line_of(edge)
                } else {
                    Line::Free
                };
                let drawn: Vec<[f64; 2]> = match line {
                    // A rung shared with the face before or after.
                    Line::Start if f != 0 => continue,
                    Line::End if f + 1 != count => continue,
                    Line::Start | Line::End => vec![laid_at(0), laid_at(last)],
                    Line::Bottom | Line::Top => {
                        // Through the rungs' ends it passes, as the rails.
                        let k = usize::from(line == Line::Top);
                        let (a0, a1) = (places[0][0], places[last][0]);
                        let (low, high) = (a0.min(a1), a0.max(a1));
                        let mut passed: Vec<usize> = (0..=s1 - s0)
                            .filter(|&q| {
                                let a = (q * FINE) as f64;
                                a > low + ON_LINE * along && a < high - ON_LINE * along
                            })
                            .collect();
                        if a1 < a0 {
                            passed.reverse();
                        }
                        let mut drawn = vec![laid_at(0)];
                        drawn.extend(passed.iter().map(|&q| opened[2 * (s0 + q) + k]));
                        drawn.push(laid_at(last));
                        drawn
                    }
                    Line::Free => (0..count_points).map(laid_at).collect(),
                };
                let off = (flat_length(&drawn) - curve_length(&at_neutral, 2)).abs();
                edges = edges.max(off);
                if wire.outer {
                    pieces.push(drawn);
                } else if hole.is_empty() {
                    hole = drawn;
                } else {
                    hole.extend(drawn.into_iter().skip(1));
                }
            }
            if !wire.outer && !hole.is_empty() {
                holes.push(closed_loop(hole));
            }
        }
    }
    let mut outline: Vec<[f64; 2]> = if cut_out.iter().any(|c| *c) {
        joined(pieces).ok_or(
            "the faces' outlines do not join into one: a face cut out of its surface must keep \
             the whole edges it shares with the faces beside it",
        )?
    } else {
        let mut outline: Vec<[f64; 2]> = (0..=n).map(|s| opened[2 * s]).collect();
        outline.extend((0..=n).rev().map(|s| opened[2 * s + 1]));
        outline
    };
    outline = closed_loop(outline);
    if outline.len() < 3 {
        return Err("the strip has no area".into());
    }
    // Where the rails meet between the strip's ends the outline touches
    // itself: its two fans meet at that point only (mitcad#2574).
    let pinch_points: Vec<[f64; 2]> = pinches.iter().map(|&s| opened[2 * s]).collect();
    if crosses_itself_but(&outline, &pinch_points) {
        return Err(
            "the pattern laid flat overlaps itself: develop fewer faces at once, or open the \
             strip at another face"
                .into(),
        );
    }
    for (k, hole) in holes.iter().enumerate() {
        if hole.len() < 3 {
            return Err("a hole of the faces has no area laid flat".into());
        }
        if crosses_itself(hole)
            || loops_cross(hole, &outline)
            || holes[..k].iter().any(|other| loops_cross(hole, other))
        {
            return Err(
                "a hole laid flat crosses the pattern's outline or another hole: use more \
                 triangles"
                    .into(),
            );
        }
    }
    // Fold lines only where there is material.
    if cut_out.iter().any(|c| *c) || !holes.is_empty() {
        let loops: Vec<&[[f64; 2]]> = std::iter::once(outline.as_slice())
            .chain(holes.iter().map(Vec::as_slice))
            .collect();
        folds = folds
            .into_iter()
            .flat_map(|fold| {
                clipped([fold.start, fold.end], &loops)
                    .into_iter()
                    .map(move |[start, end]| SheetFold {
                        start,
                        end,
                        angle: fold.angle,
                    })
            })
            .collect();
    }
    Ok(Layout {
        outline,
        holes,
        axis,
        folds,
        pinches,
        triangles: u32::try_from(laid).unwrap_or(u32::MAX),
        flat,
        spans,
        edges,
        across,
    })
}

/// The pattern's region: the outline and its holes (mitcad#2558), their
/// lines the generated curves from [`OUTLINE`] on, the outline's first.
fn pattern_region(
    feature: FeatureUid,
    outline: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
) -> Result<MaterialRegion, String> {
    if holes.is_empty() {
        return polygon(feature, OUTLINE, outline);
    }
    let mut slot = OUTLINE;
    let mut loops = Vec::with_capacity(holes.len() + 1);
    for points in std::iter::once(outline).chain(holes.iter().map(Vec::as_slice)) {
        let mut segments = Vec::with_capacity(points.len());
        for (i, start) in points.iter().enumerate() {
            segments.push(SheetSegment {
                id: SheetCurveId::Generated { feature, slot },
                geometry: SegmentGeometry::Line {
                    start: *start,
                    end: points[(i + 1) % points.len()],
                },
            });
            slot = slot.saturating_add(1);
        }
        loops.push(SheetLoop { segments });
    }
    MaterialRegion::from_legacy(&SheetRegion { loops })
}

/// The length of a flat polyline.
fn flat_length(points: &[[f64; 2]]) -> f64 {
    points.windows(2).map(|w| flat_distance(w[0], w[1])).sum()
}

/// A polygon's points without repeats, nor the first again at the end.
fn closed_loop(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points.dedup_by(|a, b| flat_distance(*a, *b) <= POINT);
    while points.len() > 1 && flat_distance(points[0], *points.last().expect("a point")) <= POINT {
        points.pop();
    }
    points
}

/// The barycentric coordinates of `p` seen on the plane of the triangle
/// `[a, b, c]`; None for a triangle of no area.
fn barycentric([a, b, c]: [V3; 3], p: V3) -> Option<[f64; 3]> {
    let (e0, e1, e2) = (sub(b, a), sub(c, a), sub(p, a));
    let (d00, d01, d11) = (dot(e0, e0), dot(e0, e1), dot(e1, e1));
    let (d20, d21) = (dot(e2, e0), dot(e2, e1));
    let denominator = d00 * d11 - d01 * d01;
    if denominator <= 1e-18 * (d00 * d11).max(POINT) {
        return None;
    }
    let l1 = (d11 * d20 - d01 * d21) / denominator;
    let l2 = (d00 * d21 - d01 * d20) / denominator;
    Some([1.0 - l1 - l2, l1, l2])
}

/// The pieces of an outline joined end to end into one loop, each either
/// way round; None when they do not close into one.
fn joined(mut pieces: Vec<Vec<[f64; 2]>>) -> Option<Vec<[f64; 2]>> {
    pieces.retain(|p| !p.is_empty());
    if pieces.is_empty() {
        return None;
    }
    let mut outline = pieces.swap_remove(0);
    while !pieces.is_empty() {
        let end = *outline.last()?;
        let (k, reversed, gap) = pieces
            .iter()
            .enumerate()
            .flat_map(|(k, p)| {
                [
                    (k, false, flat_distance(p[0], end)),
                    (k, true, flat_distance(*p.last().expect("a point"), end)),
                ]
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))?;
        if gap > JOIN {
            return None;
        }
        let mut piece = pieces.swap_remove(k);
        if reversed {
            piece.reverse();
        }
        outline.extend(piece.into_iter().skip(1));
    }
    (flat_distance(outline[0], *outline.last()?) <= JOIN).then_some(outline)
}

/// Two closed polygons cross or touch.
fn loops_cross(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    let sides = |p: &[[f64; 2]]| -> Vec<[[f64; 2]; 2]> {
        (0..p.len()).map(|i| [p[i], p[(i + 1) % p.len()]]).collect()
    };
    let (sa, sb) = (sides(a), sides(b));
    sa.iter()
        .any(|&s| sb.iter().any(|&t| segment_crossing(s, t).is_some()))
}

/// Where the segments `s` and `t` meet: the share along `s`; None when
/// they do not (or run along one another).
fn segment_crossing(s: [[f64; 2]; 2], t: [[f64; 2]; 2]) -> Option<f64> {
    let d = [s[1][0] - s[0][0], s[1][1] - s[0][1]];
    let e = [t[1][0] - t[0][0], t[1][1] - t[0][1]];
    let w = [t[0][0] - s[0][0], t[0][1] - s[0][1]];
    let denominator = d[0] * e[1] - d[1] * e[0];
    if denominator.abs() <= 1e-15 * (d[0].hypot(d[1]) * e[0].hypot(e[1])).max(POINT) {
        return None;
    }
    let u = (w[0] * e[1] - w[1] * e[0]) / denominator;
    let v = (w[0] * d[1] - w[1] * d[0]) / denominator;
    ((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)).then_some(u)
}

/// Whether `p` lies inside the material of `loops` (the outline and its
/// holes): inside an odd number of them.
fn inside(p: [f64; 2], loops: &[&[[f64; 2]]]) -> bool {
    let mut odd = false;
    for points in loops {
        for i in 0..points.len() {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
            {
                odd = !odd;
            }
        }
    }
    odd
}

/// The parts of the segment inside the material of `loops`.
fn clipped(segment: [[f64; 2]; 2], loops: &[&[[f64; 2]]]) -> Vec<[[f64; 2]; 2]> {
    let mut cuts = vec![0.0, 1.0];
    for points in loops {
        for i in 0..points.len() {
            let side = [points[i], points[(i + 1) % points.len()]];
            if let Some(u) = segment_crossing(segment, side) {
                cuts.push(u);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    let at = |u: f64| {
        [
            segment[0][0] + (segment[1][0] - segment[0][0]) * u,
            segment[0][1] + (segment[1][1] - segment[0][1]) * u,
        ]
    };
    let mut parts: Vec<[[f64; 2]; 2]> = Vec::new();
    for w in cuts.windows(2) {
        let (u0, u1) = (w[0], w[1]);
        if flat_distance(at(u0), at(u1)) <= POINT || !inside(at((u0 + u1) / 2.0), loops) {
            continue;
        }
        match parts.last_mut() {
            Some(last) if flat_distance(last[1], at(u0)) <= POINT => last[1] = at(u1),
            _ => parts.push([at(u0), at(u1)]),
        }
    }
    parts
}

/// Two sides of the closed polygon that are not neighbours cross or touch.
fn crosses_itself(points: &[[f64; 2]]) -> bool {
    crosses_itself_but(points, &[])
}

/// Two sides of the closed polygon that are not neighbours cross or touch,
/// other than two sides that both end at one of `allowed` (where the
/// polygon passes through a point twice, touching itself there).
fn crosses_itself_but(points: &[[f64; 2]], allowed: &[[f64; 2]]) -> bool {
    let n = points.len();
    let at_allowed = |i: usize| -> Option<[f64; 2]> {
        let (a, b) = (points[i], points[(i + 1) % n]);
        allowed
            .iter()
            .copied()
            .find(|q| flat_distance(a, *q) <= POINT || flat_distance(b, *q) <= POINT)
    };
    let side = |i: usize| (points[i], points[(i + 1) % n]);
    let orientation = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let boxes: Vec<[f64; 4]> = (0..n)
        .map(|i| {
            let (a, b) = side(i);
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[0].max(b[0]),
                a[1].max(b[1]),
            ]
        })
        .collect();
    for i in 0..n {
        for k in i + 2..n {
            if i == 0 && k == n - 1 {
                continue;
            }
            let (p, q) = (boxes[i], boxes[k]);
            if p[2] < q[0] - POINT
                || q[2] < p[0] - POINT
                || p[3] < q[1] - POINT
                || q[3] < p[1] - POINT
            {
                continue;
            }
            if let (Some(a), Some(b)) = (at_allowed(i), at_allowed(k))
                && flat_distance(a, b) <= POINT
            {
                continue;
            }
            let ((a, b), (c, d)) = (side(i), side(k));
            let (d1, d2) = (orientation(a, b, c), orientation(a, b, d));
            let (d3, d4) = (orientation(c, d, a), orientation(c, d, b));
            if d1 * d2 <= 0.0 && d3 * d4 <= 0.0 {
                return true;
            }
        }
    }
    false
}

/// The grids of a face's pieces over their parameters, `columns` by
/// `rows`, with their boundaries' edges at `edge_steps` steps (none for
/// 0), refused naming the face.
fn grid<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    shape: &K::Shape,
    face: &FaceName,
    [columns, rows]: [usize; 2],
    edge_steps: usize,
) -> Result<Vec<SheetFaceGrid>, String> {
    ctx.kernel
        .sheet_face_grid(shape, face, columns, rows, edge_steps)
        .map_err(|e| {
            let e = kernel_error(e);
            placed_at(
                vec![NoticePlace::face(face.to_string())],
                format!(
                    "the face could not be sampled: {}",
                    crate::notice::user_message(&e)
                ),
                format!("face {face} could not be sampled: {e}"),
            )
        })
}

/// Convert to Sheet Metal by triangulation (`development: triangulated`),
/// or None where the exact development applies: when the feature does not
/// ask for it, and for one flat, cylindrical or conical face (with a remark
/// that it was not needed).
pub(super) fn convert<K: Kernel>(
    d: &SheetConvertDef<f64>,
    ctx: &mut EvalContext<'_, K>,
    shape: &K::Shape,
    name: &FaceName,
    owner: crate::ids::ComponentUid,
    rules_version: &str,
    rules: &SheetRulesDef<f64>,
) -> Result<Option<FeatureOutput<K::Shape>>, String> {
    if d.development != Some(SheetDevelopment::Triangulated) {
        if let Some(face) = d.faces.first() {
            return Err(placed_at(
                vec![NoticePlace::face(face.to_string())],
                "further faces are laid flat with the picked one only by triangulation: choose \
                 the approximate development, or pick one face",
                format!(
                    "face {face}: further faces are laid flat with the picked one only by \
                     triangulation (development triangulated)"
                ),
            ));
        }
        return Ok(None);
    }
    // A name of pieces on different surfaces (a loft's side split where
    // the other section has a vertex) has no one surface: it is triangulated.
    let surface = ctx.kernel.face_geometry(shape, name).ok();
    if d.faces.is_empty()
        && matches!(
            surface,
            Some(
                SurfaceGeometry::Plane { .. }
                    | SurfaceGeometry::Cylinder { .. }
                    | SurfaceGeometry::Cone { .. }
            )
        )
    {
        ctx.inform(
            "the face is flat, cylindrical or conical and lies flat exactly: it is developed \
             exactly, and the approximate development is not needed",
        );
        return Ok(None);
    }
    if let Some(edge) = d.bends.first().or(d.rips.first()) {
        return Err(placed_at(
            vec![NoticePlace::edge(edge.to_string())],
            "faces laid flat by triangulation are converted on their own: bend and rip edges \
             join flat faces only",
            format!(
                "edge {edge}: faces laid flat by triangulation are converted on their own; bend \
                 and rip edges join flat faces only"
            ),
        ));
    }
    let mut names = vec![name.clone()];
    names.extend(ctx.faces_on(d.body, shape, &d.faces)?);
    for (i, face) in names.iter().enumerate() {
        if names[..i].contains(face) {
            return Err(placed_at(
                vec![NoticePlace::face(face.to_string())],
                "a face is picked more than once",
                format!("face {face} is picked more than once"),
            ));
        }
    }
    let t = rules.thickness;
    let k = super::curved::rolled_k(rules)?;
    let triangles = d.triangles.unwrap_or(DEFAULT_TRIANGLES);
    // The first look: how the faces join into a strip. A face the kernel
    // split (a loft's side where the other section has a vertex) is a face
    // of the strip for each piece; `pieces` holds the name's index and the
    // piece's. A planar face's grid runs between its rungs, the edges it
    // shares with the faces beside it (mitcad#2574), built from its
    // boundary.
    let mut sampled = Vec::new();
    let mut pieces: Vec<(usize, usize)> = Vec::new();
    for (n, face) in names.iter().enumerate() {
        for (p, g) in grid(ctx, shape, face, [PROBE, PROBE], EDGE_STEPS)?
            .into_iter()
            .enumerate()
        {
            sampled.push(g);
            pieces.push((n, p));
        }
    }
    let planes = plane_rungs(&sampled);
    let probes: Vec<SheetFaceGrid> = sampled
        .iter()
        .zip(&planes)
        .map(|(g, rungs)| {
            rungs
                .and_then(|r| plane_grid(g, r, [PROBE, PROBE]))
                .unwrap_or_else(|| g.clone())
        })
        .collect();
    let chain = chain(&probes).map_err(|(m, e)| {
        let face = &names[pieces[m].0];
        placed_at(
            vec![NoticePlace::face(face.to_string())],
            e.clone(),
            format!("face {face}: {e}"),
        )
    })?;
    let lengths: Vec<f64> = chain
        .faces
        .iter()
        .map(|&(m, orientation)| {
            rail_length(Oriented {
                grid: &probes[m],
                orientation,
            })
        })
        .collect();
    let shares = strips(&lengths, (triangles / 2.0).round().max(1.0) as usize);
    // The faces sampled FINE times finer than the strips along them.
    let mut grids = Vec::with_capacity(chain.faces.len());
    for (&(m, orientation), &count) in chain.faces.iter().zip(&shares) {
        let along = count * FINE;
        let size = if orientation.transpose {
            [ACROSS, along]
        } else {
            [along, ACROSS]
        };
        let (n, p) = pieces[m];
        let g = match planes[m] {
            // Built from the boundary sampled at the first look.
            Some(rungs) => plane_grid(&sampled[m], rungs, size)
                .ok_or("a face's grid does not fit its strips")?,
            None => grid(ctx, shape, &names[n], size, EDGE_STEPS)?
                .into_iter()
                .nth(p)
                .ok_or("a face's pieces changed while it was sampled")?,
        };
        // A face cut out of its surface, or with holes, is laid out by its
        // boundary (mitcad#2558); without it, it must fill its grid.
        let covered = grid_area(&g, 0.0);
        if !g.wires.iter().any(|w| w.outer)
            && (covered - g.area).abs() > FILLS * g.area.abs() + 1e-6
        {
            return Err(placed_at(
                vec![NoticePlace::face(names[n].to_string())],
                format!(
                    "the face is cut out of its surface (its area is {:.3} mm\u{b2}, its \
                     surface's between its parameter lines {covered:.3} mm\u{b2}): triangulation \
                     runs between two edges of a face bounded by its surface's parameter lines, \
                     as a loft's or a ruled face is",
                    g.area
                ),
                format!(
                    "face {} is cut out of its surface (area {:.6} mm\u{b2}, its surface's \
                     between its parameter lines {covered:.6} mm\u{b2})",
                    names[n], g.area
                ),
            ));
        }
        grids.push((g, orientation));
    }
    // The neutral surface from the inner side; the plate's volume at the
    // middle of the material: each face's own area (less its holes, only
    // what is cut out of its surface) as its grid's grows or shrinks there.
    let area = |depth: f64| -> f64 { grids.iter().map(|(g, _)| grid_area(g, depth)).sum() };
    let depth = if area(0.0) <= area(t) {
        k * t
    } else {
        (1.0 - k) * t
    };
    let plate = t * grids
        .iter()
        .map(|(g, _)| {
            let front = grid_area(g, 0.0);
            if front > POINT {
                g.area * grid_area(g, t / 2.0) / front
            } else {
                grid_area(g, t / 2.0)
            }
        })
        .sum::<f64>();
    let body = ctx
        .kernel
        .mass_properties(shape)
        .map_err(kernel_error)?
        .volume;
    if (plate - body).abs() > PLATE * body.abs().max(plate) {
        let places = names
            .iter()
            .map(|f| NoticePlace::face(f.to_string()))
            .collect();
        return Err(placed_at(
            places,
            format!(
                "the body is not a plate of its rules' thickness {t} mm behind the faces (its \
                 volume is {body:.3} mm\u{b3}, the faces' area times the thickness \
                 {plate:.3} mm\u{b3}): pick every face of its front, or split the body first"
            ),
            format!(
                "the body is not a plate of its rules' thickness {t} mm behind faces {} (volume \
                 {body:.6} mm\u{b3}, the faces' area times the thickness {plate:.6} mm\u{b3})",
                names
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    let gap = if chain.closed {
        let gap = d.corner_gap.unwrap_or(t);
        if !gap.is_finite() || gap <= TOLERANCE {
            return Err(placed_at(
                vec![NoticePlace::face(name.to_string())],
                "the faces close on themselves and are opened at a seam to lie flat: the corner \
                 gap there must be above 0",
                format!(
                    "the faces from face {name} close on themselves: they are opened at a seam to \
                     lie flat, and the corner gap there must be above 0"
                ),
            ));
        }
        gap
    } else {
        0.0
    };
    let oriented: Vec<Oriented<'_>> = grids
        .iter()
        .map(|(grid, orientation)| Oriented {
            grid,
            orientation: *orientation,
        })
        .collect();
    let layout = lay_flat(&oriented, depth, chain.closed, gap).map_err(|e| {
        let places = names
            .iter()
            .map(|f| NoticePlace::face(f.to_string()))
            .collect();
        placed_at(
            places,
            format!("the faces could not be laid flat by triangulation: {e}"),
            format!("the faces from face {name} could not be laid flat by triangulation: {e}"),
        )
    })?;
    // The pattern on the plane touching the faces at the strip's first
    // point, its rung along the chart's y axis (where the rails meet there,
    // the line to the next rung's second-rail end), seen from the front.
    let first = oriented[0];
    let origin = first.point(0, 0);
    let n0 = unit_or(first.normal(0, 0), first.normal(0, 1));
    let towards = if layout.axis == 3 {
        first.point(FINE, first.across())
    } else {
        first.point(0, first.across())
    };
    let rung = sub(towards, origin);
    let y = unit_or(sub(rung, scale(n0, dot(rung, n0))), [0.0; 3]);
    let x = cross(y, n0);
    if norm(x) <= 0.5 {
        return Err("the strip's first rung lies along the faces' normal".into());
    }
    let uid = ctx.uid;
    let mut region = pattern_region(uid, &layout.outline, &layout.holes)?;
    develop::orient(&mut region)?;
    let said = format!(
        "laid flat approximately by {} triangles: lengths along its edges differ from the \
         faces' by up to {:.3} mm, across it by up to {:.3} mm",
        layout.triangles, layout.edges, layout.across
    );
    ctx.inform(said);
    let state = SheetState {
        schema: 2,
        algorithm: ALGORITHM_VERSION,
        body: d.body,
        owner,
        shape_generation: String::new(),
        rules: d.rules,
        rules_version: rules_version.to_owned(),
        panels: Vec::new(),
        charts: vec![SheetPatch {
            domain: SheetDomainRef::Panel(PanelId {
                feature: uid,
                slot: 0,
            }),
            regions: vec![region],
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame {
                    origin,
                    x_axis: x,
                    y_axis: y,
                },
                front_to_back_sign: MaterialParity::Negative,
            },
        }],
        bends: Vec::new(),
        joins: Vec::new(),
        opened: Vec::new(),
        lineage: Vec::new(),
        overlays: Vec::new(),
        gaps: Vec::new(),
        invalidated: None,
        blockers: Vec::new(),
        approximation: Some(SheetApproximation {
            triangles: layout.triangles,
            edges: layout.edges,
            across: layout.across,
            seam: chain.closed,
            folds: layout.folds,
        }),
    };
    // The folded body is the body: the pattern is no development of it
    // that the kernel builds back.
    let mut output = FeatureOutput::default();
    finish(&mut output, state, rules, shape.clone())?;
    Ok(Some(output))
}

/// What a Flat Pattern of a sheet laid flat by triangulation says of it
/// (its properties, mitcad#2442): the triangles, the largest differences of
/// lengths along its edges and across it, whether it was opened at a seam,
/// and its fold lines in flat coordinates (`to_flat`), the angle in
/// degrees (positive: folded towards the side seen).
pub(crate) fn summary(state: &SheetState, to_flat: Option<Isometry2>, back: bool) -> Option<Value> {
    let a = state.approximation.as_ref()?;
    // The holes laid out with the strip (mitcad#2558): its region's loops
    // after the outline.
    let holes = state
        .charts
        .first()
        .and_then(|c| c.regions.first())
        .map_or(0, |r| r.loops.len().saturating_sub(1));
    let mut value = json!({
        "method": "triangulation",
        "triangles": a.triangles,
        "edges": a.edges,
        "across": a.across,
        "seam": a.seam,
        "holes": holes,
    });
    if let Some(map) = to_flat {
        value["folds"] = json!(
            a.folds
                .iter()
                .map(|f| json!({
                    "start": map.point(f.start),
                    "end": map.point(f.end),
                    "angle": (if back { -f.angle } else { f.angle }).to_degrees(),
                }))
                .collect::<Vec<_>>()
        );
    }
    Some(value)
}

/// The triangulation's fold lines as bend lines of the Flat Pattern's
/// drawing (mitcad#2442): up where the sheet folds towards the side seen.
pub(crate) fn fold_bends<S>(
    pattern: &super::super::flat::FlatPattern<S>,
) -> Vec<super::super::flat::FlatBend> {
    use super::super::flat::{FlatBend, FlatLine};
    let Some(a) = pattern.developed.approximation.as_ref() else {
        return Vec::new();
    };
    let Some(map) = pattern
        .domains
        .iter()
        .find(|d| d.domain == SheetDomainRef::Panel(pattern.def.stationary))
        .map(|d| d.to_flat)
    else {
        return Vec::new();
    };
    let back = pattern.def.reference_side == SheetReferenceSide::Back;
    a.folds
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let towards_seen = (f.angle > 0.0) != back;
            FlatBend {
                bend: BendId {
                    feature: pattern.def.stationary.feature,
                    slot: OUTLINE.saturating_add(u32::try_from(i).unwrap_or(u32::MAX)),
                },
                angle: f.angle,
                radius: 0.0,
                k_factor: 0.0,
                allowance: 0.0,
                direction: if towards_seen { "up" } else { "down" },
                centerlines: vec![FlatLine {
                    start: map.point(f.start),
                    end: map.point(f.end),
                }],
                tangents: Vec::new(),
                slits: Vec::new(),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "triangulated_tests.rs"]
mod tests;
