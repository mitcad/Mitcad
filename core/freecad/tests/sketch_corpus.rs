// SPDX-License-Identifier: MIT
//! Sketches of the FreeCAD documents under `MITCAD_FCSTD_CORPUS` (a path
//! list; not part of the repository, the test is skipped without it)
//! against FreeCAD's own reading of them in their dumps
//! (`tools/freecad-export/dump.py`, `sketch`): each constraint's type,
//! references, value, name and flags; each geometry's type, construction
//! flag and points (start, end, centre as the sketcher names them); the
//! external links.

use std::path::{Path, PathBuf};

use mitcad_freecad::sketch::{Curve, PointPos};
use mitcad_freecad::{FcstdFile, Sketch};
use serde_json::Value;

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(&p, out);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("fcstd"))
            && p.with_extension("json").is_file()
        {
            out.push(p);
        }
    }
}

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
}

/// The differences between one sketch and FreeCAD's dump of it.
fn compare(name: &str, sketch: &Sketch, dump: &Value, out: &mut Vec<String>) {
    let mut differ = |what: String| out.push(format!("{name}: {what}"));
    for p in &sketch.problems {
        differ(format!("not read: {p}"));
    }
    let constraints = dump["constraints"].as_array().cloned().unwrap_or_default();
    if constraints.len() != sketch.constraints.len() {
        differ(format!(
            "{} constraints against FreeCAD's {}",
            sketch.constraints.len(),
            constraints.len()
        ));
    }
    for (i, (c, d)) in sketch.constraints.iter().zip(&constraints).enumerate() {
        let refs = [
            (c.first, "first", "first_pos"),
            (c.second, "second", "second_pos"),
            (c.third, "third", "third_pos"),
        ];
        let mut same = c.kind.name() == d["type"].as_str().unwrap_or("?")
            && c.name == d["name"].as_str().unwrap_or("")
            && close(c.value, number(&d["value"]))
            && c.driving == d["driving"].as_bool().unwrap_or(true)
            && d["active"].as_bool().is_none_or(|a| a == c.active);
        for (r, geo, pos) in refs {
            same &= d[geo].as_i64() == Some(i64::from(r.geo))
                && d[pos].as_i64() == Some(i64::from(r.pos.index()));
        }
        if !same {
            differ(format!("constraint {i}: {c:?} against FreeCAD's {d}"));
        }
    }
    let geometry = dump["geometry"].as_array().cloned().unwrap_or_default();
    if geometry.len() != sketch.geometry.len() {
        differ(format!(
            "{} geometries against FreeCAD's {}",
            sketch.geometry.len(),
            geometry.len()
        ));
    }
    for (i, (g, d)) in sketch.geometry.iter().zip(&geometry).enumerate() {
        if g.curve.type_name() != d["type"].as_str().unwrap_or("?") {
            differ(format!(
                "geometry {i}: {} against FreeCAD's {}",
                g.curve.type_name(),
                d["type"]
            ));
            continue;
        }
        if let Some(construction) = d["construction"].as_bool()
            && construction != g.construction
        {
            differ(format!("geometry {i}: construction {}", g.construction));
        }
        if matches!(g.curve, Curve::Other { .. }) {
            continue;
        }
        for (pos, key) in [
            (PointPos::Start, "start"),
            (PointPos::End, "end"),
            (PointPos::Mid, "mid"),
        ] {
            let (Some(mine), Some(theirs)) = (g.curve.point(pos), d["points"][key].as_array())
            else {
                continue;
            };
            let far = (0..2).any(|k| (mine[k] - number(&theirs[k])).abs() > 1e-9);
            if far {
                differ(format!(
                    "geometry {i} ({}): {key} {mine:?} against FreeCAD's {theirs:?}",
                    g.curve.type_name()
                ));
            }
        }
    }
    let links: Vec<String> = sketch
        .links
        .iter()
        .map(|l| format!("{}.{}", l.object, l.element))
        .collect();
    let theirs: Vec<String> = dump["external"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            format!(
                "{}.{}",
                e[0].as_str().unwrap_or("?"),
                e[1].as_str().unwrap_or("?")
            )
        })
        .collect();
    if links != theirs {
        differ(format!("external {links:?} against FreeCAD's {theirs:?}"));
    }
    if sketch.external.len() != sketch.links.len() && sketch.external.iter().all(Option::is_none) {
        differ("external geometry without its links".to_owned());
    }
}

#[test]
fn sketches_read_as_freecad_reads_them() {
    let Some(paths) = std::env::var_os("MITCAD_FCSTD_CORPUS") else {
        eprintln!("MITCAD_FCSTD_CORPUS not set; skipped");
        return;
    };
    let mut files = Vec::new();
    for dir in std::env::split_paths(&paths) {
        collect(&dir, &mut files);
    }
    let mut sketches = 0;
    let mut differences = Vec::new();
    for path in &files {
        let dump: Value =
            serde_json::from_str(&std::fs::read_to_string(path.with_extension("json")).unwrap())
                .unwrap();
        // Dumps older than format 3 have no sketches.
        if dump["version"].as_u64().unwrap_or(0) < 3 {
            continue;
        }
        let file = FcstdFile::open(path).unwrap();
        for o in &file.document.objects {
            if !o.type_name.starts_with("Sketcher::SketchObject") {
                continue;
            }
            let Some(d) = dump["objects"]
                .as_array()
                .and_then(|a| a.iter().find(|x| x["name"] == o.name.as_str()))
            else {
                continue;
            };
            if d["sketch"].get("geometry").is_none() {
                continue;
            }
            sketches += 1;
            let name = format!("{}: {}", path.display(), o.name);
            compare(&name, &Sketch::of(o), &d["sketch"], &mut differences);
        }
    }
    eprintln!("{sketches} sketches compared");
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
