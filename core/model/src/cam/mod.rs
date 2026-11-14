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
        }
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
        }
        if let Some(cutting) = &self.cutting {
            cutting.validate().map_err(|e| format!("Tool {n}: {e}"))?;
        }
        Ok(())
    }

    /// The tool as the engine computes with it.
    pub fn engine(&self) -> Result<engine::Tool, String> {
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
    pub fn check(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(
                "A machine's id is 1 to 64 letters, digits, underscores or hyphens.".to_owned(),
            );
        }
        let m = &self.machine;
        if m.name.trim().is_empty() || m.name.chars().count() > MAX_NAME {
            return Err("A machine needs a name of at most 120 characters.".to_owned());
        }
        if engine::dialect(&m.controller).is_none() {
            return Err(format!(
                "The machine {} has a controller MitCad does not write for.",
                m.name
            ));
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
}

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
        }
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
        out
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
            machine.check()?;
            if self.machines[..i].iter().any(|m| m.id == machine.id) {
                return Err(format!("the machine {} is listed twice", machine.id));
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
                return Err(format!("the id {} is used twice", setup.id));
            }
            for op in &setup.operations {
                check_operation(op)?;
                if !ids.insert(op.id) {
                    return Err(format!("the id {} is used twice", op.id));
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

/// What is wrong with a setup by itself.
pub fn check_setup(setup: &CamSetup) -> Result<(), String> {
    check_name(&setup.name, "setup")?;
    if !(1..=6).contains(&setup.work_offset) {
        return Err("The work offset must be one of G54 to G59.".to_owned());
    }
    if let Some(dialect) = &setup.dialect
        && engine::dialect(dialect).is_none()
    {
        return Err("MitCad does not write G-code for that controller.".to_owned());
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
    Ok(())
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
                    crate::ids::EntityUid::parse_curve(curve)
                        .map_err(|_| "A sketch curve is named like c3.".to_owned())?;
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
        };
        let diameter = length(&self.diameter)?;
        let flute_length = opt_length(&self.flute_length)?.unwrap_or(diameter * 3.0);
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
        };
        if tool.name.is_empty() {
            tool.name = tool.default_name();
        }
        tool.check()?;
        Ok(tool)
    }
}
