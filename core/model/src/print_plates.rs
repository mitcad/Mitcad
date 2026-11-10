// SPDX-License-Identifier: MIT
//! Print plates (mitcad#1050): the design carries the plates it prints
//! on, so that the 3MF goes to the slicer laid out and ready to print.
//!
//! **What goes on a plate** ([`PlateItem`]): a body, or an occurrence
//! (which brings every body it places, as assembled), with a count
//! ("4 of this on plate 2"). An item is on one plate at most: putting it
//! on another moves it. What no item names goes on the first plate, as
//! the design places it: each placement of a body there is a piece of its
//! own.
//!
//! **Layout.** Automatic within each plate, by the export's arranger
//! (`exchange/bed.rs`): each piece on its print bed face (else the
//! automatic orientation's), the pieces side by side along X, the row
//! centred on the plate, wrapped into rows along Y where it is wider than
//! the bed (mitcad#1316).
//!
//! **Plate settings** ([`PrintPlate::values`]): process values of the
//! key map, in its JSON form, as a profile keeps them. The plate is a
//! level of the settings order: profile < plate < component < occurrence
//! < body; its values come after the design's own (the root's values and
//! profiles) and before every component's, so a body's own value still
//! wins (`document/print_settings.rs`).
//!
//! The commands and queries are in `document/print_plates.rs`; the 3MF of
//! several plates in `exchange/plates.rs`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{BodyUid, OccurrenceUid};
use crate::print_keys::{self, PrintSet};

/// The most copies an item can have on a plate.
pub const MAX_COUNT: u32 = 999;

/// The most plates a design can have (the slicers' own limits are lower
/// or none; a grid of 36 plates is still a sensible project).
pub const MAX_PLATES: usize = 36;

/// What a plate item names: a body (every placement of it alike: the
/// body itself), or an occurrence by its path from the root (the bodies it
/// places, as assembled).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlateTarget {
    Body(BodyUid),
    Occurrence(Vec<OccurrenceUid>),
}

impl PlateTarget {
    /// `F1.b0`, or the occurrence's path of uids `O1/O4`.
    pub fn text(&self) -> String {
        match self {
            Self::Body(uid) => uid.to_string(),
            Self::Occurrence(path) => path_text(path),
        }
    }
}

/// A path of occurrence uids as text: `O1/O4`.
pub fn path_text(path: &[OccurrenceUid]) -> String {
    path.iter()
        .map(|o| o.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// A body or an occurrence on a plate, with how many of it are printed
/// there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ItemFile", into = "ItemFile")]
pub struct PlateItem {
    pub target: PlateTarget,
    pub count: u32,
}

/// A plate item as the design file keeps it: `{"body": "F1.b0", "count":
/// 4}` or `{"occurrence": ["O1", "O4"]}` (a count of 1 left out).
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<BodyUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    occurrence: Option<Vec<OccurrenceUid>>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    count: u32,
}

fn one() -> u32 {
    1
}

fn is_one(count: &u32) -> bool {
    *count == 1
}

impl TryFrom<ItemFile> for PlateItem {
    type Error = String;

    fn try_from(file: ItemFile) -> Result<Self, String> {
        let target = match (file.body, file.occurrence) {
            (Some(body), None) => PlateTarget::Body(body),
            (None, Some(path)) if !path.is_empty() => PlateTarget::Occurrence(path),
            (None, Some(_)) => {
                return Err(
                    "a plate item's occurrence is a path of one or more occurrences".into(),
                );
            }
            _ => return Err("a plate item names a body or an occurrence".into()),
        };
        Ok(PlateItem {
            target,
            count: file.count,
        })
    }
}

impl From<PlateItem> for ItemFile {
    fn from(item: PlateItem) -> Self {
        let (body, occurrence) = match item.target {
            PlateTarget::Body(uid) => (Some(uid), None),
            PlateTarget::Occurrence(path) => (None, Some(path)),
        };
        ItemFile {
            body,
            occurrence,
            count: item.count,
        }
    }
}

/// An item that `place_on_plate` took off another plate, since an item
/// is on one plate at most (mitcad#1595): the target, the plate it left
/// with the count it had there, and the plate it went to with the count it
/// has there now (the one given, else the one it had).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlateMove {
    pub target: PlateTarget,
    pub from_id: String,
    pub from_name: String,
    pub from_count: u32,
    pub to_id: String,
    pub to_name: String,
    pub count: u32,
}

/// A print plate: its id (`plate1`, never given again while the design
/// has it), its name for people, what goes on it and its own settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintPlate {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<PlateItem>,
    /// Process values of the key map by key, in its JSON form.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Value>,
}

impl PrintPlate {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            items: Vec::new(),
            values: BTreeMap::new(),
        }
    }

    /// The item of a target on this plate.
    pub fn item(&self, target: &PlateTarget) -> Option<&PlateItem> {
        self.items.iter().find(|i| i.target == *target)
    }

    /// The fields are valid: an id, a name, each item once with a count of
    /// 1 to [`MAX_COUNT`], each value a process key's (not one kept per
    /// slicer) of its kind.
    pub fn check(&self) -> Result<(), String> {
        if !crate::material::valid_id(&self.id) {
            return Err(format!(
                "{} is no plate id: use letters, digits, '_' and '-'",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            return Err(format!("plate {}: the name is empty", self.id));
        }
        for (i, item) in self.items.iter().enumerate() {
            if item.count == 0 || item.count > MAX_COUNT {
                return Err(format!(
                    "plate {}: {} has a count of {}; a plate takes 1 to {MAX_COUNT} of an item",
                    self.name,
                    item.target.text(),
                    item.count
                ));
            }
            if self.items[..i].iter().any(|o| o.target == item.target) {
                return Err(format!(
                    "plate {}: {} is listed more than once",
                    self.name,
                    item.target.text()
                ));
            }
        }
        for (key, value) in &self.values {
            check_value(key, value).map_err(|e| format!("plate {}: {e}", self.name))?;
        }
        Ok(())
    }

    /// Writes every value that reads as one of its key in the key map's
    /// JSON form (`"15%"` as 15), as a profile does.
    pub fn normalize(&mut self) {
        for (key, value) in self.values.iter_mut() {
            let Some(def) = print_keys::key(key).filter(|d| !d.per_dialect) else {
                continue;
            };
            if let Ok(parsed) = crate::print_profile::value_of(def, value) {
                *value = parsed.to_json();
            }
        }
    }
}

/// A plate value: a key of the `Print Process` set, not one kept per
/// slicer, with a value of its kind.
pub fn check_value(key: &str, value: &Value) -> Result<(), String> {
    let def = print_keys::key(key)
        .filter(|def| def.set == PrintSet::Process)
        .ok_or_else(|| format!("{key} is no key of the Print Process set"))?;
    if def.per_dialect {
        return Err(format!(
            "{key} names a slicer's own preset: a plate does not keep it"
        ));
    }
    print_keys::Value::from_json(def, value)
        .map(|_| ())
        .map_err(|e| format!("{key}: {e}"))
}

/// The first free plate id and the name that goes with it: `plate<n>`,
/// `Plate <n>`, the smallest `n` whose id and name are both free.
pub fn new_plate(plates: &[PrintPlate]) -> (String, String) {
    (1..)
        .map(|n| (format!("plate{n}"), format!("Plate {n}")))
        .find(|(id, name)| {
            !plates
                .iter()
                .any(|p| p.id == *id || p.name.trim().eq_ignore_ascii_case(name))
        })
        .expect("a free plate number")
}

/// The keys the Bambu family keeps per plate rather than per object
/// (Mitcad's keys): written in the plate's block of
/// `model_settings.config`, never as an object's override.
pub const BAMBU_PLATE_KEYS: [&str; 2] = ["spiral_vase", "complete_objects"];

/// Where plate `index` of `count` lies in a Bambu-family project, mm:
/// the plates in a grid of `ceil(sqrt(count))` columns (one more when the
/// square root rounds down), each a plate's width and depth apart plus a
/// fifth of them, the rows going towards −Y.
pub fn bambu_plate_origin(index: usize, count: usize, size: [f64; 2]) -> [f64; 2] {
    let root = (count.max(1) as f64).sqrt();
    let rounded = root.round();
    let columns = if root > rounded {
        rounded as usize + 1
    } else {
        rounded as usize
    }
    .max(1);
    let (row, column) = (index / columns, index % columns);
    let gap = 1.0 / 5.0;
    [
        column as f64 * size[0] * (1.0 + gap),
        0.0 - row as f64 * size[1] * (1.0 + gap),
    ]
}

/// Where bed `index` lies in a PrusaSlicer project of several beds
/// (PrusaSlicer 2.9 and later), mm: the beds on a grid that grows in
/// rings from the first ((0, 0), then (1, 0), (0, 1), (1, 1), (2, 0),
/// ...), a bed's size plus the gap apart, the gap 3/10 of the bed's
/// diagonal and at most 100 mm.
pub fn prusa_bed_origin(index: usize, size: [f64; 2]) -> [f64; 2] {
    let gap = (size[0].hypot(size[1]) * 0.3).min(100.0);
    let [x, y] = prusa_grid(index);
    [x as f64 * (size[0] + gap), y as f64 * (size[1] + gap)]
}

/// The grid place of PrusaSlicer's bed `index` (in its first quadrant).
fn prusa_grid(index: usize) -> [usize; 2] {
    if index == 0 {
        return [0, 0];
    }
    let mut id = index + 1;
    let mut ring = 1;
    while (ring + 1) * (ring + 1) < id {
        ring += 1;
    }
    id -= ring * ring;
    if id <= ring {
        [ring, id - 1]
    } else {
        [id - ring - 1, ring]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn items_keep_a_body_or_an_occurrence_with_a_count() {
        let mut plate = PrintPlate::new("plate1", "Plate 1");
        plate.items.push(PlateItem {
            target: PlateTarget::Body(BodyUid::new(crate::ids::FeatureUid(1), 0)),
            count: 4,
        });
        plate.items.push(PlateItem {
            target: PlateTarget::Occurrence(vec![OccurrenceUid(1), OccurrenceUid(4)]),
            count: 1,
        });
        let text = serde_json::to_value(&plate).unwrap();
        assert_eq!(
            text["items"],
            json!([{"body": "F1.b0", "count": 4}, {"occurrence": ["O1", "O4"]}])
        );
        let back: PrintPlate = serde_json::from_value(text).unwrap();
        assert_eq!(back, plate);
        assert!(serde_json::from_value::<PlateItem>(json!({"count": 2})).is_err());
        assert!(
            serde_json::from_value::<PlateItem>(json!({"body": "F1.b0", "occurrence": ["O1"]}))
                .is_err()
        );
        plate.items[0].count = 0;
        assert!(plate.check().is_err());
    }

    #[test]
    fn plate_values_are_process_keys() {
        let mut plate = PrintPlate::new("plate1", "Plate 1");
        plate.values.insert("spiral_vase".into(), json!(true));
        plate.values.insert("layer_height".into(), json!(0.3));
        assert_eq!(plate.check(), Ok(()));
        plate
            .values
            .insert("bed_shape".into(), json!([[0, 0], [1, 0], [1, 1]]));
        assert!(plate.check().is_err());
    }

    #[test]
    fn new_plates_take_the_first_free_number() {
        let plates = vec![
            PrintPlate::new("plate1", "Plate 1"),
            PrintPlate::new("plate3", "Vase"),
        ];
        assert_eq!(new_plate(&plates), ("plate2".into(), "Plate 2".into()));
    }

    #[test]
    fn slicers_lay_plates_out_on_their_grids() {
        // Two plates side by side, a fifth of the width apart.
        assert_eq!(bambu_plate_origin(0, 2, [256.0, 256.0]), [0.0, 0.0]);
        assert_eq!(bambu_plate_origin(1, 2, [256.0, 256.0]), [307.2, 0.0]);
        // Five plates: three columns, the second row towards -Y.
        assert_eq!(bambu_plate_origin(3, 5, [200.0, 100.0]), [0.0, -120.0]);
        // PrusaSlicer's rings: (1, 0), (0, 1), (1, 1), (2, 0).
        assert_eq!(prusa_grid(1), [1, 0]);
        assert_eq!(prusa_grid(2), [0, 1]);
        assert_eq!(prusa_grid(3), [1, 1]);
        assert_eq!(prusa_grid(4), [2, 0]);
        assert_eq!(prusa_grid(8), [2, 2]);
        // A 250 x 210 bed: the gap is 0.3 of its diagonal (97.98 mm).
        let [x, y] = prusa_bed_origin(1, [250.0, 210.0]);
        assert!((x - (250.0 + 0.3 * 250f64.hypot(210.0))).abs() < 1e-9);
        assert_eq!(y, 0.0);
        // A large bed: the gap at most 100 mm.
        assert_eq!(prusa_bed_origin(2, [400.0, 400.0]), [0.0, 500.0]);
    }
}
