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
    /// The Center of Mass marker (mitcad#1094): there or not, and shown
    /// or hidden by its light bulb.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center_of_mass: Option<CenterOfMassMarker>,
    /// What View › Show hides in the whole design (mitcad#1287).
    #[serde(default, skip_serializing_if = "HiddenKinds::is_default")]
    pub hidden: HiddenKinds,
}

/// The kinds of items View › Show hides in the whole design, every
/// component's (mitcad#1287): the view leaves them out whatever their own
/// light bulbs say, which stay as they are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiddenKinds {
    /// The design's origin and the joint origins.
    #[serde(default, skip_serializing_if = "is_false")]
    pub origins: bool,
    /// Construction planes, axes and points.
    #[serde(default, skip_serializing_if = "is_false")]
    pub construction: bool,
    /// Sketches.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sketches: bool,
}

impl HiddenKinds {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The Center of Mass marker of the browser's row under Origin
/// (mitcad#1094): the view draws it at the center of mass of the design's
/// bodies as placed (the `center_of_mass` query) while it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CenterOfMassMarker {
    pub visible: bool,
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
                .map_err(|_| format!("{path} is not an occurrence path such as O1/O4"))
        })
        .collect()
}

/// Whether an isolation ([`Document::isolation`]) shows a body placed at
/// an occurrence path (empty for the root's), as the view decides it
/// (mitcad#1702): the body isolated at that path, or the path in an
/// isolated occurrence. Everything while nothing is isolated.
pub(crate) fn isolation_shows(
    isolation: &[Isolated],
    body: BodyUid,
    path: &[OccurrenceUid],
) -> bool {
    isolation.is_empty()
        || isolation.iter().any(|item| match item.body {
            Some(isolated) => {
                isolated == body
                    && if item.occurrence.is_empty() {
                        path.is_empty()
                    } else {
                        path_uids(&item.occurrence).is_ok_and(|uids| uids == path)
                    }
            }
            None => path_uids(&item.occurrence)
                .is_ok_and(|uids| !uids.is_empty() && path.starts_with(&uids)),
        })
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

    /// Shows or hides the root's Origin folder as the design opens
    /// (mitcad#1683: the application shows a new design's origin): no undo
    /// step and no new revision, so the document stays as modified as it
    /// was, as [`Document::activate_component_on_open`] activates.
    pub fn set_origin_visible_on_open(&mut self, visible: bool) {
        self.state.display.origin = visible;
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

    // The Center of Mass marker (mitcad#1094): display state as the Origin
    // folder is, each change an undo step that recomputes nothing.

    /// Adds the Center of Mass marker, shown (`Add Center of Mass`); a
    /// hidden one is shown (`Show Center of Mass`), a shown one stays as
    /// it is.
    pub fn add_center_of_mass(&mut self) -> Result<(), ModelError> {
        let label = match self.state.display.center_of_mass {
            Some(CenterOfMassMarker { visible: true }) => return Ok(()),
            Some(_) => "Show Center of Mass",
            None => "Add Center of Mass",
        };
        self.apply_with(|state| {
            state.display.center_of_mass = Some(CenterOfMassMarker { visible: true });
            Ok((label.to_owned(), (), false))
        })
    }

    /// Shows or hides the Center of Mass marker (`Show Center of Mass`,
    /// `Hide Center of Mass`) unless nothing changes; refused while there
    /// is none.
    pub fn set_center_of_mass_visible(&mut self, visible: bool) -> Result<(), ModelError> {
        let Some(marker) = self.state.display.center_of_mass else {
            return Err(invalid("the design has no Center of Mass marker"));
        };
        if marker.visible == visible {
            return Ok(());
        }
        let label = if visible {
            "Show Center of Mass"
        } else {
            "Hide Center of Mass"
        };
        self.apply_with(|state| {
            state.display.center_of_mass = Some(CenterOfMassMarker { visible });
            Ok((label.to_owned(), (), false))
        })
    }

    /// Deletes the Center of Mass marker (`Delete Center of Mass`);
    /// refused while there is none.
    pub fn delete_center_of_mass(&mut self) -> Result<(), ModelError> {
        if self.state.display.center_of_mass.is_none() {
            return Err(invalid("the design has no Center of Mass marker"));
        }
        self.apply_with(|state| {
            state.display.center_of_mass = None;
            Ok(("Delete Center of Mass".to_owned(), (), false))
        })
    }

    // View › Show (mitcad#1287): display state as the Origin folder is,
    // each change an undo step that recomputes nothing.

    /// Shows or hides the origins, the construction geometry or the
    /// sketches of the whole design (`Some(true)` shows, `None` leaves the
    /// kind as it is): one undo step (`Hide All Construction`) unless
    /// nothing changes. No feature's own light bulb changes.
    pub fn set_view_show(
        &mut self,
        origins: Option<bool>,
        construction: Option<bool>,
        sketches: Option<bool>,
    ) -> Result<(), ModelError> {
        if origins.is_none() && construction.is_none() && sketches.is_none() {
            return Err(invalid(
                "set_view_show needs origins, construction or sketches",
            ));
        }
        let before = self.state.display.hidden;
        let mut after = before;
        let mut parts = Vec::new();
        for (shown, hidden, name) in [
            (origins, &mut after.origins, "Origins"),
            (construction, &mut after.construction, "Construction"),
            (sketches, &mut after.sketches, "Sketches"),
        ] {
            if let Some(shown) = shown
                && *hidden == shown
            {
                *hidden = !shown;
                parts.push(format!(
                    "{} All {name}",
                    if shown { "Show" } else { "Hide" }
                ));
            }
        }
        if after == before {
            return Ok(());
        }
        let label = parts.join(", ");
        self.apply_with(|state| {
            state.display.hidden = after;
            Ok((label, (), false))
        })
    }
}
