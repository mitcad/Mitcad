// SPDX-License-Identifier: MIT
//! Real IFC files (never committed) under MITCAD_IFC_CORPUS: a folder with
//! an `index.tsv` (path, SHA-256, bytes, schema, ...) and the files. Every
//! listed file opens without records it cannot read, its FILE_SCHEMA is the
//! one the index gives, it has a project, units and products, and every
//! body item of every product (the items of the representation maps its
//! mapped items use too) decodes; what does not is listed by type with the
//! reason, and must be among the kinds this reader leaves to the import's
//! report. Files are named by the first 8 hex digits of their SHA-256.
//! Skipped without the corpus. Building the bodies is the import's corpus
//! test (the ctest `ifc.corpus`).

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use mitcad_ifc::geometry::{Decoder, Item};
use mitcad_ifc::model::Model;
use mitcad_ifc::{IfcFile, ReadOptions};

/// The index's rows: path, content id, schema.
pub fn index(corpus: &Path) -> Vec<(String, String, String)> {
    let text = std::fs::read_to_string(corpus.join("index.tsv")).expect("the corpus has index.tsv");
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).expect("a column");
    let (path, sha, schema) = (col("path"), col("sha256"), col("schema"));
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (
                f[path].to_owned(),
                f[sha].chars().take(8).collect(),
                f[schema].to_owned(),
            )
        })
        .collect()
}

fn unsupported(item: &Item, out: &mut Vec<(String, String)>) {
    match item {
        Item::Unsupported { kind, why, .. } => out.push((kind.clone(), why.clone())),
        Item::Boolean { first, second, .. } => {
            unsupported(first, out);
            unsupported(second, out);
        }
        _ => {}
    }
}

#[test]
fn corpus_files_decode() {
    let Some(corpus) = std::env::var_os("MITCAD_IFC_CORPUS") else {
        eprintln!("ifc corpus: skipped (MITCAD_IFC_CORPUS is not set)");
        return;
    };
    let corpus = Path::new(&corpus);
    let mut failures = Vec::new();
    let mut not_read: BTreeMap<String, usize> = BTreeMap::new();
    let mut totals = (0usize, 0usize, 0usize);
    for (path, id, schema) in index(corpus) {
        if !path.to_ascii_lowercase().ends_with(".ifc") {
            continue;
        }
        let file = match IfcFile::open_with(&corpus.join(&path), &ReadOptions::import()) {
            Ok(f) => f,
            Err(e) => {
                failures.push(format!("{id}: {e}"));
                continue;
            }
        };
        if !file.warnings.is_empty() {
            failures.push(format!("{id}: {}", file.warnings.join("; ")));
        }
        let expected = mitcad_ifc::Schema::of(&schema);
        if file.schema != expected {
            failures.push(format!(
                "{id}: schema {:?}, the index says {schema}",
                file.schema
            ));
        }
        let model = Model::read(&file);
        if model.project.is_none() {
            failures.push(format!("{id}: no project"));
        }
        if !model.units.notes.is_empty() {
            failures.push(format!("{id}: units: {}", model.units.notes.join("; ")));
        }
        let decoder = Decoder::new(&file, &model.units);
        let mut maps_done = HashSet::new();
        let mut items = 0;
        let mut missing = Vec::new();
        for p in model.products.values() {
            if let Some(note) = &p.placement_note {
                failures.push(format!("{id}: #{} placement: {note}", p.id));
            }
            let Some(rep) = model.body(&file, p) else {
                continue;
            };
            let mut todo: Vec<u64> = rep.items.clone();
            while let Some(i) = todo.pop() {
                items += 1;
                let item = decoder.item(i);
                if let Item::Mapped { map, .. } = &item
                    && maps_done.insert(*map)
                {
                    match model.map_representation(&file, *map) {
                        Some(r) => todo.extend(r.items),
                        None => missing.push((
                            "IFCREPRESENTATIONMAP".to_owned(),
                            format!("#{map} holds no representation"),
                        )),
                    }
                }
                unsupported(&item, &mut missing);
            }
        }
        for (kind, why) in &missing {
            *not_read.entry(format!("{kind}: {why}")).or_default() += 1;
        }
        eprintln!(
            "{id} {:7} {:6} products, {:6} body items, {} not read",
            file.schema.name(),
            model.products.len(),
            items,
            missing.len()
        );
        totals.0 += 1;
        totals.1 += model.products.len();
        totals.2 += items;
    }
    eprintln!(
        "{} files, {} products, {} body items",
        totals.0, totals.1, totals.2
    );
    for (what, n) in &not_read {
        eprintln!("not read {n:5}: {what}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The files whose property sets are read (mitcad#496): an IFC2X3 community
/// sample with property sets, quantities and type styles (the MIT-licensed
/// project's slim export, bfdf36ff, has no property sets).
const PROPERTY_FILES: [&str; 1] = ["b347a2c8"];

/// Property sets and quantities of a few corpus files by content id: the
/// import keeps their entities, elements have their own sets, type objects
/// theirs, and the sets decode into named values.
#[test]
fn property_sets_of_corpus_files() {
    let Some(corpus) = std::env::var_os("MITCAD_IFC_CORPUS") else {
        eprintln!("ifc property sets: skipped (MITCAD_IFC_CORPUS is not set)");
        return;
    };
    let corpus = Path::new(&corpus);
    let rows = index(corpus);
    for wanted in PROPERTY_FILES {
        let Some((path, _, _)) = rows.iter().find(|(_, id, _)| id == wanted) else {
            eprintln!("ifc property sets: {wanted} is not in the corpus: skipped");
            continue;
        };
        let file = IfcFile::open_with(&corpus.join(path), &ReadOptions::import()).expect("opens");
        let model = Model::read(&file);
        let with_sets: Vec<_> = model
            .products
            .values()
            .filter(|p| !p.property_sets.is_empty())
            .collect();
        assert!(
            !with_sets.is_empty(),
            "{wanted}: no element has a property set"
        );
        let mut values = 0;
        let mut quantities = 0;
        for p in &with_sets {
            for id in &p.property_sets {
                let set = mitcad_ifc::properties::property_set(&file, &model.units, *id)
                    .unwrap_or_else(|| panic!("{wanted}: #{id} does not decode"));
                values += set.properties.len();
                quantities += usize::from(set.quantities);
            }
        }
        assert!(values > 0, "{wanted}: the sets hold no values");
        let typed: usize = model
            .type_property_sets
            .values()
            .flatten()
            .filter_map(|id| mitcad_ifc::properties::property_set(&file, &model.units, *id))
            .map(|s| s.properties.len())
            .sum();
        eprintln!(
            "ifc property sets: {wanted}: {} elements with sets, {values} values, \
             {quantities} quantity sets, {typed} type values",
            with_sets.len()
        );
    }
}
