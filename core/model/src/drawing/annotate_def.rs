// SPDX-License-Identifier: MIT
//! The annotations on a drawing's views as a design keeps them (mitcad#707,
//! phase 2; mitcad#729): their types, picks and settings, the text notes on
//! the sheet and the lines, arcs and texts drawn outside the model, with the
//! checks of what can be kept. They are part of a drawing's definition
//! ([`super::def`]); their layout on the sheet is the drawing crate's
//! (`mitcad-model-drawing`, its `annotate`), above the model (mitcad#777).

use std::f64::consts::TAU;

use mitcad_vec3::vec2::{norm as len2, sub as sub2};
use serde::{Deserialize, Serialize};

use super::P2;
use super::tables_def::TableSettings;
use crate::document::{AnnotationFormat, GeometricTolerance, MeasureKind, Tolerance, datum_letter};
use crate::features::OccurrencePath;
use crate::ids::BodyUid;
use crate::topo::{EdgeName, FaceName};

/// The most circles a centre pattern marks (mitcad#729).
pub const MAX_PATTERN: usize = 256;
/// The largest letters a drawing takes, paper mm (mitcad#729).
pub const MAX_TEXT_HEIGHT: f64 = 100.0;
/// The letters' height of dimensions and annotations without their own,
/// paper mm.
#[doc(hidden)]
pub const DIMENSION_TEXT: f64 = 3.5;

/// What an annotation on a view is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationType {
    /// Between two picks, horizontal, vertical or aligned in the view.
    Linear,
    /// Between two straight edges seen in the view.
    Angle,
    /// Of a circle or a round face.
    Radius,
    Diameter,
    /// From an origin pick to another, along the view's x or y.
    Ordinate,
    /// Crossed centre lines of a circle seen end on; the centreline of a
    /// round face seen from the side.
    CenterMark,
    /// Through the centres of two circles, between two edges, or a round
    /// face's axis.
    Centerline,
    /// A leader note: text on an arrow to a pick.
    Note,
    /// A datum label (ISO 5459).
    Datum,
    /// A feature control frame (ISO 1101).
    Frame,
    /// A surface finish symbol (ISO 1302).
    Finish,
    /// The round holes the view sees end on, tagged and listed
    /// (mitcad#707 phase 3, [`super::tables_def`]).
    HoleTable,
    /// The components the view shows, numbered and listed.
    PartsList,
    /// A component's number in a parts list, in a circle on a leader to
    /// it.
    Balloon,
    /// Centre marks on the circles of a pattern, made in one step, with
    /// the circle or the line through their centres (mitcad#729).
    CenterPattern,
    /// Thin lines that run straight edges on beyond their ends: to where
    /// two of them meet (a corner rounded or chamfered away), or one by a
    /// length (mitcad#729).
    EdgeExtension,
    /// A leader note on a hole (mitcad#2054): its count, size, depth or
    /// thread and its counterbore or countersink as the model's hole
    /// callout writes them, worked out whenever the sheet is drawn, with
    /// the drawing's own texts before, after or in place of them
    /// (`format`).
    HoleCallout,
    /// A weld symbol (mitcad#2350): a reference line with its arrow to an
    /// edge, a corner or a joint, the arrow side's and the other side's
    /// symbols with their sizes, and a tail ([`WeldSymbol`]), drawn by the
    /// drawing's standard (ISO 2553 or AWS A2.4).
    Weld,
}

impl AnnotationType {
    pub const ALL: [Self; 18] = [
        Self::Linear,
        Self::Angle,
        Self::Radius,
        Self::Diameter,
        Self::Ordinate,
        Self::CenterMark,
        Self::Centerline,
        Self::Note,
        Self::Datum,
        Self::Frame,
        Self::Finish,
        Self::HoleTable,
        Self::PartsList,
        Self::Balloon,
        Self::CenterPattern,
        Self::EdgeExtension,
        Self::HoleCallout,
        Self::Weld,
    ];

    /// Its name in commands and files.
    pub fn key(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Angle => "angle",
            Self::Radius => "radius",
            Self::Diameter => "diameter",
            Self::Ordinate => "ordinate",
            Self::CenterMark => "center_mark",
            Self::Centerline => "centerline",
            Self::Note => "note",
            Self::Datum => "datum",
            Self::Frame => "frame",
            Self::Finish => "finish",
            Self::HoleTable => "hole_table",
            Self::PartsList => "parts_list",
            Self::Balloon => "balloon",
            Self::CenterPattern => "center_pattern",
            Self::EdgeExtension => "edge_extension",
            Self::HoleCallout => "hole_callout",
            Self::Weld => "weld",
        }
    }

    /// How many picks it takes, at least and at most: a hole table its
    /// origin or none, a parts list none, a centre pattern its circles.
    pub fn picks(self) -> (usize, usize) {
        match self {
            Self::Linear | Self::Angle | Self::Ordinate => (2, 2),
            Self::Centerline | Self::EdgeExtension => (1, 2),
            Self::CenterPattern => (1, MAX_PATTERN),
            Self::HoleTable => (0, 1),
            Self::PartsList => (0, 0),
            _ => (1, 1),
        }
    }

    /// Whether it is a table ([`super::tables_def`]): placed on the sheet, its
    /// rows worked out from the model.
    pub fn is_table(self) -> bool {
        matches!(self, Self::HoleTable | Self::PartsList)
    }

    /// Whether it shows a measured value.
    pub fn is_dimension(self) -> bool {
        matches!(
            self,
            Self::Linear | Self::Angle | Self::Radius | Self::Diameter | Self::Ordinate
        )
    }

    /// The kind of #702's dimensions whose words it takes: its prefix (Ø,
    /// R), whether its value is a length or an angle.
    pub fn measure_kind(self) -> MeasureKind {
        match self {
            Self::Angle => MeasureKind::Angle,
            Self::Radius => MeasureKind::Radius,
            Self::Diameter => MeasureKind::Diameter,
            _ => MeasureKind::Distance,
        }
    }

    /// What a click for it picks: points first, lines, circles or round
    /// faces, or anything.
    pub fn wants(self) -> Want {
        match self {
            Self::Ordinate | Self::HoleTable => Want::Point,
            Self::Angle => Want::Straight,
            Self::EdgeExtension => Want::Line,
            Self::Radius | Self::Diameter => Want::Round,
            Self::CenterMark | Self::CenterPattern | Self::HoleCallout => Want::Circle,
            Self::Datum => Want::Datum,
            _ => Want::Any,
        }
    }

    /// Whether its picks may be a centre line or a centre pattern's
    /// circle the view draws ([`ViewPick::drawn`], mitcad#1693): not for
    /// what draws centre lines itself, an edge extension or a table.
    pub fn takes_drawn(self) -> bool {
        !matches!(
            self,
            Self::CenterMark
                | Self::Centerline
                | Self::CenterPattern
                | Self::EdgeExtension
                | Self::HoleTable
                | Self::PartsList
                | Self::Balloon
                | Self::HoleCallout
                | Self::Weld
        )
    }
}

/// What a click on a view picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Want {
    /// A point near the click if there is one, else an edge or a face.
    #[default]
    Any,
    /// An edge's end or a circle's centre (a circle's rim gives its
    /// centre).
    Point,
    /// A straight edge.
    Line,
    /// A circle or a round face.
    Circle,
    /// A point a line or an arc drawn outside the model snaps to
    /// (mitcad#1272): an edge's end, a straight edge's middle, or a
    /// circle's or an arc's centre.
    Snap,
    /// What a datum label goes on (mitcad#1296): as `Any`, else a planar
    /// face the view sees face on with the click inside its outline.
    Datum,
    /// A model edge, whole, whatever its curve (mitcad#1546: the edge whose
    /// line Edit Line changes): no point and no face.
    Edge,
    /// What a dimension whose picks choose its type picks (mitcad#1693):
    /// as `Any`, and a centre pattern's circle.
    Dimension,
    /// What a radius or a diameter measures (mitcad#1693): a circle or a
    /// round face, or a centre pattern's circle.
    Round,
    /// What an angle is between (mitcad#1693): a straight edge, or a
    /// centre line the view draws (a centre mark's line, a centre
    /// pattern's radial line, a centreline).
    Straight,
}

/// A centre line or a centre pattern's circle that an annotation of the
/// view (or the view's automatic centre marks) draws, picked as a line or
/// a circle of its own (mitcad#1693): what a pick with it stands for in
/// place of its edge or face, which names the circle it is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Drawn {
    /// The circle (an arc over them where they stand on a part of it)
    /// through the centres of the circles of the centre pattern
    /// annotation `pattern` of the same view; the pick one of them.
    PatternCircle { pattern: u32 },
    /// The radial line of the centre pattern annotation `pattern` through
    /// the centre of the pick's circle, one of the pattern's: from the
    /// pattern's centre out past that circle.
    PatternLine { pattern: u32 },
    /// One line of the centre mark of the pick's circle, from its centre
    /// toward `arm` on the sheet, past its rim.
    CenterMark { arm: MarkArm },
    /// The line of the centreline annotation `centerline` of the same
    /// view; the pick its first.
    Centerline { centerline: u32 },
}

/// Which line of a centre mark, from the circle's centre: toward the
/// sheet's right, top, left or bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkArm {
    Right,
    Up,
    Left,
    Down,
}

impl MarkArm {
    pub const ALL: [Self; 4] = [Self::Right, Self::Up, Self::Left, Self::Down];

    /// Its direction on the sheet (unit).
    pub fn direction(self) -> [f64; 2] {
        match self {
            Self::Right => [1.0, 0.0],
            Self::Up => [0.0, 1.0],
            Self::Left => [-1.0, 0.0],
            Self::Down => [0.0, -1.0],
        }
    }

    /// The one nearest a direction on the sheet.
    pub fn nearest(d: [f64; 2]) -> Self {
        Self::ALL
            .into_iter()
            .max_by(|a, b| {
                let (da, db) = (a.direction(), b.direction());
                (da[0] * d[0] + da[1] * d[1]).total_cmp(&(db[0] * d[0] + db[1] * d[1]))
            })
            .unwrap_or(Self::Right)
    }
}

/// The direction a linear or an ordinate dimension measures along: the
/// view's x, its y, or the line between its picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Along {
    Horizontal,
    Vertical,
    Aligned,
}

/// A point of an edge or a round face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickPoint {
    Start,
    End,
    /// A circle's or a round face's centre, a straight edge's middle.
    Center,
}

/// Model topology picked on a view: a body where an occurrence places it
/// (a path from the drawing's component; its own placement when left
/// out) and an edge or a face of it, or a point of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewPick {
    pub body: BodyUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<EdgeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<FaceName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<PickPoint>,
    /// A point of a planar face (mitcad#1296: a datum label on the face,
    /// picked inside its outline where the view sees it face on), in the
    /// face's own coordinates: millimetres from its plane's origin along
    /// the plane's own axes ([`face_axes`]), in its body's coordinates; so
    /// it keeps its place on the face when the model or the view changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_face: Option<[f64; 2]>,
    /// A point of a planar face a section's plane cuts, on its cut line
    /// (mitcad#1361), in the face's own coordinates as `on_face`: the pick
    /// is then the piece of the cut line under it (the cut edges joined end
    /// to end that it lies on), not the line across the face's gaps; its
    /// `point` that piece's start, end or middle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_cut: Option<[f64; 2]>,
    /// A centre line or a centre pattern's circle the view draws for this
    /// pick's circle, picked as a line or a circle of its own
    /// (mitcad#1693): the pick then stands for that, found again on every
    /// draw from the circle (or the centreline's first pick) and the
    /// annotation drawing it, so it follows the model as they do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawn: Option<Drawn>,
    /// Which side of a torus face's axis it is picked on (mitcad#1694: a
    /// fillet round a turned part seen from the side, whose outline is an
    /// arc of its tube either side of the axis): a direction from the axis
    /// that way, in its body's coordinates. The pick is then that arc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<[f64; 3]>,
    /// A silhouette of a curved face (mitcad#1909): the outline a view
    /// draws where the face turns away from the eye (a torus seen along
    /// its axis, a sphere, a cone, a free-form face), picked by a point of
    /// it in its body's coordinates. The pick is then that outline: of the
    /// face's silhouettes seen along the view's direction, the one nearest
    /// the point, so it follows the model while the face keeps an outline
    /// there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<[f64; 3]>,
    /// An extreme point (mitcad#1909): a direction in its body's
    /// coordinates. The pick is then the point of what the rest of it
    /// names (an edge, a round face, a torus face's side, a silhouette)
    /// that lies furthest that way as its view sees it: the top of a
    /// curved outline for an overall size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extreme: Option<[f64; 3]>,
    /// A bend line of a Flat Pattern view (mitcad#2280): the pick is then
    /// that line of the body's developed blank (no edge and no face), or
    /// its `point`, found again on every draw from the pattern, so it
    /// follows the model while the bend is there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend: Option<BendLinePick>,
}

/// A bend line of a Flat Pattern (mitcad#2280): a bend of the sheet and
/// which piece of its centre line (a bend whose material is cut across has
/// one piece each side), from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BendLinePick {
    pub bend: crate::sheet::BendId,
    #[serde(default, skip_serializing_if = "is_first")]
    pub piece: usize,
}

fn is_first(piece: &usize) -> bool {
    *piece == 0
}

impl ViewPick {
    /// The occurrence path it names, empty for none.
    pub fn path(&self) -> &[crate::ids::OccurrenceUid] {
        self.occurrence.as_ref().map_or(&[], |p| p.0.as_slice())
    }

    fn check(&self) -> Result<(), String> {
        // A Flat Pattern's bend line (mitcad#2280): the line or a point of
        // it, nothing else.
        if self.bend.is_some() {
            if self.edge.is_some()
                || self.face.is_some()
                || self.on_face.is_some()
                || self.on_cut.is_some()
                || self.drawn.is_some()
                || self.side.is_some()
                || self.outline.is_some()
                || self.extreme.is_some()
            {
                return Err("a bend line is picked alone: no edge or face with it".to_owned());
            }
            return Ok(());
        }
        if self.drawn.is_some()
            && (self.point.is_some() || self.on_face.is_some() || self.on_cut.is_some())
        {
            return Err(
                "a center line or a center pattern's circle is picked whole: no point".to_owned(),
            );
        }
        if let Some(side) = self.side {
            if self.face.is_none()
                || self.edge.is_some()
                || self.on_face.is_some()
                || self.on_cut.is_some()
            {
                return Err("a side of an axis is a pick of a round face alone".to_owned());
            }
            if side.iter().any(|v| !v.is_finite()) {
                return Err("a side of an axis is three numbers".to_owned());
            }
        }
        if let Some(on) = self.outline {
            if self.face.is_none()
                || self.edge.is_some()
                || self.side.is_some()
                || self.on_face.is_some()
                || self.on_cut.is_some()
                || matches!(self.point, Some(PickPoint::Start | PickPoint::End))
            {
                return Err("an outline is a pick of a curved face alone".to_owned());
            }
            if on.iter().any(|v| !v.is_finite()) {
                return Err("a point of an outline is three numbers".to_owned());
            }
        }
        if let Some(toward) = self.extreme {
            if self.point.is_some()
                || self.on_face.is_some()
                || self.on_cut.is_some()
                || self.drawn.is_some()
            {
                return Err(
                    "an extreme point is of an edge, a round face or an outline picked whole"
                        .to_owned(),
                );
            }
            if toward.iter().any(|v| !v.is_finite()) || toward.iter().all(|v| *v == 0.0) {
                return Err("the direction of an extreme point is three numbers".to_owned());
            }
        }
        if let Some(on) = self.on_face {
            if self.face.is_none() || self.edge.is_some() || self.point.is_some() {
                return Err("a point on a face is a pick of the face alone".to_owned());
            }
            if on.iter().any(|v| !v.is_finite()) {
                return Err("a point on a face is two numbers".to_owned());
            }
        }
        if let Some(on) = self.on_cut {
            if self.face.is_none() || self.edge.is_some() || self.on_face.is_some() {
                return Err("a point on a cut line is a pick of the face it cuts".to_owned());
            }
            if on.iter().any(|v| !v.is_finite()) {
                return Err("a point on a cut line is two numbers".to_owned());
            }
        }
        match (&self.edge, &self.face, self.point) {
            (Some(_), None, _) => Ok(()),
            (None, Some(_), None | Some(PickPoint::Center)) => Ok(()),
            // A piece of a section's cut line has ends (mitcad#1361).
            (None, Some(_), Some(_)) if self.on_cut.is_some() => Ok(()),
            (None, Some(_), Some(_)) => Err("a face's point is its center".to_owned()),
            _ => Err("a pick is a body's edge or face".to_owned()),
        }
    }
}

/// A planar face's own axes (mitcad#1296): two unit vectors at right
/// angles to each other and to its unit `normal`, chosen from the normal
/// alone, so that a point kept in them ([`ViewPick::on_face`]) stays on
/// the face as long as its plane keeps its direction.
pub fn face_axes(normal: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let n = mitcad_vec3::unit(normal).unwrap_or([0.0, 0.0, 1.0]);
    let other = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let x = mitcad_vec3::unit(mitcad_vec3::cross(other, n)).unwrap_or([0.0, 1.0, 0.0]);
    (x, mitcad_vec3::cross(n, x))
}

/// How a datum label's leader ends on its feature (ISO 5459,
/// mitcad#1296): a filled triangle on an edge or its extension line, a
/// filled dot on a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terminator {
    Triangle,
    Dot,
}

impl Terminator {
    pub fn key(self) -> &'static str {
        match self {
            Self::Triangle => "triangle",
            Self::Dot => "dot",
        }
    }
}

/// How a surface finish symbol says the surface is made (ISO 1302): any
/// way, by removing material (a bar across the symbol) or without (a
/// circle in it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishProcess {
    #[default]
    Any,
    Removal,
    NoRemoval,
}

/// A surface finish symbol's requirements: the process and the text over
/// it (`Ra 1.6`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceFinish {
    #[serde(default, skip_serializing_if = "is_any")]
    pub process: FinishProcess,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

fn is_any(process: &FinishProcess) -> bool {
    *process == FinishProcess::Any
}

// --- Weld symbols (mitcad#2350) ---------------------------------------------

/// The most a weld symbol's sizes may be, in the drawing's length unit.
pub const MAX_WELD_SIZE: f64 = 100_000.0;
/// The most welds of an intermittent weld a symbol counts.
pub const MAX_WELD_COUNT: u32 = 10_000;
/// The longest tail text and joint key of a weld symbol, characters.
pub const MAX_WELD_TEXT: usize = 200;

/// A weld symbol's basic symbol (ISO 2553, AWS A2.4; mitcad#2350): the
/// weld's kind, drawn on its side of the reference line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldKind {
    /// A triangle, its upright leg on the left.
    Fillet,
    /// Square butt (I): two upright lines.
    Square,
    /// Single V butt.
    V,
    /// Single bevel butt (half V): its upright leg on the left.
    Bevel,
    /// Single V butt with a broad root face (Y).
    Y,
    /// Single U butt.
    U,
    /// Single J butt: its upright leg on the left.
    J,
    /// Flare V: two arcs back to back.
    FlareV,
    /// Flare bevel: an upright line and an arc.
    FlareBevel,
    /// Plug or slot: a rectangle on the line.
    Plug,
    /// Spot: a circle.
    Spot,
    /// Seam: a circle with two lines through it.
    Seam,
    /// Back or backing weld (a back run): a half circle.
    BackWeld,
    /// A backing strip: a rectangle closed on the line.
    Backing,
    /// Surfacing: two arcs side by side.
    Surfacing,
}

impl WeldKind {
    pub const ALL: [Self; 15] = [
        Self::Fillet,
        Self::Square,
        Self::V,
        Self::Bevel,
        Self::Y,
        Self::U,
        Self::J,
        Self::FlareV,
        Self::FlareBevel,
        Self::Plug,
        Self::Spot,
        Self::Seam,
        Self::BackWeld,
        Self::Backing,
        Self::Surfacing,
    ];

    /// Its name in commands and files.
    pub fn key(self) -> &'static str {
        match self {
            Self::Fillet => "fillet",
            Self::Square => "square",
            Self::V => "v",
            Self::Bevel => "bevel",
            Self::Y => "y",
            Self::U => "u",
            Self::J => "j",
            Self::FlareV => "flare_v",
            Self::FlareBevel => "flare_bevel",
            Self::Plug => "plug",
            Self::Spot => "spot",
            Self::Seam => "seam",
            Self::BackWeld => "back_weld",
            Self::Backing => "backing",
            Self::Surfacing => "surfacing",
        }
    }

    /// Its words in the `drawing` query's label.
    pub fn words(self) -> &'static str {
        match self {
            Self::Fillet => "fillet",
            Self::Square => "square butt",
            Self::V => "V butt",
            Self::Bevel => "bevel butt",
            Self::Y => "Y butt",
            Self::U => "U butt",
            Self::J => "J butt",
            Self::FlareV => "flare V",
            Self::FlareBevel => "flare bevel",
            Self::Plug => "plug",
            Self::Spot => "spot",
            Self::Seam => "seam",
            Self::BackWeld => "back weld",
            Self::Backing => "backing",
            Self::Surfacing => "surfacing",
        }
    }

    /// Whether it is a groove (butt) weld: its size the depth of its
    /// preparation, its penetration written with it.
    pub fn is_groove(self) -> bool {
        matches!(
            self,
            Self::Square
                | Self::V
                | Self::Bevel
                | Self::Y
                | Self::U
                | Self::J
                | Self::FlareV
                | Self::FlareBevel
        )
    }
}

/// What a fillet weld's size measures (ISO 2553): its throat (`a`, the
/// default) or its leg (`z`). AWS A2.4 writes a leg bare and a throat in
/// parentheses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilletMeasure {
    #[default]
    Throat,
    Leg,
}

/// The weld's face: flush (flat), convex or concave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldContour {
    Flush,
    Convex,
    Concave,
}

/// How a weld's face is finished, written as its letter beyond its
/// contour: chipped (C), ground (G), hammered (H), machined (M), rolled
/// (R), by any way (U).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeldFinish {
    Chip,
    Grind,
    Hammer,
    Machine,
    Roll,
    Unspecified,
}

impl WeldFinish {
    /// Its letter on the drawing.
    pub fn letter(self) -> &'static str {
        match self {
            Self::Chip => "C",
            Self::Grind => "G",
            Self::Hammer => "H",
            Self::Machine => "M",
            Self::Roll => "R",
            Self::Unspecified => "U",
        }
    }
}

/// One side's weld of a weld symbol: its basic symbol and its sizes, in
/// the drawing's length unit as given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeldSide {
    pub symbol: WeldKind,
    /// A fillet's throat or leg (`measure`), a groove's depth of
    /// preparation, a spot's or a plug's diameter, a seam's width, a
    /// surfacing's thickness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    /// What a fillet's size measures; the throat when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measure: Option<FilletMeasure>,
    /// The depth of penetration (ISO's `s`; AWS's effective throat in
    /// parentheses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub penetration: Option<f64>,
    /// How many welds an intermittent weld (or spots, plugs) has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// Each weld's length (a continuous weld's when no pitch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    /// An intermittent weld's pitch, from one weld's middle to the next:
    /// more than its length. ISO 2553 writes the gap between them (`e`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contour: Option<WeldContour>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<WeldFinish>,
    /// The weld's toes blended smoothly into the parts.
    #[serde(default, skip_serializing_if = "is_false")]
    pub toes_blended: bool,
}

impl WeldSide {
    /// A weld of this kind without sizes.
    pub fn new(symbol: WeldKind) -> Self {
        Self {
            symbol,
            size: None,
            measure: None,
            penetration: None,
            count: None,
            length: None,
            pitch: None,
            contour: None,
            finish: None,
            toes_blended: false,
        }
    }

    fn check(&self) -> Result<(), String> {
        let size = |value: Option<f64>, what: &str| match value {
            Some(v) if !(v.is_finite() && v > 0.0 && v <= MAX_WELD_SIZE) => Err(format!(
                "a weld's {what} is more than 0 and at most {MAX_WELD_SIZE}"
            )),
            _ => Ok(()),
        };
        size(self.size, "size")?;
        size(self.penetration, "penetration")?;
        size(self.length, "length")?;
        size(self.pitch, "pitch")?;
        if self.measure.is_some() && self.symbol != WeldKind::Fillet {
            return Err("only a fillet weld's size measures its throat or its leg".to_owned());
        }
        if let Some(count) = self.count
            && !(1..=MAX_WELD_COUNT).contains(&count)
        {
            return Err(format!("a weld symbol counts 1 to {MAX_WELD_COUNT} welds"));
        }
        if let (Some(length), Some(pitch)) = (self.length, self.pitch)
            && pitch <= length
        {
            return Err("an intermittent weld's pitch is more than each weld's length".to_owned());
        }
        if self.pitch.is_some() && self.length.is_none() {
            return Err("an intermittent weld's pitch goes with each weld's length".to_owned());
        }
        Ok(())
    }
}

/// A weld symbol (ISO 2553 or AWS A2.4 by the drawing's standard,
/// mitcad#2350): the arrow side's weld and the other side's, each with its
/// own symbol and sizes, the supplementary symbols at the arrow's end of
/// the reference line (all around, field weld), and the tail's text (a
/// process number or a note). A weld proposed for a BUILD joint keeps the
/// joint's key (`joint`), so that proposing again finds it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeldSymbol {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrow_side: Option<WeldSide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_side: Option<WeldSide>,
    /// Welded all around: a circle where the arrow meets the reference
    /// line.
    #[serde(default, skip_serializing_if = "is_false")]
    pub all_around: bool,
    /// Welded on site (a field weld): a flag where the arrow meets the
    /// reference line.
    #[serde(default, skip_serializing_if = "is_false")]
    pub field: bool,
    /// The two sides' intermittent welds staggered: the other side's
    /// symbol drawn offset along the line.
    #[serde(default, skip_serializing_if = "is_false")]
    pub staggered: bool,
    /// The tail's text: a process number (ISO 4063), a specification or a
    /// note; no tail when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<String>,
    /// The BUILD joint it was proposed for (`build_junctions`' id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<String>,
}

impl WeldSymbol {
    /// A symbol of one weld on the arrow side.
    pub fn arrow(side: WeldSide) -> Self {
        Self {
            arrow_side: Some(side),
            ..Self::default()
        }
    }

    /// Why it cannot be kept, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        if self.arrow_side.is_none() && self.other_side.is_none() {
            return Err(
                "a weld symbol has a weld on the arrow side, the other side or both".to_owned(),
            );
        }
        for side in [&self.arrow_side, &self.other_side].into_iter().flatten() {
            side.check()?;
        }
        if self.staggered && (self.arrow_side.is_none() || self.other_side.is_none()) {
            return Err("staggered welds are on both sides".to_owned());
        }
        if let Some(tail) = &self.tail
            && (tail.trim().is_empty() || tail.chars().count() > MAX_WELD_TEXT)
        {
            return Err(format!(
                "a weld symbol's tail has a text of 1 to {MAX_WELD_TEXT} characters"
            ));
        }
        if let Some(joint) = &self.joint
            && (joint.trim().is_empty()
                || joint.trim() != joint.as_str()
                || joint.chars().count() > MAX_WELD_TEXT)
        {
            return Err("a weld symbol's joint is a BUILD joint's key".to_owned());
        }
        Ok(())
    }

    /// Its words in the `drawing` query: each side's weld and sizes, then
    /// all around, field weld and the tail.
    pub fn label(&self) -> String {
        let number = |v: f64| {
            let text = format!("{v:.3}");
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        };
        let side = |s: &WeldSide, which: &str| {
            let mut words = vec![s.symbol.words().to_owned()];
            if let Some(size) = s.size {
                let prefix = match (s.symbol, s.measure.unwrap_or_default()) {
                    (WeldKind::Fillet, FilletMeasure::Throat) => "a",
                    (WeldKind::Fillet, FilletMeasure::Leg) => "z",
                    _ => "",
                };
                words.push(format!("{prefix}{}", number(size)));
            }
            if let Some(p) = s.penetration {
                words.push(format!("s{}", number(p)));
            }
            match (s.count, s.length, s.pitch) {
                (count, Some(l), Some(p)) => words.push(format!(
                    "{}{}-{}",
                    count.map(|n| format!("{n}×")).unwrap_or_default(),
                    number(l),
                    number(p)
                )),
                (count, Some(l), None) => words.push(format!(
                    "{}{}",
                    count.map(|n| format!("{n}×")).unwrap_or_default(),
                    number(l)
                )),
                (Some(n), None, _) => words.push(format!("{n}×")),
                (None, None, _) => {}
            }
            format!("{} ({which})", words.join(" "))
        };
        let mut parts = Vec::new();
        if let Some(s) = &self.arrow_side {
            parts.push(side(s, "arrow side"));
        }
        if let Some(s) = &self.other_side {
            parts.push(side(s, "other side"));
        }
        if self.staggered {
            parts.push("staggered".to_owned());
        }
        if self.all_around {
            parts.push("all around".to_owned());
        }
        if self.field {
            parts.push("field weld".to_owned());
        }
        if let Some(tail) = &self.tail {
            parts.push(tail.trim().to_owned());
        }
        parts.join(" / ")
    }
}

/// An annotation on a view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewAnnotation {
    /// Unique among the drawing's annotations and text notes, from 1.
    pub id: u32,
    #[serde(rename = "type")]
    pub kind: AnnotationType,
    pub picks: Vec<ViewPick>,
    /// A linear dimension's (and an ordinate's: horizontal or vertical).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub along: Option<Along>,
    /// A dimension line's place across it from its default one's anchor
    /// (paper mm, signed): a linear dimension's from the middle of its
    /// picks, an ordinate's from its origin, an angle's arc radius; the
    /// default (beyond the view's box) when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_offset: Option<f64>,
    /// The text's shift from its default place, paper mm (along and across
    /// a dimension line); a leader's text, a datum's square, a frame, a
    /// finish symbol from the arrow's point; a hole callout's on a circle
    /// seen end on from the circle's centre (`text_from_center`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_offset: Option<P2>,
    /// Where on the view (view mm) a leader's pick was clicked: its arrow
    /// points at the pick's point nearest it (not a hole callout's on a
    /// circle seen end on kept from its centre: `text_from_center`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<P2>,
    #[serde(default, skip_serializing_if = "AnnotationFormat::is_default")]
    pub format: AnnotationFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<Tolerance>,
    /// A note's text, lines split at `\n`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A datum label's letter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<GeometricTolerance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<SurfaceFinish>,
    /// A weld symbol's welds, sizes and tail (mitcad#2350).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weld: Option<WeldSymbol>,
    /// A hole table's or a parts list's settings (its place on the sheet,
    /// columns, sort, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableSettings>,
    /// Its letters' height, paper mm: its own over the drawing's
    /// (mitcad#729); the drawing's (3.5 mm unless it has one) when left
    /// out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    /// How far a horizontal or vertical dimension's direction has turned
    /// with its view (degrees, counter-clockwise; mitcad#729): it keeps
    /// measuring along the same direction of the model when its view is
    /// rotated. None: along the view's x or y.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<f64>,
    /// Where a datum label's triangle or a frame's arrow sits along its
    /// pick (mitcad#1295): the fraction of the way from the pick's start,
    /// by its own direction (an edge from its start to its end, an arc
    /// from its first end to its second, a round face along its axis), 0
    /// to 1; so it keeps its place on the edge when the model or the view
    /// changes. None (older files, a point, a whole circle): the pick's
    /// point nearest `at`. A datum label on a straight edge may sit on the
    /// edge's line beyond its ends (below 0 or above 1, mitcad#1296): it
    /// then draws its own extension line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip_along: Option<f64>,
    /// A datum label's terminator (mitcad#1296); left out: a dot on a
    /// face picked face on ([`ViewPick::on_face`]), else a triangle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminator: Option<Terminator>,
    /// The model annotation it was kept from (mitcad#1406), by its name
    /// (`Distance1`): a copy the drawing owns, moved and edited without
    /// changing the model. While it is on a view of a sheet, the model's
    /// annotation is not brought into that sheet's views again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_from: Option<String>,
    /// A hole callout's text kept from its hole's centre (mitcad#2212):
    /// on a circle seen end on, `text_offset` is the leader's shoulder from
    /// the circle's centre and the arrow lands where the line from the
    /// shoulder to the centre crosses the circle. Unset (a callout written
    /// before, one kept from the model): `text_offset` is from where its
    /// arrow was (the circle's point nearest `at`, else its point at 0°),
    /// read so when laid out (the text stays where it was drawn, the arrow
    /// points at the centre) and stored from the centre at its next move.
    #[serde(default, skip_serializing_if = "is_false")]
    pub text_from_center: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// A text note on the sheet, without a leader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetNote {
    /// Unique among the drawing's annotations and text notes.
    pub id: u32,
    /// Lines split at `\n`.
    pub text: String,
    /// Where its first line starts on the sheet, mm.
    pub at: P2,
    /// Its letters' height, mm (3.5 when left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

impl SheetNote {
    pub fn check(&self) -> Result<(), String> {
        let at = format!("note {}", self.id);
        if self.text.trim().is_empty() {
            return Err(format!("{at}: a note needs a text"));
        }
        if !self.at.iter().all(|v| v.is_finite()) {
            return Err(format!("{at}: its place is not a point"));
        }
        if let Some(h) = self.height
            && !(h.is_finite() && h > 0.0)
        {
            return Err(format!("{at}: its height is more than 0"));
        }
        Ok(())
    }
}

impl ViewAnnotation {
    /// An annotation of this type on these picks, every other field left
    /// to its default.
    pub fn new(id: u32, kind: AnnotationType, picks: Vec<ViewPick>) -> Self {
        Self {
            id,
            kind,
            picks,
            along: None,
            line_offset: None,
            text_offset: None,
            at: None,
            format: AnnotationFormat::default(),
            tolerance: None,
            text: None,
            letter: None,
            frame: None,
            finish: None,
            weld: None,
            table: None,
            height: None,
            turn: None,
            tip_along: None,
            terminator: None,
            kept_from: None,
            text_from_center: false,
        }
    }

    /// A datum label's terminator: its own, else a dot on a face picked
    /// face on and a triangle on anything else (mitcad#1296).
    pub fn terminator(&self) -> Terminator {
        self.terminator.unwrap_or_else(|| {
            if self.picks.first().is_some_and(|p| p.on_face.is_some()) {
                Terminator::Dot
            } else {
                Terminator::Triangle
            }
        })
    }

    /// Why it cannot be kept, if it cannot (not its geometry).
    pub fn check(&self) -> Result<(), String> {
        let at = format!("annotation {}", self.id);
        self.check_fields().map_err(|e| format!("{at}: {e}"))
    }

    fn check_fields(&self) -> Result<(), String> {
        let kind = self.kind;
        let name = kind.key().replace('_', " ");
        let (least, most) = kind.picks();
        if !(least..=most).contains(&self.picks.len()) {
            return Err(if least == most {
                format!(
                    "a {name} takes {least} pick{}",
                    if least == 1 { "" } else { "s" }
                )
            } else {
                format!("a {name} takes {least} to {most} picks")
            });
        }
        for pick in &self.picks {
            pick.check()?;
        }
        if !kind.takes_drawn() && self.picks.iter().any(|p| p.drawn.is_some()) {
            return Err(format!(
                "a {name} is of the model's edges and faces, not of a center line or a center \
                 pattern's circle"
            ));
        }
        match (kind, self.along) {
            (AnnotationType::Linear, None) => {
                return Err("a linear dimension measures along a direction".to_owned());
            }
            (AnnotationType::Ordinate, None | Some(Along::Aligned)) => {
                return Err("an ordinate dimension measures along the view's x or y".to_owned());
            }
            (AnnotationType::Linear | AnnotationType::Ordinate, Some(_)) | (_, None) => {}
            (_, Some(_)) => return Err(format!("a {name} has no direction to measure along")),
        }
        if let Some(offset) = self.line_offset
            && !offset.is_finite()
        {
            return Err("the line's offset is a number".to_owned());
        }
        if self
            .text_offset
            .is_some_and(|o| o.iter().any(|v| !v.is_finite()))
            || self.at.is_some_and(|o| o.iter().any(|v| !v.is_finite()))
        {
            return Err("a place is two numbers".to_owned());
        }
        if let Some(t) = self.tip_along {
            match kind {
                // On the edge's line beyond its ends too (mitcad#1296).
                AnnotationType::Datum if !t.is_finite() => {
                    return Err("a place along an edge is a number".to_owned());
                }
                AnnotationType::Datum => {}
                AnnotationType::Frame if !(0.0..=1.0).contains(&t) => {
                    return Err("a place along an edge is a fraction from 0 to 1".to_owned());
                }
                AnnotationType::Frame => {}
                _ => return Err(format!("a {name} has no place along its edge")),
            }
        }
        if self.terminator.is_some() && kind != AnnotationType::Datum {
            return Err(format!("a {name} has no terminator: a datum label has"));
        }
        if self.text_from_center && kind != AnnotationType::HoleCallout {
            return Err(format!(
                "a {name} keeps its text from its arrow: a hole callout keeps it from its hole's \
                 centre"
            ));
        }
        if kind.is_dimension() {
            crate::document::check_format(&self.format)?;
            if let Some(tolerance) = &self.tolerance {
                crate::document::check_tolerance(tolerance, kind.measure_kind())?;
            }
        } else if kind == AnnotationType::HoleCallout {
            // Its own texts before, after and in place of the hole's lines
            // (mitcad#2054); the hole's sizes are written as the model's
            // callout writes them.
            if self.format.decimals.is_some()
                || self.format.prefix.is_some()
                || self.tolerance.is_some()
            {
                return Err(
                    "a hole callout takes texts before, after or in place of its lines: no \
                     decimals, prefix or tolerance"
                        .to_owned(),
                );
            }
        } else if !self.format.is_default() || self.tolerance.is_some() {
            return Err(format!("a {name} has no value's format or tolerance"));
        }
        if self.text.is_some() != (kind == AnnotationType::Note) {
            return Err(if kind == AnnotationType::Note {
                "a note needs a text".to_owned()
            } else {
                format!("a {name} has no text of its own")
            });
        }
        if let Some(text) = &self.text
            && text.trim().is_empty()
        {
            return Err("a note needs a text".to_owned());
        }
        match (&self.letter, kind) {
            (Some(letter), AnnotationType::Datum) => {
                if datum_letter(letter)? != *letter {
                    return Err("a datum's letter is written without spaces".to_owned());
                }
            }
            (None, AnnotationType::Datum) => return Err("a datum needs its letter".to_owned()),
            (Some(_), _) => return Err("only a datum has a letter".to_owned()),
            (None, _) => {}
        }
        match (&self.frame, kind) {
            (Some(frame), AnnotationType::Frame) => crate::document::check_frame(frame)?,
            (None, AnnotationType::Frame) => {
                return Err("a feature control frame needs its tolerance".to_owned());
            }
            (Some(_), _) => {
                return Err("only a feature control frame has a geometric tolerance".to_owned());
            }
            (None, _) => {}
        }
        if self.finish.is_some() && kind != AnnotationType::Finish {
            return Err("only a surface finish symbol has a finish".to_owned());
        }
        match (&self.weld, kind) {
            (Some(weld), AnnotationType::Weld) => weld.check()?,
            (None, AnnotationType::Weld) => {
                return Err("a weld symbol needs its welds".to_owned());
            }
            (Some(_), _) => return Err("only a weld symbol has welds".to_owned()),
            (None, _) => {}
        }
        match (&self.table, kind.is_table()) {
            (Some(table), true) => table.check(kind == AnnotationType::HoleTable)?,
            (None, true) => return Err(format!("a {name} needs its place on the sheet")),
            (Some(_), false) => return Err(format!("a {name} is no table")),
            (None, false) => {}
        }
        if let Some(height) = self.height {
            check_height(height)?;
        }
        if let Some(turn) = self.turn {
            let measured = matches!(kind, AnnotationType::Linear | AnnotationType::Ordinate)
                && matches!(self.along, Some(Along::Horizontal | Along::Vertical));
            if !measured {
                return Err(
                    "only a horizontal or vertical dimension turns with its view".to_owned(),
                );
            }
            if !turn.is_finite() {
                return Err("a turn is a number of degrees".to_owned());
            }
        }
        if let Some(from) = &self.kept_from
            && (from.trim().is_empty() || from.trim() != from.as_str())
        {
            return Err("a model annotation is named without spaces around".to_owned());
        }
        Ok(())
    }
}

/// Why letters cannot be this high, if they cannot (mitcad#729).
#[doc(hidden)]
pub fn check_height(height: f64) -> Result<(), String> {
    if height.is_finite() && height > 0.0 && height <= MAX_TEXT_HEIGHT {
        Ok(())
    } else {
        Err(format!(
            "letters are more than 0 and at most {MAX_TEXT_HEIGHT} mm high"
        ))
    }
}

// --- Lines, arcs and texts drawn outside the model (mitcad#729) ------------

/// How wide a line or an arc drawn outside the model is (mitcad#1447):
/// ISO 128's narrow, wide and extra wide lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PenWidth {
    /// Narrow, as dimension lines.
    #[default]
    Thin,
    /// Wide, as visible edges.
    Thick,
    /// Extra wide, twice the wide line (mitcad#1402).
    ExtraThick,
}

/// How a line or an arc drawn outside the model is broken (mitcad#1447).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PenPattern {
    /// Unbroken.
    #[default]
    Continuous,
    /// Dashed, as hidden edges.
    Dashed,
    /// Long dash dotted, as centre lines.
    Chain,
    /// Long dash double dotted (ISO 128 type 05, the phantom line,
    /// mitcad#1546), as adjacent parts and other positions.
    Phantom,
}

/// The pen of a line or an arc drawn on a sheet or in a view: any width
/// with any pattern (mitcad#1447; one pen of five before).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SketchPen {
    pub width: PenWidth,
    pub pattern: PenPattern,
}

/// A pen by the one name commands and files gave it before mitcad#1447
/// (`pen`), still taken, and still written where it says exactly what the
/// pen is: `thin`, `thick` and `extra_thick` continuous, `hidden` thin and
/// dashed, `chain` thin and long dash dotted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchPenName {
    Thin,
    Thick,
    ExtraThick,
    Hidden,
    Chain,
}

impl From<SketchPenName> for SketchPen {
    fn from(name: SketchPenName) -> Self {
        let (width, pattern) = match name {
            SketchPenName::Thin => (PenWidth::Thin, PenPattern::Continuous),
            SketchPenName::Thick => (PenWidth::Thick, PenPattern::Continuous),
            SketchPenName::ExtraThick => (PenWidth::ExtraThick, PenPattern::Continuous),
            SketchPenName::Hidden => (PenWidth::Thin, PenPattern::Dashed),
            SketchPenName::Chain => (PenWidth::Thin, PenPattern::Chain),
        };
        Self { width, pattern }
    }
}

impl SketchPen {
    pub const fn new(width: PenWidth, pattern: PenPattern) -> Self {
        Self { width, pattern }
    }

    /// Its name from before mitcad#1447, when one says exactly what it is.
    pub fn name(self) -> Option<SketchPenName> {
        match (self.width, self.pattern) {
            (PenWidth::Thin, PenPattern::Continuous) => Some(SketchPenName::Thin),
            (PenWidth::Thick, PenPattern::Continuous) => Some(SketchPenName::Thick),
            (PenWidth::ExtraThick, PenPattern::Continuous) => Some(SketchPenName::ExtraThick),
            (PenWidth::Thin, PenPattern::Dashed) => Some(SketchPenName::Hidden),
            (PenWidth::Thin, PenPattern::Chain) => Some(SketchPenName::Chain),
            (PenWidth::Thick | PenWidth::ExtraThick, PenPattern::Dashed | PenPattern::Chain)
            | (_, PenPattern::Phantom) => None,
        }
    }

    /// This pen changed as a command or a file gives it: a `pen` name sets
    /// both parts, then `width` and `pattern` set their own.
    pub fn chosen(
        self,
        name: Option<SketchPenName>,
        width: Option<PenWidth>,
        pattern: Option<PenPattern>,
    ) -> Self {
        let mut pen = name.map_or(self, Self::from);
        if let Some(width) = width {
            pen.width = width;
        }
        if let Some(pattern) = pattern {
            pen.pattern = pattern;
        }
        pen
    }

    /// Its fields in a file: none for the default thin continuous pen, its
    /// old `pen` name where one says exactly what it is (so that a file of
    /// those pens is written as before mitcad#1447), else `width` and
    /// `pattern`.
    fn file_fields(self) -> (Option<SketchPenName>, Option<PenWidth>, Option<PenPattern>) {
        match self.name() {
            Some(SketchPenName::Thin) => (None, None, None),
            Some(name) => (Some(name), None, None),
            None => (None, Some(self.width), Some(self.pattern)),
        }
    }
}

/// A line, an arc or a text drawn on a sheet (sheet mm) or in a view (the
/// view's coordinates, model mm, so that it moves, scales and turns with
/// the view), outside the model: nothing of the model is picked, and it
/// follows nothing but its view. A text on a sheet is a [`SheetNote`].
/// Its id is one of the drawing's annotation ids. Its file form:
/// `SketchItemFile` below.
#[derive(Debug, Clone, PartialEq)]
pub enum SketchItem {
    Line {
        id: u32,
        from: P2,
        to: P2,
        pen: SketchPen,
    },
    /// Counter-clockwise from `start` to `end` (radians; `end` - `start`
    /// up to a whole turn).
    Arc {
        id: u32,
        center: P2,
        radius: f64,
        start: f64,
        end: f64,
        pen: SketchPen,
    },
    /// Lines split at `\n`, the first one starting at `at`.
    Text {
        id: u32,
        text: String,
        at: P2,
        /// Its letters' height, paper mm; the drawing's when left out.
        height: Option<f64>,
    },
}

/// A [`SketchItem`] as files and the `drawing` query hold it: a line's or
/// an arc's pen left out when thin and continuous, by its old `pen` name
/// where one says exactly what it is (files of those pens are written as
/// before mitcad#1447), else as `width` and `pattern`. Read: `pen` sets
/// both, then `width` and `pattern` their own.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum SketchItemFile {
    Line {
        id: u32,
        from: P2,
        to: P2,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pen: Option<SketchPenName>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<PenWidth>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<PenPattern>,
    },
    Arc {
        id: u32,
        center: P2,
        radius: f64,
        start: f64,
        end: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pen: Option<SketchPenName>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<PenWidth>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pattern: Option<PenPattern>,
    },
    Text {
        id: u32,
        text: String,
        at: P2,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
    },
}

impl From<&SketchItem> for SketchItemFile {
    fn from(item: &SketchItem) -> Self {
        match item {
            SketchItem::Line { id, from, to, pen } => {
                let (name, width, pattern) = pen.file_fields();
                Self::Line {
                    id: *id,
                    from: *from,
                    to: *to,
                    pen: name,
                    width,
                    pattern,
                }
            }
            SketchItem::Arc {
                id,
                center,
                radius,
                start,
                end,
                pen,
            } => {
                let (name, width, pattern) = pen.file_fields();
                Self::Arc {
                    id: *id,
                    center: *center,
                    radius: *radius,
                    start: *start,
                    end: *end,
                    pen: name,
                    width,
                    pattern,
                }
            }
            SketchItem::Text {
                id,
                text,
                at,
                height,
            } => Self::Text {
                id: *id,
                text: text.clone(),
                at: *at,
                height: *height,
            },
        }
    }
}

impl From<SketchItemFile> for SketchItem {
    fn from(file: SketchItemFile) -> Self {
        match file {
            SketchItemFile::Line {
                id,
                from,
                to,
                pen,
                width,
                pattern,
            } => Self::Line {
                id,
                from,
                to,
                pen: SketchPen::default().chosen(pen, width, pattern),
            },
            SketchItemFile::Arc {
                id,
                center,
                radius,
                start,
                end,
                pen,
                width,
                pattern,
            } => Self::Arc {
                id,
                center,
                radius,
                start,
                end,
                pen: SketchPen::default().chosen(pen, width, pattern),
            },
            SketchItemFile::Text {
                id,
                text,
                at,
                height,
            } => Self::Text {
                id,
                text,
                at,
                height,
            },
        }
    }
}

impl Serialize for SketchItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SketchItemFile::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SketchItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        SketchItemFile::deserialize(deserializer).map(Self::from)
    }
}

impl SketchItem {
    pub fn id(&self) -> u32 {
        match self {
            Self::Line { id, .. } | Self::Arc { id, .. } | Self::Text { id, .. } => *id,
        }
    }

    /// Its name in commands, files and undo steps.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Line { .. } => "line",
            Self::Arc { .. } => "arc",
            Self::Text { .. } => "text",
        }
    }

    /// Its words in the `drawing` query: a text's lines on one line, else
    /// its kind.
    pub fn label(&self) -> String {
        match self {
            Self::Text { text, .. } => text.replace('\n', " / "),
            other => other.key().to_owned(),
        }
    }

    /// Why it cannot be kept, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        let at = format!("{} {}", self.key(), self.id());
        let point = |p: &P2| p.iter().all(|v| v.is_finite());
        let why = match self {
            Self::Line { from, to, .. } => {
                if !(point(from) && point(to)) {
                    Some("its ends are points".to_owned())
                } else if len2(sub2(*to, *from)) < 1e-9 {
                    Some("a line has a length".to_owned())
                } else {
                    None
                }
            }
            Self::Arc {
                center,
                radius,
                start,
                end,
                ..
            } => {
                if !(point(center) && radius.is_finite() && *radius > 0.0) {
                    Some("an arc is about a point, of some size".to_owned())
                } else if !(start.is_finite()
                    && end.is_finite()
                    && end > start
                    && end - start <= TAU + 1e-9)
                {
                    Some("an arc turns from its start to its end, a whole turn at most".to_owned())
                } else {
                    None
                }
            }
            Self::Text {
                text, at, height, ..
            } => {
                if text.trim().is_empty() {
                    Some("a text needs a text".to_owned())
                } else if !point(at) {
                    Some("its place is not a point".to_owned())
                } else {
                    height.and_then(|h| check_height(h).err())
                }
            }
        };
        match why {
            Some(why) => Err(format!("{at}: {why}")),
            None if self.id() == 0 => Err("annotation ids start from 1".to_owned()),
            None => Ok(()),
        }
    }

    /// Moved by `f` (a point of its coordinates to its new place), its
    /// arc's angles turned by `turn` radians and its radius scaled by
    /// `scale`: carried to other coordinates.
    pub fn carried(&mut self, f: &dyn Fn(P2) -> P2, turn: f64, scale: f64) {
        match self {
            Self::Line { from, to, .. } => {
                *from = f(*from);
                *to = f(*to);
            }
            Self::Arc {
                center,
                radius,
                start,
                end,
                ..
            } => {
                *center = f(*center);
                *radius *= scale;
                let sweep = *end - *start;
                *start = (*start + turn).rem_euclid(TAU);
                *end = *start + sweep;
            }
            Self::Text { at, .. } => *at = f(*at),
        }
    }
}
