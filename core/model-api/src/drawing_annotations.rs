// SPDX-License-Identifier: MIT
//! Annotations on drawings' views (mitcad#707 phase 2,
//! `mitcad_model_drawing::annotate`; commands.md, "Annotations on drawing
//! views"): the commands `add_drawing_annotation`,
//! `edit_drawing_annotation` and `delete_drawing_annotation`, the
//! `drawing_pick` query, and the annotations worked out when a sheet is
//! drawn.
//!
//! A pick names model topology through its view: a body the view shows
//! (with the occurrence that places it) and one of its edges or faces by
//! its topological name. A click on a sheet is turned into such a pick
//! here: the kernel lists the body's named edges, round faces and planar
//! faces (`Kernel::edge_geometries`, `Kernel::face_geometries`, kept per
//! shape version in the projection cache), each is projected as the view
//! sees it, and the one nearest the click is taken. A view sees what it
//! shows (mitcad#717): a section what lies behind its plane (and a planar
//! face where its plane cuts it, the piece of its cut line under the click,
//! mitcad#1361), a detail what lies in its circle, a
//! broken view what lies beside its bands; of picks as near as one
//! another, one the view draws as a visible line goes first, then the one
//! nearest the eye. When the sheet is drawn each pick is looked up again
//! by its name (`Kernel::edge_geometry`, `Kernel::face_geometry`), so the
//! annotation follows the model; a pick that is gone makes it broken.
//!
//! Values are written by #702's engine (`annotations.rs`): the same
//! prefixes, decimals, own texts, tolerances and fit codes as the
//! dimensions of the model. On a pictorial view (an isometric one) a
//! length seen along the view's x or y, an ordinate and an angle would be
//! foreshortened: they are refused there, and an aligned dimension
//! measures the true distance of its picks in 3D.

use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::annotations::{
    FormatEdit, ModelItem, ModelShape, Units as WordUnits, frame_words, present, value_text,
};
use super::drawing_tables::{PartsList, RowKey, TableEdit, row_of};
use super::drawings::{Evaluated, Shown};
use super::prelude::*;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry, Vec3};
use mitcad_model::features::OccurrencePath;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::internal::drawing::DIMENSION_TEXT;
use mitcad_model::internal::fingerprint::Fingerprint;
use mitcad_model::kernel::{Curve3, Kernel, KernelError};
use mitcad_model::notice::Notice;
use mitcad_model::topo::{EdgeName, FaceName};
use mitcad_model::transform::Transform;
use mitcad_model::{
    AnnotationFormat, Document, GeometricTolerance, MeasureKind, MeasurePick, Tolerance,
};
use mitcad_model_drawing::annotate::{
    self, Along, AnnotationType, BodyGeometry, FlatFace, Frame, PenPattern, PenWidth, PickPoint,
    RoundFace, Seen, SheetNote, SketchItem, SketchPen, SketchPenName, SurfaceFinish, Terminator,
    ViewAnnotation, ViewPick, Want,
};
use mitcad_model_drawing::clip::ViewMap;
use mitcad_model_drawing::compose::{Bounds, PlacedView, SketchDimension, ViewRole, view_bounds};
use mitcad_model_drawing::def::{
    Drawing, DrawingDefaults, DrawingSheet, DrawingUid, Subject, ViewKind,
};
use mitcad_model_drawing::internal::BodyInView;
use mitcad_model_drawing::internal::sketch_dimension;
use mitcad_model_drawing::sheet::{Group, Item};
use mitcad_model_drawing::tables::{
    HOLE_COLUMNS, Levels, PARTS_COLUMNS, TableSettings, header, lay_out_table,
};
use mitcad_model_drawing::{CutFace, CutPlane, P2, Units, ViewCurve, ViewSpec, ViewSpline, clip};

/// A pick as a command gives it: model topology, or a point on the sheet
/// (mm) where the nearest pick of the view is found.
// Read from a command and gone with it: one value, so a boxed pick would
// save nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum PickInput {
    Pick(ViewPick),
    Point(P2),
}

/// Linear dimensions made at once: from the first pick to each other one
/// (baseline, stacked) or from each to the next (chain, in a row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Series {
    Baseline,
    Chain,
}

/// The paper between the dimension lines of a baseline series, mm.
const BASELINE_STEP: f64 = 7.0;
/// How far a model point may lie in front of a section's plane and still
/// be seen, mm.
const CUT_TOLERANCE: f64 = 1e-6;
/// How far a section's cut edge may lie from a planar face to be its cut,
/// mm.
const PLANE_TOLERANCE: f64 = 1e-3;
/// Picks within this of the nearest (paper mm) are as near as it: the one
/// drawn as a visible line goes first, then the one nearest the eye.
const PICK_TIE: f64 = 0.25;
/// How near a visible line of the view a pick's point must lie to count as
/// seen on it, paper mm.
const VISIBLE_NEAR: f64 = 0.05;
/// A parts list's balloons (mitcad#725): their column's distance right
/// of the view's box and the room of each, paper mm.
const BALLOON_COLUMN: f64 = 15.0;
const BALLOON_ROW: f64 = 12.0;

/// `add_drawing_annotation`: an annotation on a view, or (`"type":
/// "text"`) a text note on a sheet.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddAnnotationCommand {
    pub(super) drawing: String,
    #[serde(default)]
    pub(super) view: Option<u32>,
    /// A text note's sheet.
    #[serde(default)]
    sheet: usize,
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(default)]
    picks: Vec<PickInput>,
    #[serde(default)]
    along: Option<Along>,
    /// Where its dimension line or text goes on the sheet; a text note's
    /// place.
    #[serde(default)]
    pub(super) at: Option<P2>,
    #[serde(default)]
    format: Option<AnnotationFormat>,
    #[serde(default)]
    tolerance: Option<Tolerance>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    letter: Option<String>,
    #[serde(default)]
    frame: Option<GeometricTolerance>,
    #[serde(default)]
    finish: Option<SurfaceFinish>,
    /// Its letters' height, mm (a text note's, mitcad#707; every
    /// annotation's own, mitcad#729).
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    series: Option<Series>,
    /// A hole table's or a parts list's settings (`at`: its place).
    #[serde(default)]
    table: Option<TableEdit>,
    /// A parts list's balloons (mitcad#725): one on each row's bodies the
    /// view shows, as the Balloon tool places them.
    #[serde(default)]
    balloons: bool,
    /// A line's two ends, an arc's centre, start and end, on the sheet
    /// (mm; mitcad#729).
    #[serde(default)]
    points: Vec<P2>,
    /// A line's or an arc's pen by its name of before mitcad#1447 (its
    /// width and pattern).
    #[serde(default)]
    pen: Option<SketchPenName>,
    /// A line's or an arc's width and pattern (mitcad#1447), over `pen`.
    #[serde(default)]
    width: Option<PenWidth>,
    #[serde(default)]
    pattern: Option<PenPattern>,
    /// A datum label's terminator (mitcad#1296); left out: by its pick.
    #[serde(default)]
    terminator: Option<Terminator>,
}

impl AddAnnotationCommand {
    /// Whether a line's or an arc's pen is given: `pen`, `width` or
    /// `pattern`.
    fn has_pen(&self) -> bool {
        self.pen.is_some() || self.width.is_some() || self.pattern.is_some()
    }
}

/// `edit_drawing_annotation`: what is given changes; `null` sets a field
/// back to its default.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditAnnotationCommand {
    pub(super) drawing: String,
    pub(super) annotation: u32,
    /// A drag on the sheet: its line or text moves.
    #[serde(default, rename = "move")]
    pub(super) shift: Option<P2>,
    /// A drag of a datum label's triangle or a frame's arrow on the sheet
    /// (mitcad#1295): its tip slides along its pick, the square or the
    /// frame following.
    #[serde(default)]
    slide: Option<P2>,
    /// Its dimension line or text through this sheet point; a text note's
    /// new place.
    #[serde(default)]
    at: Option<P2>,
    #[serde(default)]
    picks: Option<Vec<PickInput>>,
    #[serde(default)]
    along: Option<Along>,
    #[serde(default)]
    format: Option<FormatEdit>,
    #[serde(default, deserialize_with = "present")]
    tolerance: Option<Option<Tolerance>>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    letter: Option<String>,
    #[serde(default)]
    frame: Option<GeometricTolerance>,
    #[serde(default, deserialize_with = "present")]
    finish: Option<Option<SurfaceFinish>>,
    #[serde(default, deserialize_with = "present")]
    line_offset: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    text_offset: Option<Option<P2>>,
    /// Its letters' height, mm; `null`: the drawing's (mitcad#729).
    #[serde(default, deserialize_with = "present")]
    height: Option<Option<f64>>,
    #[serde(default)]
    table: Option<TableEdit>,
    /// A line's or an arc's points on the sheet, as when it was added
    /// (mitcad#729).
    #[serde(default)]
    points: Option<Vec<P2>>,
    #[serde(default)]
    pen: Option<SketchPenName>,
    /// A line's or an arc's width and pattern (mitcad#1447): each changes
    /// alone, over `pen`.
    #[serde(default)]
    width: Option<PenWidth>,
    #[serde(default)]
    pattern: Option<PenPattern>,
    /// A datum label's terminator (mitcad#1296); `null`: by its pick.
    #[serde(default, deserialize_with = "present")]
    terminator: Option<Option<Terminator>>,
}

impl EditAnnotationCommand {
    /// Whether a line's or an arc's pen is given: `pen`, `width` or
    /// `pattern`.
    fn has_pen(&self) -> bool {
        self.pen.is_some() || self.width.is_some() || self.pattern.is_some()
    }
}

/// The `drawing_pick` query: what a click at a sheet point (mm) picks on
/// a view (the one given, else the smallest drawn view there), for an
/// annotation of a type or what is wanted.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PickQuery {
    drawing: String,
    #[serde(default)]
    sheet: usize,
    #[serde(default)]
    view: Option<u32>,
    at: P2,
    #[serde(default, rename = "type")]
    kind: Option<AnnotationType>,
    #[serde(default)]
    want: Option<Want>,
}

/// The `drawing_corners` query (mitcad#1338): the corners each drawn view
/// of a sheet shows, sheet mm.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CornersQuery {
    drawing: String,
    #[serde(default)]
    sheet: usize,
}

/// `drawing_model_items` (mitcad#1406), the Drawing tab's Model Items: the
/// views given of a sheet (all its views when none are) retrieve the
/// sketch dimensions (`sketch_dimensions`) and the model's annotations
/// (`model_annotations`) they see square on; each `true` (the default)
/// switches its own on, `false` leaves it as it is. One undo step.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelItemsCommand {
    drawing: String,
    #[serde(default)]
    sheet: usize,
    #[serde(default)]
    views: Option<Vec<u32>>,
    #[serde(default)]
    sketch_dimensions: Option<bool>,
    #[serde(default)]
    model_annotations: Option<bool>,
}

/// `keep_drawing_model_items` (mitcad#1406): the model's annotations a view
/// draws (`names`; all of them when left out) kept as the drawing's own
/// annotations of that view, to be moved and edited without changing the
/// model. One undo step.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KeepModelItemsCommand {
    drawing: String,
    view: u32,
    #[serde(default)]
    names: Option<Vec<String>>,
}

/// A sheet's annotations worked out: their groups to draw, their entries
/// for the `drawing` query, why those that are broken are, the sketch
/// dimensions its views retrieved (indices into the list given), and the
/// model's annotations its views brought in (mitcad#1406: `view`, `name`,
/// `kind`, `label`, `box`).
pub(super) struct Annotated {
    pub(super) groups: Vec<Group>,
    pub(super) entries: Vec<Value>,
    pub(super) warnings: Vec<String>,
    pub(super) retrieved: Vec<usize>,
    pub(super) model_items: Vec<Value>,
}

/// How a view sees a pick: in its plane, and the model point (the
/// drawing's coordinates) that stands for it there (its point, a line's
/// middle, a circle's centre, a round face's axis' middle).
#[derive(Debug, Clone, Copy)]
struct Found {
    seen: Seen,
    at: Vec3,
}

/// A pick a click can find and how its view sees it.
#[derive(Debug, Clone)]
struct Candidate {
    pick: ViewPick,
    found: Found,
    /// A centre's pick made on the line of its centre mark (mitcad#1338):
    /// that line (view mm), shown under the pointer with the centre.
    line: Option<[P2; 2]>,
}

/// A view drawn, with what its annotations need: its index among the
/// sheet's drawn views and the bodies it shows.
pub(super) struct ViewAt<'a> {
    pub(super) component: ComponentUid,
    pub(super) placed: &'a PlacedView,
    pub(super) shown: Vec<Shown>,
    /// A detail's circle (view mm): picks outside it are not seen.
    pub(super) circle: Option<(P2, f64)>,
    /// It looks at the model from an angle (an isometric view, a detail of
    /// one): its lengths along x and y are foreshortened.
    pictorial: bool,
    /// Along no axis and not a section, whatever faces it looks square at:
    /// an aligned dimension measures its picks' true distance in 3D, as it
    /// always has there (mitcad#722).
    angled: bool,
    /// On a view along a given direction, the bodies (with their
    /// placements) that have a planar face it looks square at
    /// (mitcad#718): their annotations take the view as orthographic.
    square: Vec<(BodyUid, Vec<OccurrenceUid>)>,
    /// How many occurrences below the drawing's component its subject lies
    /// (an occurrence's path; 0 for the design or chosen bodies).
    pub(super) subject_depth: usize,
    /// Whether it draws its automatic centre marks, and the picks of its
    /// centre mark annotations (mitcad#1338: a click on a mark's line
    /// picks its circle's centre).
    center_marks: bool,
    marked: Vec<ViewPick>,
}

impl ViewAt<'_> {
    /// Whether an annotation of these picks sees the model from an angle:
    /// the view does, unless it looks square at a planar face of each
    /// pick's body (mitcad#722: not of any body it shows).
    fn pictorial_for(&self, picks: &[ViewPick]) -> bool {
        if !self.angled {
            return false;
        }
        if picks.is_empty() {
            return self.pictorial;
        }
        !picks.iter().all(|p| {
            self.square
                .iter()
                .any(|(body, path)| *body == p.body && path.as_slice() == p.path())
        })
    }

    /// Whether an annotation takes its picks as points and its value as
    /// their true distance in 3D: on a view from an angle for its picks,
    /// and an aligned dimension on any view along no axis (one square to a
    /// face too, so that files' aligned dimensions keep their values,
    /// mitcad#722).
    fn true_distance(
        &self,
        kind: AnnotationType,
        along: Option<Along>,
        picks: &[ViewPick],
    ) -> bool {
        self.pictorial_for(picks)
            || (self.angled && kind == AnnotationType::Linear && along == Some(Along::Aligned))
    }

    pub(super) fn frame(&self) -> Frame<'_> {
        self.frame_with(DIMENSION_TEXT)
    }

    /// Its frame for letters `text` high (mitcad#729).
    pub(super) fn frame_with(&self, text: f64) -> Frame<'_> {
        Frame {
            map: &self.placed.map,
            view_box: self.placed.sheet_box(),
            text,
        }
    }

    /// Whether a view point is seen: inside a detail's circle, outside a
    /// break's band.
    pub(super) fn sees(&self, p: P2) -> bool {
        let inside = self.circle.is_none_or(|(c, r)| {
            let d = [p[0] - c[0], p[1] - c[1]];
            (d[0] * d[0] + d[1] * d[1]).sqrt() <= r + 1e-9
        });
        inside && !self.placed.map.in_break(p)
    }

    /// A section view's plane.
    fn cut(&self) -> Option<CutPlane> {
        self.placed.spec.section
    }

    /// Whether a model point is not taken away by the section's plane.
    pub(super) fn kept(&self, p: Vec3) -> bool {
        self.cut().is_none_or(|c| side(&c, p) <= CUT_TOLERANCE)
    }

    /// Whether a view point lies on a line the view draws as visible (its
    /// visible edges and the outlines of its cut faces).
    fn on_visible(&self, p: P2) -> bool {
        self.on_visible_within(p, VISIBLE_NEAR / self.placed.map.scale)
    }

    /// Whether a view point lies within `near` (view mm) of a line the
    /// view draws as visible.
    fn on_visible_within(&self, p: P2, near: f64) -> bool {
        let projection = &self.placed.projection;
        let on_edge = projection
            .edges
            .iter()
            .filter(|e| e.visible)
            .any(|e| curve_distance(&e.curve, p) <= near);
        on_edge
            || projection.cuts.iter().flatten().any(|lp| {
                (0..lp.len())
                    .any(|i| annotate::segment_distance(p, lp[i], lp[(i + 1) % lp.len()]) <= near)
            })
    }

    /// Whether the view draws a round feature's lines (mitcad#1286): at
    /// least half of `points` (view points along a circle seen end on, or
    /// along a round face's outlines seen from the side) lie on its visible
    /// lines, within a tenth of the feature's `radius` (a thread's crest,
    /// drawn by convention, is not quite the hole's own circle) or
    /// `VISIBLE_NEAR` on the sheet. A hole inside the part, which a view
    /// without hidden lines leaves out, is not drawn: where its outline
    /// crosses another line is not enough.
    fn draws(&self, points: &[P2], radius: f64) -> bool {
        let near = (VISIBLE_NEAR / self.placed.map.scale).max(0.1 * radius);
        let on = points
            .iter()
            .filter(|p| self.on_visible_within(**p, near))
            .count();
        !points.is_empty() && 2 * on >= points.len()
    }
}

/// How far a point is from a projected curve.
fn curve_distance(curve: &ViewCurve, p: P2) -> f64 {
    match curve {
        ViewCurve::Line(a, b) => annotate::segment_distance(p, *a, *b),
        ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => points
            .windows(2)
            .map(|w| annotate::segment_distance(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min),
        ViewCurve::Arc {
            center,
            radius,
            start,
            end,
        } => {
            let d = [p[0] - center[0], p[1] - center[1]];
            let angle = d[1].atan2(d[0]);
            if (angle - start).rem_euclid(TAU) <= end - start + 1e-12 {
                ((d[0] * d[0] + d[1] * d[1]).sqrt() - radius).abs()
            } else {
                let at = |a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
                let (s, e) = (at(*start), at(*end));
                ((p[0] - s[0]).hypot(p[1] - s[1])).min((p[0] - e[0]).hypot(p[1] - e[1]))
            }
        }
    }
}

/// How far a model point lies on the normal's side of a section's plane
/// (the side taken away).
fn side(cut: &CutPlane, p: Vec3) -> f64 {
    mitcad_vec3::dot(sub3(p, cut.origin), cut.normal)
}

/// A curve of a body moved by its placement into the drawing's
/// coordinates.
fn placed_curve(curve: &CurveGeometry, t: &Transform) -> CurveGeometry {
    match curve {
        CurveGeometry::Line { start, end } => CurveGeometry::Line {
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
        CurveGeometry::Circle {
            center,
            normal,
            radius,
            start,
            end,
        } => CurveGeometry::Circle {
            center: t.apply_point(*center),
            normal: t.apply_vector(*normal),
            radius: *radius,
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
        CurveGeometry::Other { kind, start, end } => CurveGeometry::Other {
            kind: kind.clone(),
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
    }
}

/// A sketch dimension of a component where a placement puts it: its
/// points and directions taken through `t` (mitcad#744).
pub(super) fn placed_dimension(dim: &SketchDimension, t: &Transform) -> SketchDimension {
    match dim {
        SketchDimension::Linear {
            a,
            b,
            along,
            text,
            value,
            normal,
        } => SketchDimension::Linear {
            a: t.apply_point(*a),
            b: t.apply_point(*b),
            along: along.map(|v| t.apply_vector(v)),
            text: text.map(|p| t.apply_point(p)),
            value: *value,
            normal: t.apply_vector(*normal),
        },
        SketchDimension::Radial {
            center,
            radius,
            diameter,
            text,
            value,
            normal,
        } => SketchDimension::Radial {
            center: t.apply_point(*center),
            radius: *radius,
            diameter: *diameter,
            text: text.map(|p| t.apply_point(p)),
            value: *value,
            normal: t.apply_vector(*normal),
        },
    }
}

/// One retrieved sketch dimension (in the drawing's coordinates) laid out
/// on a view, when the view sees it square on and shows where it is; its
/// value with the drawing's decimals (mitcad#729).
fn retrieved_dimension(
    view: &ViewAt<'_>,
    dim: &SketchDimension,
    units: WordUnits,
    decimals: Option<u8>,
) -> Option<Group> {
    let spec = &view.placed.spec;
    let (normal, points, kind, value) = match dim {
        SketchDimension::Linear {
            a,
            b,
            normal,
            value,
            ..
        } => (*normal, vec![*a, *b], MeasureKind::Distance, value.abs()),
        SketchDimension::Radial {
            center,
            normal,
            diameter,
            value,
            ..
        } => (
            *normal,
            vec![*center],
            if *diameter {
                MeasureKind::Diameter
            } else {
                MeasureKind::Radius
            },
            *value,
        ),
    };
    let square = mitcad_vec3::dot(spec.toward_eye, normal).abs();
    if square <= 1.0 - 1e-9
        || !points
            .iter()
            .all(|p| view.kept(*p) && view.sees(spec.project(*p)))
    {
        return None;
    }
    let format = AnnotationFormat {
        decimals,
        ..AnnotationFormat::default()
    };
    let (words, _) = value_text(kind, &format, None, value, units);
    let mut g = sketch_dimension(dim, spec, &view.placed.map, &words.text)?;
    g.class = "retrieved-dimension";
    Some(g)
}

fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mid3(a: Vec3, b: Vec3) -> Vec3 {
    [
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        (a[2] + b[2]) / 2.0,
    ]
}

fn ends(curve: &CurveGeometry) -> (Vec3, Vec3) {
    match curve {
        CurveGeometry::Line { start, end }
        | CurveGeometry::Circle { start, end, .. }
        | CurveGeometry::Other { start, end, .. } => (*start, *end),
    }
}

/// What a section's plane leaves of a curve: a straight edge cut where it
/// crosses the plane, a circle or another curve whole while some of it
/// lies behind the plane; none when all of it lies in front.
fn kept_curve(curve: &CurveGeometry, cut: &CutPlane) -> Option<CurveGeometry> {
    let kept = |p: Vec3| side(cut, p) <= CUT_TOLERANCE;
    match curve {
        CurveGeometry::Line { start, end } => {
            let (s, e) = (side(cut, *start), side(cut, *end));
            match (s <= CUT_TOLERANCE, e <= CUT_TOLERANCE) {
                (false, false) => None,
                (true, true) => Some(curve.clone()),
                (from_start, _) => {
                    let at = mitcad_vec3::add_scaled(*start, sub3(*end, *start), s / (s - e));
                    Some(if from_start {
                        CurveGeometry::Line {
                            start: *start,
                            end: at,
                        }
                    } else {
                        CurveGeometry::Line {
                            start: at,
                            end: *end,
                        }
                    })
                }
            }
        }
        CurveGeometry::Circle {
            center,
            normal,
            radius,
            start,
            end,
        } => annotate::arc_points3(*center, *normal, *radius, *start, *end)
            .into_iter()
            .any(kept)
            .then(|| curve.clone()),
        CurveGeometry::Other { start, end, .. } => {
            (kept(*start) || kept(*end)).then(|| curve.clone())
        }
    }
}

/// How a view sees an edge (in the drawing's coordinates) or a point of
/// it: a section's what lies behind its plane.
fn seen_edge(
    view: &ViewAt<'_>,
    curve: &CurveGeometry,
    point: Option<PickPoint>,
) -> Result<Found, String> {
    let spec = &view.placed.spec;
    let (start, end) = ends(curve);
    let curve = match view.cut() {
        Some(cut) => {
            let gone = match point {
                Some(PickPoint::Start) => !view.kept(start),
                Some(PickPoint::End) => !view.kept(end),
                _ => false,
            };
            if gone {
                return Err("the point lies in front of the section's plane".to_owned());
            }
            kept_curve(curve, &cut)
                .ok_or_else(|| "the edge lies in front of the section's plane".to_owned())?
        }
        None => curve.clone(),
    };
    let seen = annotate::seen_curve(&curve, point, |p| spec.project(p), spec.toward_eye)?;
    let at = match (point, &curve) {
        (Some(PickPoint::Start), _) => start,
        (Some(PickPoint::End), _) => end,
        (_, CurveGeometry::Circle { center, .. }) => *center,
        (_, CurveGeometry::Line { start, end } | CurveGeometry::Other { start, end, .. }) => {
            mid3(*start, *end)
        }
    };
    Ok(Found { seen, at })
}

/// How far a round face reaches along its axis (from its axis point): the
/// circles that bound it (its own edges, mitcad#717: not another face's
/// on the same axis) with its radius, by their centres; how far the widest
/// of them turns (2 pi for a whole circle) and that circle; none when it
/// has none.
fn round_extent(
    geometry: &BodyGeometry,
    face: &RoundFace,
) -> Option<(f64, f64, f64, CurveGeometry)> {
    let tolerance = 1e-6 * face.radius.max(1.0);
    let bounds = |edge: &EdgeName| {
        edge.faces()
            .iter()
            .any(|f| *f == face.face || face.names.contains(f))
    };
    let mut out: Option<(f64, f64, f64, CurveGeometry)> = None;
    for (name, curve) in &geometry.edges {
        let CurveGeometry::Circle {
            center,
            normal,
            radius: r,
            start,
            end,
        } = curve
        else {
            continue;
        };
        if !bounds(name) {
            continue;
        }
        let along = mitcad_vec3::dot(sub3(*center, face.origin), face.axis);
        let off = mitcad_vec3::norm(sub3(
            sub3(*center, face.origin),
            mitcad_vec3::scale(face.axis, along),
        ));
        let n = mitcad_vec3::unit(*normal).unwrap_or([0.0, 0.0, 1.0]);
        if off > tolerance
            || (r - face.radius).abs() > tolerance
            || mitcad_vec3::dot(n, face.axis).abs() < 1.0 - 1e-6
        {
            continue;
        }
        let sweep = annotate::arc_sweep(*center, n, *r, *start, *end);
        out = Some(match out {
            None => (along, along, sweep, curve.clone()),
            Some((lo, hi, widest, arc)) => {
                let (widest, arc) = if sweep > widest {
                    (sweep, curve.clone())
                } else {
                    (widest, arc)
                };
                (lo.min(along), hi.max(along), widest, arc)
            }
        });
    }
    out
}

/// How a view sees a round face: end on, as its circle (an arc of it where
/// its ends are arcs); from the side, as its axis over the face's length
/// (none when it is seen at a slant or its ends are not circles); a
/// section's what lies behind its plane. With how far its widest end
/// turns.
fn seen_round(
    view: &ViewAt<'_>,
    geometry: &BodyGeometry,
    face: &RoundFace,
    transform: &Transform,
) -> Option<(Found, f64)> {
    let spec = &view.placed.spec;
    let (mut lo, mut hi, sweep, widest) = round_extent(geometry, face)?;
    let placed_axis = mitcad_vec3::unit(transform.apply_vector(face.axis))?;
    let point3 = |t: f64| transform.apply_point(mitcad_vec3::add_scaled(face.origin, face.axis, t));
    if let Some(cut) = view.cut() {
        let (s0, s1) = (side(&cut, point3(lo)), side(&cut, point3(hi)));
        if s0 > CUT_TOLERANCE && s1 > CUT_TOLERANCE {
            return None;
        }
        if (s0 - s1).abs() > 1e-12 {
            let t = lo + (hi - lo) * (s0 / (s0 - s1));
            if s0 > CUT_TOLERANCE {
                lo = t;
            } else if s1 > CUT_TOLERANCE {
                hi = t;
            }
        }
    }
    let facing = mitcad_vec3::dot(placed_axis, spec.toward_eye);
    if facing.abs() > 1.0 - 1e-6 {
        // The end nearer the eye stands for it.
        let near = if facing > 0.0 { hi } else { lo };
        let arc = if sweep < TAU - 1e-9 {
            match annotate::seen_curve(
                &placed_curve(&widest, transform),
                None,
                |p| spec.project(p),
                spec.toward_eye,
            ) {
                Ok(Seen::Circle { arc, .. }) => arc,
                _ => None,
            }
        } else {
            None
        };
        return Some((
            Found {
                seen: Seen::Circle {
                    center: spec.project(point3(near)),
                    radius: face.radius,
                    end_on: true,
                    across: [1.0, 0.0],
                    arc,
                },
                at: point3(near),
            },
            sweep,
        ));
    }
    if facing.abs() > 1e-6 || hi - lo < 1e-9 {
        return None;
    }
    Some((
        Found {
            seen: Seen::Axis {
                a: spec.project(point3(lo)),
                b: spec.project(point3(hi)),
                radius: face.radius,
            },
            at: point3((lo + hi) / 2.0),
        },
        sweep,
    ))
}

/// Whether a view draws a round face it sees from the side, its axis from
/// `a` to `b` (view mm): one of its outlines, `radius` either side of the
/// axis, lies on the view's visible lines (mitcad#1286). A hole across a
/// bar, inside it, is not drawn by a view without hidden lines: its
/// centreline would cross the bar alone, past the outline, between the
/// holes the view sees end on.
fn draws_outline(view: &ViewAt<'_>, a: P2, b: P2, radius: f64) -> bool {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0].hypot(d[1]);
    if length < 1e-12 {
        return false;
    }
    let across = [-d[1] / length * radius, d[0] / length * radius];
    [1.0, -1.0].into_iter().any(|side: f64| {
        let along: Vec<P2> = [0.2, 0.4, 0.6, 0.8]
            .into_iter()
            .map(|t: f64| {
                [
                    a[0] + d[0] * t + across[0] * side,
                    a[1] + d[1] * t + across[1] * side,
                ]
            })
            .collect();
        view.draws(&along, radius)
    })
}

/// How far apart along their line two cut edges' ends may lie and still be
/// joined end to end into one piece of a cut line (mitcad#1361), view mm.
const CUT_JOIN: f64 = 1e-4;

/// Where a section's plane cuts a planar face, as the view draws it: the
/// edges of the cut faces that lie in the face's plane, along one line
/// (mitcad#717: the cut edges are picked as the faces they cut), joined end
/// to end into pieces (mitcad#1361: a face the plane cuts beside a hole is
/// cut in two, and a piece is picked alone).
struct CutLine {
    /// A view point of the line and its unit direction (view): the line's
    /// points are `first + along * t`.
    first: P2,
    along: P2,
    /// The pieces as ranges of `t`, each from its lower to its higher, in
    /// order along the line, with gaps between them.
    pieces: Vec<(f64, f64)>,
    /// The section's plane along the view's eye.
    depth: f64,
}

impl CutLine {
    fn at(&self, t: f64) -> P2 {
        [
            self.first[0] + self.along[0] * t,
            self.first[1] + self.along[1] * t,
        ]
    }

    /// How far along the line a view point lies.
    fn t(&self, p: P2) -> f64 {
        (p[0] - self.first[0]) * self.along[0] + (p[1] - self.first[1]) * self.along[1]
    }

    /// A view point of the cut, in the model: on the section's plane, which
    /// the view looks at square on.
    fn lift(&self, spec: &ViewSpec, p: P2) -> Vec3 {
        mitcad_vec3::add(
            mitcad_vec3::add(
                mitcad_vec3::scale(spec.x_axis, p[0]),
                mitcad_vec3::scale(spec.y_axis(), p[1]),
            ),
            mitcad_vec3::scale(spec.toward_eye, self.depth),
        )
    }

    /// The line from `low` to `high` as the view sees it.
    fn found(&self, spec: &ViewSpec, low: f64, high: f64) -> Found {
        let (a, b) = (self.at(low), self.at(high));
        Found {
            seen: Seen::Line(a, b),
            at: self.lift(spec, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]),
        }
    }

    /// The point `t` along the line as the view sees it.
    fn point(&self, spec: &ViewSpec, t: f64) -> Found {
        let p = self.at(t);
        Found {
            seen: Seen::Point(p),
            at: self.lift(spec, p),
        }
    }

    /// The whole line, from its first piece's start to its last piece's
    /// end across its gaps: a pick of the face alone (`{"body", "face"}`,
    /// mitcad#717).
    fn span(&self, spec: &ViewSpec) -> Option<Found> {
        let low = self.pieces.first()?.0;
        let high = self.pieces.last()?.1;
        Some(self.found(spec, low, high))
    }

    /// The piece a view point lies on (within `CUT_JOIN` along the line).
    fn piece_at(&self, p: P2) -> Option<(f64, f64)> {
        let t = self.t(p);
        self.pieces
            .iter()
            .copied()
            .find(|(low, high)| t >= low - CUT_JOIN && t <= high + CUT_JOIN)
    }
}

/// Where a section's plane cuts a planar face as the view draws it
/// ([`CutLine`]). `cuts`: the cut faces of the face's own body as the view
/// shows them (mitcad#718: another body's coplanar cut faces made the line
/// longer). None off a section, for a face parallel to the plane or one it
/// does not cut.
fn cut_line(
    view: &ViewAt<'_>,
    cuts: &[CutFace],
    face: &FlatFace,
    transform: &Transform,
) -> Option<CutLine> {
    let cut = view.cut()?;
    let spec = &view.placed.spec;
    let n = mitcad_vec3::unit(transform.apply_vector(face.normal))?;
    let o = transform.apply_point(face.origin);
    let direction = mitcad_vec3::cross(n, cut.normal);
    if mitcad_vec3::norm(direction) < 1e-9 {
        return None;
    }
    let along = spec.project(direction);
    let length = (along[0] * along[0] + along[1] * along[1]).sqrt();
    if length < 1e-12 {
        return None;
    }
    let mut line = CutLine {
        first: [0.0, 0.0],
        along: [along[0] / length, along[1] / length],
        pieces: Vec::new(),
        depth: mitcad_vec3::dot(cut.origin, spec.toward_eye),
    };
    let on = |p: P2| mitcad_vec3::dot(sub3(line.lift(spec, p), o), n).abs() <= PLANE_TOLERANCE;
    let mut edges: Vec<(P2, P2)> = Vec::new();
    for lp in cuts.iter().flatten() {
        for i in 0..lp.len() {
            let (a, b) = (lp[i], lp[(i + 1) % lp.len()]);
            if (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-9 && on(a) && on(b) {
                edges.push((a, b));
            }
        }
    }
    line.first = edges.first()?.0;
    let mut ranges: Vec<(f64, f64)> = edges
        .iter()
        .map(|(a, b)| {
            let (ta, tb) = (line.t(*a), line.t(*b));
            (ta.min(tb), ta.max(tb))
        })
        .collect();
    ranges.sort_by(|x, y| x.0.total_cmp(&y.0));
    for (low, high) in ranges {
        match line.pieces.last_mut() {
            Some(last) if low <= last.1 + CUT_JOIN => last.1 = last.1.max(high),
            _ => line.pieces.push((low, high)),
        }
    }
    Some(line)
}

/// A planar face's plane where an occurrence places it (mitcad#1296): its
/// origin and unit normal, and the images of the face's own axes
/// (`annotate::face_axes`, in its body's coordinates).
struct WorldPlane {
    origin: Vec3,
    normal: Vec3,
    x: Vec3,
    y: Vec3,
}

impl WorldPlane {
    /// The model point of a point in the face's own coordinates.
    fn point(&self, on: [f64; 2]) -> Vec3 {
        mitcad_vec3::add(
            self.origin,
            mitcad_vec3::add(
                mitcad_vec3::scale(self.x, on[0]),
                mitcad_vec3::scale(self.y, on[1]),
            ),
        )
    }

    /// A model point's coordinates along the face's own axes.
    fn coordinates(&self, p: Vec3) -> [f64; 2] {
        let d = sub3(p, self.origin);
        let along = |axis: Vec3| {
            let length = mitcad_vec3::dot(axis, axis);
            if length > 1e-24 {
                mitcad_vec3::dot(d, axis) / length
            } else {
                0.0
            }
        };
        [along(self.x), along(self.y)]
    }
}

fn world_plane(face: &FlatFace, transform: &Transform) -> Option<WorldPlane> {
    let (x, y) = annotate::face_axes(face.normal);
    Some(WorldPlane {
        origin: transform.apply_point(face.origin),
        normal: mitcad_vec3::unit(transform.apply_vector(face.normal))?,
        x: transform.apply_vector(x),
        y: transform.apply_vector(y),
    })
}

/// Points along a face's boundary curve, closely enough to tell a click
/// inside the face from one outside (mitcad#1296): a line's ends, a
/// circle's or an ellipse's points every 7.5 degrees, a B-spline's control
/// polygon.
fn curve3_points(curve: &Curve3) -> Vec<Vec3> {
    match curve {
        Curve3::Point(_) => Vec::new(),
        Curve3::Line { start, end } => vec![*start, *end],
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            let (from, to) = if *closed {
                (*start, *start + TAU)
            } else {
                (*start, *end)
            };
            let y_axis = mitcad_vec3::cross(*normal, *x_axis);
            let n = (((to - from).abs() / (TAU / 48.0)).ceil() as usize).max(2);
            (0..=n)
                .map(|i| {
                    let t = from + (to - from) * i as f64 / n as f64;
                    mitcad_vec3::add(
                        *center,
                        mitcad_vec3::add(
                            mitcad_vec3::scale(*x_axis, major * t.cos()),
                            mitcad_vec3::scale(y_axis, minor * t.sin()),
                        ),
                    )
                })
                .collect()
        }
        Curve3::BSpline { poles, .. } => poles.clone(),
        Curve3::Helix { .. } => Vec::new(),
    }
}

/// Whether a view point lies inside an outline of polylines (its loops in
/// any order and direction; a hole's loop takes its area out).
fn inside_outline(q: P2, outline: &[Vec<P2>]) -> bool {
    let mut inside = false;
    for line in outline {
        for pair in line.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if (a[1] > q[1]) != (b[1] > q[1]) {
                let x = a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if x > q[0] {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

/// Its name in undo steps: a dimension, a centre mark, a note, ….
fn noun(kind: AnnotationType) -> &'static str {
    match kind {
        AnnotationType::CenterMark => "center mark",
        AnnotationType::Centerline => "centreline",
        AnnotationType::Note => "note",
        AnnotationType::Datum => "datum label",
        AnnotationType::Frame => "feature control frame",
        AnnotationType::Finish => "surface finish symbol",
        AnnotationType::HoleTable => "hole table",
        AnnotationType::PartsList => "parts list",
        AnnotationType::Balloon => "balloon",
        AnnotationType::CenterPattern => "center pattern",
        AnnotationType::EdgeExtension => "edge extension",
        _ => "dimension",
    }
}

/// The `type` of a dimension whose picks choose its type (mitcad#1272).
pub(super) const CHOSEN: &str = "dimension";

/// What `add_drawing_annotation` is asked for: an annotation of a type, or
/// a dimension whose picks choose its type ([`CHOSEN`], mitcad#1272).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Asked {
    Type(AnnotationType),
    Dimension,
}

impl Asked {
    /// The type of a command's `type`; None for an unknown one (and for
    /// a text, a line or an arc, which are no annotation types).
    pub(super) fn of(key: &str) -> Option<Self> {
        if key == CHOSEN {
            return Some(Self::Dimension);
        }
        serde_json::from_value(json!(key)).ok().map(Self::Type)
    }

    /// Its type before its picks choose one: a dimension's checks and
    /// words are a linear one's.
    pub(super) fn kind(self) -> AnnotationType {
        match self {
            Self::Type(kind) => kind,
            Self::Dimension => AnnotationType::Linear,
        }
    }

    /// What a click for it picks: anything for a dimension.
    pub(super) fn wants(self) -> Want {
        match self {
            Self::Type(kind) => kind.wants(),
            Self::Dimension => Want::Any,
        }
    }
}

/// A dimension whose picks chose its type (mitcad#1272): the type, the
/// picks it keeps (a straight edge alone: its two ends) with how the view
/// sees them, and the direction a linear one measures along.
struct Chosen {
    kind: AnnotationType,
    picks: Vec<ViewPick>,
    found: Vec<Found>,
    along: Option<Along>,
}

/// Whether a view looks along none of its component's axes and is no
/// section: seen from an angle unless it looks square at a face.
fn angled(spec: &ViewSpec) -> bool {
    spec.section.is_none() && !super::drawings::along_an_axis(spec)
}

/// Why an annotation of this type, along this direction, is not made on a
/// pictorial view (its value would be foreshortened), if it is not.
fn pictorial_refusal(kind: AnnotationType, along: Option<Along>) -> Option<String> {
    let refused = match kind {
        AnnotationType::Linear => along != Some(Along::Aligned),
        AnnotationType::Ordinate | AnnotationType::Angle => true,
        _ => false,
    };
    refused.then(|| {
        "the view looks at the model from an angle: its lengths along x and y, ordinates and \
         angles would be foreshortened; an aligned dimension measures the true length"
            .to_owned()
    })
}

/// A dimension's measured value as the sheet writes it without its own
/// texts: its prefix, value and tolerance, what a `format.replace` takes
/// the place of (mitcad#1292: the panel's Replace Value shows it).
fn measured_text(
    annotation: &ViewAnnotation,
    value: f64,
    units: WordUnits,
    defaults: &DrawingDefaults,
) -> String {
    let format = AnnotationFormat {
        decimals: annotation.format.decimals.or(defaults.decimals),
        prefix: annotation.format.prefix.clone(),
        ..AnnotationFormat::default()
    };
    let (words, _) = value_text(
        annotation.kind.measure_kind(),
        &format,
        annotation.tolerance.as_ref(),
        value,
        units,
    );
    words.text
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait DrawingAnnotationsOps<K: Kernel> {
    /// A body's named edges, round faces and planar faces, kept by its
    /// shape version.
    fn body_geometry(&self, body: BodyUid) -> Result<Arc<BodyGeometry>, String>;

    /// The bodies a view looks square at a planar face of (mitcad#718):
    /// their lengths in that face's plane are true, so a view along a
    /// given direction straight at an inclined face is drawn and
    /// dimensioned as an orthographic one. Only on a view along a given
    /// direction (`along_a_given_direction`) that is no section and along
    /// no axis (mitcad#722: the isometric presets and the corner views
    /// stay pictorial whatever faces they happen to look square at).
    fn square_bodies(
        &self,
        sheet: &DrawingSheet,
        id: u32,
        shown: &[Shown],
        spec: &ViewSpec,
    ) -> Vec<(BodyUid, Vec<OccurrenceUid>)>;

    /// What one body of a view shows there, from its own projection cut
    /// to the detail's circle and beside the breaks (`own_cuts`,
    /// `bodies_seen`). Kept in a bounded cache of its own by the body's
    /// version and placement and the view, apart from the views'
    /// projections (mitcad#722: an assembly's section with a parts list
    /// projected each body again on every draw and pushed the views'
    /// projections out). A failed projection is not kept.
    fn body_in_view(&self, view: &ViewAt<'_>, s: &Shown) -> Result<Arc<BodyInView>, String>;

    /// Whether a view is seen from an angle: neither along an axis of its
    /// component nor square to a face it shows (`square_bodies`), and not
    /// a section.
    fn seen_from_an_angle(
        &self,
        sheet: &DrawingSheet,
        id: u32,
        shown: &[Shown],
        spec: &ViewSpec,
    ) -> bool;

    /// The view `id` of a sheet as drawn, with its bodies; why not, when
    /// it is not drawn.
    fn view_at<'e>(
        &self,
        drawing: &Drawing,
        si: usize,
        evaluated: &'e Evaluated,
        id: u32,
    ) -> Result<ViewAt<'e>, String>;

    /// The picks of a command found on a view: model topology as given,
    /// a sheet point as the pick nearest it; with the view point of the
    /// first one clicked (a leader's arrow goes nearest it).
    fn picks_of(
        &self,
        view: &ViewAt<'_>,
        inputs: &[PickInput],
        want: Want,
    ) -> Result<(Vec<ViewPick>, Option<P2>), ApiError>;

    fn add_drawing_annotation(&mut self, c: AddAnnotationCommand) -> Result<Value, ApiError>;

    fn edit_drawing_annotation(&mut self, c: EditAnnotationCommand) -> Result<Value, ApiError>;

    fn delete_drawing_annotations(
        &mut self,
        drawing: &str,
        one: Option<u32>,
        several: Option<Vec<u32>>,
    ) -> Result<Value, ApiError>;

    /// `drawing_pick`: what a click picks on a view, with what to show of
    /// it on the sheet (`items`, sheet mm), or `"pick": null`.
    fn drawing_pick_json(&self, query: &PickQuery) -> Result<Value, ApiError>;

    /// `drawing_corners` (mitcad#1338): the corners each drawn view of a
    /// sheet shows on its visible lines, the points a click there picks.
    fn drawing_corners_json(&self, query: &CornersQuery) -> Result<Value, ApiError>;

    /// A sheet's annotations worked out for drawing it: each view's (and
    /// its automatic centre marks and retrieved dimensions) and the text
    /// notes. `sketches`: the design's sketch dimensions (what views
    /// retrieve), when the sheet's views retrieve any.
    fn annotate_sheet(
        &self,
        drawing: &Drawing,
        si: usize,
        evaluated: &Evaluated,
        units: Units,
        sketches: &[(FeatureUid, SketchDimension)],
    ) -> Annotated;

    /// Where the dimensions of the sketches that shape the shown bodies
    /// are drawn (mitcad#744): each sketch with the transforms from its
    /// component into the drawing's (`component`), one per occurrence
    /// path. A sketch of the body's own component goes where the body's
    /// placement puts it; one of the drawing's component (a body it made
    /// into another component) where it is.
    fn sketch_placements(
        &self,
        component: ComponentUid,
        shown: &[Shown],
    ) -> Vec<(FeatureUid, Transform)>;

    // Dimension spacing (mitcad#1173, `drawing_spacing.rs`).

    /// The annotation an `add_drawing_annotation` asked for `asked` would
    /// make first (the first of a series; of a dimension whose picks
    /// choose its type, the type they and the placing click choose,
    /// mitcad#1272), not yet placed, and how its view sees its picks.
    fn placing_annotation(
        &self,
        view: &ViewAt<'_>,
        c: &AddAnnotationCommand,
        asked: Asked,
        id: u32,
    ) -> Result<(ViewAnnotation, Vec<Seen>), ApiError>;

    /// How its view sees an annotation's picks now; why not, when one is
    /// gone.
    fn annotation_seen(
        &self,
        view: &ViewAt<'_>,
        annotation: &ViewAnnotation,
    ) -> Result<Vec<Seen>, String>;

    // The model's items in views (mitcad#1406).

    /// `drawing_model_items`: the Drawing tab's Model Items on views of a
    /// sheet.
    fn drawing_model_items(&mut self, c: ModelItemsCommand) -> Result<Value, ApiError>;

    /// `keep_drawing_model_items`: the model's annotations a view draws,
    /// kept as the drawing's own annotations of the view.
    fn keep_drawing_model_items(&mut self, c: KeepModelItemsCommand) -> Result<Value, ApiError>;
}

impl<K: Kernel> DrawingAnnotationsOps<K> for Document<K> {
    fn body_geometry(&self, body: BodyUid) -> Result<Arc<BodyGeometry>, String> {
        let shape = self
            .body_shape(body)
            .ok_or_else(|| format!("the body {body} is gone"))?;
        let mut fp = Fingerprint::new("drawing-body-geometry-v2");
        fp.str(&body.to_string());
        let own = fp.finish();
        fp.u128(self.body_version(body).unwrap_or(0));
        let key = fp.finish();
        if let Some(geometry) = self.drawing_cache().geometry(key) {
            return Ok(geometry);
        }
        let mut geometry = BodyGeometry::default();
        match self.kernel().edge_geometries(shape) {
            Ok(edges) => {
                for (name, curve) in edges {
                    if let Some(name) = name.and_then(|n| n.parse::<EdgeName>().ok()) {
                        geometry.edges.push((name, curve));
                    }
                }
            }
            Err(KernelError::Unsupported(_)) => {}
            Err(e) => return Err(format!("the edges of {body}: {e}")),
        }
        match self.kernel().face_geometries(shape) {
            Ok(faces) => {
                for (names, surface) in faces {
                    let names: Vec<FaceName> = names
                        .iter()
                        .filter_map(|n| n.parse::<FaceName>().ok())
                        .collect();
                    let Some(face) = names.first().cloned() else {
                        continue;
                    };
                    match surface {
                        SurfaceGeometry::Cylinder {
                            origin,
                            axis,
                            radius,
                        } => {
                            if let Some(axis) = mitcad_vec3::unit(axis) {
                                geometry.cylinders.push(RoundFace {
                                    face,
                                    names,
                                    origin,
                                    axis,
                                    radius,
                                });
                            }
                        }
                        SurfaceGeometry::Plane { origin, normal } => {
                            if let Some(normal) = mitcad_vec3::unit(normal) {
                                geometry.planes.push(FlatFace {
                                    face,
                                    names,
                                    origin,
                                    normal,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(KernelError::Unsupported(_)) => {}
            Err(e) => return Err(format!("the faces of {body}: {e}")),
        }
        let geometry = Arc::new(geometry);
        self.drawing_cache()
            .keep_geometry(own, key, Arc::clone(&geometry));
        Ok(geometry)
    }

    fn square_bodies(
        &self,
        sheet: &DrawingSheet,
        id: u32,
        shown: &[Shown],
        spec: &ViewSpec,
    ) -> Vec<(BodyUid, Vec<OccurrenceUid>)> {
        if !angled(spec) || !sheet.along_a_given_direction(id) {
            return Vec::new();
        }
        shown
            .iter()
            .filter(|s| {
                self.body_geometry(s.body).is_ok_and(|geometry| {
                    geometry.planes.iter().any(|plane| {
                        mitcad_vec3::unit(s.transform.apply_vector(plane.normal)).is_some_and(|n| {
                            mitcad_vec3::dot(n, spec.toward_eye).abs() > 1.0 - 1e-9
                        })
                    })
                })
            })
            .map(|s| (s.body, s.path.clone()))
            .collect()
    }

    fn body_in_view(&self, view: &ViewAt<'_>, s: &Shown) -> Result<Arc<BodyInView>, String> {
        let placed = view.placed;
        let version = self.body_version(s.body).unwrap_or(0);
        let mut fp = Fingerprint::new("drawing-body-in-view-v1");
        fp.u128(super::drawings::view_key(
            "drawing-body-in-view-v1",
            &[(s.body, version, s.transform)],
            &placed.spec,
        ));
        fp.option(view.circle, |fp, (center, radius)| {
            fp.f64(center[0]).f64(center[1]).f64(radius);
        });
        fp.u64(placed.map.breaks.len() as u64);
        for band in &placed.map.breaks {
            fp.u64(band.axis as u64)
                .f64(band.from)
                .f64(band.to)
                .f64(band.gap);
        }
        let key = fp.finish();
        if let Some(own) = self.drawing_cache().body_view(key) {
            return Ok(own);
        }
        let projection = self.project_now(std::slice::from_ref(s), &placed.spec)?;
        let inside = match view.circle {
            Some((center, radius)) => clip::inside_circle(&projection, center, radius),
            None => projection,
        };
        let shown = clip::without_bands(&inside, &placed.map.breaks);
        let own = Arc::new(BodyInView {
            seen: !view_bounds(&shown).is_empty(),
            cuts: shown.cuts,
        });
        self.drawing_cache().keep_body_view(key, Arc::clone(&own));
        Ok(own)
    }

    fn seen_from_an_angle(
        &self,
        sheet: &DrawingSheet,
        id: u32,
        shown: &[Shown],
        spec: &ViewSpec,
    ) -> bool {
        angled(spec) && self.square_bodies(sheet, id, shown, spec).is_empty()
    }

    fn view_at<'e>(
        &self,
        drawing: &Drawing,
        si: usize,
        evaluated: &'e Evaluated,
        id: u32,
    ) -> Result<ViewAt<'e>, String> {
        if let Some(error) = evaluated.errors.get(&id) {
            return Err(format!("view {id} is not drawn: {error}"));
        }
        let index = *evaluated
            .index_of
            .get(&id)
            .ok_or_else(|| format!("there is no view {id} on the sheet"))?;
        let shown: Vec<Shown> = evaluated
            .view_shown
            .get(&id)
            .map(|own| own.iter().map(|i| evaluated.shown[*i].clone()).collect())
            .unwrap_or_default();
        let circle = match drawing.sheets[si].view(id).map(|v| &v.kind) {
            Some(ViewKind::Detail { center, radius, .. }) => Some((*center, *radius)),
            _ => None,
        };
        let placed = &evaluated.views[index];
        let sheet = &drawing.sheets[si];
        let square = self.square_bodies(sheet, id, &shown, &placed.spec);
        let from_an_angle = angled(&placed.spec);
        let def = sheet.view(id);
        let marked = def
            .map(|v| {
                v.annotations
                    .iter()
                    .filter(|a| {
                        matches!(
                            a.kind,
                            AnnotationType::CenterMark | AnnotationType::CenterPattern
                        )
                    })
                    .flat_map(|a| a.picks.iter().cloned())
                    .collect()
            })
            .unwrap_or_default();
        Ok(ViewAt {
            center_marks: def.is_some_and(|v| v.center_marks),
            marked,
            component: drawing.component,
            placed,
            shown,
            circle,
            pictorial: from_an_angle && square.is_empty(),
            angled: from_an_angle,
            square,
            subject_depth: match drawing.sheets[si].subject(id) {
                Subject::Occurrence { occurrence } => occurrence.0.len(),
                _ => 0,
            },
        })
    }

    fn picks_of(
        &self,
        view: &ViewAt<'_>,
        inputs: &[PickInput],
        want: Want,
    ) -> Result<(Vec<ViewPick>, Option<P2>), ApiError> {
        let mut picks = Vec::with_capacity(inputs.len());
        let mut clicked = None;
        for (i, input) in inputs.iter().enumerate() {
            match input {
                PickInput::Pick(pick) => picks.push(pick.clone()),
                PickInput::Point(at) => {
                    let found = self.pick_at(view, *at, want).ok_or_else(|| {
                        ApiError(format!(
                            "nothing to pick at [{:.1}, {:.1}] on the view",
                            at[0], at[1]
                        ))
                    })?;
                    if i == 0 {
                        clicked = Some(view.placed.map.view_point(*at));
                    }
                    picks.push(found.pick);
                }
            }
        }
        Ok((picks, clicked))
    }

    fn add_drawing_annotation(&mut self, c: AddAnnotationCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        let first = old.next_annotation_id();
        // Lines, arcs and texts drawn outside the model (mitcad#729): a
        // text in a view, a line or an arc on the sheet or in a view.
        if c.kind == "line" || c.kind == "arc" || (c.kind == "text" && c.view.is_some()) {
            return self.add_sketch_item(old, c, first);
        }
        if c.kind == "text" {
            return self.add_text_note(old, c, first);
        }
        let asked = Asked::of(&c.kind).ok_or_else(|| {
            ApiError(format!(
                "unknown annotation type {}: {}, {CHOSEN}, text, line or arc",
                c.kind,
                AnnotationType::ALL.map(AnnotationType::key).join(", ")
            ))
        })?;
        let kind = asked.kind();
        if !c.points.is_empty() || c.has_pen() {
            return Err(ApiError(format!(
                "a {} is drawn from its picks: it takes no points or pen",
                noun(kind)
            )));
        }
        let view_id = c
            .view
            .ok_or_else(|| ApiError(format!("a {} is on a view", noun(kind))))?;
        if c.table.is_some() && !kind.is_table() {
            return Err(ApiError(format!("a {} has no table settings", noun(kind))));
        }
        if c.balloons && kind != AnnotationType::PartsList {
            return Err(ApiError(format!("a {} places no balloons", noun(kind))));
        }
        let (si, _) = old
            .find_view(view_id)
            .ok_or_else(|| ApiError(format!("{} has no view {view_id}", old.name)))?;
        let evaluated = self.evaluate_views(&old, si);
        let view = self
            .view_at(&old, si, &evaluated, view_id)
            .map_err(ApiError)?;
        let (mut picks, clicked) = self.picks_of(&view, &c.picks, asked.wants())?;
        // A centre pattern of one circle: every circle of its pattern
        // (mitcad#729).
        if kind == AnnotationType::CenterPattern && picks.len() == 1 {
            picks = self.pattern_of(&view, &picks[0]).map_err(ApiError)?;
        }
        let mut found: Vec<Found> = picks
            .iter()
            .map(|p| self.resolve_found(&view, p))
            .collect::<Result<_, _>>()
            .map_err(ApiError)?;
        // A dimension whose picks choose its type (mitcad#1272); a series
        // is of linear ones.
        let (kind, given) = match asked {
            Asked::Dimension if c.series.is_none() => {
                let chosen = self
                    .chosen_dimension(&view, picks, found, c.at)
                    .map_err(ApiError)?;
                picks = chosen.picks;
                found = chosen.found;
                (chosen.kind, c.along.or(chosen.along))
            }
            _ => (kind, c.along),
        };
        let frame = view.frame();
        let mut template = ViewAnnotation::new(first, kind, Vec::new());
        template.height = c.height;
        template.format = c.format.clone().unwrap_or_default();
        template.tolerance = c.tolerance.clone();
        template.text = c.text.clone();
        template.frame = c.frame.clone();
        template.terminator = c.terminator;
        template.finish = match kind {
            AnnotationType::Finish => Some(c.finish.clone().unwrap_or_default()),
            _ => c.finish.clone(),
        };
        template.at = clicked.filter(|_| {
            matches!(
                kind,
                AnnotationType::Note
                    | AnnotationType::Datum
                    | AnnotationType::Frame
                    | AnnotationType::Finish
                    | AnnotationType::Balloon
            )
        });
        // A table: its place on the sheet and its settings.
        match (kind.is_table(), &c.table) {
            (true, edit) => {
                let at = c.at.ok_or_else(|| {
                    ApiError(format!(
                        "a {} needs its place on the sheet, `at`",
                        noun(kind)
                    ))
                })?;
                if kind == AnnotationType::HoleTable && view.pictorial {
                    return Err(ApiError(
                        "a hole table lists the holes its view sees end on: this view looks \
                         at the model from an angle"
                            .to_owned(),
                    ));
                }
                let mut settings = TableSettings::at(at);
                if let Some(edit) = edit {
                    edit.apply(&mut settings);
                }
                template.table = Some(settings);
            }
            (false, Some(_)) => {
                return Err(ApiError(format!("a {} has no table settings", noun(kind))));
            }
            (false, None) => {}
        }
        if kind == AnnotationType::Datum {
            let letter = match &c.letter {
                Some(letter) => letter.trim().to_owned(),
                // One helper for the model's letters and the drawings'.
                None => self.next_datum_letter().map_err(ApiError)?,
            };
            if let Some(why) = self.datum_taken(&letter, old.uid) {
                return Err(ApiError(why));
            }
            template.letter = Some(letter);
        } else {
            template.letter = c.letter.clone();
        }
        // One annotation of the picks, or a series of linear ones.
        let sets: Vec<Vec<usize>> = match c.series {
            None => vec![(0..picks.len()).collect()],
            Some(_) if kind != AnnotationType::Linear => {
                return Err(ApiError(
                    "a baseline or chain is of linear dimensions".to_owned(),
                ));
            }
            Some(_) if picks.len() < 2 => {
                return Err(ApiError(
                    "a baseline or chain takes two picks at least".to_owned(),
                ));
            }
            Some(Series::Baseline) => (1..picks.len()).map(|i| vec![0, i]).collect(),
            Some(Series::Chain) => (1..picks.len()).map(|i| vec![i - 1, i]).collect(),
        };
        // On a pictorial view, aligned unless given: its picks' true
        // distance.
        let along = along_of(kind, given, &view, &picks, &found);
        if view.pictorial_for(&picks)
            && let Some(why) = pictorial_refusal(kind, along)
        {
            return Err(ApiError(why));
        }
        let mut added: Vec<ViewAnnotation> = Vec::new();
        // A series' first line on the sheet (across it) and the side away
        // from its picks: the others stacked beyond it or in a row with it.
        let mut base: Option<(f64, f64)> = None;
        for (k, set) in sets.iter().enumerate() {
            let mut annotation = template.clone();
            annotation.id = first + k as u32;
            annotation.picks = set.iter().map(|i| picks[*i].clone()).collect();
            annotation.along = along;
            // Its picks and fields first: what follows takes them as they
            // are.
            annotation.check().map_err(ApiError)?;
            let points = view.true_distance(kind, along, &annotation.picks);
            let own: Vec<Seen> = set.iter().map(|i| shown_seen(points, &found[*i])).collect();
            // Its place: through `at`, or after the first of a series.
            if k == 0
                && let Some(at) = c.at
            {
                annotate::placed_at(&mut annotation, &own, &frame, at);
            }
            // A datum's or a frame's tip kept as its place along its edge
            // (mitcad#1295).
            annotate::keep_tip_along(&mut annotation, &own);
            if c.series.is_some() {
                match base {
                    None => {
                        // The first one's line, as it is drawn.
                        if let Some((position, offset)) =
                            annotate::line_position(&annotation, &own, &frame)
                        {
                            annotation.line_offset = Some(offset);
                            base = Some((position, if offset < 0.0 { -1.0 } else { 1.0 }));
                        }
                    }
                    Some((position, away)) => {
                        let step = match c.series {
                            Some(Series::Baseline) => BASELINE_STEP * k as f64,
                            _ => 0.0,
                        };
                        if let Some(offset) =
                            annotate::offset_for(&annotation, &own, &frame, position + away * step)
                        {
                            annotation.line_offset = Some(offset);
                        }
                    }
                }
            }
            annotate::measure(&annotation, &own).map_err(ApiError)?;
            // Drawn from its picks alone: refused when they cannot draw it
            // (mitcad#729: parallel edges meet nowhere).
            if matches!(
                kind,
                AnnotationType::CenterPattern | AnnotationType::EdgeExtension
            ) {
                annotate::lay_out(&annotation, &own, &frame, None, &[]).map_err(ApiError)?;
            }
            annotation.check().map_err(ApiError)?;
            added.push(annotation);
        }
        // A parts list's balloons (mitcad#725), in the same undo step.
        let balloons = if c.balloons {
            let levels = template
                .table
                .as_ref()
                .map(|t| t.levels)
                .unwrap_or_default();
            self.balloons_for(&view, levels, first + added.len() as u32)
        } else {
            Vec::new()
        };
        let ids: Vec<u32> = added.iter().map(|a| a.id).collect();
        let balloon_ids: Vec<u32> = balloons.iter().map(|a| a.id).collect();
        let mut changed = old.clone();
        let i = changed.sheets[si].view_index(view_id).expect("found above");
        changed.sheets[si].views[i].annotations.extend(added);
        changed.sheets[si].views[i].annotations.extend(balloons);
        let label = match ids.as_slice() {
            [id] if balloon_ids.is_empty() => format!("Add {} {id} to {}", noun(kind), old.name),
            [id] => format!(
                "Add {} {id} and {} balloons to {}",
                noun(kind),
                balloon_ids.len(),
                old.name
            ),
            _ => format!("Add {} dimensions to {}", ids.len(), old.name),
        };
        self.set_drawing(old.uid, label, changed)?;
        Ok(
            json!({"drawing": old.uid, "annotation": ids[0], "annotations": ids,
                  "balloons": balloon_ids}),
        )
    }

    fn edit_drawing_annotation(&mut self, c: EditAnnotationCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        let id = c.annotation;
        // A line, an arc or a text drawn outside the model (mitcad#729).
        if let Some((si, view)) = old.find_sketch(id) {
            return self.edit_sketch_item(old, &c, si, view);
        }
        let (si, view_id) = old
            .find_annotation(id)
            .ok_or_else(|| ApiError(format!("{} has no annotation {id}", old.name)))?;
        if c.points.is_some() || c.has_pen() {
            return Err(ApiError(format!(
                "annotation {id} is drawn from its picks: it has no points or pen"
            )));
        }
        let mut changed = old.clone();
        let only_moved = c.picks.is_none()
            && c.along.is_none()
            && c.format.is_none()
            && c.tolerance.is_none()
            && c.text.is_none()
            && c.letter.is_none()
            && c.frame.is_none()
            && c.finish.is_none()
            && c.height.is_none()
            && c.table.is_none()
            && c.terminator.is_none();
        let Some(view_id) = view_id else {
            // A text note.
            let note = changed.sheets[si]
                .notes
                .iter_mut()
                .find(|n| n.id == id)
                .expect("found above");
            if c.picks.is_some()
                || c.along.is_some()
                || c.format.is_some()
                || c.tolerance.is_some()
                || c.letter.is_some()
                || c.frame.is_some()
                || c.finish.is_some()
                || c.line_offset.is_some()
                || c.text_offset.is_some()
                || c.table.is_some()
                || c.slide.is_some()
                || c.terminator.is_some()
            {
                return Err(ApiError(
                    "a text note has a text, a place and a height only".to_owned(),
                ));
            }
            if let Some(text) = &c.text {
                note.text.clone_from(text);
            }
            if let Some(at) = c.at {
                note.at = at;
            }
            if let Some(d) = c.shift {
                note.at = [note.at[0] + d[0], note.at[1] + d[1]];
            }
            if let Some(height) = c.height {
                // `null`: the drawing's (mitcad#729).
                note.height = height;
            }
            note.check().map_err(ApiError)?;
            let verb = if only_moved { "Move" } else { "Edit" };
            self.set_drawing(
                old.uid,
                format!("{verb} note {id} of {}", old.name),
                changed,
            )?;
            return Ok(json!({"drawing": old.uid, "annotation": id}));
        };
        let evaluated = self.evaluate_views(&old, si);
        let view = self
            .view_at(&old, si, &evaluated, view_id)
            .map_err(ApiError)?;
        let vi = changed.sheets[si].view_index(view_id).expect("found");
        // An unchanged letter is no new claim (mitcad#718: files where a
        // letter is shared refused every edit that named it).
        let current = changed.sheets[si].views[vi]
            .annotations
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.letter.clone());
        if let Some(letter) = &c.letter
            && current.as_deref() != Some(letter.trim())
            && let Some(why) = self.datum_taken(letter.trim(), old.uid)
        {
            return Err(ApiError(why));
        }
        let annotation = changed.sheets[si].views[vi]
            .annotations
            .iter_mut()
            .find(|a| a.id == id)
            .expect("found above");
        let kind = annotation.kind;
        if let Some(inputs) = &c.picks {
            let (picks, clicked) = self.picks_of(&view, inputs, kind.wants())?;
            annotation.picks = picks;
            if clicked.is_some() {
                annotation.at = clicked;
            }
            // Drawn afresh about the new picks.
            annotation.line_offset = None;
            annotation.text_offset = None;
            annotation.tip_along = None;
        }
        if let Some(along) = c.along {
            if along != annotation.along.unwrap_or(along) {
                annotation.line_offset = None;
                annotation.text_offset = None;
            }
            // Along the view's x or y as it is now (mitcad#729).
            annotation.turn = None;
            annotation.along = Some(along);
        }
        if let Some(format) = &c.format {
            format.apply(&mut annotation.format);
        }
        if let Some(tolerance) = &c.tolerance {
            annotation.tolerance.clone_from(tolerance);
        }
        if let Some(text) = &c.text {
            annotation.text = Some(text.clone());
        }
        // A datum's new letter: the frames referring to its old one follow
        // it, the model's and every drawing's (mitcad#745).
        let mut relettered: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        if let Some(letter) = &c.letter {
            let new = letter.trim().to_owned();
            if kind == AnnotationType::Datum
                && let Some(before) = &current
                && *before != new
            {
                relettered.insert(before.clone(), new.clone());
            }
            annotation.letter = Some(new);
        }
        if let Some(frame) = &c.frame {
            annotation.frame = Some(frame.clone());
        }
        if let Some(finish) = &c.finish {
            annotation.finish = match kind {
                AnnotationType::Finish => Some(finish.clone().unwrap_or_default()),
                _ => finish.clone(),
            };
        }
        if let Some(offset) = c.line_offset {
            // An angle's arc keeps a radius, as a drag and a place keep it
            // (mitcad#717).
            annotation.line_offset = match kind {
                AnnotationType::Angle => offset.map(|v| v.max(2.0)),
                _ => offset,
            };
        }
        if let Some(offset) = c.text_offset {
            annotation.text_offset = offset;
        }
        if let Some(height) = c.height {
            // Its own letters' height; `null`: the drawing's (mitcad#729).
            annotation.height = height;
        }
        if let Some(terminator) = c.terminator {
            // A datum label's (mitcad#1296; the others refuse it in their
            // check); `null`: by its pick.
            annotation.terminator = terminator;
        }
        if let Some(edit) = &c.table {
            let table = annotation
                .table
                .as_mut()
                .ok_or_else(|| ApiError(format!("a {} has no table settings", noun(kind))))?;
            edit.apply(table);
        }
        annotation.check().map_err(ApiError)?;
        if view.pictorial_for(&annotation.picks)
            && let Some(why) = pictorial_refusal(kind, annotation.along)
        {
            return Err(ApiError(why));
        }
        if c.slide.is_some() && !matches!(kind, AnnotationType::Datum | AnnotationType::Frame) {
            return Err(ApiError(format!(
                "a {} has no tip to slide along its edge: a datum label's triangle and a \
                 feature control frame's arrow slide",
                noun(kind)
            )));
        }
        if kind.is_table() {
            // A table is placed on the sheet, whatever its picks.
            if let Some(at) = c.at {
                annotate::placed_at(annotation, &[], &view.frame(), at);
            }
            if let Some(d) = c.shift {
                annotate::moved(annotation, &[], &view.frame(), d);
            }
        } else if c.at.is_some() || c.shift.is_some() || c.slide.is_some() || c.picks.is_some() {
            let points = view.true_distance(kind, annotation.along, &annotation.picks);
            let seen: Vec<Seen> = annotation
                .picks
                .iter()
                .map(|p| self.resolve_found(&view, p).map(|f| shown_seen(points, &f)))
                .collect::<Result<_, _>>()
                .map_err(|e| ApiError(format!("annotation {id} is broken: {e}")))?;
            let frame = view.frame();
            // New picks: the tip's place along the new one (mitcad#1295).
            if c.picks.is_some() {
                annotate::keep_tip_along(annotation, &seen);
            }
            if let Some(at) = c.at {
                annotate::placed_at(annotation, &seen, &frame, at);
            }
            if let Some(d) = c.slide {
                // A dot on a face slides over the face's plane (mitcad#1296).
                let on_face = annotation
                    .picks
                    .first()
                    .and_then(|p| self.slid_on_face(&view, p, d));
                match on_face {
                    Some(on) => annotation.picks[0].on_face = Some(on),
                    None => annotate::slid(annotation, &seen, &frame, d),
                }
            }
            if let Some(d) = c.shift {
                annotate::moved(annotation, &seen, &frame, d);
            }
        }
        annotation.check().map_err(ApiError)?;
        let verb = if only_moved && (c.at.is_some() || c.shift.is_some() || c.slide.is_some()) {
            "Move"
        } else {
            "Edit"
        };
        // A datum label moved (its square dragged, its tip slid) is named
        // by its letter (mitcad#1294, mitcad#1295: `Move datum A of
        // Drawing1`).
        let what = match (&annotation.letter, kind, verb) {
            (Some(letter), AnnotationType::Datum, "Move") => format!("datum {letter}"),
            _ => format!("{} {id}", noun(kind)),
        };
        self.set_drawing_relettered(
            old.uid,
            format!("{verb} {what} of {}", old.name),
            changed,
            &relettered,
        )?;
        Ok(json!({"drawing": old.uid, "annotation": id}))
    }

    fn delete_drawing_annotations(
        &mut self,
        drawing: &str,
        one: Option<u32>,
        several: Option<Vec<u32>>,
    ) -> Result<Value, ApiError> {
        let ids: Vec<u32> = match (one, several) {
            (Some(id), None) => vec![id],
            (None, Some(ids)) if !ids.is_empty() => ids,
            _ => {
                return Err(ApiError(
                    "give the `annotation` to delete, or the `annotations`".to_owned(),
                ));
            }
        };
        let old = self.find_drawing(drawing)?.clone();
        for id in &ids {
            if old.find_annotation(*id).is_none() {
                return Err(ApiError(format!("{} has no annotation {id}", old.name)));
            }
        }
        let mut changed = old.clone();
        for sheet in &mut changed.sheets {
            sheet.notes.retain(|n| !ids.contains(&n.id));
            sheet.sketch.retain(|k| !ids.contains(&k.id()));
            for view in &mut sheet.views {
                view.annotations.retain(|a| !ids.contains(&a.id));
                view.sketch.retain(|k| !ids.contains(&k.id()));
            }
        }
        let label = match ids.as_slice() {
            [id] => format!("Delete annotation {id} of {}", old.name),
            _ => format!("Delete {} annotations of {}", ids.len(), old.name),
        };
        self.set_drawing(old.uid, label, changed)?;
        Ok(json!({"drawing": old.uid, "deleted": ids}))
    }

    fn drawing_pick_json(&self, query: &PickQuery) -> Result<Value, ApiError> {
        let drawing = self.find_drawing(&query.drawing)?;
        let si = query.sheet;
        if si >= drawing.sheets.len() {
            return Err(ApiError(format!("{} has no sheet {si}", drawing.name)));
        }
        let evaluated = self.evaluate_views(drawing, si);
        // The view given, else the smallest drawn one there.
        let id = match query.view {
            Some(id) => Some(id),
            None => {
                let at = query.at;
                drawing.sheets[si]
                    .views
                    .iter()
                    .filter_map(|v| {
                        let i = *evaluated.index_of.get(&v.id)?;
                        let b = evaluated.views[i].sheet_box();
                        let inside = !b.is_empty()
                            && (0..2).all(|k| at[k] >= b.min[k] - 2.0 && at[k] <= b.max[k] + 2.0);
                        inside.then_some((b.width() * b.height(), v.id))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, id)| id)
            }
        };
        let Some(id) = id else {
            return Ok(json!({"pick": null}));
        };
        let view = self
            .view_at(drawing, si, &evaluated, id)
            .map_err(ApiError)?;
        let want = query
            .want
            .or_else(|| query.kind.map(AnnotationType::wants))
            .unwrap_or_default();
        let Some(found) = self.pick_at(&view, query.at, want) else {
            return Ok(json!({"view": id, "pick": null}));
        };
        let map = &view.placed.map;
        let scale = map.scale;
        let seen = found.found.seen;
        let (what, items) = match seen {
            Seen::Point(p) => ("point", vec![json!({"point": map.at(p)})]),
            Seen::Line(a, b) => ("line", vec![json!({"line": [map.at(a), map.at(b)]})]),
            Seen::Circle {
                center,
                radius,
                end_on: true,
                arc,
                ..
            } => (
                "circle",
                vec![
                    match arc {
                        Some((start, end)) => json!({"arc": {"center": map.at(center),
                            "radius": radius * scale, "start": start, "end": end}}),
                        None => {
                            json!({"circle": {"center": map.at(center), "radius": radius * scale}})
                        }
                    },
                    // Its centre, which a click there picks (mitcad#1272).
                    json!({"point": map.at(center)}),
                ],
            ),
            Seen::Circle {
                center,
                radius,
                across,
                arc,
                ..
            } => {
                let (low, high) = arc.unwrap_or((-radius, radius));
                let a = [center[0] + across[0] * low, center[1] + across[1] * low];
                let b = [center[0] + across[0] * high, center[1] + across[1] * high];
                ("circle", vec![json!({"line": [map.at(a), map.at(b)]})])
            }
            Seen::Axis { a, b, radius } => {
                let d = [b[0] - a[0], b[1] - a[1]];
                let length = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-12);
                let n = [-d[1] / length * radius, d[0] / length * radius];
                let edge = |s: f64| {
                    json!({"line": [map.at([a[0] + s * n[0], a[1] + s * n[1]]),
                                    map.at([b[0] + s * n[0], b[1] + s * n[1]])]})
                };
                ("axis", vec![edge(1.0), edge(-1.0)])
            }
        };
        // A point on a planar face seen face on (mitcad#1296): the face.
        let what = if found.pick.on_face.is_some() {
            "face"
        } else {
            what
        };
        let mut answer = json!({
            "view": id,
            "pick": found.pick,
            "seen": what,
            "at": map.at(seen.point()),
            "items": items,
        });
        // For a dimension whose picks choose its type (no type or want
        // asked): whether this pick makes one alone (mitcad#1361), else
        // what a second pick is wanted for.
        if query.want.is_none() && query.kind.is_none() {
            let alone =
                self.chosen_dimension(&view, vec![found.pick.clone()], vec![found.found], None);
            answer["alone"] = json!(alone.is_ok());
            if let Err(why) = alone {
                answer["next"] = json!(why);
            }
        }
        // A centre picked on its centre mark's line (mitcad#1338): the line
        // shown with it.
        if let Some([a, b]) = found.line {
            answer["center_line"] = json!(true);
            if let Some(items) = answer["items"].as_array_mut() {
                items.insert(0, json!({"line": [map.at(a), map.at(b)]}));
            }
        }
        Ok(answer)
    }

    fn drawing_corners_json(&self, query: &CornersQuery) -> Result<Value, ApiError> {
        let drawing = self.find_drawing(&query.drawing)?;
        let si = query.sheet;
        if si >= drawing.sheets.len() {
            return Err(ApiError(format!("{} has no sheet {si}", drawing.name)));
        }
        let evaluated = self.evaluate_views(drawing, si);
        let mut views = Vec::new();
        for v in &drawing.sheets[si].views {
            // A view the model could not draw shows no corners.
            let Ok(view) = self.view_at(drawing, si, &evaluated, v.id) else {
                continue;
            };
            let map = &view.placed.map;
            // The ends of edges, as a click picks them (the same candidates
            // as `drawing_pick`'s), each place once: the ends of the edges
            // that meet there are one corner.
            let mut taken = HashSet::new();
            let mut points = Vec::new();
            for c in self.candidates(&view) {
                if !matches!(c.pick.point, Some(PickPoint::Start | PickPoint::End)) {
                    continue;
                }
                let p = c.found.seen.point();
                if !view.sees(p) || !view.on_visible(p) {
                    continue;
                }
                let at = map.at(p);
                let key = ((at[0] * 1e4).round() as i64, (at[1] * 1e4).round() as i64);
                if taken.insert(key) {
                    points.push(json!(at));
                }
            }
            views.push(json!({"view": v.id, "points": points}));
        }
        Ok(json!({"views": views}))
    }

    fn annotate_sheet(
        &self,
        drawing: &Drawing,
        si: usize,
        evaluated: &Evaluated,
        units: Units,
        sketches: &[(FeatureUid, SketchDimension)],
    ) -> Annotated {
        let sheet = &drawing.sheets[si];
        let words_units = WordUnits::new(units.symbol, units.mm);
        // What the drawing's dimensions and annotations take without their
        // own (mitcad#729).
        let defaults = &drawing.defaults;
        let mut out = Annotated {
            groups: Vec::new(),
            entries: Vec::new(),
            warnings: Vec::new(),
            retrieved: Vec::new(),
            model_items: Vec::new(),
        };
        // The datum letters a frame may refer to: the model's and every
        // drawing's, as the frame dialog offers them.
        let letters = self.all_datum_letters();
        let datums: Vec<&str> = letters.iter().map(String::as_str).collect();
        // The model's annotations, when a view brings them in (mitcad#1406):
        // the root's drawings only, as the sketch dimensions. Each is drawn
        // once on the sheet, in the first view that sees it, and not where
        // the drawing keeps it as its own.
        let model_items =
            if drawing.component.is_root() && sheet.views.iter().any(|v| v.model_annotations) {
                self.model_items(words_units)
            } else {
                Vec::new()
            };
        let mut drawn_items: HashSet<String> = sheet
            .views
            .iter()
            .flat_map(|v| v.annotations.iter())
            .filter_map(|a| a.kept_from.clone())
            .collect();
        // The sketch dimensions retrieved, each once on the sheet too.
        let mut drawn_dimensions: HashSet<(usize, [i64; 3])> = HashSet::new();
        // The sheet's parts lists first: its balloons take their numbers
        // (mitcad#707 phase 3).
        let mut parts_lists: Vec<(u32, Result<PartsList, String>)> = Vec::new();
        for view in &sheet.views {
            for a in view
                .annotations
                .iter()
                .filter(|a| a.kind == AnnotationType::PartsList)
            {
                let list = self
                    .view_at(drawing, si, evaluated, view.id)
                    .and_then(|v| self.parts_list_of(a, view.id, &v));
                parts_lists.push((a.id, list));
            }
        }
        // The holes' threads, once, when a hole table asks.
        let mut threads: Option<Vec<super::drawing::ThreadOf>> = None;
        for view in &sheet.views {
            let drawn = self.view_at(drawing, si, evaluated, view.id);
            // Where the broken ones are listed: beside the view's box, or
            // its place.
            let mut broken_at = match &drawn {
                Ok(v) => {
                    let b = v.placed.sheet_box();
                    if b.is_empty() {
                        view.place
                    } else {
                        [b.max[0] + 2.0, b.max[1]]
                    }
                }
                Err(_) => view.place,
            };
            if let Ok(v) = &drawn {
                if view.center_marks {
                    out.groups.extend(self.automatic_marks(v));
                }
                if view.retrieved_dimensions
                    && drawing.component.is_root()
                    && v.placed.role != ViewRole::Pictorial
                    && !v.pictorial
                {
                    for (key, g) in
                        self.retrieved_dimensions(v, sketches, words_units, defaults.decimals)
                    {
                        // Drawn in a view before it on the sheet: not again
                        // (mitcad#1406).
                        if !drawn_dimensions.insert(key) {
                            continue;
                        }
                        out.groups.push(g);
                        if !out.retrieved.contains(&key.0) {
                            out.retrieved.push(key.0);
                        }
                    }
                }
                if view.model_annotations
                    && drawing.component.is_root()
                    && v.placed.role != ViewRole::Pictorial
                    && !v.pictorial
                {
                    for item in &model_items {
                        let name = &item.annotation.name;
                        if drawn_items.contains(name) {
                            continue;
                        }
                        let Some((annotation, seen)) = model_item_in_view(v, item) else {
                            continue;
                        };
                        // Laid out by the drawing's engine with the model's
                        // value and words; one it cannot lay out here is left
                        // to the views after it.
                        let Ok((mut laid, _, label, _)) = written(
                            &annotation,
                            v,
                            &seen,
                            item.value,
                            None,
                            words_units,
                            &datums,
                            defaults,
                        ) else {
                            continue;
                        };
                        laid.group.class = "model-annotation";
                        // Not `A<id>`: no annotation of the drawing's.
                        laid.group.id = Some(format!("M:{name}"));
                        out.model_items.push(json!({
                            "view": view.id,
                            "name": name,
                            "kind": item.annotation.kind.key(),
                            "type": annotation.kind.key(),
                            "label": label,
                            "box": bounds_json(&laid.bounds),
                        }));
                        out.groups.push(laid.group);
                        drawn_items.insert(name.clone());
                    }
                }
            }
            for annotation in &view.annotations {
                let mut entry = serde_json::to_value(annotation).unwrap_or(Value::Null);
                entry["view"] = json!(view.id);
                let laid = match &drawn {
                    Err(e) => Err(e.clone()),
                    Ok(v) => match annotation.kind {
                        AnnotationType::HoleTable => {
                            let threads = threads.get_or_insert_with(|| {
                                super::drawing::threads_of(&self.threads_json())
                            });
                            let scale = defaults.text(annotation.height) / DIMENSION_TEXT;
                            self.lay_out_hole_table(
                                annotation, v, units, threads, &mut entry, scale,
                            )
                            .map(|(laid, label, warnings)| {
                                out.warnings.extend(warnings);
                                (laid, None, label, None)
                            })
                        }
                        AnnotationType::PartsList => {
                            match parts_lists.iter().find(|(id, _)| *id == annotation.id) {
                                Some((_, Ok(list))) => {
                                    table_entry(
                                        &mut entry,
                                        annotation,
                                        false,
                                        list.data.rows.len(),
                                    );
                                    let place =
                                        annotation.table.as_ref().map_or([0.0; 2], |t| t.place);
                                    let laid = lay_out_table(
                                        annotation.id,
                                        "parts-list",
                                        &list.data,
                                        place,
                                        defaults.text(annotation.height) / DIMENSION_TEXT,
                                    );
                                    Ok((laid, None, list.label.clone(), None))
                                }
                                Some((_, Err(e))) => Err(e.clone()),
                                None => Err("the parts list is not worked out".to_owned()),
                            }
                        }
                        AnnotationType::Balloon => {
                            let number = annotation.picks.first().and_then(|pick| {
                                let own = parts_lists
                                    .iter()
                                    .filter(|(_, l)| l.as_ref().is_ok_and(|l| l.view == view.id));
                                let others = parts_lists
                                    .iter()
                                    .filter(|(_, l)| l.as_ref().is_ok_and(|l| l.view != view.id));
                                own.chain(others)
                                    .filter_map(|(_, l)| l.as_ref().ok())
                                    .find_map(|l| {
                                        let key = row_of(
                                            self.assembly(),
                                            pick.body,
                                            pick.path(),
                                            l.levels,
                                            l.depth,
                                        )?;
                                        l.item(key, pick.path())
                                    })
                            });
                            match number {
                                Some(n) => self.lay_out_annotation(
                                    annotation,
                                    v,
                                    words_units,
                                    &datums,
                                    Some(n.to_string().as_str()),
                                    defaults,
                                ),
                                // Gone, or listed nowhere: why.
                                None => self
                                    .lay_out_annotation(
                                        annotation,
                                        v,
                                        words_units,
                                        &datums,
                                        Some("?"),
                                        defaults,
                                    )
                                    .and_then(|_| Err(self.unlisted(annotation))),
                            }
                        }
                        _ => self.lay_out_annotation(
                            annotation,
                            v,
                            words_units,
                            &datums,
                            None,
                            defaults,
                        ),
                    },
                };
                match laid {
                    Ok((laid, value, text, warning)) => {
                        entry["broken"] = json!(false);
                        if let Some(value) = value {
                            entry["value"] = json!(value);
                            entry["measured"] =
                                json!(measured_text(annotation, value, words_units, defaults));
                        }
                        entry["label"] = json!(text);
                        if let Some(warning) = warning {
                            entry["warning"] = json!(warning);
                            add_annotation_notice(&mut entry, &annotation.id, &warning);
                        }
                        entry["box"] = bounds_json(&laid.bounds);
                        out.groups.push(laid.group);
                    }
                    Err(error) => {
                        entry["broken"] = json!(true);
                        // A broken annotation is a warning (mitcad#1040):
                        // `error` keeps the reason for its readers.
                        entry["error"] = json!(error);
                        add_annotation_notice(&mut entry, &annotation.id, &error);
                        let at = broken_at;
                        entry["box"] = json!([[at[0], at[1] - 6.0], [at[0] + 14.0, at[1]]]);
                        broken_at = [at[0], at[1] - 8.0];
                        out.warnings.push(format!(
                            "annotation {} on view {}: {error}",
                            annotation.id, view.id
                        ));
                    }
                }
                out.entries.push(entry);
            }
            // Lines, arcs and texts drawn in it (mitcad#729): placed by its
            // definition alone, so drawn whatever the model does.
            let map = super::drawings::view_map(sheet, view);
            for item in &view.sketch {
                let laid = annotate::lay_out_sketch(item, &map, defaults.text(None));
                out.entries.push(sketch_entry(item, Some(view.id), &laid));
                out.groups.push(laid.group);
            }
        }
        for note in &sheet.notes {
            let laid = annotate::lay_out_note(note, defaults.text(None));
            let mut entry = serde_json::to_value(note).unwrap_or(Value::Null);
            entry["type"] = json!("text");
            entry["view"] = Value::Null;
            entry["broken"] = json!(false);
            entry["label"] = json!(note.text.replace('\n', " / "));
            entry["box"] = bounds_json(&laid.bounds);
            out.groups.push(laid.group);
            out.entries.push(entry);
        }
        let on_sheet = ViewMap::new(1.0, [0.0, 0.0], [0.0, 0.0]);
        for item in &sheet.sketch {
            let laid = annotate::lay_out_sketch(item, &on_sheet, defaults.text(None));
            out.entries.push(sketch_entry(item, None, &laid));
            out.groups.push(laid.group);
        }
        out
    }

    fn sketch_placements(
        &self,
        component: ComponentUid,
        shown: &[Shown],
    ) -> Vec<(FeatureUid, Transform)> {
        let component_of: HashMap<FeatureUid, ComponentUid> =
            self.features().map(|e| (e.uid, e.component)).collect();
        let mut placements: Vec<(FeatureUid, &[OccurrenceUid], Transform)> = Vec::new();
        for s in shown {
            let body_component = self.body_component(s.body);
            for sketch in self.sketches_shaping(&[s.body]) {
                let own = component_of.get(&sketch).copied();
                let (path, transform) = if own == body_component {
                    (s.path.as_slice(), s.transform)
                } else if own == Some(component) {
                    (&[][..], Transform::IDENTITY)
                } else {
                    continue;
                };
                if !placements
                    .iter()
                    .any(|(f, p, _)| *f == sketch && *p == path)
                {
                    placements.push((sketch, path, transform));
                }
            }
        }
        placements
            .into_iter()
            .map(|(sketch, _, transform)| (sketch, transform))
            .collect()
    }

    fn placing_annotation(
        &self,
        view: &ViewAt<'_>,
        c: &AddAnnotationCommand,
        asked: Asked,
        id: u32,
    ) -> Result<(ViewAnnotation, Vec<Seen>), ApiError> {
        let (mut picks, _) = self.picks_of(view, &c.picks, asked.wants())?;
        let mut found: Vec<Found> = picks
            .iter()
            .map(|p| self.resolve_found(view, p))
            .collect::<Result<_, _>>()
            .map_err(ApiError)?;
        let (kind, given) = match asked {
            Asked::Dimension if c.series.is_none() => {
                let chosen = self
                    .chosen_dimension(view, picks, found, c.at)
                    .map_err(ApiError)?;
                picks = chosen.picks;
                found = chosen.found;
                (chosen.kind, c.along.or(chosen.along))
            }
            _ => (asked.kind(), c.along),
        };
        // As add_drawing_annotation makes it: its direction from all the
        // picks, a series' first of its first two.
        let along = along_of(kind, given, view, &picks, &found);
        let count = if c.series.is_some() {
            picks.len().min(2)
        } else {
            picks.len()
        };
        let mut annotation = ViewAnnotation::new(id, kind, picks[..count].to_vec());
        annotation.along = along;
        let points = view.true_distance(kind, along, &annotation.picks);
        let seen = found[..count]
            .iter()
            .map(|f| shown_seen(points, f))
            .collect();
        Ok((annotation, seen))
    }

    fn annotation_seen(
        &self,
        view: &ViewAt<'_>,
        annotation: &ViewAnnotation,
    ) -> Result<Vec<Seen>, String> {
        let points = view.true_distance(annotation.kind, annotation.along, &annotation.picks);
        annotation
            .picks
            .iter()
            .map(|p| self.resolve_found(view, p).map(|f| shown_seen(points, &f)))
            .collect()
    }

    fn drawing_model_items(&mut self, c: ModelItemsCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        if old.cut_sheet.is_some() {
            return Err(ApiError(
                "a cut sheet's parts bring in no items of the model".to_owned(),
            ));
        }
        let sheet = old
            .sheets
            .get(c.sheet)
            .ok_or_else(|| ApiError(format!("{} has no sheet {}", old.name, c.sheet)))?;
        let sketch = c.sketch_dimensions.unwrap_or(true);
        let annotations = c.model_annotations.unwrap_or(true);
        if !sketch && !annotations {
            return Err(ApiError(
                "Model Items switches on the sketch dimensions, the model's annotations or both: \
                 neither was asked for"
                    .to_owned(),
            ));
        }
        // The views given, else all the sheet's.
        let ids: Vec<u32> = match &c.views {
            Some(ids) if !ids.is_empty() => {
                if let Some(id) = ids.iter().find(|id| sheet.view_index(**id).is_none()) {
                    return Err(ApiError(format!(
                        "{} of {} has no view {id}",
                        sheet.name, old.name
                    )));
                }
                ids.clone()
            }
            _ => sheet.views.iter().map(|v| v.id).collect(),
        };
        if ids.is_empty() {
            return Err(ApiError(format!(
                "{} of {} has no views",
                sheet.name, old.name
            )));
        }
        let mut changed = old.clone();
        let mut switched = Vec::new();
        for view in &mut changed.sheets[c.sheet].views {
            if !ids.contains(&view.id) {
                continue;
            }
            let before = (view.retrieved_dimensions, view.model_annotations);
            view.retrieved_dimensions |= sketch;
            view.model_annotations |= annotations;
            if (view.retrieved_dimensions, view.model_annotations) != before {
                switched.push(view.id);
            }
        }
        let label = format!("Model Items on {} of {}", sheet.name, old.name);
        self.set_drawing(old.uid, label, changed)?;
        Ok(json!({"drawing": old.uid, "views": ids, "switched": switched}))
    }

    fn keep_drawing_model_items(&mut self, c: KeepModelItemsCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        if !old.component.is_root() {
            return Err(ApiError(
                "only the design's drawings bring in the model's annotations".to_owned(),
            ));
        }
        let (si, _) = old
            .find_view(c.view)
            .ok_or_else(|| ApiError(format!("{} has no view {}", old.name, c.view)))?;
        let units = match &old.sheets[si].units {
            Some(symbol) => Units::by_symbol(symbol)
                .ok_or_else(|| ApiError(format!("unknown length unit “{symbol}”")))?,
            None => Units::of(self.parameters().context().default_length_unit),
        };
        let evaluated = self.evaluate_views(&old, si);
        // What the sheet draws now: the model's annotations this view
        // brings in.
        let drawn = self.annotate_sheet(&old, si, &evaluated, units, &[]);
        let here: Vec<String> = drawn
            .model_items
            .iter()
            .filter(|e| e["view"] == c.view)
            .filter_map(|e| e["name"].as_str().map(str::to_owned))
            .collect();
        let names = match c.names {
            Some(names) if !names.is_empty() => {
                if let Some(name) = names.iter().find(|n| !here.contains(n)) {
                    return Err(ApiError(format!(
                        "view {} of {} does not draw {name}",
                        c.view, old.name
                    )));
                }
                names
            }
            _ if here.is_empty() => {
                return Err(ApiError(format!(
                    "view {} of {} draws no annotations of the model",
                    c.view, old.name
                )));
            }
            _ => here,
        };
        let view = self
            .view_at(&old, si, &evaluated, c.view)
            .map_err(ApiError)?;
        let items = self.model_items(WordUnits::new(units.symbol, units.mm));
        // A datum's letter once on the drawing.
        let mut letters: Vec<String> = old
            .sheets
            .iter()
            .flat_map(|s| s.views.iter())
            .flat_map(|v| v.annotations.iter())
            .filter_map(|a| a.letter.clone())
            .collect();
        let mut kept: Vec<ViewAnnotation> = Vec::new();
        for (next, name) in (old.next_annotation_id()..).zip(names.iter()) {
            let refused = |why: String| ApiError(format!("{name} cannot be kept: {why}"));
            let item = items
                .iter()
                .find(|i| i.annotation.name == *name)
                .ok_or_else(|| refused("the model has no such annotation".to_owned()))?;
            let (mut annotation, _) = model_item_in_view(&view, item)
                .ok_or_else(|| refused(format!("view {} does not see it", c.view)))?;
            annotation.picks = kept_picks(self, &view, item).map_err(refused)?;
            if let Some(letter) = &annotation.letter {
                if letters.contains(letter) {
                    return Err(refused(format!(
                        "datum {letter} is on {} already",
                        old.name
                    )));
                }
                letters.push(letter.clone());
            }
            annotation.id = next;
            annotation.kept_from = Some(name.clone());
            annotation.check().map_err(refused)?;
            // Drawn from its own picks from now on: they must draw it.
            let seen = self.annotation_seen(&view, &annotation).map_err(refused)?;
            annotate::measure(&annotation, &seen).map_err(refused)?;
            annotate::keep_tip_along(&mut annotation, &seen);
            kept.push(annotation);
        }
        let ids: Vec<u32> = kept.iter().map(|a| a.id).collect();
        let label = match names.as_slice() {
            [name] => format!("Keep {name} in {}", old.name),
            _ => format!("Keep {} model annotations in {}", names.len(), old.name),
        };
        let mut changed = old.clone();
        let i = changed.sheets[si].view_index(c.view).expect("found above");
        changed.sheets[si].views[i].annotations.extend(kept);
        self.set_drawing(old.uid, label, changed)?;
        Ok(json!({"drawing": old.uid, "annotations": ids, "kept": names}))
    }
}

/// The picks of the drawing's own annotation a model annotation is kept as
/// (mitcad#1406), in the view that draws it: a length's two ends, a
/// circle's edge or round face, an angle's two straight edges, a leader's
/// feature at its arrow's point ([`kept_pick`]).
fn kept_picks<K: Kernel>(
    doc: &Document<K>,
    view: &ViewAt<'_>,
    item: &ModelItem,
) -> Result<Vec<ViewPick>, String> {
    let model = &item.annotation;
    let second = || {
        model
            .b
            .as_ref()
            .ok_or_else(|| "it has no second pick".to_owned())
    };
    match item.shape {
        ModelShape::Ends([p, q]) => Ok(vec![
            kept_pick(doc, view, &model.a, p, true)?,
            kept_pick(doc, view, second()?, q, true)?,
        ]),
        ModelShape::Circle { center, .. } => {
            Ok(vec![kept_pick(doc, view, &model.a, center, false)?])
        }
        ModelShape::Arc { center, .. } => Ok(vec![
            kept_pick(doc, view, &model.a, center, false)?,
            kept_pick(doc, view, second()?, center, false)?,
        ]),
        ModelShape::Point { at, .. } => Ok(vec![kept_pick(doc, view, &model.a, at, false)?]),
    }
}

/// A model annotation's pick as its view picks it (mitcad#1406): an edge by
/// its name (with `point`, its end or middle at `at` where one is there),
/// a round face by its name (its centre with `point`), a planar face at
/// `at` on it (`on_face`, so the point stays on the face), a vertex as the
/// end of an edge the view sees there. Construction geometry and a whole
/// body are no picks of a view.
fn kept_pick<K: Kernel>(
    doc: &Document<K>,
    view: &ViewAt<'_>,
    pick: &MeasurePick,
    at: Vec3,
    point: bool,
) -> Result<ViewPick, String> {
    let body = pick.body.ok_or_else(|| {
        "a construction plane, axis or point is no pick of a drawing's view".to_owned()
    })?;
    let s = view
        .shown
        .iter()
        .find(|s| s.body == body && pick.occurrence.as_ref().is_none_or(|o| o.0 == s.path))
        .ok_or_else(|| format!("the view does not show the body {body}"))?;
    let base = ViewPick {
        body,
        occurrence: (!s.path.is_empty()).then(|| OccurrencePath(s.path.clone())),
        edge: None,
        face: None,
        point: None,
        on_face: None,
        on_cut: None,
    };
    if let Some(edge) = &pick.edge {
        let whole = ViewPick {
            edge: Some(edge.clone()),
            ..base
        };
        if point {
            for p in [PickPoint::Start, PickPoint::End, PickPoint::Center] {
                let one = ViewPick {
                    point: Some(p),
                    ..whole.clone()
                };
                if doc
                    .resolve_found(view, &one)
                    .is_ok_and(|f| mitcad_vec3::distance(f.at, at) <= KEPT_NEAR)
                {
                    return Ok(one);
                }
            }
        }
        doc.resolve_found(view, &whole)?;
        return Ok(whole);
    }
    if let Some(face) = &pick.face {
        let shape = doc
            .body_shape(body)
            .ok_or_else(|| format!("the body {} is gone", s.name))?;
        let surface = doc
            .kernel()
            .face_geometry(shape, face)
            .map_err(|_| format!("the face {face} of {} is gone", s.name))?;
        let one = match surface {
            SurfaceGeometry::Plane { origin, normal } => {
                let flat = FlatFace {
                    face: face.clone(),
                    names: vec![face.clone()],
                    origin,
                    normal: mitcad_vec3::unit(normal)
                        .ok_or_else(|| format!("the face {face} has no normal"))?,
                };
                let plane = world_plane(&flat, &s.transform)
                    .ok_or_else(|| format!("the face {face} of {} has no plane", s.name))?;
                ViewPick {
                    face: Some(face.clone()),
                    on_face: Some(plane.coordinates(at)),
                    ..base
                }
            }
            _ => ViewPick {
                face: Some(face.clone()),
                point: point.then_some(PickPoint::Center),
                ..base
            },
        };
        doc.resolve_found(view, &one)?;
        return Ok(one);
    }
    if pick.vertex.is_some() {
        let geometry = doc.body_geometry(body)?;
        for (edge, curve) in &geometry.edges {
            let (start, end) = ends(&placed_curve(curve, &s.transform));
            for (p, q) in [(PickPoint::Start, start), (PickPoint::End, end)] {
                if mitcad_vec3::distance(q, at) > KEPT_NEAR {
                    continue;
                }
                let one = ViewPick {
                    edge: Some(edge.clone()),
                    point: Some(p),
                    ..base.clone()
                };
                if doc.resolve_found(view, &one).is_ok() {
                    return Ok(one);
                }
            }
        }
        return Err(format!(
            "no edge of {} that the view sees ends at its vertex",
            s.name
        ));
    }
    Err(format!(
        "a whole body is no pick of a drawing's view: {}",
        s.name
    ))
}

/// The direction a new dimension of these picks measures along (as
/// `add_drawing_annotation` gives it): the one given, else on a pictorial
/// view aligned (its picks' true distance), else as its picks lie.
fn along_of(
    kind: AnnotationType,
    given: Option<Along>,
    view: &ViewAt<'_>,
    picks: &[ViewPick],
    found: &[Found],
) -> Option<Along> {
    match kind {
        AnnotationType::Linear => {
            Some(
                given.unwrap_or_else(|| match (found.first(), found.last()) {
                    _ if view.pictorial_for(picks) => Along::Aligned,
                    (Some(a), Some(b)) if found.len() >= 2 => {
                        annotate::default_along(&[a.seen, b.seen])
                    }
                    _ => Along::Horizontal,
                }),
            )
        }
        AnnotationType::Ordinate => Some(given.unwrap_or(Along::Horizontal)),
        _ => given,
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait DrawingAnnotationsLocal<K: Kernel> {
    /// The cut faces of one body of a section as the view shows them (in a
    /// detail's circle, beside breaks); the view's own when it shows that
    /// body alone or the body's projection fails.
    fn own_cuts(&self, view: &ViewAt<'_>, s: &Shown) -> Vec<CutFace>;

    /// Everything a click on a view can pick: each shown body's named
    /// edges (and their ends and a circle's centre), its round faces seen
    /// from the side or end on, and on a section the pieces of the cut
    /// lines of the planar faces its plane cuts (and their ends where no
    /// edge's end is seen, mitcad#1361); what the section takes away left
    /// out.
    fn candidates(&self, view: &ViewAt<'_>) -> Vec<Candidate>;

    /// The pick nearest a sheet point on a view, of what is wanted: a
    /// point within 2 mm of paper first, else an edge or a face within
    /// 3 mm; where the view shows it (its part nearest the click inside a
    /// detail's circle, beside a break's bands). Of picks as near, the one
    /// the view draws as a visible line, then the one nearest the eye.
    fn pick_at(&self, view: &ViewAt<'_>, at: P2, want: Want) -> Option<Candidate>;

    /// A planar face the view sees face on with the view point `q` inside
    /// its outline (the one nearest the eye), picked at `q`: its pick the
    /// face with the point in the face's own coordinates (mitcad#1296, a
    /// datum label on the face).
    fn face_on_at(&self, view: &ViewAt<'_>, q: P2) -> Option<Candidate>;

    /// Where a drag by `delta` (sheet) takes a point picked on a planar
    /// face (`on_face`), in the face's own coordinates: on the face's
    /// plane (mitcad#1296); None for another pick.
    fn slid_on_face(&self, view: &ViewAt<'_>, pick: &ViewPick, delta: P2) -> Option<[f64; 2]>;

    /// How a view sees a pick now, found again by its name; why not, when
    /// it is gone (or, on a section, in front of its plane).
    fn resolve_found(&self, view: &ViewAt<'_>, pick: &ViewPick) -> Result<Found, String>;

    /// The dimension picks choose (`"type": "dimension"`, mitcad#1272),
    /// `at` its placing click on the sheet: the same pick twice is one;
    /// one circle a diameter, one arc a radius, one round face seen from
    /// the side a diameter, one straight edge or one piece of a section's
    /// cut line (mitcad#1361) its length (from its start to its end); two
    /// straight edges that are not parallel an angle,
    /// any other two a linear dimension, horizontal, vertical or aligned
    /// as the placing click chooses ([`annotate::placed_along`]; aligned
    /// on a pictorial view, where an angle is a linear one too). Refused
    /// when the picks lie on one point (never 0 mm), for a point alone,
    /// and for more than two picks.
    fn chosen_dimension(
        &self,
        view: &ViewAt<'_>,
        picks: Vec<ViewPick>,
        found: Vec<Found>,
        at: Option<P2>,
    ) -> Result<Chosen, String>;

    /// Why a datum label of this letter cannot go on a drawing, if it
    /// cannot: the model or another drawing has one (mitcad#717).
    fn datum_taken(&self, letter: &str, drawing: DrawingUid) -> Option<String>;

    /// A balloon on each parts list row's bodies a view shows (mitcad#725):
    /// the bodies the list counts (`bodies_seen`), on a straight edge of
    /// them the view draws as a visible line, the first one found, numbered
    /// from `first`. Their circles stand in one column right of the view's
    /// box, from its top down, in the order of their arrows' heights, so
    /// that they cover neither one another nor the view. A row whose bodies
    /// show no such edge gets none.
    fn balloons_for(&self, view: &ViewAt<'_>, levels: Levels, first: u32) -> Vec<ViewAnnotation>;

    fn add_text_note(
        &mut self,
        old: Drawing,
        c: AddAnnotationCommand,
        id: u32,
    ) -> Result<Value, ApiError>;

    /// A line or an arc on a sheet or in a view, or a text in a view,
    /// drawn outside the model (mitcad#729): its points given on the sheet,
    /// kept in the view's coordinates so that it moves, scales and turns
    /// with the view.
    fn add_sketch_item(
        &mut self,
        old: Drawing,
        c: AddAnnotationCommand,
        id: u32,
    ) -> Result<Value, ApiError>;

    /// A line, an arc or a text drawn outside the model changed: its
    /// points or place, pen, text, height, or moved by a drag
    /// (mitcad#729).
    fn edit_sketch_item(
        &mut self,
        old: Drawing,
        c: &EditAnnotationCommand,
        si: usize,
        view: Option<u32>,
    ) -> Result<Value, ApiError>;

    /// The circles of the pattern a circle picked on a view is one of
    /// (mitcad#729): those of its body, where the same occurrence places
    /// it, that the view sees end on, as large and where it shows them,
    /// picked as it was (edges or round faces); each centre once, the one
    /// picked first, the others along the view's x and then y.
    fn pattern_of(&self, view: &ViewAt<'_>, pick: &ViewPick) -> Result<Vec<ViewPick>, String>;

    /// The sketch dimensions a view retrieves (mitcad#717): of the
    /// sketches that shape its bodies, those it sees square on, where it
    /// shows them (behind a section's plane, in a detail's circle, beside
    /// breaks); written by #702's engine. Each with its index in
    /// `sketches` and where it is placed (`retrieved_key`).
    ///
    /// A sketch's dimensions are in its component's coordinates: each
    /// placement of its shaped bodies the view shows (one per occurrence
    /// path) draws them where that placement puts them (mitcad#744), so a
    /// component placed twice gives them twice.
    fn retrieved_dimensions(
        &self,
        view: &ViewAt<'_>,
        sketches: &[(FeatureUid, SketchDimension)],
        units: WordUnits,
        decimals: Option<u8>,
    ) -> Vec<((usize, [i64; 3]), Group)>;

    /// An annotation laid out on its view: its group and box, its value,
    /// its words on one line and a warning of its words (a fit code out of
    /// range, a datum referred to that no label has). `number`: a
    /// balloon's.
    #[allow(clippy::type_complexity)]
    fn lay_out_annotation(
        &self,
        annotation: &ViewAnnotation,
        view: &ViewAt<'_>,
        units: WordUnits,
        datums: &[&str],
        number: Option<&str>,
        defaults: &DrawingDefaults,
    ) -> Result<(annotate::Laid, Option<f64>, String, Option<String>), String>;

    /// A hole table laid out with its rows (its entry given its columns),
    /// its words on one line (its rows, `no holes`) and the warnings of
    /// finding its holes.
    fn lay_out_hole_table(
        &self,
        annotation: &ViewAnnotation,
        view: &ViewAt<'_>,
        units: Units,
        threads: &[super::drawing::ThreadOf],
        entry: &mut Value,
        scale: f64,
    ) -> Result<(annotate::Laid, String, Vec<String>), String>;

    /// Why a balloon has no number: no parts list on its sheet lists what
    /// it picks.
    fn unlisted(&self, annotation: &ViewAnnotation) -> String;

    /// A view's automatic centre marks: on each circle it sees end on that
    /// turns half way round at least (concentric ones once, the largest;
    /// mitcad#717: a half-cylinder hole's too), and a centreline on each
    /// round face it sees from the side whose ends turn half way round at
    /// least; inside a detail's circle, beside breaks and behind a
    /// section's plane; on a view without hidden lines, only of what it
    /// draws (mitcad#1286: not of the holes inside the part).
    fn automatic_marks(&self, view: &ViewAt<'_>) -> Vec<Group>;

    /// The lines of the centre marks a view draws (mitcad#1338), sheet mm,
    /// each from its circle's centre outward, with the pick of that centre
    /// among `candidates`: its automatic marks while they are on, and its
    /// centre mark annotations' (as `annotate::center_mark` draws them).
    fn center_mark_lines<'c>(
        &self,
        view: &ViewAt<'_>,
        candidates: &'c [Candidate],
    ) -> Vec<(&'c Candidate, [P2; 2])>;
}

impl<K: Kernel> DrawingAnnotationsLocal<K> for Document<K> {
    fn own_cuts(&self, view: &ViewAt<'_>, s: &Shown) -> Vec<CutFace> {
        let placed = view.placed;
        if view.shown.len() < 2 {
            return placed.projection.cuts.clone();
        }
        match self.body_in_view(view, s) {
            Ok(own) => own.cuts.clone(),
            Err(_) => placed.projection.cuts.clone(),
        }
    }

    fn candidates(&self, view: &ViewAt<'_>) -> Vec<Candidate> {
        let mut out = Vec::new();
        let mut ends = Vec::new();
        for s in &view.shown {
            let Ok(geometry) = self.body_geometry(s.body) else {
                continue;
            };
            let occurrence = (!s.path.is_empty()).then(|| OccurrencePath(s.path.clone()));
            let pick = |edge: Option<&EdgeName>, face: Option<&FaceName>, point| ViewPick {
                body: s.body,
                occurrence: occurrence.clone(),
                edge: edge.cloned(),
                face: face.cloned(),
                point,
                on_face: None,
                on_cut: None,
            };
            for (edge, curve) in &geometry.edges {
                let curve = placed_curve(curve, &s.transform);
                let points: &[Option<PickPoint>] = match &curve {
                    CurveGeometry::Circle {
                        center,
                        normal,
                        radius,
                        start,
                        end,
                    } => {
                        // A whole circle has no ends to pick (mitcad#717).
                        if annotate::arc_sweep(*center, *normal, *radius, *start, *end)
                            >= TAU - 1e-9
                        {
                            &[None, Some(PickPoint::Center)]
                        } else {
                            &[
                                None,
                                Some(PickPoint::Start),
                                Some(PickPoint::End),
                                Some(PickPoint::Center),
                            ]
                        }
                    }
                    _ => &[None, Some(PickPoint::Start), Some(PickPoint::End)],
                };
                for point in points {
                    if let Ok(found) = seen_edge(view, &curve, *point) {
                        out.push(Candidate {
                            pick: pick(Some(edge), None, *point),
                            found,
                            line: None,
                        });
                    }
                }
            }
            for face in &geometry.cylinders {
                if let Some((found, _)) = seen_round(view, &geometry, face, &s.transform) {
                    out.push(Candidate {
                        pick: pick(None, Some(&face.face), None),
                        found,
                        line: None,
                    });
                }
            }
            if view.cut().is_some() {
                // Each piece of a planar face's cut line, picked at its
                // middle (mitcad#1361), and its ends.
                let cuts = self.own_cuts(view, s);
                let spec = &view.placed.spec;
                for face in &geometry.planes {
                    let (Some(line), Some(plane)) = (
                        cut_line(view, &cuts, face, &s.transform),
                        world_plane(face, &s.transform),
                    ) else {
                        continue;
                    };
                    for &(low, high) in &line.pieces {
                        let found = line.found(spec, low, high);
                        let piece = ViewPick {
                            on_cut: Some(plane.coordinates(found.at)),
                            ..pick(None, Some(&face.face), None)
                        };
                        for (point, t) in [(PickPoint::Start, low), (PickPoint::End, high)] {
                            ends.push(Candidate {
                                pick: ViewPick {
                                    point: Some(point),
                                    ..piece.clone()
                                },
                                found: line.point(spec, t),
                                line: None,
                            });
                        }
                        out.push(Candidate {
                            pick: piece,
                            found,
                            line: None,
                        });
                    }
                }
            }
        }
        // A cut line's ends where no edge's end is seen (mitcad#1361: the
        // corners of a section's cut faces, beside a hole too), each once;
        // an edge's end there stays the pick it was.
        for end in ends {
            let p = end.found.seen.point();
            let taken = out.iter().any(|c| {
                let Seen::Point(q) = c.found.seen else {
                    return false;
                };
                (q[0] - p[0]).hypot(q[1] - p[1]) <= 1e-6
            });
            if !taken {
                out.push(end);
            }
        }
        out
    }

    fn pick_at(&self, view: &ViewAt<'_>, at: P2, want: Want) -> Option<Candidate> {
        let map = &view.placed.map;
        let q = map.view_point(at);
        let reach = annotate::PICK_REACH / map.scale;
        let point_reach = annotate::POINT_REACH / map.scale;
        // A snap (mitcad#1272) goes to the nearest point, whatever its kind
        // and however it is drawn: points as near as one another are one
        // place there (the ends of edges seen on one another).
        let tie = if want == Want::Snap {
            1e-9
        } else {
            PICK_TIE / map.scale
        };
        let eye = view.placed.spec.toward_eye;
        let mut candidates = self.candidates(view);
        if want == Want::Snap {
            // A line or an arc drawn outside the model snaps to straight
            // edges' middles too (mitcad#1272).
            let middles: Vec<Candidate> = candidates
                .iter()
                .filter(|c| c.pick.point.is_none() && matches!(c.found.seen, Seen::Line(..)))
                .map(|c| {
                    let mut middle = c.clone();
                    middle.pick.point = Some(PickPoint::Center);
                    middle.found.seen = Seen::Point(c.found.seen.point());
                    middle
                })
                .collect();
            candidates.extend(middles);
        }
        let nearest = |keep: &dyn Fn(&Candidate) -> bool, within: f64| {
            let mut found: Vec<(f64, &Candidate)> = candidates
                .iter()
                .filter(|c| keep(c))
                .filter_map(|c| {
                    let d = c.found.seen.distance(q);
                    (d <= within && view.sees(c.found.seen.nearest(q))).then_some((d, c))
                })
                .collect();
            let best = found.iter().map(|(d, _)| *d).fold(f64::INFINITY, f64::min);
            found.retain(|(d, _)| *d <= best + tie);
            found
                .into_iter()
                .map(|(d, c)| {
                    let visible = view.on_visible(c.found.seen.nearest(q));
                    let depth = mitcad_vec3::dot(c.found.at, eye);
                    (visible, depth, d, c)
                })
                .max_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then(a.1.total_cmp(&b.1))
                        .then(b.2.total_cmp(&a.2))
                })
                .map(|(.., c)| c.clone())
        };
        let is_point = |c: &Candidate| c.pick.point.is_some();
        let is_line =
            |c: &Candidate| c.pick.point.is_none() && matches!(c.found.seen, Seen::Line(..));
        let is_round = |c: &Candidate| {
            c.pick.point.is_none()
                && matches!(c.found.seen, Seen::Circle { .. } | Seen::Axis { .. })
        };
        // A centre mark's line (mitcad#1338): a click on a line of a centre
        // mark the view draws (its automatic ones, or a centre mark
        // annotation's) picks that circle's or arc's centre, so that a
        // corner is dimensioned to a hole's centre line. Only when no edge
        // or point is within reach (a round face seen from the side is
        // picked on its axis anyway).
        let center_line = || {
            let marks = self.center_mark_lines(view, &candidates);
            let (_, centre, [a, b]) = marks
                .into_iter()
                .map(|(c, line)| (annotate::segment_distance(at, line[0], line[1]), c, line))
                .filter(|(d, ..)| *d <= annotate::PICK_REACH)
                .min_by(|x, y| x.0.total_cmp(&y.0))?;
            let mut centre = centre.clone();
            centre.line = Some([map.view_point(a), map.view_point(b)]);
            Some(centre)
        };
        // A section's line through a hole (mitcad#1362): a snap that finds
        // no point within reach goes onto the nearest centre line within
        // reach, a round face's axis seen from the side or a centre mark's
        // line, at the click's foot on it (the line shown with it).
        let on_centre_line = || {
            let axes = candidates.iter().filter_map(|c| match c.found.seen {
                Seen::Axis { a, b, .. } if c.pick.point.is_none() => {
                    Some((c, [map.at(a), map.at(b)]))
                }
                _ => None,
            });
            let (_, hit, [a, b], foot) = axes
                .chain(self.center_mark_lines(view, &candidates))
                .map(|(c, [a, b])| {
                    let foot = Seen::Line(a, b).nearest(at);
                    (annotate::segment_distance(at, a, b), c, [a, b], foot)
                })
                .filter(|(d, .., foot)| {
                    *d <= annotate::PICK_REACH && view.sees(map.view_point(*foot))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0))?;
            let mut hit = hit.clone();
            hit.found.seen = Seen::Point(map.view_point(foot));
            hit.line = Some([map.view_point(a), map.view_point(b)]);
            Some(hit)
        };
        match want {
            Want::Any => nearest(&is_point, point_reach)
                .or_else(|| nearest(&|c: &Candidate| c.pick.point.is_none(), reach))
                .or_else(center_line),
            Want::Point => nearest(&is_point, point_reach.max(reach))
                .or_else(|| {
                    // A circle's rim or a round face: its centre.
                    nearest(&is_round, reach).map(|mut c| {
                        c.pick.point = Some(PickPoint::Center);
                        c.found.seen = Seen::Point(c.found.seen.point());
                        c
                    })
                })
                .or_else(center_line),
            Want::Line => nearest(&is_line, reach),
            Want::Circle => nearest(&is_round, reach),
            Want::Snap => nearest(&is_point, point_reach.max(reach)).or_else(on_centre_line),
            // A datum label (mitcad#1296): as Any, else a planar face seen
            // face on under the click.
            Want::Datum => nearest(&is_point, point_reach)
                .or_else(|| nearest(&|c: &Candidate| c.pick.point.is_none(), reach))
                .or_else(|| self.face_on_at(view, q)),
        }
    }

    fn face_on_at(&self, view: &ViewAt<'_>, q: P2) -> Option<Candidate> {
        if !view.sees(q) {
            return None;
        }
        let spec = &view.placed.spec;
        let eye = spec.toward_eye;
        let mut best: Option<(f64, Candidate)> = None;
        for s in &view.shown {
            let Some(shape) = self.body_shape(s.body) else {
                continue;
            };
            // The kernel's planar faces as they are now (a click inside a
            // face is rarer than one on an edge: not worth a cache).
            let Ok(surfaces) = self.kernel().face_geometries(shape) else {
                continue;
            };
            let planes = surfaces.into_iter().filter_map(|(names, surface)| {
                let SurfaceGeometry::Plane { origin, normal } = surface else {
                    return None;
                };
                let face = names.first()?.parse::<FaceName>().ok()?;
                Some(FlatFace {
                    names: vec![face.clone()],
                    face,
                    origin,
                    normal: mitcad_vec3::unit(normal)?,
                })
            });
            for face in planes {
                let face = &face;
                let Some(plane) = world_plane(face, &s.transform) else {
                    continue;
                };
                if mitcad_vec3::dot(plane.normal, eye).abs() < 1.0 - 1e-6 {
                    continue;
                }
                let depth = mitcad_vec3::dot(plane.origin, eye);
                if best.as_ref().is_some_and(|(d, _)| *d >= depth) {
                    continue;
                }
                let Ok(boundary) = self.kernel().face_boundary(shape, &face.face) else {
                    continue;
                };
                let outline: Vec<Vec<P2>> = boundary
                    .iter()
                    .map(|(_, curve)| {
                        curve3_points(curve)
                            .into_iter()
                            .map(|p| spec.project(s.transform.apply_point(p)))
                            .collect()
                    })
                    .collect();
                if !inside_outline(q, &outline) {
                    continue;
                }
                // The click on the face's plane, in the face's own
                // coordinates.
                let at = mitcad_vec3::add(
                    mitcad_vec3::add(
                        mitcad_vec3::scale(spec.x_axis, q[0]),
                        mitcad_vec3::scale(spec.y_axis(), q[1]),
                    ),
                    mitcad_vec3::scale(eye, depth),
                );
                let on = plane.coordinates(at);
                let occurrence = (!s.path.is_empty()).then(|| OccurrencePath(s.path.clone()));
                best = Some((
                    depth,
                    Candidate {
                        pick: ViewPick {
                            body: s.body,
                            occurrence,
                            edge: None,
                            face: Some(face.face.clone()),
                            point: None,
                            on_face: Some(on),
                            on_cut: None,
                        },
                        found: Found {
                            seen: Seen::Point(spec.project(at)),
                            at,
                        },
                        line: None,
                    },
                ));
            }
        }
        best.map(|(_, c)| c)
    }

    fn slid_on_face(&self, view: &ViewAt<'_>, pick: &ViewPick, delta: P2) -> Option<[f64; 2]> {
        let on = pick.on_face?;
        let face = pick.face.as_ref()?;
        let s = view
            .shown
            .iter()
            .find(|s| s.body == pick.body && s.path.as_slice() == pick.path())?;
        let shape = self.body_shape(s.body)?;
        let SurfaceGeometry::Plane { origin, normal } =
            self.kernel().face_geometry(shape, face).ok()?
        else {
            return None;
        };
        let flat = FlatFace {
            face: face.clone(),
            names: vec![face.clone()],
            origin,
            normal: mitcad_vec3::unit(normal)?,
        };
        let plane = world_plane(&flat, &s.transform)?;
        let spec = &view.placed.spec;
        // The drag on the sheet, in the model: along the view's axes, at
        // the paper's scale, onto the face's plane.
        let scale = view.placed.map.scale;
        let shift = mitcad_vec3::add(
            mitcad_vec3::scale(spec.x_axis, delta[0] / scale),
            mitcad_vec3::scale(spec.y_axis(), delta[1] / scale),
        );
        let moved = mitcad_vec3::add(plane.point(on), shift);
        let off = mitcad_vec3::dot(sub3(moved, plane.origin), plane.normal);
        Some(plane.coordinates(mitcad_vec3::add(
            moved,
            mitcad_vec3::scale(plane.normal, -off),
        )))
    }

    fn resolve_found(&self, view: &ViewAt<'_>, pick: &ViewPick) -> Result<Found, String> {
        let s = view
            .shown
            .iter()
            .find(|s| s.body == pick.body && s.path.as_slice() == pick.path())
            .ok_or_else(|| {
                let at = pick
                    .occurrence
                    .as_ref()
                    .map_or(String::new(), |p| format!(" in {p}"));
                format!("the view does not show the body {}{at}", pick.body)
            })?;
        let shape = self
            .body_shape(s.body)
            .ok_or_else(|| format!("the body {} is gone", s.body))?;
        if let Some(edge) = &pick.edge {
            let curve = self
                .kernel()
                .edge_geometry(shape, edge)
                .map_err(|_| format!("the edge {edge} of {} is gone", s.name))?;
            return seen_edge(view, &placed_curve(&curve, &s.transform), pick.point)
                .map_err(|e| format!("the edge {edge} of {}: {e}", s.name));
        }
        let face = pick
            .face
            .as_ref()
            .ok_or_else(|| "a pick is a body's edge or face".to_owned())?;
        let surface = self
            .kernel()
            .face_geometry(shape, face)
            .map_err(|_| format!("the face {face} of {} is gone", s.name))?;
        // A point on a planar face (mitcad#1296: a datum label on it): where
        // the face's own coordinates put it now, seen as a point.
        if let Some(on) = pick.on_face {
            let SurfaceGeometry::Plane { origin, normal } = surface else {
                return Err(format!("the face {face} of {} is not planar", s.name));
            };
            let flat = FlatFace {
                face: face.clone(),
                names: vec![face.clone()],
                origin,
                normal: mitcad_vec3::unit(normal)
                    .ok_or_else(|| format!("the face {face} has no normal"))?,
            };
            let plane = world_plane(&flat, &s.transform)
                .ok_or_else(|| format!("the face {face} of {} has no plane", s.name))?;
            let at = plane.point(on);
            return Ok(Found {
                seen: Seen::Point(view.placed.spec.project(at)),
                at,
            });
        }
        let geometry = self.body_geometry(s.body)?;
        let found = match surface {
            SurfaceGeometry::Cylinder {
                origin,
                axis,
                radius,
            } => {
                let axis = mitcad_vec3::unit(axis)
                    .ok_or_else(|| format!("the face {face} has no axis"))?;
                let round = RoundFace {
                    face: face.clone(),
                    names: geometry
                        .cylinder(face)
                        .map_or_else(|| vec![face.clone()], |c| c.names.clone()),
                    origin,
                    axis,
                    radius,
                };
                seen_round(view, &geometry, &round, &s.transform)
                    .map(|(found, _)| found)
                    .ok_or_else(|| {
                        format!(
                            "the face {face} of {} is seen neither end on nor from the side",
                            s.name
                        )
                    })?
            }
            SurfaceGeometry::Plane { origin, normal } if view.cut().is_some() => {
                let flat = FlatFace {
                    face: face.clone(),
                    names: vec![face.clone()],
                    origin,
                    normal: mitcad_vec3::unit(normal).unwrap_or([0.0, 0.0, 1.0]),
                };
                let not_cut = || {
                    format!(
                        "the section's plane does not cut the face {face} of {}",
                        s.name
                    )
                };
                let line = cut_line(view, &self.own_cuts(view, s), &flat, &s.transform)
                    .ok_or_else(not_cut)?;
                let spec = &view.placed.spec;
                let Some(on) = pick.on_cut else {
                    return line.span(spec).ok_or_else(not_cut);
                };
                // The piece of the cut line its point lies on (mitcad#1361),
                // or one of its ends.
                let plane = world_plane(&flat, &s.transform)
                    .ok_or_else(|| format!("the face {face} of {} has no plane", s.name))?;
                let (low, high) =
                    line.piece_at(spec.project(plane.point(on)))
                        .ok_or_else(|| {
                            format!(
                                "the point picked on the cut of the face {face} of {} lies on no \
                             piece of it",
                                s.name
                            )
                        })?;
                match pick.point {
                    Some(PickPoint::Start) => return Ok(line.point(spec, low)),
                    Some(PickPoint::End) => return Ok(line.point(spec, high)),
                    _ => line.found(spec, low, high),
                }
            }
            _ => return Err(format!("the face {face} of {} is not round", s.name)),
        };
        Ok(match pick.point {
            Some(PickPoint::Center) => Found {
                seen: Seen::Point(found.seen.point()),
                at: found.at,
            },
            _ => found,
        })
    }

    fn chosen_dimension(
        &self,
        view: &ViewAt<'_>,
        picks: Vec<ViewPick>,
        found: Vec<Found>,
        at: Option<P2>,
    ) -> Result<Chosen, String> {
        // The same pick twice is one pick.
        let mut kept: Vec<ViewPick> = Vec::new();
        let mut seen: Vec<Found> = Vec::new();
        for (pick, f) in picks.into_iter().zip(found) {
            if !kept.contains(&pick) {
                kept.push(pick);
                seen.push(f);
            }
        }
        let pictorial = view.pictorial_for(&kept);
        let clicked = at.map(|a| view.placed.map.view_point(a));
        // Between two picks: horizontal, vertical or aligned as the click
        // chooses; never between two picks on one point.
        let linear = |picks: Vec<ViewPick>, found: Vec<Found>| -> Result<Chosen, String> {
            let (a, b) = (found[0].seen.point(), found[1].seen.point());
            let apart = if pictorial {
                mitcad_vec3::distance(found[0].at, found[1].at)
            } else {
                (b[0] - a[0]).hypot(b[1] - a[1])
            };
            if apart < 1e-9 {
                return Err(
                    "the picks lie on one point: nothing to measure between them".to_owned(),
                );
            }
            let along = if pictorial {
                Along::Aligned
            } else {
                annotate::placed_along(a, b, clicked)
            };
            Ok(Chosen {
                kind: AnnotationType::Linear,
                picks,
                found,
                along: Some(along),
            })
        };
        let alone = |kind: AnnotationType, picks: Vec<ViewPick>, found: Vec<Found>| Chosen {
            kind,
            picks,
            found,
            along: None,
        };
        match kept.len() {
            1 => {
                let pick = kept[0].clone();
                let first = seen[0].seen;
                // A straight edge, or a piece of a section's cut line
                // (mitcad#1361).
                let has_length =
                    (pick.edge.is_some() || pick.on_cut.is_some()) && pick.point.is_none();
                match first {
                    Seen::Circle { arc: Some(_), .. } => {
                        Ok(alone(AnnotationType::Radius, kept, seen))
                    }
                    Seen::Circle { .. } | Seen::Axis { .. } => {
                        Ok(alone(AnnotationType::Diameter, kept, seen))
                    }
                    Seen::Line(..) if has_length => {
                        // Its length: from its start to its end.
                        let ends: Vec<ViewPick> = [PickPoint::Start, PickPoint::End]
                            .into_iter()
                            .map(|point| ViewPick {
                                point: Some(point),
                                ..pick.clone()
                            })
                            .collect();
                        let found = ends
                            .iter()
                            .map(|p| self.resolve_found(view, p))
                            .collect::<Result<Vec<_>, _>>()?;
                        linear(ends, found)
                    }
                    // A cut face picked whole (no point on its cut line):
                    // its line runs across its gaps.
                    Seen::Line(..) => Err(
                        "a cut face picked whole spans its gaps: pick one piece of its cut \
                         line, or a second edge or point to dimension it from"
                            .to_owned(),
                    ),
                    Seen::Point(_) => Err(
                        "a point alone measures nothing: pick a second edge, circle or point"
                            .to_owned(),
                    ),
                }
            }
            2 => {
                let lines = kept.iter().all(|p| p.point.is_none());
                let crossing = match (seen[0].seen, seen[1].seen) {
                    (Seen::Line(a1, b1), Seen::Line(a2, b2)) => {
                        let d1 = [b1[0] - a1[0], b1[1] - a1[1]];
                        let d2 = [b2[0] - a2[0], b2[1] - a2[1]];
                        let cross = d1[0] * d2[1] - d1[1] * d2[0];
                        cross.abs() > 1e-6 * d1[0].hypot(d1[1]) * d2[0].hypot(d2[1])
                    }
                    _ => false,
                };
                if lines && crossing && !pictorial {
                    Ok(alone(AnnotationType::Angle, kept, seen))
                } else {
                    linear(kept, seen)
                }
            }
            0 => Err("a dimension takes one pick or two".to_owned()),
            _ => Err(
                "a dimension takes one pick (a circle, an arc or a straight edge) or two; a \
                 baseline or a chain takes more"
                    .to_owned(),
            ),
        }
    }

    fn datum_taken(&self, letter: &str, drawing: DrawingUid) -> Option<String> {
        if let Some(datum) = self.datum_named(letter) {
            return Some(format!("datum {letter} exists: {}", datum.name));
        }
        self.drawing_with_datum(letter, Some(drawing))
            .map(|d| format!("datum {letter} exists: a datum label of {}", d.name))
    }

    fn balloons_for(&self, view: &ViewAt<'_>, levels: Levels, first: u32) -> Vec<ViewAnnotation> {
        let assembly = self.assembly();
        let depth = view.subject_depth;
        let candidates = self.candidates(view);
        let map = &view.placed.map;
        let mut done: Vec<RowKey> = Vec::new();
        // Each row's pick with its arrow's point on the sheet.
        let mut found: Vec<(ViewPick, P2, P2)> = Vec::new();
        for s in &self.bodies_seen(view) {
            let Some(key) = row_of(assembly, s.body, &s.path, levels, depth) else {
                continue;
            };
            if done.contains(&key) {
                continue;
            }
            // A line of the row's bodies, a visible one if there is one.
            let line_of = |visible: bool| {
                candidates.iter().find(|c| {
                    let point = c.found.seen.point();
                    c.pick.point.is_none()
                        && matches!(c.found.seen, Seen::Line(..))
                        && row_of(assembly, c.pick.body, c.pick.path(), levels, depth) == Some(key)
                        && view.sees(point)
                        && (!visible || view.on_visible(point))
                })
            };
            let Some(c) = line_of(true).or_else(|| line_of(false)) else {
                continue;
            };
            done.push(key);
            let point = c.found.seen.point();
            found.push((c.pick.clone(), point, map.at(point)));
        }
        // The highest arrow gets the top place of the column.
        found.sort_by(|a, b| b.2[1].total_cmp(&a.2[1]));
        let bounds = &view.placed.bounds;
        let (low, high) = (map.at(bounds.min), map.at(bounds.max));
        let right = low[0].max(high[0]) + BALLOON_COLUMN;
        let top = low[1].max(high[1]) - BALLOON_ROW / 2.0;
        found
            .into_iter()
            .enumerate()
            .map(|(k, (pick, at, tip))| {
                let mut balloon =
                    ViewAnnotation::new(first + k as u32, AnnotationType::Balloon, vec![pick]);
                balloon.at = Some(at);
                let row = top - BALLOON_ROW * k as f64;
                // A view with nothing drawn has no box: beside the arrows.
                balloon.text_offset = Some(if right.is_finite() && top.is_finite() {
                    [right - tip[0], row - tip[1]]
                } else {
                    [12.0, 10.0 + BALLOON_ROW * k as f64]
                });
                balloon
            })
            .collect()
    }

    fn add_text_note(
        &mut self,
        old: Drawing,
        c: AddAnnotationCommand,
        id: u32,
    ) -> Result<Value, ApiError> {
        if c.view.is_some() || !c.picks.is_empty() || !c.points.is_empty() || c.has_pen() {
            return Err(ApiError(
                "a text note is on the sheet: give its `sheet` and `at`, no picks, points or pen"
                    .to_owned(),
            ));
        }
        let si = c.sheet;
        if si >= old.sheets.len() {
            return Err(ApiError(format!("{} has no sheet {si}", old.name)));
        }
        let note = SheetNote {
            id,
            text: c.text.clone().unwrap_or_default(),
            at: c
                .at
                .ok_or_else(|| ApiError("a text note needs its place, `at`".to_owned()))?,
            height: c.height,
        };
        note.check().map_err(ApiError)?;
        let mut changed = old.clone();
        changed.sheets[si].notes.push(note);
        self.set_drawing(old.uid, format!("Add note {id} to {}", old.name), changed)?;
        Ok(json!({"drawing": old.uid, "annotation": id, "annotations": [id]}))
    }

    fn add_sketch_item(
        &mut self,
        old: Drawing,
        c: AddAnnotationCommand,
        id: u32,
    ) -> Result<Value, ApiError> {
        if !c.picks.is_empty()
            || c.along.is_some()
            || c.format.is_some()
            || c.tolerance.is_some()
            || c.letter.is_some()
            || c.frame.is_some()
            || c.finish.is_some()
            || c.series.is_some()
            || c.table.is_some()
            || c.balloons
        {
            return Err(ApiError(format!(
                "a {} is drawn outside the model: it takes its points (a text its place, \
                 `at`), its pen, text and height only",
                c.kind
            )));
        }
        let (si, map) = match c.view {
            Some(v) => {
                let (si, view) = old
                    .find_view(v)
                    .ok_or_else(|| ApiError(format!("{} has no view {v}", old.name)))?;
                (si, super::drawings::view_map(&old.sheets[si], view))
            }
            None => {
                if c.sheet >= old.sheets.len() {
                    return Err(ApiError(format!("{} has no sheet {}", old.name, c.sheet)));
                }
                (c.sheet, ViewMap::new(1.0, [0.0, 0.0], [0.0, 0.0]))
            }
        };
        if c.kind == "text" && c.has_pen() {
            return Err(ApiError("a text has no pen".to_owned()));
        }
        let item = sketch_item(
            &c.kind,
            id,
            &c.points,
            c.at,
            c.text.as_deref(),
            c.height,
            SketchPen::default().chosen(c.pen, c.width, c.pattern),
            &map,
        )?;
        item.check().map_err(ApiError)?;
        let mut changed = old.clone();
        let sheet = &mut changed.sheets[si];
        match c.view {
            Some(v) => {
                let i = sheet.view_index(v).expect("found above");
                sheet.views[i].sketch.push(item);
            }
            None => sheet.sketch.push(item),
        }
        self.set_drawing(
            old.uid,
            format!("Add {} {id} to {}", c.kind, old.name),
            changed,
        )?;
        Ok(json!({"drawing": old.uid, "annotation": id, "annotations": [id]}))
    }

    fn edit_sketch_item(
        &mut self,
        old: Drawing,
        c: &EditAnnotationCommand,
        si: usize,
        view: Option<u32>,
    ) -> Result<Value, ApiError> {
        let id = c.annotation;
        if c.picks.is_some()
            || c.along.is_some()
            || c.format.is_some()
            || c.tolerance.is_some()
            || c.letter.is_some()
            || c.frame.is_some()
            || c.finish.is_some()
            || c.line_offset.is_some()
            || c.text_offset.is_some()
            || c.table.is_some()
            || c.slide.is_some()
            || c.terminator.is_some()
        {
            return Err(ApiError(
                "a line, an arc or a text drawn outside the model has its points (a text its \
                 place), pen, text and height only"
                    .to_owned(),
            ));
        }
        let identity = ViewMap::new(1.0, [0.0, 0.0], [0.0, 0.0]);
        let map = match view.and_then(|v| old.sheets[si].view(v)) {
            Some(v) => super::drawings::view_map(&old.sheets[si], v),
            None => identity,
        };
        let mut changed = old.clone();
        let sheet = &mut changed.sheets[si];
        let list = match view {
            Some(v) => {
                let i = sheet.view_index(v).expect("found above");
                &mut sheet.views[i].sketch
            }
            None => &mut sheet.sketch,
        };
        let item = list.iter_mut().find(|k| k.id() == id).expect("found above");
        let kind = item.key();
        if let Some(points) = &c.points {
            let pen = match item {
                SketchItem::Line { pen, .. } | SketchItem::Arc { pen, .. } => *pen,
                SketchItem::Text { .. } => {
                    return Err(ApiError(
                        "a text has its place, `at`, not points".to_owned(),
                    ));
                }
            };
            *item = sketch_item(kind, id, points, None, None, None, pen, &map)?;
        }
        // Its pen: a name sets its width and pattern, `width` and `pattern`
        // each their own (mitcad#1447).
        if c.has_pen() {
            match item {
                SketchItem::Line { pen, .. } | SketchItem::Arc { pen, .. } => {
                    *pen = pen.chosen(c.pen, c.width, c.pattern);
                }
                SketchItem::Text { .. } => return Err(ApiError("a text has no pen".to_owned())),
            }
        }
        if let Some(words) = &c.text {
            match item {
                SketchItem::Text { text, .. } => text.clone_from(words),
                _ => return Err(ApiError(format!("a {kind} has no text"))),
            }
        }
        if let Some(own) = c.height {
            match item {
                SketchItem::Text { height, .. } => *height = own,
                _ => return Err(ApiError(format!("a {kind} has no letters"))),
            }
        }
        if let Some(place) = c.at {
            match item {
                SketchItem::Text { at, .. } => *at = map.view_point(place),
                _ => {
                    return Err(ApiError(format!(
                        "a {kind} is moved by `move` or given its `points`"
                    )));
                }
            }
        }
        if let Some(d) = c.shift {
            item.carried(
                &|p| {
                    let on = map.at(p);
                    map.view_point([on[0] + d[0], on[1] + d[1]])
                },
                0.0,
                1.0,
            );
        }
        item.check().map_err(ApiError)?;
        let moved_only = c.points.is_none()
            && !c.has_pen()
            && c.text.is_none()
            && c.height.is_none()
            && (c.at.is_some() || c.shift.is_some());
        let verb = if moved_only { "Move" } else { "Edit" };
        self.set_drawing(
            old.uid,
            format!("{verb} {kind} {id} of {}", old.name),
            changed,
        )?;
        Ok(json!({"drawing": old.uid, "annotation": id}))
    }

    fn pattern_of(&self, view: &ViewAt<'_>, pick: &ViewPick) -> Result<Vec<ViewPick>, String> {
        let found = self.resolve_found(view, pick)?;
        let Seen::Circle {
            center,
            radius,
            end_on: true,
            ..
        } = found.seen
        else {
            return Err("a center pattern starts from a circle the view sees end on".to_owned());
        };
        let tolerance = 1e-6 * radius.max(1.0);
        let mut centers = vec![center];
        let mut more: Vec<(P2, ViewPick)> = Vec::new();
        for c in self.candidates(view) {
            if c.pick.point.is_some()
                || c.pick.body != pick.body
                || c.pick.path() != pick.path()
                || c.pick.face.is_some() != pick.face.is_some()
            {
                continue;
            }
            let Seen::Circle {
                center: other,
                radius: r,
                end_on: true,
                ..
            } = c.found.seen
            else {
                continue;
            };
            if (r - radius).abs() > tolerance
                || !view.sees(other)
                || centers
                    .iter()
                    .any(|p| (p[0] - other[0]).hypot(p[1] - other[1]) < 1e-6)
            {
                continue;
            }
            centers.push(other);
            more.push((other, c.pick));
        }
        more.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])));
        let mut out = vec![pick.clone()];
        out.extend(
            more.into_iter()
                .map(|(_, p)| p)
                .take(annotate::MAX_PATTERN - 1),
        );
        Ok(out)
    }

    fn retrieved_dimensions(
        &self,
        view: &ViewAt<'_>,
        sketches: &[(FeatureUid, SketchDimension)],
        units: WordUnits,
        decimals: Option<u8>,
    ) -> Vec<((usize, [i64; 3]), Group)> {
        let placements = self.sketch_placements(ComponentUid::ROOT, &view.shown);
        let mut out = Vec::new();
        for (i, (sketch, dim)) in sketches.iter().enumerate() {
            for (_, transform) in placements.iter().filter(|(f, _)| f == sketch) {
                let placed = placed_dimension(dim, transform);
                if let Some(g) = retrieved_dimension(view, &placed, units, decimals) {
                    out.push((retrieved_key(i, &placed), g));
                }
            }
        }
        out
    }

    #[allow(clippy::type_complexity)]
    fn lay_out_annotation(
        &self,
        annotation: &ViewAnnotation,
        view: &ViewAt<'_>,
        units: WordUnits,
        datums: &[&str],
        number: Option<&str>,
        defaults: &DrawingDefaults,
    ) -> Result<(annotate::Laid, Option<f64>, String, Option<String>), String> {
        if view.pictorial_for(&annotation.picks)
            && let Some(why) = pictorial_refusal(annotation.kind, annotation.along)
        {
            return Err(why);
        }
        let found: Vec<Found> = annotation
            .picks
            .iter()
            .map(|p| self.resolve_found(view, p))
            .collect::<Result<_, _>>()?;
        let points = view.true_distance(annotation.kind, annotation.along, &annotation.picks);
        let seen: Vec<Seen> = found.iter().map(|f| shown_seen(points, f)).collect();
        let mut value = annotate::measure(annotation, &seen)?;
        // On a pictorial view an aligned dimension's value is the true
        // distance of its picks; on any view along no axis too.
        if points && annotation.kind == AnnotationType::Linear && found.len() == 2 {
            value = Some(mitcad_vec3::distance(found[0].at, found[1].at));
        }
        written(
            annotation, view, &seen, value, number, units, datums, defaults,
        )
    }

    fn lay_out_hole_table(
        &self,
        annotation: &ViewAnnotation,
        view: &ViewAt<'_>,
        units: Units,
        threads: &[super::drawing::ThreadOf],
        entry: &mut Value,
        scale: f64,
    ) -> Result<(annotate::Laid, String, Vec<String>), String> {
        // Its origin pick, found again: the origin follows the model.
        let origin = annotation
            .picks
            .first()
            .map(|pick| self.resolve_found(view, pick).map(|f| f.seen.point()))
            .transpose()?;
        let (data, warnings) = self.hole_table(annotation, origin, view, units, threads)?;
        table_entry(entry, annotation, true, data.rows.len());
        let label = if data.rows.is_empty() {
            "no holes".to_owned()
        } else {
            data.rows
                .iter()
                .map(|r| r.join(" "))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let place = annotation.table.as_ref().map_or([0.0; 2], |t| t.place);
        Ok((
            lay_out_table(annotation.id, "hole-table", &data, place, scale),
            label,
            warnings,
        ))
    }

    fn unlisted(&self, annotation: &ViewAnnotation) -> String {
        let what = annotation.picks.first().map_or_else(String::new, |pick| {
            match pick
                .path()
                .first()
                .and_then(|o| self.assembly().occurrence(*o))
            {
                Some(o) => self.assembly().name(o.component),
                None => self.body_name(pick.body),
            }
        });
        format!("no parts list on the sheet lists {what}")
    }

    fn automatic_marks(&self, view: &ViewAt<'_>) -> Vec<Group> {
        let spec = &view.placed.spec;
        let map = &view.placed.map;
        let mut circles: Vec<(P2, f64)> = Vec::new();
        let mut axes: Vec<(P2, P2)> = Vec::new();
        for s in &view.shown {
            let Ok(geometry) = self.body_geometry(s.body) else {
                continue;
            };
            for (_, curve) in &geometry.edges {
                let CurveGeometry::Circle {
                    center,
                    normal,
                    radius,
                    start,
                    end,
                } = placed_curve(curve, &s.transform)
                else {
                    continue;
                };
                let half = annotate::arc_sweep(center, normal, radius, start, end) >= PI - 1e-6;
                let n = mitcad_vec3::unit(normal).unwrap_or([0.0, 0.0, 1.0]);
                if !half
                    || mitcad_vec3::dot(n, spec.toward_eye).abs() < 1.0 - 1e-6
                    || !view.kept(center)
                {
                    continue;
                }
                let c = spec.project(center);
                if !view.sees(c) {
                    continue;
                }
                // Only a circle the view draws (mitcad#1286); points off
                // its ends, eight along it.
                if !spec.hidden {
                    let along: Vec<P2> = annotate::arc_points3(center, normal, radius, start, end)
                        .into_iter()
                        .skip(1)
                        .step_by(4)
                        .map(|p| spec.project(p))
                        .collect();
                    if !view.draws(&along, radius) {
                        continue;
                    }
                }
                match circles
                    .iter_mut()
                    .find(|(o, _)| (o[0] - c[0]).abs() < 1e-6 && (o[1] - c[1]).abs() < 1e-6)
                {
                    Some(found) => found.1 = found.1.max(radius),
                    None => circles.push((c, radius)),
                }
            }
            for face in &geometry.cylinders {
                if let Some((
                    Found {
                        seen: Seen::Axis { a, b, radius },
                        ..
                    },
                    sweep,
                )) = seen_round(view, &geometry, face, &s.transform)
                    && sweep >= PI - 1e-6
                    && view.sees(a)
                    && view.sees(b)
                    && (spec.hidden || draws_outline(view, a, b, radius))
                {
                    let same =
                        |p: P2, q: P2| (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6;
                    if !axes
                        .iter()
                        .any(|(p, q)| (same(*p, a) && same(*q, b)) || (same(*p, b) && same(*q, a)))
                    {
                        axes.push((a, b));
                    }
                }
            }
        }
        let mut out = Vec::new();
        for (c, r) in circles {
            let mut g = Group::new("center-mark");
            annotate::center_mark(&mut g, map.at(c), r * map.scale);
            out.push(g);
        }
        for (a, b) in axes {
            let mut g = Group::new("center-mark");
            annotate::centerline(&mut g, map.at(a), map.at(b), 0.0, 0.0);
            out.push(g);
        }
        out
    }

    fn center_mark_lines<'c>(
        &self,
        view: &ViewAt<'_>,
        candidates: &'c [Candidate],
    ) -> Vec<(&'c Candidate, [P2; 2])> {
        let map = &view.placed.map;
        let mut groups = if view.center_marks {
            self.automatic_marks(view)
        } else {
            Vec::new()
        };
        // A centre mark annotation's circle, as it is seen now.
        for pick in &view.marked {
            let rim = candidates.iter().find(|c| {
                c.pick.point.is_none()
                    && c.pick
                        == ViewPick {
                            point: None,
                            ..pick.clone()
                        }
            });
            if let Some(Candidate {
                found:
                    Found {
                        seen:
                            Seen::Circle {
                                center,
                                radius,
                                end_on: true,
                                ..
                            },
                        ..
                    },
                ..
            }) = rim
            {
                let mut g = Group::new("center-mark");
                annotate::center_mark(&mut g, map.at(*center), radius * map.scale);
                groups.push(g);
            }
        }
        // The centres a click picks, where they are on the sheet; a line
        // from one is a mark's (a centreline's run from its middle is not).
        let centres: Vec<(&Candidate, P2)> = candidates
            .iter()
            .filter(|c| c.pick.point == Some(PickPoint::Center))
            .map(|c| (c, map.at(c.found.seen.point())))
            .collect();
        let mut out = Vec::new();
        for g in &groups {
            for item in &g.items {
                let Item::Line { a, b, .. } = item else {
                    continue;
                };
                if let Some((c, _)) = centres
                    .iter()
                    .find(|(_, p)| (p[0] - a[0]).abs() < 1e-6 && (p[1] - a[1]).abs() < 1e-6)
                {
                    out.push((*c, [*a, *b]));
                }
            }
        }
        out
    }
}

/// Gives an annotation's entry in the `drawing` query its notice
/// (mitcad#1040): a broken annotation, or one laid out with a caveat, is a
/// warning about it (`level` and `notices`).
pub(crate) fn add_annotation_notice(
    entry: &mut Value,
    id: &impl ToString,
    message: &impl ToString,
) {
    let notice = Notice::warning(message.to_string()).with_target(id.to_string());
    entry["level"] = json!(notice.level.as_str());
    let mut notices = entry["notices"].as_array().cloned().unwrap_or_default();
    notices.push(notice.to_json());
    entry["notices"] = Value::Array(notices);
}

/// A line's, an arc's or a text's entry in the `drawing` query
/// (mitcad#729): its file form with its view (null on the sheet), its
/// words and its box; a line's or an arc's `width` and `pattern` always
/// (mitcad#1447: the file form leaves them out or gives the pen's name).
fn sketch_entry(item: &SketchItem, view: Option<u32>, laid: &annotate::Laid) -> Value {
    let mut entry = serde_json::to_value(item).unwrap_or(Value::Null);
    if let SketchItem::Line { pen, .. } | SketchItem::Arc { pen, .. } = item {
        entry["width"] = json!(pen.width);
        entry["pattern"] = json!(pen.pattern);
    }
    entry["view"] = json!(view);
    entry["broken"] = json!(false);
    entry["label"] = json!(item.label());
    entry["box"] = bounds_json(&laid.bounds);
    entry
}

/// A line, an arc or a text of a command (mitcad#729), its sheet points
/// taken into the coordinates `map` puts on the sheet: a line from its two
/// points, an arc about its first from the second counter-clockwise to the
/// third (a whole circle when the third lies the way the second does), a
/// text at `at`.
#[allow(clippy::too_many_arguments)]
fn sketch_item(
    kind: &str,
    id: u32,
    points: &[P2],
    at: Option<P2>,
    text: Option<&str>,
    height: Option<f64>,
    pen: SketchPen,
    map: &ViewMap,
) -> Result<SketchItem, ApiError> {
    let local = |p: P2| map.view_point(p);
    match kind {
        "line" => {
            let [from, to] = points else {
                return Err(ApiError(
                    "a line takes its two ends on the sheet, `points`".to_owned(),
                ));
            };
            Ok(SketchItem::Line {
                id,
                from: local(*from),
                to: local(*to),
                pen,
            })
        }
        "arc" => {
            let [center, start, end] = points else {
                return Err(ApiError(
                    "an arc takes its center, start and end on the sheet, `points`".to_owned(),
                ));
            };
            let (c, s, e) = (local(*center), local(*start), local(*end));
            let radius = (s[0] - c[0]).hypot(s[1] - c[1]);
            let from = (s[1] - c[1]).atan2(s[0] - c[0]);
            let mut to = (e[1] - c[1]).atan2(e[0] - c[0]);
            while to <= from + 1e-12 {
                to += TAU;
            }
            Ok(SketchItem::Arc {
                id,
                center: c,
                radius,
                start: from,
                end: to,
                pen,
            })
        }
        _ => {
            if !points.is_empty() {
                return Err(ApiError(
                    "a text has its place, `at`, not points".to_owned(),
                ));
            }
            let at = at.ok_or_else(|| ApiError("a text needs its place, `at`".to_owned()))?;
            Ok(SketchItem::Text {
                id,
                text: text.unwrap_or_default().to_owned(),
                at: local(at),
                height,
            })
        }
    }
}

/// A table's entry in the `drawing` query: the columns it shows, those it
/// can show (its type's own, and a parts list's property columns) with
/// their headers, and how many rows it has.
fn table_entry(entry: &mut Value, annotation: &ViewAnnotation, holes: bool, rows: usize) {
    let Some(table) = &annotation.table else {
        return;
    };
    let shown = table.shown_columns(holes);
    let own: &[&str] = if holes { &HOLE_COLUMNS } else { &PARTS_COLUMNS };
    let mut available: Vec<String> = own.iter().map(|c| (*c).to_owned()).collect();
    for column in &shown {
        if !available.contains(column) {
            available.push(column.clone());
        }
    }
    entry["columns"] = json!(shown);
    entry["available"] = json!(
        available
            .iter()
            .map(|c| json!({"column": c, "header": header(c)}))
            .collect::<Vec<_>>()
    );
    entry["sort"] = json!(table.sort_column(holes));
    entry["rows"] = json!(rows);
}

/// A pick as an annotation of a pictorial view takes it: its point, so
/// that an aligned dimension runs from one to the other; as seen elsewhere.
/// `points`: [`ViewAt::true_distance`].
fn shown_seen(points: bool, found: &Found) -> Seen {
    if points {
        match found.seen {
            Seen::Line(..) | Seen::Axis { .. } => Seen::Point(found.seen.point()),
            other => other,
        }
    } else {
        found.seen
    }
}

pub(super) fn bounds_json(b: &Bounds) -> Value {
    if b.is_empty() {
        Value::Null
    } else {
        json!([b.min, b.max])
    }
}

/// An annotation laid out on its view from what the view sees of its picks
/// and its value (measured on the view, a model annotation's own,
/// mitcad#1406, or a balloon's `number`): its words with the drawing's
/// decimals where it has none of its own (mitcad#729), a frame's cells,
/// the group, its words on one line and a warning of its words.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn written(
    annotation: &ViewAnnotation,
    view: &ViewAt<'_>,
    seen: &[Seen],
    value: Option<f64>,
    number: Option<&str>,
    units: WordUnits,
    datums: &[&str],
    defaults: &DrawingDefaults,
) -> Result<(annotate::Laid, Option<f64>, String, Option<String>), String> {
    let mut format = annotation.format.clone();
    if format.decimals.is_none() {
        format.decimals = defaults.decimals;
    }
    let (words, mut warning) = match (value, number) {
        (Some(v), _) => {
            let (words, warning) = value_text(
                annotation.kind.measure_kind(),
                &format,
                annotation.tolerance.as_ref(),
                v,
                units,
            );
            (Some(words), warning)
        }
        (None, Some(n)) => (
            Some(annotate::ValueText {
                text: n.to_owned(),
                main: n.to_owned(),
                ..annotate::ValueText::default()
            }),
            None,
        ),
        (None, None) => (None, None),
    };
    let (cells, text) = match (&annotation.frame, annotation.kind) {
        (Some(frame), AnnotationType::Frame) => {
            let (cells, text, frame_warning) = frame_words(frame, units, datums);
            warning = warning.or(frame_warning);
            (cells, text)
        }
        _ => (
            Vec::new(),
            match annotation.kind {
                AnnotationType::Note => annotation
                    .text
                    .as_deref()
                    .unwrap_or("")
                    .lines()
                    .map(str::trim_end)
                    .collect::<Vec<_>>()
                    .join(" / "),
                AnnotationType::Datum => annotation.letter.clone().unwrap_or_default(),
                AnnotationType::Finish => annotation
                    .finish
                    .as_ref()
                    .and_then(|f| f.text.clone())
                    .unwrap_or_default(),
                _ => words.as_ref().map(|w| w.text.clone()).unwrap_or_default(),
            },
        ),
    };
    let frame = view.frame_with(defaults.text(annotation.height));
    let laid = annotate::lay_out(annotation, seen, &frame, words.as_ref(), &cells)?;
    Ok((laid, value, text, warning))
}

/// How far apart (model mm) a model annotation's point and a pick of its
/// view may be and still be one point, when the annotation is kept as the
/// drawing's own (mitcad#1406).
const KEPT_NEAR: f64 = 1e-6;

/// A model annotation as a view brings it in (mitcad#1406): the view's
/// annotation of its type that draws it (a length a linear dimension,
/// horizontal, vertical or aligned as its ends lie in the view; a radius,
/// a diameter and an angle their own; a datum label, a feature control
/// frame and the other leaders, a hole callout among them, notes of the
/// model's words) and what the view sees of it. None where the view does
/// not show the bodies it is on (an annotation of construction geometry
/// alone has none), does not see its plane square on (a hole callout's
/// hole end on), or does not see where it is (behind a section's plane,
/// outside a detail's circle, in a break).
fn model_item_in_view(view: &ViewAt<'_>, item: &ModelItem) -> Option<(ViewAnnotation, Vec<Seen>)> {
    let model = &item.annotation;
    let shows = |pick: &MeasurePick| {
        pick.body.is_none_or(|body| {
            view.shown
                .iter()
                .any(|s| s.body == body && pick.occurrence.as_ref().is_none_or(|o| o.0 == s.path))
        })
    };
    if !model.picks().any(|p| p.body.is_some()) || !model.picks().all(shows) {
        return None;
    }
    let spec = &view.placed.spec;
    let square = |n: Vec3| mitcad_vec3::dot(spec.toward_eye, n).abs() > 1.0 - 1e-9;
    let seen_at = |p: Vec3| {
        let q = spec.project(p);
        (view.kept(p) && view.sees(q)).then_some(q)
    };
    let mut annotation = ViewAnnotation::new(0, AnnotationType::Note, Vec::new());
    let seen = match item.shape {
        ModelShape::Ends([p, q]) => {
            if !square(item.normal) {
                return None;
            }
            let (a, b) = (seen_at(p)?, seen_at(q)?);
            if (b[0] - a[0]).hypot(b[1] - a[1]) < 1e-9 {
                return None;
            }
            annotation.kind = AnnotationType::Linear;
            // Along the direction its dimension line runs in the model, as
            // the view sees it: level, plumb or neither.
            let o = spec.project(p);
            let t = spec.project(mitcad_vec3::add_scaled(p, item.x, 1.0));
            let d = mitcad_vec3::vec2::unit([t[0] - o[0], t[1] - o[1]])
                .unwrap_or([b[0] - a[0], b[1] - a[1]]);
            annotation.along = Some(if d[1].abs() <= 1e-6 * d[0].hypot(d[1]) {
                Along::Horizontal
            } else if d[0].abs() <= 1e-6 * d[0].hypot(d[1]) {
                Along::Vertical
            } else {
                Along::Aligned
            });
            vec![Seen::Point(a), Seen::Point(b)]
        }
        ModelShape::Circle {
            center,
            axis,
            radius,
        } => {
            if !square(item.normal) {
                return None;
            }
            let c = seen_at(center)?;
            annotation.kind = if model.kind == MeasureKind::Radius {
                AnnotationType::Radius
            } else {
                AnnotationType::Diameter
            };
            if square(axis) {
                vec![Seen::Circle {
                    center: c,
                    radius,
                    end_on: true,
                    across: [1.0, 0.0],
                    arc: None,
                }]
            } else {
                // Seen from the side: across its axis as the view sees it.
                let tip = spec.project(mitcad_vec3::add_scaled(center, axis, 1.0));
                let along = mitcad_vec3::vec2::unit([tip[0] - c[0], tip[1] - c[1]])?;
                vec![Seen::Circle {
                    center: c,
                    radius,
                    end_on: false,
                    across: [-along[1], along[0]],
                    arc: None,
                }]
            }
        }
        ModelShape::Arc { center, from, to } => {
            if !square(item.normal) {
                return None;
            }
            let c = seen_at(center)?;
            // Its rays as straight edges from its corner.
            let ray = |d: Vec3| spec.project(mitcad_vec3::add_scaled(center, d, 10.0));
            annotation.kind = AnnotationType::Angle;
            vec![Seen::Line(c, ray(from)), Seen::Line(c, ray(to))]
        }
        ModelShape::Point { at, axis } => {
            let facing = match axis {
                Some(axis) => square(axis),
                None => square(item.normal),
            };
            if !facing {
                return None;
            }
            let p = seen_at(at)?;
            match model.kind {
                MeasureKind::Datum => {
                    annotation.kind = AnnotationType::Datum;
                    annotation.letter = model.letter.clone();
                }
                MeasureKind::Frame => {
                    annotation.kind = AnnotationType::Frame;
                    annotation.frame = model.frame.clone();
                }
                _ => {
                    let text = item.lines.join("\n");
                    if text.trim().is_empty() {
                        return None;
                    }
                    annotation.text = Some(text);
                }
            }
            annotation.at = Some(p);
            vec![Seen::Point(p)]
        }
    };
    if annotation.kind.is_dimension() {
        annotation.format = model.format.clone();
        annotation.tolerance = model.tolerance.clone();
    }
    Some((annotation, seen))
}

/// A dimension retrieved from a sketch, told apart from the others on a
/// sheet (mitcad#1406: each drawn in one view): its sketch dimension's
/// index and where a placement puts its first point, to a micrometre's
/// thousandth.
fn retrieved_key(i: usize, dim: &SketchDimension) -> (usize, [i64; 3]) {
    let p = match dim {
        SketchDimension::Linear { a, .. } => *a,
        SketchDimension::Radial { center, .. } => *center,
    };
    (i, p.map(|v| (v * 1e6).round() as i64))
}
