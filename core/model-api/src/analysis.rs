// SPDX-License-Identifier: MIT
//! Queries on construction geometry and analysis (F5): datums, physical
//! properties, measurement, interference, sections and comparison with a
//! STEP file, their text form for `mitcad-cli`, and shapes for display.
//! See `commands.md`.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write;

use serde::Deserialize;
use serde_json::{Value, json};

use super::measure_axes::{self, Element};
use super::prelude::*;
use super::{ApiError, parse, read_json};
use mitcad_model::analysis::{
    self, CompareOptions, Comparison, Measurement, PhysicalProperties, Selection, Separation,
};
use mitcad_model::datum::{CurveGeometry, Datum, DatumPlane, OriginDatum, SurfaceGeometry, Vec3};
use mitcad_model::features::OccurrencePath;
use mitcad_model::features::geom_ref::GeomRef;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::internal::fingerprint::Fingerprint;
use mitcad_model::internal::joints::path_component;
use mitcad_model::kernel::{Kernel, KernelError, Plane};
use mitcad_model::topo::{EdgeName, FaceName, TopoName, VertexName};
use mitcad_model::transform::Transform;
use mitcad_model::{BodyView, Document, MeasurePick};

/// Size of the shapes that stand for datums in measurements, mm: large
/// enough to act as unbounded.
const DATUM_EXTENT: f64 = 1e5;

/// Two planes whose normals are closer than this (radians, either way) are
/// parallel for Measure's offset (mitcad#646): the tolerance assembly
/// constraints hold directions to.
const PARALLEL: f64 = mitcad_model::internal::constraints::ANGULAR;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DatumsQuery {
    /// Lists the origin datums first.
    #[serde(default)]
    origin: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DatumQuery {
    /// A construction feature (`F3`) or an origin datum (`xy`).
    uid: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PropertiesQuery {
    /// Empty for all bodies at the marker.
    #[serde(default)]
    bodies: Vec<BodyUid>,
    /// g/cm³ for bodies without a material, instead of the default
    /// material's.
    #[serde(default)]
    density: Option<f64>,
    /// g/cm³ per body, overriding everything else.
    #[serde(default)]
    densities: BTreeMap<BodyUid, f64>,
}

/// A body, a face, edge or vertex of it, or a datum.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SelectionInput {
    #[serde(default)]
    body: Option<BodyUid>,
    #[serde(default)]
    face: Option<FaceName>,
    #[serde(default)]
    edge: Option<EdgeName>,
    #[serde(default)]
    vertex: Option<VertexName>,
    /// A construction feature (`F5`) or an origin datum by its id or name
    /// in any case (`xy`, `XY`, `origin`; mitcad#985).
    #[serde(default)]
    datum: Option<String>,
    /// The occurrence that places the body or datum in the design, a path
    /// from the root (`"O1/O4"`, mitcad#646), also by the occurrences'
    /// names (`"Arm:1/Pin:2"`, mitcad#985); left out, the owner's only
    /// placement (none in the root).
    #[serde(default)]
    occurrence: Option<String>,
    /// The entity form `ui_state` lists the selection in and `ui_select`
    /// takes (mitcad#985): `kind` (`face`, `edge`, `vertex`, `body`,
    /// `plane`, `axis` or `point`), `owner` (the body or the datum) and
    /// `name` (the face, edge or vertex name; empty otherwise), with
    /// `occurrence`. In place of `body`, `face`, `edge`, `vertex` and
    /// `datum`.
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    name: Option<String>,
    /// What `ui_state` adds to an entity, taken and not used, so that an
    /// entity can be passed on unchanged.
    #[serde(default, rename = "geometry")]
    _geometry: Option<Value>,
    #[serde(default, rename = "reference")]
    _reference: Option<Value>,
    #[serde(default, rename = "text")]
    _text: Option<Value>,
}

/// A selection as fields, or as text: a datum (`xy`, `F5`), or a body
/// (uid or name) with an optional `/` and the name of a face, edge or
/// vertex (`Body1/E{…|…}`).
#[derive(Debug, Clone)]
pub(super) enum SelectionArg {
    Text(String),
    Fields(Box<SelectionInput>),
}

impl<'de> Deserialize<'de> for SelectionArg {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        match Value::deserialize(deserializer)? {
            Value::String(text) => Ok(Self::Text(text)),
            value => serde_json::from_value(value)
                .map(|fields| Self::Fields(Box::new(fields)))
                .map_err(D::Error::custom),
        }
    }
}

impl SelectionArg {
    fn describe(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Fields(fields) => fields.describe(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MeasureQuery {
    a: SelectionArg,
    #[serde(default)]
    b: Option<SelectionArg>,
    /// Include the exact maximum nearest-face gap when supported (#661).
    #[serde(default)]
    face_range: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InterferenceQuery {
    #[serde(default)]
    bodies: Vec<BodyUid>,
    /// mm³; smaller overlaps are not reported.
    #[serde(default = "min_volume")]
    min_volume: f64,
}

fn min_volume() -> f64 {
    1e-6
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SectionQuery {
    /// A plane reference (`"xy"`, `"F5"`, a planar face or a fixed plane
    /// `{"origin": [...], "normal": [...]}`).
    plane: GeomRef,
    #[serde(default)]
    bodies: Vec<BodyUid>,
    /// Section each visible occurrence in design coordinates, as displayed.
    /// Omission retains the original local body query contract.
    #[serde(default)]
    world: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompareStepQuery {
    /// Path of the STEP file.
    file: String,
    /// Mitcad's side; empty for all bodies at the marker.
    #[serde(default)]
    bodies: Vec<BodyUid>,
    /// A body of the file by name or index; all bodies of the file when
    /// left out.
    #[serde(default)]
    step_body: Option<Value>,
    #[serde(default)]
    samples: Option<usize>,
    #[serde(default)]
    fuzzy: Option<f64>,
    /// Limits: the comparison passes when the largest deviation (mm) and
    /// the relative volume difference stay within them.
    #[serde(default)]
    max_deviation: Option<f64>,
    #[serde(default)]
    max_relative: Option<f64>,
}

/// `analysis_shape` requests.
#[derive(Debug, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
enum ShapeRequest {
    /// A datum as a face, edge or vertex of the given size.
    Datum {
        uid: String,
        #[serde(default = "datum_size")]
        size: f64,
    },
    /// The section faces (or `curves`) of a body.
    Section {
        plane: GeomRef,
        body: BodyUid,
        #[serde(default)]
        occurrence: Option<OccurrencePath>,
        #[serde(default)]
        curves: bool,
    },
    /// A body without the half-space on the plane's normal side.
    Clip {
        plane: GeomRef,
        body: BodyUid,
        #[serde(default)]
        occurrence: Option<OccurrencePath>,
    },
    /// The overlap of two bodies.
    Interference { a: BodyUid, b: BodyUid },
    /// The developed solid of a body's valid Flat Pattern (mitcad#210), in
    /// the body's coordinates; none for an invalid one.
    FlatPattern { body: BodyUid },
}

fn datum_size() -> f64 {
    100.0
}

/// `analysis_clips`: several bodies clipped at one plane (mitcad#438).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipsRequest {
    plane: GeomRef,
    bodies: Vec<ClipTarget>,
}

/// A body of `analysis_clips`, as the `clip` shape request gives it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipTarget {
    body: BodyUid,
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    /// The body as the previewed command leaves it (mitcad#943), in place
    /// of the body at the marker: a feature preview shown cut by Section
    /// Analysis or Hide Above Sketch.
    #[serde(default)]
    preview: bool,
    /// Where the view places the body in the design (a preview's moved
    /// occurrence, a body the preview makes): the plane is taken into the
    /// body's coordinates by this rigid placement instead of by
    /// `occurrence` (mitcad#943).
    #[serde(default)]
    transform: Option<Transform>,
}

/// A query `analysis` answers as text.
#[derive(Debug, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case")]
enum TextQuery {
    Properties(PropertiesQuery),
    Measure(MeasureQuery),
    Interference(InterferenceQuery),
    Section(SectionQuery),
    CompareStep(CompareStepQuery),
    Datums(DatumsQuery),
    // Recompute speed (P10).
    RecomputeTimes,
}

/// How many of the slowest features the text of `recompute_times` lists.
const SLOWEST: usize = 10;

fn kernel_error(what: &str) -> impl Fn(KernelError) -> ApiError + '_ {
    move |e| ApiError(format!("{what}: {e}"))
}

fn round(value: f64) -> f64 {
    // + 0.0 turns -0.0 into 0.0 in text.
    value + 0.0
}

fn point(p: Vec3) -> String {
    format!(
        "[{:.3}, {:.3}, {:.3}]",
        round(p[0]),
        round(p[1]),
        round(p[2])
    )
}

/// The JSON of a datum.
pub(super) fn datum_json(datum: &Datum) -> Value {
    // + 0.0 turns -0.0 into 0.0.
    let v = |v: Vec3| v.map(|x| x + 0.0);
    match datum {
        Datum::Plane(plane) => json!({"type": "plane", "origin": v(plane.origin),
            "normal": v(plane.normal()), "x_axis": v(plane.x_axis), "y_axis": v(plane.y_axis)}),
        Datum::Axis(axis) => {
            json!({"type": "axis", "origin": v(axis.origin), "direction": v(axis.direction)})
        }
        Datum::Point(point) => json!({"type": "point", "point": v(point.point)}),
    }
}

fn properties_json(p: &PhysicalProperties) -> Value {
    json!({
        "volume": p.volume,
        "area": p.area,
        "mass": p.mass,
        "center_of_mass": p.center_of_mass,
        "inertia": p.inertia,
        "principal_moments": p.principal_moments,
        "principal_axes": p.principal_axes,
    })
}

fn measurement_json(m: &Measurement) -> Value {
    let mut value = json!({"kind": m.kind});
    let fields = [("volume", m.volume), ("area", m.area), ("length", m.length)];
    for (key, field) in fields {
        if let Some(v) = field {
            value[key] = json!(v);
        }
    }
    if let Some(p) = m.point {
        value["point"] = json!(p);
    }
    if let Some(c) = m.circle {
        value["circle"] = json!({"center": c.center, "axis": c.axis, "radius": c.radius,
                                 "sweep": c.sweep});
    }
    value
}

fn separation_json(s: &Separation) -> Value {
    json!({"distance": s.distance, "on_a": s.on_a, "on_b": s.on_b, "inside": s.inside,
           "angle": s.angle})
}

fn comparison_json(c: &Comparison) -> Value {
    let deviation = |d: &analysis::Deviation| json!({"max": d.max, "rms": d.rms, "at": d.at, "samples": d.samples});
    json!({
        "step_bodies": c.step_bodies,
        "volume": c.volume_a,
        "step_volume": c.volume_b,
        "volume_only_mitcad": c.a_minus_b,
        "volume_only_step": c.b_minus_a,
        "relative_difference": c.relative_difference,
        "max_deviation": c.max_deviation,
        "mitcad_to_step": deviation(&c.a_to_b),
        "step_to_mitcad": deviation(&c.b_to_a),
        "bounds_difference": c.bounds_difference,
    })
}

impl SelectionInput {
    fn describe(&self) -> String {
        if let Some(datum) = &self.datum {
            return datum.clone();
        }
        if let Some(kind) = &self.kind {
            let owner = self.owner.as_deref().unwrap_or_default();
            return match self.name.as_deref().filter(|name| !name.is_empty()) {
                Some(name) => format!("{owner} {kind} {name}"),
                None => format!("{kind} {owner}"),
            };
        }
        let body = self.body.map(|b| b.to_string()).unwrap_or_default();
        match (&self.face, &self.edge, &self.vertex) {
            (Some(face), _, _) => format!("{body} face {face}"),
            (_, Some(edge), _) => format!("{body} edge {edge}"),
            (_, _, Some(vertex)) => format!("{body} vertex {vertex}"),
            _ => body,
        }
    }
}

/// A selection resolved to a shape and a name in it, with the motion that
/// places the shape in the design's coordinates (its occurrence's,
/// mitcad#646), and its plane, axis, line or point in the design's
/// coordinates where it has one (mitcad#647).
///
/// The shape is the body's own, not a moved copy (mitcad#981): a copy of
/// a body with thousands of faces (modelled threads) took a large part of
/// a second for each face the selection card measured, while a face's
/// area, an edge's length and their circles move with the motion.
/// [`placed_shape`] moves it where a measurement needs it placed.
struct Resolved<S> {
    shape: S,
    name: Option<TopoName>,
    element: Option<Element>,
    /// From the shape's coordinates into the design's; the identity for
    /// the root's bodies and for datums (made where they are).
    placed: Transform,
    /// The occurrence the selection gave, by ids (mitcad#985: a path
    /// given by names is answered as ids).
    occurrence: Option<OccurrencePath>,
}

impl<S> Resolved<S> {
    fn selection(&self) -> Selection<'_, S> {
        Selection {
            shape: &self.shape,
            name: self.name.as_ref(),
        }
    }
}

/// The resolved shape moved into the design's coordinates: itself when
/// nothing moves it, else a moved copy of the whole body.
fn placed_shape<K: Kernel>(
    kernel: &K,
    resolved: &Resolved<K::Shape>,
    what: &str,
) -> Result<K::Shape, ApiError> {
    if resolved.placed.is_identity() {
        return Ok(resolved.shape.clone());
    }
    kernel
        .transform_shape(&resolved.shape, &resolved.placed, None)
        .map_err(kernel_error(what))
}

/// One selection measured (mitcad#981): in its body's coordinates, its
/// point and circle then moved into the design's, when a rigid motion
/// places it (sizes stay as they are); else on a placed copy.
fn measure_resolved<K: Kernel>(
    kernel: &K,
    resolved: &Resolved<K::Shape>,
    what: &str,
) -> Result<Measurement, ApiError> {
    let t = resolved.placed;
    if t.is_rigid() {
        let mut found = kernel
            .measure(&resolved.selection())
            .map_err(kernel_error(what))?;
        if !t.is_identity() {
            found.point = found.point.map(|p| t.apply_point(p));
            if let Some(circle) = found.circle.as_mut() {
                circle.center = t.apply_point(circle.center);
                circle.axis = t.apply_vector(circle.axis);
            }
        }
        return Ok(found);
    }
    let shape = placed_shape(kernel, resolved, what)?;
    kernel
        .measure(&Selection {
            shape: &shape,
            name: resolved.name.as_ref(),
        })
        .map_err(kernel_error(what))
}

/// Two selections in one frame, to measure between them (mitcad#981):
/// their bodies' own shapes when one rigid motion places both (two faces
/// of one body or occurrence), with that motion to move the points found
/// into the design's coordinates; else each placed in the design, with
/// the identity.
fn common_frame<K: Kernel>(
    kernel: &K,
    a: &Resolved<K::Shape>,
    b: &Resolved<K::Shape>,
    what: &str,
) -> Result<(K::Shape, K::Shape, Transform), ApiError> {
    if a.placed == b.placed && a.placed.is_rigid() {
        return Ok((a.shape.clone(), b.shape.clone(), a.placed));
    }
    Ok((
        placed_shape(kernel, a, what)?,
        placed_shape(kernel, b, what)?,
        Transform::IDENTITY,
    ))
}

/// What the kernel found between two selections in a common frame, moved
/// into the design's coordinates by `t` (distances and angles stay).
fn moved_separation(mut found: Separation, t: &Transform) -> Separation {
    if !t.is_identity() {
        found.on_a = t.apply_point(found.on_a);
        found.on_b = t.apply_point(found.on_b);
    }
    found
}

/// What [`Document::measure_picks`] finds for a measurement object
/// (mitcad#649).
#[derive(Clone)]
pub(super) struct MeasuredPicks {
    /// The only pick measured alone (its circle, a round face's or edge's).
    pub(super) single: Option<Measurement>,
    /// Between the two picks.
    pub(super) between: Option<Separation>,
    /// Each pick's plane, axis, line or point in the design's coordinates.
    pub(super) elements: [Option<Element>; 2],
    /// What an annotation's automatic place looks at (mitcad#894), in the
    /// design's coordinates: the corners of the bounding boxes of the
    /// picks' bodies (none for datums), and the unit normals of the planar
    /// faces at the picks (a planar face picked and the faces it meets,
    /// the two faces an edge lies between, the faces at a vertex).
    pub(super) extent: Vec<Vec3>,
    pub(super) faces: Vec<Vec3>,
    /// Each pick's own outline (mitcad#902): the corners of a picked
    /// face's box in the design's coordinates, where a plane's extension
    /// line leaves it; empty for other picks or where the kernel cannot
    /// say.
    pub(super) outlines: [Vec<Vec3>; 2],
}

/// How many measured picks [`MeasureCache`] keeps at least.
const CACHED_MEASURES: usize = 256;

/// Picks measured before (mitcad#716), by the fingerprint of what they
/// were measured from ([`Document::picks_key`]: the picks as named, their
/// bodies' shape versions and placements, their datums as placed). The
/// annotations are answered again after every command, and measuring two
/// large faces can take long, so only picks whose bodies, placements or
/// datums changed are measured again. Separate from modelling results:
/// measuring changes no state. Failures are not kept. The least recently
/// used go first, beyond [`CACHED_MEASURES`] or as many as the current
/// annotations use ([`MeasureCache::hold`]), whichever is more.
#[derive(Default)]
pub(crate) struct MeasureCache {
    entries: std::sync::Mutex<VecDeque<(u128, MeasuredPicks)>>,
    /// How many keys the current annotations use at most.
    held: std::sync::atomic::AtomicUsize,
}

impl MeasureCache {
    /// Keeps at least `keys` measured picks (mitcad#758): the annotations
    /// ask for theirs in the same order after every command, so a cache
    /// smaller than that would drop each key before it is asked for again
    /// and measure every annotation each time.
    pub(super) fn hold(&self, keys: usize) {
        self.held.store(keys, std::sync::atomic::Ordering::Relaxed);
    }

    fn found(&self, key: u128) -> Option<MeasuredPicks> {
        // A hit moves to the back.
        let mut entries = self.entries.lock().ok()?;
        let i = entries.iter().position(|(k, _)| *k == key)?;
        let entry = entries.remove(i)?;
        let picks = entry.1.clone();
        entries.push_back(entry);
        Some(picks)
    }

    fn keep(&self, key: u128, picks: MeasuredPicks) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|(k, _)| *k != key);
            entries.push_back((key, picks));
            let most = CACHED_MEASURES.max(self.held.load(std::sync::atomic::Ordering::Relaxed));
            while entries.len() > most {
                entries.pop_front();
            }
        }
    }
}

/// A measurement object's pick (mitcad#649) as a selection given as fields.
fn selection_arg(pick: &MeasurePick) -> SelectionArg {
    SelectionArg::Fields(Box::new(SelectionInput {
        body: pick.body,
        face: pick.face.clone(),
        edge: pick.edge.clone(),
        vertex: pick.vertex.clone(),
        datum: pick.datum.clone(),
        occurrence: pick.occurrence.as_ref().map(ToString::to_string),
        ..SelectionInput::default()
    }))
}

/// How many placements or occurrences an error lists at most
/// (mitcad#985).
const LISTED: usize = 10;

/// An origin datum by its id or its name, in any case (`xy`, `XY`, `x`,
/// `Origin`; mitcad#985).
fn origin_datum(id: &str) -> Option<OriginDatum> {
    let id = id.trim();
    OriginDatum::ALL
        .into_iter()
        .find(|origin| origin.id().eq_ignore_ascii_case(id))
}

/// Two names the same but for case.
fn same_name(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

/// A document's analyses for the application: Section Analysis and its
/// clipped bodies, prepared off the UI thread. An extension trait
/// (mitcad#777), so that the api can live in a crate above the model.
pub trait AnalysisApi<K: Kernel> {
    /// A query prepared to be answered later without the document, on any
    /// thread (mitcad#438): `{"query": "section", ...}`, Section Analysis'
    /// area and length, as [`Document::query`] answers it now. What it
    /// holds of the bodies stays as it is when the document changes, and
    /// shares no sub-shape with them ([`Kernel::detach_all`]), so that
    /// nothing this thread writes into the bodies meanwhile is read there.
    fn prepare_query(&self, json: &str) -> Result<PreparedQuery<K>, ApiError>;

    /// Answers an analysis query (`properties`, `measure`, `interference`,
    /// `section`, `compare_step`, `datums` or `recompute_times`) as pretty
    /// JSON or as text (`mitcad-cli`; the slowest features of a recompute
    /// first).
    fn analysis(&self, json: &str, as_json: bool) -> Result<String, ApiError>;

    /// A shape for display: a datum, a section, a clipped body or the
    /// overlap of two bodies (see `commands.md`). None when there is none
    /// (an empty section or no overlap).
    fn analysis_shape(&self, json: &str) -> Result<Option<K::Shape>, ApiError>;

    /// The `clip` shapes of [`Document::analysis_shape`] for several bodies
    /// at one plane, worked out together (the OCCT kernel clips them on
    /// all cores, mitcad#438): one result per body, in the request's order,
    /// each with the error `clip` would give it (see `commands.md`).
    fn analysis_clips(&self, json: &str) -> Result<Vec<Result<K::Shape, ApiError>>, ApiError>;

    /// [`Document::analysis_clips`] prepared to be worked out later without
    /// the document, on any thread (Section Analysis off the UI thread,
    /// mitcad#438): each body detached ([`Kernel::detach_all`]: a copy that
    /// shares no sub-shape with it, or the body itself when the plane
    /// passes it, its clip then told from its kept box), with the plane in
    /// its coordinates, or the error its clip gives. What it holds stays as
    /// it is when the document changes, and nothing this thread writes into
    /// the bodies meanwhile (the view's meshes, a model job's) is read
    /// there. A body the plane cuts comes back as what is left of its copy
    /// (the copy itself when the clip leaves it whole after all).
    fn prepare_clips(&self, json: &str) -> Result<PreparedClips<K>, ApiError>;
}

impl<K: Kernel> AnalysisApi<K> for Document<K> {
    fn prepare_query(&self, json: &str) -> Result<PreparedQuery<K>, ApiError> {
        let PreparableQuery::Section(query) = parse(read_json(json)?, "prepared query")?;
        Ok(PreparedQuery(self.prepare_section(&query, true)?))
    }

    fn analysis(&self, json: &str, as_json: bool) -> Result<String, ApiError> {
        let value = read_json(json)?;
        if as_json {
            let result: Value = serde_json::from_str(&self.query(json)?).expect("valid JSON");
            return Ok(serde_json::to_string_pretty(&result).expect("serializes") + "\n");
        }
        let query: TextQuery = parse(value, "analysis query")?;
        let mut text = String::new();
        match query {
            TextQuery::Properties(q) => {
                let result = self.properties_json(&q)?;
                for body in result["bodies"].as_array().into_iter().flatten() {
                    properties_text(&mut text, body);
                }
                let total = &result["total"];
                let _ = writeln!(
                    text,
                    "Total: mass {:.6} kg, volume {:.3} mm3, centre of mass {}",
                    total["mass"].as_f64().unwrap_or(0.0),
                    total["volume"].as_f64().unwrap_or(0.0),
                    point(vec3(&total["center_of_mass"]))
                );
            }
            TextQuery::Measure(q) => {
                let result = self.measure_json(&q)?;
                measurement_text(&mut text, &q.a.describe(), &result["a"]);
                if let Some(b) = &q.b {
                    measurement_text(&mut text, &b.describe(), &result["b"]);
                    let between = &result["between"];
                    let _ = writeln!(
                        text,
                        "Distance: {:.6} mm, from {} to {}{}",
                        between["distance"].as_f64().unwrap_or(0.0),
                        point(vec3(&between["on_a"])),
                        point(vec3(&between["on_b"])),
                        if between["inside"] == json!(true) {
                            " (inside)"
                        } else {
                            ""
                        }
                    );
                    if let Some(angle) = between["angle"].as_f64() {
                        let _ = writeln!(text, "Angle: {:.6} deg", angle.to_degrees());
                    }
                    if let Some(offset) = between["offset"].as_f64() {
                        let _ = writeln!(text, "Offset: {offset:.6} mm");
                    }
                    if let Some(d) = between["axis_distance"].as_f64() {
                        let _ = writeln!(
                            text,
                            "Axis distance: {d:.6} mm, from {} to {}",
                            point(vec3(&between["axis_points"][0])),
                            point(vec3(&between["axis_points"][1]))
                        );
                    }
                    if let Some(d) = between["center_distance"].as_f64() {
                        let _ = writeln!(text, "Centre distance: {d:.6} mm");
                    }
                }
            }
            TextQuery::Interference(q) => {
                let result = self.interference_json(&q)?;
                let pairs = result["pairs"].as_array().cloned().unwrap_or_default();
                let _ = writeln!(text, "Interferences: {}", pairs.len());
                for pair in pairs {
                    let _ = writeln!(
                        text,
                        "  {} and {}: {:.3} mm3",
                        pair["names"][0].as_str().unwrap_or_default(),
                        pair["names"][1].as_str().unwrap_or_default(),
                        pair["volume"].as_f64().unwrap_or(0.0)
                    );
                }
            }
            TextQuery::Section(q) => {
                let result = self.section_json(&q)?;
                for body in result["bodies"].as_array().into_iter().flatten() {
                    let _ = writeln!(
                        text,
                        "{} ({}): {} faces, area {:.3} mm2, {} edges, length {:.3} mm",
                        body["name"].as_str().unwrap_or_default(),
                        body["uid"].as_str().unwrap_or_default(),
                        body["faces"],
                        body["area"].as_f64().unwrap_or(0.0),
                        body["edges"],
                        body["length"].as_f64().unwrap_or(0.0)
                    );
                }
                // A plane past every body (mitcad#824).
                let count = |key: &str| result[key].as_u64().unwrap_or(0);
                if count("cut") == 0 && count("kept") + count("removed") > 0 {
                    let _ = writeln!(
                        text,
                        "The plane cuts no body: {} kept whole, {} removed whole",
                        count("kept"),
                        count("removed")
                    );
                }
            }
            TextQuery::CompareStep(q) => {
                let result = self.compare_step_json(&q)?;
                comparison_text(&mut text, &result);
            }
            TextQuery::Datums(q) => {
                for datum in self.datums_json(&q).as_array().into_iter().flatten() {
                    datum_text(&mut text, datum);
                }
            }
            TextQuery::RecomputeTimes => {
                let result = self.recompute_times_json();
                let ms = |v: &Value| v["ms"].as_f64().unwrap_or(0.0);
                let mut features = result["features"].as_array().cloned().unwrap_or_default();
                let _ = writeln!(
                    text,
                    "Recompute: {} features in {:.1} ms",
                    features.len(),
                    ms(&result)
                );
                features.sort_by(|a, b| ms(b).total_cmp(&ms(a)));
                for feature in features.iter().take(SLOWEST) {
                    let _ = writeln!(
                        text,
                        "  {} {} ({}): {:.1} ms",
                        feature["uid"].as_str().unwrap_or_default(),
                        feature["name"].as_str().unwrap_or_default(),
                        feature["type"].as_str().unwrap_or_default(),
                        ms(feature)
                    );
                }
            }
        }
        Ok(text)
    }

    fn analysis_shape(&self, json: &str) -> Result<Option<K::Shape>, ApiError> {
        let value = read_json(json)?;
        // A BUILD part before it is placed (mitcad#375).
        if value.get("shape").and_then(Value::as_str) == Some("build_preview")
            && let Value::Object(fields) = value
        {
            return self.build_preview_shape(fields);
        }
        // A part placed by a mount frame before it is (mitcad#525).
        if value.get("shape").and_then(Value::as_str) == Some("mount_preview")
            && let Value::Object(mut fields) = value
        {
            fields.remove("shape");
            return self.mount_preview_shape(fields);
        }
        let request: ShapeRequest = parse(value, "shape request")?;
        let kernel = self.kernel();
        match request {
            ShapeRequest::Datum { uid, size } => {
                let (_, name, datum) = self.datum_by_id(&uid)?;
                if !(size > 0.0 && size.is_finite()) {
                    return Err(ApiError(format!(
                        "the size must be greater than zero, got {size}"
                    )));
                }
                kernel
                    .datum_shape(&datum, size)
                    .map(Some)
                    .map_err(kernel_error(&name))
            }
            ShapeRequest::Section {
                plane,
                body,
                occurrence,
                curves,
            } => {
                let plane = self.analysis_body_plane(&plane, body, occurrence.as_ref())?;
                let section = kernel
                    .section(self.require_body(body)?, &plane)
                    .map_err(kernel_error("section"))?;
                let count = if curves {
                    section.edge_count
                } else {
                    section.face_count
                };
                Ok((count > 0).then_some(if curves {
                    section.curves
                } else {
                    section.faces
                }))
            }
            ShapeRequest::Clip {
                plane,
                body,
                occurrence,
            } => {
                let plane = self.analysis_body_plane(&plane, body, occurrence.as_ref())?;
                kernel
                    .clip(self.require_body(body)?, &plane)
                    .map(Some)
                    .map_err(kernel_error("section"))
            }
            ShapeRequest::Interference { a, b } => {
                let shapes = [self.require_body(a)?, self.require_body(b)?];
                let found = kernel
                    .interferences(&shapes, min_volume())
                    .map_err(kernel_error("interference"))?;
                Ok(found.into_iter().next().map(|i| i.shape))
            }
            ShapeRequest::FlatPattern { body } => Ok(self.flat_pattern_shape(body)),
        }
    }

    fn analysis_clips(&self, json: &str) -> Result<Vec<Result<K::Shape, ApiError>>, ApiError> {
        Ok(self.clip_inputs(json, false)?.run(self.kernel()))
    }

    fn prepare_clips(&self, json: &str) -> Result<PreparedClips<K>, ApiError> {
        self.clip_inputs(json, true)
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait AnalysisOps<K: Kernel> {
    fn datums_json(&self, query: &DatumsQuery) -> Value;

    fn datum_json(&self, query: &DatumQuery) -> Result<Value, ApiError>;

    fn properties_json(&self, query: &PropertiesQuery) -> Result<Value, ApiError>;

    /// What places a pick's component in the design (its occurrence's
    /// transform; none in the root): what an annotation keeps in the pick's
    /// own coordinates follows it with this (mitcad#702).
    fn pick_transform(&self, pick: &MeasurePick) -> Result<Transform, ApiError>;

    /// What a measurement object's picks give at the marker (mitcad#649,
    /// `measurements.rs`): placed in the design as the `measure` query
    /// places them, the first measured alone when there is no second (its
    /// circle), else their separation, and each pick's plane, axis, line or
    /// point. Measured once, then taken from the [`MeasureCache`] while what
    /// they are measured from is the same (mitcad#716).
    fn measure_picks(
        &self,
        a: &MeasurePick,
        b: Option<&MeasurePick>,
    ) -> Result<MeasuredPicks, ApiError>;

    /// The fingerprint of what a measurement object's picks are measured
    /// from (mitcad#716), found without the kernel: each pick as named, its
    /// body's shape version (the recompute cache's, which changes whenever
    /// the shape does), component and placement, or its datum as placed.
    /// Picks of the same key measure the same. Fails as measuring would
    /// when a pick's body or datum is not there.
    fn picks_key(&self, a: &MeasurePick, b: Option<&MeasurePick>) -> Result<u128, ApiError>;

    fn measure_json(&self, query: &MeasureQuery) -> Result<Value, ApiError>;

    fn interference_json(&self, query: &InterferenceQuery) -> Result<Value, ApiError>;

    fn section_json(&self, query: &SectionQuery) -> Result<Value, ApiError>;

    fn compare_step_json(&self, query: &CompareStepQuery) -> Result<Value, ApiError>;

    /// Body attributes in the `bodies` query, when they are not the default.
    fn add_body_attributes(&self, uid: BodyUid, value: &mut Value);
}

impl<K: Kernel> AnalysisOps<K> for Document<K> {
    fn datums_json(&self, query: &DatumsQuery) -> Value {
        let mut datums = Vec::new();
        if query.origin {
            for origin in OriginDatum::ALL {
                let mut value = datum_json(&origin.datum());
                value["uid"] = json!(origin.id());
                value["name"] = json!(origin.display_name());
                datums.push(value);
            }
        }
        for view in self.datums() {
            let mut value = datum_json(&view.datum);
            value["uid"] = json!(view.uid);
            value["name"] = json!(view.name);
            datums.push(value);
        }
        Value::Array(datums)
    }

    fn datum_json(&self, query: &DatumQuery) -> Result<Value, ApiError> {
        let (uid, name, datum) = self.datum_by_id(&query.uid)?;
        let mut value = datum_json(&datum);
        value["uid"] = json!(uid);
        value["name"] = json!(name);
        Ok(value)
    }

    fn properties_json(&self, query: &PropertiesQuery) -> Result<Value, ApiError> {
        if let Some(density) = query.density.filter(|d| !(*d > 0.0 && d.is_finite())) {
            return Err(ApiError(format!(
                "the density must be greater than zero, got {density}"
            )));
        }
        let mut bodies = Vec::new();
        let mut all = Vec::new();
        for BodyView { uid, name, shape } in self.chosen_bodies(&query.bodies)? {
            // Its own material, else its component's default (mitcad#640);
            // `density` stands for the default.
            let (made_of, source) = self.body_material(uid, None);
            let (material, density) = match query.densities.get(&uid) {
                Some(density) if *density > 0.0 && density.is_finite() => (Value::Null, *density),
                Some(density) => {
                    return Err(ApiError(format!(
                        "the density of {uid} must be greater than zero, got {density}"
                    )));
                }
                None => match (source, query.density) {
                    (mitcad_model::MaterialSource::Default, Some(density)) => {
                        (Value::Null, density)
                    }
                    _ => (json!(made_of.id), made_of.density),
                },
            };
            let properties = self
                .kernel()
                .physical_properties(shape, density)
                .map_err(kernel_error(&name))?;
            let mut value = properties_json(&properties);
            value["uid"] = json!(uid);
            value["name"] = json!(name);
            value["material"] = material;
            value["density"] = json!(density);
            bodies.push(value);
            all.push(properties);
        }
        let total = analysis::combine(&all);
        Ok(json!({
            "bodies": bodies,
            "total": {"volume": total.volume, "area": total.area, "mass": total.mass,
                      "center_of_mass": total.center_of_mass, "inertia": total.inertia},
        }))
    }

    fn pick_transform(&self, pick: &MeasurePick) -> Result<Transform, ApiError> {
        let owner = match (&pick.datum, pick.body) {
            (Some(id), None) => id
                .parse::<FeatureUid>()
                .ok()
                .and_then(|uid| self.feature(uid))
                .map(|entry| entry.component),
            (None, Some(body)) => self.body_component(body),
            _ => None,
        };
        self.selection_transform(owner, pick.occurrence.as_ref(), "the pick")
    }

    fn measure_picks(
        &self,
        a: &MeasurePick,
        b: Option<&MeasurePick>,
    ) -> Result<MeasuredPicks, ApiError> {
        let key = self.picks_key(a, b)?;
        if let Some(found) = self.side_cache::<MeasureCache>().found(key) {
            return Ok(found);
        }
        let found = self.measure_picks_now(a, b)?;
        self.side_cache::<MeasureCache>().keep(key, found.clone());
        Ok(found)
    }

    fn picks_key(&self, a: &MeasurePick, b: Option<&MeasurePick>) -> Result<u128, ApiError> {
        let mut fp = Fingerprint::new("measured-picks-v1");
        self.pick_key(&mut fp, a)?;
        fp.bool(b.is_some());
        if let Some(b) = b {
            self.pick_key(&mut fp, b)?;
        }
        Ok(fp.finish())
    }

    fn measure_json(&self, query: &MeasureQuery) -> Result<Value, ApiError> {
        let kernel = self.kernel();
        let a = self.resolve_selection(&query.a)?;
        let mut value = json!({
            "a": measurement_json(&measure_resolved(kernel, &a, &query.a.describe())?),
        });
        add_axis_json(&mut value["a"], a.element.as_ref());
        add_plane_json(&mut value["a"], a.element.as_ref());
        add_occurrence_json(&mut value["a"], a.occurrence.as_ref());
        if let Some(b_input) = &query.b {
            let b = self.resolve_selection(b_input)?;
            value["b"] = measurement_json(&measure_resolved(kernel, &b, &b_input.describe())?);
            add_axis_json(&mut value["b"], b.element.as_ref());
            add_plane_json(&mut value["b"], b.element.as_ref());
            add_occurrence_json(&mut value["b"], b.occurrence.as_ref());
            let (shape_a, shape_b, placed) = common_frame(kernel, &a, &b, "measure")?;
            let in_frame_a = Selection {
                shape: &shape_a,
                name: a.name.as_ref(),
            };
            let in_frame_b = Selection {
                shape: &shape_b,
                name: b.name.as_ref(),
            };
            let between = kernel
                .measure_between(&in_frame_a, &in_frame_b)
                .map_err(kernel_error("measure"))?;
            value["between"] = separation_json(&moved_separation(between, &placed));
            if query.face_range {
                if value["a"]["kind"] != "face" || value["b"]["kind"] != "face" {
                    return Err(ApiError("face_range needs two faces".into()));
                }
                // A failure to compute the optional maximum must not discard
                // the exact minimum, nor return an old maximum to the caller.
                value["between"]["maximum_distance"] = json!(
                    kernel
                        .maximum_face_gap(&in_frame_a, &in_frame_b)
                        .ok()
                        .flatten()
                );
            }
            if let (Some(ea), Some(eb)) = (&a.element, &b.element) {
                // Two parallel planes (planar faces or datum planes): their
                // distance along the normal (mitcad#646).
                if let (Some(pa), Some(pb)) = (element_plane(ea), element_plane(eb))
                    && let Some(offset) = plane_offset(&pa, &pb)
                {
                    value["between"]["offset"] = json!(offset);
                }
                // Axes, directions and normals (mitcad#647): the angle the
                // kernel has no answer for, and the axes' distances.
                let found = measure_axes::between(ea, eb, PARALLEL);
                if value["between"]["angle"].is_null()
                    && let Some(angle) = found.angle
                {
                    value["between"]["angle"] = json!(angle);
                }
                if let (Some(d), Some(points)) = (found.axis_distance, found.axis_points) {
                    value["between"]["axis_distance"] = json!(d);
                    value["between"]["axis_points"] = json!(points);
                }
                if let Some(d) = found.center_distance {
                    value["between"]["center_distance"] = json!(d);
                }
            }
        }
        if query.face_range && query.b.is_none() {
            return Err(ApiError("face_range needs two faces".into()));
        }
        Ok(value)
    }

    fn interference_json(&self, query: &InterferenceQuery) -> Result<Value, ApiError> {
        let bodies = self.chosen_bodies(&query.bodies)?;
        let shapes: Vec<&K::Shape> = bodies.iter().map(|b| b.shape).collect();
        let found = self
            .kernel()
            .interferences(&shapes, query.min_volume)
            .map_err(kernel_error("interference"))?;
        let pairs: Vec<Value> = found
            .iter()
            .map(|i| {
                json!({"a": bodies[i.first].uid, "b": bodies[i.second].uid, "volume": i.volume,
                       "names": [bodies[i.first].name, bodies[i.second].name]})
            })
            .collect();
        Ok(json!({"bodies": bodies.len(), "pairs": pairs}))
    }

    fn section_json(&self, query: &SectionQuery) -> Result<Value, ApiError> {
        self.prepare_section(query, false)?.run(self.kernel())
    }

    fn compare_step_json(&self, query: &CompareStepQuery) -> Result<Value, ApiError> {
        let bodies = self.chosen_bodies(&query.bodies)?;
        if bodies.is_empty() {
            return Err(ApiError("there are no bodies to compare".to_owned()));
        }
        let shapes: Vec<&K::Shape> = bodies.iter().map(|b| b.shape).collect();
        let mut options = CompareOptions::default();
        if let Some(samples) = query.samples {
            options.samples = samples;
        }
        if let Some(fuzzy) = query.fuzzy {
            options.fuzzy = fuzzy;
        }
        options.step_body = match &query.step_body {
            None => None,
            Some(Value::String(name)) => Some(name.clone()),
            Some(Value::Number(index)) if index.is_u64() => Some(index.to_string()),
            Some(other) => {
                return Err(ApiError(format!(
                    "step_body is a body name or index, got {other}"
                )));
            }
        };
        let comparison = self
            .kernel()
            .compare_step(&shapes, &query.file, &options)
            .map_err(kernel_error(&query.file))?;
        let mut value = comparison_json(&comparison);
        value["bodies"] = json!(bodies.iter().map(|b| b.uid).collect::<Vec<_>>());
        if query.max_deviation.is_some() || query.max_relative.is_some() {
            let mut exceeded = Vec::new();
            let within = |limit: f64| comparison.max_deviation <= limit;
            if let Some(limit) = query.max_deviation.filter(|limit| !within(*limit)) {
                exceeded.push(format!(
                    "max deviation {} mm > {limit} mm",
                    comparison.max_deviation
                ));
            }
            if let Some(limit) = query.max_relative {
                match comparison.relative_difference {
                    Some(relative) if relative <= limit => {}
                    Some(relative) => {
                        exceeded.push(format!("relative difference {relative} > {limit}"))
                    }
                    None => exceeded.push("the volume difference failed".to_owned()),
                }
            }
            value["pass"] = json!(exceeded.is_empty());
            value["exceeded"] = json!(exceeded);
        }
        Ok(value)
    }

    fn add_body_attributes(&self, uid: BodyUid, value: &mut Value) {
        let attributes = self.body_attributes(uid);
        if !attributes.visible {
            value["visible"] = json!(false);
        }
        if let Some(material) = &attributes.material {
            value["material"] = json!(material);
        }
        if let Some(appearance) = &attributes.appearance {
            value["appearance"] = json!(appearance);
            // Not its material's: set by hand (mitcad#640).
            if self.appearance_set_by_hand(uid) {
                value["appearance_set_by_hand"] = json!(true);
            }
        }
        // Faces with their own appearance (mitcad#53), with the faces each
        // finds at the marker.
        let faces = self.face_appearances(uid);
        if !faces.is_empty() {
            value["face_appearances"] = json!(
                faces
                    .into_iter()
                    .map(|f| json!({"face": f.face, "appearance": f.appearance, "faces": f.faces}))
                    .collect::<Vec<_>>()
            );
        }
        // The print bed face (mitcad#731), with the faces it finds at the
        // marker or why it cannot lie on the bed.
        if let Some(bed) = self.print_bed_face_json(uid) {
            value["print_bed_face"] = bed;
        }
        // Bindings (mitcad#800): the expressions the values above follow,
        // the parameters they use, and why one gives no value.
        if attributes.has_bindings() {
            if let Some(text) = &attributes.appearance_expr {
                value["appearance_expr"] = json!(text);
            }
            if let Some(text) = &attributes.material_expr {
                value["material_expr"] = json!(text);
            }
            if let Some(text) = &attributes.visible_when {
                value["visible_when"] = json!(text);
            }
            if !attributes.face_appearance_exprs.is_empty() {
                value["face_appearance_exprs"] = json!(attributes.face_appearance_exprs);
            }
            value["dependencies"] = json!(self.state().body_binding_parameters(uid));
            let warnings = self.state().body_binding_warnings(uid);
            if !warnings.is_empty() {
                value["warnings"] = json!(warnings);
            }
        }
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait AnalysisLocal<K: Kernel> {
    /// The datum of a construction feature (`F3`) or an origin datum (`xy`).
    fn datum_by_id(&self, id: &str) -> Result<(String, String, Datum), ApiError>;

    /// The bodies asked for, or all at the marker; each must exist there.
    fn chosen_bodies(&self, uids: &[BodyUid]) -> Result<Vec<BodyView<'_, K::Shape>>, ApiError>;

    /// The fields of a selection given as text, or in the entity form
    /// (mitcad#985).
    fn selection_fields(&self, arg: &SelectionArg) -> Result<SelectionInput, ApiError>;

    /// A body by its uid (`F2.b0`) or its name at the marker.
    fn body_by_text(&self, text: &str) -> Result<BodyUid, ApiError>;

    /// The fields of a selection given in the entity form `ui_state` lists
    /// (`kind`, `owner`, `name`, `occurrence`; mitcad#985); the selection
    /// itself when given as fields.
    fn entity_fields(&self, input: &SelectionInput) -> Result<SelectionInput, ApiError>;

    /// An occurrence path from the root by occurrence ids (`O1/O4`), by
    /// the occurrences' names as the `components` query gives them
    /// (`Arm:1/Pin:2`), or both mixed (mitcad#985); `""` is the root. Ids
    /// alone are taken as they are (`selection_transform` checks them); a
    /// name that matches no occurrence where the path has got to, or
    /// several, is refused with the occurrences there. A name matches in
    /// any case, and a component's name alone matches its only occurrence
    /// there.
    fn occurrence_path(&self, text: &str) -> Result<OccurrencePath, ApiError>;

    /// Paths from the root for an error (mitcad#985): `O1/O4 (Arm:1/Pin:2)`
    /// each, at most [`LISTED`], then how many more.
    fn placements_text(&self, paths: &[Vec<OccurrenceUid>]) -> String;

    /// Where `what` of the component `owner` is placed, for an error
    /// (mitcad#985).
    fn placed_at(&self, owner: ComponentUid, what: &str) -> String;

    /// The transform of a selection's owner into the design (mitcad#646):
    /// that of the occurrence given (which must place `owner`), else of the
    /// owner's only placement; none in the root or for a component placed
    /// nowhere. A component placed more than once needs its occurrence.
    fn selection_transform(
        &self,
        owner: Option<ComponentUid>,
        occurrence: Option<&OccurrencePath>,
        what: &str,
    ) -> Result<Transform, ApiError>;

    /// A selection placed in the design (mitcad#646): a body's shape with
    /// its occurrence's transform (not applied: [`Resolved::placed`],
    /// mitcad#981), a datum moved, with its plane, axis, line or point in
    /// the design's coordinates (mitcad#647).
    fn resolve_selection(&self, arg: &SelectionArg) -> Result<Resolved<K::Shape>, ApiError>;

    /// The plane, axis, line or point of a face, edge or vertex for Measure
    /// (mitcad#647), in the body's coordinates: a cylinder's, cone's or
    /// torus's axis, a sphere's centre, a planar face's plane
    /// (`Kernel::face_plane`, its normal out of the material), a straight
    /// edge, a circle's or arc's axis through its centre, a vertex's
    /// point. None for other faces and curves.
    fn measure_element(&self, shape: &K::Shape, name: &TopoName) -> Option<Element>;

    fn pick_key(&self, fp: &mut Fingerprint, pick: &MeasurePick) -> Result<(), ApiError>;

    /// [`Document::measure_picks`] by the kernel.
    fn measure_picks_now(
        &self,
        a: &MeasurePick,
        b: Option<&MeasurePick>,
    ) -> Result<MeasuredPicks, ApiError>;

    /// What an annotation of these picks is placed by (mitcad#894,
    /// [`MeasuredPicks::extent`] and [`MeasuredPicks::faces`]): the corners
    /// of their bodies' boxes and the normals of the planar faces at them,
    /// in the design's coordinates. What the kernel cannot give is left
    /// out.
    fn pick_surroundings(&self, picks: &[&MeasurePick]) -> (Vec<Vec3>, Vec<Vec3>);

    /// A pick's own outline (mitcad#902, [`MeasuredPicks::outlines`]): the
    /// corners of the picked face's box, placed; empty for other picks.
    fn pick_outline(&self, pick: &MeasurePick) -> Vec<Vec3>;

    fn plane_of(&self, input: &GeomRef) -> Result<DatumPlane, ApiError>;

    /// An occurrence-aware display request receives a design-space plane
    /// and returns a local shape, ready for the caller's usual placement.
    fn analysis_body_plane(
        &self,
        input: &GeomRef,
        body: BodyUid,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<DatumPlane, ApiError>;

    /// The bodies `section` cuts and their planes, for
    /// [`PreparedSection::run`] (mitcad#438): here, or with `detach`
    /// without the document on another thread, the bodies then detached
    /// ([`Kernel::detach_all`]).
    fn prepare_section(
        &self,
        query: &SectionQuery,
        detach: bool,
    ) -> Result<PreparedSection<K>, ApiError>;

    /// The bodies `section` cuts, each with the plane in its coordinates.
    fn section_inputs(&self, query: &SectionQuery) -> Result<PreparedSection<K>, ApiError>;

    fn clip_inputs(&self, json: &str, detach: bool) -> Result<PreparedClips<K>, ApiError>;
}

impl<K: Kernel> AnalysisLocal<K> for Document<K> {
    fn datum_by_id(&self, id: &str) -> Result<(String, String, Datum), ApiError> {
        if let Some(origin) = origin_datum(id) {
            return Ok((
                origin.id().to_owned(),
                origin.display_name().to_owned(),
                origin.datum(),
            ));
        }
        let uid: FeatureUid = id.trim().parse().map_err(|_| {
            ApiError(format!(
                "invalid datum '{id}': a construction feature (\"F5\") or an origin datum \
                 (\"xy\", \"xz\", \"yz\", \"x\", \"y\", \"z\", \"origin\")"
            ))
        })?;
        let entry = self
            .feature(uid)
            .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
        let datum = self.datum(uid).ok_or_else(|| {
            ApiError(format!(
                "{} ({uid}) has no datum at the timeline marker",
                entry.name
            ))
        })?;
        Ok((uid.to_string(), entry.name.clone(), datum))
    }

    fn chosen_bodies(&self, uids: &[BodyUid]) -> Result<Vec<BodyView<'_, K::Shape>>, ApiError> {
        if uids.is_empty() {
            return Ok(self.bodies());
        }
        uids.iter()
            .map(|uid| {
                Ok(BodyView {
                    uid: *uid,
                    name: self.body_name(*uid),
                    shape: self.require_body(*uid)?,
                })
            })
            .collect()
    }

    fn selection_fields(&self, arg: &SelectionArg) -> Result<SelectionInput, ApiError> {
        let text = match arg {
            SelectionArg::Fields(fields) => return self.entity_fields(fields),
            SelectionArg::Text(text) => text,
        };
        if origin_datum(text).is_some() || text.trim().parse::<FeatureUid>().is_ok() {
            return Ok(SelectionInput {
                datum: Some(text.clone()),
                ..SelectionInput::default()
            });
        }
        let (body, name) = match text.split_once('/') {
            Some((body, name)) => (body, Some(name)),
            None => (text.as_str(), None),
        };
        let mut fields = SelectionInput {
            body: Some(self.body_by_text(body)?),
            ..SelectionInput::default()
        };
        match name.map(str::parse::<TopoName>).transpose() {
            Ok(None) => {}
            Ok(Some(TopoName::Face(face))) => fields.face = Some(face),
            Ok(Some(TopoName::Edge(edge))) => fields.edge = Some(edge),
            Ok(Some(TopoName::Vertex(vertex))) => fields.vertex = Some(vertex),
            Err(e) => return Err(ApiError(e.to_string())),
        }
        Ok(fields)
    }

    fn body_by_text(&self, text: &str) -> Result<BodyUid, ApiError> {
        let text = text.trim();
        match text.parse::<BodyUid>() {
            Ok(uid) => Ok(uid),
            Err(_) => self
                .bodies()
                .iter()
                .find(|b| b.name == text)
                .map(|b| b.uid)
                .ok_or_else(|| {
                    ApiError(format!("there is no body '{text}' at the timeline marker"))
                }),
        }
    }

    fn entity_fields(&self, input: &SelectionInput) -> Result<SelectionInput, ApiError> {
        let Some(kind) = input.kind.as_deref().map(str::trim) else {
            if input.owner.is_some() || input.name.is_some() {
                return Err(ApiError(
                    "a selection's \"owner\" and \"name\" go with its \"kind\" (the entity \
                     form ui_state lists)"
                        .to_owned(),
                ));
            }
            return Ok(input.clone());
        };
        if input.body.is_some()
            || input.face.is_some()
            || input.edge.is_some()
            || input.vertex.is_some()
            || input.datum.is_some()
        {
            return Err(ApiError(
                "a selection is an entity (\"kind\", \"owner\", \"name\", \"occurrence\") or \
                 fields (\"body\" with \"face\", \"edge\" or \"vertex\", or \"datum\", with \
                 \"occurrence\"), not both"
                    .to_owned(),
            ));
        }
        let owner = input
            .owner
            .as_deref()
            .map(str::trim)
            .filter(|owner| !owner.is_empty())
            .ok_or_else(|| ApiError(format!("the {kind} entity has no \"owner\"")))?;
        let name = input.name.as_deref().map(str::trim).unwrap_or_default();
        let mut fields = SelectionInput {
            occurrence: input.occurrence.clone(),
            ..SelectionInput::default()
        };
        match kind {
            "face" | "edge" | "vertex" => {
                fields.body = Some(self.body_by_text(owner)?);
                let wrong = || {
                    ApiError(format!(
                        "the {kind} entity's \"name\" '{name}' is no {kind} name of body {owner}"
                    ))
                };
                match (kind, name.parse::<TopoName>().map_err(|_| wrong())?) {
                    ("face", TopoName::Face(face)) => fields.face = Some(face),
                    ("edge", TopoName::Edge(edge)) => fields.edge = Some(edge),
                    ("vertex", TopoName::Vertex(vertex)) => fields.vertex = Some(vertex),
                    _ => return Err(wrong()),
                }
            }
            "body" | "plane" | "axis" | "point" if !name.is_empty() => {
                return Err(ApiError(format!(
                    "a {kind} entity has an empty \"name\", not '{name}'"
                )));
            }
            "body" => fields.body = Some(self.body_by_text(owner)?),
            "plane" | "axis" | "point" => fields.datum = Some(owner.to_owned()),
            other => {
                return Err(ApiError(format!(
                    "a {other} entity cannot be measured: a selection is a face, edge, vertex, \
                     body, plane, axis or point"
                )));
            }
        }
        Ok(fields)
    }

    fn occurrence_path(&self, text: &str) -> Result<OccurrencePath, ApiError> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(OccurrencePath::default());
        }
        let parts: Vec<&str> = text.split('/').map(str::trim).collect();
        if let Ok(ids) = parts
            .iter()
            .map(|part| part.parse::<OccurrenceUid>())
            .collect::<Result<Vec<_>, _>>()
        {
            return Ok(OccurrencePath(ids));
        }
        let assembly = self.assembly();
        let mut parent = ComponentUid::ROOT;
        let mut path: Vec<OccurrenceUid> = Vec::new();
        for part in parts {
            let children: Vec<(OccurrenceUid, ComponentUid)> = assembly
                .children(parent)
                .map(|o| (o.uid, o.component))
                .collect();
            let matching = |same: &dyn Fn(OccurrenceUid, ComponentUid) -> bool| {
                children
                    .iter()
                    .copied()
                    .filter(|(uid, component)| same(*uid, *component))
                    .collect::<Vec<_>>()
            };
            let mut found = matching(&|uid: OccurrenceUid, _: ComponentUid| {
                uid.to_string() == part || assembly.occurrence_name(uid) == part
            });
            if found.is_empty() {
                found = matching(&|uid: OccurrenceUid, _: ComponentUid| {
                    same_name(&assembly.occurrence_name(uid), part)
                });
            }
            if found.is_empty() {
                // A component's name alone: its only occurrence there.
                found = matching(&|_: OccurrenceUid, component: ComponentUid| {
                    same_name(&assembly.name(component), part)
                });
            }
            let listed = |among: &[(OccurrenceUid, ComponentUid)]| {
                let paths: Vec<Vec<OccurrenceUid>> = among
                    .iter()
                    .map(|(uid, _)| {
                        let mut full = path.clone();
                        full.push(*uid);
                        full
                    })
                    .collect();
                self.placements_text(&paths)
            };
            match found.as_slice() {
                [(uid, component)] => {
                    path.push(*uid);
                    parent = *component;
                }
                [] if children.is_empty() => {
                    return Err(ApiError(format!(
                        "invalid occurrence path '{text}': there is no occurrence '{part}', \
                         {} places no occurrences",
                        assembly.name(parent)
                    )));
                }
                [] => {
                    return Err(ApiError(format!(
                        "invalid occurrence path '{text}': there is no occurrence '{part}' in \
                         {}; its occurrences are {}",
                        assembly.name(parent),
                        listed(&children)
                    )));
                }
                several => {
                    return Err(ApiError(format!(
                        "invalid occurrence path '{text}': '{part}' in {} is any of {}; give \
                         one by its name or id",
                        assembly.name(parent),
                        listed(several)
                    )));
                }
            }
        }
        Ok(OccurrencePath(path))
    }

    fn placements_text(&self, paths: &[Vec<OccurrenceUid>]) -> String {
        let assembly = self.assembly();
        let listed: Vec<String> = paths
            .iter()
            .take(LISTED)
            .map(|path| {
                if path.is_empty() {
                    "\"\" (the root)".to_owned()
                } else {
                    format!(
                        "{} ({})",
                        OccurrencePath(path.clone()),
                        assembly.path_name(path)
                    )
                }
            })
            .collect();
        let text = listed.join(", ");
        if paths.len() > LISTED {
            format!(
                "{text}, … and {} more; the instances query lists them",
                paths.len() - LISTED
            )
        } else {
            text
        }
    }

    fn placed_at(&self, owner: ComponentUid, what: &str) -> String {
        if owner.is_root() {
            return format!("{what} is in the root: give no occurrence, or \"\"");
        }
        let paths = self.assembly().paths_to(owner);
        if paths.is_empty() {
            return format!(
                "{what} is in {}, which is placed nowhere in the design",
                self.assembly().name(owner)
            );
        }
        format!("{what} is placed at {}", self.placements_text(&paths))
    }

    fn selection_transform(
        &self,
        owner: Option<ComponentUid>,
        occurrence: Option<&OccurrencePath>,
        what: &str,
    ) -> Result<Transform, ApiError> {
        if let Some(path) = occurrence {
            // A path that does not exist, runs through the wrong parent or
            // ends elsewhere: the error names where the owner is placed
            // (mitcad#985).
            let wrong = match path_component(self.assembly(), ComponentUid::ROOT, &path.0) {
                Ok(component) if owner.is_none_or(|owner| owner == component) => {
                    return Ok(self.path_transform(&path.0));
                }
                Ok(_) => format!("occurrence '{path}' does not place {what}"),
                Err(e) => e,
            };
            return Err(ApiError(match owner {
                Some(owner) => format!("{wrong}; {}", self.placed_at(owner, what)),
                None => format!("{wrong}; the components query lists the occurrences"),
            }));
        }
        let Some(owner) = owner.filter(|owner| !owner.is_root()) else {
            return Ok(Transform::IDENTITY);
        };
        match self.assembly().paths_to(owner).as_slice() {
            [] => Ok(Transform::IDENTITY),
            [path] => Ok(self.path_transform(path)),
            paths => Err(ApiError(format!(
                "{what} is in a component placed {} times: {}; give one as occurrence",
                paths.len(),
                self.placements_text(paths)
            ))),
        }
    }

    fn resolve_selection(&self, arg: &SelectionArg) -> Result<Resolved<K::Shape>, ApiError> {
        let input = &self.selection_fields(arg)?;
        // By ids or names (mitcad#985); answered as ids.
        let occurrence = input
            .occurrence
            .as_deref()
            .map(|text| self.occurrence_path(text))
            .transpose()?;
        let names = [
            input.face.as_ref().map(|f| TopoName::Face(f.clone())),
            input.edge.as_ref().map(|e| TopoName::Edge(e.clone())),
            input.vertex.as_ref().map(|v| TopoName::Vertex(v.clone())),
        ];
        let mut names = names.into_iter().flatten();
        let name = names.next();
        if names.next().is_some() {
            return Err(ApiError(
                "a selection names one face, edge or vertex".to_owned(),
            ));
        }
        match (&input.datum, input.body) {
            (Some(id), None) if name.is_none() => {
                let (_, label, datum) = self.datum_by_id(id)?;
                // A construction feature's datum is in its component's
                // coordinates; an origin datum in the occurrence's given.
                let owner = id
                    .trim()
                    .parse::<FeatureUid>()
                    .ok()
                    .and_then(|uid| self.feature(uid))
                    .map(|entry| entry.component);
                let transform = self.selection_transform(owner, occurrence.as_ref(), &label)?;
                let datum = moved_datum(&datum, &transform);
                let shape = self
                    .kernel()
                    .datum_shape(&datum, DATUM_EXTENT)
                    .map_err(kernel_error(&label))?;
                let element = match datum {
                    Datum::Plane(p) => Some(Element::Plane {
                        origin: p.origin,
                        normal: p.normal(),
                    }),
                    Datum::Axis(a) => axis_element(a.origin, a.direction, false),
                    Datum::Point(p) => Some(Element::Point(p.point)),
                };
                Ok(Resolved {
                    shape,
                    name: None,
                    element,
                    placed: Transform::IDENTITY,
                    occurrence,
                })
            }
            (None, Some(body)) => {
                let local = self.require_body(body)?;
                let transform = self.selection_transform(
                    self.body_component(body),
                    occurrence.as_ref(),
                    &format!("body {body}"),
                )?;
                // The pick's plane, axis, line or point in its body's
                // coordinates, then placed.
                let element = name
                    .as_ref()
                    .and_then(|name| self.measure_element(local, name))
                    .and_then(|element| moved_element(&element, &transform));
                // The body itself, placed only where a measurement needs it
                // (placed_shape, mitcad#981).
                Ok(Resolved {
                    shape: local.clone(),
                    name,
                    element,
                    placed: transform,
                    occurrence,
                })
            }
            _ => Err(ApiError(
                "a selection is {\"body\": ...} with an optional \"face\", \"edge\" or \
                 \"vertex\", or {\"datum\": ...}, each with an optional \"occurrence\", or an \
                 entity as ui_state lists it ({\"kind\", \"owner\", \"name\", \"occurrence\"})"
                    .to_owned(),
            )),
        }
    }

    fn measure_element(&self, shape: &K::Shape, name: &TopoName) -> Option<Element> {
        let kernel = self.kernel();
        match name {
            TopoName::Face(face) => {
                let plane = || {
                    kernel.face_plane(shape, face).ok().map(|p| Element::Plane {
                        origin: p.origin,
                        normal: p.normal,
                    })
                };
                match kernel.face_geometry(shape, face) {
                    Ok(
                        SurfaceGeometry::Cylinder { origin, axis, .. }
                        | SurfaceGeometry::Cone { origin, axis, .. }
                        | SurfaceGeometry::Torus {
                            center: origin,
                            axis,
                            ..
                        },
                    ) => axis_element(origin, axis, false),
                    Ok(SurfaceGeometry::Sphere { center, .. }) => Some(Element::Point(center)),
                    Ok(SurfaceGeometry::Plane { origin, normal }) => {
                        plane().or(Some(Element::Plane { origin, normal }))
                    }
                    _ => plane(),
                }
            }
            TopoName::Edge(edge) => match kernel.edge_geometry(shape, edge).ok()? {
                CurveGeometry::Line { start, end } => Some(Element::Line { start, end }),
                CurveGeometry::Circle { center, normal, .. } => axis_element(center, normal, true),
                CurveGeometry::Other { .. } => None,
            },
            TopoName::Vertex(vertex) => kernel.vertex_point(shape, vertex).ok().map(Element::Point),
        }
    }

    fn pick_key(&self, fp: &mut Fingerprint, pick: &MeasurePick) -> Result<(), ApiError> {
        let named = serde_json::to_string(pick).map_err(|e| ApiError(e.to_string()))?;
        fp.str(&named);
        match (&pick.datum, pick.body) {
            (Some(id), None) => {
                // As `resolve_selection` places it.
                let (_, label, datum) = self.datum_by_id(id)?;
                let owner = id
                    .parse::<FeatureUid>()
                    .ok()
                    .and_then(|uid| self.feature(uid))
                    .map(|entry| entry.component);
                let transform =
                    self.selection_transform(owner, pick.occurrence.as_ref(), &label)?;
                fp.str(&format!("{:?}", moved_datum(&datum, &transform)));
            }
            (None, Some(body)) => {
                self.require_body(body)?;
                let owner = self.body_component(body);
                let transform = self.selection_transform(
                    owner,
                    pick.occurrence.as_ref(),
                    &format!("body {body}"),
                )?;
                fp.u128(self.body_version(body).unwrap_or(0))
                    .str(&format!("{owner:?}"));
                for row in &transform.linear {
                    for v in row {
                        fp.f64(*v);
                    }
                }
                for v in &transform.translation {
                    fp.f64(*v);
                }
            }
            // No pick: measuring it says why.
            _ => {}
        }
        Ok(())
    }

    fn measure_picks_now(
        &self,
        a: &MeasurePick,
        b: Option<&MeasurePick>,
    ) -> Result<MeasuredPicks, ApiError> {
        let a_arg = selection_arg(a);
        let ra = self.resolve_selection(&a_arg)?;
        let kernel = self.kernel();
        let Some(b) = b else {
            let single = measure_resolved(kernel, &ra, &a_arg.describe())?;
            let (extent, faces) = self.pick_surroundings(&[a]);
            return Ok(MeasuredPicks {
                single: Some(single),
                between: None,
                elements: [ra.element, None],
                extent,
                faces,
                outlines: [self.pick_outline(a), Vec::new()],
            });
        };
        let rb = self.resolve_selection(&selection_arg(b))?;
        let (shape_a, shape_b, placed) = common_frame(kernel, &ra, &rb, "measure")?;
        let between = kernel
            .measure_between(
                &Selection {
                    shape: &shape_a,
                    name: ra.name.as_ref(),
                },
                &Selection {
                    shape: &shape_b,
                    name: rb.name.as_ref(),
                },
            )
            .map_err(kernel_error("measure"))?;
        let between = moved_separation(between, &placed);
        let (extent, faces) = self.pick_surroundings(&[a, b]);
        Ok(MeasuredPicks {
            single: None,
            between: Some(between),
            elements: [ra.element, rb.element],
            extent,
            faces,
            outlines: [self.pick_outline(a), self.pick_outline(b)],
        })
    }

    fn pick_surroundings(&self, picks: &[&MeasurePick]) -> (Vec<Vec3>, Vec<Vec3>) {
        let kernel = self.kernel();
        let mut extent = Vec::new();
        let mut normals: Vec<Vec3> = Vec::new();
        for pick in picks {
            let Some(body) = pick.body else {
                continue;
            };
            let Some(local) = self.body_shape(body) else {
                continue;
            };
            let Ok(transform) = self.selection_transform(
                self.body_component(body),
                pick.occurrence.as_ref(),
                &format!("body {body}"),
            ) else {
                continue;
            };
            // The body's box in its own coordinates, its corners placed.
            if let Ok(Some(bounds)) = kernel.bounding_box(local) {
                extent.extend(bounds.corners().map(|c| transform.apply_point(c)));
            }
            let mut faces: Vec<FaceName> = Vec::new();
            if let Some(face) = &pick.face {
                faces.push(face.clone());
                // The faces it meets: those its edges lie between.
                for edge in kernel.edges(local).unwrap_or_default() {
                    let Some(name) = edge.name.and_then(|n| n.parse::<EdgeName>().ok()) else {
                        continue;
                    };
                    let [first, second] = name.faces();
                    if first == face && second != face {
                        faces.push(second.clone());
                    } else if second == face && first != face {
                        faces.push(first.clone());
                    }
                }
            }
            if let Some(edge) = &pick.edge {
                faces.extend(edge.faces().iter().cloned());
            }
            if let Some(vertex) = &pick.vertex {
                faces.extend(vertex.faces().iter().cloned());
            }
            for face in &faces {
                let Ok(plane) = kernel.face_plane(local, face) else {
                    continue;
                };
                if let Some(normal) = mitcad_vec3::unit(transform.apply_vector(plane.normal))
                    && !normals
                        .iter()
                        .any(|n| (mitcad_vec3::dot(*n, normal).abs() - 1.0).abs() < 1e-12)
                {
                    normals.push(normal);
                }
            }
        }
        (extent, normals)
    }

    fn pick_outline(&self, pick: &MeasurePick) -> Vec<Vec3> {
        let (Some(body), Some(face)) = (pick.body, &pick.face) else {
            return Vec::new();
        };
        let Some(local) = self.body_shape(body) else {
            return Vec::new();
        };
        let Ok(transform) = self.selection_transform(
            self.body_component(body),
            pick.occurrence.as_ref(),
            &format!("body {body}"),
        ) else {
            return Vec::new();
        };
        let kernel = self.kernel();
        // The face and its pieces after splits, by the shape's face order.
        let indices: Vec<usize> = kernel
            .faces(local)
            .unwrap_or_default()
            .iter()
            .enumerate()
            .filter(|(_, info)| {
                info.names.iter().any(|name| {
                    name.parse::<FaceName>()
                        .is_ok_and(|named| named.matches(face))
                })
            })
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            return Vec::new();
        }
        kernel
            .face_bounding_boxes(local, &indices)
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .flat_map(|bounds| bounds.corners().collect::<Vec<_>>())
            .map(|corner| transform.apply_point(corner))
            .collect()
    }

    fn plane_of(&self, input: &GeomRef) -> Result<DatumPlane, ApiError> {
        self.resolve_plane(input).map_err(ApiError)
    }

    fn analysis_body_plane(
        &self,
        input: &GeomRef,
        body: BodyUid,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<DatumPlane, ApiError> {
        let Some(path) = occurrence else {
            return self.plane_of(input);
        };
        // As a selection's occurrence, the error names where the body is
        // placed (mitcad#985).
        let placed = |wrong: String| match self.body_component(body) {
            Some(component) => ApiError(format!(
                "{wrong}; {}",
                self.placed_at(component, &format!("body {body}"))
            )),
            None => ApiError(wrong),
        };
        let owner = path_component(self.assembly(), ComponentUid::ROOT, &path.0).map_err(placed)?;
        if self.body_component(body) != Some(owner) {
            return Err(placed(format!(
                "occurrence {path} does not own body {body}"
            )));
        }
        let plane = self.analysis_plane(input, None).map_err(ApiError)?;
        let inverse = mitcad_model::assembly::inverse(&self.path_transform(&path.0));
        Ok(DatumPlane {
            origin: inverse.apply_point(plane.origin),
            x_axis: inverse.apply_vector(plane.x_axis),
            y_axis: inverse.apply_vector(plane.y_axis),
        })
    }

    fn prepare_section(
        &self,
        query: &SectionQuery,
        detach: bool,
    ) -> Result<PreparedSection<K>, ApiError> {
        let mut prepared = self.section_inputs(query)?;
        if detach {
            let items: Vec<(&K::Shape, DatumPlane)> = prepared
                .items
                .iter()
                .filter_map(|item| match item {
                    SectionInput::Cut(shape, plane) => Some((shape, *plane)),
                    _ => None,
                })
                .collect();
            let detached = self.kernel().detach_all(&items);
            let planes: Vec<DatumPlane> = items.iter().map(|(_, plane)| *plane).collect();
            prepared.items = detached
                .into_iter()
                .zip(planes)
                .map(|(detached, plane)| match detached {
                    Ok(Some(copy)) => SectionInput::Cut(copy, plane),
                    Ok(None) => SectionInput::Passes,
                    Err(error) => SectionInput::Failed(error),
                })
                .collect();
        }
        Ok(prepared)
    }

    fn section_inputs(&self, query: &SectionQuery) -> Result<PreparedSection<K>, ApiError> {
        if query.world {
            // Validate explicit body filters even when no occurrence is visible.
            for body in &query.bodies {
                self.require_body(*body)?;
            }
            let plane = self.analysis_plane(&query.plane, None).map_err(ApiError)?;
            let fixed = GeomRef::FixedPlane {
                origin: plane.origin,
                normal: plane.normal(),
                x_axis: Some(plane.x_axis),
            };
            // Each occurrence's body with the plane in its coordinates, all
            // sectioned at once (on all cores in the OCCT kernel, mitcad#438).
            let mut bodies = Vec::new();
            let mut items = Vec::new();
            let mut sides = Vec::new();
            for instance in self.instances().into_iter().filter(|i| {
                i.visible && (query.bodies.is_empty() || query.bodies.contains(&i.body))
            }) {
                let occurrence = OccurrencePath(instance.path);
                let local = self.analysis_body_plane(&fixed, instance.body, Some(&occurrence))?;
                let shape = self.require_body(instance.body)?;
                sides.push(clip_side(self.kernel(), shape, &local));
                items.push(SectionInput::Cut(shape.clone(), local));
                bodies.push((
                    json!({"uid": instance.body, "name": instance.name,
                        "occurrence": occurrence, "component": instance.component}),
                    instance.name,
                ));
            }
            return Ok(PreparedSection {
                plane,
                bodies,
                items,
                sides,
            });
        }
        let plane = self.plane_of(&query.plane)?;
        let mut bodies = Vec::new();
        let mut items = Vec::new();
        let mut sides = Vec::new();
        for BodyView { uid, name, shape } in self.chosen_bodies(&query.bodies)? {
            sides.push(clip_side(self.kernel(), shape, &plane));
            items.push(SectionInput::Cut(shape.clone(), plane));
            bodies.push((json!({"uid": uid, "name": name}), name));
        }
        Ok(PreparedSection {
            plane,
            bodies,
            items,
            sides,
        })
    }

    fn clip_inputs(&self, json: &str, detach: bool) -> Result<PreparedClips<K>, ApiError> {
        let request: ClipsRequest = parse(read_json(json)?, "clips request")?;
        let mut items: Vec<Result<ClipInput<K>, ApiError>> = request
            .bodies
            .iter()
            .map(|target| -> Result<ClipInput<K>, ApiError> {
                let plane = match &target.transform {
                    Some(placement) => {
                        let plane = self.plane_of(&request.plane)?;
                        let inverse = mitcad_model::assembly::inverse(placement);
                        DatumPlane {
                            origin: inverse.apply_point(plane.origin),
                            x_axis: inverse.apply_vector(plane.x_axis),
                            y_axis: inverse.apply_vector(plane.y_axis),
                        }
                    }
                    None => self.analysis_body_plane(
                        &request.plane,
                        target.body,
                        target.occurrence.as_ref(),
                    )?,
                };
                let shape = if target.preview {
                    self.preview_body(target.body).ok_or_else(|| {
                        ApiError(format!("body {} is not in the preview", target.body))
                    })?
                } else {
                    self.require_body(target.body)?
                };
                Ok(ClipInput {
                    shape: shape.clone(),
                    plane,
                    detached: false,
                })
            })
            .collect();
        if detach {
            let bodies: Vec<(&K::Shape, DatumPlane)> = items
                .iter()
                .filter_map(|item| item.as_ref().ok().map(|input| (&input.shape, input.plane)))
                .collect();
            let detached = self.kernel().detach_all(&bodies);
            let mut detached = detached.into_iter();
            for item in &mut items {
                if item.is_err() {
                    continue;
                }
                match detached.next() {
                    Some(Ok(Some(copy))) => {
                        if let Ok(input) = item {
                            input.shape = copy;
                            input.detached = true;
                        }
                    }
                    Some(Ok(None)) => {}
                    Some(Err(error)) => *item = Err(kernel_error("section")(error)),
                    None => *item = Err(ApiError("not detached".to_owned())),
                }
            }
        }
        Ok(PreparedClips { items })
    }
}

/// A body of [`PreparedClips`]: the shape its clip cuts (a detached copy,
/// or the body itself) and the plane in its coordinates.
struct ClipInput<K: Kernel> {
    shape: K::Shape,
    plane: DatumPlane,
    detached: bool,
}

/// `analysis_clips` prepared from the document ([`Document::prepare_clips`],
/// mitcad#438): worked out without it, on any thread.
pub struct PreparedClips<K: Kernel> {
    items: Vec<Result<ClipInput<K>, ApiError>>,
}

impl<K: Kernel> PreparedClips<K> {
    /// How many bodies it clips.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// What the clip of body `index` reads: the shape and whether it is a
    /// detached copy (else the body itself, which the plane passes); none
    /// for a body with an error.
    pub fn input(&self, index: usize) -> Option<(&K::Shape, bool)> {
        match self.items.get(index) {
            Some(Ok(input)) => Some((&input.shape, input.detached)),
            _ => None,
        }
    }

    /// The clips, one per body in the request's order, each with the error
    /// `clip` would give it (see `commands.md`).
    pub fn run(&self, kernel: &K) -> Vec<Result<K::Shape, ApiError>> {
        let mut results = Vec::with_capacity(self.items.len());
        let mut items = Vec::new();
        let mut slots = Vec::new();
        for item in &self.items {
            match item {
                Ok(input) => {
                    slots.push(results.len());
                    items.push((&input.shape, input.plane));
                    results.push(Err(ApiError("not clipped".to_owned())));
                }
                Err(error) => results.push(Err(error.clone())),
            }
        }
        for (slot, clipped) in slots.into_iter().zip(kernel.clip_all(&items)) {
            results[slot] = clipped.map_err(kernel_error("section"));
        }
        results
    }
}

/// The queries [`Document::prepare_query`] prepares.
#[derive(Debug, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case")]
enum PreparableQuery {
    Section(SectionQuery),
}

/// A query prepared from the document ([`Document::prepare_query`],
/// mitcad#438): answered without it, on any thread.
pub struct PreparedQuery<K: Kernel>(PreparedSection<K>);

impl<K: Kernel> PreparedQuery<K> {
    /// The query's answer, as the document gave it when it was prepared.
    pub fn run(&self, kernel: &K) -> Result<Value, ApiError> {
        self.0.run(kernel)
    }
}

/// What `section` cuts: the plane (the world's or the bodies'), each body's
/// own fields of the answer with its name for errors, and each body's
/// input, and what the clip does with each body (told from its box when
/// it was prepared, mitcad#824).
struct PreparedSection<K: Kernel> {
    plane: DatumPlane,
    bodies: Vec<(Value, String)>,
    items: Vec<SectionInput<K>>,
    sides: Vec<ClipSide>,
}

/// What Section Analysis' clip does with a body (mitcad#824), as the
/// bridge's `analysis::clip_side` tells it from the body's box: keeps it
/// whole (its box behind the plane, touching it at most), removes it whole
/// (its box on the normal side, touching the plane at most), or cuts it
/// (the box crosses the plane, or the body has no box).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipSide {
    Cut,
    Kept,
    Removed,
}

impl ClipSide {
    fn as_str(self) -> &'static str {
        match self {
            ClipSide::Cut => "cut",
            ClipSide::Kept => "kept",
            ClipSide::Removed => "removed",
        }
    }
}

/// How far a box may reach past the plane and still lie on one side of it:
/// OCCT's `Precision::Confusion()`, which `analysis::clip_side` allows.
const CLIP_SIDE_TOLERANCE: f64 = 1e-7;

/// [`ClipSide`] of `shape` at `plane` (in the shape's coordinates), from
/// the kernel's box of the shape: the same box the clip reads in the OCCT
/// kernel, kept with the body once measured.
fn clip_side<K: Kernel>(kernel: &K, shape: &K::Shape, plane: &DatumPlane) -> ClipSide {
    let Ok(Some(bounds)) = kernel.bounding_box(shape) else {
        return ClipSide::Cut;
    };
    let normal = plane.normal();
    // The least and the greatest signed distance of the box's corners.
    let (mut low, mut high) = (0.0, 0.0);
    let corners = bounds.min.iter().zip(&bounds.max);
    for (((min, max), origin), n) in corners.zip(&plane.origin).zip(&normal) {
        let a = (min - origin) * n;
        let b = (max - origin) * n;
        low += a.min(b);
        high += a.max(b);
    }
    if high <= CLIP_SIDE_TOLERANCE {
        ClipSide::Kept
    } else if low >= -CLIP_SIDE_TOLERANCE {
        ClipSide::Removed
    } else {
        ClipSide::Cut
    }
}

/// A body of [`PreparedSection`].
enum SectionInput<K: Kernel> {
    /// The shape to section (the body, or a detached copy) with the plane
    /// in its coordinates.
    Cut(K::Shape, DatumPlane),
    /// The plane passes the body ([`Kernel::detach_all`]): an empty section.
    Passes,
    /// The body could not be detached.
    Failed(KernelError),
}

impl<K: Kernel> PreparedSection<K> {
    /// All the bodies sectioned at once (on all cores in the OCCT kernel,
    /// mitcad#438); the first body whose section fails is the error.
    fn run(&self, kernel: &K) -> Result<Value, ApiError> {
        let cut: Vec<(&K::Shape, DatumPlane)> = self
            .items
            .iter()
            .filter_map(|item| match item {
                SectionInput::Cut(shape, plane) => Some((shape, *plane)),
                _ => None,
            })
            .collect();
        let mut sections = kernel.section_all(&cut).into_iter();
        let (mut length, mut area) = (0.0, 0.0);
        let mut bodies = Vec::with_capacity(self.bodies.len());
        // How many bodies the clip cuts, keeps whole and removes whole
        // (mitcad#824): none cut tells the panel why the section is empty.
        let mut counts = [0usize; 3];
        for (((fields, name), item), side) in self.bodies.iter().zip(&self.items).zip(&self.sides) {
            let (edges, faces, body_length, body_area) = match item {
                SectionInput::Cut(..) => {
                    let section = sections
                        .next()
                        .unwrap_or(Err(KernelError::Failed("not sectioned".to_owned())))
                        .map_err(kernel_error(name))?;
                    (
                        section.edge_count,
                        section.face_count,
                        section.length,
                        section.area,
                    )
                }
                SectionInput::Passes => (0, 0, 0.0, 0.0),
                SectionInput::Failed(error) => return Err(kernel_error(name)(error.clone())),
            };
            length += body_length;
            area += body_area;
            let mut body = fields.clone();
            body["edges"] = json!(edges);
            body["faces"] = json!(faces);
            body["length"] = json!(body_length);
            body["area"] = json!(body_area);
            body["clip"] = json!(side.as_str());
            counts[match side {
                ClipSide::Cut => 0,
                ClipSide::Kept => 1,
                ClipSide::Removed => 2,
            }] += 1;
            bodies.push(body);
        }
        let [cut, kept, removed] = counts;
        Ok(
            json!({"plane": {"origin": self.plane.origin, "normal": self.plane.normal()},
                  "bodies": bodies, "length": length, "area": area,
                  "cut": cut, "kept": kept, "removed": removed}),
        )
    }
}

/// A datum moved by `t` (into the design, by its occurrence's transform).
fn moved_datum(datum: &Datum, t: &Transform) -> Datum {
    match datum {
        Datum::Plane(p) => Datum::Plane(DatumPlane {
            origin: t.apply_point(p.origin),
            x_axis: t.apply_vector(p.x_axis),
            y_axis: t.apply_vector(p.y_axis),
        }),
        Datum::Axis(a) => Datum::Axis(mitcad_model::datum::DatumAxis {
            origin: t.apply_point(a.origin),
            direction: t.apply_vector(a.direction),
        }),
        Datum::Point(p) => Datum::Point(mitcad_model::datum::DatumPoint {
            point: t.apply_point(p.point),
        }),
    }
}

/// An axis through `origin` along `direction` (made a unit vector), for
/// Measure (mitcad#647); `center`: `origin` is a circle's centre.
fn axis_element(origin: Vec3, direction: Vec3, center: bool) -> Option<Element> {
    Some(Element::Axis {
        origin,
        direction: mitcad_vec3::unit(direction)?,
        center,
    })
}

/// An element of Measure moved by `t` (mitcad#647).
fn moved_element(element: &Element, t: &Transform) -> Option<Element> {
    Some(match element {
        Element::Plane { origin, normal } => Element::Plane {
            origin: t.apply_point(*origin),
            normal: t.apply_vector(*normal),
        },
        Element::Line { start, end } => Element::Line {
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
        Element::Axis {
            origin,
            direction,
            center,
        } => return axis_element(t.apply_point(*origin), t.apply_vector(*direction), *center),
        Element::Point(p) => Element::Point(t.apply_point(*p)),
    })
}

/// The plane of a planar face or a datum plane.
pub(super) fn element_plane(element: &Element) -> Option<Plane> {
    match element {
        Element::Plane { origin, normal } => Some(Plane {
            origin: *origin,
            normal: *normal,
        }),
        _ => None,
    }
}

/// A selection's `axis` {`origin`, `direction`} in the measure query's
/// answer (mitcad#647), where it has one.
fn add_axis_json(value: &mut Value, element: Option<&Element>) {
    if let Some((origin, direction)) = element.and_then(Element::axis) {
        // + 0.0 turns -0.0 into 0.0.
        let v = |v: Vec3| v.map(|x| x + 0.0);
        value["axis"] = json!({"origin": v(origin), "direction": v(direction)});
    }
}

/// A planar face's or datum plane's `plane` {`origin`, `normal`} in the
/// measure query's answer (mitcad#714): the middle of the face on its
/// plane and its normal out of the material, as `axis` is a cylinder's.
fn add_plane_json(value: &mut Value, element: Option<&Element>) {
    if let Some(Element::Plane { origin, normal }) = element {
        // + 0.0 turns -0.0 into 0.0.
        let v = |v: &Vec3| v.map(|x| x + 0.0);
        value["plane"] = json!({"origin": v(origin), "normal": v(normal)});
    }
}

/// The occurrence a selection gave, as ids (mitcad#985): a path given by
/// names is answered resolved.
fn add_occurrence_json(value: &mut Value, occurrence: Option<&OccurrencePath>) {
    if let Some(path) = occurrence {
        value["occurrence"] = json!(path);
    }
}

/// The offset of two parallel planes (mitcad#646): the distance between
/// them along their normal, never negative; None unless their normals are
/// parallel or opposite within [`PARALLEL`].
pub(super) fn plane_offset(a: &Plane, b: &Plane) -> Option<f64> {
    use mitcad_vec3::{cross, dot, norm, sub, unit};
    let (na, nb) = (unit(a.normal)?, unit(b.normal)?);
    // The sine of the angle between the normal lines.
    if norm(cross(na, nb)) > PARALLEL.sin() {
        return None;
    }
    // + 0.0 turns -0.0 into 0.0.
    Some(dot(sub(b.origin, a.origin), na).abs() + 0.0)
}

fn vec3(value: &Value) -> Vec3 {
    std::array::from_fn(|i| value[i].as_f64().unwrap_or(f64::NAN))
}

fn properties_text(text: &mut String, body: &Value) {
    let number = |key: &str| body[key].as_f64().unwrap_or(f64::NAN);
    let material = match body["material"].as_str() {
        Some(id) => analysis::material(id).map_or(id, |m| m.name).to_owned(),
        None => "custom".to_owned(),
    };
    let _ = writeln!(
        text,
        "{} ({}): {material}, density {} g/cm3",
        body["name"].as_str().unwrap_or_default(),
        body["uid"].as_str().unwrap_or_default(),
        number("density")
    );
    let _ = writeln!(
        text,
        "    mass {:.6} kg, volume {:.3} mm3, area {:.3} mm2",
        number("mass"),
        number("volume"),
        number("area")
    );
    let _ = writeln!(
        text,
        "    centre of mass {}",
        point(vec3(&body["center_of_mass"]))
    );
    let inertia = &body["inertia"];
    let _ = writeln!(
        text,
        "    inertia at the centre of mass (kg mm2): Ixx {:.6}, Iyy {:.6}, Izz {:.6}, Ixy {:.6}, Iyz {:.6}, Ixz {:.6}",
        round(inertia[0][0].as_f64().unwrap_or(0.0)),
        round(inertia[1][1].as_f64().unwrap_or(0.0)),
        round(inertia[2][2].as_f64().unwrap_or(0.0)),
        round(inertia[0][1].as_f64().unwrap_or(0.0)),
        round(inertia[1][2].as_f64().unwrap_or(0.0)),
        round(inertia[0][2].as_f64().unwrap_or(0.0))
    );
    let moments = vec3(&body["principal_moments"]);
    let _ = writeln!(
        text,
        "    principal moments (kg mm2): {:.6}, {:.6}, {:.6}",
        moments[0], moments[1], moments[2]
    );
}

fn measurement_text(text: &mut String, label: &str, m: &Value) {
    let mut parts = Vec::new();
    for (key, unit) in [("volume", "mm3"), ("area", "mm2"), ("length", "mm")] {
        if let Some(v) = m[key].as_f64() {
            parts.push(format!("{key} {v:.6} {unit}"));
        }
    }
    if m.get("point").is_some() {
        parts.push(format!("at {}", point(vec3(&m["point"]))));
    }
    if let Some(circle) = m.get("circle") {
        // The diameter (mitcad#194) from the radius before rounding.
        let radius = circle["radius"].as_f64().unwrap_or(0.0);
        parts.push(format!(
            "radius {radius:.6} mm, diameter {:.6} mm, centre {}",
            2.0 * radius,
            point(vec3(&circle["center"]))
        ));
    }
    if let Some(axis) = m.get("axis") {
        parts.push(format!(
            "axis through {} along {}",
            point(vec3(&axis["origin"])),
            point(vec3(&axis["direction"]))
        ));
    }
    let _ = writeln!(
        text,
        "{label} ({}): {}",
        m["kind"].as_str().unwrap_or_default(),
        parts.join(", ")
    );
}

fn comparison_text(text: &mut String, c: &Value) {
    let pass = c.get("pass").and_then(Value::as_bool);
    let number = |key: &str| c[key].as_f64();
    let _ = writeln!(
        text,
        "Compared with: {}",
        c["step_bodies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        text,
        "Volume: {:.3} mm3, STEP {:.3} mm3",
        number("volume").unwrap_or(0.0),
        number("step_volume").unwrap_or(0.0)
    );
    match number("relative_difference") {
        Some(relative) => {
            let _ = writeln!(
                text,
                "Only in Mitcad {:.6} mm3, only in STEP {:.6} mm3, relative difference {:.3e}",
                number("volume_only_mitcad").unwrap_or(0.0),
                number("volume_only_step").unwrap_or(0.0),
                relative
            );
        }
        None => {
            let _ = writeln!(text, "The volume difference could not be computed");
        }
    }
    let _ = writeln!(
        text,
        "Max deviation: {:.6} mm (Mitcad to STEP {:.6} mm, STEP to Mitcad {:.6} mm)",
        number("max_deviation").unwrap_or(0.0),
        c["mitcad_to_step"]["max"].as_f64().unwrap_or(0.0),
        c["step_to_mitcad"]["max"].as_f64().unwrap_or(0.0)
    );
    let _ = writeln!(
        text,
        "Bounding box difference: {:.6} mm",
        number("bounds_difference").unwrap_or(0.0)
    );
    match pass {
        Some(true) => {
            let _ = writeln!(text, "Within the limits");
        }
        Some(false) => {
            let exceeded: Vec<&str> = c["exceeded"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let _ = writeln!(text, "Limits exceeded: {}", exceeded.join(", "));
        }
        None => {}
    }
}

fn datum_text(text: &mut String, datum: &Value) {
    let label = format!(
        "{} ({}): {}",
        datum["name"].as_str().unwrap_or_default(),
        datum["uid"].as_str().unwrap_or_default(),
        datum["type"].as_str().unwrap_or_default()
    );
    let _ = match datum["type"].as_str() {
        Some("plane") => writeln!(
            text,
            "{label} at {}, normal {}",
            point(vec3(&datum["origin"])),
            point(vec3(&datum["normal"]))
        ),
        Some("axis") => writeln!(
            text,
            "{label} through {}, direction {}",
            point(vec3(&datum["origin"])),
            point(vec3(&datum["direction"]))
        ),
        _ => writeln!(text, "{label} at {}", point(vec3(&datum["point"]))),
    };
}
