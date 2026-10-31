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
//!   sketch's normal: an `extrude` with that `direction`, tapers included
//!   (mitcad#539).
//! - Revolution, Groove → `revolve` join or cut about the reference axis
//!   (a sketch axis line, a line of the sketch, an origin axis, a datum, an
//!   edge); angle, full turn, Midplane (half each way), two angles, up to
//!   a face, a shape, the first or the last face (through the body,
//!   mitcad#539), a groove through all as a full turn; the sign is tried
//!   both ways. FreeCAD 1.0's groove turns by its angle whatever its type:
//!   that reading too.
//! - Their profile a planar face of a body, or chosen edges of a sketch
//!   ([`super::profiles`], mitcad#539): `profile_face` (by distances or
//!   angles), or the sketch's regions inside the chosen edges.
//! - Fillet, Chamfer → `fillet` and `chamfer` on the referenced edges and
//!   faces (an edge FreeCAD merged by the faces along it); `UseAllEdges` a
//!   set of every edge of the body (`all_edges`, taken when the feature is
//!   computed, mitcad#793); a chamfer's first distance on either face is
//!   tried.
//! - Hole → `hole` at the profile sketch's circles' centres: depth or
//!   through all, flat or angled point, counterbore, countersink,
//!   counterdrill, tapered; an ISO metric, Unified, Whitworth or NPT thread
//!   is a `thread` on the walls (the hole has FreeCAD's diameter), a
//!   modelled one (ISO metric or Unified) cut as Mitcad cuts threads.
//! - Mirrored, LinearPattern, PolarPattern → `mirror`,
//!   `rectangular_pattern`, `circular_pattern` of the original features'
//!   Mitcad features (or of the Body's bodies, `TransformMode` whole
//!   shape, mitcad#1004: the copies joined to the bodies they touch, of a
//!   Body of several bodies to any of them, `combine_any`, mitcad#1006);
//!   MultiTransform → those of
//!   one transformation, of two linear patterns (one pattern of two
//!   directions) or of two mirrors (and a half turn), and for any sequence of mirrors and patterns each
//!   transformation a pattern or mirror of the one before (patterns of
//!   patterns; of the whole body each one of the body with the copies
//!   before); a Scaled transformation the `scale` (and `scale_groups`) of
//!   the one before it, first or alone a `scale_pattern` (mitcad#539);
//!   more runs than that one's elements, several Scaled transformations,
//!   the copies FreeCAD 1.0 and 1.1 leave out and Scaled transformations
//!   of the whole body (about the Body's origin) as in
//!   `chained_candidates` (mitcad#542).
//! - Boolean → `combine` with the other Bodies' bodies (of a Body of
//!   several solids, the bodies they reach: joined into one, or each cut
//!   or intersected, the others of a common removed; mitcad#1004).
//! - Draft → `draft` (a pull direction along the neutral plane's normal),
//!   Thickness → `shell` (Pipe and RectoVerso as Skin), one per body their
//!   faces are on (mitcad#1004), AdditiveLoft →
//!   `loft`, AdditivePipe → `sweep` (with sections: a `loft` along the
//!   spine; the modes Standard, Fixed as the parallel orientation, Frenet,
//!   Auxiliary and Binormal (a constant binormal turned by the pipe's
//!   placement) as the sweep's `frame`, mitcad#539), AdditiveHelix →
//!   `helix`; the additive and
//!   subtractive primitives ([`super::primitives`]).
//! - Datums → construction planes (offset from an origin plane or a face
//!   when FreeCAD attaches them so), axes and points (fixed).
//!
//! The lengths, angles, sizes and counts are the properties' parameters or
//! expressions where FreeCAD's are parametric ([`super::params`]).

use std::borrow::Cow;

use mitcad_freecad::sketch::{self as fc, Curve};
use mitcad_freecad::{LinkRef, Object, Value as FcValue};
use mitcad_model::sketch::geometry::Curve2;
use mitcad_model::{BodyUid, ComponentUid, Kernel, SketchFrame};
use serde_json::{Value, json};

use super::Importer;
use super::elements::sub_name;
use super::history::{Candidate, SketchMade, Target};
use super::params::{Kind, Q};
use super::profiles::Swept;
use super::sketches::Placed;
use mitcad_import_base::geom::{cross, dot, norm, scale, sub, unit};

/// What a fillet or chamfer works on: its body, the edges and faces named,
/// or (`all`, FreeCAD's `UseAllEdges`) every edge of the body.
struct Dressed {
    body: BodyUid,
    edges: Vec<String>,
    faces: Vec<String>,
    all: bool,
}

impl Dressed {
    /// The set's `edges` and `faces`, or `all_edges` (mitcad#793).
    fn select(&self, set: &mut Value) {
        if self.all {
            set["all_edges"] = json!(true);
            return;
        }
        if !self.edges.is_empty() {
            set["edges"] = json!(self.edges);
        }
        if !self.faces.is_empty() {
            set["faces"] = json!(self.faces);
        }
    }
}

fn uids(bodies: &[BodyUid]) -> Vec<String> {
    bodies.iter().map(ToString::to_string).collect()
}

/// A transformation of the Body's whole shape (`objects` its bodies): its
/// copies joined to them (`combine`), as FreeCAD fuses them. FreeCAD fuses
/// a copy with whichever solid of the Body it reaches, so on a Body of
/// several bodies a copy joins every one of them it touches, not only the
/// one it is a copy of (`combine_any`, mitcad#1006).
fn combine_whole(def: &mut Value, objects: &Value) {
    def["combine"] = json!(true);
    if objects["bodies"].as_array().is_some_and(|b| b.len() > 1) {
        def["combine_any"] = json!(true);
    }
}

/// One dress-up's definitions, one per body it works on (mitcad#997): the
/// first named after the object, the others `" (body 2)"` and on.
fn per_body(defs: Vec<Value>) -> Candidate {
    if defs.len() == 1 {
        return Candidate::new(defs.into_iter().next().unwrap_or_default());
    }
    Candidate::several_named(
        defs.into_iter()
            .enumerate()
            .map(|(i, def)| {
                let suffix: Cow<'static, str> = if i == 0 {
                    Cow::Borrowed("")
                } else {
                    Cow::Owned(format!(" (body {})", i + 1))
                };
                (def, suffix)
            })
            .collect(),
    )
}

/// The copies a transformation of a MultiTransform makes, the original's
/// included (1.1's second direction of a linear pattern too).
fn elements_of(step: &Object) -> i64 {
    match step.type_name.trim_start_matches("PartDesign::") {
        "Mirrored" => 2,
        "LinearPattern" => {
            step.i64("Occurrences").unwrap_or(1) * step.i64("Occurrences2").unwrap_or(1).max(1)
        }
        _ => step.i64("Occurrences").unwrap_or(1),
    }
}

/// A scale on a pattern's, mirror's or scale pattern's definition: its
/// `scale` (`scale_groups`, `scale_copies`), or one more of its `scales`
/// when it scales already (mitcad#542).
fn add_scale_rule(def: &mut Value, factor: Value, groups: Option<Value>, copies: Option<i64>) {
    if def.get("scale").is_none() {
        def["scale"] = factor;
        if let Some(groups) = groups {
            def["scale_groups"] = groups;
        }
        if let Some(copies) = copies {
            def["scale_copies"] = json!(copies);
        }
        return;
    }
    let mut rule = json!({"scale": factor});
    if let Some(groups) = groups {
        rule["groups"] = groups;
    }
    if let Some(copies) = copies {
        rule["copies"] = json!(copies);
    }
    match def.get_mut("scales").and_then(Value::as_array_mut) {
        Some(rules) => rules.push(rule),
        None => def["scales"] = json!([rule]),
    }
}

/// The copies FreeCAD 1.0 and 1.1 leave out of a MultiTransform (their
/// stored shapes lack them; 0.21's have them; mitcad#542): once a Scaled
/// transformation follows others, every copy but the original whose
/// factor is 1. Per reading of the chain (`counts` its elements), the
/// numbers of its own copies, j = p + P · e (the first one's element
/// turning fastest): the copies its `skipped_copies` leave out. `first`:
/// a Scaled transformation first (its factor, the first reading's
/// elements its occurrences); `scales`: those after others (factor,
/// occurrences, the readings before them).
fn dropped_copies(
    counts: &[i64],
    first: Option<f64>,
    scales: &[(f64, i64, usize)],
) -> Vec<Vec<i64>> {
    let mut out = vec![Vec::new(); counts.len()];
    let total: i64 = counts.iter().product();
    if scales.is_empty() || total > 100_000 {
        return out;
    }
    let factor = |j: i64| -> f64 {
        let mut f = 1.0;
        if let Some(f0) = first
            && counts[0] > 1
        {
            let e = j % counts[0];
            f *= 1.0 + (f0 - 1.0) * e as f64 / (counts[0] - 1) as f64;
        }
        for &(fs, m, at) in scales {
            let n: i64 = counts[..at.min(counts.len())].iter().product();
            if m < 2 || n < 1 {
                continue;
            }
            let run = (j % n) * m / n;
            f *= 1.0 + (fs - 1.0) * run as f64 / (m - 1) as f64;
        }
        f
    };
    let mut inner = 1;
    for (level, &n) in counts.iter().enumerate() {
        let own = inner * n.max(1);
        out[level] = (inner..own).filter(|&j| factor(j) == 1.0).collect();
        inner = own;
    }
    out
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
            "PartDesign::Scaled" => (self.scaled_alone_candidates(o, target), "scale_pattern"),
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

    /// The bounding box of the Body's bodies together (FreeCAD's Body may be
    /// several solids, mitcad#997), when it has any.
    fn body_box(&self, target: &Target) -> Option<mitcad_model::BoundingBox> {
        target
            .current
            .iter()
            .filter_map(|b| self.box_of(*b))
            .reduce(|a, b| mitcad_model::BoundingBox {
                min: [0, 1, 2].map(|k| a.min[k].min(b.min[k])),
                max: [0, 1, 2].map(|k| a.max[k].max(b.max[k])),
            })
    }

    /// The bounding box of one body.
    fn box_of(&self, body: BodyUid) -> Option<mitcad_model::BoundingBox> {
        let shape = self.doc.body_shape(body)?;
        self.doc.kernel().bounding_box(shape).ok().flatten()
    }

    /// For each centre of a hole, whether FreeCAD's stored result of the
    /// hole `o` has an edge about the axis through it (along `normal`,
    /// within the hole's widest radius): material FreeCAD drilled there. A
    /// centre without any is one where the Body has no material (a sketch
    /// bound from another Body, mitcad#955). None without a stored result.
    fn drilled_at(
        &mut self,
        o: &Object,
        centres: &[[f64; 3]],
        normal: [f64; 3],
    ) -> Option<Vec<bool>> {
        let widest = ["Diameter", "HoleCutDiameter"]
            .iter()
            .filter_map(|p| o.f64(p))
            .fold(0.0, f64::max);
        let reach = widest / 2.0 + 0.5;
        let mut points: Vec<[f64; 3]> = Vec::new();
        for i in 0..1_000_000 {
            let Some((_, curves)) =
                self.stored_element_own(&o.name, mitcad_model::ElementKind::Edge, i)
            else {
                break;
            };
            points.extend(curves.iter().flat_map(super::elements::key_points));
        }
        if points.is_empty() {
            return None;
        }
        Some(
            centres
                .iter()
                .map(|c| {
                    points.iter().any(|p| {
                        let d = sub(*p, *c);
                        norm(sub(d, scale(normal, dot(d, normal)))) <= reach
                    })
                })
                .collect(),
        )
    }

    /// Of the Body's bodies, those the tool bodies touch or reach into
    /// (FreeCAD's Body may be several solids, each a MitCad body,
    /// mitcad#1004): boxes apart are apart; where they meet the kernel's
    /// contact test tells, and a kernel without one counts them as
    /// touching. All of them when the Body is one body.
    fn reached_by(&self, tools: &[BodyUid], bodies: &[BodyUid]) -> Vec<BodyUid> {
        if bodies.len() < 2 {
            return bodies.to_vec();
        }
        let kernel = self.doc.kernel();
        bodies
            .iter()
            .copied()
            .filter(|body| {
                let Some(shape) = self.doc.body_shape(*body) else {
                    return false;
                };
                let around = self.box_of(*body);
                tools.iter().any(|tool| {
                    let Some(tool_shape) = self.doc.body_shape(*tool) else {
                        return false;
                    };
                    if let (Some(a), Some(b)) = (&around, self.box_of(*tool))
                        && !a.overlaps(&b, 1e-6)
                    {
                        return false;
                    }
                    match kernel.tool_contacts(tool_shape, &[shape]) {
                        Ok(contacts) => contacts.first() != Some(&mitcad_model::ToolContact::Apart),
                        Err(_) => true,
                    }
                })
            })
            .collect()
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

    /// A feature's profile: its sketch's material regions, or those of the
    /// sketch's edges the feature chose (FreeCAD 1.0's `AllowMultiFace`;
    /// without it FreeCAD takes the whole sketch whatever parts the link
    /// names).
    pub(super) fn profile(
        &mut self,
        o: &Object,
        property: &str,
        component: ComponentUid,
    ) -> Result<Profile, String> {
        let link = o
            .link(property)
            .ok_or_else(|| format!("no {property}"))?
            .clone();
        let kind = self.sources[0]
            .object(&link.object)
            .map(|s| s.type_name.clone())
            .unwrap_or_default();
        // A binder of a whole sketch (mitcad#955): that sketch's curves.
        if super::sketches::is_binder(&kind) && link.subs.is_empty() {
            let sketch = self.binder_sketch(&link.object, component)?;
            return self.whole_profile(sketch);
        }
        if !super::is_sketch(&kind) {
            return Err(format!(
                "a profile that is not a sketch ({}) is not translated yet",
                link.object
            ));
        }
        let sketch = self.sketch_of(&link, component)?;
        if !link.subs.is_empty() && o.bool("AllowMultiFace") == Some(true) {
            return self.chosen_profile(&link, sketch, component);
        }
        self.whole_profile(sketch)
    }

    /// A whole sketch's profile: its material regions.
    fn whole_profile(&self, sketch: SketchMade) -> Result<Profile, String> {
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
        let swept = self.swept(o, component)?;
        let (operation, participants) = self.operation(target, !pocket)?;
        let reversed = o.bool("Reversed").unwrap_or(false);
        let mut flip = reversed ^ pocket;
        // A custom direction, or one along a reference.
        let custom = o.bool("UseCustomVector") == Some(true)
            || o.link("ReferenceAxis")
                .is_some_and(|l| !l.object.is_empty());
        if custom {
            let d = self.vector(o, "Direction").ok_or("no custom direction")?;
            let n = swept.normal();
            let d = target.place.transform.apply_vector(d);
            if norm(cross(d, n)) > 1e-9 * norm(d) {
                // A sketch's regions or a body face (mitcad#547: the face's
                // extrusion takes a direction since mitcad#541).
                return self.oblique_candidates(o, &swept, d, pocket, &operation, &participants);
            }
            // FreeCAD's Direction is the one before Reversed turns it
            // (mitcad#964: a pad along its sketch's normal axis, Reversed,
            // keeps Direction along the normal and pads the other way).
            flip = (dot(d, n) < 0.0) ^ reversed;
        }
        let taper = self.prop(o, "TaperAngle", Kind::Angle, 0.0);
        let taper2 = self.prop(o, "TaperAngle2", Kind::Angle, 0.0);
        let offset = self.prop(o, "Offset", Kind::Length, 0.0);
        let mode = self.side_mode(o);
        let kind = self
            .enum_text(o, "Type")
            .unwrap_or_else(|| "Length".to_owned());
        let kind = kind.trim_start_matches('?').to_owned();
        let negative_side = mode == "Two sides"
            && ["Length", "Length2"]
                .iter()
                .any(|p| o.f64(p).is_some_and(|v| v < 0.0));
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
        // A face profile (mitcad#539): the face along its outward normal,
        // as FreeCAD sweeps it, by distances only.
        for (field, profile) in swept.readings() {
            for (flip, taper_sign, offset_sign) in &variants {
                let extent = if mode == "Symmetric" && kind.starts_with("UpTo") {
                    self.symmetric_up_to(
                        o,
                        target,
                        &swept,
                        &kind,
                        &taper,
                        &offset.signed(*offset_sign),
                    )?
                } else {
                    self.extent(
                        o,
                        target,
                        &mode,
                        &kind,
                        &taper.signed(*taper_sign),
                        &taper2.signed(*taper_sign),
                        &offset.signed(*offset_sign),
                    )?
                };
                if swept.is_face() {
                    super::profiles::face_extrusion_extent(&extent)?;
                }
                let mut def = json!({"type": "extrude", "extent": extent,
                                     "flip": flip, "operation": operation});
                def[field] = profile.clone();
                if participants.as_array().is_some_and(|p| !p.is_empty()) {
                    def["participants"] = participants.clone();
                }
                if let Some(links) = self.object_links(&def, component)? {
                    def["object_links"] = links;
                }
                // A negative length of two sides (mitcad#955): FreeCAD's
                // side then ends on the other side's side, the solid between
                // the two ends (Mitcad's signed sides; a side through all or
                // up to an object cut off there). That reading first, then
                // each side its own way.
                if negative_side && def["extent"]["type"] == "two_sides" {
                    let mut signed = def.clone();
                    signed["extent"]["signed"] = json!(true);
                    out.push(swept.candidate(signed));
                }
                out.push(swept.candidate(def));
            }
        }
        Ok(out)
    }

    /// A Pad or Pocket along a direction off the profile's normal
    /// (mitcad#539; a body face's, mitcad#547): an `extrude` along that
    /// direction, the lengths along it (FreeCAD's lengths along the
    /// profile's normal over the cosine with `AlongSketchNormal`). One
    /// length, two, or symmetric; tapers grow the profile in its plane by
    /// the length along the direction times their tangent, as FreeCAD's do
    /// (`extrude`'s `direction`).
    fn oblique_candidates(
        &mut self,
        o: &Object,
        swept: &Swept,
        direction: [f64; 3],
        pocket: bool,
        operation: &Value,
        participants: &Value,
    ) -> Result<Vec<Candidate>, String> {
        let d = unit(direction).ok_or("no custom direction")?;
        let n = swept.normal();
        let cos = dot(d, n).abs();
        if cos < 1e-9 {
            return Err("a direction in the profile's plane".to_owned());
        }
        let along_normal = o.bool("AlongSketchNormal").unwrap_or(true);
        let k = if along_normal { 1.0 / cos } else { 1.0 };
        let mode = self.side_mode(o);
        let kind_of = |this: &Self, property: &str| {
            this.enum_text(o, property)
                .map(|t| t.trim_start_matches('?').to_owned())
                .unwrap_or_else(|| "Length".to_owned())
        };
        let kind = kind_of(self, "Type");
        let kind2 = kind_of(self, "Type2");
        let length = self.prop(o, "Length", Kind::Length, 0.0).scale(k);
        let length2 = self.prop(o, "Length2", Kind::Length, 0.0).scale(k);
        let taper = self.prop(o, "TaperAngle", Kind::Angle, 0.0);
        let taper2 = self.prop(o, "TaperAngle2", Kind::Angle, 0.0);
        let with_taper = |mut v: Value, t: &Q| {
            if t.value != 0.0 {
                v["taper"] = t.json();
            }
            v
        };
        let distance =
            |q: &Q, t: &Q| with_taper(json!({"type": "distance", "distance": q.json()}), t);
        // The extent with the tapers' signs (FreeCAD's sign is uncertain:
        // both are tried).
        let extent = |sign: f64| -> Result<Value, String> {
            let (t1, t2) = (taper.signed(sign), taper2.signed(sign));
            match (mode.as_str(), kind.as_str(), kind2.as_str()) {
                ("One side", "Length", _) => Ok(distance(&length, &t1)),
                // FreeCAD tapers the second half by its second angle when
                // it has one: two sides of half the length.
                ("Symmetric", "Length" | "TwoLengths", _) if taper2.value != 0.0 => {
                    let half = length.scale(0.5);
                    Ok(json!({"type": "two_sides", "side1": distance(&half, &t1),
                              "side2": distance(&half, &t2)}))
                }
                ("Symmetric", "Length" | "TwoLengths", _) => Ok(with_taper(
                    json!({"type": "symmetric", "distance": length.json(), "full_length": true}),
                    &t1,
                )),
                ("Two sides", "TwoLengths", _) | ("Two sides", "Length", "Length") => {
                    Ok(json!({"type": "two_sides", "side1": distance(&length, &t1),
                           "side2": distance(&length2, &t2)}))
                }
                (mode, kind, _) => Err(format!(
                    "a {mode} {kind} extent along a custom direction is not translated"
                )),
            }
        };
        let tapered = taper.value != 0.0 || taper2.value != 0.0;
        let mut extents = vec![extent(1.0)?];
        if tapered {
            extents.push(extent(-1.0)?);
        }
        // Along the direction, or against it (a pocket's into the
        // material, as Reversed says).
        let reversed = o.bool("Reversed").unwrap_or(false);
        let first = if pocket ^ reversed { -1.0 } else { 1.0 };
        let mut out = Vec::new();
        for (field, profile) in swept.readings() {
            for sign in [first, -first] {
                for extent in &extents {
                    let mut def = json!({"type": "extrude", "extent": extent,
                        "direction": scale(d, sign), "operation": operation});
                    def[field] = profile.clone();
                    if participants.as_array().is_some_and(|p| !p.is_empty()) {
                        def["participants"] = participants.clone();
                    }
                    out.push(swept.candidate(def));
                }
            }
        }
        Ok(out)
    }

    /// A symmetric extent up to an object (FreeCAD 1.1's SideType
    /// Symmetric, mitcad#955): FreeCAD extrudes side one up to it (the face
    /// moved by the offset) and mirrors that side across the profile's
    /// plane. Up to a planar face parallel to the profile that is two
    /// sides: up to the face, and as far the other way (a distance, which
    /// does not follow the face). Mitcad's extrusions have no mirrored
    /// side, so other objects are not translated.
    fn symmetric_up_to(
        &mut self,
        o: &Object,
        target: &Target,
        swept: &Swept,
        kind: &str,
        taper: &Q,
        offset: &Q,
    ) -> Result<Value, String> {
        let refused = || {
            format!(
                "a symmetric {kind} extent (FreeCAD mirrors side one across the sketch plane) \
                 is translated only up to a planar face parallel to the sketch"
            )
        };
        if kind != "UpToFace" || taper.value != 0.0 {
            return Err(refused());
        }
        let Swept::Sketch(p) = swept else {
            return Err(refused());
        };
        let frame = p.sketch.frame;
        let length = self.prop(o, "Length", Kind::Length, 0.0);
        let side = self.side(o, target, kind, &length, offset, "UpToFace")?;
        // (The face's plane is measured in the profile's component.)
        let elsewhere = side["object"]["body"]
            .as_str()
            .and_then(|b| b.parse::<BodyUid>().ok())
            .is_some_and(|b| self.doc.body_component(b) != Some(target.place.component));
        if elsewhere {
            return Err(refused());
        }
        let (base, n) = self.plane_geometry(&side["object"]).ok_or_else(refused)?;
        let normal = frame.normal();
        if norm(cross(n, normal)) > 1e-9 {
            return Err(refused());
        }
        let depth = dot(sub(base, frame.origin), normal).abs() + offset.value;
        if depth <= 1e-9 {
            return Err("a symmetric extent up to a face in the sketch's plane".to_owned());
        }
        Ok(json!({"type": "two_sides", "side1": side,
                  "side2": {"type": "distance", "distance": depth}}))
    }

    /// The `object_links` of an extrusion's or a revolution's objects that
    /// are (or are faces of) bodies of another component than `own`
    /// (mitcad#955; the model's mitcad#762): each body read in its
    /// component through the occurrences where the document places the
    /// two components first, so the extent follows when either moves.
    fn object_links(&self, def: &Value, own: ComponentUid) -> Result<Option<Value>, String> {
        let objects = [
            &def["start"]["object"],
            &def["extent"]["object"],
            &def["extent"]["side1"]["object"],
            &def["extent"]["side2"]["object"],
        ];
        let mut links = serde_json::Map::new();
        for body in objects.iter().filter_map(|o| o["body"].as_str()) {
            let Some(component) = body
                .parse::<BodyUid>()
                .ok()
                .and_then(|uid| self.doc.body_component(uid))
            else {
                continue;
            };
            if component == own || links.contains_key(body) {
                continue;
            }
            let (_, link) = self
                .seen_in(component, own)
                .ok_or_else(|| format!("{body}'s component is not placed"))?;
            if let Some(link) = link {
                links.insert(body.to_owned(), link);
            }
        }
        Ok((!links.is_empty()).then_some(Value::Object(links)))
    }

    /// The point and normal of an extent's object that is a plane: an
    /// origin plane, a fixed plane or a planar face of a body.
    fn plane_geometry(&self, object: &Value) -> Option<([f64; 3], [f64; 3])> {
        let plane = match object["type"].as_str()? {
            "plane" => &object["plane"],
            "face" => object,
            _ => return None,
        };
        let vector = |v: &Value| -> Option<[f64; 3]> {
            let a = v.as_array()?;
            Some([
                a.first()?.as_f64()?,
                a.get(1)?.as_f64()?,
                a.get(2)?.as_f64()?,
            ])
        };
        match plane {
            Value::String(s) => match s.as_str() {
                "xy" => Some(([0.0; 3], [0.0, 0.0, 1.0])),
                "xz" => Some(([0.0; 3], [0.0, 1.0, 0.0])),
                "yz" => Some(([0.0; 3], [1.0, 0.0, 0.0])),
                // A construction plane made of a datum.
                uid => match self.doc.datum(uid.parse().ok()?)? {
                    mitcad_model::datum::Datum::Plane(p) => {
                        Some((p.origin, cross(p.x_axis, p.y_axis)))
                    }
                    _ => None,
                },
            },
            Value::Object(map) if map.contains_key("face") => {
                let body: BodyUid = map["body"].as_str()?.parse().ok()?;
                let face = map["face"].as_str()?.parse().ok()?;
                let shape = self.doc.body_shape(body)?;
                let p = self.doc.kernel().face_plane(shape, &face).ok()?;
                Some((p.origin, p.normal))
            }
            Value::Object(map) => Some((vector(&map["origin"])?, vector(&map["normal"])?)),
            _ => None,
        }
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
            // (Another component's body: through `object_links`.)
            let body = self
                .body_of_object(&link.object)
                .ok_or_else(|| format!("{} has no body here", link.object))?;
            return Ok(json!({"type": "body", "body": body.to_string(), "through": false}));
        }
        let found = match self.plane_ref(link, target.place.component) {
            Ok(found) => found,
            // A planar face Mitcad's body does not have as one face (FreeCAD
            // split or merged it otherwise, mitcad#955): its plane, as
            // FreeCAD takes the face's surface without its limits where the
            // profile reaches past them. Fixed: it does not follow the body.
            Err(why) => self
                .stored_face_plane(link, target.place.component)
                .ok_or(why)?,
        };
        Ok(match found {
            Value::Object(map) if map.contains_key("face") => {
                let mut face = Value::Object(map);
                face["type"] = json!("face");
                face
            }
            plane => json!({"type": "plane", "plane": plane}),
        })
    }

    /// The plane of a planar face of an object's stored shape (`Face<n>`),
    /// in `component`'s coordinates: `{"origin", "normal", "x_axis"}`.
    fn stored_face_plane(&mut self, link: &LinkRef, component: ComponentUid) -> Option<Value> {
        let element = link.subs.first().map(sub_name)?;
        let index = super::elements::element_index(element, "Face")?;
        let (from, curves) =
            self.stored_element_own(&link.object, mitcad_model::ElementKind::Face, index)?;
        let (into, _) = self.seen_in(from, component)?;
        let curves: Vec<mitcad_model::Curve3> = curves
            .iter()
            .map(|c| super::sketches::transform_curve(c, &into))
            .collect();
        let frame = super::sweep_inputs::plane_through(&curves)?;
        let normal = frame.normal();
        let points: Vec<[f64; 3]> = curves
            .iter()
            .flat_map(super::elements::key_points)
            .collect();
        let size = 1.0 + points.iter().map(|p| norm(*p)).fold(0.0, f64::max);
        let planar = points
            .iter()
            .all(|p| dot(sub(*p, frame.origin), normal).abs() <= 1e-7 * size);
        planar.then(|| json!({"origin": frame.origin, "normal": normal, "x_axis": frame.x_axis}))
    }

    // Revolution and Groove.

    fn revolve_candidates(
        &mut self,
        o: &Object,
        target: &Target,
        groove: bool,
    ) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let swept = self.swept(o, component)?;
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
        // By the angle (Angle, Midplane, Reversed).
        let by_angle = if angle.value.to_degrees() >= 360.0 - 1e-9 {
            vec![json!({"type": "full"})]
        } else if o.bool("Midplane") == Some(true) {
            vec![json!({"type": "symmetric", "angle": angle.scale(0.5).json()})]
        } else {
            vec![
                json!({"type": "angle", "angle": angle.signed(sign).json()}),
                json!({"type": "angle", "angle": angle.signed(-sign).json()}),
            ]
        };
        let mut extents: Vec<Value> = match kind.as_str() {
            "Angle" => by_angle.clone(),
            // A groove through all: a full turn (mitcad#539).
            "ThroughAll" => vec![json!({"type": "full"})],
            // Up to the last face: through the body (mitcad#539; FreeCAD
            // itself has not computed one yet).
            "UpToLast" => {
                let body = target
                    .current
                    .first()
                    .ok_or("up to the body: the Body has no solid yet")?;
                vec![json!({"type": "to_object",
                            "object": {"type": "body", "body": body.to_string(), "through": true}})]
            }
            // Over a full turn (mitcad#955): FreeCAD turns the profile back
            // by Angle2 and revolves by the sum, which OCCT's revolution
            // takes less whole turns, so the turn runs from 360° less
            // Angle2 to Angle past the profile: a negative second angle.
            "TwoAngles" if angle.value + angle2.value > std::f64::consts::TAU + 1e-9 => {
                let past = |q: &Q| {
                    Q::combine(&[q], q.value - std::f64::consts::TAU, Kind::Angle, |t| {
                        format!("{} - 360 deg", t[0])
                    })
                };
                vec![
                    json!({"type": "two_sides", "angle1": angle.json(),
                           "angle2": past(&angle2).json()}),
                    json!({"type": "two_sides", "angle1": angle2.json(),
                           "angle2": past(&angle).json()}),
                ]
            }
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
        // FreeCAD 1.0's groove turns by its angle whatever its type
        // (mitcad#539): that reading after the type's.
        if groove && kind != "Angle" {
            extents.extend(by_angle);
        }
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
        // A face profile (mitcad#539): the face turned by angles only.
        if swept.is_face() && extents.iter().any(|e| e["type"] == "to_object") {
            return Err("a face profile turned up to an object is not translated".to_owned());
        }
        let mut out = Vec::new();
        for (field, profile) in swept.readings() {
            for axis in &axes {
                for extent in &extents {
                    let mut def = json!({"type": "revolve", "axis": axis,
                                         "extent": extent, "operation": operation});
                    def[field] = profile.clone();
                    if participants.as_array().is_some_and(|p| !p.is_empty()) {
                        def["participants"] = participants.clone();
                    }
                    if let Some(links) = self.object_links(&def, component)? {
                        def["object_links"] = links;
                    }
                    out.push(swept.candidate(def));
                }
            }
        }
        Ok(out)
    }

    // Fillet and Chamfer.

    /// A dress-up's edges and faces (in its body now); with `UseAllEdges`
    /// none, and the set is of every edge of the body as it is when the
    /// feature is computed (`all_edges`, mitcad#793), so that the edges an
    /// edit upstream adds are rounded too. An edge FreeCAD merged is named
    /// by the faces along it where it can ([`super::elements`]).
    ///
    /// FreeCAD's Body may be several solids, each a Mitcad body here
    /// (mitcad#997): the edges and faces are grouped by the body they are
    /// on, in the order the link names them, one dress-up per body (with
    /// `UseAllEdges`, one per body of the Body).
    fn dressed(&mut self, o: &Object, target: &Target) -> Result<Vec<Dressed>, String> {
        if target.current.is_empty() {
            return Err("the Body has no solid yet".to_owned());
        }
        if o.bool("UseAllEdges") == Some(true) {
            let mut all = Vec::new();
            for body in &target.current {
                self.doc.body_shape(*body).ok_or("the body is gone")?;
                all.push(Dressed {
                    body: *body,
                    edges: Vec::new(),
                    faces: Vec::new(),
                    all: true,
                });
            }
            return Ok(all);
        }
        let link = o.link("Base").ok_or("no edges")?.clone();
        let mut dressed: Vec<Dressed> = Vec::new();
        for sub in &link.subs {
            let name = sub_name(sub);
            let (b, names) = self.element_names(&link.object, name)?;
            if !target.current.contains(&b) {
                return Err(format!("{}.{name} is on another body", link.object));
            }
            let at = match dressed.iter().position(|d| d.body == b) {
                Some(at) => at,
                None => {
                    dressed.push(Dressed {
                        body: b,
                        edges: Vec::new(),
                        faces: Vec::new(),
                        all: false,
                    });
                    dressed.len() - 1
                }
            };
            if name.starts_with("Face") {
                dressed[at].faces.extend(names);
            } else if name.starts_with("Edge") {
                dressed[at].edges.extend(names);
            } else {
                return Err(format!("{name}: not an edge or a face"));
            }
        }
        if dressed.is_empty() {
            return Err("no edges".to_owned());
        }
        Ok(dressed)
    }

    fn fillet_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let dressed = self.dressed(o, target)?;
        o.f64("Radius").ok_or("no radius")?;
        let radius = self.prop(o, "Radius", Kind::Length, 0.0);
        let defs = dressed.iter().map(|d| {
            let mut set = json!({"size": {"type": "constant", "radius": radius.json()}});
            d.select(&mut set);
            json!({"type": "fillet", "body": d.body.to_string(), "sets": [set]})
        });
        Ok(vec![per_body(defs.collect())])
    }

    fn chamfer_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let dressed = self.dressed(o, target)?;
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
                let defs = dressed.iter().map(|d| {
                    let mut set = json!({"size": size_def, "flip": flip});
                    d.select(&mut set);
                    json!({"type": "chamfer", "body": d.body.to_string(), "sets": [set]})
                });
                per_body(defs.collect())
            })
            .collect())
    }

    // Hole.

    fn hole_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let link = o.link("Profile").ok_or("no profile")?.clone();
        let profile_object = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or("no sketch")?;
        // A binder of a whole sketch (mitcad#955): that sketch's circles
        // and points, projected.
        let (sketch, fc_sketch) = if super::sketches::is_binder(&profile_object.type_name) {
            let sketch = self.binder_sketch(&link.object, component)?;
            let bound = self
                .sketches_of(&profile_object)
                .and_then(|s| s.into_iter().next())
                .ok_or("the binder binds no sketch")?;
            (sketch, fc::Sketch::of(&bound))
        } else {
            (
                self.sketch_of(&link, component)?,
                fc::Sketch::of(&profile_object),
            )
        };
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
        // 1.1's defining external geometry is part of the sketch's shape,
        // whose circles and arcs FreeCAD drills at too (mitcad#955).
        for (k, g) in fc_sketch.external.iter().enumerate() {
            let Some(g) = g else { continue };
            if g.defining
                && matches!(g.curve, Curve::Circle { .. } | Curve::Arc { .. })
                && let Some(id) = sketch.externals.get(&k)
            {
                let curve = format!("c{id}");
                if !points.contains(&curve) {
                    points.push(curve);
                }
            }
        }
        if points.is_empty() {
            // The sketch's points in FreeCAD's order, construction ones left
            // out as FreeCAD's sketch shape leaves them out. Coincident
            // points are one Mitcad point (the sketch merges them): FreeCAD
            // drills there once, and the same position twice would make the
            // hole's tools overlap (mitcad#835).
            for (i, g) in fc_sketch.geometry.iter().enumerate() {
                if g.construction || !matches!(g.curve, Curve::Point { .. }) {
                    continue;
                }
                if let Some(id) = sketch.points.get(&(i as i32)) {
                    let point = format!("p{id}");
                    if !points.contains(&point) {
                        points.push(point);
                    }
                }
            }
        }
        if points.is_empty() {
            return Err("the profile sketch has no circles or points".to_owned());
        }
        // Which holes reach the body (a sketch bound into another Body may
        // hold centres where that Body has no material; FreeCAD drills
        // there too, cutting nothing, mitcad#955): the axis meets the
        // body's box, and FreeCAD's result has edges about it.
        let normal = sketch.frame.normal();
        let positions: Vec<Option<[f64; 3]>> = {
            let output = self.doc.sketch_output(sketch.uid);
            let frame = sketch.frame;
            points
                .iter()
                .map(|r| {
                    let at = output.and_then(|o| {
                        let id = mitcad_model::EntityUid(r.get(1..)?.parse().ok()?);
                        if r.starts_with('c') {
                            match o.solved.curves.get(&id)? {
                                Curve2::Circle { center, .. } | Curve2::Arc { center, .. } => {
                                    Some(*center)
                                }
                                _ => None,
                            }
                        } else {
                            o.solved.points.get(&id).copied()
                        }
                    });
                    at.map(|p| frame.point(p))
                })
                .collect()
        };
        let known: Vec<[f64; 3]> = positions.iter().flatten().copied().collect();
        let mut drilled = self
            .drilled_at(o, &known, normal)
            .unwrap_or_else(|| vec![true; known.len()])
            .into_iter();
        let body_box = self.body_box(target);
        let reached: Vec<bool> = positions
            .iter()
            .map(|p| match p {
                Some(p) => {
                    let hit = drilled.next().unwrap_or(true);
                    hit && body_box.as_ref().is_none_or(|b| !misses_box(*p, normal, b))
                }
                None => true,
            })
            .collect();
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
        // A modelled thread as FreeCAD makes it: its groove cut along a
        // helix (tried before Mitcad's thread on the walls), about each
        // centre the hole drills that reaches the body: the solved
        // positions of its circles, of 1.1's defining external circles and
        // arcs, or of its points (mitcad#964: the groove took the sketch's
        // own circles only, so a hole drilled at an external arc's centre
        // had no thread there).
        let groove = match &thread {
            Some(t) if modeled => {
                let centers: Vec<[f64; 3]> = positions
                    .iter()
                    .zip(&reached)
                    .filter_map(|(p, hit)| if *hit { *p } else { None })
                    .collect();
                Some(self.thread_groove(o, t, centers, &sketch, target))
            }
            _ => None,
        };
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
        let radius = diameter.value / 2.0;
        let diameter = diameter.json();
        let point = point.json();
        // FreeCAD cuts the hole from the Body's whole shape, which may be
        // several solids (mitcad#997): each solid a hole reaches is drilled.
        // So the hole works on all the Body's bodies; the model cuts those
        // it reaches.
        let participants: Vec<BodyUid> = match target.current.as_slice() {
            [] => return Err("the Body has no solid yet".to_owned()),
            bodies => bodies.to_vec(),
        };
        let boxes: Vec<Option<mitcad_model::BoundingBox>> =
            participants.iter().map(|b| self.box_of(*b)).collect();
        let reversed = o.bool("Reversed").unwrap_or(false);
        let mut out = Vec::new();
        for flip in [reversed, !reversed] {
            let mut def = json!({"type": "hole",
                "placement": {"type": "sketch_points", "sketch": sketch.uid.to_string(), "points": points},
                "diameter": diameter, "kind": kind, "extent": extent, "flip": flip,
                "participants": uids(&participants)});
            if angled {
                def["tip_angle"] = point.clone();
            } else {
                def["flat"] = json!(true);
            }
            if let Some(t) = &taper {
                def["taper"] = t.json();
            }
            if let Some(Ok(g)) = &groove {
                out.push(g.candidate(def.clone(), flip, &participants));
            }
            let mut candidate = Candidate::new(def);
            if let Some(Err(why)) = &groove {
                candidate =
                    candidate.note(format!("FreeCAD's thread groove is not rebuilt: {why}"));
            }
            if let Some(thread) = &thread {
                // (Only the holes that reach the body have walls; of several
                // bodies, those the hole's drilled length meets.)
                let drill = scale(normal, if flip { 1.0 } else { -1.0 });
                let length = if extent["type"] == "distance" {
                    depth_value + radius
                } else {
                    f64::INFINITY
                };
                let mut faces: Vec<Value> = Vec::new();
                for (i, start) in positions.iter().enumerate() {
                    if !reached[i] {
                        continue;
                    }
                    for (body, bb) in participants.iter().zip(&boxes) {
                        let meets = participants.len() == 1
                            || match (start, bb) {
                                (Some(p), Some(bb)) => meets_box(*p, drill, length, radius, bb),
                                _ => true,
                            };
                        if meets {
                            faces.push(
                                json!({"body": body.to_string(), "face": format!("$0:hole{i}.wall")}),
                            );
                        }
                    }
                }
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

    /// A hole's enumeration whose values depend on its thread type
    /// (`ThreadSize`, `ThreadClass`): from the file's list, else the
    /// version's for the type.
    fn hole_listed(&self, o: &Object, property: &str, kind: &str) -> Option<String> {
        let FcValue::Enumeration(e) = o.value(property)? else {
            return None;
        };
        let index = usize::try_from(e.index).ok()?;
        match &e.custom {
            Some(list) => list.get(index).cloned(),
            None => mitcad_freecad::version::enum_values(
                &self.sources[0].file.document.version(),
                "PartDesign::Hole",
                &format!("{property}[{kind}]"),
            )?
            .get(index)
            .cloned(),
        }
    }

    /// A hole's thread as a Mitcad thread definition (ISO metric, a Unified
    /// UNC, UNF or UNEF, a Whitworth BSW, BSF or pipe (BSP, Mitcad's `G`),
    /// an NPT or a tyre valve (ISOTyre) size Mitcad's table has): its size
    /// from the version's list
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
            "ISOTyre" => ThreadStandard::TyreValve,
            _ => {
                return Err(format!(
                    "a {kind} thread (Mitcad has ISO metric, Unified, Whitworth, NPT and tyre \
                     valve ones)"
                ));
            }
        };
        if modeled {
            standard.check_modeled()?;
        }
        let listed = |property: &str| self.hole_listed(o, property, &kind);
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
            (ThreadStandard::TyreValve, _) => {
                thread_table::lookup(standard, &size)
                    .map_err(|_| missing())?
                    .designation
            }
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

    /// A hole's modelled ISO metric or Unified thread as FreeCAD builds it
    /// (its Hole's `makeThread`): the groove's section in a plane through
    /// the axis (60° flanks, a flat root P/8 wide at the major diameter
    /// plus the class's clearance, reaching into the bore), swept along a
    /// helix into the material from a pitch above the top face, as deep as
    /// the thread depth and a little more (P/8 for the hole's depth, P/2 for
    /// a given depth, 2P through all). Before 1.1 the section is a hexagon
    /// whose flanks start at the basic minor diameter; beyond the bore both
    /// are the same.
    fn thread_groove(
        &mut self,
        o: &Object,
        thread: &Value,
        centers: Vec<[f64; 3]>,
        sketch: &SketchMade,
        target: &Target,
    ) -> Result<ThreadGroove, String> {
        use mitcad_model::features::thread_table::{self, ThreadStandard};
        let kind = self.enum_text(o, "ThreadType").unwrap_or_default();
        let standard = match kind.as_str() {
            "ISOMetricProfile" | "ISOMetricFineProfile" => ThreadStandard::IsoMetric,
            "UNC" | "UNF" | "UNEF" => ThreadStandard::Unified,
            other => return Err(format!("a modelled {other} thread")),
        };
        if o.bool("Tapered") == Some(true) {
            return Err("a tapered hole".to_owned());
        }
        let designation = thread["designation"].as_str().ok_or("no designation")?;
        let data = thread_table::lookup(standard, designation)?;
        let pitch = data.pitch;
        let rmaj = data.major / 2.0;
        // The class's clearance on the diameter (FreeCAD's table for the
        // ISO classes G), or the custom one.
        let clearance = if o.bool("UseCustomThreadClearance") == Some(true) {
            o.f64("CustomThreadClearance").unwrap_or(0.0)
        } else if self
            .hole_listed(o, "ThreadClass", &kind)
            .is_some_and(|c| c.as_bytes().get(1) == Some(&b'G'))
        {
            const G: [(f64, f64); 25] = [
                (0.2, 0.017),
                (0.25, 0.018),
                (0.3, 0.018),
                (0.35, 0.019),
                (0.4, 0.019),
                (0.45, 0.020),
                (0.5, 0.020),
                (0.6, 0.021),
                (0.7, 0.022),
                (0.75, 0.022),
                (0.8, 0.024),
                (1.0, 0.026),
                (1.25, 0.028),
                (1.5, 0.032),
                (1.75, 0.034),
                (2.0, 0.038),
                (2.5, 0.042),
                (3.0, 0.048),
                (3.5, 0.053),
                (4.0, 0.060),
                (4.5, 0.063),
                (5.0, 0.071),
                (5.5, 0.075),
                (6.0, 0.080),
                (8.0, 0.100),
            ];
            G.iter().find(|(p, _)| pitch <= *p).map_or(0.0, |(_, c)| *c)
        } else {
            0.0
        };
        // How far the groove runs (FreeCAD keeps the thread depth it used).
        let depth = o.f64("Depth").ok_or("no depth")?;
        let thread_depth = o.f64("ThreadDepth").ok_or("no thread depth")?;
        let depth_type = self.enum_text(o, "DepthType").unwrap_or_default();
        let length = match self.enum_text(o, "ThreadDepthType").as_deref() {
            Some("Dimension")
                if depth_type == "Dimension" && thread_depth > depth - pitch / 2.0 =>
            {
                depth + pitch / 8.0
            }
            Some("Dimension") => thread_depth + pitch / 2.0,
            _ if depth_type == "ThroughAll" => thread_depth + 2.0 * pitch,
            Some("Tapped (DIN76)") => thread_depth + pitch / 2.0,
            _ => thread_depth + pitch / 8.0,
        };
        let h = 3f64.sqrt() / 2.0 * pitch;
        let section: Vec<[f64; 2]> = if self.sources[0].file.document.version().at_least(1, 1) {
            let major = rmaj + clearance / 2.0;
            let (margin_z, margin_x) = (0.001, 60f64.to_radians().tan() * 0.001);
            let base = major - 7.0 * h / 8.0 + margin_x;
            vec![
                [base, margin_z],
                [major, 7.0 * pitch / 16.0],
                [major, 9.0 * pitch / 16.0],
                [base, pitch - margin_z],
            ]
        } else {
            let minor = rmaj - 5.0 * h / 8.0;
            let (lo, hi) = (minor + clearance / 2.0, rmaj + clearance / 2.0);
            vec![
                [lo, pitch / 8.0],
                [hi, 7.0 * pitch / 16.0],
                [hi, 9.0 * pitch / 16.0],
                [lo, 7.0 * pitch / 8.0],
                [0.9 * minor, 7.0 * pitch / 8.0],
                [0.9 * minor, pitch / 8.0],
            ]
        };
        let frame = &sketch.frame;
        // (The centres whose axis misses the body, or where FreeCAD's
        // result has no edge about the axis, cut nothing there: FreeCAD
        // drills them anyway, mitcad#955. They are left out of `centers`,
        // as a groove there would refuse to cut.)
        let normal = unit(cross(frame.x_axis, frame.y_axis)).ok_or("the sketch has no normal")?;
        if centers.is_empty() {
            return Err("no circles to thread".to_owned());
        }
        // Through all, FreeCAD's groove runs on far past the body: it is cut
        // only as far as two pitches beyond the body's farthest corner from
        // the sketch plane (the same result, a shorter helix).
        let mut length = length;
        if depth_type == "ThroughAll"
            && let Some(b) = self.body_box(target)
        {
            let reach = (0..8)
                .map(|k| {
                    let corner = [
                        if k & 1 == 0 { b.min[0] } else { b.max[0] },
                        if k & 2 == 0 { b.min[1] } else { b.max[1] },
                        if k & 4 == 0 { b.min[2] } else { b.max[2] },
                    ];
                    dot(sub(corner, frame.origin), normal).abs()
                })
                .fold(0.0, f64::max);
            length = length.min(reach + 2.0 * pitch);
        }
        Ok(ThreadGroove {
            centers,
            normal,
            section,
            pitch,
            revolutions: length / pitch,
            left_handed: self.enum_text(o, "ThreadDirection").as_deref() == Some("Left"),
        })
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
            // FreeCAD transforms the Body's whole shape, which may be several
            // solids, each a MitCad body (mitcad#1004): all of them, each
            // copy joined to every one of them it touches (mitcad#1006,
            // `combine_whole`).
            if target.current.is_empty() {
                return Err("the Body has no solid yet".to_owned());
            }
            return Ok(json!({"type": "bodies", "bodies": uids(&target.current)}));
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
            combine_whole(&mut def, objects);
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
    /// (patterns of patterns); a Scaled transformation as the `scale` of
    /// the pattern or mirror before it, or first as a `scale_pattern`
    /// ([`Self::chained_candidates`]). Of the whole body (`TransformMode`
    /// whole shape): each transformation a pattern or mirror of the body
    /// joining its copies to it.
    fn multi_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let objects = self.transformed(o, target)?;
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
        // The whole body: FreeCAD scales its copies about the Body's origin
        // carried with each (mitcad#542, measured in 1.0 and 1.1).
        if objects["type"] == "bodies" {
            let origin = target.place.transform.apply_point([0.0; 3]);
            let center = json!({"type": "fixed", "point": origin});
            return self.chained_candidates(&steps, &objects, component, Some(&center));
        }
        if kinds.contains(&"Scaled") {
            return self.scaled_candidates(&steps, &objects, component);
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
        out.extend(self.chained_candidates(&steps, &objects, component, None)?);
        Ok(out)
    }

    /// A MultiTransform with a Scaled transformation. FreeCAD scales the
    /// copies the transformations before it made (the first one's turning
    /// fastest), in as many runs of equal length as its occurrences, run g
    /// by 1 + (Factor − 1) · g / (Occurrences − 1), each copy about the
    /// first original's centre of mass carried with it; a Scaled
    /// transformation first makes that many scaled copies of the originals.
    /// Two linear patterns and a Scaled one: a pattern of two directions
    /// with `scale` (and `scale_groups`); otherwise the chain
    /// ([`Self::chained_candidates`]).
    fn scaled_candidates(
        &mut self,
        steps: &[Object],
        objects: &Value,
        component: ComponentUid,
    ) -> Result<Vec<Candidate>, String> {
        let kinds: Vec<&str> = steps
            .iter()
            .map(|s| s.type_name.trim_start_matches("PartDesign::"))
            .collect();
        let mut out = Vec::new();
        if kinds == ["LinearPattern", "LinearPattern", "Scaled"] {
            let first = self.linear_defs(&steps[0], objects, component)?;
            let second = self.linear_defs(&steps[1], objects, component)?;
            let (factor, groups) = self.scaled_values(&steps[2]);
            let copies = elements_of(&steps[0]) * elements_of(&steps[1]);
            let m = steps[2].i64("Occurrences").unwrap_or(0);
            let mut merged = Vec::new();
            for a in &first {
                for b in &second {
                    if a.get("direction2").is_some()
                        || b.get("direction2").is_some()
                        || a["distance_type"] != b["distance_type"]
                    {
                        continue;
                    }
                    let mut def = a.clone();
                    def["direction2"] = b["direction1"].clone();
                    def["scale"] = factor.json();
                    if groups.text.is_some() || groups.value != copies as f64 {
                        def["scale_groups"] = groups.json();
                    }
                    merged.push(def);
                }
            }
            // FreeCAD 1.0 and 1.1 leave out the copies the scale does not
            // change ([`dropped_copies`]).
            let dropped = dropped_copies(&[copies], None, &[(factor.value, m, 1)]);
            let skipping: Vec<Value> = merged
                .iter()
                .filter(|_| !dropped[0].is_empty())
                .map(|def| {
                    let mut def = def.clone();
                    def["skipped_copies"] = json!(dropped[0]);
                    def
                })
                .collect();
            let readings = if self.drops_unscaled_copies() {
                skipping.into_iter().chain(merged).collect::<Vec<_>>()
            } else {
                merged.into_iter().chain(skipping).collect()
            };
            for def in readings {
                out.push(Candidate::new(def).note(
                    "a MultiTransform of two linear patterns and a Scaled transformation: a \
                     pattern of two directions scaling its copies",
                ));
            }
        }
        match self.chained_candidates(steps, objects, component, None) {
            Ok(chained) => out.extend(chained),
            Err(e) if out.is_empty() => return Err(e),
            Err(_) => {}
        }
        Ok(out)
    }

    /// A Scaled transformation's factor and occurrences.
    fn scaled_values(&mut self, scaled: &Object) -> (Q, Q) {
        (
            self.prop(scaled, "Factor", Kind::Number, 1.0),
            self.prop(scaled, "Occurrences", Kind::Number, 2.0),
        )
    }

    /// Whether the document's FreeCAD leaves out the copies a Scaled
    /// transformation after others does not change (1.0 and 1.1; the
    /// stored shapes lack them, 0.21's have them; mitcad#539, mitcad#542):
    /// those readings come first.
    fn drops_unscaled_copies(&self) -> bool {
        self.sources[0].file.document.version().at_least(1, 0)
    }

    /// A Scaled transformation of its own (not in a MultiTransform): a
    /// `scale_pattern` of its originals.
    fn scaled_alone_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let objects = self.transformed(o, target)?;
        if objects["type"] != "features" {
            return Err("a Scaled transformation of the whole body is not translated".to_owned());
        }
        let (factor, quantity) = self.scaled_values(o);
        Ok(vec![Candidate::new(
            json!({"type": "scale_pattern", "objects": objects,
            "quantity": quantity.json(), "scale": factor.json(), "compute": "identical"}),
        )])
    }

    /// A MultiTransform's transformations as a chain: the first a pattern
    /// or mirror of the originals, each next one of the one before (a
    /// pattern of a pattern repeats the originals at every product of the
    /// elements, as FreeCAD's transformations multiply). A Scaled
    /// transformation first is a `scale_pattern` of the originals; after
    /// others it scales the copies they made in runs (its occurrences),
    /// each copy about its own centre ([`Self::add_scaled`]): the scale of
    /// the pattern or mirror before it, and where its runs split the
    /// copies of the patterns before that one (more runs than that one's
    /// elements) the same scale, counted over all those copies, on those
    /// patterns too; several Scaled transformations multiply (mitcad#542).
    /// FreeCAD 1.0 and 1.1 leave out every copy whose factor is 1 once a
    /// Scaled transformation follows another one: those readings skip
    /// them (`skipped_copies`, [`dropped_copies`]).
    /// Of the whole body the first transformation is a pattern or mirror of
    /// the body, each next one of the one before (a pattern of patterns of
    /// the body, mitcad#547), all joining their copies to it (`combine`):
    /// the body as it was before them at every product of the elements,
    /// FreeCAD's product of the transformations, each copy scaled and
    /// skipped as above, about `center` (the Body's origin) carried. The
    /// readings of each step (FreeCAD's direction first) in every
    /// combination, the first 16.
    fn chained_candidates(
        &mut self,
        steps: &[Object],
        objects: &Value,
        component: ComponentUid,
        center: Option<&Value>,
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
        let whole = objects["type"] == "bodies";
        let mut readings: Vec<Vec<Value>> = Vec::new();
        // The elements of each reading's feature.
        let mut counts: Vec<i64> = Vec::new();
        // A Scaled transformation first (its factor), and those after
        // others (factor, occurrences, the readings before it).
        let mut first: Option<f64> = None;
        let mut scales: Vec<(f64, i64, usize)> = Vec::new();
        for step in steps {
            // Of the whole body too each one is a pattern of the one before
            // (a pattern of a pattern of the body, mitcad#547): the body as
            // it was at every product of the elements, joined to it.
            let what = match readings.len() {
                0 => objects.clone(),
                n => json!({"type": "features", "features": [format!("${}", n - 1)]}),
            };
            let kind = step.type_name.trim_start_matches("PartDesign::");
            if kind == "Scaled" {
                let (factor, groups) = self.scaled_values(step);
                let m = step.i64("Occurrences").unwrap_or(0);
                if readings.is_empty() {
                    let mut def = json!({"type": "scale_pattern", "objects": what,
                        "quantity": groups.json(), "scale": factor.json(),
                        "compute": "identical"});
                    if let Some(center) = center {
                        combine_whole(&mut def, objects);
                        def["scale_center"] = center.clone();
                    }
                    readings.push(vec![def]);
                    counts.push(m);
                    first = Some(factor.value);
                    continue;
                }
                self.add_scaled(&mut readings, &counts, &factor, &groups, m, center)?;
                scales.push((factor.value, m, readings.len()));
                continue;
            }
            readings.push(match kind {
                "Mirrored" => vec![self.mirror_def(step, &what, component)?],
                "LinearPattern" => self.linear_defs(step, &what, component)?,
                "PolarPattern" => self.polar_defs(step, &what, component)?,
                other => return Err(format!("a {other} transformation is not translated")),
            });
            if whole && let Some(defs) = readings.last_mut() {
                for def in defs {
                    combine_whole(def, objects);
                }
            }
            counts.push(elements_of(step));
        }
        if readings.is_empty() {
            return Err("a MultiTransform without transformations".to_owned());
        }
        let scaled = first.is_some() || !scales.is_empty();
        // FreeCAD 1.0 and 1.1 leave out the copies the scales do not change
        // once a Scaled transformation follows others.
        let dropped = dropped_copies(&counts, first, &scales);
        let skipping: Vec<Vec<Value>> = readings
            .iter()
            .zip(&dropped)
            .map(|(defs, list)| {
                defs.iter()
                    .map(|def| {
                        let mut def = def.clone();
                        if !list.is_empty() {
                            def["skipped_copies"] = json!(list);
                        }
                        def
                    })
                    .collect()
            })
            .collect();
        let families = if dropped.iter().all(Vec::is_empty) {
            vec![readings]
        } else if self.drops_unscaled_copies() {
            vec![skipping, readings]
        } else {
            vec![readings, skipping]
        };
        let kinds = steps
            .iter()
            .map(|s| s.type_name.trim_start_matches("PartDesign::"))
            .collect::<Vec<_>>()
            .join(", ");
        let note = if whole {
            format!(
                "a MultiTransform of the whole body ({kinds}): each transformation a pattern or \
                 mirror of the one before, the body's copies joined to it"
            )
        } else {
            format!(
                "a MultiTransform of {kinds}: each transformation a pattern or mirror of the one \
                 before{}",
                if scaled {
                    ", the Scaled ones scaling the copies before them"
                } else {
                    ""
                }
            )
        };
        let mut out = Vec::new();
        for readings in &families {
            // Every combination of the steps' readings, the first ones
            // first.
            let mut pick = vec![0usize; readings.len()];
            let mut made = 0;
            'combinations: while made < 16 {
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
                made += 1;
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
        }
        Ok(out)
    }

    /// A Scaled transformation after `readings`' transformations (`counts`
    /// their elements): FreeCAD scales copy j of the N they made (the
    /// first one's element turning fastest) by its run ⌊j · m / N⌋. The
    /// scale of the last one, counted over its own copies (N), with
    /// `scale_groups` when its runs are not its elements; more runs than
    /// its elements split the copies of the ones before it, which then
    /// scale their own copies by the same runs counted over all N
    /// (`scale_copies` N, `scales` where they scale already). `center`: the
    /// whole body's scale centre.
    fn add_scaled(
        &mut self,
        readings: &mut [Vec<Value>],
        counts: &[i64],
        factor: &Q,
        groups: &Q,
        m: i64,
        center: Option<&Value>,
    ) -> Result<(), String> {
        let total: i64 = counts.iter().product();
        let n = counts.last().copied().unwrap_or(1);
        if m < 2 || total % m != 0 {
            return Err(format!(
                "a Scaled transformation of {m} occurrences over {total} copies (FreeCAD needs \
                 the occurrences to divide them)"
            ));
        }
        let top = readings.len() - 1;
        let split = m > n;
        for (level, defs) in readings.iter_mut().enumerate() {
            if level < top && !split {
                continue;
            }
            for def in defs.iter_mut() {
                let groups =
                    (groups.text.is_some() || m != n || level < top).then(|| groups.json());
                let copies = (split || level < top).then_some(total);
                add_scale_rule(def, factor.json(), groups, copies);
                if let Some(center) = center {
                    def["scale_center"] = center.clone();
                }
            }
        }
        Ok(())
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
        // The whole body: its copies joined to it, as FreeCAD fuses them.
        if objects["type"] == "bodies" {
            combine_whole(&mut def, objects);
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
        // The whole body: its copies joined to it, as FreeCAD fuses them.
        let def = |s: f64| {
            let mut value = json!({"type": "circular_pattern", "objects": objects,
                   "axis": axis, "quantity": quantity.json(), "angle": angle.signed(s).json(),
                   "compute": "identical", "combine": false});
            if objects["type"] == "bodies" {
                combine_whole(&mut value, objects);
            }
            value
        };
        Ok(vec![def(sign), def(-sign)])
    }

    // Boolean.

    fn boolean_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let first = *target.current.first().ok_or("the Body has no solid yet")?;
        let operation = match self.enum_text(o, "Type").as_deref().unwrap_or("Fuse") {
            "Fuse" => "join",
            "Cut" => "cut",
            "Common" => "intersect",
            other => return Err(format!("a boolean {other}")),
        };
        let mut tool_bodies: Vec<BodyUid> = Vec::new();
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
                tool_bodies.push(b);
            }
        }
        if tool_bodies.is_empty() {
            return Err("no bodies to combine with".to_owned());
        }
        let tools = uids(&tool_bodies);
        let combine = |body: BodyUid, with: &[String], operation: &str, keep: bool| {
            json!({"type": "combine", "target": body.to_string(),
            "tools": with, "operation": operation, "keep_tools": keep})
        };
        // FreeCAD combines the Body's whole shape with the other Bodies',
        // and the Body may be several solids, each a MitCad body
        // (mitcad#1004): the bodies the tools reach are joined with them
        // into one (the first of them), cut by them or intersected with
        // them; the others stay as they are (a common leaves nothing of
        // them).
        let reached = self.reached_by(&tool_bodies, &target.current);
        match operation {
            "join" => {
                let (into, others) = match reached.split_first() {
                    Some((into, others)) => (*into, others.to_vec()),
                    None => (first, Vec::new()),
                };
                let mut all = tools;
                all.extend(uids(&others));
                let mut candidate = Candidate::new(combine(into, &all, "join", false));
                if reached.is_empty() && target.current.len() > 1 {
                    candidate = candidate.note(
                        "the Boolean's bodies touch none of the Body's bodies: joined to its \
                         first body as solids of their own",
                    );
                }
                Ok(vec![candidate])
            }
            "cut" => {
                // A cut that reaches none changes nothing; the tools go.
                let cut = if reached.is_empty() {
                    vec![first]
                } else {
                    reached
                };
                let defs = cut
                    .iter()
                    .enumerate()
                    .map(|(i, b)| combine(*b, &tools, "cut", i + 1 < cut.len()))
                    .collect();
                Ok(vec![per_body(defs)])
            }
            _ => {
                if reached.is_empty() {
                    return Err("the Boolean's bodies reach none of the Body's bodies".to_owned());
                }
                let mut defs: Vec<(Value, Cow<'static, str>)> = reached
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        let suffix: Cow<'static, str> = if i == 0 {
                            Cow::Borrowed("")
                        } else {
                            Cow::Owned(format!(" (body {})", i + 1))
                        };
                        (
                            combine(*b, &tools, "intersect", i + 1 < reached.len()),
                            suffix,
                        )
                    })
                    .collect();
                let outside: Vec<BodyUid> = target
                    .current
                    .iter()
                    .copied()
                    .filter(|b| !reached.contains(b))
                    .collect();
                if !outside.is_empty() {
                    defs.push((
                        json!({"type": "remove_body", "bodies": uids(&outside)}),
                        Cow::Borrowed(" (outside)"),
                    ));
                }
                Ok(vec![Candidate::several_named(defs)])
            }
        }
    }

    // Draft and Thickness.

    /// The faces a Draft or Thickness names, grouped by the body they are
    /// on, in the order the link names them: FreeCAD's Body may be several
    /// solids, each a MitCad body (mitcad#1004).
    fn faces_of(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<(BodyUid, Vec<String>)>, String> {
        if target.current.is_empty() {
            return Err("the Body has no solid yet".to_owned());
        }
        let link = o.link("Base").ok_or("no faces")?.clone();
        let mut out: Vec<(BodyUid, Vec<String>)> = Vec::new();
        for sub in &link.subs {
            let name = sub_name(sub);
            if !name.starts_with("Face") {
                continue;
            }
            let (b, names) = self.element_names(&link.object, name)?;
            if !target.current.contains(&b) {
                return Err(format!("{}.{name} is on another body", link.object));
            }
            match out.iter_mut().find(|(body, _)| *body == b) {
                Some((_, faces)) => faces.extend(names),
                None => out.push((b, names)),
            }
        }
        Ok(out)
    }

    /// FreeCAD drafts the faces of the Body's whole shape: one draft per
    /// body they are on (mitcad#1004).
    fn draft_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let grouped = self.faces_of(o, target)?;
        if grouped.is_empty() {
            return Err("no faces".to_owned());
        }
        let link = o.link("NeutralPlane").ok_or("no neutral plane")?.clone();
        let plane = self.plane_ref(&link, target.place.component)?;
        let angle = self.prop(o, "Angle", Kind::Angle, 0.0);
        let reversed = o.bool("Reversed").unwrap_or(false);
        let def = |flip: bool, a: &Q| {
            let defs = grouped
                .iter()
                .map(|(body, faces)| {
                    json!({"type": "draft", "body": body.to_string(), "faces": faces,
                           "plane": plane, "angle": a.json(), "flip": flip})
                })
                .collect();
            per_body(defs)
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
            pulled(def(reversed, &angle)),
            pulled(def(!reversed, &angle)),
            pulled(def(reversed, &opposite)),
            pulled(def(!reversed, &opposite)),
        ])
    }

    /// FreeCAD thickens the solids of the Body's whole shape that its faces
    /// are on, each opened at its own faces: one shell per body they are on
    /// (mitcad#1004). Without faces, every body is hollowed closed.
    fn thickness_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let mut grouped = self.faces_of(o, target)?;
        if grouped.is_empty() {
            grouped = target.current.iter().map(|b| (*b, Vec::new())).collect();
        }
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
            let defs = grouped
                .iter()
                .map(|(body, faces)| {
                    json!({"type": "shell", "body": body.to_string(), "faces": faces,
                           "inside": i, "outside": out, "rounded": rounded})
                })
                .collect();
            per_body(defs)
        };
        Ok(vec![
            def(inward, arc && !inward),
            def(!inward, arc && inward),
            def(inward, !(arc && !inward)),
            def(!inward, !(arc && inward)),
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
        // Sketch regions or planar faces of bodies (mitcad#543).
        let mut sections = Vec::new();
        for link in &links {
            sections.push(self.section_of(link, component)?);
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
        // How the profile turns (FreeCAD's Mode, OCCT's pipe shell modes):
        // Standard keeps its angle to the spine, Fixed does not turn,
        // Frenet follows the spine's Frenet frame, Binormal turns it only
        // about a constant binormal, Auxiliary turns it towards an
        // auxiliary spine (mitcad#539).
        let mode = self
            .enum_text(o, "Mode")
            .unwrap_or_else(|| "Standard".to_owned());
        let mut frames: Vec<Value> = Vec::new();
        let orientation = match mode.as_str() {
            "Standard" => "perpendicular",
            "Fixed" => "parallel",
            "Frenet" => {
                // Along a planar spine the Frenet frame is the standard one
                // (but where the spine's bend changes side): both readings.
                frames.push(json!({"type": "frenet"}));
                "perpendicular"
            }
            "Binormal" => {
                // FreeCAD sweeps the pipe in its own placement (the
                // profile's) and takes the binormal there as it is: it
                // turns with the pipe's placement. The pipe shell's frame
                // keeps the binormal (a constant binormal frame).
                let b = self.vector(o, "Binormal").ok_or("no binormal")?;
                let b = match o.value("Placement") {
                    Some(FcValue::Placement(p)) => {
                        mitcad_freecad::placement::Placement::from_quaternion([0.0; 3], p.rotation)
                            .apply(b)
                    }
                    _ => b,
                };
                let b = target.place.transform.apply_vector(b);
                if norm(b) < 1e-12 {
                    return Err("a binormal of no length".to_owned());
                }
                frames.push(json!({"type": "constant_binormal", "binormal": b}));
                "perpendicular"
            }
            "Auxiliary" => {
                // FreeCAD 1.1 renamed the properties (AuxiliarySpine,
                // AuxiliaryCurvilinear); older files have the old names.
                let link = ["AuxiliarySpine", "AuxillerySpine"]
                    .into_iter()
                    .find_map(|p| o.link(p).filter(|l| !l.object.is_empty()))
                    .ok_or("no auxiliary spine")?
                    .clone();
                let curve = self
                    .sketch_path(&link, component)
                    .map_err(|e| format!("the auxiliary spine: {e}"))?;
                // FreeCAD's default: the points at the same fraction of
                // the lengths.
                let curvilinear = o
                    .bool("AuxiliaryCurvilinear")
                    .or_else(|| o.bool("AuxilleryCurvelinear"))
                    .unwrap_or(true);
                frames
                    .push(json!({"type": "auxiliary", "curve": curve, "curvilinear": curvilinear}));
                "perpendicular"
            }
            other => return Err(format!("a {other} pipe is not translated yet")),
        };
        // The profile: a sketch's region or a planar face of a body
        // (mitcad#543).
        let profile = o.link("Profile").ok_or("no profile")?.clone();
        let section = self.section_of(&profile, component)?;
        let spine = o.link("Spine").ok_or("no spine")?.clone();
        let path = self.sketch_path(&spine, component)?;
        let mut def = if several {
            // Several sections: a loft through them along the spine, at
            // right angles to it or as the mode's frame turns them
            // (mitcad#543); the Fixed mode's sections do not turn, they only
            // move along it (the parallel orientation, mitcad#547).
            let mut sections = vec![section];
            for link in o.links("Sections").to_vec() {
                sections.push(self.section_of(&link, component)?);
            }
            let mut def = json!({"type": "loft", "sections": sections, "centerline": path,
                   "ruled": false, "closed": false, "operation": operation});
            if orientation != "perpendicular" {
                def["orientation"] = json!(orientation);
            }
            def
        } else {
            let mut def = json!({"type": "sweep", "path": path, "orientation": orientation,
                                 "operation": operation});
            if section["type"] == "face" {
                def["profile_face"] = json!({"body": section["body"], "face": section["face"]});
            } else {
                def["profiles"] =
                    json!([{"sketch": section["sketch"], "region": section["region"]}]);
            }
            def
        };
        if participants.as_array().is_some_and(|p| !p.is_empty()) {
            def["participants"] = participants;
        }
        let note = |c: Candidate| {
            if several {
                c.note("a pipe through several sections: a loft along its spine")
            } else {
                c
            }
        };
        // The mode's frame (mitcad#539); the Frenet mode also without it.
        let mut out: Vec<Candidate> = frames
            .iter()
            .map(|frame| {
                let mut framed = def.clone();
                framed["frame"] = frame.clone();
                note(Candidate::new(framed))
            })
            .collect();
        if frames.is_empty() || mode == "Frenet" {
            out.push(note(Candidate::new(def)));
        }
        Ok(out)
    }

    /// A loft's or pipe's section (mitcad#543): a sketch's one region, or a
    /// planar face of a body (`Pad.Face6`) as a `face` section, the face
    /// found in the body holding the object's result.
    pub(super) fn section_of(
        &mut self,
        link: &LinkRef,
        component: ComponentUid,
    ) -> Result<Value, String> {
        let object = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no {}", link.object))?;
        if super::is_sketch(&object.type_name) || link.subs.is_empty() {
            let (sketch, region) = self.one_region(link, component)?;
            return Ok(json!({"type": "profile", "sketch": sketch, "region": region}));
        }
        let [sub] = link.subs.as_slice() else {
            return Err(format!(
                "a section of {} elements of {} is not translated",
                link.subs.len(),
                link.object
            ));
        };
        let name = sub_name(sub);
        if !name.starts_with("Face") {
            return Err(format!(
                "a section of {}.{name} (not a face) is not translated",
                link.object
            ));
        }
        let (body, face) = self.element_name(&link.object, name)?;
        if self.doc.body_component(body) != Some(component) {
            return Err(format!(
                "{}.{name} is a face of another component's body",
                link.object
            ));
        }
        Ok(json!({"type": "face", "body": body.to_string(), "face": face}))
    }

    /// A pipe's spine or auxiliary spine (mitcad#539): curves of a sketch,
    /// all of its curves that are not construction lines, or the edges
    /// listed.
    pub(super) fn sketch_path(
        &mut self,
        link: &LinkRef,
        component: ComponentUid,
    ) -> Result<Value, String> {
        let object = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no {}", link.object))?;
        if !super::is_sketch(&object.type_name) {
            return Err("a spine of edges of a body is not translated yet".to_owned());
        }
        let made = self.sketch_of(link, component)?;
        let curves: Vec<String> = if link.subs.is_empty() {
            let mut all: Vec<(&i32, &u32)> = made.curves.iter().collect();
            all.sort();
            let fc_sketch = fc::Sketch::of(&object);
            all.into_iter()
                .filter(|(g, _)| fc_sketch.geometry(**g).is_some_and(|x| !x.construction))
                .map(|(_, id)| format!("c{id}"))
                .collect()
        } else {
            let mut out = Vec::new();
            for sub in &link.subs {
                let name = sub_name(sub);
                let geo = self
                    .sketch_geometry(&object, name, component)
                    .ok_or_else(|| format!("{}.{name} was not found", link.object))?;
                let id = made
                    .curves
                    .get(&geo)
                    .ok_or_else(|| format!("{}.{name} was not imported", link.object))?;
                out.push(format!("c{id}"));
            }
            out
        };
        Ok(json!({"sketch": made.uid.to_string(), "curves": curves}))
    }

    // Helix.

    /// AdditiveHelix, SubtractiveHelix → `helix`: the profile turned about
    /// the reference axis as it rises (FreeCAD sweeps it along a helix
    /// whose frame turns with it, a screw motion). The pitch and the turns
    /// as the mode gives them (pitch and height, pitch and turns, height
    /// and turns); a cone's angle or a growth as the helix's growth per
    /// turn; a subtraction outside the profile as an intersection.
    fn helix_candidates(&mut self, o: &Object, target: &Target) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let additive = o.type_name.contains("Additive");
        // FreeCAD widens a conical or growing helix keeping the profile
        // (nearly) in planes through the axis: Mitcad's helix with a
        // growth per turn and FreeCAD's construction (mitcad#83).
        let growing = self
            .enum_text(o, "Mode")
            .is_some_and(|m| m.contains("growth"));
        let profile = self.profile(o, "Profile", component)?;
        let (mut operation, participants) = self.operation(target, additive)?;
        // A subtraction outside the profile keeps what the helix sweeps
        // through (FreeCAD's common of the two).
        if !additive && o.bool("Outside") == Some(true) {
            operation = json!("intersect");
        }
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
        // Outwards per turn: the growth, or the pitch times the tangent of
        // the cone's angle.
        let growth = if growing {
            Some(self.prop(o, "Growth", Kind::Length, 0.0))
        } else if o.f64("Angle").is_some_and(|a| a.abs() > 1e-9) {
            let angle = self.prop(o, "Angle", Kind::Angle, 0.0);
            Some(Q::combine(
                &[&pitch, &angle],
                pitch.value * angle.value.tan(),
                Kind::Length,
                |t| format!("{} * tan({})", t[0], t[1]),
            ))
        } else {
            None
        }
        .filter(|g| g.value.abs() > 1e-12);
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
                if let Some(g) = &growth {
                    def["growth"] = g.json();
                    def["construction"] = json!("freecad");
                }
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
pub(super) fn interior_point(r: &mitcad_model::sketch::regions::Region) -> Option<[f64; 2]> {
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
pub(super) fn in_polygon(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
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

/// A modelled thread's groove as FreeCAD cuts it (`thread_groove`).
struct ThreadGroove {
    /// The holes' centres on the sketch plane.
    centers: Vec<[f64; 3]>,
    /// The sketch's normal.
    normal: [f64; 3],
    /// The groove's section: (distance from the axis, height above the top
    /// face) at the start.
    section: Vec<[f64; 2]>,
    pitch: f64,
    revolutions: f64,
    left_handed: bool,
}

impl ThreadGroove {
    /// For each centre a fixed plane through the axis, the groove's section
    /// on it and a helix cutting it into the material, then the hole `hole`
    /// (drilling along the sketch normal's reverse unless `flip`).
    fn candidate(&self, hole: Value, flip: bool, bodies: &[BodyUid]) -> Candidate {
        // FreeCAD's direction out of the material and its perpendicular.
        let z = if flip {
            scale(self.normal, -1.0)
        } else {
            self.normal
        };
        let x = if (z[2] - z[0]).abs() > 1e-7 {
            [z[2], 0.0, -z[0]]
        } else if (z[2] - z[1]).abs() > 1e-7 {
            [z[1], -z[0], 0.0]
        } else {
            [0.0, -z[2], z[1]]
        };
        let x = unit(x).unwrap_or([1.0, 0.0, 0.0]);
        // The grooves first, then the hole: cutting a groove that runs out
        // of the body at both ends from the drilled body can fail in the
        // geometry kernel, from the solid body it does not.
        let mut defs: Vec<(Value, &'static str)> = Vec::new();
        for center in &self.centers {
            let at = defs.len();
            let plane = json!({"type": "construction_plane", "definition": {"type": "fixed",
                "origin": center, "x_axis": x, "y_axis": z}});
            let n = self.section.len();
            let mut entities: Vec<Value> = self
                .section
                .iter()
                .enumerate()
                .map(|(k, p)| json!({"id": format!("p{}", k + 1), "type": "point", "at": p}))
                .collect();
            for k in 0..n {
                entities.push(json!({"id": format!("c{}", n + k + 1), "type": "line",
                    "start": format!("p{}", k + 1), "end": format!("p{}", (k + 1) % n + 1)}));
            }
            let section =
                json!({"type": "sketch", "plane": format!("${at}"), "entities": entities});
            let helix = json!({"type": "helix",
                "profiles": [{"sketch": format!("${}", at + 1), "region": format!("${}:region", at + 1)}],
                "axis": {"origin": center, "direction": z}, "pitch": self.pitch,
                "revolutions": self.revolutions, "left_handed": self.left_handed, "flip": true,
                "operation": "cut", "participants": uids(bodies)});
            defs.push((plane, " (thread plane)"));
            defs.push((section, " (thread section)"));
            defs.push((helix, " (thread)"));
        }
        defs.push((hole, ""));
        Candidate::several(defs).note(
            "its modelled thread as FreeCAD cuts it: the groove's section swept along a helix",
        )
    }
}

/// Whether the cylinder of `radius` about the segment from `p` along the
/// unit `n` for `length` (infinite for through all) meets the box: a hole
/// drilled there may reach the body inside it (mitcad#997).
pub(super) fn meets_box(
    p: [f64; 3],
    n: [f64; 3],
    length: f64,
    radius: f64,
    b: &mitcad_model::BoundingBox,
) -> bool {
    let grown = mitcad_model::BoundingBox {
        min: b.min.map(|v| v - radius),
        max: b.max.map(|v| v + radius),
    };
    line_span(p, n, &grown).is_some_and(|(lo, hi)| hi >= -1e-6 && lo <= length + 1e-6)
}

/// The parameters along the line through `p` along `n` where it is inside
/// the box (within 1e-6), when it meets it.
fn line_span(p: [f64; 3], n: [f64; 3], b: &mitcad_model::BoundingBox) -> Option<(f64, f64)> {
    let tol = 1e-6;
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for k in 0..3 {
        if n[k].abs() < 1e-12 {
            if p[k] < b.min[k] - tol || p[k] > b.max[k] + tol {
                return None;
            }
        } else {
            let a = (b.min[k] - tol - p[k]) / n[k];
            let c = (b.max[k] + tol - p[k]) / n[k];
            lo = lo.max(a.min(c));
            hi = hi.min(a.max(c));
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// Whether the line through `p` along `n` misses the box: a hole there
/// cuts nothing.
fn misses_box(p: [f64; 3], n: [f64; 3], b: &mitcad_model::BoundingBox) -> bool {
    line_span(p, n, b).is_none()
}
