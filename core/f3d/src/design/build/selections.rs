// SPDX-License-Identifier: MIT
//! Selections of combines, splits, patterns, mirrors and holes
//! (mitcad#67) *(read from the reference models, whose dumps name the same
//! inputs, and checked on the corpus' items)*.
//!
//! A selection is a list input ([`BODY_INPUT`]: `u32 n | n refs` after
//! its root part) of body inputs ([`BODY_REF`], named by recipes and by
//! the items that made the bodies: `producers.rs`, mitcad#96), face
//! and edge inputs ([`FACE_REF`]) and entity inputs ([`ENTITY_REF`]),
//! whose target is a feature (a pattern's or mirror's features), an origin
//! or construction plane or axis, or a sketch entity.
//!
//! - Combine: after the root part `u32 operation (0 join, 1 cut, 2
//!   intersect) | u32 r | r refs | u8 keep tools | u8 | u32 1 | ref | u32
//!   | u32 0 | u32 n | n tool selections | u64 0 | u32 m | m body records
//!   (the bodies it consumes; none when the tools are kept) | u32 1 | ref
//!   target selection` (one body input per selection; the operation, the
//!   kept tools and the lists agree in every combine of the corpora, class
//!   version 1). The `r` references are body removals
//!   (`RemoveBodyFeature`) that consume tools of other components instead
//!   of the body records (mitcad#104: one combine of the corpora, `r` 0 in
//!   all others).
//! - Split body: `u8 extend tool | u32 1 | ref | u32 | u32 0 | u32 1 | ref
//!   tool selection | u32 0 | u32 m | m body records (the pieces it makes)
//!   | u32 0 | u32 1 | ref selection of the split bodies`.
//! - Patterns and mirrors: the first selection of the input list holds the
//!   objects; a mirror's second its plane; a pattern's axis or directions
//!   are direction inputs ([`DIRECTION_INPUT`]) naming an axis, an edge or
//!   a face, with the line they store (mitcad#96). After the item's root
//!   part and its first two references, `u32 0 | u32 n | n` body records:
//!   the new bodies it makes (a mirror of bodies that makes none joins
//!   them with their images, mitcad#96).
//! - Hole: its points are key point inputs ([`KEY_POINT`]: points in the
//!   component, cm); a byte after the root part is 1 for a hole through
//!   all (the second in class version 4, the sixth in version 7, from two
//!   designs only; the seventh in the others); the type follows from its
//!   parameters (counterbore or countersink sizes).

use serde_json::{Map, Value, json};

use super::super::classes::*;
use super::super::decode::TimelineEntry;
use super::super::ir::*;
use super::super::recipe;
use super::super::stream::{Segment, f64s_at, header_end, u32_at};
use super::joints::key_point;
use super::{Builder, edge_input, face_input, json_value};

/// A direction input ([`DIRECTION_INPUT`]) as a line: its point (cm) and
/// unit direction, both in the component. A construction axis' input
/// stores its direction times its length (code 7), so the direction is
/// normalised (mitcad#96).
fn direction_line(seg: &Segment, id: u64) -> Option<(Vec3, Vec3)> {
    if seg.guid_of(id) != Some(DIRECTION_INPUT) {
        return None;
    }
    let d = seg.data_of(id);
    let p = seg.root_part(d)?.end;
    let v = f64s_at(d, p + 4, 6)?;
    if !v.iter().all(|x| x.is_finite()) {
        return None;
    }
    let len = (v[3] * v[3] + v[4] * v[4] + v[5] * v[5]).sqrt();
    (len > 1e-9).then(|| ([v[0], v[1], v[2]], [v[3] / len, v[4] / len, v[5] / len]))
}

/// The end of a reference at `p`, resolving or not (a deleted object).
fn ref_end(seg: &Segment, d: &[u8], p: usize) -> Option<usize> {
    if let Some(r) = seg.ref_at(d, p) {
        return Some(r.end);
    }
    let q = p + 9 + if seg.long_refs { 40 } else { 0 };
    (*d.get(p)? == 1 && d.get(q..q + 2)? == [0, 0]).then_some(q + 2)
}

/// `u32 n | n refs` at `p`: the ids (`None` for one that does not
/// resolve) and the end.
fn ref_list(seg: &Segment, d: &[u8], p: usize) -> Option<(Vec<Option<u64>>, usize)> {
    let n = u32_at(d, p)?;
    if n > 10_000 {
        return None;
    }
    let mut q = p + 4;
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(seg.ref_at(d, q).map(|r| r.id));
        q = ref_end(seg, d, q)?;
    }
    Some((out, q))
}

/// The tools, consumed body records and target of a combine, or the tool,
/// records and split bodies of a split, from `p` (the tools' list).
struct Lists {
    first: Vec<Option<u64>>,
    records: Vec<Option<u64>>,
    last: Vec<Option<u64>>,
}

/// The three lists from `p`, with `before` bytes before the records' list
/// and `after` bytes after it.
fn lists_at(seg: &Segment, d: &[u8], p: usize, before: usize, after: usize) -> Option<Lists> {
    let (first, q) = ref_list(seg, d, p)?;
    let (records, q) = ref_list(seg, d, q + before)?;
    let (last, _) = ref_list(seg, d, q + after)?;
    Some(Lists {
        first,
        records,
        last,
    })
}

/// The end of an object's root part, also when its attributes include
/// `double` values (a hole's `SavedHoleDepthInNonDistanceExtentsCase`),
/// which [`Segment::root_part`] does not read.
fn root_end(seg: &Segment, d: &[u8]) -> Option<usize> {
    if let Some(r) = seg.root_part(d) {
        return Some(r.end);
    }
    let mut p = header_end(d)?;
    if *d.get(p)? != 0 || *d.get(p + 1)? != 1 {
        return None;
    }
    let n = u32_at(d, p + 2)?;
    p += 6;
    for _ in 0..n.min(1000) {
        p += 4 + u32_at(d, p)? as usize;
        let ln = u32_at(d, p)? as usize;
        let typ = d.get(p + 4..p + 4 + ln)?;
        p += 4 + ln;
        p += if typ.ends_with(b"double") || typ.ends_with(b"uint64") {
            8
        } else if typ.ends_with(b"bool") {
            1
        } else if typ.ends_with(b"IString") {
            4 + 2 * u32_at(d, p)? as usize
        } else {
            return None;
        };
    }
    (p <= d.len()).then_some(p)
}

impl Builder<'_> {
    /// The selections ([`BODY_INPUT`]) among an item's inputs, in order.
    fn selections(&self, it: &TimelineEntry) -> Vec<u64> {
        self.item_inputs(it)
            .into_iter()
            .filter(|&i| self.seg.guid_of(i) == Some(BODY_INPUT))
            .collect()
    }

    /// A member of a selection as a reference: a body, face or edge by its
    /// names, a feature, a plane or axis, or a sketch entity.
    fn member(&self, m: u64) -> Option<Reference> {
        let seg = self.seg;
        match seg.guid_of(m)? {
            BODY_REF => self.body_input(m),
            FACE_REF => match recipe::of_input(seg, m)?.kind.as_str() {
                "edge" => edge_input(seg, m),
                _ => face_input(seg, m),
            },
            ENTITY_REF => {
                let target = super::decode::target_id(seg, m)?;
                if self.pos_of.contains_key(&target) {
                    return Some(self.feature_ref(target));
                }
                self.entity_target(m)
            }
            _ => None,
        }
    }

    /// Every member of a selection; `None` unless all are known.
    fn members(&self, selection: u64) -> Option<Vec<Reference>> {
        self.input_list(selection)?
            .into_iter()
            .map(|m| self.member(m))
            .collect()
    }

    /// The members of a selection as bodies; `None` unless all are.
    fn bodies_of(&self, selection: Option<u64>) -> Option<Vec<Reference>> {
        let v = self.members(selection?)?;
        (!v.is_empty() && v.iter().all(|r| matches!(r, Reference::Body(_)))).then_some(v)
    }

    /// A combine's operation, tools kept, target and tools.
    pub(super) fn combine(&self, it: &TimelineEntry) -> Detail {
        let seg = self.seg;
        let d = seg.data_of(it.id);
        let mut map = Map::new();
        let Some(p) = seg.root_part(d).map(|r| r.end) else {
            return Detail::Other(map);
        };
        let operation = match u32_at(d, p) {
            Some(0) => Some("JoinFeatureOperation"),
            Some(1) => Some("CutFeatureOperation"),
            Some(2) => Some("IntersectFeatureOperation"),
            _ => None,
        };
        if let Some(o) = operation {
            map.insert("operation".into(), json!(o));
        }
        // The tools' removals from other components (mitcad#104): `u32 n |
        // n refs` to body removals (`RemoveBodyFeature`), which consume
        // tools there instead of the body records below.
        let removals = u32_at(d, p + 4).filter(|&n| n <= 100);
        let mut k = Some(p + 8);
        for _ in 0..removals.unwrap_or(0) {
            k = k.and_then(|q| ref_end(seg, d, q));
        }
        let Some(k) = k else {
            return Detail::Other(map);
        };
        let removed = removals.is_some_and(|n| n > 0);
        let keep = d.get(k).copied().filter(|&k| k <= 1);
        if let Some(k) = keep {
            map.insert("isKeepToolBodies".into(), json!(k == 1));
        }
        // `u32 1 | ref | u32 | u32 0`, then the lists.
        let lists = (u32_at(d, k + 2) == Some(1))
            .then(|| ref_end(seg, d, k + 6))
            .flatten()
            .and_then(|q| lists_at(seg, d, q + 8, 8, 0));
        if let Some(l) = lists
            && let [Some(target)] = l.last[..]
            && (l.records.is_empty() == (keep == Some(1))
                || (removed && keep == Some(0) && l.records.is_empty()))
        {
            let tools: Option<Vec<Reference>> = l
                .first
                .iter()
                .map(|s| self.bodies_of(*s))
                .collect::<Option<Vec<_>>>()
                .map(|v| v.into_iter().flatten().collect());
            if let (Some(mut target), Some(tools)) = (self.bodies_of(Some(target)), tools)
                && target.len() == 1
                && !tools.is_empty()
            {
                map.insert("targetBody".into(), json_value(&target.remove(0)));
                map.insert("toolBodies".into(), json_value(&tools));
            }
        }
        Detail::Other(map)
    }

    /// A split's tool, split bodies and whether the tool is extended.
    pub(super) fn split(&self, it: &TimelineEntry) -> Detail {
        let seg = self.seg;
        let d = seg.data_of(it.id);
        let mut map = Map::new();
        let Some(p) = seg.root_part(d).map(|r| r.end) else {
            return Detail::Other(map);
        };
        let extend = d.get(p).copied().filter(|&e| e <= 1);
        let lists = (u32_at(d, p + 1) == Some(1))
            .then(|| ref_end(seg, d, p + 5))
            .flatten()
            .and_then(|q| lists_at(seg, d, q + 8, 4, 4));
        if let Some(l) = lists
            && let ([Some(tool)], [Some(split)]) = (&l.first[..], &l.last[..])
        {
            if let Some(bodies) = self.bodies_of(Some(*split)) {
                map.insert("splitBodies".into(), json_value(&bodies));
            }
            // One plane, face or body.
            if let Some(mut t) = self.members(*tool).filter(|t| t.len() == 1) {
                map.insert("splittingTool".into(), json_value(&t.remove(0)));
            }
            if let Some(e) = extend {
                map.insert("isSplittingToolExtended".into(), json!(e == 1));
            }
        }
        Detail::Other(map)
    }

    /// A pattern's or mirror's objects (its first selection) and their
    /// kind.
    pub(super) fn pattern_objects(&self, it: &TimelineEntry) -> Option<(Vec<Reference>, String)> {
        let first = *self.selections(it).first()?;
        let objects = self.members(first)?;
        let kind = match objects.first()? {
            Reference::Feature(_) => "FeaturesPatternType",
            Reference::Body(_) => "BodiesPatternType",
            Reference::Face(_) => "FacesPatternType",
            _ => return None,
        };
        let same = objects
            .iter()
            .all(|r| std::mem::discriminant(r) == std::mem::discriminant(&objects[0]));
        same.then(|| (objects, kind.to_owned()))
    }

    /// A mirror's plane: the member of its second selection.
    pub(super) fn mirror_plane(&self, it: &TimelineEntry) -> Option<Reference> {
        let second = *self.selections(it).get(1)?;
        let mut v = self.members(second)?;
        (v.len() == 1 && matches!(v[0], Reference::ConstructionPlane(_) | Reference::Face(_)))
            .then(|| v.remove(0))
    }

    /// The direction inputs of an item: the axis, edge or face each names,
    /// and its stored direction (a unit vector) and point where it has
    /// them ([`direction_line`]).
    #[allow(clippy::type_complexity)]
    pub(super) fn directions(
        &self,
        it: &TimelineEntry,
    ) -> Vec<(Option<Reference>, Option<Vec3>, Option<Vec3>)> {
        self.item_inputs(it)
            .into_iter()
            .filter(|&i| self.seg.guid_of(i) == Some(DIRECTION_INPUT))
            .map(|i| {
                let entity = self
                    .seg
                    .ref_ids(self.seg.data_of(i))
                    .into_iter()
                    .find_map(|r| match self.seg.guid_of(r) {
                        Some(ENTITY_REF | FACE_REF) => self.member(r),
                        _ => None,
                    });
                let line = direction_line(self.seg, i);
                (entity, line.map(|(_, d)| d), line.map(|(p, _)| p))
            })
            .collect()
    }

    /// How many new bodies a pattern or mirror makes: after its root part
    /// two references (its data and the item's), then `u32 0 | u32 n | n`
    /// body records ([`BODY_RECORD`]). A mirror of bodies that makes none
    /// joins them with their images (mitcad#96: every mirror of the
    /// learning dump).
    pub(super) fn new_bodies(&self, it: &TimelineEntry) -> Option<usize> {
        let seg = self.seg;
        let d = seg.data_of(it.id);
        let first = seg.ref_at(d, seg.root_part(d)?.end)?;
        let second = seg.ref_at(d, first.end)?;
        let p = second.end;
        let n = u32_at(d, p + 4).filter(|&n| u32_at(d, p) == Some(0) && n <= 10_000)?;
        let mut q = p + 8;
        for _ in 0..n {
            let r = seg.ref_at(d, q)?;
            if seg.guid_of(r.id) != Some(BODY_RECORD) {
                return None;
            }
            q = r.end;
        }
        Some(n as usize)
    }

    /// A hole's points (its key point inputs, component space, cm).
    pub(super) fn hole_points(&self, it: &TimelineEntry) -> Vec<Vec3> {
        self.item_inputs(it)
            .into_iter()
            .filter_map(|i| key_point(self.seg, i).map(|(p, _)| p))
            .collect()
    }

    /// Whether a hole goes through all: a byte after its root part, the
    /// second in class version 4 and the sixth in version 7 (after a `u32
    /// 2`; mitcad#96, from two designs only: a lead), else the seventh.
    pub(super) fn hole_through_all(&self, it: &TimelineEntry) -> Option<bool> {
        let d = self.seg.data_of(it.id);
        let p = root_end(self.seg, d)?;
        let at = match it.class_version {
            4 => 1,
            7 => 5,
            _ => 6,
        };
        match d.get(p + at)? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
}

/// Fills a mirror's detail map with its objects and plane.
pub(super) fn mirror_map(b: &Builder, it: &TimelineEntry) -> Map<String, Value> {
    let mut map = Map::new();
    if let Some(p) = b.mirror_plane(it) {
        map.insert("mirrorPlane".into(), json_value(&p));
    }
    if let Some((objects, kind)) = b.pattern_objects(it) {
        map.insert("inputEntities".into(), json_value(&objects));
        map.insert("patternEntityType".into(), json!(kind));
    }
    map
}
