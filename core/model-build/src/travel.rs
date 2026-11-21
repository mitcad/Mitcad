// SPDX-License-Identifier: MIT
//! A carriage's travel limits (mitcad#2088; `commands.md`, "BUILD: drags
//! and travel limits"). A part placed by its `linear.mate` (a carriage on
//! its rail, a slider in a slot) slides on its joint; that slide gets
//! `limits`: by default from where the part is flush with the rail's end A
//! to where it is flush with its end B, the end B one an expression of the
//! rail's length parameter so that it follows the rail. `build_place` and
//! `build_edit` take `limits` (`min`, `max`; left out or null: the
//! default). A drag stops the part at them; the other parts BUILD placed
//! do not drag at all (`mitcad_model`'s `build/held.rs`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geometry::{scale, x_axis};
use crate::parts::{PartInfo, PartsBuild, as_input};
use crate::place::{EditCrateOps, PlaceInput, Plan, Spot, plus, reach_along_x, refuse};
use mitcad_model::build::SideFrom;
use mitcad_model::expr::Unit;
use mitcad_model::features::{FeatureDef, Limit, ValueInput};
use mitcad_model::ids::FeatureUid;
use mitcad_model::internal::build::value_in;
use mitcad_model::joints::{JointKind, Motion};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::transform::Transform;
use mitcad_model::{Document, ModelError};

/// `build_place`'s and `build_edit`'s `limits`: the slide's Min and Max (a
/// length: a number in mm or an expression), each left out or null for
/// its default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TravelInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<ValueInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<ValueInput>,
}

/// A slide's limits as its joint keeps them.
#[derive(Debug, Clone, PartialEq)]
pub struct Travel {
    pub min: ValueInput,
    pub max: ValueInput,
}

impl Travel {
    /// The joint's `limits` for its slide.
    pub(crate) fn limits(&self) -> BTreeMap<Motion, Limit<ValueInput>> {
        BTreeMap::from([(
            Motion::Tx,
            Limit {
                min: Some(self.min.clone()),
                max: Some(self.max.clone()),
                rest: None,
            },
        )])
    }
}

/// The default travel of a part that reaches `lo` to `hi` from its frame
/// along the slide, on a rail `length` long whose length parameter is
/// `name`: flush with end A (its frame `-lo` from it) to flush with end B
/// (`length - hi`, an expression of the parameter). The values and their
/// numbers.
fn default_travel(lo: f64, hi: f64, length: f64, name: Option<&str>) -> (Travel, f64, f64) {
    let min = if lo.abs() < 1e-12 { 0.0 } else { -lo };
    let max = length - hi;
    let travel = Travel {
        min: ValueInput::Number(min),
        max: match name {
            Some(name) => ValueInput::Name(format!("{name}{}", plus(-hi))),
            None => ValueInput::Number(max),
        },
    };
    (travel, min, max)
}

/// A part's frame `own` turned by its joint's override, as the joint
/// takes it.
fn turned_own(own: Transform, o: Option<&mitcad_model::features::FrameOverride>) -> Transform {
    match o {
        Some(o) => mitcad_model::internal::joints::apply_override(&own, o).unwrap_or(own),
        None => own,
    }
}

/// The travel a new part's plan gets (`build_place`'s `limits`): for a
/// part placed by its `linear.mate` with a slide along a part cut to
/// length, its default or the limits asked for. A click past them puts the
/// part at the nearer one; a typed offset past them is refused. `limits`
/// for any other part is refused.
pub(crate) fn plan_travel<K: Kernel>(
    doc: &Document<K>,
    plan: &mut Plan,
    input: &PlaceInput,
) -> Result<(), ModelError> {
    let label = plan.source.def.label();
    let mate = plan
        .source
        .meta
        .linear
        .as_ref()
        .is_some_and(|l| l.mate.is_some());
    let length = plan.target.as_ref().and_then(|t| {
        let (id, l) = t.info.length?;
        Some((doc.parameters().name(id), l, t.info.name.clone()))
    });
    let slide = match &plan.spot {
        Spot::Joint(j) if mate && j.kind == JointKind::Slider => match &j.slide_value {
            Some((Motion::Tx, _, v)) => Some(*v),
            _ => None,
        },
        _ => None,
    };
    let (Some(v), Some((name, length, target))) = (slide, length) else {
        if input.limits.is_some() {
            return Err(refuse(format!(
                "{label} does not slide along another part: it has no travel limits"
            )));
        }
        return Ok(());
    };
    let Spot::Joint(joint) = &mut plan.spot else {
        unreachable!("a slide")
    };
    let own = turned_own(
        plan.mounts
            .get(&joint.own)
            .copied()
            .unwrap_or(Transform::IDENTITY),
        joint.own_override.as_ref(),
    );
    let (lo, hi) = reach_along_x(plan.bounds.as_ref(), &own, plan.rotate, joint.flip);
    let (travel, min, max) = asked(
        doc,
        default_travel(lo, hi, length, Some(&name)),
        input.limits.as_ref(),
        &label,
        &target,
    )?;
    if v < min - 1e-9 || v > max + 1e-9 {
        let typed = input.at.as_ref().is_some_and(|a| a.offset.is_some());
        if typed {
            return Err(refuse(beyond(&label, &target)));
        }
        // A click goes to the nearer limit.
        let at = v.clamp(min, max);
        let shift = Transform::translation(scale(x_axis(&joint.frame_b), at - v));
        joint.frame_b = shift.after(&joint.frame_b);
        joint.slide_value = Some((Motion::Tx, ValueInput::Number(at), at));
        joint.from = Some(SideFrom::A);
        plan.placement = shift.after(&plan.placement);
    }
    plan.travel = Some(travel);
    Ok(())
}

/// The travel asked for over the default (`limits`' `min` and `max`, each
/// left out for its default) and its numbers; refused when Min is past
/// Max.
fn asked<K: Kernel>(
    doc: &Document<K>,
    default: (Travel, f64, f64),
    limits: Option<&TravelInput>,
    label: &str,
    target: &str,
) -> Result<(Travel, f64, f64), ModelError> {
    let (default, dmin, dmax) = default;
    let pick = |given: Option<&ValueInput>, value: ValueInput, number: f64| match given {
        Some(v) => Ok::<_, ModelError>((v.clone(), value_in(doc, v, Unit::MM)?)),
        None => Ok((value, number)),
    };
    let (min, lo) = pick(limits.and_then(|l| l.min.as_ref()), default.min, dmin)?;
    let (max, hi) = pick(limits.and_then(|l| l.max.as_ref()), default.max, dmax)?;
    if lo > hi + 1e-9 {
        let asked = limits.is_some_and(|l| l.min.is_some() || l.max.is_some());
        return Err(refuse(if asked {
            format!("{label}'s travel limits: Min is greater than Max")
        } else {
            format!("{label} is longer than {target}: it has no room to travel on it")
        }));
    }
    Ok((Travel { min, max }, lo, hi))
}

/// The refusal of a place beyond a part's travel.
fn beyond(label: &str, target: &str) -> String {
    format!(
        "the offset puts {label} beyond its travel on {target}: it goes between Min and Max of \
         its travel limits"
    )
}

/// The slide a BUILD part's joint gives it when it slides on another part
/// cut to length (a carriage on its rail).
struct Slide {
    joint: FeatureUid,
    /// The part's reach from its frame along the slide, least and most.
    reach: (f64, f64),
    /// The target's length and its parameter's name.
    length: f64,
    name: String,
    /// The target's name.
    target: String,
}

impl Slide {
    /// Its default travel.
    fn default_travel(&self) -> (Travel, f64, f64) {
        default_travel(self.reach.0, self.reach.1, self.length, Some(&self.name))
    }
}

/// [`Slide`] of a part, its reach turned by `rotate`, else by its joint's
/// angle.
fn slide_of<K: Kernel>(doc: &Document<K>, info: &PartInfo, rotate: Option<f64>) -> Option<Slide> {
    if !doc.build_part_slides(info.component) {
        return None;
    }
    let joint = info.record.joint?;
    let entry = doc.feature(joint)?;
    let FeatureDef::Joint(j) = &entry.def else {
        return None;
    };
    if j.kind != JointKind::Slider || j.slide_axis.motion() != Motion::Tx {
        return None;
    }
    let (Some(length), Some(name)) = doc.joint_target_length(joint) else {
        return None;
    };
    let target = mitcad_model::internal::joints::path_component(
        doc.assembly(),
        entry.component,
        &j.b.occurrence.0,
    )
    .ok()
    .map(|c| doc.assembly().name(c))
    .unwrap_or_default();
    let mitcad_model::features::GeomRef::Mount { name: own, .. } = &j.a.geometry else {
        return None;
    };
    let own = turned_own(
        info.mount(own).copied().unwrap_or(Transform::IDENTITY),
        j.a.frame_override.as_ref(),
    );
    let angle = rotate
        .or_else(|| j.angle.and_then(|id| doc.parameters().value(id)))
        .unwrap_or(0.0);
    let bounds: Option<BoundingBox> = doc.part_bounds(info.component);
    Some(Slide {
        joint,
        reach: reach_along_x(bounds.as_ref(), &own, angle, j.flip),
        length,
        name,
        target,
    })
}

/// A sliding part's travel limits, in mm along the slide from the rail's
/// end A (`build_parts`' `limits`, which BUILD's Placement shows as Min
/// and Max).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TravelNow {
    /// The joint's, none for a design saved without them.
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// The defaults: flush with the rail's end A and end B.
    pub default_min: f64,
    pub default_max: f64,
}

/// [`TravelNow`] of a part; none for a part that does not slide on
/// another cut to length.
pub fn travel_of<K: Kernel>(
    doc: &Document<K>,
    component: mitcad_model::ids::ComponentUid,
) -> Option<TravelNow> {
    let info = doc.part_info(component)?;
    let slide = slide_of(doc, &info, None)?;
    let Some(FeatureDef::Joint(j)) = doc.feature(slide.joint).map(|e| &e.def) else {
        return None;
    };
    let limit = j.limits.get(&Motion::Tx);
    let round = |v: f64| (v * 1e6).round() / 1e6 + 0.0;
    let value = |id: Option<mitcad_model::parameters::ParamId>| {
        id.and_then(|id| doc.parameters().value(id)).map(round)
    };
    let (_, default_min, default_max) = slide.default_travel();
    Some(TravelNow {
        min: limit.and_then(|l| value(l.min)),
        max: limit.and_then(|l| value(l.max)),
        default_min: round(default_min),
        default_max: round(default_max),
    })
}

/// What `build_edit` does to a sliding part's limits: with `limits`, those
/// asked for over the default; turned (`rotate`), each limit that was its
/// default becomes the new default. The new place of its slide (`tx`, the
/// edited offset's value, else where it is) must be within them, else the
/// edit is refused. None when the limits stay; `limits` on a part that
/// does not slide on another is refused.
pub(crate) fn edit_travel<K: Kernel>(
    doc: &Document<K>,
    info: &PartInfo,
    limits: Option<&TravelInput>,
    rotate: Option<f64>,
    tx: Option<&ValueInput>,
) -> Result<Option<(FeatureUid, Travel)>, ModelError> {
    let Some(slide) = slide_of(doc, info, rotate) else {
        if limits.is_some() {
            return Err(refuse(format!(
                "{} does not slide along another part: it has no travel limits",
                info.name
            )));
        }
        return Ok(None);
    };
    let (joint, target) = (slide.joint, slide.target.clone());
    let Some(FeatureDef::Joint(j)) = doc.feature(joint).map(|e| &e.def) else {
        return Ok(None);
    };
    let now = j.limits.get(&Motion::Tx);
    let value_of = |id: Option<mitcad_model::parameters::ParamId>| {
        id.and_then(|id| doc.parameters().value(id))
    };
    let (default, dmin, dmax) = slide.default_travel();
    let (travel, min, max) = match (limits, rotate) {
        (Some(limits), _) => asked(
            doc,
            (default, dmin, dmax),
            Some(limits),
            &info.name,
            &target,
        )?,
        (None, Some(_)) => {
            // The limits that were their default (at the reach before the
            // turn) take the new default; others stay.
            let Some(before) = slide_of(doc, info, None) else {
                return Ok(None);
            };
            let (_, bmin, bmax) = before.default_travel();
            let input = match as_input(&FeatureDef::Joint(j.clone()), doc.parameters()) {
                FeatureDef::Joint(j) => j.limits.get(&Motion::Tx).cloned(),
                _ => None,
            };
            let keep = |id: Option<mitcad_model::parameters::ParamId>, was: f64| {
                value_of(id).is_some_and(|v| (v - was).abs() > 1e-9)
            };
            let given = TravelInput {
                min: now
                    .filter(|l| keep(l.min, bmin))
                    .and(input.as_ref().and_then(|l| l.min.clone())),
                max: now
                    .filter(|l| keep(l.max, bmax))
                    .and(input.as_ref().and_then(|l| l.max.clone())),
            };
            asked(
                doc,
                (default, dmin, dmax),
                Some(&given),
                &info.name,
                &target,
            )?
        }
        (None, None) => {
            // An offset alone stays within the limits the joint has.
            let (Some(tx), Some(now)) = (tx, now) else {
                return Ok(None);
            };
            let v = value_in(doc, tx, Unit::MM)?;
            let (lo, hi) = (value_of(now.min), value_of(now.max));
            if lo.is_some_and(|m| v < m - 1e-9) || hi.is_some_and(|m| v > m + 1e-9) {
                return Err(refuse(beyond(&info.name, &target)));
            }
            return Ok(None);
        }
    };
    let v = match tx {
        Some(tx) => value_in(doc, tx, Unit::MM)?,
        None => j
            .position
            .get(&Motion::Tx)
            .and_then(|id| doc.parameters().value(*id))
            .unwrap_or(0.0),
    };
    if v < min - 1e-9 || v > max + 1e-9 {
        return Err(refuse(match tx {
            Some(_) => beyond(&info.name, &target),
            None => format!(
                "{} is outside those travel limits: move it within them first, or keep Min and \
                 Max around where it is",
                info.name
            ),
        }));
    }
    Ok(Some((joint, travel)))
}
