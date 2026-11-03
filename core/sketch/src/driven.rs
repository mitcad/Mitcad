// SPDX-License-Identifier: MIT
//! Driven dimensions that size or place other geometry of their own sketch
//! (mitcad#1116).
//!
//! A driving dimension's value may use the reference parameter of a driven
//! dimension of the same sketch (`(d1 - d2) / 2`) when that value cannot
//! move what the driven dimension measures: the measured value is then
//! known before the dimensions that use it are solved. The sketch is solved
//! in stages ([`solve_staged`]):
//!
//! 1. with every driving dimension that uses such a reference (a *user*)
//!    left out: the driven dimensions none of the users can move measure
//!    their final values here;
//! 2. with the users whose references are all known put back, at the values
//!    those references give: the driven dimensions only they could move
//!    measure; and so on, until
//! 3. the whole sketch solves with every reference's value.
//!
//! What a user can move ([`SketchDef::driven_influence`]) is the geometry
//! connected to its own entities through active constraints, driving
//! dimensions, curves' points, patterns and offsets, leaving out the
//! entities the first stage already fixes (fully constrained or fixed):
//! geometry that is fixed without the users stays where it is when they
//! are put back, so it carries no influence. A connection is not always an
//! influence, so this errs on the side of a refusal: a refused case is
//! accepted once the measured geometry is fully constrained without its
//! users, or is not connected to theirs. A driven dimension that one of
//! its users can move, directly or through other driven dimensions and
//! their users, is a circular dependency ([`driven_cycle`]), which the
//! model refuses with the dimensions named.

use std::collections::{BTreeMap, BTreeSet};

use crate::def::SketchDef;
use crate::ids::EntityUid;
use crate::parameters::ParamId;
use crate::pattern::PatternKind;
use crate::solve::{SolveError, Solved};
use crate::{ConstraintUid, Dimension, DimensionKind, EntityIndex, Ref};

/// For each driving dimension whose value uses reference parameters of
/// driven dimensions of the same sketch (directly or through other
/// parameters), those driven dimensions.
pub type DrivenUses = BTreeMap<ConstraintUid, BTreeSet<ConstraintUid>>;

/// A circular dependency: steps `(driven, user)`, where `user` can move
/// what `driven` measures and uses the next step's driven dimension (the
/// last step's user uses the first one's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrivenCycle(pub Vec<(ConstraintUid, ConstraintUid)>);

/// How far a driven value may move between the stage that measured it and
/// the whole solve (mm, rad, relative above 1): solver noise, not an
/// influence.
const SAME: f64 = 1e-6;

impl<P: Clone> SketchDef<P> {
    /// This definition with the dimensions `held` left out of the solve
    /// (inactive).
    pub fn holding(&self, held: &BTreeSet<ConstraintUid>) -> SketchDef<P> {
        let mut def = self.clone();
        for d in def.dimensions.iter_mut().filter(|d| held.contains(&d.id)) {
            d.active = false;
        }
        def
    }
}

impl<P> SketchDef<P> {
    /// The entities a stage's solve fixes: fully constrained or fixed.
    pub fn settled(&self, solved: &Solved) -> BTreeSet<EntityUid> {
        let mut settled = solved.status.fully_constrained.clone();
        settled.extend(self.entities.iter().filter(|e| e.fixed).map(|e| e.id));
        settled
    }

    /// The points and curves a dimension refers to (an equal spacing's
    /// dimension: its constraint's).
    fn dimension_refs(&self, d: &Dimension<P>) -> Vec<Ref> {
        match d.kind {
            DimensionKind::Spacing { constraint } => self
                .constraints
                .iter()
                .find(|c| c.id == constraint)
                .map(|c| c.kind.refs())
                .unwrap_or_default(),
            _ => d.kind.refs(),
        }
    }

    /// For each driven dimension `uses` names, the users that can move what
    /// it measures; `settled` are the entities the solve without the users
    /// fixes ([`SketchDef::settled`]). See the module comment.
    pub fn driven_influence(
        &self,
        uses: &DrivenUses,
        settled: &BTreeSet<EntityUid>,
    ) -> BTreeMap<ConstraintUid, BTreeSet<ConstraintUid>> {
        let index = EntityIndex::new(&self.entities);
        // The unsettled entities of some references, curves with their
        // points.
        let free = |refs: Vec<Ref>| -> Vec<EntityUid> {
            let mut ids = Vec::new();
            for r in refs {
                ids.push(r.uid());
                if let Ref::Curve(c) = r
                    && let Some(e) = index.get(c)
                {
                    ids.extend(e.kind.points());
                }
            }
            ids.retain(|e| !settled.contains(e));
            ids
        };
        let unsettled = |mut ids: Vec<EntityUid>| {
            ids.retain(|e| !settled.contains(e));
            ids
        };
        let mut groups = Groups::default();
        // A curve moves with its points.
        for e in self.entities.iter().filter(|e| !e.is_point()) {
            let mut ids = vec![e.id];
            ids.extend(e.kind.points());
            groups.join(&unsettled(ids));
        }
        for c in self.constraints.iter().filter(|c| c.active) {
            groups.join(&free(c.kind.refs()));
        }
        let applied = |d: &Dimension<P>| d.value.is_some() && self.dimension_applied(d);
        for d in self.dimensions.iter().filter(|d| applied(d)) {
            groups.join(&free(self.dimension_refs(d)));
        }
        // Copies move with their originals, offsets with their sources.
        for p in &self.patterns {
            let mut ids: Vec<EntityUid> = p.entities.iter().map(|r| r.uid()).collect();
            for copy in &p.copies {
                ids.extend(copy.entities.iter().flat_map(|(a, b)| [a.uid(), b.uid()]));
            }
            if let PatternKind::Circular { center, .. } = &p.kind {
                ids.push(*center);
            }
            groups.join(&unsettled(ids));
        }
        for o in &self.offsets {
            let mut ids = o.curves.clone();
            ids.extend(o.made());
            ids.extend(&o.helpers);
            groups.join(&unsettled(ids));
        }
        // What each user moves: the groups of its unsettled entities (an
        // inactive one moves nothing).
        let mut moves: BTreeMap<ConstraintUid, BTreeSet<EntityUid>> = BTreeMap::new();
        for d in self.dimensions.iter().filter(|d| uses.contains_key(&d.id)) {
            let ids = if applied(d) {
                free(self.dimension_refs(d))
            } else {
                Vec::new()
            };
            moves.insert(d.id, ids.into_iter().map(|e| groups.find(e)).collect());
        }
        let fed: BTreeSet<ConstraintUid> = uses.values().flatten().copied().collect();
        let mut out = BTreeMap::new();
        for d in self.dimensions.iter().filter(|d| fed.contains(&d.id)) {
            let measured: BTreeSet<EntityUid> = free(self.dimension_refs(d))
                .into_iter()
                .map(|e| groups.find(e))
                .collect();
            let by = moves
                .iter()
                .filter(|(_, moved)| !moved.is_disjoint(&measured))
                .map(|(user, _)| *user)
                .collect();
            out.insert(d.id, by);
        }
        out
    }
}

/// Entities joined into groups that move together.
#[derive(Default)]
struct Groups(BTreeMap<EntityUid, EntityUid>);

impl Groups {
    fn find(&mut self, e: EntityUid) -> EntityUid {
        let mut root = e;
        while let Some(&up) = self.0.get(&root) {
            if up == root {
                break;
            }
            root = up;
        }
        let mut at = e;
        while at != root {
            let up = self.0.get(&at).copied().unwrap_or(root);
            self.0.insert(at, root);
            at = up;
        }
        root
    }

    fn join(&mut self, ids: &[EntityUid]) {
        let mut first = None;
        for id in ids {
            let root = self.find(*id);
            match first {
                None => first = Some(root),
                Some(f) if f != root => {
                    self.0.insert(root, f);
                }
                Some(_) => {}
            }
        }
    }
}

/// A circular dependency among the driven dimensions `uses` names, by
/// `influence` ([`SketchDef::driven_influence`]); None when they can be
/// measured one after the other.
pub fn driven_cycle(
    uses: &DrivenUses,
    influence: &BTreeMap<ConstraintUid, BTreeSet<ConstraintUid>>,
) -> Option<DrivenCycle> {
    struct Search<'a> {
        uses: &'a DrivenUses,
        influence: &'a BTreeMap<ConstraintUid, BTreeSet<ConstraintUid>>,
        /// Driven dimensions on the path (false) or done (true).
        seen: BTreeMap<ConstraintUid, bool>,
        path: Vec<(ConstraintUid, ConstraintUid)>,
    }
    impl Search<'_> {
        fn visit(&mut self, driven: ConstraintUid) -> Option<DrivenCycle> {
            self.seen.insert(driven, false);
            let users: Vec<ConstraintUid> = self
                .influence
                .get(&driven)
                .into_iter()
                .flatten()
                .copied()
                .collect();
            for user in users {
                let next: Vec<ConstraintUid> = self
                    .uses
                    .get(&user)
                    .into_iter()
                    .flatten()
                    .copied()
                    .collect();
                for next in next {
                    self.path.push((driven, user));
                    match self.seen.get(&next) {
                        Some(false) => {
                            let start = self.path.iter().position(|(d, _)| *d == next)?;
                            return Some(DrivenCycle(self.path[start..].to_vec()));
                        }
                        Some(true) => {}
                        None => {
                            if let Some(cycle) = self.visit(next) {
                                return Some(cycle);
                            }
                        }
                    }
                    self.path.pop();
                }
            }
            self.seen.insert(driven, true);
            None
        }
    }
    let mut search = Search {
        uses,
        influence,
        seen: BTreeMap::new(),
        path: Vec::new(),
    };
    for driven in influence.keys() {
        if !search.seen.contains_key(driven)
            && let Some(cycle) = search.visit(*driven)
        {
            return Some(cycle);
        }
    }
    None
}

impl DrivenCycle {
    /// The refusal, naming the dimensions and their parameters
    /// (`k6 (d2) uses d3 and can move what the driven dimension k7 (d3)
    /// measures, so d3 would depend on itself. …`).
    pub fn describe(&self, def: &SketchDef, name: &dyn Fn(ParamId) -> String) -> String {
        let dimension = |id: ConstraintUid| def.dimensions.iter().find(|d| d.id == id);
        let reference = |id: ConstraintUid| {
            dimension(id)
                .and_then(|d| d.reference)
                .map_or_else(|| id.to_string(), name)
        };
        let value = |id: ConstraintUid| {
            dimension(id)
                .and_then(|d| d.value)
                .map_or_else(|| id.to_string(), name)
        };
        let n = self.0.len();
        let steps: Vec<String> = (0..n)
            .map(|i| {
                let (driven, user) = self.0[i];
                let next = self.0[(i + 1) % n].0;
                format!(
                    "{user} ({}) uses {} and can move what the driven dimension {driven} ({}) \
                     measures",
                    value(user),
                    reference(next),
                    reference(driven)
                )
            })
            .collect();
        format!(
            "{}, so {} would depend on itself. A driving dimension can use a driven \
             dimension of its own sketch only when it cannot move what that one measures: \
             constrain that geometry fully without it, or measure geometry it does not move",
            steps.join("; "),
            reference(self.0.first().map_or(ConstraintUid(0), |s| s.0))
        )
    }
}

/// Refuses a circular dependency among the driven dimensions of `def` that
/// its own driving dimensions use (`uses`), with [`DrivenCycle::describe`];
/// `value` and `name` as for [`SketchDef::solve_with`]. A sketch that does
/// not solve without the users is not refused here (its recompute tells
/// what is wrong).
pub fn check_uses(
    def: &SketchDef,
    uses: &DrivenUses,
    value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    name: &dyn Fn(ParamId) -> String,
) -> Result<(), String> {
    if uses.is_empty() {
        return Ok(());
    }
    let held = uses.keys().copied().collect();
    let Ok(first) = def.holding(&held).solve_with(value, name) else {
        return Ok(());
    };
    let influence = def.driven_influence(uses, &def.settled(&first));
    match driven_cycle(uses, &influence) {
        Some(cycle) => Err(cycle.describe(def, name)),
        None => Ok(()),
    }
}

/// One stage's solve for [`solve_staged`]: the dimensions held out and the
/// driven values known so far.
pub type StageSolve<'a> = dyn FnMut(&BTreeSet<ConstraintUid>, &BTreeMap<ConstraintUid, f64>) -> Result<Solved, SolveError>
    + 'a;

/// Solves `def`, whose driving dimensions use its own driven ones (`uses`),
/// in stages (see the module comment). `solve(held, known)` solves the
/// definition with the dimensions `held` left out
/// ([`SketchDef::holding`]) and the reference of each driven dimension in
/// `known` at that value; its last call is the whole sketch with every
/// value known. The used driven dimensions report the values their users
/// were solved with. A circular dependency, or a driven value the whole
/// solve moves after all, fails with what it is.
pub fn solve_staged(
    def: &SketchDef,
    uses: &DrivenUses,
    name: &dyn Fn(ParamId) -> String,
    solve: &mut StageSolve<'_>,
) -> Result<Solved, SolveError> {
    let none = BTreeMap::new();
    let mut held: BTreeSet<ConstraintUid> = uses.keys().copied().collect();
    let first = match solve(&held, &none) {
        Ok(solved) => solved,
        // What is wrong with the sketch, as its whole solve says it.
        Err(error) => return Err(solve(&BTreeSet::new(), &none).err().unwrap_or(error)),
    };
    let influence = def.driven_influence(uses, &def.settled(&first));
    if let Some(cycle) = driven_cycle(uses, &influence) {
        return Err(SolveError::new(cycle.describe(def, name)));
    }
    let fed: BTreeSet<ConstraintUid> = uses.values().flatten().copied().collect();
    let mut known: BTreeMap<ConstraintUid, f64> = BTreeMap::new();
    let mut solved = first;
    loop {
        // What this stage measured for good: the driven dimensions none of
        // the users still left out can move.
        for d in &fed {
            if !known.contains_key(d)
                && influence.get(d).is_none_or(|by| by.is_disjoint(&held))
                && let Some(value) = solved.status.driven.get(d)
            {
                known.insert(*d, *value);
            }
        }
        if held.is_empty() {
            break;
        }
        let next: BTreeSet<ConstraintUid> = held
            .iter()
            .filter(|user| {
                uses.get(*user)
                    .is_some_and(|ds| ds.iter().any(|d| !known.contains_key(d)))
            })
            .copied()
            .collect();
        if next.len() == held.len() {
            // Without a cycle, only a driven dimension that measures
            // nothing stops the stages.
            let missing = fed
                .iter()
                .find(|d| !known.contains_key(d))
                .copied()
                .unwrap_or(ConstraintUid(0));
            let reference = def
                .dimensions
                .iter()
                .find(|d| d.id == missing)
                .and_then(|d| d.reference)
                .map_or_else(|| missing.to_string(), name);
            return Err(SolveError::new(format!(
                "the driven dimension {missing} ({reference}) measures nothing, so the \
                 dimensions that use it have no value"
            )));
        }
        held = next;
        solved = solve(&held, &known)?;
    }
    // The whole solve must measure what the users were solved with.
    for (d, value) in &known {
        let Some(now) = solved.status.driven.get(d).copied() else {
            continue;
        };
        if (now - value).abs() > SAME * (1.0 + now.abs().max(value.abs())) {
            let users: Vec<String> = uses
                .iter()
                .filter(|(_, ds)| ds.contains(d))
                .map(|(user, _)| user.to_string())
                .collect();
            let reference = def
                .dimensions
                .iter()
                .find(|x| x.id == *d)
                .and_then(|x| x.reference)
                .map_or_else(|| d.to_string(), name);
            return Err(SolveError::new(format!(
                "{} moved what the driven dimension {d} ({reference}) measures: {value} \
                 before, {now} after, so {reference} would depend on itself. Constrain that \
                 geometry fully without them",
                users.join(", ")
            )));
        }
    }
    for (d, value) in known {
        solved.status.driven.insert(d, value);
    }
    Ok(solved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(n: u32) -> ConstraintUid {
        ConstraintUid(n)
    }

    fn set(ids: &[u32]) -> BTreeSet<ConstraintUid> {
        ids.iter().map(|n| k(*n)).collect()
    }

    #[test]
    fn a_chain_of_driven_values_has_no_cycle() {
        // k10 uses k1, k11 uses k2; k10 moves what k2 measures.
        let uses = DrivenUses::from([(k(10), set(&[1])), (k(11), set(&[2]))]);
        let influence = BTreeMap::from([(k(1), set(&[])), (k(2), set(&[10]))]);
        assert_eq!(driven_cycle(&uses, &influence), None);
    }

    #[test]
    fn a_user_that_moves_what_it_uses_is_a_cycle() {
        let uses = DrivenUses::from([(k(10), set(&[1]))]);
        let influence = BTreeMap::from([(k(1), set(&[10]))]);
        assert_eq!(
            driven_cycle(&uses, &influence),
            Some(DrivenCycle(vec![(k(1), k(10))]))
        );
    }

    #[test]
    fn a_cycle_through_two_driven_dimensions_is_found() {
        // k10 uses k1 and moves k2; k11 uses k2 and moves k1.
        let uses = DrivenUses::from([(k(10), set(&[1])), (k(11), set(&[2]))]);
        let influence = BTreeMap::from([(k(1), set(&[11])), (k(2), set(&[10]))]);
        assert_eq!(
            driven_cycle(&uses, &influence),
            Some(DrivenCycle(vec![(k(1), k(11)), (k(2), k(10))]))
        );
    }
}
