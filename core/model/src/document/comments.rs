// SPDX-License-Identifier: MIT
//! Comments (mitcad#1119): notes pinned to the model that point an AI agent
//! (or a colleague) at the right place — "make this hole an M6 thread" on
//! a face, "this fillet is too small" on a feature. The agent reads the
//! open ones, does the work and marks each addressed with what it did; the
//! user resolves them. They are not annotations, which are dimensions and
//! notes of the design itself (`annotations.rs`).
//!
//! Each comment has an id (`C1`, `C2`, …: never reused within a document,
//! the next one is one past the largest ever used, kept as
//! `next_comment`), its text, its target ([`CommentTarget`]), a status
//! ([`CommentStatus`]: open, addressed, resolved), its author (`user`, or
//! an MCP client's name such as `claude-code`), the resolution written
//! when it was addressed and by whom, replies, and when it was written
//! (RFC 3339 UTC, whole seconds).
//!
//! A target is a timeline feature, a body or one of its faces, edges or
//! vertices by topological name, a sketch's curve, point, constraint or
//! dimension, a parameter, or a free point in its component's coordinates
//! placed by an occurrence, as an annotation's `at` is. The model checks a
//! target's form; the API (`model-api/src/comments.rs`) finds it at the
//! timeline marker, refuses one that is not there when it is added or
//! retargeted, and settles the comments after every command
//! ([`Document::settle_comments`]): a target found keeps its place as the
//! last known one, a target gone makes its comment **orphaned** (a flag,
//! not a status) at that place, until it is found again or retargeted.
//!
//! They change no geometry: each change is an undo step of its own that
//! recomputes nothing, so a model change and an `address_comment` in one
//! `model_command` array (merged into one step) undo together.

use serde::{Deserialize, Deserializer, Serialize};

use super::{Document, ModelError, invalid};
use crate::datum::Vec3;
use crate::features::{OccurrencePath, is_false};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::sketch::{ConstraintUid, Ref};
use crate::topo::{EdgeName, FaceName, VertexName};

/// The author of the comments the user writes in the application, and of
/// those a command adds without an author.
pub const COMMENT_USER: &str = "user";

/// Places closer than this (mm) are the same last known place: the
/// baseline tolerance of geometric comparisons.
const SAME_PLACE: f64 = 1e-6;

/// Where a comment is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentStatus {
    /// Waiting for the work it asks for.
    Open,
    /// An agent (or the user) did the work and said what in its
    /// resolution; the user has not resolved it yet.
    Addressed,
    /// Done: the user resolved it.
    Resolved,
}

impl CommentStatus {
    /// The status in files, commands and queries.
    pub fn key(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Addressed => "addressed",
            Self::Resolved => "resolved",
        }
    }
}

/// What a comment is pinned to, in the fields of the `measure` query's
/// picks: one of `{"feature"}`, `{"body"}` with an optional `face`, `edge`
/// or `vertex` and `occurrence`, `{"sketch"}` with one of `curve`, `point`,
/// `constraint` or `dimension`, `{"parameter"}`, or `{"at"}` (a point in
/// its component's coordinates) with an optional `component` (empty or
/// left out: the root) and `occurrence`. An empty `occurrence` is the
/// root's (or the owner's only placement), as in `measure`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<FeatureUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<FaceName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<EdgeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertex: Option<VertexName>,
    /// A sketch (its feature uid) with one of the four below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<FeatureUid>,
    /// A sketch's curve, `c<n>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<String>,
    /// A sketch's point, `p<n>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<String>,
    /// A sketch's constraint, `k<n>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<String>,
    /// A sketch's dimension, `k<n>` (constraints and dimensions share the
    /// numbers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    /// A parameter, by its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter: Option<String>,
    /// A free point, mm in its component's coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Vec3>,
    /// The component a free point is in; none (or `""`) for the root.
    #[serde(
        default,
        deserialize_with = "component_or_root",
        skip_serializing_if = "Option::is_none"
    )]
    pub component: Option<ComponentUid>,
    /// The occurrence that places a body or a free point's component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
}

/// A component id, or the root for an empty one (`""`, null).
fn component_or_root<'de, D: Deserializer<'de>>(d: D) -> Result<Option<ComponentUid>, D::Error> {
    let text = Option::<String>::deserialize(d)?;
    match text.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(text) => text
            .parse::<ComponentUid>()
            .map(|uid| Some(uid).filter(|uid| !uid.is_root()))
            .map_err(serde::de::Error::custom),
    }
}

/// A sketch's item a comment is pinned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentSketchItem {
    /// A curve (`c<n>`) or a point (`p<n>`).
    Entity(Ref),
    Constraint(ConstraintUid),
    Dimension(ConstraintUid),
}

/// What a [`CommentTarget`] is, by its fields ([`CommentTarget::kind`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommentTargetKind<'a> {
    Feature(FeatureUid),
    Body(BodyUid),
    Face(BodyUid, &'a FaceName),
    Edge(BodyUid, &'a EdgeName),
    Vertex(BodyUid, &'a VertexName),
    SketchItem {
        sketch: FeatureUid,
        item: CommentSketchItem,
    },
    Parameter(&'a str),
    /// A free point in its component's coordinates.
    Point(Vec3),
}

/// The forms a target takes, for an error.
const TARGET_FORMS: &str = "a target is {\"feature\"}, {\"body\"} with an optional \"face\", \
     \"edge\" or \"vertex\" and \"occurrence\", {\"sketch\"} with one \"curve\", \"point\", \
     \"constraint\" or \"dimension\", {\"parameter\"}, or {\"at\": [x, y, z]} with an optional \
     \"component\" and \"occurrence\"";

impl CommentTarget {
    /// What the target is, or why its fields are no target (not whether
    /// it is in the model: the API finds that).
    pub fn kind(&self) -> Result<CommentTargetKind<'_>, String> {
        let count = |flags: &[bool]| flags.iter().filter(|flag| **flag).count();
        let groups = count(&[
            self.feature.is_some(),
            self.body.is_some(),
            self.sketch.is_some(),
            self.parameter.is_some(),
            self.at.is_some(),
        ]);
        let names = count(&[
            self.face.is_some(),
            self.edge.is_some(),
            self.vertex.is_some(),
        ]);
        let items = count(&[
            self.curve.is_some(),
            self.point.is_some(),
            self.constraint.is_some(),
            self.dimension.is_some(),
        ]);
        let placed = self.occurrence.is_some();
        let in_component = self.component.is_some();
        let form = || TARGET_FORMS.to_owned();
        if groups != 1 {
            return Err(form());
        }
        if let Some(uid) = self.feature {
            if names + items > 0 || placed || in_component {
                return Err(form());
            }
            return Ok(CommentTargetKind::Feature(uid));
        }
        if let Some(body) = self.body {
            if names > 1 || items > 0 || in_component {
                return Err(form());
            }
            return Ok(match (&self.face, &self.edge, &self.vertex) {
                (Some(face), _, _) => CommentTargetKind::Face(body, face),
                (_, Some(edge), _) => CommentTargetKind::Edge(body, edge),
                (_, _, Some(vertex)) => CommentTargetKind::Vertex(body, vertex),
                _ => CommentTargetKind::Body(body),
            });
        }
        if let Some(sketch) = self.sketch {
            if names > 0 || items != 1 || placed || in_component {
                return Err(form());
            }
            let entity = |text: &str, point: bool| -> Result<CommentSketchItem, String> {
                let found = text.trim().parse::<Ref>().map_err(|e| e.to_string())?;
                if found.is_point() != point {
                    return Err(format!(
                        "{text} is no {}",
                        if point {
                            "point (p<n>)"
                        } else {
                            "curve (c<n>)"
                        }
                    ));
                }
                Ok(CommentSketchItem::Entity(found))
            };
            let constraint = |text: &str| {
                text.trim()
                    .parse::<ConstraintUid>()
                    .map_err(|e| e.to_string())
            };
            let item = match (&self.curve, &self.point, &self.constraint, &self.dimension) {
                (Some(curve), _, _, _) => entity(curve, false)?,
                (_, Some(point), _, _) => entity(point, true)?,
                (_, _, Some(k), _) => CommentSketchItem::Constraint(constraint(k)?),
                (_, _, _, Some(k)) => CommentSketchItem::Dimension(constraint(k)?),
                _ => return Err(form()),
            };
            return Ok(CommentTargetKind::SketchItem { sketch, item });
        }
        if let Some(name) = &self.parameter {
            if names + items > 0 || placed || in_component {
                return Err(form());
            }
            let name = name.trim();
            if name.is_empty() {
                return Err("a parameter target names the parameter".to_owned());
            }
            return Ok(CommentTargetKind::Parameter(name));
        }
        match self.at {
            Some(at) if names + items == 0 => {
                if at.iter().any(|c| !c.is_finite()) {
                    return Err("a point's coordinates are numbers".to_owned());
                }
                Ok(CommentTargetKind::Point(at))
            }
            _ => Err(form()),
        }
    }

    /// The target as stored: names trimmed, the root's component and an
    /// empty occurrence left out, so that equal targets compare equal.
    pub fn normalized(mut self) -> Self {
        for text in [
            &mut self.curve,
            &mut self.point,
            &mut self.constraint,
            &mut self.dimension,
            &mut self.parameter,
        ]
        .into_iter()
        .flatten()
        {
            *text = text.trim().to_owned();
        }
        if self.component.is_some_and(ComponentUid::is_root) {
            self.component = None;
        }
        if self
            .occurrence
            .as_ref()
            .is_some_and(OccurrencePath::is_empty)
        {
            self.occurrence = None;
        }
        self
    }
}

/// A reply to a comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentReply {
    pub author: String,
    pub text: String,
    /// RFC 3339 UTC, whole seconds.
    pub created: String,
}

/// A comment kept in the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    /// The number of its id `C<n>`.
    #[serde(with = "comment_id")]
    pub id: u32,
    pub text: String,
    pub target: CommentTarget,
    pub status: CommentStatus,
    /// The target was not found at the marker after the last change: the
    /// comment stays at its last known place until the target is found
    /// again or the comment is retargeted.
    #[serde(default, skip_serializing_if = "is_false")]
    pub orphaned: bool,
    /// `user` or an MCP client's name.
    pub author: String,
    /// What was done, written when it was addressed; kept when it is
    /// reopened, until the next address replaces it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// Who addressed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub addressed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<CommentReply>,
    /// RFC 3339 UTC, whole seconds.
    pub created: String,
    /// The last place its target was found, mm in the design's (root's)
    /// coordinates at the marker; none for a parameter or a target that
    /// has no place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Vec3>,
}

/// A comment id as text, `C<n>`.
mod comment_id {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(id: &u32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("C{id}"))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
        let text = String::deserialize(d)?;
        super::comment_number(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid comment id {text} (expected C<number>)"))
        })
    }
}

/// The number of a comment id `C<n>` (`n` from 1), if the text is one.
pub fn comment_number(text: &str) -> Option<u32> {
    crate::ids::parse_number::<u32>(text.trim(), "C").filter(|n| *n > 0)
}

impl Comment {
    /// Its id, `C3`.
    pub fn name(&self) -> String {
        format!("C{}", self.id)
    }

    /// Why the comment is not one a command could have made (a file's).
    pub fn check(&self) -> Result<(), String> {
        if self.text.trim().is_empty() {
            return Err("the text is empty".to_owned());
        }
        if self.author.trim().is_empty() {
            return Err("the author is empty".to_owned());
        }
        self.target.kind().map_err(|e| format!("target: {e}"))?;
        if self
            .resolution
            .as_ref()
            .is_some_and(|r| r.trim().is_empty())
        {
            return Err("the resolution is empty".to_owned());
        }
        if self.place.is_some_and(|p| p.iter().any(|c| !c.is_finite())) {
            return Err("the place is not a point".to_owned());
        }
        for (i, reply) in self.replies.iter().enumerate() {
            if reply.text.trim().is_empty() || reply.author.trim().is_empty() {
                return Err(format!("replies[{i}]: a reply has an author and a text"));
            }
        }
        Ok(())
    }
}

/// Where a comment's target is at the marker after a change, for
/// [`Document::settle_comments`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommentFound {
    /// Found, at this place (none for a parameter or a target without one).
    At(Option<Vec3>),
    /// Not found: the comment is orphaned.
    Gone,
}

/// A text a comment command takes: trimmed, not empty.
fn text_of(text: &str, what: &str) -> Result<String, ModelError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(invalid(format!("{what} needs a text")));
    }
    Ok(text.to_owned())
}

/// The author a command names, or [`COMMENT_USER`].
fn author_of(author: Option<&str>) -> String {
    author
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .unwrap_or(COMMENT_USER)
        .to_owned()
}

/// Now, RFC 3339 UTC in whole seconds.
fn now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    crate::file::utc_text(seconds)
}

/// A place given, if it is a point.
fn finite(place: Option<Vec3>) -> Option<Vec3> {
    place.filter(|p| p.iter().all(|c| c.is_finite()))
}

fn same_place(a: Vec3, b: Vec3) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() <= SAME_PLACE)
}

impl<K: Kernel> Document<K> {
    /// The comments in the order they were added.
    pub fn comments(&self) -> &[Comment] {
        &self.state.comments
    }

    /// The comment of this id (`C3`).
    pub fn comment(&self, id: &str) -> Result<&Comment, ModelError> {
        Ok(&self.state.comments[self.comment_index(id)?])
    }

    /// The id the next comment takes: one past the largest ever used.
    pub fn next_comment_id(&self) -> String {
        format!("C{}", self.state.next_comment)
    }

    /// Adds an open comment on `target` (its form checked here; the API
    /// checks first that it is in the model and finds `place`, design
    /// coordinates), written now by `author` ([`COMMENT_USER`] when none);
    /// an undo step `Add Comment C1` that recomputes nothing. Returns its
    /// id.
    pub fn add_comment(
        &mut self,
        text: &str,
        target: CommentTarget,
        author: Option<&str>,
        place: Option<Vec3>,
    ) -> Result<String, ModelError> {
        let text = text_of(text, "a comment")?;
        let target = target.normalized();
        target.kind().map_err(invalid)?;
        let id = self.state.next_comment;
        let next = id
            .checked_add(1)
            .ok_or_else(|| invalid("no comment ids are left"))?;
        let comment = Comment {
            id,
            text,
            target,
            status: CommentStatus::Open,
            orphaned: false,
            author: author_of(author),
            resolution: None,
            addressed_by: None,
            replies: Vec::new(),
            created: now(),
            place: finite(place),
        };
        let name = comment.name();
        let label = format!("Add Comment {name}");
        self.apply_with(|state| {
            state.comments.push(comment);
            state.next_comment = next;
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes a comment's text; an undo step `Edit Comment C1`, none when
    /// the text stays. Returns whether it changed.
    pub fn edit_comment(&mut self, id: &str, text: &str) -> Result<bool, ModelError> {
        let i = self.comment_index(id)?;
        let text = text_of(text, "a comment")?;
        if self.state.comments[i].text == text {
            return Ok(false);
        }
        let label = format!("Edit Comment {}", self.state.comments[i].name());
        self.apply_with(|state| {
            state.comments[i].text = text;
            Ok((label, (), false))
        })?;
        Ok(true)
    }

    /// Adds a reply by `author` ([`COMMENT_USER`] when none); an undo step
    /// `Reply to C1`. The status stays.
    pub fn reply_comment(
        &mut self,
        id: &str,
        text: &str,
        author: Option<&str>,
    ) -> Result<(), ModelError> {
        let i = self.comment_index(id)?;
        let reply = CommentReply {
            author: author_of(author),
            text: text_of(text, "a reply")?,
            created: now(),
        };
        let label = format!("Reply to {}", self.state.comments[i].name());
        self.apply_with(|state| {
            state.comments[i].replies.push(reply);
            Ok((label, (), false))
        })
    }

    /// Pins a comment to another target (its form checked here; the API
    /// checks first that it is in the model and finds `place`), which
    /// clears `orphaned`; an undo step `Retarget C1`, none when nothing
    /// changes. Returns whether it changed.
    pub fn retarget_comment(
        &mut self,
        id: &str,
        target: CommentTarget,
        place: Option<Vec3>,
    ) -> Result<bool, ModelError> {
        let i = self.comment_index(id)?;
        let target = target.normalized();
        target.kind().map_err(invalid)?;
        let place = finite(place).or(self.state.comments[i].place);
        let comment = &self.state.comments[i];
        if comment.target == target && !comment.orphaned && comment.place == place {
            return Ok(false);
        }
        let label = format!("Retarget {}", comment.name());
        self.apply_with(|state| {
            let comment = &mut state.comments[i];
            comment.target = target;
            comment.orphaned = false;
            comment.place = place;
            Ok((label, (), false))
        })?;
        Ok(true)
    }

    /// Marks a comment addressed with what was done (`resolution`, not
    /// empty) and by whom ([`COMMENT_USER`] when none); an undo step
    /// `Address C1`, none when it is addressed so already. A resolved
    /// comment is reopened first.
    pub fn address_comment(
        &mut self,
        id: &str,
        resolution: &str,
        author: Option<&str>,
    ) -> Result<(), ModelError> {
        let i = self.comment_index(id)?;
        let resolution = text_of(resolution, "addressing a comment")?;
        let author = author_of(author);
        let comment = &self.state.comments[i];
        if comment.status == CommentStatus::Resolved {
            return Err(invalid(format!(
                "{} is resolved: reopen it first",
                comment.name()
            )));
        }
        if comment.status == CommentStatus::Addressed
            && comment.resolution.as_deref() == Some(resolution.as_str())
            && comment.addressed_by.as_deref() == Some(author.as_str())
        {
            return Ok(());
        }
        let label = format!("Address {}", comment.name());
        self.apply_with(|state| {
            let comment = &mut state.comments[i];
            comment.status = CommentStatus::Addressed;
            comment.resolution = Some(resolution);
            comment.addressed_by = Some(author);
            Ok((label, (), false))
        })
    }

    /// Resolves a comment, open or addressed; an undo step `Resolve C1`,
    /// none when it is resolved already. Whether an agent may resolve is
    /// the caller's to decide (the MCP layer refuses it by default).
    pub fn resolve_comment(&mut self, id: &str) -> Result<(), ModelError> {
        let i = self.comment_index(id)?;
        let comment = &self.state.comments[i];
        if comment.status == CommentStatus::Resolved {
            return Ok(());
        }
        let label = format!("Resolve {}", comment.name());
        self.apply_with(|state| {
            state.comments[i].status = CommentStatus::Resolved;
            Ok((label, (), false))
        })
    }

    /// Opens an addressed or resolved comment again, with a reply by
    /// `author` when `text` is given (why: it is not done yet); an undo
    /// step `Reopen C1`. The resolution stays until the next address
    /// replaces it. An open comment without a text changes nothing; with
    /// one, it gets the reply.
    pub fn reopen_comment(
        &mut self,
        id: &str,
        text: Option<&str>,
        author: Option<&str>,
    ) -> Result<(), ModelError> {
        let i = self.comment_index(id)?;
        let reply = match text {
            Some(text) => Some(CommentReply {
                author: author_of(author),
                text: text_of(text, "a reply")?,
                created: now(),
            }),
            None => None,
        };
        let comment = &self.state.comments[i];
        if comment.status == CommentStatus::Open && reply.is_none() {
            return Ok(());
        }
        let label = format!("Reopen {}", comment.name());
        self.apply_with(|state| {
            let comment = &mut state.comments[i];
            comment.status = CommentStatus::Open;
            comment.replies.extend(reply);
            Ok((label, (), false))
        })
    }

    /// Deletes a comment; an undo step `Delete Comment C1`. Its id is not
    /// given again.
    pub fn delete_comment(&mut self, id: &str) -> Result<(), ModelError> {
        let i = self.comment_index(id)?;
        let label = format!("Delete Comment {}", self.state.comments[i].name());
        self.apply_with(|state| {
            state.comments.remove(i);
            Ok((label, (), false))
        })
    }

    /// Takes where the comments' targets are at the marker after a change,
    /// as the API found them (by comment number): a target found keeps its
    /// place as the last known one and its comment is no orphan; a target
    /// gone makes its comment orphaned at its last known place. Neither an
    /// undo step nor a new revision: the places follow the model as the
    /// results do, and undo takes back a state as it was settled. Returns
    /// whether anything changed.
    pub fn settle_comments(&mut self, found: &[(u32, CommentFound)]) -> bool {
        let mut changed = false;
        for (id, where_now) in found {
            let Some(comment) = self.state.comments.iter_mut().find(|c| c.id == *id) else {
                continue;
            };
            match where_now {
                CommentFound::At(place) => {
                    if comment.orphaned {
                        comment.orphaned = false;
                        changed = true;
                    }
                    if let Some(place) = finite(*place)
                        && comment.place.is_none_or(|p| !same_place(p, place))
                    {
                        comment.place = Some(place);
                        changed = true;
                    }
                }
                CommentFound::Gone => {
                    if !comment.orphaned {
                        comment.orphaned = true;
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    fn comment_index(&self, id: &str) -> Result<usize, ModelError> {
        let number =
            comment_number(id).ok_or_else(|| invalid(format!("{id} is no comment id (C<n>)")))?;
        self.state
            .comments
            .iter()
            .position(|c| c.id == number)
            .ok_or_else(|| invalid(format!("there is no comment {}", id.trim())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(value: serde_json::Value) -> CommentTarget {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn targets_take_one_form() {
        use serde_json::json;
        let kinds = [
            json!({"feature": "F7"}),
            json!({"body": "F2.b0", "occurrence": "O1/O4"}),
            json!({"body": "F2.b0", "face": "F2:end(r{c1})"}),
            json!({"sketch": "F1", "curve": "c4"}),
            json!({"sketch": "F1", "point": "p3"}),
            json!({"sketch": "F1", "constraint": "k2"}),
            json!({"sketch": "F1", "dimension": "k5"}),
            json!({"parameter": "width"}),
            json!({"at": [1, 2, 3], "component": "", "occurrence": ""}),
        ];
        for value in kinds {
            assert!(target(value.clone()).kind().is_ok(), "{value}");
        }
        for value in [
            json!({}),
            json!({"feature": "F7", "body": "F2.b0"}),
            json!({"feature": "F7", "occurrence": "O1"}),
            json!({"body": "F2.b0", "face": "F2:end(r{c1})", "edge": "E{F2:a|F2:b}"}),
            json!({"sketch": "F1"}),
            json!({"sketch": "F1", "curve": "p3"}),
            json!({"sketch": "F1", "point": "c4"}),
            json!({"sketch": "F1", "curve": "c4", "point": "p3"}),
            json!({"parameter": "  "}),
            json!({"at": [1, 2, 3], "face": "F2:end(r{c1})"}),
        ] {
            assert!(target(value.clone()).kind().is_err(), "{value}");
        }
        assert!(serde_json::from_value::<CommentTarget>(json!({"face2": "x"})).is_err());
        // The root's component and an empty occurrence are left out.
        let point = target(json!({"at": [1, 2, 3], "component": "C0", "occurrence": ""}));
        assert_eq!(point.normalized(), target(json!({"at": [1, 2, 3]})));
    }

    #[test]
    fn ids_are_c_and_a_number_from_one() {
        assert_eq!(comment_number("C3"), Some(3));
        assert_eq!(comment_number(" C12 "), Some(12));
        for text in ["C0", "C03", "c3", "3", "C", "C-1", "F3"] {
            assert_eq!(comment_number(text), None, "{text}");
        }
    }
}
