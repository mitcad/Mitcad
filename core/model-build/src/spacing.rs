// SPDX-License-Identifier: MIT
//! Spacing sets (mitcad#478; `commands.md`, "BUILD: timber construction",
//! *Spacing sets*): a member repeated at a centre distance (studs, joists)
//! or as a board row with a gap (decks, fences, cladding), along a member's
//! side or along a direction over a span; kept as one set that is edited
//! and laid out again as one.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::geometry::{V3, add, dot, inverse, scale, unit, x_axis};
use crate::place::{
    PlaceInput, Placed, Plan, SideFrom, Spot, attach_part, current_bounds, detach, edit_part,
    make_part, refuse, value_in,
};
use crate::prelude::*;
use mitcad_model::expr::Unit;
use mitcad_model::features::derived_part::PartCut;
use mitcad_model::features::{FeatureDef, ValueInput};
use mitcad_model::ids::ComponentUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::{DocState, Document, ModelError};

/// What a set's members are spaced by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingMode {
    /// `distance` is the centre distance.
    #[default]
    Centre,
    /// `distance` is the gap between boards.
    Gap,
}

/// How a board row ends at its span.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingFit {
    /// The last board is ripped to what is left.
    #[default]
    Cut,
    /// The gaps are widened so that the last board ends at the span.
    Spread,
}

/// `build_place`'s and `build_edit`'s `spacing`: every field optional, an
/// edit's fields replacing the set's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpacingInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SpacingMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<ValueInput>,
    /// A fixed count; 0 (an edit's) goes back to the count from the span.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// On a side: the first member's near face from end A.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<ValueInput>,
    /// On a side: the last member's far face from end A (end B when left
    /// out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<ValueInput>,
    /// Otherwise: the direction the set runs, in the host's coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub along: Option<V3>,
    /// Otherwise: the span along it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<ValueInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<SpacingFit>,
    // Fasteners (mitcad#497).
    /// The wood fastener each member is fastened to its hosts with (its
    /// `config` a chosen size, else sized from the members); none: no
    /// fasteners.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fastener: Option<crate::PartInput>,
    /// How many at each junction (default 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fastener_count: Option<u32>,
}

impl SpacingInput {
    /// This set's fields with an edit's over them.
    pub fn merged(&self, edit: &SpacingInput) -> SpacingInput {
        let pick = |a: &Option<ValueInput>, b: &Option<ValueInput>| b.clone().or_else(|| a.clone());
        SpacingInput {
            mode: edit.mode.or(self.mode),
            distance: pick(&self.distance, &edit.distance),
            count: match edit.count {
                Some(0) => None,
                Some(n) => Some(n),
                None => self.count,
            },
            from: pick(&self.from, &edit.from),
            to: pick(&self.to, &edit.to),
            along: edit.along.or(self.along),
            length: pick(&self.length, &edit.length),
            fit: edit.fit.or(self.fit),
            fastener: edit.fastener.clone().or_else(|| self.fastener.clone()),
            fastener_count: edit.fastener_count.or(self.fastener_count),
        }
    }
}

/// What the lead member's record keeps of its set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SetDef {
    pub spacing: SpacingInput,
    /// The first member's placement, its part a source of the design.
    pub base: PlaceInput,
    pub count: usize,
}

/// Where the members' near faces go from the first one's (along the set),
/// and the last one's width when it is ripped: `span` from the first
/// member's near face to the last one's far face, `w` a member's width
/// along the set; `cover` the width a board that joins the next covers
/// (mitcad#477), its row laid edge to edge.
pub(crate) fn positions(
    mode: SpacingMode,
    distance: Option<f64>,
    count: Option<u32>,
    fit: SpacingFit,
    span: f64,
    w: f64,
    cover: Option<f64>,
) -> Result<(Vec<f64>, Option<f64>), String> {
    const TOL: f64 = 1e-6;
    if w.is_nan() || w <= 0.0 {
        return Err("the member has no width along the set".to_owned());
    }
    if !(span.is_finite() && span > 0.0) {
        return Err(format!(
            "the set's span must be greater than zero, got {span} mm"
        ));
    }
    if span < w - TOL {
        return Err(format!(
            "the span of {} mm is narrower than one member ({} mm)",
            mitcad_model::internal::build::round(span),
            mitcad_model::internal::build::round(w)
        ));
    }
    let even = |n: usize, free: f64| -> Vec<f64> {
        if n <= 1 {
            vec![0.0]
        } else {
            (0..n).map(|i| free * i as f64 / (n - 1) as f64).collect()
        }
    };
    let free = span - w;
    match mode {
        SpacingMode::Centre => {
            if let Some(n) = count.filter(|n| *n > 0) {
                return Ok((even(n as usize, free), None));
            }
            let d = distance
                .filter(|d| *d > 0.0)
                .ok_or("the center distance must be greater than zero")?;
            if free <= TOL {
                return Ok((vec![0.0], None));
            }
            let n = ((free / d) - TOL).ceil() as usize + 1;
            let mut out: Vec<f64> = (0..n - 1).map(|i| i as f64 * d).collect();
            out.push(free);
            Ok((out, None))
        }
        SpacingMode::Gap => {
            let gap = distance.unwrap_or(0.0);
            if gap < 0.0 {
                return Err("the gap must not be negative".to_owned());
            }
            // Boards that join (mitcad#477): edge to edge, each the cover
            // width on from the last, no gap; the last ripped.
            if let Some(c) = cover.filter(|c| *c > TOL && *c <= w + TOL) {
                let n = match count.filter(|n| *n > 0) {
                    Some(n) => {
                        let n = n as usize;
                        if (n - 1) as f64 * c + w > span + TOL {
                            return Err(format!(
                                "{n} boards do not fit in {} mm",
                                mitcad_model::internal::build::round(span)
                            ));
                        }
                        return Ok(((0..n).map(|i| i as f64 * c).collect(), None));
                    }
                    None => (((span - (w - c)) / c - TOL).ceil().max(1.0)) as usize,
                };
                let out: Vec<f64> = (0..n).map(|i| i as f64 * c).collect();
                let last = span - (n - 1) as f64 * c;
                let rip = (last < w - TOL).then_some(last);
                return Ok((out, rip));
            }
            if let Some(n) = count.filter(|n| *n > 0) {
                let n = n as usize;
                if n as f64 * w > span + TOL {
                    return Err(format!(
                        "{n} boards do not fit in {} mm",
                        mitcad_model::internal::build::round(span)
                    ));
                }
                return Ok((even(n, free), None));
            }
            let step = w + gap;
            match fit {
                SpacingFit::Cut => {
                    let n = ((span / step) - TOL).ceil().max(1.0) as usize;
                    let out: Vec<f64> = (0..n).map(|i| i as f64 * step).collect();
                    let last = span - (n - 1) as f64 * step;
                    let rip = (last < w - TOL).then_some(last);
                    Ok((out, rip))
                }
                SpacingFit::Spread => {
                    let n = (((span + gap) / step) + TOL).floor().max(1.0) as usize;
                    Ok((even(n, free), None))
                }
            }
        }
    }
}

/// A set worked out: each member's placement and plan, and the last
/// member's rip.
pub struct SetPlan {
    pub plans: Vec<Plan>,
    pub rip: Option<PartCut<ValueInput>>,
    /// What the set notes (a gap boards that join do not keep).
    pub warnings: Vec<String>,
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait SpacingBuild<K: Kernel> {
    /// Plans a set: the first member as `base` places it, the others along
    /// the side it is on or along `spacing.along`.
    fn plan_set(&self, base: &PlaceInput, spacing: &SpacingInput) -> Result<SetPlan, ModelError>;

    /// The wood fasteners a set's members are fastened to their hosts with
    /// (mitcad#497): of every junction of a member with a part outside the
    /// set, the first fastener (which takes the others with it).
    fn set_fasteners(&self, lead: ComponentUid) -> Vec<ComponentUid>;
}

impl<K: Kernel> SpacingBuild<K> for Document<K> {
    fn plan_set(&self, base: &PlaceInput, spacing: &SpacingInput) -> Result<SetPlan, ModelError> {
        let mut base = base.clone();
        base.spacing = None;
        let first = self.plan_place(&base)?;
        let mode = spacing.mode.unwrap_or_default();
        let distance = match &spacing.distance {
            Some(v) => Some(value_in(self, v, Unit::MM)?),
            None => None,
        };
        let fit = spacing.fit.unwrap_or_default();
        let mm = |v: &Option<ValueInput>| -> Result<Option<f64>, ModelError> {
            v.as_ref().map(|v| value_in(self, v, Unit::MM)).transpose()
        };
        // Boards that join (mitcad#477) laid across their width: edge to
        // edge, the step their cover width.
        let cover = |along_host: V3| -> Option<f64> {
            let c = first.source.meta.joined_cover()?;
            let local = inverse(&first.placement).apply_vector(along_host);
            (mode == SpacingMode::Gap && local[0].abs() > 1.0 - 1e-6).then_some(c)
        };
        let side = match &first.spot {
            Spot::Joint(j) if first.place == "side" => j.side.clone().map(|s| (s, j.frame_b)),
            _ => None,
        };
        let mut inputs = Vec::new();
        let (along_host, rip_width) = match side {
            Some((s, frame_b)) => {
                let w = s.far - s.near;
                let length = s
                    .length
                    .ok_or_else(|| refuse("the member it stands on has no length"))?;
                // The base placement's typed offset (the panel's Offset and
                // From, mitcad#492) places the set where it places the
                // first member: from end A (or the middle) the first one's
                // near face, from end B the last one's far face.
                let typed = base
                    .at
                    .as_ref()
                    .and_then(|a| a.offset.as_ref().map(|_| a.from));
                let from = match mm(&spacing.from)? {
                    Some(v) => v,
                    None if typed.is_some_and(|f| f != Some(SideFrom::B)) => s.offset,
                    None => 0.0,
                };
                let to = match mm(&spacing.to)? {
                    Some(v) => v,
                    None if typed == Some(Some(SideFrom::B)) => s.offset + w,
                    None => length,
                };
                let (offsets, rip) = positions(
                    mode,
                    distance,
                    spacing.count,
                    fit,
                    to - from,
                    w,
                    cover(x_axis(&frame_b)),
                )
                .map_err(refuse)?;
                let n = offsets.len();
                for (i, o) in offsets.iter().enumerate() {
                    let mut input = base.clone();
                    let mut at = input.at.clone().unwrap_or_default();
                    let near = from + o;
                    if i + 1 == n
                        && n > 1
                        && spacing.to.is_none()
                        && (near + w - length).abs() < 1e-6
                    {
                        at.from = Some(SideFrom::B);
                        at.offset = Some(ValueInput::Number(0.0));
                    } else {
                        at.from = Some(SideFrom::A);
                        at.offset = Some(ValueInput::Number((near * 1000.0).round() / 1000.0));
                    }
                    input.at = Some(at);
                    inputs.push(input);
                }
                (x_axis(&frame_b), rip)
            }
            None => {
                let along = spacing.along.and_then(unit).ok_or_else(|| {
                    refuse("a set that is not on a side needs its direction (along)")
                })?;
                let span = mm(&spacing.length)?
                    .ok_or_else(|| refuse("a set that is not on a side needs its span (length)"))?;
                let bounds = first
                    .bounds
                    .ok_or_else(|| refuse("the member has no size"))?;
                let (lo, hi) = bounds
                    .corners()
                    .map(|c| dot(first.placement.apply_point(c), along))
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                        (a.min(v), b.max(v))
                    });
                let (offsets, rip) = positions(
                    mode,
                    distance,
                    spacing.count,
                    fit,
                    span,
                    hi - lo,
                    cover(along),
                )
                .map_err(refuse)?;
                for o in offsets {
                    let mut input = base.clone();
                    let mut at = input.at.clone().unwrap_or_default();
                    at.point = Some(add(at.point.unwrap_or([0.0; 3]), scale(along, o)));
                    input.at = Some(at);
                    if let Some(to) = &mut input.to {
                        to.point = add(to.point, scale(along, o));
                    }
                    inputs.push(input);
                }
                (along, rip)
            }
        };
        let mut plans = Vec::new();
        for (i, input) in inputs.iter_mut().enumerate() {
            if i > 0 {
                input.name = None;
            }
            plans.push(self.plan_place(input)?);
        }
        // The last board ripped to what is left: a box beyond that width
        // along the set, in its coordinates.
        let rip = match (rip_width, plans.last()) {
            (Some(r), Some(plan)) => {
                let b = plan
                    .bounds
                    .ok_or_else(|| refuse("the member has no size"))?;
                let local = inverse(&plan.placement).apply_vector(along_host);
                let k = if local[0].abs() >= local[1].abs() {
                    0
                } else {
                    1
                };
                let mut origin = [b.min[0] - 1.0, b.min[1] - 1.0, b.min[2] - 1.0];
                let mut size = [
                    b.max[0] - b.min[0] + 2.0,
                    b.max[1] - b.min[1] + 2.0,
                    b.max[2] - b.min[2] + 2.0,
                ];
                let cut = b.max[k] - b.min[k] - r;
                if local[k] >= 0.0 {
                    origin[k] = b.min[k] + r;
                } else {
                    origin[k] = b.min[k] - 1.0;
                }
                size[k] = cut + 1.0;
                Some(PartCut {
                    kind: "rip".to_owned(),
                    with: None,
                    junction: None,
                    origin: origin.map(|v| ValueInput::Number(tidy(v))),
                    x: crate::geometry::X,
                    y: crate::geometry::Y,
                    size: size.map(|v| ValueInput::Number(tidy(v))),
                    depth: None,
                })
            }
            _ => None,
        };
        // Boards that join keep no gap (mitcad#477).
        let mut warnings = Vec::new();
        if cover(along_host).is_some() && distance.is_some_and(|d| d > 1e-9) {
            warnings.push(format!(
                "{} joins edge to edge: laid without the gap of {} mm",
                first.source.def.label(),
                mitcad_model::internal::build::round(distance.unwrap_or(0.0))
            ));
        }
        Ok(SetPlan {
            plans,
            rip,
            warnings,
        })
    }

    fn set_fasteners(&self, lead: ComponentUid) -> Vec<ComponentUid> {
        let members: BTreeSet<ComponentUid> = self.set_members(lead).into_iter().collect();
        self.assembly()
            .components
            .iter()
            .filter(|c| {
                c.build.as_ref().is_some_and(|b| {
                    b.role == crate::BuildRole::Fastener
                        && b.added_by.is_none()
                        && b.junction.as_deref().is_some_and(|j| {
                            let sides = junction_components(j);
                            sides.len() == 2
                                && sides.iter().filter(|c| members.contains(c)).count() == 1
                        })
                })
            })
            .map(|c| c.uid)
            .collect()
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait SpacingCrateOps<K: Kernel> {
    /// Places a spacing set (`build_place` with `spacing`).
    fn build_place_set(&mut self, input: &PlaceInput) -> Result<Placed, ModelError>;

    /// Fastens each member of a set to its hosts (mitcad#497): `count`
    /// fasteners of `part` (default 2) at every junction of a member with a
    /// part outside the set that has no connector. Returns how many
    /// junctions got them.
    fn fasten_set(
        &mut self,
        lead: ComponentUid,
        part: &crate::PartInput,
        count: Option<u32>,
        warnings: &mut Vec<String>,
    ) -> usize;

    /// Edits a spacing set as one (`build_edit` with `spacing`): the set's
    /// fields replaced by the edit's, its member size by `config`; the
    /// members laid out anew. Returns what changed.
    fn edit_set(
        &mut self,
        component: ComponentUid,
        edit: &SpacingInput,
        config: Option<&String>,
    ) -> Result<Vec<String>, ModelError>;
}

impl<K: Kernel> SpacingCrateOps<K> for Document<K> {
    fn build_place_set(&mut self, input: &PlaceInput) -> Result<Placed, ModelError> {
        let mut spacing = input.spacing.clone().unwrap_or_default();
        spacing.fastener = spacing.fastener.map(|f| self.kept_fastener(&f));
        let mut set = self.plan_set(input, &spacing)?;
        let active = self.build_host();
        self.state().editable(active)?;
        let first = &set.plans[0];
        let count = set.plans.len();
        let designation = if first.designation.is_empty() {
            first.source.def.label()
        } else {
            first.designation.clone()
        };
        // Later members use the source the first one adds.
        let id = first.source.def.id;
        let config = first.source.config.clone();
        for plan in set.plans.iter_mut().skip(1) {
            plan.source.new = plan.source.new && plan.source.def.id != id;
        }
        let mut base = input.clone();
        base.spacing = None;
        base.part = crate::PartInput {
            source: Some(id),
            config,
            ..crate::PartInput::default()
        };
        let def = SetDef {
            spacing,
            base,
            count,
        };
        let label = format!("Place {designation} x{count}");
        let fastener = def.spacing.fastener.clone();
        let fastener_count = def.spacing.fastener_count;
        let depth = self.undo_depth();
        let mut placed = self.apply(|state| {
            let mut placed: Option<Placed> = None;
            let mut members = Vec::new();
            for plan in &set.plans {
                let made = make_part(state, plan, active)?;
                members.push(made.component.expect("made"));
                match &mut placed {
                    None => placed = Some(made),
                    Some(p) => {
                        p.features.extend(made.features);
                        p.warnings.extend(made.warnings);
                    }
                }
            }
            let lead = members[0];
            mark_set(state, &members, lead, &def)?;
            if let (Some(rip), Some(last)) = (&set.rip, members.last()) {
                add_rip(state, *last, rip.clone())?;
            }
            let mut placed = placed.expect("one member at least");
            placed.set = members;
            placed.warnings.extend(set.warnings.iter().cloned());
            Ok((label.clone(), placed))
        })?;
        // Each member fastened to its hosts (mitcad#497), in the same step.
        if let (Some(part), Some(lead)) = (&fastener, placed.set.first().copied()) {
            let mut warnings = Vec::new();
            self.fasten_set(lead, part, fastener_count, &mut warnings);
            placed.warnings.extend(warnings);
            self.merge_undo(depth, &label);
        }
        Ok(placed)
    }

    fn fasten_set(
        &mut self,
        lead: ComponentUid,
        part: &crate::PartInput,
        count: Option<u32>,
        warnings: &mut Vec<String>,
    ) -> usize {
        let members: BTreeSet<ComponentUid> = self.set_members(lead).into_iter().collect();
        let list: Vec<ComponentUid> = members.iter().copied().collect();
        let junctions: Vec<String> = self
            .build_junctions(Some(&list))
            .into_iter()
            .filter(|j| members.contains(&j.a.component) != members.contains(&j.b.component))
            .filter(|j| j.connectors.is_empty())
            .map(|j| j.id)
            .collect();
        self.fasten_junctions(&junctions, part, count, warnings)
    }

    fn edit_set(
        &mut self,
        component: ComponentUid,
        edit: &SpacingInput,
        config: Option<&String>,
    ) -> Result<Vec<String>, ModelError> {
        let info = self
            .part_info(component)
            .ok_or_else(|| refuse("no such BUILD part"))?;
        let lead = info.record.set.unwrap_or(component);
        let lead_info = self
            .part_info(lead)
            .ok_or_else(|| refuse(format!("{} is in no spacing set", info.name)))?;
        let def: SetDef = lead_info
            .record
            .spacing
            .clone()
            .and_then(|v| serde_json::from_value(v).ok())
            .ok_or_else(|| refuse(format!("{} is in no spacing set", info.name)))?;
        let mut spacing = def.spacing.merged(edit);
        spacing.fastener = spacing.fastener.map(|f| self.kept_fastener(&f));
        let mut base = def.base.clone();
        if let Some(config) = config {
            base.part.config = Some(config.clone());
        }
        let members = self.set_members(lead);
        let set = self.plan_set(&base, &spacing)?;
        let keep = members.len().min(set.plans.len());
        let mut removals = Vec::new();
        // A set fastened to its hosts (mitcad#497): its fasteners go first
        // and are driven anew.
        let fastened = def.spacing.fastener.is_some() || spacing.fastener.is_some();
        if fastened {
            for f in self.set_fasteners(lead) {
                removals.push(self.plan_remove(f)?);
            }
        }
        for c in &members[keep..] {
            removals.push(self.plan_remove(*c)?);
        }
        let active = self.build_host();
        self.state().editable(active)?;
        let count = set.plans.len();
        let def = SetDef {
            spacing,
            base,
            count,
        };
        let mut changed = vec!["spacing".to_owned()];
        if config.is_some() {
            changed.push("config".to_owned());
        }
        let label = format!("Edit {}", lead_info.name);
        let fastener = def.spacing.fastener.clone();
        let fastener_count = def.spacing.fastener_count;
        let depth = self.undo_depth();
        let changed = self.apply(|state| {
            let mut all = Vec::new();
            for (i, plan) in set.plans.iter().enumerate() {
                if i < keep {
                    replace_member(state, plan, active, members[i])?;
                    all.push(members[i]);
                } else {
                    let made = make_part(state, plan, active)?;
                    all.push(made.component.expect("made"));
                }
            }
            for removal in &removals {
                crate::connect::apply_remove(state, removal)?;
            }
            mark_set(state, &all, all[0], &def)?;
            if let (Some(rip), Some(last)) = (&set.rip, all.last()) {
                add_rip(state, *last, rip.clone())?;
            }
            Ok((label.clone(), changed))
        })?;
        if let Some(part) = &fastener {
            let mut warnings = Vec::new();
            self.fasten_set(lead, part, fastener_count, &mut warnings);
            self.merge_undo(depth, &label);
        }
        Ok(changed)
    }
}

/// The components of a junction's id (`C5:end_a|C3:face_xp`).
fn junction_components(id: &str) -> Vec<ComponentUid> {
    id.split('|')
        .filter_map(|side| side.split(':').next()?.parse().ok())
        .collect()
}

/// Marks the members of a set and keeps its definition on the lead.
fn mark_set(
    state: &mut DocState,
    members: &[ComponentUid],
    lead: ComponentUid,
    def: &SetDef,
) -> Result<(), ModelError> {
    let value = serde_json::to_value(def).map_err(|e| refuse(e.to_string()))?;
    for c in members {
        if let Some(record) = state
            .assembly
            .component_mut(*c)
            .and_then(|c| c.build.as_mut())
        {
            record.set = Some(lead);
            record.spacing = (*c == lead).then(|| value.clone());
        }
    }
    Ok(())
}

/// Rips a member: its derived part takes the cut.
fn add_rip(
    state: &mut DocState,
    component: ComponentUid,
    rip: PartCut<ValueInput>,
) -> Result<(), ModelError> {
    let Some(part) = state
        .assembly
        .component(component)
        .and_then(|c| c.build.as_ref())
        .and_then(|b| b.part)
    else {
        return Ok(());
    };
    edit_part(state, part, |def| def.cuts.push(rip))
}

/// Places an existing member anew as a plan says (a set laid out again):
/// its bound lengths detached and made again, its joint edited (or made
/// anew when its kind changes), its derived part's row, values, ends and
/// rip set; the component, its occurrence, its derived part and the
/// connectors on it stay.
pub(crate) fn replace_member(
    state: &mut DocState,
    plan: &Plan,
    active: ComponentUid,
    component: ComponentUid,
) -> Result<(), ModelError> {
    let record = state
        .assembly
        .component(component)
        .and_then(|c| c.build.clone())
        .ok_or_else(|| refuse("a member of the set is gone"))?;
    let part = record
        .part
        .ok_or_else(|| refuse("a member of the set has no derived part"))?;
    let occurrence = state
        .assembly
        .occurrences
        .iter()
        .find(|o| o.component == component && o.parent == active)
        .map(|o| o.uid)
        .ok_or_else(|| refuse("a member of the set is not placed in the BUILD host"))?;
    for bound in current_bounds(state, component) {
        detach(state, component, bound, part)?;
    }
    let values = plan.values.clone();
    let ends = plan.own_ends.clone();
    let config = plan.source.config.clone();
    edit_part(state, part, |def| {
        def.config = config;
        for (name, value) in values {
            def.parameters.insert(name, value);
        }
        def.ends = ends;
        def.cuts.retain(|c| c.kind != "rip");
    })?;
    if let Some(o) = state.assembly.occurrence_mut(occurrence) {
        match &plan.spot {
            Spot::Grounded(t) => {
                o.transform = *t;
                o.grounded = true;
            }
            Spot::Joint(_) => o.grounded = false,
        }
    }
    let reuse = record.joint.filter(|j| state.position(*j).is_some());
    let (joint, bound) = attach_part(state, plan, active, occurrence, part, reuse)?;
    if let Some(r) = state
        .assembly
        .component_mut(component)
        .and_then(|c| c.build.as_mut())
    {
        r.joint = joint;
        r.bound = bound;
        r.points = plan.points.clone();
        r.from = match &plan.spot {
            Spot::Joint(j) => j.from,
            Spot::Grounded(_) => None,
        };
    }
    Ok(())
}

/// When a set's lead goes, the next member that stays takes the set over.
pub(crate) fn hand_over_set(state: &mut DocState, going: &BTreeSet<ComponentUid>) {
    let leads: Vec<(ComponentUid, Value)> = going
        .iter()
        .filter_map(|c| {
            let record = state.assembly.component(*c)?.build.as_ref()?;
            Some((*c, record.spacing.clone()?))
        })
        .collect();
    for (lead, spacing) in leads {
        let heir = state
            .assembly
            .components
            .iter()
            .filter(|c| !going.contains(&c.uid))
            .find(|c| c.build.as_ref().is_some_and(|b| b.set == Some(lead)))
            .map(|c| c.uid);
        let Some(heir) = heir else { continue };
        let others: Vec<ComponentUid> = state
            .assembly
            .components
            .iter()
            .filter(|c| c.build.as_ref().is_some_and(|b| b.set == Some(lead)))
            .map(|c| c.uid)
            .collect();
        for c in others {
            if let Some(r) = state
                .assembly
                .component_mut(c)
                .and_then(|c| c.build.as_mut())
            {
                r.set = Some(heir);
                r.spacing = (c == heir).then(|| spacing.clone());
            }
        }
    }
}

/// Takes the joinery cuts made with parts that go out of the others.
pub(crate) fn drop_cuts_with(
    state: &mut DocState,
    going: &BTreeSet<ComponentUid>,
) -> Result<(), ModelError> {
    let parts: Vec<mitcad_model::ids::FeatureUid> = state
        .features
        .iter()
        .filter(|e| !going.contains(&e.component))
        .filter(|e| match &e.def {
            FeatureDef::DerivedPart(d) => d
                .cuts
                .iter()
                .any(|c| c.with.is_some_and(|w| going.contains(&w))),
            _ => false,
        })
        .map(|e| e.uid)
        .collect();
    for part in parts {
        edit_part(state, part, |def| {
            def.cuts
                .retain(|c| !c.with.is_some_and(|w| going.contains(&w)));
        })?;
    }
    Ok(())
}

/// A computed length for a definition: to a millionth of a millimetre.
fn tidy(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}
