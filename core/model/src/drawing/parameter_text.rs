// SPDX-License-Identifier: MIT
//! Parameters in a drawing's texts (mitcad#2483): `{param:Length}` in a
//! text note, a leader note, a text drawn on the sheet or in a view, and a
//! title block field stands for the value of the design's parameter
//! `Length`, filled in whenever the sheet is drawn
//! (`model-api/src/drawing_templates.rs`), so the sheet and its exports
//! follow Edit Parameters. The reference is kept in the text as written:
//! two drawings may refer to different parameters, a template saved from a
//! sheet carries its references, and one whose parameter the design lacks
//! shows the reference itself.
//!
//! In a title block field the reference is one more of the names in
//! braces ([`super::template_def::fill`]); in a note only the references
//! are filled in, its other braces staying as they are. A parameter
//! renamed takes its references along ([`Drawing::rename_parameter`]).

use super::annotate_def::{AnnotationType, SketchItem};
use super::def::Drawing;

/// What starts a parameter's reference inside the braces.
pub const PARAMETER_PREFIX: &str = "param:";

/// A reference to the parameter `name`, as Insert Parameter writes it.
pub fn reference(name: &str) -> String {
    format!("{{{PARAMETER_PREFIX}{name}}}")
}

/// The parameter a name in braces refers to: `param:Length` (the prefix
/// in any case, spaces around the name left out) gives `Length`; none for
/// another name or an empty one.
pub fn referenced(key: &str) -> Option<&str> {
    let key = key.trim();
    let prefix = key.get(..PARAMETER_PREFIX.len())?;
    if !prefix.eq_ignore_ascii_case(PARAMETER_PREFIX) {
        return None;
    }
    let name = key[PARAMETER_PREFIX.len()..].trim();
    (!name.is_empty()).then_some(name)
}

/// A text with each reference `{param:Name}` replaced by `value(Name)`; a
/// reference whose value is none stays as written, and so do braces that
/// are not a reference and a brace that is not closed.
pub fn fill_references(text: &str, value: &mut dyn FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let inside = after
            .find(['{', '}'])
            .filter(|end| after.as_bytes()[*end] == b'}')
            .map(|close| (close, &after[..close]));
        match inside.and_then(|(close, key)| Some((close, referenced(key)?))) {
            Some((close, name)) => {
                match value(name) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &after[close + 1..];
            }
            None => {
                // Not a reference: the brace as it is, and on from there.
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The parameters a text refers to, each once, in order.
pub fn references(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    fill_references(text, &mut |name| {
        if !out.iter().any(|n| n == name) {
            out.push(name.to_owned());
        }
        None
    });
    out
}

/// A text with its references to the parameter `old` made references to
/// `new` (each written as Insert Parameter writes it); none when it has
/// none.
pub fn rename_references(text: &str, old: &str, new: &str) -> Option<String> {
    let mut found = false;
    let renamed = fill_references(text, &mut |name| {
        (name == old).then(|| {
            found = true;
            reference(new)
        })
    });
    found.then_some(renamed)
}

impl Drawing {
    /// Its references to the parameter `old` made references to `new`, in
    /// its notes, its texts and its sheets' title block fields; whether
    /// any changed.
    pub fn rename_parameter(&mut self, old: &str, new: &str) -> bool {
        let mut changed = false;
        let mut follow = |text: &mut String| {
            if let Some(renamed) = rename_references(text, old, new) {
                *text = renamed;
                changed = true;
            }
        };
        for sheet in &mut self.sheets {
            for note in &mut sheet.notes {
                follow(&mut note.text);
            }
            let views_sketch = sheet.views.iter_mut().flat_map(|v| v.sketch.iter_mut());
            for item in sheet.sketch.iter_mut().chain(views_sketch) {
                if let SketchItem::Text { text, .. } = item {
                    follow(text);
                }
            }
            for annotation in sheet
                .views
                .iter_mut()
                .flat_map(|v| v.annotations.iter_mut())
            {
                if annotation.kind == AnnotationType::Note
                    && let Some(text) = annotation.text.as_mut()
                {
                    follow(text);
                }
            }
            if let Some(template) = sheet.template.as_mut() {
                for field in &mut template.title_block.fields {
                    follow(&mut field.text);
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(name: &str) -> Option<String> {
        match name {
            "Length" => Some("120 mm".to_owned()),
            "Maker" => Some("ACME".to_owned()),
            _ => None,
        }
    }

    #[test]
    fn references_are_filled_in_and_other_braces_kept() {
        let fill = |text: &str| fill_references(text, &mut values);
        assert_eq!(
            fill("BY {param:Maker}, L = {param:Length}"),
            "BY ACME, L = 120 mm"
        );
        // Spaces and the prefix's case do not matter.
        assert_eq!(fill("{ PARAM: Length }"), "120 mm");
        // A missing parameter's reference stays as written.
        assert_eq!(fill("W {param:Width}"), "W {param:Width}");
        // Other braces are the note's own text.
        assert_eq!(fill("{title} {x} {param:}"), "{title} {x} {param:}");
        assert_eq!(fill("open { brace"), "open { brace");
        assert_eq!(fill("a {b {param:Maker}}"), "a {b ACME}");
        assert_eq!(fill("ÄÖ {param:Maker}ü"), "ÄÖ ACMEü");
        assert_eq!(fill(""), "");
    }

    #[test]
    fn references_are_listed_once_and_renamed() {
        assert_eq!(
            references("{param:A} {title} {param: B} {param:A}"),
            vec!["A".to_owned(), "B".to_owned()]
        );
        assert!(references("{title} {Set/Key}").is_empty());
        assert_eq!(
            rename_references("{param:A} and { param:A }, {param:B}", "A", "C").as_deref(),
            Some("{param:C} and {param:C}, {param:B}")
        );
        assert_eq!(rename_references("{param:B}", "A", "C"), None);
        assert_eq!(reference("Length"), "{param:Length}");
        assert_eq!(referenced("param:Length"), Some("Length"));
        assert_eq!(referenced("title"), None);
        assert_eq!(referenced("pa"), None);
    }
}
