// SPDX-License-Identifier: MIT
//! The materials and appearances of a linked design's bodies (mitcad#1921).
//!
//! A component inserted linked, from a library or a file, holds the
//! design's shown bodies in a base feature. Each of those bodies is made
//! of the material its source design gives it (the body's own, else the
//! nearest component's default up its path there, else the source's
//! root's) and shows the appearance it has there (its own, else the
//! appearance of the component whose default material it takes):
//!
//! - a built-in material, or a library appearance, by its id;
//! - a material of the source design's own (one made there, or brought
//!   from a material library with its `reference`) is copied into the
//!   design once, and found again by its name and density (a numbered
//!   copy's name, `Oak (2)`, counts) or by its reference for every further
//!   body that names it, as a library part's material is (mitcad#1817);
//! - an appearance of the source design's own is copied likewise, its
//!   images read from where the source has them, and found again when an
//!   equal one is there.
//!
//! The base body records what its source gave it (`BaseBody::material`,
//! `BaseBody::appearance`). When the link is read again (`update_links`,
//! `update_link`, another version or row of a library part), a body whose
//! material is still the one its source gave takes the source's new one;
//! a material set on the inserted body by hand, or by a material binding,
//! stays. Appearances follow the same rule. A body whose source gives no
//! material gets none of its own: it is made of the design's default
//! material where the design names one (`MaterialSource::Component`), and
//! of no material otherwise (mitcad#1817: never a silent steel).
//!
//! A copy of a design (`"link": false`) brings the materials with its
//! bodies and components as every copy does (`copy.rs`).
//!
//! A component linked with options (mitcad#801) is made by a derived part
//! whose bodies the source design makes at evaluation time (mitcad#1923).
//! Its bodies take what the source in the part's row and values gives
//! them, by the same rules, when it is inserted, when its source is read
//! again (another version or row of a library part, a linked file that
//! changed) and when its row changes. To know which bodies the source
//! shows, its design is evaluated once more then, unless it gives no body
//! a material or an appearance at all. What it gave each body is recorded
//! on the component's link (`ExternalLink::given`), as a base feature's
//! bodies record it themselves.
//!
//! A design saved before mitcad#1921 has links whose bodies never took
//! their materials (`ExternalLink::looks` false): its next `update_links`
//! gives them once, from the file read again or the source the design
//! keeps, even when nothing changed.

use std::path::PathBuf;
use std::sync::Arc;

use super::copy::CopySource;
use super::{DocState, Document, ModelError};
use crate::appearance::{self, Appearance, library_appearance};
use crate::assembly::{Instance, LinkLook};
use crate::features::{BaseBody, FeatureDef};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::library::LibraryRequest;
use crate::material::{self, Material};

/// Densities this close (g/cm³) are the same.
const SAME_DENSITY: f64 = 1e-9;

/// What a linked design gives one of its shown bodies (mitcad#1921): the
/// material it is made of there, that material's own appearance when it
/// is the source design's, and the appearance the body shows there, as
/// definitions to bring into the design that links it.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceLook {
    pub material: Option<Material>,
    /// The material's default appearance in the source design.
    pub material_look: Option<Appearance>,
    /// The body's own appearance, or that of the component whose default
    /// material it takes.
    pub appearance: Option<Appearance>,
}

/// What `source` gives the body `instance` places (see the module).
/// `origin` reads the images of the source design's own appearances.
pub(super) fn source_look(
    source: &DocState,
    instance: &Instance,
    origin: &CopySource<'_>,
) -> SourceLook {
    let own = source.body_attributes.get(&instance.body);
    let own_material = own
        .and_then(|a| a.material.as_deref())
        .and_then(|id| source.material(id));
    // The nearest component's default up the body's path, then the root's.
    let inherited = || {
        instance
            .path
            .iter()
            .rev()
            .filter_map(|o| source.assembly.occurrence(*o).map(|o| o.component))
            .chain(std::iter::once(ComponentUid::ROOT))
            .find_map(|c| {
                let m = source
                    .component_materials
                    .get(&c)
                    .and_then(|id| source.material(id))?;
                Some((c, m))
            })
    };
    let (made_of, from) = match own_material {
        Some(m) => (Some(m), None),
        None => match inherited() {
            Some((c, m)) => (Some(m), Some(c)),
            None => (None, None),
        },
    };
    let look = own.and_then(|a| a.appearance.as_deref()).or_else(|| {
        from.and_then(|c| source.component_appearances.get(&c))
            .map(String::as_str)
    });
    SourceLook {
        material: made_of.cloned(),
        material_look: made_of
            .and_then(|m| m.appearance.as_deref())
            .and_then(|id| design_look(source, id, origin)),
        appearance: look.and_then(|id| design_look(source, id, origin)),
    }
}

/// An appearance of the source design as a definition to bring along: a
/// library's as it is, the design's own with its images read from where
/// the source has them. None when the source does not know it or an image
/// of it cannot be read (the body then shows its material's look).
fn design_look(source: &DocState, id: &str, origin: &CopySource<'_>) -> Option<Appearance> {
    if let Some(found) = library_appearance(id) {
        return Some(found.clone());
    }
    let mut own = source.appearances.iter().find(|a| a.id == id)?.clone();
    if let Some(texture) = own.texture.as_mut() {
        origin.texture(texture).ok()?;
    }
    Some(own)
}

/// Whether `name` is `base`, or a numbered copy's name of it
/// (`Oak (2)`, as [`material::numbered_name`] and appearance copies make
/// them).
pub(super) fn numbered_from(name: &str, base: &str) -> bool {
    let base = base.trim();
    name == base
        || name
            .strip_prefix(base)
            .and_then(|rest| rest.strip_prefix(" ("))
            .and_then(|rest| rest.strip_suffix(')'))
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

impl DocState {
    /// The id of this design's appearance for one a linked design gives: a
    /// library's as it is, else one of the design's equal to it but for
    /// its id and a numbered name (brought before), else a copy with a
    /// free id and name.
    fn take_look(&mut self, look: &Appearance) -> String {
        if library_appearance(&look.id) == Some(look) {
            return look.id.clone();
        }
        let same = |a: &Appearance| {
            let mut other = look.clone();
            other.id = a.id.clone();
            other.name = a.name.clone();
            *a == other && numbered_from(&a.name, &look.name)
        };
        if let Some(found) = self.appearances.iter().find(|a| same(a)) {
            return found.id.clone();
        }
        // An id no body names either: an undefined id stays undefined.
        let used = |id: &str| {
            self.look(id).is_some()
                || self.body_attributes.values().any(|a| {
                    a.appearance.as_deref() == Some(id)
                        || a.face_appearances.values().any(|f| f == id)
                })
        };
        let base = if look.id.trim().is_empty() {
            "appearance".to_owned()
        } else {
            look.id.clone()
        };
        let id = std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base}_{n}")))
            .find(|id| !used(id))
            .expect("a free appearance id");
        let name = material::numbered_name(&look.name, |name| {
            appearance::library()
                .iter()
                .chain(&self.appearances)
                .any(|a| a.name == name)
        });
        let mut copy = look.clone();
        copy.id = id.clone();
        copy.name = name;
        self.appearances.push(copy);
        id
    }

    /// The id of this design's material for one a linked design gives:
    /// the same one by its id (a built-in, or one brought before), else
    /// one of its density and name (or of its material library's
    /// reference), the design's own or a built-in, else a copy with a
    /// free id and name. `look` is the id of its default appearance here
    /// when the source has one.
    fn take_material(&mut self, given: &Material, look: Option<String>) -> String {
        let mut wanted = given.clone();
        match look {
            Some(id) => wanted.appearance = Some(id),
            None => {
                if wanted
                    .appearance
                    .as_deref()
                    .is_some_and(|id| self.look(id).is_none())
                {
                    wanted.appearance = None;
                }
            }
        }
        self.known_appearances_only(&mut wanted);
        if self.material(&wanted.id) == Some(&wanted) {
            return wanted.id;
        }
        let same = |m: &Material| {
            (m.density - wanted.density).abs() <= SAME_DENSITY
                && (numbered_from(&m.name, &wanted.name)
                    || (wanted.reference.is_some() && m.reference == wanted.reference))
        };
        if let Some(found) = self.materials.iter().find(|m| same(m)) {
            return found.id.clone();
        }
        if let Some(found) = material::built_in()
            .iter()
            .filter_map(|m| self.material(&m.id))
            .find(|m| same(m))
        {
            return found.id.clone();
        }
        let base = if wanted.id.trim().is_empty() {
            "linked_material".to_owned()
        } else {
            wanted.id.clone()
        };
        wanted.id = std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base}_{n}")))
            .find(|id| self.material(id).is_none())
            .expect("a free material id");
        wanted.name = material::numbered_name(&wanted.name, |name| {
            self.material_name_taken(name, &wanted.id, None)
        });
        let id = wanted.id.clone();
        self.materials.push(wanted);
        id
    }

    /// The ids of this design's material and appearance for what a linked
    /// design gives a body, brought in as they are needed.
    fn take_source_look(&mut self, given: &SourceLook) -> (Option<String>, Option<String>) {
        let material = given.material.as_ref().map(|m| {
            let look = given.material_look.as_ref().map(|a| self.take_look(a));
            self.take_material(m, look)
        });
        let appearance = given.appearance.as_ref().map(|a| self.take_look(a));
        (material, appearance)
    }

    /// The bodies `after` of the linked base feature `feature` (that are to
    /// replace `before`, empty for a new link) take what their source gives
    /// them, `looks`, one per body: each records it, and the body takes it
    /// unless what it has was set by hand (a material or appearance other
    /// than the one its source gave before, or a binding). Nothing changes
    /// when `looks` is not one per body (a link shown by its envelope).
    pub(super) fn give_linked_looks(
        &mut self,
        feature: FeatureUid,
        before: &[BaseBody],
        after: &mut [BaseBody],
        looks: &[SourceLook],
    ) {
        if looks.len() != after.len() {
            return;
        }
        for ((body, given), i) in after.iter_mut().zip(looks).zip(0u32..) {
            let (material, appearance) = self.take_source_look(given);
            let number = body.number.unwrap_or(i);
            let (was_material, was_appearance) = before
                .iter()
                .zip(0u32..)
                .find(|(b, j)| b.number.unwrap_or(*j) == number)
                .map(|(b, _)| (b.material.clone(), b.appearance.clone()))
                .unwrap_or_default();
            body.material = material.clone();
            body.appearance = appearance.clone();
            let uid = BodyUid::new(feature, number);
            let mut attributes = self.body_attributes.get(&uid).cloned().unwrap_or_default();
            if attributes.material_expr.is_none() && attributes.material == was_material {
                attributes.material = material;
            }
            if attributes.appearance_expr.is_none() && attributes.appearance == was_appearance {
                attributes.appearance = appearance;
            }
            if attributes.is_default() {
                self.body_attributes.remove(&uid);
            } else {
                self.body_attributes.insert(uid, attributes);
            }
        }
    }

    /// The bodies of a new link's base feature `feature` take what their
    /// source gives them (see [`DocState::give_linked_looks`]).
    pub(super) fn give_new_link_looks(
        &mut self,
        feature: FeatureUid,
        looks: &[SourceLook],
    ) -> Result<(), ModelError> {
        if looks.is_empty() {
            return Ok(());
        }
        let position = self.require(feature)?;
        let mut entry = (*self.features[position]).clone();
        if let FeatureDef::Base(base) = &mut entry.def {
            let mut bodies = std::mem::take(&mut base.bodies);
            self.give_linked_looks(feature, &[], &mut bodies, looks);
            base.bodies = bodies;
        }
        self.features[position] = Arc::new(entry);
        Ok(())
    }
}

/// Whether `looks` give any body a material or an appearance.
pub(super) fn gives_any(looks: &[SourceLook]) -> bool {
    looks
        .iter()
        .any(|look| look.material.is_some() || look.appearance.is_some())
}

/// Where the images of a source design's own appearances are read from
/// (mitcad#1923).
pub(super) enum LookOrigin {
    /// A library's, at the version the source was read at.
    Library(LibraryRequest),
    /// The folder of a linked file; None when it is not known (an image at
    /// a relative path is then left out).
    Folder(Option<PathBuf>),
}

/// What a source design gives the bodies of a derived part (mitcad#1923),
/// one per body in the part's order; None when it gives no body a
/// material or an appearance, whichever bodies it shows.
pub(super) type PartLooks = Option<Vec<SourceLook>>;

/// Whether `source` gives none of its bodies a material or an appearance:
/// no component of it has a default material and no body a material or an
/// appearance of its own.
fn gives_nothing(source: &DocState) -> bool {
    source
        .component_materials
        .values()
        .all(|id| source.material(id).is_none())
        && source.body_attributes.values().all(|a| {
            a.material
                .as_deref()
                .and_then(|id| source.material(id))
                .is_none()
                && a.appearance.is_none()
        })
}

impl<K: Kernel> Document<K> {
    /// What the source design `configured` (a derived part's source in the
    /// part's row and values, `label` naming it) gives each body of the
    /// part (mitcad#1923), the images of its own appearances read from
    /// `origin`. The design is evaluated to know which bodies it shows,
    /// unless it gives no body anything.
    pub(super) fn part_looks(
        &self,
        configured: &DocState,
        label: &str,
        origin: &LookOrigin,
    ) -> Result<PartLooks, ModelError> {
        if gives_nothing(configured) {
            return Ok(None);
        }
        let resolver = match origin {
            LookOrigin::Library(_) => self.link_resolver(),
            LookOrigin::Folder(_) => None,
        };
        let from = match (origin, &resolver) {
            (LookOrigin::Library(request), Some(resolver)) => CopySource::Library {
                resolver: &**resolver,
                request,
            },
            (LookOrigin::Library(_), None) => CopySource::Document(None),
            (LookOrigin::Folder(folder), _) => CopySource::Document(folder.as_deref()),
        };
        let (_, looks) = self.design_shapes_looked(configured, label, Some(&from))?;
        Ok(Some(looks))
    }
}

impl DocState {
    /// The bodies of the derived part `part` that makes the component
    /// `component`, linked with options, take what their source gives them
    /// (`looks`, mitcad#1923): each body whose material is still the one
    /// the source gave it before (none for a link made before mitcad#1921)
    /// takes the new one, a material set on it by hand or by a binding
    /// stays; appearances alike. The link records what was given.
    pub(super) fn give_part_looks(
        &mut self,
        component: ComponentUid,
        part: FeatureUid,
        looks: &PartLooks,
    ) {
        let before: Vec<LinkLook> = self
            .assembly
            .component(component)
            .and_then(|c| c.link.as_ref())
            .map(|l| l.given.clone())
            .unwrap_or_default();
        let mut given = Vec::new();
        for look in looks.iter().flatten() {
            let (material, appearance) = self.take_source_look(look);
            given.push(LinkLook {
                material,
                appearance,
            });
        }
        let count = before.len().max(given.len());
        for (i, number) in (0..count).zip(0u32..) {
            let was = before.get(i).cloned().unwrap_or_default();
            let now = given.get(i).cloned().unwrap_or_default();
            let uid = BodyUid::new(part, number);
            let mut attributes = self.body_attributes.get(&uid).cloned().unwrap_or_default();
            if attributes.material_expr.is_none() && attributes.material == was.material {
                attributes.material = now.material;
            }
            if attributes.appearance_expr.is_none() && attributes.appearance == was.appearance {
                attributes.appearance = now.appearance;
            }
            if attributes.is_default() {
                self.body_attributes.remove(&uid);
            } else {
                self.body_attributes.insert(uid, attributes);
            }
        }
        if let Some(link) = self
            .assembly
            .component_mut(component)
            .and_then(|c| c.link.as_mut())
        {
            link.given = given;
            link.looks = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_names_are_found_again() {
        assert!(numbered_from("Oak", "Oak"));
        assert!(numbered_from("Oak (2)", "Oak"));
        assert!(numbered_from("Oak (12)", " Oak "));
        assert!(!numbered_from("Oak ()", "Oak"));
        assert!(!numbered_from("Oak (x)", "Oak"));
        assert!(!numbered_from("Oaken", "Oak"));
        assert!(!numbered_from("Oak (2) (3)", "Oak (3)"));
    }

    #[test]
    fn a_material_is_taken_once_and_found_again() {
        let mut state = DocState::default();
        let mut wood = material::built_in_material("steel").unwrap().clone();
        wood.id = "test_wood".to_owned();
        wood.name = "Test wood".to_owned();
        wood.density = 0.613;
        wood.appearance = None;
        wood.appearances.clear();
        wood.reference = None;
        assert_eq!(state.take_material(&wood, None), "test_wood");
        assert_eq!(state.take_material(&wood, None), "test_wood");
        assert_eq!(state.materials.len(), 1);
        // Another design's of another density: a name and id of its own,
        // found again by them.
        let mut heavy = wood.clone();
        heavy.density = 0.917;
        assert_eq!(state.take_material(&heavy, None), "test_wood_2");
        assert_eq!(state.materials[1].name, "Test wood (2)");
        assert_eq!(state.take_material(&heavy, None), "test_wood_2");
        assert_eq!(state.materials.len(), 2);
        // A built-in by its id.
        let steel = material::built_in_material("steel").unwrap().clone();
        assert_eq!(state.take_material(&steel, None), "steel");
        assert_eq!(state.materials.len(), 2);
    }
}
