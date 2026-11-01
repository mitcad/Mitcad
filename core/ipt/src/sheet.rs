// SPDX-License-Identifier: MIT
//! Sheet-metal features (mitcad#211) as the dump IR's sheet items
//! (`core/import/SCHEMA.md`, *Sheet metal*), which the import replays as
//! MitCad's native sheet features (`sheet_rules`, `sheet_face`, ...).
//!
//! A sheet-metal feature of the browser is a group record (a face
//! `904bc1c4…`, a flange `08dcddc3…`) whose label shows the features it is
//! made of, ordinary feature records (`914d8790…`) named `Plate<n>`,
//! `Bend<n>` and `Corner<n>` (see `README.md`, *Sheet metal*). The rules
//! of the part (thickness, bend radius, reliefs) are the sheet-metal style
//! of the application segment, whose values the part's sheet-metal
//! parameters (`Thickness`, `BendRadius`, ...) hold with their
//! expressions; the K-factor is the style's unfold rule's.

use serde_json::{Value, json};

use crate::IptFile;
use crate::dc::{self, Label, type_id};
use crate::features::{Context, ENUM_OPERATION, Translated};

/// A sheet-metal face (a group of a plate, and bends where it joins the
/// sheet) *(seen)*.
pub const FACE_GROUP: [u8; 16] = type_id("904bc1c4d311ff2860004da99dccefb0");
/// A sheet-metal flange (a group of plates, bends and corners) *(seen)*.
pub const FLANGE_GROUP: [u8; 16] = type_id("08dcddc3d311d0976000a7a99dccefb0");
/// A plate's side enumeration (slot 2 of a plate) *(seen)*.
pub const PLATE_SIDE: [u8; 16] = type_id("69b9505efb4bbaa925c3dfa870817810");
/// A plate's or flange's path selection (slot 0 of a plate), of the layout
/// of a profile selection *(seen)*.
pub const PATH_SELECTION: [u8; 16] = type_id("473f20fcd11101cf000835bd0663dc09");

/// The application segment's sheet-metal style (`6bc0d610…`) *(seen)*.
pub const STYLE: [u8; 16] = type_id("6bc0d61008462369c73c0faca268550f");
/// The application segment's unfold rule (`3e0b6bda…`) *(seen)*.
pub const UNFOLD_RULE: [u8; 16] = type_id("3e0b6bdaf5417a648da91fbd093fe213");

/// The sheet-metal style of a part, as far as it is read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    /// The style's name (`Default_mm`).
    pub name: Option<String>,
    /// The unfold rule's name and its K-factor: the value and the text it
    /// is stored as (`0.440 ul`).
    pub unfold: Option<String>,
    pub k_factor: Option<(f64, String)>,
}

/// The UTF-16 texts of a record: a u32 count of 1 to 255 code units
/// followed by that many printable units, each with its offset.
pub fn texts(bytes: &[u8]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 4 <= bytes.len() {
        let n =
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
        let end = at + 4 + 2 * n;
        if (1..=255).contains(&n) && end <= bytes.len() {
            let units: Vec<u16> = (at + 4..end)
                .step_by(2)
                .map(|k| u16::from_le_bytes([bytes[k], bytes[k + 1]]))
                .collect();
            if units
                .iter()
                .all(|&u| (0x20..0x7f).contains(&u) || u >= 0xa0)
                && let Ok(s) = String::from_utf16(&units)
            {
                out.push((at, s));
                at = end;
                continue;
            }
        }
        at += 1;
    }
    out
}

/// A K-factor's text (`0.440 ul`): its value.
fn k_value(text: &str) -> Option<f64> {
    let number = text.strip_suffix(" ul").unwrap_or(text).trim();
    let v: f64 = number.parse().ok()?;
    (0.0..=1.0).contains(&v).then_some(v)
}

impl Style {
    /// The style of a part: its application segment's style and unfold
    /// rule records. A part keeps one unfold rule *(seen: every
    /// sheet-metal part of the corpora)*; its first text is its name, its
    /// second its internal name (`1:Default_KFactor`), and the next its
    /// K-factor as a unitless value (`0.440 ul`) *(seen)*.
    pub fn read(file: &IptFile) -> Style {
        let mut style = Style::default();
        let Some(segment) = file.segments.iter().find(|s| s.name == "PmAppSegment") else {
            return style;
        };
        let Ok(content) = file.segment_data(segment) else {
            return style;
        };
        let Ok(records) = &content.records else {
            return style;
        };
        for r in records {
            let t = &content.tables.types[r.type_index()].id;
            let bytes = &content.data[r.range.clone()];
            if *t == UNFOLD_RULE && style.unfold.is_none() {
                let texts = texts(bytes);
                style.unfold = texts.first().map(|t| t.1.clone());
                style.k_factor = texts
                    .iter()
                    .skip(2)
                    .find_map(|(_, s)| k_value(s).map(|v| (v, s.clone())));
            } else if *t == STYLE && style.name.is_none() {
                style.name = texts(bytes).first().map(|t| t.1.clone());
            }
        }
        style
    }
}

/// A parameter of the part's table by its name, as the dump's parameter
/// reference.
fn named(cx: &Context, name: &str) -> Option<Value> {
    let p = cx
        .params
        .list
        .iter()
        .find(|p| p.in_table && p.name == name)?;
    Some(json!({"kind": "parameter", "name": p.name, "value": p.value}))
}

/// The rules a sheet-metal feature's body follows: the thickness its plate
/// names, and the style's other values as the part's sheet-metal
/// parameters hold them (`BendRadius`, `BendReliefWidth`,
/// `BendReliefDepth`, `CornerReliefSize` *(seen: the style's expressions
/// in that order)*); the K-factor from the unfold rule. The relief shapes
/// of the style are not decoded.
fn rules(cx: &Context, style: &Style, thickness: Value) -> Value {
    let mut inferred = vec![
        "bend_relief.type: the style's relief shapes (enumerations of record 6bc0d610) \
         are not decoded"
            .to_owned(),
        "corner_relief.type: the style's relief shapes are not decoded".to_owned(),
        "corner_relief.depth: the file has none (MitCad does not use it)".to_owned(),
    ];
    let mut need = |name: &str, what: &str| {
        named(cx, name).unwrap_or_else(|| {
            inferred.push(format!("{what}: no parameter {name}"));
            Value::Null
        })
    };
    let bend_radius = need("BendRadius", "bend_radius");
    let relief_width = need("BendReliefWidth", "bend_relief.width");
    let relief_depth = need("BendReliefDepth", "bend_relief.depth");
    let corner = need("CornerReliefSize", "corner_relief.width");
    let mut r = json!({
        "name": style.name,
        "thickness": thickness,
        "bend_radius": bend_radius,
        "bend_relief": {"type": "rectangular", "width": relief_width, "depth": relief_depth},
        "corner_relief": {"type": "round", "width": corner, "depth": corner},
    });
    match &style.k_factor {
        Some((k, text)) => r["k_factor"] = json!({"value": k, "text": text, "rule": style.unfold}),
        None => inferred.push("k_factor: the unfold rule's K-factor is not read".to_owned()),
    }
    r["inferred"] = json!(inferred);
    r
}

/// The plates of a sheet-metal group: its label's features that select a
/// path (slot 0), in the label's order.
fn plates(cx: &Context, label: Option<&Label>) -> Vec<usize> {
    label
        .map_or(&[][..], |l| l.children.as_slice())
        .iter()
        .copied()
        .filter(|&c| cx.dc.is(c, &crate::design::FEATURE))
        .filter(|&c| {
            cx.slots(c)
                .ok()
                .and_then(|s| s.first().copied().flatten())
                .is_some_and(|s| cx.dc.is(s, &PATH_SELECTION))
        })
        .collect()
}

/// The profile references of a plate's path selection in its sketch: one
/// of the selection as measured (`profile`), or, where its loops lie apart
/// (a face of several separate regions *(seen)*), one per loop.
fn profile_refs(
    cx: &Context,
    selection: usize,
    sketch: usize,
    index: i64,
    name: &str,
) -> Option<Vec<Value>> {
    let m = crate::sketch::sketch_matrix(cx.dc, sketch).ok()?;
    let reference = |area: f64, c: [f64; 2]| {
        json!({"kind": "profile", "sketch": name, "sketch_timeline_index": index,
               "area": area, "centroid": [c[0], c[1], 0.0]})
    };
    let loops = cx.profiles.borrow_mut().loops(cx.dc, selection)?;
    let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
    let (x, y, o) = (column(0), column(1), column(3));
    let local = |p: [f64; 3]| {
        let d = [p[0] / 10.0 - o[0], p[1] / 10.0 - o[1], p[2] / 10.0 - o[2]];
        [
            d[0] * x[0] + d[1] * x[1] + d[2] * x[2],
            d[0] * y[0] + d[1] * y[1] + d[2] * y[2],
        ]
    };
    let flat: Vec<Vec<[f64; 2]>> = loops
        .iter()
        .map(|l| l.iter().map(|&p| local(p)).collect())
        .collect();
    let apart = flat.len() > 1
        && flat.iter().enumerate().all(|(i, a)| {
            flat.iter()
                .enumerate()
                .all(|(j, b)| i == j || !inside(a[0], b))
        });
    if apart {
        return Some(
            flat.iter()
                .map(|l| {
                    let (a, c) = shoelace(l);
                    reference(a.abs(), c)
                })
                .collect(),
        );
    }
    let measured = cx.profiles.borrow_mut().measure(cx.dc, selection, &m)?;
    Some(vec![reference(measured.area, measured.centroid)])
}

/// Whether a point lies inside a polygon (even-odd).
fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut odd = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if p[0] < x {
                odd = !odd;
            }
        }
    }
    odd
}

/// A polygon's signed area and centroid.
fn shoelace(p: &[[f64; 2]]) -> (f64, [f64; 2]) {
    let n = p.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (u, v) = (p[i], p[(i + 1) % n]);
        let cross = u[0] * v[1] - v[0] * u[1];
        a += cross;
        cx += (u[0] + v[0]) * cross;
        cy += (u[1] + v[1]) * cross;
    }
    a /= 2.0;
    if a.abs() < 1e-300 {
        return (0.0, [0.0, 0.0]);
    }
    (a, [cx / (6.0 * a), cy / (6.0 * a)])
}

/// The bodies of a plate: its lists of bodies (slot 15), or in the older
/// layout (major 16, 15 slots) the body its slot 6 names directly
/// *(seen: the one major-16 part, mitcad#280)*.
fn plate_bodies(cx: &Context, plate: usize) -> Vec<usize> {
    let listed = cx.feature_bodies(plate);
    if !listed.is_empty() {
        return listed;
    }
    let slots = cx.slots(plate).unwrap_or_default();
    match slots.get(6).copied().flatten() {
        Some(b) if slots.len() <= 15 && cx.dc.is(b, &crate::features::BODY) => vec![b],
        _ => Vec::new(),
    }
}

/// A plate's operation: its slot 16 (kind 5: 1 a new body, 3 joined to the
/// sheet). The older layout (major 16, 15 slots) stores none: the plate
/// makes its body when it is the first item to refer to it, as every
/// feature does (the decoder's producers), and joins it otherwise
/// (mitcad#280).
fn plate_operation(
    cx: &Context,
    slots: &[Option<usize>],
    plate: usize,
) -> Result<&'static str, String> {
    let slot16 = slots.get(16).copied().flatten();
    if slot16.is_none() && slots.len() <= 15 {
        let bodies = plate_bodies(cx, plate);
        if bodies.is_empty() {
            return Err("its body was not read (older layout)".to_owned());
        }
        return Ok(if bodies.iter().all(|b| !cx.bodies.of.contains_key(b)) {
            "NewBodyFeatureOperation"
        } else {
            "JoinFeatureOperation"
        });
    }
    match cx.enumeration(slot16, &ENUM_OPERATION) {
        Some(1) => Ok("NewBodyFeatureOperation"),
        Some(3) => Ok("JoinFeatureOperation"),
        other => Err(format!("operation {other:?} is not known")),
    }
}

/// A sheet-metal face: its plate's slots 0 the profile (a path selection
/// of the sketch's profile), 1 the direction the material lies along from
/// the sketch's plane, 2 a side enumeration (`69b9505e…`, 0 or 1), 3 the
/// thickness, 15 its bodies, 16 the operation (kind 5: 1 a new body, 3
/// joined to the sheet) *(seen)*. A face joined to the sheet has a bend
/// part of its own besides the plate, whose slot 0 lists the edges the
/// bend joins (mitcad#280).
pub fn face(
    cx: &Context,
    label: Option<&Label>,
    sketch: Option<(i64, String)>,
    sketch_record: Option<usize>,
    style: &Style,
    names: &crate::naming::Resolver,
) -> Result<Translated, String> {
    let plates = plates(cx, label);
    let [plate] = plates.as_slice() else {
        return Err(format!("{} plates", plates.len()));
    };
    let slots = cx.slots(*plate)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let operation = plate_operation(cx, &slots, *plate)?;
    let thickness = cx
        .parameter(slot(3), &mut t.parameters)
        .ok_or("no thickness")?;
    let direction = cx.direction(slot(1)).ok_or("no direction")?;
    // (The older layout's side is a boolean, false in its one face.)
    let side = cx
        .enumeration(slot(2), &PLATE_SIDE)
        .or_else(|| cx.boolean(slot(2)).map(u16::from));
    let Some((index, name)) = sketch else {
        return Err("no sketch".to_owned());
    };
    let profile = slot(0)
        .zip(sketch_record)
        .and_then(|(s, k)| profile_refs(cx, s, k, index, &name))
        .ok_or("its profile was not measured")?;
    // (The sheet-metal parameters belong to the style, not to the item.)
    let rules = rules(cx, style, thickness.clone());
    let mut s = json!({
        "kind": "face",
        "operation": operation,
        "profile": profile,
        "thickness": thickness,
        "direction": direction,
        "rules": rules,
    });
    // The side enumeration: 1 puts the material against the direction
    // (mitcad#242) *(verified: the joined faces reached with a valid sheet,
    // side 1, lie in their panel's plane; the new-body faces, side 0, along
    // the direction)*.
    if let Some(side) = side {
        s["side"] = json!(side);
    }
    // A face joined to the sheet: its bend's radius (the bend part's slot
    // 1), the edges its bend joins (the bend part's slot 0, a list of edge
    // items read by their names; empty where the face is joined without a
    // bend, mitcad#280) and the body it joins.
    if operation == "JoinFeatureOperation" {
        let bends = bends(cx, label);
        let [bend] = bends.as_slice() else {
            return Err(format!("a joined face of {} bends", bends.len()));
        };
        let bend_slots = cx.slots(*bend)?;
        let radius = cx
            .parameter(bend_slots.get(1).copied().flatten(), &mut t.parameters)
            .ok_or("no bend radius")?;
        let list = bend_slots.first().copied().flatten();
        let items = list.map_or_else(Vec::new, |e| names.naming.items_under(e));
        // (The joined faces bent onto the sheet list two items of another
        // class, `3839a63b…`, naming the face's own plate: not decoded,
        // mitcad#280.)
        let listed = cx.list(list).map_or(0, |l| l.len());
        let mut edge_list = Vec::new();
        let mut edge_errors = Vec::new();
        for (k, e) in names.edges_of(&items, *bend).into_iter().enumerate() {
            match e {
                Ok(e) => edge_list.push(e),
                Err(why) => edge_errors.push(format!("edge {}: {why}", k + 1)),
            }
        }
        s["bend"] = json!({
            "radius": radius,
            "listed": listed,
            "edges": items.len(),
            "edge_list": edge_list,
            "edge_errors": edge_errors,
        });
        s["participantBodies"] = json!(
            plate_bodies(cx, *plate)
                .iter()
                .map(|&b| cx.bodies.reference(b, None))
                .collect::<Vec<_>>()
        );
    }
    t.detail = json!({ "sheet": s });
    t.raw = Some(json!({"sheet": {"side": side, "plate": plate}}));
    Ok(t)
}

/// A flange's or plate's dimension record (`b1377779…`): after the header
/// the two points it measures between (cm, model coordinates) and, among
/// the references after them, its parameter *(seen)*.
pub const DIMENSION: [u8; 16] = type_id("b1377779d2116794a00049803603c8c9");
/// An edge of a feature's edge list (`82695c37…`): a B-rep naming id, not
/// decoded *(seen)*.
pub const EDGE_ITEM: [u8; 16] = type_id("82695c37d111516b0008a1ba32a3dc09");

/// The points and parameter of a dimension record.
fn dimension(cx: &Context, record: usize) -> Option<([f64; 3], [f64; 3], usize)> {
    let b = cx.dc.bytes(record);
    let mut r = dc::Reader::at(b, cx.dc.header_len());
    let a = [r.f64().ok()?, r.f64().ok()?, r.f64().ok()?];
    let e = [r.f64().ok()?, r.f64().ok()?, r.f64().ok()?];
    let param = (r.pos..b.len().saturating_sub(3)).find_map(|k| {
        let v = u32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]);
        dc::reference(v)
            .filter(|&p| v & 0x8000_0000 != 0 && p < cx.dc.len())
            .filter(|&p| cx.params.by_record(p).is_some())
    })?;
    Some((a, e, param))
}

/// The edge items under a record (an edge list and the lists in it).
fn edge_items(cx: &Context, record: usize) -> usize {
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![(record, 0)];
    let mut n = 0;
    while let Some((r, depth)) = stack.pop() {
        if !seen.insert(r) || depth > 4 {
            continue;
        }
        if cx.dc.is(r, &EDGE_ITEM) {
            n += 1;
            continue;
        }
        let b = cx.dc.bytes(r);
        let mut at = cx.dc.header_len();
        while at + 4 <= b.len() {
            let v = u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
            match dc::reference(v) {
                Some(c) if v & 0x8000_0000 != 0 && c < cx.dc.len() => {
                    stack.push((c, depth + 1));
                    at += 4;
                }
                _ => at += 1,
            }
        }
    }
    n
}

/// A flange's width extents (its plate's slot 14, `19dfdc53…`): a list of
/// extents of its edges.
pub const WIDTH_EXTENTS: [u8; 16] = type_id("19dfdc53834c227cef4e61826c1a938c");
/// A flange edge's width extent (`f922288f…`): after the header and the
/// prefix the item of its edge, then the two points the flange spans (cm)
/// *(seen)*.
pub const WIDTH_EXTENT: [u8; 16] = type_id("f922288f234a108352131e91e1bc70c4");
/// The definition of a width extent (`fe193a84…`, the extent's only
/// referrer besides the root collection): after the header u32 its type
/// (4: offsets from the edge's two ends), u32 2, then the body, the edge's
/// item, an internal variable (the width), a direction, the thickness, 0,
/// the extent, the bodies, 0, two work points (the ends of the edge the
/// offsets are measured from) and the two offsets *(seen: the 5 extents of
/// the corpora, all of type 4)*.
pub const WIDTH_DEFINITION: [u8; 16] = type_id("fe193a84d4110d0f600014aa9dccefb0");

/// The width extents of a flange's edges by their items: the ends the
/// offsets are measured from (cm) and the offsets, or why they were not
/// read.
fn width_extents(
    cx: &Context,
    list: Option<usize>,
    made: &mut Vec<usize>,
) -> std::collections::HashMap<usize, Result<Value, String>> {
    let mut out = std::collections::HashMap::new();
    let extents = cx
        .list(list.filter(|&l| cx.dc.is(l, &WIDTH_EXTENTS)))
        .unwrap_or_default();
    for x in extents.into_iter().filter(|&x| cx.dc.is(x, &WIDTH_EXTENT)) {
        let Ok(Some(item)) = cx.dc.fields(x).reference() else {
            continue;
        };
        let read = |made: &mut Vec<usize>| -> Result<Value, String> {
            let refers = |d: usize, r: usize| {
                let b = cx.dc.bytes(d);
                let v = 0x8000_0000 | (r as u32 + 1);
                (0..b.len().saturating_sub(3))
                    .any(|k| u32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]) == v)
            };
            let def = cx
                .dc
                .of_type(&WIDTH_DEFINITION)
                .find(|&d| refers(d, x))
                .ok_or("its width extent's definition was not found")?;
            let mut r = cx.dc.body(def);
            let words = (0..15)
                .map(|_| r.u32())
                .collect::<dc::Result<Vec<u32>>>()
                .map_err(|e| format!("its width extent: {e}"))?;
            if words[0] != 4 {
                return Err(format!("a width extent of type {} (not decoded)", words[0]));
            }
            let at = |k: usize| dc::reference(words[k]).filter(|&r| r < cx.dc.len());
            let point = |k: usize| {
                at(k)
                    .and_then(|p| crate::features::work_point(cx.dc, p))
                    .ok_or("its width extent's ends were not read")
            };
            let offset = |k: usize, made: &mut Vec<usize>| {
                cx.parameter(at(k), made)
                    .ok_or("its width extent's offsets were not read")
            };
            Ok(json!({
                "type": "offsets",
                "from": point(11)?,
                "to": point(12)?,
                "offset_from": offset(13, made)?,
                "offset_to": offset(14, made)?,
            }))
        };
        out.insert(item, read(made));
    }
    out
}

/// The bends of a sheet-metal group: its label's features that are not
/// plates and name a bend radius and the plate's bend record (slot 1 a
/// parameter) *(seen: `Bend<n>` parts)*.
fn bends(cx: &Context, label: Option<&Label>) -> Vec<usize> {
    let plates = plates(cx, label);
    label
        .map_or(&[][..], |l| l.children.as_slice())
        .iter()
        .copied()
        .filter(|&c| cx.dc.is(c, &crate::design::FEATURE) && !plates.contains(&c))
        .filter(|&c| {
            cx.slots(c)
                .ok()
                .and_then(|s| s.get(1).copied().flatten())
                .is_some_and(|p| cx.params.by_record(p).is_some())
        })
        .collect()
}

/// A sheet-metal flange: its plate's slots 0 the flange's path (an open
/// wire across the middle of the edge: from the stationary tangent line on
/// the inner face through the bend to the flange's end), 1 a direction, 3
/// the thickness, 8 its dimensions (the length from its datum point to the
/// flange's end, and the angle), 12 the edges (items, read by their names:
/// [`crate::naming`]), 14 their width extents, 15 its bodies, 16 the
/// operation; its bend's slot 1 the bend
/// radius *(seen)*. The path and the length's datum point are given as
/// stored; the import tells the flange's position and length datum from
/// them.
pub fn flange(
    cx: &Context,
    label: Option<&Label>,
    style: &Style,
    names: &crate::naming::Resolver,
) -> Result<Translated, String> {
    let plates = plates(cx, label);
    let [plate] = plates.as_slice() else {
        return Err(format!("{} plates", plates.len()));
    };
    let bends = bends(cx, label);
    let [bend] = bends.as_slice() else {
        return Err(format!("{} bends", bends.len()));
    };
    let slots = cx.slots(*plate)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let operation = plate_operation(cx, &slots, *plate)?;
    let thickness = cx
        .parameter(slot(3), &mut Vec::new())
        .ok_or("no thickness")?;
    let edges = slot(12).map_or(0, |e| edge_items(cx, e));
    // Its edges, by their names, found in the history state the flange
    // starts from, each with its width extent where it has one (slot 14)
    // (mitcad#242).
    let items = slot(12).map_or_else(Vec::new, |e| names.naming.items_under(e));
    let extents = width_extents(cx, slot(14), &mut t.parameters);
    let mut edge_list: Vec<Value> = Vec::new();
    let mut edge_errors: Vec<String> = Vec::new();
    for (k, (item, found)) in items.iter().zip(names.edges_of(&items, *plate)).enumerate() {
        match (found, extents.get(item)) {
            (Ok(mut e), Some(Ok(extent))) => {
                e["width_extent"] = extent.clone();
                edge_list.push(e);
            }
            (Ok(e), None) => edge_list.push(e),
            (Err(why), _) => edge_errors.push(format!("edge {}: {why}", k + 1)),
            (_, Some(Err(why))) => edge_errors.push(format!("edge {}: {why}", k + 1)),
        }
    }
    let path: Vec<[f64; 3]> = slot(0)
        .and_then(|s| cx.profiles.borrow_mut().loops(cx.dc, s))
        .and_then(|l| l.into_iter().next())
        .ok_or("its path was not read")?
        .into_iter()
        .map(|p| p.map(|x| x / 10.0))
        .collect();
    if path.len() < 3 {
        return Err("its path has too few points".to_owned());
    }
    // Its dimensions: the length (a length parameter) and the angle.
    let mut length = None;
    let mut angle = None;
    for d in cx.list(slot(8)).unwrap_or_default() {
        if !cx.dc.is(d, &DIMENSION) {
            continue;
        }
        let Some((a, e, p)) = dimension(cx, d) else {
            continue;
        };
        let Some(param) = cx.params.by_record(p) else {
            continue;
        };
        let is_angle = matches!(param.quantity, Ok(crate::params::Quantity::Angle));
        let reference = cx.parameter(Some(p), &mut t.parameters);
        if is_angle {
            angle = reference;
        } else {
            length = reference.map(|r| (r, a, e));
        }
    }
    let (length, datum, end) = length.ok_or("no length dimension")?;
    let angle = angle.ok_or("no angle dimension")?;
    let bend_slots = cx.slots(*bend)?;
    let radius = cx
        .parameter(bend_slots.get(1).copied().flatten(), &mut t.parameters)
        .ok_or("no bend radius")?;
    // Its path, at most 97 points.
    let step = (path.len() / 96).max(1);
    let mut sampled: Vec<[f64; 3]> = path.iter().step_by(step).copied().collect();
    if sampled.last() != path.last() {
        sampled.push(*path.last().expect("points"));
    }
    t.detail = json!({"sheet": {
        "kind": "flange",
        "operation": operation,
        "edges": edges,
        "edge_list": edge_list,
        "edge_errors": edge_errors,
        "path": sampled,
        "length": length,
        "length_datum": datum,
        "length_end": end,
        "angle": angle,
        "radius": radius,
        "thickness": thickness,
        "participantBodies": plate_bodies(cx, *plate).iter()
            .map(|&b| cx.bodies.reference(b, None)).collect::<Vec<_>>(),
        "rules": {"name": style.name},
        "corner": corner_settings(cx, slot(9), &mut t.parameters),
    }});
    t.raw = Some(json!({"sheet": {"plate": plate, "bend": bend}}));
    Ok(t)
}

/// A flange's corner settings (`83ce7786…`).
pub const CORNER_SETTINGS: [u8; 16] = type_id("83ce7786a946cde11ebd99b4fe02afb2");
/// The corner settings' relief shape enumeration (`720c3e2b…`, kind 501).
pub const RELIEF_SHAPE: [u8; 16] = type_id("720c3e2bd311976060007aa99dccefb0");
/// The corner settings' fifth reference (`e0a8d8af…`, kind 8).
pub const CORNER_KIND: [u8; 16] = type_id("e0a8d8afd3117d5a0008928f8b1f0f69");

/// A flange's corner settings (its plate's slot 9, `83ce7786…`): after the
/// header the references 0 a boolean (true: the style's settings; false:
/// the flange's own), 1 the gap (`GapSize`), 2 the corner relief size
/// (`CornerReliefSize`), 3 the corner relief shape (`720c3e2b…`, kind 501:
/// 102 or 103 with the style's settings, 5 in the one flange with its own),
/// 4 an enumeration (`e0a8d8af…`, kind 8: 3 in every flange), 5 an
/// enumeration (`720c3e2b…`: 303 in every flange), 6 `JacobiRadiusSize`, 7
/// none, 8 a boolean (true in every flange) *(seen: the 35 flanges of the
/// corpora)*. Which corner the shapes and the gap make is not decoded
/// beyond that (mitcad#242: the one flange with its own settings has a gap,
/// thickness and bend radius all 1.5 mm, which its corner's dimensions do
/// not tell apart).
fn corner_settings(cx: &Context, record: Option<usize>, made: &mut Vec<usize>) -> Value {
    let Some(record) = record.filter(|&r| cx.dc.is(r, &CORNER_SETTINGS)) else {
        return Value::Null;
    };
    let b = cx.dc.bytes(record);
    let start = cx.dc.header_len() + cx.dc.prefix_len();
    let refs: Vec<Option<usize>> = (start..b.len().saturating_sub(3))
        .step_by(4)
        .take(9)
        .map(|k| {
            dc::reference(u32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]))
                .filter(|&r| r < cx.dc.len())
        })
        .collect();
    let at = |i: usize| refs.get(i).copied().flatten();
    json!({
        "style": cx.boolean(at(0)),
        "gap": cx.parameter(at(1), made),
        "relief_size": cx.parameter(at(2), made),
        "relief_shape": cx.enumeration(at(3), &RELIEF_SHAPE),
        "kind": cx.enumeration(at(4), &CORNER_KIND),
        "other": cx.enumeration(at(5), &RELIEF_SHAPE),
        "jacobi_radius": cx.parameter(at(6), made),
        "flag": cx.boolean(at(8)),
    })
}

/// A sheet-metal cut: a feature record of the extrusion's layout (slots 0
/// operation, 1 profiles, 2 direction, 3 reversed, 4 distance, 5 taper, 6
/// extent) *(seen: every cut of the corpus, operation 2)*, read as an
/// extrusion; the import makes it a cut through the sheet's thickness.
pub fn cut(
    cx: &Context,
    record: usize,
    sketch: Option<(i64, String)>,
    sketch_record: Option<usize>,
    items: &std::collections::HashMap<usize, i64>,
) -> Result<Translated, String> {
    let mut t = crate::features::extrude(cx, record, sketch, sketch_record, items)?;
    let extrude = std::mem::take(&mut t.detail);
    let direction = t
        .raw
        .as_ref()
        .and_then(|r| r["extrude"]["direction_vector"].as_array().cloned());
    // Slots 31 and 33: two booleans of the sheet-metal cut beyond the
    // extrusion's layout (false in every cut of the corpora *(seen)*; the
    // cuts that reach a bend, with both false, cut straight through it,
    // mitcad#242).
    let slots = cx.slots(record)?;
    let flag = |i: usize| cx.boolean(slots.get(i).copied().flatten());
    let options = json!([flag(31), flag(33)]);
    t.detail = json!({"sheet": {
        "kind": "cut",
        "options": options,
        "operation": extrude["operation"],
        "profile": extrude["profile"],
        "extentType": extrude["extentType"],
        "extentOne": extrude["extentOne"],
        "direction": direction,
        "participantBodies": extrude.get("participantBodies").cloned()
            .unwrap_or_else(|| json!(cx.feature_bodies(record).iter()
                .map(|&b| cx.bodies.reference(b, None)).collect::<Vec<_>>())),
    }});
    Ok(t)
}

/// A sheet-metal corner round or chamfer: a feature record of a fillet's
/// layout (slot 0 the edge sets `dae9481b…`) or a chamfer's (slot 0 the
/// edges, 2 and 3 the distances, 4 the form `3200aa7d…`, 10 the angle)
/// *(seen: every corner round and chamfer of the corpora)*, read as one;
/// its edges are read by their names ([`crate::naming`]) and given with
/// each edge set.
pub fn corner(
    cx: &Context,
    record: usize,
    round: bool,
    names: &crate::naming::Resolver,
) -> Result<Translated, String> {
    let mut t = if round {
        crate::features::fillet(cx, record)?
    } else {
        crate::features::chamfer(cx, record)?
    };
    let mut dressup = std::mem::take(&mut t.detail);
    // The edges of each set by their names, found in the history state the
    // feature starts from (mitcad#242): a round's sets are its slot 0's
    // edge sets (each's first reference its edges), a chamfer's one set its
    // slot 0.
    let slots = cx.slots(record)?;
    let slot0 = slots.first().copied().flatten();
    let roots: Vec<usize> = if round {
        cx.list(slot0.filter(|&r| cx.dc.is(r, &crate::features::FILLET_SETS)))
            .unwrap_or_default()
            .into_iter()
            .filter(|&s| cx.dc.is(s, &crate::features::FILLET_SET))
            .filter_map(|s| cx.dc.fields(s).reference().ok().flatten())
            .collect()
    } else {
        slot0.into_iter().collect()
    };
    let mut errors = Vec::new();
    if let Some(sets) = dressup["edgeSets"].as_array_mut() {
        if roots.len() != sets.len() {
            errors.push(format!(
                "{} edge lists for {} edge sets",
                roots.len(),
                sets.len()
            ));
        } else {
            for (k, (set, root)) in sets.iter_mut().zip(&roots).enumerate() {
                let mut edges = Vec::new();
                for (j, e) in names.edges_under(*root, record).into_iter().enumerate() {
                    match e {
                        Ok(e) => edges.push(e),
                        Err(e) => errors.push(format!("set {}, edge {}: {e}", k + 1, j + 1)),
                    }
                }
                if edges.is_empty() && errors.is_empty() {
                    errors.push(format!("set {}: no edges named", k + 1));
                }
                set["edges"] = json!(edges);
            }
        }
    }
    t.detail = json!({"sheet": {
        "edge_errors": errors,
        "kind": if round { "corner_round" } else { "corner_chamfer" },
        "dressup": dressup,
        "participantBodies": cx.feature_bodies(record).iter()
            .map(|&b| cx.bodies.reference(b, None)).collect::<Vec<_>>(),
    }});
    Ok(t)
}

/// A sheet-metal bend between two faces of the sheet (`BendFeature`, a
/// feature record `914d8790…` of the layout of a flange's bend part):
/// slots 0 a list of the two edges it joins (B-rep names, read with
/// [`crate::naming`]), 1 the bend radius, 2 the thickness, 3 the bend
/// record (`92596ba9…`), 4 the bend relief (`64880596…`), 7 the transition,
/// 9 its bodies *(seen: the 4 bends of the corpora)* (mitcad#242).
pub fn bend(
    cx: &Context,
    record: usize,
    names: &crate::naming::Resolver,
) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let radius = cx
        .parameter(slot(1), &mut t.parameters)
        .ok_or("no bend radius")?;
    let thickness = cx
        .parameter(slot(2), &mut Vec::new())
        .ok_or("no thickness")?;
    let items = slot(0).map_or_else(Vec::new, |e| names.naming.items_under(e));
    let mut edge_list = Vec::new();
    let mut edge_errors = Vec::new();
    for (k, e) in names.edges_of(&items, record).into_iter().enumerate() {
        match e {
            Ok(e) => edge_list.push(e),
            Err(why) => edge_errors.push(format!("edge {}: {why}", k + 1)),
        }
    }
    t.detail = json!({"sheet": {
        "kind": "bend",
        "edges": items.len(),
        "edge_list": edge_list,
        "edge_errors": edge_errors,
        "bend_record": bend_record(cx, slot(3)),
        "radius": radius,
        "thickness": thickness,
        "participantBodies": cx.feature_bodies(record).iter()
            .map(|&b| cx.bodies.reference(b, None)).collect::<Vec<_>>(),
    }});
    Ok(t)
}

/// A bend record (`92596ba9…`: a plate's slot 4, a bend part's and a bend
/// feature's slot 3).
pub const BEND_RECORD: [u8; 16] = type_id("92596ba9d311efa86000b3a99dccefb0");
/// A bend record's enumerations (`7267cc3b…`, kind 9).
pub const BEND_KIND: [u8; 16] = type_id("7267cc3bd31104a96000b3a99dccefb0");

/// A bend record (mitcad#280): after the header and the prefix the
/// references 0 a boolean, 1 the thickness, 2 an enumeration (`7267cc3b…`,
/// kind 9), 3 and 4 two directions, 5 the bend radius, 6 an enumeration
/// (`7267cc3b…`), 7 and 8 two lengths, 9 and 10 booleans *(seen: every
/// bend record of the sheet-metal parts)*. The enumerations are 1 and 1 in
/// a flange's bend, 0 and 1 in a joined face's bend part, 3 and 5 in a bend
/// feature joining parallel faces (a double bend of two 45-degree bends,
/// one of its edges fixed); the directions are zero but in those bend
/// features: the bends' axis `a` and the direction `d` from the fixed edge's
/// panel towards the other panel's plane, `a x d` pointing from the fixed
/// edge away from its panel's material *(seen: the four double bends of the
/// corpora, each giving its history state; the fixed edge is also the first
/// of the two in each)*.
fn bend_record(cx: &Context, record: Option<usize>) -> Value {
    let Some(record) = record.filter(|&r| cx.dc.is(r, &BEND_RECORD)) else {
        return Value::Null;
    };
    let b = cx.dc.bytes(record);
    let start = cx.dc.header_len() + cx.dc.prefix_len();
    let refs: Vec<Option<usize>> = (start..b.len().saturating_sub(3))
        .step_by(4)
        .take(10)
        .map(|k| {
            dc::reference(u32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]))
                .filter(|&r| r < cx.dc.len())
        })
        .collect();
    let at = |i: usize| refs.get(i).copied().flatten();
    // (A zero direction is no direction: `direction` reads unit vectors.)
    json!({
        "kinds": [cx.enumeration(at(2), &BEND_KIND), cx.enumeration(at(6), &BEND_KIND)],
        "axis": cx.direction(at(3)),
        "direction": cx.direction(at(4)),
    })
}

/// The record of a fold's line: a list (`911b3d83…`) of a work axis on the
/// line of the fold's sketch *(seen)*.
pub const FOLD_LINE: [u8; 16] = type_id("911b3d83d411e38f00000688149c4721");
/// A fold's enumeration of slot 0 (`c12c2606…`) *(seen: 1)*.
pub const FOLD_KIND: [u8; 16] = type_id("c12c2606d411b98700000288149c4721");

/// A sheet-metal fold (`FoldFeature`, a feature record of its own) *(seen:
/// one fold, in a part saved by a 2012 release)*: slots 0 an enumeration
/// (`c12c2606…`, 1), 1 and 2 booleans (both false), 3 the angle, 4 the bend
/// radius, 5 its line (a list of a work axis on the line of the fold's
/// sketch, cm), 6 the bend relief, 7 the body, 8 and 12 booleans, 10 the
/// thickness, 13 the transition, 14 an enumeration (mitcad#242). Its line
/// is a line of the sketch before it, the work axis running from the
/// line's start to its end (mitcad#280). With the enumeration 1 and both
/// booleans false, the bend starts at the line and the material on the
/// line's right (seen along the sketch's normal) stays, the other side
/// turning against the sketch's normal *(seen: the one fold of the
/// corpora, a sketch on the outside of a flange's wall, its history
/// state; the sketch's normal and the side away from the material are one
/// there, so which of them the turn follows is not told apart)*; the
/// other values are not decoded.
pub fn fold(
    cx: &Context,
    record: usize,
    sketch: Option<(i64, String)>,
) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let angle = cx
        .parameter(slot(3), &mut t.parameters)
        .ok_or("no fold angle")?;
    let radius = cx
        .parameter(slot(4), &mut t.parameters)
        .ok_or("no bend radius")?;
    let line = cx
        .list(slot(5).filter(|&r| cx.dc.is(r, &FOLD_LINE)))
        .and_then(|l| l.first().copied())
        .and_then(|a| crate::features::axis(cx.dc, a))
        .ok_or("its line was not read")?;
    let flags: Vec<Option<bool>> = [1, 2, 8, 12]
        .into_iter()
        .map(|i| cx.boolean(slot(i)))
        .collect();
    t.detail = json!({"sheet": {
        "kind": "fold",
        "angle": angle,
        "radius": radius,
        "line": {"point": line.0, "direction": line.1},
        "kind_enumeration": cx.enumeration(slot(0), &FOLD_KIND),
        "flags": flags,
        "sketch": sketch.map(|(index, name)| json!({"name": name, "timeline_index": index})),
        "participantBodies": fold_bodies(cx, record, slot(7)).iter()
            .map(|&b| cx.bodies.reference(b, None)).collect::<Vec<_>>(),
    }});
    Ok(t)
}

/// A fold's bodies: its lists of bodies, or the body its slot 7 names
/// directly (the older layout, mitcad#280).
fn fold_bodies(cx: &Context, record: usize, slot7: Option<usize>) -> Vec<usize> {
    let listed = cx.feature_bodies(record);
    if !listed.is_empty() {
        return listed;
    }
    slot7
        .filter(|&b| cx.dc.is(b, &crate::features::BODY))
        .into_iter()
        .collect()
}

/// The bodies of a sheet-metal group's plates.
pub fn group_bodies(cx: &Context, label: Option<&Label>) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for p in plates(cx, label) {
        for b in plate_bodies(cx, p) {
            if !out.contains(&b) {
                out.push(b);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str) -> Vec<u8> {
        let mut b = (s.encode_utf16().count() as u32).to_le_bytes().to_vec();
        b.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
        b
    }

    /// An unfold rule's texts: its name, internal name and the K-factor as
    /// a unitless value.
    #[test]
    fn the_unfold_rule_keeps_its_k_factor_as_text() {
        let mut b = vec![0u8; 27];
        b.extend(utf16("Default_KFactor"));
        b.extend([0; 6]);
        b.extend(utf16("1:Default_KFactor"));
        b.extend([0x0A, 0, 1, 0, 0, 0]);
        b.extend(utf16("0.440 ul"));
        let t = texts(&b);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].1, "Default_KFactor");
        assert_eq!(k_value(&t[2].1), Some(0.44));
        assert_eq!(k_value("1.5 ul"), None);
    }
}
