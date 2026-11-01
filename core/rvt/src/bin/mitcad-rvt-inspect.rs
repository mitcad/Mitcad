// SPDX-License-Identifier: MIT
//! Command-line inspector for `.rvt` and `.rfa` files: the compound file's
//! streams, the metadata, the schema, the element table and the element
//! records walked by the schema.

use std::path::Path;
use std::process::ExitCode;

use serde_json::json;

use mitcad_rvt::elements::{Model, category_counts, element_table};
use mitcad_rvt::schema::{KNOWN_UNKNOWNS, Schema};
use mitcad_rvt::{RvtFile, walk};

const USAGE: &str = "usage: mitcad-rvt-inspect <command> ...
  list <file>                     the compound file's streams with sizes
  info <file> [--json]            release, build, kind, the family's Atom
                                  data, linked files, preview, partitions,
                                  schema summary
  stream <file> <path> [--raw]    a stream's contents (inflated) as hex, or
                                  its stored bytes raw to standard output
  schema <file> [<class>] [--json]
                                  the classes (tag, base, fields with type
                                  words), or one class with its bases; the
                                  unknown encodings, the known unknowns
  table <file> [--json]           the element table: declared ids, owners
  elements <file> [--json|--csv] [--all]
                                  every declared element: class, category,
                                  level, family, type, transform, box (feet);
                                  without --all the text lists placed
                                  elements with a box only
  levels <file>                   the levels with their elevations (feet)
  materials <file>                the materials: name, color, transparency
  record <file> <id> [--all]      an element's header and object records
                                  walked by the schema, as JSON (--all: from
                                  every partition that has one)
  classes <file> [<class>]        the classes of the element records and of
                                  the objects behind their pointers, counted;
                                  with a class, the elements that hold one
  records <file> <class>[,<class>...]
                                  the latest records of the elements of
                                  those classes, one JSON line each
  building <file>                 the building elements' parameters (walls,
                                  floors, roofs, instances, types, grids,
                                  links), GlobalIds and the profile families'
                                  curves as JSON
  documents <file> [<partition> [<doc> [<class>|@geometry]]]
                                  the documents a partition carries (its own,
                                  the loaded families'): classes counted, or
                                  one document's records as JSON lines
  geometry <file> [<id>] [--all|--bounds|--points]
                                  the geometry the file keeps: faces by
                                  surface, seam gaps; one element's bodies
                                  (--points: each face's loops in feet)
  preview <file> <out.png>        the preview image
  write-test <out> [family]       a test project (or family) of the writers";

fn open(path: &str) -> Result<RvtFile, String> {
    RvtFile::open(Path::new(path)).map_err(|e| format!("{path}: {e}"))
}

fn schema_of(file: &RvtFile) -> Result<Schema, String> {
    file.schema().map_err(|e| e.to_string())
}

fn model_of(file: &RvtFile, schema: &Schema) -> Result<Model, String> {
    Model::read(file, schema).map_err(|e| e.to_string())
}

fn cmd_list(path: &str) -> Result<(), String> {
    let f = open(path)?;
    println!(
        "compound file version {}, {:?}",
        f.container.version, f.kind
    );
    for (p, size) in f.streams() {
        println!("{size:>12} {p}");
    }
    Ok(())
}

fn info_json(f: &RvtFile) -> serde_json::Value {
    let schema = f.schema();
    let preview = f.preview();
    json!({
        "kind": f.kind,
        "release": f.release(),
        "release_by_table": f.release_by_table(),
        "build": f.info.build,
        "layout": f.info.layout,
        "fields": f.info.fields.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
        "part_atom": f.part_atom(),
        "external_files": f.external_files(),
        "preview": preview.as_ref().map(|png| json!({
            "bytes": png.len(),
            "size": mitcad_rvt::info::png_size(png),
        })),
        "partitions": f.partition_numbers(),
        "schema": match &schema {
            Ok(s) => json!({
                "classes": s.classes.len(),
                "id_width": s.id_width,
                "bytes": s.size,
                "stopped": s.stopped,
                "unknown_encodings": s.unknown_encodings().len(),
            }),
            Err(e) => json!({ "error": e.to_string() }),
        },
    })
}

fn cmd_info(path: &str, as_json: bool) -> Result<(), String> {
    let f = open(path)?;
    let info = info_json(&f);
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&info).expect("serializes")
        );
        return Ok(());
    }
    println!(
        "{:?} saved by release {} (build {}), BasicFileInfo layout {}",
        f.kind,
        f.release().map_or("?".into(), |r| r.to_string()),
        f.info.build.as_deref().unwrap_or("?"),
        f.info.layout
    );
    if let Some(r) = f.release_by_table() {
        println!("the element table's class tag names release {r}");
    }
    for (k, v) in &f.info.fields {
        println!("  {k}: {v}");
    }
    if let Some(atom) = f.part_atom() {
        println!("family: {:?}, categories {:?}", atom.title, atom.categories);
    }
    for link in f.external_files() {
        println!(
            "external file {:?} {} ({})",
            link.element_id, link.name, link.kind
        );
    }
    println!("preview: {}", info["preview"]);
    println!("partitions: {:?}", f.partition_numbers());
    println!("schema: {}", info["schema"]);
    Ok(())
}

fn hex(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let bytes: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let text: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("{:08x}  {:<48} {text}", i * 16, bytes.join(" "));
    }
}

fn cmd_stream(path: &str, stream: &str, raw: bool) -> Result<(), String> {
    let f = open(path)?;
    if raw {
        use std::io::Write;
        let data = f.raw(stream).map_err(|e| e.to_string())?;
        std::io::stdout()
            .write_all(&data)
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let data = f.inflated(stream).map_err(|e| e.to_string())?;
    hex(&data);
    Ok(())
}

fn cmd_schema(path: &str, class: Option<&str>, as_json: bool) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let classes: Vec<u16> = match class {
        Some(name) => schema
            .lineage(schema.tag(name).ok_or_else(|| format!("no class {name}"))?)
            .into_iter()
            .rev()
            .collect(),
        None => schema.classes.iter().map(|c| c.tag).collect(),
    };
    if as_json {
        let list: Vec<_> = classes.iter().filter_map(|t| schema.class(*t)).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "classes": list,
                "id_width": schema.id_width,
                "unknown_encodings": schema.unknown_encodings(),
                "known_unknowns": KNOWN_UNKNOWNS,
            }))
            .expect("serializes")
        );
        return Ok(());
    }
    for tag in classes {
        let c = schema.class(tag).expect("listed");
        println!(
            "{tag:5} {}{} v{} ({} fields, {} GUIDs)",
            c.name,
            c.base
                .map_or(String::new(), |b| format!(" : {}", schema.class_name(b))),
            c.version,
            c.fields.len(),
            c.guids
        );
        for field in &c.fields {
            let ty = &field.ty;
            let class = ty
                .class
                .map_or(String::new(), |t| format!(" {}", schema.class_name(t)));
            let encoding = match ty.encoding() {
                Ok(e) => format!("{e:?}"),
                Err(e) => format!("UNKNOWN: {e}"),
            };
            println!(
                "        {:<40} {:#08x}{class}{} {encoding}",
                field.name,
                ty.word,
                if ty.stored() { "" } else { " (not stored)" }
            );
        }
    }
    let unknown = schema.unknown_encodings();
    println!(
        "{} classes, ElementId of {} bytes, {} of {} bytes read{}; {} unknown encodings",
        schema.classes.len(),
        schema.id_width,
        schema.parsed,
        schema.size,
        schema
            .stopped
            .as_ref()
            .map_or(String::new(), |s| format!(" (stopped: {s})")),
        unknown.len()
    );
    for u in unknown {
        println!("  unknown: {u}");
    }
    println!("known unknowns:");
    for (what, why) in KNOWN_UNKNOWNS {
        println!("  {what}: {why}");
    }
    Ok(())
}

fn cmd_table(path: &str, as_json: bool) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let table = element_table(&f, &schema).map_err(|e| e.to_string())?;
    if as_json {
        println!("{}", serde_json::to_string(&table).expect("serializes"));
    } else {
        println!("{} declared elements", table.len());
        for d in &table {
            println!("{:>12} owner {}", d.id, d.owner);
        }
    }
    Ok(())
}

fn fmt_box(b: &Option<[f64; 6]>) -> String {
    b.map_or("-".into(), |b| {
        format!(
            "[{:.4} {:.4} {:.4}]..[{:.4} {:.4} {:.4}]",
            b[0], b[1], b[2], b[3], b[4], b[5]
        )
    })
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn cmd_elements(path: &str, format: &str, all: bool) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let model = model_of(&f, &schema)?;
    match format {
        "json" => {
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "release": f.release(),
                    "declared": model.declared.len(),
                    "elements": model.elements,
                    "levels": model.levels,
                    "materials": model.materials,
                    "partitions": model.partitions,
                    "undeclared": model.undeclared,
                    "header_errors": model.header_errors,
                    "object_errors": model.object_errors,
                    "objects_walked": model.objects_walked,
                    "objects_partial": model.objects_partial,
                }))
                .expect("serializes")
            );
        }
        "csv" => {
            println!(
                "id,class,category,level,family,type,name,x,y,z,min_x,min_y,min_z,max_x,max_y,max_z"
            );
            for e in &model.elements {
                let num = |v: Option<f64>| v.map_or(String::new(), |v| v.to_string());
                let t = e.transform;
                let b = e.bbox;
                println!(
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    e.id,
                    csv_field(e.class.as_deref().unwrap_or_default()),
                    e.category.map_or(String::new(), |c| c.to_string()),
                    e.level.map_or(String::new(), |l| l.to_string()),
                    csv_field(e.family_name.as_deref().unwrap_or_default()),
                    csv_field(e.type_name.as_deref().unwrap_or_default()),
                    csv_field(&e.display_name()),
                    num(t.map(|t| t[9])),
                    num(t.map(|t| t[10])),
                    num(t.map(|t| t[11])),
                    num(b.map(|b| b[0])),
                    num(b.map(|b| b[1])),
                    num(b.map(|b| b[2])),
                    num(b.map(|b| b[3])),
                    num(b.map(|b| b[4])),
                    num(b.map(|b| b[5]))
                );
            }
        }
        _ => {
            let levels: std::collections::HashMap<i64, &str> = model
                .levels
                .iter()
                .map(|l| (l.id, l.name.as_str()))
                .collect();
            for e in &model.elements {
                let placed = e.bbox.is_some() && !e.is_type && e.owner_view.is_none();
                if !all && !placed {
                    continue;
                }
                println!(
                    "{:>10} {:<24} {:<26} level {:<20} {} box {}{}{}",
                    e.id,
                    e.class.as_deref().unwrap_or("-"),
                    e.category_name
                        .map(str::to_string)
                        .or(e.category.map(|c| c.to_string()))
                        .unwrap_or_else(|| "-".into()),
                    e.level
                        .map(|l| levels
                            .get(&l)
                            .map_or(l.to_string(), |n| format!("{n} ({l})")))
                        .unwrap_or_else(|| "-".into()),
                    e.display_name(),
                    fmt_box(&e.bbox),
                    e.transform.map_or(String::new(), |t| format!(
                        " at [{:.4} {:.4} {:.4}]",
                        t[9], t[10], t[11]
                    )),
                    e.error
                        .as_ref()
                        .map_or(String::new(), |er| format!(" ERROR {er}"))
                );
            }
            println!(
                "{} declared, {} with records, {} objects walked, {} read in part, {} header and {} object records not walked, {} records of undeclared ids",
                model.declared.len(),
                model
                    .elements
                    .iter()
                    .filter(|e| e.partition.is_some())
                    .count(),
                model.objects_walked,
                model.objects_partial,
                model.header_errors,
                model.object_errors,
                model.undeclared
            );
            for (category, n) in category_counts(&model.elements) {
                println!("  {n:>7} {category}");
            }
            for p in &model.partitions {
                println!(
                    "  partition {}: {} headers, {} objects, {} members{}",
                    p.number,
                    p.headers,
                    p.objects,
                    p.members,
                    p.stopped
                        .as_ref()
                        .map_or(String::new(), |s| format!(", stopped: {s}"))
                );
            }
        }
    }
    Ok(())
}

fn cmd_levels(path: &str) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let model = model_of(&f, &schema)?;
    for l in &model.levels {
        println!(
            "{:>10} {:>12.4} ft {:>12.1} mm  {}{}",
            l.id,
            l.elevation,
            l.elevation * mitcad_rvt::elements::FOOT_MM,
            l.name,
            if l.building_story == Some(false) {
                " (not a storey)"
            } else {
                ""
            }
        );
    }
    Ok(())
}

fn cmd_materials(path: &str) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let model = model_of(&f, &schema)?;
    for m in &model.materials {
        println!(
            "{:>10} {:<40} color {:?} transparency {:?}",
            m.id, m.name, m.color, m.transparency
        );
    }
    Ok(())
}

fn cmd_record(path: &str, id: i64, all: bool) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let partitions = f.partitions(&schema).map_err(|e| e.to_string())?;
    let mut found = Vec::new();
    for p in partitions.iter().rev() {
        let walked = |slots: &[mitcad_rvt::partition::Slot]| {
            slots
                .iter()
                .find(|s| s.id == id)
                .map(|s| match walk::walk(&schema, p.bytes(s)) {
                    Ok(r) => r.to_json(&schema),
                    Err(e) => json!({ "error": e.to_string(), "bytes": s.size }),
                })
        };
        let header = walked(&p.headers);
        let object = walked(&p.objects);
        if header.is_some() || object.is_some() {
            found.push(json!({ "partition": p.number, "header": header, "object": object }));
            if !all {
                break;
            }
        }
    }
    if found.is_empty() {
        return Err(format!("no record of element {id}"));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&found).expect("serializes")
    );
    Ok(())
}

/// Every element whose latest object record is of one of `classes`
/// (comma separated), one JSON line each: id, partition, header, object
/// (read as far as it walks).
fn cmd_records(path: &str, classes: &str) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let wanted: Vec<&str> = classes.split(',').map(str::trim).collect();
    let partitions = f.partitions(&schema).map_err(|e| e.to_string())?;
    let mut latest: std::collections::BTreeMap<i64, usize> = Default::default();
    for (i, p) in partitions.iter().enumerate() {
        for slot in &p.objects {
            latest.insert(slot.id, i);
        }
    }
    for (id, i) in latest {
        let p = &partitions[i];
        let Some(slot) = p.objects.iter().find(|s| s.id == id) else {
            continue;
        };
        let (record, _, error) = walk::walk_partial(&schema, p.bytes(slot));
        let Some(record) = record else { continue };
        if !wanted.contains(&schema.class_name(record.root.class)) {
            continue;
        }
        let header = p
            .headers
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| walk::walk(&schema, p.bytes(s)).ok())
            .map(|r| r.to_json(&schema));
        println!(
            "{}",
            json!({
                "id": id,
                "partition": p.number,
                "header": header,
                "object": record.to_json(&schema),
                "error": error.map(|e| e.to_string()),
            })
        );
    }
    Ok(())
}

/// The building elements' parameters (walls, floors, roofs, instances,
/// types, grids, links) as JSON, with the elements' GlobalIds.
fn cmd_building(path: &str) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let model = model_of(&f, &schema)?;
    let ids: std::collections::BTreeMap<i64, String> = model.global_ids().into_iter().collect();
    println!(
        "{}",
        serde_json::to_string(
            &json!({"building": model.building, "global_ids": ids, "profiles": model.profiles})
        )
        .expect("serializes")
    );
    Ok(())
}

fn cmd_classes(path: &str, wanted: Option<&str>) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let mut counts: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for p in f.partitions(&schema).map_err(|e| e.to_string())? {
        for slot in &p.objects {
            let (record, _, _) = walk::walk_partial(&schema, p.bytes(slot));
            let Some(record) = record else { continue };
            if let Some(w) = wanted
                && (schema.class_name(record.root.class) == w
                    || record
                        .pointees
                        .iter()
                        .any(|o| schema.class_name(o.class) == w))
            {
                println!(
                    "{} {} (partition {})",
                    slot.id,
                    schema.class_name(record.root.class),
                    p.number
                );
            }
            counts
                .entry(schema.class_name(record.root.class).to_string())
                .or_default()
                .0 += 1;
            for o in &record.pointees {
                counts
                    .entry(schema.class_name(o.class).to_string())
                    .or_default()
                    .1 += 1;
            }
        }
    }
    for (class, (elements, objects)) in counts {
        println!("{elements:>8} {objects:>8} {class}");
    }
    Ok(())
}

/// The documents each partition carries after its own chains: their
/// records' classes counted; with a partition and a document number, its
/// records as JSON lines (only those of `class` when given).
fn cmd_documents(
    path: &str,
    partition: Option<u32>,
    doc: Option<usize>,
    class: Option<&str>,
) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let numbers = match partition {
        Some(n) => vec![n],
        None => f.partition_numbers(),
    };
    for n in numbers {
        let docs = f.documents(n, &schema, true).map_err(|e| e.to_string())?;
        if doc.is_none() {
            println!(
                "partition {n}: {} documents{}",
                docs.documents.len(),
                docs.stopped
                    .as_deref()
                    .map_or(String::new(), |s| format!(", stopped: {s}"))
            );
        }
        for (k, d) in docs.documents.iter().enumerate() {
            if doc.is_some_and(|w| w != k) {
                continue;
            }
            let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
            let mut errors = 0;
            let geometry = class == Some("@geometry");
            let class = class.filter(|c| *c != "@geometry");
            let slots = if geometry && doc.is_some() {
                &d.geometry
            } else {
                &d.objects
            };
            for slot in slots {
                let (record, _, error) = walk::walk_partial(&schema, docs.bytes(slot));
                if error.is_some() {
                    errors += 1;
                }
                let Some(record) = record else { continue };
                let name = schema.class_name(record.root.class).to_string();
                if doc.is_some() {
                    if class.is_none_or(|c| {
                        c == name
                            || record
                                .pointees
                                .iter()
                                .any(|o| schema.class_name(o.class) == c)
                    }) {
                        println!(
                            "{}",
                            json!({"id": slot.id, "object": record.to_json(&schema),
                                "error": error.map(|e| e.to_string())})
                        );
                    }
                    continue;
                }
                *counts.entry(name).or_default() += 1;
            }
            if doc.is_none() {
                let ids = d.objects.iter().map(|s| s.id);
                let (lo, hi) = (ids.clone().min(), ids.max());
                let list: Vec<String> = counts
                    .iter()
                    .filter(|(c, _)| {
                        c.contains("Symbol")
                            || c.contains("Form")
                            || c.contains("Family")
                            || c.contains("Extrusion")
                            || c.contains("Sweep")
                            || c.contains("Blend")
                            || c.contains("Revolution")
                    })
                    .map(|(c, n)| format!("{c} {n}"))
                    .collect();
                println!(
                    "  document {k}: {} elements, ids {lo:?} to {hi:?}, {errors} partial, {} geometry records: {}",
                    d.objects.len(),
                    d.geometry.iter().filter(|s| s.size > 2).count(),
                    list.join(", ")
                );
            }
        }
    }
    Ok(())
}

/// The geometry the file keeps (`GElement`s): per element (or the one
/// asked for) its bodies' faces by surface, how far apart the two sides of
/// their edges land (feet), its placed types; `--all` with the B-reps.
fn cmd_geometry(
    path: &str,
    id: Option<i64>,
    all: bool,
    bounds: bool,
    points: bool,
) -> Result<(), String> {
    let f = open(path)?;
    let schema = schema_of(&f)?;
    let model = model_of(&f, &schema)?;
    if bounds {
        for (eid, g) in &model.geometry {
            let b: Vec<[f64; 6]> = g.solids.iter().filter_map(|s| s.bounds()).collect();
            if !b.is_empty() {
                println!("{}", json!({"id": eid, "bounds": b}));
            }
        }
        return Ok(());
    }
    let mut surfaces: std::collections::BTreeMap<String, usize> = Default::default();
    let mut worst = (0.0f64, 0i64);
    let mut gaps: std::collections::BTreeMap<String, f64> = Default::default();
    for (eid, g) in &model.geometry {
        if id.is_some_and(|w| w != *eid) {
            continue;
        }
        for s in &g.solids {
            for face in &s.faces {
                *surfaces
                    .entry(face.surface.class().to_string())
                    .or_default() += 1;
            }
            let gap = s.seam_gap();
            if gap > worst.0 {
                worst = (gap, *eid);
            }
            // By the surfaces an edge joins.
            for e in &s.edges {
                let (Some(a), Some(b)) = (e.faces[0], e.faces[1]) else {
                    continue;
                };
                let (a, b) = (&s.faces[a].surface, &s.faces[b].surface);
                for (p, q) in e.points[0].iter().zip(&e.points[1]) {
                    if let (Some(p), Some(q)) = (a.at(*p), b.at(*q)) {
                        let d = (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>().sqrt();
                        let mut key = [a.class(), b.class()];
                        key.sort_unstable();
                        let w = gaps.entry(key.join("/")).or_insert(0.0);
                        *w = w.max(d);
                    }
                }
            }
        }
        if id.is_some() {
            let summary = json!({
                "id": eid,
                "bbox": g.bbox,
                "solids": g.solids.iter().map(|s| json!({
                    "faces": s.faces.len(), "edges": s.edges.len(), "seam_gap": s.seam_gap(),
                    "bounds": s.bounds(),
                    "surfaces": s.faces.iter().map(|f| f.surface.class()).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "instances": g.instances.iter().map(|p| json!({
                    "symbol": p.symbol, "transform": p.transform, "embedded": p.embedded.is_some(),
                })).collect::<Vec<_>>(),
                "notes": g.notes,
                "curves": g.curves,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).expect("serializes")
            );
            if all {
                println!("{}", serde_json::to_string(&g.solids).expect("serializes"));
            }
            if points {
                // Each face's loops as points (feet), one line per loop.
                for (k, s) in g.solids.iter().enumerate() {
                    for (f, face) in s.faces.iter().enumerate() {
                        for l in 0..face.loops.len() {
                            let pts: Vec<Option<[f64; 3]>> = s
                                .loop_uv(f, l)
                                .into_iter()
                                .map(|uv| face.surface.at(uv))
                                .collect();
                            println!(
                                "{}",
                                json!({"solid": k, "face": f, "loop": l,
                                    "surface": face.surface.class(), "points": pts})
                            );
                        }
                    }
                }
            }
        }
    }
    if id.is_none() {
        println!(
            "{} elements with geometry, {} records not walked",
            model.geometry.len(),
            model.geometry_errors
        );
        let mut notes: std::collections::BTreeMap<&str, usize> = Default::default();
        for n in model.geometry.values().flat_map(|g| &g.notes) {
            *notes.entry(n.as_str()).or_default() += 1;
        }
        println!("notes: {notes:?}");
        println!("faces by surface: {surfaces:?}");
        println!("largest seam gaps (ft) by the surfaces joined: {gaps:?}");
        println!("largest seam gap {:.3e} ft (element {})", worst.0, worst.1);
    }
    Ok(())
}

fn cmd_preview(path: &str, out: &str) -> Result<(), String> {
    let f = open(path)?;
    let png = f.preview().ok_or("the file has no preview image")?;
    std::fs::write(out, &png).map_err(|e| format!("{out}: {e}"))?;
    println!(
        "{} bytes, {:?}",
        png.len(),
        mitcad_rvt::info::png_size(&png)
    );
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let arg = |i: usize| {
        args.get(i)
            .map(String::as_str)
            .ok_or_else(|| USAGE.to_string())
    };
    let flag = |f: &str| args.iter().any(|a| a == f);
    match arg(1)? {
        "list" => cmd_list(arg(2)?),
        "info" => cmd_info(arg(2)?, flag("--json")),
        "stream" => cmd_stream(arg(2)?, arg(3)?, flag("--raw")),
        "schema" => cmd_schema(
            arg(2)?,
            args.get(3)
                .filter(|a| !a.starts_with("--"))
                .map(String::as_str),
            flag("--json"),
        ),
        "table" => cmd_table(arg(2)?, flag("--json")),
        "elements" => cmd_elements(
            arg(2)?,
            if flag("--json") {
                "json"
            } else if flag("--csv") {
                "csv"
            } else {
                "text"
            },
            flag("--all"),
        ),
        "levels" => cmd_levels(arg(2)?),
        "materials" => cmd_materials(arg(2)?),
        "record" => cmd_record(
            arg(2)?,
            arg(3)?
                .parse()
                .map_err(|_| "the element id is a number".to_string())?,
            flag("--all"),
        ),
        "preview" => cmd_preview(arg(2)?, arg(3)?),
        "classes" => cmd_classes(arg(2)?, args.get(3).map(String::as_str)),
        "records" => cmd_records(arg(2)?, arg(3)?),
        "building" => cmd_building(arg(2)?),
        "geometry" => cmd_geometry(
            arg(2)?,
            args.get(3).and_then(|a| a.parse().ok()),
            flag("--all"),
            flag("--bounds"),
            flag("--points"),
        ),
        "documents" => cmd_documents(
            arg(2)?,
            args.get(3).and_then(|a| a.parse().ok()),
            args.get(4).and_then(|a| a.parse().ok()),
            args.get(5).map(String::as_str),
        ),
        "write-test" => {
            let out = arg(2)?;
            let data = if args.get(3).map(String::as_str) == Some("family") {
                mitcad_rvt::testfile::family()
            } else {
                mitcad_rvt::testfile::project()
            };
            std::fs::write(out, data).map_err(|e| format!("{out}: {e}"))
        }
        _ => Err(USAGE.to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
