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
//!   them all at once.
//! - Patterns and mirrors of features among the features (patterns of
//!   patterns, mitcad#4): their originals are repeated at every place the
//!   pattern put them (its elements, the original's too), so a pattern of
//!   a pattern places the originals at every product of their elements,
//!   the inner element first. Each copy's faces carry the inner instance's
//!   name inside the outer one's.
//! - Faces: the faces of a body bound a boss or a pocket together with
//!   planar caps ([`crate::kernel::Kernel::face_tool`]); its copies are
//!   joined to or cut from the body like a feature's tool.
//!
//! Copied faces are named `<pattern>:inst<element>(<original name>)`.
//!
//! Element numbering: along a direction the positions are 0, 1, …, q − 1
//! and, when symmetric, −1, …, −(q − 1); a rectangular pattern numbers the
//! elements direction 1 first: element = k1 + n1 · k2. The ids in .f3d
//! designs (experiment K43) may differ; the import matches elements by transform.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::bodies::{BodySet, apply_tool, apply_tool_strays};
use super::face_refs::UNSUPPORTED;
use super::geom_ref::{GeomRef, PathCurve, PathRef, Want, path_curve};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput,
    Operation, References, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{BooleanOp, Kernel};
use crate::parameters::ParamId;
use crate::topo::FaceName;
use crate::transform::{Instance, Transform, add, cross, scaled, sub};

/// What a pattern or a mirror repeats (`patternEntityType` in .f3d designs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PatternObjects {
    Bodies { bodies: Vec<BodyUid> },
    Features { features: Vec<FeatureUid> },
    Faces { body: BodyUid, faces: Vec<FaceName> },
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
    /// As Identical (a faster copy of faces has no counterpart here).
    Optimized,
}

fn is_adjust(option: &ComputeOption) -> bool {
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
    /// Elements on both sides of the original, `quantity` (and the extent)
    /// on each side (experiment K41).
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
    /// Elements on both sides, `quantity` and `angle` on each side.
    #[serde(default, skip_serializing_if = "is_false")]
    pub symmetric: bool,
    #[serde(default, skip_serializing_if = "is_adjust")]
    pub compute: ComputeOption,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed_elements: Vec<u32>,
}

/// Serde default of optional values, without the `Default` bound that
/// `#[serde(default)]` puts on the value type.
pub(crate) fn none<T>() -> Option<T> {
    None
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

impl<P> PatternDirection<P> {
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
            suppressed_elements: self.suppressed_elements.clone(),
        })
    }
}

impl<P> CircularPatternDef<P> {
    pub const TYPE: &'static str = "circular_pattern";
    pub const BASE_NAME: &'static str = "CircularPattern";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<CircularPatternDef<Q>, E> {
        Ok(CircularPatternDef {
            objects: self.objects.clone(),
            axis: self.axis.clone(),
            quantity: f("quantity", &self.quantity)?,
            angle: f("angle", &self.angle)?,
            symmetric: self.symmetric,
            compute: self.compute,
            suppressed_elements: self.suppressed_elements.clone(),
        })
    }
}

impl<P> PathPatternDef<P> {
    pub const TYPE: &'static str = "path_pattern";
    pub const BASE_NAME: &'static str = "PathPattern";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PathPatternDef<Q>, E> {
        Ok(PathPatternDef {
            objects: self.objects.clone(),
            path: self.path.clone(),
            quantity: f("quantity", &self.quantity)?,
            distance: f("distance", &self.distance)?,
            distance_type: self.distance_type,
            start: self.start,
            flip: self.flip,
            along_path: self.along_path,
            symmetric: self.symmetric,
            compute: self.compute,
            suppressed_elements: self.suppressed_elements.clone(),
        })
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
                        Some((Self::Features { .. }, _)) => continue,
                        Some(_) => {
                            return Err(format!(
                                "{} ({}) cannot be patterned or mirrored: it repeats bodies or \
                                 faces, not features",
                                entry.name, entry.uid
                            ));
                        }
                        None => {}
                    }
                    if entry.def.info().tool_use().is_none() {
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

fn check_suppressed(suppressed: &[u32]) -> Result<(), String> {
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
fn quantity(what: &str, value: f64) -> Result<u32, String> {
    let rounded = value.round();
    if !value.is_finite() || (value - rounded).abs() > 1e-9 || rounded < 1.0 {
        return Err(format!(
            "{what} must be a whole number of at least 1, got {value}"
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

/// Positions along a direction: 0, 1, …, q − 1, then −1, …, −(q − 1) when
/// symmetric.
fn positions(quantity: u32, symmetric: bool) -> Vec<f64> {
    let forward = (0..quantity).map(f64::from);
    let backward = (1..quantity).map(|i| -f64::from(i));
    if symmetric {
        forward.chain(backward).collect()
    } else {
        forward.collect()
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
        FeatureDef::Mirror(d) => Some((&d.objects, &[])),
        _ => None,
    }
}

/// A place of a feature with a tool among a pattern's objects: the steps
/// of the patterns of features (patterns of patterns) that took it there,
/// innermost first: each element's transform and the instance that names
/// its copies.
struct Placed {
    feature: FeatureUid,
    steps: Vec<(Transform, Instance)>,
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
    if entry.def.info().tool_use().is_some() {
        return Ok(vec![Placed {
            feature: uid,
            steps: Vec::new(),
        }]);
    }
    let Some((PatternObjects::Features { features }, suppressed)) = repeated(&entry.def) else {
        return Err(format!("{} cannot be patterned or mirrored", entry.name));
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
    let mut out = Vec::new();
    for (index, transform) in elements.iter().enumerate() {
        let index = index as u32;
        if index > 0 && suppressed.contains(&index) {
            continue;
        }
        for inner in features {
            for mut placed in originals(ctx, *inner, depth + 1)? {
                if index > 0 {
                    placed.steps.push((
                        *transform,
                        Instance {
                            feature: uid,
                            index,
                        },
                    ));
                }
                out.push(placed);
                if out.len() > MAX_PLACES {
                    return Err(format!(
                        "{} repeats its originals at more than {MAX_PLACES} places",
                        entry.name
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// A tool moved through a place's steps (each copy named by its instance),
/// then by `transform` named by `instance`. `moved`: the tool is in its
/// place already (rebuilt there), only the names change.
fn placed_copy<K: Kernel>(
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

/// The elements other than the original that are not suppressed.
fn active(all: Vec<Transform>, suppressed: &[u32]) -> Result<Vec<Element>, String> {
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

impl RectangularPatternDef {
    fn elements<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Vec<Element>, String> {
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
        ctx.elements.clone_from(&all);
        active(all, &self.suppressed_elements)
    }
}

impl CircularPatternDef {
    fn elements<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Vec<Element>, String> {
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
        ctx.elements.clone_from(&all);
        active(all, &self.suppressed_elements)
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

impl PathPatternDef {
    fn elements<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<Vec<Element>, String> {
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
        let curve = path_curve(ctx, &self.path)?;
        let s0 = self.start * curve.length();
        let sign = if self.flip { -1.0 } else { 1.0 };
        let step = sign * step(self.distance_type, distance, q);
        let all: Vec<Transform> = positions(q, self.symmetric)
            .into_iter()
            .map(|i| path_transform(&curve, s0, s0 + i * step, self.along_path))
            .collect();
        ctx.elements.clone_from(&all);
        active(all, &self.suppressed_elements)
    }
}

/// Places copies of the objects at the elements: the evaluation shared by
/// patterns and mirrors. `combine` joins copies of bodies to their
/// originals when they touch (a mirror's `combine`).
pub(crate) fn repeat<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    objects: &PatternObjects,
    compute: ComputeOption,
    elements: &[Element],
    combine: bool,
) -> Result<FeatureOutput<K::Shape>, String> {
    let instance = |uid: FeatureUid, element: &Element| Instance {
        feature: uid,
        index: element.index,
    };
    match objects {
        PatternObjects::Bodies { bodies } => {
            let mut changes = Vec::new();
            let count = bodies.len() as u32;
            for (j, body) in bodies.iter().enumerate() {
                let shape = ctx.body(*body)?;
                let mut merged = None;
                for element in elements {
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
                        let result = ctx
                            .kernel
                            .boolean(BooleanOp::Join, &[target], &copy)
                            .map_err(|e| e.to_string())?;
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
            let mut places = Vec::new();
            for uid in features {
                places.extend(originals(ctx, *uid, 0)?);
            }
            for place in places {
                let uid = &place.feature;
                let entry = ctx.feature_entry(*uid)?;
                let tool_use = entry
                    .def
                    .info()
                    .tool_use()
                    .ok_or_else(|| format!("{} cannot be patterned or mirrored", entry.name))?;
                let stored = ctx.feature_tool(*uid)?;
                // The place's steps, composed: where the copy at an element
                // goes is the element's transform after them.
                let at = place
                    .steps
                    .iter()
                    .fold(Transform::IDENTITY, |done, (t, _)| t.after(&done));
                let mut reached = false;
                if compute == ComputeOption::Adjust {
                    let mut copies = Vec::with_capacity(elements.len());
                    for element in elements {
                        let transform = element.transform.after(&at);
                        // A tool is rebuilt in place for rigid motions only;
                        // a mirror reflects the finished tool.
                        let placed = if transform.is_rigid() {
                            entry
                                .def
                                .evaluator::<K>()
                                .placed_tool(ctx, *uid, &transform)
                        } else {
                            None
                        };
                        let copy = match placed {
                            Some(tool) => placed_copy(
                                ctx,
                                &tool.map_err(|e| format!("element {}: {e}", element.index))?,
                                &place.steps,
                                &element.transform,
                                instance(ctx.uid, element),
                                true,
                            )?,
                            None => placed_copy(
                                ctx,
                                &stored,
                                &place.steps,
                                &element.transform,
                                instance(ctx.uid, element),
                                false,
                            )?,
                        };
                        copies.push(copy);
                    }
                    match all_at_once(
                        ctx,
                        &mut set,
                        tool_use.operation,
                        &tool_use.participants,
                        &copies,
                    ) {
                        Some(r) => reached = r,
                        None => {
                            for copy in &copies {
                                reached |= apply_tool(
                                    ctx,
                                    &mut set,
                                    tool_use.operation,
                                    &tool_use.participants,
                                    copy,
                                )?;
                            }
                        }
                    }
                } else if !elements.is_empty() {
                    let tool = moved_copies(ctx, &stored, &place.steps, elements)?;
                    reached = apply_tool(
                        ctx,
                        &mut set,
                        tool_use.operation,
                        &tool_use.participants,
                        &tool,
                    )?;
                }
                if !reached && !elements.is_empty() {
                    return Err(format!("no copy of {} reaches a body", entry.name));
                }
            }
            Ok(FeatureOutput {
                changes: set.changes(),
                ..FeatureOutput::default()
            })
        }
        PatternObjects::Faces { body, faces } => {
            let shape = ctx.body(*body)?;
            let (tool, op) = ctx
                .kernel
                .face_tool(&shape, faces)
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
                match all_at_once(ctx, &mut set, operation, &[*body], &copies) {
                    Some(r) => reached = r,
                    None => {
                        for copy in &copies {
                            reached |= apply_tool(ctx, &mut set, operation, &[*body], copy)?;
                        }
                    }
                }
            } else if !elements.is_empty() {
                let tool = moved_copies(ctx, &tool, &[], elements)?;
                reached = apply_tool(ctx, &mut set, operation, &[*body], &tool)?;
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

/// The tool moved through a place's steps (none for the pattern's own
/// objects), then to every element, as one shape.
fn moved_copies<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    tool: &K::Shape,
    steps: &[(Transform, Instance)],
    elements: &[Element],
) -> Result<K::Shape, String> {
    let copies = elements
        .iter()
        .map(|element| {
            let instance = Instance {
                feature: ctx.uid,
                index: element.index,
            };
            placed_copy(ctx, tool, steps, &element.transform, instance, false)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if copies.len() == 1 {
        return Ok(copies.into_iter().next().expect("one copy"));
    }
    let refs: Vec<&K::Shape> = copies.iter().collect();
    ctx.kernel.unite(&refs).map_err(|e| e.to_string())
}

fn check_distance(what: &str, q: u32, value: f64) -> Result<(), String> {
    if !value.is_finite() || (q > 1 && value == 0.0) {
        Err(format!("the {what} must not be zero"))
    } else {
        Ok(())
    }
}

impl FeatureInfo for RectangularPatternDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.objects.add_references(&mut references);
        self.direction1.axis.add_to(&mut references);
        if let Some(direction) = &self.direction2 {
            direction.axis.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        self.direction1.axis.check(ctx, Want::Axis)?;
        if let Some(direction) = &self.direction2 {
            direction.axis.check(ctx, Want::Axis)?;
        }
        check_suppressed(&self.suppressed_elements)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        for direction in std::iter::once(&self.direction1).chain(&self.direction2) {
            let q = quantity("pattern quantity", value(direction.quantity))?;
            check_distance("pattern distance", q, value(direction.distance))?;
        }
        Ok(())
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
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        self.axis.check(ctx, Want::Axis)?;
        check_suppressed(&self.suppressed_elements)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let q = quantity("pattern quantity", value(self.quantity))?;
        check_distance("pattern angle", q, value(self.angle))
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
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.objects.check(ctx)?;
        self.path.check(ctx)?;
        if !self.start.is_finite() {
            return Err("the start of the path pattern must be finite".to_owned());
        }
        check_suppressed(&self.suppressed_elements)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let q = quantity("pattern quantity", value(self.quantity))?;
        check_distance("pattern distance", q, value(self.distance))
    }

    fn creates_bodies(&self) -> bool {
        self.objects.creates_bodies()
    }
}

impl<K: Kernel> Evaluate<K> for RectangularPatternDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let elements = self.elements(ctx)?;
        repeat(ctx, &self.objects, self.compute, &elements, false)
    }
}

impl<K: Kernel> Evaluate<K> for CircularPatternDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let elements = self.elements(ctx)?;
        repeat(ctx, &self.objects, self.compute, &elements, false)
    }
}

impl<K: Kernel> Evaluate<K> for PathPatternDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let elements = self.elements(ctx)?;
        repeat(ctx, &self.objects, self.compute, &elements, false)
    }
}

#[cfg(test)]
#[path = "f4_tests.rs"]
mod f4_tests;
