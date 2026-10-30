// SPDX-License-Identifier: MIT
//! Feature definitions and the interface every feature type implements.
//!
//! Each feature type lives in its own module with its definition struct,
//! generic over how values refer to parameters (`P`): [`ParamId`] in the
//! model, the parameter name in design files and a name or a number
//! ([`ValueInput`]) in commands. The struct's serde form is the file and
//! command form of the feature. A type implements:
//!
//! - `map_params`, which converts between the value forms field by field
//!   and names each field's slot (`"extent.distance"`), used for error
//!   locations, parameter comments and keeping parameters on edits;
//!   `map_params!` (`mapping.rs`) writes it from the list of fields when
//!   every value maps under its field's name;
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

// Assembly constraints (mitcad#148).
pub mod constraint_set;

// Placeholders of imported items that did not come in (mitcad#170).
pub mod placeholder;
// Pattern at independently referenced points (mitcad#197).
pub mod point_pattern;
pub use point_pattern::PointPatternDef;
// Bodies taken out of the design.
pub mod remove_body;
pub use remove_body::RemoveBodyDef;
// Bodies of other components copied, linked to them (mitcad#324).
pub mod derive;
pub use derive::DeriveDef;
// BUILD: derived parts and bound lengths (mitcad#375).
pub mod bound_length;
pub mod derived_part;
pub use bound_length::BoundLengthDef;
pub use derived_part::DerivedPartDef;
// Mesh bodies refused by surface operations (mitcad#399).
pub(crate) mod surface_bodies;

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
    combine::{CombineDef, CombineOperation, CombineTool},
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
// Assembly constraints (mitcad#148).
pub use constraint_set::{ConstraintRow, ConstraintSetDef, ConstraintType};
// Placeholders of imported items that did not come in (mitcad#170).
pub use placeholder::PlaceholderDef;
// Assembly-scoped Extrude Cut (mitcad#200).
pub mod extrude_scope;
pub mod sheet;
pub use sheet::{SheetFaceDef, SheetRulesDef};
pub mod sheet_folded;
pub use sheet_folded::{
    SheetBendDef, SheetCornerChamferDef, SheetCornerRoundDef, SheetFlangeDef, SheetFoldDef,
};
// Sheet-metal Unfold, Refold and the minimal Cut (mitcad#208).
pub mod sheet_unfold;
pub use sheet_unfold::{SheetRefoldDef, SheetUnfoldDef};
pub mod sheet_cut;
pub use sheet_cut::SheetCutDef;
// Sheet-metal corner relief (mitcad#209).
pub mod sheet_relief;
pub use sheet_relief::SheetCornerReliefDef;
// The faces a pattern's optimized compute option copies.
mod face_copy;
// Faces, edges and vertices found again after a sketch change (mitcad#272).
pub(crate) mod renamed;
// Body ids of a split's pieces kept by matching remembered pieces (mitcad#408).
pub mod pieces;
pub use pieces::{PieceGroup, PieceMark, SplitPieces};
// Sheet-metal Contour Flange, Convert to Sheet Metal and Perforate (mitcad#461).
pub mod sheet_contour;
pub use sheet_contour::SheetContourDef;
pub mod sheet_perforate;
pub use sheet_perforate::SheetPerforateDef;
pub mod sheet_convert;
pub use sheet_convert::SheetConvertDef;
// Motion links between joints (mitcad#466).
pub mod motion_link;
pub use motion_link::{Coupling, Follower, LinkSize, LinkedMotion, MotionLinkDef};
// Emboss: profiles raised or sunk on faces, wrapped on cylinders (mitcad#458).
pub mod emboss;
pub use emboss::{EmbossDef, EmbossMode};
// Bounding solids: a box around bodies grown by a margin (mitcad#460).
pub mod bounding_solid;
pub use bounding_solid::BoundingSolidDef;
// Boundary Fill: solids of the cells tool bodies enclose (mitcad#457).
pub mod boundary_fill;
pub use boundary_fill::BoundaryFillDef;
// Convert Mesh: mesh bodies rebuilt as planes and cylinders (mitcad#462).
pub mod mesh_convert;
pub use mesh_convert::MeshConvertDef;
// Patch, Stitch and Thicken: surface bodies filled, sewn and made solid (mitcad#456).
pub mod patch;
pub mod stitch;
pub mod thicken;
pub use patch::{EdgeContinuity, Opening, PatchDef, PatchLoop};
pub use stitch::StitchDef;
pub use thicken::ThickenDef;
// Surface modelling: surface bodies and their features (mitcad#455).
pub mod extend_surface;
pub mod offset_surface;
pub mod ruled_surface;
pub mod surface_sweeps;
pub use extend_surface::ExtendSurfaceDef;
pub use offset_surface::OffsetSurfaceDef;
pub use ruled_surface::RuledSurfaceDef;
pub mod sheet_edges;
pub use sheet_edges::OpeningEdgesRef;
pub mod reverse_normal;
pub mod trim_surface;
pub mod unstitch;
pub mod untrim_surface;
pub use reverse_normal::ReverseNormalDef;
pub use trim_surface::TrimSurfaceDef;
pub use unstitch::UnstitchDef;
pub use untrim_surface::UntrimSurfaceDef;
pub mod blend_surface;
pub mod boundary_surface;
pub mod fair_surface;
pub mod match_surface;
pub use blend_surface::BlendSurfaceDef;
pub use boundary_surface::BoundarySurfaceDef;
pub use fair_surface::FairSurfaceDef;
pub use match_surface::MatchSurfaceDef;
// Plastic part features and plastic rules (mitcad#459).
pub mod plastic;
pub use plastic::{BossDef, GrooveDef, LipDef, PlasticRuleDef, RestDef, SnapFitDef};
// Bodies moved into another component on the timeline (mitcad#465).
pub mod body_transfer;
pub use body_transfer::BodyTransferDef;
// Gears: spur, helical, herringbone, ring, rack and bevel (mitcad#472).
pub mod gear;
pub mod gear_profile;
pub use gear::{GearDef, GearKind};
// Faces moved by a rigid motion (mitcad#343).
pub mod move_face;
pub use move_face::MoveFaceDef;
// Scale patterns: copies scaled where they are (mitcad#539).
pub mod scale_pattern;
pub use scale_pattern::ScalePatternDef;
// Timing belt pulleys and idlers (mitcad#562).
pub mod pulley;
pub mod pulley_profile;
pub use pulley::{PulleyDef, PulleyKind};
// Printable bearings: print-in-place and airsoft-ball bearings (mitcad#577).
pub mod bearing_sizes;
pub mod printable_bearing;
pub use printable_bearing::{Element, PrintableBearingDef};
// Seats on shafts and in holes: pulleys and gears placed by a shaft (mitcad#578).
pub mod seat;
pub use seat::{Seat, SeatFrame};
// `map_params!`: the regular `map_params` written from the fields
// (mitcad#586).
mod mapping;
// Placement by picked geometry; Gear Pair and Belt Drive (mitcad#656).
pub mod drives;
pub mod place_at;
pub use place_at::{PlaceAt, Placed};
// Clearance holes and head recesses by screw size (mitcad#903).
pub mod hole_sizes;
// Feature inputs drawn where they were: a Fillet's or Chamfer's picked
// edges kept with its result (mitcad#957).
pub mod inputs;
pub use inputs::FeatureInputs;
// What a Join touches in child occurrences when it touches none of its
// participants (mitcad#958).
mod join_elsewhere;

use crate::assembly::Assembly;
use crate::datum::Datum;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLinkExt;
use crate::parameters::{ParamId, Parameters};
use crate::recompute::{BodyState, FeatureResult, Pending, Read, Version, Versioned};
use crate::topo::EdgeName;
use crate::transform::Transform;

// A value in a command (moved down to the base, mitcad#780).
pub use mitcad_model_base::values::ValueInput;

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
            /// Every feature type's name, in the order of the types (what
            /// MCP lists, mitcad#714).
            pub const TYPE_NAMES: &'static [&'static str] = &[$($def::<ParamId>::TYPE,)*];

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
    // Assembly constraints (mitcad#148).
    ConstraintSet(ConstraintSetDef),
    // Placeholders of imported items that did not come in (mitcad#170).
    Placeholder(PlaceholderDef),
    PatternAtPoints(PointPatternDef),
    SheetRules(SheetRulesDef), SheetFace(SheetFaceDef),
    SheetFlange(SheetFlangeDef), SheetBend(SheetBendDef), SheetFold(SheetFoldDef), SheetCornerRound(SheetCornerRoundDef), SheetCornerChamfer(SheetCornerChamferDef),
    // Sheet-metal Unfold, Refold and the minimal Cut (mitcad#208).
    SheetUnfold(SheetUnfoldDef), SheetRefold(SheetRefoldDef), SheetCut(SheetCutDef),
    // Bodies taken out of the design.
    RemoveBody(RemoveBodyDef),
    // Sheet-metal corner relief (mitcad#209).
    SheetCornerRelief(SheetCornerReliefDef),
    // Bodies of other components copied, linked to them (mitcad#324).
    Derive(DeriveDef),
    // BUILD: derived parts and bound lengths (mitcad#375).
    DerivedPart(DerivedPartDef), BoundLength(BoundLengthDef),
    // Sheet-metal Contour Flange, Convert to Sheet Metal and Perforate (mitcad#461).
    SheetContour(SheetContourDef), SheetPerforate(SheetPerforateDef), SheetConvert(SheetConvertDef),
    // Motion links between joints (mitcad#466).
    MotionLink(MotionLinkDef),
    // Emboss: profiles raised or sunk on faces, wrapped on cylinders (mitcad#458).
    Emboss(EmbossDef),
    // Bounding solids (mitcad#460).
    BoundingSolid(BoundingSolidDef),
    // Boundary Fill: solids of the cells tool bodies enclose (mitcad#457).
    BoundaryFill(BoundaryFillDef),
    // Convert Mesh (mitcad#462).
    MeshConvert(MeshConvertDef),
    // Patch, Stitch and Thicken: surface bodies filled, sewn and made solid (mitcad#456).
    Patch(PatchDef), Stitch(StitchDef), Thicken(ThickenDef),
    // Surface modelling: surface bodies and their features (mitcad#455).
    OffsetSurface(OffsetSurfaceDef), RuledSurface(RuledSurfaceDef), ExtendSurface(ExtendSurfaceDef),
    TrimSurface(TrimSurfaceDef), UntrimSurface(UntrimSurfaceDef), Unstitch(UnstitchDef),
    ReverseNormal(ReverseNormalDef),
    BlendSurface(BlendSurfaceDef), BoundarySurface(BoundarySurfaceDef), MatchSurface(MatchSurfaceDef),
    FairSurface(FairSurfaceDef),
    // Plastic part features and plastic rules (mitcad#459).
    PlasticRule(PlasticRuleDef), Boss(BossDef), Lip(LipDef), Groove(GrooveDef), SnapFit(SnapFitDef), Rest(RestDef),
    // Bodies moved into another component on the timeline (mitcad#465).
    BodyTransfer(BodyTransferDef),
    // Gears: spur, helical, herringbone, ring, rack and bevel (mitcad#472).
    Gear(GearDef),
    // Faces moved by a rigid motion (mitcad#343).
    MoveFace(MoveFaceDef),
    // Scale patterns: copies scaled where they are (mitcad#539).
    ScalePattern(ScalePatternDef),
    // Timing belt pulleys and idlers (mitcad#562).
    Pulley(PulleyDef),
    // Printable bearings (mitcad#577).
    PrintableBearing(PrintableBearingDef),
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

    /// The motion of a Move, a Move of occurrences or a Move Face, whose
    /// points and axes may be picked in occurrences (mitcad#699).
    pub(crate) fn move_spec(&self) -> Option<&MoveSpec> {
        match self {
            FeatureDef::Move(m) => Some(&m.transform),
            FeatureDef::MoveOccurrence(m) => Some(&m.transform),
            FeatureDef::MoveFace(m) => Some(&m.transform),
            _ => None,
        }
    }

    /// [`FeatureDef::move_spec`], to change.
    pub(crate) fn move_spec_mut(&mut self) -> Option<&mut MoveSpec> {
        match self {
            FeatureDef::Move(m) => Some(&mut m.transform),
            FeatureDef::MoveOccurrence(m) => Some(&mut m.transform),
            FeatureDef::MoveFace(m) => Some(&mut m.transform),
            _ => None,
        }
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

    /// The name of the component a new-component operation makes, from
    /// the definition and its values (mitcad#577: `Bearing 608 (8x22x7)`);
    /// None: `Component<n>`.
    fn component_name(&self, _value: &dyn Fn(ParamId) -> f64) -> Option<String> {
        None
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

    /// What the open command's preview of this feature shows besides its
    /// result, made only for that preview and never cached (mitcad#712:
    /// a split's or trim's tool as it extends it), with `ctx` as the
    /// feature's evaluation sees the design. None: the preview shows the
    /// feature's own `tool`, if any.
    fn preview_tool(&self, _ctx: &mut EvalContext<'_, K>) -> Option<K::Shape> {
        None
    }
}

/// What a feature produced. A failed feature produces nothing and the body
/// state passes it by.
#[derive(Debug)]
pub struct FeatureOutput<S> {
    pub sheet_rules: Option<SheetRulesDef<f64>>,
    pub sheets: BTreeMap<BodyUid, crate::sheet::SheetChange>,
    pub sketch: Option<SketchOutput>,
    pub changes: Vec<BodyChange<S>>,
    /// Explicit ownership for cross-component results, including split pieces.
    /// Missing entries retain each feature's existing implicit ownership rules.
    pub owners: BTreeMap<BodyUid, ComponentUid>,
    /// The tool body before a boolean, for previews.
    pub tool: Option<S>,
    /// The plane, axis or point of a construction feature.
    pub datum: Option<Datum>,
    /// Occurrences it moves (F6).
    pub placements: Vec<PlacementChange>,
    /// A derived part's mount frames by name, in its component's
    /// coordinates (mitcad#375).
    pub mounts: BTreeMap<String, Transform>,
    /// The values of reference parameters the feature measured (a bound
    /// length, mitcad#375): the features after it use them.
    pub measured: Vec<(ParamId, f64)>,
    /// A tapped hole's walls whose modelled thread has its crest off them
    /// (leaving no piece of them, or only pieces outside the thread's
    /// part), as they were before the thread (mitcad#436, mitcad#628): the
    /// threads query lists the thread from them.
    pub threaded_walls: Vec<hole::ThreadedWall>,
    /// What the feature picked, drawn where it was (a Fillet's or a
    /// Chamfer's edges, mitcad#957); None when the kernel gave nothing.
    pub inputs: Option<FeatureInputs>,
}

impl<S> Default for FeatureOutput<S> {
    fn default() -> Self {
        Self {
            sheet_rules: None,
            sheets: BTreeMap::new(),
            sketch: None,
            changes: Vec::new(),
            owners: BTreeMap::new(),
            tool: None,
            datum: None,
            placements: Vec::new(),
            mounts: BTreeMap::new(),
            measured: Vec::new(),
            threaded_walls: Vec::new(),
            inputs: None,
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
    /// Where the features of `all` are (mitcad#841).
    pub(crate) index: &'a crate::feature_index::FeatureIndex,
}

impl CheckContext<'_> {
    /// A feature before the one being checked.
    pub fn feature(&self, uid: FeatureUid) -> Result<&FeatureEntry, String> {
        // The feature and whether it comes before this one: `earlier`
        // begins `all`, so the index of `all` tells both.
        let begins_all = self.earlier.len() <= self.all.len()
            && std::ptr::eq(self.earlier.as_ptr(), self.all.as_ptr());
        let found = if begins_all {
            self.index
                .position(self.all, uid)
                .map(|i| (i < self.earlier.len(), &self.all[i]))
        } else {
            self.earlier
                .iter()
                .find(|f| f.uid == uid)
                .map(|f| (true, f))
                .or_else(|| self.all.iter().find(|f| f.uid == uid).map(|f| (false, f)))
        };
        match found {
            Some((true, entry)) => Ok(&**entry),
            _ if uid == self.uid => Err("a feature cannot refer to itself".to_owned()),
            Some((false, later)) => Err(format!(
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
    pub sheets: &'a crate::sheet::SheetStates,
    pub sheet_rules: &'a crate::sheet::SheetRules,
    pub params: &'a Parameters,
    /// The bodies of the feature's component.
    pub bodies: &'a BodyState<S>,
    /// The bodies whose states hold a stand-in, with the restored results
    /// that set them (mitcad#858): a body read is unpacked as it is read
    /// (mitcad#886, [`EvalContext::body`] and the other readers).
    pub pending: &'a Pending<S>,
    /// The component of every body at this point, for messages about the
    /// bodies of other components.
    pub owners: &'a BTreeMap<BodyUid, ComponentUid>,
    pub assembly: &'a Assembly,
    pub sketches: &'a BTreeMap<FeatureUid, Versioned<Arc<SketchOutput>>>,
    pub datums: &'a BTreeMap<FeatureUid, Versioned<Datum>>,
    pub features: &'a [Arc<FeatureEntry>],
    /// Where the features of `features` are (mitcad#841).
    pub index: &'a crate::feature_index::FeatureIndex,
    /// Results of the features before this one.
    pub history: &'a [FeatureResult<S>],
    /// Every component's bodies, for linked geometry (mitcad#100).
    pub components: &'a BTreeMap<ComponentUid, Arc<BodyState<S>>>,
    /// Every occurrence's placement at this point of the timeline.
    pub placements: &'a BTreeMap<OccurrenceUid, Transform>,
    /// B-rep data of base features being added and the shapes it was
    /// written from (mitcad#244, `recompute::Cache::written_from`).
    pub written_from: &'a [(Brep, S)],
    /// The document's join tolerance (`DocState::join_tolerance`,
    /// mitcad#298), read through [`EvalContext::join`].
    pub join_tolerance: Option<f64>,
    /// The design's library sources, read by derived parts (mitcad#375).
    pub sources: &'a crate::build::LibrarySources,
    /// The pieces each feature remembers of the bodies it split, and the
    /// bodies that are other pieces than before or pieces no longer made,
    /// at this point (mitcad#408, `pieces.rs`).
    pub split_pieces: &'a SplitPieces,
    pub changed_pieces: &'a BTreeMap<BodyUid, String>,
    pub gone_pieces: &'a BTreeMap<BodyUid, FeatureUid>,
}

// (References only: an evaluation and its preview's extra see the same,
// mitcad#712.)
impl<S> Clone for Env<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for Env<'_, S> {}

impl<'a, S> Env<'a, S> {
    /// The feature `uid` (mitcad#841: by the index, not a search).
    pub(crate) fn feature(&self, uid: FeatureUid) -> Option<&'a Arc<FeatureEntry>> {
        self.index.find(self.features, uid)
    }

    /// The result of the earlier feature `uid`: a recompute's results are
    /// in timeline order, one per feature, so it is where the feature is
    /// in the timeline; else (results of another kind) searched for.
    pub(crate) fn result_of(&self, uid: FeatureUid) -> Option<&'a FeatureResult<S>> {
        let history = self.history;
        if let Some(i) = self.index.position(self.features, uid)
            && let Some(result) = history.get(i)
            && result.uid == uid
        {
            return Some(result);
        }
        history.iter().find(|r| r.uid == uid)
    }
}

/// Inputs of an evaluating feature. Every read is recorded, so the
/// recompute cache can tell whether a stored result still applies.
pub struct EvalContext<'a, K: Kernel> {
    pub(crate) sheet_footprint: BTreeSet<BodyUid>,
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
    pub(crate) pattern_anchor: Option<point_pattern::PatternAnchor>,
    pub(crate) cut_targets: Option<extrude_scope::CutTargets>,
    /// Where a sweep's profile lies on its path (mitcad#795).
    pub(crate) sweep_place: Option<sweep::SweepPlace>,
    /// What a move's motion was worked out from (mitcad#838).
    pub(crate) move_basis: Option<moves::MotionBasis>,
    /// A pattern's copy of this feature (its tool rebuilt in place): an
    /// object that is no longer in its body is found in the body as it was
    /// before this feature, as the original found it.
    pub(crate) objects_before: Option<FeatureUid>,
    /// The bodies of other components that the geometry the feature refers
    /// to is on, by their links (a construction plane's, mitcad#196): read
    /// in their components and moved into the feature's coordinates.
    pub(crate) geom_links: std::collections::BTreeMap<BodyUid, crate::links::OccurrenceLink>,
    next_body: u32,
    /// The pieces of bodies it splits (mitcad#408).
    pub(crate) pieces: pieces::PieceState,
    /// How long unpacking the restored shapes it read took (mitcad#886):
    /// not part of the evaluation's time.
    pub(crate) unpack_time: std::time::Duration,
}

impl<'a, K: Kernel> EvalContext<'a, K> {
    pub(crate) fn new(kernel: &'a K, uid: FeatureUid, env: Env<'a, K::Shape>) -> Self {
        let pieces = pieces::PieceState::new(uid, env.split_pieces.get(&uid).cloned());
        Self {
            pieces,
            sheet_footprint: BTreeSet::new(),
            kernel,
            uid,
            env,
            reads: Vec::new(),
            warnings: Vec::new(),
            elements: Vec::new(),
            pattern_anchor: None,
            cut_targets: None,
            sweep_place: None,
            move_basis: None,
            objects_before: None,
            geom_links: std::collections::BTreeMap::new(),
            next_body: 0,
            unpack_time: std::time::Duration::ZERO,
        }
    }

    /// Unpacks the shapes of the bodies about to be read (each with the
    /// version its state holds) whose states hold a stand-in: the restored
    /// results that set them, on the machine's threads (mitcad#886). The
    /// bodies the feature does not read stay packed.
    fn unpack_read(&mut self, read: impl IntoIterator<Item = (BodyUid, Version)>) {
        if self.env.pending.is_empty() {
            return;
        }
        let start = std::time::Instant::now();
        if crate::recompute::unpack_read(self.env.pending, read) {
            self.unpack_time += start.elapsed();
        }
    }

    /// The feature succeeded with a caveat: it is shown as a warning.
    pub(crate) fn sheet_rules(
        &mut self,
        uid: FeatureUid,
    ) -> Result<Versioned<SheetRulesDef<f64>>, String> {
        let rules = self.env.sheet_rules.get(&uid);
        self.reads
            .push(Read::SheetRules(uid, rules.map(|r| r.version)));
        rules
            .cloned()
            .ok_or_else(|| format!("sheet rules {uid} are unavailable"))
    }

    pub(crate) fn sheet(&mut self, body: BodyUid) -> Option<Versioned<crate::sheet::SheetState>> {
        let state = self.env.sheets.get(&body);
        self.reads.push(Read::Sheet(body, state.map(|s| s.version)));
        state.cloned()
    }

    /// Reads valid material domains; blocked retained geometry cannot be used
    /// as a successful upstream sheet result by a later sheet operation.
    pub fn require_sheet(&mut self, body: BodyUid) -> Result<crate::sheet::SheetState, String> {
        let state = self
            .sheet(body)
            .ok_or_else(|| format!("sheet {body} is unavailable"))?;
        if state.value.status() != "valid" {
            return Err(format!("sheet {body} is {}", state.value.status()));
        }
        let owner = self.env.owners.get(&body).copied();
        let shape = owner
            .and_then(|c| self.env.components.get(&c))
            .and_then(|b| b.get(&body));
        if owner != Some(state.value.owner)
            || shape.is_none_or(|b| format!("{:032x}", b.version) != state.value.shape_generation)
        {
            return Err(format!("sheet {body} does not match current geometry"));
        }
        Ok(state.value)
    }

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

    /// [`Kernel::boolean`]; a join or a cut within the document's join
    /// tolerance where it has one ([`Kernel::join_within`], mitcad#298, and
    /// [`Kernel::cut_within`], mitcad#235: an importer's bodies that match a
    /// file's only within its geometric tolerance). A kernel without them
    /// joins and cuts exactly.
    pub(crate) fn boolean(
        &mut self,
        op: crate::kernel::BooleanOp,
        targets: &[&K::Shape],
        tool: &K::Shape,
    ) -> Result<crate::kernel::BooleanOutput<K::Shape>, crate::kernel::KernelError> {
        if op != crate::kernel::BooleanOp::Intersect {
            let read = Read::JoinTolerance(self.env.join_tolerance.map(f64::to_bits));
            if !self.reads.contains(&read) {
                self.reads.push(read);
            }
            if let Some(tolerance) = self.env.join_tolerance {
                let widened = if op == crate::kernel::BooleanOp::Join {
                    self.kernel.join_within(targets, tool, tolerance)
                } else {
                    self.kernel.cut_within(targets, tool, tolerance)
                };
                match widened {
                    Err(crate::kernel::KernelError::Unsupported(_)) => {}
                    result => return result,
                }
            }
        }
        self.kernel.boolean(op, targets, tool)
    }

    /// [`Kernel::unite`]; within the document's join tolerance where it has
    /// one ([`Kernel::unite_within`], mitcad#397: a Combine's join of an
    /// importer's bodies whose faces lie on each other only within the
    /// file's geometric tolerance). A kernel without it unites exactly.
    pub(crate) fn unite(
        &mut self,
        shapes: &[&K::Shape],
    ) -> Result<K::Shape, crate::kernel::KernelError> {
        let read = Read::JoinTolerance(self.env.join_tolerance.map(f64::to_bits));
        if !self.reads.contains(&read) {
            self.reads.push(read);
        }
        if let Some(tolerance) = self.env.join_tolerance {
            match self.kernel.unite_within(shapes, tolerance) {
                Err(crate::kernel::KernelError::Unsupported(_)) => {}
                result => return result,
            }
        }
        self.kernel.unite(shapes)
    }

    pub fn param(&mut self, id: ParamId) -> Result<f64, String> {
        let value = self.env.params.value(id);
        self.reads
            .push(Read::Param(id, self.env.params.read_bits(id)));
        // A text parameter (mitcad#664) gives no number.
        if self.env.params.text(id).is_some() {
            return Err(format!(
                "parameter {} is a text, not a number",
                self.env.params.name(id)
            ));
        }
        value.ok_or_else(|| format!("parameter {} does not exist", self.env.params.name(id)))
    }

    /// The text of a text parameter (mitcad#664), such as a sketch text's.
    pub fn param_text(&mut self, id: ParamId) -> Result<String, String> {
        self.reads
            .push(Read::Param(id, self.env.params.read_bits(id)));
        let params = self.env.params;
        match (params.get(id), params.text(id)) {
            (_, Some(text)) => Ok(text.to_owned()),
            (Some(_), None) => Err(format!(
                "parameter {} is a number, not a text",
                params.name(id)
            )),
            (None, None) => Err(format!("parameter {} does not exist", params.name(id))),
        }
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
        let env = self.env;
        let body = env.bodies.get(&uid);
        self.reads.push(Read::Body(uid, body.map(|b| b.version)));
        if let Some(body) = body {
            self.unpack_read([(uid, body.version)]);
            let shape = crate::recompute::held_shape(env.pending, uid, body)
                .value
                .clone();
            // Another piece of a split than before (mitcad#408).
            self.warn_changed_piece(uid);
            return Ok(shape);
        }
        let creator = self.feature_name(uid.feature);
        Err(match self.env.owners.get(&uid) {
            Some(owner) => format!(
                "body {uid} (from {creator}) belongs to {}; a feature works on the bodies \
                 of its own component (activate that component)",
                self.env.assembly.name(*owner)
            ),
            None => match self.gone_piece(uid) {
                Some(gone) => gone,
                None => format!(
                    "body {uid} (from {creator}) does not exist at this point of the timeline"
                ),
            },
        })
    }

    /// All bodies of the feature's component, in order of their ids.
    pub fn bodies(&mut self) -> Vec<(BodyUid, K::Shape)> {
        let env = self.env;
        let read: Vec<(BodyUid, Version)> =
            env.bodies.iter().map(|(u, b)| (*u, b.version)).collect();
        self.unpack_read(read.iter().copied());
        self.reads.push(Read::AllBodies(read));
        env.bodies
            .iter()
            .map(|(u, b)| {
                (
                    *u,
                    crate::recompute::held_shape(env.pending, *u, b)
                        .value
                        .clone(),
                )
            })
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
        for uid in link.source.0.iter().chain(&link.target.0) {
            self.placement(*uid);
        }
        let source = link.source_component(assembly)?;
        let component = self.component();
        let target = link.target_path(assembly, component)?;
        for uid in link.source.0.iter().chain(&target) {
            self.placement(*uid);
            if assembly
                .occurrence(*uid)
                .is_some_and(|o| o.pattern.is_some())
                && !self.env.placements.contains_key(uid)
            {
                return Err(format!(
                    "pattern occurrence {uid} is unavailable at this timeline position"
                ));
            }
        }
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
        let env = self.env;
        let body = env.components.get(&component).and_then(|b| b.get(&uid));
        self.reads
            .push(Read::BodyIn(component, uid, body.map(|b| b.version)));
        if let Some(body) = body {
            self.unpack_read([(uid, body.version)]);
            self.warn_changed_piece(uid);
        }
        body.map(|b| {
            crate::recompute::held_shape(env.pending, uid, b)
                .value
                .clone()
        })
        .ok_or_else(|| {
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
        let env = self.env;
        let bodies: Vec<(BodyUid, &Versioned<K::Shape>)> = env
            .components
            .get(&component)
            .into_iter()
            .flat_map(|b| b.iter())
            .map(|(uid, b)| (*uid, b))
            .collect();
        let read: Vec<(BodyUid, Version)> =
            bodies.iter().map(|(uid, b)| (*uid, b.version)).collect();
        self.unpack_read(read.iter().copied());
        self.reads.push(Read::BodiesIn(component, read));
        bodies
            .into_iter()
            .map(|(uid, b)| {
                (
                    uid,
                    crate::recompute::held_shape(env.pending, uid, b)
                        .value
                        .clone(),
                )
            })
            .collect()
    }

    /// The id of the next body this feature creates; ids of pieces it
    /// remembers are passed by (mitcad#408).
    pub fn new_body(&mut self) -> BodyUid {
        if !self.pieces.reserved.is_empty() {
            self.pieces.used = true;
        }
        loop {
            self.next_body += 1;
            let index = self.next_body - 1;
            if !self.pieces.reserved.contains(&index) {
                return BodyUid::new(self.uid, index);
            }
        }
    }

    pub fn feature_name(&self, uid: FeatureUid) -> String {
        self.env
            .feature(uid)
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

    /// Checks that every edge resolves on the shape and gives the names
    /// the kernel gets: each edge's own, or that of the edge between the
    /// faces a sketch change renamed, with a warning (`renamed.rs`,
    /// mitcad#272).
    pub fn require_edges(
        &mut self,
        body: BodyUid,
        shape: &K::Shape,
        edges: &[EdgeName],
    ) -> Result<Vec<EdgeName>, String> {
        self.edges_on(body, shape, edges)
    }
}

/// Serde helper for `skip_serializing_if`.
pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

/// The last component of a slot path: `extent.distance` -> `distance`.
#[doc(hidden)]
pub fn slot_label(slot: &str) -> &str {
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
