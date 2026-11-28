// SPDX-License-Identifier: MIT
//! CAM kept with the design (mitcad#1013, `docs/cam.md`, "Data model"):
//! setups of the stock, each with its work coordinates, its machine, its
//! post settings and its ordered operations, and the machines the setups
//! use, copied into the design with their tool libraries. None of it is on
//! the timeline: editing CAM never recomputes the model, and the model's
//! recompute never computes toolpaths. Each operation records the
//! fingerprint of its inputs when its toolpath is generated, so that a
//! change of the model (or of the operation) marks it out of date.
//!
//! The toolpaths themselves are computed by the crate `mitcad-cam`
//! ([`engine`]) from the planar outlines the model gives
//! (`document/cam.rs`, [`outline`]); they are not saved.
//!
//! Values the user types (depths, feeds, margins) are [`CamValue`]s: a
//! JSON number (millimetres, mm/min, rpm or degrees) or an expression
//! with units that the design's parameters evaluate (`"0.25 in"`,
//! `"thickness + 1"`), so that inch users enter inches and the operation
//! follows the parameters.
//!
//! The machine library (a machine with its tools per file, in the user's
//! folder and in the project's) is in [`library`].

pub mod library;
pub mod outline;
// The stock simulation's options (mitcad#1013, phase 3).
pub mod simulate;
// Turn Finishing into undercuts: the Plunge choice (mitcad#2503).
pub mod plunge;
pub use plunge::{PLUNGE_CLEARANCE, Plunge};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use mitcad_cam as engine;
use mitcad_cam::ops::{Direction, Side};
use mitcad_cam::{Coolant, Cutting, Machine, PostSettings, SpindleDirection};

use crate::features::OccurrencePath;
use crate::features::geom_ref::GeomRef;
use crate::ids::{BodyUid, FeatureUid};
use crate::parameters::Parameters;
use crate::topo::{EdgeName, FaceName, RegionKey};

/// The version of the design file's `cam` section this build writes.
pub const CAM_VERSION: u32 = 1;
/// The most setups a design has.
pub const MAX_SETUPS: usize = 64;
/// The most operations a setup has.
pub const MAX_OPERATIONS: usize = 256;
/// The most machines a design keeps.
pub const MAX_MACHINES: usize = 32;
/// The most tools a machine's library has.
pub const MAX_TOOLS: usize = 1000;
/// The longest name of a setup, an operation, a machine or a tool.
pub const MAX_NAME: usize = 120;

/// What a [`CamValue`] measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// Millimetres (and millimetres per minute for feeds: an expression
    /// `"40 in"` for a feed is 40 inches per minute).
    Length,
    /// Degrees.
    Angle,
    /// A plain number (spindle speeds in rpm, counts).
    Number,
}

/// A value as the user gave it: a JSON number (millimetres, mm/min, rpm or
/// degrees, whatever the design's units) or an expression the design's
/// parameters evaluate, with units (`"0.25 in"`; a bare number in it is in
/// the design's length or angle unit).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CamValue {
    Number(f64),
    Expression(String),
}

impl From<f64> for CamValue {
    fn from(value: f64) -> Self {
        CamValue::Number(value)
    }
}

impl CamValue {
    /// The value in millimetres (mm/min), degrees or as a number.
    pub fn evaluate(&self, params: &Parameters, kind: ValueKind) -> Result<f64, String> {
        let value = match self {
            CamValue::Number(number) => *number,
            CamValue::Expression(text) => {
                let context = params.context();
                let unit = match kind {
                    ValueKind::Length => crate::expr::Unit::of_length(context.default_length_unit),
                    ValueKind::Angle => crate::expr::Unit::of_angle(context.default_angle_unit),
                    ValueKind::Number => crate::expr::Unit::NONE,
                };
                let quantity = params
                    .table()
                    .evaluate(text, unit)
                    .map_err(|e| format!("“{text}” is not a value here: {e}"))?;
                match kind {
                    ValueKind::Angle => quantity.value.to_degrees(),
                    _ => quantity.value,
                }
            }
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err("A value is not a finite number.".to_owned())
        }
    }
}

fn zero() -> CamValue {
    CamValue::Number(0.0)
}

fn two() -> u32 {
    2
}

// Tools.

/// The kind of a tool's cutting end; angles in degrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolKind {
    /// A flat (square) end mill.
    Flat,
    /// A ball end mill.
    Ball,
    /// A bull nose end mill: a flat end with rounded corners.
    Bull { corner_radius: f64 },
    /// A twist drill with its point's included angle.
    Drill {
        #[serde(default = "drill_point")]
        point_angle: f64,
    },
    /// A chamfer mill: its included angle and the diameter of its tip.
    Chamfer {
        #[serde(default = "ninety")]
        angle: f64,
        #[serde(default)]
        tip_diameter: f64,
    },
    /// An engraving cutter (V-bit): its included angle and tip.
    Engraver {
        #[serde(default = "thirty")]
        angle: f64,
        #[serde(default)]
        tip_diameter: f64,
    },
    // Lathe tools (mitcad#1869): the number is the turret station (1 to
    // 99), the diameter the holder's size (a boring bar's diameter), the
    // flute length the deepest cut and the length the overhang.
    /// An outside turning tool with its insert's nose radius.
    Turning {
        #[serde(default)]
        nose_radius: f64,
    },
    /// A boring bar with its insert's nose radius.
    Boring {
        #[serde(default)]
        nose_radius: f64,
    },
    /// A grooving insert, outside or in a bore.
    Grooving {
        width: f64,
        #[serde(default)]
        nose_radius: f64,
        #[serde(default)]
        inside: bool,
    },
    /// A parting blade.
    Parting { width: f64 },
    /// A threading insert with its included angle, outside or in a bore.
    Threading {
        #[serde(default = "sixty")]
        angle: f64,
        #[serde(default)]
        inside: bool,
    },
}

fn sixty() -> f64 {
    60.0
}

fn drill_point() -> f64 {
    118.0
}

fn ninety() -> f64 {
    90.0
}

fn thirty() -> f64 {
    30.0
}

impl ToolKind {
    /// The kind in words, for a tool's default name.
    pub fn words(&self) -> &'static str {
        match self {
            ToolKind::Flat => "flat end mill",
            ToolKind::Ball => "ball end mill",
            ToolKind::Bull { .. } => "bull nose end mill",
            ToolKind::Drill { .. } => "drill",
            ToolKind::Chamfer { .. } => "chamfer mill",
            ToolKind::Engraver { .. } => "engraving cutter",
            ToolKind::Turning { .. } => "turning tool",
            ToolKind::Boring { .. } => "boring bar",
            ToolKind::Grooving { .. } => "grooving tool",
            ToolKind::Parting { .. } => "parting tool",
            ToolKind::Threading { .. } => "threading tool",
        }
    }

    /// Whether it is a lathe tool.
    pub fn is_turning(&self) -> bool {
        matches!(
            self,
            ToolKind::Turning { .. }
                | ToolKind::Boring { .. }
                | ToolKind::Grooving { .. }
                | ToolKind::Parting { .. }
                | ToolKind::Threading { .. }
        )
    }

    /// The shape as the engine's tools have it (`mitcad_cam::ToolShape`'s
    /// JSON).
    fn engine_shape(&self) -> Value {
        match self {
            ToolKind::Flat => json!({"kind": "flat_end_mill"}),
            ToolKind::Ball => json!({"kind": "ball_end_mill"}),
            ToolKind::Bull { corner_radius } => {
                json!({"kind": "bull_nose", "corner_radius": corner_radius})
            }
            ToolKind::Drill { point_angle } => json!({"kind": "drill", "point_angle": point_angle}),
            ToolKind::Chamfer {
                angle,
                tip_diameter,
            }
            | ToolKind::Engraver {
                angle,
                tip_diameter,
            } => json!({"kind": "v_bit", "angle": angle, "tip_diameter": tip_diameter}),
            // Lathe tools are not milling tools ([`CamTool::engine`]).
            ToolKind::Turning { .. }
            | ToolKind::Boring { .. }
            | ToolKind::Grooving { .. }
            | ToolKind::Parting { .. }
            | ToolKind::Threading { .. } => Value::Null,
        }
    }
}

/// A tool of a machine's tool library (lengths in millimetres).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CamTool {
    /// The tool number (`T`), also its slot in a tool changer; 1 to 999.
    pub number: u32,
    #[serde(default)]
    pub name: String,
    pub shape: ToolKind,
    /// The cutting diameter.
    pub diameter: f64,
    /// The shank's diameter, when it differs from the cutting diameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shank_diameter: Option<f64>,
    /// The flutes' length: the deepest cut in one pass.
    pub flute_length: f64,
    /// The stick-out from the holder to the tip.
    pub length: f64,
    #[serde(default = "two")]
    pub flutes: u32,
    /// The holder the tool sits in: a holder of the machine's spindle
    /// assembly, by its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
    /// Default speeds and feeds of the operations that use it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cutting: Option<Cutting>,
    /// A milling tool on a lathe's turret is a live tool (mitcad#2067):
    /// its holder along the spindle (axial, cutting the face) or across it
    /// (radial, cutting the diameter); axial when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<LiveMount>,
    /// A turning tool's or boring bar's insert and holder (mitcad#2502):
    /// the ISO codes, the tip and approach angles and the hand worked out
    /// from them, and what was typed over them; none when only the nose
    /// radius is known (tools kept before it have none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<engine::turning::InsertGeometry>,
}

/// How a live tool sits on a lathe's turret (mitcad#2067).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveMount {
    /// Along the spindle axis: it mills and drills the part's face.
    #[default]
    Axial,
    /// Across the spindle axis: it mills and drills the diameter.
    Radial,
    /// In a milling head that tilts about Y (a B axis, mitcad#2319): it
    /// cuts the face, the diameter, planes and tilted planes.
    Head,
}

/// Where a milling operation in a lathe setup cuts with its live tool
/// (mitcad#2067).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LivePlace {
    /// On the part's face, with the C axis interpolating (polar) or
    /// indexed (drilling).
    #[default]
    Face,
    /// On the diameter, with the C axis interpolating (cylindrical) or
    /// indexed (drilling). Faces round the spindle axis, or sketch
    /// geometry wrapped onto the diameter (mitcad#2319).
    Diameter,
    /// On a plane with the Y axis, the C axis clamped (mitcad#2319): the
    /// face off the spindle axis, a flat across it, or a plane tilted by a
    /// B head, as the operation's plane angles or its inputs give it.
    Plane,
}

/// Whether a tool goes into a machine's library: lathe tools on a lathe,
/// milling tools on a mill (mitcad#1869), and on a lathe with a C axis and
/// live tools milling tools too, as its live tools on stations 1 to 99
/// (mitcad#2067); a live holder only on such a lathe.
pub fn tool_fits(tool: &CamTool, m: &Machine) -> Result<(), String> {
    let n = tool.number;
    match &m.lathe {
        None if tool.shape.is_turning() => {
            Err(format!("The machine {} takes milling tools only.", m.name))
        }
        None if tool.mount.is_some() => Err(format!(
            "Tool {n} sits in a live tool's holder, which only a lathe's turret has."
        )),
        None => Ok(()),
        Some(_) if tool.shape.is_turning() => Ok(()),
        Some(lathe) if !lathe.mills() => {
            Err(format!("The lathe {} takes lathe tools only.", m.name))
        }
        Some(_) if n > 99 => Err(format!(
            "Tool {n} is a live tool: its number is its turret station, 1 to 99."
        )),
        Some(_) => Ok(()),
    }
}

/// A number without trailing zeros, for names.
fn short(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

impl CamTool {
    /// `6 mm flat end mill`.
    pub fn default_name(&self) -> String {
        format!("{} mm {}", short(self.diameter), self.shape.words())
    }

    /// What is wrong with the tool, in words.
    pub fn check(&self) -> Result<(), String> {
        let n = self.number;
        if !(1..=999).contains(&n) {
            return Err("A tool number must be from 1 to 999.".to_owned());
        }
        if self.name.chars().count() > MAX_NAME {
            return Err(format!("Tool {n}'s name is too long."));
        }
        let positive = |v: f64| v.is_finite() && v > 0.0;
        if !positive(self.diameter) {
            return Err(format!("Tool {n} needs a diameter above zero."));
        }
        if !positive(self.flute_length) {
            return Err(format!("Tool {n} needs a flute length above zero."));
        }
        if !(self.length.is_finite() && self.length >= self.flute_length) {
            return Err(format!(
                "Tool {n} sticks out less than its flutes are long."
            ));
        }
        if self.shank_diameter.is_some_and(|d| !positive(d)) {
            return Err(format!("Tool {n}'s shank diameter must be above zero."));
        }
        if !(1..=16).contains(&self.flutes) {
            return Err(format!("Tool {n} must have 1 to 16 flutes."));
        }
        match self.shape {
            ToolKind::Bull { corner_radius } => {
                if !(positive(corner_radius) && corner_radius <= self.diameter / 2.0) {
                    return Err(format!(
                        "Tool {n}'s corner radius must be above zero and within its radius."
                    ));
                }
            }
            ToolKind::Drill { point_angle } => {
                if !(point_angle > 0.0 && point_angle < 180.0) {
                    return Err(format!(
                        "Tool {n}'s point angle must be between 0 and 180 degrees."
                    ));
                }
            }
            ToolKind::Chamfer {
                angle,
                tip_diameter,
            }
            | ToolKind::Engraver {
                angle,
                tip_diameter,
            } => {
                if !(angle > 0.0 && angle < 180.0) {
                    return Err(format!(
                        "Tool {n}'s angle must be between 0 and 180 degrees."
                    ));
                }
                if !(tip_diameter.is_finite()
                    && tip_diameter >= 0.0
                    && tip_diameter < self.diameter)
                {
                    return Err(format!(
                        "Tool {n}'s tip must be narrower than its diameter."
                    ));
                }
            }
            ToolKind::Flat | ToolKind::Ball => {}
            ToolKind::Turning { nose_radius } | ToolKind::Boring { nose_radius } => {
                if !(nose_radius.is_finite() && nose_radius >= 0.0) {
                    return Err(format!("Tool {n}'s nose radius must be zero or more."));
                }
            }
            ToolKind::Grooving {
                width, nose_radius, ..
            } => {
                if !(positive(width)
                    && nose_radius.is_finite()
                    && nose_radius >= 0.0
                    && 2.0 * nose_radius <= width)
                {
                    return Err(format!(
                        "Tool {n}'s insert needs a width above zero and at least twice its corner radius."
                    ));
                }
            }
            ToolKind::Parting { width } => {
                if !positive(width) {
                    return Err(format!("Tool {n}'s blade needs a width above zero."));
                }
            }
            ToolKind::Threading { angle, .. } => {
                if !(angle > 0.0 && angle < 120.0) {
                    return Err(format!(
                        "Tool {n}'s thread angle must be between 0 and 120 degrees."
                    ));
                }
            }
        }
        if self.shape.is_turning() && n > 99 {
            return Err(format!(
                "Tool {n} is a lathe tool: its number is its turret station, 1 to 99."
            ));
        }
        if self.shape.is_turning() && self.mount.is_some() {
            return Err(format!(
                "Tool {n} is a lathe tool: only milling tools and drills sit in a live tool's holder."
            ));
        }
        if let Some(cutting) = &self.cutting {
            cutting.validate().map_err(|e| format!("Tool {n}: {e}"))?;
        }
        // The insert and holder of a turning tool or boring bar (mitcad#2502).
        if let Some(insert) = &self.insert {
            if !matches!(
                self.shape,
                ToolKind::Turning { .. } | ToolKind::Boring { .. }
            ) {
                return Err(format!(
                    "Tool {n}: only turning tools and boring bars take an insert and a holder."
                ));
            }
            insert.validate().map_err(|e| format!("Tool {n}: {e}"))?;
        }
        Ok(())
    }

    /// The tool as the engine computes with it.
    pub fn engine(&self) -> Result<engine::Tool, String> {
        if self.shape.is_turning() {
            return Err(format!(
                "Tool {} is a lathe tool, not a milling tool.",
                self.number
            ));
        }
        let name = if self.name.trim().is_empty() {
            self.default_name()
        } else {
            self.name.clone()
        };
        let value = json!({
            "number": self.number,
            "name": name,
            "shape": self.shape.engine_shape(),
            "diameter": self.diameter,
            "flute_length": self.flute_length,
            "length": self.length,
            "flutes": self.flutes,
            "cutting": self.cutting,
        });
        serde_json::from_value(value).map_err(|_| format!("Tool {} cannot be used.", self.number))
    }
}

impl CamTool {
    /// A lathe tool as the engine's turning computes with it.
    pub fn turning_engine(&self) -> Result<engine::TurningTool, String> {
        use engine::turning::TurningToolKind as K;
        let kind = match self.shape {
            ToolKind::Turning { .. } => K::Turning,
            ToolKind::Boring { .. } => K::Boring {
                bar_diameter: self.diameter,
            },
            ToolKind::Grooving { width, inside, .. } => K::Grooving { width, inside },
            ToolKind::Parting { width } => K::Parting { width },
            ToolKind::Threading { angle, inside } => K::Threading { angle, inside },
            _ => {
                return Err(format!(
                    "Tool {} is a milling tool, not a lathe tool.",
                    self.number
                ));
            }
        };
        let nose_radius = match self.shape {
            ToolKind::Turning { nose_radius }
            | ToolKind::Boring { nose_radius }
            | ToolKind::Grooving { nose_radius, .. } => nose_radius,
            _ => 0.0,
        };
        let name = if self.name.trim().is_empty() {
            self.default_name()
        } else {
            self.name.clone()
        };
        let tool = engine::TurningTool {
            number: self.number,
            offset: 0,
            name,
            kind,
            nose_radius,
            max_depth: self.flute_length,
            // The trailing edge the simulation draws (mitcad#2503).
            back_angle: matches!(
                self.shape,
                ToolKind::Turning { .. } | ToolKind::Boring { .. }
            )
            .then(|| self.back_angle())
            .flatten(),
        };
        tool.validate()?;
        Ok(tool)
    }
}

/// A machine's tools by number (the tool library of one machine).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CamToolLibrary {
    #[serde(default)]
    pub tools: Vec<CamTool>,
}

impl CamToolLibrary {
    pub fn get(&self, number: u32) -> Option<&CamTool> {
        self.tools.iter().find(|t| t.number == number)
    }

    /// Adds a tool or replaces the one with its number; sorted by number.
    pub fn put(&mut self, tool: CamTool) {
        match self.tools.iter_mut().find(|t| t.number == tool.number) {
            Some(t) => *t = tool,
            None => {
                self.tools.push(tool);
                self.tools.sort_by_key(|t| t.number);
            }
        }
    }

    /// Takes a tool away; whether it was there.
    pub fn remove(&mut self, number: u32) -> bool {
        let before = self.tools.len();
        self.tools.retain(|t| t.number != number);
        self.tools.len() != before
    }

    pub fn check(&self) -> Result<(), String> {
        if self.tools.len() > MAX_TOOLS {
            return Err(format!("A tool library has at most {MAX_TOOLS} tools."));
        }
        for (i, tool) in self.tools.iter().enumerate() {
            tool.check()?;
            if self.tools[..i].iter().any(|t| t.number == tool.number) {
                return Err(format!("Two tools have the number {}.", tool.number));
            }
        }
        Ok(())
    }
}

// Machines.

/// Where a design's machine was copied from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineOrigin {
    /// `user` or `project`.
    pub library: String,
    pub id: String,
    /// The library file's version that was copied.
    #[serde(default)]
    pub version: u32,
}

/// A machine the design's setups use, with its tool library: a copy, so
/// that a design machines the same after the library changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CamMachine {
    /// Its id in the design (and the library's file name): letters,
    /// digits, `_` and `-`.
    pub id: String,
    pub machine: Machine,
    #[serde(default)]
    pub tools: CamToolLibrary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<MachineOrigin>,
}

/// Whether a text is a machine id: 1 to 64 ASCII letters, digits, `_` or
/// `-`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl CamMachine {
    /// What is wrong with a machine put in: [`Self::check_stored`] and a
    /// controller MitCad writes for, of the machine's kind.
    pub fn check(&self) -> Result<(), String> {
        self.check_with(true)
    }

    /// What is wrong with a machine a design file keeps,
    /// whatever its controller: a controller MitCad does not write for (a
    /// design made in another build, mitcad#2531) still opens with its
    /// toolpaths, and the post refuses when asked to write G-code.
    pub fn check_stored(&self) -> Result<(), String> {
        self.check_with(false)
    }

    fn check_with(&self, controller: bool) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(
                "A machine's id is 1 to 64 letters, digits, underscores or hyphens.".to_owned(),
            );
        }
        let m = &self.machine;
        if m.name.trim().is_empty() || m.name.chars().count() > MAX_NAME {
            return Err("A machine needs a name of at most 120 characters.".to_owned());
        }
        let lathe = m.lathe.is_some();
        let known = !controller
            || if lathe {
                engine::turning_dialect(&m.controller).is_some()
            } else {
                engine::dialect(&m.controller).is_some()
            };
        if !known {
            // A dialect of the other kind of machine says which kind it is
            // for (mitcad#2119); an unknown one is only refused.
            return Err(if lathe {
                match engine::dialect(&m.controller) {
                    Some(d) => format!(
                        "The lathe {} needs a lathe controller; {} writes for milling machines.",
                        m.name,
                        d.label()
                    ),
                    None => format!(
                        "The machine {} has a controller MitCad does not write for.",
                        m.name
                    ),
                }
            } else {
                match engine::turning_dialect(&m.controller) {
                    Some(d) => format!(
                        "The machine {} is a milling machine; {} writes for lathes.",
                        m.name,
                        d.label()
                    ),
                    None => format!(
                        "The machine {} has a controller MitCad does not write for.",
                        m.name
                    ),
                }
            });
        }
        if let Some(l) = &m.lathe {
            l.validate()
                .map_err(|e| format!("The machine {}: {e}", m.name))?;
        }
        for tool in &self.tools.tools {
            tool_fits(tool, m)?;
        }
        if !(m.spindle.min_rpm >= 0.0 && m.spindle.max_rpm >= m.spindle.min_rpm) {
            return Err(format!(
                "The machine {}'s spindle speeds must run from a minimum up to a maximum.",
                m.name
            ));
        }
        if !(m.max_feed > 0.0 && m.rapid_feed > 0.0) {
            return Err(format!(
                "The machine {}'s feeds must be above zero.",
                m.name
            ));
        }
        self.tools
            .check()
            .map_err(|e| format!("The machine {}: {e}", m.name))
    }
}

// Setups.

/// The stock a setup cuts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stock {
    /// A box round the setup's bodies (their bounding box in the work
    /// axes) grown by margins: on the four sides, above and below.
    AroundBodies {
        #[serde(default = "zero")]
        side: CamValue,
        #[serde(default = "zero")]
        top: CamValue,
        #[serde(default = "zero")]
        bottom: CamValue,
    },
    /// A body of the design: its bounding box in the work axes, as the
    /// occurrence places it (see [`CamBody`]).
    Body {
        body: BodyUid,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
}

impl Stock {
    /// The stock body, when the stock is one.
    pub fn body(&self) -> Option<CamBody> {
        match self {
            Stock::Body { body, occurrence } => Some(CamBody {
                body: *body,
                occurrence: occurrence.clone(),
            }),
            Stock::AroundBodies { .. } => None,
        }
    }
}

/// A body a setup machines, with the occurrence that places it: the
/// occurrences from the design's root down to the body's component
/// (`"O1/O4"`). Empty for a body of the root, or of a component placed
/// once (its only placement). A component placed twice has a body in
/// each placement. Written as the body's id alone when the occurrence is
/// empty (`"F2.b0"`), else as `{"body", "occurrence"}`.
#[derive(Debug, Clone, PartialEq)]
pub struct CamBody {
    pub body: BodyUid,
    pub occurrence: OccurrencePath,
}

impl From<BodyUid> for CamBody {
    fn from(body: BodyUid) -> Self {
        CamBody {
            body,
            occurrence: OccurrencePath::default(),
        }
    }
}

/// [`CamBody`] as written: the body's id alone, or with its occurrence.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum CamBodyText {
    Plain(BodyUid),
    Placed {
        body: BodyUid,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
}

impl Serialize for CamBody {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.occurrence.is_empty() {
            CamBodyText::Plain(self.body).serialize(s)
        } else {
            CamBodyText::Placed {
                body: self.body,
                occurrence: self.occurrence.clone(),
            }
            .serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for CamBody {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match CamBodyText::deserialize(d)? {
            CamBodyText::Plain(body) => body.into(),
            CamBodyText::Placed { body, occurrence } => CamBody { body, occurrence },
        })
    }
}

impl Default for Stock {
    fn default() -> Self {
        Stock::AroundBodies {
            side: zero(),
            top: zero(),
            bottom: zero(),
        }
    }
}

/// A point of the stock's box in the work axes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockPoint {
    /// The top's corner at the lowest X and Y (the usual router zero).
    #[default]
    TopFrontLeft,
    TopFrontRight,
    TopBackLeft,
    TopBackRight,
    /// The middle of the top.
    TopCenter,
    BottomFrontLeft,
    BottomCenter,
}

impl StockPoint {
    /// The point of a box `[min, max]`.
    pub fn of(self, min: [f64; 3], max: [f64; 3]) -> [f64; 3] {
        let mid = |k: usize| (min[k] + max[k]) / 2.0;
        match self {
            StockPoint::TopFrontLeft => [min[0], min[1], max[2]],
            StockPoint::TopFrontRight => [max[0], min[1], max[2]],
            StockPoint::TopBackLeft => [min[0], max[1], max[2]],
            StockPoint::TopBackRight => [max[0], max[1], max[2]],
            StockPoint::TopCenter => [mid(0), mid(1), max[2]],
            StockPoint::BottomFrontLeft => [min[0], min[1], min[2]],
            StockPoint::BottomCenter => [mid(0), mid(1), min[2]],
        }
    }
}

/// Where the work zero is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WcsOrigin {
    /// A point of the stock's box.
    Stock {
        #[serde(default)]
        corner: StockPoint,
    },
    /// A point of the model: a vertex, a sketch point, a circular edge's
    /// centre, a straight edge's middle (any point reference), in the
    /// occurrence it was picked in (see [`CamInput`]). Boxed: a reference
    /// is much larger than a corner (the JSON is the same).
    Point {
        at: Box<GeomRef>,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
}

impl Default for WcsOrigin {
    fn default() -> Self {
        WcsOrigin::Stock {
            corner: StockPoint::default(),
        }
    }
}

/// The work coordinate system: its origin and axes. Z is the tool axis,
/// pointing up out of the material.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Wcs {
    #[serde(default)]
    pub origin: WcsOrigin,
    /// Z along a planar face's outward normal, a plane's normal or an axis
    /// (a straight edge, a cylindrical face); the design's Z when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<GeomRef>,
    /// The occurrence `z` was picked in (see [`CamInput`]).
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub z_occurrence: OccurrencePath,
    /// X along an axis (a straight edge), turned into the plane across Z;
    /// the plane's own X (a face's), else the design's X, when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<GeomRef>,
    /// The occurrence `x` was picked in.
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub x_occurrence: OccurrencePath,
}

fn one_u8() -> u8 {
    1
}

fn default_safe_height() -> CamValue {
    CamValue::Number(15.0)
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// A setup: one clamping of the stock and what is cut in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CamSetup {
    /// Unique among the design's setups and operations.
    pub id: u32,
    pub name: String,
    /// The bodies it machines, each as its occurrence places it; every
    /// body at the marker, as placed, when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<CamBody>,
    #[serde(default)]
    pub stock: Stock,
    #[serde(default)]
    pub wcs: Wcs,
    /// 1 to 6 for G54 to G59.
    #[serde(default = "one_u8")]
    pub work_offset: u8,
    /// How far above the stock's top the tool changes and travels.
    #[serde(default = "default_safe_height")]
    pub safe_height: CamValue,
    /// The design's machine (its id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// The post-processor's dialect; the machine's controller when left
    /// out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<String>,
    /// Units (millimetres or inches), arcs, line numbers, comments, tool
    /// length offsets of the written program.
    #[serde(default)]
    pub post: PostSettings,
    /// The program probes the work zero first (MillenniumOS' G6600).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub probe_work_zero: bool,
    /// The machine position of the work zero, for the machine check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<[f64; 3]>,
    /// Its toolpaths drawn in the view: the Browser's eye, kept in the
    /// design (written only when off). Display state: it changes no
    /// fingerprint.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<CamOperation>,
    /// What a setup on a lathe adds (mitcad#1869).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lathe: Option<LatheSetup>,
}

/// A lathe setup's own values (mitcad#1869, `docs/cam.md`, "Turning"):
/// the stock's bore (a tube, a drilled hole), where the turret indexes,
/// the program's number and where the chuck's jaws are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LatheSetup {
    /// The stock's bore diameter; a solid bar when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_bore: Option<CamValue>,
    /// A bar of a given size (mitcad#2550), with stock `around_bodies`
    /// only: its diameter in place of the side margin, and its length
    /// from its face (the top margin in front of the part's face) towards
    /// the chuck in place of the bottom margin; each margin when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_diameter: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_length: Option<CamValue>,
    /// The tool change position: a diameter and a work Z; 50 mm clear of
    /// the stock's diameter and its face when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_diameter: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_z: Option<CamValue>,
    /// The program's number (`O1000`), 1 to 9999.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_number: Option<u32>,
    /// How far the chuck's jaws are behind the stock's back (the end
    /// towards the chuck), for the lathe check; unchecked when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chuck_gap: Option<CamValue>,
    /// The sub-spindle takes the part over before the operations on its
    /// back (mitcad#2319); none: the program stays on the main spindle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_over: Option<HandOverSetup>,
    /// Blocks of G-code the programmer writes into the program between
    /// the operations (mitcad#2506), in their order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_code: Vec<ManualCode>,
    /// The turned profile is the bodies spun about the spindle axis
    /// (mitcad#2629): every feature at whatever spindle angle, not only
    /// what the section through the axis cuts, so that turning leaves the
    /// material live tools mill. The live tools read the bodies as they
    /// are.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spun_target: bool,
}

/// A block of G-code the programmer writes into a lathe setup's program
/// (mitcad#2506): the stock's handling the operations do not compute, as
/// pulling the bar after parting off. It is written as it is after the
/// operation `after` (its id; before the first operation when left out),
/// and after the blocks before it that follow the same operation; when
/// that operation is suppressed, where it would be. Neither the simulation
/// nor the read-back of the program sees what it does: the post leaves the
/// machine in a defined state before it (the coolant and the spindle off,
/// the tool at the tool change position) and states everything again after
/// it (`docs/cam.md`, "Manual G-code").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManualCode {
    #[serde(default = "manual_code_name")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u32>,
    /// Its lines, as they are written.
    #[serde(default)]
    pub text: String,
}

fn manual_code_name() -> String {
    "Manual G-code".to_owned()
}

/// The most lines a manual block has (mitcad#2506).
pub const MAX_MANUAL_LINES: usize = 500;
/// The most manual blocks a setup has.
pub const MAX_MANUAL_BLOCKS: usize = 64;

impl ManualCode {
    /// Its lines that are written: each trimmed, blank ones left out.
    pub fn lines(&self) -> Vec<String> {
        self.text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

impl CamSetup {
    /// Keeps the manual blocks in their place when an operation leaves the
    /// setup (mitcad#2506): those after it follow the operation before it,
    /// or come first.
    pub fn operation_leaves(&mut self, index: usize) {
        let Some(gone) = self.operations.get(index).map(|o| o.id) else {
            return;
        };
        let before = index
            .checked_sub(1)
            .and_then(|i| self.operations.get(i))
            .map(|o| o.id);
        if let Some(lathe) = &mut self.lathe {
            for block in &mut lathe.manual_code {
                if block.after == Some(gone) {
                    block.after = before;
                }
            }
        }
    }
}

/// The sub-spindle's hand-over of a lathe setup (mitcad#2319), each value
/// its default when left out: the sub-spindle grips the part 10 mm from
/// its back (the work Z where its jaws' face comes to), pulls it 50 mm
/// back, the spindles synchronised at 500 rpm, the sub-spindle coming up at
/// 1000 mm/min, its work offset G55.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HandOverSetup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grip_z: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withdraw: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<CamValue>,
    /// 1 to 6 for G54 to G59.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_offset: Option<u8>,
}

impl LatheSetup {
    fn values(&self) -> Vec<&CamValue> {
        let hand_over = self.hand_over.as_ref();
        [
            &self.stock_bore,
            &self.stock_diameter,
            &self.stock_length,
            &self.change_diameter,
            &self.change_z,
            &self.chuck_gap,
        ]
        .into_iter()
        .flatten()
        .chain(hand_over.into_iter().flat_map(|h| {
            [&h.grip_z, &h.withdraw, &h.rpm, &h.feed]
                .into_iter()
                .flatten()
        }))
        .collect()
    }

    fn values_mut(&mut self) -> Vec<&mut CamValue> {
        let hand_over = self.hand_over.as_mut();
        [
            &mut self.stock_bore,
            &mut self.stock_diameter,
            &mut self.stock_length,
            &mut self.change_diameter,
            &mut self.change_z,
            &mut self.chuck_gap,
        ]
        .into_iter()
        .flatten()
        .chain(hand_over.into_iter().flat_map(|h| {
            [&mut h.grip_z, &mut h.withdraw, &mut h.rpm, &mut h.feed]
                .into_iter()
                .flatten()
        }))
        .collect()
    }
}

/// The depths an operation cuts between (work Z); each from the stock and
/// the inputs when left out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DepthsInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_down: Option<CamValue>,
}

/// The heights the tool moves at between cuts (work Z).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HeightsInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retract: Option<CamValue>,
}

/// Speeds and feeds of an operation; the tool's where left out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CuttingInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spindle_rpm: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plunge_feed: Option<CamValue>,
}

/// How the tool goes into the material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// Straight down (a centre-cutting tool).
    Plunge,
    /// Down along the cut at an angle (degrees from the horizontal).
    Ramp { angle: CamValue },
    /// Down a helix of a radius at an angle.
    Helix {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<CamValue>,
        angle: CamValue,
    },
}

/// Holding tabs left on an outside contour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tabs {
    pub count: u32,
    pub width: CamValue,
    pub height: CamValue,
}

/// Arcs into and out of a wall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lead {
    pub radius: CamValue,
}

/// What an operation does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationKind {
    /// Rows across the stock's top (or the inputs' extent).
    Facing,
    /// Along outlines: outside, inside or on the line.
    Contour {
        #[serde(default = "outside")]
        side: Side,
    },
    /// Offset loops clearing a region with its islands.
    Pocket,
    /// Holes at points, in one go or in pecks (`chip_break`: pecks that
    /// back off a little instead of leaving the hole). `canned_cycle`
    /// allows the post to write the holes as the controller's drilling
    /// cycle (on by default); the setup's post settings
    /// (`post.canned_cycles`) decide whether cycles are written at all.
    Drill {
        #[serde(default)]
        peck: bool,
        #[serde(default)]
        chip_break: bool,
        #[serde(default = "allowed")]
        canned_cycle: bool,
    },
    /// Constant-engagement clearing of a region.
    Adaptive,
    /// What a bigger tool left in a region (`from_tool`, its number).
    Rest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_tool: Option<u32>,
    },
    // 3-axis machining of the bodies' surfaces (phase 3): their inputs
    // are bodies (the setup's when none) and, as a boundary, faces or
    // sketch profiles (the stock's outline when none).
    /// Levels from the top down, each a pocket of where the tool may go
    /// at that height; `adaptive` clears them with adaptive clearing.
    #[serde(rename = "roughing_3d")]
    Roughing3d {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        adaptive: bool,
    },
    /// Parallel passes over the surfaces at `angle` (degrees from the work
    /// X; 0 when left out), where the surface is at most `slope` steep
    /// (degrees from the horizontal; every slope when left out).
    RasterFinish {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        angle: Option<CamValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slope: Option<CamValue>,
    },
    /// Constant-height contours round the surfaces at each level, where
    /// the surface is at least `slope` steep (every slope when left out).
    WaterlineFinish {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slope: Option<CamValue>,
    },
    // Turning on a lathe (mitcad#1869): from the profile of the setup's
    // bodies about the spindle axis. Z values are work Z along the
    // spindle (0 on the part's face, negative towards the chuck); a
    // diameter is a diameter.
    /// Facing the part's front from the stock's face.
    TurnFace,
    /// Roughing the outside or the bore in passes along Z; `plunge` how
    /// far the passes go into undercuts (mitcad#2549; not written when
    /// they do not, so older files keep their roughing).
    TurnRough {
        #[serde(default)]
        side: TurnSide,
        #[serde(default, skip_serializing_if = "Plunge::is_none")]
        plunge: Plunge,
    },
    /// Finishing the outside or the bore along its contour; `plunge` how
    /// it goes into undercuts (mitcad#2503; not written when it does
    /// not).
    TurnFinish {
        #[serde(default)]
        side: TurnSide,
        #[serde(default, skip_serializing_if = "Plunge::is_none")]
        plunge: Plunge,
    },
    /// A groove between two Z (from the groove's face input when left
    /// out), down to the profile there.
    TurnGroove {
        #[serde(default)]
        side: TurnSide,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        z_from: Option<CamValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        z_to: Option<CamValue>,
    },
    /// Parting the part off at its back (or at `z`).
    TurnPartOff {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        z: Option<CamValue>,
    },
    /// A thread of `pitch` from `z_start` to `z_end` on `diameter` (the
    /// major diameter outside, the minor in a bore), `depth` deep
    /// (radius); the diameter and the Z from the threaded face input, the
    /// depth from the pitch, when left out.
    TurnThread {
        #[serde(default)]
        side: TurnSide,
        pitch: CamValue,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diameter: Option<CamValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        z_start: Option<CamValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        z_end: Option<CamValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        depth: Option<CamValue>,
    },
}

pub use engine::turning::TurnSide;

fn outside() -> Side {
    Side::Outside
}

fn allowed() -> bool {
    true
}

impl OperationKind {
    /// The kind's id as the operation's JSON names it.
    pub fn id(&self) -> &'static str {
        match self {
            OperationKind::Facing => "facing",
            OperationKind::Contour { .. } => "contour",
            OperationKind::Pocket => "pocket",
            OperationKind::Drill { .. } => "drill",
            OperationKind::Adaptive => "adaptive",
            OperationKind::Rest { .. } => "rest",
            OperationKind::Roughing3d { .. } => "roughing_3d",
            OperationKind::RasterFinish { .. } => "raster_finish",
            OperationKind::WaterlineFinish { .. } => "waterline_finish",
            OperationKind::TurnFace => "turn_face",
            OperationKind::TurnRough { .. } => "turn_rough",
            OperationKind::TurnFinish { .. } => "turn_finish",
            OperationKind::TurnGroove { .. } => "turn_groove",
            OperationKind::TurnPartOff { .. } => "turn_part_off",
            OperationKind::TurnThread { .. } => "turn_thread",
        }
    }

    /// Whether it is a turning operation (a lathe's).
    pub fn is_turning(&self) -> bool {
        matches!(
            self,
            OperationKind::TurnFace
                | OperationKind::TurnRough { .. }
                | OperationKind::TurnFinish { .. }
                | OperationKind::TurnGroove { .. }
                | OperationKind::TurnPartOff { .. }
                | OperationKind::TurnThread { .. }
        )
    }

    /// Whether it machines the bodies' surfaces (3-axis), from their
    /// meshes.
    pub fn machines_surfaces(&self) -> bool {
        matches!(
            self,
            OperationKind::Roughing3d { .. }
                | OperationKind::RasterFinish { .. }
                | OperationKind::WaterlineFinish { .. }
        )
    }

    /// Whether a live tool on a lathe cuts it (mitcad#2067): facing,
    /// contours, pockets, adaptive clearing and drilling.
    pub fn cuts_live(&self) -> bool {
        matches!(
            self,
            OperationKind::Facing
                | OperationKind::Contour { .. }
                | OperationKind::Pocket
                | OperationKind::Adaptive
                | OperationKind::Drill { .. }
        )
    }

    /// The default name, before its number: `Pocket 2`.
    pub fn words(&self) -> &'static str {
        match self {
            OperationKind::Facing => "Facing",
            OperationKind::Contour { .. } => "Contour",
            OperationKind::Pocket => "Pocket",
            OperationKind::Drill { .. } => "Drill",
            OperationKind::Adaptive => "Adaptive",
            OperationKind::Rest { .. } => "Rest",
            OperationKind::Roughing3d { .. } => "3D Roughing",
            OperationKind::RasterFinish { .. } => "Raster Finishing",
            OperationKind::WaterlineFinish { .. } => "Waterline Finishing",
            OperationKind::TurnFace => "Turn Facing",
            OperationKind::TurnRough { .. } => "Turn Roughing",
            OperationKind::TurnFinish { .. } => "Turn Finishing",
            OperationKind::TurnGroove { .. } => "Grooving",
            OperationKind::TurnPartOff { .. } => "Parting Off",
            OperationKind::TurnThread { .. } => "Threading",
        }
    }
}

/// A geometric input of an operation, by the references the rest of the
/// model uses, each with the occurrence it was picked in (`occurrence`,
/// `"O1/O4"`: the occurrences from the design's root down to the
/// component of the body or sketch, as joints and sections keep theirs).
/// The outline is computed where that occurrence places it, so that the
/// same face picked in each placement of a component placed twice gives
/// two outlines. Left out (empty) for the root's own geometry, or for a
/// component placed once (its only placement is taken).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CamInput {
    /// The stock's outline at its top.
    Stock,
    /// A planar face level with the work plane: its loops (a pocket's
    /// floor and walls, a contour's outline) at its height.
    Face {
        body: BodyUid,
        face: FaceName,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
    /// A sketch's profile region: its loops at the sketch plane.
    Region {
        sketch: FeatureUid,
        region: RegionKey,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
    /// Edges of a body, chained into outlines.
    Edges {
        body: BodyUid,
        edges: Vec<EdgeName>,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
    /// Curves of a sketch (`c3`), chained into outlines.
    Curves {
        sketch: FeatureUid,
        curves: Vec<String>,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
    /// A point: a hole's centre for drilling (a point reference, or a
    /// cylindrical face's axis).
    Point {
        at: GeomRef,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
    /// A body whose surfaces a 3-axis operation machines (phase 3).
    Body {
        body: BodyUid,
        #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
        occurrence: OccurrencePath,
    },
}

impl CamInput {
    /// The body or sketch it reads.
    pub fn source(&self) -> (Option<BodyUid>, Option<FeatureUid>) {
        match self {
            CamInput::Stock | CamInput::Point { .. } => (None, None),
            CamInput::Face { body, .. }
            | CamInput::Edges { body, .. }
            | CamInput::Body { body, .. } => (Some(*body), None),
            CamInput::Region { sketch, .. } | CamInput::Curves { sketch, .. } => {
                (None, Some(*sketch))
            }
        }
    }

    /// The occurrence it was picked in; none for the stock.
    pub fn occurrence(&self) -> Option<&OccurrencePath> {
        match self {
            CamInput::Stock => None,
            CamInput::Face { occurrence, .. }
            | CamInput::Region { occurrence, .. }
            | CamInput::Edges { occurrence, .. }
            | CamInput::Curves { occurrence, .. }
            | CamInput::Point { occurrence, .. }
            | CamInput::Body { occurrence, .. } => Some(occurrence),
        }
    }
}

/// An operation of a setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CamOperation {
    /// Unique among the design's setups and operations.
    pub id: u32,
    pub name: String,
    pub kind: OperationKind,
    /// The tool's number in the setup's machine's library.
    pub tool: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<CamInput>,
    #[serde(default)]
    pub depths: DepthsInput,
    #[serde(default)]
    pub heights: HeightsInput,
    #[serde(default)]
    pub cutting: CuttingInput,
    /// The distance between rows or loops; the operation's default share
    /// of the diameter when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stepover: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_to_leave: Option<CamValue>,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub spindle: SpindleDirection,
    #[serde(default)]
    pub coolant: Coolant,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Entry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tabs: Option<Tabs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<Lead>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
    /// The fingerprint of the inputs its toolpath was last generated from
    /// (32 hex digits); none before the first generate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<String>,
    /// A turning operation's speeds, feeds and depths (mitcad#1869).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turning: Option<TurningInput>,
    /// Where a milling operation in a lathe setup cuts with its live tool
    /// (mitcad#2067): on the face when left out. On the diameter its top,
    /// bottom, clearance and retract are diameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<LivePlace>,
    /// On a plane (mitcad#2319): the C axis's angle and the tool's tilt
    /// (degrees), each from the inputs when left out (a planar face's
    /// normal, a hole's axis).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<LivePlaneAngles>,
    /// An operation on the sub-spindle, after the setup's hand-over: a
    /// turning one turns the part's back (mitcad#2319), a live tool's cuts
    /// it with the sub-spindle's C axis holding it (mitcad#2375), each in
    /// the sub-spindle's work coordinates.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sub_spindle: bool,
}

/// The angles of a live tool's plane (mitcad#2319): `c` the C axis's
/// (from the work X towards the work Y), `b` the tool's tilt from the
/// spindle axis (0 the face, 90 a flat across it).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LivePlaneAngles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<CamValue>,
}

/// A turning operation's values (mitcad#1869), each the operation's
/// default when left out: the spindle at a constant surface speed
/// (m/min) under a limit (rpm), or at a constant speed (the cutting
/// values' rpm) when `constant_rpm`, as threading always is; the feed per
/// revolution; the depth of cut (radius) of roughing, the peck of grooving
/// and parting; the stock to leave along Z (the operation's stock to leave,
/// a radius, when left out).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TurningInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_speed: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rpm: Option<CamValue>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub constant_rpm: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_per_rev: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth_of_cut: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peck: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_to_leave_z: Option<CamValue>,
    /// Turn Roughing and Turn Finishing within limits along the work Z
    /// (mitcad#2629): from (towards the face) and to (towards the chuck);
    /// the face and the part's end when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_from: Option<CamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_to: Option<CamValue>,
}

impl CamOperation {
    /// Every value it has, with what it measures, for the fingerprint.
    pub fn values(&self) -> Vec<(&CamValue, ValueKind)> {
        use ValueKind::{Angle, Length, Number};
        let mut out: Vec<(&CamValue, ValueKind)> = Vec::new();
        for (value, kind) in [
            (&self.depths.top, Length),
            (&self.depths.bottom, Length),
            (&self.depths.step_down, Length),
            (&self.heights.clearance, Length),
            (&self.heights.retract, Length),
            (&self.cutting.spindle_rpm, Number),
            (&self.cutting.feed, Length),
            (&self.cutting.plunge_feed, Length),
            (&self.stepover, Length),
            (&self.stock_to_leave, Length),
        ] {
            if let Some(value) = value {
                out.push((value, kind));
            }
        }
        match &self.entry {
            Some(Entry::Ramp { angle }) => out.push((angle, Angle)),
            Some(Entry::Helix { radius, angle }) => {
                if let Some(radius) = radius {
                    out.push((radius, Length));
                }
                out.push((angle, Angle));
            }
            Some(Entry::Plunge) | None => {}
        }
        if let Some(tabs) = &self.tabs {
            out.push((&tabs.width, Length));
            out.push((&tabs.height, Length));
        }
        if let Some(lead) = &self.lead {
            out.push((&lead.radius, Length));
        }
        // The 3-axis operations' angles.
        match &self.kind {
            OperationKind::RasterFinish { angle, slope } => {
                out.extend([angle, slope].into_iter().flatten().map(|v| (v, Angle)));
            }
            OperationKind::WaterlineFinish { slope: Some(slope) } => out.push((slope, Angle)),
            _ => {}
        }
        // Turning (mitcad#1869).
        match &self.kind {
            OperationKind::TurnGroove { z_from, z_to, .. } => {
                out.extend([z_from, z_to].into_iter().flatten().map(|v| (v, Length)));
            }
            OperationKind::TurnPartOff { z: Some(z) } => out.push((z, Length)),
            OperationKind::TurnThread {
                pitch,
                diameter,
                z_start,
                z_end,
                depth,
                ..
            } => {
                out.push((pitch, Length));
                out.extend(
                    [diameter, z_start, z_end, depth]
                        .into_iter()
                        .flatten()
                        .map(|v| (v, Length)),
                );
            }
            _ => {}
        }
        if let Some(t) = &self.turning {
            for (value, kind) in [
                (&t.surface_speed, Number),
                (&t.max_rpm, Number),
                (&t.feed_per_rev, Length),
                (&t.depth_of_cut, Length),
                (&t.peck, Length),
                (&t.stock_to_leave_z, Length),
                (&t.limit_from, Length),
                (&t.limit_to, Length),
            ] {
                if let Some(value) = value {
                    out.push((value, kind));
                }
            }
        }
        // A live tool's plane (mitcad#2319).
        if let Some(plane) = &self.plane {
            out.extend(
                [&plane.c, &plane.b]
                    .into_iter()
                    .flatten()
                    .map(|v| (v, Angle)),
            );
        }
        out
    }

    /// Every value it has, as [`CamOperation::values`] lists them (the
    /// same values in the same order), to be changed: a parameter's rename
    /// rewrites the expressions (mitcad#1830).
    pub fn values_mut(&mut self) -> Vec<(&mut CamValue, ValueKind)> {
        use ValueKind::{Angle, Length, Number};
        let mut out: Vec<(&mut CamValue, ValueKind)> = Vec::new();
        for (value, kind) in [
            (&mut self.depths.top, Length),
            (&mut self.depths.bottom, Length),
            (&mut self.depths.step_down, Length),
            (&mut self.heights.clearance, Length),
            (&mut self.heights.retract, Length),
            (&mut self.cutting.spindle_rpm, Number),
            (&mut self.cutting.feed, Length),
            (&mut self.cutting.plunge_feed, Length),
            (&mut self.stepover, Length),
            (&mut self.stock_to_leave, Length),
        ] {
            if let Some(value) = value {
                out.push((value, kind));
            }
        }
        match &mut self.entry {
            Some(Entry::Ramp { angle }) => out.push((angle, Angle)),
            Some(Entry::Helix { radius, angle }) => {
                if let Some(radius) = radius {
                    out.push((radius, Length));
                }
                out.push((angle, Angle));
            }
            Some(Entry::Plunge) | None => {}
        }
        if let Some(tabs) = &mut self.tabs {
            out.push((&mut tabs.width, Length));
            out.push((&mut tabs.height, Length));
        }
        if let Some(lead) = &mut self.lead {
            out.push((&mut lead.radius, Length));
        }
        match &mut self.kind {
            OperationKind::RasterFinish { angle, slope } => {
                out.extend([angle, slope].into_iter().flatten().map(|v| (v, Angle)));
            }
            OperationKind::WaterlineFinish { slope: Some(slope) } => out.push((slope, Angle)),
            // Turning (mitcad#1869).
            OperationKind::TurnGroove { z_from, z_to, .. } => {
                out.extend([z_from, z_to].into_iter().flatten().map(|v| (v, Length)));
            }
            OperationKind::TurnPartOff { z: Some(z) } => out.push((z, Length)),
            OperationKind::TurnThread {
                pitch,
                diameter,
                z_start,
                z_end,
                depth,
                ..
            } => {
                out.push((pitch, Length));
                out.extend(
                    [diameter, z_start, z_end, depth]
                        .into_iter()
                        .flatten()
                        .map(|v| (v, Length)),
                );
            }
            _ => {}
        }
        if let Some(t) = &mut self.turning {
            for (value, kind) in [
                (&mut t.surface_speed, Number),
                (&mut t.max_rpm, Number),
                (&mut t.feed_per_rev, Length),
                (&mut t.depth_of_cut, Length),
                (&mut t.peck, Length),
                (&mut t.stock_to_leave_z, Length),
                (&mut t.limit_from, Length),
                (&mut t.limit_to, Length),
            ] {
                if let Some(value) = value {
                    out.push((value, kind));
                }
            }
        }
        // A live tool's plane (mitcad#2319).
        if let Some(plane) = &mut self.plane {
            out.extend(
                [&mut plane.c, &mut plane.b]
                    .into_iter()
                    .flatten()
                    .map(|v| (v, Angle)),
            );
        }
        out
    }
}

impl CamValue {
    /// Whether it is an expression that names a parameter.
    pub fn uses_parameter(&self, name: &str) -> bool {
        match self {
            CamValue::Number(_) => false,
            CamValue::Expression(text) => {
                crate::expr::Expr::parse(text).is_ok_and(|expr| expr.references().contains(&name))
            }
        }
    }

    /// Rewrites an expression's references to a renamed parameter
    /// (mitcad#1830); a number, and a text that does not parse, stay.
    pub fn rename_parameter(&mut self, old: &str, new: &str) {
        if let CamValue::Expression(text) = self
            && let Ok(expr) = crate::expr::Expr::parse(text)
            && expr.references().contains(&old)
        {
            *text = crate::expr::replace_references(text, &expr, old, new);
        }
    }
}

impl CamSetup {
    /// Every value of the setup itself (not its operations').
    pub fn values(&self) -> Vec<(&CamValue, ValueKind)> {
        let mut out = vec![(&self.safe_height, ValueKind::Length)];
        if let Stock::AroundBodies { side, top, bottom } = &self.stock {
            out.extend([
                (side, ValueKind::Length),
                (top, ValueKind::Length),
                (bottom, ValueKind::Length),
            ]);
        }
        if let Some(lathe) = &self.lathe {
            out.extend(lathe.values().into_iter().map(|v| (v, ValueKind::Length)));
        }
        out
    }

    /// Every value of the setup itself, as [`CamSetup::values`] lists
    /// them, to be changed.
    pub fn values_mut(&mut self) -> Vec<(&mut CamValue, ValueKind)> {
        let mut out = vec![(&mut self.safe_height, ValueKind::Length)];
        if let Stock::AroundBodies { side, top, bottom } = &mut self.stock {
            out.extend([
                (side, ValueKind::Length),
                (top, ValueKind::Length),
                (bottom, ValueKind::Length),
            ]);
        }
        if let Some(lathe) = &mut self.lathe {
            out.extend(
                lathe
                    .values_mut()
                    .into_iter()
                    .map(|v| (v, ValueKind::Length)),
            );
        }
        out
    }

    pub fn operation(&self, id: u32) -> Option<&CamOperation> {
        self.operations.iter().find(|o| o.id == id)
    }
}

fn cam_version() -> u32 {
    CAM_VERSION
}

fn one_u32() -> u32 {
    1
}

/// The design's CAM: its machines and setups (the file's `cam` section).
/// Read tolerantly: fields a newer MitCad adds inside it are ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CamState {
    #[serde(default = "cam_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub machines: Vec<CamMachine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setups: Vec<CamSetup>,
    /// The id the next setup or operation takes.
    #[serde(default = "one_u32")]
    pub next_id: u32,
}

impl Default for CamState {
    fn default() -> Self {
        CamState {
            version: CAM_VERSION,
            machines: Vec::new(),
            setups: Vec::new(),
            next_id: 1,
        }
    }
}

impl CamState {
    /// Nothing to save: the file leaves the section out.
    pub fn is_empty(&self) -> bool {
        self.machines.is_empty() && self.setups.is_empty()
    }

    pub fn machine(&self, id: &str) -> Option<&CamMachine> {
        self.machines.iter().find(|m| m.id == id)
    }

    /// The setup and the index of an operation.
    pub fn find_operation(&self, id: u32) -> Option<(usize, usize)> {
        self.setups.iter().enumerate().find_map(|(s, setup)| {
            setup
                .operations
                .iter()
                .position(|o| o.id == id)
                .map(|o| (s, o))
        })
    }

    /// The largest id used.
    fn largest_id(&self) -> u32 {
        self.setups
            .iter()
            .flat_map(|s| std::iter::once(s.id).chain(s.operations.iter().map(|o| o.id)))
            .max()
            .unwrap_or(0)
    }

    /// Takes the next id.
    pub fn take_id(&mut self) -> u32 {
        let id = self.next_id.max(self.largest_id() + 1);
        self.next_id = id + 1;
        id
    }

    /// Rewrites every setup's and operation's expressions that name a
    /// renamed parameter (mitcad#1830), as the bindings follow a rename.
    pub fn rename_parameter(&mut self, old: &str, new: &str) {
        for setup in &mut self.setups {
            for (value, _) in setup.values_mut() {
                value.rename_parameter(old, new);
            }
            for op in &mut setup.operations {
                for (value, _) in op.values_mut() {
                    value.rename_parameter(old, new);
                }
            }
        }
    }

    /// The names of the parameters its setups' and operations' values use.
    pub fn parameter_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for setup in &self.setups {
            let values = setup
                .values()
                .into_iter()
                .chain(setup.operations.iter().flat_map(CamOperation::values));
            for (value, _) in values {
                if let CamValue::Expression(text) = value
                    && let Ok(expr) = crate::expr::Expr::parse(text)
                {
                    for name in expr.references() {
                        if !names.iter().any(|n| n == name) {
                            names.push(name.to_owned());
                        }
                    }
                }
            }
        }
        names
    }

    /// The setups and operations whose values name a parameter, in words,
    /// for refusing its delete (mitcad#1830): `the CAM setup Setup 1`,
    /// `the CAM operation Pocket 1 of Setup 1`.
    pub fn parameter_users(&self, name: &str) -> Vec<String> {
        let mut users = Vec::new();
        for setup in &self.setups {
            if setup.values().iter().any(|(v, _)| v.uses_parameter(name)) {
                users.push(format!("the CAM setup {}", setup.name));
            }
            for op in &setup.operations {
                if op.values().iter().any(|(v, _)| v.uses_parameter(name)) {
                    users.push(format!("the CAM operation {} of {}", op.name, setup.name));
                }
            }
        }
        users
    }

    /// What is wrong with the section as a whole: ids, names and machines
    /// (an input whose body or sketch is gone is kept: the toolpath says
    /// so). The next id is set past the largest.
    pub fn check(&mut self) -> Result<(), String> {
        if self.machines.len() > MAX_MACHINES {
            return Err(format!("a design keeps at most {MAX_MACHINES} machines"));
        }
        if self.setups.len() > MAX_SETUPS {
            return Err(format!("a design has at most {MAX_SETUPS} setups"));
        }
        for (i, machine) in self.machines.iter().enumerate() {
            machine.check_stored()?;
            if self.machines[..i].iter().any(|m| m.id == machine.id) {
                return Err("two machines have the same id".to_owned());
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        for setup in &self.setups {
            check_setup(setup)?;
            if let Some(machine) = &setup.machine
                && self.machine(machine).is_none()
            {
                return Err(format!(
                    "the setup {} uses a machine the design does not keep",
                    setup.name
                ));
            }
            if !ids.insert(setup.id) {
                return Err("two setups or operations have the same id".to_owned());
            }
            for op in &setup.operations {
                check_operation(op)?;
                if !ids.insert(op.id) {
                    return Err("two setups or operations have the same id".to_owned());
                }
            }
        }
        self.next_id = self.next_id.max(self.largest_id() + 1);
        Ok(())
    }
}

/// A name of 1 to [`MAX_NAME`] characters.
pub fn check_name(name: &str, what: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("A {what} needs a name."));
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!(
            "A {what}'s name has at most {MAX_NAME} characters."
        ));
    }
    Ok(())
}

/// Whether MitCad writes for a setup's dialect, when one is set: checked
/// where a setup is added or its dialect set, not when a design is read
/// (a dialect of another build opens, and the post refuses it in words,
/// mitcad#2531).
pub fn check_dialect(dialect: Option<&str>) -> Result<(), String> {
    if let Some(dialect) = dialect
        && engine::dialect(dialect).is_none()
        && engine::turning_dialect(dialect).is_none()
    {
        return Err("MitCad does not write G-code for that controller.".to_owned());
    }
    Ok(())
}

/// What is wrong with a setup by itself (its dialect: [`check_dialect`]).
pub fn check_setup(setup: &CamSetup) -> Result<(), String> {
    check_name(&setup.name, "setup")?;
    if !(1..=6).contains(&setup.work_offset) {
        return Err("The work offset must be one of G54 to G59.".to_owned());
    }
    if !(setup.post.arc_tolerance.is_finite() && setup.post.arc_tolerance > 0.0) {
        return Err("The arc tolerance must be more than zero.".to_owned());
    }
    if setup
        .placement
        .is_some_and(|p| p.iter().any(|v| !v.is_finite()))
    {
        return Err("The work zero's machine position must be numbers.".to_owned());
    }
    if setup.operations.len() > MAX_OPERATIONS {
        return Err(format!("A setup has at most {MAX_OPERATIONS} operations."));
    }
    if let Some(number) = setup.lathe.as_ref().and_then(|l| l.program_number)
        && !(1..=9999).contains(&number)
    {
        return Err("The program number must be from 1 to 9999.".to_owned());
    }
    // Manual G-code (mitcad#2506); what the controller takes is checked
    // when the program is written.
    let manual = setup.lathe.as_ref().map_or(&[][..], |l| &l.manual_code[..]);
    if manual.len() > MAX_MANUAL_BLOCKS {
        return Err(format!(
            "A setup has at most {MAX_MANUAL_BLOCKS} blocks of manual G-code."
        ));
    }
    for block in manual {
        check_name(&block.name, "manual G-code block")?;
        let lines = block.lines();
        if lines.is_empty() {
            return Err(format!("The manual G-code {} has no code.", block.name));
        }
        if lines.len() > MAX_MANUAL_LINES {
            return Err(format!(
                "The manual G-code {} has more than {MAX_MANUAL_LINES} lines.",
                block.name
            ));
        }
    }
    Ok(())
}

/// The refusal of a sketch text named in an input of sketch curves
/// (mitcad#2588, GitHub mitcad/Mitcad#745): the text is one outline of
/// glyphs, not curves the outline building reads. Written without an id,
/// so that the notice cleaner leaves it whole.
pub const SKETCH_TEXT_AS_CURVES: &str = "A CAM operation cannot follow a sketch text: Explode Text turns it into sketch curves to pick instead.";

/// The refusal of any other name in an input of sketch curves that is no
/// curve's id (mitcad#2588), without an example id for the same reason.
pub const NOT_A_SKETCH_CURVE: &str = "An input of sketch curves names the sketch's curves by their ids, as the sketch query lists them.";

/// A curve of an input of sketch curves by its name (`c3`): a sketch
/// text's id (`t3`) is refused with what to do instead, any other name
/// that is no curve's id with how curves are named (mitcad#2588). The
/// operation check and both outline builders (planar and wrapped) read
/// the names through it.
pub fn curve_input_uid(name: &str) -> Result<crate::ids::EntityUid, String> {
    crate::ids::EntityUid::parse_curve(name).map_err(|_| {
        if crate::ids::parse_number::<u32>(name, "t").is_some() {
            SKETCH_TEXT_AS_CURVES.to_owned()
        } else {
            NOT_A_SKETCH_CURVE.to_owned()
        }
    })
}

/// What is wrong with an operation by itself.
pub fn check_operation(op: &CamOperation) -> Result<(), String> {
    check_name(&op.name, "operation")?;
    if !(1..=999).contains(&op.tool) {
        return Err("An operation's tool number must be from 1 to 999.".to_owned());
    }
    if let Some(tabs) = &op.tabs
        && !(1..=64).contains(&tabs.count)
    {
        return Err("An outline has 1 to 64 holding tabs.".to_owned());
    }
    // A turning operation reads the setup's bodies; grooving and
    // threading may read the face they cut (mitcad#1869).
    if op.kind.is_turning() {
        let faces = matches!(
            op.kind,
            OperationKind::TurnGroove { .. } | OperationKind::TurnThread { .. }
        );
        if op
            .inputs
            .iter()
            .any(|i| !(faces && matches!(i, CamInput::Face { .. })))
        {
            return Err(
                "A turning operation turns the setup's bodies; grooving and threading also take the face they cut."
                    .to_owned(),
            );
        }
        if op.live.is_some() || op.plane.is_some() {
            return Err("A turning operation turns: it has no live tool's place.".to_owned());
        }
        return Ok(());
    }
    // The sub-spindle turns the part's back (mitcad#2319) and holds it for
    // the live tools (mitcad#2375): a milling operation on it cuts with a
    // live tool.
    if op.sub_spindle && !op.kind.cuts_live() {
        return Err(
            "On the sub-spindle an operation turns the part's back or cuts it with a live tool."
                .to_owned(),
        );
    }
    if op.plane.is_some() && op.live != Some(LivePlace::Plane) {
        return Err("Plane angles are for a live tool on a plane.".to_owned());
    }
    // A live tool's place on a lathe (mitcad#2067).
    if let Some(place) = op.live {
        if !op.kind.cuts_live() {
            return Err(
                "Live tools face, contour, pocket, clear adaptively and drill; the other operations do not cut with them."
                    .to_owned(),
            );
        }
        if place == LivePlace::Diameter {
            let drill = matches!(op.kind, OperationKind::Drill { .. });
            if matches!(op.kind, OperationKind::Facing) {
                return Err(
                    "A live tool faces the part's face: on the diameter, contour or pocket the faces round the spindle axis."
                        .to_owned(),
                );
            }
            // Faces round the axis, or a sketch's profiles and curves
            // wrapped onto the diameter (mitcad#2319).
            let fits = |i: &CamInput| {
                if drill {
                    matches!(i, CamInput::Point { .. })
                } else {
                    matches!(
                        i,
                        CamInput::Face { .. } | CamInput::Region { .. } | CamInput::Curves { .. }
                    )
                }
            };
            if !op.inputs.iter().all(fits) {
                return Err(
                    "On the diameter a live tool mills the faces round the spindle axis or the sketch geometry wrapped onto it, and drills holes at points."
                        .to_owned(),
                );
            }
        }
    }
    // The 3-axis operations read bodies and a boundary of faces or
    // profiles; only they read bodies.
    let surfaces = op.kind.machines_surfaces();
    for input in &op.inputs {
        let fits = match input {
            CamInput::Body { .. } => surfaces,
            CamInput::Edges { .. } | CamInput::Curves { .. } | CamInput::Point { .. } => !surfaces,
            CamInput::Stock | CamInput::Face { .. } | CamInput::Region { .. } => true,
        };
        if !fits {
            return Err(if surfaces {
                "A 3D operation takes bodies, and faces or sketch profiles as its boundary."
                    .to_owned()
            } else {
                "Only the 3D operations take bodies as inputs.".to_owned()
            });
        }
    }
    for input in &op.inputs {
        match input {
            CamInput::Edges { edges, .. } if edges.is_empty() => {
                return Err("An input of edges needs at least one edge.".to_owned());
            }
            CamInput::Curves { curves, .. } => {
                if curves.is_empty() {
                    return Err("An input of sketch curves needs at least one curve.".to_owned());
                }
                for curve in curves {
                    curve_input_uid(curve)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// Command inputs.

/// Reads a field that may be given as null to clear it: absent is None,
/// null is Some(None).
fn double<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// A setup by its id or its name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SetupRef {
    Id(u32),
    Name(String),
}

/// The fields of `cam_setup_add` and `cam_setup_edit`: those given are
/// set; `machine`, `dialect` and `placement` are cleared by null.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CamSetupInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub bodies: Option<Vec<CamBody>>,
    #[serde(default)]
    pub stock: Option<Stock>,
    #[serde(default)]
    pub wcs: Option<Wcs>,
    #[serde(default)]
    pub work_offset: Option<u8>,
    #[serde(default)]
    pub safe_height: Option<CamValue>,
    #[serde(default, deserialize_with = "double")]
    pub machine: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub dialect: Option<Option<String>>,
    #[serde(default)]
    pub post: Option<PostSettings>,
    #[serde(default)]
    pub probe_work_zero: Option<bool>,
    #[serde(default, deserialize_with = "double")]
    pub placement: Option<Option<[f64; 3]>>,
    /// Its toolpaths shown (the Browser's eye).
    #[serde(default)]
    pub visible: Option<bool>,
    /// A lathe setup's values (mitcad#1869); null clears them.
    #[serde(default, deserialize_with = "double")]
    pub lathe: Option<Option<LatheSetup>>,
}

impl CamSetupInput {
    /// Only the eye: `{"visible": …}` and nothing else.
    pub fn only_visibility(&self) -> Option<bool> {
        let visible = self.visible?;
        let rest = CamSetupInput {
            visible: None,
            ..self.clone()
        };
        (rest == CamSetupInput::default()).then_some(visible)
    }

    /// Sets what is given on a setup.
    pub fn apply(self, setup: &mut CamSetup) {
        if let Some(name) = self.name {
            setup.name = name.trim().to_owned();
        }
        if let Some(bodies) = self.bodies {
            setup.bodies = bodies;
        }
        if let Some(stock) = self.stock {
            setup.stock = stock;
        }
        if let Some(wcs) = self.wcs {
            setup.wcs = wcs;
        }
        if let Some(offset) = self.work_offset {
            setup.work_offset = offset;
        }
        if let Some(height) = self.safe_height {
            setup.safe_height = height;
        }
        if let Some(machine) = self.machine {
            setup.machine = machine;
        }
        if let Some(dialect) = self.dialect {
            setup.dialect = dialect;
        }
        if let Some(post) = self.post {
            setup.post = post;
        }
        if let Some(probe) = self.probe_work_zero {
            setup.probe_work_zero = probe;
        }
        if let Some(placement) = self.placement {
            setup.placement = placement;
        }
        if let Some(visible) = self.visible {
            setup.visible = visible;
        }
        if let Some(lathe) = self.lathe {
            setup.lathe = lathe;
        }
    }
}

/// The fields of `cam_operation_add` (`kind` and `tool` needed) and
/// `cam_operation_edit`: those given are set; the optional values are
/// cleared by null (back to their defaults).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CamOperationInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: Option<OperationKind>,
    #[serde(default)]
    pub tool: Option<u32>,
    #[serde(default)]
    pub inputs: Option<Vec<CamInput>>,
    #[serde(default)]
    pub depths: Option<DepthsInput>,
    #[serde(default)]
    pub heights: Option<HeightsInput>,
    #[serde(default)]
    pub cutting: Option<CuttingInput>,
    #[serde(default, deserialize_with = "double")]
    pub stepover: Option<Option<CamValue>>,
    #[serde(default, deserialize_with = "double")]
    pub stock_to_leave: Option<Option<CamValue>>,
    #[serde(default)]
    pub direction: Option<Direction>,
    #[serde(default)]
    pub spindle: Option<SpindleDirection>,
    #[serde(default)]
    pub coolant: Option<Coolant>,
    #[serde(default, deserialize_with = "double")]
    pub entry: Option<Option<Entry>>,
    #[serde(default, deserialize_with = "double")]
    pub tabs: Option<Option<Tabs>>,
    #[serde(default, deserialize_with = "double")]
    pub lead: Option<Option<Lead>>,
    #[serde(default)]
    pub suppressed: Option<bool>,
    /// A turning operation's values (mitcad#1869); null clears them.
    #[serde(default, deserialize_with = "double")]
    pub turning: Option<Option<TurningInput>>,
    /// Where a live tool cuts on a lathe (mitcad#2067); null: the face.
    #[serde(default, deserialize_with = "double")]
    pub live: Option<Option<LivePlace>>,
    /// A live tool's plane angles (mitcad#2319); null: from the inputs.
    #[serde(default, deserialize_with = "double")]
    pub plane: Option<Option<LivePlaneAngles>>,
    /// An operation on the sub-spindle: turning (mitcad#2319) or a live
    /// tool's (mitcad#2375).
    #[serde(default)]
    pub sub_spindle: Option<bool>,
}

impl CamOperationInput {
    /// Sets what is given on an operation.
    pub fn apply(self, op: &mut CamOperation) {
        if let Some(name) = self.name {
            op.name = name.trim().to_owned();
        }
        if let Some(kind) = self.kind {
            op.kind = kind;
        }
        if let Some(tool) = self.tool {
            op.tool = tool;
        }
        if let Some(inputs) = self.inputs {
            op.inputs = inputs;
        }
        if let Some(depths) = self.depths {
            op.depths = depths;
        }
        if let Some(heights) = self.heights {
            op.heights = heights;
        }
        if let Some(cutting) = self.cutting {
            op.cutting = cutting;
        }
        if let Some(stepover) = self.stepover {
            op.stepover = stepover;
        }
        if let Some(stock) = self.stock_to_leave {
            op.stock_to_leave = stock;
        }
        if let Some(direction) = self.direction {
            op.direction = direction;
        }
        if let Some(spindle) = self.spindle {
            op.spindle = spindle;
        }
        if let Some(coolant) = self.coolant {
            op.coolant = coolant;
        }
        if let Some(entry) = self.entry {
            op.entry = entry;
        }
        if let Some(tabs) = self.tabs {
            op.tabs = tabs;
        }
        if let Some(lead) = self.lead {
            op.lead = lead;
        }
        if let Some(suppressed) = self.suppressed {
            op.suppressed = suppressed;
        }
        if let Some(turning) = self.turning {
            op.turning = turning;
        }
        if let Some(live) = self.live {
            op.live = live;
        }
        if let Some(plane) = self.plane {
            op.plane = plane;
        }
        if let Some(sub) = self.sub_spindle {
            op.sub_spindle = sub;
        }
    }
}

/// A tool as `cam_tool_put` takes it: lengths as [`CamValue`]s (inches
/// with their unit), angles in degrees.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CamToolInput {
    pub number: u32,
    #[serde(default)]
    pub name: Option<String>,
    pub shape: ToolShapeInput,
    pub diameter: CamValue,
    #[serde(default)]
    pub shank_diameter: Option<CamValue>,
    /// Three diameters when left out.
    #[serde(default)]
    pub flute_length: Option<CamValue>,
    /// The flute length and 5 mm when left out.
    #[serde(default)]
    pub length: Option<CamValue>,
    #[serde(default)]
    pub flutes: Option<u32>,
    #[serde(default)]
    pub holder: Option<String>,
    #[serde(default)]
    pub cutting: Option<CuttingValues>,
    /// A live tool's holder on a lathe (mitcad#2067).
    #[serde(default)]
    pub mount: Option<LiveMount>,
    /// A turning tool's or boring bar's insert and holder (mitcad#2502).
    #[serde(default)]
    pub insert: Option<InsertInput>,
}

/// A turning tool's insert and holder as `cam_tool_put` takes them
/// (mitcad#2502): ISO codes (`CNMG 120408`, `PCLNR 2525 M12`, a boring
/// bar's `S25S-PCLNR 12`), angles in degrees, a hand, or codes and values
/// together: a typed value wins over the code's and is kept as typed, so
/// that leaving it out later goes back to the code's.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsertInput {
    #[serde(default)]
    pub insert: Option<String>,
    #[serde(default)]
    pub holder: Option<String>,
    #[serde(default)]
    pub tip_angle: Option<CamValue>,
    #[serde(default)]
    pub approach_angle: Option<CamValue>,
    #[serde(default)]
    pub hand: Option<engine::turning::Hand>,
}

impl InsertInput {
    /// Whether nothing is given: the tool then has no insert.
    pub fn is_empty(&self) -> bool {
        let blank = |s: &Option<String>| s.as_deref().is_none_or(|s| s.trim().is_empty());
        blank(&self.insert)
            && blank(&self.holder)
            && self.tip_angle.is_none()
            && self.approach_angle.is_none()
            && self.hand.is_none()
    }
}

/// A tool's kind with its values as [`CamValue`]s.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToolShapeInput {
    Flat,
    Ball,
    Bull {
        corner_radius: CamValue,
    },
    Drill {
        #[serde(default)]
        point_angle: Option<CamValue>,
    },
    Chamfer {
        #[serde(default)]
        angle: Option<CamValue>,
        #[serde(default)]
        tip_diameter: Option<CamValue>,
    },
    Engraver {
        #[serde(default)]
        angle: Option<CamValue>,
        #[serde(default)]
        tip_diameter: Option<CamValue>,
    },
    // Lathe tools (mitcad#1869).
    Turning {
        #[serde(default)]
        nose_radius: Option<CamValue>,
    },
    Boring {
        #[serde(default)]
        nose_radius: Option<CamValue>,
    },
    Grooving {
        width: CamValue,
        #[serde(default)]
        nose_radius: Option<CamValue>,
        #[serde(default)]
        inside: bool,
    },
    Parting {
        width: CamValue,
    },
    Threading {
        #[serde(default)]
        angle: Option<CamValue>,
        #[serde(default)]
        inside: bool,
    },
}

/// A tool's default speeds and feeds as values (feeds a length per
/// minute: `"40 in"` is 40 inches per minute).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuttingValues {
    pub spindle_rpm: CamValue,
    pub feed: CamValue,
    pub plunge_feed: CamValue,
}

impl CamToolInput {
    /// The tool in millimetres and degrees, checked.
    pub fn resolve(&self, params: &Parameters) -> Result<CamTool, String> {
        let length = |v: &CamValue| v.evaluate(params, ValueKind::Length);
        let angle = |v: &Option<CamValue>, default: f64| match v {
            Some(v) => v.evaluate(params, ValueKind::Angle),
            None => Ok(default),
        };
        let opt_length = |v: &Option<CamValue>| v.as_ref().map(length).transpose();
        let shape = match &self.shape {
            ToolShapeInput::Flat => ToolKind::Flat,
            ToolShapeInput::Ball => ToolKind::Ball,
            ToolShapeInput::Bull { corner_radius } => ToolKind::Bull {
                corner_radius: length(corner_radius)?,
            },
            ToolShapeInput::Drill { point_angle } => ToolKind::Drill {
                point_angle: angle(point_angle, drill_point())?,
            },
            ToolShapeInput::Chamfer {
                angle: a,
                tip_diameter,
            } => ToolKind::Chamfer {
                angle: angle(a, ninety())?,
                tip_diameter: opt_length(tip_diameter)?.unwrap_or(0.0),
            },
            ToolShapeInput::Engraver {
                angle: a,
                tip_diameter,
            } => ToolKind::Engraver {
                angle: angle(a, thirty())?,
                tip_diameter: opt_length(tip_diameter)?.unwrap_or(0.0),
            },
            ToolShapeInput::Turning { nose_radius } => ToolKind::Turning {
                nose_radius: opt_length(nose_radius)?.unwrap_or(0.0),
            },
            ToolShapeInput::Boring { nose_radius } => ToolKind::Boring {
                nose_radius: opt_length(nose_radius)?.unwrap_or(0.0),
            },
            ToolShapeInput::Grooving {
                width,
                nose_radius,
                inside,
            } => ToolKind::Grooving {
                width: length(width)?,
                nose_radius: opt_length(nose_radius)?.unwrap_or(0.0),
                inside: *inside,
            },
            ToolShapeInput::Parting { width } => ToolKind::Parting {
                width: length(width)?,
            },
            ToolShapeInput::Threading { angle: a, inside } => ToolKind::Threading {
                angle: angle(a, sixty())?,
                inside: *inside,
            },
        };
        let (shape, insert) = self.resolve_insert(params, shape)?;
        let diameter = length(&self.diameter)?;
        // A lathe tool's flute length is the deepest it cuts.
        let flute_default = match shape {
            ToolKind::Turning { .. } | ToolKind::Boring { .. } | ToolKind::Threading { .. } => 4.0,
            ToolKind::Grooving { .. } => 5.0,
            ToolKind::Parting { .. } => 25.0,
            _ => diameter * 3.0,
        };
        let flute_length = opt_length(&self.flute_length)?.unwrap_or(flute_default);
        let tool_length = opt_length(&self.length)?.unwrap_or(flute_length + 5.0);
        let cutting = match &self.cutting {
            Some(c) => Some(Cutting {
                spindle_rpm: c.spindle_rpm.evaluate(params, ValueKind::Number)?,
                feed: length(&c.feed)?,
                plunge_feed: length(&c.plunge_feed)?,
            }),
            None => None,
        };
        let mut tool = CamTool {
            number: self.number,
            name: self
                .name
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_owned(),
            shape,
            diameter,
            shank_diameter: opt_length(&self.shank_diameter)?,
            flute_length,
            length: tool_length,
            flutes: self.flutes.unwrap_or(2),
            holder: self
                .holder
                .as_deref()
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .map(str::to_owned),
            cutting,
            mount: self.mount,
            insert,
        };
        if tool.name.is_empty() {
            tool.name = tool.default_name();
        }
        tool.check()?;
        Ok(tool)
    }

    /// A turning tool's or boring bar's insert and holder (mitcad#2502),
    /// with the shape whose nose radius is the insert code's corner radius
    /// when none is typed. A typed nose radius stays whatever the code.
    fn resolve_insert(
        &self,
        params: &Parameters,
        shape: ToolKind,
    ) -> Result<(ToolKind, Option<engine::turning::InsertGeometry>), String> {
        use engine::turning::{InsertGeometry, TypedInsert};
        let Some(input) = self.insert.as_ref().filter(|i| !i.is_empty()) else {
            return Ok((shape, None));
        };
        let (typed_radius, boring) = match &self.shape {
            ToolShapeInput::Turning { nose_radius } => (nose_radius, false),
            ToolShapeInput::Boring { nose_radius } => (nose_radius, true),
            _ => {
                return Err(
                    "Only turning tools and boring bars take an insert and a holder.".to_owned(),
                );
            }
        };
        let angle = |v: &Option<CamValue>| {
            v.as_ref()
                .map(|v| v.evaluate(params, ValueKind::Angle))
                .transpose()
        };
        let nose_radius = typed_radius
            .as_ref()
            .map(|v| v.evaluate(params, ValueKind::Length))
            .transpose()?;
        let typed = TypedInsert {
            tip_angle: angle(&input.tip_angle)?,
            approach_angle: angle(&input.approach_angle)?,
            hand: input.hand,
            nose_radius,
        };
        let read = InsertGeometry::read(
            input.insert.as_deref().unwrap_or_default(),
            input.holder.as_deref().unwrap_or_default(),
            typed,
        )?;
        if let Some(holder) = &read.holder
            && holder.bar
            && !boring
        {
            return Err(format!(
                "{} is a boring bar's code: a turning tool needs a holder's code, like PCLNR 2525 M12.",
                holder.text
            ));
        }
        // The insert code's corner radius when none is typed.
        let shape = match (nose_radius, read.code_nose_radius()) {
            (None, Some(r)) if boring => ToolKind::Boring { nose_radius: r },
            (None, Some(r)) => ToolKind::Turning { nose_radius: r },
            _ => shape,
        };
        Ok((shape, Some(read.geometry)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An operation with every value it can have, each `gap + 1`.
    fn every_value() -> CamOperation {
        let v = "gap + 1";
        serde_json::from_value(json!({
            "id": 2, "name": "Raster", "tool": 1,
            "kind": {"type": "raster_finish", "angle": v, "slope": v},
            "depths": {"top": v, "bottom": v, "step_down": v},
            "heights": {"clearance": v, "retract": v},
            "cutting": {"spindle_rpm": v, "feed": v, "plunge_feed": v},
            "stepover": v, "stock_to_leave": v,
            "entry": {"kind": "helix", "radius": v, "angle": v},
            "tabs": {"count": 2, "width": v, "height": v},
            "lead": {"radius": v}
        }))
        .unwrap()
    }

    #[test]
    fn values_mut_lists_what_values_lists() {
        let mut op = every_value();
        let kinds: Vec<ValueKind> = op.values().into_iter().map(|(_, k)| k).collect();
        let kinds_mut: Vec<ValueKind> = op.values_mut().into_iter().map(|(_, k)| k).collect();
        assert_eq!(kinds.len(), 17);
        assert_eq!(kinds, kinds_mut);
        let mut setup: CamSetup = serde_json::from_value(json!({
            "id": 1, "name": "Setup 1", "safe_height": "gap + 1",
            "stock": {"kind": "around_bodies", "side": "gap", "top": "gap", "bottom": "gap"}
        }))
        .unwrap();
        let kinds: Vec<ValueKind> = setup.values().into_iter().map(|(_, k)| k).collect();
        let kinds_mut: Vec<ValueKind> = setup.values_mut().into_iter().map(|(_, k)| k).collect();
        assert_eq!(kinds.len(), 4);
        assert_eq!(kinds, kinds_mut);
    }

    #[test]
    fn a_rename_rewrites_every_value_and_the_users_are_named() {
        let mut setup: CamSetup = serde_json::from_value(json!({
            "id": 1, "name": "Setup 1", "safe_height": 15,
            "stock": {"kind": "around_bodies", "top": "gaps * 2"}
        }))
        .unwrap();
        setup.operations.push(every_value());
        let mut cam = CamState {
            setups: vec![setup],
            ..CamState::default()
        };
        assert_eq!(cam.parameter_names(), ["gaps", "gap"]);
        assert_eq!(
            cam.parameter_users("gap"),
            ["the CAM operation Raster of Setup 1"]
        );
        assert_eq!(cam.parameter_users("gaps"), ["the CAM setup Setup 1"]);
        assert!(cam.parameter_users("gip").is_empty());

        cam.rename_parameter("gap", "gip");
        let op = &cam.setups[0].operations[0];
        for (value, _) in op.values() {
            assert_eq!(value, &CamValue::Expression("gip + 1".to_owned()));
        }
        // Another parameter whose name starts the same stays.
        assert_eq!(
            cam.setups[0].values()[2].0,
            &CamValue::Expression("gaps * 2".to_owned())
        );
        assert_eq!(cam.setups[0].safe_height, CamValue::Number(15.0));
        assert!(cam.parameter_users("gap").is_empty());
    }
}
