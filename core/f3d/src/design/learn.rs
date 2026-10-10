// SPDX-License-Identifier: MIT
//! The raw records behind timeline items, for the import's learning dump
//! (`core/import/src/learn.rs`, mitcad#96): an item's object and the input
//! objects it refers to, each with its bytes and the tokens the framing
//! explains (header, root part, references, strings). What lies between
//! the tokens is what the decoder does not explain yet; the hypothesis
//! tester (`tools/f3d-learn/learn.py`) reads its fields.
//!
//! Also the ids the sketch entities carry (`crv_primary_id`, ...), by
//! which feature inputs name them.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{Map, Value, json};

use super::classes::*;
use super::stream::{AttrValue, Segment, header_end, hex, text_at};

/// How deep the input objects of an item are followed.
const DEPTH: usize = 3;
/// The most objects of one item's record.
const MAX_OBJECTS: usize = 48;
/// Objects longer than this keep their tokens but not their bytes.
const MAX_BYTES: usize = 16 * 1024;

/// Classes whose objects are not part of an item's record: other items,
/// sketches and their entities, components, parameters (decoded on their
/// own), timelines and feature lists.
fn stops_at(seg: &Segment, id: u64) -> bool {
    let Some(g) = seg.guid_of(id) else {
        return true;
    };
    if id == 0 || seg.is_kind_of(id, SKETCH_CURVE) || classes_is_point(g) {
        return true;
    }
    matches!(
        g,
        TIMELINE
            | FEATURE_MANAGER
            | FEATURE_LIST
            | PARAMETER
            | PARAMETER_LIST
            | HEALTH
            | COMPONENT
            | OCCURRENCE
            | OCCURRENCE_CONTAINER
            | BREP_REF
            | BLOB_HOLDER
            | SKETCH
            | SKETCH_FEATURE
            | SKETCH_TEXT
            | SKETCH_CONSTRAINT
            | SKETCH_DIMENSION
            | LINEAR_DIMENSION
            | ANGULAR_DIMENSION
            | RADIAL_DIMENSION
            | DIAMETER_DIMENSION
            | SKETCH_TRANSFORM
            | ORIGIN_PLANE
            | ORIGIN_AXIS
            | ORIGIN_POINT
    )
}

fn classes_is_point(g: &str) -> bool {
    super::classes::is_point_class(Some(g))
}

/// The tokens the framing explains in an object's data, in order, as
/// `{"at", "end", "t", ...}`: `header`, `root` (with its attributes),
/// `ref` (`id`, `class`), `str16` (`text`) and `sub` (the sub-chunk's
/// start, zero length).
pub fn tokens(seg: &Segment, d: &[u8], sub: usize) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(h) = header_end(d) else {
        return out;
    };
    out.push(json!({"at": 0, "end": h, "t": "header"}));
    let mut p = h;
    if let Some(root) = seg.root_part(d) {
        let mut attrs = Map::new();
        for a in &root.attrs {
            let v = match &a.value {
                AttrValue::U64(v) => json!(v),
                AttrValue::Bool(b) => json!(b),
                AttrValue::Text(s) => json!(s),
            };
            attrs.insert(a.key.clone(), v);
        }
        let refs: Vec<Value> = root
            .refs
            .iter()
            .map(|r| r.map_or(Value::Null, |id| json!(id)))
            .collect();
        out.push(json!({"at": h, "end": root.end, "t": "root", "refs": refs, "attrs": attrs}));
        p = root.end;
    }
    while p < d.len() {
        if sub > 0 && p == sub {
            out.push(json!({"at": p, "end": p, "t": "sub"}));
        }
        if d[p] == 1
            && let Some(r) = seg.ref_at(d, p)
        {
            let class = seg.guid_of(r.id).unwrap_or("?");
            let mut t = json!({"at": p, "end": r.end, "t": "ref", "id": r.id,
                               "class": class.get(..8).unwrap_or(class)});
            if let Some(c) = r.context {
                t["context"] = json!(c);
            }
            out.push(t);
            p = r.end;
            continue;
        }
        if let Some((s, e)) = text_at(d, p)
            && plausible_text(&s)
        {
            out.push(json!({"at": p, "end": e, "t": "str16", "text": s}));
            p = e;
            continue;
        }
        p += 1;
    }
    out
}

/// A `str16` that is text rather than numbers that read as one: several
/// characters below U+2000, or one alphanumeric ASCII character.
fn plausible_text(s: &str) -> bool {
    let n = s.chars().count();
    s.chars().all(|c| (' '..'\u{2000}').contains(&c))
        && (n > 1 || s.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// The record of a timeline item: its object (path `item`) and the input
/// objects it refers to, followed [`DEPTH`] references deep (paths
/// `item>0897AF07#0>C46D3EEB#1`: the class's first eight digits and the
/// position among the parent's references to that class). Each object:
/// `path`, `id`, `class`, `module`, `version`, `len`, `hex` (left out over
/// [`MAX_BYTES`]), `sub` (the sub-chunk's start, 0 without), `tokens`.
/// Other timeline items, sketches and their entities, parameters and the
/// like are not followed ([`stops_at`]); an object met again is listed
/// once, at its first path.
pub fn item_record(seg: &Segment, item: u64, items: &HashSet<u64>) -> Value {
    let mut objects = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([(item, "item".to_owned(), 0usize)]);
    seen.insert(item);
    while let Some((id, path, depth)) = queue.pop_front() {
        if objects.len() >= MAX_OBJECTS {
            break;
        }
        let Some(o) = seg.object(id) else { continue };
        let d = seg.data(o);
        let class = seg.class(o);
        let sub = seg.sub_start(o);
        let toks = tokens(seg, d, sub);
        let mut obj = json!({
            "path": path,
            "id": id,
            "class": class.map_or("?", |c| c.guid.as_str()),
            "module": class.map_or("", |c| c.module.as_str()),
            "version": class.map_or(0, |c| c.version),
            "len": d.len(),
            "sub": sub,
        });
        if d.len() <= MAX_BYTES {
            obj["hex"] = json!(hex(d));
            if seg.guid_of(id) == Some(super::recipe::RECIPE)
                && let Some(details) = super::recipe::parse_details(seg, d)
            {
                obj["recipe"] = json!({
                    "kind": details.recipe.kind,
                    "entities": details.recipe.entities,
                    "secondary_entities": details.secondary_entities,
                    "tail_offset": details.tail_offset,
                    "tail_hex": hex(&details.tail),
                    "tail_interpretation": "unverified",
                });
            }
        }
        if depth < DEPTH {
            let mut ordinal: HashMap<String, usize> = HashMap::new();
            for t in &toks {
                if t["t"] != "ref" {
                    continue;
                }
                let Some(r) = t["id"].as_u64() else { continue };
                let c8 = t["class"].as_str().unwrap_or("?").to_owned();
                let k = ordinal.entry(c8.clone()).or_default();
                let child = format!("{path}>{c8}#{k}");
                *k += 1;
                if seen.contains(&r) || items.contains(&r) || stops_at(seg, r) {
                    continue;
                }
                seen.insert(r);
                queue.push_back((r, child, depth + 1));
            }
        }
        obj["tokens"] = Value::Array(toks);
        objects.push(obj);
    }
    json!({"long_refs": seg.long_refs, "objects": objects})
}

/// The sketch object (`44A64366`) of a sketch feature (`8DA771B7`).
pub fn sketch_of_feature(seg: &Segment, feature: u64) -> Option<u64> {
    seg.objects_of(SKETCH)
        .find(|s| seg.ref_ids(seg.data(s)).contains(&feature))
        .map(|s| s.id)
}

/// The entities of a sketch by their sketch-local ids (`c5`, `p3`): the
/// object id, class (first eight digits) and the `u64` root-part
/// attributes (`crv_primary_id`, `crv_secondary_id`, `pt_tag`,
/// `EntityGenesis`, ...). `local_ids` as
/// [`super::sketch::sketch_details`] gives them.
pub fn sketch_entities(
    seg: &Segment,
    sketch: u64,
    local_ids: &HashMap<u64, (u64, String)>,
) -> Map<String, Value> {
    let mut out = Map::new();
    for (&e, (s, local)) in local_ids {
        if *s != sketch || !(local.starts_with('c') || local.starts_with('p')) {
            continue;
        }
        let class = seg.guid_of(e).unwrap_or("?");
        let mut attrs = Map::new();
        if let Some(root) = seg.root_part(seg.data_of(e)) {
            for a in &root.attrs {
                if let AttrValue::U64(v) = a.value {
                    attrs.insert(a.key.clone(), json!(v));
                }
            }
        }
        out.insert(
            local.clone(),
            json!({"object": e, "class": class.get(..8).unwrap_or(class), "attrs": attrs}),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::tests::Doc;

    #[test]
    fn records_follow_inputs_and_stop_at_items() {
        // An extrude with a profile input, which lists one profile id, and
        // a reference to another item.
        let mut d = Doc::default();
        let root = "98542EB9-A4F2-4137-A808-DBB5B3CD6159";
        d.class(root, "", 0);
        for g in [EXTRUDE, PROFILE, PROFILE_ID] {
            d.class(g, root, 1);
        }
        let r = |id: u64| {
            let mut b = vec![1];
            b.extend(id.to_le_bytes());
            b.extend([0, 0]);
            b
        };
        let mut item = vec![0, 0];
        item.extend(r(71));
        item.extend(7u32.to_le_bytes());
        item.extend(r(80));
        d.obj(60, EXTRUDE, item);
        let mut profile = vec![0, 0];
        profile.extend(r(60));
        profile.extend(1u32.to_le_bytes());
        profile.extend(r(72));
        profile.extend(228u32.to_le_bytes());
        d.obj(71, PROFILE, profile);
        d.obj(72, PROFILE_ID, vec![0, 0, 9, 0, 0, 0]);
        d.obj(80, EXTRUDE, vec![0, 0]);
        let seg = d.segment();
        let items = HashSet::from([60, 80]);
        let rec = item_record(&seg, 60, &items);
        let objects = rec["objects"].as_array().unwrap();
        let paths: Vec<&str> = objects
            .iter()
            .map(|o| o["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            ["item", "item>0897AF07#0", "item>0897AF07#0>C46D3EEB#0"]
        );
        let toks = objects[1]["tokens"].as_array().unwrap();
        let kinds: Vec<&str> = toks.iter().map(|t| t["t"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["header", "root", "ref", "ref"]);
        assert_eq!(toks[3]["id"], 72);
        // The u32 after the last reference is what no token explains.
        let end = toks[3]["end"].as_u64().unwrap() as usize;
        assert_eq!(objects[1]["len"].as_u64().unwrap() as usize, end + 4);
        assert!(objects[0]["hex"].as_str().unwrap().len() > 10);
    }

    #[test]
    fn text_needs_more_than_a_digit_pair() {
        assert!(plausible_text("201"));
        assert!(plausible_text("a"));
        assert!(!plausible_text("\u{1}"));
        assert!(!plausible_text("\u{4e00}"));
    }
}
