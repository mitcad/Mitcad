// SPDX-License-Identifier: MIT
//! Read-only bill-of-materials rows and their geometry/material comparison.
//!
//! The component/library list remains available separately: a BOM represents
//! a component part once, and lists unstructured bodies once. Explicit
//! feature-created, linked and library components retain their part identity.

use super::{Document, PartsListRow};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

/// A part's measured geometry in its component's coordinates. Placements,
/// names and topological feature ids deliberately do not enter the key.
#[derive(Debug, Clone, PartialEq)]
pub struct PartSignature {
    pub volume: f64,
    pub dimensions: [f64; 3],
    pub faces: usize,
    pub materials: Vec<String>,
}

impl PartSignature {
    /// Absolute tolerance, without rounding into bins (nearby values on
    /// opposite sides of a rounding boundary must still match).
    pub fn matches(&self, other: &Self) -> bool {
        const TOLERANCE: f64 = 1e-6;
        self.faces == other.faces
            && self.materials.iter().all(|id| other.materials.contains(id))
            && other.materials.iter().all(|id| self.materials.contains(id))
            && (self.volume - other.volume).abs() <= TOLERANCE
            && self
                .dimensions
                .iter()
                .zip(other.dimensions)
                .all(|(a, b)| (a - b).abs() <= TOLERANCE)
    }
}

/// A BOM row before or after grouping. `names` retains every source row
/// for tooltips; the first member supplies descriptive fields.
#[derive(Debug, Clone)]
pub struct BomRow {
    pub part: PartsListRow,
    pub body: Option<BodyUid>,
    pub names: Vec<String>,
    /// Effective physical material ids, independent of geometry probes.
    pub materials: Vec<String>,
    /// Measured only when grouping is requested; None when unsupported.
    pub signature: Option<PartSignature>,
}

impl<K: Kernel> Document<K> {
    /// Unknown/unsupported geometry is never considered equal to another
    /// unknown part. Measurements come from the kernel, not display meshes.
    pub fn part_signature(&self, bodies: &[BodyUid]) -> Option<PartSignature> {
        if bodies.is_empty() {
            return None;
        }
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        let mut volume = 0.0;
        let mut faces = 0;
        let mut materials = Vec::new();
        for body in bodies {
            let shape = self.body_shape(*body)?;
            let bounds = self.kernel().bounding_box(shape).ok()??;
            let mass = self.kernel().mass_properties(shape).ok()?;
            if !mass.volume.is_finite()
                || !bounds.min.iter().chain(&bounds.max).all(|x| x.is_finite())
            {
                return None;
            }
            volume += mass.volume.abs();
            faces += self.kernel().face_count(shape).ok()?;
            for axis in 0..3 {
                min[axis] = min[axis].min(bounds.min[axis]);
                max[axis] = max[axis].max(bounds.max[axis]);
            }
            materials.push(self.body_material(*body, None).0.id.clone());
        }
        if !volume.is_finite() {
            return None;
        }
        materials.sort();
        materials.dedup();
        Some(PartSignature {
            volume,
            dimensions: std::array::from_fn(|i| max[i] - min[i]),
            faces,
            materials,
        })
    }

    /// A drawing's first-level assembly row includes its descendants, in
    /// the assembly's own frame. Material paths remain in the drawing's
    /// context; a placement's world translation does not enter the key.
    pub fn component_part_signature(
        &self,
        component: ComponentUid,
        context: ComponentUid,
        path: &[OccurrenceUid],
    ) -> Option<PartSignature> {
        let mut open = vec![(
            component,
            Transform::IDENTITY,
            path.to_vec(),
            vec![component],
        )];
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        let mut volume = 0.0;
        let mut faces = 0;
        let mut materials = Vec::new();
        while let Some((component, transform, path, ancestors)) = open.pop() {
            for body in self.component_bodies(component) {
                let original = self.body_shape(body)?;
                let moved;
                let shape = if transform == Transform::IDENTITY {
                    original
                } else {
                    moved = self
                        .kernel()
                        .transform_shape(original, &transform, None)
                        .ok()?;
                    &moved
                };
                let bounds = self.kernel().bounding_box(shape).ok()??;
                let mass = self.kernel().mass_properties(shape).ok()?;
                if !mass.volume.is_finite()
                    || !bounds.min.iter().chain(&bounds.max).all(|x| x.is_finite())
                {
                    return None;
                }
                volume += mass.volume.abs();
                faces += self.kernel().face_count(shape).ok()?;
                for axis in 0..3 {
                    min[axis] = min[axis].min(bounds.min[axis]);
                    max[axis] = max[axis].max(bounds.max[axis]);
                }
                materials.push(self.body_material_under(body, context, &path).0.id.clone());
            }
            for occurrence in self.assembly().children(component) {
                if ancestors.contains(&occurrence.component) || ancestors.len() >= 64 {
                    return None;
                }
                let mut child_path = path.clone();
                child_path.push(occurrence.uid);
                let mut child_ancestors = ancestors.clone();
                child_ancestors.push(occurrence.component);
                let placement = self
                    .placement(occurrence.uid)
                    .unwrap_or(occurrence.transform);
                open.push((
                    occurrence.component,
                    transform.after(&placement),
                    child_path,
                    child_ancestors,
                ));
            }
        }
        if materials.is_empty() || !volume.is_finite() {
            return None;
        }
        materials.sort();
        materials.dedup();
        Some(PartSignature {
            volume,
            dimensions: std::array::from_fn(|i| max[i] - min[i]),
            faces,
            materials,
        })
    }

    /// Physical materials of a component and its descendants, with the
    /// same drawing-relative paths as `component_part_signature`, but no
    /// geometry query (also usable in an ungrouped report).
    pub fn component_part_materials(
        &self,
        component: ComponentUid,
        context: ComponentUid,
        path: &[OccurrenceUid],
    ) -> Option<Vec<String>> {
        let mut open = vec![(component, path.to_vec(), vec![component])];
        let mut materials = Vec::new();
        while let Some((component, path, ancestors)) = open.pop() {
            materials.extend(
                self.component_bodies(component)
                    .into_iter()
                    .map(|body| self.body_material_under(body, context, &path).0.id.clone()),
            );
            for occurrence in self.assembly().children(component) {
                if ancestors.contains(&occurrence.component) || ancestors.len() >= 64 {
                    return None;
                }
                let mut child_path = path.clone();
                child_path.push(occurrence.uid);
                let mut child_ancestors = ancestors.clone();
                child_ancestors.push(occurrence.component);
                open.push((occurrence.component, child_path, child_ancestors));
            }
        }
        materials.sort();
        materials.dedup();
        Some(materials)
    }

    /// A report only: no saved state, occurrence, undo item or body changes.
    /// An unstructured multi-body component is replaced by its bodies, not
    /// counted again. Explicit component parts keep their catalogue identity.
    pub fn bom_rows(&self, group_identical: bool) -> Vec<BomRow> {
        let mut rows = Vec::new();
        let instances = self.instances();
        let mut by_body = std::collections::BTreeMap::new();
        for instance in &instances {
            by_body
                .entry(instance.body)
                .or_insert_with(Vec::new)
                .push(instance);
        }
        let mut parts = self.parts_list();
        parts.push(PartsListRow {
            component: ComponentUid::ROOT,
            name: String::new(),
            designation: String::new(),
            quantity: 1,
            linked: false,
            library: None,
            supplier: String::new(),
            part_number: String::new(),
            supplier_url: String::new(),
            supplier_parts: Vec::new(),
        });
        for part in parts {
            let bodies = self.component_bodies(part.component);
            if bodies.is_empty() {
                if !part.component.is_root() {
                    rows.push(BomRow {
                        names: vec![part.name.clone()],
                        part,
                        body: None,
                        materials: Vec::new(),
                        signature: None,
                    });
                }
                continue;
            }
            let atomic = self
                .assembly()
                .component(part.component)
                .is_some_and(|c| c.created_by.is_some() || c.link.is_some() || c.library.is_some());
            let separate = part.component.is_root() || (bodies.len() > 1 && !atomic);
            let sets = if separate {
                bodies.iter().map(|body| vec![*body]).collect::<Vec<_>>()
            } else {
                vec![bodies]
            };
            for own_bodies in sets {
                let body = own_bodies[0];
                let mut own = part.clone();
                if separate {
                    own.name = self.body_name(body);
                    own.designation = own.name.clone();
                }
                own.quantity = 0;
                let mut by_material: Vec<(Vec<String>, BomRow)> = Vec::new();
                let measured = if group_identical {
                    self.part_signature(&own_bodies)
                } else {
                    None
                };
                for instance in by_body.get(&body).into_iter().flatten() {
                    let mut material: Vec<String> = own_bodies
                        .iter()
                        .map(|body| self.body_material(*body, Some(&instance.path)).0.id.clone())
                        .collect();
                    material.sort();
                    material.dedup();
                    let name = if separate && !part.component.is_root() {
                        format!("{} / {}", part.name, own.name)
                    } else {
                        own.name.clone()
                    };
                    // The same component can inherit different materials through
                    // different assembly paths. Those placements stay separate.
                    if let Some((_, row)) = by_material.iter_mut().find(|(id, _)| *id == material) {
                        row.part.quantity += 1;
                    } else {
                        let mut signature = measured.clone();
                        if let Some(signature) = &mut signature {
                            signature.materials = material.clone();
                        }
                        let mut one = own.clone();
                        one.quantity = 1;
                        by_material.push((
                            material.clone(),
                            BomRow {
                                part: one,
                                body: separate.then_some(body),
                                names: vec![name],
                                materials: material,
                                signature,
                            },
                        ));
                    }
                }
                // Unplaced definitions are still useful in a BOM, with quantity 0.
                if by_material.is_empty() {
                    by_material.push((
                        Vec::new(),
                        BomRow {
                            names: vec![own.name.clone()],
                            part: own,
                            body: separate.then_some(body),
                            materials: {
                                let mut materials: Vec<String> = own_bodies
                                    .iter()
                                    .map(|body| self.body_material(*body, None).0.id.clone())
                                    .collect();
                                materials.sort();
                                materials.dedup();
                                materials
                            },
                            signature: measured,
                        },
                    ));
                }
                rows.extend(by_material.into_iter().map(|(_, row)| row));
            }
        }
        if !group_identical {
            return rows;
        }
        let mut grouped: Vec<BomRow> = Vec::new();
        for row in rows {
            let found = row.signature.as_ref().and_then(|key| {
                grouped
                    .iter_mut()
                    .find(|g| g.signature.as_ref().is_some_and(|k| k.matches(key)))
            });
            if let Some(group) = found {
                group.part.quantity += row.part.quantity;
                group.names.extend(row.names);
            } else {
                grouped.push(row);
            }
        }
        grouped
    }
}
