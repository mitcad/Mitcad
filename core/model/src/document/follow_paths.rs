// SPDX-License-Identifier: MIT
//! The stores beside the timeline that name occurrences by their paths and
//! bodies by their ids follow the edits that change those (mitcad#1318):
//! print plate items (mitcad#1050), comment targets (mitcad#1119) and the
//! recorded bodies and picks of drawing views (mitcad#1293). Reparent
//! Occurrence, Move Features into a Component (and Move History into
//! Component) and Save Component as Design with Replace call
//! [`DocState::follow`] in the undo step of their change, beside their own
//! rewrites of links, joint origins, Isolate and contact sets.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::DocState;
use crate::drawing::def::Subject;
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::joints::path_component;
use crate::print_plates::PlateTarget;

/// What changed, for [`DocState::follow`].
pub(crate) enum Follow<'a> {
    /// An occurrence moved to another parent (Reparent Occurrence): a path
    /// from the root that leads through `old` (its path before) leads
    /// through `new` (after). Plate items and comment targets; the
    /// drawings follow in the move's plan, which checks them.
    Moved {
        old: &'a [OccurrenceUid],
        new: &'a [OccurrenceUid],
    },
    /// Bodies of `source` went into a new component placed in it by
    /// `occurrence` (Move Features into a Component, Move History into
    /// Component): a path that placed one of them in `source` goes on
    /// through the occurrence. Comment targets, drawings' recorded bodies
    /// and picks; plate items name bodies by id, and the occurrences that
    /// held them still do.
    Into {
        source: ComponentUid,
        occurrence: OccurrenceUid,
        bodies: &'a [BodyUid],
    },
    /// References renamed (Save Component as Design with Replace): renames
    /// the references in a JSON value in place (body ids, face, edge and
    /// vertex names, occurrence paths as text) and tells whether any
    /// changed. Plate items of bodies, comment targets, drawings' recorded
    /// bodies and picks.
    Renamed(&'a dyn Fn(&mut Value) -> bool),
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
            }
            Follow::Into {
                source,
                occurrence,
                bodies,
            } => {
                let assembly = &self.assembly;
                let ends_in_source = |from: ComponentUid, path: &[OccurrenceUid]| {
                    path_component(assembly, from, path).is_ok_and(|c| c == *source)
                };
                for comment in &mut self.comments {
                    let target = &mut comment.target;
                    if target.body.is_some_and(|b| bodies.contains(&b))
                        && let Some(path) = target.occurrence.as_mut()
                        && ends_in_source(ComponentUid::ROOT, &path.0)
                    {
                        path.0.push(*occurrence);
                    }
                }
                for drawing in &mut self.drawings {
                    let from = drawing.component;
                    for view in drawing.sheets.iter_mut().flat_map(|s| s.views.iter_mut()) {
                        // A body recorded without a path is found wherever
                        // the design places it.
                        if let Some(Subject::Bodies { bodies: shown }) = &mut view.subject {
                            for body in shown.iter_mut() {
                                if bodies.contains(&body.body)
                                    && let Some(path) = body.occurrence.as_mut()
                                    && ends_in_source(from, &path.0)
                                {
                                    path.0.push(*occurrence);
                                }
                            }
                        }
                        let picks = view.annotations.iter_mut().flat_map(|a| a.picks.iter_mut());
                        for pick in picks {
                            if !bodies.contains(&pick.body) {
                                continue;
                            }
                            let mut path = pick.occurrence.clone().unwrap_or_default();
                            if ends_in_source(from, &path.0) {
                                path.0.push(*occurrence);
                                pick.occurrence = Some(path);
                            }
                        }
                        // A body's or an edge's line style (mitcad#1546) as
                        // a pick.
                        for line in &mut view.lines {
                            if !line.body.is_some_and(|b| bodies.contains(&b)) {
                                continue;
                            }
                            let mut path = line.occurrence.clone().unwrap_or_default();
                            if ends_in_source(from, &path.0) {
                                path.0.push(*occurrence);
                                line.occurrence = Some(path);
                            }
                        }
                    }
                }
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
                for drawing in &mut self.drawings {
                    for view in drawing.sheets.iter_mut().flat_map(|s| s.views.iter_mut()) {
                        if let Some(Subject::Bodies { bodies }) = &mut view.subject {
                            for body in bodies.iter_mut() {
                                renamed(body, *rename);
                            }
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
