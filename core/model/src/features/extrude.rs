// SPDX-License-Identifier: MIT
//! Extrude: profiles of a sketch swept into a tool body that becomes new
//! bodies or joins, cuts or intersects participant bodies, with these
//! options: two sides, symmetric extents, extents up to an object or
//! through all, a start offset or start object, tapers and thin walls.
//!
//! The operations and profile handling here are shared by the other profile
//! features (revolve, hole).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::base::apply_tool;
use super::reference::ExtentObject;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    SketchOutput, ToolUse, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{
    BooleanOp, Bound, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, Kernel, SideEnd, ThinWall,
    WallLocation,
};
use crate::parameters::ParamId;
use crate::profile::{ProfileRegion, dot};
use crate::topo::RegionKey;

/// A profile region of a sketch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRef {
    pub sketch: FeatureUid,
    pub region: RegionKey,
}

/// Where the extrusion starts (the start extent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Start<P = ParamId> {
    /// The sketch plane.
    ProfilePlane,
    /// The profile moved `offset` along the sketch normal (not reversed by
    /// `flip`); the extent is measured from there.
    Offset { offset: P },
    /// The profile projected along the sketch normal onto a plane or a
    /// planar face, moved `offset` along its normal.
    Object {
        object: Box<ExtentObject>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        offset: Option<P>,
    },
}

// Not derived: that would need P: Default.
#[allow(clippy::derivable_impls)]
impl<P> Default for Start<P> {
    fn default() -> Self {
        Self::ProfilePlane
    }
}

impl<P> Start<P> {
    pub fn is_profile_plane(&self) -> bool {
        matches!(self, Self::ProfilePlane)
    }
}

/// How far the extrusion goes, along the sketch normal (reversed by
/// `flip`). Tapers are angles in radians; a positive taper widens the
/// profile away from the start, a negative one narrows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Extent<P = ParamId> {
    /// One side; a negative distance goes the other way.
    Distance {
        distance: P,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        taper: Option<P>,
    },
    /// One side, up to an object (moved by `offset`; positive is longer).
    ToObject {
        object: Box<ExtentObject>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        offset: Option<P>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        taper: Option<P>,
    },
    /// Through all participant bodies (all bodies if none are listed), one
    /// way or both.
    ThroughAll {
        #[serde(default, skip_serializing_if = "is_false")]
        both_sides: bool,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        taper: Option<P>,
    },
    /// Both ways from the start, `distance` to each side, or in total with
    /// `full_length` (`isFullLength` in .f3d designs); the same taper both ways.
    Symmetric {
        distance: P,
        #[serde(default, skip_serializing_if = "is_false")]
        full_length: bool,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        taper: Option<P>,
    },
    /// Side one along the direction, side two against it, each with its own
    /// extent and taper.
    TwoSides { side1: Side<P>, side2: Side<P> },
}

/// The extent of one side of a two-sided extrusion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Side<P = ParamId> {
    Distance {
        distance: P,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        taper: Option<P>,
    },
    ToObject {
        object: Box<ExtentObject>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        offset: Option<P>,
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        taper: Option<P>,
    },
    ThroughAll {
        #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
        taper: Option<P>,
    },
}

/// A thin extrusion: walls along the profile curves instead of the regions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thin<P = ParamId> {
    /// Side 1 lies away from the region's material (outside the outer
    /// boundary, into holes), side 2 towards it.
    pub location: WallLocation,
    pub thickness: P,
    /// The wall of side two of a two-sided extent when it differs.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub side2: Option<ThinSide<P>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThinSide<P = ParamId> {
    pub location: WallLocation,
    pub thickness: P,
}

/// The Operation setting. A new component makes new bodies in a new
/// component placed in the feature's own (F6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    NewBody,
    Join,
    Cut,
    Intersect,
    NewComponent,
}

impl Operation {
    pub fn makes_bodies(self) -> bool {
        matches!(self, Self::NewBody | Self::NewComponent)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtrudeDef<P = ParamId> {
    /// Regions of one sketch.
    pub profiles: Vec<ProfileRef>,
    #[serde(
        default = "Default::default",
        skip_serializing_if = "Start::is_profile_plane"
    )]
    pub start: Start<P>,
    pub extent: Extent<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thin: Option<Thin<P>>,
    /// Reverses the direction (the sketch normal by default).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    pub operation: Operation,
    /// The bodies a Join, Cut or Intersect works on; empty for all bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
}

type Mapper<'a, P, Q, E> = dyn FnMut(&str, &P) -> Result<Q, E> + 'a;

fn map_option<P, Q, E>(
    slot: &str,
    value: &Option<P>,
    f: &mut Mapper<'_, P, Q, E>,
) -> Result<Option<Q>, E> {
    value.as_ref().map(|v| f(slot, v)).transpose()
}

impl<P> Side<P> {
    fn map_params<Q, E>(&self, prefix: &str, f: &mut Mapper<'_, P, Q, E>) -> Result<Side<Q>, E> {
        let slot = |field: &str| format!("{prefix}.{field}");
        Ok(match self {
            Self::Distance { distance, taper } => Side::Distance {
                distance: f(&slot("distance"), distance)?,
                taper: map_option(&slot("taper"), taper, f)?,
            },
            Self::ToObject {
                object,
                offset,
                taper,
            } => Side::ToObject {
                object: object.clone(),
                offset: map_option(&slot("offset"), offset, f)?,
                taper: map_option(&slot("taper"), taper, f)?,
            },
            Self::ThroughAll { taper } => Side::ThroughAll {
                taper: map_option(&slot("taper"), taper, f)?,
            },
        })
    }
}

impl<P> ExtrudeDef<P> {
    pub const TYPE: &'static str = "extrude";
    pub const BASE_NAME: &'static str = "Extrude";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<ExtrudeDef<Q>, E> {
        let start = match &self.start {
            Start::ProfilePlane => Start::ProfilePlane,
            Start::Offset { offset } => Start::Offset {
                offset: f("start.offset", offset)?,
            },
            Start::Object { object, offset } => Start::Object {
                object: object.clone(),
                offset: map_option("start.offset", offset, f)?,
            },
        };
        let extent = match &self.extent {
            Extent::Distance { distance, taper } => Extent::Distance {
                distance: f("extent.distance", distance)?,
                taper: map_option("extent.taper", taper, f)?,
            },
            Extent::ToObject {
                object,
                offset,
                taper,
            } => Extent::ToObject {
                object: object.clone(),
                offset: map_option("extent.offset", offset, f)?,
                taper: map_option("extent.taper", taper, f)?,
            },
            Extent::ThroughAll { both_sides, taper } => Extent::ThroughAll {
                both_sides: *both_sides,
                taper: map_option("extent.taper", taper, f)?,
            },
            Extent::Symmetric {
                distance,
                full_length,
                taper,
            } => Extent::Symmetric {
                distance: f("extent.distance", distance)?,
                full_length: *full_length,
                taper: map_option("extent.taper", taper, f)?,
            },
            Extent::TwoSides { side1, side2 } => Extent::TwoSides {
                side1: side1.map_params("extent.side1", f)?,
                side2: side2.map_params("extent.side2", f)?,
            },
        };
        let thin = match &self.thin {
            None => None,
            Some(thin) => Some(Thin {
                location: thin.location,
                thickness: f("thin.thickness", &thin.thickness)?,
                side2: match &thin.side2 {
                    None => None,
                    Some(side) => Some(ThinSide {
                        location: side.location,
                        thickness: f("thin.side2.thickness", &side.thickness)?,
                    }),
                },
            }),
        };
        Ok(ExtrudeDef {
            profiles: self.profiles.clone(),
            start,
            extent,
            thin,
            flip: self.flip,
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }

    /// The sketch of the profiles.
    pub fn sketch(&self) -> Option<FeatureUid> {
        self.profiles.first().map(|p| p.sketch)
    }

    /// The objects the extrusion starts from or runs to.
    fn objects(&self) -> Vec<&ExtentObject> {
        let mut objects: Vec<&ExtentObject> = Vec::new();
        if let Start::Object { object, .. } = &self.start {
            objects.push(object);
        }
        match &self.extent {
            Extent::ToObject { object, .. } => objects.push(object),
            Extent::TwoSides { side1, side2 } => {
                for side in [side1, side2] {
                    if let Side::ToObject { object, .. } = side {
                        objects.push(object);
                    }
                }
            }
            _ => {}
        }
        objects
    }
}

/// The references of profiles: their sketch.
pub(crate) fn profile_references(profiles: &[ProfileRef], references: &mut References) {
    references
        .features
        .extend(profiles.iter().map(|p| p.sketch));
}

/// Checks that the profiles are distinct regions of one sketch before the
/// feature. Region keys depend on the solved geometry, so evaluation finds
/// them (or the best match when the sketch changed, see
/// [`SketchOutput::resolve_region`]).
pub(crate) fn check_profiles(
    ctx: &CheckContext<'_>,
    profiles: &[ProfileRef],
) -> Result<(), String> {
    let Some(first) = profiles.first() else {
        return Err("no profiles selected".to_owned());
    };
    ctx.sketch(first.sketch)?;
    for (i, profile) in profiles.iter().enumerate() {
        if profile.sketch != first.sketch {
            return Err("the profiles must come from one sketch".to_owned());
        }
        if profiles[..i].contains(profile) {
            return Err(format!(
                "profile {} is listed more than once",
                profile.region
            ));
        }
    }
    Ok(())
}

/// Checks the participant bodies of an operation.
pub(crate) fn check_participants(
    ctx: &CheckContext<'_>,
    operation: Operation,
    participants: &[BodyUid],
) -> Result<(), String> {
    if operation.makes_bodies() && !participants.is_empty() {
        return Err("a new body has no participant bodies".to_owned());
    }
    for (i, body) in participants.iter().enumerate() {
        if participants[..i].contains(body) {
            return Err(format!("body {body} is listed more than once"));
        }
        ctx.body(*body)?;
    }
    Ok(())
}

/// The evaluated sketch of the profiles and their regions: each by its key,
/// or the best match when the sketch changed (it keeps the referenced key,
/// so the faces made from it keep their names). Two references that now
/// find the same region use it once.
pub(crate) fn profile_regions<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    profiles: &[ProfileRef],
) -> Result<(Arc<SketchOutput>, Vec<ProfileRegion>), String> {
    let sketch_uid = profiles.first().ok_or("no profiles selected")?.sketch;
    let sketch = ctx.sketch(sketch_uid)?;
    let mut regions: Vec<ProfileRegion> = Vec::new();
    for p in profiles {
        let region = sketch.resolve_region(&p.region).ok_or_else(|| {
            format!(
                "{} has no profile {}",
                ctx.feature_name(sketch_uid),
                p.region
            )
        })?;
        if !regions.iter().any(|r| r.loops == region.loops) {
            regions.push(region);
        }
    }
    Ok((sketch, regions))
}

/// The bodies an operation works on: none for new bodies, the listed ones,
/// or all.
pub(crate) fn participant_bodies<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    operation: Operation,
    participants: &[BodyUid],
) -> Result<Vec<(BodyUid, K::Shape)>, String> {
    if operation.makes_bodies() {
        Ok(Vec::new())
    } else if participants.is_empty() {
        Ok(ctx.bodies())
    } else {
        participants
            .iter()
            .map(|uid| ctx.body(*uid).map(|shape| (*uid, shape)))
            .collect()
    }
}

/// Applies the operation with the tool to the participants (see
/// `base::apply_tool`). `what` names the tool in errors
/// ("extrusion").
pub(crate) fn apply_operation<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    operation: Operation,
    participants: Vec<(BodyUid, K::Shape)>,
    tool: K::Shape,
    what: &str,
) -> Result<FeatureOutput<K::Shape>, String> {
    let mut output = FeatureOutput {
        tool: Some(tool.clone()),
        ..FeatureOutput::default()
    };
    let op = match operation {
        Operation::NewBody | Operation::NewComponent => {
            for solid in ctx.kernel.solids(&tool).map_err(|e| e.to_string())? {
                output.changes.push(BodyChange::Set(ctx.new_body(), solid));
            }
            return Ok(output);
        }
        Operation::Join => BooleanOp::Join,
        Operation::Cut => BooleanOp::Cut,
        Operation::Intersect => BooleanOp::Intersect,
    };
    output.changes = apply_tool(ctx, op, &participants, &tool, what)?;
    Ok(output)
}

impl FeatureInfo for ExtrudeDef {
    fn references(&self) -> References {
        let mut references = References::default();
        profile_references(&self.profiles, &mut references);
        for body in &self.participants {
            references.body(*body);
        }
        for object in self.objects() {
            object.add_references(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_profiles(ctx, &self.profiles)?;
        check_participants(ctx, self.operation, &self.participants)?;
        if let Start::Object { object, .. } = &self.start
            && !object.is_planar()
        {
            return Err("an extrusion starts from a plane or a planar face, not a body".to_owned());
        }
        if let Some(thin) = &self.thin
            && thin.side2.is_some()
            && !matches!(
                self.extent,
                Extent::TwoSides { .. } | Extent::Symmetric { .. }
            )
        {
            return Err("only an extrusion to two sides has a second wall".to_owned());
        }
        for object in self.objects() {
            object.check(ctx)?;
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        match &self.extent {
            Extent::Distance { distance, .. } if value(*distance) == 0.0 => {
                return Err("extrude distance must not be zero".to_owned());
            }
            Extent::Symmetric { distance, .. } if !is_positive(value(*distance)) => {
                return Err(format!(
                    "extrude distance must be greater than zero, got {}",
                    value(*distance)
                ));
            }
            Extent::TwoSides { side1, side2 } => {
                for side in [side1, side2] {
                    if let Side::Distance { distance, .. } = side
                        && value(*distance) < 0.0
                    {
                        return Err(format!(
                            "the distances of a two-sided extrusion must not be negative, got {}",
                            value(*distance)
                        ));
                    }
                }
                if let (Side::Distance { distance: d1, .. }, Side::Distance { distance: d2, .. }) =
                    (side1, side2)
                    && !is_positive(value(*d1) + value(*d2))
                {
                    return Err(
                        "the two extrude distances must add up to more than zero".to_owned()
                    );
                }
            }
            _ => {}
        }
        if let Some(thin) = &self.thin {
            let walls = [
                Some(thin.thickness),
                thin.side2.as_ref().map(|s| s.thickness),
            ];
            for thickness in walls.into_iter().flatten() {
                if !is_positive(value(thickness)) {
                    return Err(format!(
                        "the wall thickness must be greater than zero, got {}",
                        value(thickness)
                    ));
                }
            }
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        // New bodies, tool pieces that join nothing, and pieces of split bodies.
        true
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }

    fn profiles(&self) -> &[ProfileRef] {
        &self.profiles
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }
}

/// One side as evaluation sees it, and its taper.
type SideOf<'a> = (SideView<'a>, Option<ParamId>);

/// One side as evaluation sees it.
enum SideView<'a> {
    Distance(ParamId),
    ToObject(&'a ExtentObject, Option<ParamId>),
    ThroughAll,
}

impl<'a> SideView<'a> {
    fn of(side: &'a Side) -> (Self, Option<ParamId>) {
        match side {
            Side::Distance { distance, taper } => (Self::Distance(*distance), *taper),
            Side::ToObject {
                object,
                offset,
                taper,
            } => (Self::ToObject(object, *offset), *taper),
            Side::ThroughAll { taper } => (Self::ThroughAll, *taper),
        }
    }
}

impl<K: Kernel> Evaluate<K> for ExtrudeDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (sketch, regions) = profile_regions(ctx, &self.profiles)?;
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let frame = sketch.frame;
        let normal = frame.normal();
        let mut direction = if self.flip {
            normal.map(|v| -v)
        } else {
            normal
        };

        let (start, start_offset) = match &self.start {
            Start::ProfilePlane => (ExtrudeStart::Offset(0.0), 0.0),
            Start::Offset { offset } => {
                let offset = ctx.param(*offset)?;
                (ExtrudeStart::Offset(offset), offset)
            }
            Start::Object { object, offset } => {
                let offset = offset.map(|id| ctx.param(id)).transpose()?.unwrap_or(0.0);
                let target = object.resolve(ctx)?;
                (ExtrudeStart::Object(Bound { target, offset }), 0.0)
            }
        };

        // The sides: (extent, taper) of side one and maybe side two.
        let (one, two): (SideOf<'_>, Option<SideOf<'_>>) = match &self.extent {
            Extent::Distance { distance, taper } => ((SideView::Distance(*distance), *taper), None),
            Extent::ToObject {
                object,
                offset,
                taper,
            } => ((SideView::ToObject(object, *offset), *taper), None),
            Extent::ThroughAll { both_sides, taper } => (
                (SideView::ThroughAll, *taper),
                both_sides.then_some((SideView::ThroughAll, *taper)),
            ),
            Extent::Symmetric {
                distance, taper, ..
            } => (
                (SideView::Distance(*distance), *taper),
                Some((SideView::Distance(*distance), *taper)),
            ),
            Extent::TwoSides { side1, side2 } => (SideView::of(side1), Some(SideView::of(side2))),
        };
        let two_sides = matches!(self.extent, Extent::TwoSides { .. });
        // A one-sided distance may be negative: it reverses the direction.
        if let (SideView::Distance(distance), None) = (&one.0, &two)
            && ctx.param(*distance)? < 0.0
        {
            direction = direction.map(|v| -v);
        }
        let walls = match &self.thin {
            None => (None, None),
            Some(thin) => {
                let first = ThinWall {
                    location: thin.location,
                    thickness: ctx.positive(thin.thickness)?,
                };
                let second = match &thin.side2 {
                    None => first,
                    Some(side) => ThinWall {
                        location: side.location,
                        thickness: ctx.positive(side.thickness)?,
                    },
                };
                (Some(first), Some(second))
            }
        };

        let origin: [f64; 3] = std::array::from_fn(|i| frame.origin[i] + start_offset * normal[i]);
        let mut sides = Vec::new();
        for (k, (view, taper)) in [Some(one), two].into_iter().flatten().enumerate() {
            let along = if k == 0 {
                direction
            } else {
                direction.map(|v| -v)
            };
            let end = match view {
                SideView::Distance(id) => {
                    let d = match &self.extent {
                        Extent::Symmetric { full_length, .. } => {
                            let d = ctx.positive(id)?;
                            if *full_length { d / 2.0 } else { d }
                        }
                        Extent::TwoSides { .. } => {
                            let d = ctx.param(id)?;
                            if d < 0.0 {
                                return Err(format!(
                                    "{} must not be negative, got {d}",
                                    ctx.param_name(id)
                                ));
                            }
                            d
                        }
                        _ => ctx.param(id)?.abs(),
                    };
                    if d == 0.0 && !two_sides {
                        return Err(format!("{} must not be zero", ctx.param_name(id)));
                    }
                    SideEnd::Distance(d)
                }
                SideView::ToObject(object, offset) => {
                    let offset = offset.map(|id| ctx.param(id)).transpose()?.unwrap_or(0.0);
                    SideEnd::Target(Bound {
                        target: object.resolve(ctx)?,
                        offset,
                    })
                }
                SideView::ThroughAll => {
                    let bodies = if participants.is_empty() {
                        ctx.bodies()
                    } else {
                        participants.clone()
                    };
                    // Both ways through all: the same length each way.
                    let both = matches!(
                        self.extent,
                        Extent::ThroughAll {
                            both_sides: true,
                            ..
                        }
                    );
                    SideEnd::Distance(through_all_length(
                        ctx.kernel, origin, along, &bodies, both,
                    )?)
                }
            };
            let taper = taper.map(|id| ctx.param(id)).transpose()?.unwrap_or(0.0);
            sides.push(ExtrudeSide {
                end,
                taper,
                thin: if k == 0 { walls.0 } else { walls.1 },
            });
        }
        // A side of length zero leaves a one-sided extrusion.
        let zero =
            |side: &ExtrudeSide<K::Shape>| matches!(side.end, SideEnd::Distance(d) if d == 0.0);
        let mut sides = sides.into_iter();
        let mut side1 = sides.next().expect("side one");
        let mut side2 = sides.next();
        if side2.as_ref().is_some_and(zero) {
            side2 = None;
        } else if side2.is_some() && zero(&side1) {
            side1 = side2.take().expect("side two");
            direction = direction.map(|v| -v);
        }
        if zero(&side1) {
            return Err("the two extrude distances must add up to more than zero".to_owned());
        }

        let spec = ExtrudeFeatureSpec {
            feature: ctx.uid,
            frame,
            regions: &regions,
            direction,
            start,
            side1,
            side2,
        };
        let tool = ctx
            .kernel
            .extrude_feature(&spec)
            .map_err(|e| e.to_string())?;
        apply_operation(ctx, self.operation, participants, tool, "extrusion")
    }
}

/// A length along `direction` from `origin` that reaches past every body
/// (both ways with `both`).
fn through_all_length<K: Kernel>(
    kernel: &K,
    origin: [f64; 3],
    direction: [f64; 3],
    bodies: &[(BodyUid, K::Shape)],
    both: bool,
) -> Result<f64, String> {
    let mut reach = f64::NEG_INFINITY;
    for (_, shape) in bodies {
        let Some(bounds) = kernel.bounding_box(shape).map_err(|e| e.to_string())? else {
            continue;
        };
        for corner in bounds.corners() {
            let offset = dot(std::array::from_fn(|i| corner[i] - origin[i]), direction);
            reach = reach.max(if both { offset.abs() } else { offset });
        }
    }
    if reach == f64::NEG_INFINITY {
        return Err("there are no bodies to extrude through".to_owned());
    }
    if reach <= 0.0 {
        return Err("there are no bodies in the extrude direction".to_owned());
    }
    // Past the far side, so the cap is outside every body.
    Ok(reach * 1.25 + 1.0)
}

/// False for zero, negative and NaN values.
pub(crate) fn is_positive(value: f64) -> bool {
    value > 0.0
}
