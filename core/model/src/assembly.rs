// SPDX-License-Identifier: MIT
//! Components and occurrences (F6): the design is a root
//! component; other components are placed in it, or in each other, by
//! occurrences. A component owns bodies, sketches, construction geometry
//! and features, all in its own coordinates; an occurrence places the
//! component in its parent component with a rigid transform. Several
//! occurrences of one component share its definition, so a change shows in
//! all of them, and a component placed in another one is placed again with
//! every occurrence of that one (subassemblies).
//!
//! The design has one timeline. Every feature belongs to a component
//! ([`crate::FeatureEntry::component`]), which new features take from the
//! active component, and recompute builds each component's bodies in its
//! own coordinates (`recompute.rs`). Where the bodies are in the design
//! follows from the occurrences: their placements (moved by timeline
//! features, [`crate::features::occurrence`]) composed along each path
//! from the root ([`Assembly::instances`]).
//!
//! The structure (components, occurrences, flags, the active component)
//! is part of the definition state, so undo and the project file cover it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::transform::Transform;

/// The root component's name unless renamed.
pub const ROOT_NAME: &str = "Root";

/// A component other than the root.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentDef {
    pub uid: ComponentUid,
    pub name: String,
    /// The feature that made it: a new-component operation (its bodies go
    /// into this component) or a component from bodies. None for an empty
    /// component, an inserted one and a copy.
    pub created_by: Option<FeatureUid>,
    /// Another project file this component shows (read only), when linked.
    pub link: Option<ExternalLink>,
    /// The library it came from, linked or copied (mitcad#64, mitcad#63):
    /// the version, configuration, licence and designation.
    pub library: Option<crate::library::LibraryRef>,
}

/// A component linked from another project file: its bodies are the
/// file's, read again when the source has changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalLink {
    /// The file as given; a relative path is relative to the directory of
    /// the project file that links it.
    pub path: String,
    /// A digest of the source text when it was last read, to tell whether
    /// it changed.
    pub digest: String,
    /// The base feature that holds the linked bodies.
    pub feature: FeatureUid,
}

/// One placement of a component in a parent component.
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub uid: OccurrenceUid,
    pub component: ComponentUid,
    /// The component it is placed in; the root component for top-level
    /// occurrences.
    pub parent: ComponentUid,
    /// The placement in the parent's coordinates before timeline features
    /// move it (a rigid transform).
    pub transform: Transform,
    /// Fixed in place (Ground): it cannot be moved.
    pub grounded: bool,
    /// The browser's light bulb.
    pub visible: bool,
    /// The number in its name, `Component1:2`.
    pub number: u32,
}

/// The components and occurrences of a design.
#[derive(Debug, Clone, PartialEq)]
pub struct Assembly {
    /// The root component's name.
    pub root_name: String,
    /// Components other than the root, in the order they were made.
    pub components: Vec<ComponentDef>,
    /// Every occurrence, in the order they were made.
    pub occurrences: Vec<Occurrence>,
    /// Where new features, components and occurrences go.
    pub active: ComponentUid,
    pub next_component: u32,
    pub next_occurrence: u32,
}

impl Default for Assembly {
    fn default() -> Self {
        Self {
            root_name: ROOT_NAME.to_owned(),
            components: Vec::new(),
            occurrences: Vec::new(),
            active: ComponentUid::ROOT,
            next_component: 1,
            next_occurrence: 1,
        }
    }
}

/// A body as placed in the design: the occurrences from the root down to
/// its component (empty for the root's bodies) and the transform from the
/// component's coordinates to the design's.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub path: Vec<OccurrenceUid>,
    pub component: ComponentUid,
    pub body: BodyUid,
    pub transform: Transform,
    /// Every occurrence on the path is shown.
    pub visible: bool,
}

/// How deep occurrences may nest (a guard against cycles in loaded data).
const MAX_DEPTH: usize = 64;

impl Assembly {
    pub fn is_empty(&self) -> bool {
        self.components.is_empty() && self.occurrences.is_empty()
    }

    pub fn exists(&self, uid: ComponentUid) -> bool {
        uid.is_root() || self.component(uid).is_some()
    }

    pub fn component(&self, uid: ComponentUid) -> Option<&ComponentDef> {
        self.components.iter().find(|c| c.uid == uid)
    }

    pub(crate) fn component_mut(&mut self, uid: ComponentUid) -> Option<&mut ComponentDef> {
        self.components.iter_mut().find(|c| c.uid == uid)
    }

    /// A component's display name.
    pub fn name(&self, uid: ComponentUid) -> String {
        if uid.is_root() {
            return self.root_name.clone();
        }
        self.component(uid)
            .map_or_else(|| uid.to_string(), |c| c.name.clone())
    }

    pub fn occurrence(&self, uid: OccurrenceUid) -> Option<&Occurrence> {
        self.occurrences.iter().find(|o| o.uid == uid)
    }

    pub(crate) fn occurrence_mut(&mut self, uid: OccurrenceUid) -> Option<&mut Occurrence> {
        self.occurrences.iter_mut().find(|o| o.uid == uid)
    }

    /// An occurrence's display name, `Component1:2`.
    pub fn occurrence_name(&self, uid: OccurrenceUid) -> String {
        self.occurrence(uid).map_or_else(
            || uid.to_string(),
            |o| format!("{}:{}", self.name(o.component), o.number),
        )
    }

    /// The occurrences placed directly in a component.
    pub fn children(&self, parent: ComponentUid) -> impl Iterator<Item = &Occurrence> {
        self.occurrences.iter().filter(move |o| o.parent == parent)
    }

    /// The occurrences of a component.
    pub fn occurrences_of(&self, component: ComponentUid) -> impl Iterator<Item = &Occurrence> {
        self.occurrences
            .iter()
            .filter(move |o| o.component == component)
    }

    /// The component a feature made, if any.
    pub fn created_by(&self, feature: FeatureUid) -> Option<ComponentUid> {
        self.components
            .iter()
            .find(|c| c.created_by == Some(feature))
            .map(|c| c.uid)
    }

    /// Whether `inner` is `outer` or placed in it, directly or through
    /// other components.
    pub fn contains(&self, outer: ComponentUid, inner: ComponentUid) -> bool {
        let mut seen = BTreeSet::new();
        let mut todo = vec![outer];
        while let Some(c) = todo.pop() {
            if c == inner {
                return true;
            }
            if seen.insert(c) {
                todo.extend(self.children(c).map(|o| o.component));
            }
        }
        false
    }

    /// The next free `<base><n>` component name.
    pub(crate) fn unused_name(&self, base: &str) -> String {
        (1..)
            .map(|n| format!("{base}{n}"))
            .find(|name| *name != self.root_name && self.components.iter().all(|c| &c.name != name))
            .expect("an unused name exists")
    }

    /// Makes a component (not yet placed anywhere).
    pub(crate) fn create(
        &mut self,
        name: Option<&str>,
        created_by: Option<FeatureUid>,
    ) -> ComponentUid {
        let uid = ComponentUid(self.next_component);
        self.next_component += 1;
        let name = match name {
            Some(name) => name.to_owned(),
            None => self.unused_name("Component"),
        };
        self.components.push(ComponentDef {
            uid,
            name,
            created_by,
            link: None,
            library: None,
        });
        uid
    }

    /// Places a component in `parent`; the caller checks that this makes
    /// no cycle ([`Assembly::contains`]).
    pub(crate) fn place(
        &mut self,
        component: ComponentUid,
        parent: ComponentUid,
        transform: Transform,
    ) -> OccurrenceUid {
        let uid = OccurrenceUid(self.next_occurrence);
        self.next_occurrence += 1;
        let number = self
            .occurrences_of(component)
            .map(|o| o.number)
            .max()
            .unwrap_or(0)
            + 1;
        self.occurrences.push(Occurrence {
            uid,
            component,
            parent,
            transform,
            grounded: false,
            visible: true,
            number,
        });
        uid
    }

    /// Removes a component with its occurrences and the occurrences placed
    /// in it (its features are the caller's).
    pub(crate) fn remove(&mut self, uid: ComponentUid) {
        self.components.retain(|c| c.uid != uid);
        self.occurrences
            .retain(|o| o.component != uid && o.parent != uid);
        if !self.exists(self.active) {
            self.active = ComponentUid::ROOT;
        }
    }

    /// Every body as placed in the design: the root's bodies, then each
    /// occurrence's in a depth-first walk of the occurrence tree.
    /// `placement` gives an occurrence's transform at the timeline marker
    /// and `bodies` a component's bodies.
    pub fn instances(
        &self,
        placement: &dyn Fn(OccurrenceUid) -> Transform,
        bodies: &dyn Fn(ComponentUid) -> Vec<BodyUid>,
    ) -> Vec<Instance> {
        let mut out = Vec::new();
        let mut path = Vec::new();
        self.walk(
            ComponentUid::ROOT,
            Transform::IDENTITY,
            true,
            &mut path,
            placement,
            bodies,
            &mut out,
        );
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &self,
        component: ComponentUid,
        transform: Transform,
        visible: bool,
        path: &mut Vec<OccurrenceUid>,
        placement: &dyn Fn(OccurrenceUid) -> Transform,
        bodies: &dyn Fn(ComponentUid) -> Vec<BodyUid>,
        out: &mut Vec<Instance>,
    ) {
        for body in bodies(component) {
            out.push(Instance {
                path: path.clone(),
                component,
                body,
                transform,
                visible,
            });
        }
        if path.len() >= MAX_DEPTH {
            return;
        }
        for o in self.children(component) {
            if path.contains(&o.uid) {
                continue;
            }
            path.push(o.uid);
            let placed = transform.after(&placement(o.uid));
            self.walk(
                o.component,
                placed,
                visible && o.visible,
                path,
                placement,
                bodies,
                out,
            );
            path.pop();
        }
    }

    /// The paths from the root to every placement of a component (one
    /// empty path for the root itself).
    pub fn paths_to(&self, component: ComponentUid) -> Vec<Vec<OccurrenceUid>> {
        if component.is_root() {
            return vec![Vec::new()];
        }
        let mut paths = Vec::new();
        for o in self.occurrences_of(component) {
            if o.parent == component {
                continue;
            }
            for mut path in self.paths_to(o.parent) {
                if path.len() < MAX_DEPTH {
                    path.push(o.uid);
                    paths.push(path);
                }
            }
        }
        paths
    }

    /// Text of a path: occurrence names joined by `/` (`Arm:1/Pin:2`).
    pub fn path_name(&self, path: &[OccurrenceUid]) -> String {
        path.iter()
            .map(|o| self.occurrence_name(*o))
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Finds a path by its uids (`O1/O4`) or names (`Arm:1/Pin:2`); the
    /// empty text is the root.
    pub fn find_path(&self, text: &str) -> Option<Vec<OccurrenceUid>> {
        let text = text.trim();
        if text.is_empty() {
            return Some(Vec::new());
        }
        let mut parent = ComponentUid::ROOT;
        let mut path = Vec::new();
        for part in text.split('/') {
            let o = self
                .children(parent)
                .find(|o| o.uid.to_string() == part || self.occurrence_name(o.uid) == part)?;
            path.push(o.uid);
            parent = o.component;
        }
        Some(path)
    }

    /// Checks the structure read from a file: known components, parents
    /// and features, no cycles, unique names and numbers.
    pub(crate) fn check(&self, features: &BTreeSet<FeatureUid>) -> Result<(), String> {
        let mut names = BTreeSet::from([self.root_name.as_str()]);
        for c in &self.components {
            if c.uid.is_root() {
                return Err("components: C0 is the root component".to_owned());
            }
            if c.name.trim().is_empty() {
                return Err(format!("component {}: the name is empty", c.uid));
            }
            if !names.insert(c.name.as_str()) {
                return Err(format!(
                    "component name '{}' is used more than once",
                    c.name
                ));
            }
            if let Some(f) = c.created_by.filter(|f| !features.contains(f)) {
                return Err(format!("{}: feature {f} does not exist", c.name));
            }
            if let Some(link) = c.link.as_ref().filter(|l| !features.contains(&l.feature)) {
                return Err(format!(
                    "{}: feature {} does not exist",
                    c.name, link.feature
                ));
            }
        }
        let mut numbers = BTreeMap::new();
        for o in &self.occurrences {
            for c in [o.component, o.parent] {
                if !self.exists(c) {
                    return Err(format!(
                        "occurrence {}: component {c} does not exist",
                        o.uid
                    ));
                }
            }
            if o.component.is_root() {
                return Err(format!(
                    "occurrence {}: the root component cannot be placed",
                    o.uid
                ));
            }
            if !(o.transform.is_finite() && is_rigid(&o.transform)) {
                return Err(format!(
                    "occurrence {}: the transform must be a rotation and a translation",
                    o.uid
                ));
            }
            if numbers.insert((o.component, o.number), o.uid).is_some() {
                return Err(format!(
                    "occurrence {}: {} is used more than once",
                    o.uid,
                    self.occurrence_name(o.uid)
                ));
            }
        }
        for o in &self.occurrences {
            if self.contains(o.component, o.parent) {
                return Err(format!(
                    "occurrence {}: {} would contain itself",
                    o.uid,
                    self.name(o.component)
                ));
            }
        }
        if !self.exists(self.active) {
            return Err(format!(
                "the active component {} does not exist",
                self.active
            ));
        }
        Ok(())
    }
}

/// Whether a transform is a rotation and a translation, within the
/// rounding of imported matrices.
pub fn is_rigid(t: &Transform) -> bool {
    let m = &t.linear;
    let orthonormal = (0..3).all(|r| {
        (0..3).all(|c| {
            let product: f64 = (0..3).map(|k| m[k][r] * m[k][c]).sum();
            (product - f64::from(u8::from(r == c))).abs() <= 1e-6
        })
    });
    orthonormal && t.determinant() > 0.0
}

/// A transform as rows of the rotation and the translation, `[[r00, r01,
/// r02, tx], …]` (the file and command form).
pub fn matrix_rows(t: &Transform) -> [[f64; 4]; 3] {
    std::array::from_fn(|r| {
        [
            t.linear[r][0],
            t.linear[r][1],
            t.linear[r][2],
            t.translation[r],
        ]
    })
}

/// The transform of rows of a rotation and a translation; four rows (a
/// 4x4 matrix) need `[0, 0, 0, 1]` last.
pub fn from_rows(rows: &[Vec<f64>]) -> Result<Transform, String> {
    let shape_error = || "a transform is three or four rows of four numbers".to_owned();
    if !(rows.len() == 3 || rows.len() == 4) || rows.iter().any(|r| r.len() != 4) {
        return Err(shape_error());
    }
    if rows.len() == 4 {
        let last = &rows[3];
        if last[0].abs() + last[1].abs() + last[2].abs() > 1e-9 || (last[3] - 1.0).abs() > 1e-9 {
            return Err("the last row of a 4x4 transform must be [0, 0, 0, 1]".to_owned());
        }
    }
    let t = Transform {
        linear: std::array::from_fn(|r| std::array::from_fn(|c| rows[r][c])),
        translation: std::array::from_fn(|r| rows[r][3]),
    };
    if t.is_finite() && is_rigid(&t) {
        Ok(t)
    } else {
        Err("an occurrence transform must be a rotation and a translation".to_owned())
    }
}

/// The inverse of a rigid transform.
pub fn inverse(t: &Transform) -> Transform {
    let linear: [[f64; 3]; 3] = std::array::from_fn(|r| std::array::from_fn(|c| t.linear[c][r]));
    let moved = Transform {
        linear,
        translation: [0.0; 3],
    }
    .apply_vector(t.translation);
    Transform {
        linear,
        translation: moved.map(|v| -v),
    }
}

/// A 4x4 row-major matrix of a transform, for display.
pub fn matrix4(t: &Transform) -> [[f64; 4]; 4] {
    let rows = matrix_rows(t);
    [rows[0], rows[1], rows[2], [0.0, 0.0, 0.0, 1.0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shift(x: f64) -> Transform {
        Transform::translation([x, 0.0, 0.0])
    }

    #[test]
    fn instances_compose_the_placements_along_each_path() {
        let mut a = Assembly::default();
        let arm = a.create(None, None);
        let pin = a.create(Some("Pin"), None);
        let o1 = a.place(arm, ComponentUid::ROOT, shift(100.0));
        let o2 = a.place(arm, ComponentUid::ROOT, shift(200.0));
        let o3 = a.place(pin, arm, shift(10.0));
        assert_eq!(a.occurrence_name(o2), "Component1:2");
        assert_eq!(a.occurrence_name(o3), "Pin:1");
        let body = |c: ComponentUid| vec![BodyUid::new(FeatureUid(u64::from(c.0) + 1), 0)];
        let placement = |o: OccurrenceUid| a.occurrence(o).unwrap().transform;
        let instances = a.instances(&placement, &body);
        let found: Vec<(Vec<OccurrenceUid>, f64)> = instances
            .iter()
            .map(|i| (i.path.clone(), i.transform.translation[0]))
            .collect();
        assert_eq!(
            found,
            vec![
                (vec![], 0.0),
                (vec![o1], 100.0),
                (vec![o1, o3], 110.0),
                (vec![o2], 200.0),
                (vec![o2, o3], 210.0),
            ]
        );
        assert_eq!(a.paths_to(pin), vec![vec![o1, o3], vec![o2, o3]]);
        assert_eq!(a.find_path("Component1:2/Pin:1"), Some(vec![o2, o3]));
        assert_eq!(a.find_path("O1/O3"), Some(vec![o1, o3]));
        assert_eq!(a.find_path("Pin:1"), None);
        assert_eq!(a.path_name(&[o1, o3]), "Component1:1/Pin:1");
        assert!(a.contains(arm, pin) && !a.contains(pin, arm));
    }

    #[test]
    fn hidden_occurrences_hide_what_they_contain() {
        let mut a = Assembly::default();
        let arm = a.create(None, None);
        let pin = a.create(None, None);
        let o1 = a.place(arm, ComponentUid::ROOT, Transform::IDENTITY);
        a.place(pin, arm, Transform::IDENTITY);
        a.occurrence_mut(o1).unwrap().visible = false;
        let body = |c: ComponentUid| {
            if c.is_root() {
                vec![]
            } else {
                vec![BodyUid::new(FeatureUid(1), c.0)]
            }
        };
        let placement = |o: OccurrenceUid| a.occurrence(o).unwrap().transform;
        let visible: Vec<bool> = a
            .instances(&placement, &body)
            .iter()
            .map(|i| i.visible)
            .collect();
        assert_eq!(visible, vec![false, false]);
    }

    #[test]
    fn transforms_read_and_invert() {
        let t = from_rows(&[
            vec![0.0, -1.0, 0.0, 5.0],
            vec![1.0, 0.0, 0.0, 6.0],
            vec![0.0, 0.0, 1.0, 7.0],
        ])
        .unwrap();
        let back = inverse(&t).after(&t);
        assert!(back.is_identity());
        assert_eq!(matrix_rows(&t)[0], [0.0, -1.0, 0.0, 5.0]);
        let scaled = vec![vec![2.0, 0.0, 0.0, 0.0]; 3];
        assert!(from_rows(&scaled).is_err());
        assert!(
            from_rows(&[
                vec![1.0, 0.0, 0.0, 0.0],
                vec![0.0, 1.0, 0.0, 0.0],
                vec![0.0, 0.0, 1.0, 0.0],
                vec![0.0, 0.0, 1.0, 1.0],
            ])
            .is_err()
        );
    }
}
