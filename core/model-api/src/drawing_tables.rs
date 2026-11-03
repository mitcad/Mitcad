// SPDX-License-Identifier: MIT
//! Tables on drawings' views (mitcad#707 phase 3, `mitcad_model_drawing::tables`;
//! commands.md, "Tables on drawing views"): the rows of a hole table and of
//! a parts list worked out from the model whenever a sheet is drawn, and
//! the numbers of balloons.
//!
//! A **hole table** lists the round holes of the bodies its view shows
//! that the view sees end on (`Kernel::round_holes`, with their threads,
//! as File › Export Drawing's), where the view shows them: in a detail's
//! circle, beside breaks, behind a section's plane. Its holes are tagged in
//! the view, by size (a letter per size, numbered along the view's x then
//! y) or by number, and their X and Y are measured along the view's axes
//! from its origin: a pick (a point of the model, so it follows it), the
//! view's lower left corner, or the drawing's component's origin.
//!
//! A **parts list** lists the components of the occurrences its view shows
//! right below its subject (or, with `parts`, the components with bodies
//! at any depth), each once with how many of its occurrences the view
//! shows; the bodies of the drawing's own component are rows of their own.
//! Its columns are its own (item, quantity, name, part number, description,
//! material) or any property of the components (`Key`, `Set/Key`), as the
//! `parts_list` query has them; rows are sorted by a column and numbered in
//! that order. A **balloon** picks an edge or a face of a component's body
//! on a view and shows that component's number: in a parts list of its own
//! view, else of another view of the sheet. A balloon whose component is
//! gone, or that no parts list lists, is broken.

use serde::Deserialize;

use super::annotations::present;
use super::drawing::ThreadOf;
use super::drawing_annotations::ViewAt;
use super::drawings::Shown;
use super::prelude::*;
use mitcad_model::assembly::Assembly;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, PartsListRow};
use mitcad_model_drawing::Units;
use mitcad_model_drawing::annotate::ViewAnnotation;
use mitcad_model_drawing::internal::hole_text;
use mitcad_model_drawing::tables::{
    Levels, TAG_TEXT, TableData, TableOrigin, TableSettings, TagStyle, compare_cells, header,
};

/// A table's settings as `add_drawing_annotation` and
/// `edit_drawing_annotation` take them: what is given changes; `null`
/// sets `sort` and `title` back to their defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TableEdit {
    #[serde(default)]
    columns: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    sort: Option<Option<String>>,
    #[serde(default)]
    descending: Option<bool>,
    #[serde(default)]
    tags: Option<TagStyle>,
    #[serde(default)]
    origin: Option<TableOrigin>,
    #[serde(default)]
    levels: Option<Levels>,
    #[serde(default)]
    group_identical: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    title: Option<Option<String>>,
}

impl TableEdit {
    /// The settings changed as it says.
    pub(super) fn apply(&self, table: &mut TableSettings) {
        if let Some(columns) = &self.columns {
            table.columns = columns.iter().map(|c| c.trim().to_owned()).collect();
        }
        if let Some(sort) = &self.sort {
            table.sort = sort
                .as_ref()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty());
        }
        if let Some(descending) = self.descending {
            table.descending = descending;
        }
        if let Some(tags) = self.tags {
            table.tags = tags;
        }
        if let Some(origin) = self.origin {
            table.origin = origin;
        }
        if let Some(group_identical) = self.group_identical {
            table.group_identical = group_identical;
        }
        if let Some(levels) = self.levels {
            table.levels = levels;
        }
        if let Some(title) = &self.title {
            table.title = title
                .as_ref()
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty());
        }
    }
}

/// What a row of a parts list stands for: a component, or a body of the
/// drawing's own component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RowKey {
    Component(ComponentUid),
    Body(BodyUid),
}

/// A parts list worked out: its view, its levels, each row's key with its
/// number, its table, and its rows in words (`1 Plate x2; 2 Pin x4`).
pub(super) struct PartsList {
    pub(super) view: u32,
    pub(super) levels: Levels,
    /// How many occurrences below the drawing's component its view's
    /// subject lies.
    pub(super) depth: usize,
    pub(super) items: Vec<(RowKey, Vec<OccurrenceUid>, u32)>,
    pub(super) data: TableData,
    pub(super) label: String,
}

impl PartsList {
    /// The number of the row this key has, if it has one.
    pub(super) fn item(&self, key: RowKey, path: &[OccurrenceUid]) -> Option<u32> {
        let placement = match (key, self.levels) {
            (RowKey::Body(_), _) => &[][..],
            (RowKey::Component(_), Levels::Top) => path.get(..=self.depth)?,
            (RowKey::Component(_), Levels::Parts) => path,
        };
        if let Some((_, _, number)) = self
            .items
            .iter()
            .find(|(k, p, _)| *k == key && p == placement)
        {
            return Some(*number);
        }
        // Another view can show a different placement of the same part.
        // Preserve that legacy lookup only when its row is unambiguous.
        let mut numbers = self
            .items
            .iter()
            .filter(|(k, _, _)| *k == key)
            .map(|(_, _, n)| *n);
        let first = numbers.next()?;
        numbers.all(|n| n == first).then_some(first)
    }
}

/// The row a pick's body is in, in a parts list of these levels whose
/// view's subject lies `depth` occurrences below the drawing's component
/// (`path` starts there): the component of the occurrence right below the
/// subject, or of the body's own occurrence; the body itself when no
/// occurrence below the subject places it.
pub(super) fn row_of(
    assembly: &Assembly,
    body: BodyUid,
    path: &[OccurrenceUid],
    levels: Levels,
    depth: usize,
) -> Option<RowKey> {
    let occurrence = if path.len() > depth {
        match levels {
            Levels::Top => path.get(depth),
            Levels::Parts => path.last(),
        }
    } else {
        None
    };
    match occurrence {
        None => Some(RowKey::Body(body)),
        Some(o) => assembly
            .occurrence(*o)
            .map(|o| RowKey::Component(o.component)),
    }
}

/// The rows of a table sorted by a column (its index in `columns`, if the
/// table shows it) and in which way.
fn sorted<T>(rows: &mut [(T, Vec<String>)], at: Option<usize>, descending: bool) {
    if let Some(at) = at {
        rows.sort_by(|a, b| {
            let order = compare_cells(&a.1[at], &b.1[at]);
            if descending { order.reverse() } else { order }
        });
    } else if descending {
        rows.reverse();
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait DrawingTablesOps<K: Kernel> {
    /// A hole table on its view worked out: its rows, its holes' tags on
    /// the view and its origin.
    fn hole_table(
        &self,
        annotation: &ViewAnnotation,
        origin_at: Option<[f64; 2]>,
        view: &ViewAt<'_>,
        units: Units,
        threads: &[ThreadOf],
    ) -> Result<(TableData, Vec<String>), String>;

    /// The bodies a view shows something of: on a section, a detail or a
    /// broken view, those whose own projection keeps a line behind the
    /// plane, in the circle or beside the breaks (mitcad#718: a parts list
    /// counted what the section took away); all its bodies on other views.
    /// A body whose projection fails counts as seen. Each body's answer is
    /// kept on its own (`body_in_view`, mitcad#722).
    fn bodies_seen(&self, view: &ViewAt<'_>) -> Vec<Shown>;

    /// A parts list of its view (`view_id`) worked out: its rows sorted
    /// and numbered, each row's key with its number.
    fn parts_list_of(
        &self,
        annotation: &ViewAnnotation,
        view_id: u32,
        view: &ViewAt<'_>,
    ) -> Result<PartsList, String>;
}

impl<K: Kernel> DrawingTablesOps<K> for Document<K> {
    fn hole_table(
        &self,
        annotation: &ViewAnnotation,
        origin_at: Option<[f64; 2]>,
        view: &ViewAt<'_>,
        units: Units,
        threads: &[ThreadOf],
    ) -> Result<(TableData, Vec<String>), String> {
        let settings = annotation
            .table
            .as_ref()
            .ok_or_else(|| "a hole table needs its settings".to_owned())?;
        let spec = &view.placed.spec;
        let map = &view.placed.map;
        let eye = spec.toward_eye;
        let mut warnings = Vec::new();
        // The holes the view sees end on, where it shows them: each with its
        // place in the view and its words.
        struct Seen {
            at: [f64; 2],
            radius: f64,
            size: String,
            depth: String,
            thread: String,
        }
        let mut holes: Vec<Seen> = Vec::new();
        for s in &view.shown {
            for h in self.shown_holes(s, threads, &mut warnings) {
                let hole = &h.hole;
                let along = mitcad_vec3::dot(hole.direction, eye);
                if along.abs() < 1.0 - 1e-6 || !(hole.through || along < 0.0) {
                    continue;
                }
                let far = mitcad_vec3::add_scaled(hole.opening, hole.direction, hole.depth);
                let at = spec.project(hole.opening);
                if !view.sees(at) || !(view.kept(hole.opening) || view.kept(far)) {
                    continue;
                }
                if holes
                    .iter()
                    .any(|o| (o.at[0] - at[0]).abs() < 1e-6 && (o.at[1] - at[1]).abs() < 1e-6)
                {
                    continue;
                }
                let text = hole_text(&h, units);
                holes.push(Seen {
                    at,
                    radius: hole.diameter / 2.0,
                    size: match &text.extra {
                        Some(extra) => format!("{} {extra}", text.size),
                        None => text.size,
                    },
                    depth: text.depth,
                    thread: text.thread.unwrap_or_else(|| "-".to_owned()),
                });
            }
        }
        // Tagged along the view's x, then its y.
        // Their x on a grid of 1e-6 mm, so that holes in one column count
        // as one x and the order stays a total one.
        holes.sort_by(|a, b| {
            let column = |p: f64| (p / 1e-6).round();
            column(a.at[0])
                .total_cmp(&column(b.at[0]))
                .then(a.at[1].total_cmp(&b.at[1]))
        });
        let mut sizes: Vec<(String, u32)> = Vec::new();
        let mut tags: Vec<String> = Vec::new();
        for (i, h) in holes.iter().enumerate() {
            tags.push(match settings.tags {
                TagStyle::Numbers => (i + 1).to_string(),
                TagStyle::BySize => {
                    let key = format!("{}|{}|{}", h.size, h.depth, h.thread);
                    let k = match sizes.iter().position(|(s, _)| *s == key) {
                        Some(k) => k,
                        None => {
                            sizes.push((key, 0));
                            sizes.len() - 1
                        }
                    };
                    sizes[k].1 += 1;
                    format!("{}{}", size_letters(k), sizes[k].1)
                }
            });
        }
        let origin = match (origin_at, settings.origin) {
            (Some(at), _) => at,
            (None, TableOrigin::Model) => [0.0, 0.0],
            (None, TableOrigin::Corner) => view.placed.bounds.min,
        };
        let mut columns = settings.shown_columns(true);
        let shown = columns.len();
        // Sorted by a column it does not show too: its cells, left off
        // (mitcad#718, as a parts list does).
        let sort = settings.sort_column(true);
        if !columns.contains(&sort) {
            columns.push(sort.clone());
        }
        let mut rows: Vec<(usize, Vec<String>)> = holes
            .iter()
            .zip(&tags)
            .enumerate()
            .map(|(i, (h, tag))| {
                let cells = columns
                    .iter()
                    .map(|c| match c.as_str() {
                        "tag" => tag.clone(),
                        "x" => units.length(h.at[0] - origin[0]),
                        "y" => units.length(h.at[1] - origin[1]),
                        "size" => h.size.clone(),
                        "depth" => h.depth.clone(),
                        _ => h.thread.clone(),
                    })
                    .collect();
                (i, cells)
            })
            .collect();
        sorted(
            &mut rows,
            columns.iter().position(|c| *c == sort),
            settings.descending,
        );
        columns.truncate(shown);
        for (_, cells) in &mut rows {
            cells.truncate(shown);
        }
        let scale = map.scale;
        let data = TableData {
            title: settings
                .title
                .clone()
                .unwrap_or_else(|| format!("HOLES ({})", units.symbol)),
            header: columns.iter().map(|c| header(c.as_str())).collect(),
            rows: rows.into_iter().map(|(_, cells)| cells).collect(),
            // Below right of each hole, as the export tags them.
            tags: holes
                .iter()
                .zip(&tags)
                .map(|(h, tag)| {
                    let p = map.at(h.at);
                    let r = h.radius * scale * std::f64::consts::FRAC_1_SQRT_2;
                    ([p[0] + r + 0.5, p[1] - r - 0.5 - TAG_TEXT], tag.clone())
                })
                .collect(),
            origin: Some(map.at(origin)),
        };
        Ok((data, warnings))
    }

    fn bodies_seen(&self, view: &ViewAt<'_>) -> Vec<Shown> {
        let placed = view.placed;
        let partial =
            placed.spec.section.is_some() || view.circle.is_some() || !placed.map.breaks.is_empty();
        if !partial {
            return view.shown.clone();
        }
        view.shown
            .iter()
            .filter(|s| match self.body_in_view(view, s) {
                Ok(own) => own.seen,
                Err(_) => true,
            })
            .cloned()
            .collect()
    }

    fn parts_list_of(
        &self,
        annotation: &ViewAnnotation,
        view_id: u32,
        view: &ViewAt<'_>,
    ) -> Result<PartsList, String> {
        let settings = annotation
            .table
            .as_ref()
            .ok_or_else(|| "a parts list needs its settings".to_owned())?;
        let assembly = self.assembly();
        // Each row's key with the placements counted (an occurrence's path
        // once, however many bodies it has).
        // Below the view's subject: an occurrence's view lists what is in
        // it, not the occurrence itself.
        let depth = view.subject_depth;
        let mut keys: Vec<(RowKey, Vec<Vec<OccurrenceUid>>)> = Vec::new();
        let seen = self.bodies_seen(view);
        for s in &seen {
            let Some(key) = row_of(assembly, s.body, &s.path, settings.levels, depth) else {
                continue;
            };
            let placement: Vec<OccurrenceUid> = match (key, settings.levels) {
                (RowKey::Body(_), _) => Vec::new(),
                (RowKey::Component(_), Levels::Top) => s.path[..=depth].to_vec(),
                (RowKey::Component(_), Levels::Parts) => s.path.clone(),
            };
            match keys.iter_mut().find(|(k, _)| *k == key) {
                Some((_, placements)) => {
                    if !placements.contains(&placement) {
                        placements.push(placement);
                    }
                }
                None => keys.push((key, vec![placement])),
            }
        }
        let list: Vec<PartsListRow> = self.parts_list();
        // Every grouped source key keeps the same item number, including
        // balloons attached to a component other than the representative.
        type Group = (
            RowKey,
            Vec<(RowKey, Vec<OccurrenceUid>)>,
            usize,
            Option<mitcad_model::PartSignature>,
            Vec<String>,
        );
        let mut groups: Vec<Group> = Vec::new();
        for (key, placements) in &keys {
            let bodies = match key {
                RowKey::Body(body) => vec![*body],
                RowKey::Component(component) => self.component_bodies(*component),
            };
            for placement in placements {
                let mut signature = if !settings.group_identical {
                    None
                } else if let (RowKey::Component(component), Levels::Top) = (key, settings.levels) {
                    self.component_part_signature(*component, view.component, placement)
                } else {
                    self.part_signature(&bodies)
                };
                let mut materials =
                    if let (RowKey::Component(component), Levels::Top) = (key, settings.levels) {
                        if let Some(signature) = &signature {
                            signature.materials.clone()
                        } else {
                            self.component_part_materials(*component, view.component, placement)
                                .unwrap_or_default()
                        }
                    } else {
                        bodies
                            .iter()
                            .map(|body| {
                                let path = if matches!(key, RowKey::Body(_)) {
                                    seen.iter()
                                        .find(|s| s.body == *body)
                                        .map(|s| s.path.as_slice())
                                        .unwrap_or(placement)
                                } else {
                                    placement
                                };
                                self.body_material_under(*body, view.component, path)
                                    .0
                                    .id
                                    .clone()
                            })
                            .collect()
                    };
                materials.sort();
                materials.dedup();
                if let Some(signature) = &mut signature {
                    signature.materials = materials.clone();
                }
                let group = if settings.group_identical {
                    signature.as_ref().and_then(|s| {
                        groups
                            .iter_mut()
                            .find(|g| g.3.as_ref().is_some_and(|other| other.matches(s)))
                    })
                } else {
                    groups.iter_mut().find(|g| g.0 == *key)
                };
                if let Some(group) = group {
                    group.1.push((*key, placement.clone()));
                    group.2 += 1;
                    group.4.extend(materials);
                    group.4.sort();
                    group.4.dedup();
                } else {
                    groups.push((
                        *key,
                        vec![(*key, placement.clone())],
                        1,
                        signature,
                        materials,
                    ));
                }
            }
        }
        let mut columns = settings.shown_columns(false);
        let shown = columns.len();
        // Sorted by a column it does not show too: its cells, left off.
        // The items are numbered in the order of the rows, so sorted by
        // item the rows are numbered by name and `descending` lists them
        // from the last number up (mitcad#718: it did nothing).
        let by_item = settings.sort_column(false) == "item";
        let sort = if by_item {
            "name".to_owned()
        } else {
            settings.sort_column(false)
        };
        if !columns.contains(&sort) {
            columns.push(sort.clone());
        }
        let mut rows: Vec<(usize, Vec<String>)> = groups
            .iter()
            .enumerate()
            .map(|(index, (key, _, quantity, _, materials))| {
                let cells = columns
                    .iter()
                    .map(|c| {
                        if super::libraries::physical_material_column(c) && !materials.is_empty() {
                            return materials
                                .iter()
                                .filter_map(|id| self.material(id))
                                .map(|m| m.name.clone())
                                .collect::<Vec<_>>()
                                .join(", ");
                        }
                        self.parts_cell(*key, c, *quantity, &list)
                    })
                    .collect();
                (index, cells)
            })
            .collect();
        sorted(
            &mut rows,
            columns.iter().position(|c| *c == sort),
            settings.descending && !by_item,
        );
        columns.truncate(shown);
        for (_, cells) in &mut rows {
            cells.truncate(shown);
        }
        // Numbered in that order.
        let item = columns.iter().position(|c| c == "item");
        let mut items = Vec::new();
        let mut words = Vec::new();
        let count = rows.len();
        if by_item && settings.descending {
            rows.reverse();
        }
        for (n, (key, cells)) in rows.iter_mut().enumerate() {
            let number = if by_item && settings.descending {
                (count - n) as u32
            } else {
                n as u32 + 1
            };
            if let Some(i) = item {
                cells[i] = number.to_string();
            }
            let group = &groups[*key];
            items.extend(
                group
                    .1
                    .iter()
                    .map(|(key, path)| (*key, path.clone(), number)),
            );
            let quantity = group.2;
            words.push(format!(
                "{number} {} x{quantity}",
                self.parts_cell(group.0, "name", quantity, &list)
            ));
        }
        Ok(PartsList {
            view: view_id,
            levels: settings.levels,
            depth,
            items,
            label: if words.is_empty() {
                "no components".to_owned()
            } else {
                words.join("; ")
            },
            data: TableData {
                title: settings
                    .title
                    .clone()
                    .unwrap_or_else(|| "BILL OF MATERIALS".to_owned()),
                header: columns.iter().map(|c| header(c.as_str())).collect(),
                rows: rows.into_iter().map(|(_, cells)| cells).collect(),
                tags: Vec::new(),
                origin: None,
            },
        })
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait DrawingTablesLocal<K: Kernel> {
    /// A parts list's cell: its own columns, or a property of the
    /// component (none of a body's).
    fn parts_cell(
        &self,
        key: RowKey,
        column: &str,
        quantity: usize,
        list: &[PartsListRow],
    ) -> String;

    /// A parts list's cell of a material's property (mitcad#1213: its key or
    /// name, `density`), in the design's units, temperatures in the
    /// preferred unit; empty for another column or a value not given.
    fn material_cell(&self, material: &mitcad_model::material::Material, column: &str) -> String;
}

impl<K: Kernel> DrawingTablesLocal<K> for Document<K> {
    fn parts_cell(
        &self,
        key: RowKey,
        column: &str,
        quantity: usize,
        list: &[PartsListRow],
    ) -> String {
        match key {
            RowKey::Body(body) => match column {
                "quantity" => quantity.to_string(),
                "name" => self.body_name(body),
                "material" => self.body_material(body, None).0.name.clone(),
                // A property of its material (mitcad#1213).
                other => self.material_cell(self.body_material(body, None).0, other),
            },
            RowKey::Component(component) => {
                let row = list.iter().find(|r| r.component == component);
                match (column, row) {
                    ("item", _) => String::new(),
                    ("quantity", _) => quantity.to_string(),
                    ("name", Some(row)) => row.designation.clone(),
                    ("name", None) => self.assembly().name(component),
                    ("part_number", Some(row)) => row.part_number.clone(),
                    ("description", Some(row)) => self
                        .component_property_text(component, row, "Description")
                        .unwrap_or_default(),
                    ("material", Some(row)) => self
                        .component_property_text(component, row, "Material")
                        .unwrap_or_default(),
                    // A property of the component, else of the material its
                    // bodies are made of (mitcad#1213).
                    (key, Some(row)) => self
                        .component_property_text(component, row, key)
                        .filter(|text| !text.is_empty())
                        .or_else(|| {
                            super::materials::component_made_of(self, component)
                                .map(|m| self.material_cell(m, key))
                        })
                        .unwrap_or_default(),
                    (key, None) => super::materials::component_made_of(self, component)
                        .map(|m| self.material_cell(m, key))
                        .unwrap_or_default(),
                }
            }
        }
    }

    fn material_cell(&self, material: &mitcad_model::material::Material, column: &str) -> String {
        // Lengths, stresses and densities in the design's system,
        // temperatures in the preferred unit (mitcad#1213).
        let system = super::materials::MaterialsOps::material_system(self);
        let temperature = mitcad_model::material_units::preferred_temperature();
        super::materials::material_value_text(material, column, system, temperature)
            .unwrap_or_default()
    }
}

/// The letters of the `k`-th size of a hole table: A, B, …, Z, then AA, AB,
/// … (I, O and Q left out, as they read as digits).
fn size_letters(k: usize) -> String {
    const LETTERS: &[u8] = b"ABCDEFGHJKLMNPRSTUVWXYZ";
    let n = LETTERS.len();
    if k < n {
        (LETTERS[k] as char).to_string()
    } else {
        let k = k - n;
        format!("{}{}", LETTERS[(k / n) % n] as char, LETTERS[k % n] as char)
    }
}

#[cfg(test)]
mod tests {
    use super::size_letters;

    #[test]
    fn sizes_are_lettered_without_i_o_and_q() {
        assert_eq!(size_letters(0), "A");
        assert_eq!(size_letters(7), "H");
        assert_eq!(size_letters(8), "J");
        assert_eq!(size_letters(22), "Z");
        assert_eq!(size_letters(23), "AA");
        assert_eq!(size_letters(24), "AB");
    }
}
