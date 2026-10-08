// SPDX-License-Identifier: MIT
//! Sketch commands (`sketch.*`) and the `sketch` query; see `commands.md`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, parse};
use crate::document::Document;
use crate::features::sketch::{FrameDef, SketchDef, SketchOutput};
use crate::features::{FeatureDef, SketchPlane, ValueInput};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::sketch::edit::{EditReport, PointInput};
use crate::sketch::geometry::Curve2;
use crate::sketch::modify::{DragTarget, Item, Map2, point_input};
use crate::sketch::offset::derived_entities;
use crate::sketch::pattern::PatternChange;
use crate::sketch::solve::{SketchStatus, SolverItem};
use crate::sketch::tools::{ArcMode, CircleMode, RectangleMode, SlotMode};
use crate::sketch::{
    ConstraintKind, ConstraintUid, DimensionKind, EntityIndex, EntityKind, Ref, SketchText,
    TextAlign, TextPath, TextVAlign,
};
use crate::topo::TopoName;

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", deny_unknown_fields)]
pub(super) enum SketchCommand {
    #[serde(rename = "sketch.create")]
    Create {
        #[serde(default)]
        plane: SketchPlane,
        #[serde(default)]
        frame: Option<FrameDef>,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "sketch.add_rectangle")]
    AddRectangle {
        sketch: FeatureUid,
        corner: [f64; 2],
        width: ValueInput,
        height: ValueInput,
    },
    #[serde(rename = "sketch.add_circle")]
    AddCircle {
        sketch: FeatureUid,
        center: [f64; 2],
        diameter: ValueInput,
    },
    #[serde(rename = "sketch.add_point")]
    AddPoint {
        sketch: FeatureUid,
        at: [f64; 2],
        #[serde(default)]
        fixed: bool,
    },
    #[serde(rename = "sketch.add_line")]
    AddLine {
        sketch: FeatureUid,
        start: Value,
        end: Value,
        #[serde(default)]
        construction: bool,
        #[serde(default)]
        centerline: bool,
    },
    #[serde(rename = "sketch.rectangle")]
    Rectangle {
        sketch: FeatureUid,
        mode: String,
        #[serde(default)]
        a: Option<[f64; 2]>,
        #[serde(default)]
        b: Option<[f64; 2]>,
        #[serde(default)]
        c: Option<[f64; 2]>,
        #[serde(default)]
        center: Option<[f64; 2]>,
        #[serde(default)]
        corner: Option<[f64; 2]>,
        #[serde(default)]
        construction: bool,
    },
    #[serde(rename = "sketch.circle")]
    Circle {
        sketch: FeatureUid,
        mode: String,
        #[serde(default)]
        center: Option<Value>,
        #[serde(default)]
        radius: Option<f64>,
        #[serde(default)]
        a: Option<[f64; 2]>,
        #[serde(default)]
        b: Option<[f64; 2]>,
        #[serde(default)]
        c: Option<[f64; 2]>,
        #[serde(default, with = "curves_opt")]
        lines: Option<Vec<EntityUid>>,
        #[serde(default)]
        near: Option<[f64; 2]>,
        #[serde(default)]
        construction: bool,
    },
    #[serde(rename = "sketch.arc")]
    Arc {
        sketch: FeatureUid,
        mode: String,
        #[serde(default)]
        start: Option<Value>,
        #[serde(default)]
        through: Option<[f64; 2]>,
        #[serde(default)]
        end: Option<Value>,
        #[serde(default)]
        center: Option<Value>,
        #[serde(default)]
        clockwise: bool,
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        construction: bool,
    },
    #[serde(rename = "sketch.polygon")]
    Polygon {
        sketch: FeatureUid,
        center: [f64; 2],
        vertex: [f64; 2],
        sides: usize,
        #[serde(default = "yes")]
        inscribed: bool,
        #[serde(default)]
        construction: bool,
    },
    #[serde(rename = "sketch.slot")]
    Slot {
        sketch: FeatureUid,
        mode: String,
        width: f64,
        #[serde(default)]
        a: Option<[f64; 2]>,
        #[serde(default)]
        b: Option<[f64; 2]>,
        #[serde(default)]
        center: Option<[f64; 2]>,
        #[serde(default)]
        start: Option<[f64; 2]>,
        #[serde(default)]
        end: Option<[f64; 2]>,
    },
    #[serde(rename = "sketch.ellipse")]
    Ellipse {
        sketch: FeatureUid,
        center: Value,
        major: [f64; 2],
        minor_radius: f64,
        #[serde(default)]
        start: Option<Value>,
        #[serde(default)]
        end: Option<Value>,
        #[serde(default)]
        construction: bool,
    },
    #[serde(rename = "sketch.spline")]
    Spline {
        sketch: FeatureUid,
        points: Vec<Value>,
        #[serde(default)]
        degree: Option<u32>,
        #[serde(default)]
        construction: bool,
    },
    #[serde(rename = "sketch.add_text")]
    AddText {
        sketch: FeatureUid,
        text: String,
        #[serde(default)]
        at: Option<[f64; 2]>,
        height: f64,
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
        #[serde(default)]
        spacing: f64,
        #[serde(default)]
        frame: Option<Value>,
        #[serde(default)]
        path: Option<TextPath>,
        #[serde(default)]
        flip_x: bool,
        #[serde(default)]
        flip_y: bool,
    },
    // Sketch text edited afterwards (P3).
    #[serde(rename = "sketch.edit_text")]
    EditText {
        sketch: FeatureUid,
        id: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        at: Option<[f64; 2]>,
        #[serde(default)]
        height: Option<f64>,
        #[serde(default)]
        angle: Option<f64>,
        #[serde(default)]
        font: Option<String>,
        #[serde(default)]
        bold: Option<bool>,
        #[serde(default)]
        italic: Option<bool>,
        #[serde(default)]
        align: Option<TextAlign>,
        #[serde(default)]
        valign: Option<TextVAlign>,
        #[serde(default)]
        spacing: Option<f64>,
        #[serde(default)]
        path: Option<TextPath>,
        #[serde(default)]
        flip_x: Option<bool>,
        #[serde(default)]
        flip_y: Option<bool>,
    },
    #[serde(rename = "sketch.add_constraint")]
    AddConstraint {
        sketch: FeatureUid,
        constraint: ConstraintKind,
    },
    #[serde(rename = "sketch.add_dimension")]
    AddDimension {
        sketch: FeatureUid,
        dimension: DimensionKind,
        #[serde(default)]
        value: Option<ValueInput>,
        #[serde(default)]
        driven: bool,
        #[serde(default)]
        text: Option<[f64; 2]>,
    },
    #[serde(rename = "sketch.set_dimension")]
    SetDimension {
        sketch: FeatureUid,
        dimension: ConstraintUid,
        value: ValueInput,
    },
    #[serde(rename = "sketch.set_driven")]
    SetDriven {
        sketch: FeatureUid,
        dimension: ConstraintUid,
        #[serde(default = "yes")]
        driven: bool,
    },
    #[serde(rename = "sketch.remove")]
    Remove {
        sketch: FeatureUid,
        items: Vec<String>,
    },
    #[serde(rename = "sketch.set_construction")]
    SetConstruction {
        sketch: FeatureUid,
        #[serde(with = "curves")]
        curves: Vec<EntityUid>,
        #[serde(default = "yes")]
        construction: bool,
    },
    #[serde(rename = "sketch.set_centerline")]
    SetCenterline {
        sketch: FeatureUid,
        #[serde(with = "curves")]
        lines: Vec<EntityUid>,
        #[serde(default = "yes")]
        centerline: bool,
    },
    #[serde(rename = "sketch.set_fixed")]
    SetFixed {
        sketch: FeatureUid,
        entities: Vec<Ref>,
        #[serde(default = "yes")]
        fixed: bool,
    },
    #[serde(rename = "sketch.drag")]
    Drag {
        sketch: FeatureUid,
        entity: Ref,
        #[serde(default)]
        to: Option<[f64; 2]>,
        #[serde(default)]
        by: Option<[f64; 2]>,
        #[serde(default)]
        radius: Option<f64>,
    },
    #[serde(rename = "sketch.move")]
    Move {
        sketch: FeatureUid,
        entities: Vec<Ref>,
        #[serde(default)]
        by: Option<[f64; 2]>,
        #[serde(default)]
        rotate: Option<Rotation>,
        #[serde(default)]
        copy: bool,
    },
    #[serde(rename = "sketch.trim")]
    Trim {
        sketch: FeatureUid,
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
        at: [f64; 2],
    },
    #[serde(rename = "sketch.extend")]
    Extend {
        sketch: FeatureUid,
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
        at: [f64; 2],
    },
    #[serde(rename = "sketch.fillet")]
    Fillet {
        sketch: FeatureUid,
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
        radius: ValueInput,
    },
    #[serde(rename = "sketch.chamfer")]
    Chamfer {
        sketch: FeatureUid,
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
        distance: ValueInput,
        #[serde(default)]
        distance2: Option<ValueInput>,
        #[serde(default)]
        angle: Option<ValueInput>,
    },
    #[serde(rename = "sketch.offset")]
    Offset {
        sketch: FeatureUid,
        #[serde(with = "curves")]
        curves: Vec<EntityUid>,
        distance: ValueInput,
    },
    #[serde(rename = "sketch.mirror")]
    Mirror {
        sketch: FeatureUid,
        entities: Vec<Ref>,
        #[serde(with = "crate::ids::curve_serde")]
        axis: EntityUid,
    },
    #[serde(rename = "sketch.circular_pattern")]
    CircularPattern {
        sketch: FeatureUid,
        entities: Vec<Ref>,
        center: Value,
        count: u32,
        #[serde(default = "full_turn")]
        angle: ValueInput,
    },
    #[serde(rename = "sketch.rectangular_pattern")]
    RectangularPattern {
        sketch: FeatureUid,
        entities: Vec<Ref>,
        #[serde(default = "x_axis")]
        direction: [f64; 2],
        count: [u32; 2],
        spacing: [ValueInput; 2],
    },
    // Sketch patterns and offsets edited afterwards (P4).
    #[serde(rename = "sketch.edit_pattern")]
    EditPattern {
        sketch: FeatureUid,
        pattern: ConstraintUid,
        #[serde(default)]
        count: Option<Value>,
        #[serde(default)]
        angle: Option<ValueInput>,
        #[serde(default)]
        spacing: Option<[ValueInput; 2]>,
        #[serde(default)]
        direction: Option<[f64; 2]>,
        #[serde(default)]
        center: Option<Value>,
        #[serde(default)]
        entities: Option<Vec<Ref>>,
    },
    #[serde(rename = "sketch.edit_offset")]
    EditOffset {
        sketch: FeatureUid,
        offset: ConstraintUid,
        #[serde(default)]
        distance: Option<ValueInput>,
        #[serde(default)]
        flip: bool,
    },
    #[serde(rename = "sketch.project")]
    Project {
        sketch: FeatureUid,
        source: TopoName,
        #[serde(default)]
        body: Option<BodyUid>,
        #[serde(default)]
        linked: bool,
    },
    // Sketch mode (U2): a dimension's value dragged to another place.
    #[serde(rename = "sketch.set_dimension_text")]
    SetDimensionText {
        sketch: FeatureUid,
        dimension: ConstraintUid,
        text: [f64; 2],
    },
    // Files (U6): a DXF drawing's geometry into the sketch.
    #[serde(rename = "sketch.import_dxf")]
    ImportDxf {
        sketch: FeatureUid,
        path: String,
        /// The unit of a drawing that names none (`mm` by default).
        #[serde(default)]
        unit: Option<String>,
        /// Where the drawing's origin goes, in sketch coordinates.
        #[serde(default)]
        at: Option<[f64; 2]>,
        /// Only the entities on these layers (P9); all when left out.
        #[serde(default)]
        layers: Option<Vec<String>>,
    },
}

/// The `dxf_info` query (P9): what the Insert DXF dialog offers, its
/// unit, its layers and where its entities lie.
pub(super) fn dxf_info_json(path: &str) -> Result<Value, ApiError> {
    let drawing = mitcad_dxf::read_file(path).map_err(|e| ApiError(format!("{path}: {e}")))?;
    let unit = match drawing.units {
        mitcad_dxf::Units::Unitless => Value::Null,
        units => json!(dxf_unit_name(units)),
    };
    let bounds_json = |bounds: Option<(mitcad_dxf::Point2, mitcad_dxf::Point2)>| {
        bounds.map(|(min, max)| json!({"min": [min.x, min.y], "max": [max.x, max.y]}))
    };
    let layers: Vec<Value> = drawing
        .used_layers()
        .into_iter()
        .map(|(name, entities)| {
            let layer = drawing.layer(&name);
            let bounds = bounds_json(drawing.on_layers(std::slice::from_ref(&name)).bounds());
            json!({"name": name, "entities": entities,
                   "visible": layer.is_none_or(|l| l.visible && !l.frozen),
                   "bounds": bounds})
        })
        .collect();
    let mut value = json!({
        "unit": unit,
        "unit_mm": drawing.units.millimeters(),
        "entities": drawing.entities.len(),
        "layers": layers,
    });
    if let Some(bounds) = bounds_json(drawing.bounds()) {
        value["bounds"] = bounds;
    }
    Ok(value)
}

/// The short name of a drawing unit: `mm`, `in`, ..., or its `$INSUNITS`
/// code for the rarer ones.
fn dxf_unit_name(units: mitcad_dxf::Units) -> String {
    use mitcad_dxf::Units;
    match units {
        Units::Millimeters => "mm".to_owned(),
        Units::Centimeters => "cm".to_owned(),
        Units::Meters => "m".to_owned(),
        Units::Inches => "in".to_owned(),
        Units::Feet => "ft".to_owned(),
        Units::Microns => "um".to_owned(),
        other => format!("$INSUNITS {}", other.code()),
    }
}

/// A unit for a DXF drawing that names none.
fn dxf_unit(unit: Option<&str>) -> Result<mitcad_dxf::Units, ApiError> {
    use mitcad_dxf::Units;
    Ok(match unit.unwrap_or("mm") {
        "mm" => Units::Millimeters,
        "cm" => Units::Centimeters,
        "m" => Units::Meters,
        "in" => Units::Inches,
        "ft" => Units::Feet,
        other => {
            return Err(ApiError(format!(
                "unknown drawing unit '{other}' (mm, cm, m, in, ft)"
            )));
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Rotation {
    center: Value,
    angle: f64,
}

fn yes() -> bool {
    true
}

fn full_turn() -> ValueInput {
    ValueInput::Number(std::f64::consts::TAU)
}

/// A pattern count: `n` or `[n1, n2]`.
fn pattern_count(value: &Value) -> Result<[u32; 2], String> {
    let count = |v: &Value| {
        v.as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| "a count is a whole number, or [n1, n2]".to_owned())
    };
    match value {
        Value::Array(items) if items.len() == 2 => Ok([count(&items[0])?, count(&items[1])?]),
        other => Ok([count(other)?, 1]),
    }
}

fn x_axis() -> [f64; 2] {
    [1.0, 0.0]
}

/// Lists of curve ids: `["c1", "c2"]`.
mod curves {
    use serde::Deserialize;

    use crate::ids::EntityUid;

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EntityUid>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|n| EntityUid::parse_curve(n).map_err(serde::de::Error::custom))
            .collect()
    }
}

mod curves_opt {
    use crate::ids::EntityUid;

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Vec<EntityUid>>, D::Error> {
        super::curves::deserialize(d).map(Some)
    }
}

fn point(value: &Value) -> Result<PointInput, String> {
    point_input(value)
}

/// The copies a pattern made of its listed entities, instance by instance.
fn pattern_made(def: &SketchDef, id: ConstraintUid) -> Vec<String> {
    def.patterns
        .iter()
        .find(|p| p.id == id)
        .map(|p| {
            p.copies
                .iter()
                .flat_map(|c| {
                    p.entities
                        .iter()
                        .filter_map(|r| c.entities.get(r).map(ToString::to_string))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn need<T>(value: Option<T>, field: &str, mode: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("\"{mode}\" needs \"{field}\""))
}

fn names<T: ToString>(items: &[T]) -> Value {
    json!(items.iter().map(ToString::to_string).collect::<Vec<_>>())
}

fn item(item: &SolverItem) -> String {
    match item {
        SolverItem::Constraint(id) => id.to_string(),
        SolverItem::Fixed(entity) => entity.to_string(),
    }
}

pub(super) fn status_json(status: &SketchStatus, def: &SketchDef) -> Value {
    let refs: Vec<String> = status
        .fully_constrained
        .iter()
        .filter_map(|uid| def.entity(*uid).map(|e| e.as_ref().to_string()))
        .collect();
    json!({
        "dof": status.dof,
        "fully_constrained": refs,
        "redundant": status.redundant.iter().map(item).collect::<Vec<_>>(),
        "conflicts": status.conflicts.iter()
            .map(|set| set.iter().map(item).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
    })
}

/// The result of an edit: what it made and the sketch's status.
pub(super) fn report_json(report: &EditReport, def: Option<&SketchDef>) -> Value {
    let texts: Vec<String> = report.texts.iter().map(|t| format!("t{}", t.0)).collect();
    let mut value = json!({
        "entities": names(&report.entities),
        "constraints": names(&report.constraints),
        "dimensions": names(&report.dimensions),
        "parameters": report.parameters,
    });
    if !texts.is_empty() {
        value["texts"] = json!(texts);
    }
    if !report.removed.is_empty() {
        value["removed"] = json!(report.removed);
    }
    if let Some(def) = def {
        value["status"] = status_json(&report.status, def);
    }
    value
}

fn curve_json(curve: &Curve2) -> Value {
    match curve {
        Curve2::Line { a, b } => json!({"start": a, "end": b}),
        Curve2::Circle { center, radius } => json!({"center": center, "radius": radius}),
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        } => {
            let (a, b) = curve.ends().expect("an arc has ends");
            json!({"center": center, "radius": radius, "start_angle": start, "end_angle": end,
                   "start": a, "end": b})
        }
        Curve2::Ellipse {
            center,
            major,
            minor,
            rotation,
        } => json!({"center": center, "major_radius": major, "minor_radius": minor,
                    "rotation": rotation}),
        Curve2::EllipticalArc {
            center,
            major,
            minor,
            rotation,
            start,
            end,
        } => {
            let (a, b) = curve.ends().expect("an arc has ends");
            json!({"center": center, "major_radius": major, "minor_radius": minor,
                   "rotation": rotation, "start_angle": start, "end_angle": end,
                   "start": a, "end": b})
        }
        Curve2::Nurbs(n) => json!({"degree": n.degree, "control": n.control,
                                   "weights": n.weights, "knots": n.knots}),
    }
}

impl<K: Kernel> Document<K> {
    pub(super) fn run_sketch_command(&mut self, command: SketchCommand) -> Result<Value, ApiError> {
        Ok(match command {
            SketchCommand::Create { plane, frame, name } => {
                let def = FeatureDef::Sketch(SketchDef {
                    frame: frame.map(Box::new),
                    ..SketchDef::new(plane)
                });
                let added = self.add_feature(&def, name.as_deref())?;
                json!({"uid": added.uid, "name": added.name})
            }
            SketchCommand::AddRectangle {
                sketch,
                corner,
                width,
                height,
            } => {
                let added = self.add_rectangle(sketch, corner, &width, &height)?;
                shape_added(&added, self.sketch_def(sketch))
            }
            SketchCommand::AddCircle {
                sketch,
                center,
                diameter,
            } => {
                let added = self.add_circle(sketch, center, &diameter)?;
                shape_added(&added, self.sketch_def(sketch))
            }
            SketchCommand::Project {
                sketch,
                source,
                body,
                linked,
            } => {
                let report = self.project_into_sketch(sketch, &source, body, linked)?;
                report_json(&report, self.sketch_def(sketch))
            }
            SketchCommand::ImportDxf {
                sketch,
                path,
                unit,
                at,
                layers,
            } => {
                let unitless = dxf_unit(unit.as_deref())?;
                let mut drawing = mitcad_dxf::read_file(&path)
                    .map_err(|e| ApiError(format!("{path}: {e}")))?
                    .to_millimeters(unitless);
                if let Some(layers) = layers {
                    let used = drawing.used_layers();
                    if let Some(missing) =
                        layers.iter().find(|l| !used.iter().any(|(n, _)| n == *l))
                    {
                        return Err(ApiError(format!("{path} has nothing on a layer {missing}")));
                    }
                    drawing = drawing.on_layers(&layers);
                }
                let (inserted, report) =
                    self.edit_sketch(sketch, "Insert DXF into {sketch}", |edit| {
                        crate::sketch::dxf::insert_drawing(edit, &drawing, at.unwrap_or_default())
                    })?;
                let mut value = report_json(&report, self.sketch_def(sketch));
                let mut warnings = drawing.warnings.clone();
                warnings.extend(inserted.skipped);
                value["curves"] = json!(inserted.curves);
                value["points"] = json!(inserted.points);
                value["text_count"] = json!(inserted.texts);
                value["warnings"] = json!(warnings);
                value
            }
            command => self.edit_command(command)?,
        })
    }

    /// The commands that edit a sketch through [`SketchEdit`].
    fn edit_command(&mut self, command: SketchCommand) -> Result<Value, ApiError> {
        use SketchCommand as C;
        let mut made: Vec<String> = Vec::new();
        let made_ref = &mut made;
        let (sketch, label): (FeatureUid, &str) = match &command {
            C::AddPoint { sketch, .. } => (*sketch, "Add Point to {sketch}"),
            C::AddLine { sketch, .. } => (*sketch, "Add Line to {sketch}"),
            C::Rectangle { sketch, .. } => (*sketch, "Add Rectangle to {sketch}"),
            C::Circle { sketch, .. } => (*sketch, "Add Circle to {sketch}"),
            C::Arc { sketch, .. } => (*sketch, "Add Arc to {sketch}"),
            C::Polygon { sketch, .. } => (*sketch, "Add Polygon to {sketch}"),
            C::Slot { sketch, .. } => (*sketch, "Add Slot to {sketch}"),
            C::Ellipse { sketch, .. } => (*sketch, "Add Ellipse to {sketch}"),
            C::Spline { sketch, .. } => (*sketch, "Add Spline to {sketch}"),
            C::AddText { sketch, .. } => (*sketch, "Add Text to {sketch}"),
            C::EditText { sketch, .. } => (*sketch, "Edit Text in {sketch}"),
            C::AddConstraint { sketch, .. } => (*sketch, "Add Constraint to {sketch}"),
            C::AddDimension { sketch, .. } => (*sketch, "Add Dimension to {sketch}"),
            C::SetDimension { sketch, .. } => (*sketch, "Change Dimension in {sketch}"),
            C::SetDriven { sketch, .. } => (*sketch, "Change Dimension in {sketch}"),
            C::Remove { sketch, .. } => (*sketch, "Delete in {sketch}"),
            C::SetConstruction { sketch, .. } => (*sketch, "Construction in {sketch}"),
            C::SetCenterline { sketch, .. } => (*sketch, "Centerline in {sketch}"),
            C::SetFixed { sketch, .. } => (*sketch, "Fix in {sketch}"),
            C::Drag { sketch, .. } => (*sketch, "Drag in {sketch}"),
            C::Move { sketch, copy, .. } => (
                *sketch,
                if *copy {
                    "Copy in {sketch}"
                } else {
                    "Move in {sketch}"
                },
            ),
            C::Trim { sketch, .. } => (*sketch, "Trim in {sketch}"),
            C::Extend { sketch, .. } => (*sketch, "Extend in {sketch}"),
            C::Fillet { sketch, .. } => (*sketch, "Fillet in {sketch}"),
            C::Chamfer { sketch, .. } => (*sketch, "Chamfer in {sketch}"),
            C::Offset { sketch, .. } => (*sketch, "Offset in {sketch}"),
            C::Mirror { sketch, .. } => (*sketch, "Mirror in {sketch}"),
            C::CircularPattern { sketch, .. } => (*sketch, "Circular Pattern in {sketch}"),
            C::RectangularPattern { sketch, .. } => (*sketch, "Rectangular Pattern in {sketch}"),
            C::EditPattern { sketch, .. } => (*sketch, "Edit Pattern in {sketch}"),
            C::EditOffset { sketch, .. } => (*sketch, "Edit Offset in {sketch}"),
            C::SetDimensionText { sketch, .. } => (*sketch, "Move Dimension in {sketch}"),
            C::Create { .. }
            | C::AddRectangle { .. }
            | C::AddCircle { .. }
            | C::Project { .. }
            | C::ImportDxf { .. } => {
                unreachable!("handled above")
            }
        };
        let curve_list = |ids: Vec<EntityUid>| -> Vec<String> {
            ids.into_iter().map(|c| Ref::Curve(c).to_string()).collect()
        };
        let ((), report) = self.edit_sketch(sketch, label, |edit| {
            match command {
                C::AddPoint { at, fixed, .. } => {
                    let p = edit.point(PointInput::At(at))?;
                    edit.def.entity_mut(p).expect("added").fixed = fixed;
                }
                C::AddLine {
                    start,
                    end,
                    construction,
                    centerline,
                    ..
                } => {
                    let l =
                        edit.add_line(point(&start)?, point(&end)?, construction, centerline)?;
                    made_ref.push(Ref::Curve(l).to_string());
                }
                C::Rectangle {
                    mode,
                    a,
                    b,
                    c,
                    center,
                    corner,
                    construction,
                    ..
                } => {
                    let m = mode.as_str();
                    let mode = match m {
                        "two_point" => RectangleMode::TwoPoint {
                            a: need(a, "a", m)?,
                            b: need(b, "b", m)?,
                        },
                        "three_point" => RectangleMode::ThreePoint {
                            a: need(a, "a", m)?,
                            b: need(b, "b", m)?,
                            c: need(c, "c", m)?,
                        },
                        "center" => RectangleMode::Center {
                            center: need(center, "center", m)?,
                            corner: need(corner, "corner", m)?,
                        },
                        other => {
                            return Err(format!(
                                "unknown rectangle mode '{other}' (two_point, three_point, center)"
                            ));
                        }
                    };
                    *made_ref = curve_list(edit.rectangle(mode, construction)?);
                }
                C::Circle {
                    mode,
                    center,
                    radius,
                    a,
                    b,
                    c,
                    lines,
                    near,
                    construction,
                    ..
                } => {
                    let m = mode.as_str();
                    let lines = lines.unwrap_or_default();
                    let line = |i: usize| {
                        lines
                            .get(i)
                            .copied()
                            .ok_or_else(|| format!("\"{m}\" needs {} \"lines\"", i + 1))
                    };
                    let mode = match m {
                        "center" => CircleMode::CenterRadius {
                            center: point(&need(center, "center", m)?)?,
                            radius: need(radius, "radius", m)?,
                        },
                        "two_point" => CircleMode::TwoPoint {
                            a: need(a, "a", m)?,
                            b: need(b, "b", m)?,
                        },
                        "three_point" => CircleMode::ThreePoint {
                            a: need(a, "a", m)?,
                            b: need(b, "b", m)?,
                            c: need(c, "c", m)?,
                        },
                        "two_tangent" => CircleMode::TwoTangent {
                            a: line(0)?,
                            b: line(1)?,
                            radius: need(radius, "radius", m)?,
                            near: need(near, "near", m)?,
                        },
                        "three_tangent" => CircleMode::ThreeTangent {
                            a: line(0)?,
                            b: line(1)?,
                            c: line(2)?,
                        },
                        other => {
                            return Err(format!(
                                "unknown circle mode '{other}' (center, two_point, three_point, \
                                 two_tangent, three_tangent)"
                            ));
                        }
                    };
                    let id = edit.circle(mode, construction)?;
                    made_ref.push(Ref::Curve(id).to_string());
                }
                C::Arc {
                    mode,
                    start,
                    through,
                    end,
                    center,
                    clockwise,
                    from,
                    construction,
                    ..
                } => {
                    let m = mode.as_str();
                    let mode = match m {
                        "three_point" => ArcMode::ThreePoint {
                            start: point(&need(start, "start", m)?)?,
                            through: need(through, "through", m)?,
                            end: point(&need(end, "end", m)?)?,
                        },
                        "center" => {
                            let end = need(end, "end", m)?;
                            let end: [f64; 2] = serde_json::from_value(end)
                                .map_err(|_| "\"end\" is a direction point [x, y]".to_owned())?;
                            ArcMode::Center {
                                center: point(&need(center, "center", m)?)?,
                                start: point(&need(start, "start", m)?)?,
                                end,
                                clockwise,
                            }
                        }
                        "tangent" => ArcMode::Tangent {
                            from: crate::sketch::point_serde::parse(&need(from, "from", m)?)
                                .map_err(|e| e.to_string())?,
                            end: point(&need(end, "end", m)?)?,
                        },
                        other => {
                            return Err(format!(
                                "unknown arc mode '{other}' (three_point, center, tangent)"
                            ));
                        }
                    };
                    let id = edit.arc(mode, construction)?;
                    made_ref.push(Ref::Curve(id).to_string());
                }
                C::Polygon {
                    center,
                    vertex,
                    sides,
                    inscribed,
                    construction,
                    ..
                } => {
                    *made_ref =
                        curve_list(edit.polygon(center, vertex, sides, inscribed, construction)?);
                }
                C::Slot {
                    mode,
                    width,
                    a,
                    b,
                    center,
                    start,
                    end,
                    ..
                } => {
                    let m = mode.as_str();
                    let mode = match m {
                        "center_to_center" => SlotMode::CenterToCenter {
                            a: need(a, "a", m)?,
                            b: need(b, "b", m)?,
                        },
                        "center_point" => SlotMode::CenterPoint {
                            center: need(center, "center", m)?,
                            end: need(end, "end", m)?,
                        },
                        "arc" => SlotMode::Arc {
                            center: need(center, "center", m)?,
                            start: need(start, "start", m)?,
                            end: need(end, "end", m)?,
                        },
                        other => {
                            return Err(format!(
                                "unknown slot mode '{other}' (center_to_center, center_point, arc)"
                            ));
                        }
                    };
                    *made_ref = curve_list(edit.slot(mode, width)?);
                }
                C::Ellipse {
                    center,
                    major,
                    minor_radius,
                    start,
                    end,
                    construction,
                    ..
                } => {
                    let arc = match (start, end) {
                        (Some(s), Some(e)) => Some((point(&s)?, point(&e)?)),
                        (None, None) => None,
                        _ => return Err("an elliptical arc needs \"start\" and \"end\"".to_owned()),
                    };
                    let id =
                        edit.ellipse(point(&center)?, major, minor_radius, arc, construction)?;
                    made_ref.push(Ref::Curve(id).to_string());
                }
                C::Spline {
                    points,
                    degree,
                    construction,
                    ..
                } => {
                    let points = points.iter().map(point).collect::<Result<Vec<_>, _>>()?;
                    let id = edit.spline(&points, degree, construction)?;
                    made_ref.push(Ref::Curve(id).to_string());
                }
                C::AddText {
                    text,
                    at,
                    height,
                    angle,
                    font,
                    bold,
                    italic,
                    align,
                    valign,
                    spacing,
                    frame,
                    path,
                    flip_x,
                    flip_y,
                    ..
                } => {
                    let mut t = SketchText {
                        angle,
                        font,
                        bold,
                        italic,
                        align,
                        valign,
                        spacing,
                        path,
                        flip_x,
                        flip_y,
                        ..SketchText::new(EntityUid(0), &text, at.unwrap_or_default(), height)
                    };
                    match (frame, at, path) {
                        (Some(frame), _, _) => {
                            t.frame = edit.text_frame(&frame, angle)?;
                            if let Some((corner, _)) = edit.frame_corner(&t.frame) {
                                t.at = corner;
                            }
                        }
                        (None, None, None) => {
                            return Err("a text needs \"at\", a \"frame\" or a \"path\"".to_owned());
                        }
                        _ => {}
                    }
                    let id = edit.text(t)?;
                    made_ref.push(format!("t{}", id.0));
                }
                C::EditText {
                    id,
                    text,
                    at,
                    height,
                    angle,
                    font,
                    bold,
                    italic,
                    align,
                    valign,
                    spacing,
                    path,
                    flip_x,
                    flip_y,
                    ..
                } => {
                    let uid = crate::ids::parse_number(&id, "t")
                        .map(EntityUid)
                        .ok_or_else(|| format!("'{id}' is not a text like \"t5\""))?;
                    let t = edit
                        .def
                        .texts
                        .iter_mut()
                        .find(|t| t.id == uid)
                        .ok_or_else(|| format!("text {id} does not exist"))?;
                    if let Some(v) = text {
                        t.text = v;
                    }
                    if let Some(v) = at {
                        t.at = v;
                    }
                    if let Some(v) = height {
                        t.height = v;
                    }
                    if let Some(v) = angle {
                        t.angle = v;
                    }
                    if let Some(v) = font {
                        t.font = v;
                    }
                    if let Some(v) = bold {
                        t.bold = v;
                    }
                    if let Some(v) = italic {
                        t.italic = v;
                    }
                    if let Some(v) = align {
                        t.align = v;
                    }
                    if let Some(v) = valign {
                        t.valign = v;
                    }
                    if let Some(v) = spacing {
                        t.spacing = v;
                    }
                    if let Some(v) = path {
                        t.path = Some(v);
                        t.frame.clear();
                    }
                    if let Some(v) = flip_x {
                        t.flip_x = v;
                    }
                    if let Some(v) = flip_y {
                        t.flip_y = v;
                    }
                    made_ref.push(id);
                }
                C::AddConstraint { constraint, .. } => {
                    let before = edit.redundancy().ok();
                    let id = edit.add_constraint(constraint);
                    if let Some(before) = before {
                        edit.require_needed(before, &id.to_string())?;
                    }
                }
                C::AddDimension {
                    dimension,
                    value,
                    driven,
                    text,
                    ..
                } => {
                    let before = edit.redundancy().ok();
                    let id = edit.add_dimension(dimension, value.as_ref(), driven, None)?;
                    if let (Some(before), false) = (before, driven) {
                        edit.require_needed(before, &id.to_string())?;
                    }
                    if let Some(at) = text {
                        edit.set_text_position(id, at)?;
                    }
                }
                C::SetDimension {
                    dimension, value, ..
                } => edit.set_dimension(dimension, &value)?,
                C::SetDriven {
                    dimension, driven, ..
                } => {
                    let before = edit.redundancy().ok();
                    edit.set_driven(dimension, driven)?;
                    if let (Some(before), false) = (before, driven) {
                        edit.require_needed(before, &dimension.to_string())?;
                    }
                }
                C::Remove { items, .. } => {
                    let items = items
                        .iter()
                        .map(|i| i.parse::<Item>())
                        .collect::<Result<Vec<_>, _>>()?;
                    edit.remove(&items)?;
                }
                C::SetConstruction {
                    curves,
                    construction,
                    ..
                } => edit.set_construction(&curves, construction)?,
                C::SetCenterline {
                    lines, centerline, ..
                } => edit.set_centerline(&lines, centerline)?,
                C::SetFixed {
                    entities, fixed, ..
                } => edit.set_fixed(&entities, fixed)?,
                C::Drag {
                    entity,
                    to,
                    by,
                    radius,
                    ..
                } => {
                    let target = match (entity, to, by, radius) {
                        (Ref::Point(p), Some(to), None, None) => DragTarget::Point(p, to),
                        (Ref::Curve(c), None, Some(by), None) => DragTarget::By(c, by),
                        (Ref::Curve(c), None, None, Some(r)) => DragTarget::Radius(c, r),
                        _ => {
                            return Err(
                                "drag a point \"to\" a position, or a curve \"by\" an offset or \
                                 to a \"radius\""
                                    .to_owned(),
                            );
                        }
                    };
                    edit.drag(&[target])?;
                }
                C::Move {
                    entities,
                    by,
                    rotate,
                    copy,
                    ..
                } => {
                    let map = match (by, rotate) {
                        (Some(by), None) => Map2::translation(by),
                        (None, Some(r)) => {
                            let center = match point(&r.center)? {
                                PointInput::At(at) => at,
                                PointInput::Existing(p) => edit.at(p)?,
                            };
                            Map2::rotation(center, r.angle)
                        }
                        _ => return Err("give \"by\" or \"rotate\"".to_owned()),
                    };
                    if copy {
                        let copies =
                            edit.copy_entities(&entities, &map, &Default::default(), true)?;
                        for r in &entities {
                            if let Some(c) = copies.get(&r.uid()) {
                                made_ref.push(
                                    match r {
                                        Ref::Point(_) => Ref::Point(*c),
                                        Ref::Curve(_) => Ref::Curve(*c),
                                    }
                                    .to_string(),
                                );
                            }
                        }
                    } else {
                        edit.move_entities(&entities, &map)?;
                    }
                }
                C::Trim { curve, at, .. } => edit.trim(curve, at)?,
                C::Extend { curve, at, .. } => edit.extend(curve, at)?,
                C::Fillet { a, b, radius, .. } => {
                    let arc = edit.fillet(a, b, &radius)?;
                    made_ref.push(Ref::Curve(arc).to_string());
                }
                C::Chamfer {
                    a,
                    b,
                    distance,
                    distance2,
                    angle,
                    ..
                } => {
                    let line = edit.chamfer(a, b, &distance, distance2.as_ref(), angle.as_ref())?;
                    made_ref.push(Ref::Curve(line).to_string());
                }
                C::Offset {
                    curves, distance, ..
                } => {
                    let (_, made) = edit.offset(&curves, &distance)?;
                    *made_ref = curve_list(made);
                }
                C::Mirror { entities, axis, .. } => {
                    *made_ref = edit
                        .mirror(&entities, axis)?
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                }
                C::CircularPattern {
                    entities,
                    center,
                    count,
                    angle,
                    ..
                } => {
                    let id = edit.circular_pattern(&entities, point(&center)?, count, &angle)?;
                    *made_ref = pattern_made(&edit.def, id);
                }
                C::RectangularPattern {
                    entities,
                    direction,
                    count,
                    spacing,
                    ..
                } => {
                    let id = edit.rectangular_pattern(
                        &entities,
                        direction,
                        count,
                        [&spacing[0], &spacing[1]],
                    )?;
                    *made_ref = pattern_made(&edit.def, id);
                }
                C::EditPattern {
                    pattern,
                    count,
                    angle,
                    spacing,
                    direction,
                    center,
                    entities,
                    ..
                } => {
                    let change = PatternChange {
                        count: count.as_ref().map(pattern_count).transpose()?,
                        angle,
                        spacing,
                        direction,
                        center: center.as_ref().map(point).transpose()?,
                        entities,
                    };
                    edit.edit_pattern(pattern, &change)?;
                    *made_ref = pattern_made(&edit.def, pattern);
                }
                C::EditOffset {
                    offset,
                    distance,
                    flip,
                    ..
                } => {
                    edit.edit_offset(offset, distance.as_ref(), flip)?;
                    if let Some(o) = edit.def.offsets.iter().find(|o| o.id == offset) {
                        *made_ref = curve_list(o.made());
                    }
                }
                C::SetDimensionText {
                    dimension, text, ..
                } => edit.set_text_position(dimension, text)?,
                C::Create { .. }
                | C::AddRectangle { .. }
                | C::AddCircle { .. }
                | C::Project { .. }
                | C::ImportDxf { .. } => {
                    unreachable!("handled above")
                }
            }
            Ok(())
        })?;
        let mut value = report_json(&report, self.sketch_def(sketch));
        if !made.is_empty() {
            value["made"] = json!(made);
        }
        Ok(value)
    }

    fn sketch_def(&self, uid: FeatureUid) -> Option<&SketchDef> {
        match &self.feature(uid)?.def {
            FeatureDef::Sketch(def) => Some(def),
            _ => None,
        }
    }

    /// The `sketch` query: definition, solved geometry, status and regions.
    pub(super) fn sketch_json(&self, uid: FeatureUid) -> Result<Value, ApiError> {
        let entry = self
            .feature(uid)
            .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
        let FeatureDef::Sketch(def) = &entry.def else {
            return Err(ApiError(format!("{} ({uid}) is not a sketch", entry.name)));
        };
        let params = self.parameters();
        let output: Option<&SketchOutput> = self.sketch_output(uid);
        // Linked projections where their sources are now.
        let followed = self.followed_sketch(uid, def);
        let def = followed.as_ref().unwrap_or(def);
        // The status of the definition with the current values, whether
        // or not the sketch evaluated.
        let solved = def.solve_with(
            &mut |id| {
                params
                    .value(id)
                    .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
            },
            &|id| params.name(id),
        );
        let (solved, error) = match solved {
            Ok(solved) => (Some(solved), None),
            Err(e) => (None, Some(e)),
        };
        let status = match (&solved, &error) {
            (Some(s), _) => s.status.clone(),
            (None, Some(e)) => (*e.status).clone(),
            _ => SketchStatus::default(),
        };
        // What derived offsets compute (P4): not solved (a drag or a move pulls
        // their source), their inner points not shown.
        let derived = derived_entities(&def.offsets, &EntityIndex::new(&def.entities));
        let entities: Vec<Value> = def
            .entities
            .iter()
            .map(|e| {
                let mut value = serde_json::to_value(e).expect("serializes");
                if derived.contains(&e.id) {
                    value["derived"] = json!(true);
                }
                if let Some(solved) = &solved {
                    match &e.kind {
                        EntityKind::Point { .. } => {
                            if let Some(at) = solved.points.get(&e.id) {
                                value["at"] = json!(at);
                            }
                        }
                        _ => {
                            if let Some(curve) = solved.curves.get(&e.id) {
                                value["geometry"] = curve_json(curve);
                            }
                        }
                    }
                }
                value["fully_constrained"] = json!(status.fully_constrained.contains(&e.id));
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
                let measured = match d.value {
                    Some(id) => params.value(id),
                    None => status.driven.get(&d.id).copied(),
                };
                value["measured"] = json!(measured);
                if let Some(param) = d.value.and_then(|id| params.get(id)) {
                    value["expression"] = json!(param.expression());
                }
                value
            })
            .collect();
        let regions: Vec<Value> = output
            .map(|o| {
                // Letter material of texts, for picking a whole text.
                let letters: std::collections::BTreeSet<crate::topo::CurveId> = o
                    .texts
                    .iter()
                    .flat_map(crate::sketch::text::letter_contours)
                    .collect();
                o.region_info
                    .iter()
                    .map(|r| {
                        let loops: Vec<Vec<String>> = r
                            .profile
                            .loops
                            .iter()
                            .map(|l| {
                                let mut keys: Vec<String> =
                                    l.segments.iter().map(|s| s.key.to_string()).collect();
                                keys.dedup();
                                keys
                            })
                            .collect();
                        let mut value = json!({"key": r.profile.key, "area": r.area,
                                               "centroid": r.centroid, "loops": loops});
                        // A region bounded by one glyph contour belongs to
                        // its text.
                        let outer: Vec<crate::topo::CurveId> =
                            r.profile.key.segments().map(|s| s.curve).collect();
                        if let [curve @ crate::topo::CurveId::Glyph { text, .. }] = outer.as_slice()
                        {
                            value["text"] = json!(format!("t{}", text.0));
                            value["letter"] = json!(letters.contains(curve));
                        }
                        value
                    })
                    .collect()
            })
            .unwrap_or_default();
        let frame = output.map(|o| {
            json!({"origin": o.frame.origin, "x_axis": o.frame.x_axis, "y_axis": o.frame.y_axis,
                   "normal": o.frame.normal()})
        });
        // Patterns and offsets with their parameters' names, values and
        // expressions.
        let valued = |id: &crate::parameters::ParamId| {
            json!({"parameter": params.name(*id), "value": params.value(*id),
                   "expression": params.get(*id).map(|p| p.expression().to_owned())})
        };
        let patterns: Vec<Value> = def
            .patterns
            .iter()
            .map(|p| {
                let mut value = serde_json::to_value(
                    p.map_value(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                        .expect("names never fail"),
                )
                .expect("serializes");
                let values: Vec<Value> = p.values().into_iter().map(valued).collect();
                value["values"] = json!(values);
                value
            })
            .collect();
        // Texts with their glyph outlines (when the sketch evaluated and
        // the kernel has fonts) and the font they come from.
        let texts: Vec<Value> = def
            .texts
            .iter()
            .map(|t| {
                let mut value = serde_json::to_value(t).expect("serializes");
                if let Some(o) = output.and_then(|o| o.texts.iter().find(|o| o.id == t.id)) {
                    value["family"] = json!(o.family);
                    value["fallback"] = json!(o.fallback);
                    value["outline"] = json!(
                        o.contours
                            .iter()
                            .map(|c| json!({"id": c.id.to_string(),
                                            "curves": c.parts.iter().map(curve_json)
                                                .collect::<Vec<_>>()}))
                            .collect::<Vec<_>>()
                    );
                }
                value
            })
            .collect();
        let offsets: Vec<Value> = def
            .offsets
            .iter()
            .map(|o| {
                let mut value = serde_json::to_value(
                    o.map_value(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                        .expect("names never fail"),
                )
                .expect("serializes");
                value["value"] = valued(&o.distance);
                value
            })
            .collect();
        let mut result = json!({
            "uid": uid,
            "name": entry.name,
            "plane": def.plane,
            "frame": frame,
            "solved": solved.is_some(),
            "error": error.map(|e| e.message),
            "entities": entities,
            "constraints": def.constraints,
            "dimensions": dimensions,
            "projections": def.projections,
            "texts": texts,
            "patterns": patterns,
            "offsets": offsets,
            "regions": regions,
        });
        let status = status_json(&status, def);
        for key in ["dof", "fully_constrained", "redundant", "conflicts"] {
            result[key] = status[key].clone();
        }
        Ok(result)
    }
}

fn shape_added(added: &crate::document::ShapeAdded, def: Option<&SketchDef>) -> Value {
    let curves: Vec<String> = added
        .curves
        .iter()
        .map(|c| Ref::Curve(*c).to_string())
        .collect();
    let mut value = report_json(&added.report, def);
    value["curves"] = json!(curves);
    value["region"] = json!(added.region);
    value
}

/// Parses a `sketch.*` command.
pub(super) fn parse_sketch_command(value: Value) -> Result<SketchCommand, ApiError> {
    parse(value, "sketch command")
}
