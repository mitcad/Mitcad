// SPDX-License-Identifier: MIT
//! A whole design linked with its components (mitcad#2209).
//!
//! Insert Component, linked, of a whole design that has components keeps
//! the design's structure, read only: the linked component holds the
//! bodies of the design's root (a base feature that may hold none), each
//! component of the design becomes a component of its own placed in it
//! (or in another of them) as the design places it, holding the
//! component's own bodies as a base feature, and the design's joints,
//! as-built joints and rigid groups come along as features of the
//! components they belong to, their occurrences grounded as the design
//! grounds them. Supported constraint sets and motion links come along
//! too (mitcad#2252). A joint's origins become fixed planes at the frames the
//! design gives them, its values numbers, so that nothing of it depends
//! on the design's history. [`crate::assembly::LinkTree`] records what was
//! made for what, by the design's uids.
//!
//! None of it can be changed here ([`DocState::editable`] refuses the
//! components, so their features and occurrences, and the joints of the
//! linked component itself): it changes only with the file. An occurrence
//! of the linked component can be made Flexible as any subassembly's
//! (mitcad#1793, mitcad#2016): its joints then move in its parent for it
//! alone, and the parent's joints may join the design's parts.
//!
//! Reading the file again (Update Link, Update All Links, opening the
//! design) updates everything in place: bodies, placements, grounding,
//! joints (whose uids stay, so a flexible occurrence keeps its own values
//! of them, within their new limits), new components, occurrences and
//! joints come in, and those the design no longer has go. A part gone
//! that a feature of this design uses (a joint of the parent on it) stays
//! where it was, with its saved bodies and a message; a flexible
//! occurrence's own values of a joint gone are dropped with a message.
//!
//! Choosing parts on a tree prunes its components and relationships.
//! A design without components, a link initially inserted with chosen
//! parts (mitcad#2130), and links saved before keep the flat form: the
//! bodies as one base feature.
//!
//! A design whose root holds nothing of its own and one component (a new
//! design's shape: the root and Component1, mitcad#2429) comes in as that
//! component, without the root's level: the linked component stands for
//! it ([`LinkTop`], [`single_component`]), its occurrence placed where the
//! insert places it composed with the component's placement in the
//! design. Read again while the design keeps that shape, it stays so; once
//! the design has root content or more components, the design's whole
//! structure comes in, with a line saying so. Chosen parts keep their
//! paths from the design's root; the selection starts below the one
//! component. The one component then gets
//! a component of its own inside the linked one, which takes the base
//! feature of its bodies and the joints made for its own as they are, and
//! the references of this design to its parts follow it (mitcad#2441,
//! `DocState::open_top`): a joint or pick of `part:1/Key:1` names
//! `part:1/Component1:1/Key:1`, one on the bodies of `part:1` names
//! `part:1/Component1:1`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use super::copy::CopySource;
use super::linked_looks::SourceLook;
use super::mounts::SourceMount;
use super::{ComponentCopy, DocState, Document, ModelError, invalid};
use crate::assembly::{
    Assembly, ExternalLink, Flex, Instance, LinkPart, LinkTop, LinkTree, LinkedComponent,
    LinkedFeature, LinkedOccurrence,
};
use crate::constraints::GeoKind;
use crate::exchange::ImportBody;
use crate::features::{
    BaseBody, BaseDef, FeatureDef, GeomRef, JointOrigin, OccurrencePath, References, ValueInput,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::Motion;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::policy::Policy;
use crate::recompute::{Cache, recompute};
use crate::transform::Transform;

/// A design read to link with its components: its root's own bodies and
/// mount frames, its components, occurrences and joints.
pub(super) struct TreeRead {
    /// The root's own visible bodies, in its coordinates, and what the
    /// design gives each (mitcad#1921).
    pub root: Vec<BaseBody>,
    pub root_looks: Vec<SourceLook>,
    /// The design's mount frames (mitcad#525), as a flat link has them.
    pub mounts: Vec<SourceMount>,
    pub components: Vec<ReadComponent>,
    pub occurrences: Vec<ReadOccurrence>,
    pub features: Vec<ReadFeature>,
    pub parts: Option<Vec<LinkPart>>,
    pub messages: Vec<String>,
    /// Read as its one component (mitcad#2429): "the root" above is that
    /// component, its components those placed in it.
    pub top: Option<LinkTop>,
}

/// A component of the design placed in it, with its own visible bodies in
/// its coordinates.
pub(super) struct ReadComponent {
    pub source: ComponentUid,
    variant: Variant,
    previous: Option<ComponentUid>,
    pub name: String,
    pub policy: Policy,
    pub bodies: Vec<BaseBody>,
    pub looks: Vec<SourceLook>,
}

/// An occurrence of the design: where the design places it at its marker.
pub(super) struct ReadOccurrence {
    pub source: OccurrenceUid,
    component: Variant,
    parent: Variant,
    previous: Option<OccurrenceUid>,
    pub transform: Transform,
    pub visible: bool,
    pub transparent: bool,
    pub grounded: bool,
    /// Flexible in the design, with its own values (mitcad#1805), by the
    /// design's uids.
    pub flexible: Option<Flex>,
}

/// A joint, as-built joint or rigid group of the design: its definition
/// with numbers for values and fixed planes for origins, its paths still
/// the design's occurrences.
pub(super) struct ReadFeature {
    pub source: FeatureUid,
    pub name: String,
    component: Variant,
    pub def: FeatureDef<ValueInput>,
}

/// A linked design kept with its components, read again: the component,
/// the file's digest and name, and what was read.
pub(super) struct TreeUpdate {
    pub component: ComponentUid,
    pub digest: String,
    pub file: String,
    pub read: TreeRead,
    pub envelopes: Vec<(FeatureUid, Vec<BaseBody>)>,
}

/// Whether a whole design linked now keeps its components: it has
/// components placed in its root.
pub(super) fn has_components(source: &DocState) -> bool {
    source
        .assembly
        .children(ComponentUid::ROOT)
        .any(|o| o.pattern.is_none())
}

/// Resolved part names for the picker and a message for each missing part.
fn tree_parts(
    assembly: &Assembly,
    root_bodies: &[BodyUid],
    names: &BTreeMap<BodyUid, String>,
    file: &str,
    chosen: Option<&[LinkPart]>,
) -> (Option<Vec<LinkPart>>, Vec<String>) {
    let mut messages = Vec::new();
    let parts = chosen.map(|chosen| {
        chosen
            .iter()
            .cloned()
            .map(|mut part| {
                part.gone = !super::link_parts::part_in(assembly, root_bodies, &part);
                if !part.gone {
                    part.name = part.body.map_or_else(
                        || assembly.path_name(&part.occurrence),
                        |body| names.get(&body).cloned().unwrap_or_default(),
                    );
                } else {
                    let name = if part.name.is_empty() {
                        "A chosen item"
                    } else {
                        &part.name
                    };
                    messages.push(format!(
                        "{name} is no longer in {}",
                        super::components::file_label(file)
                    ));
                }
                part
            })
            .collect()
    });
    (parts, messages)
}

/// A component's occurrence paths can request different subsets of the
/// same source definition. Give each distinct subset its own definition
/// and relationship namespace. Keep previously distinct definitions even
/// when their selections become equal, so existing paths still resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Variant(usize);

impl Variant {
    const ROOT: Self = Self(0);
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SelectionKey {
    source: ComponentUid,
    previous: Option<ComponentUid>,
    /// None when this component and all its descendants are selected.
    parts: Option<Vec<Vec<OccurrenceUid>>>,
}

struct SelectedComponent {
    source: ComponentUid,
    previous: Option<ComponentUid>,
    example: Vec<OccurrenceUid>,
    own: bool,
}

struct SelectedOccurrence {
    source: OccurrenceUid,
    parent: Variant,
    component: Variant,
    previous: Option<OccurrenceUid>,
}

/// Index the old link by parent and source occurrence, so pre-existing
/// shared or split definitions can both be followed during a refresh.
#[derive(Default)]
struct PriorOccurrences {
    by_parent: BTreeMap<(ComponentUid, OccurrenceUid), (OccurrenceUid, ComponentUid)>,
    unique: BTreeMap<OccurrenceUid, Option<(OccurrenceUid, ComponentUid)>>,
    unique_components: BTreeMap<ComponentUid, Option<ComponentUid>>,
    /// A link that stood for its design's one component, read with its
    /// root now (mitcad#2441): that component's occurrence in the design
    /// and the linked component, where what was made for the parts placed
    /// in it is placed.
    opened: Option<(OccurrenceUid, ComponentUid)>,
}

#[derive(Default)]
struct TreeSelection {
    known: BTreeMap<SelectionKey, Variant>,
    components: Vec<SelectedComponent>,
    occurrences: Vec<SelectedOccurrence>,
}

impl TreeSelection {
    fn visit(
        &mut self,
        assembly: &Assembly,
        previous: &PriorOccurrences,
        key: SelectionKey,
        path: Vec<OccurrenceUid>,
    ) -> Variant {
        if let Some(variant) = self.known.get(&key) {
            return *variant;
        }
        let variant = Variant(self.components.len());
        self.components.push(SelectedComponent {
            source: key.source,
            previous: key.previous,
            example: path.clone(),
            own: key.parts.is_none(),
        });
        self.known.insert(key.clone(), variant);
        for child in assembly.children(key.source) {
            let parts = if let Some(selected) = &key.parts {
                let tails: Vec<_> = selected
                    .iter()
                    .filter_map(|part| part.strip_prefix(&[child.uid]).map(|tail| tail.to_vec()))
                    .collect();
                if tails.is_empty() {
                    continue;
                }
                (!tails.iter().any(Vec::is_empty)).then_some(tails)
            } else {
                None
            };
            let old = key
                .previous
                .and_then(|parent| previous.by_parent.get(&(parent, child.uid)).copied())
                .or_else(|| previous.unique.get(&child.uid).copied().flatten());
            // The one component the link stood for (mitcad#2441): what was
            // made for the parts in it is in the linked component itself.
            let opened = previous
                .opened
                .filter(|(at, made)| {
                    key.source.is_root() && *at == child.uid && key.previous == Some(*made)
                })
                .map(|(_, made)| made);
            let mut child_path = path.clone();
            child_path.push(child.uid);
            let component = self.visit(
                assembly,
                previous,
                SelectionKey {
                    source: child.component,
                    previous: old.map(|(_, c)| c).or(opened).or_else(|| {
                        if parts.is_none() {
                            previous
                                .unique_components
                                .get(&child.component)
                                .copied()
                                .flatten()
                        } else {
                            None
                        }
                    }),
                    parts,
                },
                child_path,
            );
            self.occurrences.push(SelectedOccurrence {
                source: child.uid,
                parent: variant,
                component,
                previous: old.map(|(o, _)| o),
            });
        }
        variant
    }
}

/// The selection from the design's root, or from its one component `top`
/// (mitcad#2429): the chosen parts' paths, from the design's root, then
/// below that component (the component itself chosen: all of it).
fn tree_selection(
    assembly: &Assembly,
    parts: Option<&[LinkPart]>,
    existing: Option<(ComponentUid, &LinkTree)>,
    host: &Assembly,
    top: Option<&LinkTop>,
) -> TreeSelection {
    let selected = parts.map(|parts| {
        let mut paths: Vec<_> = parts
            .iter()
            .filter(|part| !part.gone && part.body.is_none())
            .map(|part| part.occurrence.clone())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths
    });
    let selected = match (selected, top) {
        (Some(paths), Some(top)) => {
            let tails: Vec<Vec<OccurrenceUid>> = paths
                .iter()
                .filter_map(|path| path.strip_prefix(&[top.occurrence]).map(<[_]>::to_vec))
                .collect();
            (!tails.iter().any(Vec::is_empty)).then_some(tails)
        }
        (selected, _) => selected,
    };
    let mut previous = PriorOccurrences::default();
    if let Some((made, tree)) = existing {
        // It stood for the one component and comes in with its root now
        // (mitcad#2441, `DocState::open_top`).
        if top.is_none() {
            previous.opened = tree.top.map(|before| (before.occurrence, made));
        }
        for item in tree.components.iter().filter(|item| !item.gone) {
            previous
                .unique_components
                .entry(item.source)
                .and_modify(|row| *row = None)
                .or_insert(Some(item.component));
        }
        let rows = tree.occurrences.iter();
        for item in rows
            .clone()
            .filter(|item| item.gone)
            .chain(rows.filter(|item| !item.gone))
        {
            if let Some(occurrence) = host.occurrence(item.occurrence) {
                previous.by_parent.insert(
                    (occurrence.parent, item.source),
                    (occurrence.uid, occurrence.component),
                );
                if parts.is_none() && !item.gone {
                    previous
                        .unique
                        .entry(item.source)
                        .and_modify(|row| *row = None)
                        .or_insert(Some((occurrence.uid, occurrence.component)));
                }
            }
        }
    }
    let mut selection = TreeSelection::default();
    selection.visit(
        assembly,
        &previous,
        SelectionKey {
            source: top.map_or(ComponentUid::ROOT, |t| t.component),
            previous: existing.map(|(component, _)| component),
            parts: selected,
        },
        top.map(|t| vec![t.occurrence]).unwrap_or_default(),
    );
    selection
}

/// The one component a design comes in as (mitcad#2429): its root holds
/// no feature of its own (no body, sketch, feature or construction, also
/// none suppressed or rolled back), no drawing, and exactly one occurrence, of a
/// component placed only there that is neither linked, a library part nor
/// made by a feature. None: the design comes in with its root.
pub(crate) fn single_component(source: &DocState) -> Option<LinkTop> {
    // A drawing of the root is the root's own too: a copy of the one
    // component would leave it behind.
    if source.features.iter().any(|f| f.component.is_root())
        || source.drawings.iter().any(|d| d.component.is_root())
    {
        return None;
    }
    let a = &source.assembly;
    let mut children = a.children(ComponentUid::ROOT);
    let only = children.next()?;
    if children.next().is_some() || only.pattern.is_some() {
        return None;
    }
    let component = a.component(only.component)?;
    let plain = component.link.is_none()
        && component.library.is_none()
        && component.created_by.is_none()
        && !component.in_place;
    if !plain || a.occurrences_of(only.component).count() != 1 {
        return None;
    }
    Some(LinkTop {
        component: only.component,
        occurrence: only.uid,
        placement: only.transform,
    })
}

/// Mount frames of a design (in its root's coordinates) in those of its
/// one component `top` (mitcad#2429).
pub(super) fn mounts_in_top(top: &LinkTop, mounts: Vec<SourceMount>) -> Vec<SourceMount> {
    let inverse = crate::assembly::inverse(&top.placement);
    mounts
        .into_iter()
        .map(|m| SourceMount {
            frame: inverse.after(&m.frame),
            ..m
        })
        .collect()
}

/// What a link that stands for its design's one component (mitcad#2429)
/// stands for when the design is read again: that, while the design keeps
/// the shape, else (None) the design's root. A link of the root stays so.
pub(super) fn top_now(tree: &LinkTree, source: &DocState) -> Option<LinkTop> {
    tree.top.and_then(|_| single_component(source))
}

/// A joint origin at a frame: a fixed plane through it on the same path.
fn fixed_origin(origin: &JointOrigin, frame: &Transform) -> JointOrigin {
    JointOrigin {
        occurrence: origin.occurrence.clone(),
        geometry: super::mounts::fixed_plane(frame),
        frame_override: None,
    }
}

fn flexible_components(assembly: &Assembly, root: ComponentUid) -> BTreeSet<ComponentUid> {
    let mut components = BTreeSet::from([root]);
    let mut todo = vec![root];
    while let Some(parent) = todo.pop() {
        for child in assembly
            .children(parent)
            .filter(|child| child.flexible.is_some())
        {
            if components.insert(child.component) {
                todo.push(child.component);
            }
        }
    }
    components
}

/// A base feature of a design linked with its components: its bodies
/// from the file `file`, none allowed.
fn tree_base(bodies: Vec<BaseBody>, file: &str) -> FeatureDef<ValueInput> {
    FeatureDef::Base(BaseDef {
        source: Some(file.to_owned()),
        linked_tree: true,
        ..BaseDef::new(bodies)
    })
}

/// What was made for the selected source, scoped to the definition that
/// owns each occurrence and feature.
struct PlacedTree {
    components: BTreeMap<Variant, ComponentUid>,
    occurrences: BTreeMap<(Variant, OccurrenceUid), (OccurrenceUid, Variant)>,
    features: BTreeMap<(Variant, FeatureUid), FeatureUid>,
}

impl PlacedTree {
    fn new(root: ComponentUid) -> Self {
        Self {
            components: BTreeMap::from([(Variant::ROOT, root)]),
            occurrences: BTreeMap::new(),
            features: BTreeMap::new(),
        }
    }

    /// Map an entire path in order; one source occurrence can have
    /// different host IDs below different retained parents.
    fn map_path(&self, mut at: Variant, path: &mut [OccurrenceUid]) -> Option<Variant> {
        for occurrence in path {
            let (host, next) = self.occurrences.get(&(at, *occurrence))?;
            *occurrence = *host;
            at = *next;
        }
        Some(at)
    }

    fn inside(&self, root: Variant) -> BTreeSet<Variant> {
        let mut inside = BTreeSet::from([root]);
        let mut todo = vec![root];
        while let Some(parent) = todo.pop() {
            for (_, (_, child)) in self
                .occurrences
                .range((parent, OccurrenceUid(0))..=(parent, OccurrenceUid(u32::MAX)))
            {
                if inside.insert(*child) {
                    todo.push(*child);
                }
            }
        }
        inside
    }

    fn flex(&self, source: &Flex, root: Variant) -> Flex {
        let inside = self.inside(root);
        let values = |old: &BTreeMap<FeatureUid, BTreeMap<Motion, f64>>| {
            self.features
                .iter()
                .filter_map(|((at, src), &uid)| {
                    if !inside.contains(at) {
                        return None;
                    }
                    Some((uid, old.get(src)?.clone()))
                })
                .collect()
        };
        Flex {
            position: values(&source.position),
            start: values(&source.start),
            free_parts: source.free_parts,
            placements: source
                .placements
                .iter()
                .filter_map(|placement| {
                    let mut placement = placement.clone();
                    self.map_path(root, &mut placement.path)?;
                    Some(placement)
                })
                .collect(),
        }
    }
}

/// Before a placement starts using its own definition, keep the paths
/// and shape references of the parent relationships attached to it.
struct LinkPathRemap<'a> {
    root: ComponentUid,
    before: &'a Assembly,
    old: &'a LinkTree,
    new: &'a LinkTree,
    placed: &'a PlacedTree,
}

impl LinkPathRemap<'_> {
    fn path(
        &self,
        mut owner: ComponentUid,
        path: &mut [OccurrenceUid],
    ) -> Option<(ComponentUid, ComponentUid)> {
        let mut offset = 0;
        while owner != self.root {
            let old = self.before.occurrence(*path.get(offset)?)?;
            if old.parent != owner {
                return None;
            }
            owner = old.component;
            offset += 1;
        }
        let mut source_path = Vec::new();
        let mut original = self.root;
        for uid in &path[offset..] {
            let old = self.before.occurrence(*uid)?;
            if old.parent != original {
                return None;
            }
            source_path.push(
                self.old
                    .occurrences
                    .iter()
                    .find(|row| row.occurrence == *uid)?
                    .source,
            );
            original = old.component;
        }
        let variant = self.placed.map_path(Variant::ROOT, &mut source_path)?;
        path[offset..].copy_from_slice(&source_path);
        Some((original, self.placed.components[&variant]))
    }

    fn origin(
        &self,
        owner: ComponentUid,
        origin: &mut JointOrigin,
        split: &BTreeSet<OccurrenceUid>,
    ) -> bool {
        let must_follow = origin.occurrence.0.iter().any(|uid| split.contains(uid));
        let Some((from, to)) = self.path(owner, &mut origin.occurrence.0) else {
            return !must_follow;
        };
        if from == to {
            return true;
        }
        let old = self
            .old
            .components
            .iter()
            .find(|entry| entry.component == from);
        let new = self
            .new
            .components
            .iter()
            .find(|entry| entry.component == to);
        if let (Some(old), Some(new)) = (old, new) {
            origin.geometry = super::copy::copy_geometry(
                &origin.geometry,
                &HashMap::from([(old.feature, new.feature)]),
            );
        }
        true
    }
}

/// A read relationship in its new owner's reference namespace. An
/// endpoint outside the selection makes the relationship inapplicable.
fn tree_feature_def(f: &ReadFeature, placed: &PlacedTree) -> Option<FeatureDef<ValueInput>> {
    let mut def = f.def.clone();
    let map = |path: &mut [OccurrenceUid]| placed.map_path(f.component, path);
    match &mut def {
        FeatureDef::Joint(j) => {
            map(&mut j.a.occurrence.0)?;
            map(&mut j.b.occurrence.0)?;
        }
        FeatureDef::AsBuiltJoint(j) => {
            map(&mut j.a.0)?;
            map(&mut j.b.0)?;
            if let Some(origin) = &mut j.origin {
                map(&mut origin.occurrence.0)?;
            }
        }
        FeatureDef::RigidGroup(g) => {
            for uid in &mut g.occurrences {
                map(std::slice::from_mut(uid))?;
            }
        }
        FeatureDef::ConstraintSet(set) => {
            for row in &mut set.rows {
                for origin in [&mut row.a, &mut row.b]
                    .into_iter()
                    .chain(row.a2.iter_mut())
                    .chain(row.b2.iter_mut())
                {
                    map(&mut origin.occurrence.0)?;
                }
            }
        }
        FeatureDef::MotionLink(link) => {
            for motion in [&mut link.a, &mut link.b] {
                let variant = match motion.occurrence.as_mut() {
                    Some(uid) => map(std::slice::from_mut(uid))?,
                    None => f.component,
                };
                motion.joint = *placed.features.get(&(variant, motion.joint))?;
            }
        }
        _ => return None,
    }
    Some(def)
}

impl<K: Kernel> Document<K> {
    /// The design `source` (read from `path`; the images of its
    /// appearances through `origin`) to link with its components: one
    /// evaluation at its marker. Relationships that are suppressed or
    /// rolled back, fail there, or depend on geometry that a frozen frame
    /// cannot represent are left out. Refused when no bodies are selected.
    /// With `top` (mitcad#2429, [`single_component`]) it is read as that
    /// component: its bodies, mount frames and relationships as the
    /// root's, the components placed in it as the design's, the root's
    /// level left out.
    pub(super) fn design_tree(
        &self,
        source: &DocState,
        path: &str,
        origin: &CopySource<'_>,
        chosen: Option<&[LinkPart]>,
        existing: Option<(ComponentUid, &LinkTree)>,
        top: Option<&LinkTop>,
    ) -> Result<TreeRead, ModelError> {
        let kernel = &self.kernel;
        // A cancel (P7) stops this too.
        let monitor = self.monitor.as_ref();
        let result = recompute(kernel, source, &mut Cache::default(), monitor)?;
        let mut mounts = super::mounts::evaluated_mounts(source, &result);
        if let Some(top) = top {
            mounts = mounts_in_top(top, mounts);
        }
        let names = super::display_names(source, &result);
        let a = source.assembly.at_placements(&result.placements);
        let root_bodies: Vec<_> = result
            .owners
            .iter()
            .filter(|(_, c)| c.is_root())
            .map(|(body, _)| *body)
            .collect();
        let (parts, messages) = tree_parts(&a, &root_bodies, &names, path, chosen);
        let envelope_body = |component, body| {
            a.component(component)
                .and_then(|c| c.link.as_ref())
                .and_then(|link| link.tree.as_ref())
                .is_some_and(|tree| tree.is_envelope_body(body))
        };
        // From the design's root, or from its one component (mitcad#2429),
        // which then stands for the root (Variant::ROOT).
        let selection = tree_selection(&a, parts.as_deref(), existing, &self.state.assembly, top);
        let mut by_source = BTreeMap::<ComponentUid, Vec<Variant>>::new();
        for (index, chosen) in selection.components.iter().enumerate() {
            by_source
                .entry(chosen.source)
                .or_default()
                .push(Variant(index));
        }
        // A component's own visible bodies, in its coordinates.
        let own =
            |selected: &SelectedComponent| -> Result<(Vec<BaseBody>, Vec<SourceLook>), ModelError> {
                let c = selected.source;
                let mut bodies = Vec::new();
                let mut looks = Vec::new();
                let mut numbers = Vec::new();
                let mut number = 0;
                let mut first_face = Some(0u32);
                for (body, owner) in &result.owners {
                    let shown = source.body_attributes.get(body).is_none_or(|b| b.visible);
                    if *owner != c || !shown || envelope_body(c, *body) {
                        continue;
                    }
                    let Some(shape) = result.bodies.get(body) else {
                        continue;
                    };
                    let take_body = if c.is_root() {
                        parts
                            .as_ref()
                            .is_none_or(|parts| parts.iter().any(|p| p.body == Some(*body)))
                    } else {
                        selected.own
                    };
                    let index = (number, first_face);
                    number += 1;
                    if c.is_root() && parts.is_some() {
                        first_face = first_face.and_then(|first| {
                            first.checked_add(
                                u32::try_from(kernel.face_count(&shape.value).ok()?).ok()?,
                            )
                        });
                    }
                    if !take_body {
                        continue;
                    }
                    bodies.push(ImportBody {
                        name: names.get(body).cloned(),
                        color: None,
                        shape: shape.value.clone(),
                        face_colors: Vec::new(),
                    });
                    let instance = Instance {
                        path: selected.example.clone(),
                        component: c,
                        body: *body,
                        transform: Transform::IDENTITY,
                        visible: true,
                    };
                    looks.push(super::linked_looks::source_look(source, &instance, origin));
                    numbers.push(index);
                }
                let mut bodies = self.base_bodies(&bodies)?;
                if c.is_root() && parts.is_some() {
                    for (body, (index, first_face)) in bodies.iter_mut().zip(numbers) {
                        body.number = Some(index);
                        body.first_face = first_face;
                    }
                }
                Ok((bodies, looks))
            };
        let (root, root_looks) = own(&selection.components[Variant::ROOT.0])?;
        let mut components = Vec::with_capacity(selection.components.len().saturating_sub(1));
        for c in &a.components {
            for &variant in by_source.get(&c.uid).into_iter().flatten() {
                // The one component read as the root (mitcad#2429).
                if variant == Variant::ROOT {
                    continue;
                }
                let selected = &selection.components[variant.0];
                let (bodies, looks) = own(selected)?;
                components.push(ReadComponent {
                    source: c.uid,
                    variant,
                    previous: selected.previous,
                    name: c.name.clone(),
                    policy: c.policy,
                    bodies,
                    looks,
                });
            }
        }
        if root.is_empty() && components.iter().all(|c| c.bodies.is_empty()) {
            return Err(invalid(format!("{path} has no bodies to show")));
        }
        let mut selected_occurrences = selection.occurrences;
        selected_occurrences.sort_by_key(|o| (o.source, o.parent));
        let occurrences = selected_occurrences
            .into_iter()
            .filter_map(|selected| {
                let o = a.occurrence(selected.source)?;
                Some(ReadOccurrence {
                    source: o.uid,
                    component: selected.component,
                    parent: selected.parent,
                    previous: selected.previous,
                    transform: result
                        .placements
                        .get(&o.uid)
                        .copied()
                        .unwrap_or(o.transform),
                    visible: o.visible,
                    transparent: o.transparent,
                    grounded: o.grounded,
                    flexible: o.flexible.clone(),
                })
            })
            .collect();
        // The joints that hold at the marker, with their values as numbers
        // and their origins where the design has them.
        let holding: BTreeSet<FeatureUid> = result
            .results
            .iter()
            .filter(|r| r.status.is_ok())
            .map(|r| r.uid)
            .collect();
        let mut features = Vec::new();
        for entry in &source.features[..source.marker] {
            let wanted = matches!(
                entry.def,
                FeatureDef::Joint(_)
                    | FeatureDef::AsBuiltJoint(_)
                    | FeatureDef::RigidGroup(_)
                    | FeatureDef::ConstraintSet(_)
                    | FeatureDef::MotionLink(_)
            );
            if !wanted
                || entry.suppressed
                || !by_source.contains_key(&entry.component)
                || !holding.contains(&entry.uid)
            {
                continue;
            }
            let number = entry.def.map_params(&mut |_, id: &ParamId| {
                source
                    .parameters
                    .value(*id)
                    .map(ValueInput::Number)
                    .ok_or(())
            });
            let Ok(mut def) = number else {
                continue;
            };
            let fixed = |o: &JointOrigin| {
                crate::joints::resolved_origin_frame(kernel, source, &result, entry.component, o)
                    .ok()
                    .map(|frame| fixed_origin(o, &frame))
            };
            let framed = match (&entry.def, &mut def) {
                (FeatureDef::Joint(j), FeatureDef::Joint(d)) => match (fixed(&j.a), fixed(&j.b)) {
                    (Some(a), Some(b)) => {
                        d.a = a;
                        d.b = b;
                        true
                    }
                    _ => false,
                },
                (FeatureDef::AsBuiltJoint(j), FeatureDef::AsBuiltJoint(d)) => match &j.origin {
                    None => true,
                    Some(o) => {
                        d.origin = fixed(o);
                        d.origin.is_some()
                    }
                },
                (_, FeatureDef::ConstraintSet(set)) => set.rows.iter_mut().all(|row| {
                    [&mut row.a, &mut row.b]
                        .into_iter()
                        .chain(row.a2.iter_mut())
                        .chain(row.b2.iter_mut())
                        .all(|o| {
                            let Ok(geo) = crate::joints::resolved_constraint_geo(
                                kernel,
                                source,
                                &result,
                                entry.component,
                                o,
                            ) else {
                                return false;
                            };
                            let [origin, _, _, direction] = crate::joints::frame_parts(&geo.frame);
                            o.geometry = match geo.kind {
                                GeoKind::Point => GeomRef::FixedPoint(origin),
                                GeoKind::Plane => super::mounts::fixed_plane(&geo.frame),
                                GeoKind::Axis => GeomRef::FixedAxis { origin, direction },
                                GeoKind::Circle | GeoKind::Cylinder | GeoKind::Sphere => {
                                    return false;
                                }
                            };
                            o.frame_override = None;
                            true
                        })
                }),
                (_, FeatureDef::MotionLink(link)) => link.coupling.origins().is_empty(),
                _ => true,
            };
            if framed {
                for &variant in &by_source[&entry.component] {
                    features.push(ReadFeature {
                        source: entry.uid,
                        name: entry.name.clone(),
                        component: variant,
                        def: def.clone(),
                    });
                }
            }
        }
        Ok(TreeRead {
            root,
            root_looks,
            mounts,
            components,
            occurrences,
            features,
            parts,
            messages,
            top: top.copied(),
        })
    }
}

impl DocState {
    fn remap_parent_links(
        &mut self,
        remap: &LinkPathRemap<'_>,
        split: &BTreeSet<OccurrenceUid>,
    ) -> Result<(), ModelError> {
        for index in 0..self.features.len() {
            let feature = &self.features[index];
            let component = feature.component;
            if component == remap.root
                || remap.old.holds(component)
                || remap.new.holds(component)
                || !crate::joints::referenced_occurrences(&feature.def)
                    .iter()
                    .any(|uid| split.contains(uid))
            {
                continue;
            }
            let mut entry = (**feature).clone();
            let fixed_move = entry.def.move_spec().is_none_or(|spec| {
                spec.occurrence_paths()
                    .iter()
                    .all(|path| !path.0.iter().any(|uid| split.contains(uid)))
            });
            let path = |path: &mut [OccurrenceUid]| {
                let must_follow = path.iter().any(|uid| split.contains(uid));
                remap.path(component, path).is_some() || !must_follow
            };
            let origin = |origin: &mut JointOrigin| remap.origin(component, origin, split);
            let followed = match &mut entry.def {
                FeatureDef::Joint(joint) => origin(&mut joint.a) && origin(&mut joint.b),
                FeatureDef::AsBuiltJoint(joint) => {
                    path(&mut joint.a.0)
                        && path(&mut joint.b.0)
                        && joint.origin.as_mut().is_none_or(origin)
                }
                FeatureDef::ConstraintSet(set) => set.rows.iter_mut().all(|row| {
                    [&mut row.a, &mut row.b]
                        .into_iter()
                        .chain(row.a2.iter_mut())
                        .chain(row.b2.iter_mut())
                        .all(origin)
                }),
                // These features use the placement itself, which keeps
                // its ID, rather than geometry inside its definition.
                FeatureDef::RigidGroup(_) | FeatureDef::CapturePosition(_) => continue,
                FeatureDef::MoveOccurrence(_) if fixed_move => continue,
                _ => {
                    return Err(invalid(format!(
                        "{} uses a placement that needs its own linked parts; its references cannot follow this choice",
                        entry.name
                    )));
                }
            };
            if !followed || self.check_feature(index, &entry).is_err() {
                return Err(invalid(format!(
                    "{} uses a part that this choice would leave out",
                    entry.name
                )));
            }
            self.features[index] = Arc::new(entry);
        }
        Ok(())
    }

    /// Makes the components, occurrences and joints of a design read to
    /// link with its components in the linked component `made` (its own
    /// base feature and mount frames are made by the insert); `file` names
    /// the file in the base features. Returns what was made for what.
    pub(super) fn build_link_tree(
        &mut self,
        made: ComponentUid,
        read: &TreeRead,
        file: &str,
    ) -> Result<LinkTree, ModelError> {
        let mut tree = LinkTree {
            // Standing for the design's one component (mitcad#2429).
            top: read.top,
            ..LinkTree::default()
        };
        let mut placed = PlacedTree::new(made);
        for c in &read.components {
            let record = self.new_tree_component(c, file)?;
            placed.components.insert(c.variant, record.component);
            tree.components.push(record);
        }
        for o in &read.occurrences {
            let uid = self.place_tree_occurrence(
                placed.components[&o.component],
                placed.components[&o.parent],
                o,
            );
            placed
                .occurrences
                .insert((o.parent, o.source), (uid, o.component));
            tree.occurrences.push(LinkedOccurrence {
                source: o.source,
                occurrence: uid,
                gone: false,
            });
        }
        for f in &read.features {
            let Some(def) = tree_feature_def(f, &placed) else {
                continue;
            };
            let feature = self.add_tree_feature(f, placed.components[&f.component], &def)?;
            placed.features.insert((f.component, f.source), feature);
            tree.features.push(LinkedFeature {
                source: f.source,
                feature,
            });
        }
        self.take_inner_flex(read, &placed);
        Ok(tree)
    }

    /// A component made for one of the design's, with its bodies.
    fn new_tree_component(
        &mut self,
        c: &ReadComponent,
        file: &str,
    ) -> Result<LinkedComponent, ModelError> {
        let name = self.free_component_name(&c.name);
        let component = self.assembly.create(Some(&name), None);
        self.assembly.set_policy(component, c.policy);
        let added = self.add_feature_in(&tree_base(c.bodies.clone(), file), None, component)?;
        self.give_new_link_looks(added.uid, &c.looks)?;
        Ok(LinkedComponent {
            source: c.source,
            name: c.name.clone(),
            component,
            feature: added.uid,
            gone: false,
        })
    }

    /// An occurrence made for one of the design's, as the design has it.
    fn place_tree_occurrence(
        &mut self,
        component: ComponentUid,
        parent: ComponentUid,
        o: &ReadOccurrence,
    ) -> OccurrenceUid {
        let uid = self.assembly.place(component, parent, o.transform);
        if let Some(placed) = self.assembly.occurrence_mut(uid) {
            placed.grounded = o.grounded;
            placed.visible = o.visible;
            placed.transparent = o.transparent;
        }
        uid
    }

    /// A joint, as-built joint or rigid group made for one of the design's,
    /// under its name there when that is free.
    fn add_tree_feature(
        &mut self,
        f: &ReadFeature,
        component: ComponentUid,
        def: &FeatureDef<ValueInput>,
    ) -> Result<FeatureUid, ModelError> {
        let name = (!f.name.trim().is_empty() && self.check_feature_name(&f.name).is_ok())
            .then(|| f.name.clone());
        Ok(self.add_feature_in(def, name.as_deref(), component)?.uid)
    }

    /// The occurrences inside the linked component flexible as the design
    /// has them (mitcad#1805), with its own values of the joints made here.
    fn take_inner_flex(&mut self, read: &TreeRead, placed: &PlacedTree) {
        for o in &read.occurrences {
            let Some(&(uid, component)) = placed.occurrences.get(&(o.parent, o.source)) else {
                continue;
            };
            let flex = o.flexible.as_ref().map(|f| placed.flex(f, component));
            if let Some(occurrence) = self.assembly.occurrence_mut(uid) {
                occurrence.flexible = flex;
            }
        }
    }

    /// A base feature of the tree takes new bodies, with what the design
    /// gives them (mitcad#1921).
    fn set_tree_bodies(
        &mut self,
        feature: FeatureUid,
        mut bodies: Vec<BaseBody>,
        looks: &[SourceLook],
    ) -> Result<(), ModelError> {
        let position = self.require(feature)?;
        let mut entry = (*self.features[position]).clone();
        if let FeatureDef::Base(base) = &mut entry.def {
            self.give_linked_looks(feature, &base.bodies, &mut bodies, looks);
            base.bodies = bodies;
            base.linked_tree = true;
        }
        self.features[position] = Arc::new(entry);
        Ok(())
    }

    /// The occurrences that features other than the linked design's own
    /// use (a joint of the parent on a part of it): every occurrence on
    /// their paths.
    fn used_outside(&self, tree: &LinkTree, made: ComponentUid) -> BTreeSet<OccurrenceUid> {
        self.features
            .iter()
            .filter(|f| !tree.made(f.uid) && f.component != made && !tree.holds(f.component))
            .flat_map(|f| crate::joints::referenced_occurrences(&f.def))
            .collect()
    }

    /// A joint's limits now: per motion its least and greatest value.
    fn joint_limits(&self, joint: FeatureUid) -> BTreeMap<Motion, (Option<f64>, Option<f64>)> {
        let limits = match self.entry(joint).map(|e| &e.def) {
            Some(FeatureDef::Joint(j)) => &j.limits,
            Some(FeatureDef::AsBuiltJoint(j)) => &j.limits,
            _ => return BTreeMap::new(),
        };
        let value = |id: Option<ParamId>| id.and_then(|id| self.parameters.value(id));
        limits
            .iter()
            .map(|(m, l)| (*m, (value(l.min), value(l.max))))
            .collect()
    }

    /// A linked design that came in as its one component (mitcad#2429)
    /// read again as `now` (None: with its root, as the design has more
    /// now): the occurrences of the linked component `made` follow where
    /// the design's root places what the linked component stands for
    /// (the one component's placement, or the root's identity), the moves
    /// made here kept. Returns the one component it stood for when the
    /// design now comes in with its root (`DocState::open_top` then
    /// gives that component one of its own). A design linked with its root
    /// stays so.
    fn take_top(
        &mut self,
        made: ComponentUid,
        tree: &mut LinkTree,
        now: Option<LinkTop>,
    ) -> Option<LinkTop> {
        let before = tree.top?;
        let placement = now.map_or(Transform::IDENTITY, |t| t.placement);
        if placement != before.placement {
            // An occurrence at T ∘ before goes to T ∘ placement.
            let shift = crate::assembly::inverse(&before.placement).after(&placement);
            for o in self
                .assembly
                .occurrences
                .iter_mut()
                .filter(|o| o.component == made)
            {
                o.transform = o.transform.after(&shift);
            }
        }
        tree.top = now;
        now.is_none().then_some(before)
    }

    /// A linked design that came in as its one component `top`
    /// (mitcad#2429), read again with its root as `read` (mitcad#2441):
    /// that component gets a component of its own, placed in the linked
    /// component `made` where the design places it, and takes the base
    /// feature of its bodies and the joints made for its own, which keep
    /// their uids; the linked component gets a new base feature for the
    /// root's bodies (`link` follows). The references of this design to
    /// its parts follow, so that they name the same parts and geometry: a
    /// path through an occurrence of `made` to a part placed in the one
    /// component goes on through the new occurrence (as Reparent
    /// Occurrence makes it follow), and so does one that ends at it and
    /// names the geometry of its bodies (as Move History into Component
    /// makes it follow) or a fixed frame or origin plane of a joint origin
    /// (`deepen_fixed_origins`). `read` then takes the new component and
    /// occurrence as what was made for the one component before, so that
    /// the update keeps them. The line of the update that says so, naming
    /// what could not follow; when the design no longer places the one
    /// component in its root, nothing can follow and the line says that
    /// the parts may need to be picked again.
    fn open_top(
        &mut self,
        made: ComponentUid,
        link: &mut ExternalLink,
        tree: &mut LinkTree,
        top: &LinkTop,
        read: &mut TreeRead,
        file: &str,
    ) -> Result<String, ModelError> {
        let name = self.assembly.name(made);
        let at_o = read.occurrences.iter().position(|o| {
            o.source == top.occurrence && o.parent == Variant::ROOT && o.previous.is_none()
        });
        let at_c = at_o.and_then(|i| {
            let variant = read.occurrences[i].component;
            read.components
                .iter()
                .position(|c| c.variant == variant && c.source == top.component)
        });
        let (Some(at_o), Some(at_c)) = (at_o, at_c) else {
            return Ok(format!(
                "{name} changed its structure: {file} now holds more than its one component, so \
                 it comes in with its root and components as the file has them; joints and \
                 references to its parts may need to be picked again"
            ));
        };
        let (c, o) = (&read.components[at_c], &read.occurrences[at_o]);
        let variant = c.variant;
        let old = link.feature;
        let bodies: Vec<BodyUid> = match self.entry(old).map(|e| &e.def) {
            Some(FeatureDef::Base(base)) => (0u32..)
                .zip(&base.bodies)
                .map(|(i, _)| base.body_uid(old, i))
                .collect(),
            _ => Vec::new(),
        };
        // The one component's own, with its occurrence.
        let component_name = self.free_component_name(&c.name);
        let component = self.assembly.create(Some(&component_name), None);
        self.assembly.set_policy(component, c.policy);
        let occurrence = self.place_tree_occurrence(component, made, o);
        tree.occurrences.push(LinkedOccurrence {
            source: o.source,
            occurrence,
            gone: false,
        });
        // Its bodies' base feature and the joints made for its own go into
        // it, as they are.
        for f in &mut self.features {
            if f.component == made && (f.uid == old || tree.made(f.uid)) {
                Arc::make_mut(f).component = component;
            }
        }
        tree.components.push(LinkedComponent {
            source: c.source,
            name: c.name.clone(),
            component,
            feature: old,
            gone: false,
        });
        // The update keeps them as what was made for it.
        read.components[at_c].previous = Some(component);
        read.occurrences[at_o].previous = Some(occurrence);
        // The linked component's own bodies are the root's now.
        let added = self.add_feature_in(&tree_base(read.root.clone(), file), None, made)?;
        self.give_new_link_looks(added.uid, &read.root_looks)?;
        link.feature = added.uid;
        if let Some(l) = self
            .assembly
            .component_mut(made)
            .and_then(|c| c.link.as_mut())
        {
            l.feature = added.uid;
        }

        // What the design made for it goes its own way.
        let skip: BTreeSet<FeatureUid> = self
            .features
            .iter()
            .filter(|f| f.component == made || tree.holds(f.component) || tree.made(f.uid))
            .map(|f| f.uid)
            .collect();
        // The geometry of its bodies and its fixed frames, seen through an
        // occurrence of the linked component.
        super::move_features::follow_into(
            self,
            made,
            component,
            occurrence,
            &[old],
            &bodies,
            !read.root.is_empty(),
        );
        self.deepen_fixed_origins(made, occurrence, &skip);
        self.deepen_picked_points(made, occurrence, &BTreeSet::from([old]), &skip);
        // The parts placed in it, one level down: those made for them
        // before, placed in the linked component (`tree_selection` looks
        // them up there).
        let inside: Vec<OccurrenceUid> = read
            .occurrences
            .iter()
            .filter(|r| r.parent == variant)
            .filter_map(|r| r.previous)
            .filter(|uid| {
                self.assembly
                    .occurrence(*uid)
                    .is_some_and(|x| x.parent == made)
            })
            .collect();
        let mut moves = Vec::new();
        for path in self.assembly.paths_to(made) {
            for part in &inside {
                let mut old = path.clone();
                old.push(*part);
                let mut new = path.clone();
                new.extend([occurrence, *part]);
                moves.push((old, new));
            }
        }
        let stuck = super::reparent::follow_moved_down(self, &moves, &skip);
        for part in &inside {
            if let Some(x) = self.assembly.occurrence_mut(*part) {
                x.parent = component;
            }
        }
        Ok(if stuck.is_empty() {
            format!(
                "{name} changed its structure: {file} now holds more than its one component, so \
                 it comes in with its root and components as the file has them; the references \
                 to its parts follow them"
            )
        } else {
            format!(
                "{name} changed its structure: {file} now holds more than its one component, so \
                 it comes in with its root and components as the file has them; the references \
                 to its parts follow them except these, which may need to be picked again: {}",
                stuck.join("; ")
            )
        })
    }

    /// The points and axes of this design's features (not `skip`) picked
    /// on the linked component `made` that its joint origins' rewrites do
    /// not carry (mitcad#2466): Pattern at Points' targets and anchor and
    /// moves' picked points and axes (paths from the root that end at an
    /// occurrence of `made`), when fixed or an origin plane, axis or point
    /// or on the geometry of `moved` (the bodies of the one component it
    /// stood for), and motion links' origins on that geometry (the fixed
    /// ones go in [`DocState::deepen_fixed_origins`]), go on through
    /// `occurrence`, which places that component now. The occurrences a
    /// pattern made keep their keys.
    fn deepen_picked_points(
        &mut self,
        made: ComponentUid,
        occurrence: OccurrenceUid,
        moved: &BTreeSet<FeatureUid>,
        skip: &BTreeSet<FeatureUid>,
    ) {
        let assembly = self.assembly.clone();
        // From `from`, fixed geometry too when `fixed`.
        let deepen =
            |from: ComponentUid, path: &mut OccurrencePath, geometry: &GeomRef, fixed: bool| {
                if crate::joints::path_component(&assembly, from, &path.0) != Ok(made) {
                    return false;
                }
                let is_fixed = matches!(
                    geometry,
                    GeomRef::Origin(_)
                        | GeomRef::FixedPlane { .. }
                        | GeomRef::FixedAxis { .. }
                        | GeomRef::FixedPoint(_)
                );
                let mut refs = References::default();
                geometry.add_to(&mut refs);
                if (fixed && is_fixed) || refs.features.iter().any(|f| moved.contains(f)) {
                    path.0.push(occurrence);
                    return true;
                }
                false
            };
        let mut patterns = Vec::new();
        for f in &mut self.features {
            if skip.contains(&f.uid) {
                continue;
            }
            let mut def = f.def.clone();
            let mut changed = false;
            match &mut def {
                FeatureDef::PatternAtPoints(p) => {
                    for point in p.targets.iter_mut().chain(p.anchor.iter_mut()) {
                        if let Some(path) = point.occurrence.as_mut() {
                            changed |= deepen(ComponentUid::ROOT, path, &point.point, true);
                        }
                    }
                }
                FeatureDef::MotionLink(link) => {
                    for o in link.coupling.origins_mut() {
                        changed |= deepen(f.component, &mut o.occurrence, &o.geometry, false);
                    }
                }
                other => {
                    if let Some(spec) = other.move_spec_mut() {
                        for (geometry, path) in spec.placed_references_mut() {
                            if let Some(path) = path.as_mut() {
                                changed |= deepen(ComponentUid::ROOT, path, geometry, true);
                            }
                        }
                    }
                }
            }
            if !changed {
                continue;
            }
            if let (FeatureDef::PatternAtPoints(before), FeatureDef::PatternAtPoints(after)) =
                (&f.def, &def)
            {
                patterns.push((f.uid, before.clone(), after.clone()));
            }
            Arc::make_mut(f).def = def;
        }
        for (uid, before, after) in patterns {
            self.follow_point_pattern(uid, &before, &after);
        }
    }

    /// The joint origins of this design's features (not `skip`) on a fixed
    /// frame or an origin plane, axis or point of the linked component
    /// `made` (a path that ends at one of its occurrences) go on through
    /// `occurrence` (mitcad#2441): they were in the coordinates of the one
    /// component it stood for, which that occurrence places now. Joints,
    /// as-built joints' origins, constraint rows, bound lengths and motion
    /// links.
    fn deepen_fixed_origins(
        &mut self,
        made: ComponentUid,
        occurrence: OccurrenceUid,
        skip: &BTreeSet<FeatureUid>,
    ) {
        let assembly = self.assembly.clone();
        for f in &mut self.features {
            if skip.contains(&f.uid) {
                continue;
            }
            let component = f.component;
            let deepen = |path: &mut OccurrencePath, geometry: &GeomRef| -> bool {
                let fixed = matches!(
                    geometry,
                    GeomRef::Origin(_)
                        | GeomRef::FixedPlane { .. }
                        | GeomRef::FixedAxis { .. }
                        | GeomRef::FixedPoint(_)
                );
                if fixed && crate::joints::path_component(&assembly, component, &path.0) == Ok(made)
                {
                    path.0.push(occurrence);
                    return true;
                }
                false
            };
            let origin = |o: &mut JointOrigin| deepen(&mut o.occurrence, &o.geometry);
            let mut def = f.def.clone();
            let mut changed = false;
            match &mut def {
                FeatureDef::Joint(j) => {
                    changed |= origin(&mut j.a);
                    changed |= origin(&mut j.b);
                }
                FeatureDef::AsBuiltJoint(j) => {
                    if let Some(o) = j.origin.as_mut() {
                        changed |= origin(o);
                    }
                }
                FeatureDef::ConstraintSet(set) => {
                    for row in &mut set.rows {
                        for o in [&mut row.a, &mut row.b]
                            .into_iter()
                            .chain(row.a2.as_mut())
                            .chain(row.b2.as_mut())
                        {
                            changed |= origin(o);
                        }
                    }
                }
                FeatureDef::BoundLength(b) => {
                    changed |= deepen(&mut b.from.occurrence, &b.from.geometry);
                    changed |= deepen(&mut b.to.occurrence, &b.to.geometry);
                }
                FeatureDef::MotionLink(l) => {
                    for o in l.coupling.origins_mut() {
                        changed |= origin(o);
                    }
                }
                _ => {}
            }
            if changed {
                Arc::make_mut(f).def = def;
            }
        }
    }

    /// A linked design kept with its components read again: everything
    /// made for it updated in place (see the module docs). Adds a line to
    /// `messages` per part gone that stays and per flexible occurrence's
    /// own value dropped.
    pub(super) fn update_link_tree(
        &mut self,
        update: TreeUpdate,
        messages: &mut Vec<String>,
    ) -> Result<(), ModelError> {
        let TreeUpdate {
            component: made,
            digest,
            file,
            mut read,
            envelopes,
        } = update;
        let Some(mut link) = self.assembly.component(made).and_then(|c| c.link.clone()) else {
            return Ok(());
        };
        let Some(mut tree) = link.tree.clone() else {
            return Ok(());
        };
        messages.extend(read.messages.iter().cloned());
        // Standing for the design's one component, or its root again
        // (mitcad#2429): back with its root, the one component it stood
        // for gets one of its own and the references follow (mitcad#2441),
        // before what was made is taken as the link's previous state, so
        // that the update keeps that component as made for it.
        if let Some(top) = self.take_top(made, &mut tree, read.top) {
            let line = self.open_top(made, &mut link, &mut tree, &top, &mut read, &file)?;
            messages.push(line);
        }
        let prior_tree = tree.clone();
        let prior_assembly = self.assembly.clone();
        // The root's own bodies and mount frames.
        super::mounts::update_linked_mounts(self, made, &read.mounts)?;
        self.set_tree_bodies(link.feature, read.root.clone(), &read.root_looks)?;
        for (feature, bodies) in envelopes {
            self.set_tree_bodies(feature, bodies, &[])?;
        }

        // The components: taken again, or new.
        let mut placed = PlacedTree::new(made);
        for record in &mut tree.components {
            record.gone = true;
        }
        for c in &read.components {
            let found = tree.components.iter().position(|t| {
                t.gone
                    && Some(t.component) == c.previous
                    && t.source == c.source
                    && self.assembly.exists(t.component)
                    && self.entry(t.feature).is_some()
            });
            match found {
                Some(i) => {
                    let (component, feature) =
                        (tree.components[i].component, tree.components[i].feature);
                    self.set_tree_bodies(feature, c.bodies.clone(), &c.looks)?;
                    // Renamed in the design: renamed here.
                    if tree.components[i].name != c.name {
                        let name = self.free_component_name(&c.name);
                        if let Some(def) = self.assembly.component_mut(component) {
                            def.name = name;
                        }
                    }
                    self.assembly.set_policy(component, c.policy);
                    tree.components[i].name = c.name.clone();
                    tree.components[i].gone = false;
                    placed.components.insert(c.variant, component);
                }
                None => {
                    let record = self.new_tree_component(c, &file)?;
                    if let Some(previous) = c.previous.and_then(|previous| {
                        prior_tree
                            .components
                            .iter()
                            .find(|r| r.component == previous)
                    }) {
                        let position = self.require(record.feature)?;
                        let after = self.require(previous.feature)? + 1;
                        if after < position {
                            let entry = self.features.remove(position);
                            self.features.insert(after, entry);
                        }
                    }
                    placed.components.insert(c.variant, record.component);
                    tree.components.push(record);
                }
            }
        }

        // The occurrences: placed and grounded again, or new. Shown and
        // see-through stay as this design has them.
        for record in &mut tree.occurrences {
            record.gone = true;
        }
        let mut split = BTreeSet::new();
        let whole = link.parts.is_none() && read.parts.is_none();
        for o in &read.occurrences {
            let component = placed.components[&o.component];
            let parent = placed.components[&o.parent];
            let found = tree.occurrences.iter().position(|t| {
                t.gone
                    && Some(t.occurrence) == o.previous
                    && t.source == o.source
                    && self.assembly.occurrence(t.occurrence).is_some_and(|x| {
                        (x.parent == parent
                            || (whole
                                && prior_tree
                                    .occurrences
                                    .iter()
                                    .filter(|row| row.source == o.source && !row.gone)
                                    .count()
                                    == 1))
                            && read
                                .components
                                .iter()
                                .find(|c| c.variant == o.component)
                                .is_some_and(|c| {
                                    prior_tree.components.iter().any(|old| {
                                        old.component == x.component && old.source == c.source
                                    })
                                })
                    })
            });
            let uid = match found {
                Some(i) => {
                    tree.occurrences[i].gone = false;
                    let uid = tree.occurrences[i].occurrence;
                    if let Some(x) = self.assembly.occurrence_mut(uid) {
                        if x.component != component {
                            split.insert(uid);
                        }
                        x.parent = parent;
                        x.component = component;
                        x.transform = o.transform;
                        x.grounded = o.grounded;
                    }
                    uid
                }
                None => {
                    let uid = self.place_tree_occurrence(component, parent, o);
                    tree.occurrences.push(LinkedOccurrence {
                        source: o.source,
                        occurrence: uid,
                        gone: false,
                    });
                    uid
                }
            };
            placed
                .occurrences
                .insert((o.parent, o.source), (uid, o.component));
        }
        let remap = LinkPathRemap {
            root: made,
            before: &prior_assembly,
            old: &prior_tree,
            new: &tree,
            placed: &placed,
        };
        self.remap_parent_links(&remap, &split)?;

        // The joints, as-built joints and rigid groups: edited in place
        // (their uids stay), new, or gone.
        let mut kept = Vec::new();
        let mut replacements = BTreeMap::<FeatureUid, Vec<FeatureUid>>::new();
        for f in &read.features {
            let Some(def) = tree_feature_def(f, &placed) else {
                continue;
            };
            let component = placed.components[&f.component];
            let same = tree
                .features
                .iter()
                .find(|row| {
                    row.source == f.source
                        && self.entry(row.feature).is_some_and(|e| {
                            e.component == component && e.def.type_name() == def.type_name()
                        })
                })
                .map(|row| row.feature);
            let feature = match same {
                Some(uid) => {
                    self.edit_feature(uid, &def)?;
                    uid
                }
                None => {
                    let feature = self.add_tree_feature(f, component, &def)?;
                    if let Some(previous) = prior_tree
                        .features
                        .iter()
                        .find(|row| row.source == f.source)
                    {
                        let position = self.require(feature)?;
                        let after = self.require(previous.feature)? + 1;
                        if after < position {
                            let entry = self.features.remove(position);
                            self.features.insert(after, entry);
                        }
                    }
                    feature
                }
            };
            placed.features.insert((f.component, f.source), feature);
            let owner = if f.component == Variant::ROOT {
                Some(made)
            } else {
                read.components
                    .iter()
                    .find(|c| c.variant == f.component)
                    .and_then(|c| c.previous)
            };
            if let Some(old) = prior_tree.features.iter().find(|row| {
                row.source == f.source
                    && self
                        .entry(row.feature)
                        .is_some_and(|e| Some(e.component) == owner)
            }) && old.feature != feature
            {
                replacements.entry(old.feature).or_default().push(feature);
            }
            kept.push(LinkedFeature {
                source: f.source,
                feature,
            });
        }
        let gone: BTreeSet<FeatureUid> = tree
            .features
            .iter()
            .map(|f| f.feature)
            .filter(|uid| !kept.iter().any(|k| k.feature == *uid) && self.entry(*uid).is_some())
            .collect();
        let gone_names: BTreeMap<FeatureUid, String> = gone
            .iter()
            .filter_map(|uid| Some((*uid, self.entry(*uid)?.name.clone())))
            .collect();
        let choosing = !crate::assembly::same_parts(read.parts.as_deref(), link.parts.as_deref());
        if !gone.is_empty() {
            if choosing {
                messages.extend(gone_names.iter().filter(|(uid, _)| !replacements.contains_key(uid)).map(|(_, name)| {
                    format!(
                        "{name} was dropped because its parts or related joints were not chosen"
                    )
                }));
            }
            let mut removed = gone.clone();
            removed.extend(self.dependents(&gone));
            self.remove_features(&removed);
        }
        tree.features = kept;
        let remap = LinkPathRemap {
            root: made,
            before: &prior_assembly,
            old: &prior_tree,
            new: &tree,
            placed: &placed,
        };
        self.take_inner_flex(&read, &placed);

        // A flexible occurrence of the linked component keeps its own
        // values of the joints still there, within their limits now; those
        // of joints gone are dropped, with a line.
        let flexible: Vec<OccurrenceUid> = self
            .assembly
            .occurrences
            .iter()
            .filter(|o| {
                o.flexible.is_some()
                    && (o.component == made || prior_assembly.contains(o.component, made))
            })
            .map(|o| o.uid)
            .collect();
        for uid in flexible {
            let name = self.assembly.occurrence_name(uid);
            let Some(occurrence) = self.assembly.occurrence(uid) else {
                continue;
            };
            let scope = occurrence.component;
            let Some(mut flex) = occurrence.flexible.clone() else {
                continue;
            };
            let accessible = flexible_components(&self.assembly, scope);
            for (joint, joint_name) in &gone_names {
                if !replacements.contains_key(joint)
                    && (flex.position.contains_key(joint) || flex.start.contains_key(joint))
                {
                    let why = if choosing {
                        "the chosen parts no longer have the joint".to_owned()
                    } else {
                        format!("{file} no longer has the joint")
                    };
                    messages.push(format!(
                        "{name}: its own value of {joint_name} is dropped, as {why}"
                    ));
                }
            }
            for values in [&mut flex.position, &mut flex.start] {
                for (old, new) in &replacements {
                    if let Some(value) = values.get(old).cloned() {
                        for uid in new {
                            if self
                                .entry(*uid)
                                .is_some_and(|entry| accessible.contains(&entry.component))
                            {
                                values.entry(*uid).or_insert_with(|| value.clone());
                            }
                        }
                    }
                }
            }
            for placement in &mut flex.placements {
                remap.path(scope, &mut placement.path);
            }
            let keep = |joint: &FeatureUid, _: &mut BTreeMap<Motion, f64>| {
                self.entry(*joint).is_some_and(|entry| {
                    !replacements.contains_key(joint) || accessible.contains(&entry.component)
                })
            };
            flex.position.retain(keep);
            flex.start.retain(keep);
            for (joint, values) in flex.position.iter_mut().chain(flex.start.iter_mut()) {
                let limits = self.joint_limits(*joint);
                for (motion, v) in values.iter_mut() {
                    if let Some((min, max)) = limits.get(motion) {
                        *v = v
                            .max(min.unwrap_or(f64::NEG_INFINITY))
                            .min(max.unwrap_or(f64::INFINITY));
                    }
                }
            }
            if let Some(o) = self.assembly.occurrence_mut(uid) {
                o.flexible = Some(flex);
            }
        }
        // The parts the design no longer has: those a feature of this
        // design uses stay where they are, with their saved bodies and a
        // line; the others go, with the components no longer placed.
        let used = self.used_outside(&tree, made);
        let mut dropped = BTreeSet::new();
        for record in tree.occurrences.iter().filter(|t| t.gone) {
            if self.assembly.occurrence(record.occurrence).is_none() {
                continue;
            }
            if used.contains(&record.occurrence) {
                let name = self.assembly.occurrence_name(record.occurrence);
                let why = if choosing {
                    "was not chosen".to_owned()
                } else {
                    format!("is no longer in {file}")
                };
                messages.push(format!(
                    "{name} {why}; it stays where it was, as this design uses it"
                ));
            } else {
                dropped.insert(record.occurrence);
            }
        }
        self.assembly
            .occurrences
            .retain(|o| !dropped.contains(&o.uid));
        let unplaced: BTreeSet<ComponentUid> = tree
            .components
            .iter()
            .map(|t| t.component)
            .filter(|c| {
                self.assembly.exists(*c) && self.assembly.occurrences_of(*c).next().is_none()
            })
            .collect();
        if !unplaced.is_empty() {
            self.delete_components(unplaced);
        }
        let mut orphans = crate::joints::orphaned(self);
        if !orphans.is_empty() {
            orphans.extend(self.dependents(&orphans));
            self.remove_features(&orphans);
        }
        tree.components
            .retain(|t| self.assembly.exists(t.component) && self.entry(t.feature).is_some());
        tree.occurrences
            .retain(|t| self.assembly.occurrence(t.occurrence).is_some());
        tree.features.retain(|t| self.entry(t.feature).is_some());

        if let Some(l) = self
            .assembly
            .component_mut(made)
            .and_then(|c| c.link.as_mut())
        {
            l.digest = digest;
            l.looks = true;
            l.parts = read.parts;
            l.tree = Some(tree);
        }
        Ok(())
    }
}

/// Puts linked designs kept with their components, read again, into a
/// state (`update_links`, `update_link`); the lines they give.
pub(super) fn apply_tree_updates(
    state: &mut DocState,
    updates: Vec<TreeUpdate>,
) -> Result<Vec<String>, ModelError> {
    let mut messages = Vec::new();
    for update in updates {
        state.update_link_tree(update, &mut messages)?;
    }
    Ok(messages)
}

/// A linked design's record of what was made for its components
/// (mitcad#2209) after a copy of the linked component into `design`
/// (Save Component, the links inside the saved one): by the copies'
/// uids, leaving out what was not copied.
pub(super) fn copied_tree(
    tree: &LinkTree,
    source: &DocState,
    design: &DocState,
    copy: &ComponentCopy,
) -> LinkTree {
    let components: BTreeMap<ComponentUid, ComponentUid> = copy
        .occurrences
        .iter()
        .filter_map(|(from, to)| {
            Some((
                source.assembly.occurrence(*from)?.component,
                design.assembly.occurrence(*to)?.component,
            ))
        })
        .collect();
    LinkTree {
        components: tree
            .components
            .iter()
            .filter_map(|c| {
                Some(LinkedComponent {
                    component: *components.get(&c.component)?,
                    feature: *copy.features.get(&c.feature)?,
                    ..c.clone()
                })
            })
            .collect(),
        occurrences: tree
            .occurrences
            .iter()
            .filter_map(|o| {
                Some(LinkedOccurrence {
                    occurrence: *copy.occurrences.get(&o.occurrence)?,
                    ..o.clone()
                })
            })
            .collect(),
        features: tree
            .features
            .iter()
            .filter_map(|f| {
                Some(LinkedFeature {
                    feature: *copy.features.get(&f.feature)?,
                    ..f.clone()
                })
            })
            .collect(),
        envelopes: tree
            .envelopes
            .iter()
            .filter_map(|envelope| {
                Some(crate::assembly::TreeEnvelope {
                    feature: *copy.features.get(&envelope.feature)?,
                    occurrences: envelope
                        .occurrences
                        .iter()
                        .filter_map(|o| copy.occurrences.get(o).copied())
                        .collect(),
                    detail: envelope.detail,
                })
            })
            .collect(),
        // It stands for the design's one component as before (mitcad#2429).
        top: tree.top,
    }
}
