// SPDX-License-Identifier: MIT
//! What the imports' notes on the timeline share (mitcad#170): the notes'
//! texts of a base feature standing in for items, of an item that came in
//! partly, and a note as a sentence, gathered by feature.

use std::collections::BTreeMap;

use mitcad_model::FeatureUid;

/// How many items a base feature's note names; the rest are counted.
const NAMED_ITEMS: usize = 8;

/// The note of a base feature standing in for items: each one's name
/// with its reason, items in a row with the same reason sharing it, at
/// most [`NAMED_ITEMS`] named.
pub fn stands_in_for(items: &[(String, Option<String>)]) -> String {
    let mut runs: Vec<(Vec<&str>, Option<&str>)> = Vec::new();
    for (name, reason) in items.iter().take(NAMED_ITEMS) {
        let reason = reason.as_deref().map(str::trim).filter(|r| !r.is_empty());
        match runs.last_mut() {
            Some((names, last)) if *last == reason => names.push(name),
            _ => runs.push((vec![name], reason)),
        }
    }
    let mut text = runs
        .iter()
        .map(|(names, reason)| match reason {
            Some(reason) => format!("{} ({reason})", names.join(", ")),
            None => names.join(", "),
        })
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > NAMED_ITEMS {
        text.push_str(&format!(
            " and {} more items (see the import report)",
            items.len() - NAMED_ITEMS
        ));
    }
    // The import's own wording said plainly (mitcad#1893).
    crate::report_status::plain_words(&format!("Stands in for {text}"))
}

/// The note of an item that came in partly.
pub fn partly(note: Option<&str>) -> String {
    match note.map(str::trim).filter(|n| !n.is_empty()) {
        Some(note) => crate::report_status::plain_words(&format!("Partly imported: {note}")),
        None => "Partly imported".to_owned(),
    }
}

/// Adds `note` to the notes of `uid`, after what it has.
pub fn push(notes: &mut BTreeMap<FeatureUid, String>, uid: FeatureUid, note: String) {
    match notes.get_mut(&uid) {
        Some(text) if !text.contains(&note) => {
            text.push_str("; ");
            text.push_str(&note);
        }
        Some(_) => {}
        None => {
            notes.insert(uid, note);
        }
    }
}

/// A note as the start of a timeline message: its first letter capital.
pub fn sentence(note: &str) -> String {
    let mut chars = note.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_feature_names_the_items_it_stands_in_for() {
        let items = vec![
            (
                "Extrude5".to_owned(),
                Some("its result is not in the file's history".to_owned()),
            ),
            (
                "Fillet2".to_owned(),
                Some("the import was stopped".to_owned()),
            ),
            (
                "Fillet3".to_owned(),
                Some("the import was stopped".to_owned()),
            ),
            ("Shell1".to_owned(), None),
        ];
        assert_eq!(
            stands_in_for(&items),
            "Stands in for Extrude5 (its result is not in the file's history), Fillet2, Fillet3 \
             (the import was stopped), Shell1"
        );
        let many: Vec<_> = (0..11)
            .map(|i| {
                (
                    format!("Item{i}"),
                    Some("the import was stopped".to_owned()),
                )
            })
            .collect();
        assert_eq!(
            stands_in_for(&many),
            "Stands in for Item0, Item1, Item2, Item3, Item4, Item5, Item6, Item7 (the import \
             was stopped) and 3 more items (see the import report)"
        );
    }

    #[test]
    fn notes_read_as_sentences() {
        assert_eq!(sentence("suppressed in the file"), "Suppressed in the file");
        assert_eq!(
            partly(Some("left out: 3 constraints")),
            "Partly imported: left out: 3 constraints"
        );
        assert_eq!(partly(None), "Partly imported");
    }
}
