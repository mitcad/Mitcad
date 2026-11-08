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
}

impl AnnotationType {
    pub const ALL: [Self; 16] = [
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
            Self::Angle | Self::EdgeExtension => Want::Line,
            Self::Radius | Self::Diameter | Self::CenterMark | Self::CenterPattern => Want::Circle,
            Self::Datum => Want::Datum,
            _ => Want::Any,
        }
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
}

impl ViewPick {
    /// The occurrence path it names, empty for none.
    pub fn path(&self) -> &[crate::ids::OccurrenceUid] {
        self.occurrence.as_ref().map_or(&[], |p| p.0.as_slice())
    }

    fn check(&self) -> Result<(), String> {
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
    /// finish symbol from the arrow's point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_offset: Option<P2>,
    /// Where on the view (view mm) a leader's pick was clicked: its arrow
    /// points at the pick's point nearest it.
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
            table: None,
            height: None,
            turn: None,
            tip_along: None,
            terminator: None,
            kept_from: None,
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
        if kind.is_dimension() {
            crate::document::check_format(&self.format)?;
            if let Some(tolerance) = &self.tolerance {
                crate::document::check_tolerance(tolerance, kind.measure_kind())?;
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
            (PenWidth::Thick | PenWidth::ExtraThick, PenPattern::Dashed | PenPattern::Chain) => {
                None
            }
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
