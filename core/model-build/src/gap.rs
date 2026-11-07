// SPDX-License-Identifier: MIT
//! The middle of a free stretch of a side (mitcad#1401; `commands.md`,
//! "BUILD: profile assemblies", *Middle of Gap*): the profiles standing on
//! a side's slot split it into free stretches, each from the far face of
//! the profile towards end A (or end A) to the near face of the one
//! towards end B (or end B). A part placed from the middle of one
//! (`at.from` `gap`) is centred between them: its joint's slide is an
//! expression of their slides (and of the side's length at an end), so
//! that it stays centred when either moves or the side's length changes.
//! When one of them goes, the model binds it to the side's nearer end in
//! place (`mitcad_model::build`, `gap.rs`).

use std::collections::BTreeSet;

use mitcad_model::features::{FeatureDef, GeomRef, JointDef, ValueInput};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::joints::JointKind;
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::{ParamId, Parameters};
use mitcad_model::{Document, ModelError};

use crate::BuildRole;
use crate::place::{EditCrateOps, plus, refuse};

/// A profile standing on a side's slot: its joint's slide parameter, its
/// reach along the slot from its slide, and where its faces are (from end
/// A of the side).
#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    pub component: ComponentUid,
    pub param: String,
    pub reach: (f64, f64),
    pub near: f64,
    pub far: f64,
}

/// A free stretch of a side, as distances from its end A: from the far
/// face of the profile `a` (end A when None) to the near face of `b` (end
/// B when None).
#[derive(Debug, Clone, PartialEq)]
pub struct SideGap {
    pub start: f64,
    pub end: f64,
    pub a: Option<Standing>,
    pub b: Option<Standing>,
}

impl SideGap {
    pub fn middle(&self) -> f64 {
        (self.start + self.end) / 2.0
    }

    /// Whether a profile bounds it: the whole free side is the side's
    /// middle, not a stretch between profiles.
    pub fn between(&self) -> bool {
        self.a.is_some() || self.b.is_some()
    }

    /// Whether a part `width` wide fits in it.
    pub fn takes(&self, width: f64) -> bool {
        self.end - self.start >= width - 1e-9
    }

    /// The profiles it lies between, as the part's record keeps them.
    pub fn neighbours(&self) -> [Option<ComponentUid>; 2] {
        [
            self.a.as_ref().map(|s| s.component),
            self.b.as_ref().map(|s| s.component),
        ]
    }
}

/// Whether the parameter `id` uses `on`, directly or through others.
fn depends_on(params: &Parameters, id: ParamId, on: ParamId) -> bool {
    let mut stack = vec![id];
    let mut seen = BTreeSet::new();
    while let Some(p) = stack.pop() {
        if p == on {
            return true;
        }
        if seen.insert(p)
            && let Some(param) = params.get(p)
        {
            stack.extend(param.dependencies().iter().copied());
        }
    }
    false
}

/// The parameter a BUILD part's joint slides by.
fn slide_param<K: Kernel>(doc: &Document<K>, component: ComponentUid) -> Option<ParamId> {
    let joint = doc.part_info(component)?.record.joint?;
    let FeatureDef::Joint(j) = &doc.feature(joint)?.def else {
        return None;
    };
    j.position.get(&j.slide_axis.motion()).copied()
}

/// The profiles standing on the slot `slot` of the derived part `part` at
/// `path` (their joints slide on it), but `except`; and none whose slide
/// uses `except`'s, which would make a cycle.
pub(crate) fn standing_on<K: Kernel>(
    doc: &Document<K>,
    path: &[OccurrenceUid],
    part: FeatureUid,
    slot: &str,
    except: Option<ComponentUid>,
) -> Vec<Standing> {
    let avoid = except.and_then(|c| slide_param(doc, c));
    let mut out = Vec::new();
    for c in &doc.assembly().components {
        let Some(record) = &c.build else {
            continue;
        };
        if record.role != BuildRole::Profile || Some(c.uid) == except {
            continue;
        }
        let Some(joint) = record.joint else {
            continue;
        };
        let Some(FeatureDef::Joint(j)) = doc.feature(joint).map(|e| &e.def) else {
            continue;
        };
        let on_slot = match &j.b.geometry {
            GeomRef::Mount { feature, name } => *feature == part && name == slot,
            _ => false,
        };
        if j.kind != JointKind::Slider || j.b.occurrence.0.as_slice() != path || !on_slot {
            continue;
        }
        let Some(&id) = j.position.get(&j.slide_axis.motion()) else {
            continue;
        };
        let Some(at) = doc.parameters().value(id) else {
            continue;
        };
        if avoid.is_some_and(|a| depends_on(doc.parameters(), id, a)) {
            continue;
        }
        let Some(info) = doc.part_info(c.uid) else {
            continue;
        };
        let reach = doc.side_reach(&info, joint, None);
        out.push(Standing {
            component: c.uid,
            param: doc.parameters().name(id),
            reach,
            near: at + reach.0,
            far: at + reach.1,
        });
    }
    out
}

/// The free stretches of a side `length` long that the profiles
/// `standing` leave, from end A; the whole side when none stands on it.
pub(crate) fn gaps_along(standing: &[Standing], length: f64) -> Vec<SideGap> {
    let mut sorted = standing.to_vec();
    sorted.sort_by(|p, q| p.near.total_cmp(&q.near));
    let mut out = Vec::new();
    let mut start = 0.0;
    let mut a: Option<Standing> = None;
    for s in sorted {
        let end = s.near.min(length);
        if end > start + 1e-9 {
            out.push(SideGap {
                start,
                end,
                a: a.clone(),
                b: Some(s.clone()),
            });
        }
        if s.far >= start {
            start = s.far;
            a = Some(s);
        }
    }
    if length > start + 1e-9 {
        out.push(SideGap {
            start,
            end: length,
            a,
            b: None,
        });
    }
    out
}

/// The free stretch a point along the side (from end A) is in.
pub(crate) fn gap_containing(gaps: &[SideGap], at: f64) -> Option<&SideGap> {
    gaps.iter()
        .find(|g| g.start - 1e-9 <= at && at <= g.end + 1e-9)
}

/// The slide of a part centred in `gap`, `offset` from its middle (towards
/// end B), as the joint keeps it: the middle of the bounding faces, an
/// expression of the profiles' slides, end B the side's length `length`
/// (between 30 profiles `(d14 + d20) / 2`, `d20 / 2 - 7.5` from end A,
/// `(d14 + d3) / 2 + 7.5` to end B). `middle` is the part's middle from
/// its slide.
pub(crate) fn gap_slide_value(
    gap: &SideGap,
    offset: &ValueInput,
    middle: f64,
    length: Option<&str>,
) -> Result<ValueInput, ModelError> {
    let mut terms = Vec::new();
    let mut k = -middle;
    if let Some(a) = &gap.a {
        terms.push(a.param.clone());
        k += a.reach.1 / 2.0;
    }
    match &gap.b {
        Some(b) => {
            terms.push(b.param.clone());
            k += b.reach.0 / 2.0;
        }
        None => terms.push(
            length
                .ok_or_else(|| refuse("the part has no length to find the free stretch's end"))?
                .to_owned(),
        ),
    }
    let half = match terms.as_slice() {
        [one] => format!("{one} / 2"),
        _ => format!("({}) / 2", terms.join(" + ")),
    };
    Ok(ValueInput::Name(match offset {
        ValueInput::Number(n) => format!("{half}{}", plus(k + n)),
        ValueInput::Name(text) => format!("{half} + ({text}){}", plus(k)),
    }))
}

/// The free stretch a placed part stands in (`centre` along the slot of
/// its joint `j`, from end A of a side `length` long), between the other
/// profiles on that slot: for an edit to Middle of Gap.
pub(crate) fn gap_of_part<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
    name: &str,
    j: &JointDef,
    centre: f64,
    length: Option<f64>,
) -> Result<SideGap, ModelError> {
    let GeomRef::Mount {
        feature,
        name: slot,
    } = &j.b.geometry
    else {
        return Err(refuse(format!(
            "{name} is not on a profile's side: Middle of Gap needs one"
        )));
    };
    let length = length.ok_or_else(|| refuse(format!("the profile {name} is on has no length")))?;
    let standing = standing_on(doc, &j.b.occurrence.0, *feature, slot, Some(component));
    gap_containing(&gaps_along(&standing, length), centre)
        .cloned()
        .ok_or_else(|| refuse(format!("{name} stands in no free stretch between profiles")))
}
