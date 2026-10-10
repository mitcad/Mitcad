// SPDX-License-Identifier: MIT
//! Body and face operations of external dumps (their inputs are
//! fingerprints and feature references the stream decoder does not give):
//! combine, mirror, circular and rectangular patterns, shell, offset
//! faces, move, split body, hole, thread and replace face (`commands.md`,
//! F1, F2 and F4 import tables). Without those inputs a
//! combine is found with the history: the bodies the next state changed
//! are its target and tools; so are the faces a replace face replaced.

use mitcad_f3d::design::ir::{Detail, Fingerprint, Geometry, Reference, TimelineItem, Vec3};
use mitcad_model::{BodyUid, Cylinder, FaceName, FeatureUid, Kernel, Plane};
use serde_json::{Map, Value, json};

use crate::geom::{self, mm, mm3};
use crate::history::{EXACT, RELATIVE, Sig};
use crate::{Candidate, Importer, features, refs};

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

/// The dump's compute option as Mitcad's.
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
            let forward = (0..*q).map(|i| i as f64);
            let backward = (1..*q).map(|i| -(i as f64));
            let positions: Vec<f64> = if self.symmetric {
                forward.chain(backward).collect()
            } else {
                forward.collect()
            };
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
        let n = |q: i64| if self.symmetric { 2 * q - 1 } else { q };
        n(self.one.2) * self.two.as_ref().map_or(1, |t| n(t.2)) - 1
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
    // Mitcad's patterns have at most 10,000 elements.
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
                        Extent::TwoSides { side1, side2 } => side(side1) || side(side2),
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
                    defs: vec![def],
                    note: Some(format!(
                        "inputs not decoded: copies of {} checked against the history",
                        uid
                    )),
                    guess: true,
                    predicted: Some(added * copies),
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
        let bodies: Vec<&K::Shape> = self.doc.bodies().iter().map(|b| b.shape).collect();
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
        let bodies: Vec<(BodyUid, Sig)> = self
            .doc
            .bodies()
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
                        defs: vec![json!({"type": "mirror",
                        "objects": {"type": "bodies", "bodies": objects},
                        "plane": plane, "combine": combine, "compute": "adjust"})],
                        note: Some(format!(
                            "inputs not decoded: {} of {} checked against the history",
                            if combine { "a joined mirror" } else { "copies" },
                            objects.join(", ")
                        )),
                        guess: true,
                        predicted: Some(volume),
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
    /// the features before the item that Mitcad can mirror (they make a
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
                    defs: vec![json!({"type": "mirror",
                        "objects": {"type": "features", "features": uids},
                        "plane": plane, "compute": "adjust"})],
                    note: Some(format!(
                        "inputs not decoded: a mirror of {} checked against the history",
                        uids.join(", ")
                    )),
                    guess: true,
                    predicted: Some(sum),
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
                    links.insert(
                        tool.clone(),
                        json!({"source": only_path(c)?, "target": only_path(own)?}),
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
    fn object_variants(&self, inputs: &[Reference]) -> Result<Vec<(Value, bool)>, String> {
        let without = self.objects(inputs, false)?;
        let mut out = vec![(without.clone(), false)];
        if let Ok(with) = self.objects(inputs, true)
            && with != without
        {
            if self.oracle.enabled {
                out.push((with, true));
            } else {
                out.insert(0, (with, true));
            }
        }
        Ok(out)
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
        let dependent = |r: &&Reference| {
            matches!(r, Reference::Feature(f) if matches!(
                f.object_type.as_deref(),
                Some("FilletFeature" | "ChamferFeature")
            ))
        };
        let kept: Vec<&Reference> = inputs.iter().filter(|r| !dependent(r)).collect();
        let inputs = if kept.is_empty() || with_dressups {
            inputs.iter().collect()
        } else {
            kept
        };
        for r in inputs {
            match r {
                Reference::Feature(f) if with_dressups && dependent(&r) => {
                    let uids = f
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.dressups.get(&i))
                        .ok_or("a patterned fillet or chamfer did not come in parametric")?;
                    features.extend(uids.iter().map(ToString::to_string));
                }
                Reference::Feature(f) => {
                    let uid = f
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.features.get(&i))
                        .ok_or("a patterned feature was not imported")?;
                    features.push(uid.to_string());
                }
                Reference::Body(fp) => bodies.push(
                    self.body_ref(fp)
                        .ok_or("a body was not found in the replay")?
                        .to_string(),
                ),
                Reference::Face(fp) => {
                    let (b, f) = refs::resolve_face(self.doc, fp)
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
                let (body, edge) = refs::resolve_edge(self.doc, fp)?;
                Some(json!({"body": body.to_string(), "edge": edge.to_string()}))
            }
            Reference::Face(fp) => {
                let (body, face) = refs::resolve_face(self.doc, fp)?;
                Some(json!({"body": body.to_string(), "face": face.to_string()}))
            }
            _ => None,
        }
    }

    /// A pattern axis: an axis reference, or a line of an imported sketch.
    fn pattern_axis(&self, r: &Reference) -> Option<Value> {
        if let Reference::SketchEntity(e) = r {
            let s = self.sketches.get(&e.sketch_timeline_index.flatten()?)?;
            let curve = s.ids.get(&e.id.clone().flatten()?)?;
            return Some(json!({"sketch": s.uid.to_string(), "curve": curve}));
        }
        self.axis_ref(r)
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
                let (body, face) = refs::resolve_face(self.doc, fp)?;
                Some(json!({"body": body.to_string(), "face": face.to_string()}))
            }
            _ => None,
        }
    }

    /// A mirror or pattern whose objects, and plane or axis, quantity and
    /// angle resolve (rectangular patterns: their directions are not
    /// decoded from the streams).
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
                get("axis")
                    .and_then(reference)
                    .and_then(|r| self.axis_ref(&r))
                    .or_else(|| decoded_line(get("_f3d_axis")))
                    .is_some()
                    && self.value_of(get("quantity")).is_some()
                    && self.value_of(get("totalAngle")).is_some()
            }
            _ => false,
        }
    }

    /// An item from its decoded inputs; where the file's history checks the
    /// result, followed by what the history suggests without those inputs
    /// (mitcad#67), so that inputs decoded wrongly or not found in the
    /// replay never make the item worse than guessing them.
    pub(crate) fn translate_op(
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
        if !self.oracle.enabled || !decoded.iter().any(|k| map.contains_key(*k)) {
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
                let target = target
                    .as_ref()
                    .and_then(|r| self.body_of(r))
                    .ok_or("the target body was not found")?;
                let tools: Vec<String> = tools
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("a tool body was not found")?;
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
                Ok(vec![Candidate::new(def)])
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
                        let entity = get("axis")
                            .and_then(reference)
                            .and_then(|r| self.pattern_axis(&r));
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
                let mut out: Vec<Candidate> = variants
                    .iter()
                    .map(|(objects, dressed)| dressed_note(def(objects, combine), *dressed))
                    .collect();
                out.extend(variants.iter().map(|(objects, dressed)| Candidate {
                    guess: true,
                    ..dressed_note(def(objects, !combine), *dressed)
                }));
                Ok(out)
            }
            "CircularPatternFeature" => {
                let variants = self.object_variants(&references(get("inputEntities")))?;
                let axis = get("axis")
                    .and_then(reference)
                    .and_then(|r| self.axis_ref(&r))
                    .or_else(|| decoded_line(get("_f3d_axis")))
                    .ok_or("the pattern axis was not decoded")?;
                let quantity = self
                    .value_of(get("quantity"))
                    .ok_or("the quantity was not decoded")?;
                let angle = self
                    .value_of(get("totalAngle"))
                    .ok_or("the angle was not decoded")?;
                Ok(variants
                    .iter()
                    .map(|(objects, dressed)| {
                        let mut def = json!({"type": "circular_pattern", "objects": objects,
                            "axis": axis, "quantity": quantity, "angle": angle,
                            "symmetric": flag(item, "isSymmetric").unwrap_or(false),
                            "compute": compute(item)});
                        suppressed(&mut def, get("suppressedElementsIds"));
                        dressed_note(def, *dressed)
                    })
                    .collect())
            }
            "RectangularPatternFeature" => {
                let variants = self.object_variants(&references(get("inputEntities")))?;
                let (objects, _) = variants.last().expect("the objects without dressups");
                let direction = |entity: &str,
                                 vector: &str,
                                 quantity: &str,
                                 distance: &str,
                                 symmetric: &str| {
                    let axis = get(entity)
                        .and_then(reference)
                        .and_then(|r| self.axis_ref(&r))
                        .or_else(|| {
                            let d: [f64; 3] = serde_json::from_value(get(vector)?.clone()).ok()?;
                            Some(json!({"origin": [0.0, 0.0, 0.0], "direction": geom::unit(d)?}))
                        })?;
                    Some(
                        json!({"axis": axis, "quantity": self.value_of(get(quantity))?,
                                "distance": self.value_of(get(distance))?,
                                "symmetric": get(symmetric).and_then(Value::as_bool).unwrap_or(false)}),
                    )
                };
                let Some(one) = direction(
                    "directionOneEntity",
                    "directionOne",
                    "quantityOne",
                    "distanceOne",
                    "isSymmetricInDirectionOne",
                ) else {
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
                if let Some(two) = direction(
                    "directionTwoEntity",
                    "directionTwo",
                    "quantityTwo",
                    "distanceTwo",
                    "isSymmetricInDirectionTwo",
                ) {
                    def["direction2"] = two;
                }
                suppressed(&mut def, get("suppressedElementsIds"));
                Ok(variants
                    .iter()
                    .map(|(objects, dressed)| {
                        let mut def = def.clone();
                        def["objects"] = objects.clone();
                        dressed_note(def, *dressed)
                    })
                    .collect())
            }
            "ShellFeature" => {
                let inputs = references(get("inputEntities"));
                let mut faces = Vec::new();
                let mut body = None;
                for r in &inputs {
                    match r {
                        Reference::Face(fp) => {
                            let (b, f) = refs::resolve_face(self.doc, fp)
                                .ok_or("a shell face was not found")?;
                            body = Some(b);
                            faces.push(f.to_string());
                        }
                        Reference::Body(fp) => {
                            body = self.body_of(r).or_else(|| self.only_solid(fp));
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
                Ok(vec![Candidate::new(def)])
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
                    let (b, f) = refs::resolve_face(self.doc, fp).ok_or("a face was not found")?;
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
                    defs,
                    note: None,
                    guess: false,
                    predicted: None,
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
                Ok(vec![
                    Candidate::new(json!({"type": "move", "bodies": bodies,
                    "transform": {"type": "free", "matrix": matrix}}))
                    .with_note("as a fixed transform"),
                ])
            }
            "SplitBodyFeature" => {
                let bodies: Vec<String> = references(get("splitBodies"))
                    .iter()
                    .map(|r| self.body_of(r).map(|b| b.to_string()))
                    .collect::<Option<_>>()
                    .ok_or("a split body was not found")?;
                let tool = match get("splittingTool").and_then(reference) {
                    Some(Reference::ConstructionPlane(c)) => {
                        match c.origin.clone().flatten().as_deref() {
                            Some(o @ ("XY" | "XZ" | "YZ")) => json!(o.to_lowercase()),
                            _ => {
                                let g = c
                                    .geometry
                                    .as_ref()
                                    .ok_or("the splitting plane was not decoded")?;
                                json!({"origin": mm3(g.origin.ok_or("no origin")?),
                                   "normal": geom::unit(g.normal.ok_or("no normal")?)})
                            }
                        }
                    }
                    Some(Reference::Face(fp)) => {
                        let (b, f) = refs::resolve_face(self.doc, &fp)
                            .ok_or("the splitting face was not found")?;
                        json!({"body": b.to_string(), "face": f.to_string()})
                    }
                    Some(r @ Reference::Body(_)) => {
                        let b = self.body_of(&r).ok_or("the splitting body was not found")?;
                        json!({"body": b.to_string()})
                    }
                    _ => return Err("the splitting tool was not decoded".to_owned()),
                };
                let extend = flag(item, "isSplittingToolExtended").unwrap_or(true);
                let mut candidates = vec![Candidate::new(
                    json!({"type": "split_body", "bodies": bodies, "tool": tool, "extend": extend}),
                )];
                // The file's split leaves a selected body the tool does not
                // divide as it is, Mitcad's fails: each body alone after
                // (mitcad#96).
                if bodies.len() > 1 {
                    for b in &bodies {
                        let mut c = Candidate::new(
                            json!({"type": "split_body", "bodies": [b], "tool": tool, "extend": extend}),
                        )
                        .with_note("only the body the tool divides");
                        c.guess = true;
                        candidates.push(c);
                    }
                }
                Ok(candidates)
            }
            "SplitFaceFeature" => self.split_face(&other),
            "HoleFeature" => self.hole(&other),
            "ThreadFeature" => self.thread(&other),
            "ReplaceFaceFeature" => self.replace_face(&other),
            other => Err(format!("{other} is not supported yet")),
        }
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
        let named = get("holePositionDefinition")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|m| m.values())
            .filter_map(reference)
            .find_map(|r| match r {
                Reference::Face(fp) => refs::resolve_face(self.doc, &fp),
                _ => None,
            });
        let (body, face) = match named {
            Some(f) => f,
            None => refs::planar_faces(self.doc)
                .into_iter()
                .find(|(_, _, p)| geom::dot(geom::sub(at, p.origin), p.normal).abs() < 1e-4)
                .map(|(b, f, _)| (b, f))
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
        let def = self.hole_def(d, body, &face.to_string(), &points, kind)?;
        // A hole through all whose depth the file keeps (a lead, mitcad#96):
        // through all first, the history to check it.
        let mut defs = Vec::new();
        if through_all_lead(d) && def["extent"]["type"] == "distance" {
            let mut all = def.clone();
            all["extent"] = json!({"type": "through_all"});
            defs.push((all, true));
        }
        defs.push((def, false));
        let mut out = Vec::new();
        for (def, guess) in defs {
            out.extend(
                self.hole_variants(d, vec![def])?
                    .into_iter()
                    .map(|(defs, note)| Candidate {
                        defs,
                        note,
                        guess,
                        predicted: None,
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
            let Some(shape) = self
                .doc
                .bodies()
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
                        .map(|(body, face, points)| self.hole_def(d, *body, face, points, kind))
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
                    if self.doc.bodies().len() > own.len() && defs[0].get("participants").is_none()
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
                            self.hole_variants(d, defs)?.into_iter().enumerate()
                        {
                            let mut note = "placed where the history removes material".to_owned();
                            if let Some(n) = thread_note {
                                note = format!("{note}; {n}");
                            }
                            let c = Candidate {
                                defs,
                                note: Some(note),
                                guess: true,
                                predicted: None,
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
        let kernel = self.doc.kernel();
        let state: Vec<(crate::StoredBody<K::Shape>, Sig)> =
            self.oracle.state(kernel, next)?.to_vec();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.uid, b.shape.clone(), Sig::of(kernel, b.shape)))
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
        body: BodyUid,
        face: &str,
        points: &[[f64; 3]],
        kind: &str,
    ) -> Result<Value, String> {
        let get = |k: &str| d.get(k);
        let mut def = json!({"type": "hole",
            "placement": {"type": "face", "body": body.to_string(), "face": face, "points": points},
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
    fn hole_variants(&self, d: &Map<String, Value>, defs: Vec<Value>) -> Result<Vec<Defs>, String> {
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
            let body = def["placement"]["body"]
                .as_str()
                .and_then(|b| b.parse::<BodyUid>().ok())
                .ok_or("the hole's body")?;
            let count = def["placement"]["points"].as_array().map_or(0, Vec::len);
            match self.hole_thread(d, body, k, count) {
                Ok(Some((thread, n))) => {
                    threads.push(thread);
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
    /// cylinder points as the file has it (`threadLocation`), which Mitcad's
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
    fn thread(&mut self, d: &Map<String, Value>) -> Result<Vec<Candidate>, String> {
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
        // Each face with the end its thread starts from, in Mitcad's
        // terms, its cylinder, and the radius the file sizes it to.
        let mut threaded: Vec<(BodyUid, String, bool, Cylinder, Option<f64>)> = Vec::new();
        let mut internal = None;
        for r in &faces {
            let Reference::Face(fp) = r else {
                return Err("a threaded face of another kind".to_owned());
            };
            // By a point on it, or by the thread's ends (`.ipt` threads).
            let (body, face) = refs::resolve_face(self.doc, fp)
                .or_else(|| refs::resolve_cylinder_through(self.doc, fp))
                .ok_or("a threaded face was not found")?;
            let shape = self
                .doc
                .body_shape(body)
                .ok_or("a threaded face was not found")?;
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
            threaded.push((body, face.to_string(), low_end, cylinder, target));
        }
        let (spec, note) = thread_spec(info, Some(d), internal)?;
        // A modelled thread: the profile of the file's diameters, turned as
        // the history shows it on each face.
        let mut modelled: Option<(Option<Value>, Vec<Option<f64>>)> = None;
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
            for (_, _, low_end, c, _) in &threaded {
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
                    // In tenths of a degree, as the measurement goes.
                    if let Some(a) = angle.map(|a| (a.to_degrees() * 10.0).round() / 10.0)
                        && a != 0.0
                    {
                        def["angle"] = json!(format!("{a} deg"));
                    }
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
        let with = |n: &str| match &note {
            Some(note) => format!("{n}; {note}"),
            None => n.to_owned(),
        };
        let mut candidates = Vec::new();
        // A modelled thread sizes the threaded part of its faces itself:
        // first alone, then after the faces are sized as a whole.
        if modelled.is_some() {
            candidates.push(Candidate {
                defs: plain.clone(),
                note: note.clone(),
                guess: false,
                predicted: None,
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
            let mut offsets: Vec<Value> = threaded
                .iter()
                .filter_map(|(body, face, _, c, target)| {
                    let m = (*target)?;
                    let distance = if c.internal {
                        c.radius - m
                    } else {
                        m - c.radius
                    };
                    Some(json!({"type": "offset_face", "body": body.to_string(),
                        "faces": [face], "distance": distance}))
                })
                .collect();
            offsets.extend(plain.iter().cloned());
            candidates.push(Candidate {
                defs: offsets,
                note: Some(with("its faces sized to the thread's diameter")),
                guess: false,
                predicted: sized_change,
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
                defs,
                note: Some(with("its faces sized to the thread's diameter by a tube")),
                guess: false,
                predicted: sized_change,
                first: false,
            });
        }
        if modelled.is_none() {
            let sizes = threaded.iter().any(|t| t.4.is_some());
            candidates.push(Candidate {
                defs: plain,
                note,
                guess: false,
                predicted: sizes.then_some(0.0),
                first: false,
            });
        }
        Ok(candidates)
    }

    /// The angle of a modelled thread on a face (`Cylinder`) as the next
    /// history states show it, in Mitcad's terms (`ThreadSpec`): with the
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
        let x = if along[0].abs() > 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let reference = geom::unit(geom::sub(x, geom::scale(along, geom::dot(x, along))))?;
        let across = geom::cross(along, reference);
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
            for (body, _) in state {
                // Where the segments cross the body's faces (mitcad#68: a
                // classifier of every point took seconds on a stored
                // thread's spline faces), else the points classified.
                let crossed: Result<Vec<bool>, _> = segments
                    .iter()
                    .map(|(from, to)| {
                        let crossings = kernel.segment_crossings(&body.shape, *from, *to)?;
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
                let groove = cy.atan2(cx) / TAU * pitch;
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
    /// Mitcad's basic minor diameter). A partial thread runs from the
    /// hole's start (the low end of its wall's cylinder). None for an
    /// untapped hole.
    fn hole_thread(
        &self,
        d: &Map<String, Value>,
        body: BodyUid,
        k: usize,
        count: usize,
    ) -> Result<Option<(Value, Option<String>)>, String> {
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
            // The profile of the file's diameters; the walls do not exist
            // before the hole, so the angle is not measured (0).
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
        Ok(Some((def, note)))
    }

    /// A combine the dump does not describe (the stream decoder): the
    /// bodies the next history states changed are its target and tools;
    /// the decoded `operation` (when not empty) first (mitcad#96).
    fn combine_by_history(&mut self, operation: &str) -> Result<Vec<Candidate>, String> {
        if !self.oracle.enabled {
            return Err("its bodies were not decoded (no history to find them)".to_owned());
        }
        let kernel = self.doc.kernel();
        let current: Vec<(BodyUid, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.uid, Sig::of(kernel, b.shape)))
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
                for operation in ["join", "cut", "intersect"] {
                    let def = json!({"type": "combine", "target": target.to_string(),
                        "tools": tools.iter().map(ToString::to_string).collect::<Vec<_>>(),
                        "operation": operation, "keep_tools": keep});
                    if !candidates.iter().any(|c: &Candidate| c.defs[0] == def) {
                        let mut c = Candidate::new(def);
                        c.guess = true;
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
            match refs::resolve_face(self.doc, fp) {
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
                let shape = self
                    .doc
                    .body_shape(*b)
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
    /// chain, since Mitcad's chain also takes faces tangent to them that
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
                let (b, f) = refs::resolve_face(self.doc, fp).ok_or("a face was not found")?;
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
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.uid, b.shape.clone(), Sig::of(kernel, b.shape)))
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
                let (b, f) =
                    refs::resolve_face(self.doc, fp).ok_or("the splitting face was not found")?;
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
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .filter(|b| only.is_none_or(|o| o == b.uid))
            .map(|b| (b.uid, b.shape.clone(), Sig::of(kernel, b.shape)))
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
        let solids: Vec<BodyUid> = self
            .doc
            .bodies()
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
        let shape = self
            .doc
            .body_shape(body)
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

/// A modelled thread's diameters from the dump's ThreadInfo (centimetres):
/// the major, minor and pitch diameters in millimetres, when all are given
/// and in order.
/// The note of a pattern or mirror that repeats its fillets and chamfers.
const DRESSED: &str = "its fillets and chamfers repeated on the copies";

/// A pattern's or mirror's candidate; one whose objects have the fillets
/// and chamfers among them says so (mitcad#105).
fn dressed_note(def: Value, dressed: bool) -> Candidate {
    let candidate = Candidate::new(def);
    if dressed {
        candidate.with_note(DRESSED)
    } else {
        candidate
    }
}

fn thread_diameters(info: Option<&Map<String, Value>>) -> Option<Value> {
    let info = info?;
    let get = |key: &str| info.get(key).and_then(Value::as_f64).map(mm);
    let (major, minor, pitch) = (
        get("majorDiameter")?,
        get("minorDiameter")?,
        get("pitchDiameter")?,
    );
    (minor > 0.0 && minor < pitch && pitch < major)
        .then(|| json!({"major": major, "minor": minor, "pitch": pitch}))
}

/// Whether the point at `t` (a fraction of the way) along a segment lies
/// in the material, by where the segment crosses the faces
/// ([`Kernel::segment_crossings`]): before a crossing that enters it, out;
/// after the last, as that one left it. A segment that crosses nothing
/// counts as outside.
pub(crate) fn inside_at(crossings: &[(f64, bool)], t: f64) -> bool {
    match crossings.iter().find(|(at, _)| *at > t) {
        Some((_, entering)) => !entering,
        None => crossings.last().is_some_and(|(_, entering)| *entering),
    }
}

/// Whether an axis points "up" as a modelled thread's helix takes it
/// (`geometry/include/mitcad/geometry/thread.hpp`): +Z, across Z +Y, else +X.
fn thread_upwards(d: Vec3) -> bool {
    const ACROSS: f64 = 1e-9;
    if d[2].abs() > ACROSS {
        d[2] > 0.0
    } else if d[1].abs() > ACROSS {
        d[1] > 0.0
    } else {
        d[0] > 0.0
    }
}

/// A parameter's value in millimetres.
fn parameter_mm(v: Option<&Value>) -> Option<f64> {
    reference(v?)?.parameter()?.value.map(mm)
}

/// A thread size from the dump's ThreadInfo (and the thread feature's
/// flags), checked against Mitcad's thread table: ISO metric (also the
/// `M` sizes of other metric types), Unified, and the parallel pipe
/// threads (`G 1/4-19` → Mitcad's `G 1/4`). A class Mitcad does not list
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
                pipe.ok_or_else(|| format!("{designation} is not in Mitcad's thread table"))?;
            (ThreadStandard::Whitworth, pipe)
        }
        Some(t) if t.contains("Metric") && !t.contains("Forming") => {
            (ThreadStandard::IsoMetric, designation.to_owned())
        }
        None => (ThreadStandard::IsoMetric, designation.to_owned()),
        Some(t) => return Err(format!("thread type {t} is not supported")),
    };
    thread_table::lookup(standard, &designation)?;
    let mut spec = json!({"designation": designation});
    if standard != ThreadStandard::IsoMetric {
        spec["standard"] = json!(standard);
    }
    let mut note = None;
    if let Some(class) = info.get("threadClass").and_then(Value::as_str) {
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
