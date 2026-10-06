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
