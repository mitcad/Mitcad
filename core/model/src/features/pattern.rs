// SPDX-License-Identifier: MIT
//! Patterns (Rectangular, Circular and Path Pattern), and the
//! instance logic mirrors share. A pattern places its objects at elements:
//! element 0 is the original (the identity), every other element a copy.
//!
//! - Bodies: each copy is a new body, `<pattern>.b<k>` with
//!   k = (element − 1) · (number of bodies) + (body's position), so the ids
//!   of the other copies stay when an element is suppressed.
//! - Features: the features' tool bodies ([`super::FeatureInfo::tool_use`])
//!   are combined with the bodies again at each element, with the feature's
//!   operation. Adjust (the default) applies the elements one after
//!   another, rebuilding a feature's tool in the element's place when the
//!   feature can ([`super::Evaluate::placed_tool`]) and skipping elements
//!   that reach no body; for a cut or a join it combines them all at once
//!   when that gives the same bodies (no body split apart), which is much
//!   faster on large bodies. Identical and
//!   Optimized move the tool the feature left to every element and combine
//!   them all at once; consecutive features that cut the same bodies (a
//!   hole and the copies of a pattern of it) are cut in one operation with
//!   all their copies. Groups of copies whose boxes stay away from the
//!   bodies' boxes change nothing and are left out of a cut (and of
//!   Adjust's join), before the copies are united ([`reaching`]). A run
//!   of features that all join, or all cut, one body, whose copies stay
//!   apart from the other features' copies, goes in one operation where
//!   that gives the same body and face names (mitcad#855,
//!   [`Copying::together`]).
//! - Patterns and mirrors of features among the features (patterns of
//!   patterns, mitcad#4): their originals are repeated at every place the
//!   pattern put them (its elements, the original's too), so a pattern of
//!   a pattern places the originals at every product of their elements,
//!   the inner element first. Each copy's faces carry the inner instance's
//!   name inside the outer one's.
//! - Fillets and chamfers among the features (mitcad#105) are applied
//!   again on the copies of the edges they round, found by the copies'
//!   names ([`super::dressup_copies`]); the features then go in timeline
//!   order. With Optimized the faces the features made are copied
//!   instead: the material they added and removed, moved to every
//!   element and joined and cut in one operation each
//!   (`face_copy.rs`), with no fillet or chamfer worked out again.
//! - Faces: the faces of a body bound a boss or a pocket together with
//!   planar caps ([`crate::kernel::Kernel::face_tool`]); its copies are
//!   joined to or cut from the body like a feature's tool.
//!
//! Copied faces are named `<pattern>:inst<element>(<original name>)`.
//!
//! Element numbering: along a direction the positions are 0, 1, …, q − 1;
//! when symmetric the q elements lie on both sides of the original: 0, 1,
//! …, q − 1 − ⌊q / 2⌋, then −1, …, −⌊q / 2⌋ (three elements: 0, 1, −1; four:
//! 0, 1, −1, −2). A rectangular pattern numbers the elements direction 1
//! first: element = k1 + n1 · k2. The ids in .f3d designs (experiment K43)
//! may differ; the import matches elements by transform.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::base::{Joined, body_lumps, join_pieces};
use super::bodies::{BodySet, apply_tool, apply_tool_strays};
use super::dressup_copies::{copied_features, is_dressup, repeat_dressup};
use super::face_refs::UNSUPPORTED;
use super::geom_ref::{GeomRef, PathChain, PathCurve, PathRef, Want, path_curve};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureDef, FeatureEntry, FeatureInfo,
    FeatureOutput, Operation, References, ToolUse, is_false,
};
use mitcad_vec3::{add, cross, dot, scale as scaled, sub};

use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{
    BooleanOp, BooleanOutput, BooleanPiece, BoundingBox, Kernel, KernelError, ToolContact,
};
use crate::parameters::ParamId;
use crate::recompute::Read;
use crate::topo::FaceName;
use crate::transform::{Instance, Transform};

/// What a pattern or a mirror repeats (`patternEntityType` in .f3d designs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PatternObjects {
    Bodies {
        bodies: Vec<BodyUid>,
    },
    Features {
        features: Vec<FeatureUid>,
    },
    Faces {
        body: BodyUid,
        faces: Vec<FaceName>,
    },
    /// Repeated occurrences of shared component definitions (Pattern at Points).
    Components {
        occurrences: Vec<super::OccurrencePath>,
    },
}

/// The compute option for patterns and mirrors of features.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputeOption {
    /// Each element separately, its tool rebuilt in place where possible.
    #[default]
    Adjust,
    /// The original tool moved to every element.
    Identical,
    /// As Identical, but the fillets and chamfers among the features are
    /// copied as the faces the features made (`face_copy.rs`) instead
    /// of being worked out again on each copy.
    Optimized,
}

pub(super) fn is_adjust(option: &ComputeOption) -> bool {
    *option == ComputeOption::Adjust
}

/// How spacing is given (`PatternDistanceType` in .f3d designs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingType {
    /// The distance between neighbouring elements.
    Spacing,
    /// The distance from the first element to the last.
    Extent,
}

/// One direction of a rectangular pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternDirection<P = ParamId> {
    pub axis: GeomRef,
    /// Elements along the direction, the original included.
    pub quantity: P,
    /// Spacing or extent (see [`SpacingType`]); negative goes against the
    /// axis.
    pub distance: P,
    /// The `quantity` elements on both sides of the original, ⌊quantity /
    /// 2⌋ of them before it (the extent and the spacing as without).
    #[serde(default, skip_serializing_if = "is_false")]
    pub symmetric: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RectangularPatternDef<P = ParamId> {
    pub objects: PatternObjects,
    pub direction1: PatternDirection<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub direction2: Option<PatternDirection<P>>,
    pub distance_type: SpacingType,
    #[serde(default, skip_serializing_if = "is_adjust")]
    pub compute: ComputeOption,
    /// Copies of features act only on the bodies each feature changed
    /// where the feature has no participants of its own (the `.f3d`
    /// import's patterns, mitcad#74); by default on the feature's
    /// participants, every body without them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub original_bodies: bool,
    /// Scales the copies (FreeCAD's Scaled transformation, mitcad#59): the
    /// last element's factor; element e of n is scaled by
    /// 1 + (factor − 1) · e / (n − 1) about the centre of mass of the first
    /// object (a feature's tool, a body) carried to the copy (through the
    /// elements of patterns among the features too, mitcad#539).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale: Option<P>,
    /// The runs the copies are scaled in ([`Scaling`], mitcad#539): the
    /// copies of the originals, numbered the inner patterns' copies first,
    /// fall into this many runs of equal length, run g scaled by
    /// 1 + (scale − 1) · g / (groups − 1). Default: the elements (each
    /// element its own run).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_groups: Option<P>,
    /// Joins each copy of a body to its original when they touch, as a
    /// mirror's `combine` (mitcad#539); bodies only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine: bool,
    /// With `combine`: a copy joins every body of the pattern it touches,
    /// not only its original ([`Combine::Any`], mitcad#1006).
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine_any: bool,
    /// The copies the scale's runs are counted over (mitcad#542; default
    /// the pattern's own: the inner patterns' copies times its elements):
    /// more for a pattern inside a pattern of patterns whose runs a later
    /// pattern's scale continues ([`Scaling`]).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_copies: Option<P>,
    /// More scales, each as `scale` with its own runs; the factors
    /// multiply (mitcad#542).
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub scales: Vec<ScaleRule<P>>,
    /// The point the copies are scaled about, carried to each copy,
    /// instead of the first object's centre of mass (mitcad#542).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_center: Option<GeomRef>,
    /// Copies of features the pattern does not make, by their numbers
    /// p + P · e (p the inner copy, P the inner copies, e the element;
    /// mitcad#542): patterns of the pattern still place the originals
    /// there, unlike `suppressed_elements`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_copies: Vec<u32>,
    /// Elements left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed_elements: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CircularPatternDef<P = ParamId> {
    pub objects: PatternObjects,
    pub axis: GeomRef,
    /// Elements, the original included.
    pub quantity: P,
    /// The total angle (radians): a full turn spaces the elements by
    /// angle / quantity, a partial one by angle / (quantity − 1); negative
    /// turns the other way.
    pub angle: P,
    /// The `quantity` elements on both sides of the original, ⌊quantity /
    /// 2⌋ of them turned back from it, at the same spacing as without
    /// (not for a full turn).
    #[serde(default, skip_serializing_if = "is_false")]
    pub symmetric: bool,
    #[serde(default, skip_serializing_if = "is_adjust")]
    pub compute: ComputeOption,
    /// Copies of features act only on the bodies each feature changed
    /// where the feature has no participants of its own (the `.f3d`
    /// import's patterns, mitcad#74); by default on the feature's
    /// participants, every body without them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub original_bodies: bool,
    /// Scales the copies, as [`RectangularPatternDef::scale`].
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale: Option<P>,
    /// As [`RectangularPatternDef::scale_groups`].
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_groups: Option<P>,
    /// As [`RectangularPatternDef::combine`].
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine: bool,
    /// As [`RectangularPatternDef::combine_any`].
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine_any: bool,
    /// As [`RectangularPatternDef::scale_copies`].
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_copies: Option<P>,
    /// As [`RectangularPatternDef::scales`].
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub scales: Vec<ScaleRule<P>>,
    /// As [`RectangularPatternDef::scale_center`].
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_center: Option<GeomRef>,
    /// As [`RectangularPatternDef::skipped_copies`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_copies: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed_elements: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPatternDef<P = ParamId> {
    pub objects: PatternObjects,
    /// A sketch line, arc or circle, or a fixed line (paths of edges are
    /// not supported yet).
    pub path: PathRef,
    pub quantity: P,
    /// Along the path (arc length); negative goes backwards.
    pub distance: P,
    pub distance_type: SpacingType,
    /// Where the original sits on the path, 0 at its start and 1 at its end.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub start: f64,
    /// Where the original sits on the path given by a point instead of
    /// `start` (mitcad#673): the path's point nearest to it (mm). The `.f3d`
    /// import's patterns keep their objects' centre.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub start_point: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// Turns the copies with the path's direction (`isOrientationAlongPath`
    /// in .f3d designs); they keep the original's orientation by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub along_path: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub symmetric: bool,
    #[serde(default, skip_serializing_if = "is_adjust")]
    pub compute: ComputeOption,
    /// Copies of features act only on the bodies each feature changed
    /// where the feature has no participants of its own (the `.f3d`
    /// import's patterns, mitcad#74); by default on the feature's
    /// participants, every body without them.
    #[serde(default, skip_serializing_if = "is_false")]
    pub original_bodies: bool,
    /// Scales the copies, as [`RectangularPatternDef::scale`].
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale: Option<P>,
    /// As [`RectangularPatternDef::scale_groups`].
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_groups: Option<P>,
    /// As [`RectangularPatternDef::combine`]; for features too (mitcad#673):
    /// the bodies the copies make (copies of a feature that made a new
    /// body) join the body of the features, where they touch it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine: bool,
    /// As [`RectangularPatternDef::combine_any`]; bodies and patterns of
    /// bodies only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub combine_any: bool,
    /// As [`RectangularPatternDef::scale_copies`].
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_copies: Option<P>,
    /// As [`RectangularPatternDef::scales`].
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub scales: Vec<ScaleRule<P>>,
    /// As [`RectangularPatternDef::scale_center`].
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub scale_center: Option<GeomRef>,
    /// As [`RectangularPatternDef::skipped_copies`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_copies: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed_elements: Vec<u32>,
}

/// Serde default of optional values, without the `Default` bound that
/// `#[serde(default)]` puts on the value type.
pub(crate) fn none<T>() -> Option<T> {
    None
}

pub(super) fn map_scale<P, Q, E>(
    scale: &Option<P>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<Q>, E> {
    scale.as_ref().map(|p| f("scale", p)).transpose()
}

pub(super) fn map_scale_groups<P, Q, E>(
    groups: &Option<P>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<Q>, E> {
    groups.as_ref().map(|p| f("scale_groups", p)).transpose()
}

/// One more scale of a pattern's copies (`scales`, mitcad#542): its
/// factor, runs and the copies they are counted over, as a pattern's
/// `scale`, `scale_groups` and `scale_copies`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaleRule<P = ParamId> {
    pub scale: P,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub groups: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub copies: Option<P>,
}

pub(super) fn map_scale_copies<P, Q, E>(
    copies: &Option<P>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<Q>, E> {
    copies.as_ref().map(|p| f("scale_copies", p)).transpose()
}

pub(super) fn map_scales<P, Q, E>(
    scales: &[ScaleRule<P>],
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Vec<ScaleRule<Q>>, E> {
    let mut out = Vec::with_capacity(scales.len());
    for (i, rule) in scales.iter().enumerate() {
        out.push(ScaleRule {
            scale: f(&format!("scales[{i}].scale"), &rule.scale)?,
            groups: rule
                .groups
                .as_ref()
                .map(|p| f(&format!("scales[{i}].groups"), p))
                .transpose()?,
            copies: rule
                .copies
                .as_ref()
                .map(|p| f(&format!("scales[{i}].copies"), p))
                .transpose()?,
        });
    }
    Ok(out)
}

/// A pattern's or mirror's scale options together ([`Scaling`]).
#[derive(Clone, Copy)]
pub(super) struct ScaleSpec<'a> {
    pub scale: Option<ParamId>,
    pub groups: Option<ParamId>,
    pub copies: Option<ParamId>,
    pub more: &'a [ScaleRule],
    pub center: Option<&'a GeomRef>,
}

impl ScaleSpec<'_> {
    /// Whether it scales at all.
    fn scales(&self) -> bool {
        self.scale.is_some() || !self.more.is_empty()
    }
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

impl<P> PatternDirection<P> {
    // Written by hand: the slots take the direction (`direction1.quantity`).
    fn map_params<Q, E>(
        &self,
        prefix: &str,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PatternDirection<Q>, E> {
        Ok(PatternDirection {
            axis: self.axis.clone(),
            quantity: f(&format!("{prefix}.quantity"), &self.quantity)?,
            distance: f(&format!("{prefix}.distance"), &self.distance)?,
            symmetric: self.symmetric,
        })
    }
}

impl<P> RectangularPatternDef<P> {
    pub const TYPE: &'static str = "rectangular_pattern";
    pub const BASE_NAME: &'static str = "RectangularPattern";

    // Written by hand: the directions map with their prefixes.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<RectangularPatternDef<Q>, E> {
        Ok(RectangularPatternDef {
            objects: self.objects.clone(),
            direction1: self.direction1.map_params("direction1", f)?,
            direction2: match &self.direction2 {
                Some(direction) => Some(direction.map_params("direction2", f)?),
                None => None,
            },
            distance_type: self.distance_type,
            compute: self.compute,
            original_bodies: self.original_bodies,
            scale: map_scale(&self.scale, f)?,
            scale_groups: map_scale_groups(&self.scale_groups, f)?,
            combine: self.combine,
            combine_any: self.combine_any,
            scale_copies: map_scale_copies(&self.scale_copies, f)?,
            scales: map_scales(&self.scales, f)?,
            scale_center: self.scale_center.clone(),
            skipped_copies: self.skipped_copies.clone(),
            suppressed_elements: self.suppressed_elements.clone(),
        })
    }
}

impl<P> CircularPatternDef<P> {
    pub const TYPE: &'static str = "circular_pattern";
    pub const BASE_NAME: &'static str = "CircularPattern";

    map_params! {
        CircularPatternDef {
            objects: clone,
            axis: clone,
            quantity: param,
            angle: param,
            symmetric: copy,
            compute: copy,
            original_bodies: copy,
            scale: with(map_scale),
            scale_groups: with(map_scale_groups),
            combine: copy,
            combine_any: copy,
            scale_copies: with(map_scale_copies),
            scales: with(map_scales),
            scale_center: clone,
            skipped_copies: clone,
            suppressed_elements: clone,
        }
    }
}

impl<P> PathPatternDef<P> {
    pub const TYPE: &'static str = "path_pattern";
    pub const BASE_NAME: &'static str = "PathPattern";

    map_params! {
        PathPatternDef {
            objects: clone,
            path: clone,
            quantity: param,
            distance: param,
            distance_type: copy,
            start: copy,
            start_point: copy,
            flip: copy,
            along_path: copy,
            symmetric: copy,
            compute: copy,
            original_bodies: copy,
            scale: with(map_scale),
            scale_groups: with(map_scale_groups),
            combine: copy,
            combine_any: copy,
            scale_copies: with(map_scale_copies),
            scales: with(map_scales),
            scale_center: clone,
            skipped_copies: clone,
            suppressed_elements: clone,
        }
    }
}

// Checks shared with mirrors.

impl PatternObjects {
    pub(crate) fn add_references(&self, references: &mut References) {
        match self {
            Self::Bodies { bodies } => {
                for body in bodies {
                    references.body(*body);
                }
            }
            Self::Features { features } => references.features.extend(features),
            Self::Components { .. } => {}
            Self::Faces { body, faces } => {
                references.body(*body);
                for face in faces {
                    references.features.extend(face.features());
                }
            }
        }
    }

    pub(crate) fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        fn unique<T: PartialEq + std::fmt::Display>(items: &[T], what: &str) -> Result<(), String> {
            if items.is_empty() {
                return Err(format!("no {what}s selected"));
            }
            for (i, item) in items.iter().enumerate() {
                if items[..i].contains(item) {
                    return Err(format!("{what} {item} is listed more than once"));
                }
            }
            Ok(())
        }
        match self {
            Self::Components { .. } => Err("components require Pattern at Points".to_owned()),
            Self::Bodies { bodies } => {
                unique(bodies, "body")?;
                bodies.iter().try_for_each(|body| ctx.body(*body))
            }
            Self::Features { features } => {
                unique(features, "feature")?;
                for uid in features {
                    let entry = ctx.feature(*uid)?;
                    // A pattern or mirror of features repeats its originals.
                    match repeated(&entry.def) {
                        // A pattern of bodies stands for its bodies at its
                        // elements (mitcad#547).
                        Some((Self::Features { .. } | Self::Bodies { .. }, _)) => continue,
                        Some(_) => {
                            return Err(format!(
                                "{} ({}) cannot be patterned or mirrored: it repeats faces, not \
                                 features or bodies",
                                entry.name, entry.uid
                            ));
                        }
                        None => {}
                    }
                    if replay_tool_definition(entry)?.is_none()
                        && !is_dressup(&entry.def)
                        && !changes_sheet(&entry.def)
                        && !makes_sheets(&entry.def)
                    {
                        return Err(format!(
                            "{} ({}) cannot be patterned or mirrored: a {} has no tool body",
                            entry.name,
                            entry.uid,
                            entry.def.type_name()
                        ));
                    }
                }
                Ok(())
            }
            Self::Faces { body, faces } => {
                unique(faces, "face")?;
                ctx.body(*body)?;
                for face in faces {
                    for feature in face.features() {
                        ctx.feature(feature).map_err(|e| format!("{face}: {e}"))?;
                    }
                }
                Ok(())
            }
        }
    }

    pub(crate) fn creates_bodies(&self) -> bool {
        true
    }
}

/// Scoped cuts need document-aware target resolution, which the ordinary
/// FeatureInfo::tool_use contract deliberately cannot provide (mitcad#569).
fn replay_tool_definition(entry: &FeatureEntry) -> Result<Option<ToolUse>, String> {
    if let FeatureDef::Extrude(extrude) = &entry.def
        && let Some(scope) = extrude.active_cut_scope()
    {
        scope.check_local_replay()?;
        return Ok(Some(ToolUse {
            operation: Operation::Cut,
            participants: Vec::new(),
        }));
    }
    Ok(entry.def.info().tool_use())
}

fn replay_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    uid: FeatureUid,
    set: Option<&BodySet<K::Shape>>,
) -> Result<Option<ToolUse>, String> {
    let entry = ctx.feature_entry(uid)?;
    if matches!(&entry.def, FeatureDef::Extrude(extrude) if extrude.active_cut_scope().is_some()) {
        // Read the source before classification can fail. An edit of its
        // selection must invalidate a failed replay even when the local
        // body's geometry is unchanged by that edit.
        let output = ctx
            .env
            .history
            .iter()
            .find(|result| result.uid == uid)
            .and_then(|result| result.output.as_ref());
        ctx.reads
            .push(Read::Output(uid, output.map(|output| output.version)));
    }
    let mut tool = replay_tool_definition(&entry)?;
    if let FeatureDef::Extrude(extrude) = &entry.def
        && let Some(scope) = extrude.active_cut_scope()
        && let Some(tool) = &mut tool
    {
        tool.participants = scope.local_replay_bodies(ctx)?;
        if let Some(set) = set {
            // Each copy may split a selected body. Its new pieces remain
            // targets of the component selection for subsequent copies.
            tool.participants = set
                .uids()
                .into_iter()
                .filter(|body| {
                    !set.get(*body).is_some_and(|shape| ctx.is_mesh(shape))
                        && !scope.excluded_bodies().iter().any(|excluded| {
                            excluded.body == *body
                                && excluded.component == scope.context.component
                                && excluded.occurrence == scope.context.occurrence
                        })
                })
                .collect();
            if tool.participants.is_empty() {
                return Err("no eligible local bodies selected for cut feature replay".to_owned());
            }
        }
    }
    Ok(tool)
}

pub(super) fn check_suppressed(suppressed: &[u32]) -> Result<(), String> {
    for (i, element) in suppressed.iter().enumerate() {
        if *element == 0 {
            return Err("element 0 is the original and cannot be suppressed".to_owned());
        }
        if suppressed[..i].contains(element) {
            return Err(format!("element {element} is listed more than once"));
        }
    }
    Ok(())
}

/// The number of elements a quantity parameter gives.
pub(super) fn quantity(what: &str, value: f64) -> Result<u32, String> {
    let rounded = value.round();
    if !value.is_finite() || (value - rounded).abs() > 1e-9 || rounded < 1.0 {
        return Err(mitcad_model_base::internal::parameters::count_error(
            format!("{what} must be a whole number of at least 1, got {value}"),
            value,
        ));
    }
    if rounded > f64::from(MAX_ELEMENTS) {
        return Err(format!(
            "{what} must be at most {MAX_ELEMENTS}, got {value}"
        ));
    }
    Ok(rounded as u32)
}

/// Elements of one pattern at most.
const MAX_ELEMENTS: u32 = 10_000;

/// Positions along a direction: 0, 1, …, q − 1; when symmetric the `q`
/// elements spread over both sides of the original, ⌊q / 2⌋ before it and
/// the rest after it (0, 1, …, q − 1 − ⌊q / 2⌋, then −1, …, −⌊q / 2⌋), as
/// the `.f3d` designs do (the one-option variants of a rectangular and a
/// circular pattern, mitcad#186).
fn positions(quantity: u32, symmetric: bool) -> Vec<f64> {
    if symmetric && quantity > 1 {
        let before = quantity / 2;
        let after = quantity - 1 - before;
        (0..=after)
            .map(f64::from)
            .chain((1..=before).map(|i| -f64::from(i)))
            .collect()
    } else {
        (0..quantity).map(f64::from).collect()
    }
}

/// The step between elements.
fn step(distance_type: SpacingType, distance: f64, quantity: u32) -> f64 {
    match distance_type {
        SpacingType::Spacing => distance,
        SpacingType::Extent if quantity > 1 => distance / f64::from(quantity - 1),
        SpacingType::Extent => 0.0,
    }
}

/// A copy: its element number (`inst<index>`) and transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Element {
    pub index: u32,
    pub transform: Transform,
}

/// What a pattern or a mirror repeats and the elements it leaves out; None
/// for other features.
pub(crate) fn repeated(def: &FeatureDef) -> Option<(&PatternObjects, &[u32])> {
    match def {
        FeatureDef::RectangularPattern(d) => Some((&d.objects, &d.suppressed_elements)),
        FeatureDef::CircularPattern(d) => Some((&d.objects, &d.suppressed_elements)),
        FeatureDef::PathPattern(d) => Some((&d.objects, &d.suppressed_elements)),
        FeatureDef::PatternAtPoints(d) => Some((&d.objects, &[])),
        FeatureDef::Mirror(d) => Some((&d.objects, &[])),
        FeatureDef::ScalePattern(d) => Some((&d.objects, &d.suppressed_elements)),
        _ => None,
    }
}

macro_rules! scale_options {
    ($($def:ident),*) => {$(
        impl $def {
            /// Its scale options together.
            pub(super) fn scale_spec(&self) -> ScaleSpec<'_> {
                ScaleSpec {
                    scale: self.scale,
                    groups: self.scale_groups,
                    copies: self.scale_copies,
                    more: &self.scales,
                    center: self.scale_center.as_ref(),
                }
            }
        }
    )*};
}

scale_options!(RectangularPatternDef, CircularPatternDef, PathPatternDef);

/// A pattern's or mirror's scale options; None when it does not scale.
fn scale_params(def: &FeatureDef) -> Option<ScaleSpec<'_>> {
    let spec = match def {
        FeatureDef::RectangularPattern(d) => d.scale_spec(),
        FeatureDef::CircularPattern(d) => d.scale_spec(),
        FeatureDef::PathPattern(d) => d.scale_spec(),
        FeatureDef::Mirror(d) => d.scale_spec(),
        FeatureDef::ScalePattern(d) => d.scale_spec(),
        _ => return None,
    };
    spec.scales().then_some(spec)
}

/// A sheet feature that changes the material of an existing sheet body
/// (mitcad#242): a pattern or mirror repeats the material it added and
/// removed ([`repeat_sheet_change`]), and the sheet-domain adapter takes
/// the result back into the sheet's domains.
pub(crate) fn changes_sheet(def: &FeatureDef) -> bool {
    match def {
        FeatureDef::SheetFace(d) => d.body.is_some(),
        FeatureDef::SheetFlange(_)
        | FeatureDef::SheetCut(_)
        | FeatureDef::SheetCornerRound(_)
        | FeatureDef::SheetCornerChamfer(_)
        | FeatureDef::SheetCornerRelief(_)
        | FeatureDef::SheetPerforate(_) => true,
        _ => false,
    }
}

/// A face that makes new sheet bodies, whose copies are new sheet bodies
/// ([`crate::sheet::replay::copy_faces`], mitcad#280).
pub(crate) fn makes_sheets(def: &FeatureDef) -> bool {
    matches!(def, FeatureDef::SheetFace(d) if d.body.is_none())
}

/// The copies of a sheet feature's change at the elements (mitcad#242):
/// the material it added to a body (its result less the body before it)
/// joined and the material it removed (the body before it less its result)
/// cut, each moved by the place's steps and the element's transform. The
/// feature's own operation is not repeated: its inputs (boundaries,
/// panels, corners) name domains of the original, not of the copies.
fn repeat_sheet_change<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    set: &mut BodySet<K::Shape>,
    place: &Placed,
    elements: &[Element],
) -> Result<bool, String> {
    let uid = place.feature;
    let at = place
        .steps
        .iter()
        .fold(Transform::IDENTITY, |done, (t, _)| t.after(&done));
    let mut reached = false;
    for body in ctx.feature_changed_bodies(uid)? {
        let Some(before) = ctx.body_before(uid, body) else {
            continue;
        };
        let Some(after) = ctx
            .env
            .history
            .iter()
            .find(|r| r.uid == uid)
            .and_then(|r| r.shape_set(body))
            .cloned()
        else {
            continue;
        };
        let volume = |shape: &K::Shape| -> Result<f64, String> {
            Ok(ctx
                .kernel
                .mass_properties(shape)
                .map_err(|e| e.to_string())?
                .volume)
        };
        let least = 1e-9 * volume(&before)?.abs().max(1.0);
        let mut changes = Vec::new();
        for (operation, from, less) in [
            (Operation::Join, &after, &before),
            (Operation::Cut, &before, &after),
        ] {
            let difference = ctx
                .kernel
                .boolean(BooleanOp::Cut, &[from], less)
                .map_err(|e| e.to_string())?;
            for piece in difference.pieces {
                if volume(&piece.shape)? > least {
                    changes.push((operation, piece.shape));
                }
            }
        }
        for element in elements {
            let transform = element.transform.after(&at);
            for (operation, piece) in &changes {
                let copy = ctx
                    .kernel
                    .transform_shape(
                        piece,
                        &transform,
                        Some(Instance {
                            feature: ctx.uid,
                            index: element.index,
                        }),
                    )
                    .map_err(|e| e.to_string())?;
                reached |= apply_tool(ctx, set, *operation, &[body], &copy)?;
            }
        }
    }
    Ok(reached)
}

/// A place of a feature with a tool among a pattern's objects: the steps
/// of the patterns of features (patterns of patterns) that took it there,
/// innermost first: each element's transform and the instance that names
/// its copies.
pub(super) struct Placed {
    pub(super) feature: FeatureUid,
    pub(super) steps: Vec<(Transform, Instance)>,
    /// The copy's number among the copies the patterns of patterns make of
    /// the original (the innermost pattern's element turning fastest, 0
    /// the original itself), and how many those are: a scaled pattern's
    /// runs count them ([`Scaling`]).
    pub(super) index: u32,
    pub(super) count: u32,
}

/// The same feature at the same place: its copy number does not count (a
/// mirror of a feature and of a mirror whose original it is reaches the
/// feature's place by two numberings).
impl PartialEq for Placed {
    fn eq(&self, other: &Self) -> bool {
        self.feature == other.feature && self.steps == other.steps
    }
}

impl Placed {
    /// The place's steps composed: where the original goes.
    fn at(&self) -> Transform {
        self.steps
            .iter()
            .fold(Transform::IDENTITY, |done, (t, _)| t.after(&done))
    }
}

/// Places at most that patterns of patterns give.
const MAX_PLACES: usize = 10_000;

/// The features with a tool a pattern's object stands for, at their places:
/// a feature with a tool where it is; a pattern or mirror of features its
/// originals at each of its elements (the original's too), recursively.
fn originals<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    uid: FeatureUid,
    depth: usize,
) -> Result<Vec<Placed>, String> {
    let entry = ctx.feature_entry(uid)?;
    if replay_tool(ctx, uid, None)?.is_some()
        || is_dressup(&entry.def)
        || changes_sheet(&entry.def)
        || makes_sheets(&entry.def)
    {
        return Ok(vec![Placed {
            feature: uid,
            steps: Vec::new(),
            index: 0,
            count: 1,
        }]);
    }
    let Some((objects, suppressed)) = repeated(&entry.def) else {
        return Err(format!("{} cannot be patterned or mirrored", entry.name));
    };
    // A pattern of bodies stands for its bodies as they were before it at
    // each of its elements (patterns of patterns of bodies, mitcad#547):
    // the place's feature is the body pattern itself.
    let (features, of_bodies): (&[FeatureUid], bool) = match objects {
        PatternObjects::Features { features } => (features, false),
        PatternObjects::Bodies { .. } => (&[], true),
        _ => return Err(format!("{} cannot be patterned or mirrored", entry.name)),
    };
    if depth > 32 {
        return Err(format!("{} nests patterns too deeply", entry.name));
    }
    let elements = match ctx.feature_elements(uid)? {
        // A mirror stored before it kept its elements: its reflection.
        elements if elements.is_empty() => match &entry.def {
            FeatureDef::Mirror(mirror) => {
                let frame = ctx.plane(&mirror.plane)?;
                let reflection = Transform::mirror(frame.origin, frame.normal())
                    .ok_or("the mirror plane has no normal")?;
                vec![Transform::IDENTITY, reflection]
            }
            _ => return Err(format!("{} has no elements", entry.name)),
        },
        elements => elements,
    };
    // A scaled pattern scales each copy about its own centre (mitcad#539):
    // the copies of an inner pattern's copies about theirs.
    let scaling = match scale_params(&entry.def) {
        Some(spec) if elements.len() > 1 => {
            Some(Scaling::stored(ctx, objects, &spec, &elements, depth + 1)?)
        }
        _ => None,
    };
    let count = elements.len() as u32;
    let mut out = Vec::new();
    for (index, transform) in elements.iter().enumerate() {
        let index = index as u32;
        if index > 0 && suppressed.contains(&index) {
            continue;
        }
        let mut inner_places = Vec::new();
        if of_bodies {
            inner_places.push(Placed {
                feature: uid,
                steps: Vec::new(),
                index: 0,
                count: 1,
            });
        }
        for inner in features {
            inner_places.extend(originals(ctx, *inner, depth + 1)?);
        }
        for mut placed in inner_places {
            if index > 0 {
                let step = match &scaling {
                    Some(s) => s.copy(index, &placed)?,
                    None => *transform,
                };
                placed.steps.push((
                    step,
                    Instance {
                        feature: uid,
                        index,
                    },
                ));
            }
            placed.index = placed
                .index
                .saturating_add(placed.count.saturating_mul(index));
            placed.count = placed.count.saturating_mul(count);
            out.push(placed);
            if out.len() > MAX_PLACES {
                return Err(format!(
                    "{} repeats its originals at more than {MAX_PLACES} places",
                    entry.name
                ));
            }
        }
    }
    Ok(out)
}

/// A tool moved through a place's steps (each copy named by its instance),
/// then by `transform` named by `instance`. `moved`: the tool is in its
/// place already (rebuilt there), only the names change.
pub(super) fn placed_copy<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    tool: &K::Shape,
    steps: &[(Transform, Instance)],
    transform: &Transform,
    instance: Instance,
    moved: bool,
) -> Result<K::Shape, String> {
    let identity = Transform::IDENTITY;
    let pick = |t: &Transform| if moved { identity } else { *t };
    let mut copy: Option<K::Shape> = None;
    for (t, inner) in steps {
        let from = copy.as_ref().unwrap_or(tool);
        copy = Some(
            ctx.kernel
                .transform_shape(from, &pick(t), Some(*inner))
                .map_err(|e| e.to_string())?,
        );
    }
    ctx.kernel
        .transform_shape(
            copy.as_ref().unwrap_or(tool),
            &pick(transform),
            Some(instance),
        )
        .map_err(|e| e.to_string())
}

/// The bodies a pattern of bodies repeats; None for other features (a
/// place's feature is such a pattern where a pattern of patterns of bodies
/// places them, mitcad#547).
fn body_leaf<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    uid: FeatureUid,
) -> Result<Option<Vec<BodyUid>>, String> {
    let entry = ctx.feature_entry(uid)?;
    Ok(match repeated(&entry.def) {
        Some((PatternObjects::Bodies { bodies }, _)) => Some(bodies.clone()),
        _ => None,
    })
}

/// A body as it was before the pattern of bodies that repeats it.
fn source_body<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    pattern: FeatureUid,
    body: BodyUid,
) -> Result<K::Shape, String> {
    ctx.body_before(pattern, body).ok_or_else(|| {
        format!(
            "body {body} does not exist before {}",
            ctx.feature_name(pattern)
        )
    })
}

/// A pattern of patterns of bodies (mitcad#547): at each place (a pattern
/// of bodies at the elements of the patterns between), its bodies as they
/// were before that pattern, moved through the place's steps and each of
/// the place's own elements (`own`, scaled and skipped as the pattern
/// says). With `combine` a copy that touches its body joins it (the body
/// as the patterns before made it), else it is a new body
/// `<pattern>.b<k>`, k counting the copies in order. With `combine_any`
/// a copy joins every body it touches of those the patterns' bodies are
/// now (the ones still there) and of the copies before it that stayed
/// bodies of their own ([`join_any`], mitcad#1006).
fn repeat_body_places<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    places: &[Placed],
    own: &[Vec<Element>],
    combine: Combine,
) -> Result<FeatureOutput<K::Shape>, String> {
    if combine == Combine::Any {
        return repeat_body_places_joining_any(ctx, places, own);
    }
    let combine = combine.joins();
    let mut changes = Vec::new();
    let mut merged: Vec<(BodyUid, K::Shape)> = Vec::new();
    let mut next = 0u32;
    for (place, elements) in places.iter().zip(own) {
        let bodies = body_leaf(ctx, place.feature)?.unwrap_or_default();
        for body in bodies {
            let source = source_body(ctx, place.feature, body)?;
            if combine {
                // Copies of a mesh body are not joined (mitcad#399).
                ctx.require_surfaces(body, &source)?;
            }
            for element in elements {
                let instance = Instance {
                    feature: ctx.uid,
                    index: element.index,
                };
                let copy = placed_copy(
                    ctx,
                    &source,
                    &place.steps,
                    &element.transform,
                    instance,
                    false,
                )?;
                if combine {
                    let target = match merged.iter().find(|(b, _)| *b == body) {
                        Some((_, shape)) => shape.clone(),
                        None => ctx.body(body)?,
                    };
                    let result = combined(ctx, &target, &copy)?;
                    // A copy that touches the body joins it; one that does
                    // not stays a body of its own.
                    let joined = result.pieces.into_iter().find(|p| !p.sources.is_empty());
                    if let Some(piece) = joined.filter(|_| result.touched[0]) {
                        match merged.iter_mut().find(|(b, _)| *b == body) {
                            Some(entry) => entry.1 = piece.shape,
                            None => merged.push((body, piece.shape)),
                        }
                        continue;
                    }
                }
                changes.push(BodyChange::Set(BodyUid::new(ctx.uid, next), copy));
                next += 1;
            }
        }
    }
    for (body, shape) in merged {
        changes.push(BodyChange::Set(body, shape));
    }
    Ok(FeatureOutput {
        changes,
        ..FeatureOutput::default()
    })
}

/// [`repeat_body_places`] with `combine_any` (mitcad#1006): the bodies of
/// the patterns that are still there are the bodies the copies join to
/// begin with; a copy that touches none is the new body
/// `<pattern>.b<k>`, k counting such bodies in order.
fn repeat_body_places_joining_any<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    places: &[Placed],
    own: &[Vec<Element>],
) -> Result<FeatureOutput<K::Shape>, String> {
    let mut listed: Vec<BodyUid> = Vec::new();
    for place in places {
        for body in body_leaf(ctx, place.feature)?.unwrap_or_default() {
            if !listed.contains(&body) {
                listed.push(body);
            }
        }
    }
    // A body an earlier pattern merged into another is no longer there.
    let mut start = Vec::new();
    for body in listed {
        if ctx.env.bodies.contains_key(&body) {
            let shape = ctx.body(body)?;
            // Copies of a mesh body are not joined (mitcad#399).
            ctx.require_surfaces(body, &shape)?;
            start.push((body, shape));
        }
    }
    let mut joining = Joining::new(ctx.kernel, start);
    let mut next = 0u32;
    for (place, elements) in places.iter().zip(own) {
        let bodies = body_leaf(ctx, place.feature)?.unwrap_or_default();
        for body in bodies {
            let source = source_body(ctx, place.feature, body)?;
            ctx.require_surfaces(body, &source)?;
            for element in elements {
                let instance = Instance {
                    feature: ctx.uid,
                    index: element.index,
                };
                let copy = placed_copy(
                    ctx,
                    &source,
                    &place.steps,
                    &element.transform,
                    instance,
                    false,
                )?;
                let pieces = match join_any(ctx, &mut joining, &copy)? {
                    Some(strays) => strays,
                    None => vec![copy],
                };
                for piece in pieces {
                    joining.add(ctx.kernel, BodyUid::new(ctx.uid, next), piece);
                    next += 1;
                }
            }
        }
    }
    Ok(FeatureOutput {
        changes: joining.changes(),
        ..FeatureOutput::default()
    })
}

/// The elements other than the original that are not suppressed.
pub(super) fn active(all: Vec<Transform>, suppressed: &[u32]) -> Result<Vec<Element>, String> {
    if all.len() > MAX_ELEMENTS as usize {
        return Err(format!(
            "the pattern has {} elements, at most {MAX_ELEMENTS} are allowed",
            all.len()
        ));
    }
    let suppressed: BTreeSet<u32> = suppressed.iter().copied().collect();
    Ok(all
        .into_iter()
        .enumerate()
        .skip(1)
        .map(|(i, transform)| Element {
            index: i as u32,
            transform,
        })
        .filter(|e| !suppressed.contains(&e.index))
        .collect())
}

/// How a pattern scales its copies (its `scale`, mitcad#59 and mitcad#539).
/// Every copy of the originals is scaled about its own centre: the first
/// original's centre of mass carried to the copy, through the elements of
/// the patterns among the features (patterns of patterns) as well. The
/// copies of the originals, numbered j = p + P · e (p the copy's number
/// among the P copies the inner patterns make, e the element), fall into
/// `groups` runs of equal length: run g = ⌊j · groups / (P · n)⌋ is scaled
/// by 1 + (factor − 1) · g / (groups − 1), as FreeCAD's Scaled
/// transformation scales slices of the transformations before it. With as
/// many groups as elements each element is a run.
///
/// Several scales (`scales`, mitcad#542) multiply. A scale's runs may be
/// counted over more copies than the pattern's own (`scale_copies` N, the
/// runs then ⌊j · groups / N⌋): the copies of a pattern inside a pattern
/// of patterns whose runs a later pattern's scale continues. A copy of an
/// inner copy is scaled relative to that copy, so that each copy ends up
/// at the factor of its own runs whatever the inner patterns scaled the
/// copy it is made of by (the factor of copy j over that of copy p; with
/// the pattern's own copies as N and no more runs than elements the inner
/// copies' runs are the first, factor 1). `scale_center`: the point the
/// copies are scaled about, carried to each copy.
#[derive(Debug, Clone)]
pub(crate) struct Scaling {
    rules: Vec<Rule>,
    /// The first original's centre of mass where it is (or the chosen
    /// centre).
    center: [f64; 3],
    /// The elements' motions without the scaling (element 0 the identity).
    rigid: Vec<Transform>,
}

/// One scale: its factor, runs and the copies they are counted over (None:
/// the pattern's own).
#[derive(Debug, Clone)]
struct Rule {
    factor: f64,
    groups: u32,
    copies: Option<u32>,
}

impl Rule {
    /// The factor of copy `j` of `total` copies.
    fn at(&self, j: u64, total: u64) -> f64 {
        if self.groups < 2 || total == 0 {
            return 1.0;
        }
        let run = j * u64::from(self.groups) / total;
        1.0 + (self.factor - 1.0) * run as f64 / f64::from(self.groups - 1)
    }

    fn total(&self, own: u64) -> u64 {
        self.copies.map_or(own, u64::from)
    }
}

impl Scaling {
    /// The scaling of a pattern whose elements are `rigid`: its factors
    /// and runs read, the centre found ([`first_center`]).
    pub(super) fn new<K: Kernel>(
        ctx: &mut EvalContext<'_, K>,
        objects: &PatternObjects,
        spec: &ScaleSpec<'_>,
        rigid: Vec<Transform>,
        depth: usize,
    ) -> Result<Self, String> {
        let n = rigid.len() as u32;
        let mut read = Vec::new();
        if let Some(scale) = spec.scale {
            read.push((scale, spec.groups, spec.copies));
        }
        for rule in spec.more {
            read.push((rule.scale, rule.groups, rule.copies));
        }
        let mut rules = Vec::with_capacity(read.len());
        for (factor, groups, copies) in read {
            let name = ctx.param_name(factor);
            let factor = ctx.param(factor)?;
            check_scale(&name, factor)?;
            let copies = match copies {
                Some(copies) => {
                    let name = ctx.param_name(copies);
                    Some(quantity(&name, ctx.param(copies)?)?)
                }
                None => None,
            };
            let groups = match groups {
                Some(groups) => {
                    let name = ctx.param_name(groups);
                    let m = quantity(&name, ctx.param(groups)?)?;
                    // Over the pattern's own copies, runs beyond its
                    // elements would split the inner patterns' copies.
                    let most = copies.unwrap_or(n);
                    if m < 2 || m > most {
                        return Err(format!(
                            "{name} must be from 2 to {most} (the pattern's {}), got {m}",
                            if copies.is_some() {
                                "scale copies"
                            } else {
                                "elements"
                            }
                        ));
                    }
                    m
                }
                None => n,
            };
            rules.push(Rule {
                factor,
                groups,
                copies,
            });
        }
        let center = match spec.center {
            Some(point) => ctx.point(point)?,
            None => first_center(ctx, objects, depth)?,
        };
        Ok(Self {
            rules,
            center,
            rigid,
        })
    }

    /// The scaling of an earlier pattern from the elements it stored
    /// (scaled for the original's own copies): its motions without the
    /// scaling found again, for the copies of the inner copies of a pattern
    /// of it ([`originals`]).
    fn stored<K: Kernel>(
        ctx: &mut EvalContext<'_, K>,
        objects: &PatternObjects,
        spec: &ScaleSpec<'_>,
        stored: &[Transform],
        depth: usize,
    ) -> Result<Self, String> {
        let mut scaling = Self::new(ctx, objects, spec, stored.to_vec(), depth)?;
        // A stored element scales about its rigid motion's image of the
        // centre, which the scaling keeps where it is.
        for (e, element) in stored.iter().enumerate() {
            let s = scaling.factor_at(e as u32, 0, 1);
            scaling.rigid[e] =
                Transform::scale(element.apply_point(scaling.center), [1.0 / s; 3]).after(element);
        }
        Ok(scaling)
    }

    /// The factor of the copy at element `index` of inner copy `inner` of
    /// `count`, relative to that inner copy.
    fn factor_at(&self, index: u32, inner: u32, count: u32) -> f64 {
        let count = u64::from(count.max(1));
        let own = count * self.rigid.len() as u64;
        let j = u64::from(inner) + count * u64::from(index);
        self.rules
            .iter()
            .map(|rule| {
                let total = rule.total(own);
                rule.at(j, total) / rule.at(u64::from(inner), total)
            })
            .product()
    }

    /// The runs must divide the copies they are counted over, which hold
    /// the pattern's own.
    fn check_count(&self, count: u32) -> Result<(), String> {
        let own = u64::from(count.max(1)) * self.rigid.len() as u64;
        for rule in &self.rules {
            let total = rule.total(own);
            if !total.is_multiple_of(u64::from(rule.groups)) {
                return Err(format!(
                    "the {} scale groups do not divide the {total} copies they are counted over",
                    rule.groups
                ));
            }
            if own > total {
                return Err(format!(
                    "the pattern makes {own} copies, more than the {total} its scale counts"
                ));
            }
        }
        Ok(())
    }

    /// The transform of element `index` for the copies at `place`: its
    /// motion, then the scaling about the copy's centre.
    fn copy(&self, index: u32, place: &Placed) -> Result<Transform, String> {
        self.check_count(place.count)?;
        let t = self.rigid[index as usize];
        let s = self.factor_at(index, place.index, place.count);
        let center = t.apply_point(place.at().apply_point(self.center));
        Ok(Transform::scale(center, [s; 3]).after(&t))
    }

    /// The elements as stored: scaled for the original's own copies.
    fn stored_elements(&self) -> Vec<Transform> {
        (0..self.rigid.len() as u32)
            .map(|e| {
                let t = self.rigid[e as usize];
                let s = self.factor_at(e, 0, 1);
                Transform::scale(t.apply_point(self.center), [s; 3]).after(&t)
            })
            .collect()
    }

    /// Each place's own elements where a place's copies are not the
    /// original's (patterns among the features); None when every place
    /// takes the elements as they are.
    fn per_place(
        &self,
        places: &[Placed],
        elements: &[Element],
    ) -> Result<Option<Vec<Vec<Element>>>, String> {
        for place in places {
            self.check_count(place.count)?;
        }
        if places.iter().all(|p| p.steps.is_empty() && p.index == 0) {
            return Ok(None);
        }
        let mut out = Vec::with_capacity(places.len());
        for place in places {
            let mut own = Vec::with_capacity(elements.len());
            for element in elements {
                own.push(Element {
                    index: element.index,
                    transform: self.copy(element.index, place)?,
                });
            }
            out.push(own);
        }
        Ok(Some(out))
    }
}

/// The centre of mass of a pattern's first object where it is: a body's,
/// or the tool of the first feature at its first place.
fn first_center<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    objects: &PatternObjects,
    depth: usize,
) -> Result<[f64; 3], String> {
    let shape = match objects {
        PatternObjects::Bodies { bodies } => ctx.body(bodies[0])?,
        PatternObjects::Features { features } => {
            let mut places = Vec::new();
            for uid in features {
                places.extend(originals(ctx, *uid, depth)?);
            }
            if places.iter().any(|p| {
                ctx.feature_entry(p.feature)
                    .is_ok_and(|e| is_dressup(&e.def))
            }) {
                return Err(format!(
                    "{UNSUPPORTED}scaled copies of fillets and chamfers"
                ));
            }
            let place = places
                .into_iter()
                .next()
                .ok_or("the pattern has no original to scale about")?;
            // A pattern of bodies' original: its first body before it.
            let tool = match body_leaf(ctx, place.feature)? {
                Some(bodies) => source_body(ctx, place.feature, bodies[0])?,
                None => ctx.feature_tool(place.feature)?,
            };
            ctx.kernel
                .transform_shape(&tool, &place.at(), None)
                .map_err(|e| e.to_string())?
        }
        PatternObjects::Faces { .. } => {
            return Err(format!("{UNSUPPORTED}scaled copies of faces"));
        }
        PatternObjects::Components { .. } => return Err("components cannot be scaled".to_owned()),
    };
    Ok(ctx
        .kernel
        .mass_properties(&shape)
        .map_err(|e| e.to_string())?
        .center)
}

/// A pattern's elements with its `scale` ([`Scaling`]): as stored (each
/// element scaled for the original's own copies), and the scaling the
/// copies of patterns among the features take theirs from. Elements of
/// bodies must fall into whole runs.
pub(super) fn scale_elements<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    objects: &PatternObjects,
    spec: &ScaleSpec<'_>,
    all: Vec<Transform>,
) -> Result<(Vec<Transform>, Option<Scaling>), String> {
    if !spec.scales() {
        if spec.groups.is_some() || spec.copies.is_some() || spec.center.is_some() {
            return Err("scale groups, copies and centre need a scale".to_owned());
        }
        return Ok((all, None));
    }
    if all.len() < 2 {
        for factor in spec.scale.iter().chain(spec.more.iter().map(|r| &r.scale)) {
            let name = ctx.param_name(*factor);
            check_scale(&name, ctx.param(*factor)?)?;
        }
        return Ok((all, None));
    }
    let scaling = Scaling::new(ctx, objects, spec, all, 0)?;
    // Copies of features are checked at their places ([`Scaling::per_place`]).
    if matches!(objects, PatternObjects::Bodies { .. }) {
        scaling.check_count(1)?;
    }
    Ok((scaling.stored_elements(), Some(scaling)))
}

/// A scale factor must be finite and positive.
fn check_scale(what: &str, value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("{what} must be a positive factor, got {value}"))
    }
}

/// A pattern's active elements and its scaling.
type Elements = (Vec<Element>, Option<Scaling>);

impl RectangularPatternDef {
    fn elements<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Elements, String> {
        let mut directions = Vec::new();
        for (name, direction) in [
            ("direction1", Some(&self.direction1)),
            ("direction2", self.direction2.as_ref()),
        ] {
            let Some(direction) = direction else {
                directions.push(([0.0; 3], vec![0.0]));
                continue;
            };
            let q = quantity(
                &ctx.param_name(direction.quantity),
                ctx.param(direction.quantity)?,
            )?;
            let distance = ctx.param(direction.distance)?;
            if !distance.is_finite() || (q > 1 && distance == 0.0) {
                return Err(format!(
                    "{} must not be zero",
                    ctx.param_name(direction.distance)
                ));
            }
            let axis = ctx
                .axis(&direction.axis)
                .map_err(|e| format!("{name}: {e}"))?;
            let offset = scaled(axis.direction, step(self.distance_type, distance, q));
            directions.push((offset, positions(q, direction.symmetric)));
        }
        let (offset1, along1) = &directions[0];
        let (offset2, along2) = &directions[1];
        let count = along1.len() * along2.len();
        if count > MAX_ELEMENTS as usize {
            return Err(format!(
                "the pattern has {count} elements, at most {MAX_ELEMENTS} are allowed"
            ));
        }
        let mut all = Vec::with_capacity(count);
        for j in along2 {
            for i in along1 {
                all.push(Transform::translation(add(
                    scaled(*offset1, *i),
                    scaled(*offset2, *j),
                )));
            }
        }
        let (all, scaling) = scale_elements(ctx, &self.objects, &self.scale_spec(), all)?;
        ctx.elements.clone_from(&all);
        Ok((active(all, &self.suppressed_elements)?, scaling))
    }
}

impl CircularPatternDef {
    fn elements<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Elements, String> {
        let q = quantity(&ctx.param_name(self.quantity), ctx.param(self.quantity)?)?;
        let angle = ctx.param(self.angle)?;
        if !angle.is_finite() || (q > 1 && angle == 0.0) {
            return Err(format!("{} must not be zero", ctx.param_name(self.angle)));
        }
        let full = (angle.abs() - std::f64::consts::TAU).abs() <= 1e-9;
        let step = if full {
            angle / f64::from(q)
        } else if q > 1 {
            angle / f64::from(q - 1)
        } else {
            0.0
        };
        let axis = ctx.axis(&self.axis)?;
        let all: Vec<Transform> = positions(q, self.symmetric && !full)
            .into_iter()
            .map(|i| {
                Transform::rotation(axis.origin, axis.direction, i * step)
                    .expect("a unit axis direction")
            })
            .collect();
        let (all, scaling) = scale_elements(ctx, &self.objects, &self.scale_spec(), all)?;
        ctx.elements.clone_from(&all);
        Ok((active(all, &self.suppressed_elements)?, scaling))
    }
}

/// The transform taking the element at arc length `from` of a path to the
/// one at `to`: before the start and past the end the path goes on
/// straight or around the circle.
fn path_transform(curve: &PathCurve, from: f64, to: f64, along_path: bool) -> Transform {
    match curve {
        PathCurve::Arc {
            center,
            x,
            y,
            radius,
            ..
        } if along_path => Transform::rotation(*center, cross(*x, *y), (to - from) / radius)
            .expect("a unit normal"),
        _ => Transform::translation(sub(curve.point(to), curve.point(from))),
    }
}

/// The transform taking the element at arc length `from` of a chain of
/// curves to the one at `to` (mitcad#464): moved along, and with
/// `along_path` turned about the sketch's normal as the path's tangent
/// turns.
fn chain_transform(chain: &PathChain, from: f64, to: f64, along_path: bool) -> Transform {
    let (p0, t0) = chain.at(from);
    let (p1, t1) = chain.at(to);
    let shift = Transform::translation(sub(p1, p0));
    if !along_path {
        return shift;
    }
    let angle = dot(cross(t0, t1), chain.normal).atan2(dot(t0, t1));
    shift.after(&Transform::rotation(p0, chain.normal, angle).expect("a unit normal"))
}

impl PathPatternDef {
    fn elements<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Elements, String> {
        let q = quantity(&ctx.param_name(self.quantity), ctx.param(self.quantity)?)?;
        let distance = ctx.param(self.distance)?;
        if !distance.is_finite() || (q > 1 && distance == 0.0) {
            return Err(format!(
                "{} must not be zero",
                ctx.param_name(self.distance)
            ));
        }
        if matches!(self.path, PathRef::Edges { .. }) {
            return Err(format!("{UNSUPPORTED}path patterns along edges"));
        }
        let sign = if self.flip { -1.0 } else { 1.0 };
        let step = sign * step(self.distance_type, distance, q);
        let all: Vec<Transform> = if let PathRef::SketchCurves { sketch, curves } = &self.path {
            // Along a chain of lines and arcs (mitcad#464).
            let chain = PathChain::of_sketch(ctx, *sketch, curves)?;
            let s0 = match self.start_point {
                Some(p) => chain.nearest(p),
                None => self.start * chain.length(),
            };
            positions(q, self.symmetric)
                .into_iter()
                .map(|i| chain_transform(&chain, s0, s0 + i * step, self.along_path))
                .collect()
        } else {
            let curve = path_curve(ctx, &self.path)?;
            let s0 = match self.start_point {
                Some(p) => curve.nearest(p),
                None => self.start * curve.length(),
            };
            positions(q, self.symmetric)
                .into_iter()
                .map(|i| path_transform(&curve, s0, s0 + i * step, self.along_path))
                .collect()
        };
        let (all, scaling) = scale_elements(ctx, &self.objects, &self.scale_spec(), all)?;
        ctx.elements.clone_from(&all);
        Ok((active(all, &self.suppressed_elements)?, scaling))
    }
}

/// How a pattern's or mirror's copies of bodies join bodies (`combine`,
/// `combine_any`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Combine {
    /// Each copy is a new body.
    No,
    /// A copy joins the body it is a copy of where they touch (`combine`).
    Own,
    /// A copy joins every body of the pattern it touches (`combine_any`,
    /// mitcad#1006): the patterned bodies and the copies before it that
    /// stayed bodies of their own ([`join_any`]), as FreeCAD fuses the
    /// copies of a Body's whole shape with whatever they reach.
    Any,
}

impl Combine {
    /// From a definition's `combine` and `combine_any`.
    pub(crate) fn of(combine: bool, any: bool) -> Self {
        match (combine, any) {
            (false, _) => Self::No,
            (true, false) => Self::Own,
            (true, true) => Self::Any,
        }
    }

    /// Whether copies join bodies at all.
    fn joins(self) -> bool {
        self != Self::No
    }
}

/// Places copies of the objects at the elements: the evaluation shared by
/// patterns and mirrors. `combine` joins copies of bodies to their
/// originals when they touch (a mirror's `combine`), or to every body of
/// the pattern they touch ([`Combine::Any`]). `scaling`: the
/// pattern's scale, for the copies of the copies patterns among the
/// features make ([`Scaling`]; the elements are scaled for the originals'
/// own copies). `skipped`: the copies of features not made, by their
/// numbers (`skipped_copies`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn repeat<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    objects: &PatternObjects,
    compute: ComputeOption,
    original_bodies: bool,
    elements: &[Element],
    combine: Combine,
    scaling: Option<&Scaling>,
    skipped: &[u32],
) -> Result<FeatureOutput<K::Shape>, String> {
    let instance = |uid: FeatureUid, element: &Element| Instance {
        feature: uid,
        index: element.index,
    };
    match objects {
        PatternObjects::Components { .. } => Err("components require Pattern at Points".to_owned()),
        PatternObjects::Bodies { bodies } if combine == Combine::Any => {
            repeat_bodies_joining_any(ctx, bodies, elements, skipped)
        }
        PatternObjects::Bodies { bodies } => {
            let combine = combine.joins();
            let mut changes = Vec::new();
            let count = bodies.len() as u32;
            for (j, body) in bodies.iter().enumerate() {
                let shape = ctx.body(*body)?;
                if combine {
                    // Copies of a mesh body are not joined (mitcad#399).
                    ctx.require_surfaces(*body, &shape)?;
                }
                let mut merged = None;
                // Skipped copies (mitcad#547): their numbers are the
                // elements'.
                for element in elements.iter().filter(|e| !skipped.contains(&e.index)) {
                    let copy = ctx
                        .kernel
                        .transform_shape(
                            &shape,
                            &element.transform,
                            Some(instance(ctx.uid, element)),
                        )
                        .map_err(|e| e.to_string())?;
                    let uid = BodyUid::new(ctx.uid, (element.index - 1) * count + j as u32);
                    if combine {
                        let target = merged.as_ref().unwrap_or(&shape);
                        let result = combined(ctx, target, &copy)?;
                        // A copy that touches the original joins it; one
                        // that does not stays a body of its own.
                        let joined = result.pieces.into_iter().find(|p| !p.sources.is_empty());
                        if let Some(piece) = joined.filter(|_| result.touched[0]) {
                            merged = Some(piece.shape);
                            continue;
                        }
                    }
                    changes.push(BodyChange::Set(uid, copy));
                }
                if let Some(merged) = merged {
                    changes.push(BodyChange::Set(*body, merged));
                }
            }
            Ok(FeatureOutput {
                changes,
                ..FeatureOutput::default()
            })
        }
        PatternObjects::Features { features } => {
            let mut set = BodySet::new(ctx.bodies());
            let mut places: Vec<Placed> = Vec::new();
            for uid in features {
                for place in originals(ctx, *uid, 0)? {
                    // A feature at the same place twice (a mirror of a
                    // feature and of a pattern whose original it is) is
                    // copied once: the second copy would reach nothing.
                    if !places.contains(&place) {
                        places.push(place);
                    }
                }
            }
            // Sheet features are repeated by the pattern's elements: not
            // the copies of an inner pattern's copies scaled about their
            // own centres (mitcad#539).
            if scaling.is_some()
                && places.iter().any(|p| !p.steps.is_empty())
                && places.iter().any(|p| {
                    ctx.feature_entry(p.feature)
                        .is_ok_and(|e| changes_sheet(&e.def) || makes_sheets(&e.def))
                })
            {
                return Err(format!(
                    "{UNSUPPORTED}scaled copies of patterns of sheet features"
                ));
            }
            // Copies of faces that made new sheet bodies are new sheet
            // bodies (mitcad#280).
            if places.iter().all(|p| {
                ctx.feature_entry(p.feature)
                    .is_ok_and(|e| makes_sheets(&e.def))
            }) {
                let mut copies = Vec::new();
                for place in &places {
                    let at = place
                        .steps
                        .iter()
                        .fold(Transform::IDENTITY, |done, (t, _)| t.after(&done));
                    for element in elements {
                        copies.push(crate::sheet::replay::Copy {
                            feature: place.feature,
                            transform: element.transform.after(&at),
                            instance: Instance {
                                feature: ctx.uid,
                                index: element.index,
                            },
                        });
                    }
                }
                if let Some(output) = crate::sheet::replay::copy_faces(ctx, &copies)? {
                    return Ok(output);
                }
            }
            // Copies of sheet features on one sheet body are repeated on the
            // sheet, in timeline order (mitcad#242).
            if places.iter().any(|p| {
                ctx.feature_entry(p.feature)
                    .is_ok_and(|e| changes_sheet(&e.def))
                    || crate::sheet::replay::kept_as_change(ctx, p.feature, None)
            }) {
                let timeline = &ctx.env.features;
                let position = |uid: FeatureUid| timeline.iter().position(|f| f.uid == uid);
                let mut ordered: Vec<&Placed> = places.iter().collect();
                ordered.sort_by_key(|p| position(p.feature));
                let mut copies = Vec::new();
                for place in ordered {
                    let at = place
                        .steps
                        .iter()
                        .fold(Transform::IDENTITY, |done, (t, _)| t.after(&done));
                    for element in elements {
                        copies.push(crate::sheet::replay::Copy {
                            feature: place.feature,
                            transform: element.transform.after(&at),
                            instance: Instance {
                                feature: ctx.uid,
                                index: element.index,
                            },
                        });
                    }
                }
                if let Some(output) = crate::sheet::replay::repeat(ctx, &copies)? {
                    return Ok(output);
                }
            }
            // Fillets and chamfers (mitcad#105) go on the copies' edges,
            // after the copies of the features before them.
            let mut dressups = BTreeSet::new();
            for place in &places {
                if is_dressup(&ctx.feature_entry(place.feature)?.def) {
                    dressups.insert(place.feature);
                }
            }
            let mut copied = BTreeSet::new();
            if !dressups.is_empty() {
                let mut nested = |uid: FeatureUid| match ctx.feature_entry(uid) {
                    Ok(entry) => match repeated(&entry.def) {
                        Some((PatternObjects::Features { features }, _)) => features.clone(),
                        _ => Vec::new(),
                    },
                    Err(_) => Vec::new(),
                };
                copied = copied_features(features, &mut nested);
                let timeline = &ctx.env.features;
                let position = |uid: FeatureUid| timeline.iter().position(|f| f.uid == uid);
                places.sort_by_key(|p| position(p.feature));
            }
            // A scaled pattern's copies of an inner pattern's copies scale
            // about their own centres (mitcad#539).
            let mut per_place = match scaling {
                Some(scaling) => scaling.per_place(&places, elements)?,
                None => None,
            };
            // Skipped copies (mitcad#542): by their numbers p + P · e.
            if !skipped.is_empty() {
                let mut own =
                    per_place.unwrap_or_else(|| places.iter().map(|_| elements.to_vec()).collect());
                for (list, place) in own.iter_mut().zip(&places) {
                    list.retain(|e| {
                        let copy = place
                            .index
                            .saturating_add(place.count.saturating_mul(e.index));
                        !skipped.contains(&copy)
                    });
                }
                per_place = Some(own);
            }
            // Patterns of bodies among the features (mitcad#547): their
            // bodies copied at every place.
            let mut leaves = 0;
            for place in &places {
                if body_leaf(ctx, place.feature)?.is_some() {
                    leaves += 1;
                }
            }
            if leaves > 0 {
                if leaves < places.len() {
                    return Err(
                        "patterns of bodies cannot be patterned together with features".to_owned(),
                    );
                }
                let own =
                    per_place.unwrap_or_else(|| places.iter().map(|_| elements.to_vec()).collect());
                return repeat_body_places(ctx, &places, &own, combine);
            }
            let copying = Copying {
                places: &places,
                compute,
                original_bodies,
                copied: &copied,
                dressups: &dressups,
                per_place: per_place.as_deref(),
            };
            // A requested point is an explicit placement, never an optional
            // lattice element: report which target could not replay the group.
            if matches!(
                ctx.feature_entry(ctx.uid)?.def,
                FeatureDef::PatternAtPoints(_)
            ) {
                for element in elements {
                    for place in &places {
                        if !copying.place(ctx, &mut set, place, std::slice::from_ref(element))? {
                            return Err(format!(
                                "target {}: {}",
                                element.index,
                                copying.unreached(ctx, place)
                            ));
                        }
                    }
                }
                return Ok(FeatureOutput {
                    changes: set.changes(),
                    ..FeatureOutput::default()
                });
            }
            // Optimized copies the faces of fillets and chamfers (and the
            // features they round) instead of rounding at each copy. A
            // failure there is the pattern's: the roundings are not worked
            // out copy by copy as the other options do.
            if compute == ComputeOption::Optimized
                && !dressups.is_empty()
                && super::face_copy::copy_faces(ctx, &mut set, &places, elements)?
            {
                return Ok(FeatureOutput {
                    changes: set.changes(),
                    ..FeatureOutput::default()
                });
            }
            let saved = (set.clone(), ctx.next_body, ctx.warnings.len());
            match copying.by_places(ctx, &mut set, elements) {
                Ok(()) => {}
                // A fillet's copies that fail on the copies of all the
                // features: the features of one element after another (the
                // copies of the next element's features then cut into or join
                // the rounded ones, as they would be built one by one).
                Err(_) if !dressups.is_empty() && elements.len() > 1 => {
                    (set, ctx.next_body) = (saved.0, saved.1);
                    ctx.warnings.truncate(saved.2);
                    copying.by_elements(ctx, &mut set, elements)?;
                }
                Err(e) => return Err(e),
            }
            if combine.joins() {
                join_copies(ctx, &mut set, &places)?;
            }
            Ok(FeatureOutput {
                changes: set.changes(),
                ..FeatureOutput::default()
            })
        }
        PatternObjects::Faces { body, faces } => {
            let shape = ctx.surface_body(*body)?;
            // A face a sketch change renamed is found again (mitcad#272).
            let mut found = Vec::with_capacity(faces.len());
            for face in faces {
                found.push(super::renamed::face_for(ctx, &shape, face)?);
            }
            let (tool, op) = ctx
                .kernel
                .face_tool(&shape, &found)
                .map_err(|e| e.to_string())?;
            let operation = if op == BooleanOp::Cut {
                Operation::Cut
            } else {
                Operation::Join
            };
            let mut set = BodySet::new(ctx.bodies());
            let mut reached = false;
            if compute == ComputeOption::Adjust {
                let mut copies = Vec::with_capacity(elements.len());
                for element in elements {
                    let copy = ctx
                        .kernel
                        .transform_shape(
                            &tool,
                            &element.transform,
                            Some(instance(ctx.uid, element)),
                        )
                        .map_err(|e| e.to_string())?;
                    copies.push(copy);
                }
                let copies = reaching(ctx, &set, &[*body], copies);
                match all_at_once(ctx, &mut set, operation, &[*body], &copies) {
                    Some(r) => reached = r,
                    None => {
                        for copy in &copies {
                            reached |= apply_tool(ctx, &mut set, operation, &[*body], copy)?;
                        }
                    }
                }
            } else if !elements.is_empty() {
                let reach =
                    (operation == Operation::Cut).then_some((&set, std::slice::from_ref(body)));
                if let Some(tool) = moved_copies(ctx, &tool, &[], elements, reach)? {
                    reached = apply_tool(ctx, &mut set, operation, &[*body], &tool)?;
                }
            }
            if !reached && !elements.is_empty() {
                return Err("no copy of the faces reaches the body".to_owned());
            }
            Ok(FeatureOutput {
                changes: set.changes(),
                ..FeatureOutput::default()
            })
        }
    }
}

/// A pattern's copies of features at their places, in the order to apply
/// them.
struct Copying<'a> {
    places: &'a [Placed],
    compute: ComputeOption,
    original_bodies: bool,
    /// The features whose faces the pattern copies, and the fillets and
    /// chamfers among them (mitcad#105, [`super::dressup_copies`]).
    copied: &'a BTreeSet<FeatureUid>,
    dressups: &'a BTreeSet<FeatureUid>,
    /// Each place's own elements (a scaled pattern of patterns,
    /// [`Scaling::per_place`]); None: every place at the pattern's.
    per_place: Option<&'a [Vec<Element>]>,
}

impl Copying<'_> {
    /// The elements of the place at `i`.
    fn elements_of<'e>(&'e self, i: usize, elements: &'e [Element]) -> &'e [Element] {
        match self.per_place {
            Some(all) => &all[i],
            None => elements,
        }
    }

    /// Each place at all elements, one place after another.
    fn by_places<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &mut BodySet<K::Shape>,
        elements: &[Element],
    ) -> Result<(), String> {
        let mut i = 0;
        // Runs of places in one operation, until one is not kept
        // (Copying::together).
        let mut runs = runs_enabled();
        while i < self.places.len() {
            let run = self.cut_run(ctx, set, i)?;
            if run > 1 {
                self.cut_together(ctx, set, &self.places[i..i + run], elements)?;
                i += run;
                continue;
            }
            if runs && let Together::Applied(n) = self.together(ctx, set, i, elements, &mut runs)? {
                i += n;
                continue;
            }
            let place = &self.places[i];
            let own = self.elements_of(i, elements);
            if !self.place(ctx, set, place, own)? && !own.is_empty() {
                return Err(self.unreached(ctx, place));
            }
            i += 1;
        }
        Ok(())
    }

    /// A run of places from `from` on whose copies all join, or all cut,
    /// the same one body, applied in one operation (mitcad#855: a pattern
    /// of a dozen features joined to a body of 3 000 faces took a join per
    /// feature, 2 s each, though every join changed only a few faces).
    ///
    /// The run gives the body, its face names included, that the places
    /// give one after another, because the places act on parts of the body
    /// apart from each other: the boxes of the copies of different places
    /// stay more than [`REACH`] apart (each place's copies made as
    /// [`Copying::place`] makes them, all of them, before [`reaching`]
    /// leaves any out), so no copy meets another place's copies, nor the
    /// faces they made or removed, and what one place does is the same
    /// before and after another's. Each place's copies are united as
    /// [`Copying::place`] unites them, and the places' tools go to the
    /// operation as one compound. Then the run is kept only when
    /// - the operation made no new body (no body split apart, no copy of
    ///   a join left on its own);
    /// - some face of each place's copies is in the result: each place
    ///   reaches the body as it does alone (copies that touch nothing are
    ///   left out alike, apart from all others);
    /// - a face of the body that became several (whose pieces are numbered
    ///   `#k` in their geometric order, which holes in a piece change) has
    ///   the copies of one place only near its box: that place splits it
    ///   as it does alone, and no other place changes the pieces.
    ///
    /// The last is foreseen from the boxes before the operation, so that a
    /// run that would not be kept is not tried ([`SharedFaces`]): a run
    /// ends before a place whose copies come near a face of the body that
    /// another place's copies come near too, unless every such copy stands
    /// inside the face (a flat face whose box holds the copy's across it:
    /// bosses or holes on one face, which split it only in rare shapes,
    /// such as a ring). When a run is not kept even so, the bodies are as
    /// before, its places go one by one and so does the rest of the
    /// pattern (`runs`), so a pattern wastes at most one operation.
    /// With `MITCAD_BOOLEAN_TRACE` set, each run, and why it was not taken
    /// or kept, is a line on standard error. Off with the environment
    /// variable `MITCAD_NO_PATTERN_RUNS=1`, for comparisons.
    fn together<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &mut BodySet<K::Shape>,
        from: usize,
        elements: &[Element],
        runs: &mut bool,
    ) -> Result<Together, String> {
        if from + 1 >= self.places.len() {
            return Ok(Together::No);
        }
        // The copies are made only when the next place acts on the same
        // body in the same way.
        let first = self.run_target(ctx, set, from, elements);
        let next = self.run_target(ctx, set, from + 1, elements);
        let why_not = match (&first, &next) {
            (Ok(a), Ok(b)) if a.0 == b.0 && a.1 == b.1 => None,
            (Ok(a), Ok(b)) => Some(format!(
                "{} {} {} and {} {} {}",
                self.place_name(from),
                verb_of(a.0),
                a.1,
                self.place_name(from + 1),
                verb_of(b.0),
                b.1
            )),
            (Err(e), _) => Some(format!("{}: {e}", self.place_name(from))),
            (_, Err(e)) => Some(format!("{}: {e}", self.place_name(from + 1))),
        };
        if let Some(why) = why_not {
            trace_run(ctx, || {
                format!("no run at {}: {why}", self.place_name(from))
            });
            return Ok(Together::No);
        }
        let warnings = ctx.warnings.len();
        let mut run: Vec<RunPlace<K::Shape>> = Vec::new();
        let mut shared: Option<SharedFaces> = None;
        for i in from..self.places.len() {
            let before = ctx.warnings.len();
            let ends = match self.run_place(ctx, set, i, elements) {
                Err(e) => Some(e),
                Ok(member) => {
                    let why = if run
                        .first()
                        .is_some_and(|f| f.operation != member.operation || f.body != member.body)
                    {
                        Some("another body or operation".to_owned())
                    } else if let Some(other) = run.iter().find(|o| !apart(&o.boxes, &member.boxes))
                    {
                        Some(format!(
                            "its copies come near those of {}",
                            self.place_name(other.index)
                        ))
                    } else {
                        // The body's faces near the copies, once a second
                        // place is to join the run.
                        if shared.is_none() && run.len() == 1 {
                            shared = Some(SharedFaces::of(ctx, set, &run[0]));
                        }
                        match shared.as_mut().map(|s| s.add(&member)) {
                            Some(Err((face, other))) => Some(format!(
                                "face {face} of {}, near its copies and those of {}, may split",
                                member.body,
                                self.place_name(other)
                            )),
                            _ => None,
                        }
                    };
                    if why.is_none() {
                        run.push(member);
                    }
                    why
                }
            };
            if let Some(why) = ends {
                ctx.warnings.truncate(before);
                trace_run(ctx, || {
                    format!(
                        "run from {} ends before {}: {why}",
                        self.place_name(from),
                        self.place_name(i)
                    )
                });
                break;
            }
        }
        if run.len() < 2 {
            ctx.warnings.truncate(warnings);
            return Ok(Together::No);
        }
        let names = || {
            run.iter()
                .map(|m| self.place_name(m.index))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let saved = (set.clone(), ctx.next_body, ctx.pieces.clone());
        let declined = match self.apply_run(ctx, set, &run) {
            Ok(Ok(())) => {
                // As many passes over the body as the places one by one make
                // (each cuts it once), so a later split numbers its pieces
                // as before (mitcad#408).
                if run[0].operation == Operation::Cut {
                    ctx.pieces.skip_passes(run[0].body, run.len() as u32 - 1);
                }
                trace_run(ctx, || format!("run kept: {}", names()));
                return Ok(Together::Applied(run.len()));
            }
            Ok(Err(declined)) => declined,
            Err(e) => format!("the operation failed: {e}"),
        };
        (*set, ctx.next_body, ctx.pieces) = saved;
        ctx.warnings.truncate(warnings);
        trace_run(ctx, || {
            format!(
                "run not kept: {}: {declined}; the pattern goes on one place by one",
                names()
            )
        });
        *runs = false;
        Ok(Together::No)
    }

    /// The place at `i` for traces: its feature, and the place's number
    /// among the pattern's.
    fn place_name(&self, i: usize) -> String {
        format!("{}/{i}", self.places[i].feature)
    }

    /// What the place at `i` does as a member of a run
    /// ([`Copying::together`]): a feature that joins or cuts one body (not
    /// a fillet, chamfer or thread, not a sheet's or a scoped cut), with
    /// the operation, the body and the participants [`Copying::place`]
    /// gives it. Otherwise why it cannot be one (an error is left to
    /// [`Copying::place`] to report).
    fn run_target<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &BodySet<K::Shape>,
        i: usize,
        elements: &[Element],
    ) -> Result<(Operation, BodyUid, Vec<BodyUid>), String> {
        let uid = self.places[i].feature;
        if self.elements_of(i, elements).is_empty() {
            return Err("all its copies skipped".to_owned());
        }
        if self.dressups.contains(&uid) {
            return Err("a fillet, chamfer or thread".to_owned());
        }
        let entry = ctx.feature_entry(uid)?;
        if changes_sheet(&entry.def) {
            return Err("a sheet feature".to_owned());
        }
        if matches!(&entry.def, FeatureDef::Extrude(e) if e.active_cut_scope().is_some()) {
            return Err("a scoped cut".to_owned());
        }
        let Some(tool_use) = replay_tool(ctx, uid, Some(set))? else {
            return Err("no tool".to_owned());
        };
        let operation = tool_use.operation;
        if !matches!(operation, Operation::Cut | Operation::Join) {
            return Err("neither a join nor a cut".to_owned());
        }
        let participants = if self.original_bodies && tool_use.participants.is_empty() {
            changed_bodies(ctx, uid, set)?
        } else {
            present_participants(ctx, uid, operation, &tool_use.participants, set)?
        };
        // The one body the operation works on (all bodies: the only one
        // that is not a mesh).
        let body = match participants[..] {
            [body] => body,
            [] => {
                let mut solids = set
                    .uids()
                    .into_iter()
                    .filter(|b| set.get(*b).is_some_and(|s| !ctx.is_mesh(s)));
                match (solids.next(), solids.next()) {
                    (Some(body), None) => body,
                    _ => return Err("all bodies, several of them".to_owned()),
                }
            }
            _ => return Err(format!("{} bodies", participants.len())),
        };
        Ok((operation, body, participants))
    }

    /// The place at `i` as a member of a run ([`Copying::together`]):
    /// [`Copying::run_target`], with its copies as [`Copying::place`] makes
    /// them and their boxes. Otherwise why it cannot be one.
    fn run_place<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &BodySet<K::Shape>,
        i: usize,
        elements: &[Element],
    ) -> Result<RunPlace<K::Shape>, String> {
        let (operation, body, participants) = self.run_target(ctx, set, i, elements)?;
        let place = &self.places[i];
        let uid = place.feature;
        let own = self.elements_of(i, elements);
        let entry = ctx.feature_entry(uid)?;
        let stored = ctx.feature_tool(uid)?;
        let all = if self.compute == ComputeOption::Adjust {
            adjust_copies(ctx, &entry, place, &stored, own)?
        } else {
            moved_list(ctx, &stored, &place.steps, own)?
        };
        let mut boxes = Vec::with_capacity(all.len());
        for copy in &all {
            match ctx.kernel.enclosing_box(copy) {
                Ok(Some(b)) => boxes.push(b),
                _ => return Err("a copy without a box".to_owned()),
            }
        }
        // The copies the operation gets, as Copying::place leaves them.
        let copies = if self.compute == ComputeOption::Adjust || operation == Operation::Cut {
            reaching(ctx, set, &participants, all)
        } else {
            all
        };
        if copies.is_empty() {
            return Err("no copy near the body".to_owned());
        }
        // As Copying::place unites them (all_at_once, moved_copies).
        let tool = if copies.len() == 1 {
            copies[0].clone()
        } else {
            let refs: Vec<&K::Shape> = copies.iter().collect();
            ctx.kernel
                .unite(&refs)
                .map_err(|e| format!("its copies' union failed: {e}"))?
        };
        Ok(RunPlace {
            index: i,
            operation,
            body,
            copies,
            tool,
            boxes,
        })
    }

    /// The run's places applied in one operation, kept when it gives the
    /// bodies they give one by one (the conditions of
    /// [`Copying::together`]); otherwise why not (the bodies then changed).
    fn apply_run<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &mut BodySet<K::Shape>,
        run: &[RunPlace<K::Shape>],
    ) -> Result<Result<(), String>, String> {
        let (operation, body) = (run[0].operation, run[0].body);
        let Some(target) = set.get(body).cloned() else {
            return Ok(Err(format!("no body {body}")));
        };
        let tools: Vec<K::Shape> = run.iter().map(|m| m.tool.clone()).collect();
        let tool = match ctx.kernel.compound(&tools) {
            Ok(tool) => tool,
            Err(KernelError::Unsupported(_)) => {
                let refs: Vec<&K::Shape> = tools.iter().collect();
                ctx.kernel.unite(&refs).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };
        // Adjust leaves out a join's copies that touch nothing; Identical
        // makes bodies of them (and the run is not kept).
        let adjust_join = self.compute == ComputeOption::Adjust && operation == Operation::Join;
        let next_body = ctx.next_body;
        if !apply_tool_strays(ctx, set, operation, &[body], &tool, !adjust_join)? {
            return Ok(Err("it reached no body".to_owned()));
        }
        if ctx.next_body != next_body {
            return Ok(Err("it made a new body".to_owned()));
        }
        let Some(result) = set.get(body).cloned() else {
            return Ok(Err(format!("it took away {body}")));
        };
        for member in run {
            if !member.copies.iter().any(|c| has_faces_of(ctx, &result, c)) {
                return Ok(Err(format!(
                    "no face of the copies of {} in the result",
                    self.place_name(member.index)
                )));
            }
        }
        Ok(splits_of_one_place(ctx, &target, &result, run, |i| {
            self.place_name(run[i].index)
        }))
    }

    /// How many places from `from` on cut the same bodies with their copies
    /// all at once (Identical and Optimized; no fillet or chamfer among
    /// them): those are cut together ([`Copying::cut_together`]). 1 (or 0)
    /// when the place at `from` is not such a cut.
    fn cut_run<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &BodySet<K::Shape>,
        from: usize,
    ) -> Result<usize, String> {
        // Places with elements of their own are cut one by one.
        if self.compute == ComputeOption::Adjust || self.per_place.is_some() {
            return Ok(1);
        }
        let mut first = None;
        let mut run = 0;
        for place in &self.places[from..] {
            if self.dressups.contains(&place.feature) {
                break;
            }
            let Some(participants) = self.cut_participants(ctx, set, place)? else {
                break;
            };
            match &first {
                None => first = Some(participants),
                Some(p) if *p == participants => {}
                Some(_) => break,
            }
            run += 1;
        }
        Ok(run.max(1))
    }

    /// The bodies the copies of the feature at `place` cut; None when it
    /// does not cut.
    fn cut_participants<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &BodySet<K::Shape>,
        place: &Placed,
    ) -> Result<Option<Vec<BodyUid>>, String> {
        let Some(tool_use) = replay_tool(ctx, place.feature, Some(set))? else {
            return Ok(None);
        };
        if tool_use.operation != Operation::Cut {
            return Ok(None);
        }
        let participants = if self.original_bodies && tool_use.participants.is_empty() {
            changed_bodies(ctx, place.feature, set)?
        } else {
            present_participants(
                ctx,
                place.feature,
                tool_use.operation,
                &tool_use.participants,
                set,
            )?
        };
        Ok(Some(participants))
    }

    /// The places of a run of cuts ([`Copying::cut_run`]) in one operation:
    /// the copies of all of them united and cut out of their bodies at once,
    /// which gives the bodies the cuts one after another give (mitcad#181: a
    /// pattern of a hole and of its circular pattern of 21 was 44 cuts of 28
    /// copies each from a body of 4 000 faces, more than a minute). A place
    /// none of whose copies reaches a body fails the pattern as it does
    /// alone.
    fn cut_together<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &mut BodySet<K::Shape>,
        places: &[Placed],
        elements: &[Element],
    ) -> Result<(), String> {
        if elements.is_empty() {
            return Ok(());
        }
        let participants = self
            .cut_participants(ctx, set, &places[0])?
            .unwrap_or_default();
        let mut tools = Vec::with_capacity(places.len());
        for place in places {
            let stored = ctx.feature_tool(place.feature)?;
            match moved_copies(
                ctx,
                &stored,
                &place.steps,
                elements,
                Some((&*set, participants.as_slice())),
            )? {
                Some(tool) => tools.push(tool),
                None => return Err(self.unreached(ctx, place)),
            }
        }
        let tool = if tools.len() == 1 {
            tools.pop().expect("one tool")
        } else {
            let refs: Vec<&K::Shape> = tools.iter().collect();
            ctx.kernel.unite(&refs).map_err(|e| e.to_string())?
        };
        if !apply_tool(ctx, set, Operation::Cut, &participants, &tool)? {
            return Err(self.unreached(ctx, &places[0]));
        }
        Ok(())
    }

    /// Every place at each element, one element after another.
    fn by_elements<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &mut BodySet<K::Shape>,
        elements: &[Element],
    ) -> Result<(), String> {
        let mut reached = vec![false; self.places.len()];
        // Places all of whose copies are skipped have none to reach.
        for (i, done) in reached.iter_mut().enumerate() {
            *done = self.elements_of(i, elements).is_empty();
        }
        for element in elements {
            for (i, place) in self.places.iter().enumerate() {
                // The place's own element of that number (none: skipped).
                let Some(own) = self
                    .elements_of(i, elements)
                    .iter()
                    .find(|e| e.index == element.index)
                else {
                    continue;
                };
                reached[i] |= self.place(ctx, set, place, std::slice::from_ref(own))?;
            }
        }
        match reached.iter().position(|r| !r) {
            Some(i) if !elements.is_empty() => Err(self.unreached(ctx, &self.places[i])),
            _ => Ok(()),
        }
    }

    fn unreached<K: Kernel>(&self, ctx: &EvalContext<'_, K>, place: &Placed) -> String {
        let name = ctx.feature_name(place.feature);
        if self.dressups.contains(&place.feature) {
            format!("no copy of the edges of {name} is on a body")
        } else {
            format!("no copy of {name} reaches a body")
        }
    }

    /// The copies of the feature at `place` at the elements; false when none
    /// reaches a body (a fillet's: none finds an edge).
    fn place<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        set: &mut BodySet<K::Shape>,
        place: &Placed,
        elements: &[Element],
    ) -> Result<bool, String> {
        if self.dressups.contains(&place.feature) {
            return repeat_dressup(
                ctx,
                set,
                place.feature,
                &place.steps,
                elements,
                self.copied,
                self.dressups,
            );
        }
        let uid = &place.feature;
        let entry = ctx.feature_entry(*uid)?;
        if changes_sheet(&entry.def) {
            return repeat_sheet_change(ctx, set, place, elements);
        }
        let tool_use = replay_tool(ctx, *uid, Some(set))?
            .ok_or_else(|| format!("{} cannot be patterned or mirrored", entry.name))?;
        let stored = ctx.feature_tool(*uid)?;
        let participants = if self.original_bodies && tool_use.participants.is_empty() {
            changed_bodies(ctx, *uid, set)?
        } else {
            present_participants(ctx, *uid, tool_use.operation, &tool_use.participants, set)?
        };
        let mut reached = false;
        if self.compute == ComputeOption::Adjust {
            let mut copies = adjust_copies(ctx, &entry, place, &stored, elements)?;
            if matches!(tool_use.operation, Operation::Cut | Operation::Join) {
                copies = reaching(ctx, set, &participants, copies);
            }
            match all_at_once(ctx, set, tool_use.operation, &participants, &copies) {
                Some(r) => reached = r,
                None => {
                    for copy in &copies {
                        reached |= apply_tool(ctx, set, tool_use.operation, &participants, copy)?;
                    }
                }
            }
        } else if !elements.is_empty() {
            // A join's copies that touch nothing become bodies here, so only
            // a cut's are left out.
            let reach =
                (tool_use.operation == Operation::Cut).then_some((&*set, participants.as_slice()));
            if let Some(tool) = moved_copies(ctx, &stored, &place.steps, elements, reach)? {
                reached = apply_tool(ctx, set, tool_use.operation, &participants, &tool)?;
            }
        }
        Ok(reached)
    }
}

/// A pattern of features with `combine` (mitcad#673): the bodies its
/// copies made (copies of a feature that made a new body, with their
/// fillets and chamfers) joined to the one body the features made or
/// changed, as it is at the pattern, in one operation; a copy that touches
/// nothing stays a body of its own. The `.f3d` designs whose pattern of
/// such a feature makes no body keep the copies in the original's body.
fn join_copies<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    set: &mut BodySet<K::Shape>,
    places: &[Placed],
) -> Result<(), String> {
    let made = set.made();
    if made.is_empty() {
        return Ok(());
    }
    let mut targets: Vec<BodyUid> = Vec::new();
    for place in places {
        for body in changed_bodies(ctx, place.feature, set)? {
            if set.was_there(body) && !targets.contains(&body) {
                targets.push(body);
            }
        }
    }
    let [target] = targets[..] else {
        return Err(format!(
            "the copies are joined to the body of the patterned features, but they changed {} \
             bodies that are there",
            targets.len()
        ));
    };
    let shapes: Vec<K::Shape> = made.iter().filter_map(|b| set.get(*b).cloned()).collect();
    // The bodies are apart (each a solid of the copies' union, or of a body
    // the copies did not reach), so one compound of them is the tool; a
    // union of them again took 4 s for 372 copies.
    let tool = match ctx.kernel.compound(&shapes) {
        Ok(tool) => tool,
        Err(KernelError::Unsupported(_)) => {
            let refs: Vec<&K::Shape> = shapes.iter().collect();
            ctx.kernel.unite(&refs).map_err(|e| e.to_string())?
        }
        Err(e) => return Err(e.to_string()),
    };
    for body in made {
        set.remove(body);
    }
    apply_tool(ctx, set, Operation::Join, &[target], &tool)?;
    Ok(())
}

/// The bodies feature `uid` changed (set or removed) that are still there,
/// for copies that act on them only ([`RectangularPatternDef::original_bodies`]);
/// none (every body) when none is left.
fn changed_bodies<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    uid: FeatureUid,
    set: &BodySet<K::Shape>,
) -> Result<Vec<BodyUid>, String> {
    Ok(ctx
        .feature_changed_bodies(uid)?
        .into_iter()
        .filter(|b| set.get(*b).is_some())
        .collect())
}

/// The participants of feature `uid` for its copies, as the bodies are at
/// the pattern's point (mitcad#406): a participant the feature itself
/// merged into another (a join that touched both) is the body it went
/// into ([`merged_into`]), each body listed once. A participant that is not
/// there otherwise fails the pattern, saying so when the feature consumed
/// it itself (a cut that left nothing of it).
fn present_participants<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    uid: FeatureUid,
    operation: Operation,
    participants: &[BodyUid],
    set: &BodySet<K::Shape>,
) -> Result<Vec<BodyUid>, String> {
    let mut present: Vec<BodyUid> = Vec::with_capacity(participants.len());
    for (i, body) in participants.iter().enumerate() {
        let body = if set.get(*body).is_some() {
            *body
        } else {
            merged_into(ctx, uid, operation, &participants[..i], *body)?
        };
        if !present.contains(&body) {
            present.push(body);
        }
    }
    Ok(present)
}

/// The body that participant `body`, which is not there at the pattern's
/// point, went into when feature `uid` merged it. A join keeps a merged
/// body under the first of its participants among it (the others are
/// removed), so it is one of `earlier`, the participants before `body`,
/// that the feature set; of several, the one whose shape after the feature
/// holds the box of `body` before it (the smallest such box), else the
/// first. (Those are there: [`present_participants`] has checked them.)
fn merged_into<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    uid: FeatureUid,
    operation: Operation,
    earlier: &[BodyUid],
    body: BodyUid,
) -> Result<BodyUid, String> {
    let changes = ctx.feature_body_changes(uid)?;
    if !changes.iter().any(|(b, s)| *b == body && s.is_none()) {
        return Err(format!(
            "body {body} does not exist at this point of the timeline"
        ));
    }
    let name = ctx.feature_name(uid);
    let candidates: Vec<(BodyUid, &K::Shape)> = earlier
        .iter()
        .filter_map(|e| {
            changes
                .iter()
                .find_map(|(b, s)| (b == e).then_some(s.as_ref()).flatten())
                .map(|s| (*e, s))
        })
        .collect();
    if operation != Operation::Join || candidates.is_empty() {
        return Err(format!(
            "participant {body} of {name} was consumed by {name} itself"
        ));
    }
    let mut target = candidates[0].0;
    if candidates.len() > 1 {
        let before = ctx.body_before(uid, body);
        let measure = |shape: &K::Shape| ctx.kernel.enclosing_box(shape).ok().flatten();
        let holds = |outer: &BoundingBox, inner: &BoundingBox| {
            (0..3).all(|i| {
                outer.min[i] <= inner.min[i] + REACH && inner.max[i] <= outer.max[i] + REACH
            })
        };
        let volume = |b: &BoundingBox| {
            (0..3)
                .map(|i| (b.max[i] - b.min[i]).max(0.0))
                .product::<f64>()
        };
        if let Some(inner) = before.as_ref().and_then(measure) {
            let holding = candidates
                .iter()
                .filter_map(|(c, shape)| measure(shape).map(|b| (*c, b)))
                .filter(|(_, b)| holds(b, &inner))
                .min_by(|a, b| volume(&a.1).total_cmp(&volume(&b.1)));
            if let Some((c, _)) = holding {
                target = c;
            }
        }
    }
    Ok(target)
}

/// How far (mm) a mirror image may lie from the body and still count as
/// the body's own faces when the two are joined ([`combined`]).
const NEAR_COPY_SLACK: f64 = 0.1;

/// A body joined with its copy (a mirror's `combine`). The mirror image of
/// a nearly symmetric body lies on the body almost everywhere, nearly but
/// not exactly, and a boolean of the whole shapes intersects every such
/// pair of faces (minutes for free-form faces, mitcad#88): such a copy is
/// joined only where it differs from the body by more than
/// [`NEAR_COPY_SLACK`] ([`Kernel::join_near_copy`]); any other copy, and
/// with a kernel without it, the whole shapes. A body of several solids is
/// joined lump by lump and keeps the lumps the copy does not reach; the
/// result has at most one piece of the body (its solids in one compound),
/// so no solid of it is dropped (mitcad#1003).
fn combined<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    body: &K::Shape,
    copy: &K::Shape,
) -> Result<BooleanOutput<K::Shape>, String> {
    let lumps = body_lumps(ctx.kernel, body);
    let result = if lumps.len() > 1 {
        let refs: Vec<&K::Shape> = lumps.iter().collect();
        ctx.kernel.boolean(BooleanOp::Join, &refs, copy)
    } else {
        match ctx.kernel.join_near_copy(body, copy, NEAR_COPY_SLACK) {
            Ok(Some(result)) => Ok(result),
            Ok(None) | Err(KernelError::Unsupported(_)) => {
                ctx.kernel.boolean(BooleanOp::Join, &[body], copy)
            }
            Err(e) => Err(e),
        }
    }
    .map_err(|e| e.to_string())?;
    let touched = result.touched.iter().any(|t| *t);
    let mut pieces = Vec::new();
    for joined in join_pieces(ctx.kernel, result.pieces, &[lumps], &result.touched) {
        match joined {
            Joined::Body(_, shape) => pieces.push(BooleanPiece {
                shape,
                sources: vec![0],
            }),
            Joined::Stray(shape) => pieces.push(BooleanPiece {
                shape,
                sources: Vec::new(),
            }),
            // (Not with one body.)
            Joined::Merged(_) => {}
            Joined::Apart(_) => {
                return Err(
                    "the join leaves the body in several solids the kernel cannot hold in one body"
                        .to_owned(),
                );
            }
        }
    }
    Ok(BooleanOutput {
        pieces,
        touched: vec![touched],
    })
}

/// Boxes this far apart still meet (mm) when the bodies a copy reaches
/// are found ([`Joining::reached`]), as for the automatic operation.
const CONTACT_BOX_SLACK: f64 = 1.0e-5;

/// The bodies a pattern's copies join with `combine_any` (mitcad#1006), as
/// the pattern changes them.
struct Joining<S> {
    /// The bodies a copy may join, each with its shape so far and its box:
    /// those there before the pattern first, in their order, then the
    /// bodies the pattern made.
    bodies: Vec<(BodyUid, S, Option<BoundingBox>)>,
    /// The bodies there before the pattern.
    before: Vec<BodyUid>,
    /// The bodies the pattern set, in the order it first set them.
    set: Vec<BodyUid>,
}

impl<S: Clone> Joining<S> {
    fn new<K: Kernel<Shape = S>>(kernel: &K, bodies: Vec<(BodyUid, S)>) -> Self {
        Self {
            before: bodies.iter().map(|(uid, _)| *uid).collect(),
            bodies: bodies
                .into_iter()
                .map(|(uid, shape)| {
                    let around = kernel.bounding_box(&shape).ok().flatten();
                    (uid, shape, around)
                })
                .collect(),
            set: Vec::new(),
        }
    }

    /// A body the pattern makes.
    fn add<K: Kernel<Shape = S>>(&mut self, kernel: &K, uid: BodyUid, shape: S) {
        let around = kernel.bounding_box(&shape).ok().flatten();
        self.bodies.push((uid, shape, around));
        self.set.push(uid);
    }

    /// Body i's new shape.
    fn reshape<K: Kernel<Shape = S>>(&mut self, kernel: &K, i: usize, shape: S) {
        let around = kernel.bounding_box(&shape).ok().flatten();
        let uid = self.bodies[i].0;
        self.bodies[i].1 = shape;
        self.bodies[i].2 = around;
        if !self.set.contains(&uid) {
            self.set.push(uid);
        }
    }

    /// Takes out the bodies merged into others, by their positions.
    fn remove(&mut self, mut merged: Vec<usize>) {
        merged.sort_unstable();
        merged.dedup();
        for i in merged.into_iter().rev() {
            self.bodies.remove(i);
        }
    }

    /// Of the bodies, those a copy (with its box) touches or reaches into:
    /// boxes apart are apart; where the boxes of several meet, the kernel's
    /// contact test tells, and without one they all count (the join then
    /// tells).
    fn reached<K: Kernel<Shape = S>>(
        &self,
        kernel: &K,
        copy: &S,
        around: Option<BoundingBox>,
    ) -> Vec<usize> {
        let near: Vec<usize> = self
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, (_, _, body))| match (&around, body) {
                (Some(a), Some(b)) => a.overlaps(b, CONTACT_BOX_SLACK),
                _ => true,
            })
            .map(|(i, _)| i)
            .collect();
        if near.len() < 2 {
            return near;
        }
        let shapes: Vec<&S> = near.iter().map(|i| &self.bodies[*i].1).collect();
        match kernel.tool_contacts(copy, &shapes) {
            Ok(contacts) if contacts.len() == near.len() => near
                .into_iter()
                .zip(contacts)
                .filter(|(_, contact)| *contact != ToolContact::Apart)
                .map(|(i, _)| i)
                .collect(),
            _ => near,
        }
    }

    /// Sets of the bodies the pattern changed or made that are still there,
    /// in the order it first set them, then removals of the bodies there
    /// before it that it merged into others.
    fn changes(self) -> Vec<BodyChange<S>> {
        let mut changes: Vec<BodyChange<S>> = self
            .set
            .iter()
            .filter_map(|uid| {
                self.bodies
                    .iter()
                    .find(|(b, _, _)| b == uid)
                    .map(|(b, shape, _)| BodyChange::Set(*b, shape.clone()))
            })
            .collect();
        changes.extend(
            self.before
                .iter()
                .filter(|uid| !self.bodies.iter().any(|(b, _, _)| b == *uid))
                .map(|uid| BodyChange::Remove(*uid)),
        );
        changes
    }
}

/// Joins a copy with every body of `joining` it touches (`combine_any`,
/// mitcad#1006): with one of them as `combine` joins a copy with its
/// original ([`combined`]: a mirror image's near-copy join too); with
/// several in one join of their lumps, the bodies it joins together going
/// into the first of them (the bodies there before the pattern first).
/// None when it touches none; else the solids of the copy that touch none,
/// to be bodies of their own.
fn join_any<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    joining: &mut Joining<K::Shape>,
    copy: &K::Shape,
) -> Result<Option<Vec<K::Shape>>, String> {
    let around = ctx.kernel.bounding_box(copy).ok().flatten();
    let reached = joining.reached(ctx.kernel, copy, around);
    let mut strays = Vec::new();
    match reached[..] {
        [] => return Ok(None),
        [i] => {
            let result = combined(ctx, &joining.bodies[i].1, copy)?;
            if !result.touched[0] {
                return Ok(None);
            }
            let mut joined = None;
            for piece in result.pieces {
                if piece.sources.is_empty() {
                    strays.push(piece.shape);
                } else {
                    joined = Some(piece.shape);
                }
            }
            if let Some(shape) = joined {
                joining.reshape(ctx.kernel, i, shape);
            }
        }
        _ => {
            let mut lumps = Vec::with_capacity(reached.len());
            for i in &reached {
                lumps.push(body_lumps(ctx.kernel, &joining.bodies[*i].1));
            }
            let refs: Vec<&K::Shape> = lumps.iter().flatten().collect();
            let result = ctx
                .kernel
                .boolean(BooleanOp::Join, &refs, copy)
                .map_err(|e| e.to_string())?;
            if !result.touched.iter().any(|t| *t) {
                return Ok(None);
            }
            let mut merged = Vec::new();
            for joined in join_pieces(ctx.kernel, result.pieces, &lumps, &result.touched) {
                match joined {
                    Joined::Body(k, shape) => joining.reshape(ctx.kernel, reached[k], shape),
                    Joined::Merged(k) => merged.push(reached[k]),
                    Joined::Stray(shape) | Joined::Apart(shape) => strays.push(shape),
                }
            }
            joining.remove(merged);
        }
    }
    Ok(Some(strays))
}

/// A pattern or mirror of bodies with `combine_any` (mitcad#1006): the
/// copies of each body as it was before the pattern at the elements (the
/// bodies in their order, each at every element), each joined with every
/// body of the pattern it touches ([`join_any`]). A copy that touches none
/// is the body `<pattern>.b<k>` it is without `combine`; solids of a copy
/// that touch none while the rest of it does are bodies after those.
fn repeat_bodies_joining_any<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    bodies: &[BodyUid],
    elements: &[Element],
    skipped: &[u32],
) -> Result<FeatureOutput<K::Shape>, String> {
    let count = bodies.len() as u32;
    let mut originals = Vec::with_capacity(bodies.len());
    for body in bodies {
        let shape = ctx.body(*body)?;
        // Copies of a mesh body are not joined (mitcad#399).
        ctx.require_surfaces(*body, &shape)?;
        originals.push((*body, shape));
    }
    let mut joining = Joining::new(ctx.kernel, originals.clone());
    let mut next = elements.iter().map(|e| e.index).max().unwrap_or(0) * count;
    for (j, (_, shape)) in originals.iter().enumerate() {
        // Skipped copies (mitcad#547): their numbers are the elements'.
        for element in elements.iter().filter(|e| !skipped.contains(&e.index)) {
            let instance = Instance {
                feature: ctx.uid,
                index: element.index,
            };
            let copy = ctx
                .kernel
                .transform_shape(shape, &element.transform, Some(instance))
                .map_err(|e| e.to_string())?;
            match join_any(ctx, &mut joining, &copy)? {
                None => {
                    let uid = BodyUid::new(ctx.uid, (element.index - 1) * count + j as u32);
                    joining.add(ctx.kernel, uid, copy);
                }
                Some(strays) => {
                    for stray in strays {
                        joining.add(ctx.kernel, BodyUid::new(ctx.uid, next), stray);
                        next += 1;
                    }
                }
            }
        }
    }
    Ok(FeatureOutput {
        changes: joining.changes(),
        ..FeatureOutput::default()
    })
}

/// Adjust's copies of a feature's tool applied in one boolean operation
/// instead of one after another: the same result for a cut or a join
/// (removing and adding are associative; copies of a join that touch no
/// body are left out, as Adjust skips them) and much faster on large
/// bodies with many copies. Only taken when it makes no new body (no body
/// split apart), so the bodies keep the ids one copy after another gives
/// them. None when the copies are to be applied one by one (the bodies
/// unchanged).
fn all_at_once<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    set: &mut BodySet<K::Shape>,
    operation: Operation,
    participants: &[BodyUid],
    copies: &[K::Shape],
) -> Option<bool> {
    if copies.len() < 2 || !matches!(operation, Operation::Cut | Operation::Join) {
        return None;
    }
    let refs: Vec<&K::Shape> = copies.iter().collect();
    let tool = ctx.kernel.unite(&refs).ok()?;
    let (saved, next_body) = (set.clone(), ctx.next_body);
    match apply_tool_strays(ctx, set, operation, participants, &tool, false) {
        Ok(reached) if ctx.next_body == next_body => Some(reached),
        _ => {
            *set = saved;
            ctx.next_body = next_body;
            None
        }
    }
}

/// Adjust's copies of the feature at `place` at the elements, in their
/// order: its tool rebuilt at each copy's place where the feature can
/// ([`super::Evaluate::placed_tool`]; for rigid motions only, a mirror
/// reflects the finished tool), else `stored`, the tool it left, moved
/// there.
fn adjust_copies<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    entry: &FeatureEntry,
    place: &Placed,
    stored: &K::Shape,
    elements: &[Element],
) -> Result<Vec<K::Shape>, String> {
    // The place's steps, composed: where the copy at an element goes is
    // the element's transform after them.
    let at = place.at();
    let mut copies = Vec::with_capacity(elements.len());
    for element in elements {
        let instance = Instance {
            feature: ctx.uid,
            index: element.index,
        };
        let transform = element.transform.after(&at);
        let placed = if transform.is_rigid() {
            entry
                .def
                .evaluator::<K>()
                .placed_tool(ctx, place.feature, &transform)
        } else {
            None
        };
        let copy = match placed {
            Some(tool) => placed_copy(
                ctx,
                &tool.map_err(|e| format!("element {}: {e}", element.index))?,
                &place.steps,
                &element.transform,
                instance,
                true,
            )?,
            None => placed_copy(
                ctx,
                stored,
                &place.steps,
                &element.transform,
                instance,
                false,
            )?,
        };
        copies.push(copy);
    }
    Ok(copies)
}

/// The tool moved through a place's steps (none for the pattern's own
/// objects), then to every element, one copy each.
fn moved_list<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    tool: &K::Shape,
    steps: &[(Transform, Instance)],
    elements: &[Element],
) -> Result<Vec<K::Shape>, String> {
    elements
        .iter()
        .map(|element| {
            let instance = Instance {
                feature: ctx.uid,
                index: element.index,
            };
            placed_copy(ctx, tool, steps, &element.transform, instance, false)
        })
        .collect()
}

/// The tool moved through a place's steps (none for the pattern's own
/// objects), then to every element, as one shape. With `reach` (a cut)
/// only the copies [`reaching`] keeps; None when it keeps none.
fn moved_copies<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    tool: &K::Shape,
    steps: &[(Transform, Instance)],
    elements: &[Element],
    reach: Option<(&BodySet<K::Shape>, &[BodyUid])>,
) -> Result<Option<K::Shape>, String> {
    let mut copies = moved_list(ctx, tool, steps, elements)?;
    if let Some((set, participants)) = reach {
        copies = reaching(ctx, set, participants, copies);
    }
    if copies.len() <= 1 {
        return Ok(copies.into_iter().next());
    }
    let refs: Vec<&K::Shape> = copies.iter().collect();
    ctx.kernel.unite(&refs).map(Some).map_err(|e| e.to_string())
}

/// How far (mm) the boxes of copies may be from a body's box and still be
/// kept by [`reaching`]: wider than the tolerances a boolean operation
/// works with.
const REACH: f64 = 0.1;

/// The copies a cut (or Adjust's join, which skips copies that reach no
/// body) needs, in their order: the others change no body. Copies whose
/// boxes come within [`REACH`] of each other form a group (their union can
/// merge their faces, so a group is kept or left out whole), and a group is
/// kept when one of its copies' boxes comes within [`REACH`] of the box of
/// a participant ([`BodySet::participants`]). A large pattern of holes over
/// a round plate has most copies off the plate, and uniting and cutting
/// with them took most of its time. All copies when a box cannot be
/// measured.
pub(super) fn reaching<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    set: &BodySet<K::Shape>,
    participants: &[BodyUid],
    copies: Vec<K::Shape>,
) -> Vec<K::Shape> {
    // (Boxes that hold the shapes: the tight ones of a body of thousands of
    // spline faces took half of a pattern's time.)
    let measure = |shape: &K::Shape| ctx.kernel.enclosing_box(shape).map_err(|_| ());
    let Ok(boxes) = copies.iter().map(measure).collect::<Result<Vec<_>, ()>>() else {
        return copies;
    };
    let Ok(bodies) = set
        .participants(participants)
        .into_iter()
        .map(measure)
        .collect::<Result<Vec<_>, ()>>()
    else {
        return copies;
    };
    let bodies: Vec<BoundingBox> = bodies.into_iter().flatten().collect();
    let near = |a: &BoundingBox, b: &BoundingBox| {
        (0..3).all(|i| a.min[i] <= b.max[i] + REACH && b.min[i] <= a.max[i] + REACH)
    };
    // Groups of copies with touching boxes (a copy without a box is kept),
    // found by sweeping the boxes along x.
    let mut group: Vec<usize> = (0..copies.len()).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    let mut order: Vec<usize> = (0..copies.len()).filter(|&i| boxes[i].is_some()).collect();
    let low = |i: usize| boxes[i].as_ref().map_or(f64::NEG_INFINITY, |b| b.min[0]);
    order.sort_by(|&a, &b| low(a).total_cmp(&low(b)));
    for (k, &i) in order.iter().enumerate() {
        let a = boxes[i].as_ref().expect("measured");
        for &j in &order[k + 1..] {
            let b = boxes[j].as_ref().expect("measured");
            if b.min[0] > a.max[0] + REACH {
                break;
            }
            if near(a, b) {
                let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                group[ri] = rj;
            }
        }
    }
    let mut kept = vec![false; copies.len()];
    for (i, copy_box) in boxes.iter().enumerate() {
        let reaches = match copy_box {
            Some(b) => bodies.iter().any(|body| near(b, body)),
            None => true,
        };
        if reaches {
            let r = root(&mut group, i);
            kept[r] = true;
        }
    }
    let keep: Vec<bool> = (0..copies.len())
        .map(|i| kept[root(&mut group, i)])
        .collect();
    copies
        .into_iter()
        .zip(keep)
        .filter_map(|(copy, keep)| keep.then_some(copy))
        .collect()
}

/// How [`Copying::by_places`] goes on at a place ([`Copying::together`]).
enum Together {
    /// So many places were applied in one operation.
    Applied(usize),
    /// No run here: the place goes on its own.
    No,
}

/// A place of a run applied in one operation ([`Copying::together`]).
struct RunPlace<S> {
    /// Its number among the pattern's places.
    index: usize,
    operation: Operation,
    /// The body it joins or cuts.
    body: BodyUid,
    /// The copies the operation gets ([`reaching`] has left out those that
    /// stay away from the body where [`Copying::place`] leaves them out),
    /// and the tool they make (united, as [`Copying::place`] unites them).
    copies: Vec<S>,
    tool: S,
    /// The boxes of all the place's copies.
    boxes: Vec<BoundingBox>,
}

/// Whether patterns apply runs of their features in one operation
/// ([`Copying::together`]); `MITCAD_NO_PATTERN_RUNS=1` turns it off, for
/// comparisons.
fn runs_enabled() -> bool {
    std::env::var_os("MITCAD_NO_PATTERN_RUNS").is_none_or(|v| v.is_empty() || v == "0")
}

/// Whether two boxes come within [`REACH`] of each other.
fn meet(a: &BoundingBox, b: &BoundingBox) -> bool {
    (0..3).all(|i| a.min[i] <= b.max[i] + REACH && b.min[i] <= a.max[i] + REACH)
}

/// Whether no box of `a` comes within [`REACH`] of a box of `b`.
fn apart(a: &[BoundingBox], b: &[BoundingBox]) -> bool {
    !a.iter().any(|x| b.iter().any(|y| meet(x, y)))
}

/// Whether some face of `copy` is a face of `result` (or pieces of one), by
/// their names; false when the kernel cannot tell.
fn has_faces_of<K: Kernel>(ctx: &EvalContext<'_, K>, result: &K::Shape, copy: &K::Shape) -> bool {
    let Ok(faces) = ctx.kernel.faces(copy) else {
        return false;
    };
    faces
        .iter()
        .flat_map(|f| f.names.iter())
        .filter_map(|name| name.parse::<FaceName>().ok())
        .any(|name| ctx.kernel.count_faces(result, &name).is_ok_and(|n| n > 0))
}

/// Whether each face of `target` that became several faces of `result`
/// has the copies of one place of the run only near its box
/// ([`Copying::together`]); otherwise why not. `name` names the run's
/// places for the reason.
fn splits_of_one_place<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    target: &K::Shape,
    result: &K::Shape,
    run: &[RunPlace<K::Shape>],
    name: impl Fn(usize) -> String,
) -> Result<(), String> {
    let Ok(faces) = ctx.kernel.faces(target) else {
        return Err("the body's faces unknown".to_owned());
    };
    let mut split = Vec::new();
    for (i, face) in faces.iter().enumerate() {
        // The pieces of a face number each of its names alike; a face
        // without a name gives pieces without one.
        let Some(text) = face.names.first() else {
            continue;
        };
        let Ok(name) = text.parse::<FaceName>() else {
            return Err(format!("face name {text} unread"));
        };
        match (
            ctx.kernel.count_faces(target, &name),
            ctx.kernel.count_faces(result, &name),
        ) {
            (Ok(before), Ok(after)) if after <= before => {}
            (Ok(_), Ok(_)) => split.push(i),
            _ => return Err(format!("face {text} not found")),
        }
    }
    if split.is_empty() {
        return Ok(());
    }
    let boxes = match ctx.kernel.face_boxes(target) {
        Ok(boxes) if boxes.len() == faces.len() => boxes,
        _ => return Err("the body's face boxes unknown".to_owned()),
    };
    for i in split {
        let [min, max] = boxes[i];
        let face = BoundingBox { min, max };
        let near: Vec<String> = (0..run.len())
            .filter(|m| run[*m].boxes.iter().any(|b| meet(b, &face)))
            .map(&name)
            .collect();
        if near.len() > 1 {
            return Err(format!(
                "face {} split near the copies of {}",
                faces[i].names.first().map_or("", String::as_str),
                near.join(", ")
            ));
        }
    }
    Ok(())
}

/// The faces of a run's body near the copies of its places, foreseeing
/// before the operation whether the run splits a face near the copies of
/// two places ([`Copying::together`]): the body's face boxes (none known:
/// nothing foreseen, the check after the operation decides), and for each
/// face the first place near it and whether a place near it may split it.
/// A copy may split a face unless it stands inside it: the face is flat
/// (its box no thicker than [`REACH`] along an axis) and the copy's box
/// lies inside the face's box across it, more than [`REACH`] from its
/// sides. (Inside, a copy makes a hole or a boss in the face, and splits
/// it only in rare shapes, such as a ring; the check after the operation
/// finds those.)
struct SharedFaces {
    boxes: Vec<BoundingBox>,
    near: Vec<Option<(usize, bool)>>,
}

impl SharedFaces {
    fn of<K: Kernel, S>(
        ctx: &EvalContext<'_, K>,
        set: &BodySet<K::Shape>,
        first: &RunPlace<S>,
    ) -> Self {
        let boxes: Vec<BoundingBox> = set
            .get(first.body)
            .and_then(|body| ctx.kernel.face_boxes(body).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|[min, max]| BoundingBox { min, max })
            .collect();
        let mut shared = Self {
            near: vec![None; boxes.len()],
            boxes,
        };
        // The first place cannot conflict with itself.
        let _ = shared.add(first);
        shared
    }

    /// Adds the place's copies; the first face near them and an earlier
    /// place's copies that one of them may split, with that place's index.
    fn add<S>(&mut self, place: &RunPlace<S>) -> Result<(), (usize, usize)> {
        for (f, face) in self.boxes.iter().enumerate() {
            let near: Vec<&BoundingBox> = place.boxes.iter().filter(|b| meet(b, face)).collect();
            if near.is_empty() {
                continue;
            }
            let splits = near.iter().any(|b| !stands_inside(b, face));
            match self.near[f] {
                None => self.near[f] = Some((place.index, splits)),
                Some((other, _)) if other == place.index => {}
                Some((other, others_split)) => {
                    if splits || others_split {
                        return Err((f, other));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Whether a copy's box stands inside a flat face's box: within [`REACH`]
/// of the face's plane and more than [`REACH`] from its sides across it.
fn stands_inside(copy: &BoundingBox, face: &BoundingBox) -> bool {
    let thin = (0..3).find(|i| face.max[*i] - face.min[*i] <= REACH);
    let Some(across) = thin else {
        return false;
    };
    (0..3)
        .filter(|i| *i != across)
        .all(|i| copy.min[i] > face.min[i] + REACH && copy.max[i] < face.max[i] - REACH)
}

/// A line of the booleans' trace (`MITCAD_BOOLEAN_TRACE`, the kernel's
/// lines on standard error) about the pattern's runs.
fn trace_run<K: Kernel>(ctx: &EvalContext<'_, K>, text: impl FnOnce() -> String) {
    if std::env::var_os("MITCAD_BOOLEAN_TRACE").is_some() {
        eprintln!("boolean-trace: pattern {} {}", ctx.uid, text());
    }
}

/// What an operation does to a body, for the trace.
fn verb_of(operation: Operation) -> &'static str {
    match operation {
        Operation::Join => "joins",
        Operation::Cut => "cuts",
        _ => "changes",
    }
}

fn check_distance(what: &str, q: u32, value: f64) -> Result<(), String> {
    if !value.is_finite() || (q > 1 && value == 0.0) {
        Err(format!("the {what} must not be zero"))
    } else {
        Ok(())
    }
}

/// `combine` joins bodies only; scale groups come with a scale
/// (mitcad#539).
pub(super) fn check_options(
    ctx: &CheckContext<'_>,
    objects: &PatternObjects,
    combine: bool,
    spec: &ScaleSpec<'_>,
    skipped: &[u32],
) -> Result<(), String> {
    // Bodies, or patterns of bodies among the features (mitcad#547).
    let of_bodies = match objects {
        PatternObjects::Bodies { .. } => true,
        PatternObjects::Features { features } => {
            let mut all = !features.is_empty();
            for uid in features {
                let entry = ctx.feature(*uid)?;
                all &= matches!(
                    repeated(&entry.def),
                    Some((PatternObjects::Bodies { .. }, _))
                );
            }
            all
        }
        _ => false,
    };
    if combine && !of_bodies {
        return Err("combine applies to bodies and patterns of bodies only".to_owned());
    }
    if spec.scale.is_none() && (spec.groups.is_some() || spec.copies.is_some()) {
        return Err("scale groups and scale copies need a scale".to_owned());
    }
    if let Some(center) = spec.center {
        if !spec.scales() {
            return Err("a scale centre needs a scale".to_owned());
        }
        center.check(ctx, Want::Point)?;
    }
    if !skipped.is_empty() {
        if !matches!(
            objects,
            PatternObjects::Features { .. } | PatternObjects::Bodies { .. }
        ) {
            return Err("skipped copies apply to features and bodies only".to_owned());
        }
        for (i, copy) in skipped.iter().enumerate() {
            if *copy == 0 {
                return Err("copy 0 is the original and cannot be skipped".to_owned());
            }
            if skipped[..i].contains(copy) {
                return Err(format!("copy {copy} is listed more than once"));
            }
        }
    }
    Ok(())
}

/// `combine_any` comes with `combine` (mitcad#1006); what it joins is
/// checked as `combine`'s ([`check_options`]).
pub(super) fn check_combine_any(combine: bool, any: bool) -> Result<(), String> {
    if any && !combine {
        return Err("combine_any needs combine".to_owned());
    }
    Ok(())
}

/// The references of the scale options (its centre).
pub(super) fn add_scale_references(spec: &ScaleSpec<'_>, references: &mut References) {
    if let Some(center) = spec.center {
        center.add_to(references);
    }
}

/// The scales' factors, groups (at least two runs) and copies.
pub(super) fn check_scale_values(
    value: &dyn Fn(ParamId) -> f64,
    spec: &ScaleSpec<'_>,
) -> Result<(), String> {
    let main = spec.scale.map(|scale| (scale, spec.groups, spec.copies));
    let more = spec.more.iter().map(|r| (r.scale, r.groups, r.copies));
    for (scale, groups, copies) in main.into_iter().chain(more) {
        check_scale("pattern scale", value(scale))?;
        if let Some(g) = groups
            && quantity("pattern scale groups", value(g))? < 2
        {
            return Err("the pattern scale groups must be at least 2".to_owned());
        }
        if let Some(c) = copies {
            quantity("pattern scale copies", value(c))?;
        }
    }
    Ok(())
}

impl FeatureInfo for RectangularPatternDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.objects.add_references(&mut references);
        self.direction1.axis.add_to(&mut references);
        if let Some(direction) = &self.direction2 {
            direction.axis.add_to(&mut references);
        }
        add_scale_references(&self.scale_spec(), &mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        self.direction1.axis.check(ctx, Want::Axis)?;
        if let Some(direction) = &self.direction2 {
            direction.axis.check(ctx, Want::Axis)?;
        }
        check_combine_any(self.combine, self.combine_any)?;
        check_options(
            ctx,
            &self.objects,
            self.combine,
            &self.scale_spec(),
            &self.skipped_copies,
        )?;
        check_suppressed(&self.suppressed_elements)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        for direction in std::iter::once(&self.direction1).chain(&self.direction2) {
            let q = quantity("pattern quantity", value(direction.quantity))?;
            check_distance("pattern distance", q, value(direction.distance))?;
        }
        check_scale_values(value, &self.scale_spec())
    }

    fn creates_bodies(&self) -> bool {
        self.objects.creates_bodies()
    }
}

impl FeatureInfo for CircularPatternDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.objects.add_references(&mut references);
        self.axis.add_to(&mut references);
        add_scale_references(&self.scale_spec(), &mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        self.axis.check(ctx, Want::Axis)?;
        check_combine_any(self.combine, self.combine_any)?;
        check_options(
            ctx,
            &self.objects,
            self.combine,
            &self.scale_spec(),
            &self.skipped_copies,
        )?;
        check_suppressed(&self.suppressed_elements)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let q = quantity("pattern quantity", value(self.quantity))?;
        check_distance("pattern angle", q, value(self.angle))?;
        check_scale_values(value, &self.scale_spec())
    }

    fn creates_bodies(&self) -> bool {
        self.objects.creates_bodies()
    }
}

impl FeatureInfo for PathPatternDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.objects.add_references(&mut references);
        self.path.add_to(&mut references);
        add_scale_references(&self.scale_spec(), &mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        self.path.check(ctx)?;
        if !self.start.is_finite()
            || self
                .start_point
                .is_some_and(|p| p.iter().any(|v| !v.is_finite()))
        {
            return Err("the start of the path pattern must be finite".to_owned());
        }
        // A path pattern's copies of features join the body of the features
        // with `combine` too (mitcad#673).
        let features = matches!(self.objects, PatternObjects::Features { .. });
        if self.combine && matches!(self.objects, PatternObjects::Faces { .. }) {
            return Err("combine applies to bodies and features only".to_owned());
        }
        check_combine_any(self.combine, self.combine_any)?;
        check_options(
            ctx,
            &self.objects,
            (self.combine && !features) || self.combine_any,
            &self.scale_spec(),
            &self.skipped_copies,
        )?;
        check_suppressed(&self.suppressed_elements)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let q = quantity("pattern quantity", value(self.quantity))?;
        check_distance("pattern distance", q, value(self.distance))?;
        check_scale_values(value, &self.scale_spec())
    }

    fn creates_bodies(&self) -> bool {
        self.objects.creates_bodies()
    }
}

impl<K: Kernel> Evaluate<K> for RectangularPatternDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (elements, scaling) = self.elements(ctx)?;
        repeat(
            ctx,
            &self.objects,
            self.compute,
            self.original_bodies,
            &elements,
            Combine::of(self.combine, self.combine_any),
            scaling.as_ref(),
            &self.skipped_copies,
        )
    }
}

impl<K: Kernel> Evaluate<K> for CircularPatternDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (elements, scaling) = self.elements(ctx)?;
        repeat(
            ctx,
            &self.objects,
            self.compute,
            self.original_bodies,
            &elements,
            Combine::of(self.combine, self.combine_any),
            scaling.as_ref(),
            &self.skipped_copies,
        )
    }
}

impl<K: Kernel> Evaluate<K> for PathPatternDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (elements, scaling) = self.elements(ctx)?;
        repeat(
            ctx,
            &self.objects,
            self.compute,
            self.original_bodies,
            &elements,
            Combine::of(self.combine, self.combine_any),
            scaling.as_ref(),
            &self.skipped_copies,
        )
    }
}
