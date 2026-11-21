// SPDX-License-Identifier: MIT
//! The design's sketch blocks (mitcad#2110, `mitcad_model::sketch::block`):
//! Create Block, Insert Block, Explode Block, Edit Block and the blocks'
//! names, each one undo step. An edited block makes every instance follow,
//! in every sketch and in the blocks that hold it.

use std::collections::BTreeSet;
use std::sync::Arc;

use super::{DocState, Document, FeatureEntry, ModelError, invalid};
use crate::features::{FeatureDef, SketchDef, SketchPlane, ValueInput};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;
use crate::sketch::ConstraintUid;
use crate::sketch::block::{SketchBlock, holds, sync_nested};
use crate::sketch::edit::{EditReport, SketchEdit};
use crate::sketch::geometry::P2;
use crate::sketch::modify::Item;

/// What a block command did.
#[derive(Debug, Clone, Default)]
pub struct BlockEdit {
    /// The sketch it edited (the one an edit of a block opened in).
    pub sketch: Option<FeatureUid>,
    pub block: u32,
    /// The instance it made, opened or exploded.
    pub instance: Option<ConstraintUid>,
    /// The edit of the sketch.
    pub report: EditReport,
    /// What the user is to know, in words.
    pub warnings: Vec<String>,
}

/// A count in words: "a constraint" or "3 constraints".
fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_owned()
    } else {
        format!("{n} {many}")
    }
}

impl DocState {
    /// A block by its name or its number.
    pub fn block_id(&self, block: &str) -> Result<u32, ModelError> {
        if let Some(b) = self.sketch_blocks.iter().find(|b| b.name == block) {
            return Ok(b.id);
        }
        block
            .parse::<u32>()
            .ok()
            .filter(|n| self.sketch_blocks.iter().any(|b| b.id == *n))
            .ok_or_else(|| invalid(format!("there is no block {block}")))
    }

    fn block(&self, id: u32) -> Result<&SketchBlock, ModelError> {
        self.sketch_blocks
            .iter()
            .find(|b| b.id == id)
            .ok_or_else(|| invalid(format!("there is no block {id}")))
    }

    /// How many instances of block `id` there are, in sketches and in
    /// other blocks.
    pub fn block_instance_count(&self, id: u32) -> usize {
        let in_sketches: usize = self
            .features
            .iter()
            .filter_map(|f| match &f.def {
                FeatureDef::Sketch(def) => {
                    Some(def.blocks.iter().filter(|b| b.block == id).count())
                }
                _ => None,
            })
            .sum();
        let in_blocks: usize = self
            .sketch_blocks
            .iter()
            .map(|b| b.instances.iter().filter(|i| i.block == id).count())
            .sum();
        in_sketches + in_blocks
    }

    /// The sketch that has a block open for an edit, and that instance.
    pub fn open_block_edit(&self) -> Option<(FeatureUid, ConstraintUid, u32)> {
        self.features.iter().find_map(|f| match &f.def {
            FeatureDef::Sketch(def) => def
                .blocks
                .iter()
                .find(|b| b.open.is_some())
                .map(|b| (f.uid, b.id, b.block)),
            _ => None,
        })
    }

    /// A free name `Block<n>`.
    fn free_block_name(&self) -> String {
        (1..)
            .map(|n| format!("Block{n}"))
            .find(|name| !self.sketch_blocks.iter().any(|b| &b.name == name))
            .expect("a free name")
    }

    fn check_block_name(&self, name: &str, except: Option<u32>) -> Result<String, ModelError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(invalid("the block needs a name"));
        }
        if self
            .sketch_blocks
            .iter()
            .any(|b| b.name == name && Some(b.id) != except)
        {
            return Err(invalid(format!("a block named {name} exists")));
        }
        Ok(name.to_owned())
    }

    /// A sketch edit inside a document edit, recorded as a step of `kind`.
    fn edit_sketch_in<T>(
        &mut self,
        sketch: FeatureUid,
        kind: Option<&str>,
        edit: impl FnOnce(&mut SketchEdit<'_>) -> Result<T, String>,
    ) -> Result<(T, EditReport), ModelError> {
        if let Some(component) = self.entry(sketch).map(|e| e.component) {
            self.editable(component)?;
        }
        let position = self.require(sketch)?;
        let entry = self.features[position].clone();
        let FeatureDef::Sketch(def) = &entry.def else {
            return Err(invalid(format!("{} is not a sketch", entry.name)));
        };
        let before = crate::sketch::steps::Items::of(def);
        let (value, def, report) = {
            let mut editor =
                SketchEdit::new(def.clone(), &mut self.parameters, sketch, &entry.name);
            let value = edit(&mut editor).map_err(invalid)?;
            let (mut def, mut report) = editor.finish().map_err(invalid)?;
            report.step = crate::sketch::steps::record(&mut def, &before, kind, &[]);
            (value, def, report)
        };
        let edited = FeatureEntry {
            def: FeatureDef::Sketch(def),
            ..(*entry).clone()
        };
        self.check_feature(position, &edited)
            .map_err(ModelError::Invalid)?;
        self.features[position] = Arc::new(edited);
        self.release_params(sketch);
        Ok((value, report))
    }

    /// Every instance of block `changed` made again from it: in the blocks
    /// that hold it (and so on outward), then in the sketches. Returns how
    /// many constraints of sketches lost their geometry.
    fn follow_block(&mut self, changed: u32) -> Result<usize, ModelError> {
        let mut todo = vec![changed];
        let mut done: BTreeSet<u32> = BTreeSet::new();
        while let Some(b) = todo.pop() {
            if !done.insert(b) {
                continue;
            }
            let edited = self.block(b)?.clone();
            for outer in &mut self.sketch_blocks {
                if outer.instances.iter().any(|n| n.block == b)
                    && sync_nested(outer, &edited).map_err(invalid)?
                {
                    todo.push(outer.id);
                }
            }
        }
        let blocks: Vec<SketchBlock> = self
            .sketch_blocks
            .iter()
            .filter(|b| done.contains(&b.id))
            .cloned()
            .collect();
        let sketches: Vec<FeatureUid> = self
            .features
            .iter()
            .filter(|f| {
                matches!(&f.def, FeatureDef::Sketch(d) if d.blocks.iter().any(|i| done.contains(&i.block)))
            })
            .map(|f| f.uid)
            .collect();
        let mut lost = 0;
        for sketch in sketches {
            let (n, _) = self.edit_sketch_in(sketch, None, |edit| {
                let mut n = 0;
                for b in &blocks {
                    n += edit.sync_block(b)?;
                }
                Ok(n)
            })?;
            lost += n;
        }
        Ok(lost)
    }
}

impl<K: Kernel> Document<K> {
    /// The design's sketch blocks, in the order they were made.
    pub fn sketch_blocks(&self) -> &[SketchBlock] {
        &self.state.sketch_blocks
    }

    /// A block's number by its name (or its number as text).
    pub fn sketch_block_id(&self, block: &str) -> Result<u32, ModelError> {
        self.state.block_id(block)
    }

    /// How many instances of a block there are.
    pub fn sketch_block_instances(&self, id: u32) -> usize {
        self.state.block_instance_count(id)
    }

    /// The sketch with a block open for an edit, its instance and block.
    pub fn open_sketch_block(&self) -> Option<(FeatureUid, ConstraintUid, u32)> {
        self.state.open_block_edit()
    }

    /// Create Block: `items` of a sketch become block `name` (Block<n>
    /// without one), its base point at `base` and its base direction
    /// `angle` radians from the sketch's x axis; the selection becomes an
    /// instance of it in place.
    pub fn add_sketch_block(
        &mut self,
        sketch: FeatureUid,
        items: &[Item],
        name: Option<&str>,
        base: P2,
        angle: f64,
    ) -> Result<BlockEdit, ModelError> {
        self.apply(|state| {
            let name = match name {
                Some(name) => state.check_block_name(name, None)?,
                None => state.free_block_name(),
            };
            let id = state.sketch_blocks.iter().map(|b| b.id).max().unwrap_or(0) + 1;
            // The sketch's constraints that place the selection stay when
            // the sketch still solves with them.
            let made = state
                .edit_sketch_in(sketch, Some("create_block"), |edit| {
                    edit.make_block(id, &name, items, base, angle, true)
                })
                .or_else(|_| {
                    state.edit_sketch_in(sketch, Some("create_block"), |edit| {
                        edit.make_block(id, &name, items, base, angle, false)
                    })
                });
            let ((block, instance), report) = made?;
            state.sketch_blocks.push(block);
            Ok((
                format!("Create Block {name}"),
                BlockEdit {
                    sketch: Some(sketch),
                    block: id,
                    instance: Some(instance),
                    report,
                    warnings: Vec::new(),
                },
            ))
        })
    }

    /// Blocks a paste from another design brings (numbered for this
    /// design, `mitcad_model_api`'s paste resolves them), one undo step.
    pub fn add_pasted_sketch_blocks(&mut self, blocks: Vec<SketchBlock>) -> Result<(), ModelError> {
        if blocks.is_empty() {
            return Ok(());
        }
        let mut all = self.state.sketch_blocks.clone();
        all.extend(blocks);
        crate::sketch::block::check_blocks(&all).map_err(invalid)?;
        self.apply(|state| {
            state.sketch_blocks = all;
            Ok(("Add Blocks".to_owned(), ()))
        })
    }

    pub fn rename_sketch_block(&mut self, block: &str, new_name: &str) -> Result<(), ModelError> {
        let id = self.state.block_id(block)?;
        let new_name = self.state.check_block_name(new_name, Some(id))?;
        let old = self.state.block(id)?.name.clone();
        if old == new_name {
            return Ok(());
        }
        self.apply(|state| {
            if let Some(b) = state.sketch_blocks.iter_mut().find(|b| b.id == id) {
                b.name = new_name;
            }
            Ok((format!("Rename Block {old}"), ()))
        })
    }

    /// Deletes a block; refused while it has instances.
    pub fn remove_sketch_block(&mut self, block: &str) -> Result<(), ModelError> {
        let id = self.state.block_id(block)?;
        let name = self.state.block(id)?.name.clone();
        let count = self.state.block_instance_count(id);
        if count > 0 {
            return Err(invalid(format!(
                "block {name} cannot be deleted: it has {}; explode or delete them first",
                counted(count, "an instance", "instances")
            )));
        }
        self.apply(|state| {
            state.sketch_blocks.retain(|b| b.id != id);
            Ok((format!("Delete Block {name}"), ()))
        })
    }

    /// Insert Block: an instance of `block` in a sketch, its base point at
    /// `at`, turned by `angle`.
    pub fn insert_sketch_block(
        &mut self,
        sketch: FeatureUid,
        block: &str,
        at: P2,
        angle: f64,
    ) -> Result<BlockEdit, ModelError> {
        let id = self.state.block_id(block)?;
        // Not into the block being edited, nor one that holds it.
        if let Some((open_in, _, open)) = self.state.open_block_edit()
            && open_in == sketch
            && holds(&self.state.sketch_blocks, id, open)
        {
            return Err(invalid(
                "a block cannot hold an instance of itself, nor of a block that holds it",
            ));
        }
        self.apply(|state| {
            let definition = state.block(id)?.clone();
            let (instance, report) =
                state.edit_sketch_in(sketch, Some("insert_block"), |edit| {
                    edit.insert_block(&definition, at, angle)
                })?;
            Ok((
                format!("Insert Block {}", definition.name),
                BlockEdit {
                    sketch: Some(sketch),
                    block: id,
                    instance: Some(instance),
                    report,
                    warnings: Vec::new(),
                },
            ))
        })
    }

    /// The instance `instance` of a sketch, with its block.
    fn instance_block(
        &self,
        sketch: FeatureUid,
        instance: ConstraintUid,
    ) -> Result<SketchBlock, ModelError> {
        let def = match self.state.entry(sketch).map(|e| &e.def) {
            Some(FeatureDef::Sketch(def)) => def,
            _ => return Err(invalid(format!("feature {sketch} is not a sketch"))),
        };
        let record = def
            .blocks
            .iter()
            .find(|b| b.id == instance)
            .ok_or_else(|| invalid("the block instance is not in the sketch"))?;
        Ok(self.state.block(record.block)?.clone())
    }

    /// Explode Block: an instance becomes plain sketch geometry with its
    /// block's constraints and dimensions.
    pub fn explode_sketch_block(
        &mut self,
        sketch: FeatureUid,
        instance: ConstraintUid,
    ) -> Result<BlockEdit, ModelError> {
        let block = self.instance_block(sketch, instance)?;
        self.apply(|state| {
            let (left_out, report) =
                state.edit_sketch_in(sketch, Some("explode_block"), |edit| {
                    edit.explode_block(instance, &block)
                })?;
            let mut warnings = Vec::new();
            if left_out > 0 {
                warnings.push(format!(
                    "{} of the block left out: a turned instance has no horizontal or vertical \
                     of its block",
                    counted(
                        left_out,
                        "a constraint or dimension",
                        "constraints and dimensions"
                    )
                ));
            }
            Ok((
                format!("Explode Block {}", block.name),
                BlockEdit {
                    sketch: Some(sketch),
                    block: block.id,
                    instance: Some(instance),
                    report,
                    warnings,
                },
            ))
        })
    }

    /// Edit Block: opens a block for an edit. With `at` (a sketch and an
    /// instance of the block in it) in place, its geometry free and the
    /// block's constraints and dimensions in the sketch; without, in a
    /// sketch made for the edit (on XY, the block's base point at the
    /// origin), which goes when the edit ends.
    pub fn edit_sketch_block(
        &mut self,
        block: &str,
        at: Option<(FeatureUid, ConstraintUid)>,
    ) -> Result<BlockEdit, ModelError> {
        let id = self.state.block_id(block)?;
        if self.state.open_block_edit().is_some() {
            return Err(invalid(
                "a block is being edited: finish it (Finish Block) before editing another",
            ));
        }
        let definition = self.state.block(id)?.clone();
        if let Some((sketch, instance)) = at
            && self.instance_block(sketch, instance)?.id != id
        {
            return Err(invalid("the instance is not of this block"));
        }
        let component = self.state.assembly.active;
        self.apply(|state| {
            let (sketch, instance, temporary) = match at {
                Some((sketch, instance)) => (sketch, instance, false),
                None => {
                    let name = (1..)
                        .map(|n| {
                            if n == 1 {
                                format!("Block {}", definition.name)
                            } else {
                                format!("Block {} {n}", definition.name)
                            }
                        })
                        .find(|n| !state.features.iter().any(|f| &f.name == n))
                        .expect("a free name");
                    let def = FeatureDef::Sketch(SketchDef::<ValueInput>::new(SketchPlane::Xy));
                    let added = state.add_feature_in(&def, Some(&name), component)?;
                    let (instance, _) = state.edit_sketch_in(added.uid, None, |edit| {
                        edit.insert_block(&definition, [0.0, 0.0], 0.0)
                    })?;
                    (added.uid, instance, true)
                }
            };
            let (left_out, report) = state.edit_sketch_in(sketch, None, |edit| {
                edit.open_block(instance, &definition, temporary)
            })?;
            let mut warnings = Vec::new();
            if left_out > 0 {
                warnings.push(format!(
                    "{} of the block left out while it is edited here: a turned instance has no \
                     horizontal or vertical of its block; edit the block on its own to keep them",
                    counted(
                        left_out,
                        "a constraint or dimension",
                        "constraints and dimensions"
                    )
                ));
            }
            Ok((
                format!("Edit Block {}", definition.name),
                BlockEdit {
                    sketch: Some(sketch),
                    block: id,
                    instance: Some(instance),
                    report,
                    warnings,
                },
            ))
        })
    }

    /// Finish Block: the open block's geometry becomes the block, and every
    /// instance follows; with `cancel`, the block stays as it was. A sketch
    /// made for the edit goes.
    pub fn finish_sketch_block(&mut self, cancel: bool) -> Result<BlockEdit, ModelError> {
        let (sketch, instance, id) = self
            .state
            .open_block_edit()
            .ok_or_else(|| invalid("no block is being edited"))?;
        let definition = self.state.block(id)?.clone();
        self.apply(|state| {
            let temporary = match state.entry(sketch).map(|e| &e.def) {
                Some(FeatureDef::Sketch(def)) => def
                    .blocks
                    .iter()
                    .find(|b| b.id == instance)
                    .and_then(|b| b.open.as_ref())
                    .is_some_and(|o| o.temporary),
                _ => false,
            };
            let mut warnings = Vec::new();
            let report = if cancel {
                state
                    .edit_sketch_in(sketch, None, |edit| {
                        edit.cancel_block(instance, &definition)
                    })?
                    .1
            } else {
                let (closed, report) =
                    state.edit_sketch_in(sketch, Some("edit_block"), |edit| {
                        edit.close_block(instance)
                    })?;
                // Not a block holding itself.
                let mut blocks = state.sketch_blocks.clone();
                if let Some(b) = blocks.iter_mut().find(|b| b.id == id) {
                    b.content = closed.content.clone();
                    b.instances = closed.instances.clone();
                }
                if closed.instances.iter().any(|n| holds(&blocks, n.block, id)) {
                    return Err(invalid(
                        "a block cannot hold an instance of itself, nor of a block that holds it",
                    ));
                }
                state.sketch_blocks = blocks;
                let lost = state.follow_block(id)?;
                if lost > 0 {
                    warnings.push(format!(
                        "{} lost {} geometry with the edit of the block",
                        counted(lost, "a constraint", "constraints"),
                        if lost == 1 { "its" } else { "their" }
                    ));
                }
                report
            };
            if temporary {
                state.remove_features(&BTreeSet::from([sketch]));
                state.release_params(sketch);
            }
            Ok((
                format!("Edit Block {}", definition.name),
                BlockEdit {
                    sketch: (!temporary).then_some(sketch),
                    block: id,
                    instance: Some(instance),
                    report,
                    warnings,
                },
            ))
        })
    }
}
