// SPDX-License-Identifier: MIT
//! Move/Copy of bodies: a rigid motion given as a matrix (free move),
//! distances along the model axes or an axis, a rotation about an axis, or
//! from a point to a point or a position. A moved body keeps its id and its
//! faces their names. With `copy` the bodies stay and moved copies become
//! new bodies, `<move>.b<j>` with faces `<move>:inst1(<name>)`; a copy that
//! does not move is a copy and paste of bodies.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::point_pattern::{axis_in, placement, point_in};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureEntry, FeatureInfo, FeatureOutput,
    OccurrencePath, References, is_false,
};
use mitcad_vec3::{scale as scaled, sub};

use crate::assembly::{Assembly, inverse};
use crate::datum::{DatumAxis, Vec3};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::transform::{Instance, Transform};

/// The motion of a move. Distances in millimetres, angles in radians.
///
/// A point or an axis may be geometry of a component where an occurrence
/// places it (mitcad#699): `from_occurrence`, `to_occurrence`,
/// `point_occurrence` and `axis_occurrence` beside the reference give the
/// occurrence's absolute path from the root (`"O1/O4"`; `""` the root), as
/// Pattern at Points' points do. The reference is then resolved in that
/// occurrence's component and placed where the occurrence puts it at the
/// feature's point of the timeline (an occurrence the feature moves: where
/// it was before this motion), then taken into the coordinates of the
/// feature's component through the occurrence of it that the first such
/// path passes through (mitcad#2305: a part picked through the second
/// occurrence of its parent is taken through that one, so that it lands on
/// the picked target there), else through the occurrence a
/// `move_occurrence` records in `parent_occurrence` (mitcad#2307: both
/// points at the root level), else through its first occurrence. Without
/// one, the reference is in the feature's component's coordinates, as
/// before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)]
pub enum MoveSpec<P = ParamId> {
    /// Rows of a rotation and a translation, `[[r00, r01, r02, tx], …]`
    /// (the `defineAsFreeMove` matrix of .f3d designs, in millimetres).
    Free { matrix: [[f64; 4]; 3] },
    /// Along the model axes.
    TranslateXyz { x: P, y: P, z: P },
    /// Along an axis (an edge's own direction).
    TranslateAlong {
        axis: GeomRef,
        distance: P,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        axis_occurrence: Option<OccurrencePath>,
    },
    /// About an axis, right-handed.
    Rotate {
        axis: GeomRef,
        angle: P,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        axis_occurrence: Option<OccurrencePath>,
    },
    /// Takes `from` onto `to`; with `along` an axis, only along that axis
    /// (mitcad#2339).
    PointToPoint {
        from: GeomRef,
        to: GeomRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_occurrence: Option<OccurrencePath>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_occurrence: Option<OccurrencePath>,
        #[serde(default, skip_serializing_if = "MoveAlong::is_all")]
        along: MoveAlong,
    },
    /// Takes `point` to the position.
    PointToPosition {
        point: GeomRef,
        x: P,
        y: P,
        z: P,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        point_occurrence: Option<OccurrencePath>,
    },
}

/// What Point to Point keeps of the motion from its origin to its target
/// (mitcad#2339): all of it, or only its distance along one axis of the
/// feature's component (the component that holds what moves), so that a
/// body is brought level with a face, an edge or a point along that axis
/// without moving sideways.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveAlong {
    /// The whole motion, as before `along` existed.
    #[default]
    All,
    X,
    Y,
    Z,
}

impl MoveAlong {
    /// Whether the whole motion is kept (left out of the definition).
    pub fn is_all(&self) -> bool {
        *self == Self::All
    }

    /// The part of `motion` that is kept: the one component along the
    /// axis (exactly, the others exactly 0), or all of it.
    pub fn keep(self, motion: Vec3) -> Vec3 {
        let axis = match self {
            Self::All => return motion,
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        };
        std::array::from_fn(|i| if i == axis { motion[i] } else { 0.0 })
    }
}

/// What a move's motion was worked out from (mitcad#838): a Rotate's axis
/// and the points of Point to Point and Point to Position as the move
/// resolved them, in the feature's component's coordinates (mm, a unit
/// direction). A preview reports it (`motion`), so that a drag of the
/// angle or the position moves the shown bodies about the same axis and
/// from the same point as the exact result, without asking the model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MotionBasis {
    /// Turns right-handed about the axis through `origin` along
    /// `direction` by the angle.
    Rotate { origin: Vec3, direction: Vec3 },
    /// Moves by `to - from`.
    PointToPoint { from: Vec3, to: Vec3 },
    /// Moves by the position minus `point`.
    PointToPosition { point: Vec3 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveDef<P = ParamId> {
    pub bodies: Vec<BodyUid>,
    pub transform: MoveSpec<P>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub copy: bool,
}

impl<P> MoveDef<P> {
    pub const TYPE: &'static str = "move";
    pub const BASE_NAME: &'static str = "Move";

    map_params! {
        MoveDef {
            bodies: clone,
            transform: map,
            copy: copy,
        }
    }
}

impl<P> MoveSpec<P> {
    /// Converts the value fields (slots `transform.x`, ...).
    // Written by hand: an enum, its slots by kind.
    pub(crate) fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<MoveSpec<Q>, E> {
        Ok(match self {
            MoveSpec::Free { matrix } => MoveSpec::Free { matrix: *matrix },
            MoveSpec::TranslateXyz { x, y, z } => MoveSpec::TranslateXyz {
                x: f("transform.x", x)?,
                y: f("transform.y", y)?,
                z: f("transform.z", z)?,
            },
            MoveSpec::TranslateAlong {
                axis,
                distance,
                axis_occurrence,
            } => MoveSpec::TranslateAlong {
                axis: axis.clone(),
                distance: f("transform.distance", distance)?,
                axis_occurrence: axis_occurrence.clone(),
            },
            MoveSpec::Rotate {
                axis,
                angle,
                axis_occurrence,
            } => MoveSpec::Rotate {
                axis: axis.clone(),
                angle: f("transform.angle", angle)?,
                axis_occurrence: axis_occurrence.clone(),
            },
            MoveSpec::PointToPoint {
                from,
                to,
                from_occurrence,
                to_occurrence,
                along,
            } => MoveSpec::PointToPoint {
                from: from.clone(),
                to: to.clone(),
                from_occurrence: from_occurrence.clone(),
                to_occurrence: to_occurrence.clone(),
                along: *along,
            },
            MoveSpec::PointToPosition {
                point,
                x,
                y,
                z,
                point_occurrence,
            } => MoveSpec::PointToPosition {
                point: point.clone(),
                x: f("transform.x", x)?,
                y: f("transform.y", y)?,
                z: f("transform.z", z)?,
                point_occurrence: point_occurrence.clone(),
            },
        })
    }

    /// The references to geometry with the occurrences they were picked
    /// in (mitcad#699), in the order of the fields.
    pub(crate) fn placed_references(&self) -> Vec<(&GeomRef, Option<&OccurrencePath>)> {
        match self {
            MoveSpec::TranslateAlong {
                axis,
                axis_occurrence,
                ..
            }
            | MoveSpec::Rotate {
                axis,
                axis_occurrence,
                ..
            } => vec![(axis, axis_occurrence.as_ref())],
            MoveSpec::PointToPoint {
                from,
                to,
                from_occurrence,
                to_occurrence,
                ..
            } => vec![
                (from, from_occurrence.as_ref()),
                (to, to_occurrence.as_ref()),
            ],
            MoveSpec::PointToPosition {
                point,
                point_occurrence,
                ..
            } => vec![(point, point_occurrence.as_ref())],
            MoveSpec::Free { .. } | MoveSpec::TranslateXyz { .. } => Vec::new(),
        }
    }

    /// [`MoveSpec::placed_references`], to change.
    pub(crate) fn placed_references_mut(
        &mut self,
    ) -> Vec<(&mut GeomRef, &mut Option<OccurrencePath>)> {
        match self {
            MoveSpec::TranslateAlong {
                axis,
                axis_occurrence,
                ..
            }
            | MoveSpec::Rotate {
                axis,
                axis_occurrence,
                ..
            } => vec![(axis, axis_occurrence)],
            MoveSpec::PointToPoint {
                from,
                to,
                from_occurrence,
                to_occurrence,
                ..
            } => vec![(from, from_occurrence), (to, to_occurrence)],
            MoveSpec::PointToPosition {
                point,
                point_occurrence,
                ..
            } => vec![(point, point_occurrence)],
            MoveSpec::Free { .. } | MoveSpec::TranslateXyz { .. } => Vec::new(),
        }
    }

    /// The occurrence paths the points and axes were picked in.
    pub(crate) fn occurrence_paths(&self) -> Vec<&OccurrencePath> {
        self.placed_references()
            .into_iter()
            .filter_map(|(_, path)| path)
            .collect()
    }
}

impl MoveSpec {
    pub(crate) fn add_references(&self, references: &mut References) {
        for (reference, _) in self.placed_references() {
            reference.add_to(references);
        }
    }

    pub(crate) fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match self {
            Self::Free { matrix } => free_transform(matrix).map(|_| ()),
            Self::TranslateXyz { .. } => Ok(()),
            Self::TranslateAlong { axis, .. } | Self::Rotate { axis, .. } => {
                axis.check(ctx, Want::Axis)
            }
            Self::PointToPoint { from, to, .. } => {
                from.check(ctx, Want::Point)?;
                to.check(ctx, Want::Point)
            }
            Self::PointToPosition { point, .. } => point.check(ctx, Want::Point),
        }
    }

    /// The check of components (instead of the general one in
    /// `document.rs`): sketches and construction geometry a point or an
    /// axis names must be in the component its occurrence path ends in, or
    /// without a path in the feature's. A path that leads nowhere now is
    /// left to recompute, which reports it.
    pub(crate) fn check_components(
        &self,
        assembly: &Assembly,
        features: &[Arc<FeatureEntry>],
        entry: &FeatureEntry,
    ) -> Result<(), String> {
        for (reference, path) in self.placed_references() {
            let component = match path {
                Some(path) => {
                    match crate::joints::path_component(assembly, ComponentUid::ROOT, &path.0) {
                        Ok(component) => component,
                        Err(_) => continue,
                    }
                }
                None => entry.component,
            };
            let mut references = References::default();
            reference.add_to(&mut references);
            for uid in references.features {
                if let Some(other) = features.iter().find(|f| f.uid == uid)
                    && crate::joints::placed_geometry(&other.def)
                    && other.component != component
                {
                    return Err(format!(
                        "{} is in {}, not in {}",
                        other.name,
                        assembly.name(other.component),
                        assembly.name(component)
                    ));
                }
            }
        }
        Ok(())
    }

    /// The motion in the feature's component's coordinates. `parent`: the
    /// occurrence of the feature's component what moves was picked
    /// through (a `move_occurrence`'s `parent_occurrence`, mitcad#2307),
    /// the frame when no picked path passes through one.
    pub(crate) fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        parent: Option<&OccurrencePath>,
    ) -> Result<Transform, String> {
        // The paths the points and axes were picked in, from, to, point or
        // axis first: the first that passes through an occurrence of the
        // feature's component gives the frame (mitcad#2305); else the
        // occurrence what moves was picked through (mitcad#2307).
        let mut picked = self.occurrence_paths();
        picked.extend(parent);
        let transform = match self {
            Self::Free { matrix } => free_transform(matrix)?,
            Self::TranslateXyz { x, y, z } => {
                Transform::translation([ctx.param(*x)?, ctx.param(*y)?, ctx.param(*z)?])
            }
            Self::TranslateAlong {
                axis,
                distance,
                axis_occurrence,
            } => {
                let axis = placed_axis(ctx, axis, axis_occurrence.as_ref(), &picked)?;
                Transform::translation(scaled(axis.direction, ctx.param(*distance)?))
            }
            Self::Rotate {
                axis,
                angle,
                axis_occurrence,
            } => {
                let axis = placed_axis(ctx, axis, axis_occurrence.as_ref(), &picked)?;
                // The axis the motion turns about, for a drag (mitcad#838).
                ctx.move_basis = Some(MotionBasis::Rotate {
                    origin: axis.origin,
                    direction: axis.direction,
                });
                Transform::rotation(axis.origin, axis.direction, ctx.param(*angle)?)
                    .expect("a unit axis direction")
            }
            Self::PointToPoint {
                from,
                to,
                from_occurrence,
                to_occurrence,
                along,
            } => {
                let from = placed_point(ctx, from, from_occurrence.as_ref(), &picked)?;
                let to = placed_point(ctx, to, to_occurrence.as_ref(), &picked)?;
                // The points as resolved; the app keeps Along's part of the
                // motion itself during a drag (mitcad#2339).
                ctx.move_basis = Some(MotionBasis::PointToPoint { from, to });
                // Along an axis of the feature's component, only that
                // distance (mitcad#2339).
                Transform::translation(along.keep(sub(to, from)))
            }
            Self::PointToPosition {
                point,
                x,
                y,
                z,
                point_occurrence,
            } => {
                let point = placed_point(ctx, point, point_occurrence.as_ref(), &picked)?;
                ctx.move_basis = Some(MotionBasis::PointToPosition { point });
                let position = [ctx.param(*x)?, ctx.param(*y)?, ctx.param(*z)?];
                Transform::translation(sub(position, point))
            }
        };
        if transform.is_finite() {
            Ok(transform)
        } else {
            Err("the move is not finite".to_owned())
        }
    }
}

/// From the design's coordinates into the feature's component's: the
/// inverse of where its occurrence on the move's picked paths places it
/// ([`frame_path`], mitcad#2305), else its first occurrence (identity in
/// the root).
fn from_world<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    picked: &[&OccurrencePath],
) -> Result<Transform, String> {
    let component = ctx.component();
    let through = picked
        .iter()
        .find_map(|path| frame_path(ctx.env.assembly, component, &path.0));
    let path = crate::links::first_path(ctx.env.assembly, component, through)?;
    Ok(inverse(&placement(ctx, &OccurrencePath(path))?))
}

/// The part of an occurrence path (absolute, from the root) that places
/// `component`, when the path passes through an occurrence of it (a point
/// of a part picked through the second occurrence of its parent, the
/// feature's component: that occurrence); none for the root and for a
/// path outside the component (mitcad#2305).
fn frame_path<'a>(
    assembly: &Assembly,
    component: ComponentUid,
    path: &'a [OccurrenceUid],
) -> Option<&'a [OccurrenceUid]> {
    if component == ComponentUid::ROOT {
        return None;
    }
    (1..=path.len()).map(|n| &path[..n]).find(|prefix| {
        crate::joints::path_component(assembly, ComponentUid::ROOT, prefix) == Ok(component)
    })
}

/// A move's point in the feature's coordinates: picked in an occurrence,
/// where the occurrence places it now (mitcad#699), taken through the
/// feature's component's occurrence on the move's picked paths (`picked`,
/// [`from_world`]); else as the feature's component has it.
fn placed_point<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    point: &GeomRef,
    occurrence: Option<&OccurrencePath>,
    picked: &[&OccurrencePath],
) -> Result<Vec3, String> {
    match occurrence {
        None => ctx.point(point),
        Some(path) => {
            let from_world = from_world(ctx, picked)?;
            point_in(ctx, point, path, &from_world)
        }
    }
}

/// A move's axis in the feature's coordinates, as [`placed_point`].
fn placed_axis<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    axis: &GeomRef,
    occurrence: Option<&OccurrencePath>,
    picked: &[&OccurrencePath],
) -> Result<DatumAxis, String> {
    match occurrence {
        None => ctx.axis(axis),
        Some(path) => {
            let from_world = from_world(ctx, picked)?;
            axis_in(ctx, axis, path, &from_world)
        }
    }
}

/// A free move's matrix as a transform; it must be a rotation and a
/// translation.
fn free_transform(matrix: &[[f64; 4]; 3]) -> Result<Transform, String> {
    let transform = Transform {
        linear: std::array::from_fn(|r| std::array::from_fn(|c| matrix[r][c])),
        translation: std::array::from_fn(|r| matrix[r][3]),
    };
    if transform.is_finite() && transform.is_rigid() {
        Ok(transform)
    } else {
        Err("a free move must be a rotation and a translation".to_owned())
    }
}

/// Checks a list of bodies to move, scale or align.
pub(crate) fn check_bodies(ctx: &CheckContext<'_>, bodies: &[BodyUid]) -> Result<(), String> {
    if bodies.is_empty() {
        return Err("no bodies selected".to_owned());
    }
    for (i, body) in bodies.iter().enumerate() {
        if bodies[..i].contains(body) {
            return Err(format!("body {body} is listed more than once"));
        }
        ctx.body(*body)?;
    }
    Ok(())
}

/// Moves the bodies, or copies them with `copy`: the output shared by
/// moves, alignments and scales.
pub(crate) fn place_bodies<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    bodies: &[BodyUid],
    transform: &Transform,
    copy: bool,
) -> Result<FeatureOutput<K::Shape>, String> {
    let mut changes = Vec::new();
    let mut sheets = std::collections::BTreeMap::new();
    for (j, body) in bodies.iter().enumerate() {
        let shape = ctx.body(*body)?;
        let instance = copy.then_some(Instance {
            feature: ctx.uid,
            index: 1,
        });
        // A rigid motion moves the body as a location on its own shape, its
        // faces keeping their display meshes (mitcad#838); a scale copies.
        let moved = if transform.is_rigid() {
            ctx.kernel.move_shape(&shape, transform, instance)
        } else {
            ctx.kernel.transform_shape(&shape, transform, instance)
        }
        .map_err(|e| e.to_string())?;
        let uid = if copy {
            BodyUid::new(ctx.uid, j as u32)
        } else {
            *body
        };
        changes.push(BodyChange::Set(uid, moved));
        if !copy && transform.is_rigid() {
            sheets.insert(uid, crate::sheet::SheetChange::Transform(*transform));
        }
    }
    Ok(FeatureOutput {
        sheets,
        changes,
        ..FeatureOutput::default()
    })
}

impl FeatureInfo for MoveDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        self.transform.add_references(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_bodies(ctx, &self.bodies)?;
        self.transform.check(ctx)
    }

    fn creates_bodies(&self) -> bool {
        self.copy
    }
}

impl<K: Kernel> Evaluate<K> for MoveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let transform = self.transform.resolve(ctx, None)?;
        place_bodies(ctx, &self.bodies, &transform, self.copy)
    }
}
