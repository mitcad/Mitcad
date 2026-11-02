// SPDX-License-Identifier: MIT
//! Annotations (mitcad#702, `document/annotations.rs`): the commands
//! `add_annotation`, `edit_annotation` and `delete_annotation`, and the
//! `annotations` query (the `measurements` query is the same), which
//! measures each one at the timeline marker (`measurements.rs`), writes its
//! value ISO 129 style with its prefix, decimals, own text and tolerance,
//! and lays it out in its plane: the dimension and extension lines, an
//! angle's arc, the arrows, and where its text stands and which way it
//! runs. Hidden ones are measured only when asked for, and picks are
//! measured again only when their bodies or datums changed (mitcad#716,
//! `Document::measure_picks`). See `commands.md`, "Annotations".
//!
//! Everything is in the design's coordinates and millimetres, the texts'
//! height too (mitcad#894: the design's, else one of the ISO 3098 series by
//! the size of what is annotated); a length's automatic dimension line
//! lies outside its bodies' extent in its plane, stacked outside parallel
//! ones. The application projects it, keeps the text readable within its
//! plane (ISO 129-1) and sizes arrowheads by the text's height on screen.

use mitcad_vec3::{add, add_scaled, cross, distance, dot, norm, scale, sub, unit, unit_above};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};

use super::ApiError;
use super::analysis::{MeasureCache, MeasuredPicks};
use super::drawing_annotations::add_annotation_notice;
use super::measure_axes::Element;
use super::measurements::{
    Drawn, Found, across, add_drawn, check_kind, default_kind, no_single_value, values_of,
};
use super::prelude::*;
use mitcad_model::datum::Vec3;
use mitcad_model::internal::document::normalized_color;
use mitcad_model::kernel::Kernel;
use mitcad_model::transform::Transform;
use mitcad_model::{
    Annotation, AnnotationFormat, AnnotationPlane, Characteristic, DatumNeed, Document,
    GeometricTolerance, MeasureKind, MeasurePick, TEXT_HEIGHTS, Tolerance, automatic_text_height,
};

const X: Vec3 = [1.0, 0.0, 0.0];
const Y: Vec3 = [0.0, 1.0, 0.0];
const Z: Vec3 = [0.0, 0.0, 1.0];

/// A length's dimension line from its ends when the offset is not set:
/// this share of the length, at least 1 mm.
const LINE_SHARE: f64 = 0.25;
/// The default places in text heights (mitcad#902: none in a view's pixels
/// frozen when an annotation is added, so they keep their proportion to
/// the texts): an angle's arc radius when neither it nor the picks' reach
/// is known; a note's text from its arrow's point, along and across; a
/// circle's text beyond its rim at least (and 1.5 radii from its centre);
/// a length's dimension line from its ends when no body is there to keep
/// clear of and a view chose its side.
const ARC_SHARE: f64 = 5.0;
const NOTE_SHARE: [f64; 2] = [4.0, 3.0];
const CIRCLE_SHARE: f64 = 3.0;
const BARE_LINE_SHARE: f64 = 2.5;
/// A leader's horizontal landing under its text's start, and how far past
/// the bodies' extent its text stands when placed automatically: text
/// heights (mitcad#902). The application draws the landing in proportion
/// to its text as drawn (mitcad#1168: longer where a text held to its
/// least height on screen is drawn larger than the design's).
const LANDING_SHARE: f64 = 1.5;
const LEADER_CLEAR: f64 = 2.0;
/// A text moved nearer its leader's point than its landing is long: the
/// landing at most this share of the way back to the point along the
/// text, so that the leader meets it at an obtuse bend (mitcad#1168).
const LANDING_REACH: f64 = 0.5;
/// A datum label's stem from its surface to its letter's square: text
/// heights.
const DATUM_STEM: f64 = 3.0;
/// The direction towards the eye of the default isometric view: a round
/// face's circle is drawn at its end on that side, where its rim shows.
const ISO_EYE: Vec3 = [1.0, -1.0, 1.0];
/// A circle's text from its centre when not moved: radii.
const CIRCLE_TEXT: f64 = 1.5;
/// Deviations without set decimals show up to this many.
const DEVIATION_DECIMALS: usize = 4;
/// A length's dimension line placed automatically (mitcad#894; mitcad#902:
/// as far off as a drawing's first line): this many text heights clear of
/// its bodies' outline, and a parallel one stacked this many further out.
const CLEAR_SHARE: f64 = 6.0;
const STACK_SHARE: f64 = 2.5;
/// Directions closer than this (the sine of the angle) are parallel, and a
/// face's normal this square to a length holds it.
const SQUARE: f64 = 1e-6;

/// A field that may be given as `null`: `Some(None)` then, `None` when
/// left out.
pub(super) fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

/// What the view looked like when an annotation is added: the direction it
/// looks along (from the eye) and its up direction. It suggests the plane
/// (square to the view) and the side of a length's line, which are then
/// kept. The size of a pixel (mm) near the picks is still taken but no
/// longer used: the places are in text heights (mitcad#902).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewHint {
    direction: Vec3,
    #[serde(default)]
    up: Option<Vec3>,
    #[serde(default, rename = "pixel")]
    _pixel: Option<f64>,
}

/// `add_annotation`: the fields of [`Annotation`], each optional but the
/// first pick, and the view it is added in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NewAnnotation {
    a: MeasurePick,
    #[serde(default)]
    b: Option<MeasurePick>,
    /// What the picks give first when left out; `note` when a `text` is
    /// given.
    #[serde(default)]
    kind: Option<MeasureKind>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    plane: Option<AnnotationPlane>,
    #[serde(default)]
    line_offset: Option<f64>,
    #[serde(default)]
    text_offset: Option<[f64; 2]>,
    #[serde(default)]
    format: Option<AnnotationFormat>,
    #[serde(default)]
    tolerance: Option<Tolerance>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    at: Option<Vec3>,
    #[serde(default)]
    view: Option<ViewHint>,
    /// A datum's letter (the next free one when left out).
    #[serde(default)]
    letter: Option<String>,
    /// A feature control frame's tolerance, and the dimension it stands
    /// under instead of a leader.
    #[serde(default)]
    frame: Option<GeometricTolerance>,
    #[serde(default)]
    on: Option<String>,
    /// Only what it would add (mitcad#1097): the annotation as the
    /// `annotations` query would give it once added, nothing stored.
    #[serde(default)]
    preview: bool,
}

/// `edit_annotation`: the annotation's `name` (or `names`, several at
/// once) and what changes; `null` sets a field back to its default.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnnotationEdit {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    names: Option<Vec<String>>,
    #[serde(default)]
    new_name: Option<String>,
    #[serde(default)]
    kind: Option<MeasureKind>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default, deserialize_with = "present")]
    group: Option<Option<String>>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    a: Option<MeasurePick>,
    #[serde(default, deserialize_with = "present")]
    b: Option<Option<MeasurePick>>,
    #[serde(default, deserialize_with = "present")]
    plane: Option<Option<AnnotationPlane>>,
    #[serde(default, deserialize_with = "present")]
    line_offset: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    text_offset: Option<Option<[f64; 2]>>,
    #[serde(default)]
    format: Option<FormatEdit>,
    #[serde(default, deserialize_with = "present")]
    tolerance: Option<Option<Tolerance>>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default, deserialize_with = "present")]
    at: Option<Option<Vec3>>,
    #[serde(default)]
    letter: Option<String>,
    #[serde(default)]
    frame: Option<GeometricTolerance>,
    #[serde(default, deserialize_with = "present")]
    on: Option<Option<String>>,
}

/// The format's fields that change, the others kept.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormatEdit {
    #[serde(default, deserialize_with = "present")]
    decimals: Option<Option<u8>>,
    #[serde(default, deserialize_with = "present")]
    prefix: Option<Option<String>>,
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default, deserialize_with = "present")]
    replace: Option<Option<String>>,
}

impl FormatEdit {
    pub(super) fn apply(&self, format: &mut AnnotationFormat) {
        if let Some(decimals) = self.decimals {
            format.decimals = decimals;
        }
        if let Some(prefix) = &self.prefix {
            format.prefix.clone_from(prefix);
        }
        if let Some(before) = &self.before {
            before.trim().clone_into(&mut format.before);
        }
        if let Some(after) = &self.after {
            after.trim().clone_into(&mut format.after);
        }
        if let Some(replace) = &self.replace {
            format.replace = replace
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(str::to_owned);
        }
    }
}

/// The names a command gives: `name`, or `names` of several.
pub(super) fn names_of(
    name: Option<String>,
    names: Option<Vec<String>>,
) -> Result<Vec<String>, ApiError> {
    match (name, names) {
        (Some(name), None) => Ok(vec![name]),
        (None, Some(names)) if !names.is_empty() => Ok(names),
        _ => Err(ApiError(
            "give the annotation's `name`, or the `names` of several".to_owned(),
        )),
    }
}

/// What an annotation is drawn from: a length's two ends, a circle, an
/// angle's corner and rays, or a note's point (with its pick's plane's
/// normal).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Ends([Vec3; 2]),
    Circle {
        center: Vec3,
        axis: Vec3,
        radius: f64,
    },
    Arc {
        center: Vec3,
        from: Vec3,
        to: Vec3,
        reach: Option<f64>,
    },
    Note {
        anchor: Vec3,
        normal: Option<Vec3>,
        /// The picked surface's outward normal at the point (a planar
        /// face's, a round face's radius): a datum label's stem stands on
        /// it (mitcad#902).
        surface: Option<Vec3>,
    },
}

impl Shape {
    fn of(found: &Found) -> Self {
        match found.drawn {
            Drawn::Ends(ends) => Self::Ends(ends),
            Drawn::Circle {
                center,
                axis,
                radius,
                ..
            } => Self::Circle {
                center,
                axis,
                radius,
            },
            Drawn::Arc {
                center,
                from,
                to,
                reach,
            } => Self::Arc {
                center,
                from,
                to,
                reach,
            },
        }
    }
}

/// An angle's arc: its centre, radius, the unit rays it runs between (from
/// `from` towards `to`) and its plane's normal.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ArcDrawn {
    center: Vec3,
    radius: f64,
    from: Vec3,
    to: Vec3,
    normal: Vec3,
}

/// Where an annotation is drawn, in the design's coordinates.
#[derive(Debug, Clone, PartialEq)]
struct Drawing {
    /// Its plane: a point of it, the unit axes `text_offset` is along —
    /// `x` (a length's dimension line, the tangent at an angle's arc's
    /// middle, a circle's `across` of its axis, a note's text) and `y`
    /// (across it: the side a length's line is offset to) — and the
    /// normal, a right-handed frame.
    origin: Vec3,
    x: Vec3,
    y: Vec3,
    normal: Vec3,
    /// The dimension line's offset or the arc's radius in use.
    line_offset: f64,
    /// The text's offset in use, along `x` and `y`.
    text_offset: [f64; 2],
    /// From a measured point to the dimension line (the application leaves
    /// a small gap at the point and runs a little past the line).
    extension_lines: Vec<[Vec3; 2]>,
    /// Dimension lines and a note's leader.
    dimension_lines: Vec<[Vec3; 2]>,
    arcs: Vec<ArcDrawn>,
    /// Each arrow's tip and the unit direction it points along.
    arrows: Vec<[Vec3; 2]>,
    /// A circle's centre, for its centre mark.
    center: Option<Vec3>,
    /// Where the text stands (its middle on the line, or a note's start)
    /// and the unit directions it runs along and up.
    text: [Vec3; 3],
    /// The height of its text, mm (mitcad#894): the design's.
    text_height: f64,
    /// A leader's text's place, for those laid out after it.
    text_box: Option<TextBox>,
}

impl Drawing {
    fn to_json(&self) -> Value {
        // + 0.0 turns -0.0 into 0.0.
        let v = |v: Vec3| v.map(|x| x + 0.0);
        let pairs = |lines: &[[Vec3; 2]]| -> Vec<[Vec3; 2]> {
            lines.iter().map(|[p, q]| [v(*p), v(*q)]).collect()
        };
        let mut value = json!({
            "plane": {"origin": v(self.origin), "x": v(self.x), "y": v(self.y),
                      "normal": v(self.normal)},
            "line_offset": self.line_offset + 0.0,
            "text_offset": [self.text_offset[0] + 0.0, self.text_offset[1] + 0.0],
            "extension_lines": pairs(&self.extension_lines),
            "dimension_lines": pairs(&self.dimension_lines),
            "arcs": self.arcs.iter().map(|arc| json!({
                "center": v(arc.center), "radius": arc.radius, "from": v(arc.from),
                "to": v(arc.to), "normal": v(arc.normal)})).collect::<Vec<_>>(),
            "arrows": self.arrows.iter().map(|[tip, direction]| json!({
                "tip": v(*tip), "direction": v(*direction)})).collect::<Vec<_>>(),
            "text": {"at": v(self.text[0]), "direction": v(self.text[1]),
                     "up": v(self.text[2])},
            "text_height": self.text_height,
        });
        if let Some(center) = self.center {
            value["center"] = json!(v(center));
        }
        value
    }
}

/// What an annotation's automatic places look at (mitcad#894): the height
/// of the design's annotations' texts and, for a length, the corners of its
/// bodies' boxes, the normals of the planar faces at its picks, whether
/// each pick is a plane (its extension line then leaves the body at its
/// outline), the dimension lines laid out before it (a parallel one stacks
/// outside them) and the direction towards the eye of the view an
/// annotation is added in (a length's line goes to that side, mitcad#902).
#[derive(Debug, Clone, Copy)]
struct Around<'a> {
    height: f64,
    extent: &'a [Vec3],
    faces: &'a [Vec3],
    planes: [bool; 2],
    /// Each pick's own outline (the corners of a face's box): a plane's
    /// extension line leaves the face there (mitcad#902).
    outlines: [&'a [Vec3]; 2],
    taken: &'a [Taken],
    prefer: Option<Vec3>,
    /// The leaders' texts laid out before it: a leader's text placed
    /// automatically moves up past them (mitcad#902).
    boxes: &'a [TextBox],
    /// The size of a leader's text, mm (width, height), as estimated from
    /// its words.
    text_size: Option<[f64; 2]>,
}

/// Where a leader's text lies (mitcad#902): its plane's normal and `x`, and
/// the rectangle it takes there, in coordinates along `x` and up.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TextBox {
    normal: Vec3,
    x: Vec3,
    low: [f64; 2],
    high: [f64; 2],
    /// Its leader, from the point to the knee.
    leader: [[f64; 2]; 2],
}

impl Around<'_> {
    /// Nothing around: a length's line where it always was (a quarter of
    /// its length off), the texts this high.
    #[cfg(test)]
    fn bare(height: f64) -> Around<'static> {
        Around {
            height,
            extent: &[],
            faces: &[],
            planes: [false; 2],
            outlines: [&[], &[]],
            taken: &[],
            prefer: None,
            boxes: &[],
            text_size: None,
        }
    }

    /// Around the annotation of these picks.
    fn of<'a>(height: f64, picks: &'a MeasuredPicks, taken: &'a [Taken]) -> Around<'a> {
        let plane = |e: Option<Element>| matches!(e, Some(Element::Plane { .. }));
        Around {
            height,
            extent: &picks.extent,
            faces: &picks.faces,
            planes: [plane(picks.elements[0]), plane(picks.elements[1])],
            outlines: [&picks.outlines[0], &picks.outlines[1]],
            taken,
            prefer: None,
            boxes: &[],
            text_size: None,
        }
    }
}

/// A length's dimension line laid out (its plane's normal and `x`, its
/// ends), for those laid out after it to stack outside it.
#[derive(Debug, Clone, Copy)]
struct Taken {
    normal: Vec3,
    x: Vec3,
    from: Vec3,
    to: Vec3,
}

/// The annotations laid out so far in a pass (mitcad#894): the texts'
/// height, the lengths' dimension lines, and each one's text frame (`at`,
/// along, up) by name, where a frame under it stands.
#[derive(Debug, Default)]
struct Laid {
    height: f64,
    taken: Vec<Taken>,
    texts: std::collections::BTreeMap<String, [Vec3; 3]>,
    /// Each leader's lines (a hole callout's; 0 for others) and which end
    /// of its text the leader comes to (`start`, `end`), by name: a frame
    /// under a hole callout stands below its lines (mitcad#902).
    leaders: std::collections::BTreeMap<String, (usize, &'static str)>,
    /// The leaders' texts laid out (mitcad#902).
    boxes: Vec<TextBox>,
}

/// The document's length unit for values in words.
#[derive(Debug, Clone, Copy)]
pub(super) struct Units {
    symbol: &'static str,
    millimetres: f64,
}

impl Units {
    #[cfg(test)]
    pub(super) const MM: Self = Self {
        symbol: "mm",
        millimetres: 1.0,
    };

    /// A unit by its symbol and its millimetres (a drawing sheet's).
    pub(super) fn new(symbol: &'static str, millimetres: f64) -> Self {
        Self {
            symbol,
            millimetres,
        }
    }

    /// The same unit as the drawings write lengths in (a hole callout's,
    /// mitcad#902).
    pub(super) fn drawing(self) -> mitcad_model_drawing::Units {
        mitcad_model_drawing::Units {
            symbol: self.symbol,
            mm: if self.millimetres > 0.0 {
                self.millimetres
            } else {
                1.0
            },
        }
    }
}

/// A value of a dimension on a drawing's view in words (mitcad#707 phase
/// 2): the words of a dimension of the model of this kind, format and
/// tolerance, one engine for both; with why a fit code's deviations are
/// not shown, if they are not.
pub(super) fn value_text(
    kind: MeasureKind,
    format: &AnnotationFormat,
    tolerance: Option<&Tolerance>,
    value: f64,
    units: Units,
) -> (mitcad_model_drawing::annotate::ValueText, Option<String>) {
    let mut annotation = Annotation::new("", kind, "#000000", MeasurePick::default());
    annotation.format = format.clone();
    annotation.tolerance = tolerance.cloned();
    let label = dimension_label(&annotation, value, units);
    (
        mitcad_model_drawing::annotate::ValueText {
            text: label.text,
            main: label.main,
            upper: label.upper,
            lower: label.lower,
            brackets: label.brackets,
            after: label.after,
        },
        label.warning,
    )
}

/// A feature control frame's cells (the symbol's text first) and all of
/// them on one line, as the model's frames have them, with the warning for
/// a datum referred to that no datum label has (`datums`: those there are).
pub(super) fn frame_words(
    frame: &GeometricTolerance,
    units: Units,
    datums: &[&str],
) -> (Vec<String>, String, Option<String>) {
    let label = frame_label(frame, units, datums);
    (label.frame, label.text, label.warning)
}

/// A dimension's value in words, ISO 129 style: `main` (its own text
/// before, the prefix, the value, a symmetric tolerance or a fit code),
/// a stack of `upper` over `lower` (deviations, limits, a fit's
/// deviations in `brackets`), then the text `after`; `text` all on one
/// line. A note's `lines`.
#[derive(Debug, Clone, Default, PartialEq)]
struct Label {
    text: String,
    main: String,
    upper: Option<String>,
    lower: Option<String>,
    brackets: bool,
    after: String,
    /// The tolerance alone in words (`±0.1`, `+0.2/-0.1`, `H7`).
    tolerance: String,
    /// The tolerance's deviations (mm or degrees), upper and lower.
    deviations: Option<(f64, f64)>,
    /// Why a fit code's deviations are not shown.
    warning: Option<String>,
    lines: Vec<String>,
    /// A feature control frame's cells: the symbol, the tolerance, the
    /// datums referred to.
    frame: Vec<String>,
    /// A datum label's letter.
    datum: Option<String>,
}

impl Label {
    fn to_json(&self) -> Value {
        let mut value = json!({"text": self.text});
        if let Some(warning) = &self.warning {
            value["warning"] = json!(warning);
        }
        if !self.lines.is_empty() {
            value["lines"] = json!(self.lines);
            return value;
        }
        if !self.frame.is_empty() {
            value["frame"] = json!(self.frame);
            value["tolerance"] = json!(self.tolerance);
            return value;
        }
        if let Some(datum) = &self.datum {
            value["datum"] = json!(datum);
            return value;
        }
        value["main"] = json!(self.main);
        if let (Some(upper), Some(lower)) = (&self.upper, &self.lower) {
            value["upper"] = json!(upper);
            value["lower"] = json!(lower);
            value["brackets"] = json!(self.brackets);
        }
        if !self.after.is_empty() {
            value["after"] = json!(self.after);
        }
        if !self.tolerance.is_empty() {
            value["tolerance"] = json!(self.tolerance);
        }
        value
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait AnnotationsOps<K: Kernel> {
    fn add_annotation_command(&mut self, new: NewAnnotation) -> Result<Value, ApiError>;

    fn edit_annotation_command(&mut self, edit: AnnotationEdit) -> Result<Value, ApiError>;

    /// The `annotations` query (and `measurements`): each annotation in its
    /// file form, with what its picks give at the marker, its value in
    /// words and where it is drawn, or why it cannot be measured there. A
    /// hidden one is measured only with `measure_hidden` (mitcad#716);
    /// otherwise it is marked `"measured": false`, with an `error` only
    /// when a pick's body or datum is not there.
    fn annotations_json(&self, measure_hidden: bool) -> Value;

    /// The `annotation_text_height` query (mitcad#894): the height set
    /// (null for the automatic one), the height in use, the automatic one
    /// and the ISO 3098 series it is taken from.
    fn annotation_text_height_json(&self) -> Value;
}

impl<K: Kernel> AnnotationsOps<K> for Document<K> {
    fn add_annotation_command(&mut self, new: NewAnnotation) -> Result<Value, ApiError> {
        let picks = self
            .measure_picks(&new.a, new.b.as_ref())
            .map_err(|e| ApiError(format!("the annotation's picks: {e}")))?;
        // A text makes a note, a letter a datum, a tolerance a frame.
        let given = new.kind.or_else(|| {
            if new.text.is_some() {
                Some(MeasureKind::Note)
            } else if new.letter.is_some() {
                Some(MeasureKind::Datum)
            } else if new.frame.is_some() {
                Some(MeasureKind::Frame)
            } else {
                None
            }
        });
        let kind = match given {
            Some(kind) if kind.is_leader() => kind,
            kind => {
                let found = values_of(&picks);
                let kind = match kind {
                    Some(kind) => kind,
                    None => default_kind(&picks, &found).ok_or_else(no_single_value)?,
                };
                check_kind(kind, &found)?;
                kind
            }
        };
        let mut annotation = Annotation::new(
            new.name.as_deref().unwrap_or(""),
            kind,
            new.color.as_deref().unwrap_or(""),
            new.a,
        );
        annotation.b = new.b;
        annotation.visible = new.visible.unwrap_or(true);
        annotation.group = new.group;
        annotation.plane = new.plane;
        annotation.line_offset = new.line_offset;
        annotation.text_offset = new.text_offset;
        annotation.format = new.format.unwrap_or_default();
        annotation.tolerance = new.tolerance;
        annotation.text = new.text;
        annotation.at = new.at;
        annotation.frame = new.frame;
        annotation.on = new.on;
        // A datum takes the first free letter unless given one.
        if kind == MeasureKind::Datum {
            annotation.letter = Some(match new.letter {
                Some(letter) => letter.trim().to_owned(),
                None => self.next_datum_letter().map_err(ApiError)?,
            });
        } else {
            annotation.letter = new.letter;
        }
        // A note, a datum or a frame needs a point to point at: a body picked
        // without one has none. A datum or a frame stands on a feature, not
        // on a point (under a dimension the pick is only an anchor); a
        // frame under a dimension needs that dimension.
        if kind.is_leader() && annotation.on.is_none() {
            feature_pick(kind, &annotation.a, &picks).map_err(ApiError)?;
            self.shape_of(&annotation, &picks).map_err(ApiError)?;
        }
        // A hole callout needs a hole (or a round face or edge) to write.
        if kind == MeasureKind::HoleCallout {
            let unit = self.parameters().context().default_length_unit;
            let units = Units::new(unit.symbol(), unit.millimetres());
            self.hole_callout_lines(&annotation.a, &picks, units.drawing())
                .map_err(ApiError)?;
        }
        // A material or process note needs a body to write (mitcad#1049).
        if kind.is_body_note() {
            self.body_note_lines(kind, &annotation.a)
                .map_err(ApiError)?;
        }
        if let Some(on) = &annotation.on {
            self.dimension_to_stand_under(on.trim()).map_err(ApiError)?;
        }
        if let Some(view) = &new.view {
            self.place_for_view(&mut annotation, &picks, view)?;
        }
        if new.preview {
            return self.annotation_preview(annotation);
        }
        let name = self.add_annotation(annotation)?;
        Ok(json!({"name": name, "kind": kind}))
    }

    fn edit_annotation_command(&mut self, edit: AnnotationEdit) -> Result<Value, ApiError> {
        let AnnotationEdit {
            name,
            names,
            new_name,
            kind,
            color,
            group,
            visible,
            a,
            b,
            plane,
            line_offset,
            text_offset,
            format,
            tolerance,
            text,
            at,
            letter,
            frame,
            on,
        } = edit;
        let names = names_of(name, names)?;
        if names.len() > 1 && (new_name.is_some() || a.is_some() || b.is_some()) {
            return Err(ApiError(
                "several annotations change together without a new name or picks".to_owned(),
            ));
        }
        let color = color
            .map(|c| normalized_color(&c))
            .transpose()
            .map_err(ApiError)?;
        let picks_change = a.is_some() || b.is_some();
        // The kind each shows afterwards, checked against its picks: one
        // they do not have any more is replaced by their first.
        let mut kinds = Vec::with_capacity(names.len());
        for name in &names {
            let current = self.annotation(name)?;
            let new_kind = kind.unwrap_or(current.kind);
            if new_kind != current.kind && (new_kind.is_leader() || current.kind.is_leader()) {
                return Err(ApiError(format!(
                    "{name}: a {0} stays a {0}, a dimension a dimension",
                    current.kind.key()
                )));
            }
            // Under a dimension afterwards, or on a leader again.
            let on_after = match &on {
                Some(on) => on.clone(),
                None => current.on.clone(),
            };
            let leader_again = current.on.is_some() && on_after.is_none();
            // A note's pick or point changed: it must still give a point,
            // as when it was added (another pick drops the point kept in
            // the old pick's coordinates unless a new one is given).
            // So must a frame taken from under its dimension; under one, its
            // pick is only an anchor and nothing is checked.
            if current.kind.is_leader()
                && on_after.is_none()
                && (a.is_some() || at.is_some() || leader_again)
            {
                let mut probe = current.clone();
                if let Some(a) = &a {
                    probe.a = a.clone();
                    probe.at = None;
                }
                if let Some(at) = at {
                    probe.at = at;
                }
                let picks = self
                    .measure_picks(&probe.a, None)
                    .map_err(|e| ApiError(format!("{name}: the picks: {e}")))?;
                self.shape_of(&probe, &picks)
                    .and_then(|_| feature_pick(current.kind, &probe.a, &picks))
                    .map_err(|e| ApiError(format!("{name}: {e}")))?;
            }
            // Under a dimension there is (not itself, not a leader).
            if let Some(Some(under)) = &on {
                self.dimension_to_stand_under(under.trim())
                    .map_err(|e| ApiError(format!("{name}: {e}")))?;
            }
            if !(picks_change || kind.is_some()) {
                kinds.push(new_kind);
                continue;
            }
            let new_a = a.as_ref().unwrap_or(&current.a);
            let new_b = match &b {
                Some(b) => b.as_ref(),
                None => current.b.as_ref(),
            };
            let picks = self
                .measure_picks(new_a, new_b)
                .map_err(|e| ApiError(format!("{name}: the picks: {e}")))?;
            if new_kind.is_leader() {
                kinds.push(new_kind);
                continue;
            }
            let found = values_of(&picks);
            let resolved = if kind.is_some() || found.iter().any(|f| f.kind == new_kind) {
                new_kind
            } else {
                default_kind(&picks, &found).ok_or_else(no_single_value)?
            };
            check_kind(resolved, &found).map_err(|e| ApiError(format!("{name}: {e}")))?;
            kinds.push(resolved);
        }
        let mut index = 0;
        let changed = self.edit_annotations(&names, None, |annotation| {
            let kind = kinds[index];
            index += 1;
            // Another kind or other picks are drawn afresh.
            if kind != annotation.kind || picks_change {
                annotation.line_offset = None;
                annotation.text_offset = None;
                if matches!(
                    kind,
                    MeasureKind::Radius | MeasureKind::Diameter | MeasureKind::Angle
                ) {
                    annotation.plane = None;
                }
                // Between an angle and a length a tolerance means another
                // thing (degrees, millimetres, a fit).
                if kind.is_angle() != annotation.kind.is_angle() {
                    annotation.tolerance = None;
                }
            }
            annotation.kind = kind;
            if let Some(new_name) = &new_name {
                annotation.name.clone_from(new_name);
            }
            if let Some(color) = &color {
                annotation.color.clone_from(color);
            }
            if let Some(group) = &group {
                annotation.group.clone_from(group);
            }
            if let Some(visible) = visible {
                annotation.visible = visible;
            }
            if let Some(a) = &a {
                annotation.a = a.clone();
                // A direction and a point kept in the old pick's
                // coordinates mean nothing in the new one's: dropped unless
                // given again (below).
                if annotation.plane.as_ref().is_some_and(|p| p.pick.is_none()) {
                    annotation.plane = None;
                }
                annotation.at = None;
            }
            if let Some(b) = &b {
                annotation.b.clone_from(b);
            }
            if let Some(plane) = &plane {
                annotation.plane.clone_from(plane);
            }
            if let Some(offset) = line_offset {
                annotation.line_offset = offset;
            }
            if let Some(offset) = text_offset {
                annotation.text_offset = offset;
            }
            if let Some(format) = &format {
                format.apply(&mut annotation.format);
            }
            if let Some(tolerance) = &tolerance {
                annotation.tolerance.clone_from(tolerance);
            }
            if let Some(text) = &text {
                annotation.text = Some(text.clone());
            }
            if let Some(letter) = &letter {
                annotation.letter = Some(letter.trim().to_owned());
            }
            if let Some(frame) = &frame {
                annotation.frame = Some(frame.clone());
            }
            if let Some(on) = &on {
                annotation.on.clone_from(on);
            }
            if let Some(at) = at {
                annotation.at = at;
            }
            Ok(())
        })?;
        Ok(json!({"changed": changed}))
    }

    fn annotations_json(&self, measure_hidden: bool) -> Value {
        let unit = self.parameters().context().default_length_unit;
        let units = Units {
            symbol: unit.symbol(),
            millimetres: unit.millimetres(),
        };
        // Each uses at most two keys, its picks' and its plane's pick's
        // (mitcad#758); all of them stay measured.
        self.side_cache::<MeasureCache>()
            .hold(2 * self.annotations().len());
        Value::Array(self.laid_out(units, measure_hidden, None).0)
    }

    fn annotation_text_height_json(&self) -> Value {
        let automatic = automatic_text_height(self.annotated_size(None));
        let set = self.annotation_text_height();
        json!({
            "text_height": set,
            "in_use": set.unwrap_or(automatic),
            "automatic": automatic,
            "series": TEXT_HEIGHTS,
        })
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait AnnotationsLocal<K: Kernel> {
    /// The plane and the places a view suggests for what is not given: a
    /// note's plane square to the view (its text along the view's right);
    /// a length's plane a plane of its part where one holds it — a planar
    /// face at a pick, else a coordinate plane turned to hold its ends —,
    /// the one most facing the view (seen along its ends, the one most
    /// square to the view's up; mitcad#894); both kept in the first pick's
    /// own coordinates; a length's dimension line outside its bodies'
    /// extent in that plane, on the side away from them (the view's upper
    /// side when both are as clear), stacked outside parallel ones; the
    /// other places are left to the defaults in text heights (mitcad#902).
    fn place_for_view(
        &self,
        annotation: &mut Annotation,
        picks: &MeasuredPicks,
        view: &ViewHint,
    ) -> Result<(), ApiError>;

    /// What a dimension or a leader (a note, a datum, a frame) of these
    /// picks is drawn from; a leader's point as picked is in its pick's own
    /// coordinates.
    fn shape_of(&self, annotation: &Annotation, picks: &MeasuredPicks) -> Result<Shape, String>;

    /// One annotation of the `annotations` query, laid out after those in
    /// `laid` (mitcad#894: its automatic dimension line stacked outside
    /// theirs, a frame under a dimension at that dimension's text).
    fn annotation_json(
        &self,
        annotation: &Annotation,
        units: Units,
        measure_hidden: bool,
        laid: &mut Laid,
    ) -> Value;

    /// Every annotation of the `annotations` query in order, and the
    /// lengths' dimension lines laid out: each laid out after those before
    /// it, the frames under dimensions last. With `extra`, an annotation
    /// about to be added, laid out last of all as it will be once added
    /// (mitcad#1097).
    fn laid_out(
        &self,
        units: Units,
        measure_hidden: bool,
        extra: Option<&Annotation>,
    ) -> (Vec<Value>, Vec<Taken>);

    /// `add_annotation`'s preview (mitcad#1097): the annotation as the
    /// `annotations` query will give it once added — its name, label and
    /// drawing laid out after the others — with `"preview": true`; nothing
    /// is stored, no undo step made. Why it would be refused, as the add's
    /// error; why one that would be broken at once cannot be drawn.
    fn annotation_preview(&self, annotation: Annotation) -> Result<Value, ApiError>;

    /// The size of the design (mitcad#894, mitcad#902): the diagonal of the
    /// box of the bodies the shown annotations' picks are on, with
    /// `extra`'s (an annotation being added), and of the root's bodies;
    /// none when no body is known.
    fn annotated_size(&self, extra: Option<&MeasuredPicks>) -> Option<f64>;

    /// The height of the annotations' texts in use (mm): the design's, else
    /// the automatic one by [`AnnotationsLocal::annotated_size`].
    fn text_height_in_use(&self, extra: Option<&MeasuredPicks>) -> f64;

    /// Why `on` names no dimension a frame can stand under: none of that
    /// name, or a note, a datum or a frame (the frame itself among them).
    fn dimension_to_stand_under(&self, on: &str) -> Result<(), String>;

    /// Where a frame under a dimension stands: at that dimension's text, in
    /// its text's frame (`x` along it, `y` up), marked `attached` with the
    /// dimension's name; the application sets the frame just below the
    /// dimension line there; none while that dimension is hidden. Why
    /// not, when the dimension is not there.
    fn attached_drawing(&self, on: &str, laid: &Laid) -> Result<Option<Value>, String>;

    /// The normal and the text's direction its plane gives, where it has
    /// one: a direction kept, or the normal of the planar face or datum
    /// plane picked for it, as that pick is now.
    fn plane_hint(&self, annotation: &Annotation) -> Result<(Option<Vec3>, Option<Vec3>), String>;
}

impl<K: Kernel> AnnotationsLocal<K> for Document<K> {
    fn place_for_view(
        &self,
        annotation: &mut Annotation,
        picks: &MeasuredPicks,
        view: &ViewHint,
    ) -> Result<(), ApiError> {
        let direction = unit(view.direction)
            .ok_or_else(|| ApiError("a view's direction is not zero".to_owned()))?;
        let up = view.up.and_then(unit);
        let kind = annotation.kind;
        let planar = matches!(
            kind,
            MeasureKind::Distance
                | MeasureKind::Offset
                | MeasureKind::AxisDistance
                | MeasureKind::CenterDistance
                | MeasureKind::Note
                | MeasureKind::Datum
                | MeasureKind::Frame
                | MeasureKind::HoleCallout
                | MeasureKind::Material
                | MeasureKind::ProcessNote
                | MeasureKind::Diameter
        );
        let transform = self.pick_transform(&annotation.a)?;
        let shape = self.shape_of(annotation, picks).ok();
        if annotation.plane.is_none() && planar {
            let toward = scale(direction, -1.0);
            let normal = match shape {
                Some(Shape::Ends([p, q])) => unit(sub(q, p))
                    .map(|d| viewed_normal(d, &picks.faces, toward, up))
                    .unwrap_or(toward),
                _ => toward,
            };
            // A leader's text (a diameter's too, mitcad#902) reads along the
            // view's horizontal.
            let x = if kind.is_leader() || kind == MeasureKind::Diameter {
                up.and_then(|up| unit(cross(direction, up)))
            } else {
                None
            };
            annotation.plane = Some(AnnotationPlane {
                normal: Some(back(&transform, normal)),
                x: x.map(|x| back(&transform, x)),
                pick: None,
            });
        }
        let (Some(shape), Ok((normal, x))) = (shape, self.plane_hint(annotation)) else {
            return Ok(());
        };
        // Outside the lines already there (mitcad#894).
        let unit_of = self.parameters().context().default_length_unit;
        let units = Units {
            symbol: unit_of.symbol(),
            millimetres: unit_of.millimetres(),
        };
        let taken = match shape {
            Shape::Ends(_) => self.laid_out(units, false, None).1,
            _ => Vec::new(),
        };
        let height = self.text_height_in_use(Some(picks));
        let mut around = Around::of(height, picks, &taken);
        // The side of a length's line towards the eye (mitcad#902).
        around.prefer = Some(scale(direction, -1.0));
        let drawing = layout(annotation, &shape, normal, x, &around);
        // A length's line is kept on the side the view chose; the other
        // places stay the defaults, in text heights, which follow the
        // design's text height (mitcad#902: no pixels frozen here).
        if let Shape::Ends(_) = shape
            && annotation.line_offset.is_none()
        {
            // No body to keep clear of (datums): a few text heights off, on
            // the view's upper side.
            let size = if picks.extent.is_empty() {
                let side = match up {
                    Some(up) if dot(drawing.y, up) < 0.0 => -1.0,
                    _ => 1.0,
                };
                side * BARE_LINE_SHARE * height
            } else {
                drawing.line_offset
            };
            annotation.line_offset = Some(size);
        }
        Ok(())
    }

    fn shape_of(&self, annotation: &Annotation, picks: &MeasuredPicks) -> Result<Shape, String> {
        if annotation.kind.is_leader() {
            let at = match annotation.at {
                Some(at) => Some(
                    self.pick_transform(&annotation.a)
                        .map_err(|e| e.to_string())?
                        .apply_point(at),
                ),
                None => None,
            };
            // A hole callout's arrow on the hole's edge at its opening, on
            // the side it is seen from (mitcad#902).
            if annotation.kind == MeasureKind::HoleCallout {
                let toward = self.plane_hint(annotation).ok().and_then(|(n, _)| n);
                if let Some(anchor) = self.hole_callout_anchor(&annotation.a, at, toward) {
                    return Ok(Shape::Note {
                        anchor,
                        normal: None,
                        surface: None,
                    });
                }
            }
            return note_shape(picks, at);
        }
        values_of(picks)
            .iter()
            .find(|f| f.kind == annotation.kind)
            .map(Shape::of)
            .ok_or_else(|| format!("the picks have no {}", annotation.kind.key()))
    }

    fn annotation_json(
        &self,
        annotation: &Annotation,
        units: Units,
        measure_hidden: bool,
        laid: &mut Laid,
    ) -> Value {
        let mut value = serde_json::to_value(annotation).expect("annotations serialize");
        value["visible"] = json!(annotation.visible);
        if annotation.kind.is_leader() {
            // A frame may refer to the drawings' datum labels too: the
            // model's and the drawings' letters are one set (mitcad#745).
            let letters = self.all_datum_letters();
            let datums: Vec<&str> = letters.iter().map(String::as_str).collect();
            value["label"] = leader_label(annotation, units, &datums).to_json();
        }
        // A material or process note's words, written from its body as the
        // design is now (mitcad#1049); a pick that is no body's breaks it.
        if annotation.kind.is_body_note() {
            match self.body_note_lines(annotation.kind, &annotation.a) {
                Ok(lines) => {
                    value["label"] = Label {
                        text: lines.join(" / "),
                        lines,
                        ..Label::default()
                    }
                    .to_json();
                }
                Err(error) => {
                    value["error"] = json!(error);
                    return value;
                }
            }
        }
        // Hidden: not drawn, so not measured unless asked for (mitcad#716);
        // whether its picks are there is known without the kernel. A frame
        // under a dimension is broken only by that dimension.
        if !annotation.visible && !measure_hidden {
            value["measured"] = json!(false);
            let under = annotation.on.is_some() && annotation.kind == MeasureKind::Frame;
            if !under && let Err(error) = self.picks_key(&annotation.a, annotation.b.as_ref()) {
                value["error"] = json!(format!("a pick is not there: {error}"));
            }
            return value;
        }
        // A frame under a dimension: drawn, or broken, by that dimension
        // alone; its own pick (the dimension's first when it was put there)
        // may be stale.
        if let Some(on) = annotation
            .on
            .as_deref()
            .filter(|_| annotation.kind == MeasureKind::Frame)
        {
            match self.attached_drawing(on, laid) {
                Ok(Some(attached)) => value["drawing"] = attached,
                Ok(None) => {} // under a hidden dimension: not drawn either
                Err(error) => value["error"] = json!(error),
            }
            return value;
        }
        let picks = match self.measure_picks(&annotation.a, annotation.b.as_ref()) {
            Ok(picks) => picks,
            Err(error) => {
                value["error"] = json!(format!("a pick is not there: {error}"));
                return value;
            }
        };
        // A datum label or a frame on its leader that stands on a point (a
        // frame whose dimension was deleted, a file): broken, with why.
        if let Err(error) = feature_pick(annotation.kind, &annotation.a, &picks) {
            value["error"] = json!(error);
            return value;
        }
        // A hole callout's lines from its hole's feature as it is now
        // (mitcad#902): they follow its size, depth and thread.
        let mut callout_lines = 0;
        if annotation.kind == MeasureKind::HoleCallout {
            match self.hole_callout_lines(&annotation.a, &picks, units.drawing()) {
                Ok(lines) => {
                    callout_lines = lines.len();
                    value["label"] = Label {
                        text: lines.join(" / "),
                        lines,
                        ..Label::default()
                    }
                    .to_json();
                }
                Err(error) => {
                    value["error"] = json!(error);
                    return value;
                }
            }
        }
        let mut measured = None;
        if !annotation.kind.is_leader() {
            let found = values_of(&picks);
            let mut values = Map::new();
            for f in &found {
                values.insert(f.kind.key().to_owned(), json!(f.value));
            }
            value["values"] = Value::Object(values);
            match found.iter().find(|f| f.kind == annotation.kind) {
                Some(f) => {
                    value["value"] = json!(f.value);
                    add_drawn(&mut value, &f.drawn);
                    measured = Some(*f);
                }
                None => {
                    value["error"] = json!(format!(
                        "the picks have no {} now",
                        annotation.kind.key().replace('_', " ")
                    ));
                    return value;
                }
            }
        }
        let (normal, x) = match self.plane_hint(annotation) {
            Ok(hint) => hint,
            Err(error) => {
                value["error"] = json!(format!("the plane: {error}"));
                return value;
            }
        };
        let shape = match measured {
            Some(found) => Shape::of(&found),
            None => match self.shape_of(annotation, &picks) {
                Ok(shape) => shape,
                Err(error) => {
                    value["error"] = json!(error);
                    return value;
                }
            },
        };
        // A leader's text's size, for its place among the others' (a
        // diameter on a leader: its value's words).
        let leader = drawn_as_leader(annotation, normal);
        let text_size = leader.then(|| {
            let under = self
                .annotations()
                .iter()
                .filter(|a| a.visible && a.on.as_deref() == Some(annotation.name.as_str()))
                .count();
            let words = match measured {
                Some(found) if !annotation.kind.is_leader() => {
                    json!({"lines": [dimension_label(annotation, found.value, units).text]})
                }
                _ => value["label"].clone(),
            };
            leader_text_size(&words, laid.height, under)
        });
        let mut around = Around::of(laid.height, &picks, &laid.taken);
        around.boxes = &laid.boxes;
        around.text_size = text_size;
        let laid_out = layout(annotation, &shape, normal, x, &around);
        if let Some(text_box) = laid_out.text_box {
            laid.boxes.push(text_box);
        }
        // What those after it stack outside and stand under (mitcad#894).
        if let (Shape::Ends(_), Some([from, to])) = (shape, laid_out.dimension_lines.first()) {
            laid.taken.push(Taken {
                normal: laid_out.normal,
                x: laid_out.x,
                from: *from,
                to: *to,
            });
        }
        laid.texts.insert(annotation.name.clone(), laid_out.text);
        let mut drawing = laid_out.to_json();
        // A datum label's leader ends in a filled triangle on its feature
        // (ISO 5459), not an arrow; a note's or a frame's in a dot on a
        // face, an arrow on an edge or an outline (ISO 128-22, mitcad#902);
        // a hole callout's in an arrow on the hole's edge.
        match annotation.kind {
            MeasureKind::Datum => drawing["terminator"] = json!("datum"),
            MeasureKind::Note
            | MeasureKind::Frame
            | MeasureKind::Material
            | MeasureKind::ProcessNote
                if annotation.a.face.is_some() =>
            {
                drawing["terminator"] = json!("dot");
            }
            _ => {}
        }
        // A leader's text runs from the leader's end away from its arrow:
        // it starts there, or ends there when it lies on the arrow's other
        // side along the text (mitcad#902; a frame under a hole callout
        // takes the callout's).
        if leader && let (Some([tip, _]), [at, x, _]) = (laid_out.arrows.first(), laid_out.text) {
            // A datum's square centred on its stem's end (mitcad#902).
            let align = if annotation.kind == MeasureKind::Datum {
                "middle"
            } else if dot(sub(*tip, at), x) > 0.0 {
                "end"
            } else {
                "start"
            };
            drawing["align"] = json!(align);
            laid.leaders
                .insert(annotation.name.clone(), (callout_lines, align));
        }
        value["drawing"] = drawing;
        if let Some(found) = measured {
            let label = dimension_label(annotation, found.value, units);
            if let Some((upper, lower)) = label.deviations {
                value["deviations"] = json!({"upper": upper + 0.0, "lower": lower + 0.0});
            }
            value["label"] = label.to_json();
        }
        value
    }

    fn dimension_to_stand_under(&self, on: &str) -> Result<(), String> {
        match self.annotation(on) {
            Err(_) => Err(format!("there is no dimension {on} to stand under")),
            // A hole callout takes one too (mitcad#902).
            Ok(dimension) if !dimension.kind.takes_frame() => {
                Err(format!("{on} is no dimension or hole callout"))
            }
            Ok(_) => Ok(()),
        }
    }

    fn laid_out(
        &self,
        units: Units,
        measure_hidden: bool,
        extra: Option<&Annotation>,
    ) -> (Vec<Value>, Vec<Taken>) {
        let annotations: Vec<&Annotation> = self.annotations().iter().chain(extra).collect();
        // The texts' height with the one to be added among the shown ones
        // (`annotated_size` counts those on their own picks).
        let extra_picks = extra
            .filter(|a| a.visible && a.on.is_none())
            .and_then(|a| self.measure_picks(&a.a, a.b.as_ref()).ok());
        let mut laid = Laid {
            height: self.text_height_in_use(extra_picks.as_ref()),
            ..Laid::default()
        };
        let mut values = vec![Value::Null; annotations.len()];
        // Frames under dimensions last: they stand at their dimension's
        // text as it was laid out.
        let under = |a: &Annotation| a.kind == MeasureKind::Frame && a.on.is_some();
        for pass in [false, true] {
            for (value, annotation) in values.iter_mut().zip(annotations.iter().copied()) {
                if under(annotation) == pass {
                    *value = self.annotation_json(annotation, units, measure_hidden, &mut laid);
                    // A broken annotation is a warning about it
                    // (mitcad#1040); `error` keeps the reason.
                    if let Some(error) = value["error"].as_str().map(str::to_owned) {
                        add_annotation_notice(value, &annotation.name, &error);
                    }
                }
            }
        }
        (values, laid.taken)
    }

    fn annotation_preview(&self, annotation: Annotation) -> Result<Value, ApiError> {
        // As add_annotation stores it (its name, colour, group, frame), so
        // that what is shown is what is added.
        let annotation = self.annotation_to_add(annotation)?;
        let unit = self.parameters().context().default_length_unit;
        let units = Units::new(unit.symbol(), unit.millimetres());
        // As the `annotations` query holds them once it is added.
        self.side_cache::<MeasureCache>()
            .hold(2 * (self.annotations().len() + 1));
        let mut value = self
            .laid_out(units, false, Some(&annotation))
            .0
            .pop()
            .expect("the annotation previewed is laid out last");
        // One that would be broken at once has nothing to show: why, as
        // the preview's error (a command's answer keeps no `error` of its
        // own).
        if let Some(error) = value["error"].as_str() {
            return Err(ApiError(error.to_owned()));
        }
        value["preview"] = json!(true);
        Ok(value)
    }

    fn annotated_size(&self, extra: Option<&MeasuredPicks>) -> Option<f64> {
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        let mut grow = |picks: &MeasuredPicks| {
            for corner in &picks.extent {
                for ((l, h), c) in low.iter_mut().zip(high.iter_mut()).zip(corner) {
                    *l = l.min(*c);
                    *h = h.max(*c);
                }
            }
        };
        for annotation in self.annotations() {
            // A frame under a dimension: its pick is only an anchor.
            if !annotation.visible || annotation.on.is_some() {
                continue;
            }
            if let Ok(picks) = self.measure_picks(&annotation.a, annotation.b.as_ref()) {
                grow(&picks);
            }
        }
        if let Some(picks) = extra {
            grow(picks);
        }
        // The design's own bodies too (mitcad#902: the texts' height by the
        // design's size, not only by what is annotated).
        let kernel = self.kernel();
        for body in self.bodies() {
            if self.body_component(body.uid) != Some(mitcad_model::ids::ComponentUid::ROOT) {
                continue;
            }
            if let Ok(Some(bounds)) = kernel.bounding_box(body.shape) {
                for corner in bounds.corners() {
                    for ((l, h), c) in low.iter_mut().zip(high.iter_mut()).zip(corner) {
                        *l = l.min(c);
                        *h = h.max(c);
                    }
                }
            }
        }
        let size = distance(low, high);
        (size.is_finite() && size > 0.0).then_some(size)
    }

    fn text_height_in_use(&self, extra: Option<&MeasuredPicks>) -> f64 {
        self.annotation_text_height()
            .unwrap_or_else(|| automatic_text_height(self.annotated_size(extra)))
    }

    fn attached_drawing(&self, on: &str, laid: &Laid) -> Result<Option<Value>, String> {
        let dimension = self
            .annotation(on)
            .map_err(|_| format!("the dimension {on} is not there"))?;
        if !dimension.kind.takes_frame() {
            return Err(format!("{on} is no dimension"));
        }
        if !dimension.visible {
            return Ok(None);
        }
        // A hole callout laid out before (frames come last) unless broken,
        // its text too (its hole gone): a frame under it is broken with it.
        if dimension.kind == MeasureKind::HoleCallout && !laid.leaders.contains_key(on) {
            return Err(format!("{on} is broken"));
        }
        let picks = self
            .measure_picks(&dimension.a, dimension.b.as_ref())
            .map_err(|e| format!("{on}: a pick is not there: {e}"))?;
        let shape = self
            .shape_of(dimension, &picks)
            .map_err(|e| format!("{on}: {e}"))?;
        let (normal, x) = self
            .plane_hint(dimension)
            .map_err(|e| format!("{on}: the plane: {e}"))?;
        // Where its dimension's text was laid out in this pass, else alone.
        let [at, direction, up] = match laid.texts.get(on) {
            Some(text) => *text,
            None => {
                layout(
                    dimension,
                    &shape,
                    normal,
                    x,
                    &Around::of(laid.height, &picks, &laid.taken),
                )
                .text
            }
        };
        let v = |v: Vec3| v.map(|x| x + 0.0);
        let mut drawing = json!({
            "plane": {"origin": v(at), "x": v(direction), "y": v(up),
                      "normal": v(cross(direction, up))},
            "line_offset": 0.0,
            "text_offset": [0.0, 0.0],
            "extension_lines": [],
            "dimension_lines": [],
            "arcs": [],
            "arrows": [],
            "text": {"at": v(at), "direction": v(direction), "up": v(up)},
            "text_height": laid.height,
            "attached": on,
        });
        // Under a hole callout or a diameter on a leader (mitcad#902): below
        // its lines, on the same leader, its side of the text the leader's.
        if dimension.kind == MeasureKind::HoleCallout || laid.leaders.contains_key(on) {
            let (lines, align) = laid.leaders.get(on).copied().unwrap_or((1, "start"));
            drawing["under_lines"] = json!(lines.max(1));
            drawing["align"] = json!(align);
        }
        Ok(Some(drawing))
    }

    fn plane_hint(&self, annotation: &Annotation) -> Result<(Option<Vec3>, Option<Vec3>), String> {
        let Some(plane) = &annotation.plane else {
            return Ok((None, None));
        };
        if let Some(pick) = &plane.pick {
            let picks = self
                .measure_picks(pick, None)
                .map_err(|e| format!("its pick is not there: {e}"))?;
            return match picks.elements[0] {
                Some(Element::Plane { normal, .. }) => Ok((unit(normal), None)),
                _ => Err("its pick is not a planar face or a datum plane".to_owned()),
            };
        }
        // A direction kept in the first pick's own coordinates, which
        // follows it.
        let transform = self
            .pick_transform(&annotation.a)
            .map_err(|e| format!("its pick is not placed: {e}"))?;
        let placed = |v: Option<Vec3>| v.map(|v| transform.apply_vector(v)).and_then(unit);
        Ok((placed(plane.normal), placed(plane.x)))
    }
}

/// A direction of the design in a rigid transform's own coordinates (its
/// inverse is its transpose).
fn back(transform: &Transform, v: Vec3) -> Vec3 {
    std::array::from_fn(|c| (0..3).map(|r| transform.linear[r][c] * v[r]).sum::<f64>() + 0.0)
}

/// Why a datum label or a feature control frame cannot stand on this pick:
/// they mark a feature (a face, an edge, an axis, a plane), not a vertex
/// or a datum point; a hole callout a hole, a body's round face or edge
/// (mitcad#902). A note may point anywhere.
fn feature_pick(
    kind: MeasureKind,
    pick: &MeasurePick,
    picks: &MeasuredPicks,
) -> Result<(), String> {
    let what = match kind {
        MeasureKind::Datum => "a datum label",
        MeasureKind::Frame => "a feature control frame",
        // A hole callout points at a hole: a body's face or edge.
        MeasureKind::HoleCallout => {
            if pick.body.is_some() && (pick.face.is_some() || pick.edge.is_some()) {
                return Ok(());
            }
            return Err("a hole callout points at a hole: its round face or edge".to_owned());
        }
        _ => return Ok(()),
    };
    let point = pick.vertex.is_some()
        || (pick.datum.is_some() && matches!(picks.elements[0], Some(Element::Point(_))));
    if point {
        return Err(format!(
            "{what} stands on a face, an edge, an axis or a plane, not on a point"
        ));
    }
    Ok(())
}

/// A note's arrow point: a vertex; on a circle or a round face, the point
/// of it nearest where the note was picked (`at`, in the design's
/// coordinates); that point brought onto a planar face's plane or a
/// straight edge or an axis; else where it was picked.
fn note_shape(picks: &MeasuredPicks, at: Option<Vec3>) -> Result<Shape, String> {
    let element = picks.elements[0];
    let single = picks.single.as_ref();
    let normal = match element {
        Some(Element::Plane { normal, .. }) => unit(normal),
        _ => None,
    };
    let mut surface = normal;
    let anchor = if let Some(point) = single.and_then(|m| m.point) {
        point
    } else if let Some(circle) = single.and_then(|m| m.circle).filter(|c| c.radius > 0.0) {
        let center = circle.center;
        let radial = match unit(circle.axis) {
            Some(n) => {
                let foot = at.map_or(center, |p| add_scaled(center, n, dot(sub(p, center), n)));
                let radial = at
                    .and_then(|p| unit(sub(p, foot)))
                    .unwrap_or_else(|| across(n));
                (foot, radial)
            }
            // A sphere.
            None => (center, at.and_then(|p| unit(sub(p, center))).unwrap_or(Z)),
        };
        surface = Some(radial.1);
        add_scaled(radial.0, radial.1, circle.radius)
    } else {
        match element {
            Some(Element::Plane { origin, normal: n }) => match (at, unit(n)) {
                (Some(p), Some(n)) => add_scaled(p, n, -dot(sub(p, origin), n)),
                _ => origin,
            },
            Some(Element::Line { start, end }) => {
                let along = sub(end, start);
                let length = dot(along, along);
                match at {
                    Some(p) if length > 0.0 => {
                        let t = (dot(sub(p, start), along) / length).clamp(0.0, 1.0);
                        add_scaled(start, along, t)
                    }
                    _ => scale(add(start, end), 0.5),
                }
            }
            Some(Element::Axis {
                origin, direction, ..
            }) => match (at, unit(direction)) {
                (Some(p), Some(d)) => add_scaled(origin, d, dot(sub(p, origin), d)),
                _ => origin,
            },
            Some(Element::Point(point)) => point,
            None => at.ok_or_else(|| {
                "the note's pick has no point to point at: pick it where the arrow goes".to_owned()
            })?,
        }
    };
    Ok(Shape::Note {
        anchor,
        normal,
        surface,
    })
}

/// `v` without its part along the unit `along`, as a unit vector; none
/// when less than 1e-6 of it is left (it lies nearly along `along`).
fn square_to(v: Vec3, along: Vec3) -> Option<Vec3> {
    unit_above(sub(v, scale(along, dot(v, along))), 1e-6)
}

/// The coordinate axis a length's plane is square to when nothing else
/// says: the one least along the length (Z, then Y, then X when even).
fn default_normal(along: Vec3) -> Vec3 {
    let mut best = Z;
    let mut least = f64::INFINITY;
    for axis in [Z, Y, X] {
        let part = dot(along, axis).abs();
        if part < least - 1e-9 {
            least = part;
            best = axis;
        }
    }
    best
}

/// Lays an annotation out in its plane: `normal` and `x` are what its
/// plane gives (a direction kept or a pick's normal); a length's automatic
/// plane and line look at what is `around` it, and the text is the
/// design's height (mitcad#894).
fn layout(
    annotation: &Annotation,
    shape: &Shape,
    normal: Option<Vec3>,
    x: Option<Vec3>,
    around: &Around<'_>,
) -> Drawing {
    let h = around.height;
    let mut drawing = match *shape {
        Shape::Ends(ends) => layout_length(annotation, ends, normal, around),
        Shape::Circle {
            center,
            axis,
            radius,
        } => match normal {
            Some(normal) if drawn_as_leader(annotation, Some(normal)) => {
                layout_diameter_leader(annotation, center, axis, radius, normal, x, around)
            }
            _ => layout_circle(annotation, center, axis, radius, around),
        },
        Shape::Arc {
            center,
            from,
            to,
            reach,
        } => layout_angle(annotation, center, from, to, reach, h),
        Shape::Note {
            anchor,
            normal: own,
            surface,
        } => layout_note(annotation, anchor, normal.or(own), x, surface, around),
    };
    drawing.text_height = around.height;
    drawing
}

/// A length from `p` to `q`: in the plane through them square to the
/// normal (turned square to them; when none is given, a plane of the part
/// at a pick nearest the coordinate plane least along them,
/// [`automatic_normal`]), the dimension line parallel to them
/// `line_offset` across (when not set, outside the bodies' extent in the
/// plane on the side away from them and stacked outside parallel lines,
/// [`automatic_offset`]; a quarter of the length, at least 1 mm, without a
/// body), extension lines to it from the ends (from the picked face's own
/// outline where a pick is a plane), an arrow at each end pointing out, the text at
/// its middle; text moved past an end has the line run on under it.
fn layout_length(
    annotation: &Annotation,
    [p, q]: [Vec3; 2],
    hint: Option<Vec3>,
    around: &Around<'_>,
) -> Drawing {
    let length = distance(p, q);
    let along = unit(sub(q, p));
    let normal = match along {
        Some(d) => hint
            .and_then(|n| square_to(n, d))
            .unwrap_or_else(|| automatic_normal(d, around.faces)),
        None => hint.unwrap_or(Z),
    };
    let x = along.unwrap_or_else(|| across(normal));
    let y = cross(normal, x);
    // The line's side: the one set, else the one a view's eye is on, else
    // where the bodies reach less far.
    let side = match annotation.line_offset {
        Some(offset) if offset < 0.0 => -1.0,
        Some(_) => 1.0,
        None => preferred_side(p, y, around),
    };
    // An offset of two planes picked: its ends slide within both faces to
    // their edge on the line's side and to their far edge along the
    // plane's normal (away from the eye), so that the dimension stands
    // outside the part with its extension lines from the faces' edges
    // (mitcad#902).
    let [p, q] = slid_ends([p, q], normal, y, side, around);
    let offset = annotation
        .line_offset
        .unwrap_or_else(|| automatic_offset(normal, [p, q], x, y, side, around));
    let a = add_scaled(p, y, offset);
    let b = add_scaled(q, y, offset);
    // A plane picked: its extension line leaves the face picked at that
    // face's own outline on the line's side, not across the face, and
    // never from the edge of the bodies' box in mid-air (mitcad#902); an
    // edge or a vertex from its point. The application leaves a small gap
    // there.
    let start = |point: Vec3, plane: bool, outline: &[Vec3]| {
        if !plane || outline.is_empty() {
            return point;
        }
        let reach = outline
            .iter()
            .map(|corner| dot(sub(*corner, point), y))
            .fold(0.0f64, |far, s| {
                if offset >= 0.0 {
                    far.max(s)
                } else {
                    far.min(s)
                }
            });
        add_scaled(point, y, reach.clamp(offset.min(0.0), offset.max(0.0)))
    };
    let middle = scale(add(a, b), 0.5);
    let [u, v] = annotation.text_offset.unwrap_or([0.0, 0.0]);
    let foot = add_scaled(middle, x, u);
    let mut dimension_lines = vec![[a, b]];
    if u.abs() > length / 2.0 {
        dimension_lines.push([if u > 0.0 { b } else { a }, foot]);
    }
    Drawing {
        origin: p,
        x,
        y,
        normal,
        line_offset: offset,
        text_offset: [u, v],
        extension_lines: vec![
            [start(p, around.planes[0], around.outlines[0]), a],
            [start(q, around.planes[1], around.outlines[1]), b],
        ],
        dimension_lines,
        arcs: Vec::new(),
        arrows: vec![[a, scale(x, -1.0)], [b, x]],
        center: None,
        text: [add_scaled(foot, y, v), x, y],
        text_height: 0.0,
        text_box: None,
    }
}

/// A radius or a diameter in its circle's plane: the line from the centre
/// (a radius) or across the circle (a diameter) towards the text, which
/// stands one and a half radii out, and at least [`CIRCLE_SHARE`] text
/// heights `h` past the rim, unless moved, with an arrow on the rim
/// pointing out (both rims for a diameter) and the line run on to the text
/// outside the circle.
fn layout_circle(
    annotation: &Annotation,
    center: Vec3,
    axis: Vec3,
    radius: f64,
    around: &Around<'_>,
) -> Drawing {
    let h = around.height;
    let normal = unit(axis).unwrap_or(Z);
    let center = shown_centre(center, normal, around);
    let u0 = across(normal);
    let w = cross(normal, u0);
    let [u, v] = annotation.text_offset.unwrap_or([0.0, 0.0]);
    // Its text away from the bodies' middle, out of the part (mitcad#902):
    // a diameter's past their extent, on a leader from outside.
    let outward = extent_centre(around.extent)
        .and_then(|c| square_to(sub(center, c), normal))
        .unwrap_or(u0);
    let diameter = annotation.kind == MeasureKind::Diameter;
    let past = around
        .extent
        .iter()
        .map(|c| dot(sub(*c, center), outward))
        .fold(0.0f64, f64::max)
        + LEADER_CLEAR * h;
    let out = if diameter {
        past.max(radius + CIRCLE_SHARE * h)
    } else {
        (CIRCLE_TEXT * radius).max(radius + CIRCLE_SHARE * h)
    };
    let text_at = add_scaled(add_scaled(add_scaled(center, outward, out), u0, u), w, v);
    let toward = sub(text_at, center);
    let x = square_to(toward, normal).unwrap_or(u0);
    let rim = add_scaled(center, x, radius);
    let mut dimension_lines = Vec::new();
    let arrows;
    if diameter {
        // ISO 129-1's leader to a circle: an arrow on the rim pointing at
        // the centre, nothing across the face.
        arrows = vec![[rim, scale(x, -1.0)]];
        dimension_lines.push([text_at, rim]);
    } else {
        arrows = vec![[rim, x]];
        dimension_lines.push([center, rim]);
        if norm(toward) > radius {
            dimension_lines.push([rim, text_at]);
        }
    }
    let y = cross(normal, x);
    Drawing {
        origin: center,
        x: u0,
        y: w,
        normal,
        line_offset: radius,
        text_offset: [u, v],
        extension_lines: Vec::new(),
        dimension_lines,
        arcs: Vec::new(),
        arrows,
        // A radius' centre mark; a diameter's leader has none.
        center: (!diameter).then_some(center),
        text: [text_at, x, y],
        text_height: 0.0,
        text_box: None,
    }
}

/// A round face picked: its circle's centre at the face's end towards the
/// default view's eye, where its rim shows (mitcad#902).
fn shown_centre(center: Vec3, normal: Vec3, around: &Around<'_>) -> Vec3 {
    if around.outlines[0].is_empty() {
        return center;
    }
    let along = around.outlines[0].iter().map(|c| dot(*c, normal));
    let end = if dot(normal, ISO_EYE) >= 0.0 {
        along.fold(f64::NEG_INFINITY, f64::max)
    } else {
        along.fold(f64::INFINITY, f64::min)
    };
    add_scaled(center, normal, end - dot(center, normal))
}

/// Whether an annotation is drawn as a leader with its text on a landing:
/// a note, a datum label, a frame, a hole callout, and a diameter given a
/// plane of its own (one added in a view, mitcad#902: ISO 129-1's leader
/// to a circle, its text level in that plane).
fn drawn_as_leader(annotation: &Annotation, plane: Option<Vec3>) -> bool {
    annotation.kind.is_leader() || (annotation.kind == MeasureKind::Diameter && plane.is_some())
}

/// A diameter in a plane of its own (mitcad#902): a leader from outside to
/// its circle's rim, at the rim's point that lies out from the bodies'
/// middle as seen in that plane, its arrow on the rim, its text on a
/// landing as a note's ([`layout_note`]); no centre mark.
fn layout_diameter_leader(
    annotation: &Annotation,
    center: Vec3,
    axis: Vec3,
    radius: f64,
    plane: Vec3,
    x: Option<Vec3>,
    around: &Around<'_>,
) -> Drawing {
    let axis = unit(axis).unwrap_or(Z);
    let center = shown_centre(center, axis, around);
    let x = x
        .and_then(|x| square_to(x, plane))
        .or_else(|| square_to(X, plane))
        .unwrap_or(Y);
    let y = cross(plane, x);
    let out = extent_centre(around.extent)
        .map(|c| sub(center, c))
        .unwrap_or(y);
    let seen = add(scale(x, dot(out, x)), scale(y, dot(out, y)));
    let radial = square_to(seen, axis)
        .or_else(|| square_to(y, axis))
        .unwrap_or_else(|| across(axis));
    let rim = add_scaled(center, radial, radius);
    let mut drawing = layout_note(annotation, rim, Some(plane), Some(x), Some(radial), around);
    drawing.line_offset = radius;
    drawing
}

/// The normal of a length's plane when none is given (mitcad#894): the
/// coordinate axis least along it ([`default_normal`]) turned square to it,
/// or where a planar face at a pick holds the length and is not parallel
/// to that, the face's nearest it.
fn automatic_normal(along: Vec3, faces: &[Vec3]) -> Vec3 {
    let Some(coordinate) = square_to(default_normal(along), along) else {
        return Z;
    };
    let mut best: Option<(f64, Vec3)> = None;
    for normal in faces
        .iter()
        .copied()
        .filter(|n| dot(*n, along).abs() < SQUARE)
    {
        let near = dot(normal, coordinate).abs();
        if best.is_none_or(|(b, _)| near > b + 1e-9) {
            best = Some((near, normal));
        }
    }
    match best {
        Some((near, normal)) if near < 1.0 - SQUARE => {
            if dot(normal, coordinate) < 0.0 {
                scale(normal, -1.0)
            } else {
                normal
            }
        }
        _ => coordinate,
    }
}

/// The normal of a length's plane a view suggests (mitcad#894): of the
/// planes of the part that hold it — the planar faces at its picks, then
/// the coordinate planes turned to hold it — the one most facing the view
/// (`toward` the eye); seen edge on all of them (along the length), the
/// one most square to the view's `up`. It faces the eye, or the view's up
/// when edge on.
fn viewed_normal(along: Vec3, faces: &[Vec3], toward: Vec3, up: Option<Vec3>) -> Vec3 {
    let candidates: Vec<Vec3> = faces
        .iter()
        .copied()
        .filter(|n| dot(*n, along).abs() < SQUARE)
        .chain([Z, Y, X].iter().filter_map(|axis| square_to(*axis, along)))
        .collect();
    let pick = |score: &dyn Fn(Vec3) -> f64| {
        let mut best: Option<(f64, Vec3)> = None;
        for normal in &candidates {
            let value = score(*normal);
            if best.is_none_or(|(b, _)| value > b + 1e-9) {
                best = Some((value, *normal));
            }
        }
        best
    };
    // As facing (an isometric view sees a box's faces alike): the one most
    // along the view's up, a horizontal plane under a view with Z up, as a
    // drawing lays its lengths (mitcad#902).
    let facing =
        pick(&|n: Vec3| dot(n, toward).abs() + 1e-3 * up.map_or(0.0, |up| dot(n, up).abs()));
    let chosen = match facing {
        Some((_, normal)) if dot(normal, toward).abs() >= SQUARE => normal,
        _ => match up.and_then(|up| pick(&|n: Vec3| dot(n, up).abs())) {
            Some((_, normal)) => normal,
            None => return square_to(toward, along).unwrap_or(toward),
        },
    };
    let side = dot(chosen, toward);
    let flip = if side.abs() > 1e-9 {
        side < 0.0
    } else {
        up.is_some_and(|up| dot(chosen, up) < 0.0)
    };
    if flip { scale(chosen, -1.0) } else { chosen }
}

/// The middle of the bodies' extent (the corners of their boxes); none
/// without a body.
fn extent_centre(extent: &[Vec3]) -> Option<Vec3> {
    if extent.is_empty() {
        return None;
    }
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for corner in extent {
        for ((l, h), c) in low.iter_mut().zip(high.iter_mut()).zip(corner) {
            *l = l.min(*c);
            *h = h.max(*c);
        }
    }
    Some(scale(add(low, high), 0.5))
}

/// How far the bodies' extent reaches from `p` along `y` either way (both
/// at least 0): the farthest corner on each side.
fn extent_across(extent: &[Vec3], p: Vec3, y: Vec3) -> (f64, f64) {
    extent
        .iter()
        .fold((0.0f64, 0.0f64), |(above, below), corner| {
            let s = dot(sub(*corner, p), y);
            (above.max(s), below.max(-s))
        })
}

/// The side of a length's automatic dimension line along `y` (+1 or -1):
/// the one towards the eye of the view it was added in (`prefer`; a line
/// in front of the part stands outside it on the picture, mitcad#902), else
/// the side its bodies reach less far past its ends, else `y`'s.
fn preferred_side(p: Vec3, y: Vec3, around: &Around<'_>) -> f64 {
    if let Some(toward) = around.prefer {
        let s = dot(y, toward);
        if s.abs() > 1e-6 {
            return if s < 0.0 { -1.0 } else { 1.0 };
        }
    }
    let (above, below) = extent_across(around.extent, p, y);
    if around.extent.is_empty() || (above - below).abs() <= 1e-9 || above < below {
        1.0
    } else {
        -1.0
    }
}

/// The ends of a length between two planes picked, slid together within
/// both faces' outlines (the corners of their boxes): along `y` to their
/// common edge on `side`, along the plane's `normal` to their common far
/// edge (the plane faces the eye: away from it). Unchanged for other picks,
/// or where the faces share no edge that way.
fn slid_ends(
    [p, q]: [Vec3; 2],
    normal: Vec3,
    y: Vec3,
    side: f64,
    around: &Around<'_>,
) -> [Vec3; 2] {
    let [a, b] = around.outlines;
    if !(around.planes[0] && around.planes[1]) || a.is_empty() || b.is_empty() {
        return [p, q];
    }
    let range = |outline: &[Vec3], d: Vec3| {
        outline
            .iter()
            .map(|c| dot(*c, d))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| {
                (lo.min(t), hi.max(t))
            })
    };
    // Where both faces are, along a direction: none when they do not
    // overlap.
    let common = |d: Vec3| {
        let ((alo, ahi), (blo, bhi)) = (range(a, d), range(b, d));
        let (lo, hi) = (alo.max(blo), ahi.min(bhi));
        (lo <= hi + 1e-9).then_some((lo, hi))
    };
    let mut shift = [0.0; 3];
    if let Some((lo, hi)) = common(y) {
        let level = if side > 0.0 { hi } else { lo };
        shift = add_scaled(shift, y, level - dot(p, y));
    }
    if let Some((lo, _)) = common(normal) {
        shift = add_scaled(shift, normal, lo - dot(p, normal));
    }
    [add(p, shift), add(q, shift)]
}

/// A length's dimension line placed automatically (mitcad#894), signed
/// along `y` from its ends: on `side`, [`CLEAR_SHARE`] text heights past
/// its bodies' extent; without a body a quarter of the length, at least 1
/// mm; then [`stacked`] outside parallel lines there.
fn automatic_offset(
    normal: Vec3,
    [p, q]: [Vec3; 2],
    x: Vec3,
    y: Vec3,
    side: f64,
    around: &Around<'_>,
) -> f64 {
    let base = if around.extent.is_empty() {
        side * (LINE_SHARE * distance(p, q)).max(1.0)
    } else {
        let (above, below) = extent_across(around.extent, p, y);
        let clear = CLEAR_SHARE * around.height;
        if side > 0.0 {
            above + clear
        } else {
            -(below + clear)
        }
    };
    stacked(base, normal, [p, q], x, y, around)
}

/// A dimension line `offset` from a length's ends along `y`, moved out on
/// its side past the parallel lines laid out before it that it would come
/// within [`STACK_SHARE`] text heights of, where they overlap it along the
/// length.
fn stacked(
    offset: f64,
    normal: Vec3,
    [p, q]: [Vec3; 2],
    x: Vec3,
    y: Vec3,
    around: &Around<'_>,
) -> f64 {
    let h = around.height;
    let side = if offset < 0.0 { -1.0 } else { 1.0 };
    let length = dot(sub(q, p), x);
    let parallel = |a: Vec3, b: Vec3| dot(a, b).abs() > 1.0 - SQUARE;
    let mut lines: Vec<f64> = around
        .taken
        .iter()
        .filter(|t| parallel(t.normal, normal) && parallel(t.x, x))
        .filter(|t| {
            let (u, v) = (dot(sub(t.from, p), x), dot(sub(t.to, p), x));
            u.min(v) < length.max(0.0) + h && u.max(v) > length.min(0.0) - h
        })
        .map(|t| side * dot(sub(t.from, p), y))
        .filter(|s| *s > 1e-9)
        .collect();
    lines.sort_by(f64::total_cmp);
    let mut at = offset.abs();
    for line in lines {
        if (line - at).abs() < STACK_SHARE * h - 1e-9 {
            at = line + STACK_SHARE * h;
        }
    }
    side * at
}

/// An angle in the plane of its rays: an arc about the corner from the
/// first ray to the second, `line_offset` out (half the distance to the
/// nearer pick, else [`ARC_SHARE`] text heights `h`, when not set), the
/// rays from the corner to it as extension lines, an arrow at each end of
/// the arc pointing out along it, the text on the arc's middle running
/// along it. Parallel picks have no sector: the text at the corner.
fn layout_angle(
    annotation: &Annotation,
    center: Vec3,
    from: Vec3,
    to: Vec3,
    reach: Option<f64>,
    h: f64,
) -> Drawing {
    let radius = annotation
        .line_offset
        .unwrap_or_else(|| reach.unwrap_or(ARC_SHARE * h));
    let [u, v] = annotation.text_offset.unwrap_or([0.0, 0.0]);
    let sector = unit(from).zip(unit(to)).and_then(|(f, t)| {
        let angle = dot(f, t).clamp(-1.0, 1.0).acos();
        (angle > 1e-6).then(|| {
            let w = square_to(t, f).unwrap_or_else(|| across(f));
            (f, t, w, angle)
        })
    });
    let Some((f, t, w, angle)) = sector else {
        let x = unit(from).unwrap_or(X);
        let normal = square_to(default_normal(x), x).unwrap_or(Z);
        let y = cross(normal, x);
        return Drawing {
            origin: center,
            x,
            y,
            normal,
            line_offset: radius,
            text_offset: [u, v],
            extension_lines: Vec::new(),
            dimension_lines: Vec::new(),
            arcs: Vec::new(),
            arrows: Vec::new(),
            center: None,
            text: [add_scaled(add_scaled(center, x, u), y, v), x, y],
            text_height: 0.0,
            text_box: None,
        };
    };
    let normal = cross(f, w);
    let half = angle / 2.0;
    let middle = add(scale(f, half.cos()), scale(w, half.sin()));
    let along = add(scale(f, -half.sin()), scale(w, half.cos()));
    let start = add_scaled(center, f, radius);
    let end = add_scaled(
        center,
        add(scale(f, angle.cos()), scale(w, angle.sin())),
        radius,
    );
    let end_tangent = add(scale(f, -angle.sin()), scale(w, angle.cos()));
    let text_at = add_scaled(
        add_scaled(add_scaled(center, middle, radius), along, u),
        middle,
        v,
    );
    Drawing {
        origin: center,
        x: along,
        y: middle,
        normal,
        line_offset: radius,
        text_offset: [u, v],
        extension_lines: vec![[center, start], [center, end]],
        dimension_lines: Vec::new(),
        arcs: vec![ArcDrawn {
            center,
            radius,
            from: f,
            to: t,
            normal,
        }],
        arrows: vec![[start, scale(w, -1.0)], [end, end_tangent]],
        center: None,
        text: [text_at, along, middle],
        text_height: 0.0,
        text_box: None,
    }
}

/// A note (a datum label, a frame, a hole callout): the leader from the
/// text to its point with an arrow there, in the plane square to the
/// normal (the plane picked or kept, else its pick's face, else XY), the
/// text `text_offset` from the point along and across ([`NOTE_SHARE`] text
/// heights `h` when not set), running along `x` (the direction kept, else
/// X or Y in the plane).
fn layout_note(
    annotation: &Annotation,
    mut anchor: Vec3,
    normal: Option<Vec3>,
    x: Option<Vec3>,
    surface: Option<Vec3>,
    around: &Around<'_>,
) -> Drawing {
    let h = around.height;
    let normal = normal.unwrap_or(Z);
    let x = x
        .and_then(|x| square_to(x, normal))
        .or_else(|| square_to(X, normal))
        .or_else(|| square_to(Y, normal))
        .unwrap_or(X);
    let y = cross(normal, x);
    let landing = LANDING_SHARE * h;
    let in_plane = |p: Vec3| [dot(p, x), dot(p, y)];
    // A datum label (ISO 5459) stands on a short stem square to its surface,
    // its letter's square centred past the stem's end (mitcad#902); on a
    // planar face, its triangle on the face's outline where the stem
    // leaves it, the stem square to that outline in the label's plane.
    let stem = (annotation.kind == MeasureKind::Datum && annotation.text_offset.is_none())
        .then(|| surface.and_then(|s| square_to(s, normal)))
        .flatten()
        .map(|s| {
            match surface.and_then(|face| outline_exit(anchor, s, face, around.outlines[0], x, y)) {
                Some((foot, out)) => {
                    anchor = foot;
                    out
                }
                None => s,
            }
        });
    let (mut text_at, mut knee) = match (annotation.text_offset, extent_centre(around.extent)) {
        (Some([u, v]), _) => {
            let text_at = add_scaled(add_scaled(anchor, x, u), y, v);
            let side = if u < 0.0 { -1.0 } else { 1.0 };
            // Moved near its point, the landing shortens: the knee never
            // past the point along the text, where the leader would double
            // back to it at an acute bend (mitcad#1168).
            let landing = landing.min(LANDING_REACH * u.abs());
            let knee = if annotation.kind == MeasureKind::Datum {
                text_at
            } else {
                add_scaled(text_at, x, -side * landing)
            };
            (text_at, knee)
        }
        (None, _) if stem.is_some() => {
            let s = stem.unwrap_or(y);
            let knee = add_scaled(anchor, s, DATUM_STEM * h);
            // The square's middle where its side (half of it h away) meets
            // the stem's end.
            let reach = h / dot(s, x).abs().max(dot(s, y).abs()).max(0.5);
            (add_scaled(knee, s, reach), knee)
        }
        (None, Some(middle)) => {
            // The shortest way out of the part (mitcad#902): of the
            // directions up and out to either side (and down and out, the
            // lengths' side, less wanted), the one that leaves the bodies' extent soonest
            // without crossing a leader or a text laid out before it; its
            // own side first.
            let own = if dot(sub(anchor, middle), x) < 0.0 {
                -1.0
            } else {
                1.0
            };
            // Oblique only, as a drawing's leaders (mitcad#902): never along
            // the text, which would read as a dimension line, nor straight
            // up.
            let mut best: Option<(f64, Vec3)> = None;
            for (sx, sy, cost) in [
                (own, 1.0, 0.0),
                (0.5 * own, 1.0, 0.3),
                (-own, 1.0, 1.0),
                (-0.5 * own, 1.0, 1.3),
                (own, -1.0, 3.0),
                (-own, -1.0, 4.0),
            ] {
                let Some(out) = unit(add(scale(x, sx), scale(y, sy))) else {
                    continue;
                };
                let reach = around
                    .extent
                    .iter()
                    .map(|c| dot(sub(*c, anchor), out))
                    .fold(0.0f64, f64::max)
                    + LEADER_CLEAR * h;
                let knee = add_scaled(anchor, out, reach);
                let leader = [in_plane(anchor), in_plane(knee)];
                let crossings = around
                    .boxes
                    .iter()
                    .filter(|b| parallel_boxes(b, normal, x))
                    .filter(|b| crosses(leader, b.leader) || crosses_rect(leader, b.low, b.high))
                    .count();
                let score = reach + (cost + 20.0 * crossings as f64) * h;
                if best.is_none_or(|(b, _)| score < b - 1e-9) {
                    best = Some((score, out));
                }
            }
            let out = best.map_or(y, |(_, out)| out);
            let reach = around
                .extent
                .iter()
                .map(|c| dot(sub(*c, anchor), out))
                .fold(0.0f64, f64::max)
                + LEADER_CLEAR * h;
            let knee = add_scaled(anchor, out, reach);
            let side = if dot(out, x) < 0.0 { -1.0 } else { 1.0 };
            (add_scaled(knee, x, side * landing), knee)
        }
        (None, None) => {
            let text_at = add_scaled(
                add_scaled(anchor, x, NOTE_SHARE[0] * h),
                y,
                NOTE_SHARE[1] * h,
            );
            (text_at, add_scaled(text_at, x, -landing))
        }
    };
    // Its text's place: the text runs away from the knee (a datum's square
    // centred). Placed automatically, it moves on past the leaders' texts
    // laid out before it in a parallel plane that it would overlap.
    let centred = stem.is_some();
    let side = if dot(sub(text_at, knee), x) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let text_box = around.text_size.map(|[width, height]| {
        // A note's first line level with the landing, the rest below it;
        // a datum's square centred.
        let hang = if centred { height / 2.0 } else { 0.8 * h };
        let rect = |at: Vec3, knee: Vec3| {
            let mut placed = text_rect(
                at,
                x,
                y,
                if centred { 0.0 } else { side },
                [width, height, hang],
                normal,
            );
            placed.leader = [in_plane(anchor), in_plane(knee)];
            placed
        };
        let mut placed = rect(text_at, knee);
        if annotation.text_offset.is_none() && !around.extent.is_empty() {
            let margin = 0.5 * h;
            // Up, or along a datum's stem.
            let away = stem.unwrap_or(y);
            for _ in 0..around.boxes.len() + 1 {
                let Some(push) =
                    around
                        .boxes
                        .iter()
                        .filter(|b| overlap(b, &placed, margin))
                        .map(|b| {
                            // How far along `away` it moves to clear that box.
                            let a = in_plane(away);
                            let need = |lo: f64, hi: f64, own_lo: f64, own_hi: f64, d: f64| {
                                if d > 1e-9 {
                                    (hi + margin - own_lo) / d
                                } else if d < -1e-9 {
                                    (lo - margin - own_hi) / d
                                } else {
                                    f64::INFINITY
                                }
                            };
                            need(b.low[0], b.high[0], placed.low[0], placed.high[0], a[0]).min(
                                need(b.low[1], b.high[1], placed.low[1], placed.high[1], a[1]),
                            )
                        })
                        .filter(|d| d.is_finite())
                        .reduce(f64::max)
                else {
                    break;
                };
                text_at = add_scaled(text_at, away, push);
                knee = add_scaled(knee, away, push);
                placed = rect(text_at, knee);
            }
            // Nor on a length's dimension line laid out before it, as seen
            // in its plane: on past it, half a text height at a time.
            let low = |b: &TextBox| [b.low[0] - margin, b.low[1] - margin];
            let high = |b: &TextBox| [b.high[0] + margin, b.high[1] + margin];
            for _ in 0..40 {
                let on_line = around.taken.iter().any(|t| {
                    crosses_rect(
                        [in_plane(t.from), in_plane(t.to)],
                        low(&placed),
                        high(&placed),
                    )
                });
                if !on_line {
                    break;
                }
                text_at = add_scaled(text_at, away, 0.5 * h);
                knee = add_scaled(knee, away, 0.5 * h);
                placed = rect(text_at, knee);
            }
        }
        placed
    });
    let pointing = unit(sub(anchor, knee))
        .or_else(|| unit(sub(anchor, text_at)))
        .unwrap_or_else(|| scale(y, -1.0));
    let offset = sub(text_at, anchor);
    // A datum's stem alone (its square stands at the stem's end).
    let dimension_lines = if knee == text_at || centred {
        vec![[knee, anchor]]
    } else {
        // The leader to the point first, then the landing.
        vec![[knee, anchor], [text_at, knee]]
    };
    Drawing {
        origin: anchor,
        x,
        y,
        normal,
        line_offset: 0.0,
        text_offset: [dot(offset, x) + 0.0, dot(offset, y) + 0.0],
        extension_lines: Vec::new(),
        dimension_lines,
        arcs: Vec::new(),
        arrows: vec![[anchor, pointing]],
        center: None,
        text: [text_at, x, y],
        text_height: 0.0,
        text_box,
    }
}

/// Where a datum label's stem leaves a planar face (mitcad#902): from
/// `anchor` along `s` (in the label's plane of axes `x`, `y`), the point on
/// the face's outline (the convex outline of its box's corners seen in that
/// plane) where it goes out, and the stem's direction there, square to that
/// outline in the plane and outward; none when the face (of normal `face`)
/// is not planar or not seen.
fn outline_exit(
    anchor: Vec3,
    s: Vec3,
    face: Vec3,
    outline: &[Vec3],
    x: Vec3,
    y: Vec3,
) -> Option<(Vec3, Vec3)> {
    let face = unit(face)?;
    let size = outline
        .iter()
        .map(|c| distance(*c, anchor))
        .fold(0.0f64, f64::max);
    // Planar: every corner in the face's plane through the anchor.
    if outline.len() < 3
        || size <= 0.0
        || outline
            .iter()
            .any(|c| dot(sub(*c, anchor), face).abs() > 1e-6 * size.max(1.0))
    {
        return None;
    }
    let flat = |p: Vec3| [dot(sub(p, anchor), x), dot(sub(p, anchor), y)];
    let mut points: Vec<([f64; 2], usize)> = outline
        .iter()
        .enumerate()
        .map(|(i, c)| (flat(*c), i))
        .collect();
    points.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])));
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    // The convex outline (Andrew's monotone chain).
    let mut hull: Vec<([f64; 2], usize)> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Vec<_> = if pass == 0 {
            points.clone()
        } else {
            points.iter().rev().copied().collect()
        };
        for p in ordered {
            while hull.len() >= start + 2
                && turn(hull[hull.len() - 2].0, hull[hull.len() - 1].0, p.0) <= 1e-12 * size * size
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    if hull.len() < 3 {
        return None;
    }
    let d = [dot(s, x), dot(s, y)];
    let mut best: Option<(f64, usize, f64)> = None;
    for i in 0..hull.len() {
        let (a, b) = (hull[i].0, hull[(i + 1) % hull.len()].0);
        let e = [b[0] - a[0], b[1] - a[1]];
        let det = e[0] * d[1] - d[0] * e[1];
        if det.abs() < 1e-12 {
            continue;
        }
        let t = (e[0] * a[1] - a[0] * e[1]) / det;
        let u = (d[0] * a[1] - d[1] * a[0]) / det;
        if t > -1e-9 && (-1e-9..=1.0 + 1e-9).contains(&u) && best.is_none_or(|(bt, _, _)| t > bt) {
            best = Some((t, i, u.clamp(0.0, 1.0)));
        }
    }
    let (_, i, u) = best?;
    let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
    let e = [b.0[0] - a.0[0], b.0[1] - a.0[1]];
    let length = e[0].hypot(e[1]);
    if length <= 0.0 {
        return None;
    }
    let mut n = [-e[1] / length, e[0] / length];
    if n[0] * d[0] + n[1] * d[1] < 0.0 {
        n = [-n[0], -n[1]];
    }
    let on = add_scaled(outline[a.1], sub(outline[b.1], outline[a.1]), u);
    let foot = add_scaled(on, face, -dot(sub(on, anchor), face));
    Some((foot, add(scale(x, n[0]), scale(y, n[1]))))
}

/// Whether a text box lies in a plane parallel to one of this normal and
/// `x`.
fn parallel_boxes(b: &TextBox, normal: Vec3, x: Vec3) -> bool {
    dot(b.normal, normal).abs() > 1.0 - SQUARE && dot(b.x, x) > 1.0 - SQUARE
}

/// Whether two segments (in a plane's coordinates) cross, not only touch.
fn crosses(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> bool {
    let side = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let (d1, d2) = (side(a[0], a[1], b[0]), side(a[0], a[1], b[1]));
    let (d3, d4) = (side(b[0], b[1], a[0]), side(b[0], b[1], a[1]));
    d1 * d2 < -1e-12 && d3 * d4 < -1e-12
}

/// Whether a segment runs through a rectangle.
fn crosses_rect(a: [[f64; 2]; 2], low: [f64; 2], high: [f64; 2]) -> bool {
    let inside = |p: [f64; 2]| p[0] > low[0] && p[0] < high[0] && p[1] > low[1] && p[1] < high[1];
    let corners = [
        [low[0], low[1]],
        [high[0], low[1]],
        [high[0], high[1]],
        [low[0], high[1]],
    ];
    inside(a[0]) || inside(a[1]) || (0..4).any(|i| crosses(a, [corners[i], corners[(i + 1) % 4]]))
}

/// The rectangle a leader's text takes in its plane: `width` along `x`
/// from `at` (back from it for `side` -1, centred on it for 0), `height`
/// down from `hang` above it.
fn text_rect(
    at: Vec3,
    x: Vec3,
    y: Vec3,
    side: f64,
    [width, height, hang]: [f64; 3],
    normal: Vec3,
) -> TextBox {
    let (u, v) = (dot(at, x), dot(at, y));
    let (left, right) = if side < 0.0 {
        (u - width, u)
    } else if side > 0.0 {
        (u, u + width)
    } else {
        (u - width / 2.0, u + width / 2.0)
    };
    TextBox {
        normal,
        x,
        low: [left, v + hang - height],
        high: [right, v + hang],
        leader: [[u, v], [u, v]],
    }
}

/// Whether two leaders' texts in parallel planes overlap, `margin` apart at
/// least.
fn overlap(a: &TextBox, b: &TextBox, margin: f64) -> bool {
    let parallel = parallel_boxes(a, b.normal, b.x);
    parallel
        && a.low[0] < b.high[0] + margin
        && b.low[0] < a.high[0] + margin
        && a.low[1] < b.high[1] + margin
        && b.low[1] < a.high[1] + margin
}

/// The size (mm: width, height) of a leader's text by its label's words in
/// text heights `h` (ISO 3098 type B's narrow letters, a frame's cells two
/// heights high), with the frames under it (`under`).
fn leader_text_size(label: &Value, h: f64, under: usize) -> [f64; 2] {
    let chars = |t: &str| t.chars().count() as f64;
    let (width, height) = if let Some(lines) = label["lines"].as_array() {
        let widest = lines
            .iter()
            .map(|l| chars(l.as_str().unwrap_or("")))
            .fold(0.0f64, f64::max);
        (widest * 0.75 * h, lines.len().max(1) as f64 * 1.6 * h)
    } else if let Some(cells) = label["frame"].as_array() {
        let width = cells
            .iter()
            .map(|c| (chars(c.as_str().unwrap_or("")) * 0.75 * h + h).max(2.0 * h))
            .sum::<f64>();
        (width, 2.0 * h)
    } else {
        (2.0 * h, 2.0 * h)
    };
    // Frames hang below the lines.
    let frames = under as f64;
    [
        width.max(if under > 0 { 12.0 * h } else { 0.0 }),
        height + frames * 2.25 * h,
    ]
}

/// A number with `decimals` decimals, or up to `most` without trailing
/// zeros; never `-0`.
fn fixed(value: f64, decimals: Option<u8>, most: usize) -> String {
    let mut text = match decimals {
        Some(d) => format!("{:.*}", usize::from(d), value),
        None => {
            let mut text = format!("{value:.most$}");
            if text.contains('.') {
                while text.ends_with('0') {
                    text.pop();
                }
                if text.ends_with('.') {
                    text.pop();
                }
            }
            text
        }
    };
    if text.starts_with('-') && text[1..].chars().all(|c| c == '0' || c == '.') {
        text.remove(0);
    }
    text
}

/// A deviation with its sign; `0` alone.
fn signed(value: f64, decimals: Option<u8>) -> String {
    let text = fixed(value.abs(), decimals, DEVIATION_DECIMALS);
    if text.chars().all(|c| c == '0' || c == '.') {
        "0".to_owned()
    } else if value < 0.0 {
        format!("-{text}")
    } else {
        format!("+{text}")
    }
}

/// `before` and `rest` with a space between, either may be empty.
fn joined(before: &str, rest: &str) -> String {
    match (before.trim(), rest) {
        ("", rest) => rest.to_owned(),
        (before, "") => before.to_owned(),
        (before, rest) => format!("{before} {rest}"),
    }
}

/// A dimension's value (mm or radians) in words, in the document's length
/// unit (none written for millimetres) or degrees.
fn dimension_label(annotation: &Annotation, value: f64, units: Units) -> Label {
    let format = &annotation.format;
    let angle = annotation.kind.is_angle();
    let millimetres = if units.millimetres > 0.0 {
        units.millimetres
    } else {
        1.0
    };
    // The value shown, a tolerance (mm or degrees) in the unit shown, and
    // the unit written after each.
    let shown = if angle {
        value.to_degrees()
    } else {
        value / millimetres
    };
    let tolerance_scale = if angle { 1.0 } else { 1.0 / millimetres };
    let unit_text = if angle {
        "°".to_owned()
    } else if units.symbol == "mm" {
        String::new()
    } else {
        format!(" {}", units.symbol)
    };
    let deviation_unit = if angle { "°" } else { "" };
    let number = |v: f64| fixed(v, format.decimals, 3);
    // Two deviations with the decimals they need, the same for both (ISO
    // 129), not the value's.
    let pair = |upper: f64, lower: f64, most: usize| {
        let (upper, lower) = (upper * tolerance_scale, lower * tolerance_scale);
        let decimals = needed(upper, most).max(needed(lower, most));
        (
            format!("{}{deviation_unit}", signed(upper, Some(decimals))),
            format!("{}{deviation_unit}", signed(lower, Some(decimals))),
        )
    };
    let prefix = format
        .prefix
        .clone()
        .unwrap_or_else(|| annotation.kind.default_prefix().to_owned());
    let nominal = format!("{prefix}{}{unit_text}", number(shown));
    let mut label = Label::default();
    match (&format.replace, &annotation.tolerance) {
        (Some(replace), _) => replace.clone_into(&mut label.main),
        (None, None) => label.main = nominal,
        (None, Some(Tolerance::Symmetric { value })) => {
            let plus_minus = format!(
                "±{}{deviation_unit}",
                fixed(value * tolerance_scale, None, DEVIATION_DECIMALS)
            );
            label.main = format!("{nominal} {plus_minus}");
            label.tolerance = plus_minus;
            label.deviations = Some((*value, -*value));
        }
        (None, Some(Tolerance::Deviations { upper, lower })) => {
            let (up, down) = pair(*upper, *lower, DEVIATION_DECIMALS);
            label.main = nominal;
            label.upper = Some(up);
            label.lower = Some(down);
            label.deviations = Some((*upper, *lower));
        }
        (None, Some(Tolerance::Limits { upper, lower })) => {
            // The largest and the smallest size with as many decimals as
            // the value and the deviations need.
            let (largest, smallest) = (
                shown + upper * tolerance_scale,
                shown + lower * tolerance_scale,
            );
            let decimals = format
                .decimals
                .unwrap_or_else(|| needed(shown, 3))
                .max(needed(upper * tolerance_scale, DEVIATION_DECIMALS))
                .max(needed(lower * tolerance_scale, DEVIATION_DECIMALS));
            label.main = prefix;
            label.upper = Some(format!(
                "{}{unit_text}",
                fixed(largest, Some(decimals), DEVIATION_DECIMALS)
            ));
            label.lower = Some(format!(
                "{}{unit_text}",
                fixed(smallest, Some(decimals), DEVIATION_DECIMALS)
            ));
            label.deviations = Some((*upper, *lower));
        }
        (
            None,
            Some(Tolerance::Fit {
                code,
                deviations: shown_too,
            }),
        ) => {
            let code = code.trim();
            label.main = format!("{nominal} {code}");
            code.clone_into(&mut label.tolerance);
            // A fit is of a size as written (its range by the value shown,
            // not a hair over it); a radius' is its diameter's, and its
            // deviations are half the diameter's, as a radius.
            // (In millimetres only: rounded in another unit the size would
            // move by up to half its last decimal, a range off.)
            let written = if units.symbol == "mm" {
                fixed(shown, format.decimals, 3)
                    .parse::<f64>()
                    .map_or(value, |v| v * millimetres)
            } else {
                value
            };
            let radius = annotation.kind == MeasureKind::Radius;
            let size = if radius { 2.0 * written } else { written };
            match mitcad_model::iso286::deviations(code, size) {
                Ok((upper, lower)) => {
                    let (upper, lower) = if radius {
                        (upper / 2.0, lower / 2.0)
                    } else {
                        (upper, lower)
                    };
                    label.deviations = Some((upper, lower));
                    if *shown_too {
                        // A radius' halved deviations may need a fifth decimal.
                        let most = DEVIATION_DECIMALS + usize::from(radius);
                        let (up, down) = pair(upper, lower, most);
                        label.upper = Some(up);
                        label.lower = Some(down);
                        label.brackets = true;
                    }
                }
                Err(error) => label.warning = Some(error),
            }
        }
    }
    label.main = joined(&format.before, &label.main);
    format.after.trim().clone_into(&mut label.after);
    let stack = match (&label.upper, &label.lower) {
        (Some(upper), Some(lower)) => {
            let pair = format!("{upper}/{lower}");
            if label.tolerance.is_empty() || label.brackets {
                if label.brackets {
                    label.tolerance = format!("{} ({pair})", label.tolerance);
                } else {
                    label.tolerance.clone_from(&pair);
                }
            }
            if label.brackets {
                format!("({pair})")
            } else {
                pair
            }
        }
        _ => String::new(),
    };
    label.text = joined(&joined(&label.main, &stack), &label.after);
    label
}

/// The decimals a number needs, up to `most`: none for a whole one.
fn needed(value: f64, most: usize) -> u8 {
    let text = fixed(value.abs(), None, most);
    text.split_once('.')
        .map_or(0, |(_, decimals)| u8::try_from(decimals.len()).unwrap_or(0))
}

/// A leader's words: a note's lines, a datum's letter, a feature control
/// frame's cells (`datums`: the letters the design's datum labels have,
/// for a warning on one referred to that is not there).
fn leader_label(annotation: &Annotation, units: Units, datums: &[&str]) -> Label {
    match (annotation.kind, &annotation.frame) {
        (MeasureKind::Datum, _) => {
            let letter = annotation.letter.clone().unwrap_or_default();
            Label {
                text: letter.clone(),
                datum: Some(letter),
                ..Label::default()
            }
        }
        (MeasureKind::Frame, Some(frame)) => frame_label(frame, units, datums),
        _ => note_label(annotation),
    }
}

/// A feature control frame's cells (ISO 1101): the characteristic's
/// symbol; the tolerance, Ø before a cylindrical zone's, the material
/// condition after it, in the document's length unit (none written); each
/// datum referred to, in order. On one line, `|` between them.
fn frame_label(frame: &GeometricTolerance, units: Units, datums: &[&str]) -> Label {
    let per = if units.millimetres > 0.0 {
        units.millimetres
    } else {
        1.0
    };
    let mut tolerance = format!(
        "{}{}",
        if frame.diameter { "Ø" } else { "" },
        fixed(frame.value / per, None, DEVIATION_DECIMALS)
    );
    if let Some(modifier) = frame.modifier {
        tolerance.push(' ');
        tolerance.push_str(modifier.symbol());
    }
    let mut cells = vec![frame.characteristic.symbol().to_owned(), tolerance.clone()];
    cells.extend(frame.datums.iter().cloned());
    let missing: Vec<&str> = frame
        .datums
        .iter()
        .map(String::as_str)
        .filter(|d| !datums.contains(d))
        .collect();
    Label {
        text: cells.join(" | "),
        tolerance,
        warning: (!missing.is_empty()).then(|| format!("no datum label {}", missing.join(", "))),
        frame: cells,
        ..Label::default()
    }
}

/// The `geometric_characteristics` query: each characteristic a feature
/// control frame can have, in ISO 1101's order, with its symbol and its
/// rules: whether it refers to datums (`none`, `optional`, `required`),
/// takes a cylindrical zone (Ø) and a material condition (Ⓜ, Ⓛ). An editor
/// of frames offers them so; `add_annotation` and `edit_annotation` check
/// them.
pub(super) fn geometric_characteristics_json() -> Value {
    Value::Array(
        Characteristic::ALL
            .iter()
            .map(|c| {
                let datums = match c.datums() {
                    DatumNeed::None => "none",
                    DatumNeed::Optional => "optional",
                    DatumNeed::Required => "required",
                };
                json!({"key": c.key(), "symbol": c.symbol(), "datums": datums,
                       "diameter": c.takes_diameter(), "modifier": c.takes_modifier()})
            })
            .collect(),
    )
}

/// A note's text: its lines, and all of them on one line.
fn note_label(annotation: &Annotation) -> Label {
    let lines: Vec<String> = annotation
        .text
        .as_deref()
        .unwrap_or("")
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect();
    Label {
        text: lines.join(" / "),
        lines,
        ..Label::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} instead of {b}");
    }

    fn near_point(a: Vec3, b: Vec3) {
        for (x, y) in a.into_iter().zip(b) {
            near(x, y);
        }
    }

    fn dimension(kind: MeasureKind) -> Annotation {
        let pick = MeasurePick {
            datum: Some("xy".to_owned()),
            ..MeasurePick::default()
        };
        let mut annotation = Annotation::new("D", kind, "#d9480f", pick.clone());
        if !kind.single() {
            annotation.b = Some(pick);
        }
        annotation
    }

    fn text_of(annotation: &Annotation, value: f64) -> String {
        dimension_label(annotation, value, Units::MM).text
    }

    #[test]
    fn values_are_written_with_their_prefix_decimals_and_own_text() {
        let mut d = dimension(MeasureKind::Diameter);
        assert_eq!(text_of(&d, 20.0), "Ø20");
        assert_eq!(text_of(&d, 12.34567), "Ø12.346");
        d.format.decimals = Some(2);
        assert_eq!(text_of(&d, 20.0), "Ø20.00");
        d.format.prefix = Some(String::new());
        d.format.before = "4 x".to_owned();
        d.format.after = "THRU".to_owned();
        assert_eq!(text_of(&d, 20.0), "4 x 20.00 THRU");
        d.format.replace = Some("SEE NOTE".to_owned());
        d.tolerance = Some(Tolerance::Symmetric { value: 0.1 });
        assert_eq!(text_of(&d, 20.0), "4 x SEE NOTE THRU");
        let r = dimension(MeasureKind::Radius);
        assert_eq!(text_of(&r, 5.0), "R5");
        // Another length unit is written after the value.
        let cm = Units {
            symbol: "cm",
            millimetres: 10.0,
        };
        assert_eq!(dimension_label(&r, 25.0, cm).text, "R2.5 cm");
        // Angles in degrees.
        let mut a = dimension(MeasureKind::Angle);
        assert_eq!(text_of(&a, std::f64::consts::FRAC_PI_2), "90°");
        a.tolerance = Some(Tolerance::Symmetric { value: 0.5 });
        assert_eq!(text_of(&a, std::f64::consts::FRAC_PI_6), "30° ±0.5°");
    }

    #[test]
    fn tolerances_are_written_iso_129_style() {
        let mut d = dimension(MeasureKind::Distance);
        d.tolerance = Some(Tolerance::Symmetric { value: 0.1 });
        let label = dimension_label(&d, 40.0, Units::MM);
        assert_eq!(label.text, "40 ±0.1");
        assert_eq!(label.tolerance, "±0.1");
        assert_eq!(label.deviations, Some((0.1, -0.1)));

        d.tolerance = Some(Tolerance::Deviations {
            upper: 0.2,
            lower: -0.1,
        });
        let label = dimension_label(&d, 40.0, Units::MM);
        assert_eq!(label.main, "40");
        assert_eq!(label.upper.as_deref(), Some("+0.2"));
        assert_eq!(label.lower.as_deref(), Some("-0.1"));
        assert!(!label.brackets);
        assert_eq!(label.text, "40 +0.2/-0.1");
        d.tolerance = Some(Tolerance::Deviations {
            upper: 0.0,
            lower: -0.05,
        });
        assert_eq!(text_of(&d, 40.0), "40 0/-0.05");

        d.tolerance = Some(Tolerance::Limits {
            upper: 0.1,
            lower: -0.1,
        });
        let label = dimension_label(&d, 10.0, Units::MM);
        assert_eq!(label.main, "");
        assert_eq!(label.text, "10.1/9.9");
        assert_eq!(label.tolerance, "10.1/9.9");

        let mut hole = dimension(MeasureKind::Diameter);
        hole.tolerance = Some(Tolerance::Fit {
            code: "H7".to_owned(),
            deviations: false,
        });
        let label = dimension_label(&hole, 20.0, Units::MM);
        assert_eq!(label.text, "Ø20 H7");
        assert_eq!(label.tolerance, "H7");
        let (upper, lower) = label.deviations.unwrap();
        near(upper, 0.021);
        near(lower, 0.0);
        hole.tolerance = Some(Tolerance::Fit {
            code: "g6".to_owned(),
            deviations: true,
        });
        let label = dimension_label(&hole, 20.0, Units::MM);
        assert_eq!(label.text, "Ø20 g6 (-0.007/-0.020)");
        assert!(label.brackets);
        assert_eq!(label.tolerance, "g6 (-0.007/-0.020)");
        // A size the fit codes do not cover: the code with a warning.
        let label = dimension_label(&hole, 600.0, Units::MM);
        assert_eq!(label.text, "Ø600 g6");
        assert!(label.warning.unwrap().contains("500 mm"));
        // A radius' fit is of its diameter's size, half its deviations: R10 H7
        // is Ø20 H7's, as a radius.
        let mut radius = dimension(MeasureKind::Radius);
        radius.tolerance = Some(Tolerance::Fit {
            code: "H7".to_owned(),
            deviations: true,
        });
        assert_eq!(text_of(&radius, 10.0), "R10 H7 (+0.0105/0)");
        // Halved, a radius' deviations may need a fifth decimal.
        radius.tolerance = Some(Tolerance::Fit {
            code: "js6".to_owned(),
            deviations: true,
        });
        assert_eq!(text_of(&radius, 10.0), "R10 js6 (+0.00325/-0.00325)");
        // In inches the range is the measured size's: Ø10 mm (0.394 in as
        // written, 10.0076 mm) stays in the range up to 10 mm.
        let mut inch_hole = dimension(MeasureKind::Diameter);
        inch_hole.tolerance = Some(Tolerance::Fit {
            code: "H7".to_owned(),
            deviations: false,
        });
        let inches = Units {
            symbol: "in",
            millimetres: 25.4,
        };
        let label = dimension_label(&inch_hole, 10.0, inches);
        assert_eq!(label.text, "Ø0.394 in H7");
        let (upper, _) = label.deviations.unwrap();
        near(upper, 0.015);
        // The range by the size as written: a measured 10.0004 shown as 10
        // takes the range up to 10.
        hole.tolerance = Some(Tolerance::Fit {
            code: "H7".to_owned(),
            deviations: true,
        });
        assert_eq!(text_of(&hole, 10.0004), "Ø10 H7 (+0.015/0)");
        hole.format.decimals = Some(4);
        assert_eq!(text_of(&hole, 10.0004), "Ø10.0004 H7 (+0.018/0)");
        hole.format.decimals = None;
        // Tolerances keep their own decimals: none set or none on the value.
        let mut d = dimension(MeasureKind::Distance);
        d.format.decimals = Some(0);
        d.tolerance = Some(Tolerance::Symmetric { value: 0.1 });
        assert_eq!(text_of(&d, 40.0), "40 ±0.1");
        d.tolerance = Some(Tolerance::Limits {
            upper: 0.05,
            lower: -0.1,
        });
        assert_eq!(text_of(&d, 40.0), "40.05/39.90");
        d.format.decimals = Some(2);
        d.tolerance = Some(Tolerance::Deviations {
            upper: 0.1,
            lower: -0.25,
        });
        assert_eq!(text_of(&d, 40.0), "40.00 +0.10/-0.25");
    }

    #[test]
    fn a_frame_writes_its_zone_in_the_document_unit() {
        let frame = GeometricTolerance {
            characteristic: mitcad_model::Characteristic::Position,
            value: 0.254,
            diameter: true,
            modifier: Some(mitcad_model::MaterialCondition::M),
            datums: vec!["A".to_owned(), "C".to_owned()],
        };
        let inches = Units {
            symbol: "in",
            millimetres: 25.4,
        };
        let label = frame_label(&frame, inches, &["A"]);
        assert_eq!(label.text, "⌖ | Ø0.01 Ⓜ | A | C");
        assert_eq!(label.tolerance, "Ø0.01 Ⓜ");
        assert_eq!(label.warning.as_deref(), Some("no datum label C"));
        let millimetres = Units {
            symbol: "mm",
            millimetres: 1.0,
        };
        assert_eq!(
            frame_label(&frame, millimetres, &["A", "C"]).tolerance,
            "Ø0.254 Ⓜ"
        );
    }

    #[test]
    fn a_length_lies_in_its_plane_with_its_line_offset_and_text() {
        let mut d = dimension(MeasureKind::Distance);
        let ends = [[0.0, 0.0, 0.0], [40.0, 0.0, 0.0]];
        // No plane: square to Z (the axis least along X), the line a
        // quarter of the length off along +Y.
        let drawing = layout(&d, &Shape::Ends(ends), None, None, &Around::bare(3.5));
        near_point(drawing.normal, Z);
        near_point(drawing.x, X);
        near_point(drawing.y, Y);
        near(drawing.line_offset, 10.0);
        assert_eq!(
            drawing.extension_lines,
            vec![
                [[0.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
                [[40.0, 0.0, 0.0], [40.0, 10.0, 0.0]]
            ]
        );
        assert_eq!(
            drawing.dimension_lines,
            vec![[[0.0, 10.0, 0.0], [40.0, 10.0, 0.0]]]
        );
        assert_eq!(
            drawing.arrows,
            vec![
                [[0.0, 10.0, 0.0], [-1.0, 0.0, 0.0]],
                [[40.0, 10.0, 0.0], [1.0, 0.0, 0.0]]
            ]
        );
        near_point(drawing.text[0], [20.0, 10.0, 0.0]);
        // A plane square to Y (the front), the line below, the text moved
        // past the second end: the line runs on under it.
        d.line_offset = Some(-6.0);
        d.text_offset = Some([30.0, 2.0]);
        let drawing = layout(
            &d,
            &Shape::Ends(ends),
            Some([0.0, -1.0, 0.0]),
            None,
            &Around::bare(3.5),
        );
        near_point(drawing.normal, [0.0, -1.0, 0.0]);
        near_point(drawing.y, [0.0, 0.0, 1.0]);
        near_point(drawing.dimension_lines[0][0], [0.0, 0.0, -6.0]);
        assert_eq!(drawing.dimension_lines.len(), 2);
        near_point(drawing.dimension_lines[1][1], [50.0, 0.0, -6.0]);
        near_point(drawing.text[0], [50.0, 0.0, -4.0]);
        // A normal that is not square to the ends is turned square.
        let drawing = layout(
            &d,
            &Shape::Ends(ends),
            Some([1.0, 0.0, 1.0]),
            None,
            &Around::bare(3.5),
        );
        near_point(drawing.normal, Z);
    }

    #[test]
    fn circles_angles_and_notes_are_laid_out_in_their_planes() {
        // A diameter of 10 about Z through (5, 5, 0): along X (`across` of
        // Z), its text at least three text heights past the rim (mitcad#902:
        // 5 + 10.5), on a leader from outside.
        let d = dimension(MeasureKind::Diameter);
        let circle = Shape::Circle {
            center: [5.0, 5.0, 0.0],
            axis: [0.0, 0.0, 2.0],
            radius: 5.0,
        };
        let drawing = layout(&d, &circle, None, None, &Around::bare(3.5));
        let x = across(Z);
        near_point(drawing.x, x);
        near_point(drawing.text[0], add_scaled([5.0, 5.0, 0.0], x, 15.5));
        // A large circle: three text heights past its rim too.
        let large = Shape::Circle {
            center: [0.0, 0.0, 0.0],
            axis: Z,
            radius: 40.0,
        };
        near_point(
            layout(&d, &large, None, None, &Around::bare(3.5)).text[0],
            add_scaled([0.0; 3], x, 50.5),
        );
        // ISO 129-1's leader to a circle: from the text to the rim, its
        // arrow there pointing at the centre; nothing across the face, no
        // centre mark.
        assert_eq!(
            drawing.dimension_lines,
            vec![[
                add_scaled([5.0, 5.0, 0.0], x, 15.5),
                add_scaled([5.0, 5.0, 0.0], x, 5.0)
            ]]
        );
        assert_eq!(drawing.arrows.len(), 1);
        near_point(drawing.arrows[0][1], scale(x, -1.0));
        assert_eq!(drawing.center, None);
        // A radius with its text dragged inside: one arrow, no run-on.
        let mut r = dimension(MeasureKind::Radius);
        r.text_offset = Some([-12.0, 0.0]);
        let drawing = layout(&r, &circle, None, None, &Around::bare(3.5));
        assert_eq!(drawing.arrows.len(), 1);
        assert_eq!(drawing.dimension_lines.len(), 1);

        // 90 degrees between X and Y at the origin, the arc 8 out.
        let mut a = dimension(MeasureKind::Angle);
        a.line_offset = Some(8.0);
        let arc = Shape::Arc {
            center: [0.0, 0.0, 0.0],
            from: X,
            to: Y,
            reach: Some(3.0),
        };
        let drawing = layout(&a, &arc, None, None, &Around::bare(3.5));
        near_point(drawing.normal, Z);
        let s = std::f64::consts::FRAC_1_SQRT_2;
        near_point(drawing.text[0], [8.0 * s, 8.0 * s, 0.0]);
        near_point(drawing.arrows[0][0], [8.0, 0.0, 0.0]);
        near_point(drawing.arrows[0][1], [0.0, -1.0, 0.0]);
        near_point(drawing.arrows[1][0], [0.0, 8.0, 0.0]);
        near_point(drawing.arrows[1][1], [-1.0, 0.0, 0.0]);
        near(drawing.arcs[0].radius, 8.0);
        // Without its own radius: the picks' reach.
        a.line_offset = None;
        near(
            layout(&a, &arc, None, None, &Around::bare(3.5)).arcs[0].radius,
            3.0,
        );

        // A note on a point, in the plane square to Y, its text along X.
        let mut note = dimension(MeasureKind::Note);
        note.text = Some("Deburr".to_owned());
        let shape = Shape::Note {
            anchor: [1.0, 2.0, 3.0],
            normal: Some(Z),
            surface: Some(Z),
        };
        let drawing = layout(
            &note,
            &shape,
            Some([0.0, -1.0, 0.0]),
            None,
            &Around::bare(3.5),
        );
        near_point(drawing.x, X);
        near_point(drawing.y, Z);
        // Four text heights along, three across (mitcad#902).
        near_point(drawing.text[0], [15.0, 2.0, 13.5]);
        near_point(drawing.arrows[0][0], [1.0, 2.0, 3.0]);
        // The leader to the point from the knee of its landing, a text
        // height and a half before the text (mitcad#902).
        near_point(drawing.dimension_lines[1][1], [9.75, 2.0, 13.5]);
        near_point(drawing.dimension_lines[0][1], [1.0, 2.0, 3.0]);
        near_point(drawing.arrows[0][1], unit([-8.75, 0.0, -10.5]).unwrap());
        // Without a reach of its picks an angle's arc is five text heights
        // out.
        let mut free = dimension(MeasureKind::Angle);
        free.line_offset = None;
        let open = Shape::Arc {
            center: [0.0, 0.0, 0.0],
            from: X,
            to: Y,
            reach: None,
        };
        near(
            layout(&free, &open, None, None, &Around::bare(2.0)).arcs[0].radius,
            10.0,
        );
        // Without a plane of its own: its face's.
        let drawing = layout(&note, &shape, None, None, &Around::bare(3.5));
        near_point(drawing.normal, Z);
        near_point(drawing.y, Y);
        assert_eq!(note_label(&note).lines, ["Deburr"]);
    }

    #[test]
    fn numbers_are_written_without_minus_zero() {
        assert_eq!(fixed(-0.0001, None, 3), "0");
        assert_eq!(fixed(-0.0001, Some(2), 3), "0.00");
        assert_eq!(fixed(2.5, None, 3), "2.5");
        assert_eq!(fixed(2.0, None, 3), "2");
        assert_eq!(signed(0.0, Some(3)), "0");
        assert_eq!(signed(-0.021, None), "-0.021");
        assert_eq!(signed(0.021, Some(2)), "+0.02");
        assert_eq!(joined(" ", "x"), "x");
        assert_eq!(joined("a", ""), "a");
    }

    /// The corners of a 20 x 10 x 5 box from the origin.
    fn box_corners() -> Vec<Vec3> {
        let mut corners = Vec::new();
        for x in [0.0, 20.0] {
            for y in [0.0, 10.0] {
                for z in [0.0, 5.0] {
                    corners.push([x, y, z]);
                }
            }
        }
        corners
    }

    #[test]
    fn a_length_is_placed_outside_its_body_and_stacks_outside_parallel_ones() {
        // mitcad#894: the top front edge of the box, along X, in the box's
        // top plane (square to Z): the body lies on +Y of it, so the line
        // goes to -Y, 6 text heights past the box's front (mitcad#902).
        let d = dimension(MeasureKind::Distance);
        let corners = box_corners();
        let faces = [Z, [0.0, -1.0, 0.0]];
        let ends = [[0.0, 0.0, 5.0], [20.0, 0.0, 5.0]];
        let around = Around {
            height: 2.5,
            extent: &corners,
            faces: &faces,
            planes: [false; 2],
            outlines: [&[], &[]],
            taken: &[],
            prefer: None,
            boxes: &[],
            text_size: None,
        };
        let first = layout(&d, &Shape::Ends(ends), None, None, &around);
        near_point(first.normal, Z);
        near_point(first.y, Y);
        near(first.line_offset, -15.0);
        near_point(first.dimension_lines[0][0], [0.0, -15.0, 5.0]);
        // Outside the box's extent in the plane, on the side away from it.
        for [from, to] in &first.dimension_lines {
            assert!(from[1] < 0.0 && to[1] < 0.0, "{first:?}");
        }
        near(first.text_height, 2.5);
        // A second one along the same edge stacks 2.5 text heights further
        // out; one elsewhere along X does not.
        let taken = [Taken {
            normal: first.normal,
            x: first.x,
            from: first.dimension_lines[0][0],
            to: first.dimension_lines[0][1],
        }];
        let stacked_around = Around {
            taken: &taken,
            ..around
        };
        let half = [[0.0, 0.0, 5.0], [10.0, 0.0, 5.0]];
        let second = layout(&d, &Shape::Ends(half), None, None, &stacked_around);
        near(second.line_offset, -21.25);
        near_point(second.dimension_lines[0][1], [10.0, -21.25, 5.0]);
        let apart = [[40.0, 0.0, 5.0], [50.0, 0.0, 5.0]];
        let elsewhere = layout(&d, &Shape::Ends(apart), None, None, &stacked_around);
        near(elsewhere.line_offset, -15.0);
        // A line set by hand keeps its place.
        let mut set = dimension(MeasureKind::Distance);
        set.line_offset = Some(4.0);
        near(
            layout(&set, &Shape::Ends(half), None, None, &stacked_around).line_offset,
            4.0,
        );
    }

    #[test]
    fn an_offset_of_planes_leaves_the_body_at_its_outline() {
        // From the top to the bottom of the box through its middle: in the
        // plane square to Y, the line 6 text heights past the box's side
        // (both sides as clear: +y, here -X), the extension lines from the
        // box's outline there.
        let d = dimension(MeasureKind::Offset);
        let corners = box_corners();
        let faces = [Z, [0.0, -1.0, 0.0], X];
        let ends = [[10.0, 5.0, 5.0], [10.0, 5.0, 0.0]];
        // The top and bottom faces' own outlines: the box's top and bottom.
        let top: Vec<Vec3> = corners.iter().copied().filter(|c| c[2] == 5.0).collect();
        let bottom: Vec<Vec3> = corners.iter().copied().filter(|c| c[2] == 0.0).collect();
        let around = Around {
            height: 2.5,
            extent: &corners,
            faces: &faces,
            planes: [true, true],
            outlines: [&top, &bottom],
            taken: &[],
            prefer: None,
            boxes: &[],
            text_size: None,
        };
        let drawing = layout(&d, &Shape::Ends(ends), None, None, &around);
        near_point(drawing.normal, Y);
        near_point(drawing.y, [-1.0, 0.0, 0.0]);
        // The ends slid within both faces to their edge on the line's side
        // (x 0) and their far edge along the plane's normal (y 0,
        // mitcad#902): the line 6 text heights past the box there, the
        // extension lines from the faces' corners.
        near(drawing.line_offset, 15.0);
        near_point(drawing.extension_lines[0][0], [0.0, 0.0, 5.0]);
        near_point(drawing.extension_lines[0][1], [-15.0, 0.0, 5.0]);
        near_point(drawing.extension_lines[1][0], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn extension_lines_start_at_the_picked_geometry_not_at_a_box() {
        // mitcad#902: a boss's top face (x from 8 to 12) on the 20 x 10 x 5
        // box, offset from the box's bottom: the boss's extension line
        // leaves the boss's own outline (x = 8), not the box's edge in
        // mid-air (x = 0); the bottom's leaves the bottom's outline.
        let d = dimension(MeasureKind::Offset);
        let corners = box_corners();
        // The planes at the picks, as an offset of planes through the box's
        // middle has them: the length's plane square to Y (the front's).
        let faces = [Z, [0.0, -1.0, 0.0], X];
        let boss: Vec<Vec3> = vec![[8.0, 4.0, 5.0], [12.0, 6.0, 5.0]];
        let bottom: Vec<Vec3> = corners.iter().copied().filter(|c| c[2] == 0.0).collect();
        let ends = [[10.0, 5.0, 5.0], [10.0, 5.0, 0.0]];
        let around = Around {
            height: 2.5,
            extent: &corners,
            faces: &faces,
            planes: [true, true],
            outlines: [&boss, &bottom],
            taken: &[],
            prefer: None,
            boxes: &[],
            text_size: None,
        };
        let drawing = layout(&d, &Shape::Ends(ends), None, None, &around);
        near_point(drawing.y, [-1.0, 0.0, 0.0]);
        // The ends slid to where both faces are (the boss's edge, y 4).
        near_point(drawing.extension_lines[0][0], [8.0, 4.0, 5.0]);
        near_point(drawing.extension_lines[1][0], [0.0, 4.0, 0.0]);
        // An edge or a vertex (not a plane): from its point itself.
        let edges = Around {
            planes: [false, false],
            ..around
        };
        let drawing = layout(&d, &Shape::Ends(ends), None, None, &edges);
        near_point(drawing.extension_lines[0][0], ends[0]);
        near_point(drawing.extension_lines[1][0], ends[1]);
        // A plane whose outline is not known: from its point, never from
        // the bodies' box.
        let unknown = Around {
            outlines: [&[], &[]],
            ..around
        };
        let drawing = layout(&d, &Shape::Ends(ends), None, None, &unknown);
        near_point(drawing.extension_lines[0][0], ends[0]);
    }

    #[test]
    fn a_datum_stands_on_its_faces_outline_and_a_diameter_on_a_leader() {
        // mitcad#902, the 20 x 10 x 5 box seen from the isometric view: the
        // labels in the plane facing the eye, along the view's horizontal.
        let corners = box_corners();
        let front: Vec<Vec3> = corners.iter().copied().filter(|c| c[1] == 0.0).collect();
        let eye = unit(ISO_EYE).unwrap();
        let right = unit([1.0, 1.0, 0.0]).unwrap();
        let around = Around {
            height: 2.5,
            extent: &corners,
            faces: &[],
            planes: [true, false],
            outlines: [&front, &[]],
            taken: &[],
            prefer: None,
            boxes: &[],
            text_size: Some([5.0, 5.0]),
        };
        // A datum on the front face: its triangle on the face's outline
        // where its stem leaves the face (the bottom edge), the stem square
        // to that edge as seen, out of the face.
        let datum = dimension(MeasureKind::Datum);
        let on_front = Shape::Note {
            anchor: [10.0, 0.0, 2.5],
            normal: Some([0.0, -1.0, 0.0]),
            surface: Some([0.0, -1.0, 0.0]),
        };
        let drawing = layout(&datum, &on_front, Some(eye), Some(right), &around);
        let [tip, pointing] = drawing.arrows[0];
        near(tip[1], 0.0);
        near(tip[2], 0.0);
        assert!(tip[0] > 0.0 && tip[0] < 20.0, "{tip:?}");
        let seen_x = square_to(X, eye).unwrap();
        near(dot(pointing, seen_x), 0.0);
        let [knee, foot] = drawing.dimension_lines[0];
        near_point(foot, tip);
        assert!(dot(sub(knee, tip), [0.0, -1.0, 0.0]) > 0.0, "{knee:?}");
        // A diameter with a plane of its own: a leader from outside to its
        // rim with a landing, the arrow on the rim, no centre mark.
        let dia = dimension(MeasureKind::Diameter);
        let circle = Shape::Circle {
            center: [15.0, 5.0, 5.0],
            axis: Z,
            radius: 2.0,
        };
        let drawing = layout(&dia, &circle, Some(eye), Some(right), &around);
        assert!(drawing.center.is_none());
        assert_eq!(drawing.dimension_lines.len(), 2);
        let [rim, _] = drawing.arrows[0];
        near(rim[2], 5.0);
        near(distance(rim, [15.0, 5.0, 5.0]), 2.0);
        near_point(drawing.normal, eye);
        // Without a plane: in its circle's, as before.
        let drawing = layout(&dia, &circle, None, None, &around);
        near_point(drawing.normal, Z);
    }

    #[test]
    fn a_view_takes_the_plane_of_the_part_that_faces_it_most() {
        let along = X;
        let faces = [Z, [0.0, -1.0, 0.0]];
        // From the front, from above, obliquely from above.
        near_point(
            viewed_normal(along, &faces, [0.0, -1.0, 0.0], Some(Z)),
            [0.0, -1.0, 0.0],
        );
        near_point(viewed_normal(along, &faces, Z, Some(Y)), Z);
        near_point(viewed_normal(along, &faces, [0.0, -0.6, 0.8], Some(Z)), Z);
        // From behind the front face: its plane, facing the eye.
        near_point(viewed_normal(along, &faces, Y, Some(Z)), Y);
        // Seen along the length: the plane most square to the view's up.
        near_point(viewed_normal(along, &faces, [-1.0, 0.0, 0.0], Some(Z)), Z);
        near_point(
            viewed_normal(along, &[], [-1.0, 0.0, 0.0], Some([0.0, 1.0, 0.0])),
            Y,
        );
        // Oblique to every axis, without faces: a coordinate plane turned
        // to hold it.
        let diagonal = unit([1.0, 1.0, 0.0]).unwrap();
        let normal = viewed_normal(diagonal, &[], Z, Some(Y));
        near(dot(normal, diagonal), 0.0);
        near_point(normal, Z);
    }

    #[test]
    fn a_leaders_landing_follows_its_text_and_bends_obtusely_when_moved_near() {
        // mitcad#1168: a note in the plane square to Z, its text along X.
        let mut note = dimension(MeasureKind::Note);
        note.text = Some("Deburr".to_owned());
        let shape = Shape::Note {
            anchor: [0.0, 0.0, 0.0],
            normal: Some(Z),
            surface: None,
        };
        let landing = |drawing: &Drawing| {
            let [text_at, knee] = drawing.dimension_lines[1];
            distance(text_at, knee)
        };
        // The bend at the knee between the leader back to the point and
        // the landing, in degrees.
        let bend = |drawing: &Drawing| {
            let [knee, point] = drawing.dimension_lines[0];
            let [text_at, _] = drawing.dimension_lines[1];
            let back = unit(sub(point, knee)).unwrap();
            let on = unit(sub(text_at, knee)).unwrap();
            dot(back, on).clamp(-1.0, 1.0).acos().to_degrees()
        };
        // Its landing a text height and a half long, whatever the height:
        // 5 mm and 10 mm texts, twice as long.
        for height in [5.0, 10.0] {
            let drawing = layout(&note, &shape, Some(Z), Some(X), &Around::bare(height));
            near(landing(&drawing), LANDING_SHARE * height);
            assert!(bend(&drawing) > 90.0, "{drawing:?}");
        }
        // Moved far out, the full landing; moved nearer its point along
        // the text than the landing is long (4 mm, 5 mm text), it shortens
        // to half the way back, the knee 2 mm out from the point: the bend
        // stays obtuse, where the full landing's knee 3.5 mm past the point
        // would bend back acutely.
        note.text_offset = Some([30.0, 20.0]);
        let far = layout(&note, &shape, Some(Z), Some(X), &Around::bare(5.0));
        near(landing(&far), 7.5);
        assert!(bend(&far) > 90.0, "{far:?}");
        for [u, v] in [[4.0, 20.0], [-4.0, 20.0], [4.0, -20.0], [1.0, 3.0]] {
            note.text_offset = Some([u, v]);
            let near_it = layout(&note, &shape, Some(Z), Some(X), &Around::bare(5.0));
            near(landing(&near_it), LANDING_REACH * f64::abs(u));
            near_point(near_it.text[0], [u, v, 0.0]);
            assert!(bend(&near_it) > 90.0, "{u} {v}: {near_it:?}");
        }
        // Right above its point: no landing, the leader straight to the
        // text's start.
        note.text_offset = Some([0.0, 20.0]);
        let above = layout(&note, &shape, Some(Z), Some(X), &Around::bare(5.0));
        assert_eq!(above.dimension_lines.len(), 1, "{above:?}");
        near_point(above.dimension_lines[0][0], [0.0, 20.0, 0.0]);
    }
}
