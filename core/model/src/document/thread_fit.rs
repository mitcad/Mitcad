// SPDX-License-Identifier: MIT
//! A thread's face fitted to the thread (mitcad#1632): a hole too small
//! for an internal thread (or a shaft too large for an external one) takes
//! the thread's crest diameter by a change of what sets its diameter, the
//! value or dimension the thread's evaluation found
//! ([`crate::features::thread::fit`]). Auto Fix does it for a warned
//! thread; `add_feature` with `fit_faces` (the Thread panel's *Fit the
//! hole to the thread*) does it with the thread, in one undo step.
//!
//! - **The expression keeps its form.** One number of the expression is
//!   solved for, the one that carries the value (`72/2` becomes
//!   `73.835/2`), in its own unit (`2.9 in` stays inches); an expression
//!   no number of which can give the value becomes the value.
//! - **A named parameter** the expression uses: when nothing else uses it,
//!   it is changed (its own expression keeping its form). When other
//!   places use it the fix is not silent: [`SharedChoice::Parameter`]
//!   changes it (the result says in how many other places it is used),
//!   [`SharedChoice::Own`] gives the dimension its own value in its place;
//!   without a choice the fix is not taken and says both.
//! - **Checked before it is taken:** a fix that would make a feature that
//!   built fail, or after which the face still does not fit, is not taken,
//!   with the reason.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Added, DocState, Document, ModelError, PreviewReport, invalid};
use crate::expr::{
    Expr, NodeKind, Span, Unit, format_number, format_value, replace_references,
    value_to_expression, with_decimal_points,
};
use crate::features::thread::ThreadFitMiss;
use crate::features::{FeatureDef, ValueInput};
use crate::ids::{ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::{ParamId, Parameters};
use crate::recompute::{FeatureStatus, Recomputed};

/// What a fit does with a named parameter that other places use too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedChoice {
    /// Change the parameter, and with it every place that uses it.
    Parameter,
    /// Give this dimension its own value; the parameter stays as it is.
    Own,
}

/// A face fitted to its thread: what was changed, from what to what.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoFixFit {
    /// The thread.
    pub thread: FeatureUid,
    pub thread_name: String,
    /// The face (its topological name, for agents).
    pub face: String,
    /// Its diameter before and the diameter it was given (mm).
    pub diameter: f64,
    pub needed: f64,
    pub internal: bool,
    /// The feature whose value or dimension set it (a Hole, a Cylinder, a
    /// sketch).
    pub source: FeatureUid,
    pub source_name: String,
    /// The parameter changed: the value's or the dimension's own, or the
    /// named parameter its expression uses.
    pub parameter: String,
    /// Whether that is a named parameter the dimension uses, not its own.
    pub named: bool,
    /// Whether the source is a sketch's dimension (else a Hole's or a
    /// Cylinder's diameter).
    pub sketch: bool,
    pub old: String,
    pub new: String,
    /// How many other places use the parameter changed (a named
    /// parameter changed with [`SharedChoice::Parameter`]); 0 otherwise.
    pub others: usize,
}

impl AutoFixFit {
    /// What it did, in words (no ids or names of faces, mitcad#1327):
    /// `the hole fitted to the thread: Sketch2's dimension 72/2 is now
    /// 73.835/2`.
    pub fn summary(&self) -> String {
        // (Whole messages, without a word as a value: docs/translations.md.)
        let changed = if self.named {
            match self.others {
                0 => format!("the parameter {} is now {}", self.parameter, self.new),
                1 => format!(
                    "the parameter {} is now {} (used in 1 other place)",
                    self.parameter, self.new
                ),
                n => format!(
                    "the parameter {} is now {} (used in {n} other places)",
                    self.parameter, self.new
                ),
            }
        } else if self.source_name.is_empty() {
            format!("its value {} is now {}", self.old, self.new)
        } else if self.sketch {
            format!(
                "{}'s dimension {} is now {}",
                self.source_name, self.old, self.new
            )
        } else {
            format!(
                "{}'s diameter {} is now {}",
                self.source_name, self.old, self.new
            )
        };
        if self.internal {
            format!("the hole fitted to the thread: {changed}")
        } else {
            format!("the shaft fitted to the thread: {changed}")
        }
    }
}

/// Why a face is not fitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoFit {
    /// Nothing Auto Fix can change sets its diameter (why).
    Source(String),
    /// A named parameter that other places use: the caller chooses.
    Choice { parameter: String, others: usize },
    /// It cannot be done (why).
    Refused(String),
}

impl NoFit {
    /// What to do, in words, for Auto Fix's `left`.
    pub fn advice(&self, internal: bool) -> String {
        match self {
            Self::Source(reason) if internal => format!("change the hole by hand ({reason})"),
            Self::Source(reason) => format!("change the shaft by hand ({reason})"),
            Self::Choice { parameter, others } if *others == 1 => format!(
                "the parameter {parameter} is used in 1 other place: choose to change the \
                 parameter or to give the dimension its own value"
            ),
            Self::Choice { parameter, others } => format!(
                "the parameter {parameter} is used in {others} other places: choose to change \
                 the parameter or to give the dimension its own value"
            ),
            Self::Refused(reason) => reason.clone(),
        }
    }
}

/// The parameter changes of a fit and what they do.
#[derive(Debug, Clone, PartialEq)]
pub struct FitEdits {
    /// Parameters with their new expressions.
    pub edits: Vec<(ParamId, String)>,
    /// A slot that names a shared parameter, given one of its own
    /// ([`SharedChoice::Own`]).
    pub own: Option<OwnSlot>,
    pub fit: AutoFixFit,
}

/// A feature's slot that names parameter `old` given a new parameter of
/// its own with `expression`.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnSlot {
    pub feature: FeatureUid,
    pub old: ParamId,
    pub expression: String,
    pub unit: Unit,
}

/// How close (mm, rad) a rewritten expression must come to its value: half
/// a micrometre, so that a thread's crest diameter is written as a person
/// would (M76x2's minor diameter 73.8349365 as `73.835`; the face's
/// diameter then lies far inside the fit check's allowance).
const CLOSE: f64 = 5e-4;

/// An expression's value (mm or rad) for a parameter in `unit`.
fn value_of(params: &Parameters, text: &str, unit: Unit) -> Option<f64> {
    params
        .table()
        .evaluate(text, unit)
        .ok()
        .map(|q| q.value)
        .filter(|v| v.is_finite())
}

/// The slot (a number of the expression, or a parameter it uses) that
/// gives `target`: `render(y)` is the expression with the slot at `y`,
/// `at` its value now. The slot's value that gives it and how much of the
/// expression's value it carries (|at × slope|); None when the expression
/// is not linear in it or the value would change sign.
fn solve_slot(
    params: &Parameters,
    unit: Unit,
    target: f64,
    at: f64,
    render: &dyn Fn(f64) -> String,
) -> Option<(f64, f64)> {
    let step = 1e-3 * at.abs().max(1.0);
    let f0 = value_of(params, &render(at), unit)?;
    let f1 = value_of(params, &render(at + step), unit)?;
    let f2 = value_of(params, &render(at + 2.0 * step), unit)?;
    let slope = (f1 - f0) / step;
    if slope.abs() < 1e-12 || ((f2 - f1) - (f1 - f0)).abs() > 1e-9 * (f1 - f0).abs().max(1e-12) {
        return None;
    }
    let y = at + (target - f0) / slope;
    // (A number keeps its sign: `-2` is a negation of 2.)
    if !y.is_finite() || (at > 0.0 && y <= 0.0) || (at < 0.0 && y >= 0.0) {
        return None;
    }
    Some((y, (at * slope).abs()))
}

/// The expression `render` makes of `y` with the fewest decimals (up to
/// 9) that still give `target`: `73.835/2`, not `73.83500000000001/2`.
fn rounded(
    params: &Parameters,
    unit: Unit,
    target: f64,
    y: f64,
    render: &dyn Fn(&str) -> String,
) -> Option<String> {
    (0..=9)
        .map(|digits| format_number(y, digits))
        .chain([format_number(y, 12)])
        .map(|number| render(&number))
        .find(|text| value_of(params, text, unit).is_some_and(|v| (v - target).abs() <= CLOSE))
}

/// `expression` (of a parameter in `unit`) with one of its numbers
/// changed so that it gives `target` (mm or rad), keeping its form: the
/// number that carries most of its value, in its own unit (`72/2` giving
/// 36.9175 is `73.835/2`). None when no number can.
pub fn solve_literal(
    params: &Parameters,
    expression: &str,
    unit: Unit,
    target: f64,
) -> Option<String> {
    let text = with_decimal_points(expression).into_owned();
    let expr = Expr::parse(&text).ok()?;
    let mut numbers: Vec<(Span, f64)> = Vec::new();
    expr.root().walk(&mut |node| {
        if let NodeKind::Number(x) = node.kind {
            numbers.push((node.span, x));
        }
    });
    let at =
        |span: Span, number: &str| format!("{}{number}{}", &text[..span.start], &text[span.end..]);
    // Each number with the value it needs and how much it carries: the
    // one that carries most, of two alike the larger.
    let mut best: Option<(Span, f64, f64, f64)> = None;
    for &(span, x) in &numbers {
        let render = |y: f64| at(span, &format_number(y, 12));
        let Some((y, carries)) = solve_slot(params, unit, target, x, &render) else {
            continue;
        };
        let better = best.is_none_or(|(_, old, _, carried)| {
            carries > carried + 1e-12 || ((carries - carried).abs() <= 1e-12 && x.abs() > old.abs())
        });
        if better {
            best = Some((span, x, y, carries));
        }
    }
    let (span, _, y, _) = best?;
    rounded(params, unit, target, y, &|number| at(span, number))
}

/// A value as an expression in `unit`, rounded as [`rounded`] does.
fn plain(params: &Parameters, unit: Unit, target: f64) -> String {
    (0..=9)
        .map(|digits| format_value(target, unit, digits))
        .find(|text| value_of(params, text, unit).is_some_and(|v| (v - target).abs() <= CLOSE))
        .unwrap_or_else(|| value_to_expression(target, unit))
}

/// `expression` rewritten to give `target`: a number of it solved for, or
/// the plain value.
fn rewritten(params: &Parameters, expression: &str, unit: Unit, target: f64) -> String {
    solve_literal(params, expression, unit, target).unwrap_or_else(|| plain(params, unit, target))
}

/// How many places use parameter `id`: the slots of the features'
/// definitions that name it and the parameters whose expressions do.
fn uses(state: &DocState, id: ParamId) -> usize {
    let slots: usize = state
        .features
        .iter()
        .map(|f| f.def.params().iter().filter(|p| **p == id).count())
        .sum();
    slots + state.parameters.table().dependents(id).len()
}

/// The changes that fit `miss`'s face of `thread` (named `thread_name`)
/// in `state`, with `choice` for a named parameter used elsewhere.
pub fn plan_fit(
    state: &DocState,
    thread: FeatureUid,
    thread_name: &str,
    miss: &ThreadFitMiss,
    choice: Option<SharedChoice>,
) -> Result<FitEdits, NoFit> {
    let Some(source) = &miss.source else {
        return Err(NoFit::Source(
            miss.reason
                .clone()
                .unwrap_or_else(|| "nothing sets its diameter".to_owned()),
        ));
    };
    let params = &state.parameters;
    let slot = source.param();
    let Some(own) = params.get(slot) else {
        return Err(NoFit::Refused("its dimension is gone".to_owned()));
    };
    if params.is_reference(slot) {
        return Err(NoFit::Refused("its dimension is a driven one".to_owned()));
    }
    let source_entry = state.entry(source.feature);
    if let Some(entry) = source_entry
        && let Err(error) = state.editable(entry.component)
    {
        return Err(NoFit::Refused(error.to_string()));
    }
    let source_name = source_entry.map(|e| e.name.clone()).unwrap_or_default();
    let sketch = source_entry.is_some_and(|e| matches!(e.def, FeatureDef::Sketch(_)));
    let unit = own.unit();
    let target = source.value;
    let fit =
        |parameter: String, named: bool, old: String, new: String, others: usize| AutoFixFit {
            thread,
            thread_name: thread_name.to_owned(),
            face: miss.face.to_string(),
            diameter: miss.diameter,
            needed: miss.needed,
            internal: miss.internal,
            source: source.feature,
            source_name: source_name.clone(),
            parameter,
            named,
            old,
            new,
            others,
            sketch,
        };
    // The slot names a parameter of its own (`bore` typed as the
    // dimension's value): that parameter is the named one, its value the
    // one wanted.
    if params.owner(slot) != Some(source.feature) {
        let others = uses(state, slot).saturating_sub(1);
        return match (others, choice) {
            (0, _) | (_, Some(SharedChoice::Parameter)) => {
                let new = rewritten(params, own.expression(), unit, target);
                Ok(FitEdits {
                    edits: vec![(slot, new.clone())],
                    own: None,
                    fit: fit(
                        own.name().to_owned(),
                        true,
                        own.expression().to_owned(),
                        new,
                        others,
                    ),
                })
            }
            (_, Some(SharedChoice::Own)) => {
                let new = plain(params, unit, target);
                Ok(FitEdits {
                    edits: Vec::new(),
                    own: Some(OwnSlot {
                        feature: source.feature,
                        old: slot,
                        expression: new.clone(),
                        unit,
                    }),
                    fit: fit(own.name().to_owned(), false, own.name().to_owned(), new, 0),
                })
            }
            (_, None) => Err(NoFit::Choice {
                parameter: own.name().to_owned(),
                others,
            }),
        };
    }
    let text = with_decimal_points(own.expression()).into_owned();
    let expr = Expr::parse(&text).map_err(|e| NoFit::Refused(e.to_string()))?;
    // The named parameter the value comes from, if any: the one that
    // carries most of it.
    let mut best: Option<(ParamId, f64, f64)> = None;
    for name in expr.references() {
        let Some(id) = params.find(name) else {
            continue;
        };
        let Some(param) = params.get(id) else {
            continue;
        };
        let Some(at) = params.value(id) else {
            continue;
        };
        let render = |y: f64| {
            replace_references(
                &text,
                &expr,
                name,
                &format!("({})", value_to_expression(y, param.unit())),
            )
        };
        if let Some((y, carries)) = solve_slot(params, unit, target, at, &render)
            && best.is_none_or(|(_, _, carried)| carries > carried)
        {
            best = Some((id, y, carries));
        }
    }
    let Some((named, value, _)) = best else {
        // Numbers only.
        let new = rewritten(params, &text, unit, target);
        return Ok(FitEdits {
            edits: vec![(slot, new.clone())],
            own: None,
            fit: fit(
                own.name().to_owned(),
                false,
                own.expression().to_owned(),
                new,
                0,
            ),
        });
    };
    let param = params.get(named).expect("found");
    // (The dimension's own expression is one of its uses.)
    let others = uses(state, named).saturating_sub(1);
    match (others, choice) {
        (0, _) | (_, Some(SharedChoice::Parameter)) => {
            if params.is_reference(named) {
                return Err(NoFit::Refused(format!(
                    "the parameter {} is a driven dimension's",
                    param.name()
                )));
            }
            let new = rewritten(params, param.expression(), param.unit(), value);
            Ok(FitEdits {
                edits: vec![(named, new.clone())],
                own: None,
                fit: fit(
                    param.name().to_owned(),
                    true,
                    param.expression().to_owned(),
                    new,
                    others,
                ),
            })
        }
        (_, Some(SharedChoice::Own)) => {
            // The parameter's value in its place, in the parameter's unit.
            let new = if matches!(expr.root().kind, NodeKind::Reference(_)) {
                plain(params, unit, target)
            } else {
                let number = plain(params, param.unit(), value);
                replace_references(&text, &expr, param.name(), &format!("({number})"))
            };
            if value_of(params, &new, unit).is_none_or(|v| (v - target).abs() > 1e-6) {
                return Err(NoFit::Refused(
                    "the dimension's own value does not give the diameter".to_owned(),
                ));
            }
            Ok(FitEdits {
                edits: vec![(slot, new.clone())],
                own: None,
                fit: fit(
                    own.name().to_owned(),
                    false,
                    own.expression().to_owned(),
                    new,
                    0,
                ),
            })
        }
        (_, None) => Err(NoFit::Choice {
            parameter: param.name().to_owned(),
            others,
        }),
    }
}

/// Writes a fit's changes into `state`: the parameters' new expressions,
/// and a slot that named a shared parameter given a parameter of its own.
/// The position of a feature whose definition changed, if any.
pub(crate) fn apply_fit(state: &mut DocState, plan: &FitEdits) -> Result<Option<usize>, String> {
    if !plan.edits.is_empty() {
        let edits: Vec<(ParamId, &str)> = plan
            .edits
            .iter()
            .map(|(id, text)| (*id, text.as_str()))
            .collect();
        state
            .parameters
            .set_expressions(&edits)
            .map_err(|e| e.to_string())?;
    }
    let Some(own) = &plan.own else {
        return Ok(None);
    };
    let position = state
        .features
        .iter()
        .position(|f| f.uid == own.feature)
        .ok_or_else(|| "the feature of the dimension is gone".to_owned())?;
    let entry = state.features[position].clone();
    // Its slot's name ends the new parameter's comment, as a dimension's
    // own does (`Sketch2 point_line_distance`).
    let mut what = String::new();
    let _ = entry.def.map_params(&mut |name, p: &ParamId| {
        if *p == own.old && what.is_empty() {
            what = name.rsplit('.').next().unwrap_or(name).to_owned();
        }
        Ok::<_, ()>(*p)
    });
    let comment = format!("{} {what}", entry.name);
    let id = state
        .parameters
        .add_dimension(&own.expression, own.unit, &comment, Some(own.feature))
        .map_err(|e| e.to_string())?;
    let def = entry
        .def
        .map_params(&mut |_, p: &ParamId| Ok::<_, ()>(if *p == own.old { id } else { *p }))
        .map_err(|()| "the definition could not be changed".to_owned())?;
    std::sync::Arc::make_mut(&mut state.features[position]).def = def;
    Ok(Some(position))
}

/// The misses of `uid`'s result in `result`: its faces that do not fit.
pub(crate) fn misses_in<S>(result: &Recomputed<S>, uid: FeatureUid) -> Vec<ThreadFitMiss> {
    result
        .result(uid)
        .filter(|r| r.status.is_ok())
        .and_then(|r| r.output.as_deref())
        .map(|o| o.thread_fits.clone())
        .unwrap_or_default()
}

/// What a fit tried left: the face and why.
pub type FitLeft = Vec<(String, NoFit)>;

impl<K: Kernel> Document<K> {
    /// The faces of thread `uid` that do not fit it, as last computed,
    /// with what sets each (mitcad#1632).
    pub fn thread_fits(&self, uid: FeatureUid) -> Vec<ThreadFitMiss> {
        misses_in(&self.result, uid)
    }

    /// The previewed thread's faces that do not fit it (the Thread panel's
    /// note, mitcad#1632).
    pub fn preview_thread_fits(&self) -> Vec<ThreadFitMiss> {
        self.preview
            .as_ref()
            .map(|preview| misses_in(&preview.result, preview.uid))
            .unwrap_or_default()
    }

    /// What fitting `miss` would ask (a named parameter used elsewhere) or
    /// why it cannot be done; None when it can be done as it is.
    pub fn fit_question(&self, miss: &ThreadFitMiss) -> Option<NoFit> {
        plan_fit(&self.state, FeatureUid(0), "", miss, None).err()
    }

    /// `state` with thread `uid`'s faces fitted to it, each checked as
    /// Auto Fix checks its fixes (the features that built still build, the
    /// face fits after); the fits made and what was left.
    pub(crate) fn fitted(
        &mut self,
        mut state: DocState,
        uid: FeatureUid,
        choice: Option<SharedChoice>,
    ) -> Result<(DocState, Vec<AutoFixFit>, FitLeft), ModelError> {
        let name = state
            .entry(uid)
            .map(|e| e.name.clone())
            .ok_or_else(|| invalid(format!("feature {uid} does not exist")))?;
        let mut result = self.compute_after(&state)?;
        let built: BTreeSet<FeatureUid> = result
            .results
            .iter()
            .filter(|r| r.status.is_ok())
            .map(|r| r.uid)
            .collect();
        let mut fits = Vec::new();
        let mut left: FitLeft = Vec::new();
        let mut tried: BTreeSet<String> = BTreeSet::new();
        loop {
            let misses = misses_in(&result, uid);
            let Some(miss) = misses
                .into_iter()
                .find(|m| !tried.contains(&m.face.to_string()))
            else {
                break;
            };
            let face = miss.face.to_string();
            tried.insert(face.clone());
            let plan = match plan_fit(&state, uid, &name, &miss, choice) {
                Ok(plan) => plan,
                Err(why) => {
                    left.push((face, why));
                    continue;
                }
            };
            match self.try_fit(&state, &plan, uid, &face, &built)? {
                Ok((candidate, computed)) => {
                    state = candidate;
                    result = computed;
                    fits.push(plan.fit);
                }
                Err(why) => left.push((face, NoFit::Refused(why))),
            }
        }
        Ok((state, fits, left))
    }

    /// `state` with `plan`'s changes, computed: refused (why) when a
    /// feature of `built` fails or `face` of thread `uid` still does not
    /// fit.
    #[allow(clippy::type_complexity)]
    pub(crate) fn try_fit(
        &mut self,
        state: &DocState,
        plan: &FitEdits,
        uid: FeatureUid,
        face: &str,
        built: &BTreeSet<FeatureUid>,
    ) -> Result<Result<(DocState, Recomputed<K::Shape>), String>, ModelError> {
        let mut candidate = state.clone();
        if let Err(error) = apply_fit(&mut candidate, plan) {
            return Ok(Err(error));
        }
        let computed = self.compute_after(&candidate)?;
        if let Some(broken) = computed
            .results
            .iter()
            .find(|r| built.contains(&r.uid) && matches!(r.status, FeatureStatus::Failed(_)))
        {
            let name = candidate
                .entry(broken.uid)
                .map_or_else(|| broken.uid.to_string(), |e| e.name.clone());
            return Ok(Err(format!("fixing it would make {name} fail")));
        }
        if misses_in(&computed, uid)
            .iter()
            .any(|m| m.face.to_string() == face)
        {
            return Ok(Err(
                "the face still does not fit after the change".to_owned()
            ));
        }
        Ok(Ok((candidate, computed)))
    }

    /// Adds a thread with its faces fitted to it (mitcad#1632: the Thread
    /// panel's *Fit the hole to the thread*), in one undo step `Add
    /// Thread1`: the fits made and what was left.
    pub fn add_feature_fitted(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
        component: Option<ComponentUid>,
        choice: Option<SharedChoice>,
    ) -> Result<(Added, Vec<AutoFixFit>, FitLeft), ModelError> {
        if !matches!(def, FeatureDef::Thread(_)) {
            return Err(invalid("fit_faces is for threads"));
        }
        let component = component.unwrap_or(self.state.assembly.active);
        self.state.editable(component)?;
        let beside = self.component_parent_beside(component);
        let mut state = self.state.clone();
        let added = state.add_feature_in(def, name, component)?;
        state.place_made_beside(added.uid, None, beside);
        let (state, fits, left) = self.fitted(state, added.uid, choice)?;
        let label = format!("Add {}", added.name);
        self.apply(|current| {
            *current = state;
            Ok((label, (added, fits, left)))
        })
    }

    /// [`Document::preview_add`] of a thread with its faces fitted to it,
    /// as [`Document::add_feature_fitted`] adds it: what moves with the
    /// faces is shown.
    pub fn preview_add_fitted(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
        choice: Option<SharedChoice>,
    ) -> Result<(PreviewReport, Vec<AutoFixFit>, FitLeft), ModelError> {
        if !matches!(def, FeatureDef::Thread(_)) {
            return Err(invalid("fit_faces is for threads"));
        }
        let beside = self.component_parent_beside(self.state.assembly.active);
        let mut state = self.state.clone();
        let added = state.add_feature(def, name)?;
        state.place_made_beside(added.uid, None, beside);
        let (state, fits, left) = self.fitted(state, added.uid, choice)?;
        let report = self.preview_state(state, added.uid)?;
        Ok((report, fits, left))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameters::default_context;

    fn params() -> Parameters {
        Parameters::new(default_context())
    }

    fn mm() -> Unit {
        Unit::MM
    }

    #[test]
    fn the_number_that_carries_the_value_is_solved_for() {
        let p = params();
        assert_eq!(
            solve_literal(&p, "72/2", mm(), 73.835 / 2.0).as_deref(),
            Some("73.835/2")
        );
        assert_eq!(
            solve_literal(&p, "3 mm", mm(), 3.5).as_deref(),
            Some("3.5 mm")
        );
        assert_eq!(
            solve_literal(&p, "2*36", mm(), 2.0 * 36.9175).as_deref(),
            Some("2*36.9175")
        );
        // In its own unit.
        assert_eq!(
            solve_literal(&p, "1 in", mm(), 1.25 * 25.4).as_deref(),
            Some("1.25 in")
        );
        // Decimal commas are written as points.
        assert_eq!(
            solve_literal(&p, "8,5", mm(), 8.376).as_deref(),
            Some("8.376")
        );
        // Nothing to solve for.
        assert_eq!(solve_literal(&p, "pi", mm(), 4.0), None);
    }

    #[test]
    fn a_plain_value_is_rounded_as_far_as_it_stays_exact() {
        let p = params();
        assert_eq!(plain(&p, mm(), 73.835), "73.835 mm");
        assert_eq!(rewritten(&p, "pi", mm(), 4.0), "4 mm");
    }
}
