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
//!
//! For printed parts (mitcad#1518) the table also has ISO 4032's nuts,
//! whose hex pockets ([`ScrewSize::nut_pocket`]) are the nut with a
//! clearance, and a series of heat-set threaded inserts: the hole diameter
//! and depth per screw size ([`ScrewSize::insert`]) from an insert's
//! published dimensions, the source named in the data file.

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
    /// ISO 4032's nut (mitcad#1518): its width across flats s and its
    /// height m (max).
    pub nut: (f64, f64),
    /// The heat-set insert of the size (mitcad#1518), when the series has
    /// one.
    pub insert: Option<InsertHole>,
}

/// A heat-set insert's hole (mitcad#1518), in millimetres: the published
/// hole diameter, the insert's length, and the hole's depth (the length
/// with the table's allowance).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InsertHole {
    pub diameter: f64,
    pub length: f64,
    pub depth: f64,
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

    /// The hex pocket of the size's ISO 4032 nut (mitcad#1518): its width
    /// across flats and its depth, the nut's across flats and height each
    /// with `clearance` (rounded to a micrometre: 5.5 + 0.3 is 5.8).
    pub fn nut_pocket(&self, clearance: f64) -> (f64, f64) {
        let round = |v: f64| (v * 1e6).round() / 1e6;
        let (across_flats, height) = self.nut;
        (round(across_flats + clearance), round(height + clearance))
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
    iso_4032: Section<NutRow>,
    nut_pocket: NutPocket,
    heat_set_insert: InsertSection,
}

#[derive(Deserialize)]
struct NutRow {
    size: String,
    across_flats: f64,
    height: f64,
}

#[derive(Deserialize)]
struct NutPocket {
    clearance: f64,
}

#[derive(Deserialize)]
struct InsertSection {
    title: String,
    allowance: f64,
    rows: Vec<InsertRow>,
}

#[derive(Deserialize)]
struct InsertRow {
    size: String,
    diameter: f64,
    length: f64,
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
    /// The titles of ISO 4032 and of the insert series (mitcad#1518).
    printed_titles: [String; 2],
    nut_clearance: f64,
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
                let nut = table
                    .iso_4032
                    .rows
                    .iter()
                    .find(|row| &row.size == size)
                    .expect("every size has an ISO 4032 nut");
                let insert = table
                    .heat_set_insert
                    .rows
                    .iter()
                    .find(|row| &row.size == size)
                    .map(|row| InsertHole {
                        diameter: row.diameter,
                        length: row.length,
                        depth: ((row.length + table.heat_set_insert.allowance) * 1e6).round() / 1e6,
                    });
                ScrewSize {
                    size: size.clone(),
                    diameter: clearance.d,
                    clearance: [clearance.fine, clearance.medium, clearance.coarse],
                    counterbore,
                    countersink,
                    iso_10642_head: head,
                    iso_15065: sink,
                    nut: (nut.across_flats, nut.height),
                    insert,
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
            printed_titles: [table.iso_4032.title, table.heat_set_insert.title],
            nut_clearance: table.nut_pocket.clearance,
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

/// The titles of the printed parts' sections (mitcad#1518): ISO 4032's
/// nuts and the heat-set insert series (naming its source).
pub fn printed_sources() -> &'static [String; 2] {
    &table().printed_titles
}

/// The clearance a nut pocket has over its nut when none is given
/// (mitcad#1518), on the width across flats and on the depth.
pub fn nut_clearance() -> f64 {
    table().nut_clearance
}

/// The heat-set insert hole of a size (`M3`), or why there is none
/// (mitcad#1518).
pub fn insert(size: &str) -> Result<InsertHole, String> {
    let wanted = size.trim();
    lookup(wanted)?.insert.ok_or_else(|| {
        format!(
            "no heat-set insert for the screw size {wanted}: the sizes are {}",
            sizes()
                .iter()
                .filter(|s| s.insert.is_some())
                .map(|s| s.size.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
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

    #[test]
    fn nut_pockets_are_iso_4032_nuts_with_the_clearance() {
        // mitcad#1518: every size has its nut, wider across the flats than
        // the counterbore's screw and lower than wide.
        for size in sizes() {
            let (s, m) = size.nut;
            assert!(s > size.diameter && m > 0.0 && m < s, "{size:?}");
        }
        let m3 = lookup("M3").unwrap();
        assert_eq!(m3.nut, (5.5, 2.4));
        assert_eq!(nut_clearance(), 0.3);
        assert_eq!(m3.nut_pocket(nut_clearance()), (5.8, 2.7));
        assert_eq!(m3.nut_pocket(0.0), (5.5, 2.4));
        assert_eq!(lookup("M10").unwrap().nut, (16.0, 8.4));
        assert!(printed_sources()[0].contains("ISO 4032"));
    }

    #[test]
    fn insert_holes_come_from_the_series() {
        // mitcad#1518: the published hole diameter, the insert's length and
        // the depth 1 mm over it.
        let m3 = insert(" m3 ").unwrap();
        assert_eq!(
            m3,
            InsertHole {
                diameter: 4.0,
                length: 5.7,
                depth: 6.7
            }
        );
        assert_eq!(insert("M6").unwrap().depth, 13.7);
        for (screw, hole) in sizes().iter().filter_map(|s| s.insert.map(|i| (s, i))) {
            assert!(hole.diameter > screw.diameter && hole.depth > hole.length);
        }
        let error = insert("M8").unwrap_err();
        assert!(
            error.contains("no heat-set insert for the screw size M8")
                && error.contains("M2, M2.5, M3, M4, M5, M6"),
            "{error}"
        );
        assert!(insert("M7").unwrap_err().contains("no clearance hole"));
        assert!(printed_sources()[1].contains("ruthex"));
    }
}
