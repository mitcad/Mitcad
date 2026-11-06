// SPDX-License-Identifier: MIT
//! Joints (SolidCraft's `crates/doc/src/joints.rs`) as MitCad joint
//! features of the root, after the timeline.
//!
//! - Kinds: rigid, revolute, slider, cylindrical, pin slot, planar and
//!   ball are MitCad's; the relations that tie only some directions
//!   (parallel, perpendicular, angle, point on a plane or a line, line on
//!   a plane) have no counterpart and are listed.
//! - Origins: a face centre is the planar face at the pick (its middle and
//!   outward normal); a circle centre is the circular edge at the end of
//!   the cylindrical face at the pick nearer to it, z out of the face's
//!   end; a frame or a point is a fixed point with its axes; an edge point
//!   the straight edge's middle. Picks are in the occurrence's component's
//!   coordinates.
//! - SolidCraft mates the origins (B's z against A's) unless `flip`;
//!   MitCad's z axes point the same way unless `flip`: the flip is turned
//!   over. The joint's values are its position, its limits MitCad's.
//! - The occurrences keep the placements the file gives them: a joint that
//!   does not hold there (frames found otherwise than SolidCraft finds
//!   them) comes in as an as-built joint of the placements, with a note.
//! - Contact sets and motion studies (SolidCraft's `motion.rs`) are
//!   MitCad's (mitcad#454): see [`contact_sets`] and [`motion_studies`].
//!   Motion links are MitCad's `motion_link` features (mitcad#466, see
//!   [`motion_links`]); exploded views are listed as not mapped.

use mitcad_model::features::OccurrencePath;
use mitcad_model::joints::Motion;
use mitcad_model::{
    ComponentUid, ContactSet, FeatureDef, FeatureUid, Kernel, MotionStudy, StudyKey, Transform,
};
use serde_json::{Value, json};

use super::Importer;
use super::report::{JointReport, Outcome};
use mitcad_import_base::geom::{distance, dot, scale, sub};

/// SolidCraft's names of a kind's values, and MitCad's motions for them.
fn motions(kind: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match kind {
        "rigid" => ("rigid", &[]),
        "revolute" => ("revolute", &["rz"]),
        "slider" => ("slider", &["tz"]),
        "cylindrical" => ("cylindrical", &["rz", "tz"]),
        "pin_slot" => ("pin_slot", &["rz", "tx"]),
        "planar" => ("planar", &["tx", "ty", "rz"]),
        "ball" => ("ball", &["rz", "ry", "rx"]),
        _ => return None,
    })
}

pub(super) fn joints<K: Kernel>(im: &mut Importer<'_, '_, K>) {
    let joints: Vec<Value> = im.design.joints().to_vec();
    for j in joints {
        let report = joint(im, &j);
        im.report.joints.push(report);
    }
    contact_sets(im);
    motion_studies(im);
    motion_links(im);
    // Exploded views: not mapped yet; the reader keeps them.
    let n = im.design.exploded_views().len();
    if n > 0 {
        im.report.notes.push(format!(
            "{n} exploded views of the assembly: not mapped yet"
        ));
    }
}

/// The file's joints that came in as joint features (or as-built joints),
/// and their motions in SolidCraft's order of values.
fn joint_features<K: Kernel>(
    im: &Importer<'_, '_, K>,
    as_built: bool,
) -> std::collections::HashMap<u64, (FeatureUid, &'static [&'static str])> {
    let mut joints = std::collections::HashMap::new();
    for report in &im.report.joints {
        let Some(uid) = report
            .feature
            .as_deref()
            .and_then(|f| f.parse::<FeatureUid>().ok())
        else {
            continue;
        };
        let fits = match im.doc.feature(uid).map(|f| &f.def) {
            Some(FeatureDef::Joint(_)) => true,
            Some(FeatureDef::AsBuiltJoint(_)) => as_built,
            _ => false,
        };
        if let (true, Some((_, dofs))) = (fits, motions(&report.kind)) {
            joints.insert(report.id, (uid, dofs));
        }
    }
    joints
}

/// The file's motion links (`joints.rs`, `MotionLink`) as MitCad's
/// `motion_link` features (mitcad#466): b's value follows a's by the
/// coupling (a plain ratio without one; gears, a rack and pinion and a
/// screw by their sizes as numbers, expressions or picked round faces; a
/// cam by its profile, follower and tip). The offset is MitCad's own, the
/// one that keeps the joints where the file places the parts (MitCad's
/// joint values count from MitCad's frames). A link whose joints did not
/// come in, or that does not fit, is left out with a note.
fn motion_links<K: Kernel>(im: &mut Importer<'_, '_, K>) {
    let joints = joint_features(im, true);
    let links: Vec<Value> = im.design.motion_links().to_vec();
    for (i, l) in links.iter().enumerate() {
        let coupling = l
            .get("coupling")
            .and_then(|c| c.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("ratio")
            .to_owned();
        let mut report = JointReport {
            id: i as u64 + 1,
            name: format!("link {}", i + 1),
            kind: coupling,
            outcome: Outcome::Parametric,
            feature: None,
            notes: Vec::new(),
        };
        match link(im, &joints, l) {
            Ok((uid, notes)) => {
                report.feature = Some(uid.to_string());
                report.name = im
                    .doc
                    .feature(uid)
                    .map_or_else(|| report.name.clone(), |f| f.name.clone());
                if !notes.is_empty() {
                    report.outcome = Outcome::Partial;
                }
                report.notes = notes;
            }
            Err(e) => {
                report.outcome = Outcome::Failed;
                im.report
                    .notes
                    .push(format!("motion {} did not come in: {e}", report.name));
                report.notes.push(e);
            }
        }
        im.report.links.push(report);
    }
}

/// One motion link: the feature added, and notes.
fn link<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    joints: &std::collections::HashMap<u64, (FeatureUid, &'static [&'static str])>,
    l: &Value,
) -> Result<(FeatureUid, Vec<String>), String> {
    let side = |key: &str, index: &str| -> Result<(u64, Value), String> {
        let id = l.get(key).and_then(Value::as_u64).unwrap_or(0);
        let i = l.get(index).and_then(Value::as_u64).unwrap_or(0) as usize;
        let (uid, dofs) = joints
            .get(&id)
            .ok_or_else(|| format!("its joint {id} did not come in as a joint"))?;
        let motion = dofs
            .get(i)
            .ok_or_else(|| format!("joint {id} has no moving value {i}"))?;
        Ok((id, json!({"joint": uid.to_string(), "motion": motion})))
    };
    let (a_id, a) = side("a", "ia")?;
    let (_, b) = side("b", "ib")?;
    let notes = Vec::new();
    let c = l.get("coupling").cloned().unwrap_or(Value::Null);
    let flag = |k: &str| c.get(k).and_then(Value::as_bool).unwrap_or(false);
    let reverse = flag("reverse");
    let coupling = match c.get("type").and_then(Value::as_str) {
        None => {
            let ratio = l.get("ratio").and_then(Value::as_f64).unwrap_or(1.0);
            json!({"type": "ratio", "ratio": ratio})
        }
        Some("gear") => json!({"type": "gear", "ra": size(im, &c["ra"])?,
                               "rb": size(im, &c["rb"])?, "internal": flag("internal")}),
        Some("rack_pinion") => json!({"type": "rack_pinion", "radius": size(im, &c["radius"])?}),
        Some("screw") => json!({"type": "screw", "lead": size(im, &c["pitch"])?}),
        Some("cam") => cam(im, &c, a_id)?,
        Some(other) => return Err(format!("a coupling of the kind `{other}`")),
    };
    let def = json!({"a": a, "b": b, "coupling": coupling, "reverse": reverse});
    let def: mitcad_model::features::MotionLinkDef<mitcad_model::ValueInput> =
        serde_json::from_value(def).map_err(|e| format!("definition: {e}"))?;
    let before: Vec<(mitcad_model::OccurrenceUid, Option<Transform>)> = im
        .occurrences
        .values()
        .map(|o| (*o, im.doc.placement(*o)))
        .collect();
    let added = im
        .doc
        .add_motion_link(&def, Some(true), None, Some(ComponentUid::ROOT))
        .map_err(|e| e.to_string())?;
    let failed = im
        .doc
        .status(added.uid)
        .and_then(|s| s.error())
        .map(str::to_owned);
    let moved = before.iter().any(|(o, t)| match (t, im.doc.placement(*o)) {
        (Some(a), Some(b)) => !same(a, &b),
        _ => false,
    });
    if let Some(e) = failed {
        im.doc.undo();
        return Err(e);
    }
    if moved {
        im.doc.undo();
        return Err("it would move the parts from where the file places them".to_owned());
    }
    im.params.adopt(im.doc, &added);
    Ok((added.uid, notes))
}

/// A coupling's size (`Size`): millimetres, an expression, or a picked
/// round face (a joint origin of a circle centre).
fn size<K: Kernel>(im: &mut Importer<'_, '_, K>, s: &Value) -> Result<Value, String> {
    match s {
        Value::Number(n) => Ok(json!(format!("{} mm", n.as_f64().unwrap_or(0.0)))),
        Value::String(e) => Ok(json!(im.params.expression(im.doc, e)?)),
        Value::Object(_) => {
            let mut notes = Vec::new();
            let o = origin(im, s, &mut notes)?;
            // The round face (or its circular edge) is the size; the
            // frame's parts are not.
            Ok(json!({"occurrence": o["occurrence"], "geometry": o["geometry"]}))
        }
        _ => Err("a coupling without its size".to_owned()),
    }
}

/// A cam coupling (`joints_cam.rs`, `Cam`): its profile as a sketch's
/// curves or the planar face its picked edges bound, its follower and tip.
fn cam<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    c: &Value,
    cam_joint: u64,
) -> Result<Value, String> {
    let p = &c["profile"];
    let profile = match p.get("type").and_then(Value::as_str).unwrap_or("") {
        "sketch" => {
            let id = p.get("sketch").and_then(Value::as_u64).unwrap_or(0);
            let made = im
                .sketches
                .get(&id)
                .ok_or_else(|| format!("the cam's sketch {id} did not come in"))?;
            let curves: Vec<String> = p
                .get("curves")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(|c| made.curve(c).ok())
                .collect();
            let uid = made.uid;
            let component = im
                .doc
                .feature(uid)
                .map_or(ComponentUid::ROOT, |f| f.component);
            // The side of the cam joint in the sketch's component.
            let path = cam_side(im, cam_joint, |_, c| c == component)?;
            json!({"occurrence": path, "geometry": {"sketch": uid.to_string(), "curves": curves}})
        }
        "edges" => {
            let occurrence = p.get("occurrence").and_then(Value::as_u64).unwrap_or(0);
            let (path, component) = occurrence_path(im, occurrence)?;
            let vec3 = |v: &Value| -> Option<[f64; 3]> {
                let a = v.as_array()?;
                Some([
                    a.first()?.as_f64()?,
                    a.get(1)?.as_f64()?,
                    a.get(2)?.as_f64()?,
                ])
            };
            let picks: Vec<[f64; 3]> = p
                .get("picks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(vec3)
                .collect();
            let (body, face) = profile_face(im, component, &picks)?;
            json!({"occurrence": path, "geometry": {"body": body.to_string(), "face": face}})
        }
        other => return Err(format!("a cam profile of the kind `{other}`")),
    };
    let f = &c["follower"];
    let follower = match f.get("type").and_then(Value::as_str).unwrap_or("knife") {
        "knife" => json!({"type": "knife"}),
        "flat" => json!({"type": "flat"}),
        "roller" => json!({"type": "roller", "radius": size(im, &f["radius"])?}),
        other => return Err(format!("a cam follower of the kind `{other}`")),
    };
    let mut out = json!({"type": "cam", "profile": profile, "follower": follower});
    if let Some(tip) = c.get("tip").filter(|t| !t.is_null()) {
        let mut notes = Vec::new();
        out["tip"] = origin(im, tip, &mut notes)?;
    }
    Ok(out)
}

/// The occurrence path of the side of a file's joint that `fits`.
fn cam_side<K: Kernel>(
    im: &Importer<'_, '_, K>,
    joint: u64,
    fits: impl Fn(&str, ComponentUid) -> bool,
) -> Result<String, String> {
    let j = im
        .design
        .joints()
        .iter()
        .find(|j| j.get("id").and_then(Value::as_u64) == Some(joint))
        .ok_or_else(|| format!("no joint {joint}"))?;
    for key in ["b", "a"] {
        let o = j[key]
            .get("occurrence")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let (path, component) = occurrence_path(im, o)?;
        if fits(&path, component) {
            return Ok(path);
        }
    }
    Err("the cam's profile must be on a part its joint turns".to_owned())
}

/// The planar face of a cam's body whose boundary has the picked edges.
fn profile_face<K: Kernel>(
    im: &Importer<'_, '_, K>,
    component: ComponentUid,
    picks: &[[f64; 3]],
) -> Result<(mitcad_model::BodyUid, String), String> {
    let first = picks.first().ok_or("a cam profile without picked edges")?;
    let (body, edge) = im.edge_at(component, None, *first)?;
    let kernel = im.doc.kernel();
    let shape = im.doc.body_shape(body).ok_or("the cam's body is gone")?;
    let faces = kernel.faces(shape).map_err(|e| e.to_string())?;
    let mut best: Option<String> = None;
    for face in faces {
        let Some(name) = face.names.first() else {
            continue;
        };
        if !edge.contains(name.as_str()) {
            continue;
        }
        let Ok(parsed) = name.parse::<mitcad_model::FaceName>() else {
            continue;
        };
        let Ok(mitcad_model::datum::SurfaceGeometry::Plane { origin, normal }) =
            kernel.face_geometry(shape, &parsed)
        else {
            continue;
        };
        if picks
            .iter()
            .all(|p| dot(sub(*p, origin), normal).abs() <= 1e-4)
        {
            best = Some(name.clone());
            break;
        }
    }
    best.map(|f| (body, f))
        .ok_or_else(|| "the cam's picked edges bound no planar face".to_owned())
}

/// The file's contact sets as MitCad's (`motion.rs`, `ContactSet`): each
/// set's occurrences by their paths from the root, suppressed as in the
/// file; with contact off in the file every set comes in suppressed, and
/// "all contact" (every occurrence against every other) is a set of every
/// occurrence.
fn contact_sets<K: Kernel>(im: &mut Importer<'_, '_, K>) {
    let (enabled, all) = im.design.contact();
    let mut sets: Vec<(String, Vec<u64>, bool)> = im
        .design
        .contact_sets()
        .iter()
        .map(|s| {
            let name = s
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let occurrences = s
                .get("occurrences")
                .and_then(Value::as_array)
                .map(|o| o.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default();
            let suppressed = s
                .get("suppressed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            (name, occurrences, suppressed || !enabled || all)
        })
        .collect();
    if all {
        let every = im.design.occurrences.iter().map(|o| o.id).collect();
        sets.push(("All Contact".to_owned(), every, !enabled));
    }
    if !sets.is_empty() && !enabled {
        im.report
            .notes
            .push("contact was off in the file: its contact sets came in suppressed".to_owned());
    }
    for (name, occurrences, suppressed) in sets {
        let label = if name.is_empty() {
            "a contact set".to_owned()
        } else {
            format!("contact set {name}")
        };
        let mut paths = Vec::new();
        for id in occurrences {
            match occurrence_path(im, id).and_then(|(path, _)| {
                serde_json::from_value::<OccurrencePath>(json!(path)).map_err(|e| e.to_string())
            }) {
                Ok(path) if !path.is_empty() && !paths.contains(&path) => paths.push(path),
                Ok(_) => {}
                Err(e) => im
                    .report
                    .notes
                    .push(format!("{label}: occurrence {id} left out: {e}")),
            }
        }
        // MitCad's names are its own: a taken name gets the next number.
        let mut unique = name.trim().to_owned();
        if !unique.is_empty() {
            let base = unique.clone();
            let mut n = 2;
            while im.doc.contact_sets().iter().any(|s| s.name == unique) {
                unique = format!("{base} ({n})");
                n += 1;
            }
        }
        let set = ContactSet {
            name: unique,
            occurrences: paths,
            suppressed,
        };
        if let Err(e) = im.doc.add_contact_set(set) {
            im.report
                .notes
                .push(format!("{label} did not come in: {e}"));
        }
    }
}

/// The file's motion studies as MitCad's (`motion.rs`, `MotionStudy`):
/// a key's joint is the joint feature the file's joint became, its value
/// index the kind's motion in SolidCraft's order (`motions`); values are
/// in millimetres and radians in both. Keys of joints that came in as
/// as-built joints (whose values count from the recorded placement) or
/// not at all are left out, with a note.
fn motion_studies<K: Kernel>(im: &mut Importer<'_, '_, K>) {
    // The file's joints that came in as joint features, and their motions.
    let mut joints: std::collections::HashMap<u64, (FeatureUid, &'static [&'static str])> =
        std::collections::HashMap::new();
    for report in &im.report.joints {
        let Some(uid) = report
            .feature
            .as_deref()
            .and_then(|f| f.parse::<FeatureUid>().ok())
        else {
            continue;
        };
        if !matches!(
            im.doc.feature(uid).map(|f| &f.def),
            Some(FeatureDef::Joint(_))
        ) {
            continue;
        }
        if let Some((_, dofs)) = motions(&report.kind) {
            joints.insert(report.id, (uid, dofs));
        }
    }
    let studies: Vec<Value> = im.design.motion_studies().to_vec();
    for s in studies {
        let name = s
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        let label = if name.is_empty() {
            "a motion study".to_owned()
        } else {
            format!("motion study {name}")
        };
        let steps = s
            .get("steps")
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .map_or(mitcad_model::motion::DEFAULT_STEPS, |n| {
                n.round()
                    .clamp(1.0, f64::from(mitcad_model::motion::MAX_STEPS)) as u32
            });
        let mut keys: Vec<StudyKey> = Vec::new();
        for k in s
            .get("keys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let joint = k.get("joint").and_then(Value::as_u64).unwrap_or(0);
            let index = k.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            let Some((uid, dofs)) = joints.get(&joint) else {
                im.report.notes.push(format!(
                    "{label}: the keys of joint {joint} left out (it did not come in as a joint)"
                ));
                continue;
            };
            let Some(motion) = dofs
                .get(index)
                .and_then(|m| serde_json::from_value::<Motion>(json!(m)).ok())
            else {
                im.report.notes.push(format!(
                    "{label}: joint {joint} has no moving value {index}"
                ));
                continue;
            };
            let mut points: Vec<(f64, f64)> = k
                .get("points")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
                .filter(|(s, v)| s.is_finite() && v.is_finite())
                .collect();
            points.sort_by(|a, b| a.0.total_cmp(&b.0));
            if points.is_empty() {
                continue;
            }
            match keys
                .iter_mut()
                .find(|key| key.joint == *uid && key.motion == motion)
            {
                Some(key) => {
                    for (s, v) in points {
                        key.set_point(s, v);
                    }
                }
                None => keys.push(StudyKey {
                    joint: *uid,
                    motion,
                    points,
                }),
            }
        }
        let mut unique = name.clone();
        if !unique.is_empty() {
            let mut n = 2;
            while im.doc.motion_studies().iter().any(|m| m.name == unique) {
                unique = format!("{name} ({n})");
                n += 1;
            }
        }
        let study = MotionStudy {
            name: unique,
            steps,
            keys,
        };
        if let Err(e) = im.doc.add_motion_study(study) {
            im.report
                .notes
                .push(format!("{label} did not come in: {e}"));
        }
    }
}

fn joint<K: Kernel>(im: &mut Importer<'_, '_, K>, j: &Value) -> JointReport {
    let id = j.get("id").and_then(Value::as_u64).unwrap_or(0);
    let name = j
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Joint")
        .to_owned();
    let kind = j
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut report = JointReport {
        id,
        name: name.clone(),
        kind: kind.clone(),
        outcome: Outcome::Parametric,
        feature: None,
        notes: Vec::new(),
    };
    let Some((mitcad_kind, dofs)) = motions(&kind) else {
        report.outcome = Outcome::Unmapped;
        report.notes.push(format!(
            "a {kind} relation (it ties only some directions or points) has no MitCad counterpart"
        ));
        im.report.notes.push(format!(
            "joint {name}: {kind} relations have no MitCad counterpart"
        ));
        return report;
    };
    let result = (|| -> Result<(Value, Vec<String>), String> {
        let mut notes = Vec::new();
        let a = origin(im, &j["a"], &mut notes)?;
        let b = origin(im, &j["b"], &mut notes)?;
        let exprs = &j["exprs"];
        let number = |v: &Value| v.as_f64().unwrap_or(0.0);
        let value_of = |im: &mut Importer<'_, '_, K>,
                        expr: Option<&str>,
                        n: f64,
                        angle: bool|
         -> Result<Value, String> {
            match expr {
                Some(e) => Ok(json!(im.params.expression(im.doc, e)?)),
                None if angle => Ok(json!(format!("{} deg", n.to_degrees()))),
                None => Ok(json!(format!("{n} mm"))),
            }
        };
        let offset = value_of(im, exprs["offset"].as_str(), number(&j["offset"]), false)?;
        let angle = value_of(im, exprs["angle"].as_str(), number(&j["angle"]), true)?;
        let mut position = serde_json::Map::new();
        let mut limits = serde_json::Map::new();
        for (i, motion) in dofs.iter().enumerate() {
            let turn = motion.starts_with('r');
            if let Some(v) = j["values"].get(i).and_then(Value::as_f64) {
                let e = exprs["values"].get(i).and_then(Value::as_str);
                position.insert((*motion).to_owned(), value_of(im, e, v, turn)?);
            }
            if let Some([min, max]) = j["limits"]
                .get(i)
                .and_then(|l| l.as_array())
                .map(|l| [l.first(), l.get(1)])
                && let (Some(min), Some(max)) =
                    (min.and_then(Value::as_f64), max.and_then(Value::as_f64))
            {
                limits.insert(
                    (*motion).to_owned(),
                    json!({"min": value_of(im, None, min, turn)?, "max": value_of(im, None, max, turn)?}),
                );
            }
        }
        let flip = !j["flip"].as_bool().unwrap_or(false);
        let def = json!({"type": "joint", "kind": mitcad_kind, "a": a, "b": b, "offset": offset, "angle": angle,
            "flip": flip, "position": position, "limits": limits});
        Ok((def, notes))
    })();
    let (def, notes) = match result {
        Ok(x) => x,
        Err(e) => {
            report.outcome = Outcome::Failed;
            report.notes.push(e);
            return report;
        }
    };
    report.notes.extend(notes);
    // The placements the file gives: the joint must hold there.
    let before: Vec<(mitcad_model::OccurrenceUid, Option<Transform>)> = im
        .occurrences
        .values()
        .map(|o| (*o, im.doc.placement(*o)))
        .collect();
    let moved = |im: &Importer<'_, '_, K>| {
        before.iter().any(|(o, t)| match (t, im.doc.placement(*o)) {
            (Some(a), Some(b)) => !same(a, &b),
            _ => false,
        })
    };
    // Driven at the file's values; else with its values free (MitCad's
    // frames may turn otherwise about z than SolidCraft's, so the same
    // placement has other values); else as built.
    let mut tries = vec![(def.clone(), None)];
    if def["position"].as_object().is_some_and(|p| !p.is_empty()) {
        let mut free = def.clone();
        free["position"] = json!({});
        tries.push((
            free,
            Some("its values are free (at the file's values the frames as MitCad finds them would move the parts)"),
        ));
    }
    let mut why = String::new();
    for (def, note) in tries {
        match im.add(&def, Some(&name), ComponentUid::ROOT) {
            Ok(uid) if !moved(im) => {
                report.feature = Some(uid.to_string());
                if j["suppressed"].as_bool().unwrap_or(false) {
                    let _ = im.doc.set_suppressed(uid, true);
                }
                if let Some(n) = note {
                    report.notes.push(n.to_owned());
                }
                report.outcome = if report.notes.is_empty() {
                    Outcome::Parametric
                } else {
                    Outcome::Partial
                };
                return report;
            }
            Ok(_) => {
                im.doc.undo();
                why = "the joint's frames do not hold where the file places the parts".to_owned();
            }
            Err(e) => why = e,
        }
    }
    {
        {
            // As built: the two occurrences joined where they are.
            let path = |side: &Value| side.get("occurrence").cloned().unwrap_or(json!(""));
            let def = json!({"type": "as_built_joint", "kind": def["kind"], "a": path(&def["a"]),
                "b": path(&def["b"]), "limits": def["limits"]});
            match im.add(&def, Some(&name), ComponentUid::ROOT) {
                Ok(uid) => {
                    report.feature = Some(uid.to_string());
                    report.outcome = Outcome::Partial;
                    report
                        .notes
                        .push(format!("an as-built joint of the placements: {why}"));
                }
                Err(e) => {
                    report.outcome = Outcome::Failed;
                    report.notes.push(format!("{why}; as built: {e}"));
                }
            }
        }
    }
    report
}

fn same(a: &Transform, b: &Transform) -> bool {
    (0..3).all(|i| {
        (a.translation[i] - b.translation[i]).abs() < 1e-6
            && (0..3).all(|k| (a.linear[i][k] - b.linear[i][k]).abs() < 1e-9)
    })
}

/// A joint origin (`{"occurrence": id, "snap": …}`) as MitCad's: the
/// occurrence path from the root and the geometry in its component.
fn origin<K: Kernel>(
    im: &mut Importer<'_, '_, K>,
    o: &Value,
    notes: &mut Vec<String>,
) -> Result<Value, String> {
    let occurrence = o.get("occurrence").and_then(Value::as_u64).unwrap_or(0);
    let (path, component) = occurrence_path(im, occurrence)?;
    let snap = &o["snap"];
    let vec3 = |v: &Value| -> Option<[f64; 3]> {
        let a = v.as_array()?;
        Some([
            a.first()?.as_f64()?,
            a.get(1)?.as_f64()?,
            a.get(2)?.as_f64()?,
        ])
    };
    let mut out = json!({"occurrence": path});
    match snap.get("type").and_then(Value::as_str).unwrap_or("") {
        "face_center" => {
            let pick = vec3(&snap["pick"]).ok_or("a face center without its pick")?;
            let (body, face) = im.face_at(component, None, pick)?;
            out["geometry"] = json!({"body": body.to_string(), "face": face});
            if snap.get("x").is_some_and(|x| !x.is_null()) {
                notes.push("the turn of the origin's frame about z is MitCad's".to_owned());
            }
        }
        "circle_center" => {
            let pick = vec3(&snap["pick"]).ok_or("a circle center without its pick")?;
            let (body, face) = im.face_at(component, None, pick)?;
            let shape = im.doc.body_shape(body).ok_or("the body is gone")?;
            let kernel = im.doc.kernel();
            let name: mitcad_model::FaceName =
                face.parse().map_err(|_| format!("a face name {face}"))?;
            let cylinder = kernel
                .face_cylinder(shape, &name)
                .map_err(|e| format!("{face} is not round: {e}"))?;
            let axis = cylinder.axis;
            // The end nearer the pick, and z out of the face there.
            let along = dot(sub(pick, axis.origin), axis.direction);
            let high = along >= cylinder.length / 2.0;
            let centre = if high {
                mitcad_import_base::geom::add(axis.origin, scale(axis.direction, cylinder.length))
            } else {
                axis.origin
            };
            let z = if high {
                axis.direction
            } else {
                scale(axis.direction, -1.0)
            };
            // The circular edge there.
            let edge = kernel.edges(shape).ok().and_then(|edges| {
                edges.into_iter().filter_map(|e| e.name).find(|e| {
                    e.contains(&face)
                        && e.parse::<mitcad_model::EdgeName>().ok().is_some_and(|en| {
                            matches!(kernel.edge_geometry(shape, &en), Ok(mitcad_model::datum::CurveGeometry::Circle { center, .. }) if distance(center, centre) < 1e-4)
                        })
                })
            });
            match edge {
                Some(e) => out["geometry"] = json!({"body": body.to_string(), "edge": e}),
                None => {
                    out["geometry"] = json!({"body": body.to_string(), "face": face});
                    out["frame_override"] = json!({"origin": centre});
                }
            }
            if !out.get("frame_override").is_some_and(|f| f.is_object()) {
                out["frame_override"] = json!({});
            }
            out["frame_override"]["z_axis"] = json!(z);
        }
        "frame" => {
            let origin = vec3(&snap["origin"]).ok_or("a frame without its origin")?;
            let z = vec3(&snap["z"]).unwrap_or([0.0, 0.0, 1.0]);
            out["geometry"] = json!({"point": origin});
            out["frame_override"] = json!({"z_axis": z});
            if let Some(x) = vec3(&snap["x"]) {
                out["frame_override"]["x_axis"] = json!(x);
            }
        }
        "point" => {
            let p = vec3(&snap["point"]).ok_or("a point origin without its point")?;
            out["geometry"] = json!({"point": p});
            if let Some(z) = vec3(&snap["z"]) {
                out["frame_override"] = json!({"z_axis": z});
            }
            // The direction its frame's x takes (format 28).
            if let Some(x) = vec3(&snap["x"]) {
                if out.get("frame_override").is_none() {
                    out["frame_override"] = json!({});
                }
                out["frame_override"]["x_axis"] = json!(x);
            }
        }
        "edge" => {
            let pick = vec3(&snap["pick"]).ok_or("an edge origin without its pick")?;
            let (body, edge) = im.edge_at(component, None, pick)?;
            out["geometry"] = json!({"body": body.to_string(), "edge": edge});
            if snap.get("corner").is_some_and(|c| !c.is_null()) {
                notes.push("an origin at an edge's corner is at the edge's middle".to_owned());
            }
            // Its z (format 38): a fixed direction, or along the edge.
            if let Some(z) = vec3(&snap["axis"]["fixed"]) {
                out["frame_override"] = json!({"z_axis": z});
            } else if snap["axis"].get("along").is_some() {
                notes.push("an origin's z along its edge is MitCad's edge frame".to_owned());
            }
        }
        other => return Err(format!("a joint origin of the kind `{other}`")),
    }
    Ok(out)
}

/// The occurrence path from the root to the file's occurrence, and the
/// MitCad component it places.
fn occurrence_path<K: Kernel>(
    im: &Importer<'_, '_, K>,
    occurrence: u64,
) -> Result<(String, ComponentUid), String> {
    if occurrence == 0 {
        return Ok((String::new(), ComponentUid::ROOT));
    }
    let mut chain = Vec::new();
    let mut at = occurrence;
    for _ in 0..64 {
        let o = im
            .design
            .occurrences
            .iter()
            .find(|o| o.id == at)
            .ok_or_else(|| format!("no occurrence {at}"))?;
        chain.push(
            im.occurrences
                .get(&o.id)
                .ok_or_else(|| format!("occurrence {} did not come in", o.name))?
                .to_string(),
        );
        if o.parent == 0 {
            break;
        }
        // The parent component's (first) occurrence.
        at = im
            .design
            .occurrences
            .iter()
            .find(|p| p.component == o.parent)
            .map(|p| p.id)
            .ok_or("a component placed nowhere")?;
    }
    chain.reverse();
    let component = im
        .design
        .occurrences
        .iter()
        .find(|o| o.id == occurrence)
        .map(|o| im.component_of(o.component))
        .unwrap_or(ComponentUid::ROOT);
    Ok((chain.join("/"), component))
}
