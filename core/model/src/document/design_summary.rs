// SPDX-License-Identifier: MIT
//! The design's summary (mitcad#1088, File › Design Properties…): Title,
//! Subject, Author, Keywords and Comments, kept as the root component's
//! property set `Summary` (the set and keys the `.ipt` import fills), so
//! that the summary shows in Property Sets…, searches and title blocks
//! like any other property.

use super::{Document, ModelError};
use crate::ids::ComponentUid;
use crate::kernel::Kernel;
use crate::properties::{BaseType, Property, PropertyOwner, PropertyValue, Source};

/// The root's property set that holds the summary.
pub const SUMMARY_SET: &str = "Summary";

/// The summary's keys, in the order Design Properties… shows them.
pub const SUMMARY_KEYS: [&str; 5] = ["Title", "Subject", "Author", "Keywords", "Comments"];

impl<K: Kernel> Document<K> {
    /// The summary's texts in [`SUMMARY_KEYS`]' order, empty where the
    /// root has no such property.
    pub fn design_summary(&self) -> Vec<(&'static str, String)> {
        SUMMARY_KEYS
            .iter()
            .map(|key| {
                let text = self
                    .component_stored_property(ComponentUid::ROOT, &format!("{SUMMARY_SET}/{key}"))
                    .unwrap_or_default();
                (*key, text)
            })
            .collect()
    }

    /// Sets the summary's texts given (`changes`: a key of
    /// [`SUMMARY_KEYS`] and its text; an empty or blank text removes the
    /// property) as one undo step, `Design Properties`. A text as the
    /// property has it already changes nothing; with no change no step is
    /// made. A user's text over an imported one keeps the imported value
    /// beside it, as `set_property` does. Returns how many changed.
    pub fn set_design_summary(&mut self, changes: &[(&str, String)]) -> Result<usize, ModelError> {
        let owner = PropertyOwner::Component(ComponentUid::ROOT);
        let store = &self.state().properties;
        let mut edits: Vec<(String, Option<Property>)> = Vec::new();
        for (key, text) in changes {
            let Some(key) = SUMMARY_KEYS
                .iter()
                .find(|k| k.eq_ignore_ascii_case(key.trim()))
            else {
                return Err(ModelError::Invalid(format!(
                    "{key} is no summary field ({})",
                    SUMMARY_KEYS.join(", ")
                )));
            };
            let existing = store.get(owner, SUMMARY_SET, key);
            if text.trim().is_empty() {
                if existing.is_some() {
                    edits.push(((*key).to_owned(), None));
                }
                continue;
            }
            let value = PropertyValue::Text(text.clone());
            if existing.is_some_and(|p| p.value.same(&value) && p.ty == BaseType::Text.as_str()) {
                continue;
            }
            let mut property = Property::new(key, BaseType::Text, value, Source::User);
            if let Some(existing) = existing
                && existing.source != Source::User
                && existing.source != Source::Computed
            {
                // The imported value stays beside the user's.
                let imported = existing
                    .imported
                    .clone()
                    .unwrap_or_else(|| existing.value.clone());
                property.source = existing.source;
                property.imported = (!imported.same(&property.value)).then_some(imported);
            }
            edits.push(((*key).to_owned(), Some(property)));
        }
        let count = edits.len();
        if count == 0 {
            return Ok(0);
        }
        self.apply_with(|state| {
            for (key, property) in edits {
                match property {
                    Some(property) => state.properties.put(owner, SUMMARY_SET, property),
                    None => {
                        state.properties.remove(owner, SUMMARY_SET, &key);
                    }
                }
            }
            Ok(("Design Properties".to_owned(), (), false))
        })?;
        Ok(count)
    }
}
