// SPDX-License-Identifier: MIT
//! Purge volumes on filament changes (mitcad#705): how much the printer
//! purges when it changes from one filament to another, the table the
//! Bambu Studio family (`flush_volumes_matrix`) and PrusaSlicer
//! (`wiping_volumes_matrix`) keep in a project, a row per filament changed
//! from and a column per filament changed to, in mm³.
//!
//! **A material's values.** Each material carries two values in its
//! filament settings ([`crate::print_keys::MITCAD_OWN`]): how much it takes
//! to purge *into* it (`purge_into`, the new filament pushing the old one
//! out) and *out of* it (`purge_out`, what it leaves in the nozzle). A
//! material that sets neither takes them from its colour
//! ([`colour_defaults`]): a light colour shows what is left of the one
//! before, so purging into it takes more; a dark one stains what comes
//! after, so purging out of it takes more. By the colour's lightness `L`
//! (CIE L*, 0 black to 1 white):
//!
//! - into: 70 + 140 · L mm³,
//! - out of: 70 + 140 · (1 − L) mm³,
//!
//! so mid grey takes 140 each way (the slicers' own default pair), black
//! 70 into and 210 out, white 210 into and 70 out.
//!
//! **The computed table** ([`computed`]): from one filament to another,
//! what the first needs out plus what the second needs in, times the
//! process' purge multiplier (`flush_multiplier`, 1 when unset), rounded
//! to whole mm³; nothing from a filament to itself. Black to white so
//! takes 420 mm³, white to black 140.
//!
//! **Overrides** ([`PurgeOverride`]): the design keeps a volume set by
//! hand for an ordered pair of materials; it stands as it is (the
//! multiplier does not apply to it) until it is reset. [`Matrix`] gives
//! the design's filament slots each pair's computed volume, the volume
//! used and whether it was set by hand.
//!
//! **Written** ([`crate::print_files`]): the Bambu family gets the table
//! divided by the multiplier with the multiplier beside it (the slicer
//! multiplies), and each filament's two values (`flush_volumes_vector`,
//! out then in); PrusaSlicer, which has no multiplier, the table as it is
//! with the custom table switched on, and the values multiplied
//! (`wiping_volumes_extruders`). An import reads a project's table as the
//! overrides of its materials' pairs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A filament's purge volume into it and out of it (mm³), Mitcad keys.
pub const PURGE_INTO: &str = "purge_into";
pub const PURGE_OUT: &str = "purge_out";
/// The process' multiplier of computed volumes.
pub const MULTIPLIER: &str = "flush_multiplier";

/// The least volume a colour's default gives (mm³), and how much more the
/// lightness adds at most.
const LEAST: f64 = 70.0;
const SPAN: f64 = 140.0;

/// A filament's purge volumes (mm³).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PurgeValues {
    /// Purging into it: changing to it.
    pub into: f64,
    /// Purging out of it: changing from it.
    pub out: f64,
}

/// A purge volume set by hand for a pair of materials (by id): from one to
/// the other, in mm³.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeOverride {
    pub from: String,
    pub to: String,
    pub volume: f64,
}

impl PurgeOverride {
    /// Whether it is valid: two materials, a volume of zero or more.
    pub fn check(&self) -> Result<(), String> {
        if self.from.trim().is_empty() || self.to.trim().is_empty() {
            return Err("a purge volume names the materials it is between".to_owned());
        }
        if self.from == self.to {
            return Err(format!(
                "a purge volume is between two materials, not from {} to itself",
                self.from
            ));
        }
        check_volume(self.volume)
    }
}

/// Whether a purge volume can be one: finite, zero or more.
pub fn check_volume(volume: f64) -> Result<(), String> {
    if volume.is_finite() && volume >= 0.0 {
        Ok(())
    } else {
        Err(format!("a purge volume is 0 mm³ or more, got {volume}"))
    }
}

/// The lightness of an sRGB colour (components in [0, 1]): CIE L* / 100,
/// 0 black to 1 white.
pub fn lightness(color: [f64; 3]) -> f64 {
    let linear = |c: f64| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let [r, g, b] = color.map(linear);
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let l = if y > 216.0 / 24389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24389.0 / 27.0
    };
    (l / 100.0).clamp(0.0, 1.0)
}

/// A colour's default purge volumes, whole mm³ (none: mid grey's).
pub fn colour_defaults(color: Option<[f64; 3]>) -> PurgeValues {
    let l = color.map_or(0.5, lightness);
    PurgeValues {
        into: (LEAST + SPAN * l).round(),
        out: (LEAST + SPAN * (1.0 - l)).round(),
    }
}

/// A filament's purge volumes from its values (`get`: a filament key's
/// JSON value): the two it sets, else its colour's (`filament_colour`).
pub fn filament_values<'a>(get: impl Fn(&str) -> Option<&'a Value>) -> PurgeValues {
    let color = get("filament_colour")
        .and_then(Value::as_str)
        .and_then(crate::print_files::hex_rgb);
    let defaults = colour_defaults(color);
    let own = |key: &str| {
        get(key)
            .and_then(Value::as_f64)
            .filter(|v| check_volume(*v).is_ok())
    };
    PurgeValues {
        into: own(PURGE_INTO).unwrap_or(defaults.into),
        out: own(PURGE_OUT).unwrap_or(defaults.out),
    }
}

/// The multiplier a process value gives: a positive number, else 1.
pub fn multiplier_of(value: Option<&Value>) -> f64 {
    value
        .and_then(Value::as_f64)
        .filter(|m| m.is_finite() && *m > 0.0)
        .unwrap_or(1.0)
}

/// The computed volume from one filament to another: out of the first plus
/// into the second, times the multiplier, whole mm³.
pub fn computed(from: PurgeValues, to: PurgeValues, multiplier: f64) -> f64 {
    ((from.out + to.into) * multiplier).round()
}

/// A pair of the table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// The computed volume (0 from a filament to itself).
    pub computed: f64,
    /// The volume used: the override, else the computed one.
    pub volume: f64,
    /// Whether the design sets it by hand.
    pub edited: bool,
}

/// The purge table of filament slots.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub multiplier: f64,
    /// Each slot's purge values.
    pub slots: Vec<PurgeValues>,
    /// `cells[from][to]`.
    pub cells: Vec<Vec<Cell>>,
}

impl Matrix {
    /// The table of slots (each its material's id and purge values), the
    /// multiplier and the design's overrides.
    pub fn new(
        slots: &[(&str, PurgeValues)],
        multiplier: f64,
        overrides: &[PurgeOverride],
    ) -> Self {
        let cells = slots
            .iter()
            .enumerate()
            .map(|(i, (from, from_values))| {
                slots
                    .iter()
                    .enumerate()
                    .map(|(j, (to, to_values))| {
                        if i == j {
                            return Cell {
                                computed: 0.0,
                                volume: 0.0,
                                edited: false,
                            };
                        }
                        // Two colours of one material (mitcad#1055): their
                        // colours' volume, never one set by hand.
                        let computed = computed(*from_values, *to_values, multiplier);
                        let set = overrides
                            .iter()
                            .find(|o| from != to && o.from == *from && o.to == *to)
                            .map(|o| o.volume);
                        Cell {
                            computed,
                            volume: set.unwrap_or(computed),
                            edited: set.is_some(),
                        }
                    })
                    .collect()
            })
            .collect();
        Matrix {
            multiplier,
            slots: slots.iter().map(|(_, v)| *v).collect(),
            cells,
        }
    }

    /// The volumes used, row by row (from), each row's columns (to).
    pub fn volumes(&self) -> Vec<f64> {
        self.cells
            .iter()
            .flat_map(|row| row.iter().map(|c| c.volume))
            .collect()
    }
}

/// The overrides a project's table gives its filaments: each pair of
/// different filaments `(from, to, volume)` of a row-by-row `n × n` table
/// (its first `n × n` when it has one per extruder), times `multiplier`
/// (the Bambu family's tables are multiplied by the slicer). None when the
/// table is not of `n` filaments.
pub fn table_pairs(entries: &[f64], n: usize, multiplier: f64) -> Option<Vec<(usize, usize, f64)>> {
    if n == 0 || entries.len() < n * n || !entries.len().is_multiple_of(n * n) {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let volume = entries[i * n + j] * multiplier;
            if i != j && check_volume(volume).is_ok() {
                out.push((i, j, (volume * 1000.0).round() / 1000.0));
            }
        }
    }
    Some(out)
}

/// A material's purge values for the material editor's empty fields: by
/// the colour it prints with.
pub fn defaults_json(color: Option<[f64; 3]>) -> BTreeMap<&'static str, f64> {
    let d = colour_defaults(color);
    BTreeMap::from([(PURGE_INTO, d.into), (PURGE_OUT, d.out)])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const BLACK: [f64; 3] = [0.0, 0.0, 0.0];
    const WHITE: [f64; 3] = [1.0, 1.0, 1.0];

    #[test]
    fn colours_give_more_into_light_and_out_of_dark() {
        assert_eq!(
            colour_defaults(Some(BLACK)),
            PurgeValues {
                into: 70.0,
                out: 210.0
            }
        );
        assert_eq!(
            colour_defaults(Some(WHITE)),
            PurgeValues {
                into: 210.0,
                out: 70.0
            }
        );
        // #777777 is mid grey (L* 50): the slicers' own pair; no colour
        // the same.
        let grey = [119.0 / 255.0; 3];
        let mid = PurgeValues {
            into: 140.0,
            out: 140.0,
        };
        assert_eq!(colour_defaults(Some(grey)), mid);
        assert_eq!(colour_defaults(None), mid);
        assert!((lightness(grey) - 0.5).abs() < 0.001);
    }

    #[test]
    fn dark_to_light_takes_more_than_light_to_dark() {
        let black = colour_defaults(Some(BLACK));
        let white = colour_defaults(Some(WHITE));
        assert_eq!(computed(black, white, 1.0), 420.0);
        assert_eq!(computed(white, black, 1.0), 140.0);
        // Times the multiplier, whole mm³.
        assert_eq!(computed(black, white, 1.5), 630.0);
        assert_eq!(computed(white, black, 0.333), 47.0);
    }

    #[test]
    fn a_filaments_own_values_win_over_its_colour() {
        let filament = BTreeMap::from([
            ("filament_colour".to_owned(), json!("#000000")),
            (PURGE_INTO.to_owned(), json!(95)),
        ]);
        let values = filament_values(|k| filament.get(k));
        assert_eq!(
            values,
            PurgeValues {
                into: 95.0,
                out: 210.0
            }
        );
        // A negative value is none.
        let filament = BTreeMap::from([(PURGE_OUT.to_owned(), json!(-5))]);
        assert_eq!(filament_values(|k| filament.get(k)).out, 140.0);
    }

    #[test]
    fn the_multiplier_is_positive_or_one() {
        assert_eq!(multiplier_of(Some(&json!(1.2))), 1.2);
        assert_eq!(multiplier_of(Some(&json!(0))), 1.0);
        assert_eq!(multiplier_of(Some(&json!("x"))), 1.0);
        assert_eq!(multiplier_of(None), 1.0);
    }

    #[test]
    fn the_matrix_takes_overrides_as_they_are() {
        let black = colour_defaults(Some(BLACK));
        let white = colour_defaults(Some(WHITE));
        let overrides = [PurgeOverride {
            from: "white".to_owned(),
            to: "black".to_owned(),
            volume: 300.0,
        }];
        let m = Matrix::new(&[("black", black), ("white", white)], 2.0, &overrides);
        assert_eq!(m.volumes(), vec![0.0, 840.0, 300.0, 0.0]);
        assert!(m.cells[1][0].edited && !m.cells[0][1].edited);
        assert_eq!(m.cells[1][0].computed, 280.0);
        assert!(!m.cells[0][0].edited);
    }

    #[test]
    fn a_projects_table_gives_its_pairs() {
        // Two extruders' tables of three filaments: the first one's.
        let mut entries: Vec<f64> = (0..9).map(|k| f64::from(k * 10)).collect();
        entries.extend(vec![1.0; 9]);
        let pairs = table_pairs(&entries, 3, 1.5).unwrap();
        assert_eq!(pairs.len(), 6);
        assert_eq!(pairs[0], (0, 1, 15.0));
        assert_eq!(pairs[5], (2, 1, 105.0));
        assert!(table_pairs(&entries[..8], 3, 1.0).is_none());
    }

    #[test]
    fn overrides_are_between_two_materials() {
        let mut o = PurgeOverride {
            from: "pla".to_owned(),
            to: "petg".to_owned(),
            volume: 200.0,
        };
        o.check().unwrap();
        o.volume = -1.0;
        assert!(o.check().is_err());
        o.volume = 1.0;
        o.to = "pla".to_owned();
        assert!(o.check().is_err());
    }
}
