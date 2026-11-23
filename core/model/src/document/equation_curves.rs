// SPDX-License-Identifier: MIT
//! Equation curves of sketches (mitcad#2237, `mitcad_sketch::equation`)
//! name the design's parameters in their formulas by text: a renamed
//! parameter is renamed in them too, in the undo step of the rename, as
//! the bindings' expressions follow it.

use std::sync::Arc;

use super::DocState;
use crate::features::FeatureDef;

impl DocState {
    /// Rewrites the equation curves' references to a renamed parameter.
    pub(crate) fn rename_in_equation_curves(&mut self, old: &str, new: &str) {
        for i in 0..self.features.len() {
            let uses = match &self.features[i].def {
                FeatureDef::Sketch(def) => def
                    .equations
                    .iter()
                    .any(|q| q.parameter_names().contains(old)),
                _ => false,
            };
            if !uses {
                continue;
            }
            if let FeatureDef::Sketch(def) = &mut Arc::make_mut(&mut self.features[i]).def {
                for q in &mut def.equations {
                    q.rename_parameter(old, new);
                }
            }
        }
    }
}
