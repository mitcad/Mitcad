// SPDX-License-Identifier: MIT
//! The design stream decoder against the dumps of the reference models
//! (`MITCAD_F3D_MODELS`, default `~/f3d-models`, kept outside the
//! repository: `<id>/<id>.f3d` with its dump `<id>/<id>.json`; skipped
//! when the directory is missing), for what the decoder reads beyond the
//! reference decoder that `design_corpus.rs` compares with.
//!
//! - Light bulbs (mitcad#6): every sketch's and construction plane's
//!   decoded `props.isLightBulbOn` equals the dump's light bulb
//!   (`props.isLightBulbOn`, else a sketch's `detail.isVisible`).
//! - Fillet and chamfer edges found by their names (mitcad#33).
//! - Parameters (class version 8 of the 2026 writers among them).
//! - Threads and tapped holes' threads, and threads' faces found by their
//!   names (mitcad#35).
//! - The component that owns each item (mitcad#37).
//! - The inputs of sweeps and lofts (mitcad#34).
//! - Where the timeline's items put the occurrences, joints' motion types
//!   and rigid groups' members (mitcad#81).
//! - Extrusions' objects of extents up to an object and symmetric lengths
//!   (mitcad#96).

use std::path::PathBuf;

use mitcad_f3d::design::{self, ir};

fn models_dir() -> Option<PathBuf> {
    let dir = match std::env::var_os("MITCAD_F3D_MODELS") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME")?).join("f3d-models"),
    };
    dir.is_dir().then_some(dir)
}

/// The models with both an `.f3d` and its dump, by id.
fn models(dir: &std::path::Path) -> Vec<(String, PathBuf, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            let f3d = e.path().join(format!("{id}.f3d"));
            let json = e.path().join(format!("{id}.json"));
            (f3d.is_file() && json.is_file()).then_some((id, f3d, json))
        })
        .collect();
    out.sort();
    out
}

/// A dump's timeline items of a type, in timeline order.
fn of_kind<'a>(dump: &'a ir::Dump, kind: &str) -> Vec<&'a ir::TimelineItem> {
    dump.timeline_items()
        .iter()
        .filter(|i| i.object_type() == Some(kind))
        .collect()
}

#[test]
fn light_bulbs_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut agree, mut unknown) = (0usize, 0usize);
    let mut differ = Vec::new();
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        for kind in ["Sketch", "ConstructionPlane"] {
            let twins = of_kind(&decoded, kind);
            for (k, item) in of_kind(&reference, kind).into_iter().enumerate() {
                let Some(expected) = item.light_bulb() else {
                    continue;
                };
                // By timeline index; items of a timeline group have none in
                // the dump (and the decoder does not read their names), so
                // by their order among the items of their type.
                let twin = match item.index {
                    Some(_) => twins.iter().find(|d| d.index == item.index),
                    None => twins.get(k),
                }
                .copied();
                let what = format!("{id}: {kind} {}", item.name().unwrap_or("?"));
                match twin.and_then(ir::TimelineItem::light_bulb) {
                    Some(on) if on == expected => agree += 1,
                    Some(on) => differ.push(format!("{what}: decoded {on}, dump {expected}")),
                    None => {
                        unknown += 1;
                        differ.push(format!("{what}: not decoded"));
                    }
                }
            }
        }
    }
    println!(
        "light bulbs of sketches and construction planes: {agree} agree, {} differ \
         ({unknown} not decoded)",
        differ.len() - unknown
    );
    for d in &differ {
        println!("  {d}");
    }
    if agree + differ.len() == 0 {
        eprintln!("no reference model has sketches; skipped");
        return;
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

/// The component that owns each feature, sketch and construction item
/// (mitcad#37) is the dump's: the root as the root, others by name.
#[test]
fn item_components_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let root_of = |dump: &ir::Dump| {
        dump.document
            .as_ref()
            .and_then(|d| d.root_component.clone())
    };
    let owned = |t: &str| t == "Sketch" || t.starts_with("Construction") || t.ends_with("Feature");
    let (mut agree, mut differ) = (0usize, Vec::new());
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        let (ref_root, dec_root) = (root_of(&reference), root_of(&decoded));
        for item in reference.timeline_items() {
            let (Some(t), Some(index)) = (item.object_type(), item.index) else {
                continue;
            };
            let Some(expected) = item.component.clone().flatten().filter(|_| owned(t)) else {
                continue;
            };
            let Some(twin) = decoded
                .timeline_items()
                .iter()
                .find(|d| d.index == Some(index))
            else {
                continue;
            };
            let got = twin.component.clone().flatten();
            let same = match (&got, expected == ref_root.clone().unwrap_or_default()) {
                (Some(g), true) => Some(g) == dec_root.as_ref(),
                (Some(g), false) => *g == expected,
                (None, _) => false,
            };
            if same {
                agree += 1;
            } else {
                differ.push(format!(
                    "{id}: {t} {}: decoded {got:?}, dump {expected}",
                    item.name().unwrap_or("?")
                ));
            }
        }
    }
    println!("item components: {agree} agree, {} differ", differ.len());
    assert!(differ.is_empty(), "{differ:#?}");
}

/// Construction and centre lines (mitcad#33): every sketch curve the
/// decoder marks is so in the dump, and every one the dump marks is
/// decoded so.
#[test]
fn construction_curves_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut agree, mut normal) = (0usize, 0usize);
    let mut differ = Vec::new();
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        let twins = of_kind(&decoded, "Sketch");
        for item in of_kind(&reference, "Sketch") {
            let Some(twin) = twins
                .iter()
                .find(|d| d.index == item.index && d.index.is_some())
            else {
                continue;
            };
            let curves = |i: &ir::TimelineItem| match &i.detail {
                Some(ir::Detail::Sketch(s)) => s.curves.clone().unwrap_or_default(),
                _ => Vec::new(),
            };
            let mine = curves(twin);
            for c in curves(item) {
                let Some(m) = mine.iter().find(|m| m.id == c.id) else {
                    continue;
                };
                let kind = |c: &ir::SketchCurve| {
                    (
                        c.is_construction == Some(true),
                        c.is_center_line == Some(true),
                    )
                };
                if kind(m) != kind(&c) {
                    differ.push(format!(
                        "{id}: {} {:?}: decoded {:?}, dump {:?}",
                        item.name().unwrap_or("?"),
                        c.id,
                        kind(m),
                        kind(&c)
                    ));
                } else if kind(&c) == (false, false) {
                    normal += 1;
                } else {
                    agree += 1;
                }
            }
        }
    }
    println!(
        "construction curves: {agree} agree, {normal} normal, {} differ",
        differ.len()
    );
    for d in &differ {
        println!("  {d}");
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

/// Moves (mitcad#33): the decoded transform is the dump's, and the moved
/// bodies are found (by their names) in the history.
#[test]
fn moves_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut agree, mut differ) = (0usize, Vec::new());
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        let twins = of_kind(&decoded, "MoveFeature");
        for (k, item) in of_kind(&reference, "MoveFeature").into_iter().enumerate() {
            let map = |i: &ir::TimelineItem| match &i.detail {
                Some(ir::Detail::Other(m)) => m.clone(),
                _ => Default::default(),
            };
            let (theirs, mine) = (map(item), twins.get(k).map(|t| map(t)).unwrap_or_default());
            let matrix = |m: &serde_json::Map<String, serde_json::Value>| {
                serde_json::from_value::<[[f64; 4]; 4]>(m.get("transform")?.clone()).ok()
            };
            let what = format!("{id}: {}", item.name().unwrap_or("?"));
            let same = match (matrix(&mine), matrix(&theirs)) {
                (Some(a), Some(b)) => {
                    (0..4).all(|r| (0..4).all(|c| (a[r][c] - b[r][c]).abs() < 1e-9))
                }
                _ => false,
            };
            let found = mine
                .get("inputEntities")
                .and_then(|v| v.as_array())
                .is_some_and(|a| {
                    !a.is_empty() && a.iter().all(|b| b["_f3d"].get("edge_points").is_some())
                });
            if same && found {
                agree += 1;
            } else {
                differ.push(format!("{what}: transform {same}, bodies found {found}"));
            }
        }
    }
    println!("moves: {agree} agree, {} differ", differ.len());
    for d in &differ {
        println!("  {d}");
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

/// The edge sets of a fillet or chamfer.
fn edge_sets(item: &ir::TimelineItem) -> Vec<ir::EdgeSet> {
    match &item.detail {
        Some(ir::Detail::Fillet(f)) => f.edge_sets.clone().unwrap_or_default(),
        Some(ir::Detail::Chamfer(c)) => c.edge_sets.clone().unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn mid_points(set: &ir::EdgeSet) -> Vec<ir::Vec3> {
    set.edges
        .iter()
        .flatten()
        .filter_map(|r| match r {
            ir::Reference::Edge(fp) => fp.mid_point,
            _ => None,
        })
        .collect()
}

/// Fillet and chamfer edges found by their names (mitcad#33): every edge
/// the decoder found is one of its set's edges in the dump (by the middle
/// point), and a set has as many edges as there.
#[test]
fn dressup_edges_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut found, mut missing) = (0usize, 0usize);
    let mut differ = Vec::new();
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        for kind in ["FilletFeature", "ChamferFeature"] {
            let twins = of_kind(&decoded, kind);
            for (k, item) in of_kind(&reference, kind).into_iter().enumerate() {
                let Some(twin) = twins.get(k) else { continue };
                let what = format!("{id}: {}", item.name().unwrap_or("?"));
                let (sets, ours) = (edge_sets(item), edge_sets(twin));
                if ours.iter().all(|s| s.edges.is_none()) {
                    missing += sets.len();
                    continue;
                }
                if sets.len() != ours.len() {
                    differ.push(format!("{what}: {} sets, dump {}", ours.len(), sets.len()));
                    continue;
                }
                let near = |a: &ir::Vec3, b: &ir::Vec3| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6);
                for (s, (theirs, mine)) in sets.iter().zip(&ours).enumerate() {
                    let (theirs, mine) = (mid_points(theirs), mid_points(mine));
                    // Items after the timeline marker have no state to find
                    // edges in, and the dump has no edges of some.
                    if mine.is_empty() || theirs.is_empty() {
                        missing += 1;
                        continue;
                    }
                    if mine.len() == theirs.len()
                        && mine.iter().all(|m| theirs.iter().any(|t| near(m, t)))
                    {
                        found += 1;
                    } else {
                        differ.push(format!("{what}: set {s}: {mine:?}, dump {theirs:?}"));
                    }
                }
            }
        }
    }
    println!(
        "fillet and chamfer edge sets: {found} found, {} differ, {missing} not decoded",
        differ.len()
    );
    for d in &differ {
        println!("  {d}");
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

/// The thread fields of a `ThreadInfo` (lengths cm, the angle degrees),
/// for comparison.
fn thread_info(info: Option<&serde_json::Value>) -> Option<(Vec<String>, Vec<f64>)> {
    let info = info?;
    let text = [
        "threadType",
        "threadDesignation",
        "threadClass",
        "threadSize",
    ]
    .iter()
    .map(|k| {
        info.get(*k)
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_owned()
    })
    .chain([format!("{:?}", info.get("isInternal"))])
    .collect();
    let numbers = [
        "threadAngle",
        "majorDiameter",
        "minorDiameter",
        "pitchDiameter",
        "threadPitch",
    ]
    .iter()
    .map(|k| info.get(*k).and_then(|v| v.as_f64()).unwrap_or(f64::NAN))
    .collect();
    Some((text, numbers))
}

/// Threads (mitcad#35): every thread decodes with the dump's size, class,
/// type, side, flags and location, and its faces are found in the history
/// with the dump's cylinder (axis direction included, which tells the ends
/// apart) and a point on it; a tapped hole's thread with the dump's size.
#[test]
fn threads_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut agree, mut differ) = (0usize, Vec::new());
    let same_info = |a: Option<&serde_json::Value>, b: Option<&serde_json::Value>| match (
        thread_info(a),
        thread_info(b),
    ) {
        (Some((ta, na)), Some((tb, nb))) => {
            ta == tb && na.iter().zip(&nb).all(|(x, y)| (x - y).abs() < 1e-9)
        }
        _ => false,
    };
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        let twins = of_kind(&decoded, "ThreadFeature");
        for (k, item) in of_kind(&reference, "ThreadFeature").into_iter().enumerate() {
            let what = format!("{id}: {}", item.name().unwrap_or("?"));
            let (Some(ir::Detail::Thread(theirs)), Some(ir::Detail::Thread(mine))) =
                (&item.detail, twins.get(k).and_then(|t| t.detail.as_ref()))
            else {
                differ.push(format!("{what}: not decoded"));
                continue;
            };
            let flags =
                |t: &ir::ThreadDetail| (t.is_modeled, t.is_full_length, t.thread_location.clone());
            if !same_info(theirs.thread_info.as_ref(), mine.thread_info.as_ref()) {
                differ.push(format!("{what}: {:?}", mine.thread_info));
                continue;
            }
            if flags(theirs) != flags(mine) {
                differ.push(format!(
                    "{what}: {:?}, dump {:?}",
                    flags(mine),
                    flags(theirs)
                ));
                continue;
            }
            let faces = |t: &ir::ThreadDetail| -> Vec<ir::Fingerprint> {
                t.input_cylindrical_faces
                    .iter()
                    .flatten()
                    .filter_map(|r| match r {
                        ir::Reference::Face(fp) => Some((**fp).clone()),
                        _ => None,
                    })
                    .collect()
            };
            let (theirs, mine) = (faces(theirs), faces(mine));
            let fits = theirs.len() == mine.len()
                && theirs.iter().zip(&mine).all(|(t, m)| {
                    let (Some(tg), Some(mg), Some(p)) = (&t.geometry, &m.geometry, m.point_on_face)
                    else {
                        return false;
                    };
                    let (Some(o), Some(a), Some(r)) = (tg.origin, tg.axis, tg.radius) else {
                        return false;
                    };
                    let off_axis = |q: ir::Vec3| {
                        let d = [q[0] - o[0], q[1] - o[1], q[2] - o[2]];
                        let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
                        (0..3)
                            .map(|i| (d[i] - h * a[i]).powi(2))
                            .sum::<f64>()
                            .sqrt()
                    };
                    let ma = mg.axis.unwrap_or_default();
                    let along = ma[0] * a[0] + ma[1] * a[1] + ma[2] * a[2];
                    along > 1.0 - 1e-9
                        && mg.radius.is_some_and(|mr| (mr - r).abs() < 1e-9)
                        && mg.origin.is_some_and(|mo| off_axis(mo) < 1e-9)
                        && (off_axis(p) - r).abs() < 1e-9
                });
            if fits {
                agree += 1;
            } else {
                differ.push(format!("{what}: faces {mine:?}, dump {theirs:?}"));
            }
        }
        let holes = of_kind(&decoded, "HoleFeature");
        for (k, item) in of_kind(&reference, "HoleFeature").into_iter().enumerate() {
            let (Some(ir::Detail::Hole(theirs)), Some(ir::Detail::Hole(mine))) =
                (&item.detail, holes.get(k).and_then(|t| t.detail.as_ref()))
            else {
                continue;
            };
            let tapped = |h: &ir::HoleDetail| {
                h.hole_tap_type
                    .as_deref()
                    .is_some_and(|t| t.starts_with("Tapped"))
            };
            if !tapped(theirs) && !tapped(mine) {
                continue;
            }
            let what = format!("{id}: {}", item.name().unwrap_or("?"));
            if tapped(theirs)
                && tapped(mine)
                && same_info(
                    theirs.tapped_hole_info.as_ref(),
                    mine.tapped_hole_info.as_ref(),
                )
            {
                agree += 1;
            } else {
                differ.push(format!("{what}: {:?}", mine.tapped_hole_info));
            }
        }
    }
    println!("threads: {agree} agree, {} differ", differ.len());
    for d in &differ {
        println!("  {d}");
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

/// A path's entities (a list of `PathEntity` items, one item, a dump's
/// rail record or a reference) for comparison: sketch curves and points by
/// their sketch's timeline index and id, edges by their middle points,
/// profiles by their sketch's timeline index.
fn path_keys(v: &serde_json::Value) -> Vec<String> {
    use serde_json::Value;
    match v {
        Value::Array(a) => a.iter().flat_map(path_keys).collect(),
        Value::Object(m) if m.contains_key("entity") => path_keys(&m["entity"]),
        Value::Object(m) => vec![match m.get("kind").and_then(Value::as_str) {
            Some("sketch_entity") => format!("{}:{}", m["sketch_timeline_index"], m["id"]),
            Some("profile") => format!("profile of {}", m["sketch_timeline_index"]),
            Some("edge") => {
                let p: Vec<String> = m["mid_point"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|x| format!("{:.5}", x.as_f64().unwrap_or(f64::NAN) + 0.0))
                    .collect();
                format!("edge at {}", p.join(" "))
            }
            k => format!("{k:?}"),
        }],
        _ => Vec::new(),
    }
}

/// A parameter reference for comparison: its name and value.
fn parameter_key(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(p) => format!("{} = {}", p["name"], p["value"]),
        None => "-".to_owned(),
    }
}

/// The inputs of a sweep, pipe or loft (mitcad#34) for comparison: the
/// operation, profiles, paths, rails, sections and the first and last
/// sections' conditions.
fn sweep_inputs(item: &ir::TimelineItem) -> serde_json::Value {
    use serde_json::{Value, json};
    let d = item
        .detail
        .as_ref()
        .map(|d| serde_json::to_value(d).unwrap())
        .unwrap_or_default();
    let operation = d.get("operation").cloned().unwrap_or(Value::Null);
    match item.object_type() {
        Some("LoftFeature") => {
            let sections = d["loftSections"].as_array().cloned().unwrap_or_default();
            let n = sections.len();
            let sections: Vec<Value> = sections
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let mut entity = path_keys(&s["entity"]);
                    entity.sort();
                    let c = &s["endCondition"];
                    let condition = (i == 0 || i + 1 == n).then(|| {
                        format!(
                            "{} angle {} weight {}",
                            c["_type"],
                            parameter_key(c.get("angle")),
                            parameter_key(c.get("weight"))
                        )
                    });
                    json!({"entity": entity, "condition": condition})
                })
                .collect();
            let guides: Vec<Vec<String>> = d["centerLineOrRails"]
                .as_array()
                .into_iter()
                .flatten()
                .map(path_keys)
                .collect();
            json!({"operation": operation, "sections": sections, "guides": guides,
                   "centerline": d["centerLineOrRails.isCenterLine"]})
        }
        _ => {
            let rail = d.get("guideRail").filter(|r| !r.is_null());
            json!({"operation": operation, "profile": path_keys(&d["profile"])
                       .into_iter().collect::<std::collections::BTreeSet<_>>(),
                   "path": path_keys(&d["path"]), "rail": rail.map(path_keys)})
        }
    }
}

/// Sweeps, pipes and lofts (mitcad#34): the decoded inputs are the dump's
/// (operation, profiles' sketches, paths and guide rails by their sketch
/// curves or edges, loft sections, the first and last sections' conditions
/// with their parameters, centre lines and rails).
#[test]
fn sweeps_and_lofts_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut agree, mut differ) = (0usize, Vec::new());
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        for kind in ["SweepFeature", "PipeFeature", "LoftFeature"] {
            let twins = of_kind(&decoded, kind);
            for (k, item) in of_kind(&reference, kind).into_iter().enumerate() {
                let what = format!("{id}: {}", item.name().unwrap_or("?"));
                let theirs = sweep_inputs(item);
                let mine = twins.get(k).map(|t| sweep_inputs(t));
                if mine.as_ref() == Some(&theirs) {
                    agree += 1;
                } else {
                    differ.push(format!("{what}: {mine:?}, dump {theirs}"));
                }
            }
        }
    }
    println!(
        "sweeps, pipes and lofts: {agree} agree, {} differ",
        differ.len()
    );
    for d in &differ {
        println!("  {d}");
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

/// Every parameter of the reference models decodes (class version 8 of
/// the 2026 writers among them) with the dump's value.
#[test]
fn parameters_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let mut differ = Vec::new();
    let mut agree = 0usize;
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        // Text parameters have no value.
        for p in reference.all_parameters().filter(|p| p.value.is_some()) {
            let name = p.name.clone().unwrap_or_default();
            let twin = decoded.all_parameters().find(|q| q.name == p.name);
            match (twin.and_then(|q| q.value), p.value) {
                (Some(a), Some(b)) if (a - b).abs() <= 1e-9 * b.abs().max(1.0) => agree += 1,
                (a, b) => differ.push(format!("{id}: {name}: decoded {a:?}, dump {b:?}")),
            }
        }
    }
    println!("parameters: {agree} agree, {} differ", differ.len());
    for d in differ.iter().take(40) {
        println!("  {d}");
    }
    assert!(differ.is_empty(), "{} differ", differ.len());
}

/// A reference as a comparable key: a feature, plane or axis by its
/// timeline index or origin name, a body by `body`.
fn reference_key(v: &serde_json::Value) -> String {
    match v["kind"].as_str() {
        Some("body") => "body".to_owned(),
        Some(k @ ("face" | "edge")) => k.to_owned(),
        Some(k) => format!("{k}:{}:{}", v["origin"], v["timeline_index"]),
        None => v.to_string(),
    }
}

/// Every body of a decoded selection was found, with edge points inside the
/// reference's body's box (cm, 1e-6).
fn bodies_agree(mine: &serde_json::Value, theirs: &serde_json::Value) -> bool {
    let (Some(a), Some(b)) = (mine.as_array(), theirs.as_array()) else {
        return false;
    };
    a.len() == b.len()
        && a.iter().zip(b).all(|(m, t)| {
            let (Some(points), Some(min), Some(max)) = (
                m["_f3d"]["edge_points"].as_array(),
                t["bbox"]["min"].as_array(),
                t["bbox"]["max"].as_array(),
            ) else {
                return false;
            };
            !points.is_empty()
                && points.iter().all(|p| {
                    (0..3).all(|i| {
                        let x = p[i].as_f64().unwrap_or(f64::NAN);
                        x >= min[i].as_f64().unwrap_or(0.0) - 1e-6
                            && x <= max[i].as_f64().unwrap_or(0.0) + 1e-6
                    })
                })
        })
}

#[test]
fn selections_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut agree, mut differ) = (0usize, Vec::new());
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        for kind in [
            "CombineFeature",
            "SplitBodyFeature",
            "MirrorFeature",
            "CircularPatternFeature",
            "RectangularPatternFeature",
            "HoleFeature",
        ] {
            let twins = of_kind(&decoded, kind);
            for (k, item) in of_kind(&reference, kind).into_iter().enumerate() {
                let detail = |i: &ir::TimelineItem| {
                    serde_json::to_value(i.detail.as_ref()).unwrap_or_default()
                };
                let theirs = detail(item);
                let mine = twins.get(k).map(|t| detail(t)).unwrap_or_default();
                let key = |v: &serde_json::Value| match v {
                    serde_json::Value::Array(a) => {
                        a.iter().map(reference_key).collect::<Vec<_>>().join(",")
                    }
                    v => reference_key(v),
                };
                let mut bad = Vec::new();
                let mut same = |field: &str, ok: bool| {
                    if !ok {
                        bad.push(field.to_owned());
                    }
                };
                match kind {
                    "CombineFeature" => {
                        same("operation", mine["operation"] == theirs["operation"]);
                        same(
                            "isKeepToolBodies",
                            mine["isKeepToolBodies"] == theirs["isKeepToolBodies"],
                        );
                        same(
                            "targetBody",
                            bodies_agree(
                                &serde_json::json!([mine["targetBody"]]),
                                &serde_json::json!([theirs["targetBody"]]),
                            ),
                        );
                        same(
                            "toolBodies",
                            bodies_agree(&mine["toolBodies"], &theirs["toolBodies"]),
                        );
                    }
                    "SplitBodyFeature" => {
                        same(
                            "splitBodies",
                            bodies_agree(&mine["splitBodies"], &theirs["splitBodies"]),
                        );
                        same(
                            "splittingTool",
                            key(&mine["splittingTool"]) == key(&theirs["splittingTool"]),
                        );
                    }
                    "HoleFeature" => {
                        same("holeType", mine["holeType"] == theirs["holeType"]);
                        same("position", mine["position"] == theirs["position"]);
                        let extent = |v: &serde_json::Value| {
                            v["extentDefinition"]["_type"].as_str().map(str::to_owned)
                        };
                        // An extent up to an object is not decoded.
                        if extent(&theirs).as_deref() != Some("OneSideToExtentDefinition") {
                            same("extentDefinition", extent(&mine) == extent(&theirs));
                        }
                    }
                    _ => {
                        let objects = |v: &serde_json::Value| {
                            v["inputEntities"].as_array().map(|a| {
                                a.iter()
                                    .map(|o| match o["kind"].as_str() {
                                        Some("feature") => o["timeline_index"].to_string(),
                                        k => k.unwrap_or("?").to_owned(),
                                    })
                                    .collect::<Vec<_>>()
                            })
                        };
                        same("inputEntities", objects(&mine) == objects(&theirs));
                        for field in [
                            "mirrorPlane",
                            "axis",
                            "directionOneEntity",
                            "directionTwoEntity",
                        ] {
                            if !theirs[field].is_null() || !mine[field].is_null() {
                                same(field, key(&mine[field]) == key(&theirs[field]));
                            }
                        }
                    }
                }
                if bad.is_empty() {
                    agree += 1;
                } else {
                    differ.push(format!(
                        "{id}: {}: {}",
                        item.name().unwrap_or("?"),
                        bad.join(", ")
                    ));
                }
            }
        }
    }
    println!("selections: {agree} agree, {} differ", differ.len());
    for d in &differ {
        println!("  {d}");
    }
    assert!(differ.is_empty(), "{differ:#?}");
}

/// Where the timeline's items put each top-level occurrence (mitcad#81):
/// its last decoded placement is the dump's transform (where the file
/// shows it); joints' motion types and slots are the dump's, and rigid
/// groups (as-built joints of the rigid group type) join the dump's
/// occurrences.
#[test]
fn placements_joints_and_rigid_groups_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut agree, mut differ) = (0usize, Vec::new());
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        // Top-level occurrences by component, in order.
        let mut taken = vec![false; decoded.occurrences.as_ref().map_or(0, Vec::len)];
        for o in reference.occurrences.iter().flatten() {
            let (Some(component), Some(expected)) = (o.component.clone().flatten(), o.transform)
            else {
                continue;
            };
            let found = decoded
                .occurrences
                .iter()
                .flatten()
                .enumerate()
                .find(|(k, d)| {
                    !taken[*k] && d.component.clone().flatten().as_deref() == Some(&component)
                });
            let Some((k, node)) = found else {
                continue;
            };
            taken[k] = true;
            let Some(last) = node
                .f3d
                .as_ref()
                .and_then(|f| f.placements.as_ref())
                .and_then(|p| p.last())
                .and_then(|p| p.transform)
            else {
                continue;
            };
            let close = (0..4).all(|r| (0..4).all(|c| (last[r][c] - expected[r][c]).abs() < 1e-9));
            if close {
                agree += 1;
            } else {
                differ.push(format!(
                    "{id}: {component}: decoded {last:?}, dump {expected:?}"
                ));
            }
        }
        let detail = |item: &ir::TimelineItem| {
            item.detail
                .as_ref()
                .and_then(|d| serde_json::to_value(d).ok())
                .unwrap_or_default()
        };
        let twin = |index: Option<i64>| {
            decoded
                .timeline_items()
                .iter()
                .find(|d| d.index == index)
                .cloned()
        };
        for item in of_kind(&reference, "Joint") {
            let expected = detail(item)["jointMotion"]["_type"].clone();
            let got = twin(item.index).map(|t| detail(&t)["jointMotion"]["_type"].clone());
            if got.as_ref() == Some(&expected) {
                agree += 1;
            } else {
                differ.push(format!(
                    "{id}: joint {}: decoded {got:?}, dump {expected}",
                    item.name().unwrap_or("?")
                ));
            }
        }
        for item in of_kind(&reference, "RigidGroup") {
            let names = |v: &serde_json::Value| {
                let mut n: Vec<String> = v["occurrences"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|o| o["component"].as_str().map(str::to_owned))
                    .collect();
                n.sort();
                n
            };
            let expected = names(&detail(item));
            let got = twin(item.index)
                .filter(|t| t.object_type() == Some("RigidGroup"))
                .map(|t| names(&detail(&t)));
            // With "include children" the file lists the members'
            // children too.
            if got
                .as_ref()
                .is_some_and(|g| expected.iter().all(|e| g.contains(e)))
            {
                agree += 1;
            } else {
                differ.push(format!(
                    "{id}: rigid group {}: decoded {got:?}, dump {expected:?}",
                    item.name().unwrap_or("?")
                ));
            }
        }
    }
    println!(
        "placements, joints and rigid groups: {agree} agree, {} differ",
        differ.len()
    );
    assert!(differ.is_empty(), "{differ:#?}");
}

/// Extrusions' extents (mitcad#96): the object of an extent up to an
/// object (a face found by its names at the dump's point, a plane by its
/// name) and a symmetric extent's length.
#[test]
fn extrusion_extents_match_the_reference_dumps() {
    let Some(dir) = models_dir() else {
        eprintln!("reference models not found; skipped");
        return;
    };
    let (mut agree, mut differ) = (0usize, Vec::new());
    for (id, f3d, json) in models(&dir) {
        let text = std::fs::read_to_string(&json).unwrap();
        let reference = ir::Dump::from_json(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let designs = design::decode_path(&f3d).unwrap_or_else(|e| panic!("{id}: {e}"));
        let Some(decoded) = designs.into_iter().find_map(|d| d.dump) else {
            panic!("{id}: no design decoded");
        };
        let twins = of_kind(&decoded, "ExtrudeFeature");
        for (k, item) in of_kind(&reference, "ExtrudeFeature")
            .into_iter()
            .enumerate()
        {
            let theirs = serde_json::to_value(item.detail.as_ref()).unwrap_or_default();
            let Some(twin) = twins.get(k) else {
                differ.push(format!("{id}: {}: not decoded", item.name().unwrap_or("?")));
                continue;
            };
            let mine = serde_json::to_value(twin.detail.as_ref()).unwrap_or_default();
            let raw = twin.f3d.as_ref().and_then(|f| f.extrude.as_ref());
            let ok = if theirs["extentOne"]["_type"] == "ToEntityExtentDefinition" {
                let (a, b) = (&mine["extentOne"]["entity"], &theirs["extentOne"]["entity"]);
                let near = |x: &serde_json::Value, y: &serde_json::Value| {
                    (0..3).all(|i| match (x[i].as_f64(), y[i].as_f64()) {
                        (Some(p), Some(q)) => (p - q).abs() < 1e-6,
                        _ => false,
                    })
                };
                mine["extentOne"]["_type"] == "ToEntityExtentDefinition"
                    && match b["kind"].as_str() {
                        Some("face") => {
                            a["kind"] == "face" && near(&a["point_on_face"], &b["point_on_face"])
                        }
                        _ => reference_key(a) == reference_key(b),
                    }
            } else if theirs["symmetricExtent"]["_type"] == "SymmetricExtentDefinition" {
                raw.and_then(|r| r.full_length)
                    == theirs["symmetricExtent"]["isFullLength"].as_bool()
            } else {
                continue;
            };
            if ok {
                agree += 1;
            } else {
                differ.push(format!(
                    "{id}: {}: {} / {}",
                    item.name().unwrap_or("?"),
                    mine["extentOne"],
                    theirs["extentOne"]
                ));
            }
        }
    }
    println!("extrusion extents: {agree} agree, {} differ", differ.len());
    for d in &differ {
        println!("  {d}");
    }
    assert!(differ.is_empty(), "{differ:#?}");
}
