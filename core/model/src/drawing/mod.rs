// SPDX-License-Identifier: MIT
//! Drawings (mitcad#471) as the model keeps them: the definitions a design
//! file stores and the options of the `export_drawing` command
//! (commands.md), below the drawing crate.
//!
//! The kernel projects the views ([`crate::Kernel::project_view`]) and
//! finds the round holes ([`crate::Kernel::round_holes`]); the drawing crate
//! (`mitcad-model-drawing`, mitcad#777) lays them out on the sheet and
//! writes the files without any geometry library. Lengths on the sheet are
//! millimetres of paper; lengths of the model are millimetres too, shown in
//! the drawing's unit.
//!
//! Drawings kept in the design (mitcad#707) are definitions only
//! ([`def`]): sheets and their views, each placed on its sheet with its
//! scale, style and subject, their annotations ([`annotate_def`]), tables
//! ([`tables_def`]), template and revisions ([`template_def`]). Their lines
//! are projected again from the model whenever they are asked for
//! (`model-api/src/drawings.rs`).

pub mod def;
// The stored parts of annotations, tables and templates (mitcad#777).
pub mod annotate_def;
pub mod tables_def;
pub mod template_def;
// Cut sheets: flat parts at 1:1 for cutting (mitcad#853).
pub mod cut_sheet_def;

use serde::{Deserialize, Serialize};

use crate::datum::Vec3;

// What the kernel works out: the projected views and round holes, below
// the model (mitcad#777).
pub use mitcad_model_base::drawing_view::{
    CutFace, CutPlane, P2, RoundHole, ViewCurve, ViewEdge, ViewEdgeKind, ViewProjection, ViewSpec,
    ViewSpline,
};

// --- The command's options --------------------------------------------------

/// The sheet sizes: ISO 216 A4 to A0 and ANSI A to E (`ANSI_A`, …),
/// width and height in millimetres in landscape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SheetSize {
    #[default]
    A4,
    A3,
    A2,
    A1,
    A0,
    #[serde(rename = "ANSI_A")]
    AnsiA,
    #[serde(rename = "ANSI_B")]
    AnsiB,
    #[serde(rename = "ANSI_C")]
    AnsiC,
    #[serde(rename = "ANSI_D")]
    AnsiD,
    #[serde(rename = "ANSI_E")]
    AnsiE,
}

impl SheetSize {
    pub const ALL: [Self; 10] = [
        Self::A4,
        Self::A3,
        Self::A2,
        Self::A1,
        Self::A0,
        Self::AnsiA,
        Self::AnsiB,
        Self::AnsiC,
        Self::AnsiD,
        Self::AnsiE,
    ];

    /// The name in the command and in the title block.
    pub fn name(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::A2 => "A2",
            Self::A1 => "A1",
            Self::A0 => "A0",
            Self::AnsiA => "ANSI_A",
            Self::AnsiB => "ANSI_B",
            Self::AnsiC => "ANSI_C",
            Self::AnsiD => "ANSI_D",
            Self::AnsiE => "ANSI_E",
        }
    }

    /// Width and height in landscape, mm.
    pub fn landscape(self) -> (f64, f64) {
        const INCH: f64 = 25.4;
        match self {
            Self::A4 => (297.0, 210.0),
            Self::A3 => (420.0, 297.0),
            Self::A2 => (594.0, 420.0),
            Self::A1 => (841.0, 594.0),
            Self::A0 => (1189.0, 841.0),
            Self::AnsiA => (11.0 * INCH, 8.5 * INCH),
            Self::AnsiB => (17.0 * INCH, 11.0 * INCH),
            Self::AnsiC => (22.0 * INCH, 17.0 * INCH),
            Self::AnsiD => (34.0 * INCH, 22.0 * INCH),
            Self::AnsiE => (44.0 * INCH, 34.0 * INCH),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Landscape,
    Portrait,
}

/// ISO 5456-2: third angle (the top view above the front view, the right
/// view on its right) or first angle (the top view below, the right view on
/// the left).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    #[default]
    ThirdAngle,
    FirstAngle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewName {
    Front,
    Top,
    Right,
    Iso,
}

impl ViewName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Top => "top",
            Self::Right => "right",
            Self::Iso => "iso",
        }
    }

    /// The view's direction to the eye and x axis, Z up: the front from -Y,
    /// the top from +Z, the right from +X, the isometric view from the
    /// front, right and above.
    pub fn axes(self) -> (Vec3, Vec3) {
        let s3 = 1.0 / 3f64.sqrt();
        let s2 = 1.0 / 2f64.sqrt();
        match self {
            Self::Front => ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
            Self::Top => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            Self::Right => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            Self::Iso => ([s3, -s3, s3], [s2, s2, 0.0]),
        }
    }
}

/// The scale: chosen to fit the sheet, or given as `"1:2"`, `"2:1"` or a
/// number (sheet millimetres per model millimetre).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ScaleChoice {
    #[default]
    Fit,
    Fixed(f64),
}

impl<'de> Deserialize<'de> for ScaleChoice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = serde_json::Value::deserialize(deserializer)?;
        let scale = match &value {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) if s == "fit" => return Ok(Self::Fit),
            serde_json::Value::String(s) => parse_scale(s),
            _ => None,
        };
        match scale {
            Some(s) if s.is_finite() && s > 0.0 => Ok(Self::Fixed(s)),
            _ => Err(D::Error::custom(format!(
                "a scale is \"fit\", \"1:2\", \"2:1\" or a positive number, not {value}"
            ))),
        }
    }
}

/// `"1:2"` as 0.5, `"2:1"` as 2.
pub fn parse_scale(text: &str) -> Option<f64> {
    let (a, b) = text.split_once(':')?;
    let a: f64 = a.trim().parse().ok()?;
    let b: f64 = b.trim().parse().ok()?;
    (a > 0.0 && b > 0.0).then_some(a / b)
}

/// `1:2` for 0.5, `2:1` for 2, `1:2.5` for 0.4.
pub fn scale_text(scale: f64) -> String {
    if scale >= 1.0 {
        format!("{}:1", trim_number(scale, 3))
    } else {
        format!("1:{}", trim_number(1.0 / scale, 3))
    }
}

/// The section view's plane: a Section Analysis of the document by name, or
/// a plane by a point and its normal.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum SectionChoice {
    Analysis { analysis: String },
    Plane { origin: Vec3, normal: Vec3 },
}

fn default_views() -> Vec<ViewName> {
    vec![ViewName::Front, ViewName::Top, ViewName::Right]
}

fn yes() -> bool {
    true
}

/// The choices of a drawing, as the `export_drawing` command takes them
/// (commands.md); the application keeps them in its settings.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingOptions {
    #[serde(default)]
    pub sheet: SheetSize,
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub scale: ScaleChoice,
    #[serde(default)]
    pub projection: Projection,
    #[serde(default = "default_views")]
    pub views: Vec<ViewName>,
    #[serde(default)]
    pub section: Option<SectionChoice>,
    /// Hidden edges dashed.
    #[serde(default)]
    pub hidden_lines: bool,
    /// Edges between faces that meet tangentially, thin.
    #[serde(default)]
    pub tangent_edges: bool,
    /// The overall dimensions of the views and the diameters of round holes.
    #[serde(default = "yes")]
    pub dimensions: bool,
    /// The dimensions of sketches whose plane is parallel to a view.
    #[serde(default)]
    pub sketch_dimensions: bool,
    #[serde(default = "yes")]
    pub hole_table: bool,
    /// The unit of the values (`mm`, `in`, …); the design's when left out.
    #[serde(default)]
    pub units: Option<String>,
    /// The design's name in the title block.
    #[serde(default)]
    pub title: Option<String>,
    /// The date in the title block (today's, YYYY-MM-DD, when left out).
    #[serde(default)]
    pub date: Option<String>,
    /// One sheet per body.
    #[serde(default)]
    pub per_body: bool,
    /// Exact edges (false: from a triangulation, quicker on large bodies).
    /// Bodies with free-form faces are drawn from a fine triangulation
    /// either way, its edges as lines and arcs where they follow them
    /// (`ViewSpec::quick`, mitcad#962).
    #[serde(default = "yes")]
    pub exact: bool,
}

impl Default for DrawingOptions {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({})).expect("the defaults")
    }
}

/// The unit the values are shown in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Units {
    pub symbol: &'static str,
    /// Millimetres per unit.
    pub mm: f64,
}

impl Units {
    pub const MM: Self = Self {
        symbol: "mm",
        mm: 1.0,
    };

    pub fn of(unit: crate::expr::LengthUnit) -> Self {
        Self {
            symbol: unit.symbol(),
            mm: unit.millimetres(),
        }
    }

    pub fn by_symbol(symbol: &str) -> Option<Self> {
        crate::expr::LengthUnit::ALL
            .into_iter()
            .find(|u| u.symbol() == symbol)
            .map(Self::of)
    }

    /// A length (mm) in this unit as the drawing writes it: at most two
    /// decimals in millimetres, three in larger units, without trailing
    /// zeros.
    pub fn length(&self, mm: f64) -> String {
        let decimals = if self.mm < 5.0 { 2 } else { 3 };
        trim_number(mm / self.mm, decimals)
    }
}

/// A number with at most `decimals` decimals, without trailing zeros.
pub fn trim_number(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    };
    if text == "-0" { "0".to_owned() } else { text }
}

// --- Small vector helpers ---------------------------------------------------

// The vector helpers live in `mitcad-vec3`; `unit` here divides by the length.
pub(crate) use mitcad_vec3::unit_div as unit;
pub(crate) use mitcad_vec3::{cross, dot};

/// The view of a section plane: the eye on its normal side, x horizontal
/// (along Z × normal) unless the plane is horizontal (then +X).
pub fn section_axes(normal: Vec3) -> (Vec3, Vec3) {
    let x = unit(cross([0.0, 0.0, 1.0], normal)).unwrap_or([1.0, 0.0, 0.0]);
    (normal, x)
}

/// Today's date as YYYY-MM-DD (UTC).
pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The proleptic Gregorian date of a count of days since 1970-01-01.
#[doc(hidden)]
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}
