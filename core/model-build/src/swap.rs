// SPDX-License-Identifier: MIT
//! Fasteners swapped in place (mitcad#497; `commands.md`, "BUILD:
//! choosing and swapping fasteners"): the fasteners of a design made from
//! another fastener, each component keeping its place, its joint and its
//! junction, in one undo step. A wood screw or nail becomes another wood
//! fastener, sized from the members it joins unless a row is named, with
//! the warnings `build_connect` gives; a connector's nut or screw becomes
//! another fastener of its kind.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::connect::evaluated;
use crate::parts::{as_input, designation, inherit_appearance};
use crate::place::{PartInput, SourceUse, edit_part, own_frame, refuse};
use crate::prelude::*;
use crate::wood::{Fastening, WoodSize, axis_frame};
use crate::{BuildRole, SourceId};
use mitcad_model::features::{FeatureDef, FrameOverride, GeomRef};
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{DocState, Document, ModelError};

/// The fields of `build_swap_fasteners`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwapInput {
    /// The fasteners to swap (components).
    pub fasteners: Vec<String>,
    /// The fastener they become; its `config` the size (left out: a wood
    /// fastener sized from the members it joins, another its default row).
    pub part: PartInput,
}

/// A fastener swapped: its component, its row, and a wood fastener's
/// diameter and length.
pub type SwappedSize = (ComponentUid, Option<String>, Option<(f64, f64)>);

/// What `build_swap_fasteners` did.
#[derive(Debug, Clone, Default)]
pub struct Swapped {
    pub components: Vec<ComponentUid>,
    /// Each one's row, and a wood fastener's diameter and length.
    pub sizes: Vec<SwappedSize>,
    pub warnings: Vec<String>,
    pub designation: String,
}

/// One fastener swapped: its component, derived part and joint, the row
/// it is made in, the frame its joint holds it by and its name.
struct Swap {
    component: ComponentUid,
    part: FeatureUid,
    joint: FeatureUid,
    config: Option<String>,
    size: Option<WoodSize>,
    own: Option<String>,
    own_override: Option<FrameOverride>,
    name: String,
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait SwapBuild<K: Kernel> {
    /// `build_swap_fasteners`: the fasteners made from `part` where they
    /// are, in one undo step.
    fn build_swap_fasteners(&mut self, input: &SwapInput) -> Result<Swapped, ModelError>;

    /// Where a wood fastener enters the member it is driven from, in the
    /// BUILD host: its joint's frame on that member (`build_connect`'s
    /// `frame_override`).
    fn entry_point(&self, joint: FeatureUid) -> Option<crate::geometry::V3>;
}

impl<K: Kernel> SwapBuild<K> for Document<K> {
    fn build_swap_fasteners(&mut self, input: &SwapInput) -> Result<Swapped, ModelError> {
        if input.fasteners.is_empty() {
            return Err(refuse("name the fasteners to swap (fasteners)"));
        }
        let source = self.fastener_source(&input.part)?;
        let label = source.def.label();
        if source.meta.role != BuildRole::Fastener {
            return Err(refuse(format!("{label} is no fastener")));
        }
        let wood = source.meta.wood.is_some();
        let kind = source
            .meta
            .wood
            .as_ref()
            .map_or("screw", |w| if w.kind == "nail" { "nail" } else { "screw" });
        // The new part's frames, for a nut's or a screw's own frame.
        let mounts = if wood {
            BTreeMap::new()
        } else {
            evaluated(self, &source, &[])?.0
        };
        let own = if wood {
            Some(axis_frame(&source))
        } else {
            own_frame(&source.meta, &mounts, &["slot", "hole", "face"])
                .or_else(|| mounts.keys().next().cloned())
        };
        let mut seen = BTreeSet::new();
        let mut swaps = Vec::new();
        let mut warnings = Vec::new();
        // The junctions' fastenings and sizes, worked out once each.
        let mut sized: BTreeMap<String, Option<(Fastening, WoodSize)>> = BTreeMap::new();
        for text in &input.fasteners {
            let component = self.component_by_ref(text)?;
            if !seen.insert(component) {
                continue;
            }
            let info = self.part_info(component).ok_or_else(|| {
                refuse(format!(
                    "{} is no BUILD part",
                    self.assembly().name(component)
                ))
            })?;
            if info.record.role != BuildRole::Fastener {
                return Err(refuse(format!("{} is no fastener", info.name)));
            }
            let (Some(part), Some(joint)) = (info.part, info.record.joint) else {
                return Err(refuse(format!(
                    "{} has no derived part and joint to keep",
                    info.name
                )));
            };
            // Wood for wood, a connector's nut or screw for another.
            match (info.meta.wood.is_some(), wood) {
                (true, false) => {
                    return Err(refuse(format!(
                        "{label} is no wood screw or nail: {} is driven into timber",
                        info.name
                    )));
                }
                (false, true) => {
                    return Err(refuse(format!(
                        "{label} is a wood fastener: {} is a connector's nut or screw",
                        info.name
                    )));
                }
                _ => {}
            }
            if !wood
                && !info.meta.systems.is_empty()
                && !source.meta.systems.is_empty()
                && !source
                    .meta
                    .systems
                    .iter()
                    .any(|n| info.meta.systems.iter().any(|o| n.fits(o) || o.fits(n)))
            {
                let labels = |s: &[crate::System]| {
                    s.iter().map(|s| s.label()).collect::<Vec<_>>().join(", ")
                };
                return Err(refuse(format!(
                    "the systems do not fit: {label} fits {}, {} is for {}",
                    labels(&source.meta.systems),
                    info.name,
                    labels(&info.meta.systems)
                )));
            }
            let (config, size) = if wood {
                // At its junction: sized from the members (or the row
                // named), with the warnings of its place there.
                let at = info.record.junction.clone();
                let fastening = match &at {
                    Some(id) => {
                        if !sized.contains_key(id) {
                            let found = self
                                .junction_of(id)
                                .ok()
                                .and_then(|j| self.fastening(&j))
                                .map(|f| {
                                    let size = self.wood_size(&source, Some(&f), &mut warnings)?;
                                    Ok::<_, ModelError>((f, size))
                                })
                                .transpose()?;
                            sized.insert(id.clone(), found);
                        }
                        sized.get(id).cloned().flatten()
                    }
                    None => None,
                };
                let size = match &fastening {
                    Some((_, size)) => size.clone(),
                    None => self.wood_size(&source, None, &mut warnings)?,
                };
                if let (Some((fastening, size)), Some(id)) = (&fastening, &at)
                    && let Some(point) = self.entry_point(joint)
                {
                    let i = self.place_in_junction(component, id);
                    let mut used = source.clone();
                    used.config = size.config.clone();
                    let mut driven = size.clone();
                    driven.length += self.wood_seat(&used)?.2;
                    self.fastener_warnings(fastening, &driven, i, point, kind, &mut warnings);
                }
                (size.config.clone(), Some(size))
            } else {
                (input.part.config.clone(), None)
            };
            let (seat, own_override) = if wood && info.record.junction.is_some() {
                let mut used = source.clone();
                used.config = config.clone();
                let (seat, override_frame, _) = self.wood_seat(&used)?;
                (Some(seat), override_frame)
            } else {
                // A connector's screw keeps its seat on the connector's wall
                // (mitcad#1115): its own frame's override stays.
                let kept = match self.feature(joint).map(|e| &e.def) {
                    Some(FeatureDef::Joint(j)) => j.a.frame_override,
                    _ => None,
                };
                (own.clone(), kept)
            };
            let shown = designation(&source.def, config.as_deref(), &BTreeMap::new());
            swaps.push(Swap {
                component,
                part,
                joint,
                config,
                size,
                own: seat,
                own_override,
                name: if shown.is_empty() {
                    label.clone()
                } else {
                    shown
                },
            });
        }
        let active = self.build_host();
        self.state().editable(active)?;
        let shown = swaps
            .first()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| label.clone());
        let undo = format!("Swap {} fastener(s) for {shown}", swaps.len());
        let mut seen_warning = BTreeSet::new();
        warnings.retain(|w| seen_warning.insert(w.clone()));
        let out = Swapped {
            components: swaps.iter().map(|s| s.component).collect(),
            sizes: swaps
                .iter()
                .map(|s| {
                    (
                        s.component,
                        s.config.clone(),
                        s.size.as_ref().map(|z| (z.diameter, z.length)),
                    )
                })
                .collect(),
            warnings,
            designation: shown,
        };
        self.apply(|state| {
            let id = source_id(state, &source);
            for swap in &swaps {
                edit_part(state, swap.part, |def| {
                    def.source = id;
                    def.config = swap.config.clone();
                    def.parameters.clear();
                })?;
                inherit_appearance(state, swap.part, &source.def);
                hold_by(
                    state,
                    swap.joint,
                    swap.part,
                    swap.own.as_deref(),
                    swap.own_override,
                )?;
                if let Some(c) = state.assembly.component_mut(swap.component)
                    && let Some(record) = c.build.as_mut()
                {
                    record.source = Some(id);
                }
                if state.assembly.name(swap.component) != swap.name {
                    let name = state.free_component_name(&swap.name);
                    if let Some(c) = state.assembly.component_mut(swap.component) {
                        c.name = name;
                    }
                }
            }
            Ok((undo, out))
        })
    }

    fn entry_point(&self, joint: FeatureUid) -> Option<crate::geometry::V3> {
        let entry = self.feature(joint)?;
        let FeatureDef::Joint(j) = &entry.def else {
            return None;
        };
        let origin = j.b.frame_override.as_ref()?.origin?;
        let through = mitcad_model::internal::joints::path_component(
            self.assembly(),
            entry.component,
            &j.b.occurrence.0,
        )
        .ok()?;
        Some(self.member(through)?.placement.apply_point(origin))
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait SwapCrateOps<K: Kernel> {
    /// A fastener as a definition keeps it (a frame's, a set's): a source
    /// of the design as its library reference, so that it is found again
    /// when its last fastener went (laid out again).
    fn kept_fastener(&self, part: &PartInput) -> PartInput;

    /// A fastener's number at its junction (from 1): its place among the
    /// junction's fasteners, in the order they were made.
    fn place_in_junction(&self, component: ComponentUid, junction: &str) -> usize;
}

impl<K: Kernel> SwapCrateOps<K> for Document<K> {
    fn kept_fastener(&self, part: &PartInput) -> PartInput {
        let Some(def) = part
            .source
            .and_then(|id| self.state().library_sources.get(id))
        else {
            return part.clone();
        };
        PartInput {
            library: Some(def.library.library.clone()),
            url: def.library.url.clone(),
            rev: Some(def.library.rev.clone()),
            component: Some(def.library.component.clone()),
            config: part.config.clone(),
            source: None,
            parameters: part.parameters.clone(),
        }
    }

    fn place_in_junction(&self, component: ComponentUid, junction: &str) -> usize {
        self.assembly()
            .components
            .iter()
            .filter(|c| {
                c.build.as_ref().is_some_and(|b| {
                    b.role == BuildRole::Fastener && b.junction.as_deref() == Some(junction)
                })
            })
            .position(|c| c.uid == component)
            .map_or(1, |i| i + 1)
    }
}

/// The design's source of `source`: added the first time.
fn source_id(state: &mut DocState, source: &SourceUse) -> SourceId {
    if !source.new {
        return source.def.id;
    }
    let library = &source.def.library;
    if let Some(id) = state
        .library_sources
        .find(&library.library, &library.component, &library.rev)
    {
        return id;
    }
    let mut def = source.def.clone();
    def.id = state.library_sources.next_id();
    let id = def.id;
    state.library_sources.0.push(def);
    id
}

/// A fastener's joint holding it by its frame `own` (its design's origin
/// when None) of its derived part `part`.
fn hold_by(
    state: &mut DocState,
    joint: FeatureUid,
    part: FeatureUid,
    own: Option<&str>,
    own_override: Option<FrameOverride>,
) -> Result<(), ModelError> {
    let entry = state
        .entry(joint)
        .ok_or(ModelError::NoSuchFeature(joint))?
        .clone();
    let FeatureDef::Joint(mut def) = as_input(&entry.def, &state.parameters) else {
        return Err(refuse(format!("{} is no joint", entry.name)));
    };
    let geometry = match own {
        Some(name) => GeomRef::Mount {
            feature: part,
            name: name.to_owned(),
        },
        None => GeomRef::Origin(mitcad_model::datum::OriginDatum::Xy),
    };
    if def.a.geometry == geometry && def.a.frame_override == own_override {
        return Ok(());
    }
    def.a.geometry = geometry;
    def.a.frame_override = own_override;
    state.edit_feature(joint, &FeatureDef::Joint(def))?;
    Ok(())
}
