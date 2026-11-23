// SPDX-License-Identifier: MIT
//! The planning of Cell Grid (mitcad#2238; commands.md "Cell Grid"): a
//! pure 2D function from the field's loops and the grid's values to the
//! cells' closed outlines, their count and the open area. No kernel: the
//! feature (`super`) sweeps the planned outlines into one tool.
//!
//! # The field
//! The distinct loops of the picked profile regions (a region's hole and the
//! region filling it are one loop), nested: a point lies in the field when
//! an odd number of loops encloses it. An outer loop is a field, a loop
//! inside it a keep-out, a loop inside that a field again. Lines are taken
//! as they are; arcs, circles, ellipses and splines are followed by chords
//! within [`FLATTEN`] of the curve, and each chord carries how far the
//! curve bulges past it, so that clearances are never overestimated.
//!
//! # The lattice
//! With `s` the cell size (its inscribed diameter), `w` the web and
//! `p = s + w`, cell `(i, j)` lies at
//! - hexagons, and circles and polygons in staggered rows:
//!   `(i p sqrt(3) / 2, i p / 2 + j p)`, columns along y, every other
//!   column half a pitch up; a hexagon has a flat top at angle 0;
//! - squares, and circles and polygons in square rows: `(i p, j p)`;
//! - triangles: with `t = s / 2 + w / 2`, `(i sqrt(3) t, 3 j t)`, a
//!   triangle pointing down and `t` higher where `i + j` is odd;
//!
//! turned by `angle` about the anchor and moved to it: a cell's centre sits
//! on the anchor, so two fields with the same anchor, angle, shape, size
//! and web share one lattice. A polygon's pitch is its width across each
//! neighbour's direction plus the web, so that neighbours keep the web
//! between them; it has a flat side at the bottom.
//!
//! # Edge cells
//! - `whole`: only cells inside the field, each at least `margin` from
//!   every edge of the field and its keep-outs.
//! - `snap`: a cell an edge crosses becomes the largest piece of it cut
//!   along the cell's own lines that fits as a whole cell does: for a
//!   hexagon the 5/6 left by a corner cut between every second vertex, a
//!   half between opposite vertices, or a corner triangle; for a square a
//!   half across its middle or a diagonal half. Triangles, circles and
//!   polygons have no such lines: their edge cells are left out, as with
//!   `whole`.
//! - `free`: a cell an edge crosses is trimmed to the field as drawn (the
//!   feature intersects the cells with the field's prism); a trimmed piece
//!   smaller than [`MIN_TRIMMED`] of a cell is left out. No margin.
//!
//! Cells are listed column by column (`i`, then `j`); more than
//! [`MAX_CELLS`] are refused with the count.

use std::collections::{BTreeSet, HashMap};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6, PI, TAU};

use serde::{Deserialize, Serialize};

use crate::profile::{ProfileLoop, ProfileRegion, SegmentGeometry};
use crate::topo::RegionKey;

/// The most cells one Cell Grid cuts.
pub const MAX_CELLS: usize = 250;
/// The most sides of a polygon cell.
pub const MAX_SIDES: u32 = 64;
/// The smallest cell size and web, mm.
pub const MIN_SIZE: f64 = 1e-3;
/// How closely chords follow a curve of the field, mm.
pub const FLATTEN: f64 = 1e-4;
/// The smallest trimmed piece kept, as a part of a whole cell's area.
pub const MIN_TRIMMED: f64 = 0.01;
/// The most lattice positions looked at.
const MAX_CANDIDATES: f64 = 200_000.0;
/// How far a cell may reach past an edge and still be inside, mm: what
/// the geometry rounds.
const TOL: f64 = 1e-7;
/// How far apart the ends of a loop's segments may lie and the loop still
/// be closed, mm.
const CLOSED: f64 = 1e-4;
/// The most chords of one curve.
const MAX_CHORDS: usize = 4096;
/// The sides of the polygon a circle cell is clipped as (free cells' areas).
const CIRCLE_CLIP_SIDES: usize = 256;

/// The shape of the cells; each has the inscribed diameter `size`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellShape {
    /// Regular hexagons, a flat top at angle 0.
    #[default]
    Hexagon,
    /// Squares along the grid's axes.
    Square,
    /// Equilateral triangles pointing up and down in turn.
    Triangle,
    /// Circles of diameter `size`.
    Circle,
    /// Regular polygons of `sides` sides, a flat side at the bottom.
    Polygon,
}

/// How the centres of circle and polygon cells are laid out (hexagons,
/// squares and triangles have their own tilings).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellRows {
    /// Every other column half a pitch up, as hexagons stack.
    #[default]
    Staggered,
    /// Rows and columns.
    Square,
}

/// What happens to a cell an edge of the field or of a keep-out crosses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellClip {
    /// Only whole cells.
    #[default]
    Whole,
    /// The largest piece cut along the cell's own lines that fits.
    Snap,
    /// Trimmed to the field as drawn.
    Free,
}

/// The grid's values, mm and radians, in the sketch's coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSpec {
    pub shape: CellShape,
    /// A polygon's sides, 3 to [`MAX_SIDES`].
    pub sides: u32,
    pub rows: CellRows,
    /// A cell's inscribed diameter.
    pub size: f64,
    /// The wall between neighbouring cells.
    pub web: f64,
    /// The grid's turn about the anchor.
    pub angle: f64,
    /// A cell's centre sits here.
    pub anchor: [f64; 2],
    pub clip: CellClip,
    /// The least clearance from a cell to every edge (not with `free`).
    pub margin: f64,
}

/// What a planned cell is of its whole cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    Whole,
    /// A hexagon less a corner triangle.
    FiveSixths,
    Half,
    /// A hexagon's corner triangle.
    Sixth,
    /// A whole cell the field's prism trims (`free`).
    Trimmed,
}

/// A cell's outline in sketch coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum Outline {
    /// Corners counter-clockwise; `sides[k]` names the side from corner k
    /// to corner k + 1: the whole cell's side it lies on, or [`CHORD`].
    Polygon {
        points: Vec<[f64; 2]>,
        sides: Vec<u32>,
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
}

/// The side number of the line a snapped piece is cut along.
pub const CHORD: u32 = 100;

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedCell {
    /// The lattice index `(i, j)`.
    pub index: [i64; 2],
    pub piece: Piece,
    pub outline: Outline,
    /// Its open area, mm²: of a trimmed cell the part inside the field.
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub cells: Vec<PlannedCell>,
    /// The cells' area, mm².
    pub open_area: f64,
    /// The field's area, mm².
    pub field_area: f64,
    /// A whole cell's area, mm².
    pub cell_area: f64,
}

impl Plan {
    /// The open area as a part of the field's.
    pub fn open_fraction(&self) -> f64 {
        if self.field_area > 0.0 {
            self.open_area / self.field_area
        } else {
            0.0
        }
    }
}

/// The cells of the grid `spec` in the field of `loops` (see the module).
pub fn plan(loops: &[ProfileLoop], spec: &GridSpec) -> Result<Plan, String> {
    check(spec)?;
    let field = Field::new(loops)?;
    if field.area <= TOL {
        return Err("the profiles enclose no area to fill with cells".to_owned());
    }
    let lattice = Lattice::new(spec);
    let candidates = lattice.candidates(&field, spec)?;
    let cell_area = lattice.cell_area();
    let index = EdgeIndex::new(&field.edges, lattice.pitch());
    let mut cells = Vec::new();
    for (i, j) in candidates {
        let outline = lattice.outline(i, j, spec);
        if field.fits(&index, &outline, spec.margin) {
            cells.push(PlannedCell {
                index: [i, j],
                piece: Piece::Whole,
                area: cell_area,
                outline,
            });
            continue;
        }
        match spec.clip {
            CellClip::Whole => {}
            CellClip::Snap => {
                if let Some((piece, outline)) = lattice
                    .pieces(&outline)
                    .into_iter()
                    .find(|(_, piece)| field.fits(&index, piece, spec.margin))
                {
                    cells.push(PlannedCell {
                        index: [i, j],
                        piece,
                        area: outline.area(),
                        outline,
                    });
                }
            }
            CellClip::Free => {
                let area = field.overlap(&outline);
                if area >= MIN_TRIMMED * cell_area {
                    cells.push(PlannedCell {
                        index: [i, j],
                        piece: Piece::Trimmed,
                        area,
                        outline,
                    });
                }
            }
        }
    }
    if cells.is_empty() {
        return Err(format!(
            "no cell of size {} mm fits in the field: use smaller cells, a thinner web or margin, \
             or trimmed edge cells",
            short(spec.size)
        ));
    }
    if cells.len() > MAX_CELLS {
        return Err(too_many(cells.len(), false));
    }
    let open_area = cells.iter().map(|c| c.area).sum();
    Ok(Plan {
        cells,
        open_area,
        field_area: field.area,
        cell_area,
    })
}

/// The field of `loops` as regions to sweep (a trimming prism): each loop
/// at an even depth with the loops right inside it as its holes.
pub fn field_regions(loops: &[ProfileLoop]) -> Result<Vec<ProfileRegion>, String> {
    let field = Field::new(loops)?;
    let mut regions = Vec::new();
    for (k, outer) in field.sources.iter().enumerate() {
        let depth = field.depths[k];
        if depth % 2 == 1 {
            continue;
        }
        let Some(key) = RegionKey::new(outer.segments.iter().map(|s| s.key.clone())) else {
            continue;
        };
        let mut region_loops = vec![outer.clone()];
        region_loops.extend(
            field
                .sources
                .iter()
                .enumerate()
                .filter(|(m, _)| {
                    field.depths[*m] == depth + 1 && encloses(&field.loops[k], field.loops[*m][0])
                })
                .map(|(_, hole)| hole.clone()),
        );
        regions.push(ProfileRegion {
            key,
            loops: region_loops,
        });
    }
    Ok(regions)
}

/// The refusal of a grid of `count` cells, counted (`estimated` false) or
/// estimated by the field's area. One whole message per case, so that each
/// is translated whole (mitcad#2288).
fn too_many(count: usize, estimated: bool) -> String {
    if estimated {
        format!(
            "the grid would have about {count} cells, more than the {MAX_CELLS} one Cell Grid \
             cuts: use larger cells or a smaller field"
        )
    } else {
        format!(
            "the grid has {count} cells, more than the {MAX_CELLS} one Cell Grid cuts: use larger \
             cells or a smaller field"
        )
    }
}

/// A length for messages: at most 4 decimals, no trailing zeros.
fn short(value: f64) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// Refuses values that make no grid.
pub fn check(spec: &GridSpec) -> Result<(), String> {
    if !(spec.size.is_finite() && spec.size >= MIN_SIZE) {
        return Err(format!(
            "the cell size must be at least {MIN_SIZE} mm, got {}",
            spec.size
        ));
    }
    if !(spec.web.is_finite() && spec.web > 0.0) {
        return Err(format!(
            "the web between cells must be greater than zero, got {}",
            spec.web
        ));
    }
    if spec.web < MIN_SIZE {
        return Err(format!(
            "the web between cells must be at least {MIN_SIZE} mm, got {}",
            spec.web
        ));
    }
    if spec.shape == CellShape::Polygon && !(3..=MAX_SIDES).contains(&spec.sides) {
        return Err(format!(
            "a polygon cell has 3 to {MAX_SIDES} sides, got {}",
            spec.sides
        ));
    }
    if !(spec.margin.is_finite() && spec.margin >= 0.0) {
        return Err(format!(
            "the margin must not be negative, got {}",
            spec.margin
        ));
    }
    if spec.clip == CellClip::Free && spec.margin > 0.0 {
        return Err(
            "trimmed edge cells follow the field's edges as drawn, so they keep no margin: set \
             the margin to 0, or choose whole or snapped edge cells"
                .to_owned(),
        );
    }
    if !spec.angle.is_finite() || !spec.anchor.iter().all(|v| v.is_finite()) {
        return Err("the grid's angle and anchor must be numbers".to_owned());
    }
    Ok(())
}

// 2D helpers.

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// The polygon's signed area (counter-clockwise positive).
pub fn polygon_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|k| cross(points[k], points[(k + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// The box of the points (minimum, maximum).
fn bounds_of<'a>(points: impl Iterator<Item = &'a [f64; 2]>) -> ([f64; 2], [f64; 2]) {
    points.fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(min, max), p| {
            (
                [min[0].min(p[0]), min[1].min(p[1])],
                [max[0].max(p[0]), max[1].max(p[1])],
            )
        },
    )
}

fn centroid(points: &[[f64; 2]]) -> [f64; 2] {
    let n = points.len() as f64;
    let sum = points
        .iter()
        .fold([0.0, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
    [sum[0] / n, sum[1] / n]
}

/// The distance from `p` to the segment from `a` to `b`.
fn point_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 {
        (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    length(sub(p, lerp(a, b, t)))
}

/// The distance between two segments.
fn segment_segment(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    let d1 = cross(sub(b, a), sub(c, a));
    let d2 = cross(sub(b, a), sub(d, a));
    let d3 = cross(sub(d, c), sub(a, c));
    let d4 = cross(sub(d, c), sub(b, c));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return 0.0;
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

// The field.

/// A chord of the field's boundary and how far its curve bulges past it.
#[derive(Debug, Clone, Copy)]
struct Edge {
    a: [f64; 2],
    b: [f64; 2],
    bulge: f64,
}

impl Edge {
    fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        let b = self.bulge;
        (
            [self.a[0].min(self.b[0]) - b, self.a[1].min(self.b[1]) - b],
            [self.a[0].max(self.b[0]) + b, self.a[1].max(self.b[1]) + b],
        )
    }
}

struct Field {
    /// Each distinct loop as a closed polygon (the last point is not the
    /// first again).
    loops: Vec<Vec<[f64; 2]>>,
    /// The profile loops they are, in the same order.
    sources: Vec<ProfileLoop>,
    /// How many other loops enclose each loop.
    depths: Vec<usize>,
    edges: Vec<Edge>,
    area: f64,
    min: [f64; 2],
    max: [f64; 2],
}

impl Field {
    fn new(loops: &[ProfileLoop]) -> Result<Self, String> {
        let mut seen: Vec<BTreeSet<String>> = Vec::new();
        let mut polygons = Vec::new();
        let mut sources = Vec::new();
        let mut edges = Vec::new();
        for profile_loop in loops {
            let keys: BTreeSet<String> = profile_loop
                .segments
                .iter()
                .map(|s| s.key.to_string())
                .collect();
            if seen.contains(&keys) {
                continue;
            }
            seen.push(keys);
            let (points, bulges) = flatten_loop(profile_loop)?;
            if points.len() < 3 {
                continue;
            }
            let n = points.len();
            for k in 0..n {
                edges.push(Edge {
                    a: points[k],
                    b: points[(k + 1) % n],
                    bulge: bulges[k],
                });
            }
            polygons.push(points);
            sources.push(profile_loop.clone());
        }
        let depths: Vec<usize> = (0..polygons.len())
            .map(|k| {
                let probe = polygons[k][0];
                polygons
                    .iter()
                    .enumerate()
                    .filter(|(other, polygon)| *other != k && encloses(polygon, probe))
                    .count()
            })
            .collect();
        let area = polygons
            .iter()
            .zip(&depths)
            .map(|(polygon, depth)| {
                let a = polygon_area(polygon).abs();
                if depth % 2 == 0 { a } else { -a }
            })
            .sum();
        let (min, max) = bounds_of(polygons.iter().flatten());
        Ok(Self {
            loops: polygons,
            sources,
            depths,
            edges,
            area,
            min,
            max,
        })
    }

    /// Whether `p` lies in the field: inside an odd number of loops.
    fn inside(&self, p: [f64; 2]) -> bool {
        self.loops.iter().filter(|l| encloses(l, p)).count() % 2 == 1
    }

    /// Whether the cell (or piece) lies in the field with `margin` to every
    /// edge.
    fn fits(&self, index: &EdgeIndex, cell: &Outline, margin: f64) -> bool {
        if !self.inside(cell.inner_point()) {
            return false;
        }
        let (min, max) = cell.bounds();
        let reach = margin + TOL;
        index.near(
            [min[0] - reach, min[1] - reach],
            [max[0] + reach, max[1] + reach],
            |k| {
                let edge = &self.edges[k];
                if cell.penetrated_by(edge) {
                    return false;
                }
                margin <= 0.0 || cell.distance_to(edge) - edge.bulge >= margin - TOL
            },
        )
    }

    /// The area of the cell inside the field.
    fn overlap(&self, cell: &Outline) -> f64 {
        let clip = cell.polygon_for_clipping();
        self.loops
            .iter()
            .zip(&self.depths)
            .map(|(polygon, depth)| {
                let a = polygon_area(&clip_convex(polygon, &clip)).abs();
                if depth % 2 == 0 { a } else { -a }
            })
            .sum::<f64>()
            .max(0.0)
    }
}

/// Crossing-number test of `p` against a closed polygon.
fn encloses(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    for k in 0..n {
        let a = polygon[k];
        let b = polygon[(k + 1) % n];
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Sutherland-Hodgman: the part of `subject` (any simple polygon) inside
/// the convex counter-clockwise polygon `clip`. Where the part has several
/// pieces they are joined by edges of no area, so its area is right.
fn clip_convex(subject: &[[f64; 2]], clip: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = subject.to_vec();
    let n = clip.len();
    for k in 0..n {
        if out.is_empty() {
            break;
        }
        let a = clip[k];
        let b = clip[(k + 1) % n];
        let side = |p: [f64; 2]| cross(sub(b, a), sub(p, a));
        let input = std::mem::take(&mut out);
        let m = input.len();
        for q in 0..m {
            let p = input[q];
            let r = input[(q + 1) % m];
            let (sp, sr) = (side(p), side(r));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sr >= 0.0) {
                out.push(lerp(p, r, sp / (sp - sr)));
            }
        }
    }
    out
}

/// The loop's chords in order around it and each chord's bulge.
fn flatten_loop(profile_loop: &ProfileLoop) -> Result<(Vec<[f64; 2]>, Vec<f64>), String> {
    let pieces: Vec<(Vec<[f64; 2]>, f64)> = profile_loop
        .segments
        .iter()
        .map(|s| flatten(&s.geometry))
        .collect();
    let distance = |a: [f64; 2], b: [f64; 2]| length(sub(a, b));
    let mut points: Vec<[f64; 2]> = Vec::new();
    let mut bulges: Vec<f64> = Vec::new();
    let curve = |k: usize| {
        profile_loop
            .segments
            .get(k)
            .map(|s| s.key.to_string())
            .unwrap_or_default()
    };
    for (k, (samples, bulge)) in pieces.iter().enumerate() {
        let (Some(&first), Some(&last)) = (samples.first(), samples.last()) else {
            continue;
        };
        let reverse = match points.last() {
            Some(&end) => distance(end, last) < distance(end, first),
            // The first piece runs toward the second.
            None => pieces.get(k + 1).is_some_and(|(next, _)| {
                let near = |p: [f64; 2]| {
                    next.first()
                        .zip(next.last())
                        .map_or(f64::INFINITY, |(a, b)| distance(p, *a).min(distance(p, *b)))
                };
                near(first) < near(last)
            }),
        };
        let ordered: Vec<[f64; 2]> = if reverse {
            samples.iter().rev().copied().collect()
        } else {
            samples.clone()
        };
        if let Some(&end) = points.last() {
            let gap = distance(end, ordered[0]);
            if gap > CLOSED {
                return Err(format!(
                    "the field's loop is not closed: {} and {} lie {} mm apart",
                    curve(k - 1),
                    curve(k),
                    short(gap)
                ));
            }
            points.pop();
        }
        points.extend(ordered.iter().copied());
        bulges.extend(std::iter::repeat_n(*bulge, ordered.len() - 1));
    }
    if points.len() > 1 {
        let gap = distance(points[0], points[points.len() - 1]);
        if gap > CLOSED {
            return Err(format!(
                "the field's loop through {} is not closed: its ends lie {} mm apart",
                curve(0),
                short(gap)
            ));
        }
        points.pop();
    }
    Ok((points, bulges))
}

/// Points along a curve, its ends included, and how far the curve bulges
/// past each chord.
fn flatten(geometry: &SegmentGeometry) -> (Vec<[f64; 2]>, f64) {
    let arc = |center: [f64; 2], radius: f64, start: f64, sweep: f64| {
        let step = if radius > FLATTEN {
            2.0 * (1.0 - FLATTEN / radius).acos()
        } else {
            FRAC_PI_2
        };
        let n = ((sweep / step).ceil() as usize).clamp(2, MAX_CHORDS);
        let points = (0..=n)
            .map(|k| {
                let a = start + sweep * k as f64 / n as f64;
                [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
            })
            .collect();
        let bulge = radius * (1.0 - (sweep / n as f64 / 2.0).cos());
        (points, bulge)
    };
    let ellipse = |center: [f64; 2], a: f64, b: f64, rotation: f64, start: f64, sweep: f64| {
        // Chords of parameter step dt bulge at most a³ dt² / (8 b²).
        let (major, minor) = (a.max(b), a.min(b).max(1e-12));
        let dt = (8.0 * minor * minor * FLATTEN / major.powi(3)).sqrt();
        let n = ((sweep / dt).ceil() as usize).clamp(2, MAX_CHORDS);
        let dt = sweep / n as f64;
        let (c, s) = (rotation.cos(), rotation.sin());
        let points = (0..=n)
            .map(|k| {
                let t = start + dt * k as f64;
                let (x, y) = (a * t.cos(), b * t.sin());
                [center[0] + x * c - y * s, center[1] + x * s + y * c]
            })
            .collect();
        (points, major.powi(3) * dt * dt / (8.0 * minor * minor))
    };
    let sweep = |start: f64, end: f64| {
        let mut sweep = end - start;
        while sweep <= 0.0 {
            sweep += TAU;
        }
        sweep
    };
    match geometry {
        SegmentGeometry::Line { start, end } => (vec![*start, *end], 0.0),
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => arc(
            *center,
            *radius,
            *start_angle,
            sweep(*start_angle, *end_angle),
        ),
        SegmentGeometry::Circle {
            center,
            radius,
            start,
        } => arc(*center, *radius, *start, TAU),
        SegmentGeometry::Ellipse {
            center,
            major_radius,
            minor_radius,
            rotation,
        } => ellipse(*center, *major_radius, *minor_radius, *rotation, 0.0, TAU),
        SegmentGeometry::EllipseArc {
            center,
            major_radius,
            minor_radius,
            rotation,
            start_angle,
            end_angle,
        } => ellipse(
            *center,
            *major_radius,
            *minor_radius,
            *rotation,
            *start_angle,
            sweep(*start_angle, *end_angle),
        ),
        SegmentGeometry::BSpline { poles, .. } => {
            let (t0, t1) =
                mitcad_model_base::segments::bspline_range(geometry).unwrap_or((0.0, 1.0));
            let n = (poles.len() * 64).clamp(16, MAX_CHORDS);
            let at = |t: f64| mitcad_model_base::segments::bspline_point(geometry, t);
            let points: Vec<[f64; 2]> = (0..=n)
                .filter_map(|k| at(t0 + (t1 - t0) * k as f64 / n as f64))
                .collect();
            // The bulge: the largest gap between a chord's middle and the
            // curve's point between its ends, with room to spare.
            let bulge = (0..n)
                .filter_map(|k| {
                    let mid = at(t0 + (t1 - t0) * (k as f64 + 0.5) / n as f64)?;
                    Some(point_segment(mid, *points.get(k)?, *points.get(k + 1)?))
                })
                .fold(0.0, f64::max)
                * 1.5
                + FLATTEN;
            (points, bulge)
        }
    }
}

/// The field's edges by squares of the plane, to find those near a cell.
struct EdgeIndex {
    size: f64,
    buckets: HashMap<(i64, i64), Vec<usize>>,
    /// Edges too long to list by squares, looked at for every cell.
    long: Vec<usize>,
    edges: Vec<([f64; 2], [f64; 2])>,
}

impl EdgeIndex {
    fn new(edges: &[Edge], pitch: f64) -> Self {
        let size = pitch.max(1e-6);
        let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let mut long = Vec::new();
        let mut bounds = Vec::with_capacity(edges.len());
        for (k, edge) in edges.iter().enumerate() {
            let (min, max) = edge.bounds();
            bounds.push((min, max));
            let (i0, j0) = Self::square(min, size);
            let (i1, j1) = Self::square(max, size);
            if (i1 - i0 + 1).saturating_mul(j1 - j0 + 1) > 4096 {
                long.push(k);
                continue;
            }
            for i in i0..=i1 {
                for j in j0..=j1 {
                    buckets.entry((i, j)).or_default().push(k);
                }
            }
        }
        Self {
            size,
            buckets,
            long,
            edges: bounds,
        }
    }

    fn square(p: [f64; 2], size: f64) -> (i64, i64) {
        ((p[0] / size).floor() as i64, (p[1] / size).floor() as i64)
    }

    /// Calls `each` with the edges whose bounds meet the box from `min` to
    /// `max`, each once, until it returns false; false then.
    fn near(&self, min: [f64; 2], max: [f64; 2], mut each: impl FnMut(usize) -> bool) -> bool {
        let (i0, j0) = Self::square(min, self.size);
        let (i1, j1) = Self::square(max, self.size);
        let mut seen = BTreeSet::new();
        let meets = |k: usize| {
            let (a, b) = self.edges[k];
            a[0] <= max[0] && b[0] >= min[0] && a[1] <= max[1] && b[1] >= min[1]
        };
        for &k in &self.long {
            if meets(k) && !each(k) {
                return false;
            }
        }
        for i in i0..=i1 {
            for j in j0..=j1 {
                for &k in self.buckets.get(&(i, j)).map_or(&[][..], Vec::as_slice) {
                    if seen.insert(k) && meets(k) && !each(k) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

// The cells.

impl Outline {
    /// A point inside it.
    fn inner_point(&self) -> [f64; 2] {
        match self {
            Self::Polygon { points, .. } => centroid(points),
            Self::Circle { center, .. } => *center,
        }
    }

    fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        match self {
            Self::Polygon { points, .. } => bounds_of(points.iter()),
            Self::Circle { center, radius } => (
                [center[0] - radius, center[1] - radius],
                [center[0] + radius, center[1] + radius],
            ),
        }
    }

    /// Its area, mm².
    pub fn area(&self) -> f64 {
        match self {
            Self::Polygon { points, .. } => polygon_area(points).abs(),
            Self::Circle { radius, .. } => PI * radius * radius,
        }
    }

    /// Whether the edge (its curve: the chord with its bulge) goes into the
    /// cell further than [`TOL`].
    fn penetrated_by(&self, edge: &Edge) -> bool {
        let depth = TOL + edge.bulge;
        match self {
            Self::Circle { center, radius } => {
                point_segment(*center, edge.a, edge.b) < radius - depth
            }
            Self::Polygon { points, .. } => {
                // Cyrus-Beck: the part of the chord inside the cell shrunk
                // by `depth`.
                let (mut t0, mut t1) = (0.0f64, 1.0f64);
                let d = sub(edge.b, edge.a);
                let n = points.len();
                for k in 0..n {
                    let p = points[k];
                    let side = sub(points[(k + 1) % n], p);
                    let l = length(side);
                    if l <= 0.0 {
                        continue;
                    }
                    // Inward distance of a point x: cross(side, x - p) / l.
                    let at_a = cross(side, sub(edge.a, p)) / l - depth;
                    let rate = cross(side, d) / l;
                    if rate.abs() < 1e-300 {
                        if at_a <= 0.0 {
                            return false;
                        }
                        continue;
                    }
                    let t = -at_a / rate;
                    if rate > 0.0 {
                        t0 = t0.max(t);
                    } else {
                        t1 = t1.min(t);
                    }
                    if t0 >= t1 {
                        return false;
                    }
                }
                t1 - t0 > 1e-12
            }
        }
    }

    /// The distance from the chord to the cell, when it does not go in.
    fn distance_to(&self, edge: &Edge) -> f64 {
        match self {
            Self::Circle { center, radius } => {
                (point_segment(*center, edge.a, edge.b) - radius).max(0.0)
            }
            Self::Polygon { points, .. } => {
                let n = points.len();
                (0..n)
                    .map(|k| segment_segment(edge.a, edge.b, points[k], points[(k + 1) % n]))
                    .fold(f64::INFINITY, f64::min)
            }
        }
    }

    fn polygon_for_clipping(&self) -> Vec<[f64; 2]> {
        match self {
            Self::Polygon { points, .. } => points.clone(),
            Self::Circle { center, radius } => (0..CIRCLE_CLIP_SIDES)
                .map(|k| {
                    let a = TAU * k as f64 / CIRCLE_CLIP_SIDES as f64;
                    [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
                })
                .collect(),
        }
    }
}

/// The lattice of a grid (see the module).
struct Lattice {
    shape: CellShape,
    rows: CellRows,
    /// The pitch: between neighbouring centres (triangles: `t`, the big
    /// triangle's inscribed radius).
    p: f64,
    /// A cell's inscribed radius.
    r: f64,
    /// The cell's corners about its centre, counter-clockwise, unturned
    /// (triangles: up, then down); empty for a circle.
    shapes: Vec<Vec<[f64; 2]>>,
    /// How far a cell reaches from its centre.
    reach: f64,
}

impl Lattice {
    fn new(spec: &GridSpec) -> Self {
        let r = spec.size / 2.0;
        let s3 = 3f64.sqrt();
        let regular = |n: usize, first: f64| -> Vec<[f64; 2]> {
            let rc = r / (PI / n as f64).cos();
            (0..n)
                .map(|k| {
                    let a = first + TAU * k as f64 / n as f64;
                    [rc * a.cos(), rc * a.sin()]
                })
                .collect()
        };
        let (p, shapes) = match spec.shape {
            CellShape::Hexagon => (spec.size + spec.web, vec![regular(6, 0.0)]),
            CellShape::Square => (
                spec.size + spec.web,
                vec![vec![[-r, -r], [r, -r], [r, r], [-r, r]]],
            ),
            CellShape::Triangle => (
                r + spec.web / 2.0,
                vec![
                    vec![[-s3 * r, -r], [s3 * r, -r], [0.0, 2.0 * r]],
                    vec![[s3 * r, r], [-s3 * r, r], [0.0, -2.0 * r]],
                ],
            ),
            CellShape::Circle => (spec.size + spec.web, Vec::new()),
            CellShape::Polygon => {
                let n = spec.sides as usize;
                let polygon = regular(n, -FRAC_PI_2 + PI / n as f64);
                let width = |angle: f64| {
                    let u = [angle.cos(), angle.sin()];
                    let (lo, hi) = polygon.iter().fold((f64::MAX, f64::MIN), |(lo, hi), q| {
                        let t = dot(*q, u);
                        (lo.min(t), hi.max(t))
                    });
                    hi - lo
                };
                let directions: &[(f64, f64)] = match spec.rows {
                    CellRows::Staggered => &[(FRAC_PI_2, 1.0), (FRAC_PI_6, 1.0), (-FRAC_PI_6, 1.0)],
                    CellRows::Square => &[
                        (0.0, 1.0),
                        (FRAC_PI_2, 1.0),
                        (FRAC_PI_4, std::f64::consts::SQRT_2),
                        (-FRAC_PI_4, std::f64::consts::SQRT_2),
                    ],
                };
                let p = directions
                    .iter()
                    .map(|&(angle, reach)| (width(angle) + spec.web) / reach)
                    .fold(0.0, f64::max);
                (p, vec![polygon])
            }
        };
        let reach = shapes
            .iter()
            .flatten()
            .map(|q| length(*q))
            .fold(r, f64::max);
        Self {
            shape: spec.shape,
            rows: spec.rows,
            p,
            r,
            shapes,
            reach,
        }
    }

    /// The distance between neighbouring centres, about.
    fn pitch(&self) -> f64 {
        match self.shape {
            CellShape::Triangle => 2.0 * 3f64.sqrt() * self.p,
            _ => self.p,
        }
    }

    fn staggered(&self) -> bool {
        match self.shape {
            CellShape::Hexagon => true,
            CellShape::Circle | CellShape::Polygon => self.rows == CellRows::Staggered,
            CellShape::Square | CellShape::Triangle => false,
        }
    }

    /// Cell `(i, j)`'s centre, unturned, from the anchor.
    fn center(&self, i: i64, j: i64) -> [f64; 2] {
        let (fi, fj) = (i as f64, j as f64);
        let s3 = 3f64.sqrt();
        match self.shape {
            CellShape::Triangle => {
                let up = (i + j).rem_euclid(2) == 0;
                [
                    fi * s3 * self.p,
                    3.0 * fj * self.p + if up { 0.0 } else { self.p },
                ]
            }
            _ if self.staggered() => [fi * self.p * s3 / 2.0, fi * self.p / 2.0 + fj * self.p],
            _ => [fi * self.p, fj * self.p],
        }
    }

    /// The area of a whole cell.
    fn cell_area(&self) -> f64 {
        match self.shapes.first() {
            Some(points) => polygon_area(points).abs(),
            None => PI * self.r * self.r,
        }
    }

    /// The lattice indices whose cells may reach into the field, refused
    /// when they would be too many.
    fn candidates(&self, field: &Field, spec: &GridSpec) -> Result<Vec<(i64, i64)>, String> {
        // The field's box in the lattice's own axes.
        let (c, s) = (spec.angle.cos(), spec.angle.sin());
        let corners: Vec<[f64; 2]> = [field.min[0], field.max[0]]
            .into_iter()
            .flat_map(|x| [field.min[1], field.max[1]].map(|y| [x, y]))
            .map(|[x, y]| {
                let d = [x - spec.anchor[0], y - spec.anchor[1]];
                [d[0] * c + d[1] * s, -d[0] * s + d[1] * c]
            })
            .collect();
        let (low, high) = bounds_of(corners.iter());
        let min = [low[0] - self.reach, low[1] - self.reach];
        let max = [high[0] + self.reach, high[1] + self.reach];
        // About how many cells the field holds, by area.
        let per_cell = match self.shape {
            // A triangle of inscribed radius t per position.
            CellShape::Triangle => 3.0 * 3f64.sqrt() * self.p * self.p,
            _ if self.staggered() => self.p * self.p * 3f64.sqrt() / 2.0,
            _ => self.p * self.p,
        };
        let estimate = field.area / per_cell;
        if estimate > (4 * MAX_CELLS) as f64 {
            return Err(too_many(estimate.round() as usize, true));
        }
        let s3 = 3f64.sqrt();
        let (column, row) = match self.shape {
            CellShape::Triangle => (s3 * self.p, 3.0 * self.p),
            _ if self.staggered() => (self.p * s3 / 2.0, self.p),
            _ => (self.p, self.p),
        };
        let i0 = (min[0] / column).floor() - 1.0;
        let i1 = (max[0] / column).ceil() + 1.0;
        let j0 = (min[1] / row).floor() - 1.0;
        let j1 = (max[1] / row).ceil() + 1.0;
        let positions = (i1 - i0 + 1.0) * (j1 - j0 + 1.0);
        if positions.is_nan() || positions > MAX_CANDIDATES {
            return Err(format!(
                "the field spans {} lattice positions, too many to plan: use larger cells or a \
                 smaller field",
                positions.round()
            ));
        }
        let (i0, i1, j0, j1) = (i0 as i64, i1 as i64, j0 as i64, j1 as i64);
        let mut out = Vec::new();
        for i in i0..=i1 {
            // A staggered column is half a pitch up.
            let shift = if self.staggered() { i.div_euclid(2) } else { 0 };
            let (from, to) = if self.staggered() {
                (j0 - shift - 1, j1 - shift + 1)
            } else {
                (j0, j1)
            };
            for j in from..=to {
                let center = self.center(i, j);
                if center[0] + self.reach < min[0]
                    || center[0] - self.reach > max[0]
                    || center[1] + self.reach < min[1]
                    || center[1] - self.reach > max[1]
                {
                    continue;
                }
                out.push((i, j));
            }
        }
        Ok(out)
    }

    /// Cell `(i, j)`'s outline in sketch coordinates.
    fn outline(&self, i: i64, j: i64, spec: &GridSpec) -> Outline {
        let (c, s) = (spec.angle.cos(), spec.angle.sin());
        let place = |q: [f64; 2]| {
            [
                spec.anchor[0] + q[0] * c - q[1] * s,
                spec.anchor[1] + q[0] * s + q[1] * c,
            ]
        };
        let center = self.center(i, j);
        if self.shape == CellShape::Circle {
            return Outline::Circle {
                center: place(center),
                radius: self.r,
            };
        }
        let key = match self.shape {
            CellShape::Triangle if (i + j).rem_euclid(2) == 1 => 1,
            _ => 0,
        };
        let corners = &self.shapes[key];
        Outline::Polygon {
            points: corners
                .iter()
                .map(|q| place([center[0] + q[0], center[1] + q[1]]))
                .collect(),
            sides: (0..corners.len() as u32).collect(),
        }
    }

    /// The pieces a snapped cell may become, largest first (see the
    /// module): none for triangles, circles and polygons.
    fn pieces(&self, cell: &Outline) -> Vec<(Piece, Outline)> {
        let Outline::Polygon { points, .. } = cell else {
            return Vec::new();
        };
        let corner = |k: usize| points[k % points.len()];
        let piece = |kind: Piece, corners: Vec<([f64; 2], u32)>| {
            let (points, sides) = corners.into_iter().unzip();
            (kind, Outline::Polygon { points, sides })
        };
        let side = |k: usize| (k % points.len()) as u32;
        let mut out = Vec::new();
        match self.shape {
            CellShape::Hexagon => {
                // Less the corner k + 1: k, then k + 2 .. k + 5.
                for k in 0..6 {
                    out.push(piece(
                        Piece::FiveSixths,
                        vec![
                            (corner(k), CHORD),
                            (corner(k + 2), side(k + 2)),
                            (corner(k + 3), side(k + 3)),
                            (corner(k + 4), side(k + 4)),
                            (corner(k + 5), side(k + 5)),
                        ],
                    ));
                }
                for k in 0..6 {
                    out.push(piece(
                        Piece::Half,
                        vec![
                            (corner(k), side(k)),
                            (corner(k + 1), side(k + 1)),
                            (corner(k + 2), side(k + 2)),
                            (corner(k + 3), CHORD),
                        ],
                    ));
                }
                for k in 0..6 {
                    out.push(piece(
                        Piece::Sixth,
                        vec![
                            (corner(k), side(k)),
                            (corner(k + 1), side(k + 1)),
                            (corner(k + 2), CHORD),
                        ],
                    ));
                }
            }
            CellShape::Square => {
                // Halves across the middle: the corners k and k + 1 with
                // the middles of the two sides beside them.
                for k in 0..4 {
                    let next = lerp(corner(k + 1), corner(k + 2), 0.5);
                    let before = lerp(corner(k + 3), corner(k), 0.5);
                    out.push(piece(
                        Piece::Half,
                        vec![
                            (corner(k), side(k)),
                            (corner(k + 1), side(k + 1)),
                            (next, CHORD),
                            (before, side(k + 3)),
                        ],
                    ));
                }
                for k in 0..4 {
                    out.push(piece(
                        Piece::Half,
                        vec![
                            (corner(k), side(k)),
                            (corner(k + 1), side(k + 1)),
                            (corner(k + 2), CHORD),
                        ],
                    ));
                }
            }
            CellShape::Triangle | CellShape::Circle | CellShape::Polygon => {}
        }
        out
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
