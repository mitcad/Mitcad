// SPDX-License-Identifier: MIT
//! Extrude: profiles of a sketch swept into a tool body that becomes new
//! bodies or joins, cuts or intersects participant bodies, with these
//! options: two sides, symmetric extents, extents up to an object or
//! through all, a start offset or start object, tapers and thin walls.
//!
//! The operations and profile handling here are shared by the other profile
//! features (revolve, hole).

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::base::apply_tool;
use super::path::{self, SketchCurves};
use super::reference::{Across, ExtentObject};
// Surface extrusions (mitcad#455).
use super::surface_sweeps;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    SketchOutput, ToolUse, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{
    BooleanOp, Bound, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, FaceExtrudeSpec, Kernel,
    SideEnd, ThinWall, WallLocation,
};
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use crate::profile::ProfileRegion;
use crate::topo::RegionKey;
use crate::transform::Transform;
use mitcad_vec3::dot;

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
    ///
    /// With `signed` (mitcad#765: what the Extrude panel makes) a side's
    /// distance has a sign: side one ends `d1` along the direction and side
    /// two `d2` against it, so a negative distance ends on the other side's
    /// side and the solid lies between the two ends only (50 and -30: from
    /// 30 to 50 along the direction, not touching the start). The ends must
    /// not meet or cross (`d1 + d2 > 0`). The side that holds the solid
    /// keeps its taper, measured from the start as without the other side's
    /// end; the other side's taper has nothing to shape. Without `signed`
    /// (designs saved before, the imports' features) each side goes its own
    /// way by its distance's size, as the stored designs measure it.
    TwoSides {
        side1: Side<P>,
        side2: Side<P>,
        #[serde(default, skip_serializing_if = "is_false")]
        signed: bool,
    },
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
    /// boundary, into holes), side 2 towards it. Along `curves`, side 1
    /// lies outside a closed chain and to the right of an open one (see
    /// [`path::wall_chains`] for which way it runs), side 2 inside or to
    /// its left.
    pub location: WallLocation,
    /// The thickness (on side 1 for a wall on both sides).
    pub thickness: P,
    /// The thickness on side 2 of a wall on both sides (`both`).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thickness2: Option<P>,
    /// The wall of side two of a two-sided extent when it differs.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub side2: Option<ThinSide<P>>,
    /// Walls along these sketch curves, open chains or closed loops,
    /// instead of along the profiles' boundaries (mitcad#463); the
    /// extrusion then has no profiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves: Option<SketchCurves>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThinSide<P = ParamId> {
    pub location: WallLocation,
    pub thickness: P,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thickness2: Option<P>,
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
    /// Regions of one sketch, a set (see `selected_regions`); none for
    /// thin walls along curves.
    #[serde(
        default,
        deserialize_with = "profile_set",
        skip_serializing_if = "Vec::is_empty"
    )]
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
    /// Cut targets; retained as dormant editor choices for other operations.
    /// None preserves the legacy component-local participant behavior exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_scope: Option<super::extrude_scope::CutScope>,
    /// Bodies of other components that the extrusion's objects are or are
    /// faces of (mitcad#104, as a combine's `tool_links`): each is read in
    /// its component and moved into the extrusion's coordinates by the
    /// placements at its point of the timeline.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_links: BTreeMap<BodyUid, OccurrenceLink>,
    /// A surface extrusion (mitcad#455, `surface_sweeps.rs`): walls without
    /// caps, a new surface body; not thin.
    #[serde(default, skip_serializing_if = "is_false")]
    pub surface: bool,
    /// Sketch curves a surface extrusion sweeps beside or instead of the
    /// profiles: open chains or closed loops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves: Option<SketchCurves>,
    /// The profiles wrapped onto this cylindrical or conical face (mitcad#473),
    /// as an emboss wraps them, and grown along its normals by the distance:
    /// out of the material, or into it with `flip` or a negative distance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<super::reference::FaceRef>,
    /// A planar face of a body as the profile instead of sketch regions
    /// (mitcad#535): its bounded area, holes kept, swept along its outward
    /// normal (reversed by `flip` or a negative distance), found by its
    /// topological name on every recompute. Distances only, with their
    /// tapers, along `direction` when given (mitcad#541; a curved face only
    /// along a direction, without a taper or offset); from the face or an
    /// offset from it; no thin walls, surface, curves, wrap or `cut_scope`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_face: Option<super::reference::FaceRef>,
    /// The direction the profiles are extruded along instead of the sketch
    /// normal (mitcad#539): a fixed vector of any length but zero, not in
    /// the sketch plane, reversed by `flip`. Distances are measured along
    /// it; a taper grows the profile in its plane by `d tan(taper)` at
    /// distance `d`. Not with a start object or wrap; with a face profile
    /// (mitcad#541) not in the face's plane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f64; 3]>,
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
    // Written by hand: the side maps by its kind under its prefix.
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

    /// Saved targets have no effect until the operation is Cut again.
    pub(crate) fn active_cut_scope(&self) -> Option<&super::extrude_scope::CutScope> {
        (self.operation == Operation::Cut)
            .then_some(self.cut_scope.as_ref())
            .flatten()
    }

    // Written by hand: start, extent and thin map by their kind (`extent.distance`).
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
            Extent::TwoSides {
                side1,
                side2,
                signed,
            } => Extent::TwoSides {
                side1: side1.map_params("extent.side1", f)?,
                side2: side2.map_params("extent.side2", f)?,
                signed: *signed,
            },
        };
        let thin = match &self.thin {
            None => None,
            Some(thin) => Some(Thin {
                location: thin.location,
                thickness: f("thin.thickness", &thin.thickness)?,
                thickness2: map_option("thin.thickness2", &thin.thickness2, f)?,
                side2: match &thin.side2 {
                    None => None,
                    Some(side) => Some(ThinSide {
                        location: side.location,
                        thickness: f("thin.side2.thickness", &side.thickness)?,
                        thickness2: map_option("thin.side2.thickness2", &side.thickness2, f)?,
                    }),
                },
                curves: thin.curves.clone(),
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
            cut_scope: self.cut_scope.clone(),
            object_links: self.object_links.clone(),
            surface: self.surface,
            curves: self.curves.clone(),
            wrap: self.wrap.clone(),
            profile_face: self.profile_face.clone(),
            direction: self.direction,
        })
    }

    /// The sketch of the profiles (or of the curves thin walls follow or a
    /// surface extrusion sweeps); none for a body face's extrusion.
    pub fn sketch(&self) -> Option<FeatureUid> {
        self.profiles
            .first()
            .map(|p| p.sketch)
            .or_else(|| self.wall_curves().map(|c| c.sketch))
            .or_else(|| self.curves.as_ref().map(|c| c.sketch))
    }

    /// The curves thin walls follow instead of the profiles' boundaries.
    pub fn wall_curves(&self) -> Option<&SketchCurves> {
        self.thin.as_ref().and_then(|t| t.curves.as_ref())
    }

    /// The distances of a two-sided extent, side one's and side two's: none
    /// for a side up to an object or through all, nor for other extents.
    pub fn two_side_distances(&self) -> [Option<&P>; 2] {
        fn distance<P>(side: &Side<P>) -> Option<&P> {
            match side {
                Side::Distance { distance, .. } => Some(distance),
                _ => None,
            }
        }
        match &self.extent {
            Extent::TwoSides { side1, side2, .. } => [distance(side1), distance(side2)],
            _ => [None, None],
        }
    }

    /// Whether the extent is two-sided with signed distances (mitcad#765).
    pub fn signed_two_sides(&self) -> bool {
        matches!(self.extent, Extent::TwoSides { signed: true, .. })
    }

    /// The objects the extrusion starts from or runs to.
    fn objects(&self) -> Vec<&ExtentObject> {
        let mut objects: Vec<&ExtentObject> = Vec::new();
        if let Start::Object { object, .. } = &self.start {
            objects.push(object);
        }
        match &self.extent {
            Extent::ToObject { object, .. } => objects.push(object),
            Extent::TwoSides { side1, side2, .. } => {
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

/// Profile references as the set they stand for (mitcad#199): a reference
/// listed again adds nothing and is dropped, the first one kept in its
/// place.
pub(crate) fn distinct_profiles(profiles: Vec<ProfileRef>) -> Vec<ProfileRef> {
    let mut distinct: Vec<ProfileRef> = Vec::new();
    for profile in profiles {
        if !distinct.contains(&profile) {
            distinct.push(profile);
        }
    }
    distinct
}

/// Reads the `profiles` of a definition as a set ([`distinct_profiles`]):
/// a list with a reference twice, as selection plumbing could give, is the
/// feature it means and not an invalid one.
pub(crate) fn profile_set<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ProfileRef>, D::Error> {
    Vec::<ProfileRef>::deserialize(deserializer).map(distinct_profiles)
}

/// Checks that the profiles are regions of one sketch before the feature.
/// Region keys depend on the solved geometry, so evaluation finds them (or
/// the best match when the sketch changed, see
/// [`SketchOutput::resolve_region`]). A region referenced more than once is
/// no error: the references are a set (`selected_regions`).
pub(crate) fn check_profiles(
    ctx: &CheckContext<'_>,
    profiles: &[ProfileRef],
) -> Result<(), String> {
    let Some(first) = profiles.first() else {
        return Err("no profiles selected".to_owned());
    };
    ctx.sketch(first.sketch)?;
    if profiles.iter().any(|p| p.sketch != first.sketch) {
        return Err("the profiles must come from one sketch".to_owned());
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

/// The area profile references select in an evaluated sketch (mitcad#199),
/// as the regions handed to the kernel; preview and evaluation both use it.
///
/// A selectable area is one profile region: a face of the planar
/// arrangement of the sketch's curves, with its holes, as the view shows
/// and highlights it (a ring between two circles is one region, the disc
/// inside it another). The selected area is the odd/even (XOR) composition
/// of the selected regions: covered by an odd number of them it is
/// extruded. Regions of one sketch never overlap, so that is each selected
/// region once, in the order first listed, whatever the order or
/// repetition of the list; a hole of a region stays a hole unless the
/// region filling it is selected too (then the two fuse, as touching
/// regions do). Toggling is the application's: a click on a selected
/// region takes its reference out instead of listing it again.
///
/// Each reference finds its regions by its key; when the sketch changed,
/// the regions inside the outline its key traces now (the region renamed,
/// or all its pieces when the change split it), else the best match (see
/// [`SketchOutput::found`]; the main region keeps the referenced key, so
/// the faces made from it keep their names). Two references that find the
/// same region, such as a key listed twice or an old key and the current
/// one, use it once. Err: the first reference that finds no region.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub fn selected_regions<'a>(
    sketch: &SketchOutput,
    profiles: &'a [ProfileRef],
) -> Result<Vec<ProfileRegion>, &'a ProfileRef> {
    found_regions(sketch, profiles, "").map(|(regions, _)| regions)
}

/// The regions of `found_regions` and the warnings of the references found
/// under other keys, each with the reference it is about.
pub(crate) type FoundRegionsOf<'a> = (Vec<ProfileRegion>, Vec<(String, &'a ProfileRef)>);

/// `selected_regions` with the warnings of the references found under
/// other keys (`FoundRegions::warning`, for the sketch named `name`), each
/// with the reference it is about.
pub(crate) fn found_regions<'a>(
    sketch: &SketchOutput,
    profiles: &'a [ProfileRef],
    name: &str,
) -> Result<FoundRegionsOf<'a>, &'a ProfileRef> {
    let mut regions: Vec<ProfileRegion> = Vec::new();
    let mut warnings = Vec::new();
    for p in profiles {
        let found = sketch.found(&p.region).ok_or(p)?;
        warnings.extend(found.warning(&p.region, name).map(|w| (w, p)));
        for region in found.profiles(&p.region) {
            if !regions.iter().any(|r| r.loops == region.loops) {
                regions.push(region);
            }
        }
    }
    Ok((regions, warnings))
}

/// The evaluated sketch of the profiles and the regions they select
/// (`selected_regions`). A reference found under other keys warns
/// (`region r{…} found as r{…} + r{…} after a change in Sketch1`), as
/// renamed faces do (mitcad#272, mitcad#407); its notice has the regions
/// as places of the sketch (mitcad#1334).
pub(crate) fn profile_regions<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    profiles: &[ProfileRef],
) -> Result<(Arc<SketchOutput>, Vec<ProfileRegion>), String> {
    let sketch_uid = profiles.first().ok_or("no profiles selected")?.sketch;
    let sketch = ctx.sketch(sketch_uid)?;
    let name = ctx.feature_name(sketch_uid);
    let (regions, warnings) = found_regions(&sketch, profiles, &name)
        .map_err(|p| format!("{name} has no profile {}", p.region))?;
    for (warning, p) in warnings {
        let place = crate::notice::NoticePlace::region_of(p.sketch, p.region.to_string());
        ctx.warn_at(warning, vec![place]);
    }
    Ok((sketch, regions))
}

/// The bodies an operation works on: none for new bodies, the listed ones,
/// or all. Mesh bodies are left out of all bodies and refused when listed
/// (mitcad#399).
pub(crate) fn participant_bodies<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    operation: Operation,
    participants: &[BodyUid],
) -> Result<Vec<(BodyUid, K::Shape)>, String> {
    if operation.makes_bodies() {
        Ok(Vec::new())
    } else if participants.is_empty() {
        Ok(ctx.surface_bodies())
    } else {
        participants
            .iter()
            .map(|uid| ctx.surface_body(*uid).map(|shape| (*uid, shape)))
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
        if let Some(curves) = self.wall_curves() {
            curves.add_references(&mut references);
        }
        if let Some(curves) = &self.curves {
            curves.add_references(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        for object in self.objects() {
            object.add_references(&mut references);
        }
        // A cut scope's excluded bodies are no references (mitcad#1236):
        // they name what the cut leaves alone.
        if let Some(face) = &self.wrap {
            face.add_references(&mut references);
        }
        // A body face as the profile (mitcad#535): its body and the
        // features its name refers to.
        if let Some(face) = &self.profile_face {
            face.add_references(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if let Some(face) = &self.profile_face {
            check_face_profile(self, ctx, face)?;
        }
        if self.surface {
            if self.thin.is_some() {
                return Err("a surface extrusion has no thin walls".to_owned());
            }
            if self.cut_scope.is_some() {
                return Err("a surface extrusion cuts nothing".to_owned());
            }
        } else if self.curves.is_some() && self.wall_curves().is_some() {
            return Err("only a surface extrusion sweeps sketch curves".to_owned());
        }
        if let Some(face) = &self.wrap {
            // Wrapped (mitcad#473): profiles by a distance, nothing else.
            if self.wall_curves().is_some() || self.thin.is_some() || self.surface {
                return Err(
                    "a wrapped extrusion takes profiles, not thin walls or surfaces".to_owned(),
                );
            }
            if !matches!(self.extent, Extent::Distance { taper: None, .. }) {
                return Err("a wrapped extrusion goes a distance, without a taper".to_owned());
            }
            if !matches!(self.start, Start::ProfilePlane) || self.cut_scope.is_some() {
                return Err(
                    "a wrapped extrusion starts on the face it wraps onto, in its own component"
                        .to_owned(),
                );
            }
            face.check(ctx)?;
        }
        match self.wall_curves() {
            // The face is the profile (checked above).
            _ if self.profile_face.is_some() => {}
            Some(_) if !self.profiles.is_empty() => {
                return Err("thin walls along curves take no profiles".to_owned());
            }
            Some(curves) => curves.check(ctx)?,
            None => surface_sweeps::check_inputs(
                ctx,
                self.surface,
                self.operation,
                &self.profiles,
                self.curves.as_ref(),
                "extrusion",
            )?,
        }
        if let Some(scope) = self.active_cut_scope() {
            if !self.participants.is_empty() {
                return Err("cut_scope requires no legacy participant list".to_owned());
            }
            scope.check()?;
        }
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
        if let Some(thin) = &self.thin {
            let walls = [
                Some((thin.location, thin.thickness2.is_some())),
                thin.side2
                    .as_ref()
                    .map(|s| (s.location, s.thickness2.is_some())),
            ];
            for (location, second) in walls.into_iter().flatten() {
                match (location == WallLocation::Both, second) {
                    (true, false) => {
                        return Err("a wall on both sides needs thickness2".to_owned());
                    }
                    (false, true) => {
                        return Err("only a wall on both sides has thickness2".to_owned());
                    }
                    _ => {}
                }
            }
        }
        // A direction of its own (mitcad#539).
        if let Some(direction) = self.direction {
            unit_direction(direction)?;
            // A body face goes along it too (mitcad#541).
            if self.wrap.is_some() {
                return Err(
                    "a wrapped extrusion goes along the face's normals, not a direction".to_owned(),
                );
            }
            if matches!(self.start, Start::Object { .. }) {
                return Err(
                    "an extrusion along a direction starts at the sketch plane or offset from it"
                        .to_owned(),
                );
            }
        }
        for object in self.objects() {
            object.check(ctx)?;
        }
        if let Some(body) = self
            .object_links
            .keys()
            .find(|b| !self.objects().iter().any(|o| o.body() == Some(**b)))
        {
            return Err(format!("body {body} is linked but not an object's"));
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
            Extent::TwoSides {
                side1,
                side2,
                signed,
            } => {
                // Signed distances (mitcad#765) must leave a length between
                // the two ends; without `signed` the sign of a side's distance
                // does not matter (see `tool`; commands refuse a negative
                // one there, mitcad#693).
                if let (Side::Distance { distance: d1, .. }, Side::Distance { distance: d2, .. }) =
                    (side1, side2)
                {
                    let (d1, d2) = (value(*d1), value(*d2));
                    if *signed {
                        if !is_positive(d1 + d2) {
                            return Err(sides_overlap(d1, d2));
                        }
                    } else if !is_positive(d1.abs() + d2.abs()) {
                        return Err(
                            "the two extrude distances must add up to more than zero".to_owned()
                        );
                    }
                }
            }
            _ => {}
        }
        if let Some(thin) = &self.thin {
            let walls = [
                Some(thin.thickness),
                thin.thickness2,
                thin.side2.as_ref().map(|s| s.thickness),
                thin.side2.as_ref().and_then(|s| s.thickness2),
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
        // A pattern must not silently replay an assembly cut only locally.
        if self.active_cut_scope().is_some() {
            return None;
        }
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
        if self.surface {
            return self.surface_output(ctx);
        }
        if let Some(face) = &self.profile_face {
            // A body face as the profile (mitcad#535).
            let participants = participant_bodies(ctx, self.operation, &self.participants)?;
            let uid = ctx.uid;
            let tool = self.face_tool(ctx, face, uid)?;
            return apply_operation(ctx, self.operation, participants, tool, "extrusion");
        }
        let (sketch, regions) = self.sketch_regions(ctx)?;
        if let (Some(face), Extent::Distance { distance, .. }) = (&self.wrap, &self.extent) {
            // Wrapped onto a cylinder or a cone (mitcad#473), as an emboss.
            let d = ctx.param(*distance)?;
            let raise = (d > 0.0) != self.flip;
            let participants = participant_bodies(ctx, self.operation, &self.participants)?;
            let tool = super::emboss::wrapped_tool(
                ctx,
                sketch.frame,
                &regions,
                face,
                d.abs(),
                raise,
                super::emboss::WrapOffsets::default(),
            )?
            .ok_or_else(|| {
                format!(
                    "to wrap the profiles onto face {}, it must be cylindrical or conical",
                    face.face
                )
            })?;
            return apply_operation(ctx, self.operation, participants, tool, "extrusion");
        }
        let discovery = self
            .active_cut_scope()
            .map(|scope| scope.discover(ctx))
            .transpose()?;
        let through_all = matches!(
            &self.extent,
            Extent::ThroughAll { .. }
                | Extent::TwoSides {
                    side1: Side::ThroughAll { .. },
                    ..
                }
                | Extent::TwoSides {
                    side2: Side::ThroughAll { .. },
                    ..
                }
        );
        let participants = match &discovery {
            Some(discovery) if through_all => discovery.in_source_frame(ctx)?,
            Some(_) => Vec::new(),
            None => participant_bodies(ctx, self.operation, &self.participants)?,
        };
        let uid = ctx.uid;
        let tool = self.tool(
            ctx,
            &sketch,
            &regions,
            &participants,
            &Transform::IDENTITY,
            uid,
        )?;
        match (self.active_cut_scope(), discovery) {
            (Some(scope), Some(discovery)) => scope.apply(ctx, discovery, tool),
            _ => apply_operation(ctx, self.operation, participants, tool, "extrusion"),
        }
    }

    /// The tool of an extrusion whose end depends on bodies (up to an
    /// object, through all bodies) rebuilt with the profile moved by
    /// `placement`: the copies of a pattern then end on the object as the
    /// original does, where the moved tool would end where the object lies
    /// moved too. An extrusion by distances only has the moved tool.
    fn placed_tool(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        placement: &Transform,
    ) -> Option<Result<K::Shape, String>> {
        // A body face's extrusion goes distances only: its tool is moved.
        if self.surface || self.profile_face.is_some() || !self.depends_on_bodies() {
            return None;
        }
        let (sketch, regions) = match self.sketch_regions(ctx) {
            Ok(p) => p,
            Err(e) => return Some(Err(e)),
        };
        let participants = if let Some(scope) = self.active_cut_scope() {
            // Preserve Through All's assembly bounds, including excluded
            // candidates, while replay applies the tool only to the checked
            // local bodies. The pattern has already checked that boundary.
            match scope
                .discover(ctx)
                .and_then(|discovery| discovery.in_source_frame(ctx))
            {
                Ok(bodies) => bodies,
                Err(e) => return Some(Err(e)),
            }
        } else if self.operation.makes_bodies() || self.participants.is_empty() {
            match participant_bodies(ctx, self.operation, &self.participants) {
                Ok(p) => p,
                Err(e) => return Some(Err(e)),
            }
        } else {
            // A participant the original merged into another is not there
            // at the pattern; the body it went into, another participant,
            // stands for it (mitcad#406; the pattern has checked them).
            let mut present = Vec::with_capacity(self.participants.len());
            let mut missing = None;
            for uid in &self.participants {
                match ctx.body(*uid) {
                    Ok(shape) => present.push((*uid, shape)),
                    Err(e) => {
                        missing.get_or_insert(e);
                    }
                }
            }
            match missing {
                Some(e) if present.is_empty() => return Some(Err(e)),
                _ => present,
            }
        };
        // Objects the original took away are found as it found them.
        let outer = ctx.objects_before.replace(feature);
        let tool = self.tool(ctx, &sketch, &regions, &participants, placement, feature);
        ctx.objects_before = outer;
        Some(tool)
    }
}

impl ExtrudeDef {
    /// The evaluated sketch and the regions the profiles select (none for
    /// walls along curves).
    fn sketch_regions<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<(Arc<SketchOutput>, Vec<ProfileRegion>), String> {
        match self.wall_curves() {
            Some(curves) => Ok((ctx.sketch(curves.sketch)?, Vec::new())),
            None => profile_regions(ctx, &self.profiles),
        }
    }

    /// Whether the extrusion's start or an end is found from bodies.
    fn depends_on_bodies(&self) -> bool {
        let to_object = |side: &Side| !matches!(side, Side::Distance { .. });
        !self.objects().is_empty()
            || match &self.extent {
                Extent::ThroughAll { .. } => true,
                Extent::TwoSides { side1, side2, .. } => to_object(side1) || to_object(side2),
                _ => false,
            }
    }

    /// The extrusion's tool body for the sketch's frame moved by
    /// `placement`, its faces named after `feature`.
    fn tool<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        sketch: &SketchOutput,
        regions: &[ProfileRegion],
        participants: &[(BodyUid, K::Shape)],
        placement: &Transform,
        feature: FeatureUid,
    ) -> Result<K::Shape, String> {
        self.tool_with(ctx, sketch, regions, participants, placement, feature, None)
    }

    /// [`Self::tool`], or with `sheet` the surface the curves sweep
    /// ([`Kernel::extrude_sheet`]).
    #[allow(clippy::too_many_arguments)]
    fn tool_with<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        sketch: &SketchOutput,
        regions: &[ProfileRegion],
        participants: &[(BodyUid, K::Shape)],
        placement: &Transform,
        feature: FeatureUid,
        sheet: Option<&SketchCurves>,
    ) -> Result<K::Shape, String> {
        let frame = placement.apply_frame(&sketch.frame);
        let normal = frame.normal();
        // A point or a line object stands for the plane through it parallel
        // to the profile's (mitcad#1329).
        let across = Across {
            normal,
            lies: "in a plane parallel to the profile's plane",
        };
        // The sketch normal, or a direction of its own (mitcad#539) moved
        // with the profile.
        let along = match self.direction {
            Some(d) => {
                let d = unit_direction(placement.apply_vector(d))?;
                if dot(d, normal).abs() < 1e-6 {
                    return Err("the extrusion direction lies in the sketch plane".to_owned());
                }
                d
            }
            None => normal,
        };
        let mut direction = if self.flip { along.map(|v| -v) } else { along };

        let (start, start_offset) = match &self.start {
            Start::ProfilePlane => (ExtrudeStart::Offset(0.0), 0.0),
            Start::Offset { offset } => {
                let offset = ctx.param(*offset)?;
                (ExtrudeStart::Offset(offset), offset)
            }
            Start::Object { object, offset } => {
                let offset = offset.map(|id| ctx.param(id)).transpose()?.unwrap_or(0.0);
                let target = object.resolve_across(ctx, &self.object_links, Some(across))?;
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
            Extent::TwoSides { side1, side2, .. } => {
                (SideView::of(side1), Some(SideView::of(side2)))
            }
        };
        let two_sides = matches!(self.extent, Extent::TwoSides { .. });
        // Signed side distances (mitcad#765): one side may end on the
        // other's side.
        let signed = self.signed_two_sides();
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
                    thickness2: thin
                        .thickness2
                        .map(|t| ctx.positive(t))
                        .transpose()?
                        .unwrap_or(0.0),
                };
                let second = match &thin.side2 {
                    None => first,
                    Some(side) => ThinWall {
                        location: side.location,
                        thickness: ctx.positive(side.thickness)?,
                        thickness2: side
                            .thickness2
                            .map(|t| ctx.positive(t))
                            .transpose()?
                            .unwrap_or(0.0),
                    },
                };
                (Some(first), Some(second))
            }
        };

        let origin: [f64; 3] = std::array::from_fn(|i| frame.origin[i] + start_offset * normal[i]);
        let mut sides = Vec::new();
        // The sides' distances as given (not through all's lengths).
        let mut distances: [Option<f64>; 2] = [None, None];
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
                        // Each side goes its own way by the distance's size:
                        // the sign of a two-sided distance does not turn it
                        // (a cut through all one way with a negative second
                        // distance cuts that far the other way in the API
                        // reference dumps). Commands refuse a negative one
                        // there (mitcad#693, `model-api/src/two_sides.rs`); imports add
                        // theirs through the document's own API.
                        _ if !signed => ctx.param(id)?.abs(),
                        // Signed (mitcad#765): a negative side ends on the
                        // other side's side, which the kernel then starts
                        // there.
                        _ => ctx.param(id)?,
                    };
                    if d == 0.0 && !two_sides {
                        return Err(format!("{} must not be zero", ctx.param_name(id)));
                    }
                    distances[k] = Some(d);
                    SideEnd::Distance(d)
                }
                SideView::ToObject(object, offset) => {
                    let offset = offset.map(|id| ctx.param(id)).transpose()?.unwrap_or(0.0);
                    SideEnd::Target(Bound {
                        target: object.resolve_across(ctx, &self.object_links, Some(across))?,
                        offset,
                    })
                }
                SideView::ThroughAll => {
                    let bodies = if participants.is_empty() && self.active_cut_scope().is_none() {
                        ctx.bodies()
                    } else {
                        participants.to_vec()
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
        // Signed ends that meet or cross leave nothing between them; a
        // negative side beside one up to an object or through all is the
        // kernel's to check (mitcad#765).
        if signed
            && let [Some(d1), Some(d2)] = distances
            && !is_positive(d1 + d2)
        {
            return Err(sides_overlap(d1, d2));
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

        let sheet = match sheet {
            Some(curves) => Some(surface_sweeps::chains(ctx, curves, &frame)?),
            None => None,
        };
        let chains = match self.wall_curves() {
            Some(curves) => Some(path::wall_chains(curves.curves_on(ctx, &frame)?, &frame)?),
            None => None,
        };
        let spec = ExtrudeFeatureSpec {
            feature,
            frame,
            regions,
            direction,
            start,
            side1,
            side2,
        };
        match (&sheet, &chains) {
            (Some(sheet), _) => ctx.kernel.extrude_sheet(&spec, sheet),
            (None, Some(chains)) => ctx.kernel.extrude_curves(&spec, chains),
            (None, None) => ctx.kernel.extrude_feature(&spec),
        }
        .map_err(|e| e.to_string())
    }

    /// A surface extrusion's bodies (mitcad#455): the profiles' walls (the
    /// solid tool without its caps), then the curves'.
    fn surface_output<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        let uid = ctx.uid;
        let mut sheets = Vec::new();
        if !self.profiles.is_empty() {
            let (sketch, regions) = profile_regions(ctx, &self.profiles)?;
            let tool = self.tool(ctx, &sketch, &regions, &[], &Transform::IDENTITY, uid)?;
            sheets.push(surface_sweeps::walls(ctx, &tool)?);
        }
        if let Some(curves) = &self.curves {
            let sketch = ctx.sketch(curves.sketch)?;
            sheets.push(self.tool_with(
                ctx,
                &sketch,
                &[],
                &[],
                &Transform::IDENTITY,
                uid,
                Some(curves),
            )?);
        }
        surface_sweeps::output(ctx, sheets)
    }
}

// A planar body face as the profile (mitcad#535).

/// What an extrusion of a body face goes: distances on one side, both
/// sides alike or two sides, each with its taper (mitcad#541).
const FACE_EXTENTS: &str =
    "an extrusion of a body face goes a distance (one side, symmetric or two sides)";

/// Checks an extrusion whose profile is a body face: nothing a sketch
/// profile has besides it, distances only, the face's body and features
/// before the feature.
fn check_face_profile(
    def: &ExtrudeDef,
    ctx: &CheckContext<'_>,
    face: &super::reference::FaceRef,
) -> Result<(), String> {
    if !def.profiles.is_empty() {
        return Err("an extrusion of a body face takes no sketch profiles".to_owned());
    }
    if def.thin.is_some() || def.surface || def.curves.is_some() || def.wrap.is_some() {
        return Err(
            "an extrusion of a body face has no thin walls, surface, curves or wrap".to_owned(),
        );
    }
    if def.cut_scope.is_some() {
        return Err(
            "an extrusion of a body face works on the bodies of its own component, without \
             cut_scope"
                .to_owned(),
        );
    }
    if matches!(def.start, Start::Object { .. }) {
        return Err("an extrusion of a body face starts at the face or offset from it".to_owned());
    }
    let distance = |side: &Side| matches!(side, Side::Distance { .. });
    let by_distance = match &def.extent {
        Extent::Distance { .. } | Extent::Symmetric { .. } => true,
        Extent::TwoSides { side1, side2, .. } => distance(side1) && distance(side2),
        _ => false,
    };
    if !by_distance {
        return Err(FACE_EXTENTS.to_owned());
    }
    face.check(ctx)
}

impl ExtrudeDef {
    /// The tool of an extrusion whose profile is a body face: the face as
    /// found on its body now (a face a sketch change renamed is found
    /// again, with a warning), swept by the kernel along its outward
    /// normal, its faces named after `feature`. A face that is gone, not
    /// planar or of a mesh body fails the feature; no other profile is
    /// taken instead.
    fn face_tool<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        face: &super::reference::FaceRef,
        feature: FeatureUid,
    ) -> Result<K::Shape, String> {
        let body = ctx.surface_body(face.body)?;
        let found = ctx.face_on(face.body, &body, &face.face)?;
        let start = match &self.start {
            Start::ProfilePlane => 0.0,
            Start::Offset { offset } => ctx.param(*offset)?,
            Start::Object { .. } => {
                return Err(
                    "an extrusion of a body face starts at the face or offset from it".to_owned(),
                );
            }
        };
        let mut reversed = self.flip;
        // Each side's taper (mitcad#541), none when not given.
        let taper = |ctx: &mut EvalContext<'_, K>, t: &Option<ParamId>| -> Result<f64, String> {
            Ok(match *t {
                Some(id) => ctx.param(id)?,
                None => 0.0,
            })
        };
        let (side1, side2, taper1, taper2) = match &self.extent {
            Extent::Distance { distance, taper: t } => {
                let d = ctx.param(*distance)?;
                if d == 0.0 {
                    return Err(format!("{} must not be zero", ctx.param_name(*distance)));
                }
                // A negative distance goes into the material.
                if d < 0.0 {
                    reversed = !reversed;
                }
                (d.abs(), None, taper(ctx, t)?, 0.0)
            }
            Extent::Symmetric {
                distance,
                full_length,
                taper: t,
            } => {
                let d = ctx.positive(*distance)?;
                let d = if *full_length { d / 2.0 } else { d };
                let t = taper(ctx, t)?;
                (d, Some(d), t, t)
            }
            Extent::TwoSides {
                side1:
                    Side::Distance {
                        distance: d1,
                        taper: t1,
                    },
                side2:
                    Side::Distance {
                        distance: d2,
                        taper: t2,
                    },
                signed,
            } => {
                // Each side goes its own way by its distance's size, as a
                // sketch profile's, or with `signed` (mitcad#765) by its
                // signed distance: a negative side ends on the other side's
                // side. A side of length zero leaves one side.
                let (a, b) = (ctx.param(*d1)?, ctx.param(*d2)?);
                let (a, b) = if *signed {
                    if !is_positive(a + b) {
                        return Err(sides_overlap(a, b));
                    }
                    (a, b)
                } else {
                    (a.abs(), b.abs())
                };
                let (t1, t2) = (taper(ctx, t1)?, taper(ctx, t2)?);
                match (a == 0.0, b == 0.0) {
                    (false, false) => (a, Some(b), t1, t2),
                    (false, true) => (a, None, t1, 0.0),
                    (true, false) => {
                        reversed = !reversed;
                        (b, None, t2, 0.0)
                    }
                    (true, true) => {
                        return Err(
                            "the two extrude distances must add up to more than zero".to_owned()
                        );
                    }
                }
            }
            _ => return Err(FACE_EXTENTS.to_owned()),
        };
        // A direction of its own (mitcad#541), as a unit vector.
        let direction = self.direction.map(unit_direction).transpose()?;
        let spec = FaceExtrudeSpec {
            feature,
            body: &body,
            face: &found,
            key: &face.face,
            reversed,
            start,
            side1,
            side2,
            direction,
            taper1,
            taper2,
        };
        ctx.kernel
            .extrude_face(&spec)
            .map_err(super::face_refs::kernel_error)
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

/// The error of signed two-sided distances (mitcad#765) whose ends meet or
/// cross: side one ends `d1` along the direction, side two `-d2`.
fn sides_overlap(d1: f64, d2: f64) -> String {
    format!(
        "the two sides overlap: side 1 ends at {d1} and side 2 at {} along the extrusion \
         direction, so nothing lies between them; the two distances must add up to more \
         than zero",
        -d2 + 0.0
    )
}

/// False for zero, negative and NaN values.
pub(crate) fn is_positive(value: f64) -> bool {
    value > 0.0
}

/// An extrusion's own direction (mitcad#539) as a unit vector: finite and
/// of some length.
fn unit_direction(direction: [f64; 3]) -> Result<[f64; 3], String> {
    let length = dot(direction, direction).sqrt();
    if !length.is_finite() || length < 1e-12 {
        return Err(format!(
            "the extrusion direction must be a vector of some length, got {direction:?}"
        ));
    }
    Ok(direction.map(|v| v / length))
}
