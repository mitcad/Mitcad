// SPDX-License-Identifier: MIT
//! BUILD parts (mitcad#375): the `build` block of a library component,
//! what `library check` says about it, and how parts are matched to what
//! the BUILD tab asks for (a junction of two profiles, or a role and a
//! slot system in a picker). The format is in `docs/libraries.md`, "BUILD
//! parts"; the commands in `core/model-api/src/commands.md`,
//! "Library commands".
//!
//! The block is kept in the manifest as it was written (unknown fields
//! survive a round trip) and read here leniently: a value is a JSON number
//! (mm, kg, degrees) or text with a unit (`"9 mm"`, `"0.9 kg"`).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::manifest::{Manifest, ManifestComponent, Problems, valid_id};

/// What a BUILD part is.
pub const ROLES: [&str; 6] = ["profile", "connector", "fastener", "panel", "linear", "cap"];
/// The kinds of mount frames.
/// (`tongue` and `groove`: a profiled board's edges, mitcad#477.)
pub const FRAME_KINDS: [&str; 9] = [
    "end", "slot", "bore", "face", "rail", "shaft", "hole", "tongue", "groove",
];
/// Where a connector can go.
pub const JUNCTIONS: [&str; 5] = ["L", "T", "cross", "inline", "surface"];
/// Where a connector sits at its junction.
pub const SPACES: [&str; 4] = ["inner_corner", "outer", "end", "through"];
/// What a linear part is.
pub const LINEAR_KINDS: [&str; 11] = [
    "rail", "carriage", "shaft", "bearing", "screw", "nut", "pulley", "gear", "rack", "belt",
    "plate",
];
/// The joint kinds a linear part's mate may ask for.
pub const JOINT_KINDS: [&str; 7] = [
    "rigid",
    "revolute",
    "slider",
    "cylindrical",
    "pin_slot",
    "planar",
    "ball",
];
/// The slot widths of the usual systems (mm).
pub const SLOT_WIDTHS: [f64; 4] = [5.0, 6.0, 8.0, 10.0];
/// The sides of a profile, by their outward direction.
pub const PROFILE_SIDES: [&str; 4] = ["x+", "x-", "y+", "y-"];

/// The prefix of a mount frame's joint origin in the design.
pub const MOUNT_PREFIX: &str = "mount:";

/// What a quantity measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// mm
    Length,
    /// kg
    Mass,
    /// degrees
    Angle,
}

/// A value of the block: a number (mm, kg, degrees) or text with a unit
/// (`"9 mm"`, `"6 m"`, `"0.9 kg"`, `"900 g"`, `"45 deg"`).
pub fn quantity(value: &Value, kind: Kind) -> Result<f64, String> {
    let text = match value {
        Value::Number(n) => return n.as_f64().ok_or_else(|| format!("{n} is no number")),
        Value::String(s) => s.trim(),
        other => return Err(format!("{other} is no value")),
    };
    let split = text
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-' || c == '+' || c == 'e')
        })
        .unwrap_or(text.len());
    // `e` belongs to the number only when a digit follows it.
    let (mut number, mut unit) = text.split_at(split);
    if number.ends_with('e') {
        number = &number[..number.len() - 1];
        unit = &text[number.len()..];
    }
    let number: f64 = number
        .replace(',', ".")
        .parse()
        .map_err(|_| format!("“{text}” is no value"))?;
    let factor = match (kind, unit.trim()) {
        (Kind::Length, "" | "mm") => 1.0,
        (Kind::Length, "cm") => 10.0,
        (Kind::Length, "m") => 1000.0,
        (Kind::Length, "in" | "\"") => 25.4,
        (Kind::Length, "ft") => 304.8,
        (Kind::Mass, "" | "kg") => 1.0,
        (Kind::Mass, "g") => 0.001,
        (Kind::Angle, "" | "deg" | "°") => 1.0,
        (Kind::Angle, "rad") => 180.0 / std::f64::consts::PI,
        (_, unit) => {
            let what = match kind {
                Kind::Length => "a length",
                Kind::Mass => "a mass",
                Kind::Angle => "an angle",
            };
            return Err(format!("“{text}” is not {what} ({unit} is no unit of it)"));
        }
    };
    if !number.is_finite() {
        return Err(format!("“{text}” is no value"));
    }
    Ok(number * factor)
}

/// A slot system: the profile type, the slot width and the series.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct System {
    /// `I` or `B`.
    pub kind: String,
    /// The slot width, mm.
    pub slot: Option<f64>,
    /// The series (20, 30, 40, ...).
    pub grid: Option<f64>,
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

impl System {
    /// `{"type": "B", "slot": 8, "grid": 30}`, or text: `B/8/30`, `B8`,
    /// `I 5 20`.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let system = match value {
            Value::Object(object) => Self {
                kind: object
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_ascii_uppercase(),
                slot: object.get("slot").and_then(number),
                grid: object.get("grid").and_then(number),
            },
            Value::String(text) => {
                let text = text.trim();
                let mut chars = text.chars();
                let kind = chars.next().map(|c| c.to_ascii_uppercase().to_string());
                let numbers: Vec<f64> = chars
                    .as_str()
                    .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                    .filter(|part| !part.is_empty())
                    .filter_map(|part| part.parse().ok())
                    .collect();
                Self {
                    kind: kind.unwrap_or_default(),
                    slot: numbers.first().copied(),
                    grid: numbers.get(1).copied(),
                }
            }
            other => return Err(format!("{other} is no slot system")),
        };
        if system.kind != "I" && system.kind != "B" {
            return Err(format!(
                "{value} is no slot system: its type is {}, not I or B",
                system.kind
            ));
        }
        if system.slot.is_some_and(|s| s <= 0.0) || system.grid.is_some_and(|g| g <= 0.0) {
            return Err(format!("{value} is no slot system: sizes are above 0"));
        }
        Ok(system)
    }

    /// `B-type slot 8 (30)`.
    pub fn text(&self) -> String {
        let mut out = format!("{}-type", self.kind);
        if let Some(slot) = self.slot {
            out.push_str(&format!(" slot {}", trim_number(slot)));
        }
        if let Some(grid) = self.grid {
            out.push_str(&format!(" ({})", trim_number(grid)));
        }
        out
    }

    /// Whether a part made for `self` fits `other`: the same type, and the
    /// same slot and series where both name one.
    pub fn fits(&self, other: &Self) -> bool {
        let same = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-6,
            _ => true,
        };
        self.kind == other.kind && same(self.slot, other.slot) && same(self.grid, other.grid)
    }

    /// Whether both name the same series (a closer fit than one without).
    fn same_grid(&self, other: &Self) -> bool {
        matches!((self.grid, other.grid), (Some(a), Some(b)) if (a - b).abs() < 1e-6)
    }
}

fn trim_number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A component's `build` block, read.
#[derive(Debug, Clone, Copy)]
pub struct BuildPart<'a> {
    pub value: &'a Value,
}

impl<'a> BuildPart<'a> {
    pub fn of(component: &'a ManifestComponent) -> Option<Self> {
        component
            .build
            .as_ref()
            .filter(|b| b.is_object())
            .map(|value| Self { value })
    }

    pub fn role(&self) -> &'a str {
        self.value.get("role").and_then(Value::as_str).unwrap_or("")
    }

    fn block(&self, key: &str) -> Option<&'a Value> {
        self.value.get(key).filter(|v| v.is_object())
    }

    /// The systems the part is made for: a profile's `system`, the others'
    /// `systems`. Entries that cannot be read are left out.
    pub fn systems(&self) -> Vec<System> {
        let mut out = Vec::new();
        if let Some(system) = self.value.get("system")
            && let Ok(system) = System::from_value(system)
        {
            out.push(system);
        }
        for system in self
            .value
            .get("systems")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Ok(system) = System::from_value(system) {
                out.push(system);
            }
        }
        out
    }

    /// The frames: name and entry.
    pub fn frames(&self) -> Vec<(&'a str, &'a Value)> {
        self.value
            .get("frames")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|f| Some((f.get("name")?.as_str()?, f)))
            .collect()
    }

    /// The library references the part names (a connector's nuts and
    /// screws, a panel's clamp), with where they are in the block.
    pub fn references(&self) -> Vec<(String, &'a Value)> {
        let mut out = Vec::new();
        if let Some(connector) = self.block("connector") {
            for (i, f) in connector
                .get("fasteners")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                for key in ["nut", "screw", "washer", "nail"] {
                    if let Some(reference) = f.get(key).filter(|r| !r.is_null()) {
                        out.push((format!("connector.fasteners[{i}].{key}"), reference));
                    }
                }
            }
        }
        if let Some(clamp) = self
            .block("panel")
            .and_then(|p| p.get("clamp"))
            .filter(|c| !c.is_null())
        {
            out.push(("panel.clamp".to_owned(), clamp));
        }
        out
    }

    /// A timber member (mitcad#478): `kind` `timber`.
    pub fn is_timber(&self) -> bool {
        self.value.get("kind").and_then(Value::as_str) == Some("timber")
    }

    /// A connector on the members' outer faces, fitted by their sections
    /// (a bracket, joist hanger or post base).
    pub fn is_outer(&self) -> bool {
        self.block("connector")
            .and_then(|c| c.get("placement"))
            .and_then(Value::as_str)
            == Some("outer")
    }

    /// A wood screw or nail: a `wood` block.
    pub fn is_wood_fastener(&self) -> bool {
        self.block("wood").is_some()
    }

    /// A sheet laid on a member's face.
    pub fn is_face_sheet(&self) -> bool {
        self.block("panel")
            .and_then(|p| p.get("mount"))
            .and_then(Value::as_str)
            == Some("face")
    }

    /// A part of timber construction: offered only where timber is asked
    /// for, never in the slot profiles' pickers and junctions.
    pub fn is_timber_family(&self) -> bool {
        self.is_timber() || self.is_outer() || self.is_wood_fastener() || self.is_face_sheet()
    }

    /// The kinds a picker's `kind` may name: a linear part's, the block's
    /// own (`timber`), `wood` for a wood fastener, `outer` and the fit's
    /// kind (`angle`, `hanger`, `post_base`) for an outer connector.
    fn kinds(&self) -> Vec<&'a str> {
        let mut out: Vec<&'a str> = Vec::new();
        let linear = self.block("linear").and_then(|l| l.get("kind"));
        let fit = self
            .block("connector")
            .and_then(|c| c.get("fit"))
            .and_then(|f| f.get("kind"));
        for kind in [linear, self.value.get("kind"), fit]
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            out.push(kind);
        }
        if self.is_wood_fastener() {
            out.push("wood");
        }
        if self.is_outer() {
            out.push("outer");
        }
        out
    }
}

/// Whether `name` can name a frame: 1 to 64 of letters, digits, `_`, `-`,
/// `+` and `.`.
pub fn valid_frame_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'.'))
}

/// The side and number of a profile's slot frame name (`slot_x+_2`).
pub fn slot_frame(name: &str) -> Option<(&str, u32)> {
    let rest = name.strip_prefix("slot_")?;
    let (side, n) = rest.rsplit_once('_')?;
    let n: u32 = n.parse().ok()?;
    (PROFILE_SIDES.contains(&side) && n >= 1).then_some((side, n))
}

fn list(items: &[&str]) -> String {
    items.join(", ")
}

/// A length field of `block` (when there): above 0 unless `zero` allows 0.
fn length_field(
    block: &Value,
    key: &str,
    path: &str,
    zero: bool,
    label: &str,
    out: &mut Problems,
) -> Option<f64> {
    let value = block.get(key).filter(|v| !v.is_null())?;
    match quantity(value, Kind::Length) {
        Ok(v) if v > 0.0 || (zero && v == 0.0) => Some(v),
        Ok(v) => {
            out.error(format!(
                "{label}: build.{path}{key} is {v} mm; it must be {}",
                if zero { "0 or more" } else { "above 0" }
            ));
            None
        }
        Err(e) => {
            out.error(format!("{label}: build.{path}{key}: {e}"));
            None
        }
    }
}

/// Checks a library reference's form (`{"library", "component",
/// "config"}`); whether it resolves is checked with the folder.
fn check_reference(reference: &Value, path: &str, label: &str, out: &mut Problems) {
    let library = reference.get("library").and_then(Value::as_str);
    let component = reference.get("component").and_then(Value::as_str);
    if !library.is_some_and(valid_id) || !component.is_some_and(valid_id) {
        out.error(format!(
            "{label}: build.{path} is no library reference ({{\"library\": id, \"component\": \
             id, \"config\": row}})"
        ));
    }
    if reference
        .get("config")
        .is_some_and(|c| !c.is_null() && !c.is_string())
    {
        out.error(format!("{label}: build.{path}.config is no row name"));
    }
}

/// What is wrong with a component's `build` block by itself (the design
/// it names is checked with the folder, [`check_design`]).
pub fn check_component(
    _manifest: &Manifest,
    component: &ManifestComponent,
    label: &str,
    out: &mut Problems,
) {
    let Some(build) = &component.build else {
        return;
    };
    if !build.is_object() {
        out.error(format!("{label}: build is no object"));
        return;
    }
    let part = BuildPart { value: build };
    let role = part.role();
    if !ROLES.contains(&role) {
        out.error(format!(
            "{label}: build.role {role} is none of {}",
            list(&ROLES)
        ));
    }

    // Systems.
    if let Some(system) = build.get("system") {
        match System::from_value(system) {
            Ok(system) => check_system(&system, "system", label, out),
            Err(e) => out.error(format!("{label}: build.system: {e}")),
        }
    }
    let systems = build.get("systems");
    if let Some(systems) = systems {
        match systems.as_array() {
            Some(array) => {
                for (i, system) in array.iter().enumerate() {
                    match System::from_value(system) {
                        Ok(system) => check_system(&system, &format!("systems[{i}]"), label, out),
                        Err(e) => out.error(format!("{label}: build.systems[{i}]: {e}")),
                    }
                }
            }
            None => out.error(format!("{label}: build.systems is no list")),
        }
    }
    let has_systems = systems
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    // Timber parts (mitcad#478) have no slot system: a member, an outer
    // connector fitted by the members' sections. Nor has a profile without
    // slots (a steel tube, mitcad#873).
    let timber = part.is_timber();
    let outer = part.is_outer();
    let slotted = part
        .frames()
        .iter()
        .any(|(_, f)| f.get("kind").and_then(Value::as_str) == Some("slot"));
    match role {
        "profile" if build.get("system").is_none() && !timber && slotted => out.error(format!(
            "{label}: a profile names its slot system (build.system)"
        )),
        "connector" | "cap" if !has_systems && build.get("system").is_none() && !outer => {
            out.error(format!(
                "{label}: a {role} lists the slot systems it fits (build.systems)"
            ));
        }
        _ => {}
    }

    // Cut to length.
    match build.get("length") {
        Some(length) if length.is_object() => check_length(length, label, out),
        Some(_) => out.error(format!("{label}: build.length is no object")),
        None if role == "profile" => out.error(format!(
            "{label}: a profile is cut to length: it needs build.length"
        )),
        None => {}
    }

    if role != "profile"
        && let Some(section) = build.get("section").filter(|s| !s.is_null())
    {
        // A cap's section matches its profile's.
        if section.is_object() {
            for key in ["width", "height"] {
                length_field(section, key, "section.", false, label, out);
            }
        } else {
            out.error(format!("{label}: build.section is no object"));
        }
    }
    if role == "profile" {
        match build.get("section").filter(|s| s.is_object()) {
            Some(section) => {
                for key in ["width", "height"] {
                    if length_field(section, key, "section.", false, label, out).is_none()
                        && section.get(key).is_none_or(Value::is_null)
                    {
                        out.error(format!("{label}: build.section has no {key}"));
                    }
                }
            }
            None => out.error(format!(
                "{label}: a profile gives its outer size (build.section: width, height)"
            )),
        }
        match build.get("slot").filter(|s| s.is_object()) {
            Some(slot) => {
                for key in ["depth", "opening", "inner_width"] {
                    length_field(slot, key, "slot.", false, label, out);
                }
            }
            None if timber || !slotted => {}
            None => out.warn(format!(
                "{label}: no build.slot (depth, opening, inner width): panels and nuts cannot \
                 be fitted to it"
            )),
        }
        match build.get("mass_per_metre").filter(|m| !m.is_null()) {
            Some(mass) => match quantity(mass, Kind::Mass) {
                Ok(m) if m > 0.0 => {}
                Ok(_) => out.error(format!("{label}: build.mass_per_metre is not above 0")),
                Err(e) => out.error(format!("{label}: build.mass_per_metre: {e}")),
            },
            // A member's mass is its density times its volume.
            None if timber => {}
            None => out.warn(format!(
                "{label}: no build.mass_per_metre: the cut list shows no mass"
            )),
        }
    }
    check_timber_fields(build, role, label, out);

    check_frames(&part, role, label, out);

    match (role, part.block("connector")) {
        ("connector", Some(connector)) => check_connector(&part, connector, label, out),
        ("connector", None) => out.error(format!(
            "{label}: a connector says where it fits (build.connector)"
        )),
        _ => {}
    }
    match (role, part.block("panel")) {
        ("panel", Some(panel)) => check_panel(panel, label, out),
        ("panel", None) => out.error(format!("{label}: a panel says how it mounts (build.panel)")),
        _ => {}
    }
    match (role, part.block("linear")) {
        ("linear", Some(linear)) => check_linear(&part, linear, label, out),
        ("linear", None) => out.error(format!(
            "{label}: a linear part says what it is (build.linear)"
        )),
        _ => {}
    }
}

fn check_system(system: &System, path: &str, label: &str, out: &mut Problems) {
    match system.slot {
        Some(slot) if !SLOT_WIDTHS.iter().any(|w| (w - slot).abs() < 1e-6) => out.warn(format!(
            "{label}: build.{path}: slot {} is none of the usual widths 5, 6, 8, 10",
            trim_number(slot)
        )),
        None => out.warn(format!("{label}: build.{path} names no slot width")),
        _ => {}
    }
}

fn check_length(length: &Value, label: &str, out: &mut Problems) {
    match length.get("parameter").and_then(Value::as_str) {
        Some(name) if mitcad_model::expr::is_valid_name(name) => {}
        Some(name) => out.error(format!(
            "{label}: build.length.parameter {name} is no parameter name"
        )),
        None => out.error(format!(
            "{label}: build.length names the design's parameter that sets the length \
             (parameter)"
        )),
    }
    let min = length_field(length, "min", "length.", false, label, out);
    let max = length_field(length, "max", "length.", false, label, out);
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        out.error(format!(
            "{label}: build.length.min ({min} mm) is above its max ({max} mm)"
        ));
    }
    let _ = length_field(length, "default", "length.", false, label, out);
    match length.get("stock") {
        Some(Value::Array(stock)) => {
            for (i, value) in stock.iter().enumerate() {
                match quantity(value, Kind::Length) {
                    Ok(v) if v <= 0.0 => {
                        out.error(format!("{label}: build.length.stock[{i}] is not above 0"));
                    }
                    Ok(v) if min.is_some_and(|m| v < m) || max.is_some_and(|m| v > m) => {
                        out.warn(format!(
                            "{label}: build.length.stock[{i}] ({v} mm) is outside the part's \
                             limits"
                        ));
                    }
                    Ok(_) => {}
                    Err(e) => out.error(format!("{label}: build.length.stock[{i}]: {e}")),
                }
            }
        }
        Some(Value::Null) | None => {}
        Some(_) => out.error(format!("{label}: build.length.stock is no list")),
    }
    if let Some(pitch) = length.get("pitch").filter(|p| !p.is_null()) {
        if pitch.is_object() {
            if length_field(pitch, "distance", "length.pitch.", false, label, out).is_none()
                && pitch.get("distance").is_none_or(Value::is_null)
            {
                out.error(format!("{label}: build.length.pitch has no distance"));
            }
            length_field(pitch, "first", "length.pitch.", true, label, out);
        } else {
            out.error(format!("{label}: build.length.pitch is no object"));
        }
    }
}

fn check_frames(part: &BuildPart<'_>, role: &str, label: &str, out: &mut Problems) {
    let raw = part.value.get("frames");
    let entries = match raw {
        Some(Value::Array(entries)) => entries.as_slice(),
        Some(_) => {
            out.error(format!("{label}: build.frames is no list"));
            return;
        }
        None => &[],
    };
    if entries.is_empty() && role != "fastener" && ROLES.contains(&role) {
        out.error(format!(
            "{label}: a {role} names its mount frames (build.frames)"
        ));
    }
    let mut names = BTreeSet::new();
    for (i, frame) in entries.iter().enumerate() {
        let Some(name) = frame.get("name").and_then(Value::as_str) else {
            out.error(format!("{label}: build.frames[{i}] has no name"));
            continue;
        };
        if !valid_frame_name(name) {
            out.error(format!(
                "{label}: frame “{name}” is no frame name (letters, digits, _ - + .)"
            ));
        } else if !names.insert(name) {
            out.error(format!("{label}: frame {name} is listed twice"));
        }
        let kind = frame.get("kind").and_then(Value::as_str).unwrap_or("");
        if !FRAME_KINDS.contains(&kind) {
            out.error(format!(
                "{label}: frame {name}: kind {kind} is none of {}",
                list(&FRAME_KINDS)
            ));
        }
        match frame.get("accepts") {
            Some(Value::Array(accepts)) => {
                for accept in accepts {
                    if !accept.as_str().is_some_and(|a| ROLES.contains(&a)) {
                        out.error(format!(
                            "{label}: frame {name}: accepts {accept}, which is no role ({})",
                            list(&ROLES)
                        ));
                    }
                }
            }
            Some(Value::Null) | None => {}
            Some(_) => out.error(format!("{label}: frame {name}: accepts is no list")),
        }
        length_field(
            frame,
            "diameter",
            &format!("frames[{i}]."),
            false,
            label,
            out,
        );
        if role == "profile" {
            let slot = slot_frame(name);
            if kind == "slot" && slot.is_none() {
                out.error(format!(
                    "{label}: slot frame {name} is not named slot_<side>_<n> (side x+, x-, y+ \
                     or y-, n from 1)"
                ));
            }
            if slot.is_some() && kind != "slot" {
                out.error(format!(
                    "{label}: frame {name} is a slot frame of kind {kind}"
                ));
            }
            if matches!(name, "end_a" | "end_b") && kind != "end" {
                out.error(format!("{label}: frame {name} is of kind {kind}, not end"));
            }
        }
    }
    if role == "profile" {
        for end in ["end_a", "end_b"] {
            if !names.contains(end) {
                out.error(format!("{label}: a profile has the frame {end}"));
            }
        }
        // A profile without slots (a steel tube, mitcad#873) names no slot
        // system either; one that names a system needs its slot frames.
        if !part.is_timber()
            && part.value.get("system").is_some()
            && !names.iter().any(|n| slot_frame(n).is_some())
        {
            out.warn(format!(
                "{label}: the profile has no slot frames: nothing can be placed on its sides"
            ));
        }
    }
}

fn frame_names<'a>(part: &BuildPart<'a>) -> BTreeSet<&'a str> {
    part.frames().into_iter().map(|(name, _)| name).collect()
}

fn check_connector(part: &BuildPart<'_>, connector: &Value, label: &str, out: &mut Problems) {
    let frames = frame_names(part);
    match connector.get("junctions").and_then(Value::as_array) {
        Some(junctions) if !junctions.is_empty() => {
            for junction in junctions {
                if !junction.as_str().is_some_and(|j| JUNCTIONS.contains(&j)) {
                    out.error(format!(
                        "{label}: build.connector.junctions: {junction} is none of {}",
                        list(&JUNCTIONS)
                    ));
                }
            }
        }
        _ => out.error(format!(
            "{label}: build.connector.junctions lists where it can go ({})",
            list(&JUNCTIONS)
        )),
    }
    let space = connector.get("space").and_then(Value::as_str).unwrap_or("");
    if !SPACES.contains(&space) {
        out.error(format!(
            "{label}: build.connector.space {space} is none of {}",
            list(&SPACES)
        ));
    }
    if let Some(envelope) = connector.get("envelope").filter(|e| !e.is_null()) {
        if envelope.is_object() {
            for key in ["x", "y", "z"] {
                length_field(envelope, key, "connector.envelope.", true, label, out);
            }
        } else {
            out.error(format!("{label}: build.connector.envelope is no object"));
        }
    }
    let frame_known = |name: Option<&str>, path: &str, out: &mut Problems| match name {
        Some(name) if frames.contains(name) => {}
        Some(name) => out.error(format!(
            "{label}: build.connector.{path} names the frame {name}, which the part does not \
             list"
        )),
        None => out.error(format!("{label}: build.connector.{path} names no frame")),
    };
    for (i, mate) in connector
        .get("mates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        frame_known(
            mate.get("frame").and_then(Value::as_str),
            &format!("mates[{i}]"),
            out,
        );
        let to = mate.get("to").and_then(Value::as_str).unwrap_or("");
        if !FRAME_KINDS.contains(&to) {
            out.error(format!(
                "{label}: build.connector.mates[{i}].to {to} is no frame kind ({})",
                list(&FRAME_KINDS)
            ));
        }
    }
    for (i, fastener) in connector
        .get("fasteners")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        // Wood fasteners (an outer connector's) name their hole frames,
        // their count and the member they go into.
        match fastener.get("frames").and_then(Value::as_array) {
            Some(holes) => {
                for (j, hole) in holes.iter().enumerate() {
                    frame_known(hole.as_str(), &format!("fasteners[{i}].frames[{j}]"), out);
                }
                if fastener
                    .get("count")
                    .is_some_and(|c| c.as_u64() != Some(holes.len() as u64))
                {
                    out.error(format!(
                        "{label}: build.connector.fasteners[{i}].count is not the number of its \
                         frames ({})",
                        holes.len()
                    ));
                }
                if !matches!(fastener.get("on").and_then(Value::as_str), Some("a" | "b")) {
                    out.error(format!(
                        "{label}: build.connector.fasteners[{i}].on names the member they go \
                         into (a or b)"
                    ));
                }
            }
            None => frame_known(
                fastener.get("frame").and_then(Value::as_str),
                &format!("fasteners[{i}]"),
                out,
            ),
        }
        for key in ["nut", "screw", "washer", "nail"] {
            if let Some(reference) = fastener.get(key).filter(|r| !r.is_null()) {
                check_reference(
                    reference,
                    &format!("connector.fasteners[{i}].{key}"),
                    label,
                    out,
                );
            }
        }
    }
    for (i, machining) in connector
        .get("machining")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let on = machining.get("on").and_then(Value::as_str).unwrap_or("");
        if on != "a" && on != "b" {
            out.error(format!(
                "{label}: build.connector.machining[{i}].on {on} is neither a nor b"
            ));
        }
        let frame = machining.get("frame").and_then(Value::as_str).unwrap_or("");
        if frame != "end" && frame != "slot" {
            out.error(format!(
                "{label}: build.connector.machining[{i}].frame {frame} is neither end nor slot"
            ));
        }
        match machining.get("hole").filter(|h| h.is_object()) {
            Some(hole) => {
                let path = format!("connector.machining[{i}].hole.");
                for key in ["diameter", "depth"] {
                    if length_field(hole, key, &path, false, label, out).is_none()
                        && hole.get(key).is_none_or(Value::is_null)
                    {
                        out.error(format!("{label}: build.{path}{key} is missing"));
                    }
                }
                if hole
                    .get("thread")
                    .is_some_and(|t| !t.is_null() && !t.is_string())
                {
                    out.error(format!("{label}: build.{path}thread is no designation"));
                }
            }
            None => out.error(format!(
                "{label}: build.connector.machining[{i}] has no hole"
            )),
        }
    }
}

fn check_panel(panel: &Value, label: &str, out: &mut Problems) {
    let mount = panel.get("mount").and_then(Value::as_str).unwrap_or("");
    if !matches!(mount, "slot" | "surface" | "face") {
        out.error(format!(
            "{label}: build.panel.mount {mount} is none of slot, surface, face"
        ));
    }
    // A sheet's sizes sold: [[width, height], ...].
    if let Some(stock) = panel.get("stock").filter(|s| !s.is_null()) {
        let sizes = stock.as_array().filter(|sizes| {
            sizes.iter().all(|size| {
                size.as_array().is_some_and(|wh| {
                    wh.len() == 2
                        && wh
                            .iter()
                            .all(|v| quantity(v, Kind::Length).is_ok_and(|v| v > 0.0))
                })
            })
        });
        if sizes.is_none() {
            out.error(format!(
                "{label}: build.panel.stock is no list of sheet sizes [width, height]"
            ));
        }
    }
    match panel.get("thickness").filter(|t| !t.is_null()) {
        Some(Value::String(name)) if mitcad_model::expr::is_valid_name(name.trim()) => {}
        Some(value) => match quantity(value, Kind::Length) {
            Ok(t) if t > 0.0 => {}
            Ok(_) => out.error(format!("{label}: build.panel.thickness is not above 0")),
            Err(e) => out.error(format!(
                "{label}: build.panel.thickness is neither a value nor a parameter: {e}"
            )),
        },
        None => out.error(format!(
            "{label}: build.panel.thickness gives the thickness or its parameter"
        )),
    }
    for key in ["width", "height"] {
        if let Some(value) = panel.get(key).filter(|v| !v.is_null())
            && !value
                .as_str()
                .is_some_and(|n| mitcad_model::expr::is_valid_name(n.trim()))
        {
            out.error(format!(
                "{label}: build.panel.{key} names the parameter that sets it"
            ));
        }
    }
    length_field(panel, "clearance", "panel.", true, label, out);
    match panel.get("clamp").filter(|c| !c.is_null()) {
        Some(clamp) => check_reference(clamp, "panel.clamp", label, out),
        None if mount == "surface" => out.warn(format!(
            "{label}: a surface panel names the clamp that holds it (build.panel.clamp)"
        )),
        None => {}
    }
}

fn check_linear(part: &BuildPart<'_>, linear: &Value, label: &str, out: &mut Problems) {
    let kind = linear.get("kind").and_then(Value::as_str).unwrap_or("");
    if !LINEAR_KINDS.contains(&kind) {
        out.error(format!(
            "{label}: build.linear.kind {kind} is none of {}",
            list(&LINEAR_KINDS)
        ));
    }
    // A rack is a solid of its length: its teeth cannot be cut from an end
    // section.
    if matches!(kind, "rail" | "shaft") && part.value.get("length").is_none() {
        out.warn(format!(
            "{label}: a {kind} is usually cut to length (build.length)"
        ));
    }
    if let Some(size) = linear.get("size").filter(|s| !s.is_null())
        && !(size.is_string() || size.is_number())
    {
        out.error(format!(
            "{label}: build.linear.size is no size (text or a number)"
        ));
    }
    // Drive data: a screw's lead and pitch, a gear's, rack's or pulley's
    // module and teeth.
    for key in ["lead", "pitch", "module"] {
        length_field(linear, key, "linear.", false, label, out);
    }
    if let Some(teeth) = linear.get("teeth").filter(|t| !t.is_null())
        && !teeth.as_u64().is_some_and(|t| t > 0)
    {
        out.error(format!("{label}: build.linear.teeth is no count of teeth"));
    }
    if let Some(mate) = linear.get("mate").filter(|m| !m.is_null()) {
        let frames = frame_names(part);
        match mate.get("frame").and_then(Value::as_str) {
            Some(frame) if frames.contains(frame) => {}
            Some(frame) => out.error(format!(
                "{label}: build.linear.mate names the frame {frame}, which the part does not list"
            )),
            None => out.error(format!("{label}: build.linear.mate names no frame")),
        }
        let on = mate.get("on").and_then(Value::as_str).unwrap_or("");
        if !FRAME_KINDS.contains(&on) {
            out.error(format!(
                "{label}: build.linear.mate.on {on} is no frame kind ({})",
                list(&FRAME_KINDS)
            ));
        }
        let joint = mate.get("joint").and_then(Value::as_str).unwrap_or("");
        if !JOINT_KINDS.contains(&joint) {
            out.error(format!(
                "{label}: build.linear.mate.joint {joint} is none of {}",
                list(&JOINT_KINDS)
            ));
        }
        if let Some(axis) = mate.get("axis").filter(|a| !a.is_null())
            && !axis.as_str().is_some_and(|a| matches!(a, "x" | "y" | "z"))
        {
            out.error(format!(
                "{label}: build.linear.mate.axis {axis} is not x, y or z"
            ));
        }
    }
}

// ---------------------------------------------------------------------
// Timber construction (mitcad#478): members, wood fasteners, outer
// connectors and sheets, and the configuration rows' own blocks.

/// What a member's availability may be.
pub const AVAILABILITIES: [&str; 2] = ["standard", "less common"];
/// What an outer connector is fitted as.
pub const FIT_KINDS: [&str; 3] = ["angle", "hanger", "post_base"];
/// A profiled board's profile (mitcad#477).
pub const BOARD_PROFILES: [&str; 4] = ["tongue_and_groove", "shiplap", "feather_edge", "deck"];

/// A profiled board's `board` block (mitcad#477; the part's, or a row's,
/// merged over it by its readers): its profile, a cover width above 0 and
/// not over the section's width, its area (mm², above 0), its tongue,
/// overlap and edge radius as lengths and its grooves as a count.
fn check_board(
    block: &Value,
    board: &Value,
    role: &str,
    path: &str,
    label: &str,
    out: &mut Problems,
) {
    let at = format!("{path}board.");
    if !board.is_object() {
        out.error(format!("{label}: build.{path}board is no object"));
        return;
    }
    if role != "profile" {
        out.error(format!(
            "{label}: build.{path}board belongs to a timber member, not a {role}"
        ));
    }
    match board.get("profile").filter(|p| !p.is_null()) {
        Some(profile)
            if profile
                .as_str()
                .is_some_and(|p| BOARD_PROFILES.contains(&p)) => {}
        Some(profile) => out.error(format!(
            "{label}: build.{at}profile {profile} is none of {}",
            list(&BOARD_PROFILES)
        )),
        None if path.is_empty() => out.error(format!(
            "{label}: a profiled board names its profile (build.board.profile)"
        )),
        None => {}
    }
    let cover = length_field(board, "cover", &at, false, label, out);
    let width = block
        .get("section")
        .and_then(|s| s.get("width"))
        .and_then(|w| quantity(w, Kind::Length).ok());
    if let (Some(cover), Some(width)) = (cover, width)
        && cover > width + 1e-6
    {
        out.error(format!(
            "{label}: build.{at}cover is {cover} mm, wider than the board's {width} mm"
        ));
    }
    match board.get("area").filter(|a| !a.is_null()) {
        Some(area) if area.as_f64().is_some_and(|a| a > 0.0) => {}
        Some(area) => out.error(format!(
            "{label}: build.{at}area {area} is no number of mm² above 0"
        )),
        None => {}
    }
    if let Some(tongue) = board.get("tongue").filter(|t| !t.is_null()) {
        for key in ["length", "thickness"] {
            length_field(tongue, key, &format!("{at}tongue."), false, label, out);
        }
    }
    for key in ["overlap", "edge_radius"] {
        length_field(board, key, &at, false, label, out);
    }
    if let Some(grooves) = board.get("grooves").filter(|g| !g.is_null())
        && grooves.as_u64().is_none()
    {
        out.error(format!("{label}: build.{at}grooves {grooves} is no count"));
    }
}

/// A density in kg/m3: a number, or text with the unit `kg/m3`.
pub fn density(value: &Value) -> Result<f64, String> {
    let number = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s
            .trim()
            .trim_end_matches("kg/m3")
            .trim_end_matches("kg/m³")
            .trim()
            .replace(',', ".")
            .parse()
            .ok(),
        _ => None,
    };
    match number {
        Some(v) if v.is_finite() && v > 0.0 => Ok(v),
        _ => Err(format!("{value} is no density in kg/m3 above 0")),
    }
}

/// The fields of timber parts in a build block, or in a configuration
/// row's block (`build.rows`, merged over the part's by its readers).
fn check_timber_fields(block: &Value, role: &str, label: &str, out: &mut Problems) {
    check_timber_block(block, role, "", label, out);
    let part = BuildPart { value: block };
    if part.is_timber()
        && role == "profile"
        && block
            .get("material")
            .and_then(|m| m.get("density"))
            .is_none()
    {
        out.warn(format!(
            "{label}: no build.material.density: the cut list shows no mass"
        ));
    }
    match block.get("rows") {
        Some(Value::Object(rows)) => {
            for (name, row) in rows {
                if row.is_object() {
                    check_timber_block(row, role, &format!("rows.{name}."), label, out);
                } else {
                    out.error(format!("{label}: build.rows.{name} is no object"));
                }
            }
        }
        Some(Value::Null) | None => {}
        Some(_) => out.error(format!(
            "{label}: build.rows is no object of configuration rows"
        )),
    }
}

fn check_timber_block(block: &Value, role: &str, path: &str, label: &str, out: &mut Problems) {
    if let Some(kind) = block.get("kind").filter(|k| !k.is_null())
        && kind.as_str() != Some("timber")
    {
        out.error(format!("{label}: build.{path}kind {kind} is not timber"));
    }
    if let Some(availability) = block.get("availability").filter(|a| !a.is_null())
        && !availability
            .as_str()
            .is_some_and(|a| AVAILABILITIES.contains(&a))
    {
        out.error(format!(
            "{label}: build.{path}availability {availability} is none of {}",
            list(&AVAILABILITIES)
        ));
    }
    match block.get("material").filter(|m| !m.is_null()) {
        Some(material) if material.is_object() => {
            if let Some(value) = material.get("density").filter(|d| !d.is_null())
                && let Err(e) = density(value)
            {
                out.error(format!("{label}: build.{path}material.density: {e}"));
            }
        }
        Some(_) => out.error(format!("{label}: build.{path}material is no object")),
        None => {}
    }
    for key in ["section", "nominal"] {
        if !path.is_empty()
            && let Some(size) = block.get(key).filter(|s| !s.is_null())
        {
            for side in ["width", "height"] {
                length_field(size, side, &format!("{path}{key}."), false, label, out);
            }
        }
    }
    if let Some(board) = block.get("board").filter(|b| !b.is_null()) {
        check_board(block, board, role, path, label, out);
    }
    if let Some(wood) = block.get("wood").filter(|w| !w.is_null()) {
        if role != "fastener" {
            out.error(format!(
                "{label}: build.{path}wood belongs to a fastener, not a {role}"
            ));
        }
        if let Some(kind) = wood.get("kind").filter(|k| !k.is_null())
            && !matches!(kind.as_str(), Some("screw" | "nail"))
        {
            out.error(format!(
                "{label}: build.{path}wood.kind {kind} is neither screw nor nail"
            ));
        }
        for key in ["diameter", "length"] {
            length_field(wood, key, &format!("{path}wood."), false, label, out);
        }
        // The fastener Timber's frames and sets start with (mitcad#516).
        if let Some(default) = wood.get("default").filter(|d| !d.is_null())
            && !default.is_boolean()
        {
            out.error(format!(
                "{label}: build.{path}wood.default {default} is neither true nor false"
            ));
        }
    }
    let Some(connector) = block.get("connector").filter(|c| c.is_object()) else {
        return;
    };
    if let Some(placement) = connector.get("placement").filter(|p| !p.is_null())
        && placement.as_str() != Some("outer")
    {
        out.error(format!(
            "{label}: build.{path}connector.placement {placement} is not outer"
        ));
    }
    let outer = connector.get("placement").and_then(Value::as_str) == Some("outer");
    match connector.get("fit").filter(|f| !f.is_null()) {
        Some(fit) if fit.is_object() => {
            let kind = fit.get("kind").and_then(Value::as_str).unwrap_or("");
            if !FIT_KINDS.contains(&kind) {
                out.error(format!(
                    "{label}: build.{path}connector.fit.kind {kind} is none of {}",
                    list(&FIT_KINDS)
                ));
            }
            for key in ["section", "width"] {
                let Some(pair) = fit.get(key).filter(|v| !v.is_null()) else {
                    continue;
                };
                let values: Option<Vec<f64>> =
                    pair.as_array().filter(|a| a.len() == 2).and_then(|a| {
                        a.iter()
                            .map(|v| quantity(v, Kind::Length).ok().filter(|v| *v > 0.0))
                            .collect()
                    });
                match values {
                    Some(v) if key == "width" && v[0] > v[1] => out.error(format!(
                        "{label}: build.{path}connector.fit.width's min is above its max"
                    )),
                    Some(_) => {}
                    None => out.error(format!(
                        "{label}: build.{path}connector.fit.{key} is no pair of lengths above 0"
                    )),
                }
            }
        }
        Some(_) => out.error(format!("{label}: build.{path}connector.fit is no object")),
        None if outer && path.is_empty() => out.error(format!(
            "{label}: an outer connector says what it fits (build.connector.fit)"
        )),
        None => {}
    }
}

/// Whether an outer connector fits a junction of members whose sections
/// `build_junctions` gives in its `wood` (`sections.a`, `sections.b`, [W,
/// T] each): the junction's kind, and `fit.section` (member a's section,
/// either way round, within 1 mm) or `fit.width` (a's smaller dimension
/// within [min, max]) of the part or of one of its rows.
fn fit_outer(part: &BuildPart<'_>, junction: &Value, wood: &Value) -> Fit {
    let connector = part.block("connector");
    let kind = junction.get("kind").and_then(Value::as_str).unwrap_or("");
    let kinds: Vec<&str> = connector
        .and_then(|c| c.get("junctions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !kinds.contains(&kind) {
        return Fit::Near(format!(
            "it goes on {} junctions, not on {}",
            if kinds.is_empty() {
                "no".to_owned()
            } else {
                kinds.join(", ")
            },
            if kind.is_empty() { "this one" } else { kind }
        ));
    }
    let section: Option<Vec<f64>> = wood
        .get("sections")
        .and_then(|s| s.get("a"))
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .and_then(|a| a.iter().map(Value::as_f64).collect());
    let Some(a) = section else {
        return Fit::Fits(0);
    };
    let fits = |fit: &Value| -> Result<u32, String> {
        let pair = |key: &str| -> Option<Vec<f64>> {
            fit.get(key)
                .and_then(Value::as_array)
                .filter(|p| p.len() == 2)
                .and_then(|p| p.iter().map(|v| quantity(v, Kind::Length).ok()).collect())
        };
        if let Some(s) = pair("section") {
            let same = |x: f64, y: f64| (x - y).abs() <= 1.0;
            if !((same(s[0], a[0]) && same(s[1], a[1])) || (same(s[0], a[1]) && same(s[1], a[0]))) {
                return Err(format!(
                    "it fits {} × {}, the member is {} × {}",
                    trim_number(s[0]),
                    trim_number(s[1]),
                    trim_number(a[0]),
                    trim_number(a[1])
                ));
            }
            return Ok(2);
        }
        if let Some(w) = pair("width") {
            let smaller = a[0].min(a[1]);
            if smaller < w[0] - 1e-6 || smaller > w[1] + 1e-6 {
                return Err(format!(
                    "it fits members {} to {} mm thick, the member is {}",
                    trim_number(w[0]),
                    trim_number(w[1]),
                    trim_number(smaller)
                ));
            }
        }
        Ok(1)
    };
    let mut reason = None;
    let rows = part.value.get("rows").and_then(Value::as_object);
    let candidates =
        connector
            .and_then(|c| c.get("fit"))
            .into_iter()
            .chain(rows.into_iter().flat_map(|rows| {
                rows.values()
                    .filter_map(|r| r.get("connector").and_then(|c| c.get("fit")))
            }));
    for fit in candidates {
        match fits(fit) {
            Ok(score) => return Fit::Fits(score),
            Err(why) => {
                reason.get_or_insert(why);
            }
        }
    }
    Fit::Near(reason.unwrap_or_else(|| "it says nothing of what it fits".to_owned()))
}

/// How a timber part fits a junction of two members (one with `wood`):
/// wood fasteners fit any, outer connectors by [`fit_outer`]; nothing else.
fn fit_wood_junction(part: &BuildPart<'_>, junction: &Value, wood: &Value) -> Fit {
    match part.role() {
        "fastener" if part.is_wood_fastener() => Fit::Fits(0),
        "connector" if part.is_outer() => fit_outer(part, junction, wood),
        _ => Fit::No,
    }
}

// ---------------------------------------------------------------------
// The design a BUILD part names.

/// The parameters of a design file: name and expression, and whether it
/// is a user parameter (no feature owns it).
fn parameters(design: &Value) -> BTreeMap<&str, (&str, bool)> {
    design
        .get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?;
            let expression = p.get("expression").and_then(Value::as_str).unwrap_or("");
            let user = p.get("owner").is_none_or(Value::is_null);
            Some((name, (expression, user)))
        })
        .collect()
}

fn features(design: &Value) -> impl Iterator<Item = &Value> {
    design
        .get("features")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn feature<'a>(design: &'a Value, uid: &str) -> Option<&'a Value> {
    features(design).find(|f| f.get("uid").and_then(Value::as_str) == Some(uid))
}

/// The design's mount frames: the `joint_origin` features named
/// `mount:<name>`, or with a `mount` of that name (mitcad#525), by name.
pub fn mount_frames(design: &Value) -> BTreeMap<&str, &Value> {
    features(design)
        .filter(|f| f.get("type").and_then(Value::as_str) == Some("joint_origin"))
        .filter_map(|f| {
            let own = f
                .get("mount")
                .and_then(|m| m.get("name"))
                .and_then(Value::as_str);
            let name = match own {
                Some(name) => name,
                None => f.get("name")?.as_str()?.strip_prefix(MOUNT_PREFIX)?,
            };
            Some((name, f))
        })
        .collect()
}

/// Whether `text` mentions `name` as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    let is_name = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(is_name) && !after.is_some_and(is_name)
    })
}

/// Whether a definition's value slot (a parameter's name in the file)
/// uses parameter `name`: it is that parameter, or its own parameter's
/// expression names it.
fn slot_uses(design: &Value, slot: Option<&Value>, name: &str) -> bool {
    let Some(slot) = slot.and_then(Value::as_str).map(str::trim) else {
        return false;
    };
    if slot == name {
        return true;
    }
    parameters(design)
        .get(slot)
        .is_some_and(|(expression, _)| mentions(expression, name))
}

fn vector(value: Option<&Value>) -> Option<[f64; 3]> {
    let array = value?.as_array()?;
    if array.len() != 3 {
        return None;
    }
    Some([array[0].as_f64()?, array[1].as_f64()?, array[2].as_f64()?])
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

/// A joint origin's z axis and origin where its geometry is a fixed plane
/// (or the XY origin plane), after its `flip`.
fn fixed_frame(frame: &Value) -> Option<([f64; 3], [f64; 3])> {
    let flip = frame.get("flip").and_then(Value::as_bool).unwrap_or(false);
    let geometry = frame.get("geometry")?;
    let (origin, mut z) = match geometry {
        Value::String(s) if s == "xy" => ([0.0; 3], [0.0, 0.0, 1.0]),
        Value::Object(_) => (
            vector(geometry.get("origin"))?,
            vector(geometry.get("normal"))?,
        ),
        _ => return None,
    };
    if flip {
        z = [-z[0], -z[1], -z[2]];
    }
    Some((origin, z))
}

fn side_direction(side: &str) -> [f64; 3] {
    match side {
        "x+" => [1.0, 0.0, 0.0],
        "x-" => [-1.0, 0.0, 0.0],
        "y+" => [0.0, 1.0, 0.0],
        _ => [0.0, -1.0, 0.0],
    }
}

/// What is wrong with the design a BUILD part names (its file's JSON):
/// the parameters the block names are user parameters of the design,
/// every frame is a `mount:<name>` joint origin, and a profile follows the
/// layout (a section on XY extruded by the length along +Z, end A at z =
/// 0, end B following the length, slots at z = 0 facing out of their
/// sides).
pub fn check_design(
    component: &ManifestComponent,
    label: &str,
    design: &Value,
    out: &mut Problems,
) {
    let Some(part) = BuildPart::of(component) else {
        return;
    };
    let role = part.role();
    let parameters = parameters(design);
    let user = |name: &str| parameters.get(name).is_some_and(|(_, user)| *user);
    let length = part
        .value
        .get("length")
        .and_then(|l| l.get("parameter"))
        .and_then(Value::as_str);
    if let Some(name) = length
        && !user(name)
    {
        out.error(format!(
            "{label}: {}: the length parameter {name} is no user parameter of the design",
            component.path
        ));
    }
    if let Some(panel) = part.block("panel") {
        for key in ["thickness", "width", "height"] {
            if let Some(name) = panel.get(key).and_then(Value::as_str).map(str::trim)
                && mitcad_model::expr::is_valid_name(name)
                && !user(name)
            {
                out.error(format!(
                    "{label}: {}: build.panel.{key} names {name}, which is no user parameter of \
                     the design",
                    component.path
                ));
            }
        }
    }
    let mounts = mount_frames(design);
    let frames = part.frames();
    for (name, _) in &frames {
        if !mounts.contains_key(name) {
            out.error(format!(
                "{label}: {}: the frame {name} is no joint origin {MOUNT_PREFIX}{name} of the \
                 design",
                component.path
            ));
        }
    }
    let listed: BTreeSet<&str> = frames.iter().map(|(name, _)| *name).collect();
    for name in mounts.keys() {
        if !listed.contains(name) {
            out.warn(format!(
                "{label}: {}: the joint origin {MOUNT_PREFIX}{name} is not in build.frames",
                component.path
            ));
        }
    }
    // A configuration row's own block names a row of the design.
    if let Some(own) = part.value.get("rows").and_then(Value::as_object) {
        let names: BTreeSet<String> = rows(design).into_iter().map(|r| r.name).collect();
        for name in own.keys() {
            if !names.contains(name) {
                out.error(format!(
                    "{label}: {}: build.rows names {name}, which is no configuration row of the \
                     design",
                    component.path
                ));
            }
        }
    }
    for row in rows(design) {
        if let Some(url) = &row.supplier_url
            && !url.starts_with("https://")
        {
            out.error(format!(
                "{label}: {}: row {}: the supplier_url {url} is no https:// address",
                component.path, row.name
            ));
        }
    }
    if role == "profile"
        && let Some(length) = length
    {
        check_profile_layout(component, label, design, length, &mounts, out);
    }
}

fn check_profile_layout(
    component: &ManifestComponent,
    label: &str,
    design: &Value,
    length: &str,
    mounts: &BTreeMap<&str, &Value>,
    out: &mut Problems,
) {
    let path = &component.path;
    let extrusion = features(design).find(|f| {
        f.get("type").and_then(Value::as_str) == Some("extrude")
            && f.get("extent")
                .is_some_and(|e| e.get("type").and_then(Value::as_str) == Some("distance"))
            && slot_uses(
                design,
                f.get("extent").and_then(|e| e.get("distance")),
                length,
            )
    });
    match extrusion {
        None => out.error(format!(
            "{label}: {path}: no extrusion by {length} makes the profile (the layout: the \
             section on XY extruded by the length along +Z)"
        )),
        Some(extrude) => {
            if extrude.get("flip").and_then(Value::as_bool) == Some(true) {
                out.error(format!(
                    "{label}: {path}: the profile is extruded along -Z; end B is at z = {length}"
                ));
            }
            let sketch = extrude
                .get("profiles")
                .and_then(|p| p.get(0))
                .and_then(|p| p.get("sketch"))
                .and_then(Value::as_str)
                .and_then(|uid| feature(design, uid));
            if let Some(sketch) = sketch
                && sketch
                    .get("plane")
                    .is_some_and(|plane| plane.as_str() != Some("xy"))
            {
                out.error(format!(
                    "{label}: {path}: the profile's section is not sketched on XY"
                ));
            }
        }
    }
    for (name, frame) in mounts {
        let fixed = fixed_frame(frame);
        match *name {
            "end_a" => {
                if let Some((origin, z)) = fixed
                    && !(near(origin, [0.0; 3]) && near(z, [0.0, 0.0, -1.0]))
                {
                    out.error(format!(
                        "{label}: {path}: end_a is at the origin pointing along -Z"
                    ));
                }
            }
            "end_b" => {
                let geometry = frame.get("geometry");
                if geometry.is_some_and(|g| g.is_object() || g.as_str() == Some("xy")) {
                    out.error(format!(
                        "{label}: {path}: end_b is fixed; it follows the length (a joint origin \
                         on a plane offset from XY by {length})"
                    ));
                } else if let Some(plane) = geometry
                    .and_then(Value::as_str)
                    .and_then(|uid| feature(design, uid))
                {
                    // XY, or a fixed plane parallel to it, offset by the length.
                    let definition = plane.get("definition");
                    let base = definition.and_then(|d| d.get("plane"));
                    let parallel = base.is_some_and(|b| {
                        b.as_str() == Some("xy")
                            || vector(b.get("normal"))
                                .is_some_and(|n| n[0].abs() < 1e-9 && n[1].abs() < 1e-9)
                    });
                    let follows = plane.get("type").and_then(Value::as_str)
                        == Some("construction_plane")
                        && parallel
                        && slot_uses(design, definition.and_then(|d| d.get("distance")), length);
                    if !follows {
                        out.warn(format!(
                            "{label}: {path}: end_b's plane is not XY offset by {length}"
                        ));
                    }
                    if frame.get("flip").and_then(Value::as_bool) == Some(true) {
                        out.error(format!(
                            "{label}: {path}: end_b points along +Z, out of end B"
                        ));
                    }
                }
            }
            name => {
                if let Some((side, _)) = slot_frame(name)
                    && let Some((origin, z)) = fixed
                {
                    if origin[2].abs() > 1e-6 {
                        out.error(format!(
                            "{label}: {path}: the slot frame {name} is at z = 0 (end A)"
                        ));
                    }
                    if !near(z, side_direction(side)) {
                        out.error(format!(
                            "{label}: {path}: the slot frame {name} points out of the {side} side"
                        ));
                    }
                }
            }
        }
    }
}

/// A configuration row of a design, with what the BUILD tab reads of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    /// The row's own part number, before the component's.
    pub part_number: Option<String>,
    /// The row's own product page (a sheet sold in widths has one per width).
    pub supplier_url: Option<String>,
}

/// The rows of a design's configuration table.
pub fn rows(design: &Value) -> Vec<Row> {
    let text = |row: &Value, key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    };
    design
        .get("configurations")
        .and_then(|c| c.get("rows"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some(Row {
                name: row.get("name")?.as_str()?.to_owned(),
                part_number: text(row, "part_number"),
                supplier_url: text(row, "supplier_url"),
            })
        })
        .collect()
}

/// What a library reference resolves to.
pub enum Resolved {
    /// The component, with its rows' names.
    Found(Vec<String>),
    /// The library has no such component.
    Missing,
    /// The library is not at hand (not fetched here).
    Unknown,
}

/// Checks that a part's library references resolve: `resolve(library,
/// component)` says what the library has.
pub fn check_references(
    component: &ManifestComponent,
    label: &str,
    resolve: &mut dyn FnMut(&str, &str) -> Resolved,
    out: &mut Problems,
) {
    let Some(part) = BuildPart::of(component) else {
        return;
    };
    for (path, reference) in part.references() {
        let (Some(library), Some(id)) = (
            reference.get("library").and_then(Value::as_str),
            reference.get("component").and_then(Value::as_str),
        ) else {
            continue;
        };
        let config = reference.get("config").and_then(Value::as_str);
        match resolve(library, id) {
            Resolved::Found(rows) => {
                if let Some(config) = config
                    && !rows.iter().any(|r| r == config)
                {
                    out.error(format!(
                        "{label}: build.{path}: {library}/{id} has no configuration {config}"
                    ));
                }
            }
            Resolved::Missing => out.error(format!(
                "{label}: build.{path}: {library} has no component {id}"
            )),
            Resolved::Unknown => out.warn(format!(
                "{label}: build.{path}: the library {library} is not fetched here, so {id} was \
                 not checked"
            )),
        }
    }
}

// ---------------------------------------------------------------------
// Fitting parts to what the BUILD tab asks for.

/// How a part fits.
#[derive(Debug, Clone, PartialEq)]
pub enum Fit {
    /// It fits; a higher score fits more closely.
    Fits(u32),
    /// It nearly fits (one thing is wrong): the reason, for a greyed row.
    Near(String),
    /// It is not what was asked for.
    No,
}

/// The systems a junction's profiles have: its `systems` (a list, or `a`
/// and `b`), else its sides' own `system`.
fn junction_systems(junction: &Value) -> Vec<System> {
    let mut values: Vec<&Value> = Vec::new();
    match junction.get("systems") {
        Some(Value::Array(list)) => values.extend(list.iter()),
        Some(Value::Object(sides)) => values.extend(sides.values()),
        _ => {
            for side in ["a", "b"] {
                if let Some(system) = junction.get(side).and_then(|s| s.get("system")) {
                    values.push(system);
                }
            }
        }
    }
    let mut out: Vec<System> = Vec::new();
    for value in values {
        if let Ok(system) = System::from_value(value)
            && !out.contains(&system)
        {
            out.push(system);
        }
    }
    out
}

fn systems_text(systems: &[System]) -> String {
    systems
        .iter()
        .map(System::text)
        .collect::<Vec<_>>()
        .join(" or ")
}

/// How a connector fits a junction (`build_junctions`' entry): its
/// `junctions`, `systems` and `space` against the junction's `kind`, the
/// systems of its profiles and its free room.
pub fn fit_junction(component: &ManifestComponent, junction: &Value) -> Fit {
    let Some(part) = BuildPart::of(component) else {
        return Fit::No;
    };
    // Timber (mitcad#478): a junction of two members (with `wood`) takes
    // wood fasteners and outer connectors only; a junction of slot
    // profiles never takes them.
    if let Some(wood) = junction.get("wood").filter(|w| w.is_object()) {
        return fit_wood_junction(&part, junction, wood);
    }
    if part.is_timber_family() {
        return Fit::No;
    }
    if part.role() != "connector" {
        return Fit::No;
    }
    let connector = part.block("connector");
    let mut reasons = Vec::new();
    let mut score = 0;

    let kind = junction.get("kind").and_then(Value::as_str).unwrap_or("");
    let kinds: Vec<&str> = connector
        .and_then(|c| c.get("junctions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if kinds.contains(&kind) {
        if kinds.len() == 1 {
            score += 1;
        }
    } else {
        reasons.push(format!(
            "it goes on {} junctions, not on {}",
            if kinds.is_empty() {
                "no".to_owned()
            } else {
                kinds.join(", ")
            },
            if kind.is_empty() { "this one" } else { kind }
        ));
    }

    let made_for = part.systems();
    let wanted = junction_systems(junction);
    let unfitted: Vec<&System> = wanted
        .iter()
        .filter(|w| !made_for.iter().any(|m| m.fits(w)))
        .collect();
    if unfitted.is_empty() {
        if !wanted.is_empty()
            && wanted
                .iter()
                .all(|w| made_for.iter().any(|m| m.fits(w) && m.same_grid(w)))
        {
            score += 2;
        }
    } else {
        reasons.push(format!(
            "it is made for {}, not {}",
            if made_for.is_empty() {
                "no slot system".to_owned()
            } else {
                systems_text(&made_for)
            },
            unfitted
                .iter()
                .map(|s| s.text())
                .collect::<Vec<_>>()
                .join(" and ")
        ));
    }

    let space = connector
        .and_then(|c| c.get("space"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if space == "inner_corner"
        && let Some(room) = junction.get("space").filter(|s| s.is_object())
    {
        let axes = ["x", "y", "z"];
        let free: Vec<Option<f64>> = axes
            .iter()
            .map(|a| room.get(*a).and_then(|v| quantity(v, Kind::Length).ok()))
            .collect();
        if free.iter().flatten().any(|v| *v <= 0.0) {
            reasons.push("there is no room in the inner corner".to_owned());
        } else if let Some(envelope) = connector
            .and_then(|c| c.get("envelope"))
            .filter(|e| e.is_object())
        {
            let needed: Vec<Option<f64>> = axes
                .iter()
                .map(|a| {
                    envelope
                        .get(*a)
                        .and_then(|v| quantity(v, Kind::Length).ok())
                })
                .collect();
            let short = needed
                .iter()
                .zip(&free)
                .any(|(n, f)| matches!((n, f), (Some(n), Some(f)) if n > f));
            if short {
                let text = |values: &[Option<f64>]| {
                    values
                        .iter()
                        .map(|v| v.map_or("-".to_owned(), trim_number))
                        .collect::<Vec<_>>()
                        .join(" x ")
                };
                reasons.push(format!(
                    "it needs {} mm in the inner corner, which has {} mm",
                    text(&needed),
                    text(&free)
                ));
            }
        }
    }

    match reasons.len() {
        0 => Fit::Fits(score),
        1 => Fit::Near(reasons.remove(0)),
        _ => Fit::No,
    }
}

/// What a picker asks for (Profile, Panel, Linear Component; or a part to go
/// with another, as a carriage with its rail).
#[derive(Debug, Clone, Default)]
pub struct Picker<'a> {
    /// The part's role.
    pub role: Option<&'a str>,
    /// The slot system it is placed on.
    pub system: Option<System>,
    /// A linear part's `linear.kind` (`carriage`).
    pub kind: Option<&'a str>,
    /// A linear part's `linear.size`: what it goes with (a rail's size, a
    /// shaft's diameter, a spindle, a belt's profile).
    pub size: Option<&'a Value>,
    /// A panel's `panel.mount` (`face`: the sheets laid on members).
    pub mount: Option<&'a str>,
}

impl Picker<'_> {
    /// Whether it asks for anything only BUILD parts have.
    pub fn is_empty(&self) -> bool {
        self.role.is_none()
            && self.system.is_none()
            && self.kind.is_none()
            && self.size.is_none()
            && self.mount.is_none()
    }
}

/// Whether two sizes are the same: as lengths when both are (`8`, `"8
/// mm"`), else as text without case.
pub fn same_size(a: &Value, b: &Value) -> bool {
    if let (Ok(x), Ok(y)) = (quantity(a, Kind::Length), quantity(b, Kind::Length)) {
        return (x - y).abs() < 1e-6;
    }
    match (size_text(a), size_text(b)) {
        (Some(x), Some(y)) => x.trim().eq_ignore_ascii_case(y.trim()),
        _ => false,
    }
}

fn size_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// How a part fits a picker: the role and a linear part's kind must be
/// the same; a part made for no system fits any system, and one without
/// a size any size. One thing wrong (the system, the size) is a near fit.
pub fn fit_picker(component: &ManifestComponent, picker: &Picker<'_>) -> Fit {
    let Some(part) = BuildPart::of(component) else {
        return Fit::No;
    };
    if picker.role.is_some_and(|r| r != part.role()) {
        return Fit::No;
    }
    let linear = part.block("linear");
    // A picker's kind is a linear part's, or of timber (mitcad#478):
    // `timber` (members), `wood` (wood fasteners), `outer`, `angle`,
    // `hanger`, `post_base` (outer connectors). Timber parts are offered
    // only when asked for (by that kind, or sheets by `mount: face`): the
    // slot profiles' pickers list none.
    if let Some(kind) = picker.kind {
        if !part.kinds().contains(&kind) {
            return Fit::No;
        }
    } else if part.is_timber_family() && !(picker.mount == Some("face") && part.is_face_sheet()) {
        return Fit::No;
    }
    if let Some(mount) = picker.mount
        && part
            .block("panel")
            .and_then(|p| p.get("mount"))
            .and_then(Value::as_str)
            != Some(mount)
    {
        return Fit::No;
    }
    let mut reasons = Vec::new();
    let mut score = 0;
    // The wood fastener a library marks as the framing default
    // (`wood.default`, mitcad#516) comes first in the wood picker.
    if picker.kind == Some("wood")
        && part
            .block("wood")
            .and_then(|w| w.get("default"))
            .and_then(Value::as_bool)
            == Some(true)
    {
        score += 1;
    }
    if let Some(wanted) = &picker.system {
        let made_for = part.systems();
        if !made_for.is_empty() {
            match made_for.iter().find(|m| m.fits(wanted)) {
                Some(m) => score += u32::from(m.same_grid(wanted)),
                None => reasons.push(format!(
                    "it is made for {}, not {}",
                    systems_text(&made_for),
                    wanted.text()
                )),
            }
        }
    }
    if let Some(wanted) = picker.size
        && let Some(size) = linear.and_then(|l| l.get("size")).filter(|s| !s.is_null())
    {
        if same_size(size, wanted) {
            score += 2;
        } else {
            reasons.push(format!(
                "it is for {}, not {}",
                size_text(size).unwrap_or_default(),
                size_text(wanted).unwrap_or_default()
            ));
        }
    }
    match reasons.len() {
        0 => Fit::Fits(score),
        1 => Fit::Near(reasons.remove(0)),
        _ => Fit::No,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn values_and_systems_are_read() {
        assert_eq!(quantity(&json!("9 mm"), Kind::Length), Ok(9.0));
        assert_eq!(quantity(&json!("6 m"), Kind::Length), Ok(6000.0));
        assert_eq!(quantity(&json!(12.5), Kind::Length), Ok(12.5));
        assert!((quantity(&json!("900 g"), Kind::Mass).unwrap() - 0.9).abs() < 1e-12);
        assert_eq!(quantity(&json!("1e3 mm"), Kind::Length), Ok(1000.0));
        assert!(quantity(&json!("9 kg"), Kind::Length).is_err());
        assert!(quantity(&json!("nine"), Kind::Length).is_err());
        let b8 = System::from_value(&json!({"type": "B", "slot": 8, "grid": 30})).unwrap();
        assert_eq!(System::from_value(&json!("B/8/30")).unwrap(), b8);
        assert_eq!(b8.text(), "B-type slot 8 (30)");
        let any_b8 = System::from_value(&json!("b8")).unwrap();
        assert!(any_b8.fits(&b8) && b8.fits(&any_b8));
        assert!(!b8.fits(&System::from_value(&json!("B/8/40")).unwrap()));
        assert!(!b8.fits(&System::from_value(&json!("I/8/30")).unwrap()));
        assert!(System::from_value(&json!("X/8")).is_err());
        assert_eq!(slot_frame("slot_x+_2"), Some(("x+", 2)));
        assert_eq!(slot_frame("slot_z+_1"), None);
        assert_eq!(slot_frame("slot_x+_0"), None);
        assert!(mentions("L + 2 mm", "L") && !mentions("La + 2", "L"));
    }
}
