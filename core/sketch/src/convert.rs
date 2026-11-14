// SPDX-License-Identifier: MIT
//! Convert to Sketch Geometry (mitcad#257): selected projected curves and
//! points become ordinary sketch geometry, free (not fixed), not reference
//! geometry and no longer following their source.
//!
//! - A converted curve takes its points with it, except a point that a
//!   projected curve that stays also uses: that point stays projected
//!   (fixed, still following), and the converted curve stays joined to it.
//! - A selected projected point (a projected vertex) converts the same way
//!   when no projected curve that stays uses it.
//! - The converted entities leave their linked projection; a projection
//!   with nothing left goes, the others keep following their sources with
//!   what is left. A projection made before projections followed by
//!   geometry (an import's, or one of a file of an earlier version) follows
//!   only a source whose curves come back as they were listed, so with some
//!   of its entities converted it keeps its geometry from then on; the
//!   command warns about it.
//! - Constraints and dimensions stay; region keys (made of curve ids) do not
//!   change. A stale curve converted is no longer stale: staleness is about
//!   a projection's entities.
//! - A selected curve an offset made (mitcad#1755) frees that offset: all
//!   of its curves become plain sketch geometry, no longer tied to the
//!   source nor made again from it ([`SketchEdit::free_offset`]).

use std::collections::BTreeSet;

use super::edit::SketchEdit;
use super::{ConstraintUid, Ref};
use crate::ids::EntityUid;

/// What [`SketchEdit::convert_projected`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Converted {
    /// The entities that became ordinary sketch geometry: curves, then
    /// points, in the definition's order.
    pub converted: Vec<Ref>,
    /// Points of converted curves, or selected points, that stay projected
    /// because a projected curve that stays uses them.
    pub kept: Vec<EntityUid>,
    /// Selected entities that are not projected, left as they are.
    pub ignored: Vec<Ref>,
    /// The offsets freed because a curve they made was selected
    /// (mitcad#1755); their curves are in `converted`.
    pub offsets: Vec<ConstraintUid>,
    /// The sources of projections that follow only a source like the one
    /// they were made from and lost entities: they keep their geometry from
    /// now on.
    pub unfollowing: Vec<String>,
}

impl SketchEdit<'_> {
    /// Whether an entity is projected geometry: reference geometry, or
    /// listed by a linked projection.
    pub(crate) fn projected(&self, id: EntityUid) -> bool {
        self.def.entity(id).is_some_and(|e| e.reference)
            || self
                .def
                .projections
                .iter()
                .any(|p| p.entities.iter().any(|r| r.uid() == id))
    }

    /// Turns the projected entities among `entities` into ordinary sketch
    /// geometry, and frees the offsets that made the curves among them
    /// (see the module documentation). Refused when none of them is
    /// projected or an offset's, or when nothing would change (every
    /// selected point is used by projected curves that stay).
    pub fn convert_projected(&mut self, entities: &[Ref]) -> Result<Converted, String> {
        let mut out = Converted::default();
        let mut curves: BTreeSet<EntityUid> = BTreeSet::new();
        let mut points: BTreeSet<EntityUid> = BTreeSet::new();
        let mut offset_curves: Vec<EntityUid> = Vec::new();
        for r in entities {
            let e = self
                .def
                .entity(r.uid())
                .ok_or_else(|| format!("{r} does not exist"))?;
            if e.is_point() != r.is_point() {
                return Err(format!("{r} is a {}", e.kind.type_name()));
            }
            if !self.projected(e.id) {
                if !e.is_point() && !self.offsets_making(&[e.id]).is_empty() {
                    offset_curves.push(e.id);
                } else if !out.ignored.contains(r) {
                    out.ignored.push(*r);
                }
            } else if e.is_point() {
                points.insert(e.id);
            } else {
                curves.insert(e.id);
            }
        }
        if curves.is_empty() && points.is_empty() && offset_curves.is_empty() {
            return Err(
                "nothing to convert: none of the selected entities is projected geometry or \
                 an offset's curve"
                    .to_owned(),
            );
        }
        // Every curve of each offset that made a selected one (mitcad#1755).
        let freed: BTreeSet<EntityUid> = self
            .def
            .offsets
            .iter()
            .filter(|o| o.made().iter().any(|c| offset_curves.contains(c)))
            .flat_map(|o| o.made())
            .collect();
        out.offsets = self.free_offsets_of(&offset_curves)?;
        if curves.is_empty() && points.is_empty() {
            for e in &self.def.entities {
                if freed.contains(&e.id) {
                    out.converted.push(e.as_ref());
                }
            }
            return Ok(out);
        }
        // A projected curve that stays holds its points.
        let held = |p: EntityUid| {
            self.def.entities.iter().any(|e| {
                !e.is_point()
                    && !curves.contains(&e.id)
                    && self.projected(e.id)
                    && e.kind.points().contains(&p)
            })
        };
        let mut candidates: Vec<EntityUid> = Vec::new();
        for e in &self.def.entities {
            if curves.contains(&e.id) {
                for p in e.kind.points() {
                    if !candidates.contains(&p) {
                        candidates.push(p);
                    }
                }
            }
        }
        for e in &self.def.entities {
            if points.contains(&e.id) && !candidates.contains(&e.id) {
                candidates.push(e.id);
            }
        }
        let mut converting: BTreeSet<EntityUid> = curves.clone();
        for p in candidates {
            if !self.projected(p) {
                continue;
            }
            if held(p) {
                out.kept.push(p);
            } else {
                converting.insert(p);
            }
        }
        if converting.is_empty() && out.offsets.is_empty() {
            let names: Vec<String> = out.kept.iter().map(|p| format!("p{}", p.0)).collect();
            return Err(format!(
                "nothing to convert: projected curves that stay use {}; convert them too",
                names.join(", ")
            ));
        }
        // Curves first, then points, as the definition lists them.
        for e in &self.def.entities {
            if (converting.contains(&e.id) || freed.contains(&e.id)) && !e.is_point() {
                out.converted.push(e.as_ref());
            }
        }
        for e in &self.def.entities {
            if converting.contains(&e.id) && e.is_point() {
                out.converted.push(e.as_ref());
            }
        }
        for e in &mut self.def.entities {
            if converting.contains(&e.id) {
                e.reference = false;
                e.fixed = false;
            }
        }
        for projection in &mut self.def.projections {
            let before = projection.entities.len();
            projection
                .entities
                .retain(|r| !converting.contains(&r.uid()));
            let lost = projection.entities.len() < before;
            if lost && !projection.entities.is_empty() && !projection.by_geometry {
                out.unfollowing.push(projection.source.to_string());
            }
        }
        self.def.projections.retain(|p| !p.entities.is_empty());
        Ok(out)
    }
}
