// SPDX-License-Identifier: MIT
//! Forms in the document (mitcad#1757): a Form's cage read, and edited in
//! one step.
//!
//! An edit ([`Document::edit_form`]) changes a copy of the cage
//! (`mitcad_subdiv::edit`), checks it and puts it as the Form's definition
//! as one undo step named after the edit (`Extrude in Form1`); the
//! features after it recompute. A refused edit, or one whose Form would
//! fail to evaluate (a cage whose limit surface the kernel cannot make a
//! body of), leaves the document as it was: no step, the cage unchanged.

use std::sync::Arc;

use super::{Document, ModelError, invalid};
use crate::FeatureStatus;
use crate::features::path::samples;
use crate::features::{FeatureDef, FeatureEntry, FormDef};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::topo::FaceName;
use mitcad_subdiv::Cage;
use mitcad_vec3::{Vec3, cross, distance, dot};

/// The most points one curve of a face's outline gives a Form's cage.
const MOST_POINTS_A_CURVE: usize = 8;

/// How near two curves' ends must be to join into an outline, mm.
const JOIN: f64 = 1e-4;

fn not_a_form(name: &str, uid: FeatureUid) -> ModelError {
    // The name for a person, the uid for agents (mitcad#1644).
    invalid(crate::notice::said(
        format!("{name} is not a Form"),
        format!("{name} ({uid}) is not a Form"),
    ))
}

/// At most [`MOST_POINTS_A_CURVE`] points of a curve's samples, its ends
/// kept.
fn few(points: Vec<Vec3>) -> Vec<Vec3> {
    if points.len() <= MOST_POINTS_A_CURVE {
        return points;
    }
    let step = (points.len() - 1).div_ceil(MOST_POINTS_A_CURVE);
    let last = *points.last().expect("points");
    let mut kept: Vec<Vec3> = points.into_iter().step_by(step).collect();
    if kept.last() != Some(&last) {
        kept.push(last);
    }
    kept
}

/// The curves (each as points from its start to its end) joined end to
/// end into closed loops, each without its closing point.
fn loops(mut curves: Vec<Vec<Vec3>>) -> Vec<Vec<Vec3>> {
    let mut found = Vec::new();
    while !curves.is_empty() {
        let mut outline = curves.remove(0);
        loop {
            let (start, end) = (outline[0], *outline.last().expect("points"));
            if outline.len() > 2 && distance(start, end) < JOIN {
                outline.pop();
                found.push(outline);
                break;
            }
            let next = curves.iter().position(|c| {
                distance(c[0], end) < JOIN || distance(*c.last().expect("points"), end) < JOIN
            });
            let Some(next) = next else {
                break;
            };
            let mut curve = curves.remove(next);
            if distance(curve[0], end) >= JOIN {
                curve.reverse();
            }
            outline.extend(curve.into_iter().skip(1));
        }
    }
    found
}

/// A loop's area seen along `normal`.
fn area(points: &[Vec3], normal: Vec3) -> f64 {
    let n = points.len();
    let mut sum = [0.0; 3];
    for (i, p) in points.iter().enumerate() {
        let c = cross(*p, points[(i + 1) % n]);
        sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]];
    }
    dot(sum, normal).abs() / 2.0
}

impl<K: Kernel> Document<K> {
    /// A Form's stored definition.
    pub fn form_def(&self, uid: FeatureUid) -> Result<&FormDef, ModelError> {
        let entry = self
            .state
            .entry(uid)
            .ok_or(ModelError::NoSuchFeature(uid))?;
        match &entry.def {
            FeatureDef::Form(def) => Ok(def),
            _ => Err(not_a_form(&entry.name, uid)),
        }
    }

    /// Edits a Form's cage as one undo step, `<what> in <Form's name>`
    /// (see the module); returns what the edit returns.
    pub fn edit_form<T>(
        &mut self,
        uid: FeatureUid,
        what: &str,
        edit: impl FnOnce(&mut Cage) -> Result<T, String>,
    ) -> Result<T, ModelError> {
        let position = self.state.require(uid)?;
        let entry = self.state.features[position].clone();
        self.state.editable(entry.component)?;
        let FeatureDef::Form(def) = &entry.def else {
            return Err(not_a_form(&entry.name, uid));
        };
        let mut cage = def.cage.clone();
        let value = edit(&mut cage).map_err(|e| invalid(format!("{}: {e}", entry.name)))?;
        cage.check()
            .map_err(|e| invalid(format!("{}: {e}", entry.name)))?;
        let edited = FeatureEntry {
            def: FeatureDef::Form(FormDef {
                cage,
                ..def.clone()
            }),
            ..(*entry).clone()
        };
        let label = format!("{what} in {}", entry.name);
        self.apply_checked(
            move |state| {
                state
                    .check_feature(position, &edited)
                    .map_err(ModelError::Invalid)?;
                state.features[position] = Arc::new(edited);
                Ok((label, value))
            },
            move |_, result| match result.result(uid).map(|r| &r.status) {
                Some(FeatureStatus::Failed(message)) => Err(invalid(message.clone())),
                _ => Ok(()),
            },
        )
    }

    /// The outline of a planar face of a body, as points in order round
    /// it, and the face's normal out of the material: where a Form starts
    /// from a face of the model. Each curve gives a few points (a line its
    /// ends); of several loops (a face with holes) the largest.
    pub fn face_outline_points(
        &self,
        body: BodyUid,
        face: &FaceName,
    ) -> Result<(Vec<Vec3>, Vec3), ModelError> {
        let shape = self
            .body_shape(body)
            .ok_or_else(|| invalid(format!("body {body} is not in the design")))?;
        let plane = self
            .kernel()
            .face_plane(shape, face)
            .map_err(|e| invalid(format!("a Form starts from a planar face: {e}")))?;
        let boundary = self
            .kernel()
            .face_boundary(shape, face)
            .map_err(|e| invalid(e.to_string()))?;
        let curves: Vec<Vec<Vec3>> = boundary
            .iter()
            .map(|(_, curve)| few(samples(curve)))
            .filter(|points| points.len() >= 2)
            .collect();
        let outline = loops(curves)
            .into_iter()
            .max_by(|a, b| area(a, plane.normal).total_cmp(&area(b, plane.normal)))
            .filter(|outline| outline.len() >= 3)
            .ok_or_else(|| invalid("the face has no closed outline to start a Form from"))?;
        Ok((outline, plane.normal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_join_into_loops_and_keep_few_points() {
        let square = vec![
            vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
            vec![[10.0, 10.0, 0.0], [10.0, 0.0, 0.0]],
            vec![[10.0, 10.0, 0.0], [0.0, 10.0, 0.0]],
            vec![[0.0, 10.0, 0.0], [0.0, 0.0, 0.0]],
        ];
        let found = loops(square);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].len(), 4);
        assert_eq!(area(&found[0], [0.0, 0.0, 1.0]), 100.0);
        let many: Vec<Vec3> = (0..=32).map(|k| [f64::from(k), 0.0, 0.0]).collect();
        let kept = few(many);
        assert!(kept.len() <= MOST_POINTS_A_CURVE + 1);
        assert_eq!(kept[0], [0.0; 3]);
        assert_eq!(*kept.last().unwrap(), [32.0, 0.0, 0.0]);
    }
}
