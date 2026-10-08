// SPDX-License-Identifier: MIT
//! Inputs of sweeps, pipes and lofts (mitcad#34): profiles, paths, guide
//! rails, loft sections, centre lines and rails, end conditions and
//! operations *(read from the reference models, whose dumps name the same
//! inputs, and checked on the corpus' and other designs' sweeps, pipes and
//! lofts)*.
//!
//! A path, a rail or a loft section is a list input ([`BODY_INPUT`]: `u32 n
//! | n refs` after its root part) of
//! - entity inputs ([`ENTITY_REF`]) whose target is a sketch curve
//!   ([`SKETCH_CURVE_ID`]) or point ([`SKETCH_POINT_ID`]), named by the
//!   ids the sketch entities carry as root-part attributes
//!   (`crv_primary_id` and `crv_secondary_id`, `pt_tag`);
//! - edge inputs ([`FACE_REF`] with an edge recipe), found in the history
//!   by [`crate::design::inputs`];
//! - a profile source ([`PROFILE_SOURCE`]) for a loft section of a
//!   sketch profile.
//!
//! Inputs of other kinds (edges of other components through their
//! occurrences, faces as a sweep's guide surfaces) are left out, so that
//! the item is not translated.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::super::classes::*;
use super::super::decode::{ParameterEntry, TimelineEntry};
use super::super::ir::*;
use super::super::stream::{AttrValue, Segment, i32_at, u32_at, u64_at};
use super::{Builder, edge_input, face_input, json_value, paramref, source_sketch};

/// Sketch entities by the ids feature inputs name them with, per sketch
/// object; `None` where two entities share them.
#[derive(Default)]
pub(super) struct EntityTags {
    /// (sketch, `crv_primary_id`, `crv_secondary_id`) -> curve object.
    curves: HashMap<(u64, u64, u64), Option<u64>>,
    /// (sketch, `pt_tag`) -> point object.
    points: HashMap<(u64, u64), Option<u64>>,
}

impl EntityTags {
    /// The tags of the sketch entities (`local_ids`: entity -> (sketch,
    /// sketch-local id)).
    pub(super) fn new(seg: &Segment, local_ids: &HashMap<u64, (u64, String)>) -> Self {
        let mut out = Self::default();
        for (&e, (sketch, _)) in local_ids {
            let Some(root) = seg.root_part(seg.data_of(e)) else {
                continue;
            };
            let attr = |key: &str| {
                root.attrs
                    .iter()
                    .find(|a| a.key == key)
                    .and_then(|a| match a.value {
                        AttrValue::U64(v) => Some(v),
                        _ => None,
                    })
            };
            if let Some(primary) = attr("crv_primary_id") {
                let key = (*sketch, primary, attr("crv_secondary_id").unwrap_or(0));
                out.curves
                    .entry(key)
                    .and_modify(|v| *v = None)
                    .or_insert(Some(e));
            } else if let Some(tag) = attr("pt_tag") {
                out.points
                    .entry((*sketch, tag))
                    .and_modify(|v| *v = None)
                    .or_insert(Some(e));
            }
        }
        out
    }
}

/// A reference or a null reference (`00`) at `p`, and the end.
fn ref_or_null(seg: &Segment, d: &[u8], p: usize) -> Option<(Option<u64>, usize)> {
    if *d.get(p)? == 0 {
        return Some((None, p + 1));
    }
    let r = seg.ref_at(d, p)?;
    Some((Some(r.id), r.end))
}

/// `n` references or null references from `p`, and the end.
fn ref_slots(seg: &Segment, d: &[u8], mut p: usize, n: usize) -> Option<(Vec<Option<u64>>, usize)> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let (r, q) = ref_or_null(seg, d, p)?;
        out.push(r);
        p = q;
    }
    Some((out, p))
}

/// The operation by its code (as an extrusion's: 1 join, 2 cut, 4 new
/// body).
fn operation(code: u32) -> Option<String> {
    match code {
        1 => Some("JoinFeatureOperation".into()),
        2 => Some("CutFeatureOperation".into()),
        4 => Some("NewBodyFeatureOperation".into()),
        _ => None,
    }
}

/// A loft's end condition: `i32 | u64 0 | ref angle | u32 kind | ref
/// weight` (references to parameter holders, or null).
struct Condition {
    angle: Option<u64>,
    kind: u32,
    weight: Option<u64>,
}

fn condition_at(seg: &Segment, d: &[u8], p: usize) -> Option<(Condition, usize)> {
    i32_at(d, p)?;
    if u64_at(d, p + 4)? != 0 {
        return None;
    }
    let (angle, q) = ref_or_null(seg, d, p + 12)?;
    let kind = u32_at(d, q)?;
    let (weight, q) = ref_or_null(seg, d, q + 4)?;
    Some((
        Condition {
            angle,
            kind,
            weight,
        },
        q,
    ))
}

impl Builder<'_> {
    /// The parameter a holder holds.
    fn held(&self, holder: Option<u64>) -> Option<&ParameterEntry> {
        let h = holder?;
        self.dec.parameters.iter().find(|p| p.holder == Some(h))
    }

    /// The members of a list input (`u32 n | n refs` after its root part).
    pub(super) fn input_list(&self, holder: u64) -> Option<Vec<u64>> {
        if self.seg.guid_of(holder) != Some(BODY_INPUT) {
            return None;
        }
        let d = self.seg.data_of(holder);
        let p = self.seg.root_part(d)?.end;
        self.seg.ref_list_at(d, p).map(|(v, _)| v)
    }

    /// A sketch entity as a reference.
    fn sketch_entity(&self, e: u64) -> Option<Reference> {
        let (sid, lid) = self.local_ids.get(&e)?;
        let f = self.feat_of_sk.get(sid).copied();
        let object_type = if is_point_class(self.seg.guid_of(e)) {
            Some("SketchPoint".to_owned())
        } else {
            self.curve_types.get(&(*sid, lid.clone())).cloned()
        };
        Some(Reference::SketchEntity(SketchEntityRef {
            sketch: Some(self.name(f)),
            sketch_timeline_index: Some(self.pos(f)),
            id: Some(Some(lid.clone())),
            object_type,
            ..SketchEntityRef::default()
        }))
    }

    /// The sketch curve or point an entity input names.
    pub(super) fn input_entity(&self, input: u64) -> Option<Reference> {
        let seg = self.seg;
        for r in seg.ref_ids(seg.data_of(input)) {
            let d = seg.data_of(r);
            let e = match seg.guid_of(r) {
                Some(SKETCH_CURVE_ID) => {
                    let p = seg.root_part(d)?.end;
                    let (secondary, sketch, primary) =
                        (u64_at(d, p)?, u64_at(d, p + 8)?, u64_at(d, p + 16)?);
                    *self.tags.curves.get(&(sketch, primary, secondary))?
                }
                Some(SKETCH_POINT_ID) => {
                    let p = seg.root_part(d)?.end;
                    let (sketch, tag) = (u64_at(d, p)?, u64_at(d, p + 8)?);
                    *self.tags.points.get(&(sketch, tag))?
                }
                _ => continue,
            };
            return self.sketch_entity(e?);
        }
        None
    }

    /// A path of a list input: its sketch curves or edges as `PathEntity`
    /// items; `None` unless every member is one.
    fn path(&self, holder: u64) -> Option<Value> {
        let items = self
            .input_list(holder)?
            .into_iter()
            .map(|i| {
                let entity = match self.seg.guid_of(i) {
                    Some(ENTITY_REF) => self.input_entity(i)?,
                    Some(FACE_REF) => edge_input(self.seg, i)?,
                    _ => return None,
                };
                Some(json!({"_type": "PathEntity", "entity": json_value(&entity)}))
            })
            .collect::<Option<Vec<Value>>>()?;
        (!items.is_empty()).then_some(Value::Array(items))
    }

    /// A path that must be there: its items, else a value the import
    /// refuses (an input of a kind not decoded).
    fn path_or_error(&self, holder: u64) -> Value {
        self.path(holder)
            .unwrap_or_else(|| json!({"_error": "not decoded"}))
    }

    /// Profile references of a profile source: one per selected profile
    /// (`C46D3EEB`) it refers to, at least one.
    fn source_profiles(&self, src: u64) -> Option<Vec<Reference>> {
        let seg = self.seg;
        let sketch = source_sketch(seg, src)?;
        let mut ids = seg.ref_ids(seg.data_of(src));
        ids.retain(|&r| seg.guid_of(r) == Some(PROFILE_ID));
        ids.sort_unstable();
        ids.dedup();
        let f = self.feat_of_sk.get(&sketch).copied();
        let r = Reference::Profile(ProfileRef {
            sketch: Some(self.name(f)),
            sketch_timeline_index: Some(self.pos(f)),
            ..ProfileRef::default()
        });
        Some(vec![r; ids.len().max(1)])
    }

    /// The profiles of a sweep: of the first profile source among its
    /// inputs, or in a list input of them.
    fn input_profiles(&self, it: &TimelineEntry) -> Option<Vec<Reference>> {
        let is_source =
            |i: u64| matches!(self.seg.guid_of(i), Some(PROFILE_SOURCE | PROFILE_SOURCE_2));
        self.item_inputs(it).into_iter().find_map(|i| {
            if is_source(i) {
                return self.source_profiles(i);
            }
            let list = self.input_list(i)?;
            list.into_iter()
                .find(|&s| is_source(s))
                .and_then(|s| self.source_profiles(s))
        })
    }

    /// The references after the root part, from the first parameter holder
    /// on: `n` slots.
    fn slots(&self, it: &TimelineEntry, n: usize) -> Option<(Vec<Option<u64>>, usize)> {
        let seg = self.seg;
        let o = seg.object(it.id)?;
        let d = seg.data(o);
        let start = seg.root_part(d)?.end;
        let (first, _) = seg
            .refs_in(d, start, seg.main_end(o))
            .into_iter()
            .find(|(_, r)| seg.guid_of(r.id) == Some(PARAMETER_HOLDER))?;
        ref_slots(seg, d, first, n)
    }

    /// A sweep's operation, profiles, path, guide rail and guide surfaces.
    /// After the root part *(class version 6)*: `u32 operation | u8 | u32 |
    /// 5 × u8 | f64[3]`, then `ref` × 10: the holders of `AgainstDistance`,
    /// `AgainstRailDistance`, `AlongDistance` and `AlongRailDistance`, the
    /// guide surfaces (null without), the path, the guide rail (null
    /// without), the profile input (`B3E4AB56`), and the holders of
    /// `TaperAngle` and `TwistAngle`.
    pub(super) fn sweep_inputs(&self, it: &TimelineEntry, x: &mut SweepDetail) {
        let seg = self.seg;
        let Some(o) = seg.object(it.id) else { return };
        let d = seg.data(o);
        if let Some(code) = seg.root_part(d).and_then(|r| u32_at(d, r.end)) {
            x.operation = operation(code);
        }
        x.profile = self.input_profiles(it);
        let Some((slots, _)) = self.slots(it, 10) else {
            return;
        };
        let holders = slots[..4]
            .iter()
            .all(|s| s.is_some_and(|h| seg.guid_of(h) == Some(PARAMETER_HOLDER)));
        let Some(path) = slots[5].filter(|&p| holders && seg.guid_of(p) == Some(BODY_INPUT)) else {
            return;
        };
        x.path = self.path(path);
        if let Some(rail) = slots[6] {
            x.guide_rail = Some(self.path_or_error(rail));
        }
        if let Some(surfaces) = slots[4] {
            let faces: Vec<Value> = self
                .input_list(surfaces)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|i| face_input(seg, i))
                .map(|f| json_value(&f))
                .collect();
            let faces = if faces.is_empty() {
                json!([{"_error": "not decoded"}])
            } else {
                Value::Array(faces)
            };
            x.other.insert("guideSurfaces".to_owned(), faces);
        }
    }

    /// A pipe's operation and path. After the root part *(class version 2;
    /// one design)*: `u8 0 | u32 operation | u8 | u8`, then `ref` × 7: the
    /// holders of `AgainstDistance` and `AlongDistance`, the section input
    /// (`9414C87F`), the holders of `SectionSize` and `SectionThickness`, a
    /// null reference and the path. Section type and hollowness are not
    /// decoded.
    pub(super) fn pipe_inputs(&self, it: &TimelineEntry, x: &mut PipeDetail) {
        let seg = self.seg;
        let Some(o) = seg.object(it.id) else { return };
        let d = seg.data(o);
        if let Some(root) = seg.root_part(d)
            && d.get(root.end) == Some(&0)
            && let Some(code) = u32_at(d, root.end + 1)
        {
            x.operation = operation(code);
        }
        let Some((slots, _)) = self.slots(it, 7) else {
            return;
        };
        if let Some(path) = slots[6].filter(|&p| seg.guid_of(p) == Some(BODY_INPUT)) {
            x.path = self.path(path);
        }
    }

    /// A loft (class version 9). After the root part:
    ///
    /// ```text
    /// u8 × 4 | u32 operation | ref centre line (null without)
    /// condition of the last section | u32 n | n × ref rail
    /// u8 | u32 | u8 | ref (B0387387) | u32 n | n × ref section (2F3200BA)
    /// condition of the first section | ...
    /// ```
    ///
    /// A condition is `i32 | u64 0 | ref angle | u32 kind | ref weight`;
    /// kinds: 0 free, 1 tangent, 2 smooth, 3 direction, 4 sharp point
    /// *(assumed: no design has one)*, 5 tangent point. Closed lofts are not
    /// decoded.
    pub(super) fn loft(&self, it: &TimelineEntry) -> Detail {
        let mut map = Map::new();
        if let Some(fields) = self.loft_fields(it) {
            map = fields;
        }
        Detail::Other(map)
    }

    fn loft_fields(&self, it: &TimelineEntry) -> Option<Map<String, Value>> {
        let seg = self.seg;
        let o = seg.object(it.id)?;
        let d = seg.data(o);
        let p = seg.root_part(d)?.end;
        let mut map = Map::new();
        if let Some(op) = operation(u32_at(d, p + 4)?) {
            map.insert("operation".to_owned(), json!(op));
        }
        let (centerline, q) = ref_or_null(seg, d, p + 8)?;
        let (last, q) = condition_at(seg, d, q)?;
        let n = u32_at(d, q)? as usize;
        if n > 64 {
            return None;
        }
        let (rails, q) = ref_slots(seg, d, q + 4, n)?;
        // The sections: the first list of section references after them.
        let (at, _) = seg
            .refs_in(d, q, seg.main_end(o))
            .into_iter()
            .find(|(_, r)| seg.guid_of(r.id) == Some(LOFT_SECTION))?;
        let (sections, q) = seg.ref_list_at(d, at.checked_sub(4)?)?;
        if sections.len() < 2
            || sections
                .iter()
                .any(|&s| seg.guid_of(s) != Some(LOFT_SECTION))
        {
            return None;
        }
        let (first, _) = condition_at(seg, d, q)?;
        let count = sections.len();
        let list: Vec<Value> = sections
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let mut section = json!({"_type": "LoftSection", "index": i});
                if let Some(entity) = self.section_entity(s) {
                    section["entity"] = entity;
                }
                let condition = match i {
                    0 => Some(&first),
                    i if i + 1 == count => Some(&last),
                    _ => None,
                };
                if let Some(c) = condition.and_then(|c| self.condition(c)) {
                    section["endCondition"] = c;
                }
                section
            })
            .collect();
        map.insert("loftSections".to_owned(), Value::Array(list));
        let guides: Vec<Value> = match centerline {
            Some(c) => vec![self.path_or_error(c)],
            None => rails
                .iter()
                .map(|r| match r {
                    Some(r) => self.path_or_error(*r),
                    None => json!({"_error": "not decoded"}),
                })
                .collect(),
        };
        map.insert("centerLineOrRails".to_owned(), Value::Array(guides));
        map.insert(
            "centerLineOrRails.isCenterLine".to_owned(),
            json!(centerline.is_some()),
        );
        Some(map)
    }

    /// A loft section's entity: a sketch profile, a point, or a path of a
    /// body's edges.
    fn section_entity(&self, section: u64) -> Option<Value> {
        let seg = self.seg;
        let list = seg
            .ref_ids(seg.data_of(section))
            .into_iter()
            .find(|&r| seg.guid_of(r) == Some(BODY_INPUT))?;
        let members = self.input_list(list)?;
        match members.as_slice() {
            [s] if matches!(seg.guid_of(*s), Some(PROFILE_SOURCE | PROFILE_SOURCE_2)) => self
                .source_profiles(*s)?
                .into_iter()
                .next()
                .map(|r| json_value(&r)),
            // A point (a curve is no section Mitcad takes).
            [e] if seg.guid_of(*e) == Some(ENTITY_REF) => {
                let r = self.input_entity(*e)?;
                let point = matches!(&r, Reference::SketchEntity(s)
                    if s.object_type.as_deref() == Some("SketchPoint"));
                point.then(|| json_value(&r))
            }
            [f] if face_input(seg, *f).is_some() => face_input(seg, *f).map(|r| json_value(&r)),
            _ => self.path(list),
        }
    }

    /// A condition as the dump's `endCondition`; none for a kind not known.
    fn condition(&self, c: &Condition) -> Option<Value> {
        let param = |h: Option<u64>| self.held(h).map(|p| json_value(&paramref(p)));
        let (kind, angle, weight) = match c.kind {
            0 => ("LoftFreeEndCondition", false, false),
            1 => ("LoftTangentEndCondition", false, true),
            2 => ("LoftSmoothEndCondition", false, true),
            3 => ("LoftDirectionEndCondition", true, true),
            4 => ("LoftPointSharpEndCondition", false, false),
            5 => ("LoftPointTangentEndCondition", false, true),
            _ => return None,
        };
        let mut v = json!({"_type": kind});
        if angle && let Some(a) = param(c.angle) {
            v["angle"] = a;
        }
        if weight && let Some(w) = param(c.weight) {
            v["weight"] = w;
        }
        Some(v)
    }
}
