// SPDX-License-Identifier: MIT
//! Cutting tools and the tool library (docs/cam.md, "Tools").

use serde::{Deserialize, Serialize};

/// The shape of a tool's cutting end.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolShape {
    /// A flat end mill (square end).
    FlatEndMill,
    /// A ball end mill: the end is a half sphere of the tool's radius.
    BallEndMill,
    /// A bull nose end mill: a flat end with rounded corners.
    BullNose { corner_radius: f64 },
    /// A twist drill with its point's included angle (degrees).
    Drill { point_angle: f64 },
    /// A V-bit (engraving, chamfers) with its included angle (degrees) and
    /// the diameter of its flat tip (0 for a sharp point).
    VBit { angle: f64, tip_diameter: f64 },
}

impl ToolShape {
    /// Whether the end cuts a flat floor across the tool's full diameter
    /// less its corner radius, which the 2.5-axis wall operations assume.
    pub fn flat_bottom(&self) -> bool {
        matches!(self, ToolShape::FlatEndMill | ToolShape::BullNose { .. })
    }
}

/// Speeds and feeds of a tool or an operation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cutting {
    /// Spindle speed (rpm).
    pub spindle_rpm: f64,
    /// Cutting feed (mm/min).
    pub feed: f64,
    /// Feed of plunges and ramps (mm/min).
    pub plunge_feed: f64,
}

impl Cutting {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.spindle_rpm.is_finite() && self.spindle_rpm >= 0.0) {
            return Err("The spindle speed must be zero or more.".into());
        }
        if !(self.feed.is_finite() && self.feed > 0.0) {
            return Err("The feed must be more than zero.".into());
        }
        if !(self.plunge_feed.is_finite() && self.plunge_feed > 0.0) {
            return Err("The plunge feed must be more than zero.".into());
        }
        Ok(())
    }
}

/// A cutting tool as the controller's tool table knows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// The tool number (`T`); also its slot in a tool changer.
    pub number: u32,
    pub name: String,
    pub shape: ToolShape,
    /// Cutting diameter (mm).
    pub diameter: f64,
    /// Length of the flutes (mm): the deepest cut in one pass.
    pub flute_length: f64,
    /// Stick-out from the holder or collet to the tip (mm), for the
    /// collision checks and the tool length.
    pub length: f64,
    #[serde(default = "two")]
    pub flutes: u32,
    /// Default speeds and feeds for operations that use the tool.
    #[serde(default)]
    pub cutting: Option<Cutting>,
}

fn two() -> u32 {
    2
}

impl Tool {
    /// A flat end mill with the usual proportions: flutes three times the
    /// diameter, stick-out a little longer.
    pub fn flat_end_mill(number: u32, diameter: f64) -> Tool {
        Tool {
            number,
            name: format!("{} mm flat end mill", fmt_mm(diameter)),
            shape: ToolShape::FlatEndMill,
            diameter,
            flute_length: diameter * 3.0,
            length: diameter * 3.0 + 5.0,
            flutes: 2,
            cutting: None,
        }
    }

    pub fn radius(&self) -> f64 {
        self.diameter / 2.0
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.diameter.is_finite() && self.diameter > 0.0) {
            return Err(format!("Tool {} has no diameter.", self.number));
        }
        if !(self.flute_length.is_finite() && self.flute_length > 0.0) {
            return Err(format!("Tool {} has no flute length.", self.number));
        }
        if !(self.length.is_finite() && self.length >= self.flute_length) {
            return Err(format!(
                "Tool {} sticks out less than its flutes are long.",
                self.number
            ));
        }
        if let ToolShape::BullNose { corner_radius } = self.shape
            && !(corner_radius > 0.0 && corner_radius <= self.radius())
        {
            return Err(format!(
                "Tool {}'s corner radius must be within its radius.",
                self.number
            ));
        }
        Ok(())
    }
}

/// A number in millimetres without trailing zeros (for names).
pub(crate) fn fmt_mm(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// The tools of a machine or a project, by number.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolLibrary {
    pub tools: Vec<Tool>,
}

impl ToolLibrary {
    pub fn get(&self, number: u32) -> Option<&Tool> {
        self.tools.iter().find(|t| t.number == number)
    }

    /// Adds a tool or replaces the one with its number.
    pub fn put(&mut self, tool: Tool) {
        match self.tools.iter_mut().find(|t| t.number == tool.number) {
            Some(t) => *t = tool,
            None => {
                self.tools.push(tool);
                self.tools.sort_by_key(|t| t.number);
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        for (i, t) in self.tools.iter().enumerate() {
            t.validate()?;
            if self.tools[..i].iter().any(|o| o.number == t.number) {
                return Err(format!("Two tools have the number {}.", t.number));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_puts_and_replaces_by_number() {
        let mut lib = ToolLibrary::default();
        lib.put(Tool::flat_end_mill(3, 6.0));
        lib.put(Tool::flat_end_mill(1, 3.175));
        lib.put(Tool::flat_end_mill(3, 8.0));
        assert_eq!(
            lib.tools.iter().map(|t| t.number).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(lib.get(3).unwrap().diameter, 8.0);
        assert_eq!(lib.get(1).unwrap().name, "3.175 mm flat end mill");
        assert!(lib.validate().is_ok());
    }

    #[test]
    fn tool_json_round_trips() {
        let mut t = Tool::flat_end_mill(2, 6.0);
        t.shape = ToolShape::BullNose { corner_radius: 0.5 };
        t.cutting = Some(Cutting {
            spindle_rpm: 18000.0,
            feed: 1200.0,
            plunge_feed: 300.0,
        });
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"kind\":\"bull_nose\""));
        let back: Tool = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn validation_names_the_tool() {
        let mut t = Tool::flat_end_mill(4, 6.0);
        t.diameter = 0.0;
        assert_eq!(t.validate().unwrap_err(), "Tool 4 has no diameter.");
        let mut b = Tool::flat_end_mill(5, 6.0);
        b.shape = ToolShape::BullNose { corner_radius: 4.0 };
        assert!(b.validate().is_err());
    }
}
