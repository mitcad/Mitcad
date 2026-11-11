// SPDX-License-Identifier: MIT
//! Design policy (mitcad#220): what may be created in, and linked to and
//! from, the root and each component. Every owner (the root counts as
//! one) has nine settings of its own; nothing is inherited through the
//! hierarchy.
//!
//! - Containment: sketches, modelling features and components placed
//!   directly in the owner. Sketches are governed by `sketches` only;
//!   components (child occurrences, also those a feature makes) by
//!   `components`. Assembly features that arrange the owner's occurrences
//!   (joints, rigid groups, assembly constraints, occurrence moves and
//!   captured positions, patterns of occurrences, BUILD's bound lengths
//!   measured between them) and Component from
//!   Bodies (whose component counts under `components`) are not
//!   modelling features.
//! - Sketch attachment: a sketch whose plane is another owner's face or
//!   plane (its `plane_link`) needs both its own owner's
//!   `attach_to_others` and the other owner's `attachments_from_others`.
//!   An owner's first sketch (no sketch or modelling feature of the owner
//!   precedes it in the timeline) may also attach with its owner's
//!   `first_sketch_attaches` and the other owner's
//!   `first_sketch_attachments_from_others` (mitcad#293): either setting of
//!   a side allows that side.
//! - References: every other link to another owner's geometry (linked
//!   projections, extrude objects, combine and split tools, sweep bodies,
//!   construction planes on faces) needs the consumer owner's
//!   `reference_others` and the source owner's `references_from_others`.
//!   An unlinked projection keeps no link and is not a reference. Joints
//!   and assembly constraints connect occurrences and are not governed by
//!   the reference settings.
//!
//! A policy governs new actions only. Content that breaks it is a
//! violation, listed and counted, never removed or detached; the model
//! keeps evaluating it. Commands refuse a change that adds a violation
//! ([`new_violations`]); one that keeps or removes violations is allowed,
//! so existing objects stay editable.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::assembly::Assembly;
use crate::features::{FeatureDef, FeatureEntry};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::links::{OccurrenceLink, OccurrenceLinkExt};

/// The nine settings of one owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    /// Sketches may be created in or moved into the owner.
    pub sketches: bool,
    /// Modelling features may be created in or moved into the owner.
    pub features: bool,
    /// Components may be created, inserted or moved directly under it.
    pub components: bool,
    /// Its sketches may attach to another owner's face or plane.
    pub attach_to_others: bool,
    /// Other owners' sketches may attach to its faces and planes.
    pub attachments_from_others: bool,
    /// Its first sketch may attach to another owner's face or plane, also
    /// when `attach_to_others` is off (mitcad#293). A stored policy without
    /// it reads it as on.
    #[serde(default = "on")]
    pub first_sketch_attaches: bool,
    /// Other owners' first sketches may attach to its faces and planes,
    /// also when `attachments_from_others` is off (mitcad#293). A stored
    /// policy without it reads it as on.
    #[serde(default = "on")]
    pub first_sketch_attachments_from_others: bool,
    /// Its sketches and features may reference other owners' geometry.
    pub reference_others: bool,
    /// Other owners may reference its geometry.
    pub references_from_others: bool,
}

impl Policy {
    /// The general defaults, the Hybrid preset: everything allowed
    /// (mitcad#421). What an owner without a stored policy has, a new
    /// design's root and Component1, and a new component.
    pub const GENERAL: Self = Self {
        sketches: true,
        features: true,
        components: true,
        attach_to_others: true,
        attachments_from_others: true,
        first_sketch_attaches: true,
        first_sketch_attachments_from_others: true,
        reference_others: true,
        references_from_others: true,
    };
    /// Holds sketches and features, no child components; sketch
    /// attachment refused both ways except a first sketch's, references
    /// allowed.
    pub const PART: Self = Self {
        sketches: true,
        features: true,
        components: false,
        attach_to_others: false,
        attachments_from_others: false,
        first_sketch_attaches: true,
        first_sketch_attachments_from_others: true,
        reference_others: true,
        references_from_others: true,
    };
    /// Holds components, no sketches or modelling features of its own;
    /// takes other components' first sketches.
    pub const ASSEMBLY: Self = Self {
        sketches: false,
        features: false,
        components: true,
        first_sketch_attaches: false,
        ..Self::PART
    };

    /// The policy of a new component that features move into from an
    /// owner with this policy: containment allowed (the general defaults),
    /// the link settings of the owner they came from, so that their links
    /// and the links to their geometry stay as allowed as they were.
    pub fn for_moved_content(&self) -> Self {
        Self {
            attach_to_others: self.attach_to_others,
            attachments_from_others: self.attachments_from_others,
            first_sketch_attaches: self.first_sketch_attaches,
            first_sketch_attachments_from_others: self.first_sketch_attachments_from_others,
            reference_others: self.reference_others,
            references_from_others: self.references_from_others,
            ..Self::GENERAL
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Self::GENERAL
    }

    /// A preset by name: `hybrid` (the general defaults; `defaults`, its
    /// name before mitcad#421, is an alias), `part` or `assembly`.
    pub fn preset(name: &str) -> Option<Self> {
        match name {
            "hybrid" | "defaults" => Some(Self::GENERAL),
            "part" => Some(Self::PART),
            "assembly" => Some(Self::ASSEMBLY),
            _ => None,
        }
    }

    /// The preset this policy is, if any. Without sketches,
    /// `first_sketch_attaches` governs nothing and is not compared: an
    /// Assembly stored before the setting existed reads it as on and is
    /// still one (mitcad#293).
    pub fn preset_name(&self) -> Option<&'static str> {
        [
            ("assembly", Self::ASSEMBLY),
            ("part", Self::PART),
            ("hybrid", Self::GENERAL),
        ]
        .into_iter()
        .find(|(_, p)| {
            p == self
                || (!self.sketches
                    && *p
                        == Self {
                            first_sketch_attaches: p.first_sketch_attaches,
                            ..*self
                        })
        })
        .map(|(name, _)| name)
    }

    pub fn allows(&self, setting: Setting) -> bool {
        match setting {
            Setting::Sketches => self.sketches,
            Setting::Features => self.features,
            Setting::Components => self.components,
            Setting::AttachToOthers => self.attach_to_others,
            Setting::AttachmentsFromOthers => self.attachments_from_others,
            Setting::FirstSketchAttaches => self.first_sketch_attaches,
            Setting::FirstSketchAttachmentsFromOthers => self.first_sketch_attachments_from_others,
            Setting::ReferenceOthers => self.reference_others,
            Setting::ReferencesFromOthers => self.references_from_others,
        }
    }
}

/// The serde default of the first-sketch settings (mitcad#293).
fn on() -> bool {
    true
}

impl Default for Policy {
    fn default() -> Self {
        Self::GENERAL
    }
}

/// One of the nine settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Setting {
    Sketches,
    Features,
    Components,
    AttachToOthers,
    AttachmentsFromOthers,
    FirstSketchAttaches,
    FirstSketchAttachmentsFromOthers,
    ReferenceOthers,
    ReferencesFromOthers,
}

impl Setting {
    pub const ALL: [Self; 9] = [
        Self::Sketches,
        Self::Features,
        Self::Components,
        Self::AttachToOthers,
        Self::AttachmentsFromOthers,
        Self::FirstSketchAttaches,
        Self::FirstSketchAttachmentsFromOthers,
        Self::ReferenceOthers,
        Self::ReferencesFromOthers,
    ];

    /// The field name of [`Policy`] in files and commands.
    pub fn key(self) -> &'static str {
        match self {
            Self::Sketches => "sketches",
            Self::Features => "features",
            Self::Components => "components",
            Self::AttachToOthers => "attach_to_others",
            Self::AttachmentsFromOthers => "attachments_from_others",
            Self::FirstSketchAttaches => "first_sketch_attaches",
            Self::FirstSketchAttachmentsFromOthers => "first_sketch_attachments_from_others",
            Self::ReferenceOthers => "reference_others",
            Self::ReferencesFromOthers => "references_from_others",
        }
    }
}

/// What breaks a setting: a feature or an occurrence placed in the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolicyObject {
    Feature(FeatureUid),
    Occurrence(OccurrenceUid),
}

/// One offending item: for containment the object directly in `owner`;
/// for attachments and references the consumer (a sketch or feature) and
/// `other`, the owner on the other side of the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Violation {
    pub owner: ComponentUid,
    pub setting: Setting,
    pub object: PolicyObject,
    pub other: Option<ComponentUid>,
}

/// The containment setting a feature is created under, if any.
pub fn containment<P>(def: &FeatureDef<P>) -> Option<Setting> {
    match def {
        FeatureDef::Sketch(_) => Some(Setting::Sketches),
        FeatureDef::ComponentFromBodies(_)
        | FeatureDef::MoveOccurrence(_)
        | FeatureDef::CapturePosition(_)
        | FeatureDef::Joint(_)
        | FeatureDef::AsBuiltJoint(_)
        | FeatureDef::JointOrigin(_)
        | FeatureDef::RigidGroup(_)
        | FeatureDef::ConstraintSet(_)
        | FeatureDef::PatternAtPoints(_)
        | FeatureDef::BoundLength(_) => None,
        _ => Some(Setting::Features),
    }
}

/// Whether a link attaches a sketch (its plane) or references geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkKind {
    Attachment,
    Reference,
}

/// Whether a feature links to other components' geometry (the recompute's
/// workers leave it to the recompute, mitcad#842).
pub(crate) fn links_other_components(def: &FeatureDef) -> bool {
    !links(def).is_empty()
}

/// The links of a feature to other components' geometry.
fn links(def: &FeatureDef) -> Vec<(LinkKind, &OccurrenceLink)> {
    let references: Vec<&OccurrenceLink> = match def {
        FeatureDef::Sketch(s) => s
            .projections
            .iter()
            .filter_map(|p| p.link.as_ref())
            .collect(),
        // A 3D sketch's linked reference records (mitcad#1543), its points'
        // faces too (mitcad#1549).
        FeatureDef::SpatialSketch(s) => s
            .attachments()
            .map(|(_, _, source)| source)
            .chain(s.reference_curves().map(|(_, source)| source))
            .chain(s.face_records().map(|(_, _, source)| source))
            .filter_map(|source| source.link())
            .collect(),
        FeatureDef::Combine(c) => c.links().map(|(_, link)| link).collect(),
        FeatureDef::Sweep(s) => s.body_links.values().collect(),
        FeatureDef::SplitBody(s) => s.tool_links.values().collect(),
        FeatureDef::Extrude(x) => x.object_links.values().collect(),
        FeatureDef::Revolve(r) => r.object_links.values().collect(),
        FeatureDef::ConstructionPlane(p) => {
            p.links.values().chain(p.datum_links.values()).collect()
        }
        FeatureDef::ConstructionAxis(a) => a.links.values().chain(a.datum_links.values()).collect(),
        FeatureDef::ConstructionPoint(p) => {
            p.links.values().chain(p.datum_links.values()).collect()
        }
        FeatureDef::Derive(d) => d.links.values().collect(),
        FeatureDef::BodyTransfer(t) => vec![&t.link],
        // A pulley's or gear's seat on another component's shaft (mitcad#578).
        FeatureDef::Pulley(_) | FeatureDef::Gear(_) | FeatureDef::PrintableBearing(_) => {
            def.seat_link().map(|(_, link)| link).into_iter().collect()
        }
        _ => Vec::new(),
    };
    let mut out: Vec<(LinkKind, &OccurrenceLink)> = references
        .into_iter()
        .map(|link| (LinkKind::Reference, link))
        .collect();
    if let FeatureDef::Sketch(s) = def
        && let Some(link) = &s.plane_link
    {
        out.push((LinkKind::Attachment, link));
    }
    out
}

/// Whether a feature counts for an owner's first sketch: a sketch or a
/// modelling feature before it makes a sketch not its first (mitcad#293).
pub(crate) fn starts_owner<P>(def: &FeatureDef<P>) -> bool {
    matches!(
        containment(def),
        Some(Setting::Sketches | Setting::Features)
    )
}

/// The setting of `policy` that refuses one side of a link governed by
/// `setting` (an outgoing or incoming attachment or reference), if any. A
/// first sketch's attachment is also allowed by the side's first-sketch
/// setting (mitcad#293); when both are off, that one is named.
pub(crate) fn refused(policy: &Policy, setting: Setting, first: bool) -> Option<Setting> {
    if policy.allows(setting) {
        return None;
    }
    let first_sketch = match setting {
        Setting::AttachToOthers => Some(Setting::FirstSketchAttaches),
        Setting::AttachmentsFromOthers => Some(Setting::FirstSketchAttachmentsFromOthers),
        _ => None,
    };
    match first_sketch.filter(|_| first) {
        Some(s) if policy.allows(s) => None,
        Some(s) => Some(s),
        None => Some(setting),
    }
}

/// Every violation of a design's content under the policies `policy_of`
/// gives, one entry per offending link (a consumer with two links to the
/// same owner is listed twice; [`unique`] lists it once).
pub(crate) fn violations(
    assembly: &Assembly,
    features: &[std::sync::Arc<FeatureEntry>],
    policy_of: &dyn Fn(ComponentUid) -> Policy,
) -> Vec<Violation> {
    let mut out = Vec::new();
    // Owners with a sketch or modelling feature so far: a sketch of an
    // owner not among them is its first (mitcad#293).
    let mut started = BTreeSet::new();
    for f in features {
        let owner = f.component;
        let own = policy_of(owner);
        let first = !started.contains(&owner);
        if starts_owner(&f.def) {
            started.insert(owner);
        }
        if let Some(setting) = containment(&f.def)
            && !own.allows(setting)
        {
            out.push(Violation {
                owner,
                setting,
                object: PolicyObject::Feature(f.uid),
                other: None,
            });
        }
        for (kind, link) in links(&f.def) {
            let Ok(source) = link.source_component(assembly) else {
                continue;
            };
            if source == owner {
                continue;
            }
            let (outgoing, incoming) = match kind {
                LinkKind::Attachment => (Setting::AttachToOthers, Setting::AttachmentsFromOthers),
                LinkKind::Reference => (Setting::ReferenceOthers, Setting::ReferencesFromOthers),
            };
            let object = PolicyObject::Feature(f.uid);
            if let Some(setting) = refused(&own, outgoing, first) {
                out.push(Violation {
                    owner,
                    setting,
                    object,
                    other: Some(source),
                });
            }
            if let Some(setting) = refused(&policy_of(source), incoming, first) {
                out.push(Violation {
                    owner: source,
                    setting,
                    object,
                    other: Some(owner),
                });
            }
        }
    }
    for o in &assembly.occurrences {
        if !policy_of(o.parent).components {
            out.push(Violation {
                owner: o.parent,
                setting: Setting::Components,
                object: PolicyObject::Occurrence(o.uid),
                other: None,
            });
        }
    }
    out
}

/// Each offending object once per owner and setting, in order.
pub fn unique(violations: &[Violation]) -> Vec<(ComponentUid, Setting, PolicyObject)> {
    let set: BTreeSet<_> = violations
        .iter()
        .map(|v| (v.owner, v.setting, v.object))
        .collect();
    set.into_iter().collect()
}

/// The violations `after` has more of than `before` (counted per owner,
/// setting, object and other owner, so that a new link of a consumer that
/// already broke a setting counts too).
pub(crate) fn new_violations(before: &[Violation], after: &[Violation]) -> Vec<Violation> {
    let mut counts: BTreeMap<Violation, i64> = BTreeMap::new();
    for v in before {
        *counts.entry(*v).or_default() -= 1;
    }
    let mut out = Vec::new();
    for v in after {
        let n = counts.entry(*v).or_default();
        *n += 1;
        if *n > 0 && !out.contains(v) {
            out.push(*v);
        }
    }
    out
}
