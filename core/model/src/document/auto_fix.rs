// SPDX-License-Identifier: MIT
//! Auto Fix (mitcad#1223): a feature whose stored references are no longer
//! found exactly is computed with what the model found instead, and keeps
//! a warning naming both (a region `found as` another or as the sum of
//! several, mitcad#407; a face, edge or vertex a sketch change renamed,
//! mitcad#272). Auto Fix writes what was found into the definitions, so
//! that the result stays as it is and the warnings go, as one undo step.
//!
//! - What was found is read from the warnings themselves
//!   ([`Repair::parse`]): they name the stored reference and exactly what
//!   the computation used in its place, so the fix uses nothing else.
//! - A face, edge or vertex name is replaced wherever the definition holds
//!   it. A region reference in a list of profiles takes every region it was
//!   found as, at its place, each once (the profiles the computation
//!   used); a region reference elsewhere (a loft section, a sheet's panel
//!   slot) takes the main region, the one the computation used there.
//! - A fixed feature's faces made from a region take the region's new key
//!   (`F2:end(r{…})`): a later feature that named them finds them again,
//!   with a warning of its own, which the same Auto Fix fixes too ("also
//!   updated").
//! - Each feature's fix is computed before it is taken: one that would make
//!   a feature that built fail, or that leaves its warning, is not taken,
//!   and the warning stays with the reason. A reference that is gone or is
//!   ambiguous (the feature fails) cannot be fixed: it is reported with
//!   what to do ("pick it again").
//! - The whole Auto Fix is one undo step, `Auto Fix <feature>`, `Auto Fix
//!   <group>` for a timeline group's features, or `Auto Fix <n> Features`.
//! - A thread whose face does not fit it (mitcad#1632) has that face fitted
//!   by a change of the value or dimension that sets its diameter
//!   ([`super::thread_fit`]), checked the same way, after the references.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::Arc;

use serde_json::Value;

use super::thread_fit::{AutoFixFit, NoFit, SharedChoice, misses_in, plan_fit};
use super::{DocState, Document, ModelError};
use crate::features::{FeatureDef, FeatureEntry};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::recompute::{FeatureResult, FeatureStatus, Recomputed};
use crate::topo::{RegionKey, TopoName};

/// What a warning says the model found in place of a stored reference.
#[derive(Debug, Clone, PartialEq, Eq)]
// A few repairs per Auto Fix, never stored: the topological names stay
// inline.
#[allow(clippy::large_enum_variant)]
pub enum Repair {
    /// A face, edge or vertex found under another name (mitcad#272):
    /// `edge E{…} found as E{…} after a change in Sketch1`.
    Name { stored: TopoName, found: TopoName },
    /// A region of the sketch named `sketch` found as other regions, the
    /// main one first (mitcad#407): `region r{…} found as r{…} + r{…} after
    /// a change in Sketch1`.
    Region {
        sketch: String,
        stored: RegionKey,
        found: Vec<RegionKey>,
    },
}

const FOUND_AS: &str = " found as ";
const AFTER: &str = " after a change in ";

impl Repair {
    /// The repair a warning names; None for any other warning. A prefix
    /// before the reference (`section 2: `) is allowed.
    pub fn parse(warning: &str) -> Option<Self> {
        let (head, rest) = warning.split_once(FOUND_AS)?;
        let (found, sketch) = rest.split_once(AFTER)?;
        let mut words = head.rsplitn(3, ' ');
        let stored = words.next()?;
        match words.next()? {
            "region" => {
                let stored: RegionKey = stored.parse().ok()?;
                let found = found
                    .split(" + ")
                    .map(str::parse)
                    .collect::<Result<Vec<RegionKey>, _>>()
                    .ok()?;
                (!found.is_empty() && !found.contains(&stored)).then(|| Self::Region {
                    sketch: sketch.to_owned(),
                    stored,
                    found,
                })
            }
            "face" | "edge" | "vertex" => {
                let stored: TopoName = stored.parse().ok()?;
                let found: TopoName = found.parse().ok()?;
                (stored != found).then_some(Self::Name { stored, found })
            }
            _ => None,
        }
    }

    /// Writes the repair into a definition's JSON; false when the
    /// definition does not hold the stored reference.
    fn apply(&self, state: &DocState, def: &mut Value) -> bool {
        match self {
            Self::Name { stored, found } => rename(def, stored, &found.to_string()),
            Self::Region {
                sketch,
                stored,
                found,
            } => {
                let sketches: Vec<Value> = state
                    .features
                    .iter()
                    .filter(|f| f.name == *sketch && matches!(f.def, FeatureDef::Sketch(_)))
                    .filter_map(|f| serde_json::to_value(f.uid).ok())
                    .collect();
                let is_reference = |item: &Value| -> bool {
                    item.get("sketch").is_some_and(|s| sketches.contains(s))
                        && item
                            .get("region")
                            .and_then(Value::as_str)
                            .and_then(|r| r.parse::<RegionKey>().ok())
                            .is_some_and(|r| r == *stored)
                };
                regions(def, &is_reference, found)
            }
        }
    }
}

/// Every string of the JSON that names `stored` names `found` instead.
fn rename(value: &mut Value, stored: &TopoName, found: &str) -> bool {
    match value {
        Value::String(text) => {
            let same = text.parse::<TopoName>().is_ok_and(|name| name == *stored);
            if same {
                *text = found.to_owned();
            }
            same
        }
        Value::Array(items) => {
            // Every item is visited: each one holding the name is renamed.
            let mut changed = false;
            for item in items {
                changed |= rename(item, stored, found);
            }
            changed
        }
        Value::Object(map) => {
            let mut changed = false;
            for item in map.values_mut() {
                changed |= rename(item, stored, found);
            }
            changed
        }
        _ => false,
    }
}

/// Every region reference of the JSON (`is_reference`) takes the regions
/// it was found as: in a list of profiles all of them, elsewhere the main
/// one.
fn regions(value: &mut Value, is_reference: &dyn Fn(&Value) -> bool, found: &[RegionKey]) -> bool {
    if is_reference(value) {
        value["region"] = Value::String(found[0].to_string());
        return true;
    }
    match value {
        Value::Object(map) => {
            let mut changed = false;
            for (key, item) in map.iter_mut() {
                changed |= if key == "profiles" {
                    profiles(item, is_reference, found)
                } else {
                    regions(item, is_reference, found)
                };
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= regions(item, is_reference, found);
            }
            changed
        }
        _ => false,
    }
}

/// A list of profiles: a reference's place takes every region it was found
/// as, in the order found, each once; a region the list already has before
/// that place stays there, and one it has after moves up (the order the
/// computation used them in, so the bodies keep their numbers).
fn profiles(list: &mut Value, is_reference: &dyn Fn(&Value) -> bool, found: &[RegionKey]) -> bool {
    if !list.is_array() {
        return regions(list, is_reference, found);
    }
    let Value::Array(items) = list else {
        return false;
    };
    let mut changed = false;
    while let Some(at) = items.iter().position(is_reference) {
        let reference = items.remove(at);
        let mut place = at;
        for key in found {
            let mut piece = reference.clone();
            piece["region"] = Value::String(key.to_string());
            if items[..place].contains(&piece) {
                continue;
            }
            items.retain(|item| *item != piece);
            items.insert(place, piece);
            place += 1;
        }
        changed = true;
    }
    changed
}

/// The marker a parameter rides along as while a definition is JSON (as
/// Reorient Design's).
const PARAM: char = '\u{1}';

/// The definition with the repairs written into it, and which of them it
/// held; None when it held none (or has no references, a base feature).
fn rewrite(
    state: &DocState,
    entry: &FeatureEntry,
    repairs: &[Repair],
) -> Option<(FeatureDef, Vec<bool>)> {
    if matches!(entry.def, FeatureDef::Base(_)) {
        return None;
    }
    let mut slots = Vec::new();
    let marked = entry
        .def
        .map_params(&mut |_, id: &ParamId| {
            slots.push(*id);
            Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
        })
        .ok()?;
    let mut value = serde_json::to_value(&marked).ok()?;
    let applied: Vec<bool> = repairs
        .iter()
        .map(|repair| repair.apply(state, &mut value))
        .collect();
    if !applied.contains(&true) {
        return None;
    }
    let marked: FeatureDef<String> = serde_json::from_value(value).ok()?;
    let def = marked
        .map_params(&mut |_, marker: &String| {
            marker
                .strip_prefix(PARAM)
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(|i| slots.get(i).copied())
                .ok_or(())
        })
        .ok()?;
    Some((def, applied))
}

/// What a warning or an error says about a reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
    /// Found under another name: Auto Fix writes it.
    Renamed,
    /// Like several (`… is ambiguous after a sketch change: …`).
    Tie,
    /// Not there (`the body has no edge E{…}`, `… no longer exists …`,
    /// `Sketch1 has no profile r{…}`).
    Gone,
}

fn problem(message: &str) -> Option<Problem> {
    const GONE: [&str; 6] = [
        "has no face ",
        "has no edge ",
        "has no vertex ",
        "has no profile ",
        " no longer exists",
        " does not exist because ",
    ];
    if Repair::parse(message).is_some() {
        Some(Problem::Renamed)
    } else if message.contains(" is ambiguous after a sketch change") {
        Some(Problem::Tie)
    } else {
        GONE.iter()
            .any(|gone| message.contains(gone))
            .then_some(Problem::Gone)
    }
}

/// The warnings of a thread's faces that do not fit it (mitcad#1632).
fn fit_warnings<S>(result: &FeatureResult<S>) -> Vec<&str> {
    match &result.output {
        Some(output) if result.status.is_ok() => output
            .thread_fits
            .iter()
            .map(|m| m.warning.as_str())
            .collect(),
        _ => Vec::new(),
    }
}

/// The reference problems of a feature's result: its warnings about
/// references and about faces that do not fit a thread, and its error
/// when it is about a reference.
fn problems<S>(result: Option<&FeatureResult<S>>) -> Vec<String> {
    let Some(result) = result else {
        return Vec::new();
    };
    let fits = fit_warnings(result);
    let mut found: Vec<String> = result
        .warnings()
        .iter()
        .filter(|w| problem(w).is_some() || fits.contains(&w.as_str()))
        .cloned()
        .collect();
    if let FeatureStatus::Failed(error) = &result.status
        && problem(error).is_some()
    {
        found.push(error.agent_text().to_owned());
    }
    found
}

/// The result of the feature `uid` at `position` of a recompute.
fn result_at<S>(
    computed: &Recomputed<S>,
    position: usize,
    uid: FeatureUid,
) -> Option<&FeatureResult<S>> {
    computed
        .results
        .get(position)
        .filter(|r| r.uid == uid)
        .or_else(|| computed.result(uid))
}

/// A problem without its suggestions (mitcad#1187), as the summary names
/// it.
fn short(message: &str) -> String {
    message
        .split_once("; existing edges of body")
        .map_or(message, |(head, _)| head)
        .to_owned()
}

/// Why a warning was not fixed (a fix it refused), else what to do.
fn advice(message: &str, refused: Option<&String>) -> String {
    match problem(message) {
        Some(Problem::Tie) => "pick one of them again".to_owned(),
        Some(Problem::Gone) => "pick it again".to_owned(),
        _ => refused
            .cloned()
            .unwrap_or_else(|| "it was not written into the definition".to_owned()),
    }
}

/// Why a renamed reference was not written: the definition does not hold
/// it (it came from elsewhere, such as a sketch it refers to).
const NOT_HELD: &str = "the definition does not hold the reference";

/// A reference problem Auto Fix left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoFixLeft {
    pub feature: FeatureUid,
    /// The feature's name.
    pub name: String,
    /// The warning or the error (without suggestions of edges).
    pub message: String,
    /// What to do, or why it was not fixed.
    pub advice: String,
}

/// What an Auto Fix did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutoFix {
    /// The reference problems of the features asked for, before.
    pub warnings: usize,
    /// Those of them gone after.
    pub fixed: usize,
    /// The features whose definitions changed, in timeline order.
    pub changed: Vec<FeatureUid>,
    /// Those of them not asked for: later features that named the faces a
    /// fix renamed.
    pub followed: Vec<FeatureUid>,
    /// The problems left, in timeline order.
    pub left: Vec<AutoFixLeft>,
    /// The threads' faces fitted to them (mitcad#1632), in timeline order.
    pub fitted: Vec<AutoFixFit>,
    /// The named parameters a fit left for a choice (used elsewhere):
    /// the thread, the parameter, how many other places use it.
    pub choices: Vec<(FeatureUid, String, usize)>,
    /// The filament changes at faces whose faces an edit renamed, written
    /// with the names found (mitcad#1643; an Auto Fix of every feature).
    pub filament_faces: usize,
    /// The undo step's name; None when nothing changed.
    pub label: Option<String>,
    /// The summary: `Auto Fix: 2 of 3 warnings fixed; Hole1: the body has
    /// no edge, pick it again`, in plain words (mitcad#1612: `left` names
    /// the edge for agents).
    pub message: String,
}

fn summary(
    total: usize,
    fixed: usize,
    left: &[AutoFixLeft],
    followed: &[String],
    fitted: &[AutoFixFit],
) -> String {
    if total == 0 && left.is_empty() {
        return "Auto Fix: nothing to fix".to_owned();
    }
    let noun = if total == 1 { "warning" } else { "warnings" };
    let mut text = format!("Auto Fix: {fixed} of {total} {noun} fixed");
    // What the fits changed (mitcad#1632), in words.
    for fit in fitted {
        let _ = write!(text, "; {}: {}", fit.thread_name, fit.summary());
    }
    // In plain words (mitcad#1612): `left` keeps the agents' texts.
    for item in left {
        let _ = write!(
            text,
            "; {}: {}, {}",
            item.name,
            crate::notice::user_message(&item.message),
            item.advice
        );
    }
    if !followed.is_empty() {
        let _ = write!(text, "; also updated: {}", followed.join(", "));
    }
    text
}

/// Remembers why the fix of these warnings of `uid` was not taken.
fn refuse(
    refused: &mut BTreeMap<(FeatureUid, String), String>,
    uid: FeatureUid,
    warnings: &[&String],
    reason: &str,
) {
    for warning in warnings {
        refused.insert((uid, (*warning).clone()), reason.to_owned());
    }
}

impl<K: Kernel> Document<K> {
    /// How many of the feature's warnings Auto Fix can write into its
    /// definition (the timeline offers Auto Fix when there are any).
    pub fn auto_fixable(&self, uid: FeatureUid) -> usize {
        let Some(entry) = self.state.entry(uid) else {
            return 0;
        };
        // A thread's faces whose diameter something Auto Fix can change
        // sets (mitcad#1632), also those that ask for a choice.
        let fits = if matches!(entry.def, FeatureDef::Thread(_)) {
            self.thread_fits(uid)
                .iter()
                .filter(|m| {
                    plan_fit(
                        &self.state,
                        uid,
                        &entry.name,
                        m,
                        Some(SharedChoice::Parameter),
                    )
                    .is_ok()
                })
                .count()
        } else {
            0
        };
        let repairs: Vec<Repair> = self
            .warnings(uid)
            .iter()
            .filter_map(|w| Repair::parse(w.as_str()))
            .collect();
        if repairs.is_empty() || self.state.editable(entry.component).is_err() {
            return fits;
        }
        fits + rewrite(&self.state, entry, &repairs)
            .map_or(0, |(_, applied)| applied.iter().filter(|a| **a).count())
    }

    /// The named parameters fitting thread `uid`'s faces would change that
    /// other places use too (mitcad#1632): each with how many; Auto Fix
    /// asks for a [`SharedChoice`] then.
    pub fn auto_fix_choices(&self, uid: FeatureUid) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for miss in self.thread_fits(uid) {
            if let Some(NoFit::Choice { parameter, others }) = self.fit_question(&miss)
                && !out.iter().any(|(p, _)| *p == parameter)
            {
                out.push((parameter, others));
            }
        }
        out
    }

    /// Auto Fix of the features (all when none are given; see the module
    /// docs): one undo step, or none when nothing could be fixed. A
    /// cancelled computation (P7) changes nothing.
    pub fn auto_fix(&mut self, features: &[FeatureUid]) -> Result<AutoFix, ModelError> {
        self.auto_fix_with(features, None)
    }

    /// [`Document::auto_fix`] with what a thread's fit does with a named
    /// parameter that other places use (mitcad#1632): without `choice`
    /// such a fit is not taken and `choices` names the parameter.
    pub fn auto_fix_with(
        &mut self,
        features: &[FeatureUid],
        choice: Option<SharedChoice>,
    ) -> Result<AutoFix, ModelError> {
        let mut asked: Vec<usize> = if features.is_empty() {
            (0..self.state.features.len()).collect()
        } else {
            features
                .iter()
                .map(|uid| self.state.require(*uid))
                .collect::<Result<_, _>>()?
        };
        asked.sort_unstable();
        asked.dedup();
        // The filament changes at faces an edit renamed (mitcad#1643): the
        // design's references too, with an Auto Fix of every feature.
        let filament_faces = if features.is_empty() {
            self.renamed_change_faces()
        } else {
            Vec::new()
        };
        let asked_uids: Vec<FeatureUid> =
            asked.iter().map(|&p| self.state.features[p].uid).collect();
        let before: Vec<(FeatureUid, String)> = asked
            .iter()
            .zip(&asked_uids)
            .flat_map(|(&p, &uid)| {
                problems(result_at(&self.result, p, uid))
                    .into_iter()
                    .map(move |text| (uid, text))
            })
            .collect();
        // Nothing that built may fail because of a fix.
        let built: BTreeSet<FeatureUid> = self
            .result
            .results
            .iter()
            .filter(|r| r.status.is_ok())
            .map(|r| r.uid)
            .collect();

        // Each feature in timeline order, on the state the fixes before it
        // made: all its warnings (None), or those a fix before it made.
        let mut next = self.state.clone();
        let mut computed: Option<Recomputed<K::Shape>> = None;
        let mut work: BTreeMap<usize, Option<BTreeSet<String>>> =
            asked.iter().map(|&p| (p, None)).collect();
        let mut refused: BTreeMap<(FeatureUid, String), String> = BTreeMap::new();
        let mut changed: Vec<usize> = Vec::new();
        while let Some((position, only)) = work.pop_first() {
            let entry = next.features[position].clone();
            let uid = entry.uid;
            let last = computed.as_ref().unwrap_or(&self.result);
            let repairs: Vec<(String, Repair)> = result_at(last, position, uid)
                .map_or(&[][..], |r| r.warnings())
                .iter()
                .filter(|w| only.as_ref().is_none_or(|set| set.contains(*w)))
                .filter_map(|w| Repair::parse(w).map(|repair| (w.clone(), repair)))
                .collect();
            if repairs.is_empty() {
                continue;
            }
            let warnings: Vec<&String> = repairs.iter().map(|(w, _)| w).collect();
            if let Err(error) = next.editable(entry.component) {
                refuse(&mut refused, uid, &warnings, &error.to_string());
                continue;
            }
            let list: Vec<Repair> = repairs.iter().map(|(_, r)| r.clone()).collect();
            let Some((def, applied)) = rewrite(&next, &entry, &list) else {
                refuse(&mut refused, uid, &warnings, NOT_HELD);
                continue;
            };
            let held = |want: bool| -> Vec<&String> {
                warnings
                    .iter()
                    .zip(&applied)
                    .filter(|(_, h)| **h == want)
                    .map(|(w, _)| *w)
                    .collect()
            };
            let fixing = held(true);
            refuse(&mut refused, uid, &held(false), NOT_HELD);
            let mut candidate = next.clone();
            Arc::make_mut(&mut candidate.features[position]).def = def;
            let fixed_entry = candidate.features[position].clone();
            if let Err(error) = candidate.check_feature(position, &fixed_entry) {
                refuse(&mut refused, uid, &fixing, &error);
                continue;
            }
            // The later features' warnings before this fix: those it makes
            // (a face it renamed) are its own to fix.
            let later_before: Vec<Vec<String>> = (position + 1..next.features.len())
                .map(|p| {
                    result_at(last, p, next.features[p].uid)
                        .map_or_else(Vec::new, |r| r.warnings().to_vec())
                })
                .collect();
            let result = self.compute_after(&candidate)?;
            if let Some(broken) = result
                .results
                .iter()
                .find(|r| built.contains(&r.uid) && matches!(r.status, FeatureStatus::Failed(_)))
            {
                let name = candidate
                    .entry(broken.uid)
                    .map_or_else(|| broken.uid.to_string(), |e| e.name.clone());
                refuse(
                    &mut refused,
                    uid,
                    &fixing,
                    &format!("fixing it would make {name} fail"),
                );
                continue;
            }
            let now: Vec<String> =
                result_at(&result, position, uid).map_or_else(Vec::new, |r| r.warnings().to_vec());
            if fixing.iter().all(|w| now.contains(*w)) {
                refuse(
                    &mut refused,
                    uid,
                    &fixing,
                    "the warning stays with the names found",
                );
                continue;
            }
            for (offset, old) in later_before.iter().enumerate() {
                let p = position + 1 + offset;
                let Some(later) = result_at(&result, p, candidate.features[p].uid) else {
                    continue;
                };
                let made: BTreeSet<String> = later
                    .warnings()
                    .iter()
                    .filter(|w| !old.contains(*w) && Repair::parse(w).is_some())
                    .cloned()
                    .collect();
                if made.is_empty() {
                    continue;
                }
                // (A feature that fixes all its warnings already keeps None.)
                if let Some(set) = work.entry(p).or_insert_with(|| Some(BTreeSet::new())) {
                    set.extend(made);
                }
            }
            next = candidate;
            computed = Some(result);
            changed.push(position);
        }

        // The threads' faces that do not fit them (mitcad#1632), each on
        // the state the fixes before it made.
        let mut fitted: Vec<AutoFixFit> = Vec::new();
        let mut fitted_positions: Vec<usize> = Vec::new();
        // The features a fit gave a parameter of their own (mitcad#1632).
        let mut owned: Vec<usize> = Vec::new();
        let mut choices: Vec<(FeatureUid, String, usize)> = Vec::new();
        for &position in &asked {
            let entry = next.features[position].clone();
            if !matches!(entry.def, FeatureDef::Thread(_)) {
                continue;
            }
            let uid = entry.uid;
            let mut tried: BTreeSet<String> = BTreeSet::new();
            loop {
                let misses = misses_in(computed.as_ref().unwrap_or(&self.result), uid);
                let Some(miss) = misses
                    .into_iter()
                    .find(|m| !tried.contains(&m.face.to_string()))
                else {
                    break;
                };
                let face = miss.face.to_string();
                tried.insert(face.clone());
                let plan = match plan_fit(&next, uid, &entry.name, &miss, choice) {
                    Ok(plan) => plan,
                    Err(why) => {
                        if let NoFit::Choice { parameter, others } = &why {
                            choices.push((uid, parameter.clone(), *others));
                        }
                        refused.insert((uid, miss.warning.clone()), why.advice(miss.internal));
                        continue;
                    }
                };
                match self.try_fit(&next, &plan, uid, &face, &built)? {
                    Ok((candidate, result)) => {
                        next = candidate;
                        computed = Some(result);
                        if let Some(own) = &plan.own
                            && let Some(p) = next.position(own.feature)
                            && !owned.contains(&p)
                        {
                            owned.push(p);
                        }
                        fitted.push(plan.fit);
                        if !fitted_positions.contains(&position) {
                            fitted_positions.push(position);
                        }
                    }
                    Err(why) => {
                        refused.insert((uid, miss.warning.clone()), why);
                    }
                }
            }
        }

        let changed_uids: Vec<FeatureUid> = changed
            .iter()
            .map(|&p| self.state.features[p].uid)
            .collect();
        // The fitted threads name the step too (`Auto Fix Thread2`).
        let mut named_uids = changed_uids.clone();
        for &p in &fitted_positions {
            let uid = self.state.features[p].uid;
            if !named_uids.contains(&uid) {
                named_uids.push(uid);
            }
        }
        let label = if !named_uids.is_empty() {
            Some(self.auto_fix_label(&asked_uids, &named_uids))
        } else if !filament_faces.is_empty() {
            Some("Auto Fix Filament Changes".to_owned())
        } else {
            None
        };
        if let Some(label) = &label {
            let entries: Vec<(usize, Arc<FeatureEntry>)> = changed
                .iter()
                .chain(&owned)
                .map(|&p| (p, next.features[p].clone()))
                .collect();
            let label = label.clone();
            // The fits' parameters, as the last fit left them (only fits
            // change parameters).
            let parameters = (!fitted.is_empty()).then(|| next.parameters.clone());
            let faces = filament_faces.clone();
            self.apply(|state| {
                for (p, entry) in entries {
                    state.features[p] = entry;
                }
                // The filament changes' faces by the names found.
                for (s, e, found) in faces {
                    if let Some(change) = state
                        .filament_changes
                        .get_mut(s)
                        .and_then(|schedule| schedule.changes.get_mut(e))
                        && let Some(crate::features::geom_ref::GeomRef::Face { face, .. }) =
                            &mut change.at
                    {
                        *face = found;
                    }
                }
                if let Some(parameters) = parameters {
                    state.parameters = parameters;
                }
                Ok((label, ()))
            })?;
        }

        // What is left, in timeline order: the problems of the features
        // asked for and of those it followed to.
        let followed: Vec<FeatureUid> = changed_uids
            .iter()
            .copied()
            .filter(|u| !asked_uids.contains(u))
            .collect();
        let mut reported: Vec<(usize, FeatureUid)> = asked
            .iter()
            .copied()
            .zip(asked_uids.iter().copied())
            .chain(
                followed
                    .iter()
                    .filter_map(|&u| self.state.position(u).map(|p| (p, u))),
            )
            .collect();
        reported.sort_unstable();
        let name = |uid: FeatureUid| {
            self.state
                .entry(uid)
                .map_or_else(|| uid.to_string(), |e| e.name.clone())
        };
        let mut left = Vec::new();
        let mut now: BTreeMap<FeatureUid, Vec<String>> = BTreeMap::new();
        for &(position, uid) in &reported {
            let texts = problems(result_at(&self.result, position, uid));
            for text in &texts {
                left.push(AutoFixLeft {
                    feature: uid,
                    name: name(uid),
                    message: short(text),
                    advice: advice(text, refused.get(&(uid, text.clone()))),
                });
            }
            now.insert(uid, texts);
        }
        let fixed = before
            .iter()
            .filter(|(uid, text)| !now.get(uid).is_some_and(|texts| texts.contains(text)))
            .count();
        let followed_names: Vec<String> = followed.iter().map(|&u| name(u)).collect();
        let mut message = summary(before.len(), fixed, &left, &followed_names, &fitted);
        // The filament changes' faces found again (mitcad#1643).
        let alone = before.is_empty() && left.is_empty();
        match (filament_faces.len(), alone) {
            (0, _) => {}
            (1, true) => message = "Auto Fix: the face of a filament change found again".to_owned(),
            (1, false) => message.push_str("; the face of a filament change found again"),
            (n, true) => {
                message = format!("Auto Fix: the faces of {n} filament changes found again");
            }
            (n, false) => {
                let _ = write!(message, "; the faces of {n} filament changes found again");
            }
        }
        Ok(AutoFix {
            warnings: before.len(),
            fixed,
            changed: changed_uids,
            followed,
            left,
            fitted,
            choices,
            filament_faces: filament_faces.len(),
            label,
            message,
        })
    }

    /// The undo step's name: the one feature fixed, the timeline group
    /// asked for, or how many features were fixed.
    fn auto_fix_label(&self, asked: &[FeatureUid], changed: &[FeatureUid]) -> String {
        if asked.len() > 1
            && let Some(group) = self.state.groups.iter().find(|g| {
                g.features.len() == asked.len() && g.features.iter().all(|u| asked.contains(u))
            })
        {
            return format!("Auto Fix {}", group.name);
        }
        let own: Vec<FeatureUid> = changed
            .iter()
            .copied()
            .filter(|u| asked.contains(u))
            .collect();
        match own.as_slice() {
            [one] => format!(
                "Auto Fix {}",
                self.state
                    .entry(*one)
                    .map_or_else(|| one.to_string(), |e| e.name.clone())
            ),
            _ => format!("Auto Fix {} Features", own.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_parse_as_repairs() {
        let region = Repair::parse(
            "region r{c1[c4,c2],c2[c1,c3]} found as r{c1[c4,c5]} + r{c5[c1,c3]} \
             after a change in Sketch 1",
        );
        assert_eq!(
            region,
            Some(Repair::Region {
                sketch: "Sketch 1".to_owned(),
                stored: "r{c1[c4,c2],c2[c1,c3]}".parse().unwrap(),
                found: vec![
                    "r{c1[c4,c5]}".parse().unwrap(),
                    "r{c5[c1,c3]}".parse().unwrap()
                ],
            })
        );
        let edge = Repair::parse(
            "section 2: edge E{F2:a|F2:b} found as E{F2:a|F2:c} after a change in Sketch1",
        );
        assert_eq!(
            edge,
            Some(Repair::Name {
                stored: "E{F2:a|F2:b}".parse().unwrap(),
                found: "E{F2:a|F2:c}".parse().unwrap(),
            })
        );
        for other in [
            "a loft section is one region; of the pieces of r{c1} the first is used",
            "fillet built as rings",
            "face F2:a found as nothing after a change in Sketch1",
        ] {
            assert_eq!(Repair::parse(other), None, "{other}");
        }
    }

    #[test]
    fn problems_are_told_apart() {
        assert_eq!(
            problem("face F2:a found as F2:b after a change in Sketch1"),
            Some(Problem::Renamed)
        );
        assert_eq!(
            problem("F2:side(c1[c4,c2]) is ambiguous after a sketch change: F2:x, F2:y"),
            Some(Problem::Tie)
        );
        assert_eq!(
            problem("the body has no edge E{F2:a|F2:b}"),
            Some(Problem::Gone)
        );
        assert_eq!(problem("Sketch1 has no profile r{c1}"), Some(Problem::Gone));
        assert_eq!(problem("fillet built as rings"), None);
        assert_eq!(
            short("the body has no edge E{x}; existing edges of body F2.b0 that share …"),
            "the body has no edge E{x}"
        );
    }

    #[test]
    fn a_split_region_takes_its_place_in_a_list_once() {
        let found: Vec<RegionKey> = vec!["r{c2}".parse().unwrap(), "r{c3}".parse().unwrap()];
        let is_reference = |item: &Value| -> bool {
            item.get("sketch") == Some(&Value::from("F1"))
                && item.get("region").and_then(Value::as_str) == Some("r{c1}")
        };
        let profile = |region: &str| serde_json::json!({"sketch": "F1", "region": region});
        // Another sketch's region of the same key is not the reference.
        let mut def = serde_json::json!({
            "profiles": [profile("r{c4}"), profile("r{c1}"), profile("r{c9}"), profile("r{c3}"),
                         {"sketch": "F7", "region": "r{c1}"}],
            "section": profile("r{c1}"),
        });
        assert!(regions(&mut def, &is_reference, &found));
        assert_eq!(
            def["profiles"],
            serde_json::json!([profile("r{c4}"), profile("r{c2}"), profile("r{c3}"),
                               profile("r{c9}"), {"sketch": "F7", "region": "r{c1}"}])
        );
        // Elsewhere the main region.
        assert_eq!(def["section"], profile("r{c2}"));
        assert!(!regions(&mut def, &is_reference, &found));
    }

    #[test]
    fn a_name_is_replaced_wherever_the_definition_holds_it() {
        let stored: TopoName = "F2:side(c1[c4,c2])".parse().unwrap();
        let mut def = serde_json::json!({
            "faces": ["F2:side(c1[c4,c2])", "F2:side(c3[c2,c4])"],
            "plane": {"body": "F2.b0", "face": "F2:side(c1[c4,c2])"},
            "distance": "\u{1}0",
        });
        assert!(rename(&mut def, &stored, "F2:side(c1[c4,c9])"));
        assert_eq!(
            def,
            serde_json::json!({
                "faces": ["F2:side(c1[c4,c9])", "F2:side(c3[c2,c4])"],
                "plane": {"body": "F2.b0", "face": "F2:side(c1[c4,c9])"},
                "distance": "\u{1}0",
            })
        );
        assert!(!rename(&mut def, &stored, "F2:side(c1[c4,c9])"));
    }
}
