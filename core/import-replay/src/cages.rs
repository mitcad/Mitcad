// SPDX-License-Identifier: MIT
//! The control cages of Form bodies (mitcad#1365): the file's cages
//! (`_f3d.cages`, `mitcad_f3d_design::cage`) kept with the bodies the import
//! brings in for them.
//!
//! The import has no translation of a Form item: its bodies come in as the
//! file's stored bodies, a base feature (the fallback). When a fallback
//! brings in new bodies, each cage not yet kept by a body of the design,
//! whose item is not after the items replayed so far, goes to the body it
//! encloses most closely: a surface lies in the convex hull of its
//! control points, so a Form body's box lies inside its cage's grip box;
//! of the bodies whose boxes do, the one whose box comes closest to the
//! cage's on every side takes it, each body and each cage once. A cage that
//! encloses no new body stays in the dump only.
//!
//! The report says of each kept cage whether it maps onto a Form's cage
//! (mitcad#1757, `ControlCage::form_cage`), so that it opens as an editable
//! Form (`form.create` with the primitive `kept_cage`), and if not, why in
//! words; the body stays as the file stores it either way.

use std::collections::BTreeSet;

use mitcad_dump::cage::{FormCage, VertexTag};
use mitcad_model::features::FeatureDef;
use mitcad_model::features::cage::{CageEdge, CageGrip, CageVertex, CageVertexTag, ControlCage};
use mitcad_model::{BaseInput, Kernel};

use crate::report::FormCageReport;
use crate::{Importer, geom};

/// A box: its lowest and highest corners, mm.
type Bounds = ([f64; 3], [f64; 3]);

/// How far, relative to its grip box's diagonal, a body's box may reach
/// past the box and still count as inside (the kernel's boxes of curved
/// faces are not tight).
const MARGIN: f64 = 0.02;

/// The same in mm, beside [`MARGIN`].
const MARGIN_MM: f64 = 0.1;

impl<K: Kernel> Importer<'_, K> {
    /// Gives the new bodies of a fallback's base features their cages
    /// (`BaseInput::cages`).
    pub(crate) fn attach_cages(&mut self, inputs: &mut [BaseInput<K::Shape>]) {
        let Some(cages) = self.dump.f3d.as_ref().and_then(|f| f.cages.as_deref()) else {
            return;
        };
        if cages.is_empty() || inputs.iter().all(|i| i.bodies.is_empty()) {
            return;
        }
        let kept: BTreeSet<&str> = self
            .doc
            .features()
            .filter_map(|f| match &f.def {
                FeatureDef::Base(base) => Some(base),
                _ => None,
            })
            .flat_map(|base| base.bodies.iter())
            .filter_map(|body| body.cage.as_ref()?.source.as_deref())
            .collect();
        let latest = self.report.items.iter().map(|i| i.index).max();
        let open: Vec<&FormCage> = cages
            .iter()
            .filter(|c| !kept.contains(c.name.as_str()))
            .filter(|c| match (c.item_index, latest) {
                (Some(item), Some(latest)) => item <= latest,
                _ => true,
            })
            .collect();
        if open.is_empty() {
            return;
        }
        let kernel = self.doc.kernel();
        let mut at = Vec::new();
        let mut boxes = Vec::new();
        for (i, input) in inputs.iter().enumerate() {
            for (j, body) in input.bodies.iter().enumerate() {
                if let Ok(Some(b)) = kernel.bounding_box(&body.shape) {
                    at.push((i, j));
                    boxes.push((b.min, b.max));
                }
            }
        }
        let cage_boxes: Vec<Option<Bounds>> = open
            .iter()
            .map(|c| {
                c.grip_box()
                    .map(|(lo, hi)| (lo.map(geom::mm), hi.map(geom::mm)))
            })
            .collect();
        for (c, b) in pair(&cage_boxes, &boxes) {
            let (i, j) = at[b];
            let kept = model_cage(open[c]);
            let opens = kept.form_cage();
            self.report.form_cages.push(FormCageReport {
                cage: kept.source.clone().unwrap_or_default(),
                body: inputs[i].bodies[j].name.clone(),
                form: opens.is_ok(),
                why: opens.err(),
            });
            let cages = &mut inputs[i].cages;
            if cages.len() <= j {
                cages.resize(j + 1, None);
            }
            cages[j] = Some(Box::new(kept));
        }
    }
}

/// Which body each cage goes to, as (cage, body) pairs: a body whose box
/// lies inside the cage's (within the margins), the closest fit first, each
/// body and each cage once.
pub(crate) fn pair(cages: &[Option<Bounds>], bodies: &[Bounds]) -> Vec<(usize, usize)> {
    let mut fits: Vec<(f64, usize, usize)> = Vec::new();
    for (c, cage) in cages.iter().enumerate() {
        let Some((lo, hi)) = cage else { continue };
        let diagonal = geom::distance(*lo, *hi);
        let margin = MARGIN * diagonal + MARGIN_MM;
        for (b, (body_lo, body_hi)) in bodies.iter().enumerate() {
            let inside =
                (0..3).all(|k| body_lo[k] >= lo[k] - margin && body_hi[k] <= hi[k] + margin);
            if inside {
                let gap: f64 = (0..3)
                    .map(|k| (body_lo[k] - lo[k]).abs() + (hi[k] - body_hi[k]).abs())
                    .sum();
                fits.push((gap, c, b));
            }
        }
    }
    fits.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    let mut cages_taken = BTreeSet::new();
    let mut bodies_taken = BTreeSet::new();
    let mut out = Vec::new();
    for (_, c, b) in fits {
        if !cages_taken.contains(&c) && !bodies_taken.contains(&b) {
            cages_taken.insert(c);
            bodies_taken.insert(b);
            out.push((c, b));
        }
    }
    out
}

/// The model's cage of a file's cage: lengths in mm, the editor records as
/// written.
pub(crate) fn model_cage(cage: &FormCage) -> ControlCage {
    let mm3 = |p: [f64; 3]| p.map(geom::mm);
    let tag = |t: VertexTag| match t {
        VertexTag::Smooth => CageVertexTag::Smooth,
        VertexTag::Dart => CageVertexTag::Dart,
        VertexTag::Crease => CageVertexTag::Crease,
        VertexTag::Corner => CageVertexTag::Corner,
    };
    ControlCage {
        source: Some(cage.name.clone()).filter(|n| !n.is_empty()),
        degree: cage.degree,
        vertices: cage
            .vertices
            .iter()
            .map(|v| CageVertex {
                position: mm3(v.position),
                weight: v.weight,
                tag: tag(v.tag),
            })
            .collect(),
        faces: cage.faces.clone(),
        edges: cage
            .edges
            .iter()
            .map(|e| CageEdge {
                vertices: e.vertices,
                knot: e.knot,
                crease: e.crease,
                boundary: e.boundary,
            })
            .collect(),
        grips: cage
            .grips
            .iter()
            .map(|g| CageGrip {
                vertex: g.vertex,
                position: mm3(g.position),
                weight: g.weight,
            })
            .collect(),
        settings: cage.settings.iter().cloned().collect(),
        tolerance: cage.tolerance.map(geom::mm),
        geometric_tolerance: cage.geometric_tolerance.map(geom::mm),
        native: cage.native.iter().map(|(_, line)| line.clone()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitcad_dump::cage::{CageEdge as FileEdge, CageGrip as FileGrip, CageVertex as FileVertex};

    /// A decoded cage of a cube of 2 cm as the file keeps it: uniform knot
    /// intervals of 1, the faces as the half-edges run round them, the
    /// edge from the vertex at (2, 0, 0) to the one at (2, 2, 0) creased.
    fn file_cube() -> FormCage {
        let corner = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 2.0],
            [2.0, 0.0, 2.0],
            [2.0, 2.0, 2.0],
            [0.0, 2.0, 2.0],
        ];
        let faces: Vec<Vec<u32>> = vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ];
        let mut edges: Vec<FileEdge> = Vec::new();
        for f in &faces {
            for k in 0..f.len() {
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                if !edges.iter().any(|e| e.vertices == [b, a]) {
                    edges.push(FileEdge {
                        vertices: [a, b],
                        crease: [a, b] == [2, 1],
                        ..FileEdge::default()
                    });
                }
            }
        }
        FormCage {
            name: "TSpline.cube.tsm".into(),
            degree: 3,
            vertices: corner
                .iter()
                .map(|&position| FileVertex {
                    position,
                    tag: VertexTag::Smooth,
                    ..FileVertex::default()
                })
                .collect(),
            faces,
            edges,
            settings: vec![("end-conditions".into(), "SUBD_CREASES".into())],
            ..FormCage::default()
        }
    }

    /// A decoded uniform cage maps onto a Form's cage in mm (mitcad#1757):
    /// its vertices and faces in order, closed, the creased edge a crease.
    #[test]
    fn a_uniform_file_cage_opens_as_a_form_cage() {
        let kept = model_cage(&file_cube());
        let form = kept.form_cage().expect("a Form cage");
        form.check().unwrap();
        assert!(form.full().closed());
        assert_eq!(form.vertices.len(), 8);
        assert_eq!(form.position(7), Some([20.0, 20.0, 20.0]));
        assert_eq!(form.faces.len(), 6);
        assert_eq!(form.faces[0].vertices, [1, 4, 3, 2]);
        assert_eq!(form.crease(2, 3), Some(f64::INFINITY));
        assert_eq!(form.creases.len(), 1);
    }

    /// A cage with a secondary control point, a weight or knot intervals
    /// of their own does not map; the words say why.
    #[test]
    fn a_file_cage_that_is_no_form_cage_says_why() {
        let mut grips = file_cube();
        grips.grips.push(FileGrip {
            vertex: 2,
            position: [2.1, 2.1, 0.0],
            weight: 1.0,
        });
        let why = model_cage(&grips).form_cage().unwrap_err();
        assert!(why.contains("beside its vertices"), "{why}");
        let mut knots = file_cube();
        knots.edges[3].knot = 0.5;
        let why = model_cage(&knots).form_cage().unwrap_err();
        assert!(why.contains("knot intervals"), "{why}");
        let mut weight = file_cube();
        weight.vertices[5].weight = 0.8;
        let why = model_cage(&weight).form_cage().unwrap_err();
        assert!(why.contains("weight"), "{why}");
    }

    #[test]
    fn each_cage_goes_to_the_body_it_encloses_most_closely() {
        // A large cage round both bodies, a small one round the small body.
        let large = Some(([0.0; 3], [100.0; 3]));
        let small = Some(([10.0; 3], [20.0; 3]));
        let big_body = ([1.0; 3], [99.0; 3]);
        let small_body = ([11.0; 3], [19.0; 3]);
        // A body past the margins is not enclosed.
        let outside = ([150.0; 3], [160.0; 3]);
        let pairs = pair(&[large, small, None], &[small_body, outside, big_body]);
        assert_eq!(pairs, [(0, 2), (1, 0)]);
        // A box slightly past the cage's (curved faces' loose boxes) is
        // inside.
        assert_eq!(pair(&[small], &[([9.9; 3], [20.1; 3])]), [(0, 0)]);
        assert!(pair(&[small], &[([5.0; 3], [20.0; 3])]).is_empty());
    }

    #[test]
    fn a_file_cage_becomes_the_models_in_mm() {
        let cage = FormCage {
            name: "TSpline.a.tsm".into(),
            degree: 3,
            vertices: vec![
                FileVertex {
                    position: [1.0, 2.0, 3.0],
                    tag: VertexTag::Corner,
                    ..FileVertex::default()
                },
                FileVertex::default(),
            ],
            edges: vec![FileEdge {
                vertices: [0, 1],
                crease: true,
                ..FileEdge::default()
            }],
            settings: vec![("end-conditions".into(), "SUBD_CREASES".into())],
            tolerance: Some(0.001),
            native: vec![(40, "105sym 0".into())],
            ..FormCage::default()
        };
        let model = model_cage(&cage);
        model.check().unwrap();
        assert_eq!(model.source.as_deref(), Some("TSpline.a.tsm"));
        assert_eq!(model.vertices[0].position, [10.0, 20.0, 30.0]);
        assert_eq!(model.vertices[0].tag, CageVertexTag::Corner);
        assert!(model.edges[0].crease);
        assert_eq!(model.settings["end-conditions"], "SUBD_CREASES");
        assert_eq!(model.tolerance, Some(0.01));
        assert_eq!(model.native, ["105sym 0"]);
    }
}
