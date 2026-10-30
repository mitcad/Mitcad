// SPDX-License-Identifier: MIT
//! Drawings kept in the design (mitcad#707, `mitcad_model_drawing::def`,
//! `document/drawings.rs`; commands.md, "Drawings in the design"): the
//! commands that add, change and delete drawings, sheets and views, and
//! the `drawings` and `drawing` queries.
//!
//! A drawing keeps definitions only. Its views are worked out here
//! whenever they are asked for: each base view's subject is found in the
//! drawing's component at the timeline marker, the kernel projects it
//! ([`mitcad_model::Kernel::project_view`], kept by what it showed in the
//! document's projection cache), a detail cuts its parent's lines to its
//! circle and breaks take bands out ([`mitcad_model_drawing::clip`]), and the
//! sheet is drawn as an export is ([`mitcad_model_drawing::compose::render`]).
//! A view whose subject is gone is not drawn; the query says why.
//!
//! Points the application picks on a sheet come in sheet millimetres; the
//! commands turn them into the view's coordinates through its place,
//! scale and breaks, so the application keeps no geometry of its own.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::annotations::present;
use super::cut_sheets::PartEdit;
use super::dispatch::named_enum;
use super::drawing_annotations::{AddAnnotationCommand, EditAnnotationCommand};
use super::drawing_templates::{RevisionCommand, SaveTemplateCommand};
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::assembly::{Assembly, inverse};
use mitcad_model::datum::Vec3;
use mitcad_model::exchange::{BodyKind, MeshTolerance};
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::internal::document::deleted_views;
use mitcad_model::internal::fingerprint::Fingerprint;
use mitcad_model::kernel::Kernel;
use mitcad_model::transform::Transform;
use mitcad_model_build::prelude::*;
use mitcad_model_drawing::annotate::{turned, turned_with_view};
use mitcad_model_drawing::clip::{self, Band, ViewMap};
use mitcad_model_drawing::compose::{
    self, Bounds, Composed, DetailMark, HoleInput, PlacedView, SectionMark, SheetContent,
    SheetSetup, TitleInfo, ViewRole, view_bounds,
};
use mitcad_model_drawing::def::{
    Annotations, BreakDirection, Drawing, DrawingDefaults, DrawingSheet, DrawingView, Side,
    StandardView, Subject, ViewBreak, ViewDirection, ViewKind, ViewStyle, section_plane,
};
use mitcad_model_drawing::internal::Shading;
use mitcad_model_drawing::sheet::{Anchor, Item, Pen, Sheet};
use mitcad_model_drawing::template::SheetTemplate;
use mitcad_model_drawing::{
    CutPlane, DrawingOptions, Orientation, P2, Projection, SheetSize, Units, ViewProjection,
    ViewSpec, parse_scale, scale_text, today,
};
use mitcad_vec3::{cross, dot};

/// The title block's fields a command sets: left out, a field stays; `""`
/// clears it.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TitleFields {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    number: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    date: Option<String>,
}

/// `add_drawing`: a drawing in a component's Drawings folder (the root's
/// when left out), with one sheet, or the sheets File › Export Drawing
/// lays out (`layout`: its options and bodies).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddDrawingCommand {
    #[serde(default)]
    component: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    size: Option<SheetSize>,
    #[serde(default)]
    orientation: Option<Orientation>,
    #[serde(default)]
    projection: Option<Projection>,
    #[serde(default)]
    title_block: Option<TitleFields>,
    #[serde(default)]
    units: Option<String>,
    #[serde(default)]
    annotations: Option<Annotations>,
    #[serde(default)]
    layout: Option<Value>,
    /// Its first sheet's template: a built-in one's id (`iso`, `ansi`), a
    /// template's data, or `""` or null for the simple title block.
    #[serde(default, deserialize_with = "present")]
    template: Option<Option<Value>>,
    /// The decimals and the letters' height (mm) of its dimensions and
    /// annotations without their own (mitcad#729).
    #[serde(default)]
    decimals: Option<u8>,
    #[serde(default)]
    text_height: Option<f64>,
}

/// `edit_drawing`: its name, and the decimals and the letters' height of
/// its dimensions and annotations without their own (mitcad#729; `null`:
/// none of the drawing's).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditDrawingCommand {
    drawing: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, deserialize_with = "present")]
    decimals: Option<Option<u8>>,
    #[serde(default, deserialize_with = "present")]
    text_height: Option<Option<f64>>,
}

/// `add_drawing_sheet` and `edit_drawing_sheet`: a sheet's fields (`sheet`: where a new one
/// goes, the end when left out; the one to edit).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SheetCommand {
    drawing: String,
    #[serde(default)]
    sheet: Option<usize>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    size: Option<SheetSize>,
    #[serde(default)]
    orientation: Option<Orientation>,
    #[serde(default)]
    projection: Option<Projection>,
    #[serde(default)]
    title_block: Option<TitleFields>,
    /// `""` for the design's.
    #[serde(default)]
    units: Option<String>,
    #[serde(default)]
    annotations: Option<Annotations>,
    /// As `add_drawing`'s; a new sheet takes the last one's when left out.
    #[serde(default, deserialize_with = "present")]
    template: Option<Option<Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ViewType {
    Base,
    Projected,
    Section,
    Detail,
}

/// `add_drawing_view`: points in sheet millimetres.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddViewCommand {
    drawing: String,
    #[serde(default)]
    sheet: usize,
    #[serde(rename = "type")]
    kind: ViewType,
    /// A base view's: a standard view's name, `{"toward_eye", "x_axis"}`
    /// or `{"toward_eye", "up"}` (the camera's); the front when left out.
    #[serde(default)]
    direction: Option<Value>,
    #[serde(default)]
    subject: Option<Subject>,
    /// The view it comes from (projected, section and detail views).
    #[serde(default)]
    parent: Option<u32>,
    /// A projected view's side of its parent; from `at` when left out.
    #[serde(default)]
    side: Option<Side>,
    /// Where the view's middle goes (a detail's centre); beside its parent
    /// or in the sheet's middle when left out.
    #[serde(default)]
    at: Option<P2>,
    /// A section's line on its parent, two sheet points.
    #[serde(default)]
    line: Option<[P2; 2]>,
    /// A section seen the other way.
    #[serde(default)]
    flip: bool,
    #[serde(default)]
    letter: Option<String>,
    #[serde(default)]
    hatch: Option<bool>,
    /// A detail's circle on its parent: its centre and radius on the sheet.
    #[serde(default)]
    center: Option<P2>,
    #[serde(default)]
    radius: Option<f64>,
    /// `"1:2"`, `"2:1"` or a number; the parent's (a detail twice its
    /// parent's), or one that fits for a base view, when left out.
    #[serde(default)]
    scale: Option<Value>,
    #[serde(default)]
    style: Option<ViewStyle>,
    #[serde(default)]
    label: Option<String>,
    /// Centre marks and centrelines worked out from the model (on when
    /// left out).
    #[serde(default)]
    center_marks: Option<bool>,
    /// The sketch dimensions it sees square on (off when left out).
    #[serde(default)]
    retrieved_dimensions: Option<bool>,
}

/// A break picked on a view: two sheet points whose coordinates along the
/// view's axis (`direction`) bound the band taken out.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BreakInput {
    direction: BreakDirection,
    from: P2,
    to: P2,
    #[serde(default)]
    gap: Option<f64>,
}

/// `edit_drawing_view`: what is given changes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditViewCommand {
    drawing: String,
    view: u32,
    /// Its anchor's new place on the sheet.
    #[serde(default)]
    place: Option<P2>,
    /// Moved by this on the sheet (a drag).
    #[serde(default, rename = "move")]
    shift: Option<P2>,
    /// `"1:2"`, `"2:1"`, a number, or `"parent"` for its parent's (a base
    /// view's: one that fits the sheet, as `"fit"`).
    #[serde(default)]
    scale: Option<Value>,
    #[serde(default)]
    style: Option<ViewStyle>,
    /// `""` for the default.
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    direction: Option<Value>,
    #[serde(default)]
    subject: Option<Subject>,
    #[serde(default)]
    side: Option<Side>,
    #[serde(default)]
    letter: Option<String>,
    #[serde(default)]
    hatch: Option<bool>,
    /// A section seen the other way.
    #[serde(default)]
    flip: bool,
    /// A detail's circle: its centre and radius on the sheet.
    #[serde(default)]
    center: Option<P2>,
    #[serde(default)]
    radius: Option<f64>,
    /// The breaks, all of them (an empty list removes them).
    #[serde(default)]
    breaks: Option<Vec<BreakInput>>,
    #[serde(default)]
    add_break: Option<BreakInput>,
    #[serde(default)]
    aligned: Option<bool>,
    #[serde(default)]
    center_marks: Option<bool>,
    #[serde(default)]
    retrieved_dimensions: Option<bool>,
    /// How far it is turned on the sheet, degrees counter-clockwise
    /// (mitcad#729).
    #[serde(default)]
    rotation: Option<f64>,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum DrawingCommand {
        "add_drawing" => AddDrawing(Box<AddDrawingCommand>),
        "edit_drawing" => EditDrawing(Box<EditDrawingCommand>),
        "delete_drawing" => DeleteDrawing {
            drawing: String,
        },
        "add_drawing_sheet" => AddSheet(Box<SheetCommand>),
        "edit_drawing_sheet" => EditSheet(Box<SheetCommand>),
        "delete_drawing_sheet" => DeleteSheet {
            drawing: String,
            sheet: usize,
        },
        "add_drawing_view" => AddView(Box<AddViewCommand>),
        "edit_drawing_view" => EditView(Box<EditViewCommand>),
        "delete_drawing_view" => DeleteView {
            drawing: String,
            view: u32,
        },
        // Annotations on views and text notes (mitcad#707 phase 2,
        // `drawing_annotations.rs`).
        "add_drawing_annotation" => AddAnnotation(Box<AddAnnotationCommand>),
        "edit_drawing_annotation" => EditAnnotation(Box<EditAnnotationCommand>),
        "delete_drawing_annotation" => DeleteAnnotation {
            drawing: String,
            #[serde(default)]
            annotation: Option<u32>,
            #[serde(default)]
            annotations: Option<Vec<u32>>,
        },
        // Revisions and templates (mitcad#707 phase 4,
        // `drawing_templates.rs`).
        "add_drawing_revision" => AddRevision(Box<RevisionCommand>),
        "edit_drawing_revision" => EditRevision(Box<RevisionCommand>),
        "delete_drawing_revision" => DeleteRevision {
            drawing: String,
            revision: String,
        },
        "save_drawing_template" => SaveTemplate(Box<SaveTemplateCommand>),
    }
}

/// The `drawing` query: a drawing with each sheet's views; the one sheet
/// asked for (the first by default) drawn, with its items unless
/// `geometry` is false. `title`: the design's name, for the root's
/// drawings without a title of their own.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DrawingQuery {
    drawing: String,
    #[serde(default)]
    sheet: usize,
    #[serde(default = "yes")]
    geometry: bool,
    #[serde(default)]
    title: Option<String>,
}

fn yes() -> bool {
    true
}

/// The side of the box a view that cannot be drawn keeps on its sheet, mm.
const BROKEN_BOX: f64 = 30.0;

/// A body where a drawing's view shows it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Shown {
    pub(super) body: BodyUid,
    pub(super) name: String,
    /// From the body's component into the drawing's.
    pub(super) transform: Transform,
    /// The occurrences from the drawing's component down to the body's
    /// (empty: drawn where it is).
    pub(super) path: Vec<OccurrenceUid>,
}

/// A body placed under a drawing's component.
struct Placement {
    /// The occurrences from the drawing's component down to the body's.
    path: Vec<OccurrenceUid>,
    body: BodyUid,
    name: String,
    transform: Transform,
}

/// A view worked out: as it is placed, and the bodies it shows (indices
/// into [`Evaluated::shown`]).
type Worked = (PlacedView, Vec<usize>);

/// A sheet's views worked out.
pub(super) struct Evaluated {
    pub(super) views: Vec<PlacedView>,
    /// The index in `views` of each view drawn.
    pub(super) index_of: BTreeMap<u32, usize>,
    /// Why a view is not drawn.
    pub(super) errors: BTreeMap<u32, String>,
    /// The bodies its views show, each once, in the order they come.
    pub(super) shown: Vec<Shown>,
    /// The bodies each view drawn shows (indices into `shown`).
    pub(super) view_shown: BTreeMap<u32, Vec<usize>>,
}

/// A sheet drawn.
pub(super) struct SheetDrawn {
    pub(super) composed: Composed,
    pub(super) evaluated: Evaluated,
    /// The annotations of its views and its text notes as the `drawing`
    /// query lists them.
    pub(super) annotations: Vec<Value>,
    /// Its title block's fields as drawn: each label with its text.
    pub(super) title_fields: Vec<Value>,
}

/// A scale a command asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ScaleInput {
    /// After its parent (`"parent"` or null).
    Parent,
    /// One of ISO 5455's that fits the sheet (`"fit"`, a base view's).
    Fit,
    Given(f64),
}

/// A scale in a command: `"1:2"`, `"2:1"` or a number; `"parent"` or null
/// for the parent's; `"fit"` for one that fits the sheet.
fn scale_value(value: &Value) -> Result<ScaleInput, ApiError> {
    let scale = match value {
        Value::Null => return Ok(ScaleInput::Parent),
        Value::String(s) if s == "parent" => return Ok(ScaleInput::Parent),
        Value::String(s) if s == "fit" => return Ok(ScaleInput::Fit),
        Value::String(s) => parse_scale(s),
        Value::Number(n) => n.as_f64(),
        _ => None,
    };
    match scale {
        Some(s) if s.is_finite() && s > 0.0 => Ok(ScaleInput::Given(s)),
        _ => Err(ApiError(format!(
            "a scale is \"1:2\", \"2:1\", a positive number, \"parent\" or \"fit\", not {value}"
        ))),
    }
}

/// A base view's direction in a command.
fn direction_value(value: &Value) -> Result<ViewDirection, ApiError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Looking {
        toward_eye: [f64; 3],
        up: [f64; 3],
    }
    if let Ok(looking) = serde_json::from_value::<Looking>(value.clone()) {
        return ViewDirection::looking(looking.toward_eye, looking.up).ok_or_else(|| {
            ApiError("a view's direction and its up direction must not be parallel".to_owned())
        });
    }
    serde_json::from_value(value.clone()).map_err(|e| ApiError(format!("invalid direction: {e}")))
}

/// Whether a view looks along an axis of its component's coordinates (an
/// orthographic view) rather than from an angle.
pub(super) fn along_an_axis(spec: &ViewSpec) -> bool {
    spec.toward_eye.iter().any(|v| (v.abs() - 1.0).abs() < 1e-9)
}

/// Whether the occurrences of a path lead down from a component.
fn path_from(assembly: &Assembly, component: ComponentUid, path: &[OccurrenceUid]) -> bool {
    let mut at = component;
    for o in path {
        match assembly.occurrence(*o) {
            Some(occurrence) if occurrence.parent == at => at = occurrence.component,
            _ => return false,
        }
    }
    true
}

/// Whether `MITCAD_DRAWING_TRACE` asks for the drawings' trace (mitcad#962):
/// `drawing-trace:` lines on standard error, as the kernel prints one per
/// projected view, about the kernel's projections (the bodies placed, the
/// kernel's milliseconds), each sheet's views worked out (how many were
/// projected and how many taken from the projection cache) and the phases
/// of drawing a sheet and adding a view.
pub(super) fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("MITCAD_DRAWING_TRACE").is_some())
}

thread_local! {
    /// The kernel's projections made on this thread and their
    /// milliseconds, for the trace's line per sheet.
    static PROJECTIONS: std::cell::Cell<(u64, f64)> = const { std::cell::Cell::new((0, 0.0)) };
}

/// The phases of one step for the trace, printed when it ends.
pub(super) struct Phases {
    what: Option<String>,
    start: std::time::Instant,
    last: std::time::Instant,
    line: String,
}

impl Phases {
    pub(super) fn new(what: impl FnOnce() -> String) -> Self {
        let now = std::time::Instant::now();
        Self {
            what: tracing().then(what),
            start: now,
            last: now,
            line: String::new(),
        }
    }

    pub(super) fn mark(&mut self, phase: &str) {
        if self.what.is_some() {
            let now = std::time::Instant::now();
            let ms = (now - self.last).as_secs_f64() * 1e3;
            self.line.push_str(&format!(" {phase}={ms:.1}"));
            self.last = now;
        }
    }
}

impl Drop for Phases {
    fn drop(&mut self) {
        if let Some(what) = &self.what {
            let ms = self.start.elapsed().as_secs_f64() * 1e3;
            eprintln!("drawing-trace: {what} total={ms:.1} ms{}", self.line);
        }
    }
}

/// The cache key of a view's projection or shading.
pub(super) fn view_key(tag: &str, shown: &[(BodyUid, u128, Transform)], spec: &ViewSpec) -> u128 {
    let mut fp = Fingerprint::new(tag);
    for v in spec.toward_eye.iter().chain(&spec.x_axis) {
        fp.f64(*v);
    }
    fp.bool(spec.hidden)
        .bool(spec.smooth)
        .bool(spec.exact)
        .bool(spec.quick)
        .f64(spec.deflection);
    fp.option(spec.section.as_ref(), |fp, plane| {
        for v in plane.origin.iter().chain(&plane.normal) {
            fp.f64(*v);
        }
    });
    fp.u64(shown.len() as u64);
    for (body, version, t) in shown {
        fp.str(&body.to_string()).u128(*version);
        for row in &t.linear {
            for v in row {
                fp.f64(*v);
            }
        }
        for v in &t.translation {
            fp.f64(*v);
        }
    }
    fp.finish()
}

/// The area of a sheet the views go in: inside the frame, above the title
/// block and its revision table's room (mitcad#722).
fn view_area(sheet: &DrawingSheet) -> (P2, P2) {
    let (width, height) = sheet.dimensions();
    let ((min, max), (_, title_max), revisions) = match &sheet.template {
        Some(t) => (
            t.frame(width, height),
            t.title_box(width, height),
            t.revision_room(),
        ),
        None => (
            compose::frame(width, height),
            compose::title_box(width, height),
            0.0,
        ),
    };
    (
        [min[0] + 4.0, title_max[1] + revisions + 4.0],
        [max[0] - 4.0, max[1] - 4.0],
    )
}

/// A view's map onto its sheet from its definition alone.
pub(super) fn view_map(sheet: &DrawingSheet, view: &DrawingView) -> ViewMap {
    ViewMap {
        scale: sheet.scale(view.id),
        anchor: sheet.anchor(view.id),
        place: view.place,
        breaks: view
            .breaks
            .iter()
            .map(|b| Band {
                axis: b.direction.axis(),
                from: b.from,
                to: b.to,
                gap: b.gap,
            })
            .collect(),
    }
}

/// A break picked on the sheet as the view's band.
fn break_of(map: &ViewMap, input: &BreakInput) -> Result<ViewBreak, ApiError> {
    let k = input.direction.axis();
    let a = map.view_point(input.from)[k];
    let b = map.view_point(input.to)[k];
    let gap = input.gap.unwrap_or(mitcad_model_drawing::def::BREAK_GAP);
    if !(a.is_finite() && b.is_finite()) || (a - b).abs() < 1e-9 {
        return Err(ApiError(
            "a break runs from one place to another".to_owned(),
        ));
    }
    Ok(ViewBreak {
        direction: input.direction,
        from: a.min(b),
        to: a.max(b),
        gap,
    })
}

/// The parent a new view names.
fn parent_view(sheet: &DrawingSheet, parent: Option<u32>) -> Result<&DrawingView, ApiError> {
    let parent = parent.ok_or_else(|| ApiError("the view needs a parent".to_owned()))?;
    sheet
        .view(parent)
        .ok_or_else(|| ApiError(format!("there is no view {parent} on the sheet")))
}

/// A view's frame: its x and y axes and the direction to the eye.
fn frame_of((e, x): (Vec3, Vec3)) -> [Vec3; 3] {
    [x, cross(e, x), e]
}

/// Whether two vectors are the same.
fn same_vector(a: Vec3, b: Vec3) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

/// A view's breaks carried over a quarter turn (`quarters` of 90 degrees
/// counter-clockwise) of its coordinates: a band along x becomes one
/// along y and the other way, mirrored where the axis turned over.
fn turned_breaks(breaks: &mut [ViewBreak], quarters: i64) {
    for b in breaks {
        let flip = |b: &mut ViewBreak| {
            (b.from, b.to) = (-b.to, -b.from);
        };
        match (quarters.rem_euclid(4), b.direction) {
            (1, BreakDirection::Horizontal) => b.direction = BreakDirection::Vertical,
            (1, BreakDirection::Vertical) => {
                b.direction = BreakDirection::Horizontal;
                flip(b);
            }
            (2, _) => flip(b),
            (3, BreakDirection::Horizontal) => {
                b.direction = BreakDirection::Vertical;
                flip(b);
            }
            (3, BreakDirection::Vertical) => b.direction = BreakDirection::Horizontal,
            _ => {}
        }
    }
}

/// Carries what views keep in their coordinates over a change of their
/// axes (mitcad#729: a view turned on the sheet, a base view's direction,
/// a projected view's side, a section looking the other way, the sheet's
/// projection method), parents first, so that the views from a view
/// changed and their annotations follow it:
///
/// - a view turned in its own plane (its eye the same): its annotations,
///   what is drawn in it, its breaks (by quarter turns only) and the
///   circles of its details turn with it;
/// - a view that looks along another direction: what is kept on it stays
///   where it is on it: a section drawn on it is turned with it, so that
///   its line stays where it was drawn (the section then cuts what the
///   view shows there now), a detail keeps its circle; annotations follow
///   their picks, and are broken where the view no longer sees them.
fn follow_turns(before: &DrawingSheet, after: &mut DrawingSheet) -> Result<(), ApiError> {
    for i in 0..after.views.len() {
        let id = after.views[i].id;
        // A view the sheet did not have, or that could not be worked out.
        let Ok(old) = before.axes(id) else {
            continue;
        };
        // A section on a view that turned to another direction: its plane
        // turned with the view, its line where it is on it.
        if let ViewKind::Section {
            parent: Some(parent),
            ..
        } = after.views[i].kind
            && let (Ok(po), Ok(pn)) = (before.axes(parent), after.axes(parent))
            && !same_vector(po.0, pn.0)
        {
            let (from, to) = (frame_of(po), frame_of(pn));
            let carry = |v: Vec3| -> Vec3 {
                let c = [dot(v, from[0]), dot(v, from[1]), dot(v, from[2])];
                std::array::from_fn(|k| c[0] * to[0][k] + c[1] * to[1][k] + c[2] * to[2][k])
            };
            if let ViewKind::Section { origin, normal, .. } = &mut after.views[i].kind {
                *origin = carry(*origin);
                *normal = carry(*normal);
            }
        }
        let Ok(new) = after.axes(id) else {
            continue;
        };
        if !same_vector(old.0, new.0) || same_vector(old.1, new.1) {
            continue;
        }
        // Turned in its own plane: what it shows turns `angle` degrees.
        let y = cross(old.0, old.1);
        let angle = (-dot(new.1, y)).atan2(dot(new.1, old.1)).to_degrees();
        let quarters = (angle / 90.0).round();
        let view = &mut after.views[i];
        if !view.breaks.is_empty() {
            if (angle - 90.0 * quarters).abs() > 1e-6 {
                return Err(ApiError(format!(
                    "view {id} has breaks: it turns by quarter turns only while it has them \
                     (Remove Breaks first)"
                )));
            }
            turned_breaks(&mut view.breaks, quarters as i64);
        }
        for annotation in &mut view.annotations {
            turned_with_view(annotation, angle);
        }
        for item in &mut view.sketch {
            item.carried(&|p| turned(p, Some(angle)), angle.to_radians(), 1.0);
        }
        // Its details keep showing what they showed.
        for detail in &mut after.views {
            if let ViewKind::Detail { parent, center, .. } = &mut detail.kind
                && *parent == id
            {
                *center = turned(*center, Some(angle));
            }
        }
    }
    Ok(())
}

/// Keeps each aligned view in line with its parent, parents first.
fn align(sheet: &mut DrawingSheet) {
    for i in 0..sheet.views.len() {
        let id = sheet.views[i].id;
        let Some(k) = sheet.aligned_axis(id) else {
            continue;
        };
        if let Some(p) = aligned_place(sheet, id, k) {
            sheet.views[i].place[k] = p;
        }
    }
}

/// Where a view of this size (sheet mm) goes near `target` without
/// overlapping the boxes of the other views: moved on in steps along
/// `away` while it does, then (`both_ways`) the other way, as long as it
/// stays inside the sheet's area for views (`area`); at `target`,
/// overlapping, when no way finds room. A view beside its parent moves
/// away from it only: the other way would take it past its parent, to
/// the wrong side.
fn clear_of(
    boxes: &[Bounds],
    area: (P2, P2),
    target: P2,
    size: P2,
    away: P2,
    both_ways: bool,
) -> P2 {
    const MARGIN: f64 = 5.0;
    const STEP: f64 = 5.0;
    let hits = |t: P2| {
        boxes.iter().any(|b| {
            (0..2).all(|k| {
                t[k] - size[k] / 2.0 < b.max[k] + MARGIN && t[k] + size[k] / 2.0 > b.min[k] - MARGIN
            })
        })
    };
    let (lo, hi) = area;
    // Inside the area along the way it moves (a view larger than the area
    // stays where it is put).
    let inside = |t: P2| {
        (0..2).all(|k| {
            away[k] == 0.0 || (t[k] - size[k] / 2.0 >= lo[k] && t[k] + size[k] / 2.0 <= hi[k])
        })
    };
    if away == [0.0, 0.0] || !hits(target) {
        return target;
    }
    let ways: &[f64] = if both_ways { &[1.0, -1.0] } else { &[1.0] };
    for &way in ways {
        let mut t = target;
        loop {
            t = [t[0] + way * away[0] * STEP, t[1] + way * away[1] * STEP];
            if !inside(t) {
                break;
            }
            if !hits(t) {
                return t;
            }
        }
    }
    target
}

/// The place along `k` that keeps a view in line with its parent: the
/// parent's anchor and the same point of the view on one line (a view of
/// a detail lines up with the detail's centre).
fn aligned_place(sheet: &DrawingSheet, id: u32, k: usize) -> Option<f64> {
    let view = sheet.view(id)?;
    let parent = sheet.view(view.kind.parent()?)?;
    let s = sheet.scale(id);
    let a_p = sheet.anchor(parent.id);
    let a_c = sheet.anchor(id);
    Some(parent.place[k] - s * (a_p[k] - a_c[k]))
}

/// A sheet's fields set from a command.
#[allow(clippy::too_many_arguments)]
fn set_fields(
    sheet: &mut DrawingSheet,
    name: &Option<String>,
    size: Option<SheetSize>,
    orientation: Option<Orientation>,
    projection: Option<Projection>,
    title: &Option<TitleFields>,
    units: &Option<String>,
    annotations: Option<Annotations>,
    template: Option<Option<SheetTemplate>>,
) -> Result<(), ApiError> {
    if let Some(template) = template {
        sheet.template = template;
    }
    if let Some(name) = name {
        sheet.name = name.trim().to_owned();
    }
    if let Some(size) = size {
        sheet.size = size;
    }
    if let Some(orientation) = orientation {
        sheet.orientation = orientation;
    }
    if let Some(projection) = projection {
        sheet.projection = projection;
    }
    if let Some(t) = title {
        let set = |field: &mut Option<String>, value: &Option<String>| {
            if let Some(value) = value {
                let value = value.trim();
                *field = (!value.is_empty()).then(|| value.to_owned());
            }
        };
        set(&mut sheet.title_block.title, &t.title);
        set(&mut sheet.title_block.number, &t.number);
        set(&mut sheet.title_block.author, &t.author);
        set(&mut sheet.title_block.date, &t.date);
    }
    if let Some(units) = units {
        let units = units.trim();
        if !units.is_empty() && Units::by_symbol(units).is_none() {
            return Err(ApiError(format!("unknown length unit '{units}'")));
        }
        sheet.units = (!units.is_empty()).then(|| units.to_owned());
    }
    if let Some(annotations) = annotations {
        sheet.annotations = annotations;
    }
    Ok(())
}

/// A command's `template`, when given: the template, or none (`""`, null).
fn resolve_template(
    value: &Option<Option<Value>>,
) -> Result<Option<Option<SheetTemplate>>, ApiError> {
    match value {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(v)) => super::drawing_templates::template_choice(v).map(Some),
    }
}

/// The polygons of a shading inside a detail's circle.
fn shading_inside_circle(shading: &[(Vec<P2>, f64)], center: P2, radius: f64) -> Shading {
    clip::polygons_inside_circle(shading, center, radius)
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait DrawingsOps<K: Kernel> {
    fn run_drawing_command(&mut self, command: DrawingCommand) -> Result<Value, ApiError>;

    /// The bodies a subject shows in a drawing's component: the shown
    /// bodies of the component or of an occurrence below it, or chosen
    /// bodies where they are shown (or wherever they are, when nowhere).
    fn subject_bodies(
        &self,
        component: ComponentUid,
        subject: &Subject,
    ) -> Result<Vec<Shown>, String>;

    /// The shapes of the bodies where a view shows them.
    fn placed_shapes(&self, shown: &[Shown]) -> Result<Vec<K::Shape>, String>;

    /// The kernel's projection of the bodies, kept by what it showed.
    fn projected(&self, shown: &[Shown], spec: &ViewSpec) -> Result<Arc<ViewProjection>, String>;

    /// The kernel's projection of the bodies, not kept: for answers kept
    /// on their own (`body_in_view`, mitcad#722).
    fn project_now(&self, shown: &[Shown], spec: &ViewSpec) -> Result<ViewProjection, String>;

    /// A sheet's views worked out, parents first.
    fn evaluate_views(&self, drawing: &Drawing, si: usize) -> Evaluated;

    /// The title of a drawing's sheets without one of their own: the
    /// component's name, or the design's (`title`, else the root's name).
    fn drawing_title(&self, drawing: &Drawing, title: Option<&str>) -> String;

    /// Draws a sheet of a drawing: its views, marks and annotations on the
    /// sheet with its title block.
    fn draw_sheet(
        &self,
        drawing: &Drawing,
        si: usize,
        title: Option<&str>,
    ) -> Result<SheetDrawn, ApiError>;

    /// `drawings`: every drawing with its component and sheets.
    fn drawings_json(&self) -> Value;

    /// `drawing`: a drawing's sheets and views, one sheet drawn.
    fn drawing_json(&self, query: &DrawingQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> DrawingsOps<K> for Document<K> {
    fn run_drawing_command(&mut self, command: DrawingCommand) -> Result<Value, ApiError> {
        match command {
            DrawingCommand::AddDrawing(c) => self.add_drawing_command(*c),
            DrawingCommand::EditDrawing(c) => {
                let old = self.find_drawing(&c.drawing)?.clone();
                let mut changed = old.clone();
                if let Some(name) = &c.name {
                    changed.name.clone_from(name);
                }
                // The drawing's defaults (mitcad#729): `null` none.
                if let Some(decimals) = c.decimals {
                    changed.defaults.decimals = decimals;
                }
                if let Some(height) = c.text_height {
                    changed.defaults.text_height = height;
                }
                let label = if c.decimals.is_none() && c.text_height.is_none() {
                    format!("Rename {}", old.name)
                } else {
                    format!("Edit {}", old.name)
                };
                self.set_drawing(old.uid, label, changed)?;
                Ok(json!({"drawing": old.uid}))
            }
            DrawingCommand::DeleteDrawing { drawing } => {
                let uid = self.find_drawing(&drawing)?.uid;
                self.delete_drawing(uid)?;
                Ok(json!({"drawing": uid}))
            }
            DrawingCommand::AddSheet(c) => {
                let old = self.find_drawing(&c.drawing)?.clone();
                let mut changed = old.clone();
                // The last sheet's setup, without its views and its text
                // notes (their ids are the drawing's, mitcad#742).
                let mut sheet = old
                    .sheets
                    .last()
                    .cloned()
                    .unwrap_or_else(|| DrawingSheet::new(""));
                sheet.views.clear();
                sheet.notes.clear();
                sheet.name = (1..)
                    .map(|n| format!("Sheet{n}"))
                    .find(|n| !old.sheets.iter().any(|s| s.name == *n))
                    .expect("a free name");
                set_fields(
                    &mut sheet,
                    &c.name,
                    c.size,
                    c.orientation,
                    c.projection,
                    &c.title_block,
                    &c.units,
                    c.annotations,
                    resolve_template(&c.template)?,
                )?;
                let at = c
                    .sheet
                    .unwrap_or(changed.sheets.len())
                    .min(changed.sheets.len());
                let label = format!("Add {} to {}", sheet.name, old.name);
                changed.sheets.insert(at, sheet);
                self.set_drawing(old.uid, label, changed)?;
                Ok(json!({"drawing": old.uid, "sheet": at}))
            }
            DrawingCommand::EditSheet(c) => {
                let old = self.find_drawing(&c.drawing)?.clone();
                let index = c.sheet.unwrap_or(0);
                let mut changed = old.clone();
                let sheet = changed
                    .sheets
                    .get_mut(index)
                    .ok_or_else(|| ApiError(format!("{} has no sheet {index}", old.name)))?;
                let label = format!("Edit {} of {}", sheet.name, old.name);
                set_fields(
                    sheet,
                    &c.name,
                    c.size,
                    c.orientation,
                    c.projection,
                    &c.title_block,
                    &c.units,
                    c.annotations,
                    resolve_template(&c.template)?,
                )?;
                // A changed projection method turns the projected views:
                // the sections and details drawn on them follow
                // (mitcad#729), and they stay in line with their parents.
                follow_turns(&old.sheets[index], sheet)?;
                align(sheet);
                self.set_drawing(old.uid, label, changed)?;
                Ok(json!({"drawing": old.uid, "sheet": index}))
            }
            DrawingCommand::DeleteSheet { drawing, sheet } => {
                let old = self.find_drawing(&drawing)?.clone();
                if sheet >= old.sheets.len() {
                    return Err(ApiError(format!("{} has no sheet {sheet}", old.name)));
                }
                if old.sheets.len() == 1 {
                    return Err(ApiError(
                        "a drawing keeps one sheet at least: delete the drawing instead".to_owned(),
                    ));
                }
                let mut changed = old.clone();
                let gone = changed.sheets.remove(sheet);
                self.set_drawing(
                    old.uid,
                    format!("Delete {} of {}", gone.name, old.name),
                    changed,
                )?;
                Ok(json!({"drawing": old.uid}))
            }
            DrawingCommand::AddView(c) => {
                if self.find_drawing(&c.drawing)?.cut_sheet.is_some() {
                    return Err(ApiError(format!(
                        "{} is a cut sheet: it takes parts (add_cut_sheet_parts), not views",
                        c.drawing
                    )));
                }
                self.add_view_command(*c)
            }
            DrawingCommand::EditView(c) => {
                // A cut sheet's parts (mitcad#853) move and turn as views do.
                if self.find_drawing(&c.drawing)?.cut_sheet.is_none() {
                    return self.edit_view_command(*c);
                }
                let other = c.scale.is_some()
                    || c.style.is_some()
                    || c.label.is_some()
                    || c.direction.is_some()
                    || c.subject.is_some()
                    || c.side.is_some()
                    || c.letter.is_some()
                    || c.hatch.is_some()
                    || c.flip
                    || c.center.is_some()
                    || c.radius.is_some()
                    || c.breaks.is_some()
                    || c.add_break.is_some()
                    || c.aligned.is_some()
                    || c.center_marks.is_some()
                    || c.retrieved_dimensions.is_some();
                if other {
                    return Err(ApiError(
                        "a cut sheet's part is moved and turned only (place, move, rotation)"
                            .to_owned(),
                    ));
                }
                self.edit_cut_part(
                    &c.drawing,
                    c.view,
                    PartEdit {
                        place: c.place,
                        shift: c.shift,
                        rotation: c.rotation,
                        ..PartEdit::default()
                    },
                )
            }
            DrawingCommand::DeleteView { drawing, view } => {
                if self.find_drawing(&drawing)?.cut_sheet.is_some() {
                    return self.delete_cut_parts(&drawing, &[view]);
                }
                let old = self.find_drawing(&drawing)?.clone();
                let (index, _) = old
                    .find_view(view)
                    .ok_or_else(|| ApiError(format!("{} has no view {view}", old.name)))?;
                let mut changed = old.clone();
                let sheet = &mut changed.sheets[index];
                let gone = deleted_views(sheet, view);
                sheet.views.retain(|v| !gone.contains(&v.id));
                self.set_drawing(
                    old.uid,
                    format!("Delete view {view} of {}", old.name),
                    changed,
                )?;
                Ok(json!({"drawing": old.uid, "deleted": gone}))
            }
            DrawingCommand::AddAnnotation(c) => self.add_drawing_annotation(*c),
            DrawingCommand::EditAnnotation(c) => self.edit_drawing_annotation(*c),
            DrawingCommand::DeleteAnnotation {
                drawing,
                annotation,
                annotations,
            } => self.delete_drawing_annotations(&drawing, annotation, annotations),
            DrawingCommand::AddRevision(c) => self.add_drawing_revision(*c),
            DrawingCommand::EditRevision(c) => self.edit_drawing_revision(*c),
            DrawingCommand::DeleteRevision { drawing, revision } => {
                self.delete_drawing_revision(&drawing, &revision)
            }
            DrawingCommand::SaveTemplate(c) => self.save_drawing_template(*c),
        }
    }

    fn subject_bodies(
        &self,
        component: ComponentUid,
        subject: &Subject,
    ) -> Result<Vec<Shown>, String> {
        let all = self.placements_in(component)?;
        let assembly = self.assembly();
        let shown = |p: &Placement| Shown {
            body: p.body,
            name: p.name.clone(),
            transform: p.transform,
            path: p.path.clone(),
        };
        let mut out: Vec<Shown> = Vec::new();
        match subject {
            Subject::Design => {
                out.extend(all.iter().filter(|(_, s)| *s).map(|(p, _)| shown(p)));
            }
            Subject::Occurrence { occurrence } => {
                let path = &occurrence.0;
                if path.is_empty() || !path_from(assembly, component, path) {
                    return Err(format!("the occurrence {occurrence} is gone"));
                }
                let below = |p: &Placement| {
                    p.path.starts_with(path)
                        && self.body_attributes(p.body).visible
                        && p.path[path.len()..]
                            .iter()
                            .all(|o| assembly.occurrence(*o).is_some_and(|o| o.visible))
                };
                out.extend(all.iter().filter(|(p, _)| below(p)).map(|(p, _)| shown(p)));
            }
            Subject::Bodies { bodies } => {
                for b in bodies {
                    let mine: Vec<&(Placement, bool)> = all
                        .iter()
                        .filter(|(p, _)| {
                            p.body == b.body && b.occurrence.as_ref().is_none_or(|o| p.path == o.0)
                        })
                        .collect();
                    if mine.is_empty() {
                        // A body no occurrence places is drawn where it is.
                        if b.occurrence.is_none() && self.body_shape(b.body).is_some() {
                            out.push(Shown {
                                body: b.body,
                                name: self.body_name(b.body),
                                transform: Transform::IDENTITY,
                                path: Vec::new(),
                            });
                            continue;
                        }
                        return Err(format!("the body {} is gone", b.body));
                    }
                    let visible: Vec<_> = mine.iter().filter(|(_, s)| *s).collect();
                    if visible.is_empty() {
                        out.extend(mine.iter().map(|(p, _)| shown(p)));
                    } else {
                        out.extend(visible.iter().map(|(p, _)| shown(p)));
                    }
                }
            }
        }
        // Empty bodies show nothing.
        out.retain(|s| {
            !self
                .body_shape(s.body)
                .is_some_and(|shape| matches!(self.kernel().body_kind(shape), Ok(BodyKind::Empty)))
        });
        Ok(out)
    }

    fn placed_shapes(&self, shown: &[Shown]) -> Result<Vec<K::Shape>, String> {
        shown
            .iter()
            .map(|s| {
                let shape = self
                    .body_shape(s.body)
                    .ok_or_else(|| format!("the body {} is gone", s.body))?;
                if s.transform.is_identity() {
                    Ok(shape.clone())
                } else {
                    self.kernel()
                        .transform_shape(shape, &s.transform, None)
                        .map_err(|e| format!("{}: {e}", s.name))
                }
            })
            .collect()
    }

    fn projected(&self, shown: &[Shown], spec: &ViewSpec) -> Result<Arc<ViewProjection>, String> {
        if shown.is_empty() {
            return Ok(Arc::new(ViewProjection::default()));
        }
        let key = view_key("drawing-view-v1", &self.keyed(shown), spec);
        if let Some(projection) = self.drawing_cache().projection(key) {
            return Ok(projection);
        }
        let projection = Arc::new(self.project_now(shown, spec)?);
        self.drawing_cache()
            .keep_projection(key, Arc::clone(&projection));
        Ok(projection)
    }

    fn project_now(&self, shown: &[Shown], spec: &ViewSpec) -> Result<ViewProjection, String> {
        let started = std::time::Instant::now();
        let shapes = self.placed_shapes(shown)?;
        let placed = started.elapsed().as_secs_f64() * 1e3;
        let refs: Vec<&K::Shape> = shapes.iter().collect();
        let projection = self
            .kernel()
            .project_view(&refs, spec)
            .map_err(|e| e.to_string());
        let ms = started.elapsed().as_secs_f64() * 1e3;
        PROJECTIONS.with(|p| {
            let (n, total) = p.get();
            p.set((n + 1, total + ms));
        });
        if tracing() {
            let how = match &projection {
                Ok(p) if p.polygonal => "polygonal",
                Ok(_) => "exact",
                Err(_) => "failed",
            };
            let bodies: Vec<String> = shown
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{i} {}", s.name))
                .collect();
            eprintln!(
                "drawing-trace: project {} bodies {how}: place={placed:.1} kernel={:.1} total={ms:.1} ms; bodies {}",
                shown.len(),
                ms - placed,
                bodies.join(", ")
            );
        }
        projection
    }

    fn evaluate_views(&self, drawing: &Drawing, si: usize) -> Evaluated {
        let started = std::time::Instant::now();
        let before = PROJECTIONS.with(std::cell::Cell::get);
        let sheet = &drawing.sheets[si];
        let mut out = Evaluated {
            views: Vec::new(),
            index_of: BTreeMap::new(),
            errors: BTreeMap::new(),
            shown: Vec::new(),
            view_shown: BTreeMap::new(),
        };
        let mut ids: Vec<String> = Vec::new();
        for view in &sheet.views {
            let mut work = || -> Result<Worked, String> {
                if let Some(parent) = view.kind.parent()
                    && !out.index_of.contains_key(&parent)
                {
                    return Err(format!("its parent, view {parent}, is not drawn"));
                }
                // A detail is projected in its own style, as the view it
                // enlarges looks, and cut to its circle.
                let spec = sheet.spec(view.id)?;
                let style = sheet.style(view.id);
                let shown = self.subject_bodies(drawing.component, &sheet.subject(view.id))?;
                let mut own: Vec<usize> = Vec::new();
                for s in &shown {
                    let i = match out.shown.iter().position(|o| o == s) {
                        Some(i) => i,
                        None => {
                            out.shown.push(s.clone());
                            out.shown.len() - 1
                        }
                    };
                    own.push(i);
                }
                let mut projection = self.projected(&shown, &spec)?;
                let mut shading = if style.shaded {
                    Some(self.shaded(&shown, &spec)?)
                } else {
                    None
                };
                if let ViewKind::Detail { center, radius, .. } = &view.kind {
                    projection = Arc::new(clip::inside_circle(&projection, *center, *radius));
                    shading =
                        shading.map(|s| Arc::new(shading_inside_circle(&s, *center, *radius)));
                }
                let map = view_map(sheet, view);
                let shown_projection = clip::without_bands(&projection, &map.breaks);
                let shading_shown = shading
                    .as_ref()
                    .map(|s| clip::polygons_without_bands(s, &map.breaks))
                    .unwrap_or_default();
                let bounds = view_bounds(&shown_projection);
                // A view that shows nothing gets no annotations.
                let role = match &view.kind {
                    ViewKind::Section { .. } => ViewRole::Section,
                    ViewKind::Detail { .. } => ViewRole::Detail,
                    // Square to an axis, or to a face it shows (mitcad#718).
                    _ if !bounds.is_empty()
                        && !self.seen_from_an_angle(sheet, view.id, &shown, &spec) =>
                    {
                        ViewRole::Orthographic
                    }
                    _ => ViewRole::Pictorial,
                };
                // Its group's id: by its direction (however it is turned on
                // the sheet, mitcad#729) or kind, else its id.
                let (own_eye, own_x) = sheet.own_axes(view.id)?;
                let mut id = match &view.kind {
                    ViewKind::Section { .. } => "view-section".to_owned(),
                    ViewKind::Detail { .. } => "view-detail".to_owned(),
                    _ => StandardView::of_axes(own_eye, own_x).map_or_else(
                        || format!("view-V{}", view.id),
                        |v| format!("view-{}", v.key()),
                    ),
                };
                if ids.contains(&id) {
                    id = format!("view-V{}", view.id);
                }
                ids.push(id.clone());
                // A section's hatching, also in its details.
                let hatch = sheet
                    .views
                    .iter()
                    .filter(|v| v.id == view.id || sheet.dependents(v.id).contains(&view.id))
                    .rev()
                    .find_map(|v| match &v.kind {
                        ViewKind::Section { hatch, .. } => Some(*hatch),
                        _ => None,
                    })
                    .unwrap_or(true);
                Ok((
                    PlacedView {
                        id,
                        role,
                        spec,
                        projection: shown_projection,
                        map,
                        bounds,
                        label: sheet.label(view.id),
                        hatch,
                        shading: shading_shown,
                        holes: None,
                    },
                    own,
                ))
            };
            match work() {
                Ok((placed, own)) => {
                    out.index_of.insert(view.id, out.views.len());
                    out.views.push(placed);
                    out.view_shown.insert(view.id, own);
                }
                Err(e) => {
                    out.errors.insert(view.id, e);
                }
            }
        }
        if tracing() {
            let (n, ms) = PROJECTIONS.with(std::cell::Cell::get);
            let projected = n - before.0;
            eprintln!(
                "drawing-trace: views of sheet {si} of {}: {} views, {projected} projected ({:.1} ms), {} reused, {} not drawn, {:.1} ms",
                drawing.name,
                sheet.views.len(),
                ms - before.1,
                (out.views.len() as u64).saturating_sub(projected),
                out.errors.len(),
                started.elapsed().as_secs_f64() * 1e3
            );
        }
        out
    }

    fn drawing_title(&self, drawing: &Drawing, title: Option<&str>) -> String {
        if !drawing.component.is_root() {
            return self.assembly().name(drawing.component);
        }
        match title.filter(|t| !t.trim().is_empty()) {
            Some(t) => t.to_owned(),
            None => {
                let root = &self.assembly().root_name;
                if root == mitcad_model::assembly::ROOT_NAME {
                    "Untitled".to_owned()
                } else {
                    root.clone()
                }
            }
        }
    }

    fn draw_sheet(
        &self,
        drawing: &Drawing,
        si: usize,
        title: Option<&str>,
    ) -> Result<SheetDrawn, ApiError> {
        let mut phases = Phases::new(|| format!("draw sheet {si} of {}", drawing.name));
        let sheet = &drawing.sheets[si];
        let units = match &sheet.units {
            Some(symbol) => Units::by_symbol(symbol)
                .ok_or_else(|| ApiError(format!("unknown length unit '{symbol}'")))?,
            None => Units::of(self.parameters().context().default_length_unit),
        };
        let mut evaluated = self.evaluate_views(drawing, si);
        phases.mark("views");
        let mut warnings: Vec<String> = evaluated
            .errors
            .iter()
            .map(|(id, e)| format!("view {id}: {e}"))
            .collect();
        // The sections' lines and the details' circles.
        let mut sections = Vec::new();
        let mut details = Vec::new();
        for view in &sheet.views {
            match &view.kind {
                ViewKind::Section {
                    parent,
                    origin,
                    normal,
                    span,
                    letter,
                    ..
                } => {
                    let plane = CutPlane {
                        origin: *origin,
                        normal: *normal,
                    };
                    let on = match parent {
                        Some(p) => evaluated.index_of.get(p).copied(),
                        None => compose::edge_on_view(&evaluated.views, &plane),
                    };
                    if let Some(on) = on {
                        sections.push(SectionMark {
                            on,
                            plane,
                            span: if parent.is_some() { *span } else { None },
                            letter: letter.clone(),
                        });
                    }
                }
                ViewKind::Detail {
                    parent,
                    center,
                    radius,
                    letter,
                } => {
                    if let Some(on) = evaluated.index_of.get(parent) {
                        details.push(DetailMark {
                            on: *on,
                            center: *center,
                            radius: *radius,
                            letter: letter.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        // The holes of the bodies shown, for the automatic annotations;
        // each view tags those of the bodies it shows.
        let mut holes: Vec<HoleInput> = Vec::new();
        if sheet.annotations.dimensions || sheet.annotations.hole_table {
            let threads = super::drawing::threads_of(&self.threads_json());
            // The holes of each body shown (indices into `holes`).
            let mut of_body: Vec<Vec<usize>> = Vec::new();
            for s in &evaluated.shown {
                let start = holes.len();
                holes.extend(self.shown_holes(s, &threads, &mut warnings));
                of_body.push((start..holes.len()).collect());
            }
            for (id, own) in &evaluated.view_shown {
                if let Some(&i) = evaluated.index_of.get(id) {
                    evaluated.views[i].holes = Some(
                        own.iter()
                            .flat_map(|b| of_body[*b].iter().copied())
                            .collect(),
                    );
                }
            }
        }
        phases.mark("holes");
        let retrieving = sheet.views.iter().any(|v| v.retrieved_dimensions);
        let all_sketch_dimensions =
            if (sheet.annotations.sketch_dimensions || retrieving) && drawing.component.is_root() {
                self.sketch_dimensions()
            } else {
                Vec::new()
            };
        // The views' annotations, centre marks and retrieved dimensions,
        // and the sheet's text notes; a broken one is a warning.
        let annotated = self.annotate_sheet(drawing, si, &evaluated, units, &all_sketch_dimensions);
        warnings.extend(annotated.warnings.iter().cloned());
        phases.mark("annotations");
        // The sheet's own sketch dimensions (File › Export Drawing's), but
        // those a view retrieved already (mitcad#717: once each); each
        // where the placements of the bodies its sketch shapes put it
        // (mitcad#744), as it is when the sheet shows none of them.
        let sketch_dimensions: Vec<compose::SketchDimension> =
            if sheet.annotations.sketch_dimensions {
                let placements = self.sketch_placements(drawing.component, &evaluated.shown);
                let mut out = Vec::new();
                for (i, (sketch, d)) in all_sketch_dimensions.iter().enumerate() {
                    if annotated.retrieved.contains(&i) {
                        continue;
                    }
                    let mut placed = placements
                        .iter()
                        .filter(|(f, _)| f == sketch)
                        .map(|(_, t)| super::drawing_annotations::placed_dimension(d, t))
                        .peekable();
                    if placed.peek().is_none() {
                        out.push(d.clone());
                    } else {
                        out.extend(placed);
                    }
                }
                out
            } else {
                Vec::new()
            };
        let mut bodies: Vec<String> = Vec::new();
        for s in &evaluated.shown {
            if !bodies.contains(&s.name) {
                bodies.push(s.name.clone());
            }
        }
        // The title block's scale: the first base view's.
        let scale = sheet
            .views
            .iter()
            .find(|v| matches!(v.kind, ViewKind::Base { .. }))
            .or(sheet.views.first())
            .map_or(1.0, |v| sheet.scale(v.id));
        let mut info = TitleInfo {
            title: sheet
                .title_block
                .title
                .clone()
                .unwrap_or_else(|| self.drawing_title(drawing, title)),
            number: sheet.title_block.number.clone(),
            author: sheet.title_block.author.clone(),
            bodies,
            date: sheet.title_block.date.clone().unwrap_or_else(today),
            page: (drawing.sheets.len() > 1)
                .then(|| format!("{}/{}", si + 1, drawing.sheets.len())),
            template: None,
        };
        // A template's fields filled in from the design (mitcad#707 phase
        // 4, `drawing_templates.rs`).
        info.template = self.filled_template(drawing, si, title, scale, units);
        phases.mark("title");
        let title_fields = super::drawing_templates::title_fields(&info);
        let content = SheetContent {
            views: evaluated.views.clone(),
            sections,
            details,
            holes,
            sketch_dimensions,
            annotations: annotated.groups,
        };
        let setup = SheetSetup {
            size: sheet.size,
            orientation: sheet.orientation,
            projection: sheet.projection,
        };
        let composed = compose::render(
            &setup,
            &sheet.annotations,
            units,
            &info,
            scale,
            &content,
            warnings,
        );
        phases.mark("compose");
        Ok(SheetDrawn {
            composed,
            evaluated,
            annotations: annotated.entries,
            title_fields,
        })
    }

    fn drawings_json(&self) -> Value {
        let list: Vec<Value> = self
            .drawings()
            .iter()
            .map(|d| {
                // A cut sheet (mitcad#853): its material's size and its
                // parts as its sheet's views.
                let sheets: Vec<Value> = match &d.cut_sheet {
                    Some(cut) => vec![json!({
                        "name": d.sheets.first().map_or("Sheet1", |s| s.name.as_str()),
                        "size": format!("{} x {} mm", mitcad_model_drawing::trim_number(cut.width, 3),
                                        mitcad_model_drawing::trim_number(cut.height, 3)),
                        "views": cut.parts.len(),
                    })],
                    None => d.sheets.iter().map(|s| json!({
                        "name": s.name,
                        "size": s.size.name(),
                        "views": s.views.len(),
                    })).collect(),
                };
                json!({
                    "drawing": d.uid,
                    "name": d.name,
                    "kind": if d.cut_sheet.is_some() { "cut_sheet" } else { "drawing" },
                    "component": d.component,
                    "component_name": self.assembly().name(d.component),
                    "exported": d.exported,
                    "sheets": sheets,
                })
            })
            .collect();
        json!(list)
    }

    fn drawing_json(&self, query: &DrawingQuery) -> Result<Value, ApiError> {
        let drawing = self.find_drawing(&query.drawing)?;
        // A cut sheet (mitcad#853): its parts as views.
        if drawing.cut_sheet.is_some() {
            if query.sheet > 0 {
                return Err(ApiError(format!(
                    "{} has no sheet {}",
                    drawing.name, query.sheet
                )));
            }
            return Ok(self.cut_sheet_drawing_json(drawing, query.geometry));
        }
        if query.sheet >= drawing.sheets.len() {
            return Err(ApiError(format!(
                "{} has no sheet {}",
                drawing.name, query.sheet
            )));
        }
        let drawn = self.draw_sheet(drawing, query.sheet, query.title.as_deref())?;
        let sheets: Vec<Value> = drawing
            .sheets
            .iter()
            .enumerate()
            .map(|(si, sheet)| {
                let (width, height) = sheet.dimensions();
                let mut value = json!({
                    "name": sheet.name,
                    "size": sheet.size.name(),
                    "orientation": sheet.orientation,
                    "projection": sheet.projection,
                    "width": width,
                    "height": height,
                    "title_block": sheet.title_block,
                    "template": sheet.template.as_ref().map(|t| json!({
                        "id": t.id,
                        "name": t.name,
                        "library": t.library,
                        "choice": t.choice(),
                    })),
                    "units": sheet.units,
                    "annotations": sheet.annotations,
                    "views": sheet.views.iter().map(|v| view_json(sheet, v)).collect::<Vec<_>>(),
                });
                if si == query.sheet {
                    let evaluated = &drawn.evaluated;
                    if let Some(views) = value["views"].as_array_mut() {
                        for view in views {
                            let id = view["id"].as_u64().unwrap_or(0) as u32;
                            if let Some(e) = evaluated.errors.get(&id) {
                                // A view that cannot be drawn keeps a box
                                // at its place, to be picked and deleted.
                                view["error"] = json!(e);
                                view["broken"] = json!(true);
                                let p = sheet.view(id).map_or([0.0, 0.0], |v| v.place);
                                let h = BROKEN_BOX / 2.0;
                                view["box"] = json!([[p[0] - h, p[1] - h], [p[0] + h, p[1] + h]]);
                            }
                            if let Some(i) = evaluated.index_of.get(&id) {
                                let b = evaluated.views[*i].sheet_box();
                                view["box"] = if b.is_empty() {
                                    Value::Null
                                } else {
                                    json!([b.min, b.max])
                                };
                            }
                        }
                    }
                    value["scale"] = json!(scale_text(drawn.composed.scale));
                    value["warnings"] = json!(drawn.composed.warnings);
                    value["annotations"] = json!(drawn.annotations);
                    value["title_fields"] = json!(drawn.title_fields);
                    if query.geometry {
                        value["groups"] = groups_json(&drawn.composed.sheet);
                    }
                }
                value
            })
            .collect();
        Ok(json!({
            "drawing": drawing.uid,
            "name": drawing.name,
            "component": drawing.component,
            "component_name": self.assembly().name(drawing.component),
            "sheet": query.sheet,
            "sheets": sheets,
            // Its revision table (mitcad#707 phase 4).
            "revisions": drawing.revisions,
            "next_revision": mitcad_model_drawing::template::next_revision_letter(&drawing.revisions),
            // What a feature control frame can refer to: the letters of the
            // model's datum labels and the drawings' (mitcad#717).
            "datum_letters": self.all_datum_letters(),
            // Its dimensions' and annotations' own unless they have one
            // (mitcad#729): null for the engine's decimals, 3.5 mm.
            "decimals": drawing.defaults.decimals,
            "text_height": drawing.defaults.text_height,
        }))
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait DrawingsLocal<K: Kernel> {
    /// A drawing laid out as File › Export Drawing lays one out (`layout`:
    /// `export_drawing`'s fields without `path` and `format`), numbered as
    /// the one laid out last (`exported`, mitcad#725), with the layout's
    /// warnings.
    fn laid_out(&self, layout: &Value) -> Result<(Drawing, Vec<String>), ApiError>;

    fn add_drawing_command(&mut self, c: AddDrawingCommand) -> Result<Value, ApiError>;

    fn add_view_command(&mut self, c: AddViewCommand) -> Result<Value, ApiError>;

    /// Where a new projected or section view's middle goes when no point
    /// is given, and the way away from its parent: beside its parent on its
    /// side, 15 mm of paper apart. A section goes on the side it is seen
    /// from in third angle projection, on the other side in first angle.
    fn beside_parent(
        &self,
        drawing: &Drawing,
        si: usize,
        id: u32,
        size: P2,
    ) -> Result<(P2, P2), ApiError>;

    fn edit_view_command(&mut self, c: EditViewCommand) -> Result<Value, ApiError>;

    /// A view worked out with its sheet (its box in view coordinates and,
    /// through its map, on the sheet).
    fn placed_view(&self, drawing: &Drawing, si: usize, id: u32) -> Result<PlacedView, ApiError>;

    /// The instances of the bodies under a component, in its coordinates,
    /// with whether each is shown there (the body and the occurrences
    /// below the component).
    fn placements_in(&self, component: ComponentUid) -> Result<Vec<(Placement, bool)>, String>;

    fn keyed(&self, shown: &[Shown]) -> Vec<(BodyUid, u128, Transform)>;

    /// A shaded view's faces: the bodies' triangles seen along the view,
    /// the farthest first, each grey by how squarely it faces the eye; a
    /// section's without what its plane takes away.
    fn shaded(&self, shown: &[Shown], spec: &ViewSpec) -> Result<Arc<Shading>, String>;
}

impl<K: Kernel> DrawingsLocal<K> for Document<K> {
    fn laid_out(&self, layout: &Value) -> Result<(Drawing, Vec<String>), ApiError> {
        let (options, bodies) = layout_options(layout)?;
        let auto = self.auto_drawing(&options, &bodies)?;
        let mut drawing = auto.drawing;
        // A kept drawing is dated when it is drawn, unless dated.
        if options.date.is_none() {
            for sheet in &mut drawing.sheets {
                sheet.title_block.date = None;
            }
        }
        drawing.exported = self
            .drawings()
            .iter()
            .map(|d| d.exported)
            .max()
            .unwrap_or(0)
            + 1;
        Ok((drawing, auto.warnings))
    }

    fn add_drawing_command(&mut self, c: AddDrawingCommand) -> Result<Value, ApiError> {
        let component = match &c.component {
            Some(text) => self.component_by_ref(text)?,
            None => ComponentUid::ROOT,
        };
        let mut warnings = Vec::new();
        let mut drawing = match &c.layout {
            Some(layout) => {
                if !component.is_root() {
                    return Err(ApiError(
                        "an export's layout is of the design: add it to the root's drawings"
                            .to_owned(),
                    ));
                }
                let (drawing, laid_out_warnings) = self.laid_out(layout)?;
                warnings = laid_out_warnings;
                drawing
            }
            None => Drawing {
                uid: mitcad_model_drawing::def::DrawingUid(0),
                name: String::new(),
                component,
                sheets: vec![DrawingSheet::new("Sheet1")],
                revisions: Vec::new(),
                exported: 0,
                defaults: DrawingDefaults::default(),
                cut_sheet: None,
            },
        };
        drawing.component = component;
        drawing.defaults.decimals = c.decimals;
        drawing.defaults.text_height = c.text_height;
        if let Some(name) = &c.name {
            drawing.name = name.clone();
        }
        // A template's own sheet size and projection method unless they
        // are given (mitcad#725: ISO first angle, ANSI third angle).
        let template = resolve_template(&c.template)?;
        let size = c.size.or_else(|| {
            template
                .as_ref()
                .and_then(|t| t.as_ref().and_then(|t| t.size))
        });
        // A laid out drawing keeps its layout's projection: its views are
        // placed by it.
        let projection = c.projection.or_else(|| {
            template
                .as_ref()
                .filter(|_| c.layout.is_none())
                .and_then(|t| t.as_ref().and_then(|t| t.projection))
        });
        for sheet in &mut drawing.sheets {
            set_fields(
                sheet,
                &None,
                size,
                c.orientation,
                projection,
                &c.title_block,
                &c.units,
                c.annotations,
                template.clone(),
            )?;
        }
        let uid = self.add_drawing(drawing)?;
        let added = self.find_drawing(&uid.to_string())?;
        Ok(json!({
            "drawing": uid,
            "name": added.name,
            "component": component,
            "sheets": added.sheets.len(),
            "warnings": warnings,
        }))
    }

    fn add_view_command(&mut self, c: AddViewCommand) -> Result<Value, ApiError> {
        let mut phases = Phases::new(|| format!("add view to {}", c.drawing));
        let old = self.find_drawing(&c.drawing)?.clone();
        let si = c.sheet;
        let sheet = old
            .sheets
            .get(si)
            .ok_or_else(|| ApiError(format!("{} has no sheet {si}", old.name)))?;
        let id = old.next_view_id();
        if c.subject.is_some() && c.kind != ViewType::Base {
            return Err(ApiError("only a base view has a subject".to_owned()));
        }
        let parent_of = |parent: Option<u32>| parent_view(sheet, parent);
        let asked = c
            .scale
            .as_ref()
            .map(scale_value)
            .transpose()?
            .unwrap_or(ScaleInput::Parent);
        if asked == ScaleInput::Fit && c.kind != ViewType::Base {
            return Err(ApiError(
                "only a base view's scale fits the sheet: give a scale or \"parent\"".to_owned(),
            ));
        }
        let scale = match asked {
            ScaleInput::Given(s) => Some(s),
            _ => None,
        };
        // Where it goes: its anchor's place, or where its middle goes; with
        // no place given, away from the other views in this direction.
        let mut place: Option<P2> = None;
        let mut middle: Option<P2> = c.at;
        let mut away: Option<P2> = None;
        let (kind, subject) = match c.kind {
            ViewType::Base => {
                let direction = match &c.direction {
                    Some(d) => direction_value(d)?,
                    None => ViewDirection::Named(StandardView::Front),
                };
                if middle.is_none() {
                    let (lo, hi) = view_area(sheet);
                    middle = Some([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]);
                    away = Some([1.0, 0.0]);
                }
                (ViewKind::Base { direction }, c.subject.clone())
            }
            ViewType::Projected => {
                let parent = parent_of(c.parent)?;
                let side = match (c.side, c.at) {
                    (Some(side), _) => side,
                    (None, Some(at)) => {
                        let b = self.placed_view(&old, si, parent.id)?.sheet_box();
                        Side::towards(b.center(), at)
                    }
                    (None, None) => match sheet.projection {
                        Projection::ThirdAngle => Side::Right,
                        Projection::FirstAngle => Side::Left,
                    },
                };
                (
                    ViewKind::Projected {
                        parent: parent.id,
                        side,
                    },
                    None,
                )
            }
            ViewType::Section => {
                let parent = parent_of(c.parent)?;
                let [a, b] = c.line.ok_or_else(|| {
                    ApiError("a section view needs its line on the parent".to_owned())
                })?;
                let map = view_map(sheet, parent);
                let spec = sheet.spec(parent.id).map_err(ApiError)?;
                let (origin, normal, span) =
                    section_plane(&spec, map.view_point(a), map.view_point(b), c.flip)
                        .ok_or_else(|| ApiError("a section line has a length".to_owned()))?;
                (
                    ViewKind::Section {
                        parent: Some(parent.id),
                        origin,
                        normal,
                        span: Some(span),
                        letter: c.letter.clone().unwrap_or_else(|| old.next_letter()),
                        hatch: c.hatch.unwrap_or(true),
                    },
                    None,
                )
            }
            ViewType::Detail => {
                let parent = parent_of(c.parent)?;
                let map = view_map(sheet, parent);
                let center = c.center.ok_or_else(|| {
                    ApiError("a detail view needs its circle's centre on the parent".to_owned())
                })?;
                let radius = c.radius.unwrap_or(10.0);
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(ApiError("a detail's circle has a size".to_owned()));
                }
                let center = map.view_point(center);
                place = c.at;
                (
                    ViewKind::Detail {
                        parent: parent.id,
                        center,
                        radius: radius / map.scale,
                        letter: c.letter.clone().unwrap_or_else(|| old.next_letter()),
                    },
                    None,
                )
            }
        };
        let mut changed = old.clone();
        changed.sheets[si].views.push(DrawingView {
            id,
            kind,
            place: place.unwrap_or([0.0, 0.0]),
            scale,
            style: c.style,
            label: c.label.clone().filter(|l| !l.trim().is_empty()),
            subject,
            breaks: Vec::new(),
            aligned: true,
            center_marks: c.center_marks.unwrap_or(true),
            retrieved_dimensions: c.retrieved_dimensions.unwrap_or(false),
            annotations: Vec::new(),
            rotation: 0.0,
            sketch: Vec::new(),
        });
        changed.check().map_err(ApiError)?;
        phases.mark("define");
        // Clear of the views already on the sheet, inside its area (their
        // boxes worked out only when a place is to be found).
        let clear = |target: P2, size: P2, away: P2, both_ways: bool| {
            let boxes: Vec<Bounds> = self
                .evaluate_views(&old, si)
                .views
                .iter()
                .map(PlacedView::sheet_box)
                .filter(|b| !b.is_empty())
                .collect();
            clear_of(
                &boxes,
                view_area(&old.sheets[si]),
                target,
                size,
                away,
                both_ways,
            )
        };
        if c.kind == ViewType::Detail {
            if place.is_none() {
                // Beside its parent, on the right.
                let parent = c.parent.expect("checked");
                let pb = self.placed_view(&changed, si, parent)?.sheet_box();
                let sheet = &changed.sheets[si];
                let reach = match &sheet.view(id).expect("added").kind {
                    ViewKind::Detail { radius, .. } => radius * sheet.scale(id),
                    _ => 0.0,
                };
                let target = [pb.max[0] + 15.0 + reach, pb.center()[1]];
                let view = changed.sheets[si].views.last_mut().expect("added");
                view.place = clear(target, [2.0 * reach, 2.0 * reach], [1.0, 0.0], false);
            }
        } else {
            // Its box is known once it is projected.
            let b = self.placed_view(&changed, si, id)?.bounds;
            phases.mark("project");
            if b.is_empty() {
                return Err(ApiError(format!("view {id} would show nothing")));
            }
            let sheet = &mut changed.sheets[si];
            if scale.is_none() && c.kind == ViewType::Base {
                let fit = fitting_scale(sheet, &b);
                sheet.views.last_mut().expect("added").scale = Some(fit);
            }
            let s = sheet.scale(id);
            let size = [b.width() * s, b.height() * s];
            let target = match middle {
                Some(m) => match away {
                    Some(d) => clear(m, size, d, true),
                    None => m,
                },
                None => {
                    let (target, d) = self.beside_parent(&changed, si, id, size)?;
                    clear(target, size, d, false)
                }
            };
            // The box's middle at the target: place + s * (centre - anchor).
            let sheet = &mut changed.sheets[si];
            let anchor = sheet.anchor(id);
            let center = b.center();
            let view = sheet.views.last_mut().expect("added");
            view.place = [
                target[0] - s * (center[0] - anchor[0]),
                target[1] - s * (center[1] - anchor[1]),
            ];
        }
        phases.mark("place");
        align(&mut changed.sheets[si]);
        let label = format!("Add view {id} to {}", old.name);
        self.set_drawing(old.uid, label, changed)?;
        phases.mark("store");
        Ok(json!({"drawing": old.uid, "sheet": si, "view": id}))
    }

    fn beside_parent(
        &self,
        drawing: &Drawing,
        si: usize,
        id: u32,
        size: P2,
    ) -> Result<(P2, P2), ApiError> {
        let sheet = &drawing.sheets[si];
        let view = sheet.view(id).expect("added");
        let parent = view
            .kind
            .parent()
            .ok_or_else(|| ApiError("a base view needs a place".to_owned()))?;
        let pb = self.placed_view(drawing, si, parent)?.sheet_box();
        let (sx, sy) = match &view.kind {
            ViewKind::Projected { side, .. } => side.steps(),
            ViewKind::Section { normal, .. } => {
                // Where the eye is: the normal's side in the parent's plane.
                let spec = sheet.spec(parent).map_err(ApiError)?;
                let x = dot(*normal, spec.x_axis);
                let y = dot(*normal, spec.y_axis());
                let mirror = match sheet.projection {
                    Projection::ThirdAngle => 1.0,
                    Projection::FirstAngle => -1.0,
                };
                if x.abs() >= y.abs() {
                    (mirror * x.signum(), 0.0)
                } else {
                    (0.0, mirror * y.signum())
                }
            }
            _ => (1.0, 0.0),
        };
        let c = pb.center();
        let gap = 15.0;
        Ok((
            [
                c[0] + sx * (pb.width() / 2.0 + gap + size[0] / 2.0),
                c[1] + sy * (pb.height() / 2.0 + gap + size[1] / 2.0),
            ],
            [sx, sy],
        ))
    }

    fn edit_view_command(&mut self, c: EditViewCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        let id = c.view;
        let (si, _) = old
            .find_view(id)
            .ok_or_else(|| ApiError(format!("{} has no view {id}", old.name)))?;
        let mut changed = old.clone();
        let mut moved = false;
        let mut fit = false;
        // Turned on the sheet (mitcad#729): about the middle of its box,
        // which stays where it is (its box's middle on the sheet and in
        // the view's coordinates, before).
        let turn = match c.rotation {
            Some(r) if !r.is_finite() => {
                return Err(ApiError("a rotation is a number of degrees".to_owned()));
            }
            Some(r) => {
                let mut r = r.rem_euclid(360.0);
                if r > 180.0 {
                    r -= 360.0;
                }
                let current = old.sheets[si].view(id).map_or(0.0, |v| v.rotation);
                (r != current).then_some(r)
            }
            None => None,
        };
        let pivot = match turn {
            Some(_) => self.placed_view(&old, si, id).ok().and_then(|placed| {
                (!placed.bounds.is_empty())
                    .then(|| (placed.sheet_box().center(), placed.bounds.center()))
            }),
            None => None,
        };
        {
            let sheet = &mut changed.sheets[si];
            let parent_map = sheet
                .view(id)
                .and_then(|v| v.kind.parent())
                .and_then(|p| sheet.view(p))
                .map(|p| view_map(sheet, p));
            let own_map = view_map(sheet, sheet.view(id).expect("found"));
            let i = sheet.view_index(id).expect("found");
            let view = &mut sheet.views[i];
            // Where it was: a turn and a move take the views from it along.
            let before = view.place;
            if let Some(r) = turn {
                if matches!(view.kind, ViewKind::Detail { .. }) {
                    return Err(ApiError(
                        "a detail turns with its parent: turn its parent".to_owned(),
                    ));
                }
                let by = r - view.rotation;
                view.rotation = r;
                // The middle of its box stays where it was: the view
                // point there turns about the view's origin.
                if let Some((on_sheet, in_view)) = pivot {
                    let s = own_map.scale;
                    let p = turned(in_view, Some(by));
                    view.place = [on_sheet[0] - s * p[0], on_sheet[1] - s * p[1]];
                }
            }
            if let Some(scale) = &c.scale {
                match scale_value(scale)? {
                    // A base view has no parent: its automatic scale is
                    // the one that fits the sheet, as when it was added.
                    ScaleInput::Parent if matches!(view.kind, ViewKind::Base { .. }) => fit = true,
                    ScaleInput::Parent => view.scale = None,
                    ScaleInput::Given(s) => view.scale = Some(s),
                    ScaleInput::Fit => {
                        if !matches!(view.kind, ViewKind::Base { .. }) {
                            return Err(ApiError(
                                "only a base view's scale fits the sheet".to_owned(),
                            ));
                        }
                        fit = true;
                    }
                }
            }
            if let Some(style) = c.style {
                view.style = Some(style);
            }
            if let Some(label) = &c.label {
                view.label = (!label.trim().is_empty()).then(|| label.trim().to_owned());
            }
            if let Some(subject) = &c.subject {
                if !matches!(view.kind, ViewKind::Base { .. }) {
                    return Err(ApiError("only a base view has a subject".to_owned()));
                }
                view.subject = Some(subject.clone());
            }
            if let Some(aligned) = c.aligned {
                view.aligned = aligned;
            }
            if let Some(on) = c.center_marks {
                view.center_marks = on;
            }
            if let Some(on) = c.retrieved_dimensions {
                view.retrieved_dimensions = on;
            }
            // A view turned another way takes the views from it and their
            // annotations along (`follow_turns` below, mitcad#729).
            match &mut view.kind {
                ViewKind::Base { direction } => {
                    if let Some(d) = &c.direction {
                        *direction = direction_value(d)?;
                    }
                }
                ViewKind::Projected { side, .. } => {
                    if let Some(s) = c.side {
                        *side = s;
                    }
                }
                ViewKind::Section {
                    normal,
                    span,
                    letter,
                    hatch,
                    ..
                } => {
                    if c.flip {
                        *normal = normal.map(|v| -v);
                        // The line runs the other way along itself.
                        if let Some([a, b]) = *span {
                            *span = Some([-b, -a]);
                        }
                    }
                    if let Some(l) = &c.letter {
                        *letter = l.trim().to_owned();
                    }
                    if let Some(h) = c.hatch {
                        *hatch = h;
                    }
                }
                ViewKind::Detail {
                    center,
                    radius,
                    letter,
                    ..
                } => {
                    if let Some(map) = &parent_map {
                        if let Some(at) = c.center {
                            *center = map.view_point(at);
                        }
                        if let Some(r) = c.radius {
                            if !(r.is_finite() && r > 0.0) {
                                return Err(ApiError("a detail's circle has a size".to_owned()));
                            }
                            *radius = r / map.scale;
                        }
                    }
                    if let Some(l) = &c.letter {
                        *letter = l.trim().to_owned();
                    }
                }
            }
            let wrong = |what: &str| ApiError(format!("view {id} has no {what}"));
            match &view.kind {
                ViewKind::Base { .. } => {
                    if c.side.is_some() || c.letter.is_some() || c.hatch.is_some() || c.flip {
                        return Err(wrong("side, letter or hatching"));
                    }
                }
                ViewKind::Projected { .. } => {
                    if c.direction.is_some() || c.letter.is_some() || c.hatch.is_some() || c.flip {
                        return Err(wrong("direction, letter or hatching"));
                    }
                }
                ViewKind::Section { .. } => {
                    if c.direction.is_some() || c.side.is_some() {
                        return Err(wrong("direction or side"));
                    }
                }
                ViewKind::Detail { .. } => {
                    if c.direction.is_some() || c.side.is_some() || c.hatch.is_some() || c.flip {
                        return Err(wrong("direction, side or hatching"));
                    }
                }
            }
            if (c.center.is_some() || c.radius.is_some())
                && !matches!(view.kind, ViewKind::Detail { .. })
            {
                return Err(wrong("circle"));
            }
            if let Some(breaks) = &c.breaks {
                view.breaks = Vec::new();
                let mut map = own_map.clone();
                map.breaks.clear();
                for b in breaks {
                    view.breaks.push(break_of(&map, b)?);
                }
            }
            if let Some(b) = &c.add_break {
                view.breaks.push(break_of(&own_map, b)?);
            }
            // Moving: by a drag, or to a place.
            if let Some(place) = c.place {
                view.place = place;
            }
            if let Some(d) = c.shift {
                view.place = [view.place[0] + d[0], view.place[1] + d[1]];
            }
            let delta = [view.place[0] - before[0], view.place[1] - before[1]];
            if delta != [0.0, 0.0] {
                moved = true;
                // An aligned view moves along its parent only.
                if let Some(k) = sheet.aligned_axis(id)
                    && let Some(p) = aligned_place(sheet, id, k)
                {
                    sheet.views[i].place[k] = p;
                }
                let delta = [
                    sheet.views[i].place[0] - before[0],
                    sheet.views[i].place[1] - before[1],
                ];
                // The views from it move with it.
                for dependent in sheet.dependents(id) {
                    if let Some(j) = sheet.view_index(dependent) {
                        let p = &mut sheet.views[j].place;
                        *p = [p[0] + delta[0], p[1] + delta[1]];
                    }
                }
            }
            // Turned: the views from it, their annotations and its own
            // follow (mitcad#729).
            follow_turns(&old.sheets[si], sheet)?;
            align(sheet);
        }
        if fit {
            // One of ISO 5455's scales that fits its box on the sheet.
            let b = self.placed_view(&changed, si, id)?.bounds;
            let sheet = &mut changed.sheets[si];
            let s = fitting_scale(sheet, &b);
            let i = sheet.view_index(id).expect("found");
            sheet.views[i].scale = Some(s);
            align(sheet);
        }
        let label = if moved
            && c.scale.is_none()
            && c.style.is_none()
            && c.label.is_none()
            && c.direction.is_none()
            && c.subject.is_none()
            && c.side.is_none()
            && c.breaks.is_none()
            && c.add_break.is_none()
            && turn.is_none()
        {
            format!("Move view {id} of {}", old.name)
        } else {
            format!("Edit view {id} of {}", old.name)
        };
        self.set_drawing(old.uid, label, changed)?;
        Ok(json!({"drawing": old.uid, "view": id}))
    }

    fn placed_view(&self, drawing: &Drawing, si: usize, id: u32) -> Result<PlacedView, ApiError> {
        let mut evaluated = self.evaluate_views(drawing, si);
        if let Some(error) = evaluated.errors.remove(&id) {
            return Err(ApiError(format!("view {id}: {error}")));
        }
        let i = *evaluated
            .index_of
            .get(&id)
            .ok_or_else(|| ApiError(format!("there is no view {id}")))?;
        Ok(evaluated.views.swap_remove(i))
    }

    fn placements_in(&self, component: ComponentUid) -> Result<Vec<(Placement, bool)>, String> {
        let assembly = self.assembly();
        let prefix = assembly
            .paths_to(component)
            .into_iter()
            .next()
            .ok_or_else(|| format!("{} is not placed in the design", assembly.name(component)))?;
        let back = inverse(&self.path_transform(&prefix));
        let shown_path = |path: &[OccurrenceUid]| {
            path.iter()
                .all(|o| assembly.occurrence(*o).is_some_and(|o| o.visible))
        };
        Ok(self
            .instances()
            .into_iter()
            .filter(|i| i.path.starts_with(&prefix))
            .map(|i| {
                let path = i.path[prefix.len()..].to_vec();
                let shown = self.body_attributes(i.body).visible && shown_path(&path);
                (
                    Placement {
                        path,
                        body: i.body,
                        name: i.name,
                        transform: back.after(&i.transform),
                    },
                    shown,
                )
            })
            .collect())
    }

    fn keyed(&self, shown: &[Shown]) -> Vec<(BodyUid, u128, Transform)> {
        shown
            .iter()
            .map(|s| (s.body, self.body_version(s.body).unwrap_or(0), s.transform))
            .collect()
    }

    fn shaded(&self, shown: &[Shown], spec: &ViewSpec) -> Result<Arc<Shading>, String> {
        let key = view_key("drawing-shade-v1", &self.keyed(shown), spec);
        if let Some(shading) = self.drawing_cache().shading(key) {
            return Ok(shading);
        }
        let tolerance = MeshTolerance {
            deviation: 0.1,
            angle: 0.35,
            face_triangles: 0,
            max_edge: 0.0,
        };
        let eye = spec.toward_eye;
        let mut faces: Vec<(f64, Vec<P2>, f64)> = Vec::new();
        for s in shown {
            let shape = self
                .body_shape(s.body)
                .ok_or_else(|| format!("the body {} is gone", s.body))?;
            let mesh = match self.kernel().triangle_mesh(shape, &tolerance) {
                Ok(mesh) => mesh,
                Err(mitcad_model::kernel::KernelError::Unsupported(_)) => continue,
                Err(e) => return Err(format!("{}: {e}", s.name)),
            };
            let points: Vec<[f64; 3]> = mesh
                .vertices
                .iter()
                .map(|v| s.transform.apply_point(*v))
                .collect();
            // A mirroring placement turns the triangles' winding over.
            let m = &s.transform.linear;
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            let outward = if det < 0.0 { -1.0 } else { 1.0 };
            for t in &mesh.triangles {
                let [a, b, c] = t.map(|i| points[i as usize]);
                let n = mitcad_vec3::cross(
                    [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                    [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
                )
                .map(|v| v * outward);
                let length = dot(n, n).sqrt();
                if length < 1e-12 {
                    continue;
                }
                let facing = dot(n, eye) / length;
                if facing <= 0.0 {
                    continue;
                }
                let middle = [
                    (a[0] + b[0] + c[0]) / 3.0,
                    (a[1] + b[1] + c[1]) / 3.0,
                    (a[2] + b[2] + c[2]) / 3.0,
                ];
                if let Some(plane) = &spec.section {
                    let o = plane.origin;
                    let side = dot(
                        [middle[0] - o[0], middle[1] - o[1], middle[2] - o[2]],
                        plane.normal,
                    );
                    if side > 0.0 {
                        continue;
                    }
                }
                faces.push((
                    dot(middle, eye),
                    vec![spec.project(a), spec.project(b), spec.project(c)],
                    0.5 + 0.45 * facing,
                ));
            }
        }
        faces.sort_by(|a, b| a.0.total_cmp(&b.0));
        let shading: Shading = faces.into_iter().map(|(_, p, g)| (p, g)).collect();
        let shading = Arc::new(shading);
        self.drawing_cache().keep_shading(key, Arc::clone(&shading));
        Ok(shading)
    }
}

/// A view's definition and what follows from it, for the `drawing` query.
fn view_json(sheet: &DrawingSheet, view: &DrawingView) -> Value {
    let mut value = json!({
        "id": view.id,
        "type": view.kind.key(),
        "parent": view.kind.parent(),
        "place": view.place,
        "scale": sheet.scale(view.id),
        "scale_text": scale_text(sheet.scale(view.id)),
        "own_scale": view.scale.is_some(),
        // What it would be without its own: its parent's (twice that for
        // a detail), 1:1 for a base view.
        "automatic_scale": scale_text(match &view.kind {
            ViewKind::Base { .. } => 1.0,
            ViewKind::Detail { parent, .. } => 2.0 * sheet.scale(*parent),
            kind => kind.parent().map_or(1.0, |p| sheet.scale(p)),
        }),
        "style": sheet.style(view.id),
        "own_style": view.style.is_some(),
        "label": sheet.label(view.id),
        "aligned": sheet.aligned_axis(view.id).map(|k| if k == 0 { "x" } else { "y" }),
        "breaks": view.breaks,
        "center_marks": view.center_marks,
        "retrieved_dimensions": view.retrieved_dimensions,
        "annotations": view.annotations.iter().map(|a| a.id).collect::<Vec<_>>(),
        // Turned on the sheet, and what is drawn in it (mitcad#729).
        "rotation": view.rotation,
        "sketch": view.sketch.iter().map(|k| k.id()).collect::<Vec<_>>(),
    });
    match &view.kind {
        ViewKind::Base { direction } => {
            value["direction"] = serde_json::to_value(direction).unwrap_or(Value::Null);
            value["subject"] = serde_json::to_value(sheet.subject(view.id)).unwrap_or(Value::Null);
        }
        ViewKind::Projected { side, .. } => value["side"] = json!(side),
        ViewKind::Section {
            letter,
            hatch,
            span,
            ..
        } => {
            value["letter"] = json!(letter);
            value["hatch"] = json!(hatch);
            value["span"] = json!(span);
        }
        ViewKind::Detail {
            letter,
            center,
            radius,
            parent,
        } => {
            value["letter"] = json!(letter);
            // The circle on the parent, on the sheet.
            if let Some(p) = sheet.view(*parent) {
                let map = view_map(sheet, p);
                value["circle"] = json!({
                    "center": map.at(*center),
                    "radius": radius * map.scale,
                });
            }
        }
    }
    value
}

fn pen_name(pen: Pen) -> &'static str {
    match pen {
        Pen::Thick => "thick",
        Pen::Thin => "thin",
        Pen::Hidden => "hidden",
        Pen::Hatch => "hatch",
        Pen::Chain => "chain",
        Pen::Frame => "frame",
    }
}

/// A sheet's groups for the application to draw: each item with its pen,
/// points in sheet millimetres.
pub(super) fn groups_json(sheet: &Sheet) -> Value {
    let groups: Vec<Value> = sheet
        .groups
        .iter()
        .map(|g| {
            let items: Vec<Value> = g
                .items
                .iter()
                .map(|item| match item {
                    Item::Line { pen, a, b } => json!({"line": [a, b], "pen": pen_name(*pen)}),
                    Item::Arc {
                        pen,
                        center,
                        radius,
                        start,
                        end,
                    } => json!({"arc": {"center": center, "radius": radius, "start": start,
                                "end": end}, "pen": pen_name(*pen)}),
                    Item::Polyline {
                        pen,
                        points,
                        closed,
                    } => json!({"polyline": points, "closed": closed, "pen": pen_name(*pen)}),
                    Item::Fill { points } => json!({ "fill": points }),
                    Item::Shade { points, grey } => json!({"shade": points, "grey": grey}),
                    Item::Text {
                        at,
                        height,
                        angle,
                        anchor,
                        text,
                    } => json!({"text": text, "at": at, "height": height, "angle": angle,
                    "anchor": match anchor {
                        Anchor::Start => "start",
                        Anchor::Middle => "middle",
                        Anchor::End => "end",
                    }}),
                })
                .collect();
            json!({"class": g.class, "id": g.id, "items": items})
        })
        .collect();
    json!(groups)
}

/// The largest of ISO 5455's scales at which a view's box takes at most
/// two fifths of the sheet's area for views each way.
fn fitting_scale(sheet: &DrawingSheet, b: &Bounds) -> f64 {
    let (lo, hi) = view_area(sheet);
    let room = [(hi[0] - lo[0]) * 0.4, (hi[1] - lo[1]) * 0.4];
    compose::SCALES
        .iter()
        .copied()
        .find(|s| s * b.width() <= room[0] && s * b.height() <= room[1])
        .unwrap_or(compose::SCALES[compose::SCALES.len() - 1])
}

/// An export's options and bodies from a `layout`: export_drawing's fields
/// without `path` and `format`.
pub(super) fn layout_options(layout: &Value) -> Result<(DrawingOptions, Vec<String>), ApiError> {
    let mut map = match layout {
        Value::Object(map) => map.clone(),
        _ => return Err(ApiError("a layout is export_drawing's options".to_owned())),
    };
    let bodies: Vec<String> = match map.remove("bodies") {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => serde_json::from_value(value)
            .map_err(|e| ApiError(format!("invalid layout bodies: {e}")))?,
    };
    let options = serde_json::from_value(Value::Object(map))
        .map_err(|e| ApiError(format!("invalid layout: {e}")))?;
    Ok((options, bodies))
}

#[cfg(test)]
mod tests {
    use super::{Bounds, P2, clear_of};

    fn bounds(min: P2, max: P2) -> Bounds {
        let mut b = Bounds::empty();
        b.add(min);
        b.add(max);
        b
    }

    #[test]
    fn a_new_view_goes_clear_of_the_others_inside_the_sheet() {
        let area = ([0.0, 0.0], [100.0, 100.0]);
        let size = [10.0, 10.0];
        let right = [1.0, 0.0];
        let middle = [bounds([40.0, 40.0], [60.0, 60.0])];
        // Free: where asked.
        assert_eq!(
            clear_of(&middle, area, [20.0, 20.0], size, right, true),
            [20.0, 20.0]
        );
        // On a view: moved on past it.
        let t = clear_of(&middle, area, [50.0, 50.0], size, right, true);
        assert!(t[0] - 5.0 >= 65.0 - 1e-9 && t[0] + 5.0 <= 100.0, "{t:?}");
        // No room that way inside the sheet: the other way.
        let wide = [bounds([40.0, 40.0], [100.0, 60.0])];
        let t = clear_of(&wide, area, [50.0, 50.0], size, right, true);
        assert!(t[0] + 5.0 <= 35.0 + 1e-9 && t[0] - 5.0 >= 0.0, "{t:?}");
        // No room at all: where asked, overlapping, never off the sheet.
        let across = [bounds([0.0, 40.0], [100.0, 60.0])];
        assert_eq!(
            clear_of(&across, area, [50.0, 50.0], size, right, true),
            [50.0, 50.0]
        );
        // A view beside its parent never goes back past it: with no room
        // the way it moves, it stays where it would go.
        assert_eq!(
            clear_of(&wide, area, [50.0, 50.0], size, right, false),
            [50.0, 50.0]
        );
        let t = clear_of(&middle, area, [50.0, 50.0], size, right, false);
        assert!(t[0] - 5.0 >= 65.0 - 1e-9, "{t:?}");
    }
}
