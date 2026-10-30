// SPDX-License-Identifier: MIT
//! Clearance holes and screw head recesses for ISO metric screws from
//! `core/model/data/hole_sizes.json` (mitcad#903): the clearance hole
//! diameters of ISO 273 (fine, medium and coarse series), the counterbores
//! for ISO 4762 heads of DIN 974-1 (series 1; the depth the head height
//! with an allowance), the countersinks for ISO 10642 and ISO 7721 heads
//! (ISO 10642's head diameter, ISO 15065's countersink) and a tapped
//! hole's countersink just over the thread. The Hole panel fills its
//! fields from them (the `hole_sizes` query); a hole stores the values as
//! plain values, and a clearance hole names its size and fit
//! ([`HoleClearance`](super::hole::HoleClearance)) so that the panel can
//! show them again.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::thread_table::{self, ThreadStandard};

/// ISO 273's series of clearance holes: Fine, Normal (the medium series)
/// and Coarse.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearanceFit {
    Fine,
    #[default]
    Normal,
    Coarse,
}

impl ClearanceFit {
    pub const ALL: [ClearanceFit; 3] = [Self::Fine, Self::Normal, Self::Coarse];

    /// The name the Hole panel shows.
    pub fn title(self) -> &'static str {
        match self {
            Self::Fine => "Fine",
            Self::Normal => "Normal",
            Self::Coarse => "Coarse",
        }
    }
}

/// The full angle of the countersinks of the table (90 degrees), in
/// degrees.
pub const COUNTERSINK_ANGLE_DEGREES: f64 = 90.0;

/// A screw size's holes and recesses, in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrewSize {
    /// `M6`.
    pub size: String,
    /// The nominal thread diameter d.
    pub diameter: f64,
    /// ISO 273's clearance hole: fine, medium (Normal), coarse.
    pub clearance: [f64; 3],
    /// DIN 974-1's counterbore for an ISO 4762 head: diameter and depth.
    pub counterbore: Option<(f64, f64)>,
    /// The countersink at 90 degrees: the larger of ISO 10642's head
    /// diameter and ISO 15065's countersink; None for a size neither has.
    pub countersink: Option<f64>,
    /// ISO 10642's head diameter dk (theoretical, max), when it has the
    /// size.
    pub iso_10642_head: Option<f64>,
    /// ISO 15065's countersink diameter d2, when it has the size.
    pub iso_15065: Option<f64>,
}

impl ScrewSize {
    /// The clearance hole's diameter of a fit.
    pub fn clearance_hole(&self, fit: ClearanceFit) -> f64 {
        match fit {
            ClearanceFit::Fine => self.clearance[0],
            ClearanceFit::Normal => self.clearance[1],
            ClearanceFit::Coarse => self.clearance[2],
        }
    }
}

#[derive(Deserialize)]
struct Table {
    sizes: Vec<String>,
    iso_273: Section<ClearanceRow>,
    din_974_1: Section<CounterboreRow>,
    iso_10642: Section<HeadRow>,
    iso_15065: Section<CountersinkRow>,
    tapped_countersink: TappedCountersink,
}

#[derive(Deserialize)]
struct Section<R> {
    title: String,
    rows: Vec<R>,
}

#[derive(Deserialize)]
struct ClearanceRow {
    size: String,
    d: f64,
    fine: f64,
    medium: f64,
    coarse: f64,
}

#[derive(Deserialize)]
struct CounterboreRow {
    size: String,
    diameter: f64,
    depth: f64,
}

#[derive(Deserialize)]
struct HeadRow {
    size: String,
    head_diameter: f64,
}

#[derive(Deserialize)]
struct CountersinkRow {
    size: String,
    diameter: f64,
}

#[derive(Deserialize)]
struct TappedCountersink {
    factor: f64,
}

struct Sizes {
    sizes: Vec<ScrewSize>,
    titles: [String; 4],
    tapped_factor: f64,
}

fn table() -> &'static Sizes {
    static SIZES: OnceLock<Sizes> = OnceLock::new();
    SIZES.get_or_init(|| {
        let table: Table = serde_json::from_str(include_str!("../../data/hole_sizes.json"))
            .expect("the hole size table is valid JSON");
        let sizes = table
            .sizes
            .iter()
            .map(|size| {
                let clearance = table
                    .iso_273
                    .rows
                    .iter()
                    .find(|row| &row.size == size)
                    .expect("every size has a clearance hole");
                let counterbore = table
                    .din_974_1
                    .rows
                    .iter()
                    .find(|row| &row.size == size)
                    .map(|row| (row.diameter, row.depth));
                let head = table
                    .iso_10642
                    .rows
                    .iter()
                    .find(|row| &row.size == size)
                    .map(|row| row.head_diameter);
                let sink = table
                    .iso_15065
                    .rows
                    .iter()
                    .find(|row| &row.size == size)
                    .map(|row| row.diameter);
                let countersink = match (head, sink) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
                ScrewSize {
                    size: size.clone(),
                    diameter: clearance.d,
                    clearance: [clearance.fine, clearance.medium, clearance.coarse],
                    counterbore,
                    countersink,
                    iso_10642_head: head,
                    iso_15065: sink,
                }
            })
            .collect();
        Sizes {
            sizes,
            titles: [
                table.iso_273.title,
                table.din_974_1.title,
                table.iso_10642.title,
                table.iso_15065.title,
            ],
            tapped_factor: table.tapped_countersink.factor,
        }
    })
}

/// The sizes, smallest first.
pub fn sizes() -> &'static [ScrewSize] {
    &table().sizes
}

/// The size of a designation (`M6`, `m6`, ` M6 `), or why there is none.
pub fn lookup(size: &str) -> Result<&'static ScrewSize, String> {
    let wanted = size.trim();
    sizes()
        .iter()
        .find(|s| s.size.eq_ignore_ascii_case(wanted))
        .ok_or_else(|| {
            format!(
                "no clearance hole for the screw size {wanted}: the sizes are {}",
                sizes()
                    .iter()
                    .map(|s| s.size.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The titles of the standards the table comes from: the clearance holes,
/// the counterbores, the ISO 10642 heads and the ISO 15065 countersinks.
pub fn sources() -> &'static [String; 4] {
    &table().titles
}

/// A tapped hole's countersink diameter for a thread's major diameter:
/// just over it (the table's factor times it, rounded to 0.1 mm, at least
/// 0.1 mm over it), at [`COUNTERSINK_ANGLE_DEGREES`].
pub fn tapped_countersink(major: f64) -> f64 {
    let rounded = (major * table().tapped_factor * 10.0).round() / 10.0;
    // (Less a little for the rounding of the sum: 6 + 0.1 is 6.1.)
    rounded.max(((major + 0.1) * 10.0 - 1e-9).ceil() / 10.0)
}

/// The screw size of a tapped hole's thread size (mitcad#980): an ISO
/// metric thread's size (`6`, its nominal diameter in mm, whatever its
/// pitch) is the screw size of that nominal diameter (`M6`), whose
/// counterbore and counterdrill the tapped hole takes; None for a size the
/// table has no row for (`5.5`) and for every other standard.
pub fn of_thread(standard: ThreadStandard, size: &str) -> Option<&'static ScrewSize> {
    if standard != ThreadStandard::IsoMetric {
        return None;
    }
    let d: f64 = size.trim().parse().ok()?;
    sizes().iter().find(|s| (s.diameter - d).abs() < 1e-9)
}

/// A thread size's major diameter in mm (mitcad#980: a tapped hole's
/// countersink from its size); None for a size the standard lacks.
pub fn thread_major(standard: ThreadStandard, size: &str) -> Option<f64> {
    let wanted = size.trim();
    thread_table::sizes(standard)
        .into_iter()
        .find(|s| s.size == wanted)
        .map(|s| s.major)
}

/// The bore of a tapped hole of a thread designation (mitcad#980): its
/// basic minor diameter D1 in mm; None for a designation the standard
/// lacks.
pub fn thread_bore(standard: ThreadStandard, designation: &str) -> Option<f64> {
    thread_table::lookup(standard, designation)
        .ok()
        .map(|data| data.minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_size_has_its_clearance_holes_and_recesses() {
        assert_eq!(sizes().len(), 14);
        assert_eq!(sizes()[0].size, "M1.6");
        assert_eq!(sizes()[13].size, "M24");
        for size in sizes() {
            let [fine, medium, coarse] = size.clearance;
            assert!(
                size.diameter < fine && fine < medium && medium < coarse,
                "{size:?}"
            );
            let (bore, depth) = size.counterbore.expect("a counterbore");
            assert!(bore > coarse && depth > 0.0, "{size:?}");
            if let Some(sink) = size.countersink {
                assert!(sink > coarse, "{size:?}");
            }
        }
        let m6 = lookup(" m6 ").unwrap();
        assert_eq!(m6.clearance_hole(ClearanceFit::Normal), 6.6);
        assert_eq!(m6.counterbore, Some((11.0, 6.4)));
        assert_eq!(m6.countersink, Some(13.44));
        assert_eq!(lookup("M24").unwrap().countersink, None);
        assert!(lookup("M7").unwrap_err().contains("M1.6, M2"));
    }

    #[test]
    fn a_tapped_countersink_is_just_over_the_major_diameter() {
        assert_eq!(tapped_countersink(6.0), 6.6);
        assert_eq!(tapped_countersink(10.0), 11.0);
        // At least 0.1 over a small thread's.
        assert_eq!(tapped_countersink(1.0), 1.1);
        assert!((tapped_countersink(6.35) - 7.0).abs() < 1e-12);
    }

    #[test]
    fn a_tapped_hole_takes_the_screw_size_of_its_thread() {
        let metric = ThreadStandard::IsoMetric;
        // Any pitch of an ISO metric size is the size's screw.
        assert_eq!(of_thread(metric, "6").unwrap().size, "M6");
        assert_eq!(of_thread(metric, " 1.6 ").unwrap().size, "M1.6");
        let m10 = of_thread(metric, "10").unwrap();
        assert_eq!(m10.counterbore, Some((18.0, 10.6)));
        // ISO 261's sizes the table has no screw for, and other standards.
        assert!(of_thread(metric, "5.5").is_none());
        assert!(of_thread(metric, "30").is_none());
        assert!(of_thread(ThreadStandard::Unified, "1/4").is_none());
        assert!(of_thread(ThreadStandard::Custom, "6").is_none());
        assert_eq!(thread_major(metric, "6"), Some(6.0));
        assert_eq!(thread_major(metric, "7.5"), None);
        let bore = thread_bore(metric, "M6x1").unwrap();
        assert!((bore - (6.0 - 1.082532)).abs() < 1e-4, "{bore}");
        assert_eq!(thread_bore(metric, "M7x3"), None);
    }
}
