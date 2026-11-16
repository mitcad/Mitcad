// SPDX-License-Identifier: MIT
//! The `assembly_path` query (mitcad#1737): a body slid along a direction
//! or turned about an axis, as it would be put into place, and where it
//! first runs into other bodies, on the exact shapes. The bodies are taken
//! where they are placed in the design (an occurrence's body as that
//! occurrence places it); at each position of the path the moving body is
//! moved (`Kernel::move_shape`, the input is not changed), its clearance
//! to the others is measured (`Kernel::measure_between`) and, where it
//! touches, whether it overlaps one (`Kernel::interferences`). Which
//! positions are measured, the bisection to the first contact and the
//! minimum clearance are `mitcad_model::assembly_path`'s. At a hit, the
//! faces that meet (`Kernel::face_separations`) are named for an agent.
//! See `commands.md`, "Assembly paths".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::analysis::{PlacedBody, SelectionArg};
use super::prelude::*;
use mitcad_model::analysis::Selection;
use mitcad_model::assembly_path::{self, Motion, PathResult, PathSettings, Probe, TOUCHING};
use mitcad_model::datum::Vec3;
use mitcad_model::features::geom_ref::GeomRef;
use mitcad_model::kernel::{FaceInfo, Kernel, KernelError};
use mitcad_model::transform::Transform;
use mitcad_model::{Document, ModelError};

/// The most steps a path is measured at.
const MOST_STEPS: usize = 1000;
/// The steps when the query gives none.
const DEFAULT_STEPS: usize = 20;
/// The tolerance when the query gives none, mm.
const DEFAULT_TOLERANCE: f64 = 0.001;
/// The finest and the coarsest tolerance, mm.
const TOLERANCE_RANGE: (f64, f64) = (1e-6, 1.0);
/// The smallest overlap that counts when the query gives none, mm³: the
/// `interference` query's.
const DEFAULT_MIN_VOLUME: f64 = 1e-6;

/// The `assembly_path` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssemblyPathQuery {
    /// The body that moves, as `measure` takes a body (a uid or name, or
    /// `{"body", "occurrence"}`).
    body: SelectionArg,
    /// The bodies it moves past, the same way.
    against: Vec<SelectionArg>,
    #[serde(default)]
    slide: Option<SlideInput>,
    #[serde(default)]
    turn: Option<TurnInput>,
    #[serde(default = "default_steps")]
    steps: usize,
    #[serde(default = "default_tolerance")]
    tolerance: f64,
    #[serde(default = "default_min_volume")]
    min_volume: f64,
}

fn default_steps() -> usize {
    DEFAULT_STEPS
}

fn default_tolerance() -> f64 {
    DEFAULT_TOLERANCE
}

fn default_min_volume() -> f64 {
    DEFAULT_MIN_VOLUME
}

/// Along a direction by a distance (mm; negative: against the direction).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SlideInput {
    direction: DirectionInput,
    distance: f64,
}

/// A direction as a vector in the design's coordinates, or the direction
/// of an axis reference (`"z"`, a construction axis, a straight edge, a
/// cylindrical face).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DirectionInput {
    Vector(Vec3),
    Reference(GeomRef),
}

/// About an axis (an axis reference, or `{"origin", "direction"}` in the
/// design's coordinates) by an angle (radians, right-hand rule).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnInput {
    axis: GeomRef,
    angle: f64,
}

/// The query's settings, checked.
struct Checked {
    motion: Motion,
    steps: usize,
    tolerance: f64,
    min_volume: f64,
}

/// Answers the query.
pub(super) fn query<K: Kernel>(
    doc: &Document<K>,
    query: &AssemblyPathQuery,
) -> Result<Value, ApiError> {
    let checked = check(doc, query)?;
    let moving = doc.placed_body(&query.body, "body")?;
    if query.against.is_empty() {
        return Err(ApiError(
            "against names the bodies the body moves past; it is empty".to_owned(),
        ));
    }
    let mut others = Vec::with_capacity(query.against.len());
    for (index, arg) in query.against.iter().enumerate() {
        let other = doc.placed_body(arg, &format!("against[{index}]"))?;
        if other.uid == moving.uid && other.placed == moving.placed {
            return Err(ApiError(format!(
                "against[{index}] is the moving body itself"
            )));
        }
        others.push(other);
    }
    let kernel = doc.kernel();
    let corners: Vec<Vec3> = kernel
        .enclosing_box(&moving.shape)
        .map_err(kernel_error(&moving.name))?
        .map(|found| {
            found
                .corners()
                .map(|c| moving.placed.apply_point(c))
                .collect()
        })
        .unwrap_or_default();
    let speed = checked
        .motion
        .speed(corners)
        .ok_or_else(|| ApiError(format!("{} has no extent to turn", moving.name)))?;
    let settings = PathSettings {
        length: checked.motion.length(),
        steps: checked.steps,
        tolerance: checked.tolerance,
        speed,
    };
    settings.check().map_err(ApiError)?;
    let placed_others = others
        .iter()
        .map(|other| placed(kernel, &other.shape, &other.placed, &other.name))
        .collect::<Result<Vec<_>, _>>()?;
    let path = Path {
        doc,
        moving: &moving,
        others: &others,
        placed_others: &placed_others,
        motion: checked.motion,
        min_volume: checked.min_volume,
    };
    let found = assembly_path::search(&settings, |t| path.probe(t))?;
    path.answer(&settings, &found)
}

/// The motion and the numbers of the query, or why they cannot be used.
fn check<K: Kernel>(doc: &Document<K>, query: &AssemblyPathQuery) -> Result<Checked, ApiError> {
    let motion = match (&query.slide, &query.turn) {
        (Some(slide), None) => {
            let direction = match &slide.direction {
                DirectionInput::Vector(vector) => *vector,
                DirectionInput::Reference(reference) => {
                    doc.resolve_axis(reference).map_err(ApiError)?.direction
                }
            };
            Motion::slide(direction, slide.distance).map_err(ApiError)?
        }
        (None, Some(turn)) => {
            let axis = doc.resolve_axis(&turn.axis).map_err(ApiError)?;
            Motion::turn(axis.origin, axis.direction, turn.angle).map_err(ApiError)?
        }
        _ => {
            return Err(ApiError(
                "give the path as slide {\"direction\", \"distance\"} or as turn \
                 {\"axis\", \"angle\"}, one of them"
                    .to_owned(),
            ));
        }
    };
    if !(1..=MOST_STEPS).contains(&query.steps) {
        return Err(ApiError(format!(
            "steps must be 1 to {MOST_STEPS}, got {}",
            query.steps
        )));
    }
    let (finest, coarsest) = TOLERANCE_RANGE;
    if !(finest..=coarsest).contains(&query.tolerance) {
        return Err(ApiError(format!(
            "the tolerance must be {finest} to {coarsest} mm, got {}",
            query.tolerance
        )));
    }
    if !(query.min_volume.is_finite() && query.min_volume >= 0.0) {
        return Err(ApiError(format!(
            "min_volume must be zero or more mm³, got {}",
            query.min_volume
        )));
    }
    Ok(Checked {
        motion,
        steps: query.steps,
        tolerance: query.tolerance,
        min_volume: query.min_volume,
    })
}

fn kernel_error(what: &str) -> impl Fn(KernelError) -> ApiError + '_ {
    move |e| ApiError(format!("{what}: {e}"))
}

/// A shape moved by a transform: itself when nothing moves it.
fn placed<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    transform: &Transform,
    what: &str,
) -> Result<K::Shape, ApiError> {
    if transform.is_identity() {
        return Ok(shape.clone());
    }
    kernel
        .move_shape(shape, transform, None)
        .map_err(kernel_error(what))
}

/// The bodies of a query with the path, measured on the kernel.
struct Path<'a, K: Kernel> {
    doc: &'a Document<K>,
    moving: &'a PlacedBody<K::Shape>,
    others: &'a [PlacedBody<K::Shape>],
    /// The others in the design's coordinates.
    placed_others: &'a [K::Shape],
    motion: Motion,
    min_volume: f64,
}

impl<K: Kernel> Path<'_, K> {
    fn kernel(&self) -> &K {
        self.doc.kernel()
    }

    /// The moving body at `t` along the path, in the design's coordinates.
    fn moved(&self, t: f64) -> Result<K::Shape, ApiError> {
        let at = self.motion.at(t).after(&self.moving.placed);
        placed(self.kernel(), &self.moving.shape, &at, &self.moving.name)
    }

    /// The clearance of two shapes, 0 where one lies inside the other.
    fn clearance(&self, a: &K::Shape, b: &K::Shape, name: &str) -> Result<f64, ApiError> {
        let found = self
            .kernel()
            .measure_between(
                &Selection {
                    shape: a,
                    name: None,
                },
                &Selection {
                    shape: b,
                    name: None,
                },
            )
            .map_err(kernel_error(name))?;
        Ok(if found.inside { 0.0 } else { found.distance })
    }

    /// The volume two shapes overlap by (0 below the smallest that counts).
    fn overlap(&self, a: &K::Shape, b: &K::Shape, name: &str) -> Result<f64, ApiError> {
        let found = self
            .kernel()
            .interferences(&[a, b], self.min_volume)
            .map_err(kernel_error(name))?;
        Ok(found.iter().map(|i| i.volume).sum())
    }

    fn stop(&self) -> Result<(), ApiError> {
        if self.doc.monitor().is_some_and(|m| m.is_cancelled()) {
            Err(ModelError::Cancelled.into())
        } else {
            Ok(())
        }
    }

    /// The clearance to the nearest other body at `t`, and whether the
    /// moving body overlaps one there.
    fn probe(&self, t: f64) -> Result<Probe, ApiError> {
        self.stop()?;
        let shape = self.moved(t)?;
        let mut clearance = f64::INFINITY;
        let mut overlap = false;
        for (other, against) in self.placed_others.iter().zip(self.others) {
            let distance = self.clearance(&shape, other, &against.name)?;
            if distance < TOUCHING && !overlap {
                overlap = self.overlap(&shape, other, &against.name)? > 0.0;
            }
            clearance = clearance.min(distance);
        }
        Ok(Probe { clearance, overlap })
    }

    /// The path turns: its positions are radians, not mm.
    fn turn(&self) -> bool {
        matches!(self.motion, Motion::Turn { .. })
    }

    fn body_json(&self, body: &PlacedBody<K::Shape>) -> Value {
        let mut value = json!({"body": body.uid, "name": body.name});
        if let Some(path) = &body.occurrence {
            value["occurrence"] = json!(path);
        }
        value
    }

    fn answer(&self, settings: &PathSettings, found: &PathResult) -> Result<Value, ApiError> {
        let turn = self.turn();
        let mut value = json!({
            "body": self.body_json(self.moving),
            "against": self.others.iter().map(|o| self.body_json(o)).collect::<Vec<_>>(),
            "motion": if turn { "turn" } else { "slide" },
            "unit": if turn { "rad" } else { "mm" },
            "length": settings.length,
            "steps": settings.steps,
            "tolerance": settings.tolerance,
            "resolution": settings.resolution(),
            "path": found.steps.iter()
                .map(|s| json!({"at": s.at, "clearance": s.clearance}))
                .collect::<Vec<_>>(),
            "touches_at_end": found.touches_at_end,
            "certified": found.certified,
            "probes": found.probes,
            "hit": Value::Null,
            "min_clearance": Value::Null,
        });
        if let Some(min) = found.min_clearance {
            let mut entry = json!({"clearance": min.clearance, "at": min.at});
            if let Some(closest) = self.closest(min.at)? {
                entry["closest"] = closest;
            }
            value["min_clearance"] = entry;
        }
        if let Some(hit) = found.hit {
            let mut entry = json!({
                "step": hit.step,
                "at": hit.clear,
                "contact": hit.contact,
                "at_start": hit.at_start,
            });
            if turn {
                entry["at_deg"] = json!(hit.clear.to_degrees());
            }
            let step_at = settings.step_at(hit.step);
            let at_step = self.moved(step_at)?;
            let deeper = self.moved(hit.deeper)?;
            let mut bodies = Vec::new();
            let mut hit_bodies = Vec::new();
            for (index, (other, against)) in self.placed_others.iter().zip(self.others).enumerate()
            {
                if self.overlap(&deeper, other, &against.name)? <= 0.0 {
                    continue;
                }
                let mut body = self.body_json(against);
                body["volume_at_step"] = json!(self.overlap(&at_step, other, &against.name)?);
                bodies.push(body);
                hit_bodies.push(index);
            }
            entry["bodies"] = json!(bodies);
            if !hit.at_start {
                self.contact_faces(&mut entry, hit.clear, &hit_bodies, settings);
            }
            value["hit"] = entry;
        }
        Ok(value)
    }

    /// The other body nearest at `t`, with its clearance.
    fn closest(&self, t: f64) -> Result<Option<Value>, ApiError> {
        let shape = self.moved(t)?;
        let mut best: Option<(usize, f64)> = None;
        for (index, (other, against)) in self.placed_others.iter().zip(self.others).enumerate() {
            let distance = self.clearance(&shape, other, &against.name)?;
            if best.is_none_or(|(_, d)| distance < d) {
                best = Some((index, distance));
            }
        }
        Ok(best.map(|(index, _)| self.body_json(&self.others[index])))
    }

    /// The faces that meet at the contact (for agents: topological names),
    /// measured where the body is last clear: pairs of a face of the
    /// moving body and a face of a body hit less than twice the tolerance
    /// apart, without the pairs that were that close before the body moved
    /// (a face it rests on). What the kernel cannot measure leaves
    /// `faces_error`.
    fn contact_faces(
        &self,
        entry: &mut Value,
        clear: f64,
        hit_bodies: &[usize],
        settings: &PathSettings,
    ) {
        let within = 2.0 * settings.tolerance;
        match self.faces_at(clear, hit_bodies, within) {
            Ok((faces, point)) => {
                entry["faces"] = json!(faces);
                if let Some(point) = point {
                    entry["point"] = json!(point);
                }
            }
            Err(error) => {
                entry["faces"] = json!([]);
                entry["faces_error"] = json!(error.0);
            }
        }
    }

    fn faces_at(
        &self,
        clear: f64,
        hit_bodies: &[usize],
        within: f64,
    ) -> Result<(Vec<Value>, Option<Vec3>), ApiError> {
        let kernel = self.kernel();
        let at_clear = self.moved(clear)?;
        let at_start = if clear > 0.0 {
            Some(self.moved(0.0)?)
        } else {
            None
        };
        let moving_faces = kernel
            .faces(&self.moving.shape)
            .map_err(kernel_error(&self.moving.name))?;
        let mut found = Vec::new();
        let mut nearest: Option<(f64, Vec3)> = None;
        for &index in hit_bodies {
            let (other, against) = (&self.placed_others[index], &self.others[index]);
            let other_faces = kernel
                .faces(&against.shape)
                .map_err(kernel_error(&against.name))?;
            let before: Vec<(usize, usize)> = match &at_start {
                Some(start) => kernel
                    .face_separations(start, other, within)
                    .map_err(kernel_error(&against.name))?
                    .iter()
                    .filter(|s| s.distance <= within)
                    .map(|s| (s.face_a, s.face_b))
                    .collect(),
                None => Vec::new(),
            };
            let mut pairs = kernel
                .face_separations(&at_clear, other, within)
                .map_err(kernel_error(&against.name))?;
            pairs.retain(|s| s.distance <= within);
            // Pairs that already met at the start are the faces it rests
            // on, unless they are all there is.
            if pairs
                .iter()
                .any(|s| !before.contains(&(s.face_a, s.face_b)))
            {
                pairs.retain(|s| !before.contains(&(s.face_a, s.face_b)));
            }
            pairs.sort_by(|a, b| a.distance.total_cmp(&b.distance));
            for pair in pairs {
                if nearest.is_none_or(|(d, _)| pair.distance < d) {
                    nearest = Some((pair.distance, pair.on_a));
                }
                let mut face = json!({
                    "moving_face": face_name(&moving_faces, pair.face_a),
                    "face": face_name(&other_faces, pair.face_b),
                    "body": against.uid,
                    "distance": pair.distance,
                    "on_moving": pair.on_a,
                    "on_other": pair.on_b,
                });
                if let Some(path) = &against.occurrence {
                    face["occurrence"] = json!(path);
                }
                found.push(face);
            }
        }
        Ok((found, nearest.map(|(_, point)| point)))
    }
}

/// A face's topological name by its index in `Kernel::faces`' order (its
/// first, for a face of several names); null for an unnamed face.
fn face_name(faces: &[FaceInfo], index: usize) -> Value {
    faces
        .get(index)
        .and_then(|face| face.names.first())
        .map_or(Value::Null, |name| json!(name))
}
