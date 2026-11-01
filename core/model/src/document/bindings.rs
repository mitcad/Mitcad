// SPDX-License-Identifier: MIT
//! Bindings (mitcad#800; `docs/configurations.md`, "Bindings"): a body's
//! appearance and material, a face's appearance, a feature's suppression
//! and the visibility of a body or an occurrence can follow an expression
//! of parameters instead of a value set by hand. A configuration only sets
//! parameters, and these follow.
//!
//! - An appearance or a material follows a text: an id, else the name of
//!   exactly one appearance or material. A text that names none, or names
//!   two, gives the body a warning and the default look: no appearance of
//!   its own (its material's), or no material of its own (its component's).
//! - Suppression and visibility follow a condition: a number, true when it
//!   is not zero (`Color = 'Black'`, `Length > 1 m`). A condition that
//!   cannot be evaluated (a parameter it names is gone) leaves the value as
//!   it was, with a warning.
//! - The expression is kept beside the value it gives (`appearance_expr`
//!   next to `appearance` in [`BodyAttributes`], [`Bindings`] for features
//!   and occurrences). Every command settles the values again after its
//!   edit (`Document::apply`), so undo, the design file and every reader
//!   see the value as if it were set by hand. A changed suppression
//!   recomputes from that feature on, as `suppress_feature` does;
//!   appearance, material and visibility recompute nothing.
//! - A material that follows an expression gives the body the new
//!   material's appearance by the rule of mitcad#640 (`materials.rs`):
//!   unless the body's appearance was set by hand or follows an expression
//!   of its own.
//! - Setting the value by hand removes the expression, in the same undo
//!   step; an empty expression removes it and keeps the value it gave.
//!   Renames rewrite the expressions, and a parameter that one uses cannot
//!   be deleted.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::bodies::faces_label;
use super::{BodyAttributes, DocState, Document, ModelError, invalid};
use crate::appearance;
use crate::expr::{Expr, Value, replace_references, with_decimal_points};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::material::{self, DEFAULT_MATERIAL};
use crate::parameters::ParamId;

/// The bindings of features and occurrences; those of bodies are in their
/// [`BodyAttributes`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bindings {
    /// Conditions under which features are suppressed, by feature.
    pub suppressed_when: BTreeMap<FeatureUid, String>,
    /// Conditions under which occurrences are shown, by occurrence.
    pub occurrence_visible_when: BTreeMap<OccurrenceUid, String>,
}

impl Bindings {
    pub fn is_empty(&self) -> bool {
        self.suppressed_when.is_empty() && self.occurrence_visible_when.is_empty()
    }
}

/// What a binding's expression gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// A text that names an appearance.
    Appearance,
    /// A text that names a material.
    Material,
    /// A condition: a number, true when it is not zero.
    Condition,
}

impl BodyAttributes {
    /// Whether any value of the body follows an expression.
    pub fn has_bindings(&self) -> bool {
        self.appearance_expr.is_some()
            || self.material_expr.is_some()
            || self.visible_when.is_some()
            || !self.face_appearance_exprs.is_empty()
    }

    /// The body's binding expressions.
    fn binding_texts(&self) -> impl Iterator<Item = &str> {
        self.appearance_expr
            .iter()
            .chain(&self.material_expr)
            .chain(&self.visible_when)
            .chain(self.face_appearance_exprs.values())
            .map(String::as_str)
    }
}

/// The value of an expression against the parameters.
fn evaluate(state: &DocState, text: &str) -> Result<Value, String> {
    state
        .parameters
        .table()
        .evaluate_value(text)
        .map_err(|e| e.to_string())
}

/// The text an appearance's or a material's expression gives.
fn text_of(state: &DocState, text: &str) -> Result<String, String> {
    match evaluate(state, text)? {
        Value::Text(name) => Ok(name),
        Value::Number(_) => Err(format!(
            "“{text}” gives a number; it must give a text: an id or a name"
        )),
    }
}

/// Whether a condition holds.
fn holds(state: &DocState, text: &str) -> Result<bool, String> {
    match evaluate(state, text)? {
        Value::Number(q) => Ok(q.value != 0.0),
        Value::Text(_) => Err(format!(
            "“{text}” gives a text; a condition gives a number (true when not 0)"
        )),
    }
}

/// The only id of `ids`, else why there is none.
fn unique(ids: &[&str], what: &str, text: &str) -> Result<String, String> {
    match ids {
        [id] => Ok((*id).to_owned()),
        [] => Err(format!("there is no {what} {text}")),
        _ => Err(format!(
            "{text} names {} {what}s ({})",
            ids.len(),
            ids.join(", ")
        )),
    }
}

impl DocState {
    /// The appearance a text names: an appearance of the library or of the
    /// design by id, else the only one of that name.
    pub(crate) fn appearance_named(&self, text: &str) -> Result<String, String> {
        let text = text.trim();
        if appearance::library_appearance(text).is_some()
            || self.appearances.iter().any(|a| a.id == text)
        {
            return Ok(text.to_owned());
        }
        // The design's own that a library id does not hide.
        let own = self
            .appearances
            .iter()
            .filter(|a| appearance::library_appearance(&a.id).is_none());
        let ids: Vec<&str> = appearance::library()
            .iter()
            .chain(own)
            .filter(|a| a.name == text)
            .map(|a| a.id.as_str())
            .collect();
        unique(&ids, "appearance", text)
    }

    /// The material a text names: a material of the design or a built-in
    /// one by id, else the only one of that name.
    pub(crate) fn material_named(&self, text: &str) -> Result<String, String> {
        let text = text.trim();
        if self.material(text).is_some() {
            return Ok(text.to_owned());
        }
        // The built-in ones that a design's material does not replace.
        let built_in = material::built_in()
            .iter()
            .filter(|m| self.materials.iter().all(|own| own.id != m.id));
        let ids: Vec<&str> = self
            .materials
            .iter()
            .chain(built_in)
            .filter(|m| m.name == text)
            .map(|m| m.id.as_str())
            .collect();
        unique(&ids, "material", text)
    }

    /// The appearance of a material (None: the component's default, then
    /// the root's, else steel), for a body of `component`.
    fn material_look(&self, material: Option<&str>, component: ComponentUid) -> Option<String> {
        let found = match material {
            Some(id) => self.material(id),
            None => [component, ComponentUid::ROOT]
                .iter()
                .find_map(|c| self.component_materials.get(c))
                .and_then(|id| self.material(id))
                .or_else(|| self.material(DEFAULT_MATERIAL)),
        };
        found.and_then(|m| m.appearance.clone())
    }

    /// The component of a body's creating feature.
    fn body_component_of(&self, uid: BodyUid) -> ComponentUid {
        self.entry(uid.feature)
            .map_or(ComponentUid::ROOT, |f| f.component)
    }

    /// Sets every bound value to what its expression gives now, and lets
    /// go of the bindings of features and occurrences that are gone.
    /// Returns whether a feature's suppression changed: it then recomputes.
    pub(crate) fn settle_bindings(&mut self) -> bool {
        if !self.bindings.suppressed_when.is_empty() {
            let features: BTreeSet<FeatureUid> = self.features.iter().map(|f| f.uid).collect();
            self.bindings
                .suppressed_when
                .retain(|uid, _| features.contains(uid));
        }
        let assembly = &self.assembly;
        self.bindings
            .occurrence_visible_when
            .retain(|uid, _| assembly.occurrence(*uid).is_some());
        // Bodies.
        let bound: Vec<BodyUid> = self
            .body_attributes
            .iter()
            .filter(|(_, a)| a.has_bindings())
            .map(|(uid, _)| *uid)
            .collect();
        for uid in bound {
            let mut attributes = self.body_attributes[&uid].clone();
            self.settle_body(uid, &mut attributes);
            self.body_attributes.insert(uid, attributes);
        }
        // Occurrences.
        let shown: Vec<(OccurrenceUid, bool)> = self
            .bindings
            .occurrence_visible_when
            .iter()
            .filter_map(|(uid, text)| holds(self, text).ok().map(|v| (*uid, v)))
            .collect();
        for (uid, visible) in shown {
            if let Some(occurrence) = self.assembly.occurrence_mut(uid) {
                occurrence.visible = visible;
            }
        }
        // Features.
        let suppressed: Vec<(FeatureUid, bool)> = self
            .bindings
            .suppressed_when
            .iter()
            .filter_map(|(uid, text)| holds(self, text).ok().map(|v| (*uid, v)))
            .collect();
        let mut changed = false;
        for (uid, value) in suppressed {
            if let Some(position) = self.position(uid)
                && self.features[position].suppressed != value
            {
                Arc::make_mut(&mut self.features[position]).suppressed = value;
                changed = true;
            }
        }
        changed
    }

    /// Sets a body's bound values to what their expressions give now.
    fn settle_body(&self, uid: BodyUid, a: &mut BodyAttributes) {
        if let Some(text) = &a.material_expr {
            let new = text_of(self, text)
                .and_then(|name| self.material_named(&name))
                .ok();
            if new != a.material {
                // The new material's look, unless the body's appearance
                // was set by hand or follows its own expression (mitcad#640).
                if a.appearance_expr.is_none() {
                    let component = self.body_component_of(uid);
                    let old_look = self.material_look(a.material.as_deref(), component);
                    if a.appearance.is_none() || a.appearance == old_look {
                        a.appearance = self.material_look(new.as_deref(), component);
                    }
                }
                a.material = new;
            }
        }
        if let Some(text) = &a.appearance_expr {
            a.appearance = text_of(self, text)
                .and_then(|name| self.appearance_named(&name))
                .ok();
        }
        for (face, text) in &a.face_appearance_exprs {
            match text_of(self, text).and_then(|name| self.appearance_named(&name)) {
                Ok(id) => {
                    a.face_appearances.insert(face.clone(), id);
                }
                Err(_) => {
                    a.face_appearances.remove(face);
                }
            }
        }
        if let Some(text) = &a.visible_when
            && let Ok(visible) = holds(self, text)
        {
            a.visible = visible;
        }
    }

    /// Why a body's bindings do not give a value: one line each.
    pub fn body_binding_warnings(&self, uid: BodyUid) -> Vec<String> {
        let Some(a) = self.body_attributes.get(&uid) else {
            return Vec::new();
        };
        let mut warnings = Vec::new();
        if let Some(text) = &a.appearance_expr
            && let Err(e) = text_of(self, text).and_then(|name| self.appearance_named(&name))
        {
            warnings.push(format!("appearance: {e}; the body has the default look"));
        }
        if let Some(text) = &a.material_expr
            && let Err(e) = text_of(self, text).and_then(|name| self.material_named(&name))
        {
            warnings.push(format!(
                "material: {e}; the body has its component's material"
            ));
        }
        for (face, text) in &a.face_appearance_exprs {
            if let Err(e) = text_of(self, text).and_then(|name| self.appearance_named(&name)) {
                warnings.push(format!(
                    "appearance of face {face}: {e}; the face has the body's look"
                ));
            }
        }
        if let Some(text) = &a.visible_when
            && let Err(e) = holds(self, text)
        {
            warnings.push(format!("show when: {e}"));
        }
        warnings
    }

    /// Why a feature's suppression does not follow its condition.
    pub fn feature_binding_warning(&self, uid: FeatureUid) -> Option<String> {
        let text = self.bindings.suppressed_when.get(&uid)?;
        holds(self, text)
            .err()
            .map(|e| format!("suppress when: {e}"))
    }

    /// Why an occurrence's visibility does not follow its condition.
    pub fn occurrence_binding_warning(&self, uid: OccurrenceUid) -> Option<String> {
        let text = self.bindings.occurrence_visible_when.get(&uid)?;
        holds(self, text).err().map(|e| format!("show when: {e}"))
    }

    /// The parameters expressions name, each once, in their order.
    pub fn expression_parameters<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for text in texts {
            let Ok(expr) = Expr::parse(text) else {
                continue;
            };
            for name in expr.references() {
                if !names.iter().any(|n| n == name) {
                    names.push(name.to_owned());
                }
            }
        }
        names
    }

    /// The parameters a body's bindings use.
    pub fn body_binding_parameters(&self, uid: BodyUid) -> Vec<String> {
        self.body_attributes
            .get(&uid)
            .map(|a| Self::expression_parameters(a.binding_texts()))
            .unwrap_or_default()
    }

    /// The parameters that bindings use: a feature's own that a binding
    /// uses stays, as a user parameter, when the feature is deleted.
    pub(crate) fn binding_param_ids(&self) -> BTreeSet<ParamId> {
        let texts = self
            .body_attributes
            .values()
            .flat_map(BodyAttributes::binding_texts)
            .chain(self.bindings.suppressed_when.values().map(String::as_str))
            .chain(
                self.bindings
                    .occurrence_visible_when
                    .values()
                    .map(String::as_str),
            );
        Self::expression_parameters(texts)
            .iter()
            .filter_map(|name| self.parameters.find(name))
            .collect()
    }

    /// The bindings that use a parameter, for refusing its delete: `the
    /// appearance of Body1`, `the suppression of Extrude2`.
    pub(crate) fn binding_users(&self, name: &str) -> Vec<String> {
        let uses =
            |text: &String| Expr::parse(text).is_ok_and(|expr| expr.references().contains(&name));
        let mut users = Vec::new();
        for (uid, a) in &self.body_attributes {
            let body = self
                .body_names
                .get(uid)
                .cloned()
                .unwrap_or_else(|| uid.to_string());
            if a.appearance_expr.as_ref().is_some_and(uses) {
                users.push(format!("the appearance of {body}"));
            }
            if a.material_expr.as_ref().is_some_and(uses) {
                users.push(format!("the material of {body}"));
            }
            if a.face_appearance_exprs.values().any(uses) {
                users.push(format!("a face appearance of {body}"));
            }
            if a.visible_when.as_ref().is_some_and(uses) {
                users.push(format!("the visibility of {body}"));
            }
        }
        for (uid, text) in &self.bindings.suppressed_when {
            if uses(text) {
                let feature = self
                    .entry(*uid)
                    .map_or_else(|| uid.to_string(), |f| f.name.clone());
                users.push(format!("the suppression of {feature}"));
            }
        }
        for (uid, text) in &self.bindings.occurrence_visible_when {
            if uses(text) {
                users.push(format!(
                    "the visibility of {}",
                    self.assembly.occurrence_name(*uid)
                ));
            }
        }
        users
    }

    /// Rewrites the bindings' references to a renamed parameter.
    pub(crate) fn rename_in_bindings(&mut self, old: &str, new: &str) {
        self.edit_binding_texts(|text| {
            if let Ok(expr) = Expr::parse(text)
                && expr.references().contains(&old)
            {
                *text = replace_references(text, &expr, old, new);
            }
        });
    }

    /// Runs `edit` on every binding's expression.
    pub(crate) fn edit_binding_texts(&mut self, mut edit: impl FnMut(&mut String)) {
        for a in self.body_attributes.values_mut() {
            a.appearance_expr.iter_mut().for_each(&mut edit);
            a.material_expr.iter_mut().for_each(&mut edit);
            a.visible_when.iter_mut().for_each(&mut edit);
            a.face_appearance_exprs.values_mut().for_each(&mut edit);
        }
        self.bindings
            .suppressed_when
            .values_mut()
            .for_each(&mut edit);
        self.bindings
            .occurrence_visible_when
            .values_mut()
            .for_each(&mut edit);
    }

    /// A binding's expression as it is kept (trimmed, decimal points), or
    /// None for the empty text (no binding); refused when it does not
    /// evaluate, or gives a number where a text is wanted or the other way
    /// round. A text that names no appearance or material is accepted: the
    /// body shows the warning.
    pub(crate) fn checked_binding(
        &self,
        kind: BindingKind,
        text: &str,
    ) -> Result<Option<String>, ModelError> {
        let text = with_decimal_points(text.trim()).into_owned();
        if text.is_empty() {
            return Ok(None);
        }
        match kind {
            BindingKind::Appearance | BindingKind::Material => {
                text_of(self, &text).map_err(invalid)?;
            }
            BindingKind::Condition => {
                holds(self, &text).map_err(invalid)?;
            }
        }
        Ok(Some(text))
    }
}

impl<K: Kernel> Document<K> {
    /// The condition under which a feature is suppressed, if any.
    pub fn suppressed_when(&self, uid: FeatureUid) -> Option<&str> {
        self.state
            .bindings
            .suppressed_when
            .get(&uid)
            .map(String::as_str)
    }

    /// The condition under which an occurrence is shown, if any.
    pub fn occurrence_visible_when(&self, uid: OccurrenceUid) -> Option<&str> {
        self.state
            .bindings
            .occurrence_visible_when
            .get(&uid)
            .map(String::as_str)
    }

    /// Lets a body's attributes follow an expression: `change` sets it on a
    /// copy, which is then settled; one undo step named `verb` and the
    /// body's name, none when nothing changes.
    fn bind_body(
        &mut self,
        uid: BodyUid,
        verb: &str,
        change: impl FnOnce(&mut BodyAttributes),
    ) -> Result<(), ModelError> {
        if self.body_shape(uid).is_none() {
            return Err(invalid(format!(
                "body {uid} does not exist at the timeline marker"
            )));
        }
        let mut attributes = self.body_attributes(uid);
        change(&mut attributes);
        self.state.settle_body(uid, &mut attributes);
        self.set_attributes(uid, verb, attributes)
    }

    /// Lets a body's appearance follow an expression that gives an
    /// appearance id or name; the empty text removes the expression and
    /// keeps the appearance it gave.
    pub fn bind_body_appearance(
        &mut self,
        uid: BodyUid,
        expression: &str,
    ) -> Result<(), ModelError> {
        let text = self
            .state
            .checked_binding(BindingKind::Appearance, expression)?;
        let verb = if text.is_some() {
            "Set Appearance of"
        } else {
            "Clear Appearance Expression of"
        };
        self.bind_body(uid, verb, |a| a.appearance_expr = text)
    }

    /// Lets a body's material follow an expression that gives a material
    /// id or name; the empty text removes the expression and keeps the
    /// material it gave.
    pub fn bind_body_material(&mut self, uid: BodyUid, expression: &str) -> Result<(), ModelError> {
        let text = self
            .state
            .checked_binding(BindingKind::Material, expression)?;
        let verb = if text.is_some() {
            "Set Material of"
        } else {
            "Clear Material Expression of"
        };
        self.bind_body(uid, verb, |a| a.material_expr = text)
    }

    /// Lets a body's visibility follow a condition; the empty text removes
    /// the condition and keeps the visibility it gave.
    pub fn bind_body_visible(&mut self, uid: BodyUid, expression: &str) -> Result<(), ModelError> {
        let text = self
            .state
            .checked_binding(BindingKind::Condition, expression)?;
        let body = self.body_name(uid);
        let label = if text.is_some() {
            format!("Show {body} When")
        } else {
            format!("Clear Show When of {body}")
        };
        if self.body_shape(uid).is_none() {
            return Err(invalid(format!(
                "body {uid} does not exist at the timeline marker"
            )));
        }
        let mut attributes = self.body_attributes(uid);
        attributes.visible_when = text;
        self.state.settle_body(uid, &mut attributes);
        if attributes == self.body_attributes(uid) {
            return Ok(());
        }
        // As `set_body_visible` does.
        self.apply(|state| {
            if attributes.is_default() {
                state.body_attributes.remove(&uid);
            } else {
                state.body_attributes.insert(uid, attributes);
            }
            Ok((label, ()))
        })
    }

    /// Lets the appearance of faces of a body follow an expression (the
    /// faces as `set_face_appearance` takes them); the empty text removes
    /// the expression and keeps the appearance it gave.
    pub fn bind_face_appearance(
        &mut self,
        uid: BodyUid,
        faces: &[String],
        expression: &str,
    ) -> Result<(), ModelError> {
        let text = self
            .state
            .checked_binding(BindingKind::Appearance, expression)?;
        let names = self.checked_faces(uid, faces)?;
        let what = faces_label(names.len());
        let verb = if text.is_some() {
            format!("Set Appearance of {what} of")
        } else {
            format!("Clear Appearance Expression of {what} of")
        };
        self.bind_body(uid, &verb, |a| {
            for name in names {
                match &text {
                    Some(text) => {
                        a.face_appearance_exprs.insert(name, text.clone());
                    }
                    None => {
                        a.face_appearance_exprs.remove(&name);
                    }
                }
            }
        })
    }

    /// Lets a feature's suppression follow a condition; the empty text
    /// removes the condition and keeps the suppression it gave. The
    /// features from it on recompute when the suppression changes.
    pub fn bind_suppressed(&mut self, uid: FeatureUid, expression: &str) -> Result<(), ModelError> {
        let position = self.state.require(uid)?;
        self.state
            .editable(self.state.features[position].component)?;
        let text = self
            .state
            .checked_binding(BindingKind::Condition, expression)?;
        if self.state.bindings.suppressed_when.get(&uid) == text.as_ref() {
            return Ok(());
        }
        let name = self.state.features[position].name.clone();
        let label = if text.is_some() {
            format!("Suppress {name} When")
        } else {
            format!("Clear Suppress When of {name}")
        };
        // The suppression itself is settled with the other bindings.
        self.apply(|state| {
            match text {
                Some(text) => state.bindings.suppressed_when.insert(uid, text),
                None => state.bindings.suppressed_when.remove(&uid),
            };
            Ok((label, ()))
        })
    }

    /// Lets an occurrence's visibility follow a condition; the empty text
    /// removes the condition and keeps the visibility it gave.
    pub fn bind_occurrence_visible(
        &mut self,
        uid: OccurrenceUid,
        expression: &str,
    ) -> Result<(), ModelError> {
        self.state.require_occurrence(uid)?;
        let text = self
            .state
            .checked_binding(BindingKind::Condition, expression)?;
        if self.state.bindings.occurrence_visible_when.get(&uid) == text.as_ref() {
            return Ok(());
        }
        let name = self.state.assembly.occurrence_name(uid);
        let label = if text.is_some() {
            format!("Show {name} When")
        } else {
            format!("Clear Show When of {name}")
        };
        self.apply_with(|state| {
            match text {
                Some(text) => state.bindings.occurrence_visible_when.insert(uid, text),
                None => state.bindings.occurrence_visible_when.remove(&uid),
            };
            Ok((label, (), false))
        })
    }
}
