// SPDX-License-Identifier: MIT
//! Screw thread sizes from `core/model/data/threads.json`: designations
//! such as `M10x1.5`, `M10` (the coarse pitch), `1/4-20 UNC`, `1/4-20 BSW`,
//! `G 1/4` and `1/4-18 NPT`, with the basic dimensions of the standard's
//! profile, in millimetres: the 60-degree profile of ISO 68-1 (ISO metric
//! and Unified), the 55-degree Whitworth profile (BSW, BSF and the parallel
//! pipe threads G of ISO 228-1) and the 60-degree taper pipe profile (NPT,
//! its size at the pipe's outside diameter: the 1:16 taper is not kept),
//! and the tyre valve threads of ISO 4570 (`5V1`, `8V1`; the 60-degree
//! profile).

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// The thread standards (the thread types `ISO Metric profile`, `ANSI
/// Unified Screw Threads`, British Standard Whitworth and NPT).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStandard {
    #[default]
    IsoMetric,
    Unified,
    /// British Standard Whitworth (BSW), Fine (BSF) and Pipe (G, parallel):
    /// the 55-degree profile (mitcad#4).
    Whitworth,
    /// American National Standard taper pipe threads (mitcad#4).
    Npt,
    /// Tyre valve threads (ISO 4570): `5V1`, `8V1`, … (mitcad#59).
    TyreValve,
}

/// Basic dimensions of a thread size.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadData {
    pub standard: ThreadStandard,
    /// The canonical designation: `M10x1.5`, `1/4-20 UNC`.
    pub designation: String,
    /// The basic major diameter D (NPT: the pipe's outside diameter).
    pub major: f64,
    pub pitch: f64,
    /// The basic minor diameter D1 = D - 2 h: the crests of an internal
    /// thread, the bore of a tapped hole.
    pub minor: f64,
    /// The basic pitch diameter D2 = D - h.
    pub pitch_diameter: f64,
    /// The basic profile's radial depth h: 5H/8 = 0.541266 P of the
    /// 60-degree profile (ISO 68-1), 0.640327 P of the Whitworth profile,
    /// 0.8 P of the taper pipe profile.
    pub depth: f64,
}

impl ThreadData {
    fn new(standard: ThreadStandard, designation: String, major: f64, pitch: f64) -> Self {
        // ISO 68-1: D1 = D - 5H/4, D2 = D - 3H/4, the depth 5H/8; the
        // other profiles are as deep as the pitch diameter is below D.
        let (below_minor, below_pitch, depth) = match standard {
            ThreadStandard::IsoMetric | ThreadStandard::Unified | ThreadStandard::TyreValve => {
                let h = 3f64.sqrt() / 2.0 * pitch;
                (1.25 * h, 0.75 * h, 0.625 * h)
            }
            ThreadStandard::Whitworth => {
                let h = 0.640_327 * pitch;
                (2.0 * h, h, h)
            }
            ThreadStandard::Npt => {
                let h = 0.8 * pitch;
                (2.0 * h, h, h)
            }
        };
        Self {
            standard,
            designation,
            major,
            pitch,
            minor: major - below_minor,
            pitch_diameter: major - below_pitch,
            depth,
        }
    }
}

#[derive(Deserialize)]
struct Table {
    iso_metric: IsoTable,
    unified: UnifiedTable,
    whitworth: WhitworthTable,
    npt: NptTable,
    tyre_valve: TyreValveTable,
}

#[derive(Deserialize)]
struct TyreValveTable {
    classes_external: Vec<String>,
    classes_internal: Vec<String>,
    sizes: Vec<TyreValveSize>,
}

#[derive(Deserialize)]
struct TyreValveSize {
    /// The designation, `5V1`.
    size: String,
    d: f64,
    pitch: f64,
}

#[derive(Deserialize)]
struct WhitworthTable {
    classes_external: Vec<String>,
    classes_internal: Vec<String>,
    sizes: Vec<WhitworthSize>,
    /// The parallel pipe threads (G).
    pipe: Vec<PipeSize>,
}

#[derive(Deserialize)]
struct WhitworthSize {
    size: String,
    d_in: f64,
    #[serde(rename = "BSW")]
    bsw: Option<f64>,
    #[serde(rename = "BSF")]
    bsf: Option<f64>,
}

#[derive(Deserialize)]
struct PipeSize {
    size: String,
    /// The major diameter, millimetres.
    d: f64,
    tpi: f64,
}

#[derive(Deserialize)]
struct NptTable {
    classes_external: Vec<String>,
    classes_internal: Vec<String>,
    sizes: Vec<NptSize>,
}

#[derive(Deserialize)]
struct NptSize {
    size: String,
    /// The pipe's outside diameter, inches.
    d_in: f64,
    tpi: f64,
}

#[derive(Deserialize)]
struct IsoTable {
    classes_external: Vec<String>,
    classes_internal: Vec<String>,
    sizes: Vec<IsoSize>,
}

#[derive(Deserialize)]
struct IsoSize {
    d: f64,
    /// The coarse pitch first.
    pitches: Vec<f64>,
}

#[derive(Deserialize)]
struct UnifiedTable {
    classes_external: Vec<String>,
    classes_internal: Vec<String>,
    sizes: Vec<UnifiedSize>,
}

#[derive(Deserialize)]
struct UnifiedSize {
    size: String,
    d_in: f64,
    #[serde(rename = "UNC")]
    unc: Option<f64>,
    #[serde(rename = "UNF")]
    unf: Option<f64>,
    #[serde(rename = "UNEF")]
    unef: Option<f64>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/threads.json"))
            .expect("the thread table is valid JSON")
    })
}

const INCH: f64 = 25.4;

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(1.0)
}

/// The size of a designation, or why there is none.
pub fn lookup(standard: ThreadStandard, designation: &str) -> Result<ThreadData, String> {
    let unknown = || format!("no {} thread {designation}", standard_name(standard));
    match standard {
        ThreadStandard::IsoMetric => {
            let text = designation.trim();
            let rest = text
                .strip_prefix('M')
                .or_else(|| text.strip_prefix('m'))
                .ok_or_else(unknown)?;
            let (size, pitch) = match rest.split_once(['x', 'X', '×']) {
                Some((size, pitch)) => (size.trim(), Some(pitch.trim())),
                None => (rest.trim(), None),
            };
            let d: f64 = size.parse().map_err(|_| unknown())?;
            let entry = table()
                .iso_metric
                .sizes
                .iter()
                .find(|s| same(s.d, d))
                .ok_or_else(unknown)?;
            let p = match pitch {
                None => entry.pitches[0],
                Some(pitch) => {
                    let p: f64 = pitch.parse().map_err(|_| unknown())?;
                    *entry
                        .pitches
                        .iter()
                        .find(|q| same(**q, p))
                        .ok_or_else(unknown)?
                }
            };
            Ok(ThreadData::new(standard, format!("M{d}x{p}"), d, p))
        }
        ThreadStandard::Unified => {
            let text = designation.trim();
            let (thread, series) = text.rsplit_once(' ').ok_or_else(unknown)?;
            let (size, tpi) = thread.rsplit_once('-').ok_or_else(unknown)?;
            let tpi: f64 = tpi.trim().parse().map_err(|_| unknown())?;
            let entry = table()
                .unified
                .sizes
                .iter()
                .find(|s| s.size == size.trim())
                .ok_or_else(unknown)?;
            let listed = match series.trim() {
                "UNC" => entry.unc,
                "UNF" => entry.unf,
                "UNEF" => entry.unef,
                _ => None,
            };
            if !listed.is_some_and(|t| same(t, tpi)) {
                return Err(unknown());
            }
            Ok(ThreadData::new(
                standard,
                format!("{}-{tpi} {}", entry.size, series.trim()),
                entry.d_in * INCH,
                INCH / tpi,
            ))
        }
        ThreadStandard::Whitworth => {
            let text = designation.trim();
            // A pipe thread: `G 1/4`.
            if let Some(size) = text.strip_prefix('G') {
                let entry = table()
                    .whitworth
                    .pipe
                    .iter()
                    .find(|s| s.size == size.trim())
                    .ok_or_else(unknown)?;
                return Ok(ThreadData::new(
                    standard,
                    format!("G {}", entry.size),
                    entry.d,
                    INCH / entry.tpi,
                ));
            }
            let (thread, series) = text.rsplit_once(' ').ok_or_else(unknown)?;
            let (size, tpi) = thread.rsplit_once('-').ok_or_else(unknown)?;
            let tpi: f64 = tpi.trim().parse().map_err(|_| unknown())?;
            let entry = table()
                .whitworth
                .sizes
                .iter()
                .find(|s| s.size == size.trim())
                .ok_or_else(unknown)?;
            let listed = match series.trim() {
                "BSW" => entry.bsw,
                "BSF" => entry.bsf,
                _ => None,
            };
            if !listed.is_some_and(|t| same(t, tpi)) {
                return Err(unknown());
            }
            Ok(ThreadData::new(
                standard,
                format!("{}-{tpi} {}", entry.size, series.trim()),
                entry.d_in * INCH,
                INCH / tpi,
            ))
        }
        ThreadStandard::Npt => {
            let text = designation.trim();
            let thread = text.strip_suffix("NPT").ok_or_else(unknown)?;
            let (size, tpi) = thread.trim().rsplit_once('-').ok_or_else(unknown)?;
            let tpi: f64 = tpi.trim().parse().map_err(|_| unknown())?;
            let entry = table()
                .npt
                .sizes
                .iter()
                .find(|s| s.size == size.trim() && same(s.tpi, tpi))
                .ok_or_else(unknown)?;
            Ok(ThreadData::new(
                standard,
                format!("{}-{} NPT", entry.size, entry.tpi),
                entry.d_in * INCH,
                INCH / entry.tpi,
            ))
        }
        ThreadStandard::TyreValve => {
            let text = designation.trim();
            let entry = table()
                .tyre_valve
                .sizes
                .iter()
                .find(|s| s.size.eq_ignore_ascii_case(text))
                .ok_or_else(unknown)?;
            Ok(ThreadData::new(
                standard,
                entry.size.clone(),
                entry.d,
                entry.pitch,
            ))
        }
    }
}

/// Checks a tolerance class: lower case for external threads, upper case
/// for internal ones (`6g`, `6H`; `2A`, `2B`; Whitworth's by name). `internal`
/// None accepts either.
pub fn check_class(
    standard: ThreadStandard,
    class: &str,
    internal: Option<bool>,
) -> Result<(), String> {
    let (external_classes, internal_classes) = classes(standard);
    let external = external_classes.iter().any(|c| c == class);
    let inside = internal_classes.iter().any(|c| c == class);
    match internal {
        _ if !external && !inside => Err(format!(
            "unknown {} thread class {class}",
            standard_name(standard)
        )),
        Some(true) if !inside => Err(format!("class {class} is for external threads")),
        Some(false) if !external => Err(format!("class {class} is for internal threads")),
        _ => Ok(()),
    }
}

fn standard_name(standard: ThreadStandard) -> &'static str {
    match standard {
        ThreadStandard::IsoMetric => "ISO metric",
        ThreadStandard::Unified => "Unified",
        ThreadStandard::Whitworth => "Whitworth",
        ThreadStandard::Npt => "NPT",
        ThreadStandard::TyreValve => "tyre valve",
    }
}

impl ThreadStandard {
    /// The standards in the order a size table lists them: ISO metric
    /// first, the default (metric defaults, `docs/architecture.md`; inch
    /// sizes only by choice).
    pub const ALL: [Self; 5] = [
        Self::IsoMetric,
        Self::Unified,
        Self::Whitworth,
        Self::Npt,
        Self::TyreValve,
    ];

    /// The name of the thread type (as .f3d designs store it for ISO metric
    /// and Unified threads).
    pub fn title(self) -> &'static str {
        match self {
            Self::IsoMetric => "ISO Metric profile",
            Self::Unified => "ANSI Unified Screw Threads",
            Self::Whitworth => "British Standard Whitworth",
            Self::Npt => "ANSI Taper Pipe Threads (NPT)",
            Self::TyreValve => "ISO Tyre Valve Threads",
        }
    }

    /// The tolerance class a new thread gets: 6H/6g, 2B/2A, Medium.
    pub fn default_class(self, internal: bool) -> &'static str {
        match (self, internal) {
            (Self::IsoMetric, true) => "6H",
            (Self::IsoMetric, false) => "6g",
            (Self::Unified, true) => "2B",
            (Self::Unified, false) => "2A",
            (Self::Whitworth, _) => "Medium",
            (Self::Npt | Self::TyreValve, _) => "Standard",
        }
    }

    /// Whether a modelled thread can be cut: the geometry cuts the straight
    /// 60-degree basic profile, which the Whitworth (55 degrees) and taper
    /// pipe threads are not.
    pub fn check_modeled(self) -> Result<(), String> {
        match self {
            Self::IsoMetric | Self::Unified | Self::TyreValve => Ok(()),
            Self::Whitworth => Err(
                "a Whitworth thread cannot be modelled: Mitcad cuts the 60-degree profile; make \
                 it cosmetic"
                    .to_owned(),
            ),
            Self::Npt => Err(
                "an NPT thread cannot be modelled: Mitcad cuts straight threads, not tapered \
                 ones; make it cosmetic"
                    .to_owned(),
            ),
        }
    }
}

/// A nominal size of a standard with its designations (P9: the size table
/// of the Hole and Thread panels).
#[derive(Debug, Clone, PartialEq)]
pub struct SizeEntry {
    /// The nominal size as the table names it: `10`, `1/4`, `#10`.
    pub size: String,
    /// The major diameter in millimetres.
    pub major: f64,
    /// The designations of the size, the coarse pitch (UNC) first; each is
    /// accepted by [`lookup`].
    pub designations: Vec<String>,
}

/// The sizes of a standard, smallest first.
pub fn sizes(standard: ThreadStandard) -> Vec<SizeEntry> {
    match standard {
        ThreadStandard::IsoMetric => table()
            .iso_metric
            .sizes
            .iter()
            .map(|s| SizeEntry {
                size: format!("{}", s.d),
                major: s.d,
                designations: s.pitches.iter().map(|p| format!("M{}x{p}", s.d)).collect(),
            })
            .collect(),
        ThreadStandard::Unified => table()
            .unified
            .sizes
            .iter()
            .map(|s| {
                let series = [("UNC", s.unc), ("UNF", s.unf), ("UNEF", s.unef)];
                SizeEntry {
                    size: s.size.clone(),
                    major: s.d_in * INCH,
                    designations: series
                        .iter()
                        .filter_map(|(name, tpi)| tpi.map(|t| format!("{}-{t} {name}", s.size)))
                        .collect(),
                }
            })
            .collect(),
        // The bolt sizes (BSW first), then the pipe sizes (`G 1/4`).
        ThreadStandard::Whitworth => {
            let whitworth = &table().whitworth;
            let bolts = whitworth.sizes.iter().map(|s| {
                let series = [("BSW", s.bsw), ("BSF", s.bsf)];
                SizeEntry {
                    size: s.size.clone(),
                    major: s.d_in * INCH,
                    designations: series
                        .iter()
                        .filter_map(|(name, tpi)| tpi.map(|t| format!("{}-{t} {name}", s.size)))
                        .collect(),
                }
            });
            let pipes = whitworth.pipe.iter().map(|s| SizeEntry {
                size: format!("G {}", s.size),
                major: s.d,
                designations: vec![format!("G {}", s.size)],
            });
            bolts.chain(pipes).collect()
        }
        ThreadStandard::Npt => table()
            .npt
            .sizes
            .iter()
            .map(|s| SizeEntry {
                size: s.size.clone(),
                major: s.d_in * INCH,
                designations: vec![format!("{}-{} NPT", s.size, s.tpi)],
            })
            .collect(),
        ThreadStandard::TyreValve => {
            let mut sizes: Vec<SizeEntry> = table()
                .tyre_valve
                .sizes
                .iter()
                .map(|s| SizeEntry {
                    size: s.size.clone(),
                    major: s.d,
                    designations: vec![s.size.clone()],
                })
                .collect();
            sizes.sort_by(|a, b| a.major.total_cmp(&b.major));
            sizes
        }
    }
}

/// The tolerance classes of a standard: (external, internal).
pub fn classes(standard: ThreadStandard) -> (&'static [String], &'static [String]) {
    let t = table();
    match standard {
        ThreadStandard::IsoMetric => (
            &t.iso_metric.classes_external,
            &t.iso_metric.classes_internal,
        ),
        ThreadStandard::Unified => (&t.unified.classes_external, &t.unified.classes_internal),
        ThreadStandard::Whitworth => (&t.whitworth.classes_external, &t.whitworth.classes_internal),
        ThreadStandard::Npt => (&t.npt.classes_external, &t.npt.classes_internal),
        ThreadStandard::TyreValve => (
            &t.tyre_valve.classes_external,
            &t.tyre_valve.classes_internal,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn iso_metric_sizes_follow_the_basic_profile() {
        let m6 = lookup(ThreadStandard::IsoMetric, "M6x1").unwrap();
        assert_eq!(m6.designation, "M6x1");
        assert!(close(m6.major, 6.0) && close(m6.pitch, 1.0));
        assert!(close(m6.minor, 4.917468), "{}", m6.minor);
        assert!(close(m6.pitch_diameter, 5.350481), "{}", m6.pitch_diameter);
        assert!(close(m6.depth, 0.541266), "{}", m6.depth);
        // Without a pitch: the coarse one.
        let m10 = lookup(ThreadStandard::IsoMetric, "M10").unwrap();
        assert_eq!(m10.designation, "M10x1.5");
        assert!(close(m10.minor, 8.376202), "{}", m10.minor);
        assert_eq!(
            lookup(ThreadStandard::IsoMetric, " m10 X 1.25 ")
                .unwrap()
                .designation,
            "M10x1.25"
        );
        for bad in ["M10x2", "M13", "10x1.5", "Mx1", ""] {
            let error = lookup(ThreadStandard::IsoMetric, bad).unwrap_err();
            assert!(error.contains("no ISO metric thread"), "{error}");
        }
    }

    #[test]
    fn unified_sizes_are_in_inches() {
        let quarter = lookup(ThreadStandard::Unified, "1/4-20 UNC").unwrap();
        assert!(close(quarter.major, 6.35) && close(quarter.pitch, 1.27));
        assert_eq!(quarter.designation, "1/4-20 UNC");
        let ten = lookup(ThreadStandard::Unified, "#10-32 UNF").unwrap();
        assert!(close(ten.major, 4.826));
        assert!(lookup(ThreadStandard::Unified, "1 1/2-6 UNC").is_ok());
        assert!(lookup(ThreadStandard::Unified, "1/4-24 UNC").is_err());
        assert!(lookup(ThreadStandard::Unified, "1/4-20").is_err());
    }

    #[test]
    fn every_listed_designation_is_found() {
        for standard in ThreadStandard::ALL {
            let sizes = sizes(standard);
            assert!(sizes.len() > 15, "{standard:?}");
            for size in &sizes {
                assert!(!size.designations.is_empty(), "{}", size.size);
                for designation in &size.designations {
                    let data = lookup(standard, designation).unwrap();
                    assert_eq!(&data.designation, designation);
                    assert!(close(data.major, size.major), "{designation}");
                }
            }
            let (external, internal) = classes(standard);
            assert!(external.iter().any(|c| c == standard.default_class(false)));
            assert!(internal.iter().any(|c| c == standard.default_class(true)));
        }
        let iso = sizes(ThreadStandard::IsoMetric);
        let m10 = iso.iter().find(|s| s.size == "10").unwrap();
        assert_eq!(m10.designations[0], "M10x1.5");
        let unified = sizes(ThreadStandard::Unified);
        let quarter = unified.iter().find(|s| s.size == "1/4").unwrap();
        assert_eq!(quarter.designations[0], "1/4-20 UNC");
    }

    #[test]
    fn classes_tell_internal_from_external() {
        assert!(check_class(ThreadStandard::IsoMetric, "6g", Some(false)).is_ok());
        assert!(check_class(ThreadStandard::IsoMetric, "6H", Some(true)).is_ok());
        assert!(check_class(ThreadStandard::IsoMetric, "6H", None).is_ok());
        let error = check_class(ThreadStandard::IsoMetric, "6g", Some(true)).unwrap_err();
        assert!(error.contains("external"), "{error}");
        assert!(check_class(ThreadStandard::IsoMetric, "9z", None).is_err());
        assert!(check_class(ThreadStandard::Unified, "2B", Some(true)).is_ok());
        assert!(check_class(ThreadStandard::Whitworth, "Normal", Some(true)).is_ok());
        assert!(check_class(ThreadStandard::Whitworth, "Close", Some(true)).is_err());
    }

    #[test]
    fn whitworth_and_npt_sizes_follow_their_profiles() {
        let quarter = lookup(ThreadStandard::Whitworth, "1/4-20 BSW").unwrap();
        assert!(close(quarter.major, 6.35) && close(quarter.pitch, 1.27));
        // The 55-degree profile: h = 0.640327 P.
        assert!(close(quarter.depth, 0.813215), "{}", quarter.depth);
        assert!(
            close(quarter.minor, 6.35 - 2.0 * 0.813215),
            "{}",
            quarter.minor
        );
        assert_eq!(
            lookup(ThreadStandard::Whitworth, "5/16-22 BSF")
                .unwrap()
                .designation,
            "5/16-22 BSF"
        );
        assert!(lookup(ThreadStandard::Whitworth, "1/4-26 BSW").is_err());
        assert!(lookup(ThreadStandard::Whitworth, "1/8-28 BSF").is_err());
        // Pipe threads by their own major diameters.
        let pipe = lookup(ThreadStandard::Whitworth, "G1/4").unwrap();
        assert_eq!(pipe.designation, "G 1/4");
        assert!(close(pipe.major, 13.157) && close(pipe.pitch, 25.4 / 19.0));
        // NPT at the pipe's outside diameter, h = 0.8 P.
        let npt = lookup(ThreadStandard::Npt, "1 1/4-11.5 NPT").unwrap();
        assert!(close(npt.major, 1.66 * 25.4) && close(npt.depth, 0.8 * 25.4 / 11.5));
        assert!(lookup(ThreadStandard::Npt, "1/4-20 NPT").is_err());
        // Tyre valve threads (ISO 4570) on the 60-degree profile.
        let valve = lookup(ThreadStandard::TyreValve, "8v1").unwrap();
        assert_eq!(valve.designation, "8V1");
        assert!(close(valve.major, 7.798) && close(valve.pitch, 0.794));
        assert!(close(valve.depth, 0.625 * 3f64.sqrt() / 2.0 * 0.794));
        assert!(lookup(ThreadStandard::TyreValve, "8V9").is_err());
        assert!(ThreadStandard::TyreValve.check_modeled().is_ok());
        // Neither is modelled (the geometry cuts straight 60-degree threads).
        assert!(ThreadStandard::Unified.check_modeled().is_ok());
        assert!(ThreadStandard::Whitworth.check_modeled().is_err());
        assert!(ThreadStandard::Npt.check_modeled().is_err());
    }
}
