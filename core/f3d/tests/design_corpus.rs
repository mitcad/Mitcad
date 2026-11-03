// SPDX-License-Identifier: MIT
//! Design stream decoder on the local .f3d corpus (`MITCAD_F3D_CORPUS`,
//! default `~/f3d-corpus`; skipped when missing).
//!
//! - Every design segment parses: the whole MetaStream, every object tag is
//!   256 + its class index.
//! - The dump IR equals, field by field, the reference decoder's JSON (one
//!   file per corpus design, written by the separate Python decoder of the
//!   format study, which is not part of this repository), read from
//!   `MITCAD_F3D_DESIGN_REF` (default `~/f3d-design-ref`; this part is
//!   skipped when the directory is missing). The light bulbs, which only
//!   this decoder reads, are left out ([`DECODER_ONLY`]).
//! - Every parameter expression evaluated with `mitcad_model::expr::f3d`
//!   equals the stored value.
//!
//! Run with `--nocapture` for the agreement report.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use mitcad_expr::{LengthUnit, Quantity, f3d};
use mitcad_f3d::design::{self, Design, ir};
use serde_json::Value;

fn home_dir(var: &str, default: &str) -> Option<PathBuf> {
    let dir = match std::env::var_os(var) {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME")?).join(default),
    };
    dir.is_dir().then_some(dir)
}

/// `.f3d`/`.f3z` files under `dir`, sorted by path text (the reference
/// decoder's order, which numbers its JSON files).
fn corpus_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                let name = p.to_string_lossy().to_lowercase();
                if name.ends_with(".f3d") || name.ends_with(".f3z") {
                    out.push(p);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort_by_key(|p| p.to_string_lossy().into_owned());
    out
}

/// Field-level comparison of two JSON values.
#[derive(Default)]
struct Agreement {
    equal: usize,
    differ: usize,
    /// In the reference only.
    missing: usize,
    /// In the Rust dump only.
    extra: usize,
    /// Field path (indices as `[]`) -> (equal, not equal).
    by_field: BTreeMap<String, (usize, usize)>,
    examples: Vec<String>,
}

const IGNORED: &[&str] = &[
    ".generator.decoder",
    // A split's construction plane that a later item deleted: its geometry
    // is kept in the file and read as the plane (the API reference dumps
    // agree); the reference decoder named it a face without geometry.
    ".timeline.items[].detail.referencePlane.kind",
    ".timeline.items[].detail.referencePlane.name",
    ".timeline.items[].detail.referencePlane.origin",
    ".timeline.items[].detail.referencePlane.timeline_index",
    ".timeline.items[].detail.referencePlane.geometry",
    // Where this decoder reads what the reference decoder did not, checked
    // against the API reference dumps (`tools/f3d-learn/apiref.py`): the
    // names users gave sketches (the sketch object holds them, the item's
    // tail does not) and the sketches' names where they are referred to, the
    // default names of unnamed components (the reference took their
    // material), and a text parameter's comment and number.
    ".timeline.items[].name",
    ".timeline.items[].detail.name",
    ".timeline.items[].detail.profile[].sketch",
    ".timeline.items[].detail.profile[].sketch_timeline_index",
    ".timeline.items[].detail.referencePlane.sketch",
    ".parameters.model[].createdBy.name",
    ".parameters.model[].createdBy.sketch",
    ".parameters.model[].createdBy.sketch_timeline_index",
    ".components[].name",
    ".occurrences[].component",
    ".parameters.model[].value",
    ".parameters.model[].comment",
];

/// The reference JSON predates the dump format's own names (mitcad#26):
/// its `schema` name and the build key of `_f3d.format` (the one key there
/// ending in `_build`) are given the current names before the comparison.
/// The reference decoder read the lines of the sketch-line subclasses at
/// the base class's place, which gave denormal and huge numbers
/// (mitcad#178): where a reference line has such coordinates, this
/// decoder's sketch is taken (its curves, the points' curve lists and the
/// design's count of non-finite numbers).
fn line_subclasses(current: &Value, reference: Value) -> Value {
    own_sketches(
        current,
        faulty_lines(current, false_tails(current, reference)),
    )
}

/// Item tails the reference decoder found 20 bytes too late (mitcad#719):
/// the zeros before a reference in the sub-items read as an empty base
/// name, the index 0 or 256 and no sub-items, and the `u32` before the
/// base name as a result number below -1, which no item has (-1 is none).
/// The `u32` after the index is zero on every tail, which rules such a
/// candidate out here: where the reference's result number is below -1,
/// this decoder's tail is taken (base name, index, result number,
/// sub-items) and with it its count of decoded bytes.
fn false_tails(current: &Value, mut reference: Value) -> Value {
    let Some(items) = reference
        .pointer_mut("/timeline/items")
        .and_then(Value::as_array_mut)
    else {
        return reference;
    };
    let mut any = false;
    for (i, it) in items.iter_mut().enumerate() {
        let false_tail = it["_f3d"]["result_no"].as_i64().is_some_and(|n| n < -1);
        let ours = &current["timeline"]["items"][i]["_f3d"];
        if !false_tail || ours.is_null() {
            continue;
        }
        any = true;
        for k in ["base_name", "index", "result_no", "sub_features"] {
            if !ours[k].is_null() {
                it["_f3d"][k] = ours[k].clone();
            }
        }
    }
    if any && let Some(bytes) = current.pointer("/_f3d/coverage/decoded_bytes") {
        reference["_f3d"]["coverage"]["decoded_bytes"] = bytes.clone();
    }
    reference
}

/// Sketches this decoder reads differently from the reference decoder
/// (mitcad#186): ones it links to their feature through a recipe object (the
/// reference gives an empty detail), and ones with a fitted spline, whose
/// handle lines are no curves here (the API lists none) and are counted in
/// the reference's ids: this decoder's sketch is taken (counts, curves, points,
/// constraints, dimensions and the frame).
fn own_sketches(current: &Value, mut reference: Value) -> Value {
    let Some(items) = reference
        .pointer_mut("/timeline/items")
        .and_then(Value::as_array_mut)
    else {
        return reference;
    };
    for (i, it) in items.iter_mut().enumerate() {
        let ours = &current["timeline"]["items"][i]["detail"];
        let Some(curves) = ours["curves"].as_array() else {
            continue;
        };
        let missing = it["detail"]["counts"].is_null();
        let handles = curves.iter().any(|c| c["type"] == "SketchFittedSpline")
            && it["detail"]["counts"]["curves"] != ours["counts"]["curves"];
        if !(missing || handles) {
            continue;
        }
        for k in [
            "name",
            "origin",
            "xDirection",
            "yDirection",
            "referencePlane",
            "model_frame",
            "counts",
            "curves",
            "points",
            "constraints",
            "dimensions",
        ] {
            if !ours[k].is_null() {
                it["detail"][k] = ours[k].clone();
            }
        }
    }
    reference
}

fn faulty_lines(current: &Value, mut reference: Value) -> Value {
    let odd = |v: &Value| {
        v.as_f64()
            .is_some_and(|x| x != 0.0 && (x.abs() < 1e-200 || x.abs() > 1e30))
    };
    let faulty = |c: &Value| {
        ["startPoint", "endPoint"].iter().any(|k| {
            c["geometry"][*k]
                .as_array()
                .is_some_and(|p| p.iter().any(odd))
        })
    };
    let Some(items) = reference
        .pointer_mut("/timeline/items")
        .and_then(Value::as_array_mut)
    else {
        return reference;
    };
    let mut any = false;
    for (i, it) in items.iter_mut().enumerate() {
        let bad = it["detail"]["curves"]
            .as_array()
            .is_some_and(|cs| cs.iter().any(faulty));
        if !bad {
            continue;
        }
        any = true;
        let ours = &current["timeline"]["items"][i]["detail"];
        for k in ["curves", "points"] {
            if !ours[k].is_null() {
                it["detail"][k] = ours[k].clone();
            }
        }
    }
    if any && let Some(f) = reference.get_mut("_f3d").and_then(Value::as_object_mut) {
        match current.pointer("/_f3d/non_finite") {
            Some(n) => f.insert("non_finite".to_owned(), n.clone()),
            None => f.remove("non_finite"),
        };
    }
    reference
}

/// The names this decoder shows for construction planes, axes and joint
/// origins (`Plane1`, `Axis1`, `Joint Origin1`) as the reference decoder
/// keeps them, over the file's internal bases (`WorkPlane1`, `WorkAxis1`,
/// `JointOrigin1`): in the items and in every reference to them.
fn internal_names(v: &mut Value) {
    match v {
        Value::Object(m) => {
            let kind = m.get("kind").and_then(Value::as_str).unwrap_or("");
            let typ = m.get("objectType").and_then(Value::as_str).unwrap_or("");
            let (shown, internal) = if kind == "construction_plane" || typ == "ConstructionPlane" {
                ("Plane", "WorkPlane")
            } else if kind == "construction_axis" || typ == "ConstructionAxis" {
                ("Axis", "WorkAxis")
            } else if typ == "JointOrigin" {
                ("Joint Origin", "JointOrigin")
            } else {
                ("", "")
            };
            if !shown.is_empty()
                && let Some(Value::String(name)) = m.get_mut("name")
                && let Some(n) = name.strip_prefix(shown)
                && !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
            {
                *name = format!("{internal}{n}");
            }
            for x in m.values_mut() {
                internal_names(x);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(internal_names),
        _ => {}
    }
}

fn current_names(reference: &Value) -> Value {
    let mut r = reference.clone();
    if let Some(format) = r.pointer_mut("/_f3d/format").and_then(Value::as_object_mut) {
        let old: Vec<String> = format
            .keys()
            .filter(|k| k.ends_with("_build") && k.as_str() != "writer_build")
            .cloned()
            .collect();
        for key in old {
            if let Some(build) = format.remove(&key) {
                format.insert("writer_build".to_owned(), build);
            }
        }
    }
    if r.get("schema").is_some() {
        r["schema"] = Value::from(ir::SCHEMA_NAME);
    }
    // A text parameter has no number, and its default expression (`1 mm`)
    // is no comment (the API reference dumps); the reference decoder gave
    // it a zero and that comment.
    for list in ["user", "model"] {
        if let Some(params) = r
            .pointer_mut(&format!("/parameters/{list}"))
            .and_then(Value::as_array_mut)
        {
            for p in params.iter_mut().filter(|p| p["unit"] == "Text") {
                if let Some(o) = p.as_object_mut() {
                    o.remove("value");
                    o.remove("comment");
                }
            }
        }
    }
    r
}

/// Fields this decoder writes beyond the reference decoder: the light
/// bulbs of sketches and construction planes (mitcad#6), checked against
/// the dumps of the reference models instead (`design_models.rs`), and an
/// extrusion's direction vector.
const DECODER_ONLY: &[&str] = &[
    // The appearances and materials (mitcad#228), which the reference
    // decoder did not read.
    "._f3d.appearance",
    // The external documents of mitcad#196 (their keys, GUIDs and versions,
    // and the occurrences' GUIDs that name levels inside them) and a sweep's
    // orientation (the controlled variant models), which the reference
    // decoder did not read.
    "._f3d.external_documents[].guids",
    "._f3d.external_documents[].document_guid",
    "._f3d.external_documents[].version",
    ".occurrences[]._f3d.guids",
    ".occurrences[].children[]._f3d.guids",
    ".occurrences[].children[].children[]._f3d.guids",
    ".timeline.items[].detail.orientation",
    // The light bulbs of hidden occurrences, checked against the API
    // reference dumps instead (core/import/docs/f3d-appearances.md, *Light bulbs*).
    ".occurrences[].isLightBulbOn",
    ".occurrences[].children[].isLightBulbOn",
    ".occurrences[].children[].children[].isLightBulbOn",
    // A coil's choices, plane and placement, and a pipe's section and
    // hollow flag (mitcad#25); the reference decoder read none of them.
    ".timeline.items[].detail.coilType",
    ".timeline.items[].detail.sectionType",
    ".timeline.items[].detail.sectionPosition",
    ".timeline.items[].detail.isClockwise",
    ".timeline.items[].detail.isHollow",
    ".timeline.items[].detail.plane",
    ".timeline.items[].detail.frame",
    ".timeline.items[].detail.placement",
    // A move's definition (rotate by its angle parameter, translate,
    // free), which the reference decoder did not read; checked against the
    // API reference dumps.
    ".timeline.items[].detail.definition",
    // The inputs of removals, deleted faces and body copies, which the
    // reference decoder did not read.
    ".timeline.items[].detail.deletedFaces",
    ".timeline.items[].detail.sourceBody",
    ".timeline.items[].detail.itemToRemove",
    // The deleted plane's geometry (see IGNORED).
    ".timeline.items[].detail.referencePlane.geometry",
    ".timeline.items[].detail.referencePlane.name",
    ".timeline.items[].detail.referencePlane.origin",
    ".timeline.items[].detail.referencePlane.timeline_index",
    // Parameters' owners (counts and texts have holders of their own
    // classes, features outside the timeline are named by their tails),
    // favorites, and the sketches whose item refers to the sketch only: the
    // reference decoder read none of them (the API reference dumps).
    ".parameters.model[].createdBy",
    ".parameters.model[].isFavorite",
    ".timeline.items[].detail.name",
    ".parameters.user[].isFavorite",
    ".timeline.items[].detail.dimensions",
    ".timeline.items[].detail.constraints",
    ".timeline.items[].detail.curves",
    ".timeline.items[].detail.points",
    ".timeline.items[].detail.model_frame",
    ".timeline.items[].detail.counts",
    ".timeline.items[].detail.origin",
    ".timeline.items[].detail.xDirection",
    ".timeline.items[].detail.yDirection",
    ".timeline.items[].detail.referencePlane",
    ".timeline.items[].props",
    ".timeline.items[]._f3d.light_bulb",
    // Fillet and chamfer edges by their names (mitcad#33), checked against
    // the reference models' dumps (`design_models.rs`).
    ".timeline.items[].detail.edgeSets[].edges",
    // Construction and centre lines (mitcad#33), likewise.
    ".timeline.items[].detail.curves[].isConstruction",
    ".timeline.items[].detail.curves[].isCenterLine",
    // Driven sketch dimensions (mitcad#33).
    ".timeline.items[].detail.dimensions[].isDriving",
    // Bodies of moves, splits, patterns and mirrors, a move's transform and
    // a split's plane (mitcad#33).
    ".timeline.items[].detail.inputEntities",
    ".timeline.items[].detail.transform",
    ".timeline.items[].detail.splitBodies",
    ".timeline.items[].detail.splittingTool",
    // An extrusion's direction vector (mitcad#42).
    ".timeline.items[]._f3d.extrude.direction_vector",
    // An extrusion's flag, input slots and the object of an extent up to
    // an object (mitcad#96), checked against the reference models' dumps
    // (`design_models.rs`).
    ".timeline.items[]._f3d.extrude.flag",
    ".timeline.items[]._f3d.extrude.slot_roles",
    ".timeline.items[]._f3d.extrude.full_length",
    ".timeline.items[].detail.extentOne.entity",
    // An extrusion's thin-wall block (mitcad#186), checked against the
    // reference models' dumps (`design_models.rs`).
    ".timeline.items[]._f3d.extrude.thin",
    ".timeline.items[]._f3d.extrude.thin_location",
    ".timeline.items[]._f3d.extrude.thin_holder",
    ".timeline.items[].detail.isThinExtrude",
    ".timeline.items[].detail.thinExtrudeWallLocationOne",
    ".timeline.items[].detail.thinExtrudeWallThicknessOne",
    // Revolutions' flags, shells' options and holes' position definitions
    // (the API reference dumps).
    ".timeline.items[].detail.holePositionDefinition",
    ".timeline.items[].detail.isProjectAxis",
    ".timeline.items[].detail.extentDefinition.isSymmetric",
    ".timeline.items[].detail.isTangentChain",
    ".timeline.items[].detail.shellType",
    // Offset and pattern constraints' values and curves (mitcad#36).
    ".timeline.items[].detail.constraints[].props",
    // Threads, their faces and tapped holes' threads (mitcad#35).
    ".timeline.items[].detail.threadInfo",
    ".timeline.items[].detail.isModeled",
    ".timeline.items[].detail.isFullLength",
    ".timeline.items[].detail.inputCylindricalFaces",
    ".timeline.items[].detail.threadLocation",
    ".timeline.items[].detail.holeTapType",
    ".timeline.items[].detail.tappedHoleInfo",
    ".timeline.items[].detail.thread",
    // Types of the timeline items the reference decoder does not know, and
    // the names of timeline groups (mitcad#43).
    ".timeline.items[].objectType",
    ".parameters.model[].createdBy.objectType",
    ".timeline.items[].name",
    // The component that owns each item (mitcad#37), checked against the
    // reference models' dumps (`design_models.rs`).
    ".timeline.items[].component",
    ".timeline.items[]._f3d.component",
    // Inputs of sweeps, pipes and lofts (mitcad#34), checked against the
    // reference models' dumps (`design_models.rs`).
    ".timeline.items[].detail.operation",
    ".timeline.items[].detail.profile",
    ".timeline.items[].detail.path",
    ".timeline.items[].detail.guideRail",
    ".timeline.items[].detail.guideSurfaces",
    ".timeline.items[].detail.loftSections",
    ".timeline.items[].detail.centerLineOrRails",
    ".timeline.items[].detail.centerLineOrRails.isCenterLine",
    // What joints, as-built joints, joint origins and ground items say
    // (mitcad#66); the reference models have no joints.
    ".timeline.items[].detail._f3d",
    ".timeline.items[].detail.geometryOrOriginOne",
    ".timeline.items[].detail.geometryOrOriginTwo",
    ".timeline.items[].detail.geometry",
    ".timeline.items[].detail.occurrenceOne",
    ".timeline.items[].detail.occurrenceTwo",
    ".timeline.items[].detail.occurrence",
    ".timeline.items[].detail.jointMotion",
    ".timeline.items[].detail.angle",
    ".timeline.items[].detail.offset",
    ".timeline.items[].detail.offsetX",
    ".timeline.items[].detail.offsetY",
    ".timeline.items[].detail.isFlipped",
    ".occurrences[].isGrounded",
    // Combines, splits, patterns, mirrors and holes (mitcad#67), checked
    // against the reference models' dumps (`design_models.rs`).
    ".timeline.items[].detail.targetBody",
    ".timeline.items[].detail.toolBodies",
    // A pattern's quantities by their parameters (mitcad#641), which the
    // reference decoder gave as values only.
    ".timeline.items[].detail.quantity.name",
    ".timeline.items[].detail.quantityOne.name",
    ".timeline.items[].detail.quantityTwo.name",
    ".timeline.items[].detail.isKeepToolBodies",
    ".timeline.items[].detail.isSplittingToolExtended",
    ".timeline.items[].detail.patternEntityType",
    ".timeline.items[].detail.mirrorPlane",
    ".timeline.items[].detail.axis",
    ".timeline.items[].detail.directionOneEntity",
    ".timeline.items[].detail.directionTwoEntity",
    ".timeline.items[].detail.directionOne",
    ".timeline.items[].detail.directionTwo",
    ".timeline.items[].detail.holeType",
    ".timeline.items[].detail.position",
    ".timeline.items[].detail._f3d_positions",
    ".timeline.items[].detail.counterboreDiameter",
    ".timeline.items[].detail.counterboreDepth",
    ".timeline.items[].detail.extentDefinition",
    // Captured positions and the occurrences that occurrence items made
    // (mitcad#75; `.detail.occurrence` is above).
    ".timeline.items[].detail.positions",
    // Where the timeline's items put each occurrence, and rigid groups'
    // members (mitcad#81), checked against the joint test model's dump
    // (`design_models.rs`).
    ".occurrences[]._f3d.placements",
    ".occurrences[].children[]._f3d.placements",
    ".occurrences[].children[].children[]._f3d.placements",
    ".occurrences[].children[].children[].children[]._f3d.placements",
    ".timeline.items[].detail.occurrences",
    // Inputs read from the learning dump (mitcad#96): profile loops, a
    // pattern's axis line, a hole's through-all flag beside its depth, a
    // mirror's combine.
    ".timeline.items[].detail._f3d_profile_loops",
    // Text profiles whose texts the design no longer has (mitcad#332).
    ".timeline.items[].detail._f3d_missing_texts",
    // The texts an extrusion's text profiles name, with its profile count
    // (mitcad#1177).
    ".timeline.items[].detail._f3d_text_profiles",
    ".timeline.items[].detail._f3d_axis",
    ".timeline.items[].detail._f3d_through_all",
    ".timeline.items[].detail.isCombine",
    // A pattern's or mirror's settings from its data object (mitcad#186),
    // checked against the one-option variants' dumps (`design_models.rs`).
    ".timeline.items[].detail.isSymmetric",
    ".timeline.items[].detail.isSymmetricInDirectionOne",
    ".timeline.items[].detail.isSymmetricInDirectionTwo",
    ".timeline.items[].detail.patternComputeOption",
    ".timeline.items[].detail.patternDistanceType",
    ".timeline.items[].detail.suppressedElementsIds",
    // A join's or cut's participants by the items that made them
    // (mitcad#96), checked against the bodies the history accepted with
    // the import's learning dump.
    ".timeline.items[].detail.participantBodies",
    // Sketch splines, ellipses and elliptical arcs (mitcad#154), checked
    // against the reference models' dumps (`design_models.rs`).
    ".timeline.items[].detail.curves[].type",
    ".timeline.items[].detail.curves[].geometry",
    ".timeline.items[].detail.curves[].centerSketchPoint",
    ".timeline.items[].detail.curves[].startSketchPoint",
    ".timeline.items[].detail.curves[].endSketchPoint",
    ".timeline.items[].detail.curves[].majorAxisRadius",
    ".timeline.items[].detail.curves[].minorAxisRadius",
    ".timeline.items[].detail.curves[].majorAxis",
    ".timeline.items[].detail.curves[].degree",
    // A curve's fixed and linked flags and a fitted spline's fit points
    // (mitcad#186), checked against the API reference dumps.
    ".timeline.items[].detail.curves[].isFixed",
    // The sketch's origin point is fixed (mitcad#641; the reference dumps
    // record no point flags).
    ".timeline.items[].detail.points[].isFixed",
    ".timeline.items[].detail.curves[].isReference",
    ".timeline.items[].detail.curves[].isLinked",
    ".timeline.items[].detail.curves[].fitPoints",
    // Fillet and chamfer options and the sets of variable, chord length
    // and asymmetric fillets (mitcad#154), likewise.
    ".timeline.items[].detail.edgeSets",
    ".timeline.items[].detail.edgeSets[]._type",
    ".timeline.items[].detail.edgeSets[].continuity",
    ".timeline.items[].detail.edgeSets[].isTangentChain",
    ".timeline.items[].detail.edgeSets[].isFlipped",
    ".timeline.items[].detail.edgeSets[].tangencyWeight",
    ".timeline.items[].detail.isRollingBallCorner",
    ".timeline.items[].detail.cornerType",
    // A shell's faces or body and outside thickness, offset faces' faces
    // (mitcad#154).
    ".timeline.items[].detail.outsideThickness",
    ".timeline.items[].detail.inputFaces",
    // Cylinder and sphere primitives' sizes (mitcad#154).
    ".timeline.items[].detail.height",
    ".timeline.items[].detail.diameter",
    // An extrusion's solid or surface, start and sides (mitcad#154).
    ".timeline.items[].detail.isSolid",
    ".timeline.items[]._f3d.extrude.solid",
    ".timeline.items[]._f3d.extrude.start",
    ".timeline.items[]._f3d.extrude.sides",
    // The operation of a component made from bodies in the new component's
    // history (mitcad#156), checked against the history by the import.
    ".timeline.items[]._f3d.made_result_no",
    // The constraints of assembly relationships and tangent relationships
    // (mitcad#155), checked against the occurrences' placements by the
    // import (`occurrenceOne`/`Two` are above).
    ".timeline.items[].detail.rows",
    ".timeline.items[].detail.bodyOne",
    ".timeline.items[].detail.bodyTwo",
    ".timeline.items[].detail.pointOne",
    ".timeline.items[].detail.pointTwo",
    ".timeline.items[].detail.faceOne",
    ".timeline.items[].detail.faceTwo",
    // Items suppressed in the file, from their tails' state bytes
    // (mitcad#180), checked on items checked in the original application.
    ".timeline.items[].isSuppressed",
    // The two state bytes before an item's reference, items after the
    // timeline marker and the marker, a thread's handedness,
    // checked against the controlled variants' dumps (`design_models.rs`).
    ".timeline.items[]._f3d.after",
    ".timeline.items[].isRolledBack",
    ".timeline.original_marker_position",
    ".timeline.restored_marker_position",
    ".timeline.items[].detail.isRightHanded",
    ".timeline.items[].detail.threadInfo.isRightHanded",
    ".timeline.items[].detail.tappedHoleInfo.isRightHanded",
    ".timeline.items[].detail.thread.isRightHanded",
    // A construction plane's or point's definition code (mitcad#186),
    // checked against the reference models' dumps (`design_models.rs`).
    ".timeline.items[]._f3d.datum_code",
    // The entities and type bit of constraints the reference decoder lists
    // without them (mitcad#186: see ADDED).
    ".timeline.items[].detail.constraints[].refs",
    ".timeline.items[].detail.constraints[]._f3d",
    // What a derive brings from other documents (mitcad#324), which the
    // reference decoder did not read.
    ".timeline.items[].detail.source",
    ".timeline.items[].detail.placements",
    ".timeline.items[].detail.entities",
    // The faces a move face moves (mitcad#343), which the reference decoder
    // did not read; the import finds them by their face inputs.
    ".timeline.items[].detail.faces",
];

/// Fields this decoder fills where the reference decoder has an empty
/// text: the names of occurrence items, after their component
/// (mitcad#43).
const NAMED: &[&str] = &[".timeline.items[].name"];

/// Fields this decoder fills where the reference decoder has null: the
/// sketch of a parameter or a profile, for a sketch this decoder links to
/// its feature through a recipe object (mitcad#186).
const FILLED: &[&str] = &[
    ".parameters.model[].createdBy.sketch",
    ".parameters.model[].createdBy.sketch_timeline_index",
    ".timeline.items[].detail.profile[].sketch",
    ".timeline.items[].detail.profile[].sketch_timeline_index",
];

/// Values this decoder gives where the reference decoder has none:
/// (field, this decoder's value). Offset dimensions (mitcad#36), which
/// refer to one curve.
const ADDED: &[(&str, &str)] = &[
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchOffsetCurvesDimension",
    ),
    (
        ".parameters.model[].createdBy.objectType",
        "SketchOffsetCurvesDimension",
    ),
    // Constraints the reference decoder lists without type, entities and
    // mask: a rectangular pattern constraint of newer writers (the values
    // sit over 200 bytes before the sketch) and a text's path constraint
    // (mitcad#186).
    (
        ".timeline.items[].detail.constraints[].type",
        "RectangularPatternConstraint",
    ),
    // Dimension types the reference decoder does not know (mitcad#186).
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchEllipseMajorRadiusDimension",
    ),
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchEllipseMinorRadiusDimension",
    ),
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchTangentDistanceDimension",
    ),
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchDistanceBetweenPointAndSurfaceDimension",
    ),
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchDistanceBetweenLineAndPlanarSurfaceDimension",
    ),
    // Constraint types the reference decoder does not know (mitcad#186).
    (
        ".timeline.items[].detail.constraints[].type",
        "SmoothConstraint",
    ),
    (
        ".timeline.items[].detail.constraints[].type",
        "ControlPolygonConstraint",
    ),
    (
        ".timeline.items[].detail.constraints[].type",
        "TextBoxConstraint",
    ),
    (
        ".timeline.items[].detail.constraints[].type",
        "TextPathConstraint",
    ),
];

/// Values this decoder tells apart where the reference decoder does not:
/// (field, this decoder's value, the reference's). Offset dimensions that
/// measure a diameter about a line (mitcad#33).
const REFINED: &[(&str, &str, &str)] = &[
    // Rigid groups, which the file stores as as-built joints (mitcad#81).
    (".timeline.items[].objectType", "RigidGroup", "AsBuiltJoint"),
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchLinearDiameterDimension",
        "SketchOffsetDimension",
    ),
    (
        ".parameters.model[].createdBy.objectType",
        "SketchLinearDiameterDimension",
        "SketchOffsetDimension",
    ),
    // The distances of offsets (mitcad#36).
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchOffsetCurvesDimension",
        "SketchOffsetDimension",
    ),
    (
        ".parameters.model[].createdBy.objectType",
        "SketchOffsetCurvesDimension",
        "SketchOffsetDimension",
    ),
    (
        ".timeline.items[].detail.dimensions[].type",
        "SketchOffsetCurvesDimension",
        "SketchConcentricCircleDimension",
    ),
    (
        ".parameters.model[].createdBy.objectType",
        "SketchOffsetCurvesDimension",
        "SketchConcentricCircleDimension",
    ),
];

fn leaves(v: &Value) -> usize {
    match v {
        Value::Object(m) => m.values().map(leaves).sum::<usize>().max(1),
        Value::Array(a) => a.iter().map(leaves).sum::<usize>().max(1),
        _ => 1,
    }
}

impl Agreement {
    fn note(&mut self, field: &str, ok: bool, n: usize, what: impl FnOnce() -> String) {
        let e = self.by_field.entry(field.to_string()).or_default();
        if ok {
            e.0 += n;
        } else {
            e.1 += n;
            if self.examples.len() < 40 {
                self.examples.push(what());
            }
        }
    }

    fn compare(&mut self, label: &str, path: &str, field: &str, rust: &Value, reference: &Value) {
        if IGNORED.contains(&field) {
            return;
        }
        match (rust, reference) {
            (Value::Object(a), Value::Object(b)) => {
                let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
                keys.sort();
                keys.dedup();
                for k in keys {
                    let p = format!("{path}.{k}");
                    let f = format!("{field}.{k}");
                    match (a.get(k), b.get(k)) {
                        (Some(x), Some(y)) => self.compare(label, &p, &f, x, y),
                        (None, Some(y)) => {
                            let n = leaves(y);
                            self.missing += n;
                            self.note(&f, false, n, || format!("{label}: {p} missing: {y}"));
                        }
                        (Some(_), None) if DECODER_ONLY.contains(&f.as_str()) => {}
                        (Some(x), None)
                            if ADDED.iter().any(|(field, v)| *field == f && x == *v) => {}
                        (Some(x), None) => {
                            let n = leaves(x);
                            self.extra += n;
                            self.note(&f, false, n, || format!("{label}: {p} extra: {x}"));
                        }
                        (None, None) => {}
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                let f = format!("{field}[]");
                for i in 0..a.len().max(b.len()) {
                    let p = format!("{path}[{i}]");
                    match (a.get(i), b.get(i)) {
                        (Some(x), Some(y)) => self.compare(label, &p, &f, x, y),
                        (None, Some(y)) => {
                            let n = leaves(y);
                            self.missing += n;
                            self.note(&f, false, n, || format!("{label}: {p} missing: {y}"));
                        }
                        (Some(x), None) => {
                            let n = leaves(x);
                            self.extra += n;
                            self.note(&f, false, n, || format!("{label}: {p} extra: {x}"));
                        }
                        (None, None) => {}
                    }
                }
                if a.is_empty() && b.is_empty() {
                    self.equal += 1;
                    self.note(field, true, 1, String::new);
                }
            }
            (x, y) => {
                let refined = REFINED
                    .iter()
                    .any(|(f, r, reference)| *f == field && x == *r && y == *reference);
                let named = NAMED.contains(&field)
                    && y.as_str() == Some("")
                    && x.as_str().is_some_and(|n| !n.is_empty());
                let filled = FILLED.contains(&field) && y.is_null() && !x.is_null();
                let ok = x == y || refined || named || filled;
                if ok {
                    self.equal += 1;
                } else {
                    self.differ += 1;
                }
                self.note(field, ok, 1, || {
                    format!("{label}: {path}: rust {x} reference {y}")
                });
            }
        }
    }
}

/// Evaluates every parameter with Mitcad's expression engine.
#[derive(Default, Debug)]
struct ExprCheck {
    ok: usize,
    mismatch: usize,
    /// Text parameters and expressions the engine does not evaluate.
    skipped: usize,
    examples: Vec<String>,
}

fn check_expressions(dump: &ir::Dump, label: &str, out: &mut ExprCheck) {
    let params: Vec<&ir::Parameter> = dump.all_parameters().collect();
    let mut values: HashMap<String, Quantity> = HashMap::new();
    for p in &params {
        let (Some(name), Some(unit), Some(value)) = (&p.name, &p.unit, p.value) else {
            continue;
        };
        // A text parameter (unit `Text`, mitcad#664) has no number.
        if let Ok(u) = f3d::parse_unit(unit)
            && !u.is_text()
        {
            values.insert(name.clone(), f3d::internal_quantity(value, u.dims()));
        }
    }
    let ctx = f3d::context(LengthUnit::Millimetre);
    for p in &params {
        let (Some(name), Some(expr), Some(unit), Some(value)) =
            (&p.name, &p.expression, &p.unit, p.value)
        else {
            continue;
        };
        if unit == "Text" {
            out.skipped += 1;
            continue;
        }
        match f3d::check_parameter(expr, unit, value, &values, &ctx, 1e-9) {
            Ok((_, true)) => out.ok += 1,
            Ok((q, false)) => {
                out.mismatch += 1;
                out.examples.push(format!(
                    "{label}: {name} = {expr:?} -> {q}, stored {value} ({unit})"
                ));
            }
            Err(e) => {
                out.skipped += 1;
                if out.examples.len() < 20 {
                    out.examples
                        .push(format!("{label}: {name} = {expr:?}: {e}"));
                }
            }
        }
    }
}

#[test]
fn corpus_designs_match_the_reference_decoder() {
    let Some(corpus) = home_dir("MITCAD_F3D_CORPUS", "f3d-corpus") else {
        eprintln!("corpus not found; skipped");
        return;
    };
    let reference = home_dir("MITCAD_F3D_DESIGN_REF", "f3d-design-ref");
    let ref_files: Vec<PathBuf> = reference
        .as_ref()
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    // Each reference names its design (`source.file`); it is paired by that
    // name, not by the number in its file name, which shifts when a design
    // is added to the corpus.
    let mut ref_by_design: HashMap<String, (PathBuf, String)> = HashMap::new();
    for file in ref_files
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "json"))
    {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let Ok(r) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if let Some(name) = r["source"]["file"].as_str() {
            ref_by_design.insert(name.to_string(), (file.clone(), text));
        }
    }
    let mut agreement = Agreement::default();
    let mut exprs = ExprCheck::default();
    let (mut designs, mut compared, mut segments_parsed) = (0usize, 0usize, 0usize);
    let (mut items, mut named, mut params, mut sketches) = (0usize, 0usize, 0usize, 0usize);
    let (mut points, mut curves, mut constraints, mut dimensions) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut identical, mut objects) = (0usize, 0usize);
    // Sketches whose light bulb decodes (mitcad#6).
    let mut bulbs = 0usize;
    for path in corpus_files(&corpus) {
        for (label, doc) in design::documents(&path).unwrap() {
            designs += 1;
            let Some(s) = design::find_design_streams(&doc).unwrap() else {
                continue;
            };
            let d = Design::parse(&s.meta, s.bulk).unwrap_or_else(|e| panic!("{label}: {e}"));
            segments_parsed += 1;
            // Framing: the whole MetaStream; every object header has the
            // index id and the tag 256 + class index.
            assert!(d.segment.meta.tail_ok, "{label}: MetaStream tail");
            for o in &d.segment.objects {
                let h = d.segment.header(o).unwrap();
                assert_eq!(
                    (h.class_index(), h.id),
                    (o.class, o.id),
                    "{label}: {}",
                    o.id
                );
                objects += 1;
            }
            let dump = d.dump(&label, &s.segment_dir);
            check_expressions(&dump, &label, &mut exprs);
            let tl = dump.timeline_items();
            items += tl.len();
            named += tl.iter().filter(|i| i.name().is_some()).count();
            params += dump.all_parameters().count();
            for it in tl {
                if let Some(ir::Detail::Sketch(s)) = &it.detail {
                    sketches += 1;
                    bulbs += usize::from(it.light_bulb().is_some());
                    points += s.points.as_ref().map_or(0, Vec::len);
                    curves += s.curves.as_ref().map_or(0, Vec::len);
                    constraints += s.constraints.as_ref().map_or(0, Vec::len);
                    dimensions += s.dimensions.as_ref().map_or(0, Vec::len);
                }
            }
            // The IR reads its own output back unchanged.
            let v = serde_json::to_value(&dump).unwrap();
            let back: ir::Dump = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                v,
                "{label}: round trip"
            );

            let Some((file, text)) = ref_by_design.get(label.as_str()) else {
                continue;
            };
            let r: Value = serde_json::from_str(text).unwrap();
            assert_eq!(
                r["source"]["file"],
                Value::from(label.as_str()),
                "{}",
                file.display()
            );
            // The reference reads into the IR as well.
            let rd = ir::Dump::from_json(text).unwrap();
            assert_eq!(
                serde_json::to_value(&rd).unwrap(),
                r,
                "{label}: reference round trip"
            );
            // The parts of items in other components are items of this
            // decoder's only (mitcad#169), checked against the history by
            // the import.
            let mut v = v;
            internal_names(&mut v);
            if let Some(items) = v
                .pointer_mut("/timeline/items")
                .and_then(Value::as_array_mut)
            {
                items.retain(|it| it.pointer("/_f3d/part_of").is_none());
                // Driven dimensions and the class of dimensions not read,
                // and construction planes' inputs (mitcad#178): this
                // decoder's only; the reference has the dimension's id and
                // the plane's parameter.
                for (index, it) in items.iter_mut().enumerate() {
                    // A hole through all or up to an object (its slot of
                    // role 5): the reference decoder reads the kept depth as
                    // a distance.
                    if it["objectType"] == "HoleFeature"
                        && it["detail"]["extentDefinition"]["_type"] != "DistanceExtentDefinition"
                        && let Some(reference) =
                            r.pointer(&format!("/timeline/items/{index}/detail/extentDefinition"))
                        && let Some(d) = it["detail"].as_object_mut()
                    {
                        d.insert("extentDefinition".into(), reference.clone());
                    }
                    // The extents this decoder reads from the extrude record
                    // (side kinds, the flag byte, the symmetric kind, the
                    // start): the reference decoder has none of them, and
                    // gives a symmetric extent as one side's distance, without its extent type.
                    if it["objectType"] == "ExtrudeFeature"
                        && let Some(d) = it["detail"].as_object_mut()
                    {
                        d.remove("hasTwoExtents");
                        d.remove("symmetricExtent");
                        if d.get("extentType") == Some(&Value::from("SymmetricFeatureExtentType")) {
                            d.remove("extentType");
                        }
                        if d["startExtent"]["_type"] == "ProfilePlaneStartDefinition" {
                            d.remove("startExtent");
                        }
                        for side in ["extentOne", "extentTwo"] {
                            let Some(e) = d.get_mut(side).and_then(Value::as_object_mut) else {
                                continue;
                            };
                            for k in [
                                "isSymmetric",
                                "isPositiveDirection",
                                "isChained",
                                "isMinimumSolution",
                                "directionHint",
                                "isFullLength",
                                "taperAngle",
                            ] {
                                e.remove(k);
                            }
                            match e.get("_type").and_then(Value::as_str) {
                                Some("SymmetricExtentDefinition") => {
                                    e.insert(
                                        "_type".into(),
                                        Value::from("DistanceExtentDefinition"),
                                    );
                                }
                                Some("ThroughAllExtentDefinition") => {
                                    d.remove(side);
                                }
                                _ => {}
                            }
                        }
                    }
                    // A construction point's position (mitcad#186).
                    if it["objectType"] == "ConstructionPoint"
                        && let Some(d) = it["detail"].as_object_mut()
                    {
                        d.remove("geometry");
                    }
                    if let Some(dims) = it
                        .pointer_mut("/detail/dimensions")
                        .and_then(Value::as_array_mut)
                    {
                        for d in dims.iter_mut() {
                            let f = &d["_f3d"];
                            if !f.is_null()
                                && (f.get("flags").is_none() || f.get("expression").is_some())
                            {
                                *d = serde_json::json!({"id": d["id"].clone()});
                            }
                        }
                    }
                    if let Some(def) = it
                        .pointer_mut("/detail/definition")
                        .and_then(Value::as_object_mut)
                    {
                        for k in [
                            "planarEntity",
                            "linearEntity",
                            "planarEntityOne",
                            "planarEntityTwo",
                            "linearEntityOne",
                            "linearEntityTwo",
                            "pointEntity",
                            "pointEntityOne",
                            "pointEntityTwo",
                            "pointEntityThree",
                            "pathEntities",
                            // Planes' faces and points, points' and axes'
                            // entities (mitcad#186).
                            "tangentFace",
                            "point",
                            "edgeEntity",
                            "circularFace",
                            "circularEntity",
                            "planeOne",
                            "planeTwo",
                            "planeThree",
                        ] {
                            def.remove(k);
                        }
                        let parameter = ["offset", "angle", "distance"]
                            .iter()
                            .any(|k| def.contains_key(*k));
                        if !parameter {
                            it["detail"]
                                .as_object_mut()
                                .expect("a detail")
                                .remove("definition");
                        }
                    }
                }
                // Sketch texts (mitcad#186): the reference decoder does not
                // read them (`sketch_texts_match_the_reference_dumps` checks
                // them against the API dumps).
                for it in items.iter_mut() {
                    if let Some(detail) = it["detail"].as_object_mut() {
                        detail.remove("texts");
                    }
                }
                // The faces an extrusion's profile selects (mitcad#169):
                // the reference decoder gives each as one more profile of
                // the extrusion's sketch.
                for it in items.iter_mut() {
                    let Some(profile) = it
                        .pointer_mut("/detail/profile")
                        .and_then(Value::as_array_mut)
                    else {
                        continue;
                    };
                    let Some(sketch) = profile.iter().find(|p| p["kind"] == "profile").cloned()
                    else {
                        continue;
                    };
                    for p in profile.iter_mut().filter(|p| p["kind"] == "face") {
                        *p = sketch.clone();
                    }
                }
            }
            let before = agreement.differ + agreement.missing + agreement.extra;
            agreement.compare(&label, "", "", &v, &line_subclasses(&v, current_names(&r)));
            if agreement.differ + agreement.missing + agreement.extra == before {
                identical += 1;
            }
            compared += 1;
        }
    }
    println!(
        "designs {designs}, design segments {segments_parsed} ({objects} objects); \
         timeline items {items} (named {named}), \
         parameters {params}, sketches {sketches} (points {points}, curves {curves}, \
         constraints {constraints}, dimensions {dimensions}; light bulb decoded {bulbs}, \
         unknown {})",
        sketches - bulbs
    );
    println!(
        "parameter expressions (mitcad_model::expr::f3d): ok {}, mismatch {}, skipped {}",
        exprs.ok, exprs.mismatch, exprs.skipped
    );
    for e in &exprs.examples {
        println!("  {e}");
    }
    assert_eq!(
        exprs.mismatch, 0,
        "parameter values differ from their expressions"
    );
    if compared == 0 {
        eprintln!("reference JSON not found; comparison skipped");
        return;
    }
    let total = agreement.equal + agreement.differ + agreement.missing + agreement.extra;
    println!(
        "reference comparison: {compared} designs ({identical} identical); fields {total}: \
         equal {}, different {}, missing {}, extra {} ({:.4} % agree)",
        agreement.equal,
        agreement.differ,
        agreement.missing,
        agreement.extra,
        100.0 * agreement.equal as f64 / total.max(1) as f64
    );
    let mut fields: Vec<_> = agreement.by_field.iter().collect();
    fields.sort_by_key(|(_, (ok, bad))| std::cmp::Reverse(ok + bad));
    println!("fields (equal / total):");
    for (f, (ok, bad)) in fields.iter().take(60) {
        println!("  {:>8} / {:<8} {f}", ok, ok + bad);
    }
    for (f, (ok, bad)) in &fields {
        if *bad > 0 {
            println!("  DIFF {f}: {bad} of {}", ok + bad);
        }
    }
    for e in &agreement.examples {
        println!("  {e}");
    }
    assert_eq!(
        agreement.differ + agreement.missing + agreement.extra,
        0,
        "the dump IR differs from the reference decoder; if the reference decoder \
         changed, regenerate the reference JSON (MITCAD_F3D_DESIGN_REF) with it"
    );
}

/// The inputs found on several threads (`resolve_inputs`, mitcad#103) give
/// the dump one thread gives, for every corpus design.
#[test]
fn inputs_found_on_threads_give_the_same_dump() {
    let Some(dir) = home_dir("MITCAD_F3D_CORPUS", "f3d-corpus") else {
        eprintln!("corpus not found; skipped");
        return;
    };
    let mut compared = 0;
    for path in corpus_files(&dir) {
        let Ok(documents) = design::documents(&path) else {
            continue;
        };
        for (label, doc) in &documents {
            let Ok(one) = design::decode_document(doc, label) else {
                continue;
            };
            let mut four = design::decode_streams(doc, label).expect("decoded once");
            design::resolve_inputs(&mut four, doc, 4);
            let json = |d: &design::FileDesign| serde_json::to_value(&d.dump).expect("serializes");
            assert!(json(&one) == json(&four), "{label}: the dumps differ");
            compared += 1;
        }
    }
    eprintln!("{compared} designs compared");
}
