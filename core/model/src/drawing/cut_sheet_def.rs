// SPDX-License-Identifier: MIT
//! Cut sheets (mitcad#853): a drawing of its own kind that collects flat
//! parts at 1:1 for cutting them out of a sheet of material (a laser, a
//! water jet, a plasma cutter). The design keeps the material's size and
//! the parts: each a body seen along the normal of one of its planar faces
//! (its largest when none is chosen), placed and turned on the sheet by
//! hand. Their outlines are worked out from the model whenever they are
//! asked for (`mitcad_model_drawing::cut_sheet`), so a cut sheet follows
//! the model; a part whose body is gone stays, marked as missing.
//!
//! A part's `place` is where the middle of its outline's box goes on the
//! sheet (millimetres from the sheet's lower left corner), and it turns
//! about that middle by `rotation` (degrees counter-clockwise), so that a
//! part that grows or shrinks stays where it was put.

use serde::{Deserialize, Serialize};

use super::P2;
use crate::features::OccurrencePath;
use crate::ids::BodyUid;
use crate::topo::FaceName;

/// The material's width of a new cut sheet, mm.
pub const DEFAULT_WIDTH: f64 = 600.0;
/// The material's height of a new cut sheet, mm.
pub const DEFAULT_HEIGHT: f64 = 300.0;
/// The largest side of a sheet of material, mm.
pub const MAX_SIZE: f64 = 100_000.0;
/// How many copies of a part one command adds at most.
pub const MAX_COUNT: u32 = 1000;

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// What a cut sheet keeps besides its drawing's name and folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutSheet {
    /// The material's width, mm.
    pub width: f64,
    /// The material's height, mm.
    pub height: f64,
    /// In the order they were added; a part added with a count is that
    /// many parts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<CutPart>,
}

impl Default for CutSheet {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            parts: Vec::new(),
        }
    }
}

/// One part on a cut sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutPart {
    /// Unique on the cut sheet, from 1 (the `view` of the drawing
    /// commands).
    pub id: u32,
    pub body: BodyUid,
    /// The occurrence that places the body (a path from the drawing's
    /// component); wherever it is shown when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    /// The planar face it is seen along (from outside the body); its
    /// largest planar face when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<FaceName>,
    /// Where the middle of its outline's box lies on the sheet, mm.
    pub place: P2,
    /// How far it is turned about that middle, degrees counter-clockwise.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
}

impl CutSheet {
    pub fn part(&self, id: u32) -> Option<&CutPart> {
        self.parts.iter().find(|p| p.id == id)
    }

    pub fn part_mut(&mut self, id: u32) -> Option<&mut CutPart> {
        self.parts.iter_mut().find(|p| p.id == id)
    }

    /// The id the next part takes.
    pub fn next_part_id(&self) -> u32 {
        self.parts.iter().map(|p| p.id).max().unwrap_or(0) + 1
    }

    /// Why the cut sheet cannot be kept, if it cannot (not its geometry).
    pub fn check(&self) -> Result<(), String> {
        check_size(self.width, self.height)?;
        let mut ids: Vec<u32> = Vec::with_capacity(self.parts.len());
        for part in &self.parts {
            if part.id == 0 {
                return Err("a cut sheet's parts are numbered from 1".to_owned());
            }
            if ids.contains(&part.id) {
                return Err(format!("part {} is listed more than once", part.id));
            }
            ids.push(part.id);
            if !(part.place[0].is_finite() && part.place[1].is_finite()) {
                return Err(format!("part {} has no place on the sheet", part.id));
            }
            if !part.rotation.is_finite() {
                return Err(format!("part {} is turned by no number", part.id));
            }
        }
        Ok(())
    }
}

/// Whether a sheet of material of this size can be kept.
pub fn check_size(width: f64, height: f64) -> Result<(), String> {
    let fine = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_SIZE;
    if fine(width) && fine(height) {
        Ok(())
    } else {
        Err(format!(
            "a cut sheet's material is more than 0 and at most {MAX_SIZE} mm each way, not {width} × {height}"
        ))
    }
}

/// An angle in degrees brought into (-180, 180].
pub fn normalized_degrees(degrees: f64) -> f64 {
    let mut d = degrees.rem_euclid(360.0);
    if d > 180.0 {
        d -= 360.0;
    }
    // Whole turns written exactly (a quarter turn stays 90, not 89.99…).
    let rounded = d.round();
    if (d - rounded).abs() < 1e-9 {
        rounded + 0.0
    } else {
        d
    }
}
