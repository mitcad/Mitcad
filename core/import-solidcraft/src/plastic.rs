// SPDX-License-Identifier: MIT
//! The file's plastic features (Boss, Lip and Groove, Snap Fit, Rest) and
//! plastic rules as Mitcad's (mitcad#459, `features::plastic`).
//!
//! The rule that applies to a body (the one assigned to it, else the
//! design's active rule; a design's own rule before the library's of the
//! same name) becomes a `plastic_rule` feature, made before the first
//! feature that needs it, with every expression of the rule (`Thickness`
//! in them its own thickness); the features on that body name it, so that
//! a draft or a groove's clearance left out comes from it as in the file.

use mitcad_model::{BodyUid, ComponentUid, FeatureUid, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::kinds as k;
use serde_json::{Value, json};

use super::{Importer, MapResult, Mapped, NotMapped};

impl<K: Kernel> Importer<'_, '_, K> {
    /// The rule that applies to a body, made as a feature when it is not
    /// yet; None without one.
    fn plastic_rule(
        &mut self,
        body: BodyUid,
        component: ComponentUid,
    ) -> Result<Option<FeatureUid>, String> {
        let plastic = &self.design.plastic;
        let file_name = self
            .bodies
            .iter()
            .find(|(_, b)| **b == body)
            .map(|(n, _)| n.clone());
        let name = file_name
            .and_then(|n| {
                plastic
                    .pointer(&format!("/assigned/{}", pointer_key(&n)))
                    .cloned()
            })
            .or_else(|| plastic.get("active").cloned())
            .and_then(|v| v.as_str().map(str::to_owned))
            .filter(|n| !n.is_empty());
        let Some(name) = name else {
            return Ok(None);
        };
        if let Some(uid) = self.plastic_rules.get(&name) {
            return Ok(Some(*uid));
        }
        let own = plastic
            .get("rules")
            .and_then(Value::as_array)
            .and_then(|rules| {
                rules
                    .iter()
                    .find(|r| r.get("name").and_then(Value::as_str) == Some(name.as_str()))
                    .cloned()
            });
        let rule = own
            .or_else(|| {
                mitcad_solidcraft::plastic::library()
                    .into_iter()
                    .find(|r| r.get("name").and_then(Value::as_str) == Some(name.as_str()))
            })
            .ok_or_else(|| format!("the design has no plastic rule `{name}`"))?;
        let text = |field: &str| rule.get(field).and_then(Value::as_str).map(str::to_owned);
        let thickness =
            text("thickness").ok_or_else(|| format!("plastic rule `{name}` has no thickness"))?;
        let e = self.params.expression(self.doc, &thickness)?;
        let slot = self.params.unnamed(self.doc, &e);
        let param = slot
            .split_once(" = ")
            .map_or_else(|| slot.clone(), |(n, _)| n.to_owned());
        let mut def = json!({"type": "plastic_rule", "rule_name": name, "thickness": slot});
        if let Some(material) = text("material").filter(|m| !m.is_empty()) {
            def["material"] = json!(material);
        }
        for (field, _) in mitcad_solidcraft::plastic::FIELDS.iter().skip(1) {
            let Some(expr) = text(field) else {
                continue;
            };
            let e =
                self.params
                    .expression_with(self.doc, &expr, &[("Thickness", param.clone())])?;
            let key = match *field {
                "draft" => "draft_angle",
                "min_draft" => "min_draft_angle",
                other => other,
            };
            def[key] = json!(self.params.unnamed(self.doc, &e));
        }
        let uid = self.add(&def, Some(&name), component)?;
        self.plastic_rules.insert(name, uid);
        Ok(Some(uid))
    }

    /// The slot of the input at `pointer` when the feature has it.
    fn optional_slot(
        &mut self,
        f: &Feature,
        pointer: &str,
        present: bool,
        def: &mut Value,
        key: &str,
    ) -> Result<(), String> {
        if present {
            def[key] = json!(self.slot(f, pointer)?);
        }
        Ok(())
    }
}

/// A body name as a JSON pointer token.
fn pointer_key(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

pub(super) fn boss<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let b: k::Boss = f.typed()?;
    let (body, face) = im.face_named(
        component,
        b.body.as_deref(),
        b.position,
        f.face_names.first().map(String::as_str),
    )?;
    let rule = im.plastic_rule(body, component)?;
    let mut def = json!({"type": "boss", "body": body.to_string(), "face": face,
        "point": b.position, "direction": b.direction,
        "diameter": im.slot(f, "/diameter")?, "height": im.slot(f, "/height")?});
    for (pointer, present, key) in [
        ("/hole_diameter", b.hole_diameter.is_some(), "hole_diameter"),
        ("/hole_depth", b.hole_depth.is_some(), "hole_depth"),
        ("/draft", b.draft.is_some(), "draft_angle"),
        ("/fillet", b.fillet.is_some(), "fillet"),
        ("/ribs", b.ribs.is_some(), "ribs"),
        ("/rib_thickness", b.rib_thickness.is_some(), "rib_thickness"),
        ("/rib_length", b.rib_length.is_some(), "rib_length"),
        ("/rib_offset", b.rib_offset.is_some(), "rib_offset"),
    ] {
        im.optional_slot(f, pointer, present, &mut def, key)?;
    }
    if let Some(rule) = rule {
        def["rule"] = json!(rule.to_string());
    }
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

pub(super) fn lip<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let l: k::Lip = f.typed()?;
    let (body, face) = im.face_named(
        component,
        l.body.as_deref(),
        l.face,
        f.face_names.first().map(String::as_str),
    )?;
    let mut notes = Vec::new();
    let def = if l.groove {
        let rule = im.plastic_rule(body, component)?;
        let mut def = json!({"type": "groove", "body": body.to_string(), "face": face,
            "width": im.slot(f, "/width")?, "depth": im.slot(f, "/height")?,
            "outside": l.outside});
        im.optional_slot(f, "/gap", l.gap.is_some(), &mut def, "gap")?;
        if let Some(rule) = rule {
            def["rule"] = json!(rule.to_string());
        }
        def
    } else {
        if l.gap.is_some() {
            notes.push("its gap is not used: only a groove has one".to_owned());
        }
        json!({"type": "lip", "body": body.to_string(), "face": face,
            "width": im.slot(f, "/width")?, "height": im.slot(f, "/height")?,
            "outside": l.outside})
    };
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::partial(notes))
}

pub(super) fn snap_fit<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let s: k::SnapFit = f.typed()?;
    if s.base_height.is_some() {
        return Err(NotMapped::Unmapped(
            "its base reaches below the face, which MitCad's snap fit does not take".to_owned(),
        ));
    }
    if s.new_body {
        return Err(NotMapped::Unmapped(
            "it is a body of its own, and MitCad's snap fit joins the body it stands on".to_owned(),
        ));
    }
    let (body, face) = im.face_named(
        component,
        s.body.as_deref(),
        s.position,
        f.face_names.first().map(String::as_str),
    )?;
    let def = json!({"type": "snap_fit", "body": body.to_string(), "face": face,
        "point": s.position, "direction": s.direction, "hook": s.hook,
        "length": im.slot(f, "/length")?, "thickness": im.slot(f, "/thickness")?,
        "width": im.slot(f, "/width")?, "catch_depth": im.slot(f, "/catch_depth")?,
        "catch_length": im.slot(f, "/catch_length")?});
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}

pub(super) fn rest<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    f: &Feature,
    component: ComponentUid,
) -> MapResult {
    let r: k::Rest = f.typed()?;
    let (body, face) = im.face_named(
        component,
        r.body.as_deref(),
        r.position,
        f.face_names.first().map(String::as_str),
    )?;
    let rule = im.plastic_rule(body, component)?;
    let mut def = json!({"type": "rest", "body": body.to_string(), "face": face,
        "point": r.position, "direction": r.direction,
        "width": im.slot(f, "/width")?, "height": im.slot(f, "/height")?});
    if let Some(along) = r.along {
        def["along"] = json!(along);
    }
    im.optional_slot(f, "/length", r.length.is_some(), &mut def, "length")?;
    im.optional_slot(f, "/draft", r.draft.is_some(), &mut def, "draft_angle")?;
    im.optional_slot(
        f,
        "/thickness",
        r.thickness.is_some(),
        &mut def,
        "thickness",
    )?;
    if let Some(rule) = rule {
        def["rule"] = json!(rule.to_string());
    }
    im.add(&def, Some(&f.name), component)?;
    Ok(Mapped::parametric())
}
