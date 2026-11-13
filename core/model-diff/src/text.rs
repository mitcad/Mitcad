// SPDX-License-Identifier: MIT
//! The comparison for people: each change's line, the summary and
//! [`DesignDiff::to_text`]. Arrows are `->` and units ASCII (`mm^3`), so
//! any terminal shows them.

use std::fmt::Write;

use serde_json::Value;

use crate::{
    BodyGeometry, DesignDiff, FeatureChange, FieldChange, ItemChange, Kind, Measure,
    ParameterChange, ParameterState, SketchChange, SketchItems, ValueChange, motion, number,
    scalars,
};
use mitcad_model::expr::format_number;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::transform::Transform;

/// At most this many ids are listed in a line.
const IDS: usize = 6;

/// A value in a line: texts as they are, numbers with at most six
/// decimals, short lists in brackets, anything else as JSON, shortened.
pub(super) fn value(v: &Value) -> String {
    match v {
        Value::Null => "(none)".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.as_f64().map_or_else(|| n.to_string(), number),
        Value::String(text) => text.clone(),
        Value::Array(items) if scalars(items) && items.len() <= 12 => {
            let items: Vec<String> = items.iter().map(value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Array(items) if scalars(items) => format!("[{} items]", items.len()),
        // A tagged value: `offset (offset d4)`.
        Value::Object(fields) if fields.get("type").is_some_and(Value::is_string) => {
            let rest: Vec<String> = fields
                .iter()
                .filter(|(key, _)| *key != "type")
                .map(|(key, v)| format!("{key} {}", value(v)))
                .collect();
            let tag = value(&fields["type"]);
            if rest.is_empty() {
                tag
            } else {
                shorten(&format!("{tag} ({})", rest.join(", ")), 80)
            }
        }
        other => shorten(&other.to_string(), 80),
    }
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut short: String = text.chars().take(max).collect();
        short.push_str("...");
        short
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// A list of ids, the first few of them.
fn ids(list: &[String]) -> String {
    let mut text = list
        .iter()
        .take(IDS)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if list.len() > IDS {
        let _ = write!(text, " and {} more", list.len() - IDS);
    }
    text
}

/// A field change's line: `extent.distance d3 -> d7`, `suppressed`.
pub(super) fn field(field: &str, from: &Value, to: &Value) -> String {
    match field {
        "suppressed" => {
            return if to.as_bool() == Some(true) {
                "suppressed"
            } else {
                "unsuppressed"
            }
            .to_owned();
        }
        "visible" => {
            return match to.as_bool() {
                Some(false) => "hidden",
                Some(true) => "shown",
                None => "shown as the application decides",
            }
            .to_owned();
        }
        "grounded" => {
            return if to.as_bool() == Some(true) {
                "grounded"
            } else {
                "no longer grounded"
            }
            .to_owned();
        }
        // mitcad#1093.
        "transparent" => {
            return if to.as_bool() == Some(true) {
                "transparent"
            } else {
                "opaque"
            }
            .to_owned();
        }
        "name" => return format!("renamed from {}", value(from)),
        _ => {}
    }
    if field.rsplit('.').next() == Some("brep") {
        let size = |v: &Value| v.get("size").and_then(Value::as_u64);
        return match (size(from), size(to)) {
            (Some(a), Some(b)) if a != b => {
                format!("{field} B-rep data changed ({a} -> {b} bytes)")
            }
            _ => format!("{field} B-rep data changed"),
        };
    }
    // An item of a list comes or goes.
    match (from, to) {
        (Value::Null, _) if field.ends_with(']') => format!("{field} added"),
        (_, Value::Null) if field.ends_with(']') => format!("{field} deleted"),
        _ => format!("{field} {} -> {}", value(from), value(to)),
    }
}

/// A parameter's name with its comment: `d3 (Extrude1 distance)`.
fn parameter_label(name: &str, comment: &str) -> String {
    if comment.is_empty() {
        name.to_owned()
    } else {
        format!("{name} ({comment})")
    }
}

/// An added or deleted parameter's line: `width = 30 mm: added`, `d5
/// (Extrude2 distance) = d3 * 2 = 30 mm: deleted`.
pub(super) fn parameter_line(name: &str, state: &ParameterState, what: &str) -> String {
    let mut line = format!(
        "{} = {}",
        parameter_label(name, &state.comment),
        state.expression
    );
    if state.expression != state.text {
        let _ = write!(line, " = {}", state.text);
    }
    let _ = write!(line, ": {what}");
    line
}

/// A changed parameter's line: `d3 (Extrude1 distance): 10 mm -> 15 mm`,
/// `d5: 20 mm -> 30 mm (d3 * 2)`, `height: renamed from d2`.
pub(super) fn parameter_modified(
    old: &str,
    new: &str,
    x: &ParameterState,
    y: &ParameterState,
    fields: &[&str],
    owners: &(Option<String>, Option<String>),
) -> String {
    let has = |field: &str| fields.contains(&field);
    let mut parts = Vec::new();
    if has("name") {
        parts.push(format!("renamed from {old}"));
    }
    if has("expression") {
        let plain = |s: &ParameterState| s.expression == s.text;
        let mut part = format!("{} -> {}", x.expression, y.expression);
        if has("value") && !(plain(x) && plain(y)) {
            let _ = write!(part, " ({} -> {})", x.text, y.text);
        }
        parts.push(part);
    } else if has("value") {
        parts.push(format!("{} -> {} ({})", x.text, y.text, y.expression));
    }
    if has("unit") {
        let unit = |u: &str| if u.is_empty() { "(none)" } else { u }.to_owned();
        parts.push(format!("unit {} -> {}", unit(&x.unit), unit(&y.unit)));
    }
    if has("comment") {
        parts.push(format!("comment \"{}\" -> \"{}\"", x.comment, y.comment));
    }
    let owner = |o: &Option<String>| o.clone().unwrap_or_else(|| "(none)".to_owned());
    if has("kind") {
        parts.push(match (y.kind, &owners.1) {
            ("model", Some(feature)) => format!("now a dimension of {feature}"),
            ("model", None) => "now a model parameter".to_owned(),
            _ => "now a user parameter".to_owned(),
        });
    } else if has("owner") {
        parts.push(format!(
            "owner {} -> {}",
            owner(&owners.0),
            owner(&owners.1)
        ));
    }
    if has("favorite") {
        parts.push(
            if y.favorite {
                "marked as a favorite"
            } else {
                "no longer a favorite"
            }
            .to_owned(),
        );
    }
    format!("{}: {}", parameter_label(new, &y.comment), parts.join("; "))
}

/// An added feature's line: `Fillet1 (F3, fillet): added after Extrude1`.
pub(super) fn feature_added(
    uid: FeatureUid,
    name: &str,
    type_name: &str,
    after: Option<&str>,
) -> String {
    match after {
        Some(previous) => format!("{name} ({uid}, {type_name}): added after {previous}"),
        None => format!("{name} ({uid}, {type_name}): added at the start"),
    }
}

/// The fields of a feature's entry, as opposed to its definition's.
const ENTRY_FIELDS: [&str; 5] = ["name", "suppressed", "component", "visible", "type"];

/// A feature's line: `Extrude1 (F2): distance 10 mm -> 15 mm; operation
/// new_body -> join`. `after` is where a moved feature is now: after a
/// feature, or at the start (None).
pub(super) fn feature_modified(
    uid: FeatureUid,
    name: &str,
    fields: &[FieldChange],
    values: &[ValueChange],
    sketch: Option<&SketchChange>,
    after: Option<Option<&str>>,
) -> String {
    let (entry, def): (Vec<&FieldChange>, Vec<&FieldChange>) = fields
        .iter()
        .partition(|f| ENTRY_FIELDS.contains(&f.field.as_str()));
    let mut parts: Vec<String> = entry.iter().map(|f| f.text.clone()).collect();
    parts.extend(values.iter().map(|v| v.text.clone()));
    parts.extend(def.iter().map(|f| f.text.clone()));
    if let Some(sketch) = sketch {
        let text = sketch_text(sketch);
        if !text.is_empty() {
            parts.push(text);
        }
    }
    match after {
        Some(Some(previous)) => parts.push(format!("moved after {previous}")),
        Some(None) => parts.push("moved to the start".to_owned()),
        None => {}
    }
    format!("{name} ({uid}): {}", parts.join("; "))
}

/// `entities 8 -> 10 (added c9, p10), constraints 4 (changed k2), 2
/// entities moved (p6, p7)`.
fn sketch_text(sketch: &SketchChange) -> String {
    let mut parts = Vec::new();
    let lists: [(&str, &SketchItems); 3] = [
        ("entities", &sketch.entities),
        ("constraints", &sketch.constraints),
        ("dimensions", &sketch.dimensions),
    ];
    for (what, items) in lists {
        if items.is_empty() {
            continue;
        }
        let mut part = if items.from == items.to {
            format!("{what} {}", items.to)
        } else {
            format!("{what} {} -> {}", items.from, items.to)
        };
        let details: Vec<String> = [
            ("added", &items.added),
            ("deleted", &items.deleted),
            ("changed", &items.modified),
        ]
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .map(|(how, list)| format!("{how} {}", ids(list)))
        .collect();
        if !details.is_empty() {
            let _ = write!(part, " ({})", details.join("; "));
        }
        parts.push(part);
    }
    if !sketch.moved.is_empty() {
        parts.push(format!(
            "{} moved ({})",
            plural(sketch.moved.len(), "entity", "entities"),
            ids(&sketch.moved)
        ));
    }
    parts.join(", ")
}

/// An occurrence's placement change: `placement moved by [10, 0, 0] mm and
/// turned 90°`.
pub(super) fn transform(a: &Transform, b: &Transform) -> String {
    let (angle, shift) = motion(a, b);
    let mut parts = Vec::new();
    if shift.iter().any(|v| v.abs() > 1e-9) {
        let shift: Vec<String> = shift.iter().map(|v| number(*v)).collect();
        parts.push(format!("moved by [{}] mm", shift.join(", ")));
    }
    if angle > 1e-9 {
        parts.push(format!("turned {}°", format_number(angle.to_degrees(), 3)));
    }
    if parts.is_empty() {
        "placement changed".to_owned()
    } else {
        format!("placement {}", parts.join(" and "))
    }
}

/// A measured value with three decimals, or that it was not measured.
fn quantity(value: Option<f64>, unit: &str) -> String {
    value.map_or_else(
        || "(not measured)".to_owned(),
        |v| format!("{} {unit}", format_number(v, 3)),
    )
}

/// `volume 24000 -> 36000 mm^3 (+12000, +50%)`; empty when the same.
fn measure(what: &str, m: &Measure, unit: &str) -> String {
    if !m.differs() {
        return String::new();
    }
    let (Some(from), Some(to), Some(change)) = (m.from, m.to, m.change) else {
        return format!(
            "{what} {} -> {}",
            quantity(m.from, unit),
            quantity(m.to, unit)
        );
    };
    let signed = |v: f64, decimals| {
        let text = format_number(v, decimals);
        if v > 0.0 { format!("+{text}") } else { text }
    };
    let mut text = format!(
        "{what} {} -> {} {unit} ({}",
        format_number(from, 3),
        format_number(to, 3),
        signed(change, 3)
    );
    if let Some(relative) = m.relative {
        let _ = write!(text, ", {}%", signed(relative * 100.0, 2));
    }
    text.push(')');
    text
}

/// A body's line of the geometry: `Body1 (F2.b0): volume 24000 -> 36000
/// mm^3 (+12000, +50%); area 6800 -> 7800 mm^2 (+1000, +14.71%)`.
pub(super) fn body_geometry(
    kind: Kind,
    uid: &BodyUid,
    name: &str,
    volume: &Measure,
    area: &Measure,
) -> String {
    let label = format!("{name} ({uid})");
    match kind {
        Kind::Added => format!(
            "{label}: added, volume {}, area {}",
            quantity(volume.to, "mm^3"),
            quantity(area.to, "mm^2")
        ),
        Kind::Deleted => format!(
            "{label}: deleted, volume {}, area {}",
            quantity(volume.from, "mm^3"),
            quantity(area.from, "mm^2")
        ),
        _ => {
            let parts: Vec<String> = [
                measure("volume", volume, "mm^3"),
                measure("area", area, "mm^2"),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect();
            format!("{label}: {}", parts.join("; "))
        }
    }
}

/// A changed parameter in the summary: `d3 20 mm -> 25 mm`, `d2 ->
/// height`.
fn parameter_short(p: &ParameterChange) -> String {
    let (Some(x), Some(y)) = (&p.from, &p.to) else {
        return p.name.clone();
    };
    if p.fields.contains(&"value") {
        format!("{} {} -> {}", p.name, x.text, y.text)
    } else if p.fields.contains(&"expression") {
        format!("{} {} -> {}", p.name, x.expression, y.expression)
    } else if let (Some(old), ["name"]) = (&p.renamed_from, p.fields.as_slice()) {
        format!("{old} -> {}", p.name)
    } else {
        format!("{} changed", p.name)
    }
}

/// Counts of added and deleted items, and of changed ones (`changed`
/// their word).
fn counts(parts: &mut Vec<String>, kinds: &[Kind], one: &str, many: &str, changed: &str) {
    let count = |kind: Kind| kinds.iter().filter(|k| **k == kind).count();
    let (added, deleted, modified) = (
        count(Kind::Added),
        count(Kind::Deleted),
        count(Kind::Modified),
    );
    if added > 0 {
        parts.push(format!("+{}", plural(added, one, many)));
    }
    if deleted > 0 {
        parts.push(format!("-{}", plural(deleted, one, many)));
    }
    if modified > 0 {
        parts.push(format!("{} {changed}", plural(modified, one, many)));
    }
}

/// The summary: `d3 20 mm -> 25 mm, +1 feature, 2 features modified`.
pub(super) fn summary(diff: &DesignDiff) -> String {
    summary_of(diff, false)
}

/// The summary, for a person without the ids of the document's fields
/// when `plain` (mitcad#1741).
fn summary_of(diff: &DesignDiff, plain: bool) -> String {
    let mut parts: Vec<String> = diff
        .document
        .iter()
        .map(|c| {
            if plain {
                plain_field(c)
            } else {
                c.text.clone()
            }
        })
        .collect();
    let kinds: Vec<Kind> = diff.parameters.iter().map(|p| p.kind).collect();
    let changed: Vec<&ParameterChange> = diff
        .parameters
        .iter()
        .filter(|p| p.kind == Kind::Modified)
        .collect();
    let added_or_deleted: Vec<Kind> = kinds
        .iter()
        .copied()
        .filter(|k| *k != Kind::Modified)
        .collect();
    counts(&mut parts, &added_or_deleted, "parameter", "parameters", "");
    if changed.len() <= 2 {
        parts.extend(changed.iter().map(|p| parameter_short(p)));
    } else {
        parts.push(format!("{} parameters changed", changed.len()));
    }
    let kinds: Vec<Kind> = diff.features.iter().map(|f| f.kind).collect();
    counts(&mut parts, &kinds, "feature", "features", "modified");
    let moved = kinds.iter().filter(|k| **k == Kind::Moved).count();
    if moved > 0 {
        parts.push(format!("{} moved", plural(moved, "feature", "features")));
    }
    for (items, one, many) in [
        (&diff.components, "component", "components"),
        (&diff.occurrences, "occurrence", "occurrences"),
        (&diff.bodies, "body", "bodies"),
        (&diff.groups, "timeline group", "timeline groups"),
        (&diff.views, "named view", "named views"),
        (&diff.analyses, "analysis", "analyses"),
        (&diff.appearances, "appearance", "appearances"),
        (&diff.flat_patterns, "flat pattern", "flat patterns"),
        (&diff.materials, "material", "materials"),
        (&diff.print_profiles, "print profile", "print profiles"),
        (&diff.annotations, "annotation", "annotations"),
        (&diff.drawings, "drawing", "drawings"),
        (&diff.tables, "table", "tables"),
    ] {
        let kinds: Vec<Kind> = items.iter().map(|i| i.kind).collect();
        counts(&mut parts, &kinds, one, many, "changed");
    }
    if let Some(geometry) = &diff.geometry {
        if geometry.volume.differs() {
            parts.push(format!(
                "volume {} -> {}",
                quantity(geometry.volume.from, "mm^3"),
                quantity(geometry.volume.to, "mm^3")
            ));
        } else if !geometry.bodies.is_empty() {
            parts.push(format!(
                "geometry of {} changed",
                plural(geometry.bodies.len(), "body", "bodies")
            ));
        }
    }
    if parts.is_empty() {
        "no changes".to_owned()
    } else {
        parts.join(", ")
    }
}

// For a person (mitcad#1741, `docs/ui-conventions.md` "What a text says"):
// the same lines without internal names and ids. Items are named by their
// names; a field whose values name ids says only that it changed; a
// sketch counts its items.

/// Whether `text` names something by an internal name or id: a
/// topological name, a region key, a sketch's or a feature's id
/// ([`mitcad_model::notice::user_text_leaks`]), or a component's or an
/// occurrence's (`C2`, `O1`).
fn names_ids(text: &str) -> bool {
    if mitcad_model::notice::user_text_leaks(text).is_some() {
        return true;
    }
    let bytes = text.as_bytes();
    let word = |i: usize| i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_');
    (0..bytes.len()).any(|i| {
        matches!(bytes[i], b'C' | b'O') && (i == 0 || !word(i - 1)) && {
            let mut end = i + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            end > i + 1 && !word(end)
        }
    })
}

/// A field's path in words: `profiles[0].region` -> `profiles region`.
fn field_words(field: &str) -> String {
    let mut words = String::new();
    let mut depth = 0;
    for c in field.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            _ if depth > 0 => {}
            '.' | '_' => words.push(' '),
            _ => words.push(c),
        }
    }
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A field change for a person: its line, or that the field changed when
/// its values name ids.
fn plain_field(f: &FieldChange) -> String {
    if names_ids(&f.text) {
        format!("{} changed", field_words(&f.field))
    } else {
        f.text.clone()
    }
}

/// A value change for a person: by its parameter where its label names a
/// sketch's item (`k5 length`).
fn plain_value(v: &ValueChange) -> String {
    if names_ids(&v.label) {
        format!("{} {} -> {}", v.to_parameter, v.from_text, v.to_text)
    } else {
        v.text.clone()
    }
}

/// A sketch's changes counted: `entities 8 -> 10 (2 added), constraints 4
/// (1 changed), 2 entities moved`.
fn plain_sketch(sketch: &SketchChange) -> String {
    let mut parts = Vec::new();
    for (what, items) in [
        ("entities", &sketch.entities),
        ("constraints", &sketch.constraints),
        ("dimensions", &sketch.dimensions),
    ] {
        if items.is_empty() {
            continue;
        }
        let mut part = if items.from == items.to {
            format!("{what} {}", items.to)
        } else {
            format!("{what} {} -> {}", items.from, items.to)
        };
        let details: Vec<String> = [
            ("added", items.added.len()),
            ("deleted", items.deleted.len()),
            ("changed", items.modified.len()),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(how, count)| format!("{count} {how}"))
        .collect();
        if !details.is_empty() {
            let _ = write!(part, " ({})", details.join(", "));
        }
        parts.push(part);
    }
    if !sketch.moved.is_empty() {
        parts.push(format!(
            "{} moved",
            plural(sketch.moved.len(), "entity", "entities")
        ));
    }
    parts.join(", ")
}

/// `text` without the uid after the name at its start: `Extrude1 (F2):`
/// -> `Extrude1:`, `Fillet1 (F3, fillet):` -> `Fillet1 (fillet):`.
fn without_uid(text: &str, name: &str, uid: &str) -> String {
    if let Some(rest) = text.strip_prefix(&format!("{name} ({uid}, ")) {
        format!("{name} ({rest}")
    } else if let Some(rest) = text.strip_prefix(&format!("{name} ({uid})")) {
        format!("{name}{rest}")
    } else {
        text.to_owned()
    }
}

/// Lines without repeats, in their order (a definition's fields that each
/// say "profiles changed").
fn unique(parts: Vec<String>) -> Vec<String> {
    let mut seen = Vec::new();
    for part in parts {
        if !seen.contains(&part) {
            seen.push(part);
        }
    }
    seen
}

/// A feature's line for a person.
fn plain_feature(f: &FeatureChange) -> String {
    let text = without_uid(&f.text, &f.name, &f.uid.to_string());
    if !names_ids(&text) || matches!(f.kind, Kind::Added | Kind::Deleted) {
        return text;
    }
    let (entry, def): (Vec<&FieldChange>, Vec<&FieldChange>) = f
        .fields
        .iter()
        .partition(|field| ENTRY_FIELDS.contains(&field.field.as_str()));
    let mut parts: Vec<String> = entry.into_iter().map(plain_field).collect();
    parts.extend(f.values.iter().map(plain_value));
    parts.extend(def.into_iter().map(plain_field));
    if let Some(sketch) = &f.sketch {
        let counted = plain_sketch(sketch);
        if !counted.is_empty() {
            parts.push(counted);
        }
    }
    // Where it moved to: the last part of its line, by name.
    if f.moved
        && let Some(moved) = f.text.rsplit("; ").next().and_then(|last| {
            let last = last.rsplit_once("): ").map_or(last, |(_, rest)| rest);
            last.starts_with("moved ").then(|| last.to_owned())
        })
    {
        parts.push(moved);
    }
    format!("{}: {}", f.name, unique(parts).join("; "))
}

/// A component's, occurrence's, body's or other item's line for a person.
fn plain_item(item: &ItemChange) -> String {
    let text = match &item.uid {
        Some(uid) => without_uid(&item.text, &item.name, uid),
        None => item.text.clone(),
    };
    if !names_ids(&text) {
        return text;
    }
    let what = match item.kind {
        Kind::Added => "added".to_owned(),
        Kind::Deleted => "deleted".to_owned(),
        _ => unique(item.fields.iter().map(plain_field).collect()).join("; "),
    };
    format!("{}: {what}", item.name)
}

/// A parameter's line for a person.
fn plain_parameter(p: &ParameterChange) -> String {
    if names_ids(&p.text) && !names_ids(&p.name) {
        format!("{}: changed", p.name)
    } else {
        p.text.clone()
    }
}

impl DesignDiff {
    /// The differences for people (`mitcad-cli diff`): the summary, then a
    /// line per change by section.
    pub fn to_text(&self) -> String {
        self.render(false)
    }

    /// The same for a person in the application (mitcad#1741, the Version
    /// History window): features, bodies and the rest by their names, a
    /// sketch's items counted, a field whose values name internal ids
    /// (references, region keys) only as changed; no topological names,
    /// region keys or ids. [`DesignDiff::to_text`] stays the agents' and
    /// the CLI's.
    pub fn to_plain_text(&self) -> String {
        self.render(true)
    }

    /// The summary for a person (mitcad#1741): [`DesignDiff::summary`]
    /// with the document's fields as [`DesignDiff::to_plain_text`] says
    /// them.
    pub fn plain_summary(&self) -> String {
        summary_of(self, true)
    }

    fn render(&self, plain: bool) -> String {
        if self.identical {
            return "No differences\n".to_owned();
        }
        let summary = if plain {
            self.plain_summary()
        } else {
            self.summary.clone()
        };
        let mut out = format!("Summary: {summary}\n");
        let mut section = |title: &str, lines: Vec<String>| {
            if !lines.is_empty() {
                let _ = writeln!(out, "{title}:");
                for line in lines {
                    let _ = writeln!(out, "  {line}");
                }
            }
        };
        let field = |c: &FieldChange| {
            if plain {
                plain_field(c)
            } else {
                c.text.clone()
            }
        };
        section("Document", self.document.iter().map(field).collect());
        section(
            "Parameters",
            self.parameters
                .iter()
                .map(|p| {
                    if plain {
                        plain_parameter(p)
                    } else {
                        p.text.clone()
                    }
                })
                .collect(),
        );
        section(
            "Timeline",
            self.features
                .iter()
                .map(|f| {
                    if plain {
                        plain_feature(f)
                    } else {
                        f.text.clone()
                    }
                })
                .collect(),
        );
        for (title, items) in [
            ("Components", &self.components),
            ("Occurrences", &self.occurrences),
            ("Bodies", &self.bodies),
            ("Timeline groups", &self.groups),
            ("Named views", &self.views),
            ("Analyses", &self.analyses),
            ("Appearances", &self.appearances),
            ("Flat Patterns", &self.flat_patterns),
            ("Materials", &self.materials),
            ("Print profiles", &self.print_profiles),
            ("Annotations", &self.annotations),
            ("Drawings", &self.drawings),
            ("Tables", &self.tables),
        ] {
            section(
                title,
                items
                    .iter()
                    .map(|c| if plain { plain_item(c) } else { c.text.clone() })
                    .collect(),
            );
        }
        if let Some(geometry) = &self.geometry {
            let _ = writeln!(out, "Geometry:");
            for body in &geometry.bodies {
                let BodyGeometry {
                    text, uid, name, ..
                } = body;
                if plain {
                    let _ = writeln!(out, "  {}", without_uid(text, name, &uid.to_string()));
                } else {
                    let _ = writeln!(out, "  {text}");
                }
            }
            let totals: Vec<String> = [
                measure("volume", &geometry.volume, "mm^3"),
                measure("area", &geometry.area, "mm^2"),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect();
            // One body that is all there is needs no total.
            let only = |body: &BodyGeometry| {
                body.volume.from == geometry.volume.from
                    && body.volume.to == geometry.volume.to
                    && body.area.from == geometry.area.from
                    && body.area.to == geometry.area.to
            };
            if totals.is_empty() {
                let _ = writeln!(
                    out,
                    "  Total: volume {}, area {} (the same)",
                    quantity(geometry.volume.to, "mm^3"),
                    quantity(geometry.area.to, "mm^2")
                );
            } else if !matches!(geometry.bodies.as_slice(), [body] if only(body)) {
                let _ = writeln!(out, "  Total: {}", totals.join("; "));
            }
            for error in &geometry.errors {
                if plain {
                    let _ = writeln!(
                        out,
                        "  warning: {}",
                        mitcad_model::notice::user_message(error)
                    );
                } else {
                    let _ = writeln!(out, "  warning: {error}");
                }
            }
        }
        out
    }
}
