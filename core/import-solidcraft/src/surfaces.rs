// SPDX-License-Identifier: MIT
//! Patch, Stitch and Thicken (SolidCraft's `FeatureKind::Patch`, `Stitch`
//! and `Thicken`, `crates/doc/src/eval_surface.rs`) as Mitcad's `patch`,
//! `stitch` and `thicken` features (mitcad#456).
//!
//! - A patch's sketch regions are a `patch` of the sketch's profiles (a
//!   surface body each); its loop of edges, picked by points on them, is
//!   the loop of the body's named edges at the picks, or the body's opening
//!   (a loop of free edges, which have no names) the picks lie on, found by
//!   the faces it runs along. The continuity of the whole loop is kept,
//!   single edges' where they have names; the points are kept. The crown
//!   is left out (a loop in a plane is filled by the plane either way).
//! - A stitch is a `stitch` of the bodies, the first keeping its name; a
//!   tolerance of 0 (SolidCraft's own, from the bodies' size) is Mitcad's
//!   own precision.
//! - A thicken is a `thicken` of the bodies with the operation and its
//!   targets; the surfaces stay.

use mitcad_model::{BodyUid, ComponentUid, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::kinds::{self as k, Operation};
use serde_json::{Value, json};

use super::refs::REACH;
use super::{Importer, MapResult, Mapped, NotMapped};
use mitcad_import_base::geom::distance;

/// How far a pick may lie from the points of an opening (mm): the reach
/// of a pick and half the step between the opening's points.
const OPENING_REACH: f64 = REACH + 0.25;

/// What a pick of a patch's loop found.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Pick {
    /// An opening of the body: its index among the kernel's open
    /// boundaries.
    Opening(BodyUid, usize),
    /// A named edge of the body.
    Edge(BodyUid, String),
}

/// A continuity of the file (`g0` to `g3`) as Mitcad's.
fn continuity(g: &str) -> Result<&'static str, NotMapped> {
    match g.to_ascii_lowercase().as_str() {
        "g0" => Ok("g0"),
        "g1" => Ok("g1"),
        "g2" => Ok("g2"),
        other => Err(NotMapped::Unmapped(format!(
            "a patch that meets its faces with {other} continuity"
        ))),
    }
}

impl<K: Kernel> Importer<'_, '_, K> {
    pub(super) fn surface_feature(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        match f.kind.as_str() {
            "patch" => self.patch(f, component),
            "stitch" => self.stitch(f, component),
            "thicken" => self.thicken(f, component),
            other => Err(NotMapped::Unmapped(format!(
                "`{other}` is not a surface feature this import knows"
            ))),
        }
    }

    fn patch(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let p: k::Patch = f.typed()?;
        // Loops of a sketch's 3D curves (format 52), which this import does
        // not bring in.
        if !p.profiles_3d.is_empty() {
            return Err(NotMapped::Unmapped(
                "it patches loops of a sketch's 3D curves, which this import does not bring in"
                    .to_owned(),
            ));
        }
        let mut notes = Vec::new();
        let mut def = json!({"type": "patch"});
        if let Some(sketch) = p.sketch {
            def["profiles"] = json!(self.profiles(sketch, &p.profiles)?);
        }
        if !p.edges.is_empty() {
            def["boundary"] = self.patch_boundary(f, component, &p, &mut notes)?;
        }
        if def.get("profiles").is_none() && def.get("boundary").is_none() {
            return Err("a patch needs a sketch region or a loop of edges".into());
        }
        if p.crown != 0.0 {
            notes.push(format!(
                "the crown ({}) is left out: the fill only passes through the loop and its points \
                 (a loop in a plane is filled by the plane either way)",
                p.crown
            ));
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::partial(notes))
    }

    /// The loop of a patch: the named edges at the picks, or the opening
    /// they lie on.
    fn patch_boundary(
        &self,
        f: &Feature,
        component: ComponentUid,
        p: &k::Patch,
        notes: &mut Vec<String>,
    ) -> Result<Value, NotMapped> {
        let picks: Vec<Pick> = p
            .edges
            .iter()
            .enumerate()
            .map(|(i, at)| {
                self.loop_pick(
                    component,
                    p.body.as_deref(),
                    *at,
                    f.edge_names.get(i).map(String::as_str),
                )
            })
            .collect::<Result<_, _>>()?;
        let g = continuity(&p.continuity.all)?;
        let mut boundary = match &picks[0] {
            Pick::Opening(body, index) => {
                if picks.iter().any(|q| q != &picks[0]) {
                    return Err("the picks of the patch's loop are not on one opening".into());
                }
                json!({"body": body.to_string(), "opening": self.opening(*body, *index)?})
            }
            Pick::Edge(body, _) => {
                let mut edges: Vec<String> = Vec::new();
                for pick in &picks {
                    match pick {
                        Pick::Edge(b, edge) if b == body => {
                            if !edges.contains(edge) {
                                edges.push(edge.clone());
                            }
                        }
                        _ => {
                            return Err(
                                "the picks of the patch's loop are not edges of one body".into()
                            );
                        }
                    }
                }
                json!({"body": body.to_string(), "edges": edges})
            }
        };
        if g != "g0" {
            boundary["continuity"] = json!(g);
        }
        let mut own = Vec::new();
        for set in &p.continuity.edges {
            let g = continuity(&set.g)?;
            match &picks[0] {
                Pick::Edge(body, _) => {
                    let (b, edge) =
                        self.edge_named(component, p.body.as_deref(), set.edge, None)?;
                    if b != *body {
                        return Err("a continuity pick is not on the patch's body".into());
                    }
                    own.push(json!({"edge": edge, "continuity": g}));
                }
                Pick::Opening(..) => {
                    notes.push(format!(
                        "the {g} continuity of the edge at {:?} is left out (the edges of an \
                         opening have no names); the whole loop's is kept",
                        set.edge
                    ));
                }
            }
        }
        if !own.is_empty() {
            boundary["edge_continuity"] = json!(own);
        }
        if !p.points.is_empty() {
            boundary["points"] = json!(p.points);
        }
        Ok(boundary)
    }

    /// What a pick of a loop is on: an opening of a body near it, else a
    /// named edge.
    pub(super) fn loop_pick(
        &self,
        component: ComponentUid,
        body: Option<&str>,
        at: [f64; 3],
        name: Option<&str>,
    ) -> Result<Pick, NotMapped> {
        let bodies = match body.filter(|b| !b.is_empty()) {
            Some(name) => vec![self.body(name)?],
            None => self.component_bodies(component),
        };
        let kernel = self.doc.kernel();
        let mut best: Option<(f64, BodyUid, usize)> = None;
        for uid in bodies {
            let Some(shape) = self.doc.body_shape(uid) else {
                continue;
            };
            let Ok(openings) = kernel.open_boundaries(shape) else {
                continue;
            };
            for (index, opening) in openings.iter().enumerate() {
                let d = opening
                    .points
                    .iter()
                    .map(|q| distance(*q, at))
                    .fold(f64::INFINITY, f64::min);
                if best.as_ref().is_none_or(|(b, ..)| d < *b) {
                    best = Some((d, uid, index));
                }
            }
        }
        if let Some((d, uid, index)) = best
            && d <= OPENING_REACH
        {
            return Ok(Pick::Opening(uid, index));
        }
        let (uid, edge) = self.edge_named(component, body, at, name)?;
        Ok(Pick::Edge(uid, edge))
    }

    /// The `opening` of a patch: the faces the body's open boundary at
    /// `index` runs along, and its place among those along the same faces.
    pub(super) fn opening(&self, body: BodyUid, index: usize) -> Result<Value, NotMapped> {
        let shape = self
            .doc
            .body_shape(body)
            .ok_or_else(|| format!("body {body} has no shape"))?;
        let openings = self
            .doc
            .kernel()
            .open_boundaries(shape)
            .map_err(|e| e.to_string())?;
        let faces = |i: usize| -> Vec<String> {
            let mut names: Vec<String> =
                openings[i].faces.iter().map(ToString::to_string).collect();
            names.sort();
            names.dedup();
            names
        };
        let wanted = faces(index);
        if wanted.is_empty() {
            return Err("the opening of the patch runs along no named face".into());
        }
        let place = (0..index).filter(|i| faces(*i) == wanted).count();
        let mut opening = json!({"faces": wanted});
        if place > 0 {
            opening["index"] = json!(place);
        }
        Ok(opening)
    }

    fn stitch(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let s: k::Stitch = f.typed()?;
        let bodies = self.body_ids(&s.bodies)?;
        if bodies.is_empty() {
            return Err("pick the surfaces to stitch".into());
        }
        let tolerance = match &s.tolerance {
            Some(_) => self.slot(f, "/tolerance")?,
            None => self.params.unnamed(self.doc, "0"),
        };
        let def = json!({"type": "stitch", "bodies": bodies, "tolerance": tolerance});
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }

    fn thicken(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let t: k::Thicken = f.typed()?;
        let bodies = self.body_ids(&t.bodies)?;
        if bodies.is_empty() {
            return Err("pick the surfaces to thicken".into());
        }
        let thickness = self.slot(f, "/thickness")?;
        let participants = if t.operation == Operation::NewBody {
            Vec::new()
        } else {
            self.body_ids(&t.targets)?
        };
        let mut def = json!({"type": "thicken", "bodies": bodies, "thickness": thickness,
            "operation": t.operation.as_str(), "participants": participants});
        if t.symmetric {
            def["symmetric"] = json!(true);
        }
        self.add(&def, Some(&f.name), component)?;
        Ok(Mapped::parametric())
    }
}
