// SPDX-License-Identifier: MIT
//! Surface modelling (SolidCraft's `FeatureKind::Surface*`, `Unstitch` and
//! `ReverseNormal`, `crates/doc/src/eval_surface.rs`) as MitCad's surface
//! features (mitcad#455).
//!
//! - A surface extrude, revolve or sweep is the solid feature with
//!   `surface` set: the walls of its profiles (none when the file gives no
//!   profile selection) and of its `curves`, as the solid one's extent,
//!   angles and path. A surface sweep of curves takes no twist, end scale
//!   or rails in Mitcad: such a sweep is not mapped.
//! - A surface loft's sections are profile sections (one region each) or
//!   curve sections; ruled between two sections, smooth through more, as
//!   SolidCraft lofts sheets. End continuities and crowns beyond G0 are
//!   left out with a note; sections on a body's edges are not mapped.
//! - An offset surface is `offset_surface` of the faces at the picks (all
//!   of the body without picks), its chain kept; the fitting tolerance is
//!   not needed (MitCad offsets exactly). A ruled surface is
//!   `ruled_surface`, an extend `extend_surface`: their picked edges are
//!   the named edges at the picks, or, on a surface body's open boundary
//!   (free edges, which have no names), the opening the picks lie on with
//!   the faces the picked edges bound. An extend up to a plane, a face or
//!   the bounds is not mapped.
//! - A trim, untrim, unstitch or reverse normal is MitCad's own; a mutual
//!   trim, a trim by every curve of a sketch and an untrim of some of a
//!   face's holes are not mapped.
//! - A blend surface is `blend_surface` of the two picked chains, each
//!   end's continuity (up to G2) and bulge; the alignment points are left
//!   out with a note. A boundary patch through three or four sketch curves
//!   is a `boundary_surface` loop, one between two curves a loft of them
//!   (the curves lie in the sketch's plane, so every order holds; a crown
//!   is not mapped). A network is a `boundary_surface` network; one joined
//!   to its body is followed by a `stitch` into the body of its first
//!   curve on edges; continuity on network curves is not mapped. A match
//!   is `match_surface` of one face (none picked: the body's only one), a
//!   fairing `fair_surface`.

use mitcad_model::{ComponentUid, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::kinds::{self as k, Operation};
use serde_json::{Value, json};

use super::features::{extrude_def, revolve_def, sweep_def};
use super::surfaces::Pick;
use super::{Importer, MapResult, Mapped, NotMapped};
use mitcad_model::BodyUid;

impl<K: Kernel> Importer<'_, '_, K> {
    pub(super) fn surface_modelling(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        match f.kind.as_str() {
            "surface_extrude" => self.surface_extrude(f, component),
            "surface_revolve" => self.surface_revolve(f, component),
            "surface_sweep" => self.surface_sweep(f, component),
            "surface_loft" => self.surface_loft(f, component),
            "surface_offset" => self.surface_offset(f, component),
            "surface_ruled" => self.surface_ruled(f, component),
            "surface_extend" => self.surface_extend(f, component),
            "surface_trim" => self.surface_trim(f, component),
            "surface_untrim" => self.surface_untrim(f, component),
            "unstitch" | "reverse_normal" => self.body_faces(f, component),
            "surface_blend" => self.surface_blend(f, component),
            "surface_boundary" => self.surface_boundary(f, component),
            "surface_network" => self.surface_network(f, component),
            "surface_match" => self.surface_match(f, component),
            "surface_fair" => self.surface_fair(f, component),
            other => Err(NotMapped::Unmapped(format!(
                "`{other}` is not a surface feature this import knows"
            ))),
        }
    }

    /// The regions of a sheet's profile selection; none without one.
    fn sheet_profiles(
        &self,
        sketch: u64,
        profiles: Option<&k::ProfileSel>,
    ) -> Result<Vec<Value>, NotMapped> {
        match profiles {
            Some(sel) => Ok(self.profiles(sketch, sel)?),
            None => Ok(Vec::new()),
        }
    }

    /// The `curves` of a surface sweep: the sketch's curves as MitCad's.
    fn sheet_curves(&self, sketch: u64, curves: &[String]) -> Result<Option<Value>, NotMapped> {
        if curves.is_empty() {
            return Ok(None);
        }
        let made = self
            .sketches
            .get(&sketch)
            .ok_or("the sketch did not come in")?;
        let ids = curves
            .iter()
            .map(|c| made.curve(c))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(json!({"sketch": made.uid.to_string(), "curves": ids})))
    }

    /// Makes a solid feature's definition a surface's.
    fn as_surface(
        &self,
        def: &mut Value,
        sketch: u64,
        profiles: &[Value],
        curves: &[String],
    ) -> Result<(), NotMapped> {
        let curves = self.sheet_curves(sketch, curves)?;
        if profiles.is_empty() && curves.is_none() {
            return Err("the surface has no profiles or curves to sweep".into());
        }
        def["surface"] = json!(true);
        if let Some(curves) = curves {
            def["curves"] = curves;
        }
        if let Some(map) = def.as_object_mut() {
            map.remove("participants");
        }
        Ok(())
    }

    fn surface_extrude(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let s: k::SurfaceExtrude = f.typed()?;
        let profiles = self.sheet_profiles(s.sketch, s.profiles.as_ref())?;
        let e = k::Extrude {
            sketch: s.sketch,
            profiles: s.profiles.clone().unwrap_or_default(),
            extent: s.extent.clone(),
            operation: Operation::NewBody,
            targets: Vec::new(),
            thin: None,
        };
        let mut def = extrude_def(self, f, component, &e, Some(profiles.clone()))?;
        self.as_surface(&mut def, s.sketch, &profiles, &s.curves)?;
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    fn surface_revolve(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let s: k::SurfaceRevolve = f.typed()?;
        let profiles = self.sheet_profiles(s.sketch, s.profiles.as_ref())?;
        // SolidCraft turns a sheet's positive angles towards the sketch
        // normal.
        let r = k::Revolve {
            sketch: s.sketch,
            profiles: s.profiles.clone().unwrap_or_default(),
            axis: s.axis.clone(),
            angle: s.angle.clone(),
            operation: Operation::NewBody,
            targets: Vec::new(),
            angle2: s.angle2.clone(),
            symmetric: s.symmetric,
            convention: k::RevolveConvention::SketchNormal,
            to: None,
        };
        let mut def = revolve_def(self, f, component, &r, Some(profiles.clone()))?;
        self.as_surface(&mut def, s.sketch, &profiles, &s.curves)?;
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    fn surface_sweep(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let s: k::SurfaceSweep = f.typed()?;
        if !s.curves.is_empty() && (s.twist.is_some() || s.scale.is_some() || !s.rails.is_empty()) {
            return Err(NotMapped::Unmapped(
                "a surface sweep of curves with a twist, an end scale or rails".to_owned(),
            ));
        }
        let profiles = self.sheet_profiles(s.sketch, s.profiles.as_ref())?;
        let sweep = k::Sweep {
            sketch: s.sketch,
            profiles: s.profiles.clone().unwrap_or_default(),
            path_sketch: s.path_sketch,
            path: s.path.clone(),
            orientation: s.orientation.clone(),
            twist: s.twist.clone(),
            scale: s.scale.clone(),
            rails: s.rails.clone(),
            rail_fit: s.rail_fit.clone(),
            operation: Operation::NewBody,
            targets: Vec::new(),
        };
        let mut notes = Vec::new();
        let mut def = sweep_def(self, f, &sweep, Some(profiles.clone()), &mut notes)?;
        self.as_surface(&mut def, s.sketch, &profiles, &s.curves)?;
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::partial(notes))
    }

    fn surface_loft(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let l: k::SurfaceLoft = f.typed()?;
        let mut notes = Vec::new();
        if l.sections.len() < 2 {
            return Err("a surface loft needs two or more sections".into());
        }
        let mut sections = Vec::new();
        let curves = l.sections.iter().any(|s| !s.curves.is_empty());
        for (i, s) in l.sections.iter().enumerate() {
            if s.body.is_some() || !s.edges.is_empty() {
                return Err(NotMapped::Unmapped(
                    "a surface loft section on a body's edges".to_owned(),
                ));
            }
            match (curves, s.curves.is_empty()) {
                (true, false) if s.profiles.is_none() => {
                    let Some(Value::Object(c)) = self.sheet_curves(s.sketch, &s.curves)? else {
                        return Err(format!("section {} has no curves", i + 1).into());
                    };
                    sections.push(json!({"type": "curves", "sketch": c["sketch"],
                        "curves": c["curves"]}));
                }
                (true, _) => {
                    return Err(NotMapped::Unmapped(
                        "a surface loft mixing profile and curve sections".to_owned(),
                    ));
                }
                (false, _) => {
                    let regions = self
                        .profiles(s.sketch, s.profiles.as_ref().unwrap_or(&k::ProfileSel::All))?;
                    let [region] = regions.as_slice() else {
                        return Err(NotMapped::Failed(format!(
                            "a loft section of {} profiles",
                            regions.len()
                        )));
                    };
                    sections.push(json!({"type": "profile", "sketch": region["sketch"],
                        "region": region["region"]}));
                }
            }
        }
        if let Some(ends) = &l.continuity
            && ends.iter().any(|g| !g.eq_ignore_ascii_case("g0"))
        {
            notes.push(format!(
                "the ends' continuity ({}, {}) is left out: the loft only passes through its \
                 sections",
                ends[0], ends[1]
            ));
        }
        if let Some(crown) = l.crown
            && crown.iter().any(|c| *c != 0.0)
        {
            notes.push(format!(
                "the crown ({}, {}) is left out",
                crown[0], crown[1]
            ));
        }
        let def = json!({"type": "loft", "sections": sections, "ruled": l.sections.len() == 2,
            "surface": true, "operation": "new_body"});
        self.add(&def, Some(&f.name), component)?;
        if notes.is_empty() {
            Ok(Mapped::parametric())
        } else {
            Ok(Mapped::partial(notes))
        }
    }

    /// The edges picked on a body: its named edges at the picks, and the
    /// free edges as openings with the faces the picked ones bound.
    fn picked_edges(
        &self,
        f: &Feature,
        component: ComponentUid,
        body: &str,
        picks: &[[f64; 3]],
    ) -> Result<(BodyUid, Vec<String>, Vec<Value>), NotMapped> {
        self.picked_edges_named(component, body, picks, &f.edge_names)
    }

    /// [`Self::picked_edges`] with the picks' persistent names (one each,
    /// or none).
    fn picked_edges_named(
        &self,
        component: ComponentUid,
        body: &str,
        picks: &[[f64; 3]],
        names: &[String],
    ) -> Result<(BodyUid, Vec<String>, Vec<Value>), NotMapped> {
        let mut uid: Option<BodyUid> = None;
        let mut edges: Vec<String> = Vec::new();
        // Each opening's index with the faces of its picked edges and the
        // picks (each picks the free edge nearest it).
        let mut openings: Vec<(usize, Vec<String>, Vec<[f64; 3]>)> = Vec::new();
        for (i, at) in picks.iter().enumerate() {
            let name = names.get(i).map(String::as_str);
            let (b, pick) = match self.loop_pick(component, Some(body), *at, name)? {
                Pick::Opening(b, index) => {
                    let (_, face) = self.face_at(component, Some(body), *at)?;
                    (b, Err((index, face)))
                }
                Pick::Edge(b, edge) => (b, Ok(edge)),
            };
            if uid.is_some_and(|u| u != b) {
                return Err("the picked edges are not on one body".into());
            }
            uid = Some(b);
            match pick {
                Ok(edge) => {
                    if !edges.contains(&edge) {
                        edges.push(edge);
                    }
                }
                Err((index, face)) => match openings.iter_mut().find(|(o, _, _)| *o == index) {
                    Some((_, faces, points)) => {
                        if !faces.contains(&face) {
                            faces.push(face);
                        }
                        points.push(*at);
                    }
                    None => openings.push((index, vec![face], vec![*at])),
                },
            }
        }
        let uid = uid.ok_or("no edges picked")?;
        let openings = openings
            .into_iter()
            .map(|(index, faces, at)| {
                Ok(json!({"opening": self.opening(uid, index)?, "faces": faces, "at": at}))
            })
            .collect::<Result<Vec<_>, NotMapped>>()?;
        Ok((uid, edges, openings))
    }

    fn surface_offset(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let o: k::SurfaceOffset = f.typed()?;
        let mut body: Option<BodyUid> = match o.body.as_deref().filter(|b| !b.is_empty()) {
            Some(name) => Some(self.body(name)?),
            None => None,
        };
        let mut faces = Vec::new();
        for (i, at) in o.faces.iter().enumerate() {
            let (b, face) = self.face_named(
                component,
                o.body.as_deref(),
                *at,
                f.face_names.get(i).map(String::as_str),
            )?;
            if body.is_some_and(|u| u != b) {
                return Err("the picked faces are not of one body".into());
            }
            body = Some(b);
            if !faces.contains(&face) {
                faces.push(face);
            }
        }
        let body = body.ok_or("an offset surface needs its body or faces")?;
        let mut def = json!({"type": "offset_surface", "body": body.to_string(),
            "distance": self.slot(f, "/distance")?});
        if !faces.is_empty() {
            def["faces"] = json!(faces);
        }
        if o.chain {
            def["chain"] = json!(true);
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    fn surface_ruled(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let r: k::SurfaceRuled = f.typed()?;
        let (body, edges, openings) = self.picked_edges(f, component, &r.body, &r.edges)?;
        let mode = r.mode.as_deref().unwrap_or("normal");
        if !matches!(mode, "normal" | "tangent" | "angle" | "direction") {
            return Err(NotMapped::Unmapped(format!(
                "a ruled surface of mode {mode}"
            )));
        }
        let mut def = json!({"type": "ruled_surface", "body": body.to_string(),
            "distance": self.slot(f, "/distance")?, "mode": mode});
        if !edges.is_empty() {
            def["edges"] = json!(edges);
        }
        if !openings.is_empty() {
            def["openings"] = json!(openings);
        }
        if r.angle.is_some() {
            def["angle"] = json!(self.slot(f, "/angle")?);
        } else if mode == "angle" {
            def["angle"] = json!(self.params.unnamed(self.doc, "0 deg"));
        }
        if let Some(direction) = r.direction {
            def["direction"] = json!(direction);
        }
        if r.flip {
            def["flip"] = json!(true);
        }
        match r.corners.as_deref() {
            None | Some("mitre") => {}
            Some("round") => def["corners"] = json!("round"),
            Some(other) => {
                return Err(NotMapped::Unmapped(format!(
                    "a ruled surface with {other} corners"
                )));
            }
        }
        if let Some(at) = r.face {
            let (b, face) = self.face_at(component, Some(&r.body), at)?;
            if b == body {
                def["face"] = json!(face);
            }
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    fn surface_extend(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let e: k::SurfaceExtend = f.typed()?;
        if e.up_to.is_some() {
            return Err(NotMapped::Unmapped(
                "an extension up to a plane, a face or the surface's bounds".to_owned(),
            ));
        }
        let mode = e.mode.as_deref().unwrap_or("natural");
        if !matches!(mode, "natural" | "tangent" | "curvature") {
            return Err(NotMapped::Unmapped(format!("an extension of mode {mode}")));
        }
        let (body, edges, openings) = self.picked_edges(f, component, &e.body, &e.edges)?;
        let mut def = json!({"type": "extend_surface", "body": body.to_string(),
            "distance": self.slot(f, "/distance")?, "mode": mode});
        if !edges.is_empty() {
            def["edges"] = json!(edges);
        }
        if !openings.is_empty() {
            def["openings"] = json!(openings);
        }
        let mut notes = Vec::new();
        if !e.neighbours {
            notes.push(
                "the extension's sides follow the surface (the file keeps them square to the edge)"
                    .to_owned(),
            );
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::partial(notes))
    }

    fn surface_trim(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let t: k::SurfaceTrim = f.typed()?;
        if t.with.is_some() {
            return Err(NotMapped::Unmapped(
                "a mutual trim of two surfaces".to_owned(),
            ));
        }
        let body = self.body(&t.body)?;
        let mut notes = Vec::new();
        let mut def = json!({"type": "trim_surface", "body": body.to_string()});
        if let Some(curves) = &t.curves {
            let made = self
                .sketches
                .get(&curves.sketch)
                .cloned()
                .ok_or("the trim's sketch did not come in")?;
            if curves.curves.is_empty() {
                return Err(NotMapped::Unmapped(
                    "a trim by every curve of a sketch".to_owned(),
                ));
            }
            let ids = curves
                .curves
                .iter()
                .map(|c| made.curve(c))
                .collect::<Result<Vec<_>, _>>()?;
            def["tool"] = json!({"sketch": made.uid.to_string(), "curves": ids});
            if let Some(direction) = curves.direction {
                def["direction"] = json!(direction);
            }
        } else if let Some(tool) = &t.tool {
            let (b, face) = self.face_at(component, Some(&tool.body), tool.point)?;
            def["tool"] = json!({"body": b.to_string(), "face": face});
        } else {
            def["tool"] = self.feature_plane(f, &t.plane, "/plane", component, &mut notes)?;
        }
        let mut keep = vec![t.keep];
        keep.extend(t.keep_more.iter().copied());
        def["keep"] = json!(keep);
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::partial(notes))
    }

    fn surface_untrim(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let u: k::SurfaceUntrim = f.typed()?;
        let holes = match &u.holes {
            None => true,
            Some(points) if points.is_empty() => false,
            Some(_) => {
                return Err(NotMapped::Unmapped(
                    "an untrim of some of a face's holes (MitCad takes all of them away)"
                        .to_owned(),
                ));
            }
        };
        if !holes && !u.outer {
            return Err("an untrim that restores nothing".into());
        }
        let (body, face) = self.face_named(
            component,
            Some(&u.face.body),
            u.face.point,
            f.face_names.first().map(String::as_str),
        )?;
        let mut def = json!({"type": "untrim_surface", "body": body.to_string(), "face": face});
        if !holes {
            def["holes"] = json!(false);
        }
        if u.outer {
            def["outer"] = json!(true);
        }
        if u.in_place {
            def["in_place"] = json!(true);
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    /// An unstitch or a reverse normal: a body and its faces at the picks.
    fn body_faces(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let b: k::BodyFaces = f.typed()?;
        let body = self.body(&b.body)?;
        let mut faces: Vec<String> = Vec::new();
        for (i, at) in b.faces.iter().enumerate() {
            let (on, face) = self.face_named(
                component,
                Some(&b.body),
                *at,
                f.face_names.get(i).map(String::as_str),
            )?;
            if on != body {
                return Err("a picked face is not of the body".into());
            }
            if !faces.contains(&face) {
                faces.push(face);
            }
        }
        let mut def = json!({"type": f.kind.as_str(), "body": body.to_string()});
        if !faces.is_empty() {
            def["faces"] = json!(faces);
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    /// A chain of edges picked on a body as a MitCad chain (`body`,
    /// `edges`, `openings`).
    fn chain_value(
        &self,
        component: ComponentUid,
        body: &str,
        picks: &[[f64; 3]],
        names: &[String],
    ) -> Result<Value, NotMapped> {
        let (uid, edges, openings) = self.picked_edges_named(component, body, picks, names)?;
        let mut chain = json!({"body": uid.to_string()});
        if !edges.is_empty() {
            chain["edges"] = json!(edges);
        }
        if !openings.is_empty() {
            chain["openings"] = json!(openings);
        }
        Ok(chain)
    }

    fn surface_blend(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let b: k::SurfaceBlend = f.typed()?;
        // The picks' names: the first end's, then the second's.
        let split = b.from.edges.len();
        let (from_names, to_names) = if f.edge_names.len() == split + b.to.edges.len() {
            f.edge_names.split_at(split)
        } else {
            (&[][..], &[][..])
        };
        let mut sides = Vec::new();
        for (i, (side, names)) in [(&b.from, from_names), (&b.to, to_names)]
            .into_iter()
            .enumerate()
        {
            if side.edges.is_empty() {
                return Err("a blend needs a chain of edges at each end".into());
            }
            let mut value = self.chain_value(component, &side.body, &side.edges, names)?;
            value["continuity"] = json!(order(&b.continuity[i])?);
            if b.bulge[i] != 0.0 {
                if b.bulge[i] < -0.9 {
                    return Err(NotMapped::Unmapped(format!(
                        "a blend bulge of {} (MitCad's go down to -0.9)",
                        b.bulge[i]
                    )));
                }
                value["bulge"] = json!(b.bulge[i]);
            }
            sides.push(value);
        }
        let mut notes = Vec::new();
        let mut def = json!({"type": "blend_surface", "from": sides[0], "to": sides[1]});
        if b.align.is_some() {
            notes.push(
                "the alignment points are left out: the blend leaves the faces beside its chains"
                    .to_owned(),
            );
        }
        if b.stitch {
            def["stitch"] = json!(true);
            notes.push(format!(
                "the blend is sewn into {}, which keeps its name and takes in {}",
                b.from.body, b.to.body
            ));
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::partial(notes))
    }

    fn surface_boundary(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let s: k::SurfaceBoundary = f.typed()?;
        let made = self
            .sketches
            .get(&s.sketch)
            .cloned()
            .ok_or("the boundary's sketch did not come in")?;
        let curves = s
            .curves
            .iter()
            .map(|c| Ok(json!({"sketch": made.uid.to_string(), "curves": [made.curve(c)?]})))
            .collect::<Result<Vec<_>, NotMapped>>()?;
        for g in &s.orders {
            order(g)?;
        }
        // The curves lie in the sketch's plane: the patch through three or
        // four of them, or a bridge between two, lies in it as well, so
        // every order holds.
        let def = match curves.len() {
            3 | 4 if s.crown == 0.0 => json!({"type": "boundary_surface", "boundary": curves}),
            2 if s.crown == 0.0 => json!({"type": "boundary_surface", "u": curves}),
            2..=4 => {
                return Err(NotMapped::Unmapped(format!(
                    "a crowned boundary patch (crown {})",
                    s.crown
                )));
            }
            n => return Err(format!("a boundary patch through {n} curves").into()),
        };
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    fn surface_network(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let n: k::SurfaceNetwork = f.typed()?;
        let picks: usize = n.u.iter().chain(&n.v).map(|c| c.edges.len()).sum();
        let named = f.edge_names.len() == picks;
        let mut next = 0;
        let mut target: Option<String> = None;
        let mut sides = [Vec::new(), Vec::new()];
        for (side, curves) in [&n.u, &n.v].into_iter().enumerate() {
            for curve in curves {
                if !curve.continuity.eq_ignore_ascii_case("g0") {
                    return Err(NotMapped::Unmapped(format!(
                        "a network curve meeting its face with {} continuity",
                        curve.continuity
                    )));
                }
                let value = if let Some(point) = curve.point {
                    json!({"point": point})
                } else if let Some(body) = &curve.body {
                    let names = if named {
                        &f.edge_names[next..next + curve.edges.len()]
                    } else {
                        &[][..]
                    };
                    next += curve.edges.len();
                    let chain = self.chain_value(component, body, &curve.edges, names)?;
                    if target.is_none() {
                        target = chain["body"].as_str().map(str::to_owned);
                    }
                    chain
                } else if let Some(sketch) = curve.sketch {
                    self.sheet_curves(sketch, &curve.curves)?
                        .ok_or("a network curve has no curves")?
                } else {
                    return Err("a network curve is neither curves, edges nor a point".into());
                };
                sides[side].push(value);
            }
        }
        let [u, v] = sides;
        let mut def = json!({"type": "boundary_surface", "u": u});
        if !v.is_empty() {
            def["v"] = json!(v);
        }
        match n.operation {
            Operation::NewBody => {
                self.add(&def, Some(&f.name), component)?;
            }
            Operation::Join => {
                // Sewn into the body of its first curve on edges.
                let target = target.ok_or(
                    "a joined boundary surface needs a curve on a body's edges (it is sewn into \
                     that body)",
                )?;
                let made = self.add(&def, Some(&f.name), component)?;
                let tolerance = self.params.unnamed(self.doc, "0.01 mm");
                let stitch = json!({"type": "stitch", "bodies": [target, format!("{made}.b0")],
                    "tolerance": tolerance});
                self.add(&stitch, Some(&format!("{} Join", f.name)), component)?;
            }
            other => {
                return Err(NotMapped::Unmapped(format!(
                    "a boundary surface with the {} operation",
                    other.as_str()
                )));
            }
        }
        Ok(Mapped::parametric())
    }

    fn surface_match(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let m: k::SurfaceMatch = f.typed()?;
        let body = self.body(&m.body)?;
        let mut def = json!({"type": "match_surface", "body": body.to_string(),
            "continuity": order(&m.order)?});
        match m.faces.as_slice() {
            [] => {}
            [at] => {
                let (on, face) = self.face_named(
                    component,
                    Some(&m.body),
                    *at,
                    f.face_names.first().map(String::as_str),
                )?;
                if on != body {
                    return Err("the picked face is not of the body".into());
                }
                def["face"] = json!(face);
            }
            _ => {
                return Err(NotMapped::Unmapped(
                    "a match of several faces (MitCad rebuilds one)".to_owned(),
                ));
            }
        }
        let target = self.chain_value(component, &m.target, &m.edges, &f.edge_names)?;
        def["target"] = target["body"].clone();
        for key in ["edges", "openings"] {
            if let Some(value) = target.get(key) {
                def[key] = value.clone();
            }
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    fn surface_fair(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let s: k::SurfaceFair = f.typed()?;
        if !(s.strength > 0.0 && s.strength <= 1.0) || s.iterations == 0 || s.iterations > 1000 {
            return Err(NotMapped::Unmapped(format!(
                "a fairing of strength {} in {} iterations (MitCad takes more than 0 to 1, in 1 to \
                 1000)",
                s.strength, s.iterations
            )));
        }
        let body = self.body(&s.body)?;
        let mut faces: Vec<String> = Vec::new();
        for (i, at) in s.faces.iter().enumerate() {
            let (on, face) = self.face_named(
                component,
                Some(&s.body),
                *at,
                f.face_names.get(i).map(String::as_str),
            )?;
            if on != body {
                return Err("a picked face is not of the body".into());
            }
            if !faces.contains(&face) {
                faces.push(face);
            }
        }
        let mut def = json!({"type": "fair_surface", "body": body.to_string(),
            "strength": s.strength, "iterations": s.iterations, "keep_edges": s.keep_edges,
            "tolerance": self.slot(f, "/tolerance")?});
        if !faces.is_empty() {
            def["faces"] = json!(faces);
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }
}

/// A continuity of the file (`g0` to `g3`) as MitCad's.
fn order(g: &str) -> Result<&'static str, NotMapped> {
    match g.to_ascii_lowercase().as_str() {
        "g0" => Ok("g0"),
        "g1" => Ok("g1"),
        "g2" => Ok("g2"),
        other => Err(NotMapped::Unmapped(format!(
            "{other} continuity (MitCad's go up to G2)"
        ))),
    }
}
