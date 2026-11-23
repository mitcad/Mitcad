// SPDX-License-Identifier: MIT
//! What a script made from a design's timeline (`export_script`,
//! mitcad#2245) does not make again, and what keeps a design from being
//! exported at all, in words. The script rebuilds the parameters, the
//! features, the components and occurrences, the timeline groups and
//! marker, the bodies' names and the light bulbs and drawn sizes of
//! sketches and construction features; everything else the definition state
//! keeps beside the timeline is listed here, one line per kind, so that
//! whoever replays the script knows what the replay lacks.

use super::DocState;

impl DocState {
    /// What a replay of the design's exported script lacks, one line per
    /// kind in words (`appearances (2)`, `image references (1)`): empty
    /// when the script makes everything the design has.
    pub fn script_omissions(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut count = |what: &str, n: usize| {
            if n > 0 {
                out.push(format!("{what} ({n})"));
            }
        };
        count("appearances", self.appearances.len());
        count(
            "bodies' visibility, materials and appearances",
            self.body_attributes.len(),
        );
        count("materials", self.materials.len());
        count(
            "components' materials and appearances",
            self.component_materials.len() + self.component_appearances.len(),
        );
        count("image references", self.image_references.len());
        count("named views", self.named_views.len());
        count("analyses", self.analyses.len());
        count("annotations", self.annotations.len());
        count("drawings", self.drawings.len());
        count("comments", self.comments.len());
        count("selection sets", self.selection_sets.len());
        count("flat patterns", self.flat_patterns.len());
        count("print profiles", self.print_profiles.len());
        count("print plates", self.print_plates.len());
        count("filament changes", self.filament_changes.len());
        count("purge volumes", self.purge_volumes.len());
        count(
            "motion studies and contact sets",
            self.motion_studies.len() + self.contact_sets.len(),
        );
        count("acknowledged warnings", self.acknowledged_warnings.len());
        count("import notes", self.import_notes.len());
        let mut kind = |what: &str, present: bool| {
            if present {
                out.push(what.to_owned());
            }
        };
        kind("the configuration table", !self.configurations.is_empty());
        kind("tables", !self.tables.is_empty());
        kind(
            "suppression and visibility that follow expressions",
            !self.bindings.is_empty(),
        );
        kind("properties", !self.properties.is_empty());
        kind("CAM setups", !self.cam.is_empty());
        kind("render settings", self.render != Default::default());
        kind("the direction of gravity", self.gravity.is_some());
        kind(
            "the height of annotation texts",
            self.annotation_text_height.is_some(),
        );
        kind(
            "the import's tolerance for joins",
            self.join_tolerance.is_some(),
        );
        let assembly = &self.assembly;
        kind(
            "occurrence folders",
            assembly.occurrences.iter().any(|o| o.folder.is_some()),
        );
        kind(
            "see-through occurrences",
            assembly.occurrences.iter().any(|o| o.transparent),
        );
        kind(
            "flexible occurrences",
            assembly.occurrences.iter().any(|o| o.flexible.is_some()),
        );
        out
    }

    /// Why the design cannot be exported as a script at all, in words; None
    /// when it can. Its sketch blocks, the library parts BUILD placed and
    /// tables that expressions look values up in have no commands a script
    /// could make them with, and features use them.
    pub fn script_refusal(&self) -> Option<String> {
        if !self.sketch_blocks.is_empty() {
            return Some(
                "the design has sketch blocks, which a script cannot make again".to_owned(),
            );
        }
        if !self.library_sources.is_empty() {
            return Some(
                "the design has library parts BUILD placed, which a script cannot make again"
                    .to_owned(),
            );
        }
        if self
            .parameters
            .iter()
            .any(|p| !p.table_references().is_empty())
        {
            return Some(
                "parameters look values up in the design's tables, which a script cannot make \
                 again"
                    .to_owned(),
            );
        }
        None
    }
}
