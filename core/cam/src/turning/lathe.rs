// SPDX-License-Identifier: MIT
//! The lathe's own data beside the machine's (docs/cam.md, "Turning"): a
//! [`crate::Machine`] with a [`Lathe`] is a lathe. The machine's travel,
//! spindle limits, feeds, zones and park keep their meaning, in the
//! lathe's axes: machine X is the distance of the tool's controlled point
//! from the spindle axis (a radius; Y of the travel and the zones is not
//! used), Z along the spindle. The tool changer is the turret.
//!
//! A lathe with a C axis and live tools (mitcad#2067) mills and drills on
//! the part's face and diameter ([`super::live`]); its [`MillTurn`] data
//! holds the live spindle's speeds and the codes its builder gives the C
//! axis and the live tools. A Y axis mills off the spindle axis, a milling
//! head that tilts about Y (a B axis) mills and drills at an angle, and a
//! sub-spindle takes the part over for its back (mitcad#2319); the travel's
//! Y is the Y axis's then. The sub-spindle has a C axis of its own for live
//! tools on the part's back, and its speeds, travel and chuck
//! ([`SubSpindle`], mitcad#2375); its body (chuck and housing), the zones
//! moving with it, where it waits, and a control that mirrors only Z on it
//! ([`MillTurn::sub_mirror_z_only`], mitcad#2392); a revolved profile of
//! its body and the turret's body round the tool ([`TurretBody`],
//! mitcad#2405); a body per station, checked against the main chuck and
//! the machine's fixed zones too ([`StationBody`], mitcad#2412).

use serde::{Deserialize, Serialize};

/// The axes a lathe has besides X and Z: a C axis and live tools are used
/// by the live tool operations (mitcad#2067); a Y axis by the operations on
/// a plane, a tilting head by those on a tilted plane, and a sub-spindle by
/// the hand-over and the operations on the part's back (mitcad#2319).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatheAxes {
    /// The spindle can be positioned and interpolated as a C axis.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub c: bool,
    /// The turret has driven (live) tools.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live_tools: bool,
    /// The turret moves in Y.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub y: bool,
    /// A milling head that tilts about Y.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub b: bool,
    /// A sub-spindle facing the main one, which takes the part over.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sub_spindle: bool,
}

fn twelve() -> u32 {
    12
}

/// The live spindle and the codes of the C axis and the live tools
/// (mitcad#2067). Builders give these their own M codes, so each is the
/// lathe's: one block each (`c_axis_on` and `c_axis_off` a list of
/// blocks), `{rpm}` in the live spindle's blocks standing for its speed.
/// The defaults are one common set: C axis on with `M154` and to its
/// reference with `G28 H0.`, off with `M155`, the clamp `M14` and `M15`,
/// the live tool forward `M133 P{rpm}`, reverse `M134 P{rpm}`, stop
/// `M135`.
///
/// The sub-spindle's (mitcad#2319): the letter of its axis along Z (`A`),
/// the spindles synchronised `M213` and apart `M212`, its chuck closed
/// `M78` and open `M79`, the main chuck open `M69`, the sub-spindle forward
/// `M203`, reverse `M204` and stop `M205`; the B head clamped `M107` and
/// freed `M108`.
///
/// The sub-spindle's C axis (mitcad#2375), for live tools on the part's
/// back while the sub-spindle holds it: on with `M254` and to its reference
/// with `G28 H0.`, off with `M255`, its clamp `M114` and `M115`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MillTurn {
    /// The live spindle's speeds (rpm).
    pub live_min_rpm: f64,
    pub live_max_rpm: f64,
    /// Switches the main spindle to C axis mode (and references it).
    pub c_axis_on: Vec<String>,
    /// Back to the main spindle's turning mode.
    pub c_axis_off: Vec<String>,
    /// Clamps (brakes) the C axis where it is indexed, and frees it.
    pub c_clamp: String,
    pub c_unclamp: String,
    /// Starts the live tool forward (M3's sense) or in reverse at `{rpm}`.
    pub live_cw: String,
    pub live_ccw: String,
    pub live_stop: String,
    // The B head and the sub-spindle (mitcad#2319).
    /// Clamps the B head where it is tilted, and frees it.
    pub b_clamp: String,
    pub b_unclamp: String,
    /// The letter of the sub-spindle's axis along Z: `A`, `B` (on a lathe
    /// without a B head) or `E`.
    pub sub_axis: String,
    /// Synchronises the sub-spindle with the main one, and lets it go.
    pub sync_on: String,
    pub sync_off: String,
    pub sub_chuck_close: String,
    pub sub_chuck_open: String,
    pub main_chuck_open: String,
    /// Starts the sub-spindle forward (M3's sense), in reverse, stops it.
    pub sub_cw: String,
    pub sub_ccw: String,
    pub sub_stop: String,
    // The sub-spindle's C axis (mitcad#2375).
    /// Switches the sub-spindle to its C axis mode (and references it).
    pub sub_c_axis_on: Vec<String>,
    /// Back to the sub-spindle's turning mode.
    pub sub_c_axis_off: Vec<String>,
    /// Clamps the sub-spindle's C axis where it is indexed, and frees it.
    pub sub_c_clamp: String,
    pub sub_c_unclamp: String,
    // The sub-spindle's coordinates on the control (mitcad#2392).
    /// The control mirrors only Z on the sub-spindle: X, Y and C keep the
    /// main spindle's senses there (C from X towards the machine's Y), so
    /// the post writes the live tools' Y and C turned round and their arcs
    /// across Y the other way. Off (the default): the control turns the
    /// part round about X, Y and Z both turned round.
    pub sub_mirror_z_only: bool,
}

impl Default for MillTurn {
    fn default() -> Self {
        MillTurn {
            live_min_rpm: 0.0,
            live_max_rpm: 6000.0,
            c_axis_on: vec!["M154".to_owned(), "G28 H0.".to_owned()],
            c_axis_off: vec!["M155".to_owned()],
            c_clamp: "M14".to_owned(),
            c_unclamp: "M15".to_owned(),
            live_cw: "M133 P{rpm}".to_owned(),
            live_ccw: "M134 P{rpm}".to_owned(),
            live_stop: "M135".to_owned(),
            b_clamp: "M107".to_owned(),
            b_unclamp: "M108".to_owned(),
            sub_axis: "A".to_owned(),
            sync_on: "M213".to_owned(),
            sync_off: "M212".to_owned(),
            sub_chuck_close: "M78".to_owned(),
            sub_chuck_open: "M79".to_owned(),
            main_chuck_open: "M69".to_owned(),
            sub_cw: "M203".to_owned(),
            sub_ccw: "M204".to_owned(),
            sub_stop: "M205".to_owned(),
            sub_c_axis_on: vec!["M254".to_owned(), "G28 H0.".to_owned()],
            sub_c_axis_off: vec!["M255".to_owned()],
            sub_c_clamp: "M114".to_owned(),
            sub_c_unclamp: "M115".to_owned(),
            sub_mirror_z_only: false,
        }
    }
}

impl MillTurn {
    /// Whether it is the default set (left out of the file then).
    pub fn is_default(&self) -> bool {
        *self == MillTurn::default()
    }

    /// The live spindle's block for a speed and sense.
    pub fn live_start(&self, rpm: f64, cw: bool) -> String {
        let code = if cw { &self.live_cw } else { &self.live_ccw };
        code.replace("{rpm}", &format!("{}", rpm.round() as i64))
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.live_min_rpm.is_finite()
            && self.live_min_rpm >= 0.0
            && self.live_max_rpm.is_finite()
            && self.live_max_rpm >= self.live_min_rpm
            && self.live_max_rpm > 0.0)
        {
            return Err(
                "The live spindle's speeds must run from a minimum up to a maximum above zero."
                    .to_owned(),
            );
        }
        let block = |text: &str, speed: bool, what: &str| -> Result<(), String> {
            let t = text.trim();
            let printable = t
                .chars()
                .all(|c| c.is_ascii() && !c.is_ascii_control() && c != '%');
            if t.is_empty() || t.len() > 120 || !printable {
                return Err(format!(
                    "The code that {what} must be one block of printable characters."
                ));
            }
            // `{rpm}` is the only name filled in, and only in the live
            // spindle's blocks.
            let rest = if speed {
                t.replace("{rpm}", "")
            } else {
                t.to_owned()
            };
            if rest.contains(['{', '}']) {
                return Err(format!(
                    "The code that {what} names something MitCad does not fill in: only {{rpm}} in the live spindle's codes."
                ));
            }
            Ok(())
        };
        if self.c_axis_on.is_empty() || self.c_axis_on.len() > 8 {
            return Err("The C axis needs one to eight blocks that switch it on.".to_owned());
        }
        if self.c_axis_off.is_empty() || self.c_axis_off.len() > 8 {
            return Err("The C axis needs one to eight blocks that switch it off.".to_owned());
        }
        for line in &self.c_axis_on {
            block(line, false, "switches the C axis on")?;
        }
        for line in &self.c_axis_off {
            block(line, false, "switches the C axis off")?;
        }
        block(&self.c_clamp, false, "clamps the C axis")?;
        block(&self.c_unclamp, false, "frees the C axis")?;
        block(&self.live_cw, true, "starts the live tool")?;
        block(&self.live_ccw, true, "starts the live tool in reverse")?;
        block(&self.live_stop, false, "stops the live tool")?;
        // The B head and the sub-spindle (mitcad#2319).
        block(&self.b_clamp, false, "clamps the B head")?;
        block(&self.b_unclamp, false, "frees the B head")?;
        block(&self.sync_on, false, "synchronises the spindles")?;
        block(&self.sync_off, false, "lets the spindles go")?;
        block(
            &self.sub_chuck_close,
            false,
            "closes the sub-spindle's chuck",
        )?;
        block(&self.sub_chuck_open, false, "opens the sub-spindle's chuck")?;
        block(&self.main_chuck_open, false, "opens the main chuck")?;
        block(&self.sub_cw, false, "starts the sub-spindle")?;
        block(&self.sub_ccw, false, "starts the sub-spindle in reverse")?;
        block(&self.sub_stop, false, "stops the sub-spindle")?;
        if !matches!(self.sub_axis.as_str(), "A" | "B" | "E") {
            return Err("The sub-spindle's axis is one of the letters A, B and E.".to_owned());
        }
        // The sub-spindle's C axis (mitcad#2375).
        if self.sub_c_axis_on.is_empty() || self.sub_c_axis_on.len() > 8 {
            return Err(
                "The sub-spindle's C axis needs one to eight blocks that switch it on.".to_owned(),
            );
        }
        if self.sub_c_axis_off.is_empty() || self.sub_c_axis_off.len() > 8 {
            return Err(
                "The sub-spindle's C axis needs one to eight blocks that switch it off.".to_owned(),
            );
        }
        for line in &self.sub_c_axis_on {
            block(line, false, "switches the sub-spindle's C axis on")?;
        }
        for line in &self.sub_c_axis_off {
            block(line, false, "switches the sub-spindle's C axis off")?;
        }
        block(&self.sub_c_clamp, false, "clamps the sub-spindle's C axis")?;
        block(&self.sub_c_unclamp, false, "frees the sub-spindle's C axis")?;
        Ok(())
    }

    /// The blocks that switch the C axis of the main spindle or (`sub`) the
    /// sub-spindle on (mitcad#2375).
    pub fn c_on(&self, sub: bool) -> &[String] {
        if sub {
            &self.sub_c_axis_on
        } else {
            &self.c_axis_on
        }
    }

    /// The blocks that switch that C axis off.
    pub fn c_off(&self, sub: bool) -> &[String] {
        if sub {
            &self.sub_c_axis_off
        } else {
            &self.c_axis_off
        }
    }

    /// That C axis's clamp.
    pub fn clamp(&self, sub: bool) -> &str {
        if sub {
            self.sub_c_clamp.trim()
        } else {
            self.c_clamp.trim()
        }
    }

    /// That C axis's unclamp.
    pub fn unclamp(&self, sub: bool) -> &str {
        if sub {
            self.sub_c_unclamp.trim()
        } else {
            self.c_unclamp.trim()
        }
    }

    /// The sub-spindle's axis letter.
    pub fn sub_letter(&self) -> char {
        self.sub_axis.chars().next().unwrap_or('A')
    }

    /// The sub-spindle's start block for a sense.
    pub fn sub_start(&self, cw: bool) -> &str {
        if cw { &self.sub_cw } else { &self.sub_ccw }
    }
}

/// The sub-spindle's own data (mitcad#2375), each left out of the file at
/// its default: its speeds (a highest speed of zero: the main spindle's
/// range), its travel (the machine Z of its jaws' face, from its lowest to
/// its highest; none: not checked) and its chuck's diameter (checked as the
/// main chuck is: no tool behind its jaws' face within it).
///
/// Its body and the zones moving with it (mitcad#2392): the chuck reaches
/// `chuck_length` back from its jaws' face (zero: without end, as before),
/// its housing a cylinder of `housing_diameter` from the chuck's back,
/// `housing_length` long (zero: without end); `zones` are boxes that move
/// with it, their Z measured from its jaws' face (positive away from the
/// main spindle), X the machine's (a radius; checked in X and Z as a
/// lathe's zones are entered, their Y kept; on a lathe with a Y axis Y is
/// the machine's and checked, turning at its Y 0, mitcad#2412). While the
/// main spindle cuts alone its jaws' face waits at `park` (machine Z), else at
/// the far end of its travel; with neither its body is checked only where
/// it holds the part.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubSpindle {
    #[serde(skip_serializing_if = "is_zero")]
    pub min_rpm: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub max_rpm: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub travel: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "is_zero")]
    pub chuck_diameter: f64,
    // Its body and the zones moving with it (mitcad#2392).
    #[serde(skip_serializing_if = "is_zero")]
    pub chuck_length: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub housing_diameter: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub housing_length: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub park: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<crate::machine::Zone>,
    // A revolved profile of its own (mitcad#2405).
    /// Its body's profile beside the chuck and the housing: `[z, diameter]`
    /// points, z from its jaws' face (positive away from the main spindle,
    /// negative for a nose in front of it), in order along z; two points at
    /// one z make a step. The body is what the profile encloses about the
    /// spindle axis from its first z to its last. Empty: none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub profile: Vec<[f64; 2]>,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// A part of the sub-spindle's body (mitcad#2392); its own revolved
/// profile (mitcad#2405).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubBody {
    Chuck,
    Housing,
    Profile,
}

impl SubBody {
    /// The finding of an operation's tool running into it, in words.
    pub fn finding(self, name: &str) -> String {
        match self {
            SubBody::Chuck => format!("{name}: the tool runs into the sub-spindle's chuck."),
            SubBody::Housing => format!("{name}: the tool runs into the sub-spindle's housing."),
            SubBody::Profile => format!("{name}: the tool runs into the sub-spindle's body."),
        }
    }

    /// The finding of the turret's body running into it (mitcad#2405).
    pub fn turret_finding(self, name: &str) -> String {
        match self {
            SubBody::Chuck => format!("{name}: the turret runs into the sub-spindle's chuck."),
            SubBody::Housing => format!("{name}: the turret runs into the sub-spindle's housing."),
            SubBody::Profile => format!("{name}: the turret runs into the sub-spindle's body."),
        }
    }
}

/// How far a part reaches when it has no end (a length of zero) as the
/// view draws it: its diameter, at least this (mm, mitcad#2405).
const ENDLESS_DRAWN: f64 = 20.0;

/// The turret's body round the tool (mitcad#2405): a box from the tool's
/// controlled point, `x` along the machine's X (a radius, positive away
/// from the spindle axis) and `z` along its Z (positive away from the main
/// spindle), each from its lowest to its highest; like a lathe's zones in X
/// and Z only, at the tool's Y. Checked against the sub-spindle's body and
/// the zones moving with it, and against the main chuck and the machine's
/// fixed zones (mitcad#2412).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurretBody {
    pub x: [f64; 2],
    pub z: [f64; 2],
}

impl TurretBody {
    pub fn validate(&self) -> Result<(), String> {
        let ordered = |r: [f64; 2]| r[0].is_finite() && r[1].is_finite() && r[0] <= r[1];
        if !(ordered(self.x) && ordered(self.z)) {
            return Err(
                "The turret's body needs X and Z from the tool, each from a lowest up to a highest."
                    .to_owned(),
            );
        }
        Ok(())
    }

    /// Its X and Z on the machine with the tool's controlled point at
    /// machine X `x` and at `z` along Z.
    pub fn at(&self, x: f64, z: f64) -> ([f64; 2], [f64; 2]) {
        (
            [x + self.x[0], x + self.x[1]],
            [z + self.z[0], z + self.z[1]],
        )
    }
}

/// The turret's body round the tools at one station (mitcad#2412), as
/// [`TurretBody`]: X and Z from the tool's controlled point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StationBody {
    pub station: u32,
    pub x: [f64; 2],
    pub z: [f64; 2],
}

impl StationBody {
    /// Its box round the tool.
    pub fn body(&self) -> TurretBody {
        TurretBody {
            x: self.x,
            z: self.z,
        }
    }
}

/// The least distance from zero of the values from `r[0]` to `r[1]`.
pub(crate) fn nearest_zero(r: [f64; 2]) -> f64 {
    if r[0] <= 0.0 && r[1] >= 0.0 {
        0.0
    } else {
        r[0].abs().min(r[1].abs())
    }
}

impl SubSpindle {
    /// Whether it is all defaults (left out of the file then).
    pub fn is_default(&self) -> bool {
        *self == SubSpindle::default()
    }

    /// Its speed range (rpm): its own, else the main spindle's
    /// (`main`: lowest and highest).
    pub fn speeds(&self, main: [f64; 2]) -> [f64; 2] {
        if self.max_rpm > 0.0 {
            [self.min_rpm, self.max_rpm]
        } else {
            main
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.min_rpm.is_finite()
            && self.min_rpm >= 0.0
            && self.max_rpm.is_finite()
            && self.max_rpm >= 0.0
            && (self.max_rpm == 0.0 || self.max_rpm >= self.min_rpm))
        {
            return Err(
                "The sub-spindle's speeds must run from a minimum up to a maximum, or be left at zero."
                    .to_owned(),
            );
        }
        if let Some([lo, hi]) = self.travel
            && !(lo.is_finite() && hi.is_finite() && lo <= hi)
        {
            return Err(
                "The sub-spindle's travel must run from its lowest Z up to its highest.".to_owned(),
            );
        }
        if !(self.chuck_diameter.is_finite() && self.chuck_diameter >= 0.0) {
            return Err("The sub-spindle's chuck diameter must be zero or more.".to_owned());
        }
        // Its body and the zones moving with it (mitcad#2392).
        let size = |v: f64| v.is_finite() && v >= 0.0;
        if !(size(self.chuck_length) && size(self.housing_diameter) && size(self.housing_length)) {
            return Err(
                "The sub-spindle's chuck length and its housing's size must be zero or more."
                    .to_owned(),
            );
        }
        if self.park.is_some_and(|z| !z.is_finite()) {
            return Err(
                "Where the sub-spindle waits must be a machine Z of its jaws' face.".to_owned(),
            );
        }
        for zone in &self.zones {
            let b = &zone.bounds;
            let ordered = (0..3)
                .all(|k| b.min[k].is_finite() && b.max[k].is_finite() && b.min[k] <= b.max[k]);
            if zone.name.trim().is_empty() || !ordered {
                return Err(
                    "A zone that moves with the sub-spindle needs a name and bounds from a minimum up to a maximum."
                        .to_owned(),
                );
            }
        }
        // Its own profile (mitcad#2405).
        if !self.profile.is_empty() {
            let points = self
                .profile
                .iter()
                .all(|p| p[0].is_finite() && p[1].is_finite() && p[1] >= 0.0);
            let ordered = self.profile.windows(2).all(|w| w[0][0] <= w[1][0]);
            if self.profile.len() < 2 || !points || !ordered {
                return Err(
                    "The sub-spindle's profile needs two or more points in order along Z, each with a diameter of zero or more."
                        .to_owned(),
                );
            }
        }
        Ok(())
    }

    /// The radius of its profile at `behind` (mitcad#2405): the largest
    /// where a step stands there; none outside the profile.
    pub fn profile_radius(&self, behind: f64) -> Option<f64> {
        const EPS: f64 = 1e-9;
        let mut out: Option<f64> = None;
        for w in self.profile.windows(2) {
            let ([z0, d0], [z1, d1]) = (w[0], w[1]);
            if behind < z0 - EPS || behind > z1 + EPS {
                continue;
            }
            let d = if z1 - z0 <= EPS {
                d0.max(d1)
            } else {
                let t = ((behind - z0) / (z1 - z0)).clamp(0.0, 1.0);
                d0 + (d1 - d0) * t
            };
            out = Some(out.map_or(d / 2.0, |r: f64| r.max(d / 2.0)));
        }
        out
    }

    /// The largest radius of its profile from `behind[0]` to `behind[1]`
    /// (mitcad#2405); none where the profile is not.
    fn profile_radius_across(&self, behind: [f64; 2]) -> Option<f64> {
        let (first, last) = (self.profile.first()?, self.profile.last()?);
        let lo = behind[0].max(first[0]);
        let hi = behind[1].min(last[0]);
        if lo > hi + 1e-9 {
            return None;
        }
        // Linear between its points: the largest at an end or at a point.
        let mut best = self.profile_radius(lo)?.max(self.profile_radius(hi)?);
        for p in &self.profile {
            if p[0] >= lo - 1e-9 && p[0] <= hi + 1e-9 {
                best = best.max(p[1] / 2.0);
            }
        }
        Some(best)
    }

    /// The parts of its body a box reaches into (mitcad#2405, the turret's
    /// body against it): `r_min` the box's least distance from the spindle
    /// axis, `behind` how far it reaches behind the jaws' face, from its
    /// nearest to its farthest.
    pub fn body_across(&self, r_min: f64, behind: [f64; 2]) -> Vec<SubBody> {
        const EPS: f64 = 1e-9;
        let mut out = Vec::new();
        // The jaws' face itself is free, as for a point.
        if behind[1] > EPS {
            if self.chuck_diameter > 0.0
                && r_min < self.chuck_diameter / 2.0 + EPS
                && (self.chuck_length <= 0.0 || behind[0] <= self.chuck_length + EPS)
            {
                out.push(SubBody::Chuck);
            }
            let start = self.chuck_length.max(0.0);
            if self.housing_diameter > 0.0
                && r_min < self.housing_diameter / 2.0 + EPS
                && behind[1] >= start - EPS
                && (self.housing_length <= 0.0 || behind[0] <= start + self.housing_length + EPS)
            {
                out.push(SubBody::Housing);
            }
        }
        if self
            .profile_radius_across(behind)
            .is_some_and(|r| r_min < r + EPS)
        {
            out.push(SubBody::Profile);
        }
        out
    }

    /// Its body as the view draws it (mitcad#2405): each part (the chuck,
    /// the housing, its profile) as `[behind, radius]` points along it, a
    /// part without end as long as its diameter (at least 20 mm).
    pub fn outline(&self) -> Vec<Vec<[f64; 2]>> {
        let drawn = |length: f64, diameter: f64| {
            if length > 0.0 {
                length
            } else {
                diameter.max(ENDLESS_DRAWN)
            }
        };
        let mut out = Vec::new();
        if self.chuck_diameter > 0.0 {
            let (l, r) = (
                drawn(self.chuck_length, self.chuck_diameter),
                self.chuck_diameter / 2.0,
            );
            out.push(vec![[0.0, r], [l, r]]);
        }
        if self.housing_diameter > 0.0 {
            let start = self.chuck_length.max(0.0);
            let (l, r) = (
                drawn(self.housing_length, self.housing_diameter),
                self.housing_diameter / 2.0,
            );
            out.push(vec![[start, r], [start + l, r]]);
        }
        if self.profile.len() >= 2 {
            out.push(self.profile.iter().map(|p| [p[0], p[1] / 2.0]).collect());
        }
        out
    }

    /// Where its jaws' face waits while the main spindle cuts alone
    /// (machine Z, mitcad#2392): its `park`, else the far end of its
    /// travel; none when neither is known.
    pub fn waiting_z(&self) -> Option<f64> {
        self.park.or(self.travel.map(|t| t[1]))
    }

    /// The part of its body a point is in (mitcad#2392): `r` the point's
    /// distance from the spindle axis, `behind` how far it is behind the
    /// jaws' face (away from the main spindle). The jaws' face itself is
    /// free (the part is gripped there); its own profile (mitcad#2405) is
    /// where it says, a nose in front of the face too.
    pub fn body_at(&self, r: f64, behind: f64) -> Option<SubBody> {
        const EPS: f64 = 1e-9;
        if behind <= EPS {
            return self
                .profile_radius(behind)
                .filter(|rp| r < rp + EPS)
                .map(|_| SubBody::Profile);
        }
        if self.chuck_diameter > 0.0
            && r < self.chuck_diameter / 2.0 + EPS
            && (self.chuck_length <= 0.0 || behind <= self.chuck_length + EPS)
        {
            return Some(SubBody::Chuck);
        }
        let start = self.chuck_length.max(0.0);
        if self.housing_diameter > 0.0
            && r < self.housing_diameter / 2.0 + EPS
            && behind >= start - EPS
            && (self.housing_length <= 0.0 || behind <= start + self.housing_length + EPS)
        {
            return Some(SubBody::Housing);
        }
        if self.profile_radius(behind).is_some_and(|rp| r < rp + EPS) {
            return Some(SubBody::Profile);
        }
        None
    }

    /// Whether it has a body or zones to check (mitcad#2392), its own
    /// profile too (mitcad#2405).
    pub fn has_body(&self) -> bool {
        self.chuck_diameter > 0.0
            || self.housing_diameter > 0.0
            || !self.zones.is_empty()
            || !self.profile.is_empty()
    }
}

/// What makes a machine a lathe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lathe {
    /// The turret's stations (a tool's number is its station).
    #[serde(default = "twelve")]
    pub turret_stations: u32,
    /// The chuck's diameter and how far its jaws reach out along Z from
    /// the face they grip at; the setup's placement puts it on the
    /// machine. Checked as a zone.
    #[serde(default)]
    pub chuck_diameter: f64,
    #[serde(default)]
    pub chuck_length: f64,
    #[serde(default)]
    pub axes: LatheAxes,
    /// The live spindle and the C axis's codes (mitcad#2067).
    #[serde(default, skip_serializing_if = "MillTurn::is_default")]
    pub mill_turn: MillTurn,
    /// The sub-spindle's speeds, travel and chuck (mitcad#2375).
    #[serde(default, skip_serializing_if = "SubSpindle::is_default")]
    pub sub_spindle: SubSpindle,
    /// The turret's body round the tool (mitcad#2405); none: only the
    /// tool's controlled point is checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret_body: Option<TurretBody>,
    /// The turret's body of single stations (mitcad#2412), each in place
    /// of `turret_body` for the tools at its station.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub station_bodies: Vec<StationBody>,
}

impl Default for Lathe {
    fn default() -> Self {
        Lathe {
            turret_stations: twelve(),
            chuck_diameter: 0.0,
            chuck_length: 0.0,
            axes: LatheAxes::default(),
            mill_turn: MillTurn::default(),
            sub_spindle: SubSpindle::default(),
            turret_body: None,
            station_bodies: Vec::new(),
        }
    }
}

impl Lathe {
    /// Whether it mills and drills with live tools on the C axis.
    pub fn mills(&self) -> bool {
        self.axes.c && self.axes.live_tools
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=99).contains(&self.turret_stations) {
            return Err("A turret has 1 to 99 stations.".to_owned());
        }
        if !(self.chuck_diameter.is_finite()
            && self.chuck_diameter >= 0.0
            && self.chuck_length.is_finite()
            && self.chuck_length >= 0.0)
        {
            return Err("The chuck's size must be zero or more.".to_owned());
        }
        self.mill_turn.validate()?;
        self.sub_spindle.validate()?;
        if let Some(body) = &self.turret_body {
            body.validate()?;
        }
        // The bodies of single stations (mitcad#2412): each on the turret,
        // once.
        for (k, body) in self.station_bodies.iter().enumerate() {
            let taken = self.station_bodies[..k]
                .iter()
                .any(|b| b.station == body.station);
            if !(1..=self.turret_stations).contains(&body.station) || taken {
                return Err(
                    "A station's turret body needs a station on the turret, each station once."
                        .to_owned(),
                );
            }
            body.body().validate()?;
        }
        // A B head and a sub-spindle each need a letter of their own
        // (mitcad#2319).
        if self.axes.b && self.axes.sub_spindle && self.mill_turn.sub_letter() == 'B' {
            return Err(
                "The B head has the letter B: give the sub-spindle's axis A or E.".to_owned(),
            );
        }
        if (self.axes.y || self.axes.b) && !self.mills() {
            return Err("A Y axis or a B head mills with live tools on a C axis.".to_owned());
        }
        Ok(())
    }

    /// Whether it mills off the spindle axis with a Y axis (mitcad#2319).
    pub fn has_y(&self) -> bool {
        self.mills() && self.axes.y
    }

    /// Whether its milling head tilts (a B axis, mitcad#2319).
    pub fn has_b(&self) -> bool {
        self.mills() && self.axes.b
    }

    /// The turret's body round a tool at `station` (mitcad#2412): the
    /// station's own, else the one of every station; none: only the tool's
    /// controlled point is checked.
    pub fn turret_body_at(&self, station: u32) -> Option<TurretBody> {
        self.station_bodies
            .iter()
            .find(|b| b.station == station)
            .map(StationBody::body)
            .or(self.turret_body)
    }
}
