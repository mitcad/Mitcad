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
//! Y is the Y axis's then.

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
        Ok(())
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
}

impl Default for Lathe {
    fn default() -> Self {
        Lathe {
            turret_stations: twelve(),
            chuck_diameter: 0.0,
            chuck_length: 0.0,
            axes: LatheAxes::default(),
            mill_turn: MillTurn::default(),
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
}
