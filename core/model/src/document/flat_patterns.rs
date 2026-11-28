// SPDX-License-Identifier: MIT
//! Flat Patterns kept in the document (mitcad#208): one derived item per
//! sheet-metal body, saved in the design file. Setting or removing one is an
//! undo step that recomputes nothing; the pattern itself is evaluated from
//! the body's current sheet state at the timeline marker whenever it is
//! asked for, so edits made after it was set are always included. It is not
//! a body: occurrences, body lists and physical properties never see it.

use std::sync::Arc;

use serde_json::{Value, json};

use super::{Added, Document, ModelError, PreviewReport, invalid};
use crate::features::{FeatureDef, ValueInput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::sheet::flat::{self, FlatFailure, FlatPattern, FlatPatternDef};
use crate::sheet::process::SheetProcess;
use crate::sheet::{EdgeChange, FeatureWords, PanelId, SheetDomainRef, SheetReferenceSide};

/// The last evaluation of each body's Flat Pattern, by its fingerprint. It
/// is separate from modelling results: evaluating a pattern changes no state.
pub(crate) type FlatCache<S> =
    std::sync::Mutex<std::collections::BTreeMap<BodyUid, (u128, FlatOutcome<S>)>>;
pub(crate) type FlatOutcome<S> = Arc<Result<FlatPattern<S>, FlatFailure>>;

/// A refusal a person reads as `shown`, in words, and agents read as
/// `detail`, with the body's and the features' ids (mitcad#1639, the rule
/// of mitcad#1327).
fn told(shown: String, detail: String) -> ModelError {
    invalid(
        crate::notice::FeatureError::new(shown)
            .with_detail(detail)
            .text(),
    )
}

impl<K: Kernel> Document<K> {
    /// The Flat Pattern settings, by body.
    pub fn flat_pattern_defs(&self) -> &std::collections::BTreeMap<BodyUid, FlatPatternDef> {
        &self.state.flat_patterns
    }

    /// Creates or changes the Flat Pattern of a sheet-metal body. Without a
    /// fixed panel it keeps the current one, or takes the body's first panel.
    pub fn set_flat_pattern(
        &mut self,
        body: BodyUid,
        stationary: Option<PanelId>,
        x_axis: Option<flat::FlatXAxis>,
        reference_side: Option<SheetReferenceSide>,
        shown: Option<bool>,
    ) -> Result<(), ModelError> {
        let old = self.state.flat_patterns.get(&body).cloned();
        let Some(state) = self.last_result().sheets.get(&body) else {
            return Err(self.not_a_sheet(body));
        };
        let panels: Vec<PanelId> = state
            .value
            .charts
            .iter()
            .filter_map(|p| match p.domain {
                SheetDomainRef::Panel(id) => Some(id),
                SheetDomainRef::Bend(_) => None,
            })
            .collect();
        let stationary = match stationary.or(old.as_ref().map(|d| d.stationary)) {
            Some(panel) => panel,
            None => *panels.iter().min().ok_or_else(|| self.no_panel(body))?,
        };
        if !panels.contains(&stationary) {
            let detail = format!(
                "panel {} slot {} is not a panel of sheet {body}",
                stationary.feature, stationary.slot
            );
            return Err(told(
                match self.flat_body_name(body) {
                    Some(name) => format!("the chosen fixed panel is not a panel of {name}"),
                    None => "the chosen fixed panel is not a panel of this sheet".to_owned(),
                },
                detail,
            ));
        }
        let def = FlatPatternDef {
            stationary,
            x_axis: x_axis
                .or(old.as_ref().map(|d| d.x_axis))
                .unwrap_or_default(),
            reference_side: reference_side
                .or(old.as_ref().map(|d| d.reference_side))
                .unwrap_or(SheetReferenceSide::Front),
            shown: shown.or(old.as_ref().map(|d| d.shown)).unwrap_or(false),
        };
        if old.as_ref() == Some(&def) {
            return Ok(());
        }
        let label = match &old {
            // Only the display choice changed (mitcad#210).
            Some(old)
                if FlatPatternDef {
                    shown: def.shown,
                    ..old.clone()
                } == def =>
            {
                if def.shown {
                    "Show Flat Pattern"
                } else {
                    "Show Folded Body"
                }
            }
            Some(_) => "Edit Flat Pattern",
            None => "Create Flat Pattern",
        };
        self.apply_with(|state| {
            state.flat_patterns.insert(body, def);
            Ok((label.to_owned(), (), false))
        })
    }

    pub fn remove_flat_pattern(&mut self, body: BodyUid) -> Result<(), ModelError> {
        if !self.state.flat_patterns.contains_key(&body) {
            let detail = format!("body {body} has no Flat Pattern");
            return Err(told(
                match self.flat_body_name(body) {
                    Some(name) => format!("{name} has no Flat Pattern"),
                    None => "this body has no Flat Pattern".to_owned(),
                },
                detail,
            ));
        }
        self.apply_with(|state| {
            state.flat_patterns.remove(&body);
            Ok(("Delete Flat Pattern".to_owned(), (), false))
        })
    }

    /// The evaluated Flat Pattern of a body at the marker, or why there is
    /// none. Repeated requests reuse the evaluation while nothing it
    /// depends on changed.
    pub fn flat_pattern(&self, body: BodyUid) -> FlatOutcome<K::Shape> {
        let Some(def) = self.state.flat_patterns.get(&body) else {
            return Arc::new(Err(FlatFailure::told(
                "unavailable",
                "this body has no Flat Pattern".to_owned(),
                format!("body {body} has no Flat Pattern"),
            )));
        };
        self.flat_pattern_with(body, def)
    }

    /// A body's Flat Pattern evaluated with these settings, kept as
    /// [`Document::flat_pattern`] keeps its item's (the last one per body,
    /// by everything it is made of).
    fn flat_pattern_with(&self, body: BodyUid, def: &FlatPatternDef) -> FlatOutcome<K::Shape> {
        let result = self.last_result();
        let name = |feature: FeatureUid| self.flat_feature_name(feature);
        let key = flat::fingerprint(result, body, def, &name);
        if let Ok(cache) = self.flat_cache.lock()
            && let Some((cached, outcome)) = cache.get(&body)
            && *cached == key
        {
            return outcome.clone();
        }
        let edges = |feature: FeatureUid| self.flat_feature_edges(feature);
        let words = FeatureWords {
            name: &name,
            edges: &edges,
        };
        let outcome = Arc::new(flat::evaluate(&self.kernel, result, body, def, &words));
        if let Ok(mut cache) = self.flat_cache.lock() {
            cache.insert(body, (key, outcome.clone()));
        }
        outcome
    }

    /// A feature's name as the timeline shows it, for the reason of a
    /// Flat Pattern whose sheet is not valid (mitcad#1636).
    fn flat_feature_name(&self, feature: FeatureUid) -> String {
        self.feature(feature)
            .map_or_else(|| feature.to_string(), |f| f.name.clone())
    }

    /// What a fillet or chamfer the sheet keeps does to its edges, for the
    /// reason of a Flat Pattern that cannot develop it (mitcad#1942).
    fn flat_feature_edges(&self, feature: FeatureUid) -> EdgeChange {
        self.feature(feature)
            .map_or(EdgeChange::Other, |f| EdgeChange::of(&f.def))
    }

    /// A body as a Flat Pattern's message names it (mitcad#1639): its name
    /// as the Browser shows it; none when it has no name at the marker
    /// (the message then says "this sheet" or "this body", never its id).
    fn flat_body_name(&self, body: BodyUid) -> Option<String> {
        let name = self.body_name(body);
        (name != body.to_string()).then_some(name)
    }

    /// The refusal of a body that is no sheet at the marker.
    fn not_a_sheet(&self, body: BodyUid) -> ModelError {
        let detail = format!("body {body} is not a sheet-metal body at the timeline marker");
        told(
            match self.flat_body_name(body) {
                Some(name) => format!("{name} is not a sheet-metal body at the timeline marker"),
                None => "this body is not a sheet-metal body at the timeline marker".to_owned(),
            },
            detail,
        )
    }

    /// The refusal of a sheet without a panel.
    fn no_panel(&self, body: BodyUid) -> ModelError {
        let detail = format!("sheet {body} has no panel");
        told(
            match self.flat_body_name(body) {
                Some(name) => format!("{name} has no panel"),
                None => "this sheet has no panel".to_owned(),
            },
            detail,
        )
    }

    /// Why a body's Flat Pattern cannot be exported, as a person reads it
    /// (mitcad#1639): the body by its name, the reasons in words; one
    /// whole message per status. Agents read the body's id and the
    /// reasons with their ids in the detail.
    fn flat_refusal(&self, body: BodyUid, failure: &FlatFailure) -> ModelError {
        let reasons = failure.diagnostics.join("; ");
        let shown = match (self.flat_body_name(body), failure.status) {
            (Some(name), "blocked") => format!("the Flat Pattern of {name} is blocked: {reasons}"),
            (Some(name), "unavailable") => {
                format!("the Flat Pattern of {name} is unavailable: {reasons}")
            }
            (Some(name), _) => format!("the Flat Pattern of {name} is invalid: {reasons}"),
            (None, "blocked") => format!("the Flat Pattern of this sheet is blocked: {reasons}"),
            (None, "unavailable") => {
                format!("the Flat Pattern of this sheet is unavailable: {reasons}")
            }
            (None, _) => format!("the Flat Pattern of this sheet is invalid: {reasons}"),
        };
        let detail = format!(
            "the Flat Pattern of {body} is {}: {}",
            failure.status,
            failure.agent_reasons()
        );
        told(shown, detail)
    }

    /// The developed solid of a body's valid Flat Pattern, for display.
    pub fn flat_pattern_shape(&self, body: BodyUid) -> Option<K::Shape> {
        match &*self.flat_pattern(body) {
            Ok(pattern) => Some(pattern.solid.clone()),
            Err(_) => None,
        }
    }

    /// The `flat_pattern` query: one body's pattern, or all of them.
    pub fn flat_pattern_json(&self, body: Option<BodyUid>) -> Value {
        let one = |body: BodyUid| {
            let mut value = match &*self.flat_pattern(body) {
                Ok(pattern) => pattern.json(),
                Err(failure) => flat::failure_json(body, failure),
            };
            if let Some(def) = self.state.flat_patterns.get(&body) {
                value["settings"] = json!(def);
            }
            value
        };
        match body {
            Some(body) => one(body),
            None => Value::Array(self.state.flat_patterns.keys().map(|b| one(*b)).collect()),
        }
    }

    /// The settings a body's Flat Pattern has, or those a new one would
    /// get (its lowest panel, the panel's x axis, the front side).
    fn flat_settings(&self, body: BodyUid) -> Result<FlatPatternDef, ModelError> {
        if let Some(def) = self.state.flat_patterns.get(&body) {
            return Ok(def.clone());
        }
        let state = self
            .last_result()
            .sheets
            .get(&body)
            .ok_or_else(|| self.not_a_sheet(body))?;
        let stationary = state
            .value
            .charts
            .iter()
            .filter_map(|p| match p.domain {
                SheetDomainRef::Panel(id) => Some(id),
                SheetDomainRef::Bend(_) => None,
            })
            .min()
            .ok_or_else(|| self.no_panel(body))?;
        Ok(FlatPatternDef {
            stationary,
            x_axis: flat::FlatXAxis::PanelX,
            reference_side: SheetReferenceSide::Front,
            shown: false,
        })
    }

    /// The developed blank of a sheet-metal body as the Sheet Metal list
    /// and cut sheets take it (mitcad#1980): its Flat Pattern with its own
    /// settings and side, or a new item's settings when it has none. A
    /// body that is no sheet, or a pattern that is not valid, is the error,
    /// in words (the ids in its detail, as `export_flat_pattern_dxf`'s).
    pub fn developed_flat_pattern(
        &self,
        body: BodyUid,
    ) -> Result<FlatOutcome<K::Shape>, ModelError> {
        self.evaluate_flat_pattern(body, None)
            .map(|(_, pattern)| pattern)
    }

    /// The body's valid Flat Pattern with an optional export-side override.
    fn evaluate_flat_pattern(
        &self,
        body: BodyUid,
        reference_side: Option<SheetReferenceSide>,
    ) -> Result<(FlatPatternDef, FlatOutcome<K::Shape>), ModelError> {
        let mut def = self.flat_settings(body)?;
        // The item's own evaluation is cached; other settings are not.
        let own = self.state.flat_patterns.contains_key(&body)
            && reference_side.is_none_or(|side| side == def.reference_side);
        if let Some(side) = reference_side {
            def.reference_side = side;
        }
        let outcome = if own {
            self.flat_pattern(body)
        } else if reference_side.is_none() && !self.state.flat_patterns.contains_key(&body) {
            // A body without an item, as the Sheet Metal list, cut sheets
            // and drawings' Flat Pattern views take it (mitcad#2280: a view
            // asks for it many times each time it is drawn): kept as an
            // item's evaluation is.
            self.flat_pattern_with(body, &def)
        } else {
            Arc::new(flat::evaluate(
                &self.kernel,
                self.last_result(),
                body,
                &def,
                &FeatureWords {
                    name: &|feature| self.flat_feature_name(feature),
                    edges: &|feature| self.flat_feature_edges(feature),
                },
            ))
        };
        match &*outcome {
            Ok(_) => Ok((def, outcome)),
            Err(failure) => Err(self.flat_refusal(body, failure)),
        }
    }

    /// The manufacturing table of a sheet's current Flat Pattern.
    pub fn bend_table(
        &self,
        body: BodyUid,
        reference_side: Option<SheetReferenceSide>,
    ) -> Result<(FlatPatternDef, crate::sheet::bend_table::BendTable), ModelError> {
        let (def, outcome) = self.evaluate_flat_pattern(body, reference_side)?;
        let pattern = outcome.as_ref().as_ref().expect("a valid Flat Pattern");
        let table = crate::sheet::bend_table::BendTable::from_bends(
            &self.body_name(body),
            &self.flat_feature_name(pattern.developed.rules),
            pattern.thickness,
            &pattern.bends,
        );
        Ok((def, table))
    }

    /// Writes the Flat Pattern of a sheet-metal body to a DXF file
    /// (mitcad#210): the cut contours and bend lines of its evaluated outline
    /// with the item's settings (or a new item's, when the body has none),
    /// viewed from `reference_side` when given. Only a valid pattern is
    /// written; otherwise the reasons are the error. Returns the drawing.
    pub fn export_flat_pattern_dxf(
        &self,
        body: BodyUid,
        path: &str,
        reference_side: Option<SheetReferenceSide>,
        options: &crate::sheet::flat_dxf::FlatDxfOptions,
        write: &mitcad_dxf::WriteOptions,
    ) -> Result<(FlatPatternDef, mitcad_dxf::Drawing), ModelError> {
        let (def, outcome) = self.evaluate_flat_pattern(body, reference_side)?;
        let pattern = outcome.as_ref().as_ref().expect("a valid Flat Pattern");
        // The layers of the sheet's process profile (mitcad#1939).
        let options = crate::sheet::flat_dxf::FlatDxfOptions {
            process: self.sheet_process_of_rules(pattern.developed.rules),
            ..*options
        };
        // A triangulated pattern's fold lines as bend lines, when asked for
        // (mitcad#2442).
        let mut bends = pattern.bends.clone();
        if options.folds {
            bends.extend(crate::sheet::folded::triangulated::fold_bends(pattern));
        }
        let drawing = crate::sheet::flat_dxf::drawing_with_forms(
            &pattern.regions,
            &bends,
            &pattern.facet_lines,
            &pattern.beads,
            &options,
        )
        .map_err(|e| {
            // The walls it names by their features, the rest in words
            // (mitcad#1639); the ids for agents.
            let named = |feature: FeatureUid| self.flat_feature_name(feature);
            let error =
                crate::notice::FeatureError::from_text(&crate::sheet::name_walls(&e, &named));
            let detail = format!(
                "the Flat Pattern of {body}: {}",
                crate::notice::FeatureError::from_text(&e).agent_text()
            );
            told(
                match self.flat_body_name(body) {
                    Some(name) => format!("the Flat Pattern of {name}: {}", error.message),
                    None => format!("the Flat Pattern of this sheet: {}", error.message),
                },
                detail,
            )
        })?;
        mitcad_dxf::write_file(&drawing, path, write)
            .map_err(|e| invalid(format!("cannot write {path}: {e}")))?;
        Ok((def, drawing))
    }

    /// The `flat_point` query: a flat position on the body, or the flat
    /// position of a body point.
    pub fn flat_point_json(
        &self,
        body: BodyUid,
        flat: Option<[f64; 2]>,
        model: Option<[f64; 3]>,
    ) -> Result<Value, String> {
        let outcome = self.flat_pattern(body);
        let pattern = match &*outcome {
            Ok(pattern) => pattern,
            Err(failure) => return Err(failure.agent_reasons()),
        };
        let state = &self
            .last_result()
            .sheets
            .get(&body)
            .ok_or("the body has no sheet state")?
            .value;
        pattern.locate(state, flat, model)
    }

    /// The sheet state of a body at the marker and its thickness.
    fn sheet_at(&self, body: BodyUid) -> Result<(&crate::sheet::SheetState, f64), String> {
        let result = self.last_result();
        let state = result
            .sheets
            .get(&body)
            .filter(|_| result.bodies.contains_key(&body))
            .ok_or_else(|| {
                format!("body {body} is not a sheet-metal body at the timeline marker")
            })?;
        let thickness = result
            .sheet_rules
            .get(&state.value.rules)
            .map(|r| r.value.thickness)
            .ok_or_else(|| format!("the rules of sheet {body} are not available"))?;
        Ok((&state.value, thickness))
    }

    /// The `sheet_reference` query (mitcad#210): a face or edge of a sheet
    /// body picked in the view as the semantic reference sheet features
    /// take (its panel or bend, the boundary along a wall, the corner of
    /// two walls); `at` chooses among pieces of one curve.
    pub fn sheet_reference_json(
        &self,
        body: BodyUid,
        face: Option<&str>,
        edge: Option<&str>,
        at: Option<[f64; 3]>,
    ) -> Result<Value, String> {
        let (state, thickness) = self.sheet_at(body)?;
        let answer = crate::sheet::pick::reference(state, thickness, face, edge, at);
        // An edge named after an ordinary feature on a sheet (mitcad#242):
        // a corner found by where the edge runs.
        if answer.is_err()
            && face.is_none()
            && let Some(name) = edge
            && let Ok(name) = name.parse::<crate::topo::EdgeName>()
            && let Some(shape) = self.last_result().bodies.get(&body)
            && let Ok(curve) = self.kernel().edge_geometry(&shape.value, &name)
        {
            let straight = matches!(curve, crate::datum::CurveGeometry::Line { .. });
            let (start, end) = match curve {
                crate::datum::CurveGeometry::Line { start, end }
                | crate::datum::CurveGeometry::Other { start, end, .. } => (start, end),
                crate::datum::CurveGeometry::Circle { .. } => return answer,
            };
            if let Ok(mut corner) = crate::sheet::pick::corner_at(state, thickness, start, end) {
                corner["body"] = serde_json::json!(body);
                return Ok(corner);
            }
            // A straight edge on a panel's surface along a boundary
            // (mitcad#2117: the outline of material a Join added to a
            // panel, which keeps the Join's own face names).
            if straight
                && let Ok(mut found) = crate::sheet::pick::boundary_at(state, thickness, start, end)
            {
                found["body"] = serde_json::json!(body);
                return Ok(found);
            }
        }
        // A planar face named after an ordinary feature on a sheet
        // (mitcad#2117): the panel whose surface holds the point picked.
        if answer.is_err()
            && edge.is_none()
            && face.is_some()
            && let Some(at) = at
            && let Ok(found) = crate::sheet::pick::panel_at(state, thickness, at)
        {
            return Ok(found);
        }
        // The Sheet commands show the refusal of a pick in their panels
        // (mitcad#1942): walls and features by their names.
        answer.map_err(|e| crate::sheet::worded(&e, &|f| self.flat_feature_name(f)))
    }

    /// The `sheet_corner_edges` query (mitcad#1766): the thickness edge
    /// each stored corner of a Corner Round or Corner Chamfer lies on in
    /// the body at the marker (between two walls, or between a wall and
    /// the side of a bend for a corner beside a bend), null for a corner
    /// the body has no edge for. An edit highlights the corners by them.
    pub fn sheet_corner_edges_json(
        &self,
        body: BodyUid,
        corners: &[crate::features::sheet_folded::SheetCorner],
    ) -> Result<Value, String> {
        let (state, thickness) = self.sheet_at(body)?;
        let shape = self.last_result().bodies.get(&body).map(|s| &s.value);
        let names: Vec<String> = shape
            .and_then(|s| self.kernel().edges(s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|edge| edge.name)
            .collect();
        // A point on an edge: its middle, which chooses among pieces.
        let at = |name: &str| {
            let parsed = name.parse::<crate::topo::EdgeName>().ok()?;
            let half = crate::datum::PathParameter::Fraction(0.5);
            let on = self
                .kernel()
                .path_point(shape?, std::slice::from_ref(&parsed), half)
                .ok()?;
            Some(on.point)
        };
        let edges: Vec<Option<String>> = corners
            .iter()
            .map(|corner| crate::sheet::pick::corner_edge(state, thickness, corner, &names, &at))
            .collect();
        Ok(json!({"body": body, "edges": edges}))
    }

    /// The `sheet_flange_corners` query (mitcad#1877): the corners a
    /// flange on the boundaries `edges` of the sheet `body` would have
    /// (each pair of them that meets, by their indices, and the corner's
    /// point), for the Flange panel's per-corner Miter.
    pub fn sheet_flange_corners_json(
        &self,
        body: BodyUid,
        edges: &[crate::sheet::BoundaryRef],
    ) -> Result<Value, String> {
        let (state, thickness) = self.sheet_at(body)?;
        let corners = crate::sheet::folded::flange_edges::edge_corners(state, thickness, edges)?;
        let corners: Vec<Value> = corners
            .into_iter()
            .map(|(edges, at)| json!({"edges": edges, "at": at}))
            .collect();
        Ok(json!({"body": body, "corners": corners}))
    }

    /// The `sheet_seam` query (mitcad#210): the boundaries of the seam two
    /// panels of a sheet share, which a Bend folds.
    pub fn sheet_seam_json(
        &self,
        body: BodyUid,
        stationary: PanelId,
        moving: PanelId,
    ) -> Result<Value, String> {
        let (state, _) = self.sheet_at(body)?;
        // Bend's panel shows the refusal (mitcad#1942): walls by their
        // features' names.
        crate::sheet::pick::seam(state, stationary, moving)
            .map_err(|e| crate::sheet::worded(&e, &|f| self.flat_feature_name(f)))
    }

    /// The process profile of sheet rules (mitcad#1939): the one they name,
    /// else the one their component's material class gives
    /// ([`SheetProcess::from_class`]), else metal.
    pub fn sheet_process_of_rules(&self, rules: FeatureUid) -> SheetProcess {
        self.sheet_process_source(rules).0
    }

    /// The process profile of a body (mitcad#1939): its sheet rules', or
    /// for a body that is no sheet its component's material's, else metal.
    pub fn sheet_process_of_body(&self, body: BodyUid) -> SheetProcess {
        if let Some(state) = self.last_result().sheets.get(&body) {
            return self.sheet_process_of_rules(state.value.rules);
        }
        self.body_component(body)
            .and_then(|component| self.component_process(component))
            .unwrap_or_default()
    }

    /// A sketch's curves and text outlines as 3D polylines in its
    /// component's coordinates (mitcad#1939: the marks of a cut sheet's part
    /// and of a die line), construction curves left out; None for a
    /// feature that is no evaluated sketch.
    pub fn sketch_mark_polylines(&self, sketch: FeatureUid) -> Option<Vec<Vec<[f64; 3]>>> {
        let output = self.sketch_output(sketch)?;
        let construction: Vec<crate::ids::EntityUid> = match self.feature(sketch).map(|f| &f.def) {
            Some(FeatureDef::Sketch(def)) => def
                .entities
                .iter()
                .filter(|e| e.construction)
                .map(|e| e.id)
                .collect(),
            _ => Vec::new(),
        };
        let frame = output.frame;
        let mut out = Vec::new();
        for (id, curve) in &output.solved.curves {
            if construction.contains(id) {
                continue;
            }
            let (t0, t1) = curve.domain();
            out.push(
                curve
                    .sample(t0, t1)
                    .iter()
                    .map(|p| frame.point(*p))
                    .collect(),
            );
        }
        for text in output.texts.iter() {
            for contour in &text.contours {
                for part in &contour.parts {
                    let (t0, t1) = part.domain();
                    out.push(
                        part.sample(t0, t1)
                            .iter()
                            .map(|p| frame.point(*p))
                            .collect(),
                    );
                }
            }
        }
        Some(out)
    }

    /// The profile of sheet rules and where it comes from: `rules`,
    /// `material` or `default`.
    fn sheet_process_source(&self, rules: FeatureUid) -> (SheetProcess, &'static str) {
        let Some(entry) = self.feature(rules) else {
            return (SheetProcess::Metal, "default");
        };
        if let FeatureDef::SheetRules(def) = &entry.def
            && let Some(process) = def.process
        {
            return (process, "rules");
        }
        self.component_process(entry.component)
            .map_or((SheetProcess::Metal, "default"), |p| (p, "material"))
    }

    /// The profile a component's material class gives, if any.
    fn component_process(&self, component: ComponentUid) -> Option<SheetProcess> {
        let material = self.material(self.component_material(component)?)?;
        crate::material::material_sheet_process(material)
    }

    /// The `sheet_process` query (mitcad#1939): the process profile of a
    /// component's sheet part (the active component's when None): its
    /// newest sheet rules at the marker, else its material's class; and the
    /// commands the profiles filter, each available or not with the reason
    /// in words. `rules` is null when the component has no sheet rules
    /// (then nothing is filtered: every command is available).
    pub fn sheet_process_json(&self, component: Option<ComponentUid>) -> Value {
        let component = component.unwrap_or(self.state.assembly.active);
        let rules = self
            .features()
            .take(self.marker())
            .filter(|f| {
                f.component == component
                    && !f.suppressed
                    && matches!(f.def, FeatureDef::SheetRules(_))
            })
            .last()
            .map(|f| f.uid);
        let (process, source) = match rules {
            Some(rules) => self.sheet_process_source(rules),
            None => self
                .component_process(component)
                .map_or((SheetProcess::Metal, "default"), |p| (p, "material")),
        };
        let mut commands = crate::sheet::process::commands_json(process);
        if rules.is_none()
            && let Some(list) = commands.as_array_mut()
        {
            for command in list {
                *command = json!({"command": command["command"], "available": true});
            }
        }
        json!({
            "component": component,
            "rules": rules,
            "process": process.id(),
            "label": process.label(),
            "source": source,
            "commands": commands,
            "boards": crate::sheet::process::BOARD_PRESETS,
        })
    }

    /// The `sheet_bodies` query (mitcad#210): each sheet-metal body at the
    /// marker with its rules, its domains and its Flat Pattern's state, for
    /// the browser.
    pub fn sheet_bodies_json(&self) -> Value {
        let result = self.last_result();
        let rows = result
            .sheets
            .iter()
            .filter(|(body, _)| result.bodies.contains_key(body))
            .map(|(body, state)| {
                let state = &state.value;
                let rules = result.sheet_rules.get(&state.rules).map(|r| &r.value);
                let panels = state
                    .charts
                    .iter()
                    .filter(|p| matches!(p.domain, SheetDomainRef::Panel(_)))
                    .count();
                let flat = self.state.flat_patterns.get(body).map(|def| {
                    let mut value = match &*self.flat_pattern(*body) {
                        Ok(pattern) => {
                            let mut value = json!({"status": "valid", "area": pattern.area,
                                                   "bends": pattern.bends.len()});
                            // How approximate a triangulated pattern is
                            // (mitcad#2442), without its fold lines.
                            if let Some(mut a) = pattern.approximation() {
                                if let Some(a) = a.as_object_mut() {
                                    a.remove("folds");
                                }
                                value["approximation"] = a;
                            }
                            value
                        }
                        Err(failure) => json!({"status": failure.status,
                                               "diagnostics": failure.diagnostics}),
                    };
                    value["shown"] = json!(def.shown);
                    value["stationary"] = json!(def.stationary);
                    value["reference_side"] = json!(def.reference_side);
                    value
                });
                let k = rules.and_then(|r| match r.allowance {
                    crate::features::sheet::Allowance::KFactor { k } => Some(k),
                    _ => None,
                });
                json!({
                    "body": body,
                    "name": self.body_name(*body),
                    "status": state.status(),
                    "rules": state.rules,
                    "rules_name": self.feature(state.rules).map(|f| f.name.clone()),
                    "thickness": rules.map(|r| r.thickness),
                    "bend_radius": rules.map(|r| r.bend_radius),
                    "k_factor": k,
                    "allowance": rules.map(|r| r.allowance.kind()),
                    "process": self.sheet_process_of_rules(state.rules).id(),
                    "kerf": rules.map(|r| r.kerf.unwrap_or(0.0)),
                    "panels": panels,
                    "bends": state.bends.len(),
                    "open": state.opened.len(),
                    "flat_pattern": flat,
                })
            })
            .collect();
        Value::Array(rows)
    }

    /// Adds a Face with new rules of its own (mitcad#210): the rules feature
    /// just before it, both in one undo step named after the Face. `face`
    /// makes the Face's definition for the rules' uid.
    pub fn add_face_with_rules(
        &mut self,
        rules: &FeatureDef<ValueInput>,
        face: &dyn Fn(FeatureUid) -> Result<FeatureDef<ValueInput>, ModelError>,
        name: Option<&str>,
        component: Option<ComponentUid>,
    ) -> Result<Added, ModelError> {
        let component = component.unwrap_or(self.state.assembly.active);
        self.state.editable(component)?;
        self.apply(|state| {
            let made = state.add_feature_in(rules, None, component)?;
            let def = face(made.uid)?;
            let added = state.add_feature_in(&def, name, component)?;
            Ok((format!("Add {}", added.name), added))
        })
    }

    /// Previews a Face with new rules of its own: both features on a copy,
    /// reported as the Face.
    pub fn preview_face_with_rules(
        &mut self,
        rules: &FeatureDef<ValueInput>,
        face: &dyn Fn(FeatureUid) -> Result<FeatureDef<ValueInput>, ModelError>,
        name: Option<&str>,
    ) -> Result<PreviewReport, ModelError> {
        let mut state = self.state.clone();
        let made = state.add_feature(rules, None)?;
        let def = face(made.uid)?;
        self.check_profiles(&def)?;
        let added = state.add_feature(&def, name)?;
        self.preview_state(state, added.uid)
    }
}

// The sheet state and rules at the marker (mitcad#778: the api's
// `sheet_state` and `sheet_rules` queries answer with these, and the import
// calls them without the api).

impl<K: Kernel> Document<K> {
    /// The `sheet_state` query: one sheet body's state at the marker, or
    /// every sheet body's, as the query answers it.
    pub fn sheet_state_json(&self, body: Option<BodyUid>) -> Value {
        crate::sheet::state_query(self.last_result(), body)
    }

    /// The `sheet_rules` query: one sheet-metal rules feature's rules at
    /// the marker, or every one's, as the query answers it.
    pub fn sheet_rules_json(&self, feature: Option<FeatureUid>) -> Value {
        crate::sheet::rules_query(self.last_result(), feature)
    }
}
