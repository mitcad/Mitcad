// SPDX-License-Identifier: MIT
//! Project file version 1 (V0), converted to the current model on load.
//!
//! Version 1 named features by their names and gave them no uids: feature i
//! becomes `F<i+1>`. Sketch shapes get curve ids in order (a rectangle four,
//! a circle one), profiles are the regions of their shapes, a join or cut
//! names the body (its new_body extrude) it works on, and fillet edges were
//! named through an extruded profile:
//! `{"extrude": "Extrude2", "role": "side", "index": 1}` is the edge swept
//! from corner 1 of the profile Extrude2 extruded, and `start_cap` and
//! `end_cap` name profile edges at the sketch plane and at the far end. They
//! become edge names between the extrude's faces, e.g.
//! `E{F4:side(c5[c8,c6])|F4:side(c6[c5,c7])}`. Bodies are named Body1, ...
//! in the order of their extrudes.
//!
//! The checks keep version 1's own error messages, which name features.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use serde::Deserialize;

use super::{FileError, legacy_name};
use crate::document::DocState;
use crate::expr::{ParamSpec, Unit, value_to_expression};
use crate::features::{
    Extent, ExtrudeDef, FeatureDef, FeatureEntry, FilletDef, Operation, ProfileRef, SketchDef,
    SketchPlane,
};
use crate::ids::{BodyUid, ComponentUid, EntityUid, FeatureUid};
use crate::parameters::ParamId;
use crate::sketch::legacy::{LegacyShape, convert, settle};
use crate::sketch::regions::Region;
use crate::topo::{EdgeName, FaceName, RegionKey, SegmentKey};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    #[allow(dead_code)]
    format: String,
    #[allow(dead_code)]
    version: u64,
    parameters: Vec<FileParameter>,
    features: Vec<FileFeature>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileParameter {
    name: String,
    value: f64,
    #[serde(default)]
    comment: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum FileFeature {
    Sketch {
        name: String,
        shapes: Vec<FileShape>,
    },
    Extrude {
        name: String,
        sketch: String,
        profile: usize,
        distance: String,
        operation: FileOperation,
        body: Option<String>,
    },
    Fillet {
        name: String,
        body: String,
        edges: Vec<FileEdge>,
        radius: String,
    },
}

impl FileFeature {
    fn name(&self) -> &str {
        match self {
            Self::Sketch { name, .. } | Self::Extrude { name, .. } | Self::Fillet { name, .. } => {
                name
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum FileShape {
    Rectangle {
        corner: [f64; 2],
        width: String,
        height: String,
    },
    Circle {
        center: [f64; 2],
        diameter: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileOperation {
    NewBody,
    Join,
    Cut,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEdge {
    extrude: String,
    role: FileEdgeRole,
    index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileEdgeRole {
    Side,
    StartCap,
    EndCap,
}

impl fmt::Display for FileEdgeRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Side => "side edge",
            Self::StartCap => "start cap edge",
            Self::EndCap => "end cap edge",
        })
    }
}

pub(super) fn load(json: &str) -> Result<(DocState, Vec<String>), FileError> {
    let file: ProjectFile =
        serde_json::from_str(json).map_err(|e| FileError::Schema(e.to_string()))?;
    let mut state = DocState::default();
    // Values were millimetres. Names that are now units, functions or
    // constants in expressions get free names.
    let mut warnings = Vec::new();
    let mut taken: HashSet<String> = file.parameters.iter().map(|p| p.name.clone()).collect();
    let mut renamed = HashMap::new();
    for (i, p) in file.parameters.iter().enumerate() {
        let invalid = |message: String| FileError::Invalid(format!("parameters[{i}]: {message}"));
        if p.name.is_empty() {
            return Err(invalid("the name is empty".to_owned()));
        }
        let name = match legacy_name(&p.name, &taken) {
            Some(new) => {
                warnings.push(format!(
                    "parameter '{}' is renamed to '{new}': '{}' is a unit, function or \
                     constant in expressions",
                    p.name, p.name
                ));
                taken.insert(new.clone());
                renamed.insert(p.name.clone(), new.clone());
                new
            }
            None => p.name.clone(),
        };
        let spec = ParamSpec::user(&name, &value_to_expression(p.value, Unit::MM), Unit::MM)
            .with_comment(&p.comment);
        state
            .parameters
            .add(spec, None)
            .map_err(|e| invalid(e.to_string()))?;
    }

    // All names first, to tell a missing feature from a later one.
    let mut positions = HashMap::new();
    for (i, f) in file.features.iter().enumerate() {
        let invalid = |message: String| FileError::Invalid(format!("features[{i}]: {message}"));
        if f.name().is_empty() {
            return Err(invalid("the name is empty".to_owned()));
        }
        if positions.insert(f.name(), i).is_some() {
            return Err(invalid(format!(
                "feature name '{}' is used more than once",
                f.name()
            )));
        }
    }

    let mut converter = Converter {
        features: &file.features,
        positions,
        state,
        shapes: Vec::new(),
        regions: Vec::new(),
        renamed,
    };
    for (i, f) in file.features.iter().enumerate() {
        converter.convert(i, f).map_err(|message| {
            FileError::Invalid(format!("features[{i}] ({}): {message}", f.name()))
        })?;
    }
    let mut state = converter.state;

    // A dimension used by one feature is that feature's model parameter.
    let owners: Vec<(ParamId, FeatureUid)> = state
        .parameters
        .iter()
        .filter_map(|p| {
            let users: Vec<FeatureUid> = state
                .features
                .iter()
                .filter(|f| f.def.params().contains(&p.id()))
                .map(|f| f.uid)
                .collect();
            (users.len() == 1).then(|| (p.id(), users[0]))
        })
        .collect();
    for (id, owner) in owners {
        state.parameters.set_owner(id, Some(owner));
    }
    let mut count = 0;
    for (i, f) in file.features.iter().enumerate() {
        if matches!(
            f,
            FileFeature::Extrude {
                operation: FileOperation::NewBody,
                ..
            }
        ) {
            count += 1;
            state
                .body_names
                .insert(BodyUid::new(uid(i), 0), format!("Body{count}"));
        }
    }
    state.marker = state.features.len();
    state.next_uid = state.features.len() as u64 + 1;
    for i in 0..state.features.len() {
        let entry = state.features[i].clone();
        state.check_feature(i, &entry).map_err(|message| {
            FileError::Invalid(format!("features[{i}] ({}): {message}", entry.name))
        })?;
    }
    Ok((state, warnings))
}

fn uid(index: usize) -> FeatureUid {
    FeatureUid(index as u64 + 1)
}

/// The timeline converted so far.
struct Converter<'a> {
    features: &'a [FileFeature],
    positions: HashMap<&'a str, usize>,
    state: DocState,
    /// The converted shapes of each feature (sketches only).
    shapes: Vec<Vec<LegacyShape<ParamId>>>,
    /// The profile regions of each sketch.
    regions: Vec<Vec<Region>>,
    /// Parameters renamed on load.
    renamed: HashMap<String, String>,
}

impl Converter<'_> {
    fn parameter(&self, name: &str) -> Result<ParamId, String> {
        let actual = self.renamed.get(name).map_or(name, String::as_str);
        self.state
            .parameters
            .find(actual)
            .ok_or_else(|| format!("parameter '{name}' does not exist"))
    }

    /// A feature before the one being converted.
    fn earlier(&self, name: &str) -> Result<usize, String> {
        match self.positions.get(name) {
            None => Err(format!("feature '{name}' does not exist")),
            Some(&index) if index >= self.state.features.len() => Err(format!(
                "feature '{name}' does not come before this feature in the history"
            )),
            Some(&index) => Ok(index),
        }
    }

    fn name(&self, index: usize) -> &str {
        self.features[index].name()
    }

    /// A new_body extrude before this feature.
    fn body(&self, name: &str) -> Result<usize, String> {
        let index = self.earlier(name)?;
        match &self.features[index] {
            FileFeature::Extrude {
                operation: FileOperation::NewBody,
                ..
            } => Ok(index),
            _ => Err(format!(
                "'{}' is not a new_body extrude, so it is not a body",
                self.name(index)
            )),
        }
    }

    /// The sketch shape an extrude extruded.
    fn extruded_shape(&self, extrude: usize) -> Option<(FeatureUid, &LegacyShape<ParamId>)> {
        let FileFeature::Extrude {
            sketch, profile, ..
        } = &self.features[extrude]
        else {
            return None;
        };
        let sketch = self.positions[sketch.as_str()];
        Some((uid(sketch), self.shapes[sketch].get(*profile)?))
    }

    fn convert(&mut self, index: usize, feature: &FileFeature) -> Result<(), String> {
        let mut shapes = Vec::new();
        let mut regions = Vec::new();
        let def = match feature {
            FileFeature::Sketch { shapes: file, .. } => {
                let mut next = 1;
                for (i, shape) in file.iter().enumerate() {
                    shapes.push(
                        self.shape(shape, &mut next)
                            .map_err(|e| format!("shapes[{i}]: {e}"))?,
                    );
                }
                for shape in &shapes {
                    let (what, point) = match shape {
                        LegacyShape::Rectangle { corner, .. } => ("corner", corner),
                        LegacyShape::Circle { center, .. } => ("center", center),
                    };
                    if !point.iter().all(|v| v.is_finite()) {
                        return Err(format!("{what} must be a finite point, got {point:?}"));
                    }
                }
                let params = &self.state.parameters;
                let converted = convert(shapes.clone(), &|id| params.value(*id));
                let mut def = SketchDef {
                    entities: converted.entities,
                    constraints: converted.constraints,
                    dimensions: converted.dimensions,
                    ..SketchDef::new(SketchPlane::Xy)
                };
                // Stored positions are solved ones; the regions tell what
                // V0's overlapping shapes become.
                if let Ok(solved) = settle(&mut def, params) {
                    regions = def.regions_of(&solved, &[]);
                }
                FeatureDef::Sketch(def)
            }
            FileFeature::Extrude {
                sketch,
                profile,
                distance,
                operation,
                body,
                ..
            } => {
                let sketch = self.earlier(sketch)?;
                if !matches!(self.features[sketch], FileFeature::Sketch { .. }) {
                    return Err(format!("'{}' is not a sketch", self.name(sketch)));
                }
                let shape = self.shapes[sketch]
                    .get(*profile)
                    .ok_or_else(|| format!("{} has no profile {profile}", self.name(sketch)))?;
                // A shape on its own is its region. V0 did not split
                // overlapping shapes; the regions that make up the shape
                // give the same solid.
                let own = shape.region();
                let all = &self.regions[sketch];
                let profile_regions: Vec<RegionKey> =
                    if all.is_empty() || all.iter().any(|r| r.profile.key == own) {
                        vec![own]
                    } else {
                        let params = &self.state.parameters;
                        let inside: Vec<RegionKey> = all
                            .iter()
                            .filter(|r| shape.contains(&r.outline, &|id| params.value(*id)))
                            .map(|r| r.profile.key.clone())
                            .collect();
                        if inside.is_empty() { vec![own] } else { inside }
                    };
                let (operation, participants) = match (operation, body) {
                    (FileOperation::NewBody, None) => (Operation::NewBody, Vec::new()),
                    (FileOperation::NewBody, Some(_)) => {
                        return Err("a new_body extrude has no \"body\"".to_owned());
                    }
                    (FileOperation::Join | FileOperation::Cut, None) => {
                        return Err(
                            "a join or cut extrude needs the \"body\" it modifies".to_owned()
                        );
                    }
                    (operation, Some(body)) => {
                        let body = BodyUid::new(uid(self.body(body)?), 0);
                        let operation = if *operation == FileOperation::Join {
                            Operation::Join
                        } else {
                            Operation::Cut
                        };
                        (operation, vec![body])
                    }
                };
                FeatureDef::Extrude(ExtrudeDef {
                    profiles: profile_regions
                        .into_iter()
                        .map(|region| ProfileRef {
                            sketch: uid(sketch),
                            region,
                        })
                        .collect(),
                    start: Default::default(),
                    extent: Extent::Distance {
                        distance: self.parameter(distance)?,
                        taper: None,
                    },
                    thin: None,
                    flip: false,
                    operation,
                    participants,
                })
            }
            FileFeature::Fillet {
                body,
                edges,
                radius,
                ..
            } => {
                let body = self.body(body)?;
                if edges.is_empty() {
                    return Err("no edges".to_owned());
                }
                let mut names: Vec<EdgeName> = Vec::with_capacity(edges.len());
                let mut seen = Vec::new();
                for edge in edges {
                    let extrude = self.earlier(&edge.extrude)?;
                    let describe = format!("{} {} of {}", edge.role, edge.index, edge.extrude);
                    let on_chain = match &self.features[extrude] {
                        FileFeature::Extrude {
                            operation: FileOperation::NewBody,
                            ..
                        } => extrude == body,
                        FileFeature::Extrude {
                            body: Some(target), ..
                        } => self.positions.get(target.as_str()) == Some(&body),
                        _ => false,
                    };
                    if !on_chain {
                        return Err(format!(
                            "{describe} is not on the body of {}",
                            self.name(body)
                        ));
                    }
                    if seen.contains(&(extrude, edge.role, edge.index)) {
                        return Err(format!("{describe} is listed more than once"));
                    }
                    seen.push((extrude, edge.role, edge.index));
                    let name = self.edge(extrude, edge).ok_or_else(|| {
                        format!("{} has no {} {}", edge.extrude, edge.role, edge.index)
                    })?;
                    names.push(name);
                }
                FeatureDef::Fillet(FilletDef::constant(
                    BodyUid::new(uid(body), 0),
                    names,
                    self.parameter(radius)?,
                ))
            }
        };
        self.shapes.push(shapes);
        self.regions.push(regions);
        self.state.features.push(Arc::new(FeatureEntry {
            uid: uid(index),
            name: feature.name().to_owned(),
            suppressed: false,
            component: ComponentUid::ROOT,
            def,
        }));
        Ok(())
    }

    fn shape(&self, shape: &FileShape, next: &mut u32) -> Result<LegacyShape<ParamId>, String> {
        let mut curve = || {
            *next += 1;
            EntityUid(*next - 1)
        };
        Ok(match shape {
            FileShape::Rectangle {
                corner,
                width,
                height,
            } => LegacyShape::Rectangle {
                corner: *corner,
                width: self.parameter(width)?,
                height: self.parameter(height)?,
                curves: std::array::from_fn(|_| curve()),
            },
            FileShape::Circle { center, diameter } => LegacyShape::Circle {
                center: *center,
                diameter: self.parameter(diameter)?,
                curve: curve(),
            },
        })
    }

    /// The edge a version 1 reference names on the extrude's faces. Corner
    /// i of a rectangle is where lines i - 1 and i meet; profile edge i is
    /// line i.
    fn edge(&self, extrude: usize, edge: &FileEdge) -> Option<EdgeName> {
        let (_, shape) = self.extruded_shape(extrude)?;
        let feature = uid(extrude);
        let segments: Vec<SegmentKey> = shape.segments();
        let region: RegionKey = shape.region();
        let side = |i: usize| FaceName::side(feature, segments[i].clone());
        let count = segments.len();
        match (edge.role, count) {
            (FileEdgeRole::Side, 4) if edge.index < 4 => {
                Some(EdgeName::new(side((edge.index + 3) % 4), side(edge.index)))
            }
            (FileEdgeRole::StartCap, _) if edge.index < count => Some(EdgeName::new(
                side(edge.index),
                FaceName::start(feature, region),
            )),
            (FileEdgeRole::EndCap, _) if edge.index < count => Some(EdgeName::new(
                side(edge.index),
                FaceName::end(feature, region),
            )),
            _ => None,
        }
    }
}
