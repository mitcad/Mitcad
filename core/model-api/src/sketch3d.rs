// SPDX-License-Identifier: MIT
//! The JSON commands and the query of 3D sketches (mitcad#1522;
//! `commands.md`, "3D sketches"): the `sketch3d.*` namespace beside the
//! planar `sketch.*` commands, whose two-coordinate points keep their
//! meaning. Points are `[x, y, z]` in the sketch's frame, or an existing
//! point's id.
//!
//! Each command is one edit of the model
//! (`Document::edit_spatial_sketch`): a segment, arc or spline with its new
//! points, attachments, axis or normal alignment and dimensions is solved
//! and taken whole as one undo step, or refused with nothing changed
//! (definition, parameters, ids, history). The `sketch3d` query reads the
//! definition with its last evaluation: the frame, the solved points and
//! curves, the status and the failure.
//!
//! Linked reference geometry (mitcad#1543): `sketch3d.attach` attaches a
//! point to a source point or puts it on a source curve or, with an On Face
//! relation, on a face of a body (mitcad#1549), `sketch3d.include`
//! makes (or reuses) a source curve's reference curve, `sketch3d.detach`
//! removes either link, `sketch3d.replace_reference` points a record at
//! another source of its kind; a source of another component is given with
//! the `occurrence` that shows it. The query lists the records
//! (`references`).

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::features::spatial_path;
use mitcad_model::features::spatial_sketch::{
    SolvedCurve, SpatialAxis, SpatialConstraintDef, SpatialConstraintKind, SpatialCurve,
    SpatialCurveKind, SpatialDimensionKind, SpatialDirection, SpatialFailure, SpatialFailureKind,
    SpatialFrame, SpatialPlane, SpatialReferenceCurve, SpatialSense, SpatialSketchOutput,
    SpatialSketchStatus, SpatialSolved, SpatialSource, SpatialSplineEnd, SpatialTangentPull,
    spline_end_tangent, sweep_about, to_component,
};
use mitcad_model::features::{
    FeatureDef, OccurrencePath, SketchPlane, SpatialSketchDef, ValueInput,
};
use mitcad_model::ids::{EntityUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::notice;
use mitcad_model::sketch::clipboard::Fragment;
use mitcad_model::sketch::spatial_clipboard::{SpatialFragment, from_planar};
use mitcad_model::sketch::spatial_edit::{
    ArcPlacement, ArcRequest, FilletCorner, FilletMade, PolygonRequest, RectangleCorners,
    RectangleRequest, SegmentRequest, ShapeMade, SpatialEdit, SpatialEditReport, SpatialPointInput,
    SplineRequest, TextRequest,
};
use mitcad_model::sketch::spatial_planar::planar_view;
use mitcad_model::sketch::{ConstraintUid, Ref, SketchText, TextAlign, TextVAlign};
use mitcad_model::{Document, PlanarConversion, SketchConversion, SpatialEditError};

fn yes() -> bool {
    true
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", deny_unknown_fields)]
    pub(super) enum Sketch3dCommand {
        "sketch3d.create" => Create {
            /// What the frame follows: `xy` (the default), `xz`, `yz`, a
            /// construction plane `"F5"` or a planar face `{"face", "body"}`
            /// of the active component.
            #[serde(default)]
            support: SketchPlane,
            #[serde(default)]
            name: Option<String>,
        },
        "sketch3d.add_point" => AddPoint {
            sketch: FeatureUid,
            at: [f64; 3],
            /// Attach it to a source point, or put it on a source curve
            /// (mitcad#1543; `source_input` reads it).
            #[serde(default)]
            source: Option<Value>,
            /// Fix it where it is.
            #[serde(default)]
            fixed: bool,
        },
        "sketch3d.add_line" => AddLine {
            sketch: FeatureUid,
            start: Value,
            end: Value,
            #[serde(default)]
            construction: bool,
            /// Keep the line along a frame axis (`sense` along it).
            #[serde(default)]
            axis: Option<SpatialAxis>,
            #[serde(default)]
            sense: Option<SpatialSense>,
            /// A driving true length.
            #[serde(default)]
            length: Option<ValueInput>,
            /// Driving signed offsets from the start to the end along X, Y
            /// and Z; null leaves one free.
            #[serde(default)]
            offsets: Option<[Option<ValueInput>; 3]>,
            #[serde(default)]
            start_source: Option<Value>,
            #[serde(default)]
            end_source: Option<Value>,
            /// A rotated placement plane (`{"x", "normal"}`, mitcad#1661):
            /// `axis` and `offsets` along its x, y and normal.
            #[serde(default)]
            frame: Option<Value>,
        },
        "sketch3d.add_arc" => AddArc {
            sketch: FeatureUid,
            /// `three_point` (`through`), `center` (`center`, `normal`) or
            /// `tangent` (`from`: the curve the arc continues).
            mode: String,
            start: Value,
            end: Value,
            #[serde(default)]
            through: Option<[f64; 3]>,
            #[serde(default)]
            center: Option<Value>,
            #[serde(default)]
            normal: Option<[f64; 3]>,
            #[serde(default)]
            from: Option<String>,
            #[serde(default)]
            construction: bool,
            /// `Keep Plane`: the normal along a frame axis.
            #[serde(default)]
            normal_axis: Option<SpatialAxis>,
            #[serde(default)]
            normal_sense: Option<SpatialSense>,
            #[serde(default)]
            radius: Option<ValueInput>,
            #[serde(default)]
            arc_length: Option<ValueInput>,
            #[serde(default)]
            start_source: Option<Value>,
            #[serde(default)]
            end_source: Option<Value>,
        },
        "sketch3d.add_circle" => AddCircle {
            sketch: FeatureUid,
            center: Value,
            radius: ValueInput,
            #[serde(default)]
            normal: Option<[f64; 3]>,
            #[serde(default)]
            normal_axis: Option<SpatialAxis>,
            #[serde(default)]
            normal_sense: Option<SpatialSense>,
            /// Plane-bound on a frame plane (`xy`, `yz` or `zx`, mitcad#1550):
            /// the normal bound to the axis square to it; or on a rotated
            /// plane (`{"x", "normal"}`, mitcad#1661): the normal held
            /// along its normal.
            #[serde(default)]
            plane: Option<Value>,
            /// Points held on the circle (a 2-point or 3-point circle's
            /// existing points, mitcad#1550).
            #[serde(default)]
            through: Vec<String>,
            /// A driving radius dimension of `radius` (default true).
            #[serde(default = "yes")]
            dimension: bool,
            /// The driving dimension a diameter instead of a radius.
            #[serde(default)]
            diameter: bool,
            #[serde(default)]
            construction: bool,
        },
        "sketch3d.add_spline" => AddSpline {
            sketch: FeatureUid,
            points: Vec<Value>,
            /// Left out: a cubic through the points; a degree: a B-spline
            /// by these control points.
            #[serde(default)]
            degree: Option<u32>,
            #[serde(default)]
            closed: bool,
            #[serde(default)]
            ends: Option<[SpatialSplineEnd; 2]>,
            #[serde(default)]
            construction: bool,
        },
        "sketch3d.add_constraint" => AddConstraint {
            sketch: FeatureUid,
            constraint: Value,
        },
        "sketch3d.add_dimension" => AddDimension {
            sketch: FeatureUid,
            dimension: SpatialDimensionKind,
            #[serde(default)]
            value: Option<ValueInput>,
            #[serde(default)]
            driven: bool,
        },
        "sketch3d.set_dimension" => SetDimension {
            sketch: FeatureUid,
            dimension: String,
            value: ValueInput,
        },
        "sketch3d.set_driven" => SetDriven {
            sketch: FeatureUid,
            dimension: String,
            #[serde(default = "yes")]
            driven: bool,
        },
        "sketch3d.set_construction" => SetConstruction {
            sketch: FeatureUid,
            curves: Vec<String>,
            #[serde(default = "yes")]
            construction: bool,
        },
        "sketch3d.fix" => Fix {
            sketch: FeatureUid,
            points: Vec<String>,
            #[serde(default = "yes")]
            fixed: bool,
        },
        "sketch3d.detach" => Detach {
            sketch: FeatureUid,
            points: Vec<String>,
        },
        // Linked reference geometry (mitcad#1543).
        "sketch3d.attach" => Attach {
            sketch: FeatureUid,
            point: String,
            source: Value,
            /// Project the source onto the sketch's plane instead of
            /// including it in true 3D.
            #[serde(default)]
            projected: bool,
        },
        "sketch3d.include" => Include {
            sketch: FeatureUid,
            source: Value,
            #[serde(default)]
            projected: bool,
        },
        "sketch3d.replace_reference" => ReplaceReference {
            sketch: FeatureUid,
            /// An attachment (`k3`) or a reference curve (`c7`).
            item: String,
            source: Value,
        },
        "sketch3d.release_from_plane" => ReleaseFromPlane {
            sketch: FeatureUid,
            points: Vec<String>,
        },
        "sketch3d.release_plane" => ReleasePlane {
            sketch: FeatureUid,
            curves: Vec<String>,
        },
        "sketch3d.remove" => Remove {
            sketch: FeatureUid,
            items: Vec<String>,
            /// Delete the curves and relations that use a listed point
            /// with it.
            #[serde(default)]
            dependents: bool,
        },
        "sketch3d.drag" => Drag {
            sketch: FeatureUid,
            #[serde(default)]
            points: Vec<DragGoal>,
            /// End tangents of open fit-point splines pulled toward wanted
            /// tangents (mitcad#1526: a tangent handle's drag).
            #[serde(default)]
            tangents: Vec<TangentGoal>,
            /// Only say where the points would go; nothing changes.
            #[serde(default)]
            preview: bool,
        },
        // A planar sketch turned into a 3D sketch in place (mitcad#1544).
        "sketch3d.convert" => Convert {
            /// The planar sketch.
            sketch: FeatureUid,
        },
        // A 3D sketch whose geometry lies in its sketch plane turned back
        // into a planar sketch in place (mitcad#1550).
        "sketch3d.convert_to_2d" => ConvertTo2d {
            /// The 3D sketch.
            sketch: FeatureUid,
        },
        // Rounding corners between lines (mitcad#1548).
        "sketch3d.fillet" => Fillet {
            sketch: FeatureUid,
            /// Two lines that share an end: their corner.
            #[serde(default)]
            a: Option<String>,
            #[serde(default)]
            b: Option<String>,
            /// Corners: points where two ordinary lines end.
            #[serde(default)]
            points: Vec<String>,
            radius: ValueInput,
        },
        // Plane-bound drawing tools (mitcad#1550): on the frame plane
        // `plane` (`xy`, `yz` or `zx`) through the first point given.
        "sketch3d.add_ellipse" => AddEllipse {
            sketch: FeatureUid,
            center: Value,
            /// The end of the major axis.
            major: Value,
            /// Where the solve starts the minor radius (mm).
            minor_radius: f64,
            plane: Value,
            /// Driving major and minor radius dimensions.
            #[serde(default)]
            major_value: Option<ValueInput>,
            #[serde(default)]
            minor_value: Option<ValueInput>,
            #[serde(default)]
            construction: bool,
        },
        "sketch3d.add_rectangle" => AddRectangle {
            sketch: FeatureUid,
            /// `two_point` (`a`, `b`), `center` (`center`, `corner`) or
            /// `three_point` (`a`, `b`, `c`).
            mode: String,
            #[serde(default)]
            a: Option<Value>,
            #[serde(default)]
            b: Option<Value>,
            #[serde(default)]
            c: Option<[f64; 3]>,
            #[serde(default)]
            center: Option<Value>,
            #[serde(default)]
            corner: Option<Value>,
            plane: Value,
            /// Driving lengths of the first and second side.
            #[serde(default)]
            width: Option<ValueInput>,
            #[serde(default)]
            height: Option<ValueInput>,
            #[serde(default)]
            construction: bool,
        },
        "sketch3d.add_polygon" => AddPolygon {
            sketch: FeatureUid,
            center: Value,
            /// The first corner (inscribed) or the middle of the first side
            /// (circumscribed).
            vertex: Value,
            sides: usize,
            #[serde(default = "yes")]
            inscribed: bool,
            plane: Value,
            /// A driving radius of the construction circle the polygon is
            /// inscribed in or circumscribed about.
            #[serde(default)]
            radius: Option<ValueInput>,
            #[serde(default)]
            construction: bool,
        },
        "sketch3d.add_text" => AddText {
            sketch: FeatureUid,
            text: String,
            /// The anchor: where the text starts (or a frame's corner).
            at: [f64; 3],
            plane: Value,
            height: f64,
            /// The frame's opposite corner: a text in a frame.
            #[serde(default)]
            frame: Option<[f64; 3]>,
            #[serde(default)]
            angle: f64,
            #[serde(default)]
            font: String,
            #[serde(default)]
            bold: bool,
            #[serde(default)]
            italic: bool,
            #[serde(default)]
            align: TextAlign,
            #[serde(default)]
            valign: TextVAlign,
        },
        // Paste of copied sketch geometry (mitcad#1661): a `sketch_fragment`
        // answer's `fragment`, of a 3D sketch or of a planar one.
        "sketch3d.paste" => Paste {
            sketch: FeatureUid,
            fragment: Value,
            /// Where the copy's base point goes (local `[x, y, z]`): where
            /// it was for a 3D sketch's copy, the origin for a planar one.
            #[serde(default)]
            at: Option<[f64; 3]>,
            /// The frame plane a planar copy lies on, through `at`: `xy`
            /// (the default), `yz` or `zx`.
            #[serde(default)]
            plane: Option<String>,
        },
    }
}

/// The axis square to a frame plane named in a command: `xy`, `yz` or
/// `zx`.
fn plane_axis(name: &str) -> Result<SpatialAxis, String> {
    SpatialAxis::of_plane(name).ok_or_else(|| format!("unknown plane \"{name}\" (xy, yz or zx)"))
}

/// A rotated plane's frame in a command (mitcad#1661): `{"x": [x, y, z],
/// "normal": [x, y, z]}`, made unit and square.
fn rotated_frame(value: &Value) -> Result<SpatialFrame, String> {
    let vector = |name: &str| -> Result<[f64; 3], String> {
        serde_json::from_value(value.get(name).cloned().unwrap_or(Value::Null)).map_err(|_| {
            format!("a rotated plane is {{\"x\": [x, y, z], \"normal\": [x, y, z]}}: \"{name}\" is missing or not three numbers")
        })
    };
    if let Some(map) = value.as_object()
        && let Some(extra) = map.keys().find(|k| *k != "x" && *k != "normal")
    {
        return Err(format!(
            "a rotated plane takes \"x\" and \"normal\", not \"{extra}\""
        ));
    }
    SpatialFrame::new(vector("x")?, vector("normal")?)
}

/// The plane of a plane-bound shape in a command: a frame plane's name
/// (`xy`, `yz`, `zx`, mitcad#1550), or a rotated plane's frame
/// (mitcad#1661).
fn shape_plane(value: &Value) -> Result<SpatialPlane, String> {
    match value {
        Value::String(name) => plane_axis(name).map(SpatialPlane::Axis),
        Value::Object(_) => rotated_frame(value).map(SpatialPlane::Frame),
        _ => Err("\"plane\" is xy, yz, zx or a rotated plane {\"x\", \"normal\"}".to_owned()),
    }
}

/// The frame axis a request names: a frame plane's, or `Z` standing in
/// for a rotated plane's (the editor takes the plane itself).
fn plane_normal(plane: SpatialPlane) -> SpatialAxis {
    match plane {
        SpatialPlane::Axis(axis) => axis,
        SpatialPlane::Frame(_) => SpatialAxis::Z,
    }
}

/// A field a rectangle mode needs.
fn field(value: Option<Value>, name: &str, mode: &str) -> Result<SpatialPointInput, String> {
    point_input(&need(value, name, mode)?)
}

/// The corners of `sketch3d.add_rectangle`'s mode from its fields; a field
/// of another mode is refused.
fn rectangle_corners(
    mode: &str,
    a: Option<Value>,
    b: Option<Value>,
    c: Option<[f64; 3]>,
    center: Option<Value>,
    corner: Option<Value>,
) -> Result<RectangleCorners, String> {
    match mode {
        "two_point" => {
            refuse_field(&c, "c", mode)?;
            refuse_field(&center, "center", mode)?;
            refuse_field(&corner, "corner", mode)?;
            Ok(RectangleCorners::TwoPoint {
                a: field(a, "a", mode)?,
                b: field(b, "b", mode)?,
            })
        }
        "center" => {
            refuse_field(&a, "a", mode)?;
            refuse_field(&b, "b", mode)?;
            refuse_field(&c, "c", mode)?;
            Ok(RectangleCorners::Center {
                center: field(center, "center", mode)?,
                corner: field(corner, "corner", mode)?,
            })
        }
        "three_point" => {
            refuse_field(&center, "center", mode)?;
            refuse_field(&corner, "corner", mode)?;
            Ok(RectangleCorners::ThreePoint {
                a: field(a, "a", mode)?,
                b: field(b, "b", mode)?,
                c: need(c, "c", mode)?,
            })
        }
        other => Err(format!(
            "unknown rectangle mode \"{other}\" (two_point, center or three_point)"
        )),
    }
}

/// What a plane-bound shape made (mitcad#1550): `made` its outline's
/// curves, `corners`, `center`, `construction` curves and `relations`.
fn shape_json(report: &SpatialEditReport, made: &ShapeMade) -> Value {
    let mut value = report_json(report);
    value["made"] = json!(
        made.curves
            .iter()
            .map(|c| curve_name(*c))
            .collect::<Vec<_>>()
    );
    value["corners"] = json!(names_of_points(&made.points));
    if let Some(c) = made.center {
        value["center"] = json!(point_name(c));
    }
    value["construction"] = json!(
        made.construction
            .iter()
            .map(|c| curve_name(*c))
            .collect::<Vec<_>>()
    );
    value["relations"] = json!(names(&made.constraints));
    value
}

/// A point pulled toward a local target by a drag.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DragGoal {
    point: String,
    to: [f64; 3],
}

/// An open fit-point spline's end tangent `C'(t)` pulled toward `to` (the
/// spline's parameter running from 0 to 1) at its `start` or `end`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TangentGoal {
    curve: String,
    end: String,
    to: [f64; 3],
}

fn tangent_pull(goal: &TangentGoal) -> Result<SpatialTangentPull, String> {
    let end = match goal.end.as_str() {
        "start" => 0,
        "end" => 1,
        other => {
            return Err(format!("unknown tangent end \"{other}\" (start or end)"));
        }
    };
    Ok(SpatialTangentPull {
        curve: curve_id(&goal.curve)?,
        end,
        tangent: goal.to,
    })
}

pub(super) fn parse_sketch3d_command(value: Value) -> Result<Sketch3dCommand, ApiError> {
    serde_json::from_value(value).map_err(|e| ApiError(format!("invalid 3D sketch command: {e}")))
}

fn point_id(text: &str) -> Result<EntityUid, String> {
    mitcad_model::sketch::internal::point_serde::parse(text).map_err(|e| e.to_string())
}

fn curve_id(text: &str) -> Result<EntityUid, String> {
    EntityUid::parse_curve(text).map_err(|e| e.to_string())
}

fn constraint_id(text: &str) -> Result<ConstraintUid, String> {
    text.parse::<ConstraintUid>().map_err(|e| e.to_string())
}

fn point_name(id: EntityUid) -> String {
    format!("p{}", id.0)
}

fn curve_name(id: EntityUid) -> String {
    format!("c{}", id.0)
}

fn names<T: ToString>(items: &[T]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

/// A point of a command: `[x, y, z]` (a new point at local coordinates) or
/// an existing point's id. Two coordinates are refused: a 3D sketch's
/// points have three.
fn point_input(value: &Value) -> Result<SpatialPointInput, String> {
    match value {
        Value::String(text) => point_id(text).map(SpatialPointInput::Existing),
        Value::Array(items) if items.len() == 3 => {
            let mut at = [0.0; 3];
            for (k, item) in items.iter().enumerate() {
                at[k] = item
                    .as_f64()
                    .ok_or("a point's coordinates are numbers (millimetres)")?;
            }
            Ok(SpatialPointInput::At(at))
        }
        Value::Array(items) => Err(format!(
            "a 3D sketch's point has three coordinates [x, y, z], not {}",
            items.len()
        )),
        _ => Err("a point is [x, y, z] (a new point) or a point id like \"p3\"".to_owned()),
    }
}

/// A source of a command (mitcad#1543): as the definition writes it
/// ([`SpatialSource`]), or with `occurrence` (and `context`), as
/// `sketch.project` takes them, for geometry of another component picked
/// where an occurrence shows it: the record's link is made from those
/// paths (none for the sketch's own component).
fn source_input<K: Kernel>(
    doc: &Document<K>,
    sketch: FeatureUid,
    value: &Value,
) -> Result<SpatialSource, ApiError> {
    let mut value = value.clone();
    if let Value::Object(map) = &mut value
        && (map.contains_key("occurrence") || map.contains_key("context"))
    {
        let mut path = |field: &str| -> Result<Option<OccurrencePath>, ApiError> {
            map.remove(field)
                .filter(|v| !v.is_null())
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| ApiError(format!("a source's \"{field}\": {e}")))
        };
        let occurrence = path("occurrence")?;
        let context = path("context")?;
        let Some(occurrence) = occurrence else {
            return Err(ApiError(
                "a source's \"context\" needs its \"occurrence\"".to_owned(),
            ));
        };
        if map.contains_key("link") {
            return Err(ApiError(
                "a source has \"occurrence\" or \"link\", not both".to_owned(),
            ));
        }
        let component = doc
            .feature(sketch)
            .map(|entry| entry.component)
            .ok_or_else(|| ApiError(format!("feature {sketch} does not exist")))?;
        let link = mitcad_model::internal::links::link_for(
            doc.assembly(),
            component,
            &occurrence.0,
            context.as_ref().map(|c| &c.0[..]),
        )
        .map_err(|e| ApiError(format!("a source's occurrence: {e}")))?;
        if let Some(link) = link {
            let link = serde_json::to_value(link).expect("a link serializes");
            map.insert("link".to_owned(), link);
        }
    }
    SpatialSource::from_json(value).map_err(ApiError)
}

/// An optional source of a command ([`source_input`]).
fn optional_source<K: Kernel>(
    doc: &Document<K>,
    sketch: FeatureUid,
    value: Option<&Value>,
) -> Result<Option<SpatialSource>, ApiError> {
    value.map(|v| source_input(doc, sketch, v)).transpose()
}

/// The linked reference records of a sketch (mitcad#1543): each
/// attachment (`k`) and reference curve (`c`) with its source, body,
/// occurrence path, mode (`include` or `project`), what uses it and its
/// status in the last evaluation (`ok`, `failed` with the message, or
/// `unknown` when the evaluation did not reach it).
fn references_json(def: &SpatialSketchDef, output: Option<&SpatialSketchOutput>) -> Vec<Value> {
    let failure = output
        .and_then(|o| o.failure.as_ref())
        .filter(|f| f.kind == SpatialFailureKind::Reference);
    let record = |source: &SpatialSource, projected: bool, found: bool, failed: bool| {
        let mut value = json!({
            "kind": source.kind_name(),
            "source": source,
            "mode": if projected { "project" } else { "include" },
            "status": if failed { "failed" } else if found { "ok" } else { "unknown" },
        });
        if let Some(body) = source.body() {
            value["body"] = json!(body);
        }
        if let Some(link) = source.link() {
            value["occurrence"] = json!(link.source);
        }
        // In plain words, the agents' text beside it (mitcad#1644).
        if let (true, Some(failure)) = (failed, failure) {
            value["message"] = json!(failure.user_message());
            value["detail"] = json!(failure.agent_text());
        }
        value
    };
    let mut list = Vec::new();
    for (id, point, source, projected) in def.attachment_records() {
        let found = output.is_some_and(|o| o.targets.contains_key(&id));
        let failed = failure.is_some_and(|f| f.constraints == [id]);
        let mut value = record(source, projected, found, failed);
        value["id"] = json!(id.to_string());
        value["users"] = json!([point_name(point)]);
        list.push(value);
    }
    for (id, source, projected) in def.reference_records() {
        let found = output.is_some_and(|o| o.references.contains_key(&id));
        let failed = failure.is_some_and(|f| f.entities == [id]);
        let mut value = record(source, projected, found, failed);
        value["id"] = json!(curve_name(id));
        value["users"] = json!(names(&def.reference_users(id)));
        list.push(value);
    }
    // Points on faces (mitcad#1549): each On Face relation is its face's
    // record (`kind` "face").
    for (id, point, source) in def.face_records() {
        let found = output.is_some_and(|o| o.surfaces.contains_key(&id));
        let failed = failure.is_some_and(|f| f.constraints == [id]);
        let mut value = record(source, false, found, failed);
        value["id"] = json!(id.to_string());
        value["users"] = json!([point_name(point)]);
        list.push(value);
    }
    list
}

/// An axis with its sense; a sense without an axis is refused.
fn axis_of(
    axis: Option<SpatialAxis>,
    sense: Option<SpatialSense>,
    field: &str,
) -> Result<Option<(SpatialAxis, SpatialSense)>, String> {
    match (axis, sense) {
        (Some(axis), sense) => Ok(Some((axis, sense.unwrap_or_default()))),
        (None, Some(_)) => Err(format!("a sense needs \"{field}\"")),
        (None, None) => Ok(None),
    }
}

fn need<T>(value: Option<T>, field: &str, mode: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("\"{mode}\" needs \"{field}\""))
}

/// A field that the mode does not take.
fn refuse_field<T>(value: &Option<T>, field: &str, mode: &str) -> Result<(), String> {
    match value {
        Some(_) => Err(format!("\"{mode}\" takes no \"{field}\"")),
        None => Ok(()),
    }
}

/// The local unit vector of an axis in a sense.
fn axis_vector(axis: SpatialAxis, sense: SpatialSense) -> [f64; 3] {
    let mut v = [0.0; 3];
    v[axis.index()] = sense.sign();
    v
}

/// A refused edit: the error names the relations and entities of a
/// refused solve.
fn refusal(error: SpatialEditError) -> ApiError {
    ApiError(error.to_string())
}

/// Edits a 3D sketch as one undo step, `what` naming it.
fn edit<K: Kernel, T>(
    doc: &mut Document<K>,
    sketch: FeatureUid,
    what: &str,
    change: impl FnOnce(&mut SpatialEdit<'_>) -> Result<T, String>,
) -> Result<(T, SpatialEditReport), ApiError> {
    doc.edit_spatial_sketch(sketch, what, change)
        .map_err(refusal)
}

/// The status after a solve: degrees of freedom, what is determined, the
/// redundant relations and every dimension's measured value.
fn status_json(status: &SpatialSketchStatus) -> Value {
    let measured: serde_json::Map<String, Value> = status
        .measured
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    json!({
        "dof": status.dof,
        "fully_constrained": status.fully_constrained,
        "determined": {
            "points": status.determined_points.iter().map(|p| point_name(*p)).collect::<Vec<_>>(),
            "lines": status.determined_lines.iter().map(|c| curve_name(*c)).collect::<Vec<_>>(),
            "curves": status.determined_curves.iter().map(|c| curve_name(*c)).collect::<Vec<_>>(),
        },
        "redundant": names(&status.redundant),
        "measured": measured,
    })
}

/// The result of an edit: the new ids (`entities` in the order they were
/// made), the parameters created, what was removed and the status.
fn report_json(report: &SpatialEditReport) -> Value {
    let mut entities: Vec<(EntityUid, String)> = report
        .points
        .iter()
        .map(|p| (*p, point_name(*p)))
        .chain(report.curves.iter().map(|c| (*c, curve_name(*c))))
        .collect();
    entities.sort_by_key(|(id, _)| id.0);
    let mut value = json!({
        "entities": entities.into_iter().map(|(_, name)| name).collect::<Vec<_>>(),
        "points": report.points.iter().map(|p| point_name(*p)).collect::<Vec<_>>(),
        "curves": report.curves.iter().map(|c| curve_name(*c)).collect::<Vec<_>>(),
        "constraints": names(&report.constraints),
        "dimensions": names(&report.dimensions),
        "parameters": report.parameters,
        "status": status_json(&report.status),
    });
    if !report.removed.is_empty() {
        value["removed"] = json!(report.removed);
    }
    // Own parameters kept as user parameters (mitcad#1535), as
    // `sketch.set_dimension` reports them.
    if !report.notes.is_empty() {
        value["notes"] = json!(report.notes);
    }
    if !report.kept.is_empty() {
        let kept: Vec<Value> = report
            .kept
            .iter()
            .map(|k| {
                json!({"name": k.name, "dimension": k.dimension.to_string(),
                       "parameter": k.parameter})
            })
            .collect();
        value["user_parameters"] = json!(kept);
    }
    if !report.texts.is_empty() {
        value["texts"] = json!(
            report
                .texts
                .iter()
                .map(|t| format!("t{}", t.0))
                .collect::<Vec<_>>()
        );
    }
    value
}

/// One corner a fillet rounded (mitcad#1548): the corner, its lines, the
/// arc with its centre and tangent points, the construction lines to the
/// corner and those that took the lines' segment relations.
fn fillet_json(made: &FilletMade) -> Value {
    let curves = |ids: &[EntityUid]| ids.iter().map(|c| curve_name(*c)).collect::<Vec<_>>();
    json!({
        "corner": point_name(made.corner),
        "lines": curves(&made.lines[..]),
        "arc": curve_name(made.arc),
        "center": point_name(made.center),
        "tangent_points": names_of_points(&made.tangent_points[..]),
        "corner_lines": curves(&made.corner_lines[..]),
        "full_lines": curves(made.full_lines.as_slice()),
    })
}

/// An entity of a failure by its kind's prefix, as the definition has it.
fn entity_name(def: &SpatialSketchDef, id: EntityUid) -> String {
    if def.point(id).is_some() {
        point_name(id)
    } else if def.curve(id).is_some() {
        curve_name(id)
    } else {
        format!("t{}", id.0)
    }
}

/// A failure as the query gives it (mitcad#1644): `message` in plain words
/// for a person, `detail` with the ids for agents (when it says more),
/// `places` the vertices, edges and faces of bodies it is about; its
/// relations and entities (the sketch's own places) by id.
fn failure_json(def: &SpatialSketchDef, failure: &SpatialFailure) -> Value {
    let error = failure.error();
    let mut value = json!({
        "kind": failure.kind.as_str(),
        "message": error.message,
        "constraints": names(&failure.constraints),
        "entities": failure.entities.iter().map(|e| entity_name(def, *e)).collect::<Vec<_>>(),
    });
    if !error.detail.is_empty() {
        value["detail"] = json!(error.detail);
    }
    if !error.places.is_empty() {
        value["places"] = json!(error.places);
    }
    value
}

/// A solved curve's own geometry in local coordinates: a line's ends and
/// length; an arc's or circle's normal and radius (an arc's sweep and
/// length too); a spline's degree, knots, weights and control points (a
/// fit-point spline's hidden ones included, positions only).
fn curve_geometry(solved: &SpatialSolved, curve: &SpatialCurve) -> Option<Value> {
    let at = |p: &EntityUid| solved.points.get(p).copied();
    if let SpatialCurveKind::Line { start, end } = &curve.kind {
        let (s, e) = (at(start)?, at(end)?);
        return Some(json!({"start": s, "end": e, "length": mitcad_vec3::distance(s, e)}));
    }
    match solved.curves.get(&curve.id)? {
        SolvedCurve::Round { normal, radius } => {
            let mut value = json!({"normal": normal, "radius": radius});
            if let SpatialCurveKind::Arc {
                center, start, end, ..
            } = &curve.kind
                && let (Some(c), Some(s), Some(e)) = (at(center), at(start), at(end))
            {
                let sweep = sweep_about(*normal, mitcad_vec3::sub(s, c), mitcad_vec3::sub(e, c));
                value["sweep"] = json!(sweep);
                value["length"] = json!(radius * sweep);
            }
            Some(value)
        }
        SolvedCurve::Spline(spline) => {
            let mut value = json!({
                "degree": spline.degree,
                "knots": spline.knots,
                "weights": spline.weights,
                "control_points": spline.control_points,
            });
            // An open fit-point spline's end tangents C'(t), where its
            // tangent handles are (mitcad#1526).
            if let SpatialCurveKind::FittedSpline { closed: false, .. } = &curve.kind {
                value["end_tangents"] =
                    json!([spline_end_tangent(spline, 0), spline_end_tangent(spline, 1)]);
            }
            Some(value)
        }
        // A plane-bound ellipse's radii and its plane's normal (mitcad#1550).
        SolvedCurve::Ellipse { major, minor } => {
            let normal = curve.ellipse_normal().unwrap_or([0.0, 0.0, 1.0]);
            Some(json!({"major": major, "minor": minor, "normal": normal}))
        }
    }
}

/// A reference curve's geometry in local coordinates, as found before the
/// solve (mitcad#1524: the editing context draws it): `kind` `line`
/// (`start`, `end`), `conic` (a circle or an ellipse: `center`, `normal`,
/// `x_axis`, `major`, `minor` radii and the parameter range `start`, `end`,
/// or the whole curve when `closed`), `bspline` (`degree`, `poles`,
/// `weights`, empty for a non-rational one, and the full `knots`) or
/// `helix` (`center`, `axis`, `x_axis`, `radius`, `pitch`, `start`, `end`).
fn reference_geometry(curve: &SpatialReferenceCurve) -> Value {
    match curve {
        SpatialReferenceCurve::Line { start, end } => {
            json!({"kind": "line", "start": start, "end": end})
        }
        SpatialReferenceCurve::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => json!({"kind": "conic", "center": center, "normal": normal, "x_axis": x_axis,
                    "major": major, "minor": minor, "start": start, "end": end, "closed": closed}),
        SpatialReferenceCurve::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => json!({"kind": "bspline", "degree": degree, "poles": poles, "weights": weights,
                    "knots": knots}),
        SpatialReferenceCurve::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => json!({"kind": "helix", "center": center, "axis": axis, "x_axis": x_axis,
                    "radius": radius, "pitch": pitch, "start": start, "end": end}),
    }
}

/// How an arc of `mode` finds its centre and plane, from the fields that
/// mode takes; a field of another mode is refused.
fn arc_placement(
    mode: &str,
    through: Option<[f64; 3]>,
    center: Option<Value>,
    normal: Option<[f64; 3]>,
    from: Option<String>,
    normal_axis: Option<(SpatialAxis, SpatialSense)>,
) -> Result<ArcPlacement, String> {
    match mode {
        "three_point" => {
            refuse_field(&center, "center", mode)?;
            refuse_field(&normal, "normal", mode)?;
            refuse_field(&from, "from", mode)?;
            Ok(ArcPlacement::ThreePoint {
                through: need(through, "through", mode)?,
            })
        }
        "center" => {
            refuse_field(&through, "through", mode)?;
            refuse_field(&from, "from", mode)?;
            let center = point_input(&need(center, "center", mode)?)?;
            // The active plane's normal when only `normal_axis` is given.
            let normal = match (normal, normal_axis) {
                (Some(normal), _) => normal,
                (None, Some((axis, sense))) => axis_vector(axis, sense),
                (None, None) => {
                    return Err("\"center\" needs \"normal\" (or \"normal_axis\")".to_owned());
                }
            };
            Ok(ArcPlacement::Centre { center, normal })
        }
        "tangent" => {
            refuse_field(&through, "through", mode)?;
            refuse_field(&center, "center", mode)?;
            refuse_field(&normal, "normal", mode)?;
            Ok(ArcPlacement::Tangent {
                curve: curve_id(&need(from, "from", mode)?)?,
            })
        }
        other => Err(format!(
            "unknown arc mode \"{other}\" (three_point, center or tangent)"
        )),
    }
}

/// Adds a relation given as in the definition, without `id`: the edit's
/// own checks for each kind (Fix where the point is now when `at` is left
/// out, a tangency's sense from the geometry when `sense` is left out, On
/// Sketch Plane by the points' elevation groups).
fn add_constraint(e: &mut SpatialEdit<'_>, constraint: &Value) -> Result<ConstraintUid, String> {
    if constraint.get("type").and_then(Value::as_str) == Some("fix")
        && constraint.get("at").is_none()
    {
        let fields = constraint.as_object().map_or(0, serde_json::Map::len);
        let point = constraint
            .get("point")
            .and_then(Value::as_str)
            .ok_or("a fix needs \"point\"")?;
        if fields != 2 {
            return Err("a fix takes \"point\" and, optionally, \"at\"".to_owned());
        }
        return e.fix(point_id(point)?);
    }
    let kind: SpatialConstraintKind = serde_json::from_value(constraint.clone())
        .map_err(|error| format!("constraint: {error}"))?;
    match kind {
        SpatialConstraintKind::Attach {
            point,
            source,
            projected,
        } => e.attach_as(point, source, projected),
        // A point on a face (mitcad#1549), as sketch3d.attach makes it.
        SpatialConstraintKind::OnFace { point, source } => e.attach(point, source),
        SpatialConstraintKind::Align { line, axis, sense } => e.align(line, axis, sense),
        SpatialConstraintKind::Tangent { a, b, at, sense } => {
            let id = e.tangent(a, b, at)?;
            if let Some(sense) = sense
                && let Some(SpatialConstraintDef {
                    kind: SpatialConstraintKind::Tangent { sense: kept, .. },
                    ..
                }) = e.def.constraints.iter_mut().find(|c| c.id == id)
            {
                *kept = Some(sense);
            }
            Ok(id)
        }
        SpatialConstraintKind::Equal { a, b } => e.equal(a, b),
        SpatialConstraintKind::NormalAlign { curve, axis, sense } => {
            e.align_normal(curve, axis, sense)
        }
        SpatialConstraintKind::OnSketchPlane { points } => e.keep_on_sketch_plane(&points),
        SpatialConstraintKind::InPlane { .. }
        | SpatialConstraintKind::InPlaneFixed { .. }
        | SpatialConstraintKind::InFrame { .. } => Err(
            "in-plane relations are kept from a converted planar sketch; relations added to a 3D \
             sketch are 3D ones"
                .to_owned(),
        ),
        kind => e.add_constraint(kind),
    }
}

/// The result of `sketch3d.convert` (mitcad#1544): the kept ids, what the
/// planar relations became, the plane group, the attachments and
/// reference curves of linked projections, the rewritten paths and the
/// status.
fn conversion_json<K: Kernel>(
    doc: &Document<K>,
    sketch: FeatureUid,
    converted: &SketchConversion,
) -> Value {
    let name = doc
        .feature(sketch)
        .map(|f| f.name.clone())
        .unwrap_or_default();
    let ids = |list: &[ConstraintUid]| names(list);
    let points = |list: &[EntityUid]| list.iter().map(|p| point_name(*p)).collect::<Vec<_>>();
    let mut kept = json!({});
    if let Some(def) = doc.spatial_sketch_def(sketch) {
        let new: BTreeSet<ConstraintUid> = converted
            .fixes
            .iter()
            .chain(&converted.attachments)
            .chain(converted.on_sketch_plane.iter().map(|(k, _)| k))
            .copied()
            .collect();
        kept = json!({
            "points": def.points.iter().map(|p| point_name(p.id)).collect::<Vec<_>>(),
            "curves": def.curves.iter().map(|c| curve_name(c.id)).collect::<Vec<_>>(),
            "texts": def.texts.iter().map(|t| format!("t{}", t.planar.id.0)).collect::<Vec<_>>(),
            "constraints": def.constraints.iter().map(|c| c.id).filter(|k| !new.contains(k))
                .map(|k| k.to_string()).collect::<Vec<_>>(),
            "dimensions": def.dimensions.iter().map(|d| d.id.to_string()).collect::<Vec<_>>(),
        });
    }
    let mut value = json!({
        "uid": sketch,
        "name": name,
        "type": <SpatialSketchDef>::TYPE,
        "kept": kept,
        "in_plane": ids(&converted.in_plane),
        "coincident": ids(&converted.coincident),
        "fixes": ids(&converted.fixes),
        "attachments": ids(&converted.attachments),
        "references": converted.references.iter().map(|c| curve_name(*c)).collect::<Vec<_>>(),
        "dropped": points(&converted.dropped),
        "rewritten": converted.rewritten.iter()
            .map(|(uid, name)| json!({"uid": uid, "name": name}))
            .collect::<Vec<_>>(),
        "status": status_json(&converted.status),
    });
    if let Some((relation, kept)) = &converted.on_sketch_plane {
        value["on_sketch_plane"] =
            json!({"relation": relation.to_string(), "points": points(kept)});
    }
    // What the 3D sketch keeps of the planar sketch in its planar records
    // (mitcad#1550).
    if !converted.kept.is_empty() {
        value["planar"] = json!(converted.kept);
    }
    value
}

/// The result of `sketch3d.convert_to_2d` (mitcad#1550): the planar
/// sketch's ids, the 3D relations it left out (`dropped`), the attachments
/// and reference curves that became linked projections (`projected`), the
/// rewritten paths, notes and the planar sketch's freedom.
fn planar_conversion_json<K: Kernel>(
    doc: &Document<K>,
    sketch: FeatureUid,
    converted: &PlanarConversion,
) -> Value {
    let name = doc
        .feature(sketch)
        .map(|f| f.name.clone())
        .unwrap_or_default();
    let mut kept = json!({});
    if let Some(FeatureDef::Sketch(def)) = doc.feature(sketch).map(|f| &f.def) {
        kept = json!({
            "points": def.entities.iter().filter(|e| e.is_point())
                .map(|e| point_name(e.id)).collect::<Vec<_>>(),
            "curves": def.entities.iter().filter(|e| !e.is_point())
                .map(|e| curve_name(e.id)).collect::<Vec<_>>(),
            "texts": def.texts.iter().map(|t| format!("t{}", t.id.0)).collect::<Vec<_>>(),
            "constraints": def.constraints.iter().map(|c| c.id.to_string()).collect::<Vec<_>>(),
            "dimensions": def.dimensions.iter().map(|d| d.id.to_string()).collect::<Vec<_>>(),
        });
    }
    let dof = doc.sketch_output(sketch).map(|o| o.solved.status.dof);
    json!({
        "uid": sketch,
        "name": name,
        "type": "sketch",
        "kept": kept,
        "dropped": names(&converted.dropped),
        "projected": converted.projected,
        "notes": converted.notes,
        "rewritten": converted.rewritten.iter()
            .map(|(uid, name)| json!({"uid": uid, "name": name}))
            .collect::<Vec<_>>(),
        "dof": dof,
    })
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait Sketch3dOps<K: Kernel> {
    fn run_sketch3d_command(&mut self, command: Sketch3dCommand) -> Result<Value, ApiError>;

    /// The `sketch3d` query: the definition with its last evaluation.
    fn sketch3d_json(&self, uid: FeatureUid) -> Result<Value, ApiError>;

    /// The `sketch3d` query's `route` of a path along curves of the sketch
    /// `uid` (mitcad#1527), which the query has read already.
    fn sketch3d_route_json(&self, uid: FeatureUid, path: &RouteQuery) -> Value;
}

/// Curves of a 3D sketch as a Pipe's or a Sweep's path (mitcad#1527), for
/// the `sketch3d` query's `route`: the typed path's fields without the
/// sketch, which the query names.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RouteQuery {
    curves: Vec<String>,
    #[serde(default)]
    chain: bool,
    #[serde(default)]
    reversed: bool,
}

/// A route's refusal (mitcad#1644): `error` with the ids and points for
/// agents, `message` in plain words for a person, `places` the points it
/// is about.
fn route_error(text: &str) -> Value {
    let error = notice::FeatureError::from_text(text);
    let mut value = json!({"error": error.agent_text(), "message": error.message});
    if !error.places.is_empty() {
        value["places"] = json!(error.places);
    }
    value
}

/// A route as the `sketch3d` query gives it (mitcad#1527): the curves in
/// order along the path, each with whether the path runs along its own
/// direction, and the points where it starts and ends; or why the curves
/// are no path (`error`, as a Pipe along them would fail).
fn route_json(
    def: &SpatialSketchDef,
    uid: FeatureUid,
    name: &str,
    path: &RouteQuery,
    output: Option<&SpatialSketchOutput>,
) -> Value {
    let curves: Result<Vec<EntityUid>, String> = path.curves.iter().map(|c| curve_id(c)).collect();
    let curves = match curves {
        Ok(curves) => curves,
        Err(error) => return route_error(&error),
    };
    let typed = spatial_path::SpatialPath {
        sketch: uid,
        curves,
        reversed: path.reversed,
        chain: path.chain,
    };
    let at = |p: EntityUid| output.and_then(|o| o.point(p));
    let legs = match spatial_path::route(def, &typed, name, &at) {
        Ok(legs) => legs,
        Err(error) => return route_error(&error),
    };
    // The curve's own end the path leaves it by, or reaches it at.
    let end = |leg: &spatial_path::Leg, last: bool| {
        def.curve(leg.curve)
            .and_then(SpatialCurve::ends)
            .map(|[a, b]| point_name(if leg.forward == last { b } else { a }))
    };
    let (Some(first), Some(last)) = (legs.first(), legs.last()) else {
        return json!({"error": "no curves selected"});
    };
    // A closed route (mitcad#1550): round to where it starts; a closed
    // curve has no start or end point.
    json!({
        "curves": legs.iter().map(|l| curve_name(l.curve)).collect::<Vec<_>>(),
        "forward": legs.iter().map(|l| l.forward).collect::<Vec<_>>(),
        "start": end(first, false),
        "end": end(last, true),
        "closed": spatial_path::is_closed_route(def, &legs),
    })
}

impl<K: Kernel> Sketch3dOps<K> for Document<K> {
    fn run_sketch3d_command(&mut self, command: Sketch3dCommand) -> Result<Value, ApiError> {
        Ok(match command {
            Sketch3dCommand::Create { support, name } => {
                // In the input form, as add_feature takes definitions.
                let def: SpatialSketchDef<ValueInput> = SpatialSketchDef::new(support);
                let added = self.add_feature(&FeatureDef::SpatialSketch(def), name.as_deref())?;
                json!({"uid": added.uid, "name": added.name})
            }
            Sketch3dCommand::AddPoint {
                sketch,
                at,
                source,
                fixed,
            } => {
                let source = optional_source(self, sketch, source.as_ref())?;
                let (point, report) = edit(self, sketch, "Add Point", |e| {
                    let point = e.add_point(at)?;
                    if let Some(source) = source {
                        e.attach(point, source)?;
                    }
                    if fixed {
                        e.fix(point)?;
                    }
                    Ok(point)
                })?;
                let mut value = report_json(&report);
                value["made"] = json!([point_name(point)]);
                value
            }
            Sketch3dCommand::AddLine {
                sketch,
                start,
                end,
                construction,
                axis,
                sense,
                length,
                offsets,
                start_source,
                end_source,
                frame,
            } => {
                let frame = frame
                    .as_ref()
                    .map(rotated_frame)
                    .transpose()
                    .map_err(ApiError)?;
                let request = SegmentRequest {
                    start: point_input(&start).map_err(ApiError)?,
                    end: point_input(&end).map_err(ApiError)?,
                    construction,
                    axis: axis_of(axis, sense, "axis").map_err(ApiError)?,
                    length,
                    offsets: offsets.unwrap_or([None, None, None]),
                    start_source: optional_source(self, sketch, start_source.as_ref())?,
                    end_source: optional_source(self, sketch, end_source.as_ref())?,
                };
                let (made, report) = edit(self, sketch, "Add Line", |e| {
                    e.add_segment_in(&request, frame)
                })?;
                let mut value = report_json(&report);
                value["made"] = json!([curve_name(made.line)]);
                value["start"] = json!(point_name(made.start));
                value["end"] = json!(point_name(made.end));
                value
            }
            Sketch3dCommand::AddArc {
                sketch,
                mode,
                start,
                end,
                through,
                center,
                normal,
                from,
                construction,
                normal_axis,
                normal_sense,
                radius,
                arc_length,
                start_source,
                end_source,
            } => {
                let normal_axis =
                    axis_of(normal_axis, normal_sense, "normal_axis").map_err(ApiError)?;
                let placement = arc_placement(&mode, through, center, normal, from, normal_axis)
                    .map_err(ApiError)?;
                let request = ArcRequest {
                    start: point_input(&start).map_err(ApiError)?,
                    end: point_input(&end).map_err(ApiError)?,
                    placement,
                    construction,
                    normal_axis,
                    radius,
                    arc_length,
                    start_source: optional_source(self, sketch, start_source.as_ref())?,
                    end_source: optional_source(self, sketch, end_source.as_ref())?,
                };
                let (made, report) = edit(self, sketch, "Add Arc", |e| e.add_arc(&request))?;
                let mut value = report_json(&report);
                value["made"] = json!([curve_name(made.arc)]);
                value["center"] = json!(point_name(made.center));
                value["start"] = json!(point_name(made.start));
                value["end"] = json!(point_name(made.end));
                value
            }
            Sketch3dCommand::AddCircle {
                sketch,
                center,
                radius,
                normal,
                normal_axis,
                normal_sense,
                plane,
                through,
                dimension,
                diameter,
                construction,
            } => {
                let center = point_input(&center).map_err(ApiError)?;
                let normal_axis =
                    axis_of(normal_axis, normal_sense, "normal_axis").map_err(ApiError)?;
                // Plane-bound on a frame plane (mitcad#1550).
                let on = plane
                    .as_ref()
                    .map(shape_plane)
                    .transpose()
                    .map_err(ApiError)?;
                if on.is_some() && (normal.is_some() || normal_axis.is_some()) {
                    return Err(ApiError(
                        "a circle on a \"plane\" takes no \"normal\" or \"normal_axis\"".to_owned(),
                    ));
                }
                let bound = match on {
                    Some(SpatialPlane::Axis(axis)) => Some(SpatialDirection {
                        axis,
                        sense: SpatialSense::Positive,
                    }),
                    _ => None,
                };
                // A rotated plane's (mitcad#1661): the normal held along
                // its normal.
                let held = on.and_then(|p| p.frame()).map(|f| f.normal);
                let normal = match (normal, normal_axis, bound) {
                    (_, _, Some(direction)) => direction.unit(),
                    (_, _, None) if held.is_some() => held.unwrap_or([0.0, 0.0, 1.0]),
                    (Some(normal), _, None) => normal,
                    (None, Some((axis, sense)), None) => axis_vector(axis, sense),
                    (None, None, None) => {
                        return Err(ApiError(
                            "a circle needs \"normal\", \"normal_axis\" or \"plane\"".to_owned(),
                        ));
                    }
                };
                if !dimension && !matches!(radius, ValueInput::Number(_)) {
                    return Err(ApiError(
                        "without a dimension a circle's radius is a number".to_owned(),
                    ));
                }
                // `through` is optional: no points held on the circle.
                let through = if through.is_empty() {
                    Vec::new()
                } else {
                    point_ids(&through).map_err(ApiError)?
                };
                let (circle, report) = edit(self, sketch, "Add Circle", |e| {
                    // Where the circle starts: the value when it is known.
                    let start = match &radius {
                        ValueInput::Number(v) => *v,
                        ValueInput::Name(name) => e
                            .params()
                            .find(name)
                            .and_then(|id| e.params().value(id))
                            .unwrap_or(1.0),
                    };
                    let start = if diameter { start / 2.0 } else { start };
                    let circle = e.add_circle(center, start, normal, bound, construction)?;
                    if let (Some((axis, sense)), None) = (normal_axis, bound) {
                        e.align_normal(circle, axis, sense)?;
                    }
                    if let Some(direction) = held {
                        e.add_constraint(SpatialConstraintKind::NormalAlong {
                            curve: circle,
                            direction,
                        })?;
                    }
                    for p in &through {
                        e.point_on(*p, circle)?;
                    }
                    if dimension {
                        let kind = if diameter {
                            SpatialDimensionKind::Diameter { curve: circle }
                        } else {
                            SpatialDimensionKind::Radius { curve: circle }
                        };
                        let k = e.add_dimension(kind, Some(&radius), false)?;
                        // An expression's value, once its parameter has one.
                        let value = e
                            .def
                            .dimension(k)
                            .and_then(|d| d.value)
                            .and_then(|id| e.params().value(id))
                            .filter(|v| v.is_finite() && *v > 0.0);
                        if let Some(value) = value
                            && let Some(SpatialCurve {
                                kind: SpatialCurveKind::Circle { radius: r, .. },
                                ..
                            }) = e.def.curves.iter_mut().find(|c| c.id == circle)
                        {
                            *r = if diameter { value / 2.0 } else { value };
                        }
                    }
                    Ok(circle)
                })?;
                let mut value = report_json(&report);
                value["made"] = json!([curve_name(circle)]);
                value
            }
            Sketch3dCommand::AddEllipse {
                sketch,
                center,
                major,
                minor_radius,
                plane,
                major_value,
                minor_value,
                construction,
            } => {
                let center = point_input(&center).map_err(ApiError)?;
                let major = point_input(&major).map_err(ApiError)?;
                let on = shape_plane(&plane).map_err(ApiError)?;
                let (ellipse, report) = edit(self, sketch, "Add Ellipse", |e| {
                    let ellipse =
                        e.add_ellipse_on(center, major, minor_radius, on, construction)?;
                    if let Some(value) = &major_value {
                        e.add_dimension(
                            SpatialDimensionKind::MajorRadius { curve: ellipse },
                            Some(value),
                            false,
                        )?;
                    }
                    if let Some(value) = &minor_value {
                        e.add_dimension(
                            SpatialDimensionKind::MinorRadius { curve: ellipse },
                            Some(value),
                            false,
                        )?;
                    }
                    Ok(ellipse)
                })?;
                let mut value = report_json(&report);
                value["made"] = json!([curve_name(ellipse)]);
                value
            }
            Sketch3dCommand::AddRectangle {
                sketch,
                mode,
                a,
                b,
                c,
                center,
                corner,
                plane,
                width,
                height,
                construction,
            } => {
                let on = shape_plane(&plane).map_err(ApiError)?;
                let request = RectangleRequest {
                    corners: rectangle_corners(&mode, a, b, c, center, corner).map_err(ApiError)?,
                    normal: plane_normal(on),
                    construction,
                };
                let (made, report) = edit(self, sketch, "Add Rectangle", |e| {
                    let made = e.add_rectangle_on(&request, on)?;
                    let sides = made.sides.unwrap_or([made.curves[0], made.curves[1]]);
                    for (line, value) in sides.iter().zip([&width, &height]) {
                        if let Some(value) = value {
                            e.add_dimension(
                                SpatialDimensionKind::Length { line: *line },
                                Some(value),
                                false,
                            )?;
                        }
                    }
                    Ok(made)
                })?;
                shape_json(&report, &made)
            }
            Sketch3dCommand::AddPolygon {
                sketch,
                center,
                vertex,
                sides,
                inscribed,
                plane,
                radius,
                construction,
            } => {
                let on = shape_plane(&plane).map_err(ApiError)?;
                let request = PolygonRequest {
                    center: point_input(&center).map_err(ApiError)?,
                    vertex: point_input(&vertex).map_err(ApiError)?,
                    sides,
                    inscribed,
                    normal: plane_normal(on),
                    construction,
                };
                let (made, report) = edit(self, sketch, "Add Polygon", |e| {
                    let made = e.add_polygon_on(&request, on)?;
                    if let (Some(value), Some(circle)) = (&radius, made.construction.first()) {
                        e.add_dimension(
                            SpatialDimensionKind::Radius { curve: *circle },
                            Some(value),
                            false,
                        )?;
                    }
                    Ok(made)
                })?;
                shape_json(&report, &made)
            }
            Sketch3dCommand::AddText {
                sketch,
                text,
                at,
                plane,
                height,
                frame,
                angle,
                font,
                bold,
                italic,
                align,
                valign,
            } => {
                let on = shape_plane(&plane).map_err(ApiError)?;
                let mut planar = SketchText::new(EntityUid(0), &text, [0.0, 0.0], height);
                planar.angle = angle;
                planar.font = font;
                planar.bold = bold;
                planar.italic = italic;
                planar.align = align;
                planar.valign = valign;
                let request = TextRequest {
                    text: planar,
                    at,
                    frame,
                    normal: plane_normal(on),
                };
                let (id, report) = edit(self, sketch, "Add Text", |e| e.add_text_on(&request, on))?;
                let mut value = report_json(&report);
                value["made"] = json!([format!("t{}", id.0)]);
                value
            }
            Sketch3dCommand::AddSpline {
                sketch,
                points,
                degree,
                closed,
                ends,
                construction,
            } => {
                if ends.is_some() && degree.is_some() {
                    return Err(ApiError(
                        "\"ends\" are a fit-point spline's: a spline with \"degree\" ends at its \
                         first and last control points"
                            .to_owned(),
                    ));
                }
                let points = points
                    .iter()
                    .map(point_input)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(ApiError)?;
                let request = SplineRequest {
                    points,
                    degree,
                    closed,
                    ends: ends.unwrap_or([SpatialSplineEnd::Natural; 2]),
                    construction,
                };
                let (made, report) = edit(self, sketch, "Add Spline", |e| e.add_spline(&request))?;
                let mut value = report_json(&report);
                value["made"] = json!([curve_name(made.spline)]);
                value["spline_points"] = json!(
                    made.points
                        .iter()
                        .map(|p| point_name(*p))
                        .collect::<Vec<_>>()
                );
                value
            }
            Sketch3dCommand::AddConstraint { sketch, constraint } => {
                let (id, report) = edit(self, sketch, "Add Constraint", |e| {
                    add_constraint(e, &constraint)
                })?;
                let mut value = report_json(&report);
                value["made"] = json!([id.to_string()]);
                value
            }
            Sketch3dCommand::AddDimension {
                sketch,
                dimension,
                value,
                driven,
            } => {
                if matches!(dimension, SpatialDimensionKind::InPlane { .. }) {
                    return Err(ApiError(
                        "in-plane dimensions are kept from a converted planar sketch; dimensions \
                         added to a 3D sketch are 3D ones"
                            .to_owned(),
                    ));
                }
                if driven && value.is_some() {
                    return Err(ApiError(
                        "a driven dimension only measures: it takes no \"value\"".to_owned(),
                    ));
                }
                let (id, report) = edit(self, sketch, "Add Dimension", |e| {
                    e.add_dimension(dimension, value.as_ref(), driven)
                })?;
                let mut result = report_json(&report);
                result["made"] = json!([id.to_string()]);
                result
            }
            Sketch3dCommand::SetDimension {
                sketch,
                dimension,
                value,
            } => {
                let id = constraint_id(&dimension).map_err(ApiError)?;
                let ((), report) = edit(self, sketch, "Edit Dimension", |e| {
                    e.set_dimension_value(id, &value)
                })?;
                report_json(&report)
            }
            Sketch3dCommand::SetDriven {
                sketch,
                dimension,
                driven,
            } => {
                let id = constraint_id(&dimension).map_err(ApiError)?;
                let what = if driven {
                    "Make Driven"
                } else {
                    "Make Driving"
                };
                let ((), report) = edit(self, sketch, what, |e| e.set_driven(id, driven))?;
                report_json(&report)
            }
            Sketch3dCommand::SetConstruction {
                sketch,
                curves,
                construction,
            } => {
                let curves = curves
                    .iter()
                    .map(|c| curve_id(c.as_str()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(ApiError)?;
                let ((), report) = edit(self, sketch, "Construction", |e| {
                    for c in &curves {
                        e.set_construction(*c, construction)?;
                    }
                    Ok(())
                })?;
                report_json(&report)
            }
            Sketch3dCommand::Fix {
                sketch,
                points,
                fixed,
            } => {
                let points = point_ids(&points).map_err(ApiError)?;
                let what = if fixed { "Fix" } else { "Unfix" };
                let ((), report) = edit(self, sketch, what, |e| {
                    for p in &points {
                        if fixed {
                            e.fix(*p)?;
                        } else {
                            e.unfix(*p)?;
                        }
                    }
                    Ok(())
                })?;
                report_json(&report)
            }
            Sketch3dCommand::Detach { sketch, points } => {
                let points = point_ids(&points).map_err(ApiError)?;
                let ((), report) = edit(self, sketch, "Detach", |e| {
                    for p in &points {
                        e.detach(*p)?;
                    }
                    Ok(())
                })?;
                report_json(&report)
            }
            Sketch3dCommand::Attach {
                sketch,
                point,
                source,
                projected,
            } => {
                let point = point_id(&point).map_err(ApiError)?;
                let source = source_input(self, sketch, &source)?;
                let ((id, reference), report) = edit(self, sketch, "Attach", |e| {
                    let id = e.attach_as(point, source, projected)?;
                    let reference = e.def.constraint(id).and_then(|c| match c.kind {
                        SpatialConstraintKind::PointOn { curve, .. } => Some(curve),
                        _ => None,
                    });
                    Ok((id, reference))
                })?;
                let mut value = report_json(&report);
                value["made"] = json!([id.to_string()]);
                if let Some(curve) = reference {
                    value["reference"] = json!(curve_name(curve));
                }
                value
            }
            Sketch3dCommand::Include {
                sketch,
                source,
                projected,
            } => {
                let source = source_input(self, sketch, &source)?;
                let what = if projected { "Project" } else { "Include" };
                let ((curve, reused), report) = edit(self, sketch, what, |e| {
                    let before = e.def.curves.len();
                    let curve = e.include(source, projected)?;
                    Ok((curve, e.def.curves.len() == before))
                })?;
                let mut value = report_json(&report);
                value["made"] = json!([curve_name(curve)]);
                value["reused"] = json!(reused);
                value
            }
            Sketch3dCommand::ReplaceReference {
                sketch,
                item,
                source,
            } => {
                let source = source_input(self, sketch, &source)?;
                let ((), report) = edit(self, sketch, "Replace Reference", |e| {
                    if item.starts_with('k') {
                        e.replace_attachment(constraint_id(&item)?, source)
                    } else {
                        e.replace_reference_curve(curve_id(&item)?, source)
                    }
                })?;
                report_json(&report)
            }
            Sketch3dCommand::ReleaseFromPlane { sketch, points } => {
                let points = point_ids(&points).map_err(ApiError)?;
                let (released, report) = edit(self, sketch, "Release from Plane", |e| {
                    e.release_from_plane(&points)
                })?;
                let mut value = report_json(&report);
                value["released"] =
                    json!(released.iter().map(|p| point_name(*p)).collect::<Vec<_>>());
                value
            }
            Sketch3dCommand::ReleasePlane { sketch, curves } => {
                let curves = curves
                    .iter()
                    .map(|c| curve_id(c.as_str()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(ApiError)?;
                let ((), report) = edit(self, sketch, "Release Plane", |e| {
                    for c in &curves {
                        e.release_plane(*c)?;
                    }
                    Ok(())
                })?;
                report_json(&report)
            }
            Sketch3dCommand::Remove {
                sketch,
                items,
                dependents,
            } => {
                let ((), report) = edit(self, sketch, "Delete", |e| remove(e, &items, dependents))?;
                report_json(&report)
            }
            Sketch3dCommand::Drag {
                sketch,
                points,
                tangents,
                preview,
            } => {
                if points.is_empty() && tangents.is_empty() {
                    return Err(ApiError(
                        "a drag needs \"points\" or \"tangents\"".to_owned(),
                    ));
                }
                let goals = points
                    .iter()
                    .map(|g| point_id(&g.point).map(|p| (p, g.to)))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(ApiError)?;
                let pulls = tangents
                    .iter()
                    .map(tangent_pull)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(ApiError)?;
                if preview {
                    let solved = self
                        .preview_spatial_drag_with(sketch, &goals, &pulls)
                        .map_err(refusal)?;
                    let points: serde_json::Map<String, Value> = solved
                        .points
                        .iter()
                        .map(|(p, at)| (point_name(*p), json!(at)))
                        .collect();
                    // Each curve's own geometry where the drag would take
                    // it, as the query gives it (mitcad#1526: the preview
                    // draws it).
                    let curves: serde_json::Map<String, Value> = self
                        .spatial_sketch_def(sketch)
                        .map(|def| {
                            def.curves
                                .iter()
                                .filter_map(|c| {
                                    curve_geometry(&solved, c).map(|g| (curve_name(c.id), g))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    json!({"points": points, "curves": curves,
                           "status": status_json(&solved.status)})
                } else {
                    let report = self
                        .drag_spatial_sketch_with(sketch, &goals, &pulls)
                        .map_err(refusal)?;
                    report_json(&report)
                }
            }
            Sketch3dCommand::Convert { sketch } => {
                let converted = self
                    .convert_sketch_to_spatial(sketch)
                    .map_err(|e| ApiError(e.to_string()))?;
                conversion_json(self, sketch, &converted)
            }
            Sketch3dCommand::ConvertTo2d { sketch } => {
                let converted = self
                    .convert_sketch_to_planar(sketch)
                    .map_err(|e| ApiError(e.to_string()))?;
                planar_conversion_json(self, sketch, &converted)
            }
            Sketch3dCommand::Fillet {
                sketch,
                a,
                b,
                points,
                radius,
            } => {
                let mut corners = Vec::new();
                match (a, b) {
                    (Some(a), Some(b)) => corners.push(FilletCorner::Lines(
                        curve_id(&a).map_err(ApiError)?,
                        curve_id(&b).map_err(ApiError)?,
                    )),
                    (None, None) => {}
                    _ => {
                        return Err(ApiError(
                            "a fillet of two lines names both, \"a\" and \"b\"".to_owned(),
                        ));
                    }
                }
                if !points.is_empty() {
                    for p in point_ids(&points).map_err(ApiError)? {
                        corners.push(FilletCorner::Point(p));
                    }
                }
                if corners.is_empty() {
                    return Err(ApiError(
                        "a fillet needs \"a\" and \"b\" (two lines that share an end) or \
                         \"points\" (corners where two lines meet)"
                            .to_owned(),
                    ));
                }
                let (made, report) =
                    edit(self, sketch, "Fillet", |e| e.fillets(&corners, &radius))?;
                let mut value = report_json(&report);
                value["made"] = json!(made.iter().map(|f| curve_name(f.arc)).collect::<Vec<_>>());
                value["fillets"] = json!(made.iter().map(fillet_json).collect::<Vec<_>>());
                if let Some(k) = made.first().and_then(|f| f.dimension) {
                    value["dimension"] = json!(k.to_string());
                }
                value
            }
            Sketch3dCommand::Paste {
                sketch,
                fragment,
                at,
                plane,
            } => paste(self, sketch, fragment, at, plane.as_deref())?,
        })
    }

    fn sketch3d_json(&self, uid: FeatureUid) -> Result<Value, ApiError> {
        let entry = self
            .feature(uid)
            .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
        let def = match &entry.def {
            FeatureDef::SpatialSketch(def) => def,
            // The name for a person, the uid for agents (mitcad#1644).
            FeatureDef::Sketch(_) => {
                return Err(ApiError(notice::said(
                    format!("{} is a 2D sketch, which this does not take", entry.name),
                    format!(
                        "{} ({uid}) is a 2D sketch: the sketch query reads it",
                        entry.name
                    ),
                )));
            }
            _ => {
                return Err(ApiError(notice::said(
                    format!("{} is not a 3D sketch", entry.name),
                    format!("{} ({uid}) is not a 3D sketch", entry.name),
                )));
            }
        };
        let params = self.parameters();
        let output = self.spatial_sketch(uid);
        let solved = output.and_then(|o| o.solved.as_ref());
        let frame = output.and_then(|o| o.frame.as_ref());
        let status = solved.map(|s| &s.status);
        // The points the On Sketch Plane relation keeps in the plane: its
        // representatives' elevation groups.
        let kept: BTreeSet<EntityUid> = match def.on_sketch_plane().map(|c| &c.kind) {
            Some(SpatialConstraintKind::OnSketchPlane { points }) => {
                let representatives: BTreeSet<EntityUid> = points.iter().copied().collect();
                def.elevation_groups()
                    .into_iter()
                    .filter(|(_, group)| representatives.contains(group))
                    .map(|(point, _)| point)
                    .collect()
            }
            _ => BTreeSet::new(),
        };
        let points: Vec<Value> = def
            .points
            .iter()
            .map(|p| {
                let at = solved
                    .and_then(|s| s.points.get(&p.id))
                    .copied()
                    .unwrap_or(p.at);
                let mut value = json!({"id": point_name(p.id), "at": at});
                if let Some(frame) = frame {
                    value["position"] = json!(to_component(frame, at));
                }
                if let Some(status) = status {
                    value["determined"] = json!(status.determined_points.contains(&p.id));
                }
                if kept.contains(&p.id) {
                    value["on_sketch_plane"] = json!(true);
                }
                value
            })
            .collect();
        let curves: Vec<Value> = def
            .curves
            .iter()
            .map(|c| {
                let mut value = serde_json::to_value(c).expect("serializes");
                if c.is_plane_bound() {
                    value["plane_bound"] = json!(true);
                }
                if let Some(status) = status {
                    value["determined"] = json!(
                        status.determined_lines.contains(&c.id)
                            || status.determined_curves.contains(&c.id)
                    );
                }
                if let Some(geometry) = solved.and_then(|s| curve_geometry(s, c)) {
                    value["geometry"] = geometry;
                } else if let Some(reference) = output.and_then(|o| o.references.get(&c.id)) {
                    // A reference curve as found before the solve, also
                    // when the solve failed (mitcad#1524: drawn with its
                    // source).
                    value["geometry"] = reference_geometry(reference);
                }
                value
            })
            .collect();
        let redundant = |id: &ConstraintUid| status.is_some_and(|s| s.redundant.contains(id));
        let constraints: Vec<Value> = def
            .constraints
            .iter()
            .map(|c| {
                let mut value = serde_json::to_value(c).expect("serializes");
                if c.kind.is_in_plane() {
                    value["in_plane"] = json!(true);
                }
                // A tangency's sense as solved, an attachment's target as
                // found before the solve.
                if let Some(sense) = solved.and_then(|s| s.tangent_senses.get(&c.id)) {
                    value["sense"] = json!(sense);
                }
                if let Some(target) = output.and_then(|o| o.targets.get(&c.id)) {
                    value["target"] = json!(target);
                }
                if redundant(&c.id) {
                    value["redundant"] = json!(true);
                }
                value
            })
            .collect();
        let dimensions: Vec<Value> = def
            .dimensions
            .iter()
            .map(|d| {
                let mut value = serde_json::to_value(
                    d.map_value(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                        .expect("names never fail"),
                )
                .expect("serializes");
                value["driven"] = json!(d.is_driven());
                if matches!(d.kind, SpatialDimensionKind::InPlane { .. }) {
                    value["in_plane"] = json!(true);
                }
                if let Some(param) = d.value.and_then(|id| params.get(id)) {
                    value["expression"] = json!(param.expression());
                }
                if let Some(measured) = status.and_then(|s| s.measured.get(&d.id)) {
                    value["measured"] = json!(measured);
                }
                if redundant(&d.id) {
                    value["redundant"] = json!(true);
                }
                value
            })
            .collect();
        let mut value = json!({
            "uid": uid,
            "name": entry.name,
            "type": <SpatialSketchDef>::TYPE,
            "support": def.support,
            "frame": frame.map(|f| json!({"origin": f.origin, "x_axis": f.x_axis,
                                         "y_axis": f.y_axis, "normal": f.normal()})),
            "points": points,
            "curves": curves,
            "constraints": constraints,
            "dimensions": dimensions,
            "references": references_json(def, output),
            "chains": chains_json(uid, def),
            "evaluated": output.is_some(),
            "status": status.map(status_json),
            "warnings": self.warnings(uid),
        });
        if !def.texts.is_empty() {
            // Each text with its id, its plane and its outline where the
            // solve put its frame (mitcad#1550: the view draws it).
            let at = |p: EntityUid| {
                solved
                    .and_then(|s| s.points.get(&p).copied())
                    .or_else(|| def.position(p))
            };
            let outlines = def.text_outlines(&at, self.kernel());
            let texts: Vec<Value> = def
                .texts
                .iter()
                .map(|t| {
                    let mut text = serde_json::to_value(t).expect("serializes");
                    text["id"] = json!(format!("t{}", t.planar.id.0));
                    // A rotated plane's text (mitcad#1661) names no frame
                    // plane; its frame says where it is.
                    text["plane"] = if t.frame.is_some() {
                        json!("rotated")
                    } else {
                        json!(t.normal.plane_name())
                    };
                    match &outlines {
                        Ok(list) => {
                            if let Some((_, contours)) =
                                list.iter().find(|(id, _)| *id == t.planar.id)
                            {
                                text["outline"] = json!(contours);
                            }
                        }
                        Err(error) => text["outline_error"] = json!(error),
                    }
                    text
                })
                .collect();
            value["texts"] = json!(texts);
        }
        if let Some(failure) = output.and_then(|o| o.failure.as_ref()) {
            value["failure"] = failure_json(def, failure);
        }
        // mitcad#1550: the profile regions its closed loops in the sketch
        // plane bound, whether it all lies in the plane (it can become a
        // 2D sketch: `sketch3d.convert_to_2d`) and what lies out of it, and
        // what it keeps of a converted planar sketch.
        if let Some(planar) = self.sketch_output(uid) {
            value["regions"] = json!(
                planar
                    .regions
                    .iter()
                    .map(|r| r.key.to_string())
                    .collect::<Vec<_>>()
            );
            // Its curves in the sketch plane, which ribs, webs, thin walls
            // and surfaces of curves take (mitcad#1550).
            value["plane_curves"] = json!(
                planar
                    .solved
                    .curves
                    .keys()
                    .filter(|id| def.curve(**id).is_some())
                    .map(|id| id.curve_name())
                    .collect::<Vec<_>>()
            );
        }
        if let Some(solved) = output.filter(|o| o.is_solved()) {
            let flat = planar_view(def, solved);
            value["flat"] = json!(flat.off_plane.is_empty());
            if !flat.off_plane.is_empty() {
                value["off_plane"] = json!(flat.off_plane);
                // The same in plain words for a person (mitcad#1644).
                value["off_plane_messages"] = json!(flat.off_plane_said);
            }
        }
        if !def.planar.is_empty() {
            let planar = def
                .planar
                .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                .expect("names never fail");
            value["planar"] = serde_json::to_value(&planar).expect("serializes");
        }
        Ok(value)
    }

    fn sketch3d_route_json(&self, uid: FeatureUid, path: &RouteQuery) -> Value {
        match self.feature(uid).map(|entry| (&entry.def, &entry.name)) {
            Some((FeatureDef::SpatialSketch(def), name)) => {
                route_json(def, uid, name, path, self.spatial_sketch(uid))
            }
            _ => json!({"error": format!("{uid} is not a 3D sketch")}),
        }
    }
}

/// The chains that chain selection finds in the sketch (mitcad#1523): the
/// curves of each in order, whether it is closed, where it branches, and
/// the path that takes it whole when it is open and does not branch.
fn chains_json(uid: FeatureUid, def: &SpatialSketchDef) -> Vec<Value> {
    spatial_path::chains(def)
        .iter()
        .map(|chain| {
            let curves: Vec<String> = chain.legs.iter().map(|l| curve_name(l.curve)).collect();
            let mut value = json!({"curves": curves, "closed": chain.closed});
            if !chain.branches.is_empty() {
                value["branches"] = json!(names_of_points(&chain.branches));
            }
            if let Some(path) = chain.path(uid) {
                value["path"] = serde_json::to_value(path).expect("serializes");
            }
            value
        })
        .collect()
}

fn names_of_points(points: &[EntityUid]) -> Vec<String> {
    points.iter().map(|p| point_name(*p)).collect()
}

fn point_ids(points: &[String]) -> Result<Vec<EntityUid>, String> {
    if points.is_empty() {
        return Err("no points are given".to_owned());
    }
    points.iter().map(|p| point_id(p.as_str())).collect()
}

/// Removes relations and dimensions (`k`), curves (`c`) and points (`p`),
/// in that order. A point that a curve or relation not listed still uses
/// is refused naming them, unless `dependents`; an item that an earlier
/// one took with it is not looked for again.
fn remove(e: &mut SpatialEdit<'_>, items: &[String], dependents: bool) -> Result<(), String> {
    if items.is_empty() {
        return Err("no items are given".to_owned());
    }
    let mut relations = Vec::new();
    let mut curves = Vec::new();
    let mut points = Vec::new();
    let mut texts = Vec::new();
    for item in items {
        if item.starts_with('k') {
            let id = constraint_id(item)?;
            if e.def.constraint(id).is_none() && e.def.dimension(id).is_none() {
                return Err(notice::said(
                    "the relation or dimension does not exist",
                    format!("{id} does not exist"),
                ));
            }
            relations.push(id);
            continue;
        }
        // A text (mitcad#1550).
        if let Some(number) = item.strip_prefix('t') {
            let id = number
                .parse::<u32>()
                .map(EntityUid)
                .map_err(|_| format!("\"{item}\" is not an id like \"t5\""))?;
            texts.push(id);
            continue;
        }
        match item.parse::<Ref>().map_err(|error| error.to_string())? {
            Ref::Point(id) => {
                e.position(id)?;
                points.push(id);
            }
            Ref::Curve(id) => {
                if e.def.curve(id).is_none() {
                    return Err(notice::said(
                        "the curve does not exist",
                        format!("curve {item} does not exist"),
                    ));
                }

                curves.push(id);
            }
        }
    }
    for id in texts {
        e.delete_text(id)?;
    }
    for id in relations {
        if e.def.constraint(id).is_some() || e.def.dimension(id).is_some() {
            e.remove_relation(id)?;
        }
    }
    for id in curves {
        if e.def.curve(id).is_some() {
            e.delete_curve(id)?;
        }
    }
    for id in points {
        if e.def.point(id).is_some() {
            e.delete_point(id, dependents)?;
        }
    }
    Ok(())
}

/// Whether the 3D sketch `sketch` can take a copied `source` as it is
/// (mitcad#1661): the origin, or geometry of features before it in the
/// timeline, of its own component; another component's, seen through a
/// link, only when the copy's sketch (`copied_from`) is of the same
/// component, as the link starts there.
fn source_fits<K: Kernel>(
    doc: &Document<K>,
    sketch: FeatureUid,
    copied_from: Option<FeatureUid>,
    source: &SpatialSource,
) -> bool {
    let features: Vec<_> = doc.features().map(|f| (f.uid, f.component)).collect();
    let Some(target) = features.iter().position(|(uid, _)| *uid == sketch) else {
        return false;
    };
    let component = features[target].1;
    let linked = source.link().is_some();
    if linked {
        let from = copied_from
            .and_then(|uid| features.iter().find(|(u, _)| *u == uid))
            .map(|(_, c)| *c);
        if from != Some(component) {
            return false;
        }
    }
    source.features().iter().all(|uid| {
        features
            .iter()
            .position(|(u, _)| u == uid)
            .is_some_and(|i| i < target && (linked || features[i].1 == component))
    })
}

/// `sketch3d.paste` (mitcad#1661): a 3D sketch's copy moved by `at - base`,
/// or a planar sketch's copy on the frame plane `plane` through `at`, as
/// one undo step. A source the sketch cannot take ([`source_fits`]) is
/// dropped with what used it; when a source kept does not evaluate here (a
/// copy of another design), the paste is made again without its sources.
fn paste<K: Kernel>(
    doc: &mut Document<K>,
    sketch: FeatureUid,
    fragment: Value,
    at: Option<[f64; 3]>,
    plane: Option<&str>,
) -> Result<Value, ApiError> {
    // What the copy left out, which was said at the copy.
    let (fragment, copy_left_out) = if SpatialFragment::is_spatial(&fragment) {
        if plane.is_some() {
            return Err(ApiError(
                "a 3D sketch's copy keeps its own planes: \"plane\" is for a 2D sketch's copy"
                    .to_owned(),
            ));
        }
        let read: Result<SpatialFragment, _> = serde_json::from_value(fragment);
        let spatial = read.map_err(|e| ApiError(format!("fragment: {e}")))?;
        let left_out = spatial.left_out;
        (spatial, left_out)
    } else {
        let read: Result<Fragment, _> = serde_json::from_value(fragment);
        let planar = read.map_err(|e| ApiError(format!("fragment: {e}")))?;
        let normal = match plane {
            Some(name) => plane_axis(name).map_err(ApiError)?,
            None => SpatialAxis::Z,
        };
        let on_plane = from_planar(&planar, normal, at.unwrap_or([0.0; 3]));
        (on_plane.map_err(ApiError)?, planar.left_out)
    };
    let base = fragment.base;
    let to = at.unwrap_or(base);
    let by = [to[0] - base[0], to[1] - base[1], to[2] - base[2]];
    // The sources this sketch takes, decided before the edit.
    let fits: Vec<SpatialSource> = fragment
        .sources()
        .into_iter()
        .filter(|s| source_fits(doc, sketch, fragment.sketch, s))
        .cloned()
        .collect();
    let attempt = edit(doc, sketch, "Paste", |e| {
        e.paste(&fragment, by, &|s: &SpatialSource| fits.contains(s))
    });
    let (pasted, report) = match attempt {
        Ok(done) => done,
        Err(error) => {
            if fits.is_empty() {
                return Err(error);
            }
            let again = edit(doc, sketch, "Paste", |e| {
                e.paste(&fragment, by, &|_: &SpatialSource| false)
            });
            again.map_err(|_| error)?
        }
    };
    let mut value = report_json(&report);
    value["made"] = json!(pasted.made);
    value["kept"] = json!(pasted.kept);
    value["left_out"] = json!(pasted.left_out);
    value["dropped"] = json!(pasted.dropped);
    // In words, with no ids (mitcad#1327): what the copy left out was said
    // then, so only what this paste (or the planar copy's move onto a
    // plane) left out.
    let mut warnings: Vec<String> = Vec::new();
    let left = pasted.left_out.saturating_sub(copy_left_out);
    if left > 0 {
        warnings.push(format!(
            "{left} relation(s) and dimension(s) left out: they name geometry the copied \
             geometry lacks"
        ));
    }
    if pasted.dropped > 0 {
        warnings.push(format!(
            "{} link(s) to geometry outside the copy left out: this sketch cannot use their \
             sources",
            pasted.dropped
        ));
    }
    if !pasted.values.is_empty() {
        // Counted for a person, the ids for agents (mitcad#1612).
        let counted = match pasted.values.len() {
            1 => "a dimension took its value".to_owned(),
            n => format!("{n} dimensions took their values"),
        };
        warnings.push(
            notice::FeatureError::new(format!(
                "{counted}: the parameters their expressions use are not in this design"
            ))
            .with_detail(format!(
                "{} took their values: the parameters their expressions use are not in this \
                 design",
                pasted.values.join(", ")
            ))
            .text(),
        );
        value["values"] = json!(pasted.values);
    }
    if !warnings.is_empty() {
        value["warnings"] = json!(warnings);
    }
    Ok(value)
}
