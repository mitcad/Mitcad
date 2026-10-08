// SPDX-License-Identifier: MIT
//! The part's design as the import's dump IR (`mitcad_f3d::design::ir::Dump`,
//! `core/import/SCHEMA.md`), so that `mitcad-import` replays it against the
//! ASM history of the B-rep record as it replays an `.f3d` design:
//!
//! - the parameters of the part's parameter table, with their expressions
//!   in Mitcad's expression language (`params`);
//! - the timeline: the browser's top-level features in order, each with
//!   the ASM history state its operation made (the history state table),
//!   and the sketches they use before them; work features and the end of
//!   the part are noted.
//!
//! Features whose definitions are not decoded come with their type and
//! state only: the import replaces them by the bodies of that state.

use std::collections::{BTreeMap, HashMap, HashSet};

use mitcad_f3d::design::ir::Dump;
use serde_json::{Value, json};

use crate::dc::{self, Definitions, Label, type_id};
use crate::params::{self, DisplayUnit, Kind, Parameters, Quantity};
use crate::{IptError, IptFile};

/// The record of a feature (extrusion, hole, fillet, ...).
pub const FEATURE: [u8; 16] = type_id("914d8790d011f8d10008cabc0663dc09");
/// A planar sketch.
pub const SKETCH: [u8; 16] = type_id("114d8790d011f8d10008cabc0663dc09");
/// A work plane.
pub const WORK_PLANE: [u8; 16] = type_id("42df52ced011d0d20008ccbc0663dc09");
/// The end of the part: features after it are rolled back.
pub const END_OF_PART: [u8; 16] = type_id("24fd418fd211ac6e00082aab32a3dc09");
/// The table of history states (see [`history_states`]).
pub const STATE_TABLE: [u8; 16] = type_id("b6212145d511d614100061a6bba647b5");

/// Feature kinds by their labels' class ids, as the dump's object types.
const KINDS: [(&str, &str); 17] = [
    ("3111a90cd0118b83000819b00524dc09", "ExtrudeFeature"),
    ("5f015400d211251c600067b79b49ebb0", "RevolveFeature"),
    ("1a7d751fd2119c54a00020803603c8c9", "HoleFeature"),
    ("dc15f7f1d1114205000830b00524dc09", "FilletFeature"),
    ("3f7100f9d2118b6f6000f0a89dccefb0", "ChamferFeature"),
    (
        "dac27a76d2110d2e60002aab01f31bb0",
        "RectangularPatternFeature",
    ),
    ("9426c2b6d211263660002cab01f31bb0", "CircularPatternFeature"),
    ("a326c2b6d211263660002cab01f31bb0", "MirrorFeature"),
    ("12963cb8d1118a2860008bb801f31bb0", "ShellFeature"),
    ("0009faf4d1110b2e60008eb801f31bb0", "DraftFeature"),
    ("e0ef542fd111340900084eba32a3dc09", "LoftFeature"),
    ("e0d168d9d411e2350000418c74736564", "RibFeature"),
    ("baa87183d311347bc000e39545df724f", "SplitFeature"),
    ("8fbf948a9a407bc5f53dceaeadac7b53", "BoundaryPatchFeature"),
    (
        "8598cc98d311eba86000f4b21d6eefb0",
        "SheetMetalCornerChamferFeature",
    ),
    (
        "b54dd6f6d311c4966000f2b21d6eefb0",
        "SheetMetalCornerRoundFeature",
    ),
    ("2d11a90cd0118b83000819b00524dc09", "BaseFeature"),
];

/// Top-level records that are features by their record type (their
/// labels' class ids are zero).
const RECORD_KINDS: [(&str, &str); 3] = [
    ("904bc1c4d311ff2860004da99dccefb0", "SheetMetalFaceFeature"),
    (
        "08dcddc3d311d0976000a7a99dccefb0",
        "SheetMetalFlangeFeature",
    ),
    ("35c6c6fbba491aa41baaaca1a66851b6", "BaseFeature"),
];

/// The object type of a top-level record, when it is a feature.
fn feature_kind(dc: &Definitions, record: usize, label: Option<&Label>) -> Option<String> {
    let class = label.map(|l| dc::type_text(&l.class));
    if let Some(class) = &class
        && let Some((_, kind)) = KINDS.iter().find(|(c, _)| c == class)
    {
        return Some((*kind).to_owned());
    }
    let t = dc::type_text(dc.type_of(record)?);
    if let Some((_, kind)) = RECORD_KINDS.iter().find(|(c, _)| *c == t) {
        return Some((*kind).to_owned());
    }
    // Another feature record: named by its label ("Emboss2" → "Emboss").
    (dc.is(record, &FEATURE) || class.is_some_and(|c| c != "0".repeat(32))).then(|| {
        let name = label.map_or("Unknown", |l| {
            l.name.trim_end_matches(|c: char| c.is_ascii_digit())
        });
        let words: String = name.split_whitespace().collect();
        format!("{words}Feature")
    })
}

/// The ASM history states each node's operation started and ended at.
///
/// The state table record (`b6212145…`) lists, per feature, its node
/// number (the u32 at the end of its header, with bit 31 set), then
/// `02 00 00 30 02 00 00 00 02 00 00 00 08 01 00 00 02 00 00 00 01` and the
/// i32 id of an ASM history state: first the state before each feature,
/// then the state after it, in the same order *(verified)*.
pub fn history_states(dc: &Definitions) -> HashMap<u32, Vec<i64>> {
    const MARK: [u8; 21] = [
        0x02, 0x00, 0x00, 0x30, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x08, 0x01, 0x00,
        0x00, 0x02, 0x00, 0x00, 0x00, 0x01,
    ];
    let mut out: HashMap<u32, Vec<i64>> = HashMap::new();
    for record in dc.of_type(&STATE_TABLE) {
        let bytes = dc.bytes(record);
        let mut at = 4;
        while at + MARK.len() + 4 <= bytes.len() {
            if bytes[at..at + MARK.len()] == MARK {
                let node = u32::from_le_bytes([
                    bytes[at - 4],
                    bytes[at - 3],
                    bytes[at - 2],
                    bytes[at - 1],
                ]);
                let s = at + MARK.len();
                let state =
                    i32::from_le_bytes([bytes[s], bytes[s + 1], bytes[s + 2], bytes[s + 3]]);
                out.entry(node & 0x7FFF_FFFF)
                    .or_default()
                    .push(i64::from(state));
                at = s + 4;
            } else {
                at += 1;
            }
        }
    }
    out
}

/// What the import report says about the parameters.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ParameterSummary {
    /// Parameter records in the file.
    pub records: usize,
    /// In the part's parameter table: model (`d<n>`) and user parameters.
    pub model: usize,
    pub user: usize,
    /// Parameter records outside the part's table (annotations, reference
    /// dimensions, features' internal tables): not imported.
    pub outside: usize,
    /// The table's internal variables (`RDxVar<n>`): not imported.
    pub internal: usize,
    /// Records that could not be read, with why.
    pub unread: Vec<String>,
}

/// What the import report says about the expressions of the part's
/// parameters: decoded, and evaluated against the stored values.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ExpressionSummary {
    /// Expressions decoded and translated into Mitcad's language.
    pub translated: usize,
    /// Of those, the ones whose value (evaluated here) is the stored one.
    pub agree: usize,
    /// Translated, but evaluating to another value: `name: value, stored`.
    pub differ: Vec<String>,
    /// Not translated (the stored value is used): `name: why`.
    pub not_translated: Vec<String>,
}

/// A timeline item of the design, for the report.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ItemInfo {
    pub index: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub object_type: String,
    /// The record in the definitions segment.
    pub record: usize,
    /// The ASM history state its operation made, when the file names it.
    pub state: Option<i64>,
}

/// The part's design.
#[derive(Clone, Debug)]
pub struct Design {
    pub dump: Dump,
    /// Timeline index → the id of the ASM history state the item's
    /// operation made.
    pub results: BTreeMap<i64, i64>,
    pub items: Vec<ItemInfo>,
    pub parameters: ParameterSummary,
    pub expressions: ExpressionSummary,
    /// Top-level records not imported (work features, after the end of the
    /// part, ...): `name: why`.
    pub left_out: Vec<String>,
    /// What the decoder left open or could not read of a feature: `name:
    /// what`.
    pub notes: Vec<String>,
}

impl Design {
    /// The number of features (items with a history state).
    pub fn features(&self) -> usize {
        self.results.len()
    }
}

/// The unit string of a parameter in the dump.
fn unit_text(q: Quantity, length: DisplayUnit) -> &'static str {
    match q {
        Quantity::Length => length.symbol(),
        Quantity::Angle => "deg",
        Quantity::Unitless => "",
    }
}

/// The parameters as the dump's user and model parameters, and their
/// summaries; `owners` gives the item that made a model parameter (its
/// `createdBy`).
fn parameters(
    params: &Parameters,
    length: DisplayUnit,
    owners: &HashMap<usize, Value>,
) -> (Value, ParameterSummary, ExpressionSummary) {
    let mut summary = ParameterSummary {
        records: params.list.len() + params.unread.len(),
        unread: params
            .unread
            .iter()
            .map(|(r, e)| format!("record {r}: {e}"))
            .collect(),
        ..ParameterSummary::default()
    };
    let mut expressions = ExpressionSummary::default();
    let mut user = Vec::new();
    let mut model = Vec::new();
    for p in &params.list {
        if !p.in_table {
            if p.name.starts_with("RDxVar") {
                summary.internal += 1;
            } else {
                summary.outside += 1;
            }
            continue;
        }
        let quantity = match &p.quantity {
            Ok(q) => *q,
            Err(e) => {
                summary.unread.push(format!("{}: {e}", p.name));
                continue;
            }
        };
        let stored = params::number(
            p.value,
            match quantity {
                Quantity::Length => length,
                Quantity::Angle => DisplayUnit::Deg,
                Quantity::Unitless => DisplayUnit::Unitless,
            },
        );
        let expression = match params.text(p, length) {
            Ok(text) => {
                expressions.translated += 1;
                match params.evaluate(p) {
                    Ok(v) if (v - p.value).abs() <= 1e-9 * p.value.abs().max(1e-9) => {
                        expressions.agree += 1;
                        text
                    }
                    Ok(v) => {
                        expressions
                            .differ
                            .push(format!("{}: {v}, stored {}", p.name, p.value));
                        stored
                    }
                    Err(e) => {
                        expressions.differ.push(format!("{}: {e}", p.name));
                        stored
                    }
                }
            }
            Err(e) => {
                expressions.not_translated.push(format!("{}: {e}", p.name));
                stored
            }
        };
        let mut entry = json!({
            "name": p.name,
            "expression": expression,
            "value": p.value,
            "unit": unit_text(quantity, length),
        });
        if let Some(owner) = owners.get(&p.record) {
            entry["createdBy"] = owner.clone();
        }
        match p.kind {
            Kind::Model => {
                summary.model += 1;
                model.push(entry);
            }
            Kind::User => {
                summary.user += 1;
                user.push(entry);
            }
        }
    }
    (json!({"user": user, "model": model}), summary, expressions)
}

/// A sketch's detail and the parameters of its dimensions (`{}` and a
/// note when it cannot be read).
fn sketch_detail(
    dc: &Definitions,
    record: usize,
    name: &str,
    params: &Parameters,
    left_out: &mut Vec<String>,
    ids: &mut HashMap<usize, HashMap<usize, String>>,
) -> (Value, Vec<(usize, String)>) {
    match crate::sketch::sketch(dc, record, name, params) {
        Ok(s) => {
            ids.insert(record, s.ids.clone());
            let made = s.detail["dimensions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["isDriving"] == true)
                .filter_map(|d| {
                    let name = d["parameter"]["name"].as_str()?;
                    let p = params.list.iter().find(|p| p.in_table && p.name == name)?;
                    Some((p.record, d["id"].as_str()?.to_owned()))
                })
                .collect();
            (s.detail, made)
        }
        Err(e) => {
            left_out.push(format!("{name}: {e}"));
            (json!({}), Vec::new())
        }
    }
}

/// Sketches not on an origin plane name the work plane they lie on, when
/// one comes before them on the timeline (coplanar, the same way up).
fn planes_of_sketches(items: &mut [Value]) {
    let vec = |v: &Value| -> Option<[f64; 3]> { serde_json::from_value(v.clone()).ok() };
    let planes: Vec<(i64, [f64; 3], [f64; 3])> = items
        .iter()
        .filter(|i| i["objectType"] == "ConstructionPlane")
        .filter_map(|i| {
            let g = &i["detail"]["geometry"];
            Some((i["index"].as_i64()?, vec(&g["origin"])?, vec(&g["normal"])?))
        })
        .collect();
    for item in items.iter_mut().filter(|i| i["objectType"] == "Sketch") {
        let d = &item["detail"];
        if d.get("referencePlane").is_some() {
            continue;
        }
        let (Some(index), Some(origin), Some(normal)) = (
            item["index"].as_i64(),
            vec(&d["model_frame"]["origin"]),
            vec(&d["model_frame"]["z_axis"]),
        ) else {
            continue;
        };
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let on = planes.iter().find(|(i, o, n)| {
            *i < index
                && (dot(*n, normal) - 1.0).abs() < 1e-9
                && dot([origin[0] - o[0], origin[1] - o[1], origin[2] - o[2]], *n).abs() < 1e-7
        });
        if let Some((i, _, _)) = on {
            item["detail"]["referencePlane"] =
                json!({"kind": "construction_plane", "origin": null, "timeline_index": i});
        }
    }
}

/// Records the sketch at `index` as the maker of its dimensions'
/// parameters.
fn own(owners: &mut HashMap<usize, Value>, made: &[(usize, String)], index: i64) {
    for (record, id) in made {
        owners.entry(*record).or_insert_with(|| {
            json!({"kind": "sketch_dimension", "objectType": "SketchDimension",
                   "sketch_timeline_index": index, "id": id})
        });
    }
}

/// The top-level records of the browser: the children of the document's
/// label.
fn top_level(dc: &Definitions, labels: &HashMap<usize, Label>) -> Vec<usize> {
    let Some(document) = dc.of_type(&dc::DOCUMENT).next() else {
        return Vec::new();
    };
    labels
        .get(&document)
        .map(|l| l.children.clone())
        .unwrap_or_default()
}

/// Reads the part's design; None when the file has no definitions segment.
/// `label` names the file in the report.
pub fn read(file: &IptFile, label: &str) -> Result<Option<Design>, IptError> {
    let Some(dc) = file.definitions()? else {
        return Ok(None);
    };
    let info = file.document();
    let length = info
        .length_unit
        .and_then(DisplayUnit::length)
        .unwrap_or(DisplayUnit::Mm);
    Ok(Some(design(
        &dc,
        length,
        label,
        info.part_number.as_deref(),
    )))
}

/// The design of a definitions segment.
pub fn design(dc: &Definitions, length: DisplayUnit, label: &str, part: Option<&str>) -> Design {
    let params = Parameters::read(dc);
    let mut owners: HashMap<usize, Value> = HashMap::new();
    let labels = dc.labels();
    let states = history_states(dc);
    let mut items: Vec<Value> = Vec::new();
    let mut infos = Vec::new();
    let mut results = BTreeMap::new();
    let mut left_out = Vec::new();
    let mut emitted: HashSet<usize> = HashSet::new();
    let mut sketch_items: HashMap<usize, (i64, String)> = HashMap::new();
    let mut notes: Vec<String> = Vec::new();
    // Timeline indices of the features and work planes by record.
    let mut item_of: HashMap<usize, i64> = HashMap::new();
    let profiles = std::cell::RefCell::new(crate::profile::Profiles::default());
    let mut sketch_ids: HashMap<usize, HashMap<usize, String>> = HashMap::new();
    let mut ended = false;
    let name_of = |r: usize| labels.get(&r).map(|l| l.name.clone());
    let push = |items: &mut Vec<Value>,
                infos: &mut Vec<ItemInfo>,
                record: usize,
                object_type: &str,
                name: Option<String>,
                detail: Value,
                state: Option<i64>| {
        let index = items.len() as i64;
        let mut item = json!({
            "index": index,
            "name": name,
            "objectType": object_type,
            "detail": detail,
            "_ipt": {"record": record},
        });
        if let Some(s) = state {
            item["_ipt"]["state"] = json!(s);
        }
        items.push(item);
        infos.push(ItemInfo {
            index,
            name: name.unwrap_or_default(),
            object_type: object_type.to_owned(),
            record,
            state,
        });
        index
    };
    for record in top_level(dc, &labels) {
        let label = labels.get(&record);
        let name = name_of(record);
        let shown = name.clone().unwrap_or_else(|| format!("record {record}"));
        if dc.is(record, &END_OF_PART) {
            ended = true;
            continue;
        }
        if dc.is(record, &SKETCH) {
            if ended {
                left_out.push(format!("{shown}: after the end of the part"));
            } else if emitted.insert(record) {
                let (detail, made) =
                    sketch_detail(dc, record, &shown, &params, &mut left_out, &mut sketch_ids);
                let index = push(&mut items, &mut infos, record, "Sketch", name, detail, None);
                own(&mut owners, &made, index);
                sketch_items.insert(record, (index, shown.clone()));
            }
            continue;
        }
        let Some(kind) = feature_kind(dc, record, label) else {
            if dc.is(record, &WORK_PLANE) && !ended {
                // The planes it is measured from first (shown under it).
                let mut planes = vec![record];
                let mut k = 0;
                while k < planes.len() && planes.len() < 32 {
                    for &c in labels
                        .get(&planes[k])
                        .map_or(&[][..], |l| l.children.as_slice())
                    {
                        if dc.is(c, &WORK_PLANE) && !planes.contains(&c) {
                            planes.push(c);
                        }
                    }
                    k += 1;
                }
                for &plane in planes.iter().rev() {
                    let cx = crate::features::Context {
                        dc,
                        params: &params,
                        sketches: &sketch_items,
                        profiles: &profiles,
                        sketch_ids: &sketch_ids,
                    };
                    // Origin planes are the document's own.
                    let origin = crate::features::plane_reference(&cx, plane, &item_of)
                        .is_some_and(|r| !r["origin"].is_null());
                    if item_of.contains_key(&plane) || origin {
                        continue;
                    }
                    let plane_name = name_of(plane);
                    match crate::features::work_plane(&cx, plane, &item_of) {
                        Ok(t) => {
                            let index = push(
                                &mut items,
                                &mut infos,
                                plane,
                                "ConstructionPlane",
                                plane_name,
                                t.detail,
                                None,
                            );
                            item_of.insert(plane, index);
                            for r in t.parameters {
                                owners.entry(r).or_insert_with(|| {
                                    json!({"kind": "feature", "objectType": "ConstructionPlane",
                                           "timeline_index": index})
                                });
                            }
                        }
                        Err(e) => left_out.push(format!(
                            "{}: {e}",
                            name_of(plane).unwrap_or_else(|| format!("record {plane}"))
                        )),
                    }
                }
            }
            continue;
        };
        if ended {
            left_out.push(format!("{shown}: after the end of the part"));
            continue;
        }
        // The sketches it uses come first.
        let mut sketch = None;
        let sketch_record =
            label.and_then(|l| l.children.iter().copied().find(|&c| dc.is(c, &SKETCH)));
        for &child in label.map_or(&[][..], |l| l.children.as_slice()) {
            if dc.is(child, &SKETCH) && emitted.insert(child) {
                let child_name = name_of(child);
                let shown = child_name
                    .clone()
                    .unwrap_or_else(|| format!("record {child}"));
                let (detail, made) =
                    sketch_detail(dc, child, &shown, &params, &mut left_out, &mut sketch_ids);
                let index = push(
                    &mut items, &mut infos, child, "Sketch", child_name, detail, None,
                );
                own(&mut owners, &made, index);
                sketch_items.insert(child, (index, shown));
            }
            if sketch.is_none() && dc.is(child, &SKETCH) {
                sketch = sketch_items.get(&child).cloned();
            }
        }
        let state = dc
            .header(record)
            .and_then(|h| states.get(&h.node))
            .and_then(|s| s.get(1))
            .copied();
        let cx = crate::features::Context {
            dc,
            params: &params,
            sketches: &sketch_items,
            profiles: &profiles,
            sketch_ids: &sketch_ids,
        };
        let translated = match kind.as_str() {
            "ExtrudeFeature" => Some(crate::features::extrude(&cx, record, sketch, sketch_record)),
            "HoleFeature" => Some(crate::features::hole(&cx, record, sketch_record)),
            "RevolveFeature" => Some(crate::features::revolve(&cx, record, sketch, sketch_record)),
            "FilletFeature" => Some(crate::features::fillet(&cx, record)),
            "ChamferFeature" => Some(crate::features::chamfer(&cx, record)),
            "RectangularPatternFeature" => {
                Some(crate::features::rectangular_pattern(&cx, record, &item_of))
            }
            "CircularPatternFeature" => {
                Some(crate::features::circular_pattern(&cx, record, &item_of))
            }
            "MirrorFeature" => Some(crate::features::mirror(&cx, record, &item_of)),
            _ => None,
        };
        let (detail, raw, made) = match translated {
            Some(Ok(t)) => {
                if !t.notes.is_empty() {
                    notes.push(format!("{shown}: {}", t.notes.join("; ")));
                }
                (t.detail, t.raw, t.parameters)
            }
            Some(Err(e)) => {
                notes.push(format!("{shown}: {e}"));
                (json!({}), None, Vec::new())
            }
            None => (json!({}), None, Vec::new()),
        };
        let index = push(&mut items, &mut infos, record, &kind, name, detail, state);
        item_of.insert(record, index);
        if let Some(raw) = raw {
            items[index as usize]["_f3d"] = raw;
        }
        for r in made {
            owners.entry(r).or_insert_with(
                || json!({"kind": "feature", "objectType": kind, "timeline_index": index}),
            );
        }
        if let Some(s) = state {
            results.insert(index, s);
        }
    }
    planes_of_sketches(&mut items);
    let (parameters_json, parameters, expressions) = parameters(&params, length, &owners);
    let count = items.len();
    let dump = json!({
        "schema": mitcad_f3d::design::ir::SCHEMA_NAME,
        "schema_version": mitcad_f3d::design::ir::SCHEMA_VERSION,
        "generator": {"decoder": "mitcad-ipt"},
        "units": {"length": "cm", "angle": "rad"},
        "source": {"mode": "ipt", "file": label},
        "document": {
            "name": part,
            "design_type": "ParametricDesignType",
            "default_length_units": length.symbol(),
        },
        "parameters": parameters_json,
        "timeline": {"available": true, "count": count, "items": items},
    });
    let dump: Dump = serde_json::from_value(dump).expect("the dump is well formed");
    Design {
        dump,
        results,
        items: infos,
        parameters,
        expressions,
        left_out,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdesign::cube_design;

    fn cube(major: u8) -> Design {
        let dc = Definitions::from_records(major, &cube_design(major));
        design(&dc, DisplayUnit::Mm, "cube.ipt", Some("CUBE"))
    }

    #[test]
    fn reads_the_parameters_sketch_and_extrusion() {
        for major in [25, 28] {
            let d = cube(major);
            let items: Vec<(&str, &str, Option<i64>)> = d
                .items
                .iter()
                .map(|i| (i.name.as_str(), i.object_type.as_str(), i.state))
                .collect();
            assert_eq!(
                items,
                [
                    ("Sketch1", "Sketch", None),
                    ("Extrusion1", "ExtrudeFeature", Some(1))
                ],
                "major {major}"
            );
            assert_eq!(d.results, BTreeMap::from([(1, 1)]));
            assert_eq!((d.parameters.model, d.parameters.user), (4, 1));
            assert_eq!(d.expressions.translated, 5);
            assert_eq!(d.expressions.agree, 5);
            let json = serde_json::to_value(&d.dump).unwrap();
            let user = &json["parameters"]["user"][0];
            assert_eq!(user["name"], "Side");
            assert_eq!(user["expression"], "10 mm");
            assert_eq!(user["unit"], "mm");
            let d0 = &json["parameters"]["model"][0];
            assert_eq!(
                (d0["name"].as_str(), d0["expression"].as_str()),
                (Some("d0"), Some("Side"))
            );
            // The sketch dimension's parameter belongs to the sketch, the
            // distance to the extrusion.
            assert_eq!(d0["createdBy"]["kind"], "sketch_dimension");
            assert_eq!(
                json["parameters"]["model"][2]["createdBy"]["timeline_index"],
                1
            );
            let sketch = &json["timeline"]["items"][0]["detail"];
            assert_eq!(sketch["counts"]["points"], 4);
            assert_eq!(sketch["counts"]["curves"], 4);
            assert_eq!(sketch["counts"]["constraints"], 4);
            assert_eq!(sketch["counts"]["dimensions"], 2);
            assert_eq!(sketch["referencePlane"]["origin"], "XY");
            assert_eq!(sketch["curves"][1]["startSketchPoint"], "p1");
            assert_eq!(sketch["dimensions"][1]["parameter"]["name"], "d1");
            let extrude = &json["timeline"]["items"][1];
            assert_eq!(extrude["detail"]["operation"], "NewBodyFeatureOperation");
            assert_eq!(extrude["detail"]["extentOne"]["distance"]["name"], "d2");
            assert_eq!(extrude["detail"]["profile"][0]["sketch_timeline_index"], 0);
            assert_eq!(
                extrude["_f3d"]["extrude"]["direction_vector"],
                json!([0.0, 0.0, 1.0])
            );
        }
    }

    #[test]
    fn reads_the_test_part_with_its_design() {
        let f = IptFile::parse(crate::testdata::test_part_with_design()).unwrap();
        let d = read(&f, "part.ipt").unwrap().unwrap();
        assert_eq!(d.items.len(), 2);
        assert_eq!(d.features(), 1);
        // Without definitions, no design.
        let f = IptFile::parse(crate::testdata::test_part()).unwrap();
        let d = read(&f, "part.ipt").unwrap().unwrap();
        assert!(d.items.is_empty());
        assert_eq!(d.parameters.records, 0);
    }
}
