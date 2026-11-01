// SPDX-License-Identifier: MIT
//! Tables on a drawing's views (mitcad#707 phase 3): a **hole table** of
//! the round holes a view sees end on, each tagged in the view (by size:
//! A1, A2, B1, …, or numbered) and listed with its place from an origin,
//! its size, depth and thread; and a **parts list** of the components a
//! view shows, numbered, each with its quantity and what the design says
//! of it, which the view's **balloons** refer to by number.
//!
//! A table is an annotation of its view ([`super::annotate::ViewAnnotation`]
//! with [`TableSettings`]): it is kept with the view, saved, undone and
//! copied as the other annotations are, and its rows are worked out from
//! the model whenever the sheet is drawn (`model-api/src/drawing_tables.rs`), so it
//! follows the design. This module draws a table from its rows, without
//! the model; its settings are part of the model's stored drawings
//! (`tables_def`, re-exported here, mitcad#777).

// The stored settings (mitcad#777: a part of the model's drawing
// definitions), under their old paths.
pub use mitcad_model::drawing::tables_def::*;

use super::P2;
use super::annotate::{Laid, group_bounds};
use super::sheet::{Anchor, Group, Pen, text_width};

/// The height of a table's texts and of its rows, mm.
const TEXT: f64 = 2.5;
const ROW: f64 = 5.0;
/// The height of a table's title, mm.
const TITLE: f64 = 3.5;

/// The header of a column: a hole table's or a parts list's own, a
/// property's key in capitals.
pub fn header(column: &str) -> String {
    match column {
        "tag" => "TAG".to_owned(),
        "x" => "X".to_owned(),
        "y" => "Y".to_owned(),
        "size" => "SIZE".to_owned(),
        "depth" => "DEPTH".to_owned(),
        "thread" => "THREAD".to_owned(),
        "item" => "ITEM".to_owned(),
        "quantity" => "QTY".to_owned(),
        "name" => "NAME".to_owned(),
        "part_number" => "PART NUMBER".to_owned(),
        "description" => "DESCRIPTION".to_owned(),
        "material" => "MATERIAL".to_owned(),
        other => other
            .rsplit_once('/')
            .map_or(other, |(_, key)| key)
            .trim()
            .to_uppercase(),
    }
}

/// A table worked out from the model: its title, its columns' headers, its
/// rows' cells; what it draws on its view (a hole table's tags, at their
/// holes on the sheet) and where its X and Y are measured from (sheet).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableData {
    pub title: String,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub tags: Vec<(P2, String)>,
    pub origin: Option<P2>,
}

/// Orders cells as a table sorts them: numbers by value, tags (letters,
/// then a number: A2, AA1) by their letters, shorter first, and then their
/// numbers, other text alphabetically.
///
/// A total order (a sort may panic without one): the kind first, numbers
/// before tags before other text, then within the kind, then the texts as
/// they are.
pub fn compare_cells(a: &str, b: &str) -> std::cmp::Ordering {
    enum Cell {
        Number(f64),
        Tag(usize, String, u64),
        Text(String),
    }
    fn cell(s: &str) -> Cell {
        if let Ok(x) = s.trim().parse::<f64>()
            && x.is_finite()
        {
            return Cell::Number(x);
        }
        let letters: String = s.chars().take_while(|c| c.is_ascii_uppercase()).collect();
        let digits = &s[letters.len()..];
        if !letters.is_empty()
            && !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && let Ok(n) = digits.parse::<u64>()
        {
            return Cell::Tag(letters.len(), letters, n);
        }
        Cell::Text(s.to_lowercase())
    }
    let kind = |c: &Cell| match c {
        Cell::Number(_) => 0,
        Cell::Tag(..) => 1,
        Cell::Text(_) => 2,
    };
    let (x, y) = (cell(a), cell(b));
    let within = match (&x, &y) {
        (Cell::Number(p), Cell::Number(q)) => p.total_cmp(q),
        (Cell::Tag(lp, sp, np), Cell::Tag(lq, sq, nq)) => (lp, sp, np).cmp(&(lq, sq, nq)),
        (Cell::Text(p), Cell::Text(q)) => p.cmp(q),
        _ => std::cmp::Ordering::Equal,
    };
    kind(&x).cmp(&kind(&y)).then(within).then_with(|| a.cmp(b))
}

/// Draws a table (its group `A<id>` of class `class`) with its upper left
/// corner at `place`: its title, then its header and rows in cells as wide
/// as their texts, then on its view its tags and the cross of its origin.
/// `scale`: its letters and rows against their usual size (its own
/// letters' height or its drawing's over 3.5 mm, mitcad#729).
pub fn lay_out_table(
    id: u32,
    class: &'static str,
    data: &TableData,
    place: P2,
    scale: f64,
) -> Laid {
    let (title, text, row) = (TITLE * scale, TEXT * scale, ROW * scale);
    let mut g = Group::new(class);
    g.id = Some(format!("A{id}"));
    g.text(
        [place[0], place[1] - title],
        title,
        0.0,
        Anchor::Start,
        data.title.clone(),
    );
    let columns: Vec<f64> = (0..data.header.len())
        .map(|i| {
            std::iter::once(&data.header[i])
                .chain(data.rows.iter().filter_map(|r| r.get(i)))
                .map(|cell| text_width(cell, text) + 3.0 * scale)
                .fold(8.0 * scale, f64::max)
        })
        .collect();
    let width: f64 = columns.iter().sum();
    let top = place[1] - title - 2.0 * scale;
    let bottom = top - row * (data.rows.len() + 1) as f64;
    if !columns.is_empty() {
        g.rect(Pen::Thin, [place[0], bottom], [place[0] + width, top]);
        let mut x = place[0];
        for (i, w) in columns.iter().enumerate() {
            if i > 0 {
                g.line(Pen::Thin, [x, bottom], [x, top]);
            }
            x += w;
        }
        let mut y = top;
        for (r, cells) in std::iter::once(&data.header)
            .chain(data.rows.iter())
            .enumerate()
        {
            if r > 0 {
                g.line(Pen::Thin, [place[0], y], [place[0] + width, y]);
            }
            let mut x = place[0];
            for (i, w) in columns.iter().enumerate() {
                if let Some(cell) = cells.get(i).filter(|c| !c.is_empty()) {
                    g.text(
                        [x + 1.5 * scale, y - row + 1.5 * scale],
                        text,
                        0.0,
                        Anchor::Start,
                        cell.clone(),
                    );
                }
                x += w;
            }
            y -= row;
        }
    }
    for (at, tag) in &data.tags {
        g.text(*at, text, 0.0, Anchor::Start, tag.clone());
    }
    if let Some(o) = data.origin {
        g.items.push(super::sheet::Item::Arc {
            pen: Pen::Thin,
            center: o,
            radius: 1.2,
            start: 0.0,
            end: std::f64::consts::TAU,
        });
        g.line(Pen::Thin, [o[0] - 2.0, o[1]], [o[0] + 2.0, o[1]]);
        g.line(Pen::Thin, [o[0], o[1] - 2.0], [o[0], o[1] + 2.0]);
    }
    let bounds = group_bounds(&g);
    Laid { group: g, bounds }
}

/// The height of a tag's text beside its hole, mm.
pub const TAG_TEXT: f64 = TEXT;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::Item;

    #[test]
    fn a_table_is_drawn_in_cells_under_its_title() {
        let data = TableData {
            title: "HOLES (mm)".to_owned(),
            header: vec!["TAG".to_owned(), "SIZE".to_owned()],
            rows: vec![
                vec!["A1".to_owned(), "Ø6.6".to_owned()],
                vec!["A2".to_owned(), "Ø6.6".to_owned()],
            ],
            tags: vec![([50.0, 50.0], "A1".to_owned())],
            origin: Some([10.0, 10.0]),
        };
        let laid = lay_out_table(3, "hole-table", &data, [100.0, 150.0], 1.0);
        assert_eq!(laid.group.id.as_deref(), Some("A3"));
        let texts: Vec<&str> = laid
            .group
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            [
                "HOLES (mm)",
                "TAG",
                "SIZE",
                "A1",
                "Ø6.6",
                "A2",
                "Ø6.6",
                "A1"
            ]
        );
        // Three rows of 5 mm under the title, its upper left corner where
        // it was put.
        assert!((laid.bounds.min[0] - 10.0 + 2.0).abs() < 1e-9);
        assert!(laid.bounds.max[1] <= 150.0 + 1e-9);
    }

    #[test]
    fn settings_are_checked_and_cells_sorted() {
        let mut settings = TableSettings::at([10.0, 20.0]);
        settings.check(true).unwrap();
        settings.check(false).unwrap();
        assert_eq!(settings.shown_columns(true), HOLE_COLUMNS);
        assert_eq!(settings.shown_columns(false), PARTS_DEFAULT);
        settings.columns = vec!["tag".to_owned(), "colour".to_owned()];
        assert!(settings.check(true).unwrap_err().contains("not colour"));
        settings.check(false).unwrap();
        settings.sort = Some("colour".to_owned());
        settings.check(false).unwrap();
        settings.sort = Some("weight".to_owned());
        assert!(settings.check(false).unwrap_err().contains("to sort by"));
        // Its own columns again: the levels are what is wrong for holes.
        settings.sort = None;
        settings.columns.clear();
        settings.levels = Levels::Parts;
        assert!(settings.check(true).unwrap_err().contains("no levels"));
        assert_eq!(header("Shop/Supplier"), "SUPPLIER");
        assert_eq!(header("part_number"), "PART NUMBER");
        let mut cells = vec!["B1", "A10", "A2", "AA1"];
        cells.sort_by(|a, b| compare_cells(a, b));
        assert_eq!(cells, ["A2", "A10", "B1", "AA1"]);
        let mut numbers = vec!["12.5", "4", "-3"];
        numbers.sort_by(|a, b| compare_cells(a, b));
        assert_eq!(numbers, ["-3", "4", "12.5"]);
        let mut names = vec!["Plate", "pin", "Bracket"];
        names.sort_by(|a, b| compare_cells(a, b));
        assert_eq!(names, ["Bracket", "pin", "Plate"]);
    }

    #[test]
    fn mixed_cells_sort_in_a_total_order() {
        // Numbers, tags and other text mixed (mitcad#707 review: "25" <
        // "100" < "1a-2" < "25" was a cycle, and a sort may panic on one).
        let base = [
            "25", "100", "1a-2", "A2", "A10", "AA1", "-3", "Ø6.6", "x", "X", "", "B1", "1e400",
            "nan", "M8", "12.5", "Pin", "pin", "007", "7", "A02", "Z9",
        ];
        let mut cells: Vec<&str> = base.iter().chain(base.iter()).copied().collect();
        assert!(cells.len() > 20);
        for a in &cells {
            assert_eq!(compare_cells(a, a), std::cmp::Ordering::Equal);
            for b in &cells {
                assert_eq!(
                    compare_cells(a, b),
                    compare_cells(b, a).reverse(),
                    "{a} {b}"
                );
                for c in &cells {
                    if compare_cells(a, b).is_le() && compare_cells(b, c).is_le() {
                        assert!(compare_cells(a, c).is_le(), "{a} <= {b} <= {c}");
                    }
                }
            }
        }
        cells.sort_by(|a, b| compare_cells(a, b));
        assert!(cells.windows(2).all(|w| compare_cells(w[0], w[1]).is_le()));
        // Numbers first, by value; then tags; then text.
        assert_eq!(&cells[..2], ["-3", "-3"]);
        assert!(cells.iter().position(|c| *c == "A2") < cells.iter().position(|c| *c == "1a-2"));
        assert!(cells.iter().position(|c| *c == "25") < cells.iter().position(|c| *c == "100"));
    }
}
