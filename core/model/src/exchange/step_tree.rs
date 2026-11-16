// SPDX-License-Identifier: MIT
//! STEP files of the design's component tree (mitcad#1915): a STEP file of
//! bodies that occurrences place keeps the design's components. Each
//! component that places a body written is a definition, written once and
//! instanced by every occurrence that places it
//! ([`Kernel::write_step_tree`]), as the import reads assemblies back
//! (`assembly_import`).
//!
//! - The tree is made from the placements the other formats write (the
//!   shown occurrences of a body's component, all of them when none is
//!   shown, within the occurrence asked for): every occurrence on their
//!   paths is an instance, placed in its parent by its own placement (as a
//!   flexed joint moves it there).
//! - Two occurrences of a component share a definition when they place the
//!   same bodies and the same instances at the same placements; otherwise
//!   (a body shown in one only, a joint flexed in one) each is a
//!   definition of its own, named after the component.
//! - The design's own bodies and its top occurrences go into the top
//!   assembly, in a coordinate system's frame when one is asked for
//!   (mitcad#1775).
//! - A file of bodies that no occurrence places is written as before
//!   ([`Kernel::write_file`]), and so is every file when the kernel has no
//!   component trees.

use std::collections::BTreeMap;

use super::{
    BodyPlacement, ExportBody, ExportDefinition, ExportInstance, ExportOptions, ExportTree,
    kernel_error,
};
use crate::document::{Document, ModelError};
use crate::ids::{ComponentUid, OccurrenceUid};
use crate::kernel::{Kernel, KernelError};
use crate::transform::Transform;

/// What one occurrence path places: the bodies written there (by index)
/// and the occurrences one level below it.
#[derive(Default)]
struct PathNode {
    bodies: Vec<usize>,
    children: Vec<OccurrenceUid>,
}

/// What makes two occurrences' contents one definition: the component, its
/// bodies, and each instance's occurrence, definition and placement
/// (exactly).
type DefinitionKey = (
    ComponentUid,
    Vec<usize>,
    Vec<(OccurrenceUid, usize, [u64; 12])>,
);

/// A placement's numbers exactly, -0 as 0.
fn transform_bits(t: &Transform) -> [u64; 12] {
    let numbers = t.linear.iter().flatten().chain(&t.translation);
    let mut bits = [0; 12];
    for (bit, number) in bits.iter_mut().zip(numbers) {
        *bit = (number + 0.0).to_bits();
    }
    bits
}

impl<K: Kernel> Document<K> {
    /// Writes `bodies` to a STEP file as the design's component tree (see
    /// the module documentation) when an occurrence places one of them.
    /// `paths` holds each body's occurrence paths (in `bodies` order),
    /// none for a body written once as the design has it; `frame` maps the
    /// design's coordinates into a coordinate system's. False when no tree
    /// was written: no occurrence places a body, or the kernel has no
    /// component trees ([`Kernel::write_file`] writes the file then).
    pub(super) fn write_step_tree(
        &self,
        path: &str,
        bodies: &[ExportBody<'_, K::Shape>],
        paths: &[Vec<Vec<OccurrenceUid>>],
        frame: Option<Transform>,
        options: &ExportOptions,
    ) -> Result<bool, ModelError> {
        if paths.iter().flatten().all(Vec::is_empty) {
            return Ok(false);
        }
        // What each occurrence path places.
        let mut nodes: BTreeMap<Vec<OccurrenceUid>, PathNode> = BTreeMap::new();
        nodes.insert(Vec::new(), PathNode::default());
        for (index, body_paths) in paths.iter().enumerate() {
            if body_paths.is_empty() {
                nodes.entry(Vec::new()).or_default().bodies.push(index);
            }
            for occurrences in body_paths {
                let node = nodes.entry(occurrences.clone()).or_default();
                if !node.bodies.contains(&index) {
                    node.bodies.push(index);
                }
                for (depth, &child) in occurrences.iter().enumerate() {
                    let parent = nodes.entry(occurrences[..depth].to_vec()).or_default();
                    if !parent.children.contains(&child) {
                        parent.children.push(child);
                    }
                }
            }
        }
        let assembly = self.assembly();
        let component_of = |occurrences: &[OccurrenceUid]| {
            occurrences
                .last()
                .and_then(|o| assembly.occurrence(*o))
                .map_or(ComponentUid::ROOT, |o| o.component)
        };
        // The design's own bodies in the frame asked for.
        let framed: Vec<BodyPlacement> = frame
            .map(|transform| BodyPlacement {
                transform,
                name: String::new(),
            })
            .into_iter()
            .collect();
        // Deepest first, so that the definitions a path's instances place
        // are made before its own; the design (the empty path) last, as
        // `definitions[0]`.
        let mut order: Vec<Vec<OccurrenceUid>> = nodes.keys().cloned().collect();
        order.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        let mut definitions = vec![ExportDefinition {
            name: String::new(),
            bodies: Vec::new(),
            instances: Vec::new(),
        }];
        let mut known: BTreeMap<DefinitionKey, usize> = BTreeMap::new();
        let mut of_path: BTreeMap<Vec<OccurrenceUid>, usize> = BTreeMap::new();
        for occurrences in order {
            let mut node = nodes.remove(&occurrences).unwrap_or_default();
            let component = component_of(&occurrences);
            // The instances in the order the component has its occurrences.
            node.children.sort_by_key(|c| {
                assembly
                    .children(component)
                    .position(|o| o.uid == *c)
                    .unwrap_or(usize::MAX)
            });
            node.bodies.sort_unstable();
            let design = occurrences.is_empty();
            let instances: Vec<(OccurrenceUid, usize, Transform)> = node
                .children
                .iter()
                .map(|&child| {
                    let mut below = occurrences.clone();
                    below.push(child);
                    let step = self.occurrence_step(&occurrences, child);
                    let step = match frame {
                        Some(f) if design => f.after(&step),
                        _ => step,
                    };
                    (child, of_path[&below], step)
                })
                .collect();
            let placements: &[BodyPlacement] = if design { &framed[..] } else { &framed[..0] };
            let made = ExportDefinition {
                name: if design {
                    String::new()
                } else {
                    assembly.name(component)
                },
                bodies: node
                    .bodies
                    .iter()
                    .map(|&i| ExportBody {
                        name: bodies[i].name,
                        color: bodies[i].color,
                        shape: bodies[i].shape,
                        placements,
                    })
                    .collect(),
                instances: instances
                    .iter()
                    .map(|&(child, definition, transform)| ExportInstance {
                        definition,
                        transform,
                        name: assembly.occurrence_name(child),
                    })
                    .collect(),
            };
            if design {
                definitions[0] = made;
                continue;
            }
            let key: DefinitionKey = (
                component,
                node.bodies.clone(),
                instances
                    .iter()
                    .map(|(child, definition, transform)| {
                        (*child, *definition, transform_bits(transform))
                    })
                    .collect(),
            );
            let index = *known.entry(key).or_insert_with(|| {
                definitions.push(made);
                definitions.len() - 1
            });
            of_path.insert(occurrences, index);
        }
        let tree = ExportTree { definitions };
        match self.kernel().write_step_tree(path, &tree, options) {
            Ok(()) => Ok(true),
            Err(KernelError::Unsupported(_)) => Ok(false),
            Err(e) => Err(kernel_error(e)),
        }
    }
}
