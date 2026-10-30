// SPDX-License-Identifier: MIT
//! The layout of a sheet: the views in their places after the projection
//! method at a scale that fits (or the one given), their overall
//! dimensions, the diameters of round holes with leaders, the hole table
//! and the title block (ISO 5457 frame, ISO 7200 title block, ISO 129
//! dimensions, ISO 128 lines).
//!
//! Two steps: [`layout`] places an export's views in a grid at one scale,
//! and [`render`] draws placed views with their marks (section lines,
//! detail circles, breaks), the automatic annotations, the frame and the
//! title block. A drawing kept in the design places its views itself
//! (mitcad#707) and is drawn by [`render`] alone; [`compose`] is the two
//! together.

use std::collections::BTreeMap;
use std::f64::consts::{FRAC_1_SQRT_2, PI, TAU};

// The 2D helpers under names apart from the 3D `dot`, `cross` and `unit`.
use mitcad_vec3::vec2::{self, perp};
use mitcad_vec3::vec2::{add as add2, dot as dot2, norm as len2, scale as mul2, sub as sub2};
use mitcad_vec3::{cross, dot, unit_div as unit};

use super::clip::ViewMap;
use super::def::Annotations;
use super::sheet::{Anchor, Group, Item, Pen, Sheet, text_width};
use super::{
    CutPlane, DrawingOptions, Orientation, P2, Projection, RoundHole, ScaleChoice, SheetSize,
    Units, ViewCurve, ViewEdgeKind, ViewName, ViewProjection, ViewSpec, ViewSpline, scale_text,
};
use mitcad_model::datum::Vec3;
use mitcad_model::internal::drawing::DIMENSION_TEXT;

/// A view to lay out.
#[derive(Debug, Clone)]
pub struct ViewInput {
    /// None: the section view.
    pub name: Option<ViewName>,
    pub spec: ViewSpec,
    pub projection: ViewProjection,
}

/// A round hole with its thread's designation (the `threads` query).
#[derive(Debug, Clone)]
pub struct HoleInput {
    pub hole: RoundHole,
    pub thread: Option<String>,
}

/// A dimension of a sketch, in model coordinates, drawn in a view that
/// looks along its sketch's normal.
#[derive(Debug, Clone, PartialEq)]
pub enum SketchDimension {
    /// Between two points, measured along `along` (a sketch axis) or
    /// aligned with them.
    Linear {
        a: Vec3,
        b: Vec3,
        along: Option<Vec3>,
        text: Option<Vec3>,
        value: f64,
        normal: Vec3,
    },
    /// A circle's or arc's radius or diameter.
    Radial {
        center: Vec3,
        radius: f64,
        diameter: bool,
        text: Option<Vec3>,
        value: f64,
        normal: Vec3,
    },
}

#[derive(Debug, Clone, Default)]
pub struct SheetInput {
    /// The design's name.
    pub title: String,
    pub bodies: Vec<String>,
    pub date: String,
    pub views: Vec<ViewInput>,
    pub holes: Vec<HoleInput>,
    pub sketch_dimensions: Vec<SketchDimension>,
}

/// A laid out sheet and what it holds.
#[derive(Debug, Clone)]
pub struct Composed {
    pub sheet: Sheet,
    pub scale: f64,
    pub views: usize,
    pub dimensions: usize,
    pub holes: usize,
    pub hole_rows: usize,
    pub polygonal: bool,
    pub warnings: Vec<String>,
}

// Sheet lengths, mm.
const MARGIN_LEFT: f64 = 20.0; // ISO 5457: the filing margin
const MARGIN: f64 = 10.0;
const TITLE_WIDTH: f64 = 180.0;
const TITLE_HEIGHT: f64 = 32.0;
const CALLOUT_TEXT: f64 = 3.0;
const SMALL_TEXT: f64 = 2.5;
const LABEL_TEXT: f64 = 2.0;
pub(crate) const ARROW_LENGTH: f64 = 3.0;
// ISO 129-1: a closed filled arrowhead of 30 degrees.
const ARROW_HALF_WIDTH: f64 = 0.8;
pub(crate) const EXTENSION_GAP: f64 = 1.0;
pub(crate) const EXTENSION_BEYOND: f64 = 2.0;
const DIMENSION_OFFSET: f64 = 10.0;
// The room around a view for its dimensions (left, below) and leaders.
const ROOM_LEFT: f64 = 16.0;
const ROOM_BELOW: f64 = 16.0;
const ROOM_RIGHT: f64 = 14.0;
const ROOM_ABOVE: f64 = 14.0;
const HATCH_SPACING: f64 = 2.5;
const ROW_HEIGHT: f64 = 5.0;
/// Under the hole table: where its X and Y are measured from.
const TABLE_NOTE: [&str; 2] = ["X, Y FROM THE LOWER LEFT", "CORNER OF THE VIEW"];
/// How far a break line reaches beyond its view's box, mm.
const BREAK_BEYOND: f64 = 3.0;

/// ISO 5455's scales, largest first.
pub const SCALES: [f64; 15] = [
    50.0, 20.0, 10.0, 5.0, 2.0, 1.0, 0.5, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002, 0.001,
];

/// The frame's corners on a sheet of this size.
pub fn frame(width: f64, height: f64) -> (P2, P2) {
    ([MARGIN_LEFT, MARGIN], [width - MARGIN, height - MARGIN])
}

/// The title block's box on a sheet of this size.
pub fn title_box(width: f64, height: f64) -> (P2, P2) {
    let (min, max) = frame(width, height);
    (
        [max[0] - TITLE_WIDTH.min(max[0] - min[0]), min[1]],
        [max[0], min[1] + TITLE_HEIGHT],
    )
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: P2,
    pub max: P2,
}

impl Bounds {
    pub fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 2],
            max: [f64::NEG_INFINITY; 2],
        }
    }
    pub fn add(&mut self, p: P2) {
        for (i, v) in p.into_iter().enumerate() {
            self.min[i] = self.min[i].min(v);
            self.max[i] = self.max[i].max(v);
        }
    }
    pub fn is_empty(&self) -> bool {
        !(self.min[0] <= self.max[0] && self.min[1] <= self.max[1])
    }
    pub fn width(&self) -> f64 {
        self.max[0] - self.min[0]
    }
    pub fn height(&self) -> f64 {
        self.max[1] - self.min[1]
    }
    pub fn center(&self) -> P2 {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
        ]
    }
}

/// Whether the angle lies in the counter-clockwise range from `start` to
/// `end`.
fn in_sweep(angle: f64, start: f64, end: f64) -> bool {
    let t = (angle - start).rem_euclid(TAU);
    t <= end - start + 1e-12
}

/// The box of a view's edges and cut faces, in view coordinates.
pub fn view_bounds(projection: &ViewProjection) -> Bounds {
    let mut b = Bounds::empty();
    for edge in &projection.edges {
        match &edge.curve {
            ViewCurve::Line(p, q) => {
                b.add(*p);
                b.add(*q);
            }
            ViewCurve::Arc {
                center,
                radius,
                start,
                end,
            } => {
                let at = |a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
                b.add(at(*start));
                b.add(at(*end));
                for k in 0..4 {
                    let a = f64::from(k) * PI / 2.0;
                    if in_sweep(a, *start, *end) {
                        b.add(at(a));
                    }
                }
            }
            ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => {
                for p in points {
                    b.add(*p);
                }
            }
        }
    }
    for face in &projection.cuts {
        for lp in face {
            for p in lp {
                b.add(*p);
            }
        }
    }
    b
}

/// What a view is for the automatic annotations: an orthographic view
/// gets its overall sizes and tags the holes it sees end on; a pictorial
/// (isometric) view, a section and a detail get none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewRole {
    Orthographic,
    Pictorial,
    Section,
    Detail,
}

impl ViewRole {
    /// An export's view by its name (None: the section).
    pub fn of(name: Option<ViewName>) -> Self {
        match name {
            None => Self::Section,
            Some(ViewName::Iso) => Self::Pictorial,
            Some(_) => Self::Orthographic,
        }
    }
}

/// A view placed on a sheet.
#[derive(Debug, Clone)]
pub struct PlacedView {
    /// The id of its group (`view-front`).
    pub id: String,
    pub role: ViewRole,
    pub spec: ViewSpec,
    /// In view coordinates, cut to a detail's circle and its breaks.
    pub projection: ViewProjection,
    pub map: ViewMap,
    /// Its box in view coordinates.
    pub bounds: Bounds,
    /// Written under it (`SECTION A-A`).
    pub label: Option<String>,
    /// Its cut faces hatched.
    pub hatch: bool,
    /// Faces shaded under the lines, each a polygon in view coordinates
    /// with its grey (0 black, 1 white), the farthest first.
    pub shading: Vec<(Vec<P2>, f64)>,
    /// The holes of the bodies it shows (indices into the sheet's holes);
    /// all of them when None.
    pub holes: Option<Vec<usize>>,
}

impl PlacedView {
    /// Its box on the sheet.
    pub fn sheet_box(&self) -> Bounds {
        let mut b = Bounds::empty();
        if !self.bounds.is_empty() {
            b.add(self.map.at(self.bounds.min));
            b.add(self.map.at(self.bounds.max));
        }
        b
    }
}

/// A section's cutting plane drawn across a view.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionMark {
    /// The view it is drawn on (an index into the sheet's views).
    pub on: usize,
    pub plane: CutPlane,
    /// From and to along the line (view millimetres from the projection of
    /// the plane's origin); across the view's box when None.
    pub span: Option<[f64; 2]>,
    pub letter: String,
}

/// A detail's circle drawn on its parent.
#[derive(Debug, Clone, PartialEq)]
pub struct DetailMark {
    pub on: usize,
    /// View coordinates.
    pub center: P2,
    pub radius: f64,
    pub letter: String,
}

/// A sheet's size, orientation and projection method.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetSetup {
    pub size: SheetSize,
    pub orientation: Orientation,
    pub projection: Projection,
}

impl SheetSetup {
    pub fn of(options: &DrawingOptions) -> Self {
        Self {
            size: options.sheet,
            orientation: options.orientation,
            projection: options.projection,
        }
    }

    /// Width and height, mm.
    pub fn dimensions(&self) -> (f64, f64) {
        let (long, short) = self.size.landscape();
        match self.orientation {
            Orientation::Landscape => (long, short),
            Orientation::Portrait => (short, long),
        }
    }
}

/// The fields of the title block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleInfo {
    pub title: String,
    /// The drawing number; without one the block names the bodies.
    pub number: Option<String>,
    pub author: Option<String>,
    pub bodies: Vec<String>,
    pub date: String,
    /// The sheet's number of how many (`1/2`), for a drawing of several.
    pub page: Option<String>,
    /// A sheet template's frame, title block and revision table with their
    /// fields filled in (mitcad#707 phase 4): drawn instead of the simple
    /// title block above.
    pub template: Option<super::template::Filled>,
}

/// What a sheet shows.
#[derive(Debug, Clone, Default)]
pub struct SheetContent {
    pub views: Vec<PlacedView>,
    pub sections: Vec<SectionMark>,
    pub details: Vec<DetailMark>,
    /// In the coordinates of the views' model.
    pub holes: Vec<HoleInput>,
    pub sketch_dimensions: Vec<SketchDimension>,
    /// Drawn after the views and their marks: the annotations of a kept
    /// drawing's views and sheet (mitcad#707, `annotate`).
    pub annotations: Vec<Group>,
}

/// The unit vector along `a`, or the sheet's x axis when `a` has no length.
fn unit2(a: P2) -> P2 {
    vec2::unit(a).unwrap_or([1.0, 0.0])
}

/// A filled arrowhead whose tip is at `tip`, pointing along `direction`.
pub(crate) fn arrow(group: &mut Group, tip: P2, direction: P2) {
    let d = unit2(direction);
    let base = sub2(tip, mul2(d, ARROW_LENGTH));
    let side = mul2(perp(d), ARROW_HALF_WIDTH);
    group.items.push(Item::Fill {
        points: vec![tip, add2(base, side), sub2(base, side)],
    });
}

/// A text's baseline direction that reads from below or from the right
/// (ISO 129-1), and the side its text goes to.
pub(crate) fn upright(direction: P2) -> f64 {
    let mut angle = direction[1].atan2(direction[0]);
    if angle > PI / 2.0 + 1e-9 {
        angle -= PI;
    } else if angle <= -PI / 2.0 + 1e-9 {
        angle += PI;
    }
    angle
}

/// A linear dimension between `a` and `b` (sheet points) measured along
/// `direction`, its dimension line `offset` away on the side of
/// `perp(direction)` (negative: the other side).
fn linear_dimension(a: P2, b: P2, direction: P2, offset: f64, text: &str) -> Group {
    let mut g = Group::new("dimension");
    let d = unit2(direction);
    let n = perp(d);
    // The feet on the dimension line: both points moved across to it.
    let base = add2(a, mul2(n, offset));
    let along = dot2(sub2(b, a), d);
    let fa = base;
    let fb = add2(base, mul2(d, along));
    let side = if offset >= 0.0 { 1.0 } else { -1.0 };
    for (from, foot) in [(a, fa), (b, fb)] {
        let reach = dot2(sub2(foot, from), n) * side;
        if reach > EXTENSION_GAP {
            g.line(
                Pen::Thin,
                add2(from, mul2(n, side * EXTENSION_GAP)),
                add2(foot, mul2(n, side * EXTENSION_BEYOND)),
            );
        }
    }
    g.line(Pen::Thin, fa, fb);
    let length = along.abs();
    let towards = unit2(sub2(fb, fa));
    if length >= 2.0 * ARROW_LENGTH + 1.0 {
        arrow(&mut g, fa, mul2(towards, -1.0));
        arrow(&mut g, fb, towards);
    } else {
        // Too short: the arrows outside, pointing in.
        g.line(Pen::Thin, fa, sub2(fa, mul2(towards, ARROW_LENGTH + 2.0)));
        g.line(Pen::Thin, fb, add2(fb, mul2(towards, ARROW_LENGTH + 2.0)));
        arrow(&mut g, fa, towards);
        arrow(&mut g, fb, mul2(towards, -1.0));
    }
    let angle = upright(towards);
    let up = [-angle.sin(), angle.cos()];
    let middle = mul2(add2(fa, fb), 0.5);
    g.text(
        add2(middle, mul2(up, 1.0)),
        DIMENSION_TEXT,
        angle,
        Anchor::Middle,
        text,
    );
    g
}

/// The height of a callout's text lines.
fn callout_height(lines: usize) -> f64 {
    lines as f64 * CALLOUT_TEXT * 1.6
}

/// A leader from a circle to text lines on a shoulder at `knee`, the arrow
/// on the circle; the shoulder and the text run to the right when `side`
/// is positive, else to the left.
fn leader(center: P2, radius: f64, knee: P2, side: f64, lines: &[String]) -> Group {
    let mut g = Group::new("dimension");
    let d = unit2(sub2(knee, center));
    let on = add2(center, mul2(d, radius));
    let width = lines
        .iter()
        .map(|l| text_width(l, CALLOUT_TEXT))
        .fold(0.0, f64::max);
    g.line(Pen::Thin, on, knee);
    g.line(Pen::Thin, knee, add2(knee, [side * (width + 2.0), 0.0]));
    arrow(&mut g, on, mul2(d, -1.0));
    let (x, anchor) = if side > 0.0 {
        (knee[0] + 1.0, Anchor::Start)
    } else {
        (knee[0] - 1.0, Anchor::End)
    };
    for (i, line) in lines.iter().rev().enumerate() {
        let y = knee[1] + 1.0 + i as f64 * CALLOUT_TEXT * 1.6;
        g.text([x, y], CALLOUT_TEXT, 0.0, anchor, line.clone());
    }
    g
}

/// Hatching at 45 degrees of a face's loops (sheet points), even-odd.
fn hatch(group: &mut Group, loops: &[Vec<P2>]) {
    // Turned by -45 degrees the hatch lines are horizontal.
    let (c, s) = (FRAC_1_SQRT_2, -FRAC_1_SQRT_2);
    let turn = |p: P2| [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
    let back = |p: P2| [p[0] * c + p[1] * s, -p[0] * s + p[1] * c];
    let turned: Vec<Vec<P2>> = loops
        .iter()
        .map(|l| l.iter().map(|p| turn(*p)).collect())
        .collect();
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for p in turned.iter().flatten() {
        low = low.min(p[1]);
        high = high.max(p[1]);
    }
    if high.partial_cmp(&low) != Some(std::cmp::Ordering::Greater) {
        return;
    }
    let mut y = (low / HATCH_SPACING).floor() * HATCH_SPACING + HATCH_SPACING / 2.0;
    while y < high {
        let mut xs = Vec::new();
        for lp in &turned {
            for i in 0..lp.len() {
                let p = lp[i];
                let q = lp[(i + 1) % lp.len()];
                if (p[1] <= y) != (q[1] <= y) {
                    xs.push(p[0] + (y - p[1]) / (q[1] - p[1]) * (q[0] - p[0]));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            group.line(Pen::Hatch, back([pair[0], y]), back([pair[1], y]));
        }
        y += HATCH_SPACING;
    }
}

/// A view's place in the grid of views: (row from the top, column).
fn cell(name: Option<ViewName>, projection: super::Projection) -> (usize, usize) {
    use super::Projection::{FirstAngle, ThirdAngle};
    match (projection, name) {
        (ThirdAngle, Some(ViewName::Top)) => (0, 0),
        (ThirdAngle, Some(ViewName::Iso)) => (0, 1),
        (ThirdAngle, Some(ViewName::Front)) => (1, 0),
        (ThirdAngle, Some(ViewName::Right)) => (1, 1),
        (ThirdAngle, None) => (1, 2),
        (FirstAngle, Some(ViewName::Right)) => (0, 0),
        (FirstAngle, Some(ViewName::Front)) => (0, 1),
        (FirstAngle, None) => (0, 2),
        (FirstAngle, Some(ViewName::Iso)) => (1, 0),
        (FirstAngle, Some(ViewName::Top)) => (1, 1),
    }
}

/// The text lines of a hole's callout (without the count) and its row of
/// the hole table.
#[doc(hidden)]
pub struct HoleText {
    pub size: String,
    pub depth: String,
    pub extra: Option<String>,
    pub thread: Option<String>,
}

#[doc(hidden)]
pub fn hole_text(h: &HoleInput, units: Units) -> HoleText {
    let hole = &h.hole;
    let extra = match (hole.counterbore, hole.countersink) {
        (Some((d, depth)), _) => Some(format!(
            "CBORE Ø{} DEPTH {}",
            units.length(d),
            units.length(depth)
        )),
        (None, Some((d, angle))) => Some(format!(
            "CSK Ø{} x {}°",
            units.length(d),
            super::trim_number(angle.to_degrees(), 1)
        )),
        (None, None) => None,
    };
    HoleText {
        size: format!("Ø{}", units.length(hole.diameter)),
        depth: if hole.through {
            "THRU".to_owned()
        } else {
            units.length(hole.depth)
        },
        extra,
        thread: h.thread.clone(),
    }
}

/// How a hole callout writes its depths and its counterbore or countersink
/// (mitcad#902): in the exported drawing's words (`10`, `CBORE Ø11 DEPTH
/// 6.8`, `CSK Ø12 x 90°`), or in ISO 129-1's symbols as the annotations on
/// the model have them (`↧ 10`, `⌴ Ø11 ↧ 6.8`, `⌵ Ø12 × 90°`). `THRU` either
/// way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutStyle {
    Words,
    Symbols,
}

/// A hole callout's lines, one engine for the exported drawing and the
/// annotations on the model (mitcad#902): the count of equal holes (`4x`,
/// none for one), the thread's designation of a tapped hole or else `Ø`
/// and the diameter, then `THRU` or the depth; a tapped hole whose thread
/// ends before the hole does (`thread_depth`) has the thread's depth on the
/// first line and the drill's size and depth on the second; then the
/// counterbore or countersink.
pub fn hole_callout(
    h: &HoleInput,
    count: usize,
    thread_depth: Option<f64>,
    units: Units,
    style: CalloutStyle,
) -> Vec<String> {
    let hole = &h.hole;
    let symbols = style == CalloutStyle::Symbols;
    let depth = |d: f64| {
        if symbols {
            format!("↧ {}", units.length(d))
        } else {
            units.length(d)
        }
    };
    let through_or_depth = if hole.through {
        "THRU".to_owned()
    } else {
        depth(hole.depth)
    };
    let size = format!("Ø{}", units.length(hole.diameter));
    let count = if count > 1 {
        format!("{count}x ")
    } else {
        String::new()
    };
    let mut lines = Vec::new();
    match (&h.thread, thread_depth) {
        // The thread ends before the hole: its depth, then the drill's.
        (Some(thread), Some(t)) if hole.through || (t - hole.depth).abs() > 1e-6 => {
            lines.push(format!("{count}{thread} {}", depth(t)));
            lines.push(format!("{size} {through_or_depth}"));
        }
        (Some(thread), _) => lines.push(format!("{count}{thread} {through_or_depth}")),
        (None, _) => lines.push(format!("{count}{size} {through_or_depth}")),
    }
    let extra = match (hole.counterbore, hole.countersink, symbols) {
        (Some((d, depth)), _, false) => Some(format!(
            "CBORE Ø{} DEPTH {}",
            units.length(d),
            units.length(depth)
        )),
        (Some((d, depth)), _, true) => {
            Some(format!("⌴ Ø{} ↧ {}", units.length(d), units.length(depth)))
        }
        (None, Some((d, angle)), false) => Some(format!(
            "CSK Ø{} x {}°",
            units.length(d),
            super::trim_number(angle.to_degrees(), 1)
        )),
        (None, Some((d, angle)), true) => Some(format!(
            "⌵ Ø{} × {}°",
            units.length(d),
            super::trim_number(angle.to_degrees(), 1)
        )),
        (None, None, _) => None,
    };
    lines.extend(extra);
    lines
}

/// A callout of a hole size in a view.
struct Callout {
    view: usize,
    lines: Vec<String>,
    /// View coordinates of the hole it points at, and its radius.
    at: P2,
    radius: f64,
}

/// A hole as one view shows it.
struct ViewHole<'a> {
    input: &'a HoleInput,
    /// View coordinates of the axis.
    at: P2,
    tag: String,
}

/// The automatic annotations of views: the holes each orthographic view
/// sees end on with their tags, the callouts of their sizes and the rows
/// of the hole table with its columns.
struct Plan<'a> {
    tagged: Vec<(usize, Vec<ViewHole<'a>>)>,
    callouts: Vec<Callout>,
    table_rows: Vec<[String; 6]>,
    header: [String; 6],
    columns: [f64; 6],
    table_width: f64,
}

/// A view as the automatic annotations see it.
struct PlanView<'a> {
    role: ViewRole,
    spec: &'a ViewSpec,
    bounds: Bounds,
    /// The holes of the bodies it shows (indices); all when None.
    holes: Option<&'a [usize]>,
    /// Its breaks: a hole inside one is not shown.
    map: Option<&'a ViewMap>,
}

impl<'a> PlanView<'a> {
    fn of(view: &'a PlacedView) -> Self {
        Self {
            role: view.role,
            spec: &view.spec,
            bounds: view.bounds,
            holes: view.holes.as_deref(),
            map: Some(&view.map),
        }
    }
}

fn plan<'a>(
    views: &[PlanView<'_>],
    holes: &'a [HoleInput],
    annotations: &Annotations,
    units: Units,
) -> Plan<'a> {
    // The holes each orthographic view shows end on, tagged per view.
    let mut tagged: Vec<(usize, Vec<ViewHole<'_>>)> = Vec::new();
    let mut letter = b'A';
    for (index, view) in views.iter().enumerate() {
        if view.role != ViewRole::Orthographic {
            continue;
        }
        let eye = view.spec.toward_eye;
        let spec = view.spec;
        let mut shown: Vec<ViewHole<'_>> = holes
            .iter()
            .enumerate()
            .filter(|(i, _)| view.holes.is_none_or(|own| own.contains(i)))
            .map(|(_, h)| h)
            .filter(|h| {
                let along = dot(h.hole.direction, eye);
                along.abs() > 1.0 - 1e-6 && (h.hole.through || along < 0.0)
            })
            .filter(|h| {
                view.map
                    .is_none_or(|m| !m.in_break(spec.project(h.hole.opening)))
            })
            .map(|h| ViewHole {
                input: h,
                at: spec.project(h.hole.opening),
                tag: String::new(),
            })
            .collect();
        if shown.is_empty() {
            continue;
        }
        shown.sort_by(|a, b| {
            (a.at[0], a.at[1])
                .partial_cmp(&(b.at[0], b.at[1]))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (k, h) in shown.iter_mut().enumerate() {
            h.tag = format!("{}{}", letter as char, k + 1);
        }
        letter = letter.saturating_add(1);
        tagged.push((index, shown));
    }

    // The diameters of round holes, once per size ("4x Ø6.6 THRU"), each
    // on a leader from the first hole of its size in the first view that
    // shows it end on.
    let mut callouts: Vec<Callout> = Vec::new();
    if annotations.dimensions {
        let mut labelled: Vec<String> = Vec::new();
        for (index, holes) in &tagged {
            let mut sizes: Vec<(String, Vec<&ViewHole<'_>>)> = Vec::new();
            for h in holes {
                // A tapped hole by its thread, others by their diameter: the
                // engine of the model's hole callouts too (mitcad#902).
                let lines = hole_callout(h.input, 1, None, units, CalloutStyle::Words);
                let key = lines.join("\n");
                match sizes.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, members)) => members.push(h),
                    None => sizes.push((key, vec![h])),
                }
            }
            for (key, members) in sizes {
                if labelled.contains(&key) {
                    continue;
                }
                let mut lines: Vec<String> = key.split('\n').map(str::to_owned).collect();
                if members.len() > 1 {
                    lines[0] = format!("{}x {}", members.len(), lines[0]);
                }
                callouts.push(Callout {
                    view: *index,
                    lines,
                    at: members[0].at,
                    radius: members[0].input.hole.diameter / 2.0,
                });
                labelled.push(key);
            }
        }
    }

    // The hole table's columns and rows.
    let table_rows: Vec<[String; 6]> = if annotations.hole_table {
        tagged
            .iter()
            .flat_map(|(index, holes)| {
                let bounds = views[*index].bounds;
                holes.iter().map(move |h| {
                    let text = hole_text(h.input, units);
                    let mut size = text.size.clone();
                    if let Some(extra) = &text.extra {
                        size = format!("{size} {extra}");
                    }
                    [
                        h.tag.clone(),
                        units.length(h.at[0] - bounds.min[0]),
                        units.length(h.at[1] - bounds.min[1]),
                        size,
                        text.depth.clone(),
                        text.thread.clone().unwrap_or_else(|| "-".to_owned()),
                    ]
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let header = ["TAG", "X", "Y", "SIZE", "DEPTH", "THREAD"].map(str::to_owned);
    let mut columns = [0.0f64; 6];
    for row in std::iter::once(&header).chain(table_rows.iter()) {
        for (i, cell) in row.iter().enumerate() {
            columns[i] = columns[i].max(text_width(cell, SMALL_TEXT) + 3.0);
        }
    }
    // As wide as its note at least.
    let note_width = text_width(TABLE_NOTE[0], SMALL_TEXT) + 1.0;
    let sum: f64 = columns.iter().sum();
    if sum < note_width {
        columns[3] += note_width - sum;
    }
    let table_width: f64 = if table_rows.is_empty() {
        0.0
    } else {
        columns.iter().sum()
    };
    Plan {
        tagged,
        callouts,
        table_rows,
        header,
        columns,
        table_width,
    }
}

/// The views of an export with something to show, and their boxes.
pub fn shown_views<'a>(
    views: &'a [ViewInput],
    warnings: &mut Vec<String>,
) -> Vec<(&'a ViewInput, Bounds)> {
    views
        .iter()
        .filter_map(|v| {
            let b = view_bounds(&v.projection);
            if b.is_empty() {
                warnings.push(format!(
                    "the {} view shows nothing",
                    v.name.map_or("section", ViewName::as_str)
                ));
                None
            } else {
                Some((v, b))
            }
        })
        .collect()
}

/// Where an export's views go: each view's map (by the index of `views`),
/// the scale and the warnings.
#[derive(Debug, Clone)]
pub struct Layout {
    pub scale: f64,
    pub maps: Vec<Option<ViewMap>>,
    pub warnings: Vec<String>,
}

/// An export's views in a grid after the projection method, at a scale
/// that fits beside the hole table and the callouts (or the one given),
/// each view's box centred in its cell, the grid centred above the title
/// block.
pub fn layout(
    options: &DrawingOptions,
    units: Units,
    views: &[(&ViewInput, Bounds)],
    holes: &[HoleInput],
) -> Layout {
    let (width, height) = SheetSetup::of(options).dimensions();
    let (frame_min, frame_max) = frame(width, height);
    let mut warnings = Vec::new();
    let specs: Vec<PlanView<'_>> = views
        .iter()
        .map(|(v, b)| PlanView {
            role: ViewRole::of(v.name),
            spec: &v.spec,
            bounds: *b,
            holes: None,
            map: None,
        })
        .collect();
    let plan = plan(&specs, holes, &Annotations::of(options), units);
    // The room above a view for its callouts.
    let above = |view: usize| {
        let needed: f64 = plan
            .callouts
            .iter()
            .filter(|c| c.view == view)
            .map(|c| callout_height(c.lines.len()) + 2.0)
            .sum();
        if needed > 0.0 {
            ROOM_ABOVE.max(needed + 6.0)
        } else {
            ROOM_ABOVE
        }
    };

    // The area of the views.
    let table_width = plan.table_width;
    let area_min = [frame_min[0] + 4.0, frame_min[1] + TITLE_HEIGHT + 4.0];
    let area_max = [
        frame_max[0]
            - 4.0
            - if table_width > 0.0 {
                table_width + 6.0
            } else {
                0.0
            },
        frame_max[1] - 4.0,
    ];

    // The grid: rows and columns of the views present.
    let mut cells: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for (i, (view, _)) in views.iter().enumerate() {
        cells.insert(cell(view.name, options.projection), i);
    }
    let mut rows: Vec<usize> = cells.keys().map(|(r, _)| *r).collect();
    rows.sort_unstable();
    rows.dedup();
    let mut cols: Vec<usize> = cells.keys().map(|(_, c)| *c).collect();
    cols.sort_unstable();
    cols.dedup();
    let col_width: Vec<f64> = cols
        .iter()
        .map(|c| {
            cells
                .iter()
                .filter(|((_, cc), _)| cc == c)
                .map(|(_, &i)| views[i].1.width())
                .fold(0.0, f64::max)
        })
        .collect();
    let row_height: Vec<f64> = rows
        .iter()
        .map(|r| {
            cells
                .iter()
                .filter(|((rr, _), _)| rr == r)
                .map(|(_, &i)| views[i].1.height())
                .fold(0.0, f64::max)
        })
        .collect();
    let row_above: Vec<f64> = rows
        .iter()
        .map(|r| {
            cells
                .iter()
                .filter(|((rr, _), _)| rr == r)
                .map(|(_, &i)| above(i))
                .fold(ROOM_ABOVE, f64::max)
        })
        .collect();
    let room_x = (ROOM_LEFT + ROOM_RIGHT) * cols.len() as f64;
    let room_y = ROOM_BELOW * rows.len() as f64 + row_above.iter().sum::<f64>();
    let sum_w: f64 = col_width.iter().sum();
    let sum_h: f64 = row_height.iter().sum();
    let avail_w = area_max[0] - area_min[0];
    let avail_h = area_max[1] - area_min[1];
    let fit = {
        let sx = if sum_w > 1e-9 {
            (avail_w - room_x) / sum_w
        } else {
            f64::INFINITY
        };
        let sy = if sum_h > 1e-9 {
            (avail_h - room_y) / sum_h
        } else {
            f64::INFINITY
        };
        sx.min(sy)
    };
    let scale = match options.scale {
        ScaleChoice::Fixed(s) => {
            if s > fit * (1.0 + 1e-9) && !views.is_empty() {
                warnings.push(format!(
                    "at {} the views do not fit on the sheet",
                    scale_text(s)
                ));
            }
            s
        }
        ScaleChoice::Fit => match SCALES.iter().find(|&&s| s <= fit * (1.0 + 1e-9)) {
            Some(&s) => s,
            None => {
                let s = SCALES[SCALES.len() - 1];
                if !views.is_empty() {
                    warnings.push(format!(
                        "at {} the views do not fit on the sheet",
                        scale_text(s)
                    ));
                }
                s
            }
        },
    };

    // Each view's place: centred in its cell, the grid centred in the area.
    let col_sheet: Vec<f64> = col_width
        .iter()
        .map(|w| scale * w + ROOM_LEFT + ROOM_RIGHT)
        .collect();
    let row_sheet: Vec<f64> = row_height
        .iter()
        .zip(&row_above)
        .map(|(h, a)| scale * h + ROOM_BELOW + a)
        .collect();
    let total_w: f64 = col_sheet.iter().sum();
    let total_h: f64 = row_sheet.iter().sum();
    let left = area_min[0] + ((avail_w - total_w) / 2.0).max(0.0);
    let top = area_max[1] - ((avail_h - total_h) / 2.0).max(0.0);
    let mut maps: Vec<Option<ViewMap>> = vec![None; views.len()];
    for (&(r, c), &i) in &cells {
        let ci = cols.iter().position(|x| *x == c).unwrap_or(0);
        let ri = rows.iter().position(|x| *x == r).unwrap_or(0);
        let x =
            left + col_sheet[..ci].iter().sum::<f64>() + ROOM_LEFT + scale * col_width[ci] / 2.0;
        let y = top
            - row_sheet[..ri].iter().sum::<f64>()
            - row_above[ri]
            - scale * row_height[ri] / 2.0;
        maps[i] = Some(ViewMap::new(scale, views[i].1.center(), [x, y]));
    }
    Layout {
        scale,
        maps,
        warnings,
    }
}

/// The first orthographic view that sees a plane edge on: where the
/// cutting plane of a section without a parent is drawn.
pub fn edge_on_view(views: &[PlacedView], plane: &CutPlane) -> Option<usize> {
    views.iter().position(|v| {
        v.role == ViewRole::Orthographic && dot(v.spec.toward_eye, plane.normal).abs() < 1e-9
    })
}

pub fn compose(options: &DrawingOptions, units: Units, input: &SheetInput) -> Composed {
    let mut warnings = Vec::new();
    let shown = shown_views(&input.views, &mut warnings);
    let laid = layout(options, units, &shown, &input.holes);
    warnings.extend(laid.warnings);
    let mut views = Vec::new();
    for ((view, bounds), map) in shown.iter().zip(laid.maps) {
        let Some(map) = map else { continue };
        views.push(PlacedView {
            id: format!("view-{}", view.name.map_or("section", ViewName::as_str)),
            role: ViewRole::of(view.name),
            spec: view.spec.clone(),
            projection: view.projection.clone(),
            map,
            bounds: *bounds,
            label: view.name.is_none().then(|| "SECTION A-A".to_owned()),
            hatch: true,
            shading: Vec::new(),
            holes: None,
        });
    }
    // The section's cutting plane in a view that sees it edge on.
    let sections = views
        .iter()
        .find(|v| v.role == ViewRole::Section)
        .and_then(|v| v.spec.section)
        .and_then(|plane| {
            edge_on_view(&views, &plane).map(|on| SectionMark {
                on,
                plane,
                span: None,
                letter: "A".to_owned(),
            })
        })
        .into_iter()
        .collect();
    let content = SheetContent {
        views,
        sections,
        details: Vec::new(),
        holes: input.holes.clone(),
        sketch_dimensions: input.sketch_dimensions.clone(),
        annotations: Vec::new(),
    };
    let title = TitleInfo {
        title: input.title.clone(),
        bodies: input.bodies.clone(),
        date: input.date.clone(),
        ..TitleInfo::default()
    };
    render(
        &SheetSetup::of(options),
        &Annotations::of(options),
        units,
        &title,
        laid.scale,
        &content,
        warnings,
    )
}

/// The pen of an edge.
fn edge_pen(visible: bool, kind: ViewEdgeKind) -> Pen {
    match (visible, kind) {
        (false, _) => Pen::Hidden,
        (true, ViewEdgeKind::Smooth) => Pen::Thin,
        (true, _) => Pen::Thick,
    }
}

/// Draws placed views on a sheet: each view (shading, hatching, edges,
/// break lines) with its label, the section lines and detail circles, the
/// automatic annotations, the hole table, the frame and the title block
/// (`scale` the one it gives).
pub fn render(
    setup: &SheetSetup,
    annotations: &Annotations,
    units: Units,
    title: &TitleInfo,
    scale: f64,
    content: &SheetContent,
    mut warnings: Vec<String>,
) -> Composed {
    let (width, height) = setup.dimensions();
    let (frame_min, frame_max) = match &title.template {
        Some(filled) => filled.template.frame(width, height),
        None => frame(width, height),
    };
    // A template's block with its revision table's room (mitcad#722).
    let title_height = title.template.as_ref().map_or(TITLE_HEIGHT, |filled| {
        filled.template.title_block.height + filled.template.revision_room()
    });
    let mut groups = Vec::new();
    let views = &content.views;
    let specs: Vec<PlanView<'_>> = views.iter().map(PlanView::of).collect();
    let plan = plan(&specs, &content.holes, annotations, units);
    let hole_count: usize = plan.tagged.iter().map(|(_, h)| h.len()).sum();
    // Rows that fit beside the views, above the title block.
    let room_rows =
        ((frame_max[1] - frame_min[1] - title_height - 12.0) / ROW_HEIGHT).floor() as usize;
    let shown_rows = plan.table_rows.len().min(room_rows.saturating_sub(3));
    if shown_rows < plan.table_rows.len() {
        warnings.push(format!(
            "the hole table shows {shown_rows} of {} holes: the sheet has no room for more",
            plan.table_rows.len()
        ));
    }

    // The views.
    let mut polygonal = false;
    let mut dimensions = 0;
    for view in views {
        let place = &view.map;
        polygonal |= view.projection.polygonal;
        let mut g = Group::new("view");
        g.id = Some(view.id.clone());
        for (polygon, grey) in &view.shading {
            g.items.push(Item::Shade {
                points: polygon.iter().map(|p| place.at(*p)).collect(),
                grey: *grey,
            });
        }
        if view.hatch {
            for face in &view.projection.cuts {
                let loops: Vec<Vec<P2>> = face
                    .iter()
                    .map(|l| l.iter().map(|p| place.at(*p)).collect())
                    .collect();
                hatch(&mut g, &loops);
            }
        }
        for edge in &view.projection.edges {
            let pen = edge_pen(edge.visible, edge.kind);
            g.items.push(match &edge.curve {
                ViewCurve::Line(p, q) => Item::Line {
                    pen,
                    a: place.at(*p),
                    b: place.at(*q),
                },
                ViewCurve::Arc {
                    center,
                    radius,
                    start,
                    end,
                } => Item::Arc {
                    pen,
                    center: place.at(*center),
                    radius: radius * place.scale,
                    start: *start,
                    end: *end,
                },
                // A B-spline drawn as its polyline (mitcad#853).
                ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => {
                    Item::Polyline {
                        pen,
                        points: points.iter().map(|p| place.at(*p)).collect(),
                        closed: false,
                    }
                }
            });
        }
        if !view.bounds.is_empty() {
            break_lines(&mut g, view);
        }
        groups.push(g);
        if view.bounds.is_empty() {
            continue;
        }
        let lo = place.at(view.bounds.min);
        let hi = place.at(view.bounds.max);
        if let Some(text) = &view.label {
            let mut label = Group::new("view-label");
            label.text(
                [(lo[0] + hi[0]) / 2.0, lo[1] - 9.0],
                5.0,
                0.0,
                Anchor::Middle,
                text.clone(),
            );
            groups.push(label);
        }
        // Overall dimensions of the orthographic views.
        if annotations.dimensions && view.role == ViewRole::Orthographic {
            groups.push(linear_dimension(
                lo,
                [hi[0], lo[1]],
                [1.0, 0.0],
                -DIMENSION_OFFSET,
                &units.length(view.bounds.width()),
            ));
            groups.push(linear_dimension(
                lo,
                [lo[0], hi[1]],
                [0.0, 1.0],
                DIMENSION_OFFSET,
                &units.length(view.bounds.height()),
            ));
            dimensions += 2;
        }
    }

    // The sections' cutting planes and the details' circles.
    for mark in &content.sections {
        if let Some(g) = section_line(views, mark) {
            groups.push(g);
        }
    }
    for mark in &content.details {
        if let Some(view) = views.get(mark.on) {
            groups.push(detail_circle(view, mark));
        }
    }
    // The annotations placed on the views and the sheet.
    for g in &content.annotations {
        if g.class == "dimension" {
            dimensions += 1;
        }
        groups.push(g.clone());
    }

    // The callouts above their views, stacked, each shoulder pointing away
    // from the view's middle.
    let mut stacked: BTreeMap<usize, f64> = BTreeMap::new();
    for callout in &plan.callouts {
        let view = &views[callout.view];
        let place = &view.map;
        let lo = place.at(view.bounds.min);
        let hi = place.at(view.bounds.max);
        let middle = place.at(view.bounds.center());
        let rise = stacked.entry(callout.view).or_insert(4.0);
        let center = place.at(callout.at);
        let knee_y = hi[1] + *rise;
        let width = callout
            .lines
            .iter()
            .map(|l| text_width(l, CALLOUT_TEXT))
            .fold(0.0, f64::max)
            + 2.0;
        let knee_x = |side: f64| {
            (center[0] + side * (0.25 * (knee_y - center[1])).max(3.0))
                .clamp(lo[0] - 4.0, hi[0] + 4.0)
        };
        // Away from the view's middle, unless the text would leave the frame.
        let mut side = if center[0] >= middle[0] { 1.0 } else { -1.0 };
        if (side < 0.0 && knee_x(side) - width < frame_min[0] + 2.0)
            || (side > 0.0 && knee_x(side) + width > frame_max[0] - 2.0)
        {
            side = -side;
        }
        let knee = [knee_x(side), knee_y];
        *rise += callout_height(callout.lines.len()) + 2.0;
        groups.push(leader(
            center,
            callout.radius * place.scale,
            knee,
            side,
            &callout.lines,
        ));
        dimensions += 1;
    }

    // The holes' tags, below right of each, and the origin of the table's
    // X and Y: the view's lower left corner.
    for (index, holes) in &plan.tagged {
        let view = &views[*index];
        let place = &view.map;
        if !annotations.hole_table {
            continue;
        }
        for h in holes {
            let at = place.at(h.at);
            let r = h.input.hole.diameter / 2.0 * place.scale;
            let mut tag = Group::new("hole-tag");
            tag.id = Some(h.tag.clone());
            tag.text(
                [
                    at[0] + r * FRAC_1_SQRT_2 + 0.5,
                    at[1] - r * FRAC_1_SQRT_2 - 0.5 - SMALL_TEXT,
                ],
                SMALL_TEXT,
                0.0,
                Anchor::Start,
                h.tag.clone(),
            );
            groups.push(tag);
        }
        let corner = place.at(view.bounds.min);
        let mut datum = Group::new("hole-datum");
        datum.items.push(Item::Arc {
            pen: Pen::Thin,
            center: corner,
            radius: 1.2,
            start: 0.0,
            end: TAU,
        });
        datum.line(
            Pen::Thin,
            add2(corner, [-2.0, 0.0]),
            add2(corner, [2.0, 0.0]),
        );
        datum.line(
            Pen::Thin,
            add2(corner, [0.0, -2.0]),
            add2(corner, [0.0, 2.0]),
        );
        groups.push(datum);
    }

    // Sketch dimensions in the views that look along their sketches.
    if annotations.sketch_dimensions {
        for dim in &content.sketch_dimensions {
            let normal = match dim {
                SketchDimension::Linear { normal, .. } | SketchDimension::Radial { normal, .. } => {
                    *normal
                }
            };
            let target = views.iter().find(|v| {
                v.role == ViewRole::Orthographic
                    && dot(v.spec.toward_eye, normal).abs() > 1.0 - 1e-9
            });
            let Some(view) = target else {
                continue;
            };
            if let Some(g) = sketch_dimension(dim, &view.spec, &view.map, &sketch_text(dim, units))
            {
                groups.push(g);
                dimensions += 1;
            }
        }
    }

    // The hole table, top right.
    let mut hole_rows = 0;
    if shown_rows > 0 {
        let table_width = plan.table_width;
        let columns = plan.columns;
        let x0 = frame_max[0] - 4.0 - table_width;
        let mut y = frame_max[1] - 4.0;
        let mut table = Group::new("hole-table");
        table.text(
            [x0, y - 3.0],
            SMALL_TEXT,
            0.0,
            Anchor::Start,
            format!("HOLES ({})", units.symbol),
        );
        y -= 5.0;
        let bottom = y - ROW_HEIGHT * (shown_rows + 1) as f64;
        table.rect(Pen::Thin, [x0, bottom], [x0 + table_width, y]);
        let mut x = x0;
        for (i, w) in columns.iter().enumerate() {
            if i > 0 {
                table.line(Pen::Thin, [x, bottom], [x, y]);
            }
            table.text(
                [x + 1.5, y - ROW_HEIGHT + 1.5],
                SMALL_TEXT,
                0.0,
                Anchor::Start,
                plan.header[i].clone(),
            );
            x += w;
        }
        groups.push(table);
        for row in plan.table_rows.iter().take(shown_rows) {
            y -= ROW_HEIGHT;
            let mut g = Group::new("hole-row");
            g.id = Some(row[0].clone());
            g.line(Pen::Thin, [x0, y], [x0 + table_width, y]);
            let mut x = x0;
            for (i, w) in columns.iter().enumerate() {
                g.text(
                    [x + 1.5, y - ROW_HEIGHT + 1.5],
                    SMALL_TEXT,
                    0.0,
                    Anchor::Start,
                    row[i].clone(),
                );
                x += w;
            }
            groups.push(g);
            hole_rows += 1;
        }
        let mut note = Group::new("hole-table");
        for (i, line) in TABLE_NOTE.iter().enumerate() {
            note.text(
                [x0, y - ROW_HEIGHT - 3.5 - i as f64 * SMALL_TEXT * 1.5],
                SMALL_TEXT,
                0.0,
                Anchor::Start,
                *line,
            );
        }
        groups.push(note);
    }

    // The frame and the title block: a template's, or the simple one.
    match &title.template {
        Some(filled) => groups.extend(super::template::draw(filled, width, height)),
        None => {
            let mut frame_group = Group::new("frame");
            frame_group.rect(Pen::Frame, frame_min, frame_max);
            groups.push(frame_group);
            let (title_min, title_max) = title_box(width, height);
            groups.push(title_block(
                setup,
                units,
                title,
                scale,
                title_min,
                title_max[0],
            ));
        }
    }

    Composed {
        sheet: Sheet {
            width,
            height,
            title: title.title.clone(),
            groups,
        },
        scale,
        views: views.len(),
        dimensions,
        holes: hole_count,
        hole_rows,
        polygonal,
        warnings,
    }
}

/// The break lines of a view: a thin line with a zigzag across the view
/// at each side of each break, in the view's group.
fn break_lines(group: &mut Group, view: &PlacedView) {
    let place = &view.map;
    let b = view.sheet_box();
    for band in &place.breaks {
        let k = band.axis;
        let across = 1 - k;
        if band.from > view.bounds.max[k] || band.to < view.bounds.min[k] {
            continue;
        }
        // The two sides of the gap on the sheet.
        let mut at = [0.0; 2];
        at[across] = view.bounds.min[across];
        at[k] = band.from;
        let near = place.at(at)[k];
        let far = near + band.gap;
        for side in [near, far] {
            let (low, high) = (b.min[across] - BREAK_BEYOND, b.max[across] + BREAK_BEYOND);
            let middle = (low + high) / 2.0;
            let zig = 1.5;
            let along = [
                (low, 0.0),
                (middle - zig, 0.0),
                (middle - zig / 2.0, zig),
                (middle + zig / 2.0, -zig),
                (middle + zig, 0.0),
                (high, 0.0),
            ];
            let points = along
                .iter()
                .map(|(t, off)| {
                    let mut p = [0.0; 2];
                    p[k] = side + off;
                    p[across] = *t;
                    p
                })
                .collect();
            group.items.push(Item::Polyline {
                pen: Pen::Thin,
                points,
                closed: false,
            });
        }
    }
}

/// A section's cutting plane as a chain line across a view that sees it
/// edge on, thick at its ends, with arrows in the direction of sight and
/// the section's letter; None when the view does not see it edge on.
fn section_line(views: &[PlacedView], mark: &SectionMark) -> Option<Group> {
    let view = views.get(mark.on)?;
    let section = &mark.plane;
    if dot(view.spec.toward_eye, section.normal).abs() >= 1e-9 || view.bounds.is_empty() {
        return None;
    }
    let place = &view.map;
    let bounds = view.bounds;
    let along3 = cross(section.normal, view.spec.toward_eye);
    let along = unit2([
        dot(along3, view.spec.x_axis),
        dot(along3, view.spec.y_axis()),
    ]);
    let through = view.spec.project(section.origin);
    let (t0, t1) = match mark.span {
        Some([t0, t1]) => (t0, t1),
        None => {
            // The line's run across the view's box.
            let corners = [
                bounds.min,
                [bounds.max[0], bounds.min[1]],
                bounds.max,
                [bounds.min[0], bounds.max[1]],
            ];
            let ts: Vec<f64> = corners
                .iter()
                .map(|c| dot2(sub2(*c, through), along))
                .collect();
            (
                ts.iter().copied().fold(f64::INFINITY, f64::min),
                ts.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            )
        }
    };
    let a = place.at(add2(through, mul2(along, t0)));
    let b = place.at(add2(through, mul2(along, t1)));
    let a = sub2(a, mul2(along, 6.0));
    let b = add2(b, mul2(along, 6.0));
    let mut g = Group::new("section-line");
    g.line(Pen::Chain, a, b);
    g.line(Pen::Thick, a, add2(a, mul2(along, 5.0)));
    g.line(Pen::Thick, b, sub2(b, mul2(along, 5.0)));
    // The direction of sight: against the normal.
    let sight3 = section.normal.map(|v| -v);
    let sight = unit2([
        dot(sight3, view.spec.x_axis),
        dot(sight3, view.spec.y_axis()),
    ]);
    for end in [a, b] {
        let start = sub2(end, mul2(sight, 8.0));
        g.line(Pen::Thin, start, sub2(end, mul2(sight, 0.5)));
        arrow(&mut g, sub2(end, mul2(sight, 0.5)), sight);
        let label = sub2(start, mul2(sight, 1.5));
        g.text(
            [label[0], label[1] - DIMENSION_TEXT / 2.0],
            5.0,
            0.0,
            Anchor::Middle,
            mark.letter.clone(),
        );
    }
    Some(g)
}

/// A detail's circle on its parent, thin, with its letter beside it.
fn detail_circle(view: &PlacedView, mark: &DetailMark) -> Group {
    let mut g = Group::new("detail-circle");
    let center = view.map.at(mark.center);
    let radius = mark.radius * view.map.scale;
    g.items.push(Item::Arc {
        pen: Pen::Thin,
        center,
        radius,
        start: 0.0,
        end: TAU,
    });
    let at = add2(center, mul2([FRAC_1_SQRT_2, FRAC_1_SQRT_2], radius + 2.0));
    g.text(at, 5.0, 0.0, Anchor::Start, mark.letter.clone());
    g
}

/// A sketch dimension's value as File › Export Drawing writes it.
pub(crate) fn sketch_text(dim: &SketchDimension, units: Units) -> String {
    match dim {
        SketchDimension::Linear { value, .. } => units.length(value.abs()),
        SketchDimension::Radial {
            diameter: true,
            value,
            ..
        } => format!("Ø{}", units.length(*value)),
        SketchDimension::Radial { value, .. } => format!("R{}", units.length(*value)),
    }
}

/// A dimension of a sketch in a view that looks along its normal, its
/// value written `text`.
#[doc(hidden)]
pub fn sketch_dimension(
    dim: &SketchDimension,
    spec: &ViewSpec,
    place: &ViewMap,
    text: &str,
) -> Option<Group> {
    let to_sheet = |p: Vec3| place.at(spec.project(p));
    match dim {
        SketchDimension::Linear {
            a,
            b,
            along,
            text: at,
            ..
        } => {
            let pa = to_sheet(*a);
            let pb = to_sheet(*b);
            let direction = match along {
                Some(v) => [dot(*v, spec.x_axis), dot(*v, spec.y_axis())],
                None => sub2(pb, pa),
            };
            if len2(direction) < 1e-9 {
                return None;
            }
            let d = unit2(direction);
            let n = perp(d);
            // The dimension line through the text, or 8 mm off.
            let mut offset = match at {
                Some(t) => dot2(sub2(to_sheet(*t), pa), n),
                None => 8.0,
            };
            if offset.abs() < 6.0 {
                offset = if offset < 0.0 { -6.0 } else { 6.0 };
            }
            let mut g = linear_dimension(pa, pb, d, offset, text);
            g.class = "sketch-dimension";
            Some(g)
        }
        SketchDimension::Radial {
            center,
            radius,
            text: at,
            ..
        } => {
            let c = to_sheet(*center);
            let r = radius * place.scale;
            let direction = match at {
                Some(t) => unit2(sub2(to_sheet(*t), c)),
                None => [FRAC_1_SQRT_2, FRAC_1_SQRT_2],
            };
            let on = add2(c, mul2(direction, r));
            let end = add2(on, mul2(direction, 8.0));
            let mut g = Group::new("sketch-dimension");
            g.line(Pen::Thin, on, end);
            arrow(&mut g, on, mul2(direction, -1.0));
            let label = text.to_owned();
            let anchor = if direction[0] >= 0.0 {
                Anchor::Start
            } else {
                Anchor::End
            };
            let shift = if direction[0] >= 0.0 { 1.0 } else { -1.0 };
            g.text(
                [end[0] + shift, end[1] - DIMENSION_TEXT / 2.0],
                DIMENSION_TEXT,
                0.0,
                anchor,
                label,
            );
            Some(g)
        }
    }
}

/// A text fitted into a cell's width: smaller down to 2 mm, then cut.
pub(crate) fn fitted(text: &str, height: f64, width: f64) -> (String, f64) {
    let w = text_width(text, height);
    if w <= width {
        return (text.to_owned(), height);
    }
    let smaller = height * width / w;
    if smaller >= 2.0 {
        return (text.to_owned(), smaller);
    }
    let mut cut: String = text.to_owned();
    while !cut.is_empty() && text_width(&format!("{cut}..."), 2.0) > width {
        cut.pop();
    }
    (format!("{cut}..."), 2.0)
}

/// ISO 7200's title block: the title, the drawing number (or the bodies),
/// the scale, the units, the date, the sheet, the projection method and
/// who drew it (or where it was made).
fn title_block(
    setup: &SheetSetup,
    units: Units,
    title: &TitleInfo,
    scale: f64,
    min: P2,
    right: f64,
) -> Group {
    let mut g = Group::new("title-block");
    let x0 = min[0];
    let y0 = min[1];
    let w = right - x0;
    let top = y0 + TITLE_HEIGHT;
    g.rect(Pen::Frame, [x0, y0], [right, top]);
    let split = x0 + w * 2.0 / 3.0;
    let row1 = top - 12.0;
    let row2 = row1 - 10.0;
    g.line(Pen::Thin, [x0, row1], [right, row1]);
    g.line(Pen::Thin, [x0, row2], [right, row2]);
    g.line(Pen::Thin, [split, row2], [split, top]);
    let third = [x0 + w * 0.28, x0 + w * 0.5, x0 + w * 0.78];
    for x in third {
        g.line(Pen::Thin, [x, y0], [x, row2]);
    }
    let mut cell =
        |label: &str, value: &str, from: f64, to: f64, bottom: f64, cell_top: f64, size: f64| {
            g.text(
                [from + 1.5, cell_top - 1.5 - LABEL_TEXT],
                LABEL_TEXT,
                0.0,
                Anchor::Start,
                label,
            );
            let (text, h) = fitted(value, size, to - from - 3.0);
            g.text([from + 1.5, bottom + 2.0], h, 0.0, Anchor::Start, text);
        };
    cell("TITLE", &title.title, x0, split, row1, top, 5.0);
    cell("SCALE", &scale_text(scale), split, right, row1, top, 5.0);
    let (label, value) = match &title.number {
        Some(number) => ("DRAWING NO.", number.clone()),
        None => ("BODY", title.bodies.join(", ")),
    };
    cell(label, &value, x0, split, row2, row1, DIMENSION_TEXT);
    cell(
        "UNITS",
        units.symbol,
        split,
        right,
        row2,
        row1,
        DIMENSION_TEXT,
    );
    cell("DATE", &title.date, x0, third[0], y0, row2, DIMENSION_TEXT);
    let sheet = match &title.page {
        Some(page) => format!("{} {page}", setup.size.name()),
        None => setup.size.name().to_owned(),
    };
    cell(
        "SHEET",
        &sheet,
        third[0],
        third[1],
        y0,
        row2,
        DIMENSION_TEXT,
    );
    let method = match setup.projection {
        Projection::ThirdAngle => "THIRD ANGLE",
        Projection::FirstAngle => "FIRST ANGLE",
    };
    cell(
        &format!("PROJECTION: {method}"),
        "",
        third[1],
        third[2],
        y0,
        row2,
        DIMENSION_TEXT,
    );
    let (label, value) = match &title.author {
        Some(author) => ("DRAWN BY", author.as_str()),
        None => ("MADE WITH", "Mitcad"),
    };
    cell(label, value, third[2], right, y0, row2, DIMENSION_TEXT);
    projection_symbol(&mut g, setup.projection, [third[1], y0 + 3.5]);
    g
}

/// ISO 5456-2's symbol of the projection method: a truncated cone's side
/// view and its end view (the end view to the right of the side view in
/// third angle, to the left in first angle), from `at` (its left end, on
/// its axis) about 17 mm to the right.
pub(crate) fn projection_symbol(g: &mut Group, projection: Projection, at: P2) {
    let cy = at[1];
    let (cone_x, circles_x) = match projection {
        Projection::ThirdAngle => (at[0] + 3.0, at[0] + 12.0),
        Projection::FirstAngle => (at[0] + 11.0, at[0] + 3.0),
    };
    let r_big = 2.2;
    let r_small = 1.2;
    g.items.push(Item::Polyline {
        pen: Pen::Thin,
        points: vec![
            [cone_x, cy - r_big],
            [cone_x + 6.0, cy - r_small],
            [cone_x + 6.0, cy + r_small],
            [cone_x, cy + r_big],
        ],
        closed: true,
    });
    for r in [r_big, r_small] {
        g.items.push(Item::Arc {
            pen: Pen::Thin,
            center: [circles_x + r_big, cy],
            radius: r,
            start: 0.0,
            end: TAU,
        });
    }
}

/// The axes of a section view of the plane.
pub fn section_spec(plane: &CutPlane, hidden: bool, smooth: bool, exact: bool) -> ViewSpec {
    let (toward_eye, x_axis) = super::section_axes(plane.normal);
    ViewSpec {
        toward_eye,
        x_axis,
        hidden,
        smooth,
        exact,
        quick: true,
        section: Some(*plane),
        deflection: 0.01,
    }
}

/// The unit normal of a plane given by any normal.
pub fn cut_plane(origin: Vec3, normal: Vec3) -> Option<CutPlane> {
    unit(normal).map(|normal| CutPlane { origin, normal })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hole(diameter: f64, through: bool, depth: f64) -> RoundHole {
        RoundHole {
            face: None,
            opening: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, -1.0],
            diameter,
            through,
            depth,
            counterbore: None,
            countersink: None,
        }
    }

    #[test]
    fn hole_callouts_in_words_and_in_symbols() {
        // mitcad#902: one engine for the drawing's words and the model's
        // symbols.
        let mut plain = HoleInput {
            hole: hole(6.6, true, 10.0),
            thread: None,
        };
        plain.hole.counterbore = Some((11.0, 6.8));
        assert_eq!(
            hole_callout(&plain, 4, None, Units::MM, CalloutStyle::Words),
            ["4x Ø6.6 THRU", "CBORE Ø11 DEPTH 6.8"]
        );
        assert_eq!(
            hole_callout(&plain, 4, None, Units::MM, CalloutStyle::Symbols),
            ["4x Ø6.6 THRU", "⌴ Ø11 ↧ 6.8"]
        );
        let mut sunk = HoleInput {
            hole: hole(5.0, false, 12.0),
            thread: None,
        };
        sunk.hole.countersink = Some((10.0, std::f64::consts::FRAC_PI_2));
        assert_eq!(
            hole_callout(&sunk, 1, None, Units::MM, CalloutStyle::Words),
            ["Ø5 12", "CSK Ø10 x 90°"]
        );
        assert_eq!(
            hole_callout(&sunk, 1, None, Units::MM, CalloutStyle::Symbols),
            ["Ø5 ↧ 12", "⌵ Ø10 × 90°"]
        );
        // A tapped hole by its thread; one whose thread ends first has the
        // drill's size and depth below.
        let tapped = HoleInput {
            hole: hole(5.0, false, 15.0),
            thread: Some("M6x1 - 6H".to_owned()),
        };
        assert_eq!(
            hole_callout(&tapped, 1, Some(15.0), Units::MM, CalloutStyle::Symbols),
            ["M6x1 - 6H ↧ 15"]
        );
        assert_eq!(
            hole_callout(&tapped, 2, Some(12.0), Units::MM, CalloutStyle::Symbols),
            ["2x M6x1 - 6H ↧ 12", "Ø5 ↧ 15"]
        );
        assert_eq!(
            hole_callout(&tapped, 1, None, Units::MM, CalloutStyle::Words),
            ["M6x1 - 6H 15"]
        );
    }
}
