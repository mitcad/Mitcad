// SPDX-License-Identifier: MIT
//! Sheet-metal items (mitcad#211): the decoded sheet items of the dump
//! (`SCHEMA.md`, *Sheet metal*) as Mitcad's native sheet features
//! (`commands.md`, *Sheet rules and base Face*, *Folded sheet features*).
//!
//! A sheet item's detail is `{"sheet": {"kind": ..., ...}}`, written by the
//! `.ipt` and `.f3d` decoders alike. A face brings the rules its body
//! follows: the first face of a rule makes a `sheet_rules` feature (named
//! after the file's rule) with the face, in the face's item; later faces
//! of the same rule use it. Inputs the decoder could not read are listed in
//! the rules' `inferred`, and make the candidate `inferred` with a note
//! naming them.

use mitcad_dump::{Reference, TimelineItem};
use mitcad_model::FeatureUid;
use mitcad_model::features::FeatureDef;
use mitcad_model::sheet::SheetReferenceSide;
use mitcad_vec3::{add, cross, dot, norm, sub, unit_div};
use serde_json::{Map, Value, json};

use mitcad_import_replay::report::InputProvenance;
use mitcad_import_replay::{Candidate, ImportKernel, Importer};

/// A reference of the sheet item's JSON (a parameter, a profile).
fn reference(v: &Value) -> Option<Reference> {
    v.as_object().map(|m| Reference::from_map(m.clone()))
}

fn vector(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([
        a.first()?.as_f64()?,
        a.get(1)?.as_f64()?,
        a.get(2)?.as_f64()?,
    ])
}

/// The name of the rules feature made for a rule of the file.
fn rules_name(rules: &Map<String, Value>) -> String {
    match rules.get("name").and_then(Value::as_str) {
        Some(n) if !n.trim().is_empty() => format!("Sheet rules {n}"),
        _ => "Sheet rules".to_owned(),
    }
}

/// The sheet-metal family of the replay (mitcad#211, mitcad#785): sheet
/// items as Mitcad's native sheet features. The core reaches it through
/// `Families::sheet_item` and `Families::dressups_on_sheets`.
pub trait Sheets<K: ImportKernel> {
    /// The candidates of a sheet item.
    fn sheet_item(
        &mut self,
        index: i64,
        item: &TimelineItem,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String>;

    /// A rule's value: a parameter reference (its imported parameter or
    /// value, mm), or a number given as `{"value": v}` (unitless).
    fn sheet_value(&self, v: &Value, what: &str) -> Result<Value, String>;

    /// The `sheet_rules` definition of a decoded rule, with the reference
    /// side, and the inputs the decoder did not read.
    fn sheet_rules_def(
        &self,
        rules: &Map<String, Value>,
        side: &str,
    ) -> Result<(Value, Vec<String>), String>;

    /// An earlier rules feature made for this rule of the file with this
    /// reference side (the file's faces of one rule share it; its values
    /// are the rule's parameters).
    fn sheet_rules_made(&self, name: &str, side: SheetReferenceSide) -> Option<FeatureUid>;

    /// A sheet-metal face: the regions of its profile in its sketch as a
    /// `sheet_face`, its material on the side of the sketch's plane the
    /// decoded direction points to (the rules' reference side: `front`
    /// along the Mitcad sketch's normal, `back` against it).
    fn sheet_face(
        &mut self,
        index: i64,
        item: &TimelineItem,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String>;

    /// A face whose regions make new sheet bodies (`sheet_face` without a
    /// body), with the rules of its rule and side.
    fn sheet_new_face(
        &mut self,
        index: i64,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String>;

    /// A sheet-metal cut (mitcad#211, mitcad#242): a cut of sketch profiles
    /// straight along the sketch's normal, as the file's cut with neither
    /// option set goes (through a bend too), as a `sheet_cut` of mode
    /// `straight` with the file's extent (a distance or through all) and
    /// direction, when it goes into the material at least the sheet's
    /// thickness. A cut of part of the thickness has no sheet counterpart
    /// and is not translated.
    fn sheet_cut(
        &mut self,
        index: i64,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String>;

    /// A flange whose edge, position and length datum the decoder read
    /// (the `.f3d` edge flange): the edge found in the replay by its
    /// geometry is a boundary of a panel (`sheet_reference`); the flange
    /// turns towards the side of the face the edge was selected on (a back
    /// face's edge: a positive angle, towards the material's front-to-back
    /// normal).
    fn sheet_edge_flange(
        &mut self,
        sheet: &Map<String, Value>,
        edges: &[Map<String, Value>],
        count: usize,
    ) -> Result<Vec<Candidate>, String>;

    /// A face joined to the sheet (the `.ipt` face of operation join,
    /// mitcad#242). Its material is its sketch's slab, along the decoded
    /// direction (against it for side 1). Where that slab is a panel's of
    /// the sheet (the sketch on the panel's front or back surface, the
    /// material going into it), the face is joined in that plane: a
    /// `sheet_face` joined to the sheet body, which adds its regions to the
    /// panel it touches (`read`) *(verified: the two joined faces the
    /// corpora reach with a valid sheet, which #211 took for double bends
    /// because it did not read the side)*. A face at an angle to the panels
    /// is joined by a bend between edges the file names by B-rep naming
    /// ids (not decoded yet): it falls back with that reason.
    fn sheet_joined_face(
        &mut self,
        index: i64,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String>;

    /// A corner round or chamfer (the `.ipt` sheet-metal corners): its
    /// edges, read by their names (mitcad#242), found in the replay as a
    /// fillet's or chamfer's given edges are (an item without them: from
    /// the history state, as before), each the thickness edge where
    /// two walls of a panel meet, which `sheet_reference` names as the
    /// corner of the panel's contour; then `sheet_corner_round` with the
    /// radius, or `sheet_corner_chamfer` with the distances (first along
    /// the incoming boundary; a two-distance chamfer either way round).
    fn sheet_corner(
        &mut self,
        kind: &str,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String>;

    /// A rounding's or chamfer's candidate as sheet corner rounds or
    /// chamfers: where every edge is the thickness edge at a straight corner
    /// of a valid sheet's panel (`sheet_reference`), the same geometry as
    /// `sheet_corner_round` / `sheet_corner_chamfer` of those corners.
    fn as_sheet_corners(&self, c: &Candidate, round: bool) -> Option<Candidate>;

    /// An ordinary rounding's or chamfer's candidates on a sheet, each tried
    /// first as the sheet corners it rounds or bevels, where all its edges
    /// are such corners: the same geometry, and the body stays a sheet for
    /// the sheet items after it (mitcad#211).
    fn dressups_on_sheets(&self, candidates: Vec<Candidate>, round: bool) -> Vec<Candidate>;

    /// Why a body is not a valid sheet, naming the timeline item that took
    /// its sheet meaning away (the sheet state's `invalidated` or first
    /// blocker): the note of a sheet item that falls back there.
    fn lost_sheet(&self, body: &str, state: &Value) -> String;

    /// A bend between two faces of the sheet (the `.ipt` bend, mitcad#242):
    /// its two edges read from the file by their names, found in the
    /// replay, each a straight boundary of a panel. Mitcad's `sheet_bend`
    /// joins two separate sheet bodies by their already placed ports
    /// (`tangent_ports`), or opens a shared coplanar seam of one body
    /// (`virtual_sharp`); a bend between two panels of one body that meet
    /// at no seam closes a loop of material, which the model does not
    /// make.
    fn sheet_bend_item(&mut self, sheet: &Map<String, Value>) -> Result<Vec<Candidate>, String>;

    /// A bend between edges of parallel panels of two sheet bodies (the
    /// `.ipt` bend whose bend record's kinds are 3 and 5, mitcad#280): two
    /// opposite 45-degree bends and a straight between them, Mitcad's
    /// `sheet_bend` of port mode `double`. The fixed edge (whose panel stays
    /// whole) is the one pointing along the bend record's axis × direction
    /// away from its panel's material; the direction must point from its
    /// panel's plane to the other's. The body the file's bend works on keeps
    /// its id; the other is consumed.
    fn sheet_double_bend(
        &mut self,
        sheet: &Map<String, Value>,
        edges: &[Map<String, Value>],
        found: &[(String, Value)],
    ) -> Result<Vec<Candidate>, String>;

    /// A fold (the `.ipt` fold, mitcad#242): its angle, radius and line are
    /// read; which side stays and which way it folds are not decoded (see
    /// `core/ipt/README.md`, *Sheet metal*), so it is not translated.
    fn sheet_fold_item(&mut self, sheet: &Map<String, Value>) -> Result<Vec<Candidate>, String>;

    /// The thickness of a sheet's rules.
    fn sheet_thickness(&self, state: &Value) -> Option<f64>;

    /// A flange's offsets (`{start, end}`) from a width extent of the
    /// `.ipt` (mitcad#242: `from`, `to` the ends of the edge the offsets
    /// `offset_from`, `offset_to` are measured from, cm): the boundary's
    /// axial start is its chart edge's end where the panel's front-to-back
    /// sign is positive, else its start (as the model's port frame).
    fn sheet_offsets(
        &self,
        state: &Value,
        b: &Value,
        x: &Value,
    ) -> Result<(&'static str, Value), String>;

    /// A sheet body's state at the marker, as the `sheet_state` query
    /// answers it (mitcad#778: asked of the model, without the api).
    fn sheet_state(&self, body: &str) -> Result<Value, String>;

    /// Every sheet body's state at the marker (the `sheet_state` query
    /// without a body).
    fn sheet_states(&self) -> Result<Value, String>;

    /// A sheet-metal rules feature's rules at the marker, as the
    /// `sheet_rules` query answers them.
    fn sheet_rules(&self, feature: &str) -> Result<Value, String>;

    /// The semantic reference of a sheet body's edge (`at`: a point on
    /// it, mm), as the `sheet_reference` query answers it.
    fn sheet_reference(
        &self,
        body: &str,
        edge: &str,
        at: Option<[f64; 3]>,
    ) -> Result<Value, String>;

    /// The sheet body a sheet item works on: its decoded body, else the
    /// document's only valid sheet body.
    fn sheet_body(&self, sheet: &Map<String, Value>) -> Result<String, String>;

    /// A sheet-metal flange on one edge (mitcad#211). The decoder gives the
    /// flange's stored path across the middle of the edge (from the
    /// stationary tangent line on the inner face through the bend to the
    /// flange's end), the point its length is measured from, and its angle,
    /// length and bend radius parameters. Here:
    ///
    /// - the panel is the one whose inner or outer surface holds the path's
    ///   start, and the boundary is the panel's straight free boundary the
    ///   path's start direction meets first;
    /// - the bend goes towards the side of the surface the path starts on
    ///   (the back surface: a positive angle, towards the material's
    ///   front-to-back normal; the front: a negative one);
    /// - the position is the one whose distance from the boundary to the
    ///   tangent line (`inside`: the outer setback, `outside`: the inner
    ///   setback, `bend_line`: half the allowance) is the path start's;
    /// - the length's mode is `outer` or `inner` where its datum is the
    ///   outer or inner virtual sharp of the bend, `tangent` on the tangent
    ///   line.
    ///
    /// Each must match exactly one choice within 1e-3 mm, or the flange is
    /// not translated. The flange's edges (one or several) are those the
    /// decoder read by their names (mitcad#242): one `sheet_flange` per
    /// edge with those inputs, the path's boundary among them.
    fn sheet_flange(&mut self, sheet: &Map<String, Value>) -> Result<Vec<Candidate>, String>;

    /// A hem (the `.f3d` hem, mitcad#1364, mitcad#1378): the flange
    /// `hem_as_flange` makes of it, on its edge as an edge flange's (a
    /// rolled hem a flange past 180 degrees with no wall; a teardrop, rope
    /// or double hem the flange's hem of that form).
    fn sheet_hem(&mut self, sheet: &Map<String, Value>) -> Result<Vec<Candidate>, String>;
}

impl<K: ImportKernel> Sheets<K> for Importer<'_, K> {
    fn sheet_item(
        &mut self,
        index: i64,
        item: &TimelineItem,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String> {
        match sheet.get("kind").and_then(Value::as_str) {
            Some("face") => self.sheet_face(index, item, sheet),
            Some("flange") => self.sheet_flange(sheet),
            Some("cut") => self.sheet_cut(index, sheet),
            Some(kind @ ("corner_round" | "corner_chamfer")) => self.sheet_corner(kind, sheet),
            Some("bend") => self.sheet_bend_item(sheet),
            Some("fold") => self.sheet_fold_item(sheet),
            Some("hem") => self.sheet_hem(sheet),
            Some(kind) => Err(format!("a sheet-metal {kind}: not translated yet")),
            None => Err("a sheet-metal item of no kind".to_owned()),
        }
    }

    fn sheet_value(&self, v: &Value, what: &str) -> Result<Value, String> {
        // A rule's expression of the file's parameters (the `.f3d` rule's
        // `Thickness * 0.5` as `d7 * 0.5`): in the imported parameters'
        // names, when every name in it was imported.
        if v.get("name").is_none()
            && let Some(e) = v.get("expression").and_then(Value::as_str)
        {
            let mut out = String::new();
            let mut word = String::new();
            let mut ok = true;
            let flush = |word: &mut String, out: &mut String, ok: &mut bool| {
                if word.is_empty() {
                    return;
                }
                if word.starts_with(|c: char| c.is_ascii_digit()) {
                    out.push_str(word);
                } else if let Some(name) = self.params.get(word.as_str()) {
                    out.push_str(name);
                } else {
                    *ok = false;
                }
                word.clear();
            };
            for c in e.chars() {
                if c.is_alphanumeric() || c == '_' || c == '.' {
                    word.push(c);
                } else {
                    flush(&mut word, &mut out, &mut ok);
                    out.push(c);
                }
            }
            flush(&mut word, &mut out, &mut ok);
            if ok {
                return Ok(json!(out));
            }
        }
        if let Some(r) = reference(v)
            && let Some(value) = self.value(&Some(r))
        {
            return Ok(value);
        }
        v.get("value")
            .and_then(Value::as_f64)
            .map(|x| json!(x))
            .ok_or_else(|| format!("its rule's {what} was not decoded"))
    }

    fn sheet_rules_def(
        &self,
        rules: &Map<String, Value>,
        side: &str,
    ) -> Result<(Value, Vec<String>), String> {
        let get = |k: &str| rules.get(k).unwrap_or(&Value::Null);
        let relief = |k: &str| -> Result<Value, String> {
            let r = get(k);
            Ok(json!({
                "type": r["type"].as_str().ok_or_else(|| format!("no {k} type"))?,
                "width": self.sheet_value(&r["width"], &format!("{k} width"))?,
                "depth": self.sheet_value(&r["depth"], &format!("{k} depth"))?,
            }))
        };
        let def = json!({
            "type": "sheet_rules",
            "thickness": self.sheet_value(get("thickness"), "thickness")?,
            "bend_radius": self.sheet_value(get("bend_radius"), "bend radius")?,
            "allowance": {"type": "k_factor", "k": self.sheet_value(get("k_factor"), "K-factor")?},
            "reference_side": side,
            "bend_relief": relief("bend_relief")?,
            "corner_relief": relief("corner_relief")?,
        });
        let inferred = get("inferred")
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Ok((def, inferred))
    }

    fn sheet_rules_made(&self, name: &str, side: SheetReferenceSide) -> Option<FeatureUid> {
        self.doc
            .features()
            .take(self.doc.marker())
            .filter(|f| !f.suppressed && f.name == name)
            .find(|f| matches!(&f.def, FeatureDef::SheetRules(d) if d.reference_side == side))
            .map(|f| f.uid)
    }

    fn sheet_face(
        &mut self,
        index: i64,
        _item: &TimelineItem,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String> {
        match sheet.get("operation").and_then(Value::as_str) {
            Some("NewBodyFeatureOperation") => self.sheet_new_face(index, sheet),
            Some("JoinFeatureOperation") => self.sheet_joined_face(index, sheet),
            other => Err(format!("a sheet-metal face of operation {other:?}")),
        }
    }

    fn sheet_new_face(
        &mut self,
        index: i64,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String> {
        let profiles: Vec<Reference> = sheet
            .get("profile")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(reference).collect())
            .unwrap_or_default();
        let (sketch, last_sketch) = self.profile_sketch(&profiles, index)?;
        // The regions the decoded loops select (`.f3d`), else those of the
        // measured profiles (`.ipt`).
        let sets = match self.mapped_set(sheet, sketch) {
            Some(set) => vec![set],
            None => self.region_sets(&profiles, sketch, None, None)?,
        };
        let info = &self.sketches[&sketch];
        let frame = self
            .doc
            .sketch_output(info.uid)
            .map(|o| o.frame)
            .ok_or("its sketch did not evaluate")?;
        let sketch_uid = info.uid.to_string();
        // The material along the file's sketch normal (the `.f3d` base
        // flange), or along a decoded direction.
        let direction = if sheet.get("along_sketch_normal") == Some(&json!(true)) {
            info.frame.normal()
        } else {
            material_direction(sheet)?
        };
        let along = dot(direction, frame.normal());
        if along.abs() < 1e-6 {
            return Err("its material direction lies in the sketch's plane".to_owned());
        }
        let side = if along > 0.0 { "front" } else { "back" };
        let rules = sheet
            .get("rules")
            .and_then(Value::as_object)
            .ok_or("its rules were not decoded")?;
        let (rules_def, inferred) = self.sheet_rules_def(rules, side)?;
        let name = rules_name(rules);
        let existing = self.sheet_rules_made(
            &name,
            if side == "front" {
                SheetReferenceSide::Front
            } else {
                SheetReferenceSide::Back
            },
        );
        let mut candidates = Vec::new();
        for set in &sets {
            let regions: Vec<Value> = set
                .keys
                .iter()
                .map(|r| json!({"sketch": sketch_uid, "region": r}))
                .collect();
            let mut defs = Vec::new();
            let rules_ref = match existing {
                Some(uid) => json!(uid.to_string()),
                None => {
                    let mut def = rules_def.clone();
                    def["_import_name"] = json!(name);
                    defs.push(def);
                    json!("$0")
                }
            };
            defs.push(json!({"type": "sheet_face", "rules": rules_ref, "profiles": regions}));
            let mut c = Candidate::new(defs.remove(0));
            c.defs.extend(defs);
            c.guess = set.guess;
            let read = !set.guess && !last_sketch && inferred.is_empty();
            c.input_provenance = if read {
                InputProvenance::Read
            } else {
                InputProvenance::Inferred
            };
            if !inferred.is_empty() && existing.is_none() {
                c.note = Some(format!(
                    "rules' inputs not read from the file: {}",
                    inferred.join("; ")
                ));
            }
            candidates.push(c);
        }
        Ok(candidates)
    }

    fn sheet_cut(
        &mut self,
        index: i64,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String> {
        if sheet.get("operation").and_then(Value::as_str) != Some("CutFeatureOperation") {
            return Err("a sheet-metal cut that does not cut".to_owned());
        }
        let profiles: Vec<Reference> = sheet
            .get("profile")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(reference).collect())
            .unwrap_or_default();
        let (sketch, last_sketch) = self.profile_sketch(&profiles, index)?;
        let body = self.sheet_body(sheet)?;
        let state = self.sheet_state(&body)?;
        if state["status"] != "valid" {
            return Err(self.lost_sheet(&body, &state));
        }
        let rules_uid = state["rules"].as_str().ok_or("its sheet has no rules")?;
        let rules = self.sheet_rules(rules_uid)?;
        let t = rules["rules"]["thickness"]
            .as_f64()
            .ok_or("its rules have no thickness")?;
        // Its two options beyond an extrusion's: slot 31, cut across the
        // bends (the profile cut from the developed sheet, mitcad#280
        // *(verified: the one such cut of the corpora gives its state)*),
        // and slot 33 (cut normal to the face by the defaults' order; true
        // in no cut of the corpora, not decoded). A cut with neither is an
        // extrusion's straight cut.
        let option = |k: usize| sheet["options"][k] == json!(true);
        if option(1) {
            return Err(
                "a sheet-metal cut with its slot-33 option set (normal to the face): not \
                 decoded"
                    .to_owned(),
            );
        }
        let across = option(0);
        let extent = sheet.get("extentOne").cloned().unwrap_or(Value::Null);
        let extent = match extent["_type"].as_str() {
            Some("ThroughAllExtentDefinition") => json!({"type": "through_all"}),
            Some("DistanceExtentDefinition") => {
                let d = reference(&extent["distance"])
                    .as_ref()
                    .and_then(Reference::parameter)
                    .and_then(|p| p.value)
                    .ok_or("its distance was not decoded")?;
                if d.abs() * 10.0 < t - 1e-6 {
                    return Err(format!(
                        "a cut {:.4} mm deep into a sheet {t:.4} mm thick: not a cut through \
                         the sheet",
                        d.abs() * 10.0
                    ));
                }
                let value = self
                    .value(&reference(&extent["distance"]))
                    .ok_or("its distance was not decoded")?;
                json!({"type": "distance", "distance": match (d < 0.0, value) {
                    (true, Value::String(name)) => json!(format!("-({name})")),
                    (true, v) => json!(-v.as_f64().unwrap_or(0.0)),
                    (false, v) => v,
                }})
            }
            other => return Err(format!("a sheet-metal cut with the extent {other:?}")),
        };
        if sheet.get("extentType").and_then(Value::as_str) != Some("OneSideFeatureExtentType") {
            return Err("a sheet-metal cut to two sides".to_owned());
        }
        let info = &self.sketches[&sketch];
        let frame = self
            .doc
            .sketch_output(info.uid)
            .map(|o| o.frame)
            .ok_or("its sketch did not evaluate")?;
        let sketch_uid = info.uid.to_string();
        let direction = sheet
            .get("direction")
            .and_then(vector)
            .and_then(unit_div)
            .ok_or("its direction was not decoded")?;
        let along = dot(direction, frame.normal());
        if along.abs() < 1.0 - 1e-6 {
            return Err("its direction is not normal to its sketch".to_owned());
        }
        // Across the bends: cut from the developed sheet, every bend opened
        // while the panel whose surface holds the sketch stays.
        let developed = if across {
            let mut panels = Vec::new();
            for chart in state["charts"].as_array().into_iter().flatten() {
                if chart["domain"]["kind"] != "panel" {
                    continue;
                }
                let f = &chart["embedding"]["front_frame"];
                let (Some(o), Some(x), Some(y)) = (
                    vector(&f["origin"]),
                    vector(&f["x_axis"]),
                    vector(&f["y_axis"]),
                ) else {
                    continue;
                };
                let n = unit_div(cross(x, y)).ok_or("a panel without a plane")?;
                let q = dot(sub(frame.origin, o), n).abs();
                if dot(n, frame.normal()).abs() > 1.0 - 1e-9 && (q <= 1e-4 || (q - t).abs() <= 1e-4)
                {
                    panels.push(chart["domain"]["id"].clone());
                }
            }
            let [panel] = panels.as_slice() else {
                return Err(format!(
                    "a cut across the bends whose sketch lies on {} panels' surfaces (one is \
                     needed)",
                    panels.len()
                ));
            };
            let bends: Vec<Value> = state["bends"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|b| b["id"].clone())
                .collect();
            Some((panel.clone(), bends))
        } else {
            None
        };
        let sets = self.region_sets(&profiles, sketch, None, None)?;
        let mut candidates = Vec::new();
        for set in &sets {
            let regions: Vec<Value> = set
                .keys
                .iter()
                .map(|r| json!({"sketch": sketch_uid, "region": r}))
                .collect();
            let def = match &developed {
                Some((panel, bends)) => json!({
                    "type": "sheet_cut", "body": body, "stationary": panel,
                    "open_bends": bends, "profiles": regions,
                }),
                None => json!({
                    "type": "sheet_cut", "body": body, "mode": "straight",
                    "extent": extent, "flip": along < 0.0, "profiles": regions,
                }),
            };
            let mut c = Candidate::new(def);
            c.guess = set.guess;
            c.input_provenance = if c.guess || last_sketch {
                InputProvenance::Inferred
            } else {
                InputProvenance::Read
            };
            candidates.push(c);
        }
        Ok(candidates)
    }

    fn sheet_edge_flange(
        &mut self,
        sheet: &Map<String, Value>,
        edges: &[Map<String, Value>],
        count: usize,
    ) -> Result<Vec<Candidate>, String> {
        let position = match sheet.get("position").and_then(Value::as_str) {
            Some(p @ ("inside" | "outside")) => p,
            // The bend tangent to the side face: its outer surface touches
            // the side plane through the edge (mitcad#242) *(verified: the
            // hems and the 90-degree flanges of both designs give their
            // states)*.
            Some("tangent") => "side_tangent",
            Some(other) => {
                return Err(format!(
                    "bend position {other}: Mitcad's flange has no such position (inside, \
                     outside, bend line, side tangent)"
                ));
            }
            None => return Err("no bend position".to_owned()),
        };
        // Height datum 3: from the plane tangent to the bend's outer
        // surface, normal to the wall (mitcad#242) *(verified as above)*.
        let mode = match sheet.get("length_mode").and_then(Value::as_str) {
            Some(m @ ("inner" | "outer")) => m,
            Some("tangent") => "outer_tangent",
            // The wall's straight length from the bend's tangent line (a
            // rolled hem's 0, mitcad#1378).
            Some("straight") => "tangent",
            other => return Err(format!("length datum {other:?} is not known")),
        };
        let widths: Vec<(String, Value)> = sheet
            .get("widths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|w| {
                Some((
                    w.get("role")?.as_str()?.to_owned(),
                    w.get("parameter")?.clone(),
                ))
            })
            .collect();
        // Each edge in the replay (an edge may come twice in the inputs):
        // its body, name, boundary and the side its face is on.
        let mut found: Vec<FoundEdge> = Vec::new();
        for edge in edges {
            let fp = mitcad_dump::Fingerprint::from_map(edge.clone());
            let mid = fp
                .mid_point
                .ok_or("its edge was not found in the file's history")?;
            let (body, name) = mitcad_import_replay::internal::refs::resolve_edge(self.doc, &fp)
                .ok_or("its edge was not found")?;
            let (body, name) = (body.to_string(), name.to_string());
            if found.iter().any(|f| f.0 == body && f.1 == name) {
                continue;
            }
            let answer = self.sheet_reference(&body, &name, Some(mid.map(|x| x * 10.0)))?;
            let boundary = answer
                .get("boundary")
                .filter(|b| b.is_object())
                .cloned()
                .ok_or_else(|| format!("its edge {name} is not a panel's boundary"))?;
            if answer["straight"] != true {
                return Err("its edge is not straight".to_owned());
            }
            let on_back = name.contains("sheet_back(");
            if !on_back && !name.contains("sheet_front(") {
                return Err(format!("its edge {name} is not on a panel's surface"));
            }
            // Whether the file's edge runs along the boundary's axial
            // direction (where Mitcad's flange offsets start), for the
            // widths measured from the edge's ends.
            let along = match (
                fp.start_point,
                fp.end_point,
                answer["axial"]["start"].as_array(),
                answer["axial"]["end"].as_array(),
            ) {
                (Some(Some(s)), Some(Some(e)), Some(a), Some(b))
                    if a.len() == 3 && b.len() == 3 =>
                {
                    let axis: Vec<f64> = (0..3)
                        .map(|i| b[i].as_f64().unwrap_or(0.0) - a[i].as_f64().unwrap_or(0.0))
                        .collect();
                    Some((0..3).map(|i| (e[i] - s[i]) * axis[i]).sum::<f64>() > 0.0)
                }
                _ => None,
            };
            found.push((body, name, boundary, on_back, along));
        }
        if found.len() != count {
            return Err(format!(
                "{} of its {count} edges found in the replay",
                found.len()
            ));
        }
        if found.iter().any(|f| f.0 != found[0].0) {
            return Err("its edges are on several bodies".to_owned());
        }
        let signed = |key: &str, negative: bool| -> Result<Value, String> {
            let v = sheet.get(key).ok_or_else(|| format!("no {key}"))?;
            let v = match (v.get("model"), self.value(&reference(v))) {
                // A value the import made in the model's terms (a hem's
                // angle and radius, mitcad#1364).
                (Some(m), _) => m.clone(),
                (None, Some(v)) => v,
                (None, None) => json!(
                    v.get("value")
                        .and_then(Value::as_f64)
                        .ok_or_else(|| format!("no {key}"))?
                        * 10.0
                ),
            };
            Ok(match (negative, &v) {
                (false, _) => v,
                (true, Value::String(name)) => json!(format!("-{name}")),
                (true, v) => json!(-v.as_f64().unwrap_or(0.0)),
            })
        };
        let width_value = |p: &Value| -> Result<Value, String> {
            match self.value(&reference(p)) {
                Some(v) => Ok(v),
                None => p
                    .get("value")
                    .and_then(Value::as_f64)
                    .map(|v| json!(v * 10.0))
                    .ok_or_else(|| "a width was not decoded".to_owned()),
            }
        };
        let role = |name: &str| widths.iter().find(|(r, _)| r == name).map(|(_, p)| p);
        let mut roles: Vec<&str> = widths.iter().map(|(r, _)| r.as_str()).collect();
        roles.sort_unstable();
        // One flange per edge, in the file's order; flipped, a flange turns
        // to the other side.
        let flip = sheet.get("flip") == Some(&json!(true));
        let mut defs = Vec::new();
        for (body, _, boundary, on_back, along) in &found {
            let back = *on_back != flip;
            let mut def = json!({
                "type": "sheet_flange",
                "body": body,
                "boundary": boundary,
                "angle": signed("angle", !back)?,
                "length": {"mode": mode, "value": signed("length", false)?},
                "position": position,
                "radius": signed("radius", false)?,
            });
            // A hem whose bends follow from its sizes (mitcad#1378).
            if let Some(hem) = sheet.get("hem_model") {
                def["hem"] = hem.clone();
            }
            // Its width along the edge (mitcad#242), measured from the
            // edge's midpoint: the same distance both ways (`EdgeWidth`),
            // or one towards the file's edge's start point and one towards
            // its end (`EdgeWidth_1`, `EdgeWidth_2`) *(verified: the partial
            // flanges of both designs, one wider than its edge, give their
            // states)*.
            match roles.as_slice() {
                [] => {}
                ["EdgeWidth"] => {
                    let w = width_value(role("EdgeWidth").expect("listed"))?;
                    def["width"] = json!({"type": "middle", "before": w, "after": w});
                }
                ["EdgeWidth_1", "EdgeWidth_2"] => {
                    let first = width_value(role("EdgeWidth_1").expect("listed"))?;
                    let second = width_value(role("EdgeWidth_2").expect("listed"))?;
                    let Some(along) = along else {
                        return Err("its edge's direction was not found".to_owned());
                    };
                    def["width"] = if *along {
                        json!({"type": "middle", "before": first, "after": second})
                    } else {
                        json!({"type": "middle", "before": second, "after": first})
                    };
                }
                other => {
                    return Err(format!(
                        "a flange along part of its edge with the widths {other:?}: not seen, \
                         not translated"
                    ));
                }
            }
            defs.push(def);
        }
        let mut c = Candidate::new(defs.remove(0));
        c.defs.extend(defs);
        c.input_provenance = InputProvenance::Read;
        Ok(vec![c])
    }

    fn sheet_joined_face(
        &mut self,
        index: i64,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String> {
        let profiles: Vec<Reference> = sheet
            .get("profile")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(reference).collect())
            .unwrap_or_default();
        let (sketch, last_sketch) = self.profile_sketch(&profiles, index)?;
        let body = self.sheet_body(sheet)?;
        let state = self.sheet_state(&body)?;
        if state["status"] != "valid" {
            return Err(self.lost_sheet(&body, &state));
        }
        let rules_uid = state["rules"]
            .as_str()
            .ok_or("its sheet has no rules")?
            .to_owned();
        let rules = self.sheet_rules(&rules_uid)?;
        let t = rules["rules"]["thickness"]
            .as_f64()
            .ok_or("its rules have no thickness")?;
        let info = &self.sketches[&sketch];
        let frame = self
            .doc
            .sketch_output(info.uid)
            .map(|o| o.frame)
            .ok_or("its sketch did not evaluate")?;
        let sketch_uid = info.uid.to_string();
        // The face's material: from its sketch's plane along the decoded
        // direction (against it for side 1), as thick as the sheet.
        let direction = unit_div(material_direction(sheet)?).ok_or("no material direction")?;
        let along = dot(direction, frame.normal());
        if along.abs() < 1.0 - 1e-6 {
            return Err("its material direction is not normal to its sketch".to_owned());
        }
        // A panel whose material is the face's slab: the sketch on one of
        // its surfaces, the material going into it.
        let mut coplanar = false;
        for chart in state["charts"].as_array().into_iter().flatten() {
            if chart["domain"]["kind"] != "panel" || chart["embedding"]["type"] != "plane" {
                continue;
            }
            let f = &chart["embedding"]["front_frame"];
            let (Some(o), Some(x), Some(y)) = (
                vector(&f["origin"]),
                vector(&f["x_axis"]),
                vector(&f["y_axis"]),
            ) else {
                continue;
            };
            let sign = if chart["embedding"]["front_to_back_sign"] == "negative" {
                -1.0
            } else {
                1.0
            };
            let n = cross(x, y).map(|c| c * sign);
            let q = dot(sub(frame.origin, o), n);
            let into = dot(direction, n);
            if (q.abs() <= 1e-3 && into > 1.0 - 1e-6)
                || ((q - t).abs() <= 1e-3 && into < -1.0 + 1e-6)
            {
                coplanar = true;
            }
        }
        if !coplanar {
            // The joint is its bend part's list (slot 0, mitcad#280). An
            // empty list: the face is joined by no bend, and its regions,
            // lying in no panel's plane, touch no material of the sheet (the
            // file's body holds them as separate lumps): each is a new sheet
            // body with the sheet's rules, which the history state checks.
            let bend = sheet.get("bend").unwrap_or(&Value::Null);
            return match bend.get("listed").and_then(Value::as_u64) {
                Some(0) => {
                    let mut candidates = self.sheet_new_face(index, sheet)?;
                    for c in &mut candidates {
                        let note = "joined by no bend (its bend part lists no edge), its \
                                    regions apart from the sheet: new sheet bodies";
                        c.note = Some(match c.note.take() {
                            Some(n) => format!("{note}; {n}"),
                            None => note.to_owned(),
                        });
                    }
                    Ok(candidates)
                }
                // A face bent onto the sheet: its bend part lists items of
                // the face's own plate (class `3839a63b…`), not decoded; it
                // is not translated, and never by choosing among candidate
                // edge pairs.
                Some(n) => Err(format!(
                    "a face joined by a bend: its bend part lists {n} items of the face's own \
                     plate, not decoded yet"
                )),
                None => Err("a face joined by a bend: its bend part was not decoded".to_owned()),
            };
        }
        let sets = match self.mapped_set(sheet, sketch) {
            Some(set) => vec![set],
            None => self.region_sets(&profiles, sketch, None, None)?,
        };
        let mut candidates = Vec::new();
        for set in &sets {
            let regions: Vec<Value> = set
                .keys
                .iter()
                .map(|r| json!({"sketch": sketch_uid, "region": r}))
                .collect();
            let mut c = Candidate::new(json!({
                "type": "sheet_face", "rules": rules_uid, "body": body, "profiles": regions,
            }));
            c.guess = set.guess;
            c.input_provenance = if set.guess || last_sketch {
                InputProvenance::Inferred
            } else {
                InputProvenance::Read
            };
            candidates.push(c);
        }
        Ok(candidates)
    }

    fn sheet_corner(
        &mut self,
        kind: &str,
        sheet: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String> {
        let dressup = sheet
            .get("dressup")
            .and_then(Value::as_object)
            .cloned()
            .ok_or("its edges and sizes were not decoded")?;
        let round = kind == "corner_round";
        // The `.ipt` corner's edges by their names (mitcad#242), in its
        // edge sets: every one read from the file, or it is not translated.
        let named = match sheet.get("edge_errors").and_then(Value::as_array) {
            Some(errors) if !errors.is_empty() => {
                let errors: Vec<&str> = errors.iter().filter_map(Value::as_str).collect();
                return Err(format!(
                    "its edges were not all found by their names in the file: {}",
                    errors.join("; ")
                ));
            }
            Some(_) => true,
            None => false,
        };
        // On a body that is no longer a valid sheet there is no sheet corner.
        if let Ok(body) = self.sheet_body(sheet) {
            let state = self.sheet_state(&body)?;
            if state["status"] != "valid" {
                return Err(self.lost_sheet(&body, &state));
            }
        }
        let dressups = if round {
            let d = mitcad_dump::FilletDetail::from_map(dressup);
            self.fillet(&d)?
        } else {
            let d = mitcad_dump::ChamferDetail::from_map(dressup);
            self.chamfer(&d)?
        };
        let mut out = Vec::new();
        for c in dressups {
            // (A body whose sheet meaning is gone gives no sheet corner: the
            // item falls back, its note naming what took the meaning away.)
            let lost = c
                .defs
                .iter()
                .filter_map(|d| d["body"].as_str())
                .find_map(|body| {
                    let s = self.sheet_state(body).ok()?;
                    (s["status"] != "valid").then(|| self.lost_sheet(body, &s))
                });
            if let Some(note) = lost {
                return Err(note);
            }
            if let Some(mut n) = self.as_sheet_corners(&c, round) {
                if named {
                    // Its edges read from the file, its sizes its
                    // parameters (a two-distance chamfer's other reading
                    // is a guess).
                    n.input_provenance = if c.guess {
                        InputProvenance::Inferred
                    } else {
                        InputProvenance::Read
                    };
                    n.note =
                        Some("its corners: edges read from the file by their names".to_owned());
                }
                out.push(n);
            }
        }
        if out.is_empty() {
            return Err("no corner of the sheet's panels among its edges".to_owned());
        }
        Ok(out)
    }

    fn as_sheet_corners(&self, c: &Candidate, round: bool) -> Option<Candidate> {
        let mut defs = Vec::new();
        for def in &c.defs {
            let body = def["body"].as_str()?;
            let state = self.sheet_state(body).ok()?;
            if state["status"] != "valid" {
                return None;
            }
            for set in def["sets"].as_array().into_iter().flatten() {
                let mut corners = Vec::new();
                for e in set["edges"].as_array().into_iter().flatten() {
                    let name = e.as_str()?;
                    let a = self.sheet_reference(body, name, None).ok()?;
                    if !a["corner"].is_object() || a["straight"] != true {
                        return None;
                    }
                    corners.push(a["corner"].clone());
                }
                if corners.is_empty() {
                    return None;
                }
                let size = &set["size"];
                if round {
                    if size["type"] != "constant" {
                        return None;
                    }
                    defs.push(json!({"type": "sheet_corner_round", "body": body,
                                     "corners": corners, "radius": size["radius"]}));
                } else {
                    let (first, second) = match size["type"].as_str() {
                        Some("equal_distance") => {
                            (size["distance"].clone(), size["distance"].clone())
                        }
                        Some("two_distances") => {
                            let (a, b) = (size["distance1"].clone(), size["distance2"].clone());
                            if set["flip"] == true { (b, a) } else { (a, b) }
                        }
                        _ => return None,
                    };
                    defs.push(json!({"type": "sheet_corner_chamfer", "body": body,
                        "corners": corners, "distances": {"first": first, "second": second}}));
                }
            }
        }
        if defs.is_empty() {
            return None;
        }
        let mut n = Candidate::new(defs.remove(0));
        n.defs.extend(defs);
        n.guess = c.guess;
        n.input_provenance = InputProvenance::Inferred;
        n.note = Some(
            "its corners: the edges the history state rounds or bevels (the file names them \
             by B-rep naming ids)"
                .to_owned(),
        );
        Some(n)
    }

    fn dressups_on_sheets(&self, candidates: Vec<Candidate>, round: bool) -> Vec<Candidate> {
        // (Nothing to do in a design without a valid sheet.)
        let any = self
            .sheet_states()
            .ok()
            .and_then(|s| {
                s.as_array()
                    .map(|a| a.iter().any(|s| s["status"] == "valid"))
            })
            .unwrap_or(false);
        if !any {
            return candidates;
        }
        let mut out = Vec::new();
        for c in candidates {
            if let Some(n) = self.as_sheet_corners(&c, round) {
                out.push(n);
            }
            out.push(c);
        }
        out
    }

    fn lost_sheet(&self, body: &str, state: &Value) -> String {
        let culprit = state["invalidated"]
            .as_object()
            .or_else(|| state["blockers"].as_array()?.first()?.as_object());
        let Some(c) = culprit else {
            return format!("its sheet body {body} is {}", state["status"]);
        };
        let feature = c.get("feature").and_then(Value::as_str).unwrap_or("?");
        let name = feature
            .parse::<FeatureUid>()
            .ok()
            .and_then(|uid| self.doc.feature(uid))
            .map_or_else(|| feature.to_owned(), |f| format!("{} ({feature})", f.name));
        format!(
            "its sheet body {body} is no longer a valid sheet: {name} took its sheet meaning \
             away ({})",
            c.get("message").and_then(Value::as_str).unwrap_or("")
        )
    }

    fn sheet_bend_item(&mut self, sheet: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let errors: Vec<&str> = sheet
            .get("edge_errors")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !errors.is_empty() {
            return Err(format!(
                "its edges were not all found by their names in the file: {}",
                errors.join("; ")
            ));
        }
        let edges: Vec<Map<String, Value>> = sheet
            .get("edge_list")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|e| e.as_object().cloned()).collect())
            .unwrap_or_default();
        if edges.len() != 2 {
            return Err(format!("a bend of {} edges read", edges.len()));
        }
        let mut found = Vec::new();
        for (k, e) in edges.iter().enumerate() {
            let fp = mitcad_dump::Fingerprint::from_map(e.clone());
            let mid = fp
                .mid_point
                .ok_or_else(|| format!("edge {}: no middle point", k + 1))?;
            let (body, name) = mitcad_import_replay::internal::refs::resolve_edge(self.doc, &fp)
                .ok_or_else(|| format!("edge {}: not found in the replay", k + 1))?;
            let (body, name) = (body.to_string(), name.to_string());
            let state = self.sheet_state(&body)?;
            if state["status"] != "valid" {
                return Err(self.lost_sheet(&body, &state));
            }
            let answer = self.sheet_reference(&body, &name, Some(mid.map(|x| x * 10.0)))?;
            let boundary = answer
                .get("boundary")
                .filter(|b| b.is_object())
                .cloned()
                .ok_or_else(|| format!("edge {}: {name} is not a panel's boundary", k + 1))?;
            found.push((body, boundary));
        }
        // The panels' material normals: parallel panels need two bends (a
        // jog), which one `sheet_bend` does not make.
        let normal = |body: &str, panel: &Value| -> Option<[f64; 3]> {
            let s = self.sheet_state(body).ok()?;
            let chart = s["charts"]
                .as_array()?
                .iter()
                .find(|c| c["domain"]["kind"] == "panel" && c["domain"]["id"] == *panel)?;
            let f = &chart["embedding"]["front_frame"];
            unit_div(cross(vector(&f["x_axis"])?, vector(&f["y_axis"])?))
        };
        let parallel = match (
            normal(&found[0].0, &found[0].1["panel"]),
            normal(&found[1].0, &found[1].1["panel"]),
        ) {
            (Some(a), Some(b)) => norm(cross(a, b)) < 1e-6,
            _ => false,
        };
        if parallel && found[0].0 != found[1].0 {
            return self.sheet_double_bend(sheet, &edges, &found);
        }
        if parallel {
            return Err(format!(
                "its two edges are boundaries of parallel panels ({} and {}): a double bend \
                 (two bends and a jog between them), which one sheet_bend does not make{}",
                found[0].1["panel"],
                found[1].1["panel"],
                if found[0].0 == found[1].0 {
                    "; the panels are of one sheet body, so the bend also closes a loop of \
                     material"
                } else {
                    ""
                }
            ));
        }
        if found[0].0 == found[1].0 {
            return Err(format!(
                "its two edges are boundaries of panels of one sheet body ({} and {}) that \
                 meet at no seam: the bend closes a loop of material, which Mitcad's sheet_bend \
                 does not make",
                found[0].1["panel"], found[1].1["panel"]
            ));
        }
        Err(
            "a bend between two sheet bodies: the bend's angle and sides are not decoded \
             (no such bend in the test files to read them from)"
                .to_owned(),
        )
    }

    fn sheet_double_bend(
        &mut self,
        sheet: &Map<String, Value>,
        edges: &[Map<String, Value>],
        found: &[(String, Value)],
    ) -> Result<Vec<Candidate>, String> {
        let record = sheet.get("bend_record").unwrap_or(&Value::Null);
        let kinds: Vec<Option<u64>> = record["kinds"]
            .as_array()
            .map(|a| a.iter().map(Value::as_u64).collect())
            .unwrap_or_default();
        if kinds != [Some(3), Some(5)] {
            return Err(format!(
                "its two edges are boundaries of parallel panels, and its bend record's kinds \
                 {kinds:?} are not the double bend's (3 and 5, two 45-degree bends): not decoded"
            ));
        }
        let (Some(axis), Some(direction)) = (vector(&record["axis"]), vector(&record["direction"]))
        else {
            return Err("its double bend's axis and direction were not read".to_owned());
        };
        let outward = unit_div(cross(axis, direction))
            .ok_or("its double bend's axis and direction are parallel")?;
        // Each edge's middle, and a point of its panel's material (the mean
        // of its chart's corners).
        let mut sides = Vec::new();
        for (k, (body, boundary)) in found.iter().enumerate() {
            let mid = edges[k]
                .get("mid_point")
                .and_then(vector)
                .map(|p| p.map(|x| x * 10.0))
                .ok_or_else(|| format!("edge {}: no middle point", k + 1))?;
            let state = self.sheet_state(body)?;
            let chart = state["charts"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|c| c["domain"]["kind"] == "panel" && c["domain"]["id"] == boundary["panel"])
                .ok_or_else(|| format!("edge {}: its panel was not found", k + 1))?;
            let f = &chart["embedding"]["front_frame"];
            let (Some(o), Some(x), Some(y)) = (
                vector(&f["origin"]),
                vector(&f["x_axis"]),
                vector(&f["y_axis"]),
            ) else {
                return Err(format!("edge {}: its panel is not planar", k + 1));
            };
            let corners: Vec<[f64; 2]> = chart["regions"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|r| r["loops"].as_array().into_iter().flatten().take(1))
                .flat_map(|l| l["edges"].as_array().into_iter().flatten())
                .filter_map(|e| vector2(&e["curve"]["geometry"]["start"]))
                .collect();
            if corners.is_empty() {
                return Err(format!("edge {}: its panel's outline was not read", k + 1));
            }
            let n = corners.len() as f64;
            let c = [
                corners.iter().map(|p| p[0]).sum::<f64>() / n,
                corners.iter().map(|p| p[1]).sum::<f64>() / n,
            ];
            let inside = add(o, add(x.map(|v| v * c[0]), y.map(|v| v * c[1])));
            sides.push((mid, dot(sub(inside, mid), outward) < 0.0));
        }
        let fixed = match (sides[0].1, sides[1].1) {
            (true, false) => 0,
            (false, true) => 1,
            _ => {
                return Err(
                    "its double bend's stored axis and direction point away from both or \
                     neither of its edges' panels"
                        .to_owned(),
                );
            }
        };
        if dot(sub(sides[1 - fixed].0, sides[fixed].0), direction) <= 0.0 {
            return Err(
                "its double bend's stored direction does not point from the fixed edge's panel \
                 to the other"
                    .to_owned(),
            );
        }
        // The body the file's bend works on keeps its id.
        let body = self.sheet_body(sheet)?;
        let stationary = found
            .iter()
            .position(|(b, _)| *b == body)
            .ok_or("neither of its edges is on the body it works on")?;
        let moving = 1 - stationary;
        let radius = self
            .value(&sheet.get("radius").and_then(reference))
            .ok_or("no bend radius")?;
        let def = json!({
            "type": "sheet_bend",
            "body": found[stationary].0,
            "moving_body": found[moving].0,
            "stationary": found[stationary].1,
            "moving": found[moving].1,
            "angle": std::f64::consts::FRAC_PI_4,
            "port_mode": "double",
            "fixed": if fixed == stationary { "stationary" } else { "moving" },
            "radius": radius,
        });
        let mut c = Candidate::new(def);
        c.input_provenance = InputProvenance::Read;
        c.note = Some(
            "a double bend: two 45-degree bends (its bend record's kinds 3 and 5), its edges read \
             from the file by their names, the fixed one by the stored axis and direction"
                .to_owned(),
        );
        Ok(vec![c])
    }

    fn sheet_fold_item(&mut self, sheet: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let body = self.sheet_body(sheet)?;
        let state = self.sheet_state(&body)?;
        if state["status"] != "valid" {
            return Err(self.lost_sheet(&body, &state));
        }
        // The decoded case (mitcad#280): enumeration 1, both booleans
        // false: the bend starts at the line, the material on the line's
        // right (seen along the sketch's normal) stays and the other side
        // turns against the sketch's normal.
        let flags: Vec<Option<bool>> = sheet
            .get("flags")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(Value::as_bool).collect())
            .unwrap_or_default();
        let kind = sheet.get("kind_enumeration").and_then(Value::as_u64);
        if kind != Some(1) || flags.get(..2) != Some(&[Some(false), Some(false)][..]) {
            return Err(format!(
                "a fold of enumeration {kind:?} and booleans {flags:?}: only 1 with both first \
                 booleans false is decoded"
            ));
        }
        let line = &sheet["line"];
        let (Some(point), Some(direction)) = (vector(&line["point"]), vector(&line["direction"]))
        else {
            return Err("its line was not read".to_owned());
        };
        let point = point.map(|x| x * 10.0);
        let direction = unit_div(direction).ok_or("its line has no direction")?;
        let index = sheet["sketch"]["timeline_index"]
            .as_i64()
            .ok_or("its sketch was not decoded")?;
        let info = self
            .sketches
            .get(&index)
            .ok_or_else(|| format!("its sketch (item {index}) was not imported"))?;
        let output = self
            .doc
            .sketch_output(info.uid)
            .ok_or("its sketch did not evaluate")?;
        let frame = output.frame;
        let sketch_uid = info.uid.to_string();
        // The sketch's line on the fold's line (both ends on it).
        let off = |p: [f64; 3]| {
            let d = sub(p, point);
            norm(sub(d, direction.map(|c| c * dot(d, direction))))
        };
        let on: Vec<_> = output
            .solved
            .curves
            .iter()
            .filter_map(|(id, c)| match c {
                mitcad_model::sketch::geometry::Curve2::Line { a, b } => {
                    let (p, q) = (frame.point(*a), frame.point(*b));
                    (off(p) <= 1e-4 && off(q) <= 1e-4 && norm(sub(q, p)) > 1e-4)
                        .then_some((*id, p, q))
                }
                _ => None,
            })
            .collect();
        let [(curve, p, q)] = on.as_slice() else {
            return Err(format!(
                "{} lines of its sketch lie on its line (one is needed)",
                on.len()
            ));
        };
        let middle = add(*p, sub(*q, *p).map(|c| c / 2.0));
        // The panel whose plane holds the line, its material around it.
        let n_sketch = frame.normal();
        let t = self
            .sheet_thickness(&state)
            .ok_or("its sheet has no thickness")?;
        let mut found = Vec::new();
        for chart in state["charts"].as_array().into_iter().flatten() {
            if chart["domain"]["kind"] != "panel" {
                continue;
            }
            let f = &chart["embedding"]["front_frame"];
            let (Some(o), Some(x), Some(y)) = (
                vector(&f["origin"]),
                vector(&f["x_axis"]),
                vector(&f["y_axis"]),
            ) else {
                continue;
            };
            let sign = if chart["embedding"]["front_to_back_sign"] == "negative" {
                -1.0
            } else {
                1.0
            };
            let n = cross(x, y).map(|c| c * sign);
            let depth = dot(sub(middle, o), n);
            if !(depth.abs() <= 1e-4 || (depth - t).abs() <= 1e-4) {
                continue;
            }
            let local = [dot(sub(middle, o), x), dot(sub(middle, o), y)];
            let outline: Vec<[f64; 2]> = chart["regions"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|r| r["loops"].as_array().into_iter().flatten().take(1))
                .flat_map(|l| l["edges"].as_array().into_iter().flatten())
                .filter_map(|e| vector2(&e["curve"]["geometry"]["start"]))
                .collect();
            if inside2(local, &outline) {
                found.push((chart["domain"]["id"].clone(), x, y, n));
            }
        }
        let [(panel, x, y, n)] = found.as_slice() else {
            return Err(format!(
                "{} panels of the sheet hold its line (one is needed)",
                found.len()
            ));
        };
        // Left or right of the Mitcad line (its own direction, in the
        // panel's front frame): where the side that stays lies.
        let stays = cross(direction, n_sketch);
        let along = [dot(sub(*q, *p), *x), dot(sub(*q, *p), *y)];
        let right = [along[1], -along[0]];
        let side = if dot(stays, *x) * right[0] + dot(stays, *y) * right[1] > 0.0 {
            "right"
        } else {
            "left"
        };
        // A positive angle turns the moving side towards the material's
        // front-to-back normal; the fold turns against the sketch's normal.
        let negative = dot(n_sketch, *n) > 0.0;
        let value = |key: &str, negative: bool| -> Result<Value, String> {
            let v = self
                .value(&sheet.get(key).and_then(reference))
                .ok_or_else(|| format!("no {key}"))?;
            Ok(match (negative, &v) {
                (false, _) => v,
                (true, Value::String(name)) => json!(format!("-({name})")),
                (true, v) => json!(-v.as_f64().unwrap_or(0.0)),
            })
        };
        let def = json!({
            "type": "sheet_fold",
            "body": body,
            "panel": panel,
            "line": {"sketch": sketch_uid, "curve": format!("c{}", curve.0)},
            "stationary_side": side,
            "angle": value("angle", negative)?,
            "position": "edge",
            "radius": value("radius", false)?,
        });
        let mut c = Candidate::new(def);
        c.input_provenance = InputProvenance::Read;
        c.note = Some(
            "a fold: its line a line of its sketch, the side that stays and its direction from \
             the fold's enumeration and booleans (the one decoded case)"
                .to_owned(),
        );
        Ok(vec![c])
    }

    fn sheet_thickness(&self, state: &Value) -> Option<f64> {
        let rules = state["rules"].as_str()?;
        self.sheet_rules(rules).ok()?["rules"]["thickness"].as_f64()
    }

    fn sheet_offsets(
        &self,
        state: &Value,
        b: &Value,
        x: &Value,
    ) -> Result<(&'static str, Value), String> {
        let edge = state["charts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["domain"]["kind"] == "panel" && c["domain"]["id"] == b["panel"])
            .flat_map(|c| {
                c["regions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|r| r["loops"].as_array().into_iter().flatten())
                    .flat_map(|l| l["edges"].as_array().into_iter().flatten())
                    .map(move |e| (c, e))
            })
            .find(|(_, e)| {
                e["piece"]["curve"] == b["curve"]
                    && b.get("piece").is_none_or(|p| e["piece"]["piece"] == *p)
            });
        let (chart, edge) = edge.ok_or("its boundary was not found in the sheet")?;
        let g = &edge["curve"]["geometry"];
        let f = &chart["embedding"]["front_frame"];
        let (Some(o), Some(ax), Some(ay), Some(a), Some(z)) = (
            vector(&f["origin"]),
            vector(&f["x_axis"]),
            vector(&f["y_axis"]),
            vector2(&g["start"]),
            vector2(&g["end"]),
        ) else {
            return Err("its boundary is not a line of a planar panel".to_owned());
        };
        let at = |p: [f64; 2]| add(o, add(ax.map(|c| c * p[0]), ay.map(|c| c * p[1])));
        let positive = chart["embedding"]["front_to_back_sign"] == "positive";
        let (start, end) = if positive {
            (at(z), at(a))
        } else {
            (at(a), at(z))
        };
        let (Some(from), Some(to)) = (vector(&x["from"]), vector(&x["to"])) else {
            return Err("its ends were not read".to_owned());
        };
        let (from, to) = (from.map(|c| c * 10.0), to.map(|c| c * 10.0));
        // (The ends lie on either surface of the panel: compared in its
        // plane.)
        let n = unit_div(cross(ax, ay)).ok_or("its panel has no plane")?;
        let near = |p: [f64; 3], q: [f64; 3]| {
            let d = sub(p, q);
            norm(sub(d, n.map(|c| c * dot(d, n)))) <= 1e-2
        };
        let value = |key: &str| {
            self.value(&x.get(key).and_then(reference))
                .ok_or_else(|| format!("its {key} was not read"))
        };
        let (first, second) = if near(from, start) && near(to, end) {
            ("offset_from", "offset_to")
        } else if near(to, start) && near(from, end) {
            ("offset_to", "offset_from")
        } else {
            return Err("its ends are not the ends of its boundary in the replay".to_owned());
        };
        // A negative offset: the flange goes beyond that end of the edge.
        // Mitcad's width from the boundary's midpoint takes it: each side
        // half the boundary less the offset (mitcad#242).
        let negative = [first, second].iter().any(|key| {
            x.get(*key)
                .and_then(reference)
                .as_ref()
                .and_then(Reference::parameter)
                .and_then(|p| p.value)
                .is_some_and(|v| v < 0.0)
        });
        if !negative {
            return Ok((
                "offsets",
                json!({"start": value(first)?, "end": value(second)?}),
            ));
        }
        let half = norm(sub(end, start)) / 2.0;
        let less = |key: &str| -> Result<Value, String> {
            Ok(match value(key)? {
                Value::String(name) => json!(format!("{} mm - ({name})", fmt_mm(half))),
                v => json!(half - v.as_f64().unwrap_or(0.0)),
            })
        };
        Ok((
            "width",
            // The overhang starts at the bend's moving tangent line, not
            // cut back by a relief *(seen: the corpus's overhanging flange,
            // its history state)*.
            json!({"type": "middle", "before": less(first)?, "after": less(second)?,
                   "root": 0}),
        ))
    }

    fn sheet_state(&self, body: &str) -> Result<Value, String> {
        Ok(self.doc.sheet_state_json(Some(query_body(body)?)))
    }

    fn sheet_states(&self) -> Result<Value, String> {
        Ok(self.doc.sheet_state_json(None))
    }

    fn sheet_rules(&self, feature: &str) -> Result<Value, String> {
        let feature = feature
            .parse::<FeatureUid>()
            .map_err(|e| format!("invalid query: {e}"))?;
        Ok(self.doc.sheet_rules_json(Some(feature)))
    }

    fn sheet_reference(
        &self,
        body: &str,
        edge: &str,
        at: Option<[f64; 3]>,
    ) -> Result<Value, String> {
        self.doc
            .sheet_reference_json(query_body(body)?, None, Some(edge), at)
    }

    fn sheet_body(&self, sheet: &Map<String, Value>) -> Result<String, String> {
        let refs: Option<Vec<Reference>> = sheet
            .get("participantBodies")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(reference).collect());
        if let Some(b) = self.participants(&refs).into_iter().next() {
            return Ok(b);
        }
        let all = self.sheet_states()?;
        let valid: Vec<&Value> = all
            .as_array()
            .map(|a| a.iter().filter(|s| s["status"] == "valid").collect())
            .unwrap_or_default();
        match valid.as_slice() {
            [one] => one["body"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "a sheet body without an id".to_owned()),
            [] => Err("no valid sheet body before it".to_owned()),
            _ => Err("its sheet body was not decoded, and there are several".to_owned()),
        }
    }

    fn sheet_flange(&mut self, sheet: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let edges = sheet.get("edges").and_then(Value::as_u64).unwrap_or(0);
        // The decoded edges (the `.f3d` edge flange's inputs).
        let decoded: Vec<Map<String, Value>> = match sheet.get("edge_list") {
            Some(Value::Array(list)) => {
                list.iter().filter_map(|e| e.as_object().cloned()).collect()
            }
            _ => sheet
                .get("edge")
                .and_then(Value::as_object)
                .cloned()
                .into_iter()
                .collect(),
        };
        // (The `.ipt` flange gives its stored path besides its edges.)
        if !decoded.is_empty() && sheet.get("path").is_none() {
            return self.sheet_edge_flange(sheet, &decoded, edges as usize);
        }
        // The `.ipt` flange's edges by their names (mitcad#242): every one
        // read from the file, or the flange is not translated.
        let errors: Vec<&str> = sheet
            .get("edge_errors")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !errors.is_empty() {
            return Err(format!(
                "its edges were not all found by their names in the file: {}",
                errors.join("; ")
            ));
        }
        if decoded.len() != edges as usize {
            return Err(format!(
                "{} of its {edges} edges read from the file",
                decoded.len()
            ));
        }
        // A flange on several edges with corner settings of its own (not
        // the style's): the corners it makes where its walls meet are not
        // decoded (mitcad#242, `core/ipt/README.md`, *Sheet metal*); with
        // the style's settings Mitcad's trim across the earlier bend gives
        // the file's corners *(verified: the corpus's flanges on adjacent
        // edges)*.
        let corner = sheet.get("corner").unwrap_or(&Value::Null);
        if edges > 1 && corner["style"] == false {
            return Err(format!(
                "a flange on {edges} edges with corner settings of its own (corner relief \
                 shape {}, gap {}): the corners its walls make are not decoded",
                corner["relief_shape"],
                corner["gap"]["value"]
                    .as_f64()
                    .map_or("?".to_owned(), |g| format!("{:.4} mm", g * 10.0))
            ));
        }
        if sheet.get("operation").and_then(Value::as_str) != Some("JoinFeatureOperation") {
            return Err("a sheet-metal flange that makes a new body".to_owned());
        }
        let body = self.sheet_body(sheet)?;
        let state = self.sheet_state(&body)?;
        if state["status"] != "valid" {
            return Err(self.lost_sheet(&body, &state));
        }
        let rules_uid = state["rules"].as_str().ok_or("its sheet has no rules")?;
        let rules = self.sheet_rules(rules_uid)?;
        let t = rules["rules"]["thickness"]
            .as_f64()
            .ok_or("its rules have no thickness")?;
        let k = rules["rules"]["allowance"]["k"]
            .as_f64()
            .ok_or("its rules have no K-factor")?;
        let points: Vec<[f64; 3]> = sheet
            .get("path")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(vector)
                    .map(|p| p.map(|x| x * 10.0))
                    .collect()
            })
            .unwrap_or_default();
        let (Some(&start), Some(&end)) = (points.first(), points.last()) else {
            return Err("its path was not decoded".to_owned());
        };
        // The path's start direction (the first point 1e-3 mm away).
        let along = points
            .iter()
            .map(|p| sub(*p, start))
            .find(|d| norm(*d) > 1e-3)
            .and_then(unit_div)
            .ok_or("its path has no length")?;
        let param_value = |key: &str| -> Option<f64> {
            reference(sheet.get(key)?)
                .as_ref()
                .and_then(Reference::parameter)
                .and_then(|p| p.value)
        };
        let theta = param_value("angle").ok_or("no angle")?.abs();
        let r = param_value("radius").ok_or("no bend radius")? * 10.0;
        let datum = sheet
            .get("length_datum")
            .and_then(vector)
            .map(|p| p.map(|x| x * 10.0))
            .ok_or("no length datum")?;
        let si = r * (theta / 2.0).tan();
        let so = (r + t) * (theta / 2.0).tan();
        let ba = theta * (r + k * t);
        const TOL: f64 = 1e-3;

        // The panel whose surface holds the path's start.
        let mut found: Option<(Value, Value, f64, bool, [f64; 3])> = None;
        for chart in state["charts"].as_array().into_iter().flatten() {
            if chart["domain"]["kind"] != "panel" || chart["embedding"]["type"] != "plane" {
                continue;
            }
            let f = &chart["embedding"]["front_frame"];
            let (Some(o), Some(x), Some(y)) = (
                vector(&f["origin"]),
                vector(&f["x_axis"]),
                vector(&f["y_axis"]),
            ) else {
                continue;
            };
            let sign = if chart["embedding"]["front_to_back_sign"] == "negative" {
                -1.0
            } else {
                1.0
            };
            let n = cross(x, y).map(|c| c * sign);
            let q = dot(sub(start, o), n);
            let back = (q - t).abs() <= TOL;
            // (The path's first points are on its bend's arc, which leaves
            // the surface tangentially: the chord to the next point is a
            // little off the plane.)
            if !back && q.abs() > TOL || dot(along, n).abs() > 0.5 {
                continue;
            }
            let p = [dot(sub(start, o), x), dot(sub(start, o), y)];
            let flat = [dot(along, x), dot(along, y)];
            let len = (flat[0] * flat[0] + flat[1] * flat[1]).sqrt();
            let d = [flat[0] / len, flat[1] / len];
            let in_plane = [0, 1, 2].map(|i| d[0] * x[i] + d[1] * y[i]);
            // The first straight boundary the ray meets.
            let mut best: Option<(f64, &Value)> = None;
            for e in chart["regions"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|r| r["loops"].as_array().into_iter().flatten())
                .flat_map(|l| l["edges"].as_array().into_iter().flatten())
            {
                let g = &e["curve"]["geometry"];
                if g["type"] != "line" {
                    continue;
                }
                let (Some(a), Some(b)) = (vector2(&g["start"]), vector2(&g["end"])) else {
                    continue;
                };
                if let Some(s) = ray_hit(p, d, a, b)
                    && best.is_none_or(|(bs, _)| s < bs)
                {
                    best = Some((s, e));
                }
            }
            if let Some((s, e)) = best {
                if found.is_some() {
                    return Err("its path starts on two panels".to_owned());
                }
                found = Some((chart["domain"]["id"].clone(), e.clone(), s, back, in_plane));
            }
        }
        let (panel, edge, s, back, along) = found.ok_or("no panel holds its path's start")?;
        let mut notes =
            vec!["edge: the panel's boundary its stored path starts towards".to_owned()];
        let position = [
            ("outside", si),
            ("inside", so),
            ("bend_line", ba / 2.0),
            ("edge", 0.0),
        ]
        .into_iter()
        .filter(|(_, v)| (v - s).abs() <= TOL)
        .map(|(p, _)| p)
        .collect::<Vec<_>>();
        let [position] = position.as_slice() else {
            return Err(format!(
                "its tangent line lies {s:.4} mm from its edge: no single position \
                 (outside {si:.4}, inside {so:.4}, bend line {:.4})",
                ba / 2.0
            ));
        };
        notes.push(format!("position {position} from the path"));
        // The material's front-to-back normal of the panel.
        let n_back = {
            let f = &state["charts"]
                .as_array()
                .and_then(|c| {
                    c.iter()
                        .find(|c| c["domain"]["id"] == panel && c["domain"]["kind"] == "panel")
                })
                .ok_or("no chart")?["embedding"];
            let (x, y) = (
                vector(&f["front_frame"]["x_axis"]).ok_or("no frame")?,
                vector(&f["front_frame"]["y_axis"]).ok_or("no frame")?,
            );
            let sign = if f["front_to_back_sign"] == "negative" {
                -1.0
            } else {
                1.0
            };
            cross(x, y).map(|c| c * sign)
        };
        // The bend goes to the side the path's end lies on: towards the
        // back (a positive angle) or the front; that side's surface is the
        // inner one. The path runs on either surface *(seen: on the back
        // surface in every flange of the corpus, inner or outer)*.
        let towards = dot(sub(end, start), n_back);
        if towards.abs() <= TOL {
            return Err("its path ends in its panel's plane".to_owned());
        }
        let positive = towards > 0.0;
        let n_inner = n_back.map(|c| if positive { c } else { -c });
        // The path's start on the inner surface.
        let inner_start = if back == positive {
            start
        } else {
            add(start, n_inner.map(|c| c * t))
        };
        let edge_dir = unit_div(cross(along, n_inner)).ok_or("no edge direction")?;
        let in_section = |p: [f64; 3]| {
            let v = sub(p, inner_start);
            sub(v, edge_dir.map(|c| c * dot(v, edge_dir)))
        };
        let datum = in_section(datum);
        let inner_sharp = along.map(|c| c * si);
        let outer_sharp = sub(along.map(|c| c * so), n_inner.map(|c| c * t));
        let mode = if norm(sub(datum, outer_sharp)) <= TOL {
            "outer"
        } else if norm(sub(datum, inner_sharp)) <= TOL {
            "inner"
        } else if norm(datum) <= TOL || norm(add(datum, n_inner.map(|c| c * t))) <= TOL {
            "tangent"
        } else {
            return Err("its length is measured from no virtual sharp of the bend".to_owned());
        };
        notes.push(format!("length datum {mode} from its dimension's point"));
        let signed = |key: &str, negative: bool| -> Result<Value, String> {
            let v = self
                .value(&sheet.get(key).and_then(reference))
                .ok_or_else(|| format!("no {key}"))?;
            Ok(match (negative, &v) {
                (false, _) => v,
                (true, Value::String(name)) => json!(format!("-{name}")),
                (true, v) => json!(-v.as_f64().unwrap_or(0.0)),
            })
        };
        let mut boundary = json!({"panel": panel, "curve": edge["piece"]["curve"]});
        // A curve in pieces names its piece.
        let pieces = state["charts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["domain"]["id"] == panel && c["domain"]["kind"] == "panel")
            .flat_map(|c| c["regions"].as_array().into_iter().flatten())
            .flat_map(|r| r["loops"].as_array().into_iter().flatten())
            .flat_map(|l| l["edges"].as_array().into_iter().flatten())
            .filter(|e| e["piece"]["curve"] == edge["piece"]["curve"])
            .count();
        if pieces > 1 {
            boundary["piece"] = edge["piece"]["piece"].clone();
        }
        let def = json!({
            "type": "sheet_flange",
            "body": body,
            "boundary": boundary,
            "angle": signed("angle", !positive)?,
            "length": {"mode": mode, "value": signed("length", false)?},
            "position": position,
            "radius": signed("radius", false)?,
        });
        if decoded.is_empty() {
            // (A file without names of its edges: the flange's edge is the
            // boundary its path starts towards.)
            if edges > 1 {
                return Err(format!(
                    "a flange on {edges} edges whose edges were not read from the file"
                ));
            }
            let mut c = Candidate::new(def);
            c.input_provenance = InputProvenance::Inferred;
            c.note = Some(notes.join("; "));
            return Ok(vec![c]);
        }
        // Each edge read from the file (mitcad#242), found in the replay by
        // its geometry in the history state the flange starts from: a
        // straight boundary of a panel of the sheet, on the surface the
        // path's edge is on (the flange bends the same way at each).
        let mut defs: Vec<Value> = Vec::new();
        let mut sides: Vec<bool> = Vec::new();
        let mut path_side = None;
        for (k, e) in decoded.iter().enumerate() {
            let fp = mitcad_dump::Fingerprint::from_map(e.clone());
            let mid = fp
                .mid_point
                .ok_or_else(|| format!("edge {}: no middle point", k + 1))?;
            let (on, name) = mitcad_import_replay::internal::refs::resolve_edge(self.doc, &fp)
                .ok_or_else(|| format!("edge {}: not found in the replay", k + 1))?;
            if on.to_string() != body {
                return Err(format!("edge {}: on another body than its sheet", k + 1));
            }
            let name = name.to_string();
            let answer = self.sheet_reference(&body, &name, Some(mid.map(|x| x * 10.0)))?;
            let b = answer
                .get("boundary")
                .filter(|b| b.is_object())
                .cloned()
                .ok_or_else(|| format!("edge {}: {name} is not a panel's boundary", k + 1))?;
            if answer["straight"] != true {
                return Err(format!("edge {}: not straight", k + 1));
            }
            let on_back = name.contains("sheet_back(");
            if !on_back && !name.contains("sheet_front(") {
                return Err(format!(
                    "edge {}: {name} is not on a panel's surface",
                    k + 1
                ));
            }
            // The path's edge: the boundary it starts towards.
            if b["panel"] == boundary["panel"]
                && b["curve"] == boundary["curve"]
                && b.get("piece")
                    .is_none_or(|p| Some(p) == boundary.get("piece"))
            {
                path_side = Some(on_back);
            }
            let mut d = def.clone();
            // A width extent: offsets from the edge's two ends, each
            // measured from the end the file names; Mitcad's start offset
            // is at the boundary's start along its axial direction.
            if let Some(x) = e.get("width_extent").filter(|x| x.is_object()) {
                let (key, v) = self
                    .sheet_offsets(&state, &b, x)
                    .map_err(|why| format!("edge {}: its width extent: {why}", k + 1))?;
                d[key] = v;
            }
            d["boundary"] = b;
            defs.push(d);
            sides.push(on_back);
        }
        let Some(side) = path_side else {
            return Err("its stored path starts towards none of the edges it names".to_owned());
        };
        if sides.iter().any(|&s| s != side) {
            return Err("its edges lie on both surfaces of the sheet".to_owned());
        }
        let mut c = Candidate::new(defs.remove(0));
        c.defs.extend(defs);
        c.input_provenance = InputProvenance::Inferred;
        notes[0] = format!("edges: {} read from the file by their names", decoded.len());
        c.note = Some(notes.join("; "));
        Ok(vec![c])
    }

    fn sheet_hem(&mut self, sheet: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        // A parameter in the model's terms: the imported parameter, else
        // its value (mm; an angle in radians).
        let model = |key: &str| {
            sheet.get(key).and_then(|p| {
                self.value(&reference(p)).or_else(|| {
                    let v = p.get("value").and_then(Value::as_f64)?;
                    Some(json!(if key == "hem_angle" { v } else { v * 10.0 }))
                })
            })
        };
        let (flange, note) = hem_as_flange(sheet, &model)?;
        let edges: Vec<Map<String, Value>> = match flange.get("edge_list") {
            Some(Value::Array(list)) => {
                list.iter().filter_map(|e| e.as_object().cloned()).collect()
            }
            _ => flange
                .get("edge")
                .and_then(Value::as_object)
                .cloned()
                .into_iter()
                .collect(),
        };
        if edges.is_empty() {
            return Err("its edge was not decoded".to_owned());
        }
        let mut candidates = self.sheet_edge_flange(&flange, &edges, 1)?;
        for c in &mut candidates {
            c.note = Some(note.clone());
        }
        Ok(candidates)
    }
}

/// The flange a hem is (mitcad#1364, mitcad#1378), with a note, or why it
/// is none; `model` gives a hem parameter (`gap`, `length`, `hem_radius`,
/// `hem_angle`, `setback`) in the model's terms: an imported parameter's
/// name, else its value (mm, an angle in radians). Every form folds over
/// the panel at the hem's bend position (`tangent`: `side_tangent`), its
/// length measured from the plane touching the bend's outer surface (the
/// length datum `tangent`, MitCad's `outer_tangent`):
///
/// - an open or flat hem folds its wall back over the panel in one bend of
///   180 degrees: a flange of 180 degrees whose inside radius is half the
///   gap *(verified: the volumes of the open hem of gap 2 mm and the flat
///   hem of gap 0.02 mm of the reference models; the flat hem's gap is its
///   inside diameter too)*;
/// - a rolled hem is one bend of its angle and inside radius with no wall:
///   a flange of that angle, its length 0 from the bend's tangent line
///   (`straight`, MitCad's `tangent`) *(verified: the reference model's
///   rolled hem of 270 degrees and radius 3 mm, its volume)*;
/// - a teardrop or rope hem is the flange's hem of that form with the
///   hem's gap, its radius the hem's radius *(verified: the volumes of the
///   reference models' teardrop and rope hems of radius 2 mm, gap 1 mm and
///   length 10 mm; the inner end corner of the teardrop's wall lies a gap
///   above the panel and its outer one at the length; the rope's second
///   bend, of the same radius, turns back by as much as the first passes
///   180 degrees, its wall a gap above the panel, ending at the length)*;
/// - a double hem is the flange's double hem with the hem's gap and
///   setback, every bend's inside radius half the gap: its length runs to
///   the second fold's outer surface, and the wall folded under ends the
///   setback short of the first wall's inner surface *(verified: the
///   reference model's double hem of gap 2 mm, length 10 mm, setback 1 mm:
///   its stored faces (two bends of 90 degrees with a wall of gap +
///   thickness between them, walls of 3 mm over and under the 180-degree
///   fold) and volume)*.
fn hem_as_flange(
    sheet: &Map<String, Value>,
    model: &dyn Fn(&str) -> Option<Value>,
) -> Result<(Map<String, Value>, String), String> {
    let form = sheet
        .get("form")
        .and_then(Value::as_str)
        .ok_or("a hem of no form")?;
    let needed =
        |key: &str, what: &str| model(key).ok_or_else(|| format!("its {what} was not decoded"));
    let half = |gap: Value| -> Result<Value, String> {
        Ok(match gap {
            Value::String(name) => json!(format!("{name} / 2")),
            v => json!(v.as_f64().ok_or("its gap is not a number")? / 2.0),
        })
    };
    let mut flange = sheet.clone();
    flange.insert("kind".to_owned(), json!("flange"));
    flange.insert("angle".to_owned(), json!({"model": std::f64::consts::PI}));
    flange.insert("length_mode".to_owned(), json!("tangent"));
    let note = match form {
        "open" | "flat" => {
            let radius = half(needed("gap", "gap")?)?;
            flange.insert("radius".to_owned(), json!({"model": radius}));
            format!(
                "{} hem: a flange of 180 degrees whose inside radius is half its gap, its length \
                 measured from the bend's outer surface",
                if form == "open" { "an open" } else { "a flat" }
            )
        }
        "rolled" => {
            let angle = needed("hem_angle", "angle")?;
            let radius = needed("hem_radius", "radius")?;
            flange.insert("angle".to_owned(), json!({"model": angle}));
            flange.insert("radius".to_owned(), json!({"model": radius}));
            flange.insert("length".to_owned(), json!({"model": 0.0}));
            flange.insert("length_mode".to_owned(), json!("straight"));
            "a rolled hem: a flange of its angle and radius with no wall".to_owned()
        }
        "teardrop" | "rope" => {
            let radius = needed("hem_radius", "radius")?;
            let gap = needed("gap", "gap")?;
            flange.insert("radius".to_owned(), json!({"model": radius}));
            flange.insert("hem_model".to_owned(), json!({"form": form, "gap": gap}));
            format!(
                "a {form} hem: a flange's {form} hem of its gap, radius and length, its bend past \
                 180 degrees"
            )
        }
        "double" => {
            let gap = needed("gap", "gap")?;
            let setback = needed("setback", "setback")?;
            flange.insert("radius".to_owned(), json!({"model": half(gap.clone())?}));
            flange.insert(
                "hem_model".to_owned(),
                json!({"form": "double", "gap": gap, "setback": setback}),
            );
            "a double hem: a flange's double hem of its gap, setback and length, every bend's \
             inside radius half its gap"
                .to_owned()
        }
        other => return Err(format!("a hem of form {other}: not known")),
    };
    Ok((flange, note))
}

/// A length in mm for an expression: at most 12 significant digits.
fn fmt_mm(v: f64) -> String {
    let s = format!("{v:.12}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

fn vector2(v: &Value) -> Option<[f64; 2]> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
}

/// The direction a face's material lies along from its sketch's plane:
/// the decoded direction, reversed by side 1 (the `.ipt` plate's side
/// enumeration, mitcad#242).
fn material_direction(sheet: &Map<String, Value>) -> Result<[f64; 3], String> {
    let direction = sheet
        .get("direction")
        .and_then(vector)
        .ok_or("its material direction was not decoded")?;
    Ok(match sheet.get("side").and_then(Value::as_u64) {
        Some(1) => direction.map(|c| -c),
        Some(0) | None => direction,
        Some(other) => return Err(format!("its plate's side {other} is not known")),
    })
}

/// An `.f3d` flange's edge found in the replay: its body, name, panel
/// boundary, whether it lies on the panel's back surface, and whether the
/// file's edge runs along the boundary's axial direction.
type FoundEdge = (String, String, Value, bool, Option<bool>);

/// Where the ray `p + s d` (s > 0) crosses the segment `a b`.
/// Whether a point lies inside a polygon (even-odd).
fn inside2(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
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

fn ray_hit(p: [f64; 2], d: [f64; 2], a: [f64; 2], b: [f64; 2]) -> Option<f64> {
    let e = [b[0] - a[0], b[1] - a[1]];
    let den = d[0] * e[1] - d[1] * e[0];
    if den.abs() < 1e-12 {
        return None;
    }
    let w = [a[0] - p[0], a[1] - p[1]];
    let s = (w[0] * e[1] - w[1] * e[0]) / den;
    let u = (w[0] * d[1] - w[1] * d[0]) / den;
    (s > -1e-6 && (-1e-9..=1.0 + 1e-9).contains(&u)).then_some(s.max(0.0))
}

/// A body by its id as the sheet queries take it.
fn query_body(body: &str) -> Result<mitcad_model::BodyUid, String> {
    body.parse().map_err(|e| format!("invalid query: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ray from inside a square meets the side it points to, at its
    /// distance; one pointing away from a segment misses it.
    #[test]
    fn a_ray_meets_the_boundary_it_points_to() {
        let p = [2.0, 3.0];
        let hit = ray_hit(p, [0.0, -1.0], [0.0, 0.0], [10.0, 0.0]);
        assert!((hit.unwrap() - 3.0).abs() < 1e-12);
        assert_eq!(ray_hit(p, [0.0, 1.0], [0.0, 0.0], [10.0, 0.0]), None);
        assert_eq!(ray_hit(p, [1.0, 0.0], [0.0, 0.0], [10.0, 0.0]), None);
    }

    /// A decoded hem of a form (the decoder's keys, parameters in cm).
    fn hem(form: &str) -> Map<String, Value> {
        let p = |name: &str, value: f64| json!({"kind": "parameter", "name": name, "value": value, "unit": "mm"});
        let v = json!({
            "kind": "hem", "operation": "JoinFeatureOperation", "form": form,
            "edges": 1, "position": "tangent",
            "edge_list": [{"kind": "edge", "mid_point": [5.0, 0.0, 0.25]}],
            "gap": p("d2", 0.2), "length": p("d3", 1.0), "hem_radius": p("d4", 0.3),
            "hem_angle": {"kind": "parameter", "name": "d5",
                          "value": 1.5 * std::f64::consts::PI, "unit": "deg"},
            "setback": p("d6", 0.1),
            "rule_radius": {"value": 0.25, "from_rule": true},
        });
        v.as_object().expect("an object").clone()
    }

    /// The hem's parameters by their imported names (`None`: not imported).
    fn names(key: &str) -> Option<Value> {
        let name = match key {
            "gap" => "d2",
            "length" => "d3",
            "hem_radius" => "d4",
            "hem_angle" => "d5",
            "setback" => "d6",
            _ => return None,
        };
        Some(json!(name))
    }

    /// An open or flat hem is a 180-degree flange of half its gap, its
    /// length from the bend's outer surface, on its edge at its position.
    #[test]
    fn an_open_hem_is_a_flange_of_half_its_gap() {
        let (f, note) = hem_as_flange(&hem("open"), &names).expect("a flange");
        assert_eq!(f["kind"], "flange");
        assert_eq!(f["angle"], json!({"model": std::f64::consts::PI}));
        assert_eq!(f["radius"], json!({"model": "d2 / 2"}));
        assert_eq!(f["length_mode"], "tangent");
        assert_eq!(f["position"], "tangent");
        assert_eq!(f["length"]["name"], "d3");
        assert_eq!(f["edge_list"].as_array().map(Vec::len), Some(1));
        assert!(f.get("hem_model").is_none());
        assert!(
            note.starts_with("an open hem: a flange of 180 degrees"),
            "{note}"
        );
        // A gap that is no imported parameter: half its mm.
        let mm = |key: &str| {
            if key == "gap" {
                Some(json!(0.02))
            } else {
                None
            }
        };
        let (f, _) = hem_as_flange(&hem("flat"), &mm).expect("a flange");
        assert_eq!(f["radius"], json!({"model": 0.01}));
        assert!(hem_as_flange(&hem("open"), &|_: &str| -> Option<Value> { None }).is_err());
    }

    /// The forms past 180 degrees (mitcad#1378): a rolled hem is a flange
    /// of its angle and radius with no wall; a teardrop, rope or double hem
    /// the flange's hem of that form, its length from the bend's outer
    /// surface (a double hem's every bend of half its gap).
    #[test]
    fn hems_past_half_a_turn_are_flanges_with_their_form() {
        let (f, note) = hem_as_flange(&hem("rolled"), &names).expect("a rolled flange");
        assert_eq!(f["angle"], json!({"model": "d5"}));
        assert_eq!(f["radius"], json!({"model": "d4"}));
        assert_eq!(f["length"], json!({"model": 0.0}));
        assert_eq!(f["length_mode"], "straight");
        assert!(f.get("hem_model").is_none());
        assert!(note.starts_with("a rolled hem"), "{note}");
        for form in ["teardrop", "rope"] {
            let (f, note) = hem_as_flange(&hem(form), &names).expect("a hem flange");
            assert_eq!(f["angle"], json!({"model": std::f64::consts::PI}));
            assert_eq!(f["radius"], json!({"model": "d4"}));
            assert_eq!(f["length"]["name"], "d3");
            assert_eq!(f["length_mode"], "tangent");
            assert_eq!(f["hem_model"], json!({"form": form, "gap": "d2"}));
            assert!(note.contains(form), "{note}");
        }
        let (f, _) = hem_as_flange(&hem("double"), &names).expect("a double hem flange");
        assert_eq!(f["radius"], json!({"model": "d2 / 2"}));
        assert_eq!(
            f["hem_model"],
            json!({"form": "double", "gap": "d2", "setback": "d6"})
        );
        // An angle that is no imported parameter stays in radians.
        let raw = |key: &str| {
            if key == "hem_angle" {
                Some(json!(4.5))
            } else {
                names(key)
            }
        };
        let (f, _) = hem_as_flange(&hem("rolled"), &raw).expect("a rolled flange");
        assert_eq!(f["angle"], json!({"model": 4.5}));
        let none = |key: &str| if key == "setback" { None } else { names(key) };
        assert!(
            hem_as_flange(&hem("double"), &none)
                .unwrap_err()
                .contains("setback")
        );
        assert!(hem_as_flange(&hem("spiral"), &names).is_err());
    }
}
