// SPDX-License-Identifier: MIT
//! Project file versions 2 and 3 (see the module documentation of `file`):
//! version 3 is version 2 with B-rep data in a project's store.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{FORMAT, FileError, SINGLE_VERSION, VERSION, has_references, legacy_name};
use crate::assembly::{
    Assembly, ComponentDef, ExternalLink, Occurrence, ROOT_NAME, from_rows, matrix_rows,
};
use crate::document::{
    BodyAttributes, DisplayState, DocState, NamedView, TimelineGroup, check_components,
};
use crate::expr::{EvalContext, ParamSpec, TableError, Unit, value_to_expression};
use crate::features::{CheckContext, FeatureDef, FeatureEntry, is_false};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::parameters::{Parameters, default_context, slot_unit};
use crate::sketch::legacy::{LegacyShape, convert, settle};
use crate::transform::Transform;

// Writing: field order is the order in the file.

#[derive(Serialize)]
struct FileOut<'a> {
    format: &'static str,
    version: u64,
    units: UnitsOut,
    parameters: Vec<ParameterOut<'a>>,
    // Components and occurrences (F6), when there are any.
    #[serde(skip_serializing_if = "Option::is_none")]
    root_component: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    components: Vec<ComponentOut<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    occurrences: Vec<OccurrenceOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_component: Option<ComponentUid>,
    features: Vec<FeatureOut<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bodies: Vec<BodyOut<'a>>,
    // Named views (U5), when there are any.
    #[serde(skip_serializing_if = "<[NamedView]>::is_empty")]
    views: &'a [NamedView],
    // The Origin folder and Isolate (P9), when not the defaults.
    #[serde(skip_serializing_if = "DisplayState::is_default")]
    display: &'a DisplayState,
    // Timeline groups (P9), when there are any.
    #[serde(skip_serializing_if = "<[TimelineGroup]>::is_empty")]
    groups: &'a [TimelineGroup],
    #[serde(skip_serializing_if = "Option::is_none")]
    marker: Option<usize>,
}

#[derive(Serialize)]
struct ComponentOut<'a> {
    uid: ComponentUid,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_by: Option<FeatureUid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    link: Option<&'a ExternalLink>,
}

#[derive(Serialize)]
struct OccurrenceOut {
    uid: OccurrenceUid,
    component: ComponentUid,
    parent: ComponentUid,
    number: u32,
    /// Rows of the rotation and the translation; left out for the
    /// identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    transform: Option<[[f64; 4]; 3]>,
    #[serde(skip_serializing_if = "is_false")]
    grounded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    visible: Option<bool>,
}

fn is_root(component: &ComponentUid) -> bool {
    component.is_root()
}

#[derive(Serialize)]
struct UnitsOut {
    length: &'static str,
    angle: &'static str,
}

#[derive(Serialize)]
struct ParameterOut<'a> {
    name: &'a str,
    expression: &'a str,
    unit: String,
    /// The value when saved (mm, rad), for readers; loading evaluates the
    /// expression.
    value: f64,
    comment: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<FeatureUid>,
    // Favourite parameters (P9).
    #[serde(skip_serializing_if = "is_false")]
    favorite: bool,
}

#[derive(Serialize)]
struct FeatureOut<'a> {
    uid: FeatureUid,
    name: &'a str,
    #[serde(skip_serializing_if = "is_false")]
    suppressed: bool,
    #[serde(skip_serializing_if = "is_root")]
    component: ComponentUid,
    /// Set by the user for sketches and construction features (U3).
    #[serde(skip_serializing_if = "Option::is_none")]
    visible: Option<bool>,
    #[serde(flatten)]
    def: FeatureDef<String>,
}

#[derive(Serialize)]
struct BodyOut<'a> {
    uid: BodyUid,
    name: &'a str,
    // Attributes, when not the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    material: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    appearance: Option<&'a str>,
}

pub(super) fn save(state: &DocState) -> String {
    let parameters = &state.parameters;
    let context = parameters.context();
    let assembly = &state.assembly;
    let file = FileOut {
        format: FORMAT,
        // Version 3 only when base features refer to a project's B-rep store.
        version: if has_references(state) {
            VERSION
        } else {
            SINGLE_VERSION
        },
        units: UnitsOut {
            length: context.default_length_unit.symbol(),
            angle: context.default_angle_unit.symbol(),
        },
        parameters: parameters
            .iter()
            .map(|p| ParameterOut {
                name: p.name(),
                expression: p.expression(),
                unit: p.unit().to_string(),
                value: p.value(),
                comment: p.comment(),
                owner: parameters.owner(p.id()),
                favorite: state.favorites.contains(&p.id()),
            })
            .collect(),
        root_component: (assembly.root_name != ROOT_NAME).then_some(assembly.root_name.as_str()),
        components: assembly
            .components
            .iter()
            .map(|c| ComponentOut {
                uid: c.uid,
                name: &c.name,
                created_by: c.created_by,
                link: c.link.as_ref(),
            })
            .collect(),
        occurrences: assembly
            .occurrences
            .iter()
            .map(|o| OccurrenceOut {
                uid: o.uid,
                component: o.component,
                parent: o.parent,
                number: o.number,
                transform: (!o.transform.is_identity()).then(|| matrix_rows(&o.transform)),
                grounded: o.grounded,
                visible: (!o.visible).then_some(false),
            })
            .collect(),
        active_component: (!assembly.active.is_root()).then_some(assembly.active),
        features: state
            .features
            .iter()
            .map(|f| FeatureOut {
                uid: f.uid,
                name: &f.name,
                suppressed: f.suppressed,
                component: f.component,
                visible: state.feature_visibility.get(&f.uid).copied(),
                def: f
                    .def
                    .map_params(&mut |_, id| Ok::<_, ()>(parameters.name(*id)))
                    .expect("names never fail"),
            })
            .collect(),
        bodies: state
            .body_names
            .iter()
            .map(|(uid, name)| {
                let attributes = state.body_attributes.get(uid);
                BodyOut {
                    uid: *uid,
                    name,
                    visible: attributes.filter(|a| !a.visible).map(|_| false),
                    material: attributes.and_then(|a| a.material.as_deref()),
                    appearance: attributes.and_then(|a| a.appearance.as_deref()),
                }
            })
            .collect(),
        views: &state.named_views,
        display: &state.display,
        groups: &state.groups,
        marker: (state.marker < state.features.len()).then_some(state.marker),
    };
    let mut json =
        serde_json::to_string_pretty(&file).expect("the project file schema always serializes");
    json.push('\n');
    json
}

// Reading: the JSON value is walked, so every error can say where it is.

/// A parameter: `expression` and `unit`, or only a `value` (millimetres or
/// radians) in files written before expressions.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterIn {
    name: String,
    #[serde(default)]
    expression: Option<String>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    owner: Option<FeatureUid>,
    #[serde(default)]
    favorite: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitsIn {
    #[serde(default)]
    length: Option<String>,
    #[serde(default)]
    angle: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyIn {
    uid: BodyUid,
    name: String,
    #[serde(default = "visible")]
    visible: bool,
    #[serde(default)]
    material: Option<String>,
    #[serde(default)]
    appearance: Option<String>,
}

fn visible() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentIn {
    uid: ComponentUid,
    name: String,
    #[serde(default)]
    created_by: Option<FeatureUid>,
    #[serde(default)]
    link: Option<ExternalLink>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OccurrenceIn {
    uid: OccurrenceUid,
    component: ComponentUid,
    #[serde(default)]
    parent: ComponentUid,
    number: u32,
    #[serde(default)]
    transform: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    grounded: bool,
    #[serde(default = "visible")]
    visible: bool,
}

struct FeatureIn {
    uid: FeatureUid,
    name: String,
    suppressed: bool,
    component: ComponentUid,
    visible: Option<bool>,
    def: FeatureDef<String>,
    /// A sketch of rectangles and circles (`"shapes"`, early version 2):
    /// converted again with the parameter values once they are loaded, so
    /// that the stored positions are exact.
    shapes: Option<Value>,
}

const TOP_FIELDS: [&str; 14] = [
    "format",
    "version",
    "units",
    "parameters",
    "root_component",
    "components",
    "occurrences",
    "active_component",
    "features",
    "bodies",
    "marker",
    "views",
    "display",
    "groups",
];

fn schema(message: impl Into<String>) -> FileError {
    FileError::Schema(message.into())
}

fn invalid(message: impl Into<String>) -> FileError {
    FileError::Invalid(message.into())
}

fn array(
    top: &mut Map<String, Value>,
    field: &str,
    required: bool,
) -> Result<Vec<Value>, FileError> {
    match top.remove(field) {
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(schema(format!("\"{field}\" must be an array"))),
        None if required => Err(schema(format!("missing field `{field}`"))),
        None => Ok(Vec::new()),
    }
}

fn feature_in(i: usize, value: Value) -> Result<FeatureIn, FileError> {
    let Value::Object(mut fields) = value else {
        return Err(schema(format!("features[{i}]: expected an object")));
    };
    let text = |fields: &mut Map<String, Value>, field: &str| match fields.remove(field) {
        Some(Value::String(text)) => Ok(text),
        Some(_) => Err(schema(format!(
            "features[{i}]: \"{field}\" must be a string"
        ))),
        None => Err(schema(format!("features[{i}]: missing field `{field}`"))),
    };
    let uid_text = text(&mut fields, "uid")?;
    let name = text(&mut fields, "name")?;
    let uid = uid_text
        .parse()
        .map_err(|e| schema(format!("features[{i}] ({name}): {e}")))?;
    let suppressed = match fields.remove("suppressed") {
        None => false,
        Some(Value::Bool(value)) => value,
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"suppressed\" must be true or false"
            )));
        }
    };
    let component = match fields.remove("component") {
        None => ComponentUid::ROOT,
        Some(Value::String(text)) => text
            .parse()
            .map_err(|e| schema(format!("features[{i}] ({name}): {e}")))?,
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"component\" must be a component id"
            )));
        }
    };
    let visible = match fields.remove("visible") {
        None => None,
        Some(Value::Bool(value)) => Some(value),
        Some(_) => {
            return Err(schema(format!(
                "features[{i}] ({name}): \"visible\" must be true or false"
            )));
        }
    };
    let shapes = fields.get("shapes").cloned();
    let def = serde_json::from_value(Value::Object(fields))
        .map_err(|e| schema(format!("features[{i}] ({name}): {e}")))?;
    Ok(FeatureIn {
        uid,
        name,
        suppressed,
        component,
        visible,
        def,
        shapes,
    })
}

/// The components and occurrences of a file; files without them have
/// only the root component, which holds every feature.
fn assembly_in(top: &mut Map<String, Value>) -> Result<Assembly, FileError> {
    let mut assembly = Assembly::default();
    match top.remove("root_component") {
        None => {}
        Some(Value::String(name)) if !name.trim().is_empty() => assembly.root_name = name,
        Some(_) => return Err(schema("\"root_component\" must be a name")),
    }
    for (i, c) in array(top, "components", false)?.into_iter().enumerate() {
        let c: ComponentIn =
            serde_json::from_value(c).map_err(|e| schema(format!("components[{i}]: {e}")))?;
        if assembly.exists(c.uid) {
            return Err(invalid(format!(
                "components[{i}]: component {} is listed more than once",
                c.uid
            )));
        }
        assembly.next_component = assembly.next_component.max(c.uid.0 + 1);
        assembly.components.push(ComponentDef {
            uid: c.uid,
            name: c.name,
            created_by: c.created_by,
            link: c.link,
        });
    }
    for (i, o) in array(top, "occurrences", false)?.into_iter().enumerate() {
        let o: OccurrenceIn =
            serde_json::from_value(o).map_err(|e| schema(format!("occurrences[{i}]: {e}")))?;
        if assembly.occurrence(o.uid).is_some() {
            return Err(invalid(format!(
                "occurrences[{i}]: occurrence {} is listed more than once",
                o.uid
            )));
        }
        let transform = match &o.transform {
            None => Transform::IDENTITY,
            Some(rows) => from_rows(rows).map_err(|e| invalid(format!("occurrences[{i}]: {e}")))?,
        };
        assembly.next_occurrence = assembly.next_occurrence.max(o.uid.0 + 1);
        assembly.occurrences.push(Occurrence {
            uid: o.uid,
            component: o.component,
            parent: o.parent,
            transform,
            grounded: o.grounded,
            visible: o.visible,
            number: o.number,
        });
    }
    match top.remove("active_component") {
        None => {}
        Some(value) => {
            assembly.active = serde_json::from_value(value)
                .map_err(|e| schema(format!("active_component: {e}")))?;
        }
    }
    Ok(assembly)
}

pub(super) fn load(value: Value) -> Result<(DocState, Vec<String>), FileError> {
    let Value::Object(mut top) = value else {
        return Err(FileError::NotAProject);
    };
    if let Some(field) = top.keys().find(|k| !TOP_FIELDS.contains(&k.as_str())) {
        return Err(schema(format!(
            "unknown field `{field}`, expected one of {}",
            TOP_FIELDS.map(|f| format!("`{f}`")).join(", ")
        )));
    }
    let context = match top.remove("units") {
        None => default_context(),
        Some(units) => {
            let units: UnitsIn =
                serde_json::from_value(units).map_err(|e| schema(format!("units: {e}")))?;
            units_context(&units)?
        }
    };
    let parameters = array(&mut top, "parameters", true)?
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            serde_json::from_value::<ParameterIn>(p)
                .map_err(|e| schema(format!("parameters[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let assembly = assembly_in(&mut top)?;
    let features = array(&mut top, "features", true)?
        .into_iter()
        .enumerate()
        .map(|(i, f)| feature_in(i, f))
        .collect::<Result<Vec<_>, _>>()?;
    let bodies = array(&mut top, "bodies", false)?
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            serde_json::from_value::<BodyIn>(b).map_err(|e| schema(format!("bodies[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let views = array(&mut top, "views", false)?
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let view = serde_json::from_value::<NamedView>(v)
                .map_err(|e| schema(format!("views[{i}]: {e}")))?;
            view.check()
                .map_err(|e| invalid(format!("views[{i}]: {e}")))?;
            Ok(view)
        })
        .collect::<Result<Vec<_>, FileError>>()?;
    let display = match top.remove("display") {
        None => DisplayState::default(),
        Some(value) => serde_json::from_value::<DisplayState>(value)
            .map_err(|e| schema(format!("display: {e}")))?,
    };
    let groups = array(&mut top, "groups", false)?
        .into_iter()
        .enumerate()
        .map(|(i, g)| {
            serde_json::from_value::<TimelineGroup>(g)
                .map_err(|e| schema(format!("groups[{i}]: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let marker = match top.remove("marker") {
        None => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|m| usize::try_from(m).ok())
                .ok_or_else(|| schema("\"marker\" must be a feature count"))?,
        ),
    };

    let mut state = DocState::default();
    let mut warnings = Vec::new();
    let renamed = load_parameters(&mut state, context, &parameters, &features, &mut warnings)?;

    // Uids and names first, so references can tell a missing feature from a
    // later one.
    let mut uids = HashSet::new();
    let mut names = HashSet::new();
    for (i, f) in features.iter().enumerate() {
        if !uids.insert(f.uid) {
            return Err(invalid(format!(
                "features[{i}] ({}): uid {} is used more than once",
                f.name, f.uid
            )));
        }
        if f.name.trim().is_empty() {
            return Err(invalid(format!("features[{i}]: the name is empty")));
        }
        if !names.insert(f.name.as_str()) {
            return Err(invalid(format!(
                "features[{i}]: feature name '{}' is used more than once",
                f.name
            )));
        }
    }
    let mut entries = Vec::with_capacity(features.len());
    for (i, f) in features.iter().enumerate() {
        let params = &state.parameters;
        let mut def = f
            .def
            .map_params(&mut |slot, name: &String| {
                let actual = renamed.get(name).unwrap_or(name);
                params
                    .find(actual)
                    .ok_or_else(|| format!("{slot}: parameter '{name}' does not exist"))
            })
            .map_err(|e| invalid(format!("features[{i}] ({}): {e}", f.name)))?;
        if let (Some(shapes), FeatureDef::Sketch(sketch)) = (&f.shapes, &mut def)
            && let Ok(shapes) = serde_json::from_value::<Vec<LegacyShape<String>>>(shapes.clone())
        {
            let size = |name: &String| {
                params
                    .find(renamed.get(name).unwrap_or(name))
                    .and_then(|id| params.value(id))
            };
            let exact = convert(shapes, &size);
            for (entity, placed) in sketch.entities.iter_mut().zip(exact.entities) {
                entity.kind = placed.kind;
            }
            // Values that do not solve load and fail at recompute.
            let _ = settle(sketch, params);
        }
        entries.push(Arc::new(FeatureEntry {
            uid: f.uid,
            name: f.name.clone(),
            suppressed: f.suppressed,
            component: f.component,
            def,
        }));
        if let Some(visible) = f.visible {
            state.feature_visibility.insert(f.uid, visible);
        }
    }
    let feature_uids: BTreeSet<FeatureUid> = uids.iter().copied().collect();
    assembly.check(&feature_uids).map_err(invalid)?;
    for (i, entry) in entries.iter().enumerate() {
        let ctx = CheckContext {
            uid: entry.uid,
            earlier: &entries[..i],
            all: &entries,
        };
        entry
            .def
            .info()
            .check(&ctx)
            .and_then(|()| check_components(&assembly, &entries, entry))
            .map_err(|e| invalid(format!("features[{i}] ({}): {e}", entry.name)))?;
    }
    state.features = entries;
    state.assembly = assembly;

    for (i, p) in parameters.iter().enumerate() {
        if let Some(owner) = p.owner.filter(|o| !uids.contains(o)) {
            return Err(invalid(format!(
                "parameters[{i}]: the owner {owner} is not a feature of the file"
            )));
        }
    }
    for (i, body) in bodies.into_iter().enumerate() {
        if !uids.contains(&body.uid.feature) {
            return Err(invalid(format!(
                "bodies[{i}]: feature {} does not exist",
                body.uid.feature
            )));
        }
        if body.name.trim().is_empty() {
            return Err(invalid(format!("bodies[{i}]: the name is empty")));
        }
        if state.body_names.insert(body.uid, body.name).is_some() {
            return Err(invalid(format!(
                "bodies[{i}]: body {} is listed more than once",
                body.uid
            )));
        }
        if let Some(id) = body
            .material
            .as_deref()
            .filter(|id| crate::analysis::material(id).is_none())
        {
            return Err(invalid(format!("bodies[{i}]: unknown material '{id}'")));
        }
        let attributes = BodyAttributes {
            visible: body.visible,
            material: body.material,
            appearance: body.appearance,
        };
        if !attributes.is_default() {
            state.body_attributes.insert(body.uid, attributes);
        }
    }
    state.marker = match marker {
        Some(m) if m > state.features.len() => {
            return Err(invalid(format!(
                "the marker {m} is past the end of the timeline ({} features)",
                state.features.len()
            )));
        }
        Some(m) => m,
        None => state.features.len(),
    };
    state.next_uid = state
        .features
        .iter()
        .map(|f| f.uid.0 + 1)
        .max()
        .unwrap_or(1);
    for (i, view) in views.iter().enumerate() {
        if views[..i].iter().any(|v| v.name == view.name) {
            return Err(invalid(format!(
                "views[{i}]: named view '{}' is listed more than once",
                view.name
            )));
        }
    }
    state.named_views = views;
    state.display = display;
    for (i, group) in groups.iter().enumerate() {
        if let Some(missing) = group
            .features
            .iter()
            .find(|uid| state.position(**uid).is_none())
        {
            return Err(invalid(format!(
                "groups[{i}] ({}): feature {missing} does not exist",
                group.name
            )));
        }
    }
    state.groups = groups;
    state.normalize_groups();
    Ok((state, warnings))
}

/// The default units of a file.
fn units_context(units: &UnitsIn) -> Result<EvalContext, FileError> {
    let mut context = default_context();
    if let Some(text) = &units.length {
        context.default_length_unit = Unit::parse(text)
            .ok()
            .and_then(|u| match (u.length(), u.angle()) {
                (Some((unit, 1)), None) => Some(unit),
                _ => None,
            })
            .ok_or_else(|| invalid(format!("units: '{text}' is not a length unit")))?;
    }
    if let Some(text) = &units.angle {
        context.default_angle_unit = Unit::parse(text)
            .ok()
            .and_then(|u| match (u.length(), u.angle()) {
                (None, Some((unit, 1))) => Some(unit),
                _ => None,
            })
            .ok_or_else(|| invalid(format!("units: '{text}' is not an angle unit")))?;
    }
    Ok(context)
}

/// Builds the parameters. Files from before expressions store values: they
/// become expressions in millimetres, or degrees when a feature uses the
/// parameter as an angle, and names that are now reserved (`m`, `E`, `in`,
/// ...) are renamed with a warning. Returns the renames.
fn load_parameters(
    state: &mut DocState,
    context: EvalContext,
    parameters: &[ParameterIn],
    features: &[FeatureIn],
    warnings: &mut Vec<String>,
) -> Result<HashMap<String, String>, FileError> {
    let mut angles = HashSet::new();
    for f in features {
        let _ = f.def.map_params(&mut |slot, name: &String| {
            if slot_unit(slot) == Unit::DEG {
                angles.insert(name.clone());
            }
            Ok::<_, ()>(())
        });
    }
    let mut taken: HashSet<String> = parameters.iter().map(|p| p.name.clone()).collect();
    let mut renamed = HashMap::new();
    let mut seen = HashSet::new();
    let mut specs = Vec::with_capacity(parameters.len());
    for (i, p) in parameters.iter().enumerate() {
        let at = |message: String| invalid(format!("parameters[{i}]: {message}"));
        if p.name.is_empty() {
            return Err(at("the name is empty".to_owned()));
        }
        if !seen.insert(p.name.as_str()) {
            return Err(at(format!("parameter '{}' already exists", p.name)));
        }
        let spec = match (&p.expression, p.value) {
            (Some(expression), _) => {
                let unit = Unit::parse(p.unit.as_deref().unwrap_or(""))
                    .map_err(|e| at(format!("unit: {e}")))?;
                ParamSpec::user(&p.name, expression, unit)
            }
            (None, Some(value)) => {
                if p.unit.is_some() {
                    return Err(at("a unit needs an expression".to_owned()));
                }
                let unit = if angles.contains(&p.name) {
                    Unit::DEG
                } else {
                    Unit::MM
                };
                let name = match legacy_name(&p.name, &taken) {
                    Some(new) => {
                        warnings.push(format!(
                            "parameter '{}' is renamed to '{new}': '{}' is a unit, function \
                             or constant in expressions",
                            p.name, p.name
                        ));
                        taken.insert(new.clone());
                        renamed.insert(p.name.clone(), new.clone());
                        new
                    }
                    None => p.name.clone(),
                };
                ParamSpec::user(&name, &value_to_expression(value, unit), unit)
            }
            (None, None) => {
                return Err(schema(format!(
                    "parameters[{i}]: missing field `expression`"
                )));
            }
        };
        specs.push((spec.with_comment(&p.comment), p.owner));
    }
    let index_of = |name: &str| {
        parameters
            .iter()
            .position(|p| renamed.get(&p.name).unwrap_or(&p.name) == name)
    };
    state.parameters = Parameters::build(context, specs).map_err(|e| {
        let name = match &e {
            TableError::InvalidName(name)
            | TableError::ReservedName(name)
            | TableError::DuplicateName(name) => Some(name.as_str()),
            TableError::Parse { parameter, .. } | TableError::Eval { parameter, .. } => {
                Some(parameter.as_str())
            }
            _ => None,
        };
        match name.and_then(index_of) {
            Some(i) => invalid(format!("parameters[{i}]: {e}")),
            None => invalid(format!("parameters: {e}")),
        }
    })?;
    // Favourites (P9).
    for p in parameters.iter().filter(|p| p.favorite) {
        if let Some(id) = state
            .parameters
            .find(renamed.get(&p.name).unwrap_or(&p.name))
        {
            state.favorites.insert(id);
        }
    }
    Ok(renamed)
}
