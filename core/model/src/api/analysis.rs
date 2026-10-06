// SPDX-License-Identifier: MIT
//! Queries on construction geometry and analysis (F5): datums, physical
//! properties, measurement, interference, sections and comparison with a
//! STEP file, their text form for `mitcad-cli`, and shapes for display.
//! See `commands.md`.

use std::collections::BTreeMap;
use std::fmt::Write;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, parse, read_json};
use crate::analysis::{
    self, CompareOptions, Comparison, Measurement, PhysicalProperties, Selection, Separation,
};
use crate::datum::{Datum, DatumPlane, OriginDatum, Vec3};
use crate::document::{BodyView, Document};
use crate::features::geom_ref::GeomRef;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{Kernel, KernelError};
use crate::topo::{EdgeName, FaceName, TopoName, VertexName};

/// Size of the shapes that stand for datums in measurements, mm: large
/// enough to act as unbounded.
const DATUM_EXTENT: f64 = 1e5;

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
    #[serde(default)]
    datum: Option<String>,
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
        curves: bool,
    },
    /// A body without the half-space on the plane's normal side.
    Clip { plane: GeomRef, body: BodyUid },
    /// The overlap of two bodies.
    Interference { a: BodyUid, b: BodyUid },
}

fn datum_size() -> f64 {
    100.0
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
        let body = self.body.map(|b| b.to_string()).unwrap_or_default();
        match (&self.face, &self.edge, &self.vertex) {
            (Some(face), _, _) => format!("{body} face {face}"),
            (_, Some(edge), _) => format!("{body} edge {edge}"),
            (_, _, Some(vertex)) => format!("{body} vertex {vertex}"),
            _ => body,
        }
    }
}

/// A selection resolved to a shape and a name in it.
struct Resolved<S> {
    shape: S,
    name: Option<TopoName>,
}

impl<S> Resolved<S> {
    fn selection(&self) -> Selection<'_, S> {
        Selection {
            shape: &self.shape,
            name: self.name.as_ref(),
        }
    }
}

impl<K: Kernel> Document<K> {
    /// The datum of a construction feature (`F3`) or an origin datum (`xy`).
    fn datum_by_id(&self, id: &str) -> Result<(String, String, Datum), ApiError> {
        if let Some(origin) = OriginDatum::parse(id) {
            return Ok((
                origin.id().to_owned(),
                origin.display_name().to_owned(),
                origin.datum(),
            ));
        }
        let uid: FeatureUid = id.parse().map_err(|e| ApiError(format!("{e}")))?;
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

    pub(super) fn datums_json(&self, query: &DatumsQuery) -> Value {
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

    pub(super) fn datum_json(&self, query: &DatumQuery) -> Result<Value, ApiError> {
        let (uid, name, datum) = self.datum_by_id(&query.uid)?;
        let mut value = datum_json(&datum);
        value["uid"] = json!(uid);
        value["name"] = json!(name);
        Ok(value)
    }

    /// The bodies asked for, or all at the marker; each must exist there.
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

    pub(super) fn properties_json(&self, query: &PropertiesQuery) -> Result<Value, ApiError> {
        if let Some(density) = query.density.filter(|d| !(*d > 0.0 && d.is_finite())) {
            return Err(ApiError(format!(
                "the density must be greater than zero, got {density}"
            )));
        }
        let mut bodies = Vec::new();
        let mut all = Vec::new();
        for BodyView { uid, name, shape } in self.chosen_bodies(&query.bodies)? {
            let attributes = self.body_attributes(uid);
            let (material, density) = match query.densities.get(&uid) {
                Some(density) if *density > 0.0 && density.is_finite() => (Value::Null, *density),
                Some(density) => {
                    return Err(ApiError(format!(
                        "the density of {uid} must be greater than zero, got {density}"
                    )));
                }
                None => match (&attributes.material, query.density) {
                    (None, Some(density)) => (Value::Null, density),
                    _ => {
                        let material = attributes.material();
                        (json!(material.id), material.density)
                    }
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

    /// The fields of a selection given as text.
    fn selection_fields(&self, arg: &SelectionArg) -> Result<SelectionInput, ApiError> {
        let text = match arg {
            SelectionArg::Fields(fields) => return Ok(SelectionInput::clone(fields)),
            SelectionArg::Text(text) => text,
        };
        if OriginDatum::parse(text).is_some() || text.parse::<FeatureUid>().is_ok() {
            return Ok(SelectionInput {
                datum: Some(text.clone()),
                ..SelectionInput::default()
            });
        }
        let (body, name) = match text.split_once('/') {
            Some((body, name)) => (body, Some(name)),
            None => (text.as_str(), None),
        };
        let uid = match body.parse::<BodyUid>() {
            Ok(uid) => uid,
            Err(_) => self
                .bodies()
                .iter()
                .find(|b| b.name == body)
                .map(|b| b.uid)
                .ok_or_else(|| {
                    ApiError(format!("there is no body '{body}' at the timeline marker"))
                })?,
        };
        let mut fields = SelectionInput {
            body: Some(uid),
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

    fn resolve_selection(&self, arg: &SelectionArg) -> Result<Resolved<K::Shape>, ApiError> {
        let input = &self.selection_fields(arg)?;
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
                let shape = self
                    .kernel()
                    .datum_shape(&datum, DATUM_EXTENT)
                    .map_err(kernel_error(&label))?;
                Ok(Resolved { shape, name: None })
            }
            (None, Some(body)) => Ok(Resolved {
                shape: self.require_body(body)?.clone(),
                name,
            }),
            _ => Err(ApiError(
                "a selection is {\"body\": ...} with an optional \"face\", \"edge\" or \
                 \"vertex\", or {\"datum\": ...}"
                    .to_owned(),
            )),
        }
    }

    pub(super) fn measure_json(&self, query: &MeasureQuery) -> Result<Value, ApiError> {
        let a = self.resolve_selection(&query.a)?;
        let mut value = json!({
            "a": measurement_json(
                &self.kernel().measure(&a.selection()).map_err(kernel_error(&query.a.describe()))?
            ),
        });
        if let Some(b_input) = &query.b {
            let b = self.resolve_selection(b_input)?;
            value["b"] = measurement_json(
                &self
                    .kernel()
                    .measure(&b.selection())
                    .map_err(kernel_error(&b_input.describe()))?,
            );
            let between = self
                .kernel()
                .measure_between(&a.selection(), &b.selection())
                .map_err(kernel_error("measure"))?;
            value["between"] = separation_json(&between);
        }
        Ok(value)
    }

    pub(super) fn interference_json(&self, query: &InterferenceQuery) -> Result<Value, ApiError> {
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

    fn plane_of(&self, input: &GeomRef) -> Result<DatumPlane, ApiError> {
        self.resolve_plane(input).map_err(ApiError)
    }

    pub(super) fn section_json(&self, query: &SectionQuery) -> Result<Value, ApiError> {
        let plane = self.plane_of(&query.plane)?;
        let mut bodies = Vec::new();
        let (mut length, mut area) = (0.0, 0.0);
        for BodyView { uid, name, shape } in self.chosen_bodies(&query.bodies)? {
            let section = self
                .kernel()
                .section(shape, &plane)
                .map_err(kernel_error(&name))?;
            length += section.length;
            area += section.area;
            bodies.push(
                json!({"uid": uid, "name": name, "edges": section.edge_count,
                "faces": section.face_count, "length": section.length, "area": section.area}),
            );
        }
        Ok(
            json!({"plane": {"origin": plane.origin, "normal": plane.normal()},
                  "bodies": bodies, "length": length, "area": area}),
        )
    }

    pub(super) fn compare_step_json(&self, query: &CompareStepQuery) -> Result<Value, ApiError> {
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

    /// Body attributes in the `bodies` query, when they are not the default.
    pub(super) fn add_body_attributes(&self, uid: BodyUid, value: &mut Value) {
        let attributes = self.body_attributes(uid);
        if !attributes.visible {
            value["visible"] = json!(false);
        }
        if let Some(material) = &attributes.material {
            value["material"] = json!(material);
        }
        if let Some(appearance) = &attributes.appearance {
            value["appearance"] = json!(appearance);
        }
    }

    /// Answers an analysis query (`properties`, `measure`, `interference`,
    /// `section`, `compare_step`, `datums` or `recompute_times`) as pretty
    /// JSON or as text (`mitcad-cli`; the slowest features of a recompute
    /// first).
    pub fn analysis(&self, json: &str, as_json: bool) -> Result<String, ApiError> {
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

    /// A shape for display: a datum, a section, a clipped body or the
    /// overlap of two bodies (see `commands.md`). None when there is none
    /// (an empty section or no overlap).
    pub fn analysis_shape(&self, json: &str) -> Result<Option<K::Shape>, ApiError> {
        let request: ShapeRequest = parse(read_json(json)?, "shape request")?;
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
                curves,
            } => {
                let plane = self.plane_of(&plane)?;
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
            ShapeRequest::Clip { plane, body } => {
                let plane = self.plane_of(&plane)?;
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
        }
    }
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
        parts.push(format!(
            "radius {:.6} mm, centre {}",
            circle["radius"].as_f64().unwrap_or(0.0),
            point(vec3(&circle["center"]))
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
