// SPDX-License-Identifier: MIT
//! Geometry of other components (mitcad#100). A feature works on its own
//! component's geometry, in that component's coordinates; a sketch's plane
//! and its projections may also be geometry of another component. An
//! [`OccurrenceLink`] says where both components are seen: the occurrence
//! path from the root to the component the geometry is in (`source`) and
//! the one to the sketch's own component (`target`, where the user saw
//! it). The geometry is resolved in its component's coordinates and moved
//! into the sketch's by the placements at the sketch's point of the
//! timeline:
//!
//! `T = placement(target)⁻¹ · placement(source)`
//!
//! so the sketch follows the geometry when either occurrence moves. A
//! target path that no longer ends in the sketch's component (a copy of
//! the component, an occurrence deleted) is replaced by the first path to
//! that component in a depth-first walk. Evaluation records the
//! placements and the other component's bodies it reads, so the recompute
//! cache keys on them like on the feature's own inputs.

use serde::{Deserialize, Serialize};

use crate::assembly::{Assembly, inverse};
use crate::features::joint::OccurrencePath;
use crate::ids::{ComponentUid, OccurrenceUid};
use crate::joints::path_component;
use crate::transform::Transform;

/// Where linked geometry and the feature that uses it are placed: two
/// occurrence paths from the root (empty for the root itself).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceLink {
    /// To the component the geometry is in.
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub source: OccurrencePath,
    /// To the component of the feature that uses it.
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub target: OccurrencePath,
}

impl OccurrenceLink {
    /// The component the geometry is in.
    pub(crate) fn source_component(&self, assembly: &Assembly) -> Result<ComponentUid, String> {
        path_component(assembly, ComponentUid::ROOT, &self.source.0)
            .map_err(|e| format!("the linked geometry's occurrence: {e}"))
    }

    /// The path `component` (the using feature's) is seen at: the target,
    /// else the first path to it.
    pub(crate) fn target_path(
        &self,
        assembly: &Assembly,
        component: ComponentUid,
    ) -> Result<Vec<OccurrenceUid>, String> {
        first_path(assembly, component, Some(&self.target.0))
    }

    /// The transform from the source component's coordinates into those of
    /// `component` (the using feature's), with `placement` giving each
    /// occurrence's placement at that point of the timeline.
    pub(crate) fn transform(
        &self,
        assembly: &Assembly,
        component: ComponentUid,
        placement: &mut dyn FnMut(OccurrenceUid) -> Transform,
    ) -> Result<Transform, String> {
        let target = self.target_path(assembly, component)?;
        let along = |path: &[OccurrenceUid],
                     placement: &mut dyn FnMut(OccurrenceUid) -> Transform| {
            path.iter()
                .fold(Transform::IDENTITY, |t, o| t.after(&placement(*o)))
        };
        let source = along(&self.source.0, &mut *placement);
        Ok(inverse(&along(&target, &mut *placement)).after(&source))
    }
}

/// `given` when it leads from the root to `component`, else the first path
/// to it in a depth-first walk of the occurrence tree (where the
/// application shows the component's sketches). Fails for a component
/// placed nowhere.
pub(crate) fn first_path(
    assembly: &Assembly,
    component: ComponentUid,
    given: Option<&[OccurrenceUid]>,
) -> Result<Vec<OccurrenceUid>, String> {
    if let Some(given) = given
        && path_component(assembly, ComponentUid::ROOT, given) == Ok(component)
    {
        return Ok(given.to_vec());
    }
    fn walk(
        assembly: &Assembly,
        parent: ComponentUid,
        component: ComponentUid,
        path: &mut Vec<OccurrenceUid>,
    ) -> bool {
        if parent == component {
            return true;
        }
        if path.len() >= 64 {
            return false;
        }
        for o in assembly.children(parent) {
            if path.contains(&o.uid) {
                continue;
            }
            path.push(o.uid);
            if walk(assembly, o.component, component, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    if walk(assembly, ComponentUid::ROOT, component, &mut path) {
        Ok(path)
    } else {
        Err(format!(
            "{} is not placed in the design",
            assembly.name(component)
        ))
    }
}

/// The link for geometry picked at occurrence path `source` (from the
/// root) for a feature of `component` seen at `target` (the first path to
/// it when None or not leading there). None when the geometry is in the
/// feature's own component, whose coordinates need no link.
pub(crate) fn link_for(
    assembly: &Assembly,
    component: ComponentUid,
    source: &[OccurrenceUid],
    target: Option<&[OccurrenceUid]>,
) -> Result<Option<OccurrenceLink>, String> {
    let at = path_component(assembly, ComponentUid::ROOT, source)
        .map_err(|e| format!("occurrence path {}: {e}", OccurrencePath(source.to_vec())))?;
    if at == component {
        return Ok(None);
    }
    Ok(Some(OccurrenceLink {
        source: OccurrencePath(source.to_vec()),
        target: OccurrencePath(first_path(assembly, component, target)?),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Root with A:1 (O1) and B:1 (O2) side by side, and C:1 (O3) in A.
    fn assembly() -> (Assembly, [ComponentUid; 3]) {
        let mut a = Assembly::default();
        let ca = a.create(Some("A"), None);
        let cb = a.create(Some("B"), None);
        let cc = a.create(Some("C"), None);
        a.place(
            ca,
            ComponentUid::ROOT,
            Transform::translation([10.0, 0.0, 0.0]),
        );
        a.place(
            cb,
            ComponentUid::ROOT,
            Transform::translation([0.0, 20.0, 0.0]),
        );
        a.place(cc, ca, Transform::translation([0.0, 0.0, 5.0]));
        (a, [ca, cb, cc])
    }

    #[test]
    fn a_link_moves_geometry_between_sibling_components() {
        let (assembly, [_, b, _]) = assembly();
        let link = link_for(&assembly, b, &[OccurrenceUid(1), OccurrenceUid(3)], None)
            .unwrap()
            .expect("another component");
        assert_eq!(link.target.0, vec![OccurrenceUid(2)]);
        let mut placement = |o: OccurrenceUid| {
            assembly
                .occurrence(o)
                .map_or(Transform::IDENTITY, |o| o.transform)
        };
        let t = link.transform(&assembly, b, &mut placement).unwrap();
        // C's origin is at (10, 0, 5) in the design, (10, -20, 5) in B.
        assert_eq!(t.apply_point([0.0; 3]), [10.0, -20.0, 5.0]);
    }

    #[test]
    fn the_own_component_needs_no_link_and_a_stale_target_falls_back() {
        let (assembly, [a, b, _]) = assembly();
        assert_eq!(link_for(&assembly, a, &[OccurrenceUid(1)], None), Ok(None));
        // A target that does not lead to B: B's first path instead.
        let link = link_for(&assembly, b, &[], Some(&[OccurrenceUid(1)]))
            .unwrap()
            .unwrap();
        assert_eq!(link.target.0, vec![OccurrenceUid(2)]);
        assert!(link_for(&assembly, b, &[OccurrenceUid(9)], None).is_err());
    }
}
