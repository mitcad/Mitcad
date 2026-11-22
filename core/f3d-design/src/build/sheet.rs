// SPDX-License-Identifier: MIT
//! Sheet-metal flanges (mitcad#211) as the dump IR's sheet items
//! (`core/import/SCHEMA.md`, *Sheet metal*). Layouts follow the cadmpeg
//! format description (*BaseFlange*, *EdgeFlange*), checked on the
//! corpus's flanges; the sheet-metal rule's layout is our reading
//! *(seen)*.
//!
//! - A **base flange** (`Face<n>`): the sketch profile it selects, its
//!   thickness along the sketch's positive normal, as thick as its rule
//!   (the f64 at offset 123 of the object *(verified: equal to the rule's
//!   thickness parameter in every base flange of the corpus)*).
//! - An **edge flange** (`EdgeFlange<n>`): at `85 + S` (S is 0 or 4, the
//!   one whose marked references name the angle's and height's parameter
//!   holders) `u32` bend position (1 outside the edge, 2 inside it, 3 at
//!   it, 4 tangent to a side plane), `u32` edge count, the edge wrapper
//!   and settings references, `u32` height datum (1 the inner faces, 2 the
//!   outer ones, 3 the plane tangent to the bend's outer surface), the
//!   angle's and the height's holders, a `u32`, and 15 bytes after that
//!   `u32` the inside bend radius (f64, cm). The edge is its edge input,
//!   found in the history by its names as fillet edges are. Width owners
//!   (`EdgeWidth` both ways, `EdgeWidth_1` and `EdgeWidth_2` towards the
//!   edge's start and end) are distances from the edge's midpoint
//!   (mitcad#242).
//! - The **rule** (`B42ED662…`, one a design *(seen)*): expressions, each
//!   a `str16` and its value (f64, cm), in this order: bend radius, bend
//!   relief width, bend relief depth, two more; five GUID texts; `u32 0`,
//!   `u32 3`; the corner relief size; `u32 0`; the K-factor (f64); the
//!   material's name; another expression; the thickness as a `str16` and
//!   its value, then a reference to the thickness parameter *(seen: the
//!   corpus's designs of one rule; the order of the relief fields is
//!   taken from their default expressions, not from variants)*. A later
//!   writer keeps the K-factor in an unfold rule the rule refers to
//!   ([`UNFOLD_RULE`]): the f64 after its root part (mitcad#2117); the
//!   rule then has the material's name where the K-factor was.
//! - A **hem** (`Hem<n>`, mitcad#1364; cadmpeg's *Hem* layouts): its
//!   parameters name their roles (`HemGap`, `HemLength`, `HemRadius`,
//!   `HemAngle`, `HemSetback`) and their holders are the marked owner
//!   references of its fixed section. The section at base `b` (`85 + S`):
//!   `u32` bend position (4, tangent to the side face, the edge flange's
//!   code *(seen: every hem)*), `u32` edge count 1, the edge wrapper and
//!   settings references, then the owners from `b + 41`; after them (at
//!   `b + 66` with two owners, `b + 76` with three) a `u32` form, then a
//!   byte and the rule's inside bend radius (f64, cm).
//!   Forms: 1 open, 2 rolled, 3 double, 4 teardrop, 5 rope, 6 flat
//!   *(ours: one hem of each form in the reference models, each made as
//!   that form; the description takes the form from the parameters and
//!   the result instead)*. The edge is its edge input, as an edge
//!   flange's.
//! - An **Unfold** (`EC187027…`, `Unfold<n>`, mitcad#2117): its input
//!   slots (`2CA5A1CD…`, as an extrusion's, [`decode::extent_slots`]) in
//!   the order the record names them: the bends' faces (role 16, a face
//!   input per bend face: both cylinders of each bend it unfolds), the
//!   stationary face (role 16, one face input) and the body (role 4); then
//!   a reference to the sheet-metal rule (`E75C86D6…`) *(seen: one Unfold,
//!   twelve bends of three edge flanges named by their 24 faces)*.
//! - A **Refold** (`483D584F…`, `Refold<n>`): one input slot, the body
//!   (role 8), and the rule's reference; it names no bend nor the Unfold
//!   (a group item of class `99F375B3…`, `UnfoldRefoldGroupMode`, pairs
//!   the two), so it closes every bend the Unfold before it opened
//!   *(seen: the Unfold's pair)*.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::super::classes::{BODY_REF, FACE_REF};
use super::super::decode::{self, ParameterEntry, TimelineEntry};
use super::super::ir::*;
use super::super::stream::{f64_at, str16_at, u32_at};
use super::{Builder, edge_input, face_input, paramref};

/// The flanges' class (base and edge flanges, hems).
pub const FLANGE: &str = "7AFA0A65-B53C-472C-ACA4-5C7C0A65001C";

/// The Unfold's class (mitcad#2117).
pub const UNFOLD: &str = "EC187027-FF97-48A3-8031-F923807E630B";

/// The Refold's class (mitcad#2117).
pub const REFOLD: &str = "483D584F-29AA-46DA-ABA2-F139B0ED8100";

/// The role of an Unfold's face slots (its bends' and its stationary
/// face's).
const UNFOLD_FACES: u32 = 16;

/// The roles of a slot of the bodies an Unfold or Refold works on.
const UNFOLD_BODIES: [u32; 2] = [decode::PARTICIPANT_ROLE, decode::PARTICIPANT_ROLE_2];

/// An Unfold's inputs by their slots: the bend faces' inputs, the
/// stationary face's input and the body inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct UnfoldSlots {
    bends: Vec<u64>,
    stationary: u64,
    bodies: Vec<u64>,
}

/// An Unfold's slots (role, inputs) in the order its record names them:
/// the first face slot holds the bends' faces, the second the stationary
/// face (one input); the body slots the bodies.
fn unfold_slots(slots: &[(u32, Vec<u64>)]) -> Result<UnfoldSlots, String> {
    let faces: Vec<&Vec<u64>> = slots
        .iter()
        .filter(|(role, _)| *role == UNFOLD_FACES)
        .map(|(_, inputs)| inputs)
        .collect();
    let [bends, stationary] = faces.as_slice() else {
        return Err(format!(
            "{} face slots (role {UNFOLD_FACES}): the bends' and the stationary face's are \
             expected",
            faces.len()
        ));
    };
    let [stationary] = stationary.as_slice() else {
        return Err(format!(
            "its stationary face's slot holds {} inputs (one is expected)",
            stationary.len()
        ));
    };
    if bends.is_empty() {
        return Err("its bends' slot holds no face".to_owned());
    }
    Ok(UnfoldSlots {
        bends: bends.to_vec(),
        stationary: *stationary,
        bodies: unfold_bodies(slots),
    })
}

/// The inputs of the body slots of an Unfold or Refold.
fn unfold_bodies(slots: &[(u32, Vec<u64>)]) -> Vec<u64> {
    let mut out = Vec::new();
    for (_, inputs) in slots.iter().filter(|(r, _)| UNFOLD_BODIES.contains(r)) {
        for &i in inputs {
            if !out.contains(&i) {
                out.push(i);
            }
        }
    }
    out
}

/// The sheet-metal rule's class.
pub const RULE: &str = "B42ED662-8999-4C1B-A744-759D1D446D55";

/// A rule's decoded fields.
#[derive(Clone, Debug, Default)]
pub struct Rule {
    pub name: Option<String>,
    /// The expressions with their values (cm), in order.
    pub expressions: Vec<(String, f64)>,
    pub k_factor: Option<f64>,
    pub thickness: Option<u64>,
}

/// The expressions of a rule: `str16` texts followed by an f64, read
/// from the start of the object; GUID texts and others are passed over.
fn rule(d: &[u8]) -> Rule {
    let mut r = Rule::default();
    let mut p = 0;
    let mut texts: Vec<(String, usize, usize)> = Vec::new();
    while p + 4 <= d.len() {
        match str16_at(d, p) {
            Some((s, end))
                if !s.is_empty() && s.len() < 256 && s.chars().all(|c| !c.is_control()) =>
            {
                texts.push((s, p, end));
                p = end;
            }
            _ => p += 1,
        }
    }
    for (s, _, end) in &texts {
        let is_guid = s.len() == 36 && s.chars().filter(|&c| c == '-').count() == 4;
        if is_guid {
            continue;
        }
        if let Some(v) = f64_at(d, *end).filter(|v| v.is_finite() && *v > 1e-9 && *v < 1e4) {
            r.expressions.push((s.clone(), v));
        }
    }
    // The K-factor: the f64 four bytes after the corner relief size's
    // value, the material's name right after it. A writer that keeps the
    // K-factor in an unfold rule of its own (`UNFOLD_RULE`, mitcad#2117) has
    // the material's name there instead: no K-factor here.
    if let Some((_, _, end)) = texts.iter().find(|(s, _, _)| s.contains("* 4")) {
        let at = end + 8 + 4;
        let named_after = texts.iter().any(|(_, p, _)| *p == at + 8);
        r.k_factor = f64_at(d, at).filter(|k| named_after && plausible_k(*k));
    }
    r.name = texts
        .iter()
        .rev()
        .find(|(s, _, _)| s.contains('('))
        .map(|(s, _, _)| s.clone());
    r
}

/// The unfold rule a sheet-metal rule refers to (mitcad#2117; the class of
/// `Linear (k=0.44)`): the K-factor its own record holds.
pub const UNFOLD_RULE: &str = "B4439D38-AFFE-464E-A4CB-CD6620BEBB78";

/// A K-factor a record can hold: between 0.001 and 1 (a value read from
/// other bytes is far off it).
fn plausible_k(k: f64) -> bool {
    (1e-3..=1.0).contains(&k)
}

/// An unfold rule's K-factor: the f64 right after its root part (`start`),
/// else the `k=` its name gives (`Linear (k=0.44)`) *(seen: one such
/// rule, both 0.44)*.
fn unfold_rule_k(d: &[u8], start: usize) -> Option<f64> {
    if let Some(k) = f64_at(d, start).filter(|k| plausible_k(*k)) {
        return Some(k);
    }
    let mut p = start;
    while p + 4 <= d.len() {
        match str16_at(d, p) {
            Some((s, end)) if !s.is_empty() && s.len() < 256 => {
                if let Some(k) = s
                    .split("k=")
                    .nth(1)
                    .and_then(|t| t.trim_end_matches(')').trim().parse::<f64>().ok())
                    .filter(|k| plausible_k(*k))
                {
                    return Some(k);
                }
                p = end;
            }
            _ => p += 1,
        }
    }
    None
}

/// A hem's form by its code (the module's notes).
fn hem_form(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "open",
        2 => "rolled",
        3 => "double",
        4 => "teardrop",
        5 => "rope",
        6 => "flat",
        _ => return None,
    })
}

/// The parameter roles each hem form owns, in their sorted order.
fn hem_roles(form: &str) -> &'static [&'static str] {
    match form {
        "open" | "flat" => &["HemGap", "HemLength"],
        "rolled" => &["HemAngle", "HemRadius"],
        "teardrop" | "rope" => &["HemGap", "HemLength", "HemRadius"],
        "double" => &["HemGap", "HemLength", "HemSetback"],
        _ => &[],
    }
}

/// The fields of a hem's fixed section.
#[derive(Clone, Debug, PartialEq)]
struct HemSection {
    /// The bend position (the edge flange's codes).
    position: u32,
    /// The form's code.
    form: u32,
    /// The rule's inside bend radius (cm).
    rule_radius: f64,
}

/// A hem's fixed section: the one base `b` with the edge count 1 at
/// `b + 4`, marked references at `b + 8` and `b + 19`, and each of the
/// owners (the hem parameters' holders) once among the references from
/// `b + 40` to `b + 66`; `at` reads the marked reference at an offset.
fn hem_section(
    d: &[u8],
    at: &dyn Fn(usize) -> Option<u64>,
    owners: &[u64],
) -> Result<HemSection, String> {
    let tail = match owners.len() {
        2 => 66,
        3 => 76,
        n => return Err(format!("{n} parameter owners: no hem layout has them")),
    };
    let fits: Vec<usize> = (0..d.len().saturating_sub(tail + 13))
        .filter(|&b| {
            u32_at(d, b + 4) == Some(1)
                && at(b + 8).is_some()
                && at(b + 19).is_some()
                && owners
                    .iter()
                    .all(|&o| (b + 40..=b + 66).filter(|&q| at(q) == Some(o)).count() == 1)
        })
        .collect();
    let [b] = fits.as_slice() else {
        return Err(format!(
            "its fixed section was not found ({} fits)",
            fits.len()
        ));
    };
    let b = *b;
    let position = u32_at(d, b).ok_or("no bend position")?;
    let form = u32_at(d, b + tail).ok_or("no form")?;
    let rule_radius = f64_at(d, b + tail + 5)
        .filter(|r| r.is_finite() && *r > 0.0)
        .ok_or("no inside bend radius")?;
    Ok(HemSection {
        position,
        form,
        rule_radius,
    })
}

impl Builder<'_> {
    /// The design's sheet-metal rule.
    pub(super) fn sheet_rule(&self) -> Option<Rule> {
        let o = self.seg.objects_of(RULE).next()?;
        let d = self.seg.data(o);
        let mut r = rule(d);
        let refs = self.seg.ref_ids(d);
        r.thickness = refs
            .iter()
            .copied()
            .find(|id| self.dec.parameters.iter().any(|p| p.id == *id));
        // The K-factor of the unfold rule it refers to, where it has one
        // (mitcad#2117).
        if let Some(k) = refs
            .iter()
            .find(|id| self.seg.guid_of(**id) == Some(UNFOLD_RULE))
            .and_then(|&id| {
                let u = self.seg.data_of(id);
                unfold_rule_k(u, self.seg.root_part(u)?.end)
            })
        {
            r.k_factor = Some(k);
        }
        Some(r)
    }

    /// The rule as the IR's rules: its thickness parameter, and the other
    /// fields as expressions of it.
    fn sheet_rules_json(&self) -> Value {
        let Some(rule) = self.sheet_rule() else {
            return json!({"inferred": ["the design's sheet-metal rule was not found"]});
        };
        let thickness = rule
            .thickness
            .and_then(|id| self.dec.parameters.iter().find(|p| p.id == id));
        let mut inferred = vec![
            "bend_relief.type, corner_relief.type: the rule's relief shapes are not decoded"
                .to_owned(),
            "corner_relief.depth: the rule has none (MitCad does not use it)".to_owned(),
        ];
        // An expression of the rule in the thickness parameter's name.
        let expr = |k: usize, what: &str, inferred: &mut Vec<String>| -> Value {
            match (rule.expressions.get(k), thickness) {
                (Some((text, value)), Some(t)) => {
                    let e = text.replace("Thickness", &t.name);
                    json!({"kind": "parameter", "expression": e, "value": value,
                           "_rule_expression": text})
                }
                _ => {
                    inferred.push(format!("{what}: not read"));
                    Value::Null
                }
            }
        };
        let bend_radius = expr(0, "bend_radius", &mut inferred);
        let relief_width = expr(1, "bend_relief.width", &mut inferred);
        let relief_depth = expr(2, "bend_relief.depth", &mut inferred);
        let corner = rule
            .expressions
            .iter()
            .position(|(s, _)| s.contains("* 4"))
            .map_or(Value::Null, |k| {
                expr(k, "corner_relief.width", &mut inferred)
            });
        let mut out = json!({
            "name": rule.name,
            "thickness": thickness.map(paramref),
            "bend_radius": bend_radius,
            "bend_relief": {"type": "rectangular", "width": relief_width, "depth": relief_depth},
            "corner_relief": {"type": "round", "width": corner, "depth": corner},
        });
        match rule.k_factor {
            Some(k) => out["k_factor"] = json!({"value": k}),
            None => inferred.push("k_factor: not read".to_owned()),
        }
        out["inferred"] = json!(inferred);
        out
    }

    /// A flange item: a base flange, an edge flange or a hem.
    pub(super) fn flange(
        &self,
        it: &TimelineEntry,
        rl: &BTreeMap<&str, Vec<&ParameterEntry>>,
    ) -> Detail {
        let mut map = Map::new();
        let sheet = if rl.contains_key("FlangeHeight") {
            self.edge_flange(it, rl)
        } else if rl.keys().any(|k| k.starts_with("Hem")) {
            self.hem(it, rl)
        } else {
            Ok(self.base_flange(it))
        };
        match sheet {
            Ok(s) => {
                map.insert("sheet".to_owned(), s);
            }
            Err(e) => {
                map.insert("_sheet_error".to_owned(), json!(e));
            }
        }
        Detail::Other(map)
    }

    fn base_flange(&self, it: &TimelineEntry) -> Value {
        let mut s = json!({
            "kind": "face",
            "operation": "NewBodyFeatureOperation",
            "along_sketch_normal": true,
            "rules": self.sheet_rules_json(),
        });
        // Its profile's sketch (one profile a loop list names).
        let profile = self.profiles(it.id).or_else(|| {
            let sketch = self.profile_sketch(it.id)?;
            let f = self.feat_of_sk.get(&sketch).copied();
            Some(vec![Reference::Profile(ProfileRef {
                sketch: Some(self.name(f)),
                sketch_timeline_index: Some(self.pos(f)),
                ..ProfileRef::default()
            })])
        });
        if let Some(p) = profile {
            s["profile"] = serde_json::to_value(&p).expect("serializes");
        }
        if let Some(loops) = self.profile_loops(it.id) {
            s["_f3d_profile_loops"] = loops;
        }
        // Its own thickness (offset 123), which is the rule's.
        if let Some(t) = f64_at(self.seg.data_of(it.id), 123) {
            s["_thickness"] = json!(t);
        }
        s
    }

    fn edge_flange(
        &self,
        it: &TimelineEntry,
        rl: &BTreeMap<&str, Vec<&ParameterEntry>>,
    ) -> Result<Value, String> {
        let seg = self.seg;
        let d = seg.data_of(it.id);
        let first = |role: &str| rl.get(role).and_then(|v| v.first()).copied();
        let angle = first("FlangeAngle").ok_or("no angle")?;
        let height = first("FlangeHeight").ok_or("no height")?;
        // The section is found by the marked angle and height owners (the
        // parameters' holders), one after the other at `p`: before them
        // the height datum, the settings reference, one wrapper reference
        // per edge, the edge count and the bend position; after them a
        // u32 and, 15 bytes after it, the radius.
        let at = |p: usize| seg.ref_at(d, p).map(|r| r.id);
        let anchors: Vec<usize> = (0..d.len().saturating_sub(22))
            .filter(|&p| at(p) == angle.holder && at(p + 11) == height.holder)
            .collect();
        let [p] = anchors.as_slice() else {
            return Err(format!(
                "its fixed section was not found ({} fits)",
                anchors.len()
            ));
        };
        let p = *p;
        let datum = u32_at(d, p.checked_sub(4).ok_or("no datum")?).ok_or("no height datum")?;
        let wrappers = |e: usize| p.checked_sub(15 + 11 * e + 4);
        let edges = (1..=8usize)
            .find(|&e| {
                wrappers(e).and_then(|q| u32_at(d, q)) == Some(e as u32)
                    && (0..e).all(|k| at(wrappers(e).unwrap_or(0) + 4 + 11 * k).is_some())
            })
            .ok_or("its edge count was not found")?;
        let position =
            u32_at(d, wrappers(edges).ok_or("no position")? - 4).ok_or("no bend position")?;
        let edges = edges as u32;
        // The byte four after the u32 after the owners: 1 when the flange
        // turns to the other side *(seen: 0 or 1)*.
        let flip = match d.get(p + 26) {
            Some(0) => false,
            Some(1) => true,
            other => return Err(format!("its side byte {other:?} is not known")),
        };
        let radius = f64_at(d, p + 37)
            .filter(|r| r.is_finite() && *r > 0.0)
            .ok_or("no inside bend radius")?;
        let position = match position {
            1 => "outside",
            2 => "inside",
            3 => "adjacent",
            4 => "tangent",
            other => return Err(format!("bend position {other} is not known")),
        };
        let mode = match datum {
            1 => "inner",
            2 => "outer",
            3 => "tangent",
            other => return Err(format!("height datum {other} is not known")),
        };
        let edge_inputs: Vec<Reference> = self
            .item_inputs(it)
            .into_iter()
            .filter_map(|i| edge_input(seg, i))
            .collect();
        let widths: Vec<Value> = ["EdgeWidth", "EdgeWidth_1", "EdgeWidth_2", "EdgeOffset_1"]
            .iter()
            .filter_map(|k| first(k).map(|p| json!({"role": k, "parameter": paramref(p)})))
            .collect();
        let radius = match first("BendRadius") {
            Some(p) => serde_json::to_value(paramref(p)).expect("serializes"),
            None => json!({"value": radius, "from_rule": true}),
        };
        Ok(json!({
            "kind": "flange",
            "operation": "JoinFeatureOperation",
            "edges": edges,
            "edge": edge_inputs.first(),
            // Every edge input: per edge its group's operand, then the
            // aggregate group's (cadmpeg), so each edge may come twice.
            "edge_list": edge_inputs,
            "position": position,
            "length_mode": mode,
            "flip": flip,
            "angle": paramref(angle),
            "length": paramref(height),
            "radius": radius,
            "widths": widths,
        }))
    }

    /// A hem (mitcad#1364): its form, bend position, edge and the
    /// parameters its form owns.
    fn hem(
        &self,
        it: &TimelineEntry,
        rl: &BTreeMap<&str, Vec<&ParameterEntry>>,
    ) -> Result<Value, String> {
        let seg = self.seg;
        let d = seg.data_of(it.id);
        let params: Vec<(&str, &ParameterEntry)> = rl
            .iter()
            .filter(|(role, _)| role.starts_with("Hem"))
            .flat_map(|(role, ps)| ps.iter().map(move |p| (*role, *p)))
            .collect();
        let owners: Vec<u64> = params
            .iter()
            .map(|(_, p)| p.holder)
            .collect::<Option<Vec<u64>>>()
            .ok_or("a hem parameter without its holder")?;
        let at = |p: usize| seg.ref_at(d, p).map(|r| r.id);
        let section = hem_section(d, &at, &owners)?;
        let form = hem_form(section.form)
            .ok_or_else(|| format!("hem form {} is not known", section.form))?;
        let roles: Vec<&str> = params.iter().map(|(role, _)| *role).collect();
        if roles.as_slice() != hem_roles(form) {
            return Err(format!(
                "a {form} hem with the parameters {roles:?}: not seen"
            ));
        }
        let position = match section.position {
            4 => "tangent",
            other => return Err(format!("hem bend position {other}: not seen")),
        };
        let edge_inputs: Vec<Reference> = self
            .item_inputs(it)
            .into_iter()
            .filter_map(|i| edge_input(seg, i))
            .collect();
        let mut out = json!({
            "kind": "hem",
            "operation": "JoinFeatureOperation",
            "form": form,
            "edges": 1,
            "edge": edge_inputs.first(),
            // Its edge input: the group's operand, then the aggregate
            // group's (cadmpeg), so the edge may come twice.
            "edge_list": edge_inputs,
            "position": position,
            "rule_radius": {"value": section.rule_radius, "from_rule": true},
        });
        for (role, key) in [
            ("HemGap", "gap"),
            ("HemLength", "length"),
            ("HemRadius", "hem_radius"),
            ("HemAngle", "hem_angle"),
            ("HemSetback", "setback"),
        ] {
            if let Some((_, p)) = params.iter().find(|(r, _)| *r == role) {
                out[key] = serde_json::to_value(paramref(p)).expect("serializes");
            }
        }
        Ok(out)
    }

    /// An Unfold or Refold item (mitcad#2117): the faces and body its slots
    /// name, as a sheet item (`unfold`, `refold`).
    pub(super) fn unfold_refold(&self, it: &TimelineEntry) -> Detail {
        let mut map = Map::new();
        let sheet = if it.class == REFOLD {
            self.refold(it)
        } else {
            self.unfold(it)
        };
        match sheet {
            Ok(s) => {
                map.insert("sheet".to_owned(), s);
            }
            Err(e) => {
                map.insert("_sheet_error".to_owned(), json!(e));
            }
        }
        Detail::Other(map)
    }

    /// The slots an item names, as (role, inputs).
    fn item_slots(&self, it: &TimelineEntry) -> Vec<(u32, Vec<u64>)> {
        decode::extent_slots(self.seg, it.id)
            .into_iter()
            .map(|s| (s.role, s.inputs))
            .collect()
    }

    /// The body references of body inputs (a body slot's).
    fn unfold_body_refs(&self, inputs: &[u64]) -> Vec<Reference> {
        inputs
            .iter()
            .filter(|&&i| self.seg.guid_of(i) == Some(BODY_REF))
            .filter_map(|&i| self.body_input(i))
            .collect()
    }

    fn unfold(&self, it: &TimelineEntry) -> Result<Value, String> {
        let slots = unfold_slots(&self.item_slots(it))?;
        // A face input by its names (its geometry found in the history
        // before the item, `super::super::inputs`).
        let face = |i: u64, what: &str| -> Result<Reference, String> {
            if self.seg.guid_of(i) != Some(FACE_REF) {
                return Err(format!("{what} is no face input"));
            }
            face_input(self.seg, i).ok_or_else(|| format!("{what}'s names do not decode"))
        };
        let stationary = face(slots.stationary, "its stationary face")?;
        let bends: Vec<Reference> = slots
            .bends
            .iter()
            .enumerate()
            .map(|(k, &i)| face(i, &format!("bend face {}", k + 1)))
            .collect::<Result<_, _>>()?;
        let mut out = json!({
            "kind": "unfold",
            "stationary": stationary,
            "bends": bends,
        });
        let bodies = self.unfold_body_refs(&slots.bodies);
        if !bodies.is_empty() {
            out["participantBodies"] = json!(bodies);
        }
        Ok(out)
    }

    fn refold(&self, it: &TimelineEntry) -> Result<Value, String> {
        let slots = self.item_slots(it);
        if slots.iter().any(|(role, _)| *role == UNFOLD_FACES) {
            return Err("a refold that names faces (bends of its own): not seen".to_owned());
        }
        let mut out = json!({"kind": "refold"});
        let bodies = self.unfold_body_refs(&unfold_bodies(&slots));
        if !bodies.is_empty() {
            out["participantBodies"] = json!(bodies);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str16(s: &str) -> Vec<u8> {
        let mut b = (s.encode_utf16().count() as u32).to_le_bytes().to_vec();
        b.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
        b
    }

    /// A rule's expressions with their values, its K-factor after the
    /// corner relief size and its name (the layout of the corpus's rule).
    #[test]
    fn a_rule_gives_its_expressions_and_k_factor() {
        let mut d = vec![0u8; 20];
        for (e, v) in [
            ("Thickness", 0.25),
            ("Thickness", 0.25),
            ("Thickness * 0.5", 0.125),
            ("Thickness * 2.0", 0.5),
        ] {
            d.extend(str16(e));
            d.extend(f64::to_le_bytes(v));
        }
        d.extend(str16("00000000-0000-0000-0000-000000000000"));
        d.extend([0, 0, 0, 0, 3, 0, 0, 0]);
        d.extend(str16("Thickness * 4.0"));
        d.extend(f64::to_le_bytes(1.0));
        d.extend([0; 4]);
        d.extend(f64::to_le_bytes(0.44));
        d.extend(str16("Steel"));
        d.extend(str16("Steel (mm)"));
        let r = rule(&d);
        assert_eq!(r.expressions.len(), 5, "{:?}", r.expressions);
        assert_eq!(r.expressions[2], ("Thickness * 0.5".to_owned(), 0.125));
        assert_eq!(r.k_factor, Some(0.44));
        assert_eq!(r.name.as_deref(), Some("Steel (mm)"));
    }

    /// A rule whose writer keeps the K-factor in an unfold rule of its own
    /// (mitcad#2117): the material's name follows the corner relief size's
    /// value and its four zero bytes, so the rule gives no K-factor (the
    /// name's bytes read as one were 1.8e-306); the unfold rule's record
    /// gives it after its root part, or by its name.
    #[test]
    fn a_rule_without_its_k_factor_and_its_unfold_rule() {
        let mut d = vec![0u8; 20];
        for (e, v) in [("Thickness", 0.2), ("Thickness * 4.0", 0.8)] {
            d.extend(str16(e));
            d.extend(f64::to_le_bytes(v));
        }
        d.extend([0; 4]);
        d.extend(str16("Steel"));
        d.extend(str16("Thickness"));
        d.extend(f64::to_le_bytes(0.2));
        d.extend(str16("Steel (mm)"));
        let r = rule(&d);
        assert_eq!(r.k_factor, None, "{:?}", r.k_factor);
        assert_eq!(r.name.as_deref(), Some("Steel (mm)"));
        // The unfold rule: its K-factor right after the root part.
        let mut u = vec![0u8; 21];
        u.extend(f64::to_le_bytes(0.44));
        u.extend(str16("00000000-0000-0000-0000-000000000000"));
        u.extend([0; 4]);
        u.extend(str16("Linear (k=0.44)"));
        assert_eq!(unfold_rule_k(&u, 21), Some(0.44));
        // Without it there, by its name; with neither, none.
        let mut named = vec![0u8; 29];
        named.extend(str16("Linear (k=0.38)"));
        assert_eq!(unfold_rule_k(&named, 21), Some(0.38));
        assert_eq!(unfold_rule_k(&[0u8; 40], 21), None);
    }

    /// A marked reference (flag, id, two zero bytes).
    fn mark(id: u64) -> Vec<u8> {
        let mut b = vec![1];
        b.extend(id.to_le_bytes());
        b.extend([0, 0]);
        b
    }

    /// A hem record's fixed section (the corpus's layout) after `skip`
    /// bytes: bend position, edge count 1, wrapper and settings, the
    /// retained fields, the owners at their section offsets, the form at
    /// `tail`, a byte, the rule's radius and a result count.
    fn hem_record(skip: usize, owners: &[(usize, u64)], tail: usize, form: u32) -> Vec<u8> {
        let mut d = vec![0u8; skip];
        d.extend(4u32.to_le_bytes());
        d.extend(1u32.to_le_bytes());
        d.extend(mark(0x13f));
        d.extend(mark(0x14f));
        d.extend(1u32.to_le_bytes());
        d.extend([0, 0]);
        d.extend(4u32.to_le_bytes());
        d.resize(skip + tail, 0);
        for &(at, o) in owners {
            d[skip + at..skip + at + 11].copy_from_slice(&mark(o));
        }
        d.extend(form.to_le_bytes());
        d.push(0);
        d.extend(0.25f64.to_le_bytes());
        d.extend([0; 6]);
        d.extend(2u32.to_le_bytes());
        d.extend(mark(0x174));
        d.extend(vec![0u8; 40]);
        d
    }

    /// The marked references of a synthetic record whose ids are known.
    fn reader(d: &[u8], known: &[u64]) -> impl Fn(usize) -> Option<u64> {
        let d = d.to_vec();
        let known = known.to_vec();
        move |p: usize| {
            let b = d.get(p..p + 11)?;
            let id = u64::from_le_bytes(b[1..9].try_into().ok()?);
            (b[0] == 1 && b[9..] == [0, 0] && known.contains(&id)).then_some(id)
        }
    }

    /// An open hem's section (two owners, header shift 4) gives its form,
    /// bend position and the rule's radius; a teardrop's (three owners)
    /// its form further on; owners not in the section find none.
    #[test]
    fn a_hem_section_gives_its_form_position_and_radius() {
        let known = [0x13a, 0x13d, 0x140, 0x13f, 0x14f, 0x174];
        let d = hem_record(89, &[(42, 0x13a), (53, 0x13d)], 66, 1);
        let at = reader(&d, &known);
        let s = hem_section(&d, &at, &[0x13a, 0x13d]).expect("found");
        assert_eq!(
            s,
            HemSection {
                position: 4,
                form: 1,
                rule_radius: 0.25
            }
        );
        assert_eq!(hem_form(s.form), Some("open"));
        assert_eq!(hem_roles("open"), ["HemGap", "HemLength"]);
        // A rolled hem's owners start a byte earlier, 13 bytes apart.
        let d = hem_record(85, &[(41, 0x13a), (54, 0x13d)], 66, 2);
        let at = reader(&d, &known);
        let s = hem_section(&d, &at, &[0x13a, 0x13d]).expect("found");
        assert_eq!(hem_form(s.form), Some("rolled"));
        let d = hem_record(89, &[(42, 0x13d), (53, 0x13a), (64, 0x140)], 76, 4);
        let at = reader(&d, &known);
        let s = hem_section(&d, &at, &[0x13a, 0x13d, 0x140]).expect("found");
        assert_eq!(hem_form(s.form), Some("teardrop"));
        assert_eq!(s.rule_radius, 0.25);
        assert!(hem_section(&d, &at, &[0x13a, 0x999]).is_err());
        assert!(hem_section(&d, &at, &[0x13a]).is_err());
    }

    /// An Unfold's slots (mitcad#2117): the first face slot holds the
    /// bends' faces, the second the one stationary face, the body slots
    /// the body (each once); other layouts are refused.
    #[test]
    fn an_unfolds_slots_give_its_bends_stationary_face_and_body() {
        let slots = vec![
            (16, vec![11, 12, 13, 14]),
            (16, vec![21]),
            (4, vec![31]),
            (8, vec![31]),
        ];
        assert_eq!(
            unfold_slots(&slots),
            Ok(UnfoldSlots {
                bends: vec![11, 12, 13, 14],
                stationary: 21,
                bodies: vec![31],
            })
        );
        // Two stationary faces, a slot missing, no bend.
        assert!(unfold_slots(&[(16, vec![11]), (16, vec![21, 22])]).is_err());
        assert!(unfold_slots(&[(16, vec![11]), (4, vec![31])]).is_err());
        assert!(unfold_slots(&[(16, vec![]), (16, vec![21])]).is_err());
        // A Refold's body slot.
        assert_eq!(unfold_bodies(&[(8, vec![31])]), [31]);
        assert!(unfold_bodies(&[(16, vec![11])]).is_empty());
    }
}
