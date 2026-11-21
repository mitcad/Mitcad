// SPDX-License-Identifier: MIT
//! Sketch blocks (mitcad#2110, `mitcad_model::sketch::block`; see
//! `commands.md`, "Sketch blocks"): the design's commands
//! `add_sketch_block`, `rename_sketch_block`, `remove_sketch_block`,
//! `edit_sketch_block` and `finish_sketch_block`, the sketch's
//! `sketch.insert_block` and `sketch.explode_block`, and the `sketch_blocks`
//! query.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::sketch::report_json;
use mitcad_model::features::{FeatureDef, SketchDef};
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::sketch::ConstraintUid;
use mitcad_model::sketch::block::SketchBlock;
use mitcad_model::sketch::clipboard::Fragment;
use mitcad_model::sketch::modify::Item;
use mitcad_model::{BlockEdit, Document, ModelError};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum BlockCommand {
        // Create Block: a sketch's selection becomes a block and an
        // instance of it in place.
        "add_sketch_block" => Add {
            sketch: FeatureUid,
            /// Points and curves (`p3`, `c4`).
            items: Vec<String>,
            #[serde(default)]
            name: Option<String>,
            /// The base point, `[x, y]` in the sketch; the first selected
            /// point when left out.
            #[serde(default)]
            base: Option<[f64; 2]>,
            /// The base direction from the sketch's x axis, radians.
            #[serde(default)]
            angle: f64,
        },
        "rename_sketch_block" => Rename {
            block: String,
            name: String,
        },
        "remove_sketch_block" => Remove { block: String },
        // Edit Block: in place (`sketch` and `instance`) or in a sketch of
        // its own.
        "edit_sketch_block" => Edit {
            block: String,
            #[serde(default)]
            sketch: Option<FeatureUid>,
            #[serde(default)]
            instance: Option<ConstraintUid>,
        },
        // Finish Block: the open block taken, or left as it was (`cancel`).
        "finish_sketch_block" => Finish {
            #[serde(default)]
            cancel: bool,
        },
    }
}

/// The `sketch_blocks` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BlocksQuery {}

fn failed(error: ModelError) -> ApiError {
    ApiError(error.to_string())
}

/// A block command's answer: the sketch's edit as a sketch command's, with
/// the block, the instance and the sketch.
fn answer<K: Kernel>(doc: &Document<K>, edit: &BlockEdit) -> Value {
    let def = edit
        .sketch
        .and_then(|s| match doc.feature(s).map(|f| &f.def) {
            Some(FeatureDef::Sketch(def)) => Some(def),
            _ => None,
        });
    let mut value = report_json(&edit.report, def);
    value["block"] = json!(edit.block);
    if let Some(name) = doc
        .sketch_blocks()
        .iter()
        .find(|b| b.id == edit.block)
        .map(|b| b.name.clone())
    {
        value["name"] = json!(name);
    }
    if let Some(sketch) = edit.sketch {
        value["sketch"] = json!(sketch);
    }
    if let Some(instance) = edit.instance {
        value["instance"] = json!(instance);
    }
    if !edit.warnings.is_empty() {
        let mut all: Vec<Value> = value["warnings"].as_array().cloned().unwrap_or_default();
        all.extend(edit.warnings.iter().cloned().map(Value::String));
        value["warnings"] = json!(all);
    }
    value
}

/// The first point of the first selected item: the default base point.
fn first_point(def: &SketchDef, items: &[Item]) -> Option<[f64; 2]> {
    use mitcad_model::sketch::EntityKind;
    let at = |id| match def.entity(id)?.kind {
        EntityKind::Point { at } => Some(at),
        _ => None,
    };
    items.iter().find_map(|item| match item {
        Item::Entity(r) => {
            let e = def.entity(r.uid())?;
            match &e.kind {
                EntityKind::Point { at } => Some(*at),
                kind => kind
                    .ends()
                    .map(|(start, _)| start)
                    .or_else(|| kind.points().first().copied())
                    .and_then(at),
            }
        }
        _ => None,
    })
}

pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: BlockCommand,
) -> Result<Value, ApiError> {
    match command {
        BlockCommand::Add {
            sketch,
            items,
            name,
            base,
            angle,
        } => {
            let items = items
                .iter()
                .map(|i| i.parse::<Item>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(ApiError)?;
            let def = match doc.feature(sketch).map(|f| &f.def) {
                Some(FeatureDef::Sketch(def)) => def,
                _ => return Err(ApiError(format!("feature {sketch} is not a sketch"))),
            };
            let base = base
                .or_else(|| first_point(def, &items))
                .unwrap_or_default();
            let edit = doc
                .add_sketch_block(sketch, &items, name.as_deref(), base, angle)
                .map_err(failed)?;
            Ok(answer(doc, &edit))
        }
        BlockCommand::Rename { block, name } => {
            doc.rename_sketch_block(&block, &name).map_err(failed)?;
            Ok(json!({"name": name.trim()}))
        }
        BlockCommand::Remove { block } => {
            doc.remove_sketch_block(&block).map_err(failed)?;
            Ok(json!({"removed": block}))
        }
        BlockCommand::Edit {
            block,
            sketch,
            instance,
        } => {
            let at = match (sketch, instance) {
                (Some(sketch), Some(instance)) => Some((sketch, instance)),
                (None, None) => None,
                _ => {
                    return Err(ApiError(
                        "an edit in place names the sketch and the instance".to_owned(),
                    ));
                }
            };
            let edit = doc.edit_sketch_block(&block, at).map_err(failed)?;
            Ok(answer(doc, &edit))
        }
        BlockCommand::Finish { cancel } => {
            let edit = doc.finish_sketch_block(cancel).map_err(failed)?;
            Ok(answer(doc, &edit))
        }
    }
}

/// `sketch.insert_block`.
pub(crate) fn insert_block<K: Kernel>(
    doc: &mut Document<K>,
    sketch: FeatureUid,
    block: &str,
    at: [f64; 2],
    angle: f64,
) -> Result<Value, ApiError> {
    let edit = doc
        .insert_sketch_block(sketch, block, at, angle)
        .map_err(failed)?;
    Ok(answer(doc, &edit))
}

/// `sketch.explode_block`.
pub(crate) fn explode_block<K: Kernel>(
    doc: &mut Document<K>,
    sketch: FeatureUid,
    instance: ConstraintUid,
) -> Result<Value, ApiError> {
    let edit = doc.explode_sketch_block(sketch, instance).map_err(failed)?;
    Ok(answer(doc, &edit))
}

/// A block's curves as polylines in its own frame (the Insert Block list's
/// preview), and its size.
fn outline(block: &SketchBlock) -> (Vec<Value>, [f64; 4]) {
    let def = SketchDef {
        entities: block.content.entities.clone(),
        ..SketchDef::default()
    };
    let mut lines = Vec::new();
    let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    let solved = def.solve_with(&mut |_| Ok(0.0), &|_| String::new());
    if let Ok(solved) = solved {
        for (id, curve) in &solved.curves {
            if def.entity(*id).is_some_and(|e| e.construction) {
                continue;
            }
            let (t0, t1) = curve.domain();
            let samples = curve.sample(t0, t1);
            for p in &samples {
                bounds = [
                    bounds[0].min(p[0]),
                    bounds[1].min(p[1]),
                    bounds[2].max(p[0]),
                    bounds[3].max(p[1]),
                ];
            }
            lines.push(json!(samples));
        }
    }
    if bounds[0] > bounds[2] {
        bounds = [0.0; 4];
    }
    (lines, bounds)
}

/// The `sketch_blocks` answer: each block with its name, how many
/// instances it has (in sketches and in other blocks), the blocks it
/// holds, its outline in its own frame and its bounds, and where a block
/// is open for an edit.
pub(crate) fn blocks_json<K: Kernel>(doc: &Document<K>) -> Value {
    let blocks: Vec<Value> = doc
        .sketch_blocks()
        .iter()
        .map(|b| {
            let (lines, bounds) = outline(b);
            json!({
                "id": b.id,
                "name": b.name,
                "instances": doc.sketch_block_instances(b.id),
                "holds": b.holds(),
                "entities": b.content.entities.len(),
                "outline": lines,
                "bounds": bounds,
            })
        })
        .collect();
    let mut value = json!({ "blocks": blocks });
    if let Some((sketch, instance, block)) = doc.open_sketch_block() {
        value["open"] = json!({"sketch": sketch, "instance": instance, "block": block});
    }
    value
}

/// The blocks a copied fragment's instances use, with those they hold,
/// for a paste into another design.
pub(crate) fn add_fragment_blocks<K: Kernel>(doc: &Document<K>, fragment: &mut Fragment) {
    let mut wanted: Vec<u32> = fragment.instances.iter().map(|i| i.block).collect();
    let mut out: Vec<SketchBlock> = Vec::new();
    while let Some(id) = wanted.pop() {
        if out.iter().any(|b| b.id == id) {
            continue;
        }
        if let Some(b) = doc.sketch_blocks().iter().find(|b| b.id == id) {
            wanted.extend(b.holds());
            out.push(b.clone());
        }
    }
    fragment.blocks = out;
}

/// The blocks a pasted fragment brings, as this design's: a block of the
/// same name and content is the same; another of that name takes the
/// copy renamed; a missing one is added. Returns the fragment's block
/// numbers to this design's, and the blocks to add (already renumbered).
pub(crate) fn resolve_fragment_blocks<K: Kernel>(
    doc: &Document<K>,
    fragment: &Fragment,
) -> (BTreeMap<u32, u32>, Vec<SketchBlock>) {
    let mut map: BTreeMap<u32, u32> = BTreeMap::new();
    let mut added: Vec<SketchBlock> = Vec::new();
    let mut next = doc.sketch_blocks().iter().map(|b| b.id).max().unwrap_or(0) + 1;
    let same = |a: &SketchBlock, b: &SketchBlock| a.content == b.content;
    // Blocks without nested ones first, so that those that hold them see
    // their numbers.
    let mut todo: Vec<&SketchBlock> = fragment.blocks.iter().collect();
    let mut guard = 0;
    while !todo.is_empty() && guard <= fragment.blocks.len() {
        guard += 1;
        let mut rest = Vec::new();
        for b in todo {
            if !b.holds().iter().all(|n| map.contains_key(n)) {
                rest.push(b);
                continue;
            }
            let mut copy = b.clone();
            for n in &mut copy.instances {
                n.block = map[&n.block];
            }
            let existing = doc
                .sketch_blocks()
                .iter()
                .chain(added.iter())
                .find(|o| o.name == copy.name && same(o, &copy) && o.holds() == copy.holds())
                .map(|o| o.id);
            match existing {
                Some(id) => {
                    map.insert(b.id, id);
                }
                None => {
                    let taken = |name: &str| {
                        doc.sketch_blocks()
                            .iter()
                            .chain(added.iter())
                            .any(|o| o.name == name)
                    };
                    if taken(&copy.name) {
                        let base = copy.name.clone();
                        copy.name = (2..)
                            .map(|n| format!("{base} ({n})"))
                            .find(|n| !taken(n))
                            .expect("a free name");
                    }
                    copy.id = next;
                    next += 1;
                    map.insert(b.id, copy.id);
                    added.push(copy);
                }
            }
        }
        todo = rest;
    }
    (map, added)
}
