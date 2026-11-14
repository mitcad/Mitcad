// SPDX-License-Identifier: MIT
//! Annotations (mitcad#702), grown from the measurement objects of
//! mitcad#649: dimensions and leader notes kept in the document, managed in
//! the application's Annotations window, drawn in their planes in the view
//! and saved in the design file. Each has a name (`Distance1`, `Note1`,
//! ...), a colour, a light bulb, an optional group, the kind of value it
//! shows (or `note`) and one or two picks.
//!
//! A dimension shows a measured value (ISO 129 style, millimetres): its
//! prefix (Ø, R), decimals and own text before or after it or in its place
//! ([`AnnotationFormat`]) and a tolerance ([`Tolerance`]: symmetric,
//! deviations, limits or an ISO 286 fit code, `crate::iso286`). A leader
//! note is a text on an arrow to one pick.
//!
//! A pick refers to geometry as the `measure` query's selections do: a
//! body with a face, edge or vertex by its topological name, or a datum,
//! each with the occurrence that places it. The value is worked out at the
//! timeline marker whenever it is asked for (`model-api/src/measurements.rs`,
//! `model-api/src/annotations.rs`), so it follows the model; a pick that is gone
//! makes the annotation broken, and the query says why instead of giving a
//! value.
//!
//! Where it is drawn is kept relative to its picks: the plane it lies in
//! ([`AnnotationPlane`]: a direction, or a planar face or datum plane
//! picked), the dimension line's distance from the measured ends
//! (`line_offset`) and the text's shift from its default place
//! (`text_offset`), in millimetres in that plane.
//!
//! They change no geometry: adding, editing and deleting one is an undo
//! step that recomputes nothing.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Document, ModelError, invalid};
use crate::datum::Vec3;
use crate::features::OccurrencePath;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::topo::{EdgeName, FaceName, VertexName};

/// The colour a new annotation takes (mitcad#894): no colour of its own,
/// the neutral annotation colour that follows the view's background tone
/// (near-black on a light background, light grey on a dark one), drawn as
/// a technical drawing's dimensions are.
pub const AUTO_COLOR: &str = "auto";

/// The six colours new annotations took in turn before mitcad#894 (sRGB).
/// The program chose them, not the user: an annotation stored in a design
/// file with one of them is read as [`AUTO_COLOR`] ([`read_old_defaults`]).
pub const OLD_DEFAULT_COLORS: [&str; 6] = [
    "#d9480f", // orange
    "#1971c2", // blue
    "#2f9e44", // green
    "#c2255c", // magenta
    "#7048e8", // violet
    "#e67700", // amber
];

/// The ISO 3098 series of text heights (mm) that the automatic height of
/// annotations' texts takes from (mitcad#894).
pub const TEXT_HEIGHTS: [f64; 5] = [2.5, 3.5, 5.0, 7.0, 10.0];

/// The heights (mm) a design's annotations' texts may be set to.
pub const TEXT_HEIGHT_RANGE: (f64, f64) = (0.1, 1000.0);

/// Annotations read from a design file whose colour is one of the
/// [`OLD_DEFAULT_COLORS`] take [`AUTO_COLOR`] (mitcad#894): the rotating
/// defaults are gone, and the program, not the user, chose those.
pub(crate) fn read_old_defaults(annotations: &mut [Annotation]) {
    for annotation in annotations {
        if OLD_DEFAULT_COLORS.contains(&annotation.color.as_str()) {
            AUTO_COLOR.clone_into(&mut annotation.color);
        }
    }
}

/// The most decimals a dimension shows.
pub const MAX_DECIMALS: u8 = 6;

/// An annotation kept in the document: a dimension or a leader note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub name: String,
    /// The value it shows, or `note`.
    pub kind: MeasureKind,
    /// `#rrggbb`, lower case, or [`AUTO_COLOR`].
    pub color: String,
    /// Drawn in the view; left out of files when true.
    #[serde(default = "shown", skip_serializing_if = "is_shown")]
    pub visible: bool,
    /// The group it belongs to (`Manufacturing`), shown and hidden
    /// together in the application; none when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub a: MeasurePick,
    /// The second pick: every dimension but a radius or a diameter has
    /// one; a note has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b: Option<MeasurePick>,
    /// The plane it lies in; chosen from its picks when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<AnnotationPlane>,
    /// A length's dimension line from its ends, or an angle's arc radius,
    /// mm (signed for a length: across the ends in the plane); the
    /// default when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_offset: Option<f64>,
    /// The text's shift from its default place, mm along and across the
    /// dimension line in its plane; a note's text from the arrow's point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_offset: Option<[f64; 2]>,
    /// How a dimension's value is written.
    #[serde(default, skip_serializing_if = "AnnotationFormat::is_default")]
    pub format: AnnotationFormat,
    /// A dimension's tolerance; none when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<Tolerance>,
    /// A note's text, lines split at `\n`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A note's point as it was picked, in its pick's own (component's)
    /// coordinates, placed by the pick's occurrence: the arrow points at
    /// the point of the pick nearest it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Vec3>,
    /// A datum label's letter (`A`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter: Option<String>,
    /// A feature control frame's geometric tolerance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<GeometricTolerance>,
    /// The dimension (its name) a feature control frame stands under,
    /// instead of a leader to its pick.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
}

/// A geometric characteristic of ISO 1101 and its symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Characteristic {
    Straightness,
    Flatness,
    Circularity,
    Cylindricity,
    LineProfile,
    SurfaceProfile,
    Parallelism,
    Perpendicularity,
    Angularity,
    Position,
    Concentricity,
    Symmetry,
    CircularRunout,
    TotalRunout,
}

/// Whether a characteristic's tolerance refers to datums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumNeed {
    /// A form tolerance: none.
    None,
    /// Either way (a profile, a position).
    Optional,
    /// Orientation, coaxiality, symmetry, runout: at least one.
    Required,
}

impl Characteristic {
    pub const ALL: [Self; 14] = [
        Self::Straightness,
        Self::Flatness,
        Self::Circularity,
        Self::Cylindricity,
        Self::LineProfile,
        Self::SurfaceProfile,
        Self::Parallelism,
        Self::Perpendicularity,
        Self::Angularity,
        Self::Position,
        Self::Concentricity,
        Self::Symmetry,
        Self::CircularRunout,
        Self::TotalRunout,
    ];

    /// Its name in commands and files.
    pub fn key(self) -> &'static str {
        match self {
            Self::Straightness => "straightness",
            Self::Flatness => "flatness",
            Self::Circularity => "circularity",
            Self::Cylindricity => "cylindricity",
            Self::LineProfile => "line_profile",
            Self::SurfaceProfile => "surface_profile",
            Self::Parallelism => "parallelism",
            Self::Perpendicularity => "perpendicularity",
            Self::Angularity => "angularity",
            Self::Position => "position",
            Self::Concentricity => "concentricity",
            Self::Symmetry => "symmetry",
            Self::CircularRunout => "circular_runout",
            Self::TotalRunout => "total_runout",
        }
    }

    /// Its ISO 1101 symbol.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Straightness => "⏤",
            Self::Flatness => "⏥",
            Self::Circularity => "○",
            Self::Cylindricity => "⌭",
            Self::LineProfile => "⌒",
            Self::SurfaceProfile => "⌓",
            Self::Parallelism => "∥",
            Self::Perpendicularity => "⟂",
            Self::Angularity => "∠",
            Self::Position => "⌖",
            Self::Concentricity => "◎",
            Self::Symmetry => "⌯",
            Self::CircularRunout => "↗",
            Self::TotalRunout => "⌰",
        }
    }

    pub fn datums(self) -> DatumNeed {
        match self {
            Self::Straightness | Self::Flatness | Self::Circularity | Self::Cylindricity => {
                DatumNeed::None
            }
            Self::LineProfile | Self::SurfaceProfile | Self::Position => DatumNeed::Optional,
            _ => DatumNeed::Required,
        }
    }

    /// Whether its tolerance zone may be a cylinder (Ø): of an axis.
    pub fn takes_diameter(self) -> bool {
        matches!(
            self,
            Self::Straightness
                | Self::Parallelism
                | Self::Perpendicularity
                | Self::Angularity
                | Self::Position
                | Self::Concentricity
        )
    }

    /// Whether it takes a material condition (Ⓜ, Ⓛ): of a feature of size.
    pub fn takes_modifier(self) -> bool {
        matches!(
            self,
            Self::Straightness
                | Self::Parallelism
                | Self::Perpendicularity
                | Self::Angularity
                | Self::Position
                | Self::Concentricity
                | Self::Symmetry
        )
    }
}

/// A material condition modifier of a geometric tolerance (ISO 2692).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaterialCondition {
    /// Maximum material requirement, Ⓜ.
    M,
    /// Least material requirement, Ⓛ.
    L,
}

impl MaterialCondition {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::M => "Ⓜ",
            Self::L => "Ⓛ",
        }
    }
}

/// A feature control frame's tolerance (ISO 1101): the characteristic, the
/// zone's width (mm; `diameter` for a cylindrical zone, Ø), a material
/// condition and the datums referred to, in order of precedence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometricTolerance {
    pub characteristic: Characteristic,
    pub value: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub diameter: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier: Option<MaterialCondition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datums: Vec<String>,
}

impl GeometricTolerance {
    fn check(&self) -> Result<(), String> {
        let name = self.characteristic.key().replace('_', " ");
        if !(self.value.is_finite() && self.value > 0.0) {
            return Err("a geometric tolerance is more than 0".to_owned());
        }
        if self.diameter && !self.characteristic.takes_diameter() {
            return Err(format!("a {name} zone is no cylinder (Ø)"));
        }
        if self.modifier.is_some() && !self.characteristic.takes_modifier() {
            return Err(format!("a {name} takes no material condition"));
        }
        if self.datums.len() > 3 {
            return Err("a frame refers to three datums at most".to_owned());
        }
        for (i, datum) in self.datums.iter().enumerate() {
            datum_letter(datum)?;
            if self.datums[..i].contains(datum) {
                return Err(format!("datum {datum} is referred to twice"));
            }
        }
        match (self.characteristic.datums(), self.datums.is_empty()) {
            (DatumNeed::None, false) => Err(format!("a {name} refers to no datum")),
            (DatumNeed::Required, true) => Err(format!("a {name} refers to a datum")),
            _ => Ok(()),
        }
    }
}

/// Frames whose `on` names no dimension or hole callout (none of that
/// name, or a note, a datum or a frame) stand on their leaders again, as
/// when their dimension is deleted: a later dimension of that name does not
/// take them.
pub(crate) fn settle_frames(annotations: &mut [Annotation]) {
    let dimensions: Vec<String> = annotations
        .iter()
        .filter(|a| a.kind.takes_frame())
        .map(|a| a.name.clone())
        .collect();
    for annotation in annotations.iter_mut() {
        if annotation
            .on
            .as_ref()
            .is_some_and(|on| !dimensions.contains(on))
        {
            annotation.on = None;
        }
    }
}

/// A datum's letter as ISO 5459 has them, or why the text is none: one or
/// two capitals, I, O and Q left out.
pub fn datum_letter(text: &str) -> Result<String, String> {
    let letter = text.trim();
    let valid = (1..=2).contains(&letter.len())
        && letter
            .chars()
            .all(|c| c.is_ascii_uppercase() && !matches!(c, 'I' | 'O' | 'Q'));
    if valid {
        Ok(letter.to_owned())
    } else {
        Err(format!(
            "a datum is one or two capital letters but I, O and Q, got “{letter}”"
        ))
    }
}

/// The kept measurements of mitcad#649 are annotations.
pub type Measurement = Annotation;

fn shown() -> bool {
    true
}

fn is_shown(visible: &bool) -> bool {
    *visible
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// What an annotation shows: of two picks their closest distance, the
/// angle between them, the offset of two parallel planes, the distance of
/// axes, or of two circles' centres (mitcad#646, mitcad#647); of one pick
/// with a circle (a round edge or face) its radius or diameter; or a
/// leader note's text on one pick; a datum label (ISO 5459) or a
/// feature control frame (ISO 1101) on one pick (mitcad#702, phase 2); a
/// hole callout on a hole's round face or edge, its text from the feature
/// that made the hole (mitcad#902); a material note or a process note on a
/// body, its text the body's material or its print settings (mitcad#1049).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasureKind {
    Distance,
    Angle,
    Offset,
    AxisDistance,
    CenterDistance,
    Radius,
    Diameter,
    Note,
    Datum,
    Frame,
    HoleCallout,
    /// A note of the body's material, written from the design (mitcad#1049).
    Material,
    /// A note of the body's print settings: its printer and process
    /// profiles and the values set on the body, its occurrences or its
    /// components (mitcad#1049).
    ProcessNote,
}

impl MeasureKind {
    /// The kinds of dimension, each a value its picks have.
    pub const ALL: [Self; 7] = [
        Self::Distance,
        Self::Angle,
        Self::Offset,
        Self::AxisDistance,
        Self::CenterDistance,
        Self::Radius,
        Self::Diameter,
    ];

    /// Its name in commands and files (`axis_distance`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Distance => "distance",
            Self::Angle => "angle",
            Self::Offset => "offset",
            Self::AxisDistance => "axis_distance",
            Self::CenterDistance => "center_distance",
            Self::Radius => "radius",
            Self::Diameter => "diameter",
            Self::Note => "note",
            Self::Datum => "datum",
            Self::Frame => "frame",
            Self::HoleCallout => "hole_callout",
            Self::Material => "material",
            Self::ProcessNote => "process_note",
        }
    }

    /// The first part of default names (`Distance1`, `AxisDistance1`).
    pub fn name_stem(self) -> &'static str {
        match self {
            Self::Distance => "Distance",
            Self::Angle => "Angle",
            Self::Offset => "Offset",
            Self::AxisDistance => "AxisDistance",
            Self::CenterDistance => "CenterDistance",
            Self::Radius => "Radius",
            Self::Diameter => "Diameter",
            Self::Note => "Note",
            Self::Datum => "Datum",
            Self::Frame => "Frame",
            Self::HoleCallout => "HoleCallout",
            Self::Material => "Material",
            Self::ProcessNote => "ProcessNote",
        }
    }

    /// Whether it is on one pick (a radius, a diameter, a note) rather
    /// than between two.
    pub fn single(self) -> bool {
        matches!(
            self,
            Self::Radius
                | Self::Diameter
                | Self::Note
                | Self::Datum
                | Self::Frame
                | Self::HoleCallout
                | Self::Material
                | Self::ProcessNote
        )
    }

    /// Whether its value is an angle (radians) rather than a length (mm).
    pub fn is_angle(self) -> bool {
        self == Self::Angle
    }

    /// Whether it is a leader note rather than a dimension.
    pub fn is_note(self) -> bool {
        self == Self::Note
    }

    /// Whether it is drawn on a leader to its pick (a note, a datum label,
    /// a feature control frame, a hole callout, a material or process
    /// note) rather than measured: no value of its own.
    pub fn is_leader(self) -> bool {
        matches!(
            self,
            Self::Note
                | Self::Datum
                | Self::Frame
                | Self::HoleCallout
                | Self::Material
                | Self::ProcessNote
        )
    }

    /// Whether its text is written from the body it points at, as the
    /// design has it now (mitcad#1049): a material note, a process note.
    pub fn is_body_note(self) -> bool {
        matches!(self, Self::Material | Self::ProcessNote)
    }

    /// Whether a feature control frame can stand under it (`on`): a
    /// dimension, or a hole callout on its leader (mitcad#902).
    pub fn takes_frame(self) -> bool {
        !self.is_leader() || self == Self::HoleCallout
    }

    /// Whether its value is a length (every dimension but an angle).
    pub fn is_length(self) -> bool {
        !self.is_angle() && !self.is_leader()
    }

    /// The prefix its value has unless the format says otherwise.
    pub fn default_prefix(self) -> &'static str {
        match self {
            Self::Diameter => "Ø",
            Self::Radius => "R",
            _ => "",
        }
    }
}

/// A pick of an annotation, as the `measure` query takes a selection given
/// as fields: a body with an optional face, edge or vertex, or a datum
/// (`F5`, `xy`), each with the occurrence that places it in the design
/// (a path from the root, `"O1/O4"`; left out in the root, or for the
/// owner's only placement).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasurePick {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<FaceName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<EdgeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertex: Option<VertexName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
}

impl MeasurePick {
    /// The features the pick names (mitcad#1756): its body's maker, the
    /// features in its face's, edge's or vertex's name, and its datum when
    /// that is a feature (`F5`; not the origin's planes and axes, `xy`).
    pub fn features(&self) -> BTreeSet<FeatureUid> {
        let mut features = BTreeSet::new();
        if let Some(body) = self.body {
            features.insert(body.feature);
        }
        if let Some(face) = &self.face {
            features.extend(face.features());
        }
        if let Some(edge) = &self.edge {
            features.extend(edge.features());
        }
        if let Some(vertex) = &self.vertex {
            for face in vertex.faces() {
                features.extend(face.features());
            }
        }
        if let Some(uid) = self
            .datum
            .as_deref()
            .and_then(|datum| datum.trim().parse::<FeatureUid>().ok())
        {
            features.insert(uid);
        }
        features
    }

    /// Why the pick is no pick, if it is not one (not its geometry).
    fn check(&self) -> Result<(), String> {
        let names = [
            self.face.is_some(),
            self.edge.is_some(),
            self.vertex.is_some(),
        ]
        .iter()
        .filter(|named| **named)
        .count();
        match (&self.body, &self.datum) {
            (Some(_), None) if names <= 1 => Ok(()),
            (None, Some(datum)) if names == 0 && !datum.trim().is_empty() => Ok(()),
            _ => {
                Err("a pick is a body with an optional face, edge or vertex, or a datum".to_owned())
            }
        }
    }
}

/// The plane an annotation lies in: `normal`, a direction its plane is
/// square to (turned square to a length's ends; `x`, for a note, the
/// direction its text runs along), both in its first pick's own
/// coordinates so that they turn with its occurrence; or `pick`, a planar
/// face or a datum plane its plane is parallel to, which it follows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationPlane {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick: Option<MeasurePick>,
}

impl AnnotationPlane {
    fn check(&self) -> Result<(), String> {
        match (&self.normal, &self.pick) {
            (Some(normal), None) => {
                if mitcad_vec3::unit(*normal).is_none() {
                    return Err("a plane's normal is a direction, not zero".to_owned());
                }
                if let Some(x) = &self.x {
                    let n = mitcad_vec3::unit(*normal).unwrap_or([0.0, 0.0, 1.0]);
                    if x.iter().any(|c| !c.is_finite())
                        || mitcad_vec3::unit(mitcad_vec3::cross(n, *x)).is_none()
                    {
                        return Err("a plane's x is a direction in it".to_owned());
                    }
                }
                Ok(())
            }
            (None, Some(pick)) if self.x.is_none() => pick.check(),
            _ => Err("a plane is a normal (with an optional x) or a pick".to_owned()),
        }
    }
}

/// How a dimension's value is written: `decimals` (up to three, trailing
/// zeros left out, when none), a `prefix` in place of its kind's (Ø, R;
/// `""` for none), text `before` and `after` it, or a text that `replaces`
/// the value and its tolerance.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationFormat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub before: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>,
}

impl AnnotationFormat {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    fn check(&self) -> Result<(), String> {
        match self.decimals {
            Some(d) if d > MAX_DECIMALS => {
                Err(format!("a dimension shows at most {MAX_DECIMALS} decimals"))
            }
            _ => Ok(()),
        }
    }
}

/// A dimension's tolerance, in millimetres for lengths and degrees for
/// angles: `symmetric` ±`value`; `deviations` from the value, `upper` over
/// `lower` (signed); `limits`, the same deviations shown as the largest
/// and the smallest size; an ISO 286 `fit` code (`H7` a hole's, `g6` a
/// shaft's) on a length, with its deviations shown in brackets when
/// `deviations` is set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Tolerance {
    Symmetric {
        value: f64,
    },
    Deviations {
        upper: f64,
        lower: f64,
    },
    Limits {
        upper: f64,
        lower: f64,
    },
    Fit {
        code: String,
        #[serde(default, skip_serializing_if = "is_false")]
        deviations: bool,
    },
}

impl Tolerance {
    /// Its name in commands and files.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Symmetric { .. } => "symmetric",
            Self::Deviations { .. } => "deviations",
            Self::Limits { .. } => "limits",
            Self::Fit { .. } => "fit",
        }
    }

    fn check(&self, kind: MeasureKind) -> Result<(), String> {
        let finite = |v: f64| {
            if v.is_finite() {
                Ok(())
            } else {
                Err("a tolerance is a number".to_owned())
            }
        };
        match self {
            Self::Symmetric { value } => {
                finite(*value)?;
                if *value > 0.0 {
                    Ok(())
                } else {
                    Err("a symmetric tolerance is more than 0".to_owned())
                }
            }
            Self::Deviations { upper, lower } | Self::Limits { upper, lower } => {
                finite(*upper)?;
                finite(*lower)?;
                if upper > lower {
                    Ok(())
                } else {
                    Err("the upper deviation is more than the lower".to_owned())
                }
            }
            Self::Fit { code, .. } => {
                if !kind.is_length() {
                    return Err("a fit code is a length's tolerance".to_owned());
                }
                crate::iso286::FitCode::parse(code).map(|_| ())
            }
        }
    }
}

/// Why a value's format cannot be kept, if it cannot: the checks of the
/// annotations of the model, for those of drawings' views too (mitcad#707).
pub(crate) fn check_format(format: &AnnotationFormat) -> Result<(), String> {
    format.check()
}

/// Why a tolerance cannot be kept on a dimension of this kind.
pub(crate) fn check_tolerance(tolerance: &Tolerance, kind: MeasureKind) -> Result<(), String> {
    tolerance.check(kind)
}

/// Why a feature control frame's tolerance cannot be kept.
pub(crate) fn check_frame(frame: &GeometricTolerance) -> Result<(), String> {
    frame.check()
}

/// An annotation's colour as `#rrggbb` in lower case or `auto` (any case,
/// [`AUTO_COLOR`]), or why the text is not one.
pub fn normalized_color(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.eq_ignore_ascii_case(AUTO_COLOR) {
        return Ok(AUTO_COLOR.to_owned());
    }
    let hex = text.strip_prefix('#').unwrap_or("");
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(format!("#{}", hex.to_ascii_lowercase()))
    } else {
        Err(format!("a color is \"#rrggbb\" or \"auto\", got “{text}”"))
    }
}

/// Why a text height (mm) of a design's annotations cannot be set, if it
/// cannot: a number from 0.1 to 1000.
pub fn check_text_height(height: f64) -> Result<(), String> {
    let (least, most) = TEXT_HEIGHT_RANGE;
    if height.is_finite() && (least..=most).contains(&height) {
        Ok(())
    } else {
        Err(format!(
            "an annotation text height is {least} to {most} mm, got {height}"
        ))
    }
}

/// The automatic text height (mm) of a model of this size (the diagonal of
/// what is annotated, mm; mitcad#894): of the ISO 3098 series
/// ([`TEXT_HEIGHTS`]), the largest at most a fiftieth of the size, at least
/// the smallest; 3.5 mm when the size is not known.
pub fn automatic_text_height(size: Option<f64>) -> f64 {
    let Some(size) = size.filter(|s| s.is_finite() && *s > 0.0) else {
        return TEXT_HEIGHTS[1];
    };
    TEXT_HEIGHTS
        .iter()
        .rev()
        .copied()
        .find(|h| *h <= size / 50.0 + 1e-9)
        .unwrap_or(TEXT_HEIGHTS[0])
}

/// A material or process note in words, as refusals name it.
pub fn body_note_words(kind: MeasureKind) -> &'static str {
    match kind {
        MeasureKind::ProcessNote => "process note",
        _ => "material note",
    }
}

/// A group's name trimmed, none when empty.
pub(crate) fn normalized_group(group: Option<&str>) -> Option<String> {
    group
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_owned)
}

impl Annotation {
    /// A dimension or a note of these picks with every other field left
    /// to its default: shown, in no group, in the plane its picks give,
    /// its value as it is.
    pub fn new(name: &str, kind: MeasureKind, color: &str, a: MeasurePick) -> Self {
        Self {
            name: name.to_owned(),
            kind,
            color: color.to_owned(),
            visible: true,
            group: None,
            a,
            b: None,
            plane: None,
            line_offset: None,
            text_offset: None,
            format: AnnotationFormat::default(),
            tolerance: None,
            text: None,
            at: None,
            letter: None,
            frame: None,
            on: None,
        }
    }

    /// Whether it has only what a measurement object of mitcad#649 had (a
    /// dimension's name, kind, colour, light bulb and picks): such ones are
    /// still written as `measurements`, which older versions read.
    pub fn is_measurement_form(&self) -> bool {
        !self.kind.is_leader()
            && self.group.is_none()
            && self.plane.is_none()
            && self.line_offset.is_none()
            && self.text_offset.is_none()
            && self.format.is_default()
            && self.tolerance.is_none()
            && self.text.is_none()
            && self.at.is_none()
            && self.letter.is_none()
            && self.frame.is_none()
            && self.on.is_none()
    }

    /// Every pick it refers to: `a`, `b` and its plane's.
    pub fn picks(&self) -> impl Iterator<Item = &MeasurePick> {
        [
            Some(&self.a),
            self.b.as_ref(),
            self.plane.as_ref().and_then(|p| p.pick.as_ref()),
        ]
        .into_iter()
        .flatten()
    }

    /// Why the annotation cannot be kept, if it cannot (not its geometry).
    pub(crate) fn check(&self) -> Result<(), String> {
        self.check_fields()
            .map_err(|e| format!("{}: {e}", self.name))
    }

    /// A frame's datum letters and the dimension it stands under, trimmed
    /// (` A` is `A`), as they are checked and stored.
    fn normalize_frame(&mut self) {
        if let Some(frame) = &mut self.frame {
            for datum in &mut frame.datums {
                *datum = datum.trim().to_owned();
            }
        }
        if let Some(on) = &mut self.on {
            *on = on.trim().to_owned();
        }
    }

    fn check_fields(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the annotation needs a name".to_owned());
        }
        let color = normalized_color(&self.color)?;
        if color != self.color {
            return Err(format!("the color is written \"{color}\""));
        }
        if let Some(group) = &self.group
            && normalized_group(Some(group.as_str())).as_deref() != Some(group.as_str())
        {
            return Err("a group has a name without spaces around it".to_owned());
        }
        self.a.check()?;
        if self.kind.is_body_note() && self.a.body.is_none() {
            return Err(format!("a {} points at a body", body_note_words(self.kind)));
        }
        match (&self.b, self.kind.single()) {
            (Some(b), false) => b.check()?,
            (None, true) => {}
            (Some(_), true) => {
                return Err(if self.kind.is_leader() {
                    format!("a {} points at one pick", self.kind.key())
                } else {
                    format!("a {} is measured on one pick", self.kind.key())
                });
            }
            (None, false) => {
                return Err(format!(
                    "a {} is measured between two picks",
                    self.kind.key().replace('_', " ")
                ));
            }
        }
        if let Some(plane) = &self.plane {
            // A diameter's plane is its leader's (mitcad#902: added in a
            // view, a leader from outside with its text level there).
            if matches!(self.kind, MeasureKind::Radius | MeasureKind::Angle) {
                return Err(format!(
                    "a {} lies in the plane of its {}",
                    self.kind.key(),
                    if self.kind.is_angle() {
                        "sides"
                    } else {
                        "circle"
                    }
                ));
            }
            plane.check()?;
        }
        if let Some(offset) = self.line_offset {
            if !offset.is_finite() {
                return Err("the line's offset is a number".to_owned());
            }
            match self.kind {
                MeasureKind::Note
                | MeasureKind::Datum
                | MeasureKind::Frame
                | MeasureKind::HoleCallout
                | MeasureKind::Material
                | MeasureKind::ProcessNote
                | MeasureKind::Radius
                | MeasureKind::Diameter => {
                    return Err(format!(
                        "a {} has no dimension line to move",
                        self.kind.key()
                    ));
                }
                MeasureKind::Angle if offset <= 0.0 => {
                    return Err("an angle's arc has a radius more than 0".to_owned());
                }
                _ => {}
            }
        }
        if let Some(offset) = self.text_offset
            && offset.iter().any(|x| !x.is_finite())
        {
            return Err("the text's offset is two numbers".to_owned());
        }
        if let Some(at) = self.at {
            if !self.kind.is_leader() {
                return Err(
                    "only a note, a datum or a frame has a point it was picked at".to_owned(),
                );
            }
            if at.iter().any(|x| !x.is_finite()) {
                return Err("a point is three numbers".to_owned());
            }
        }
        if self.letter.is_some() && self.kind != MeasureKind::Datum {
            return Err("only a datum has a letter".to_owned());
        }
        if (self.frame.is_some() || self.on.is_some()) && self.kind != MeasureKind::Frame {
            return Err("only a feature control frame has a geometric tolerance".to_owned());
        }
        if self.kind.is_leader() {
            if !self.format.is_default() || self.tolerance.is_some() {
                return Err(if self.kind.is_note() {
                    "a note has a text, not a value's format or tolerance".to_owned()
                } else {
                    format!("a {} has no value's format or tolerance", self.kind.key())
                });
            }
            if self.text.is_some() != self.kind.is_note() {
                return Err(if self.kind.is_note() {
                    "a note needs a text".to_owned()
                } else {
                    format!("a {} has no text of its own", self.kind.key())
                });
            }
            match self.kind {
                MeasureKind::Note => {
                    if self.text.as_deref().is_none_or(|t| t.trim().is_empty()) {
                        return Err("a note needs a text".to_owned());
                    }
                }
                MeasureKind::Datum => {
                    let letter = self
                        .letter
                        .as_deref()
                        .ok_or_else(|| "a datum needs its letter".to_owned())?;
                    if datum_letter(letter)? != letter {
                        return Err("a datum's letter is written without spaces".to_owned());
                    }
                }
                // Its text comes from the hole's feature (mitcad#902), a
                // material or process note's from its body (mitcad#1049).
                MeasureKind::HoleCallout | MeasureKind::Material | MeasureKind::ProcessNote => {}
                _ => {
                    self.frame
                        .as_ref()
                        .ok_or_else(|| "a feature control frame needs its tolerance".to_owned())?
                        .check()?;
                    if self.on.as_deref().is_some_and(|on| on.trim().is_empty()) {
                        return Err("a frame stands under a dimension named".to_owned());
                    }
                }
            }
        } else {
            if self.text.is_some() {
                return Err(
                    "a dimension's own text is its format's before, after or replace".to_owned(),
                );
            }
            self.format.check()?;
            if let Some(tolerance) = &self.tolerance {
                tolerance.check(self.kind)?;
            }
        }
        Ok(())
    }
}

/// A measurement to add: what [`Document::add_measurement`] takes once the
/// API has measured it (`model-api/src/measurements.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct NewMeasurement {
    pub kind: MeasureKind,
    pub a: MeasurePick,
    pub b: Option<MeasurePick>,
    /// The next free `<Kind><n>` when none.
    pub name: Option<String>,
    /// [`AUTO_COLOR`] when none.
    pub color: Option<String>,
}

impl<K: Kernel> Document<K> {
    /// The annotations in the order they were added.
    pub fn annotations(&self) -> &[Annotation] {
        &self.state.annotations
    }

    /// The annotations (the kept measurements of mitcad#649).
    pub fn measurements(&self) -> &[Annotation] {
        self.annotations()
    }

    /// The annotation of this name.
    pub fn annotation(&self, name: &str) -> Result<&Annotation, ModelError> {
        Ok(&self.state.annotations[self.annotation_index(name)?])
    }

    /// Whether the annotation's picks name a feature after the timeline
    /// marker (mitcad#1756): its picks, its plane's pick and, for a frame
    /// under a dimension, that dimension's picks. A pick that is not there
    /// while so is rolled back with the marker, as those features are (a
    /// sketch edited rolls the timeline back to it), not broken: it comes
    /// back when the marker does. With the marker at the end nothing is
    /// rolled back.
    pub fn annotation_rolled_back(&self, annotation: &Annotation) -> bool {
        let after: BTreeSet<FeatureUid> = self
            .features()
            .skip(self.marker())
            .map(|feature| feature.uid)
            .collect();
        if after.is_empty() {
            return false;
        }
        let mut picks: Vec<&MeasurePick> = vec![&annotation.a];
        picks.extend(annotation.b.as_ref());
        picks.extend(
            annotation
                .plane
                .as_ref()
                .and_then(|plane| plane.pick.as_ref()),
        );
        if annotation.kind == MeasureKind::Frame
            && let Some(dimension) = annotation
                .on
                .as_deref()
                .and_then(|on| self.annotation(on).ok())
        {
            picks.push(&dimension.a);
            picks.extend(dimension.b.as_ref());
        }
        picks
            .iter()
            .any(|pick| !pick.features().is_disjoint(&after))
    }

    pub fn measurement(&self, name: &str) -> Result<&Annotation, ModelError> {
        self.annotation(name)
    }

    /// The next free default name of a kind: `Distance1`, `Distance2`, ...
    pub fn next_measurement_name(&self, kind: MeasureKind) -> String {
        let stem = kind.name_stem();
        (1..)
            .map(|n| format!("{stem}{n}"))
            .find(|name| self.annotation_index(name).is_err())
            .expect("a free name")
    }

    /// The datum label of this letter, if there is one.
    pub fn datum_named(&self, letter: &str) -> Option<&Annotation> {
        self.state
            .annotations
            .iter()
            .find(|a| a.kind == MeasureKind::Datum && a.letter.as_deref() == Some(letter))
    }

    /// The first datum letter not in use: A, B, C, … (I, O and Q left
    /// out), then AA, AB, …; neither a datum label's nor one a frame still
    /// refers to, of the model or of a drawing's views (mitcad#717: one
    /// helper for both). Why none, when every one is taken.
    pub fn next_datum_letter(&self) -> Result<String, String> {
        self.state.next_datum_letter(&[])
    }

    /// The height (mm) the design's annotations' texts are set to
    /// (mitcad#894); none for the automatic one, which the API works out
    /// from the size of what is annotated ([`automatic_text_height`]).
    pub fn annotation_text_height(&self) -> Option<f64> {
        self.state.annotation_text_height
    }

    /// Sets the height of the design's annotations' texts (mm, 0.1 to
    /// 1000), or back to the automatic one with None; an undo step
    /// `Annotation Text Height` that recomputes nothing, none when nothing
    /// changes.
    pub fn set_annotation_text_height(&mut self, height: Option<f64>) -> Result<(), ModelError> {
        if let Some(height) = height {
            check_text_height(height).map_err(invalid)?;
        }
        if self.state.annotation_text_height == height {
            return Ok(());
        }
        self.apply_with(|state| {
            state.annotation_text_height = height;
            Ok(("Annotation Text Height".to_owned(), (), false))
        })
    }

    /// Adds an annotation; an undo step `Add Distance1` that recomputes
    /// nothing. An empty name takes the next free `<Kind><n>`, an empty
    /// colour [`AUTO_COLOR`]; the group is trimmed. Returns its
    /// name. Its picks are not checked against the model here: the API
    /// measures them first.
    pub fn add_annotation(&mut self, annotation: Annotation) -> Result<String, ModelError> {
        let annotation = self.annotation_to_add(annotation)?;
        let name = annotation.name.clone();
        let label = format!("Add {name}");
        self.apply_with(|state| {
            state.annotations.push(annotation);
            // A frame under no dimension stands on its leader.
            settle_frames(&mut state.annotations);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// The annotation as [`Document::add_annotation`] would add it — its
    /// name, colour, group and frame as they would be stored — or why it
    /// would be refused, without adding it (mitcad#1097: the preview of
    /// `add_annotation`, which then cannot differ from what is added).
    pub fn annotation_to_add(&self, mut annotation: Annotation) -> Result<Annotation, ModelError> {
        annotation.name = annotation.name.trim().to_owned();
        if annotation.name.is_empty() {
            // A datum by its letter (`DatumA`) where that name is free.
            annotation.name = annotation
                .letter
                .as_deref()
                .filter(|_| annotation.kind == MeasureKind::Datum)
                .map(|letter| format!("Datum{letter}"))
                .filter(|name| self.annotation_index(name).is_err())
                .unwrap_or_else(|| self.next_measurement_name(annotation.kind));
        }
        if let Some(letter) = &annotation.letter
            && self.datum_named(letter).is_some()
        {
            return Err(invalid(format!("datum {letter} exists")));
        }
        // The drawings' datum labels and the model's are one set of letters.
        if let Some(letter) = &annotation.letter
            && let Some(drawing) = self.drawing_with_datum(letter, None)
        {
            return Err(invalid(format!(
                "datum {letter} exists: a datum label of {}",
                drawing.name
            )));
        }
        if self.annotation_index(&annotation.name).is_ok() {
            return Err(invalid(format!("an annotation {} exists", annotation.name)));
        }
        annotation.color = if annotation.color.trim().is_empty() {
            AUTO_COLOR.to_owned()
        } else {
            normalized_color(&annotation.color).map_err(invalid)?
        };
        annotation.group = normalized_group(annotation.group.as_deref());
        annotation.normalize_frame();
        annotation.check().map_err(invalid)?;
        // A frame under no dimension stands on its leader, as
        // `settle_frames` leaves it once added.
        if let Some(on) = annotation.on.clone()
            && !self.annotation(&on).is_ok_and(|a| a.kind.takes_frame())
        {
            annotation.on = None;
        }
        Ok(annotation)
    }

    /// Adds a measurement, shown; an undo step `Add Distance1` that
    /// recomputes nothing. Returns its name.
    pub fn add_measurement(&mut self, new: NewMeasurement) -> Result<String, ModelError> {
        let mut annotation = Annotation::new(
            new.name.as_deref().unwrap_or(""),
            new.kind,
            new.color.as_deref().unwrap_or(""),
            new.a,
        );
        annotation.b = new.b;
        self.add_annotation(annotation)
    }

    /// Changes the annotations of these names as `change` does, as one
    /// undo step (`Edit Distance1`, `Edit 3 annotations`, or `label`), and
    /// checks each afterwards; names stay unique. Returns whether anything
    /// changed: nothing changed is no undo step.
    pub fn edit_annotations(
        &mut self,
        names: &[String],
        label: Option<String>,
        mut change: impl FnMut(&mut Annotation) -> Result<(), String>,
    ) -> Result<bool, ModelError> {
        if names.is_empty() {
            return Err(invalid("name the annotations to change"));
        }
        let mut indices = Vec::with_capacity(names.len());
        for name in names {
            let i = self.annotation_index(name)?;
            if indices.contains(&i) {
                return Err(invalid(format!("{name} is named twice")));
            }
            indices.push(i);
        }
        let mut changed = self.state.annotations.clone();
        for &i in &indices {
            let before = changed[i].name.clone();
            let letter = changed[i].letter.clone();
            change(&mut changed[i]).map_err(|e| invalid(format!("{before}: {e}")))?;
            changed[i].name = changed[i].name.trim().to_owned();
            // A datum still named by its letter (`DatumA`) follows a new
            // letter, where that name is free.
            let follows = changed[i].kind == MeasureKind::Datum
                && changed[i].name == before
                && changed[i].letter != letter
                && letter.is_some_and(|old| before == format!("Datum{old}"));
            if follows
                && let Some(new) = changed[i]
                    .letter
                    .as_deref()
                    .map(|l| format!("Datum{}", l.trim()))
                && !changed.iter().any(|a| a.name == new)
            {
                changed[i].name = new;
            }
            changed[i].group = normalized_group(changed[i].group.as_deref());
            changed[i].normalize_frame();
            changed[i].check().map_err(invalid)?;
        }
        // Names and letters unique first: a letter in use is refused as such,
        // before the frames referring to the old one would follow it.
        for (i, annotation) in changed.iter().enumerate() {
            if changed[..i].iter().any(|a| a.name == annotation.name) {
                return Err(invalid(format!("an annotation {} exists", annotation.name)));
            }
            if let Some(letter) = &annotation.letter
                && changed[..i]
                    .iter()
                    .any(|a| a.letter.as_ref() == Some(letter))
            {
                return Err(invalid(format!("datum {letter} exists")));
            }
            if let Some(letter) = &annotation.letter
                && self.state.annotations.get(i).map(|a| &a.letter) != Some(&annotation.letter)
                && let Some(drawing) = self.drawing_with_datum(letter, None)
            {
                return Err(invalid(format!(
                    "datum {letter} exists: a datum label of {}",
                    drawing.name
                )));
            }
        }
        // Frames under a renamed dimension follow its name.
        let renamed: Vec<(String, String)> = indices
            .iter()
            .map(|&i| {
                (
                    self.state.annotations[i].name.clone(),
                    changed[i].name.clone(),
                )
            })
            .filter(|(old, new)| old != new)
            .collect();
        // Frames referring to a datum whose letter changed follow its letter.
        let relettered: Vec<(String, String)> = indices
            .iter()
            .filter_map(|&i| {
                let before = &self.state.annotations[i];
                match (&before.letter, &changed[i].letter) {
                    (Some(old), Some(new)) if before.kind == MeasureKind::Datum && old != new => {
                        Some((old.clone(), new.clone()))
                    }
                    _ => None,
                }
            })
            .collect();
        for annotation in &mut changed {
            if let Some(on) = &annotation.on
                && let Some((_, new)) = renamed.iter().find(|(old, _)| old == on)
            {
                annotation.on = Some(new.clone());
            }
            if let Some(frame) = &mut annotation.frame {
                for datum in &mut frame.datums {
                    if let Some((_, new)) = relettered.iter().find(|(old, _)| old == datum) {
                        datum.clone_from(new);
                    }
                }
            }
        }
        // Any frame left under no dimension stands on its leader again.
        settle_frames(&mut changed);
        // What the follow-ups changed must hold as well: a frame referring
        // to A and C, with A lettered C, would refer to C twice.
        for (j, annotation) in changed.iter().enumerate() {
            if *annotation != self.state.annotations[j] {
                annotation.check().map_err(invalid)?;
            }
        }
        // The drawings' frames follow a datum's new letter as well, in the
        // same step (mitcad#745: the model's and the drawings' letters are
        // one set).
        let renamed_letters: std::collections::BTreeMap<String, String> =
            relettered.iter().cloned().collect();
        let mut drawings = self.state.drawings_relettered(&renamed_letters, None)?;
        // The drawings' annotations kept from a renamed one follow its name
        // (mitcad#1406), so that it is not brought into their views again.
        let renames = if renamed.is_empty() {
            &[][..]
        } else {
            &self.state.drawings[..]
        };
        for (i, original) in renames.iter().enumerate() {
            let mut drawing = drawings
                .iter()
                .find(|(j, _)| *j == i)
                .map_or_else(|| original.clone(), |(_, d)| d.clone());
            let mut followed = false;
            for view in drawing.sheets.iter_mut().flat_map(|s| s.views.iter_mut()) {
                for kept in &mut view.annotations {
                    if let Some(from) = &kept.kept_from
                        && let Some((_, new)) = renamed.iter().find(|(old, _)| old == from)
                    {
                        kept.kept_from = Some(new.clone());
                        followed = true;
                    }
                }
            }
            if followed {
                drawings.retain(|(j, _)| *j != i);
                drawings.push((i, drawing));
            }
        }
        if changed == self.state.annotations {
            return Ok(false);
        }
        let label = label.unwrap_or_else(|| match names {
            [name] => format!("Edit {name}"),
            _ => format!("Edit {} annotations", names.len()),
        });
        self.apply_with(|state| {
            state.annotations = changed;
            for (i, drawing) in drawings {
                state.drawings[i] = drawing;
            }
            Ok((label, (), false))
        })?;
        Ok(true)
    }

    /// Deletes the annotations of these names as one undo step (`Delete
    /// Distance1`, `Delete 3 annotations`).
    pub fn delete_annotations(&mut self, names: &[String]) -> Result<(), ModelError> {
        if names.is_empty() {
            return Err(invalid("name the annotations to delete"));
        }
        for name in names {
            self.annotation_index(name)?;
        }
        let label = match names {
            [name] => format!("Delete {name}"),
            _ => format!("Delete {} annotations", names.len()),
        };
        let names = names.to_vec();
        self.apply_with(|state| {
            state.annotations.retain(|a| !names.contains(&a.name));
            // Frames under a dimension deleted point at their own pick again.
            settle_frames(&mut state.annotations);
            Ok((label, (), false))
        })
    }

    pub fn rename_measurement(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        self.annotation_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the annotation needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.annotation_index(new_name).is_ok() {
            return Err(invalid(format!("an annotation {new_name} exists")));
        }
        self.edit_annotations(&[name.to_owned()], Some(format!("Rename {name}")), |a| {
            new_name.clone_into(&mut a.name);
            Ok(())
        })
        .map(|_| ())
    }

    /// Gives an annotation another colour (`#rrggbb`); an undo step
    /// `Colour of Distance1` unless nothing changes.
    pub fn set_measurement_color(&mut self, name: &str, color: &str) -> Result<(), ModelError> {
        self.annotation_index(name)?;
        let color = normalized_color(color).map_err(invalid)?;
        self.edit_annotations(&[name.to_owned()], Some(format!("Color of {name}")), |a| {
            a.color.clone_from(&color);
            Ok(())
        })
        .map(|_| ())
    }

    /// Shows or hides an annotation; an undo step `Show Distance1` /
    /// `Hide Distance1` unless nothing changes.
    pub fn set_measurement_visible(&mut self, name: &str, visible: bool) -> Result<(), ModelError> {
        self.edit_annotations(
            &[name.to_owned()],
            Some(format!("{} {name}", if visible { "Show" } else { "Hide" })),
            |a| {
                a.visible = visible;
                Ok(())
            },
        )
        .map(|_| ())
    }

    /// Changes the kind of value an annotation shows; an undo step `Kind
    /// of Distance1` unless nothing changes. The API checks first that its
    /// picks have that value.
    pub fn set_measurement_kind(
        &mut self,
        name: &str,
        kind: MeasureKind,
    ) -> Result<(), ModelError> {
        self.edit_annotations(&[name.to_owned()], Some(format!("Kind of {name}")), |a| {
            if a.kind != kind && (a.kind.is_leader() || kind.is_leader()) {
                return Err(format!(
                    "a {0} stays a {0}, a dimension a dimension",
                    a.kind.key()
                ));
            }
            if a.kind != kind {
                // Drawn afresh: another kind lies elsewhere.
                a.line_offset = None;
                a.text_offset = None;
                if matches!(
                    kind,
                    MeasureKind::Radius | MeasureKind::Diameter | MeasureKind::Angle
                ) {
                    a.plane = None;
                }
                // Between an angle and a length a tolerance means another
                // thing (degrees, millimetres, a fit).
                if kind.is_angle() != a.kind.is_angle() {
                    a.tolerance = None;
                }
            }
            a.kind = kind;
            Ok(())
        })
        .map(|_| ())
    }

    pub fn delete_measurement(&mut self, name: &str) -> Result<(), ModelError> {
        self.delete_annotations(&[name.to_owned()])
    }

    fn annotation_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .annotations
            .iter()
            .position(|m| m.name == name)
            .ok_or_else(|| invalid(format!("there is no annotation {name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_six_hex_digits_in_lower_case() {
        assert_eq!(normalized_color(" #A1b2C3 ").unwrap(), "#a1b2c3");
        for wrong in [
            "a1b2c3",
            "#a1b2c",
            "#a1b2c3d",
            "#g1b2c3",
            "red",
            "",
            "automatic",
        ] {
            assert!(normalized_color(wrong).is_err(), "{wrong}");
        }
        for color in OLD_DEFAULT_COLORS {
            assert_eq!(normalized_color(color).unwrap(), color);
        }
        // Auto (mitcad#894), in any case.
        assert_eq!(normalized_color(" Auto ").unwrap(), AUTO_COLOR);
        let mut auto = Annotation::new("Note1", MeasureKind::Note, AUTO_COLOR, body());
        auto.text = Some("Deburr".to_owned());
        auto.check().unwrap();
    }

    #[test]
    fn old_default_colours_read_as_auto() {
        // The six colours the program gave in turn: read as Auto; another
        // colour, chosen by the user, stays.
        let mut annotations: Vec<Annotation> = OLD_DEFAULT_COLORS
            .iter()
            .chain(["#112233", "#e8590c"].iter())
            .enumerate()
            .map(|(i, color)| {
                let mut a =
                    Annotation::new(&format!("Offset{i}"), MeasureKind::Offset, color, body());
                a.b = Some(body());
                a
            })
            .collect();
        read_old_defaults(&mut annotations);
        let colors: Vec<&str> = annotations.iter().map(|a| a.color.as_str()).collect();
        assert_eq!(
            colors,
            [
                "auto", "auto", "auto", "auto", "auto", "auto", "#112233", "#e8590c"
            ]
        );
        for annotation in &annotations {
            annotation.check().unwrap();
        }
    }

    #[test]
    fn text_heights_follow_the_iso_3098_series() {
        assert_eq!(automatic_text_height(None), 3.5);
        assert_eq!(automatic_text_height(Some(0.0)), 3.5);
        assert_eq!(automatic_text_height(Some(30.0)), 2.5);
        assert_eq!(automatic_text_height(Some(150.0)), 2.5);
        assert_eq!(automatic_text_height(Some(175.0)), 3.5);
        assert_eq!(automatic_text_height(Some(260.0)), 5.0);
        assert_eq!(automatic_text_height(Some(350.0)), 7.0);
        assert_eq!(automatic_text_height(Some(5000.0)), 10.0);
        check_text_height(5.0).unwrap();
        for wrong in [0.0, -1.0, 0.05, 2000.0, f64::NAN] {
            assert!(check_text_height(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn kinds_have_their_keys_and_stems() {
        for kind in MeasureKind::ALL
            .into_iter()
            .chain([MeasureKind::Note, MeasureKind::HoleCallout])
        {
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json, serde_json::json!(kind.key()));
            assert_eq!(serde_json::from_value::<MeasureKind>(json).unwrap(), kind);
        }
        // mitcad#902: a hole callout is a leader on one pick that a frame can
        // stand under, as a dimension.
        assert_eq!(MeasureKind::HoleCallout.key(), "hole_callout");
        assert_eq!(MeasureKind::HoleCallout.name_stem(), "HoleCallout");
        assert!(MeasureKind::HoleCallout.single() && MeasureKind::HoleCallout.is_leader());
        assert!(!MeasureKind::HoleCallout.is_note() && !MeasureKind::HoleCallout.is_length());
        assert!(MeasureKind::HoleCallout.takes_frame() && MeasureKind::Distance.takes_frame());
        assert!(!MeasureKind::Note.takes_frame() && !MeasureKind::Frame.takes_frame());
        assert_eq!(MeasureKind::AxisDistance.name_stem(), "AxisDistance");
        assert_eq!(MeasureKind::Note.name_stem(), "Note");
        assert!(MeasureKind::Radius.single() && MeasureKind::Diameter.single());
        assert!(MeasureKind::Note.single() && MeasureKind::Note.is_note());
        assert!(!MeasureKind::Offset.single());
        assert_eq!(MeasureKind::Diameter.default_prefix(), "Ø");
        assert_eq!(MeasureKind::Radius.default_prefix(), "R");
        assert!(!MeasureKind::Angle.is_length() && MeasureKind::Offset.is_length());
        // mitcad#1049: a material note and a process note are leaders on
        // one body, their text written from it.
        for kind in [MeasureKind::Material, MeasureKind::ProcessNote] {
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json, serde_json::json!(kind.key()));
            assert_eq!(serde_json::from_value::<MeasureKind>(json).unwrap(), kind);
            assert!(kind.single() && kind.is_leader() && kind.is_body_note());
            assert!(!kind.is_note() && !kind.takes_frame() && !kind.is_length());
        }
        assert_eq!(MeasureKind::Material.key(), "material");
        assert_eq!(MeasureKind::ProcessNote.key(), "process_note");
        assert_eq!(MeasureKind::ProcessNote.name_stem(), "ProcessNote");
        assert!(!MeasureKind::Note.is_body_note() && !MeasureKind::HoleCallout.is_body_note());
    }

    #[test]
    fn a_material_or_process_note_points_at_a_body_and_has_no_text() {
        for kind in [MeasureKind::Material, MeasureKind::ProcessNote] {
            let mut note = Annotation::new("Material1", kind, AUTO_COLOR, body());
            note.check().unwrap();
            note.text = Some("PLA".to_owned());
            assert!(note.check().unwrap_err().contains("no text of its own"));
            note.text = None;
            note.line_offset = Some(2.0);
            assert!(note.check().unwrap_err().contains("no dimension line"));
            note.line_offset = None;
            note.a = MeasurePick {
                datum: Some("xy".to_owned()),
                ..MeasurePick::default()
            };
            assert!(note.check().unwrap_err().contains("points at a body"));
        }
    }

    fn body() -> MeasurePick {
        MeasurePick {
            body: Some("F2.b0".parse().unwrap()),
            ..MeasurePick::default()
        }
    }

    #[test]
    fn a_measurement_has_the_picks_its_kind_needs() {
        let mut m = Annotation::new("Distance1", MeasureKind::Distance, "#d9480f", body());
        assert!(m.check().unwrap_err().contains("between two picks"));
        m.b = Some(MeasurePick {
            datum: Some("xy".to_owned()),
            ..MeasurePick::default()
        });
        m.check().unwrap();
        m.kind = MeasureKind::Radius;
        assert!(m.check().unwrap_err().contains("on one pick"));
        m.b = None;
        m.check().unwrap();
        m.a = MeasurePick::default();
        assert!(m.check().unwrap_err().contains("a pick is a body"));
        m.a = body();
        m.color = "#D9480F".to_owned();
        assert!(m.check().unwrap_err().contains("written"));
    }

    #[test]
    fn tolerances_formats_planes_and_notes_are_checked() {
        let mut d = Annotation::new("Diameter1", MeasureKind::Diameter, "#d9480f", body());
        d.tolerance = Some(Tolerance::Symmetric { value: 0.1 });
        d.check().unwrap();
        d.tolerance = Some(Tolerance::Symmetric { value: 0.0 });
        assert!(d.check().unwrap_err().contains("more than 0"));
        d.tolerance = Some(Tolerance::Deviations {
            upper: -0.1,
            lower: 0.2,
        });
        assert!(d.check().unwrap_err().contains("upper deviation"));
        d.tolerance = Some(Tolerance::Limits {
            upper: 0.2,
            lower: -0.1,
        });
        d.check().unwrap();
        d.tolerance = Some(Tolerance::Fit {
            code: "H7".to_owned(),
            deviations: true,
        });
        d.check().unwrap();
        d.tolerance = Some(Tolerance::Fit {
            code: "Q7".to_owned(),
            deviations: false,
        });
        assert!(d.check().is_err());
        d.tolerance = None;
        d.format.decimals = Some(MAX_DECIMALS + 1);
        assert!(d.check().unwrap_err().contains("decimals"));
        d.format.decimals = Some(2);
        // A radius lies in its circle's plane; a diameter may have its
        // leader's plane (mitcad#902). Neither has a line to move.
        d.plane = Some(AnnotationPlane {
            normal: Some([0.0, 0.0, 1.0]),
            ..AnnotationPlane::default()
        });
        d.check().unwrap();
        d.kind = MeasureKind::Radius;
        assert!(d.check().unwrap_err().contains("plane of its circle"));
        d.kind = MeasureKind::Diameter;
        d.plane = None;
        d.line_offset = Some(5.0);
        assert!(d.check().unwrap_err().contains("no dimension line"));
        d.line_offset = None;
        d.check().unwrap();

        let mut angle = Annotation::new("Angle1", MeasureKind::Angle, "#d9480f", body());
        angle.b = Some(body());
        angle.tolerance = Some(Tolerance::Fit {
            code: "H7".to_owned(),
            deviations: false,
        });
        assert!(angle.check().unwrap_err().contains("length's tolerance"));
        angle.tolerance = Some(Tolerance::Symmetric { value: 0.5 });
        angle.line_offset = Some(-1.0);
        assert!(angle.check().unwrap_err().contains("radius more than 0"));
        angle.line_offset = Some(8.0);
        angle.check().unwrap();

        let mut distance = Annotation::new("Distance1", MeasureKind::Distance, "#d9480f", body());
        distance.b = Some(body());
        distance.plane = Some(AnnotationPlane::default());
        assert!(distance.check().unwrap_err().contains("a normal"));
        distance.plane = Some(AnnotationPlane {
            normal: Some([0.0, 0.0, 0.0]),
            ..AnnotationPlane::default()
        });
        assert!(distance.check().unwrap_err().contains("not zero"));
        distance.plane = Some(AnnotationPlane {
            pick: Some(MeasurePick {
                datum: Some("xy".to_owned()),
                ..MeasurePick::default()
            }),
            ..AnnotationPlane::default()
        });
        distance.line_offset = Some(-4.0);
        distance.check().unwrap();
        distance.group = Some(" Shop ".to_owned());
        assert!(distance.check().unwrap_err().contains("group"));

        let mut note = Annotation::new("Note1", MeasureKind::Note, "#d9480f", body());
        assert!(note.check().unwrap_err().contains("needs a text"));
        note.text = Some("Deburr\nall edges".to_owned());
        note.at = Some([1.0, 2.0, 3.0]);
        note.check().unwrap();
        note.tolerance = Some(Tolerance::Symmetric { value: 0.1 });
        assert!(note.check().unwrap_err().contains("a note has a text"));
        note.tolerance = None;
        note.b = Some(body());
        assert!(note.check().unwrap_err().contains("one pick"));
        note.b = None;
        note.plane = Some(AnnotationPlane {
            normal: Some([0.0, 0.0, 1.0]),
            x: Some([0.0, 0.0, 2.0]),
            pick: None,
        });
        assert!(note.check().unwrap_err().contains("direction in it"));
    }

    #[test]
    fn old_measurements_read_as_annotations_and_write_back_the_same() {
        let old = serde_json::json!({"name": "Offset1", "kind": "offset", "color": "#d9480f",
            "visible": false, "a": {"body": "F2.b0"}, "b": {"datum": "xy"}});
        let annotation: Annotation = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(annotation.group, None);
        assert_eq!(annotation.tolerance, None);
        assert!(annotation.format.is_default());
        annotation.check().unwrap();
        assert_eq!(serde_json::to_value(&annotation).unwrap(), old);
    }
}
