// SPDX-License-Identifier: MIT
//! What the browser shows that the document keeps (P9): the root's Origin
//! folder shown or not, the bodies and components the view is isolated to,
//! and the favourite parameters. Each change is an undo step that
//! recomputes nothing, as the light bulbs are (`browser.rs`).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Document, ModelError, invalid};
use crate::features::is_false;
use crate::ids::{BodyUid, OccurrenceUid};
use crate::kernel::Kernel;

/// A body or a component occurrence the view is isolated to, as the
/// browser names it: a body (in the occurrence that places it, empty for
/// the root's), or an occurrence by its path from the root (`O1/O4`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Isolated {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyUid>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub occurrence: String,
}

/// The document's display state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayState {
    /// The root's origin planes, axes and point are shown.
    #[serde(default, skip_serializing_if = "is_false")]
    pub origin: bool,
    /// Only these are shown (Isolate); none: everything.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub isolated: Vec<Isolated>,
}

impl DisplayState {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The occurrence uids of a path (`O1/O4`).
fn path_uids(path: &str) -> Result<Vec<OccurrenceUid>, String> {
    path.split('/')
        .map(|part| {
            part.parse::<OccurrenceUid>()
                .map_err(|_| format!("'{path}' is not an occurrence path such as O1/O4"))
        })
        .collect()
}

impl<K: Kernel> Document<K> {
    pub fn display(&self) -> &DisplayState {
        &self.state.display
    }

    /// The isolated items that are there at the marker: bodies that exist
    /// and occurrences whose path does. When none of them is, nothing is
    /// isolated (a deleted body leaves the view whole).
    pub fn isolation(&self) -> Vec<Isolated> {
        if self.state.display.isolated.is_empty() {
            return Vec::new(); // the usual case, asked on every refresh
        }
        let bodies: BTreeSet<BodyUid> = self.bodies().iter().map(|b| b.uid).collect();
        let assembly = self.assembly();
        let placed = |path: &str| {
            path.is_empty()
                || path_uids(path)
                    .is_ok_and(|uids| uids.iter().all(|uid| assembly.occurrence(*uid).is_some()))
        };
        self.state
            .display
            .isolated
            .iter()
            .filter(|item| {
                placed(&item.occurrence) && item.body.is_none_or(|b| bodies.contains(&b))
            })
            .cloned()
            .collect()
    }

    /// Shows or hides the root's Origin folder: an undo step (`Show
    /// Origin`) unless nothing changes.
    pub fn set_origin_visible(&mut self, visible: bool) -> Result<(), ModelError> {
        if self.state.display.origin == visible {
            return Ok(());
        }
        let label = if visible {
            "Show Origin"
        } else {
            "Hide Origin"
        };
        self.apply_with(|state| {
            state.display.origin = visible;
            Ok((label.to_owned(), (), false))
        })
    }

    /// Isolates the view to bodies and occurrences (none: Unisolate): an
    /// undo step unless nothing changes.
    pub fn set_isolation(&mut self, items: Vec<Isolated>) -> Result<(), ModelError> {
        for item in &items {
            if item.body.is_none() && item.occurrence.is_empty() {
                return Err(invalid("isolate a body or an occurrence"));
            }
            if !item.occurrence.is_empty() {
                for uid in path_uids(&item.occurrence).map_err(invalid)? {
                    if self.assembly().occurrence(uid).is_none() {
                        return Err(invalid(format!("occurrence {uid} does not exist")));
                    }
                }
            }
        }
        let mut items = items;
        items.sort();
        items.dedup();
        if self.state.display.isolated == items {
            return Ok(());
        }
        let label = if items.is_empty() {
            "Unisolate".to_owned()
        } else {
            format!("Isolate {} item(s)", items.len())
        };
        self.apply_with(|state| {
            state.display.isolated = items;
            Ok((label, (), false))
        })
    }

    /// Marks a parameter a favourite or not (Change Parameters' star): an
    /// undo step unless nothing changes.
    pub fn set_parameter_favorite(&mut self, name: &str, favorite: bool) -> Result<(), ModelError> {
        let id = self.param_id(name)?;
        if self.state.favorites.contains(&id) == favorite {
            return Ok(());
        }
        let verb = if favorite { "Add" } else { "Remove" };
        self.apply_with(|state| {
            if favorite {
                state.favorites.insert(id);
            } else {
                state.favorites.remove(&id);
            }
            Ok((format!("{verb} Favorite {name}"), (), false))
        })
    }

    /// Whether a parameter is a favourite.
    pub fn is_favorite_parameter(&self, name: &str) -> bool {
        self.parameters()
            .find(name)
            .is_some_and(|id| self.state.favorites.contains(&id))
    }
}
