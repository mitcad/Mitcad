// SPDX-License-Identifier: MIT
//! Exploded views of assemblies (mitcad#2113): what an explode view
//! stores. The document's side (commands, the resolved offsets, Auto
//! Explode) is `document/explode.rs`.
//!
//! An **explode view** belongs to an assembly component (the root, or any
//! component with occurrences) and is an ordered list of **steps**. A step
//! moves a set of occurrences (paths from the view's component) and of
//! the component's own bodies by a distance along a direction and/or
//! turns them by an angle about an axis. The values are expressions
//! (`40`, `40 mm`, `2 * gap`, `90 deg`) read with the design's
//! parameters, so they follow them; a plain number is millimetres or
//! degrees. Directions and axes are a fixed vector in the component's
//! coordinates or geometry: an edge, an axis, a face (its normal, a round
//! face's axis), a plane, or an origin axis of the component or of an
//! occurrence's component, as a joint origin picks it.
//!
//! A step's motion is applied after the earlier steps': a part moved in
//! two steps ends at the sum of both, an occurrence moved with its parent
//! moves with it. A turn is about the axis where the design has it
//! assembled; an axis given as a vector alone goes through the centre of
//! the parts the step moves, where the earlier steps left them. The
//! explode never changes the design's placements, joints or timeline: it
//! is a view state, saved in the design file and changed by undoable
//! commands that recompute nothing.

use serde::{Deserialize, Serialize};

use crate::datum::Vec3;
use crate::features::{GeomRef, OccurrencePath, is_false};
use crate::ids::{BodyUid, ComponentUid};

/// A step's time in the animation when it has none of its own (seconds).
pub const DEFAULT_STEP_SECONDS: f64 = 1.0;
/// The longest a step may take (seconds).
pub const MAX_STEP_SECONDS: f64 = 3600.0;
/// Auto Explode's factor when none is given: each part moves by its own
/// size (its box's diagonal) times it.
pub const DEFAULT_AUTO_FACTOR: f64 = 1.0;
/// The most steps a view may have.
pub const MAX_STEPS: usize = 10_000;

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_root(component: &ComponentUid) -> bool {
    component.is_root()
}

/// An explode view of an assembly component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplodeView {
    /// Unique among the design's explode views (`Explode1`).
    pub name: String,
    /// The component it explodes; its steps' occurrences are paths from
    /// it, its directions in its coordinates. The root's (`C0`) is left
    /// out.
    #[serde(default, skip_serializing_if = "is_root")]
    pub component: ComponentUid,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ExplodeStep>,
}

/// One step of an explode view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplodeStep {
    /// Its own name; left out, the panel and the query call it `Step <n>`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The occurrences it moves, paths from the view's component; what
    /// they place moves with them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occurrences: Vec<OccurrencePath>,
    /// Bodies of the view's component itself that it moves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<BodyUid>,
    /// How far it moves them along `direction`: an expression of a length
    /// (a plain number in millimetres).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<ExplodeAxis>,
    /// How far it turns them about `axis`: an expression of an angle (a
    /// plain number in degrees).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<ExplodeAxis>,
    /// Trail lines from each moved part's reference point back to where
    /// the step found it (on by default).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub trails: bool,
    /// The trails' reference point, in the view component's coordinates
    /// where the design has the parts assembled; left out, each part's
    /// bounding box centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<Vec3>,
    /// Its time in the animation (seconds); left out,
    /// [`DEFAULT_STEP_SECONDS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
}

/// A step's direction or axis: a fixed vector in the view component's
/// coordinates, or geometry (`geometry` placed by `occurrence`, a path
/// from the view's component; empty for the component's own) whose frame
/// gives it as a joint origin's does: a planar face's normal, a round
/// face's or a circular edge's axis, a straight edge, an axis, a plane's
/// normal. An axis turns about the line through the frame's origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplodeAxis {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector: Option<Vec3>,
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub occurrence: OccurrencePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<GeomRef>,
    /// The other way round.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
}

impl ExplodeAxis {
    /// A fixed direction.
    pub fn along(vector: Vec3) -> Self {
        Self {
            vector: Some(vector),
            occurrence: OccurrencePath::default(),
            geometry: None,
            flip: false,
        }
    }

    /// Why it cannot be kept, if it cannot (not its geometry, which only
    /// the recompute can resolve).
    pub(crate) fn check(&self) -> Result<(), String> {
        match (&self.vector, &self.geometry) {
            (Some(_), Some(_)) => Err("give a vector or geometry, not both".to_owned()),
            (None, None) => Err("give a vector or geometry".to_owned()),
            (Some(v), None) => {
                if !self.occurrence.is_empty() {
                    return Err("a vector has no occurrence".to_owned());
                }
                if v.iter().any(|x| !x.is_finite()) || v.iter().all(|x| x.abs() < 1e-12) {
                    return Err("the vector must be finite and not zero".to_owned());
                }
                Ok(())
            }
            (None, Some(_)) => Ok(()),
        }
    }
}

impl ExplodeStep {
    /// A step that moves occurrences along a fixed direction.
    pub fn moving(occurrences: Vec<OccurrencePath>, distance: &str, direction: Vec3) -> Self {
        Self {
            name: String::new(),
            occurrences,
            bodies: Vec::new(),
            distance: Some(distance.to_owned()),
            direction: Some(ExplodeAxis::along(direction)),
            angle: None,
            axis: None,
            trails: true,
            point: None,
            seconds: None,
        }
    }

    /// Its time in the animation.
    pub fn seconds(&self) -> f64 {
        self.seconds.unwrap_or(DEFAULT_STEP_SECONDS)
    }

    /// Its name as the panel shows it: its own, else `Step <n>` (`index`
    /// from 0).
    pub fn shown_name(&self, index: usize) -> String {
        if self.name.trim().is_empty() {
            format!("Step {}", index + 1)
        } else {
            self.name.clone()
        }
    }

    /// Why the step cannot be kept, if it cannot (not its occurrences and
    /// values, which the document checks).
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.occurrences.iter().any(OccurrencePath::is_empty) {
            return Err("the component itself is not an occurrence it can move".to_owned());
        }
        for (i, path) in self.occurrences.iter().enumerate() {
            if self.occurrences[..i].contains(path) {
                return Err(format!("{path} is listed more than once"));
            }
        }
        for (i, body) in self.bodies.iter().enumerate() {
            if self.bodies[..i].contains(body) {
                return Err(format!("{body} is listed more than once"));
            }
        }
        match (&self.distance, &self.direction) {
            (Some(_), None) => return Err("a distance needs a direction".to_owned()),
            (None, Some(_)) => return Err("a direction needs a distance".to_owned()),
            (Some(d), Some(direction)) => {
                if d.trim().is_empty() {
                    return Err("the distance is empty".to_owned());
                }
                direction.check().map_err(|e| format!("direction: {e}"))?;
            }
            (None, None) => {}
        }
        match (&self.angle, &self.axis) {
            (Some(_), None) => return Err("an angle needs an axis".to_owned()),
            (None, Some(_)) => return Err("an axis needs an angle".to_owned()),
            (Some(a), Some(axis)) => {
                if a.trim().is_empty() {
                    return Err("the angle is empty".to_owned());
                }
                axis.check().map_err(|e| format!("axis: {e}"))?;
            }
            (None, None) => {}
        }
        if self.distance.is_none() && self.angle.is_none() {
            return Err("a step moves along a direction, turns about an axis, or both".to_owned());
        }
        if let Some(p) = self.point
            && p.iter().any(|v| !v.is_finite())
        {
            return Err("the reference point is not finite".to_owned());
        }
        if let Some(s) = self.seconds
            && !(s.is_finite() && s > 0.0 && s <= MAX_STEP_SECONDS)
        {
            return Err(format!(
                "its time must be more than 0 and at most {MAX_STEP_SECONDS} seconds"
            ));
        }
        Ok(())
    }
}

impl ExplodeView {
    /// A view without steps.
    pub fn new(name: impl Into<String>, component: ComponentUid) -> Self {
        Self {
            name: name.into(),
            component,
            steps: Vec::new(),
        }
    }

    /// The animation's length: every step's time (seconds).
    pub fn seconds(&self) -> f64 {
        self.steps.iter().map(ExplodeStep::seconds).sum()
    }

    /// How far each step has gone at a time of the animation (seconds from
    /// the assembled design): 1 for the steps before it, the part of the
    /// step under way, 0 for those after. None: every step done.
    pub fn fractions(&self, time: Option<f64>) -> Vec<f64> {
        let Some(time) = time else {
            return vec![1.0; self.steps.len()];
        };
        let mut start = 0.0;
        self.steps
            .iter()
            .map(|s| {
                let length = s.seconds();
                let f = ((time - start) / length).clamp(0.0, 1.0);
                start += length;
                f
            })
            .collect()
    }

    /// Why the view cannot be kept, if it cannot (not its occurrences and
    /// values, which the document checks). In words, with the step's
    /// number.
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the explode view needs a name".to_owned());
        }
        if self.steps.len() > MAX_STEPS {
            return Err(format!(
                "{}: an explode view has at most {MAX_STEPS} steps",
                self.name
            ));
        }
        for (i, step) in self.steps.iter().enumerate() {
            step.check()
                .map_err(|e| format!("{}, {}: {e}", self.name, step.shown_name(i)))?;
        }
        Ok(())
    }
}

/// A number as a value's text: millimetres or degrees with their unit,
/// rounded to a micrometre (a thousandth of a degree).
pub fn value_text(value: f64, unit: &str) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    let mut text = format!("{rounded:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    format!("{text} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_follow_the_steps_times() {
        let mut view = ExplodeView::new("E", ComponentUid::ROOT);
        let mut a = ExplodeStep::moving(vec![], "10", [1.0, 0.0, 0.0]);
        a.seconds = Some(2.0);
        let b = ExplodeStep::moving(vec![], "10", [0.0, 1.0, 0.0]);
        view.steps = vec![a, b];
        assert_eq!(view.seconds(), 3.0);
        assert_eq!(view.fractions(None), vec![1.0, 1.0]);
        assert_eq!(view.fractions(Some(0.0)), vec![0.0, 0.0]);
        assert_eq!(view.fractions(Some(1.0)), vec![0.5, 0.0]);
        assert_eq!(view.fractions(Some(2.5)), vec![1.0, 0.5]);
        assert_eq!(view.fractions(Some(9.0)), vec![1.0, 1.0]);
    }

    #[test]
    fn steps_are_checked_in_words() {
        let mut step = ExplodeStep::moving(vec![], "10", [0.0, 0.0, 0.0]);
        assert!(step.check().unwrap_err().contains("not zero"));
        step.direction = None;
        assert!(step.check().unwrap_err().contains("needs a direction"));
        step.distance = None;
        assert!(step.check().unwrap_err().contains("both"));
        let mut view = ExplodeView::new("E", ComponentUid::ROOT);
        view.steps.push(step);
        assert!(view.check().unwrap_err().starts_with("E, Step 1: "));
    }

    #[test]
    fn values_as_text() {
        assert_eq!(value_text(40.0, "mm"), "40 mm");
        assert_eq!(value_text(12.34567, "mm"), "12.346 mm");
        assert_eq!(value_text(-0.0001, "deg"), "0 deg");
    }
}
