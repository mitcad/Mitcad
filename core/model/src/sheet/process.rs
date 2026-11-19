// SPDX-License-Identifier: MIT
//! Process profiles of sheet parts (mitcad#1939; commands.md, "Sheet
//! process profiles"): what a sheet part is made of and how decides its
//! allowances, joints, commands and output, held in its sheet rules.
//!
//! - **Metal** folds with a bend allowance (a K-factor, or a measured
//!   allowance or deduction per fold) and has hems and reliefs.
//! - **Board** (cardboard, corrugated board, fluted plastic sheet) folds
//!   at creases: its allowance comes from a crease table (by thickness,
//!   flute, direction and side) or a measured value per fold; it has glue
//!   flaps, tuck and lock flaps, reverse creases and standard boxes, and
//!   its die line is written with cut and crease layers.
//! - **Laser sheet** (plywood, MDF, acrylic) is not folded: its plates are
//!   ordinary solids of the sheet's thickness joined by finger joints,
//!   T-nut slots or mortises and tenons, cut with the laser's kerf; the one
//!   bend it has is a kerf bend, a living hinge whose flat pattern carries
//!   a slit lattice.
//!
//! A profile's specific logic lives here (the commands it offers, its
//! output layers, its joint defaults), not as branches in each feature.

use serde::{Deserialize, Serialize};

use crate::material::MaterialClass;

/// A sheet part's process profile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetProcess {
    #[default]
    Metal,
    Board,
    LaserSheet,
}

impl SheetProcess {
    pub const ALL: [SheetProcess; 3] = [
        SheetProcess::Metal,
        SheetProcess::Board,
        SheetProcess::LaserSheet,
    ];

    /// Its id in commands and files.
    pub fn id(self) -> &'static str {
        match self {
            SheetProcess::Metal => "metal",
            SheetProcess::Board => "board",
            SheetProcess::LaserSheet => "laser_sheet",
        }
    }

    /// Its name as the UI shows it.
    pub fn label(self) -> &'static str {
        match self {
            SheetProcess::Metal => "Metal",
            SheetProcess::Board => "Board",
            SheetProcess::LaserSheet => "Laser Sheet",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The profile a material's class gives when the rules name none:
    /// metal for metal, laser sheet for wood and sheet plastics; board is
    /// never guessed from a class (a material says so by its own
    /// `sheet_process`).
    pub fn from_class(class: MaterialClass) -> Option<Self> {
        match class {
            MaterialClass::Metal => Some(SheetProcess::Metal),
            MaterialClass::Wood | MaterialClass::Plastic => Some(SheetProcess::LaserSheet),
            MaterialClass::FdmPlastic | MaterialClass::Resin | MaterialClass::Other => None,
        }
    }

    /// Whether the profile's parts fold in the fold engine (metal and
    /// board); laser-sheet plates are joined instead, and bend only by a
    /// kerf bend.
    pub fn folds(self) -> bool {
        self != SheetProcess::LaserSheet
    }

    /// The default finger width of a finger joint, mm: 2.5 times the
    /// sheet's thickness.
    pub fn finger_width(self, thickness: f64) -> f64 {
        2.5 * thickness
    }
}

/// The commands that belong to some profiles only, with the profiles they
/// belong to. Commands not listed are offered for every profile.
const PROCESS_COMMANDS: &[(&str, &[SheetProcess])] = &[
    ("sheet.corner_relief", &[SheetProcess::Metal]),
    ("sheet.glue_flap", &[SheetProcess::Board]),
    ("sheet.standard_box", &[SheetProcess::Board]),
    ("sheet.export_die_line", &[SheetProcess::Board]),
    ("sheet.finger_joint", &[SheetProcess::LaserSheet]),
    ("sheet.t_nut_slot", &[SheetProcess::LaserSheet]),
    ("sheet.kerf_bend", &[SheetProcess::LaserSheet]),
    ("sheet.flat_pack", &[SheetProcess::LaserSheet]),
];

/// Every command the profiles filter, in the table's order.
pub fn filtered_commands() -> impl Iterator<Item = &'static str> {
    PROCESS_COMMANDS.iter().map(|(id, _)| *id)
}

/// Whether `command` belongs to `process`: Ok, or the reason it does not,
/// in words (shown as the disabled command's tooltip).
pub fn fits(process: SheetProcess, command: &str) -> Result<(), String> {
    let Some((_, allowed)) = PROCESS_COMMANDS.iter().find(|(id, _)| *id == command) else {
        return Ok(());
    };
    if allowed.contains(&process) {
        return Ok(());
    }
    let what = match command {
        "sheet.corner_relief" => "Corner reliefs",
        "sheet.glue_flap" => "Glue flaps",
        "sheet.standard_box" => "Standard boxes",
        "sheet.export_die_line" => "Die lines",
        "sheet.finger_joint" => "Finger joints",
        "sheet.t_nut_slot" => "T-nut slots",
        "sheet.kerf_bend" => "Kerf bends",
        "sheet.flat_pack" => "Flat packs",
        _ => "This command",
    };
    let belongs = match allowed[0] {
        SheetProcess::Metal => "metal sheet",
        SheetProcess::Board => "board (cardboard, corrugated board, fluted plastic sheet)",
        SheetProcess::LaserSheet => "laser-cut sheet (plywood, MDF, acrylic)",
    };
    Err(format!(
        "{what} are for {belongs}; this part's sheet rules are {}.",
        match process {
            SheetProcess::Metal => "for metal",
            SheetProcess::Board => "for board",
            SheetProcess::LaserSheet => "for laser sheet",
        }
    ))
}

/// The layers a profile's output (the Flat Pattern's DXF, a die line, a
/// cut sheet) writes each meaning on, with their colours (DXF colour
/// numbers): an output preset maps meanings to a machine's layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputPreset {
    /// Outer contours.
    pub cut_outer: String,
    /// Holes, inner contours and a kerf bend's slits.
    pub cut_inner: String,
    /// Bends or creases whose inside is on the viewed side.
    pub bend_up: String,
    /// Bends or creases whose inside is on the other side.
    pub bend_down: String,
    /// Lines scored part of the way through.
    pub score: String,
    /// Marks engraved on the surface.
    pub engrave: String,
}

impl OutputPreset {
    /// The default preset of a profile: metal's layers as they always
    /// were; board's die line with one cut layer and its creases (a
    /// reverse crease on a layer of its own); laser sheet's cut, score and
    /// engrave layers.
    pub fn of(process: SheetProcess) -> Self {
        let s = |t: &str| t.to_owned();
        match process {
            SheetProcess::Metal => OutputPreset {
                cut_outer: s("CUT_OUTER"),
                cut_inner: s("CUT_INNER"),
                bend_up: s("BEND_UP"),
                bend_down: s("BEND_DOWN"),
                score: s("SCORE"),
                engrave: s("ENGRAVE"),
            },
            SheetProcess::Board => OutputPreset {
                cut_outer: s("CUT"),
                cut_inner: s("CUT"),
                bend_up: s("CREASE"),
                bend_down: s("CREASE_REVERSE"),
                score: s("SCORE"),
                engrave: s("PRINT"),
            },
            SheetProcess::LaserSheet => OutputPreset {
                cut_outer: s("CUT_OUTER"),
                cut_inner: s("CUT_INNER"),
                bend_up: s("SCORE"),
                bend_down: s("SCORE"),
                score: s("SCORE"),
                engrave: s("ENGRAVE"),
            },
        }
    }

    /// The DXF colour of a layer of this preset: cuts white and magenta,
    /// creases and bends green and red, scores blue, engraving yellow.
    pub fn colour(&self, layer: &str) -> i16 {
        if layer == self.cut_outer {
            7
        } else if layer == self.cut_inner {
            6
        } else if layer == self.bend_up {
            3
        } else if layer == self.bend_down {
            1
        } else if layer == self.score {
            5
        } else if layer == self.engrave {
            2
        } else {
            8
        }
    }
}

/// A board the board profile starts from (mitcad#1939): its thickness, its
/// flute and a crease deduction of a 90 degree fold to start with. The
/// deductions are starting values, not measurements: calibrate them by
/// folding a sample of the board and editing the rules' crease table.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BoardPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub thickness: f64,
    pub flute: Option<&'static str>,
    pub deduction: f64,
}

/// The boards of the board profile: folding carton, single and double
/// wall corrugated board, and corrugated (fluted) polypropylene, which
/// creases nearly sharp (a deduction close to 0).
pub const BOARD_PRESETS: &[BoardPreset] = &[
    BoardPreset {
        id: "folding_carton",
        name: "Folding carton 0.5 mm",
        thickness: 0.5,
        flute: None,
        deduction: 0.5,
    },
    BoardPreset {
        id: "e_flute",
        name: "Corrugated E flute 1.5 mm",
        thickness: 1.5,
        flute: Some("E"),
        deduction: 1.5,
    },
    BoardPreset {
        id: "b_flute",
        name: "Corrugated B flute 3 mm",
        thickness: 3.0,
        flute: Some("B"),
        deduction: 3.0,
    },
    BoardPreset {
        id: "c_flute",
        name: "Corrugated C flute 4 mm",
        thickness: 4.0,
        flute: Some("C"),
        deduction: 4.0,
    },
    BoardPreset {
        id: "bc_flute",
        name: "Corrugated BC double wall 7 mm",
        thickness: 7.0,
        flute: Some("BC"),
        deduction: 7.0,
    },
    BoardPreset {
        id: "polypropylene_3",
        name: "Corrugated polypropylene 3 mm",
        thickness: 3.0,
        flute: None,
        deduction: 0.3,
    },
    BoardPreset {
        id: "polypropylene_4",
        name: "Corrugated polypropylene 4 mm",
        thickness: 4.0,
        flute: None,
        deduction: 0.4,
    },
    BoardPreset {
        id: "polypropylene_5",
        name: "Corrugated polypropylene 5 mm",
        thickness: 5.0,
        flute: None,
        deduction: 0.5,
    },
];

/// A board preset by its id.
pub fn board_preset(id: &str) -> Option<&'static BoardPreset> {
    BOARD_PRESETS.iter().find(|b| b.id == id)
}

/// The JSON of the `sheet_process` query's commands: each filtered
/// command with whether it fits `process` and why not.
pub fn commands_json(process: SheetProcess) -> serde_json::Value {
    serde_json::Value::Array(
        filtered_commands()
            .map(|command| match fits(process, command) {
                Ok(()) => serde_json::json!({"command": command, "available": true}),
                Err(reason) => {
                    serde_json::json!({"command": command, "available": false, "reason": reason})
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_filtered_by_profile_with_the_reason_in_words() {
        assert!(fits(SheetProcess::Metal, "sheet.corner_relief").is_ok());
        assert!(fits(SheetProcess::Board, "sheet.glue_flap").is_ok());
        assert!(fits(SheetProcess::LaserSheet, "sheet.finger_joint").is_ok());
        // Commands of every profile.
        for process in SheetProcess::ALL {
            assert!(fits(process, "sheet.face").is_ok());
            assert!(fits(process, "sheet.fold").is_ok());
        }
        let reason = fits(SheetProcess::Board, "sheet.finger_joint").unwrap_err();
        assert!(
            reason.starts_with("Finger joints are for laser-cut sheet"),
            "{reason}"
        );
        assert!(
            reason.ends_with("this part's sheet rules are for board."),
            "{reason}"
        );
        let reason = fits(SheetProcess::LaserSheet, "sheet.corner_relief").unwrap_err();
        assert!(reason.contains("metal sheet"), "{reason}");
        let json = commands_json(SheetProcess::Metal);
        let finger = json
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["command"] == "sheet.finger_joint")
            .unwrap();
        assert_eq!(finger["available"], false);
    }

    #[test]
    fn a_class_gives_its_profile_but_never_board() {
        assert_eq!(
            SheetProcess::from_class(MaterialClass::Metal),
            Some(SheetProcess::Metal)
        );
        assert_eq!(
            SheetProcess::from_class(MaterialClass::Wood),
            Some(SheetProcess::LaserSheet)
        );
        assert_eq!(SheetProcess::from_class(MaterialClass::Other), None);
        for process in SheetProcess::ALL {
            assert_eq!(SheetProcess::from_id(process.id()), Some(process));
        }
    }

    #[test]
    fn board_writes_cut_and_crease_layers() {
        let board = OutputPreset::of(SheetProcess::Board);
        assert_eq!(board.cut_outer, "CUT");
        assert_eq!(board.cut_inner, "CUT");
        assert_eq!(board.bend_up, "CREASE");
        assert_eq!(board.bend_down, "CREASE_REVERSE");
        let metal = OutputPreset::of(SheetProcess::Metal);
        assert_eq!(metal.bend_up, "BEND_UP");
        assert_eq!(metal.colour("BEND_DOWN"), 1);
    }
}
