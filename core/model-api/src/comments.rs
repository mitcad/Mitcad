// SPDX-License-Identifier: MIT
//! Comments (mitcad#1119, `document/comments.rs`): the commands that add,
//! edit, reply to, retarget, address, resolve, reopen and delete them, the
//! `comments` query, and how a comment's target is found at the timeline
//! marker: what it is called now (`label`) and where its pin goes
//! (`place`, design coordinates), or why it is gone (the comment is then
//! orphaned at its last known place). After every command the targets are
//! settled ([`CommentsOps::settle_comments_now`], called by
//! `DocumentApi::command`). Comments and replies carry images
//! (mitcad#2011): `add_comment` and `reply_comment` take `attachments`,
//! `remove_comment_attachment` removes one, the `comments` query lists
//! their fields and `comment_attachment` gives one's bytes. See
//! `commands.md`, "Comments".

use std::sync::{Mutex, PoisonError};

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::datum::{CurveGeometry, PathParameter, Vec3};
use mitcad_model::features::{FeatureDef, SketchDef};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::sketch::Ref;
use mitcad_model::topo::{EdgeName, FaceName};
use mitcad_model::{
    AttachmentSource, Comment, CommentAttachment, CommentFound, CommentSketchItem, CommentStatus,
    CommentTarget, CommentTargetKind, Document, attachment_number,
};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    #[allow(clippy::enum_variant_names)]
    pub(super) enum CommentCommand {
        "add_comment" => AddComment {
            text: String,
            // (Boxed: a target is large beside the other commands' fields.)
            target: Box<CommentTarget>,
            /// `user` when left out; the MCP layer sets its client's name.
            #[serde(default)]
            author: Option<String>,
            /// Images to attach (mitcad#2011).
            #[serde(default)]
            attachments: Vec<AttachmentIn>,
        },
        "edit_comment" => EditComment {
            id: String,
            text: String,
        },
        "reply_comment" => ReplyComment {
            id: String,
            text: String,
            #[serde(default)]
            author: Option<String>,
            /// Images to attach (mitcad#2011).
            #[serde(default)]
            attachments: Vec<AttachmentIn>,
        },
        "retarget_comment" => RetargetComment {
            id: String,
            target: Box<CommentTarget>,
        },
        "address_comment" => AddressComment {
            id: String,
            resolution: String,
            #[serde(default)]
            author: Option<String>,
        },
        "resolve_comment" => ResolveComment {
            id: String,
        },
        "reopen_comment" => ReopenComment {
            id: String,
            /// A reply saying why, when given.
            #[serde(default)]
            text: Option<String>,
            #[serde(default)]
            author: Option<String>,
        },
        "delete_comment" => DeleteComment {
            id: String,
        },
        // An image of a comment or of one of its replies (mitcad#2011).
        "remove_comment_attachment" => RemoveCommentAttachment {
            comment: String,
            id: String,
        },
    }
}

/// An image a command attaches (mitcad#2011): the file's bytes in base64
/// and its name.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttachmentIn {
    data: String,
    #[serde(default)]
    name: String,
}

/// The images of a command as the model takes them, or why one is no
/// base64.
fn sources(attachments: Vec<AttachmentIn>) -> Result<Vec<AttachmentSource>, ApiError> {
    let several = attachments.len() > 1;
    attachments
        .into_iter()
        .enumerate()
        .map(|(i, attachment)| {
            let bytes =
                mitcad_model::internal::base64::decode(attachment.data.trim()).map_err(|e| {
                    let what = if several {
                        format!("image {}", i + 1)
                    } else {
                        "the image".to_owned()
                    };
                    ApiError(format!("{what}'s data is no base64: {e}"))
                })?;
            Ok(AttachmentSource {
                bytes,
                name: attachment.name,
            })
        })
        .collect()
}

/// The `comment_attachment` query's fields (mitcad#2011).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttachmentQuery {
    comment: String,
    id: String,
}

/// The `comments` query's fields.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommentsQuery {
    /// Only comments of this status, or of these.
    #[serde(default)]
    status: Option<Statuses>,
    /// Only comments on this target (the same target, as stored).
    #[serde(default)]
    target: Option<CommentTarget>,
}

/// One status or several.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Statuses {
    One(CommentStatus),
    Several(Vec<CommentStatus>),
}

impl Statuses {
    fn has(&self, status: CommentStatus) -> bool {
        match self {
            Self::One(one) => *one == status,
            Self::Several(several) => several.contains(&status),
        }
    }
}

/// A target found at the marker: what it is called now and where its pin
/// goes (design coordinates; none for a parameter or where the kernel
/// cannot say).
struct Found {
    label: String,
    place: Option<Vec3>,
}

/// The `revision` of the `comments` query (mitcad#1119): a number that
/// grows whenever the comments as listed change (an edit, undo, redo, a
/// target found elsewhere or gone), so that a client can tell at a glance
/// whether to read them again. Kept beside the document: the comments last
/// listed and their number.
#[derive(Default)]
struct CommentRevision {
    seen: Mutex<(String, u64)>,
}

impl CommentRevision {
    fn of(&self, listed: &[Value]) -> u64 {
        let text = serde_json::to_string(listed).unwrap_or_default();
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        if seen.0 != text {
            seen.0 = text;
            seen.1 += 1;
        }
        seen.1
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(super) trait CommentsOps<K: Kernel> {
    fn run_comment_command(&mut self, command: CommentCommand) -> Result<Value, ApiError>;

    /// The `comments` query: `{"comments": [...], "open": n, "revision": r}`.
    fn comments_json(&self, query: &CommentsQuery) -> Result<Value, ApiError>;

    /// The `comment_attachment` query (mitcad#2011): an image's fields and
    /// its bytes in base64 (`data`).
    fn comment_attachment_json(&self, query: &AttachmentQuery) -> Result<Value, ApiError>;

    /// Finds every comment's target at the marker and keeps what was found
    /// ([`Document::settle_comments`]): the place of one found, `orphaned`
    /// for one gone. Run after every command; nothing without comments.
    fn settle_comments_now(&mut self);
}

impl<K: Kernel> CommentsOps<K> for Document<K> {
    fn run_comment_command(&mut self, command: CommentCommand) -> Result<Value, ApiError> {
        Ok(match command {
            CommentCommand::AddComment {
                text,
                target,
                author,
                attachments,
            } => {
                let found = target_now(self, &target)
                    .map_err(|e| ApiError(format!("the comment's target: {e}")))?;
                let attachments = sources(attachments)?;
                let first = self.next_attachment_id();
                let id =
                    self.add_comment(&text, *target, author.as_deref(), found.place, &attachments)?;
                added(json!({"id": id}), &first, attachments.len())
            }
            CommentCommand::EditComment { id, text } => {
                let changed = self.edit_comment(&id, &text)?;
                json!({"changed": changed})
            }
            CommentCommand::ReplyComment {
                id,
                text,
                author,
                attachments,
            } => {
                let attachments = sources(attachments)?;
                let first = self.next_attachment_id();
                self.reply_comment(&id, &text, author.as_deref(), &attachments)?;
                let reply = self.comment(&id)?.replies.len();
                added(json!({"reply": reply}), &first, attachments.len())
            }
            CommentCommand::RetargetComment { id, target } => {
                // The comment first: an unknown id is refused as such.
                self.comment(&id)?;
                let found = target_now(self, &target)
                    .map_err(|e| ApiError(format!("the comment's target: {e}")))?;
                let changed = self.retarget_comment(&id, *target, found.place)?;
                json!({"changed": changed})
            }
            CommentCommand::AddressComment {
                id,
                resolution,
                author,
            } => {
                self.address_comment(&id, &resolution, author.as_deref())?;
                json!({})
            }
            CommentCommand::ResolveComment { id } => {
                self.resolve_comment(&id)?;
                json!({})
            }
            CommentCommand::ReopenComment { id, text, author } => {
                self.reopen_comment(&id, text.as_deref(), author.as_deref())?;
                json!({})
            }
            CommentCommand::DeleteComment { id } => {
                self.delete_comment(&id)?;
                json!({})
            }
            CommentCommand::RemoveCommentAttachment { comment, id } => {
                self.remove_comment_attachment(&comment, &id)?;
                json!({})
            }
        })
    }

    fn comment_attachment_json(&self, query: &AttachmentQuery) -> Result<Value, ApiError> {
        let comment = self.comment(&query.comment)?;
        let number = attachment_number(&query.id)
            .ok_or_else(|| ApiError(format!("{} is no image id (A<n>)", query.id)))?;
        let attachment = comment.attachment(number).ok_or_else(|| {
            ApiError(format!(
                "{} has no image {}",
                comment.name(),
                query.id.trim()
            ))
        })?;
        let mut value = attachment_json(attachment);
        value["comment"] = json!(comment.name());
        value["data"] = json!(attachment.image.data.base64());
        Ok(value)
    }

    fn comments_json(&self, query: &CommentsQuery) -> Result<Value, ApiError> {
        let wanted = match &query.target {
            Some(target) => {
                target
                    .kind()
                    .map_err(|e| ApiError(format!("target: {e}")))?;
                Some(target.clone().normalized())
            }
            None => None,
        };
        let comments = self.comments();
        let all: Vec<Value> = comments.iter().map(|c| comment_json(self, c)).collect();
        let revision = self.side_cache::<CommentRevision>().of(&all);
        let open = comments
            .iter()
            .filter(|c| c.status == CommentStatus::Open)
            .count();
        let listed: Vec<Value> = comments
            .iter()
            .zip(all)
            .filter(|(c, _)| query.status.as_ref().is_none_or(|s| s.has(c.status)))
            .filter(|(c, _)| wanted.as_ref().is_none_or(|t| *t == c.target))
            .map(|(_, value)| value)
            .collect();
        Ok(json!({"comments": listed, "open": open, "revision": revision}))
    }

    fn settle_comments_now(&mut self) {
        if self.comments().is_empty() {
            return;
        }
        let found: Vec<(u32, CommentFound)> = self
            .comments()
            .iter()
            .map(|c| {
                let now = match target_now(self, &c.target) {
                    Ok(found) => CommentFound::At(found.place),
                    Err(_) => CommentFound::Gone,
                };
                (c.id, now)
            })
            .collect();
        self.settle_comments(&found);
    }
}

/// The answer of a command that attached images: `attachments`, their
/// ids from `first` (the next id before the command), when there are any.
fn added(mut value: Value, first: &str, count: usize) -> Value {
    if count > 0 {
        let first = attachment_number(first).unwrap_or(1);
        let ids: Vec<String> = (0..count)
            .map(|i| format!("A{}", u64::from(first) + i as u64))
            .collect();
        value["attachments"] = json!(ids);
    }
    value
}

/// An image's fields as the `comments` query lists it (mitcad#2011).
fn attachment_json(attachment: &CommentAttachment) -> Value {
    json!({
        "id": attachment.name_id(),
        "media_type": attachment.media_type(),
        "width": attachment.image.width,
        "height": attachment.image.height,
        "bytes": attachment.bytes(),
        "sha256": attachment.sha256(),
        "name": attachment.name,
    })
}

/// A comment as the `comments` query lists it: its stored fields, and
/// `label`, `place` and `orphaned` as its target is found now (an orphan's
/// `place` its last known one, with `error`, why it is gone). The images of
/// the comment and of each reply as `attachments`, where it has any.
fn comment_json<K: Kernel>(doc: &Document<K>, comment: &Comment) -> Value {
    let found = target_now(doc, &comment.target);
    let replies: Vec<Value> = comment
        .replies
        .iter()
        .map(|reply| {
            let mut value = json!({
                "author": reply.author,
                "text": reply.text,
                "created": reply.created,
            });
            if !reply.attachments.is_empty() {
                value["attachments"] = reply.attachments.iter().map(attachment_json).collect();
            }
            value
        })
        .collect();
    let mut value = json!({
        "id": comment.name(),
        "text": comment.text,
        "target": comment.target,
        "status": comment.status,
        "orphaned": found.is_err(),
        "author": comment.author,
        "resolution": comment.resolution,
        "replies": replies,
        "created": comment.created,
    });
    if !comment.attachments.is_empty() {
        value["attachments"] = comment.attachments.iter().map(attachment_json).collect();
    }
    match found {
        Ok(found) => {
            value["label"] = json!(found.label);
            value["place"] = json!(found.place.or(comment.place));
        }
        Err(why) => {
            value["label"] = json!(gone_label(doc, &comment.target));
            value["place"] = json!(comment.place);
            value["error"] = json!(why);
        }
    }
    if let Some(by) = &comment.addressed_by {
        value["addressed_by"] = json!(by);
    }
    value
}

/// A target found at the marker, or why it is not there.
fn target_now<K: Kernel>(doc: &Document<K>, target: &CommentTarget) -> Result<Found, String> {
    match target.kind()? {
        CommentTargetKind::Feature(uid) => {
            let entry = doc
                .feature(uid)
                .ok_or_else(|| format!("the feature {uid} is gone"))?;
            // Its first body's centre, while it has one.
            let place = body_centre(doc, BodyUid::new(uid, 0), None);
            Ok(Found {
                label: entry.name.clone(),
                place,
            })
        }
        CommentTargetKind::Body(body)
        | CommentTargetKind::Face(body, _)
        | CommentTargetKind::Edge(body, _)
        | CommentTargetKind::Vertex(body, _) => on_body(doc, target, body),
        CommentTargetKind::SketchItem { sketch, item } => sketch_item(doc, sketch, item),
        CommentTargetKind::Parameter(name) => {
            doc.parameters()
                .find(name)
                .ok_or_else(|| format!("the parameter {name} is gone"))?;
            Ok(Found {
                label: name.to_owned(),
                place: None,
            })
        }
        CommentTargetKind::Point(at) => {
            let component = target.component.unwrap_or(ComponentUid::ROOT);
            if !doc.assembly().exists(component) {
                return Err(format!("the component {component} is gone"));
            }
            let transform = doc
                .component_transform(Some(component), target.occurrence.as_ref(), "the point")
                .map_err(|e| e.0)?;
            Ok(Found {
                label: "Point".to_owned(),
                place: Some(transform.apply_point(at)),
            })
        }
        // The whole design (mitcad#2010): always there, without a place.
        CommentTargetKind::Design => Ok(Found {
            label: "Design".to_owned(),
            place: None,
        }),
    }
}

/// A body, or a face, edge or vertex of it, found at the marker: the body
/// there, its occurrence placing it, and the name there (a face or its
/// pieces after splits, an edge between matching faces, a vertex the
/// kernel finds).
fn on_body<K: Kernel>(
    doc: &Document<K>,
    target: &CommentTarget,
    body: BodyUid,
) -> Result<Found, String> {
    let local = doc
        .body_shape(body)
        .ok_or_else(|| format!("the body {body} is gone"))?;
    let transform = doc
        .component_transform(
            doc.body_component(body),
            target.occurrence.as_ref(),
            &format!("body {body}"),
        )
        .map_err(|e| e.0)?;
    let kernel = doc.kernel();
    let name = doc.body_name(body);
    let centre = || box_centre(kernel.bounding_box(local).ok().flatten());
    let (label, place) = match target.kind()? {
        CommentTargetKind::Face(_, face) => {
            // A kernel that cannot look faces up finds them all.
            if kernel.count_faces(local, face).is_ok_and(|n| n == 0) {
                return Err(format!("the face {face} is gone from {name}"));
            }
            let label = format!("{name} › {} face", face_words(doc, face));
            (label, face_middle(kernel, local, face).or_else(centre))
        }
        CommentTargetKind::Edge(_, edge) => {
            if kernel.count_edges(local, edge) == 0 {
                return Err(format!("the edge {edge} is gone from {name}"));
            }
            (
                format!("{name} › edge"),
                edge_middle(kernel, local, edge).or_else(centre),
            )
        }
        CommentTargetKind::Vertex(_, vertex) => {
            let point = kernel
                .vertex_point(local, vertex)
                .map_err(|_| format!("the vertex {vertex} is gone from {name}"))?;
            (format!("{name} › vertex"), Some(point))
        }
        _ => (name.clone(), centre()),
    };
    Ok(Found {
        label,
        place: place.map(|p| transform.apply_point(p)),
    })
}

/// A face's feature and role in words: `Extrude1 end`.
fn face_words<K: Kernel>(doc: &Document<K>, face: &FaceName) -> String {
    let feature = doc
        .feature(face.feature)
        .map_or_else(|| face.feature.to_string(), |entry| entry.name.clone());
    format!("{feature} {}", face.role)
}

/// A body's centre (its box's) in the design, placed by `occurrence` (its
/// only placement when none), if the body is there.
fn body_centre<K: Kernel>(
    doc: &Document<K>,
    body: BodyUid,
    occurrence: Option<&mitcad_model::features::OccurrencePath>,
) -> Option<Vec3> {
    let local = doc.body_shape(body)?;
    let centre = box_centre(doc.kernel().bounding_box(local).ok().flatten())?;
    let transform = doc
        .component_transform(doc.body_component(body), occurrence, "the body")
        .ok()?;
    Some(transform.apply_point(centre))
}

fn box_centre(found: Option<BoundingBox>) -> Option<Vec3> {
    let found = found?;
    let centre: Vec3 = std::array::from_fn(|i| (found.min[i] + found.max[i]) / 2.0);
    centre.iter().all(|c| c.is_finite()).then_some(centre)
}

/// The middle of a face in its body's coordinates: a planar face's on its
/// plane (as `measure` gives it), else the middle of its box (its pieces'
/// after splits).
fn face_middle<K: Kernel>(kernel: &K, shape: &K::Shape, face: &FaceName) -> Option<Vec3> {
    if let Ok(plane) = kernel.face_plane(shape, face) {
        return Some(plane.origin);
    }
    let indices: Vec<usize> = kernel
        .faces(shape)
        .ok()?
        .iter()
        .enumerate()
        .filter(|(_, info)| {
            info.names
                .iter()
                .any(|name| name.parse::<FaceName>().is_ok_and(|n| n.matches(face)))
        })
        .map(|(i, _)| i)
        .collect();
    if indices.is_empty() {
        return None;
    }
    let mut union: Option<BoundingBox> = None;
    for found in kernel
        .face_bounding_boxes(shape, &indices)
        .ok()?
        .into_iter()
        .flatten()
    {
        union = Some(match union {
            None => found,
            Some(u) => BoundingBox {
                min: std::array::from_fn(|i| u.min[i].min(found.min[i])),
                max: std::array::from_fn(|i| u.max[i].max(found.max[i])),
            },
        });
    }
    box_centre(union)
}

/// The middle of an edge (half its length along it), else of a straight
/// one's ends, in its body's coordinates.
fn edge_middle<K: Kernel>(kernel: &K, shape: &K::Shape, edge: &EdgeName) -> Option<Vec3> {
    if let Ok(found) = kernel.path_point(
        shape,
        std::slice::from_ref(edge),
        PathParameter::Fraction(0.5),
    ) {
        return Some(found.point);
    }
    match kernel.edge_geometry(shape, edge).ok()? {
        CurveGeometry::Line { start, end } => {
            Some(std::array::from_fn(|i| (start[i] + end[i]) / 2.0))
        }
        CurveGeometry::Circle { center, .. } => Some(center),
        _ => None,
    }
}

/// A sketch's curve, point, constraint or dimension found in its
/// definition; its place the middle of the points it uses as the sketch
/// last solved, in the design.
fn sketch_item<K: Kernel>(
    doc: &Document<K>,
    sketch: FeatureUid,
    item: CommentSketchItem,
) -> Result<Found, String> {
    let entry = doc
        .feature(sketch)
        .ok_or_else(|| format!("the sketch {sketch} is gone"))?;
    let FeatureDef::Sketch(def) = &entry.def else {
        return Err(format!("{} is no sketch", entry.name));
    };
    let gone = |what: String| format!("{what} is gone from {}", entry.name);
    let (words, refs) = match item {
        CommentSketchItem::Entity(found) => {
            let entity = def
                .entities
                .iter()
                .find(|e| e.id == found.uid() && e.is_point() == found.is_point())
                .ok_or_else(|| gone(found.to_string()))?;
            (format!("{} {found}", entity.kind.type_name()), vec![found])
        }
        CommentSketchItem::Constraint(k) => {
            let constraint = def
                .constraints
                .iter()
                .find(|c| c.id == k)
                .ok_or_else(|| gone(format!("the constraint {k}")))?;
            (
                format!("{} {k}", constraint.kind.type_name()),
                constraint.kind.refs(),
            )
        }
        CommentSketchItem::Dimension(k) => {
            let dimension = def
                .dimensions
                .iter()
                .find(|d| d.id == k)
                .ok_or_else(|| gone(format!("the dimension {k}")))?;
            (format!("dimension {k}"), dimension.kind.refs())
        }
    };
    let place = sketch_place(doc, sketch, def, &refs).and_then(|local| {
        doc.component_transform(Some(entry.component), None, &entry.name)
            .ok()
            .map(|t| t.apply_point(local))
    });
    Ok(Found {
        label: format!("{} › {words}", entry.name),
        place,
    })
}

/// The middle of the points these sketch entities use as the sketch last
/// solved, in its component's coordinates.
fn sketch_place<K: Kernel>(
    doc: &Document<K>,
    sketch: FeatureUid,
    def: &SketchDef,
    refs: &[Ref],
) -> Option<Vec3> {
    let output = doc.sketch_output(sketch)?;
    let solved = &output.solved;
    let mut points: Vec<[f64; 2]> = Vec::new();
    for found in refs {
        match found {
            Ref::Point(uid) => points.extend(solved.points.get(uid).copied()),
            Ref::Curve(uid) => {
                let entity = def.entities.iter().find(|e| e.id == *uid)?;
                points.extend(
                    entity
                        .kind
                        .points()
                        .iter()
                        .filter_map(|p| solved.points.get(p).copied()),
                );
            }
        }
    }
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    let u = points.iter().map(|p| p[0]).sum::<f64>() / n;
    let v = points.iter().map(|p| p[1]).sum::<f64>() / n;
    Some(output.frame.point([u, v]))
}

/// What an orphan's target was, in words, from what is left of it: names
/// where they are still there, else ids.
fn gone_label<K: Kernel>(doc: &Document<K>, target: &CommentTarget) -> String {
    let feature = |uid: FeatureUid| {
        doc.feature(uid)
            .map_or_else(|| uid.to_string(), |entry| entry.name.clone())
    };
    let body = |uid: BodyUid| {
        if doc.body_shape(uid).is_some() {
            doc.body_name(uid)
        } else {
            uid.to_string()
        }
    };
    match target.kind() {
        Ok(CommentTargetKind::Feature(uid)) => feature(uid),
        Ok(CommentTargetKind::Body(uid)) => body(uid),
        Ok(CommentTargetKind::Face(uid, face)) => {
            format!("{} › {} face", body(uid), face_words(doc, face))
        }
        Ok(CommentTargetKind::Edge(uid, _)) => format!("{} › edge", body(uid)),
        Ok(CommentTargetKind::Vertex(uid, _)) => format!("{} › vertex", body(uid)),
        Ok(CommentTargetKind::SketchItem { sketch, item }) => {
            let item = match item {
                CommentSketchItem::Entity(found) => found.to_string(),
                CommentSketchItem::Constraint(k) | CommentSketchItem::Dimension(k) => k.to_string(),
            };
            format!("{} › {item}", feature(sketch))
        }
        Ok(CommentTargetKind::Parameter(name)) => name.to_owned(),
        Ok(CommentTargetKind::Point(_)) => "Point".to_owned(),
        Ok(CommentTargetKind::Design) => "Design".to_owned(),
        Err(_) => "?".to_owned(),
    }
}
