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
//!   taken from their default expressions, not from variants)*.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use super::super::decode::{ParameterEntry, TimelineEntry};
use super::super::ir::*;
use super::super::stream::{f64_at, str16_at, u32_at};
use super::{Builder, edge_input, paramref};

/// The flanges' class (base and edge flanges, hems).
pub const FLANGE: &str = "7AFA0A65-B53C-472C-ACA4-5C7C0A65001C";

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
    // value, before the material's name.
    if let Some((_, _, end)) = texts.iter().find(|(s, _, _)| s.contains("* 4")) {
        r.k_factor = f64_at(d, end + 8 + 4).filter(|k| (0.0..=1.0).contains(k));
    }
    r.name = texts
        .iter()
        .rev()
        .find(|(s, _, _)| s.contains('('))
        .map(|(s, _, _)| s.clone());
    r
}

impl Builder<'_> {
    /// The design's sheet-metal rule.
    pub(super) fn sheet_rule(&self) -> Option<Rule> {
        let o = self.seg.objects_of(RULE).next()?;
        let d = self.seg.data(o);
        let mut r = rule(d);
        r.thickness = self
            .seg
            .ref_ids(d)
            .into_iter()
            .find(|id| self.dec.parameters.iter().any(|p| p.id == *id));
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
            Err("a hem: not decoded".to_owned())
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
}
