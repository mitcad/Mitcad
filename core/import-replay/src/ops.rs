// SPDX-License-Identifier: MIT
//! Body and face operations of external dumps (their inputs are
//! fingerprints and feature references the stream decoder does not give):
//! combine, mirror, circular, rectangular and path patterns, shell, offset
//! faces, move, split body, hole, thread and replace face (`commands.md`,
//! F1, F2 and F4 import tables). Without those inputs a
//! combine is found with the history: the bodies the next state changed
//! are its target and tools; so are the faces a replace face replaced.

use std::collections::{HashMap, HashSet};

use mitcad_dump::{Detail, EntityName, Fingerprint, Geometry, Reference, TimelineItem, Vec3};
use mitcad_model::{BodyUid, Cylinder, FaceName, FeatureUid, Kernel, Plane};
use serde_json::{Map, Value, json};

use crate::geom::{self, mm, mm3};
use crate::history::{EXACT, RELATIVE, Sig};
use crate::scaffold::CandidateCore;
use crate::{Candidate, Importer, InputProvenance, features, refs};

/// An item's detail as its JSON object (typed details written back).
fn detail_map(item: &TimelineItem) -> Map<String, Value> {
    item.detail
        .as_ref()
        .and_then(|d| serde_json::to_value(d).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn reference(v: &Value) -> Option<Reference> {
    Some(Reference::from_map(v.as_object()?.clone()))
}

fn references(v: Option<&Value>) -> Vec<Reference> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(reference).collect())
        .unwrap_or_default()
}

/// The dump's compute option as MitCad's.
/// References the stream decoder named (`_f3d`), every one of them.
fn named(refs: &[Reference]) -> bool {
    !refs.is_empty()
        && refs.iter().all(|r| match r {
            Reference::Body(fp) | Reference::Face(fp) | Reference::Edge(fp) => fp.f3d.is_some(),
            _ => false,
        })
}

fn compute(option: Option<&Value>) -> &'static str {
    match option.and_then(Value::as_str) {
        Some(o) if o.starts_with("Identical") => "identical",
        Some(o) if o.starts_with("Optimized") => "optimized",
        _ => "adjust",
    }
}

/// How many of the last extrusions before a pattern without decoded inputs
/// are tried as what it copies (the likeliest copies first, by the volume
/// they add): a pattern often copies a feature a few items back, with
/// other extrusions after it.
const PATTERNED_EXTRUSIONS: usize = 6;

/// A face a hole can start from: how far the removed piece's centre is
/// from half the depth below it, the face and the hole's point on it.
type StartFace = (f64, String, [f64; 3]);

/// The points of a hole feature on one face of a body.
type HolePoints = (BodyUid, String, Vec<[f64; 3]>);

/// A piece of material a history state removed: the body it came from,
/// its centre and its shape.
type Piece<S> = (BodyUid, [f64; 3], S);

/// Definitions to add together, with a note for the report.
type Defs = (Vec<Value>, Option<String>);

/// Copies of features whose inputs the stream decoder does not give; the
/// quantities and the axis when it decodes them.
enum Copies {
    Circular {
        angle: Value,
        symmetric: bool,
        quantity: Option<i64>,
        axis: Option<Value>,
        /// Whether the origin axes are guessed after `axis` (one from the
        /// axis' stored line, mitcad#96).
        origin_axes: bool,
    },
    Rectangular {
        distance: Value,
        quantity: Option<i64>,
        quantity_two: Option<i64>,
        distance_two: Option<Value>,
        /// The distances in mm, where decoded as values.
        steps: (Option<f64>, Option<f64>),
    },
}

/// A guessed rectangular pattern: each direction's reference and vector,
/// quantity and distance (mm, where known), and whether both directions
/// are symmetric.
struct Lattice {
    one: (Value, Vec3, i64, Option<f64>),
    two: Option<(Value, Vec3, i64, Option<f64>)>,
    symmetric: bool,
}

impl Lattice {
    /// The offsets of the copies from the original, in the model's element
    /// order (the original left out); None without the distances.
    fn offsets(&self) -> Option<Vec<Vec3>> {
        let along = |(_, d, q, step): &(Value, Vec3, i64, Option<f64>)| -> Option<Vec<Vec3>> {
            let step = (*step)?;
            // The model's element order: 0, 1, …; when symmetric the q
            // elements lie ⌊q / 2⌋ before the original and the rest after.
            let before = if self.symmetric && *q > 1 { *q / 2 } else { 0 };
            let forward = (0..*q - before).map(|i| i as f64);
            let backward = (1..=before).map(|i| -(i as f64));
            let positions: Vec<f64> = forward.chain(backward).collect();
            Some(
                positions
                    .into_iter()
                    .map(|k| geom::scale(*d, k * step))
                    .collect(),
            )
        };
        let one = along(&self.one)?;
        let two = match &self.two {
            Some(two) => along(two)?,
            None => vec![[0.0; 3]],
        };
        let mut out = Vec::with_capacity(one.len() * two.len());
        for b in &two {
            for a in &one {
                out.push(geom::add(*a, *b));
            }
        }
        out.remove(0);
        Some(out)
    }

    /// The pattern's definition.
    fn def(&self, objects: &Value, distance: &Value, distance_two: Option<&Value>) -> Value {
        let direction = |axis: &Value, q: i64, distance: &Value| {
            let mut d = json!({"axis": axis, "quantity": q, "distance": distance});
            if self.symmetric {
                d["symmetric"] = json!(true);
            }
            d
        };
        let mut def = json!({"type": "rectangular_pattern", "objects": objects,
            "direction1": direction(&self.one.0, self.one.2, distance),
            "distance_type": "spacing", "compute": "adjust"});
        if let (Some(two), Some(d2)) = (&self.two, distance_two) {
            def["direction2"] = direction(&two.0, two.2, d2);
        }
        def
    }

    /// For traces: the directions and `sym` when symmetric, e.g.
    /// `x/(0.50,0.87,0.00) sym`.
    fn label(&self) -> String {
        let one = self.one.0.as_str().unwrap_or("?");
        let two = match &self.two {
            Some((_, v, _, _)) => format!("/({:.2},{:.2},{:.2})", v[0], v[1], v[2]),
            None => String::new(),
        };
        let sym = if self.symmetric { " sym" } else { "" };
        format!("{one}{two}{sym}")
    }

    /// How many copies it makes.
    fn copies(&self) -> i64 {
        self.one.2 * self.two.as_ref().map_or(1, |t| t.2) - 1
    }
}

/// The origin axes as references and vectors.
const AXES: [(&str, Vec3); 3] = [
    ("x", [1.0, 0.0, 0.0]),
    ("y", [0.0, 1.0, 0.0]),
    ("z", [0.0, 0.0, 1.0]),
];

/// The second directions of a rectangular pattern guessed across the first
/// (an origin axis) towards another origin axis: at a right angle, and at
/// 60° and 120° (a hexagonal lattice: a perforated plate of hexagonal holes
/// with even walls, mitcad#74).
const ACROSS: [f64; 3] = [90.0, 60.0, 120.0];

/// How many points of a guessed pattern's copies are classified at most
/// (by the faces of the replay's bodies), and how many of one pattern's
/// copies at least and at most ([`Importer::copies_in_material`]).
const CLASSIFIED: usize = 400_000;
const MIN_SAMPLED: usize = 16;
const MAX_SAMPLED: usize = 256;

/// The rectangular patterns guessed for each quantity of direction one
/// (`quantity_two` of a second direction): direction one along each origin
/// axis, the second across it at right angles, neither symmetric (the
/// guesses before mitcad#74), then with `others` the other angles
/// ([`ACROSS`]) and both directions symmetric.
fn lattices(
    quantities: &[i64],
    quantity_two: Option<i64>,
    steps: (Option<f64>, Option<f64>),
    others: bool,
) -> Vec<Lattice> {
    let mut out = Vec::new();
    for first in [true, false] {
        if !first && !others {
            break;
        }
        for &q in quantities {
            for (name, a) in AXES {
                let one = (json!(name), a, q, steps.0);
                let Some(q2) = quantity_two else {
                    out.push(Lattice {
                        one,
                        two: None,
                        symmetric: !first,
                    });
                    continue;
                };
                for (other, b) in AXES.into_iter().filter(|(o, _)| *o != name) {
                    for angle in ACROSS {
                        for symmetric in [false, true] {
                            if (angle == 90.0 && !symmetric) != first {
                                continue;
                            }
                            let (axis, v) = if angle == 90.0 {
                                (json!(other), b)
                            } else {
                                let t = angle.to_radians();
                                let v = geom::add(geom::scale(a, t.cos()), geom::scale(b, t.sin()));
                                (json!({"origin": [0.0, 0.0, 0.0], "direction": v}), v)
                            };
                            out.push(Lattice {
                                one: one.clone(),
                                two: Some((axis, v, q2, steps.1)),
                                symmetric,
                            });
                        }
                    }
                }
            }
        }
    }
    // MitCad's patterns have at most 10,000 elements.
    out.retain(|l| l.copies() < 10_000);
    out
}

/// Copies of features act only on the bodies each feature changed (in a
/// design, a feature keeps the bodies it worked on: a pattern of a hole
/// leaves another body its copies reach alone, mitcad#74).
fn on_original_bodies(def: &mut Value) {
    let pattern = matches!(
        def["type"].as_str(),
        Some("rectangular_pattern" | "circular_pattern" | "path_pattern" | "mirror")
    );
    if pattern && def["objects"]["type"] == "features" {
        def["original_bodies"] = json!(true);
    }
}

/// How far (relative to its size) the centre of a body that a joined
/// mirror makes may lie from the mirror plane: the join is symmetric about
/// it, but for slivers left out along near-coincident faces.
const JOINED_CENTRE: f64 = 1e-2;

/// The origin and unit normal of a mirror plane given as an origin plane or
/// a fixed plane; None for faces and construction features.
fn plane_geometry(plane: &Value) -> Option<(Vec3, Vec3)> {
    let normal = match plane.as_str() {
        Some("yz") => [1.0, 0.0, 0.0],
        Some("xz") => [0.0, 1.0, 0.0],
        Some("xy") => [0.0, 0.0, 1.0],
        Some(_) => return None,
        None => {
            let origin: Vec3 = serde_json::from_value(plane.get("origin")?.clone()).ok()?;
            let normal: Vec3 = serde_json::from_value(plane.get("normal")?.clone()).ok()?;
            return Some((origin, geom::unit(normal)?));
        }
    };
    Some(([0.0; 3], normal))
}

impl<K: crate::ImportKernel> Importer<'_, K> {
    /// The volume the history shows the item at timeline `index` adding
    /// (negative: removing): its state against the one before.
    pub(crate) fn history_change(&mut self, index: i64) -> Option<f64> {
        let state = self.oracle.state_of_item(index)?;
        let kernel = self.doc.kernel();
        let volume = |s: Vec<Sig>| s.iter().map(|x| x.volume).sum::<f64>();
        let after = volume(self.oracle.try_sigs(kernel, state)?);
        let before = volume(self.oracle.try_sigs(kernel, state.checked_sub(1)?)?);
        Some(after - before)
    }

    /// How much volume the next history state has more than the replay.
    pub(crate) fn change_to_next(&mut self) -> Option<f64> {
        let next = self.oracle.next_index();
        let kernel = self.doc.kernel();
        let target: f64 = self
            .oracle
            .try_sigs(kernel, next)?
            .iter()
            .map(|s| s.volume)
            .sum();
        let now: f64 = self.current_sigs().iter().map(|s| s.volume).sum();
        Some(target - now)
    }

    /// Patterns without decoded inputs: copies of one of the last
    /// extrusions before the item (of the decoded features, `decoded`), as
    /// many as the history's change asks for, about each origin axis
    /// (guesses the history checks). A rectangular pattern's second
    /// direction also at 60° and 120° to the first, and both directions
    /// symmetric too, after the guesses at right angles; a cut's copies are
    /// counted where the replay has material, so that the predicted change
    /// ranks them ([`Importer::copies_in_material`], mitcad#74).
    fn guessed_copies(
        &mut self,
        item: &TimelineItem,
        copies: Copies,
        decoded: Option<Vec<mitcad_model::FeatureUid>>,
    ) -> Result<Vec<Candidate>, String> {
        let none = "no inputs decoded";
        if !self.oracle.enabled {
            return Err(format!("{none} (no history to guess them)"));
        }
        let index = item.index.ok_or(none)?;
        let change = self
            .change_to_next()
            .ok_or_else(|| format!("{none}, and its history state could not be rebuilt"))?;
        let mut recent: Vec<(i64, mitcad_model::FeatureUid)> = self
            .features
            .iter()
            .filter(|(i, _)| **i < index)
            .filter(|(_, f)| decoded.as_ref().is_none_or(|d| d.contains(f)))
            .map(|(i, f)| (*i, *f))
            .collect();
        recent.sort_by_key(|r| std::cmp::Reverse(r.0));
        let mut inputs = Vec::new();
        for (i, uid) in recent {
            let usable = self.doc.feature(uid).is_some_and(|f| match decoded {
                Some(_) => f.def.info().tool_use().is_some(),
                None => matches!(f.def, mitcad_model::FeatureDef::Extrude(_)),
            });
            if !usable {
                continue;
            }
            if let Some(added) = self.history_change(i).filter(|a| a.abs() > 1e-6) {
                inputs.push((uid, i, added));
            }
            if inputs.len() >= PATTERNED_EXTRUSIONS {
                break;
            }
        }
        if inputs.is_empty() {
            return Err(format!("{none}, and no extrusion before it to copy"));
        }
        let mut out = Vec::new();
        for (uid, at, added) in inputs {
            let objects = json!({"type": "features", "features": [uid.to_string()]});
            // An extrusion up to an object is rebuilt at each copy by
            // Adjust (the default) and moved by Identical; by
            // distances both are the same.
            let to_object = self.doc.feature(uid).is_some_and(|f| match &f.def {
                mitcad_model::FeatureDef::Extrude(e) => {
                    use mitcad_model::features::{Extent, Side};
                    let side = |s: &Side| matches!(s, Side::ToObject { .. });
                    match &e.extent {
                        Extent::ToObject { .. } => true,
                        Extent::TwoSides { side1, side2, .. } => side(side1) || side(side2),
                        _ => false,
                    }
                }
                _ => false,
            });
            // The copies the change asks for (overlaps make it a guess),
            // unless the quantity is decoded.
            let estimate = 1.0 + change / added;
            let nearest = estimate.round().clamp(-1.0, 1e3) as i64;
            let quantity = match &copies {
                Copies::Circular { quantity, .. } | Copies::Rectangular { quantity, .. } => {
                    *quantity
                }
            };
            let mut quantities = match quantity {
                Some(q) => vec![q],
                None => vec![nearest, nearest + 1, nearest - 1],
            };
            quantities.retain(|q| (2..=1000).contains(q));
            // What `copies` more copies of the feature add (overlaps aside),
            // so that the likeliest feature and quantity come first. The
            // compute option is not decoded: Adjust, then for an extrusion
            // up to an object Identical (Adjust applies its copies in one
            // boolean operation too where that gives the same bodies).
            let candidate = |def: Value, copies: f64| {
                let one = |def: Value| Candidate {
                    core: CandidateCore::new(def),
                    note: Some(format!(
                        "inputs not decoded: copies of {} checked against the history",
                        uid
                    )),
                    guess: true,
                    predicted: Some(added * copies),
                    input_provenance: crate::InputProvenance::Inferred,
                    first: false,
                };
                let mut out = Vec::new();
                if to_object {
                    let mut identical = def.clone();
                    identical["compute"] = json!("identical");
                    out.push(one(def));
                    out.push(one(identical));
                } else {
                    out.push(one(def));
                }
                out
            };
            match &copies {
                Copies::Circular {
                    angle,
                    symmetric,
                    axis,
                    origin_axes,
                    ..
                } => {
                    let mut axes: Vec<Value> = axis.iter().cloned().collect();
                    if axes.is_empty() || *origin_axes {
                        for a in ["z", "x", "y"] {
                            if !axes.contains(&json!(a)) {
                                axes.push(json!(a));
                            }
                        }
                    }
                    for q in &quantities {
                        for axis in &axes {
                            out.extend(candidate(
                                json!({"type": "circular_pattern",
                                "objects": objects, "axis": axis,
                                "quantity": q, "angle": angle, "symmetric": symmetric,
                                "compute": "adjust"}),
                                (q - 1) as f64,
                            ));
                        }
                    }
                }
                Copies::Rectangular {
                    distance,
                    quantity_two,
                    distance_two,
                    steps,
                    ..
                } => {
                    // A second direction of one element changes nothing.
                    let two = quantity_two
                        .filter(|q| *q > 1)
                        .zip(distance_two.clone().filter(|d| d.as_f64() != Some(0.0)));
                    // The other lattices only for the decoded features: for
                    // the last extrusions they would crowd out the likelier
                    // guesses at right angles (`max_candidates`).
                    let lattices = lattices(
                        &quantities,
                        two.as_ref().map(|t| t.0),
                        *steps,
                        decoded.is_some(),
                    );
                    // A cut's copies change the bodies only where the replay
                    // has material (mitcad#74).
                    let reach = match (added < 0.0).then(|| self.change_centre(at)).flatten() {
                        Some(centre) => {
                            let offsets: Option<Vec<Vec<Vec3>>> =
                                lattices.iter().map(Lattice::offsets).collect();
                            offsets.and_then(|o| self.copies_in_material(centre, &o))
                        }
                        None => None,
                    };
                    if crate::tracing()
                        && let Some(reach) = &reach
                    {
                        let counts: Vec<String> = lattices
                            .iter()
                            .zip(reach)
                            .map(|(l, n)| format!("{} {n:.0}", l.label()))
                            .collect();
                        eprintln!(
                            "import: {}: copies of {uid} in the material: {}",
                            item.name.clone().flatten().unwrap_or_default(),
                            counts.join(", ")
                        );
                    }
                    for (k, lattice) in lattices.iter().enumerate() {
                        let copies = reach.as_ref().map_or(lattice.copies() as f64, |r| r[k]);
                        let def = lattice.def(&objects, distance, two.as_ref().map(|t| &t.1));
                        out.extend(candidate(def, copies));
                    }
                }
            }
        }
        if out.is_empty() {
            return Err(format!(
                "{none}, and no extrusion before it matches the history's change"
            ));
        }
        Ok(out)
    }

    /// The centre of the material the item at timeline `index` removed (or
    /// added): the change of the moments of its history state's solids
    /// against the state before, over the change of their volume.
    fn change_centre(&mut self, index: i64) -> Option<Vec3> {
        let state = self.oracle.state_of_item(index)?;
        let kernel = self.doc.kernel();
        let moment = |sigs: Vec<Sig>| {
            sigs.iter().fold(([0.0; 3], 0.0), |(m, v), s| {
                (geom::add(m, geom::scale(s.center, s.volume)), v + s.volume)
            })
        };
        let (after, v1) = moment(self.oracle.try_sigs(kernel, state)?);
        let (before, v0) = moment(self.oracle.try_sigs(kernel, state.checked_sub(1)?)?);
        let change = v1 - v0;
        (change.abs() > 1e-6).then(|| geom::scale(geom::sub(after, before), 1.0 / change))
    }

    /// Copies of a cut that removed the material around `centre`, placed
    /// at each set of offsets from it: for each set, about how many copies
    /// have their centre in the replay's material, where a copy removes
    /// some: those with the centre in the bodies' boxes, times the share
    /// of them (of at most [`MAX_SAMPLED`] spread over them) in the
    /// material. A large pattern over a round plate has most copies off
    /// it, and the right directions put more copies on it than a lattice
    /// whose copies overlap (classifying points against the next state
    /// instead takes about 10 ms a point on a plate of a thousand holes).
    /// None when the kernel does not classify points, the bodies have too
    /// many faces to classify enough points quickly ([`CLASSIFIED`]), or
    /// the centre is in the material (the cut did not remove it: a ring).
    fn copies_in_material(&mut self, centre: Vec3, sets: &[Vec<Vec3>]) -> Option<Vec<f64>> {
        let kernel = self.doc.kernel();
        let bodies: Vec<&K::Shape> = crate::body_scope::bodies(self.doc)
            .iter()
            .map(|b| b.shape)
            .collect();
        let mut faces = 0;
        let mut boxes = Vec::new();
        for body in &bodies {
            faces += kernel.face_count(body).ok()?;
            boxes.extend(kernel.bounding_box(body).ok()?);
        }
        // About a second of classifying (a few microseconds a point and face).
        let each = CLASSIFIED / faces.max(1) / sets.len().max(1);
        if each < MIN_SAMPLED {
            return None;
        }
        let in_box = |p: Vec3| {
            boxes
                .iter()
                .any(|b| (0..3).all(|i| b.min[i] <= p[i] && p[i] <= b.max[i]))
        };
        let mut points = vec![centre];
        let mut picked: Vec<(usize, std::ops::Range<usize>)> = Vec::new();
        for set in sets {
            let near: Vec<Vec3> = set
                .iter()
                .map(|o| geom::add(centre, *o))
                .filter(|p| in_box(*p))
                .collect();
            let take = near.len().min(each.min(MAX_SAMPLED));
            let first = points.len();
            points.extend((0..take).map(|k| near[k * near.len() / take]));
            picked.push((near.len(), first..points.len()));
        }
        let mut inside = vec![false; points.len()];
        for body in &bodies {
            let found = kernel.points_inside(body, &points).ok()?;
            inside.iter_mut().zip(found).for_each(|(a, b)| *a |= b);
        }
        if inside[0] {
            return None;
        }
        Some(
            picked
                .into_iter()
                .map(|(near, range)| {
                    let sampled = range.len().max(1) as f64;
                    near as f64 * range.filter(|&i| inside[i]).count() as f64 / sampled
                })
                .collect(),
        )
    }

    /// A mirror of bodies without decoded inputs (guesses the history
    /// checks): the bodies of the item's component that the next state has
    /// twins of (the same volume and area) as new bodies; each body joined
    /// with its mirror image (a body the next state no longer has); each
    /// body as a new body. About the decoded plane, else each origin plane
    /// and the planes a body and its image in the next state are symmetric
    /// about (from their centres).
    fn mirrored_bodies(&mut self, plane: Option<Value>) -> Vec<Candidate> {
        if !self.oracle.enabled {
            return Vec::new();
        }
        let planes = match plane {
            Some(p) => vec![p],
            None => vec![json!("yz"), json!("xz"), json!("xy")],
        };
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, Sig)> = crate::body_scope::bodies(self.doc)
            .iter()
            .filter(|b| self.doc.body_component(b.uid) == Some(self.component))
            .filter_map(|b| Some((b.uid, Sig::of(kernel, b.shape)?)))
            .collect();
        let next = self.oracle.next_index();
        let target = self.oracle.try_sigs(kernel, next).unwrap_or_default();
        let current = self.current_sigs();
        let mut used = vec![false; current.len()];
        // The next state's bodies the replay does not have, and the
        // replay's bodies it no longer has.
        let new: Vec<Sig> = target
            .iter()
            .filter(
                |t| match (0..current.len()).find(|&j| !used[j] && current[j].same(t)) {
                    Some(j) => {
                        used[j] = true;
                        false
                    }
                    None => true,
                },
            )
            .copied()
            .collect();
        let gone: Vec<BodyUid> = bodies
            .iter()
            .filter(|(_, s)| !target.iter().any(|t| t.same(s)))
            .map(|(b, _)| *b)
            .collect();
        let rel = |x: f64, y: f64| (x - y).abs() / x.abs().max(y.abs()).max(1e-9);
        let twin = |a: &Sig, b: &Sig| rel(a.volume, b.volume) < 1e-4 && rel(a.area, b.area) < 1e-4;
        // The plane a body and its mirror image are symmetric about: half way
        // between their centres (a copy), or through the centre of the two
        // joined (a body twice as large).
        let between = |from: [f64; 3], to: [f64; 3]| -> Option<(Vec3, Vec3)> {
            let normal = geom::unit(geom::sub(to, from))?;
            Some((geom::scale(geom::add(from, to), 0.5), normal))
        };
        let mut derived: Vec<(Vec3, Vec3)> = Vec::new();
        for (_, s) in &bodies {
            for n in &new {
                let plane = if twin(n, s) {
                    between(s.center, n.center)
                } else if rel(n.volume, 2.0 * s.volume) < 1e-3 {
                    // The centre of the joined body is on the plane.
                    between(s.center, geom::sub(geom::scale(n.center, 2.0), s.center))
                } else {
                    None
                };
                if let Some(p) = plane
                    && !derived.iter().any(|q| {
                        geom::norm(geom::cross(p.1, q.1)) < 1e-6
                            && geom::dot(geom::sub(p.0, q.0), q.1).abs() < 1e-4
                    })
                {
                    derived.push(p);
                }
            }
        }
        // Each plane with its geometry where it is known.
        let mut planes: Vec<(Value, Option<(Vec3, Vec3)>)> = planes
            .into_iter()
            .map(|p| {
                let geometry = plane_geometry(&p);
                (p, geometry)
            })
            .collect();
        for (origin, normal) in derived.into_iter().take(3) {
            planes.push((self.plane_at(origin, normal), Some((origin, normal))));
        }
        // A body joined with its mirror image (mitcad#88) holds the body and
        // the image and is symmetric about the plane: a new body of the next
        // state can be that join only with a volume from the body's to twice
        // it and its centre on the plane. Without one the guess cannot help;
        // for a nearly symmetric body it was also the slowest (the body and
        // its image lie on each other almost everywhere).
        let joinable = |body: &Sig, plane: &Option<(Vec3, Vec3)>| {
            new.iter().any(|n| {
                let size = n.volume.abs().cbrt().max(1e-3);
                n.volume >= body.volume * (1.0 - RELATIVE)
                    && n.volume <= 2.0 * body.volume * (1.0 + RELATIVE)
                    && plane.is_none_or(|(origin, normal)| {
                        geom::dot(geom::sub(n.center, origin), normal).abs() <= JOINED_CENTRE * size
                    })
            })
        };
        let mut out = Vec::new();
        let push =
            |out: &mut Vec<Candidate>, objects: Vec<String>, joined: Option<&Sig>, volume: f64| {
                let combine = joined.is_some();
                for (plane, geometry) in &planes {
                    if joined.is_some_and(|body| !joinable(body, geometry)) {
                        continue;
                    }
                    out.push(Candidate {
                        core: CandidateCore::new(json!({"type": "mirror",
                        "objects": {"type": "bodies", "bodies": objects},
                        "plane": plane, "combine": combine, "compute": "adjust"})),
                        note: Some(format!(
                            "inputs not decoded: {} of {} checked against the history",
                            if combine { "a joined mirror" } else { "copies" },
                            objects.join(", ")
                        )),
                        guess: true,
                        predicted: Some(volume),
                        input_provenance: crate::InputProvenance::Inferred,
                        first: false,
                    });
                }
            };
        let twins: Vec<(BodyUid, Sig)> = bodies
            .iter()
            .filter(|(_, s)| new.iter().any(|n| twin(n, s)))
            .copied()
            .collect();
        if !twins.is_empty() && twins.len() <= new.len() {
            let volume = twins.iter().map(|(_, s)| s.volume).sum();
            push(
                &mut out,
                twins.iter().map(|(b, _)| b.to_string()).collect(),
                None,
                volume,
            );
        }
        for (body, sig) in bodies.iter().filter(|(b, _)| gone.contains(b)).take(8) {
            push(&mut out, vec![body.to_string()], Some(sig), sig.volume);
        }
        for (body, sig) in bodies.iter().take(8) {
            push(&mut out, vec![body.to_string()], None, sig.volume);
        }
        out
    }

    /// A mirror of features without decoded inputs: sets of up to four of
    /// the features before the item that MitCad can mirror (they make a
    /// tool body), whose volume changes in the history add up to the next
    /// state's change, about the decoded plane, else each origin plane
    /// (guesses the history checks, the closest sums first).
    fn mirrored_features(&mut self, item: &TimelineItem, plane: Option<Value>) -> Vec<Candidate> {
        let (Some(index), true) = (item.index, self.oracle.enabled) else {
            return Vec::new();
        };
        let Some(change) = self.change_to_next().filter(|c| c.abs() > 1e-6) else {
            return Vec::new();
        };
        let mut recent: Vec<(i64, mitcad_model::FeatureUid)> = self
            .features
            .iter()
            .filter(|(i, _)| **i < index)
            .map(|(i, f)| (*i, *f))
            .collect();
        recent.sort_by_key(|r| std::cmp::Reverse(r.0));
        let mut inputs: Vec<(mitcad_model::FeatureUid, f64)> = Vec::new();
        for (i, uid) in recent {
            let mirrorable = self
                .doc
                .feature(uid)
                .is_some_and(|f| f.def.info().tool_use().is_some());
            if !mirrorable {
                continue;
            }
            if let Some(added) = self.history_change(i).filter(|a| a.abs() > 1e-6) {
                inputs.push((uid, added));
            }
            if inputs.len() >= 10 {
                break;
            }
        }
        // Subsets by how close their sum comes to the change.
        let mut sets: Vec<(f64, Vec<usize>)> = Vec::new();
        for k in 1..=inputs.len().min(4) {
            features::combinations(inputs.len(), k, 2000, &mut |picked: &[usize]| {
                let sum: f64 = picked.iter().map(|&p| inputs[p].1).sum();
                let off = (sum - change).abs() / change.abs();
                if off < 0.05 {
                    sets.push((off, picked.to_vec()));
                }
            });
        }
        sets.sort_by(|a, b| a.0.total_cmp(&b.0));
        let planes = match plane {
            Some(p) => vec![p],
            None => vec![json!("yz"), json!("xz"), json!("xy")],
        };
        let mut out = Vec::new();
        for (_, picked) in sets.into_iter().take(4) {
            let uids: Vec<String> = picked.iter().map(|&p| inputs[p].0.to_string()).collect();
            let sum: f64 = picked.iter().map(|&p| inputs[p].1).sum();
            for plane in &planes {
                out.push(Candidate {
                    core: CandidateCore::new(json!({"type": "mirror",
                        "objects": {"type": "features", "features": uids},
                        "plane": plane, "compute": "adjust"})),
                    note: Some(format!(
                        "inputs not decoded: a mirror of {} checked against the history",
                        uids.join(", ")
                    )),
                    guess: true,
                    predicted: Some(sum),
                    input_provenance: crate::InputProvenance::Inferred,
                    first: false,
                });
            }
        }
        out
    }

    /// A plane through `origin` with `normal`: a planar face of the
    /// replay's bodies that lies on it, else the plane itself, fixed.
    fn plane_at(&self, origin: Vec3, normal: Vec3) -> Value {
        let on = refs::planar_faces(self.doc).into_iter().find(|(_, _, p)| {
            geom::norm(geom::cross(p.normal, normal)) < 1e-6
                && geom::dot(geom::sub(p.origin, origin), normal).abs() < 1e-4
        });
        match on {
            Some((body, face, _)) => json!({"body": body.to_string(), "face": face.to_string()}),
            None => json!({"origin": origin, "normal": normal}),
        }
    }

    /// A value from a reflected parameter reference.
    fn value_of(&self, v: Option<&Value>) -> Option<Value> {
        let r = reference(v?)?;
        self.value(&Some(r))
    }

    /// The links of a combine's tools in another component than its
    /// target's (mitcad#104): the occurrence paths to the tool's component
    /// and to the target's, each placed once.
    fn tool_links(&self, target: BodyUid, tools: &[String]) -> Result<Map<String, Value>, String> {
        let assembly = self.doc.assembly();
        let only_path = |c| match assembly.paths_to(c).as_slice() {
            [path] => Ok(path
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/")),
            _ => Err(format!(
                "a tool of another component than the target's, and {} is not placed once",
                assembly.name(c)
            )),
        };
        let own = self.doc.body_component(target);
        let mut links = Map::new();
        for tool in tools {
            let Ok(uid) = tool.parse::<BodyUid>() else {
                continue;
            };
            match (self.doc.body_component(uid), own) {
                (Some(c), Some(own)) if c != own => {
                    // The occurrence the file's reference to the tool is in,
                    // where its component is placed more than once.
                    let source = match self.hinted_path(uid) {
                        Some(path) => path
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("/"),
                        None => only_path(c)?,
                    };
                    links.insert(
                        tool.clone(),
                        json!({"source": source, "target": only_path(own)?}),
                    );
                }
                _ => {}
            }
        }
        Ok(links)
    }

    fn body_of(&self, r: &Reference) -> Option<BodyUid> {
        match r {
            Reference::Body(fp) => self.body_ref(fp),
            _ => None,
        }
    }

    /// Pattern and mirror objects as decoded: without the fillets and
    /// chamfers among the features (the copies of the other features carry
    /// them where those made the edges they round), and with them where
    /// there are any and all of them came in parametric (the pattern
    /// repeats them on its copies, mitcad#105), true. Where the history
    /// checks the item, without them first: repeating fillets on many
    /// copies takes long, and most such items' states are the copies
    /// without them (the fillets' edges are not on the copies); without a
    /// history, with them first.
    fn object_variants(
        &self,
        inputs: &[Reference],
    ) -> Result<Vec<(Value, bool, InputProvenance)>, String> {
        let without = self.objects(inputs, false)?;
        // The reduced variant infers a selection only when it actually
        // omits a selected dressup. An all-dressup selection is kept.
        let omitted = inputs.iter().any(is_dressup) && inputs.iter().any(|r| !is_dressup(r));
        let provenance = if omitted {
            InputProvenance::Inferred
        } else {
            InputProvenance::Unknown
        };
        let mut out = vec![(without.clone(), false, provenance)];
        if let Ok(with) = self.objects(inputs, true)
            && with != without
        {
            if self.oracle.enabled {
                out.push((with, true, InputProvenance::Unknown));
            } else {
                out.insert(0, (with, true, InputProvenance::Unknown));
            }
        }
        // Faces with the faces between neighbours they lost (mitcad#719),
        // first, the selection as it is after them.
        if let Some(bridged) = self.bridged(inputs, &without) {
            out.insert(0, (bridged, false, InputProvenance::Inferred));
        }
        // The tapped holes without their threads, last (mitcad#733: a
        // rectangular pattern of holes tapped with a taper pipe thread
        // whose copies the file keeps as plain bores, while a circular
        // pattern of the same holes tapers its copies).
        let threads: Vec<Value> = inputs
            .iter()
            .filter_map(|r| match r {
                Reference::Feature(f) if f.object_type.as_deref() == Some("HoleFeature") => {
                    f.timeline_index.flatten()
                }
                _ => None,
            })
            .filter_map(|i| self.dressups.get(&i))
            .flatten()
            .map(|uid| json!(uid.to_string()))
            .collect();
        if !threads.is_empty() {
            let bare: Vec<(Value, bool, InputProvenance)> = out
                .iter()
                .filter_map(|(objects, with, _)| {
                    let mut objects = objects.clone();
                    objects["features"]
                        .as_array_mut()?
                        .retain(|f| !threads.contains(f));
                    Some((objects, *with, InputProvenance::Inferred))
                })
                .collect();
            out.extend(bare);
        }
        Ok(out)
    }

    /// A selection of faces (`objects`, of the face references among
    /// `inputs`, in their order) with the faces between selected faces
    /// whose recipes name each other as neighbours
    /// ([`Importer::bridging_faces`]); None where there are none.
    fn bridged(&self, inputs: &[Reference], objects: &Value) -> Option<Value> {
        if objects["type"] != "faces" {
            return None;
        }
        let body: BodyUid = objects["body"].as_str()?.parse().ok()?;
        let names: Vec<String> = objects["faces"]
            .as_array()?
            .iter()
            .filter_map(|f| f.as_str().map(str::to_owned))
            .collect();
        let fingerprints: Vec<&Fingerprint> = inputs
            .iter()
            .filter_map(|r| match r {
                Reference::Face(fp) => Some(&**fp),
                _ => None,
            })
            .collect();
        if fingerprints.len() != names.len() {
            return None;
        }
        let faces: Vec<(BodyUid, String)> = names.into_iter().map(|n| (body, n)).collect();
        let between = self.bridging_faces(&fingerprints, &faces);
        if between.is_empty() {
            return None;
        }
        let mut bridged = objects.clone();
        if let Some(list) = bridged["faces"].as_array_mut() {
            list.extend(between.into_iter().map(|(_, f)| json!(f)));
        }
        Some(bridged)
    }

    /// Pattern and mirror objects: features, bodies or faces of one body.
    /// Fillets and chamfers among the features only `with_dressups` (each
    /// with the features made for it, which must all be fillets and
    /// chamfers), else left out (where there are others).
    fn objects(&self, inputs: &[Reference], with_dressups: bool) -> Result<Value, String> {
        let mut features = Vec::new();
        let mut bodies = Vec::new();
        let mut faces: Vec<(BodyUid, String)> = Vec::new();
        // Fillets and chamfers among the features change the edges of
        // the others and leave no tool to copy: the copies of the others
        // carry them where they made those edges (mitcad#96), or the
        // pattern repeats them on its copies (mitcad#105).
        let kept: Vec<&Reference> = inputs.iter().filter(|r| !is_dressup(r)).collect();
        let inputs = if kept.is_empty() || with_dressups {
            inputs.iter().collect()
        } else {
            kept
        };
        for r in inputs {
            match r {
                Reference::Feature(f) if with_dressups && is_dressup(r) => {
                    let uids = f
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.dressups.get(&i))
                        .ok_or("a patterned fillet or chamfer did not come in parametric")?;
                    features.extend(uids.iter().map(ToString::to_string));
                }
                // An item that only brings bodies (a paste, a base feature,
                // a derive) leaves no tool to copy: its pattern copies the
                // bodies it brought (mitcad#641).
                Reference::Feature(f) if brings_bodies(f.object_type.as_deref()) => {
                    let index = f
                        .timeline_index
                        .flatten()
                        .ok_or("a patterned item without its place on the timeline")?;
                    let made: Vec<BodyUid> = self
                        .made_by(index)
                        .0
                        .into_iter()
                        .filter(|b| crate::body_scope::shape(self.doc, *b).is_some())
                        .collect();
                    if made.is_empty() {
                        return Err(format!(
                            "the bodies {} brought were not found in the replay",
                            f.name
                                .clone()
                                .flatten()
                                .unwrap_or_else(|| "the patterned item".to_owned())
                        ));
                    }
                    bodies.extend(made.iter().map(ToString::to_string));
                }
                Reference::Feature(f) => {
                    let index = f.timeline_index.flatten();
                    let uid = index
                        .and_then(|i| self.features.get(&i))
                        .ok_or(PATTERNED_NOT_IMPORTED)?;
                    features.push(uid.to_string());
                    // A tapped hole's thread with it (mitcad#719).
                    if f.object_type.as_deref() == Some("HoleFeature")
                        && let Some(threads) = index.and_then(|i| self.dressups.get(&i))
                    {
                        features.extend(threads.iter().map(ToString::to_string));
                    }
                }
                Reference::Body(fp) => bodies.push(
                    self.body_ref(fp)
                        .ok_or("a body was not found in the replay")?
                        .to_string(),
                ),
                Reference::Face(fp) => {
                    let (b, f) = self
                        .face_input(fp)
                        .ok_or("a face was not found in the replay")?;
                    faces.push((b, f.to_string()));
                }
                _ => return Err("an input of an unsupported kind".to_owned()),
            }
        }
        match (features.is_empty(), bodies.is_empty(), faces.is_empty()) {
            (false, true, true) => Ok(json!({"type": "features", "features": features})),
            (true, false, true) => Ok(json!({"type": "bodies", "bodies": bodies})),
            (true, true, false) => {
                let body = faces[0].0;
                if faces.iter().any(|(b, _)| *b != body) {
                    return Err("faces of several bodies".to_owned());
                }
                let names: Vec<String> = faces.into_iter().map(|(_, f)| f).collect();
                Ok(json!({"type": "faces", "body": body.to_string(), "faces": names}))
            }
            _ => Err("no inputs, or inputs of several kinds".to_owned()),
        }
    }

    /// The faces between two selected faces whose recipes name each other
    /// as neighbours but which the replay's body has apart (mitcad#719: a
    /// corner of the sketch rounded after the faces were selected; the
    /// file's pattern of the faces copies the rounding's face with them,
    /// its stored result has it on every copy). A gap takes the one face
    /// that meets both and no other selected face, else none.
    fn bridging_faces(
        &self,
        inputs: &[&Fingerprint],
        faces: &[(BodyUid, String)],
    ) -> Vec<(BodyUid, String)> {
        let named: Vec<Vec<Vec<EntityName>>> = inputs
            .iter()
            .map(|fp| {
                fp.f3d
                    .as_ref()
                    .and_then(|f| f.entities.clone())
                    .unwrap_or_default()
            })
            .collect();
        bridging_faces(&named, faces, &mut |body| {
            let Some(shape) = crate::body_scope::shape(self.doc, body) else {
                return Vec::new();
            };
            self.doc
                .kernel()
                .edges(shape)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|edge| edge.name?.parse::<mitcad_model::EdgeName>().ok())
                .map(|edge| edge.faces().clone().map(|f| f.to_string()))
                .collect()
        })
    }

    /// An axis reference (`GeomRef`): origin or construction axis, edge or
    /// face.
    fn axis_ref(&self, r: &Reference) -> Option<Value> {
        match r {
            Reference::ConstructionAxis(c) => match c.origin.clone().flatten().as_deref() {
                Some(o @ ("X" | "Y" | "Z")) => Some(json!(o.to_lowercase())),
                _ => {
                    if let Some(uid) = c
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.features.get(&i))
                    {
                        return Some(json!(uid.to_string()));
                    }
                    fixed_axis(c.geometry.as_ref()?)
                }
            },
            Reference::Edge(fp) => {
                let Some((body, edge)) = refs::resolve_edge(self.doc, fp) else {
                    if crate::tracing() {
                        self.trace_edges_near(fp, &|_| true);
                    }
                    return None;
                };
                Some(json!({"body": body.to_string(), "edge": edge.to_string()}))
            }
            Reference::Face(fp) => {
                // The axis or surface is all a datum needs of a face: where
                // the replay has no part of the face at the point the file
                // found it (a later feature cut it away), a face on the same
                // cylinder.
                let Some((body, face)) = self
                    .face_input(fp)
                    .or_else(|| refs::resolve_cylinder_surface(self.doc, fp))
                else {
                    if crate::tracing() {
                        eprintln!("import: a face input is not in the replay");
                        self.trace_nearest_faces(fp, &|_| true);
                    }
                    return None;
                };
                Some(json!({"body": body.to_string(), "face": face.to_string()}))
            }
            _ => None,
        }
    }

    /// A circular pattern's axis: [`Importer::pattern_axis`], except that a
    /// circle or an arc of a sketch (the axis through its centre normal to
    /// the sketch) turns the way the sketch faces, which the file's stored
    /// axis may not: then the stored line is the axis, so that a partial
    /// pattern goes the file's way round.
    fn circular_axis(&self, axis: Option<&Value>, stored: Option<&Value>) -> Option<Value> {
        let r = axis.and_then(reference)?;
        let named = self.pattern_axis(&r)?;
        let round = matches!(&r, Reference::SketchEntity(e)
            if e.object_type.as_deref().is_some_and(|t| t == "SketchCircle" || t == "SketchArc"));
        if !round {
            return Some(named);
        }
        let facing = (|| {
            let uid = named.get("sketch")?.as_str()?.parse::<FeatureUid>().ok()?;
            Some(geom::normal(&self.doc.sketch_output(uid)?.frame))
        })();
        let line = decoded_line(stored);
        let direction = stored
            .and_then(|s| serde_json::from_value::<Vec3>(s.get("direction")?.clone()).ok())
            .and_then(geom::unit);
        match (facing, direction, line) {
            (Some(n), Some(d), Some(line)) if geom::dot(n, d) < 0.0 => Some(line),
            _ => Some(named),
        }
    }

    /// A circular pattern's angle the way round the file turns: negated
    /// where the axis resolves in the replay against the file's stored axis
    /// (`_f3d_axis`, the line the file turns about). MitCad's axis of a
    /// circular edge and its origin axes point the way of their largest
    /// coordinate, whichever way the file's entity faces (mitcad#719: an
    /// arc edge and the origin Y axis stored as (0, -1, 0), the copies
    /// turned off the body the other way).
    fn circular_angle(&self, axis: &Value, angle: Value, stored: Option<&Value>) -> Value {
        let stored = stored
            .and_then(|s| serde_json::from_value::<Vec3>(s.get("direction")?.clone()).ok())
            .and_then(geom::unit);
        let resolved = serde_json::from_value::<mitcad_model::GeomRef>(axis.clone())
            .ok()
            .and_then(|r| self.doc.resolve_axis(&r).ok());
        match (stored, resolved) {
            (Some(s), Some(a)) if geom::dot(s, a.direction) < -0.5 => match angle {
                Value::String(s) => json!(format!("-({s})")),
                Value::Number(n) => n.as_f64().map_or(Value::Number(n), |x| json!(-x)),
                other => other,
            },
            _ => angle,
        }
    }

    /// A pattern axis: an axis reference, or a line of an imported sketch.
    pub(crate) fn pattern_axis(&self, r: &Reference) -> Option<Value> {
        if let Reference::SketchEntity(e) = r {
            let s = self.sketches.get(&e.sketch_timeline_index.flatten()?)?;
            let curve = s.ids.get(&e.id.clone().flatten()?)?;
            return Some(json!({"sketch": s.uid.to_string(), "curve": curve}));
        }
        self.axis_ref(r)
    }

    /// One decoded rectangular direction, with its stored vector when
    /// the named axis cannot be resolved in the replay.
    fn rectangular_direction(&self, d: &Map<String, Value>, one: bool) -> Option<Value> {
        self.rectangular_direction_by(d, one, false)
    }

    /// [`Importer::rectangular_direction`], with the stored vector as a
    /// fixed direction before the entity where `vector_first` (the
    /// entity's direction in the replay need not point where the file's
    /// does: an edge's).
    fn rectangular_direction_by(
        &self,
        d: &Map<String, Value>,
        one: bool,
        vector_first: bool,
    ) -> Option<Value> {
        let [entity, vector, quantity, distance, symmetric] = if one {
            [
                "directionOneEntity",
                "directionOne",
                "quantityOne",
                "distanceOne",
                "isSymmetricInDirectionOne",
            ]
        } else {
            [
                "directionTwoEntity",
                "directionTwo",
                "quantityTwo",
                "distanceTwo",
                "isSymmetricInDirectionTwo",
            ]
        };
        let named = || {
            d.get(entity)
                .and_then(reference)
                .and_then(|r| self.pattern_axis(&r))
        };
        let stored = || {
            let v: [f64; 3] = serde_json::from_value(d.get(vector)?.clone()).ok()?;
            Some(json!({"origin": [0.0, 0.0, 0.0], "direction": geom::unit(v)?}))
        };
        let axis = if vector_first {
            stored().or_else(named)
        } else {
            named().or_else(stored)
        }?;
        let mut distance = self.value_of(d.get(distance))?;
        // An origin axis whose stored vector points the other way
        // (mitcad#641: a pattern along Z stored with the vector (0, 0, -1)
        // and a negative distance copies upwards): the copies go against the
        // axis, as a negated distance takes them.
        // So too an edge or an axis of its own whose direction the replay
        // resolves the other way (mitcad#719, as circular patterns do:
        // `Importer::circular_angle`).
        let along = match axis.as_str() {
            Some("x") => Some([1.0, 0.0, 0.0]),
            Some("y") => Some([0.0, 1.0, 0.0]),
            Some("z") => Some([0.0, 0.0, 1.0]),
            _ => serde_json::from_value::<mitcad_model::GeomRef>(axis.clone())
                .ok()
                .and_then(|r| self.doc.resolve_axis(&r).ok())
                .map(|a| a.direction),
        };
        let stored_vector: Option<[f64; 3]> = d
            .get(vector)
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        if let (Some(a), Some(v)) = (along, stored_vector)
            && geom::dot(a, v) < -0.5
        {
            distance = match distance {
                Value::String(s) => json!(format!("-({s})")),
                Value::Number(n) => json!(-n.as_f64()?),
                other => other,
            };
        }
        Some(
            json!({"axis": axis, "quantity": self.value_of(d.get(quantity))?,
            "distance": distance,
            "symmetric": d.get(symmetric).and_then(Value::as_bool).unwrap_or(false)}),
        )
    }

    /// A plane reference (`GeomRef`): origin plane, planar face, or a fixed
    /// plane from a construction plane's geometry.
    pub(crate) fn plane_ref(&self, r: &Reference) -> Option<Value> {
        match r {
            Reference::ConstructionPlane(c) => match c.origin.clone().flatten().as_deref() {
                Some(o @ ("XY" | "XZ" | "YZ")) => Some(json!(o.to_lowercase())),
                _ => {
                    if let Some(uid) = c
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.features.get(&i))
                    {
                        return Some(json!(uid.to_string()));
                    }
                    let g = c.geometry.as_ref()?;
                    let mut v = json!({"origin": mm3(g.origin?), "normal": geom::unit(g.normal?)?});
                    if let Some(x) = g.u_direction.and_then(geom::unit) {
                        v["x_axis"] = json!(x);
                    }
                    Some(v)
                }
            },
            Reference::Face(fp) => {
                let (body, face) = self
                    .face_input(fp)
                    .or_else(|| refs::resolve_face_by_plane(self.doc, fp))?;
                self.hint_path(fp, body);
                Some(json!({"body": body.to_string(), "face": face.to_string()}))
            }
            // A profile: the plane of its sketch, fixed where it is.
            Reference::Profile(p) => {
                let info = self.sketches.get(&p.sketch_timeline_index.flatten()?)?;
                let frame = self.doc.sketch_output(info.uid)?.frame;
                Some(
                    json!({"origin": frame.origin, "normal": geom::normal(&frame),
                            "x_axis": frame.x_axis}),
                )
            }
            _ => None,
        }
    }

    /// A mirror or pattern whose decoded objects and transformation
    /// resolve in the replay.
    fn explicit_copies(&self, object_type: &str, d: &Map<String, Value>) -> bool {
        let get = |k: &str| d.get(k);
        if self
            .objects(&references(get("inputEntities")), false)
            .is_err()
        {
            return false;
        }
        match object_type {
            "MirrorFeature" => get("mirrorPlane")
                .and_then(reference)
                .and_then(|r| self.plane_ref(&r))
                .is_some(),
            "CircularPatternFeature" => {
                self.circular_axis(get("axis"), get("_f3d_axis"))
                    .or_else(|| decoded_line(get("_f3d_axis")))
                    .is_some()
                    && self.value_of(get("quantity")).is_some()
                    && self.value_of(get("totalAngle")).is_some()
            }
            "RectangularPatternFeature" => self.rectangular_direction(d, true).is_some(),
            _ => false,
        }
    }

    /// An item from its decoded inputs; where the file's history checks the
    /// result, followed by what the history suggests without those inputs
    /// (mitcad#67), so that inputs decoded wrongly or not found in the
    /// replay never make the item worse than guessing them.
    pub fn translate_op(
        &mut self,
        object_type: &str,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let mut candidates = self.op_candidates(object_type, item)?;
        for c in &mut candidates {
            c.defs.iter_mut().for_each(on_original_bodies);
        }
        // A definition both decoded and guessed is tried once.
        let mut seen: Vec<Vec<Value>> = Vec::new();
        candidates.retain(|c| {
            let new = !seen.contains(&c.defs);
            if new {
                seen.push(c.defs.clone());
            }
            new
        });
        // Where the history checks the item, a pattern or mirror that
        // repeats its fillets and chamfers comes after the guesses too: on
        // large bodies its roundings take tens of seconds, and the guesses
        // often give the state first (mitcad#105).
        if self.oracle.enabled {
            candidates.sort_by_key(|c| c.note.as_deref() == Some(DRESSED));
        }
        Ok(candidates)
    }

    /// [`Importer::translate_op`]'s candidates as decoded and guessed.
    fn op_candidates(
        &mut self,
        object_type: &str,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let explicit = self.translate_op_inputs(object_type, item);
        let decoded: &[&str] = match object_type {
            "CombineFeature" => &["targetBody", "toolBodies"],
            "HoleFeature" => &["position", "_f3d_positions"],
            "MirrorFeature" | "CircularPatternFeature" | "RectangularPatternFeature" => {
                &["inputEntities"]
            }
            _ => &[],
        };
        let map = detail_map(item);
        let guessed_too = self.oracle.enabled && decoded.iter().any(|k| map.contains_key(*k));
        // A hole's guesses come after its decoded definitions have been
        // tried, where the guesses are asked for in parts (mitcad#719): they
        // take the material its state removes, a boolean of the replay's
        // bodies with the state's, which took minutes and gigabytes on a
        // body with modelled threads, where the decoded definition was the
        // file's. The first part is the decoded definitions, the second the
        // guesses; there is none after those (nor after the first where
        // nothing is guessed).
        let mut lazy_hole = false;
        if object_type == "HoleFeature"
            && let Some(g) = self.guesses.as_mut()
        {
            if guessed_too && !g.named && explicit.as_ref().is_ok_and(|c| !c.is_empty()) {
                g.named = true;
                return explicit;
            }
            lazy_hole = g.named;
            g.done = true;
        }
        if !guessed_too {
            return explicit;
        }
        // A combine's operation the `.ipt` decoder read from the record's
        // layout (`_ipt_inputs`, mitcad#60) is not guessed: a guessed cut
        // of a coil's body took gigabytes.
        let operation = (object_type == "CombineFeature"
            && map.get("_ipt_inputs").and_then(Value::as_bool) == Some(true))
        .then(|| match map.get("operation").and_then(Value::as_str) {
            Some(o) if o.starts_with("Cut") => "cut",
            Some(o) if o.starts_with("Intersect") => "intersect",
            _ => "join",
        });
        let mut without = map;
        for k in decoded {
            without.remove(*k);
        }
        // What else mitcad#67 decodes for these items: directions, and axes
        // and planes that are edges or faces.
        for k in [
            "patternEntityType",
            "directionOneEntity",
            "directionTwoEntity",
            "directionOne",
            "directionTwo",
        ] {
            without.remove(k);
        }
        for k in ["axis", "mirrorPlane"] {
            if without
                .get(k)
                .and_then(|v| v.get("kind"))
                .and_then(Value::as_str)
                .is_some_and(|kind| matches!(kind, "edge" | "face"))
            {
                without.remove(k);
            }
        }
        if crate::tracing()
            && let Err(e) = &explicit
        {
            eprintln!(
                "import: {}: the decoded inputs give none: {e}",
                item.name.clone().flatten().unwrap_or_default()
            );
        }
        // Patterns whose inputs the file gives are not searched for
        // (mitcad#181): of the settled patterns whose decoded definition was
        // read, none took a guess (the state differs where an earlier item
        // is approximate, which no other feature or direction repairs), while
        // the guesses of the ones the history rejected took the item's
        // whole time (twenty definitions of a large body's copies, and the
        // probing for them). Nor is a pattern whose decoded feature did not
        // come in: the copies of another feature are not the file's.
        if pattern_not_searched(object_type, &explicit) {
            return explicit;
        }
        let mut guessed = item.clone();
        guessed.detail = Some(Detail::typed(Some(object_type), without));
        let guesses = self
            .translate_op_inputs(object_type, &guessed)
            .map(|mut b| {
                if let Some(o) = operation {
                    b.retain(|c| c.defs[0]["operation"] == o);
                }
                b
            })
            .and_then(|b| {
                if b.is_empty() {
                    Err("no guess of its operation".to_owned())
                } else {
                    Ok(b)
                }
            });
        // (A hole's second part: its guesses alone.)
        if lazy_hole {
            return guesses.map(|b| {
                let given: Vec<Vec<Value>> = explicit
                    .map(|a| a.into_iter().map(|c| c.defs.clone()).collect())
                    .unwrap_or_default();
                b.into_iter().filter(|c| !given.contains(&c.defs)).collect()
            });
        }
        match (explicit, guesses) {
            (Ok(mut a), Ok(b)) => {
                a.extend(b);
                Ok(a)
            }
            (Ok(a), Err(_)) => Ok(a),
            (Err(_), Ok(b)) => Ok(b),
            (Err(e), Err(_)) => Err(e),
        }
    }

    fn translate_op_inputs(
        &mut self,
        object_type: &str,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let other = detail_map(item);
        let get = |k: &str| other.get(k);
        let flag = |_: &TimelineItem, k: &str| other.get(k).and_then(Value::as_bool);
        let compute = |_: &TimelineItem| compute(other.get("patternComputeOption"));
        match object_type {
            "CombineFeature" => {
                let target = get("targetBody").and_then(reference);
                let tools = references(get("toolBodies"));
                let operation = match get("operation").and_then(Value::as_str) {
                    Some(o) if o.starts_with("Cut") => "cut",
                    Some(o) if o.starts_with("Intersect") => "intersect",
                    Some(o) if o.starts_with("Join") => "join",
                    _ => "",
                };
                if target.is_none() && tools.is_empty() {
                    return self.combine_by_history(operation);
                }
                // The bodies are those the items that made them gave (not bodies
                // found by their names or measures alone).
                let produced = |r: &Reference| {
                    matches!(r, Reference::Body(fp)
                        if self.produced_body(fp).or_else(|| self.external_body(fp)).is_some())
                };
                let bodies_produced = target.as_ref().is_some_and(produced)
                    && !tools.is_empty()
                    && tools.iter().all(produced);
                let local_document = self.combine_local_document(target.as_ref());
                let target = target
                    .as_ref()
                    .and_then(|r| self.body_of(r))
                    .ok_or("the target body was not found")?;
                // A resized pattern can leave selections of copies the file
                // no longer has. Only the decoder's precise missing-body
                // evidence permits omitting them, never a failed replay.
                let all_tools = tools.len();
                let tools: Vec<&Reference> = tools
                    .iter()
                    .filter(|r| !self.lost_combine_tool(r, local_document))
                    .collect();
                let lost = all_tools - tools.len();
                if tools.is_empty() {
                    return Err(if lost > 0 {
                        "all its tool body references are lost in the file".to_owned()
                    } else {
                        "no tool bodies were found".to_owned()
                    });
                }
                let tools: Vec<String> = tools
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("a tool body was not found")?;
                // The operation and the keep flag are decoded fields (the
                // API reference dumps agree on all 33 combines); without
                // either, the translation fills in its own.
                let inputs_read =
                    !operation.is_empty() && get("isKeepToolBodies").is_some() && bodies_produced;
                let operation = if operation.is_empty() {
                    "join"
                } else {
                    operation
                };
                // A new component's result is the target in the component
                // the import put the item in (the dump's components, F6).
                let mut def = json!({"type": "combine", "target": target.to_string(),
                    "tools": tools, "operation": operation,
                    "keep_tools": get("isKeepToolBodies").and_then(Value::as_bool).unwrap_or(false)});
                // Tools of another component than the target's (the file's
                // assembly context) by their links (mitcad#104).
                let links = self.tool_links(target, &tools)?;
                if !links.is_empty() {
                    def["tool_links"] = Value::Object(links);
                }
                let mut c = Candidate::new(def);
                if lost > 0 {
                    c.input_provenance = InputProvenance::Inferred;
                    c.note = Some(format!(
                        "{lost} of its {all_tools} tool body references are lost in the file; \
                         remaining tools inferred"
                    ));
                } else if inputs_read {
                    c.input_provenance = InputProvenance::Read;
                }
                Ok(vec![c])
            }
            // Without inputs, or with bodies the decoder named but the replay
            // lacks, the history suggests them.
            "MirrorFeature" | "CircularPatternFeature" | "RectangularPatternFeature"
                if references(get("inputEntities")).is_empty()
                    || (named(&references(get("inputEntities")))
                        && !self.explicit_copies(object_type, &other)) =>
            {
                // Decoded counts are values, not parameters.
                let count = |key: &str| {
                    get(key)
                        .and_then(reference)
                        .and_then(|r| r.parameter()?.value)
                        .map(|v| v.round() as i64)
                };
                let length = |key: &str| {
                    get(key)
                        .and_then(reference)
                        .and_then(|r| r.parameter()?.value)
                        .map(mm)
                };
                let mirror_plane = get("mirrorPlane")
                    .and_then(reference)
                    .and_then(|r| self.plane_ref(&r));
                let copies = match object_type {
                    "MirrorFeature" => {
                        // Mirrors of features, else of bodies.
                        let mut out = self.mirrored_features(item, mirror_plane.clone());
                        out.extend(self.mirrored_bodies(mirror_plane));
                        if out.is_empty() {
                            return Err(if self.oracle.enabled {
                                "no inputs decoded, and no features or bodies before it match \
                                 the history's change"
                                    .to_owned()
                            } else {
                                "no inputs decoded (no history to guess them)".to_owned()
                            });
                        }
                        return Ok(out);
                    }
                    "CircularPatternFeature" => {
                        let entity = self.circular_axis(get("axis"), get("_f3d_axis"));
                        // The axis' stored line where its entity is not
                        // found (mitcad#96), the origin axes after it.
                        let line = decoded_line(get("_f3d_axis"));
                        Copies::Circular {
                            angle: self
                                .value_of(get("totalAngle"))
                                .unwrap_or(json!(std::f64::consts::TAU)),
                            symmetric: flag(item, "isSymmetric").unwrap_or(false),
                            quantity: count("quantity"),
                            origin_axes: entity.is_none(),
                            axis: entity.or(line),
                        }
                    }
                    _ => Copies::Rectangular {
                        distance: self
                            .value_of(get("distanceOne"))
                            .ok_or("no inputs, and the distance was not decoded")?,
                        quantity: count("quantityOne"),
                        quantity_two: count("quantityTwo"),
                        distance_two: self.value_of(get("distanceTwo")),
                        steps: (length("distanceOne"), length("distanceTwo")),
                    },
                };
                self.guessed_copies(item, copies, None)
            }
            "MirrorFeature" => {
                let variants = self.object_variants(&references(get("inputEntities")))?;
                let plane = get("mirrorPlane")
                    .and_then(reference)
                    .and_then(|r| self.plane_ref(&r))
                    .ok_or("the mirror plane was not decoded")?;
                let def = |objects: &Value, combine: bool| {
                    json!({"type": "mirror", "objects": objects, "plane": plane,
                           "combine": combine, "compute": compute(item)})
                };
                // Decoded (mitcad#96), the other way a guess after it; where
                // the stream decoder does not give it, the history tells
                // whether bodies were joined with their images.
                let combine = flag(item, "isCombine").unwrap_or(false);
                // The plane, the objects, the join flag and the compute
                // option all come from the file (mitcad#186).
                let settings = get("isCombine").is_some() && get("patternComputeOption").is_some();
                let mut out: Vec<Candidate> = variants
                    .iter()
                    .map(|(objects, dressed, provenance)| {
                        read_pattern(
                            dressed_note(def(objects, combine), *dressed, *provenance),
                            settings,
                        )
                    })
                    .collect();
                out.extend(
                    variants
                        .iter()
                        .map(|(objects, dressed, provenance)| Candidate {
                            guess: true,
                            input_provenance: crate::InputProvenance::Inferred,
                            ..dressed_note(def(objects, !combine), *dressed, *provenance)
                        }),
                );
                Ok(out)
            }
            "CircularPatternFeature" => {
                let variants = self.object_variants(&references(get("inputEntities")))?;
                let axis = self
                    .circular_axis(get("axis"), get("_f3d_axis"))
                    .or_else(|| decoded_line(get("_f3d_axis")))
                    .ok_or("the pattern axis was not decoded")?;
                let quantity = self
                    .value_of(get("quantity"))
                    .ok_or("the quantity was not decoded")?;
                let angle = self
                    .value_of(get("totalAngle"))
                    .ok_or("the angle was not decoded")?;
                let angle = self.circular_angle(&axis, angle, get("_f3d_axis"));
                // Every setting the file stores is read (mitcad#186).
                let settings = [
                    "isSymmetric",
                    "patternComputeOption",
                    "suppressedElementsIds",
                ]
                .iter()
                .all(|k| get(k).is_some());
                Ok(variants
                    .iter()
                    .map(|(objects, dressed, provenance)| {
                        let mut def = json!({"type": "circular_pattern", "objects": objects,
                            "axis": axis, "quantity": quantity, "angle": angle,
                            "symmetric": flag(item, "isSymmetric").unwrap_or(false),
                            "compute": compute(item)});
                        suppressed(&mut def, get("suppressedElementsIds"));
                        read_pattern(dressed_note(def, *dressed, *provenance), settings)
                    })
                    .collect())
            }
            "RectangularPatternFeature" => {
                let variants = self.object_variants(&references(get("inputEntities")))?;
                let (objects, _, _) = variants.last().expect("the objects without dressups");
                let Some(one) = self.rectangular_direction(&other, true) else {
                    // The oldest item version stores no directions
                    // (mitcad#74): they are guessed for the decoded features.
                    let features: Option<Vec<mitcad_model::FeatureUid>> = objects["features"]
                        .as_array()
                        .map(|f| f.iter().filter_map(|u| u.as_str()?.parse().ok()).collect());
                    let distance = self.value_of(get("distanceOne"));
                    return match (features, distance) {
                        (Some(features), Some(distance)) if self.oracle.enabled => {
                            let value = |key: &str| {
                                get(key)
                                    .and_then(reference)
                                    .and_then(|r| r.parameter()?.value)
                            };
                            let copies = Copies::Rectangular {
                                distance,
                                quantity: value("quantityOne").map(|v| v.round() as i64),
                                quantity_two: value("quantityTwo").map(|v| v.round() as i64),
                                distance_two: self.value_of(get("distanceTwo")),
                                steps: (value("distanceOne").map(mm), value("distanceTwo").map(mm)),
                            };
                            self.guessed_copies(item, copies, Some(features))
                        }
                        _ => Err("direction one was not decoded".to_owned()),
                    };
                };
                let mut def = json!({"type": "rectangular_pattern",
                "direction1": one, "compute": compute(item),
                "distance_type": match get("patternDistanceType").and_then(Value::as_str) {
                    Some(t) if t.starts_with("Extent") => "extent",
                    _ => "spacing",
                }});
                if let Some(two) = self.rectangular_direction(&other, false) {
                    def["direction2"] = two;
                }
                suppressed(&mut def, get("suppressedElementsIds"));
                // The direction, symmetry, distance type, compute option
                // and suppressed elements all come from the file
                // (mitcad#186).
                let settings = [
                    "isSymmetricInDirectionOne",
                    "patternDistanceType",
                    "patternComputeOption",
                    "suppressedElementsIds",
                ]
                .iter()
                .all(|k| get(k).is_some());
                // The entity's direction in the replay can differ from the
                // file's: then the stored vectors are tried after it
                // (mitcad#186).
                let mut defs = vec![def];
                let mut by_vector = defs[0].clone();
                let mut differs = false;
                for (key, first) in [("direction1", true), ("direction2", false)] {
                    // An origin axis points where its vector does.
                    if by_vector.get(key).is_none()
                        || matches!(by_vector[key]["axis"].as_str(), Some("x" | "y" | "z"))
                    {
                        continue;
                    }
                    if let Some(v) = self.rectangular_direction_by(&other, first, true)
                        && v != by_vector[key]
                    {
                        by_vector[key] = v;
                        differs = true;
                    }
                }
                if differs {
                    defs.push(by_vector);
                }
                Ok(defs
                    .iter()
                    .flat_map(|def| {
                        variants.iter().map(move |(objects, dressed, provenance)| {
                            let mut def = def.clone();
                            def["objects"] = objects.clone();
                            read_pattern(dressed_note(def, *dressed, *provenance), settings)
                        })
                    })
                    .collect())
            }
            "PathPatternFeature" => self.path_pattern(&other),
            "ShellFeature" => {
                let inputs = references(get("inputEntities"));
                let mut faces = Vec::new();
                let mut body = None;
                let mut inferred_body = false;
                for r in &inputs {
                    match r {
                        Reference::Face(fp) => {
                            let (b, f) = self.face_input(fp).ok_or("a shell face was not found")?;
                            body = Some(b);
                            inferred_body = false;
                            faces.push(f.to_string());
                        }
                        Reference::Body(fp) => {
                            body = self.body_of(r);
                            inferred_body = body.is_none();
                            body = body.or_else(|| self.only_solid(fp));
                        }
                        _ => {}
                    }
                }
                let body = body.ok_or("the shelled body was not found")?;
                let mut def = json!({"type": "shell", "body": body.to_string(), "faces": faces,
                    "tangent_chain": flag(item, "isTangentChain").unwrap_or(true),
                    "rounded": get("shellType").and_then(Value::as_str)
                        .is_some_and(|t| t.starts_with("Rounded"))});
                if let Some(v) = self.value_of(get("insideThickness")) {
                    def["inside"] = v;
                }
                if let Some(v) = self.value_of(get("outsideThickness")) {
                    def["outside"] = v;
                }
                // The `.ipt` import gives the number of faces removed, not
                // the faces: the history gives them (mitcad#60).
                let removed = get("_ipt_removed_faces").and_then(Value::as_u64);
                if def["faces"].as_array().is_some_and(Vec::is_empty)
                    && removed.is_some_and(|n| n > 0)
                {
                    return self.shell_by_history(body, def);
                }
                let mut candidate = Candidate::new(def);
                if inferred_body {
                    candidate.input_provenance = InputProvenance::Inferred;
                } else if get("isTangentChain").is_some()
                    && get("shellType").is_some()
                    && (get("insideThickness").is_some() || get("outsideThickness").is_some())
                    && (!faces.is_empty() || !inputs.is_empty())
                {
                    // The body or faces, both thicknesses, the type and
                    // the tangent-chain flag are decoded fields (the API
                    // reference dumps agree on the 7 shells).
                    candidate.input_provenance = InputProvenance::Read;
                }
                Ok(vec![candidate])
            }
            "OffsetFacesFeature" => {
                let mut inputs = references(get("inputFaces"));
                if inputs.is_empty() {
                    inputs = references(get("faces"));
                }
                let distance = self
                    .value_of(get("distance"))
                    .ok_or("the distance was not decoded")?;
                let mut by_body: Vec<(BodyUid, Vec<String>)> = Vec::new();
                for r in &inputs {
                    let Reference::Face(fp) = r else { continue };
                    let (b, f) = self.face_input(fp).ok_or("a face was not found")?;
                    match by_body.iter_mut().find(|(x, _)| *x == b) {
                        Some((_, faces)) => faces.push(f.to_string()),
                        None => by_body.push((b, vec![f.to_string()])),
                    }
                }
                if by_body.is_empty() {
                    return Err("its faces were not decoded".to_owned());
                }
                let defs = by_body
                    .into_iter()
                    .map(|(b, faces)| {
                        json!({"type": "offset_face", "body": b.to_string(),
                                             "faces": faces, "distance": distance})
                    })
                    .collect();
                Ok(vec![Candidate {
                    core: CandidateCore::of(defs),
                    note: None,
                    guess: false,
                    predicted: None,
                    input_provenance: crate::InputProvenance::Unknown,
                    first: false,
                }])
            }
            "MoveFeature" => {
                let bodies: Vec<String> = references(get("inputEntities"))
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("a moved body was not found (moves of faces are not supported)")?;
                let m: [[f64; 4]; 4] = get("transform")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .ok_or("the move's transform was not decoded")?;
                let matrix: Vec<[f64; 4]> = (0..3)
                    .map(|r| [m[r][0], m[r][1], m[r][2], mm(m[r][3])])
                    .collect();
                let free = json!({"type": "move", "bodies": bodies,
                    "transform": {"type": "free", "matrix": matrix}});
                // The move as the file defines it (mitcad#186): a rotation
                // about an axis by its angle parameter, a translation along
                // the axes by its distances, or a free move, whose matrix is
                // its input.
                let definition = get("definition").and_then(Value::as_object);
                let kind = definition
                    .and_then(|d| d.get("_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let field = |k: &str| definition.and_then(|d| d.get(k));
                let spec = match kind {
                    "MoveFeatureRotateDefinition" => field("axisEntity")
                        .and_then(reference)
                        .and_then(|r| self.axis_ref(&r))
                        .zip(self.value_of(field("angle")))
                        .map(
                            |(axis, angle)| json!({"type": "rotate", "axis": axis, "angle": angle}),
                        ),
                    "MoveFeatureTranslateXYZDefinition" => {
                        match (
                            self.value_of(field("xDistance")),
                            self.value_of(field("yDistance")),
                            self.value_of(field("zDistance")),
                        ) {
                            (Some(x), Some(y), Some(z)) => {
                                Some(json!({"type": "translate_xyz", "x": x, "y": y, "z": z}))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let mut out = Vec::new();
                if let Some(spec) = spec {
                    let mut c = Candidate::new(json!({"type": "move", "bodies": bodies,
                        "transform": spec}));
                    c.input_provenance = InputProvenance::Read;
                    out.push(c);
                }
                let mut fixed = Candidate::new(free);
                if kind == "MoveFeatureFreeMoveDefinition" {
                    fixed.input_provenance = InputProvenance::Read;
                } else {
                    fixed = fixed.with_note("as a fixed transform");
                }
                out.push(fixed);
                Ok(out)
            }
            // An align of bodies (mitcad#641): the motion it made, a move by
            // its matrix. The geometry it aligns is not decoded, so the move
            // does not follow that geometry: its inputs count as inferred.
            "AlignFeature" => {
                let bodies: Vec<String> = references(get("inputEntities"))
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("an aligned body was not found")?;
                if bodies.is_empty() {
                    return Err("its bodies were not decoded".to_owned());
                }
                let m: [[f64; 4]; 4] = get("transform")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .ok_or("the align's motion was not decoded")?;
                let matrix: Vec<[f64; 4]> = (0..3)
                    .map(|r| [m[r][0], m[r][1], m[r][2], mm(m[r][3])])
                    .collect();
                let mut c = Candidate::new(json!({"type": "move", "bodies": bodies,
                    "transform": {"type": "free", "matrix": matrix}}))
                .with_note("the bodies moved as the align moved them (a fixed transform: the geometry it aligns is not decoded)");
                c.input_provenance = InputProvenance::Inferred;
                Ok(vec![c])
            }
            "SplitBodyFeature" => {
                let bodies: Vec<String> = references(get("splitBodies"))
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("a split body was not found")?;
                let mut tools = references(get("splittingTools"));
                if tools.is_empty() {
                    tools.extend(get("splittingTool").and_then(reference));
                }
                // Every input is read when the bodies are those the items
                // that made them gave, the tools are planes of the file's
                // geometry, faces the decoder named or bodies the items
                // that made them gave, and the extend flag is decoded.
                let produced = |r: &Reference| {
                    matches!(r, Reference::Body(fp)
                        if self.produced_body(fp).or_else(|| self.external_body(fp)).is_some())
                };
                let inputs_read = flag(item, "isSplittingToolExtended").is_some()
                    && !tools.is_empty()
                    && references(get("splitBodies")).iter().all(produced)
                    && !references(get("splitBodies")).is_empty()
                    && tools.iter().all(|t| match t {
                        Reference::ConstructionPlane(_) => true,
                        Reference::Face(fp) => fp.f3d.is_some(),
                        other => produced(other),
                    });
                let mut tools = tools
                    .into_iter()
                    .map(|r| self.split_tool(r))
                    .collect::<Result<Vec<Value>, String>>()?;
                if tools.is_empty() {
                    return Err("the splitting tool was not decoded".to_owned());
                }
                let more = tools.split_off(1);
                let tool = tools.remove(0);
                let extend = flag(item, "isSplittingToolExtended").unwrap_or(true);
                // Tools on bodies of another component than the split
                // bodies' by their links (mitcad#104).
                let tool_bodies: Vec<String> = std::iter::once(&tool)
                    .chain(&more)
                    .filter_map(|t| t["body"].as_str().map(str::to_owned))
                    .collect();
                let links = match bodies.first().and_then(|b| b.parse::<BodyUid>().ok()) {
                    Some(target) => self.tool_links(target, &tool_bodies)?,
                    None => Map::new(),
                };
                let mut first = Candidate::new(split_def(&bodies, &tool, &more, extend, &links));
                if inputs_read {
                    first.input_provenance = InputProvenance::Read;
                }
                let mut candidates = vec![first];
                // The file's split leaves a selected body the tool does not
                // divide as it is, MitCad's fails: each body alone after
                // (mitcad#96).
                if bodies.len() > 1 {
                    for b in &bodies {
                        let links = match b.parse::<BodyUid>() {
                            Ok(target) => self.tool_links(target, &tool_bodies)?,
                            Err(_) => Map::new(),
                        };
                        let mut c = Candidate::new(split_def(
                            std::slice::from_ref(b),
                            &tool,
                            &more,
                            extend,
                            &links,
                        ))
                        .with_note("only the body the tool divides");
                        c.guess = true;
                        c.input_provenance = crate::InputProvenance::Inferred;
                        candidates.push(c);
                    }
                }
                Ok(candidates)
            }
            // A body taken out of the design.
            "RemoveFeature" => {
                let body = get("itemToRemove")
                    .and_then(reference)
                    .ok_or("the removed item was not decoded")?;
                let body = self
                    .body_of(&body)
                    .ok_or("the removed body was not found")?;
                let mut c =
                    Candidate::new(json!({"type": "remove_body", "bodies": [body.to_string()]}));
                c.input_provenance = InputProvenance::Read;
                Ok(vec![c])
            }
            // The faces taken away, healed by the faces around them.
            "DeleteFaceFeature" => {
                let faces = references(get("deletedFaces"));
                let mut by_body: Vec<(BodyUid, Vec<String>)> = Vec::new();
                for r in &faces {
                    let Reference::Face(fp) = r else { continue };
                    let (b, f) = self.face_input(fp).ok_or("a face was not found")?;
                    match by_body.iter_mut().find(|(x, _)| *x == b) {
                        Some((_, list)) => list.push(f.to_string()),
                        None => by_body.push((b, vec![f.to_string()])),
                    }
                }
                if by_body.is_empty() {
                    return Err("its faces were not decoded".to_owned());
                }
                let mut c = Candidate::new(Value::Null);
                c.defs = by_body
                    .into_iter()
                    .map(|(b, faces)| {
                        json!({"type": "delete_face", "body": b.to_string(), "faces": faces})
                    })
                    .collect();
                c.input_provenance = InputProvenance::Read;
                Ok(vec![c])
            }
            // Faces moved by a rigid motion, their neighbours following
            // (mitcad#343): MitCad's move_face per body, the motion the
            // decoded matrix (its translation in cm) as a free move's.
            "MoveFaceFeature" => {
                let faces = references(get("faces"));
                let mut by_body: Vec<(BodyUid, Vec<String>)> = Vec::new();
                for r in &faces {
                    let Reference::Face(fp) = r else { continue };
                    let (b, f) = self.face_input(fp).ok_or("a moved face was not found")?;
                    match by_body.iter_mut().find(|(x, _)| *x == b) {
                        Some((_, list)) => list.push(f.to_string()),
                        None => by_body.push((b, vec![f.to_string()])),
                    }
                }
                if by_body.is_empty() {
                    return Err("its faces were not decoded".to_owned());
                }
                let m: [[f64; 4]; 4] = get("transform")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .ok_or("the faces' motion was not decoded")?;
                let matrix: Vec<[f64; 4]> = (0..3)
                    .map(|r| [m[r][0], m[r][1], m[r][2], mm(m[r][3])])
                    .collect();
                let mut c = Candidate::new(Value::Null);
                c.defs = by_body
                    .into_iter()
                    .map(|(b, faces)| {
                        json!({"type": "move_face", "body": b.to_string(), "faces": faces,
                               "transform": {"type": "free", "matrix": matrix}})
                    })
                    .collect();
                c.input_provenance = InputProvenance::Read;
                Ok(vec![c])
            }
            // A copy of bodies in their own component is a move that
            // copies and does not move. A cut and paste moves the bodies
            // into another component or keeps cached geometry where its
            // source went missing.
            "CopyPasteBodies" => self.paste_bodies(item, &other),
            "SplitFaceFeature" => self.split_face(&other),
            "HoleFeature" => self.hole(&other),
            "ThreadFeature" => self.thread(&other),
            "ReplaceFaceFeature" => self.replace_face(&other),
            other => Err(format!("{other} is not supported yet")),
        }
    }

    /// A path pattern (mitcad#468): its objects, the path's sketch curves
    /// (one curve, or a chain of lines and arcs), the quantity and distance
    /// and the settings. The stream decoder gives the flip, orientation and
    /// symmetry, and the objects' centre that places the original on the
    /// path (`start_point`, mitcad#673). Where a setting is not given, the
    /// default is tried first, then, where the history checks the item, each
    /// of flip, orientation along the path and symmetry as a guess.
    fn path_pattern(&self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let get = |k: &str| d.get(k);
        let mut variants = self.object_variants(&references(get("inputEntities")))?;
        // A pattern of features that made no body (the stream decoder's
        // count, mitcad#673): the copies of a feature that made a new body
        // joined the body it is in (`combine`). The copies of that feature's
        // own solid cannot carry the fillets and chamfers patterned with it,
        // which the joined body holds at every copy: only the definition
        // that repeats them is tried (the one without them took 7–11 s of
        // the item's 30 s for 372 copies).
        let joined = get("_f3d_new_bodies").and_then(Value::as_u64) == Some(0);
        if joined && variants.iter().any(|(_, dressed, _)| *dressed) {
            variants.retain(|(_, dressed, _)| *dressed);
        }
        let path = self.path_of(get("path"), "path")?;
        if path.get("body").is_some() {
            return Err(
                "its path is a chain of edges: MitCad's path pattern follows sketch curves only"
                    .to_owned(),
            );
        }
        let path = match path["curves"].as_array().map(Vec::as_slice) {
            Some([curve]) => json!({"sketch": path["sketch"], "curve": curve}),
            _ => path,
        };
        let quantity = self
            .value_of(get("quantity"))
            .ok_or("the quantity was not decoded")?;
        let distance = self
            .value_of(get("distance"))
            .ok_or("the distance was not decoded")?;
        let flag = |k: &str| get(k).and_then(Value::as_bool);
        let mut def = json!({"type": "path_pattern", "path": path, "quantity": quantity,
            "distance": distance,
            "distance_type": match get("patternDistanceType").and_then(Value::as_str) {
                Some(t) if t.starts_with("Extent") => "extent",
                _ => "spacing",
            },
            "start": get("startPoint").and_then(Value::as_f64).unwrap_or(0.0),
            "flip": flag("isFlipDirection").unwrap_or(false),
            "along_path": flag("isOrientationAlongPath").unwrap_or(false),
            "symmetric": flag("isSymmetric").unwrap_or(false),
            "compute": compute(get("patternComputeOption"))});
        suppressed(&mut def, get("suppressedElementsIds"));
        // The stream decoder's objects' centre (mitcad#673): the original
        // sits at the path's point nearest to it.
        let center = get("_f3d_objects_center")
            .and_then(|p| serde_json::from_value::<[f64; 3]>(p.clone()).ok());
        if let Some(c) = center.filter(|_| get("startPoint").is_none()) {
            def["start_point"] = json!(mm3(c));
        }
        let read = [
            "patternDistanceType",
            "patternComputeOption",
            "suppressedElementsIds",
        ]
        .iter()
        .all(|k| get(k).is_some());
        let unread: Vec<&str> = [
            ("startPoint", "start point"),
            ("isFlipDirection", "flip"),
            ("isOrientationAlongPath", "orientation"),
            ("isSymmetric", "symmetry"),
        ]
        .into_iter()
        .filter(|(k, _)| get(k).is_none() && !(*k == "startPoint" && center.is_some()))
        .map(|(_, what)| what)
        .collect();
        let candidate = |def: &Value, objects: &Value, dressed: bool, p: InputProvenance| {
            let mut def = def.clone();
            def["objects"] = objects.clone();
            if joined && objects["type"] == "features" {
                def["combine"] = json!(true);
            }
            let mut c = read_pattern(dressed_note(def, dressed, p), read && unread.is_empty());
            if !unread.is_empty() {
                c.input_provenance = InputProvenance::Inferred;
                if c.note.is_none() {
                    c.note = Some(format!(
                        "its {} not decoded: the defaults",
                        unread.join(", ")
                    ));
                }
            }
            c
        };
        let mut out: Vec<Candidate> = variants
            .iter()
            .map(|(objects, dressed, p)| candidate(&def, objects, *dressed, *p))
            .collect();
        // The settings not given, each the other way, as guesses the
        // history checks.
        for (key, given, what) in [
            ("flip", "isFlipDirection", "flipped"),
            (
                "along_path",
                "isOrientationAlongPath",
                "turned along the path",
            ),
            ("symmetric", "isSymmetric", "symmetric"),
        ] {
            if get(given).is_some() {
                continue;
            }
            let mut other = def.clone();
            other[key] = json!(!def[key].as_bool().unwrap_or(false));
            for (objects, dressed, p) in &variants {
                let mut c = candidate(&other, objects, *dressed, *p);
                c.guess = true;
                if !*dressed {
                    c.note = Some(format!("{what} (a guess: not decoded)"));
                }
                out.push(c);
            }
        }
        Ok(out)
    }

    /// A split's tool as MitCad's: an origin or construction plane (by its
    /// geometry where it is no origin plane), a face or a body.
    fn split_tool(&self, tool: Reference) -> Result<Value, String> {
        Ok(match tool {
            Reference::ConstructionPlane(c) => match c.origin.clone().flatten().as_deref() {
                Some(o @ ("XY" | "XZ" | "YZ")) => json!(o.to_lowercase()),
                _ => {
                    let g = c
                        .geometry
                        .as_ref()
                        .ok_or("the splitting plane was not decoded")?;
                    json!({"origin": mm3(g.origin.ok_or("no origin")?),
                           "normal": geom::unit(g.normal.ok_or("no normal")?)})
                }
            },
            Reference::Face(fp) => {
                let (b, f) = self
                    .face_input(&fp)
                    .ok_or("the splitting face was not found")?;
                json!({"body": b.to_string(), "face": f.to_string()})
            }
            r @ Reference::Body(_) => {
                let b = self.body_of(&r).ok_or("the splitting body was not found")?;
                json!({"body": b.to_string()})
            }
            _ => return Err("the splitting tool was not decoded".to_owned()),
        })
    }

    /// A copy or cut and paste of bodies. A copy that stays in its
    /// component is a `move` that copies and does not move. A cut and paste
    /// into another component is the Move to Component command with an
    /// existing destination (`cmd` instead of a feature `type`: the
    /// bodies and the features that made them go into the component the
    /// item is in, [`Importer::cut_paste_item`]); a cut whose source went
    /// missing (the file keeps the cached geometry of the result) is not
    /// translated.
    fn paste_bodies(
        &mut self,
        item: &TimelineItem,
        d: &Map<String, Value>,
    ) -> Result<Vec<Candidate>, String> {
        let cut = item
            .f3d
            .as_ref()
            .and_then(|f| f.base_name.as_deref())
            .is_some_and(|n| n.starts_with("Cut"));
        let sources = references(d.get("sourceBody"));
        if sources.is_empty() {
            return Err(if cut {
                "the body to cut is missing in the file (its reference failed there): the file keeps the cached result"
            } else {
                "its source bodies were not decoded"
            }
            .to_owned());
        }
        if cut {
            return self.cut_paste_command(item, &sources);
        }
        let bodies: Vec<BodyUid> = sources
            .iter()
            .map(|r| self.body_of(r))
            .collect::<Option<_>>()
            .ok_or("a copied body was not found")?;
        Ok(vec![copy_in_place(&bodies)])
    }

    /// The Move to Component commands of a cut and paste: the bodies cut
    /// (all of one component) go into the component the item is in, an
    /// existing one placed once like the source. Where the features cannot
    /// go, a body transfer moves the bodies alone (the second). The bodies
    /// a feature ties together stay in their component unless `together`:
    /// that command is the last, a guess the history must confirm.
    fn cut_paste_command(
        &mut self,
        item: &TimelineItem,
        sources: &[Reference],
    ) -> Result<Vec<Candidate>, String> {
        let bodies: Vec<BodyUid> = sources
            .iter()
            .map(|r| self.body_of(r))
            .collect::<Option<_>>()
            .ok_or("a body to cut was not found")?;
        let mut source = None;
        for body in &bodies {
            let component = self
                .doc
                .body_component(*body)
                .ok_or("a body to cut is in no component")?;
            match source {
                None => source = Some(component),
                Some(s) if s != component => {
                    return Err("the bodies to cut are in several components".to_owned());
                }
                _ => {}
            }
        }
        let source = source.ok_or("no body to cut")?;
        let destination = self.component;
        // Bodies that are the destination's already (their features went
        // with an earlier cut: the features that made them also made the
        // bodies of that cut): nothing is left to move. Or a cut pasted into
        // the component it came from, which the file's history keeps as a
        // copy: the body stays and its copy is added (mitcad#1175). The
        // item's own state in the history tells them apart: where it is not
        // the bodies as they are, the copy, which the usual route checks
        // against it.
        if source == destination {
            if item.index.is_some_and(|i| !self.state_is_current(i)) {
                let copy = copy_in_place(&bodies).with_note(
                    "pasted into the component it was cut from: the file's history keeps the \
                     body and adds its copy",
                );
                return Ok(vec![copy]);
            }
            let mut stays = Candidate::new(json!({"cmd": "none",
                "bodies": bodies.iter().map(ToString::to_string).collect::<Vec<_>>()}))
            .with_note("its bodies are in the component already: they went with an earlier cut");
            stays.input_provenance = InputProvenance::Read;
            return Ok(vec![stays]);
        }
        let assembly = self.doc.assembly();
        let only_path = |c| match assembly.paths_to(c).as_slice() {
            [path] => Ok(path
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/")),
            _ => Err(format!(
                "a cut and paste between components needs {} placed once",
                assembly.name(c)
            )),
        };
        let (from, into) = (only_path(source)?, only_path(destination)?);
        let command = |together: bool| {
            json!({"cmd": "move_to_component", "component": source.to_string(),
                   "bodies": bodies.iter().map(ToString::to_string).collect::<Vec<_>>(),
                   "source_occurrence": from,
                   "destination": {"kind": "existing", "component": destination.to_string(),
                                   "occurrence": into},
                   "together": together})
        };
        let mut apart = Candidate::new(command(false));
        apart.input_provenance = InputProvenance::Read;
        // Where the features cannot go (a definition that cannot change
        // coordinates, a feature of the source that works on the bodies
        // too), the bodies alone move from this point of the timeline, as
        // the item moved them (mitcad#465).
        let mut transfer = Candidate::new(json!({"type": "body_transfer",
            "bodies": bodies.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "link": {"source": from, "target": into}}));
        transfer.input_provenance = InputProvenance::Read;
        let mut together =
            Candidate::new(command(true)).with_note("with the bodies a feature ties to them");
        together.guess = true;
        together.input_provenance = InputProvenance::Inferred;
        Ok(vec![apart, transfer, together])
    }

    /// A hole's place by its distances from the lines of two edges of its
    /// face (`PlaneAndOffsetsHolePositionDefinition`: `edgeOne`,
    /// `offsetOne`, `edgeTwo`, `offsetTwo`, mitcad#719): the `face_offsets`
    /// placement on `face` of `body`, when both edges are on that body.
    ///
    /// On a cylindrical face (a shaft's side) the definition gives one
    /// face and the hole's distance from it (the shaft's end):
    /// the `cylinder_offset` placement, on the side of the axis where the
    /// point `at` the file keeps with the hole lies.
    fn hole_offsets(
        &self,
        d: &Map<String, Value>,
        body: &str,
        face: &str,
        at: Vec3,
    ) -> Option<Value> {
        let position = d.get("holePositionDefinition")?.as_object()?;
        if position.get("_type")?.as_str()? != "PlaneAndOffsetsHolePositionDefinition" {
            return None;
        }
        if let Some(Reference::Face(from)) = position.get("edgeOne").and_then(reference)
            && position.get("edgeTwo").is_none()
        {
            let uid: BodyUid = body.parse().ok()?;
            let shape = crate::body_scope::shape(self.doc, uid)?;
            let name: FaceName = face.parse().ok()?;
            self.doc.kernel().face_cylinder(shape, &name).ok()?;
            return Some(
                json!({"type": "cylinder_offset", "body": body, "face": face,
                "plane": self.plane_ref(&Reference::Face(from))?,
                "offset": self.value_of(position.get("offsetOne"))?, "toward": at}),
            );
        }
        let edge = |key: &str| -> Option<String> {
            let Reference::Edge(fp) = reference(position.get(key)?)? else {
                return None;
            };
            let (on, edge) = refs::resolve_edge(self.doc, &fp)?;
            (on.to_string() == body).then(|| edge.to_string())
        };
        Some(json!({"type": "face_offsets", "body": body, "face": face,
            "edge1": edge("edgeOne")?, "offset1": self.value_of(position.get("offsetOne"))?,
            "edge2": edge("edgeTwo")?, "offset2": self.value_of(position.get("offsetTwo"))?}))
    }

    /// A hole at a point of a planar face (F1 import table); without a
    /// decoded position, where the history state shows material removed.
    fn hole(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let get = |k: &str| d.get(k);
        let position: Option<[f64; 3]> =
            get("position").and_then(|p| serde_json::from_value(p.clone()).ok());
        let Some(position) = position else {
            if !self.oracle.enabled {
                return Err("its position was not decoded".to_owned());
            }
            return self.hole_by_history(d);
        };
        let at = mm3(position);
        // The face: one named by the position definition, else the planar
        // face through the position.
        // (Its planar entity first: the face a hole on a shaft is measured
        // from is in the definition too, mitcad#719.)
        let position = get("holePositionDefinition").and_then(Value::as_object);
        let planar: Vec<&Value> = match position.and_then(|m| m.get("planarEntity")) {
            Some(entity) => vec![entity],
            None => position.into_iter().flat_map(|m| m.values()).collect(),
        };
        let named = planar
            .into_iter()
            .filter_map(reference)
            .find_map(|r| match r {
                Reference::Face(fp) => self.face_input(&fp),
                _ => None,
            });
        // A hole placed on a construction plane or an origin plane (its
        // position definition's plane, mitcad#25): the holes start on it.
        let on_plane = if named.is_none() {
            get("holePositionDefinition")
                .and_then(Value::as_object)
                .and_then(|m| m.get("planarEntity"))
                .and_then(reference)
                .filter(|r| matches!(r, Reference::ConstructionPlane(_)))
                .and_then(|r| self.plane_ref(&r))
        } else {
            None
        };
        let inferred_face = named.is_none() && on_plane.is_none();
        let (body, face) = match (named, &on_plane) {
            (Some((b, f)), _) => (b.to_string(), f.to_string()),
            (None, Some(_)) => (String::new(), String::new()),
            (None, None) => refs::planar_faces(self.doc)
                .into_iter()
                .find(|(_, _, p)| geom::dot(geom::sub(at, p.origin), p.normal).abs() < 1e-4)
                .map(|(b, f, _)| (b.to_string(), f.to_string()))
                .ok_or("no planar face through its position")?,
        };
        let kind = get("holeType")
            .and_then(Value::as_str)
            .ok_or("its hole type was not decoded")?;
        // Every point of the hole (mitcad#67), else its position.
        let points: Vec<[f64; 3]> = get("_f3d_positions")
            .and_then(|p| serde_json::from_value::<Vec<[f64; 3]>>(p.clone()).ok())
            .filter(|p| !p.is_empty())
            .map(|p| p.into_iter().map(mm3).collect())
            .unwrap_or_else(|| vec![at]);
        let mut def = self.hole_def(d, &body, &face, &points, kind)?;
        let on_face = !inferred_face && on_plane.is_none();
        if let Some(plane) = on_plane {
            def["placement"] = json!({"type": "plane", "plane": plane, "points": points});
        }
        let mut defs = Vec::new();
        // One hole at its distances from two edges of its face (the file's
        // dimensions, mitcad#719): first, as the point the file keeps with
        // it need not be where they put the hole (3.2 mm apart in one
        // design, 296 mm in another, the hole's volume the same either
        // way). Not trusted over the point unless the hole they give lies
        // where the item's state has it (mitcad#733: a misread definition
        // would be taken by its volume alone); after it otherwise, where
        // the point's hole lies there.
        let mut offsets_after = None;
        if let (true, [_]) = (on_face, &points[..])
            && let Some(placement) = self.hole_offsets(d, &body, &face, at)
        {
            let mut offsets = def.clone();
            offsets["placement"] = placement;
            let on_state = self.hole_on_state(&offsets);
            let trusted = offsets_first(on_state, || self.hole_on_state(&def));
            if trusted {
                defs.push((offsets, false));
            } else {
                offsets_after = Some(offsets);
            }
        }
        // A hole through all whose depth the file keeps (a lead, mitcad#96):
        // through all first, the history to check it.
        if through_all_lead(d) && def["extent"]["type"] == "distance" {
            let mut all = def.clone();
            all["extent"] = json!({"type": "through_all"});
            defs.push((all, true));
        }
        defs.push((def, false));
        defs.extend(offsets_after.map(|offsets| (offsets, false)));
        let mut out = Vec::new();
        for (def, guess) in defs {
            out.extend(
                self.hole_variants(d, vec![def], true)?
                    .into_iter()
                    .map(|(defs, note)| Candidate {
                        core: CandidateCore::of(defs),
                        note,
                        guess,
                        predicted: None,
                        input_provenance: if inferred_face {
                            crate::InputProvenance::Inferred
                        } else if !guess && get("isDefaultDirection").is_some() {
                            // The face, position, type, depth and direction
                            // are decoded (mitcad#25).
                            crate::InputProvenance::Read
                        } else {
                            crate::InputProvenance::Unknown
                        },
                        first: false,
                    }),
            );
        }
        Ok(out)
    }

    /// A hole from the history: each piece of material the item's state
    /// lost is a hole. Its axis is that of the piece's cylinder (the bore);
    /// the hole starts on the plane of a planar face across the axis at
    /// either end of the piece (the point need not lie on the face: a hole
    /// may start in a groove), its point where the axis meets the plane.
    /// Without a cylinder the piece's centre is projected onto the face it
    /// starts from. The hole type is not decoded either: simple, then the
    /// kinds whose sizes the item has; the depth, then through all.
    fn hole_by_history(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let depth = d
            .get("extentDefinition")
            .and_then(Value::as_object)
            .and_then(|e| e.get("distance"))
            .and_then(reference)
            .and_then(|r| r.parameter()?.value)
            .map(mm);
        // A tapped hole's bore is its thread's minor diameter.
        let minor = d
            .get("tappedHoleInfo")
            .and_then(|i| i.get("minorDiameter"))
            .and_then(Value::as_f64);
        let radius = minor
            .or_else(|| {
                d.get("holeDiameter")
                    .and_then(reference)
                    .and_then(|r| r.parameter()?.value)
            })
            .map(|v| mm(v) / 2.0);
        let pieces = self.removed_pieces()?;
        let faces = refs::planar_faces(self.doc);
        let kernel = self.doc.kernel();
        // Per piece the faces it can start from, nearest to half the depth
        // first.
        let mut options: Vec<(BodyUid, Vec<StartFace>)> = Vec::new();
        for (body, center, piece) in pieces {
            if let Some(found) = axis_starts(kernel, &piece, radius, &faces, body) {
                options.push((body, found));
                continue;
            }
            let Some(shape) = crate::body_scope::bodies(self.doc)
                .iter()
                .find(|b| b.uid == body)
                .map(|b| b.shape.clone())
            else {
                continue;
            };
            let mut found: Vec<StartFace> = Vec::new();
            for (_, face, plane) in faces.iter().filter(|(b, _, _)| *b == body) {
                let h = geom::dot(geom::sub(center, plane.origin), plane.normal);
                if h >= -1e-6 {
                    continue;
                }
                let at = geom::sub(center, geom::scale(plane.normal, h));
                let eps = 1e-3;
                let probe = [
                    geom::sub(at, geom::scale(plane.normal, eps)),
                    geom::add(at, geom::scale(plane.normal, eps)),
                ];
                // Material just below the point, none above: the hole
                // starts on this face here (the body before the hole).
                match kernel.points_inside(&shape, &probe) {
                    Ok(v) if v == [true, false] => {}
                    _ => continue,
                }
                let score = depth.map_or(-h, |dd| (-h - dd / 2.0).abs());
                found.push((score, face.to_string(), at));
            }
            found.sort_by(|a, b| a.0.total_cmp(&b.0));
            if !found.is_empty() {
                options.push((body, found));
            }
        }
        if options.is_empty() {
            return Err("its position was not decoded, and no face was found where the history removes material".to_owned());
        }
        // The best face of each piece, then the second best (a hole through
        // a plate fits both of its sides).
        let mut placements: Vec<Vec<HolePoints>> = Vec::new();
        for rank in 0..2 {
            let mut groups: Vec<HolePoints> = Vec::new();
            for (body, found) in &options {
                let (_, face, at) = found.get(rank).unwrap_or(&found[0]);
                match groups.iter_mut().find(|(b, f, _)| b == body && f == face) {
                    Some(g) => g.2.push(*at),
                    None => groups.push((*body, face.clone(), vec![*at])),
                }
            }
            if !placements.contains(&groups) {
                placements.push(groups);
            }
        }
        let mut kinds = vec!["Simple"];
        if d.contains_key("counterboreDiameter") && d.contains_key("counterboreDepth") {
            kinds.push("Counterbore");
        }
        if d.contains_key("countersinkDiameter") && d.contains_key("countersinkAngle") {
            kinds.push("Countersink");
        }
        let mut candidates = Vec::new();
        // The extent type is not decoded: the depth (when decoded), else
        // through all.
        let mut through_all = d.clone();
        through_all.insert(
            "extentDefinition".to_owned(),
            json!({"_type": "ThroughAllExtentDefinition"}),
        );
        let mut last_error = None;
        let order = if through_all_lead(d) {
            [true, false]
        } else {
            [false, true]
        };
        for through in order {
            let d = if through { &through_all } else { d };
            for groups in &placements {
                for kind in &kinds {
                    let defs: Result<Vec<Value>, String> = groups
                        .iter()
                        .map(|(body, face, points)| {
                            self.hole_def(d, &body.to_string(), face, points, kind)
                        })
                        .collect();
                    let defs = match defs {
                        Ok(defs) => defs,
                        Err(e) => {
                            last_error = Some(e);
                            continue;
                        }
                    };
                    // All bodies, then only those the holes go into (the
                    // file's participants are not decoded).
                    let mut sets = vec![defs.clone()];
                    let mut own: Vec<String> =
                        groups.iter().map(|(b, _, _)| b.to_string()).collect();
                    own.sort();
                    own.dedup();
                    if crate::body_scope::bodies(self.doc).len() > own.len()
                        && defs[0].get("participants").is_none()
                    {
                        sets.push(
                            defs.into_iter()
                                .map(|mut def| {
                                    def["participants"] = json!(own);
                                    def
                                })
                                .collect(),
                        );
                    }
                    for (set, defs) in sets.into_iter().enumerate() {
                        for (variant, (defs, thread_note)) in
                            self.hole_variants(d, defs, false)?.into_iter().enumerate()
                        {
                            let mut note = "placed where the history removes material".to_owned();
                            if let Some(n) = thread_note {
                                note = format!("{note}; {n}");
                            }
                            let c = Candidate {
                                core: CandidateCore::of(defs),
                                note: Some(note),
                                guess: true,
                                predicted: None,
                                input_provenance: crate::InputProvenance::Inferred,
                                first: false,
                            };
                            candidates.push(((set, variant), c));
                        }
                    }
                }
            }
        }
        if candidates.is_empty() {
            return Err(last_error.unwrap_or_else(|| "no hole fits the history".to_owned()));
        }
        // Each participant set and variant over every placement, kind and
        // extent before the next: only so many candidates are tried.
        candidates.sort_by_key(|(tier, _)| *tier);
        Ok(candidates.into_iter().map(|(_, c)| c).collect())
    }

    /// The material the next history state removes from the replay's
    /// bodies: each piece with the body it came from, its centre and its
    /// shape.
    fn removed_pieces(&mut self) -> Result<Vec<Piece<K::Shape>>, String> {
        let next = self.oracle.next_index();
        // (Bodies kept within the tolerance as their stored ones, mitcad#285.)
        let stored = self.stored_counterparts();
        let kernel = self.doc.kernel();
        let state: Vec<(crate::StoredBody<K::Shape>, Sig)> =
            self.oracle.state(kernel, next)?.to_vec();
        // The bodies of the state before it: one the file's operation left
        // as it was has no material removed, whatever the replay's body
        // differs in (mitcad#719: a shaft whose modelled threads start at
        // other angles than the file's, compared at every later hole of
        // another component, a boolean of hundreds of thread faces each).
        let before: Vec<Sig> = match next.checked_sub(1) {
            Some(previous) => self.oracle.sigs(kernel, previous),
            None => Vec::new(),
        };
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = crate::body_scope::bodies(self.doc)
            .iter()
            .map(|b| {
                let sig = stored.get(&b.uid).copied();
                (
                    b.uid,
                    b.shape.clone(),
                    sig.or_else(|| Sig::of(kernel, b.shape)),
                )
            })
            .collect();
        let mut used = vec![false; state.len()];
        let mut changed = Vec::new();
        for (uid, shape, sig) in &bodies {
            let Some(sig) = sig else { continue };
            match (0..state.len()).find(|&j| !used[j] && state[j].1.same(sig)) {
                Some(j) => used[j] = true,
                None => changed.push((*uid, shape, *sig)),
            }
        }
        let mut out = Vec::new();
        for (uid, shape, sig) in changed {
            let Some(j) = (0..state.len()).filter(|&j| !used[j]).min_by(|&a, &b| {
                let d = |j: usize| geom::distance(state[j].1.center, sig.center);
                d(a).total_cmp(&d(b))
            }) else {
                continue;
            };
            used[j] = true;
            if before.iter().any(|s| s.same(&state[j].1)) {
                continue;
            }
            // (Kernel calls in a row, mitcad#82.)
            crate::tick();
            let clock = std::time::Instant::now();
            let pieces = crate::removed::removed_material(kernel, shape, &state[j].0.shape)?;
            if crate::tracing() {
                eprintln!(
                    "import: removed material: {} pieces in {:.2} s",
                    pieces.len(),
                    clock.elapsed().as_secs_f64()
                );
            }
            out.extend(pieces.into_iter().map(|(_, c, s)| (uid, c, s)));
        }
        if out.is_empty() {
            return Err("the history shows no material removed".to_owned());
        }
        Ok(out)
    }

    /// A hole definition of the item's sizes at points of a face.
    fn hole_def(
        &self,
        d: &Map<String, Value>,
        body: &str,
        face: &str,
        points: &[[f64; 3]],
        kind: &str,
    ) -> Result<Value, String> {
        let get = |k: &str| d.get(k);
        let mut def = json!({"type": "hole",
            "placement": {"type": "face", "body": body, "face": face, "points": points},
            "diameter": self.value_of(get("holeDiameter")).ok_or("no diameter")?});
        match Some(kind) {
            Some(t) if t.starts_with("Counterbore") => {
                def["kind"] = json!({"type": "counterbore",
                    "diameter": self.value_of(get("counterboreDiameter")).ok_or("no counterbore diameter")?,
                    "depth": self.value_of(get("counterboreDepth")).ok_or("no counterbore depth")?});
            }
            Some(t) if t.starts_with("Countersink") => {
                def["kind"] = json!({"type": "countersink",
                    "diameter": self.value_of(get("countersinkDiameter")).ok_or("no countersink diameter")?,
                    "angle": self.value_of(get("countersinkAngle")).ok_or("no countersink angle")?});
            }
            Some(t) if t.starts_with("Simple") => {}
            _ => return Err(format!("hole type {kind} is not supported")),
        }
        let tip = get("tipAngle").and_then(reference);
        let flat = tip
            .as_ref()
            .and_then(Reference::parameter)
            .and_then(|p| p.value)
            .is_some_and(|a| (a - std::f64::consts::PI).abs() < 1e-9);
        if flat {
            def["flat"] = json!(true);
        } else if let Some(v) = self.value(&tip) {
            def["tip_angle"] = v;
        }
        let extent = get("extentDefinition").and_then(Value::as_object);
        def["extent"] = match extent.and_then(|e| e.get("_type")).and_then(Value::as_str) {
            Some("DistanceExtentDefinition") => json!({"type": "distance",
                "depth": self.value_of(extent.and_then(|e| e.get("distance"))).ok_or("no depth")?}),
            Some("ThroughAllExtentDefinition" | "AllExtentDefinition") => {
                json!({"type": "through_all"})
            }
            _ => return Err("its extent was not decoded".to_owned()),
        };
        if get("isDefaultDirection").and_then(Value::as_bool) == Some(false) {
            def["flip"] = json!(true);
        }
        let participants: Vec<String> = references(get("participantBodies"))
            .iter()
            .filter_map(|r| self.body_of(r))
            .map(|b| b.to_string())
            .collect();
        if !participants.is_empty() {
            def["participants"] = json!(participants);
        }
        Ok(def)
    }

    /// The candidates of hole definitions: as they are, or for a tapped
    /// hole with the threads on their walls ([`Self::hole_thread`]), first
    /// with the hole's diameter parameter (the bore of the hole's own
    /// history state: the file's thread widens it to the thread's minor
    /// diameter in a state of its own, which the import leaves out), then
    /// with that minor diameter, then without the thread.
    fn hole_variants(
        &mut self,
        d: &Map<String, Value>,
        defs: Vec<Value>,
        measure: bool,
    ) -> Result<Vec<Defs>, String> {
        let tapped = d
            .get("holeTapType")
            .and_then(Value::as_str)
            .is_some_and(|t| t.starts_with("Tapped"));
        if !tapped {
            return Ok(vec![(defs, None)]);
        }
        let mut threads = Vec::new();
        let mut thread_note = None;
        let mut left_out = None;
        for (k, def) in defs.iter().enumerate() {
            // The body of its face; for holes that start on a plane (no
            // face, mitcad#719), the first of its participants, else the
            // one body of its component.
            let body = def["placement"]["body"]
                .as_str()
                .or_else(|| def["participants"].get(0).and_then(Value::as_str))
                .and_then(|b| b.parse::<BodyUid>().ok())
                .or_else(|| {
                    let own: Vec<BodyUid> = crate::body_scope::bodies(self.doc)
                        .iter()
                        .map(|b| b.uid)
                        .filter(|&b| self.doc.body_component(b) == Some(self.component))
                        .collect();
                    match own[..] {
                        [only] => Some(only),
                        _ => None,
                    }
                })
                .ok_or("the hole's body")?;
            // (One hole at distances from two edges, or along a shaft.)
            let count = match def["placement"]["points"].as_array() {
                Some(points) => points.len(),
                None => usize::from(matches!(
                    def["placement"]["type"].as_str(),
                    Some("face_offsets" | "cylinder_offset")
                )),
            };
            // A modelled thread's angles, measured in its state on the
            // walls the hole makes (mitcad#733); decoded holes only.
            let angles = if measure {
                self.hole_thread_angles(d, def, count)
            } else {
                Vec::new()
            };
            match self.hole_thread(d, body, k, count, &angles) {
                Ok(Some((thread, n))) => {
                    threads.extend(thread);
                    thread_note = thread_note.or(n);
                }
                Ok(None) => {}
                Err(why) => left_out = Some(why),
            }
        }
        let minor = d
            .get("tappedHoleInfo")
            .and_then(|i| i.get("minorDiameter"))
            .and_then(Value::as_f64)
            .map(mm);
        let mut bores = vec![defs.clone()];
        if let Some(m) = minor {
            let mut widened = defs.clone();
            for def in &mut widened {
                def["diameter"] = json!(m);
            }
            let same = defs[0]["diameter"]
                .as_f64()
                .is_some_and(|x| (x - m).abs() < 1e-9);
            if !same {
                bores.push(widened);
            }
        }
        let mut out = Vec::new();
        if left_out.is_none() {
            for bore in &bores {
                let mut all = bore.clone();
                all.extend(threads.iter().cloned());
                out.push((all, thread_note.clone()));
            }
        }
        let note = match left_out {
            Some(why) => format!("its thread is left out ({why})"),
            None => "its thread is left out".to_owned(),
        };
        for bore in bores {
            out.push((bore, Some(note.clone())));
        }
        Ok(out)
    }

    /// Threads on cylindrical faces (F1 import table). The end a partial
    /// thread is measured from is the file's: where the axis of the face's
    /// cylinder points as the file has it (`threadLocation`), which MitCad's
    /// cylinder may have the other way round; faces whose cylinders run
    /// the other way get a thread of their own.
    ///
    /// A thread in the file also sizes its whole faces: an external one to
    /// the thread's major diameter, an internal one to its minor diameter
    /// (`threadInfo`; the reference models' cosmetic threads, a partial one
    /// too). Candidates do so with an `offset_face` of each face, or with
    /// a tube of material between the radii, before the thread; another
    /// leaves the faces as they are (a modelled thread sizes its threaded
    /// part itself, and is tried so first).
    ///
    /// A cylindrical face the file keeps whole may be in pieces in the
    /// replay (a join across a bore splits it where the file's face goes on):
    /// the definitions on the face with the pieces end to end with it are
    /// tried first, then those on the face alone.
    fn thread(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let (mut candidates, pieces) = self.thread_candidates(d, true)?;
        if pieces {
            candidates.extend(self.thread_candidates(d, false)?.0);
        }
        Ok(candidates)
    }

    /// [`Importer::thread`]'s candidates, with the faces' pieces end to end
    /// (`join_pieces`, not for a partial thread), and whether any were added.
    fn thread_candidates(
        &mut self,
        d: &Map<String, Value>,
        join_pieces: bool,
    ) -> Result<(Vec<Candidate>, bool), String> {
        let mut faces = references(d.get("inputCylindricalFaces"));
        if faces.is_empty() {
            faces.extend(d.get("inputCylindricalFace").and_then(reference));
        }
        if faces.is_empty() {
            return Err("its faces were not decoded".to_owned());
        }
        let modeled = d.get("isModeled").and_then(Value::as_bool) == Some(true);
        let partial = d.get("isFullLength").and_then(Value::as_bool) == Some(false);
        let low = d
            .get("threadLocation")
            .and_then(Value::as_str)
            .is_some_and(|l| l.starts_with("Low"));
        let info = d.get("threadInfo").and_then(Value::as_object);
        let diameter = |key: &str| {
            info.and_then(|i| i.get(key))
                .and_then(Value::as_f64)
                .map(mm)
        };
        let (major, minor) = (diameter("majorDiameter"), diameter("minorDiameter"));
        // Each face with the end its thread starts from, in MitCad's
        // terms, its cylinder, and the radius the file sizes it to.
        let mut threaded: Vec<(BodyUid, String, bool, Cylinder, Option<f64>)> = Vec::new();
        // Per face, where the file's helix starts: the file's axis and the
        // direction of the surface's seam ([`thread_phase`]).
        let mut starts: Vec<(Option<Vec3>, Option<Vec3>)> = Vec::new();
        let mut internal = None;
        // The pieces of the faces that continue them (see [`Importer::thread`]).
        let mut pieces: Vec<(BodyUid, String, bool, Cylinder, Option<f64>)> = Vec::new();
        // Per face, the chamfers at its ends its sizing moves with it
        // ([`end_chamfers`]).
        let mut chamfers: Vec<Vec<(FaceName, f64)>> = Vec::new();
        for r in &faces {
            let Reference::Face(fp) = r else {
                return Err("a threaded face of another kind".to_owned());
            };
            // By a point on it, or by the thread's ends (`.ipt` threads).
            let (body, face) = self
                .face_input(fp)
                .or_else(|| refs::resolve_cylinder_through(self.doc, fp))
                .ok_or("a threaded face was not found")?;
            let shape =
                crate::body_scope::shape(self.doc, body).ok_or("a threaded face was not found")?;
            let cylinder = self
                .doc
                .kernel()
                .face_cylinder(shape, &face)
                .map_err(|e| format!("a threaded face: {e}"))?;
            internal = Some(cylinder.internal);
            let file_axis = fp.geometry.as_ref().and_then(|g| g.axis);
            let turned = file_axis.is_some_and(|a| geom::dot(a, cylinder.axis.direction) < 0.0);
            let low_end = low != (partial && turned);
            let target = if cylinder.internal { minor } else { major }
                .map(|m| m / 2.0)
                .filter(|m| (m - cylinder.radius).abs() > 1e-6);
            if target.is_some() {
                chamfers.push(end_chamfers(self.doc.kernel(), shape, &cylinder));
            } else {
                chamfers.push(Vec::new());
            }
            threaded.push((body, face.to_string(), low_end, cylinder, target));
            starts.push((file_axis, fp.f3d.as_ref().and_then(|f| f.seam)));
            if join_pieces && !partial {
                // The pieces of the file's face start no helix of their own
                // that the file tells: their angles are measured.
                for (name, piece) in coaxial_pieces(self.doc.kernel(), shape, &face, &cylinder) {
                    let target = if piece.internal { minor } else { major }
                        .map(|m| m / 2.0)
                        .filter(|m| (m - piece.radius).abs() > 1e-6);
                    pieces.push((body, name.to_string(), low_end, piece, target));
                }
            }
        }
        let named = threaded.len();
        for piece in pieces {
            if !threaded.iter().any(|t| t.0 == piece.0 && t.1 == piece.1) {
                threaded.push(piece);
                starts.push((None, None));
                chamfers.push(Vec::new());
            }
        }
        let joined = threaded.len() > named;
        let (spec, note) = thread_spec(info, Some(d), internal)?;
        // A modelled thread: the profile of the file's diameters, turned as
        // the history shows it on each face.
        let mut modelled: Option<(Option<Value>, Vec<Option<f64>>)> = None;
        // Per face: its angle was read from the file, not measured.
        let mut phase_read: Vec<bool> = Vec::new();
        if modeled {
            let standard: mitcad_model::features::thread_table::ThreadStandard =
                serde_json::from_value(
                    spec.get("standard").cloned().unwrap_or(json!("iso_metric")),
                )
                .map_err(|e| e.to_string())?;
            standard.check_modeled()?;
            let data = mitcad_model::features::thread_table::lookup(
                standard,
                spec["designation"].as_str().unwrap_or_default(),
            )?;
            let diameters = thread_diameters(info);
            let pitch_radius = diameters
                .as_ref()
                .and_then(|d| d["pitch"].as_f64())
                .unwrap_or(data.pitch_diameter)
                / 2.0;
            let right = spec
                .get("right_handed")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let length = partial
                .then(|| parameter_mm(d.get("threadLength")))
                .flatten();
            let offset = parameter_mm(d.get("threadOffset")).unwrap_or(0.0);
            let mut angles = Vec::new();
            for (k, (_, _, low_end, c, _)) in threaded.iter().enumerate() {
                // Where the file's helix starts (a right-handed thread on a
                // whole cylinder), else the groove found in later states.
                let read = right
                    .then(|| thread_phase(starts[k].0, starts[k].1))
                    .flatten();
                phase_read.push(read.is_some());
                if read.is_some() {
                    angles.push(read);
                    continue;
                }
                // The middle of the threaded part along the face's axis.
                let middle = match length {
                    Some(l) if *low_end => offset + l / 2.0,
                    Some(l) => c.length - offset - l / 2.0,
                    None => c.length / 2.0,
                };
                let clock = std::time::Instant::now();
                angles.push(self.thread_angle(c, middle, data.pitch, pitch_radius, right));
                if crate::tracing() {
                    eprintln!(
                        "import: the thread's angle measured in {:.2} s",
                        clock.elapsed().as_secs_f64()
                    );
                }
            }
            modelled = Some((diameters, angles));
        }
        // The threads on faces of these names, one per end they start from
        // (a modelled thread one per face: each has its own angle).
        let threads = |names: &[String]| -> Result<Vec<Value>, String> {
            let mut groups: Vec<(bool, Vec<Value>, Option<f64>)> = Vec::new();
            for (k, ((body, _, low_end, _, _), name)) in threaded.iter().zip(names).enumerate() {
                let face = json!({"body": body.to_string(), "face": name});
                let angle = modelled.as_ref().and_then(|(_, a)| a[k]);
                match groups
                    .iter_mut()
                    .find(|(l, _, _)| l == low_end && modelled.is_none())
                {
                    Some((_, faces, _)) => faces.push(face),
                    None => groups.push((*low_end, vec![face], angle)),
                }
            }
            let mut defs = Vec::new();
            for (low_end, faces, angle) in groups {
                let mut def = json!({"type": "thread", "faces": faces, "thread": spec});
                if let Some((diameters, _)) = &modelled {
                    def["modeled"] = json!(true);
                    if let Some(d) = diameters {
                        def["diameters"] = d.clone();
                    }
                    // In ten-thousandths of a degree: the groove found
                    // where the faces cross (mitcad#161) places the flanks
                    // within a micrometre.
                    if let Some(a) = angle.map(|a| (a.to_degrees() * 1e4).round() / 1e4)
                        && a != 0.0
                    {
                        def["angle"] = json!(format!("{a} deg"));
                    }
                } else if let Some(d) = taper_diameters(info) {
                    // A cosmetic taper pipe thread (mitcad#733).
                    def["diameters"] = d;
                }
                // A taper pipe thread tapers (mitcad#733).
                if taper_diameters(info).is_some() {
                    def["taper"] = json!(true);
                }
                if partial {
                    def["length"] = self
                        .value_of(d.get("threadLength"))
                        .ok_or("no thread length")?;
                    if let Some(o) = self.value_of(d.get("threadOffset")) {
                        def["offset"] = o;
                    }
                    def["location"] = json!(if low_end { "low_end" } else { "high_end" });
                }
                defs.push(def);
            }
            Ok(defs)
        };
        let names: Vec<String> = threaded.iter().map(|t| t.1.clone()).collect();
        let plain = threads(&names)?;
        // Sizing a face by offsets or by a tube changes its geometric
        // representation, not its inputs. A phase measured from later
        // states is an inferred input even when that phase is zero.
        // A thread whose faces the stream decoder named, whose handedness it
        // read and whose class MitCad lists has every input read, a modelled
        // one when its start angle was read from the file too (the seam of a
        // whole cylinder, [`thread_phase`]) and not found in later states.
        let faces_named = faces
            .iter()
            .all(|r| matches!(r, Reference::Face(fp) if fp.f3d.is_some()));
        let measured = modelled.as_ref().is_some_and(|(_, angles)| {
            angles
                .iter()
                .zip(&phase_read)
                .any(|(angle, read)| angle.is_some() && !read)
        });
        let input_provenance = if measured {
            crate::InputProvenance::Inferred
        } else if (modelled.is_none() || phase_read.iter().all(|r| *r))
            && note.is_none()
            && faces_named
            && spec.get("right_handed").is_some()
        {
            crate::InputProvenance::Read
        } else {
            crate::InputProvenance::Unknown
        };
        let with = |n: &str| match &note {
            Some(note) => format!("{n}; {note}"),
            None => n.to_owned(),
        };
        let mut candidates = Vec::new();
        // A modelled thread sizes the threaded part of its faces itself:
        // first alone, then after the faces are sized as a whole.
        if modelled.is_some() {
            candidates.push(Candidate {
                core: CandidateCore::of(plain.clone()),
                note: note.clone(),
                guess: false,
                predicted: None,
                input_provenance,
                first: false,
            });
        }
        if threaded.iter().any(|t| t.4.is_some()) {
            // The volume sizing the faces as a whole adds (a cosmetic
            // thread adds none), so that the history's change ranks the
            // definitions (mitcad#68): a cosmetic thread whose state shows
            // no sizing is tried alone first.
            let sized_change = (modelled.is_none()).then(|| {
                threaded
                    .iter()
                    .filter_map(|(_, _, _, c, target)| {
                        let m = (*target)?;
                        let ring = std::f64::consts::PI * (m * m - c.radius * c.radius) * c.length;
                        // A shaft made thicker or a bore narrower gains.
                        Some(if c.internal { -ring } else { ring })
                    })
                    .sum::<f64>()
            });
            // The faces moved to the radius (out of the material: towards
            // the axis of a hole's wall). Offsetting a face of a large or
            // stored body can take long (38 s where the tube below took
            // 1.3 s) or not build: it is given a few seconds
            // (`Importer::try_candidate`, mitcad#68).
            // The chamfers at the faces' ends move with them (mitcad#335).
            let mut offsets: Vec<Value> = Vec::new();
            for ((body, face, _, c, target), cones) in threaded.iter().zip(&chamfers) {
                let Some(m) = *target else {
                    continue;
                };
                let distance = if c.internal {
                    c.radius - m
                } else {
                    m - c.radius
                };
                offsets.push(json!({"type": "offset_face", "body": body.to_string(),
                    "faces": [face], "distance": distance}));
                for (cone, cos) in cones {
                    offsets.push(json!({"type": "offset_face", "body": body.to_string(),
                        "faces": [cone.to_string()], "distance": distance * cos}));
                }
            }
            offsets.extend(plain.iter().cloned());
            candidates.push(Candidate {
                core: CandidateCore::of(offsets),
                note: Some(with("its faces sized to the thread's diameter")),
                guess: false,
                predicted: sized_change,
                input_provenance,
                first: false,
            });
            // Or the tube of material between the radii (a little wider on
            // the face's side) along the face, cut from or joined to the
            // body, where an offset does not build; the thread then goes on
            // the tube's side that is the new face.
            let mut defs = Vec::new();
            let mut sized = Vec::new();
            for (body, face, _, c, target) in &threaded {
                let Some(m) = *target else {
                    sized.push(face.clone());
                    continue;
                };
                let cylinder = |radius: f64| {
                    json!({"type": "cylinder",
                        "plane": {"origin": c.axis.origin, "normal": c.axis.direction},
                        "center": [0.0, 0.0], "diameter": 2.0 * radius, "height": c.length,
                        "operation": "new_body"})
                };
                let k = defs.len();
                let shrink = m < c.radius;
                let (outer, inner) = if shrink {
                    (c.radius + 0.01, m)
                } else {
                    (m, c.radius - 0.01)
                };
                // Material goes from a shaft made thinner and a bore made
                // wider, and comes to the others.
                let operation = if shrink != c.internal { "cut" } else { "join" };
                defs.push(cylinder(outer));
                defs.push(cylinder(inner));
                defs.push(json!({"type": "combine", "target": format!("${k}.b0"),
                    "tools": [format!("${}.b0", k + 1)], "operation": "cut", "keep_tools": false}));
                defs.push(json!({"type": "combine", "target": body.to_string(),
                    "tools": [format!("${k}.b0")], "operation": operation, "keep_tools": false}));
                // The new face: the tube's inside where the radius shrinks.
                sized.push(format!("${}:side0", if shrink { k + 1 } else { k }));
            }
            defs.extend(threads(&sized)?);
            candidates.push(Candidate {
                core: CandidateCore::of(defs),
                note: Some(with("its faces sized to the thread's diameter by a tube")),
                guess: false,
                predicted: sized_change,
                input_provenance,
                first: false,
            });
        }
        if modelled.is_none() {
            let sizes = threaded.iter().any(|t| t.4.is_some());
            candidates.push(Candidate {
                core: CandidateCore::of(plain),
                note,
                guess: false,
                predicted: sizes.then_some(0.0),
                input_provenance,
                first: false,
            });
        }
        Ok((candidates, joined))
    }

    /// The angle of a modelled thread on a face (`Cylinder`) as the next
    /// history states show it, in MitCad's terms (`ThreadSpec`): with the
    /// axis pointing up from the face's low end that way, where the groove
    /// lies at the pitch diameter at four angles about the axis, over a
    /// pitch about `middle` (along the face's own axis). The material
    /// there must fill half of each pitch; None when no state shows that
    /// consistently (the thread then takes angle 0).
    fn thread_angle(
        &mut self,
        c: &Cylinder,
        middle: f64,
        pitch: f64,
        pitch_radius: f64,
        right: bool,
    ) -> Option<f64> {
        use std::f64::consts::{PI, TAU};
        const SAMPLES: usize = 48;
        let (mut origin, mut along) = (c.axis.origin, geom::unit(c.axis.direction)?);
        let mut middle = middle;
        if !thread_upwards(along) {
            origin = geom::add(origin, geom::scale(along, c.length));
            along = geom::scale(along, -1.0);
            middle = c.length - middle;
        }
        let (reference, across) = thread_reference(along)?;
        let base = if c.internal { 0.75 } else { 0.25 } * pitch;
        let sign = if right { 1.0 } else { -1.0 };
        // Per angle, the samples along a pitch at the pitch radius: on a
        // segment from just before the first to just after the last.
        let mut points = Vec::new();
        let mut segments = Vec::new();
        let at = |radial: [f64; 3], s: f64| {
            geom::add(
                origin,
                geom::add(geom::scale(along, s), geom::scale(radial, pitch_radius)),
            )
        };
        for k in 0..4 {
            let theta = k as f64 * PI / 2.0;
            let radial = geom::add(
                geom::scale(reference, theta.cos()),
                geom::scale(across, theta.sin()),
            );
            for j in 0..SAMPLES {
                let s = middle - pitch / 2.0 + (j as f64 + 0.5) * pitch / SAMPLES as f64;
                points.push(at(radial, s));
            }
            segments.push((
                at(radial, middle - pitch / 2.0),
                at(radial, middle + pitch / 2.0),
            ));
        }
        let kernel = self.doc.kernel();
        for q in self.lookahead(3) {
            let clock = std::time::Instant::now();
            let Ok(state) = self.oracle.state(kernel, q) else {
                continue;
            };
            let mut inside = vec![false; points.len()];
            // Per angle, the groove's middle exactly where one body's faces
            // cross the segment (mitcad#161: the samples alone placed a
            // thread's flanks up to 0.015 mm off, too far for a sweep
            // along the flank to find it).
            let mut exact: Vec<Option<f64>> = vec![None; segments.len()];
            let mut crossed_by = vec![0usize; segments.len()];
            // Only the bodies of the thread's own component: another
            // component's in the same place (a nut's internal thread round
            // a bolt's, its mate) would fill the groove (mitcad#379).
            let component = self.component;
            let own = state
                .iter()
                .filter(|(body, _)| self.components.of_body(body.component) == component);
            for (body, _) in own {
                // Where the segments cross the body's faces (mitcad#68: a
                // classifier of every point took seconds on a stored
                // thread's spline faces), else the points classified.
                let crossed: Result<Vec<bool>, _> = segments
                    .iter()
                    .enumerate()
                    .map(|(k, (from, to))| {
                        let crossings = kernel.segment_crossings(&body.shape, *from, *to)?;
                        if !crossings.is_empty() {
                            crossed_by[k] += 1;
                            exact[k] =
                                groove_middle(&crossings).map(|f| middle - pitch / 2.0 + f * pitch);
                        }
                        Ok((0..SAMPLES)
                            .map(|j| inside_at(&crossings, (j as f64 + 0.5) / SAMPLES as f64))
                            .collect::<Vec<bool>>())
                    })
                    .collect::<Result<Vec<Vec<bool>>, mitcad_model::KernelError>>()
                    .map(|per_angle| per_angle.concat());
                let found = match crossed {
                    Ok(found) => Ok(found),
                    Err(_) => kernel.points_inside(&body.shape, &points),
                };
                if let Ok(found) = found {
                    for (a, b) in inside.iter_mut().zip(found) {
                        *a |= b;
                    }
                }
            }
            if crate::tracing() {
                eprintln!(
                    "import: thread angle, state {q} in {:.2} s",
                    clock.elapsed().as_secs_f64()
                );
            }
            // Per angle: the middle of the empty half (the groove), as a
            // mean direction on the circle of one pitch.
            let mut angles = Vec::new();
            for k in 0..4 {
                let (mut cx, mut cy, mut empty) = (0.0, 0.0, 0);
                for j in 0..SAMPLES {
                    if !inside[k * SAMPLES + j] {
                        let s = middle - pitch / 2.0 + (j as f64 + 0.5) * pitch / SAMPLES as f64;
                        let a = TAU * s / pitch;
                        cx += a.cos();
                        cy += a.sin();
                        empty += 1;
                    }
                }
                let fraction = empty as f64 / SAMPLES as f64;
                if crate::tracing() {
                    eprintln!(
                        "import: thread angle, state {q}, angle {k}: {fraction:.2} empty, mean {:.2}",
                        (cx * cx + cy * cy).sqrt() / (empty as f64).max(1.0)
                    );
                }
                // A half circle's mean direction has length 2/pi.
                if !(0.4..=0.6).contains(&fraction)
                    || (cx * cx + cy * cy).sqrt() / (empty as f64) < 0.55
                {
                    break;
                }
                let groove = match exact[k] {
                    Some(s) if crossed_by[k] == 1 => s,
                    _ => cy.atan2(cx) / TAU * pitch,
                };
                let theta = k as f64 * PI / 2.0;
                angles.push(theta - sign * (groove - base) * TAU / pitch);
            }
            if angles.len() < 4 {
                continue;
            }
            let (sx, sy) = angles
                .iter()
                .fold((0.0, 0.0), |(x, y), a| (x + a.cos(), y + a.sin()));
            let mean = sy.atan2(sx);
            let agree = angles.iter().all(|a| {
                let d = (a - mean).rem_euclid(TAU);
                d.min(TAU - d) < 0.2
            });
            if agree {
                return Some(mean);
            }
        }
        None
    }

    /// A tapped hole's thread on the walls of the holes of the `k`-th
    /// definition of its candidate (`count` holes): a thread feature, so
    /// that the hole keeps its own bore (the hole's `thread` would make it
    /// MitCad's basic minor diameter). A partial thread runs from the
    /// hole's start (the low end of its wall's cylinder). None for an
    /// untapped hole.
    fn hole_thread(
        &self,
        d: &Map<String, Value>,
        body: BodyUid,
        k: usize,
        count: usize,
        angles: &[Option<f64>],
    ) -> Result<Option<HoleThreads>, String> {
        let tapped = d
            .get("holeTapType")
            .and_then(Value::as_str)
            .is_some_and(|t| t.starts_with("Tapped"));
        if !tapped {
            return Ok(None);
        }
        let info = d.get("tappedHoleInfo").and_then(Value::as_object);
        let thread = d.get("thread").and_then(Value::as_object);
        let modeled = thread
            .and_then(|t| t.get("isModeled"))
            .and_then(Value::as_bool)
            == Some(true);
        let (spec, note) = thread_spec(info, thread, Some(true))?;
        let faces: Vec<Value> = (0..count)
            .map(|i| json!({"body": body.to_string(), "face": format!("${k}:hole{i}.wall")}))
            .collect();
        let mut def = json!({"type": "thread", "faces": faces, "thread": spec});
        if modeled {
            // The profile of the file's diameters, turned as the thread's
            // state shows it on each wall (`angles`; else 0).
            let standard: mitcad_model::features::thread_table::ThreadStandard =
                serde_json::from_value(
                    spec.get("standard").cloned().unwrap_or(json!("iso_metric")),
                )
                .map_err(|e| e.to_string())?;
            standard.check_modeled()?;
            def["modeled"] = json!(true);
            if let Some(d) = thread_diameters(info) {
                def["diameters"] = d;
            }
        } else if let Some(d) = taper_diameters(info) {
            // A cosmetic taper pipe thread tapers the bore from the
            // diameters it starts with (mitcad#733).
            def["diameters"] = d;
        }
        // A taper pipe thread starts at the hole's start, the low end of
        // its wall's cylinder, also through a plate (both ends free).
        if taper_diameters(info).is_some() {
            def["location"] = json!("low_end");
            def["taper"] = json!(true);
        }
        if let Some(t) = thread
            && t.get("isFullLength").and_then(Value::as_bool) == Some(false)
        {
            def["length"] = self
                .value_of(t.get("threadLength"))
                .ok_or("no thread length")?;
            if let Some(o) = self.value_of(t.get("threadOffset")) {
                def["offset"] = o;
            }
            def["location"] = json!("low_end");
        }
        // A modelled thread turned as measured: a thread of its own on each
        // wall whose angle differs from the first's (mitcad#733).
        if !modeled || angles.iter().all(|a| a.is_none_or(|a| a == 0.0)) {
            return Ok(Some((vec![def], note)));
        }
        let mut threads: Vec<(Option<f64>, Value)> = Vec::new();
        for (i, face) in faces.into_iter().enumerate() {
            let angle = angles.get(i).copied().flatten();
            match threads.iter_mut().find(|(a, _)| *a == angle) {
                Some((_, t)) => {
                    if let Some(list) = t["faces"].as_array_mut() {
                        list.push(face);
                    }
                }
                None => {
                    let mut t = def.clone();
                    t["faces"] = json!([face]);
                    // In ten-thousandths of a degree, as the threads' own.
                    if let Some(a) = angle.map(|a| (a.to_degrees() * 1e4).round() / 1e4)
                        && a != 0.0
                    {
                        t["angle"] = json!(format!("{a} deg"));
                    }
                    threads.push((angle, t));
                }
            }
        }
        Ok(Some((threads.into_iter().map(|(_, t)| t).collect(), note)))
    }

    /// The walls' cylinders of the `count` holes of the hole definition
    /// `def`: the walls do not exist before the hole, so the hole is made
    /// on its own, its walls read and the hole taken back. None for a wall
    /// not found; empty when the hole fails.
    fn hole_walls(&mut self, def: &Value, count: usize) -> Vec<Option<Cylinder>> {
        let depth = self.doc.undo_depth();
        let candidate = Candidate {
            core: CandidateCore::of(vec![def.clone()]),
            note: None,
            guess: false,
            predicted: None,
            input_provenance: crate::InputProvenance::Unknown,
            first: false,
        };
        let walls: Vec<Option<Cylinder>> = match self.add_all(&candidate, None) {
            Ok(uids) => {
                let kernel = self.doc.kernel();
                let bodies = crate::body_scope::bodies(self.doc);
                (0..count)
                    .map(|i| {
                        let wall = mitcad_model::features::hole::hole_face(uids[0], i, "wall");
                        bodies
                            .iter()
                            .find_map(|b| kernel.face_cylinder(b.shape, &wall).ok())
                    })
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        self.undo_to(depth);
        walls
    }

    /// Whether the one hole of `def` lies where the item's own history
    /// state has its hole (mitcad#733): eight points round the inside of its
    /// wall, at nine tenths of its radius halfway along it, all outside the
    /// state's bodies. A hole a few millimetres off the file's has some in
    /// the material; the file's own is clear of it. None without the
    /// history, the item's state or the wall.
    fn hole_on_state(&mut self, def: &Value) -> Option<bool> {
        if !self.oracle.enabled {
            return None;
        }
        let state = self.own_state?;
        let c = self.hole_walls(def, 1).into_iter().next().flatten()?;
        let along = geom::unit(c.axis.direction)?;
        let (across, other) = mitcad_model::features::thread::thread_reference(along)?;
        let middle = geom::add(c.axis.origin, geom::scale(along, c.length / 2.0));
        let points: Vec<Vec3> = (0..8)
            .map(|k| {
                let t = k as f64 * std::f64::consts::TAU / 8.0;
                let out = geom::add(geom::scale(across, t.cos()), geom::scale(other, t.sin()));
                geom::add(middle, geom::scale(out, 0.9 * c.radius))
            })
            .collect();
        let kernel = self.doc.kernel();
        let bodies = self.oracle.state(kernel, state).ok()?.to_vec();
        for (body, _) in &bodies {
            let inside = kernel.points_inside(&body.shape, &points).ok()?;
            if inside.into_iter().any(|i| i) {
                return Some(false);
            }
        }
        Some(true)
    }

    /// The angles of a tapped hole's modelled thread on the walls of the
    /// holes of `def` (`count` holes), measured in the thread's state as a
    /// thread item's are ([`Importer::thread_angle`]): the walls do not
    /// exist before the hole, so the hole is made on its own first, its
    /// walls' cylinders read and the hole taken back (mitcad#733: the
    /// modelled threads at angle 0 left the bodies after them 0.42 mm off
    /// the file's). Empty for a hole without a modelled right-handed
    /// thread; None for a wall not measured.
    fn hole_thread_angles(
        &mut self,
        d: &Map<String, Value>,
        def: &Value,
        count: usize,
    ) -> Vec<Option<f64>> {
        let thread = d.get("thread").and_then(Value::as_object);
        let modeled = thread
            .and_then(|t| t.get("isModeled"))
            .and_then(Value::as_bool)
            == Some(true);
        let right = thread
            .and_then(|t| t.get("isRightHanded"))
            .and_then(Value::as_bool)
            != Some(false);
        let tapped = d
            .get("holeTapType")
            .and_then(Value::as_str)
            .is_some_and(|t| t.starts_with("Tapped"));
        if !(tapped && modeled && right && self.oracle.enabled) {
            return Vec::new();
        }
        let info = d.get("tappedHoleInfo").and_then(Value::as_object);
        let Ok((spec, _)) = thread_spec(info, thread, Some(true)) else {
            return Vec::new();
        };
        let standard =
            serde_json::from_value(spec.get("standard").cloned().unwrap_or(json!("iso_metric")))
                .unwrap_or_default();
        let Ok(data) = mitcad_model::features::thread_table::lookup(
            standard,
            spec["designation"].as_str().unwrap_or_default(),
        ) else {
            return Vec::new();
        };
        let pitch_radius = thread_diameters(info)
            .and_then(|d| d["pitch"].as_f64())
            .unwrap_or(data.pitch_diameter)
            / 2.0;
        let partial = thread
            .and_then(|t| t.get("isFullLength"))
            .and_then(Value::as_bool)
            == Some(false);
        let length = partial
            .then(|| parameter_mm(thread.and_then(|t| t.get("threadLength"))))
            .flatten();
        let offset = parameter_mm(thread.and_then(|t| t.get("threadOffset"))).unwrap_or(0.0);
        self.hole_walls(def, count)
            .into_iter()
            .map(|wall| {
                let c = wall?;
                // The middle of the threaded part, from the hole's start
                // (the low end of the wall's axis).
                let middle = match length {
                    Some(l) => offset + l / 2.0,
                    None => c.length / 2.0,
                };
                self.thread_angle(&c, middle, data.pitch, pitch_radius, true)
            })
            .collect()
    }

    /// A combine the dump does not describe (the stream decoder): the
    /// bodies the next history states changed are its target and tools;
    /// the decoded `operation` (when not empty) first (mitcad#96).
    fn combine_by_history(&mut self, operation: &str) -> Result<Vec<Candidate>, String> {
        if !self.oracle.enabled {
            return Err("its bodies were not decoded (no history to find them)".to_owned());
        }
        // A body kept within the geometric tolerance of the state the replay
        // is at stands for its stored body, which the next states hold
        // exactly where the combine leaves it alone (mitcad#285).
        let stored = self.stored_counterparts();
        let kernel = self.doc.kernel();
        let current: Vec<(BodyUid, Option<Sig>)> = crate::body_scope::bodies(self.doc)
            .iter()
            .map(|b| {
                let sig = stored.get(&b.uid).copied();
                (b.uid, sig.or_else(|| Sig::of(kernel, b.shape)))
            })
            .collect();
        let mut candidates = Vec::new();
        for q in self.lookahead(3) {
            let Ok(state) = self.oracle.state(kernel, q) else {
                continue;
            };
            let mut used = vec![false; state.len()];
            let mut unmatched = Vec::new();
            for (uid, sig) in &current {
                let same =
                    sig.and_then(|s| (0..state.len()).find(|&j| !used[j] && state[j].1.same(&s)));
                match same {
                    Some(j) => used[j] = true,
                    None => unmatched.push((*uid, *sig)),
                }
            }
            // A body a replayed feature made (a sweep, a pipe) can have the
            // stored body's measures but its faces split otherwise: the same
            // measures alone keep it unchanged.
            let mut changed = Vec::new();
            for (uid, sig) in unmatched {
                let near = sig.and_then(|s| {
                    (0..state.len()).find(|&j| !used[j] && state[j].1.distance(&s) <= EXACT)
                });
                match near {
                    Some(j) => used[j] = true,
                    None => changed.push(uid),
                }
            }
            if changed.is_empty() {
                continue;
            }
            let others: Vec<BodyUid> = current
                .iter()
                .map(|(u, _)| *u)
                .filter(|u| !changed.contains(u))
                .collect();
            let mut push = |target: BodyUid, tools: Vec<BodyUid>, keep: bool| {
                let names: Vec<String> = tools.iter().map(ToString::to_string).collect();
                // Tools of another component than the target's by their links.
                let links = self.tool_links(target, &names).unwrap_or_default();
                for operation in ["join", "cut", "intersect"] {
                    let mut def = json!({"type": "combine", "target": target.to_string(),
                        "tools": names, "operation": operation, "keep_tools": keep});
                    if !links.is_empty() {
                        def["tool_links"] = Value::Object(links.clone());
                    }
                    if !candidates.iter().any(|c: &Candidate| c.defs[0] == def) {
                        let mut c = Candidate::new(def);
                        c.guess = true;
                        c.input_provenance = crate::InputProvenance::Inferred;
                        c.note = Some("target and tools found with the history".to_owned());
                        candidates.push(c);
                    }
                }
            };
            // Tools removed: the changed bodies are the target and tools.
            if changed.len() >= 2 {
                for &target in &changed {
                    let tools = changed.iter().copied().filter(|b| *b != target).collect();
                    push(target, tools, false);
                }
            }
            // Tools kept: one changed body, any other body as the tool;
            // then consumed (a tool the replay has in another place than
            // the history, so that it did not count as changed).
            if changed.len() == 1 {
                for &tool in &others {
                    push(changed[0], vec![tool], true);
                }
                for &tool in &others {
                    push(changed[0], vec![tool], false);
                }
            }
        }
        if candidates.is_empty() {
            return Err("no body changes in the next history states".to_owned());
        }
        if !operation.is_empty() {
            candidates.sort_by_key(|c| c.defs[0]["operation"] != operation);
        }
        Ok(candidates)
    }

    /// A replace face target (`targetFaces`): a construction plane, a face
    /// (as a fixed plane when it is planar and not in the replay), or the
    /// faces of one surface body (the body when they are all of it).
    fn replace_target(&self, targets: &[Reference]) -> Result<Value, String> {
        if let [r @ Reference::ConstructionPlane(_)] = targets {
            return self
                .plane_ref(r)
                .ok_or_else(|| "the target plane was not decoded".to_owned());
        }
        if let [Reference::Body(fp)] = targets {
            let body = refs::resolve_sheet(self.doc, fp).ok_or("the target body was not found")?;
            return Ok(json!({"body": body.to_string()}));
        }
        let mut faces: Vec<(BodyUid, String)> = Vec::new();
        for r in targets {
            let Reference::Face(fp) = r else {
                return Err("a target of an unsupported kind".to_owned());
            };
            match self.face_input(fp) {
                Some((b, f)) => faces.push((b, f.to_string())),
                None => {
                    // A planar face of a body the replay does not have.
                    let g = fp.geometry.as_ref().filter(|g| {
                        targets.len() == 1 && g.geometry_type.as_deref() == Some("Plane")
                    });
                    let plane = g.and_then(|g| Some((g.origin?, geom::unit(g.normal?)?)));
                    return match plane {
                        Some((origin, normal)) => {
                            Ok(json!({"origin": mm3(origin), "normal": normal}))
                        }
                        None => Err("the target face was not found in the replay".to_owned()),
                    };
                }
            }
        }
        match faces.as_slice() {
            [] => Err("its target was not decoded".to_owned()),
            [(b, f)] => Ok(json!({"body": b.to_string(), "face": f})),
            [(b, _), ..] => {
                if faces.iter().any(|(x, _)| x != b) {
                    return Err("target faces of several bodies".to_owned());
                }
                let shape = crate::body_scope::shape(self.doc, *b)
                    .ok_or("the target body was not found")?;
                let all = self.doc.kernel().face_count(shape).ok();
                if all != Some(faces.len()) {
                    return Err("target faces that are part of a body".to_owned());
                }
                Ok(json!({"body": b.to_string()}))
            }
        }
    }

    /// A replace face (P5). A dump does not name the faces replaced
    /// (`sourceFaces` is read when a dump has it):
    /// the history gives them, the faces of the body a next state changed
    /// that it no longer has. They are listed in full without the tangent
    /// chain, since MitCad's chain also takes faces tangent to them that
    /// stay (a slot's sides at its round end).
    fn replace_face(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let get = |k: &str| d.get(k);
        let targets = references(get("targetFaces"));
        if targets.is_empty() {
            return Err("its target was not decoded".to_owned());
        }
        let target = self.replace_target(&targets)?;
        let chain = get("isTangentChain")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut sources = references(get("sourceFaces"));
        sources.extend(references(get("inputFaces")));
        if !sources.is_empty() {
            let mut body = None;
            let mut faces = Vec::new();
            for r in &sources {
                let Reference::Face(fp) = r else { continue };
                let (b, f) = self.face_input(fp).ok_or("a face was not found")?;
                if body.is_some_and(|x| x != b) {
                    return Err("faces of several bodies".to_owned());
                }
                body = Some(b);
                faces.push(f.to_string());
            }
            let body = body.ok_or("its faces were not decoded")?;
            return Ok(vec![Candidate::new(
                json!({"type": "replace_face", "body": body.to_string(), "faces": faces,
                       "target": target, "tangent_chain": chain}),
            )]);
        }
        if !self.oracle.enabled {
            return Err("its source faces were not decoded (no history to find them)".to_owned());
        }
        // (Bodies kept within the tolerance as their stored ones, mitcad#285.)
        let stored = self.stored_counterparts();
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = crate::body_scope::bodies(self.doc)
            .iter()
            .map(|b| {
                let sig = stored.get(&b.uid).copied();
                (
                    b.uid,
                    b.shape.clone(),
                    sig.or_else(|| Sig::of(kernel, b.shape)),
                )
            })
            .collect();
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut last_error = None;
        for q in self.lookahead(3) {
            let kernel = self.doc.kernel();
            let state: Vec<(crate::StoredBody<K::Shape>, Sig)> = match self.oracle.state(kernel, q)
            {
                Ok(s) => s.to_vec(),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };
            // The replay's bodies without an equal in the state, against
            // the state's without one in the replay.
            let mut used = vec![false; state.len()];
            let mut changed = Vec::new();
            for (uid, shape, sig) in &bodies {
                let same =
                    sig.and_then(|s| (0..state.len()).find(|&j| !used[j] && state[j].1.same(&s)));
                match same {
                    Some(j) => used[j] = true,
                    None if sig.is_some() => changed.push((*uid, shape)),
                    None => {}
                }
            }
            let after: Vec<&K::Shape> = state
                .iter()
                .zip(&used)
                .filter(|(_, u)| !**u)
                .map(|((b, _), _)| &b.shape)
                .collect();
            for (uid, shape) in changed {
                let lost = match refs::lost_faces(kernel, shape, &after) {
                    Ok(l) => l,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };
                if lost.is_empty() {
                    continue;
                }
                let faces: Vec<String> = lost.iter().map(ToString::to_string).collect();
                let def = json!({"type": "replace_face", "body": uid.to_string(), "faces": faces,
                                 "target": target, "tangent_chain": false});
                if !candidates.iter().any(|c| c.defs[0] == def) {
                    let mut c = Candidate::new(def);
                    c.guess = true;
                    c.input_provenance = crate::InputProvenance::Inferred;
                    c.note = Some(format!(
                        "the faces replaced found with the history (tangent chain {})",
                        if chain { "on in the file" } else { "off" }
                    ));
                    candidates.push(c);
                }
            }
        }
        if candidates.is_empty() {
            return Err(last_error.unwrap_or_else(|| {
                "its source faces were not decoded, and no next history state lost faces".to_owned()
            }));
        }
        Ok(candidates)
    }

    /// A split face's tools (`splittingTool`): sketch profiles as the
    /// curves around the regions they select (the regions of the same area
    /// and centroid), then the whole sketch; a construction plane; a face.
    fn split_face_tools(&mut self, d: &Map<String, Value>) -> Result<Vec<Value>, String> {
        let tools = references(d.get("splittingTool"));
        match tools.first() {
            Some(Reference::Profile(p)) => {
                let index = p
                    .sketch_timeline_index
                    .flatten()
                    .ok_or("the tool's sketch was not decoded")?;
                let uid = self
                    .sketches
                    .get(&index)
                    .map(|s| s.uid)
                    .ok_or("the tool's sketch was not imported")?;
                let mut out = Vec::new();
                let output = self
                    .doc
                    .sketch_output(uid)
                    .ok_or("the tool's sketch did not evaluate")?;
                // Each profile's region by its area and centroid (cm², cm),
                // as `Importer::region_sets` matches them.
                let mut curves: Vec<String> = Vec::new();
                let mut all = true;
                for t in &tools {
                    let Reference::Profile(p) = t else {
                        all = false;
                        continue;
                    };
                    let (Some(area), Some(centroid)) = (p.area, p.centroid) else {
                        all = false;
                        continue;
                    };
                    let area = area * 100.0;
                    let c = [mm(centroid[0]), mm(centroid[1])];
                    let tol = 1e-3 * area.sqrt().max(1.0);
                    let found = output.region_info.iter().find(|r| {
                        (r.area - area).abs() <= 1e-3 * area.abs().max(1e-6)
                            && (r.centroid[0] - c[0]).hypot(r.centroid[1] - c[1]) <= tol
                    });
                    let Some(r) = found else {
                        all = false;
                        continue;
                    };
                    for s in r.profile.key.segments() {
                        let c = s.curve.to_string();
                        if !curves.contains(&c) {
                            curves.push(c);
                        }
                    }
                }
                if all && !curves.is_empty() {
                    out.push(json!({"sketch": uid.to_string(), "curves": curves}));
                }
                out.push(json!({"sketch": uid.to_string()}));
                Ok(out)
            }
            Some(r @ Reference::ConstructionPlane(_)) => Ok(vec![
                self.plane_ref(r)
                    .ok_or("the splitting plane was not decoded")?,
            ]),
            Some(Reference::Face(fp)) => {
                let (b, f) = self
                    .face_input(fp)
                    .ok_or("the splitting face was not found")?;
                Ok(vec![json!({"body": b.to_string(), "face": f.to_string()})])
            }
            _ => Err("its splitting tool was not decoded".to_owned()),
        }
    }

    /// A split face (the `.ipt` import's, mitcad#60). Its faces are not
    /// decoded: they are the faces of the body a next history state
    /// changed (the decoded `body` when there is one) that the state no
    /// longer has whole, no face of its body there of the same surface and
    /// area.
    fn split_face(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
        let tools = self.split_face_tools(d)?;
        if !self.oracle.enabled {
            return Err("its faces were not decoded (no history to find them)".to_owned());
        }
        let only = d
            .get("body")
            .and_then(reference)
            .and_then(|r| self.body_of(&r));
        // (Bodies kept within the tolerance as their stored ones, mitcad#285.)
        let stored = self.stored_counterparts();
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = crate::body_scope::bodies(self.doc)
            .iter()
            .filter(|b| only.is_none_or(|o| o == b.uid))
            .map(|b| {
                let sig = stored.get(&b.uid).copied();
                (
                    b.uid,
                    b.shape.clone(),
                    sig.or_else(|| Sig::of(kernel, b.shape)),
                )
            })
            .collect();
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut last_error = None;
        for q in self.lookahead(3) {
            let state: Vec<(crate::StoredBody<K::Shape>, Sig)> = match self.oracle.state(kernel, q)
            {
                Ok(s) => s.to_vec(),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };
            let mut used = vec![false; state.len()];
            let mut changed = Vec::new();
            for (uid, shape, sig) in &bodies {
                let Some(sig) = sig else { continue };
                if let Some(j) = (0..state.len()).find(|&j| !used[j] && state[j].1.same(sig)) {
                    used[j] = true;
                } else {
                    changed.push((*uid, shape, *sig));
                }
            }
            if crate::tracing() {
                eprintln!(
                    "import: split face: state {q}: {} of {} bodies changed, {} stored",
                    changed.len(),
                    bodies.len(),
                    state.len()
                );
            }
            for (uid, shape, sig) in changed {
                // The same solid with its faces split: the state's body of
                // the same measures.
                let Some(j) =
                    (0..state.len()).find(|&j| !used[j] && state[j].1.distance(&sig) <= RELATIVE)
                else {
                    continue;
                };
                let (Ok(mine), Ok(stored)) = (kernel.faces(shape), kernel.faces(&state[j].0.shape))
                else {
                    continue;
                };
                if crate::tracing() {
                    eprintln!(
                        "import: split face: state {q}: {} faces of {uid} against {} stored",
                        mine.len(),
                        stored.len()
                    );
                }
                // A small piece cut off a large face leaves it almost its
                // area: a tight tolerance first, then a looser one for
                // replayed faces that differ from the stored ones a little.
                for tolerance in [1e-8, 1e-5] {
                    // Each stored face stands for one face (faces of the
                    // same area, as opposite sides, are told apart so).
                    let mut taken = vec![false; stored.len()];
                    let mut faces: Vec<String> = Vec::new();
                    for f in &mine {
                        let off = |k: usize| (stored[k].area - f.area).abs();
                        let near = (0..stored.len())
                            .filter(|&k| {
                                !taken[k]
                                    && stored[k].surface == f.surface
                                    && off(k) <= tolerance * f.area.abs().max(1e-6)
                            })
                            .min_by(|&a, &b| off(a).total_cmp(&off(b)));
                        match near {
                            Some(k) => taken[k] = true,
                            None => faces.extend(f.names.first().cloned()),
                        }
                    }
                    if faces.is_empty() {
                        continue;
                    }
                    for tool in &tools {
                        let def = json!({"type": "split_face", "body": uid.to_string(),
                                     "faces": faces, "tool": tool});
                        if !candidates.iter().any(|c| c.defs[0] == def) {
                            let mut c = Candidate::new(def);
                            c.guess = true;
                            c.input_provenance = crate::InputProvenance::Inferred;
                            c.note = Some("the faces split found with the history".to_owned());
                            candidates.push(c);
                        }
                    }
                }
            }
        }
        if candidates.is_empty() {
            return Err(last_error.unwrap_or_else(|| {
                "its faces were not decoded, and no next history state split faces".to_owned()
            }));
        }
        Ok(candidates)
    }

    /// A body reference that names nothing to find it by (no producer,
    /// name or measures: a body of the `.ipt` import's base features) is
    /// the replay's only solid, when it has one.
    fn only_solid(&self, fp: &Fingerprint) -> Option<BodyUid> {
        let anything = fp.name.is_some()
            || fp.volume.is_some()
            || fp.f3d.as_ref().is_some_and(|f| {
                f.producer.is_some() || f.edge_points.as_ref().is_some_and(|p| !p.is_empty())
            });
        if anything {
            return None;
        }
        let kernel = self.doc.kernel();
        let solids: Vec<BodyUid> = crate::body_scope::bodies(self.doc)
            .iter()
            .filter(|b| matches!(kernel.body_kind(b.shape), Ok(mitcad_model::BodyKind::Solid)))
            .map(|b| b.uid)
            .collect();
        match solids[..] {
            [only] => Some(only),
            _ => None,
        }
    }

    /// A shell whose removed faces are not decoded (the `.ipt` import's,
    /// mitcad#60): the faces of its body that a next history state no
    /// longer has (an opening where each was), as for a replace face.
    fn shell_by_history(&mut self, body: BodyUid, def: Value) -> Result<Vec<Candidate>, String> {
        if !self.oracle.enabled {
            return Err("its faces were not decoded (no history to find them)".to_owned());
        }
        let shape = crate::body_scope::shape(self.doc, body)
            .ok_or("the shelled body was not found")?
            .clone();
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut last_error = None;
        for q in self.lookahead(3) {
            let kernel = self.doc.kernel();
            let state = match self.oracle.state(kernel, q) {
                Ok(s) => s.to_vec(),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };
            let after: Vec<&K::Shape> = state.iter().map(|(b, _)| &b.shape).collect();
            // Every point of a removed face off the state's faces, else
            // most of them (those near its edges can lie on the wall's
            // end).
            for share in [1.0, 0.5] {
                let lost = match refs::lost_faces_by(kernel, &shape, &after, share) {
                    Ok(l) => l,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };
                if lost.is_empty() {
                    continue;
                }
                let mut d = def.clone();
                d["faces"] = json!(lost.iter().map(ToString::to_string).collect::<Vec<_>>());
                if !candidates.iter().any(|c| c.defs[0] == d) {
                    let mut c = Candidate::new(d);
                    c.guess = true;
                    c.input_provenance = crate::InputProvenance::Inferred;
                    c.note = Some("the faces removed found with the history".to_owned());
                    candidates.push(c);
                }
            }
        }
        if candidates.is_empty() {
            return Err(last_error.unwrap_or_else(|| {
                "its faces were not decoded, and no next history state lost faces".to_owned()
            }));
        }
        Ok(candidates)
    }
}

/// A copy of `bodies` that stays where they are (a `move` that copies with
/// the identity), the translation of a copy and paste in the bodies' own
/// component.
fn copy_in_place(bodies: &[BodyUid]) -> Candidate {
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    let bodies: Vec<String> = bodies.iter().map(ToString::to_string).collect();
    let mut c = Candidate::new(json!({"type": "move", "bodies": bodies, "copy": true,
        "transform": {"type": "free", "matrix": identity}}));
    c.input_provenance = InputProvenance::Read;
    c
}

/// Items that bring bodies and leave no tool (mitcad#641): a pattern or
/// mirror of one copies the bodies it brought.
fn brings_bodies(object_type: Option<&str>) -> bool {
    matches!(
        object_type,
        Some("CopyPasteBodies" | "BaseFeature" | "ExternalDerived" | "DerivedContext")
    )
}

/// A modelled thread's diameters from the dump's ThreadInfo (centimetres):
/// the major, minor and pitch diameters in millimetres, when all are given
/// and in order.
/// The note of a pattern or mirror that repeats its fillets and chamfers.
const DRESSED: &str = "its fillets and chamfers repeated on the copies";

/// A pattern's or mirror's candidate; one whose objects have the fillets
/// and chamfers among them says so (mitcad#105).
fn dressed_note(def: Value, dressed: bool, provenance: InputProvenance) -> Candidate {
    let mut candidate = Candidate::new(def);
    candidate.input_provenance = provenance;
    if dressed {
        candidate.with_note(DRESSED)
    } else {
        candidate
    }
}

/// A pattern or mirror whose objects, axis or plane, counts and settings
/// all come from the file: its inputs are read, and it is tried before
/// the history's suggestions (mitcad#186), except the one that repeats
/// fillets and chamfers on the copies, which stays after them (it takes
/// long on large bodies).
pub const PATTERNED_NOT_IMPORTED: &str = "a patterned feature was not imported";

/// Whether a circular or rectangular pattern's history suggestions are left
/// out (mitcad#181): its decoded definitions have read inputs, or the
/// feature it copies did not come in.
pub fn pattern_not_searched(object_type: &str, decoded: &Result<Vec<Candidate>, String>) -> bool {
    matches!(
        object_type,
        "CircularPatternFeature" | "RectangularPatternFeature"
    ) && match decoded {
        Ok(candidates) => candidates
            .iter()
            .any(|c| c.input_provenance == InputProvenance::Read && !c.guess),
        Err(e) => e == PATTERNED_NOT_IMPORTED,
    }
}

fn read_pattern(mut candidate: Candidate, read: bool) -> Candidate {
    if read && candidate.input_provenance == InputProvenance::Unknown {
        candidate.input_provenance = InputProvenance::Read;
        candidate.first = candidate.note.as_deref() != Some(DRESSED);
    }
    candidate
}

fn is_dressup(r: &Reference) -> bool {
    matches!(r, Reference::Feature(f) if matches!(
        f.object_type.as_deref(),
        Some("FilletFeature" | "ChamferFeature")
    ))
}

fn thread_diameters(info: Option<&Map<String, Value>>) -> Option<Value> {
    let info = info?;
    // A taper pipe thread's where it starts (mitcad#733).
    if let Some(taper) = taper_diameters(Some(info)) {
        return Some(taper);
    }
    let get = |key: &str| info.get(key).and_then(Value::as_f64).map(mm);
    let (major, minor, pitch) = (
        get("majorDiameter")?,
        get("minorDiameter")?,
        get("pitchDiameter")?,
    );
    (minor > 0.0 && minor < pitch && pitch < major)
        .then(|| json!({"major": major, "minor": minor, "pitch": pitch}))
}

/// A tapped hole's thread definitions (one per angle of a modelled
/// thread's walls) and a note on what was left out of them.
type HoleThreads = (Vec<Value>, Option<String>);

/// Whether a hole's distances from edges (or along a shaft) come before the
/// point the file keeps with it (mitcad#733): unless the hole they give is
/// not where the item's state has its hole (`offsets`, Some(false)) while
/// the point's is (`point`, asked only then). Without the history (None)
/// the distances first: they are the file's dimensions.
fn offsets_first(offsets: Option<bool>, point: impl FnOnce() -> Option<bool>) -> bool {
    offsets != Some(false) || point() != Some(true)
}

/// A taper pipe thread's diameters where it starts, from the taper the
/// stream decoder gives (`_f3d_taper`, mitcad#733): the pitch diameter at
/// its small end grown by the taper over its length (2 tan of the half
/// angle per length, 1:16), the major and minor diameters its height
/// above and below. The bores of the corpus' NPT and ISO 7-1 holes start
/// so (the history states' cones, within 1e-4 mm). None for a straight
/// thread.
fn taper_diameters(info: Option<&Map<String, Value>>) -> Option<Value> {
    let taper = info?.get("_f3d_taper")?.as_object()?;
    let get = |key: &str| taper.get(key).and_then(Value::as_f64);
    let angle = get("angle")?;
    let (height, length, small) = (
        mm(get("height")?),
        mm(get("length")?),
        mm(get("smallPitchDiameter")?),
    );
    let pitch = small + length * 2.0 * (angle / 2.0).tan();
    (height > 0.0 && pitch > height)
        .then(|| json!({"major": pitch + height, "minor": pitch - height, "pitch": pitch}))
}

/// Whether the point at `t` (a fraction of the way) along a segment lies
/// in the material, by where the segment crosses the faces
/// ([`Kernel::segment_crossings`]): before a crossing that enters it, out;
/// after the last, as that one left it. A segment that crosses nothing
/// counts as outside.
/// The middle of the one stretch outside the material along a segment one
/// pitch long whose crossings are `crossings` ([`Kernel::segment_crossings`]
/// with the inside before the first as [`inside_at`] takes it), as a
/// fraction of the way, the stretch taken round the ends (a groove across
/// the segment's ends); None unless there is exactly one stretch outside.
pub fn groove_middle(crossings: &[(f64, bool)]) -> Option<f64> {
    // The stretches outside: from where the segment leaves the material
    // to where it enters it again.
    let mut outside: Vec<(f64, f64)> = Vec::new();
    let mut from = (!inside_at(crossings, 0.0)).then_some(0.0);
    for &(at, entering) in crossings {
        match (entering, from) {
            (true, Some(start)) => {
                outside.push((start, at));
                from = None;
            }
            (false, None) => from = Some(at),
            _ => return None,
        }
    }
    if let Some(start) = from {
        outside.push((start, 1.0));
    }
    let (a, b) = match outside.as_slice() {
        [one] => *one,
        // One stretch round the ends (both ends were set exactly so).
        [(start, b), (a, end)] if *start == 0.0 && *end == 1.0 => (*a, b + 1.0),
        _ => return None,
    };
    Some((0.5 * (a + b)).rem_euclid(1.0))
}

pub fn inside_at(crossings: &[(f64, bool)], t: f64) -> bool {
    match crossings.iter().find(|(at, _)| *at > t) {
        Some((_, entering)) => !entering,
        None => crossings.last().is_some_and(|(_, entering)| *entering),
    }
}

/// The cylindrical faces of a body that continue `seed` (the cylinder of
/// `face`): on the same cylinder, on the same side of the material, and
/// end to end with it or with each other along the axis. A face the file
/// keeps whole is in pieces where the replay's operations split it.
fn coaxial_pieces<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    face: &FaceName,
    seed: &Cylinder,
) -> Vec<(FaceName, Cylinder)> {
    const TOLERANCE: f64 = 1e-5;
    let Some(up) = geom::unit(seed.axis.direction) else {
        return Vec::new();
    };
    let Ok(faces) = kernel.faces(shape) else {
        return Vec::new();
    };
    // The cylinders on the seed's surface, with their stretches along its
    // axis (from the seed's low end).
    let mut others: Vec<(FaceName, Cylinder, f64, f64)> = Vec::new();
    for info in faces.iter().filter(|f| f.surface == "cylinder") {
        let Some(name) = info.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
            continue;
        };
        if name == *face {
            continue;
        }
        let Ok(c) = kernel.face_cylinder(shape, &name) else {
            continue;
        };
        let Some(along) = geom::unit(c.axis.direction) else {
            continue;
        };
        let from = geom::sub(c.axis.origin, seed.axis.origin);
        let t = geom::dot(from, up);
        let off = geom::norm(geom::sub(from, geom::scale(up, t)));
        if c.internal != seed.internal
            || (c.radius - seed.radius).abs() > TOLERANCE
            || geom::dot(along, up).abs() < 1.0 - 1e-9
            || off > TOLERANCE
        {
            continue;
        }
        let (lo, hi) = if geom::dot(along, up) > 0.0 {
            (t, t + c.length)
        } else {
            (t - c.length, t)
        };
        others.push((name, c, lo, hi));
    }
    let (mut lo, mut hi) = (0.0, seed.length);
    let mut taken = Vec::new();
    while let Some(k) = others
        .iter()
        .position(|(_, _, a, b)| (a - hi).abs() <= TOLERANCE || (b - lo).abs() <= TOLERANCE)
    {
        let (name, c, a, b) = others.swap_remove(k);
        if (a - hi).abs() <= TOLERANCE {
            hi = b;
        } else {
            lo = a;
        }
        taken.push((name, c));
    }
    taken
}

/// The chamfers at the ends of a threaded face (`Cylinder`) that the
/// file's sizing of the face moves with it (mitcad#335): the cones about
/// the face's axis that meet it at one of its end circles and run on away
/// from it towards the axis (an external face's chamfer) or away from it
/// (an internal face's countersink), with the cosine of their angle to the
/// axis. Moving a face along the radius by `d` moves such a cone by
/// `d cos(angle)` along its normal: its edge with the face stays where it
/// is, and the cone stays the chamfer of the sized face, as the bodies
/// stored after a modelled thread show it (a 2 mm chamfer at a bolt's free
/// end stays a 2 mm chamfer of the face sized to the major diameter).
fn end_chamfers<K: Kernel>(kernel: &K, shape: &K::Shape, seed: &Cylinder) -> Vec<(FaceName, f64)> {
    use mitcad_model::datum::SurfaceGeometry;
    const TOLERANCE: f64 = 1e-5;
    let Some(up) = geom::unit(seed.axis.direction) else {
        return Vec::new();
    };
    let Some((reference, across)) = thread_reference(up) else {
        return Vec::new();
    };
    let Ok(faces) = kernel.faces(shape) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for info in faces.iter().filter(|f| f.surface == "cone") {
        let Some(name) = info.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
            continue;
        };
        let Ok(SurfaceGeometry::Cone {
            origin,
            axis,
            radius,
            half_angle,
        }) = kernel.face_geometry(shape, &name)
        else {
            continue;
        };
        let Some(axis) = geom::unit(axis) else {
            continue;
        };
        // About the face's axis.
        let from = geom::sub(origin, seed.axis.origin);
        let off = geom::norm(geom::sub(from, geom::scale(up, geom::dot(from, up))));
        if geom::dot(axis, up).abs() < 1.0 - 1e-9 || off > TOLERANCE {
            continue;
        }
        // The cone's radius at a point along the face's axis.
        let radius_at = |s: f64| {
            let at = geom::add(seed.axis.origin, geom::scale(up, s));
            radius + geom::dot(geom::sub(at, origin), axis) * half_angle.tan()
        };
        for (end, outward) in [(0.0, -1.0), (seed.length, 1.0)] {
            if (radius_at(end) - seed.radius).abs() > TOLERANCE {
                continue;
            }
            // Away from the face: narrowing outside, widening inside.
            let beyond = radius_at(end + outward * 0.1);
            if (beyond < seed.radius) == seed.internal {
                continue;
            }
            // The cone's face itself holds the end circle (at one of four
            // points: it may be a part of a turn).
            let centre = geom::add(seed.axis.origin, geom::scale(up, end));
            let meets = (0..4).any(|k| {
                let theta = k as f64 * std::f64::consts::FRAC_PI_2;
                let radial = geom::add(
                    geom::scale(reference, theta.cos()),
                    geom::scale(across, theta.sin()),
                );
                let point = geom::add(centre, geom::scale(radial, seed.radius));
                kernel
                    .face_point_normal(shape, &name, point)
                    .is_ok_and(|near| geom::norm(geom::sub(near.point, point)) <= TOLERANCE)
            });
            if meets {
                found.push((name.clone(), half_angle.abs().cos()));
                break;
            }
        }
    }
    found
}

// Whether an axis points "up" as a modelled thread's helix takes it, and
// its reference direction (the model's, mitcad#733).
use mitcad_model::features::thread::{thread_reference, thread_upwards};

/// The angle (radians, about the axis pointing up, from its reference
/// direction) at which a right-handed modelled thread's helix starts at the
/// low end of the axis, read from the file: the file's helix starts at the
/// seam of the cylinder's surface, at the end of the face its axis starts
/// from, whatever the thread's offset, length and location. With the file's
/// axis pointing up that end is where MitCad's helix starts too, and the
/// angle is the seam's direction. It agrees with the angle found in later
/// states on controlled variants (seams turned 0, 45 and 90 degrees,
/// offsets 0, 1 and 2.5 mm) and on the census' 20 threads on a whole cylinder
/// with its axis up (18 exactly, two within 2.3 degrees). `None` where
/// the file's axis points down (the helix starts at the other end: not
/// read) or the face is a part of a cylinder (no seam).
fn thread_phase(axis: Option<Vec3>, seam: Option<Vec3>) -> Option<f64> {
    let along = geom::unit(axis?)?;
    if !thread_upwards(along) {
        return None;
    }
    let (reference, across) = thread_reference(along)?;
    let seam = geom::unit(seam?)?;
    Some(geom::dot(seam, across).atan2(geom::dot(seam, reference)))
}

/// A parameter's value in millimetres.
fn parameter_mm(v: Option<&Value>) -> Option<f64> {
    reference(v?)?.parameter()?.value.map(mm)
}

/// A thread size from the dump's ThreadInfo (and the thread feature's
/// flags), checked against MitCad's thread table: ISO metric (also the
/// `M` sizes of other metric types), Unified, and the parallel pipe
/// threads (`G 1/4-19` → MitCad's `G 1/4`). A class MitCad does not list
/// for the face's side (`internal`, when known) is left out, with a note:
/// it only labels a cosmetic thread.
fn thread_spec(
    info: Option<&Map<String, Value>>,
    feature: Option<&Map<String, Value>>,
    internal: Option<bool>,
) -> Result<(Value, Option<String>), String> {
    use mitcad_model::features::thread_table::{self, ThreadStandard};
    let info = info.ok_or("its thread was not decoded")?;
    let kind = info.get("threadType").and_then(Value::as_str);
    let designation = info
        .get("threadDesignation")
        .and_then(Value::as_str)
        .ok_or("no thread designation")?;
    let (standard, designation) = match kind {
        Some(t) if t.contains("Unified") => (ThreadStandard::Unified, designation.to_owned()),
        // Taper pipe threads (mitcad#733): `Rc 1/2` (ISO first: its type
        // may name a taper pipe thread too), `1/2 - 14 NPT`.
        Some(t) if t.starts_with("ISO Taper") => (ThreadStandard::IsoTaper, designation.to_owned()),
        Some(t) if t.contains("NPT") || t.contains("Taper Pipe") => {
            (ThreadStandard::Npt, designation.to_owned())
        }
        Some(t) if t.contains("Pipe") && t.starts_with("BSP") => {
            // `G <size>-<threads per inch>`.
            let pipe = designation
                .rsplit_once('-')
                .filter(|(size, tpi)| {
                    let data = thread_table::lookup(ThreadStandard::Whitworth, size);
                    let tpi: Option<f64> = tpi.trim().parse().ok();
                    data.ok()
                        .zip(tpi)
                        .is_some_and(|(d, t)| (d.pitch - 25.4 / t).abs() < 1e-9)
                })
                .map(|(size, _)| size.trim().to_owned());
            let pipe =
                pipe.ok_or_else(|| format!("{designation} is not in MitCad's thread table"))?;
            (ThreadStandard::Whitworth, pipe)
        }
        Some(t) if t.contains("Metric") && t.contains("Forming") => {
            (ThreadStandard::FormingScrew, designation.to_owned())
        }
        Some(t) if t.contains("Metric") => (ThreadStandard::IsoMetric, designation.to_owned()),
        None => (ThreadStandard::IsoMetric, designation.to_owned()),
        Some(t) => return Err(format!("thread type {t} is not supported")),
    };
    thread_table::lookup(standard, &designation)?;
    let mut spec = json!({"designation": designation});
    if standard != ThreadStandard::IsoMetric {
        spec["standard"] = json!(standard);
    }
    let mut note = None;
    // (Taper pipe threads store none.)
    if let Some(class) = info
        .get("threadClass")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
    {
        match thread_table::check_class(standard, class, internal) {
            Ok(()) => spec["class"] = json!(class),
            Err(why) => note = Some(format!("its thread class is left out ({why})")),
        }
    }
    let right = feature
        .and_then(|f| f.get("isRightHanded"))
        .or_else(|| info.get("isRightHanded"))
        .and_then(Value::as_bool);
    if let Some(right) = right {
        spec["right_handed"] = json!(right);
    }
    Ok((spec, note))
}

fn suppressed(def: &mut Value, ids: Option<&Value>) {
    let ids: Vec<u64> = ids
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    if !ids.is_empty() {
        def["suppressed_elements"] = json!(ids);
    }
}

/// The faces a hole along the bore of a removed `piece` of `body` can start
/// from: the planes of the body's planar faces across the bore's axis at
/// either end of the piece, each with the point where the axis meets it.
/// The bore is the piece's cylinder nearest the hole's radius (the longest
/// without one). None when the piece has no cylinder or no such plane.
fn axis_starts<K: Kernel>(
    kernel: &K,
    piece: &K::Shape,
    radius: Option<f64>,
    faces: &[(BodyUid, FaceName, Plane)],
    body: BodyUid,
) -> Option<Vec<StartFace>> {
    // The piece's faces are named to be asked about.
    let data = kernel.brep_data(piece).ok()?;
    let (named, _) = kernel.import_brep(FeatureUid(0), &data, 0).ok()?;
    let mut bore: Option<(f64, Cylinder)> = None;
    for face in kernel.faces(&named).ok()? {
        let Some(name) = face.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
            continue;
        };
        if face.surface != "cylinder" {
            continue;
        }
        crate::tick();
        let Ok(c) = kernel.face_cylinder(&named, &name) else {
            continue;
        };
        let score = radius.map_or(-c.length, |r| (c.radius - r).abs());
        if bore.is_none_or(|(s, _)| score < s) {
            bore = Some((score, c));
        }
    }
    let (_, bore) = bore?;
    let along = geom::unit(bore.axis.direction)?;
    let origin = bore.axis.origin;
    // The piece's extent along the axis (exact for an axis along the
    // model's axes).
    let bounds = kernel.bounding_box(piece).ok()??;
    let reach: Vec<f64> = bounds
        .corners()
        .map(|p| geom::dot(geom::sub(p, origin), along))
        .collect();
    let low = reach.iter().copied().fold(f64::INFINITY, f64::min);
    let high = reach.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut found: Vec<StartFace> = Vec::new();
    for (_, face, plane) in faces.iter().filter(|(b, _, _)| *b == body) {
        let slope = geom::dot(plane.normal, along);
        if slope.abs() < 1.0 - 1e-6 {
            continue;
        }
        // The hole runs against the face's normal, into the material: the
        // plane lies at the end of the piece the normal points to.
        let at = geom::dot(geom::sub(plane.origin, origin), along);
        let end = if slope > 0.0 { high } else { low };
        if (at - end).abs() > 1e-3 {
            continue;
        }
        let point = geom::add(origin, geom::scale(along, at));
        let same_plane = found
            .iter()
            .any(|(_, _, p)| geom::distance(*p, point) < 1e-6);
        if !same_plane {
            found.push(((at - end).abs(), face.to_string(), point));
        }
    }
    (!found.is_empty()).then_some(found)
}

/// [`Importer::bridging_faces`] on the recipes' names (`named`, each
/// selected face's: its own names, then its neighbours') and the faces of
/// the replay's bodies that meet in an edge (`edges`, by name).
pub fn bridging_faces(
    named: &[Vec<Vec<EntityName>>],
    faces: &[(BodyUid, String)],
    edges: &mut dyn FnMut(BodyUid) -> Vec<[String; 2]>,
) -> Vec<(BodyUid, String)> {
    // A recipe's own names (its first entity) among another's neighbours.
    let names = |i: usize, j: usize| {
        let (Some(own), Some(others)) = (named[i].first(), named[j].get(1..)) else {
            return false;
        };
        others.iter().any(|o| o.iter().any(|n| own.contains(n)))
    };
    let mut adjacency: HashMap<BodyUid, HashMap<String, HashSet<String>>> = HashMap::new();
    let mut out: Vec<(BodyUid, String)> = Vec::new();
    let count = faces.len().min(named.len());
    for i in 0..count {
        for j in i + 1..count {
            let ((body, a), (other, b)) = (&faces[i], &faces[j]);
            if body != other || !names(i, j) || !names(j, i) {
                continue;
            }
            let next = adjacency.entry(*body).or_insert_with(|| {
                let mut next: HashMap<String, HashSet<String>> = HashMap::new();
                for [x, y] in edges(*body) {
                    next.entry(x.clone()).or_default().insert(y.clone());
                    next.entry(y).or_default().insert(x);
                }
                next
            });
            let (Some(on_a), Some(on_b)) = (next.get(a), next.get(b)) else {
                continue;
            };
            if on_a.contains(b) {
                continue;
            }
            // Not a face the others meet too (the plate's sides that every
            // wall of a cut-out meets).
            let between: Vec<&String> = on_a
                .intersection(on_b)
                .filter(|f| !faces.iter().any(|(o, s)| o == body && s == *f))
                .filter(|f| {
                    !faces.iter().any(|(o, s)| {
                        o == body && s != a && s != b && next.get(s).is_some_and(|n| n.contains(*f))
                    })
                })
                .collect();
            if let [face] = between[..]
                && !out.iter().any(|(o, s)| o == body && s == face)
            {
                out.push((*body, face.clone()));
            }
        }
    }
    out
}

/// A pattern axis from the line its direction input stores
/// (`_f3d_axis`: origin in cm and unit direction, mitcad#96): an origin
/// axis when it is one, else a fixed axis.
fn decoded_line(v: Option<&Value>) -> Option<Value> {
    let origin: Vec3 = serde_json::from_value(v?.get("origin")?.clone()).ok()?;
    let direction = geom::unit(serde_json::from_value(v?.get("direction")?.clone()).ok()?)?;
    let origin = mm3(origin);
    for (name, a) in AXES {
        let along = geom::dot(direction, a).abs();
        let off = geom::norm(geom::cross(origin, a));
        if (along - 1.0).abs() < 1e-9 && off < 1e-3 {
            return Some(json!(name));
        }
    }
    Some(json!({"origin": origin, "direction": direction}))
}

/// Whether the decoder found a hole's through-all flag set while its extent
/// is the depth the file keeps (`_f3d_through_all`, mitcad#96).
fn through_all_lead(d: &Map<String, Value>) -> bool {
    d.get("_f3d_through_all").and_then(Value::as_bool) == Some(true)
}

/// A fixed axis from a construction axis' line geometry.
fn fixed_axis(g: &Geometry) -> Option<Value> {
    let origin = g.origin.or(g.start_point)?;
    let direction = g
        .direction
        .or_else(|| Some(geom::sub(g.end_point?, g.start_point?)))?;
    Some(json!({"origin": mm3(origin), "direction": geom::unit(direction)?}))
}

/// A split definition: the bodies, the first tool and the further ones.
fn split_def(
    bodies: &[String],
    tool: &Value,
    more: &[Value],
    extend: bool,
    links: &Map<String, Value>,
) -> Value {
    let mut def = json!({"type": "split_body", "bodies": bodies, "tool": tool, "extend": extend});
    if !more.is_empty() {
        def["more_tools"] = json!(more);
    }
    if !links.is_empty() {
        def["tool_links"] = Value::Object(links.clone());
    }
    def
}

#[cfg(test)]
mod thread_phase_tests {
    use super::*;

    fn degrees(p: Option<f64>) -> f64 {
        p.expect("a phase").to_degrees()
    }

    #[test]
    fn a_thread_starts_at_the_seam_of_an_upward_axis() {
        // Seams turned 0, 45 and 90 degrees about +Z from the world X axis.
        for angle in [0.0f64, 45.0, 90.0, -135.0] {
            let r = angle.to_radians();
            let seam = [r.cos(), r.sin(), 0.0];
            let got = degrees(thread_phase(Some([0.0, 0.0, 1.0]), Some(seam)));
            assert!((got - angle).abs() < 1e-9, "{angle}: {got}");
        }
    }

    #[test]
    fn the_phase_is_measured_from_the_reference_of_the_axis() {
        // An axis across Z points towards +Y; its reference is the world X
        // axis, the direction across both -Z: a seam along +Z is at -90.
        assert!(
            (degrees(thread_phase(Some([0.0, 1.0, 0.0]), Some([1.0, 0.0, 0.0]))) - 0.0).abs()
                < 1e-9
        );
        assert!(
            (degrees(thread_phase(Some([0.0, 1.0, 0.0]), Some([0.0, 0.0, 1.0]))) + 90.0).abs()
                < 1e-9
        );
        // An axis along X takes the Y axis as its reference (the X axis
        // is parallel to it) and the Z axis across.
        assert!(
            (degrees(thread_phase(Some([1.0, 0.0, 0.0]), Some([0.0, 1.0, 0.0]))) - 0.0).abs()
                < 1e-9
        );
        assert!(
            (degrees(thread_phase(Some([1.0, 0.0, 0.0]), Some([0.0, 0.0, 1.0]))) - 90.0).abs()
                < 1e-9
        );
    }

    #[test]
    fn a_phase_is_read_only_where_the_helix_starts_at_the_low_end() {
        // The file's axis pointing down: the helix starts at the other end.
        assert_eq!(
            thread_phase(Some([0.0, 0.0, -1.0]), Some([1.0, 0.0, 0.0])),
            None
        );
        assert_eq!(
            thread_phase(Some([0.0, -1.0, 0.0]), Some([1.0, 0.0, 0.0])),
            None
        );
        // No seam (a part of a cylinder), no axis.
        assert_eq!(thread_phase(Some([0.0, 0.0, 1.0]), None), None);
        assert_eq!(thread_phase(None, Some([1.0, 0.0, 0.0])), None);
        // A seam of any length gives the same angle.
        assert_eq!(
            thread_phase(Some([0.0, 0.0, 2.0]), Some([0.0, 5.0, 0.0])),
            thread_phase(Some([0.0, 0.0, 1.0]), Some([0.0, 1.0, 0.0]))
        );
    }
}

#[cfg(test)]
mod taper_and_offset_tests {
    use super::*;

    /// A hole's distances come first unless the item's state shows the
    /// point's hole and not theirs (mitcad#733); the point is asked only
    /// then.
    #[test]
    fn hole_offsets_yield_only_to_a_point_the_state_confirms() {
        let mut asked = false;
        assert!(offsets_first(Some(true), || {
            asked = true;
            Some(false)
        }));
        assert!(!asked);
        assert!(offsets_first(None, || Some(true)));
        assert!(!offsets_first(Some(false), || Some(true)));
        assert!(offsets_first(Some(false), || Some(false)));
        assert!(offsets_first(Some(false), || None));
    }

    /// A taper pipe thread's diameters where it starts, from the decoder's
    /// taper (mitcad#733): the drum design's `1/2 - 14 NPT` (cm), whose
    /// bore starts 18.3209 mm wide in the file's state.
    #[test]
    fn taper_pipe_threads_start_where_the_files_cone_does() {
        let info = json!({"threadType": "NPT", "threadDesignation": "1/2 - 14 NPT",
            "_f3d_taper": {"angle": 0.062_479_7, "height": 0.145_142_857,
                           "tapDrill": 1.747_52, "length": 1.357_12,
                           "smallPitchDiameter": 1.892_40}});
        let d = taper_diameters(info.as_object()).unwrap();
        let minor = d["minor"].as_f64().unwrap();
        assert!((minor - 18.3209).abs() < 2e-4, "{d}");
        let (major, pitch) = (d["major"].as_f64().unwrap(), d["pitch"].as_f64().unwrap());
        assert!(((major - pitch) - 0.8 * 25.4 / 14.0).abs() < 1e-6, "{d}");
        // The thread type maps to MitCad's NPT standard, the empty class left out.
        let (spec, note) = thread_spec(info.as_object(), None, Some(true)).unwrap();
        assert_eq!(
            spec,
            json!({"designation": "1/2 - 14 NPT", "standard": "npt"})
        );
        assert_eq!(note, None);
        let rc = json!({"threadType": "ISO Taper Internal", "threadDesignation": "Rc 1/2",
                        "threadClass": ""});
        let (spec, _) = thread_spec(rc.as_object(), None, Some(true)).unwrap();
        assert_eq!(spec["standard"], "iso_taper");
        // An ISO type that names a taper pipe thread is ISO 7-1's.
        let iso = json!({"threadType": "ISO Taper Pipe Threads", "threadDesignation": "Rc 1/2"});
        let (spec, _) = thread_spec(iso.as_object(), None, Some(true)).unwrap();
        assert_eq!(spec["standard"], "iso_taper");
        // A straight thread has none.
        assert!(taper_diameters(json!({"threadType": "NPT"}).as_object()).is_none());
    }
}

#[cfg(test)]
mod coaxial_piece_tests {
    use super::*;
    use mitcad_model::{
        Axis, BooleanOp, BooleanOutput, ExtrudeSpec, FaceInfo, FeatureUid, FilletSet, KernelError,
    };

    /// A kernel that lists cylindrical faces as given: (name, origin z,
    /// direction z, radius, length, internal).
    struct Cylinders(Vec<(&'static str, f64, f64, f64, f64, bool)>);

    impl Kernel for Cylinders {
        type Shape = ();

        fn extrude(&self, _: &ExtrudeSpec<'_>) -> Result<(), KernelError> {
            Err(KernelError::Unsupported("extrusions"))
        }

        fn boolean(
            &self,
            _: BooleanOp,
            _: &[&()],
            _: &(),
        ) -> Result<BooleanOutput<()>, KernelError> {
            Err(KernelError::Unsupported("booleans"))
        }

        fn fillet(
            &self,
            _: FeatureUid,
            _: &(),
            _: &[FilletSet<'_>],
            _: bool,
        ) -> Result<(), KernelError> {
            Err(KernelError::Unsupported("fillets"))
        }

        fn solids(&self, _: &()) -> Result<Vec<()>, KernelError> {
            Ok(vec![()])
        }

        fn count_edges(&self, _: &(), _: &mitcad_model::EdgeName) -> usize {
            0
        }

        fn faces(&self, _: &()) -> Result<Vec<FaceInfo>, KernelError> {
            Ok(self
                .0
                .iter()
                .map(|f| FaceInfo {
                    names: vec![f.0.to_owned()],
                    surface: "cylinder".to_owned(),
                    area: 1.0,
                })
                .collect())
        }

        fn face_cylinder(&self, _: &(), face: &FaceName) -> Result<Cylinder, KernelError> {
            let (_, z, dz, radius, length, internal) = *self
                .0
                .iter()
                .find(|f| f.0 == face.to_string())
                .ok_or_else(|| KernelError::failed("no such face"))?;
            Ok(Cylinder {
                axis: Axis {
                    origin: [0.0, 0.0, z],
                    direction: [0.0, 0.0, dz],
                },
                radius,
                length,
                internal,
            })
        }
    }

    fn pieces(faces: Vec<(&'static str, f64, f64, f64, f64, bool)>, seed: &str) -> Vec<String> {
        let kernel = Cylinders(faces);
        let seed: FaceName = seed.parse().unwrap();
        let cylinder = kernel.face_cylinder(&(), &seed).unwrap();
        let mut names: Vec<String> = coaxial_pieces(&kernel, &(), &seed, &cylinder)
            .into_iter()
            .map(|(n, _)| n.to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_split_bore_is_followed_end_to_end() {
        // The seed from 4 to 9, and a piece below it listed with its axis
        // the other way round (from 4 down to 0).
        let bore = vec![
            ("F8:side(c13)", 4.0, 1.0, 2.0, 5.0, true),
            ("F6:side(c22)", 4.0, -1.0, 2.0, 4.0, true),
        ];
        assert_eq!(pieces(bore, "F8:side(c13)"), ["F6:side(c22)"]);
        // Chained: a third piece below the second.
        let chain = vec![
            ("F8:side(c13)", 4.0, 1.0, 2.0, 5.0, true),
            ("F6:side(c22)", 2.0, 1.0, 2.0, 2.0, true),
            ("F5:side(c21)", 0.0, 1.0, 2.0, 2.0, true),
        ];
        assert_eq!(
            pieces(chain, "F8:side(c13)"),
            ["F5:side(c21)", "F6:side(c22)"]
        );
    }

    #[test]
    fn only_the_same_cylinder_end_to_end_continues_a_face() {
        let seed = ("F8:side(c13)", 4.0, 1.0, 2.0, 5.0, true);
        for other in [
            // Another radius, the other side of the material, a gap, an
            // overlap along the axis (the other half of the surface).
            ("F6:side(c22)", 0.0, 1.0, 2.5, 4.0, true),
            ("F6:side(c22)", 0.0, 1.0, 2.0, 4.0, false),
            ("F6:side(c22)", 0.0, 1.0, 2.0, 3.0, true),
            ("F6:side(c22)", 4.0, 1.0, 2.0, 5.0, true),
        ] {
            assert!(pieces(vec![seed, other], "F8:side(c13)").is_empty());
        }
    }
}

#[cfg(test)]
mod end_chamfer_tests {
    use super::*;
    use mitcad_model::datum::{SurfaceGeometry, SurfacePoint};
    use mitcad_model::{
        Axis, BooleanOp, BooleanOutput, ExtrudeSpec, FaceInfo, FeatureUid, FilletSet, KernelError,
    };

    /// A cone about the Z axis: its name, its radius at `z0` and at `z1`
    /// (the face's extent along Z).
    type Cone = (&'static str, f64, f64, f64, f64);

    /// A kernel that lists the cones as given.
    struct Cones(Vec<Cone>);

    impl Cones {
        fn cone(&self, face: &FaceName) -> Result<Cone, KernelError> {
            self.0
                .iter()
                .find(|c| c.0 == face.to_string())
                .copied()
                .ok_or_else(|| KernelError::failed("no such face"))
        }
    }

    impl Kernel for Cones {
        type Shape = ();

        fn extrude(&self, _: &ExtrudeSpec<'_>) -> Result<(), KernelError> {
            Err(KernelError::Unsupported("extrusions"))
        }

        fn boolean(
            &self,
            _: BooleanOp,
            _: &[&()],
            _: &(),
        ) -> Result<BooleanOutput<()>, KernelError> {
            Err(KernelError::Unsupported("booleans"))
        }

        fn fillet(
            &self,
            _: FeatureUid,
            _: &(),
            _: &[FilletSet<'_>],
            _: bool,
        ) -> Result<(), KernelError> {
            Err(KernelError::Unsupported("fillets"))
        }

        fn solids(&self, _: &()) -> Result<Vec<()>, KernelError> {
            Ok(vec![()])
        }

        fn count_edges(&self, _: &(), _: &mitcad_model::EdgeName) -> usize {
            0
        }

        fn faces(&self, _: &()) -> Result<Vec<FaceInfo>, KernelError> {
            Ok(self
                .0
                .iter()
                .map(|c| FaceInfo {
                    names: vec![c.0.to_owned()],
                    surface: "cone".to_owned(),
                    area: 1.0,
                })
                .collect())
        }

        fn face_geometry(&self, _: &(), face: &FaceName) -> Result<SurfaceGeometry, KernelError> {
            let (_, z0, r0, z1, r1) = self.cone(face)?;
            Ok(SurfaceGeometry::Cone {
                origin: [0.0, 0.0, z0],
                axis: [0.0, 0.0, 1.0],
                radius: r0,
                half_angle: ((r1 - r0) / (z1 - z0)).atan(),
            })
        }

        /// The face's point at the height of `near` (within its extent),
        /// towards it.
        fn face_point_normal(
            &self,
            _: &(),
            face: &FaceName,
            near: Vec3,
        ) -> Result<SurfacePoint, KernelError> {
            let (_, z0, r0, z1, r1) = self.cone(face)?;
            let z = near[2].clamp(z0.min(z1), z0.max(z1));
            let r = r0 + (r1 - r0) * (z - z0) / (z1 - z0);
            let across = near[0].hypot(near[1]);
            let point = [near[0] / across * r, near[1] / across * r, z];
            Ok(SurfacePoint {
                point,
                normal: [near[0] / across, near[1] / across, 0.0],
            })
        }
    }

    /// A face on a cylinder of radius 5 about the Z axis from 0 to 20.
    fn shaft(internal: bool) -> Cylinder {
        Cylinder {
            axis: Axis {
                origin: [0.0, 0.0, 0.0],
                direction: [0.0, 0.0, 1.0],
            },
            radius: 5.0,
            length: 20.0,
            internal,
        }
    }

    fn found(cones: Vec<Cone>, internal: bool) -> Vec<(String, f64)> {
        end_chamfers(&Cones(cones), &(), &shaft(internal))
            .into_iter()
            .map(|(n, cos)| (n.to_string(), cos))
            .collect()
    }

    #[test]
    fn a_shafts_end_chamfers_move_with_it() {
        // 45-degree chamfers at the free end and at the other end, and a
        // 30-degree one (to the axis).
        let tip = ("F4:chamfer(c1)", 20.0, 5.0, 21.5, 3.5);
        let low = ("F5:chamfer(c2)", -1.0, 4.0, 0.0, 5.0);
        let steep = ("F6:chamfer(c3)", 20.0, 5.0, 20.0 + 3f64.sqrt(), 4.0);
        for (cone, cos) in [
            (tip, 0.5f64.sqrt()),
            (low, 0.5f64.sqrt()),
            (steep, 3f64.sqrt() / 2.0),
        ] {
            let got = found(vec![cone], false);
            assert_eq!(got.len(), 1, "{}", cone.0);
            assert_eq!(got[0].0, cone.0);
            assert!((got[0].1 - cos).abs() < 1e-9, "{}: {}", cone.0, got[0].1);
        }
    }

    #[test]
    fn only_chamfers_at_the_faces_ends_move() {
        for cone in [
            // Widening away from the face (towards a shoulder).
            ("F4:chamfer(c1)", 20.0, 5.0, 21.0, 6.0),
            // Narrowing, but off the end circle (another radius there).
            ("F4:chamfer(c1)", 20.0, 4.9, 21.0, 3.9),
            // On the cone through the end circle, but its face lies
            // further on.
            ("F4:chamfer(c1)", 20.5, 4.5, 21.5, 3.5),
        ] {
            assert!(found(vec![cone], false).is_empty(), "{cone:?}");
        }
    }

    #[test]
    fn a_bores_countersink_moves_with_it() {
        // A bore's mouth widening away from it moves; one narrowing does not.
        let sink = ("F4:chamfer(c1)", 20.0, 5.0, 21.0, 6.0);
        assert_eq!(found(vec![sink], true).len(), 1);
        let narrowing = ("F4:chamfer(c1)", 20.0, 5.0, 21.0, 4.0);
        assert!(found(vec![narrowing], true).is_empty());
    }
}
