// SPDX-License-Identifier: MIT
//! `model_render`'s overlay (mitcad#1488): what an analysis finds, worked
//! out with Print Analysis' functions (`crate::print_analysis`) on the
//! mesh Print Analysis meshes a body with (the 3MF export's default
//! tolerance, shared with its queries through the print cache), and drawn
//! as those very triangles over the bodies, so that the picture and the
//! numbers agree.
//!
//! - **overhang**: `print_analysis::overhangs`, printed with `up` upward
//!   when given, else on the body's print bed face, else with +Z up; the
//!   summary per body is `print_overhangs`' for the same direction (the
//!   placement Print Analysis prints the body from).
//! - **thickness**: `print_analysis::thickness`, its thin pieces; the
//!   summary is `print_thickness`' (and the result is the one that query
//!   keeps, so asking both computes once).
//! - **draft**: the triangles whose normals are within the minimum draft
//!   angle of square to the pull direction, either way ([`drafts`]).
//! - **failed**: every triangle of the bodies the failed features read
//!   (else the bodies of the features they name), as the failures left
//!   them: a failed feature changes nothing, so they show what it did not
//!   do.
//!
//! Each body placed more than once is analysed where each placement puts
//! it (the walls once, moved to each placement). Nothing here changes the
//! document.

use std::collections::BTreeSet;
use std::sync::Arc;

use mitcad_preview::overlay::{Overlay, OverlayKind, OverlayMesh, color_text};
use mitcad_vec3::{Vec3, cross, dot, norm, scale, sub, unit};
use serde_json::{Value, json};

use super::{display_deviation, invalid};
use crate::FeatureStatus;
use crate::document::{
    Document, InstanceView, ModelError, check_minimum_wall, check_overhang_angle,
};
use crate::exchange::{MeshTolerance, Refinement, TriangleMesh};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::print_analysis::{self, Bed};
use crate::recompute::Read;
use crate::transform::Transform;

/// How far (mm) the triangles lie off the surface, as the 3D view lifts
/// Print Analysis' (the thin walls' over the overhangs'), besides the
/// picture's own mesh deviation of the body.
const OVERHANG_LIFT: f64 = 0.05;
const THIN_LIFT: f64 = 0.08;
const DRAFT_LIFT: f64 = 0.05;
const FAILED_LIFT: f64 = 0.05;

/// Printed with +Z up.
const DOWN: Vec3 = [0.0, 0.0, -1.0];

/// A body an overlay is worked out for.
struct Analysed {
    name: String,
    version: u128,
    /// Where Print Analysis prints it from.
    placement: Transform,
    /// Its mesh in its own coordinates.
    mesh: Arc<TriangleMesh>,
    /// The picture's mesh deviation of it (mm).
    deviation: f64,
}

/// What an overlay draws and its summary.
type Drawn = (Vec<OverlayMesh>, Value);

/// The mesh tolerance of Print Analysis: the 3MF export's default.
fn print_tolerance() -> MeshTolerance {
    Refinement::default().tolerance()
}

fn corners(mesh: &TriangleMesh, triangle: usize) -> [Vec3; 3] {
    mesh.triangles[triangle].map(|i| mesh.vertices[i as usize])
}

fn area_of(p: &[Vec3; 3]) -> f64 {
    norm(cross(sub(p[1], p[0]), sub(p[2], p[0]))) / 2.0
}

fn share(part: f64, whole: f64) -> f64 {
    if whole > 0.0 { part / whole } else { 0.0 }
}

/// A piece as the print cache keeps it: 9 numbers.
fn piece_of(value: &Value) -> Option<[Vec3; 3]> {
    let n: Vec<f64> = value.as_array()?.iter().filter_map(Value::as_f64).collect();
    match n.as_slice() {
        [a0, a1, a2, b0, b1, b2, c0, c1, c2] => {
            Some([[*a0, *a1, *a2], [*b0, *b1, *b2], [*c0, *c1, *c2]])
        }
        _ => None,
    }
}

/// The inverse of an affine map; None when it has none.
fn inverse(t: &Transform) -> Option<Transform> {
    let m = t.linear;
    let det = t.determinant();
    if !det.is_finite() || det == 0.0 {
        return None;
    }
    // The inverse's columns are the cofactor rows over the determinant.
    let cofactors = [cross(m[1], m[2]), cross(m[2], m[0]), cross(m[0], m[1])];
    let linear: [[f64; 3]; 3] =
        std::array::from_fn(|r| std::array::from_fn(|c| cofactors[c][r] / det));
    let mut map = Transform {
        linear,
        translation: [0.0; 3],
    };
    map.translation = scale(map.apply_vector(t.translation), -1.0);
    Some(map)
}

/// A body's faces with too little draft.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Drafts {
    /// Of all triangles, mm².
    pub area: f64,
    /// Of those under the draft, mm².
    pub under_area: f64,
    /// Those under the draft, indices into the mesh's.
    pub triangles: Vec<usize>,
}

/// The triangles of `mesh` (counter-clockwise seen from outside) with less
/// draft than `angle` degrees for a mold pulled along `pull` (a unit
/// vector), either way: their normals within `angle` of square to the
/// pull. A face exactly at the angle has enough.
pub(crate) fn drafts(mesh: &TriangleMesh, pull: Vec3, angle: f64) -> Drafts {
    let mut result = Drafts::default();
    let limit = angle.to_radians().sin() - 1e-9;
    for t in 0..mesh.triangles.len() {
        let p = corners(mesh, t);
        let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let double = norm(n);
        if double.is_nan() || double <= 0.0 {
            continue;
        }
        let area = double / 2.0;
        result.area += area;
        if (dot(n, pull) / double).abs() < limit {
            result.under_area += area;
            result.triangles.push(t);
        }
    }
    result
}

/// The placements of a body among those drawn.
fn placements(drawn: &[&InstanceView], uid: BodyUid) -> Vec<Transform> {
    drawn
        .iter()
        .filter(|i| i.body == uid)
        .map(|i| i.transform)
        .collect()
}

impl<K: Kernel> Document<K> {
    /// The overlay over the bodies drawn (`bodies`, each once, at the
    /// placements `drawn`): its triangles, one mesh per body, and its
    /// summary for the result's `overlay`.
    pub(super) fn preview_overlay(
        &self,
        overlay: &Overlay,
        bodies: &[BodyUid],
        drawn: &[&InstanceView],
        warnings: &mut Vec<String>,
    ) -> Result<Drawn, ModelError> {
        let (meshes, mut summary) = match &overlay.kind {
            OverlayKind::Overhang { angle, up } => {
                self.overhang_overlay(overlay, *angle, *up, bodies, drawn, warnings)?
            }
            OverlayKind::Thickness { minimum } => {
                self.thickness_overlay(overlay, *minimum, bodies, drawn, warnings)?
            }
            OverlayKind::Draft { pull, angle } => {
                self.draft_overlay(overlay, *pull, *angle, bodies, drawn, warnings)?
            }
            OverlayKind::Failed => self.failed_overlay(overlay, bodies, drawn, warnings)?,
        };
        summary["type"] = json!(overlay.name());
        summary["color"] = json!(color_text(overlay.color));
        Ok((meshes, summary))
    }

    /// Stops in a cancelled model job.
    fn overlay_stop(&self) -> Result<(), ModelError> {
        if self.monitor().is_some_and(|m| m.is_cancelled()) {
            Err(ModelError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// A body's mesh as Print Analysis meshes it (kept in the print cache
    /// by the key its queries use), or why there is none.
    fn analysed(&self, uid: BodyUid, tolerance: &MeshTolerance) -> Result<Analysed, String> {
        let name = self.body_name(uid);
        let shape = self
            .body_shape(uid)
            .ok_or_else(|| format!("body {uid} does not exist at the timeline marker"))?;
        let version = self.body_version(uid).unwrap_or_default();
        let key = print_analysis::mesh_key(uid, version, tolerance);
        let mesh = match self.print_cache().mesh(key) {
            Some(mesh) => mesh,
            None => {
                let mesh = self
                    .kernel()
                    .triangle_mesh(shape, tolerance)
                    .map_err(|e| format!("{name}: {e}"))?;
                let mesh = Arc::new(mesh);
                self.print_cache().keep_mesh(key, Arc::clone(&mesh));
                mesh
            }
        };
        let diagonal = match self.kernel().bounding_box(shape) {
            Ok(Some(b)) => norm(sub(b.max, b.min)),
            _ => 0.0,
        };
        let placement = self
            .print_placements(&[uid])
            .into_iter()
            .next()
            .unwrap_or(Transform::IDENTITY);
        Ok(Analysed {
            name,
            version,
            placement,
            mesh,
            deviation: display_deviation(diagonal),
        })
    }

    /// The body, or its entry in the summary and a warning when it cannot
    /// be analysed.
    fn analysed_or_noted(
        &self,
        uid: BodyUid,
        tolerance: &MeshTolerance,
        results: &mut Vec<Value>,
        warnings: &mut Vec<String>,
    ) -> Option<Analysed> {
        match self.analysed(uid, tolerance) {
            Ok(body) => Some(body),
            Err(error) => {
                let name = self.body_name(uid);
                warnings.push(format!("the overlay leaves out {name} ({uid}): {error}"));
                results.push(json!({"body": uid, "name": name, "error": error}));
                None
            }
        }
    }

    fn overhang_overlay(
        &self,
        overlay: &Overlay,
        angle: Option<f64>,
        up: Option<Vec3>,
        bodies: &[BodyUid],
        drawn: &[&InstanceView],
        warnings: &mut Vec<String>,
    ) -> Result<Drawn, ModelError> {
        let angle = angle.unwrap_or(print_analysis::DEFAULT_ANGLE);
        check_overhang_angle(angle).map_err(ModelError::Invalid)?;
        let given = match up {
            Some(up) => Some(
                unit(scale(up, -1.0))
                    .ok_or_else(|| invalid("overlay.up must be a nonzero direction"))?,
            ),
            None => None,
        };
        let tolerance = print_tolerance();
        // Faces within the mesh's deviation of the bed lie on it, as
        // print_overhangs has it.
        let on_bed = tolerance.deviation.max(1e-6);
        let mut meshes = Vec::new();
        let mut results = Vec::new();
        let (mut area, mut overhang_area) = (0.0, 0.0);
        for &uid in bodies {
            self.overlay_stop()?;
            let Some(body) = self.analysed_or_noted(uid, &tolerance, &mut results, warnings) else {
                continue;
            };
            // Up given, else the body's print bed face, else +Z up.
            let bed_face = match given {
                Some(_) => None,
                None => match self.print_bed_face(uid).map(|face| face.plane) {
                    Some(Ok(plane)) => Some(plane),
                    Some(Err(reason)) => {
                        warnings.push(format!(
                            "{} ({uid}) is printed with +Z up: its print bed face is not used ({reason})",
                            body.name
                        ));
                        None
                    }
                    None => None,
                },
            };
            let orient = |t: &Transform| -> (Vec3, Bed) {
                match (given, &bed_face) {
                    (Some(down), _) => (down, Bed::Lowest),
                    (None, Some(plane)) => (
                        unit(t.apply_vector(plane.normal())).unwrap_or(DOWN),
                        Bed::Through(t.apply_point(plane.origin)),
                    ),
                    (None, None) => (DOWN, Bed::Lowest),
                }
            };
            // Where Print Analysis prints it from: its numbers.
            let printed = print_analysis::placed(&body.mesh, &body.placement);
            let (down, bed) = orient(&body.placement);
            let found = print_analysis::overhangs(&printed, down, angle, bed, on_bed);
            area += found.area;
            overhang_area += found.overhang_area;
            results.push(json!({
                "body": uid,
                "name": body.name,
                "down": down,
                "bed_face": bed_face.is_some(),
                "area": found.area,
                "overhang_area": found.overhang_area,
                "share": found.share(),
                "triangles": found.triangles.len(),
            }));
            // Where each placement drawn puts it.
            let mut triangles = Vec::new();
            for t in placements(drawn, uid) {
                if t == body.placement {
                    triangles.extend(found.triangles.iter().map(|&i| corners(&printed, i)));
                } else {
                    let placed = print_analysis::placed(&body.mesh, &t);
                    let (down, bed) = orient(&t);
                    let here = print_analysis::overhangs(&placed, down, angle, bed, on_bed);
                    triangles.extend(here.triangles.iter().map(|&i| corners(&placed, i)));
                }
            }
            meshes.push(OverlayMesh {
                triangles,
                color: overlay.color,
                lift: OVERHANG_LIFT + body.deviation,
            });
        }
        let summary = json!({
            "angle": angle,
            "up": up,
            "bodies": results,
            "area": area,
            "overhang_area": overhang_area,
            "share": share(overhang_area, area),
        });
        Ok((meshes, summary))
    }

    fn thickness_overlay(
        &self,
        overlay: &Overlay,
        minimum: Option<f64>,
        bodies: &[BodyUid],
        drawn: &[&InstanceView],
        warnings: &mut Vec<String>,
    ) -> Result<Drawn, ModelError> {
        let minimum = minimum.unwrap_or(print_analysis::DEFAULT_MINIMUM);
        check_minimum_wall(minimum).map_err(ModelError::Invalid)?;
        let tolerance = print_tolerance();
        let settings = json!({"minimum": minimum});
        let monitor = self.monitor().cloned();
        let cancelled = move || monitor.as_ref().is_some_and(|m| m.is_cancelled());
        let mut meshes = Vec::new();
        let mut results = Vec::new();
        let mut thinnest: Option<(f64, Value)> = None;
        let (mut regions, mut thin_triangles, mut thin_area) = (0u64, 0usize, 0.0);
        for &uid in bodies {
            self.overlay_stop()?;
            let Some(body) = self.analysed_or_noted(uid, &tolerance, &mut results, warnings) else {
                continue;
            };
            // print_thickness' result for the body, computed once for both.
            let key = print_analysis::result_key(
                print_analysis::THICKNESS_TAG,
                uid,
                body.version,
                &body.placement,
                &tolerance,
                &settings,
            );
            let value = match self.print_cache().result(key) {
                Some(value) => value,
                None => {
                    let printed = print_analysis::placed(&body.mesh, &body.placement);
                    let found = print_analysis::thickness(&printed, minimum, &cancelled)
                        .map_err(|_| ModelError::Cancelled)?;
                    let value = Arc::new(print_analysis::thickness_value(&found));
                    self.print_cache().keep_result(key, Arc::clone(&value));
                    value
                }
            };
            let pieces: Vec<[Vec3; 3]> = value["triangles"]
                .as_array()
                .map(|list| list.iter().filter_map(piece_of).collect())
                .unwrap_or_default();
            let area: f64 = pieces.iter().map(area_of).sum();
            if let Some(wall) = value["thinnest"].as_f64()
                && thinnest.as_ref().is_none_or(|(best, _)| wall < *best)
            {
                thinnest = Some((wall, value["at"].clone()));
            }
            regions += value["regions"].as_u64().unwrap_or_default();
            thin_triangles += pieces.len();
            thin_area += area;
            results.push(json!({
                "body": uid,
                "name": body.name,
                "thinnest": value["thinnest"],
                "at": value["at"],
                "to": value["to"],
                "regions": value["regions"],
                "thin_triangles": pieces.len(),
                "thin_area": area,
            }));
            // The pieces where each placement drawn puts them.
            let mut triangles = Vec::new();
            for t in placements(drawn, uid) {
                if t == body.placement {
                    triangles.extend_from_slice(&pieces);
                } else if let Some(back) = inverse(&body.placement) {
                    let map = t.after(&back);
                    triangles.extend(pieces.iter().map(|p| p.map(|c| map.apply_point(c))));
                }
            }
            meshes.push(OverlayMesh {
                triangles,
                color: overlay.color,
                lift: THIN_LIFT + body.deviation,
            });
        }
        let (wall, at) = match thinnest {
            Some((wall, at)) => (json!(wall), at),
            None => (Value::Null, Value::Null),
        };
        let summary = json!({
            "minimum": minimum,
            "bodies": results,
            "thinnest": wall,
            "at": at,
            "regions": regions,
            "thin_triangles": thin_triangles,
            "thin_area": thin_area,
        });
        Ok((meshes, summary))
    }

    fn draft_overlay(
        &self,
        overlay: &Overlay,
        pull: Vec3,
        angle: f64,
        bodies: &[BodyUid],
        drawn: &[&InstanceView],
        warnings: &mut Vec<String>,
    ) -> Result<Drawn, ModelError> {
        let pull = unit(pull).ok_or_else(|| invalid("overlay.pull must be a nonzero direction"))?;
        let tolerance = print_tolerance();
        let mut meshes = Vec::new();
        let mut results = Vec::new();
        let (mut area, mut under_area) = (0.0, 0.0);
        for &uid in bodies {
            self.overlay_stop()?;
            let Some(body) = self.analysed_or_noted(uid, &tolerance, &mut results, warnings) else {
                continue;
            };
            let printed = print_analysis::placed(&body.mesh, &body.placement);
            let found = drafts(&printed, pull, angle);
            area += found.area;
            under_area += found.under_area;
            results.push(json!({
                "body": uid,
                "name": body.name,
                "area": found.area,
                "under_area": found.under_area,
                "share": share(found.under_area, found.area),
                "triangles": found.triangles.len(),
            }));
            let mut triangles = Vec::new();
            for t in placements(drawn, uid) {
                if t == body.placement {
                    triangles.extend(found.triangles.iter().map(|&i| corners(&printed, i)));
                } else {
                    let placed = print_analysis::placed(&body.mesh, &t);
                    let here = drafts(&placed, pull, angle);
                    triangles.extend(here.triangles.iter().map(|&i| corners(&placed, i)));
                }
            }
            meshes.push(OverlayMesh {
                triangles,
                color: overlay.color,
                lift: DRAFT_LIFT + body.deviation,
            });
        }
        let summary = json!({
            "pull": pull,
            "angle": angle,
            "bodies": results,
            "area": area,
            "under_area": under_area,
            "share": share(under_area, area),
        });
        Ok((meshes, summary))
    }

    /// The failed features before the marker with the bodies they read
    /// that are there (else those of the features they name, and their
    /// own), in timeline order.
    fn failed_feature_reads(&self) -> Vec<(Value, BTreeSet<BodyUid>)> {
        let existing: BTreeSet<BodyUid> = self.bodies().iter().map(|b| b.uid).collect();
        let mut failed = Vec::new();
        for entry in self.features() {
            let Some(FeatureStatus::Failed(message)) = self.status(entry.uid) else {
                continue;
            };
            let mut read: BTreeSet<BodyUid> = BTreeSet::new();
            if let Some(output) = self
                .feature_result(entry.uid)
                .and_then(|r| r.output.as_ref())
            {
                for item in &output.reads {
                    if let Read::Body(uid, _) | Read::BodyIn(_, uid, _) = item {
                        read.insert(*uid);
                    }
                }
            }
            if read.is_empty() {
                let named = entry.def.references().features;
                read.extend(existing.iter().filter(|b| named.contains(&b.feature)));
            }
            read.extend(existing.iter().filter(|b| b.feature == entry.uid));
            read.retain(|b| existing.contains(b));
            let listed: Vec<BodyUid> = read.iter().copied().collect();
            failed.push((
                json!({"feature": entry.uid, "name": entry.name, "error": message, "bodies": listed}),
                read,
            ));
        }
        failed
    }

    fn failed_overlay(
        &self,
        overlay: &Overlay,
        bodies: &[BodyUid],
        drawn: &[&InstanceView],
        warnings: &mut Vec<String>,
    ) -> Result<Drawn, ModelError> {
        let failed = self.failed_feature_reads();
        let marked: BTreeSet<BodyUid> = failed.iter().flat_map(|(_, read)| read).copied().collect();
        let tolerance = print_tolerance();
        let mut meshes = Vec::new();
        let mut results = Vec::new();
        let mut shown = Vec::new();
        for &uid in bodies.iter().filter(|b| marked.contains(*b)) {
            self.overlay_stop()?;
            let Some(body) = self.analysed_or_noted(uid, &tolerance, &mut results, warnings) else {
                continue;
            };
            let mut triangles = Vec::new();
            for t in placements(drawn, uid) {
                let placed = print_analysis::placed(&body.mesh, &t);
                triangles.extend((0..placed.triangles.len()).map(|i| corners(&placed, i)));
            }
            meshes.push(OverlayMesh {
                triangles,
                color: overlay.color,
                lift: FAILED_LIFT + body.deviation,
            });
            shown.push(uid);
        }
        let hidden: Vec<BodyUid> = marked
            .iter()
            .filter(|b| !shown.contains(*b))
            .copied()
            .collect();
        let features: Vec<Value> = failed.into_iter().map(|(value, _)| value).collect();
        let summary = json!({
            "features": features,
            "bodies": shown,
            "not_drawn": hidden,
            "errors": results,
        });
        Ok((meshes, summary))
    }
}
