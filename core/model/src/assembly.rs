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
//! is part of the definition state, so undo and the design file cover it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::policy::Policy;
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
    /// Another design file this component shows (read only), when linked.
    pub link: Option<ExternalLink>,
    /// The library it came from, linked or copied (mitcad#64, mitcad#63):
    /// the version, configuration, licence and designation.
    pub library: Option<crate::library::LibraryRef>,
    /// What may be created in and linked to and from it (mitcad#220).
    pub policy: Policy,
    /// What the BUILD tab made it of (mitcad#375); None for other
    /// components.
    pub build: Option<crate::build::BuildRecord>,
}

/// A component linked from another design file: its bodies are the
/// file's, read again when the source has changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalLink {
    /// The file as given; a relative path is relative to the directory of
    /// the design file that links it.
    pub path: String,
    /// A digest of the source text when it was last read, to tell whether
    /// it changed.
    pub digest: String,
    /// The base feature that holds the linked bodies.
    pub feature: FeatureUid,
    /// Shown by its envelope (mitcad#1271, `document/envelope.rs`): the
    /// detail size (mm) of the one body the base feature then holds
    /// instead of the file's bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<f64>,
    /// While shown by its envelope (mitcad#1276): the frames that joints
    /// to faces, edges and vertices of the design's bodies had when it was
    /// last shown in full, which they keep using.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<EnvelopeFrame>,
}

// The detail size is checked finite and positive before it is stored, and
// the frames are rigid transforms of finite numbers.
impl Eq for ExternalLink {}

/// A joint origin's frame on a linked design's geometry, kept while the
/// design is shown by its envelope (mitcad#1276): the geometry as the
/// joint names it and its frame in the component's coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeFrame {
    pub geometry: crate::features::GeomRef,
    pub frame: Transform,
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
    /// Drawn see-through with what it contains (mitcad#1093), independent
    /// of `visible`.
    pub transparent: bool,
    /// The number in its name, `Component1:2`.
    pub number: u32,
    /// Owned by a timeline Pattern at Points. Its placement only exists
    /// after that feature succeeds; the source component stays shared.
    pub pattern: Option<crate::features::point_pattern::PatternInstance>,
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
    /// The root component's policy (mitcad#220).
    pub root_policy: Policy,
}

impl Default for Assembly {
    fn default() -> Self {
        Self {
            root_policy: Policy::GENERAL,
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

    pub fn component_mut(&mut self, uid: ComponentUid) -> Option<&mut ComponentDef> {
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

    pub fn occurrence_mut(&mut self, uid: OccurrenceUid) -> Option<&mut Occurrence> {
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
    pub fn create(&mut self, name: Option<&str>, created_by: Option<FeatureUid>) -> ComponentUid {
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
            policy: Policy::GENERAL,
            build: None,
        });
        uid
    }

    /// A component's policy (the general defaults for one that does not
    /// exist).
    pub fn policy(&self, uid: ComponentUid) -> Policy {
        if uid.is_root() {
            return self.root_policy;
        }
        self.component(uid).map_or(Policy::GENERAL, |c| c.policy)
    }

    /// Sets a component's policy; false when it does not exist.
    pub(crate) fn set_policy(&mut self, uid: ComponentUid, policy: Policy) -> bool {
        if uid.is_root() {
            self.root_policy = policy;
            return true;
        }
        match self.component_mut(uid) {
            Some(c) => {
                c.policy = policy;
                true
            }
            None => false,
        }
    }

    /// Places a component in `parent`; the caller checks that this makes
    /// no cycle ([`Assembly::contains`]).
    pub fn place(
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
            transparent: false,
            number,
            pattern: None,
        });
        uid
    }

    /// Removes a component with its occurrences and the occurrences placed
    /// in it (its features are the caller's).
    pub fn remove(&mut self, uid: ComponentUid) {
        self.components.retain(|c| c.uid != uid);
        self.occurrences
            .retain(|o| o.component != uid && o.parent != uid);
        if !self.exists(self.active) {
            self.active = ComponentUid::ROOT;
        }
    }

    /// Occurrences available at a recomputed timeline position. Definition
    /// state retains inactive pattern IDs for edits, undo and file round trips.
    pub(crate) fn at_placements(&self, placements: &BTreeMap<OccurrenceUid, Transform>) -> Self {
        let mut assembly = self.clone();
        assembly
            .occurrences
            .retain(|o| o.pattern.is_none() || placements.contains_key(&o.uid));
        assembly
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
                return Err(format!("component name {} is used more than once", c.name));
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
            if let Some(pattern) = &o.pattern
                && !features.contains(&pattern.feature)
            {
                return Err(format!(
                    "occurrence {}: pattern {} does not exist",
                    o.uid, pattern.feature
                ));
            }
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
    orthonormality_error(&t.linear) <= 1e-6 && t.determinant() > 0.0
}

// Occurrence placements kept rigid (mitcad#1664): what a drag, the joint
// solver or any command stores is made a rotation and a translation before
// it is kept, so that rounding cannot pile up over many drags; opening a
// design straightens one that drifted (also far: older builds compounded
// the drift, mitcad#1666), and saving never writes one that opening would
// refuse.

/// The largest deviation of the orthonormality products (`Mᵀ·M` from the
/// identity) that the commands which store placements straighten to the
/// nearest rotation instead of refusing. A scale or a mirror is far beyond
/// it; opening a design takes the wider [`DRIFT_BAND`].
pub const STRAIGHTENED: f64 = 1e-3;

/// The singular values (the stretches along the principal axes) a
/// placement read from a design file may have and still be straightened to
/// its nearest rotation: the drift older builds piled up over many drags
/// shrank placements by up to about 8 % (mitcad#1664, mitcad#1666). A
/// deliberate scale (2), a mirror and a degenerate transform are outside
/// it.
pub const DRIFT_BAND: (f64, f64) = (0.5, 1.5);

/// The deviation within which a placement is kept as it is, bit for bit:
/// the rounding of composing exact rotations.
const STRAIGHT: f64 = 1e-9;

/// The largest deviation of `Mᵀ·M` from the identity: 0 for an exact
/// rotation or mirror. Infinite for a matrix with a non-finite entry.
pub fn orthonormality_error(m: &[[f64; 3]; 3]) -> f64 {
    let deviations = (0..3).flat_map(|r| {
        (0..3).map(move |c| {
            let product: f64 = (0..3).map(|k| m[k][r] * m[k][c]).sum();
            (product - f64::from(u8::from(r == c))).abs()
        })
    });
    let mut worst = 0.0_f64;
    for deviation in deviations {
        if !deviation.is_finite() {
            return f64::INFINITY;
        }
        worst = worst.max(deviation);
    }
    worst
}

/// The rigid placement nearest to `t`, its translation kept: `t` itself
/// when it is a rotation within rounding, the nearest rotation (the
/// orthogonal factor of its polar decomposition) when its orthonormality
/// products are off by at most [`STRAIGHTENED`]. None for a non-finite
/// transform, a scale, a mirror or a degenerate one.
pub fn rigid_placement(t: &Transform) -> Option<Transform> {
    nearest_rigid(t, STRAIGHT)
}

/// A placement the joint solver gives, made a rotation and a translation
/// to rounding (mitcad#1666): [`rigid_placement`], but `t` is kept as it
/// is only when it is off by no more than rounding. Each solve that places
/// an occurrence from a frame that is a little off doubles how far off the
/// result is (the solver inverts the frame's rotation by transposing it),
/// so placements kept anywhere within [`rigid_placement`]'s band crept
/// toward it drag after drag; the products of two such (a body seen from
/// another occurrence) then fell outside the geometry's rigid test, and
/// circular edges came out as B-splines that joints could not use.
pub fn solved_placement(t: &Transform) -> Option<Transform> {
    nearest_rigid(t, ROUNDING)
}

/// What [`solved_placement`] keeps as it is: the rounding of a few products
/// of rotations.
const ROUNDING: f64 = 1e-15;

/// The rigid placement nearest to `t`: `t` itself when it is off by at most
/// `keep`, else the nearest rotation when off by at most [`STRAIGHTENED`].
fn nearest_rigid(t: &Transform, keep: f64) -> Option<Transform> {
    if !t.is_finite() || t.determinant() <= 0.0 {
        return None;
    }
    let error = orthonormality_error(&t.linear);
    if error <= keep {
        return Some(*t);
    }
    if error > STRAIGHTENED {
        return None;
    }
    nearest_rotation(t)
}

/// The singular values of a linear part, smallest first: the square roots
/// of the eigenvalues of `Mᵀ·M` (the closed form for a symmetric 3x3).
pub fn singular_values(m: &[[f64; 3]; 3]) -> [f64; 3] {
    let g: [[f64; 3]; 3] =
        std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| m[k][r] * m[k][c]).sum()));
    let off = g[0][1] * g[0][1] + g[0][2] * g[0][2] + g[1][2] * g[1][2];
    let mut eigen = if off <= 0.0 {
        [g[0][0], g[1][1], g[2][2]]
    } else {
        let q = (g[0][0] + g[1][1] + g[2][2]) / 3.0;
        let p =
            (((g[0][0] - q).powi(2) + (g[1][1] - q).powi(2) + (g[2][2] - q).powi(2) + 2.0 * off)
                / 6.0)
                .sqrt();
        let b: [[f64; 3]; 3] = std::array::from_fn(|r| {
            std::array::from_fn(|c| (g[r][c] - if r == c { q } else { 0.0 }) / p)
        });
        let det_b = b[0][0] * (b[1][1] * b[2][2] - b[1][2] * b[2][1])
            - b[0][1] * (b[1][0] * b[2][2] - b[1][2] * b[2][0])
            + b[0][2] * (b[1][0] * b[2][1] - b[1][1] * b[2][0]);
        let phi = (det_b / 2.0).clamp(-1.0, 1.0).acos() / 3.0;
        let largest = q + 2.0 * p * phi.cos();
        let smallest = q + 2.0 * p * (phi + 2.0 * std::f64::consts::PI / 3.0).cos();
        [largest, 3.0 * q - largest - smallest, smallest]
    };
    eigen.sort_by(f64::total_cmp);
    eigen.map(|e| e.max(0.0).sqrt())
}

/// The nearest rotation to a placement read from a design file, its
/// translation kept: for a finite one with a positive determinant whose
/// singular values all lie in [`DRIFT_BAND`] (drift, not a deliberate
/// scale); None for a scale, a mirror, a degenerate or a non-finite one.
pub fn drifted_placement(t: &Transform) -> Option<Transform> {
    if !t.is_finite() || t.determinant() <= 0.0 {
        return None;
    }
    if orthonormality_error(&t.linear) <= STRAIGHT {
        return Some(*t);
    }
    let (low, high) = DRIFT_BAND;
    if !singular_values(&t.linear)
        .iter()
        .all(|s| (low..=high).contains(s))
    {
        return None;
    }
    nearest_rotation(t)
}

/// The orthogonal factor of the polar decomposition of `t`'s linear part
/// (its nearest rotation), the translation kept. X ← X·(3I − XᵀX)/2
/// converges to it for singular values below √3, quadratically near 1:
/// from 1e-3 off four steps reach rounding, from the edges of
/// [`DRIFT_BAND`] about ten. Iterated to convergence, with a cap; None when
/// it does not converge. The determinant stays positive, so it is a
/// rotation.
fn nearest_rotation(t: &Transform) -> Option<Transform> {
    let mut x = t.linear;
    for _ in 0..64 {
        // To rounding: solved placements (mitcad#1666) must not keep even
        // 1e-14 of drift, which later solves would double.
        if orthonormality_error(&x) <= ROUNDING {
            break;
        }
        let xtx: [[f64; 3]; 3] = std::array::from_fn(|r| {
            std::array::from_fn(|c| (0..3).map(|k| x[k][r] * x[k][c]).sum())
        });
        let half: [[f64; 3]; 3] = std::array::from_fn(|r| {
            std::array::from_fn(|c| (3.0 * f64::from(u8::from(r == c)) - xtx[r][c]) / 2.0)
        });
        x = std::array::from_fn(|r| {
            std::array::from_fn(|c| (0..3).map(|k| x[r][k] * half[k][c]).sum())
        });
    }
    let straightened = Transform {
        linear: x,
        translation: t.translation,
    };
    (straightened.is_finite()
        && orthonormality_error(&straightened.linear) <= STRAIGHT
        && straightened.determinant() > 0.0)
        .then_some(straightened)
}

/// The placement a design file keeps for `t`: `t` itself when opening
/// takes it as it is, else its nearest rotation. One with no rotation
/// near it, which no command stores, keeps only its translation (the
/// identity when that is not finite either), so that the file still
/// opens.
pub fn saved_placement(t: &Transform) -> Transform {
    if t.is_finite() && is_rigid(t) {
        return *t;
    }
    drifted_placement(t).unwrap_or_else(|| {
        if t.translation.iter().all(|v| v.is_finite()) {
            Transform::translation(t.translation)
        } else {
            Transform::IDENTITY
        }
    })
}

impl Assembly {
    /// Makes the placements that differ from `before`'s rigid (see
    /// [`rigid_placement`]); those kept from `before` stay as they are.
    /// Fails, naming the occurrence, for one with no rotation near it.
    pub(crate) fn straighten_placements(&mut self, before: &Assembly) -> Result<(), String> {
        let mut straightened = Vec::new();
        for o in &self.occurrences {
            let t = o.transform;
            if t.is_finite() && orthonormality_error(&t.linear) <= STRAIGHT && t.determinant() > 0.0
            {
                continue;
            }
            if before.occurrence(o.uid).is_some_and(|b| b.transform == t) {
                continue;
            }
            let Some(rigid) = rigid_placement(&t) else {
                return Err(format!(
                    "{} would be placed by a transform that is not a rotation and a translation",
                    self.occurrence_name(o.uid)
                ));
            };
            straightened.push((o.uid, rigid));
        }
        for (uid, rigid) in straightened {
            self.occurrence_mut(uid).expect("listed").transform = rigid;
        }
        Ok(())
    }
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
    let t = rows_transform(rows)?;
    if t.is_finite() && is_rigid(&t) {
        Ok(t)
    } else {
        Err(NOT_RIGID.to_owned())
    }
}

const NOT_RIGID: &str = "an occurrence transform must be a rotation and a translation";

/// Like [`from_rows`], for an occurrence placement read from a design file
/// (mitcad#1664): one that drifted off a rotation (its singular values in
/// [`DRIFT_BAND`], drift an older MitCad piled up over many drags) is
/// straightened to the nearest rotation, and true tells that it was. A
/// scale, a mirror, a degenerate or a non-finite one is still refused.
pub fn from_rows_straightened(rows: &[Vec<f64>]) -> Result<(Transform, bool), String> {
    let t = rows_transform(rows)?;
    if t.is_finite() && is_rigid(&t) {
        return Ok((t, false));
    }
    drifted_placement(&t)
        .map(|straightened| (straightened, true))
        .ok_or_else(|| NOT_RIGID.to_owned())
}

/// The transform of three or four rows of four numbers, not yet checked.
fn rows_transform(rows: &[Vec<f64>]) -> Result<Transform, String> {
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
    Ok(Transform {
        linear: std::array::from_fn(|r| std::array::from_fn(|c| rows[r][c])),
        translation: std::array::from_fn(|r| rows[r][3]),
    })
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

    /// A turn about a skew axis with a translation, its matrix pushed off a
    /// rotation by `off` in two entries.
    fn skewed(off: f64) -> Transform {
        let mut t = Transform::rotation([3.0, -2.0, 7.0], [1.0, 2.0, 2.0], 0.7).unwrap();
        t.linear[0][1] += off;
        t.linear[2][0] -= off;
        t
    }

    #[test]
    fn placements_a_little_off_a_rotation_are_straightened() {
        // An exact rotation stays as it is, bit for bit.
        let exact = skewed(0.0);
        assert_eq!(rigid_placement(&exact), Some(exact));
        assert_eq!(saved_placement(&exact), exact);
        // Off by 1e-5: the nearest rotation, the translation kept.
        let off = skewed(1e-5);
        assert!(!is_rigid(&off));
        let straightened = rigid_placement(&off).unwrap();
        assert!(orthonormality_error(&straightened.linear) <= 1e-12);
        assert!(straightened.determinant() > 0.0);
        assert_eq!(straightened.translation, off.translation);
        assert!(
            straightened
                .linear
                .iter()
                .flatten()
                .zip(exact.linear.iter().flatten())
                .all(|(a, b)| (a - b).abs() < 1e-4),
            "{straightened:?}"
        );
        assert_eq!(saved_placement(&off), straightened);
        // Rounding kept within 1e-9 stays as it is.
        let rounded = skewed(1e-11);
        assert_eq!(rigid_placement(&rounded), Some(rounded));
        // A scale, a mirror, a degenerate or a non-finite one is no
        // rotation.
        let scaled = Transform::scale([0.0; 3], [2.0, 2.0, 2.0]);
        let mirror = Transform::mirror([0.0; 3], [0.0, 0.0, 1.0]).unwrap();
        let flat = Transform::scale([0.0; 3], [1.0, 1.0, 0.0]);
        let mut nan = exact;
        nan.linear[1][1] = f64::NAN;
        for t in [scaled, mirror, flat, nan, skewed(0.01)] {
            assert_eq!(rigid_placement(&t), None, "{t:?}");
            assert!(is_rigid(&saved_placement(&t)));
        }
        assert_eq!(saved_placement(&scaled), Transform::IDENTITY);
    }

    #[test]
    fn solved_placements_are_straightened_to_rounding() {
        // What rigid_placement keeps bit for bit, solved_placement makes a
        // rotation to rounding (mitcad#1666); an exact one stays as it is,
        // and one far off is still none.
        let exact = skewed(0.0);
        assert_eq!(solved_placement(&exact), Some(exact));
        for off in [1e-11, 1e-9, 1e-5] {
            let t = skewed(off);
            let solved = solved_placement(&t).unwrap();
            assert!(orthonormality_error(&solved.linear) <= 1e-15, "{off}");
            assert_eq!(solved.translation, t.translation);
        }
        assert_eq!(solved_placement(&skewed(0.01)), None);
    }

    #[test]
    fn a_file_placement_a_little_off_is_read_straightened() {
        let rows = |t: &Transform| matrix_rows(t).map(Vec::from).to_vec();
        let off = skewed(1e-5);
        assert!(from_rows(&rows(&off)).is_err());
        let (read, repaired) = from_rows_straightened(&rows(&off)).unwrap();
        assert!(repaired && is_rigid(&read));
        let exact = skewed(0.0);
        assert_eq!(
            from_rows_straightened(&rows(&exact)).unwrap(),
            (exact, false)
        );
        let scaled = Transform::scale([0.0; 3], [2.0, 2.0, 2.0]);
        assert_eq!(
            from_rows_straightened(&rows(&scaled)).unwrap_err(),
            "an occurrence transform must be a rotation and a translation"
        );
    }

    /// A turn with a translation whose linear part is shrunk by `s` across
    /// the turn's axis (the drift older builds piled up, mitcad#1666): its
    /// singular values are `s`, `s` and 1.
    fn shrunk(s: f64) -> (Transform, Transform) {
        let turn = Transform::rotation([3.0, -2.0, 7.0], [0.0, 0.0, 1.0], 0.7).unwrap();
        let shrunk = turn.after(&Transform::scale([0.0; 3], [s, s, 1.0]));
        (
            turn,
            Transform {
                linear: shrunk.linear,
                translation: turn.translation,
            },
        )
    }

    #[test]
    fn a_file_placement_that_drifted_far_is_read_straightened() {
        let rows = |t: &Transform| matrix_rows(t).map(Vec::from).to_vec();
        // 0.147 off (determinant 0.853), as in a reported design: beyond
        // what commands straighten, straightened when read.
        let (turn, drifted) = shrunk(0.9236);
        assert!((orthonormality_error(&drifted.linear) - 0.147).abs() < 1e-3);
        assert!((drifted.determinant() - 0.853).abs() < 1e-3);
        let values = singular_values(&drifted.linear);
        assert!((values[0] - 0.9236).abs() < 1e-6 && (values[2] - 1.0).abs() < 1e-6);
        assert_eq!(rigid_placement(&drifted), None);
        let (read, repaired) = from_rows_straightened(&rows(&drifted)).unwrap();
        assert!(repaired);
        assert!(orthonormality_error(&read.linear) <= 1e-12);
        // The polar factor of a turn times a stretch along its axes is the
        // turn.
        assert!(
            read.linear
                .iter()
                .flatten()
                .zip(turn.linear.iter().flatten())
                .all(|(a, b)| (a - b).abs() < 1e-12),
            "{read:?}"
        );
        assert_eq!(read.translation, drifted.translation);
        assert_eq!(saved_placement(&drifted), read);
        // Near the edges of the band too.
        for s in [0.51, 1.49] {
            let (_, edge) = shrunk(s);
            assert!(from_rows_straightened(&rows(&edge)).unwrap().1, "{s}");
        }
        // A deliberate scale, a stretch outside the band, a mirror or a
        // degenerate one is still refused.
        let mirror = Transform::mirror([0.0; 3], [0.0, 0.0, 1.0]).unwrap();
        for t in [
            Transform::scale([0.0; 3], [2.0, 2.0, 2.0]),
            shrunk(0.4).1,
            shrunk(1.6).1,
            mirror,
            shrunk(0.9236).1.after(&mirror),
            Transform::scale([0.0; 3], [1.0, 1.0, 0.0]),
        ] {
            assert_eq!(drifted_placement(&t), None, "{t:?}");
            assert!(from_rows_straightened(&rows(&t)).is_err(), "{t:?}");
        }
    }

    #[test]
    fn changed_placements_are_straightened_and_kept_ones_left() {
        let mut before = Assembly::default();
        let c = before.create(Some("Arm"), None);
        let kept = before.place(c, ComponentUid::ROOT, skewed(1e-8));
        let moved = before.place(c, ComponentUid::ROOT, Transform::IDENTITY);
        let mut after = before.clone();
        after.occurrence_mut(moved).unwrap().transform = skewed(1e-5);
        after.straighten_placements(&before).unwrap();
        assert_eq!(after.occurrence(kept).unwrap().transform, skewed(1e-8));
        let straightened = after.occurrence(moved).unwrap().transform;
        assert!(orthonormality_error(&straightened.linear) <= 1e-12);
        after.occurrence_mut(moved).unwrap().transform =
            Transform::scale([0.0; 3], [2.0, 2.0, 2.0]);
        let error = after.straighten_placements(&before).unwrap_err();
        assert_eq!(
            error,
            "Arm:2 would be placed by a transform that is not a rotation and a translation"
        );
    }
}
