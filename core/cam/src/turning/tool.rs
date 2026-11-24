// SPDX-License-Identifier: MIT
//! Turning tools: an insert in a holder on a turret station.

use serde::{Deserialize, Serialize};

/// What a turning tool cuts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurningToolKind {
    /// Turning and facing of outside diameters: a nose radius, cutting
    /// towards the chuck.
    Turning,
    /// Boring of inside diameters: a bar no thicker than `bar_diameter`, so
    /// that it fits bores above that.
    Boring { bar_diameter: f64 },
    /// A grooving insert `width` wide; `inside` for bores.
    Grooving {
        width: f64,
        #[serde(default)]
        inside: bool,
    },
    /// A parting blade `width` wide.
    Parting { width: f64 },
    /// A threading insert with its included `angle` (degrees, 60 for
    /// metric and unified threads); `inside` for bores.
    Threading {
        angle: f64,
        #[serde(default)]
        inside: bool,
    },
    /// A driven milling tool or drill on its station (mitcad#2067): it
    /// cuts in a live tool operation ([`super::live::LiveOp`]), whose
    /// cutter it carries; it takes no part in turning.
    Live,
}

/// A turning tool. Its `number` is its turret station; `offset` the
/// controller's offset number (the station's when 0).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurningTool {
    pub number: u32,
    #[serde(default)]
    pub offset: u32,
    pub name: String,
    pub kind: TurningToolKind,
    /// The insert's nose radius (mm), 0 for a sharp corner. A grooving or
    /// parting insert's corners have it too.
    #[serde(default)]
    pub nose_radius: f64,
    /// How deep the insert can cut in one pass (radius, mm); a grooving
    /// insert's reach.
    pub max_depth: f64,
}

impl TurningTool {
    /// The offset number written with the station (`T0101`).
    pub fn offset_number(&self) -> u32 {
        if self.offset == 0 {
            self.number
        } else {
            self.offset
        }
    }

    /// Whether it cuts inside a bore.
    pub fn inside(&self) -> bool {
        match self.kind {
            TurningToolKind::Boring { .. } => true,
            TurningToolKind::Grooving { inside, .. }
            | TurningToolKind::Threading { inside, .. } => inside,
            TurningToolKind::Turning | TurningToolKind::Parting { .. } | TurningToolKind::Live => {
                false
            }
        }
    }

    /// The width of a grooving or parting insert; 0 for the others.
    pub fn width(&self) -> f64 {
        match self.kind {
            TurningToolKind::Grooving { width, .. } | TurningToolKind::Parting { width } => width,
            _ => 0.0,
        }
    }

    /// The kind in words, for messages and default names.
    pub fn words(&self) -> &'static str {
        match self.kind {
            TurningToolKind::Turning => "turning tool",
            TurningToolKind::Boring { .. } => "boring bar",
            TurningToolKind::Grooving { .. } => "grooving tool",
            TurningToolKind::Parting { .. } => "parting tool",
            TurningToolKind::Threading { .. } => "threading tool",
            TurningToolKind::Live => "live tool",
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let n = self.number;
        if !(1..=99).contains(&n) {
            return Err("A turret station must be from 1 to 99.".to_owned());
        }
        if self.offset > 99 {
            return Err(format!("Tool {n}'s offset number must be from 1 to 99."));
        }
        if !(self.nose_radius.is_finite() && self.nose_radius >= 0.0) {
            return Err(format!("Tool {n}'s nose radius must be zero or more."));
        }
        if !(self.max_depth.is_finite() && self.max_depth > 0.0) {
            return Err(format!("Tool {n} needs a depth of cut above zero."));
        }
        match self.kind {
            TurningToolKind::Boring { bar_diameter } => {
                if !(bar_diameter.is_finite() && bar_diameter > 0.0) {
                    return Err(format!("Tool {n}'s bar needs a diameter above zero."));
                }
            }
            TurningToolKind::Grooving { width, .. } | TurningToolKind::Parting { width } => {
                if !(width.is_finite() && width > 0.0 && 2.0 * self.nose_radius <= width) {
                    return Err(format!(
                        "Tool {n}'s insert needs a width above zero and at least twice its corner radius."
                    ));
                }
            }
            TurningToolKind::Threading { angle, .. } => {
                if !(angle > 0.0 && angle < 120.0) {
                    return Err(format!(
                        "Tool {n}'s thread angle must be between 0 and 120 degrees."
                    ));
                }
            }
            TurningToolKind::Turning | TurningToolKind::Live => {}
        }
        Ok(())
    }
}
