// SPDX-License-Identifier: MIT
//! Feature definitions and the interface every feature type implements.
//!
//! Each feature type lives in its own module with its definition struct,
//! generic over how values refer to parameters (`P`): [`ParamId`] in the
//! model, the parameter name in project files and a name or a number
//! ([`ValueInput`]) in commands. The struct's serde form is the file and
//! command form of the feature. A type implements:
//!
//! - `map_params`, which converts between the value forms field by field
//!   and names each field's slot (`"extent.distance"`), used for error
//!   locations, parameter comments and keeping parameters on edits;
//! - [`FeatureInfo`]: static references, validation against the features
//!   before it, and value checks for new features;
//! - [`Evaluate`]: evaluation against the body state left by the previous
//!   features, through an [`EvalContext`] that records what it reads for the
//!   recompute cache.
//!
//! Adding a feature type means a new module and one line in
//! `feature_types!` below; nothing else lists the types.

pub mod base;
pub mod chamfer;
pub mod construction;
pub mod extrude;
pub mod fillet;
pub mod geom_ref;
pub mod sketch;
// Profile features (F1).
pub mod hole;
pub mod reference;
pub mod revolve;
pub mod thread;
pub mod thread_table;
// Transforms, patterns, mirrors, combine and primitives (F4).
pub mod align;
pub mod bodies;
pub mod combine;
pub mod mirror;
pub mod moves;
pub mod pattern;
pub mod primitive;
pub mod scale;

// Face operations (F2).
pub mod delete_face;
pub mod draft;
#[cfg(test)]
mod f2_tests;
pub mod face_refs;
pub mod offset_face;
pub mod replace_face;
pub mod shell;
pub mod split_body;
pub mod split_face;

// Sweeps, lofts, pipes, coils, ribs and webs (F3).
pub mod coil;
#[cfg(test)]
mod f3_tests;
pub mod loft;
pub mod path;
pub mod pipe;
pub mod rib;
pub mod sweep;

// Components and occurrences (F6).
pub mod occurrence;

// FreeCAD helices (mitcad#4).
pub mod helix;

// Joints between occurrences (mitcad#55).
pub mod joint;

// Fillets and chamfers repeated by patterns and mirrors (mitcad#105).
pub(crate) mod dressup_copies;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use base::{BaseBody, BaseDef, Brep};
pub use chamfer::{ChamferCorner, ChamferDef, ChamferSetDef, ChamferSizeDef};
pub use construction::{ConstructionAxisDef, ConstructionPlaneDef, ConstructionPointDef};
pub use extrude::{Extent, ExtrudeDef, Operation, ProfileRef};
pub use extrude::{Side, Start, Thin, ThinSide};
pub use fillet::{Continuity, FilletDef, FilletSetDef, FilletSizeDef, MidRadius};
pub use geom_ref::{GeomRef, PathRef, Want};
pub use hole::{HoleDef, HoleExtent, HoleKind, HolePlacement, HoleThread, SketchPoint};
pub use reference::{ExtentObject, FaceRef};
pub use revolve::{RevolveDef, RevolveExtent};
pub use sketch::{FrameDef, SketchDef, SketchOutput, SketchPlane};
pub use thread::{ThreadDef, ThreadEnd, ThreadSize};
pub use thread_table::{ThreadData, ThreadStandard};
pub use {
    align::{AlignDef, AlignRef},
    combine::{CombineDef, CombineOperation},
    mirror::MirrorDef,
    moves::{MoveDef, MoveSpec},
    pattern::{
        CircularPatternDef, ComputeOption, PathPatternDef, PatternDirection, PatternObjects,
        RectangularPatternDef, SpacingType,
    },
    primitive::{BoxDef, CylinderDef, SphereDef, TorusDef, TorusPosition},
    scale::{ScaleDef, ScaleSpec},
};

// Face operations (F2).
pub use delete_face::DeleteFaceDef;
pub use draft::DraftDef;
pub use face_refs::UNSUPPORTED;
pub use offset_face::OffsetFaceDef;
pub use replace_face::ReplaceFaceDef;
pub use shell::ShellDef;
pub use split_body::SplitBodyDef;
pub use split_face::SplitFaceDef;

// Sweeps, lofts, pipes, coils, ribs and webs (F3).
pub use coil::{CoilDef, Helix};
pub use loft::{EndCondition, LoftDef, LoftSectionDef};
pub use path::{CurvePath, SketchCurves};
pub use pipe::PipeDef;
pub use rib::{RibDef, RibExtent, WebDef};
pub use sweep::{PathExtent, SweepDef};

// Components and occurrences (F6).
pub use occurrence::{CapturePositionDef, ComponentFromBodiesDef, MoveOccurrenceDef, Position};
// FreeCAD helices (mitcad#4).
pub use helix::{HelixConstruction, HelixDef};
// Joints between occurrences (mitcad#55).
pub use joint::{
    AsBuiltJointDef, FrameOverride, JointDef, JointOrigin, JointOriginDef, JointPosition, Limit,
    Limits, OccurrencePath, RigidGroupDef,
};

use crate::assembly::Assembly;
use crate::datum::Datum;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::parameters::{ParamId, Parameters};
use crate::recompute::{BodyState, FeatureResult, Read, Versioned};
use crate::topo::EdgeName;
use crate::transform::Transform;

/// A value in a command: a parameter by name, or a number that becomes a
/// new dimension parameter of the feature (`d4`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValueInput {
    Number(f64),
    Name(String),
}

macro_rules! feature_types {
    ($($variant:ident($def:ident)),* $(,)?) => {
        /// The definition of a feature; the serde tag `type` is the type name.
        /// Reading it first brings earlier forms up to date ([`migrate`]).
        #[derive(Debug, Clone, PartialEq, Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        pub enum FeatureDef<P = ParamId> {
            $($variant($def<P>),)*
        }

        /// [`FeatureDef`] as it is read, after [`migrate`].
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum FeatureDefIn<P> {
            $($variant($def<P>),)*
        }

        impl<'de, P: Deserialize<'de>> Deserialize<'de> for FeatureDef<P> {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let mut value = serde_json::Value::deserialize(deserializer)?;
                migrate(&mut value);
                Ok(match FeatureDefIn::<P>::deserialize(value).map_err(serde::de::Error::custom)? {
                    $(FeatureDefIn::$variant(def) => Self::$variant(def),)*
                })
            }
        }

        impl<P> FeatureDef<P> {
            /// The type name in files and commands, e.g. `extrude`.
            pub fn type_name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $def::<P>::TYPE,)*
                }
            }

            /// The base of default feature names, e.g. `Extrude` for `Extrude1`.
            pub fn base_name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $def::<P>::BASE_NAME,)*
                }
            }

            /// Converts the value fields; `f` gets each field's slot and value.
            pub fn map_params<Q, E>(
                &self,
                f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
            ) -> Result<FeatureDef<Q>, E> {
                Ok(match self {
                    $(Self::$variant(def) => FeatureDef::$variant(def.map_params(f)?),)*
                })
            }
        }

        impl FeatureDef {
            pub fn info(&self) -> &dyn FeatureInfo {
                match self {
                    $(Self::$variant(def) => def,)*
                }
            }

            pub(crate) fn evaluator<K: Kernel>(&self) -> &dyn Evaluate<K> {
                match self {
                    $(Self::$variant(def) => def,)*
                }
            }
        }
    };
}

feature_types! {
    Sketch(SketchDef),
    Extrude(ExtrudeDef),
    Fillet(FilletDef),
    Chamfer(ChamferDef),
    // Face operations (F2).
    Shell(ShellDef),
    Draft(DraftDef),
    OffsetFace(OffsetFaceDef),
    DeleteFace(DeleteFaceDef),
    ReplaceFace(ReplaceFaceDef),
    SplitBody(SplitBodyDef),
    SplitFace(SplitFaceDef),
    ConstructionPlane(ConstructionPlaneDef),
    ConstructionAxis(ConstructionAxisDef),
    ConstructionPoint(ConstructionPointDef),
    Base(BaseDef),
    Revolve(RevolveDef), Hole(HoleDef), Thread(ThreadDef),
    // Transforms, patterns, mirrors, combine and primitives (F4).
    RectangularPattern(RectangularPatternDef), CircularPattern(CircularPatternDef), PathPattern(PathPatternDef), Mirror(MirrorDef), Combine(CombineDef), Move(MoveDef), Align(AlignDef), Scale(ScaleDef), Box(BoxDef), Cylinder(CylinderDef), Sphere(SphereDef), Torus(TorusDef),
    // Sweeps, lofts, pipes, coils, ribs and webs (F3).
    Sweep(SweepDef), Loft(LoftDef), Pipe(PipeDef), Coil(CoilDef), Rib(RibDef), Web(WebDef),
    // Components and occurrences (F6).
    ComponentFromBodies(ComponentFromBodiesDef), MoveOccurrence(MoveOccurrenceDef), CapturePosition(CapturePositionDef),
    // FreeCAD helices (mitcad#4).
    Helix(HelixDef),
    // Joints between occurrences (mitcad#55).
    Joint(JointDef), AsBuiltJoint(AsBuiltJointDef), JointOrigin(JointOriginDef), RigidGroup(RigidGroupDef),
}

/// Brings a feature definition in an earlier form up to date before it is
/// read: what a reference cannot carry by itself moves to the feature
/// (F2's tools, see [`face_refs::migrate_tool`]). References in earlier
/// forms are read by [`GeomRef`] and [`PathRef`] themselves.
fn migrate(value: &mut serde_json::Value) {
    if let serde_json::Value::Object(def) = value {
        face_refs::migrate_tool(def);
    }
}

impl FeatureDef {
    /// The parameters the definition uses, in slot order.
    pub fn params(&self) -> Vec<ParamId> {
        let mut params = Vec::new();
        let _ = self.map_params(&mut |_, id: &ParamId| {
            params.push(*id);
            Ok::<_, ()>(*id)
        });
        params
    }

    pub fn references(&self) -> References {
        let mut references = self.info().references();
        references.params = self.params();
        references
    }
}

/// A feature in the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureEntry {
    pub uid: FeatureUid,
    pub name: String,
    pub suppressed: bool,
    /// The component it belongs to (F6): it works on that component's
    /// bodies, in its coordinates. The root component unless made in
    /// another.
    pub component: ComponentUid,
    pub def: FeatureDef,
}

/// What a definition refers to. Every feature in `features` must come
/// before the referring feature in the timeline.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct References {
    pub params: Vec<ParamId>,
    /// Sketches, the creators of referenced bodies and the features named
    /// in topological names.
    pub features: BTreeSet<FeatureUid>,
}

impl References {
    pub fn body(&mut self, body: BodyUid) {
        self.features.insert(body.feature);
    }

    pub fn edges<'a>(&mut self, edges: impl IntoIterator<Item = &'a EdgeName>) {
        for edge in edges {
            self.features.extend(edge.features());
        }
    }
}

/// Static behaviour of a feature type.
pub trait FeatureInfo {
    /// The features it refers to; parameters come from `map_params`.
    fn references(&self) -> References;

    /// Checks the references against the features before it and the
    /// definition's own consistency.
    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String>;

    /// Rejects values a new or edited feature must not start with, as the
    /// commands do (a zero width); recompute reports them later, when a
    /// parameter change brings them in.
    fn check_values(&self, _value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        Ok(())
    }

    /// True when evaluating it can create bodies.
    fn creates_bodies(&self) -> bool {
        false
    }

    /// The sketch profiles it sweeps (they are then hidden).
    fn profiles(&self) -> &[ProfileRef] {
        &[]
    }

    /// True when it makes a new component (F6), which then holds the
    /// bodies it creates or changes; the component is placed in the
    /// feature's own component.
    fn new_component(&self) -> bool {
        false
    }
    /// How the tool body the feature leaves in [`FeatureOutput::tool`]
    /// combines with bodies, so that patterns and mirrors of the feature can
    /// repeat it (a patterned feature's tool is applied again at each
    /// instance). None: the feature cannot be patterned or mirrored.
    fn tool_use(&self) -> Option<ToolUse> {
        None
    }
}

/// What patterns and mirrors of a feature repeat at each instance: its tool
/// body combined with bodies by its operation.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUse {
    pub operation: Operation,
    /// The bodies a join, cut or intersection works on; empty for all
    /// bodies at the pattern.
    pub participants: Vec<BodyUid>,
}

/// Evaluation of a feature type with a kernel.
pub trait Evaluate<K: Kernel> {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String>;

    /// The tool of feature `feature` (this definition) rebuilt with its own
    /// placement (sketch frame, plane) moved by the rigid `placement`, faces
    /// named after `feature`; extents that depend on bodies are found again
    /// against the bodies `ctx` sees. Patterns use it for the Adjust
    /// option. None: patterns move the tool the feature left instead.
    fn placed_tool(
        &self,
        _ctx: &mut EvalContext<'_, K>,
        _feature: FeatureUid,
        _placement: &Transform,
    ) -> Option<Result<K::Shape, String>> {
        None
    }
}

/// What a feature produced. A failed feature produces nothing and the body
/// state passes it by.
#[derive(Debug)]
pub struct FeatureOutput<S> {
    pub sketch: Option<SketchOutput>,
    pub changes: Vec<BodyChange<S>>,
    /// The tool body before a boolean, for previews.
    pub tool: Option<S>,
    /// The plane, axis or point of a construction feature.
    pub datum: Option<Datum>,
    /// Occurrences it moves (F6).
    pub placements: Vec<PlacementChange>,
}

impl<S> Default for FeatureOutput<S> {
    fn default() -> Self {
        Self {
            sketch: None,
            changes: Vec::new(),
            tool: None,
            datum: None,
            placements: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum BodyChange<S> {
    /// Creates the body or replaces its shape.
    Set(BodyUid, S),
    Remove(BodyUid),
}

/// A timeline feature's change of an occurrence's placement (F6); the
/// occurrence must be placed in the feature's component and not grounded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlacementChange {
    /// Moved by the transform, in the parent component's coordinates.
    Move(OccurrenceUid, Transform),
    /// Put at the transform (a captured position).
    Set(OccurrenceUid, Transform),
}

/// The timeline seen from a feature being checked.
pub struct CheckContext<'a> {
    pub(crate) uid: FeatureUid,
    pub(crate) earlier: &'a [Arc<FeatureEntry>],
    pub(crate) all: &'a [Arc<FeatureEntry>],
}

impl CheckContext<'_> {
    /// A feature before the one being checked.
    pub fn feature(&self, uid: FeatureUid) -> Result<&FeatureEntry, String> {
        if let Some(entry) = self.earlier.iter().find(|f| f.uid == uid) {
            return Ok(entry);
        }
        if uid == self.uid {
            return Err("a feature cannot refer to itself".to_owned());
        }
        match self.all.iter().find(|f| f.uid == uid) {
            Some(later) => Err(format!(
                "{} ({uid}) does not come before this feature in the timeline",
                later.name
            )),
            None => Err(format!("feature {uid} does not exist")),
        }
    }

    pub fn sketch(&self, uid: FeatureUid) -> Result<(&FeatureEntry, &SketchDef), String> {
        let entry = self.feature(uid)?;
        match &entry.def {
            FeatureDef::Sketch(sketch) => Ok((entry, sketch)),
            _ => Err(format!("{} ({uid}) is not a sketch", entry.name)),
        }
    }

    /// A body whose creator comes before this feature. Whether the body
    /// exists is only known when the timeline is evaluated.
    pub fn body(&self, body: BodyUid) -> Result<(), String> {
        let entry = self.feature(body.feature)?;
        if entry.def.info().creates_bodies() {
            Ok(())
        } else {
            Err(format!(
                "{} ({}) does not create bodies",
                entry.name, entry.uid
            ))
        }
    }

    /// The features named in the edges come before this feature.
    pub fn edges(&self, edges: &[EdgeName]) -> Result<(), String> {
        if edges.is_empty() {
            return Err("no edges selected".to_owned());
        }
        for (i, edge) in edges.iter().enumerate() {
            if edges[..i].contains(edge) {
                return Err(format!("edge {edge} is listed more than once"));
            }
            for feature in edge.features() {
                self.feature(feature)
                    .map_err(|e| format!("edge {edge}: {e}"))?;
            }
        }
        Ok(())
    }
}

/// What evaluation reads besides the kernel.
pub(crate) struct Env<'a, S> {
    pub params: &'a Parameters,
    /// The bodies of the feature's component.
    pub bodies: &'a BodyState<S>,
    /// The component of every body at this point, for messages about the
    /// bodies of other components.
    pub owners: &'a BTreeMap<BodyUid, ComponentUid>,
    pub assembly: &'a Assembly,
    pub sketches: &'a BTreeMap<FeatureUid, Versioned<Arc<SketchOutput>>>,
    pub datums: &'a BTreeMap<FeatureUid, Versioned<Datum>>,
    pub features: &'a [Arc<FeatureEntry>],
    /// Results of the features before this one.
    pub history: &'a [FeatureResult<S>],
    /// Every component's bodies, for linked geometry (mitcad#100).
    pub components: &'a BTreeMap<ComponentUid, Arc<BodyState<S>>>,
    /// Every occurrence's placement at this point of the timeline.
    pub placements: &'a BTreeMap<OccurrenceUid, Transform>,
}

/// Inputs of an evaluating feature. Every read is recorded, so the
/// recompute cache can tell whether a stored result still applies.
pub struct EvalContext<'a, K: Kernel> {
    pub kernel: &'a K,
    /// The feature being evaluated.
    pub uid: FeatureUid,
    pub(crate) env: Env<'a, K::Shape>,
    pub(crate) reads: Vec<Read>,
    /// What the feature built with caveats (P9: yellow in the timeline).
    pub(crate) warnings: Vec<String>,
    /// A pattern's elements, suppressed ones too (P9: the panel shows a
    /// toggle at each), by element number; element 0 is the original.
    pub(crate) elements: Vec<Transform>,
    next_body: u32,
}

impl<'a, K: Kernel> EvalContext<'a, K> {
    pub(crate) fn new(kernel: &'a K, uid: FeatureUid, env: Env<'a, K::Shape>) -> Self {
        Self {
            kernel,
            uid,
            env,
            reads: Vec::new(),
            warnings: Vec::new(),
            elements: Vec::new(),
            next_body: 0,
        }
    }

    /// The feature succeeded with a caveat: it is shown as a warning.
    pub fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    /// Warns about what the geometry gave up to build a shape
    /// ([`Kernel::notes`]).
    pub fn warn_notes(&mut self, shape: &K::Shape) {
        for note in self.kernel.notes(shape) {
            self.warn(note);
        }
    }

    pub fn param(&mut self, id: ParamId) -> Result<f64, String> {
        let value = self.env.params.value(id);
        self.reads.push(Read::Param(id, value.map(f64::to_bits)));
        value.ok_or_else(|| format!("parameter {} does not exist", self.env.params.name(id)))
    }

    /// A parameter that must be greater than zero.
    pub fn positive(&mut self, id: ParamId) -> Result<f64, String> {
        let value = self.param(id)?;
        if value > 0.0 {
            Ok(value)
        } else {
            Err(format!(
                "{} must be greater than zero, got {value}",
                self.env.params.name(id)
            ))
        }
    }

    pub fn param_name(&self, id: ParamId) -> String {
        self.env.params.name(id)
    }

    /// The evaluated output of a sketch before this feature.
    pub fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<SketchOutput>, String> {
        let output = self.env.sketches.get(&uid);
        self.reads
            .push(Read::Sketch(uid, output.map(|o| o.version)));
        output
            .map(|o| o.value.clone())
            .ok_or_else(|| format!("{} has no result", self.feature_name(uid)))
    }

    /// The current shape of a body of the feature's component.
    pub fn body(&mut self, uid: BodyUid) -> Result<K::Shape, String> {
        let body = self.env.bodies.get(&uid);
        self.reads.push(Read::Body(uid, body.map(|b| b.version)));
        body.map(|b| b.value.clone()).ok_or_else(|| {
            let creator = self.feature_name(uid.feature);
            match self.env.owners.get(&uid) {
                Some(owner) => format!(
                    "body {uid} (from {creator}) belongs to {}; a feature works on the bodies \
                     of its own component (activate that component)",
                    self.env.assembly.name(*owner)
                ),
                None => format!(
                    "body {uid} (from {creator}) does not exist at this point of the timeline"
                ),
            }
        })
    }

    /// All bodies of the feature's component, in order of their ids.
    pub fn bodies(&mut self) -> Vec<(BodyUid, K::Shape)> {
        self.reads.push(Read::AllBodies(
            self.env
                .bodies
                .iter()
                .map(|(u, b)| (*u, b.version))
                .collect(),
        ));
        self.env
            .bodies
            .iter()
            .map(|(u, b)| (*u, b.value.clone()))
            .collect()
    }

    /// The component the feature belongs to.
    pub fn component(&self) -> ComponentUid {
        self.env
            .features
            .iter()
            .find(|f| f.uid == self.uid)
            .map_or(ComponentUid::ROOT, |f| f.component)
    }

    /// Where linked geometry is (mitcad#100, `links.rs`): its component
    /// and the transform from that component's coordinates into this
    /// feature's, by the placements at this point of the timeline.
    pub fn linked(
        &mut self,
        link: &crate::links::OccurrenceLink,
    ) -> Result<(ComponentUid, Transform), String> {
        let assembly = self.env.assembly;
        let source = link.source_component(assembly)?;
        let component = self.component();
        let transform = link.transform(assembly, component, &mut |o| self.placement(o))?;
        Ok((source, transform))
    }

    /// An occurrence's placement at this point of the timeline: its own
    /// transform, moved by the features before this one.
    pub fn placement(&mut self, uid: OccurrenceUid) -> Transform {
        let placed = self.env.placements.get(&uid).copied();
        self.reads.push(Read::Placement(
            uid,
            placed.as_ref().map(crate::recompute::placement_bits),
        ));
        placed
            .or_else(|| self.env.assembly.occurrence(uid).map(|o| o.transform))
            .unwrap_or(Transform::IDENTITY)
    }

    /// A body of `component` at this point (linked geometry).
    pub fn body_in(&mut self, component: ComponentUid, uid: BodyUid) -> Result<K::Shape, String> {
        if component == self.component() {
            return self.body(uid);
        }
        let components = self.env.components;
        let body = components.get(&component).and_then(|b| b.get(&uid));
        self.reads
            .push(Read::BodyIn(component, uid, body.map(|b| b.version)));
        body.map(|b| b.value.clone()).ok_or_else(|| {
            format!(
                "body {uid} (from {}) is not in {} at this point of the timeline",
                self.feature_name(uid.feature),
                self.env.assembly.name(component)
            )
        })
    }

    /// All bodies of `component` at this point, in order of their ids.
    pub fn bodies_in(&mut self, component: ComponentUid) -> Vec<(BodyUid, K::Shape)> {
        if component == self.component() {
            return self.bodies();
        }
        let components = self.env.components;
        let bodies: Vec<(BodyUid, &Versioned<K::Shape>)> = components
            .get(&component)
            .into_iter()
            .flat_map(|b| b.iter())
            .map(|(uid, b)| (*uid, b))
            .collect();
        self.reads.push(Read::BodiesIn(
            component,
            bodies.iter().map(|(uid, b)| (*uid, b.version)).collect(),
        ));
        bodies
            .into_iter()
            .map(|(uid, b)| (uid, b.value.clone()))
            .collect()
    }

    /// The id of the next body this feature creates.
    pub fn new_body(&mut self) -> BodyUid {
        self.next_body += 1;
        BodyUid::new(self.uid, self.next_body - 1)
    }

    pub fn feature_name(&self, uid: FeatureUid) -> String {
        self.env
            .features
            .iter()
            .find(|f| f.uid == uid)
            .map_or_else(|| uid.to_string(), |f| f.name.clone())
    }

    /// Explains why the body has no such edge, naming the feature after
    /// which it disappeared.
    pub fn missing_edge(&self, body: BodyUid, edge: &EdgeName) -> String {
        let mut existed = false;
        let mut lost_after = None;
        for result in self.env.history {
            let Some(shape) = result.shape_set(body) else {
                continue;
            };
            let present = self.kernel.count_edges(shape, edge) > 0;
            if existed && !present {
                lost_after = Some(result.uid);
            }
            existed = present;
        }
        if let Some(feature) = lost_after {
            return format!(
                "edge {edge} no longer exists after {}",
                self.feature_name(feature)
            );
        }
        for feature in edge.features() {
            let failed = self
                .env
                .history
                .iter()
                .find(|r| r.uid == feature)
                .is_some_and(|r| !r.status.is_ok());
            if failed {
                return format!(
                    "edge {edge} does not exist because {} failed",
                    self.feature_name(feature)
                );
            }
        }
        format!("the body has no edge {edge}")
    }

    /// Checks that every edge resolves on the shape.
    pub fn require_edges(
        &self,
        body: BodyUid,
        shape: &K::Shape,
        edges: &[EdgeName],
    ) -> Result<(), String> {
        match edges
            .iter()
            .find(|edge| self.kernel.count_edges(shape, edge) == 0)
        {
            Some(edge) => Err(self.missing_edge(body, edge)),
            None => Ok(()),
        }
    }
}

/// Serde helper for `skip_serializing_if`.
pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

/// The last component of a slot path: `extent.distance` -> `distance`.
pub(crate) fn slot_label(slot: &str) -> &str {
    slot.rsplit('.').next().unwrap_or(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_names_are_the_serde_tags() {
        let defs: Vec<FeatureDef<String>> = vec![
            serde_json::from_str(r#"{"type": "sketch", "shapes": []}"#).unwrap(),
            serde_json::from_str(
                r#"{"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
                    "extent": {"type": "distance", "distance": "d1"}, "operation": "new_body"}"#,
            )
            .unwrap(),
            serde_json::from_str(
                r#"{"type": "fillet", "body": "F2.b0", "edges": ["E{F2:a|F2:b}"], "radius": "d2"}"#,
            )
            .unwrap(),
            serde_json::from_str(
                r#"{"type": "chamfer", "body": "F2.b0", "edges": ["E{F2:a|F2:b}"],
                    "size": {"type": "equal_distance", "distance": "d3"}}"#,
            )
            .unwrap(),
        ];
        for def in defs {
            let value = serde_json::to_value(&def).unwrap();
            assert_eq!(value["type"], def.type_name());
            assert!(def.base_name().to_lowercase() == def.type_name());
        }
    }

    #[test]
    fn values_in_commands_are_names_or_numbers() {
        let def: FeatureDef<ValueInput> = serde_json::from_str(
            r#"{"type": "fillet", "body": "F2.b0", "edges": ["E{F2:a|F2:b}"], "radius": 2.5}"#,
        )
        .unwrap();
        let mut seen = Vec::new();
        def.map_params(&mut |slot, value| {
            seen.push((slot.to_owned(), value.clone()));
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(seen, vec![("radius".to_owned(), ValueInput::Number(2.5))]);
        let named: FeatureDef<ValueInput> = serde_json::from_str(
            r#"{"type": "fillet", "body": "F2.b0", "edges": ["E{F2:a|F2:b}"], "radius": "d4"}"#,
        )
        .unwrap();
        let FeatureDef::Fillet(fillet) = named else {
            panic!("a fillet");
        };
        assert_eq!(
            fillet.sets[0].size,
            FilletSizeDef::Constant {
                radius: ValueInput::Name("d4".to_owned())
            }
        );
    }

    #[test]
    fn unknown_fields_and_types_are_rejected() {
        for (json, expected) in [
            (r#"{"type": "warp"}"#, "unknown variant `warp`"),
            (
                r#"{"type": "fillet", "body": "F2.b0", "edges": [], "radius": "d2", "radus": 1}"#,
                "unknown field `radus`",
            ),
            (
                r#"{"type": "fillet", "body": "Extrude1", "edges": [], "radius": "d2"}"#,
                "invalid body id 'Extrude1'",
            ),
        ] {
            let error = serde_json::from_str::<FeatureDef<String>>(json).unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
    }
}
