// SPDX-License-Identifier: MIT
//! Standard sizes of deep groove ball bearings for the printable bearing
//! (mitcad#577): a designation's bore d, outside diameter D and width B in
//! millimetres, compiled by us from the standards' boundary dimensions:
//!
//! - ISO 15 (radial bearings, boundary dimensions): the dimension series
//!   18 (`68` and `68xx`), 19 (`69`, `69xx`), 00 (`160xx`), 10 (`60`,
//!   `60xx`), 02 (`62`, `62xx`) and 03 (`63`, `63xx`), with the usual
//!   designations of the deep groove type (6); the miniature sizes under
//!   their common `MR` designations (bore and outside diameter in the
//!   name: `MR105` is 5 x 10);
//! - ISO 8443 (flanged radial bearings): the flange's diameter and
//!   thickness of the `F` sizes (`F608`) and the miniature `MF` sizes;
//! - ABMA (the inch series `R`, ANSI/ABMA 12.2): the inch sizes converted
//!   at 25.4 mm to the inch.
//!
//! The widths are the open bearings'. A size is looked up by its
//! designation, case and spaces ignored (`f608` is `F608`).

/// A standard size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BearingSize {
    pub designation: &'static str,
    /// The series as the panel groups it: `60`, `62`, `63`, `68`, `69`,
    /// `160`, `MR`, `R` or `F`.
    pub series: &'static str,
    pub bore: f64,
    pub outside_diameter: f64,
    pub width: f64,
    /// A flanged size's flange: its diameter and thickness.
    pub flange: Option<(f64, f64)>,
}

const fn size(
    designation: &'static str,
    series: &'static str,
    d: f64,
    od: f64,
    b: f64,
) -> BearingSize {
    BearingSize {
        designation,
        series,
        bore: d,
        outside_diameter: od,
        width: b,
        flange: None,
    }
}

const fn flanged(
    designation: &'static str,
    d: f64,
    od: f64,
    b: f64,
    flange_diameter: f64,
    flange_thickness: f64,
) -> BearingSize {
    BearingSize {
        designation,
        series: "F",
        bore: d,
        outside_diameter: od,
        width: b,
        flange: Some((flange_diameter, flange_thickness)),
    }
}

/// An inch size: its dimensions in inches.
const fn inch(designation: &'static str, d: f64, od: f64, b: f64) -> BearingSize {
    size(designation, "R", d * 25.4, od * 25.4, b * 25.4)
}

/// Every standard size, in the order the panel lists them: by series, then
/// by bore.
pub const SIZES: &[BearingSize] = &[
    // Dimension series 10 (ISO 15): 60x and 60xx.
    size("604", "60", 4.0, 12.0, 4.0),
    size("605", "60", 5.0, 14.0, 5.0),
    size("606", "60", 6.0, 17.0, 6.0),
    size("607", "60", 7.0, 19.0, 6.0),
    size("608", "60", 8.0, 22.0, 7.0),
    size("609", "60", 9.0, 24.0, 7.0),
    size("6000", "60", 10.0, 26.0, 8.0),
    size("6001", "60", 12.0, 28.0, 8.0),
    size("6002", "60", 15.0, 32.0, 9.0),
    size("6003", "60", 17.0, 35.0, 10.0),
    size("6004", "60", 20.0, 42.0, 12.0),
    size("6005", "60", 25.0, 47.0, 12.0),
    size("6006", "60", 30.0, 55.0, 13.0),
    size("6007", "60", 35.0, 62.0, 14.0),
    size("6008", "60", 40.0, 68.0, 15.0),
    size("6009", "60", 45.0, 75.0, 16.0),
    size("6010", "60", 50.0, 80.0, 16.0),
    // Dimension series 02: 62x and 62xx.
    size("623", "62", 3.0, 10.0, 4.0),
    size("624", "62", 4.0, 13.0, 5.0),
    size("625", "62", 5.0, 16.0, 5.0),
    size("626", "62", 6.0, 19.0, 6.0),
    size("627", "62", 7.0, 22.0, 7.0),
    size("628", "62", 8.0, 24.0, 8.0),
    size("629", "62", 9.0, 26.0, 8.0),
    size("6200", "62", 10.0, 30.0, 9.0),
    size("6201", "62", 12.0, 32.0, 10.0),
    size("6202", "62", 15.0, 35.0, 11.0),
    size("6203", "62", 17.0, 40.0, 12.0),
    size("6204", "62", 20.0, 47.0, 14.0),
    size("6205", "62", 25.0, 52.0, 15.0),
    size("6206", "62", 30.0, 62.0, 16.0),
    size("6207", "62", 35.0, 72.0, 17.0),
    size("6208", "62", 40.0, 80.0, 18.0),
    size("6209", "62", 45.0, 85.0, 19.0),
    size("6210", "62", 50.0, 90.0, 20.0),
    // Dimension series 03: 63x and 63xx.
    size("634", "63", 4.0, 16.0, 5.0),
    size("635", "63", 5.0, 19.0, 6.0),
    size("636", "63", 6.0, 22.0, 7.0),
    size("637", "63", 7.0, 26.0, 9.0),
    size("638", "63", 8.0, 28.0, 9.0),
    size("639", "63", 9.0, 30.0, 10.0),
    size("6300", "63", 10.0, 35.0, 11.0),
    size("6301", "63", 12.0, 37.0, 12.0),
    size("6302", "63", 15.0, 42.0, 13.0),
    size("6303", "63", 17.0, 47.0, 14.0),
    size("6304", "63", 20.0, 52.0, 15.0),
    size("6305", "63", 25.0, 62.0, 17.0),
    size("6306", "63", 30.0, 72.0, 19.0),
    size("6307", "63", 35.0, 80.0, 21.0),
    size("6308", "63", 40.0, 90.0, 23.0),
    size("6309", "63", 45.0, 100.0, 25.0),
    size("6310", "63", 50.0, 110.0, 27.0),
    // Dimension series 18: 68x and 68xx.
    size("683", "68", 3.0, 7.0, 2.0),
    size("684", "68", 4.0, 9.0, 2.5),
    size("685", "68", 5.0, 11.0, 3.0),
    size("686", "68", 6.0, 13.0, 3.5),
    size("687", "68", 7.0, 14.0, 3.5),
    size("688", "68", 8.0, 16.0, 4.0),
    size("689", "68", 9.0, 17.0, 4.0),
    size("6800", "68", 10.0, 19.0, 5.0),
    size("6801", "68", 12.0, 21.0, 5.0),
    size("6802", "68", 15.0, 24.0, 5.0),
    size("6803", "68", 17.0, 26.0, 5.0),
    size("6804", "68", 20.0, 32.0, 7.0),
    size("6805", "68", 25.0, 37.0, 7.0),
    size("6806", "68", 30.0, 42.0, 7.0),
    size("6807", "68", 35.0, 47.0, 7.0),
    size("6808", "68", 40.0, 52.0, 7.0),
    size("6809", "68", 45.0, 58.0, 7.0),
    size("6810", "68", 50.0, 65.0, 7.0),
    // Dimension series 19: 69x and 69xx.
    size("693", "69", 3.0, 8.0, 3.0),
    size("694", "69", 4.0, 11.0, 4.0),
    size("695", "69", 5.0, 13.0, 4.0),
    size("696", "69", 6.0, 15.0, 5.0),
    size("697", "69", 7.0, 17.0, 5.0),
    size("698", "69", 8.0, 19.0, 6.0),
    size("699", "69", 9.0, 20.0, 6.0),
    size("6900", "69", 10.0, 22.0, 6.0),
    size("6901", "69", 12.0, 24.0, 6.0),
    size("6902", "69", 15.0, 28.0, 7.0),
    size("6903", "69", 17.0, 30.0, 7.0),
    size("6904", "69", 20.0, 37.0, 9.0),
    size("6905", "69", 25.0, 42.0, 9.0),
    size("6906", "69", 30.0, 47.0, 9.0),
    size("6907", "69", 35.0, 55.0, 10.0),
    size("6908", "69", 40.0, 62.0, 12.0),
    size("6909", "69", 45.0, 68.0, 12.0),
    size("6910", "69", 50.0, 72.0, 12.0),
    // Dimension series 00: 160xx.
    size("16001", "160", 12.0, 28.0, 7.0),
    size("16002", "160", 15.0, 32.0, 8.0),
    size("16003", "160", 17.0, 35.0, 8.0),
    size("16004", "160", 20.0, 42.0, 8.0),
    size("16005", "160", 25.0, 47.0, 8.0),
    size("16006", "160", 30.0, 55.0, 9.0),
    size("16007", "160", 35.0, 62.0, 9.0),
    size("16008", "160", 40.0, 68.0, 9.0),
    size("16009", "160", 45.0, 75.0, 10.0),
    size("16010", "160", 50.0, 80.0, 10.0),
    // Miniature sizes (ISO 15 boundaries, the MR designations).
    size("MR63", "MR", 3.0, 6.0, 2.0),
    size("MR74", "MR", 4.0, 7.0, 2.0),
    size("MR83", "MR", 3.0, 8.0, 2.5),
    size("MR84", "MR", 4.0, 8.0, 2.0),
    size("MR85", "MR", 5.0, 8.0, 2.0),
    size("MR95", "MR", 5.0, 9.0, 2.5),
    size("MR104", "MR", 4.0, 10.0, 3.0),
    size("MR105", "MR", 5.0, 10.0, 3.0),
    size("MR106", "MR", 6.0, 10.0, 2.5),
    size("MR115", "MR", 5.0, 11.0, 4.0),
    size("MR117", "MR", 7.0, 11.0, 2.5),
    size("MR126", "MR", 6.0, 12.0, 3.0),
    size("MR128", "MR", 8.0, 12.0, 2.5),
    size("MR137", "MR", 7.0, 13.0, 3.0),
    size("MR148", "MR", 8.0, 14.0, 3.5),
    // The inch series R (ABMA), in inches.
    inch("R2", 0.125, 0.375, 0.15625),
    inch("R3", 0.1875, 0.5, 0.196),
    inch("R4", 0.25, 0.625, 0.196),
    inch("R6", 0.375, 0.875, 0.21875),
    inch("R8", 0.5, 1.125, 0.25),
    inch("R10", 0.625, 1.375, 0.28125),
    inch("R12", 0.75, 1.625, 0.3125),
    inch("R144", 0.125, 0.25, 0.109375),
    inch("R155", 0.15625, 0.3125, 0.125),
    inch("R156", 0.1875, 0.3125, 0.125),
    inch("R166", 0.1875, 0.375, 0.125),
    inch("R168", 0.25, 0.375, 0.125),
    inch("R188", 0.25, 0.5, 0.1875),
    inch("R1810", 0.3125, 0.5, 0.15625),
    // Flanged sizes (ISO 8443 flanges).
    flanged("F623", 3.0, 10.0, 4.0, 11.5, 1.0),
    flanged("F624", 4.0, 13.0, 5.0, 15.0, 1.0),
    flanged("F625", 5.0, 16.0, 5.0, 18.0, 1.0),
    flanged("F626", 6.0, 19.0, 6.0, 22.0, 1.5),
    flanged("F605", 5.0, 14.0, 5.0, 16.0, 1.0),
    flanged("F606", 6.0, 17.0, 6.0, 19.0, 1.2),
    flanged("F608", 8.0, 22.0, 7.0, 25.0, 1.5),
    flanged("F688", 8.0, 16.0, 4.0, 18.0, 0.8),
    flanged("F695", 5.0, 13.0, 4.0, 15.0, 1.0),
    flanged("MF63", 3.0, 6.0, 2.0, 7.2, 0.6),
    flanged("MF84", 4.0, 8.0, 2.0, 9.2, 0.6),
    flanged("MF85", 5.0, 8.0, 2.0, 9.2, 0.6),
    flanged("MF105", 5.0, 10.0, 3.0, 11.2, 0.6),
    flanged("MF128", 8.0, 12.0, 2.5, 13.6, 0.6),
];

fn canonical(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect()
}

/// The size of a designation.
pub fn lookup(designation: &str) -> Option<&'static BearingSize> {
    let wanted = canonical(designation);
    SIZES.iter().find(|s| canonical(s.designation) == wanted)
}

/// The size of a designation, or why there is none.
pub fn size_of(designation: &str) -> Result<&'static BearingSize, String> {
    lookup(designation).ok_or_else(|| {
        format!(
            "{designation} is not a standard bearing size (60, 62, 63, 68, 69 and 160 series, MR, R and F sizes)"
        )
    })
}

/// What a size fits: a shaft by its bore, a hole by its outside diameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Bore,
    Outside,
}

/// The sizes whose bore (or outside diameter) lies within `clearance` of
/// `diameter`, the nearest first; sizes as near come in the table's order.
pub fn fitting(fit: Fit, diameter: f64, clearance: f64) -> Vec<&'static BearingSize> {
    let of = |s: &BearingSize| match fit {
        Fit::Bore => s.bore,
        Fit::Outside => s.outside_diameter,
    };
    let mut found: Vec<&'static BearingSize> = SIZES
        .iter()
        .filter(|s| (of(s) - diameter).abs() <= clearance + 1.0e-9)
        .collect();
    // A stable sort keeps the table's order among sizes as near.
    found.sort_by(|a, b| {
        (of(a) - diameter)
            .abs()
            .total_cmp(&(of(b) - diameter).abs())
    });
    found
}

/// A number as a size's name shows it: `8`, `2.5`, `9.525`.
pub fn short_number(value: f64) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.to_owned()
}
