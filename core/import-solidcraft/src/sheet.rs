// SPDX-License-Identifier: MIT
//! Sheet metal (SolidCraft's `crates/doc/src/sheet.rs`) as Mitcad's native
//! sheet features (`commands.md`, *Sheet rules and base Face*, *Folded
//! sheet features*).
//!
//! - A rule (the feature's, the design's active one, else SolidCraft's
//!   default, steel 2.5 mm, K 0.44, bend radius = thickness) is a
//!   `sheet_rules` feature, made once per rule and side; `Thickness` in its
//!   expressions is its thickness. Bend reliefs are rectangular, the
//!   corner relief the rule's shape (a notch is a rectangular relief of
//!   the rule's size).
//! - A base flange is a Face of the sketch's profiles (material along the
//!   sketch normal, or against it with `flip`).
//! - An edge flange is a Flange on the boundary at each pick: its height
//!   is measured to the outer face from the virtual sharp (`outer`), the
//!   bend inside, at the edge (outside) or on the bend line (middle); it
//!   turns towards the side of the sheet the pick is on, the other way
//!   with `flip`. A hem is a Flange of 180 degrees, its inside radius half
//!   its gap (the space between its leg and the face it folds over; a hem
//!   without one takes its rule's `hem_gap`). Picked edges are found by
//!   their saved names where the import translates them (a base flange's
//!   rims, format 62), else at the pick.
//! - A fold is a Fold on the sketch line; unfold and refold are Unfold
//!   and Refold of every bend, the base panel staying.
//! - A contour flange is a Contour Flange of the sketch lines (mitcad#461),
//!   the material right of travel (left with `flip`), the width along the
//!   sketch normal (against it with `reverse`).
//! - A conversion is a Convert to Sheet Metal of the face and edges at its
//!   points, with rules of the thickness the file measures from the body
//!   (a plate's, a shell's walls') where it does. Recognize Sheet Metal
//!   of a plate from a picked face (format 62, as an older design's
//!   conversion of a plate opens) is the same conversion of the plate;
//!   other recognitions, a conversion of several faces, with engravings
//!   or the material outside, and a conversion of a whole body from its
//!   faces are placeholders.
//! - A perforation field is a Perforate of the newest sheet (or its body).

use mitcad_model::{BodyUid, ComponentUid, FeatureUid, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::kinds as k;
use serde_json::{Value, json};

use super::{Importer, MapResult, Mapped, NotMapped};

/// A rule of the file: its expressions (SolidCraft's syntax).
struct Rule {
    name: String,
    thickness: String,
    k_factor: String,
    bend_radius: String,
    relief_width: String,
    relief_depth: String,
    corner_relief: String,
    corner_shape: String,
    /// The space between a hem's leg and the face it folds over (format
    /// 62; older designs' values doubled as they open).
    hem_gap: String,
}

impl<K: Kernel> Importer<'_, '_, K> {
    pub(super) fn sheet_feature(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        match f.kind.as_str() {
            "sheet_base" => self.sheet_base(f, component),
            "sheet_flange" => self.sheet_flange(f, component),
            "sheet_hem" => self.sheet_hem(f, component),
            "sheet_fold" => self.sheet_fold(f, component),
            "sheet_unfold" => self.sheet_unfold(f, component),
            "sheet_contour" => self.sheet_contour(f, component),
            "sheet_perforate" => self.sheet_perforate(f, component),
            "sheet_convert" => self.sheet_convert(f, component),
            // Format 62: Recognize Sheet Metal (an older design's
            // conversion of a plate opens as one), and a conversion of a
            // whole body from its faces.
            "sheet_recognize" => self.sheet_recognize(f, component),
            "sheet_surface_convert" => Err(NotMapped::Unmapped(
                "MitCad has no conversion of a whole body to sheet metal from its faces (every face a panel of the rule's thickness, no fixed face)".to_owned(),
            )),
            // Format 36: a chain of edge flanges along sketch lines drawn
            // across an edge.
            "sheet_path" => Err(NotMapped::Unmapped(
                "MitCad has no path flange (edge flanges along a chain of sketch lines, a bend at each corner)".to_owned(),
            )),
            other => Err(NotMapped::Unmapped(format!(
                "`{other}` is not a sheet feature this import knows"
            ))),
        }
    }

    fn rule(&self, name: Option<&str>) -> Rule {
        rule_of(self.design, name)
    }

    /// The `sheet_rules` feature of a rule and side, made the first time.
    fn sheet_rules(
        &mut self,
        name: Option<&str>,
        back: bool,
        component: ComponentUid,
    ) -> Result<FeatureUid, String> {
        self.sheet_rules_of(name, back, None, component)
    }

    /// [`Self::sheet_rules`], with the thickness a conversion measured in
    /// place of the rule's (`Thickness` in its other values is that one).
    fn sheet_rules_of(
        &mut self,
        name: Option<&str>,
        back: bool,
        thickness: Option<f64>,
        component: ComponentUid,
    ) -> Result<FeatureUid, String> {
        let mut rule = self.rule(name);
        if let Some(t) = thickness {
            rule.thickness = format!("{t} mm");
            rule.name = format!("{} ({t} mm)", rule.name);
        }
        let key = (rule.name.clone(), back);
        if let Some(uid) = self.sheet_rule_features.get(&key) {
            return Ok(*uid);
        }
        let with_t = |e: &str| replace_word(e, "Thickness", &format!("({})", rule.thickness));
        let thickness = self.params.expression(self.doc, &rule.thickness)?;
        let mut slot = |e: &str| -> Result<String, String> {
            let e = self.params.expression(self.doc, &with_t(e))?;
            Ok(self.params.unnamed(self.doc, &e))
        };
        let kind = match rule.corner_shape.as_str() {
            "round" => "round",
            "tear" => "tear",
            _ => "rectangular",
        };
        let def = json!({"type": "sheet_rules",
            "thickness": thickness,
            "bend_radius": slot(&rule.bend_radius)?,
            "allowance": {"type": "k_factor", "k": slot(&rule.k_factor)?},
            "reference_side": if back { "back" } else { "front" },
            "bend_relief": {"type": "rectangular", "width": slot(&rule.relief_width)?, "depth": slot(&rule.relief_depth)?},
            "corner_relief": {"type": kind, "width": slot(&rule.corner_relief)?, "depth": slot(&rule.corner_relief)?},
            // The import replays the file's corners as they are built
            // (mitcad#2342): no automatic corner relief at flange corners.
            "flange_corner_relief": false});
        let name = format!("{}{}", rule.name, if back { " (back)" } else { "" });
        let uid = self.add(&def, Some(&name), component)?;
        self.sheet_rule_features.insert(key, uid);
        Ok(uid)
    }

    fn sheet_base(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let b: k::SheetBase = f.typed()?;
        let rules = self.sheet_rules(b.rule.as_deref(), b.flip, component)?;
        let profiles = self.profiles(b.sketch, &b.profiles)?;
        let def = json!({"type": "sheet_face", "rules": rules.to_string(), "profiles": profiles});
        let uid = self.add(&def, Some(&f.name), component)?;
        for body in self.doc.bodies().iter().filter(|x| x.uid.feature == uid) {
            self.sheet_bases.insert(body.uid, uid);
            self.sheet_body_rules.insert(body.uid, b.rule.clone());
        }
        Ok(Mapped::parametric())
    }

    /// A contour flange: the chain of sketch lines in the file's order, the
    /// material right of travel (left with `flip`), the width along the
    /// sketch's normal (against it with `reverse`).
    fn sheet_contour(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let c: k::SheetContour = f.typed()?;
        let rules = self.sheet_rules(c.rule.as_deref(), false, component)?;
        let s = self
            .sketches
            .get(&c.sketch)
            .cloned()
            .ok_or("the sketch did not come in")?;
        let curves = c
            .curves
            .iter()
            .map(|id| s.curve(id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut def = json!({"type": "sheet_contour", "rules": rules.to_string(),
            "sketch": s.uid.to_string(), "curves": curves, "width": self.slot(f, "/distance")?});
        if c.flip {
            def["side"] = json!("left");
        }
        if c.reverse {
            def["reverse"] = json!(true);
        }
        let uid = self.add(&def, Some(&f.name), component)?;
        for body in self.doc.bodies().iter().filter(|x| x.uid.feature == uid) {
            self.sheet_bases.insert(body.uid, uid);
            self.sheet_body_rules.insert(body.uid, c.rule.clone());
        }
        Ok(Mapped::parametric())
    }

    /// A perforation field: its holes cut through the sheet, laid out on
    /// the developed sheet with the first panel fixed. Without a body it
    /// goes on the newest sheet, as in the file's own evaluation.
    fn sheet_perforate(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let p: k::SheetPerforate = f.typed()?;
        let body = match p.body.as_deref() {
            Some(name) => self.body(name)?,
            None => self
                .component_bodies(component)
                .into_iter()
                .filter_map(|b| self.sheet_bases.get(&b).map(|base| (*base, b)))
                .max()
                .map(|(_, b)| b)
                .ok_or("no sheet metal body")?,
        };
        let pattern = match p.pattern.as_deref() {
            None | Some("staggered") => "staggered",
            Some("square") => "square",
            Some(other) => {
                return Err(NotMapped::Failed(format!("perforation pattern `{other}`")));
            }
        };
        let mut def = json!({"type": "sheet_perforate", "body": body.to_string(),
            "diameter": self.slot(f, "/diameter")?, "pitch": self.slot(f, "/pitch")?,
            "pattern": pattern});
        if p.margin.is_some() {
            def["margin"] = json!(self.slot(f, "/margin")?);
        }
        if let Some(sketch) = p.sketch {
            let selection = p.profiles.as_ref().unwrap_or(&k::ProfileSel::All);
            def["outline"] = json!({"profiles": self.profiles(sketch, selection)?});
        } else if p.profiles.is_some() {
            return Err("a perforation outline's profiles require a sketch".into());
        }
        let anchor = if p.sketch.is_some() {
            "the outline in the developed sheet"
        } else {
            "the developed sheet"
        };
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::partial(vec![format!(
            "the holes are cut through the sheet; the grid starts at the corner of {anchor} in its first panel's axes"
        )]))
    }

    /// A conversion to sheet metal: the face at the fixed point, the edges
    /// at the bend points. The file's conversion measures its thickness: a
    /// plate's is its volume over the face's area, a shell's (the nearest
    /// parallel face behind the fixed face, no further than a quarter of
    /// the face's size) its wall's; any other body takes the rule's. The
    /// rules are made with that thickness.
    fn sheet_convert(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let c: k::SheetConvert = f.typed()?;
        // What format 62 adds to a conversion of picked faces: MitCad's
        // converts the body from one face, the material behind it.
        if !c.faces.is_empty() {
            return Err(NotMapped::Unmapped(
                "it converts several picked faces, where MitCad's Convert to Sheet Metal starts from one face"
                    .to_owned(),
            ));
        }
        if !c.engrave.is_empty() {
            return Err(NotMapped::Unmapped(
                "it keeps faces of recesses as engravings of the panels round them, which MitCad's Convert to Sheet Metal does not take"
                    .to_owned(),
            ));
        }
        if !matches!(c.direction.as_str(), "" | "inside") {
            return Err(NotMapped::Unmapped(format!(
                "its material lies `{}` the faces, where MitCad's Convert to Sheet Metal keeps it behind the face",
                c.direction
            )));
        }
        self.convert_from_face(f, component, &c.body, c.face, &c.bends, c.rule.as_deref())
            .map(|notes| {
                if notes.is_empty() {
                    Mapped::parametric()
                } else {
                    Mapped::partial(notes)
                }
            })
    }

    /// Recognize Sheet Metal (format 62), as an older design's conversion
    /// of a plate opens: the plate from the face it picked is MitCad's
    /// conversion of a plate. A recognition of the body's largest face,
    /// with faces left out or with bend edges has no MitCad counterpart.
    fn sheet_recognize(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let r: k::SheetRecognize = f.typed()?;
        let Some(face) = r.face else {
            return Err(NotMapped::Unmapped(
                "it recognizes sheet metal from the body's largest face, where MitCad's Convert to Sheet Metal needs a picked face"
                    .to_owned(),
            ));
        };
        if !r.skip.is_empty() || !r.bends.is_empty() {
            return Err(NotMapped::Unmapped(
                "it recognizes a body of bent walls as sheet metal (faces left out, bends where cylinders join them), which MitCad has no feature for"
                    .to_owned(),
            ));
        }
        let mut notes =
            self.convert_from_face(f, component, &r.body, face, &[], r.rule.as_deref())?;
        notes.push(
            "a plate read as one sheet from the picked face (SolidCraft from 0.45.0 makes it the rule's thickness, and says so where the plate's differs)"
                .to_owned(),
        );
        Ok(Mapped::partial(notes))
    }

    /// A MitCad Convert to Sheet Metal of `body_name` from the face at
    /// `face_at`, bent at the edges at `bend_points`: the notes it gives.
    fn convert_from_face(
        &mut self,
        f: &Feature,
        component: ComponentUid,
        body_name: &str,
        face_at: [f64; 3],
        bend_points: &[[f64; 3]],
        rule: Option<&str>,
    ) -> Result<Vec<String>, NotMapped> {
        let body = self.body(body_name)?;
        let (on, face) = self.face_at(component, Some(body_name), face_at)?;
        if on != body {
            return Err(NotMapped::Failed(
                "the fixed face is on another body".to_owned(),
            ));
        }
        let mut bends = Vec::new();
        for p in bend_points {
            let (on, edge) = self.edge_at(component, Some(body_name), *p)?;
            if on != body {
                return Err(NotMapped::Failed(format!(
                    "the bend edge at {p:?} is on another body"
                )));
            }
            bends.push(edge);
        }
        let thickness = self.convert_thickness(body, &face, bend_points.is_empty())?;
        let rules = self.sheet_rules_of(rule, false, thickness, component)?;
        let def = json!({"type": "sheet_convert", "rules": rules.to_string(),
            "body": body.to_string(), "face": face, "bends": bends});
        let uid = self.add(&def, Some(&f.name), component)?;
        self.sheet_bases.insert(body, uid);
        self.sheet_body_rules.insert(body, rule.map(str::to_owned));
        let mut notes = Vec::new();
        if let Some(t) = thickness {
            notes.push(format!("the sheet is {t} mm thick, as the body"));
        }
        Ok(notes)
    }

    /// The thickness a conversion takes from the body (see
    /// [`Self::sheet_convert`]), or `None` for the rule's.
    fn convert_thickness(
        &self,
        body: BodyUid,
        face: &str,
        plate: bool,
    ) -> Result<Option<f64>, String> {
        let kernel = self.doc.kernel();
        let shape = self.doc.body_shape(body).ok_or("the body is gone")?;
        let area = kernel
            .faces(shape)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|info| info.names.iter().any(|n| n == face))
            .map(|info| info.area)
            .ok_or_else(|| format!("the body has no face {face}"))?;
        if area.is_nan() || area <= 0.0 {
            return Err(format!("face {face} has no area"));
        }
        let round = |t: f64| (t * 1e9).round() / 1e9;
        if plate {
            let volume = kernel
                .mass_properties(shape)
                .map_err(|e| e.to_string())?
                .volume;
            return Ok(Some(round(volume / area)));
        }
        let Ok(mitcad_model::topo::TopoName::Face(name)) = face.parse() else {
            return Err(format!("{face} is not a face name"));
        };
        let mitcad_model::datum::SurfaceGeometry::Plane { origin, normal } = kernel
            .face_geometry(shape, &name)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let behind = kernel
            .face_geometries(shape)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(|(_, s)| match s {
                mitcad_model::datum::SurfaceGeometry::Plane {
                    origin: o,
                    normal: n,
                } if mitcad_import_base::geom::dot(n, normal) < -1.0 + 1e-9 => Some(
                    mitcad_import_base::geom::dot(mitcad_import_base::geom::sub(origin, o), normal),
                ),
                _ => None,
            })
            .filter(|d| *d > 1e-9)
            .fold(f64::INFINITY, f64::min);
        Ok((behind.is_finite() && behind * behind * 16.0 <= area).then(|| round(behind)))
    }

    /// The sheet a feature works on: its body, else the component's only
    /// sheet.
    fn sheet_body(&self, body: Option<&str>, component: ComponentUid) -> Result<BodyUid, String> {
        if let Some(name) = body {
            return self.body(name);
        }
        let sheets: Vec<BodyUid> = self
            .component_bodies(component)
            .into_iter()
            .filter(|b| self.sheet_bases.contains_key(b))
            .collect();
        match sheets.as_slice() {
            [one] => Ok(*one),
            [] => Err("no sheet metal body".to_owned()),
            _ => Err("several sheet metal bodies: the feature names none".to_owned()),
        }
    }

    /// The boundary of the sheet at an edge pick (and its saved name, the
    /// model's since format 62, where the import translates it), and
    /// whether the pick is on the sheet's back surface.
    fn boundary_at(
        &self,
        body: BodyUid,
        pick: [f64; 3],
        name: Option<&str>,
        component: ComponentUid,
    ) -> Result<(Value, bool), String> {
        let (on, edge) = self.edge_named(component, None, pick, name)?;
        if on != body {
            return Err("the edge is on another body".to_owned());
        }
        // (mitcad#778: asked of the model, without the api.)
        let answer = self
            .doc
            .sheet_reference_json(body, None, Some(edge.as_str()), Some(pick))?;
        let boundary = answer
            .get("boundary")
            .cloned()
            .ok_or_else(|| format!("the edge at {pick:?} is no boundary of a panel"))?;
        Ok((boundary, edge.contains("sheet_back")))
    }

    fn sheet_flange(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let fl: k::SheetFlange = f.typed()?;
        // An edge split into stretches, each with a bend of its own (format
        // 52).
        if fl.edges.iter().any(|e| !e.segments.is_empty()) {
            return Err(NotMapped::Unmapped(
                "an edge of it is split into stretches with bends of their own, which MitCad's flange does not take"
                    .to_owned(),
            ));
        }
        let mut notes = Vec::new();
        let body = self.sheet_body(fl.body.as_deref(), component)?;
        let position = match fl.position.as_str() {
            "outside" => "edge",
            "middle" | "mid" => {
                notes.push(
                    "a bend in the middle is on the bend line (half the allowance)".to_owned(),
                );
                "bend_line"
            }
            // Format 37: the inner virtual sharp on the edge (the flange's
            // inside flush with the edge's face at 90 degrees), and the
            // bend's outside touching the edge's face.
            "material_outside" => "outside",
            "tangent" => "side_tangent",
            "inside" => "inside",
            other => {
                return Err(NotMapped::Unmapped(format!(
                    "a flange at `{other}` is not one of MitCad's positions"
                )));
            }
        };
        // Where the heights are measured from (format 37): the outer
        // virtual sharp (none) or the inner one; past 90 degrees the line
        // touching the bend's outside is not one of MitCad's.
        let length_mode = match fl.height_from.as_deref() {
            None | Some("outer") => "outer",
            Some("inner") => "inner",
            Some(other) => {
                return Err(NotMapped::Unmapped(format!(
                    "its heights are measured from `{other}`, which MitCad's flange lengths do not take"
                )));
            }
        };
        if fl.corner_relief_shape.is_some() || fl.corner_relief.is_some() {
            notes.push("the corner relief is the rule's".to_owned());
        }
        if fl.edges.len() > 1 {
            notes.push(
                "each edge's flange is built on its own (the corners between them are not closed)"
                    .to_owned(),
            );
        }
        let several = fl.edges.len() > 1;
        for (i, e) in fl.edges.iter().enumerate() {
            let (boundary, back) = self.boundary_at(
                body,
                e.point,
                f.edge_names.get(i).map(String::as_str),
                component,
            )?;
            let angle_ptr = if e.angle.is_some() {
                format!("/edges/{i}/angle")
            } else {
                "/angle".to_owned()
            };
            let height_ptr = if e.height.is_some() {
                format!("/edges/{i}/height")
            } else {
                "/height".to_owned()
            };
            let up = back ^ fl.flip;
            let angle = if up {
                self.slot(f, &angle_ptr)?
            } else {
                self.slot_of(f, &angle_ptr, |n| format!("-{n}"))?
            };
            let mut def = json!({"type": "sheet_flange", "body": body.to_string(), "boundary": boundary,
                "angle": angle, "length": {"mode": length_mode, "value": self.slot(f, &height_ptr)?},
                "position": position});
            if fl.radius.is_some() {
                def["radius"] = json!(self.slot(f, "/radius")?);
            }
            let name = if several {
                format!("{} ({})", f.name, i + 1)
            } else {
                f.name.clone()
            };
            self.add(&def, Some(&name), component)?;
        }
        Ok(Mapped::partial(notes))
    }

    fn sheet_hem(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let h: k::SheetHem = f.typed()?;
        let body = self.sheet_body(h.body.as_deref(), component)?;
        // The gap is the space between the hem's leg and the face it folds
        // over, so the bend's inside radius is half of it (format 62; the
        // upgrade doubles an older design's gaps, the inside radius
        // itself before); a hem without a gap of its own takes its rule's
        // `hem_gap`.
        let mut notes = vec![
            "a hem is a 180 degree flange whose inside radius is half the hem's gap".to_owned(),
        ];
        let rule_radius = match h.gap {
            Some(_) => None,
            None => {
                let rule_name = self.sheet_body_rules.get(&body).cloned().flatten();
                let rule = self.rule(rule_name.as_deref());
                let half = self.params.expression(self.doc, &hem_radius(None, &rule))?;
                notes.push(format!(
                    "the gap is the rule's ({}: `{}`)",
                    rule.name, rule.hem_gap
                ));
                Some(half)
            }
        };
        for (i, p) in h.edges.iter().enumerate() {
            let (boundary, back) =
                self.boundary_at(body, *p, f.edge_names.get(i).map(String::as_str), component)?;
            let up = back ^ h.flip;
            let angle = if up { "180 deg" } else { "-180 deg" };
            let angle = self.params.unnamed(self.doc, angle);
            let radius = match &rule_radius {
                Some(half) => self.params.unnamed(self.doc, half),
                None => self.slot_of(f, "/gap", |n| format!("{n} / 2"))?,
            };
            let def = json!({"type": "sheet_flange", "body": body.to_string(), "boundary": boundary,
                "angle": angle, "length": {"mode": "tangent", "value": self.slot(f, "/length")?},
                "position": "edge", "radius": radius});
            let name = if h.edges.len() > 1 {
                format!("{} ({})", f.name, i + 1)
            } else {
                f.name.clone()
            };
            self.add(&def, Some(&name), component)?;
        }
        Ok(Mapped::partial(notes))
    }

    fn sheet_fold(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let fo: k::SheetFold = f.typed()?;
        let mut notes = Vec::new();
        let body = self.sheet_body(fo.body.as_deref(), component)?;
        let base = *self
            .sheet_bases
            .get(&body)
            .ok_or("the body is no sheet of a base flange")?;
        let (Some(sketch), Some(curve)) = (fo.sketch, fo.curve.as_ref()) else {
            return Err(NotMapped::Unmapped(
                "fold along a line given by two points (no sketch line)".to_owned(),
            ));
        };
        let s = self
            .sketches
            .get(&sketch)
            .cloned()
            .ok_or("the sketch did not come in")?;
        let line = s.curve(curve)?;
        // The side that stays: the file's fixed point, else the larger side
        // (the side of the body's centre).
        let i = s.sketch.curve_index(curve).ok_or("no fold line")?;
        let mitcad_solidcraft::sketch::CurveKind::Line { a, b } = s.sketch.curves[i].kind else {
            return Err(NotMapped::Failed(format!("{curve} is not a line")));
        };
        let (pa, pb) = (
            s.frame.point(s.sketch.points[a].pos),
            s.frame.point(s.sketch.points[b].pos),
        );
        let fixed = match fo.fixed {
            Some(p) => p,
            None => {
                let shape = self.doc.body_shape(body).ok_or("the body is gone")?;
                let bbox = self
                    .doc
                    .kernel()
                    .bounding_box(shape)
                    .ok()
                    .flatten()
                    .ok_or("the body has no box")?;
                notes.push("the side that stays is the side of the body's center".to_owned());
                std::array::from_fn(|k| (bbox.min[k] + bbox.max[k]) / 2.0)
            }
        };
        let n = s.frame.normal();
        let d = mitcad_import_base::geom::sub(pb, pa);
        let side = mitcad_import_base::geom::dot(
            mitcad_import_base::geom::cross(d, mitcad_import_base::geom::sub(fixed, pa)),
            n,
        );
        let stationary = if side >= 0.0 { "left" } else { "right" };
        let position = match fo.position.as_str() {
            "centerline" | "centre" | "center" => "bend_line",
            "mould" | "mold" => "outside",
            other => {
                notes.push(format!("a fold at `{other}` is on the bend line"));
                "bend_line"
            }
        };
        let angle = if fo.flip {
            self.slot_of(f, "/angle", |n| format!("-{n}"))?
        } else {
            self.slot(f, "/angle")?
        };
        let mut def = json!({"type": "sheet_fold", "body": body.to_string(),
            "panel": {"feature": base.to_string(), "slot": 0},
            "line": {"sketch": s.uid.to_string(), "curve": line},
            "stationary_side": stationary, "angle": angle, "position": position});
        if fo.radius.is_some() {
            def["radius"] = json!(self.slot(f, "/radius")?);
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::partial(notes))
    }

    fn sheet_unfold(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let u: k::SheetUnfold = f.typed()?;
        let body = self.sheet_body(u.body.as_deref(), component)?;
        if u.refold {
            let unfold = *self
                .sheet_unfolds
                .get(&body)
                .ok_or("a refold without an unfold before it")?;
            let def = json!({"type": "sheet_refold", "body": body.to_string(), "unfold": unfold.to_string()});
            self.add(&def, Some(&f.name), component)?;
            return Ok(Mapped::parametric());
        }
        let base = *self
            .sheet_bases
            .get(&body)
            .ok_or("the body is no sheet of a base flange")?;
        let state = self.doc.sheet_state_json(Some(body));
        let bends: Vec<Value> = state
            .get("bends")
            .and_then(Value::as_array)
            .map(|b| b.iter().filter_map(|x| x.get("id").cloned()).collect())
            .unwrap_or_default();
        if bends.is_empty() {
            return Err(NotMapped::Failed(
                "the sheet has no bends to unfold".to_owned(),
            ));
        }
        let def = json!({"type": "sheet_unfold", "body": body.to_string(),
            "stationary": {"feature": base.to_string(), "slot": 0}, "bends": bends});
        let uid = self.add(&def, Some(&f.name), component)?;
        self.sheet_unfolds.insert(body, uid);
        Ok(Mapped::partial(vec![
            "the bends there are now are unfolded; the base panel stays".to_owned(),
        ]))
    }
}

/// The rule a feature names, else the design's active one, else its first,
/// with SolidCraft's defaults (`sheet.rs`, `SheetRule::steel`) for what it
/// leaves out; the default rule when the design has none.
fn rule_of(design: &mitcad_solidcraft::Design, name: Option<&str>) -> Rule {
    let found = design.sheet_rule(name);
    let found = found.as_ref();
    let text = |value: Option<&k::Expr>, default: &str| {
        value.map_or_else(|| default.to_owned(), |e| e.as_str().to_owned())
    };
    Rule {
        name: found
            .map(|r| r.name.as_str())
            .filter(|n| !n.is_empty())
            .unwrap_or("Steel (mm)")
            .to_owned(),
        thickness: text(found.and_then(|r| r.thickness.as_ref()), "2.5 mm"),
        k_factor: text(found.and_then(|r| r.k_factor.as_ref()), "0.44"),
        bend_radius: text(found.and_then(|r| r.bend_radius.as_ref()), "Thickness"),
        relief_width: text(found.and_then(|r| r.relief_width.as_ref()), "Thickness"),
        relief_depth: text(
            found.and_then(|r| r.relief_depth.as_ref()),
            "Thickness * 0.5",
        ),
        corner_relief: text(
            found.and_then(|r| r.corner_relief.as_ref()),
            "Thickness * 4.0",
        ),
        corner_shape: found
            .and_then(|r| r.corner_relief_shape.clone())
            .unwrap_or_else(|| "notch".to_owned()),
        hem_gap: text(
            found.and_then(|r| r.hem_gap.as_ref()),
            k::SheetRule::DEFAULT_HEM_GAP,
        ),
    }
}

/// A hem's inside radius as an expression of the file's: half its own gap,
/// else half its rule's `hem_gap` (`Thickness` in it the rule's thickness).
/// The gap is the space between the hem's leg and the face it folds over
/// (format 62; the upgrade doubles an older design's values).
fn hem_radius(gap: Option<&str>, rule: &Rule) -> String {
    match gap {
        Some(gap) => format!("({gap}) / 2"),
        None => format!(
            "({}) / 2",
            replace_word(&rule.hem_gap, "Thickness", &format!("({})", rule.thickness))
        ),
    }
}

/// `text` with the whole word `word` replaced.
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let w: Vec<char> = word.chars().collect();
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut i = 0;
    while i < chars.len() {
        let starts = chars[i..].starts_with(&w)
            && (i == 0 || !ident(chars[i - 1]))
            && chars.get(i + w.len()).is_none_or(|c| !ident(*c));
        if starts {
            out.push_str(with);
            i += w.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod hem_tests {
    //! A hem's inside radius (mitcad#2153): half its gap, the space between
    //! its leg and the face it folds over since format 62, whose upgrade
    //! doubles an older design's values; a hem without a gap takes its
    //! rule's `hem_gap`.

    use mitcad_model::Document;
    use mitcad_model::testing::MockKernel;
    use mitcad_solidcraft::SolidcraftFile;
    use mitcad_solidcraft::inputs::Kind;
    use serde_json::{Value, json};

    use super::{hem_radius, rule_of};
    use crate::params::Params;

    /// A sheet of `t` thick under the rule `rule`, a hem with a gap of its
    /// own (`Own`) and one without (`Ruled`), in format `format`.
    fn design(format: u32, rule: Value) -> SolidcraftFile {
        SolidcraftFile::from_value(json!({
        "format": format!("solidcraft/{format}"), "name": "Hems", "units": "mm",
        "params": [{"name": "t", "expr": "2 mm", "unit": "mm"}],
        "sheet": {"rules": [rule], "active": "Plate"},
        "features": [
            {"id": 1, "name": "Own", "type": "sheet_hem", "edges": [[0, 0, 0]],
             "length": "10", "gap": "1 mm"},
            {"id": 2, "name": "Ruled", "type": "sheet_hem", "edges": [[0, 0, 0]],
             "length": "10"}
        ]}))
        .unwrap()
    }

    /// The inside radius of hem `name` of `file`, in mm.
    fn radius(file: &SolidcraftFile, name: &str) -> f64 {
        let d = &file.design;
        let hem = d.features.iter().find(|f| f.name == name).unwrap();
        let gap = hem.def.get("gap").and_then(Value::as_str);
        let expr = hem_radius(gap, &rule_of(d, None));
        let mut doc = Document::new(MockKernel::default());
        Params::new(d)
            .value(&mut doc, &expr, Kind::Length)
            .unwrap_or_else(|e| panic!("{expr}: {e}"))
    }

    #[test]
    fn a_format_57_hem_keeps_the_radius_its_gap_gave_it() {
        // SolidCraft 0.43.0 read a hem's gap as its inside radius.
        let file = design(
            57,
            json!({"name": "Plate", "thickness": "t", "hem_gap": "0.5 mm"}),
        );
        assert!((radius(&file, "Own") - 1.0).abs() < 1e-9);
        // The rule's `hem_gap` likewise.
        assert!((radius(&file, "Ruled") - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_format_57_hem_without_a_gap_takes_the_old_default_of_a_rule_without_one() {
        let file = design(57, json!({"name": "Plate", "thickness": "t"}));
        // Twice the old default, 0.01 mm, halved.
        assert!((radius(&file, "Ruled") - 0.01).abs() < 1e-9);
    }

    #[test]
    fn a_format_63_hem_without_a_gap_takes_half_its_rules_hem_gap() {
        let file = design(
            63,
            json!({"name": "Plate", "thickness": "t", "hem_gap": "Thickness / 2"}),
        );
        assert!(file.upgrades.is_empty());
        // Thickness 2: a gap of 1, an inside radius of 0.5.
        assert!((radius(&file, "Ruled") - 0.5).abs() < 1e-9);
        // Its own gap of 1 mm: an inside radius of 0.5.
        assert!((radius(&file, "Own") - 0.5).abs() < 1e-9);
        // A design without rules: SolidCraft's default rule's 0.01 mm.
        let bare = SolidcraftFile::from_value(json!({"format": "solidcraft/63", "name": "B",
            "params": [], "features": [{"id": 1, "name": "Ruled", "type": "sheet_hem",
            "edges": [[0, 0, 0]], "length": "10"}]}))
        .unwrap();
        assert!((radius(&bare, "Ruled") - 0.005).abs() < 1e-9);
    }
}
