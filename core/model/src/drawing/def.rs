// SPDX-License-Identifier: MIT
//! Drawings kept in the design (mitcad#707): what a drawing stores. A
//! drawing lives in the Drawings folder of the root or of a component
//! (`component`) and holds sheets; a sheet holds views, each placed on it
//! with its scale, style and label. Only the definition is kept: the lines
//! are projected from the model whenever the drawing is asked for, so the
//! drawing follows the model.
//!
//! Lengths of the model are millimetres in the coordinates of the
//! drawing's component (the design's for the root); a view's coordinates
//! are those of [`crate::drawing::ViewSpec`]: the plane through the
//! component's origin at right angles to the view's direction. Lengths on
//! the sheet are millimetres of paper from its lower left corner.
//!
//! The kinds of view:
//! - a **base view** looks along a standard direction (front, top, ...,
//!   an isometric one) or a given one, at the drawing's subject: the whole
//!   design (or component), one occurrence, or chosen bodies;
//! - a **projected view** looks at its parent's subject from one side of
//!   the parent (where it sits on the sheet, after the sheet's projection
//!   method), or from a corner (an isometric view);
//! - a **section view** cuts its parent's subject with a plane through a
//!   line drawn on the parent, seen along the plane's normal;
//! - a **detail view** shows a circle of its parent at a larger scale.
//!
//! Any view can be shortened by breaks: bands of its plane taken out and
//! the rest moved together.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::annotate_def::{SheetNote, SketchItem, ViewAnnotation};
use super::template_def::{Revision, SheetTemplate};
use super::{Orientation, P2, Projection, SheetSize, ViewSpec, cross, dot, section_axes, unit};
use crate::datum::Vec3;
use crate::features::OccurrencePath;
use crate::ids::{BodyUid, ComponentUid};

/// A drawing, written `D<n>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DrawingUid(pub u32);

impl fmt::Display for DrawingUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "D{}", self.0)
    }
}

impl FromStr for DrawingUid {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        text.trim()
            .strip_prefix('D')
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            .and_then(|n| n.parse().ok())
            .map(Self)
            .ok_or_else(|| format!("'{text}' is no drawing id like \"D1\""))
    }
}

impl Serialize for DrawingUid {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for DrawingUid {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn is_root(component: &ComponentUid) -> bool {
    component.is_root()
}

/// A drawing kept in the design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drawing {
    pub uid: DrawingUid,
    /// Unique among the drawings of its component (`Drawing1`).
    pub name: String,
    /// The component whose Drawings folder holds it; its views are of that
    /// component, in its coordinates. The root's (`C0`) is left out.
    #[serde(default, skip_serializing_if = "is_root")]
    pub component: ComponentUid,
    pub sheets: Vec<DrawingSheet>,
    /// Its revision table's rows, the latest last (mitcad#707 phase 4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<Revision>,
    /// When File › Export Drawing laid it out last, as a count over the
    /// design's drawings (1, 2, …; 0: never), so that the dialog offers the
    /// one it laid out last again (mitcad#725).
    #[serde(default, skip_serializing_if = "is_default")]
    pub exported: u32,
    /// The decimals and the letters' height of its dimensions and
    /// annotations without their own (mitcad#729).
    #[serde(default, skip_serializing_if = "is_default")]
    pub defaults: DrawingDefaults,
    /// A cut sheet's material and parts (mitcad#853): the drawing is then
    /// a cut sheet, of one sheet without views, notes, a template or
    /// revisions of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_sheet: Option<super::cut_sheet_def::CutSheet>,
}

/// What every dimension and annotation of a drawing takes unless it has
/// its own (mitcad#729): its value's decimals (a dimension's `format`),
/// its letters' height (an annotation's, a note's or a text's `height`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingDefaults {
    /// The decimals of dimensions' values; as many as they need, up to
    /// three, when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    /// The letters' height, paper mm; 3.5 when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_height: Option<f64>,
}

impl DrawingDefaults {
    /// The letters' height of an item whose own is `own`.
    pub fn text(&self, own: Option<f64>) -> f64 {
        own.or(self.text_height)
            .unwrap_or(super::annotate_def::DIMENSION_TEXT)
    }

    fn check(&self) -> Result<(), String> {
        if let Some(decimals) = self.decimals
            && decimals > crate::document::MAX_DECIMALS
        {
            return Err(format!(
                "a dimension shows at most {} decimals",
                crate::document::MAX_DECIMALS
            ));
        }
        if let Some(height) = self.text_height {
            super::annotate_def::check_height(height)?;
        }
        Ok(())
    }
}

/// One sheet of a drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingSheet {
    /// `Sheet1`, shown on its tab.
    pub name: String,
    #[serde(default)]
    pub size: SheetSize,
    #[serde(default, skip_serializing_if = "is_default")]
    pub orientation: Orientation,
    /// Where projected views go: third angle (a view seen from the right
    /// on the right) or first angle (on the left).
    #[serde(default, skip_serializing_if = "is_default")]
    pub projection: Projection,
    #[serde(default, skip_serializing_if = "TitleBlock::is_empty")]
    pub title_block: TitleBlock,
    /// Its frame, title block and revision table: a copy of the template
    /// chosen (mitcad#707 phase 4); the simple title block when left out.
    /// The title block's fields above fill the template's `{title}`,
    /// `{number}`, `{author}` and `{date}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<SheetTemplate>,
    /// The length unit of its values (`mm`, `in`, ...); the design's when
    /// left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// The automatic annotations of File › Export Drawing.
    #[serde(default, skip_serializing_if = "Annotations::is_none")]
    pub annotations: Annotations,
    /// In the order they were made: a view's parent comes before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<DrawingView>,
    /// Text notes on the sheet, without a leader (mitcad#707 phase 2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<SheetNote>,
    /// Lines and arcs drawn on the sheet, outside the model (mitcad#729);
    /// sheet millimetres.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sketch: Vec<SketchItem>,
}

/// The fields of a sheet's title block. Left out: the title is the
/// design's or the component's name, the date today's; without a number
/// the block names the bodies drawn, without an author where the drawing
/// was made.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleBlock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The drawing number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// YYYY-MM-DD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

impl TitleBlock {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The annotations a sheet works out by itself (File › Export Drawing's
/// choices): the overall sizes of the orthographic views and the
/// diameters of round holes, the hole table, the dimensions of sketches
/// seen square on. Dimensions placed by hand come with the annotation
/// engine (mitcad#702).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotations {
    #[serde(default, skip_serializing_if = "is_false")]
    pub dimensions: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hole_table: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub sketch_dimensions: bool,
}

impl Annotations {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// Those of an export's options.
    pub fn of(options: &super::DrawingOptions) -> Self {
        Self {
            dimensions: options.dimensions,
            hole_table: options.hole_table,
            sketch_dimensions: options.sketch_dimensions,
        }
    }
}

/// One view on a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingView {
    /// Unique in the drawing, from 1.
    pub id: u32,
    pub kind: ViewKind,
    /// Where the view's anchor lies on the sheet (mm): the component's
    /// origin as the view sees it, a detail's centre.
    pub place: P2,
    /// Sheet millimetres per model millimetre. Left out, a projected or
    /// section view takes its parent's, a detail view twice its parent's,
    /// a base view 1:1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Left out, a view other than a base view takes its parent's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ViewStyle>,
    /// Written under the view; a section's (`SECTION A-A`) and a detail's
    /// (`DETAIL B (2:1)`) when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What a base view (or a section without a parent) shows; the whole
    /// design (or component) when left out. Other views show their parent's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Subject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<ViewBreak>,
    /// A projected or section view stays in line with its parent when
    /// either moves.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub aligned: bool,
    /// Centre marks on the circles it sees end on and centrelines on the
    /// round faces it sees from the side, worked out from the model
    /// (mitcad#707 phase 2; on for views added by hand).
    #[serde(default, skip_serializing_if = "is_false")]
    pub center_marks: bool,
    /// The dimensions of the sketches it sees square on, retrieved from
    /// the model.
    #[serde(default, skip_serializing_if = "is_false")]
    pub retrieved_dimensions: bool,
    /// Its dimensions, centre marks, notes and symbols, in the order they
    /// were added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<ViewAnnotation>,
    /// How far it is turned on the sheet in its own plane, degrees
    /// counter-clockwise (mitcad#729): its x axis turned the other way
    /// about the direction it looks along, so that the views from it,
    /// its annotations and what is drawn in it turn with it. A detail
    /// turns with its parent.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    /// Lines, arcs and texts drawn in it, outside the model (mitcad#729),
    /// in its coordinates: they move, scale and turn with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sketch: Vec<SketchItem>,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// What a view is and where it looks from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ViewKind {
    Base {
        direction: ViewDirection,
    },
    Projected {
        parent: u32,
        /// Where it sits beside its parent.
        side: Side,
    },
    /// What lies on the normal side of the plane is taken away; the view
    /// looks at the cut along -normal.
    Section {
        /// The view the section line is drawn on; None for a section of a
        /// plane no view sees edge on (an export's).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<u32>,
        origin: Vec3,
        /// A unit vector.
        normal: Vec3,
        /// Where the section line runs on its parent: from and to along
        /// the line, in the parent's view millimetres from the origin's
        /// projection; across the parent when left out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<[f64; 2]>,
        letter: String,
        /// The cut faces hatched.
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        hatch: bool,
    },
    Detail {
        parent: u32,
        /// In the parent's view coordinates.
        center: P2,
        /// Model millimetres.
        radius: f64,
        letter: String,
    },
}

impl ViewKind {
    pub fn parent(&self) -> Option<u32> {
        match self {
            Self::Base { .. } => None,
            Self::Projected { parent, .. } | Self::Detail { parent, .. } => Some(*parent),
            Self::Section { parent, .. } => *parent,
        }
    }

    /// Its name in commands and files.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Base { .. } => "base",
            Self::Projected { .. } => "projected",
            Self::Section { .. } => "section",
            Self::Detail { .. } => "detail",
        }
    }

    pub fn letter(&self) -> Option<&str> {
        match self {
            Self::Section { letter, .. } | Self::Detail { letter, .. } => Some(letter),
            _ => None,
        }
    }
}

/// A base view's direction: a standard one (Z up: the front seen from -Y,
/// the top from +Z, the right from +X; the isometric ones from a corner
/// above or below), or a given one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewDirection {
    Named(StandardView),
    Given { toward_eye: Vec3, x_axis: Vec3 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardView {
    Front,
    Back,
    Top,
    Bottom,
    Left,
    Right,
    /// From the front, right and above.
    Iso,
    IsoTopLeft,
    IsoBottomRight,
    IsoBottomLeft,
}

impl StandardView {
    pub const ALL: [Self; 10] = [
        Self::Front,
        Self::Back,
        Self::Top,
        Self::Bottom,
        Self::Left,
        Self::Right,
        Self::Iso,
        Self::IsoTopLeft,
        Self::IsoBottomRight,
        Self::IsoBottomLeft,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
            Self::Iso => "iso",
            Self::IsoTopLeft => "iso_top_left",
            Self::IsoBottomRight => "iso_bottom_right",
            Self::IsoBottomLeft => "iso_bottom_left",
        }
    }

    pub fn is_isometric(self) -> bool {
        matches!(
            self,
            Self::Iso | Self::IsoTopLeft | Self::IsoBottomRight | Self::IsoBottomLeft
        )
    }

    /// The direction to the eye and the view's x axis.
    pub fn axes(self) -> (Vec3, Vec3) {
        let corner = |x: f64, y: f64, z: f64| {
            let s = 1.0 / 3f64.sqrt();
            let eye = [x * s, y * s, z * s];
            let across = unit(cross([0.0, 0.0, 1.0], eye)).unwrap_or([1.0, 0.0, 0.0]);
            (eye, across)
        };
        match self {
            Self::Front => ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
            Self::Back => ([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]),
            Self::Top => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            Self::Bottom => ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
            Self::Left => ([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),
            Self::Right => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            Self::Iso => {
                let (s3, s2) = (1.0 / 3f64.sqrt(), 1.0 / 2f64.sqrt());
                // As the export's isometric view (`ViewName::Iso`).
                ([s3, -s3, s3], [s2, s2, 0.0])
            }
            Self::IsoTopLeft => corner(-1.0, -1.0, 1.0),
            Self::IsoBottomRight => corner(1.0, -1.0, -1.0),
            Self::IsoBottomLeft => corner(-1.0, -1.0, -1.0),
        }
    }

    /// The standard view with these axes, if one has them.
    pub fn of_axes(toward_eye: Vec3, x_axis: Vec3) -> Option<Self> {
        let near = |a: Vec3, b: Vec3| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
        Self::ALL.into_iter().find(|v| {
            let (e, x) = v.axes();
            near(e, toward_eye) && near(x, x_axis)
        })
    }

    /// Whether a view looks along an isometric view's direction, however
    /// it is turned on the sheet (mitcad#729).
    pub fn isometric_eye(toward_eye: Vec3) -> bool {
        Self::ALL.into_iter().any(|v| {
            let (e, _) = v.axes();
            v.is_isometric() && (0..3).all(|i| (e[i] - toward_eye[i]).abs() < 1e-9)
        })
    }
}

/// A view's axes turned on the sheet by `degrees` counter-clockwise
/// (mitcad#729): what it shows turns that way, so its x axis turns the
/// other way about the direction to the eye.
pub fn rotated_axes(axes: (Vec3, Vec3), degrees: f64) -> (Vec3, Vec3) {
    if degrees == 0.0 {
        return axes;
    }
    let (e, x) = axes;
    let y = cross(e, x);
    let (s, c) = degrees.to_radians().sin_cos();
    (e, std::array::from_fn(|i| c * x[i] - s * y[i]))
}

impl ViewDirection {
    pub fn axes(&self) -> (Vec3, Vec3) {
        match self {
            Self::Named(view) => view.axes(),
            Self::Given { toward_eye, x_axis } => (*toward_eye, *x_axis),
        }
    }

    /// A direction from the eye's side and the view's up direction (the
    /// application's camera), made orthonormal.
    pub fn looking(toward_eye: Vec3, up: Vec3) -> Option<Self> {
        let eye = unit(toward_eye)?;
        let x_axis = unit(cross(up, eye))?;
        Some(Self::Given {
            toward_eye: eye,
            x_axis,
        })
    }

    fn check(&self) -> Result<(), String> {
        let (e, x) = self.axes();
        let length = |v: Vec3| dot(v, v).sqrt();
        if !(e.iter().chain(&x).all(|v| v.is_finite())
            && (length(e) - 1.0).abs() < 1e-6
            && (length(x) - 1.0).abs() < 1e-6
            && dot(e, x).abs() < 1e-6)
        {
            return Err(
                "a view's direction and x axis are unit vectors at right angles".to_owned(),
            );
        }
        Ok(())
    }
}

impl Serialize for ViewDirection {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Given {
            toward_eye: Vec3,
            x_axis: Vec3,
        }
        match self {
            Self::Named(view) => view.serialize(s),
            Self::Given { toward_eye, x_axis } => Given {
                toward_eye: *toward_eye,
                x_axis: *x_axis,
            }
            .serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for ViewDirection {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Given {
            toward_eye: Vec3,
            x_axis: Vec3,
        }
        let value = serde_json::Value::deserialize(d)?;
        if value.is_string() {
            return serde_json::from_value::<StandardView>(value)
                .map(Self::Named)
                .map_err(serde::de::Error::custom);
        }
        let given: Given = serde_json::from_value(value).map_err(|e| {
            serde::de::Error::custom(format!(
                "a direction is a standard view's name or {{\"toward_eye\", \"x_axis\"}}: {e}"
            ))
        })?;
        Ok(Self::Given {
            toward_eye: given.toward_eye,
            x_axis: given.x_axis,
        })
    }
}

/// Where a projected view sits beside its parent on the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Right,
    Left,
    Above,
    Below,
    AboveRight,
    AboveLeft,
    BelowRight,
    BelowLeft,
}

impl Side {
    pub const ALL: [Self; 8] = [
        Self::Right,
        Self::Left,
        Self::Above,
        Self::Below,
        Self::AboveRight,
        Self::AboveLeft,
        Self::BelowRight,
        Self::BelowLeft,
    ];

    /// The unit steps on the sheet: (right, up), each -1, 0 or 1.
    pub fn steps(self) -> (f64, f64) {
        match self {
            Self::Right => (1.0, 0.0),
            Self::Left => (-1.0, 0.0),
            Self::Above => (0.0, 1.0),
            Self::Below => (0.0, -1.0),
            Self::AboveRight => (1.0, 1.0),
            Self::AboveLeft => (-1.0, 1.0),
            Self::BelowRight => (1.0, -1.0),
            Self::BelowLeft => (-1.0, -1.0),
        }
    }

    /// The side a point lies on, seen from a view's centre: one of eight
    /// sectors of 45 degrees.
    pub fn towards(from: P2, to: P2) -> Self {
        let angle = (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees();
        let sector = ((angle + 360.0 + 22.5) / 45.0).floor() as i64 % 8;
        match sector {
            0 => Self::Right,
            1 => Self::AboveRight,
            2 => Self::Above,
            3 => Self::AboveLeft,
            4 => Self::Left,
            5 => Self::BelowLeft,
            6 => Self::Below,
            _ => Self::BelowRight,
        }
    }

    pub fn is_corner(self) -> bool {
        let (x, y) = self.steps();
        x != 0.0 && y != 0.0
    }

    /// The axes of the view placed on this side of a parent with the given
    /// axes. Beside or above or below, an orthographic view seen from that
    /// side (third angle) or from the opposite one (first angle); at a
    /// corner, a view from that corner of the parent's (an isometric view
    /// of a standard one) whatever the projection method.
    pub fn axes(self, parent: (Vec3, Vec3), projection: Projection) -> (Vec3, Vec3) {
        let (e, x) = parent;
        let y = cross(e, x);
        let (sx, sy) = self.steps();
        let scale = |v: Vec3, k: f64| v.map(|c| c * k);
        let add = |a: Vec3, b: Vec3| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
        if self.is_corner() {
            let eye = unit(add(e, add(scale(x, sx), scale(y, sy)))).unwrap_or(e);
            let across = unit(cross(y, eye)).unwrap_or(x);
            return (eye, across);
        }
        let sign = match projection {
            Projection::ThirdAngle => 1.0,
            Projection::FirstAngle => -1.0,
        };
        if sx != 0.0 {
            let eye = scale(x, sx * sign);
            (eye, cross(y, eye))
        } else {
            (scale(y, sy * sign), x)
        }
    }

    /// Where this side's view is kept in line with its parent: the sheet
    /// axis both share (0: the same x, 1: the same y), none at a corner.
    pub fn aligned_axis(self) -> Option<usize> {
        match self {
            Self::Right | Self::Left => Some(1),
            Self::Above | Self::Below => Some(0),
            _ => None,
        }
    }
}

/// How a view is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewStyle {
    /// Hidden edges, dashed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden_lines: bool,
    /// Edges between faces that meet tangentially, thin.
    #[serde(default, skip_serializing_if = "is_false")]
    pub tangent_edges: bool,
    /// The faces shaded under the lines.
    #[serde(default, skip_serializing_if = "is_false")]
    pub shaded: bool,
    /// Projected from a triangulation (quicker on large bodies) instead of
    /// the exact edges.
    #[serde(default, skip_serializing_if = "is_false")]
    pub polygonal: bool,
}

/// What a base view shows, in the drawing's component.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Subject {
    /// Every shown body of the component and its occurrences.
    #[default]
    Design,
    /// The shown bodies of one occurrence (a path from the drawing's
    /// component) and its occurrences.
    Occurrence { occurrence: OccurrencePath },
    /// Chosen bodies, each where an occurrence places it (a path from the
    /// drawing's component) or wherever it is shown when left out.
    Bodies { bodies: Vec<SubjectBody> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectBody {
    pub body: BodyUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
}

/// A band of a view taken out: from `from` to `to` (view millimetres)
/// across `direction`, the two sides moved together to `gap` millimetres
/// of paper apart.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewBreak {
    pub direction: BreakDirection,
    pub from: f64,
    pub to: f64,
    #[serde(default = "default_gap")]
    pub gap: f64,
}

/// The paper left between the two sides of a break, mm.
pub const BREAK_GAP: f64 = 6.0;

fn default_gap() -> f64 {
    BREAK_GAP
}

/// The view axis a break shortens: horizontal (its x, the break lines
/// upright) or vertical (its y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakDirection {
    Horizontal,
    Vertical,
}

impl BreakDirection {
    pub fn axis(self) -> usize {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
        }
    }
}

/// The letters of sections and details, in turn (I, O and Q left out,
/// as they read as digits).
pub const LETTERS: &str = "ABCDEFGHJKLMNPRSTUVWXYZ";

impl Drawing {
    /// The view of this id and its sheet.
    pub fn find_view(&self, id: u32) -> Option<(usize, &DrawingView)> {
        self.sheets
            .iter()
            .enumerate()
            .find_map(|(i, s)| s.view(id).map(|v| (i, v)))
    }

    /// The next free view id.
    pub fn next_view_id(&self) -> u32 {
        self.sheets
            .iter()
            .flat_map(|s| &s.views)
            .map(|v| v.id)
            .max()
            .unwrap_or(0)
            + 1
    }

    /// The first letter no section or detail of the drawing has.
    pub fn next_letter(&self) -> String {
        let used: Vec<&str> = self
            .sheets
            .iter()
            .flat_map(|s| &s.views)
            .filter_map(|v| v.kind.letter())
            .collect();
        for round in 1.. {
            for c in LETTERS.chars() {
                let letter = c.to_string().repeat(round);
                if !used.contains(&letter.as_str()) {
                    return letter;
                }
            }
        }
        unreachable!("the rounds go on")
    }

    /// The ids of its annotations, text notes and what is drawn outside
    /// the model (mitcad#729), sheet by sheet.
    fn annotation_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.sheets.iter().flat_map(|s| {
            s.views
                .iter()
                .flat_map(|v| {
                    v.annotations
                        .iter()
                        .map(|a| a.id)
                        .chain(v.sketch.iter().map(SketchItem::id))
                })
                .chain(s.notes.iter().map(|n| n.id))
                .chain(s.sketch.iter().map(SketchItem::id))
        })
    }

    /// Where a line, an arc or a text drawn outside the model is: its
    /// sheet, and the view it is drawn in (None: on the sheet).
    pub fn find_sketch(&self, id: u32) -> Option<(usize, Option<u32>)> {
        self.sheets.iter().enumerate().find_map(|(i, s)| {
            s.views
                .iter()
                .find(|v| v.sketch.iter().any(|k| k.id() == id))
                .map(|v| (i, Some(v.id)))
                .or_else(|| s.sketch.iter().any(|k| k.id() == id).then_some((i, None)))
        })
    }

    /// The next free annotation id.
    pub fn next_annotation_id(&self) -> u32 {
        self.annotation_ids().max().unwrap_or(0) + 1
    }

    /// Where an annotation or a text note is: its sheet, and the view it
    /// is on (None for a text note); a line, an arc or a text drawn
    /// outside the model too ([`Drawing::find_sketch`]).
    pub fn find_annotation(&self, id: u32) -> Option<(usize, Option<u32>)> {
        self.sheets
            .iter()
            .enumerate()
            .find_map(|(i, s)| {
                s.views
                    .iter()
                    .find(|v| v.annotations.iter().any(|a| a.id == id))
                    .map(|v| (i, Some(v.id)))
                    .or_else(|| s.notes.iter().any(|n| n.id == id).then_some((i, None)))
            })
            .or_else(|| self.find_sketch(id))
    }

    /// The datum letters of its datum labels.
    pub fn datum_letters(&self) -> Vec<String> {
        self.sheets
            .iter()
            .flat_map(|s| &s.views)
            .flat_map(|v| &v.annotations)
            .filter_map(|a| a.letter.clone())
            .collect()
    }

    /// The datum letters its feature control frames refer to.
    pub fn frame_datums(&self) -> Vec<String> {
        self.sheets
            .iter()
            .flat_map(|s| &s.views)
            .flat_map(|v| &v.annotations)
            .filter_map(|a| a.frame.as_ref())
            .flat_map(|f| f.datums.iter().cloned())
            .collect()
    }

    /// Every occurrence path it keeps, to change in place: its views'
    /// occurrence subjects, chosen bodies' occurrences and annotation
    /// picks' (balloons' included). Paths from the drawing's component.
    pub(crate) fn occurrence_paths_mut(&mut self) -> Vec<&mut OccurrencePath> {
        let mut out: Vec<&mut OccurrencePath> = Vec::new();
        for view in self.sheets.iter_mut().flat_map(|s| s.views.iter_mut()) {
            match &mut view.subject {
                Some(Subject::Occurrence { occurrence }) => out.push(occurrence),
                Some(Subject::Bodies { bodies }) => {
                    out.extend(bodies.iter_mut().filter_map(|b| b.occurrence.as_mut()));
                }
                Some(Subject::Design) | None => {}
            }
            out.extend(
                view.annotations
                    .iter_mut()
                    .flat_map(|a| a.picks.iter_mut())
                    .filter_map(|p| p.occurrence.as_mut()),
            );
        }
        out
    }

    /// Turns the datums its frames refer to, and with `labels` its datum
    /// labels' letters, after `renamed` (old letter to new); whether
    /// anything changed.
    pub(crate) fn reletter(
        &mut self,
        renamed: &std::collections::BTreeMap<String, String>,
        labels: bool,
    ) -> bool {
        let mut changed = false;
        let annotations = self
            .sheets
            .iter_mut()
            .flat_map(|s| s.views.iter_mut())
            .flat_map(|v| v.annotations.iter_mut());
        for annotation in annotations {
            if labels
                && let Some(letter) = &mut annotation.letter
                && let Some(new) = renamed.get(letter.as_str())
            {
                letter.clone_from(new);
                changed = true;
            }
            if let Some(frame) = &mut annotation.frame {
                for datum in &mut frame.datums {
                    if let Some(new) = renamed.get(datum.as_str()) {
                        datum.clone_from(new);
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    /// Why the drawing cannot be kept, if it cannot (not its geometry).
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a drawing needs a name".to_owned());
        }
        if self.sheets.is_empty() {
            return Err(format!("{}: a drawing needs a sheet", self.name));
        }
        self.defaults
            .check()
            .map_err(|e| format!("{}: {e}", self.name))?;
        // A cut sheet (mitcad#853) is its material and its parts: one
        // sheet, nothing drawn on it of its own.
        if let Some(cut) = &self.cut_sheet {
            cut.check().map_err(|e| format!("{}: {e}", self.name))?;
            let plain = self.sheets.iter().all(|s| {
                s.views.is_empty()
                    && s.notes.is_empty()
                    && s.sketch.is_empty()
                    && s.template.is_none()
            });
            if self.sheets.len() != 1 || !plain || !self.revisions.is_empty() {
                return Err(format!(
                    "{} is a cut sheet: it holds parts only, on one sheet (add_cut_sheet_parts)",
                    self.name
                ));
            }
        }
        let mut ids = Vec::new();
        for (i, sheet) in self.sheets.iter().enumerate() {
            sheet
                .check()
                .map_err(|e| format!("{} sheet {}: {e}", self.name, i + 1))?;
            for view in &sheet.views {
                if ids.contains(&view.id) {
                    return Err(format!(
                        "{}: view {} is listed more than once",
                        self.name, view.id
                    ));
                }
                ids.push(view.id);
            }
        }
        let mut annotations: Vec<u32> = Vec::new();
        for id in self.annotation_ids() {
            if id == 0 {
                return Err(format!("{}: annotation ids start from 1", self.name));
            }
            if annotations.contains(&id) {
                return Err(format!(
                    "{}: annotation {id} is listed more than once",
                    self.name
                ));
            }
            annotations.push(id);
        }
        let letters = self.datum_letters();
        for (i, letter) in letters.iter().enumerate() {
            if letters[..i].contains(letter) {
                return Err(format!("{}: datum {letter} is there twice", self.name));
            }
        }
        for (i, revision) in self.revisions.iter().enumerate() {
            revision
                .check()
                .map_err(|e| format!("{}: {e}", self.name))?;
            if self.revisions[..i]
                .iter()
                .any(|r| r.letter == revision.letter)
            {
                return Err(format!(
                    "{}: revision {} is there twice",
                    self.name, revision.letter
                ));
            }
        }
        Ok(())
    }
}

impl DrawingSheet {
    /// An empty sheet.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: SheetSize::default(),
            orientation: Orientation::default(),
            projection: Projection::default(),
            title_block: TitleBlock::default(),
            template: None,
            units: None,
            annotations: Annotations::default(),
            views: Vec::new(),
            notes: Vec::new(),
            sketch: Vec::new(),
        }
    }

    /// Width and height, mm.
    pub fn dimensions(&self) -> (f64, f64) {
        let (long, short) = self.size.landscape();
        match self.orientation {
            Orientation::Landscape => (long, short),
            Orientation::Portrait => (short, long),
        }
    }

    pub fn view(&self, id: u32) -> Option<&DrawingView> {
        self.views.iter().find(|v| v.id == id)
    }

    pub fn view_index(&self, id: u32) -> Option<usize> {
        self.views.iter().position(|v| v.id == id)
    }

    /// The view and its ancestors, the view first.
    fn lineage(&self, id: u32) -> Result<Vec<&DrawingView>, String> {
        let mut out: Vec<&DrawingView> = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next {
            let view = self
                .view(id)
                .ok_or_else(|| format!("there is no view {id} on the sheet"))?;
            if out.iter().any(|v| v.id == id) {
                return Err(format!("view {id} is its own parent"));
            }
            out.push(view);
            next = view.kind.parent();
        }
        Ok(out)
    }

    /// The direction to the eye and the x axis of a view, as it is turned
    /// on the sheet (its `rotation`, mitcad#729).
    pub fn axes(&self, id: u32) -> Result<(Vec3, Vec3), String> {
        let rotation = self.view(id).map_or(0.0, |v| v.rotation);
        self.own_axes(id).map(|axes| rotated_axes(axes, rotation))
    }

    /// A view's axes before its own rotation: those of its kind, after
    /// its parent's as turned (a view from a turned one turns with it).
    pub fn own_axes(&self, id: u32) -> Result<(Vec3, Vec3), String> {
        let view = self
            .view(id)
            .ok_or_else(|| format!("there is no view {id} on the sheet"))?;
        // A cycle cannot reach here: lineage refuses it first.
        self.lineage(id)?;
        Ok(match &view.kind {
            ViewKind::Base { direction } => direction.axes(),
            ViewKind::Projected { parent, side } => side.axes(self.axes(*parent)?, self.projection),
            ViewKind::Detail { parent, .. } => self.axes(*parent)?,
            ViewKind::Section { parent, normal, .. } => match parent {
                Some(parent) => section_view_axes(*normal, self.axes(*parent)?),
                None => section_axes(*normal),
            },
        })
    }

    /// A view's scale: its own, or after its parent's.
    pub fn scale(&self, id: u32) -> f64 {
        let Some(view) = self.view(id) else {
            return 1.0;
        };
        if let Some(scale) = view.scale {
            return scale;
        }
        match &view.kind {
            ViewKind::Base { .. } => 1.0,
            ViewKind::Detail { parent, .. } => 2.0 * self.scale(*parent),
            kind => kind.parent().map_or(1.0, |p| self.scale(p)),
        }
    }

    /// A view's style: its own, or its parent's.
    pub fn style(&self, id: u32) -> ViewStyle {
        match self.lineage(id) {
            Ok(views) => views.iter().find_map(|v| v.style).unwrap_or_default(),
            Err(_) => ViewStyle::default(),
        }
    }

    /// The subject of a view: its base view's.
    pub fn subject(&self, id: u32) -> Subject {
        match self.lineage(id) {
            Ok(views) => views
                .last()
                .and_then(|v| v.subject.clone())
                .unwrap_or_default(),
            Err(_) => Subject::default(),
        }
    }

    /// The point of a view's coordinates that lies at its place: the
    /// origin, or a detail's centre.
    pub fn anchor(&self, id: u32) -> P2 {
        match self.view(id).map(|v| &v.kind) {
            Some(ViewKind::Detail { center, .. }) => *center,
            _ => [0.0, 0.0],
        }
    }

    /// The views that come from this one (its children, theirs, ...), in
    /// sheet order.
    pub fn dependents(&self, id: u32) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for view in &self.views {
            let mut next = view.kind.parent();
            let mut steps = 0;
            while let Some(parent) = next {
                if parent == id {
                    out.push(view.id);
                    break;
                }
                steps += 1;
                if steps > self.views.len() {
                    break;
                }
                next = self.view(parent).and_then(|p| p.kind.parent());
            }
        }
        out
    }

    /// The sheet axis along which a view is kept in line with its parent
    /// (0: the same x, 1: the same y), if it is.
    pub fn aligned_axis(&self, id: u32) -> Option<usize> {
        let view = self.view(id)?;
        if !view.aligned {
            return None;
        }
        match &view.kind {
            // Turned on its own, it no longer shares an axis with its
            // parent (mitcad#729); one turned with its parent does.
            ViewKind::Projected { .. } if view.rotation != 0.0 => None,
            ViewKind::Projected { side, .. } => side.aligned_axis(),
            ViewKind::Section {
                parent: Some(parent),
                ..
            } => {
                let (pe, px) = self.axes(*parent).ok()?;
                let (_, x) = self.axes(id).ok()?;
                let py = cross(pe, px);
                let y = cross(self.axes(id).ok()?.0, x);
                let same = |a: Vec3, b: Vec3| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
                if same(x, px) {
                    Some(0)
                } else if same(y, py) {
                    Some(1)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Whether a view looks along a direction given as such: a base view
    /// whose `Given` direction is none of the standard views', or a detail
    /// of one (mitcad#722). Projected views, the presets and sections are
    /// not.
    pub fn along_a_given_direction(&self, id: u32) -> bool {
        let Ok(lineage) = self.lineage(id) else {
            return false;
        };
        lineage
            .into_iter()
            .find(|v| !matches!(v.kind, ViewKind::Detail { .. }))
            .is_some_and(|v| match &v.kind {
                ViewKind::Base {
                    direction: ViewDirection::Given { toward_eye, x_axis },
                } => StandardView::of_axes(*toward_eye, *x_axis).is_none(),
                _ => false,
            })
    }

    /// The view spec of a view in its own style: a section's with its
    /// plane, a detail's as the view it enlarges looks (a detail of a
    /// section is cut by the section's plane).
    pub fn spec(&self, id: u32) -> Result<ViewSpec, String> {
        let (toward_eye, x_axis) = self.axes(id)?;
        let style = self.style(id);
        // The view a detail (of a detail ...) enlarges, or the view itself.
        let seen = self
            .lineage(id)?
            .into_iter()
            .find(|v| !matches!(v.kind, ViewKind::Detail { .. }))
            .ok_or_else(|| format!("view {id} enlarges no view"))?;
        let section = match &seen.kind {
            ViewKind::Section { origin, normal, .. } => Some(super::CutPlane {
                origin: *origin,
                normal: *normal,
            }),
            _ => None,
        };
        // An isometric view (however it is turned, mitcad#729) and a
        // section show no hidden edges.
        let pictorial = StandardView::isometric_eye(toward_eye)
            || matches!(
                seen.kind,
                ViewKind::Projected { side, .. } if side.is_corner()
            );
        Ok(ViewSpec {
            toward_eye,
            x_axis,
            hidden: style.hidden_lines && !pictorial && section.is_none(),
            smooth: style.tangent_edges,
            exact: !style.polygonal,
            quick: true,
            section,
            deflection: 0.01,
        })
    }

    /// The default label of a view: a section's and a detail's.
    pub fn label(&self, id: u32) -> Option<String> {
        let view = self.view(id)?;
        if let Some(label) = &view.label {
            return Some(label.clone());
        }
        match &view.kind {
            ViewKind::Section { letter, .. } => Some(format!("SECTION {letter}-{letter}")),
            ViewKind::Detail { letter, .. } => Some(format!(
                "DETAIL {letter} ({})",
                super::scale_text(self.scale(id))
            )),
            _ => None,
        }
    }

    fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a sheet needs a name".to_owned());
        }
        if let Some(units) = &self.units
            && super::Units::by_symbol(units).is_none()
        {
            return Err(format!("unknown length unit '{units}'"));
        }
        if let Some(template) = &self.template {
            template.check()?;
            let (width, height) = self.dimensions();
            template.fits(width, height)?;
        }
        for (i, view) in self.views.iter().enumerate() {
            let at = format!("view {}", view.id);
            if view.id == 0 {
                return Err("view ids start from 1".to_owned());
            }
            if !view.place.iter().all(|v| v.is_finite()) {
                return Err(format!("{at}: its place is not a point"));
            }
            if let Some(scale) = view.scale
                && !(scale.is_finite() && scale > 0.0)
            {
                return Err(format!("{at}: a scale is a positive number"));
            }
            if let Some(parent) = view.kind.parent()
                && !self.views[..i].iter().any(|v| v.id == parent)
            {
                return Err(format!(
                    "{at}: its parent {parent} is not an earlier view of the sheet"
                ));
            }
            if view.subject.is_some()
                && !matches!(
                    view.kind,
                    ViewKind::Base { .. } | ViewKind::Section { parent: None, .. }
                )
            {
                return Err(format!("{at}: only a base view has a subject"));
            }
            match &view.kind {
                ViewKind::Base { direction } => {
                    direction.check().map_err(|e| format!("{at}: {e}"))?;
                }
                ViewKind::Projected { .. } => {}
                ViewKind::Section {
                    parent,
                    origin,
                    normal,
                    span,
                    letter,
                    ..
                } => {
                    let length = dot(*normal, *normal).sqrt();
                    if !(origin.iter().all(|v| v.is_finite()) && (length - 1.0).abs() < 1e-6) {
                        return Err(format!(
                            "{at}: a section's plane is a point and a unit normal"
                        ));
                    }
                    if parent.is_none() && view.scale.is_none() {
                        return Err(format!("{at}: a section without a parent needs a scale"));
                    }
                    if let Some([a, b]) = span
                        && !(a.is_finite() && b.is_finite() && a < b)
                    {
                        return Err(format!(
                            "{at}: a section line runs from one point to another"
                        ));
                    }
                    check_letter(letter).map_err(|e| format!("{at}: {e}"))?;
                }
                ViewKind::Detail {
                    center,
                    radius,
                    letter,
                    ..
                } => {
                    if !(center.iter().all(|v| v.is_finite())
                        && radius.is_finite()
                        && *radius > 0.0)
                    {
                        return Err(format!("{at}: a detail is a circle of some size"));
                    }
                    check_letter(letter).map_err(|e| format!("{at}: {e}"))?;
                }
            }
            for b in &view.breaks {
                if !(b.from.is_finite() && b.to.is_finite() && b.from < b.to) {
                    return Err(format!("{at}: a break runs from one place to another"));
                }
                if !(b.gap.is_finite() && b.gap >= 0.0) {
                    return Err(format!("{at}: a break's gap is not negative"));
                }
            }
            for annotation in &view.annotations {
                annotation.check().map_err(|e| format!("{at}: {e}"))?;
            }
            if !view.rotation.is_finite() {
                return Err(format!("{at}: its rotation is a number of degrees"));
            }
            if view.rotation != 0.0 && matches!(view.kind, ViewKind::Detail { .. }) {
                return Err(format!("{at}: a detail turns with its parent"));
            }
            for item in &view.sketch {
                item.check().map_err(|e| format!("{at}: {e}"))?;
            }
            let mut along: Vec<&ViewBreak> = view.breaks.iter().collect();
            along.sort_by(|a, b| {
                (a.direction.axis(), a.from)
                    .partial_cmp(&(b.direction.axis(), b.from))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for pair in along.windows(2) {
                if pair[0].direction == pair[1].direction && pair[1].from < pair[0].to {
                    return Err(format!("{at}: its breaks overlap"));
                }
            }
        }
        for note in &self.notes {
            note.check()?;
        }
        for item in &self.sketch {
            if matches!(item, SketchItem::Text { .. }) {
                return Err(format!(
                    "text {}: a text on the sheet is one of its notes",
                    item.id()
                ));
            }
            item.check()?;
        }
        Ok(())
    }
}

fn check_letter(letter: &str) -> Result<(), String> {
    if letter.trim().is_empty() || letter.chars().any(char::is_whitespace) {
        Err("a section or detail is named by a letter".to_owned())
    } else {
        Ok(())
    }
}

/// The axes of a section view drawn from a parent: in line with the
/// parent where the plane holds the parent's y axis (a side view of the
/// cut) or its x axis (a view from above or below), else the plane's own
/// ([`section_axes`]).
pub fn section_view_axes(normal: Vec3, parent: (Vec3, Vec3)) -> (Vec3, Vec3) {
    let (pe, px) = parent;
    let py = cross(pe, px);
    if dot(normal, py).abs() < 1e-9 {
        // y' = py, the eye along the normal: x' = y' x eye.
        (normal, cross(py, normal))
    } else if dot(normal, px).abs() < 1e-9 {
        (normal, px)
    } else {
        section_axes(normal)
    }
}

/// The plane of a section line drawn on a view: through the line and the
/// view's direction, its normal turned a quarter to the left of the line
/// (`flip`: to the right), so that the view looks the way the arrows point.
/// Returns the origin, the unit normal and the line's span from the
/// origin's projection along `normal x toward_eye` (as the section line is
/// drawn), or None for a line of no length.
pub fn section_plane(spec: &ViewSpec, a: P2, b: P2, flip: bool) -> Option<(Vec3, Vec3, [f64; 2])> {
    let x = spec.x_axis;
    let y = spec.y_axis();
    let point = |p: P2| {
        [
            p[0] * x[0] + p[1] * y[0],
            p[0] * x[1] + p[1] * y[1],
            p[0] * x[2] + p[1] * y[2],
        ]
    };
    let (pa, pb) = (point(a), point(b));
    let along = unit([pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]])?;
    let mut normal = unit(cross(spec.toward_eye, along))?;
    if flip {
        normal = normal.map(|v| -v);
    }
    let line = cross(normal, spec.toward_eye);
    let t = |p: Vec3| dot([p[0] - pa[0], p[1] - pa[1], p[2] - pa[2]], line);
    let (ta, tb) = (t(pa), t(pb));
    Some((pa, normal, [ta.min(tb), ta.max(tb)]))
}
