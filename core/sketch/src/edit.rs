// SPDX-License-Identifier: MIT
//! Editing a sketch: the operations behind the `sketch.*` commands. An
//! edit works on a copy of the definition with the document's parameters
//! (new dimensions get parameters owned by the sketch); when it is done,
//! the sketch is solved from the edited positions with the current values
//! and the solution is stored. A sketch that no longer solves rejects the
//! edit, as an over-constraining constraint is refused.

use std::collections::BTreeSet;

use super::conflict;
use super::geometry::dist;
use super::solve::{SketchStatus, SolveError, Solved, SolverItem, measure_in};
use super::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityKind, Ref,
};
use crate::def::SketchDef;

/// The most constraints of a conflict that a refused edit leaves out one by
/// one to find those that block it (mitcad#1566); more are all named.
const MAX_TRIED: usize = 24;
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::{ParamId, Parameters};
use crate::values::ValueInput;
use mitcad_model_base::internal::values::{resolve_value, value_uses_parameter};

/// What an edit created and removed, and the sketch's status after it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditReport {
    pub entities: Vec<Ref>,
    pub texts: Vec<EntityUid>,
    pub constraints: Vec<ConstraintUid>,
    pub dimensions: Vec<ConstraintUid>,
    /// Parameters created for new dimensions.
    pub parameters: Vec<String>,
    /// Removed entities, constraints and dimensions.
    pub removed: Vec<String>,
    pub status: SketchStatus,
    /// What the user is to know about the edit (mitcad#741: a curve left
    /// out of an offset or a projection as the same as another).
    pub warnings: Vec<String>,
    /// What the user may like to know that is no warning (mitcad#1234: the
    /// curves of an offset that vanish at its distance).
    pub notes: Vec<String>,
    /// Own parameters of dimensions kept as user parameters because the
    /// dimension's new value uses them (mitcad#1535).
    pub kept: Vec<KeptParameter>,
}

/// A dimension's own parameter kept as a user parameter (mitcad#1535):
/// its name, the dimension, and the parameter the dimension uses now (a
/// new one of its own, or the kept one for its bare name).
#[derive(Debug, Clone, PartialEq)]
pub struct KeptParameter {
    pub name: String,
    pub dimension: ConstraintUid,
    pub parameter: String,
}

/// A driving dimension's new value, resolved into its parameter: what a
/// planar sketch's and a 3D sketch's dimensions share (mitcad#1535).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DimensionValue {
    pub param: ParamId,
    /// The parameter created for it, if any.
    pub created: Option<String>,
    /// Its own parameter kept as a user parameter, with the note that says
    /// so.
    pub kept: Option<(KeptParameter, String)>,
}

/// Resolves the value of dimension `id` of sketch `owner` into a parameter
/// (`slot` and `comment` as [`resolve_value`] takes them; `old` is the
/// dimension's parameter, the one it keeps for an expression). A value
/// that uses the dimension's own parameter (`BuckleDiameter * 2` in the
/// dimension of BuckleDiameter) keeps that parameter, with its name and
/// value, as a user parameter, and the dimension takes the value as a
/// parameter of its own (mitcad#1535): the bare name makes it use the user
/// parameter. A parameter another dimension of the sketch uses as well
/// (`shared`) stays as it is; this dimension only stops using it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_dimension_value(
    params: &mut Parameters,
    owner: FeatureUid,
    id: ConstraintUid,
    slot: &str,
    comment: &str,
    value: &ValueInput,
    old: Option<ParamId>,
    shared: impl Fn(ParamId) -> bool,
) -> Result<DimensionValue, String> {
    let mut old = old;
    let used =
        old.filter(|p| params.owner(*p) == Some(owner) && value_uses_parameter(params, value, *p));
    let mut kept = None;
    if let Some(p) = used {
        if !shared(p) {
            params.set_owner(p, None);
            kept = Some(p);
        }
        old = None;
    }
    let (param, created) = resolve_value(params, slot, value, owner, comment, old.as_ref())
        .map_err(|e| e.to_string())?;
    let kept = kept.map(|p| {
        let kept_name = params.name(p);
        let shown = params
            .get(p)
            .map(|q| q.expression().to_owned())
            .unwrap_or_default();
        let now = params.name(param);
        // In plain words for a person, the dimension's id for agents
        // (mitcad#1612).
        let note = if param == p {
            crate::said(
                format!("{kept_name} is now a user parameter ({shown}); the dimension uses it"),
                format!("{kept_name} is now a user parameter ({shown}); dimension {id} uses it"),
            )
        } else {
            let expression = params
                .get(param)
                .map(|q| q.expression().to_owned())
                .unwrap_or_default();
            crate::said(
                format!(
                    "{kept_name} is now a user parameter ({shown}); the dimension is {now} = \
                     {expression}"
                ),
                format!(
                    "{kept_name} is now a user parameter ({shown}); dimension {id} is {now} = \
                     {expression}"
                ),
            )
        };
        let record = KeptParameter {
            name: kept_name,
            dimension: id,
            parameter: now,
        };
        (record, note)
    });
    Ok(DimensionValue {
        param,
        created,
        kept,
    })
}

/// A point of a command: an existing point or a new one at a position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointInput {
    At([f64; 2]),
    Existing(EntityUid),
}

/// An edit in progress.
pub struct SketchEdit<'a> {
    pub def: SketchDef,
    pub(crate) params: &'a mut Parameters,
    owner: FeatureUid,
    owner_name: String,
    next_entity: u32,
    /// The result ids an offset made again gives its source curves
    /// (mitcad#1234: a curve kept before and after keeps its id).
    pub(crate) reuse_results: std::collections::BTreeMap<EntityUid, EntityUid>,
    /// The sources' geometry an offset made again is fitted to (mitcad#1234),
    /// in place of a solve of the sketch as it is.
    pub(crate) known_sources: Option<Solved>,
    next_constraint: u32,
    pub report: EditReport,
    /// Solve and store when finishing (false for edits that set positions
    /// themselves, like a drag).
    pub solve: bool,
    /// The last drag's solve with the definition it left (mitcad#679):
    /// finishing takes it instead of solving that definition again.
    pub(crate) dragged: Option<Box<(SketchDef, Parameters, Solved)>>,
    /// What the edit does, as a refusal for a conflict names it ("the
    /// fillet", mitcad#1566); "the change" unless the command says.
    pub change: String,
    /// The constraints and dimensions there before that the edit changes
    /// (a new value, activated): a refusal counts them as the change, not
    /// as what blocks it (mitcad#1566).
    pub changing: Vec<ConstraintUid>,
}

impl<'a> SketchEdit<'a> {
    pub fn new(
        def: SketchDef,
        params: &'a mut Parameters,
        owner: FeatureUid,
        owner_name: &str,
    ) -> Self {
        let next_entity = def.next_entity().0;
        let next_constraint = def.next_constraint().0;
        Self {
            def,
            params,
            owner,
            owner_name: owner_name.to_owned(),
            next_entity,
            reuse_results: std::collections::BTreeMap::new(),
            known_sources: None,
            next_constraint,
            report: EditReport::default(),
            solve: true,
            dragged: None,
            change: "the change".to_owned(),
            changing: Vec::new(),
        }
    }

    /// Whether the sketch has a constraint or a dimension of this id.
    fn has_item(&self, id: ConstraintUid) -> bool {
        self.def.constraints.iter().any(|c| c.id == id)
            || self.def.dimensions.iter().any(|d| d.id == id)
    }

    pub fn owner(&self) -> FeatureUid {
        self.owner
    }

    /// The sketch's name, for parameter comments.
    pub fn owner_name(&self) -> &str {
        &self.owner_name
    }

    /// `count` new entity ids in order.
    pub fn ids(&mut self, count: usize) -> Vec<EntityUid> {
        (0..count)
            .map(|_| {
                self.next_entity += 1;
                EntityUid(self.next_entity - 1)
            })
            .collect()
    }

    pub fn id(&mut self) -> EntityUid {
        self.ids(1)[0]
    }

    /// Adds an entity whose id came from [`SketchEdit::ids`].
    pub fn push(&mut self, entity: Entity) -> EntityUid {
        let id = entity.id;
        self.report.entities.push(entity.as_ref());
        self.def.entities.push(entity);
        id
    }

    pub fn add_point(&mut self, at: [f64; 2]) -> EntityUid {
        let id = self.id();
        self.push(Entity::point(id, at))
    }

    /// An existing point, or a new one.
    pub fn point(&mut self, input: PointInput) -> Result<EntityUid, String> {
        match input {
            PointInput::At(at) => {
                if !at.iter().all(|v| v.is_finite()) {
                    return Err(format!("the point must be finite, got {at:?}"));
                }
                Ok(self.add_point(at))
            }
            PointInput::Existing(id) => self.existing_point(id).map(|_| id),
        }
    }

    /// Where an existing point is, or why `id` names none. Points, curves
    /// and texts share one numbering (`p3`, `c4`, `t5`), so a point id
    /// guessed ahead of a command's result may name a curve (mitcad#1269).
    pub fn existing_point(&self, id: EntityUid) -> Result<[f64; 2], String> {
        match self.def.entity(id) {
            Some(e) if e.is_point() => self.at(id),
            Some(e) => Err(format!(
                "{} is not a point: points, curves and texts of a sketch share one numbering \
                 (p{n} and c{n} are one entity), so use the point ids a result returns \
                 (entities, made) or the sketch query lists",
                e.as_ref(),
                n = id.0
            )),
            None => Err(format!("point p{} does not exist", id.0)),
        }
    }

    /// Where a point input is: the position given, or an existing point's
    /// (mitcad#1269: a drawing tool's point inputs take either).
    pub fn input_position(&self, input: PointInput) -> Result<[f64; 2], String> {
        match input {
            PointInput::At(at) => {
                if !at.iter().all(|v| v.is_finite()) {
                    return Err(format!("the point must be finite, got {at:?}"));
                }
                Ok(at)
            }
            PointInput::Existing(id) => self.existing_point(id),
        }
    }

    /// The position of a point.
    pub fn at(&self, id: EntityUid) -> Result<[f64; 2], String> {
        match self.def.entity(id).map(|e| &e.kind) {
            Some(EntityKind::Point { at }) => Ok(*at),
            _ => Err(format!("point p{} does not exist", id.0)),
        }
    }

    pub fn add_curve(&mut self, kind: EntityKind) -> EntityUid {
        let id = self.id();
        self.push(Entity::new(id, kind))
    }

    /// A curve of the sketch.
    pub fn curve(&self, id: EntityUid) -> Result<&Entity, String> {
        match self.def.entity(id) {
            Some(e) if !e.is_point() => Ok(e),
            Some(_) => Err(format!("p{} is a point, not a curve", id.0)),
            None => Err(format!("curve c{} does not exist", id.0)),
        }
    }

    pub fn constraint_id(&mut self) -> ConstraintUid {
        self.next_constraint += 1;
        ConstraintUid(self.next_constraint - 1)
    }

    pub fn add_constraint(&mut self, kind: ConstraintKind) -> ConstraintUid {
        let id = self.constraint_id();
        self.def.constraints.push(Constraint::new(id, kind));
        self.report.constraints.push(id);
        id
    }

    /// The current geometry solved with the current values. A conflict is
    /// told as [`SketchEdit::refusal`] tells it, every user constraint and
    /// dimension of it named.
    pub fn solved(&self) -> Result<Solved, String> {
        self.solve_def(&self.def)
            .map_err(|e| self.refusal(&e, false))
    }

    /// Whether the sketch solves now; a conflict told by what blocks the
    /// change ([`SketchEdit::refusal`], narrowed).
    pub fn check_solves(&self) -> Result<(), String> {
        self.solve_def(&self.def)
            .map(|_| ())
            .map_err(|e| self.refusal(&e, true))
    }

    /// A definition solved with the edit's values.
    fn solve_def(&self, def: &SketchDef) -> Result<Solved, SolveError> {
        let params = &*self.params;
        def.solve_with(
            &mut |id| {
                params
                    .value(id)
                    .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
            },
            &|id| params.name(id),
        )
    }

    /// Why the edit does not solve (mitcad#1566): a conflict in the user's
    /// words, the constraints and dimensions that block the change named
    /// ([`conflict::blocked`]) and listed by id on the last line
    /// ([`conflict::trailer`]); any other failure as the solve tells it.
    /// The blocking items are the user's own: not fixed geometry, not what
    /// the edit itself adds (a fillet's tangents and its radius). With
    /// `narrow`, only those whose leaving out lets the sketch solve with the
    /// least movement of what was there ([`SketchEdit::blocking`]); else, or
    /// when none or too many are found, all of them.
    pub fn refusal(&self, error: &SolveError, narrow: bool) -> String {
        if error.status.conflicts.is_empty() {
            return error.message.clone();
        }
        let mut items: Vec<SolverItem> = error.status.conflicts.iter().flatten().copied().collect();
        items.sort();
        items.dedup();
        let own: BTreeSet<ConstraintUid> = self
            .report
            .constraints
            .iter()
            .chain(&self.report.dimensions)
            .chain(&self.changing)
            .copied()
            .collect();
        let candidates: Vec<ConstraintUid> = conflict::user_items(&items)
            .into_iter()
            .filter(|k| !own.contains(k) && self.has_item(*k))
            .collect();
        let mut blockers = if narrow && candidates.len() <= MAX_TRIED {
            self.blocking(&candidates)
        } else {
            Vec::new()
        };
        if blockers.is_empty() {
            blockers = candidates;
        }
        let params = &*self.params;
        let inputs = self
            .def
            .dimension_inputs(&mut |id| params.value(id).ok_or_else(String::new), &|id| {
                params.name(id)
            })
            .unwrap_or_default();
        let words = conflict::Context {
            entities: &self.def.entities,
            constraints: &self.def.constraints,
            dimensions: &inputs,
            bindings: &[],
        };
        let fixed = conflict::fixed_words(&words, &items);
        format!(
            "{}{}",
            conflict::blocked(&words, &blockers, &self.change, fixed),
            conflict::trailer(&blockers)
        )
    }

    /// Of the conflict's `candidates`, those that block the change
    /// (mitcad#1566): each left out in turn, those whose leaving out lets
    /// the sketch solve, and of them the ones that move the points that were
    /// there the least (the change's own new points left out). An Equal
    /// that a fillet shortens its line against is found so rather than the
    /// dimension or the parallel that would let the line move away.
    fn blocking(&self, candidates: &[ConstraintUid]) -> Vec<ConstraintUid> {
        let made: BTreeSet<EntityUid> = self.report.entities.iter().map(|r| r.uid()).collect();
        let mut found: Vec<(f64, ConstraintUid)> = Vec::new();
        for &k in candidates {
            let mut def = self.def.clone();
            if let Some(c) = def.constraints.iter_mut().find(|c| c.id == k) {
                c.active = false;
            } else if let Some(d) = def.dimensions.iter_mut().find(|d| d.id == k) {
                d.active = false;
            } else {
                continue;
            }
            let Ok(solved) = self.solve_def(&def) else {
                continue;
            };
            let moved: f64 = def
                .entities
                .iter()
                .filter(|e| !made.contains(&e.id))
                .filter_map(|e| match e.kind {
                    EntityKind::Point { at } => solved.points.get(&e.id).map(|p| dist(*p, at)),
                    _ => None,
                })
                .sum();
            found.push((moved, k));
        }
        let least = found.iter().map(|f| f.0).fold(f64::INFINITY, f64::min);
        let within = least + (least * 1e-3).max(1e-6);
        found
            .into_iter()
            .filter(|(moved, _)| *moved <= within)
            .map(|(_, k)| k)
            .collect()
    }

    /// Adds a dimension: driven, or driving with a value (a number, a
    /// parameter name or an expression), or driving with the measured
    /// value when `value` is None. `comment` names the new parameter
    /// (`Sketch1 width`); by default it is the dimension type.
    pub fn add_dimension(
        &mut self,
        kind: DimensionKind,
        value: Option<&ValueInput>,
        driven: bool,
        comment: Option<&str>,
    ) -> Result<ConstraintUid, String> {
        let id = self.constraint_id();
        let mut dimension: Dimension<ParamId> = Dimension::new(id, kind, None);
        if !driven {
            let measured;
            let value = match value {
                Some(value) => value,
                None => {
                    // Measure on the geometry as it is now.
                    let solved = self.solved()?;
                    measured = ValueInput::Number(
                        measure_in(&dimension.kind, &solved, &self.def.constraints)
                            .ok_or("the dimension cannot be measured")?,
                    );
                    &measured
                }
            };
            let slot = dimension.slot();
            let label = dimension.kind.type_name().to_owned();
            let comment = format!("{} {}", self.owner_name, comment.unwrap_or(&label));
            let (param, created) =
                resolve_value(self.params, &slot, value, self.owner, &comment, None)
                    .map_err(|e| e.to_string())?;
            self.report.parameters.extend(created);
            dimension.value = Some(param);
        }
        self.def.dimensions.push(dimension);
        self.report.dimensions.push(id);
        Ok(id)
    }

    /// Makes text `id` follow a text parameter (mitcad#664): `value` is a
    /// text parameter's name or a text expression (`'Box ' & GridX`), which
    /// becomes the sketch's own parameter of the text (or changes the one
    /// it has); an empty name stops it following one, and None leaves it
    /// as it is. The text's `text` becomes the parameter's text.
    pub fn set_text_value(
        &mut self,
        id: EntityUid,
        value: Option<&ValueInput>,
    ) -> Result<(), String> {
        let Some(value) = value else {
            return Ok(());
        };
        if !self.def.texts.iter().any(|t| t.id == id) {
            return Err(format!("text t{} does not exist", id.0));
        }
        let old = self.def.text_values.iter().position(|v| v.text == id);
        if matches!(value, ValueInput::Name(name) if name.trim().is_empty()) {
            if let Some(i) = old {
                self.def.text_values.remove(i);
            }
            return Ok(());
        }
        let slot = format!("texts[t{}].text", id.0);
        let comment = format!("{} t{} text", self.owner_name, id.0);
        let old_param = old.map(|i| self.def.text_values[i].value);
        let (param, created) = resolve_value(
            self.params,
            &slot,
            value,
            self.owner,
            &comment,
            old_param.as_ref(),
        )
        .map_err(|e| e.to_string())?;
        self.report.parameters.extend(created);
        match old {
            Some(i) => self.def.text_values[i].value = param,
            None => self.def.text_values.push(crate::TextValue {
                text: id,
                value: param,
            }),
        }
        let text = self.params.text(param).unwrap_or_default().to_owned();
        if let Some(t) = self.def.texts.iter_mut().find(|t| t.id == id) {
            t.text = text;
            // Stored outlines of another string are dropped (mitcad#416).
            if t.glyphs.as_ref().is_some_and(|g| !g.fit(t)) {
                t.glyphs = None;
            }
        }
        Ok(())
    }

    /// The number of redundant constraints now. A constraint or driving
    /// dimension that raises it is implied by the others; the commands that
    /// add one refuse it, as they refuse one that over-constrains the
    /// sketch (a dimension can be added as driven instead).
    pub fn redundancy(&self) -> Result<usize, String> {
        Ok(self.solved()?.status.redundant.len())
    }

    /// Refuses a new constraint or driving dimension the others imply, and
    /// one that conflicts with them: then by the constraints that block it
    /// (mitcad#1566, [`SketchEdit::refusal`]). Named by the change ("the
    /// new constraint would over-constrain the sketch"), not by its id.
    pub fn require_needed(&self, before: usize) -> Result<(), String> {
        let redundant = match self.solve_def(&self.def) {
            Ok(solved) => solved.status.redundant.len(),
            Err(error) => return Err(self.refusal(&error, true)),
        };
        if redundant > before {
            return Err(format!(
                "{} would over-constrain the sketch: the others already fix what it \
                 constrains (a dimension can be added as driven)",
                self.change
            ));
        }
        Ok(())
    }

    /// Makes room for a new constraint `new` that is only partly implied
    /// (mitcad#287): when it raised the redundant count above `before`'s
    /// (the status before the command) and removing older geometric
    /// constraints of the new redundant sets brings the count back while
    /// the new one still takes a degree of freedom away, those are removed
    /// as the user's last intent replaces them, and returned. Never a
    /// dimension, a fixed entity, an equal spacing or a constraint on
    /// projected geometry (the link that holds the sketch to the model).
    /// Otherwise nothing changes and the result is empty: a contradiction
    /// fails the solve, and a constraint the others imply in full is
    /// refused by [`SketchEdit::require_needed`] as before.
    pub fn drop_implied(&mut self, new: ConstraintUid, before: &SketchStatus) -> Vec<Constraint> {
        let limit = before.redundant.len();
        let Ok(solved) = self.solved() else {
            return Vec::new();
        };
        let mut status = solved.status;
        // (index in the list when taken, constraint), in order.
        let mut taken: Vec<(usize, Constraint)> = Vec::new();
        while status.redundant.len() > limit {
            let mut progress = None;
            for k in self.implied_candidates(new, &status, before) {
                let Some(i) = self.def.constraints.iter().position(|c| c.id == k) else {
                    continue;
                };
                let c = self.def.constraints.remove(i);
                match self.solved() {
                    Ok(s) if s.status.redundant.len() < status.redundant.len() => {
                        progress = Some((i, c, s.status));
                        break;
                    }
                    _ => self.def.constraints.insert(i, c),
                }
            }
            let Some((i, c, next)) = progress else {
                break;
            };
            taken.push((i, c));
            status = next;
        }
        if taken.is_empty() || status.redundant.len() > limit || status.dof >= before.dof {
            for (i, c) in taken.into_iter().rev() {
                self.def.constraints.insert(i, c);
            }
            return Vec::new();
        }
        let removed: Vec<Constraint> = taken.into_iter().map(|(_, c)| c).collect();
        for c in &removed {
            self.report.removed.push(c.id.to_string());
        }
        removed
    }

    /// The constraints [`SketchEdit::drop_implied`] may remove: the older
    /// geometric constraints in the redundant sets that were not there
    /// before; those on the new constraint's movable entities first, each
    /// group in id order.
    fn implied_candidates(
        &self,
        new: ConstraintUid,
        status: &SketchStatus,
        before: &SketchStatus,
    ) -> Vec<ConstraintUid> {
        let mut found = std::collections::BTreeSet::new();
        for set in status
            .redundant_sets
            .iter()
            .filter(|set| !before.redundant_sets.contains(set))
        {
            for item in set {
                let SolverItem::Constraint(k) = *item else {
                    continue;
                };
                if k == new {
                    continue;
                }
                // Dimensions are not in the constraint list.
                let Some(c) = self.def.constraints.iter().find(|c| c.id == k) else {
                    continue;
                };
                let removable = !matches!(c.kind, ConstraintKind::EqualSpacing { .. })
                    && !c.kind.refs().iter().any(|r| self.projected(r.uid()));
                if removable {
                    found.insert(k);
                }
            }
        }
        // Those on the new constraint's own movable geometry first
        // (mitcad#865): a Symmetry of two circles replaces the constraint
        // that held one of the circles, not one further along the chain
        // its position follows from (a rectangle's Horizontal).
        let near = self
            .def
            .constraints
            .iter()
            .find(|c| c.id == new)
            .map(|c| self.movable_reach(&c.kind.refs()))
            .unwrap_or_default();
        let (mut first, rest): (Vec<_>, Vec<_>) = found.into_iter().partition(|k| {
            self.def
                .constraints
                .iter()
                .any(|c| c.id == *k && !self.movable_reach(&c.kind.refs()).is_disjoint(&near))
        });
        first.extend(rest);
        first
    }

    /// The entities `refs` name and the points of the curves among them,
    /// without the fixed ones (an axis, the origin, projected geometry).
    fn movable_reach(&self, refs: &[Ref]) -> std::collections::BTreeSet<EntityUid> {
        let mut reach = std::collections::BTreeSet::new();
        for r in refs {
            reach.insert(r.uid());
            if !r.is_point()
                && let Some(e) = self.def.entity(r.uid())
            {
                reach.extend(e.kind.points());
            }
        }
        reach.retain(|id| {
            self.def
                .entity(*id)
                .is_some_and(|e| !e.fixed && !e.reference)
        });
        reach
    }

    /// Leaves the radius equality out of a new Symmetry of two circles or
    /// two arcs (mitcad#865) when the others already make their radii
    /// equal (both dimensioned, or an Equal): the constraint then mirrors
    /// the positions only (`radii: false`), so that it takes what is not
    /// fixed yet instead of over-constraining the sketch. `before` is the
    /// status before the command. Returns whether it did.
    pub fn fit_symmetric(&mut self, new: ConstraintUid, before: &SketchStatus) -> bool {
        let Some(i) = self.def.constraints.iter().position(|c| c.id == new) else {
            return false;
        };
        let ConstraintKind::Symmetric {
            a: Ref::Curve(a),
            b: Ref::Curve(b),
            radii: true,
            ..
        } = self.def.constraints[i].kind
        else {
            return false;
        };
        let round = |id: EntityUid| {
            matches!(
                self.def.entity(id).map(|e| &e.kind),
                Some(EntityKind::Circle { .. } | EntityKind::Arc { .. })
            )
        };
        if !(round(a) && round(b)) {
            return false;
        }
        // The radius equality alone in its place: implied when it raises
        // the redundant count.
        let symmetric = std::mem::replace(
            &mut self.def.constraints[i].kind,
            ConstraintKind::Equal { a, b },
        );
        let implied = self
            .solved()
            .is_ok_and(|s| s.status.redundant.len() > before.redundant.len());
        self.def.constraints[i].kind = symmetric;
        if implied
            && let ConstraintKind::Symmetric { radii, .. } = &mut self.def.constraints[i].kind
        {
            *radii = false;
        }
        implied
    }

    /// A failed finish's `error`, a conflict told by
    /// [`SketchEdit::refusal`] with the constraints that block the change
    /// narrowed down (mitcad#1566); any other as it is.
    fn narrowed(&self, error: String) -> String {
        match self.solve_def(&self.def) {
            Err(e) if !e.status.conflicts.is_empty() => self.refusal(&e, true),
            _ => error,
        }
    }

    /// Checks the values, solves from the edited positions and stores the
    /// solution.
    pub fn finish(mut self) -> Result<(SketchDef, EditReport), String> {
        // Patterns follow structural edits of their originals.
        if !self.def.patterns.is_empty() {
            self.sync_patterns()?;
        }
        self.def.check_definition()?;
        let params = &*self.params;
        self.def
            .check_values(&|id| params.value(id).unwrap_or(f64::NAN))?;
        if self.solve {
            // A drag just solved this very definition with these values
            // and stored its solution (mitcad#679): solving it again gives
            // the same. Derived offsets refit first, which solves anyway,
            // and an offset whose curves that vanish changed with the drag
            // is made again (mitcad#1234).
            let dragged = self.dragged.take().filter(|dragged| {
                dragged.0 == self.def
                    && dragged.1 == *self.params
                    && !self.def.offsets.iter().any(|o| o.derived)
                    && !self.offsets_stale(&dragged.2)
            });
            let solved = match dragged {
                Some(dragged) => dragged.2,
                // A conflict names what blocks the change (mitcad#1566).
                None => self.settle_offsets().map_err(|e| self.narrowed(e))?,
            };
            solved.store(&mut self.def.entities);
            // Texts in frames keep their frame's corner and direction.
            for t in &mut self.def.texts {
                if let Some((at, angle)) = super::text::frame_anchor(t, &solved.points) {
                    t.at = at;
                    t.angle = angle;
                }
            }
            self.report.status = solved.status;
        }
        let status = self.solve.then_some(&self.report.status);
        let created = settle_references(
            &mut self.def,
            self.params,
            self.owner,
            &self.owner_name,
            &|_| None,
            status,
        )?;
        self.report.parameters.extend(created);
        Ok((self.def, self.report))
    }

    // V0's tools, used by the current application.

    /// A rectangle dimensioned by width and height from a fixed corner:
    /// lines `c<k>`..`c<k+3>` (corners counter-clockwise from `corner`),
    /// then the corner points, horizontal and vertical constraints, and
    /// length dimensions on the first two lines (see `legacy.rs`).
    pub fn dimensioned_rectangle(
        &mut self,
        corner: [f64; 2],
        width: &ValueInput,
        height: &ValueInput,
    ) -> Result<Vec<EntityUid>, String> {
        if !corner.iter().all(|v| v.is_finite()) {
            return Err(format!("the point must be finite, got {corner:?}"));
        }
        let lines = self.ids(4);
        let points = self.ids(4);
        // Start from the given size when it is a number; the dimensions
        // set it anyway.
        let size = |v: &ValueInput| match v {
            ValueInput::Number(n) if n.is_finite() && *n > 0.0 => *n,
            _ => 10.0,
        };
        let (w, h) = (size(width), size(height));
        let [x, y] = corner;
        let corners = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
        for i in 0..4 {
            self.push(Entity::new(
                lines[i],
                EntityKind::Line {
                    start: points[i],
                    end: points[(i + 1) % 4],
                    centerline: false,
                },
            ));
        }
        for i in 0..4 {
            let mut point = Entity::point(points[i], corners[i]);
            point.fixed = i == 0;
            self.push(point);
        }
        for (i, line) in lines.iter().enumerate() {
            self.add_constraint(if i.is_multiple_of(2) {
                ConstraintKind::Horizontal { line: *line }
            } else {
                ConstraintKind::Vertical { line: *line }
            });
        }
        self.add_dimension(
            DimensionKind::Length { line: lines[0] },
            Some(width),
            false,
            Some("width"),
        )?;
        self.add_dimension(
            DimensionKind::Length { line: lines[1] },
            Some(height),
            false,
            Some("height"),
        )?;
        Ok(lines)
    }

    /// A circle with a fixed centre and a diameter dimension.
    pub fn dimensioned_circle(
        &mut self,
        center: [f64; 2],
        diameter: &ValueInput,
    ) -> Result<EntityUid, String> {
        if !center.iter().all(|v| v.is_finite()) {
            return Err(format!("the point must be finite, got {center:?}"));
        }
        let ids = self.ids(2);
        let d = match diameter {
            ValueInput::Number(n) if n.is_finite() && *n > 0.0 => *n,
            _ => 10.0,
        };
        self.push(Entity::new(
            ids[0],
            EntityKind::Circle {
                center: ids[1],
                radius: d / 2.0,
                start: 0.0,
            },
        ));
        let mut point = Entity::point(ids[1], center);
        point.fixed = true;
        self.push(point);
        self.add_dimension(
            DimensionKind::Diameter { curve: ids[0] },
            Some(diameter),
            false,
            Some("diameter"),
        )?;
        Ok(ids[0])
    }
}

/// What a reference parameter's comment ends in: `Sketch1 length (driven)`.
pub(crate) const DRIVEN_SUFFIX: &str = " (driven)";

/// Gives every driven dimension its read-only reference parameter
/// (mitcad#231) and takes it from driving ones: a reference the sketch
/// owns is kept (`old` gives the one a slot had before an edit that left
/// it out), a user parameter the definition names as the reference becomes
/// the sketch's (mitcad#250: an importer's parameter of a driven dimension
/// of the file), else the next free `d<n>` is made, commented `Sketch1
/// length (driven)`. With `measured` (the solved status), references take
/// the values their dimensions measure. Returns the names of new
/// parameters.
#[doc(hidden)]
pub fn settle_references(
    def: &mut SketchDef,
    params: &mut Parameters,
    owner: FeatureUid,
    owner_name: &str,
    old: &dyn Fn(&str) -> Option<ParamId>,
    measured: Option<&SketchStatus>,
) -> Result<Vec<String>, String> {
    let mut taken = std::collections::BTreeSet::new();
    let mut created = Vec::new();
    for d in &mut def.dimensions {
        if d.value.is_some() {
            d.reference = None;
            continue;
        }
        let slot = d.slot();
        if let Some(id) = d.reference.filter(|id| {
            params.get(*id).is_some() && params.owner(*id).is_none() && !taken.contains(id)
        }) {
            let comment = format!("{owner_name} {}{DRIVEN_SUFFIX}", d.kind.type_name());
            adopt_reference(params, id, owner, &comment).map_err(|e| format!("{slot}: {e}"))?;
        }
        let usable = |id: ParamId| {
            params.get(id).is_some() && params.owner(id) == Some(owner) && !taken.contains(&id)
        };
        let id = match d
            .reference
            .filter(|id| usable(*id))
            .or_else(|| old(&slot).filter(|id| usable(*id) && params.is_reference(*id)))
        {
            Some(id) => {
                params.set_kind(id, crate::expr::ParamKind::Reference);
                id
            }
            None => {
                let unit = match crate::parameters::slot_unit(&slot) {
                    crate::expr::Unit::MM => {
                        crate::expr::Unit::of_length(params.context().default_length_unit)
                    }
                    unit => unit,
                };
                let value = measured
                    .and_then(|s| s.driven.get(&d.id).copied())
                    .unwrap_or(0.0);
                let comment = format!("{owner_name} {}{DRIVEN_SUFFIX}", d.kind.type_name());
                let id = params
                    .add_reference(value, unit, &comment, owner)
                    .map_err(|e| format!("{slot}: {e}"))?;
                created.push(params.name(id));
                id
            }
        };
        taken.insert(id);
        d.reference = Some(id);
        if let Some(value) = measured.and_then(|s| s.driven.get(&d.id)) {
            let differs = params
                .value(id)
                .is_none_or(|v| v.to_bits() != value.to_bits());
            if differs {
                params
                    .set_measured(id, *value)
                    .map_err(|e| format!("{slot}: {e}"))?;
            }
        }
    }
    Ok(created)
}

/// Makes a user parameter a sketch's reference (mitcad#250): owned by the
/// sketch, read-only, its expression its value as a number until the sketch
/// measures it; a parameter without a comment gets the reference's.
pub(crate) fn adopt_reference(
    params: &mut Parameters,
    id: ParamId,
    owner: FeatureUid,
    comment: &str,
) -> Result<(), crate::parameters::ParameterError> {
    let value = params.value(id).unwrap_or(0.0);
    params.set_owner(id, Some(owner));
    params.set_kind(id, crate::expr::ParamKind::Reference);
    params.set_measured(id, value)?;
    if params.get(id).is_some_and(|p| p.comment().is_empty()) {
        params.set_comment(id, comment)?;
    }
    Ok(())
}
