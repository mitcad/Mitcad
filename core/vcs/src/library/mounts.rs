// SPDX-License-Identifier: MIT
//! Mount frames of library parts that are no BUILD parts (mitcad#525):
//! the manifest's `mounts` block, what `library_add` writes into it from
//! the design, and what `library check` says about it. The format is in
//! `docs/libraries.md`, *Mount frames*; the model's side (the joint
//! origins' `mount`, placing a part by one) in
//! `core/model-api/src/commands.md`, "Mount frames".
//!
//! The design is what counts: the model places a part by the joint
//! origins of its design. The manifest's block tells the application which
//! parts have frames, and of what kind, before it reads a design (a
//! placement by a frame, or a drop-in for a part without).
//!
//! ```json
//! "mounts": [{"name": "foot", "kind": "foot", "joint": "rigid", "footprint": [1, 1],
//!             "pitch": [42, 42]}]
//! ```

use std::collections::{BTreeMap, BTreeSet};

use mitcad_model::{MountKind, MountSpec};
use serde_json::{Value, json};

use super::build::{Kind, MOUNT_PREFIX, quantity, valid_frame_name};
use super::manifest::{ManifestComponent, Problems};

/// The kinds of mount frames, as the manifest names them.
pub const MOUNT_KINDS: [&str; 5] = ["face", "hole", "shaft", "grid", "foot"];

fn kind_of(text: &str) -> Option<MountKind> {
    MountKind::ALL.into_iter().find(|k| k.as_str() == text)
}

fn list(items: &[&str]) -> String {
    items.join(", ")
}

/// The design's mount frames (its file's JSON): the joint origins with a
/// `mount`, else named `mount:<name>` (a face frame), each name once, in
/// the timeline's order; suppressed ones left out. A `mount` that cannot be
/// read is given as it is, for the check to name.
pub fn design_mounts(design: &Value) -> Vec<(String, Value)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for feature in design
        .get("features")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if feature.get("type").and_then(Value::as_str) != Some("joint_origin")
            || feature.get("suppressed").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        let entry = match feature.get("mount").filter(|m| !m.is_null()) {
            Some(mount) => mount.clone(),
            None => match feature
                .get("name")
                .and_then(Value::as_str)
                .and_then(|n| n.strip_prefix(MOUNT_PREFIX))
            {
                Some(name) => json!({"name": name, "kind": "face"}),
                None => continue,
            },
        };
        let Some(name) = entry.get("name").and_then(Value::as_str).map(str::to_owned) else {
            continue;
        };
        if seen.insert(name.clone()) {
            out.push((name, entry));
        }
    }
    out
}

/// The manifest's `mounts` of a design: its frames with their kinds and
/// joints (`joint` written also when rigid). None when it has none.
pub fn manifest_mounts(design: &Value) -> Option<Value> {
    let entries: Vec<Value> = design_mounts(design)
        .into_iter()
        .map(
            |(_, mount)| match serde_json::from_value::<MountSpec>(mount.clone()) {
                Ok(spec) => {
                    let mut value = json!({"name": spec.name, "kind": spec.kind.as_str(),
                                       "joint": spec.joint.as_str()});
                    if let Some(pitch) = spec.pitch {
                        value["pitch"] = json!(pitch);
                    }
                    if let Some(cells) = spec.cells {
                        value["cells"] = json!(cells);
                    }
                    if let Some(footprint) = spec.footprint {
                        value["footprint"] = json!(footprint);
                    }
                    value
                }
                Err(_) => mount,
            },
        )
        .collect();
    (!entries.is_empty()).then_some(Value::Array(entries))
}

/// Two numbers each above 0 (a grid's pitch, mm: numbers or text with a
/// unit).
fn pitch_of(value: &Value) -> Result<[f64; 2], String> {
    let items = value
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("is no pair of lengths")?;
    let mut out = [0.0; 2];
    for (i, item) in items.iter().enumerate() {
        let v = quantity(item, Kind::Length)?;
        if !(v > 0.0 && v.is_finite()) {
            return Err(format!("[{i}] is {v} mm; it must be above 0"));
        }
        out[i] = v;
    }
    Ok(out)
}

/// Two whole numbers each 1 or more (cells, a footprint).
fn counts_of(value: &Value) -> Result<[u32; 2], String> {
    let items = value
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("is no pair of counts")?;
    let mut out = [0; 2];
    for (i, item) in items.iter().enumerate() {
        match item.as_u64() {
            Some(n) if n >= 1 && n <= u64::from(u32::MAX) => out[i] = n as u32,
            _ => return Err(format!("[{i}] is no count of 1 or more")),
        }
    }
    Ok(out)
}

/// A manifest entry read, its problems told.
fn entry_of(
    entry: &Value,
    i: usize,
    label: &str,
    out: &mut Problems,
) -> Option<(String, MountSpec)> {
    let Some(name) = entry.get("name").and_then(Value::as_str) else {
        out.error(format!("{label}: mounts[{i}] has no name"));
        return None;
    };
    if !valid_frame_name(name) {
        out.error(format!(
            "{label}: mount frame “{name}” is no frame name (letters, digits, _ - + .)"
        ));
        return None;
    }
    let kind_text = entry.get("kind").and_then(Value::as_str).unwrap_or("face");
    let Some(kind) = kind_of(kind_text) else {
        out.error(format!(
            "{label}: mount frame {name}: kind {kind_text} is none of {}",
            list(&MOUNT_KINDS)
        ));
        return None;
    };
    let joint_text = entry
        .get("joint")
        .and_then(Value::as_str)
        .unwrap_or("rigid");
    let joint = mitcad_model::joints::JointKind::ALL
        .into_iter()
        .find(|j| j.as_str() == joint_text);
    let Some(joint) = joint else {
        out.error(format!(
            "{label}: mount frame {name}: joint {joint_text} is no joint kind"
        ));
        return None;
    };
    let mut spec = MountSpec::new(name, kind);
    spec.joint = joint;
    let mut ok = true;
    if let Some(pitch) = entry.get("pitch").filter(|v| !v.is_null()) {
        match pitch_of(pitch) {
            Ok(p) => spec.pitch = Some(p),
            Err(e) => {
                out.error(format!("{label}: mount frame {name}: pitch {e}"));
                ok = false;
            }
        }
    }
    for (key, slot) in [
        ("cells", &mut spec.cells),
        ("footprint", &mut spec.footprint),
    ] {
        if let Some(value) = entry.get(key).filter(|v| !v.is_null()) {
            match counts_of(value) {
                Ok(c) => *slot = Some(c),
                Err(e) => {
                    out.error(format!("{label}: mount frame {name}: {key} {e}"));
                    ok = false;
                }
            }
        }
    }
    if !ok {
        return None;
    }
    if let Err(e) = spec.check() {
        out.error(format!("{label}: mount frame {name}: {e}"));
        return None;
    }
    Some((name.to_owned(), spec))
}

/// What is wrong with a component's `mounts` by itself (the design it
/// names is checked with the folder, [`check_design`]).
pub fn check_component(component: &ManifestComponent, label: &str, out: &mut Problems) {
    let Some(mounts) = &component.mounts else {
        return;
    };
    let Some(entries) = mounts.as_array() else {
        out.error(format!("{label}: mounts is no list"));
        return;
    };
    let mut names = BTreeSet::new();
    for (i, entry) in entries.iter().enumerate() {
        if let Some((name, _)) = entry_of(entry, i, label, out)
            && !names.insert(name.clone())
        {
            out.error(format!("{label}: mount frame {name} is listed twice"));
        }
    }
}

/// Whether two mounts say the same (the manifest's and the design's).
fn same(a: &MountSpec, b: &MountSpec) -> bool {
    let pitch = match (a.pitch, b.pitch) {
        (Some(x), Some(y)) => (x[0] - y[0]).abs() < 1e-6 && (x[1] - y[1]).abs() < 1e-6,
        (None, None) => true,
        _ => false,
    };
    a.kind == b.kind
        && a.joint == b.joint
        && a.cells == b.cells
        && a.footprint == b.footprint
        && pitch
}

fn describe(spec: &MountSpec) -> String {
    let mut text = format!("{} ({})", spec.kind.as_str(), spec.joint.as_str());
    if let Some(cells) = spec.cells {
        text.push_str(&format!(", {} × {} cells", cells[0], cells[1]));
    }
    if let Some(footprint) = spec.footprint {
        text.push_str(&format!(", {} × {} footprint", footprint[0], footprint[1]));
    }
    if let Some(pitch) = spec.pitch {
        text.push_str(&format!(", pitch {} × {} mm", pitch[0], pitch[1]));
    }
    text
}

/// What is wrong with the design a component's `mounts` names (its file's
/// JSON): every listed frame is a mount frame of the design and says what
/// the design's says, and every mount of the design is a sound one; a
/// frame of the design the block does not list is a warning (for a part
/// without `build`, whose frames are `build.frames`).
pub fn check_design(
    component: &ManifestComponent,
    label: &str,
    design: &Value,
    out: &mut Problems,
) {
    let path = &component.path;
    let mut designed: BTreeMap<String, Option<MountSpec>> = BTreeMap::new();
    for (name, mount) in design_mounts(design) {
        match serde_json::from_value::<MountSpec>(mount) {
            Ok(spec) => {
                if let Err(e) = spec.check() {
                    out.error(format!("{label}: {path}: the mount frame {name}: {e}"));
                }
                designed.insert(name, Some(spec));
            }
            Err(e) => {
                out.error(format!(
                    "{label}: {path}: the mount of the frame {name} cannot be read: {e}"
                ));
                designed.insert(name, None);
            }
        }
    }
    let mut listed = BTreeSet::new();
    let mut ignored = Problems::default();
    for (i, entry) in component
        .mounts
        .as_ref()
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        // The entry's own problems are the manifest's (check_component).
        let Some((name, spec)) = entry_of(entry, i, label, &mut ignored) else {
            continue;
        };
        listed.insert(name.clone());
        match designed.get(&name) {
            None => out.error(format!(
                "{label}: {path}: the mount frame {name} is no joint origin of the design with \
                 that mount (or named {MOUNT_PREFIX}{name})"
            )),
            Some(Some(own)) if !same(own, &spec) => out.error(format!(
                "{label}: {path}: the mount frame {name} is {} in the manifest and {} in the \
                 design",
                describe(&spec),
                describe(own)
            )),
            Some(_) => {}
        }
    }
    if component.build.is_none() {
        for name in designed.keys() {
            if !listed.contains(name) {
                out.warn(format!(
                    "{label}: {path}: the mount frame {name} of the design is not in mounts"
                ));
            }
        }
    }
}
