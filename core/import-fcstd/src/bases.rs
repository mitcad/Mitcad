// SPDX-License-Identifier: MIT
//! A Body's base feature (`PartDesign::FeatureBase`, mitcad#1557): the
//! shape of the object its `BaseFeature` names (another Body, a PartDesign
//! clone of one, a Part object) as the Body's first solid.
//!
//! FreeCAD takes the base's shape without the base's own placement and
//! places it by the FeatureBase's `Placement` (a clone of a Body placed
//! where its Body wants it). MitCad's counterpart is a `derive` of the
//! MitCad bodies the import made of the base (linked to them: they follow
//! when the base changes), seen from the Body's component through the
//! occurrences where the document places the two components first, then a
//! `move` that takes that copy where FreeCAD's placement puts it (none
//! when the copy is there already). A placement driven by an expression
//! drives the move: the move is the placement plus the difference between
//! the occurrences, which does not change with the expression.
//!
//! A base that the import does not rebuild (a union of scripted objects,
//! whose shapes come from scripts MitCad does not run) has no bodies to
//! copy: the Body starts from FreeCAD's stored shape, a fallback without a
//! parametric counterpart, with the base's reason.

use std::collections::BTreeMap;

use mitcad_freecad::Object;
use mitcad_model::{BodyUid, Kernel, Transform};
use serde_json::{Value, json};

use super::history::{Candidate, Target, no_counterpart};
use super::params::{Kind, Q};
use super::{Importer, placement_of, transform};

/// Translations below this are no move, mm.
const NO_MOVE: f64 = 1e-9;

impl<K: Kernel> Importer<'_, '_, K> {
    /// The candidates of a `PartDesign::FeatureBase`.
    pub(super) fn feature_base_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        let base = o
            .link("BaseFeature")
            .filter(|l| l.file.is_none())
            .map(|l| l.object.clone())
            .ok_or("its BaseFeature names no object of this document")?;
        let base_object = self.sources[0]
            .object(&base)
            .cloned()
            .ok_or_else(|| format!("no object {base}"))?;
        if !target.current.is_empty() {
            return Err("a base feature after other solids of the Body".to_owned());
        }
        let (bodies, from) = self
            .base_bodies(&base_object)
            .ok_or_else(|| self.base_not_rebuilt(&base_object))?;
        let component = target.place.component;
        let (seen, link) = self
            .seen_in(from.component, component)
            .ok_or_else(|| format!("{base}'s component is not placed"))?;
        let mut links = BTreeMap::new();
        if let Some(link) = &link {
            for b in &bodies {
                links.insert(b.to_string(), link.clone());
            }
        }
        let mut derive = json!({"type": "derive",
            "bodies": bodies.iter().map(ToString::to_string).collect::<Vec<_>>()});
        if !links.is_empty() {
            derive["links"] = json!(links);
        }
        // Where FreeCAD puts the base's shape in this component: the
        // FeatureBase's placement on the base's shape without its own
        // placement (`from.local`: the base's frame in its component).
        let own = placement_of(o);
        let wanted = target
            .place
            .transform
            .after(&transform(&own))
            .after(&mitcad_model::assembly::inverse(&from.local));
        let moved = wanted.after(&mitcad_model::assembly::inverse(&seen));
        let copies: Vec<String> = (0..bodies.len()).map(|j| format!("$0.b{j}")).collect();
        let label = self.sources[0].label(&base);
        let mut reading = Candidate::new(derive.clone()).note(format!(
            "a linked copy of the bodies of {label} (FreeCAD's base feature)"
        ));
        if !is_identity(&moved) {
            let spec = self.base_move(o, target, &own, &moved);
            reading = reading
                .then(
                    json!({"type": "move", "bodies": copies, "transform": spec}),
                    " (placed)",
                )
                .note("placed as FreeCAD places it");
        }
        // The copy where the occurrences show the base, when FreeCAD's
        // placement differs only by them.
        let mut readings = vec![reading];
        if !is_identity(&moved) {
            readings.push(Candidate::new(derive).note(format!(
                "a linked copy of the bodies of {label} where the assembly shows them"
            )));
        }
        Ok(readings)
    }

    /// The MitCad bodies holding a base's result, and its place: a Body or
    /// a Part object the replay rebuilt, or a body of its stored shape.
    fn base_bodies(&self, base: &Object) -> Option<(Vec<BodyUid>, BasePlace)> {
        let index = self.replay.roots.get(&base.name).copied().or_else(|| {
            self.planned
                .iter()
                .position(|p| p.key.doc == 0 && p.owner.as_deref() == Some(&base.name))
        })?;
        let planned = &self.planned[index];
        let bodies: Vec<BodyUid> = match self.replay.bodies.get(&base.name) {
            Some(b) => b.clone(),
            None => match self.replay.results.get(&base.name) {
                Some(made) => made.bodies.iter().map(|(b, _)| *b).collect(),
                None => planned
                    .body
                    .into_iter()
                    .chain(planned.more.iter().copied())
                    .collect(),
            },
        };
        let bodies: Vec<BodyUid> = bodies
            .into_iter()
            .filter(|b| self.doc.body_shape(*b).is_some())
            .collect();
        if bodies.is_empty() {
            return None;
        }
        // The stored shape (in the base's container, with its placement)
        // goes into the component by the planned transform: the base's own
        // frame is that after its placement.
        let local = planned
            .transform
            .unwrap_or(Transform::IDENTITY)
            .after(&transform(&placement_of(base)));
        Some((
            bodies,
            BasePlace {
                component: planned.component,
                local,
            },
        ))
    }

    /// Why a base has no bodies to copy.
    fn base_not_rebuilt(&self, base: &Object) -> String {
        let source = &self.sources[0];
        let label = source.label(&base.name);
        // The scripted or plain objects it is made of.
        let mut kinds: Vec<String> = base
            .references()
            .filter_map(|(_, l)| source.object(&l.object))
            .filter(|x| no_counterpart(&x.type_name).is_some())
            .map(|x| x.type_name.clone())
            .collect();
        kinds.sort();
        kinds.dedup();
        if let Some(why) = no_counterpart(&base.type_name) {
            return format!("its base {label}: {why}");
        }
        if !kinds.is_empty() {
            return format!(
                "no parametric counterpart: its base {label} ({}) is made of objects without \
                 one ({}), whose shapes come from scripts or are plain shapes; the Body starts \
                 from FreeCAD's stored shape of it",
                base.type_name,
                kinds.join(", ")
            );
        }
        format!(
            "its base {label} ({}) is not imported as a body of its own",
            base.type_name
        )
    }

    /// The move that takes the copy where FreeCAD places it: a translation
    /// (with the placement's expressions, plus the fixed difference the
    /// occurrences make) when it turns nothing, else a free move.
    fn base_move(
        &mut self,
        o: &Object,
        target: &Target,
        own: &mitcad_freecad::Placement,
        moved: &Transform,
    ) -> Value {
        let rigid_into = is_identity(&Transform {
            linear: target.place.transform.linear,
            translation: [0.0; 3],
        });
        if !is_identity(&Transform {
            linear: moved.linear,
            translation: [0.0; 3],
        }) {
            let m = moved;
            let row = |r: usize| {
                json!([
                    m.linear[r][0],
                    m.linear[r][1],
                    m.linear[r][2],
                    m.translation[r]
                ])
            };
            return json!({"type": "free", "matrix": [row(0), row(1), row(2)]});
        }
        let mut axes = Vec::with_capacity(3);
        for (k, axis) in ["x", "y", "z"].iter().enumerate() {
            let value = moved.translation[k];
            // The placement's own coordinate drives it where the frame
            // into the component turns nothing.
            let q = if rigid_into {
                let path = format!("Placement.Base.{axis}");
                let stored = own.position[k];
                let q = self.quantity(
                    super::params::Source::property(&o.name, &path),
                    Kind::Length,
                    stored,
                );
                let rest = value - stored;
                if rest.abs() <= NO_MOVE {
                    Q { value, ..q }
                } else {
                    let constant = Q::number(rest.abs(), Kind::Length);
                    let sign = if rest < 0.0 { "-" } else { "+" };
                    Q::combine(&[&q, &constant], value, Kind::Length, |ops| {
                        format!("{} {sign} {}", ops[0], ops[1])
                    })
                }
            } else {
                Q::number(value, Kind::Length)
            };
            axes.push(q.json());
        }
        json!({"type": "translate_xyz", "x": axes[0], "y": axes[1], "z": axes[2]})
    }
}

/// Where a base's bodies are: their component, and the base's own frame
/// (its shape without its placement) in that component.
struct BasePlace {
    component: mitcad_model::ComponentUid,
    local: Transform,
}

fn is_identity(t: &Transform) -> bool {
    (0..3).all(|r| {
        (0..3).all(|c| (t.linear[r][c] - if r == c { 1.0 } else { 0.0 }).abs() <= 1e-12)
            && t.translation[r].abs() <= NO_MOVE
    })
}
