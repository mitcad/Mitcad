// SPDX-License-Identifier: MIT
//! The lathe's own data beside the machine's (docs/cam.md, "Turning"): a
//! [`crate::Machine`] with a [`Lathe`] is a lathe. The machine's travel,
//! spindle limits, feeds, zones and park keep their meaning, in the
//! lathe's axes: machine X is the distance of the tool's controlled point
//! from the spindle axis (a radius; Y of the travel and the zones is not
//! used), Z along the spindle. The tool changer is the turret.

use serde::{Deserialize, Serialize};

/// The axes a lathe has besides X and Z. Only X and Z are used for now; the
/// others are kept for C-axis work, live tools, a Y axis and a tilting
/// head.
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
}

fn twelve() -> u32 {
    12
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
}

impl Default for Lathe {
    fn default() -> Self {
        Lathe {
            turret_stations: twelve(),
            chuck_diameter: 0.0,
            chuck_length: 0.0,
            axes: LatheAxes::default(),
        }
    }
}

impl Lathe {
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
        Ok(())
    }
}
