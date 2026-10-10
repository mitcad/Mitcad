// SPDX-License-Identifier: MIT
//! The report of a FreeCAD import: what each object of the document became
//! (a body, a component, occurrences) or why it did not, the bodies with
//! their measures, where they were placed, the other files read, and stored
//! shapes that may be out of date.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

/// What an object became.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectOutcome {
    /// A body: its stored shape in a base feature.
    Body,
    /// A component (App::Part, Assembly).
    Component,
    /// Occurrences of the linked object's component (App::Link).
    Occurrence,
    /// Part of another object's result (a feature in a Body, an operand
    /// of a boolean, an element of a link array), or a folder.
    Included,
    /// A Mitcad feature with everything FreeCAD's object says (a sketch).
    Parametric,
    /// A Mitcad feature with what could be carried over; the report says
    /// what was not.
    Partial,
    /// A feature of the history that could not be replayed or did not give
    /// FreeCAD's result: a base feature of its stored shape in its place,
    /// which the features after it continue on.
    Fallback,
    /// Left out, with the reason.
    #[default]
    Skipped,
}

impl ObjectOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Component => "component",
            Self::Occurrence => "occurrence",
            Self::Included => "included",
            Self::Parametric => "parametric",
            Self::Partial => "partial",
            Self::Fallback => "fallback",
            Self::Skipped => "skipped",
        }
    }
}

/// A replayed feature's result against the shape FreeCAD stored for it
/// (in the component's coordinates): the Mitcad bodies of its Body (or of
/// a Part workbench object) together, and FreeCAD's.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FeatureCheck {
    pub volume: f64,
    pub area: f64,
    pub center: [f64; 3],
    pub solids: usize,
    pub freecad_volume: f64,
    pub freecad_area: f64,
    pub freecad_center: [f64; 3],
    pub freecad_solids: usize,
    /// The largest of the relative volume and area differences and the
    /// centre's distance relative to the size.
    pub distance: f64,
    pub pass: bool,
}

/// A feature of the history (a PartDesign feature, a Part workbench
/// object, a datum): what it became on Mitcad's timeline.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FeatureReport {
    pub object: String,
    pub label: String,
    #[serde(rename = "type")]
    pub object_type: String,
    /// The PartDesign Body it is in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// `parametric`, `partial`, `fallback` or `skipped`.
    pub outcome: ObjectOutcome,
    /// The Mitcad features made of it (a definition, or the fallback's
    /// base feature).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// The Mitcad feature type.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mitcad: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<FeatureCheck>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A driving dimension made of a FreeCAD constraint, with the constraint's
/// name and expression.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DimensionSource {
    /// The constraint's index in the FreeCAD sketch.
    pub index: usize,
    /// The Mitcad dimension (`k7`).
    pub dimension: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    /// FreeCAD's value (millimetres, radians) and Mitcad's.
    pub value: f64,
    pub mitcad: f64,
    /// The dimension's Mitcad value when it is a parameter (the
    /// constraint's name) or an expression of parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<String>,
}

/// What a FreeCAD expression (or a quantity) became.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionOutcome {
    /// A parameter's expression.
    Parameter,
    /// The expression of a value of a Mitcad feature or sketch.
    Expression,
    /// FreeCAD's value: no expression, or one that does not translate.
    Value,
    /// Translated, but Mitcad's value differs from FreeCAD's: FreeCAD's
    /// value is kept.
    Mismatched,
    /// Bound to a property the import does not carry over.
    #[default]
    Unused,
    /// No parameter made.
    Skipped,
}

impl ExpressionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parameter => "parameter",
            Self::Expression => "expression",
            Self::Value => "value",
            Self::Mismatched => "mismatched",
            Self::Unused => "unused",
            Self::Skipped => "skipped",
        }
    }
}

/// A Mitcad parameter made of a FreeCAD quantity: a spreadsheet's cell, a
/// property (a VarSet's, or one that expressions refer to), a sketch's
/// constraint.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ParameterReport {
    /// Empty when none was made.
    pub name: String,
    /// `Spreadsheet.B1`, `VarSet.Depth`, `Sketch.Constraints[4]`.
    pub source: String,
    /// The FreeCAD object, and the cell, property path or constraint index.
    pub object: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<usize>,
    /// What it is, by labels (the parameter's comment).
    pub describe: String,
    /// FreeCAD's expression (a cell's formula without its `=`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freecad: Option<String>,
    /// Mitcad's expression and its value (millimetres, radians or a plain
    /// number) after the import.
    pub expression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// FreeCAD's value in the same units, when the file keeps one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freecad_value: Option<f64>,
    /// `parameter` (FreeCAD's expression translated), `value` (FreeCAD's
    /// value), `mismatched` or `skipped`.
    pub outcome: ExpressionOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// An expression FreeCAD binds to a property (`ExpressionEngine`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExpressionReport {
    pub object: String,
    /// The property path (`Length`, `.Constraints.width`).
    pub path: String,
    /// FreeCAD's expression.
    pub expression: String,
    /// Mitcad's expression, or the parameter it became.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mitcad: Option<String>,
    pub outcome: ExpressionOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A sketch's solution against its stored shape: the stored edges
/// projected into the sketch, each matched by a profile curve (not
/// construction) of the solution.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ShapeCheck {
    pub edges: usize,
    pub curves: usize,
    pub matched: usize,
    /// The largest distance of a matched edge from its curve, mm.
    pub deviation: f64,
    pub pass: bool,
}

/// A sketch of the document.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SketchReport {
    pub object: String,
    pub label: String,
    /// `parametric`, `partial` or `skipped`.
    pub outcome: ObjectOutcome,
    /// The Mitcad sketch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Where it lies (`on xy`, `on F1:import(5)`, `on a fixed plane F9`).
    pub plane: String,
    /// FreeCAD's geometry and constraints.
    pub geometry: usize,
    pub constraints: usize,
    /// The Mitcad sketch's entities, constraints and dimensions.
    pub entities: usize,
    pub mitcad_constraints: usize,
    pub dimensions: usize,
    /// How far the solved points are from FreeCAD's, mm.
    pub error: f64,
    /// What was left out or changed (the sketch is partial).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dimension_sources: Vec<DimensionSource>,
    /// Expressions FreeCAD binds to constraints (constraint index,
    /// expression); what they became is in `dimension_sources` and the
    /// import's `expressions`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<(usize, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<ShapeCheck>,
    /// The profile curves: how many, their length and world centre (by
    /// length), as FreeCAD measures the sketch's shape.
    pub edges: usize,
    pub length: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<[f64; 3]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectReport {
    /// The object's position in the document.
    pub index: usize,
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub object_type: String,
    pub outcome: ObjectOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The Mitcad bodies made of it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<String>,
    /// The Mitcad component made of it or that its body went into, when
    /// not the root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Its stored shape may be out of date (touched, invalid or failed).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
}

/// A body made of an object's stored shape, measured in its component's
/// coordinates.
#[derive(Debug, Clone, Serialize)]
pub struct BodyReport {
    /// The FreeCAD object.
    pub object: String,
    /// The document it is in, when not the imported one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub uid: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// `solid` or `sheet`.
    pub kind: String,
    pub volume: f64,
    pub area: f64,
    pub center: [f64; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f64; 3]>,
    pub visible: bool,
}

/// Where an object of the document shows its geometry in the design: a body
/// at its place, or what a component placed by an App::Part or a link (an
/// element of an array) holds; in world coordinates.
#[derive(Debug, Clone, Serialize)]
pub struct PlacedReport {
    pub object: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<usize>,
    /// The occurrence path (`Part:1/Link:2`), empty at the root.
    pub path: String,
    pub bodies: usize,
    pub volume: f64,
    pub area: f64,
    /// The centre of mass of the solids (by volume), else of the sheets
    /// (by area).
    pub center: [f64; 3],
    pub visible: bool,
    /// Bodies the history's replay made are among them (Mitcad rebuilt
    /// them, so they agree with FreeCAD's shapes to the replay's
    /// tolerance, not to the digits of the stored shape).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub replayed: bool,
    /// A replayed body's place: Mitcad's measures of the shape FreeCAD
    /// stored for the object there, in world coordinates (FreeCAD's own
    /// measures of some shapes, B-spline surfaces among them, are less
    /// exact).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stored: Option<StoredMeasures>,
    /// A place of shapes the import brought in as FreeCAD stored them: their
    /// measures with the kernel's plain fixed-point integration, as FreeCAD
    /// measures (`Kernel::fixed_point_properties`; Mitcad integrates faces
    /// bounded by B-splines of many spans more exactly, mitcad#139).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed: Option<StoredMeasures>,
}

/// Measures of a shape: the volume, area and world centre of mass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StoredMeasures {
    pub volume: f64,
    pub area: f64,
    pub center: [f64; 3],
}

/// The import against a dump of the document made by FreeCAD
/// (`tools/freecad-export/dump.py`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReferenceReport {
    /// Measures compared.
    pub checked: usize,
    pub differences: Vec<String>,
    /// Places that differ where FreeCAD's measures are no reference, with
    /// the reason: FreeCAD's shape is invalid, or FreeCAD's own measures of
    /// it disagree (its surface integrals against its mesh of the shape).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_compared: Vec<String>,
    pub pass: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FcstdReport {
    pub file: String,
    /// The document's label.
    pub label: String,
    /// The FreeCAD that saved it, as written.
    pub program_version: String,
    pub schema_version: u32,
    /// The document's unit system, when it has one (1.0 and later).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// Imported as bodies only, without the history.
    pub bodies_only: bool,
    pub objects_by_type: BTreeMap<String, usize>,
    pub items: Vec<ObjectReport>,
    pub bodies: Vec<BodyReport>,
    pub placed: Vec<PlacedReport>,
    /// The document's sketches (not with `bodies_only`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sketches: Vec<SketchReport>,
    /// The features of the history in the timeline's order (not with
    /// `bodies_only`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<FeatureReport>,
    /// The parameters made of FreeCAD's quantities, and every expression
    /// of the document with what it became (not with `bodies_only`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterReport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<ExpressionReport>,
    /// Components and occurrences made (the root excluded).
    pub components: usize,
    pub occurrences: usize,
    /// Other documents read for links to them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// Linked documents that were not found.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing_files: Vec<String>,
    /// Objects whose stored shape may be out of date.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<String>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<ReferenceReport>,
}

impl FcstdReport {
    /// Objects per outcome.
    pub fn counts(&self) -> BTreeMap<ObjectOutcome, usize> {
        let mut counts = BTreeMap::new();
        for item in &self.items {
            *counts.entry(item.outcome).or_default() += 1;
        }
        counts
    }

    /// A readable summary.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let counts = self.counts();
        let n = |o: ObjectOutcome| counts.get(&o).copied().unwrap_or(0);
        let _ = writeln!(
            out,
            "{} (FreeCAD {}, schema {}): {} objects: {} bodies, {} components, {} links, {} parametric, {} partial, {} fallback, {} included, {} skipped",
            self.file,
            self.program_version,
            self.schema_version,
            self.items.len(),
            n(ObjectOutcome::Body),
            n(ObjectOutcome::Component),
            n(ObjectOutcome::Occurrence),
            n(ObjectOutcome::Parametric),
            n(ObjectOutcome::Partial),
            n(ObjectOutcome::Fallback),
            n(ObjectOutcome::Included),
            n(ObjectOutcome::Skipped)
        );
        let translated = self
            .expressions
            .iter()
            .filter(|e| {
                matches!(
                    e.outcome,
                    ExpressionOutcome::Parameter | ExpressionOutcome::Expression
                )
            })
            .count();
        let _ = writeln!(
            out,
            "  {} bodies in {} components, {} occurrences{}{}",
            self.bodies.len(),
            self.components + 1,
            self.occurrences,
            self.units
                .as_deref()
                .map_or(String::new(), |u| format!("; units {u}")),
            if self.parameters.is_empty() && self.expressions.is_empty() {
                String::new()
            } else {
                format!(
                    "; {} parameters, {translated} of {} expressions translated",
                    self.parameters
                        .iter()
                        .filter(|p| !p.name.is_empty())
                        .count(),
                    self.expressions.len()
                )
            }
        );
        let types: Vec<String> = self
            .objects_by_type
            .iter()
            .map(|(t, n)| format!("{t} {n}"))
            .collect();
        let _ = writeln!(out, "  objects by type: {}", types.join(", "));
        for item in &self.items {
            let made = if item.bodies.is_empty() {
                String::new()
            } else {
                format!(" -> {}", item.bodies.join(", "))
            };
            let made = match &item.component {
                Some(c) => format!("{made} in {c}"),
                None => made,
            };
            let _ = writeln!(
                out,
                "  {:>4} {:<24} {:<26} {:<10}{made}{}{}",
                item.index,
                truncate(&item.label, 24),
                truncate(&item.object_type, 26),
                item.outcome.as_str(),
                if item.stale { " (stale shape)" } else { "" },
                item.note
                    .as_deref()
                    .map_or(String::new(), |n| format!(": {n}"))
            );
        }
        for body in &self.bodies {
            let _ = writeln!(
                out,
                "  body {} ({}): {}, volume {:.3} mm3, area {:.3} mm2{}{}",
                body.name,
                body.uid,
                body.kind,
                body.volume,
                body.area,
                if body.visible { "" } else { ", hidden" },
                body.file
                    .as_deref()
                    .map_or(String::new(), |f| format!(", from {f}"))
            );
        }
        for s in &self.sketches {
            let _ = writeln!(
                out,
                "  sketch {} ({}): {} {}, {} geometries and {} constraints -> {} entities, {} constraints, {} dimensions, error {:.1e} mm{}",
                s.label,
                s.feature.as_deref().unwrap_or("-"),
                s.outcome.as_str(),
                s.plane,
                s.geometry,
                s.constraints,
                s.entities,
                s.mitcad_constraints,
                s.dimensions,
                s.error,
                s.shape.as_ref().map_or(String::new(), |c| format!(
                    ", stored shape {} of {} edges{}",
                    c.matched,
                    c.edges,
                    if c.pass { "" } else { " (differs)" }
                ))
            );
            for d in &s.dropped {
                let _ = writeln!(out, "    left out: {d}");
            }
            for n in &s.notes {
                let _ = writeln!(out, "    note: {n}");
            }
        }
        for f in &self.features {
            let _ = writeln!(
                out,
                "  feature {} ({}): {} {}{}{}",
                f.label,
                if f.features.is_empty() {
                    "-".to_owned()
                } else {
                    f.features.join(", ")
                },
                f.outcome.as_str(),
                if f.mitcad.is_empty() {
                    String::new()
                } else {
                    format!("{} ", f.mitcad)
                },
                f.object_type,
                f.check.as_ref().map_or(String::new(), |c| format!(
                    ", volume {:.3} mm3 against FreeCAD's {:.3}, {} solids, off by {:.1e}{}",
                    c.volume,
                    c.freecad_volume,
                    c.solids,
                    c.distance,
                    if c.pass { "" } else { " (differs)" }
                ))
            );
            for n in &f.notes {
                let _ = writeln!(out, "    note: {n}");
            }
        }
        for p in &self.parameters {
            let _ = writeln!(
                out,
                "  parameter {} ({}): {} {}{}",
                if p.name.is_empty() { "-" } else { &p.name },
                p.describe,
                p.outcome.as_str(),
                p.expression,
                p.freecad
                    .as_deref()
                    .map_or(String::new(), |f| format!(" (FreeCAD: {f})"))
            );
            if let Some(n) = &p.note {
                let _ = writeln!(out, "    note: {n}");
            }
        }
        for e in &self.expressions {
            let _ = writeln!(
                out,
                "  expression {}.{} = {}: {}{}",
                e.object,
                e.path.trim_start_matches('.'),
                e.expression,
                e.outcome.as_str(),
                e.mitcad
                    .as_deref()
                    .map_or(String::new(), |m| format!(" {m}"))
            );
            if let Some(n) = &e.note {
                let _ = writeln!(out, "    note: {n}");
            }
        }
        for file in &self.files {
            let _ = writeln!(out, "  linked file: {file}");
        }
        for file in &self.missing_files {
            let _ = writeln!(out, "  linked file missing: {file}");
        }
        for warning in &self.warnings {
            let _ = writeln!(out, "  warning: {warning}");
        }
        if let Some(r) = &self.reference {
            let _ = writeln!(
                out,
                "  reference: {} measures compared, {} differ{}",
                r.checked,
                r.differences.len(),
                if r.pass {
                    ""
                } else {
                    " (reference check failed)"
                }
            );
            for d in &r.differences {
                let _ = writeln!(out, "    {d}");
            }
            for n in &r.not_compared {
                let _ = writeln!(out, "    not compared: {n}");
            }
        }
        out
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else {
        let mut t: String = text.chars().take(width - 1).collect();
        t.push('~');
        t
    }
}
