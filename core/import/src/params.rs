// SPDX-License-Identifier: MIT
//! User and model parameters: created with the file's names and expressions
//! (read with Mitcad's expression language), in dependency order,
//! before the features that use them. Features then refer to them by name,
//! and the feature or sketch that made a model parameter adopts it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mitcad_f3d::design::ir::{Dump, Parameter, Reference};
use mitcad_model::expr::{Unit, f3d, is_reserved_name, is_valid_name, value_to_expression};
use mitcad_model::{Document, Kernel};

use crate::report::ParameterReport;

/// The file's parameter names and what they became.
#[derive(Debug, Default)]
pub struct ParamMap {
    /// Name in the file → Mitcad name.
    names: HashMap<String, String>,
    /// Timeline index of the owning feature or sketch → its model
    /// parameters (Mitcad names).
    owned: BTreeMap<i64, Vec<String>>,
}

impl ParamMap {
    /// The Mitcad name of a parameter of the file that was created.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.names.get(name).map(String::as_str)
    }

    /// The model parameters that the item at `index` made.
    pub fn owned_by(&self, index: i64) -> &[String] {
        self.owned.get(&index).map_or(&[], Vec::as_slice)
    }
}

/// The timeline item that made a model parameter.
fn owner_index(p: &Parameter) -> Option<i64> {
    match p.created_by.as_ref()? {
        Reference::Feature(f) => f.timeline_index.flatten(),
        Reference::SketchDimension(d) | Reference::SketchEntity(d) => {
            d.sketch_timeline_index.flatten()
        }
        _ => None,
    }
}

fn text(field: &Option<String>) -> Option<&str> {
    field.as_deref()
}

/// Identifiers of an expression (names, units, functions), with their byte
/// ranges; enough to rename parameters textually.
fn identifiers(expression: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut chars = expression.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c.is_alphabetic() || c == '_' {
            let mut end = start + c.len_utf8();
            while let Some(&(i, c)) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    end = i + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            // A number's exponent (1e3) is not a name.
            let in_number = expression[..start]
                .chars()
                .next_back()
                .is_some_and(|p| p.is_ascii_digit() || p == '.');
            if !in_number {
                out.push((start, end));
            }
        } else if c.is_ascii_digit() || c == '.' {
            // Skip the rest of the number, including an exponent.
            while let Some(&(_, c)) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '.' {
                    chars.next();
                } else {
                    break;
                }
            }
        }
    }
    out
}

/// The expression with parameters renamed.
fn rename_in(expression: &str, renamed: &HashMap<String, String>) -> String {
    if renamed.is_empty() {
        return expression.to_owned();
    }
    let mut out = String::new();
    let mut last = 0;
    for (start, end) in identifiers(expression) {
        if let Some(new) = renamed.get(&expression[start..end]) {
            out.push_str(&expression[last..start]);
            out.push_str(new);
            last = end;
        }
    }
    out.push_str(&expression[last..]);
    out
}

/// A free Mitcad name for a name of the file Mitcad cannot use.
pub(crate) fn free_name(name: &str, taken: &BTreeSet<String>) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = if base.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        base
    } else {
        format!("p_{base}")
    };
    if is_valid_name(&base) && !taken.contains(&base) {
        return base;
    }
    (1u32..)
        .map(|n| format!("{base}_{n}"))
        .find(|c| !taken.contains(c) && !is_reserved_name(c) && is_valid_name(c))
        .expect("a free name exists")
}

/// Creates the parameters of the dump. Text parameters are skipped;
/// expressions that do not evaluate become values.
pub fn import_parameters<K: Kernel>(
    doc: &mut Document<K>,
    dump: &Dump,
    report: &mut ParameterReport,
) -> ParamMap {
    let mut map = ParamMap::default();
    let params: Vec<&Parameter> = dump.all_parameters().collect();
    let mut taken: BTreeSet<String> = doc
        .parameters()
        .iter()
        .map(|p| p.name().to_owned())
        .collect();
    let file_names: BTreeSet<String> = params
        .iter()
        .filter_map(|p| text(&p.name).map(str::to_owned))
        .collect();
    taken.extend(file_names.iter().cloned());

    // Names Mitcad cannot use get a free one; expressions follow.
    let mut renamed = HashMap::new();
    for name in &file_names {
        if !is_valid_name(name) || doc.parameters().find(name).is_some() {
            let new = free_name(name, &taken);
            taken.insert(new.clone());
            report.renamed.push((name.clone(), new.clone()));
            renamed.insert(name.clone(), new);
        }
    }

    struct Pending<'a> {
        param: &'a Parameter,
        name: String,
        expression: String,
        unit: Option<Unit>,
        depends: Vec<String>,
    }
    let mut pending = Vec::new();
    for p in params {
        let Some(name) = text(&p.name) else {
            continue;
        };
        let unit_text = text(&p.unit).unwrap_or("");
        if unit_text == "Text" {
            report.skipped.push((
                name.to_owned(),
                "text parameters are not supported".to_owned(),
            ));
            continue;
        }
        let unit = f3d::parse_unit(unit_text).ok();
        let expression = rename_in(text(&p.expression).unwrap_or(""), &renamed);
        let new_name = renamed
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_owned());
        let depends = identifiers(&expression)
            .into_iter()
            .map(|(s, e)| expression[s..e].to_owned())
            .filter(|id| id != &new_name && taken.contains(id))
            .collect();
        pending.push(Pending {
            param: p,
            name: new_name,
            expression,
            unit,
            depends,
        });
    }

    // Dependency order: repeatedly add the parameters whose references
    // exist. What remains (cycles, unknown names) becomes values.
    let mut done: BTreeSet<String> = doc
        .parameters()
        .iter()
        .map(|p| p.name().to_owned())
        .collect();
    let mut order: Vec<usize> = Vec::new();
    let mut placed = vec![false; pending.len()];
    loop {
        let mut progress = false;
        for (i, p) in pending.iter().enumerate() {
            if !placed[i] && p.depends.iter().all(|d| done.contains(d)) {
                placed[i] = true;
                done.insert(p.name.clone());
                order.push(i);
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
    let unresolved: Vec<usize> = (0..pending.len()).filter(|i| !placed[*i]).collect();

    let stored_value = |p: &Pending<'_>| -> Option<(f64, Unit)> {
        let value = p.param.value?;
        let unit = p.unit?;
        Some((f3d::internal_to_canonical(value, unit.dims()), unit))
    };
    for i in order.into_iter().chain(unresolved.iter().copied()) {
        let p = &pending[i];
        let file_name = text(&p.param.name).unwrap_or_default().to_owned();
        let comment = text(&p.param.comment).unwrap_or("");
        let as_expression = !unresolved.contains(&i)
            && !p.expression.trim().is_empty()
            && doc
                .add_parameter_expression(&p.name, &p.expression, p.unit, comment)
                .is_ok();
        if !as_expression {
            let Some((value, unit)) = stored_value(p) else {
                report
                    .skipped
                    .push((file_name, "no value or unit to fall back on".to_owned()));
                continue;
            };
            let literal = value_to_expression(value, unit);
            if let Err(e) = doc.add_parameter_expression(&p.name, &literal, Some(unit), comment) {
                report.skipped.push((file_name, e.to_string()));
                continue;
            }
            report.literal.push(p.name.clone());
        }
        report.imported += 1;
        // Does Mitcad agree with the value stored in the file?
        if let (Some((expected, _)), Some(param)) = (
            stored_value(p),
            doc.parameters()
                .find(&p.name)
                .and_then(|id| doc.parameters().get(id)),
        ) {
            let got = param.value();
            if (got - expected).abs() > 1e-6 * expected.abs().max(1e-6) {
                report.mismatched.push(format!(
                    "{}: {} gives {got}, the file {expected}",
                    p.name, p.expression
                ));
            }
        }
        map.names.insert(file_name, p.name.clone());
        if let Some(owner) = owner_index(p.param) {
            map.owned.entry(owner).or_default().push(p.name.clone());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_skip_numbers_and_exponents() {
        let e = "d1 * 2.5e3 + sin(30 deg) - Width_2";
        let ids: Vec<&str> = identifiers(e).into_iter().map(|(s, t)| &e[s..t]).collect();
        assert_eq!(ids, ["d1", "sin", "deg", "Width_2"]);
    }

    #[test]
    fn renames_whole_names_only() {
        let renamed = HashMap::from([("mil".to_owned(), "mil_1".to_owned())]);
        assert_eq!(rename_in("mil * 2 + mils", &renamed), "mil_1 * 2 + mils");
        assert_eq!(free_name("mil", &BTreeSet::new()), "mil_1");
        assert_eq!(free_name("a b", &BTreeSet::new()), "a_b");
    }
}
