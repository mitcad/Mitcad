// SPDX-License-Identifier: MIT
//! Mortise and Tenon (mitcad#1012): a tenon grown out of a planar end
//! face of one body and its mortise cut into another body where the tenon
//! reaches.
//!
//! The tenon is a rectangle `width` x `thickness` on the end face, grown
//! `length` out of it along the face's outward normal (with `through`, up
//! to where the mortise body ends along the tenon's middle: flush with its
//! far face). It is centred on the face (the middle of the face's extent
//! along the tenon's axes) unless offset (`offset_width` along the width,
//! `offset_thickness` along the thickness); what the face leaves round it
//! are the shoulders. The width runs along the face's longest straight
//! edge (across the face's normal; × X, or × Y for a normal near X, when
//! the face has no straight edge), or across it when `turned`.
//!
//! The mortise is the tenon grown by `clearance` (default 0.1 mm) on every
//! side: its walls `clearance` out from the tenon's, its bottom
//! `clearance` past the tenon's tip (a through mortise runs on past the
//! mortise body). With `corners: rounded` the mortise's corners are round
//! by the tool's radius (`tool_diameter` / 2), as a cutter leaves them,
//! and the tenon's edges along its length are rounded to match: by the
//! tool's radius less the clearance (square when the clearance is as large
//! as the radius), so that the mortise is the tenon grown by the clearance
//! there too.
//!
//! Both bodies keep their ids: the tenon joins its body, the mortise cuts
//! the other one (a piece split off it becomes a body of the feature, as a
//! cut's). Refused with the reason: an end face that is not planar, a
//! tenon larger than the end face or offset past its edge, a tenon that
//! does not reach the mortise body.
//!
//! With a `count` above 1 (mitcad#1063) the feature makes that many
//! tenons, each with its mortise, spaced evenly along the face's long
//! direction (its longest straight edge, whether or not `turned`): the
//! first tenon's side `start_offset` from the face's end where that
//! direction starts, the last one's `end_offset` from the other end
//! (default 0 both), the same gap between each two. The offset along that
//! direction is not used then (nor the start and end offsets at a count of
//! 1); the other offset moves them all. Refused: a count that is not a
//! whole number from 1 to [`MAX_TENONS`], negative offsets, offsets that
//! leave less than a tenon, and tenons whose mortises would overlap or
//! touch.
//!
//! Face names: the tenon's sides `side(c1)` … `side(c4)` (rounded: lines
//! `c1`, `c3`, `c5`, `c7` and the round edges `c2`, `c4`, `c6`, `c8`),
//! counter-clockwise from the side along the width at -thickness / 2; its
//! tip `end(…)`. The mortise's walls `side(c11)` … (`c11` … `c18`) in the
//! same order and its bottom `end(…)`. Tenon `k` of several (from 0, from
//! the start) has the curves `20 k` on: `c21` …, its mortise `c31` ….

use serde::{Deserialize, Serialize};

use mitcad_vec3::{add, cross, dot, norm, scale, sub, unit_div};

use super::base::apply_tool;
use super::bearing_sizes::short_number;
use super::face_refs::{add_faces, check_faces, kernel_error};
use super::mapping::map_params;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, is_false,
};
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::{
    BooleanOp, Curve3, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, Kernel, SideEnd,
};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
use crate::topo::{FaceName, RegionKey, SegmentKey, TopoName};

/// The default clearance per side, mm.
pub const DEFAULT_CLEARANCE: f64 = 0.1;

/// How far a through mortise runs on past the mortise body's far face, mm
/// (besides the clearance), so that its end lies on none of the body's
/// faces.
const THROUGH_PAST: f64 = 1.0;

/// Slack of the fit checks, mm.
const SLACK: f64 = 1e-6;

/// The most tenons of one feature.
pub const MAX_TENONS: f64 = 100.0;

/// How far apart the curve ids of two tenons (and of their mortises) are.
const CURVE_STRIDE: u32 = 20;

const NO_LENGTH: &str = "a tenon that is not through needs a length";
const NO_TOOL: &str = "rounded corners need a tool diameter";

/// The mortise's corners.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corners {
    /// Square corners and edges.
    #[default]
    Square,
    /// The mortise's corners round by the tool's radius, the tenon's edges
    /// rounded to match.
    Rounded,
}

impl Corners {
    fn is_square(&self) -> bool {
        *self == Corners::Square
    }
}

/// A tenon on an end face of `body` and its mortise in `mortise_body` (see
/// the module's documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct MortiseTenonDef<P = ParamId> {
    /// The body the tenon grows out of.
    pub body: BodyUid,
    /// Its planar end face the tenon stands on.
    pub face: FaceName,
    /// The body the mortise is cut into.
    pub mortise_body: BodyUid,
    pub width: P,
    pub thickness: P,
    /// Out of the end face; left out with `through`.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub length: Option<P>,
    /// The tenon reaches the mortise body's far face.
    #[serde(default, skip_serializing_if = "is_false")]
    pub through: bool,
    /// The tenon's middle from the face's along the width; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub offset_width: Option<P>,
    /// The same along the thickness; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub offset_thickness: Option<P>,
    /// The width across the face's longest straight edge.
    #[serde(default, skip_serializing_if = "is_false")]
    pub turned: bool,
    /// Per side; default 0.1 mm.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub clearance: Option<P>,
    #[serde(default, skip_serializing_if = "Corners::is_square")]
    pub corners: Corners,
    /// The cutter's diameter of rounded corners.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub tool_diameter: Option<P>,
    /// How many tenons along the face's long direction; default 1
    /// (mitcad#1063).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub count: Option<P>,
    /// Of several tenons: the first one's side from the face's end where
    /// its long direction starts; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<P>,
    /// The last one's side from the other end; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<P>,
}

impl<P> MortiseTenonDef<P> {
    pub const TYPE: &'static str = "mortise_tenon";
    pub const BASE_NAME: &'static str = "MortiseTenon";

    map_params! {
        MortiseTenonDef {
            body: copy,
            face: clone,
            mortise_body: copy,
            width: param,
            thickness: param,
            length: option,
            through: copy,
            offset_width: option,
            offset_thickness: option,
            turned: copy,
            clearance: option,
            corners: copy,
            tool_diameter: option,
            count: option,
            start_offset: option,
            end_offset: option,
        }
    }
}

/// A joint's sizes, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sizes {
    width: f64,
    thickness: f64,
    /// None for a through tenon.
    length: Option<f64>,
    offset: [f64; 2],
    clearance: f64,
    /// The mortise's corner radius; 0 for square corners.
    radius: f64,
    /// How many tenons; 1 or more.
    count: usize,
    /// Of several tenons: the start and end offsets along the face.
    ends: [f64; 2],
}

impl Sizes {
    fn new(
        def: &MortiseTenonDef,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    ) -> Result<Self, String> {
        let width = value(def.width)?;
        let thickness = value(def.thickness)?;
        let mut optional = |id: Option<ParamId>| -> Result<Option<f64>, String> {
            id.map(&mut *value).transpose()
        };
        let length = if def.through {
            None
        } else {
            Some(optional(def.length)?.ok_or(NO_LENGTH)?)
        };
        let offset = [
            optional(def.offset_width)?.unwrap_or(0.0),
            optional(def.offset_thickness)?.unwrap_or(0.0),
        ];
        let clearance = optional(def.clearance)?.unwrap_or(DEFAULT_CLEARANCE);
        let radius = match def.corners {
            Corners::Square => 0.0,
            Corners::Rounded => optional(def.tool_diameter)?.ok_or(NO_TOOL)? / 2.0,
        };
        let count = optional(def.count)?.unwrap_or(1.0);
        if !count.is_finite()
            || (count - count.round()).abs() > 1e-9
            || !(1.0..=MAX_TENONS).contains(&count.round())
        {
            return Err(mitcad_model_base::internal::parameters::count_error(
                format!(
                    "the count of tenons must be a whole number from 1 to {MAX_TENONS}, got {count}"
                ),
                count,
            ));
        }
        let ends = [
            optional(def.start_offset)?.unwrap_or(0.0),
            optional(def.end_offset)?.unwrap_or(0.0),
        ];
        let sizes = Sizes {
            width,
            thickness,
            length,
            offset,
            clearance,
            radius,
            count: count.round() as usize,
            ends,
        };
        sizes.check(def.corners)?;
        Ok(sizes)
    }

    fn check(&self, corners: Corners) -> Result<(), String> {
        positive("tenon's width", self.width)?;
        positive("tenon's thickness", self.thickness)?;
        if let Some(length) = self.length {
            positive("tenon's length", length)?;
        }
        for offset in self.offset {
            if !offset.is_finite() {
                return Err(format!("the tenon's offset must be a length, got {offset}"));
            }
        }
        for (what, offset) in ["start", "end"].into_iter().zip(self.ends) {
            if offset < 0.0 || !offset.is_finite() {
                return Err(format!(
                    "the {what} offset must not be negative, got {offset}"
                ));
            }
        }
        if self.clearance < 0.0 || !self.clearance.is_finite() {
            return Err(format!(
                "the clearance must not be negative, got {}",
                self.clearance
            ));
        }
        if corners == Corners::Rounded {
            positive("tool diameter", 2.0 * self.radius)?;
            let narrow = self.width.min(self.thickness) + 2.0 * self.clearance;
            if 2.0 * self.radius >= narrow {
                return Err(format!(
                    "the tool diameter ({} mm) must be smaller than the mortise's {} ({} mm)",
                    short_number(2.0 * self.radius),
                    if self.thickness <= self.width {
                        "thickness"
                    } else {
                        "width"
                    },
                    short_number(narrow)
                ));
            }
        }
        Ok(())
    }

    /// The tenon's corner radius: the mortise's less the clearance.
    fn tenon_radius(&self) -> f64 {
        (self.radius - self.clearance).max(0.0)
    }
}

fn positive(what: &str, v: f64) -> Result<(), String> {
    if v > 0.0 && v.is_finite() {
        Ok(())
    } else {
        Err(format!("the {what} must be greater than zero, got {v}"))
    }
}

impl FeatureInfo for MortiseTenonDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references.body(self.mortise_body);
        add_faces(&mut references, std::iter::once(&self.face));
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, std::slice::from_ref(&self.face), "end face")?;
        ctx.body(self.mortise_body)?;
        if self.mortise_body == self.body {
            return Err(format!(
                "the mortise must go into another body than the tenon's ({})",
                self.body
            ));
        }
        if self.length.is_none() && !self.through {
            return Err(NO_LENGTH.to_owned());
        }
        if self.corners == Corners::Rounded && self.tool_diameter.is_none() {
            return Err(NO_TOOL.to_owned());
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        Sizes::new(self, &mut |id| Ok(value(id))).map(|_| ())
    }

    fn creates_bodies(&self) -> bool {
        // The pieces of a mortise body that the mortise splits.
        true
    }
}

impl<K: Kernel> Evaluate<K> for MortiseTenonDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let sizes = Sizes::new(self, &mut |id| ctx.param(id))?;
        let tenon_body = ctx.surface_body(self.body)?;
        let mortise_body = ctx.surface_body(self.mortise_body)?;
        let face = ctx.face_on(self.body, &tenon_body, &self.face)?;
        let plane = ctx
            .kernel
            .face_plane(&tenon_body, &face)
            .map_err(|_| format!("the tenon's end face {face} must be planar"))?;
        let z = unit_div(plane.normal).ok_or("the tenon's end face has no normal")?;
        let curves = ctx
            .kernel
            .curves_of(&tenon_body, &TopoName::Face(face.clone()))
            .map_err(kernel_error)?;
        let end = EndFace::new(plane.origin, z, &curves, self.turned)?;
        // The first tenon's frame and every tenon's middle in it.
        let (frame, centres) = end.tenon_frames(&sizes)?;
        let mut lengths = Vec::with_capacity(centres.len());
        for centre in &centres {
            lengths.push(match sizes.length {
                Some(length) => length,
                None => through_length(ctx.kernel, &mortise_body, frame.point(*centre), z)
                    .map_err(kernel_error)?
                    .ok_or_else(|| not_reaching(self.mortise_body))?,
            });
        }
        // Tenons one after another of one length are extruded together.
        let mut groups: Vec<(f64, Vec<usize>)> = Vec::new();
        for (k, &length) in lengths.iter().enumerate() {
            let same = groups
                .last()
                .is_some_and(|(first, _)| (first - length).abs() <= SLACK);
            if !same {
                groups.push((length, Vec::new()));
            }
            if let Some((_, members)) = groups.last_mut() {
                members.push(k);
            }
        }
        let stride = |k: usize| CURVE_STRIDE * k as u32;
        let c = sizes.clearance;
        // The mortises first: a tenon that does not reach the body is
        // refused as such.
        let mut mortises = Vec::with_capacity(groups.len());
        for (length, members) in &groups {
            let depth = if self.through {
                length + c + THROUGH_PAST
            } else {
                length + c
            };
            let regions: Vec<ProfileRegion> = members
                .iter()
                .map(|k| {
                    rectangle(
                        11 + stride(*k),
                        sizes.width + 2.0 * c,
                        sizes.thickness + 2.0 * c,
                        sizes.radius,
                        centres[*k],
                    )
                })
                .collect();
            mortises.push(extrude(ctx, frame, &regions, z, 0.0, depth)?);
        }
        let mortise = one_shape(ctx.kernel, mortises)?;
        // (apply_tool's words for a tool that touches none of its bodies.)
        let missed = "the mortise does not cut into any participant body";
        let mut changes = apply_tool(
            ctx,
            BooleanOp::Cut,
            &[(self.mortise_body, mortise_body)],
            &mortise,
            "mortise",
        )
        .map_err(|e| {
            if e == missed {
                not_reaching(self.mortise_body)
            } else {
                e
            }
        })?;
        // The tenons start a little inside their body, so that they
        // overlap the body instead of touching it.
        let mut tenons = Vec::with_capacity(groups.len());
        for (length, members) in &groups {
            let delta = (length * 0.05).clamp(1e-3, 0.5);
            let regions: Vec<ProfileRegion> = members
                .iter()
                .map(|k| {
                    rectangle(
                        1 + stride(*k),
                        sizes.width,
                        sizes.thickness,
                        sizes.tenon_radius(),
                        centres[*k],
                    )
                })
                .collect();
            tenons.push(extrude(ctx, frame, &regions, z, -delta, length + delta)?);
        }
        let tenon = one_shape(ctx.kernel, tenons)?;
        changes.extend(apply_tool(
            ctx,
            BooleanOp::Join,
            &[(self.body, tenon_body)],
            &tenon,
            "tenon",
        )?);
        Ok(FeatureOutput {
            changes,
            tool: Some(mortise),
            ..FeatureOutput::default()
        })
    }
}

fn not_reaching(body: BodyUid) -> String {
    format!("the tenon does not reach body {body}")
}

/// One shape of several: the only one, or their compound.
fn one_shape<K: Kernel>(kernel: &K, mut shapes: Vec<K::Shape>) -> Result<K::Shape, String> {
    if shapes.len() == 1 {
        return Ok(shapes.remove(0));
    }
    kernel.compound(&shapes).map_err(kernel_error)
}

/// The extrusion of `regions` along `z` from `start` for `distance`.
fn extrude<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    frame: SketchFrame,
    regions: &[ProfileRegion],
    z: [f64; 3],
    start: f64,
    distance: f64,
) -> Result<K::Shape, String> {
    ctx.kernel
        .extrude_feature(&ExtrudeFeatureSpec {
            feature: ctx.uid,
            frame,
            regions,
            direction: z,
            start: ExtrudeStart::Offset(start),
            side1: ExtrudeSide {
                end: SideEnd::Distance(distance),
                taper: 0.0,
                thin: None,
            },
            side2: None,
        })
        .map_err(kernel_error)
}

/// Where a through tenon ends: the distance from `from` along `z` to where
/// the mortise body's material ends last along that line; None when the
/// line meets no material of it ahead.
fn through_length<K: Kernel>(
    kernel: &K,
    body: &K::Shape,
    from: [f64; 3],
    z: [f64; 3],
) -> Result<Option<f64>, crate::kernel::KernelError> {
    let Some(bounds) = kernel.bounding_box(body)? else {
        return Ok(None);
    };
    let reach = bounds
        .corners()
        .map(|corner| dot(sub(corner, from), z))
        .fold(0.0, f64::max)
        + 1.0;
    let crossings = kernel.segment_crossings(body, from, add(from, scale(z, reach)))?;
    Ok(crossings
        .iter()
        .rev()
        .find(|(_, enters)| !enters)
        .map(|(t, _)| t * reach)
        .filter(|length| *length > SLACK))
}

/// The end face in the tenon's axes: x along the width, y along the
/// thickness (z × x), and the face's extent along them from `origin`.
struct EndFace {
    origin: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    min: [f64; 2],
    max: [f64; 2],
    /// x runs across the longest edge, -y along it.
    turned: bool,
}

impl EndFace {
    fn new(origin: [f64; 3], z: [f64; 3], curves: &[Curve3], turned: bool) -> Result<Self, String> {
        let points = outline_points(curves);
        if points.is_empty() {
            return Err("the tenon's end face has no edges".to_owned());
        }
        // The longest straight edge, across the normal.
        let mut longest: Option<[f64; 3]> = None;
        for curve in curves {
            if let Curve3::Line { start, end } = curve {
                let d = sub(*end, *start);
                let d = sub(d, scale(z, dot(d, z)));
                if longest.is_none_or(|l| norm(d) > norm(l) + SLACK) {
                    longest = Some(d);
                }
            }
        }
        let x = match longest.and_then(unit_div) {
            Some(x) => x,
            None => {
                let other = if z[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                unit_div(cross(z, other)).ok_or("the tenon's end face has no normal")?
            }
        };
        // Along its largest component's positive direction, so that an
        // edge's direction does not turn the offsets round.
        let largest = (0..3)
            .max_by(|a, b| x[*a].abs().total_cmp(&x[*b].abs()))
            .unwrap_or(0);
        let x = if x[largest] < 0.0 { scale(x, -1.0) } else { x };
        let (x, y) = if turned {
            (cross(z, x), scale(x, -1.0))
        } else {
            (x, cross(z, x))
        };
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in points {
            let d = sub(p, origin);
            for (i, axis) in [x, y].into_iter().enumerate() {
                let at = dot(d, axis);
                min[i] = min[i].min(at);
                max[i] = max[i].max(at);
            }
        }
        Ok(Self {
            origin,
            x,
            y,
            min,
            max,
            turned,
        })
    }

    /// The first tenon's frame (its middle on the face, x along its width,
    /// y along its thickness) and every tenon's middle in that frame (the
    /// first at the origin); refused when the tenons do not fit.
    fn tenon_frames(&self, sizes: &Sizes) -> Result<(SketchFrame, Vec<[f64; 2]>), String> {
        let size = [self.max[0] - self.min[0], self.max[1] - self.min[1]];
        let tenon = [sizes.width, sizes.thickness];
        if tenon[0] > size[0] + SLACK || tenon[1] > size[1] + SLACK {
            return Err(format!(
                "the tenon ({} × {} mm) is larger than the end face ({} × {} mm)",
                short_number(tenon[0]),
                short_number(tenon[1]),
                short_number(size[0]),
                short_number(size[1])
            ));
        }
        let middle: [f64; 2] =
            std::array::from_fn(|i| (self.min[i] + self.max[i]) / 2.0 + sizes.offset[i]);
        let middles = if sizes.count > 1 {
            self.spaced(sizes, middle)?
        } else {
            vec![middle]
        };
        // The shoulders: what the face leaves on each side of the tenons.
        let low = |i: usize| middles.iter().map(|m| m[i]).fold(f64::INFINITY, f64::min);
        let high = |i: usize| {
            middles
                .iter()
                .map(|m| m[i])
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let shoulders = [
            low(0) - tenon[0] / 2.0 - self.min[0],
            self.max[0] - high(0) - tenon[0] / 2.0,
            low(1) - tenon[1] / 2.0 - self.min[1],
            self.max[1] - high(1) - tenon[1] / 2.0,
        ];
        if shoulders.iter().any(|s| *s < -SLACK) {
            return Err(format!(
                "the tenon's offset takes it past the end face's edge (shoulders {} mm)",
                shoulders
                    .iter()
                    .map(|s| short_number(*s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let first = middles[0];
        let frame = SketchFrame {
            origin: add(
                self.origin,
                add(scale(self.x, first[0]), scale(self.y, first[1])),
            ),
            x_axis: self.x,
            y_axis: self.y,
        };
        let centres = middles
            .iter()
            .map(|m| [m[0] - first[0], m[1] - first[1]])
            .collect();
        Ok((frame, centres))
    }

    /// The middles of several tenons spaced evenly along the face's long
    /// direction between the start and end offsets, `middle` across it.
    fn spaced(&self, sizes: &Sizes, middle: [f64; 2]) -> Result<Vec<[f64; 2]>, String> {
        // The long direction is x, or -y when turned; `u` runs along it.
        let (along, sign) = if self.turned { (1, -1.0) } else { (0, 1.0) };
        let (u_min, u_max) = if self.turned {
            (-self.max[along], -self.min[along])
        } else {
            (self.min[along], self.max[along])
        };
        let a = if self.turned {
            sizes.thickness
        } else {
            sizes.width
        };
        let [start, end] = sizes.ends;
        let first = u_min + start + a / 2.0;
        let last = u_max - end - a / 2.0;
        if last < first - SLACK {
            return Err(format!(
                "the start and end offsets ({} and {} mm) leave less than a tenon's {} mm of \
                 the end face ({} mm)",
                short_number(start),
                short_number(end),
                short_number(a),
                short_number(u_max - u_min)
            ));
        }
        let n = sizes.count;
        let pitch = (last - first).max(0.0) / (n - 1) as f64;
        let mortise = a + 2.0 * sizes.clearance;
        if pitch < a + SLACK {
            return Err(format!(
                "the {n} tenons overlap: {} mm along the face each, their middles {} mm apart",
                short_number(a),
                short_number(pitch)
            ));
        }
        if pitch < mortise + SLACK {
            return Err(format!(
                "the mortises of the {n} tenons overlap: {} mm along the face each with the \
                 clearance, their middles {} mm apart",
                short_number(mortise),
                short_number(pitch)
            ));
        }
        Ok((0..n)
            .map(|k| {
                let mut m = middle;
                m[along] = sign * (first + k as f64 * pitch);
                m
            })
            .collect())
    }
}

/// Points of the face's edges enough for its extent: lines' ends, conics
/// sampled, splines' poles.
fn outline_points(curves: &[Curve3]) -> Vec<[f64; 3]> {
    const SAMPLES: usize = 32;
    let mut points = Vec::new();
    for curve in curves {
        match curve {
            Curve3::Point(p) => points.push(*p),
            Curve3::Line { start, end } => points.extend([*start, *end]),
            Curve3::Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                start,
                end,
                closed,
            } => {
                let y_axis = cross(*normal, *x_axis);
                let (from, to) = if *closed {
                    (0.0, std::f64::consts::TAU)
                } else {
                    (*start, *end)
                };
                for k in 0..=SAMPLES {
                    let t = from + (to - from) * k as f64 / SAMPLES as f64;
                    points.push(add(
                        *center,
                        add(
                            scale(*x_axis, major * t.cos()),
                            scale(y_axis, minor * t.sin()),
                        ),
                    ));
                }
            }
            Curve3::BSpline { poles, .. } => points.extend(poles.iter().copied()),
            Curve3::Helix { .. } => {}
        }
    }
    points
}

/// A sketch curve's id.
fn curve(n: u32) -> EntityUid {
    EntityUid(n)
}

/// A rectangle `a` along x and `b` along y centred on `centre`, its
/// corners rounded by `radius` (none at 0): lines `c<first>` … `c<first +
/// 3>`, or with round corners lines and arcs `c<first>` … `c<first + 7>`,
/// counter-clockwise from the side at -b / 2.
fn rectangle(first: u32, a: f64, b: f64, radius: f64, centre: [f64; 2]) -> ProfileRegion {
    let (x, y) = (a / 2.0, b / 2.0);
    let geometries: Vec<SegmentGeometry> = if radius <= 0.0 {
        let corners = [[-x, -y], [x, -y], [x, y], [-x, y]];
        (0..4)
            .map(|i| SegmentGeometry::Line {
                start: corners[i],
                end: corners[(i + 1) % 4],
            })
            .collect()
    } else {
        let r = radius;
        let (u, v) = (x - r, y - r);
        let half = std::f64::consts::FRAC_PI_2;
        let arc = |center: [f64; 2], from: f64| SegmentGeometry::Arc {
            center,
            radius: r,
            start_angle: from,
            end_angle: from + half,
        };
        vec![
            SegmentGeometry::Line {
                start: [-u, -y],
                end: [u, -y],
            },
            arc([u, -v], -half),
            SegmentGeometry::Line {
                start: [x, -v],
                end: [x, v],
            },
            arc([u, v], 0.0),
            SegmentGeometry::Line {
                start: [u, y],
                end: [-u, y],
            },
            arc([-u, v], half),
            SegmentGeometry::Line {
                start: [-x, v],
                end: [-x, -v],
            },
            arc([-u, -v], 2.0 * half),
        ]
    };
    let n = geometries.len() as u32;
    let id = |i: u32| curve(first + (i % n));
    let moved = |p: [f64; 2]| [p[0] + centre[0], p[1] + centre[1]];
    let segments: Vec<ProfileSegment> = geometries
        .into_iter()
        .zip(0..n)
        .map(|(geometry, i)| ProfileSegment {
            key: SegmentKey::between(id(i), id(i + n - 1), id(i + 1)),
            geometry: match geometry {
                SegmentGeometry::Line { start, end } => SegmentGeometry::Line {
                    start: moved(start),
                    end: moved(end),
                },
                SegmentGeometry::Arc {
                    center,
                    radius,
                    start_angle,
                    end_angle,
                } => SegmentGeometry::Arc {
                    center: moved(center),
                    radius,
                    start_angle,
                    end_angle,
                },
                other => other,
            },
        })
        .collect();
    let key = RegionKey::new(segments.iter().map(|s| s.key.clone())).expect("a loop has segments");
    ProfileRegion {
        key,
        loops: vec![ProfileLoop { segments }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(start: [f64; 3], end: [f64; 3]) -> Curve3 {
        Curve3::Line { start, end }
    }

    /// A 20 x 60 end face at x = 0 (y 0 to 20, z 0 to 60), facing +X.
    fn end_face(turned: bool) -> EndFace {
        let curves = [
            line([0.0, 0.0, 0.0], [0.0, 20.0, 0.0]),
            line([0.0, 20.0, 0.0], [0.0, 20.0, 60.0]),
            line([0.0, 20.0, 60.0], [0.0, 0.0, 60.0]),
            line([0.0, 0.0, 60.0], [0.0, 0.0, 0.0]),
        ];
        EndFace::new([0.0, 10.0, 30.0], [1.0, 0.0, 0.0], &curves, turned).unwrap()
    }

    fn sizes(width: f64, thickness: f64, offset: [f64; 2]) -> Sizes {
        Sizes {
            width,
            thickness,
            length: Some(25.0),
            offset,
            clearance: 0.1,
            radius: 0.0,
            count: 1,
            ends: [0.0, 0.0],
        }
    }

    #[test]
    fn the_width_runs_along_the_longest_edge_unless_turned() {
        let face = end_face(false);
        assert_eq!(face.x, [0.0, 0.0, 1.0]);
        assert_eq!(face.y, [0.0, -1.0, 0.0]);
        assert_eq!((face.min, face.max), ([-30.0, -10.0], [30.0, 10.0]));
        let turned = end_face(true);
        assert_eq!(turned.x, [0.0, -1.0, 0.0]);
        assert_eq!(turned.y, [0.0, 0.0, -1.0]);
        assert_eq!((turned.min, turned.max), ([-10.0, -30.0], [10.0, 30.0]));
    }

    #[test]
    fn a_tenon_is_centred_offset_or_refused_when_it_does_not_fit() {
        let face = end_face(false);
        let frame = face.tenon_frames(&sizes(40.0, 8.0, [0.0, 0.0])).unwrap().0;
        assert_eq!(frame.origin, [0.0, 10.0, 30.0]);
        let frame = face.tenon_frames(&sizes(40.0, 8.0, [10.0, 6.0])).unwrap().0;
        assert_eq!(frame.origin, [0.0, 4.0, 40.0]);
        let error = face
            .tenon_frames(&sizes(70.0, 8.0, [0.0, 0.0]))
            .unwrap_err();
        assert_eq!(
            error,
            "the tenon (70 × 8 mm) is larger than the end face (60 × 20 mm)"
        );
        let error = face
            .tenon_frames(&sizes(40.0, 8.0, [11.0, 0.0]))
            .unwrap_err();
        assert_eq!(
            error,
            "the tenon's offset takes it past the end face's edge (shoulders 21, -1, 6, 6 mm)"
        );
    }

    fn several(width: f64, thickness: f64, count: usize, ends: [f64; 2]) -> Sizes {
        Sizes {
            count,
            ends,
            ..sizes(width, thickness, [0.0, 0.0])
        }
    }

    #[test]
    fn several_tenons_are_spaced_from_the_start_of_the_long_direction() {
        // Along z from 5 to 55: middles at z = 10, 30 and 50.
        let face = end_face(false);
        let (frame, centres) = face
            .tenon_frames(&several(10.0, 8.0, 3, [5.0, 5.0]))
            .unwrap();
        assert_eq!(frame.origin, [0.0, 10.0, 10.0]);
        assert_eq!(centres, vec![[0.0, 0.0], [20.0, 0.0], [40.0, 0.0]]);
        // Turned: the thickness along z, which is -y; from z = 5 to 45,
        // middles at z = 10, 25 and 40.
        let turned = end_face(true);
        let (frame, centres) = turned
            .tenon_frames(&several(8.0, 10.0, 3, [5.0, 15.0]))
            .unwrap();
        assert_eq!(frame.origin, [0.0, 10.0, 10.0]);
        assert_eq!(centres, vec![[0.0, 0.0], [0.0, -15.0], [0.0, -30.0]]);
        // The offset across still moves them; the one along is not used.
        let mut moved = several(10.0, 8.0, 3, [5.0, 5.0]);
        moved.offset = [7.0, 2.0];
        let (frame, _) = face.tenon_frames(&moved).unwrap();
        assert_eq!(frame.origin, [0.0, 8.0, 10.0]);
    }

    #[test]
    fn several_tenons_that_do_not_fit_are_refused() {
        let face = end_face(false);
        let refused = |sizes: Sizes| face.tenon_frames(&sizes).unwrap_err();
        assert_eq!(
            refused(several(20.0, 8.0, 3, [5.0, 5.0])),
            "the 3 tenons overlap: 20 mm along the face each, their middles 15 mm apart"
        );
        // Touching tenons are refused too, and mortises that overlap.
        assert_eq!(
            refused(several(20.0, 8.0, 3, [0.0, 0.0])),
            "the 3 tenons overlap: 20 mm along the face each, their middles 20 mm apart"
        );
        assert_eq!(
            refused(several(19.9, 8.0, 3, [0.0, 0.0])),
            "the mortises of the 3 tenons overlap: 20.1 mm along the face each with the \
             clearance, their middles 20.05 mm apart"
        );
        assert_eq!(
            refused(several(10.0, 8.0, 2, [30.0, 25.0])),
            "the start and end offsets (30 and 25 mm) leave less than a tenon's 10 mm of the \
             end face (60 mm)"
        );
        let mut across = several(10.0, 8.0, 3, [5.0, 5.0]);
        across.offset = [0.0, 7.0];
        assert_eq!(
            refused(across),
            "the tenon's offset takes it past the end face's edge (shoulders 5, 5, 13, -1 mm)"
        );
    }

    #[test]
    fn rounded_rectangles_have_lines_and_quarter_arcs() {
        let region = rectangle(11, 40.2, 8.2, 3.0, [0.0, 0.0]);
        let segments = &region.loops[0].segments;
        assert_eq!(segments.len(), 8);
        assert_eq!(
            segments[1].key,
            SegmentKey::between(curve(12), curve(11), curve(13))
        );
        assert!(matches!(
            segments[1].geometry,
            SegmentGeometry::Arc { radius, .. } if radius == 3.0
        ));
        assert_eq!(
            rectangle(1, 40.0, 8.0, 0.0, [0.0, 0.0]).loops[0]
                .segments
                .len(),
            4
        );
    }

    #[test]
    fn sizes_are_checked() {
        let mut s = sizes(40.0, 8.0, [0.0, 0.0]);
        assert!(s.check(Corners::Square).is_ok());
        s.radius = 4.2;
        assert_eq!(
            s.check(Corners::Rounded).unwrap_err(),
            "the tool diameter (8.4 mm) must be smaller than the mortise's thickness (8.2 mm)"
        );
        s.radius = 3.0;
        assert!(s.check(Corners::Rounded).is_ok());
        assert!((s.tenon_radius() - 2.9).abs() < 1e-12);
        s.clearance = -0.1;
        assert!(s.check(Corners::Square).is_err());
    }
}
