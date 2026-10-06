// SPDX-License-Identifier: MIT
//! PartDesign features as Mitcad definitions (the plan's feature table,
//! `commands.md` *FreeCAD import*): each feature's candidates, tried in
//! turn and checked against FreeCAD's stored shape ([`super::history`]).
//!
//! - Pad, Pocket → `extrude` join or cut (the Body's first solid: a new
//!   body), on the material regions of the profile sketch: FreeCAD 1.1
//!   pads the regions inside an even number of the sketch's closed wires,
//!   1.0 the outer ones with their holes; both are tried. Length, through
//!   all, up to first, last, a face or a shape, symmetric (Midplane,
//!   SideType), two lengths, taper. Pocket goes against the sketch normal,
//!   `Reversed` turns either round. Along a custom direction off the
//!   sketch's normal: a `sweep` along a fixed line, without turning.
//! - Revolution, Groove → `revolve` join or cut about the reference axis
//!   (a sketch axis line, a line of the sketch, an origin axis, a datum, an
//!   edge); angle, full turn, Midplane (half each way), two angles, up to
//!   a face, a shape or the first face; the sign is tried both ways.
//! - Fillet, Chamfer → `fillet` and `chamfer` on the referenced edges and
//!   faces (all edges with `UseAllEdges`); a chamfer's first distance on
//!   either face is tried.
//! - Hole → `hole` at the profile sketch's circles' centres: depth or
//!   through all, flat or angled point, counterbore, countersink,
//!   counterdrill, tapered; an ISO metric, Unified, Whitworth or NPT thread
//!   is a `thread` on the walls (the hole has FreeCAD's diameter), a
//!   modelled one (ISO metric or Unified) cut as Mitcad cuts threads.
//! - Mirrored, LinearPattern, PolarPattern → `mirror`,
//!   `rectangular_pattern`, `circular_pattern` of the original features'
//!   Mitcad features (or of the body, `TransformMode` whole shape);
//!   MultiTransform → those of one transformation, of two linear patterns
//!   (one pattern of two directions) or of two mirrors (and a half turn),
//!   and for any sequence of mirrors and patterns each transformation a
//!   pattern or mirror of the one before (patterns of patterns).
//! - Boolean → `combine` with the other Bodies' bodies.
//! - Draft → `draft` (a pull direction along the neutral plane's normal),
//!   Thickness → `shell` (Pipe and RectoVerso as Skin), AdditiveLoft →
//!   `loft`, AdditivePipe → `sweep` (with sections: a `loft` along the
//!   spine), AdditiveHelix → `helix`; the additive and subtractive
//!   primitives ([`super::primitives`]).
//! - Datums → construction planes (offset from an origin plane or a face
//!   when FreeCAD attaches them so), axes and points (fixed).
//!
//! The lengths, angles, sizes and counts are the properties' parameters or
//! expressions where FreeCAD's are parametric ([`super::params`]).

use mitcad_freecad::sketch::{self as fc, Curve};
use mitcad_freecad::{LinkRef, Object, Value as FcValue};
use mitcad_model::{BodyUid, ComponentUid, Kernel, SketchFrame};
use serde_json::{Value, json};

use super::Importer;
use super::elements::sub_name;
use super::history::{Candidate, SketchMade, Target};
use super::params::{Kind, Q};
use super::sketches::Placed;
use crate::geom::{add, cross, dot, norm, scale, sub, unit};

fn uids(bodies: &[BodyUid]) -> Vec<String> {
    bodies.iter().map(ToString::to_string).collect()
}

/// A reading of a datum: its definition, notes, the plane it must be when
/// that is to be checked, and the bound properties it carries (their
/// expressions are left out when it is not taken).
pub(super) struct DatumReading {
    pub def: Value,
    pub notes: Vec<String>,
    pub check: Option<SketchFrame>,
    pub carries: Vec<String>,
}

/// A profile's regions: the material regions, and the alternative
/// reading (outer regions only) when it differs.
pub(super) struct Profile {
    pub sketch: SketchMade,
    pub regions: Vec<String>,
    pub outer: Option<Vec<String>>,
}

impl<K: Kernel> Importer<'_, '_, K> {
    pub(super) fn feature_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> (Result<Vec<Candidate>, String>, &'static str) {
        let t = o.type_name.as_str();
        match t {
            "PartDesign::Pad" => (self.extrude_candidates(o, target, false), "extrude"),
            "PartDesign::Pocket" => (self.extrude_candidates(o, target, true), "extrude"),
            "PartDesign::Revolution" => (self.revolve_candidates(o, target, false), "revolve"),
            "PartDesign::Groove" => (self.revolve_candidates(o, target, true), "revolve"),
            "PartDesign::Fillet" => (self.fillet_candidates(o, target), "fillet"),
            "PartDesign::Chamfer" => (self.chamfer_candidates(o, target), "chamfer"),
            "PartDesign::Hole" => (self.hole_candidates(o, target), "hole"),
            "PartDesign::Mirrored" => (self.mirrored_candidates(o, target), "mirror"),
            "PartDesign::LinearPattern" => {
                (self.linear_candidates(o, target), "rectangular_pattern")
            }
            "PartDesign::PolarPattern" => (self.polar_candidates(o, target), "circular_pattern"),
            "PartDesign::MultiTransform" => (self.multi_candidates(o, target), ""),
            "PartDesign::Boolean" => (self.boolean_candidates(o, target), "combine"),
            "PartDesign::Draft" => (self.draft_candidates(o, target), "draft"),
            "PartDesign::Thickness" => (self.thickness_candidates(o, target), "shell"),
            "PartDesign::AdditiveLoft" | "PartDesign::SubtractiveLoft" => {
                (self.loft_candidates(o, target), "loft")
            }
            "PartDesign::AdditivePipe" | "PartDesign::SubtractivePipe" => {
                (self.pipe_candidates(o, target), "")
            }
            "PartDesign::AdditiveHelix" | "PartDesign::SubtractiveHelix" => {
                (self.helix_candidates(o, target), "helix")
            }
            _ => match super::primitives::primitive_kind(t) {
                // The type is the main definition's.
                Some((kind, additive)) => (
                    self.primitive_candidates(o, &target.place, kind, Some((additive, target))),
                    "",
                ),
                None => (Err(format!("{t} is not translated yet")), ""),
            },
        }
    }

    // Shared parts.

    /// The operation of an additive or subtractive feature on the Body's
    /// bodies: a new body for the first solid.
    pub(super) fn operation(
        &self,
        target: &Target,
        additive: bool,
    ) -> Result<(Value, Value), String> {
        match (additive, target.current.is_empty()) {
            (true, true) => Ok((json!("new_body"), json!([]))),
            (true, false) => Ok((json!("join"), json!(uids(&target.current)))),
            (false, false) => Ok((json!("cut"), json!(uids(&target.current)))),
            (false, true) => Err("nothing to cut: the Body has no solid yet".to_owned()),
        }
    }

    /// The Body's one body a dress-up works on.
    pub(super) fn body(&self, target: &Target) -> Result<BodyUid, String> {
        match target.current.as_slice() {
            [b] => Ok(*b),
            [] => Err("the Body has no solid yet".to_owned()),
            _ => Err("the Body is several bodies here".to_owned()),
        }
    }

    /// The imported sketch a feature refers to, in the feature's component.
    pub(super) fn sketch_of(
        &self,
        link: &LinkRef,
        component: ComponentUid,
    ) -> Result<SketchMade, String> {
        let made = self
            .replay
            .sketches
            .get(&link.object)
            .cloned()
            .ok_or_else(|| format!("{} is not an imported sketch", link.object))?;
        if made.component != component {
            return Err(format!("{} is in another component", link.object));
        }
        Ok(made)
    }

    /// A feature's profile: its sketch's material regions.
    pub(super) fn profile(
        &self,
        o: &Object,
        property: &str,
        component: ComponentUid,
    ) -> Result<Profile, String> {
        let link = o
            .link(property)
            .ok_or_else(|| format!("no {property}"))?
            .clone();
        let is_sketch = self.sources[0]
            .object(&link.object)
            .is_some_and(|s| super::is_sketch(&s.type_name));
        if !is_sketch {
            return Err(format!(
                "a profile that is not a sketch ({}) is not translated yet",
                link.object
            ));
        }
        if !link.subs.is_empty() {
            return Err("a profile of chosen parts of a sketch is not translated yet".to_owned());
        }
        let sketch = self.sketch_of(&link, component)?;
        let output = self
            .doc
            .sketch_output(sketch.uid)
            .ok_or("the sketch did not solve")?;
        let depths = region_depths(&output.region_info);
        if depths.is_empty() {
            return Err("the sketch has no closed profile".to_owned());
        }
        let regions: Vec<String> = depths
            .iter()
            .filter(|(_, d)| d % 2 == 0)
            .map(|(k, _)| k.clone())
            .collect();
        let outer: Vec<String> = depths
            .iter()
            .filter(|(_, d)| *d == 0)
            .map(|(k, _)| k.clone())
            .collect();
        Ok(Profile {
            sketch,
            outer: (outer != regions).then_some(outer),
            regions,
        })
    }

    fn vector(&self, o: &Object, property: &str) -> Option<[f64; 3]> {
        match o.value(property)? {
            FcValue::Vector(v) => Some(*v),
            _ => None,
        }
    }

    // Pad and Pocket.

    fn extrude_candidates(
        &mut self,
        o: &Object,
        target: &Target,
        pocket: bool,
    ) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let profile = self.profile(o, "Profile", component)?;
        let (operation, participants) = self.operation(target, !pocket)?;
        let reversed = o.bool("Reversed").unwrap_or(false);
        let mut flip = reversed ^ pocket;
        // A custom direction, or one along a reference.
        let custom = o.bool("UseCustomVector") == Some(true)
            || o.link("ReferenceAxis")
                .is_some_and(|l| !l.object.is_empty());
        if custom {
            let d = self.vector(o, "Direction").ok_or("no custom direction")?;
            let n = profile.sketch.frame.normal();
            let d = target.place.transform.apply_vector(d);
            if norm(cross(d, n)) > 1e-9 * norm(d) {
                return self.oblique_candidates(o, &profile, d, pocket, &operation, &participants);
            }
            flip = dot(d, n) < 0.0;
        }
        let taper = self.prop(o, "TaperAngle", Kind::Angle, 0.0);
        let taper2 = self.prop(o, "TaperAngle2", Kind::Angle, 0.0);
        let offset = self.prop(o, "Offset", Kind::Length, 0.0);
        let mode = self.side_mode(o);
        let kind = self
            .enum_text(o, "Type")
            .unwrap_or_else(|| "Length".to_owned());
        let kind = kind.trim_start_matches('?').to_owned();
        let mut out = Vec::new();
        // FreeCAD's conventions where they are uncertain: the direction,
        // the taper's and the offset's signs.
        let mut variants = vec![(flip, 1.0, 1.0)];
        if taper.value != 0.0 || taper2.value != 0.0 {
            variants.push((flip, -1.0, 1.0));
        }
        if offset.value != 0.0 {
            variants.push((flip, 1.0, -1.0));
        }
        variants.push((!flip, 1.0, 1.0));
        let regions: Vec<Vec<String>> = std::iter::once(profile.regions.clone())
            .chain(profile.outer.clone())
            .collect();
        for regions in &regions {
            for (flip, taper_sign, offset_sign) in &variants {
                let extent = self.extent(
                    o,
                    target,
                    &mode,
                    &kind,
                    &taper.signed(*taper_sign),
                    &taper2.signed(*taper_sign),
                    &offset.signed(*offset_sign),
                )?;
                let profiles: Vec<Value> = regions
                    .iter()
                    .map(|r| json!({"sketch": profile.sketch.uid.to_string(), "region": r}))
                    .collect();
                let mut def = json!({"type": "extrude", "profiles": profiles, "extent": extent,
                                     "flip": flip, "operation": operation});
                if participants.as_array().is_some_and(|p| !p.is_empty()) {
                    def["participants"] = participants.clone();
                }
                out.push(Candidate::new(def));
            }
        }
        Ok(out)
    }

    /// A Pad or Pocket along a direction off the sketch's normal: its
    /// profile swept along a fixed line through the sketch's origin without
    /// turning, the length along the direction, or along the normal with
    /// `AlongSketchNormal`. One length, two, or symmetric; no taper.
    fn oblique_candidates(
        &mut self,
        o: &Object,
        profile: &Profile,
        direction: [f64; 3],
        pocket: bool,
        operation: &Value,
        participants: &Value,
    ) -> Result<Vec<Candidate>, String> {
        let d = unit(direction).ok_or("no custom direction")?;
        let n = profile.sketch.frame.normal();
        let cos = dot(d, n).abs();
        if cos < 1e-9 {
            return Err("a direction in the sketch's plane".to_owned());
        }
        for taper in ["TaperAngle", "TaperAngle2"] {
            if o.f64(taper).is_some_and(|t| t != 0.0) {
                return Err("a taper along a custom direction is not translated".to_owned());
            }
        }
        let along_normal = o.bool("AlongSketchNormal").unwrap_or(true);
        let travel = |length: f64| -> [f64; 3] {
            scale(d, if along_normal { length / cos } else { length })
        };
        let mode = self.side_mode(o);
        let kind = self
            .enum_text(o, "Type")
            .unwrap_or_else(|| "Length".to_owned());
        let kind2 = self
            .enum_text(o, "Type2")
            .unwrap_or_else(|| "Length".to_owned());
        let why = "a sweep along a fixed line: the direction's length is FreeCAD's value";
        let mut length = |name: &str| -> f64 {
            let q = self.prop(o, name, Kind::Length, 0.0);
            self.left_out(o, name, why);
            q.value
        };
        let (forward, backward) = match (mode.as_str(), kind.as_str(), kind2.as_str()) {
            ("One side", "Length", _) => (travel(length("Length")), [0.0; 3]),
            ("Symmetric", "Length" | "TwoLengths", _) => {
                let half = travel(length("Length") / 2.0);
                (half, half)
            }
            ("Two sides", "TwoLengths", _) | ("Two sides", "Length", "Length") => {
                (travel(length("Length")), travel(length("Length2")))
            }
            (mode, kind, _) => {
                return Err(format!(
                    "a {mode} {kind} extent along a custom direction is not translated"
                ));
            }
        };
        let origin = profile.sketch.frame.origin;
        let regions: Vec<Vec<String>> = std::iter::once(profile.regions.clone())
            .chain(profile.outer.clone())
            .collect();
        // Along the direction, or against it (a pocket's into the
        // material, as Reversed says).
        let reversed = o.bool("Reversed").unwrap_or(false);
        let first = if pocket ^ reversed { -1.0 } else { 1.0 };
        let mut out = Vec::new();
        for regions in &regions {
            for sign in [first, -first] {
                let profiles: Vec<Value> = regions
                    .iter()
                    .map(|r| json!({"sketch": profile.sketch.uid.to_string(), "region": r}))
                    .collect();
                let start = sub(origin, scale(backward, sign));
                let end = add(origin, scale(forward, sign));
                let mut def = json!({"type": "sweep", "profiles": profiles,
                    "path": {"start": start, "end": end}, "orientation": "parallel",
                    "operation": operation});
                if participants.as_array().is_some_and(|p| !p.is_empty()) {
                    def["participants"] = participants.clone();
                }
                out.push(
                    Candidate::new(def)
                        .note("along FreeCAD's custom direction: a sweep along a fixed line"),
                );
            }
        }
        Ok(out)
    }

    /// One side, two sides or symmetric (1.1's SideType, else Midplane and
    /// the TwoLengths type).
    fn side_mode(&self, o: &Object) -> String {
        if let Some(text) = self.enum_text(o, "SideType") {
            return text;
        }
        if o.bool("Midplane") == Some(true) {
            return "Symmetric".to_owned();
        }
        match self.enum_text(o, "Type").as_deref() {
            Some("TwoLengths") => "Two sides".to_owned(),
            _ => "One side".to_owned(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn extent(
        &mut self,
        o: &Object,
        target: &Target,
        mode: &str,
        kind: &str,
        taper: &Q,
        taper2: &Q,
        offset: &Q,
    ) -> Result<Value, String> {
        let length = self.prop(o, "Length", Kind::Length, 0.0);
        let with_taper = |mut v: Value, t: &Q| {
            if t.value != 0.0 {
                v["taper"] = t.json();
            }
            v
        };
        match mode {
            "Symmetric" => match kind {
                "Length" | "TwoLengths" => Ok(with_taper(
                    json!({"type": "symmetric", "distance": length.json(), "full_length": true}),
                    taper,
                )),
                "ThroughAll" => Ok(json!({"type": "through_all", "both_sides": true})),
                other => Err(format!("a symmetric {other} extent is not translated yet")),
            },
            "Two sides" => {
                let side1 = if kind == "TwoLengths" {
                    json!({"type": "distance", "distance": length.json()})
                } else {
                    self.side(o, target, kind, &length, offset, "UpToFace")?
                };
                let kind2 = self
                    .enum_text(o, "Type2")
                    .map(|t| t.trim_start_matches('?').to_owned())
                    .unwrap_or_else(|| "Length".to_owned());
                let length2 = self.prop(o, "Length2", Kind::Length, 0.0);
                let offset2 = self.prop(o, "Offset2", Kind::Length, 0.0);
                let side2 = if kind2 == "TwoLengths" {
                    json!({"type": "distance", "distance": length2.json()})
                } else {
                    self.side(o, target, &kind2, &length2, &offset2, "UpToFace2")?
                };
                Ok(
                    json!({"type": "two_sides", "side1": with_taper(side1, taper),
                          "side2": with_taper(side2, taper2)}),
                )
            }
            _ => {
                let side = self.side(o, target, kind, &length, offset, "UpToFace")?;
                let side = match side["type"].as_str() {
                    Some("distance") => side,
                    Some("through_all") => json!({"type": "through_all", "both_sides": false}),
                    _ => side,
                };
                Ok(with_taper(side, taper))
            }
        }
    }

    /// One side's extent.
    fn side(
        &mut self,
        o: &Object,
        target: &Target,
        kind: &str,
        length: &Q,
        offset: &Q,
        face_property: &str,
    ) -> Result<Value, String> {
        let body = |t: &Target| -> Result<String, String> {
            t.current
                .first()
                .map(ToString::to_string)
                .ok_or_else(|| "up to the body: the Body has no solid yet".to_owned())
        };
        let offset = offset.json();
        match kind {
            "Length" => Ok(json!({"type": "distance", "distance": length.json()})),
            "ThroughAll" => Ok(json!({"type": "through_all"})),
            "UpToFirst" => Ok(json!({"type": "to_object", "offset": offset,
                "object": {"type": "body", "body": body(target)?, "through": false}})),
            "UpToLast" => Ok(json!({"type": "to_object", "offset": offset,
                "object": {"type": "body", "body": body(target)?, "through": true}})),
            "UpToFace" => {
                let object = self.up_to(o, target, face_property)?;
                Ok(json!({"type": "to_object", "object": object, "offset": offset}))
            }
            "UpToShape" => {
                let object = self.up_to(o, target, &face_property.replace("Face", "Shape"))?;
                Ok(json!({"type": "to_object", "object": object, "offset": offset}))
            }
            other => Err(format!("a {other} extent is not translated yet")),
        }
    }

    /// The object of an extent up to a face (`UpToFace`) or a shape
    /// (`UpToShape`: one face, a plane, or a whole body's shape).
    fn up_to(&mut self, o: &Object, target: &Target, property: &str) -> Result<Value, String> {
        let links: Vec<LinkRef> = o
            .links(property)
            .iter()
            .filter(|l| !l.object.is_empty())
            .cloned()
            .collect();
        // Up to no shape in particular: up to the Body's shape.
        if links.is_empty() && property.starts_with("UpToShape") {
            let body = target
                .current
                .first()
                .ok_or("up to the body: the Body has no solid yet")?;
            return Ok(json!({"type": "body", "body": body.to_string(), "through": false}));
        }
        let [link] = links.as_slice() else {
            return Err(format!(
                "up to {} shapes is not translated",
                if links.is_empty() { "no" } else { "several" }
            ));
        };
        if link.subs.len() > 1 {
            return Err("up to several faces is not translated".to_owned());
        }
        let shape = self.sources[0]
            .object(&link.object)
            .ok_or_else(|| format!("no object {}", link.object))?;
        // A whole object that is not a plane: its body.
        let plane = shape.type_name.ends_with("Plane") || super::is_sketch(&shape.type_name);
        if link.subs.is_empty() && !plane {
            let body = self
                .body_of_object(&link.object)
                .ok_or_else(|| format!("{} has no body here", link.object))?;
            if self.doc.body_component(body) != Some(target.place.component) {
                return Err(format!("{} is in another component", link.object));
            }
            return Ok(json!({"type": "body", "body": body.to_string(), "through": false}));
        }
        Ok(match self.plane_ref(link, target.place.component)? {
            Value::Object(map) if map.contains_key("face") => {
                let mut face = Value::Object(map);
                face["type"] = json!("face");
                face
            }
            plane => json!({"type": "plane", "plane": plane}),
        })
    }

    // Revolution and Groove.

    fn revolve_candidates(
        &mut self,
        o: &Object,
        target: &Target,
        groove: bool,
    ) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let profile = self.profile(o, "Profile", component)?;
        let (operation, participants) = self.operation(target, !groove)?;
        let link = o.link("ReferenceAxis").ok_or("no axis")?.clone();
        let axis = self.axis_ref(&link, component)?;
        let angle = self.prop(o, "Angle", Kind::Angle, 360.0);
        let angle2 = self.prop(o, "Angle2", Kind::Angle, 0.0);
        let kind = self
            .enum_text(o, "Type")
            .unwrap_or_else(|| "Angle".to_owned());
        let sign = if o.bool("Reversed") == Some(true) {
            -1.0
        } else {
            1.0
        };
        let extents: Vec<Value> = match kind.as_str() {
            "Angle" if angle.value.to_degrees() >= 360.0 - 1e-9 => vec![json!({"type": "full"})],
            "Angle" if o.bool("Midplane") == Some(true) => {
                vec![json!({"type": "symmetric", "angle": angle.scale(0.5).json()})]
            }
            "Angle" => vec![
                json!({"type": "angle", "angle": angle.signed(sign).json()}),
                json!({"type": "angle", "angle": angle.signed(-sign).json()}),
            ],
            "TwoAngles" => vec![
                json!({"type": "two_sides", "angle1": angle.json(), "angle2": angle2.json()}),
                json!({"type": "two_sides", "angle1": angle2.json(), "angle2": angle.json()}),
            ],
            "UpToFace" | "UpToShape" => {
                let object = self.up_to(o, target, &kind)?;
                vec![json!({"type": "to_object", "object": object})]
            }
            "UpToFirst" => {
                let body = target
                    .current
                    .first()
                    .ok_or("up to the body: the Body has no solid yet")?;
                vec![json!({"type": "to_object",
                            "object": {"type": "body", "body": body.to_string(), "through": false}})]
            }
            other => return Err(format!("a revolution {other} is not translated yet")),
        };
        // Up to an object Mitcad turns the positive way about the axis:
        // FreeCAD's axis (`Base`, `Axis`) both ways too.
        let mut axes = vec![axis];
        if kind.starts_with("UpTo")
            && let (Some(base), Some(direction)) = (self.vector(o, "Base"), self.vector(o, "Axis"))
        {
            let t = &target.place.transform;
            let (base, direction) = (t.apply_point(base), t.apply_vector(direction));
            axes.push(json!({"origin": base, "direction": direction}));
            axes.push(json!({"origin": base, "direction": scale(direction, -1.0)}));
        }
        let regions: Vec<Vec<String>> = std::iter::once(profile.regions.clone())
            .chain(profile.outer.clone())
            .collect();
        let mut out = Vec::new();
        for regions in &regions {
            for axis in &axes {
                for extent in &extents {
                    let profiles: Vec<Value> = regions
                        .iter()
                        .map(|r| json!({"sketch": profile.sketch.uid.to_string(), "region": r}))
                        .collect();
                    let mut def = json!({"type": "revolve", "profiles": profiles, "axis": axis,
                                         "extent": extent, "operation": operation});
                    if participants.as_array().is_some_and(|p| !p.is_empty()) {
                        def["participants"] = participants.clone();
                    }
                    out.push(Candidate::new(def));
                }
            }
        }
        Ok(out)
    }

    // Fillet and Chamfer.

    /// A dress-up's edges and faces (in its body now).
    fn dressed(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<(BodyUid, Vec<String>, Vec<String>), String> {
        let body = self.body(target)?;
        let mut edges = Vec::new();
        let mut faces = Vec::new();
        if o.bool("UseAllEdges") == Some(true) {
            let shape = self.doc.body_shape(body).ok_or("the body is gone")?;
            edges = self
                .doc
                .kernel()
                .edges(shape)
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter_map(|e| e.name)
                .collect();
            return Ok((body, edges, faces));
        }
        let link = o.link("Base").ok_or("no edges")?.clone();
        for sub in &link.subs {
            let name = sub_name(sub);
            let (b, names) = self.element_names(&link.object, name)?;
            if b != body {
                return Err(format!("{}.{name} is on another body", link.object));
            }
            if name.starts_with("Face") {
                faces.extend(names);
            } else if name.starts_with("Edge") {
                edges.extend(names);
            } else {
                return Err(format!("{name}: not an edge or a face"));
            }
        }
        if edges.is_empty() && faces.is_empty() {
            return Err("no edges".to_owned());
        }
        Ok((body, edges, faces))
    }

    fn fillet_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let (body, edges, faces) = self.dressed(o, target)?;
        o.f64("Radius").ok_or("no radius")?;
        let radius = self.prop(o, "Radius", Kind::Length, 0.0);
        let mut set = json!({"size": {"type": "constant", "radius": radius.json()}});
        if !edges.is_empty() {
            set["edges"] = json!(edges);
        }
        if !faces.is_empty() {
            set["faces"] = json!(faces);
        }
        Ok(vec![Candidate::new(
            json!({"type": "fillet", "body": body.to_string(), "sets": [set]}),
        )])
    }

    fn chamfer_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let (body, edges, faces) = self.dressed(o, target)?;
        o.f64("Size").ok_or("no size")?;
        let size = self.prop(o, "Size", Kind::Length, 0.0).json();
        let kind = self
            .enum_text(o, "ChamferType")
            .unwrap_or_else(|| "Equal distance".to_owned());
        let size_def = match kind.as_str() {
            "Equal distance" => json!({"type": "equal_distance", "distance": size}),
            "Two distances" => {
                o.f64("Size2").ok_or("no second size")?;
                json!({"type": "two_distances", "distance1": size,
                       "distance2": self.prop(o, "Size2", Kind::Length, 0.0).json()})
            }
            "Distance and Angle" => {
                o.f64("Angle").ok_or("no angle")?;
                json!({"type": "distance_angle", "distance": size,
                       "angle": self.prop(o, "Angle", Kind::Angle, 0.0).json()})
            }
            other => return Err(format!("a chamfer of type {other} is not translated")),
        };
        let flip = o.bool("FlipDirection").unwrap_or(false);
        let flips: &[bool] = if kind == "Equal distance" {
            &[false]
        } else if flip {
            &[true, false]
        } else {
            &[false, true]
        };
        Ok(flips
            .iter()
            .map(|flip| {
                let mut set = json!({"size": size_def, "flip": flip});
                if !edges.is_empty() {
                    set["edges"] = json!(edges);
                }
                if !faces.is_empty() {
                    set["faces"] = json!(faces);
                }
                Candidate::new(json!({"type": "chamfer", "body": body.to_string(), "sets": [set]}))
            })
            .collect())
    }

    // Hole.

    fn hole_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let link = o.link("Profile").ok_or("no profile")?.clone();
        let sketch = self.sketch_of(&link, component)?;
        let fc_sketch = self.sources[0]
            .object(&link.object)
            .map(fc::Sketch::of)
            .ok_or("no sketch")?;
        // The circles' (and arcs') centres; else the points.
        let mut points: Vec<String> = fc_sketch
            .geometry
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                !g.construction && matches!(g.curve, Curve::Circle { .. } | Curve::Arc { .. })
            })
            .filter_map(|(i, _)| sketch.curves.get(&(i as i32)).map(|id| format!("c{id}")))
            .collect();
        if points.is_empty() {
            points = sketch.points.values().map(|id| format!("p{id}")).collect();
        }
        if points.is_empty() {
            return Err("the profile sketch has no circles or points".to_owned());
        }
        // A tapered hole: FreeCAD's angle is the wall's to the top face.
        let tapered = o.bool("Tapered") == Some(true)
            && o.f64("TaperedAngle")
                .is_some_and(|a| (a - 90.0).abs() > 1e-9);
        let taper = if tapered {
            let angle = self.prop(o, "TaperedAngle", Kind::Angle, 90.0);
            Some(Q::combine(
                &[&angle],
                std::f64::consts::FRAC_PI_2 - angle.value,
                Kind::Angle,
                |t| format!("90 deg - {}", t[0]),
            ))
        } else {
            None
        };
        if tapered && o.bool("DrillForDepth") == Some(true) {
            return Err("a tapered hole drilled for its depth is not translated".to_owned());
        }
        let mut notes = Vec::new();
        // A thread: a thread feature on the walls (the hole keeps FreeCAD's
        // diameter, which is not Mitcad's minor diameter). A modelled one
        // cuts Mitcad's basic profile from that bore, which the check
        // compares with FreeCAD's thread.
        let mut thread = None;
        let modeled = o.bool("ModelThread") == Some(true);
        if o.bool("Threaded") == Some(true)
            && modeled
            && (o.state.invalid || o.state.error.is_some())
        {
            // FreeCAD failed to model it (1.0 does): what the file keeps is
            // its stored shape.
            return Err("FreeCAD did not model its thread (the hole is invalid)".to_owned());
        }
        if o.bool("Threaded") == Some(true) {
            match self.hole_thread(o, modeled) {
                Ok(t) => thread = Some(t),
                Err(why) if modeled => return Err(format!("its modelled thread: {why}")),
                Err(why) => notes.push(format!("its cosmetic thread is left out: {why}")),
            }
        }
        o.f64("Diameter").ok_or("no diameter")?;
        let diameter = self.prop(o, "Diameter", Kind::Length, 0.0);
        let depth = self.prop(o, "Depth", Kind::Length, 0.0);
        let angled = self.enum_text(o, "DrillPoint").as_deref() != Some("Flat");
        let point = self.prop(o, "DrillPointAngle", Kind::Angle, 118.0);
        let depth_value = depth.value;
        let extent = match self
            .enum_text(o, "DepthType")
            .as_deref()
            .unwrap_or("Dimension")
        {
            "Dimension" => {
                // DrillForDepth: the depth includes the point.
                let depth = if angled && o.bool("DrillForDepth") == Some(true) {
                    Q::combine(
                        &[&depth, &diameter, &point],
                        depth.value - diameter.value / 2.0 / (point.value / 2.0).tan(),
                        Kind::Length,
                        |t| format!("{} - {} / 2 / tan({} / 2)", t[0], t[1], t[2]),
                    )
                } else {
                    depth
                };
                json!({"type": "distance", "depth": depth.json()})
            }
            "ThroughAll" => json!({"type": "through_all"}),
            other => return Err(format!("a hole {other} is not translated yet")),
        };
        let cut = self
            .enum_text(o, "HoleCutType")
            .unwrap_or_else(|| "None".to_owned());
        let kind = match cut.as_str() {
            "None" => json!({"type": "simple"}),
            "Counterbore" => {
                json!({"type": "counterbore",
                       "diameter": self.prop(o, "HoleCutDiameter", Kind::Length, 0.0).json(),
                       "depth": self.prop(o, "HoleCutDepth", Kind::Length, 0.0).json()})
            }
            "Countersink" => json!({"type": "countersink",
                "diameter": self.prop(o, "HoleCutDiameter", Kind::Length, 0.0).json(),
                "angle": self.prop(o, "HoleCutCountersinkAngle", Kind::Angle, 90.0).json()}),
            "Counterdrill" => json!({"type": "counterdrill",
                "diameter": self.prop(o, "HoleCutDiameter", Kind::Length, 0.0).json(),
                "depth": self.prop(o, "HoleCutDepth", Kind::Length, 0.0).json(),
                "angle": self.prop(o, "HoleCutCountersinkAngle", Kind::Angle, 90.0).json()}),
            other => return Err(format!("a hole cut {other} is not translated yet")),
        };
        // FreeCAD's drill point of a tapered hole is as high as the
        // straight hole's: from the wall's end that is a wider point.
        let point = match &taper {
            Some(t) if angled && extent["type"] == "distance" => {
                let r = diameter.value / 2.0;
                let bottom = r - depth_value * t.value.tan();
                let high = r / (point.value / 2.0).tan();
                if !(bottom > 0.0 && high > 0.0) {
                    return Err("the taper closes the hole".to_owned());
                }
                Q::number(2.0 * (bottom / high).atan(), Kind::Angle)
            }
            _ => point,
        };
        let diameter = diameter.json();
        let point = point.json();
        let current = self.body(target)?;
        let reversed = o.bool("Reversed").unwrap_or(false);
        let mut out = Vec::new();
        for flip in [reversed, !reversed] {
            let mut def = json!({"type": "hole",
                "placement": {"type": "sketch_points", "sketch": sketch.uid.to_string(), "points": points},
                "diameter": diameter, "kind": kind, "extent": extent, "flip": flip,
                "participants": [current.to_string()]});
            if angled {
                def["tip_angle"] = point.clone();
            } else {
                def["flat"] = json!(true);
            }
            if let Some(t) = &taper {
                def["taper"] = t.json();
            }
            let mut candidate = Candidate::new(def);
            if let Some(thread) = &thread {
                let faces: Vec<Value> = (0..points.len())
                    .map(|i| json!({"body": current.to_string(), "face": format!("$0:hole{i}.wall")}))
                    .collect();
                candidate = candidate.then(
                    json!({"type": "thread", "faces": faces, "thread": thread, "modeled": modeled}),
                    " (thread)",
                );
            }
            for n in &notes {
                candidate = candidate.partial(n.clone());
            }
            out.push(candidate);
        }
        Ok(out)
    }

    /// A hole's thread as a Mitcad thread definition (ISO metric, a Unified
    /// UNC, UNF or UNEF, a Whitworth BSW, BSF or pipe (BSP, Mitcad's `G`)
    /// or an NPT size Mitcad's table has): its size from the version's list
    /// for its thread type, its class and hand. The whole wall is threaded
    /// (a thread depth of the hole's). `modeled`: one Mitcad can cut.
    fn hole_thread(&self, o: &Object, modeled: bool) -> Result<Value, String> {
        use mitcad_model::features::thread_table::{self, ThreadStandard};
        let kind = self.enum_text(o, "ThreadType").ok_or("no thread type")?;
        let standard = match kind.as_str() {
            "ISOMetricProfile" | "ISOMetricFineProfile" => ThreadStandard::IsoMetric,
            "UNC" | "UNF" | "UNEF" => ThreadStandard::Unified,
            "BSW" | "BSF" | "BSP" => ThreadStandard::Whitworth,
            "NPT" => ThreadStandard::Npt,
            _ => {
                return Err(format!(
                    "a {kind} thread (Mitcad has ISO metric, Unified, Whitworth and NPT ones)"
                ));
            }
        };
        if modeled {
            standard.check_modeled()?;
        }
        let version = self.sources[0].file.document.version();
        let listed = |property: &str| -> Option<String> {
            let FcValue::Enumeration(e) = o.value(property)? else {
                return None;
            };
            let index = usize::try_from(e.index).ok()?;
            match &e.custom {
                Some(list) => list.get(index).cloned(),
                None => mitcad_freecad::version::enum_values(
                    &version,
                    "PartDesign::Hole",
                    &format!("{property}[{kind}]"),
                )?
                .get(index)
                .cloned(),
            }
        };
        let size = listed("ThreadSize").ok_or("its size is not known")?;
        let depth_type = self
            .enum_text(o, "ThreadDepthType")
            .unwrap_or_else(|| "Hole Depth".to_owned());
        let whole = depth_type == "Hole Depth"
            || (depth_type == "Dimension"
                && o.f64("ThreadDepth").unwrap_or(0.0) >= o.f64("Depth").unwrap_or(0.0) - 1e-9)
            || self.enum_text(o, "DepthType").as_deref() == Some("ThroughAll")
                && depth_type != "Dimension";
        if !whole {
            return Err(format!("a thread depth of {depth_type}"));
        }
        // An inch size: its designation in the series (`1/4-20 UNC`,
        // `1/4-20 BSW`, `1/4-18 NPT`); a pipe thread's `G 1/4`.
        let missing = || format!("{size} {kind} is not in Mitcad's thread table");
        let designation = match (standard, kind.as_str()) {
            (ThreadStandard::IsoMetric, _) => size,
            (ThreadStandard::Whitworth, "BSP") => {
                let g = format!("G {size}");
                thread_table::lookup(standard, &g).map_err(|_| missing())?;
                g
            }
            (ThreadStandard::Npt, _) => thread_table::sizes(standard)
                .into_iter()
                .find(|s| s.size == size)
                .and_then(|s| s.designations.into_iter().next())
                .ok_or_else(missing)?,
            _ => thread_table::sizes(standard)
                .into_iter()
                .find(|s| s.size == size)
                .and_then(|s| {
                    s.designations
                        .into_iter()
                        .find(|d| d.ends_with(&format!(" {kind}")))
                })
                .ok_or_else(missing)?,
        };
        let mut thread = json!({"designation": designation,
            "right_handed": self.enum_text(o, "ThreadDirection").as_deref() != Some("Left")});
        if standard != ThreadStandard::IsoMetric {
            thread["standard"] = json!(standard);
        }
        // A class Mitcad lists for internal threads (FreeCAD may keep the
        // index of a list it had before the thread type was set).
        if let Some(class) = listed("ThreadClass")
            .filter(|c| thread_table::check_class(standard, c, Some(true)).is_ok())
        {
            thread["class"] = json!(class);
        }
        Ok(thread)
    }

    // Transformations.

    /// What a transformation copies: its originals' Mitcad features, or the
    /// whole body.
    fn transformed(&self, o: &Object, target: &Target) -> Result<Value, String> {
        let whole = matches!(
            self.enum_text(o, "TransformMode").as_deref(),
            Some("Transform body" | "Whole shape")
        );
        let originals = o.links("Originals");
        if whole || originals.is_empty() {
            let body = self.body(target)?;
            return Ok(json!({"type": "bodies", "bodies": [body.to_string()]}));
        }
        let mut features = Vec::new();
        for link in originals {
            if !self.replay.parametric.contains(&link.object) {
                return Err(format!("{} was not replayed as a feature", link.object));
            }
            let uid = self
                .replay
                .main
                .get(&link.object)
                .ok_or_else(|| format!("{} made no feature", link.object))?;
            features.push(uid.to_string());
        }
        Ok(json!({"type": "features", "features": features}))
    }

    fn mirrored_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let objects = self.transformed(o, target)?;
        let def = self.mirror_def(o, &objects, target.place.component)?;
        Ok(vec![Candidate::new(def)])
    }

    /// A Mirrored's (or a MultiTransform's mirror's) definition.
    fn mirror_def(
        &mut self,
        o: &Object,
        objects: &Value,
        component: ComponentUid,
    ) -> Result<Value, String> {
        let link = o.link("MirrorPlane").ok_or("no mirror plane")?.clone();
        let plane = self.plane_ref(&link, component)?;
        let mut def = json!({"type": "mirror", "objects": objects, "plane": plane});
        if objects["type"] == "bodies" {
            def["combine"] = json!(true);
        }
        Ok(def)
    }

    /// MultiTransform: its transformations in turn, every copy of one
    /// transformed by the next (FreeCAD's product of them), as Mitcad
    /// features of the originals: one transformation as itself, two linear
    /// patterns as a rectangular pattern of two directions, two mirrors as
    /// both mirrors and the half turn about their planes' intersection;
    /// then, for any sequence of mirrors and linear and polar patterns,
    /// each transformation as a pattern or mirror of the one before
    /// (patterns of patterns). Scaled copies are not translated.
    fn multi_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let objects = self.transformed(o, target)?;
        if objects["type"] != "features" {
            return Err("a MultiTransform of the whole body is not translated".to_owned());
        }
        let component = target.place.component;
        let steps: Vec<Object> = o
            .links("Transformations")
            .iter()
            .filter_map(|l| self.sources[0].object(&l.object).cloned())
            .collect();
        let kinds: Vec<&str> = steps
            .iter()
            .map(|s| s.type_name.trim_start_matches("PartDesign::"))
            .collect();
        if kinds.contains(&"Scaled") {
            return Err(
                "a Scaled transformation: Mitcad's patterns and mirrors copy a feature by rigid \
                 motions and reflections, and it has no scaled copies of a feature (its scale \
                 feature scales whole bodies)"
                    .to_owned(),
            );
        }
        let candidates = |defs: Vec<Value>| defs.into_iter().map(Candidate::new).collect();
        let mut out = Vec::new();
        match kinds.as_slice() {
            [] => return Err("a MultiTransform without transformations".to_owned()),
            ["Mirrored"] => {
                return Ok(candidates(vec![
                    self.mirror_def(&steps[0], &objects, component)?,
                ]));
            }
            ["LinearPattern"] => {
                return Ok(candidates(
                    self.linear_defs(&steps[0], &objects, component)?,
                ));
            }
            ["PolarPattern"] => {
                return Ok(candidates(self.polar_defs(&steps[0], &objects, component)?));
            }
            ["LinearPattern", "LinearPattern"] => {
                let first = self.linear_defs(&steps[0], &objects, component)?;
                let second = self.linear_defs(&steps[1], &objects, component)?;
                for a in &first {
                    for b in &second {
                        // One pattern of two directions only when each is
                        // one way and both are measured alike.
                        if a.get("direction2").is_some()
                            || b.get("direction2").is_some()
                            || a["distance_type"] != b["distance_type"]
                        {
                            continue;
                        }
                        let mut def = a.clone();
                        def["direction2"] = b["direction1"].clone();
                        out.push(Candidate::new(def).note(
                            "a MultiTransform of two linear patterns: a pattern of two directions",
                        ));
                    }
                }
            }
            ["Mirrored", "Mirrored"] => {
                let first = self.mirror_def(&steps[0], &objects, component)?;
                let second = self.mirror_def(&steps[1], &objects, component)?;
                let axis = json!({"type": "construction_axis", "definition": {"type": "two_planes",
                    "plane1": first["plane"], "plane2": second["plane"]}});
                let turn = json!({"type": "circular_pattern", "objects": objects, "axis": "$2",
                    "quantity": 2, "angle": std::f64::consts::TAU, "compute": "identical"});
                out.push(
                    Candidate::several(vec![
                        (first, " (mirror 1)"),
                        (second, " (mirror 2)"),
                        (axis, " (axis)"),
                        (turn, ""),
                    ])
                    .note("a MultiTransform of two mirrors: both mirrors and the half turn about the line where their planes meet"),
                );
            }
            _ => {}
        }
        out.extend(self.chained_candidates(&steps, &objects, component)?);
        Ok(out)
    }

    /// A MultiTransform's transformations as a chain: the first a pattern
    /// or mirror of the originals, each next one of the one before (a
    /// pattern of a pattern repeats the originals at every product of the
    /// elements, as FreeCAD's transformations multiply). The readings of
    /// each step (FreeCAD's direction first) in every combination, the
    /// first 16.
    fn chained_candidates(
        &mut self,
        steps: &[Object],
        objects: &Value,
        component: ComponentUid,
    ) -> Result<Vec<Candidate>, String> {
        const SUFFIXES: [&str; 8] = [
            " (transformation 1)",
            " (transformation 2)",
            " (transformation 3)",
            " (transformation 4)",
            " (transformation 5)",
            " (transformation 6)",
            " (transformation 7)",
            " (transformation 8)",
        ];
        if steps.len() > SUFFIXES.len() + 1 {
            return Err(format!(
                "a MultiTransform of {} transformations",
                steps.len()
            ));
        }
        let mut readings: Vec<Vec<Value>> = Vec::new();
        for (i, step) in steps.iter().enumerate() {
            let what = if i == 0 {
                objects.clone()
            } else {
                json!({"type": "features", "features": [format!("${}", i - 1)]})
            };
            let kind = step.type_name.trim_start_matches("PartDesign::");
            readings.push(match kind {
                "Mirrored" => vec![self.mirror_def(step, &what, component)?],
                "LinearPattern" => self.linear_defs(step, &what, component)?,
                "PolarPattern" => self.polar_defs(step, &what, component)?,
                other => return Err(format!("a {other} transformation is not translated")),
            });
        }
        let note = format!(
            "a MultiTransform of {}: each transformation a pattern or mirror of the one before",
            steps
                .iter()
                .map(|s| s.type_name.trim_start_matches("PartDesign::"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        // Every combination of the steps' readings, the first ones first.
        let mut out = Vec::new();
        let mut pick = vec![0usize; readings.len()];
        'combinations: while out.len() < 16 {
            let last = readings.len() - 1;
            let defs: Vec<(Value, &'static str)> = pick
                .iter()
                .enumerate()
                .map(|(i, &k)| {
                    let suffix = if i == last { "" } else { SUFFIXES[i] };
                    (readings[i][k].clone(), suffix)
                })
                .collect();
            out.push(Candidate::several(defs).note(note.clone()));
            // The next combination (the last step's reading turning
            // fastest).
            for i in (0..readings.len()).rev() {
                pick[i] += 1;
                if pick[i] < readings[i].len() {
                    continue 'combinations;
                }
                pick[i] = 0;
            }
            break;
        }
        Ok(out)
    }

    /// Extent or spacing (1.0's `Mode` "length"/"offset", 1.1's
    /// "Extent"/"Spacing").
    fn spacing(&self, o: &Object, property: &str) -> bool {
        matches!(
            self.enum_text(o, property).as_deref(),
            Some("offset" | "Spacing")
        )
    }

    fn linear_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let objects = self.transformed(o, target)?;
        let defs = self.linear_defs(o, &objects, target.place.component)?;
        Ok(defs.into_iter().map(Candidate::new).collect())
    }

    /// A LinearPattern's definitions: along its direction, and the other
    /// way.
    fn linear_defs(
        &mut self,
        o: &Object,
        objects: &Value,
        component: ComponentUid,
    ) -> Result<Vec<Value>, String> {
        let link = o.link("Direction").ok_or("no direction")?.clone();
        let axis = self.axis_ref(&link, component)?;
        let sign = if o.bool("Reversed") == Some(true) {
            -1.0
        } else {
            1.0
        };
        let quantity = self.prop(o, "Occurrences", Kind::Number, 1.0);
        let spacing = self.spacing(o, "Mode");
        let distance = if spacing {
            self.prop(o, "Offset", Kind::Length, 0.0)
        } else {
            self.prop(o, "Length", Kind::Length, 0.0)
        };
        let mut def = json!({"type": "rectangular_pattern", "objects": objects,
            "direction1": {"axis": axis, "quantity": quantity.json(),
                           "distance": distance.signed(sign).json()},
            "distance_type": if spacing { "spacing" } else { "extent" }, "compute": "identical"});
        // 1.1's second direction.
        if o.i64("Occurrences2").unwrap_or(1) > 1 {
            if self.spacing(o, "Mode2") != spacing {
                return Err("two directions measured differently are not translated".to_owned());
            }
            let link2 = o.link("Direction2").ok_or("no second direction")?.clone();
            let axis2 = self.axis_ref(&link2, component)?;
            let sign2 = if o.bool("Reversed2") == Some(true) {
                -1.0
            } else {
                1.0
            };
            let distance2 = if spacing {
                self.prop(o, "Offset2", Kind::Length, 0.0)
            } else {
                self.prop(o, "Length2", Kind::Length, 0.0)
            };
            let quantity2 = self.prop(o, "Occurrences2", Kind::Number, 1.0);
            def["direction2"] = json!({"axis": axis2, "quantity": quantity2.json(),
                                       "distance": distance2.signed(sign2).json()});
        }
        let mut flipped = def.clone();
        flipped["direction1"]["distance"] = distance.signed(-sign).json();
        Ok(vec![def, flipped])
    }

    fn polar_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let objects = self.transformed(o, target)?;
        let defs = self.polar_defs(o, &objects, target.place.component)?;
        Ok(defs.into_iter().map(Candidate::new).collect())
    }

    /// A PolarPattern's definitions: its way round, and the other.
    fn polar_defs(
        &mut self,
        o: &Object,
        objects: &Value,
        component: ComponentUid,
    ) -> Result<Vec<Value>, String> {
        let link = o.link("Axis").ok_or("no axis")?.clone();
        let axis = self.axis_ref(&link, component)?;
        let quantity = self.prop(o, "Occurrences", Kind::Number, 1.0);
        let angle = if self.spacing(o, "Mode") {
            let step = self.prop(o, "Offset", Kind::Angle, 0.0);
            let count = quantity.value;
            if (step.value.to_degrees() * count - 360.0).abs() < 1e-9 {
                Q::number(std::f64::consts::TAU, Kind::Angle)
            } else {
                Q::combine(
                    &[&step, &quantity],
                    step.value * (count - 1.0),
                    Kind::Angle,
                    |t| format!("{} * ({} - 1)", t[0], t[1]),
                )
            }
        } else {
            self.prop(o, "Angle", Kind::Angle, 360.0)
        };
        let sign = if o.bool("Reversed") == Some(true) {
            -1.0
        } else {
            1.0
        };
        let def = |s: f64| {
            json!({"type": "circular_pattern", "objects": objects, "axis": axis,
                   "quantity": quantity.json(), "angle": angle.signed(s).json(),
                   "compute": "identical"})
        };
        Ok(vec![def(sign), def(-sign)])
    }

    // Boolean.

    fn boolean_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let body = self.body(target)?;
        let operation = match self.enum_text(o, "Type").as_deref().unwrap_or("Fuse") {
            "Fuse" => "join",
            "Cut" => "cut",
            "Common" => "intersect",
            other => return Err(format!("a boolean {other}")),
        };
        let mut tools = Vec::new();
        for link in o.links("Group") {
            let bodies = self
                .replay
                .bodies
                .get(&link.object)
                .cloned()
                .ok_or_else(|| format!("{} was not replayed", link.object))?;
            for b in bodies {
                if self.doc.body_component(b) != Some(target.place.component) {
                    return Err(format!("{} is in another component", link.object));
                }
                tools.push(b.to_string());
            }
        }
        if tools.is_empty() {
            return Err("no bodies to combine with".to_owned());
        }
        Ok(vec![Candidate::new(
            json!({"type": "combine", "target": body.to_string(),
            "tools": tools, "operation": operation, "keep_tools": false}),
        )])
    }

    // Draft and Thickness.

    fn faces_of(&mut self, o: &Object, target: &Target) -> Result<(BodyUid, Vec<String>), String> {
        let body = self.body(target)?;
        let link = o.link("Base").ok_or("no faces")?.clone();
        let mut faces = Vec::new();
        for sub in &link.subs {
            let name = sub_name(sub);
            if !name.starts_with("Face") {
                continue;
            }
            let (b, names) = self.element_names(&link.object, name)?;
            if b != body {
                return Err(format!("{}.{name} is on another body", link.object));
            }
            faces.extend(names);
        }
        Ok((body, faces))
    }

    fn draft_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let (body, faces) = self.faces_of(o, target)?;
        if faces.is_empty() {
            return Err("no faces".to_owned());
        }
        let link = o.link("NeutralPlane").ok_or("no neutral plane")?.clone();
        let plane = self.plane_ref(&link, target.place.component)?;
        let angle = self.prop(o, "Angle", Kind::Angle, 0.0);
        let reversed = o.bool("Reversed").unwrap_or(false);
        let def = |flip: bool, a: &Q| {
            json!({"type": "draft", "body": body.to_string(), "faces": faces, "plane": plane,
                   "angle": a.json(), "flip": flip})
        };
        // Mitcad pulls along the neutral plane's normal: a pull direction
        // along it, either way, is the same draft (the check tells).
        let pulled = |c: Candidate| {
            if o.link("PullDirection")
                .is_some_and(|l| !l.object.is_empty())
            {
                c.note("the pull direction taken as the neutral plane's normal")
            } else {
                c
            }
        };
        let opposite = angle.neg();
        Ok(vec![
            pulled(Candidate::new(def(reversed, &angle))),
            pulled(Candidate::new(def(!reversed, &angle))),
            pulled(Candidate::new(def(reversed, &opposite))),
            pulled(Candidate::new(def(!reversed, &opposite))),
        ])
    }

    fn thickness_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let (body, faces) = self.faces_of(o, target)?;
        // FreeCAD's Pipe and RectoVerso modes thicken a solid as Skin does
        // (the check tells when they do not).
        let mode = self
            .enum_text(o, "Mode")
            .unwrap_or_else(|| "Skin".to_owned());
        if !matches!(mode.as_str(), "Skin" | "Pipe" | "RectoVerso") {
            return Err(format!("a {mode} thickness is not translated yet"));
        }
        o.f64("Value").ok_or("no thickness")?;
        let value = self.prop(o, "Value", Kind::Length, 0.0).json();
        let inward = o.bool("Reversed").unwrap_or(true);
        let arc = self.enum_text(o, "Join").as_deref() != Some("Intersection");
        let def = |inside: bool, rounded: bool| {
            let (i, out) = if inside {
                (value.clone(), json!(0.0))
            } else {
                (json!(0.0), value.clone())
            };
            json!({"type": "shell", "body": body.to_string(), "faces": faces, "inside": i,
                   "outside": out, "rounded": rounded})
        };
        Ok(vec![
            Candidate::new(def(inward, arc && !inward)),
            Candidate::new(def(!inward, arc && inward)),
            Candidate::new(def(inward, !(arc && !inward))),
            Candidate::new(def(!inward, !(arc && inward))),
        ])
    }

    // Loft and Pipe.

    /// A sketch's one region (a loft's or pipe's section).
    fn one_region(
        &self,
        link: &LinkRef,
        component: ComponentUid,
    ) -> Result<(String, String), String> {
        let o = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no {}", link.object))?;
        if !super::is_sketch(&o.type_name) {
            return Err(format!("{} is not a sketch", link.object));
        }
        let profile = self.profile_of_sketch(link, component)?;
        match profile.regions.as_slice() {
            [r] => Ok((profile.sketch.uid.to_string(), r.clone())),
            _ => Err(format!(
                "{} has {} regions",
                link.object,
                profile.regions.len()
            )),
        }
    }

    fn profile_of_sketch(
        &self,
        link: &LinkRef,
        component: ComponentUid,
    ) -> Result<Profile, String> {
        let sketch = self.sketch_of(link, component)?;
        let output = self
            .doc
            .sketch_output(sketch.uid)
            .ok_or("the sketch did not solve")?;
        let depths = region_depths(&output.region_info);
        Ok(Profile {
            sketch,
            regions: depths
                .iter()
                .filter(|(_, d)| d % 2 == 0)
                .map(|(k, _)| k.clone())
                .collect(),
            outer: None,
        })
    }

    fn loft_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let additive = o.type_name.contains("Additive");
        let (operation, participants) = self.operation(target, additive)?;
        let mut links = vec![o.link("Profile").ok_or("no profile")?.clone()];
        links.extend(o.links("Sections").iter().cloned());
        let mut sections = Vec::new();
        for link in &links {
            let (sketch, region) = self.one_region(link, component)?;
            sections.push(json!({"type": "profile", "sketch": sketch, "region": region}));
        }
        let mut def = json!({"type": "loft", "sections": sections,
            "ruled": o.bool("Ruled").unwrap_or(false), "closed": o.bool("Closed").unwrap_or(false),
            "operation": operation});
        if participants.as_array().is_some_and(|p| !p.is_empty()) {
            def["participants"] = participants;
        }
        Ok(vec![Candidate::new(def)])
    }

    fn pipe_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let additive = o.type_name.contains("Additive");
        let (operation, participants) = self.operation(target, additive)?;
        let several = !o.links("Sections").is_empty();
        if several && self.enum_text(o, "Transformation").as_deref() != Some("Multisection") {
            return Err("a pipe's sections without the Multisection transformation".to_owned());
        }
        if o.link("AuxillerySpine").is_some() {
            return Err("an auxiliary spine is not translated yet".to_owned());
        }
        let orientation = match self.enum_text(o, "Mode").as_deref().unwrap_or("Standard") {
            "Standard" | "Frenet" => "perpendicular",
            "Fixed" => "parallel",
            other => return Err(format!("a {other} pipe is not translated yet")),
        };
        let profile = o.link("Profile").ok_or("no profile")?.clone();
        let (sketch, region) = self.one_region(&profile, component)?;
        let spine = o.link("Spine").ok_or("no spine")?.clone();
        let spine_object = self.sources[0]
            .object(&spine.object)
            .cloned()
            .ok_or("no spine")?;
        if !super::is_sketch(&spine_object.type_name) {
            return Err("a spine of edges of a body is not translated yet".to_owned());
        }
        let made = self.sketch_of(&spine, component)?;
        let curves: Vec<String> = if spine.subs.is_empty() {
            let mut all: Vec<(&i32, &u32)> = made.curves.iter().collect();
            all.sort();
            let fc_sketch = fc::Sketch::of(&spine_object);
            all.into_iter()
                .filter(|(g, _)| fc_sketch.geometry(**g).is_some_and(|x| !x.construction))
                .map(|(_, id)| format!("c{id}"))
                .collect()
        } else {
            let mut out = Vec::new();
            for sub in &spine.subs {
                let name = sub_name(sub);
                let geo = self
                    .sketch_geometry(&spine_object, name, component)
                    .ok_or_else(|| format!("{}.{name} was not found", spine.object))?;
                let id = made
                    .curves
                    .get(&geo)
                    .ok_or_else(|| format!("{}.{name} was not imported", spine.object))?;
                out.push(format!("c{id}"));
            }
            out
        };
        let path = json!({"sketch": made.uid.to_string(), "curves": curves});
        let mut def = if several {
            // Several sections: a loft through them along the spine, at
            // right angles to it.
            if orientation != "perpendicular" {
                return Err("a pipe through several sections without turning".to_owned());
            }
            let mut sections = vec![json!({"type": "profile", "sketch": sketch, "region": region})];
            for link in o.links("Sections").to_vec() {
                let (sketch, region) = self.one_region(&link, component)?;
                sections.push(json!({"type": "profile", "sketch": sketch, "region": region}));
            }
            json!({"type": "loft", "sections": sections, "centerline": path,
                   "ruled": false, "closed": false, "operation": operation})
        } else {
            json!({"type": "sweep", "profiles": [{"sketch": sketch, "region": region}],
                   "path": path, "orientation": orientation, "operation": operation})
        };
        if participants.as_array().is_some_and(|p| !p.is_empty()) {
            def["participants"] = participants;
        }
        let candidate = Candidate::new(def);
        Ok(vec![if several {
            candidate.note("a pipe through several sections: a loft along its spine")
        } else {
            candidate
        }])
    }

    // Helix.

    /// AdditiveHelix, SubtractiveHelix → `helix`: the profile turned about
    /// the reference axis as it rises (FreeCAD sweeps it along a helix
    /// whose frame turns with it, a screw motion). The pitch and the turns
    /// as the mode gives them (pitch and height, pitch and turns, height
    /// and turns); a cone's angle, a growth or a subtraction outside the
    /// helix is not translated.
    fn helix_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let additive = o.type_name.contains("Additive");
        // FreeCAD widens a conical or growing helix keeping the profile in
        // planes through the axis; Mitcad's helix is a screw motion, and
        // the geometry kernel's sweeps along a widening helix turn the
        // profile out of those planes (5e-5 to 7e-5 of the volume off
        // FreeCAD's shapes, whose own motion lags 2e-6 to 3e-6 behind).
        let growing = self
            .enum_text(o, "Mode")
            .is_some_and(|m| m.contains("growth"));
        if !growing && o.f64("Angle").is_some_and(|a| a.abs() > 1e-9) {
            return Err(
                "a conical helix: FreeCAD widens it keeping the profile in planes through the \
                 axis, which Mitcad's helix (a screw motion) does not"
                    .to_owned(),
            );
        }
        if growing && o.f64("Growth").is_some_and(|g| g.abs() > 1e-9) {
            return Err(
                "a growing helix: FreeCAD widens it keeping the profile in planes through the \
                 axis, which Mitcad's helix (a screw motion) does not"
                    .to_owned(),
            );
        }
        if !additive && o.bool("Outside") == Some(true) {
            return Err("a helix cut outside is not translated".to_owned());
        }
        let profile = self.profile(o, "Profile", component)?;
        let (operation, participants) = self.operation(target, additive)?;
        let link = o.link("ReferenceAxis").ok_or("no axis")?.clone();
        let axis = self.axis_ref(&link, component)?;
        let mode = self
            .enum_text(o, "Mode")
            .unwrap_or_else(|| "pitch-height-angle".to_owned());
        let pitch = self.prop(o, "Pitch", Kind::Length, 0.0);
        let height = self.prop(o, "Height", Kind::Length, 0.0);
        let turns = self.prop(o, "Turns", Kind::Number, 0.0);
        let ratio = |a: &Q, b: &Q, kind: Kind| {
            Q::combine(&[a, b], a.value / b.value, kind, |t| {
                format!("{} / {}", t[0], t[1])
            })
        };
        let (pitch, revolutions) = match mode.as_str() {
            "pitch-height-angle" => (pitch.clone(), ratio(&height, &pitch, Kind::Number)),
            "pitch-turns-angle" => (pitch, turns),
            "height-turns-angle" | "height-turns-growth" => {
                (ratio(&height, &turns, Kind::Length), turns)
            }
            other => return Err(format!("a helix of mode {other} is not translated")),
        };
        if !(pitch.value > 0.0 && revolutions.value > 0.0) {
            return Err("a helix without pitch or turns".to_owned());
        }
        let left = o.bool("LeftHanded").unwrap_or(false);
        let reversed = o.bool("Reversed").unwrap_or(false);
        let regions: Vec<Vec<String>> = std::iter::once(profile.regions.clone())
            .chain(profile.outer.clone())
            .collect();
        let mut out = Vec::new();
        for regions in &regions {
            // FreeCAD's hand and direction, then the readings where a
            // reversed helix turns the other way or where both differ.
            for (left_handed, flip) in [
                (left, reversed),
                (!left, reversed),
                (left, !reversed),
                (!left, !reversed),
            ] {
                let profiles: Vec<Value> = regions
                    .iter()
                    .map(|r| json!({"sketch": profile.sketch.uid.to_string(), "region": r}))
                    .collect();
                let mut def = json!({"type": "helix", "profiles": profiles, "axis": axis,
                    "pitch": pitch.json(), "revolutions": revolutions.json(),
                    "left_handed": left_handed, "flip": flip, "operation": operation});
                if participants.as_array().is_some_and(|p| !p.is_empty()) {
                    def["participants"] = participants.clone();
                }
                out.push(Candidate::new(def));
            }
        }
        Ok(out)
    }

    // Datums.

    /// A datum as construction features to try in turn (the last one a
    /// fixed one, taken as it is).
    pub(super) fn datum_def(
        &mut self,
        o: &Object,
        place: &Placed,
    ) -> Result<Vec<DatumReading>, String> {
        let p = place
            .transform
            .after(&super::transform(&super::placement_of(o)));
        let m = p.linear;
        let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
        let t = o.type_name.as_str();
        let fixed = |def: Value| {
            vec![DatumReading {
                def,
                notes: vec!["fixed at FreeCAD's placement".to_owned()],
                check: None,
                carries: Vec::new(),
            }]
        };
        if t.ends_with("Plane") {
            let mut readings = Vec::new();
            let frame = SketchFrame {
                origin: p.translation,
                x_axis: column(0),
                y_axis: column(1),
            };
            // A turn about its support's x or y axis an expression drives:
            // a plane at that angle to the support (either way round).
            if let Some(tilt) = self.tilt_quantity(o) {
                match self.tilted_defs(o, place.component, &tilt) {
                    Ok(defs) => {
                        let note = format!(
                            "turned from its support by its attachment offset ({})",
                            tilt.1.text.as_deref().unwrap_or_default()
                        );
                        readings.extend(defs.into_iter().map(|def| DatumReading {
                            def,
                            notes: vec![note.clone()],
                            check: Some(frame),
                            carries: vec!["AttachmentOffset.Rotation.Angle".to_owned()],
                        }));
                    }
                    Err(why) => self.left_out(
                        o,
                        "AttachmentOffset.Rotation.Angle",
                        &format!("its turn is left out: {why}"),
                    ),
                }
            }
            if readings.is_empty()
                && let Some(offset) = self.offset_plane(o, place, p.translation, column(2))
            {
                readings.push(DatumReading {
                    def: json!({"type": "construction_plane", "definition": offset}),
                    notes: Vec::new(),
                    check: None,
                    carries: Vec::new(),
                });
            }
            // Offset along its normal by one coordinate of its attachment
            // offset (a side one after a quarter turn), the others moving
            // it within itself.
            if readings.is_empty()
                && let Some((def, axis, note)) =
                    self.side_offset_def(o, &frame, place.component, false)
            {
                let path = format!("AttachmentOffset.Base.{}", ["x", "y", "z"][axis]);
                readings.push(DatumReading {
                    def,
                    notes: vec![note],
                    check: Some(frame),
                    carries: vec![path],
                });
                for (k, name) in ["x", "y", "z"].iter().enumerate() {
                    if k != axis {
                        self.kept_value(
                            o,
                            &format!("AttachmentOffset.Base.{name}"),
                            "it moves the datum plane within itself, which an offset plane does \
                             not keep",
                        );
                    }
                }
            }
            for (path, why) in [
                (
                    "AttachmentOffset.Base.x",
                    "a datum plane's side offset: the plane is fixed at FreeCAD's placement",
                ),
                (
                    "AttachmentOffset.Base.y",
                    "a datum plane's side offset: the plane is fixed at FreeCAD's placement",
                ),
                (
                    "AttachmentOffset.Rotation.Angle",
                    "a datum plane's turn: the plane is fixed at FreeCAD's placement",
                ),
            ] {
                self.kept_value(o, path, why);
            }
            readings.extend(fixed(
                json!({"type": "construction_plane", "definition": {"type": "fixed",
                    "origin": p.translation, "x_axis": column(0), "y_axis": column(1)}}),
            ));
            return Ok(readings);
        }
        if t.ends_with("Line") {
            return Ok(fixed(
                json!({"type": "construction_axis", "definition": {"type": "fixed",
                    "origin": p.translation, "direction": column(2)}}),
            ));
        }
        if t.ends_with("Point") {
            return Ok(fixed(
                json!({"type": "construction_point", "definition": {"type": "fixed",
                    "point": p.translation}}),
            ));
        }
        Err(
            "a coordinate system: Mitcad has none (features that use it take fixed geometry)"
                .to_owned(),
        )
    }

    /// A datum plane attached flat to one origin plane or planar face, only
    /// moved along its normal: an offset plane.
    fn offset_plane(
        &mut self,
        o: &Object,
        place: &Placed,
        origin: [f64; 3],
        normal: [f64; 3],
    ) -> Option<Value> {
        let mode = self.enum_text(o, "MapMode")?;
        if !matches!(mode.as_str(), "FlatFace" | "ObjectXY") {
            return None;
        }
        let support: Vec<LinkRef> = ["AttachmentSupport", "Support"]
            .iter()
            .flat_map(|p| o.links(p).iter().cloned())
            .collect();
        let [link] = support.as_slice() else {
            return None;
        };
        let offset = match o.value("AttachmentOffset") {
            Some(FcValue::Placement(p)) => *p,
            _ => mitcad_freecad::Placement::IDENTITY,
        };
        let q = offset.rotation;
        let turned = q[0].abs() > 1e-12 || q[1].abs() > 1e-12 || q[2].abs() > 1e-12;
        if offset.position[0].abs() > 1e-9 || offset.position[1].abs() > 1e-9 || turned {
            return None;
        }
        let plane = self.plane_ref(link, place.component).ok()?;
        // Mitcad's plane: its point and normal.
        let (base, n) = match &plane {
            Value::String(s) => match s.as_str() {
                "xy" => ([0.0; 3], [0.0, 0.0, 1.0]),
                "xz" => ([0.0; 3], [0.0, 1.0, 0.0]),
                "yz" => ([0.0; 3], [1.0, 0.0, 0.0]),
                _ => return None,
            },
            Value::Object(map) if map.contains_key("face") => {
                let body: BodyUid = map["body"].as_str()?.parse().ok()?;
                let face = map["face"].as_str()?.parse().ok()?;
                let shape = self.doc.body_shape(body)?;
                let p = self.doc.kernel().face_plane(shape, &face).ok()?;
                (p.origin, p.normal)
            }
            _ => return None,
        };
        if norm(cross(normal, n)) > 1e-9 {
            return None;
        }
        let distance = dot(sub(origin, base), n);
        // An offset an expression drives: that expression, along Mitcad's
        // normal.
        let sign = if dot(normal, n) < 0.0 { -1.0 } else { 1.0 };
        let distance = match self.offset_quantity(o) {
            Some(q) if (distance - sign * q.value).abs() <= 1e-9 * distance.abs().max(1.0) => {
                q.signed(sign).json()
            }
            Some(_) => {
                self.left_out(
                    o,
                    "AttachmentOffset.Base.z",
                    "the plane's offset from its support is not the attachment offset",
                );
                json!(distance)
            }
            None => json!(distance),
        };
        Some(json!({"type": "offset", "plane": plane, "distance": distance}))
    }
}

/// Each region's key with its depth: how many other regions' outer loops
/// hold a point inside it (FreeCAD fills the regions inside an even number
/// of wires).
pub(super) fn region_depths(
    regions: &[mitcad_model::sketch::regions::Region],
) -> Vec<(String, usize)> {
    regions
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let inside = interior_point(r);
            let depth = regions
                .iter()
                .enumerate()
                .filter(|(j, other)| {
                    *j != i && inside.is_some_and(|p| in_polygon(&other.outline, p))
                })
                .count();
            (r.profile.key.to_string(), depth)
        })
        .collect()
}

/// A point strictly inside a region (not in its holes): the middle of the
/// first stretch inside it along the line through its centroid.
fn interior_point(r: &mitcad_model::sketch::regions::Region) -> Option<[f64; 2]> {
    let y = r.centroid[1];
    let mut xs = Vec::new();
    for poly in std::iter::once(&r.outline).chain(&r.holes) {
        for k in 0..poly.len() {
            let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
            if (a[1] > y) != (b[1] > y) {
                xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
    }
    xs.sort_by(f64::total_cmp);
    xs.chunks(2)
        .find(|pair| pair.len() == 2 && pair[1] - pair[0] > 1e-9)
        .map(|pair| [(pair[0] + pair[1]) / 2.0, y])
}

/// Whether a point is inside a closed polygon (even-odd).
fn in_polygon(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
        {
            inside = !inside;
        }
    }
    inside
}
