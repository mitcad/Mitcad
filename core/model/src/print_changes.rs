// SPDX-License-Identifier: MIT
//! Filament changes at print heights (mitcad#1596): a schedule of heights
//! on the print bed and what happens there on a single extruder, a manual
//! change to another filament (a material and its appearance) or a pause,
//! kept in the design, written into a slicer's 3MF project and shown as
//! print colours. The design's geometry never changes for it.
//!
//! **Scope: a plate.** A manual change on a single extruder changes the
//! one filament loaded, so it changes the whole layer: every object on
//! the plate prints with the new filament from that height on. A schedule
//! therefore belongs to a plate, never to a body: the design's own
//! schedule ([`ChangeSchedule::plate`] none) is for an export of bodies
//! and for every print plate without one of its own; a print plate
//! (mitcad#1050) may have its own instead. Below the first change every
//! part on the plate must print with one filament (one material and
//! appearance): a plate whose parts print with more is a conflict, which
//! the export reports and writes no events for (several filaments at once
//! are a multi-material print, not a manual change).
//!
//! **Coordinates.** A height is millimetres above the print bed, the
//! bed's Z after the export's orientation (a body's print bed face, the
//! automatic orientation) and drop, never the design's Z: in a 3MF each
//! object is measured from its own lowest point, where the slicer lays it
//! on the bed. Heights are given in millimetres whatever the design's
//! length unit.
//!
//! **Layers.** A change happens between two layers: at a layer boundary.
//! A height is moved to the nearest boundary ([`Layers::boundary`], a tie
//! upwards), never below the first layer's top, so the first layer always
//! prints with the plate's starting filament (the bodies' own material
//! and appearance). The layer height and first layer height are the
//! plate's process values (`layer_height`, `first_layer_height`; 0.2 mm
//! each when unset; variable layer heights are not known to the design).
//! The event is written at the top of the first layer printed after it
//! ([`Layers::print_z`]), which every supported slicer assigns to exactly
//! that layer (see `docs/architecture.md`, "Filament changes at print
//! heights", for the slicers' own source lines). A change whose boundary
//! is at or above the top of the tallest object on the plate prints
//! nothing and is left out with a note; two events on one layer keep the
//! later one.
//!
//! **Validation** ([`normalized`]): at most [`MAX_EVENTS`] events, each
//! height above 0 and at most [`MAX_HEIGHT`] mm, one event per height
//! (sorted upwards); a change names a material (and optionally an
//! appearance), a pause names none and may carry a short message.
//!
//! **Preview** ([`band_mesh`]): triangles are cut by planes at the layer
//! boundaries, measured along the placement's print direction from its
//! lowest point, and each piece takes the colour of the filament printing
//! there; the design's shapes are not touched, only the drawn or written
//! triangles. `model_render` and the USDZ export use the same cuts.

use serde::{Deserialize, Serialize};

/// The most events a schedule holds.
pub const MAX_EVENTS: usize = 64;
/// The highest height an event may have, mm.
pub const MAX_HEIGHT: f64 = 2000.0;
/// The longest pause message, characters.
pub const MAX_MESSAGE: usize = 120;
/// The layer height used when a plate's process sets none, mm.
pub const DEFAULT_LAYER: f64 = 0.2;
/// Two heights closer than this are one height, mm.
const SAME: f64 = 1e-6;

/// What happens at a height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// A manual change to another filament.
    #[default]
    Change,
    /// A pause (for inserts, magnets, nuts), the filament unchanged.
    Pause,
}

impl ChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Change => "change",
            Self::Pause => "pause",
        }
    }
}

fn is_change(kind: &ChangeKind) -> bool {
    *kind == ChangeKind::Change
}

/// An event of a schedule: from `height` (mm above the print bed) on, the
/// filament `material` (with `appearance`, else the material's default),
/// or a pause with its `message`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilamentChange {
    pub height: f64,
    #[serde(default, skip_serializing_if = "is_change")]
    pub kind: ChangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl FilamentChange {
    /// Whether the event can be one on its own (the schedule checks the
    /// heights together).
    pub fn check(&self) -> Result<(), String> {
        if !(self.height.is_finite() && self.height > 0.0 && self.height <= MAX_HEIGHT) {
            return Err(format!(
                "a filament change height is above 0 and at most {MAX_HEIGHT} mm above the print \
                 bed, not {}",
                self.height
            ));
        }
        let blank = |s: &Option<String>| s.as_deref().is_some_and(|s| s.trim().is_empty());
        if blank(&self.material) || blank(&self.appearance) {
            return Err(format!(
                "the event at {} mm names an empty material or appearance",
                mm(self.height)
            ));
        }
        match self.kind {
            ChangeKind::Change => {
                if self.material.is_none() {
                    return Err(format!(
                        "the change at {} mm names the material it changes to",
                        mm(self.height)
                    ));
                }
                if self.message.is_some() {
                    return Err(format!(
                        "the change at {} mm has a message: only a pause shows one",
                        mm(self.height)
                    ));
                }
            }
            ChangeKind::Pause => {
                if self.material.is_some() || self.appearance.is_some() {
                    return Err(format!(
                        "the pause at {} mm keeps the filament: it names no material or appearance",
                        mm(self.height)
                    ));
                }
                if let Some(message) = &self.message
                    && (message.chars().count() > MAX_MESSAGE
                        || message.chars().any(char::is_control))
                {
                    return Err(format!(
                        "the pause message at {} mm is one line of at most {MAX_MESSAGE} characters",
                        mm(self.height)
                    ));
                }
            }
        }
        Ok(())
    }
}

/// A plate's schedule: the design's own (`plate` none) or a print plate's
/// (its id), its events sorted upwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSchedule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate: Option<String>,
    pub changes: Vec<FilamentChange>,
}

impl ChangeSchedule {
    /// Whether it is valid as the design file keeps it: one event or more,
    /// each valid, sorted, one per height.
    pub fn check(&self) -> Result<(), String> {
        if self.changes.is_empty() {
            return Err("a filament change schedule has one event or more".to_owned());
        }
        if self.plate.as_deref().is_some_and(|p| p.trim().is_empty()) {
            return Err("a filament change schedule's plate is a plate id".to_owned());
        }
        let sorted = normalized(&self.changes)?;
        if sorted != self.changes {
            return Err("a filament change schedule lists its heights upwards".to_owned());
        }
        Ok(())
    }
}

/// The events checked and sorted upwards; refused: more than
/// [`MAX_EVENTS`], an invalid event, two events at one height.
pub fn normalized(changes: &[FilamentChange]) -> Result<Vec<FilamentChange>, String> {
    if changes.len() > MAX_EVENTS {
        return Err(format!(
            "a schedule holds at most {MAX_EVENTS} filament changes and pauses, not {}",
            changes.len()
        ));
    }
    for change in changes {
        change.check()?;
    }
    let mut sorted = changes.to_vec();
    sorted.sort_by(|a, b| a.height.total_cmp(&b.height));
    for pair in sorted.windows(2) {
        if pair[1].height - pair[0].height < SAME {
            return Err(format!(
                "two events at {} mm: one event per height",
                mm(pair[0].height)
            ));
        }
    }
    Ok(sorted)
}

/// A height for people: `12.4`, `0.2`.
pub fn mm(height: f64) -> String {
    let text = format!("{:.6}", round6(height));
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// A length rounded to a nanometre, so that boundaries add up exactly.
fn round6(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

/// A plate's layers: the first layer's height and the others', mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layers {
    pub first: f64,
    pub height: f64,
}

impl Default for Layers {
    fn default() -> Self {
        Self {
            first: DEFAULT_LAYER,
            height: DEFAULT_LAYER,
        }
    }
}

impl Layers {
    /// The layers of a process: its layer height (0.2 mm when none or not
    /// a sensible one) and first layer height (the layer height when none).
    pub fn new(first: Option<f64>, height: Option<f64>) -> Self {
        let sensible = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0 && *v <= 10.0);
        let height = sensible(height).unwrap_or(DEFAULT_LAYER);
        let first = sensible(first).unwrap_or(height);
        Self { first, height }
    }

    /// The layers of a print's settings (`layer_height`,
    /// `first_layer_height`, which may be a percentage of the layer
    /// height).
    pub fn of(settings: &crate::document::EffectivePrint) -> Self {
        use crate::print_keys::Value;
        let number = |value: Option<Value>| match value {
            Some(Value::Number(x)) => Some(x),
            Some(Value::Integer(i)) => Some(i as f64),
            _ => None,
        };
        let height = number(settings.value("layer_height"));
        let first = match settings.value("first_layer_height") {
            Some(Value::Percent(p)) => Some(height.unwrap_or(DEFAULT_LAYER) * p / 100.0),
            other => number(other),
        };
        Self::new(first, height)
    }

    /// The layer boundary an event at `height` happens at: the nearest top
    /// of a layer (a tie upwards), never below the first layer's top.
    pub fn boundary(&self, height: f64) -> f64 {
        let k = ((height - self.first) / self.height + 1e-9)
            .round()
            .max(0.0);
        round6(self.first + k * self.height)
    }

    /// The top of the first layer printed after a boundary: where a slicer
    /// takes the event (its `print_z` or `top_z`).
    pub fn print_z(&self, boundary: f64) -> f64 {
        round6(boundary + self.height)
    }

    /// The number of the layer whose top is `print_z`, from 1 (the first
    /// layer), as slicers' previews count them.
    pub fn layer_number(&self, print_z: f64) -> u64 {
        (((print_z - self.first) / self.height).round().max(0.0) as u64) + 1
    }
}

/// An event placed on the layers: its index in the schedule, the boundary
/// it happens at, the `print_z` it is written at and that layer's number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub index: usize,
    pub boundary: f64,
    pub print_z: f64,
    pub layer: u64,
}

/// The events of a schedule on the layers, for a print `top` mm high
/// (none: not known), with notes on what is left out: an event at or
/// above the top, the earlier of two events on one layer.
pub fn place(
    changes: &[FilamentChange],
    layers: Layers,
    top: Option<f64>,
) -> (Vec<Placed>, Vec<String>) {
    let mut placed: Vec<Placed> = Vec::new();
    let mut notes = Vec::new();
    for (index, change) in changes.iter().enumerate() {
        let boundary = layers.boundary(change.height);
        if let Some(top) = top
            && boundary >= top - SAME
        {
            notes.push(format!(
                "the {} at {} mm is at or above the top of the print ({} mm): left out",
                change.kind.as_str(),
                mm(change.height),
                mm(top)
            ));
            continue;
        }
        if let Some(last) = placed.last()
            && (last.boundary - boundary).abs() < SAME
        {
            notes.push(format!(
                "the events at {} mm and {} mm fall on one layer boundary ({} mm): the one at {} \
                 mm is kept",
                mm(changes[last.index].height),
                mm(change.height),
                mm(boundary),
                mm(change.height)
            ));
            placed.pop();
        }
        let print_z = layers.print_z(boundary);
        placed.push(Placed {
            index,
            boundary,
            print_z,
            layer: layers.layer_number(print_z),
        });
    }
    (placed, notes)
}

/// The print colours of a placement: the cut heights (along its print
/// direction, in the same measure as the mesh's heights) and the colour of
/// each band, `colors[0]` below the first cut.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bands {
    pub cuts: Vec<f64>,
    pub colors: Vec<[f64; 3]>,
}

impl Bands {
    /// The bands of a placement whose heights run from `low` to `high`
    /// (its lowest point lies on the bed): the starting colour, then each
    /// change's (its boundary above the bed and its colour) that lies
    /// within the placement.
    pub fn new(start: [f64; 3], changes: &[(f64, [f64; 3])], low: f64, high: f64) -> Self {
        let mut bands = Bands {
            cuts: Vec::new(),
            colors: vec![start],
        };
        for &(boundary, color) in changes {
            let cut = low + boundary;
            if cut <= low + SAME || cut >= high - SAME {
                continue;
            }
            bands.cuts.push(cut);
            bands.colors.push(color);
        }
        bands
    }
}

/// A mesh cut into height bands ([`band_mesh`]): its vertices (the
/// original ones first, then those made on the cuts), normals (when it has
/// them), triangles, and for each triangle the original triangle it is a
/// piece of and its band.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Banded {
    pub vertices: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub source: Vec<u32>,
    pub band: Vec<usize>,
}

/// Cuts a triangle mesh at the heights `cuts` (ascending), `height` giving
/// a point's height (an affine function, e.g. along the print direction of
/// the placement). Band `b` holds what lies between `cuts[b - 1]` and
/// `cuts[b]` (below the first cut band 0, above the last the last band).
/// A triangle within one band is kept as it is (its vertices shared); one
/// across cuts is split into a convex piece per band, each a fan of
/// triangles with new vertices on the cuts (normals interpolated). The
/// pieces of a triangle follow each other in the triangles' order and keep
/// its turn. `normals` may be empty.
pub fn band_mesh(
    vertices: &[[f64; 3]],
    normals: &[[f64; 3]],
    triangles: &[[u32; 3]],
    height: impl Fn([f64; 3]) -> f64,
    cuts: &[f64],
) -> Banded {
    let with_normals = normals.len() == vertices.len() && !normals.is_empty();
    let mut out = Banded {
        vertices: vertices.to_vec(),
        normals: if with_normals {
            normals.to_vec()
        } else {
            Vec::new()
        },
        triangles: Vec::with_capacity(triangles.len()),
        source: Vec::with_capacity(triangles.len()),
        band: Vec::with_capacity(triangles.len()),
    };
    let heights: Vec<f64> = vertices.iter().map(|&v| height(v)).collect();
    let band_of = |h: f64| cuts.iter().filter(|&&c| h >= c).count();
    for (t, tri) in triangles.iter().enumerate() {
        let h = tri.map(|i| heights[i as usize]);
        let low = h[0].min(h[1]).min(h[2]);
        let high = h[0].max(h[1]).max(h[2]);
        let crosses = cuts.iter().any(|&c| low < c - SAME && high > c + SAME);
        if !crosses {
            out.triangles.push(*tri);
            out.source.push(t as u32);
            out.band.push(band_of((h[0] + h[1] + h[2]) / 3.0));
            continue;
        }
        // The triangle as a polygon of (point, normal, height).
        let corner = |i: u32| {
            let i = i as usize;
            Corner {
                point: vertices[i],
                normal: if with_normals { normals[i] } else { [0.0; 3] },
                height: heights[i],
            }
        };
        let polygon: Vec<Corner> = tri.iter().map(|&i| corner(i)).collect();
        for b in band_of(low + SAME)..=band_of(high - SAME).min(cuts.len()) {
            let mut piece = polygon.clone();
            if b > 0 {
                piece = clip(&piece, cuts[b - 1], true);
            }
            if b < cuts.len() {
                piece = clip(&piece, cuts[b], false);
            }
            if piece.len() < 3 {
                continue;
            }
            let first = out.vertices.len() as u32;
            for c in &piece {
                out.vertices.push(c.point);
                if with_normals {
                    out.normals.push(unit_or(c.normal));
                }
            }
            for k in 1..piece.len() - 1 {
                let triangle = [first, first + k as u32, first + k as u32 + 1];
                if area(&out.vertices, triangle) <= 1e-18 {
                    continue;
                }
                out.triangles.push(triangle);
                out.source.push(t as u32);
                out.band.push(b);
            }
        }
    }
    out
}

/// A polygon's corner while it is cut.
#[derive(Debug, Clone, Copy)]
struct Corner {
    point: [f64; 3],
    normal: [f64; 3],
    height: f64,
}

/// The part of a convex polygon above (`above`) or below a height.
fn clip(polygon: &[Corner], cut: f64, above: bool) -> Vec<Corner> {
    let inside = |c: &Corner| {
        if above {
            c.height >= cut
        } else {
            c.height <= cut
        }
    };
    let mut out = Vec::with_capacity(polygon.len() + 1);
    for (i, a) in polygon.iter().enumerate() {
        let b = &polygon[(i + 1) % polygon.len()];
        if inside(a) {
            out.push(*a);
        }
        if inside(a) != inside(b) {
            let t = (cut - a.height) / (b.height - a.height);
            let lerp = |x: [f64; 3], y: [f64; 3]| {
                [
                    x[0] + (y[0] - x[0]) * t,
                    x[1] + (y[1] - x[1]) * t,
                    x[2] + (y[2] - x[2]) * t,
                ]
            };
            out.push(Corner {
                point: lerp(a.point, b.point),
                normal: lerp(a.normal, b.normal),
                height: cut,
            });
        }
    }
    out
}

/// A vector of unit length, or itself when it has none.
fn unit_or(v: [f64; 3]) -> [f64; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 {
        v.map(|c| c / length)
    } else {
        v
    }
}

/// Twice a triangle's area squared, near enough to find degenerate ones.
fn area(vertices: &[[f64; 3]], [a, b, c]: [u32; 3]) -> f64 {
    let [a, b, c] = [a, b, c].map(|i| vertices[i as usize]);
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    n[0] * n[0] + n[1] * n[1] + n[2] * n[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(height: f64, material: &str) -> FilamentChange {
        FilamentChange {
            height,
            kind: ChangeKind::Change,
            material: Some(material.to_owned()),
            appearance: None,
            message: None,
        }
    }

    fn pause(height: f64) -> FilamentChange {
        FilamentChange {
            height,
            kind: ChangeKind::Pause,
            material: None,
            appearance: None,
            message: Some("Insert the magnets".to_owned()),
        }
    }

    #[test]
    fn events_are_checked_and_sorted() {
        let sorted = normalized(&[change(10.0, "pla"), pause(4.0)]).unwrap();
        assert_eq!(sorted[0].height, 4.0);
        assert_eq!(sorted[1].height, 10.0);
        assert!(normalized(&[change(4.0, "pla"), pause(4.0)]).is_err());
        assert!(normalized(&[change(0.0, "pla")]).is_err());
        assert!(normalized(&[change(-1.0, "pla")]).is_err());
        assert!(normalized(&[change(f64::NAN, "pla")]).is_err());
        assert!(normalized(&[change(MAX_HEIGHT + 1.0, "pla")]).is_err());
        let mut no_material = change(5.0, "pla");
        no_material.material = None;
        assert!(normalized(&[no_material]).is_err());
        let mut pause_with_material = pause(5.0);
        pause_with_material.material = Some("pla".to_owned());
        assert!(normalized(&[pause_with_material]).is_err());
        let many: Vec<FilamentChange> = (1..=MAX_EVENTS + 1)
            .map(|i| change(i as f64, "pla"))
            .collect();
        assert!(normalized(&many).is_err());
    }

    #[test]
    fn a_schedule_is_kept_sorted() {
        let schedule = ChangeSchedule {
            plate: None,
            changes: vec![pause(4.0), change(10.0, "pla")],
        };
        schedule.check().unwrap();
        let unsorted = ChangeSchedule {
            plate: None,
            changes: vec![change(10.0, "pla"), pause(4.0)],
        };
        assert!(unsorted.check().is_err());
        let empty = ChangeSchedule {
            plate: None,
            changes: Vec::new(),
        };
        assert!(empty.check().is_err());
        // The file's form: the kind left out for a change.
        let json = serde_json::to_value(&schedule).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"changes": [
                {"height": 4.0, "kind": "pause", "message": "Insert the magnets"},
                {"height": 10.0, "material": "pla"}
            ]})
        );
    }

    #[test]
    fn heights_move_to_the_nearest_layer_boundary() {
        let layers = Layers::new(Some(0.3), Some(0.2));
        // 10 mm lies on a boundary: 0.3 + 48 * 0.2 = 9.9, then 10.1; 10.0
        // is a tie, which goes up.
        assert_eq!(layers.boundary(9.9), 9.9);
        assert_eq!(layers.boundary(10.0), 10.1);
        assert_eq!(layers.boundary(9.95), 9.9);
        // Never within the first layer.
        assert_eq!(layers.boundary(0.05), 0.3);
        // The event's layer: the first above the boundary.
        assert_eq!(layers.print_z(9.9), 10.1);
        assert_eq!(layers.layer_number(10.1), 50);
        assert_eq!(layers.layer_number(0.3), 1);
        // Defaults.
        assert_eq!(Layers::new(None, None), Layers::default());
        assert_eq!(Layers::new(None, Some(0.1)).first, 0.1);
        assert_eq!(Layers::new(Some(-1.0), Some(f64::NAN)), Layers::default());
    }

    #[test]
    fn events_above_the_print_or_on_one_layer_are_left_out() {
        let layers = Layers::default();
        let changes = [change(5.0, "a"), change(5.05, "b"), change(30.0, "c")];
        let (placed, notes) = place(&changes, layers, Some(20.0));
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].index, 1);
        assert_eq!(placed[0].boundary, 5.0);
        assert_eq!(placed[0].print_z, 5.2);
        assert_eq!(placed[0].layer, 26);
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(notes[1].contains("top of the print"));
        let (placed, notes) = place(&changes[2..], layers, None);
        assert_eq!(placed.len(), 1);
        assert!(notes.is_empty());
    }

    #[test]
    fn a_triangle_across_a_cut_is_split_into_bands() {
        // A triangle from z 0 to 10, cut at 4: a triangle below and a
        // quadrilateral (two triangles) above.
        let vertices = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]];
        let normals = [[0.0, -1.0, 0.0]; 3];
        let banded = band_mesh(&vertices, &normals, &[[0, 1, 2]], |p| p[2], &[4.0]);
        assert_eq!(banded.band, vec![0, 0, 1]);
        assert_eq!(banded.source, vec![0, 0, 0]);
        assert_eq!(banded.normals.len(), banded.vertices.len());
        // Every piece of band 0 lies at or below the cut, of band 1 above.
        for (t, &b) in banded.triangles.iter().zip(&banded.band) {
            for i in t {
                let z = banded.vertices[*i as usize][2];
                if b == 0 {
                    assert!(z <= 4.0 + 1e-12);
                } else {
                    assert!(z >= 4.0 - 1e-12);
                }
            }
        }
        // The pieces' areas add up to the triangle's.
        let total: f64 = banded
            .triangles
            .iter()
            .map(|&t| area(&banded.vertices, t).sqrt() / 2.0)
            .sum();
        assert!((total - 50.0).abs() < 1e-9, "{total}");
        // A triangle within a band is kept with its vertices.
        let kept = band_mesh(&vertices, &[], &[[0, 1, 2]], |p| p[2], &[20.0]);
        assert_eq!(kept.triangles, vec![[0, 1, 2]]);
        assert_eq!(kept.band, vec![0]);
        assert!(kept.normals.is_empty());
    }

    #[test]
    fn bands_take_the_changes_within_the_placement() {
        let red = [1.0, 0.0, 0.0];
        let blue = [0.0, 0.0, 1.0];
        let white = [1.0; 3];
        let bands = Bands::new(white, &[(5.0, red), (50.0, blue)], 10.0, 30.0);
        assert_eq!(bands.cuts, vec![15.0]);
        assert_eq!(bands.colors, vec![white, red]);
    }
}
