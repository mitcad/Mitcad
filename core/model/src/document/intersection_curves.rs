// SPDX-License-Identifier: MIT
//! The curve of an Intersection Curve (mitcad#1405) projected into a later
//! sketch: its pieces along the sketch's normal, as body edges are, or
//! where the sketch's plane crosses it, so that its ends and points can be
//! used there. Linked, the projection follows the curve
//! ([`crate::sketch::ProjectionSource::Curve`]) whenever either of its
//! sketches changes.

use super::{Document, ModelError, Projecting, ProjectionTarget, invalid};
use crate::ids::{FeatureUid, OccurrenceUid};
use crate::kernel::{Curve3, Kernel};
use crate::sketch::ProjectionSource;
use crate::sketch::edit::EditReport;
use crate::sketch::project::{SourceCurves, plane_crossings};

impl<K: Kernel> Document<K> {
    /// Projects the curve of the Intersection Curve `source` into
    /// `sketch`: along the sketch's normal, or with `section` the points
    /// where the sketch's plane crosses it (refused when it crosses
    /// nowhere). The Intersection Curve comes before the sketch in the
    /// timeline, in the sketch's component, or in another component through
    /// the link of `occurrence` (seen at `context`, mitcad#1417), as another
    /// component's sketch geometry is projected.
    #[allow(clippy::too_many_arguments)]
    pub fn project_feature_curve(
        &mut self,
        sketch: FeatureUid,
        source: FeatureUid,
        linked: bool,
        section: bool,
        occurrence: Option<&[OccurrenceUid]>,
        context: Option<&[OccurrenceUid]>,
    ) -> Result<EditReport, ModelError> {
        let fail = |e: String| invalid(format!("projection of {source}: {e}"));
        let target = self.projection_target(sketch, occurrence, context, &fail)?;
        let at = self.state.require(source)?;
        let entry = &self.state.features[at];
        // A Helix Curve's too (mitcad#1777).
        if !entry.def.makes_curve() {
            return Err(fail(format!(
                "{} ({source}) makes no curve to project; pick an intersection curve or a \
                 helix curve",
                entry.name
            )));
        }
        let target_entry = &self.state.features[target.position];
        if at > target.position {
            return Err(fail(format!(
                "{} ({source}) comes after {} ({sketch}) in the timeline; only an \
                 intersection curve before it can be projected",
                entry.name, target_entry.name
            )));
        }
        if entry.component != target.from {
            let assembly = &self.state.assembly;
            return Err(fail(format!(
                "{} is an intersection curve of {}, not of {}; pick it where an occurrence \
                 of {} shows it",
                entry.name,
                assembly.name(entry.component),
                assembly.name(target.from),
                assembly.name(entry.component)
            )));
        }
        let name = entry.name.clone();
        let curve = self
            .feature_curve(source)
            .ok_or_else(|| fail(format!("{name} has no curve at the timeline marker")))?;
        let curves: Vec<Curve3> = if section {
            // The sketch's plane where the curve is: moved back by the link.
            let normal = target.frame.normal();
            let (origin, normal) = match &target.to_sketch {
                Some(t) => {
                    let back = crate::assembly::inverse(t);
                    (
                        back.apply_point(target.frame.origin),
                        back.apply_vector(normal),
                    )
                }
                None => (target.frame.origin, normal),
            };
            let points: Vec<Curve3> = curve
                .iter()
                .flat_map(|p| plane_crossings(&p.curve, origin, normal))
                .map(Curve3::Point)
                .collect();
            if points.is_empty() {
                return Err(fail(format!("the sketch's plane does not cross {name}")));
            }
            points
        } else {
            curve.iter().map(|p| p.curve.clone()).collect()
        };
        let groups = vec![SourceCurves {
            source: Some(ProjectionSource::Curve(source)),
            chain: Vec::new(),
            curves,
            outline: false,
        }];
        let ProjectionTarget {
            frame,
            link,
            to_sketch,
            ..
        } = target;
        self.push_projections(
            sketch,
            &frame,
            groups,
            to_sketch.as_ref(),
            Projecting {
                body: None,
                link,
                linked,
                section,
                label: if section {
                    "Section Curves to {sketch}"
                } else {
                    "Project to {sketch}"
                },
            },
        )
    }
}
