// SPDX-License-Identifier: MIT
//! Configuration tables (mitcad#64): named rows of values for some of a
//! design's parameters, so that one design stands for a family of sizes,
//! such as the screws of a standard from M3 to M12 in their lengths. A row
//! (`M5x16`) sets the expressions of the table's parameters; selectors
//! (`Size`, `Length`) give each row a place in cascaded choices. A library
//! component is inserted in one of its rows ([`crate::library`]).
//!
//! The table is data the design's author writes: nothing is computed to
//! make it, and applying a row only changes parameter expressions, as
//! Change Parameters does.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::parameters::Parameters;

/// A design's configuration table; empty when it has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configurations {
    /// The order of the cascaded choices (`Size`, then `Length`); may be
    /// empty: then a row is chosen by its name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<String>,
    /// The parameters the rows set, by name.
    #[serde(default)]
    pub parameters: Vec<String>,
    /// The row a design is inserted in when none is asked for; the first
    /// row when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub rows: Vec<ConfigurationRow>,
}

/// One row of a configuration table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRow {
    /// Unique in the table (`M5x16`).
    pub name: String,
    /// The row's value of each selector (`{"Size": "M5", "Length": "16"}`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub select: BTreeMap<String, String>,
    /// Expressions of the table's parameters (`"5 mm"`); a parameter left
    /// out keeps the design's own expression.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    /// How a bill of materials names a part made in this row
    /// (`ISO 4762 M5x16`), when it differs from the library's pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub designation: Option<String>,
}

impl Configurations {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.parameters.is_empty() && self.selectors.is_empty()
    }

    pub fn row(&self, name: &str) -> Option<&ConfigurationRow> {
        self.rows.iter().find(|row| row.name == name)
    }

    /// The row a design is inserted in when none is asked for.
    pub fn default_row(&self) -> Option<&ConfigurationRow> {
        self.default
            .as_deref()
            .and_then(|name| self.row(name))
            .or_else(|| self.rows.first())
    }

    /// The values each selector takes, in natural order (`M2.5`, `M3`,
    /// `M10`).
    pub fn selector_values(&self) -> BTreeMap<String, Vec<String>> {
        self.selectors
            .iter()
            .map(|selector| {
                let mut values: Vec<String> = self
                    .rows
                    .iter()
                    .filter_map(|row| row.select.get(selector).cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                values.sort_by(|a, b| natural_cmp(a, b));
                (selector.clone(), values)
            })
            .collect()
    }

    /// The row whose selector values are `select`.
    pub fn find(&self, select: &BTreeMap<String, String>) -> Option<&ConfigurationRow> {
        self.rows.iter().find(|row| {
            self.selectors
                .iter()
                .all(|s| row.select.get(s) == select.get(s))
        })
    }

    /// What is wrong with the table for a design with these parameters,
    /// one message per problem; empty when it is right. Values are checked
    /// by setting them on a copy of the parameters, so a row that would
    /// make a cycle or a unit mismatch is reported too.
    pub fn problems(&self, parameters: &Parameters) -> Vec<String> {
        let mut out = Vec::new();
        if self.is_empty() {
            return out;
        }
        if self.rows.is_empty() {
            out.push("the configuration table has no rows".to_owned());
        }
        let mut seen = BTreeSet::new();
        for name in &self.parameters {
            if !seen.insert(name) {
                out.push(format!("parameter {name} is in the table twice"));
            } else if parameters.find(name).is_none() {
                out.push(format!("parameter {name} of the table does not exist"));
            }
        }
        let mut selectors = BTreeSet::new();
        for selector in &self.selectors {
            if selector.trim().is_empty() {
                out.push("a selector has no name".to_owned());
            } else if !selectors.insert(selector) {
                out.push(format!("selector {selector} is in the table twice"));
            }
        }
        let mut names = BTreeSet::new();
        let mut choices = BTreeSet::new();
        for (i, row) in self.rows.iter().enumerate() {
            let label = if row.name.trim().is_empty() {
                out.push(format!("row {} has no name", i + 1));
                format!("row {}", i + 1)
            } else {
                row.name.clone()
            };
            if !names.insert(row.name.as_str()) {
                out.push(format!("{label}: the name is used by another row"));
            }
            for key in row.select.keys() {
                if !selectors.contains(key) {
                    out.push(format!("{label}: {key} is not a selector of the table"));
                }
            }
            if !self.selectors.is_empty() {
                let missing: Vec<&String> = self
                    .selectors
                    .iter()
                    .filter(|s| row.select.get(*s).is_none_or(|v| v.trim().is_empty()))
                    .collect();
                if !missing.is_empty() {
                    let missing: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
                    out.push(format!("{label}: no value for {}", missing.join(", ")));
                } else {
                    let choice: Vec<&String> =
                        self.selectors.iter().map(|s| &row.select[s]).collect();
                    if !choices.insert(choice) {
                        out.push(format!(
                            "{label}: another row has the same {}",
                            self.selectors.join(" and ")
                        ));
                    }
                }
            }
            for key in row.values.keys() {
                if !self.parameters.contains(key) {
                    out.push(format!("{label}: {key} is not a parameter of the table"));
                }
            }
            let mut trial = parameters.clone();
            if let Err(e) = apply_values(&mut trial, row) {
                out.push(format!("{label}: {e}"));
            }
        }
        if let Some(default) = &self.default
            && self.row(default).is_none()
        {
            out.push(format!("the default row {default} is not in the table"));
        }
        out
    }

    /// The row of the table whose values the parameters have now, if any:
    /// each of its expressions is the parameter's (with decimal points and
    /// spaces as written).
    pub fn current(&self, parameters: &Parameters) -> Option<&ConfigurationRow> {
        self.rows.iter().find(|row| {
            row.values.iter().all(|(name, expression)| {
                parameters
                    .find(name)
                    .and_then(|id| parameters.get(id))
                    .is_some_and(|p| same_expression(p.expression(), expression))
            })
        })
    }
}

fn same_expression(a: &str, b: &str) -> bool {
    let squeeze = |s: &str| -> String {
        crate::expr::with_decimal_points(s)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    };
    squeeze(a) == squeeze(b)
}

/// Sets the row's expressions on the parameters (each keeps its unit);
/// returns the names of the parameters whose values changed.
pub(crate) fn apply_values(
    parameters: &mut Parameters,
    row: &ConfigurationRow,
) -> Result<Vec<String>, String> {
    let mut changed = BTreeSet::new();
    for (name, expression) in &row.values {
        let id = parameters
            .find(name)
            .ok_or_else(|| format!("parameter {name} does not exist"))?;
        let set = parameters
            .set_expression(id, expression)
            .map_err(|e| format!("{name} = {expression}: {e}"))?;
        changed.extend(set.iter().map(|id| parameters.name(*id)));
    }
    Ok(changed.into_iter().collect())
}

/// Natural order of texts with numbers in them: `M2.5` < `M3` < `M10`,
/// `8` < `16` < `100`; otherwise case-insensitive, then exact.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn chunks(text: &str) -> Vec<Result<f64, String>> {
        let mut out = Vec::new();
        let mut current = String::new();
        let mut numeric = false;
        let flush = |current: &mut String, numeric: bool, out: &mut Vec<Result<f64, String>>| {
            if current.is_empty() {
                return;
            }
            let chunk = std::mem::take(current);
            match chunk.parse::<f64>() {
                Ok(value) if numeric => out.push(Ok(value)),
                _ => out.push(Err(chunk.to_lowercase())),
            }
        };
        for c in text.chars() {
            let digit = c.is_ascii_digit() || (c == '.' && numeric);
            if digit != numeric {
                flush(&mut current, numeric, &mut out);
                numeric = digit;
            }
            current.push(c);
        }
        flush(&mut current, numeric, &mut out);
        out
    }
    let (x, y) = (chunks(a), chunks(b));
    for (p, q) in x.iter().zip(&y) {
        let order = match (p, q) {
            (Ok(p), Ok(q)) => p.partial_cmp(q).unwrap_or(Ordering::Equal),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(p), Err(q)) => p.cmp(q),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_sort_naturally() {
        let mut sizes = vec!["M10", "M2.5", "M3", "M12", "M4"];
        sizes.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(sizes, ["M2.5", "M3", "M4", "M10", "M12"]);
        let mut lengths = vec!["100", "16", "8", "20"];
        lengths.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(lengths, ["8", "16", "20", "100"]);
    }
}
