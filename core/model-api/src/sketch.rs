// SPDX-License-Identifier: MIT
//! Sketch commands (`sketch.*`) and the `sketch` query; see `commands.md`.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Value, json};

use super::dispatch::named_enum;
use super::sketch_clipboard::SketchClipboardOps;
use super::{ApiError, parse};
use mitcad_model::Document;
use mitcad_model::features::sketch::{FrameDef, SketchDef, SketchOutput};
use mitcad_model::features::{FeatureDef, OccurrencePath, SketchPlane, ValueInput};
use mitcad_model::ids::{BodyUid, EntityUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::sketch::cleanup::Removal;
use mitcad_model::sketch::edit::{EditReport, PointInput, SketchEdit};
use mitcad_model::sketch::geometry::Curve2;
use mitcad_model::sketch::modify::{DragTarget, Item, Map2, point_input};
use mitcad_model::sketch::offset::derived_entities;
use mitcad_model::sketch::pattern::PatternChange;
use mitcad_model::sketch::regions::Region;
use mitcad_model::sketch::solve::{SketchStatus, SolverItem};
use mitcad_model::sketch::spacing::{SpacingDimension, SpacingInput};
use mitcad_model::sketch::stale::Keep;
use mitcad_model::sketch::tools::{ArcMode, CircleMode, RectangleMode, SlotMode};
use mitcad_model::sketch::{
    ConstraintKind, ConstraintUid, DimensionKind, EntityIndex, EntityKind, Ref, SketchText,
    SpacingMode, TextAlign, TextPath, TextVAlign,
};
use mitcad_model::topo::{RegionKey, TopoName};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", deny_unknown_fields)]
    pub(super) enum SketchCommand {
        "sketch.create" => Create {
            #[serde(default)]
            plane: SketchPlane,
            #[serde(default)]
            frame: Option<FrameDef>,
            #[serde(default)]
            name: Option<String>,
            /// Where the plane was picked: the occurrence path from the root
            /// to its component (mitcad#100); another component's plane is
            /// linked.
            #[serde(default)]
            occurrence: Option<OccurrencePath>,
            /// Where the active component is seen (its first occurrence path
            /// when left out).
            #[serde(default)]
            context: Option<OccurrencePath>,
            /// Without `frame`: the sketch gets the own frame that shows
            /// the design upright when it opens, the design's +Z up (+Y on
            /// a horizontal plane; mitcad#692, `FrameDef::upright`). Create
            /// Sketch in the application asks for it.
            #[serde(default)]
            upright: bool,
        },
        "sketch.add_rectangle" => AddRectangle {
            sketch: FeatureUid,
            corner: [f64; 2],
            width: ValueInput,
            height: ValueInput,
        },
        "sketch.add_circle" => AddCircle {
            sketch: FeatureUid,
            center: [f64; 2],
            diameter: ValueInput,
        },
        "sketch.add_point" => AddPoint {
            sketch: FeatureUid,
            at: [f64; 2],
            #[serde(default)]
            fixed: bool,
        },
        "sketch.add_line" => AddLine {
            sketch: FeatureUid,
            start: Value,
            end: Value,
            #[serde(default)]
            construction: bool,
            #[serde(default)]
            centerline: bool,
        },
        "sketch.rectangle" => Rectangle {
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
            /// Rounded corners of this radius (mitcad#867).
            #[serde(default)]
            radius: Option<ValueInput>,
        },
        "sketch.circle" => Circle {
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
        "sketch.arc" => Arc {
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
        "sketch.polygon" => Polygon {
            sketch: FeatureUid,
            center: [f64; 2],
            vertex: [f64; 2],
            sides: usize,
            #[serde(default = "yes")]
            inscribed: bool,
            #[serde(default)]
            construction: bool,
        },
        "sketch.slot" => Slot {
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
        "sketch.ellipse" => Ellipse {
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
        "sketch.spline" => Spline {
            sketch: FeatureUid,
            points: Vec<Value>,
            #[serde(default)]
            degree: Option<u32>,
            /// A closed fit point spline (mitcad#783).
            #[serde(default)]
            closed: bool,
            #[serde(default)]
            construction: bool,
        },
        "sketch.add_text" => AddText {
            sketch: FeatureUid,
            /// May be left out with a `value` (mitcad#664).
            #[serde(default)]
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
            #[serde(default)]
            cap_height: bool,
            #[serde(default)]
            wrap: bool,
            #[serde(default)]
            line_spacing: f64,
            #[serde(default)]
            pen_align: bool,
            #[serde(default)]
            ink_top: bool,
            #[serde(default)]
            glyphs: Option<mitcad_model::sketch::StoredGlyphs>,
            /// A font file instead of an installed font (mitcad#473).
            #[serde(default)]
            font_file: Option<String>,
            /// A text parameter or a text expression the text follows
            /// (mitcad#664).
            #[serde(default)]
            value: Option<ValueInput>,
        },
        // Sketch text edited afterwards (P3).
        "sketch.edit_text" => EditText {
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
            #[serde(default)]
            cap_height: Option<bool>,
            #[serde(default)]
            wrap: Option<bool>,
            #[serde(default)]
            line_spacing: Option<f64>,
            #[serde(default)]
            pen_align: Option<bool>,
            #[serde(default)]
            ink_top: Option<bool>,
            /// A font file (mitcad#473); "" leaves it for the installed `font`.
            #[serde(default)]
            font_file: Option<String>,
            /// A text parameter or a text expression the text follows
            /// (mitcad#664); "" stops it following one.
            #[serde(default)]
            value: Option<ValueInput>,
        },
        "sketch.add_constraint" => AddConstraint {
            sketch: FeatureUid,
            constraint: ConstraintKind,
        },
        "sketch.add_dimension" => AddDimension {
            sketch: FeatureUid,
            dimension: DimensionKind,
            #[serde(default)]
            value: Option<ValueInput>,
            #[serde(default)]
            driven: bool,
            #[serde(default)]
            text: Option<[f64; 2]>,
            /// The first driving dimension scales the sketch to itself
            /// (mitcad#860, `scale.rs`): the application's preference.
            #[serde(default)]
            scale_sketch: bool,
        },
        "sketch.set_dimension" => SetDimension {
            sketch: FeatureUid,
            dimension: ConstraintUid,
            value: ValueInput,
        },
        "sketch.set_driven" => SetDriven {
            sketch: FeatureUid,
            dimension: ConstraintUid,
            #[serde(default = "yes")]
            driven: bool,
        },
        "sketch.set_signed" => SetSigned {
            sketch: FeatureUid,
            dimension: ConstraintUid,
            #[serde(default = "yes")]
            signed: bool,
        },
        "sketch.remove" => Remove {
            sketch: FeatureUid,
            items: Vec<String>,
        },
        "sketch.set_construction" => SetConstruction {
            sketch: FeatureUid,
            #[serde(with = "curves")]
            curves: Vec<EntityUid>,
            #[serde(default = "yes")]
            construction: bool,
        },
        "sketch.set_centerline" => SetCenterline {
            sketch: FeatureUid,
            #[serde(with = "curves")]
            lines: Vec<EntityUid>,
            #[serde(default = "yes")]
            centerline: bool,
        },
        "sketch.set_fixed" => SetFixed {
            sketch: FeatureUid,
            entities: Vec<Ref>,
            #[serde(default = "yes")]
            fixed: bool,
        },
        "sketch.drag" => Drag {
            sketch: FeatureUid,
            #[serde(default)]
            entity: Option<Ref>,
            /// A selection dragged together by `by` (mitcad#837), instead
            /// of one `entity`.
            #[serde(default)]
            entities: Option<Vec<Ref>>,
            #[serde(default)]
            to: Option<[f64; 2]>,
            #[serde(default)]
            by: Option<[f64; 2]>,
            #[serde(default)]
            radius: Option<f64>,
        },
        "sketch.move" => Move {
            sketch: FeatureUid,
            entities: Vec<Ref>,
            #[serde(default)]
            by: Option<[f64; 2]>,
            #[serde(default)]
            rotate: Option<Rotation>,
            #[serde(default)]
            copy: bool,
        },
        "sketch.trim" => Trim {
            sketch: FeatureUid,
            #[serde(with = "mitcad_model::internal::ids::curve_serde")]
            curve: EntityUid,
            at: [f64; 2],
        },
        "sketch.extend" => Extend {
            sketch: FeatureUid,
            #[serde(with = "mitcad_model::internal::ids::curve_serde")]
            curve: EntityUid,
            at: [f64; 2],
        },
        "sketch.fillet" => Fillet {
            sketch: FeatureUid,
            #[serde(default, with = "mitcad_model::sketch::internal::curve_opt_serde")]
            a: Option<EntityUid>,
            #[serde(default, with = "mitcad_model::sketch::internal::curve_opt_serde")]
            b: Option<EntityUid>,
            /// Several corners at once (mitcad#867): where two of these
            /// lines end together, and at these corner points.
            #[serde(default, with = "curves_opt")]
            curves: Option<Vec<EntityUid>>,
            #[serde(default, with = "points_opt")]
            points: Option<Vec<EntityUid>>,
            radius: ValueInput,
        },
        "sketch.chamfer" => Chamfer {
            sketch: FeatureUid,
            #[serde(with = "mitcad_model::internal::ids::curve_serde")]
            a: EntityUid,
            #[serde(with = "mitcad_model::internal::ids::curve_serde")]
            b: EntityUid,
            distance: ValueInput,
            #[serde(default)]
            distance2: Option<ValueInput>,
            #[serde(default)]
            angle: Option<ValueInput>,
        },
        "sketch.offset" => Offset {
            sketch: FeatureUid,
            #[serde(with = "curves")]
            curves: Vec<EntityUid>,
            distance: ValueInput,
        },
        "sketch.mirror" => Mirror {
            sketch: FeatureUid,
            entities: Vec<Ref>,
            /// A line (`c4`), or the sketch's own axis `"x"` or `"y"`,
            /// made in the same step when the sketch lacks it (mitcad#876).
            axis: String,
        },
        "sketch.circular_pattern" => CircularPattern {
            sketch: FeatureUid,
            entities: Vec<Ref>,
            center: Value,
            count: u32,
            #[serde(default = "full_turn")]
            angle: ValueInput,
        },
        "sketch.rectangular_pattern" => RectangularPattern {
            sketch: FeatureUid,
            entities: Vec<Ref>,
            #[serde(default = "x_axis")]
            direction: [f64; 2],
            count: [u32; 2],
            spacing: [ValueInput; 2],
            // Per direction: the originals in the middle (an odd count).
            #[serde(default)]
            symmetric: [bool; 2],
        },
        // Sketch patterns and offsets edited afterwards (P4).
        "sketch.edit_pattern" => EditPattern {
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
        "sketch.edit_offset" => EditOffset {
            sketch: FeatureUid,
            offset: ConstraintUid,
            #[serde(default)]
            distance: Option<ValueInput>,
            #[serde(default)]
            flip: bool,
        },
        "sketch.project" => Project {
            sketch: FeatureUid,
            /// Left out for the section of a whole `body` (mitcad#736);
            /// another sketch's curve, point or curves (mitcad#904).
            #[serde(default)]
            source: Option<ProjectSource>,
            #[serde(default)]
            body: Option<BodyUid>,
            #[serde(default)]
            linked: bool,
            /// As in `sketch.create` (mitcad#100).
            #[serde(default)]
            occurrence: Option<OccurrencePath>,
            #[serde(default)]
            context: Option<OccurrencePath>,
            /// Section curves (mitcad#389): where the sketch's plane cuts the
            /// source face instead of its projection; of an edge, the points
            /// where the plane crosses it, of a whole body every face's
            /// section (Intersect, mitcad#736).
            #[serde(default)]
            section: bool,
        },
        // Sketch mode (U2): a dimension's value dragged to another place.
        "sketch.set_dimension_text" => SetDimensionText {
            sketch: FeatureUid,
            dimension: ConstraintUid,
            text: [f64; 2],
        },
        // Files (U6): a DXF drawing's geometry into the sketch.
        "sketch.import_dxf" => ImportDxf {
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
        // Equal spacing (mitcad#221).
        "sketch.add_equal_spacing" => AddEqualSpacing {
            sketch: FeatureUid,
            anchors: Vec<Ref>,
            mode: SpacingMode,
            #[serde(default, with = "mitcad_model::sketch::internal::curve_opt_serde")]
            direction: Option<EntityUid>,
            #[serde(default)]
            keep_order: bool,
            /// A common spacing dimension: driving with `spacing` (or the mean
            /// of the current intervals), or `driven`.
            #[serde(default)]
            dimension: bool,
            #[serde(default)]
            spacing: Option<ValueInput>,
            #[serde(default)]
            driven: bool,
            #[serde(default)]
            text: Option<[f64; 2]>,
        },
        "sketch.edit_equal_spacing" => EditEqualSpacing {
            sketch: FeatureUid,
            constraint: ConstraintUid,
            #[serde(default)]
            anchors: Option<Vec<Ref>>,
            #[serde(default)]
            mode: Option<SpacingMode>,
            #[serde(default, with = "mitcad_model::sketch::internal::curve_opt_serde")]
            direction: Option<EntityUid>,
            #[serde(default)]
            keep_order: bool,
        },
        // Duplicate and overlapping projected curves (mitcad#232).
        "sketch.clean_up_projections" => CleanUpProjections {
            sketch: FeatureUid,
            /// Also lines on or overlapping a collinear one.
            #[serde(default = "yes")]
            merge: bool,
        },
        // Projected curves that no longer match their source (mitcad#252).
        "sketch.clean_up_stale_projections" => CleanUpStaleProjections { sketch: FeatureUid },
        // Projected geometry into ordinary sketch geometry (mitcad#257).
        "sketch.convert_projected" => ConvertProjected {
            sketch: FeatureUid,
            entities: Vec<Ref>,
        },
        // An SVG drawing's geometry and texts into the sketch (mitcad#473).
        "sketch.import_svg" => ImportSvg {
            sketch: FeatureUid,
            path: String,
            /// Sketch millimetres per millimetre of the drawing.
            #[serde(default = "one")]
            scale: f64,
            /// Where the drawing's lower left corner goes.
            #[serde(default)]
            at: Option<[f64; 2]>,
            /// Stroked paths also as bands of their stroke width.
            #[serde(default)]
            stroke_outline: bool,
        },
        // QR codes (mitcad#473): a text drawn as a code of its content.
        "sketch.add_code" => AddCode {
            sketch: FeatureUid,
            text: String,
            /// The lower left corner of the modules.
            #[serde(default)]
            at: [f64; 2],
            /// A module's size, mm.
            #[serde(default = "one")]
            module: f64,
            #[serde(default)]
            angle: f64,
            #[serde(default)]
            kind: mitcad_model::sketch::CodeKind,
            #[serde(default)]
            ecc: mitcad_model::sketch::CodeEcc,
        },
        "sketch.edit_code" => EditCode {
            sketch: FeatureUid,
            id: String,
            #[serde(default)]
            text: Option<String>,
            #[serde(default)]
            at: Option<[f64; 2]>,
            #[serde(default)]
            module: Option<f64>,
            #[serde(default)]
            angle: Option<f64>,
            #[serde(default)]
            ecc: Option<mitcad_model::sketch::CodeEcc>,
        },
        // The sketch's own axes and origin as entities (mitcad#500): the one there
        // is, or a new one.
        "sketch.axis" => Axis { sketch: FeatureUid, axis: String },
        // Inactive constraints and dimensions (mitcad#721).
        "sketch.set_active" => SetActive {
            sketch: FeatureUid,
            items: Vec<ConstraintUid>,
            #[serde(default = "yes")]
            active: bool,
            #[serde(default)]
            reason: Option<String>,
        },
        // Copy, Cut and Paste of sketch geometry (mitcad#788,
        // `sketch_clipboard.rs`): a `sketch_fragment` answer's fragment.
        "sketch.paste" => Paste {
            sketch: FeatureUid,
            fragment: Value,
            /// Where the fragment's base point goes; where it was when left
            /// out.
            #[serde(default)]
            at: Option<[f64; 2]>,
        },
        // Redefine Sketch Plane (mitcad#787, `sketch_plane.rs`).
        "sketch.redefine_plane" => RedefinePlane(super::sketch_plane::RedefinePlane),
        // Insert Point on a spline (mitcad#836): a fit point, or a control
        // point by knot insertion that keeps the shape.
        "sketch.insert_spline_point" => InsertSplinePoint {
            sketch: FeatureUid,
            #[serde(with = "mitcad_model::internal::ids::curve_serde")]
            curve: EntityUid,
            at: [f64; 2],
        },
        // The whole sketch scaled by a factor about a point (mitcad#860,
        // `scale.rs`).
        "sketch.scale" => Scale {
            sketch: FeatureUid,
            factor: f64,
            /// A point id or `[x, y]`; the sketch origin when left out.
            #[serde(default)]
            center: Option<Value>,
        },
    }
}

fn one() -> f64 {
    1.0
}

/// A code's content, size and place checked before it is added or
/// changed (mitcad#473): a content a QR code holds, a module's size above
/// zero.
fn code_fits(text: &SketchText) -> Result<(), String> {
    if text.text.is_empty() {
        return Err("the code's content is empty".to_owned());
    }
    if !(text.height.is_finite() && text.height > 0.0) {
        return Err(format!(
            "the module size must be greater than zero, got {}",
            text.height
        ));
    }
    if let Some(code) = &text.code {
        mitcad_model::sketch::code::modules(&text.text, code)?;
    }
    Ok(())
}

/// An SVG file read (mitcad#473); a panic of the reader is its error
/// (mitcad#922: it runs in the application's process).
fn read_svg(
    path: &str,
    options: &mitcad_model::sketch::svg::SvgOptions,
) -> Result<mitcad_model::sketch::svg::SvgDrawing, ApiError> {
    mitcad_model::reader_guard::catch_panic(
        path,
        || {
            let text =
                std::fs::read_to_string(path).map_err(|e| ApiError(format!("{path}: {e}")))?;
            mitcad_model::sketch::svg::read(&text, options)
                .map_err(|e| ApiError(format!("{path}: {e}")))
        },
        ApiError,
    )
}

/// A DXF file read (P9); a panic of the reader is its error (mitcad#922:
/// it runs in the application's process).
fn read_dxf(path: &str) -> Result<mitcad_dxf::Drawing, ApiError> {
    mitcad_model::reader_guard::catch_panic(
        path,
        || mitcad_dxf::read_file(path).map_err(|e| ApiError(format!("{path}: {e}"))),
        ApiError,
    )
}

/// The `svg_info` query (mitcad#473): what the Insert SVG dialog shows, the
/// drawing's size in millimetres and what it holds.
pub(super) fn svg_info_json(path: &str) -> Result<Value, ApiError> {
    let drawing = read_svg(path, &mitcad_model::sketch::svg::SvgOptions::default())?;
    let paths: usize = drawing.shapes.iter().map(|s| s.paths.len()).sum();
    Ok(json!({
        "size": drawing.size,
        "shapes": drawing.shapes.len(),
        "paths": paths,
        "filled": drawing.shapes.iter().filter(|s| s.fill.is_some()).count(),
        "stroked": drawing.shapes.iter().filter(|s| s.stroke.is_some()).count(),
        "texts": drawing.texts.len(),
        "warnings": drawing.warnings,
    }))
}

/// The `dxf_info` query (P9): what the Insert DXF dialog offers, its
/// unit, its layers and where its entities lie.
pub(super) fn dxf_info_json(path: &str) -> Result<Value, ApiError> {
    let drawing = read_dxf(path)?;
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
            let bounds = bounds_json(drawing.layer_bounds(&name));
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

/// `sketch.project`'s `source`: model geometry by its topological name,
/// or another sketch's geometry (mitcad#904): `{"sketch": "F5", "curve":
/// "c3"}`, `{"sketch": "F5", "point": "p2"}`, or `{"sketch": "F5"}` for
/// every curve of the sketch.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ProjectSource {
    Topo(TopoName),
    Sketch {
        sketch: FeatureUid,
        entity: Option<Ref>,
    },
}

impl<'de> Deserialize<'de> for ProjectSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        match Value::deserialize(d)? {
            Value::String(text) => text
                .parse::<TopoName>()
                .map(Self::Topo)
                .map_err(D::Error::custom),
            Value::Object(map) if map.len() == 1 && map.contains_key("sketch") => {
                let sketch: FeatureUid = map["sketch"]
                    .as_str()
                    .ok_or_else(|| D::Error::custom("\"sketch\" is a sketch's uid like \"F5\""))?
                    .parse()
                    .map_err(D::Error::custom)?;
                Ok(Self::Sketch {
                    sketch,
                    entity: None,
                })
            }
            other => mitcad_model::sketch::SketchEntityRef::from_json(&other)
                .map(|e| Self::Sketch {
                    sketch: e.sketch,
                    entity: Some(e.entity),
                })
                .map_err(D::Error::custom),
        }
    }
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

    use mitcad_model::ids::EntityUid;

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EntityUid>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|n| EntityUid::parse_curve(n).map_err(serde::de::Error::custom))
            .collect()
    }
}

mod curves_opt {
    use mitcad_model::ids::EntityUid;

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Vec<EntityUid>>, D::Error> {
        super::curves::deserialize(d).map(Some)
    }
}

/// An optional list of point ids: `["p3", "p4"]`.
mod points_opt {
    use serde::Deserialize;

    use mitcad_model::ids::EntityUid;
    use mitcad_model::sketch::internal::point_serde;

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Vec<EntityUid>>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|n| point_serde::parse(n).map_err(serde::de::Error::custom))
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }
}

fn point(value: &Value) -> Result<PointInput, String> {
    point_input(value)
}

/// Mirror's line (mitcad#876): a line `c4`, or the sketch's own axis
/// `"x"` or `"y"`, made (with the origin point) in the edit when the
/// sketch has none, as `sketch.axis` makes it.
fn mirror_axis(edit: &mut SketchEdit<'_>, axis: &str) -> Result<EntityUid, String> {
    use mitcad_model::sketch::SketchAxis;
    match axis {
        "x" => Ok(edit.axis(SketchAxis::X)),
        "y" => Ok(edit.axis(SketchAxis::Y)),
        _ => EntityUid::parse_curve(axis)
            .map_err(|_| format!("invalid mirror line '{axis}' (expected c<number>, x or y)")),
    }
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
    let mut value = json!({
        "dof": status.dof,
        "fully_constrained": refs,
        "redundant": status.redundant.iter().map(item).collect::<Vec<_>>(),
        "conflicts": status.conflicts.iter()
            .map(|set| set.iter().map(item).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
    });
    // Only when something collapsed (mitcad#365): the status stays as it was.
    if !status.degenerate.is_empty() {
        value["degenerate"] = degenerate_ids(status);
    }
    value
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
    if !report.warnings.is_empty() {
        value["warnings"] = json!(report.warnings);
    }
    if let Some(def) = def {
        value["status"] = status_json(&report.status, def);
    }
    value
}

fn curve_json(curve: &Curve2) -> Value {
    match curve {
        Curve2::Line { a, b } => json!({"start": a, "end": b}),
        Curve2::Circle { center, radius, .. } => json!({"center": center, "radius": radius}),
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

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait SketchOps<K: Kernel> {
    fn run_sketch_command(&mut self, command: SketchCommand) -> Result<Value, ApiError>;

    /// The `sketch` query's text: the last answer while nothing it is made
    /// from changed (mitcad#679, [`Document::known_sketch_answer`]), else
    /// made and kept.
    fn sketch_answer(&self, uid: FeatureUid) -> Result<String, ApiError>;

    /// The `plane_geometry` query: the visible bodies' edges and vertices
    /// in the sketch's plane, in sketch coordinates, each with the `source`,
    /// `body` and `occurrence` that `sketch.project` takes; with
    /// `out_of_plane` the others too, projected, with `in_plane`, `path`
    /// and `height` (mitcad#866).
    fn plane_geometry_json(
        &self,
        sketch: FeatureUid,
        out_of_plane: bool,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> SketchOps<K> for Document<K> {
    fn run_sketch_command(&mut self, command: SketchCommand) -> Result<Value, ApiError> {
        Ok(match command {
            SketchCommand::Create {
                plane,
                frame,
                name,
                occurrence,
                context,
                upright,
            } => {
                let plane_link = match &occurrence {
                    Some(path) => mitcad_model::internal::links::link_for(
                        self.assembly(),
                        self.active_component(),
                        &path.0,
                        context.as_ref().map(|c| &c.0[..]),
                    )
                    .map_err(|e| ApiError(format!("plane: {e}")))?,
                    None => None,
                };
                let upright = upright && frame.is_none();
                let def = SketchDef {
                    plane_link,
                    frame: frame.map(Box::new),
                    ..SketchDef::new(plane)
                };
                let depth = self.undo_depth();
                let added = self.add_feature(&FeatureDef::Sketch(def.clone()), name.as_deref())?;
                if upright {
                    self.stand_upright(added.uid, def, &occurrence, &context, depth, &added.name)?;
                }
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
                occurrence,
                context,
                section,
            } => {
                let occurrence = occurrence.as_ref().map(|o| &o.0[..]);
                let context = context.as_ref().map(|c| &c.0[..]);
                let topo = match &source {
                    Some(ProjectSource::Topo(name)) => Some(name),
                    _ => None,
                };
                let report = match (&source, section) {
                    // Another sketch's geometry (mitcad#904).
                    (Some(ProjectSource::Sketch { .. }), true) => {
                        return Err(ApiError(
                            "a section's source is a face, an edge or a body, not a sketch's \
                             geometry"
                                .to_owned(),
                        ));
                    }
                    (
                        Some(ProjectSource::Sketch {
                            sketch: from,
                            entity,
                        }),
                        false,
                    ) => {
                        if body.is_some() {
                            return Err(ApiError(
                                "sketch.project of a sketch's geometry takes no body".to_owned(),
                            ));
                        }
                        let entities = entity.map(|e| vec![e]);
                        self.project_sketch_geometry(
                            sketch,
                            *from,
                            entities.as_deref(),
                            linked,
                            occurrence,
                            context,
                        )?
                    }
                    (_, true) => {
                        self.section_from(sketch, topo, body, linked, occurrence, context)?
                    }
                    (Some(ProjectSource::Topo(source)), false) => {
                        self.project_from(sketch, source, body, linked, occurrence, context)?
                    }
                    (None, false) => {
                        return Err(ApiError(
                            "sketch.project needs a source (only a section takes a whole body)"
                                .to_owned(),
                        ));
                    }
                };
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
                let mut drawing = read_dxf(&path)?.to_millimeters(unitless);
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
                        mitcad_model::sketch::dxf::insert_drawing(
                            edit,
                            &drawing,
                            at.unwrap_or_default(),
                        )
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
            SketchCommand::CleanUpProjections { sketch, merge } => {
                self.clean_up_projections(sketch, merge)?
            }
            SketchCommand::CleanUpStaleProjections { sketch } => {
                self.clean_up_stale_projections(sketch)?
            }
            SketchCommand::ConvertProjected { sketch, entities } => {
                self.convert_projected(sketch, &entities)?
            }
            SketchCommand::ImportSvg {
                sketch,
                path,
                scale,
                at,
                stroke_outline,
            } => self.import_svg(sketch, &path, scale, at, stroke_outline)?,
            SketchCommand::Axis { sketch, axis } => self.sketch_axis(sketch, &axis)?,
            SketchCommand::Paste {
                sketch,
                fragment,
                at,
            } => self.paste_fragment(sketch, fragment, at)?,
            SketchCommand::RedefinePlane(command) => {
                super::sketch_plane::SketchPlaneOps::redefine_plane_command(self, command)?
            }
            command => self.edit_command(command)?,
        })
    }

    fn sketch_answer(&self, uid: FeatureUid) -> Result<String, ApiError> {
        if let Some(answer) = self.known_sketch_answer(uid) {
            return Ok(answer);
        }
        let answer = self.sketch_json(uid)?.to_string();
        self.keep_sketch_answer(uid, &answer);
        Ok(answer)
    }

    fn plane_geometry_json(
        &self,
        sketch: FeatureUid,
        out_of_plane: bool,
    ) -> Result<Value, ApiError> {
        let items: Vec<Value> = self
            .sketch_body_geometry(sketch, out_of_plane)?
            .into_iter()
            .map(|item| {
                let (kind, geometry) = match &item.shape {
                    mitcad_model::PlaneShape::Vertex(at) => ("vertex", json!({"at": at})),
                    mitcad_model::PlaneShape::Curve(curve) => {
                        (curve_type(curve), curve_json(curve))
                    }
                    // An edge seen end on: its projected point (mitcad#866).
                    mitcad_model::PlaneShape::Point(at) => ("point", json!({"at": at})),
                };
                let mut value = json!({
                    "source": item.source.to_string(),
                    "body": item.body,
                    "kind": kind,
                    "geometry": geometry,
                });
                if let Some(path) = item.occurrence {
                    value["occurrence"] = json!(OccurrencePath(path).to_string());
                }
                // Out of the plane (mitcad#866): where it is, above or below.
                if let Some(out) = item.out_of_plane {
                    value["in_plane"] = json!(false);
                    value["path"] = json!(out.path);
                    value["height"] = json!(out.height);
                }
                value
            })
            .collect();
        Ok(Value::Array(items))
    }
}

/// A document's sketches as the `sketch` query answers them. An extension
/// trait (mitcad#777), so that the api can live in a crate above the model.
pub trait SketchApi<K: Kernel> {
    /// The `sketch` query: definition, solved geometry, status and regions.
    fn sketch_json(&self, uid: FeatureUid) -> Result<Value, ApiError>;
}

impl<K: Kernel> SketchApi<K> for Document<K> {
    fn sketch_json(&self, uid: FeatureUid) -> Result<Value, ApiError> {
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
        // or not the sketch evaluated: the evaluation's own solve while it
        // is that of this definition and these values (mitcad#679), else
        // solved here.
        let solved = match self.evaluated_solve(uid) {
            Some(solved) => Ok(solved),
            None => def
                .solve_with(
                    &mut |id| {
                        params
                            .value(id)
                            .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
                    },
                    &|id| params.name(id),
                )
                .map(std::sync::Arc::new),
        };
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
                // The spacing dimension of an inactive equal spacing is not
                // applied: shown inactive, with its equal spacing's reason
                // (mitcad#723).
                if d.active && !def.dimension_applied(d) {
                    value["active"] = json!(false);
                    if let DimensionKind::Spacing { constraint } = d.kind
                        && let Some(why) = def
                            .constraints
                            .iter()
                            .find(|c| c.id == constraint)
                            .and_then(|c| c.inactive_reason.as_deref())
                    {
                        value["inactive_reason"] = json!(why);
                    }
                }
                if let Some(param) = d.value.or(d.reference).and_then(|id| params.get(id)) {
                    value["expression"] = json!(param.expression());
                    value["comment"] = json!(param.comment());
                    // A limit took effect (mitcad#665).
                    if let Some(limited) = param.limited() {
                        value["limited"] = json!(limited.as_str());
                        value["limit_message"] = json!(super::limits::message(param));
                    }
                }
                value
            })
            .collect();
        let regions: Vec<Value> = output
            .map(|o| {
                // Letter material of texts, for picking a whole text.
                let letters: std::collections::BTreeSet<mitcad_model::topo::CurveId> = o
                    .texts
                    .iter()
                    .flat_map(mitcad_model::sketch::text::letter_contours)
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
                        let outer: Vec<mitcad_model::topo::CurveId> =
                            r.profile.key.segments().map(|s| s.curve).collect();
                        if let [curve @ mitcad_model::topo::CurveId::Glyph { text, .. }] =
                            outer.as_slice()
                        {
                            value["text"] = json!(format!("t{}", text.0));
                            value["letter"] = json!(letters.contains(curve));
                        } else if let Some(text) = outer.first().and_then(|c| c.text())
                            && outer.iter().all(|c| c.text() == Some(text))
                        {
                            // Bounded by several contours of one text, which
                            // touch at corners (a QR code's modules,
                            // mitcad#473): material when a point inside lies
                            // within an odd number of its contours.
                            value["text"] = json!(format!("t{}", text.0));
                            let inside = mitcad_model::sketch::svg::interior_point(
                                &r.outline, &r.holes, r.centroid,
                            );
                            let material = inside.is_some_and(|p| {
                                o.texts
                                    .iter()
                                    .filter(|t| t.id == text)
                                    .flat_map(|t| t.contours.iter())
                                    .filter(|c| {
                                        mitcad_model::sketch::text::composite(&c.parts).is_some_and(
                                            |curve| {
                                                let (lo, hi) = curve.domain();
                                                mitcad_model::sketch::geometry::polygon_contains(
                                                    &curve.sample(lo, hi),
                                                    p,
                                                )
                                            },
                                        )
                                    })
                                    .count()
                                    % 2
                                    == 1
                            });
                            value["letter"] = json!(material);
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
        let valued = |id: &mitcad_model::parameters::ParamId| {
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
                // The stored outlines only as whether the text has them
                // (mitcad#416); `outline` gives the shapes in use.
                if let Some(map) = value.as_object_mut()
                    && map.remove("glyphs").is_some()
                {
                    map.insert("has_glyphs".into(), json!(true));
                }
                // The text parameter it follows (mitcad#664): its text, its
                // name and expression, and whether it is the sketch's own.
                if let Some(param) = def.text_value(t.id).and_then(|id| params.get(id)) {
                    if let Some(text) = param.text() {
                        value["text"] = json!(text);
                    }
                    value["value"] = json!({
                        "parameter": param.name(),
                        "expression": param.expression(),
                        "own": params.owner(param.id()) == Some(uid),
                    });
                }
                if let Some(o) = output.and_then(|o| o.texts.iter().find(|o| o.id == t.id)) {
                    value["family"] = json!(o.family);
                    value["fallback"] = json!(o.fallback);
                    if o.stored {
                        value["stored_outlines"] = json!(true);
                    }
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
        // Linked projections with what of them did not follow (mitcad#232).
        let projections: Vec<Value> = def
            .projections
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let mut value = serde_json::to_value(p).expect("serializes");
                if let Some(report) =
                    output.and_then(|o| o.follow.iter().find(|r| r.projection == k))
                {
                    if report.lost {
                        value["lost"] = json!(true);
                    }
                    if report.kept {
                        value["kept"] = json!(true);
                    }
                    if !report.unfollowed.is_empty() {
                        let ids: Vec<String> = report
                            .unfollowed
                            .iter()
                            .filter_map(|id| p.entities.iter().find(|r| r.uid() == *id))
                            .map(ToString::to_string)
                            .collect();
                        value["unfollowed"] = json!(ids);
                    }
                    if report.unmatched > 0 {
                        value["unmatched"] = json!(report.unmatched);
                    }
                    if !report.stale.is_empty() {
                        let ids: Vec<String> = report
                            .stale
                            .iter()
                            .map(|id| Ref::Curve(*id).to_string())
                            .collect();
                        value["stale"] = json!(ids);
                    }
                }
                value
            })
            .collect();
        // What each warning is about (mitcad#264): a linked projection's
        // warning carries its entities, the others none.
        let warnings = self.warnings(uid);
        let collapsed = status.degenerate_warning();
        let warning_entities: Vec<Vec<String>> = warnings
            .iter()
            .map(|warning| {
                if collapsed.as_deref() == Some(warning.as_str()) {
                    return status
                        .degenerate
                        .iter()
                        .map(|id| Ref::Curve(*id).to_string())
                        .collect();
                }
                output
                    .and_then(|o| {
                        o.follow.iter().find_map(|report| {
                            let p = def.projections.get(report.projection)?;
                            (report.warning(p).as_deref() == Some(warning.as_str()))
                                .then(|| projection_warning_entities(report, p))
                        })
                    })
                    .unwrap_or_default()
            })
            .collect();
        let mut result = json!({
            "uid": uid,
            "name": entry.name,
            "plane": def.plane,
            "plane_link": def.plane_link,
            "frame": frame,
            "solved": solved.is_some(),
            "error": error.map(|e| e.message),
            "warnings": warnings,
            "warning_entities": warning_entities,
            "entities": entities,
            "constraints": def.constraints,
            "dimensions": dimensions,
            "projections": projections,
            "texts": texts,
            "patterns": patterns,
            "offsets": offsets,
            "regions": regions,
        });
        let value = status_json(&status, def);
        for key in ["dof", "fully_constrained", "redundant", "conflicts"] {
            result[key] = value[key].clone();
        }
        result["degenerate"] = degenerate_ids(&status);
        // The inactive constraints and dimensions the geometry does not
        // meet (mitcad#721).
        if !status.unmet_inactive.is_empty() {
            let unmet: Vec<String> = status
                .unmet_inactive
                .iter()
                .map(ToString::to_string)
                .collect();
            result["unmet_inactive"] = json!(unmet);
        }
        Ok(result)
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait SketchLocal<K: Kernel> {
    /// `sketch.import_svg` (mitcad#473): the drawing's curves and texts into
    /// the sketch, with the regions its fills cover (`filled`).
    fn import_svg(
        &mut self,
        sketch: FeatureUid,
        path: &str,
        scale: f64,
        at: Option<[f64; 2]>,
        stroke_outline: bool,
    ) -> Result<Value, ApiError>;

    /// The commands that edit a sketch through [`SketchEdit`].
    fn edit_command(&mut self, command: SketchCommand) -> Result<Value, ApiError>;

    /// `sketch.clean_up_projections` (mitcad#232): the clean-up, with what
    /// it removed and, in `warnings`, what the user is to know before
    /// keeping it (the Clean Up Projections panel shows them).
    fn clean_up_projections(&mut self, sketch: FeatureUid, merge: bool) -> Result<Value, ApiError>;

    /// `sketch.clean_up_stale_projections` (mitcad#252): the clean-up of
    /// the projected curves the last evaluation found stale, with what it
    /// removed, trimmed, unlinked and kept and, in `warnings`, the same in
    /// words and the regions later features use whose names change (the
    /// Clean Up Stale Projections panel shows them).
    fn clean_up_stale_projections(&mut self, sketch: FeatureUid) -> Result<Value, ApiError>;

    /// `sketch.axis` (mitcad#500): the entity that stands for the sketch's
    /// origin or one of its axes, made when the sketch has none (one undo
    /// step `Axis in Sketch1`); the one there is changes nothing. The result
    /// has `entity` and `new`, and the editing commands' fields when it
    /// made one.
    fn sketch_axis(&mut self, sketch: FeatureUid, axis: &str) -> Result<Value, ApiError>;

    /// `sketch.convert_projected` (mitcad#257): projected entities into
    /// ordinary sketch geometry, with what was converted, the shared points
    /// that stay projected and, in `warnings`, what the user is to know.
    fn convert_projected(
        &mut self,
        sketch: FeatureUid,
        entities: &[Ref],
    ) -> Result<Value, ApiError>;

    /// The features after a sketch: their names and their definitions as
    /// text (parameters by name).
    fn later_definitions(&self, sketch: FeatureUid) -> Vec<(String, String)>;

    /// The regions of an evaluated sketch that later features use: the
    /// profiles they sweep, as they find them, and the regions their
    /// definitions name (faces named by them).
    fn used_regions(&self, sketch: FeatureUid) -> Vec<Region>;

    /// Warnings about the regions named `before` an edit of a sketch that
    /// it renamed and later features use.
    fn renamed_regions(&self, sketch: FeatureUid, before: &[String]) -> Vec<String>;

    fn sketch_def(&self, uid: FeatureUid) -> Option<&SketchDef>;

    /// Gives a sketch just added (`def`, as added) the own frame that
    /// shows the design upright as it opens (mitcad#692,
    /// [`FrameDef::upright`]), when that differs from its plane's frame:
    /// one undo step with its adding, the steps after `depth` merged under
    /// the adding's label. The design's axes are taken where the sketch is
    /// shown: through the occurrence of the active component that `context`
    /// or `occurrence` name, else its first one.
    fn stand_upright(
        &mut self,
        uid: FeatureUid,
        def: SketchDef<ValueInput>,
        occurrence: &Option<OccurrencePath>,
        context: &Option<OccurrencePath>,
        depth: usize,
        name: &str,
    ) -> Result<(), ApiError>;
}

impl<K: Kernel> SketchLocal<K> for Document<K> {
    fn import_svg(
        &mut self,
        sketch: FeatureUid,
        path: &str,
        scale: f64,
        at: Option<[f64; 2]>,
        stroke_outline: bool,
    ) -> Result<Value, ApiError> {
        use mitcad_model::sketch::svg;
        let options = svg::SvgOptions {
            scale,
            at: at.unwrap_or_default(),
            stroke_outline,
        };
        let drawing = read_svg(path, &options)?;
        let (inserted, report) = self.edit_sketch(sketch, "Insert SVG into {sketch}", |edit| {
            svg::insert(edit, &drawing, stroke_outline)
        })?;
        let mut value = report_json(&report, self.sketch_def(sketch));
        // The regions the drawing paints, by a point inside each.
        let filled: Vec<String> = self
            .sketch_output(sketch)
            .map(|output| {
                output
                    .region_info
                    .iter()
                    .filter(|r| {
                        svg::interior_point(&r.outline, &r.holes, r.centroid).is_some_and(|p| {
                            drawing.shapes.iter().any(|s| s.covers(p, stroke_outline))
                        })
                    })
                    .map(|r| r.profile.key.to_string())
                    .collect()
            })
            .unwrap_or_default();
        value["curves"] = json!(inserted.curves);
        value["points"] = json!(inserted.points);
        value["text_count"] = json!(inserted.texts);
        value["bands"] = json!(inserted.bands);
        value["filled"] = json!(filled);
        value["size"] = json!(drawing.size);
        value["warnings"] = json!(inserted.skipped);
        Ok(value)
    }

    fn edit_command(&mut self, command: SketchCommand) -> Result<Value, ApiError> {
        use SketchCommand as C;
        let mut made: Vec<String> = Vec::new();
        let made_ref = &mut made;
        // Points that became one (mitcad#259, mitcad#260): the result's
        // `joined` and `merged`.
        let mut joined: Option<Value> = None;
        let joined_ref = &mut joined;
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
            C::SetSigned { sketch, .. } => (*sketch, "Change Dimension in {sketch}"),
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
            C::AddEqualSpacing { sketch, .. } => (*sketch, "Equal Spacing in {sketch}"),
            C::EditEqualSpacing { sketch, .. } => (*sketch, "Edit Equal Spacing in {sketch}"),
            C::AddCode { sketch, .. } => (*sketch, "Add QR Code to {sketch}"),
            C::EditCode { sketch, .. } => (*sketch, "Edit QR Code in {sketch}"),
            C::SetActive { sketch, active, .. } => (
                *sketch,
                if *active {
                    "Activate in {sketch}"
                } else {
                    "Deactivate in {sketch}"
                },
            ),
            C::InsertSplinePoint { sketch, .. } => (*sketch, "Insert Spline Point in {sketch}"),
            C::Scale { sketch, .. } => (*sketch, "Scale {sketch}"),
            C::Create { .. }
            | C::AddRectangle { .. }
            | C::AddCircle { .. }
            | C::Project { .. }
            | C::ImportDxf { .. }
            | C::CleanUpProjections { .. }
            | C::CleanUpStaleProjections { .. }
            | C::ConvertProjected { .. }
            | C::ImportSvg { .. }
            | C::Axis { .. }
            | C::Paste { .. }
            | C::RedefinePlane(_) => {
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
                    radius,
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
                    *made_ref = match radius {
                        Some(radius) => {
                            let (lines, arcs) =
                                edit.rounded_rectangle(mode, construction, &radius)?;
                            curve_list(lines.into_iter().chain(arcs).collect())
                        }
                        None => curve_list(edit.rectangle(mode, construction)?),
                    };
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
                            from: mitcad_model::sketch::internal::point_serde::parse(&need(
                                from, "from", m,
                            )?)
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
                    closed,
                    construction,
                    ..
                } => {
                    let points = points.iter().map(point).collect::<Result<Vec<_>, _>>()?;
                    let id = edit.spline(&points, degree, closed, construction)?;
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
                    cap_height,
                    wrap,
                    line_spacing,
                    pen_align,
                    ink_top,
                    glyphs,
                    font_file,
                    value,
                    ..
                } => {
                    let mut t = SketchText {
                        glyphs,
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
                        cap_height,
                        wrap,
                        line_spacing,
                        pen_align,
                        ink_top,
                        ..SketchText::new(EntityUid(0), &text, at.unwrap_or_default(), height)
                    };
                    // A text from a text parameter (mitcad#664) needs none of
                    // its own: the parameter's replaces it below.
                    let valued =
                        matches!(&value, Some(ValueInput::Name(v)) if !v.trim().is_empty());
                    if t.text.is_empty() && valued {
                        t.text = "?".to_owned();
                    }
                    if let Some(file) = font_file.as_deref().filter(|f| !f.is_empty()) {
                        mitcad_model::sketch::text::use_font_file(&mut t, file)?;
                    }
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
                    // Its text from a text parameter (mitcad#664).
                    edit.set_text_value(id, value.as_ref())?;
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
                    cap_height,
                    wrap,
                    line_spacing,
                    pen_align,
                    ink_top,
                    font_file,
                    value,
                    ..
                } => {
                    let uid = mitcad_model::internal::ids::parse_number(&id, "t")
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
                    let font_changed = font.as_ref().is_some_and(|f| *f != t.font);
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
                    if let Some(v) = cap_height {
                        t.cap_height = v;
                    }
                    if let Some(v) = wrap {
                        t.wrap = v;
                    }
                    if let Some(v) = line_spacing {
                        t.line_spacing = v;
                    }
                    if let Some(v) = pen_align {
                        t.pen_align = v;
                    }
                    if let Some(v) = ink_top {
                        t.ink_top = v;
                    }
                    // A font file chosen, left for an installed font, or
                    // read again for a new string or style (mitcad#473).
                    match font_file.as_deref() {
                        Some("") => t.font_file = None,
                        Some(path) => mitcad_model::sketch::text::use_font_file(t, path)?,
                        None if font_changed => t.font_file = None,
                        None => {}
                    }
                    // Only a file chosen in this session is read again
                    // (mitcad#929).
                    if t.font_file.is_some() && !t.glyphs.as_ref().is_some_and(|g| g.fit(t)) {
                        mitcad_model::sketch::text::read_font_file_again(t).map_err(|e| {
                            format!(
                                "text {id}: {e}; choose the font file again for a new \
                                 string or style"
                            )
                        })?;
                    }
                    // Stored outlines of another string, font or style are
                    // dropped (mitcad#416).
                    if t.glyphs.as_ref().is_some_and(|g| !g.fit(t)) {
                        t.glyphs = None;
                    }
                    // Its text from a text parameter, or no longer
                    // (mitcad#664).
                    edit.set_text_value(uid, value.as_ref())?;
                    made_ref.push(id);
                }
                C::AddConstraint { constraint, .. } => {
                    let before = edit.solved().ok().map(|s| s.status);
                    // Two curve ends made coincident become one point
                    // (mitcad#260).
                    let merge = edit.coincident_merge(&constraint);
                    let id = edit.add_constraint(constraint);
                    if let Some((keep, drop)) = merge {
                        edit.merge_coincident(id, keep, drop)?;
                        *joined_ref = Some(json!({
                            "merged": {"kept": Ref::Point(keep), "removed": Ref::Point(drop)}
                        }));
                    }
                    if let Some(before) = before {
                        // A Symmetry of circles or arcs whose radii are
                        // fixed already mirrors their positions only
                        // (mitcad#865).
                        edit.fit_symmetric(id, &before);
                        // An older constraint the new one makes redundant
                        // goes in its favour (mitcad#287).
                        let implied = edit.drop_implied(id, &before);
                        if !implied.is_empty() {
                            let list: Vec<Value> = implied
                                .iter()
                                .map(|c| {
                                    json!({"constraint": c.id.to_string(),
                                           "type": c.kind.type_name()})
                                })
                                .collect();
                            joined_ref.get_or_insert_with(|| json!({}))["implied_removed"] =
                                json!(list);
                        }
                        edit.require_needed(before.redundant.len(), &id.to_string())?;
                    }
                }
                C::AddDimension {
                    dimension,
                    value,
                    driven,
                    text,
                    scale_sketch,
                    ..
                } => {
                    let before = edit.redundancy().ok();
                    // The first driving dimension of a sketch drawn out of
                    // proportion scales it to itself (mitcad#860): measured
                    // before it is added.
                    let first = if scale_sketch && !driven {
                        edit.first_dimension_measure(&dimension)
                    } else {
                        None
                    };
                    let id = edit.add_dimension(dimension, value.as_ref(), driven, None)?;
                    if let (Some(before), false) = (before, driven) {
                        edit.require_needed(before, &id.to_string())?;
                    }
                    let scaled = match first {
                        Some(measured) => edit.scale_to_dimension(id, measured)?,
                        None => None,
                    };
                    if let Some(factor) = scaled {
                        *joined_ref = Some(json!({"scaled": factor}));
                    }
                    if let Some(at) = text {
                        // Placed on the sketch as it was drawn: scaled with it.
                        let f = scaled.unwrap_or(1.0);
                        edit.set_text_position(id, [at[0] * f, at[1] * f])?;
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
                C::SetSigned {
                    dimension, signed, ..
                } => edit.set_signed(dimension, signed)?,
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
                    entity: None,
                    entities: Some(entities),
                    to: None,
                    by: Some(by),
                    radius: None,
                    ..
                } => {
                    // A selection dragged together (mitcad#837): every
                    // point of it pulled by the offset, a point shared by
                    // several of the entities once, in one solve.
                    if entities.is_empty() {
                        return Err("\"entities\" is empty".to_owned());
                    }
                    edit.drag_together(&entities, by)?;
                }
                C::Drag {
                    entity,
                    entities,
                    to,
                    by,
                    radius,
                    ..
                } => {
                    let single = match (entity, entities) {
                        (Some(entity), None) => entity,
                        (Some(_), Some(_)) => {
                            return Err("give \"entity\" or \"entities\", not both".to_owned());
                        }
                        (None, Some(_)) => {
                            return Err("drag \"entities\" \"by\" an offset".to_owned());
                        }
                        (None, None) => {
                            return Err("give \"entity\" or \"entities\"".to_owned());
                        }
                    };
                    let target = match (single, to, by, radius) {
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
                C::Extend { curve, at, .. } => {
                    if let Some(p) = edit.extend(curve, at)? {
                        *joined_ref = Some(json!({"joined": Ref::Point(p)}));
                    }
                }
                C::Fillet {
                    a,
                    b,
                    curves,
                    points,
                    radius,
                    ..
                } => match (a, b, curves, points) {
                    (Some(a), Some(b), None, None) => {
                        let arc = edit.fillet(a, b, &radius)?;
                        made_ref.push(Ref::Curve(arc).to_string());
                    }
                    (None, None, curves, points) if curves.is_some() || points.is_some() => {
                        let arcs = edit.fillet_corners(
                            &curves.unwrap_or_default(),
                            &points.unwrap_or_default(),
                            &radius,
                        )?;
                        *made_ref = curve_list(arcs);
                    }
                    _ => {
                        return Err(
                            "a fillet takes \"a\" and \"b\", or \"curves\" and/or \"points\""
                                .to_owned(),
                        );
                    }
                },
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
                    let axis = mirror_axis(edit, &axis)?;
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
                    symmetric,
                    ..
                } => {
                    let id = edit.rectangular_pattern(
                        &entities,
                        direction,
                        count,
                        [&spacing[0], &spacing[1]],
                        symmetric,
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
                C::AddEqualSpacing {
                    anchors,
                    mode,
                    direction,
                    keep_order,
                    dimension,
                    spacing,
                    driven,
                    text,
                    ..
                } => {
                    let input = SpacingInput {
                        anchors,
                        mode,
                        direction,
                        keep_order,
                    };
                    let dimension = match (dimension || spacing.is_some() || driven, driven) {
                        (false, _) => SpacingDimension::None,
                        (true, true) => SpacingDimension::Driven,
                        (true, false) => SpacingDimension::Driving(spacing),
                    };
                    // Refused, not added in part, when it or its dimension
                    // is implied by the others or contradicts them.
                    let before = edit.redundancy().ok();
                    let (id, dim) = edit.add_equal_spacing(&input, &dimension)?;
                    if let Some(before) = before {
                        edit.require_spacing_needed(before, id)?;
                    }
                    if let (Some(dim), Some(at)) = (dim, text) {
                        edit.set_text_position(dim, at)?;
                    }
                    made_ref.push(id.to_string());
                }
                C::EditEqualSpacing {
                    constraint,
                    anchors,
                    mode,
                    direction,
                    keep_order,
                    ..
                } => {
                    let before = edit.redundancy().ok();
                    edit.edit_equal_spacing(constraint, anchors, mode, direction, keep_order)?;
                    if let Some(before) = before {
                        edit.require_spacing_needed(before, constraint)?;
                    }
                    made_ref.push(constraint.to_string());
                }
                C::AddCode {
                    text,
                    at,
                    module,
                    angle,
                    kind,
                    ecc,
                    ..
                } => {
                    let t = SketchText {
                        angle,
                        code: Some(mitcad_model::sketch::SketchCode { kind, ecc }),
                        ..SketchText::new(EntityUid(0), &text, at, module)
                    };
                    code_fits(&t)?;
                    let id = edit.text(t)?;
                    made_ref.push(format!("t{}", id.0));
                }
                C::EditCode {
                    id,
                    text,
                    at,
                    module,
                    angle,
                    ecc,
                    ..
                } => {
                    let uid = mitcad_model::internal::ids::parse_number(&id, "t")
                        .map(EntityUid)
                        .ok_or_else(|| format!("'{id}' is not a code like \"t5\""))?;
                    let t = edit
                        .def
                        .texts
                        .iter_mut()
                        .find(|t| t.id == uid && t.code.is_some())
                        .ok_or_else(|| format!("code {id} does not exist"))?;
                    if let Some(v) = text {
                        t.text = v;
                    }
                    if let Some(v) = at {
                        t.at = v;
                    }
                    if let Some(v) = module {
                        t.height = v;
                    }
                    if let Some(v) = angle {
                        t.angle = v;
                    }
                    if let (Some(v), Some(code)) = (ecc, t.code.as_mut()) {
                        code.ecc = v;
                    }
                    code_fits(t)?;
                    made_ref.push(id);
                }
                C::SetActive {
                    items,
                    active,
                    reason,
                    ..
                } => {
                    // Activated only where the sketch then solves without
                    // over-constraining it (mitcad#721).
                    let before = edit.redundancy().ok();
                    let changed = edit.set_active(&items, active, reason.as_deref())?;
                    if active && !changed.is_empty() {
                        let names: Vec<String> = changed.iter().map(ToString::to_string).collect();
                        let what = names.join(", ");
                        edit.solved()
                            .map_err(|e| format!("{what} cannot be activated: {e}"))?;
                        if let Some(before) = before {
                            edit.require_needed(before, &what)
                                .map_err(|e| format!("{what} cannot be activated: {e}"))?;
                        }
                    }
                    made_ref.extend(changed.iter().map(ToString::to_string));
                }
                C::InsertSplinePoint { curve, at, .. } => {
                    let p = edit.insert_spline_point(curve, at)?;
                    made_ref.push(Ref::Point(p).to_string());
                }
                C::Scale { factor, center, .. } => {
                    let center = match center.as_ref().map(point).transpose()? {
                        None => [0.0, 0.0],
                        Some(PointInput::At(at)) => at,
                        Some(PointInput::Existing(p)) => edit.at(p)?,
                    };
                    edit.scale(factor, center)?;
                }
                C::Create { .. }
                | C::AddRectangle { .. }
                | C::AddCircle { .. }
                | C::Project { .. }
                | C::ImportDxf { .. }
                | C::CleanUpProjections { .. }
                | C::CleanUpStaleProjections { .. }
                | C::ConvertProjected { .. }
                | C::ImportSvg { .. }
                | C::Axis { .. }
                | C::Paste { .. }
                | C::RedefinePlane(_) => {
                    unreachable!("handled above")
                }
            }
            Ok(())
        })?;
        let mut value = report_json(&report, self.sketch_def(sketch));
        if !made.is_empty() {
            value["made"] = json!(made);
        }
        if let Some(Value::Object(fields)) = joined {
            for (key, field) in fields {
                value[key.as_str()] = field;
            }
        }
        Ok(value)
    }

    fn clean_up_projections(&mut self, sketch: FeatureUid, merge: bool) -> Result<Value, ApiError> {
        let regions = |doc: &Self| -> Vec<String> {
            doc.sketch_output(sketch)
                .map(|o| o.regions.iter().map(|r| r.key.to_string()).collect())
                .unwrap_or_default()
        };
        let before = regions(self);
        let (removed, report) =
            self.edit_sketch(sketch, "Clean Up Projections in {sketch}", |edit| {
                edit.clean_up_projections(merge)
            })?;
        let after = regions(self);
        let curve = |id: EntityUid| Ref::Curve(id).to_string();
        let mut warnings = Vec::new();
        for (how, what, word) in [
            (Removal::Same, "the same as another", "as"),
            (Removal::On, "on a longer line", "on"),
            (Removal::Joined, "joined to a line it overlaps", "into"),
        ] {
            let list: Vec<String> = removed
                .iter()
                .filter(|r| r.how == how)
                .map(|r| format!("{} ({word} {})", curve(r.curve), curve(r.kept)))
                .collect();
            if !list.is_empty() {
                warnings.push(format!(
                    "{} {what} removed: {}",
                    if list.len() == 1 { "1 curve" } else { "curves" },
                    list.join(", ")
                ));
            }
        }
        for r in &removed {
            if let Some(source) = &r.link {
                warnings.push(format!(
                    "{} followed {source}; {} in its place does not (a merged curve follows \
                     one source and is not split again when the sources differ)",
                    curve(r.curve),
                    curve(r.kept)
                ));
            }
        }
        // Profile regions whose names change, where later features use them.
        let changed: Vec<&String> = before.iter().filter(|k| !after.contains(k)).collect();
        if !changed.is_empty() {
            for entry in self.features().skip_while(|e| e.uid != sketch).skip(1) {
                let params = self.parameters();
                let text = entry
                    .def
                    .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                    .ok()
                    .and_then(|def| serde_json::to_string(&def).ok())
                    .unwrap_or_default();
                for key in changed.iter().filter(|k| text.contains(k.as_str())) {
                    warnings.push(format!(
                        "{} uses the region {key}, whose name changes: check it",
                        entry.name
                    ));
                }
            }
        }
        let cleaned: Vec<Value> = removed
            .iter()
            .map(|r| {
                json!({"curve": curve(r.curve), "kept": curve(r.kept), "how": r.how.name(),
                       "link": r.link})
            })
            .collect();
        let mut value = report_json(&report, self.sketch_def(sketch));
        value["cleaned"] = json!(cleaned);
        value["warnings"] = json!(warnings);
        Ok(value)
    }

    fn clean_up_stale_projections(&mut self, sketch: FeatureUid) -> Result<Value, ApiError> {
        let output = self.sketch_output(sketch).ok_or_else(|| {
            ApiError(format!(
                "{sketch} has not evaluated: which of its projected curves are stale is not known"
            ))
        })?;
        let follow = output.follow.to_vec();
        let texts = output.texts.to_vec();
        let before: Vec<String> = output.regions.iter().map(|r| r.key.to_string()).collect();
        let used = self.used_regions(sketch);
        let (cleaned, report) =
            self.edit_sketch(sketch, "Clean Up Stale Projections in {sketch}", |edit| {
                edit.clean_up_stale_projections(&follow, &used, &texts)
            })?;
        let curves = |ids: &[EntityUid]| -> Vec<String> {
            ids.iter().map(|id| Ref::Curve(*id).to_string()).collect()
        };
        let counted = |n: usize, what: &str| {
            if n == 1 {
                format!("1 stale {what}")
            } else {
                format!("{n} stale {what}s")
            }
        };
        let mut warnings = Vec::new();
        if !cleaned.removed.is_empty() {
            warnings.push(format!(
                "{} removed: {}",
                counted(cleaned.removed.len(), "curve"),
                curves(&cleaned.removed).join(", ")
            ));
        }
        if !cleaned.trimmed.is_empty() {
            warnings.push(format!(
                "{} trimmed to the regions later features use: {}",
                counted(cleaned.trimmed.len(), "line"),
                curves(&cleaned.trimmed).join(", ")
            ));
        }
        if !cleaned.unlinked.is_empty() {
            warnings.push(format!(
                "unlinked, their curves kept as fixed references: the projection{} of {}",
                if cleaned.unlinked.len() == 1 { "" } else { "s" },
                cleaned.unlinked.join(", ")
            ));
        }
        for why in [Keep::Region, Keep::UserCurve, Keep::Constrained, Keep::Used] {
            let ids: Vec<EntityUid> = cleaned
                .kept
                .iter()
                .filter(|(_, w)| *w == why)
                .map(|(id, _)| *id)
                .collect();
            if !ids.is_empty() {
                warnings.push(format!(
                    "{} kept, {}: {}",
                    counted(ids.len(), "curve"),
                    why.words(),
                    curves(&ids).join(", ")
                ));
            }
        }
        warnings.extend(self.renamed_regions(sketch, &before));
        let kept: Vec<Value> = cleaned
            .kept
            .iter()
            .map(|(id, why)| json!({"curve": Ref::Curve(*id).to_string(), "why": why.name()}))
            .collect();
        // What a preview shows: the removed curves as they were, and the
        // pieces trimmed off (mitcad#252).
        let removed_geometry: Vec<Value> = cleaned
            .removed_geometry
            .iter()
            .map(|(id, kind, curve)| {
                json!({"curve": Ref::Curve(*id).to_string(), "type": kind,
                       "geometry": curve_json(curve)})
            })
            .collect();
        let cut: Vec<Value> = cleaned
            .cut
            .iter()
            .map(|(id, from, to)| {
                json!({"line": Ref::Curve(*id).to_string(), "start": from, "end": to})
            })
            .collect();
        let mut value = report_json(&report, self.sketch_def(sketch));
        value["stale"] = json!({
            "removed": curves(&cleaned.removed),
            "trimmed": curves(&cleaned.trimmed),
            "unlinked": cleaned.unlinked,
            "kept": kept,
            "preview": {"removed": removed_geometry, "cut": cut},
        });
        value["warnings"] = json!(warnings);
        Ok(value)
    }

    fn sketch_axis(&mut self, sketch: FeatureUid, axis: &str) -> Result<Value, ApiError> {
        use mitcad_model::sketch::axes::{AxisPick, existing};
        let pick = AxisPick::parse(axis).map_err(ApiError)?;
        let def = self
            .sketch_def(sketch)
            .ok_or_else(|| ApiError(format!("{sketch} is not a sketch")))?;
        if let Some(id) = existing(def, pick) {
            let entity = def.entity(id).expect("found").as_ref();
            return Ok(json!({"entity": entity, "new": false}));
        }
        let (id, report) =
            self.edit_sketch(sketch, "Axis in {sketch}", |edit| Ok(edit.axis_pick(pick)))?;
        let entity = self
            .sketch_def(sketch)
            .and_then(|def| def.entity(id))
            .map(|e| e.as_ref().to_string())
            .unwrap_or_default();
        let mut value = report_json(&report, self.sketch_def(sketch));
        value["entity"] = json!(entity);
        value["new"] = json!(true);
        Ok(value)
    }

    fn convert_projected(
        &mut self,
        sketch: FeatureUid,
        entities: &[Ref],
    ) -> Result<Value, ApiError> {
        let (done, report) =
            self.edit_sketch(sketch, "Convert to Sketch Geometry in {sketch}", |edit| {
                edit.convert_projected(entities)
            })?;
        let points = |ids: &[EntityUid]| -> Vec<String> {
            ids.iter().map(|p| Ref::Point(*p).to_string()).collect()
        };
        let mut warnings = Vec::new();
        if !done.kept.is_empty() {
            warnings.push(format!(
                "projected curves that stay use {}: {} projected",
                points(&done.kept).join(", "),
                if done.kept.len() == 1 {
                    "it stays"
                } else {
                    "they stay"
                }
            ));
        }
        if !done.ignored.is_empty() {
            let names: Vec<String> = done.ignored.iter().map(ToString::to_string).collect();
            warnings.push(format!(
                "not projected geometry, left as it is: {}",
                names.join(", ")
            ));
        }
        for source in &done.unfollowing {
            warnings.push(format!(
                "the projection of {source} follows only a source like the one it was made from \
                 (an import's, or of a file of an earlier version): with some of its entities \
                 converted it keeps its geometry from now on"
            ));
        }
        let mut value = report_json(&report, self.sketch_def(sketch));
        value["converted"] = names(&done.converted);
        value["kept"] = json!(points(&done.kept));
        value["warnings"] = json!(warnings);
        Ok(value)
    }

    fn later_definitions(&self, sketch: FeatureUid) -> Vec<(String, String)> {
        let params = self.parameters();
        self.features()
            .skip_while(|e| e.uid != sketch)
            .skip(1)
            .map(|entry| {
                let text = entry
                    .def
                    .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                    .ok()
                    .and_then(|def| serde_json::to_string(&def).ok())
                    .unwrap_or_default();
                (entry.name.clone(), text)
            })
            .collect()
    }

    fn used_regions(&self, sketch: FeatureUid) -> Vec<Region> {
        let Some(output) = self.sketch_output(sketch) else {
            return Vec::new();
        };
        let mut keys: BTreeSet<RegionKey> = BTreeSet::new();
        for entry in self.features().skip_while(|e| e.uid != sketch).skip(1) {
            for p in entry.def.info().profiles() {
                if p.sketch == sketch
                    && let Some(found) = output.found(&p.region)
                {
                    keys.extend(found.regions.iter().map(|r| r.profile.key.clone()));
                }
            }
        }
        let named: Vec<(String, &RegionKey)> = output
            .regions
            .iter()
            .map(|r| (r.key.to_string(), &r.key))
            .collect();
        for (_, text) in self.later_definitions(sketch) {
            for (name, key) in &named {
                if text.contains(name.as_str()) {
                    keys.insert((*key).clone());
                }
            }
        }
        output
            .region_info
            .iter()
            .filter(|r| keys.contains(&r.profile.key))
            .cloned()
            .collect()
    }

    fn renamed_regions(&self, sketch: FeatureUid, before: &[String]) -> Vec<String> {
        let after: Vec<String> = self
            .sketch_output(sketch)
            .map(|o| o.regions.iter().map(|r| r.key.to_string()).collect())
            .unwrap_or_default();
        let changed: Vec<&String> = before.iter().filter(|k| !after.contains(k)).collect();
        let mut warnings = Vec::new();
        if changed.is_empty() {
            return warnings;
        }
        for (name, text) in self.later_definitions(sketch) {
            for key in changed.iter().filter(|k| text.contains(k.as_str())) {
                warnings.push(format!(
                    "{name} uses the region {key}, whose name changes: check it"
                ));
            }
        }
        warnings
    }

    fn sketch_def(&self, uid: FeatureUid) -> Option<&SketchDef> {
        match &self.feature(uid)?.def {
            FeatureDef::Sketch(def) => Some(def),
            _ => None,
        }
    }

    fn stand_upright(
        &mut self,
        uid: FeatureUid,
        mut def: SketchDef<ValueInput>,
        occurrence: &Option<OccurrencePath>,
        context: &Option<OccurrencePath>,
        depth: usize,
        name: &str,
    ) -> Result<(), ApiError> {
        // A sketch whose plane is not found has no frame: its error says why.
        let Some(plane) = self.sketch_output(uid).map(|output| output.frame) else {
            return Ok(());
        };
        let shown = context.as_ref().or(occurrence.as_ref()).map(|p| &p.0[..]);
        let to_component = mitcad_model::internal::links::first_path(
            self.assembly(),
            self.active_component(),
            shown,
        )
        .map(|path| mitcad_model::assembly::inverse(&self.path_transform(&path)))
        .unwrap_or(mitcad_model::transform::Transform::IDENTITY);
        let axis = |v: [f64; 3]| to_component.apply_vector(v);
        let Some(own) = FrameDef::upright(&plane, axis([0.0, 0.0, 1.0]), axis([0.0, 1.0, 0.0]))
        else {
            return Ok(());
        };
        def.frame = Some(Box::new(own));
        self.edit_feature(uid, &FeatureDef::Sketch(def))?;
        self.merge_undo(depth, &format!("Add {name}"));
        Ok(())
    }
}

/// The entities a linked projection's warning is about (mitcad#264): all
/// of its curves and points when it keeps its geometry as a whole (`kept`,
/// `lost`), else its stale curves when it has any, else those that found
/// nothing like them in the source (`unfollowed`).
fn projection_warning_entities(
    report: &mitcad_model::sketch::project::ProjectionFollow,
    projection: &mitcad_model::sketch::Projection,
) -> Vec<String> {
    if report.kept || report.lost {
        return projection
            .entities
            .iter()
            .map(ToString::to_string)
            .collect();
    }
    if !report.stale.is_empty() {
        return report
            .stale
            .iter()
            .map(|id| Ref::Curve(*id).to_string())
            .collect();
    }
    report
        .unfollowed
        .iter()
        .filter_map(|id| projection.entities.iter().find(|r| r.uid() == *id))
        .map(ToString::to_string)
        .collect()
}

fn shape_added(added: &mitcad_model::ShapeAdded, def: Option<&SketchDef>) -> Value {
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

// Body edges and vertices in a sketch's plane (mitcad#256, mitcad#258).

/// The type a curve gets as a sketch entity (the `sketch` query's `type`).
fn curve_type(curve: &Curve2) -> &'static str {
    match curve {
        Curve2::Line { .. } => "line",
        Curve2::Circle { .. } => "circle",
        Curve2::Arc { .. } => "arc",
        Curve2::Ellipse { .. } => "ellipse",
        Curve2::EllipticalArc { .. } => "elliptical_arc",
        Curve2::Nurbs(_) => "spline",
    }
}

/// The curves a solve left collapsed, as ids (mitcad#365).
fn degenerate_ids(status: &SketchStatus) -> Value {
    json!(
        status
            .degenerate
            .iter()
            .map(|id| Ref::Curve(*id).to_string())
            .collect::<Vec<_>>()
    )
}
