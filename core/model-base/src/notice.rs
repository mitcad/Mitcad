// SPDX-License-Identifier: MIT
//! Notices and their severity levels (mitcad#1040): what the model tells
//! the user about a feature, a command's result, an import, an annotation,
//! a sheet metal body or a sketch, with one level of three. The levels are
//! those of `docs/ui-conventions.md` ("Messages and severity"):
//!
//! - [`NoticeLevel::Error`]: the result is missing; the work stopped (a
//!   feature that produces nothing, an export not written);
//! - [`NoticeLevel::Warning`]: it ran, but something is wrong or left out
//!   (a fillet that lost some of its edges, skipped import items, a broken
//!   annotation);
//! - [`NoticeLevel::Info`]: a neutral fact that needs no action (an import
//!   note on a feature).
//!
//! Success, hint and progress are the frontends' own; states (suppressed,
//! rolled back, inactive, hidden, pending, driven) are not severities.
//!
//! On the wire a notice is `{"level": "error" | "warning" | "info",
//! "message": …, "target": …, "places": […]}` (`target` only when it names
//! something: a feature, an annotation, a body, a sketch entity; `places`
//! only when it has some), and a result's notices are its `"notices"`
//! array.
//!
//! **Places** (mitcad#1327): a notice's message says what is wrong in the
//! design's terms and never names faces, edges or vertices by their
//! topological names (`E{…}`, `F4:side(…)`, `V{…}`), which the user sees
//! nowhere. The places it is about go into its `places` instead
//! ([`NoticePlace`]: a face, an edge or a vertex of a body by its name, a
//! point, a box), where agents find the names and the application paints
//! them in the view while the pointer is over the notice. A message given
//! with such names ([`Notice::new`]) has them taken out into its places,
//! each named in the text by its kind ("the fillet of an edge", "at a
//! vertex"; [`plain_message`]). A sketch region's name (`r{…}`) is such a
//! name too (mitcad#1334): a region of a sketch is a place, and a reference
//! a sketch change renamed reads "an edge found again after a change in
//! Sketch1", one sentence per feature for several of a kind
//! ([`merge_found_again`]: "2 edges found again …").
//!
//! **What a message says** (mitcad#1612, `docs/ui-conventions.md`): text a
//! person reads never carries topological names, sketch region keys,
//! internal ids (`k5`, `p15`, `F57`, `F2.b0`) or coordinates. A failure is
//! a [`FeatureError`]: the user's message in plain words, its places and
//! the detail for agents (the names, ids and points the message leaves
//! out). Through code that passes errors as text it travels as its message
//! with a last line [`PLACED_TAIL`] `{…}]` (the geometry kernel writes the
//! same, `geometry/include/mitcad/geometry/user_error.hpp`), which
//! [`FeatureError::from_text`] reads back. [`user_text_leaks`] finds what
//! may not be shown; [`Notice::new`] and [`FeatureError::new`] assert that
//! none is left in the model's tests.

use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// How serious a notice is. Ordered from the least to the most serious, so
/// the worst of several is their maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    /// A neutral fact that needs no action.
    Info,
    /// It ran, but something is wrong or left out; the user should check it.
    Warning,
    /// The result is missing: the work stopped.
    Error,
}

impl NoticeLevel {
    /// `info`, `warning` or `error`: the level on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// The level of its wire name; None for any other text.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// A place a notice is about (mitcad#1327): a face, an edge or a vertex of
/// a body by its topological name (the body's uid where it is known), a
/// point, or a box (millimetres, in the body's component); or a region of
/// a sketch by its key (the sketch's uid where it is known; mitcad#1334).
/// On the wire `{"kind": "face", "body": "F2.b0", "name":
/// "F7:fillet(E{…})"}`, `{"kind": "point", "at": [x, y, z]}`, `{"kind":
/// "box", "min": […], "max": […]}`, `{"kind": "region", "sketch": "F1",
/// "name": "r{…}"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoticePlace {
    Face {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        name: String,
    },
    Edge {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        name: String,
    },
    Vertex {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        name: String,
    },
    Point {
        at: [f64; 3],
    },
    Box {
        min: [f64; 3],
        max: [f64; 3],
    },
    Region {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sketch: Option<String>,
        name: String,
    },
    /// An occurrence, with every body it places (mitcad#1745: what holds
    /// a joint's side): its path of uids from the root, `O1/O4`.
    Occurrence {
        path: String,
    },
}

// (Coordinates are finite numbers: a place equals itself.)
impl Eq for NoticePlace {}

impl NoticePlace {
    pub fn face(name: impl Into<String>) -> Self {
        Self::Face {
            body: None,
            name: name.into(),
        }
    }

    pub fn edge(name: impl Into<String>) -> Self {
        Self::Edge {
            body: None,
            name: name.into(),
        }
    }

    pub fn vertex(name: impl Into<String>) -> Self {
        Self::Vertex {
            body: None,
            name: name.into(),
        }
    }

    pub fn point(at: [f64; 3]) -> Self {
        Self::Point { at }
    }

    /// A region of a sketch by its key, the sketch not known yet.
    pub fn region(name: impl Into<String>) -> Self {
        Self::Region {
            sketch: None,
            name: name.into(),
        }
    }

    /// An occurrence by its path of uids from the root (`O1/O4`).
    pub fn occurrence(path: impl Into<String>) -> Self {
        Self::Occurrence { path: path.into() }
    }

    /// A region of the sketch `sketch` (its uid) by its key.
    pub fn region_of(sketch: impl ToString, name: impl Into<String>) -> Self {
        Self::Region {
            sketch: Some(sketch.to_string()),
            name: name.into(),
        }
    }

    /// The topological name of a face, an edge or a vertex, or a region's
    /// key; None for a point or a box.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Face { name, .. }
            | Self::Edge { name, .. }
            | Self::Vertex { name, .. }
            | Self::Region { name, .. } => Some(name),
            Self::Point { .. } | Self::Box { .. } | Self::Occurrence { .. } => None,
        }
    }

    /// The body of a face, an edge or a vertex, where it is known.
    pub fn body(&self) -> Option<&str> {
        match self {
            Self::Face { body, .. } | Self::Edge { body, .. } | Self::Vertex { body, .. } => {
                body.as_deref()
            }
            Self::Point { .. }
            | Self::Box { .. }
            | Self::Region { .. }
            | Self::Occurrence { .. } => None,
        }
    }

    /// The sketch of a region, where it is known.
    pub fn sketch(&self) -> Option<&str> {
        match self {
            Self::Region { sketch, .. } => sketch.as_deref(),
            _ => None,
        }
    }

    /// The same region in `uid` when it is of no sketch yet; any other
    /// place as it is.
    pub fn in_sketch(mut self, uid: &str) -> Self {
        if let Self::Region { sketch, .. } = &mut self
            && sketch.is_none()
        {
            *sketch = Some(uid.to_owned());
        }
        self
    }

    /// Whether `other` is this place with its body or sketch given where
    /// this one has none: the same kind and name.
    fn completed_by(&self, other: &Self) -> bool {
        let unknown = match self {
            Self::Face { body, .. } | Self::Edge { body, .. } | Self::Vertex { body, .. } => {
                body.is_none() && other.body().is_some()
            }
            Self::Region { sketch, .. } => sketch.is_none() && other.sketch().is_some(),
            Self::Point { .. } | Self::Box { .. } | Self::Occurrence { .. } => false,
        };
        unknown
            && std::mem::discriminant(self) == std::mem::discriminant(other)
            && self.name() == other.name()
    }

    /// The same place in `body` when it names a face, an edge or a vertex
    /// of no body yet.
    pub fn in_body(mut self, uid: &str) -> Self {
        match &mut self {
            Self::Face { body, .. } | Self::Edge { body, .. } | Self::Vertex { body, .. } => {
                if body.is_none() {
                    *body = Some(uid.to_owned());
                }
            }
            Self::Point { .. }
            | Self::Box { .. }
            | Self::Region { .. }
            | Self::Occurrence { .. } => {}
        }
        self
    }
}

/// The places of a warning given with its message, for the feature that
/// gave it (mitcad#1327): its message and the places, one entry per
/// warning that has some.
pub type WarningPlaces = Vec<(String, Vec<NoticePlace>)>;

/// One thing the model tells the user, with its level and, when it is
/// about something in particular, what it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
    /// What it is about: a feature's uid, an annotation's id, a body's uid,
    /// a sketch entity; None for the result as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Where it is about (mitcad#1327): faces, edges, vertices, points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub places: Vec<NoticePlace>,
}

impl Notice {
    /// A notice of `message` in plain words: the topological names in it
    /// become its places ([`plain_message`]), and the ids in it words
    /// (mitcad#1612).
    pub fn new(level: NoticeLevel, message: impl Into<String>) -> Self {
        let message = message.into();
        // A failure's text with its places (mitcad#1612): its message. An
        // older text (a feature's warning, an import's note) is cleaned of
        // the names, ids and coordinates it still gives, as a failure's
        // older text is ([`FeatureError::from_text`]); its own text stays
        // the agents' where it is kept (`warnings`).
        let (message, places) = if message.contains(PLACED_TAIL) {
            let error = FeatureError::from_text(&message);
            (error.message, error.places)
        } else {
            cleaned(&message, true)
        };
        assert_user_text(&message);
        Self {
            level,
            message,
            target: None,
            places,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Warning, message)
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Info, message)
    }

    /// The same notice about `target`.
    pub fn with_target(mut self, target: impl ToString) -> Self {
        self.target = Some(target.to_string());
        self
    }

    /// The same notice with these places too (each once; a place its
    /// message named takes the body or sketch given with the same name).
    /// Where those places are of one body, the names its message gave
    /// (of no body) are taken to be of that body too; regions likewise of
    /// one sketch.
    pub fn with_places(mut self, places: impl IntoIterator<Item = NoticePlace>) -> Self {
        for place in places {
            if self.places.contains(&place) {
                continue;
            }
            match self
                .places
                .iter_mut()
                .find(|known| known.completed_by(&place))
            {
                Some(known) => *known = place,
                None => self.places.push(place),
            }
        }
        let mut bodies = self.places.iter().filter_map(NoticePlace::body);
        if let Some(body) = bodies.next().map(str::to_owned)
            && bodies.all(|other| other == body)
        {
            self = self.in_body(&body);
        }
        let mut sketches = self.places.iter().filter_map(NoticePlace::sketch);
        if let Some(sketch) = sketches.next().map(str::to_owned)
            && sketches.all(|other| other == sketch)
        {
            self = self.in_sketch(&sketch);
        }
        self
    }

    /// The same notice, its places of no body yet in `body`.
    pub fn in_body(mut self, body: &str) -> Self {
        self.places = unique(
            std::mem::take(&mut self.places)
                .into_iter()
                .map(|place| place.in_body(body)),
        );
        self
    }

    /// The same notice, its regions of no sketch yet in `sketch`.
    pub fn in_sketch(mut self, sketch: &str) -> Self {
        self.places = unique(
            std::mem::take(&mut self.places)
                .into_iter()
                .map(|place| place.in_sketch(sketch)),
        );
        self
    }

    /// The notice as JSON (`level`, `message`, `target` and `places` when
    /// it has them).
    pub fn to_json(&self) -> Value {
        let mut value = json!({"level": self.level.as_str(), "message": self.message});
        if let Some(target) = &self.target {
            value["target"] = json!(target);
        }
        if !self.places.is_empty() {
            value["places"] = serde_json::to_value(&self.places).unwrap_or(Value::Null);
        }
        value
    }
}

/// The start of the last line of a failure's text that carries its places
/// and its detail (mitcad#1612): `\n[placed: {"message": …, "detail": …,
/// "places": […]}]`, the JSON of a [`FeatureError`]. The geometry kernel
/// writes it too (`geometry/include/mitcad/geometry/user_error.hpp`); the
/// application shows the text before it.
pub const PLACED_TAIL: &str = "\n[placed: ";

/// Why something failed (mitcad#1612): what a person reads, in plain words
/// without names, ids or coordinates ([`user_text_leaks`]); the places it
/// is about, painted while the pointer is over the message; and the detail
/// for agents and the log, with the names, ids and points the message
/// leaves out (empty when it says no more than the message). A feature's
/// failure (the model's `FeatureStatus::Failed` and its output's
/// `result`), an export's or an edit's.
///
/// Passed as text it is its message with its [`PLACED_TAIL`] line
/// ([`FeatureError::text`], also `Display`); [`FeatureError::from_text`]
/// reads it back, with text before or after the tail kept around the
/// message and the detail ("Chamfer4: " before both). Read as `str`
/// (`Deref`, the comparisons with text) it is the agent's text: the detail,
/// or the message when it has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureError {
    /// What a person reads.
    #[serde(default)]
    pub message: String,
    /// What it is about: faces, edges, vertices, points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub places: Vec<NoticePlace>,
    /// What agents and the log read instead: the message with the names,
    /// ids and coordinates; empty when it would be the message.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

impl FeatureError {
    /// A failure that says `message`, which is in plain words (asserted in
    /// the model's tests). The one place a failure's sentence is made, so
    /// that a translation's id can be given here (mitcad#1284).
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        assert_user_text(&message);
        Self {
            message,
            places: Vec::new(),
            detail: String::new(),
        }
    }

    /// The same failure with the agent's text `detail`, the names, ids and
    /// coordinates its message leaves out.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        self.detail = if detail == self.message {
            String::new()
        } else {
            detail
        };
        self
    }

    /// The same failure about `place` too.
    pub fn at(mut self, place: NoticePlace) -> Self {
        if !self.places.contains(&place) {
            self.places.push(place);
        }
        self
    }

    /// The same failure about these places too.
    pub fn with_places(mut self, places: impl IntoIterator<Item = NoticePlace>) -> Self {
        for place in places {
            self = self.at(place);
        }
        self
    }

    /// The same failure, its places of no body yet in `body`.
    pub fn in_body(mut self, body: &str) -> Self {
        self.places = unique(
            std::mem::take(&mut self.places)
                .into_iter()
                .map(|place| place.in_body(body)),
        );
        self
    }

    /// What agents and the log read: the detail, or the message.
    pub fn agent_text(&self) -> &str {
        if self.detail.is_empty() {
            &self.message
        } else {
            &self.detail
        }
    }

    /// The same failure with `before` before its message and its detail
    /// ("Chamfer4: ").
    pub fn prefixed(mut self, before: &str) -> Self {
        self.message.insert_str(0, before);
        if !self.detail.is_empty() {
            self.detail.insert_str(0, before);
        }
        self
    }

    /// The same failure with `after` after its message and its detail.
    pub fn suffixed(mut self, after: &str) -> Self {
        self.message.push_str(after);
        if !self.detail.is_empty() {
            self.detail.push_str(after);
        }
        self
    }

    /// The failure as text: its message, and its [`PLACED_TAIL`] line when
    /// it has places or a detail.
    pub fn text(&self) -> String {
        if self.places.is_empty() && self.detail.is_empty() {
            return self.message.clone();
        }
        let json = serde_json::to_string(self).unwrap_or_default();
        format!("{}{PLACED_TAIL}{json}]", self.message)
    }

    /// A failure read from its text: the [`PLACED_TAIL`] lines in it (the
    /// geometry kernel's and the model's, also with text before or after
    /// them) give its places and its detail; what the text names besides
    /// (an older text, an imported one) the fallback cleaner takes out into
    /// places ([`plain_message`]), the text as it was its detail.
    pub fn from_text(text: &str) -> Self {
        let mut message = String::new();
        let mut detail = String::new();
        let mut places = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find(PLACED_TAIL) {
            let before = &rest[..at];
            let json = &rest[at + PLACED_TAIL.len()..];
            let mut stream = serde_json::Deserializer::from_str(json).into_iter::<FeatureError>();
            let read = match stream.next() {
                Some(Ok(inner)) => Some((inner, stream.byte_offset())),
                _ => None,
            };
            let Some((inner, used)) = read.filter(|(_, used)| json[*used..].starts_with(']'))
            else {
                // Not a tail: text as it is.
                let kept = &rest[..at + PLACED_TAIL.len()];
                message.push_str(kept);
                detail.push_str(kept);
                rest = json;
                continue;
            };
            message.push_str(before);
            // The detail where the message it explains is.
            match before.strip_suffix(inner.message.as_str()) {
                Some(head) if !inner.detail.is_empty() => {
                    detail.push_str(head);
                    detail.push_str(&inner.detail);
                }
                _ => {
                    detail.push_str(before);
                    if !inner.detail.is_empty() {
                        detail.push_str(" (");
                        detail.push_str(&inner.detail);
                        detail.push(')');
                    }
                }
            }
            places.extend(inner.places);
            rest = &json[used + 1..];
        }
        message.push_str(rest);
        detail.push_str(rest);
        // What an older or imported text still names, its ids too.
        let (message, named) = cleaned(&message, true);
        places.extend(named);
        let detail = if detail == message {
            String::new()
        } else {
            detail
        };
        Self {
            message,
            places: unique(places),
            detail,
        }
    }

    /// The failure as an error notice: its message and places.
    pub fn notice(&self) -> Notice {
        Notice {
            level: NoticeLevel::Error,
            message: self.message.clone(),
            target: None,
            places: self.places.clone(),
        }
    }

    /// The failure as JSON: `message`, `detail` and `places` when it has
    /// them.
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

impl From<FeatureError> for String {
    fn from(error: FeatureError) -> Self {
        error.text()
    }
}

impl From<&str> for FeatureError {
    fn from(text: &str) -> Self {
        Self::from_text(text)
    }
}

impl From<String> for FeatureError {
    fn from(text: String) -> Self {
        Self::from_text(&text)
    }
}

/// Its text with its tail ([`FeatureError::text`]), so that a failure
/// passed on in a longer text keeps its places and detail.
impl fmt::Display for FeatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

/// The agent's text ([`FeatureError::agent_text`]).
impl Deref for FeatureError {
    type Target = str;

    fn deref(&self) -> &str {
        self.agent_text()
    }
}

impl PartialEq<str> for FeatureError {
    fn eq(&self, other: &str) -> bool {
        self.agent_text() == other
    }
}

impl PartialEq<&str> for FeatureError {
    fn eq(&self, other: &&str) -> bool {
        self.agent_text() == *other
    }
}

impl PartialEq<String> for FeatureError {
    fn eq(&self, other: &String) -> bool {
        self.agent_text() == other
    }
}

/// The text a person reads of a text that may carry a failure's tail
/// (mitcad#1612): the message, cleaned of what older texts name.
pub fn user_message(text: &str) -> String {
    FeatureError::from_text(text).message
}

/// A failure about one face, edge or vertex as text (mitcad#1612):
/// `message` for the person, `place` painted, `detail` for agents.
pub fn placed(message: impl Into<String>, place: NoticePlace, detail: impl Into<String>) -> String {
    FeatureError::new(message)
        .at(place)
        .with_detail(detail)
        .text()
}

/// The failure `inner` (a text, perhaps with its own tail) about the
/// face, edge or vertex `place` (mitcad#1612): the person reads inner's
/// message, `place` is painted, and the detail says which one ("face
/// F2:top: …"), as the texts of the model said before.
pub fn about_place(place: NoticePlace, inner: &str) -> String {
    let error = FeatureError::from_text(inner);
    let kind = match &place {
        NoticePlace::Face { .. } => "face",
        NoticePlace::Edge { .. } => "edge",
        NoticePlace::Vertex { .. } => "vertex",
        NoticePlace::Region { .. } => "region",
        NoticePlace::Point { .. } | NoticePlace::Box { .. } => "place",
        NoticePlace::Occurrence { .. } => "occurrence",
    };
    let detail = format!(
        "{kind} {}: {}",
        place.name().unwrap_or_default(),
        error.agent_text()
    );
    let message = error.message.clone();
    FeatureError {
        message,
        places: error.places,
        detail: String::new(),
    }
    .at(place)
    .with_detail(detail)
    .text()
}

/// A failure's text (mitcad#1644): `message` in plain words for a
/// person, `detail` the same with the ids, names and coordinates for
/// agents; no places.
pub fn said(message: impl Into<String>, detail: impl Into<String>) -> String {
    FeatureError::new(message).with_detail(detail).text()
}

/// [`said`] about these places too (mitcad#1644).
pub fn said_at(
    message: impl Into<String>,
    detail: impl Into<String>,
    places: impl IntoIterator<Item = NoticePlace>,
) -> String {
    FeatureError::new(message)
        .with_places(places)
        .with_detail(detail)
        .text()
}

/// The failure `inner` (a text, perhaps with its own tail) after a
/// lead-in (mitcad#1644): a person reads `lead` (plain words, often empty
/// where the places say which one) and inner's message, agents `named`
/// (the lead-in with its ids, "k3: ") and inner's agent text; inner's
/// places are kept.
pub fn said_after(lead: &str, named: &str, inner: &str) -> String {
    said_around(|message| format!("{lead}{message}"), named, inner)
}

/// [`said_after`] with the person's message made of inner's by `said`
/// (mitcad#1644): one whole sentence with inner's message as its value
/// (`format!("the source of an attachment: {message}")`), so that it is
/// translated as one.
pub fn said_around(said: impl FnOnce(&str) -> String, named: &str, inner: &str) -> String {
    let error = FeatureError::from_text(inner);
    let detail = format!("{named}{}", error.agent_text());
    FeatureError::new(said(&error.message))
        .with_places(error.places)
        .with_detail(detail)
        .text()
}

/// Places each once, in their order.
fn unique(places: impl IntoIterator<Item = NoticePlace>) -> Vec<NoticePlace> {
    let mut out: Vec<NoticePlace> = Vec::new();
    for place in places {
        if !out.contains(&place) {
            out.push(place);
        }
    }
    out
}

/// The kinds of places a message names: topological names and region keys
/// ([`name_at`]), and what the fallback cleaner takes out besides
/// (mitcad#1612): a body's uid (`F2.b0`) and a coordinate tuple
/// ([`other_at`]).
#[derive(Clone, Copy, PartialEq, Eq)]
enum NamedKind {
    Face,
    Edge,
    Vertex,
    Region,
    Body,
    Point,
    // Internal ids a failure's older text gives (`FeatureError::from_text`
    // only): a sketch's curve `c3` and constraint `k5` (its point `p7` is
    // a Point), a feature's `F57`.
    Curve,
    Constraint,
    Feature,
}

impl NamedKind {
    /// The place a name of the kind names; None for a body's uid and a
    /// coordinate tuple ([`other_at`] gives a tuple's point).
    fn place(self, name: &str) -> Option<NoticePlace> {
        match self {
            Self::Face => Some(NoticePlace::face(name)),
            Self::Edge => Some(NoticePlace::edge(name)),
            Self::Vertex => Some(NoticePlace::vertex(name)),
            Self::Region => Some(NoticePlace::region(name)),
            Self::Body | Self::Point | Self::Curve | Self::Constraint | Self::Feature => None,
        }
    }

    /// The word of the kind, one and several.
    fn words(self) -> (&'static str, &'static str) {
        match self {
            Self::Face => ("face", "faces"),
            Self::Edge => ("edge", "edges"),
            Self::Vertex => ("vertex", "vertices"),
            Self::Region => ("region", "regions"),
            Self::Body => ("body", "bodies"),
            Self::Point => ("point", "points"),
            Self::Curve => ("curve", "curves"),
            Self::Constraint => ("constraint", "constraints"),
            Self::Feature => ("feature", "features"),
        }
    }

    /// The kind its word (one) names.
    fn of_word(word: &str) -> Option<Self> {
        [Self::Face, Self::Edge, Self::Vertex, Self::Region]
            .into_iter()
            .find(|kind| kind.words().0 == word)
    }
}

/// "a" or "an" before `word`.
fn article(word: &str) -> &'static str {
    if word.starts_with(|c: char| "aeiouAEIOU".contains(c)) {
        "an"
    } else {
        "a"
    }
}

/// The topological name starting at byte `i` of `text`, its kind and the
/// byte after its end: `E{…}`, `V{…}`, `F<n>:<role>(…)`, a bare
/// `F<n>:<role>` (`F35:end`, `F1:top`, `F1:side0`; mitcad#1612) or a
/// sketch region's key `r{…}` (brackets balanced, a piece's `#k` after
/// them), not after a letter, a digit, `_` or `.`. None where none starts
/// there.
fn name_at(text: &str, i: usize) -> Option<(NamedKind, usize)> {
    let bytes = text.as_bytes();
    if i > 0 {
        let before = bytes[i - 1];
        if before.is_ascii_alphanumeric() || before == b'_' || before == b'.' {
            return None;
        }
    }
    let (kind, open) = match bytes.get(i)? {
        b'E' if bytes.get(i + 1) == Some(&b'{') => (NamedKind::Edge, i + 1),
        b'V' if bytes.get(i + 1) == Some(&b'{') => (NamedKind::Vertex, i + 1),
        b'r' if bytes.get(i + 1) == Some(&b'{') => (NamedKind::Region, i + 1),
        b'F' => {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j == i + 1 || bytes.get(j) != Some(&b':') {
                return None;
            }
            j += 1;
            let role = j;
            while j < bytes.len()
                && (bytes[j].is_ascii_lowercase()
                    || bytes[j] == b'_'
                    || (j > role && bytes[j].is_ascii_digit()))
            {
                j += 1;
            }
            if j == role {
                return None;
            }
            if bytes.get(j) != Some(&b'(') {
                // A bare name (mitcad#1612), not a word that goes on.
                if bytes.get(j).is_some_and(|b| b.is_ascii_alphanumeric()) {
                    return None;
                }
                return Some((NamedKind::Face, with_piece(bytes, j)));
            }
            (NamedKind::Face, j)
        }
        _ => return None,
    };
    let mut depth = 0usize;
    let mut j = open;
    loop {
        match bytes.get(j)? {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    Some((kind, with_piece(bytes, j + 1)))
}

/// `end`, or the byte after a piece's `#k` that starts there.
fn with_piece(bytes: &[u8], end: usize) -> usize {
    if bytes.get(end) == Some(&b'#') {
        let digits = bytes[end + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits > 0 {
            return end + 1 + digits;
        }
    }
    end
}

/// Whether byte `i` of `text` starts a word: at the start, or after a
/// character that is no letter, digit, `_` or `.`.
fn word_starts(bytes: &[u8], i: usize) -> bool {
    i == 0 || {
        let before = bytes[i - 1];
        !(before.is_ascii_alphanumeric() || before == b'_' || before == b'.')
    }
}

/// Whether byte `i` ends a word: at the end, or before a character that is
/// no letter, digit or `_`.
fn word_ends(bytes: &[u8], i: usize) -> bool {
    bytes
        .get(i)
        .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
}

/// The byte after the digits from byte `i` (`i` itself when none).
fn digits_end(bytes: &[u8], i: usize) -> usize {
    i + bytes[i.min(bytes.len())..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count()
}

/// A body's uid (`F2.b0`) starting at byte `i`: the byte after it.
fn body_uid_at(text: &str, i: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(i) != Some(&b'F') || !word_starts(bytes, i) {
        return None;
    }
    let feature = digits_end(bytes, i + 1);
    if feature == i + 1
        || bytes.get(feature) != Some(&b'.')
        || bytes.get(feature + 1) != Some(&b'b')
    {
        return None;
    }
    let end = digits_end(bytes, feature + 2);
    (end > feature + 2 && word_ends(bytes, end)).then_some(end)
}

/// A number from byte `i` (`-12`, `3.5`, `1e-3`): the byte after it.
fn number_at(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i;
    if bytes.get(j) == Some(&b'-') || bytes.get(j) == Some(&b'+') {
        j += 1;
    }
    let whole = digits_end(bytes, j);
    let mut end = whole;
    if bytes.get(end) == Some(&b'.') {
        end = digits_end(bytes, end + 1);
    }
    if end == j || (whole == j && end == j + 1) {
        return None;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut k = end + 1;
        if matches!(bytes.get(k), Some(b'-' | b'+')) {
            k += 1;
        }
        let exponent = digits_end(bytes, k);
        if exponent > k {
            end = exponent;
        }
    }
    Some(end)
}

/// A coordinate tuple starting at byte `i`: `(x, y, z)` or a sketch's
/// `(x, y)`, numbers only. The byte after it and its numbers.
fn tuple_at(text: &str, i: usize) -> Option<(usize, Vec<f64>)> {
    let bytes = text.as_bytes();
    if bytes.get(i) != Some(&b'(') {
        return None;
    }
    let skip = |mut j: usize| {
        while bytes.get(j) == Some(&b' ') {
            j += 1;
        }
        j
    };
    let mut values = Vec::new();
    let mut j = skip(i + 1);
    loop {
        let end = number_at(bytes, j)?;
        values.push(text[j..end].parse::<f64>().ok()?);
        j = skip(end);
        match bytes.get(j) {
            Some(b',') if values.len() < 3 => j = skip(j + 1),
            Some(b')') if values.len() >= 2 => return Some((j + 1, values)),
            _ => return None,
        }
    }
}

/// What the fallback cleaner takes out besides topological names
/// (mitcad#1612), starting at byte `i`: a body's uid, or a coordinate
/// tuple (a point in the model's space for three numbers, none for a
/// sketch's two). Its kind, the byte after it and its place.
fn other_at(text: &str, i: usize) -> Option<(NamedKind, usize, Option<NoticePlace>)> {
    if let Some(end) = body_uid_at(text, i) {
        return Some((NamedKind::Body, end, None));
    }
    let (end, values) = tuple_at(text, i)?;
    let place = match values.as_slice() {
        [x, y, z] => Some(NoticePlace::point([*x, *y, *z])),
        _ => None,
    };
    Some((NamedKind::Point, end, place))
}

/// An internal id starting at byte `i` (`FeatureError::from_text`'s
/// cleaner only, mitcad#1612): a sketch's curve `c3`, point `p7` or
/// constraint `k5`, a feature's `F57`, a whole word. Its kind and the byte
/// after it.
fn id_at(text: &str, i: usize) -> Option<(NamedKind, usize, Option<NoticePlace>)> {
    let bytes = text.as_bytes();
    let kind = match bytes.get(i)? {
        b'c' => NamedKind::Curve,
        b'p' => NamedKind::Point,
        b'k' => NamedKind::Constraint,
        b'F' => NamedKind::Feature,
        _ => return None,
    };
    if !word_starts(bytes, i) {
        return None;
    }
    let end = digits_end(bytes, i + 1);
    (end > i + 1 && word_ends(bytes, end)).then_some((kind, end, None))
}

/// What may not be shown to a person (mitcad#1612, `docs/ui-conventions.md`
/// "What a message says"), the first found in `text`: a topological name
/// (`E{…}`, `V{…}`, `F3:side(…)`, a bare `F1:top`), a sketch region's key
/// (`r{…}`), an internal id (a sketch's `k5`, `p15`, `c14`, `r2`; a
/// feature's `F57`; a body's `F2.b0`) or a coordinate tuple
/// (`(28.28, -28.28, 202)`). None for plain words.
pub fn user_text_leaks(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if !text.is_char_boundary(i) {
            continue;
        }
        // An opened name, closed or not.
        if matches!(bytes[i], b'E' | b'V' | b'r')
            && bytes.get(i + 1) == Some(&b'{')
            && word_starts(bytes, i)
        {
            let end = name_at(text, i).map_or((i + 2).min(text.len()), |(_, end)| end);
            return Some(text[i..end].to_owned());
        }
        if let Some((_, end)) = name_at(text, i) {
            return Some(text[i..end].to_owned());
        }
        if let Some((_, end, _)) = other_at(text, i) {
            return Some(text[i..end].to_owned());
        }
        // An id: a sketch's entity, constraint or region, or a feature's.
        if matches!(bytes[i], b'k' | b'p' | b'c' | b'r' | b'F') && word_starts(bytes, i) {
            let end = digits_end(bytes, i + 1);
            if end > i + 1 && word_ends(bytes, end) {
                return Some(text[i..end].to_owned());
            }
        }
    }
    None
}

/// Whether the model's tests check what is shown (the `strict-user-text`
/// feature, which the mock kernel's crate turns on; mitcad#1612).
const STRICT_USER_TEXT: bool = cfg!(any(test, feature = "strict-user-text"));

/// In the model's tests: that `message`, text a person reads, has nothing
/// [`user_text_leaks`] finds (mitcad#1612).
fn assert_user_text(message: &str) {
    if STRICT_USER_TEXT {
        let leak = user_text_leaks(message);
        debug_assert!(
            leak.is_none(),
            "a message a person reads names {:?}: {message:?} (docs/ui-conventions.md, \
             \"What a message says\": give its places and the agent's detail instead)",
            leak.unwrap_or_default()
        );
    }
}

/// A message without the topological names in it, and the places they
/// name (mitcad#1327): each name, or a list of names of one kind ("A, B and
/// C"), is taken out; after the word of its kind ("edge", "faces", ...) the
/// word stays, with "a" or "an" before a single one unless its noun phrase
/// has a word like "the" or "its" ("the fillet of an edge", "the driven
/// dimension"; [`with_article`]); elsewhere the kind
/// takes its place ("at a vertex", "edges"). Text without names is as it
/// was. A reference a sketch change renamed (`edge E{…} found as E{…}
/// after a change in Sketch1`, a region's `r{…} found as r{…} + r{…}`)
/// reads "an edge found again after a change in Sketch1", "a region found
/// as 2 regions after a change in Sketch1" (mitcad#1334). The fallback
/// cleaner of older texts and imported ones (mitcad#1612) takes out bare
/// names (`F35:end`) as faces, a body's uid ("of body F2.b0": "of a
/// body") and coordinate tuples, which become points ("at (1, 2, 3)": "at
/// a point", the point its place).
pub fn plain_message(text: &str) -> (String, Vec<NoticePlace>) {
    cleaned(text, false)
}

/// The words of kinds after which [`cleaned`] keeps the word and takes the
/// name out ("of edge E{…}": "of an edge"). The generator of the model
/// message templates (`tools/translations/mitcad-translations.cpp`,
/// mitcad#1635) makes each template's cleaned form with the same words.
const KINDS: [&str; 32] = [
    "face",
    "faces",
    "edge",
    "edges",
    "vertex",
    "vertices",
    "corner",
    "corners",
    "region",
    "regions",
    "profile",
    "profiles",
    "body",
    "bodies",
    "point",
    "points",
    "curve",
    "curves",
    "line",
    "lines",
    "arc",
    "arcs",
    "circle",
    "circles",
    "constraint",
    "constraints",
    "dimension",
    "dimensions",
    "feature",
    "features",
    "pattern",
    "offset",
];

/// The words after which [`with_article`] puts no article: the noun phrase
/// has one.
const DETERMINERS: [&str; 13] = [
    "the", "a", "an", "its", "their", "this", "that", "each", "every", "one", "another", "which",
    "no",
];

/// The words that end a noun phrase, looking back from the word of its kind
/// ([`with_article`], mitcad#1638): prepositions, conjunctions, auxiliaries
/// and a few verbs a message puts before a kind ("does not fit face %1",
/// "the thread cut body %1"). The generator of the model message templates
/// (`tools/translations/mitcad-translations.cpp`, kPhraseEnds) has the same.
const PHRASE_ENDS: [&str; 111] = [
    "at", "on", "of", "in", "into", "onto", "to", "from", "for", "with", "without", "by", "behind",
    "between", "through", "along", "across", "over", "under", "above", "below", "beyond", "near",
    "after", "before", "than", "as", "about", "around", "against", "past", "via", "within",
    "inside", "outside", "per", "and", "or", "but", "nor", "if", "when", "where", "while",
    "because", "then", "is", "are", "was", "were", "be", "been", "being", "has", "have", "had",
    "do", "does", "did", "not", "can", "cannot", "could", "must", "may", "might", "will", "would",
    "shall", "should", "it", "they", "there", "here", "what", "who", "create", "delete", "add",
    "remove", "make", "made", "move", "moved", "change", "cut", "fit", "own", "reach", "develop",
    "use", "keep", "pick", "select", "join", "split", "need", "find", "found", "take", "took",
    "give", "gave", "get", "got", "put", "meet", "cross", "follow", "show", "hide",
];

/// Those of [`PHRASE_ENDS`] after which a verb comes: "does not fit face"
/// reads "does not fit a face", not "does not a fit face".
const VERB_LEADS: [&str; 15] = [
    "to", "do", "does", "did", "not", "can", "cannot", "could", "must", "may", "might", "will",
    "would", "shall", "should",
];

/// A character of a word of a noun phrase: a letter, an apostrophe ("the
/// result's") or a hyphen ("pre-existing").
fn phrase_char(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'\'' || b == b'-'
}

/// `out`, which ends with the word of a kind at byte `start` (its name
/// taken out, mitcad#1327), with "a" or "an" where it belongs (mitcad#1638):
/// none for several ("faces"), nor where the noun phrase has a word like
/// "the" or a possessive ("the driven dimension", "a new body", "the
/// result's face"); otherwise before the phrase, its words looked back over
/// up to three until a word that ends it ("meets a boundary curve", "at a
/// bend edge"), or before the word of the kind when a verb comes first
/// ("does not fit a face"). A capital at the start of a sentence. The
/// generator of the model message templates (`withArticle`) does the same.
fn with_article(mut out: String, start: usize) -> String {
    const SPACES: [char; 6] = [' ', '\t', '\n', '\r', '\x0c', '\x0b'];
    let lower = out[start..].to_lowercase();
    if lower.is_empty() || lower.ends_with('s') {
        return out;
    }
    let bytes = out.as_bytes();
    // The phrase's words before the kind's, nearest first.
    let mut starts: Vec<usize> = Vec::new();
    let mut at = start;
    let place = loop {
        let mut gap = at;
        while gap > 0 && b" \t\n\r".contains(&bytes[gap - 1]) {
            gap -= 1;
        }
        if gap == at || gap == 0 || !phrase_char(bytes[gap - 1]) {
            break starts.last().copied().unwrap_or(start);
        }
        let mut word_start = gap;
        while word_start > 0 && phrase_char(bytes[word_start - 1]) {
            word_start -= 1;
        }
        let previous = out[word_start..gap].to_ascii_lowercase();
        if !previous.bytes().any(|b| b.is_ascii_alphabetic()) {
            break starts.last().copied().unwrap_or(start);
        }
        if DETERMINERS.contains(&previous.as_str()) || previous.ends_with("'s") {
            return out;
        }
        if PHRASE_ENDS.contains(&previous.as_str()) || previous.ends_with('s') {
            break if VERB_LEADS.contains(&previous.as_str()) {
                if starts.len() >= 2 {
                    starts[starts.len() - 2]
                } else {
                    start
                }
            } else {
                starts.last().copied().unwrap_or(start)
            };
        }
        if starts.len() == 3 {
            break start;
        }
        starts.push(word_start);
        at = word_start;
    };
    let first_end = place
        + bytes[place..]
            .iter()
            .take_while(|b| phrase_char(**b))
            .count();
    let first = &out[place..first_end];
    let vowel = first
        .bytes()
        .next()
        .is_some_and(|b| b"aeiou".contains(&b.to_ascii_lowercase()));
    let sentence = out[..place]
        .trim_end_matches(&SPACES[..])
        .chars()
        .last()
        .is_none_or(|c| ".!?".contains(c));
    let capital = first.starts_with(|c: char| c.is_ascii_uppercase()) && sentence;
    let article = match (vowel, capital) {
        (true, true) => "An ",
        (true, false) => "an ",
        (false, true) => "A ",
        (false, false) => "a ",
    };
    let word = if place == start {
        first.to_ascii_lowercase()
    } else if capital && first[1..] == first[1..].to_ascii_lowercase() {
        first[..1].to_ascii_lowercase() + &first[1..]
    } else {
        first.to_owned()
    };
    out.replace_range(place..first_end, &format!("{article}{word}"));
    out
}

/// [`plain_message`]; with `ids`, internal ids too ([`id_at`]: "line c3
/// does not exist" reads "a line does not exist", "c3 and c5 are tangent
/// at p7" "curves are tangent at a point"), as a failure's older text is
/// read ([`FeatureError::from_text`]).
fn cleaned(text: &str, ids: bool) -> (String, Vec<NoticePlace>) {
    if let Some(renamed) = found_again(text) {
        return renamed;
    }
    enum Piece {
        Text(String),
        Names(NamedKind, usize),
    }
    let mut pieces: Vec<Piece> = Vec::new();
    let mut places = Vec::new();
    let mut found = false;
    let mut plain = String::new();
    let mut i = 0;
    while i < text.len() {
        let piece = name_at(text, i)
            .map(|(kind, end)| (kind, end, kind.place(&text[i..end])))
            .or_else(|| other_at(text, i))
            .or_else(|| if ids { id_at(text, i) } else { None });
        if let Some((kind, end, place)) = piece {
            found = true;
            places.extend(place);
            // A list of names of one kind is one piece.
            let joined = matches!(
                (pieces.last(), plain.as_str()),
                (Some(Piece::Names(last, _)), ", " | " and " | ", and " | " or " | ", or " | " + ")
                    if *last == kind
            );
            if joined {
                plain.clear();
                if let Some(Piece::Names(_, count)) = pieces.last_mut() {
                    *count += 1;
                }
            } else {
                if !plain.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut plain)));
                }
                pieces.push(Piece::Names(kind, 1));
            }
            i = end;
            continue;
        }
        let c = text[i..].chars().next().expect("a character at a boundary");
        plain.push(c);
        i += c.len_utf8();
    }
    if !found {
        return (text.to_owned(), places);
    }
    if !plain.is_empty() {
        pieces.push(Piece::Text(plain));
    }
    // The last word of `text` and the byte where it starts.
    fn last_word(text: &str) -> (&str, usize) {
        let start = text
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_alphabetic())
            .map_or(0, |(k, c)| k + c.len_utf8());
        (&text[start..], start)
    }
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(text) => out.push_str(&text),
            Piece::Names(kind, count) => {
                let before = out.strip_suffix(' ').map_or(out.len(), str::len);
                let (word, start) = last_word(&out[..before]);
                let lower = word.to_lowercase();
                if before < out.len() && KINDS.contains(&lower.as_str()) {
                    // "of edge E{…}": the word stays, without the name.
                    out.truncate(before);
                    out = with_article(out, start);
                } else {
                    let (one, many) = kind.words();
                    let phrase = if count > 1 {
                        many.to_owned()
                    } else {
                        format!("{} {one}", article(one))
                    };
                    let starts = out.trim_end().is_empty()
                        || out.trim_end().ends_with(|c: char| ".!?".contains(c));
                    if starts {
                        let mut chars = phrase.chars();
                        let first = chars.next().map(|c| c.to_ascii_uppercase());
                        out.extend(first);
                        out.push_str(chars.as_str());
                    } else {
                        out.push_str(&phrase);
                    }
                }
            }
        }
    }
    // What taking the names out left: doubled spaces, empty brackets.
    let mut message = out;
    for (from, to) in [
        ("()", ""),
        ("( ", "("),
        (" )", ")"),
        (" ,", ","),
        ("  ", " "),
    ] {
        while message.contains(from) {
            message = message.replace(from, to);
        }
    }
    (message.trim().to_owned(), places)
}

const FOUND_AS: &str = " found as ";
const AFTER_CHANGE: &str = " after a change in ";
const FOUND_AGAIN: &str = " found again after a change in ";

/// A reference a sketch change renamed, in plain words with its places
/// ([`plain_message`]): `[prefix ]<kind> <name> found as <name>[ + <name>…]
/// after a change in <sketch>`, the names whole and of that kind (the
/// warning of `renamed::warning` and `FoundRegions::warning`, which the
/// warnings keep for Auto Fix). None for any other text.
fn found_again(text: &str) -> Option<(String, Vec<NoticePlace>)> {
    let (head, rest) = text.split_once(FOUND_AS)?;
    let (found, sketch) = rest.split_once(AFTER_CHANGE)?;
    let (before, stored) = head.rsplit_once(' ')?;
    let (prefix, word) = match before.rsplit_once(' ') {
        Some((prefix, word)) => (Some(prefix), word),
        None => (None, before),
    };
    let lower = word.to_lowercase();
    let kind = NamedKind::of_word(&lower)?;
    let whole = |name: &str| name_at(name, 0) == Some((kind, name.len()));
    let found: Vec<&str> = found.split(" + ").collect();
    if sketch.is_empty() || !whole(stored) || !found.iter().all(|name| whole(name)) {
        return None;
    }
    let (sketch, sketch_places) = plain_message(sketch);
    let capital = word.starts_with(|c: char| c.is_uppercase());
    let (prefix, mut places) = match prefix {
        Some(prefix) => {
            let (prefix, places) = plain_message(prefix);
            (Some(prefix), places)
        }
        None => (None, Vec::new()),
    };
    // A loft's section before it ("section 2: "), part of the whole message
    // (mitcad#1688).
    let section = prefix
        .as_deref()
        .and_then(section_number)
        .filter(|_| !capital);
    let said = if found.len() == 1 {
        found_again_text(kind, 1, &sketch, section)?
    } else {
        found_as_text(kind, found.len(), &sketch, section)?
    };
    let said = if capital {
        let mut chars = said.chars();
        chars
            .next()
            .map(|c| c.to_ascii_uppercase())
            .into_iter()
            .chain(chars)
            .collect()
    } else {
        said
    };
    let message = match (&prefix, section) {
        (Some(prefix), None) => format!("{prefix} {said}"),
        _ => said,
    };
    places.extend(kind.place(stored));
    places.extend(found.iter().filter_map(|name| kind.place(name)));
    places.extend(sketch_places);
    Some((message, unique(places)))
}

/// The number of a loft's section in a prefix before a reference
/// (`section 2:`, with or without the space after it); None for any other
/// prefix.
fn section_number(prefix: &str) -> Option<&str> {
    let number = prefix
        .trim_end()
        .strip_prefix("section ")?
        .strip_suffix(':')?;
    (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())).then_some(number)
}

/// `count` references of a kind found again after a change in `sketch`
/// ("an edge found again after a change in Sketch1", "2 edges found again
/// after a change in Sketch1"), in a loft's `section` when given ("section
/// 2: an edge found again after a change in Sketch1"): one whole message
/// per kind, count and prefix, so that its translation gets no English
/// word as a value (mitcad#1674, mitcad#1688, docs/translations.md "The
/// model's messages"). None for a kind no reference of a sketch change is.
fn found_again_text(
    kind: NamedKind,
    count: usize,
    sketch: &str,
    section: Option<&str>,
) -> Option<String> {
    Some(match (kind, count == 1, section) {
        (NamedKind::Face, true, None) => format!("a face found again after a change in {sketch}"),
        (NamedKind::Face, false, None) => {
            format!("{count} faces found again after a change in {sketch}")
        }
        (NamedKind::Edge, true, None) => format!("an edge found again after a change in {sketch}"),
        (NamedKind::Edge, false, None) => {
            format!("{count} edges found again after a change in {sketch}")
        }
        (NamedKind::Vertex, true, None) => {
            format!("a vertex found again after a change in {sketch}")
        }
        (NamedKind::Vertex, false, None) => {
            format!("{count} vertices found again after a change in {sketch}")
        }
        (NamedKind::Region, true, None) => {
            format!("a region found again after a change in {sketch}")
        }
        (NamedKind::Region, false, None) => {
            format!("{count} regions found again after a change in {sketch}")
        }
        (NamedKind::Face, true, Some(n)) => {
            format!("section {n}: a face found again after a change in {sketch}")
        }
        (NamedKind::Face, false, Some(n)) => {
            format!("section {n}: {count} faces found again after a change in {sketch}")
        }
        (NamedKind::Edge, true, Some(n)) => {
            format!("section {n}: an edge found again after a change in {sketch}")
        }
        (NamedKind::Edge, false, Some(n)) => {
            format!("section {n}: {count} edges found again after a change in {sketch}")
        }
        (NamedKind::Vertex, true, Some(n)) => {
            format!("section {n}: a vertex found again after a change in {sketch}")
        }
        (NamedKind::Vertex, false, Some(n)) => {
            format!("section {n}: {count} vertices found again after a change in {sketch}")
        }
        (NamedKind::Region, true, Some(n)) => {
            format!("section {n}: a region found again after a change in {sketch}")
        }
        (NamedKind::Region, false, Some(n)) => {
            format!("section {n}: {count} regions found again after a change in {sketch}")
        }
        _ => return None,
    })
}

/// A reference of a kind found as `found` references after a change in
/// `sketch` ("a region found as 2 regions after a change in Sketch1"), in a
/// loft's `section` when given, one whole message per kind and prefix as
/// [`found_again_text`].
fn found_as_text(
    kind: NamedKind,
    found: usize,
    sketch: &str,
    section: Option<&str>,
) -> Option<String> {
    Some(match (kind, section) {
        (NamedKind::Face, None) => {
            format!("a face found as {found} faces after a change in {sketch}")
        }
        (NamedKind::Edge, None) => {
            format!("an edge found as {found} edges after a change in {sketch}")
        }
        (NamedKind::Vertex, None) => {
            format!("a vertex found as {found} vertices after a change in {sketch}")
        }
        (NamedKind::Region, None) => {
            format!("a region found as {found} regions after a change in {sketch}")
        }
        (NamedKind::Face, Some(n)) => {
            format!("section {n}: a face found as {found} faces after a change in {sketch}")
        }
        (NamedKind::Edge, Some(n)) => {
            format!("section {n}: an edge found as {found} edges after a change in {sketch}")
        }
        (NamedKind::Vertex, Some(n)) => {
            format!("section {n}: a vertex found as {found} vertices after a change in {sketch}")
        }
        (NamedKind::Region, Some(n)) => {
            format!("section {n}: a region found as {found} regions after a change in {sketch}")
        }
        _ => return None,
    })
}

/// The parts of a message [`found_again`] made for one reference: the
/// text before it (with its space), its kind and the sketch.
fn found_again_parts(message: &str) -> Option<(&str, NamedKind, &str)> {
    let (head, sketch) = message.split_once(FOUND_AGAIN)?;
    let (before, word) = head.rsplit_once(' ')?;
    let kind = NamedKind::of_word(word)?;
    let (prefix, said) = match before.rsplit_once(' ') {
        Some((prefix, said)) => (&before[..prefix.len() + 1], said),
        None => ("", before),
    };
    said.eq_ignore_ascii_case(article(word))
        .then_some((prefix, kind, sketch))
}

/// Notices that say the same of several references (mitcad#1334): the
/// references of one target a change in one sketch renamed, "an edge found
/// again after a change in Sketch1" once per edge, are one notice, "2 edges
/// found again after a change in Sketch1", with all their places, where the
/// first one was. Other notices are as they were.
pub fn merge_found_again(notices: Vec<Notice>) -> Vec<Notice> {
    let mut merged: Vec<(Notice, usize)> = Vec::new();
    for notice in notices {
        let known = if found_again_parts(&notice.message).is_some() {
            merged.iter().position(|(m, _)| {
                m.level == notice.level && m.target == notice.target && m.message == notice.message
            })
        } else {
            None
        };
        match known {
            Some(k) => {
                let (first, count) = &mut merged[k];
                *count += 1;
                first.places = unique(
                    std::mem::take(&mut first.places)
                        .into_iter()
                        .chain(notice.places),
                );
            }
            None => merged.push((notice, 1)),
        }
    }
    merged
        .into_iter()
        .map(|(mut notice, count)| {
            if count > 1
                && let Some((prefix, kind, sketch)) = found_again_parts(&notice.message)
                && let Some(said) = found_again_text(kind, count, sketch, section_number(prefix))
            {
                notice.message = if section_number(prefix).is_some() {
                    said
                } else {
                    format!("{prefix}{said}")
                };
            }
            notice
        })
        .collect()
}

/// The worst level of `notices`; None when there are none.
pub fn worst<'a>(notices: impl IntoIterator<Item = &'a Notice>) -> Option<NoticeLevel> {
    notices.into_iter().map(|n| n.level).max()
}

/// Notices of one level, one per message.
pub fn of_level<I>(level: NoticeLevel, messages: I) -> Vec<Notice>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    messages
        .into_iter()
        .map(|m| Notice::new(level, m))
        .collect()
}

/// The notices of a result's `"warnings"` array (mitcad#1040: the key that
/// command results had before `"notices"`): a text is a warning; an object
/// is a warning with its `message` (or `text`) and its `target` (or `uid`,
/// `id`) when it names one.
pub fn from_warnings(warnings: &Value) -> Vec<Notice> {
    warnings
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|warning| match warning {
            Value::String(text) if !text.is_empty() => Some(Notice::warning(text.as_str())),
            Value::Object(object) => {
                let message = ["message", "text"]
                    .iter()
                    .find_map(|key| object.get(*key).and_then(Value::as_str))
                    .map_or_else(|| warning.to_string(), str::to_owned);
                let target = ["target", "uid", "id"]
                    .iter()
                    .find_map(|key| object.get(*key))
                    .and_then(|t| match t {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    });
                let notice = Notice::warning(message);
                Some(match target {
                    Some(target) => notice.with_target(target),
                    None => notice,
                })
            }
            Value::String(_) | Value::Null => None,
            other => Some(Notice::warning(other.to_string())),
        })
        .collect()
}

/// Gives a command's or query's result (a JSON object) its `"notices"`
/// (mitcad#1040): those it has, then one warning per entry of its
/// `"warnings"` (of its `"warning_messages"`, the same in plain words, where
/// it has them; mitcad#1612), which stays beside them for one release so
/// that readers of that key keep working (`commands.md`, "Notices"). A result without
/// either gets none; a value that is not an object is left as it is.
pub fn add_notices(result: &mut Value) {
    let Some(object) = result.as_object_mut() else {
        return;
    };
    let mut notices: Vec<Value> = object
        .get("notices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // A sketch command's warnings in plain words where it gives them
    // (mitcad#1612: `warnings` keeps the agents' texts with ids).
    if let Some(warnings) = object
        .get("warning_messages")
        .or_else(|| object.get("warnings"))
    {
        for notice in from_warnings(warnings) {
            let value = notice.to_json();
            // A command that gave its warnings as notices already lists them.
            let listed = notices
                .iter()
                .any(|n| n["message"] == value["message"] && n["level"] == value["level"]);
            if !listed {
                notices.push(value);
            }
        }
    }
    if !notices.is_empty() {
        object.insert("notices".to_owned(), Value::Array(notices));
    }
}

/// The notices of a result as JSON values.
pub fn to_json(notices: &[Notice]) -> Value {
    Value::Array(notices.iter().map(Notice::to_json).collect())
}

/// An import's notices from its items' outcomes (mitcad#1040): one per
/// outcome whose level is a warning or an error and that some items had,
/// the worst first, each saying with `describe` what that many items did.
/// The outcomes keep their own words in the import's report rows; the
/// information ones (items that came in as asked) say nothing here.
pub fn outcome_notices<O: Copy>(
    counts: impl IntoIterator<Item = (O, usize)>,
    level: impl Fn(O) -> NoticeLevel,
    describe: impl Fn(O, usize) -> String,
) -> Vec<Notice> {
    let mut notices: Vec<Notice> = counts
        .into_iter()
        .filter(|(outcome, count)| *count > 0 && level(*outcome) > NoticeLevel::Info)
        .map(|(outcome, count)| Notice::new(level(outcome), describe(outcome, count)))
        .collect();
    // Stable: outcomes of one level keep their order.
    notices.sort_by_key(|notice| std::cmp::Reverse(notice.level));
    notices
}

/// "1 item", "3 items": a count with its noun.
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_order_from_info_to_error() {
        assert!(NoticeLevel::Info < NoticeLevel::Warning);
        assert!(NoticeLevel::Warning < NoticeLevel::Error);
        let notices = [Notice::info("a"), Notice::error("b"), Notice::warning("c")];
        assert_eq!(worst(&notices), Some(NoticeLevel::Error));
        assert_eq!(worst(std::iter::empty()), None);
    }

    #[test]
    fn levels_have_their_wire_names() {
        for level in [NoticeLevel::Info, NoticeLevel::Warning, NoticeLevel::Error] {
            assert_eq!(NoticeLevel::parse(level.as_str()), Some(level));
            assert_eq!(serde_json::to_value(level).unwrap(), json!(level.as_str()));
        }
        assert_eq!(NoticeLevel::parse("hint"), None);
    }

    #[test]
    fn a_notice_serializes_with_its_target_only_when_it_has_one() {
        assert_eq!(
            Notice::warning("lost 2 edges").to_json(),
            json!({"level": "warning", "message": "lost 2 edges"})
        );
        let targeted = Notice::error("broken").with_target("A3");
        assert_eq!(
            targeted.to_json(),
            json!({"level": "error", "message": "broken", "target": "A3"})
        );
        assert_eq!(serde_json::to_value(&targeted).unwrap(), targeted.to_json());
        let read: Notice =
            serde_json::from_value(json!({"level": "info", "message": "m"})).unwrap();
        assert_eq!(read, Notice::info("m"));
    }

    #[test]
    fn warnings_become_warning_notices() {
        let notices = from_warnings(&json!([
            "a text",
            {"message": "an object", "uid": 7},
            {"text": "a text field", "target": "F2"},
            null,
            ""
        ]));
        assert_eq!(
            notices,
            vec![
                Notice::warning("a text"),
                Notice::warning("an object").with_target("7"),
                Notice::warning("a text field").with_target("F2"),
            ]
        );
        assert!(from_warnings(&json!(null)).is_empty());
    }

    #[test]
    fn a_result_gets_notices_from_its_warnings_and_keeps_them() {
        let mut result = json!({"uid": 3, "warnings": ["angle over one turn"]});
        add_notices(&mut result);
        assert_eq!(result["warnings"], json!(["angle over one turn"]));
        assert_eq!(
            result["notices"],
            json!([{"level": "warning", "message": "angle over one turn"}])
        );
        // Notices it has stay first and are not repeated.
        let mut both = json!({
            "notices": [{"level": "info", "message": "kept"},
                        {"level": "warning", "message": "w"}],
            "warnings": ["w", "x"]
        });
        add_notices(&mut both);
        assert_eq!(
            both["notices"],
            json!([{"level": "info", "message": "kept"},
                   {"level": "warning", "message": "w"},
                   {"level": "warning", "message": "x"}])
        );
        // None without either.
        let mut plain = json!({"uid": 1});
        add_notices(&mut plain);
        assert!(plain.get("notices").is_none());
        let mut empty = json!({"warnings": []});
        add_notices(&mut empty);
        assert!(empty.get("notices").is_none());
        let mut text = json!("not an object");
        add_notices(&mut text);
        assert_eq!(text, json!("not an object"));
    }

    #[test]
    fn outcomes_give_notices_of_their_levels_the_worst_first() {
        #[derive(Clone, Copy)]
        enum Outcome {
            Built,
            Skipped,
            Failed,
        }
        let level = |o: Outcome| match o {
            Outcome::Built => NoticeLevel::Info,
            Outcome::Skipped => NoticeLevel::Warning,
            Outcome::Failed => NoticeLevel::Error,
        };
        let describe = |o: Outcome, n: usize| match o {
            Outcome::Built => format!("{} built", count(n, "item", "items")),
            Outcome::Skipped => format!("{} skipped", count(n, "item", "items")),
            Outcome::Failed => format!("{} failed", count(n, "item", "items")),
        };
        let counts = [
            (Outcome::Built, 5),
            (Outcome::Skipped, 3),
            (Outcome::Failed, 1),
        ];
        assert_eq!(
            outcome_notices(counts, level, describe),
            vec![
                Notice::error("1 item failed"),
                Notice::warning("3 items skipped")
            ]
        );
        let clean = [(Outcome::Built, 5), (Outcome::Skipped, 0)];
        assert!(outcome_notices(clean, level, describe).is_empty());
    }

    #[test]
    fn topological_names_become_places_and_leave_plain_words() {
        // After the word of its kind: the word stays, with an article.
        let (text, places) = plain_message(
            "the fillet of edge E{F4:side(c3[b{E{F2:a|F2:b}},c4])|F5:fillet(E{F2:c|F4:end(r{c1})})} \
             reaches 0.057 mm past face F4:side(c3[b{…},c4]); check the result",
        );
        assert_eq!(
            text,
            "the fillet of an edge reaches 0.057 mm past a face; check the result"
        );
        assert_eq!(
            places,
            vec![
                NoticePlace::edge(
                    "E{F4:side(c3[b{E{F2:a|F2:b}},c4])|F5:fillet(E{F2:c|F4:end(r{c1})})}"
                ),
                NoticePlace::face("F4:side(c3[b{…},c4])"),
            ]
        );
        // Elsewhere the kind; a list of one kind is one; a piece's #k is
        // part of the name; "the" keeps its word; after a semicolon the
        // article is lower case (only a sentence start gets a capital).
        let (text, places) = plain_message(
            "the fault lies in the corner of the fillet at V{F1:import(1)|F1:import(2)|F1:import(5)}, \
             ending against faces F1:import(2), F1:import(3) and F7:fillet(E{a|b})#2; \
             Face F1:import(4) is turned over; the edge E{x|y} stays",
        );
        assert_eq!(
            text,
            "the fault lies in the corner of the fillet at a vertex, ending against faces; \
             a face is turned over; the edge stays"
        );
        assert_eq!(places.len(), 6);
        assert_eq!(places[3], NoticePlace::face("F7:fillet(E{a|b})#2"));
        assert_eq!(places[5].name(), Some("E{x|y}"));
        // A name at the start of a sentence, and in brackets.
        let (text, _) = plain_message("E{a|b} could not be rounded (E{c|d})");
        assert_eq!(text, "An edge could not be rounded (an edge)");
        // Text without names, and words that only look like them, as it was.
        for plain in [
            "lost 2 of 5 edges",
            "LINE{3} and F:side() and F2:Side(c1)",
            "an M6 thread of 6g (in 2 steps)",
        ] {
            assert_eq!(plain_message(plain), (plain.to_owned(), Vec::new()));
        }
    }

    #[test]
    fn the_fallback_cleaner_takes_out_bare_names_bodies_and_points() {
        // The owner's case (mitcad#1612): bare names, a point.
        let (text, places) = plain_message(
            "the chamfer of edge E{F35:end(r{c1})|F37:side(c5[c4,c6])} reaches exactly to the far \
             edge of face F35:end where face F35:side(c1) lies beyond it, at (28.28, -28.28, 202): \
             it cannot be built at this size",
        );
        assert_eq!(
            text,
            "the chamfer of an edge reaches exactly to the far edge of a face where a face lies \
             beyond it, at a point: it cannot be built at this size"
        );
        assert_eq!(
            places,
            vec![
                NoticePlace::edge("E{F35:end(r{c1})|F37:side(c5[c4,c6])}"),
                NoticePlace::face("F35:end"),
                NoticePlace::face("F35:side(c1)"),
                NoticePlace::point([28.28, -28.28, 202.0]),
            ]
        );
        assert_eq!(user_text_leaks(&text), None);
        // A body's uid; a sketch's point (no place of its own).
        let (text, places) = plain_message("Fillet1: F2.b0 is gone; the end at (1.5, -2) moved");
        assert_eq!(text, "Fillet1: a body is gone; the end at a point moved");
        assert!(places.is_empty());
        let (text, _) = plain_message("a face could not be triangulated: F1:side0 (cylinder)");
        assert_eq!(text, "a face could not be triangulated: a face (cylinder)");
    }

    #[test]
    fn leaks_are_names_ids_and_coordinates() {
        for leaking in [
            "the edge E{F2:a|F2:b} folds",
            "an unclosed E{F2:a",
            "face F1:top is gone",
            "region r{c1} is open",
            "k5, p15 and c14 contradict each other",
            "Fillet1 needs F57",
            "body F2.b0 is gone",
            "at (28.28, -28.28, 202)",
            "the point (1, 2) of the sketch",
        ] {
            assert!(user_text_leaks(leaking).is_some(), "{leaking}");
        }
        for plain in [
            "the chamfer reaches exactly to the far edge of the face beside it",
            "Chamfer4: a thread of M6x1 (6g) is 12.5 mm long",
            "Sketch1 has no result; press Esc",
            "the 2.00 mm distance can't hold (3 more)",
        ] {
            assert_eq!(user_text_leaks(plain), None, "{plain}");
        }
    }

    #[test]
    fn a_failure_travels_as_text_with_its_places_and_detail() {
        let error = FeatureError::new(
            "the chamfer reaches exactly to the far edge of the face beside it, so that face \
             would end in a point; make the chamfer a little smaller or larger",
        )
        .at(NoticePlace::edge("E{F35:end|F37:side(c5)}"))
        .at(NoticePlace::point([28.28, -28.28, 202.0]))
        .with_detail(
            "the chamfer of edge E{F35:end|F37:side(c5)} reaches exactly to the far edge of face \
             F35:end at (28.28, -28.28, 202)",
        );
        let text = error.text();
        assert!(
            text.starts_with(&format!("{}{PLACED_TAIL}", error.message)),
            "{text}"
        );
        assert_eq!(FeatureError::from_text(&text), error);
        // Read as text it is the agent's.
        assert!(error.contains("E{F35:end|F37:side(c5)}"));
        assert_eq!(error.to_string(), text);
        // Text around it: around the message and the detail.
        let read = FeatureError::from_text(&format!("Chamfer4: {text}; check it"));
        assert_eq!(
            read.message,
            format!("Chamfer4: {}; check it", error.message)
        );
        assert_eq!(read.detail, format!("Chamfer4: {}; check it", error.detail));
        assert_eq!(read.places, error.places);
        // A plain text is its message only.
        let plain = FeatureError::from_text("fillet failed");
        assert_eq!(plain, FeatureError::new("fillet failed"));
        assert_eq!(plain.text(), "fillet failed");
        assert!(plain == "fillet failed");
        // An older text with names: cleaned, the text its detail.
        let old = FeatureError::from_text("body F2.b0 has no face F2:end(r{c9})");
        assert_eq!(old.message, "a body has no face");
        assert_eq!(old.detail, "body F2.b0 has no face F2:end(r{c9})");
        assert_eq!(old.places, vec![NoticePlace::face("F2:end(r{c9})")]);
        // Its notice.
        let notice = Notice::error(text.as_str());
        assert_eq!(notice.message, error.message);
        assert_eq!(notice.places, error.places);
        // A text that only looks like a tail stays.
        let odd = format!("x{PLACED_TAIL}not json]");
        assert_eq!(FeatureError::from_text(&odd).message, odd);
        // About a face: the inner message, the face named in the detail.
        let about = FeatureError::from_text(&about_place(
            NoticePlace::face("F2:top"),
            "the face is not planar",
        ));
        assert_eq!(about.message, "the face is not planar");
        assert_eq!(about.detail, "face F2:top: the face is not planar");
        assert_eq!(about.places, vec![NoticePlace::face("F2:top")]);
    }

    #[test]
    fn a_notice_keeps_its_places_with_their_names_and_bodies() {
        let notice = Notice::warning("the fillet of edge E{a|b} folds")
            .with_target("F7")
            .with_places([
                NoticePlace::face("F7:fillet(E{a|b})"),
                NoticePlace::point([1.0, 2.0, 3.0]),
            ])
            .in_body("F2.b0");
        assert_eq!(notice.message, "the fillet of an edge folds");
        assert_eq!(
            notice.to_json(),
            json!({"level": "warning", "message": "the fillet of an edge folds", "target": "F7",
                   "places": [{"kind": "edge", "body": "F2.b0", "name": "E{a|b}"},
                              {"kind": "face", "body": "F2.b0", "name": "F7:fillet(E{a|b})"},
                              {"kind": "point", "at": [1.0, 2.0, 3.0]}]})
        );
        assert_eq!(serde_json::to_value(&notice).unwrap(), notice.to_json());
        let read: Notice = serde_json::from_value(notice.to_json()).unwrap();
        assert_eq!(read, notice);
        // A plain message has none.
        assert!(Notice::info("built 0.1 % smaller").places.is_empty());
    }

    #[test]
    fn region_names_become_places_of_their_sketch() {
        // A region a sketch change renamed is found again; its keys are
        // the places.
        let (text, places) = plain_message(
            "region r{c7[c12,c13],c11[c9,c10]} found as r{c7[c12,c13],c15[c10,c9]} \
             after a change in Sketch1",
        );
        assert_eq!(text, "a region found again after a change in Sketch1");
        assert_eq!(
            places,
            vec![
                NoticePlace::region("r{c7[c12,c13],c11[c9,c10]}"),
                NoticePlace::region("r{c7[c12,c13],c15[c10,c9]}"),
            ]
        );
        // Split in two, after a prefix.
        let (text, places) = plain_message(
            "section 2: region r{c3[c6,c8]} found as r{c1[c6,c8]} + r{c1[c6,c8],c3[c6,c8]} \
             after a change in Sketch1",
        );
        assert_eq!(
            text,
            "section 2: a region found as 2 regions after a change in Sketch1"
        );
        assert_eq!(places.len(), 3);
        // Elsewhere as the other names: after the word of a kind, in a list
        // joined by `+`, inside a face's name.
        let (text, places) = plain_message("Sketch1 has no profile r{c1}");
        assert_eq!(text, "Sketch1 has no profile");
        assert_eq!(places, vec![NoticePlace::region("r{c1}")]);
        let (text, places) = plain_message("of the regions r{c1} + r{c2} the first is used");
        assert_eq!(text, "of the regions the first is used");
        assert_eq!(places.len(), 2);
        let (text, _) = plain_message("of the pieces of r{c1} the first is used");
        assert_eq!(text, "of the pieces of a region the first is used");
        let (text, places) = plain_message("face F2:end(r{c1}) is gone");
        assert_eq!(text, "a face is gone");
        assert_eq!(places, vec![NoticePlace::face("F2:end(r{c1})")]);
        // Words that only look like a key stay.
        for plain in [
            "for{a} and Sketch1 r(c1)",
            "a found as b after a change in Sketch1",
        ] {
            assert_eq!(plain_message(plain), (plain.to_owned(), Vec::new()));
        }
        // With the sketch given, the regions its message named are of it.
        let notice = Notice::warning("region r{a} found as r{b} after a change in Sketch1")
            .with_target("F2")
            .with_places([NoticePlace::region_of("F1", "r{a}")]);
        assert_eq!(
            notice.to_json(),
            json!({"level": "warning", "message": "a region found again after a change in Sketch1",
                   "target": "F2",
                   "places": [{"kind": "region", "sketch": "F1", "name": "r{a}"},
                              {"kind": "region", "sketch": "F1", "name": "r{b}"}]})
        );
        let read: Notice = serde_json::from_value(notice.to_json()).unwrap();
        assert_eq!(read, notice);
        assert_eq!(notice.places[0].name(), Some("r{a}"));
        assert_eq!(notice.places[0].sketch(), Some("F1"));
        assert_eq!(notice.places[0].body(), None);
    }

    #[test]
    fn renamed_references_are_one_sentence_per_feature() {
        let edge = |stored: &str, found: &str, target: &str| {
            Notice::warning(format!(
                "edge {stored} found as {found} after a change in Sketch1"
            ))
            .with_target(target)
        };
        let one = edge("E{F2:a|F2:b}", "E{F2:a|F2:c}", "F3");
        assert_eq!(one.message, "an edge found again after a change in Sketch1");
        assert_eq!(
            one.places,
            vec![
                NoticePlace::edge("E{F2:a|F2:b}"),
                NoticePlace::edge("E{F2:a|F2:c}")
            ]
        );
        let notices = merge_found_again(vec![
            one,
            Notice::warning("lost 1 of 3 edges").with_target("F3"),
            edge("E{F2:d|F2:e}", "E{F2:d|F2:f}", "F3"),
            Notice::warning("face F2:side(g) found as F2:side(h) after a change in Sketch2")
                .with_target("F3"),
            edge("E{F2:d|F2:e}", "E{F2:d|F2:f}", "F4"),
            Notice::warning("face F2:side(a) found as nothing after a change in Sketch1")
                .with_target("F3"),
        ]);
        let messages: Vec<&str> = notices.iter().map(|n| n.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "2 edges found again after a change in Sketch1",
                "lost 1 of 3 edges",
                "a face found again after a change in Sketch2",
                "an edge found again after a change in Sketch1",
                "a face found as nothing after a change in Sketch1",
            ]
        );
        assert_eq!(notices[0].places.len(), 4);
        assert_eq!(notices[0].places[2], NoticePlace::edge("E{F2:d|F2:e}"));
        assert_eq!(notices[3].target.as_deref(), Some("F4"));
        // A prefix stays before the count.
        let section = |stored: &str| {
            Notice::warning(format!(
                "section 2: vertex {stored} found as V{{x|y|z}} after a change in Sketch1"
            ))
        };
        let notices = merge_found_again(vec![section("V{a|b|c}"), section("V{d|e|f}")]);
        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices[0].message,
            "section 2: 2 vertices found again after a change in Sketch1"
        );
        // Regions and faces likewise, each kind's words whole in the
        // message (mitcad#1674: no kind's word put in as a value).
        let region = |stored: &str| {
            Notice::warning(format!(
                "region r{{{stored}}} found as r{{x}} after a change in Sketch4"
            ))
            .with_target("F5")
        };
        let notices = merge_found_again(vec![region("a"), region("b")]);
        assert_eq!(
            notices[0].message,
            "2 regions found again after a change in Sketch4"
        );
        let face = |stored: &str| {
            Notice::warning(format!(
                "face F2:side({stored}) found as F2:side(x) after a change in Sketch4"
            ))
            .with_target("F5")
        };
        let notices = merge_found_again(vec![face("a"), face("b"), face("c")]);
        assert_eq!(
            notices[0].message,
            "3 faces found again after a change in Sketch4"
        );
        let (text, _) =
            plain_message("edge E{a|b} found as E{a|c} + E{a|d} after a change in Sketch4");
        assert_eq!(text, "an edge found as 2 edges after a change in Sketch4");
        // A loft's section before the reference is part of the whole
        // message (mitcad#1688): the same English, one literal per kind,
        // count and prefix, so the prefix is translated with it.
        let (text, places) = plain_message(
            "section 3: face F2:side(a) found as F2:side(b) after a change in Sketch4",
        );
        assert_eq!(
            text,
            "section 3: a face found again after a change in Sketch4"
        );
        assert_eq!(places.len(), 2);
        let (text, _) = plain_message(
            "section 12: edge E{a|b} found as E{a|c} + E{a|d} after a change in Sketch4",
        );
        assert_eq!(
            text,
            "section 12: an edge found as 2 edges after a change in Sketch4"
        );
        let section_region = |stored: &str| {
            Notice::warning(format!(
                "section 2: region r{{{stored}}} found as r{{x}} after a change in Sketch4"
            ))
            .with_target("F5")
        };
        let notices = merge_found_again(vec![section_region("a"), section_region("b")]);
        assert_eq!(
            notices[0].message,
            "section 2: 2 regions found again after a change in Sketch4"
        );
        // Another prefix stays before the sentence as it was.
        let (text, _) =
            plain_message("rail 1: edge E{a|b} found as E{a|c} after a change in Sketch4");
        assert_eq!(
            text,
            "rail 1: an edge found again after a change in Sketch4"
        );
        assert_eq!(section_number("section 2:"), Some("2"));
        assert_eq!(section_number("section 2: "), Some("2"));
        assert_eq!(section_number("section x:"), None);
        assert_eq!(section_number("rail 1:"), None);
    }

    /// The cleaned form of a model message template (`%1`, `%2`, ...) as
    /// this cleaner shows its message (mitcad#1635): a value after the word
    /// of a kind ("line c%1", "body %1", a list "faces %1 and %2") and an
    /// id anywhere ("c%1") taken out as names, the other values kept, then
    /// numbered again. None when nothing is taken out, and for a template
    /// whose own text the cleaner changes (an id like `k0` in it) or that
    /// has a cleaner of its own (a renamed reference).
    fn cleaned_template(template: &str) -> Option<String> {
        const JOINS: [&str; 6] = [", ", " and ", ", and ", " or ", ", or ", " + "];
        if template.contains(FOUND_AS) || cleaned(template, true).0 != template {
            return None;
        }
        let bytes = template.as_bytes();
        let placeholder_at = |text: &[u8], i: usize| -> usize {
            if text[i] != b'%' || !text.get(i + 1).is_some_and(|b| (b'1'..=b'9').contains(b)) {
                return 0;
            }
            if text.get(i + 2).is_some_and(u8::is_ascii_digit) {
                3
            } else {
                2
            }
        };
        // Each value: where its name starts, its placeholder, its end and
        // its id's letter.
        let mut values: Vec<(usize, usize, usize, Option<u8>)> = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let length = placeholder_at(bytes, i);
            if length == 0 {
                i += 1;
                continue;
            }
            let id = i > 0
                && b"cpkF".contains(&bytes[i - 1])
                && word_starts(bytes, i - 1)
                && word_ends(bytes, i + length);
            values.push(if id {
                (i - 1, i, i + length, Some(bytes[i - 1]))
            } else {
                (i, i, i + length, None)
            });
            i += length;
        }
        let mut taken = vec![false; values.len()];
        let mut v = 0;
        while v < values.len() {
            let (name, _, mut end, prefix) = values[v];
            let after_kind = name >= 2 && bytes[name - 1] == b' ' && {
                let word_end = name - 1;
                let word_start = template[..word_end]
                    .bytes()
                    .rposition(|b| !b.is_ascii_alphabetic())
                    .map_or(0, |k| k + 1);
                word_start < word_end
                    && KINDS.contains(&template[word_start..word_end].to_lowercase().as_str())
            };
            // A length is no name.
            let millimetres = prefix.is_none() && template[end..].starts_with(" mm");
            if (!after_kind && prefix.is_none()) || millimetres {
                v += 1;
                continue;
            }
            taken[v] = true;
            while let Some(&(other, _, other_end, other_prefix)) = values.get(v + 1) {
                if other_prefix != prefix || other < end || !JOINS.contains(&&template[end..other])
                {
                    break;
                }
                end = other_end;
                v += 1;
                taken[v] = true;
            }
            v += 1;
        }
        if !taken.contains(&true) {
            return None;
        }
        // The message with names in the values taken out.
        let mut sample = String::new();
        let mut at = 0;
        for (value, &(_, placeholder, end, prefix)) in values.iter().enumerate() {
            if taken[value] {
                sample.push_str(&template[at..placeholder]);
                sample.push_str(if prefix.is_some() { "7" } else { "E{s}" });
                at = end;
            }
        }
        sample.push_str(&template[at..]);
        let (shown, _) = cleaned(&sample, true);
        let mut numbered = String::new();
        let mut count = 0;
        let mut i = 0;
        while i < shown.len() {
            let length = placeholder_at(shown.as_bytes(), i);
            if length == 0 {
                let c = shown[i..].chars().next().expect("a character");
                numbered.push(c);
                i += c.len_utf8();
                continue;
            }
            count += 1;
            numbered.push('%');
            numbered.push_str(&count.to_string());
            i += length;
            if shown.as_bytes().get(i).is_some_and(u8::is_ascii_digit) {
                return None;
            }
        }
        (numbered != template).then_some(numbered)
    }

    /// The model's messages are translated as they are shown (mitcad#1635):
    /// the generated list of templates (`mitcad-translations messages`,
    /// `app/framework/ModelMessageTemplates.cpp`) has the cleaned form of
    /// each template whose values this cleaner takes out, so that "line c4
    /// does not exist", shown "a line does not exist", has its translation.
    #[test]
    fn cleaned_templates_are_listed() {
        let list = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../app/framework/ModelMessageTemplates.cpp");
        let Ok(source) = std::fs::read_to_string(&list) else {
            // The crate without the application's sources.
            return;
        };
        let head = "    QT_TRANSLATE_NOOP(\"ModelMessage\", \"";
        let listed: std::collections::BTreeSet<String> = source
            .lines()
            .filter_map(|line| {
                let literal = line.strip_prefix(head)?.strip_suffix("\"),")?;
                let mut text = String::new();
                let mut chars = literal.chars();
                while let Some(c) = chars.next() {
                    if c != '\\' {
                        text.push(c);
                    } else if let Some(escaped) = chars.next() {
                        text.push(if escaped == 'n' { '\n' } else { escaped });
                    }
                }
                Some(text)
            })
            .collect();
        assert!(listed.len() > 1000, "{}: no templates read", list.display());
        // The ticket's two: a line that does not exist, a body of another
        // component.
        assert_eq!(
            cleaned_template("line c%1 does not exist").as_deref(),
            Some("a line does not exist")
        );
        assert_eq!(
            cleaned_template(
                "body %1 (from %2) belongs to %3, not to %4 where the transfer takes it from"
            )
            .as_deref(),
            Some("a body (from %1) belongs to %2, not to %3 where the transfer takes it from")
        );
        assert_eq!(
            cleaned_template("c%1 and c%2 are tangent at p%3: there is no corner to round")
                .as_deref(),
            Some("Curves are tangent at a point: there is no corner to round")
        );
        assert_eq!(cleaned_template("the radius %1 must be positive"), None);
        // No second article in a noun phrase that has one (mitcad#1638).
        assert_eq!(
            cleaned_template("the mirror line c%1 is not a line").as_deref(),
            Some("the mirror line is not a line")
        );
        assert_eq!(
            cleaned_template("the driven dimension %1 measures nothing").as_deref(),
            Some("the driven dimension measures nothing")
        );
        assert_eq!(
            cleaned_template("its offset %1 mm from the placements"),
            None
        );
        let missing: Vec<(&String, String)> = listed
            .iter()
            .filter_map(|template| {
                let shown = cleaned_template(template)?;
                (!listed.contains(&shown)).then_some((template, shown))
            })
            .collect();
        assert!(
            missing.is_empty(),
            "{} cleaned templates are not in {} (write it again with the target \
             update_translations, docs/translations.md): {:?}",
            missing.len(),
            list.display(),
            &missing[..missing.len().min(10)]
        );
    }

    /// The article of a name taken out after the word of its kind looks back
    /// over the noun phrase (mitcad#1638): none where the phrase has one,
    /// before its modifiers where it has none; a name elsewhere is its kind.
    #[test]
    fn the_article_looks_back_over_the_noun_phrase() {
        // The ticket's three: a driven dimension, a mirror line, a new body.
        assert_eq!(
            cleaned("Extrude1 moved what the driven dimension k5 measures", true).0,
            "Extrude1 moved what the driven dimension measures"
        );
        assert_eq!(
            cleaned("the mirror line c4 is not a line", true).0,
            "the mirror line is not a line"
        );
        assert_eq!(
            plain_message("a new body F2.b0 has no participant bodies").0,
            "a new body has no participant bodies"
        );
        // A possessive is the phrase's article.
        assert_eq!(
            plain_message("the result's face F3:end has no usable name").0,
            "the result's face has no usable name"
        );
        // Without one, the article goes before the modifiers; after a word
        // a verb follows ("not"), after the verb.
        assert_eq!(
            cleaned("the bend meets boundary curve c5 there", true).0,
            "the bend meets a boundary curve there"
        );
        assert_eq!(
            plain_message("Bend edge E{a|b} is not straight").0,
            "A bend edge is not straight"
        );
        assert_eq!(
            plain_message("Body1 does not fit face F2:top").0,
            "Body1 does not fit a face"
        );
        assert_eq!(plain_message("Create Profile E{a|b}").0, "Create a profile");
        // A name elsewhere: its kind (mitcad#1327), "at a vertex", "on an
        // edge"; the generated list has such a template's form with the kind
        // put in where the argument names it (mitcad#1638).
        let (text, places) = plain_message("the hole at V{F1:a|F1:b|F1:c} is too small");
        assert_eq!(text, "the hole at a vertex is too small");
        assert_eq!(places, vec![NoticePlace::vertex("V{F1:a|F1:b|F1:c}")]);
        assert_eq!(
            plain_message("the seam is on E{a|b}").0,
            "the seam is on an edge"
        );
    }
}
