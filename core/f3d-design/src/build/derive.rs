// SPDX-License-Identifier: MIT
//! Derived designs (mitcad#324): a `DerivedContext` item brings entities of
//! another design into the item's component *(read from the corpora's
//! derive items: designs that derive bodies of the parts of an assembly
//! they were drawn in, and items that bring nothing)*.
//!
//! The item ([`DERIVED_CONTEXT`], class version 3) after its root part:
//!
//! ```text
//! str16 GUID | ref placement (the design in the source) | ref 074F9459 |
//! str16 GUID | 8 bytes | ref A22FD0FD or DF53910F | str16 document GUID |
//! ref component (01 id 01 key 00 str8 class 00: the source design's root
//!   component, through the key of the source document) |
//! str16 name | str16 name | 9 bytes | ref placement | 8 bytes |
//! u32 n | n × entity
//! entity: ref feature (B1E5B971, a base feature: the entity's own copy in
//!   this design, its bodies) | u8 | u32 m | m × ref source entity (in its
//!   own document: a body, D3937028) | [u8 1 | u64 | u8 | u8 | u32] |
//!   u8 | u32 k | k × str8 property linked (Appearance, Geometry,
//!   PhysicalMaterial, Properties, Transform) | 3 bytes or u8 1 + 2 refs |
//!   ref source entity again | ref placement (the source entity's
//!   occurrence in the source design)
//! ```
//!
//! Class version 2 (mitcad#2493: the two derives of a reported package,
//! saved by build 2.0.18719; none in the corpora) has no `ref A22FD0FD or
//! DF53910F` before the document GUID, one name followed by 11 bytes
//! before the second placement, and no `u8` between an entity's feature
//! and its count `m`; the rest reads as in version 3.
//!
//! The source design is the document the item names first (an assembly);
//! each entity is named in the document that holds it (a part the
//! assembly places), by its object id there. A placement ([`PLACEMENT`])
//! gives an occurrence path (GUIDs, from the source design's root down)
//! and a matrix: the item's first placement is where the deriving design
//! sits in the source design, each entity's where the entity's occurrence
//! is. The entity's own feature holds the copied bodies (body objects of
//! this design).

use serde_json::{Map, Value, json};

use super::super::classes::*;
use super::super::decode::TimelineEntry;
use super::super::ir::*;
use super::super::stream::{Segment, str16_at, u32_at};
use super::{Builder, inputs_of, json_value, rigid_matrix};

/// The source entity of a derive, in its own document.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DerivedEntity {
    /// The entity's own feature in this design (a base feature).
    pub feature: u64,
    /// The source document's key and the entity's object id and class
    /// there.
    pub key: u64,
    pub object: u64,
    pub class: String,
    /// The properties that stay linked.
    pub links: Vec<String>,
    /// Its placement object.
    pub placement: Option<u64>,
}

/// A derive item's record.
#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct Derive {
    /// The source design: its document key and root component id there.
    pub source: Option<(u64, u64)>,
    /// The names after the source (the document's name, twice).
    pub names: Vec<String>,
    /// The placements before and after the source.
    pub placements: Vec<u64>,
    pub entities: Vec<DerivedEntity>,
}

/// A `str8` (u32 length, ASCII) at `p` made of letters.
fn word_at(d: &[u8], p: usize) -> Option<(String, usize)> {
    let n = u32_at(d, p)? as usize;
    if n == 0 || n > 64 {
        return None;
    }
    let s = d.get(p + 4..p + 4 + n)?;
    s.iter()
        .all(u8::is_ascii_alphabetic)
        .then(|| (String::from_utf8_lossy(s).into_owned(), p + 4 + n))
}

/// The words (`str8` of letters) in `d[start..end]`.
fn words_in(d: &[u8], start: usize, end: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut p = start;
    while p + 4 < end {
        match word_at(d, p) {
            Some((w, e)) if e <= end => {
                out.push(w);
                p = e;
            }
            _ => p += 1,
        }
    }
    out
}

/// Reads a derive item's record (layout in the module's comment).
pub(super) fn derive(seg: &Segment, id: u64) -> Option<Derive> {
    let o = seg.object(id)?;
    let d = seg.data(o);
    let end = seg.main_end(o).min(d.len());
    let mut out = Derive::default();
    // The source design's root component, through its document's key.
    let (at, source) = seg
        .context_refs_in(d, 0, end)
        .into_iter()
        .find(|(_, r)| r.guid == COMPONENT)?;
    out.source = Some((source.key, source.id));
    let mut p = source.end;
    while let Some((name, e)) = str16_at(d, p).filter(|(s, _)| !s.is_empty()) {
        out.names.push(name);
        p = e;
        if out.names.len() == 2 {
            break;
        }
    }
    for (q, r) in seg.refs_in(d, 0, end) {
        if seg.guid_of(r.id) == Some(PLACEMENT) && (q < at || out.placements.len() < 2) {
            out.placements.push(r.id);
            if q > at {
                p = r.end;
                break;
            }
        }
    }
    if out.placements.len() < 2 {
        return Some(out);
    }
    let n = u32_at(d, p + 8)? as usize;
    if n > 10_000 {
        return None;
    }
    p += 12;
    // Class version 2 has no byte between an entity's feature and its
    // count (mitcad#2493).
    let gap = usize::from(seg.version(o)? != 2);
    for _ in 0..n {
        let feature = seg.ref_at(d, p)?;
        if !seg.is_kind_of(feature.id, BASE_FEATURE) {
            return None;
        }
        let m = u32_at(d, feature.end + gap)? as usize;
        let first = seg.context_ref_at(d, feature.end + gap + 4)?;
        let mut q = first.end;
        for _ in 1..m {
            q = seg.context_ref_at(d, q)?.end;
        }
        // The entity ends with its placement.
        let (at, placement) = seg
            .refs_in(d, q, end)
            .into_iter()
            .find(|(_, r)| seg.guid_of(r.id) == Some(PLACEMENT))?;
        out.entities.push(DerivedEntity {
            feature: feature.id,
            key: first.key,
            object: first.id,
            class: first.guid.clone(),
            links: words_in(d, q, at),
            placement: Some(placement.id),
        });
        p = placement.end;
    }
    Some(out)
}

/// The IR `kind` of a source entity's class.
fn entity_kind(class: &str) -> &'static str {
    match class {
        BODY => "body",
        COMPONENT => "component",
        _ => "entity",
    }
}

impl Builder<'_> {
    /// A placement's matrix (cm) and occurrence path.
    fn derive_placement(&self, placement: u64) -> Value {
        let mut v = Map::new();
        if let Some(m) = rigid_matrix(self.seg.data_of(placement)) {
            v.insert("transform".into(), json_value(&m));
        }
        if let Some(o) = self.placement_occurrence(placement) {
            v.insert("occurrence".into(), o);
        }
        v.insert("_f3d".into(), json!({"object_id": placement}));
        Value::Object(v)
    }

    /// The document a key names: its URN and GUID (lower case).
    fn external_document(&self, key: u64) -> Value {
        let mut v = json!({"key": format!("{key:016x}")});
        if let Some(x) = self.seg.meta.external.iter().find(|x| x.key == key) {
            v["urn"] = json!(x.urn);
            v["document_guid"] = json!(x.guid_c.to_lowercase());
        }
        v
    }

    /// A derive item's detail (SCHEMA.md §5.3, `DerivedContext`).
    pub(super) fn derived_context(&self, it: &TimelineEntry) -> Detail {
        let mut map = Map::new();
        let Some(record) = derive(self.seg, it.id) else {
            map.insert(
                "_f3d".into(),
                json!({"why": "the derive record does not read"}),
            );
            return Detail::Other(map);
        };
        if let Some((key, component)) = record.source {
            let mut source = json!({"kind": "design"});
            if let Some(name) = record.names.first() {
                source["name"] = json!(name);
            }
            let mut f3d = self.external_document(key);
            f3d["component_object"] = json!(component);
            source["_f3d"] = f3d;
            map.insert("source".into(), source);
        }
        let placements: Vec<Value> = record
            .placements
            .iter()
            .map(|&p| self.derive_placement(p))
            .collect();
        map.insert("placements".into(), json!(placements));
        let entities: Vec<Value> = record
            .entities
            .iter()
            .map(|e| {
                let mut f3d = self.external_document(e.key);
                f3d["object_id"] = json!(e.object);
                f3d["class"] = json!(e.class);
                f3d["feature"] = json!(e.feature);
                // The feature's bodies: body objects of this design.
                f3d["bodies"] = json!(inputs_of(self.seg, e.feature, BODY));
                f3d["links"] = json!(e.links);
                let mut v = json!({"kind": entity_kind(&e.class), "_f3d": f3d});
                if let Some(p) = e.placement {
                    v["placement"] = self.derive_placement(p);
                }
                v
            })
            .collect();
        map.insert("entities".into(), json!(entities));
        Detail::Other(map)
    }
}
