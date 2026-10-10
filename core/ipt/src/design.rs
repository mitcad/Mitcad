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
const KINDS: [(&str, &str); 19] = [
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
    ("336723c07e40eacb2eff669e9add87f8", "CombineFeature"),
    ("05daba90d111386100080bbd0663dc09", "SweepFeature"),
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
/// `02 00 00 30 02 00 00 00 02 00 00 00`, a u16 and a byte each followed by
/// zeros (`08 01 00 00 02 00 00 00` in most parts; `07 01 00 00 01 00 00
/// 00` and `09 01 …` in others, the same in every entry of a record), `01`
/// and the i32 id of an ASM history state: first the state before each
/// feature, then the state after it, in the same order *(verified)*.
pub fn history_states(dc: &Definitions) -> HashMap<u32, Vec<i64>> {
    const MARK: [u8; 12] = [
        0x02, 0x00, 0x00, 0x30, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
    ];
    // The mark, the two words and the `01` before the state.
    const LEN: usize = MARK.len() + 9;
    let marked = |b: &[u8]| {
        b[..MARK.len()] == MARK && b[14..16] == [0, 0] && b[17..20] == [0, 0, 0] && b[20] == 1
    };
    let mut out: HashMap<u32, Vec<i64>> = HashMap::new();
    for record in dc.of_type(&STATE_TABLE) {
        let bytes = dc.bytes(record);
        let mut at = 4;
        while at + LEN + 4 <= bytes.len() {
            if marked(&bytes[at..at + LEN]) {
                let node = u32::from_le_bytes([
                    bytes[at - 4],
                    bytes[at - 3],
                    bytes[at - 2],
                    bytes[at - 1],
                ]);
                let s = at + LEN;
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
    /// Parameters the model computes whose expressions give another value
    /// (the stored value is used): `name: value, stored`.
    pub computed: Vec<String>,
    /// Parameters nothing imported uses (no sketch dimension, feature or
    /// work plane that comes in with them, nor another parameter's
    /// expression) whose expressions give another value: kept as their
    /// stored values, `name: value, stored` (a library feature's outputs,
    /// the parameters of annotations not decoded *(seen)*).
    pub unused: Vec<String>,
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
    // The parameters something imported uses: the items' own and those
    // the table's expressions name.
    let mut used: HashSet<usize> = owners.keys().copied().collect();
    for p in params.table() {
        if let Ok(e) = &p.expression {
            let mut named = Vec::new();
            Parameters::named(e, &mut named);
            used.extend(named);
        }
    }
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
        let evaluated = params.evaluate(p);
        let expression = match params.text(p, length) {
            // Its expression does not give the value the model computed.
            Ok(_)
                if p.is_computed()
                    && evaluated
                        .as_ref()
                        .is_ok_and(|v| (v - p.value).abs() > 1e-9 * p.value.abs().max(1e-9)) =>
            {
                expressions.computed.push(format!(
                    "{}: {}, stored {}",
                    p.name,
                    evaluated.as_ref().copied().unwrap_or_default(),
                    p.value
                ));
                stored
            }
            Ok(text) => {
                expressions.translated += 1;
                match evaluated {
                    Ok(v) if (v - p.value).abs() <= 1e-9 * p.value.abs().max(1e-9) => {
                        expressions.agree += 1;
                        text
                    }
                    // Nothing imported uses it: its value as stored.
                    Ok(v) if !used.contains(&p.record) => {
                        expressions.translated -= 1;
                        expressions
                            .unused
                            .push(format!("{}: {v}, stored {}", p.name, p.value));
                        stored
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
    flagged: &mut HashMap<usize, Vec<String>>,
) -> (Value, Vec<(usize, String)>) {
    match crate::sketch::sketch(dc, record, name, params) {
        Ok(s) => {
            ids.insert(record, s.ids.clone());
            if !s.flagged.is_empty() {
                flagged.insert(record, s.flagged.clone());
            }
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

/// The flag 0x40 marks construction geometry in most sketches, but lines
/// with it also bound profiles that features selected (in parts saved by
/// older releases *(seen)*): a flagged curve that lies on a loop of a
/// profile measured for its sketch is a profile's edge, not construction
/// geometry.
fn bounding_curves(
    items: &mut [Value],
    sketches: &HashMap<usize, (i64, String)>,
    flagged: &HashMap<usize, Vec<String>>,
    outlines: &[(usize, usize, Vec<[f64; 2]>)],
) {
    // A point on a loop: within the sampling's chord error of a segment.
    let on_loop = |p: [f64; 2], tolerance: f64, o: &[[f64; 2]]| {
        (0..o.len()).any(|i| {
            let (a, b) = (o[i], o[(i + 1) % o.len()]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = dx * dx + dy * dy;
            let t = if l2 > 0.0 {
                (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (a[0] + t * dx - p[0]).hypot(a[1] + t * dy - p[1]) < tolerance
        })
    };
    let on = |p: [f64; 2], tolerance: f64, loops: &[(usize, &[[f64; 2]])]| {
        loops.iter().any(|(_, o)| on_loop(p, tolerance, o))
    };
    // Samples along a curve.
    const SAMPLES: u32 = 16;
    // Within a loop.
    let within = |p: [f64; 2], poly: &[[f64; 2]]| {
        let mut odd = false;
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if (a[1] > p[1]) != (b[1] > p[1])
                && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
            {
                odd = !odd;
            }
        }
        odd
    };
    // Inside the profiles: within an odd number of their loops.
    let inside = |p: [f64; 2], loops: &[(usize, &[[f64; 2]])]| {
        loops.iter().filter(|(_, poly)| within(p, poly)).count() % 2 == 1
    };
    for (sketch, ids) in flagged {
        let Some((index, _)) = sketches.get(sketch) else {
            continue;
        };
        // Each loop with its selection.
        let mine: Vec<(usize, &[[f64; 2]])> = outlines
            .iter()
            .filter(|(s, _, _)| s == sketch)
            .map(|(_, g, o)| (*g, &o[..]))
            .collect();
        if mine.is_empty() {
            continue;
        }
        let Some(curves) = items
            .get_mut(*index as usize)
            .and_then(|i| i["detail"]["curves"].as_array_mut())
        else {
            continue;
        };
        for k in 0..curves.len() {
            let c = &curves[k];
            if !c["id"]
                .as_str()
                .is_some_and(|id| ids.iter().any(|f| f == id))
            {
                continue;
            }
            let g = &c["geometry"];
            let xy = |v: &Value| -> Option<[f64; 2]> { Some([v[0].as_f64()?, v[1].as_f64()?]) };
            // Points along it, and how far a sampled arc may lie off it.
            let samples: Option<(Vec<[f64; 2]>, f64)> = match g["type"].as_str() {
                Some("Line3D") => {
                    let (a, b) = (xy(&g["startPoint"]), xy(&g["endPoint"]));
                    a.zip(b).map(|(a, b)| {
                        let points = (0..=SAMPLES)
                            .map(|k| {
                                let t = f64::from(k) / f64::from(SAMPLES);
                                [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
                            })
                            .collect();
                        (points, 1e-6)
                    })
                }
                Some(t @ ("Arc3D" | "Circle3D")) => (|| {
                    let center = xy(&g["center"])?;
                    let r = g["radius"].as_f64()?;
                    let (a0, a1) = if t == "Arc3D" {
                        (g["startAngle"].as_f64()?, g["endAngle"].as_f64()?)
                    } else {
                        (0.0, std::f64::consts::TAU)
                    };
                    let points = (0..=SAMPLES)
                        .map(|k| {
                            let a = a0 + (a1 - a0) * f64::from(k) / f64::from(SAMPLES);
                            [center[0] + r * a.cos(), center[1] + r * a.sin()]
                        })
                        .collect();
                    // The loops sample a circle with 720 chords.
                    Some((
                        points,
                        r * (1.0 - (std::f64::consts::PI / 720.0).cos()) + 1e-6,
                    ))
                })(),
                _ => None,
            };
            let Some((points, tolerance)) = samples else {
                continue;
            };
            // A point on the curve itself.
            let on_curve = |p: [f64; 2]| -> bool {
                match g["type"].as_str() {
                    Some("Line3D") => {
                        let (Some(a), Some(b)) = (xy(&g["startPoint"]), xy(&g["endPoint"])) else {
                            return false;
                        };
                        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                        let l2 = dx * dx + dy * dy;
                        if l2 <= 0.0 {
                            return false;
                        }
                        let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2;
                        (-1e-9..=1.0 + 1e-9).contains(&t)
                            && (a[0] + t * dx - p[0]).hypot(a[1] + t * dy - p[1]) < tolerance
                    }
                    Some(t @ ("Arc3D" | "Circle3D")) => {
                        let (Some(center), Some(r)) = (xy(&g["center"]), g["radius"].as_f64())
                        else {
                            return false;
                        };
                        if ((p[0] - center[0]).hypot(p[1] - center[1]) - r).abs() >= tolerance {
                            return false;
                        }
                        if t == "Circle3D" {
                            return true;
                        }
                        let (Some(a0), Some(a1)) =
                            (g["startAngle"].as_f64(), g["endAngle"].as_f64())
                        else {
                            return false;
                        };
                        let tau = std::f64::consts::TAU;
                        let a = (p[1] - center[1]).atan2(p[0] - center[0]);
                        let slack = tolerance / r.max(1e-12);
                        (a - a0 + slack).rem_euclid(tau) <= a1 - a0 + 2.0 * slack
                    }
                    _ => false,
                }
            };
            // An edge of a loop on the curve: its ends and middle on it,
            // apart (a short part of a long curve that the samples miss).
            let edge_on = mine.iter().any(|(_, o)| {
                (0..o.len()).any(|i| {
                    let (a, b) = (o[i], o[(i + 1) % o.len()]);
                    (a[0] - b[0]).hypot(a[1] - b[1]) > 1e-4
                        && on_curve(a)
                        && on_curve(b)
                        && on_curve([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
                })
            });
            // On a loop all along, or along a part of it (two samples in a
            // row, or a loop's edge: more than a crossing) with the rest
            // outside the profiles (a pattern's original whose copy's edge
            // trims it, a long line that a profile's side ends on).
            let along: Vec<bool> = points.iter().map(|&p| on(p, tolerance, &mine)).collect();
            let part = (along.windows(2).any(|w| w[0] && w[1]) || edge_on)
                && points
                    .iter()
                    .zip(&along)
                    .all(|(&p, &on)| on || !inside(p, &mine));
            // Once: a copy of a curve that bounds the profile stays
            // construction geometry.
            let copy = curves.iter().enumerate().any(|(j, other)| {
                j != k && other["isConstruction"] != true && crate::sketch::same_curve(other, c)
            });
            // Or on a loop at its ends and middle (an arc's quarters): a
            // line across profiles that all lie on its sides.
            let step = SAMPLES as usize / if g["type"] == "Line3D" { 2 } else { 4 };
            let coarse = along.iter().step_by(step).all(|&a| a);
            // A line along part of a loop where another curve of the
            // sketch already bounds it (along the same line) stays
            // construction geometry: the two would overlap.
            let overlapped = || {
                let line = |v: &Value| -> Option<([f64; 2], [f64; 2])> {
                    let g = &v["geometry"];
                    (g["type"] == "Line3D").then_some(())?;
                    Some((xy(&g["startPoint"])?, xy(&g["endPoint"])?))
                };
                let Some((a, b)) = line(c) else {
                    return false;
                };
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l = dx.hypot(dy);
                if l <= 0.0 {
                    return false;
                }
                let (ux, uy) = (dx / l, dy / l);
                let at = |p: [f64; 2]| {
                    (
                        (p[0] - a[0]) * ux + (p[1] - a[1]) * uy,
                        (p[0] - a[0]) * uy - (p[1] - a[1]) * ux,
                    )
                };
                curves.iter().enumerate().any(|(j, other)| {
                    let Some((p, q)) =
                        line(other).filter(|_| j != k && other["isConstruction"] != true)
                    else {
                        return false;
                    };
                    let ((s, h1), (t, h2)) = (at(p), at(q));
                    h1.abs() < 1e-6 && h2.abs() < 1e-6 && s.max(t).min(l) - s.min(t).max(0.0) > 1e-6
                })
            };
            // Along a part of loops only where two profiles of one
            // selection meet (a line through a circle whose two halves one
            // feature takes): it bounds nothing in what the feature makes,
            // and kept it would split the feature's faces *(seen)*: on
            // two of the selection's loops, with the selection's profiles
            // on both sides (not a loop the selection has twice, along the
            // outside of its profiles *(seen)*), and on no loop of another
            // feature's selection (whose profile it bounds: kept, the first
            // feature's two profiles are a union of the sketch's regions).
            let between = || {
                // The samples on loops, with the normal there.
                let on_loops: Vec<([f64; 2], [f64; 2])> = (0..points.len())
                    .filter(|&i| along[i])
                    .filter_map(|i| {
                        let (a, b) = (
                            points[i.saturating_sub(1)],
                            points[(i + 1).min(points.len() - 1)],
                        );
                        let (tx, ty) = (b[0] - a[0], b[1] - a[1]);
                        let l = tx.hypot(ty);
                        (l > 0.0).then_some((points[i], [-ty / l, tx / l]))
                    })
                    .collect();
                // Off the curve by more than the loops' chord error.
                let off = 4.0 * tolerance + 1e-5;
                let mut groups: Vec<usize> = mine.iter().map(|(g, _)| *g).collect();
                groups.sort_unstable();
                groups.dedup();
                !on_loops.is_empty()
                    && groups.iter().any(|&g| {
                        let in_group =
                            |p: [f64; 2]| mine.iter().any(|(h, o)| *h == g && within(p, o));
                        // (Its ends may lie on another selection's loop
                        // that runs across it.)
                        let others = (1..points.len().saturating_sub(1))
                            .filter(|&i| {
                                mine.iter()
                                    .any(|(h, o)| *h != g && on_loop(points[i], tolerance, o))
                            })
                            .count();
                        others < 2
                            && on_loops.iter().all(|&(p, n)| {
                                mine.iter()
                                    .filter(|(h, o)| *h == g && on_loop(p, tolerance, o))
                                    .count()
                                    >= 2
                                    && in_group([p[0] + off * n[0], p[1] + off * n[1]])
                                    && in_group([p[0] - off * n[0], p[1] - off * n[1]])
                            })
                    })
            };
            if (along.iter().all(|&a| a) || coarse || (part && !overlapped() && !between()))
                && !copy
                && let Some(o) = curves[k].as_object_mut()
            {
                o.remove("isConstruction");
            }
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
    let mut flagged: HashMap<usize, Vec<String>> = HashMap::new();
    let mut producers = crate::features::Producers::default();
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
                let (detail, made) = sketch_detail(
                    dc,
                    record,
                    &shown,
                    &params,
                    &mut left_out,
                    &mut sketch_ids,
                    &mut flagged,
                );
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
                        bodies: &producers,
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
                let (detail, made) = sketch_detail(
                    dc,
                    child,
                    &shown,
                    &params,
                    &mut left_out,
                    &mut sketch_ids,
                    &mut flagged,
                );
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
        let after = |r: usize| {
            dc.header(r)
                .and_then(|h| states.get(&h.node))
                .and_then(|s| s.get(1))
                .copied()
        };
        // A sheet-metal feature (a face, a flange) is a group whose label
        // children are the features the table names (plates, bends,
        // corners): the last child's state is the group's.
        let state = after(record).or_else(|| {
            label.and_then(|l| {
                l.children
                    .iter()
                    .rev()
                    .filter(|&&c| !dc.is(c, &SKETCH))
                    .find_map(|&c| after(c))
            })
        });
        let cx = crate::features::Context {
            dc,
            params: &params,
            sketches: &sketch_items,
            profiles: &profiles,
            sketch_ids: &sketch_ids,
            bodies: &producers,
        };
        // The sketches its label shows (a sweep's profile and path).
        let child_sketches: Vec<(usize, i64, String)> = label
            .map_or(&[][..], |l| l.children.as_slice())
            .iter()
            .filter_map(|c| sketch_items.get(c).map(|(i, n)| (*c, *i, n.clone())))
            .collect();
        let mut as_kind: Option<&str> = None;
        let translated = match kind.as_str() {
            "ExtrudeFeature" => Some(crate::features::extrude(
                &cx,
                record,
                sketch,
                sketch_record,
                &item_of,
            )),
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
            "ThreadFeature" => Some(crate::features::thread(&cx, record)),
            "CoilFeature" => Some(crate::features::coil(&cx, record, sketch, sketch_record)),
            "CombineFeature" => Some(crate::features::combine(&cx, record)),
            "ShellFeature" => Some(crate::features::shell(&cx, record)),
            "SweepFeature" => Some(crate::features::sweep(&cx, record, &child_sketches)),
            // Its label shows its sections (`7ca882b3…`), not always their
            // sketches: the sketches before it, the latest first.
            "LoftFeature" => {
                let mut before: Vec<(usize, i64, String)> = child_sketches.clone();
                let mut all: Vec<(usize, i64, String)> = sketch_items
                    .iter()
                    .map(|(r, (i, n))| (*r, *i, n.clone()))
                    .collect();
                all.sort_by_key(|s| std::cmp::Reverse(s.1));
                before.extend(all);
                Some(crate::features::loft(&cx, record, &before))
            }
            // A split of faces or of bodies.
            "SplitFeature" => Some(
                crate::features::split(&cx, record, sketch, sketch_record, &item_of).map(
                    |(k, t)| {
                        as_kind = Some(k);
                        t
                    },
                ),
            ),
            _ => None,
        };
        let kind = as_kind.map_or(kind, str::to_owned);
        let referred = cx.feature_bodies(record);
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
        // The bodies it refers to first are its own (not a combine's: its
        // target and tools were made before it).
        if kind != "CombineFeature" {
            producers.record(index, &referred, name_of);
        }
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
    crate::features::path_curves(&mut items);
    bounding_curves(
        &mut items,
        &sketch_items,
        &flagged,
        &profiles.borrow().outlines,
    );
    let (parameters_json, parameters, expressions) = parameters(&params, length, &owners);
    for u in &expressions.unused {
        notes.push(format!(
            "{u}: its expression does not give its value, and nothing imported uses it: \
             kept as the value"
        ));
    }
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
        for major in [20, 22, 24, 25, 28] {
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
    fn reads_polygons_patterns_and_points_on_lines() {
        for major in [21, 24, 25, 28] {
            let dc = Definitions::from_records(major, &crate::testdesign::groups_design(major));
            let d = design(&dc, DisplayUnit::Mm, "groups.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let sketch = &json["timeline"]["items"][0]["detail"];
            let constraints = sketch["constraints"].as_array().unwrap();
            let types: Vec<&str> = constraints
                .iter()
                .map(|c| c["type"].as_str().unwrap())
                .collect();
            assert_eq!(
                types,
                [
                    "PolygonConstraint",
                    "CircularPatternConstraint",
                    "RectangularPatternConstraint",
                    "HorizontalPointsConstraint",
                    "VerticalPointsConstraint"
                ],
                "major {major}"
            );
            let id = |k: usize| sketch["points"][k]["id"].as_str().unwrap().to_owned();
            assert_eq!(sketch["points"][0]["isFixed"], true);
            // The corners in order around, then the centre.
            assert_eq!(
                constraints[0]["refs"]["entities"],
                json!([id(1), id(2), id(3), id(0)])
            );
            let circular = &constraints[1]["props"];
            assert_eq!(circular["centerPoint"], id(4));
            assert_eq!(circular["quantity"]["name"], "d0");
            assert_eq!(circular["totalAngle"]["name"], "d1");
            assert_eq!(circular["createdEntities"].as_array().unwrap().len(), 4);
            // The originals: the first circle and its centre.
            assert_eq!(
                constraints[1]["refs"]["entities"].as_array().unwrap().len(),
                2 + 4 + 1
            );
            let rectangular = &constraints[2]["props"];
            assert_eq!(rectangular["quantityOne"]["name"], "d2");
            assert_eq!(rectangular["distanceTwo"]["name"], "d5");
            assert_eq!(rectangular["directionOne"], json!([1.0, 0.0, 0.0]));
            // The copies lie against the second line.
            assert_eq!(rectangular["directionTwo"], json!([-0.0, -1.0, 0.0]));
            assert_eq!(rectangular["createdEntities"].as_array().unwrap().len(), 6);
            let arc = sketch["curves"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["type"] == "SketchArc")
                .unwrap();
            assert_eq!(arc["radius"], 1.0);
        }
    }

    #[test]
    fn flagged_lines_that_bound_a_selected_profile_are_not_construction() {
        for major in [21, 24] {
            let dc =
                Definitions::from_records(major, &crate::testdesign::flagged_cube_design(major));
            let d = design(&dc, DisplayUnit::Mm, "cube.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let curves = json["timeline"]["items"][0]["detail"]["curves"]
                .as_array()
                .unwrap()
                .clone();
            let construction: Vec<bool> =
                curves.iter().map(|c| c["isConstruction"] == true).collect();
            // The square's sides bound the selected profile; the diagonal
            // does not.
            assert_eq!(
                construction,
                [false, false, false, false, true],
                "major {major}"
            );
            let profile = &json["timeline"]["items"][1]["detail"]["profile"][0];
            assert!((profile["area"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn radial_lines_lines_of_no_length_and_patterns_on_their_originals() {
        for major in [21, 24, 25, 28] {
            let dc =
                Definitions::from_records(major, &crate::testdesign::sketch_fixes_design(major));
            let d = design(&dc, DisplayUnit::Mm, "fixes.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let sketch = &json["timeline"]["items"][0]["detail"];
            let constraints = sketch["constraints"].as_array().unwrap();
            let summary: Vec<String> = constraints
                .iter()
                .map(|c| {
                    let refs: Vec<&str> = c["refs"]
                        .as_object()
                        .into_iter()
                        .flat_map(|o| o.values())
                        .filter_map(Value::as_str)
                        .collect();
                    format!("{} {}", c["type"].as_str().unwrap(), refs.join(" "))
                })
                .collect();
            // The pattern's copies held on the original (points p5..p7,
            // circles c2..c4), the arc's centre p0 on the radial line c1;
            // the line of no length and its constraint are left out.
            assert_eq!(
                summary,
                [
                    "EqualConstraint c3 c2",
                    "CoincidentConstraint p5 p6",
                    "EqualConstraint c4 c3",
                    "CoincidentConstraint p6 p7",
                    "CoincidentConstraint c1 p0",
                ],
                "major {major}"
            );
            assert_eq!(sketch["curves"].as_array().unwrap().len(), 6);
            let diameter = &sketch["dimensions"][0];
            assert_eq!(diameter["type"], "SketchLinearDiameterDimension");
            assert_eq!(diameter["refs"], json!({"line": "c5", "entityTwo": "p10"}));
        }
    }

    #[test]
    fn reads_projected_splines() {
        use crate::testdesign::{COINCIDENT, DesignWriter};
        for major in [22, 24, 27, 28] {
            let mut w = DesignWriter::new(major);
            let sketch = w.sketch();
            let identity = [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
            let transform = w.transform(identity);
            let normal = w.direction([0.0, 0.0, 1.0]);
            let a = w.projected_point(sketch, [0.0, 0.0]);
            let b = w.projected_point(sketch, [3.0, 0.0]);
            let poles = [[0.0, 0.0], [1.0, 1.0], [2.0, 1.0], [3.0, 0.0]];
            let spline = w.sketch_spline(sketch, [a, b], 3, &poles, 0x4_0000);
            let on = w.sketch_constraint(COINCIDENT, None, &[Some(a), Some(spline)]);
            // A record of another kind of the projected geometry only.
            let handles = w.sketch_constraint(
                crate::dc::type_id("9d858c5d534666ea4bf6daa182a838f2"),
                None,
                &[Some(spline), Some(a), Some(b)],
            );
            w.finish_sketch(sketch, &[a, b, spline, on, handles], transform, normal);
            w.label(sketch, "Sketch1", &[], [0; 16]);
            w.top.push(sketch);
            let dc = Definitions::from_records(major, &w.finish());
            let d = design(&dc, DisplayUnit::Mm, "spline.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let sketch = &json["timeline"]["items"][0]["detail"];
            assert_eq!(sketch["constraints"], json!([]), "major {major}");
            let curve = &sketch["curves"][0];
            assert_eq!(curve["type"], "SketchFixedSpline");
            assert_eq!(curve["isReference"], true);
            let nurbs = &curve["geometry"]["nurbs"];
            assert_eq!(nurbs["degree"], 3);
            assert_eq!(
                nurbs["knots"],
                json!([0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0])
            );
            assert_eq!(nurbs["controlPoints"][1], json!([1.0, 1.0, 0.0]));
        }
    }

    #[test]
    fn a_drawn_spline_comes_in_with_its_control_polygon() {
        use crate::testdesign::DesignWriter;
        for major in [22, 24, 28] {
            let mut w = DesignWriter::new(major);
            let sketch = w.sketch();
            let identity = [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ];
            let transform = w.transform(identity);
            let normal = w.direction([0.0, 0.0, 1.0]);
            let poles = [[0.0, 0.0], [1.0, 1.0], [2.0, 1.0], [3.0, 0.0]];
            let points: Vec<usize> = poles.iter().map(|&p| w.sketch_point(sketch, p)).collect();
            let sides: Vec<usize> = (0..3)
                .map(|i| w.sketch_line(sketch, points[i], points[i + 1], 0x40))
                .collect();
            let spline = w.sketch_spline(sketch, [points[0], points[3]], 3, &poles, 0);
            // The polygon lists the points in another order: they are
            // matched by where they lie.
            let mut refs = vec![Some(spline), Some(points[1]), Some(points[0])];
            refs.extend([points[2], points[3]].map(Some));
            refs.extend(sides.iter().map(|&s| Some(s)));
            let polygon = w.sketch_constraint(crate::sketch::CONTROL_POLYGON, None, &refs);
            let mut entities = points.clone();
            entities.extend(&sides);
            entities.extend([spline, polygon]);
            w.finish_sketch(sketch, &entities, transform, normal);
            w.label(sketch, "Sketch1", &[], [0; 16]);
            w.top.push(sketch);
            let dc = Definitions::from_records(major, &w.finish());
            let d = design(&dc, DisplayUnit::Mm, "spline.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let sketch = &json["timeline"]["items"][0]["detail"];
            assert_eq!(sketch["constraints"], json!([]), "major {major}: {sketch}");
            let curve = &sketch["curves"][3];
            assert_eq!(curve["type"], "SketchControlPointSpline", "{sketch}");
            assert_eq!(curve["controlPoints"], json!(["p0", "p1", "p2", "p3"]));
        }
    }

    #[test]
    fn reads_planes_parallel_to_a_plane_through_a_point() {
        use crate::features::{PLANE_PARALLEL_GEOMETRY, PLANE_PARALLEL_POINT};
        use crate::testdesign::DesignWriter;
        for major in [21, 24, 28] {
            let mut w = DesignWriter::new(major);
            let (x, y, z) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
            // The origin's XY and XZ planes (the XZ plane stored with its
            // normal along −y), off the timeline.
            let xy = w.work_plane([0.0; 3], x, y);
            let xz = w.work_plane([0.0; 3], x, z);
            let up = w.work_plane([0.0, 0.0, 2.0], x, y);
            w.label(up, "Work Plane1", &[], [0; 16]);
            w.top.push(up);
            let p = w.work_point([1.0, 1.0, 2.0]);
            w.plane_definition(PLANE_PARALLEL_POINT, up, &[p, xy]);
            let back = w.work_plane([0.0, -3.0, 0.0], x, z);
            w.label(back, "Work Plane2", &[], [0; 16]);
            w.top.push(back);
            // The point of the model's geometry: any record here.
            w.plane_definition(PLANE_PARALLEL_GEOMETRY, back, &[xz, p]);
            let dc = Definitions::from_records(major, &w.finish());
            let d = design(&dc, DisplayUnit::Mm, "parallel.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let items = json["timeline"]["items"].as_array().unwrap();
            let def = |name: &str| {
                items
                    .iter()
                    .find(|i| i["name"] == name)
                    .map(|i| i["detail"]["definition"].clone())
                    .unwrap_or_default()
            };
            for (name, plane, offset) in [("Work Plane1", "XY", 2.0), ("Work Plane2", "XZ", -3.0)] {
                let d = def(name);
                assert_eq!(d["_type"], "ConstructionPlaneOffsetDefinition", "{json}");
                assert_eq!(d["planarEntity"]["origin"], plane);
                assert_eq!(d["offset"]["value"].as_f64(), Some(offset), "{name}");
            }
        }
    }

    #[test]
    fn reads_lofts() {
        for major in [21, 24, 27, 28] {
            let dc = Definitions::from_records(major, &crate::testdesign::loft_design(major));
            let d = design(&dc, DisplayUnit::Mm, "loft.ipt", None);
            assert!(d.notes.is_empty(), "{:?}", d.notes);
            let json = serde_json::to_value(&d.dump).unwrap();
            let items = json["timeline"]["items"].as_array().unwrap();
            let names: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
            assert_eq!(names, ["Sketch1", "Sketch2", "Loft1"], "major {major}");
            let loft = &items[2]["detail"];
            assert_eq!(loft["operation"], "NewBodyFeatureOperation");
            let sections = loft["loftSections"].as_array().unwrap();
            assert_eq!(sections.len(), 2);
            for (k, s) in sections.iter().enumerate() {
                assert_eq!(s["entity"]["sketch_timeline_index"], k);
                assert!((s["entity"]["area"].as_f64().unwrap() - 1.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn functions_and_parameters_nothing_uses() {
        for major in [20, 24, 25, 28] {
            let dc = Definitions::from_records(major, &crate::testdesign::function_design(major));
            let d = design(&dc, DisplayUnit::Mm, "functions.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let expression = |name: &str| {
                json["parameters"]
                    .as_object()
                    .unwrap()
                    .values()
                    .flat_map(|v| v.as_array().unwrap())
                    .find(|p| p["name"] == name)
                    .map(|p| p["expression"].as_str().unwrap().to_owned())
                    .unwrap()
            };
            assert_eq!(expression("d0"), "SW / cos(30 deg)", "major {major}");
            assert_eq!(expression("d1"), "SW / 2");
            // d2 differs, but nothing uses it: kept as its value, a note.
            assert_eq!(expression("d2"), "0.785398163397");
            assert_eq!(d.expressions.unused.len(), 1, "{:?}", d.expressions);
            assert!(d.notes.iter().any(|n| n.starts_with("d2: 0, stored")));
            // d3 differs and a sketch dimension uses it.
            assert_eq!(d.expressions.differ.len(), 1, "{:?}", d.expressions);
            assert!(d.expressions.differ[0].starts_with("d3"));
            assert_eq!((d.expressions.translated, d.expressions.agree), (4, 3));
        }
    }

    #[test]
    fn a_computed_parameter_keeps_its_value() {
        let mut records = crate::testdesign::cube_design(24);
        let dc = Definitions::from_records(24, &records);
        let d1 = Parameters::read(&dc)
            .list
            .iter()
            .find(|p| p.name == "d1")
            .unwrap()
            .record;
        // The model computes d1 (a driven dimension's, measured 20 mm);
        // its expression gives 10 mm.
        let bytes = &mut records[d1].1;
        bytes[10..14].copy_from_slice(&(0x0003_4200 | crate::params::COMPUTED).to_le_bytes());
        let at = bytes.len() - 20;
        bytes[at..at + 8].copy_from_slice(&2.0f64.to_le_bytes());
        let dc = Definitions::from_records(24, &records);
        let d = design(&dc, DisplayUnit::Mm, "cube.ipt", None);
        assert_eq!(d.expressions.computed.len(), 1, "{:?}", d.expressions);
        assert!(
            d.expressions.differ.is_empty(),
            "{:?}",
            d.expressions.differ
        );
        let json = serde_json::to_value(&d.dump).unwrap();
        let model = json["parameters"]["model"].as_array().unwrap();
        let d1 = model.iter().find(|p| p["name"] == "d1").unwrap();
        assert_eq!(d1["expression"], "20 mm");
    }

    #[test]
    fn a_region_selected_twice_is_one_profile() {
        let dc = Definitions::from_records(24, &crate::testdesign::twice_selected_cube_design(24));
        let d = design(&dc, DisplayUnit::Mm, "cube.ipt", None);
        let json = serde_json::to_value(&d.dump).unwrap();
        let profiles = json["timeline"]["items"][1]["detail"]["profile"]
            .as_array()
            .unwrap();
        assert_eq!(profiles.len(), 1, "{profiles:?}");
        assert!((profiles[0]["area"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn reads_the_extents_of_extrusions() {
        for (extent, kind) in [
            (1, "DistanceExtentDefinition"),
            (5, "ThroughAllExtentDefinition"),
            (4, "ToEntityExtentDefinition"),
            (7, "ToEntityExtentDefinition"),
        ] {
            let dc =
                Definitions::from_records(24, &crate::testdesign::cube_design_extent(24, extent));
            let d = design(&dc, DisplayUnit::Mm, "cube.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let detail = &json["timeline"]["items"][1]["detail"];
            assert_eq!(detail["extentOne"]["_type"], kind, "extent {extent}");
        }
    }

    #[test]
    fn reads_the_objects_extrusions_extend_to() {
        for major in [24, 27] {
            let dc = Definitions::from_records(major, &crate::testdesign::extents_design(major));
            let d = design(&dc, DisplayUnit::Mm, "extents.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let items = &json["timeline"]["items"];
            let one = |i: usize| &items[i]["detail"]["extentOne"];
            // Up to the next face: the first face of Solid1, made by item 1.
            assert_eq!(one(2)["_type"], "ToEntityExtentDefinition");
            assert_eq!(one(2)["entity"]["kind"], "body");
            assert_eq!(one(2)["entity"]["_f3d"]["producer"], 1);
            assert_eq!(one(2)["isMinimumSolution"], true);
            // Up to the plane x = 2 cm (its normal the cross of its axes).
            let plane = &one(3)["entity"];
            assert_eq!(plane["kind"], "face");
            assert_eq!(plane["geometry"]["type"], "Plane");
            assert_eq!(plane["geometry"]["origin"], json!([2.0, 0.0, 0.0]));
            assert_eq!(plane["geometry"]["normal"], json!([1.0, 0.0, 0.0]));
            assert!(plane.get("point_on_face").is_none());
            // Up to the cylinder: its axis and radius, no point on it.
            let cylinder = &one(4)["entity"];
            assert_eq!(cylinder["geometry"]["type"], "Cylinder");
            assert_eq!(cylinder["geometry"]["origin"], json!([1.0, 1.0, 0.0]));
            assert_eq!(cylinder["geometry"]["axis"], json!([0.0, 0.0, 1.0]));
            assert_eq!(cylinder["geometry"]["radius"], 0.3);
            assert!(cylinder.get("point_on_face").is_none());
            // Up to the work plane z = 3 cm.
            let work = &one(5)["entity"];
            assert_eq!(work["kind"], "construction_plane");
            assert_eq!(work["geometry"]["origin"], json!([0.0, 0.0, 3.0]));
            assert!(
                !d.notes.iter().any(|n| n.contains("not decoded")),
                "{:?}",
                d.notes
            );
        }
    }

    #[test]
    fn reads_the_extents_of_revolutions() {
        let quarter = std::f64::consts::FRAC_PI_2;
        let tau = std::f64::consts::TAU;
        // A full turn (3) keeps an angle it does not use.
        for (extent, angle, name, value) in [
            (1, quarter, Some("d0"), quarter),
            (1, tau, Some("d0"), tau),
            (3, quarter, None, tau),
            (3, 0.0, None, tau),
        ] {
            let dc = Definitions::from_records(
                24,
                &crate::testdesign::revolve_design(24, extent, angle),
            );
            let d = design(&dc, DisplayUnit::Mm, "revolved.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let detail = &json["timeline"]["items"][1]["detail"];
            let a = &detail["extentDefinition"]["angle"];
            assert_eq!(a["name"].as_str(), name, "extent {extent}: {detail}");
            assert!((a["value"].as_f64().unwrap() - value).abs() < 1e-12);
            // The square's side on the axis.
            assert_eq!(detail["axis"]["kind"], "sketch_entity");
        }
        let dc = Definitions::from_records(24, &crate::testdesign::revolve_design(24, 2, 1.0));
        let d = design(&dc, DisplayUnit::Mm, "revolved.ipt", None);
        assert!(
            d.notes[0].contains("revolution extent Some(2)"),
            "{:?}",
            d.notes
        );
    }

    #[test]
    fn a_flagged_line_that_a_profile_side_ends_on_bounds_it() {
        let line = |id: &str, a: [f64; 2], b: [f64; 2]| {
            json!({"id": id, "type": "SketchLine", "isConstruction": true,
                   "geometry": {"type": "Line3D", "startPoint": [a[0], a[1], 0.0],
                                "endPoint": [b[0], b[1], 0.0]}})
        };
        let mut items = vec![
            json!({"index": 0, "objectType": "Sketch", "detail": {"curves": [
                // Along the profile's bottom side, ten times as long.
                line("c0", [0.0, 0.0], [10.0, 0.0]),
                // Its copy.
                line("c1", [10.0, 0.0], [0.0, 0.0]),
                // Apart from it.
                line("c2", [0.0, 3.0], [10.0, 3.0]),
                // Across it.
                line("c3", [4.5, -1.0], [4.5, 2.0]),
            ]}}),
        ];
        let sketches = HashMap::from([(7, (0, "Sketch1".to_owned()))]);
        let flagged =
            HashMap::from([(7, vec!["c0".into(), "c1".into(), "c2".into(), "c3".into()])]);
        let outlines = vec![(7, 1, vec![[4.0, 0.0], [5.0, 0.0], [5.0, 1.0], [4.0, 1.0]])];
        bounding_curves(&mut items, &sketches, &flagged, &outlines);
        let construction: Vec<bool> = items[0]["detail"]["curves"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["isConstruction"] == true)
            .collect();
        assert_eq!(construction, [false, true, true, true]);
    }

    #[test]
    fn a_flagged_line_between_profiles_of_one_selection_bounds_nothing() {
        // A long line along the side two squares share, its ends outside
        // them: it bounds them only when two features take them apart, or
        // when one feature has the first square twice.
        let first = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let second = vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]];
        for (groups, other, construction) in [
            ((1, 1), &second, true),
            ((1, 2), &second, false),
            ((1, 1), &first, false),
        ] {
            let mut items = vec![
                json!({"index": 0, "objectType": "Sketch", "detail": {"curves": [
                    {"id": "c0", "type": "SketchLine", "isConstruction": true,
                     "geometry": {"type": "Line3D", "startPoint": [1.0, -1.0, 0.0],
                                  "endPoint": [1.0, 2.0, 0.0]}},
                ]}}),
            ];
            let sketches = HashMap::from([(7, (0, "Sketch1".to_owned()))]);
            let flagged = HashMap::from([(7, vec!["c0".into()])]);
            let outlines = vec![(7, groups.0, first.clone()), (7, groups.1, other.clone())];
            bounding_curves(&mut items, &sketches, &flagged, &outlines);
            let c = &items[0]["detail"]["curves"][0];
            assert_eq!(c["isConstruction"] == true, construction, "{groups:?}");
        }
        // One feature takes both squares, another the second one alone:
        // the line bounds the second's profile.
        let mut items = vec![
            json!({"index": 0, "objectType": "Sketch", "detail": {"curves": [
                {"id": "c0", "type": "SketchLine", "isConstruction": true,
                 "geometry": {"type": "Line3D", "startPoint": [1.0, -1.0, 0.0],
                              "endPoint": [1.0, 2.0, 0.0]}},
            ]}}),
        ];
        let sketches = HashMap::from([(7, (0, "Sketch1".to_owned()))]);
        let flagged = HashMap::from([(7, vec!["c0".into()])]);
        let outlines = vec![(7, 1, first), (7, 1, second.clone()), (7, 2, second)];
        bounding_curves(&mut items, &sketches, &flagged, &outlines);
        assert_ne!(items[0]["detail"]["curves"][0]["isConstruction"], true);
    }

    #[test]
    fn a_curve_drawn_twice_is_construction_once() {
        use crate::testdesign::DesignWriter;
        let mut w = DesignWriter::new(24);
        let sketch = w.sketch();
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let transform = w.transform(identity);
        let normal = w.direction([0.0, 0.0, 1.0]);
        let a = w.sketch_point(sketch, [0.0, 0.0]);
        let b = w.sketch_point(sketch, [1.0, 0.0]);
        let c = w.sketch_point(sketch, [1.0, 0.0]);
        let first = w.sketch_line(sketch, a, b, 0);
        let copy = w.sketch_line(sketch, c, a, 0);
        w.finish_sketch(sketch, &[a, b, c, first, copy], transform, normal);
        w.label(sketch, "Sketch1", &[], [0; 16]);
        w.top.push(sketch);
        let dc = Definitions::from_records(24, &w.finish());
        let d = design(&dc, DisplayUnit::Mm, "twice.ipt", None);
        let json = serde_json::to_value(&d.dump).unwrap();
        let curves = &json["timeline"]["items"][0]["detail"]["curves"];
        assert!(curves[0].get("isConstruction").is_none(), "{curves}");
        assert_eq!(curves[1]["isConstruction"], true);
    }

    #[test]
    fn reads_mid_planes_and_planes_at_an_angle() {
        use crate::testdesign::{ANGLE, DEGREE, DesignWriter};
        let mut w = DesignWriter::new(24);
        let angle_unit = w.unit(ANGLE, 1.0);
        let deg = w.unit(DEGREE, 1.0);
        let n = w.number(30.0, deg);
        let d0 = w.parameter("d0", angle_unit, n, std::f64::consts::FRAC_PI_6);
        let plane = |w: &mut DesignWriter, name: &str, v: [f64; 9]| {
            let mut b = w.header(None);
            b.extend_from_slice(&w.prefix());
            for x in v {
                b.extend_from_slice(&x.to_le_bytes());
            }
            let p = w.push(WORK_PLANE, b);
            w.label(p, name, &[], [0; 16]);
            w.top.push(p);
            p
        };
        let one = plane(&mut w, "Work Plane1", [0., 0., 1., 1., 0., 0., 0., 1., 0.]);
        let two = plane(&mut w, "Work Plane2", [0., 0., 3., 1., 0., 0., 0., 1., 0.]);
        let mid = plane(&mut w, "Work Plane3", [0., 0., 2., 1., 0., 0., 0., 1., 0.]);
        let (s, c) = (0.5, 3f64.sqrt() / 2.0);
        let turned = plane(&mut w, "Work Plane4", [0., 0., 0., 1., 0., 0., 0., c, s]);
        let def = |w: &mut DesignWriter, t: [u8; 16], refs: &[usize]| {
            let mut b = w.header(None);
            b.extend_from_slice(&(-1i32).to_le_bytes());
            for &r in refs {
                b.extend_from_slice(&((r as u32 + 1) | 0x8000_0000).to_le_bytes());
            }
            w.push(t, b);
        };
        def(&mut w, crate::features::PLANE_MID, &[mid, one, two]);
        let mut axis = w.header(None);
        axis.extend_from_slice(&w.prefix());
        for v in [0.0f64, 0.0, 0.0, 1.0, 0.0, 0.0] {
            axis.extend_from_slice(&v.to_le_bytes());
        }
        axis.push(0);
        let axis = w.push(crate::features::WORK_AXIS, axis);
        def(
            &mut w,
            crate::features::PLANE_ANGLE,
            &[turned, axis, one, d0],
        );
        let dc = Definitions::from_records(24, &w.finish());
        let d = design(&dc, DisplayUnit::Mm, "planes.ipt", None);
        let json = serde_json::to_value(&d.dump).unwrap();
        let items = json["timeline"]["items"].as_array().unwrap();
        let def = |name: &str| {
            items
                .iter()
                .find(|i| i["name"] == name)
                .map(|i| i["detail"]["definition"].clone())
                .unwrap_or_default()
        };
        let mid = def("Work Plane3");
        assert_eq!(
            mid["_type"], "ConstructionPlaneMidplaneDefinition",
            "{json}"
        );
        assert_eq!(mid["planarEntityOne"]["timeline_index"], 0);
        assert_eq!(mid["planarEntityTwo"]["timeline_index"], 1);
        let turned = def("Work Plane4");
        assert_eq!(turned["_type"], "ConstructionPlaneAtAngleDefinition");
        assert_eq!(turned["linearEntity"]["origin"], "X");
        assert_eq!(turned["planarEntity"]["timeline_index"], 0);
        assert_eq!(turned["angle"]["name"], "d0");
    }

    #[test]
    fn reads_planes_through_points_and_axes() {
        for major in [18, 24, 25, 28] {
            let dc = Definitions::from_records(major, &crate::testdesign::planes_design(major));
            let d = design(&dc, DisplayUnit::Mm, "planes.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let items = json["timeline"]["items"].as_array().unwrap();
            let def = |name: &str| {
                items
                    .iter()
                    .find(|i| i["name"] == name)
                    .map(|i| i["detail"]["definition"].clone())
                    .unwrap_or_default()
            };
            let three = def("Work Plane1");
            assert_eq!(
                three["_type"], "ConstructionPlaneThreePointsDefinition",
                "{json}"
            );
            assert_eq!(
                three["pointEntityTwo"]["geometry"]["origin"],
                json!([1.0, 0.0, 1.0])
            );
            let two = def("Work Plane2");
            assert_eq!(two["_type"], "ConstructionPlaneTwoEdgesDefinition");
            assert_eq!(
                two["linearEntityTwo"]["geometry"]["origin"],
                json!([1.0, 2.0, 0.0])
            );
            let along = def("Work Plane3");
            assert_eq!(along["_type"], "ConstructionPlaneLineAndPointDefinition");
            assert_eq!(along["linearEntity"]["origin"], "Z");
            let normal = def("Work Plane4");
            assert_eq!(normal["_type"], "ConstructionPlaneNormalToLineDefinition");
            assert_eq!(
                normal["pointEntity"]["geometry"]["origin"],
                json!([0.0, 0.0, 3.0])
            );
        }
    }

    #[test]
    fn a_rectangle_has_right_angles_and_a_driven_angle_holds_nothing() {
        use crate::testdesign::{ANGLE, DEGREE, DesignWriter};
        let mut w = DesignWriter::new(24);
        let angle_unit = w.unit(ANGLE, 1.0);
        let deg = w.unit(DEGREE, 1.0);
        let n = w.number(360.0, deg);
        let d0 = w.parameter("d0", angle_unit, n, std::f64::consts::TAU);
        // Computed by the model.
        w.records[d0].1[10..14].copy_from_slice(&0x0102_4200u32.to_le_bytes());
        let sketch = w.sketch();
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let transform = w.transform(identity);
        let normal = w.direction([0.0, 0.0, 1.0]);
        let corners = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]];
        let points: Vec<usize> = corners.iter().map(|&c| w.sketch_point(sketch, c)).collect();
        let sides: Vec<usize> = (0..4)
            .map(|i| w.sketch_line(sketch, points[i], points[(i + 1) % 4], 0))
            .collect();
        let entities: Vec<Option<usize>> = sides.iter().chain(&points).map(|&e| Some(e)).collect();
        let rectangle = w.sketch_constraint(crate::sketch::RECTANGLE, None, &entities);
        let angle = w.sketch_constraint(
            crate::sketch::POINTS_ANGLE,
            Some(d0),
            &[None, Some(points[0]), Some(points[2]), Some(points[2])],
        );
        let mut all = points.clone();
        all.extend(&sides);
        all.extend([rectangle, angle]);
        w.finish_sketch(sketch, &all, transform, normal);
        w.label(sketch, "Sketch1", &[], [0; 16]);
        w.top.push(sketch);
        let dc = Definitions::from_records(24, &w.finish());
        let d = design(&dc, DisplayUnit::Mm, "rectangle.ipt", None);
        let json = serde_json::to_value(&d.dump).unwrap();
        let constraints = &json["timeline"]["items"][0]["detail"]["constraints"];
        let kinds: Vec<&str> = constraints
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["type"].as_str())
            .collect();
        assert_eq!(kinds, ["PerpendicularConstraint"; 3], "{constraints}");
        assert_eq!(
            constraints[2]["refs"],
            json!({"lineOne": "c2", "lineTwo": "c3"})
        );
    }

    #[test]
    fn reads_segments() {
        use crate::testdesign::DesignWriter;
        let mut w = DesignWriter::new(24);
        let sketch = w.sketch();
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let transform = w.transform(identity);
        let normal = w.direction([0.0, 0.0, 1.0]);
        let a = w.sketch_point(sketch, [0.0, 0.0]);
        let b = w.sketch_point(sketch, [2.0, 1.0]);
        let segment = w.sketch_segment(sketch, a, b);
        let nothing = w.sketch_segment(sketch, b, b);
        w.finish_sketch(sketch, &[a, b, segment, nothing], transform, normal);
        w.label(sketch, "Sketch1", &[], [0; 16]);
        w.top.push(sketch);
        let dc = Definitions::from_records(24, &w.finish());
        let d = design(&dc, DisplayUnit::Mm, "segments.ipt", None);
        let json = serde_json::to_value(&d.dump).unwrap();
        let sketch = &json["timeline"]["items"][0]["detail"];
        assert_eq!(sketch["counts"]["curves"], 1, "{sketch}");
        let c = &sketch["curves"][0];
        assert_eq!(c["type"], "SketchLine");
        assert_eq!(
            (c["startSketchPoint"].as_str(), c["endSketchPoint"].as_str()),
            (Some("p0"), Some("p1"))
        );
        assert_eq!(c["geometry"]["endPoint"], json!([2.0, 1.0, 0.0]));
        assert_eq!(sketch["constraints"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn reads_ellipses() {
        use crate::testdesign::DesignWriter;
        let mut w = DesignWriter::new(24);
        let sketch = w.sketch();
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let transform = w.transform(identity);
        let normal = w.direction([0.0, 0.0, 1.0]);
        let center = w.sketch_point(sketch, [1.0, 2.0]);
        let full = w.sketch_ellipse(sketch, center, [0.0, -1.0], [3.0, 2.0], &[]);
        let a = w.sketch_point(sketch, [1.0, -1.0]);
        let b = w.sketch_point(sketch, [3.0, 2.0]);
        let arc = w.sketch_ellipse(sketch, center, [0.0, -1.0], [3.0, 2.0], &[a, b]);
        w.finish_sketch(sketch, &[center, a, b, full, arc], transform, normal);
        w.label(sketch, "Sketch1", &[], [0; 16]);
        w.top.push(sketch);
        let dc = Definitions::from_records(24, &w.finish());
        let d = design(&dc, DisplayUnit::Mm, "ellipses.ipt", None);
        let json = serde_json::to_value(&d.dump).unwrap();
        let sketch = &json["timeline"]["items"][0]["detail"];
        let curves = &sketch["curves"];
        assert_eq!(curves[0]["type"], "SketchEllipse", "{sketch}");
        assert_eq!(curves[0]["centerSketchPoint"], "p0");
        let g = &curves[0]["geometry"];
        assert_eq!(g["type"], "Ellipse3D");
        assert_eq!(g["center"], json!([1.0, 2.0, 0.0]));
        assert_eq!(g["majorAxis"], json!([0.0, -1.0, 0.0]));
        assert_eq!(
            (g["majorRadius"].as_f64(), g["minorRadius"].as_f64()),
            (Some(3.0), Some(2.0))
        );
        assert_eq!(curves[1]["type"], "SketchEllipticalArc");
        assert_eq!(curves[1]["geometry"]["type"], "EllipticalArc3D");
        assert_eq!(
            (
                curves[1]["startSketchPoint"].as_str(),
                curves[1]["endSketchPoint"].as_str()
            ),
            (Some("p1"), Some("p2"))
        );
        assert_eq!(sketch["constraints"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn reads_coils() {
        for (kind, name, made) in [
            (0, "PitchAndRevolutionCoilType", ["d0", "d2"]),
            (1, "RevolutionAndHeightCoilType", ["d2", "d1"]),
            (2, "PitchAndHeightCoilType", ["d0", "d1"]),
        ] {
            let dc = Definitions::from_records(24, &crate::testdesign::coil_design(24, kind));
            let d = design(&dc, DisplayUnit::Mm, "coil.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let item = &json["timeline"]["items"][1];
            assert_eq!(item["objectType"], "CoilFeature", "{item}");
            let detail = &item["detail"];
            assert_eq!(detail["coilType"], name);
            assert_eq!(detail["operation"], "CutFeatureOperation");
            assert_eq!(detail["pitch"]["name"], "d0");
            assert_eq!(detail["height"]["name"], "d1");
            assert_eq!(detail["revolutions"]["name"], "d2");
            assert_eq!(detail["angle"]["name"], "d3");
            // The square's side on the axis.
            assert_eq!(detail["axis"]["kind"], "sketch_entity", "{detail}");
            // The sizes its type uses (and the taper) are its parameters.
            let owned: Vec<&str> = json["parameters"]["model"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["createdBy"]["timeline_index"] == 1)
                .filter_map(|p| p["name"].as_str())
                .collect();
            let mut want: Vec<&str> = made.iter().copied().chain(["d3"]).collect();
            want.sort_unstable();
            assert_eq!(owned, want, "{kind}");
        }
        let dc = Definitions::from_records(24, &crate::testdesign::coil_design(24, 3));
        let d = design(&dc, DisplayUnit::Mm, "coil.ipt", None);
        assert!(d.notes[0].contains("coil type Some(3)"), "{:?}", d.notes);
        // An axis along y through (2, 0, 0): on no line of the sketch and
        // no origin axis, it is given by its geometry.
        let mut records = crate::testdesign::coil_design(24, 0);
        let (_, axis) = records
            .iter_mut()
            .find(|(t, _)| *t == crate::features::WORK_AXIS)
            .unwrap();
        let at = axis.len() - 49;
        axis[at..at + 8].copy_from_slice(&2.0f64.to_le_bytes());
        let dc = Definitions::from_records(24, &records);
        let d = design(&dc, DisplayUnit::Mm, "coil.ipt", None);
        let json = serde_json::to_value(&d.dump).unwrap();
        let axis = &json["timeline"]["items"][1]["detail"]["axis"];
        assert_eq!(axis["kind"], "construction_axis", "{axis}");
        assert_eq!(axis["geometry"]["origin"], json!([2.0, 0.0, 0.0]));
        assert_eq!(axis["geometry"]["direction"], json!([0.0, 1.0, 0.0]));
    }

    #[test]
    fn reads_threads() {
        for (full, modeled) in [(true, false), (false, true)] {
            let dc =
                Definitions::from_records(24, &crate::testdesign::thread_design(24, full, modeled));
            let d = design(&dc, DisplayUnit::Mm, "thread.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let item = &json["timeline"]["items"][0];
            assert_eq!(item["objectType"], "ThreadFeature", "{item}");
            let detail = &item["detail"];
            assert_eq!(
                detail["threadInfo"],
                json!({"threadType": "ISO Metric profile", "threadDesignation": "M6x1",
                       "threadClass": "6g"})
            );
            assert_eq!(detail["isModeled"], modeled);
            assert_eq!(detail["isFullLength"], full);
            let face = &detail["inputCylindricalFaces"][0];
            assert_eq!(face["geometry"]["axis"], json!([0.0, 0.0, 1.0]));
            assert_eq!(face["start_point"], json!([0.0, 0.0, 0.0]));
            assert_eq!(face["end_point"], json!([0.0, 0.0, 1.0]));
            if full {
                assert!(detail.get("threadLength").is_none(), "{detail}");
            } else {
                assert_eq!(detail["threadLength"]["name"], "d0");
                assert_eq!(detail["threadOffset"]["name"], "d1");
            }
        }
    }

    #[test]
    fn reads_an_offset_chain() {
        for major in [21, 24, 25, 28] {
            let dc = Definitions::from_records(major, &crate::testdesign::offset_design(major));
            let d = design(&dc, DisplayUnit::Mm, "offset.ipt", None);
            let json = serde_json::to_value(&d.dump).unwrap();
            let sketch = &json["timeline"]["items"][0]["detail"];
            let offset = &sketch["constraints"][0];
            assert_eq!(offset["type"], "OffsetConstraint", "{sketch}");
            assert_eq!(sketch["constraints"].as_array().unwrap().len(), 1);
            let props = &offset["props"];
            assert_eq!(props["parentCurves"], json!(["c0", "c1", "c2", "c3"]));
            assert_eq!(props["childCurves"], json!(["c4", "c5", "c6", "c7"]));
            assert!((props["distance"].as_f64().unwrap() - 0.25).abs() < 1e-12);
            assert_eq!(props["dimension"], sketch["dimensions"][0]["id"]);
            assert_eq!(sketch["dimensions"][0]["type"], "SketchOffsetDimension");
        }
    }

    #[test]
    fn reads_a_distance_between_concentric_circles() {
        use crate::testdesign::{DISTANCE, DesignWriter, LENGTH, METRE};
        let mut w = DesignWriter::new(24);
        let length = w.unit(LENGTH, 1.0);
        let mm = w.unit(METRE, 1e-3);
        let n = w.number(0.2, mm);
        let d0 = w.parameter("d0", length, n, 0.2);
        let n = w.number(0.5, mm);
        let d1 = w.parameter("d1", length, n, 0.5);
        let sketch = w.sketch();
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let transform = w.transform(identity);
        let normal = w.direction([0.0, 0.0, 1.0]);
        let center = w.sketch_point(sketch, [0.0, 0.0]);
        let inner = w.sketch_circle(sketch, center, 1.0, &[]);
        let outer = w.sketch_circle(sketch, center, 1.2, &[]);
        let gap = w.sketch_constraint(DISTANCE, Some(d0), &[None, Some(inner), Some(outer)]);
        // A distance from a point to a circle's edge stays out.
        let away = w.sketch_point(sketch, [3.0, 0.0]);
        let edge = w.sketch_constraint(DISTANCE, Some(d1), &[None, Some(away), Some(outer)]);
        w.finish_sketch(
            sketch,
            &[center, away, inner, outer, gap, edge],
            transform,
            normal,
        );
        w.label(sketch, "Sketch1", &[], [0; 16]);
        w.top.push(sketch);
        let dc = Definitions::from_records(24, &w.finish());
        let d = design(&dc, DisplayUnit::Mm, "circles.ipt", None);
        let json = serde_json::to_value(&d.dump).unwrap();
        let sketch = &json["timeline"]["items"][0]["detail"];
        assert_eq!(sketch["counts"]["curves"], 2);
        assert_eq!(sketch["counts"]["dimensions"], 1);
        let gap = &sketch["dimensions"][0];
        assert_eq!(gap["type"], "SketchConcentricCircleDimension");
        assert_eq!(gap["refs"], json!({"circleOne": "c0", "circleTwo": "c1"}));
        assert_eq!(gap["parameter"]["name"], "d0");
        let left_out = sketch["constraints"][0]["type"].as_str().unwrap();
        assert!(left_out.contains("a distance to a circle"), "{left_out}");
    }

    #[test]
    fn reads_combines_shells_and_splits() {
        for major in [21, 24, 28] {
            let dc =
                Definitions::from_records(major, &crate::testdesign::combine_design(major, true));
            let d = design(&dc, DisplayUnit::Mm, "combine.ipt", None);
            let types: Vec<&str> = d.items.iter().map(|i| i.object_type.as_str()).collect();
            assert_eq!(
                types,
                [
                    "Sketch",
                    "ExtrudeFeature",
                    "Sketch",
                    "ExtrudeFeature",
                    "CombineFeature",
                    "ShellFeature",
                    "SplitBodyFeature"
                ],
                "major {major}"
            );
            let json = serde_json::to_value(&d.dump).unwrap();
            let items = &json["timeline"]["items"];
            // The bodies by the extrusions that made them.
            let combine = &items[4]["detail"];
            assert_eq!(combine["operation"], "JoinFeatureOperation");
            assert_eq!(combine["isKeepToolBodies"], false);
            assert_eq!(combine["_ipt_inputs"], true);
            assert_eq!(combine["targetBody"]["name"], "Solid1");
            assert_eq!(combine["targetBody"]["_f3d"]["producer"], 1);
            assert_eq!(combine["toolBodies"][0]["_f3d"]["producer"], 3);
            assert_eq!(combine["toolBodies"][0]["_f3d"]["body_index"], 0);
            // A new body names no participants.
            assert!(items[1]["detail"].get("participantBodies").is_none());
            let shell = &items[5]["detail"];
            assert_eq!(shell["insideThickness"]["name"], "d2");
            assert_eq!(shell["_ipt_removed_faces"], 1);
            assert_eq!(shell["inputEntities"][0]["_f3d"]["producer"], 1);
            let split = &items[6]["detail"];
            assert_eq!(split["splitBodies"][0]["name"], "Solid1");
            assert_eq!(
                split["splittingTool"]["geometry"]["origin"],
                json!([0.0, 0.0, 0.5])
            );
        }
    }

    #[test]
    fn a_sweep_path_is_the_curves_on_its_wire() {
        // A sketch on the XZ plane (x along x, y along z) with a line and
        // an arc after it, and a line off the path; the path runs along
        // the first two.
        let r = 1.0;
        let sketch = json!({"index": 0, "objectType": "Sketch", "detail": {
        "model_frame": {"origin": [0.0, 0.0, 0.0], "x_axis": [1.0, 0.0, 0.0],
                        "y_axis": [0.0, 0.0, 1.0], "z_axis": [0.0, -1.0, 0.0]},
        "curves": [
            {"id": "c2", "geometry": {"type": "Arc3D", "center": [2.0, 1.0, 0.0],
                "radius": r, "startAngle": -std::f64::consts::FRAC_PI_2, "endAngle": 0.0}},
            {"id": "c1", "geometry": {"type": "Line3D", "startPoint": [0.0, 0.0, 0.0],
                "endPoint": [2.0, 0.0, 0.0]}},
            {"id": "c3", "geometry": {"type": "Line3D", "startPoint": [0.0, 5.0, 0.0],
                "endPoint": [2.0, 5.0, 0.0]}},
        ]}});
        let mut points = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        points.extend((1..=720).map(|k| {
            let a =
                -std::f64::consts::FRAC_PI_2 + std::f64::consts::FRAC_PI_2 * f64::from(k) / 720.0;
            [2.0 + r * a.cos(), 0.0, 1.0 + r * a.sin()]
        }));
        let sweep = json!({"index": 1, "objectType": "SweepFeature",
            "detail": {"_ipt_path": {"sketches": [0], "points": points}}});
        let mut items = vec![sketch, sweep];
        crate::features::path_curves(&mut items);
        let path: Vec<&str> = items[1]["detail"]["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["entity"]["id"].as_str().unwrap())
            .collect();
        assert_eq!(path, ["c1", "c2"]);
        assert!(items[1]["detail"].get("_ipt_path").is_none());
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

    #[test]
    fn reads_the_state_table_whatever_its_entries_words() {
        use crate::testdesign::DesignWriter;
        for (word, byte) in [(0x108, 2), (0x107, 1), (0x109, 2)] {
            let mut w = DesignWriter::new(24);
            w.states_marked(&[(7, 2, 5), (9, 5, 8)], word, byte);
            let dc = Definitions::from_records(24, &w.finish());
            let states = history_states(&dc);
            assert_eq!(states.get(&7), Some(&vec![2, 5]), "{word:#x}");
            assert_eq!(states.get(&9), Some(&vec![5, 8]), "{word:#x}");
        }
    }

    #[test]
    fn a_sheet_metal_group_takes_its_last_childs_state() {
        use crate::testdesign::DesignWriter;
        let mut w = DesignWriter::new(24);
        let child = |w: &mut DesignWriter| {
            let b = w.header(None);
            let node = u32::from_le_bytes(b[b.len() - 4..].try_into().unwrap());
            (w.push(FEATURE, b), node)
        };
        let (plate, plate_node) = child(&mut w);
        let (bend, bend_node) = child(&mut w);
        let header = w.header(None);
        let group = w.push(type_id("904bc1c4d311ff2860004da99dccefb0"), header);
        w.label(group, "Face1", &[plate, bend], [0; 16]);
        w.top.push(group);
        w.states(&[(plate_node, 2, 3), (bend_node, 3, 4)]);
        let dc = Definitions::from_records(24, &w.finish());
        let d = design(&dc, DisplayUnit::Mm, "sheet.ipt", None);
        let items: Vec<(&str, &str, Option<i64>)> = d
            .items
            .iter()
            .map(|i| (i.name.as_str(), i.object_type.as_str(), i.state))
            .collect();
        assert_eq!(items, [("Face1", "SheetMetalFaceFeature", Some(4))]);
    }
}
