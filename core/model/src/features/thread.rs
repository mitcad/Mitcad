// SPDX-License-Identifier: MIT
//! Thread: a screw thread on cylindrical faces.
//! A cosmetic thread changes no geometry; it is data on the faces that the
//! `threads` query lists for display. A modelled thread builds the
//! 60-degree profile on the faces (ISO metric, Unified and NPT threads;
//! Whitworth and ISO 7-1 threads are cosmetic only): the basic profile of
//! the size, or the profile of given diameters (a tolerance class's, as
//! .f3d designs store them), turned about its axis by an angle, with
//! optional extra radial and flank clearance for a looser fit (mitcad#195).
//! Either may have a chamfer cut at its free ends (mitcad#304), a cosmetic
//! one too.
//!
//! A custom thread (mitcad#893) takes a major diameter and a pitch of its
//! own (`custom`, expressions) on the 60-degree profile; a trapezoidal
//! thread is modelled with the 30-degree profile of ISO 2901 and its crest
//! clearance (`ThreadData::profile`). A modelled thread may have several
//! starts (`starts`, or a trapezoidal designation's lead over its pitch):
//! the grooves evenly spaced round the axis, the lead `starts` pitches.
//!
//! A taper pipe thread (NPT, ISO 7-1 `R`/`Rc`; mitcad#733) is tapered 1:16
//! on the diameter from where it starts: an internal thread at its opening,
//! an external one at its free end (see `ThreadSpec`). Even a cosmetic one
//! changes the geometry: it tapers the bore to its minor diameter (the
//! face's material inside that cone goes), or a shaft to its major diameter.
//! Its diameters at the start are the size's (`ThreadData::taper_start`) or
//! given (`diameters`, as .f3d designs store them). It has no end
//! chamfers.

use serde::{Deserialize, Serialize};

use super::reference::FaceRef;
use super::thread_table::{self, ThreadData, ThreadStandard};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{Axis, Cylinder, Kernel, ThreadChamfer, ThreadSpec, ThreadTaper};
use crate::parameters::ParamId;
use crate::topo::FaceName;
use crate::transform::Transform;

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_iso(standard: &ThreadStandard) -> bool {
    *standard == ThreadStandard::IsoMetric
}

/// A thread size of a standard (`ThreadInfo` in .f3d designs): `M10x1.5` and `6g`,
/// or `1/4-20 UNC` and `2A`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadSize {
    #[serde(default, skip_serializing_if = "is_iso")]
    pub standard: ThreadStandard,
    /// The size in the standard's table; none for a custom thread
    /// (mitcad#893), whose size the feature gives.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designation: String,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub right_handed: bool,
}

impl ThreadSize {
    pub fn data(&self) -> Result<ThreadData, String> {
        thread_table::lookup(self.standard, &self.designation)
    }

    /// Checks the designation, and the class for an internal or external
    /// thread (either when unknown).
    pub fn check(&self, internal: Option<bool>) -> Result<ThreadData, String> {
        let data = self.data()?;
        if let Some(class) = &self.class {
            thread_table::check_class(self.standard, class, internal)?;
        }
        // ISO 7-1 names the side: `Rc` internal, `R` external (mitcad#733).
        if self.standard == ThreadStandard::IsoTaper
            && let Some(internal) = internal
        {
            let rc = data.designation.starts_with("Rc");
            let size = data.designation.trim_start_matches(['R', 'c', ' ']);
            if internal && !rc {
                return Err(format!(
                    "{} is an external thread; use Rc {size} for a hole",
                    data.designation
                ));
            }
            if !internal && rc {
                return Err(format!(
                    "{} is an internal thread; use R {size} for a shaft",
                    data.designation
                ));
            }
        }
        Ok(data)
    }
}

/// A custom thread's size (mitcad#893): its major diameter and its pitch,
/// on the 60-degree profile (`ThreadData::custom`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomSize<P = ParamId> {
    pub major: P,
    pub pitch: P,
}

impl<P> CustomSize<P> {
    /// The size with its values mapped, under `<prefix>major` and
    /// `<prefix>pitch`.
    pub fn map_params<Q, E>(
        &self,
        prefix: &str,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<CustomSize<Q>, E> {
        Ok(CustomSize {
            major: f(&format!("{prefix}major"), &self.major)?,
            pitch: f(&format!("{prefix}pitch"), &self.pitch)?,
        })
    }
}

/// Checks that a thread's size is either the table's or custom
/// (mitcad#893): a custom thread (`standard` `custom`) has its `custom`
/// size and no class, and only a custom thread has one.
pub(crate) fn check_custom(size: &ThreadSize, custom: bool) -> Result<(), String> {
    let is_custom = size.standard == ThreadStandard::Custom;
    if is_custom && !custom {
        return Err(
            "a custom thread needs its size: `custom` with its `major` diameter and `pitch`"
                .to_owned(),
        );
    }
    if custom && !is_custom {
        return Err("only a custom thread (standard `custom`) has a `custom` size".to_owned());
    }
    if is_custom && size.class.is_some() {
        return Err("a custom thread has no tolerance class".to_owned());
    }
    Ok(())
}

/// A thread's size: the table's for its designation, its class and an
/// ISO 7-1 thread's side checked for an internal or an external face
/// (either when unknown), or a custom thread's of its given major diameter
/// and pitch (mitcad#893).
pub(crate) fn size_of<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    size: &ThreadSize,
    custom: Option<&CustomSize>,
    internal: Option<bool>,
) -> Result<ThreadData, String> {
    match custom {
        Some(c) if size.standard == ThreadStandard::Custom => {
            let major = ctx.param(c.major)?;
            let pitch = ctx.param(c.pitch)?;
            ThreadData::custom(major, pitch)
        }
        _ => size.check(internal),
    }
}

/// The most starts a thread may have.
pub const MAX_STARTS: u32 = 64;

/// A number of starts: a whole number from 1 to [`MAX_STARTS`].
pub(crate) fn starts_value(value: f64) -> Result<u32, String> {
    let whole = value.round();
    if value.is_finite()
        && (value - whole).abs() <= 1e-9
        && (1.0..=f64::from(MAX_STARTS)).contains(&whole)
    {
        Ok(whole as u32)
    } else {
        Err(format!(
            "a thread's number of starts must be a whole number from 1 to {MAX_STARTS}, got {value}"
        ))
    }
}

/// Whether a number of starts suits the size: a designation with a lead
/// (`Tr10x4(P2)`) has its own (2), which `starts` may only repeat.
pub(crate) fn check_starts(starts: u32, data: &ThreadData) -> Result<(), String> {
    if data.starts > 1 && starts != data.starts {
        return Err(format!(
            "{} has {} starts by its lead and pitch, not {starts}: leave out the number of starts",
            data.designation, data.starts
        ));
    }
    Ok(())
}

/// A thread's number of starts (mitcad#893): `starts` (an expression), or
/// its designation's (a trapezoidal thread's lead over its pitch), 1
/// without.
pub(crate) fn thread_starts<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    starts: Option<ParamId>,
    data: &ThreadData,
) -> Result<u32, String> {
    let Some(id) = starts else {
        return Ok(data.starts);
    };
    let starts = starts_value(ctx.param(id)?)?;
    check_starts(starts, data)?;
    Ok(starts)
}

/// The end of the cylinder a partial thread is measured from: where the
/// axis of the face's cylinder points (`HighEnd` in .f3d designs, the default), or
/// the other one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadEnd {
    #[default]
    HighEnd,
    LowEnd,
}

impl ThreadEnd {
    pub fn is_high(&self) -> bool {
        *self == Self::HighEnd
    }
}

/// The ends of a thread that get a chamfer (mitcad#304), measured like
/// `location`: the start is the end `location` names, the end the other
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadChamferEnds {
    Start,
    End,
    Both,
}

impl ThreadChamferEnds {
    /// Whether the low and the high end along the face's cylinder's axis
    /// are asked for, with the thread measured from `location`.
    pub fn low_high(self, location: ThreadEnd) -> (bool, bool) {
        let (start, end) = match self {
            Self::Start => (true, false),
            Self::End => (false, true),
            Self::Both => (true, true),
        };
        if location.is_high() {
            (end, start)
        } else {
            (start, end)
        }
    }
}

/// The default angle of a thread's end chamfer to the axis: 45 degrees
/// (an internal thread's countersink of 90 degrees).
pub const DEFAULT_CHAMFER_ANGLE: f64 = std::f64::consts::FRAC_PI_4;

/// How far (mm) a modelled thread's crest may lie from its face and still
/// be on it, as the OCCT facade tells them apart (`kSame` in
/// `geometry/src/thread.cpp`). A crest off its face takes or covers all of
/// the face along the thread's part (mitcad#628).
#[doc(hidden)]
pub const CREST_ON_FACE: f64 = 1e-6;

/// The default diameter of a thread's end chamfer at the end (mitcad#304):
/// a tenth of a pitch below the minor diameter of the size for an external
/// thread, above its major diameter for an internal one, and past the
/// profile's `root` diameter (a class's, moved by a radial clearance) when
/// that lies further out, so that no groove reaches the end's plane.
pub fn default_chamfer_diameter(data: &ThreadData, internal: bool, root: Option<f64>) -> f64 {
    let margin = data.pitch / 10.0;
    if internal {
        data.major.max(root.unwrap_or(data.major)) + margin
    } else {
        data.minor.min(root.unwrap_or(data.minor)) - margin
    }
}

/// Whether a thread part (`thread_part`) reaches the low and the high end
/// of a face of `length` (within a micrometre).
pub(crate) fn part_reaches(part: Option<(f64, f64, bool)>, length: f64) -> (bool, bool) {
    const NEAR: f64 = 1e-6;
    match part {
        None => (true, true),
        Some((l, offset, true)) => (offset + l >= length - NEAR, offset <= NEAR),
        Some((l, offset, false)) => (offset <= NEAR, offset + l >= length - NEAR),
    }
}

/// The diameters of a modelled thread's profile: its crests and roots lie
/// on the major and minor diameters, and it is half a pitch wide at the
/// pitch diameter (`majorDiameter`, `minorDiameter` and `pitchDiameter` of
/// a .f3d design's `ThreadInfo`: the middle of the class's tolerances).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadDiameters<P = ParamId> {
    pub major: P,
    pub minor: P,
    pub pitch: P,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadDef<P = ParamId> {
    /// Cylindrical faces, all internal (hole walls) or all external.
    pub faces: Vec<FaceRef>,
    pub thread: ThreadSize,
    /// Cut the thread into the geometry; cosmetic otherwise.
    #[serde(default, skip_serializing_if = "is_false")]
    pub modeled: bool,
    /// A part of each face, `length` long from `offset`, measured from
    /// `location`; the whole face (`isFullLength` in .f3d designs) without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub length: Option<P>,
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    #[serde(default, skip_serializing_if = "ThreadEnd::is_high")]
    pub location: ThreadEnd,
    /// A modelled thread's diameters, or a taper pipe thread's where it
    /// starts (mitcad#733); the size's basic ones (D, D1, D2; a taper pipe
    /// thread's at its start) without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub diameters: Option<ThreadDiameters<P>>,
    /// A modelled thread turned about its axis (see `ThreadSpec`); 0
    /// without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// A modelled thread's extra radial clearance per side (see
    /// `ThreadSpec`); 0 without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub radial_clearance: Option<P>,
    /// A modelled thread's extra clearance perpendicular to each flank (see
    /// `ThreadSpec`); 0 without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub flank_clearance: Option<P>,
    /// A chamfer at these ends of each face where the thread runs out
    /// through a free end (mitcad#304); none without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub chamfer: Option<ThreadChamferEnds>,
    /// The chamfer's angle to the axis; `DEFAULT_CHAMFER_ANGLE` without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub chamfer_angle: Option<P>,
    /// The chamfer's diameter at the end; `default_chamfer_diameter` of the
    /// size without (it follows the size).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub chamfer_diameter: Option<P>,
    /// A taper pipe thread (`npt`, `iso_taper`) tapered 1:16 on the
    /// diameter (mitcad#733): its cone is cut even when cosmetic, and NPT
    /// may be modelled. Without it (threads saved before) a taper pipe
    /// thread is a straight one, as it was: cosmetic only, no cone.
    #[serde(default, skip_serializing_if = "is_false")]
    pub taper: bool,
    /// A custom thread's size (`thread.standard` `custom`, mitcad#893): its
    /// major diameter and pitch; none for a size of the table.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomSize<P>>,
    /// The number of starts (mitcad#893), a whole number of 1 or more: the
    /// grooves evenly spaced round the axis, the lead that many pitches. A
    /// designation with a lead (`Tr10x4(P2)`) gives its own; 1 without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub starts: Option<P>,
}

impl<P> ThreadDef<P> {
    pub const TYPE: &'static str = "thread";
    pub const BASE_NAME: &'static str = "Thread";

    // Written by hand: the diameters map under nested slots (`diameters.major`).
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<ThreadDef<Q>, E> {
        Ok(ThreadDef {
            faces: self.faces.clone(),
            thread: self.thread.clone(),
            modeled: self.modeled,
            length: self.length.as_ref().map(|v| f("length", v)).transpose()?,
            offset: self.offset.as_ref().map(|v| f("offset", v)).transpose()?,
            location: self.location,
            diameters: match &self.diameters {
                Some(d) => Some(ThreadDiameters {
                    major: f("diameters.major", &d.major)?,
                    minor: f("diameters.minor", &d.minor)?,
                    pitch: f("diameters.pitch", &d.pitch)?,
                }),
                None => None,
            },
            angle: self.angle.as_ref().map(|v| f("angle", v)).transpose()?,
            radial_clearance: self
                .radial_clearance
                .as_ref()
                .map(|v| f("radial_clearance", v))
                .transpose()?,
            flank_clearance: self
                .flank_clearance
                .as_ref()
                .map(|v| f("flank_clearance", v))
                .transpose()?,
            chamfer: self.chamfer,
            chamfer_angle: self
                .chamfer_angle
                .as_ref()
                .map(|v| f("chamfer_angle", v))
                .transpose()?,
            chamfer_diameter: self
                .chamfer_diameter
                .as_ref()
                .map(|v| f("chamfer_diameter", v))
                .transpose()?,
            taper: self.taper,
            custom: self
                .custom
                .as_ref()
                .map(|c| c.map_params("custom.", &mut *f))
                .transpose()?,
            starts: self.starts.as_ref().map(|v| f("starts", v)).transpose()?,
        })
    }

    /// Whether it is a taper pipe thread that tapers (mitcad#733).
    pub fn tapers(&self) -> bool {
        self.taper && self.thread.standard.is_tapered()
    }
}

/// A body's threaded faces with the ends (low, high) to chamfer of each,
/// and the angle each is turned by.
type ThreadedBody<S> = (BodyUid, S, Vec<FaceName>, Vec<(bool, bool)>, Vec<f64>);

/// Whether an axis points "up" as a modelled thread's helix takes it
/// (`geometry/include/mitcad/geometry/thread.hpp`): +Z, across Z +Y, else
/// +X.
pub fn thread_upwards(d: [f64; 3]) -> bool {
    const ACROSS: f64 = 1e-9;
    if d[2].abs() > ACROSS {
        d[2] > 0.0
    } else if d[1].abs() > ACROSS {
        d[1] > 0.0
    } else {
        d[0] > 0.0
    }
}

/// The reference direction of a modelled thread's helix about a unit axis
/// pointing up, and the direction across both (`thread.hpp`): the world X
/// axis projected onto the plane across the axis (the Y axis for axes
/// within about 25 degrees of X), and the axis crossed with it.
pub fn thread_reference(along: [f64; 3]) -> Option<([f64; 3], [f64; 3])> {
    let x = if along[0].abs() > 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let along_x = mitcad_vec3::dot(x, along);
    let reference = mitcad_vec3::unit(std::array::from_fn(|i| x[i] - along_x * along[i]))?;
    Some((reference, mitcad_vec3::cross(along, reference)))
}

/// A modelled thread's frame on a face's cylinder (`thread.hpp`): the point
/// of the axis at the face's low end along the axis pointing up, that axis,
/// and the helix's reference direction and the direction across.
fn thread_frame(c: &Cylinder) -> Option<[[f64; 3]; 4]> {
    let mut along = mitcad_vec3::unit(c.axis.direction)?;
    let mut origin = c.axis.origin;
    if !thread_upwards(along) {
        origin = std::array::from_fn(|i| origin[i] + c.length * along[i]);
        along = along.map(|v| -v);
    }
    let (reference, across) = thread_reference(along)?;
    Some([origin, along, reference, across])
}

/// The angle of a copy of a modelled thread on the face `copy` (mitcad#733):
/// the copy is the original thread (`angle`, `pitch`, right-handed or not)
/// moved by `transform` (a rigid motion or a mirror, which turns it the
/// other way) as a whole, its helix where the moved helix is, not the
/// original's angle measured about the copy's own axis. The original
/// face's cylinder is the copy's moved back. `angle` where the transform
/// is not orthogonal (a scale). The helix rises a `lead` a turn (`pitch`
/// for a single-start thread; mitcad#893).
pub(crate) fn copied_angle(
    angle: f64,
    copy: &Cylinder,
    transform: &Transform,
    (pitch, lead): (f64, f64),
    right_handed: bool,
) -> f64 {
    use std::f64::consts::TAU;
    if !transform.is_orthogonal() || pitch <= 0.0 || lead <= 0.0 {
        return angle;
    }
    let back = crate::assembly::inverse(transform);
    let original = Cylinder {
        axis: Axis {
            origin: back.apply_point(copy.axis.origin),
            direction: back.apply_vector(copy.axis.direction),
        },
        ..*copy
    };
    let (Some([o, u, r, a]), Some([o2, u2, r2, a2])) =
        (thread_frame(&original), thread_frame(copy))
    else {
        return angle;
    };
    // The middle of the groove at `angle` from the low end (an internal
    // thread's three quarters of a pitch, an external one's a quarter; any
    // point of the helix will do, the radius too).
    let base = if copy.internal { 0.75 } else { 0.25 } * pitch;
    let on_helix: [f64; 3] =
        std::array::from_fn(|i| o[i] + base * u[i] + angle.cos() * r[i] + angle.sin() * a[i]);
    let moved = transform.apply_point(on_helix);
    let v: [f64; 3] = std::array::from_fn(|i| moved[i] - o2[i]);
    let along = mitcad_vec3::dot(v, u2);
    let at = mitcad_vec3::dot(v, a2).atan2(mitcad_vec3::dot(v, r2));
    // The copy's helix rises by a lead a turn (falls for a left-hand one):
    // its angle at the low end is where it is `along - base` below.
    let right = right_handed != (transform.determinant() < 0.0);
    let sign = if right { 1.0 } else { -1.0 };
    let turned = at - TAU * (along - base) / (lead * sign);
    // (In (-pi, pi].)
    let turned = turned - TAU * (turned / TAU).round();
    turned + 0.0
}

/// A chamfer angle's check: between 0 and 90 degrees to the axis.
fn check_chamfer_angle(angle: f64) -> Result<(), String> {
    if angle > 0.0 && angle < std::f64::consts::FRAC_PI_2 {
        Ok(())
    } else {
        Err(format!(
            "the thread's chamfer angle must be between 0 and 90 degrees to the axis, got {}°",
            angle.to_degrees()
        ))
    }
}

/// Whether a thread of `standard` can be modelled: an NPT thread only
/// tapered (`taper`, mitcad#733), as the threads saved before knew it.
pub(crate) fn check_modeled(standard: ThreadStandard, taper: bool) -> Result<(), String> {
    if standard == ThreadStandard::Npt && !taper {
        return Err(
            "an NPT thread cannot be modeled straight: Mitcad cuts it tapered (`taper`); make \
             it cosmetic or tapered"
                .to_owned(),
        );
    }
    standard.check_modeled()
}

/// The narrowest crest a flank clearance may leave, in pitches.
const MIN_CREST: f64 = 0.02;

/// A clearance's value: a finite length of 0 or more (`what`: "radial
/// clearance"); 0 without.
pub(crate) fn clearance_value(what: &str, value: Option<f64>) -> Result<f64, String> {
    match value {
        None => Ok(0.0),
        Some(v) if v.is_finite() && v >= 0.0 => Ok(v),
        Some(v) => Err(format!(
            "the thread's {what} must be a finite length of 0 or more, got {v}"
        )),
    }
}

/// A modelled thread's extra clearance (radial, flank), read and checked
/// for its own values.
pub(crate) fn thread_clearance<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    radial: Option<ParamId>,
    flank: Option<ParamId>,
) -> Result<(f64, f64), String> {
    let radial = radial.map(|id| ctx.param(id)).transpose()?;
    let flank = flank.map(|id| ctx.param(id)).transpose()?;
    Ok((
        clearance_value("radial clearance", radial)?,
        clearance_value("flank clearance", flank)?,
    ))
}

/// The crest's width along the axis with a flank clearance: half a pitch at
/// the pitch diameter, narrowing along the flanks at `flank_angle` to the
/// radius (30 degrees for the 60-degree profile, 15 for the trapezoidal
/// one) to the crest (the minor diameter of an internal thread, the major
/// one of an external one), each flank moved `flank / cos(flank_angle)`
/// into the tooth.
fn crest_width(
    pitch: f64,
    (major, minor, pitch_diameter): (f64, f64, f64),
    internal: bool,
    flank: f64,
    flank_angle: f64,
) -> f64 {
    let depth = if internal {
        pitch_diameter - minor
    } else {
        major - pitch_diameter
    };
    pitch / 2.0 - depth * flank_angle.tan() - 2.0 * flank / flank_angle.cos()
}

/// The largest flank clearance that leaves the crest its minimum width
/// (`MIN_CREST` pitches) on a thread of the profile; 0 when none does.
pub(crate) fn largest_flank_clearance(
    pitch: f64,
    diameters: (f64, f64, f64),
    internal: bool,
    flank_angle: f64,
) -> f64 {
    let crest = crest_width(pitch, diameters, internal, 0.0, flank_angle);
    ((crest - MIN_CREST * pitch) * flank_angle.cos() / 2.0).max(0.0)
}

/// Whether a modelled thread's flank clearance is the printed-fit
/// parameter `thread_flank_clearance` itself (`print_fits.rs`; the Thread
/// and Hole panels' default, mitcad#691), not a number or another
/// expression.
pub(crate) fn flank_from_fit<K: Kernel>(ctx: &EvalContext<'_, K>, flank: Option<ParamId>) -> bool {
    flank.is_some_and(|id| ctx.param_name(id) == crate::print_fits::THREAD_FLANK_CLEARANCE)
}

/// The clearance (radial, flank) a modelled thread uses (mitcad#691). The
/// printed-fit parameter is a printer's fit shared by threads of every
/// size: on a pitch too fine for it, the flank clearance it gives is
/// capped at the largest that leaves the crest its minimum width (a
/// micrometre below, so that the checks here and in the geometry pass),
/// and the thread is made. A flank clearance given otherwise is used as it
/// is; [`check_clearance`] refuses it when it leaves no tooth, naming the
/// largest that fits.
pub(crate) fn fitted_clearance(
    pitch: f64,
    diameters: (f64, f64, f64),
    internal: bool,
    (radial, flank): (f64, f64),
    from_fit: bool,
    flank_angle: f64,
) -> (f64, f64) {
    if !from_fit || crest_width(pitch, diameters, internal, flank, flank_angle) >= MIN_CREST * pitch
    {
        return (radial, flank);
    }
    let largest = largest_flank_clearance(pitch, diameters, internal, flank_angle);
    (radial, (largest - 1e-6).max(0.0))
}

/// Whether extra clearance leaves a thread of the profile (`pitch` and the
/// `major`, `minor` and pitch diameters, flanks at `flank_angle` to the
/// radius) on an internal or external face: the flank clearance moves each
/// flank `flank / cos(flank_angle)` into the tooth along the axis, the
/// crest keeping its radius, and must leave the crest at least P/50 wide;
/// the radial clearance moves the whole profile away from the material and
/// must leave an external thread a core. The geometry checks the same
/// (`geometry/src/thread.cpp`).
pub(crate) fn check_clearance(
    pitch: f64,
    diameters: (f64, f64, f64),
    internal: bool,
    (radial, flank): (f64, f64),
    flank_angle: f64,
) -> Result<(), String> {
    let (_, minor, _) = diameters;
    if flank > 0.0 {
        let crest = crest_width(pitch, diameters, internal, flank, flank_angle);
        let needed = MIN_CREST * pitch;
        if crest < needed {
            // The most that fits, rounded down so that it fits as written.
            let largest = (largest_flank_clearance(pitch, diameters, internal, flank_angle) * 1e4)
                .floor()
                / 1e4;
            return Err(format!(
                "the flank clearance of {flank} mm leaves no usable tooth: the crest would be \
                 {crest:.4} mm wide, at least {needed:.4} mm is needed; this thread takes a \
                 flank clearance of at most {largest:.4} mm"
            ));
        }
    }
    if !internal && minor - 2.0 * radial <= 0.0 {
        return Err(format!(
            "the radial clearance of {radial} mm leaves the external thread no core \
             (minor diameter {minor} mm)"
        ));
    }
    Ok(())
}

/// The part of a cylinder a thread covers, for the kernel: (length,
/// offset, from the high end), None for the whole face.
pub(crate) fn thread_part<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    length: Option<ParamId>,
    offset: Option<ParamId>,
    high_end: bool,
) -> Result<Option<(f64, f64, bool)>, String> {
    let Some(length) = length else {
        return Ok(None);
    };
    let length = ctx.positive(length)?;
    let offset = match offset {
        Some(id) => {
            let value = ctx.param(id)?;
            if value < 0.0 {
                return Err(format!(
                    "{} must not be negative, got {value}",
                    ctx.param_name(id)
                ));
            }
            value
        }
        None => 0.0,
    };
    Ok(Some((length, offset, high_end)))
}

impl FeatureInfo for ThreadDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for face in &self.faces {
            face.add_references(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.faces.is_empty() {
            return Err("no faces selected".to_owned());
        }
        for (i, face) in self.faces.iter().enumerate() {
            if self.faces[..i].contains(face) {
                return Err(format!("face {} is listed more than once", face.face));
            }
            face.check(ctx)?;
        }
        if self.offset.is_some() && self.length.is_none() {
            return Err("a thread over the whole face has no offset".to_owned());
        }
        if self.taper && !self.thread.standard.is_tapered() {
            return Err(
                "only a taper pipe thread (NPT, ISO 7-1) is tapered: leave out `taper`".to_owned(),
            );
        }
        if self.modeled {
            check_modeled(self.thread.standard, self.taper)?;
        } else if self.angle.is_some() || (self.diameters.is_some() && !self.taper) {
            return Err("only a modeled thread has diameters and an angle".to_owned());
        } else if self.radial_clearance.is_some() || self.flank_clearance.is_some() {
            return Err("only a modeled thread has extra clearance".to_owned());
        }
        if self.chamfer.is_none()
            && (self.chamfer_angle.is_some() || self.chamfer_diameter.is_some())
        {
            return Err("a thread without a chamfer has no chamfer angle or diameter".to_owned());
        }
        check_custom(&self.thread, self.custom.is_some())?;
        if self.custom.is_some() {
            return Ok(());
        }
        self.thread.check(None).map(|_| ())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Some(length) = self.length
            && (value(length).is_nan() || value(length) <= 0.0)
        {
            return Err(format!(
                "the thread length must be greater than zero, got {}",
                value(length)
            ));
        }
        if let Some(offset) = self.offset
            && value(offset) < 0.0
        {
            return Err(format!(
                "the thread offset must not be negative, got {}",
                value(offset)
            ));
        }
        if let Some(d) = &self.diameters {
            let (major, minor, pitch) = (value(d.major), value(d.minor), value(d.pitch));
            if !(minor > 0.0 && minor < pitch && pitch < major) {
                return Err(format!(
                    "the thread's diameters must grow from the minor ({minor}) to the pitch \
                     ({pitch}) to the major one ({major})"
                ));
            }
        }
        clearance_value("radial clearance", self.radial_clearance.map(value))?;
        clearance_value("flank clearance", self.flank_clearance.map(value))?;
        if let Some(custom) = &self.custom {
            ThreadData::custom(value(custom.major), value(custom.pitch))?;
        }
        if let Some(starts) = self.starts {
            let starts = starts_value(value(starts))?;
            if let Ok(data) = self.thread.data() {
                check_starts(starts, &data)?;
            }
        }
        if let Some(angle) = self.chamfer_angle {
            check_chamfer_angle(value(angle))?;
        }
        if let Some(diameter) = self.chamfer_diameter
            && (value(diameter).is_nan() || value(diameter) <= 0.0)
        {
            return Err(format!(
                "the thread's chamfer diameter must be greater than zero, got {}",
                value(diameter)
            ));
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for ThreadDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let faces = self.faces.clone();
        let threaded = self.threaded(ctx, ctx.uid, &mut |ctx| {
            let mut targets = Vec::new();
            for face in &faces {
                let shape = ctx.surface_body(face.body)?;
                // A face a sketch change renamed is found again (mitcad#272).
                let found = ctx.face_or(&shape, &face.face, || {
                    format!("body {} has no face {}", face.body, face.face)
                })?;
                targets.push((face.body, shape, found, None));
            }
            Ok(targets)
        })?;
        Ok(FeatureOutput {
            changes: threaded
                .into_iter()
                .map(|(uid, shape)| BodyChange::Set(uid, shape))
                .collect(),
            ..FeatureOutput::default()
        })
    }
}

/// The faces a thread goes on: each with its body, the body's shape, and
/// for a pattern's copy the transform that took the original there
/// (mitcad#733).
type ThreadTargets<S> = Vec<(BodyUid, S, FaceName, Option<Transform>)>;

/// What gives a thread its faces, read once the thread's own values are
/// checked ([`ThreadDef::threaded`]).
type TargetsOf<'a, K> =
    &'a mut dyn for<'c, 'e> FnMut(
        &'c mut EvalContext<'e, K>,
    ) -> Result<ThreadTargets<<K as Kernel>::Shape>, String>;

impl ThreadDef {
    /// The thread on `faces` of `shape` (a pattern's copy of the faces,
    /// mitcad#719, each with the transform that took the original there:
    /// a modelled copy is turned as the original moved there, mitcad#733),
    /// its new faces named after `feature`: the shape threaded, as it was
    /// for a cosmetic thread without chamfers.
    pub(crate) fn thread_on<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        body: BodyUid,
        shape: &K::Shape,
        faces: &[(FaceName, Option<Transform>)],
    ) -> Result<K::Shape, String> {
        let targets: ThreadTargets<K::Shape> = faces
            .iter()
            .map(|(f, t)| (body, shape.clone(), f.clone(), *t))
            .collect();
        let mut threaded = self.threaded(ctx, feature, &mut |_| Ok(targets.clone()))?;
        Ok(threaded
            .pop()
            .map_or_else(|| shape.clone(), |(_, shape)| shape))
    }

    /// The thread on the faces `targets` gives (read after the thread's own
    /// values are checked), its new faces named after `feature`: each
    /// body's new shape.
    fn threaded<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        feature: FeatureUid,
        targets: TargetsOf<'_, K>,
    ) -> Result<Vec<(BodyUid, K::Shape)>, String> {
        let part = thread_part(
            ctx,
            self.length,
            self.offset,
            self.location == ThreadEnd::HighEnd,
        )?;
        // The size: the table's, or a custom thread's (mitcad#893), and its
        // number of starts.
        let size = size_of(ctx, &self.thread, self.custom.as_ref(), None)?;
        let starts = thread_starts(ctx, self.starts, &size)?;
        let lead = size.pitch * f64::from(starts);
        // The profile's diameters: given, or the size's (`ThreadData::profile`).
        let diameters = match &self.diameters {
            Some(d) => {
                let (major, minor, pitch) = (
                    ctx.positive(d.major)?,
                    ctx.positive(d.minor)?,
                    ctx.positive(d.pitch)?,
                );
                if !(minor < pitch && pitch < major) {
                    return Err(format!(
                        "the thread's diameters must grow from the minor ({minor}) to the pitch \
                         ({pitch}) to the major one ({major})"
                    ));
                }
                Some((major, minor, pitch))
            }
            None => None,
        };
        let angle = match self.angle {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let clearance = thread_clearance(ctx, self.radial_clearance, self.flank_clearance)?;
        let (radial, _) = clearance;
        // A tapered taper pipe thread has no end chamfers: its ends are
        // left as they are (mitcad#733).
        let ends_asked = if self.tapers() && self.chamfer.is_some() {
            ctx.warn("a tapered taper pipe thread has no end chamfers: Chamfer Ends is left out");
            None
        } else {
            self.chamfer
        };
        // The chamfer's angle (checked before the faces).
        let chamfer_angle = match (ends_asked, self.chamfer_angle) {
            (Some(_), Some(id)) => {
                let angle = ctx.param(id)?;
                check_chamfer_angle(angle)?;
                Some(angle)
            }
            (Some(_), None) => Some(DEFAULT_CHAMFER_ANGLE),
            (None, _) => None,
        };
        let chamfer_diameter = self
            .chamfer_diameter
            .filter(|_| ends_asked.is_some())
            .map(|id| ctx.positive(id))
            .transpose()?;
        // The faces by body, each a cylinder, all on one side of their
        // material, with the ends to chamfer (low, high) of each.
        let mut bodies: Vec<ThreadedBody<K::Shape>> = Vec::new();
        // The thinnest external and the widest internal face, for the
        // chamfer's check.
        let mut extreme_diameter: Option<f64> = None;
        let mut internal = None;
        for (body, shape, found, moved) in targets(ctx)? {
            let cylinder = ctx
                .kernel
                .face_cylinder(&shape, &found)
                .map_err(|e| e.to_string())?;
            // A pattern's copy of a modelled thread: turned as the original
            // moved there (mitcad#733).
            let turned = match moved {
                Some(t) if self.modeled => copied_angle(
                    angle,
                    &cylinder,
                    &t,
                    (size.pitch, lead),
                    // (A mirror's copy is the other way round already.)
                    self.thread.right_handed != (t.determinant() < 0.0),
                ),
                _ => angle,
            };
            if internal.is_some_and(|i| i != cylinder.internal) {
                return Err("the faces of a thread must all be internal or all external".to_owned());
            }
            internal = Some(cylinder.internal);
            // The class and an ISO 7-1 thread's side, checked for the face.
            if self.custom.is_none() {
                self.thread.check(internal)?;
            }
            let data = &size;
            // The size must fit the cylinder: an external thread's root lies
            // inside it, an internal thread's root outside the bore (each
            // moved by the radial clearance).
            let diameter = 2.0 * cylinder.radius;
            // (A taper pipe thread's where it starts, mitcad#733.)
            let (major, minor, _) = diameters.unwrap_or_else(|| {
                (if self.tapers() {
                    data.taper_start(cylinder.internal)
                } else {
                    None
                })
                .unwrap_or_else(|| data.profile(cylinder.internal))
            });
            let fits = if cylinder.internal {
                diameter < major + 2.0 * radial
            } else {
                diameter > minor - 2.0 * radial
            };
            if self.modeled && !fits {
                return Err(format!(
                    "thread {} does not fit face {} of diameter {diameter}",
                    data.designation, found
                ));
            }
            // The ends asked for that the thread reaches; the kernel
            // chamfers those with nothing beyond them.
            let ends = match ends_asked {
                Some(chamfer) => {
                    let (low, high) = chamfer.low_high(self.location);
                    let (reaches_low, reaches_high) = part_reaches(part, cylinder.length);
                    (low && reaches_low, high && reaches_high)
                }
                None => (false, false),
            };
            // The material at the face: the face, or the crest of a modelled
            // thread where that lies further into the material.
            let material = if !self.modeled {
                diameter
            } else if cylinder.internal {
                diameter.min(minor + 2.0 * radial)
            } else {
                diameter.max(major - 2.0 * radial)
            };
            extreme_diameter = Some(match extreme_diameter {
                None => material,
                Some(d) if cylinder.internal => d.max(material),
                Some(d) => d.min(material),
            });
            match bodies.iter_mut().find(|(uid, _, _, _, _)| *uid == body) {
                Some((_, _, faces, all_ends, angles)) => {
                    faces.push(found);
                    all_ends.push(ends);
                    angles.push(turned);
                }
                None => bodies.push((body, shape, vec![found], vec![ends], vec![turned])),
            }
        }
        let mut output = Vec::new();
        let data = &size;
        let is_internal = internal.unwrap_or(false);
        let (major, minor, pitch_diameter) = diameters.unwrap_or_else(|| {
            (if self.tapers() {
                data.taper_start(is_internal)
            } else {
                None
            })
            .unwrap_or_else(|| data.profile(is_internal))
        });
        let flank_angle = data.flank_angle();
        // A tapered taper pipe thread (mitcad#733): even a cosmetic one
        // tapers the faces. Without `taper` (saved before) it is straight.
        let taper = data.taper.filter(|_| self.tapers()).map(|t| ThreadTaper {
            per_length: t.per_length,
            high_end: self.location.is_high(),
        });
        // The printed-fit flank clearance capped where the pitch is too
        // fine for it (mitcad#691); checked below with the other values.
        let clearance = if self.modeled {
            fitted_clearance(
                data.pitch,
                (major, minor, pitch_diameter),
                is_internal,
                clearance,
                flank_from_fit(ctx, self.flank_clearance),
                flank_angle,
            )
        } else {
            clearance
        };
        let (radial_clearance, flank_clearance) = clearance;
        // The end chamfers, when an end is to be chamfered.
        let chamfer = match chamfer_angle {
            Some(angle)
                if bodies
                    .iter()
                    .any(|(_, _, _, ends, _)| ends.iter().any(|&(l, h)| l || h)) =>
            {
                // The profile's root diameter, moved by the radial clearance.
                let root = self.modeled.then_some(if is_internal {
                    major + 2.0 * radial_clearance
                } else {
                    minor - 2.0 * radial_clearance
                });
                let diameter = chamfer_diameter
                    .unwrap_or_else(|| default_chamfer_diameter(data, is_internal, root));
                if let Some(material) = extreme_diameter {
                    if !is_internal && diameter >= material {
                        return Err(format!(
                            "the thread's chamfer diameter {diameter} must be smaller than the \
                             thread's diameter {material}"
                        ));
                    }
                    if is_internal && diameter <= material {
                        return Err(format!(
                            "the thread's chamfer diameter {diameter} must be larger than the \
                             thread's diameter {material}"
                        ));
                    }
                }
                Some((angle, diameter))
            }
            _ => None,
        };
        if !self.modeled && chamfer.is_none() && taper.is_none() {
            return Ok(output);
        }
        if self.modeled {
            check_clearance(
                data.pitch,
                (major, minor, pitch_diameter),
                is_internal,
                clearance,
                flank_angle,
            )?;
        }
        for (uid, shape, faces, ends, angles) in bodies {
            // Faces turned otherwise (copies of a pattern, mitcad#733): their
            // own angles, in one operation.
            let face_angles: &[f64] = if angles.iter().all(|a| *a == angle) {
                &[]
            } else {
                &angles
            };
            let spec = ThreadSpec {
                feature,
                faces: &faces,
                pitch: data.pitch,
                major,
                minor,
                pitch_diameter,
                right_handed: self.thread.right_handed,
                angle,
                face_angles,
                part,
                radial_clearance,
                flank_clearance,
                groove: self.modeled,
                chamfer: chamfer.map(|(angle, diameter)| ThreadChamfer {
                    angle,
                    diameter,
                    ends,
                }),
                taper,
                starts,
                flank_angle,
            };
            if !self.modeled
                && spec.taper.is_none()
                && spec
                    .chamfer
                    .as_ref()
                    .is_none_or(|c| c.ends.iter().all(|&(l, h)| !l && !h))
            {
                continue;
            }
            let threaded = ctx
                .kernel
                .modeled_thread(&shape, &spec)
                .map_err(|e| e.to_string())?;
            output.push((uid, threaded));
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn rod(origin: [f64; 3], direction: [f64; 3]) -> Cylinder {
        Cylinder {
            axis: Axis { origin, direction },
            radius: 1.0,
            length: 10.0,
            internal: false,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        let d = (a - b).rem_euclid(std::f64::consts::TAU);
        d.min(std::f64::consts::TAU - d) < 1e-9
    }

    /// A modelled thread's copy is turned as the original moved there
    /// (mitcad#733), not by the original's angle about its own axis.
    #[test]
    fn copies_of_modelled_threads_turn_with_the_pattern() {
        let up = [0.0, 0.0, 1.0];
        // Moved along: the same angle.
        let along = Transform::translation([20.0, 5.0, 0.0]);
        let copy = rod([20.0, 5.0, 0.0], up);
        assert!(close(
            copied_angle(0.3, &copy, &along, (1.0, 1.0), true),
            0.3
        ));
        // Turned a quarter about the Z axis: the helix a quarter on.
        let turn = Transform::rotation([0.0; 3], up, FRAC_PI_2).unwrap();
        let copy = rod([0.0, 10.0, 0.0], up);
        assert!(close(
            copied_angle(0.3, &copy, &turn, (1.0, 1.0), true),
            0.3 + FRAC_PI_2
        ));
        // Turned over about the X axis through the rod's middle: the copy's
        // axis points down, so its low end is the original's high end; the
        // groove at angle 0 a quarter pitch above the original's low end is
        // (1, 0, 9.75), where the copy's groove, 9.5 turns on, is at pi.
        let over = Transform::rotation([0.0, 0.0, 5.0], [1.0, 0.0, 0.0], PI).unwrap();
        let copy = rod([0.0, 0.0, 10.0], [0.0, 0.0, -1.0]);
        assert!(close(copied_angle(0.0, &copy, &over, (1.0, 1.0), true), PI));
        // Mirrored across x = 20: the left-handed image's groove lies
        // across the axis from the original's, at its low end.
        let mirror = Transform::mirror([20.0, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        let copy = rod([40.0, 0.0, 0.0], up);
        assert!(close(
            copied_angle(0.0, &copy, &mirror, (1.0, 1.0), true),
            PI
        ));
        // A scale keeps the angle.
        let scale = Transform::scale([0.0; 3], [2.0, 2.0, 2.0]);
        assert!(close(
            copied_angle(0.3, &copy, &scale, (1.0, 1.0), true),
            0.3
        ));
    }
}
