// SPDX-License-Identifier: MIT
//! The extent in Physical Properties (mitcad#1095, commands.md "Analysis
//! queries", the `properties` row): which bodies a `properties` query
//! covers (the bodies asked for, the bodies of the components asked for
//! with their sub-components', else every body), each with the occurrences
//! that place it in the design; a body's box along the design's axes at
//! one placement and its face, edge and vertex counts; and their totals.
//! The placed masses and centres (mitcad#1110): each body's physical
//! properties computed once per density and moved to its placements
//! ([`PlacedProperties`]), shared by the `properties` query (`analysis.rs`)
//! and the `center_of_mass` query (`center_of_mass.rs`), so that both
//! count the same bodies at the same places.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::analysis::PhysicalProperties;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::internal::features::is_mesh;
use mitcad_model::kernel::{BoundingBox, Kernel, KernelError, TopologyCounts};
use mitcad_model::transform::Transform;
use mitcad_model::{BodyView, Document, InstanceView};

/// A component of a `properties` query's `components`: its uid or name, or
/// `{"component": …, "occurrence": …}` with either or both: `occurrence`
/// (a path from the root by uids, `"O1/O4"`, or by names, `"Arm:1/Pin:2"`)
/// places the component, which then counts only as placed there; without
/// it, every placement of the component counts.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum ComponentArg {
    Name(String),
    Fields(ComponentFields),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ComponentFields {
    #[serde(default)]
    component: Option<String>,
    #[serde(default)]
    occurrence: Option<String>,
}

/// A place of a body in the design: an occurrence path and the transform
/// from the body's component's coordinates into the design's.
#[derive(Debug, Clone)]
pub(super) struct Placement {
    pub transform: Transform,
    /// The occurrences from the root down to the body's component; empty
    /// for the root's own bodies.
    pub path: Vec<OccurrenceUid>,
    /// The occurrences' names joined by `/` (`Arm:1/Pin:2`); empty for the
    /// root's bodies.
    pub path_name: String,
}

impl Placement {
    /// A body placed nowhere, in its own coordinates.
    pub const OWN: Placement = Placement {
        transform: Transform::IDENTITY,
        path: Vec::new(),
        path_name: String::new(),
    };

    fn of(instance: &InstanceView) -> Self {
        Self {
            transform: instance.transform,
            path: instance.path.clone(),
            path_name: instance.path_name.clone(),
        }
    }

    /// The occurrence path for [`Document::body_material`]: none for the
    /// root's bodies.
    pub fn material_path(&self) -> Option<&[OccurrenceUid]> {
        (!self.path.is_empty()).then_some(self.path.as_slice())
    }
}

/// The bodies' placements a query takes, each occurrence path of a body
/// once (a body asked for and in a component asked for counts once there).
#[derive(Default)]
struct Placements {
    found: BTreeMap<BodyUid, Vec<Placement>>,
    seen: BTreeSet<(BodyUid, Vec<OccurrenceUid>)>,
}

impl Placements {
    fn add(&mut self, instance: &InstanceView) {
        if self.seen.insert((instance.body, instance.path.clone())) {
            self.found
                .entry(instance.body)
                .or_default()
                .push(Placement::of(instance));
        }
    }

    /// A body's placements, taken out (none: placed nowhere here).
    fn take(&mut self, body: BodyUid) -> Option<Vec<Placement>> {
        self.found.remove(&body)
    }
}

/// What a `properties` query covers.
pub(super) struct PropertiesScope<'a, S> {
    /// Each body once, with the placements the query takes, in the
    /// assembly's order (empty: the body is placed nowhere, its own
    /// coordinates are taken).
    pub bodies: Vec<(BodyView<'a, S>, Vec<Placement>)>,
    /// The components asked for: `uid`, `name` and, when given, the
    /// `occurrence` (its path name).
    pub components: Vec<Value>,
}

/// A body's box and counts; what the kernel cannot give is left out.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Extent {
    pub bbox: Option<BoundingBox>,
    pub counts: Option<TopologyCounts>,
}

impl Extent {
    /// Adds `bbox` and `faces`, `edges`, `vertices` to a body's (or the
    /// total's) JSON.
    pub fn write(&self, value: &mut Value) {
        if let Some(bbox) = &self.bbox {
            value["bbox"] = bbox_json(bbox);
        }
        if let Some(counts) = &self.counts {
            value["faces"] = json!(counts.faces);
            value["edges"] = json!(counts.edges);
            value["vertices"] = json!(counts.vertices);
        }
    }

    /// Takes in a body's: the box around both, the counts summed (of the
    /// bodies that have them).
    pub fn add(&mut self, other: &Extent) {
        if let Some(b) = &other.bbox {
            self.bbox = Some(self.bbox.map_or(*b, |a| a.union(b)));
        }
        if let Some(c) = &other.counts {
            let sum = self.counts.unwrap_or_default();
            self.counts = Some(TopologyCounts {
                faces: sum.faces + c.faces,
                edges: sum.edges + c.edges,
                vertices: sum.vertices + c.vertices,
            });
        }
    }
}

fn bbox_json(b: &BoundingBox) -> Value {
    // + 0.0 turns -0.0 into 0.0.
    let v = |p: [f64; 3]| p.map(|x| x + 0.0);
    let size: [f64; 3] = std::array::from_fn(|i| b.max[i] - b.min[i]);
    json!({"min": v(b.min), "max": v(b.max), "size": v(size)})
}

/// Properties placed by `t`: the centre of mass and the principal axes
/// moved, the inertia tensor turned into the design's axes (L I Lᵀ). Mass,
/// volume, area and the principal moments stay: occurrences place their
/// components rigidly.
fn placed(p: &PhysicalProperties, t: &Transform) -> PhysicalProperties {
    let l = &t.linear;
    let mut moved = *p;
    moved.center_of_mass = t.apply_point(p.center_of_mass);
    moved.inertia = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            (0..3)
                .map(|a| {
                    (0..3)
                        .map(|b| l[i][a] * p.inertia[a][b] * l[j][b])
                        .sum::<f64>()
                })
                .sum()
        })
    });
    moved.principal_axes = p.principal_axes.map(|axis| t.apply_vector(axis));
    moved
}

/// The bodies' physical properties at their placements (mitcad#1110):
/// each body's computed once per density, in its component's coordinates,
/// and then moved to each placement.
pub(super) struct PlacedProperties<'d, K: Kernel> {
    doc: &'d Document<K>,
    computed: BTreeMap<(BodyUid, u64), PhysicalProperties>,
}

impl<'d, K: Kernel> PlacedProperties<'d, K> {
    pub fn new(doc: &'d Document<K>) -> Self {
        Self {
            doc,
            computed: BTreeMap::new(),
        }
    }

    /// The properties of body `uid` (`shape`, named `name` in an error)
    /// with `density` (g/cm³) at `placement`.
    pub fn at(
        &mut self,
        uid: BodyUid,
        shape: &K::Shape,
        name: &str,
        density: f64,
        placement: &Placement,
    ) -> Result<PhysicalProperties, ApiError> {
        let key = (uid, density.to_bits());
        let local = match self.computed.get(&key) {
            Some(found) => *found,
            None => {
                let found = self
                    .doc
                    .kernel()
                    .physical_properties(shape, density)
                    .map_err(|e| ApiError(format!("{name}: {e}")))?;
                self.computed.insert(key, found);
                found
            }
        };
        Ok(placed(&local, &placement.transform))
    }
}

/// The box of the shape placed by `t`: the shape's own box moved when `t`
/// only moves it, else the box of a moved copy (a rotation turns the box).
fn placed_box<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    t: &Transform,
) -> Result<Option<BoundingBox>, KernelError> {
    let turn = Transform {
        linear: t.linear,
        translation: [0.0; 3],
    };
    if turn.is_identity() {
        let shift = t.translation;
        return Ok(kernel.bounding_box(shape)?.map(|b| BoundingBox {
            min: std::array::from_fn(|i| b.min[i] + shift[i]),
            max: std::array::from_fn(|i| b.max[i] + shift[i]),
        }));
    }
    let moved = if t.is_rigid() {
        kernel.move_shape(shape, t, None)?
    } else {
        kernel.transform_shape(shape, t, None)?
    };
    kernel.bounding_box(&moved)
}

/// The document's methods of this module, as an extension trait
/// (mitcad#777).
pub(super) trait PhysicalExtentOps<K: Kernel> {
    /// The bodies of a `properties` query: those of `bodies` (in that
    /// order, each with all its placements), then those of `components`
    /// with their sub-components' (in the document's order, placed as the
    /// components are); every body with all its placements when both are
    /// empty. A body has each occurrence path once.
    fn properties_scope(
        &self,
        bodies: &[BodyUid],
        components: &[ComponentArg],
    ) -> Result<PropertiesScope<'_, K::Shape>, ApiError>;

    /// A body's box along the design's axes, of the shape placed by `t`;
    /// none from a kernel without boxes or moves.
    fn placed_extent(
        &self,
        shape: &K::Shape,
        t: &Transform,
        name: &str,
    ) -> Result<Option<BoundingBox>, ApiError>;

    /// A body's face, edge and vertex counts (none for a mesh body or from
    /// a kernel without them).
    fn body_counts(&self, shape: &K::Shape, name: &str)
    -> Result<Option<TopologyCounts>, ApiError>;
}

impl<K: Kernel> PhysicalExtentOps<K> for Document<K> {
    fn properties_scope(
        &self,
        bodies: &[BodyUid],
        components: &[ComponentArg],
    ) -> Result<PropertiesScope<'_, K::Shape>, ApiError> {
        let instances = self.instances();
        let mut placements = Placements::default();
        if bodies.is_empty() && components.is_empty() {
            for i in &instances {
                placements.add(i);
            }
            return Ok(PropertiesScope {
                bodies: self
                    .bodies()
                    .into_iter()
                    .map(|view| {
                        let placed = placements.take(view.uid).unwrap_or_default();
                        (view, placed)
                    })
                    .collect(),
                components: Vec::new(),
            });
        }
        // No bodies asked for besides the components: none (an empty list
        // means every body to `chosen_bodies`).
        let chosen = if bodies.is_empty() {
            Vec::new()
        } else {
            super::analysis::chosen_bodies(self, bodies)?
        };
        let named: BTreeSet<BodyUid> = chosen.iter().map(|view| view.uid).collect();
        for i in instances.iter().filter(|i| named.contains(&i.body)) {
            placements.add(i);
        }
        let assembly = self.assembly();
        let mut described = Vec::new();
        for arg in components {
            let (component, path) = self.component_scope(arg)?;
            let mut description = json!({"uid": component, "name": assembly.name(component)});
            if let Some(path) = path.as_ref().filter(|p| !p.is_empty()) {
                description["occurrence"] = json!(assembly.path_name(path));
            }
            described.push(description);
            let within = |i: &InstanceView| match &path {
                Some(path) => i.path.starts_with(path),
                None => {
                    component.is_root()
                        || i.component == component
                        || i.path.iter().any(|o| {
                            assembly
                                .occurrence(*o)
                                .is_some_and(|o| o.component == component)
                        })
                }
            };
            for i in instances.iter().filter(|&i| within(i)) {
                placements.add(i);
            }
        }
        let mut scoped: Vec<_> = chosen
            .into_iter()
            .map(|view| {
                let placed = placements.take(view.uid).unwrap_or_default();
                (view, placed)
            })
            .collect();
        // The components' bodies, each once, in the document's order;
        // bodies of imports in progress are no bodies of the design.
        for view in self.bodies() {
            if let Some(placed) = placements.take(view.uid) {
                scoped.push((view, placed));
            }
        }
        Ok(PropertiesScope {
            bodies: scoped,
            components: described,
        })
    }

    fn placed_extent(
        &self,
        shape: &K::Shape,
        t: &Transform,
        name: &str,
    ) -> Result<Option<BoundingBox>, ApiError> {
        match placed_box(self.kernel(), shape, t) {
            Ok(found) => Ok(found),
            // A kernel without boxes or moves: no box.
            Err(KernelError::Unsupported(_)) => Ok(None),
            Err(e) => Err(ApiError(format!("{name}: {e}"))),
        }
    }

    fn body_counts(
        &self,
        shape: &K::Shape,
        name: &str,
    ) -> Result<Option<TopologyCounts>, ApiError> {
        let kernel = self.kernel();
        // A mesh body's faces are its triangles' groups, not faces on
        // surfaces: its counts are left out.
        if is_mesh(kernel, shape) {
            return Ok(None);
        }
        match kernel.topology_counts(shape) {
            Ok(counts) => Ok(Some(counts)),
            Err(KernelError::Unsupported(_)) => Ok(None),
            Err(e) => Err(ApiError(format!("{name}: {e}"))),
        }
    }
}

/// [`PhysicalExtentOps::properties_scope`]'s component lookup.
trait ComponentScopeOps {
    /// The component of an entry and the path of the occurrence that
    /// places it, when one is given (empty for the root).
    fn component_scope(
        &self,
        arg: &ComponentArg,
    ) -> Result<(ComponentUid, Option<Vec<OccurrenceUid>>), ApiError>;

    /// A component by its uid (`C2`) or name.
    fn component_by_text(&self, text: &str) -> Result<ComponentUid, ApiError>;
}

impl<K: Kernel> ComponentScopeOps for Document<K> {
    fn component_scope(
        &self,
        arg: &ComponentArg,
    ) -> Result<(ComponentUid, Option<Vec<OccurrenceUid>>), ApiError> {
        let (component, occurrence) = match arg {
            ComponentArg::Name(name) => (Some(name.as_str()), None),
            ComponentArg::Fields(fields) => {
                (fields.component.as_deref(), fields.occurrence.as_deref())
            }
        };
        let assembly = self.assembly();
        let path = occurrence
            .map(|text| {
                assembly.find_path(text).ok_or_else(|| {
                    ApiError(format!(
                        "there is no occurrence “{}”; the components query lists them",
                        text.trim()
                    ))
                })
            })
            .transpose()?;
        let placed = path.as_ref().map(|path| {
            path.last()
                .and_then(|o| assembly.occurrence(*o))
                .map_or(ComponentUid::ROOT, |o| o.component)
        });
        let named = component
            .map(|text| self.component_by_text(text))
            .transpose()?;
        match (named, placed) {
            (Some(named), Some(placed)) if named != placed => Err(ApiError(format!(
                "occurrence {} does not place {}",
                occurrence.unwrap_or_default().trim(),
                assembly.name(named)
            ))),
            (Some(component), _) | (None, Some(component)) => Ok((component, path)),
            (None, None) => Err(ApiError(
                "an entry of components needs a component or an occurrence".to_owned(),
            )),
        }
    }

    fn component_by_text(&self, text: &str) -> Result<ComponentUid, ApiError> {
        let text = text.trim();
        let assembly = self.assembly();
        if let Ok(uid) = text.parse::<ComponentUid>()
            && assembly.exists(uid)
        {
            return Ok(uid);
        }
        std::iter::once(ComponentUid::ROOT)
            .chain(assembly.components.iter().map(|c| c.uid))
            .find(|uid| assembly.name(*uid) == text)
            .ok_or_else(|| ApiError(format!("there is no component “{text}”")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A placement turned 90 degrees about Z moves the centre and the
    /// principal axes and swaps the X and Y moments of the tensor; mass,
    /// volume and the principal moments stay.
    #[test]
    fn a_turned_placement_turns_the_inertia_tensor() {
        let p = PhysicalProperties {
            volume: 8000.0,
            area: 2400.0,
            mass: 0.0628,
            center_of_mass: [10.0, 0.0, 0.0],
            inertia: [[1.0, 0.5, 0.0], [0.5, 2.0, 0.0], [0.0, 0.0, 3.0]],
            principal_moments: [1.0, 2.0, 3.0],
            principal_axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        };
        let t = Transform {
            linear: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [100.0, 0.0, 0.0],
        };
        let moved = placed(&p, &t);
        let near = |a: f64, b: f64| (a - b).abs() <= 1e-12;
        assert!(
            moved
                .center_of_mass
                .iter()
                .zip([100.0, 10.0, 0.0])
                .all(|(a, b)| near(*a, b))
        );
        let expected = [[2.0, -0.5, 0.0], [-0.5, 1.0, 0.0], [0.0, 0.0, 3.0]];
        assert!(
            moved
                .inertia
                .iter()
                .flatten()
                .zip(expected.iter().flatten())
                .all(|(a, b)| near(*a, *b)),
            "{:?}",
            moved.inertia
        );
        assert!(near(moved.principal_axes[0][1], 1.0) && near(moved.principal_axes[1][0], -1.0));
        assert_eq!(
            (moved.mass, moved.volume, moved.principal_moments),
            (p.mass, p.volume, p.principal_moments)
        );
    }
}
