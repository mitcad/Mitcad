// SPDX-License-Identifier: MIT
//! Thread: a screw thread on cylindrical faces.
//! A cosmetic thread changes no geometry; it is data on the faces that the
//! `threads` query lists for display. A modelled thread builds the
//! 60-degree profile on the faces (ISO metric and Unified threads;
//! Whitworth and NPT threads are cosmetic only): the basic profile of the
//! size, or the profile of given diameters (a tolerance class's, as .f3d
//! designs store them), turned about its axis by an angle.

use serde::{Deserialize, Serialize};

use super::reference::FaceRef;
use super::thread_table::{self, ThreadData, ThreadStandard};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{Kernel, ThreadSpec};
use crate::parameters::ParamId;
use crate::topo::FaceName;

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
        Ok(data)
    }
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
    /// A modelled thread's diameters; the size's basic ones (D, D1, D2)
    /// without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub diameters: Option<ThreadDiameters<P>>,
    /// A modelled thread turned about its axis (see `ThreadSpec`); 0
    /// without.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
}

impl<P> ThreadDef<P> {
    pub const TYPE: &'static str = "thread";
    pub const BASE_NAME: &'static str = "Thread";

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
        })
    }
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
        if self.modeled {
            self.thread.standard.check_modeled()?;
        } else if self.diameters.is_some() || self.angle.is_some() {
            return Err("only a modelled thread has diameters and an angle".to_owned());
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
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for ThreadDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let part = thread_part(
            ctx,
            self.length,
            self.offset,
            self.location == ThreadEnd::HighEnd,
        )?;
        // The profile's diameters: given, or the size's basic ones.
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
        // The faces by body, each a cylinder, all on one side of their material.
        let mut bodies: Vec<(BodyUid, K::Shape, Vec<FaceName>)> = Vec::new();
        let mut internal = None;
        for face in &self.faces {
            let shape = ctx.body(face.body)?;
            if ctx
                .kernel
                .count_faces(&shape, &face.face)
                .map_err(|e| e.to_string())?
                == 0
            {
                return Err(format!("body {} has no face {}", face.body, face.face));
            }
            let cylinder = ctx
                .kernel
                .face_cylinder(&shape, &face.face)
                .map_err(|e| e.to_string())?;
            if internal.is_some_and(|i| i != cylinder.internal) {
                return Err("the faces of a thread must all be internal or all external".to_owned());
            }
            internal = Some(cylinder.internal);
            let data = self.thread.check(internal)?;
            // The size must fit the cylinder: an external thread's root lies
            // inside it, an internal thread's root outside the bore.
            let diameter = 2.0 * cylinder.radius;
            let (major, minor, _) =
                diameters.unwrap_or((data.major, data.minor, data.pitch_diameter));
            let fits = if cylinder.internal {
                diameter < major
            } else {
                diameter > minor
            };
            if self.modeled && !fits {
                return Err(format!(
                    "thread {} does not fit face {} of diameter {diameter}",
                    data.designation, face.face
                ));
            }
            match bodies.iter_mut().find(|(uid, _, _)| *uid == face.body) {
                Some((_, _, faces)) => faces.push(face.face.clone()),
                None => bodies.push((face.body, shape, vec![face.face.clone()])),
            }
        }
        let mut output = FeatureOutput::default();
        if !self.modeled {
            return Ok(output);
        }
        let data = self.thread.data()?;
        let (major, minor, pitch_diameter) =
            diameters.unwrap_or((data.major, data.minor, data.pitch_diameter));
        for (uid, shape, faces) in bodies {
            let spec = ThreadSpec {
                feature: ctx.uid,
                faces: &faces,
                pitch: data.pitch,
                major,
                minor,
                pitch_diameter,
                right_handed: self.thread.right_handed,
                angle,
                part,
            };
            let threaded = ctx
                .kernel
                .modeled_thread(&shape, &spec)
                .map_err(|e| e.to_string())?;
            output.changes.push(BodyChange::Set(uid, threaded));
        }
        Ok(output)
    }
}
