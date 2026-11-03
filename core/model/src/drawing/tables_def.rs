// SPDX-License-Identifier: MIT
//! The settings of a table on a drawing's view (mitcad#707 phase 3) as a
//! design keeps them: a hole table's or a parts list's place, columns,
//! sort, tags, origin and levels, with their check. A table is an
//! annotation of its view ([`super::annotate_def::ViewAnnotation`]); it is
//! worked out and drawn by the drawing crate (`mitcad-model-drawing`, its
//! `tables`), above the model (mitcad#777).

use serde::{Deserialize, Serialize};

use super::P2;

/// The columns of a hole table, in their order.
pub const HOLE_COLUMNS: [&str; 6] = ["tag", "x", "y", "size", "depth", "thread"];
/// The parts list's own columns, in their order; any other column is a
/// property of the components (`Key` or `Set/Key`).
pub const PARTS_COLUMNS: [&str; 6] = [
    "item",
    "quantity",
    "name",
    "part_number",
    "description",
    "material",
];
/// A parts list's columns when it names none.
pub const PARTS_DEFAULT: [&str; 5] = ["item", "quantity", "name", "description", "material"];

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// How a hole table tags its holes: a letter per size and a number per
/// hole of it (A1, A2, B1), or a number per hole.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagStyle {
    #[default]
    BySize,
    Numbers,
}

/// Where a hole table's X and Y are measured from when it has no origin
/// pick: the view's lower left corner, or the origin of the drawing's
/// component.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableOrigin {
    #[default]
    Corner,
    Model,
}

/// The rows of a parts list: the components of the occurrences the view
/// shows right below its subject (`top`), or every component that has
/// bodies of its own (`parts`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Levels {
    #[default]
    Top,
    Parts,
}

/// A table's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableSettings {
    /// Its upper left corner on the sheet, mm (its title's).
    pub place: P2,
    /// The columns shown, in order; the type's own when left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
    /// The column its rows are sorted by: a hole table's tags, a parts
    /// list's names when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub descending: bool,
    /// A hole table's.
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: TagStyle,
    /// A hole table's, when it has no origin pick.
    #[serde(default, skip_serializing_if = "is_default")]
    pub origin: TableOrigin,
    /// A parts list's.
    #[serde(default, skip_serializing_if = "is_default")]
    pub levels: Levels,
    /// A read-only grouping of measured geometry and material (mitcad#1176).
    #[serde(default, skip_serializing_if = "is_false")]
    pub group_identical: bool,
    /// Over it; `HOLES (mm)` and `BILL OF MATERIALS` when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl TableSettings {
    /// A table there with every other setting its default.
    pub fn at(place: P2) -> Self {
        Self {
            place,
            columns: Vec::new(),
            sort: None,
            descending: false,
            tags: TagStyle::default(),
            origin: TableOrigin::default(),
            levels: Levels::default(),
            group_identical: false,
            title: None,
        }
    }

    /// The columns a hole table (`holes`) or a parts list shows.
    pub fn shown_columns(&self, holes: bool) -> Vec<String> {
        if !self.columns.is_empty() {
            return self.columns.clone();
        }
        let own: &[&str] = if holes { &HOLE_COLUMNS } else { &PARTS_DEFAULT };
        own.iter().map(|c| (*c).to_owned()).collect()
    }

    /// The column it is sorted by.
    pub fn sort_column(&self, holes: bool) -> String {
        self.sort
            .clone()
            .unwrap_or_else(|| if holes { "tag" } else { "name" }.to_owned())
    }

    /// Why a hole table's (`holes`) or a parts list's settings cannot be
    /// kept, if they cannot.
    pub fn check(&self, holes: bool) -> Result<(), String> {
        if !self.place.iter().all(|v| v.is_finite()) {
            return Err("a table's place is a point".to_owned());
        }
        let known = |c: &str| {
            if holes {
                HOLE_COLUMNS.contains(&c)
            } else {
                !c.trim().is_empty()
            }
        };
        for (i, column) in self.columns.iter().enumerate() {
            if !known(column) {
                return Err(if holes {
                    format!(
                        "a hole table's columns are {}, not {column}",
                        HOLE_COLUMNS.join(", ")
                    )
                } else {
                    "a parts list's column is named".to_owned()
                });
            }
            if self.columns[..i].contains(column) {
                return Err(format!("the column {column} is there twice"));
            }
        }
        if let Some(sort) = &self.sort {
            let sortable = if holes {
                HOLE_COLUMNS.contains(&sort.as_str())
            } else {
                PARTS_COLUMNS.contains(&sort.as_str()) || self.columns.contains(sort)
            };
            if !sortable {
                return Err(format!("the table has no column {sort} to sort by"));
            }
        }
        if !holes && (self.tags != TagStyle::default() || self.origin != TableOrigin::default()) {
            return Err("a parts list has no tags or origin".to_owned());
        }
        if holes && self.levels != Levels::default() {
            return Err("a hole table has no levels".to_owned());
        }
        Ok(())
    }
}
