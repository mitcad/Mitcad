// SPDX-License-Identifier: MIT
//! Profiles that are not a whole sketch (mitcad#539):
//!
//! - A planar face of a body (`Profile` = (object, `Face<n>`)): Mitcad's
//!   `profile_face` of an extrusion (Pad, Pocket) or a revolution
//!   (Revolution, Groove), the face found in the body holding the object's
//!   result. FreeCAD sweeps the face along its outward normal (BRepGProp's
//!   normal of the face as it is oriented), as Mitcad does. Without
//!   `AllowMultiFace` FreeCAD takes the first element the link names; with
//!   it every one: several faces are a feature per face (mitcad#543), the
//!   first with the feature's operation, the others joined to it (as many
//!   faces as there are, mitcad#547).
//! - Chosen edges of a sketch (`Profile` = (sketch, `Edge<n>`, …) with
//!   `AllowMultiFace`, FreeCAD 1.0): FreeCAD makes faces of the wires the
//!   edges form; Mitcad takes the regions of the whole sketch inside them
//!   (the regions of the chosen curves alone, FreeCAD's material ones,
//!   hold the regions of the whole sketch inside them). Without
//!   `AllowMultiFace` (and in 0.21) FreeCAD takes the whole sketch whatever
//!   edges the link names.

use std::borrow::Cow;

use mitcad_freecad::LinkRef;
use mitcad_freecad::Object;
use mitcad_model::sketch::regions::{self, Region, RegionCurve};
use mitcad_model::{BodyUid, ComponentUid, EntityUid, FaceName, Kernel};
use serde_json::{Value, json};

use super::Importer;
use super::elements::sub_name;
use super::features::{Profile, in_polygon, interior_point, region_depths};
use super::history::SketchMade;
// Profiles of several faces (mitcad#543).
use super::history::Candidate;

/// A planar face of a body as a feature's profile.
pub(super) struct FaceProfile {
    pub body: BodyUid,
    /// Its Mitcad name in the body.
    pub face: String,
    /// Its outward normal.
    pub normal: [f64; 3],
}

/// What a profile-based feature sweeps.
pub(super) enum Swept {
    /// Regions of a sketch.
    Sketch(Box<Profile>),
    /// A planar face of a body.
    Face(FaceProfile),
    /// Several planar faces (`AllowMultiFace`, mitcad#543): a feature per
    /// face, the first with the feature's operation, the others joined to
    /// (or cutting) what it leaves.
    Faces(Vec<FaceProfile>),
}

/// The name suffix of a several-face profile's feature of face `i` (from
/// 0; mitcad#543): none for the first, `" (face 2)"` for the second, and so
/// on for as many faces as there are (mitcad#547).
pub(super) fn face_suffix(i: usize) -> Cow<'static, str> {
    if i == 0 {
        Cow::Borrowed("")
    } else {
        Cow::Owned(format!(" (face {})", i + 1))
    }
}

impl Swept {
    /// The profile fields of a definition, one per reading: `profiles` of
    /// the sketch's material regions (and of the outer ones where they
    /// differ, [`Profile::outer`]), or the `profile_face`.
    pub fn readings(&self) -> Vec<(&'static str, Value)> {
        match self {
            Swept::Sketch(p) => std::iter::once(&p.regions)
                .chain(p.outer.as_ref())
                .map(|regions| {
                    let profiles = regions
                        .iter()
                        .map(|r| json!({"sketch": p.sketch.uid.to_string(), "region": r}))
                        .collect();
                    ("profiles", Value::Array(profiles))
                })
                .collect(),
            Swept::Face(f) => vec![("profile_face", face_value(f))],
            // (`candidate` puts in each face.)
            Swept::Faces(faces) => vec![("profile_face", face_value(&faces[0]))],
        }
    }

    /// The way the profile faces: the sketch's normal, or the face's
    /// outward normal (the first face's of several).
    pub fn normal(&self) -> [f64; 3] {
        match self {
            Swept::Sketch(p) => p.sketch.frame.normal(),
            Swept::Face(f) => f.normal,
            Swept::Faces(faces) => faces[0].normal,
        }
    }

    pub fn is_face(&self) -> bool {
        matches!(self, Swept::Face(_) | Swept::Faces(_))
    }

    /// The candidate of a definition of one reading: the definition, or
    /// for several faces (mitcad#543) one definition per face, the first
    /// with the definition's operation, the others joined to the first's
    /// new body or with its join's or cut's participants.
    pub fn candidate(&self, def: Value) -> Candidate {
        let Swept::Faces(faces) = self else {
            return Candidate::new(def);
        };
        let mut defs: Vec<(Value, Cow<'static, str>)> = Vec::new();
        for (i, face) in faces.iter().enumerate() {
            let mut d = def.clone();
            d["profile_face"] = face_value(face);
            if i > 0 && d["operation"] == "new_body" {
                d["operation"] = json!("join");
                d["participants"] = json!(["$0.b0"]);
            }
            defs.push((d, face_suffix(i)));
        }
        Candidate::several_named(defs).note(format!(
            "a profile of {} faces: a feature per face",
            faces.len()
        ))
    }
}

fn face_value(f: &FaceProfile) -> Value {
    json!({"body": f.body.to_string(), "face": f.face})
}

/// Whether an extrusion's extent (an `extrude` definition's) is one a body
/// face's extrusion takes: distances (one, symmetric or two sides), with
/// their tapers (mitcad#541).
pub(super) fn face_extrusion_extent(extent: &Value) -> Result<(), String> {
    let distance = |v: &Value| v["type"] == "distance";
    let fits = match extent["type"].as_str() {
        Some("distance" | "symmetric") => true,
        Some("two_sides") => distance(&extent["side1"]) && distance(&extent["side2"]),
        _ => false,
    };
    if fits {
        Ok(())
    } else {
        Err(format!(
            "a face profile extruded {} is not translated: Mitcad extrudes a body face by \
             distances",
            extent["type"].as_str().unwrap_or("so")
        ))
    }
}

/// Whether a point lies in a region (inside its outline, not in its
/// holes).
fn in_region(r: &Region, p: [f64; 2]) -> bool {
    in_polygon(&r.outline, p) && !r.holes.iter().any(|h| in_polygon(h, p))
}

/// The keys of the regions of `all` (a sketch's) that lie in a region of
/// `chosen` whose depth passes `keep`.
fn regions_in(
    all: &[Region],
    chosen: &[Region],
    depths: &[(String, usize)],
    keep: impl Fn(usize) -> bool,
) -> Vec<String> {
    all.iter()
        .filter(|r| {
            interior_point(r).is_some_and(|p| {
                chosen
                    .iter()
                    .zip(depths)
                    .any(|(c, (_, depth))| keep(*depth) && in_region(c, p))
            })
        })
        .map(|r| r.profile.key.to_string())
        .collect()
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// What a PartDesign Pad, Pocket, Revolution or Groove sweeps: its
    /// `Profile`'s sketch regions ([`Importer::profile`]), or a planar face
    /// of a body.
    pub(super) fn swept(&mut self, o: &Object, component: ComponentUid) -> Result<Swept, String> {
        let link = o.link("Profile").ok_or("no Profile")?.clone();
        let object = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no {}", link.object))?;
        if super::is_sketch(&object.type_name) || link.subs.is_empty() {
            return self
                .profile(o, "Profile", component)
                .map(|p| Swept::Sketch(Box::new(p)));
        }
        // FreeCAD takes the first element unless the feature allows
        // several faces.
        let names: Vec<String> = if o.bool("AllowMultiFace") == Some(true) {
            link.subs.iter().map(|s| sub_name(s).to_owned()).collect()
        } else {
            vec![sub_name(&link.subs[0]).to_owned()]
        };
        // Several faces (mitcad#543): one feature per face, as many as
        // there are (mitcad#547).
        let mut faces = Vec::new();
        for name in &names {
            faces.push(self.profile_face_of(&link.object, name, component)?);
        }
        Ok(if faces.len() == 1 {
            Swept::Face(faces.remove(0))
        } else {
            Swept::Faces(faces)
        })
    }

    /// One planar face of a body as a profile (`object.Face<n>`).
    fn profile_face_of(
        &mut self,
        object: &str,
        name: &str,
        component: ComponentUid,
    ) -> Result<FaceProfile, String> {
        if !name.starts_with("Face") {
            return Err(format!(
                "a profile of {object}.{name} (not a face) is not translated"
            ));
        }
        let (body, face) = self.element_name(object, name)?;
        if self.doc.body_component(body) != Some(component) {
            return Err(format!(
                "{object}.{name} is a face of another component's body"
            ));
        }
        let shape = self
            .doc
            .body_shape(body)
            .ok_or_else(|| format!("{object}: its body is gone"))?;
        let parsed = face
            .parse::<FaceName>()
            .map_err(|e| format!("{object}.{name}: {e}"))?;
        let plane = self
            .doc
            .kernel()
            .face_plane(shape, &parsed)
            .map_err(|e| format!("{object}.{name} is not a planar face here: {e}"))?;
        Ok(FaceProfile {
            body,
            face,
            normal: plane.normal,
        })
    }

    /// The profile of chosen edges of a sketch (FreeCAD's `Profile` =
    /// (sketch, `Edge<n>`, …) with `AllowMultiFace`): the sketch's
    /// regions inside the areas the edges bound. FreeCAD 1.1 fills the
    /// areas inside an even number of the edges' wires, 1.0 the outer ones
    /// with their holes ([`Profile::outer`]); both are tried.
    pub(super) fn chosen_profile(
        &mut self,
        link: &LinkRef,
        sketch: SketchMade,
        component: ComponentUid,
    ) -> Result<Profile, String> {
        let object = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no {}", link.object))?;
        let mut ids: Vec<EntityUid> = Vec::new();
        for sub in &link.subs {
            let name = sub_name(sub);
            if !name.starts_with("Edge") {
                return Err(format!(
                    "{}.{name}: chosen parts of a sketch other than its edges are not translated",
                    link.object
                ));
            }
            let geometry = self
                .sketch_geometry(&object, name, component)
                .ok_or_else(|| format!("{}.{name} was not found", link.object))?;
            let id = sketch
                .curves
                .get(&geometry)
                .ok_or_else(|| format!("{}.{name} was not imported", link.object))?;
            if !ids.contains(&EntityUid(*id)) {
                ids.push(EntityUid(*id));
            }
        }
        let output = self
            .doc
            .sketch_output(sketch.uid)
            .ok_or("the sketch did not solve")?;
        let curves: Vec<RegionCurve> = ids
            .iter()
            .map(|id| {
                output
                    .solved
                    .curves
                    .get(id)
                    .map(|c| RegionCurve::new(*id, c.clone()))
                    .ok_or_else(|| format!("curve {} of {} has no solution", id.0, link.object))
            })
            .collect::<Result<_, _>>()?;
        let chosen = regions::regions(&curves);
        let depths = region_depths(&chosen);
        let all = &output.region_info;
        let material = regions_in(all, &chosen, &depths, |d| d % 2 == 0);
        let outer = regions_in(all, &chosen, &depths, |d| d == 0);
        if material.is_empty() {
            return Err(format!("the chosen edges of {} bound no area", link.object));
        }
        Ok(Profile {
            sketch,
            outer: (outer != material).then_some(outer),
            regions: material,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitcad_model::sketch::geometry::Curve2;

    /// A rectangle's lines `c<first>`..`c<first + 3>`.
    fn rectangle(first: u32, from: [f64; 2], to: [f64; 2]) -> Vec<RegionCurve> {
        let corners = [from, [to[0], from[1]], to, [from[0], to[1]]];
        (0..4)
            .map(|i| {
                RegionCurve::new(
                    EntityUid(first + i as u32),
                    Curve2::Line {
                        a: corners[i],
                        b: corners[(i + 1) % 4],
                    },
                )
            })
            .collect()
    }

    #[test]
    fn chosen_edges_take_the_regions_inside_them() {
        // A frame (a square c1..c4 round a square c5..c8) and a square
        // c9..c12 beside it: three regions.
        let outer = rectangle(1, [0.0, 0.0], [40.0, 40.0]);
        let inner = rectangle(5, [10.0, 10.0], [30.0, 30.0]);
        let beside = rectangle(9, [50.0, 0.0], [60.0, 10.0]);
        let every: Vec<RegionCurve> = outer.iter().chain(&inner).chain(&beside).cloned().collect();
        let all = regions::regions(&every);
        assert_eq!(all.len(), 3);
        let key_at = |p: [f64; 2]| -> String {
            all.iter()
                .find(|r| in_region(r, p))
                .map(|r| r.profile.key.to_string())
                .unwrap()
        };
        let (frame, hole, side) = (
            key_at([5.0, 5.0]),
            key_at([20.0, 20.0]),
            key_at([55.0, 5.0]),
        );
        let pick = |chosen: &[RegionCurve], keep: fn(usize) -> bool| {
            let regions = regions::regions(chosen);
            let depths = region_depths(&regions);
            let mut keys = regions_in(&all, &regions, &depths, keep);
            keys.sort();
            keys
        };
        let sorted = |mut keys: Vec<String>| {
            keys.sort();
            keys
        };
        let even: fn(usize) -> bool = |d| d % 2 == 0;
        let outermost: fn(usize) -> bool = |d| d == 0;
        // The outer square alone holds the frame and the square in it.
        assert_eq!(
            pick(&outer, even),
            sorted(vec![frame.clone(), hole.clone()])
        );
        // Both squares: the frame only, either reading.
        let both: Vec<RegionCurve> = outer.iter().chain(&inner).cloned().collect();
        assert_eq!(pick(&both, even), vec![frame.clone()]);
        assert_eq!(pick(&both, outermost), vec![frame]);
        // The inner square alone, the square beside alone.
        assert_eq!(pick(&inner, even), vec![hole]);
        assert_eq!(pick(&beside, even), vec![side]);
        // Open edges bound nothing.
        assert!(pick(&outer[..3], even).is_empty());
    }

    #[test]
    fn face_extrusions_go_by_distances() {
        let ok = [
            json!({"type": "distance", "distance": 5.0}),
            json!({"type": "symmetric", "distance": 5.0, "full_length": true}),
            json!({"type": "two_sides", "side1": {"type": "distance", "distance": 5.0},
                   "side2": {"type": "distance", "distance": 2.0}}),
            // Tapered too (mitcad#541).
            json!({"type": "distance", "distance": 5.0, "taper": 0.05}),
        ];
        for extent in &ok {
            assert!(face_extrusion_extent(extent).is_ok(), "{extent}");
        }
        let refused = [
            json!({"type": "through_all", "both_sides": false}),
            json!({"type": "two_sides", "side1": {"type": "distance", "distance": 5.0},
                   "side2": {"type": "through_all"}}),
            json!({"type": "to_object", "object": {"type": "body", "body": "F1.b0"}}),
        ];
        for extent in &refused {
            let e = face_extrusion_extent(extent).unwrap_err();
            assert!(e.contains("by distances"), "{e}");
        }
    }
}
