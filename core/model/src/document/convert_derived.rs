// SPDX-License-Identifier: MIT
//! Convert to Features (mitcad#1815, `commands.md`, "Convert to
//! Features"): a placed library part (`derived_part`) replaced in its
//! component by its source's own features, evaluated with the part's
//! configuration row and parameters, in one undo step. The design is left
//! as it was placed: every reference to the part's geometry is rewritten in
//! place to the new features' names, and nothing is made again.
//!
//! - The source's features are copied at the part's place in the timeline
//!   (Paste New's copy, `copy.rs`), their sketches' curve ids kept; the
//!   part's parameters stand for the source's parameters of the same names
//!   (a length a bound length drives keeps driving it), and a copied
//!   feature's own parameter that only names one of the part's own takes
//!   that parameter itself.
//! - Mitred ends become a construction plane at the end's angle (moved
//!   with the length for end B) and a box that cuts beyond it; joinery and
//!   rips a box cut each.
//! - `mount:<name>` joint origins are copied with their names.
//! - References: `<F>:part(<source face>)` becomes the copied feature's
//!   face, `<F>:end_a` and `<F>:end_b` the square end's face or the mitre
//!   box's bottom, a cut box's other faces that box's, `<F>:mount(<name>)`
//!   the copied joint origin and `<F>.b<i>` the i-th body. A reference no
//!   rule maps refuses the command with nothing changed.
//! - The component no longer follows its library: its BUILD record and
//!   library record go, and what its lists showed (designation, supplier,
//!   part number, material, grade, section and length) is stored as its
//!   properties.
//!
//! The converted design is computed before it is taken: a feature that
//! built and would fail, or an occurrence that would move, refuses it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::copy::{CopySource, PARAM, TopologyCopy, keep_measurement_sides};
use super::{DocState, Document, ModelError, invalid};
use crate::build::BuildMeta;
use crate::features::derived_part::{Tilt, bounds_of, end_faces, end_plane};
use crate::features::geom_ref::parse_mount;
use crate::features::joint::mount_of;
use crate::features::{BaseBody, BaseDef, Brep, FeatureDef, FeatureEntry, Operation, ValueInput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::notice::{FeatureError, NoticePlace};
use crate::parameters::ParamId;
use crate::properties::{LIBRARY_SET, PropertyOwner};
use crate::topo::{
    CurveId, EdgeName, FaceBoundaryRef, FaceName, RegionKey, RoleKey, SegmentKey, TopoName,
    VertexName,
};

/// The undo step's name.
pub const CONVERT_LABEL: &str = "Convert to Features";

/// What Convert to Features made (or, checked only, would make).
#[derive(Debug, Clone, PartialEq)]
pub struct ConvertedPart {
    /// The component the part was the first body of.
    pub component: ComponentUid,
    /// The features that took the part's place, in timeline order.
    pub features: Vec<(FeatureUid, String)>,
    /// The source's bodies have history (a section and an extrusion, or
    /// other features); false for a source of bodies without history (an
    /// imported part's), which becomes a plain body.
    pub history: bool,
    /// The part's bodies and the bodies that took their place.
    pub bodies: Vec<(BodyUid, BodyUid)>,
    /// The features elsewhere whose references were rewritten, by name.
    pub rewritten: Vec<String>,
}

/// A reference no rule maps: its text.
#[derive(Debug)]
struct Unmapped {
    name: String,
}

/// A reference no rule maps, with what holds it, for the refusal.
struct Lost {
    /// What holds it, in words: "a joint", "a drawing".
    kind: &'static str,
    /// Its name, for agents.
    holder: String,
    /// A joint or a constraint set, painted as its highlight shows it.
    joint: Option<FeatureUid>,
    unmapped: Unmapped,
}

impl Lost {
    fn of(kind: &'static str, holder: &str, unmapped: Unmapped) -> Self {
        Self {
            kind,
            holder: holder.to_owned(),
            joint: None,
            unmapped,
        }
    }
}

/// How references to the part become references to its features.
struct Rewrite<'a> {
    part: FeatureUid,
    /// The source's feature ids and their copies', as texts.
    sources: &'a HashMap<String, String>,
    /// A plain body's faces (a source without a section to extrude): the
    /// source's face names and the base feature's faces they became.
    plain: Option<&'a HashMap<String, FaceName>>,
    /// What the part itself (a bare `F7`) becomes: the feature of its first
    /// body.
    itself: FeatureUid,
    /// Body i of the part becomes `bodies[i]`.
    bodies: &'a [BodyUid],
    /// End A's and B's face: a square end's, or a mitre box's bottom.
    ends: [Option<FaceName>; 2],
    /// The boxes that cut the ends.
    end_boxes: [Option<FeatureUid>; 2],
    /// The boxes of the joinery and rips, in the part's order.
    joinery: &'a [FeatureUid],
    /// The copied `mount:<name>` joint origins, by name.
    mounts: &'a BTreeMap<String, FeatureUid>,
}

/// Fields that are texts, not references: of definitions, and the words
/// people wrote on comments, drawings and annotations.
const TEXT_FIELDS: [&str; 10] = [
    "name",
    "text",
    "font",
    "brep",
    "resolution",
    "author",
    "created",
    "comment",
    "description",
    "title",
];

impl Rewrite<'_> {
    fn part_face(&self, face: &FaceName) -> Result<FaceName, Unmapped> {
        let unmapped = || Unmapped {
            name: face.to_string(),
        };
        // A cut box's face: `<F>:cut_a(<F>:side0)` names the box's `side0`.
        let inner = || match &face.key {
            Some(RoleKey::Face(inner)) if inner.feature == self.part => Some(&**inner),
            _ => None,
        };
        let mut mapped = match face.role.as_str() {
            "part" => match (&face.key, self.plain) {
                // A face of the plain body, by its place among its faces.
                (Some(RoleKey::Face(source)), Some(plain)) => plain
                    .get(&source.to_string())
                    .cloned()
                    .ok_or_else(unmapped)?,
                // A face of a copied feature (not of one the copy left out).
                (Some(RoleKey::Face(source)), None)
                    if self.sources.contains_key(&source.feature.to_string()) =>
                {
                    TopologyCopy { uids: self.sources }.face(source)
                }
                _ => return Err(unmapped()),
            },
            "end_a" | "end_b" => self.ends[usize::from(face.role == "end_b")]
                .clone()
                .ok_or_else(unmapped)?,
            "cut_a" | "cut_b" => {
                let tool =
                    self.end_boxes[usize::from(face.role == "cut_b")].ok_or_else(unmapped)?;
                FaceName {
                    feature: tool,
                    ..inner().ok_or_else(unmapped)?.clone()
                }
            }
            role => {
                let tool = role
                    .strip_prefix("joint_")
                    .and_then(|k| k.parse::<usize>().ok())
                    .and_then(|k| self.joinery.get(k))
                    .ok_or_else(unmapped)?;
                FaceName {
                    feature: *tool,
                    ..inner().ok_or_else(unmapped)?.clone()
                }
            }
        };
        // Pieces split after the part (a hole through a side) stay pieces,
        // the outermost first.
        let mut split = face.split.clone();
        split.extend(mapped.split.iter().copied());
        mapped.split = split;
        Ok(mapped)
    }

    fn face(&self, face: &FaceName) -> Result<FaceName, Unmapped> {
        if face.feature == self.part {
            return self.part_face(face);
        }
        let mut mapped = face.clone();
        mapped.key = match &face.key {
            // Another derived part's source names are its source's own.
            Some(_) if face.role == "part" => face.key.clone(),
            Some(key) => Some(self.key(key)?),
            None => None,
        };
        Ok(mapped)
    }

    fn key(&self, key: &RoleKey) -> Result<RoleKey, Unmapped> {
        Ok(match key {
            RoleKey::Segment(segment) => RoleKey::Segment(self.segment(segment)?),
            RoleKey::Region(region) => RoleKey::Region(self.region(region)?),
            RoleKey::Edge(edge) => RoleKey::Edge(Box::new(self.edge(edge)?)),
            RoleKey::Vertex(vertex) => RoleKey::Vertex(Box::new(self.vertex(vertex)?)),
            RoleKey::Face(face) => RoleKey::Face(Box::new(self.face(face)?)),
            RoleKey::Index(_) => key.clone(),
        })
    }

    fn curve(&self, curve: &CurveId) -> Result<CurveId, Unmapped> {
        Ok(match curve {
            CurveId::FaceEdge(boundary) => CurveId::FaceEdge(Box::new(match &**boundary {
                FaceBoundaryRef::Named(edge) => FaceBoundaryRef::Named(self.edge(edge)?),
                FaceBoundaryRef::Profile { face, segment } => FaceBoundaryRef::Profile {
                    face: Box::new(self.face(face)?),
                    segment: Box::new(self.segment(segment)?),
                },
            })),
            _ => curve.clone(),
        })
    }

    fn curves(&self, curves: &[CurveId]) -> Result<Vec<CurveId>, Unmapped> {
        curves.iter().map(|c| self.curve(c)).collect()
    }

    fn segment(&self, segment: &SegmentKey) -> Result<SegmentKey, Unmapped> {
        let curve = self.curve(&segment.curve)?;
        let mut mapped = match &segment.ends {
            Some(ends) => {
                SegmentKey::with_ends(curve, self.curves(&ends.0)?, self.curves(&ends.1)?)
            }
            None => SegmentKey::closed(curve),
        };
        mapped.occurrence = segment.occurrence;
        Ok(mapped)
    }

    fn region(&self, region: &RegionKey) -> Result<RegionKey, Unmapped> {
        let segments = region
            .segments()
            .map(|s| self.segment(s))
            .collect::<Result<Vec<_>, _>>()?;
        RegionKey::new(segments).ok_or_else(|| Unmapped {
            name: region.to_string(),
        })
    }

    fn edge(&self, edge: &EdgeName) -> Result<EdgeName, Unmapped> {
        let [a, b] = edge.faces();
        let mut mapped = EdgeName::new(self.face(a)?, self.face(b)?);
        mapped.index = edge.index;
        Ok(mapped)
    }

    fn vertex(&self, vertex: &VertexName) -> Result<VertexName, Unmapped> {
        let faces = vertex
            .faces()
            .iter()
            .map(|f| self.face(f))
            .collect::<Result<Vec<_>, _>>()?;
        let mut mapped = VertexName::new(faces).ok_or_else(|| Unmapped {
            name: vertex.to_string(),
        })?;
        mapped.index = vertex.index;
        Ok(mapped)
    }

    /// A reference text rewritten: None when it does not name the part.
    fn text(&self, text: &str) -> Result<Option<String>, Unmapped> {
        if !mentions(text, self.part) {
            return Ok(None);
        }
        let unmapped = || Unmapped {
            name: text.to_owned(),
        };
        if let Ok(body) = text.parse::<BodyUid>() {
            if body.feature != self.part {
                return Ok(None);
            }
            return self
                .bodies
                .get(body.index as usize)
                .map(|b| Some(b.to_string()))
                .ok_or_else(unmapped);
        }
        if text.parse::<FeatureUid>().is_ok() {
            // The part itself: the feature of its first body.
            return Ok(Some(self.itself.to_string()));
        }
        if parse_mount(text).is_ok() {
            // A frame outside a geometry reference.
            return Err(unmapped());
        }
        if text.starts_with("r{")
            && let Ok(region) = text.parse::<RegionKey>()
        {
            return self.region(&region).map(|r| Some(r.to_string()));
        }
        match text.parse::<TopoName>() {
            Ok(TopoName::Face(face)) => self.face(&face).map(|f| Some(f.to_string())),
            Ok(TopoName::Edge(edge)) => self.edge(&edge).map(|e| Some(e.to_string())),
            Ok(TopoName::Vertex(vertex)) => self.vertex(&vertex).map(|v| Some(v.to_string())),
            Err(_) => Err(unmapped()),
        }
    }

    /// A geometry reference to one of the part's frames (`{"mount":
    /// "F7:mount(end_a)"}`): the copied joint origin's uid; None for any
    /// other value.
    fn frame(&self, value: &Value) -> Option<Result<String, Unmapped>> {
        let Value::Object(fields) = value else {
            return None;
        };
        if fields.len() != 1 {
            return None;
        }
        let Some(Value::String(mount)) = fields.get("mount") else {
            return None;
        };
        let (feature, name) = parse_mount(mount).ok()?;
        if feature != self.part {
            return None;
        }
        Some(
            self.mounts
                .get(&name)
                .map(|origin| origin.to_string())
                .ok_or_else(|| Unmapped {
                    name: mount.clone(),
                }),
        )
    }

    /// Rewrites the references to the part in a definition's JSON; whether
    /// anything changed. What cannot be mapped goes into `unmapped`.
    fn value(&self, value: &mut Value, unmapped: &mut Vec<Unmapped>) -> bool {
        // A frame of the part: the joint origin copied from it.
        match self.frame(value) {
            Some(Ok(origin)) => {
                *value = Value::String(origin);
                return true;
            }
            Some(Err(u)) => {
                unmapped.push(u);
                return false;
            }
            None => {}
        }
        match value {
            Value::String(text) if !text.starts_with(PARAM) => match self.text(text) {
                Ok(Some(new)) => {
                    *text = new;
                    true
                }
                Ok(None) => false,
                Err(u) => {
                    unmapped.push(u);
                    false
                }
            },
            Value::Array(items) => {
                let mut changed = false;
                for item in items {
                    changed |= self.value(item, unmapped);
                }
                changed
            }
            Value::Object(fields) => {
                let mut changed = false;
                let old = std::mem::take(fields);
                for (key, mut item) in old {
                    let key = match self.text(&key) {
                        Ok(Some(new)) => {
                            changed = true;
                            new
                        }
                        Ok(None) => key,
                        Err(u) => {
                            unmapped.push(u);
                            key
                        }
                    };
                    if !TEXT_FIELDS.contains(&key.as_str()) {
                        changed |= self.value(&mut item, unmapped);
                    }
                    fields.insert(key, item);
                }
                changed
            }
            _ => false,
        }
    }

    /// A definition with its references to the part rewritten: None when
    /// it names nothing of the part, or names something no rule maps (then
    /// in `unmapped`).
    fn def(
        &self,
        def: &FeatureDef,
        unmapped: &mut Vec<Unmapped>,
    ) -> Result<Option<FeatureDef>, ModelError> {
        let mut slots = Vec::new();
        let marked = def
            .map_params(&mut |_, id: &ParamId| {
                slots.push(*id);
                Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
            })
            .expect("never fails");
        let mut value = serde_json::to_value(&marked).map_err(|e| invalid(e.to_string()))?;
        let before = unmapped.len();
        if !self.value(&mut value, unmapped) || unmapped.len() > before {
            return Ok(None);
        }
        let rewritten: FeatureDef<String> =
            serde_json::from_value(value).map_err(|e| invalid(format!("{CONVERT_LABEL}: {e}")))?;
        let def = rewritten.map_params(&mut |slot, marker: &String| {
            marker
                .strip_prefix(PARAM)
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(|i| slots.get(i).copied())
                .ok_or_else(|| invalid(format!("{CONVERT_LABEL}: {slot} lost its parameter")))
        })?;
        Ok(Some(def))
    }

    /// One thing the design keeps besides its features (an annotation, a
    /// drawing, a BUILD record), its references to the part rewritten
    /// through its JSON; what no rule maps goes into `lost` as held by a
    /// `kind` named `holder`, and the item stays as it was.
    fn item<T: Serialize + DeserializeOwned>(
        &self,
        item: &mut T,
        kind: &'static str,
        holder: &str,
        lost: &mut Vec<Lost>,
    ) -> Result<(), ModelError> {
        let mut value = serde_json::to_value(&*item)
            .map_err(|e| invalid(format!("{CONVERT_LABEL}: {holder}: {e}")))?;
        let mut unmapped = Vec::new();
        let changed = self.value(&mut value, &mut unmapped);
        if !unmapped.is_empty() {
            lost.extend(unmapped.into_iter().map(|u| Lost::of(kind, holder, u)));
            return Ok(());
        }
        if changed {
            *item = serde_json::from_value(value)
                .map_err(|e| invalid(format!("{CONVERT_LABEL}: {holder}: {e}")))?;
        }
        Ok(())
    }

    /// [`Rewrite::item`] for each of a list, named by its place in it.
    fn items<T: Serialize + DeserializeOwned>(
        &self,
        items: &mut [T],
        kind: &'static str,
        lost: &mut Vec<Lost>,
    ) -> Result<(), ModelError> {
        for (i, item) in items.iter_mut().enumerate() {
            self.item(item, kind, &format!("{kind} {}", i + 1), lost)?;
        }
        Ok(())
    }
}

/// Whether a reference text names the feature `uid` (`F7`, `F7.b0`,
/// `F7:end_a`, inside an edge's name): an `F` that does not follow a
/// letter, digit or `_`, its digits followed by none. A derived part's
/// source names (`:part(...)`) are its source's ids and are skipped.
fn mentions(text: &str, uid: FeatureUid) -> bool {
    let id = uid.to_string();
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b":part(") {
            let mut depth = 0usize;
            let mut j = i + ":part".len();
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j;
            continue;
        }
        if bytes[i] == b'F' && (i == 0 || !word(bytes[i - 1])) {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && (j == bytes.len() || !word(bytes[j])) && text[i..j] == id {
                return true;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    false
}

/// What kind of reference a feature holds, in words, for the refusal.
fn kind_of(def: &FeatureDef) -> &'static str {
    match def {
        FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_) => "a joint",
        FeatureDef::ConstraintSet(_) => "an assembly constraint",
        FeatureDef::RigidGroup(_) => "a rigid group",
        FeatureDef::MotionLink(_) => "a motion link",
        FeatureDef::BoundLength(_) => "a bound length",
        FeatureDef::JointOrigin(_) => "a joint origin",
        FeatureDef::Sketch(_) | FeatureDef::SpatialSketch(_) => "a sketch",
        FeatureDef::ConstructionPlane(_)
        | FeatureDef::ConstructionAxis(_)
        | FeatureDef::ConstructionPoint(_) => "a construction feature",
        _ => "a feature",
    }
}

/// The place of an unmapped name, painted while the pointer is over the
/// refusal: a face, an edge or a vertex of the part.
fn place_of(name: &str, part: FeatureUid) -> Option<NoticePlace> {
    let body = Some(BodyUid::new(part, 0).to_string());
    match name.parse::<TopoName>().ok()? {
        TopoName::Face(face) => Some(NoticePlace::Face {
            body,
            name: face.to_string(),
        }),
        TopoName::Edge(edge) => Some(NoticePlace::Edge {
            body,
            name: edge.to_string(),
        }),
        TopoName::Vertex(vertex) => Some(NoticePlace::Vertex {
            body,
            name: vertex.to_string(),
        }),
    }
}

/// An occurrence's path of uids from the root (`O1/O4`), as a place names it.
fn occurrence_path(assembly: &crate::assembly::Assembly, occurrence: OccurrenceUid) -> String {
    let mut path = assembly
        .occurrence(occurrence)
        .and_then(|o| assembly.paths_to(o.parent).into_iter().next())
        .unwrap_or_default();
    path.push(occurrence);
    path.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// A refusal in words, its places and its detail for agents.
fn refusal(message: &str, places: Vec<NoticePlace>, detail: String) -> ModelError {
    invalid(
        FeatureError::new(format!("{CONVERT_LABEL}: {message}"))
            .with_places(places)
            .with_detail(format!("{CONVERT_LABEL}: {detail}"))
            .text(),
    )
}

impl<K: Kernel> Document<K> {
    /// The `derived_part` of a component: the first of its features.
    pub fn derived_part_of(&self, component: ComponentUid) -> Option<FeatureUid> {
        self.state
            .features
            .iter()
            .find(|f| f.component == component && matches!(f.def, FeatureDef::DerivedPart(_)))
            .map(|f| f.uid)
    }

    /// Replaces the derived part `part` with its source's features (see
    /// the module documentation); one undo step `Convert to Features`.
    /// With `check`, nothing changes: the answer says what it would make,
    /// or the refusal why not. `listed` is what the part's lists showed of
    /// it (BUILD's cut list row, opaque here), kept with its component so
    /// that the lists show it as before.
    pub fn convert_derived_part(
        &mut self,
        part: FeatureUid,
        check: bool,
        listed: Option<Value>,
    ) -> Result<ConvertedPart, ModelError> {
        let (mut next, report) = self.converted(part)?;
        if let Some(c) = next.assembly.component_mut(report.component) {
            c.listed = listed;
        }
        // The converted design at the marker: every feature that built
        // still builds, the new ones too, and no occurrence moves.
        let result = self.compute(&next)?;
        let mut failed = Vec::new();
        let mut places = Vec::new();
        let old_ok: BTreeSet<FeatureUid> = self
            .result
            .results
            .iter()
            .filter(|r| r.status.is_ok())
            .map(|r| r.uid)
            .collect();
        let new: BTreeSet<FeatureUid> = report.features.iter().map(|(uid, _)| *uid).collect();
        for f in &next.features[..next.marker] {
            if !old_ok.contains(&f.uid) && !new.contains(&f.uid) {
                continue;
            }
            if let Some(error) = result.result(f.uid).and_then(|r| r.status.failure()) {
                failed.push(format!("{} would fail: {}", f.name, error.agent_text()));
                places.extend(error.places.iter().cloned());
            }
        }
        if !failed.is_empty() {
            return Err(refusal(
                "a feature of the design would fail with the part's features",
                places,
                failed.join("; "),
            ));
        }
        let mut moved = Vec::new();
        for (occurrence, was) in &self.result.placements {
            let size = was.translation.iter().fold(1.0_f64, |m, v| m.max(v.abs()));
            let same = result.placements.get(occurrence).is_some_and(|now| {
                (0..3).all(|r| {
                    (was.translation[r] - now.translation[r]).abs() <= 1e-6 * size
                        && (0..3).all(|c| (was.linear[r][c] - now.linear[r][c]).abs() <= 1e-9)
                })
            });
            if !same {
                moved.push(*occurrence);
            }
        }
        if !moved.is_empty() {
            let names: Vec<String> = moved
                .iter()
                .map(|o| next.assembly.occurrence_name(*o))
                .collect();
            return Err(refusal(
                "a component would move: the part's features do not hold it where it is",
                moved
                    .iter()
                    .map(|o| NoticePlace::Occurrence {
                        path: occurrence_path(&next.assembly, *o),
                    })
                    .collect(),
                format!("{} would move", names.join(", ")),
            ));
        }
        if check {
            return Ok(report);
        }
        self.apply(move |state| {
            *state = next;
            Ok((CONVERT_LABEL.to_owned(), report))
        })
    }

    /// The design with the part converted, and what was made.
    fn converted(&self, part: FeatureUid) -> Result<(DocState, ConvertedPart), ModelError> {
        let state = &self.state;
        let position = state
            .position(part)
            .ok_or(ModelError::NoSuchFeature(part))?;
        let entry = state.features[position].clone();
        let FeatureDef::DerivedPart(def) = &entry.def else {
            return Err(refusal(
                "only a placed library part can be converted",
                Vec::new(),
                format!("{} ({part}) is no derived part", entry.name),
            ));
        };
        if position >= state.marker {
            return Err(refusal(
                "the part is after the timeline marker: move the marker past it first",
                Vec::new(),
                format!("{} ({part}) is rolled back", entry.name),
            ));
        }
        if entry.suppressed {
            return Err(refusal(
                "the part is suppressed: unsuppress it first",
                Vec::new(),
                format!("{} ({part}) is suppressed", entry.name),
            ));
        }
        let component = entry.component;
        let source = state.library_sources.get(def.source).ok_or_else(|| {
            refusal(
                "the part's library design is not in the design",
                Vec::new(),
                format!("library source {} is missing", def.source),
            )
        })?;
        let meta = match &source.build {
            Some(build) => BuildMeta::read(build).map_err(invalid)?,
            None => BuildMeta::default(),
        };
        let length_name = meta
            .length
            .as_ref()
            .map(|l| l.parameter.clone())
            .filter(|p| !p.is_empty());
        // The source with the part's row and values, as the part reads it.
        let (values, pinned) = crate::part_options::part_values(def, &state.parameters, part);
        let configured =
            crate::part_options::configure(source, def.config.as_deref(), &values, &pinned)
                .map_err(|e| {
                    refusal(
                        "the part's library design cannot be read with its values",
                        Vec::new(),
                        e,
                    )
                })?;
        if let Some(broken) = configured.broken {
            return Err(refusal(
                "the part's values break a rule of its library design",
                Vec::new(),
                broken.text,
            ));
        }
        let src = configured.state;
        let kernel = &self.kernel;
        let evaluated = crate::features::derived_part::evaluate_source(kernel, &src)
            .map_err(|e| refusal("the part's library design does not build", Vec::new(), e))?;
        let source_bodies: Vec<BodyUid> = evaluated
            .ids
            .iter()
            .map(|id| {
                id.ok_or_else(|| {
                    refusal(
                        "the part's library design is an assembly of components",
                        Vec::new(),
                        format!("{} places bodies in components", source.label()),
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        let length = length_name
            .as_ref()
            .and_then(|name| src.parameters.find(name))
            .and_then(|id| src.parameters.value(id));
        let value = |id: ParamId| state.parameters.value(id).unwrap_or(f64::NAN);
        // An inward end cap's cut back (mitcad#2243) stays with the part.
        if !def.cut_back.is_none() {
            return Err(refusal(
                "the part is cut back by an inward end cap",
                Vec::new(),
                format!(
                    "{} is cut back at an end for its end cap: make the cap outward or remove it first",
                    entry.name
                ),
            ));
        }
        let mut angles = [0.0; 2];
        let mut tilts = [Tilt::X; 2];
        for (i, cut) in [&def.ends.a, &def.ends.b].into_iter().enumerate() {
            if let Some(cut) = cut {
                angles[i] = value(cut.angle);
                tilts[i] = cut.tilt.unwrap_or_default();
            }
        }
        let angled = angles.map(|a| a > 1e-12);
        // A square end's faces, in the source's names: the first body's
        // that has one.
        let mut square_ends: [Option<FaceName>; 2] = [None, None];
        if let Some(length) = length {
            for (b, z) in [(false, 0.0), (true, length)] {
                let end = usize::from(b);
                if angled[end] {
                    continue;
                }
                square_ends[end] = evaluated
                    .bodies
                    .iter()
                    .find_map(|body| end_faces(kernel, body, z, b).into_iter().next());
            }
        }
        let section = if angled[0] || angled[1] {
            bounds_of(kernel, &evaluated.bodies)
        } else {
            None
        };
        // A source without a section to extrude (an imported part's body,
        // primitives) becomes a plain body without history.
        let history = src.features[..src.marker]
            .iter()
            .any(|f| matches!(f.def, FeatureDef::Extrude(_)));

        let mut next = state.clone();
        let old_marker = next.marker;
        next.features.remove(position);
        next.marker = position;
        let kept = next
            .assembly
            .component(component)
            .map(|c| (c.policy, c.created_by));
        let mut sources: HashMap<String, String> = HashMap::new();
        let mut plain_faces: Option<HashMap<String, FaceName>> = None;
        let mut mounts = BTreeMap::new();
        let bodies: Vec<BodyUid>;
        let ends: [Option<FaceName>; 2];
        let itself: FeatureUid;
        if history {
            // The part's parameters stand for the source's of the same names.
            let mut overrides = HashMap::new();
            for (name, id) in &def.parameters {
                if let Some(source_id) = src.parameters.find(name) {
                    overrides.insert(source_id, *id);
                }
            }
            let mounts_in_source: BTreeMap<String, (FeatureUid, String)> = src.features
                [..src.marker]
                .iter()
                .filter_map(|f| match &f.def {
                    FeatureDef::JointOrigin(origin) => {
                        mount_of(&f.name, origin).map(|m| (m.name, (f.uid, f.name.clone())))
                    }
                    _ => None,
                })
                .collect();
            let (_, copy) = next.copy_component_overriding(
                &src,
                &[],
                component,
                None,
                CopySource::Document(None),
                &overrides,
            )?;
            let copied = copy.features;
            bodies = source_bodies
                .iter()
                .map(|b| {
                    BodyUid::new(
                        copied.get(&b.feature).copied().unwrap_or(b.feature),
                        b.index,
                    )
                })
                .collect();
            sources = copied
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect();
            // The mount frames' joint origins keep their names.
            for (name, (uid, feature_name)) in &mounts_in_source {
                let Some(copy) = copied.get(uid).copied() else {
                    continue;
                };
                if let Some(i) = next.position(copy) {
                    let mut renamed = (*next.features[i]).clone();
                    renamed.name = feature_name.clone();
                    next.features[i] = Arc::new(renamed);
                }
                mounts.insert(name.clone(), copy);
            }
            ends = square_ends
                .map(|face| face.map(|face| TopologyCopy { uids: &sources }.face(&face)));
            itself = bodies.first().map_or(part, |b| b.feature);
        } else {
            // The evaluated bodies as one base feature; its faces are named
            // by their places (`<F>:import(<j>)`), the source's names map to
            // them.
            let mut base_bodies = Vec::with_capacity(evaluated.bodies.len());
            let base = next.new_uid();
            let mut faces = HashMap::new();
            let mut first_face = 0u32;
            for (i, shape) in evaluated.bodies.iter().enumerate() {
                let fail = |e: crate::kernel::KernelError| {
                    refusal(
                        "the part's body cannot be kept as a plain body",
                        Vec::new(),
                        format!("body {i}: {e}"),
                    )
                };
                let kind = kernel.body_kind(shape).map_err(fail)?;
                let data = kernel.brep_data(shape).map_err(fail)?;
                for info in kernel.faces(shape).map_err(fail)? {
                    let place = FaceName::new(base, "import", Some(RoleKey::Index(first_face)));
                    for name in &info.names {
                        faces.insert(name.clone(), place.clone());
                    }
                    first_face += 1;
                }
                base_bodies.push(BaseBody {
                    name: None,
                    color: None,
                    mesh: kind == crate::exchange::BodyKind::Mesh,
                    brep: Brep::new(data),
                    number: None,
                    first_face: None,
                    cage: None,
                    material: None,
                    appearance: None,
                });
            }
            let base_def = FeatureDef::Base(BaseDef {
                bodies: base_bodies,
                operation: Operation::NewBody,
                participants: Vec::new(),
                source: Some(source.label()),
                replaces: Vec::new(),
                linked_tree: false,
                values: std::marker::PhantomData,
            });
            let base_entry = FeatureEntry {
                uid: base,
                name: next.default_name(base_def.base_name()),
                suppressed: false,
                component,
                def: base_def,
            };
            next.check_feature(next.marker, &base_entry)
                .map_err(|e| refusal("the part's body cannot be kept", Vec::new(), e))?;
            next.insert_resolved(base_entry)?;
            bodies = (0..evaluated.bodies.len() as u32)
                .map(|i| BodyUid::new(base, i))
                .collect();
            // Its frames as joint origins where they are.
            for (name, frame) in &evaluated.mounts {
                let origin = next.add_feature_in(
                    &feature(json!({
                        "type": "joint_origin",
                        "geometry": {"origin": frame.translation,
                                     "normal": frame.apply_vector([0.0, 0.0, 1.0]),
                                     "x_axis": frame.apply_vector([1.0, 0.0, 0.0])},
                    }))?,
                    Some(format!("mount:{name}").as_str()),
                    component,
                )?;
                mounts.insert(name.clone(), origin.uid);
            }
            ends =
                square_ends.map(|face| face.and_then(|face| faces.get(&face.to_string()).cloned()));
            plain_faces = Some(faces);
            itself = base;
        }
        if let (Some((policy, created_by)), Some(c)) =
            (kept, next.assembly.component_mut(component))
        {
            c.policy = policy;
            c.created_by = created_by.filter(|f| *f != part);
        }

        // Mitred ends and joinery: cut features after the copies.
        let participants: Vec<Value> = bodies.iter().map(|b| json!(b.to_string())).collect();
        let length_param = length_name
            .as_ref()
            .and_then(|name| def.parameters.get(name))
            .map(|id| state.parameters.name(*id));
        let mut end_boxes = [None, None];
        for b in [false, true] {
            let end = usize::from(b);
            if !angled[end] {
                continue;
            }
            let (Some(length), Some(length_param), Some(cut)) = (
                length,
                length_param.as_deref(),
                if b { &def.ends.b } else { &def.ends.a },
            ) else {
                return Err(refusal(
                    "the part's mitred end has no length to follow",
                    Vec::new(),
                    format!("{} has angled ends but no length parameter", entry.name),
                ));
            };
            end_boxes[end] = Some(add_mitre(
                &mut next,
                component,
                section.as_ref(),
                b,
                length,
                length_param,
                &state.parameters.name(cut.angle),
                tilts[end],
                &participants,
            )?);
        }
        let mut joinery = Vec::new();
        for cut in &def.cuts {
            let origin = cut.origin.map(value);
            let normal = crate::build::cross(cut.x, cut.y);
            let size: Vec<String> = cut
                .size
                .iter()
                .map(|id| state.parameters.name(*id))
                .collect();
            let added = next.add_feature_in(
                &feature(json!({
                    "type": "box",
                    "plane": {"origin": origin, "normal": normal, "x_axis": cut.x},
                    "corner": [0, 0],
                    "length": size[0], "width": size[1], "height": size[2],
                    "operation": "cut",
                    "participants": participants,
                }))?,
                None,
                component,
            )?;
            joinery.push(added.uid);
        }
        let added = next.marker - position;
        next.marker = old_marker - 1 + added;
        let made: Vec<FeatureUid> = next.features[position..position + added]
            .iter()
            .map(|f| f.uid)
            .collect();
        let made_set: BTreeSet<FeatureUid> = made.iter().copied().collect();

        // Every reference to the part, rewritten.
        let ends = [0, 1].map(|end| match end_boxes[end] {
            Some(tool) => Some(FaceName::new(tool, "bottom", None)),
            None => ends[end].clone(),
        });
        let rewrite = Rewrite {
            part,
            sources: &sources,
            plain: plain_faces.as_ref(),
            itself,
            bodies: &bodies,
            ends,
            end_boxes,
            joinery: &joinery,
            mounts: &mounts,
        };
        let mut lost: Vec<Lost> = Vec::new();
        let mut rewritten = Vec::new();
        let mut changed_uids = BTreeSet::new();
        for entry in &mut next.features {
            if made_set.contains(&entry.uid) {
                continue;
            }
            let mut unmapped = Vec::new();
            let old = entry.clone();
            let def = rewrite.def(&old.def, &mut unmapped)?;
            let joint = matches!(
                old.def,
                FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_) | FeatureDef::ConstraintSet(_)
            );
            lost.extend(unmapped.into_iter().map(|u| Lost {
                kind: kind_of(&old.def),
                holder: old.name.clone(),
                joint: joint.then_some(old.uid),
                unmapped: u,
            }));
            if let Some(mut def) = def {
                // An asymmetric fillet's or chamfer's sides stay where they
                // were when a new name sorts an edge's faces the other way.
                keep_measurement_sides(&mut def, &old.def, &|face| {
                    rewrite
                        .face(face)
                        .map(|f| f.to_string())
                        .unwrap_or_default()
                });
                let mut changed = (*old).clone();
                changed.def = def;
                rewritten.push(changed.name.clone());
                changed_uids.insert(changed.uid);
                *entry = Arc::new(changed);
            }
        }
        // What the design keeps besides features: annotations, drawings,
        // CAM, analyses, image references, print plates, filament changes,
        // comments and the other BUILD parts' records.
        rewrite.items(&mut next.annotations, "an annotation", &mut lost)?;
        rewrite.items(&mut next.drawings, "a drawing", &mut lost)?;
        rewrite.item(&mut next.cam, "a CAM setup", "CAM", &mut lost)?;
        rewrite.items(&mut next.analyses, "an analysis", &mut lost)?;
        rewrite.items(&mut next.image_references, "an image reference", &mut lost)?;
        rewrite.items(&mut next.print_plates, "a print plate", &mut lost)?;
        rewrite.items(&mut next.filament_changes, "a filament change", &mut lost)?;
        rewrite.items(&mut next.comments, "a comment", &mut lost)?;
        rewrite.items(&mut next.decals, "a decal", &mut lost)?;
        let others: Vec<ComponentUid> = next
            .assembly
            .components
            .iter()
            .filter(|c| c.uid != component && c.build.is_some())
            .map(|c| c.uid)
            .collect();
        for c in others {
            let name = next.assembly.name(c);
            if let Some(record) = next
                .assembly
                .component_mut(c)
                .and_then(|c| c.build.as_mut())
            {
                rewrite.item(record, "a BUILD part", &name, &mut lost)?;
            }
        }
        for (body, attributes) in &mut next.body_attributes {
            let holder = body.to_string();
            let face_keys = |map: &mut BTreeMap<String, String>, lost: &mut Vec<Lost>| {
                for (face, item) in std::mem::take(map) {
                    match rewrite.text(&face) {
                        Ok(Some(new)) => {
                            map.insert(new, item);
                        }
                        Ok(None) => {
                            map.insert(face, item);
                        }
                        Err(u) => {
                            lost.push(Lost::of("a face's appearance", &holder, u));
                            map.insert(face, item);
                        }
                    }
                }
            };
            face_keys(&mut attributes.face_appearances, &mut lost);
            face_keys(&mut attributes.face_appearance_exprs, &mut lost);
            if let Some(face) = attributes.print_bed_face.clone() {
                match rewrite.text(&face) {
                    Ok(Some(new)) => attributes.print_bed_face = Some(new),
                    Ok(None) => {}
                    Err(u) => lost.push(Lost::of("a print bed face", &holder, u)),
                }
            }
        }
        if let Some(first) = lost.first() {
            let mut places: Vec<NoticePlace> = lost
                .iter()
                .filter_map(|l| place_of(&l.unmapped.name, part))
                .collect();
            places.extend(lost.iter().filter_map(|l| {
                l.joint.map(|uid| NoticePlace::Joint {
                    feature: uid.to_string(),
                })
            }));
            let detail = lost
                .iter()
                .map(|l| format!("{} names {}", l.holder, l.unmapped.name))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(refusal(
                &format!(
                    "{} would lose the part's geometry it is on: the part's features do not \
                     make it",
                    first.kind
                ),
                places,
                format!("{detail}, which no feature of the part makes"),
            ));
        }
        // The rewritten features still check (a bound length now ends at a
        // joint origin).
        for i in 0..next.features.len() {
            let f = next.features[i].clone();
            if !changed_uids.contains(&f.uid) {
                continue;
            }
            next.check_feature(i, &f).map_err(|e| {
                refusal(
                    &format!("{} would not hold on the part's features", kind_of(&f.def)),
                    Vec::new(),
                    format!("{}: {e}", f.name),
                )
            })?;
        }

        // The part's own parameters: a copied feature's own parameter that
        // only names one of them gives its slot to it; the rest go to the
        // first new feature that uses them, or with the part.
        let owned = next.parameters.owned_by(part);
        let owned_names: HashMap<String, ParamId> = owned
            .iter()
            .map(|id| (next.parameters.name(*id), *id))
            .collect();
        for &uid in &made {
            let Some(i) = next.position(uid) else {
                continue;
            };
            let mut replaced: Vec<(ParamId, ParamId)> = Vec::new();
            for slot in next.features[i].def.params() {
                if next.parameters.owner(slot) != Some(uid) {
                    continue;
                }
                let Some(target) = next
                    .parameters
                    .get(slot)
                    .and_then(|p| owned_names.get(p.expression().trim()))
                    .copied()
                else {
                    continue;
                };
                replaced.push((slot, target));
            }
            if replaced.is_empty() {
                continue;
            }
            let map: HashMap<ParamId, ParamId> = replaced.iter().copied().collect();
            let def = next.features[i]
                .def
                .map_params(&mut |_, id: &ParamId| Ok::<_, ()>(*map.get(id).unwrap_or(id)))
                .expect("never fails");
            let mut changed = (*next.features[i]).clone();
            changed.def = def;
            next.features[i] = Arc::new(changed);
            for (slot, target) in replaced {
                next.parameters.set_owner(target, Some(uid));
                let _ = next.parameters.remove(slot);
            }
        }
        for id in next.parameters.owned_by(part) {
            let user = made.iter().copied().find(|uid| {
                next.entry(*uid).is_some_and(|f| {
                    f.def.params().contains(&id)
                        || f.def.params().iter().any(|own| {
                            next.parameters
                                .get(*own)
                                .is_some_and(|p| p.dependencies().contains(&id))
                        })
                })
            });
            if let Some(user) = user {
                next.parameters.set_owner(id, Some(user));
            }
        }
        next.release_params(part);

        // What the design kept of the part's bodies follows them.
        for (i, new) in bodies.iter().enumerate() {
            let old = BodyUid::new(part, i as u32);
            if let Some(name) = next.body_names.remove(&old) {
                next.body_names.insert(*new, name);
            }
            if let Some(attributes) = next.body_attributes.remove(&old) {
                next.body_attributes.insert(*new, attributes);
            }
            if let Some(sets) = next.properties.owners.remove(&PropertyOwner::Body(old)) {
                next.properties
                    .owners
                    .insert(PropertyOwner::Body(*new), sets);
            }
            if let Some(mut flat) = next.flat_patterns.remove(&old) {
                if flat.stationary.feature == part {
                    flat.stationary.feature = new.feature;
                }
                next.flat_patterns.insert(*new, flat);
            }
        }
        next.feature_visibility.remove(&part);
        next.datum_display_sizes.remove(&part);
        next.import_notes.remove(&part);
        if let Some(condition) = next.bindings.suppressed_when.remove(&part) {
            next.bindings.suppressed_when.insert(itself, condition);
        }
        for group in &mut next.groups {
            if let Some(at) = group.features.iter().position(|f| *f == part) {
                let _ = group.features.splice(at..=at, made.iter().copied());
            }
        }
        // It no longer follows its library: what its lists showed stays
        // as its properties.
        for property in self.library_snapshot(component) {
            next.properties
                .put(PropertyOwner::Component(component), LIBRARY_SET, property);
        }
        if let Some(c) = next.assembly.component_mut(component) {
            c.build = None;
            c.library = None;
        }
        let features = made
            .iter()
            .filter_map(|uid| next.entry(*uid).map(|f| (*uid, f.name.clone())))
            .collect();
        let report = ConvertedPart {
            component,
            features,
            history,
            bodies: bodies
                .iter()
                .enumerate()
                .map(|(i, b)| (BodyUid::new(part, i as u32), *b))
                .collect(),
            rewritten,
        };
        Ok((next, report))
    }
}

/// A definition from its JSON, its values given as a command gives them.
fn feature(value: Value) -> Result<FeatureDef<ValueInput>, ModelError> {
    serde_json::from_value(value).map_err(|e| invalid(format!("{CONVERT_LABEL}: {e}")))
}

/// A mitred end as features (`b`: end B, else A): a construction plane
/// through the long side's edge at the end's angle from square (end B's
/// moved along its normal with the length, `L · cos(angle)`), and a box on
/// it that cuts away what is beyond, as large as the part's own cut. Returns
/// the box's uid.
#[allow(clippy::too_many_arguments)]
fn add_mitre(
    next: &mut DocState,
    component: ComponentUid,
    section: Option<&crate::kernel::BoundingBox>,
    b: bool,
    length: f64,
    length_param: &str,
    angle_param: &str,
    tilt: Tilt,
    participants: &[Value],
) -> Result<FeatureUid, ModelError> {
    let (point, _) = end_plane(section, b, length, 0.0, tilt)
        .ok_or_else(|| invalid(format!("{CONVERT_LABEL}: the end's cut has no normal")))?;
    let axis = [0.0, 0.0, if b { 1.0 } else { -1.0 }];
    // About the line along the section, through the long side's edge: the
    // base plane's normal (the end's axis) turns towards the lean.
    let line = crate::build::cross(axis, tilt.lean());
    let at_angle = next.add_feature_in(
        &feature(json!({
            "type": "construction_plane",
            "definition": {
                "type": "angle",
                "line": {"origin": [point[0], point[1], 0.0], "direction": line},
                "angle": angle_param,
                "plane": {"origin": [0.0, 0.0, 0.0], "normal": axis, "x_axis": [1.0, 0.0, 0.0]},
            },
        }))?,
        None,
        component,
    )?;
    let mut plane = at_angle.uid;
    if b {
        let moved = next.add_feature_in(
            &feature(json!({
                "type": "construction_plane",
                "definition": {
                    "type": "offset",
                    "plane": plane.to_string(),
                    "distance": format!("{length_param} * cos({angle_param})"),
                },
            }))?,
            None,
            component,
        )?;
        plane = moved.uid;
    }
    let size = format!("4 * ({length_param} + 1000 mm)");
    let corner = format!("-2 * ({length_param} + 1000 mm)");
    let tool = next.add_feature_in(
        &feature(json!({
            "type": "box",
            "plane": plane.to_string(),
            "corner": [corner, corner],
            "length": size, "width": size, "height": size,
            "operation": "cut",
            "participants": participants,
        }))?,
        None,
        component,
    )?;
    Ok(tool.uid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_finds_the_part_outside_source_names() {
        let f7: FeatureUid = "F7".parse().unwrap();
        assert!(mentions("F7", f7));
        assert!(mentions("F7.b0", f7));
        assert!(mentions("F7:end_a", f7));
        assert!(mentions("F7:part(F2:side(c1))", f7));
        assert!(mentions("E{F7:end_b|F7:part(F2:side(c1))}", f7));
        assert!(!mentions("F70:end_a", f7));
        assert!(!mentions("F9:part(F7:side(c1))", f7));
        assert!(!mentions("xF7", f7));
    }
}
