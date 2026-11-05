// SPDX-License-Identifier: MIT
//! Annotations on a drawing's views (mitcad#707, phase 2): dimensions
//! (linear, aligned, angular, radius, diameter, ordinate), centre marks and
//! centrelines, leader notes, datum labels, feature control frames and
//! surface finish symbols, each on one view and kept with it; and text
//! notes on the sheet.
//!
//! The annotations of mitcad#702 and these are one engine: the same value
//! formats, tolerances and ISO 286 fit codes, datum letters and geometric
//! tolerances (`core/model/src/document/annotations.rs`), and the same words for a
//! value (`model-api/src/annotations.rs` writes them). What a view adds is where it
//! is drawn: on the sheet, in the view's plane.
//!
//! A pick ([`ViewPick`]) names model topology, not a point of the sheet: a
//! body (with the occurrence that places it, from the drawing's component)
//! and one of its edges or faces by its topological name, or a point of an
//! edge (its start, end or a circle's centre). The application picks on the
//! sheet; the model finds the edge or face whose projection passes there
//! (`model-api/src/drawing_annotations.rs`), so the annotation follows the model.
//! When a pick is gone the annotation is broken: it is not drawn, and the
//! `drawing` query says why.
//!
//! Places are kept relative to the picks in millimetres of paper: a
//! dimension line's distance from its measured points (`line_offset`), the
//! text's shift from its default place (`text_offset`), a leader's text
//! from its arrow. So an annotation keeps its look at any scale and moves
//! with its view and with the model.
//!
//! This module lays an annotation out on the sheet from its picks as its
//! view sees them ([`Seen`]), without the model. The definitions are part
//! of the model's stored drawings (`annotate_def`, re-exported here,
//! mitcad#777).

use std::f64::consts::{FRAC_1_SQRT_2, PI, TAU};

use mitcad_vec3::vec2::{self, perp};
use mitcad_vec3::vec2::{add as add2, dot as dot2, norm as len2, scale as mul2, sub as sub2};

// The stored annotations (mitcad#777: a part of the model's drawing
// definitions), under their old paths.
pub use mitcad_model::drawing::annotate_def::*;

use super::P2;
use super::clip::ViewMap;
use super::compose::{ARROW_LENGTH, Bounds, EXTENSION_BEYOND, EXTENSION_GAP, arrow, upright};
use super::sheet::{Anchor, Group, Item, Pen, text_width};
use mitcad_model::Characteristic;
use mitcad_model::datum::{CurveGeometry, Vec3};
use mitcad_model::topo::{EdgeName, FaceName};

/// A dimension line's distance from what it measures (paper mm) when it is
/// placed for itself, beyond the view's box.
pub const LINE_GAP: f64 = 10.0;
/// A leader's text from its arrow (paper mm) when not moved.
pub const NOTE_SHIFT: P2 = [10.0, 8.0];
/// A datum label's square (its lower middle) from its triangle's tip on
/// the feature (paper mm) when not moved.
pub const DATUM_SHIFT: P2 = [0.0, 12.0];
/// How far centre lines reach beyond their circle or cylinder, paper mm.
pub const CENTER_BEYOND: f64 = 2.5;
/// The side of a datum label's square and a frame's cells, paper mm.
const CELL: f64 = 7.0;
/// A balloon's circle from its arrow (paper mm) when not moved, and its
/// radius at least.
pub const BALLOON_SHIFT: P2 = [12.0, 10.0];
const BALLOON_RADIUS: f64 = 3.5;
/// How near a pick must be to the click (paper mm), and a point nearer
/// than an edge to win over it.
pub const PICK_REACH: f64 = 3.0;
pub const POINT_REACH: f64 = 2.0;
/// How far an edge extension of one edge reaches beyond each end when it
/// has no length of its own, paper mm (mitcad#729).
pub const EXTENSION_LENGTH: f64 = 10.0;

/// The pen a line or an arc drawn outside the model is drawn with.
pub fn sketch_pen(pen: SketchPen) -> Pen {
    match pen {
        SketchPen::Thin => Pen::Thin,
        SketchPen::Thick => Pen::Thick,
        SketchPen::Hidden => Pen::Hidden,
        SketchPen::Chain => Pen::Chain,
    }
}

/// A line, an arc or a text drawn outside the model laid out on the
/// sheet through its map (an identity map for one on the sheet): its group
/// (`sketch`, `A<id>`) and box. `text`: the letters' height without its
/// own.
pub fn lay_out_sketch(item: &SketchItem, map: &ViewMap, text: f64) -> Laid {
    let mut g = Group::new("sketch");
    g.id = Some(format!("A{}", item.id()));
    match item {
        SketchItem::Line { from, to, pen, .. } => {
            g.line(sketch_pen(*pen), map.at(*from), map.at(*to))
        }
        SketchItem::Arc {
            center,
            radius,
            start,
            end,
            pen,
            ..
        } => {
            if map.breaks.is_empty() {
                g.items.push(Item::Arc {
                    pen: sketch_pen(*pen),
                    center: map.at(*center),
                    radius: radius * map.scale,
                    start: *start,
                    end: *end,
                });
            } else {
                // Across a break's band the arc is no arc on the sheet.
                let n = ((end - start) / TAU * 96.0).ceil().max(4.0) as usize;
                let points = (0..=n)
                    .map(|i| {
                        let a = start + (end - start) * i as f64 / n as f64;
                        map.at(add2(*center, mul2([a.cos(), a.sin()], *radius)))
                    })
                    .collect();
                g.items.push(Item::Polyline {
                    pen: sketch_pen(*pen),
                    points,
                    closed: false,
                });
            }
        }
        SketchItem::Text {
            text: words,
            at,
            height,
            ..
        } => {
            let h = height.unwrap_or(text);
            let start = map.at(*at);
            for (i, line) in words.lines().enumerate() {
                g.text(
                    [start[0], start[1] - i as f64 * h * 1.6],
                    h,
                    0.0,
                    Anchor::Start,
                    line.trim_end(),
                );
            }
        }
    }
    let bounds = group_bounds(&g);
    Laid { group: g, bounds }
}

/// A body's named edges, round faces and planar faces, in its own
/// coordinates, as the kernel gives them: what picks on views are found
/// among and centre marks are drawn on (planar faces: the cut edges of a
/// section view). Kept by the body's shape version (`ProjectionCache`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BodyGeometry {
    pub edges: Vec<(EdgeName, CurveGeometry)>,
    pub cylinders: Vec<RoundFace>,
    pub planes: Vec<FlatFace>,
}

/// A cylinder's face: its name (and the others the kernel gives it), a
/// point of its axis, the unit axis and its radius.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundFace {
    pub face: FaceName,
    pub names: Vec<FaceName>,
    pub origin: Vec3,
    pub axis: Vec3,
    pub radius: f64,
}

/// A planar face: its name (and its others), a point of it and its unit
/// normal.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatFace {
    pub face: FaceName,
    pub names: Vec<FaceName>,
    pub origin: Vec3,
    pub normal: Vec3,
}

impl BodyGeometry {
    /// The round face of this name, if the body has one.
    pub fn cylinder(&self, face: &FaceName) -> Option<&RoundFace> {
        self.cylinders
            .iter()
            .find(|c| c.face == *face || c.names.contains(face))
    }

    /// The planar face of this name, if the body has one.
    pub fn plane(&self, face: &FaceName) -> Option<&FlatFace> {
        self.planes
            .iter()
            .find(|p| p.face == *face || p.names.contains(face))
    }
}

// --- What a view sees of a pick ---------------------------------------------

/// A pick as its view sees it, in the view's coordinates (model mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seen {
    Point(P2),
    /// A straight edge, or a curve's chord.
    Line(P2, P2),
    /// A circle or an arc: its centre and radius (its true radius: a
    /// circle's diameter across `across` is seen at full length), whether
    /// it is seen end on (as a circle), and that direction (unit). An arc
    /// (`arc`, none for a whole circle) is where it is: seen end on, its
    /// angles in the view, counter-clockwise from the first to the second;
    /// from the side, how far it reaches along `across` from the centre,
    /// the lower and the higher (mitcad#717: arcs were whole circles).
    Circle {
        center: P2,
        radius: f64,
        end_on: bool,
        across: P2,
        arc: Option<(f64, f64)>,
    },
    /// A round face seen from the side: its axis from `a` to `b` and its
    /// radius.
    Axis {
        a: P2,
        b: P2,
        radius: f64,
    },
}

impl Seen {
    /// The point that stands for it: a point, a line's middle, a circle's
    /// centre, the middle of an axis.
    pub fn point(&self) -> P2 {
        match *self {
            Self::Point(p) => p,
            Self::Line(a, b) | Self::Axis { a, b, .. } => mid(a, b),
            Self::Circle { center, .. } => center,
        }
    }

    /// How far a view point is from it, view mm.
    pub fn distance(&self, q: P2) -> f64 {
        match *self {
            Self::Axis { a, b, radius } => {
                let n = mul2(perp(unit2(sub2(b, a))), radius);
                segment_distance(q, add2(a, n), add2(b, n))
                    .min(segment_distance(q, sub2(a, n), sub2(b, n)))
                    .min(segment_distance(q, a, b))
            }
            _ => len2(sub2(q, self.nearest(q))),
        }
    }

    /// The point of it nearest a view point (a leader's arrow).
    pub fn nearest(&self, q: P2) -> P2 {
        match *self {
            Self::Point(p) => p,
            Self::Line(a, b) => segment_foot(q, a, b),
            Self::Circle {
                center,
                radius,
                end_on,
                across,
                arc,
            } => {
                if end_on {
                    let d = sub2(q, center);
                    let angle = d[1].atan2(d[0]);
                    let on = |a: f64| add2(center, mul2([a.cos(), a.sin()], radius));
                    match arc {
                        Some((start, end)) if !in_sweep(angle, start, end) => {
                            // Off the arc: its nearer end.
                            let (s, e) = (on(start), on(end));
                            if len2(sub2(q, s)) <= len2(sub2(q, e)) {
                                s
                            } else {
                                e
                            }
                        }
                        _ => on(angle),
                    }
                } else {
                    let (low, high) = arc.unwrap_or((-radius, radius));
                    segment_foot(
                        q,
                        add2(center, mul2(across, low)),
                        add2(center, mul2(across, high)),
                    )
                }
            }
            Self::Axis { a, b, radius } => {
                let n = perp(unit2(sub2(b, a)));
                let foot = segment_foot(q, a, b);
                let side = if dot2(sub2(q, foot), n) >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                add2(foot, mul2(n, side * radius))
            }
        }
    }

    /// Which way it runs at its point nearest a view point (unit): a
    /// line's direction, a circle's tangent there (seen from the side, its
    /// line across), a round face's outline along its axis; None for a
    /// point (mitcad#1294: a datum's triangle sits with its base along
    /// it).
    pub fn along(&self, q: P2) -> Option<P2> {
        match *self {
            Self::Point(_) => None,
            Self::Line(a, b) | Self::Axis { a, b, .. } => vec2::unit(sub2(b, a)),
            Self::Circle {
                center,
                end_on: true,
                ..
            } => vec2::unit(sub2(self.nearest(q), center)).map(perp),
            Self::Circle { across, .. } => vec2::unit(across),
        }
    }

    /// How far along it its point nearest a view point lies, as a fraction
    /// from its start (mitcad#1295): a line's from `a` to `b`, a round
    /// face's along its axis, an arc's from its first end to its second,
    /// a circle seen from the side across from its low end; None for a
    /// point and a whole circle seen end on.
    pub fn fraction(&self, q: P2) -> Option<f64> {
        let along = |a: P2, b: P2| {
            let ab = sub2(b, a);
            let length = dot2(ab, ab);
            (length > 1e-24).then(|| (dot2(sub2(q, a), ab) / length).clamp(0.0, 1.0))
        };
        match *self {
            Self::Point(_) => None,
            Self::Line(a, b) | Self::Axis { a, b, .. } => along(a, b),
            Self::Circle {
                center,
                radius,
                end_on: false,
                across,
                arc,
            } => {
                let (low, high) = arc.unwrap_or((-radius, radius));
                along(
                    add2(center, mul2(across, low)),
                    add2(center, mul2(across, high)),
                )
            }
            Self::Circle {
                center,
                arc: Some((start, end)),
                ..
            } => {
                let sweep = end - start;
                if sweep <= 1e-12 {
                    return None;
                }
                let d = sub2(self.nearest(q), center);
                let turned = (d[1].atan2(d[0]) - start).rem_euclid(TAU);
                // Its first end, a whole turn on.
                let turned = if turned > sweep + 1e-9 { 0.0 } else { turned };
                Some((turned / sweep).clamp(0.0, 1.0))
            }
            Self::Circle { .. } => None,
        }
    }

    /// Its point a fraction `t` of the way from its start, as
    /// [`Seen::fraction`] measures it (a round face's outline on the side
    /// of `near`); None where that has none. A line's runs on along its
    /// line below 0 and above 1 (a datum on an edge's extension,
    /// mitcad#1296); the others' stop at their ends.
    pub fn at_fraction(&self, t: f64, near: P2) -> Option<P2> {
        if let Self::Line(a, b) = *self {
            return Some(add2(a, mul2(sub2(b, a), t)));
        }
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: P2, b: P2| add2(a, mul2(sub2(b, a), t));
        match *self {
            Self::Point(_) => None,
            Self::Line(a, b) => Some(lerp(a, b)),
            Self::Axis { a, b, radius } => {
                let foot = lerp(a, b);
                let n = perp(unit2(sub2(b, a)));
                let side = if dot2(sub2(near, foot), n) >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                Some(add2(foot, mul2(n, side * radius)))
            }
            Self::Circle {
                center,
                radius,
                end_on: false,
                across,
                arc,
            } => {
                let (low, high) = arc.unwrap_or((-radius, radius));
                Some(add2(center, mul2(across, low + (high - low) * t)))
            }
            Self::Circle {
                center,
                radius,
                arc: Some((start, end)),
                ..
            } => {
                let a = start + (end - start) * t;
                Some(add2(center, mul2([a.cos(), a.sin()], radius)))
            }
            Self::Circle { .. } => None,
        }
    }
}

pub(crate) fn mid(a: P2, b: P2) -> P2 {
    mul2(add2(a, b), 0.5)
}

/// Whether an angle lies in the counter-clockwise range from `start` to
/// `end` (radians).
fn in_sweep(angle: f64, start: f64, end: f64) -> bool {
    (angle - start).rem_euclid(TAU) <= end - start + 1e-12
}

/// How far an arc of a circle (`start` to `end` counter-clockwise about
/// `normal`, the same point for a whole circle) turns, radians: a whole
/// circle's 2 pi.
pub fn arc_sweep(center: Vec3, normal: Vec3, radius: f64, start: Vec3, end: Vec3) -> f64 {
    if mitcad_vec3::distance(start, end) < 1e-6 * radius.max(1.0) {
        return TAU;
    }
    let n = mitcad_vec3::unit(normal).unwrap_or([0.0, 0.0, 1.0]);
    let u = mitcad_vec3::sub(start, center);
    let w = mitcad_vec3::sub(end, center);
    let turn = mitcad_vec3::dot(mitcad_vec3::cross(u, w), n).atan2(mitcad_vec3::dot(u, w));
    turn.rem_euclid(TAU)
}

/// Points along an arc of a circle in 3D (as [`arc_sweep`] has it), its
/// ends among them.
pub fn arc_points3(center: Vec3, normal: Vec3, radius: f64, start: Vec3, end: Vec3) -> Vec<Vec3> {
    let sweep = arc_sweep(center, normal, radius, start, end);
    let n = mitcad_vec3::unit(normal).unwrap_or([0.0, 0.0, 1.0]);
    let u = mitcad_vec3::unit(mitcad_vec3::sub(start, center)).unwrap_or([1.0, 0.0, 0.0]);
    let v = mitcad_vec3::cross(n, u);
    (0..=32)
        .map(|i| {
            let a = sweep * f64::from(i) / 32.0;
            let (s, c) = a.sin_cos();
            mitcad_vec3::add(
                center,
                mitcad_vec3::add(
                    mitcad_vec3::scale(u, c * radius),
                    mitcad_vec3::scale(v, s * radius),
                ),
            )
        })
        .collect()
}

/// The unit vector along `a`, or the x axis when `a` has no length.
pub(crate) fn unit2(a: P2) -> P2 {
    vec2::unit(a).unwrap_or([1.0, 0.0])
}

fn segment_foot(q: P2, a: P2, b: P2) -> P2 {
    let ab = sub2(b, a);
    let length = dot2(ab, ab);
    if length < 1e-24 {
        return a;
    }
    let t = (dot2(sub2(q, a), ab) / length).clamp(0.0, 1.0);
    add2(a, mul2(ab, t))
}

/// How far a point is from the segment from `a` to `b`.
pub fn segment_distance(q: P2, a: P2, b: P2) -> f64 {
    len2(sub2(q, segment_foot(q, a, b)))
}

/// How a view sees a curve (in the drawing's component coordinates): the
/// view's projection `project`, the direction to its eye `eye`; a point
/// of it when `point` is given.
pub fn seen_curve(
    curve: &CurveGeometry,
    point: Option<PickPoint>,
    project: impl Fn(Vec3) -> P2,
    eye: Vec3,
) -> Result<Seen, String> {
    let (start, end) = match curve {
        CurveGeometry::Line { start, end } => (*start, *end),
        CurveGeometry::Circle { start, end, .. } => (*start, *end),
        CurveGeometry::Other { start, end, .. } => (*start, *end),
    };
    match point {
        Some(PickPoint::Start) => return Ok(Seen::Point(project(start))),
        Some(PickPoint::End) => return Ok(Seen::Point(project(end))),
        Some(PickPoint::Center) => {
            return match curve {
                CurveGeometry::Circle { center, .. } => Ok(Seen::Point(project(*center))),
                CurveGeometry::Line { start, end } => {
                    Ok(Seen::Point(mid(project(*start), project(*end))))
                }
                CurveGeometry::Other { kind, .. } => {
                    Err(format!("a {kind} edge has no center to pick"))
                }
            };
        }
        None => {}
    }
    match curve {
        CurveGeometry::Circle {
            center,
            normal,
            radius,
            start,
            end,
        } => {
            let n = mitcad_vec3::unit(*normal).unwrap_or([0.0, 0.0, 1.0]);
            let facing = mitcad_vec3::dot(n, eye);
            let end_on = facing.abs() > 1.0 - 1e-6;
            // The diameter square to both the axis and the eye is seen at
            // full length.
            let across = if end_on {
                [1.0, 0.0]
            } else {
                let d = mitcad_vec3::cross(n, eye);
                let o = project([0.0; 3]);
                unit2(sub2(project(d), o))
            };
            let c = project(*center);
            let sweep = arc_sweep(*center, n, *radius, *start, *end);
            let arc = if sweep >= TAU - 1e-9 {
                None
            } else if end_on {
                // Counter-clockwise in the view when its normal faces the
                // eye, else from its end to its start.
                let angle = |p: Vec3| {
                    let d = sub2(project(p), c);
                    d[1].atan2(d[0])
                };
                let from = if facing > 0.0 {
                    angle(*start)
                } else {
                    angle(*end)
                };
                Some((from, from + sweep))
            } else {
                let reach: Vec<f64> = arc_points3(*center, n, *radius, *start, *end)
                    .into_iter()
                    .map(|p| dot2(sub2(project(p), c), across))
                    .collect();
                Some((
                    reach.iter().copied().fold(f64::INFINITY, f64::min),
                    reach.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ))
            };
            Ok(Seen::Circle {
                center: c,
                radius: *radius,
                end_on,
                across,
                arc,
            })
        }
        _ => {
            let (a, b) = (project(start), project(end));
            if len2(sub2(b, a)) < 1e-9 {
                Ok(Seen::Point(a))
            } else {
                Ok(Seen::Line(a, b))
            }
        }
    }
}

// --- Words ---------------------------------------------------------------

/// A dimension's value in words as #702's engine writes it: the main text
/// (own text before, prefix, value, a ± tolerance or a fit code), a stack
/// of `upper` over `lower`, and text `after`; `text` all on one line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ValueText {
    pub text: String,
    pub main: String,
    pub upper: Option<String>,
    pub lower: Option<String>,
    pub brackets: bool,
    pub after: String,
}

/// The size of a stacked tolerance's text against the value's.
const STACK: f64 = 0.7;

impl ValueText {
    fn parts(&self, h: f64) -> Vec<(f64, f64, String, f64)> {
        // (along, up, text, height) from the start of the baseline.
        let mut out = Vec::new();
        let mut x = 0.0;
        if !self.main.is_empty() {
            out.push((x, 0.0, self.main.clone(), h));
            x += text_width(&self.main, h) + 0.6;
        }
        if let (Some(upper), Some(lower)) = (&self.upper, &self.lower) {
            let small = h * STACK;
            let (open, close) = if self.brackets { ("(", ")") } else { ("", "") };
            if !open.is_empty() {
                out.push((x, 0.0, open.to_owned(), h));
                x += text_width(open, h);
            }
            out.push((x, h - small, upper.clone(), small));
            out.push((x, -0.25 * small, lower.clone(), small));
            x += text_width(upper, small).max(text_width(lower, small)) + 0.3;
            if !close.is_empty() {
                out.push((x, 0.0, close.to_owned(), h));
                x += text_width(close, h);
            }
            x += 0.3;
        }
        if !self.after.is_empty() {
            out.push((x, 0.0, self.after.clone(), h));
        }
        out
    }

    /// Its width at this height, mm.
    pub fn width(&self, h: f64) -> f64 {
        self.parts(h)
            .iter()
            .map(|(x, _, t, size)| x + text_width(t, *size))
            .fold(0.0, f64::max)
    }

    /// Writes it on a baseline, its letters `h` high: `at` its start
    /// (`Anchor::Start`), its middle or its end, along `angle`.
    fn write(&self, group: &mut Group, at: P2, angle: f64, anchor: Anchor, h: f64) {
        let along = [angle.cos(), angle.sin()];
        let up = [-angle.sin(), angle.cos()];
        let shift = match anchor {
            Anchor::Start => 0.0,
            Anchor::Middle => self.width(h) / 2.0,
            Anchor::End => self.width(h),
        };
        let start = sub2(at, mul2(along, shift));
        for (x, y, text, size) in self.parts(h) {
            group.text(
                add2(add2(start, mul2(along, x)), mul2(up, y)),
                size,
                angle,
                Anchor::Start,
                text,
            );
        }
    }
}

// --- On the sheet ----------------------------------------------------------

/// What lays an annotation out: its view's map onto the sheet, the view's
/// box on the sheet and the letters' height (paper mm: the annotation's
/// own, else its drawing's; mitcad#729).
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    pub map: &'a ViewMap,
    pub view_box: Bounds,
    pub text: f64,
}

impl Frame<'_> {
    fn at(&self, p: P2) -> P2 {
        self.map.at(p)
    }
}

/// An annotation laid out: its group and its box on the sheet.
#[derive(Debug, Clone)]
pub struct Laid {
    pub group: Group,
    pub bounds: Bounds,
}

/// The value a dimension measures in its view: a length (view mm, the
/// model's) or an angle (radians); None for what is not a dimension.
pub fn measure(annotation: &ViewAnnotation, seen: &[Seen]) -> Result<Option<f64>, String> {
    match annotation.kind {
        AnnotationType::Linear => {
            let (_, d) = linear_axes(annotation.along, annotation.turn, seen)?;
            Ok(Some(dot2(sub2(seen[1].point(), seen[0].point()), d).abs()))
        }
        AnnotationType::Ordinate => {
            let (d, _) = ordinate_axes(annotation);
            Ok(Some(dot2(sub2(seen[1].point(), seen[0].point()), d)))
        }
        AnnotationType::Angle => {
            let (_, r1, r2) = corner(seen)?;
            Ok(Some(dot2(r1, r2).clamp(-1.0, 1.0).acos()))
        }
        AnnotationType::Radius | AnnotationType::Diameter => {
            let r = round_radius(&seen[0])?;
            Ok(Some(if annotation.kind == AnnotationType::Diameter {
                2.0 * r
            } else {
                r
            }))
        }
        _ => Ok(None),
    }
}

fn round_radius(seen: &Seen) -> Result<f64, String> {
    match *seen {
        Seen::Circle { radius, .. } | Seen::Axis { radius, .. } => Ok(radius),
        _ => Err("the pick is not a circle or a round face".to_owned()),
    }
}

/// The direction a linear dimension of these picks measures along when
/// none is given: the view's x or y, whichever its picks lie further apart
/// along; a straight edge with a point or a parallel edge, square to the
/// edge when aligned.
pub fn default_along(seen: &[Seen]) -> Along {
    let d = sub2(seen[1].point(), seen[0].point());
    if d[0].abs() >= d[1].abs() {
        Along::Horizontal
    } else {
        Along::Vertical
    }
}

/// How far apart two points' coordinates may be and still count as level
/// or plumb, view mm.
const LEVEL: f64 = 1e-6;

/// The direction a linear dimension between two points (view) measures
/// along when its placing click (view) chooses it (mitcad#1272), as a
/// sketch's dimensions do: placed above or below them (between them
/// along x, beyond them along y) horizontal, beside them vertical, else
/// (between them both ways, or beyond them both ways) aligned. Two points
/// level or plumb measure along that axis wherever it is placed; without
/// a click, [`default_along`].
pub fn placed_along(a: P2, b: P2, at: Option<P2>) -> Along {
    let level = (a[1] - b[1]).abs() <= LEVEL;
    let plumb = (a[0] - b[0]).abs() <= LEVEL;
    if level || plumb {
        return if plumb && !level {
            Along::Vertical
        } else {
            Along::Horizontal
        };
    }
    let Some(c) = at else {
        return default_along(&[Seen::Point(a), Seen::Point(b)]);
    };
    let between = |k: usize| c[k] > a[k].min(b[k]) && c[k] < a[k].max(b[k]);
    match (between(0), between(1)) {
        (true, false) => Along::Horizontal,
        (false, true) => Along::Vertical,
        _ => Along::Aligned,
    }
}

/// A direction of the view turned `turn` degrees counter-clockwise (a
/// dimension's that turned with its view, mitcad#729).
pub fn turned(d: P2, turn: Option<f64>) -> P2 {
    match turn {
        Some(t) if t != 0.0 => {
            let (s, c) = t.to_radians().sin_cos();
            [c * d[0] - s * d[1], s * d[0] + c * d[1]]
        }
        _ => d,
    }
}

/// The direction (unit, view) an ordinate dimension measures along and
/// the one its leader runs along: the view's x and y (vertical: y and x),
/// as turned with its view.
pub(crate) fn ordinate_axes(annotation: &ViewAnnotation) -> (P2, P2) {
    let (d, n) = match annotation.along {
        Some(Along::Vertical) => ([0.0, 1.0], [1.0, 0.0]),
        _ => ([1.0, 0.0], [0.0, 1.0]),
    };
    (turned(d, annotation.turn), turned(n, annotation.turn))
}

/// The direction (unit, view) a linear dimension measures along, and
/// whether its picks are parallel lines; a horizontal or vertical one's
/// as turned with its view (`turn`, degrees).
pub fn linear_axes(
    along: Option<Along>,
    turn: Option<f64>,
    seen: &[Seen],
) -> Result<(bool, P2), String> {
    let line_of = |s: &Seen| match *s {
        Seen::Line(a, b) => Some(unit2(sub2(b, a))),
        _ => None,
    };
    match along {
        Some(Along::Horizontal) | None => Ok((false, turned([1.0, 0.0], turn))),
        Some(Along::Vertical) => Ok((false, turned([0.0, 1.0], turn))),
        Some(Along::Aligned) => {
            let lines: Vec<P2> = seen.iter().filter_map(line_of).collect();
            if let Some(first) = lines.first() {
                // Square to the edge: the distance from it.
                return Ok((lines.len() == 2, perp(*first)));
            }
            vec2::unit(sub2(seen[1].point(), seen[0].point()))
                .map(|d| (false, d))
                .ok_or_else(|| "the picks lie on one point".to_owned())
        }
    }
}

/// The corner of two straight edges and the unit rays from it to their
/// far ends.
pub(crate) fn corner(seen: &[Seen]) -> Result<(P2, P2, P2), String> {
    let (Seen::Line(a1, b1), Seen::Line(a2, b2)) = (seen[0], seen[1]) else {
        return Err("an angle is between two straight edges".to_owned());
    };
    let d1 = sub2(b1, a1);
    let d2 = sub2(b2, a2);
    let det = vec2::cross(d1, d2);
    if det.abs() < 1e-9 * len2(d1).max(1e-9) * len2(d2).max(1e-9) {
        return Err("the edges are parallel: they make no angle".to_owned());
    }
    let t = vec2::cross(sub2(a2, a1), d2) / det;
    let c = add2(a1, mul2(d1, t));
    let far = |a: P2, b: P2| {
        if len2(sub2(a, c)) > len2(sub2(b, c)) {
            a
        } else {
            b
        }
    };
    let r1 = unit2(sub2(far(a1, b1), c));
    let r2 = unit2(sub2(far(a2, b2), c));
    Ok((c, r1, r2))
}

/// Where a dimension's text goes with its value: what the caller worked
/// out with #702's engine.
pub fn lay_out(
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    value: Option<&ValueText>,
    frame_cells: &[String],
) -> Result<Laid, String> {
    let h = frame.text;
    let mut g = Group::new(match annotation.kind {
        AnnotationType::CenterMark | AnnotationType::Centerline | AnnotationType::CenterPattern => {
            "center-mark"
        }
        AnnotationType::EdgeExtension => "edge-extension",
        AnnotationType::Note => "note",
        AnnotationType::Datum => "datum",
        AnnotationType::Frame => "feature-control-frame",
        AnnotationType::Finish => "surface-finish",
        AnnotationType::Balloon => "balloon",
        AnnotationType::HoleTable => "hole-table",
        AnnotationType::PartsList => "parts-list",
        _ => "dimension",
    });
    g.id = Some(format!("A{}", annotation.id));
    let words = || value.cloned().unwrap_or_default();
    match annotation.kind {
        AnnotationType::Linear => {
            let (_, d) = linear_axes(annotation.along, annotation.turn, seen)?;
            linear(&mut g, annotation, seen, d, frame, &words());
        }
        AnnotationType::Ordinate => ordinate(&mut g, annotation, seen, frame, &words()),
        AnnotationType::Angle => angle(&mut g, annotation, seen, frame, &words())?,
        AnnotationType::Radius | AnnotationType::Diameter => {
            round(&mut g, annotation, &seen[0], frame, &words())?
        }
        AnnotationType::CenterMark => match seen[0] {
            Seen::Circle {
                center,
                radius,
                end_on: true,
                ..
            } => center_mark(&mut g, frame.at(center), radius * frame.map.scale),
            Seen::Axis { a, b, .. } => centerline(&mut g, frame.at(a), frame.at(b), 0.0, 0.0),
            _ => {
                return Err(
                    "a center mark is of a circle seen end on, or of a round face seen from \
                     the side"
                        .to_owned(),
                );
            }
        },
        AnnotationType::Centerline => centerline_of(&mut g, seen, frame)?,
        AnnotationType::CenterPattern => center_pattern(&mut g, seen, frame)?,
        AnnotationType::EdgeExtension => edge_extension(&mut g, annotation, seen, frame)?,
        AnnotationType::Note => {
            let tip = leader_tip(annotation, &seen[0], frame);
            let shift = annotation.text_offset.unwrap_or(NOTE_SHIFT);
            let lines: Vec<String> = annotation
                .text
                .as_deref()
                .unwrap_or("")
                .lines()
                .map(|l| l.trim_end().to_owned())
                .collect();
            leader_text(&mut g, tip, add2(tip, shift), &lines, h);
        }
        AnnotationType::Datum => {
            let tip = leader_tip(annotation, &seen[0], frame);
            let shift = annotation.text_offset.unwrap_or(DATUM_SHIFT);
            // On the edge's line beyond its ends: its own extension line
            // (mitcad#1296).
            if let Some((from, to)) = datum_extension(annotation, &seen[0], frame) {
                g.line(Pen::Thin, from, to);
            }
            datum(
                &mut g,
                tip,
                leader_along(annotation, &seen[0]),
                add2(tip, shift),
                annotation.letter.as_deref().unwrap_or(""),
                h,
                annotation.terminator(),
            );
        }
        AnnotationType::Frame => {
            let tip = leader_tip(annotation, &seen[0], frame);
            let shift = annotation.text_offset.unwrap_or(NOTE_SHIFT);
            let characteristic = annotation
                .frame
                .as_ref()
                .map_or(Characteristic::Flatness, |f| f.characteristic);
            control_frame(
                &mut g,
                tip,
                add2(tip, shift),
                characteristic,
                frame_cells,
                h,
            );
        }
        AnnotationType::Finish => {
            let tip = leader_tip(annotation, &seen[0], frame);
            let shift = annotation.text_offset.unwrap_or([0.0, 0.0]);
            let finish = annotation.finish.clone().unwrap_or_default();
            surface_finish(&mut g, tip, add2(tip, shift), &finish, h);
        }
        AnnotationType::Balloon => {
            // Its number in `value` (its row's in the parts list).
            let tip = leader_tip(annotation, &seen[0], frame);
            let shift = annotation.text_offset.unwrap_or(BALLOON_SHIFT);
            balloon(&mut g, tip, add2(tip, shift), &words().main, h);
        }
        AnnotationType::HoleTable | AnnotationType::PartsList => {
            return Err("a table is laid out with its rows (tables::lay_out_table)".to_owned());
        }
    }
    let bounds = group_bounds(&g);
    Ok(Laid { group: g, bounds })
}

/// A balloon: a circle with the number (`h` high) at `at` and a leader
/// from its rim to an arrow at `tip`.
fn balloon(g: &mut Group, tip: P2, at: P2, number: &str, h: f64) {
    let r = (BALLOON_RADIUS * h / DIMENSION_TEXT).max(text_width(number, h) / 2.0 + 1.2);
    g.items.push(Item::Arc {
        pen: Pen::Thin,
        center: at,
        radius: r,
        start: 0.0,
        end: TAU,
    });
    g.text([at[0], at[1] - h / 2.0], h, 0.0, Anchor::Middle, number);
    let towards = sub2(tip, at);
    if len2(towards) > r + 0.5 {
        let from = add2(at, mul2(unit2(towards), r));
        g.line(Pen::Thin, from, tip);
        arrow(g, tip, unit2(towards));
    }
}

/// A text note on the sheet; `text`: its letters' height without its own
/// (the drawing's, mitcad#729).
pub fn lay_out_note(note: &SheetNote, text: f64) -> Laid {
    let mut g = Group::new("note");
    g.id = Some(format!("A{}", note.id));
    let h = note.height.unwrap_or(text);
    for (i, line) in note.text.lines().enumerate() {
        g.text(
            [note.at[0], note.at[1] - i as f64 * h * 1.6],
            h,
            0.0,
            Anchor::Start,
            line.trim_end(),
        );
    }
    let bounds = group_bounds(&g);
    Laid { group: g, bounds }
}

/// Where the dimension line of a linear dimension lies: across it from the
/// middle of its picks (sheet), paper mm. When placed for itself, beyond
/// the view's box on the side of the picks.
fn linear_offset(annotation: &ViewAnnotation, middle: P2, n: P2, view_box: &Bounds) -> f64 {
    if let Some(offset) = annotation.line_offset {
        return offset;
    }
    if view_box.is_empty() {
        return LINE_GAP;
    }
    let corners = [
        view_box.min,
        [view_box.max[0], view_box.min[1]],
        view_box.max,
        [view_box.min[0], view_box.max[1]],
    ];
    let across: Vec<f64> = corners.iter().map(|c| dot2(sub2(*c, middle), n)).collect();
    let high = across.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let low = across.iter().copied().fold(f64::INFINITY, f64::min);
    // On the side of the box the picks are nearer to.
    if dot2(sub2(view_box.center(), middle), n) <= 0.0 {
        high + LINE_GAP
    } else {
        low - LINE_GAP
    }
}

/// Where a linear dimension's line lies on the sheet: its distance across
/// the line (along the line's normal) from the sheet's origin, and its
/// offset from the middle of its picks, as it is drawn. None when its
/// picks give it no direction.
pub fn line_position(
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
) -> Option<(f64, f64)> {
    let (_, d) = linear_axes(annotation.along, annotation.turn, seen).ok()?;
    let n = perp(d);
    let middle = mid(frame.at(seen[0].point()), frame.at(seen[1].point()));
    let c = linear_offset(annotation, middle, n, &frame.view_box);
    Some((dot2(middle, n) + c, c))
}

/// The offset from the middle of its picks that puts a linear
/// dimension's line at `position` across the sheet (as
/// [`line_position`] gives it): the dimensions of a series in a row or
/// stacked, whatever their picks.
pub fn offset_for(
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    position: f64,
) -> Option<f64> {
    let (_, d) = linear_axes(annotation.along, annotation.turn, seen).ok()?;
    let n = perp(d);
    let middle = mid(frame.at(seen[0].point()), frame.at(seen[1].point()));
    Some(position - dot2(middle, n))
}

/// The point of a pick an extension line starts from: a straight edge's
/// end nearer the dimension line (sheet), else the pick's point.
fn anchor_of(seen: &Seen, frame: &Frame<'_>, toward: impl Fn(P2) -> f64) -> P2 {
    match *seen {
        Seen::Line(a, b) => {
            let (sa, sb) = (frame.at(a), frame.at(b));
            if toward(sa) >= toward(sb) { sa } else { sb }
        }
        _ => frame.at(seen.point()),
    }
}

/// A linear dimension between two picks along `d` (unit, the view's and
/// the sheet's axes agree).
fn linear(
    g: &mut Group,
    annotation: &ViewAnnotation,
    seen: &[Seen],
    d: P2,
    frame: &Frame<'_>,
    words: &ValueText,
) {
    let n = perp(d);
    let (pa, pb) = (frame.at(seen[0].point()), frame.at(seen[1].point()));
    let middle = mid(pa, pb);
    let c = linear_offset(annotation, middle, n, &frame.view_box);
    let side = if c >= 0.0 { 1.0 } else { -1.0 };
    let toward = |p: P2| side * dot2(sub2(p, middle), n);
    let a = anchor_of(&seen[0], frame, toward);
    let b = anchor_of(&seen[1], frame, toward);
    let foot = |p: P2| add2(p, mul2(n, c - dot2(sub2(p, middle), n)));
    dimension_line(g, a, b, foot(a), foot(b), n, annotation, words, frame.text);
}

/// Extension lines from `a` and `b` to the feet `fa` and `fb` on the
/// dimension line, the line with its arrows and the text over its middle
/// (moved along it by the text offset; the line runs on under a text
/// moved past an end), its letters `h` high.
#[allow(clippy::too_many_arguments)]
fn dimension_line(
    g: &mut Group,
    a: P2,
    b: P2,
    fa: P2,
    fb: P2,
    n: P2,
    annotation: &ViewAnnotation,
    words: &ValueText,
    h: f64,
) {
    for (from, foot) in [(a, fa), (b, fb)] {
        let reach = len2(sub2(foot, from));
        if reach > EXTENSION_GAP {
            let out = unit2(sub2(foot, from));
            g.line(
                Pen::Thin,
                add2(from, mul2(out, EXTENSION_GAP)),
                add2(foot, mul2(out, EXTENSION_BEYOND)),
            );
        }
    }
    let length = len2(sub2(fb, fa));
    let towards = if length > 1e-9 {
        unit2(sub2(fb, fa))
    } else {
        perp(n).map(|v| -v)
    };
    g.line(Pen::Thin, fa, fb);
    if length >= 2.0 * ARROW_LENGTH + 1.0 {
        arrow(g, fa, mul2(towards, -1.0));
        arrow(g, fb, towards);
    } else {
        // Too short: the arrows outside, pointing in.
        g.line(Pen::Thin, fa, sub2(fa, mul2(towards, ARROW_LENGTH + 2.0)));
        g.line(Pen::Thin, fb, add2(fb, mul2(towards, ARROW_LENGTH + 2.0)));
        arrow(g, fa, towards);
        arrow(g, fb, mul2(towards, -1.0));
    }
    // The text moves along the way it reads (as a drag and a place keep
    // it), whichever way the picks run.
    let [u, v] = annotation.text_offset.unwrap_or([0.0, 0.0]);
    let along = upright_along(towards);
    let middle = add2(mid(fa, fb), mul2(along, u));
    if u.abs() > length / 2.0 {
        let fb_ahead = dot2(sub2(fb, fa), along) >= 0.0;
        let end = if fb_ahead == (u > 0.0) { fb } else { fa };
        g.line(Pen::Thin, end, middle);
    }
    let angle = upright(towards);
    let up = [-angle.sin(), angle.cos()];
    let at = add2(middle, mul2(up, 1.0 + v));
    words.write(g, at, angle, Anchor::Middle, h);
}

/// An ordinate dimension: a thin line from the point across to the level
/// of its origin's dimension line, the value along it there.
fn ordinate(
    g: &mut Group,
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    words: &ValueText,
) {
    // Measured along d, the leader runs along n.
    let (_, n) = ordinate_axes(annotation);
    let origin = frame.at(seen[0].point());
    let p = frame.at(seen[1].point());
    let level = match annotation.line_offset {
        Some(offset) => offset,
        None => {
            let b = &frame.view_box;
            if b.is_empty() {
                LINE_GAP
            } else {
                let top = [b.max[0], b.max[1]];
                dot2(sub2(top, origin), n) + LINE_GAP
            }
        }
    };
    let across = level - dot2(sub2(p, origin), n);
    let end = add2(p, mul2(n, across));
    let out = if across >= 0.0 { n } else { mul2(n, -1.0) };
    if across.abs() > EXTENSION_GAP {
        g.line(Pen::Thin, add2(p, mul2(out, EXTENSION_GAP)), end);
    }
    let shift = annotation.text_offset.unwrap_or([0.0, 0.0]);
    let at = add2(add2(end, mul2(out, 1.0)), mul2(perp(n), -shift[0]));
    let angle = upright(out);
    let anchor = if dot2([angle.cos(), angle.sin()], out) > 0.0 {
        Anchor::Start
    } else {
        Anchor::End
    };
    // The text's baseline beside the line, its middle on it.
    let up = [-angle.sin(), angle.cos()];
    let h = frame.text;
    words.write(g, sub2(at, mul2(up, h / 2.0)), angle, anchor, h);
}

/// How far a straight edge's ends are from a corner on the sheet: the
/// nearer and the farther.
fn arm(seen: &Seen, center: P2, frame: &Frame<'_>) -> (f64, f64) {
    match *seen {
        Seen::Line(a, b) => {
            let (da, db) = (
                len2(sub2(frame.at(a), center)),
                len2(sub2(frame.at(b), center)),
            );
            (da.min(db), da.max(db))
        }
        _ => (0.0, 0.0),
    }
}

/// An angle's arc radius on the sheet: its own, else three fifths of the
/// shorter edge's reach from the corner, 8 mm at least.
fn arc_radius(annotation: &ViewAnnotation, seen: &[Seen], frame: &Frame<'_>) -> f64 {
    if let Some(radius) = annotation.line_offset {
        return radius;
    }
    let Ok((c, _, _)) = corner(seen) else {
        return 8.0;
    };
    let center = frame.at(c);
    let (_, f1) = arm(&seen[0], center, frame);
    let (_, f2) = arm(&seen[1], center, frame);
    (0.6 * f1.min(f2)).max(8.0)
}

/// An angle between two straight edges: an arc about their corner with an
/// arrow at each end, extension lines along the edges to it where it lies
/// beyond them, the value on the arc's middle.
fn angle(
    g: &mut Group,
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    words: &ValueText,
) -> Result<(), String> {
    let (c, r1, r2) = corner(seen)?;
    let center = frame.at(c);
    let (n1, f1) = arm(&seen[0], center, frame);
    let (n2, f2) = arm(&seen[1], center, frame);
    let radius = arc_radius(annotation, seen, frame);
    let (t1, t2) = (r1[1].atan2(r1[0]), r2[1].atan2(r2[0]));
    let sweep = dot2(r1, r2).clamp(-1.0, 1.0).acos();
    let (start, from, to) = if vec2::cross(r1, r2) >= 0.0 {
        (t1, r1, r2)
    } else {
        (t2, r2, r1)
    };
    g.items.push(Item::Arc {
        pen: Pen::Thin,
        center,
        radius,
        start,
        end: start + sweep,
    });
    // The rays' extension lines where the arc lies beyond an edge, or
    // short of it.
    for (ray, near, far) in [(r1, n1, f1), (r2, n2, f2)] {
        if radius > far + EXTENSION_GAP {
            g.line(
                Pen::Thin,
                add2(center, mul2(ray, far + EXTENSION_GAP)),
                add2(center, mul2(ray, radius + EXTENSION_BEYOND)),
            );
        } else if radius < near - EXTENSION_GAP {
            g.line(
                Pen::Thin,
                add2(center, mul2(ray, radius - EXTENSION_BEYOND)),
                add2(center, mul2(ray, near - EXTENSION_GAP)),
            );
        }
    }
    let tangent = |r: P2| perp(r);
    arrow(
        g,
        add2(center, mul2(from, radius)),
        mul2(tangent(from), -1.0),
    );
    arrow(g, add2(center, mul2(to, radius)), tangent(to));
    let half = start + sweep / 2.0;
    let middle = [half.cos(), half.sin()];
    let [u, v] = annotation.text_offset.unwrap_or([0.0, 0.0]);
    let along = tangent(middle);
    let at = add2(add2(center, mul2(middle, radius + 1.0 + v)), mul2(along, u));
    let angle = upright(along);
    words.write(g, at, angle, Anchor::Middle, frame.text);
    Ok(())
}

/// A radius or a diameter: of a circle seen end on, a line from its
/// centre (across it for a diameter) to the rim with an arrow, run on to
/// the text on a shoulder outside; of a circle or round face seen from the
/// side, a linear dimension across it.
fn round(
    g: &mut Group,
    annotation: &ViewAnnotation,
    seen: &Seen,
    frame: &Frame<'_>,
    words: &ValueText,
) -> Result<(), String> {
    let diameter = annotation.kind == AnnotationType::Diameter;
    let scale = frame.map.scale;
    let (center, radius, across, axis) = match *seen {
        Seen::Circle {
            center,
            radius,
            end_on: true,
            arc,
            ..
        } => {
            let c = frame.at(center);
            let r = radius * scale;
            let shift = annotation.text_offset.unwrap_or_else(|| {
                let out = r + 10.0;
                match arc {
                    // Out from the middle of an arc.
                    Some((start, end)) => {
                        let half = (start + end) / 2.0;
                        [out * half.cos(), out * half.sin()]
                    }
                    None => [out * FRAC_1_SQRT_2, out * FRAC_1_SQRT_2],
                }
            });
            let text_at = add2(c, shift);
            let mut u = vec2::unit(shift).unwrap_or([1.0, 0.0]);
            // The arrow on the arc where it is: its nearer end when the
            // text lies beyond it.
            if let Some((start, end)) = arc
                && !in_sweep(u[1].atan2(u[0]), start, end)
            {
                let angle = u[1].atan2(u[0]);
                let off = |a: f64| {
                    let d = (angle - a).rem_euclid(TAU);
                    d.min(TAU - d)
                };
                let a = if off(start) <= off(end) { start } else { end };
                u = [a.cos(), a.sin()];
            }
            let rim = add2(c, mul2(u, r));
            if diameter {
                let opposite = sub2(c, mul2(u, r));
                g.line(Pen::Thin, opposite, rim);
                arrow(g, opposite, mul2(u, -1.0));
            } else {
                g.line(Pen::Thin, c, rim);
            }
            arrow(g, rim, u);
            let side = if shift[0] >= 0.0 { 1.0 } else { -1.0 };
            let width = words.width(frame.text) + 1.0;
            if len2(shift) > r {
                g.line(Pen::Thin, rim, text_at);
                g.line(Pen::Thin, text_at, add2(text_at, [side * width, 0.0]));
            }
            let (x, anchor) = if side > 0.0 {
                (text_at[0] + 0.5, Anchor::Start)
            } else {
                (text_at[0] - 0.5, Anchor::End)
            };
            words.write(g, [x, text_at[1] + 1.0], 0.0, anchor, frame.text);
            return Ok(());
        }
        Seen::Circle {
            center,
            radius,
            across,
            ..
        } => (center, radius, across, None),
        Seen::Axis { a, b, radius } => (mid(a, b), radius, perp(unit2(sub2(b, a))), Some((a, b))),
        _ => return Err("the pick is not a circle or a round face".to_owned()),
    };
    // Seen from the side: across the circle or face, along `across`.
    let d = across;
    let n = perp(d);
    let from = if diameter {
        sub2(center, mul2(d, radius))
    } else {
        center
    };
    let to = add2(center, mul2(d, radius));
    let (a, b) = (frame.at(from), frame.at(to));
    let middle = mid(a, b);
    let c = match annotation.line_offset {
        Some(offset) => offset,
        None => match axis {
            // Beyond the face's end on the side its axis runs to.
            Some((p, q)) => {
                let (sp, sq) = (frame.at(p), frame.at(q));
                let reach = dot2(sub2(sp, middle), n).max(dot2(sub2(sq, middle), n));
                reach + LINE_GAP
            }
            None => linear_offset(annotation, middle, n, &frame.view_box),
        },
    };
    let foot = |p: P2| add2(p, mul2(n, c - dot2(sub2(p, middle), n)));
    // The extension lines start at the face's end nearer the line.
    let start = |p: P2| match axis {
        Some((s, e)) => {
            let (ss, se) = (frame.at(s), frame.at(e));
            let end =
                if (dot2(sub2(ss, middle), n) - c).abs() <= (dot2(sub2(se, middle), n) - c).abs() {
                    ss
                } else {
                    se
                };
            add2(
                p,
                mul2(n, dot2(sub2(end, middle), n) - dot2(sub2(p, middle), n)),
            )
        }
        None => p,
    };
    let (sa, sb) = (start(a), start(b));
    dimension_line(
        g,
        sa,
        sb,
        foot(a),
        foot(b),
        n,
        annotation,
        words,
        frame.text,
    );
    Ok(())
}

/// Crossed centre lines through a circle's centre (sheet), reaching
/// beyond its rim: long-dash dotted for a larger circle, thin for a small
/// one, each from the centre out so that they cross on a dash.
pub fn center_mark(g: &mut Group, center: P2, radius: f64) {
    let pen = if radius >= 4.0 { Pen::Chain } else { Pen::Thin };
    let reach = radius + CENTER_BEYOND;
    for d in [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]] {
        g.line(pen, center, add2(center, mul2(d, reach)));
    }
}

/// A centreline from `a` to `b` (sheet), run on beyond each end by the
/// circle's radius there (`ra`, `rb`) and a little more.
pub fn centerline(g: &mut Group, a: P2, b: P2, ra: f64, rb: f64) {
    let d = unit2(sub2(b, a));
    let from = sub2(a, mul2(d, ra + CENTER_BEYOND));
    let to = add2(b, mul2(d, rb + CENTER_BEYOND));
    // From the middle out: the lines cross the middle on a dash.
    let middle = mid(from, to);
    g.line(Pen::Chain, middle, from);
    g.line(Pen::Chain, middle, to);
}

fn centerline_of(g: &mut Group, seen: &[Seen], frame: &Frame<'_>) -> Result<(), String> {
    let scale = frame.map.scale;
    let rim = |s: &Seen| match *s {
        Seen::Circle {
            radius,
            end_on: true,
            ..
        } => radius * scale,
        _ => 0.0,
    };
    match seen {
        [Seen::Axis { a, b, .. }] => {
            centerline(g, frame.at(*a), frame.at(*b), 0.0, 0.0);
            Ok(())
        }
        [one] => Err(format!(
            "a centerline of one pick is a round face's axis, not a {}",
            match one {
                Seen::Point(_) => "point",
                Seen::Line(..) => "straight edge",
                _ => "circle",
            }
        )),
        [Seen::Line(a1, b1), Seen::Line(a2, b2)] => {
            // Halfway between two edges, over both.
            let d = unit2(sub2(*b1, *a1));
            let d2 = unit2(sub2(*b2, *a2));
            if vec2::cross(d, d2).abs() > 1e-6 {
                return Err("a centerline between edges needs parallel edges".to_owned());
            }
            let o = mid(mid(*a1, *b1), mid(*a2, *b2));
            let ts: Vec<f64> = [a1, b1, a2, b2]
                .iter()
                .map(|p| dot2(sub2(**p, o), d))
                .collect();
            let low = ts.iter().copied().fold(f64::INFINITY, f64::min);
            let high = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            centerline(
                g,
                frame.at(add2(o, mul2(d, low))),
                frame.at(add2(o, mul2(d, high))),
                0.0,
                0.0,
            );
            Ok(())
        }
        [
            Seen::Circle {
                center,
                radius: ra,
                end_on: true,
                arc: arc_a,
                ..
            },
            Seen::Circle {
                center: other,
                radius: rb,
                end_on: true,
                arc: arc_b,
                ..
            },
        ] if len2(sub2(*center, *other)) <= 1e-9 * ra.max(*rb).max(1.0)
            && (ra - rb).abs() > 1e-9
            && (arc_a.is_some() || arc_b.is_some()) =>
        {
            slot_centerline(g, *center, *ra, *rb, [*arc_a, *arc_b], frame);
            Ok(())
        }
        [p, q] => {
            let (a, b) = (frame.at(p.point()), frame.at(q.point()));
            if len2(sub2(b, a)) < 1e-9 {
                return Err(
                    "the picks share their center: two concentric arcs (a curved slot's sides) \
                     make its center arc, whole circles none"
                        .to_owned(),
                );
            }
            centerline(g, a, b, rim(p), rim(q));
            Ok(())
        }
        _ => Err("a centerline takes one or two picks".to_owned()),
    }
}

/// The centre arc of a curved slot (mitcad#1272) from its two concentric
/// sides (radii `ra` and `rb` about `center`, view mm; their ranges of
/// angles, none for a whole circle): halfway between them, so through the
/// centres of the slot's ends, from one end's centre to the other's and
/// run on beyond each by the ends' radius and a little more, as a
/// centreline through two circles is; long-dash dotted. The sides' ranges
/// taken together.
fn slot_centerline(
    g: &mut Group,
    center: P2,
    ra: f64,
    rb: f64,
    arcs: [Option<(f64, f64)>; 2],
    frame: &Frame<'_>,
) {
    let (start, end) = match arcs {
        [Some((s1, e1)), Some((s2, e2))] => {
            // The second range turned to lie about the first one's start.
            let shift = TAU * ((s1 - s2) / TAU).round();
            let (s2, e2) = (s2 + shift, e2 + shift);
            let start = s1.min(s2);
            (start, e1.max(e2).min(start + TAU))
        }
        [Some(range), None] | [None, Some(range)] => range,
        [None, None] => (0.0, TAU),
    };
    // The view's map onto the sheet scales and moves: its angles stay.
    let scale = frame.map.scale;
    let radius = (ra + rb) / 2.0 * scale;
    let ends = (ra - rb).abs() / 2.0 * scale;
    let beyond = if end - start >= TAU - 1e-9 {
        0.0
    } else {
        (ends + CENTER_BEYOND) / radius.max(1e-9)
    };
    let from = start - beyond;
    g.items.push(Item::Arc {
        pen: Pen::Chain,
        center: frame.at(center),
        radius,
        start: from,
        end: (end + beyond).min(from + TAU),
    });
}

/// Where a circle's centre is on the sheet and its radius there: a circle
/// seen end on, or a point (no radius).
fn mark_of(seen: &Seen, frame: &Frame<'_>) -> Option<(P2, f64)> {
    match *seen {
        Seen::Circle {
            center,
            radius,
            end_on: true,
            ..
        } => Some((frame.at(center), radius * frame.map.scale)),
        Seen::Point(p) => Some((frame.at(p), 0.0)),
        _ => None,
    }
}

/// The circle through three points, if they are not on one line.
fn circle_through(a: P2, b: P2, c: P2) -> Option<(P2, f64)> {
    let (ab, ac) = (sub2(b, a), sub2(c, a));
    let d = 2.0 * vec2::cross(ab, ac);
    if d.abs() < 1e-9 * len2(ab).max(1.0) * len2(ac).max(1.0) {
        return None;
    }
    let (b2, c2) = (dot2(ab, ab), dot2(ac, ac));
    let o = [(ac[1] * b2 - ab[1] * c2) / d, (ab[0] * c2 - ac[0] * b2) / d];
    Some((add2(a, o), len2(o)))
}

/// A centre pattern (mitcad#729): on circles about one centre, the circle
/// through their centres (long-dash dotted; an arc over them when they
/// stand on a part of it), a radial line through each, and a small cross
/// at the pattern's centre; on circles in a row, the line through their
/// centres and a short cross line on each; else a centre mark on each.
fn center_pattern(g: &mut Group, seen: &[Seen], frame: &Frame<'_>) -> Result<(), String> {
    let marks: Vec<(P2, f64)> = seen
        .iter()
        .map(|s| mark_of(s, frame))
        .collect::<Option<_>>()
        .ok_or_else(|| "a center pattern is of circles seen end on".to_owned())?;
    let pen = |r: f64| if r >= 4.0 { Pen::Chain } else { Pen::Thin };
    // The two farthest apart, and a third off their line.
    let (mut i, mut j, mut far) = (0, 0, 0.0);
    for (a, (pa, _)) in marks.iter().enumerate() {
        for (b, (pb, _)) in marks.iter().enumerate().skip(a + 1) {
            let d = len2(sub2(*pb, *pa));
            if d > far {
                (i, j, far) = (a, b, d);
            }
        }
    }
    if marks.len() < 2 || far < 1e-9 {
        for (c, r) in &marks {
            center_mark(g, *c, *r);
        }
        return Ok(());
    }
    let tolerance = 0.01_f64.max(1e-6 * far);
    let (a, b) = (marks[i].0, marks[j].0);
    let d = unit2(sub2(b, a));
    let off_line = |p: P2| vec2::cross(d, sub2(p, a)).abs();
    let third = marks
        .iter()
        .map(|(p, _)| *p)
        .max_by(|p, q| off_line(*p).total_cmp(&off_line(*q)))
        .filter(|p| off_line(*p) > tolerance);
    let Some(third) = third else {
        // In a row: the line through them, beyond the outer ones.
        let ts: Vec<f64> = marks.iter().map(|(p, _)| dot2(sub2(*p, a), d)).collect();
        let (lo, hi) = ts.iter().enumerate().fold(
            ((0, f64::INFINITY), (0, f64::NEG_INFINITY)),
            |(lo, hi), (k, t)| {
                (
                    if *t < lo.1 { (k, *t) } else { lo },
                    if *t > hi.1 { (k, *t) } else { hi },
                )
            },
        );
        centerline(
            g,
            marks[lo.0].0,
            marks[hi.0].0,
            marks[lo.0].1,
            marks[hi.0].1,
        );
        let n = perp(d);
        for (c, r) in &marks {
            let reach = r + CENTER_BEYOND;
            g.line(pen(*r), add2(*c, mul2(n, reach)), sub2(*c, mul2(n, reach)));
        }
        return Ok(());
    };
    let Some((o, radius)) = circle_through(a, b, third).filter(|(o, r)| {
        marks
            .iter()
            .all(|(p, _)| (len2(sub2(*p, *o)) - r).abs() <= tolerance)
    }) else {
        for (c, r) in &marks {
            center_mark(g, *c, *r);
        }
        return Ok(());
    };
    // Each hole's angle about the centre; the circle whole unless they
    // leave a gap wider than one and a half of their pitch.
    let mut angles: Vec<f64> = marks
        .iter()
        .map(|(p, _)| {
            let v = sub2(*p, o);
            v[1].atan2(v[0]).rem_euclid(TAU)
        })
        .collect();
    angles.sort_by(f64::total_cmp);
    let n = angles.len();
    let (gap, after) = (0..n)
        .map(|k| {
            let next = if k + 1 < n {
                angles[k + 1]
            } else {
                angles[0] + TAU
            };
            (next - angles[k], k)
        })
        .fold(
            (0.0, 0),
            |best, step| if step.0 > best.0 { step } else { best },
        );
    let widest = marks.iter().map(|(_, r)| *r).fold(0.0, f64::max);
    if gap > 1.5 * TAU / n as f64 {
        // An arc from the hole after the gap round to the one before it,
        // a little beyond each.
        let beyond = (widest + CENTER_BEYOND) / radius.max(1e-9);
        let start = angles[(after + 1) % n] - beyond;
        let end = start + (TAU - gap) + 2.0 * beyond;
        g.items.push(Item::Arc {
            pen: Pen::Chain,
            center: o,
            radius,
            start,
            end: end.min(start + TAU),
        });
    } else {
        g.items.push(Item::Arc {
            pen: Pen::Chain,
            center: o,
            radius,
            start: 0.0,
            end: TAU,
        });
    }
    for (c, r) in &marks {
        let u = unit2(sub2(*c, o));
        let reach = r + CENTER_BEYOND;
        g.line(pen(*r), sub2(*c, mul2(u, reach)), add2(*c, mul2(u, reach)));
    }
    center_mark(g, o, 0.0);
    Ok(())
}

/// An edge extension (mitcad#729): two straight edges run on from their
/// nearer ends to where their lines meet, a little past it so that the
/// lines cross there; one straight edge run on beyond both its ends by
/// `line_offset` (paper mm, [`EXTENSION_LENGTH`] when left out). Each
/// starts a small gap off the edge.
fn edge_extension(
    g: &mut Group,
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
) -> Result<(), String> {
    let lines: Vec<(P2, P2)> = seen
        .iter()
        .map(|s| match *s {
            Seen::Line(a, b) => Some((frame.at(a), frame.at(b))),
            _ => None,
        })
        .collect::<Option<_>>()
        .ok_or_else(|| "an edge extension is of straight edges".to_owned())?;
    match lines.as_slice() {
        [(a, b)] => {
            let d = unit2(sub2(*b, *a));
            let reach = annotation.line_offset.unwrap_or(EXTENSION_LENGTH).max(0.5);
            g.line(
                Pen::Thin,
                sub2(*a, mul2(d, EXTENSION_GAP)),
                sub2(*a, mul2(d, EXTENSION_GAP + reach)),
            );
            g.line(
                Pen::Thin,
                add2(*b, mul2(d, EXTENSION_GAP)),
                add2(*b, mul2(d, EXTENSION_GAP + reach)),
            );
            Ok(())
        }
        [(a1, b1), (a2, b2)] => {
            let (d1, d2) = (sub2(*b1, *a1), sub2(*b2, *a2));
            let det = vec2::cross(d1, d2);
            if det.abs() < 1e-9 * len2(d1).max(1e-9) * len2(d2).max(1e-9) {
                return Err("the edges are parallel: their lines do not meet".to_owned());
            }
            let t = vec2::cross(sub2(*a2, *a1), d2) / det;
            let corner = add2(*a1, mul2(d1, t));
            for (a, b) in [(*a1, *b1), (*a2, *b2)] {
                let d = unit2(sub2(b, a));
                let (ta, tb) = (dot2(sub2(a, corner), d), dot2(sub2(b, corner), d));
                // The corner on the edge itself: nothing to run on.
                if ta.min(tb) <= 0.0 && ta.max(tb) >= 0.0 {
                    continue;
                }
                let near = if ta.abs() <= tb.abs() { a } else { b };
                let out = unit2(sub2(corner, near));
                g.line(
                    Pen::Thin,
                    add2(near, mul2(out, EXTENSION_GAP)),
                    add2(corner, mul2(out, EXTENSION_BEYOND)),
                );
            }
            Ok(())
        }
        _ => Err("an edge extension takes one or two straight edges".to_owned()),
    }
}

/// Where a leader's arrow points (sheet): the pick's point nearest where
/// it was clicked (its point, where it is one).
fn leader_tip(annotation: &ViewAnnotation, seen: &Seen, frame: &Frame<'_>) -> P2 {
    frame.at(leader_point(annotation, seen))
}

/// Where a leader's arrow points in its view (view mm): a datum's or a
/// frame's place along its pick where it has one (mitcad#1295), else the
/// pick's point nearest where it was clicked.
fn leader_point(annotation: &ViewAnnotation, seen: &Seen) -> P2 {
    let near = annotation.at.unwrap_or_else(|| seen.point());
    annotation
        .tip_along
        .and_then(|t| seen.at_fraction(t, near))
        .unwrap_or_else(|| seen.nearest(near))
}

/// Which way the pick runs where a leader's arrow points (unit, on the
/// sheet: a view's map scales without turning); None for a point.
fn leader_along(annotation: &ViewAnnotation, seen: &Seen) -> Option<P2> {
    seen.along(leader_point(annotation, seen))
}

/// A new datum label's or frame's tip kept as its place along its pick
/// (`tip_along`, mitcad#1295), where the pick has one; the others are
/// left as they are.
pub fn keep_tip_along(annotation: &mut ViewAnnotation, seen: &[Seen]) {
    if !matches!(
        annotation.kind,
        AnnotationType::Datum | AnnotationType::Frame
    ) {
        return;
    }
    if let Some(first) = seen.first() {
        let tip = leader_point(annotation, first);
        annotation.tip_along = first.fraction(tip);
    }
}

/// A drag of a datum label's triangle or a frame's arrow by `delta`
/// (sheet, mitcad#1295): the tip slides along its pick to the point of it
/// nearest where the drag takes it, clamped to its ends; the square or the
/// frame keeps its offset from the tip, so it follows. Kept as `at` and
/// `tip_along`. Nothing for the other annotations.
pub fn slid(annotation: &mut ViewAnnotation, seen: &[Seen], frame: &Frame<'_>, delta: P2) {
    if !matches!(
        annotation.kind,
        AnnotationType::Datum | AnnotationType::Frame
    ) {
        return;
    }
    let Some(first) = seen.first() else {
        return;
    };
    let tip = leader_tip(annotation, first, frame);
    let to = frame.map.view_point(add2(tip, delta));
    // A datum on a straight edge runs on along the edge's line past its
    // ends (mitcad#1296); a frame's arrow stays on the edge.
    if let (AnnotationType::Datum, Seen::Line(a, b)) = (annotation.kind, *first) {
        let ab = sub2(b, a);
        let length = dot2(ab, ab);
        if length > 1e-24 {
            let t = dot2(sub2(to, a), ab) / length;
            annotation.at = Some(add2(a, mul2(ab, t)));
            annotation.tip_along = Some(t);
            return;
        }
    }
    let on = first.nearest(to);
    annotation.at = Some(on);
    annotation.tip_along = first.fraction(on);
}

/// A datum label's own extension line (sheet, mitcad#1296) when its tip
/// sits on its straight edge's line beyond the edge's ends: from a little
/// off the nearer end to a little past the tip, as an edge extension
/// draws it; None while the tip is on the edge.
fn datum_extension(
    annotation: &ViewAnnotation,
    seen: &Seen,
    frame: &Frame<'_>,
) -> Option<(P2, P2)> {
    let Seen::Line(a, b) = *seen else {
        return None;
    };
    let t = annotation.tip_along?;
    let end = if t > 1.0 {
        b
    } else if t < 0.0 {
        a
    } else {
        return None;
    };
    let (end, tip) = (frame.at(end), leader_tip(annotation, seen, frame));
    let out = vec2::unit(sub2(tip, end))?;
    (len2(sub2(tip, end)) > EXTENSION_GAP).then(|| {
        (
            add2(end, mul2(out, EXTENSION_GAP)),
            add2(tip, mul2(out, EXTENSION_BEYOND)),
        )
    })
}

/// A leader from `tip` (its arrow there) to text lines on a shoulder at
/// `knee`, the shoulder and the text (`h` high) running away from the tip.
fn leader_text(g: &mut Group, tip: P2, knee: P2, lines: &[String], h: f64) {
    let side = if knee[0] >= tip[0] { 1.0 } else { -1.0 };
    let width = lines.iter().map(|l| text_width(l, h)).fold(0.0, f64::max);
    g.line(Pen::Thin, tip, knee);
    g.line(Pen::Thin, knee, add2(knee, [side * (width + 2.0), 0.0]));
    arrow(g, tip, unit2(sub2(tip, knee)));
    let (x, anchor) = if side > 0.0 {
        (knee[0] + 1.0, Anchor::Start)
    } else {
        (knee[0] - 1.0, Anchor::End)
    };
    for (i, line) in lines.iter().rev().enumerate() {
        let y = knee[1] + 1.0 + i as f64 * h * 1.6;
        g.text([x, y], h, 0.0, anchor, line.clone());
    }
}

/// Half the base of a datum label's triangle and its height, paper mm.
const DATUM_HALF_BASE: f64 = 1.6;
const DATUM_HEIGHT: f64 = 2.8;

/// The radius of a datum label's dot on a face (ISO 5459, mitcad#1296),
/// paper mm.
const DATUM_DOT: f64 = 1.0;

/// A datum label's parts on the sheet (ISO 5459, mitcad#1294).
#[derive(Debug, Clone, PartialEq)]
pub struct DatumLabel {
    /// The filled terminator: a triangle's two base ends, on the feature,
    /// and its apex; or a dot's outline about the tip (mitcad#1296).
    pub mark: Vec<P2>,
    /// Where the leader leaves it: the triangle's apex, the dot's centre.
    pub from: P2,
    /// Where the leader ends: the middle of the square's side nearest
    /// `from` (its bottom, top, left or right).
    pub end: P2,
    /// The square's lower left and upper right corners.
    pub square: (P2, P2),
}

/// Where a datum label's parts go: its triangle's base along the feature
/// at `tip` (`along`: the feature's direction there, unit; none for a
/// point, when the base lies square to the leader), the triangle on the
/// square's side of the feature; the square at `at` (its lower middle)
/// `h` letters high; its leader from the triangle's apex to the side of
/// the square that faces it, so that it never crosses the square. With a
/// dot (`terminator`, mitcad#1296) the dot sits about the tip and the
/// leader leaves its centre.
pub fn datum_label(
    tip: P2,
    along: Option<P2>,
    at: P2,
    h: f64,
    terminator: Terminator,
) -> DatumLabel {
    let half = CELL * h / DIMENSION_TEXT / 2.0;
    let center = [at[0], at[1] + half];
    let towards = sub2(center, tip);
    let (base, up) = match along {
        Some(t) => {
            let n = perp(t);
            (
                t,
                if dot2(towards, n) < 0.0 {
                    mul2(n, -1.0)
                } else {
                    n
                },
            )
        }
        None => {
            let v = unit2(towards);
            (perp(v), v)
        }
    };
    let base = mul2(base, DATUM_HALF_BASE);
    let (mark, from) = match terminator {
        Terminator::Triangle => {
            let apex = add2(tip, mul2(up, DATUM_HEIGHT));
            (vec![add2(tip, base), sub2(tip, base), apex], apex)
        }
        Terminator::Dot => (
            (0..24)
                .map(|i| {
                    let a = TAU * f64::from(i) / 24.0;
                    add2(tip, mul2([a.cos(), a.sin()], DATUM_DOT))
                })
                .collect(),
            tip,
        ),
    };
    let d = sub2(from, center);
    let side = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
    let end = if d[1].abs() >= d[0].abs() {
        [center[0], center[1] + half * side(d[1])]
    } else {
        [center[0] + half * side(d[0]), center[1]]
    };
    DatumLabel {
        mark,
        from,
        end,
        square: (
            [center[0] - half, center[1] - half],
            [center[0] + half, center[1] + half],
        ),
    }
}

/// A datum label: a filled triangle or dot on the feature at `tip`, a
/// leader to a square at `at` (its lower middle) with the letter (`h`
/// high, the square in proportion); as [`datum_label`] places them.
fn datum(
    g: &mut Group,
    tip: P2,
    along: Option<P2>,
    at: P2,
    letter: &str,
    h: f64,
    terminator: Terminator,
) {
    let label = datum_label(tip, along, at, h, terminator);
    g.items.push(Item::Fill {
        points: label.mark.clone(),
    });
    if len2(sub2(label.end, label.from)) > 1e-9 {
        g.line(Pen::Thin, label.from, label.end);
    }
    let (min, max) = label.square;
    g.rect(Pen::Thin, min, max);
    g.text(
        [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0 - h / 2.0],
        h,
        0.0,
        Anchor::Middle,
        letter,
    );
}

/// The widths of a feature control frame's cells (`cells` as
/// [`control_frame`] takes them) with letters `h` high, and their cell's
/// height.
fn frame_widths(cells: &[String], h: f64) -> (Vec<f64>, f64) {
    let cell = CELL * h / DIMENSION_TEXT;
    let widths = cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                cell
            } else {
                (text_width(c, h) + 3.0).max(cell)
            }
        })
        .collect();
    (widths, cell)
}

/// Where a feature control frame's leader leaves it (mitcad#1294): the
/// frame at `at` (its left middle), `total` long and `cell` high, its
/// arrow at `tip`. From the end the tip lies beyond (left or right), else
/// from the middle of the side it lies below or above, so that the
/// leader never crosses the frame.
fn frame_leader_from(tip: P2, at: P2, total: f64, cell: f64) -> P2 {
    let (left, right) = (at[0], at[0] + total);
    let (bottom, top) = (at[1] - cell / 2.0, at[1] + cell / 2.0);
    let middle = at[0] + total / 2.0;
    if tip[0] <= left {
        at
    } else if tip[0] >= right {
        [right, at[1]]
    } else if tip[1] <= bottom {
        [middle, bottom]
    } else if tip[1] >= top {
        [middle, top]
    } else if tip[0] <= middle {
        // Inside the frame: from its nearer end.
        at
    } else {
        [right, at[1]]
    }
}

/// A feature control frame at `at` (its left middle), a leader from it to
/// an arrow at `tip`: the characteristic's symbol drawn, then the other
/// cells (`cells`: the symbol's text first, which is not written; the
/// tolerance; the datums), its letters `h` high and its cells in
/// proportion.
fn control_frame(
    g: &mut Group,
    tip: P2,
    at: P2,
    characteristic: Characteristic,
    cells: &[String],
    h: f64,
) {
    let (widths, cell) = frame_widths(cells, h);
    let total: f64 = widths.iter().sum::<f64>().max(cell);
    let bottom = at[1] - cell / 2.0;
    g.rect(Pen::Thin, [at[0], bottom], [at[0] + total, bottom + cell]);
    let mut x = at[0];
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            g.line(Pen::Thin, [x, bottom], [x, bottom + cell]);
            g.text(
                [x + w / 2.0, at[1] - h / 2.0],
                h,
                0.0,
                Anchor::Middle,
                cells[i].clone(),
            );
        } else {
            symbol(g, characteristic, [x + w / 2.0, at[1]], cell * 0.6);
        }
        x += w;
    }
    // The leader from the frame's side that faces the tip.
    let from = frame_leader_from(tip, at, total, cell);
    g.line(Pen::Thin, from, tip);
    arrow(g, tip, unit2(sub2(tip, from)));
}

/// A geometric characteristic's symbol (ISO 1101) drawn about `c`, `s`
/// mm across: as lines, so that every writer shows it.
fn symbol(g: &mut Group, characteristic: Characteristic, c: P2, s: f64) {
    let h = s / 2.0;
    let p = |x: f64, y: f64| [c[0] + x * h, c[1] + y * h];
    let circle = |g: &mut Group, r: f64| {
        g.items.push(Item::Arc {
            pen: Pen::Thin,
            center: c,
            radius: r * h,
            start: 0.0,
            end: TAU,
        });
    };
    match characteristic {
        Characteristic::Straightness => g.line(Pen::Thin, p(-1.0, 0.0), p(1.0, 0.0)),
        Characteristic::Flatness => g.items.push(Item::Polyline {
            pen: Pen::Thin,
            points: vec![p(-1.0, -0.5), p(0.6, -0.5), p(1.0, 0.5), p(-0.6, 0.5)],
            closed: true,
        }),
        Characteristic::Circularity => circle(g, 0.8),
        Characteristic::Cylindricity => {
            circle(g, 0.6);
            g.line(Pen::Thin, p(-0.9, -0.9), p(-0.2, 0.9));
            g.line(Pen::Thin, p(0.2, -0.9), p(0.9, 0.9));
        }
        Characteristic::LineProfile => g.items.push(Item::Arc {
            pen: Pen::Thin,
            center: p(0.0, -0.4),
            radius: h,
            start: 0.0,
            end: PI,
        }),
        Characteristic::SurfaceProfile => {
            g.items.push(Item::Arc {
                pen: Pen::Thin,
                center: p(0.0, -0.4),
                radius: h,
                start: 0.0,
                end: PI,
            });
            g.line(Pen::Thin, p(-1.0, -0.4), p(1.0, -0.4));
        }
        Characteristic::Parallelism => {
            g.line(Pen::Thin, p(-0.8, -0.9), p(-0.1, 0.9));
            g.line(Pen::Thin, p(0.1, -0.9), p(0.8, 0.9));
        }
        Characteristic::Perpendicularity => {
            g.line(Pen::Thin, p(-0.9, -0.9), p(0.9, -0.9));
            g.line(Pen::Thin, p(0.0, -0.9), p(0.0, 0.9));
        }
        Characteristic::Angularity => {
            g.line(Pen::Thin, p(-0.9, -0.9), p(0.9, -0.9));
            g.line(Pen::Thin, p(-0.9, -0.9), p(0.9, 0.6));
        }
        Characteristic::Position => {
            circle(g, 0.6);
            g.line(Pen::Thin, p(-1.0, 0.0), p(1.0, 0.0));
            g.line(Pen::Thin, p(0.0, -1.0), p(0.0, 1.0));
        }
        Characteristic::Concentricity => {
            circle(g, 0.4);
            circle(g, 0.85);
        }
        Characteristic::Symmetry => {
            g.line(Pen::Thin, p(-0.6, 0.6), p(0.6, 0.6));
            g.line(Pen::Thin, p(-1.0, 0.0), p(1.0, 0.0));
            g.line(Pen::Thin, p(-0.6, -0.6), p(0.6, -0.6));
        }
        Characteristic::CircularRunout | Characteristic::TotalRunout => {
            let mut runout = |x: f64| {
                g.line(Pen::Thin, p(x - 0.5, -0.9), p(x + 0.3, 0.7));
                arrow_small(g, p(x + 0.3, 0.7), [0.8, 1.6]);
            };
            if characteristic == Characteristic::TotalRunout {
                runout(-0.4);
                runout(0.4);
                g.line(Pen::Thin, p(-0.9, -0.9), p(0.9, -0.9));
            } else {
                runout(0.0);
            }
        }
    }
}

/// A small open arrowhead for the runout symbols.
fn arrow_small(g: &mut Group, tip: P2, direction: P2) {
    let d = unit2(direction);
    let back = sub2(tip, mul2(d, 1.0));
    let side = mul2(perp(d), 0.5);
    g.line(Pen::Thin, tip, add2(back, side));
    g.line(Pen::Thin, tip, sub2(back, side));
}

/// A surface finish symbol (ISO 1302) on the surface at `root` (sheet; a
/// leader from `tip` to it when moved off): a short leg and a long one at
/// 60 degrees to each other, a bar closing them for material removal, a
/// circle in them for none, the requirement (`h` high) over the long leg's
/// line.
fn surface_finish(g: &mut Group, tip: P2, root: P2, finish: &SurfaceFinish, h: f64) {
    if len2(sub2(root, tip)) > 0.5 {
        g.line(Pen::Thin, root, tip);
        arrow(g, tip, unit2(sub2(tip, root)));
    }
    let short = 1.4 * h;
    let long = 2.0 * short;
    let (c60, s60) = (0.5, 3f64.sqrt() / 2.0);
    let left = [root[0] - short * c60, root[1] + short * s60];
    let right = [root[0] + long * c60, root[1] + long * s60];
    g.line(Pen::Thin, root, left);
    g.line(Pen::Thin, root, right);
    match finish.process {
        FinishProcess::Removal => {
            // At the short leg's height across to the long leg.
            g.line(Pen::Thin, left, [root[0] + short * c60, left[1]]);
        }
        FinishProcess::NoRemoval => {
            let r = short * 0.35;
            g.items.push(Item::Arc {
                pen: Pen::Thin,
                center: [root[0], root[1] + 2.0 * r],
                radius: r,
                start: 0.0,
                end: TAU,
            });
        }
        FinishProcess::Any => {}
    }
    if let Some(text) = finish.text.as_deref().filter(|t| !t.trim().is_empty()) {
        let width = text_width(text, h) + 1.0;
        g.line(Pen::Thin, right, [right[0] + width, right[1]]);
        g.text(
            [right[0] + 0.5, right[1] - h - 0.8],
            h,
            0.0,
            Anchor::Start,
            text.trim(),
        );
    }
}

/// A group's box on the sheet: its lines' and arcs' ends and circles,
/// its texts by their width and height.
pub fn group_bounds(g: &Group) -> Bounds {
    let mut b = Bounds::empty();
    for item in &g.items {
        match item {
            Item::Line { a, b: q, .. } => {
                b.add(*a);
                b.add(*q);
            }
            Item::Arc { center, radius, .. } => {
                b.add([center[0] - radius, center[1] - radius]);
                b.add([center[0] + radius, center[1] + radius]);
            }
            Item::Polyline { points, .. } | Item::Fill { points } | Item::Shade { points, .. } => {
                for p in points {
                    b.add(*p);
                }
            }
            Item::Text {
                at,
                height,
                angle,
                anchor,
                text,
            } => {
                let w = text_width(text, *height);
                let along = [angle.cos(), angle.sin()];
                let up = [-angle.sin(), angle.cos()];
                let start = match anchor {
                    Anchor::Start => *at,
                    Anchor::Middle => sub2(*at, mul2(along, w / 2.0)),
                    Anchor::End => sub2(*at, mul2(along, w)),
                };
                for p in [
                    start,
                    add2(start, mul2(along, w)),
                    add2(start, mul2(up, *height)),
                    add2(add2(start, mul2(along, w)), mul2(up, *height)),
                ] {
                    b.add(p);
                }
            }
        }
    }
    b
}

/// The place a drag of an annotation by `delta` (sheet) gives it: a
/// dimension line moves across, its text along it; a radius' or a
/// leader's text moves; an angle's arc grows or shrinks.
pub fn moved(annotation: &mut ViewAnnotation, seen: &[Seen], frame: &Frame<'_>, delta: P2) {
    let shift = |offset: Option<P2>, default: P2| {
        let [u, v] = offset.unwrap_or(default);
        Some([u + delta[0], v + delta[1]])
    };
    match annotation.kind {
        AnnotationType::Linear => {
            let Ok((_, d)) = linear_axes(annotation.along, annotation.turn, seen) else {
                return;
            };
            let n = perp(d);
            let (pa, pb) = (frame.at(seen[0].point()), frame.at(seen[1].point()));
            let c = linear_offset(annotation, mid(pa, pb), n, &frame.view_box);
            annotation.line_offset = Some(c + dot2(delta, n));
            let [u, v] = annotation.text_offset.unwrap_or([0.0, 0.0]);
            let along = upright_along(d);
            annotation.text_offset = Some([u + dot2(delta, along), v]);
        }
        AnnotationType::Ordinate => {
            let (_, n) = ordinate_axes(annotation);
            let origin = frame.at(seen[0].point());
            let level = annotation.line_offset.unwrap_or_else(|| {
                let b = &frame.view_box;
                if b.is_empty() {
                    LINE_GAP
                } else {
                    dot2(sub2(b.max, origin), n) + LINE_GAP
                }
            });
            annotation.line_offset = Some(level + dot2(delta, n));
        }
        AnnotationType::Angle => {
            if let Ok((_, r1, r2)) = corner(seen) {
                let middle = unit2(add2(r1, r2));
                let radius = arc_radius(annotation, seen, frame);
                annotation.line_offset = Some((radius + dot2(delta, middle)).max(2.0));
            }
        }
        AnnotationType::Radius | AnnotationType::Diameter => match seen.first() {
            Some(Seen::Circle {
                radius,
                end_on: true,
                ..
            }) => {
                let out = radius * frame.map.scale + 10.0;
                annotation.text_offset = shift(
                    annotation.text_offset,
                    [out * FRAC_1_SQRT_2, out * FRAC_1_SQRT_2],
                );
            }
            Some(seen) => {
                let (center, radius, across) = match *seen {
                    Seen::Circle {
                        center,
                        radius,
                        across,
                        ..
                    } => (center, radius, across),
                    Seen::Axis { a, b, radius } => (mid(a, b), radius, perp(unit2(sub2(b, a)))),
                    _ => return,
                };
                let n = perp(across);
                let (a, b) = (
                    frame.at(sub2(center, mul2(across, radius))),
                    frame.at(add2(center, mul2(across, radius))),
                );
                let c = annotation
                    .line_offset
                    .unwrap_or_else(|| linear_offset(annotation, mid(a, b), n, &frame.view_box));
                annotation.line_offset = Some(c + dot2(delta, n));
            }
            None => {}
        },
        AnnotationType::Note | AnnotationType::Frame => {
            annotation.text_offset = shift(annotation.text_offset, NOTE_SHIFT);
        }
        AnnotationType::Datum => {
            annotation.text_offset = shift(annotation.text_offset, DATUM_SHIFT);
        }
        AnnotationType::Finish => {
            annotation.text_offset = shift(annotation.text_offset, [0.0, 0.0]);
        }
        AnnotationType::Balloon => {
            annotation.text_offset = shift(annotation.text_offset, BALLOON_SHIFT);
        }
        AnnotationType::HoleTable | AnnotationType::PartsList => {
            if let Some(table) = &mut annotation.table {
                table.place = add2(table.place, delta);
            }
        }
        // Drawn where their picks are (an edge extension's reach is its
        // `line_offset`).
        AnnotationType::CenterMark
        | AnnotationType::Centerline
        | AnnotationType::CenterPattern
        | AnnotationType::EdgeExtension => {}
    }
}

/// The angle step a leader's drag snaps to, and how near it (degrees,
/// mitcad#1294).
pub const LEADER_STEP: f64 = 45.0;
pub const LEADER_SNAP: f64 = 4.0;

/// Where the leader of a note, a datum label or a feature control frame
/// runs on the sheet, as [`lay_out`] draws it: from its end at the
/// feature (the arrow; a datum triangle's apex) to its end at the text (a
/// note's shoulder, the side of the frame or of the datum's square it
/// meets). `cells`: a frame's cells, as [`lay_out`] takes them. None for
/// the others.
pub fn leader_line(
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    cells: &[String],
) -> Option<(P2, P2)> {
    let first = seen.first()?;
    let tip = leader_tip(annotation, first, frame);
    let h = frame.text;
    match annotation.kind {
        AnnotationType::Note => {
            Some((tip, add2(tip, annotation.text_offset.unwrap_or(NOTE_SHIFT))))
        }
        AnnotationType::Datum => {
            let at = add2(tip, annotation.text_offset.unwrap_or(DATUM_SHIFT));
            let along = leader_along(annotation, first);
            let label = datum_label(tip, along, at, h, annotation.terminator());
            Some((label.from, label.end))
        }
        AnnotationType::Frame => {
            let at = add2(tip, annotation.text_offset.unwrap_or(NOTE_SHIFT));
            let (widths, cell) = frame_widths(cells, h);
            let total = widths.iter().sum::<f64>().max(cell);
            Some((tip, frame_leader_from(tip, at, total, cell)))
        }
        _ => None,
    }
}

/// A leader's drag snapped to an angle: the drag (sheet) and the angle
/// the leader then runs at from its feature, degrees counter-clockwise
/// from the sheet's x axis (0 to 315).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeaderSnap {
    pub to: P2,
    pub angle: f64,
}

/// The drag of a note's text, a datum's square or a frame by `delta`
/// (sheet) snapped so that its leader runs at a multiple of
/// [`LEADER_STEP`] when it would run within [`LEADER_SNAP`] of one, its
/// length kept (mitcad#1294); None when it would not, or for an
/// annotation without such a leader.
pub fn snap_leader(
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    cells: &[String],
    delta: P2,
) -> Option<LeaderSnap> {
    let line = |d: P2| {
        let mut placed = annotation.clone();
        moved(&mut placed, seen, frame, d);
        let (from, to) = leader_line(&placed, seen, frame, cells)?;
        let run = sub2(to, from);
        (len2(run) > 1e-6).then_some((from, to, run[1].atan2(run[0]).to_degrees()))
    };
    let (from, to, angle) = line(delta)?;
    let snapped = (angle / LEADER_STEP).round() * LEADER_STEP;
    if (angle - snapped).abs() > LEADER_SNAP {
        return None;
    }
    let r = len2(sub2(to, from));
    let a = snapped.to_radians();
    let want = add2(from, mul2([a.cos(), a.sin()], r));
    // The text's end moves with the drag; the feature's end stays while
    // the text keeps its side of the feature.
    let snapped_delta = add2(delta, sub2(want, to));
    let (_, _, check) = line(snapped_delta)?;
    let off = (check - snapped + 180.0).rem_euclid(360.0) - 180.0;
    (off.abs() < 1e-6).then_some(LeaderSnap {
        to: snapped_delta,
        // No -0 for the leader along the x axis.
        angle: snapped.rem_euclid(360.0) + 0.0,
    })
}

/// The direction a dimension's text runs along a line along `d`.
fn upright_along(d: P2) -> P2 {
    let angle = upright(d);
    [angle.cos(), angle.sin()]
}

/// The place `at` (sheet) gives a new annotation: its dimension line or
/// arc through it with its text there, a leader's text there.
pub fn placed_at(annotation: &mut ViewAnnotation, seen: &[Seen], frame: &Frame<'_>, at: P2) {
    match annotation.kind {
        AnnotationType::Linear => {
            if let Ok((_, d)) = linear_axes(annotation.along, annotation.turn, seen) {
                let n = perp(d);
                let (pa, pb) = (frame.at(seen[0].point()), frame.at(seen[1].point()));
                let middle = mid(pa, pb);
                annotation.line_offset = Some(dot2(sub2(at, middle), n));
                let u = dot2(sub2(at, middle), upright_along(d));
                // Left in the middle unless clearly off it.
                let half = len2(sub2(pb, pa)) / 2.0;
                annotation.text_offset = (u.abs() > half).then_some([u, 0.0]);
            }
        }
        AnnotationType::Ordinate => {
            let (_, n) = ordinate_axes(annotation);
            let origin = frame.at(seen[0].point());
            annotation.line_offset = Some(dot2(sub2(at, origin), n));
        }
        AnnotationType::Angle => {
            if let Ok((c, _, _)) = corner(seen) {
                annotation.line_offset = Some(len2(sub2(at, frame.at(c))).max(2.0));
            }
        }
        AnnotationType::Radius | AnnotationType::Diameter => match seen.first() {
            Some(Seen::Circle {
                center,
                end_on: true,
                ..
            }) => {
                annotation.text_offset = Some(sub2(at, frame.at(*center)));
            }
            Some(seen) => {
                let (center, radius, across) = match *seen {
                    Seen::Circle {
                        center,
                        radius,
                        across,
                        ..
                    } => (center, radius, across),
                    Seen::Axis { a, b, radius } => (mid(a, b), radius, perp(unit2(sub2(b, a)))),
                    _ => return,
                };
                let n = perp(across);
                let middle = mid(
                    frame.at(sub2(center, mul2(across, radius))),
                    frame.at(add2(center, mul2(across, radius))),
                );
                annotation.line_offset = Some(dot2(sub2(at, middle), n));
            }
            None => {}
        },
        AnnotationType::Note
        | AnnotationType::Datum
        | AnnotationType::Frame
        | AnnotationType::Finish
        | AnnotationType::Balloon => {
            let tip = leader_tip(annotation, &seen[0], frame);
            annotation.text_offset = Some(sub2(at, tip));
        }
        AnnotationType::HoleTable | AnnotationType::PartsList => {
            if let Some(table) = &mut annotation.table {
                table.place = at;
            }
        }
        AnnotationType::CenterMark
        | AnnotationType::Centerline
        | AnnotationType::CenterPattern
        | AnnotationType::EdgeExtension => {}
    }
}

/// What an annotation keeps in its view's coordinates carried over its
/// view turned by `angle` degrees counter-clockwise in its own plane
/// (mitcad#729), so that it turns with the view: where a leader's pick
/// was clicked, a leader's text and a circle's text from their points, a
/// horizontal or vertical dimension's direction (its `turn`; a quarter
/// turn makes it the other one). A dimension's line and text keep their
/// places from their picks across the line; the text keeps its side when
/// the way it reads turns over.
pub fn turned_with_view(annotation: &mut ViewAnnotation, angle: f64) {
    let rotate = |p: P2| turned(p, Some(angle));
    if let Some(at) = annotation.at {
        annotation.at = Some(rotate(at));
    }
    match annotation.kind {
        AnnotationType::Note
        | AnnotationType::Datum
        | AnnotationType::Frame
        | AnnotationType::Finish
        | AnnotationType::Balloon
        | AnnotationType::Radius
        | AnnotationType::Diameter => {
            if let Some(shift) = annotation.text_offset {
                annotation.text_offset = Some(rotate(shift));
            }
        }
        AnnotationType::Linear | AnnotationType::Ordinate
            if matches!(annotation.along, Some(Along::Horizontal | Along::Vertical)) =>
        {
            let before = match annotation.along {
                Some(Along::Vertical) if annotation.kind == AnnotationType::Linear => {
                    turned([0.0, 1.0], annotation.turn)
                }
                _ => turned([1.0, 0.0], annotation.turn),
            };
            let mut turn = (annotation.turn.unwrap_or(0.0) + angle).rem_euclid(360.0);
            if turn > 180.0 {
                turn -= 360.0;
            }
            let (left, right) = ((turn - 90.0).abs() < 1e-9, (turn + 90.0).abs() < 1e-9);
            // A quarter turn of a linear dimension is the other axis.
            if annotation.kind == AnnotationType::Linear {
                match annotation.along {
                    Some(Along::Horizontal) if left => {
                        annotation.along = Some(Along::Vertical);
                        turn = 0.0;
                    }
                    Some(Along::Vertical) if right => {
                        annotation.along = Some(Along::Horizontal);
                        turn = 0.0;
                    }
                    _ => {}
                }
            }
            annotation.turn = (turn.abs() > 1e-9).then_some(turn);
            // The text reads from below or from the right: where that
            // turns it over, its shift along and across the line does too.
            if annotation.kind == AnnotationType::Linear
                && let Some([u, v]) = annotation.text_offset
            {
                let after = rotate(before);
                if dot2(upright_along(after), rotate(upright_along(before))) < 0.0 {
                    annotation.text_offset = Some([-u, -v]);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitcad_model::Tolerance;

    fn map() -> ViewMap {
        ViewMap::new(1.0, [0.0, 0.0], [100.0, 100.0])
    }

    fn view_box() -> Bounds {
        let mut b = Bounds::empty();
        b.add([100.0, 100.0]);
        b.add([140.0, 120.0]);
        b
    }

    fn words(text: &str) -> ValueText {
        ValueText {
            text: text.to_owned(),
            main: text.to_owned(),
            ..ValueText::default()
        }
    }

    fn texts(laid: &Laid) -> Vec<String> {
        laid.group
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_linear_dimension_measures_along_the_view_and_lies_beyond_it() {
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        // The two upright sides of a 40 x 20 box at the view's origin.
        let seen = [
            Seen::Line([0.0, 0.0], [0.0, 20.0]),
            Seen::Line([40.0, 0.0], [40.0, 20.0]),
        ];
        let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
        assert_eq!(default_along(&seen), Along::Horizontal);
        d.along = Some(Along::Horizontal);
        assert_eq!(measure(&d, &seen).unwrap(), Some(40.0));
        d.along = Some(Along::Aligned);
        assert_eq!(measure(&d, &seen).unwrap(), Some(40.0));
        d.along = Some(Along::Vertical);
        assert_eq!(measure(&d, &seen).unwrap(), Some(0.0));
        d.along = Some(Along::Horizontal);
        let laid = lay_out(&d, &seen, &frame, Some(&words("40")), &[]).unwrap();
        assert_eq!(texts(&laid), ["40"]);
        // The picks lie in the lower half of the box... at its middle: the
        // line goes below or above it, beyond its edge.
        assert!(
            laid.bounds.max[1] > 120.0 + LINE_GAP - 1e-9 || laid.bounds.min[1] < 100.0 - 9.0,
            "{:?}",
            laid.bounds
        );
        // Placed through a point above: the line there.
        placed_at(&mut d, &seen, &frame, [120.0, 140.0]);
        assert_eq!(d.line_offset, Some(30.0));
        assert_eq!(d.text_offset, None);
        // Dragged down by 5: 5 nearer.
        moved(&mut d, &seen, &frame, [3.0, -5.0]);
        assert_eq!(d.line_offset, Some(25.0));
        assert_eq!(d.text_offset, Some([3.0, 0.0]));
    }

    #[test]
    fn a_dragged_text_moves_the_way_the_drag_goes_whatever_the_picks_order() {
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let left = Seen::Line([0.0, 0.0], [0.0, 20.0]);
        let right = Seen::Line([40.0, 0.0], [40.0, 20.0]);
        let text_x = |laid: &Laid| {
            laid.group
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Text { at, .. } => Some(at[0]),
                    _ => None,
                })
                .unwrap()
        };
        for seen in [[left, right], [right, left]] {
            let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
            d.along = Some(Along::Horizontal);
            let before = text_x(&lay_out(&d, &seen, &frame, Some(&words("40")), &[]).unwrap());
            // Dragged 30 mm right: past the right end, the line run on under it.
            moved(&mut d, &seen, &frame, [30.0, 0.0]);
            let laid = lay_out(&d, &seen, &frame, Some(&words("40")), &[]).unwrap();
            assert!(
                (text_x(&laid) - before - 30.0).abs() < 1e-9,
                "{before} -> {}",
                text_x(&laid)
            );
            let run_on = laid.group.items.iter().any(|i| {
                matches!(i, Item::Line { a, b, .. }
                    if (a[0] - 140.0).abs() < 1e-9 && (b[0] - 150.0).abs() < 1e-9)
            });
            assert!(run_on, "{:?}", laid.group.items);
            // Placed through a point left of the middle: the text there.
            placed_at(&mut d, &seen, &frame, [110.0, 140.0]);
            assert_eq!(d.text_offset, None, "inside the line: left in its middle");
        }
    }

    #[test]
    fn circles_angles_and_ordinates_are_measured() {
        let circle = Seen::Circle {
            center: [10.0, 10.0],
            radius: 5.0,
            end_on: true,
            across: [1.0, 0.0],
            arc: None,
        };
        let r = ViewAnnotation::new(1, AnnotationType::Radius, Vec::new());
        let dia = ViewAnnotation::new(2, AnnotationType::Diameter, Vec::new());
        assert_eq!(measure(&r, &[circle]).unwrap(), Some(5.0));
        assert_eq!(measure(&dia, &[circle]).unwrap(), Some(10.0));
        let side = Seen::Axis {
            a: [0.0, 0.0],
            b: [0.0, 30.0],
            radius: 4.0,
        };
        assert_eq!(measure(&dia, &[side]).unwrap(), Some(8.0));
        assert!(measure(&dia, &[Seen::Point([0.0, 0.0])]).is_err());
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let laid = lay_out(&dia, &[circle], &frame, Some(&words("Ø10")), &[]).unwrap();
        assert_eq!(texts(&laid), ["Ø10"]);
        assert!(lay_out(&dia, &[side], &frame, Some(&words("Ø8")), &[]).is_ok());

        // 90 degrees between X and Y.
        let a = ViewAnnotation::new(3, AnnotationType::Angle, Vec::new());
        let lines = [
            Seen::Line([0.0, 0.0], [10.0, 0.0]),
            Seen::Line([0.0, 0.0], [0.0, 10.0]),
        ];
        let value = measure(&a, &lines).unwrap().unwrap();
        assert!((value - PI / 2.0).abs() < 1e-12);
        let parallel = [
            Seen::Line([0.0, 0.0], [10.0, 0.0]),
            Seen::Line([0.0, 5.0], [10.0, 5.0]),
        ];
        assert!(measure(&a, &parallel).unwrap_err().contains("parallel"));

        let mut o = ViewAnnotation::new(4, AnnotationType::Ordinate, Vec::new());
        o.along = Some(Along::Horizontal);
        let points = [Seen::Point([0.0, 0.0]), Seen::Point([25.0, 5.0])];
        assert_eq!(measure(&o, &points).unwrap(), Some(25.0));
        o.along = Some(Along::Vertical);
        assert_eq!(measure(&o, &points).unwrap(), Some(5.0));
    }

    #[test]
    fn annotations_are_checked() {
        let pick = ViewPick {
            body: "F2.b0".parse().unwrap(),
            occurrence: None,
            edge: Some("E{F2:start|F2:side(c1[c4,c2])}".parse().unwrap()),
            face: None,
            point: None,
            on_face: None,
        };
        let mut d = ViewAnnotation::new(1, AnnotationType::Linear, vec![pick.clone()]);
        assert!(d.check().unwrap_err().contains("takes 2 picks"));
        d.picks.push(pick.clone());
        assert!(d.check().unwrap_err().contains("along a direction"));
        d.along = Some(Along::Aligned);
        d.check().unwrap();
        d.text = Some("x".to_owned());
        assert!(d.check().unwrap_err().contains("no text of its own"));
        d.text = None;
        d.tolerance = Some(Tolerance::Symmetric { value: 0.1 });
        d.check().unwrap();
        let mut note = ViewAnnotation::new(2, AnnotationType::Note, vec![pick.clone()]);
        assert!(note.check().unwrap_err().contains("needs a text"));
        note.text = Some("DEBURR".to_owned());
        note.check().unwrap();
        note.tolerance = Some(Tolerance::Symmetric { value: 0.1 });
        assert!(note.check().unwrap_err().contains("no value's format"));
        let mut datum = ViewAnnotation::new(3, AnnotationType::Datum, vec![pick.clone()]);
        assert!(datum.check().unwrap_err().contains("letter"));
        datum.letter = Some("I".to_owned());
        assert!(datum.check().is_err());
        datum.letter = Some("A".to_owned());
        datum.check().unwrap();
        let mut o = ViewAnnotation::new(4, AnnotationType::Ordinate, vec![pick.clone(), pick]);
        o.along = Some(Along::Aligned);
        assert!(o.check().unwrap_err().contains("x or y"));
    }

    #[test]
    fn marks_notes_and_symbols_are_drawn() {
        let mut g = Group::new("center-mark");
        center_mark(&mut g, [50.0, 50.0], 10.0);
        let b = group_bounds(&g);
        assert!((b.width() - 2.0 * (10.0 + CENTER_BEYOND)).abs() < 1e-9);
        let note = SheetNote {
            id: 5,
            text: "ALL EDGES\nDEBURRED".to_owned(),
            at: [20.0, 40.0],
            height: None,
        };
        note.check().unwrap();
        let laid = lay_out_note(&note, DIMENSION_TEXT);
        assert_eq!(laid.group.items.len(), 2);
        for characteristic in Characteristic::ALL {
            let mut g = Group::new("feature-control-frame");
            symbol(&mut g, characteristic, [0.0, 0.0], 4.0);
            assert!(!g.items.is_empty(), "{characteristic:?}");
        }
        let mut g = Group::new("surface-finish");
        surface_finish(
            &mut g,
            [0.0, 0.0],
            [0.0, 0.0],
            &SurfaceFinish {
                process: FinishProcess::Removal,
                text: Some("Ra 1.6".to_owned()),
            },
            DIMENSION_TEXT,
        );
        assert!(
            g.items
                .iter()
                .any(|i| matches!(i, Item::Text { text, .. } if text == "Ra 1.6"))
        );
    }

    fn circle(center: P2, radius: f64) -> Seen {
        Seen::Circle {
            center,
            radius,
            end_on: true,
            across: [1.0, 0.0],
            arc: None,
        }
    }

    fn arcs(laid: &Laid) -> Vec<(P2, f64, f64, f64)> {
        laid.group
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Arc {
                    center,
                    radius,
                    start,
                    end,
                    ..
                } => Some((*center, *radius, *start, *end)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_centre_pattern_marks_a_bolt_circle_a_row_or_each_circle() {
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let pattern = ViewAnnotation::new(1, AnnotationType::CenterPattern, Vec::new());
        // Six holes of 3 on a circle of 20 about (30, 10): the whole
        // circle through them, centred there.
        let ring: Vec<Seen> = (0..6)
            .map(|k| {
                let a = f64::from(k) * TAU / 6.0;
                circle([30.0 + 20.0 * a.cos(), 10.0 + 20.0 * a.sin()], 3.0)
            })
            .collect();
        let laid = lay_out(&pattern, &ring, &frame, None, &[]).unwrap();
        assert_eq!(laid.group.class, "center-mark");
        let pitch = arcs(&laid);
        assert_eq!(pitch.len(), 1, "{pitch:?}");
        let (c, r, start, end) = pitch[0];
        assert!(
            (c[0] - 130.0).abs() < 1e-6 && (c[1] - 110.0).abs() < 1e-6,
            "{c:?}"
        );
        assert!((r - 20.0).abs() < 1e-6 && (end - start - TAU).abs() < 1e-9);
        // Three of them on a quarter: an arc over them, not the circle.
        let quarter: Vec<Seen> = (0..3)
            .map(|k| {
                let a = f64::from(k) * TAU / 8.0;
                circle([20.0 * a.cos(), 20.0 * a.sin()], 2.0)
            })
            .collect();
        let laid = lay_out(&pattern, &quarter, &frame, None, &[]).unwrap();
        let (_, _, start, end) = arcs(&laid)[0];
        assert!(end - start < PI, "{start} {end}");
        // In a row: one line through them, no arc.
        let row = [
            circle([0.0, 0.0], 2.0),
            circle([10.0, 0.0], 2.0),
            circle([20.0, 0.0], 2.0),
        ];
        let laid = lay_out(&pattern, &row, &frame, None, &[]).unwrap();
        assert!(arcs(&laid).is_empty());
        assert!(laid.bounds.width() > 20.0 + 2.0 * (2.0 + CENTER_BEYOND) - 1e-6);
        // Scattered: a centre mark each.
        let scattered = [
            circle([0.0, 0.0], 2.0),
            circle([10.0, 3.0], 2.0),
            circle([4.0, 25.0], 2.0),
            circle([-30.0, 1.0], 2.0),
        ];
        let laid = lay_out(&pattern, &scattered, &frame, None, &[]).unwrap();
        assert!(arcs(&laid).is_empty());
        assert_eq!(laid.group.items.len(), 4 * 4);
        // A circle seen from the side is no hole of a pattern.
        let side = Seen::Axis {
            a: [0.0, 0.0],
            b: [0.0, 5.0],
            radius: 1.0,
        };
        assert!(lay_out(&pattern, &[side], &frame, None, &[]).is_err());
    }

    #[test]
    fn edge_extensions_run_edges_on_to_their_corner() {
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let mut extension = ViewAnnotation::new(1, AnnotationType::EdgeExtension, Vec::new());
        // A corner rounded away: the bottom edge ends at x 30, the right
        // side starts at y 5; their lines meet at (35, 0).
        let edges = [
            Seen::Line([0.0, 0.0], [30.0, 0.0]),
            Seen::Line([35.0, 5.0], [35.0, 20.0]),
        ];
        let laid = lay_out(&extension, &edges, &frame, None, &[]).unwrap();
        assert_eq!(laid.group.class, "edge-extension");
        let lines: Vec<(P2, P2)> = laid
            .group
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Line { a, b, .. } => Some((*a, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 2, "{lines:?}");
        // Along the bottom from x 31 to 2 mm past the corner.
        assert!((lines[0].0[0] - 131.0).abs() < 1e-9 && (lines[0].1[0] - 137.0).abs() < 1e-9);
        assert!((lines[1].1[1] - 98.0).abs() < 1e-9, "{:?}", lines[1]);
        // Parallel edges meet nowhere.
        let parallel = [
            Seen::Line([0.0, 0.0], [30.0, 0.0]),
            Seen::Line([0.0, 5.0], [30.0, 5.0]),
        ];
        assert!(lay_out(&extension, &parallel, &frame, None, &[]).is_err());
        // One edge: beyond both its ends by its reach.
        extension.line_offset = Some(5.0);
        let laid = lay_out(&extension, &edges[..1], &frame, None, &[]).unwrap();
        assert!((laid.bounds.min[0] - (100.0 - 6.0)).abs() < 1e-9);
        assert!((laid.bounds.max[0] - (130.0 + 6.0)).abs() < 1e-9);
    }

    #[test]
    fn letters_take_the_height_given() {
        let map = map();
        let frame = |text: f64| Frame {
            map: &map,
            view_box: view_box(),
            text,
        };
        let heights = |laid: &Laid| -> Vec<f64> {
            laid.group
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Text { height, .. } => Some(*height),
                    _ => None,
                })
                .collect()
        };
        let seen = [
            Seen::Line([0.0, 0.0], [0.0, 20.0]),
            Seen::Line([40.0, 0.0], [40.0, 20.0]),
        ];
        let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
        d.along = Some(Along::Horizontal);
        let laid = lay_out(&d, &seen, &frame(5.0), Some(&words("40")), &[]).unwrap();
        assert_eq!(heights(&laid), [5.0]);
        let mut note = ViewAnnotation::new(2, AnnotationType::Note, vec![]);
        note.text = Some("A\nB".to_owned());
        let laid = lay_out(&note, &seen[..1], &frame(2.5), None, &[]).unwrap();
        assert_eq!(heights(&laid), [2.5, 2.5]);
        let sheet_note = SheetNote {
            id: 3,
            text: "C".to_owned(),
            at: [0.0, 0.0],
            height: None,
        };
        assert_eq!(heights(&lay_out_note(&sheet_note, 7.0)), [7.0]);
        // Checked.
        assert!(check_height(0.0).unwrap_err().contains("high"));
        assert!(check_height(MAX_TEXT_HEIGHT + 1.0).is_err());
        check_height(5.0).unwrap();
    }

    #[test]
    fn a_dimension_turns_with_its_view() {
        let pick = ViewPick {
            body: "F2.b0".parse().unwrap(),
            occurrence: None,
            edge: Some("E{F2:start|F2:side(c1[c4,c2])}".parse().unwrap()),
            face: None,
            point: None,
            on_face: None,
        };
        let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
        d.along = Some(Along::Horizontal);
        // Seen after the view turned a quarter to the left, its picks
        // stand one above the other: it measures them as before.
        let before = [Seen::Point([0.0, 0.0]), Seen::Point([40.0, 5.0])];
        let after = [Seen::Point([0.0, 0.0]), Seen::Point([-5.0, 40.0])];
        turned_with_view(&mut d, 90.0);
        assert_eq!(d.along, Some(Along::Vertical));
        assert_eq!(d.turn, None);
        assert!((measure(&d, &after).unwrap().unwrap() - 40.0).abs() < 1e-9);
        // An angle of 30 degrees: its turn kept, its value the same.
        let mut d =
            ViewAnnotation::new(1, AnnotationType::Linear, vec![pick.clone(), pick.clone()]);
        d.along = Some(Along::Horizontal);
        let value = measure(&d, &before).unwrap().unwrap();
        turned_with_view(&mut d, 30.0);
        assert_eq!(d.along, Some(Along::Horizontal));
        assert!((d.turn.unwrap() - 30.0).abs() < 1e-9);
        d.check().unwrap();
        let rotated: Vec<Seen> = before
            .iter()
            .map(|s| Seen::Point(turned(s.point(), Some(30.0))))
            .collect();
        assert!((measure(&d, &rotated).unwrap().unwrap() - value).abs() < 1e-9);
        // An ordinate keeps its signed value.
        let mut o = ViewAnnotation::new(2, AnnotationType::Ordinate, Vec::new());
        o.along = Some(Along::Vertical);
        let value = measure(&o, &before).unwrap().unwrap();
        turned_with_view(&mut o, -120.0);
        let rotated: Vec<Seen> = before
            .iter()
            .map(|s| Seen::Point(turned(s.point(), Some(-120.0))))
            .collect();
        assert!((measure(&o, &rotated).unwrap().unwrap() - value).abs() < 1e-9);
        // A leader's text turns about its arrow.
        let mut note = ViewAnnotation::new(3, AnnotationType::Note, Vec::new());
        note.text = Some("X".to_owned());
        note.text_offset = Some([10.0, 0.0]);
        note.at = Some([5.0, 0.0]);
        turned_with_view(&mut note, 90.0);
        let [u, v] = note.text_offset.unwrap();
        assert!(u.abs() < 1e-9 && (v - 10.0).abs() < 1e-9);
        assert!((note.at.unwrap()[1] - 5.0).abs() < 1e-9);
        // An aligned dimension has no turn of its own.
        let mut aligned = ViewAnnotation::new(4, AnnotationType::Linear, vec![pick.clone(), pick]);
        aligned.along = Some(Along::Aligned);
        turned_with_view(&mut aligned, 45.0);
        assert_eq!(aligned.turn, None);
        aligned.turn = Some(10.0);
        assert!(aligned.check().unwrap_err().contains("turns with its view"));
    }

    #[test]
    fn lines_arcs_and_texts_are_drawn_where_their_view_puts_them() {
        let line = SketchItem::Line {
            id: 1,
            from: [0.0, 0.0],
            to: [10.0, 0.0],
            pen: SketchPen::Thick,
        };
        line.check().unwrap();
        // In a view at 2:1 placed at (100, 50).
        let map = ViewMap::new(2.0, [0.0, 0.0], [100.0, 50.0]);
        let laid = lay_out_sketch(&line, &map, DIMENSION_TEXT);
        assert_eq!(laid.group.class, "sketch");
        assert_eq!(laid.group.id.as_deref(), Some("A1"));
        assert!(matches!(
            laid.group.items[0],
            Item::Line { pen: Pen::Thick, a, b } if a == [100.0, 50.0] && b == [120.0, 50.0]
        ));
        let arc = SketchItem::Arc {
            id: 2,
            center: [5.0, 5.0],
            radius: 4.0,
            start: 0.0,
            end: PI,
            pen: SketchPen::Thin,
        };
        arc.check().unwrap();
        let laid = lay_out_sketch(&arc, &map, DIMENSION_TEXT);
        let (c, r, _, _) = arcs(&laid)[0];
        assert!(c == [110.0, 60.0] && (r - 8.0).abs() < 1e-12);
        let text = SketchItem::Text {
            id: 3,
            text: "KEEP\nCLEAR".to_owned(),
            at: [0.0, 10.0],
            height: None,
        };
        text.check().unwrap();
        assert_eq!(text.label(), "KEEP / CLEAR");
        assert_eq!(texts(&lay_out_sketch(&text, &map, 5.0)), ["KEEP", "CLEAR"]);
        // Carried with its view turned a quarter.
        let mut turned_arc = arc.clone();
        turned_arc.carried(&|p| turned(p, Some(90.0)), PI / 2.0, 1.0);
        let SketchItem::Arc {
            center, start, end, ..
        } = turned_arc
        else {
            panic!("an arc");
        };
        assert!((center[0] + 5.0).abs() < 1e-9 && (center[1] - 5.0).abs() < 1e-9);
        assert!((start - PI / 2.0).abs() < 1e-9 && (end - 3.0 * PI / 2.0).abs() < 1e-9);
        // Checked.
        let short = SketchItem::Line {
            id: 4,
            from: [1.0, 1.0],
            to: [1.0, 1.0],
            pen: SketchPen::Thin,
        };
        assert!(short.check().unwrap_err().contains("length"));
        let backwards = SketchItem::Arc {
            id: 5,
            center: [0.0, 0.0],
            radius: 1.0,
            start: 1.0,
            end: 0.5,
            pen: SketchPen::Thin,
        };
        assert!(backwards.check().is_err());
        // Its file form: the pen left out when thin.
        let value = serde_json::to_value(&arc).unwrap();
        assert_eq!(value["type"], "arc");
        assert!(value.get("pen").is_none());
        let back: SketchItem = serde_json::from_value(value).unwrap();
        assert_eq!(back, arc);
    }

    #[test]
    fn the_placing_click_chooses_horizontal_vertical_or_aligned() {
        // mitcad#1272: as a sketch's dimensions, between two points 40
        // apart along x and 20 along y.
        let (a, b) = ([0.0, 0.0], [40.0, 20.0]);
        let along = |at: P2| placed_along(a, b, Some(at));
        // Above or below them: horizontal; beside them: vertical.
        assert_eq!(along([20.0, 35.0]), Along::Horizontal);
        assert_eq!(along([20.0, -15.0]), Along::Horizontal);
        assert_eq!(along([55.0, 10.0]), Along::Vertical);
        assert_eq!(along([-12.0, 10.0]), Along::Vertical);
        // Between them both ways, or beyond them both ways: aligned.
        assert_eq!(along([25.0, 8.0]), Along::Aligned);
        assert_eq!(along([60.0, 40.0]), Along::Aligned);
        // Level or plumb points measure along their axis wherever placed,
        // so never 0.
        for at in [[20.0, 10.0], [60.0, 0.0], [20.0, -5.0]] {
            assert_eq!(
                placed_along([0.0, 0.0], [40.0, 0.0], Some(at)),
                Along::Horizontal
            );
            assert_eq!(
                placed_along([5.0, 0.0], [5.0, 30.0], Some(at)),
                Along::Vertical
            );
        }
        // No click: the axis they lie further apart along.
        assert_eq!(placed_along(a, b, None), Along::Horizontal);
    }

    #[test]
    fn a_centerline_of_two_concentric_arcs_is_the_slots_centre_arc() {
        // mitcad#1272: a curved slot's sides, radii 40 and 50 about the
        // view's origin from 30 to 60 degrees; its ends' centres on the
        // radius 45 at 30 and 60 degrees.
        let (from, to) = (PI / 6.0, PI / 3.0);
        let side = |radius: f64| Seen::Circle {
            center: [0.0, 0.0],
            radius,
            end_on: true,
            across: [1.0, 0.0],
            arc: Some((from, to)),
        };
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let line = ViewAnnotation::new(1, AnnotationType::Centerline, Vec::new());
        let laid = lay_out(&line, &[side(40.0), side(50.0)], &frame, None, &[]).unwrap();
        let drawn = arcs(&laid);
        assert_eq!(drawn.len(), 1, "{drawn:?}");
        let (center, radius, start, end) = drawn[0];
        assert!(len2(sub2(center, [100.0, 100.0])) < 1e-9, "{center:?}");
        assert!((radius - 45.0).abs() < 1e-9, "{radius}");
        // Through both ends' centres, run on beyond each by the ends'
        // radius (5) and a little more.
        let beyond = (5.0 + CENTER_BEYOND) / 45.0;
        assert!((start - (from - beyond)).abs() < 1e-9, "{start}");
        assert!((end - (to + beyond)).abs() < 1e-9, "{end}");
        assert!(laid.group.items.iter().all(|i| matches!(
            i,
            Item::Arc {
                pen: Pen::Chain,
                ..
            }
        )));
        // The sides in the other order, one of them given a whole turn
        // further on: the same arc (its angles may be a whole turn on).
        let mut turned = side(50.0);
        if let Seen::Circle { arc, .. } = &mut turned {
            *arc = Some((from + TAU, to + TAU));
        }
        let again = lay_out(&line, &[turned, side(40.0)], &frame, None, &[]).unwrap();
        let (_, radius, start, end) = arcs(&again)[0];
        assert!((radius - 45.0).abs() < 1e-9);
        let off = (start - (from - beyond)).rem_euclid(TAU);
        assert!(off < 1e-9 || TAU - off < 1e-9, "{start}");
        assert!(
            ((end - start) - (to - from + 2.0 * beyond)).abs() < 1e-9,
            "{start} {end}"
        );
        // Two concentric whole circles have no slot: refused.
        let whole = |radius: f64| circle([0.0, 0.0], radius);
        let error = lay_out(&line, &[whole(4.0), whole(6.0)], &frame, None, &[]).unwrap_err();
        assert!(error.contains("share their center"), "{error}");
    }

    /// Whether the segment from `a` to `b` runs through the inside of the
    /// box from `min` to `max` (touching its outline is not crossing it).
    fn crosses(a: P2, b: P2, min: P2, max: P2) -> bool {
        let eps = 1e-6;
        let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
        let d = sub2(b, a);
        for k in [0_usize, 1] {
            let (lo, hi) = (min[k] + eps, max[k] - eps);
            if d[k].abs() < 1e-12 {
                if a[k] <= lo || a[k] >= hi {
                    return false;
                }
                continue;
            }
            let (mut s0, mut s1) = ((lo - a[k]) / d[k], (hi - a[k]) / d[k]);
            if s0 > s1 {
                std::mem::swap(&mut s0, &mut s1);
            }
            t0 = t0.max(s0);
            t1 = t1.min(s1);
        }
        t1 - t0 > 1e-9
    }

    /// Whether a point lies on the outline of the box from `min` to `max`.
    fn on_outline(p: P2, min: P2, max: P2) -> bool {
        let inside = (0..2).all(|k| p[k] >= min[k] - 1e-9 && p[k] <= max[k] + 1e-9);
        let edge = (0..2).any(|k| (p[k] - min[k]).abs() < 1e-9 || (p[k] - max[k]).abs() < 1e-9);
        inside && edge
    }

    fn lines(laid: &Laid) -> Vec<(P2, P2)> {
        laid.group
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Line { a, b, .. } => Some((*a, *b)),
                _ => None,
            })
            .collect()
    }

    /// The first closed outline drawn: a datum's square, a frame's box.
    fn first_box(laid: &Laid) -> (P2, P2) {
        laid.group
            .items
            .iter()
            .find_map(|i| match i {
                Item::Polyline {
                    points,
                    closed: true,
                    ..
                } => {
                    let mut b = Bounds::empty();
                    for p in points {
                        b.add(*p);
                    }
                    Some((b.min, b.max))
                }
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_datums_triangle_sits_on_its_edge_and_its_leader_meets_the_squares_near_side() {
        // mitcad#1294: a datum on a horizontal edge at (20, 0) of the view,
        // the sheet's (120, 100); its square below left, above, left and
        // right of the tip.
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let edge = Seen::Line([0.0, 0.0], [40.0, 0.0]);
        let tip = [120.0, 100.0];
        for (shift, side) in [
            ([-10.0, -25.0], "top"),
            ([0.0, 12.0], "bottom"),
            ([-20.0, 2.0], "right"),
            ([20.0, 2.0], "left"),
        ] {
            let mut d = ViewAnnotation::new(1, AnnotationType::Datum, Vec::new());
            d.letter = Some("A".to_owned());
            d.at = Some([20.0, 0.0]);
            d.text_offset = Some(shift);
            let laid = lay_out(&d, &[edge], &frame, None, &[]).unwrap();
            let triangle: Vec<P2> = laid
                .group
                .items
                .iter()
                .find_map(|i| match i {
                    Item::Fill { points } => Some(points.clone()),
                    _ => None,
                })
                .unwrap();
            let (min, max) = first_box(&laid);
            let center = mid(min, max);
            // The base along the edge, at the tip; the apex on the square's
            // side of the edge, square to it.
            assert_eq!(triangle.len(), 3);
            for p in &triangle[..2] {
                assert!((p[1] - tip[1]).abs() < 1e-9, "{shift:?}: {triangle:?}");
            }
            assert!((mid(triangle[0], triangle[1])[0] - tip[0]).abs() < 1e-9);
            assert!((len2(sub2(triangle[0], triangle[1])) - 2.0 * DATUM_HALF_BASE).abs() < 1e-9);
            let apex = triangle[2];
            assert!((apex[0] - tip[0]).abs() < 1e-9, "{shift:?}: {apex:?}");
            assert!(
                (apex[1] - tip[1]) * (center[1] - tip[1]) > 0.0,
                "{shift:?}: the apex {apex:?} away from the square {center:?}"
            );
            // One leader, from the apex to the middle of the square's side
            // nearest it; through neither the square nor its letter.
            let leaders = lines(&laid);
            assert_eq!(leaders.len(), 1, "{shift:?}: {leaders:?}");
            let (from, to) = leaders[0];
            assert!(len2(sub2(from, apex)) < 1e-9, "{shift:?}: {from:?}");
            assert!(
                on_outline(to, min, max),
                "{shift:?}: {to:?} {min:?} {max:?}"
            );
            let expected = match side {
                "top" => [center[0], max[1]],
                "bottom" => [center[0], min[1]],
                "left" => [min[0], center[1]],
                _ => [max[0], center[1]],
            };
            assert!(
                len2(sub2(to, expected)) < 1e-9,
                "{shift:?}: {to:?} not {side}"
            );
            assert!(
                !crosses(from, to, min, max),
                "{shift:?}: through the square"
            );
            // The square where the shift puts it: its lower middle.
            assert!(len2(sub2([center[0], min[1]], add2(tip, shift))) < 1e-9);
            assert_eq!(texts(&laid), ["A"]);
        }
        // On a point the triangle sits square to the leader, as before.
        let mut d = ViewAnnotation::new(2, AnnotationType::Datum, Vec::new());
        d.letter = Some("B".to_owned());
        let triangle = Terminator::Triangle;
        let label = datum_label(tip, None, add2(tip, DATUM_SHIFT), DIMENSION_TEXT, triangle);
        assert!((label.mark[0][1] - tip[1]).abs() < 1e-9);
        assert!((label.mark[2][1] - tip[1] - DATUM_HEIGHT).abs() < 1e-9);
        assert_eq!(label.from, label.mark[2]);
        assert!(lay_out(&d, &[Seen::Point([20.0, 0.0])], &frame, None, &[]).is_ok());
        // On an upright edge the base stands upright.
        let upright = datum_label(
            tip,
            Some([0.0, 1.0]),
            [140.0, 100.0],
            DIMENSION_TEXT,
            triangle,
        );
        assert!((upright.mark[0][0] - tip[0]).abs() < 1e-9);
        assert!((upright.mark[1][0] - tip[0]).abs() < 1e-9);
        assert!((upright.mark[2][0] - tip[0] - DATUM_HEIGHT).abs() < 1e-9);
        // A circle seen end on: the base along its tangent at the tip.
        let round = circle([0.0, 0.0], 10.0);
        let t = round.along([0.0, 30.0]).unwrap();
        assert!(
            t[1].abs() < 1e-9 && (t[0].abs() - 1.0).abs() < 1e-9,
            "{t:?}"
        );
    }

    #[test]
    fn a_frames_leader_leaves_the_side_that_faces_its_tip() {
        // mitcad#1294: a frame on a horizontal edge, its arrow at the
        // sheet's (120, 100).
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let edge = Seen::Line([0.0, 0.0], [40.0, 0.0]);
        let tip = [120.0, 100.0];
        let cells = ["".to_owned(), "0.05".to_owned(), "A".to_owned()];
        // Its box left of the tip, right of it, above and below it.
        for shift in [
            [-40.0, 5.0],
            [-40.0, -5.0],
            [10.0, 8.0],
            [-10.0, 15.0],
            [-10.0, -15.0],
        ] {
            let mut f = ViewAnnotation::new(1, AnnotationType::Frame, Vec::new());
            f.at = Some([20.0, 0.0]);
            f.text_offset = Some(shift);
            let laid = lay_out(&f, &[edge], &frame, None, &cells).unwrap();
            let (min, max) = first_box(&laid);
            let leader = lines(&laid)
                .into_iter()
                .find(|(_, b)| len2(sub2(*b, tip)) < 1e-9)
                .unwrap_or_else(|| panic!("{shift:?}: no leader to the tip"));
            assert!(on_outline(leader.0, min, max), "{shift:?}: {leader:?}");
            assert!(
                !crosses(leader.0, leader.1, min, max),
                "{shift:?}: through the frame"
            );
            assert_eq!(
                leader_line(&f, &[edge], &frame, &cells),
                Some((tip, leader.0)),
                "{shift:?}"
            );
            if shift[0] < -30.0 {
                // Left of the tip: from its right end's middle.
                assert!((leader.0[0] - max[0]).abs() < 1e-9, "{shift:?}: {leader:?}");
            }
        }
    }

    #[test]
    fn a_leaders_drag_snaps_to_multiples_of_45_degrees() {
        // mitcad#1294: a datum's square, a note's text and a frame dragged
        // near 45 degrees: their leaders run at 45, as long as dragged.
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let edge = Seen::Line([0.0, 0.0], [40.0, 0.0]);
        let cells = ["".to_owned(), "0.05".to_owned()];
        let angle_of = |a: &ViewAnnotation| {
            let (from, to) = leader_line(a, &[edge], &frame, &cells).unwrap();
            let run = sub2(to, from);
            (run[1].atan2(run[0]).to_degrees(), len2(run))
        };
        for kind in [
            AnnotationType::Datum,
            AnnotationType::Note,
            AnnotationType::Frame,
        ] {
            let mut a = ViewAnnotation::new(1, kind, Vec::new());
            a.at = Some([20.0, 0.0]);
            a.letter = Some("A".to_owned());
            a.text = Some("DEBURR".to_owned());
            // A drag that puts the leader's end `degrees` from where it
            // starts, 20 mm away (the end may move to another side of a
            // square or a frame on the way: found again a few times).
            let r = 20.0;
            let drag = |degrees: f64| {
                let at = degrees.to_radians();
                let mut delta = [0.0, 0.0];
                for _ in 0..4 {
                    let mut placed = a.clone();
                    moved(&mut placed, &[edge], &frame, delta);
                    let (from, to) = leader_line(&placed, &[edge], &frame, &cells).unwrap();
                    let want = add2(from, [r * at.cos(), r * at.sin()]);
                    delta = add2(delta, sub2(want, to));
                }
                let mut placed = a.clone();
                moved(&mut placed, &[edge], &frame, delta);
                let (angle, length) = angle_of(&placed);
                assert!(
                    (angle - degrees).abs() < 1e-6,
                    "{kind:?}: {angle} for {degrees}"
                );
                assert!((length - r).abs() < 1e-6, "{kind:?}: {length}");
                delta
            };
            let snap = snap_leader(&a, &[edge], &frame, &cells, drag(47.0))
                .unwrap_or_else(|| panic!("{kind:?}: not snapped"));
            assert!((snap.angle - 45.0).abs() < 1e-9, "{kind:?}: {snap:?}");
            let mut placed = a.clone();
            moved(&mut placed, &[edge], &frame, snap.to);
            let (angle, length) = angle_of(&placed);
            assert!((angle - 45.0).abs() < 1e-6, "{kind:?}: {angle}");
            assert!((length - r).abs() < 1e-6, "{kind:?}: {length}");
            // Ten degrees off: where dragged.
            assert_eq!(
                snap_leader(&a, &[edge], &frame, &cells, drag(55.0)),
                None,
                "{kind:?}"
            );
            // Near straight up, and near straight left (180).
            for (degrees, expected) in [(92.0_f64, 90.0), (177.0, 180.0)] {
                let snap = snap_leader(&a, &[edge], &frame, &cells, drag(degrees))
                    .unwrap_or_else(|| panic!("{kind:?} {degrees}: not snapped"));
                assert!((snap.angle - expected).abs() < 1e-9, "{kind:?}: {snap:?}");
            }
        }
        // What has no such leader: no snap.
        let balloon = ViewAnnotation::new(2, AnnotationType::Balloon, Vec::new());
        assert_eq!(
            snap_leader(&balloon, &[edge], &frame, &[], [1.0, 1.0]),
            None
        );
    }

    /// Where a datum's triangle sits as laid out: its base's middle.
    fn datum_tip(d: &ViewAnnotation, seen: Seen, frame: &Frame<'_>) -> P2 {
        let laid = lay_out(d, &[seen], frame, None, &[]).unwrap();
        let triangle = laid
            .group
            .items
            .iter()
            .find_map(|i| match i {
                Item::Fill { points } => Some(points.clone()),
                _ => None,
            })
            .unwrap();
        mid(triangle[0], triangle[1])
    }

    #[test]
    fn a_datums_tip_keeps_its_place_along_its_edge_and_slides_along_it() {
        // mitcad#1295: a datum clicked above a quarter of a 40 mm edge of
        // the view (the sheet's (100, 100) its start).
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let short = Seen::Line([0.0, 0.0], [40.0, 0.0]);
        let mut d = ViewAnnotation::new(1, AnnotationType::Datum, Vec::new());
        d.letter = Some("A".to_owned());
        d.at = Some([10.0, 3.0]);
        keep_tip_along(&mut d, &[short]);
        assert!((d.tip_along.unwrap() - 0.25).abs() < 1e-12, "{d:?}");
        let pick = ViewPick {
            body: "F2.b0".parse().unwrap(),
            occurrence: None,
            edge: Some("E{F2:start|F2:side(c1[c4,c2])}".parse().unwrap()),
            face: None,
            point: None,
            on_face: None,
        };
        let picked = |a: &ViewAnnotation| {
            let mut a = a.clone();
            a.picks = vec![pick.clone()];
            a.check()
        };
        picked(&d).unwrap();
        assert!(len2(sub2(datum_tip(&d, short, &frame), [110.0, 100.0])) < 1e-9);
        // The edge made twice as long in the model: still a quarter along
        // it; a label without the place (an older file) keeps the point
        // nearest where it was clicked.
        let long = Seen::Line([0.0, 0.0], [80.0, 0.0]);
        let tip = datum_tip(&d, long, &frame);
        assert!(len2(sub2(tip, [120.0, 100.0])) < 1e-9, "{tip:?}");
        let mut older = d.clone();
        older.tip_along = None;
        let tip = datum_tip(&older, long, &frame);
        assert!(len2(sub2(tip, [110.0, 100.0])) < 1e-9, "{tip:?}");
        // The same edge the other way round: a quarter from its own start.
        let reversed = Seen::Line([80.0, 0.0], [0.0, 0.0]);
        let tip = datum_tip(&d, reversed, &frame);
        assert!(len2(sub2(tip, [160.0, 100.0])) < 1e-9, "{tip:?}");

        // Its triangle dragged 15 along the edge and 4 off it: the tip
        // slides along it, the square follows (its offset kept).
        let laid = lay_out(&d, &[long], &frame, None, &[]).unwrap();
        let square_before = first_box(&laid);
        slid(&mut d, &[long], &frame, [15.0, 4.0]);
        assert!((d.tip_along.unwrap() - 35.0 / 80.0).abs() < 1e-12, "{d:?}");
        assert_eq!(d.text_offset, None);
        let tip = datum_tip(&d, long, &frame);
        assert!(len2(sub2(tip, [135.0, 100.0])) < 1e-9, "{tip:?}");
        let laid = lay_out(&d, &[long], &frame, None, &[]).unwrap();
        let square_after = first_box(&laid);
        assert!(len2(sub2(sub2(square_after.0, square_before.0), [15.0, 0.0])) < 1e-9);
        // Past its end: on the edge's line beyond it (mitcad#1296; a
        // frame's arrow stops at the end, below).
        slid(&mut d, &[long], &frame, [55.0, -3.0]);
        assert!((d.tip_along.unwrap() - 1.125).abs() < 1e-12, "{d:?}");
        let tip = datum_tip(&d, long, &frame);
        assert!(len2(sub2(tip, [190.0, 100.0])) < 1e-9, "{tip:?}");
        slid(&mut d, &[long], &frame, [-10.0, 0.0]);
        assert!((d.tip_along.unwrap() - 1.0).abs() < 1e-12, "{d:?}");

        // A frame's arrow slides the same way; a note has no such tip.
        let cells = ["".to_owned(), "0.05".to_owned()];
        let mut f = ViewAnnotation::new(2, AnnotationType::Frame, Vec::new());
        f.at = Some([10.0, 3.0]);
        keep_tip_along(&mut f, &[long]);
        assert!((f.tip_along.unwrap() - 0.125).abs() < 1e-12);
        slid(&mut f, &[long], &frame, [-20.0, 0.0]);
        assert_eq!(f.tip_along, Some(0.0));
        let (arrow, _) = leader_line(&f, &[long], &frame, &cells).unwrap();
        assert!(len2(sub2(arrow, [100.0, 100.0])) < 1e-9, "{arrow:?}");
        let mut n = ViewAnnotation::new(3, AnnotationType::Note, Vec::new());
        n.text = Some("DEBURR".to_owned());
        keep_tip_along(&mut n, &[long]);
        slid(&mut n, &[long], &frame, [5.0, 0.0]);
        assert_eq!((n.tip_along, n.at), (None, None));
        n.tip_along = Some(0.5);
        assert!(picked(&n).unwrap_err().contains("no place along its edge"));
        // A datum may sit beyond its edge's ends, a frame's arrow not.
        d.tip_along = Some(1.5);
        picked(&d).unwrap();
        // (Checked before its missing tolerance.)
        let mut f = f.clone();
        f.tip_along = Some(1.5);
        assert!(picked(&f).unwrap_err().contains("from 0 to 1"));
        d.terminator = Some(Terminator::Dot);
        picked(&d).unwrap();
        n.tip_along = None;
        n.terminator = Some(Terminator::Dot);
        assert!(picked(&n).unwrap_err().contains("no terminator"));
    }

    #[test]
    fn places_along_arcs_round_faces_and_circles_seen_from_the_side() {
        // mitcad#1295: what `tip_along` measures on each kind of pick.
        let axis = Seen::Axis {
            a: [0.0, 0.0],
            b: [0.0, 30.0],
            radius: 4.0,
        };
        let t = axis.fraction([5.0, 10.0]).unwrap();
        assert!((t - 1.0 / 3.0).abs() < 1e-12, "{t}");
        let p = axis.at_fraction(t, [5.0, 10.0]).unwrap();
        assert!(len2(sub2(p, [4.0, 10.0])) < 1e-9, "{p:?}");
        let p = axis.at_fraction(t, [-5.0, 10.0]).unwrap();
        assert!(
            len2(sub2(p, [-4.0, 10.0])) < 1e-9,
            "the other outline: {p:?}"
        );
        let quarter = Seen::Circle {
            center: [0.0, 0.0],
            radius: 10.0,
            end_on: true,
            across: [1.0, 0.0],
            arc: Some((0.0, PI / 2.0)),
        };
        let t = quarter.fraction([10.0, 10.0]).unwrap();
        assert!((t - 0.5).abs() < 1e-12, "{t}");
        let p = quarter.at_fraction(t, [0.0, 0.0]).unwrap();
        assert!(len2(sub2(p, [10.0 * FRAC_1_SQRT_2, 10.0 * FRAC_1_SQRT_2])) < 1e-9);
        // Off the arc beyond its first end: its first end.
        assert_eq!(quarter.fraction([10.0, -5.0]), Some(0.0));
        let side = Seen::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
            end_on: false,
            across: [1.0, 0.0],
            arc: None,
        };
        let t = side.fraction([2.5, 3.0]).unwrap();
        assert!((t - 0.75).abs() < 1e-12, "{t}");
        assert!(len2(sub2(side.at_fraction(t, [0.0, 0.0]).unwrap(), [2.5, 0.0])) < 1e-9);
        // A whole circle seen end on and a point have no place along them.
        assert_eq!(circle([0.0, 0.0], 5.0).fraction([5.0, 0.0]), None);
        assert_eq!(Seen::Point([1.0, 1.0]).fraction([0.0, 0.0]), None);
    }

    #[test]
    fn a_datum_past_its_edges_end_draws_its_own_extension_line() {
        // mitcad#1296: a datum on a 40 mm edge of the view (the sheet's
        // (100, 100) to (140, 100)), its tip a quarter of the edge past
        // its end.
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let pick = ViewPick {
            body: "F2.b0".parse().unwrap(),
            occurrence: None,
            edge: Some("E{F2:start|F2:side(c1[c4,c2])}".parse().unwrap()),
            face: None,
            point: None,
            on_face: None,
        };
        let mut d = ViewAnnotation::new(1, AnnotationType::Datum, vec![pick.clone()]);
        d.letter = Some("A".to_owned());
        d.tip_along = Some(1.25);
        d.check().unwrap();
        let edge = Seen::Line([0.0, 0.0], [40.0, 0.0]);
        let extension = |d: &ViewAnnotation, seen: Seen| {
            let laid = lay_out(d, &[seen], &frame, None, &[]).unwrap();
            // The leader rises from the apex; the extension runs along
            // the edge's line.
            lines(&laid)
                .into_iter()
                .find(|(a, b)| (a[1] - 100.0).abs() < 1e-9 && (b[1] - 100.0).abs() < 1e-9)
        };
        assert_eq!(
            terminator_points(&d, edge, &frame),
            3,
            "a triangle on an edge"
        );
        let tip = datum_tip(&d, edge, &frame);
        assert!(len2(sub2(tip, [150.0, 100.0])) < 1e-9, "{tip:?}");
        // From 1 mm off the edge's end to 2 mm past the tip.
        let (from, to) = extension(&d, edge).expect("an extension line");
        assert!(len2(sub2(from, [141.0, 100.0])) < 1e-9, "{from:?}");
        assert!(len2(sub2(to, [152.0, 100.0])) < 1e-9, "{to:?}");
        // Still the edge's annotation: its pick kept.
        assert_eq!(d.picks, vec![pick.clone()]);
        // The edge twice as long in the model: a quarter past its new end.
        let long = Seen::Line([0.0, 0.0], [80.0, 0.0]);
        let tip = datum_tip(&d, long, &frame);
        assert!(len2(sub2(tip, [200.0, 100.0])) < 1e-9, "{tip:?}");
        let (from, to) = extension(&d, long).expect("an extension line");
        assert!(len2(sub2(from, [181.0, 100.0])) < 1e-9, "{from:?}");
        assert!(len2(sub2(to, [202.0, 100.0])) < 1e-9, "{to:?}");
        // Before the edge's start likewise; on the edge none.
        d.tip_along = Some(-0.25);
        let (from, to) = extension(&d, edge).expect("an extension line");
        assert!(len2(sub2(from, [99.0, 100.0])) < 1e-9, "{from:?}");
        assert!(len2(sub2(to, [88.0, 100.0])) < 1e-9, "{to:?}");
        d.tip_along = Some(0.5);
        assert_eq!(extension(&d, edge), None);
        // A triangle dragged past the end goes onto the line; dragged
        // back, onto the edge.
        slid(&mut d, &[edge], &frame, [30.0, 2.0]);
        assert!((d.tip_along.unwrap() - 1.25).abs() < 1e-12, "{d:?}");
        assert!(extension(&d, edge).is_some());
        slid(&mut d, &[edge], &frame, [-20.0, 0.0]);
        assert!((d.tip_along.unwrap() - 0.75).abs() < 1e-12, "{d:?}");
        assert_eq!(extension(&d, edge), None);
    }

    /// How many corners a datum's filled terminator has: 3 for a triangle,
    /// a dot's outline more.
    fn terminator_points(d: &ViewAnnotation, seen: Seen, frame: &Frame<'_>) -> usize {
        let laid = lay_out(d, &[seen], frame, None, &[]).unwrap();
        laid.group
            .items
            .iter()
            .find_map(|i| match i {
                Item::Fill { points } => Some(points.len()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_datum_on_a_face_ends_in_a_dot_and_its_terminator_is_chosen() {
        // mitcad#1296: a datum on a planar face picked face on: its pick a
        // point of the face, seen as a point; a dot about it, the leader
        // from its middle to the square's near side.
        let map = map();
        let frame = Frame {
            map: &map,
            view_box: view_box(),
            text: DIMENSION_TEXT,
        };
        let face = ViewPick {
            body: "F2.b0".parse().unwrap(),
            occurrence: None,
            edge: None,
            face: Some("F2:side(c1[c4,c2])".parse().unwrap()),
            point: None,
            on_face: Some([12.0, 5.0]),
        };
        let mut d = ViewAnnotation::new(1, AnnotationType::Datum, vec![face.clone()]);
        d.letter = Some("B".to_owned());
        d.check().unwrap();
        assert_eq!(d.terminator(), Terminator::Dot);
        let seen = Seen::Point([12.0, 5.0]);
        let laid = lay_out(&d, &[seen], &frame, None, &[]).unwrap();
        let dot = laid
            .group
            .items
            .iter()
            .find_map(|i| match i {
                Item::Fill { points } => Some(points.clone()),
                _ => None,
            })
            .unwrap();
        assert!(dot.len() > 3, "a dot: {dot:?}");
        for p in &dot {
            assert!(
                (len2(sub2(*p, [112.0, 105.0])) - DATUM_DOT).abs() < 1e-9,
                "{p:?}"
            );
        }
        let leaders = lines(&laid);
        assert_eq!(leaders.len(), 1, "{leaders:?}");
        assert!(
            len2(sub2(leaders[0].0, [112.0, 105.0])) < 1e-9,
            "{leaders:?}"
        );
        let (min, max) = first_box(&laid);
        assert!(on_outline(leaders[0].1, min, max));
        assert!(!crosses(leaders[0].0, leaders[0].1, min, max));
        // Its terminator chosen: a triangle on the face, a dot on an edge.
        d.terminator = Some(Terminator::Triangle);
        assert_eq!(terminator_points(&d, seen, &frame), 3);
        let mut e = ViewAnnotation::new(2, AnnotationType::Datum, Vec::new());
        e.letter = Some("C".to_owned());
        let edge = Seen::Line([0.0, 0.0], [40.0, 0.0]);
        assert_eq!(e.terminator(), Terminator::Triangle);
        e.terminator = Some(Terminator::Dot);
        assert!(terminator_points(&e, edge, &frame) > 3);
        // A point on a face is a pick of the face alone.
        let mut wrong = face;
        wrong.edge = Some("E{F2:start|F2:side(c1[c4,c2])}".parse().unwrap());
        let mut w = ViewAnnotation::new(3, AnnotationType::Datum, vec![wrong]);
        w.letter = Some("D".to_owned());
        assert!(w.check().unwrap_err().contains("of the face alone"));
        // The face's own axes: square to each other and to its normal.
        for normal in [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.6, 0.8]] {
            let (x, y) = face_axes(normal);
            for (a, b) in [(x, y), (x, normal), (y, normal)] {
                assert!(mitcad_vec3::dot(a, b).abs() < 1e-12, "{normal:?}");
            }
            assert!((mitcad_vec3::norm(x) - 1.0).abs() < 1e-12);
            assert!((mitcad_vec3::norm(y) - 1.0).abs() < 1e-12);
        }
    }
}
