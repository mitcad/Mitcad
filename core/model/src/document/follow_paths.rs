// SPDX-License-Identifier: MIT
//! The stores beside the timeline that name occurrences by their paths and
//! bodies by their ids follow the edits that change those (mitcad#1318):
//! print plate items (mitcad#1050), comment targets (mitcad#1119), the
//! recorded bodies and picks of drawing views (mitcad#1293), selection
//! set members (mitcad#2219; those of moved features and bodies follow in
//! the moves themselves, `DocState::follow_set_members`), the
//! components point clouds are placed in (mitcad#2265) and decals' faces
//! with their occurrences (mitcad#2244, mitcad#2527). Reparent
//! Occurrence, Move Features into a Component (also with an explicit
//! destination, mitcad#2555, and Move History into Component) and Save
//! Component as Design with Replace call
//! [`DocState::follow`] in the undo step of their change, beside their own
//! rewrites of links, joint origins, Isolate and contact sets.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::DocState;
use super::comments::Comment;
use super::decals::Decal;
use crate::drawing::def::{Drawing, Subject};
use crate::features::OccurrencePath;
use crate::features::geom_ref::GeomRef;
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::joints::path_component;
use crate::print_plates::PlateTarget;

/// What changed, for [`DocState::follow`].
pub(crate) enum Follow<'a> {
    /// An occurrence moved to another parent (Reparent Occurrence): a path
    /// from the root that leads through `old` (its path before) leads
    /// through `new` (after). Plate items, comment targets, selection set
    /// members, point clouds' and decals' occurrences and explode views'
    /// steps (`DocState::follow_explode_moved`); the drawings follow in the
    /// move's plan, which checks them.
    Moved {
        old: &'a [OccurrenceUid],
        new: &'a [OccurrenceUid],
    },
    /// Bodies of `source` went into a new component placed in it by
    /// `occurrence` (Move Features into a Component, Move History into
    /// Component): a path that placed one of them in `source` goes on
    /// through the occurrence. Comment targets, decals on their faces,
    /// drawings' recorded bodies and picks; plate items name bodies by id,
    /// and the occurrences that held them still do.
    Into {
        source: ComponentUid,
        occurrence: OccurrenceUid,
        bodies: &'a [BodyUid],
    },
    /// References renamed (Save Component as Design with Replace): renames
    /// the references in a JSON value in place (body ids, face, edge and
    /// vertex names, occurrence paths as text) and tells whether any
    /// changed. Plate items of bodies, comment targets, decals' faces and
    /// occurrences, drawings' recorded bodies and picks.
    Renamed(&'a dyn Fn(&mut Value) -> bool),
    /// Bodies of a component went to a chosen destination (Move Features
    /// into a Component with an explicit destination, mitcad#2555): an
    /// existing component, or a new one placed under a chosen parent. The
    /// source and the destination each have one placement in the design,
    /// `from` and `to` their paths from the root. A path that placed one of
    /// the bodies in the source places it at the destination: from the
    /// root `to` itself, from a drawing's component the part of `to` below
    /// it. What the destination is not below stays as it was, and the view
    /// then reports the body it no longer finds. The same stores as
    /// [`Follow::Into`].
    Destination {
        bodies: &'a [BodyUid],
        from: &'a [OccurrenceUid],
        to: &'a [OccurrenceUid],
    },
}

impl DocState {
    /// Plate items, comment targets and drawings' recorded bodies and picks
    /// follow `change` (see [`Follow`]). What cannot be read back after a
    /// rename stays as it was: the item, comment or view then reports what
    /// it no longer finds.
    pub(crate) fn follow(&mut self, change: &Follow<'_>) {
        match change {
            Follow::Moved { old, new } => {
                let moved = |path: &mut Vec<OccurrenceUid>| {
                    if path.starts_with(old) {
                        let rest = path.split_off(old.len());
                        path.clear();
                        path.extend_from_slice(new);
                        path.extend(rest);
                    }
                };
                for plate in &mut self.print_plates {
                    for item in &mut plate.items {
                        if let PlateTarget::Occurrence(path) = &mut item.target {
                            moved(path);
                        }
                    }
                }
                for comment in &mut self.comments {
                    if let Some(path) = comment.target.occurrence.as_mut() {
                        moved(&mut path.0);
                    }
                }
                // Selection set members (mitcad#2219).
                self.follow_set_paths(old, new);
                // Point clouds placed in it or below it (mitcad#2265): the
                // placement is in the component, which stays where it is in
                // the design, so the points do too.
                for cloud in &mut self.point_clouds {
                    if let Some(path) = cloud.occurrence.as_mut() {
                        moved(&mut path.0);
                    }
                }
                // Decals on faces of bodies placed through it (mitcad#2527):
                // their paths are from the root, as point clouds' are.
                for decal in &mut self.decals {
                    if let Some(path) = decal.occurrence.as_mut() {
                        moved(&mut path.0);
                    }
                }
                // Explode views' steps, paths from their components
                // (mitcad#2328).
                self.follow_explode_moved(old, new);
            }
            Follow::Into {
                source,
                occurrence,
                bodies,
            } => {
                let assembly = &self.assembly;
                let through = |from: ComponentUid, path: &[OccurrenceUid]| {
                    path_component(assembly, from, path)
                        .is_ok_and(|c| c == *source)
                        .then(|| {
                            let mut path = path.to_vec();
                            path.push(*occurrence);
                            path
                        })
                };
                follow_body_paths(
                    &mut self.comments,
                    &mut self.decals,
                    &mut self.drawings,
                    bodies,
                    &through,
                );
            }
            Follow::Destination { bodies, from, to } => {
                let assembly = &self.assembly;
                // `from` is the source's only path from the root, so a path
                // from `component` to the source is its end, and what is
                // above it is where `component` is placed.
                let placed = |component: ComponentUid,
                              path: &[OccurrenceUid]|
                 -> Option<Vec<OccurrenceUid>> {
                    let above = from.strip_suffix(path)?;
                    if !path_component(assembly, ComponentUid::ROOT, above)
                        .is_ok_and(|c| c == component)
                    {
                        return None;
                    }
                    Some(to.strip_prefix(above)?.to_vec())
                };
                follow_body_paths(
                    &mut self.comments,
                    &mut self.decals,
                    &mut self.drawings,
                    bodies,
                    &placed,
                );
            }
            Follow::Renamed(rename) => {
                for plate in &mut self.print_plates {
                    for item in &mut plate.items {
                        if let PlateTarget::Body(uid) = &mut item.target {
                            let mut value = Value::String(uid.to_string());
                            if rename(&mut value)
                                && let Some(new) = value.as_str().and_then(|t| t.parse().ok())
                            {
                                *uid = new;
                            }
                        }
                    }
                }
                for comment in &mut self.comments {
                    renamed(&mut comment.target, *rename);
                }
                // Selection set members (mitcad#2219).
                for set in &mut self.selection_sets {
                    for member in &mut set.members {
                        renamed(member, *rename);
                    }
                }
                // Decals' faces and occurrences (mitcad#2527), without
                // their names and images.
                for decal in &mut self.decals {
                    let mut place = (decal.face.clone(), decal.occurrence.clone());
                    renamed(&mut place, *rename);
                    (decal.face, decal.occurrence) = place;
                }
                for drawing in &mut self.drawings {
                    for view in drawing.sheets.iter_mut().flat_map(|s| s.views.iter_mut()) {
                        if let Some(Subject::Bodies { bodies }) = &mut view.subject {
                            for body in bodies.iter_mut() {
                                renamed(body, *rename);
                            }
                        }
                        // A Flat Pattern's body and occurrence (mitcad#2280).
                        if let Some(subject @ Subject::FlatPattern { .. }) = &mut view.subject {
                            renamed(subject, *rename);
                        }
                        let picks = view.annotations.iter_mut().flat_map(|a| a.picks.iter_mut());
                        for pick in picks {
                            renamed(pick, *rename);
                        }
                        for line in &mut view.lines {
                            renamed(line, *rename);
                        }
                    }
                }
            }
        }
    }
}

/// A body's new path for its old one, from the root or from the component
/// given (a drawing's); `None` keeps the old one.
type PathMap<'a> = dyn Fn(ComponentUid, &[OccurrenceUid]) -> Option<Vec<OccurrenceUid>> + 'a;

/// Comment targets, decals on faces and drawings' recorded bodies and
/// picks of `bodies` take the paths `map` gives for their old ones (from
/// the root, or from a drawing's component, as the first argument says);
/// `None` leaves a path as it was. A body a drawing records without a path
/// is found wherever the design places it; a decal without one still finds
/// the body's only placement.
fn follow_body_paths(
    comments: &mut [Comment],
    decals: &mut [Decal],
    drawings: &mut [Drawing],
    bodies: &[BodyUid],
    map: &PathMap<'_>,
) {
    for comment in comments {
        let target = &mut comment.target;
        if target.body.is_some_and(|b| bodies.contains(&b))
            && let Some(path) = target.occurrence.as_mut()
            && let Some(new) = map(ComponentUid::ROOT, &path.0)
        {
            path.0 = new;
        }
    }
    // Decals on faces of the moved bodies (mitcad#2527), their paths from
    // the root.
    for decal in decals {
        if let GeomRef::Face { body, .. } = &decal.face
            && bodies.contains(body)
            && let Some(path) = decal.occurrence.as_mut()
            && let Some(new) = map(ComponentUid::ROOT, &path.0)
        {
            path.0 = new;
        }
    }
    for drawing in drawings {
        let from = drawing.component;
        for view in drawing.sheets.iter_mut().flat_map(|s| s.views.iter_mut()) {
            if let Some(Subject::Bodies { bodies: shown }) = &mut view.subject {
                for body in shown.iter_mut() {
                    if bodies.contains(&body.body)
                        && let Some(path) = body.occurrence.as_mut()
                        && let Some(new) = map(from, &path.0)
                    {
                        path.0 = new;
                    }
                }
            }
            // A Flat Pattern's body (mitcad#2280), as a chosen body.
            if let Some(Subject::FlatPattern {
                body,
                occurrence: Some(path),
                ..
            }) = &mut view.subject
                && bodies.contains(body)
                && let Some(new) = map(from, &path.0)
            {
                path.0 = new;
            }
            let picks = view.annotations.iter_mut().flat_map(|a| a.picks.iter_mut());
            for pick in picks {
                if !bodies.contains(&pick.body) {
                    continue;
                }
                let path = pick.occurrence.clone().unwrap_or_default();
                if let Some(new) = map(from, &path.0) {
                    pick.occurrence = Some(OccurrencePath(new));
                }
            }
            // A body's or an edge's line style (mitcad#1546) as a pick.
            for line in &mut view.lines {
                if !line.body.is_some_and(|b| bodies.contains(&b)) {
                    continue;
                }
                let path = line.occurrence.clone().unwrap_or_default();
                if let Some(new) = map(from, &path.0) {
                    line.occurrence = Some(OccurrencePath(new));
                }
            }
        }
    }
}

/// `item` with the references `rename` renames in its JSON form; as it was
/// when it does not read back.
fn renamed<T: Serialize + DeserializeOwned>(item: &mut T, rename: &dyn Fn(&mut Value) -> bool) {
    let Ok(mut value) = serde_json::to_value(&*item) else {
        return;
    };
    if rename(&mut value)
        && let Ok(new) = serde_json::from_value(value)
    {
        *item = new;
    }
}
