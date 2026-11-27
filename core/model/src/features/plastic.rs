// SPDX-License-Identifier: MIT
//! Plastic part features (mitcad#459): Boss, Lip, Groove, Snap Fit and
//! Rest on the faces of a body, and the plastic rules they take their
//! defaults from.
//!
//! - A **plastic rule** (`plastic_rule`) is a timeline feature that holds a
//!   molded part's design values: the nominal wall (`thickness`), the
//!   nominal radius, the clearance, the knife edge, the reveal height, the
//!   thickness variation, the draft, the largest and smallest wall and the
//!   smallest draft. It changes no body. A Boss, a Rest or a Groove names
//!   one by `rule` and takes its draft (Boss, Rest) or its clearance
//!   (Groove) where its own value is left out; a draft below the rule's
//!   smallest draft and a wall outside its thicknesses are warnings.
//! - A **Boss** is a round post on a face: a cylinder of `diameter` grown
//!   `height` out of the face along `direction` (default: out of the face),
//!   narrowing by its draft, with `ribs` round it (`rib_thickness`,
//!   reaching `rib_length` out of its wall, their tops `rib_offset` below
//!   the boss's top), its root rounded by `fillet` where it meets the
//!   face, and a hole of `hole_diameter` down from its top (`hole_depth`,
//!   default the height: to the face).
//!   Since mitcad#2511 the ribs lean by a draft of their own
//!   (`rib_draft_angle`, or the rule's with `rib_draft`; their thickness is
//!   the face's), the first one stands at `rib_angle` about the axis, and
//!   each rib ends at its length or, by `rib_ends`, up to the wall or face
//!   of the body it meets first, or `short` before it (the kernel's rib
//!   with its outer end continued, [`RibSpec`]); the hole narrows towards
//!   its bottom by its own draft (`hole_draft_angle`, or the rule's with
//!   `hole_draft`; its diameter is the top's). A boss saved before them
//!   has none and is built as it was.
//! - A **Lip** raises a band along a rim face (a planar face round an
//!   opening: a shelled body's open end) `width` wide and `height` high;
//!   a **Groove** cuts one `width` wide and `depth` deep, both widened by
//!   the `gap` (the clearance for the lip that goes in it). Both follow
//!   the opening, or the outer edge with `outside` ([`RimBandSpec`]).
//!   With `tangent` (mitcad#2511) the band's side along the edge continues
//!   the face beside the edge along its tangent instead of rising
//!   straight; with a side draft (`draft_angle`, or the rule's with
//!   `side_draft`) its other side leans, narrowing a lip towards its top
//!   and a groove towards its bottom, so that a groove of a lip's draft
//!   takes the lip. A lip or groove saved before them is built as it was.
//! - A **Snap Fit** is a cantilever arm on a face: `thickness` across,
//!   `width` wide, `length` up from the face, with a catch at its tip that
//!   sticks out `catch_depth` towards `hook` over `catch_length`; the face
//!   of the arm on the catch's side stands on the point.
//! - A **Rest** is a pad on a face, round (`width` the diameter) or a
//!   rectangle (`length` along `along`, `width` across it), `height` high,
//!   narrowing by its draft, solid or a wall of `thickness`.
//!
//! Every feature stands on a point of a face of one body (`body`, `face`,
//! `point`: put on the face's plane for a planar face, else the face's
//! point nearest to it) and joins (a groove cuts) that body only. The
//! tools start a little inside the material so that they overlap it
//! instead of touching it; sizes are measured at the face, and a draft
//! narrows the walls from there. Without `direction` a feature grows out
//! of the face; across it, `along` (a rest's length) defaults to the
//! direction × X, or × Y for a direction near X.
//!
//! Face names: a boss's post `side(c1)` and top `end(r{c1})`, rib k's
//! sides `side(c<10+4k>…c<13+4k>)` and top (a rib up to a wall: the
//! kernel rib's faces inside `rib<k>(…)`), the hole's wall `side(c2)`
//! and bottom `start(r{c2})`, the root's rounds `fillet(<edge>)`; a rest's
//! walls `side(c1…c4)` (round: `side(c1)`), its hollow's `side(c5…c8)`
//! (`side(c2)`) and its top `end(…)`; a snap fit's faces `side(c1…c6)`
//! round its side profile (c1 the arm's foot, c2 the face on the catch's
//! side, c3 and c4 the catch, c5 the tip, c6 the back) and its ends
//! `start(…)` and `end(…)`; a lip's or groove's band
//! [`RimBandSpec`]'s.

use serde::{Deserialize, Serialize};

use mitcad_vec3::{add, scale, sub, unit_div};

use super::base::apply_tool;
use super::extrude::Operation;
use super::face_refs::{add_faces, check_faces, kernel_error};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput,
    References, ToolUse, is_false,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::{
    BooleanOp, Curve3, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeSpec, ExtrudeStart, FilletSet,
    FilletSize, Kernel, RimBandSpec, SideEnd,
};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
use crate::recompute::{FeatureStatus, Read};
use crate::sweeps::{PathCurve, RibSpec, ThicknessLocation};
use crate::topo::{EdgeName, FaceName, RegionKey, SegmentKey};
use mitcad_vec3::{cross, dot};

/// The steepest draft a feature takes, as tapered extrusions do.
const MAX_DRAFT: f64 = 85.0 * std::f64::consts::PI / 180.0;

// ---------------------------------------------------------------------------
// Plastic rules.

/// A plastic rule: a molded part's design values. Every value but the
/// nominal wall has a default (see [`PlasticRuleDef::values`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct PlasticRuleDef<P = ParamId> {
    /// The rule's name (`ABS (1.5mm)`).
    pub rule_name: String,
    /// The material it is for.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub material: String,
    /// The nominal wall thickness.
    pub thickness: P,
    /// Default: half the thickness.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub nominal_radius: Option<P>,
    /// Between parts that fit together; default 0.1 mm.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub clearance: Option<P>,
    /// Default 1 mm.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub knife_edge: Option<P>,
    /// Default 0.5 mm.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub reveal_height: Option<P>,
    /// Default 0.15 times the thickness.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thickness_variation: Option<P>,
    /// Default 2 degrees.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub draft_angle: Option<P>,
    /// No limit without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub max_thickness: Option<P>,
    /// No limit without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub min_thickness: Option<P>,
    /// Default 0.5 degrees.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub min_draft_angle: Option<P>,
}

/// A plastic rule's values (mm, radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlasticValues {
    pub thickness: f64,
    pub nominal_radius: f64,
    pub clearance: f64,
    pub knife_edge: f64,
    pub reveal_height: f64,
    pub thickness_variation: f64,
    pub draft: f64,
    pub max_thickness: Option<f64>,
    pub min_thickness: Option<f64>,
    pub min_draft: f64,
}

impl<P> PlasticRuleDef<P> {
    pub const TYPE: &'static str = "plastic_rule";
    pub const BASE_NAME: &'static str = "PlasticRule";

    map_params! {
        PlasticRuleDef {
            rule_name: clone,
            material: clone,
            thickness: param,
            nominal_radius: option,
            clearance: option,
            knife_edge: option,
            reveal_height: option,
            thickness_variation: option,
            draft_angle: option,
            max_thickness: option,
            min_thickness: option,
            min_draft_angle: option,
        }
    }
}

impl PlasticRuleDef {
    /// The values, each read by `value`; the defaults where left out.
    pub fn values(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    ) -> Result<PlasticValues, String> {
        let thickness = value(self.thickness)?;
        if !(thickness > 0.0 && thickness.is_finite()) {
            return Err(format!(
                "the plastic rule's thickness must be greater than zero, got {thickness}"
            ));
        }
        let mut read = |what: &str, id: Option<ParamId>, default: f64| -> Result<f64, String> {
            let v = match id {
                Some(id) => value(id)?,
                None => default,
            };
            if v >= 0.0 && v.is_finite() {
                Ok(v)
            } else {
                Err(format!(
                    "the plastic rule's {what} must not be negative, got {v}"
                ))
            }
        };
        let degree = std::f64::consts::PI / 180.0;
        let values = PlasticValues {
            thickness,
            nominal_radius: read("nominal radius", self.nominal_radius, 0.5 * thickness)?,
            clearance: read("clearance", self.clearance, 0.1)?,
            knife_edge: read("knife edge", self.knife_edge, 1.0)?,
            reveal_height: read("reveal height", self.reveal_height, 0.5)?,
            thickness_variation: read(
                "thickness variation",
                self.thickness_variation,
                0.15 * thickness,
            )?,
            draft: read("draft", self.draft_angle, 2.0 * degree)?,
            max_thickness: self
                .max_thickness
                .map(|id| read("largest thickness", Some(id), 0.0))
                .transpose()?,
            min_thickness: self
                .min_thickness
                .map(|id| read("smallest thickness", Some(id), 0.0))
                .transpose()?,
            min_draft: read("smallest draft", self.min_draft_angle, 0.5 * degree)?,
        };
        for (what, angle) in [
            ("draft", values.draft),
            ("smallest draft", values.min_draft),
        ] {
            if angle >= MAX_DRAFT {
                return Err(format!(
                    "the plastic rule's {what} must be below 85 degrees, got {:.4} degrees",
                    angle.to_degrees()
                ));
            }
        }
        if let (Some(max), Some(min)) = (values.max_thickness, values.min_thickness)
            && min > max
        {
            return Err(format!(
                "the plastic rule's smallest thickness {min} is above its largest {max}"
            ));
        }
        Ok(values)
    }
}

impl FeatureInfo for PlasticRuleDef {
    fn references(&self) -> References {
        References::default()
    }

    fn check(&self, _ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.rule_name.trim().is_empty() {
            return Err("a plastic rule needs a name".to_owned());
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        self.values(&mut |id| Ok(value(id))).map(|_| ())
    }
}

impl<K: Kernel> Evaluate<K> for PlasticRuleDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        self.values(&mut |id| ctx.param(id))?;
        Ok(FeatureOutput::default())
    }
}

/// The rule a feature names is a plastic rule before it.
fn check_rule(ctx: &CheckContext<'_>, rule: Option<FeatureUid>) -> Result<(), String> {
    let Some(uid) = rule else {
        return Ok(());
    };
    let entry = ctx.feature(uid)?;
    match &entry.def {
        FeatureDef::PlasticRule(_) => Ok(()),
        _ => Err(format!("{} ({uid}) is not a plastic rule", entry.name)),
    }
}

/// The values of the rule a feature names; None without one, or (with a
/// warning) when the rule is suppressed. The read covers the rule's whole
/// result, so an edit of the rule evaluates the feature again.
fn rule_values<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    rule: Option<FeatureUid>,
) -> Result<Option<(String, PlasticValues)>, String> {
    let Some(uid) = rule else {
        return Ok(None);
    };
    let entry = ctx.feature_entry(uid)?;
    let FeatureDef::PlasticRule(def) = &entry.def else {
        return Err(format!("{} ({uid}) is not a plastic rule", entry.name));
    };
    let result = ctx.env.result_of(uid);
    let status = result.map(|r| r.status.clone());
    let version = result.and_then(|r| r.output.as_ref()).map(|o| o.version);
    ctx.reads.push(Read::Output(uid, version));
    match status {
        Some(FeatureStatus::Suppressed) => {
            ctx.warn(format!(
                "the plastic rule {} is suppressed; its values are not used",
                entry.name
            ));
            return Ok(None);
        }
        Some(FeatureStatus::Failed(_)) => {
            return Err(format!("the plastic rule {} failed", entry.name));
        }
        _ => {}
    }
    let values = def.values(&mut |id| ctx.param(id))?;
    Ok(Some((entry.name.clone(), values)))
}

/// A draft below the rule's smallest is a warning.
fn warn_draft<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    rule: &Option<(String, PlasticValues)>,
    draft: f64,
) {
    if let Some((name, values)) = rule
        && draft.abs() + 1e-12 < values.min_draft
    {
        ctx.warn(format!(
            "the draft {:.4} degrees is below the smallest draft of {name}, {:.4} degrees",
            draft.abs().to_degrees(),
            values.min_draft.to_degrees()
        ));
    }
}

/// A wall outside the rule's thicknesses is a warning.
fn warn_wall<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    rule: &Option<(String, PlasticValues)>,
    what: &str,
    wall: f64,
) {
    let Some((name, values)) = rule else {
        return;
    };
    if let Some(min) = values.min_thickness
        && wall + 1e-9 < min
    {
        ctx.warn(format!(
            "the {what} {wall} mm is thinner than the smallest wall of {name}, {min} mm"
        ));
    }
    if let Some(max) = values.max_thickness
        && wall > max + 1e-9
    {
        ctx.warn(format!(
            "the {what} {wall} mm is thicker than the largest wall of {name}, {max} mm"
        ));
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers.

/// The axes across `z`: x along `along` (its part across z) or z × X (× Y
/// for z near X), and y = z × x.
fn across(z: [f64; 3], along: Option<[f64; 3]>) -> Result<([f64; 3], [f64; 3]), String> {
    let x = match along {
        Some(a) => unit_div(sub(a, scale(z, dot(a, z))))
            .ok_or("the direction along the face must not run along the feature's direction")?,
        None => {
            let other = if z[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            unit_div(cross(z, other)).ok_or("the feature's direction is zero")?
        }
    };
    Ok((x, cross(z, x)))
}

/// A sketch curve's id.
fn curve(n: u32) -> EntityUid {
    EntityUid(n)
}

/// A circle `c<n>` about the origin.
fn circle_loop(n: u32, radius: f64) -> ProfileLoop {
    ProfileLoop {
        segments: vec![ProfileSegment {
            key: SegmentKey::closed(curve(n)),
            geometry: SegmentGeometry::Circle {
                center: [0.0, 0.0],
                radius,
                start: 0.0,
            },
        }],
    }
}

/// A polygon of lines `c<first>`, `c<first + 1>`, … from point i to i + 1.
fn polygon_loop(first: u32, points: &[[f64; 2]]) -> ProfileLoop {
    let n = points.len() as u32;
    let id = |i: u32| curve(first + (i % n));
    ProfileLoop {
        segments: (0..n)
            .map(|i| ProfileSegment {
                key: SegmentKey::between(id(i), id(i + n - 1), id(i + 1)),
                geometry: SegmentGeometry::Line {
                    start: points[i as usize],
                    end: points[((i + 1) % n) as usize],
                },
            })
            .collect(),
    }
}

/// A rectangle `c<first>`…`c<first + 3>` centred on the origin, `a` along
/// x and `b` along y.
fn rectangle_loop(first: u32, a: f64, b: f64) -> ProfileLoop {
    let (x, y) = (a / 2.0, b / 2.0);
    polygon_loop(first, &[[-x, -y], [x, -y], [x, y], [-x, y]])
}

/// The region of the loops, the first the outer boundary.
fn region(loops: Vec<ProfileLoop>) -> ProfileRegion {
    let key = RegionKey::new(loops[0].segments.iter().map(|s| s.key.clone()))
        .expect("a loop has segments");
    ProfileRegion { key, loops }
}

/// How far the tools reach into the material below the face.
fn overlap(size: f64) -> f64 {
    (size * 0.05).clamp(1e-3, 0.5)
}

/// Where a feature stands: the body's shape, the point on the face and the
/// direction it grows.
struct Placed<S> {
    shape: S,
    origin: [f64; 3],
    z: [f64; 3],
}

fn placed<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    face: &FaceName,
    point: [f64; 3],
    direction: Option<[f64; 3]>,
) -> Result<Placed<K::Shape>, String> {
    let shape = ctx.surface_body(body)?;
    let face = ctx.face_on(body, &shape, face)?;
    let (origin, normal) = match ctx.kernel.face_plane(&shape, &face) {
        Ok(plane) => {
            let d = dot(sub(point, plane.origin), plane.normal);
            (sub(point, scale(plane.normal, d)), plane.normal)
        }
        Err(_) => {
            let on = ctx
                .kernel
                .face_point_normal(&shape, &face, point)
                .map_err(kernel_error)?;
            (on.point, on.normal)
        }
    };
    let z = match direction {
        Some(d) => unit_div(d).ok_or("the direction must not be zero")?,
        None => unit_div(normal).ok_or("the face has no normal there")?,
    };
    Ok(Placed { shape, origin, z })
}

/// Replaces what `changes` say of `body` with `new`.
fn replace<S>(changes: &mut Vec<BodyChange<S>>, body: BodyUid, new: Vec<BodyChange<S>>) {
    changes.retain(|c| match c {
        BodyChange::Set(b, _) | BodyChange::Remove(b) => *b != body,
    });
    changes.extend(new);
}

/// The shape `changes` set for `body`.
fn shape_of<S: Clone>(changes: &[BodyChange<S>], body: BodyUid) -> Option<S> {
    changes.iter().find_map(|c| match c {
        BodyChange::Set(b, s) if *b == body => Some(s.clone()),
        _ => None,
    })
}

fn check_direction(what: &str, v: Option<[f64; 3]>) -> Result<(), String> {
    match v {
        Some(v) if unit_div(v).is_none() => {
            Err(format!("the {what} must be a direction, got {v:?}"))
        }
        _ => Ok(()),
    }
}

fn positive(what: &str, v: f64) -> Result<(), String> {
    if v > 0.0 && v.is_finite() {
        Ok(())
    } else {
        Err(format!("the {what} must be greater than zero, got {v}"))
    }
}

fn not_negative(what: &str, v: f64) -> Result<(), String> {
    if v >= 0.0 && v.is_finite() {
        Ok(())
    } else {
        Err(format!("the {what} must not be negative, got {v}"))
    }
}

fn check_draft(v: f64) -> Result<(), String> {
    if v.is_finite() && v.abs() < MAX_DRAFT {
        Ok(())
    } else {
        Err(format!(
            "the draft must be between -85 and 85 degrees, got {:.4} degrees",
            v.to_degrees()
        ))
    }
}

/// References of a feature on a face of a body.
fn face_references(body: BodyUid, face: &FaceName, rule: Option<FeatureUid>) -> References {
    let mut references = References::default();
    references.body(body);
    add_faces(&mut references, std::iter::once(face));
    references.features.extend(rule);
    references
}

fn check_on_face(
    ctx: &CheckContext<'_>,
    body: BodyUid,
    face: &FaceName,
    rule: Option<FeatureUid>,
) -> Result<(), String> {
    ctx.body(body)?;
    check_faces(ctx, std::slice::from_ref(face), "face")?;
    check_rule(ctx, rule)
}

/// A value of a definition: the parameter's, or None when left out.
fn value_of(value: &dyn Fn(ParamId) -> f64, id: Option<ParamId>) -> Option<f64> {
    id.map(value)
}

// ---------------------------------------------------------------------------
// Boss.

/// A round post on a face (see the module's documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct BossDef<P = ParamId> {
    pub body: BodyUid,
    pub face: FaceName,
    pub point: [f64; 3],
    /// The boss grows along it; default out of the face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f64; 3]>,
    pub diameter: P,
    pub height: P,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub hole_diameter: Option<P>,
    /// From the top; default the height.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub hole_depth: Option<P>,
    /// Narrows the post; default the rule's draft, else 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub draft_angle: Option<P>,
    /// The root's rounding radius; none without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub fillet: Option<P>,
    /// How many ribs, a whole number from 0 to 64; none without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub ribs: Option<P>,
    /// Default 0.3 times the radius.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub rib_thickness: Option<P>,
    /// Out of the post's wall; default the radius.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub rib_length: Option<P>,
    /// The ribs' tops below the boss's top; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub rib_offset: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<FeatureUid>,
    /// The first rib's angle about the axis from the boss's x axis
    /// (mitcad#2511); the others follow it evenly spaced. Default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub rib_angle: Option<P>,
    /// The ribs are drafted (mitcad#2511): by `rib_draft_angle`, else by the
    /// rule's draft. A `rib_draft_angle` drafts them without it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub rib_draft: bool,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub rib_draft_angle: Option<P>,
    /// The hole is drafted (mitcad#2511): by `hole_draft_angle`, else by
    /// the rule's draft. A `hole_draft_angle` drafts it without it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hole_draft: bool,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub hole_draft_angle: Option<P>,
    /// Where each rib ends (mitcad#2511): entry k for rib k (the first rib
    /// at `rib_angle` is rib 0); a rib without an entry ends at its length.
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub rib_ends: Vec<RibEnd<P>>,
}

/// How a boss's rib ends (mitcad#2511).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RibEndKind {
    /// `rib_length` out of the post's wall, as ribs made before the choice.
    #[default]
    Length,
    /// Up to the wall or face of the body it meets first along its length.
    Wall,
}

/// Where one of a boss's ribs ends (`rib_ends`, mitcad#2511).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct RibEnd<P = ParamId> {
    #[serde(default)]
    pub end: RibEndKind,
    /// With `wall`: how far short of the wall the rib stops, along its
    /// length; default 0 (at the wall).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub short: Option<P>,
}

fn map_rib_ends<P, Q, E>(
    ends: &[RibEnd<P>],
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Vec<RibEnd<Q>>, E> {
    let mut out = Vec::with_capacity(ends.len());
    for (i, end) in ends.iter().enumerate() {
        out.push(RibEnd {
            end: end.end,
            short: end
                .short
                .as_ref()
                .map(|p| f(&format!("rib_ends[{i}].short"), p))
                .transpose()?,
        });
    }
    Ok(out)
}

impl<P> BossDef<P> {
    pub const TYPE: &'static str = "boss";
    pub const BASE_NAME: &'static str = "Boss";

    map_params! {
        BossDef {
            body: copy,
            face: clone,
            point: copy,
            direction: copy,
            diameter: param,
            height: param,
            hole_diameter: option,
            hole_depth: option,
            draft_angle: option,
            fillet: option,
            ribs: option,
            rib_thickness: option,
            rib_length: option,
            rib_offset: option,
            rule: copy,
            rib_angle: option,
            rib_draft: copy,
            rib_draft_angle: option,
            hole_draft: copy,
            hole_draft_angle: option,
            rib_ends: with(map_rib_ends),
        }
    }
}

/// A boss's ribs (mm, radians), checked.
struct RibSizes {
    count: u32,
    /// At the face.
    thickness: f64,
    length: f64,
    /// Their tops above the face.
    height: f64,
    draft: f64,
    /// The first rib's angle.
    angle: f64,
}

/// A boss's hole (mm, radians), checked.
#[derive(Clone, Copy)]
struct HoleSizes {
    /// At the boss's top.
    radius: f64,
    depth: f64,
    draft: f64,
}

/// A boss's sizes (mm, radians), checked.
struct BossSizes {
    radius: f64,
    height: f64,
    draft: f64,
    fillet: f64,
    hole: Option<HoleSizes>,
    ribs: Option<RibSizes>,
}

/// The drafts of a boss's post, ribs and hole (radians): each its own or
/// the rule's.
#[derive(Clone, Copy, Default)]
struct BossDrafts {
    post: f64,
    ribs: f64,
    hole: f64,
}

impl BossSizes {
    fn new(
        def: &BossDef,
        value: &dyn Fn(ParamId) -> f64,
        drafts: BossDrafts,
    ) -> Result<Self, String> {
        let radius = value(def.diameter) / 2.0;
        let height = value(def.height);
        let draft = drafts.post;
        positive("boss's diameter", radius)?;
        positive("boss's height", height)?;
        check_draft(draft)?;
        let top_radius = radius - height * draft.tan();
        if top_radius <= 0.0 {
            return Err("the draft closes the boss before its top".to_owned());
        }
        let fillet = value_of(value, def.fillet).unwrap_or(0.0);
        not_negative("boss's root fillet", fillet)?;
        let hole = match def.hole_diameter {
            Some(id) => {
                let r = value(id) / 2.0;
                let depth = value_of(value, def.hole_depth).unwrap_or(height);
                positive("boss's hole diameter", r)?;
                positive("boss's hole depth", depth)?;
                if r >= top_radius {
                    return Err(format!(
                        "the boss's hole ({} mm across) must fit inside its top ({} mm across)",
                        2.0 * r,
                        2.0 * top_radius
                    ));
                }
                // Its own draft (mitcad#2511): the diameter is the top's,
                // a positive draft narrows it towards its bottom.
                check_draft(drafts.hole)?;
                let bottom = r - depth * drafts.hole.tan();
                if bottom <= 0.0 {
                    return Err("the hole's draft closes it before its bottom".to_owned());
                }
                let floor = (height - depth).max(0.0);
                if bottom >= radius - floor * draft.tan() {
                    return Err("the hole's draft takes it through the boss's wall".to_owned());
                }
                Some(HoleSizes {
                    radius: r,
                    depth,
                    draft: drafts.hole,
                })
            }
            None => None,
        };
        let ribs = match def.ribs {
            Some(id) => {
                // A whole number: counts are not rounded (mitcad#662).
                let n = value(id);
                if !(0.0..=64.0).contains(&n) || (n - n.round()).abs() > 1e-9 {
                    return Err(mitcad_model_base::internal::parameters::count_error(
                        format!("a boss has a whole number of ribs from 0 to 64, got {n}"),
                        n,
                    ));
                }
                let n = n.round();
                let thickness = value_of(value, def.rib_thickness).unwrap_or(radius * 0.3);
                let length = value_of(value, def.rib_length).unwrap_or(radius);
                let offset = value_of(value, def.rib_offset).unwrap_or(0.0);
                let angle = value_of(value, def.rib_angle).unwrap_or(0.0);
                if n > 0.0 {
                    positive("rib thickness", thickness)?;
                    positive("rib length", length)?;
                    not_negative("rib offset", offset)?;
                    positive(
                        "ribs' height (the boss's height less the rib offset)",
                        height - offset,
                    )?;
                    if !angle.is_finite() {
                        return Err(format!(
                            "the first rib's angle must be a number, got {angle}"
                        ));
                    }
                    // Their own draft (mitcad#2511): the thickness is the
                    // face's, a positive draft thins them towards their tops.
                    check_draft(drafts.ribs)?;
                    if thickness / 2.0 - (height - offset) * drafts.ribs.tan() <= 0.0 {
                        return Err("the ribs' draft closes them before their tops".to_owned());
                    }
                    for (i, end) in def.rib_ends.iter().enumerate() {
                        let short = value_of(value, end.short).unwrap_or(0.0);
                        if !(short >= 0.0 && short.is_finite()) {
                            return Err(format!(
                                "rib {}'s distance short of the wall must not be negative, got {short}",
                                i + 1
                            ));
                        }
                    }
                }
                (n > 0.0).then_some(RibSizes {
                    count: n as u32,
                    thickness,
                    length,
                    height: height - offset,
                    draft: drafts.ribs,
                    angle,
                })
            }
            None => None,
        };
        Ok(Self {
            radius,
            height,
            draft,
            fillet,
            hole,
            ribs,
        })
    }
}

/// A one-side draft or a draft of a boss's ribs or hole (mitcad#2511):
/// `angle` where given; else, when `on`, the rule's draft; else None. On
/// without an angle or a rule is refused with `missing`.
fn own_draft(
    on: bool,
    angle: Option<f64>,
    rule: &Option<(String, PlasticValues)>,
    missing: &str,
) -> Result<Option<f64>, String> {
    match (angle, on, rule) {
        (Some(a), _, _) => Ok(Some(a)),
        (None, false, _) => Ok(None),
        (None, true, Some((_, values))) => Ok(Some(values.draft)),
        (None, true, None) => Err(missing.to_owned()),
    }
}

impl FeatureInfo for BossDef {
    fn references(&self) -> References {
        face_references(self.body, &self.face, self.rule)
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_on_face(ctx, self.body, &self.face, self.rule)?;
        check_direction("boss's direction", self.direction)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        // The rule's drafts are not known here: the evaluation checks them.
        BossSizes::new(
            self,
            value,
            BossDrafts {
                post: value_of(value, self.draft_angle).unwrap_or(0.0),
                ribs: value_of(value, self.rib_draft_angle).unwrap_or(0.0),
                hole: value_of(value, self.hole_draft_angle).unwrap_or(0.0),
            },
        )
        .map(|_| ())
    }
}

/// Rib `k` of `count`'s direction out of the boss's axis and the one
/// across it: the first rib at `angle` from the boss's x axis towards its
/// y axis (mitcad#2511), the others evenly after it.
fn rib_axes(x: [f64; 3], y: [f64; 3], angle: f64, k: u32, count: u32) -> [[f64; 3]; 2] {
    let a = angle + std::f64::consts::TAU * f64::from(k) / f64::from(count);
    let (c, s) = (a.cos(), a.sin());
    [add(scale(x, c), scale(y, s)), sub(scale(y, c), scale(x, s))]
}

/// The edges where the post's wall meets faces other than the feature's
/// own: the boss's root.
fn root_edges<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    wall: &FaceName,
) -> Result<Vec<EdgeName>, String> {
    let edges = kernel.edges(shape).map_err(kernel_error)?;
    Ok(edges
        .iter()
        .filter_map(|e| e.name.as_deref()?.parse::<EdgeName>().ok())
        .filter(|e| {
            let [a, b] = e.faces();
            (a.matches(wall) && b.feature != wall.feature)
                || (b.matches(wall) && a.feature != wall.feature)
        })
        .collect())
}

impl<K: Kernel> Evaluate<K> for BossDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let rule = rule_values(ctx, self.rule)?;
        let mut values = std::collections::BTreeMap::new();
        for id in self.params_used() {
            values.insert(id, ctx.param(id)?);
        }
        let value = |id: ParamId| values.get(&id).copied().unwrap_or(f64::NAN);
        let draft = match self.draft_angle {
            Some(id) => value(id),
            None => rule.as_ref().map_or(0.0, |(_, r)| r.draft),
        };
        let rib_draft = own_draft(
            self.rib_draft,
            value_of(&value, self.rib_draft_angle),
            &rule,
            "the ribs' draft needs a draft angle, or a plastic rule to take it from",
        )?;
        let hole_draft = own_draft(
            self.hole_draft,
            value_of(&value, self.hole_draft_angle),
            &rule,
            "the hole's draft needs a draft angle, or a plastic rule to take it from",
        )?;
        let sizes = BossSizes::new(
            self,
            &value,
            BossDrafts {
                post: draft,
                ribs: rib_draft.unwrap_or(0.0),
                hole: hole_draft.unwrap_or(0.0),
            },
        )?;
        if self.draft_angle.is_some() || rule.is_some() {
            warn_draft(ctx, &rule, sizes.draft);
        }
        if sizes.ribs.is_some()
            && let Some(d) = rib_draft
        {
            warn_draft(ctx, &rule, d);
        }
        if sizes.hole.is_some()
            && let Some(d) = hole_draft
        {
            warn_draft(ctx, &rule, d);
        }
        if let Some(hole) = sizes.hole {
            warn_wall(ctx, &rule, "boss's wall", sizes.radius - hole.radius);
        }
        let at = placed(ctx, self.body, &self.face, self.point, self.direction)?;
        let (x, y) = across(at.z, None)?;
        let frame = SketchFrame {
            origin: at.origin,
            x_axis: x,
            y_axis: y,
        };
        let delta = overlap(sizes.height);
        let tan = sizes.draft.tan();
        // The post, from a little inside the face, as wide as asked there.
        let post_region = region(vec![circle_loop(1, sizes.radius + delta * tan)]);
        let post = ctx
            .kernel
            .extrude_feature(&ExtrudeFeatureSpec {
                feature: ctx.uid,
                frame,
                regions: std::slice::from_ref(&post_region),
                direction: at.z,
                start: ExtrudeStart::Offset(-delta),
                side1: ExtrudeSide {
                    end: SideEnd::Distance(sizes.height + delta),
                    taper: 0.0 - sizes.draft,
                    thin: None,
                },
                side2: None,
            })
            .map_err(kernel_error)?;
        let mut tools = vec![post];
        if let Some(ribs) = &sizes.ribs {
            // Ribs up to a wall cut their inner ends back to the post's wall:
            // the post run on past the ribs' tops, so that their top edges lie
            // inside it.
            let stop = if self
                .rib_ends
                .iter()
                .take(ribs.count as usize)
                .any(|e| e.end == RibEndKind::Wall)
            {
                Some(
                    ctx.kernel
                        .extrude_feature(&ExtrudeFeatureSpec {
                            feature: ctx.uid,
                            frame,
                            regions: std::slice::from_ref(&post_region),
                            direction: at.z,
                            start: ExtrudeStart::Offset(-delta),
                            side1: ExtrudeSide {
                                end: SideEnd::Distance(sizes.height + 2.0 * delta),
                                taper: 0.0 - sizes.draft,
                                thin: None,
                            },
                            side2: None,
                        })
                        .map_err(kernel_error)?,
                )
            } else {
                None
            };
            for k in 0..ribs.count {
                let [u, v] = rib_axes(x, y, ribs.angle, k, ribs.count);
                let end = self.rib_ends.get(k as usize);
                let rib = if let (Some(stop), true) =
                    (&stop, end.is_some_and(|e| e.end == RibEndKind::Wall))
                {
                    let short = end.and_then(|e| e.short).map_or(0.0, value);
                    self.wall_rib(ctx, &at, &sizes, ribs, k, [u, v], short, stop)?
                } else {
                    self.length_rib(ctx, &at, &sizes, ribs, k, [u, v], delta)?
                };
                tools.push(rib);
            }
        }
        let tool = if tools.len() == 1 {
            tools.remove(0)
        } else {
            let all: Vec<&K::Shape> = tools.iter().collect();
            ctx.unite(&all).map_err(kernel_error)?
        };
        let mut changes = apply_tool(
            ctx,
            BooleanOp::Join,
            &[(self.body, at.shape.clone())],
            &tool,
            "boss",
        )?;
        let mut shape = shape_of(&changes, self.body).ok_or("the boss left no body")?;
        if sizes.fillet > 0.0 {
            let wall = FaceName::side(ctx.uid, SegmentKey::closed(curve(1)));
            let edges = root_edges(ctx.kernel, &shape, &wall)?;
            if edges.is_empty() {
                return Err(
                    "the boss's root fillet found no edge where the boss meets the face".to_owned(),
                );
            }
            shape = ctx
                .kernel
                .fillet(
                    ctx.uid,
                    &shape,
                    &[FilletSet {
                        edges: &edges,
                        faces: &[],
                        size: FilletSize::Constant {
                            radius: sizes.fillet,
                        },
                        tangent_chain: true,
                        curvature: false,
                        weight: 0.0,
                    }],
                    true,
                )
                .map_err(|e| format!("the boss's root fillet: {}", kernel_error(e)))?;
            ctx.warn_notes(&shape);
            replace(
                &mut changes,
                self.body,
                vec![BodyChange::Set(self.body, shape.clone())],
            );
        }
        if let Some(HoleSizes {
            radius: r,
            depth,
            draft: hole_draft,
        }) = sizes.hole
        {
            let hole = if hole_draft == 0.0 {
                let hole_region = region(vec![circle_loop(2, r)]);
                ctx.kernel.extrude(&ExtrudeSpec {
                    feature: ctx.uid,
                    frame,
                    regions: std::slice::from_ref(&hole_region),
                    direction: at.z,
                    start: sizes.height - depth,
                    end: sizes.height + delta,
                })
            } else {
                // Drafted (mitcad#2511): as wide as asked at the boss's top,
                // widening from its bottom up by the draft.
                let hole_region = region(vec![circle_loop(2, r - depth * hole_draft.tan())]);
                ctx.kernel.extrude_feature(&ExtrudeFeatureSpec {
                    feature: ctx.uid,
                    frame,
                    regions: std::slice::from_ref(&hole_region),
                    direction: at.z,
                    start: ExtrudeStart::Offset(sizes.height - depth),
                    side1: ExtrudeSide {
                        end: SideEnd::Distance(depth + delta),
                        taper: hole_draft,
                        thin: None,
                    },
                    side2: None,
                })
            }
            .map_err(kernel_error)?;
            let cut = apply_tool(
                ctx,
                BooleanOp::Cut,
                &[(self.body, shape)],
                &hole,
                "boss's hole",
            )?;
            replace(&mut changes, self.body, cut);
        }
        Ok(FeatureOutput {
            changes,
            tool: Some(tool),
            ..FeatureOutput::default()
        })
    }
}

impl BossDef {
    fn params_used(&self) -> Vec<ParamId> {
        let mut ids = Vec::new();
        let _ = self.map_params(&mut |_, id: &ParamId| {
            ids.push(*id);
            Ok::<_, ()>(*id)
        });
        ids
    }

    /// Rib `k` out to its length along `u` (`v` across it), from a little
    /// inside the face, sized at the face and leaning by the ribs' draft.
    #[allow(clippy::too_many_arguments)]
    fn length_rib<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        at: &Placed<K::Shape>,
        sizes: &BossSizes,
        ribs: &RibSizes,
        k: u32,
        [u, v]: [[f64; 3]; 2],
        delta: f64,
    ) -> Result<K::Shape, String> {
        let frame = SketchFrame {
            origin: at.origin,
            x_axis: u,
            y_axis: v,
        };
        // A drafted rib is as asked at the face: a little wider where its
        // tool starts below it.
        let grow = delta * ribs.draft.tan();
        let t = ribs.thickness / 2.0 + grow;
        let outer = sizes.radius + ribs.length + grow;
        let rib = region(vec![polygon_loop(
            10 + 4 * k,
            &[
                [sizes.radius * 0.5, -t],
                [outer, -t],
                [outer, t],
                [sizes.radius * 0.5, t],
            ],
        )]);
        if ribs.draft == 0.0 {
            return ctx
                .kernel
                .extrude(&ExtrudeSpec {
                    feature: ctx.uid,
                    frame,
                    regions: std::slice::from_ref(&rib),
                    direction: at.z,
                    start: -delta,
                    end: ribs.height,
                })
                .map_err(kernel_error);
        }
        ctx.kernel
            .extrude_feature(&ExtrudeFeatureSpec {
                feature: ctx.uid,
                frame,
                regions: std::slice::from_ref(&rib),
                direction: at.z,
                start: ExtrudeStart::Offset(-delta),
                side1: ExtrudeSide {
                    end: SideEnd::Distance(ribs.height + delta),
                    taper: 0.0 - ribs.draft,
                    thin: None,
                },
                side2: None,
            })
            .map_err(kernel_error)
    }

    /// Rib `k` up to the wall or face of the body it meets first along `u`
    /// (mitcad#2511), `short` before it: the kernel's rib from its top edge
    /// down to the face, its outer end continued to the faces it meets and
    /// its inner end cut back to the post's wall (the body and `stop`, the
    /// post run on past the ribs' tops, are the bodies it reaches), leaning
    /// by the ribs' draft. Its faces are the rib's, inside `rib<k>(…)`.
    #[allow(clippy::too_many_arguments)]
    fn wall_rib<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        at: &Placed<K::Shape>,
        sizes: &BossSizes,
        ribs: &RibSizes,
        k: u32,
        [u, _]: [[f64; 3]; 2],
        short: f64,
        stop: &K::Shape,
    ) -> Result<K::Shape, String> {
        let point = |out: f64| add(at.origin, add(scale(u, out), scale(at.z, ribs.height)));
        let chain = vec![PathCurve {
            name: format!("c{}", 11 + 4 * k),
            curve: Curve3::Line {
                start: point(sizes.radius * 0.5),
                end: point(sizes.radius + ribs.length),
            },
        }];
        let chains = [chain];
        let spec = RibSpec {
            feature: ctx.uid,
            frame: SketchFrame {
                origin: at.origin,
                x_axis: u,
                y_axis: at.z,
            },
            web: false,
            chains: &chains,
            // The rib measures its thickness at its curve, the top; the
            // boss's is the face's.
            thickness: ribs.thickness - 2.0 * ribs.height * ribs.draft.tan(),
            location: ThicknessLocation::Symmetric,
            depth: None,
            // Down from its top edge to the face.
            flip: true,
            bodies: vec![at.shape.clone(), stop.clone()],
            extend_ends: true,
            end_offset: 0.0 - short,
            direction: None,
            draft_angle: ribs.draft,
            far: &[],
        };
        let rib = ctx
            .kernel
            .rib(&spec)
            .map_err(|e| format!("rib {} of the boss: {}", k + 1, kernel_error(e)))?;
        // An end that found no wall (the rib's note).
        ctx.warn_notes(&rib);
        ctx.kernel
            .rename_faces(&rib, &format!("{}:rib{k}", ctx.uid), &[])
            .map_err(kernel_error)
    }
}

// ---------------------------------------------------------------------------
// Lip and groove.

/// A band raised along a rim face (see the module's documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct LipDef<P = ParamId> {
    pub body: BodyUid,
    /// The planar rim face.
    pub face: FaceName,
    /// Into the face from its edge.
    pub width: P,
    pub height: P,
    /// Along the face's outer edge instead of its opening.
    #[serde(default, skip_serializing_if = "is_false")]
    pub outside: bool,
    /// The side along the edge follows the face beside it along its
    /// tangent instead of rising straight (mitcad#2511).
    #[serde(default, skip_serializing_if = "is_false")]
    pub tangent: bool,
    /// The other side is drafted (mitcad#2511): by `draft_angle`, else by
    /// the rule's draft. A `draft_angle` drafts it without it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub side_draft: bool,
    /// Narrows the lip towards its top.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub draft_angle: Option<P>,
    /// Where the side draft comes from (mitcad#2511).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<FeatureUid>,
}

impl<P> LipDef<P> {
    pub const TYPE: &'static str = "lip";
    pub const BASE_NAME: &'static str = "Lip";

    map_params! {
        LipDef {
            body: copy,
            face: clone,
            width: param,
            height: param,
            outside: copy,
            tangent: copy,
            side_draft: copy,
            draft_angle: option,
            rule: copy,
        }
    }
}

/// A lip's or a groove's sizes refused on its rim face: `said` for the
/// person, the face painted (mitcad#2511).
fn on_rim(body: BodyUid, face: &FaceName, said: &str) -> String {
    crate::notice::placed(
        said,
        crate::notice::NoticePlace::face(face.to_string()).in_body(&body.to_string()),
        format!("{said} (rim face {face})"),
    )
}

/// The lip's width at its top with a draft: it must stay open.
fn check_lip_draft(
    def: &LipDef,
    width: f64,
    height: f64,
    draft: Option<f64>,
) -> Result<(), String> {
    if let Some(d) = draft {
        check_draft(d)?;
        if width - height * d.tan() <= 0.0 {
            return Err(on_rim(
                def.body,
                &def.face,
                "the lip's draft closes it before its top; make it wider, lower or its draft smaller",
            ));
        }
    }
    Ok(())
}

impl FeatureInfo for LipDef {
    fn references(&self) -> References {
        face_references(self.body, &self.face, self.rule)
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_on_face(ctx, self.body, &self.face, self.rule)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        positive("lip's width", value(self.width))?;
        positive("lip's height", value(self.height))?;
        check_lip_draft(
            self,
            value(self.width),
            value(self.height),
            value_of(value, self.draft_angle),
        )
    }
}

impl<K: Kernel> Evaluate<K> for LipDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let rule = rule_values(ctx, self.rule)?;
        let width = ctx.param(self.width)?;
        let height = ctx.param(self.height)?;
        let angle = self.draft_angle.map(|id| ctx.param(id)).transpose()?;
        positive("lip's width", width)?;
        positive("lip's height", height)?;
        let draft = own_draft(
            self.side_draft,
            angle,
            &rule,
            "the lip's side draft needs a draft angle, or a plastic rule to take it from",
        )?;
        check_lip_draft(self, width, height, draft)?;
        if let Some(d) = draft {
            warn_draft(ctx, &rule, d);
        }
        band(
            ctx,
            self.body,
            &self.face,
            self.outside,
            BandSizes {
                width,
                free: 0.0,
                // A wall that goes on from the face beside the edge starts
                // at the rim face: below it, it would only touch that face
                // along the edge.
                start: if self.tangent { 0.0 } else { -overlap(height) },
                end: height,
                tangent: self.tangent,
                // Narrowing as it rises.
                side_taper: draft.unwrap_or(0.0),
            },
            BooleanOp::Join,
            "lip",
        )
    }
}

/// A recess cut along a rim face (see the module's documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct GrooveDef<P = ParamId> {
    pub body: BodyUid,
    /// The planar rim face.
    pub face: FaceName,
    /// Into the face from its edge, before the gap.
    pub width: P,
    /// Before the gap.
    pub depth: P,
    /// Added to the width and the depth; default the rule's clearance,
    /// else 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub gap: Option<P>,
    /// Along the face's outer edge instead of its opening.
    #[serde(default, skip_serializing_if = "is_false")]
    pub outside: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<FeatureUid>,
    /// The cutter's side along the edge follows the face beside it along
    /// its tangent (mitcad#2511).
    #[serde(default, skip_serializing_if = "is_false")]
    pub tangent: bool,
    /// The groove's wall is drafted (mitcad#2511): by `draft_angle`, else
    /// by the rule's draft. A `draft_angle` drafts it without it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub side_draft: bool,
    /// Narrows the groove towards its bottom, as a lip of the same draft
    /// narrows towards its top.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub draft_angle: Option<P>,
}

impl<P> GrooveDef<P> {
    pub const TYPE: &'static str = "groove";
    pub const BASE_NAME: &'static str = "Groove";

    map_params! {
        GrooveDef {
            body: copy,
            face: clone,
            width: param,
            depth: param,
            gap: option,
            outside: copy,
            rule: copy,
            tangent: copy,
            side_draft: copy,
            draft_angle: option,
        }
    }
}

/// The groove's width at its bottom with a draft: it must stay open.
fn check_groove_draft(
    def: &GrooveDef,
    wide: f64,
    deep: f64,
    draft: Option<f64>,
) -> Result<(), String> {
    if let Some(d) = draft {
        check_draft(d)?;
        if wide - deep * d.tan() <= 0.0 {
            return Err(on_rim(
                def.body,
                &def.face,
                "the groove's draft closes it before its bottom; make it wider, shallower or its draft smaller",
            ));
        }
    }
    Ok(())
}

impl FeatureInfo for GrooveDef {
    fn references(&self) -> References {
        face_references(self.body, &self.face, self.rule)
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_on_face(ctx, self.body, &self.face, self.rule)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        positive("groove's width", value(self.width))?;
        positive("groove's depth", value(self.depth))?;
        let gap = value_of(value, self.gap).unwrap_or(0.0);
        not_negative("groove's gap", gap)?;
        check_groove_draft(
            self,
            value(self.width) + gap,
            value(self.depth) + gap,
            value_of(value, self.draft_angle),
        )
    }
}

impl<K: Kernel> Evaluate<K> for GrooveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let rule = rule_values(ctx, self.rule)?;
        let width = ctx.param(self.width)?;
        let depth = ctx.param(self.depth)?;
        let gap = match self.gap {
            Some(id) => ctx.param(id)?,
            None => rule.as_ref().map_or(0.0, |(_, r)| r.clearance),
        };
        let angle = self.draft_angle.map(|id| ctx.param(id)).transpose()?;
        positive("groove's width", width)?;
        positive("groove's depth", depth)?;
        not_negative("groove's gap", gap)?;
        let draft = own_draft(
            self.side_draft,
            angle,
            &rule,
            "the groove's side draft needs a draft angle, or a plastic rule to take it from",
        )?;
        // The cutter reaches past the edge into the free side (the
        // opening, or outside), so that no wall of it lies on the body's.
        let wide = width + gap;
        check_groove_draft(self, wide, depth + gap, draft)?;
        if let Some(d) = draft {
            warn_draft(ctx, &rule, d);
        }
        band(
            ctx,
            self.body,
            &self.face,
            self.outside,
            BandSizes {
                width: wide,
                free: wide,
                start: -(depth + gap),
                end: overlap(depth),
                tangent: self.tangent,
                // Narrowing as it goes down: a lip of the same draft fits.
                side_taper: -draft.unwrap_or(0.0),
            },
            BooleanOp::Cut,
            "groove",
        )
    }
}

struct BandSizes {
    width: f64,
    free: f64,
    start: f64,
    end: f64,
    tangent: bool,
    side_taper: f64,
}

fn band<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    face: &FaceName,
    outside: bool,
    sizes: BandSizes,
    op: BooleanOp,
    what: &str,
) -> Result<FeatureOutput<K::Shape>, String> {
    let shape = ctx.surface_body(body)?;
    let face = ctx.face_on(body, &shape, face)?;
    let tool = ctx
        .kernel
        .rim_band(&RimBandSpec {
            feature: ctx.uid,
            body: &shape,
            face: &face,
            outside,
            width: sizes.width,
            free: sizes.free,
            start: sizes.start,
            end: sizes.end,
            tangent: sizes.tangent,
            side_taper: sizes.side_taper,
        })
        .map_err(|e| format!("the {what}: {}", kernel_error(e)))?;
    let changes = apply_tool(ctx, op, &[(body, shape)], &tool, what)?;
    Ok(FeatureOutput {
        changes,
        tool: Some(tool),
        ..FeatureOutput::default()
    })
}

// ---------------------------------------------------------------------------
// Snap fit.

/// A cantilever arm with a catch (see the module's documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct SnapFitDef<P = ParamId> {
    pub body: BodyUid,
    pub face: FaceName,
    pub point: [f64; 3],
    /// The arm grows along it; default out of the face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f64; 3]>,
    /// The catch sticks out this way, across the arm.
    pub hook: [f64; 3],
    pub length: P,
    pub thickness: P,
    pub width: P,
    pub catch_depth: P,
    /// The catch's height along the arm.
    pub catch_length: P,
}

impl<P> SnapFitDef<P> {
    pub const TYPE: &'static str = "snap_fit";
    pub const BASE_NAME: &'static str = "SnapFit";

    map_params! {
        SnapFitDef {
            body: copy,
            face: clone,
            point: copy,
            direction: copy,
            hook: copy,
            length: param,
            thickness: param,
            width: param,
            catch_depth: param,
            catch_length: param,
        }
    }
}

fn snap_fit_sizes(l: f64, t: f64, w: f64, cd: f64, cl: f64) -> Result<(), String> {
    positive("snap fit's length", l)?;
    positive("snap fit's thickness", t)?;
    positive("snap fit's width", w)?;
    positive("snap fit's catch depth", cd)?;
    positive("snap fit's catch length", cl)?;
    if cl >= l {
        return Err(format!(
            "the snap fit's catch ({cl} mm) must be shorter than its arm ({l} mm)"
        ));
    }
    Ok(())
}

impl FeatureInfo for SnapFitDef {
    fn references(&self) -> References {
        face_references(self.body, &self.face, None)
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_on_face(ctx, self.body, &self.face, None)?;
        check_direction("snap fit's direction", self.direction)?;
        check_direction("snap fit's hook", Some(self.hook))
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        snap_fit_sizes(
            value(self.length),
            value(self.thickness),
            value(self.width),
            value(self.catch_depth),
            value(self.catch_length),
        )
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: Operation::Join,
            participants: vec![self.body],
        })
    }
}

impl<K: Kernel> Evaluate<K> for SnapFitDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let l = ctx.param(self.length)?;
        let t = ctx.param(self.thickness)?;
        let w = ctx.param(self.width)?;
        let cd = ctx.param(self.catch_depth)?;
        let cl = ctx.param(self.catch_length)?;
        snap_fit_sizes(l, t, w, cd, cl)?;
        let at = placed(ctx, self.body, &self.face, self.point, self.direction)?;
        let hook = unit_div(sub(self.hook, scale(at.z, dot(self.hook, at.z))))
            .ok_or("the snap fit's hook must point across the arm")?;
        let y = cross(at.z, hook);
        let delta = overlap(l);
        // The side profile (x towards the catch, y up the arm) swept across
        // the width.
        let profile = region(vec![polygon_loop(
            1,
            &[
                [-t, -delta],
                [0.0, -delta],
                [0.0, l - cl],
                [cd, l - cl],
                [0.0, l],
                [-t, l],
            ],
        )]);
        let tool = ctx
            .kernel
            .extrude(&ExtrudeSpec {
                feature: ctx.uid,
                frame: SketchFrame {
                    origin: sub(at.origin, scale(y, w / 2.0)),
                    x_axis: hook,
                    y_axis: at.z,
                },
                regions: std::slice::from_ref(&profile),
                direction: y,
                start: 0.0,
                end: w,
            })
            .map_err(kernel_error)?;
        let changes = apply_tool(
            ctx,
            BooleanOp::Join,
            &[(self.body, at.shape)],
            &tool,
            "snap fit",
        )?;
        Ok(FeatureOutput {
            changes,
            tool: Some(tool),
            ..FeatureOutput::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Rest.

/// A pad on a face (see the module's documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct RestDef<P = ParamId> {
    pub body: BodyUid,
    pub face: FaceName,
    /// The rest's centre.
    pub point: [f64; 3],
    /// The rest grows along it; default out of the face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<[f64; 3]>,
    /// A rectangle's length runs along it (see the module's documentation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub along: Option<[f64; 3]>,
    /// A round rest's diameter, a rectangle's width.
    pub width: P,
    /// A rectangle's length; none for a round rest.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub length: Option<P>,
    pub height: P,
    /// Default the rule's draft, else 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub draft_angle: Option<P>,
    /// A hollow rest's wall; none for a solid one.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thickness: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<FeatureUid>,
}

impl<P> RestDef<P> {
    pub const TYPE: &'static str = "rest";
    pub const BASE_NAME: &'static str = "Rest";

    map_params! {
        RestDef {
            body: copy,
            face: clone,
            point: copy,
            direction: copy,
            along: copy,
            width: param,
            length: option,
            height: param,
            draft_angle: option,
            thickness: option,
            rule: copy,
        }
    }
}

/// A rest's sizes: width, length, height, wall (mm) and draft.
fn rest_sizes(
    width: f64,
    length: Option<f64>,
    height: f64,
    wall: Option<f64>,
    draft: f64,
) -> Result<(), String> {
    positive("rest's width", width)?;
    positive("rest's height", height)?;
    if let Some(l) = length {
        positive("rest's length", l)?;
    }
    check_draft(draft)?;
    let narrow = width.min(length.unwrap_or(width));
    if let Some(t) = wall {
        positive("rest's wall thickness", t)?;
        if 2.0 * t >= narrow {
            return Err(format!(
                "the rest's wall ({t} mm) must be thinner than half its width ({narrow} mm)"
            ));
        }
    }
    let shrink = height * draft.tan();
    let closes = match wall {
        Some(t) => 2.0 * shrink >= t,
        None => 2.0 * shrink >= narrow,
    };
    if closes {
        return Err("the draft closes the rest before its top".to_owned());
    }
    Ok(())
}

impl FeatureInfo for RestDef {
    fn references(&self) -> References {
        face_references(self.body, &self.face, self.rule)
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_on_face(ctx, self.body, &self.face, self.rule)?;
        check_direction("rest's direction", self.direction)?;
        check_direction("rest's direction along the face", self.along)
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        rest_sizes(
            value(self.width),
            value_of(value, self.length),
            value(self.height),
            value_of(value, self.thickness),
            value_of(value, self.draft_angle).unwrap_or(0.0),
        )
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: Operation::Join,
            participants: vec![self.body],
        })
    }
}

impl<K: Kernel> Evaluate<K> for RestDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let rule = rule_values(ctx, self.rule)?;
        let width = ctx.param(self.width)?;
        let length = self.length.map(|id| ctx.param(id)).transpose()?;
        let height = ctx.param(self.height)?;
        let wall = self.thickness.map(|id| ctx.param(id)).transpose()?;
        let draft = match self.draft_angle {
            Some(id) => ctx.param(id)?,
            None => rule.as_ref().map_or(0.0, |(_, r)| r.draft),
        };
        rest_sizes(width, length, height, wall, draft)?;
        if self.draft_angle.is_some() || rule.is_some() {
            warn_draft(ctx, &rule, draft);
        }
        if let Some(t) = wall {
            warn_wall(ctx, &rule, "rest's wall", t);
        }
        let at = placed(ctx, self.body, &self.face, self.point, self.direction)?;
        let (x, y) = across(at.z, self.along)?;
        let delta = overlap(height);
        // Sized at the face: the tool starts a little inside, grown by the
        // draft over that.
        let grow = delta * draft.tan();
        let shape = |first: u32, by: f64| match length {
            Some(l) => rectangle_loop(first, l + 2.0 * by, width + 2.0 * by),
            None => circle_loop(first, width / 2.0 + by),
        };
        let mut loops = vec![shape(1, grow)];
        if let Some(t) = wall {
            loops.push(shape(if length.is_some() { 5 } else { 2 }, -t - grow));
        }
        let pad = region(loops);
        let tool = ctx
            .kernel
            .extrude_feature(&ExtrudeFeatureSpec {
                feature: ctx.uid,
                frame: SketchFrame {
                    origin: at.origin,
                    x_axis: x,
                    y_axis: y,
                },
                regions: std::slice::from_ref(&pad),
                direction: at.z,
                start: ExtrudeStart::Offset(-delta),
                side1: ExtrudeSide {
                    end: SideEnd::Distance(height + delta),
                    taper: 0.0 - draft,
                    thin: None,
                },
                side2: None,
            })
            .map_err(kernel_error)?;
        let changes = apply_tool(
            ctx,
            BooleanOp::Join,
            &[(self.body, at.shape)],
            &tool,
            "rest",
        )?;
        Ok(FeatureOutput {
            changes,
            tool: Some(tool),
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-12)
    }

    #[test]
    fn the_first_rib_stands_at_its_angle_and_the_others_follow_it_evenly() {
        // mitcad#2511: a boss on a face up +Z has its x axis along +Y and
        // its y axis along -X (`across`).
        let (x, y) = across([0.0, 0.0, 1.0], None).unwrap();
        assert!(close(x, [0.0, 1.0, 0.0]) && close(y, [-1.0, 0.0, 0.0]));
        // Without an angle, the ribs as before: the first along x.
        let [u, v] = rib_axes(x, y, 0.0, 0, 3);
        assert!(close(u, x) && close(v, y));
        // Turned by 90 degrees: the first along y (-X), the second 120
        // degrees on, the third 240.
        let quarter = std::f64::consts::FRAC_PI_2;
        let [u, v] = rib_axes(x, y, quarter, 0, 3);
        assert!(close(u, [-1.0, 0.0, 0.0]), "{u:?}");
        assert!(close(v, [0.0, -1.0, 0.0]), "{v:?}");
        let [u, _] = rib_axes(x, y, quarter, 1, 3);
        let a = quarter + std::f64::consts::TAU / 3.0;
        assert!(close(u, add(scale(x, a.cos()), scale(y, a.sin()))), "{u:?}");
        let [u, _] = rib_axes(x, y, quarter, 1, 2);
        assert!(close(u, [1.0, 0.0, 0.0]), "{u:?}");
    }
}
