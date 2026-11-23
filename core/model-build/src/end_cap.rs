// SPDX-License-Identifier: MIT
//! End caps (mitcad#2243; `commands.md`, "BUILD: profile assemblies", *End
//! caps*): `build_end_cap` closes ends of profiles cut to length with
//! plates made of their sections, each a BUILD part of its own (role
//! `end_cap`, named `<member> End Cap`) whose body is an `end_cap` feature
//! (`features/end_cap.rs`), joined to the member's end frame by a rigid
//! joint, flipped. An inward cap cuts its member back by its thickness:
//! the member's derived part's `cut_back` holds the thickness, and the
//! cap's thickness is that parameter, so both follow one value. A cap on
//! an end that has one already changes it. The member's record lists its
//! caps among what it `adds`, so they go with it; a cap removed gives its
//! member its length back.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::geometry::parts;
use crate::parts::as_input;
use crate::place::{EndFrom, EndsInput, add_feature, edit_part, refuse};
use crate::prelude::*;
use crate::{BuildItem, BuildRecord, BuildRole};
use mitcad_model::expr::Unit;
use mitcad_model::features::end_cap::{
    CapDirection, CapEnd, EndCapDef, Plate, SEAT, end_face, member_section, plate_of,
};
use mitcad_model::features::{
    FeatureDef, GeomRef, JointDef, JointOrigin, OccurrencePath, ValueInput,
};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::internal::build::value_in;
use mitcad_model::joints::{JointKind, SlideAxis};
use mitcad_model::kernel::Kernel;
use mitcad_model::transform::Transform;
use mitcad_model::{DocState, Document, ModelError};

/// An end to cap: a BUILD part (uid or name) and its end.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapAt {
    pub component: String,
    pub end: EndFrom,
}

/// `build_end_cap`'s fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndCapInput {
    pub ends: Vec<CapAt>,
    pub thickness: ValueInput,
    /// Set in from the outside of the section, a length.
    #[serde(default)]
    pub inset: Option<ValueInput>,
    /// Or as a share of the thickness.
    #[serde(default)]
    pub ratio: Option<ValueInput>,
    /// The corners' chamfer.
    #[serde(default)]
    pub chamfer: Option<ValueInput>,
    #[serde(default)]
    pub direction: CapDirection,
}

/// Why an end is not capped: the component it is about and the refusal in
/// words.
pub type CapRefusal = (Option<ComponentUid>, String);

/// What capping one end is: the member, its derived part, its path from
/// the BUILD host, the end, the cap it has there already, and the plate.
#[derive(Debug, Clone)]
pub struct CapPlan {
    pub member: ComponentUid,
    pub name: String,
    pub part: FeatureUid,
    pub path: Vec<OccurrenceUid>,
    pub end: CapEnd,
    pub existing: Option<ComponentUid>,
    pub plate: Plate,
    /// The end frame in the design's coordinates.
    pub frame: Transform,
}

/// A cap `build_end_cap` made or changed.
#[derive(Debug, Clone, PartialEq)]
pub struct CapMade {
    pub component: ComponentUid,
    pub occurrence: Option<OccurrenceUid>,
    pub member: ComponentUid,
    pub end: CapEnd,
    pub name: String,
    /// False for a cap that was there and changed.
    pub new: bool,
}

/// A BUILD end cap of the design: its component, member, end and values.
#[derive(Debug, Clone, PartialEq)]
pub struct CapInfo {
    pub component: ComponentUid,
    pub feature: FeatureUid,
    pub member: ComponentUid,
    pub end: CapEnd,
    pub direction: CapDirection,
    pub thickness: f64,
    pub inset: Option<f64>,
    pub ratio: Option<f64>,
    pub chamfer: Option<f64>,
}

/// The values a command gives, in millimetres (the ratio as it is).
struct Values {
    thickness: f64,
    inset: Option<f64>,
    ratio: Option<f64>,
    chamfer: Option<f64>,
}

/// The document's end cap methods, as an extension trait (mitcad#777).
pub trait EndCapBuild<K: Kernel> {
    /// Caps ends (`build_end_cap`), one undo step; an end with a cap has
    /// it changed.
    fn build_end_cap(&mut self, input: &EndCapInput) -> Result<Vec<CapMade>, ModelError>;

    /// What `build_end_cap` would do for each end, or why it refuses it:
    /// the end's component and its refusal in words.
    fn plan_end_caps(
        &self,
        input: &EndCapInput,
    ) -> Result<Vec<Result<CapPlan, CapRefusal>>, ModelError>;

    /// The design's BUILD end caps.
    fn end_caps(&self) -> Vec<CapInfo>;
}

/// The planning's own methods of the document (mitcad#777: an extension
/// trait, the document being the model's).
trait EndCapCrateOps<K: Kernel> {
    fn cap_values(&self, input: &EndCapInput) -> Result<Values, ModelError>;
    fn end_cap_on(&self, member: ComponentUid, end: CapEnd) -> Option<ComponentUid>;
    fn on_end(&self, part: FeatureUid, end: CapEnd, except: Option<FeatureUid>) -> Option<String>;
    fn plan_cap(
        &self,
        at: &CapAt,
        values: &Values,
        direction: CapDirection,
    ) -> Result<CapPlan, CapRefusal>;
}

impl<K: Kernel> EndCapCrateOps<K> for Document<K> {
    /// The command's values, refused in words.
    fn cap_values(&self, input: &EndCapInput) -> Result<Values, ModelError> {
        let thickness = value_in(self, &input.thickness, Unit::MM)?;
        if thickness.is_nan() || thickness <= 0.0 {
            return Err(refuse("the end cap's thickness must be greater than zero"));
        }
        let optional = |v: &Option<ValueInput>, unit: Unit| -> Result<Option<f64>, ModelError> {
            v.as_ref().map(|v| value_in(self, v, unit)).transpose()
        };
        let inset = optional(&input.inset, Unit::MM)?;
        let ratio = optional(&input.ratio, Unit::NONE)?;
        let chamfer = optional(&input.chamfer, Unit::MM)?;
        if inset.is_some() && ratio.is_some() {
            return Err(refuse(
                "an end cap's inset is a length or a ratio, not both",
            ));
        }
        for (name, value) in [("inset", inset), ("ratio", ratio), ("chamfer", chamfer)] {
            if value.is_some_and(|v| v.is_nan() || v < 0.0) {
                return Err(refuse(format!("the end cap's {name} must be zero or more")));
            }
        }
        Ok(Values {
            thickness,
            inset,
            ratio,
            chamfer,
        })
    }

    /// The BUILD end cap on a part's end, if it has one.
    fn end_cap_on(&self, member: ComponentUid, end: CapEnd) -> Option<ComponentUid> {
        self.end_caps()
            .into_iter()
            .find(|c| c.member == member && c.end == end)
            .map(|c| c.component)
    }

    /// The BUILD part placed on an end of a part, or the part's end placed
    /// on (another's joint on its end frame, or its own end frame joined
    /// to another BUILD part), other than the joint `except`.
    fn on_end(&self, part: FeatureUid, end: CapEnd, except: Option<FeatureUid>) -> Option<String> {
        let state = self.state();
        for entry in self.features() {
            let FeatureDef::Joint(j) = &entry.def else {
                continue;
            };
            if Some(entry.uid) == except {
                continue;
            }
            let at_end = |side: &JointOrigin| {
                matches!(&side.geometry, GeomRef::Mount { feature, name }
                    if *feature == part && name == end.frame())
            };
            let other = if at_end(&j.a) {
                &j.b
            } else if at_end(&j.b) {
                &j.a
            } else {
                continue;
            };
            let Ok(component) = mitcad_model::internal::joints::path_component(
                &state.assembly,
                entry.component,
                &other.occurrence.0,
            ) else {
                continue;
            };
            if let Some(c) = state.assembly.component(component)
                && c.build.is_some()
            {
                return Some(c.name.clone());
            }
        }
        None
    }

    /// Plans one end, or says why not (with the component it is about).
    fn plan_cap(
        &self,
        at: &CapAt,
        values: &Values,
        direction: CapDirection,
    ) -> Result<CapPlan, CapRefusal> {
        let component = self
            .component_by_ref(&at.component)
            .map_err(|e| (None, e.to_string()))?;
        let mut end = match at.end {
            EndFrom::A => CapEnd::A,
            EndFrom::B => CapEnd::B,
        };
        let name_of = |c: ComponentUid| self.assembly().name(c);
        let fail = |c: ComponentUid, text: String| (Some(c), text);
        // A cap picked: its member's end.
        let mut member = component;
        if let Some(cap) = self
            .end_caps()
            .into_iter()
            .find(|c| c.component == component)
        {
            member = cap.member;
            end = cap.end;
        }
        let info = self.part_info(member).ok_or_else(|| {
            fail(
                member,
                format!("{} is no part of the Build tab", name_of(member)),
            )
        })?;
        let name = info.name.clone();
        if info.meta.role != BuildRole::Profile || !info.is_cut_to_length() {
            return Err(fail(
                member,
                format!(
                    "{name} is no profile cut to length: an end cap closes a profile's or a tube's end"
                ),
            ));
        }
        let part = info
            .part
            .ok_or_else(|| fail(member, format!("{name} has no derived part")))?;
        let path = self.placed_in_host(member).ok_or_else(|| {
            fail(
                member,
                format!(
                    "{name} is not in {}, where the Build tab places components",
                    name_of(self.build_host())
                ),
            )
        })?;
        let params = self.parameters();
        if let Some(cut) = info.ends().and_then(|e| e.end(end.is_b()))
            && params.value(cut.angle).is_some_and(|a| a.abs() > 1e-12)
        {
            return Err(fail(
                member,
                format!(
                    "end {} of {name} is mitered: an end cap goes on a square end (square it with Ends first)",
                    end.letter()
                ),
            ));
        }
        let existing = self.end_cap_on(member, end);
        let except = existing
            .and_then(|c| self.part_info(c))
            .and_then(|i| i.record.joint);
        if let Some(other) = self.on_end(part, end, except) {
            return Err(fail(
                member,
                format!(
                    "{other} is placed on end {} of {name}: an end cap needs a free end",
                    end.letter()
                ),
            ));
        }
        let frame = *info
            .mounts
            .get(end.frame())
            .ok_or_else(|| fail(member, format!("{name} has no frame {}", end.frame())))?;
        let body = self
            .body_shape(BodyUid::new(part, 0))
            .ok_or_else(|| fail(member, format!("{name} has no body")))?;
        let section = member_section(self.kernel(), body, &frame, &end_face(part, end))
            .map_err(|e| fail(member, format!("{name}: {e}")))?;
        let plate = plate_of(
            &section,
            values.thickness,
            values.inset,
            values.ratio,
            values.chamfer,
        )
        .map_err(|e| fail(member, format!("end {} of {name}: {e}", end.letter())))?;
        // Inward: the member keeps something of its length.
        if direction == CapDirection::Inward
            && let Some((_, length)) = info.length
        {
            let other = info
                .def
                .as_ref()
                .and_then(|d| d.cut_back.end(!end.is_b()))
                .and_then(|id| params.value(*id))
                .unwrap_or(0.0);
            if length - other - values.thickness <= 1e-6 {
                return Err(fail(
                    member,
                    format!(
                        "{name} is {} mm long: an inward end cap of {} mm leaves nothing of it",
                        mitcad_model::build::round(length - other),
                        mitcad_model::build::round(values.thickness)
                    ),
                ));
            }
        }
        let world = self
            .host_placement()
            .after(&self.path_transform(&path))
            .after(&frame);
        Ok(CapPlan {
            member,
            name,
            part,
            path,
            end,
            existing,
            plate,
            frame: world,
        })
    }
}

impl<K: Kernel> EndCapBuild<K> for Document<K> {
    fn plan_end_caps(
        &self,
        input: &EndCapInput,
    ) -> Result<Vec<Result<CapPlan, CapRefusal>>, ModelError> {
        let values = self.cap_values(input)?;
        let mut plans: Vec<Result<CapPlan, CapRefusal>> = Vec::new();
        for at in &input.ends {
            let plan = self.plan_cap(at, &values, input.direction);
            // An end picked twice is capped once.
            if let Ok(p) = &plan
                && plans.iter().any(|q| {
                    q.as_ref()
                        .is_ok_and(|q| q.member == p.member && q.end == p.end)
                })
            {
                continue;
            }
            plans.push(plan);
        }
        Ok(plans)
    }

    fn build_end_cap(&mut self, input: &EndCapInput) -> Result<Vec<CapMade>, ModelError> {
        if input.ends.is_empty() {
            return Err(refuse("pick the ends to cap"));
        }
        let mut plans = Vec::new();
        for plan in self.plan_end_caps(input)? {
            plans.push(plan.map_err(|(_, text)| refuse(text))?);
        }
        let host = self.build_host();
        self.state().editable(host)?;
        let label = match plans.as_slice() {
            [one] => format!("End Cap {}", one.name),
            more => format!("End Caps x{}", more.len()),
        };
        let input = input.clone();
        self.apply(|state| {
            let mut made = Vec::new();
            for plan in &plans {
                made.push(make_cap(state, plan, &input, host)?);
            }
            Ok((label, made))
        })
    }

    fn end_caps(&self) -> Vec<CapInfo> {
        let params = self.parameters();
        let value = |id| params.value(id);
        self.assembly()
            .components
            .iter()
            .filter_map(|c| {
                let record = c.build.as_ref()?;
                if record.role != BuildRole::EndCap {
                    return None;
                }
                let feature = record.part?;
                let FeatureDef::EndCap(def) = &self.feature(feature)?.def else {
                    return None;
                };
                let member = self.feature(def.member?)?.component;
                Some(CapInfo {
                    component: c.uid,
                    feature,
                    member,
                    end: def.end?,
                    direction: def.direction,
                    thickness: value(def.thickness).unwrap_or(f64::NAN),
                    inset: def.inset.and_then(value),
                    ratio: def.ratio.and_then(value),
                    chamfer: def.chamfer.and_then(value),
                })
            })
            .collect()
    }
}

/// Sets or clears an end's cut back of a derived part; returns the name
/// of the parameter that holds it when set.
fn set_cut_back(
    state: &mut DocState,
    part: FeatureUid,
    end: CapEnd,
    value: Option<ValueInput>,
) -> Result<Option<String>, ModelError> {
    let had = match &state.entry(part).map(|e| &e.def) {
        Some(FeatureDef::DerivedPart(d)) => d.cut_back.end(end.is_b()).is_some(),
        _ => false,
    };
    if value.is_none() && !had {
        return Ok(None);
    }
    let set = value.is_some();
    edit_part(state, part, |def| {
        if end.is_b() {
            def.cut_back.b = value;
        } else {
            def.cut_back.a = value;
        }
    })?;
    if !set {
        return Ok(None);
    }
    Ok(match &state.entry(part).map(|e| &e.def) {
        Some(FeatureDef::DerivedPart(d)) => d
            .cut_back
            .end(end.is_b())
            .map(|id| state.parameters.name(*id)),
        _ => None,
    })
}

/// Makes or changes the cap a plan says, in `state`.
fn make_cap(
    state: &mut DocState,
    plan: &CapPlan,
    input: &EndCapInput,
    host: ComponentUid,
) -> Result<CapMade, ModelError> {
    let inward = input.direction == CapDirection::Inward;
    // Inward: the member's cut back holds the thickness first, so that
    // the cap's thickness can be that parameter.
    let thickness = if inward {
        let name = set_cut_back(state, plan.part, plan.end, Some(input.thickness.clone()))?
            .ok_or_else(|| refuse("the member's cut back has no parameter"))?;
        ValueInput::Name(name)
    } else {
        input.thickness.clone()
    };
    let def = EndCapDef::<ValueInput> {
        member: Some(plan.part),
        end: Some(plan.end),
        face: None,
        thickness,
        inset: input.inset.clone(),
        ratio: input.ratio.clone(),
        chamfer: input.chamfer.clone().filter(|c| !is_zero(c)),
        direction: input.direction,
    };
    let made = match plan.existing {
        Some(cap) => {
            let feature = state
                .assembly
                .component(cap)
                .and_then(|c| c.build.as_ref())
                .and_then(|r| r.part)
                .ok_or_else(|| refuse("the end cap has no plate"))?;
            state.edit_feature(feature, &FeatureDef::EndCap(def))?;
            CapMade {
                component: cap,
                occurrence: state.assembly.occurrences_of(cap).next().map(|o| o.uid),
                member: plan.member,
                end: plan.end,
                name: state.assembly.name(cap),
                new: false,
            }
        }
        None => {
            let name = state.free_component_name(&format!("{} End Cap", plan.name));
            let cap = state.assembly.create(Some(&name), None);
            let occurrence = state.assembly.place(cap, host, Transform::IDENTITY);
            let feature = add_feature(state, FeatureDef::EndCap(def), cap)?;
            // The member's material and look (mitcad#2243).
            let member_body = BodyUid::new(plan.part, 0);
            if let Some(attributes) = state.body_attributes.get(&member_body).cloned() {
                let own = state
                    .body_attributes
                    .entry(BodyUid::new(feature, 0))
                    .or_default();
                own.material = attributes.material.clone();
                own.material_expr = attributes.material_expr.clone();
                own.appearance = attributes.appearance.clone();
                own.appearance_expr = attributes.appearance_expr.clone();
            }
            let joint = JointDef::<ValueInput> {
                kind: JointKind::Rigid,
                slide_axis: SlideAxis::default(),
                a: JointOrigin {
                    occurrence: OccurrencePath(vec![occurrence]),
                    geometry: GeomRef::Mount {
                        feature,
                        name: SEAT.to_owned(),
                    },
                    frame_override: None,
                },
                b: JointOrigin {
                    occurrence: OccurrencePath(plan.path.clone()),
                    geometry: GeomRef::Mount {
                        feature: plan.part,
                        name: plan.end.frame().to_owned(),
                    },
                    frame_override: None,
                },
                offset: None,
                angle: None,
                flip: true,
                limits: BTreeMap::new(),
                position: BTreeMap::new(),
                start: BTreeMap::new(),
            };
            let joint = add_feature(state, FeatureDef::Joint(joint), host)?;
            state.assembly.component_mut(cap).expect("made").build = Some(BuildRecord {
                role: BuildRole::EndCap,
                part: Some(feature),
                joint: Some(joint),
                added_by: Some(plan.member),
                ..BuildRecord::default()
            });
            if let Some(record) = state
                .assembly
                .component_mut(plan.member)
                .and_then(|c| c.build.as_mut())
                && !record.adds.contains(&BuildItem::Component(cap))
            {
                record.adds.push(BuildItem::Component(cap));
            }
            CapMade {
                component: cap,
                occurrence: Some(occurrence),
                member: plan.member,
                end: plan.end,
                name,
                new: true,
            }
        }
    };
    // Outward: the member's whole length again.
    if !inward {
        set_cut_back(state, plan.part, plan.end, None)?;
    }
    Ok(made)
}

/// Whether a value is a plain zero (no chamfer).
fn is_zero(value: &ValueInput) -> bool {
    match value {
        ValueInput::Number(n) => n.abs() < 1e-12,
        ValueInput::Name(text) => text.trim().parse::<f64>().is_ok_and(|n| n.abs() < 1e-12),
    }
}

/// The caps among the components that go (`build_remove`): their members
/// that stay get their length back (no cut back) and lose them from what
/// they add. Called before the components are deleted, with the cap's
/// thickness made a number first so that no parameter refers to the cut
/// back that goes.
pub(crate) fn release_caps(
    state: &mut DocState,
    components: &std::collections::BTreeSet<ComponentUid>,
) -> Result<(), ModelError> {
    let caps: Vec<(ComponentUid, FeatureUid)> = components
        .iter()
        .filter_map(|c| {
            let record = state.assembly.component(*c)?.build.as_ref()?;
            (record.role == BuildRole::EndCap).then_some((*c, record.part?))
        })
        .collect();
    for (cap, feature) in caps {
        let Some(FeatureDef::EndCap(def)) = state.entry(feature).map(|e| e.def.clone()) else {
            continue;
        };
        let (Some(part), Some(end)) = (def.member, def.end) else {
            continue;
        };
        let Some(member) = state.entry(part).map(|e| e.component) else {
            continue;
        };
        if components.contains(&member) {
            continue;
        }
        if def.direction == CapDirection::Inward {
            // The cap's thickness a number of its own, then no cut back.
            let thickness = state.parameters.value(def.thickness).unwrap_or(0.0);
            if let FeatureDef::EndCap(mut input) =
                as_input(&FeatureDef::EndCap(def.clone()), &state.parameters)
            {
                input.thickness = ValueInput::Number(thickness);
                state.edit_feature(feature, &FeatureDef::EndCap(input))?;
            }
            set_cut_back(state, part, end, None)?;
        }
        if let Some(record) = state
            .assembly
            .component_mut(member)
            .and_then(|c| c.build.as_mut())
        {
            record
                .adds
                .retain(|item| *item != BuildItem::Component(cap));
        }
    }
    Ok(())
}

/// Refuses a `build_edit` that miters an end with an end cap (mitcad#2243):
/// the cap goes on a square end.
pub(crate) fn check_capped_ends<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
    ends: Option<&EndsInput>,
    shared: Option<EndFrom>,
) -> Result<(), ModelError> {
    let caps: Vec<CapInfo> = doc
        .end_caps()
        .into_iter()
        .filter(|c| c.member == component)
        .collect();
    if caps.is_empty() {
        return Ok(());
    }
    let name = doc.assembly().name(component);
    let refused = |end: CapEnd| {
        refuse(format!(
            "end {} of {name} has an end cap: it goes on a square end (remove the cap first)",
            end.letter()
        ))
    };
    for cap in &caps {
        if let Some(ends) = ends {
            let cut = if cap.end.is_b() { &ends.b } else { &ends.a };
            if let Some(Some(cut)) = cut {
                let angle = value_in(doc, &cut.angle, Unit::RAD).unwrap_or(0.0);
                if angle.abs() > 1e-12 {
                    return Err(refused(cap.end));
                }
            }
        }
        if let Some(end) = shared
            && (end == EndFrom::B) == cap.end.is_b()
        {
            return Err(refused(cap.end));
        }
    }
    Ok(())
}

/// The plate's outline at its start in the design's coordinates (the end
/// face for an outward cap, the cut back's face for an inward one), and
/// the vector across its thickness.
pub fn plate_outline(
    plan: &CapPlan,
    thickness: f64,
    direction: CapDirection,
) -> (Vec<[f64; 3]>, [f64; 3]) {
    let [_, _, _, z] = parts(&plan.frame);
    let z0 = if direction == CapDirection::Inward {
        -thickness
    } else {
        0.0
    };
    let points = plan
        .plate
        .outline(8)
        .into_iter()
        .map(|[x, y]| plan.frame.apply_point([x, y, z0]))
        .collect();
    (points, crate::geometry::scale(z, thickness))
}
