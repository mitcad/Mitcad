// SPDX-License-Identifier: MIT
//! A `Spreadsheet::Sheet`'s cells: `<Cells Count><Cell address="B1"
//! content="=30 mm" alias="Width" [displayUnit=…]/>…`. A cell's content
//! is an expression after `=` ([`crate::expression`]), text after `'`, or
//! a number; the file keeps no computed values.

use serde::Serialize;

use crate::document::Object;
use crate::expression::is_cell_address;
use crate::value::Value;

/// A cell as the file keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cell {
    /// `B1`.
    pub address: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

/// What a cell holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content<'a> {
    /// An expression (the text after `=`).
    Expression(&'a str),
    /// Text (after `'`, or what is no number).
    Text(&'a str),
    /// A number as written (`12`, `2.5`).
    Number(&'a str),
    Empty,
}

impl Cell {
    pub fn content(&self) -> Content<'_> {
        let c = self.content.trim();
        if let Some(e) = c.strip_prefix('=') {
            Content::Expression(e)
        } else if let Some(t) = c.strip_prefix('\'') {
            Content::Text(t)
        } else if c.is_empty() {
            Content::Empty
        } else if c.parse::<f64>().is_ok() {
            Content::Number(c)
        } else {
            Content::Text(c)
        }
    }
}

/// A spreadsheet's cells, in the file's order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Sheet {
    pub cells: Vec<Cell>,
}

impl Sheet {
    /// The cells of a `Spreadsheet::Sheet` (its `cells` property).
    pub fn of(object: &Object) -> Sheet {
        let Some(Value::Xml(list)) = object.value("cells") else {
            return Sheet::default();
        };
        let cells = list
            .elements("Cell")
            .filter_map(|c| {
                let address = c.attribute("address")?.to_owned();
                if !is_cell_address(&address) {
                    return None;
                }
                Some(Cell {
                    address,
                    content: c.attribute("content").unwrap_or_default().to_owned(),
                    alias: c
                        .attribute("alias")
                        .filter(|a| !a.is_empty())
                        .map(str::to_owned),
                })
            })
            .collect();
        Sheet { cells }
    }

    pub fn cell(&self, address: &str) -> Option<&Cell> {
        self.cells.iter().find(|c| c.address == address)
    }

    /// The cell with an alias.
    pub fn aliased(&self, alias: &str) -> Option<&Cell> {
        self.cells
            .iter()
            .find(|c| c.alias.as_deref() == Some(alias))
    }

    /// The cell a name means in the sheet's own formulas or after its
    /// name (`Sheet.Width`, `Sheet.B3`): an alias, else an address.
    pub fn named(&self, name: &str) -> Option<&Cell> {
        self.aliased(name).or_else(|| self.cell(name))
    }
}

pub fn is_sheet(type_name: &str) -> bool {
    type_name == "Spreadsheet::Sheet"
}
