// SPDX-License-Identifier: MIT
//! Analyses kept in the document (mitcad#41): section analyses, listed in
//! the browser's Analysis folder and saved in the project file. They
//! change no geometry: adding, editing, renaming, showing, hiding and
//! deleting one is an undo step that recomputes nothing.
//!
//! An analysis refers to geometry as features do (a plane reference), so
//! a face reference follows the model by its topological name; it is
//! resolved at the timeline marker whenever it is asked for, and an
//! analysis whose plane is gone says why instead of a plane.
//!
//! At most one analysis is shown: showing one (adding it, or editing it
//! shown) hides the others, so the bodies are cut by one plane.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Document, ModelError, invalid};
use crate::datum::Vec3;
use crate::features::geom_ref::GeomRef;
use crate::features::is_false;
use crate::kernel::Kernel;

/// An analysis kept in the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub name: String,
    /// Shown (its section cuts the bodies); left out of files when true.
    #[serde(default = "shown", skip_serializing_if = "is_shown")]
    pub visible: bool,
    #[serde(flatten)]
    pub def: AnalysisDef,
}

fn shown() -> bool {
    true
}

fn is_shown(visible: &bool) -> bool {
    *visible
}

/// What an analysis is, by its `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnalysisDef {
    /// The bodies cut at a plane.
    Section(SectionAnalysis),
}

impl AnalysisDef {
    /// The first part of default names (`Section1`).
    fn name_stem(&self) -> &'static str {
        match self {
            Self::Section(_) => "Section",
        }
    }
}

/// The bodies shown cut at a plane: the half-space on the side the
/// section's normal points to is cut away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionAnalysis {
    /// A plane reference: an origin plane, a construction plane, a planar
    /// face or a fixed plane.
    pub plane: GeomRef,
    /// How far the section is from the plane along its normal, in mm.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
    /// The other side is cut away: the section's normal is the plane's
    /// reversed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// A section's plane in the design's coordinates: a point on it and the
/// unit normal of the side cut away.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionPlane {
    pub origin: Vec3,
    pub normal: Vec3,
}

impl Analysis {
    /// Why the analysis cannot be kept, if it cannot (not its geometry).
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the analysis needs a name".to_owned());
        }
        match &self.def {
            AnalysisDef::Section(section) => {
                if !section.offset.is_finite() {
                    return Err(format!("{}: the offset must be a finite length", self.name));
                }
            }
        }
        Ok(())
    }
}

impl<K: Kernel> Document<K> {
    /// The analyses in the order they were added.
    pub fn analyses(&self) -> &[Analysis] {
        &self.state.analyses
    }

    /// A section's plane at the timeline marker, or why it cannot be found.
    pub fn section_plane(&self, section: &SectionAnalysis) -> Result<SectionPlane, String> {
        let plane = self.resolve_plane(&section.plane)?;
        let n = plane.normal();
        let origin = [0, 1, 2].map(|i| plane.origin[i] + n[i] * section.offset);
        // No -0 in the normal.
        let normal = n.map(|c| {
            if c == 0.0 {
                0.0
            } else if section.flip {
                -c
            } else {
                c
            }
        });
        Ok(SectionPlane { origin, normal })
    }

    /// The `analyses` query: each analysis in its file form, with a
    /// section's plane at the marker (`section`) or why it is not there
    /// (`error`).
    pub(crate) fn analyses_json(&self) -> Value {
        Value::Array(
            self.state
                .analyses
                .iter()
                .map(|analysis| {
                    let mut value = serde_json::to_value(analysis).expect("analyses serialize");
                    value["visible"] = json!(analysis.visible);
                    match &analysis.def {
                        AnalysisDef::Section(section) => match self.section_plane(section) {
                            Ok(plane) => {
                                value["section"] =
                                    json!({"origin": plane.origin, "normal": plane.normal});
                            }
                            Err(error) => value["error"] = json!(error),
                        },
                    }
                    value
                })
                .collect(),
        )
    }

    /// Why the definition cannot be kept now: its plane is no plane at the
    /// marker.
    fn check_analysis_def(&self, def: &AnalysisDef) -> Result<(), ModelError> {
        match def {
            AnalysisDef::Section(section) => {
                self.resolve_plane(&section.plane)
                    .map_err(|e| invalid(format!("the section's plane: {e}")))?;
            }
        }
        Ok(())
    }

    /// Adds an analysis, shown, and hides the others. Without a name it is
    /// `Section1`, `Section2`, ... (the next free number). Returns the name.
    pub fn add_analysis(
        &mut self,
        def: AnalysisDef,
        name: Option<&str>,
    ) -> Result<String, ModelError> {
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => {
                let stem = def.name_stem();
                (1..)
                    .map(|n| format!("{stem}{n}"))
                    .find(|name| self.analysis_index(name).is_err())
                    .expect("a free name")
            }
        };
        if self.analysis_index(&name).is_ok() {
            return Err(invalid(format!("an analysis '{name}' exists")));
        }
        let analysis = Analysis {
            name: name.clone(),
            visible: true,
            def,
        };
        analysis.check().map_err(invalid)?;
        self.check_analysis_def(&analysis.def)?;
        let label = format!("Add {name}");
        self.apply_with(|state| {
            for other in &mut state.analyses {
                other.visible = false;
            }
            state.analyses.push(analysis);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes an analysis' definition; `visible` shows (hiding the others)
    /// or hides it too. A change that changes nothing is no undo step.
    pub fn edit_analysis(
        &mut self,
        name: &str,
        def: AnalysisDef,
        visible: Option<bool>,
    ) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let old = &self.state.analyses[i];
        let visible = visible.unwrap_or(old.visible);
        let unchanged = old.def == def
            && old.visible == visible
            && (!visible || self.state.analyses.iter().filter(|a| a.visible).count() <= 1);
        if unchanged {
            return Ok(());
        }
        let analysis = Analysis {
            name: name.to_owned(),
            visible,
            def,
        };
        analysis.check().map_err(invalid)?;
        self.check_analysis_def(&analysis.def)?;
        let label = format!("Edit {name}");
        self.apply_with(|state| {
            show_only(state, i, visible);
            state.analyses[i] = analysis;
            Ok((label, (), false))
        })
    }

    /// Shows (hiding the others) or hides an analysis; an undo step
    /// `Show Section1` / `Hide Section1` unless nothing changes.
    pub fn set_analysis_visible(&mut self, name: &str, visible: bool) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let others_shown = self
            .state
            .analyses
            .iter()
            .enumerate()
            .any(|(j, a)| j != i && a.visible);
        if self.state.analyses[i].visible == visible && !(visible && others_shown) {
            return Ok(());
        }
        let label = format!("{} {name}", if visible { "Show" } else { "Hide" });
        self.apply_with(|state| {
            show_only(state, i, visible);
            state.analyses[i].visible = visible;
            Ok((label, (), false))
        })
    }

    pub fn rename_analysis(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the analysis needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.analysis_index(new_name).is_ok() {
            return Err(invalid(format!("an analysis '{new_name}' exists")));
        }
        let label = format!("Rename {name}");
        let new_name = new_name.to_owned();
        self.apply_with(|state| {
            state.analyses[i].name = new_name;
            Ok((label, (), false))
        })
    }

    pub fn delete_analysis(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let label = format!("Delete {name}");
        self.apply_with(|state| {
            state.analyses.remove(i);
            Ok((label, (), false))
        })
    }

    fn analysis_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .analyses
            .iter()
            .position(|a| a.name == name)
            .ok_or_else(|| invalid(format!("there is no analysis '{name}'")))
    }
}

/// When analysis `i` is to be shown, the others are hidden.
fn show_only(state: &mut super::DocState, i: usize, visible: bool) {
    if visible {
        for (j, other) in state.analyses.iter_mut().enumerate() {
            if j != i {
                other.visible = false;
            }
        }
    }
}
