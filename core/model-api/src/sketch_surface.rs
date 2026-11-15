// SPDX-License-Identifier: MIT
//! The `sketch_surface` query (mitcad#1768, see `commands.md`, "Sketches
//! on cylindrical and conical faces"): where a sketch's points lie on its
//! face and back. A sketch on a cylindrical or conical face lies in the
//! face's developed coordinates; the view draws its curves on the face, and
//! the pointer's rays, snaps and picks go back into the sketch through this
//! query. A sketch on a plane answers the same way on its plane, so that
//! the application needs one path for both.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::Document;
use mitcad_model::developed::{DevelopedSurface, Development};
use mitcad_model::features::FeatureDef;
use mitcad_model::features::sketch::SketchOutput;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::profile::SketchFrame;
use mitcad_model::sketch::Ref;
use mitcad_vec3::{add, dot, norm, scale, sub};

/// A ray of the view: from `origin` along `direction` (any length but 0).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ray {
    origin: [f64; 3],
    direction: [f64; 3],
}

/// `{"query": "sketch_surface", "sketch": "F3", "points": [[u, v], …],
/// "rays": [{"origin": […], "direction": […]}, …], "near": [[x, y, z], …],
/// "around": [u, v], "curves": true, "segment": 0.5}`; every list
/// optional.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SurfaceQuery {
    sketch: FeatureUid,
    /// Sketch points to place on the face.
    #[serde(default)]
    points: Vec<[f64; 2]>,
    /// Rays to meet the face with.
    #[serde(default)]
    rays: Vec<Ray>,
    /// Model points to take to the sketch (the nearest point of the face).
    #[serde(default)]
    near: Vec<[f64; 3]>,
    /// Where on the face the rays and points are looked for: the sketch
    /// point whose turn about the axis they are taken on (the sketch's
    /// origin when left out), so that a pointer moving past the far side
    /// of a cylinder keeps going on in the sketch.
    #[serde(default)]
    around: Option<[f64; 2]>,
    /// Each of the sketch's curves as a polyline on the face, with its
    /// length there.
    #[serde(default)]
    curves: bool,
    /// The longest piece of those polylines, mm (default 0.5).
    #[serde(default)]
    segment: Option<f64>,
    /// Only these curves (`c3`, …) of the sketch.
    #[serde(default)]
    only: Option<Vec<Ref>>,
}

/// The sketch's plane or face as the query sees it.
enum Surface<'a> {
    Plane(SketchFrame),
    Developed(&'a Development),
}

impl Surface<'_> {
    fn point(&self, p: [f64; 2]) -> [f64; 3] {
        match self {
            Self::Plane(frame) => frame.point(p),
            Self::Developed(d) => d.point(p),
        }
    }

    /// The face's outward normal (the plane's normal).
    fn normal(&self, p: [f64; 2]) -> [f64; 3] {
        match self {
            Self::Plane(frame) => frame.normal(),
            Self::Developed(d) => d.outward(p),
        }
    }

    fn sketch_point(&self, at: [f64; 3], around: Option<[f64; 2]>) -> Option<[f64; 2]> {
        match self {
            Self::Plane(frame) => {
                let d = sub(at, frame.origin);
                Some([dot(d, frame.x_axis), dot(d, frame.y_axis)])
            }
            Self::Developed(d) => d.sketch_point_of(at, around),
        }
    }

    /// The nearest hit ahead along the ray: its distance (in the ray's
    /// direction's units) and sketch point.
    fn hit(&self, ray: &Ray, around: Option<[f64; 2]>) -> Option<(f64, [f64; 2])> {
        match self {
            Self::Plane(frame) => {
                let n = frame.normal();
                let along = dot(ray.direction, n);
                if along.abs() < 1e-12 {
                    return None;
                }
                let t = dot(sub(frame.origin, ray.origin), n) / along;
                (t >= 0.0)
                    .then(|| self.sketch_point(add(ray.origin, scale(ray.direction, t)), around))
                    .flatten()
                    .map(|p| (t, p))
            }
            Self::Developed(d) => d
                .ray_hits(ray.origin, ray.direction, around)
                .first()
                .copied(),
        }
    }
}

/// The surface's description: its kind and size, axis, carrier frame.
fn surface_json(output: &SketchOutput) -> Value {
    let frame = |f: &SketchFrame| json!({"origin": f.origin, "x_axis": f.x_axis, "y_axis": f.y_axis, "normal": f.normal()});
    match &output.developed {
        None => json!({"kind": "plane", "frame": frame(&output.frame)}),
        Some(d) => {
            let (kind, half_angle) = match d.surface {
                DevelopedSurface::Cylinder { .. } => ("cylinder", 0.0),
                DevelopedSurface::Cone { half_angle } => ("cone", half_angle),
            };
            let mut value = json!({
                "kind": kind,
                "axis": {"origin": d.origin, "direction": d.axis},
                "radius": d.radius(),
                "internal": d.internal,
                "frame": frame(&d.frame),
            });
            if kind == "cone" {
                value["half_angle"] = json!(half_angle);
                value["apex"] = json!(d.origin);
            }
            value
        }
    }
}

/// The `surface` of the `sketch` query (mitcad#1768): the face's
/// description for a sketch on a cylindrical or conical face; None for a
/// sketch on a plane.
pub(super) fn sketch_surface_summary(output: Option<&SketchOutput>) -> Option<Value> {
    output.filter(|o| o.developed.is_some()).map(surface_json)
}

/// `{"surface": {…}, "points": [{"at", "normal"}], "rays": [{"hit",
/// "point", "at", "normal", "distance"}], "near": [[u, v] | null],
/// "curves": [{"id", "polyline", "length"}]}`.
pub(super) fn sketch_surface_json<K: Kernel>(
    doc: &Document<K>,
    query: &SurfaceQuery,
) -> Result<Value, ApiError> {
    let uid = query.sketch;
    let entry = doc
        .feature(uid)
        .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
    if !matches!(entry.def, FeatureDef::Sketch(_)) {
        return Err(ApiError(format!("{} ({uid}) is not a sketch", entry.name)));
    }
    let output = doc.sketch_output(uid).ok_or_else(|| {
        ApiError(mitcad_model::notice::said(
            format!("{} has not been evaluated", entry.name),
            format!("{} ({uid}) has no evaluated output", entry.name),
        ))
    })?;
    let surface = match &output.developed {
        Some(d) => Surface::Developed(d),
        None => Surface::Plane(output.frame),
    };
    let around = query.around;
    let points: Vec<Value> = query
        .points
        .iter()
        .map(|p| json!({"at": surface.point(*p), "normal": surface.normal(*p)}))
        .collect();
    let rays: Vec<Value> = query
        .rays
        .iter()
        .map(|ray| match surface.hit(ray, around) {
            Some((t, p)) => json!({
                "hit": true,
                "point": p,
                "at": surface.point(p),
                "normal": surface.normal(p),
                "distance": t * norm(ray.direction),
            }),
            None => json!({"hit": false}),
        })
        .collect();
    let near: Vec<Value> = query
        .near
        .iter()
        .map(|at| json!(surface.sketch_point(*at, around)))
        .collect();
    let mut value = json!({
        "surface": surface_json(output),
        "points": points,
        "rays": rays,
        "near": near,
    });
    if query.curves {
        let segment = query.segment.unwrap_or(0.5).max(1e-3);
        let curves: Vec<Value> = output
            .solved
            .curves
            .iter()
            .filter(|(id, _)| {
                query
                    .only
                    .as_ref()
                    .is_none_or(|only| only.contains(&Ref::Curve(**id)))
            })
            .map(|(id, curve)| {
                let (t0, t1) = curve.domain();
                let length = match &surface {
                    Surface::Developed(d) => d.length_on_face(&|t| curve.point(t), t0, t1),
                    Surface::Plane(_) => plane_length(&|t| curve.point(t), t0, t1),
                };
                let pieces = ((length / segment).ceil() as usize).clamp(8, 4096);
                let polyline: Vec<[f64; 3]> = (0..=pieces)
                    .map(|k| surface.point(curve.point(t0 + (t1 - t0) * k as f64 / pieces as f64)))
                    .collect();
                json!({"id": Ref::Curve(*id).to_string(), "polyline": polyline, "length": length})
            })
            .collect();
        value["curves"] = json!(curves);
    }
    Ok(value)
}

/// The length of a plane curve, summed over chords until it settles.
fn plane_length(f: &dyn Fn(f64) -> [f64; 2], t0: f64, t1: f64) -> f64 {
    let mut previous = f64::NAN;
    let mut n = 64usize;
    loop {
        let mut sum = 0.0;
        let mut last = f(t0);
        for k in 1..=n {
            let p = f(t0 + (t1 - t0) * k as f64 / n as f64);
            sum += (p[0] - last[0]).hypot(p[1] - last[1]);
            last = p;
        }
        if (sum - previous).abs() <= 1e-9 * sum.max(1e-9) || n >= 1 << 16 {
            return sum;
        }
        previous = sum;
        n *= 2;
    }
}
