// SPDX-License-Identifier: MIT
//! Screw thread sizes from `core/model/data/threads.json`: designations
//! such as `M10x1.5`, `M10` (the coarse pitch), `1/4-20 UNC`, `1/4-20 BSW`,
//! `G 1/4` and `1/4-18 NPT`, with the basic dimensions of the standard's
//! profile, in millimetres: the 60-degree profile of ISO 68-1 (ISO metric
//! and Unified), the 55-degree Whitworth profile (BSW, BSF and the parallel
//! pipe threads G of ISO 228-1), the taper pipe threads tapered 1:16 on
//! the diameter (mitcad#733): NPT's 60-degree profile (ASME B1.20.1, its
//! size at the pipe's outside diameter) and the 55-degree one of ISO 7-1
//! (`R 1/2` external, `Rc 1/2` internal), the tyre valve threads of ISO
//! 4570 (`5V1`, `8V1`; the 60-degree profile) and the metric trapezoidal
//! threads of ISO 2904 (`Tr8x1.5`, a multi-start one by its lead and pitch
//! `Tr10x4(P2)`; the 30-degree profile of ISO 2901). A custom thread
//! (mitcad#893) is not in the table: its major diameter and pitch are given
//! ([`ThreadData::custom`]), on the 60-degree profile.

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
    /// American National Standard taper pipe threads (mitcad#4), tapered
    /// 1:16 on the diameter (mitcad#733).
    Npt,
    /// Tyre valve threads (ISO 4570): `5V1`, `8V1`, … (mitcad#59).
    TyreValve,
    /// Metric forming (tapping) screw threads (ISO 1478): `ST3.5x1.3`. The
    /// 60-degree profile; a modelled one takes its diameters from the
    /// design that stores them.
    FormingScrew,
    /// Metric trapezoidal threads (ISO 2904): `Tr8x1.5`, multi-start
    /// `Tr10x4(P2)` (lead 4, pitch 2), the lead screws of linear drives
    /// (mitcad#449). The 30-degree profile, modelled with the crest
    /// clearance of ISO 2901 (mitcad#893).
    Trapezoidal,
    /// ISO 7-1 taper pipe threads (BSPT): `R 1/2` external, `Rc 1/2`
    /// internal, the 55-degree profile tapered 1:16 on the diameter
    /// (mitcad#733); cosmetic only.
    IsoTaper,
    /// A thread of a major diameter and pitch of one's own (mitcad#893):
    /// not in the table, the 60-degree profile of ISO 68-1 at that size
    /// (3D-printed parts often want a longer pitch than any standard has).
    /// The feature gives the size (`custom`); no designation, no classes.
    Custom,
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
    /// A taper pipe thread's taper (mitcad#733); None for a straight one.
    pub taper: Option<Taper>,
    /// The number of starts its designation gives (a multi-start
    /// trapezoidal thread's lead over its pitch, `Tr10x4(P2)`: 2); 1
    /// otherwise (mitcad#893).
    pub starts: u32,
}

/// The taper of a taper pipe thread (NPT, ISO 7-1; mitcad#733): its
/// diameters change by `per_length` per length along the axis (1/16), and
/// where a thread starts (an internal thread at its opening, an external
/// one at its free, small end) its pitch diameter is `internal_start` or
/// `external_start`, its major and minor diameters `depth` above and below
/// that.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Taper {
    pub per_length: f64,
    /// NPT's E1 = E0 + L1/16 at the hand-tight plane; ISO 7-1's gauge
    /// diameter less h at the gauge plane, at the opening of an `Rc`
    /// thread.
    pub internal_start: f64,
    /// NPT's E0; ISO 7-1's gauge diameter less h, less the gauge length's
    /// taper.
    pub external_start: f64,
    /// The internal thread's length a tap drill serves: NPT's effective
    /// thread length L2 = (0.8 D + 6.8) P, ISO 7-1's gauge length.
    pub length: f64,
}

/// The taper of every taper pipe thread: 1:16 on the diameter.
pub const PIPE_TAPER: f64 = 1.0 / 16.0;

impl ThreadData {
    /// A taper pipe thread's diameters where it starts (major, minor,
    /// pitch; see [`Taper`]); None for a straight thread.
    pub fn taper_start(&self, internal: bool) -> Option<(f64, f64, f64)> {
        let taper = self.taper?;
        let pitch = if internal {
            taper.internal_start
        } else {
            taper.external_start
        };
        Some((pitch + self.depth, pitch - self.depth, pitch))
    }

    /// A taper pipe thread's tap drill: the internal thread's minor
    /// diameter `length` into it (NPT: 0.6879 in for 1/2-14 NPT); None for
    /// a straight thread.
    pub fn tap_drill(&self) -> Option<f64> {
        let taper = self.taper?;
        let (_, minor, _) = self.taper_start(true)?;
        Some(minor - taper.length * taper.per_length)
    }

    /// The diameters (major, minor, pitch) of a modelled thread's profile
    /// of this size on an internal or an external face when none are
    /// given: the basic ones, but for a trapezoidal thread ISO 2901's
    /// design profile with its crest clearance `ac` (mitcad#893): an
    /// external thread's root d3 = d - 2 (H1 + ac), an internal thread's
    /// root D4 = d + 2 ac, the crests and the pitch diameter the basic
    /// ones, so that a nut and a screw of a size clear each other at the
    /// roots.
    pub fn profile(&self, internal: bool) -> (f64, f64, f64) {
        if self.standard != ThreadStandard::Trapezoidal {
            return (self.major, self.minor, self.pitch_diameter);
        }
        let ac = trapezoidal_crest_clearance(self.pitch);
        if internal {
            (self.major + 2.0 * ac, self.minor, self.pitch_diameter)
        } else {
            (self.major, self.minor - 2.0 * ac, self.pitch_diameter)
        }
    }

    /// The flanks' angle to the radial direction, radians: half the
    /// profile's included angle (30 degrees for the 60-degree profile, 15
    /// for the trapezoidal one, 27.5 for the 55-degree Whitworth one).
    pub fn flank_angle(&self) -> f64 {
        self.standard.flank_angle()
    }

    /// A custom thread's size (mitcad#893): a major diameter and a pitch of
    /// one's own on the 60-degree profile of ISO 68-1 (D1 = D - 5H/4, D2 =
    /// D - 3H/4), designated `<major>x<pitch>` (`12x3`). The pitch must
    /// leave the profile a minor diameter (P < 0.9238 D).
    pub fn custom(major: f64, pitch: f64) -> Result<Self, String> {
        if !(major.is_finite() && major > 0.0) {
            return Err(format!(
                "a custom thread's major diameter must be greater than zero, got {major}"
            ));
        }
        if !(pitch.is_finite() && pitch > 0.0) {
            return Err(format!(
                "a custom thread's pitch must be greater than zero, got {pitch}"
            ));
        }
        let designation = format!("{}x{}", short(major), short(pitch));
        let data = Self::new(ThreadStandard::Custom, designation, major, pitch);
        if data.minor <= 0.0 {
            return Err(format!(
                "a custom thread's pitch of {pitch} mm is too coarse for its diameter of {major} \
                 mm: the 60-degree profile leaves no core (the pitch must be under {:.4} mm)",
                major / (1.25 * 3f64.sqrt() / 2.0)
            ));
        }
        Ok(data)
    }
}

/// A length as a designation writes it: up to four decimals, no trailing
/// zeros (`12`, `2.5`, `1.3333`).
fn short(value: f64) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// ISO 2901's crest clearance `ac` of a trapezoidal thread's pitch: 0.15
/// mm for P 1.5, 0.25 mm for P 2 to 5, 0.5 mm for P 6 to 12, 1 mm for P 14
/// to 44 (mitcad#893).
pub fn trapezoidal_crest_clearance(pitch: f64) -> f64 {
    if pitch < 1.75 {
        0.15
    } else if pitch < 5.5 {
        0.25
    } else if pitch < 13.0 {
        0.5
    } else {
        1.0
    }
}

impl ThreadData {
    fn new(standard: ThreadStandard, designation: String, major: f64, pitch: f64) -> Self {
        // ISO 68-1: D1 = D - 5H/4, D2 = D - 3H/4, the depth 5H/8; the
        // other profiles are as deep as the pitch diameter is below D.
        let (below_minor, below_pitch, depth) = match standard {
            ThreadStandard::IsoMetric
            | ThreadStandard::Unified
            | ThreadStandard::TyreValve
            | ThreadStandard::FormingScrew
            | ThreadStandard::Custom => {
                let h = 3f64.sqrt() / 2.0 * pitch;
                (1.25 * h, 0.75 * h, 0.625 * h)
            }
            ThreadStandard::Whitworth | ThreadStandard::IsoTaper => {
                let h = 0.640_327 * pitch;
                (2.0 * h, h, h)
            }
            ThreadStandard::Npt => {
                let h = 0.8 * pitch;
                (2.0 * h, h, h)
            }
            // ISO 2901's basic profile: H1 = P/2, D1 = d - P, D2 = d - P/2.
            ThreadStandard::Trapezoidal => (pitch, 0.5 * pitch, 0.5 * pitch),
        };
        Self {
            standard,
            designation,
            major,
            pitch,
            minor: major - below_minor,
            pitch_diameter: major - below_pitch,
            depth,
            taper: None,
            starts: 1,
        }
    }
}

#[derive(Deserialize)]
struct Table {
    iso_metric: IsoTable,
    unified: UnifiedTable,
    whitworth: WhitworthTable,
    npt: NptTable,
    iso_taper: IsoTaperTable,
    tyre_valve: TyreValveTable,
    forming_screw: IsoTable,
    trapezoidal: IsoTable,
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
    /// The hand-tight engagement L1, inches.
    l1_in: f64,
}

#[derive(Deserialize)]
struct IsoTaperTable {
    classes_external: Vec<String>,
    classes_internal: Vec<String>,
    sizes: Vec<IsoTaperSize>,
}

#[derive(Deserialize)]
struct IsoTaperSize {
    size: String,
    /// The gauge diameter: the major diameter at the gauge plane, mm.
    d: f64,
    tpi: f64,
    /// The gauge length, mm.
    gauge_length: f64,
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

/// A trapezoidal designation: `Tr8x1.5`, `Tr 8 x 1.5`, `Tr8` (the
/// preferred pitch), multi-start `Tr10x4(P2)` or `Tr10x4P2` (lead 4,
/// pitch 2): the diameter, the lead and the pitch.
fn trapezoidal_parts(text: &str) -> Option<(f64, Option<f64>, Option<f64>)> {
    let text = text.trim();
    let rest = match text.get(..2) {
        Some(prefix) if prefix.eq_ignore_ascii_case("Tr") => text[2..].trim(),
        _ => return None,
    };
    let Some((size, pitch)) = rest.split_once(['x', 'X', '×']) else {
        return Some((rest.parse().ok()?, None, None));
    };
    let d: f64 = size.trim().parse().ok()?;
    match pitch.split_once(['P', 'p']) {
        Some((lead, pitch)) => {
            let lead = lead.trim().trim_end_matches('(').trim();
            let pitch = pitch.trim().trim_end_matches(')').trim();
            Some((d, Some(lead.parse().ok()?), Some(pitch.parse().ok()?)))
        }
        None => Some((d, Some(pitch.trim().parse().ok()?), None)),
    }
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
        ThreadStandard::FormingScrew => {
            // `ST3.5x1.3`; designs also store it without the `ST`
            // (`2.9x1.4`) and a size alone (`ST3.5`) takes its first pitch.
            let text = designation.trim();
            let rest = match text.get(..2) {
                Some(prefix) if prefix.eq_ignore_ascii_case("ST") => &text[2..],
                _ => text,
            };
            let (size, pitch) = match rest.split_once(['x', 'X', '×']) {
                Some((size, pitch)) => (size.trim(), Some(pitch.trim())),
                None => (rest.trim(), None),
            };
            let d: f64 = size.parse().map_err(|_| unknown())?;
            let entry = table()
                .forming_screw
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
            Ok(ThreadData::new(standard, format!("ST{d}x{p}"), d, p))
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
            let mut data = ThreadData::new(
                standard,
                format!("{}-{} NPT", entry.size, entry.tpi),
                entry.d_in * INCH,
                INCH / entry.tpi,
            );
            // ASME B1.20.1: E0 = D - (0.05 D + 1.1) P, E1 = E0 + L1/16.
            let e0 = (entry.d_in - (0.05 * entry.d_in + 1.1) / entry.tpi) * INCH;
            data.taper = Some(Taper {
                per_length: PIPE_TAPER,
                internal_start: e0 + entry.l1_in * INCH * PIPE_TAPER,
                external_start: e0,
                length: (0.8 * entry.d_in + 6.8) / entry.tpi * INCH,
            });
            Ok(data)
        }
        ThreadStandard::IsoTaper => {
            // `R 1/2` (external) or `Rc 1/2` (internal).
            let text = designation.trim();
            let (prefix, size) = if let Some(size) = text.strip_prefix("Rc") {
                ("Rc", size)
            } else if let Some(size) = text.strip_prefix('R') {
                ("R", size)
            } else {
                return Err(unknown());
            };
            let entry = table()
                .iso_taper
                .sizes
                .iter()
                .find(|s| s.size == size.trim())
                .ok_or_else(unknown)?;
            let mut data = ThreadData::new(
                standard,
                format!("{prefix} {}", entry.size),
                entry.d,
                INCH / entry.tpi,
            );
            // The gauge plane: at the opening of an internal thread, the
            // gauge length from the small end of an external one.
            let gauge = data.pitch_diameter;
            data.taper = Some(Taper {
                per_length: PIPE_TAPER,
                internal_start: gauge,
                external_start: gauge - entry.gauge_length * PIPE_TAPER,
                length: entry.gauge_length,
            });
            Ok(data)
        }
        ThreadStandard::Trapezoidal => {
            let (d, lead, pitch) = trapezoidal_parts(designation).ok_or_else(unknown)?;
            let entry = table()
                .trapezoidal
                .sizes
                .iter()
                .find(|s| same(s.d, d))
                .ok_or_else(unknown)?;
            // A single-start thread's lead is its pitch; a multi-start
            // one's a whole number (2 or more) of its pitches.
            let p = pitch.or(lead).unwrap_or(entry.pitches[0]);
            let p = *entry
                .pitches
                .iter()
                .find(|q| same(**q, p))
                .ok_or_else(unknown)?;
            let lead = lead.unwrap_or(p);
            let starts = lead / p;
            if !(starts >= 1.0 - 1e-9 && same(starts, starts.round())) {
                return Err(unknown());
            }
            let designation = if same(lead, p) {
                format!("Tr{d}x{p}")
            } else {
                format!("Tr{d}x{lead}(P{p})")
            };
            let mut data = ThreadData::new(standard, designation, d, p);
            // (A whole number, checked above.)
            data.starts = starts.round() as u32;
            Ok(data)
        }
        ThreadStandard::Custom => Err(
            "a custom thread has no designation: its major diameter and pitch are given (`custom`)"
                .to_owned(),
        ),
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
/// None accepts either. A class of the other side names the class to use
/// instead, the standard's default of the face's side (mitcad#691: "class
/// 6g is for external threads; use 6H for a hole").
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
        Some(true) if !inside => Err(format!(
            "class {class} is for external threads; use {} for a hole",
            standard.default_class(true)
        )),
        Some(false) if !external => Err(format!(
            "class {class} is for internal threads; use {} for a shaft",
            standard.default_class(false)
        )),
        _ => Ok(()),
    }
}

fn standard_name(standard: ThreadStandard) -> &'static str {
    match standard {
        ThreadStandard::IsoMetric => "ISO metric",
        ThreadStandard::Unified => "Unified",
        ThreadStandard::Whitworth => "Whitworth",
        ThreadStandard::Npt => "NPT",
        ThreadStandard::TyreValve => "tire valve",
        ThreadStandard::FormingScrew => "forming screw",
        ThreadStandard::Trapezoidal => "trapezoidal",
        ThreadStandard::IsoTaper => "ISO 7-1 taper pipe",
        ThreadStandard::Custom => "custom",
    }
}

impl ThreadStandard {
    /// The standards in the order a size table lists them: ISO metric
    /// first, the default (metric defaults, `docs/architecture.md`; inch
    /// sizes only by choice). [`Self::Custom`] has no table and is not
    /// among them.
    pub const ALL: [Self; 8] = [
        Self::IsoMetric,
        Self::Unified,
        Self::Whitworth,
        Self::Npt,
        Self::TyreValve,
        Self::FormingScrew,
        Self::Trapezoidal,
        Self::IsoTaper,
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
            Self::FormingScrew => "Metric Forming Screw Threads",
            Self::Trapezoidal => "ISO Metric Trapezoidal Threads",
            Self::IsoTaper => "ISO Taper Pipe Threads (R, Rc)",
            Self::Custom => "Custom",
        }
    }

    /// The flanks' angle to the radial direction, radians: half the
    /// profile's included angle, 30 degrees for the 60-degree profile
    /// (ISO metric, Unified, NPT, tyre valve, forming screw and custom
    /// threads), 15 for the trapezoidal one (ISO 2901), 27.5 for the
    /// 55-degree Whitworth one (Whitworth, ISO 7-1).
    pub fn flank_angle(self) -> f64 {
        match self {
            Self::Trapezoidal => 15f64.to_radians(),
            Self::Whitworth | Self::IsoTaper => 27.5f64.to_radians(),
            Self::IsoMetric
            | Self::Unified
            | Self::Npt
            | Self::TyreValve
            | Self::FormingScrew
            | Self::Custom => 30f64.to_radians(),
        }
    }

    /// The tolerance class a new thread gets: 6H/6g, 2B/2A, Medium.
    pub fn default_class(self, internal: bool) -> &'static str {
        match (self, internal) {
            (Self::IsoMetric | Self::FormingScrew, true) => "6H",
            (Self::IsoMetric | Self::FormingScrew, false) => "6g",
            (Self::Unified, true) => "2B",
            (Self::Unified, false) => "2A",
            (Self::Whitworth, _) => "Medium",
            (Self::Npt | Self::TyreValve | Self::IsoTaper, _) => "Standard",
            (Self::Trapezoidal, true) => "7H",
            (Self::Trapezoidal, false) => "7e",
            // (No classes.)
            (Self::Custom, _) => "",
        }
    }

    /// Whether a modelled thread can be cut: the geometry cuts the
    /// 60-degree basic profile, straight or tapered (NPT, mitcad#733), and
    /// the 30-degree trapezoidal one (mitcad#893), which the Whitworth and
    /// ISO 7-1 (55 degrees) threads are not.
    pub fn check_modeled(self) -> Result<(), String> {
        match self {
            Self::IsoMetric
            | Self::Unified
            | Self::TyreValve
            | Self::FormingScrew
            | Self::Npt
            | Self::Trapezoidal
            | Self::Custom => Ok(()),
            Self::Whitworth => Err(
                "a Whitworth thread cannot be modeled: Mitcad cuts the 60-degree and the \
                 trapezoidal profiles; make it cosmetic"
                    .to_owned(),
            ),
            Self::IsoTaper => Err(
                "an ISO 7-1 taper pipe thread cannot be modeled: Mitcad cuts the 60-degree and \
                 the trapezoidal profiles; make it cosmetic"
                    .to_owned(),
            ),
        }
    }

    /// Whether its threads are taper pipe threads (mitcad#733).
    pub fn is_tapered(self) -> bool {
        matches!(self, Self::Npt | Self::IsoTaper)
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
        ThreadStandard::IsoTaper => table()
            .iso_taper
            .sizes
            .iter()
            .map(|s| SizeEntry {
                size: s.size.clone(),
                major: s.d,
                designations: vec![format!("R {}", s.size), format!("Rc {}", s.size)],
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
        ThreadStandard::FormingScrew => table()
            .forming_screw
            .sizes
            .iter()
            .map(|s| SizeEntry {
                size: format!("ST{}", s.d),
                major: s.d,
                designations: s.pitches.iter().map(|p| format!("ST{}x{p}", s.d)).collect(),
            })
            .collect(),
        ThreadStandard::Trapezoidal => table()
            .trapezoidal
            .sizes
            .iter()
            .map(|s| SizeEntry {
                size: format!("Tr{}", s.d),
                major: s.d,
                designations: s.pitches.iter().map(|p| format!("Tr{}x{p}", s.d)).collect(),
            })
            .collect(),
        // (Not in the table.)
        ThreadStandard::Custom => Vec::new(),
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
        ThreadStandard::IsoTaper => (&t.iso_taper.classes_external, &t.iso_taper.classes_internal),
        ThreadStandard::TyreValve => (
            &t.tyre_valve.classes_external,
            &t.tyre_valve.classes_internal,
        ),
        ThreadStandard::FormingScrew => (
            &t.forming_screw.classes_external,
            &t.forming_screw.classes_internal,
        ),
        ThreadStandard::Trapezoidal => (
            &t.trapezoidal.classes_external,
            &t.trapezoidal.classes_internal,
        ),
        // (None.)
        ThreadStandard::Custom => (&[], &[]),
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
        for bad in ["M10x2", "M13", "10x1.5", "Mx1", "M100x5", ""] {
            let error = lookup(ThreadStandard::IsoMetric, bad).unwrap_err();
            assert!(error.contains("no ISO metric thread"), "{error}");
        }
    }

    #[test]
    fn iso_metric_sizes_reach_the_large_fine_threads() {
        // Above M64 only fine pitches exist: the largest comes first.
        let m100 = lookup(ThreadStandard::IsoMetric, "M100x4").unwrap();
        assert_eq!(m100.designation, "M100x4");
        assert!(close(m100.major, 100.0) && close(m100.pitch, 4.0));
        assert_eq!(
            lookup(ThreadStandard::IsoMetric, "M100")
                .unwrap()
                .designation,
            "M100x6"
        );
        for ok in ["M5.5x0.9", "M28x1.5", "M40x3", "M72x2", "M150x6", "M300x3"] {
            assert!(lookup(ThreadStandard::IsoMetric, ok).is_ok(), "{ok}");
        }
        assert!(lookup(ThreadStandard::IsoMetric, "M160x2").is_err());
        let sizes = sizes(ThreadStandard::IsoMetric);
        assert!(sizes.windows(2).all(|w| w[0].major < w[1].major));
    }

    #[test]
    fn forming_screw_threads_take_the_st_sizes() {
        let st = lookup(ThreadStandard::FormingScrew, "ST3.5x1.3").unwrap();
        assert_eq!(st.designation, "ST3.5x1.3");
        assert!(close(st.major, 3.5) && close(st.pitch, 1.3));
        // Designs also store it without the `ST`, or a size alone.
        for same in ["3.5x1.3", "st3.5 X 1.3", "ST3.5"] {
            assert_eq!(
                lookup(ThreadStandard::FormingScrew, same)
                    .unwrap()
                    .designation,
                "ST3.5x1.3",
                "{same}"
            );
        }
        assert_eq!(
            lookup(ThreadStandard::FormingScrew, "2.9x1.4")
                .unwrap()
                .designation,
            "ST2.9x1.4"
        );
        for bad in ["ST3.5x1.4", "ST3", "M3.5x1.3", "ST", ""] {
            let error = lookup(ThreadStandard::FormingScrew, bad).unwrap_err();
            assert!(error.contains("no forming screw thread"), "{error}");
        }
        assert_eq!(
            ThreadStandard::FormingScrew.title(),
            "Metric Forming Screw Threads"
        );
        assert!(check_class(ThreadStandard::FormingScrew, "6g", Some(false)).is_ok());
        assert!(ThreadStandard::FormingScrew.check_modeled().is_ok());
    }

    #[test]
    fn trapezoidal_threads_take_the_iso_2904_sizes() {
        // ISO 2901's basic profile: D1 = d - P, D2 = d - P/2.
        let tr8 = lookup(ThreadStandard::Trapezoidal, "Tr8x1.5").unwrap();
        assert_eq!(tr8.designation, "Tr8x1.5");
        assert!(close(tr8.major, 8.0) && close(tr8.pitch, 1.5));
        assert!(close(tr8.minor, 6.5) && close(tr8.pitch_diameter, 7.25));
        assert!(close(tr8.depth, 0.75));
        for same in ["TR8x1.5", "Tr 8 x 1.5", "tr8X1.5", "Tr8"] {
            assert_eq!(
                lookup(ThreadStandard::Trapezoidal, same)
                    .unwrap()
                    .designation,
                "Tr8x1.5",
                "{same}"
            );
        }
        // A multi-start thread: its lead, then its pitch.
        for same in ["Tr10x4(P2)", "TR10x4P2", "Tr10 x 4 (P 2)"] {
            let tr10 = lookup(ThreadStandard::Trapezoidal, same).unwrap();
            assert_eq!(tr10.designation, "Tr10x4(P2)", "{same}");
            assert!(close(tr10.pitch, 2.0) && close(tr10.minor, 8.0), "{same}");
        }
        assert_eq!(
            lookup(ThreadStandard::Trapezoidal, "Tr12x6(P3)")
                .unwrap()
                .designation,
            "Tr12x6(P3)"
        );
        for ok in ["Tr10x2", "Tr12x3", "Tr16x4", "Tr40x7", "Tr80x16"] {
            assert!(lookup(ThreadStandard::Trapezoidal, ok).is_ok(), "{ok}");
        }
        for bad in ["Tr10x3", "Tr10x3(P2)", "Tr13x3", "M8x1.5", "Tr", ""] {
            let error = lookup(ThreadStandard::Trapezoidal, bad).unwrap_err();
            assert!(error.contains("no trapezoidal thread"), "{bad}: {error}");
        }
        assert_eq!(
            ThreadStandard::Trapezoidal.title(),
            "ISO Metric Trapezoidal Threads"
        );
        assert!(check_class(ThreadStandard::Trapezoidal, "7e", Some(false)).is_ok());
        assert!(check_class(ThreadStandard::Trapezoidal, "7H", Some(true)).is_ok());
        assert!(check_class(ThreadStandard::Trapezoidal, "6g", None).is_err());
        // Modelled with ISO 2901's profile (mitcad#893).
        assert!(ThreadStandard::Trapezoidal.check_modeled().is_ok());
        assert!(close(
            ThreadStandard::Trapezoidal.flank_angle(),
            15f64.to_radians()
        ));
    }

    #[test]
    fn trapezoidal_threads_are_modelled_with_their_crest_clearance() {
        // ISO 2901: ac 0.15 for P 1.5, 0.25 for P 2-5, 0.5 for P 6-12, 1
        // for P 14 and up.
        for (pitch, ac) in [
            (1.5, 0.15),
            (2.0, 0.25),
            (5.0, 0.25),
            (6.0, 0.5),
            (12.0, 0.5),
        ] {
            assert!(close(trapezoidal_crest_clearance(pitch), ac), "{pitch}");
        }
        assert!(close(trapezoidal_crest_clearance(14.0), 1.0));
        // Tr10x2: d3 = 10 - 2 (1 + 0.25) = 7.5 outside, D4 = 10.5 inside;
        // the crests and the pitch diameter the basic ones.
        let tr10 = lookup(ThreadStandard::Trapezoidal, "Tr10x2").unwrap();
        assert_eq!(tr10.starts, 1);
        let (major, minor, pitch) = tr10.profile(false);
        assert!(close(major, 10.0) && close(minor, 7.5) && close(pitch, 9.0));
        let (major, minor, pitch) = tr10.profile(true);
        assert!(close(major, 10.5) && close(minor, 8.0) && close(pitch, 9.0));
        // A multi-start one: its starts from the lead over the pitch.
        let multi = lookup(ThreadStandard::Trapezoidal, "Tr10x4(P2)").unwrap();
        assert_eq!(multi.starts, 2);
        assert_eq!(multi.profile(false), tr10.profile(false));
        assert_eq!(
            lookup(ThreadStandard::Trapezoidal, "Tr12x6(P3)")
                .unwrap()
                .starts,
            2
        );
        // Other threads keep their basic profile.
        let m10 = lookup(ThreadStandard::IsoMetric, "M10").unwrap();
        assert_eq!(m10.starts, 1);
        assert_eq!(
            m10.profile(false),
            (m10.major, m10.minor, m10.pitch_diameter)
        );
        assert_eq!(
            m10.profile(true),
            (m10.major, m10.minor, m10.pitch_diameter)
        );
        assert!(close(m10.flank_angle(), 30f64.to_radians()));
    }

    #[test]
    fn custom_threads_take_their_own_size_on_the_60_degree_profile() {
        // 12 x 3 (mitcad#893): no ISO metric thread has that pitch.
        assert!(lookup(ThreadStandard::IsoMetric, "M12x3").is_err());
        let custom = ThreadData::custom(12.0, 3.0).unwrap();
        assert_eq!(custom.standard, ThreadStandard::Custom);
        assert_eq!(custom.designation, "12x3");
        let h = 3f64.sqrt() / 2.0 * 3.0;
        assert!(close(custom.major, 12.0) && close(custom.pitch, 3.0));
        assert!(close(custom.minor, 12.0 - 1.25 * h), "{}", custom.minor);
        assert!(close(custom.pitch_diameter, 12.0 - 0.75 * h));
        assert!(close(custom.depth, 0.625 * h));
        assert_eq!(custom.starts, 1);
        assert!(custom.taper.is_none());
        assert_eq!(
            custom.profile(false),
            (custom.major, custom.minor, custom.pitch_diameter)
        );
        assert!(close(custom.flank_angle(), 30f64.to_radians()));
        assert_eq!(
            ThreadData::custom(10.5, 1.25).unwrap().designation,
            "10.5x1.25"
        );
        assert_eq!(
            ThreadData::custom(10.0, 4.0 / 3.0).unwrap().designation,
            "10x1.3333"
        );
        // Sizes that are none, and a pitch that leaves no core.
        for (major, pitch) in [(0.0, 1.0), (10.0, 0.0), (-1.0, 1.0), (f64::NAN, 1.0)] {
            assert!(ThreadData::custom(major, pitch).is_err(), "{major} {pitch}");
        }
        let error = ThreadData::custom(10.0, 9.5).unwrap_err();
        assert!(error.contains("too coarse"), "{error}");
        // Not in the table: no designation, sizes or classes; modelled.
        let error = lookup(ThreadStandard::Custom, "12x3").unwrap_err();
        assert!(
            error.contains("custom thread has no designation"),
            "{error}"
        );
        assert!(sizes(ThreadStandard::Custom).is_empty());
        let (external, internal) = classes(ThreadStandard::Custom);
        assert!(external.is_empty() && internal.is_empty());
        assert!(ThreadStandard::Custom.check_modeled().is_ok());
        assert!(!ThreadStandard::ALL.contains(&ThreadStandard::Custom));
        assert_eq!(ThreadStandard::Custom.title(), "Custom");
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
            assert!(sizes.len() > 10, "{standard:?}");
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
        // The message names the class to use instead (mitcad#691).
        assert_eq!(error, "class 6g is for external threads; use 6H for a hole");
        assert_eq!(
            check_class(ThreadStandard::IsoMetric, "6H", Some(false)).unwrap_err(),
            "class 6H is for internal threads; use 6g for a shaft"
        );
        assert_eq!(
            check_class(ThreadStandard::Unified, "2A", Some(true)).unwrap_err(),
            "class 2A is for external threads; use 2B for a hole"
        );
        assert!(check_class(ThreadStandard::IsoMetric, "9z", None).is_err());
        assert!(check_class(ThreadStandard::Unified, "2B", Some(true)).is_ok());
        assert!(check_class(ThreadStandard::Whitworth, "Normal", Some(true)).is_ok());
        assert!(check_class(ThreadStandard::Whitworth, "Close", Some(true)).is_err());
        // A pipe thread's class A is on internal threads in the stored designs.
        assert!(check_class(ThreadStandard::Whitworth, "A", Some(true)).is_ok());
        assert!(check_class(ThreadStandard::Whitworth, "A", Some(false)).is_ok());
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
        // The geometry cuts 60-degree threads, straight or tapered.
        assert!(ThreadStandard::Unified.check_modeled().is_ok());
        assert!(ThreadStandard::Whitworth.check_modeled().is_err());
        assert!(ThreadStandard::Npt.check_modeled().is_ok());
        assert!(ThreadStandard::IsoTaper.check_modeled().is_err());
    }

    #[test]
    fn taper_pipe_threads_start_at_their_gauge_diameters() {
        // ASME B1.20.1, 1/2-14 NPT: E0 = 0.75843 in, E1 = E0 + L1/16 =
        // 0.77843 in, h = 0.8 P.
        let npt = lookup(ThreadStandard::Npt, "1/2 - 14 NPT").unwrap();
        assert_eq!(npt.designation, "1/2-14 NPT");
        let taper = npt.taper.unwrap();
        assert!(close(taper.per_length, 1.0 / 16.0));
        assert!((taper.external_start - 0.758_43 * 25.4).abs() < 1e-4);
        assert!((taper.internal_start - 0.778_43 * 25.4).abs() < 1e-4);
        let (major, minor, pitch) = npt.taper_start(true).unwrap();
        assert!(close(pitch, taper.internal_start));
        assert!(close(major - pitch, 0.8 * 25.4 / 14.0) && close(pitch - minor, major - pitch));
        // The bore's minor diameter at the opening: 0.72129 in.
        assert!((minor - 0.721_29 * 25.4).abs() < 1e-3, "{minor}");
        assert!(close(
            npt.taper_start(false).unwrap().2,
            taper.external_start
        ));
        // Its tap drill: the minor diameter L2 = 0.5337 in into the hole.
        assert!((npt.tap_drill().unwrap() - 0.687_93 * 25.4).abs() < 1e-3);
        // A straight thread has none.
        assert!(
            lookup(ThreadStandard::IsoMetric, "M10")
                .unwrap()
                .taper
                .is_none()
        );
        assert!(ThreadStandard::Npt.is_tapered() && !ThreadStandard::Whitworth.is_tapered());

        // ISO 7-1, Rc 1/2: the gauge diameter 20.955 at the opening, the
        // 55-degree profile's h = 0.640327 P; R 1/2 starts the gauge length
        // (8.2) before the gauge plane.
        let rc = lookup(ThreadStandard::IsoTaper, "Rc1/2").unwrap();
        assert_eq!(rc.designation, "Rc 1/2");
        let h = 0.640_327 * 25.4 / 14.0;
        let (major, minor, _) = rc.taper_start(true).unwrap();
        assert!(close(major, 20.955) && close(minor, 20.955 - 2.0 * h));
        let r = lookup(ThreadStandard::IsoTaper, "R 1/2").unwrap();
        assert_eq!(r.designation, "R 1/2");
        let (major, _, _) = r.taper_start(false).unwrap();
        assert!(close(major, 20.955 - 8.2 / 16.0), "{major}");
        for bad in ["Rp 1/2", "R 5/8", "G 1/2", ""] {
            assert!(lookup(ThreadStandard::IsoTaper, bad).is_err(), "{bad}");
        }
        assert_eq!(
            ThreadStandard::IsoTaper.title(),
            "ISO Taper Pipe Threads (R, Rc)"
        );
    }
}
