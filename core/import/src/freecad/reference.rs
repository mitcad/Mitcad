// SPDX-License-Identifier: MIT
//! The import against FreeCAD's own measures of the document: a dump made
//! by `tools/freecad-export/dump.py` (format `mitcad-freecad-dump`).
//!
//! Every place the import recorded ([`super::PlacedReport`]) is compared
//! with what FreeCAD reports for the same object: an object's stored
//! shape, a link's shape, or an array element's. The volume (of solids,
//! by its size: an inside-out solid's is negative in FreeCAD) and the area
//! within 1e-9 relative, the world centre of mass within 1e-6 mm plus
//! 1e-9 of the shape's size. A part's or an assembly's shape is a compound
//! FreeCAD makes of moved copies of its objects, whose measures differ from
//! the objects' own by up to about 1e-6: 1e-6 relative there. A body the
//! history's replay made is Mitcad's own build of FreeCAD's definitions
//! (from sketches solved within 1e-4 mm of FreeCAD's points): the replay's
//! tolerance, 1e-6 relative, there too.
//!
//! Where FreeCAD's shape is invalid, or FreeCAD's measures disagree with
//! FreeCAD's own mesh of the shape by more than 1 % (surface integrals OCCT
//! gets wrong on some faces), or, for a replayed body, with Mitcad's
//! measures of the shape FreeCAD stored, which the body agrees with
//! (FreeCAD's default integration of B-spline surfaces is off by up to
//! about 1e-3), FreeCAD's measures are no reference: a difference there is
//! listed as not compared. So is a difference at a place of shapes the
//! import brought in as FreeCAD stored them, where FreeCAD's measures are
//! those the kernel's plain fixed-point integration gives the same shapes:
//! FreeCAD measures so, while Mitcad integrates faces bounded by B-splines
//! of many spans more exactly (mitcad#139). Objects with faces in FreeCAD
//! that the import left out for want of faces are differences.
//!
//! An imported sketch's profile curves are compared with its stored
//! shape's edges: their number, total length and world centre (by length)
//! within 1e-6 relative.
//!
//! The parameters made of FreeCAD's quantities are compared with FreeCAD's
//! values of them (dumps of format 4: cells, VarSets' properties, bound
//! expressions, constraints) within 1e-9 relative.

use std::collections::HashMap;

use serde_json::Value;

use super::report::{ExpressionOutcome, FcstdReport, ObjectOutcome, ReferenceReport};

const RELATIVE: f64 = 1e-9;
/// For the compounds FreeCAD makes of parts and assemblies.
const GROUP_RELATIVE: f64 = 1e-6;
const ABSOLUTE: f64 = 1e-6;
/// FreeCAD's measures against its mesh of the shape.
const MESH_AGREEMENT: f64 = 1e-2;
/// Sketches: Mitcad's solution and integration against FreeCAD's measures
/// of its stored edges.
const SKETCH_RELATIVE: f64 = 1e-6;

fn number(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn point(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([number(a.first()?)?, number(a.get(1)?)?, number(a.get(2)?)?])
}

/// A value FreeCAD computed (dump.py's `quantity`) in Mitcad's units:
/// millimetres and radians; None for other units.
fn freecad_quantity(v: &Value) -> Option<f64> {
    if v["other"] == true {
        return None;
    }
    let value = number(&v["value"])?;
    let angle = v["angle"].as_i64().unwrap_or(0);
    Some(value * std::f64::consts::PI.powi(angle as i32) / 180f64.powi(angle as i32))
}

fn close(a: f64, b: f64, relative: f64) -> bool {
    (a - b).abs() <= relative * a.abs().max(b.abs()).max(1.0)
}

/// The size of FreeCAD's shape: the largest side of its bounding box, at
/// least 1.
fn size_of(expected: &Value) -> f64 {
    expected["bound_box"].as_array().map_or(1.0, |b| {
        let v: Vec<f64> = b.iter().filter_map(number).collect();
        if v.len() == 6 {
            (0..3).map(|k| (v[k + 3] - v[k]).abs()).fold(1.0, f64::max)
        } else {
            1.0
        }
    })
}

/// Whether FreeCAD's volume (of solids), area and world centre are the
/// measures `m`, within `relative` (the centre within 1e-6 mm plus
/// `relative` of the shape's size).
fn freecad_has(expected: &Value, m: &super::report::StoredMeasures, relative: f64) -> bool {
    let solids = expected["solids"].as_u64().unwrap_or(0) > 0;
    let size = size_of(expected);
    let volume = number(&expected["volume"]).map(f64::abs);
    let area = number(&expected["area"]);
    let center = point(&expected["world_center"]);
    (!solids || volume.is_some_and(|v| close(v, m.volume, relative)))
        && area.is_some_and(|a| close(a, m.area, relative))
        && center.is_some_and(|c| {
            (0..3).all(|k| (c[k] - m.center[k]).abs() <= ABSOLUTE + relative * size)
        })
}

/// Why FreeCAD's measures of a shape are no reference, if they are not.
fn unreliable(expected: &Value) -> Option<String> {
    if expected["valid"] == false {
        return Some("FreeCAD's shape is invalid".to_owned());
    }
    let disagree = |measure: &str, mesh: &str| {
        let (Some(m), Some(e)) = (number(&expected[mesh]), number(&expected[measure])) else {
            return false;
        };
        (m - e.abs()).abs() > MESH_AGREEMENT * m.abs().max(e.abs())
    };
    let solids = expected["solids"].as_u64().unwrap_or(0) > 0;
    if disagree("area", "mesh_area") || (solids && disagree("volume", "mesh_volume")) {
        return Some(format!(
            "FreeCAD's measures disagree with its mesh of the shape (area {} against {}, volume {} against {})",
            expected["area"], expected["mesh_area"], expected["volume"], expected["mesh_volume"]
        ));
    }
    None
}

/// Why FreeCAD's measures of a replayed body are no reference, if they
/// are not: they differ from Mitcad's measures of the very shape FreeCAD
/// stored, which the replayed body agrees with (FreeCAD integrates some
/// surfaces, B-splines among them, less exactly).
fn stored_apart(
    place: &super::report::PlacedReport,
    expected: &Value,
    relative: f64,
) -> Option<String> {
    let stored = place.stored?;
    let solids = expected["solids"].as_u64().unwrap_or(0) > 0;
    let size = size_of(expected);
    let near = |a: [f64; 3], b: [f64; 3]| {
        (0..3).all(|k| (a[k] - b[k]).abs() <= ABSOLUTE + relative * size)
    };
    let agrees = (!solids || close(place.volume, stored.volume, relative))
        && close(place.area, stored.area, relative)
        && near(place.center, stored.center);
    let theirs_volume = number(&expected["volume"]).map(f64::abs);
    let theirs_area = number(&expected["area"]);
    let theirs_center = point(&expected["world_center"]);
    let apart = solids && theirs_volume.is_some_and(|v| !close(v, stored.volume, relative))
        || theirs_area.is_some_and(|a| !close(a, stored.area, relative))
        || theirs_center.is_some_and(|c| !near(c, stored.center));
    (agrees && apart).then(|| {
        format!(
            "FreeCAD's measures of its stored shape differ from Mitcad's of the same shape (volume {} against {}, area {} against {}), which the import agrees with",
            expected["volume"], stored.volume, expected["area"], stored.area
        )
    })
}

/// Why FreeCAD's measures of a place of shapes the import brought in as
/// FreeCAD stored them are no reference, if they are not: they are the
/// measures of the kernel's plain fixed-point integration of the same shapes
/// ([`super::report::PlacedReport::fixed`]), as FreeCAD measures, which do
/// not follow a face's boundary curves that are B-splines of many spans;
/// Mitcad integrates those faces more exactly (mitcad#139).
fn fixed_apart(
    place: &super::report::PlacedReport,
    expected: &Value,
    relative: f64,
) -> Option<String> {
    let fixed = place.fixed?;
    (!place.replayed && freecad_has(expected, &fixed, relative)).then(|| {
        format!(
            "FreeCAD's measures are those of fixed-point integration (volume {}, area {}), \
             which Mitcad's integration of faces bounded by B-splines of many spans refines",
            fixed.volume, fixed.area
        )
    })
}

pub(crate) fn check(report: &FcstdReport, dump: &Value) -> ReferenceReport {
    let mut out = ReferenceReport::default();
    if dump["format"] != "mitcad-freecad-dump" {
        out.differences
            .push("the reference is not a dump of tools/freecad-export/dump.py".to_owned());
        return out;
    }
    let objects: HashMap<&str, &Value> = dump["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| Some((o["name"].as_str()?, o)))
        .collect();
    for place in &report.placed {
        let what = match place.element {
            Some(i) => format!("{} element {i}", place.object),
            None => place.object.clone(),
        };
        let Some(o) = objects.get(place.object.as_str()) else {
            out.differences
                .push(format!("{what}: not in the reference"));
            continue;
        };
        let expected = match (place.element, o.get("link")) {
            (Some(i), Some(link)) => &link["elements"][i],
            (None, Some(link)) => &link["shape"],
            _ => &o["shape"],
        };
        if expected.is_null() || expected.get("error").is_some() {
            out.differences
                .push(format!("{what}: the reference has no shape for it"));
            continue;
        }
        let group = matches!(
            o["type"].as_str(),
            Some("App::Part" | "Assembly::AssemblyObject")
        );
        let relative = if place.replayed {
            super::history::TOLERANCE
        } else if group {
            GROUP_RELATIVE
        } else {
            RELATIVE
        };
        let mut differences = Vec::new();
        let mut compare = |name: &str, mine: f64, theirs: Option<f64>| {
            out.checked += 1;
            match theirs {
                Some(t) if close(mine, t, relative) => {}
                Some(t) => differences.push(format!("{what}: {name} {mine} against FreeCAD's {t}")),
                None => differences.push(format!("{what}: the reference has no {name}")),
            }
        };
        // Volumes of solids (FreeCAD's volume of a sheet is a surface
        // integral without meaning).
        if expected["solids"].as_u64().unwrap_or(0) > 0 {
            compare(
                "volume",
                place.volume,
                number(&expected["volume"]).map(f64::abs),
            );
        }
        compare("area", place.area, number(&expected["area"]));
        // A part that shows only curves (sketches) in FreeCAD has no body
        // here: no centre to compare.
        let curves_only = expected["faces"].as_u64() == Some(0) && place.bodies == 0;
        if !curves_only {
            out.checked += 1;
        }
        match point(&expected["world_center"]) {
            _ if curves_only => {}
            Some(c) => {
                let tolerance = ABSOLUTE + relative * size_of(expected);
                let far = (0..3).any(|k| (place.center[k] - c[k]).abs() > tolerance);
                if far {
                    differences.push(format!(
                        "{what}: centre {:?} against FreeCAD's {:?}",
                        place.center, c
                    ));
                }
            }
            None => differences.push(format!("{what}: the reference has no world centre")),
        }
        // A difference from measures FreeCAD itself does not get right is
        // no difference of the import.
        let why = unreliable(expected)
            .or_else(|| stored_apart(place, expected, relative))
            .or_else(|| fixed_apart(place, expected, relative));
        match why {
            Some(why) if !differences.is_empty() => {
                out.not_compared
                    .push(format!("{what}: {why}; {}", differences.join("; ")));
            }
            _ => out.differences.extend(differences),
        }
    }
    // Sketches: the profile curves against the stored shape's edges.
    for s in &report.sketches {
        if s.feature.is_none() {
            continue;
        }
        // Dumps list every object: one without the sketch is no dump of
        // its sketches (a hand-made reference).
        let Some(o) = objects.get(s.object.as_str()) else {
            continue;
        };
        let expected = &o["shape"];
        if expected.is_null() || expected.get("error").is_some() || expected["null"] == true {
            continue;
        }
        let what = &s.object;
        let edges = expected["edges"].as_u64().unwrap_or(0) as usize;
        // A file whose sketch has no geometry but a stored shape with
        // edges says two things. The dump's sketch tells; a dump without
        // one (an object FreeCAD could not dump as a sketch), the file's
        // geometry as the import read it.
        let no_geometry = match o["sketch"]["geometry"].as_array() {
            Some(geometry) => geometry.is_empty(),
            None => o["sketch"].is_null() && s.geometry == 0,
        };
        if no_geometry && edges > 0 {
            out.not_compared.push(format!(
                "{what}: FreeCAD's sketch has no geometry, its stored shape {edges} edges"
            ));
            continue;
        }
        out.checked += 1;
        if edges != s.edges {
            out.differences.push(format!(
                "{what}: {} profile curves against FreeCAD's {edges} edges",
                s.edges
            ));
        }
        if let Some(length) = number(&expected["length"]) {
            out.checked += 1;
            if !close(s.length, length, SKETCH_RELATIVE) {
                out.differences.push(format!(
                    "{what}: length {} against FreeCAD's {length}",
                    s.length
                ));
            }
        }
        if let (Some(mine), Some(theirs)) = (s.center, point(&expected["world_center"])) {
            out.checked += 1;
            let size = expected["bound_box"].as_array().map_or(1.0, |b| {
                let v: Vec<f64> = b.iter().filter_map(number).collect();
                if v.len() == 6 {
                    (0..3).map(|k| (v[k + 3] - v[k]).abs()).fold(1.0, f64::max)
                } else {
                    1.0
                }
            });
            if (0..3).any(|k| (mine[k] - theirs[k]).abs() > ABSOLUTE + SKETCH_RELATIVE * size) {
                out.differences.push(format!(
                    "{what}: centre {mine:?} against FreeCAD's {theirs:?}"
                ));
            }
        }
    }
    // Parameters: Mitcad's values against FreeCAD's (cells, VarSets' and
    // bound properties, constraints).
    for p in &report.parameters {
        let (Some(mine), Some(o)) = (p.value, objects.get(p.object.as_str())) else {
            continue;
        };
        if p.outcome == ExpressionOutcome::Skipped {
            continue;
        }
        let theirs = if let Some(cell) = &p.cell {
            o["cells"]
                .as_array()
                .and_then(|cells| cells.iter().find(|c| c["address"] == cell.as_str()))
                .and_then(|c| freecad_quantity(&c["value"]))
        } else if let Some(index) = p.constraint {
            number(&o["sketch"]["constraints"][index]["value"])
        } else if let Some(path) = &p.property {
            freecad_quantity(&o["values"][path.as_str()]).or_else(|| {
                o["expressions"]
                    .as_array()?
                    .iter()
                    .find(|e| e["path"].as_str().map(|s| s.trim_start_matches('.')) == Some(path))
                    .and_then(|e| freecad_quantity(&e["value"]))
            })
        } else {
            None
        };
        let Some(theirs) = theirs else {
            continue;
        };
        out.checked += 1;
        if !close(mine, theirs, RELATIVE) {
            out.differences.push(format!(
                "parameter {} ({}): {mine} against FreeCAD's {theirs}",
                p.name, p.source
            ));
        }
    }
    // Objects with faces left out for want of them.
    for item in &report.items {
        let shape_reason = item.note.as_deref().is_some_and(|n| {
            n.starts_with("no faces") || n.starts_with("empty") || n.contains("could not be read")
        });
        if item.outcome != ObjectOutcome::Skipped || !shape_reason {
            continue;
        }
        let faces = objects
            .get(item.name.as_str())
            .and_then(|o| o["shape"]["faces"].as_u64())
            .unwrap_or(0);
        if faces > 0 {
            out.differences.push(format!(
                "{}: FreeCAD's shape has {faces} faces, but it was left out ({})",
                item.name,
                item.note.as_deref().unwrap_or("")
            ));
        }
    }
    out.pass = out.differences.is_empty();
    out
}
