// SPDX-License-Identifier: MIT
//! Command-line inspector for IFC files (`.ifc`, `.ifczip`): the header,
//! the entity types, the spatial tree with its products, single entities
//! with what they refer to, the units, and the decoded shape items of a
//! product.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use mitcad_ifc::geometry::{Decoder, Item};
use mitcad_ifc::math;
use mitcad_ifc::model::{Model, Product};
use mitcad_ifc::schema::ProductKind;
use mitcad_ifc::{IfcFile, Value};

const USAGE: &str = "usage: mitcad-ifc-inspect <command> <file> ...
  header <file>                 the header: schema, view definition, origin
  entities <file>               instances per type, most first
  tree <file>                   the spatial structure with its products
                                (type, name, GlobalId, shape)
  products <file> [--json]      every product: kind, container, openings,
                                placement, representation
  entity <file> <id> [depth]    an instance and those it refers to (to
                                depth, 2 by default)
  units <file>                  the units as millimeters and radians
  items <file> <id> [--json]    the decoded body items of product <id>";

fn open(path: &str) -> Result<IfcFile, String> {
    IfcFile::open(Path::new(path)).map_err(|e| format!("{path}: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["header", path] => header(path),
        ["entities", path] => entities(path),
        ["tree", path] => tree(path),
        ["products", path] => products(path, false),
        ["products", path, "--json"] => products(path, true),
        ["entity", path, id] => entity(path, id, 2),
        ["entity", path, id, depth] => match depth.parse() {
            Ok(d) => entity(path, id, d),
            Err(_) => Err(USAGE.to_owned()),
        },
        ["units", path] => units(path),
        ["items", path, id] => items(path, id, false),
        ["items", path, id, "--json"] => items(path, id, true),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn header(path: &str) -> Result<(), String> {
    let f = open(path)?;
    let h = &f.header;
    println!("schema: {} ({})", h.schemas.join(", "), f.schema.name());
    println!(
        "view definition: {}",
        h.view_definition().unwrap_or_else(|| "-".to_owned())
    );
    println!("description: {}", h.description.join(" | "));
    println!("implementation level: {}", h.implementation_level);
    println!("name: {}", h.name);
    println!("time stamp: {}", h.time_stamp);
    println!("preprocessor: {}", h.preprocessor_version);
    println!("originating system: {}", h.originating_system);
    println!(
        "{} instances of {} types, {} bytes",
        f.entities.len(),
        f.counts.len(),
        f.bytes
    );
    for w in &f.warnings {
        println!("warning: {w}");
    }
    Ok(())
}

fn entities(path: &str) -> Result<(), String> {
    let f = open(path)?;
    let mut counts: Vec<(&str, usize)> = f.counts.iter().map(|(k, v)| (&**k, *v)).collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (kind, n) in counts {
        println!("{n:8} {kind}");
    }
    Ok(())
}

fn shape_summary(file: &IfcFile, model: &Model, p: &Product) -> String {
    match model.body(file, p) {
        Some(r) => {
            let kinds: Vec<&str> = r.items.iter().filter_map(|i| file.kind(*i)).collect();
            format!("{}/{}: {}", r.identifier, r.kind, kinds.join(", "))
        }
        None if p.shape.is_some() => {
            let reps: Vec<String> = model
                .representations(file, p)
                .iter()
                .map(|r| format!("{}/{}", r.identifier, r.kind))
                .collect();
            format!("no body ({})", reps.join(", "))
        }
        None => "no shape".to_owned(),
    }
}

fn tree(path: &str) -> Result<(), String> {
    let f = open(path)?;
    let model = Model::read(&f);
    println!(
        "project #{} {}",
        model.project.unwrap_or(0),
        model.project_name
    );
    let mut placed = std::collections::HashSet::new();
    fn show(
        f: &IfcFile,
        model: &Model,
        id: u64,
        depth: usize,
        placed: &mut std::collections::HashSet<u64>,
    ) {
        let Some(p) = model.products.get(&id) else {
            return;
        };
        if !placed.insert(id) {
            return;
        }
        let pad = "  ".repeat(depth);
        println!(
            "{pad}#{} {} '{}' {} [{}]{}",
            p.id,
            p.kind,
            p.name,
            p.global_id,
            shape_summary(f, model, p),
            match p.elevation {
                Some(e) => format!(" elevation {e} mm"),
                None => String::new(),
            }
        );
        let mut children: Vec<u64> = p.parts.clone();
        children.extend(p.contents.iter().copied());
        children.extend(p.openings.iter().copied());
        children.extend(p.fillings.iter().copied());
        for c in children {
            show(f, model, c, depth + 1, placed);
        }
    }
    for root in &model.roots {
        show(&f, &model, *root, 1, &mut placed);
    }
    let others: Vec<&Product> = model
        .products
        .values()
        .filter(|p| !placed.contains(&p.id))
        .collect();
    if !others.is_empty() {
        println!("outside the spatial structure:");
        for p in others {
            show(&f, &model, p.id, 1, &mut placed);
        }
    }
    for n in &model.notes {
        println!("note: {n}");
    }
    Ok(())
}

fn products(path: &str, json: bool) -> Result<(), String> {
    let f = open(path)?;
    let model = Model::read(&f);
    if json {
        let list: Vec<&Product> = model.products.values().collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let mut by_kind: BTreeMap<ProductKind, usize> = BTreeMap::new();
    for p in model.products.values() {
        *by_kind.entry(p.class).or_default() += 1;
        println!(
            "#{} {} {} '{}' {} in #{} at {:?}{} [{}]",
            p.id,
            p.class.name(),
            p.kind,
            p.name,
            p.global_id,
            model.spatial_parent(p.id).unwrap_or(0),
            math::origin(&p.placement),
            match &p.placement_note {
                Some(n) => format!(" ({n})"),
                None => String::new(),
            },
            shape_summary(&f, &model, p),
        );
    }
    let counts: Vec<String> = by_kind
        .iter()
        .map(|(k, n)| format!("{n} {}", k.name()))
        .collect();
    println!("{} products: {}", model.products.len(), counts.join(", "));
    Ok(())
}

fn entity(path: &str, id: &str, depth: usize) -> Result<(), String> {
    let f = open(path)?;
    let id: u64 = id
        .trim_start_matches('#')
        .parse()
        .map_err(|_| format!("'{id}' is no instance number"))?;
    fn refs(v: &Value, out: &mut Vec<u64>) {
        match v {
            Value::Ref(id) => out.push(*id),
            Value::List(l) => l.iter().for_each(|v| refs(v, out)),
            Value::Typed(_, v) => refs(v, out),
            _ => {}
        }
    }
    fn show(f: &IfcFile, id: u64, depth: usize, left: usize, seen: &mut Vec<u64>) {
        let pad = "  ".repeat(depth);
        let Some(e) = f.get(id) else {
            println!("{pad}#{id} (not in the file)");
            return;
        };
        let args: Vec<String> = e.args.iter().map(ToString::to_string).collect();
        println!("{pad}#{}={}({})", e.id, e.kind, args.join(","));
        if left == 0 || seen.contains(&id) {
            return;
        }
        seen.push(id);
        let mut out = Vec::new();
        e.args.iter().for_each(|a| refs(a, &mut out));
        for r in out {
            show(f, r, depth + 1, left - 1, seen);
        }
    }
    show(&f, id, 0, depth, &mut Vec::new());
    Ok(())
}

fn units(path: &str) -> Result<(), String> {
    let f = open(path)?;
    let u = mitcad_ifc::units::Units::of(&f);
    println!("length: {} = {} mm", u.length_name, u.length);
    println!("plane angle: {} = {} rad", u.angle_name, u.angle);
    println!("volume: {} = {} mm3", u.volume_name, u.volume);
    println!("area: {} mm2", u.area);
    for n in &u.notes {
        println!("note: {n}");
    }
    Ok(())
}

fn items(path: &str, id: &str, json: bool) -> Result<(), String> {
    let f = open(path)?;
    let model = Model::read(&f);
    let id: u64 = id
        .trim_start_matches('#')
        .parse()
        .map_err(|_| format!("'{id}' is no instance number"))?;
    let p = model
        .products
        .get(&id)
        .ok_or_else(|| format!("#{id} is no product"))?;
    let rep = model
        .body(&f, p)
        .ok_or_else(|| format!("#{id} has no body representation"))?;
    let decoder = Decoder::new(&f, &model.units);
    let decoded: Vec<(u64, Item)> = rep.items.iter().map(|i| (*i, decoder.item(*i))).collect();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&decoded).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    println!(
        "#{} {} '{}': {}/{} at {:?}",
        p.id,
        p.kind,
        p.name,
        rep.identifier,
        rep.kind,
        math::rows(&p.placement)
    );
    for (i, item) in &decoded {
        match item {
            Item::Unsupported { id, kind, why } => {
                println!("  #{i}: not read: #{id} {kind}: {why}")
            }
            Item::Mapped { map, transform } => {
                let inner = model
                    .map_representation(&f, *map)
                    .map(|r| r.items.len())
                    .unwrap_or(0);
                println!(
                    "  #{i}: mapped item of map #{map} ({inner} items) by {:?}",
                    math::rows(transform)
                )
            }
            other => println!("  #{i}: {}", other.name()),
        }
        for (id, kind, why) in item.unsupported() {
            if !matches!(item, Item::Unsupported { .. }) {
                println!("    not read: #{id} {kind}: {why}");
            }
        }
    }
    Ok(())
}
